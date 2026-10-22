use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Limitador de intentos por IP, en memoria. Dos formas de usarse:
///
/// - "Fallo real" (bloqueado + registrar_fallo): para rutas sensibles
///   (login, registro, identificar). Solo cuenta cuando el backend
///   responde 401 de verdad -- un login exitoso, o volver a entrar
///   después de salir de otro negocio, NUNCA gasta cupo.
/// - "Conteo simple" (permitir): para verificar-usuario, que nunca
///   devuelve 401 (siempre 200, con disponible: true/false). Ahí no
///   tiene sentido contar solo fallos porque nunca habría ninguno --
///   se limita el volumen bruto de consultas en su lugar, con un cupo
///   generoso para no bloquear a nadie mientras escribe su usuario.
pub struct LimitadorIntentos {
    registro: Mutex<HashMap<IpAddr, Vec<Instant>>>,
    max_intentos: usize,
    ventana: Duration,
}

impl LimitadorIntentos {
    fn nuevo(max_intentos: usize, ventana: Duration) -> Arc<Self> {
        Arc::new(Self {
            registro: Mutex::new(HashMap::new()),
            max_intentos,
            ventana,
        })
    }

    /// Preset para login/registro/identificar: 5 FALLOS reales cada 15
    /// minutos. Se usa junto con bloqueado()/registrar_fallo().
    pub fn estricto() -> Arc<Self> {
        Self::nuevo(5, Duration::from_secs(15 * 60))
    }

    /// Preset para verificar-usuario: 30 consultas por minuto, cuenta
    /// TODA petición (no solo fallos, porque esta ruta nunca falla de
    /// esa forma). Se usa junto con permitir().
    pub fn laxo() -> Arc<Self> {
        Self::nuevo(30, Duration::from_secs(60))
    }

    fn contar_vigentes(intentos: &mut Vec<Instant>, ahora: Instant, ventana: Duration) -> usize {
        intentos.retain(|t| ahora.duration_since(*t) < ventana);
        intentos.len()
    }

    /// Cuando el mapa crece mucho (muchas IPs distintas), se borran las IPs
    /// que ya no tienen intentos dentro de la ventana. Antes nunca se
    /// borraba nada y cada IP nueva quedaba en memoria para siempre.
    fn limpiar_si_crece(mapa: &mut HashMap<IpAddr, Vec<Instant>>, ahora: Instant, ventana: Duration) {
        const MAX_IPS: usize = 5_000;
        if mapa.len() > MAX_IPS {
            mapa.retain(|_, intentos| {
                intentos.retain(|t| ahora.duration_since(*t) < ventana);
                !intentos.is_empty()
            });
        }
    }

    /// Solo CONSULTA si la IP está bloqueada en este momento -- no
    /// registra nada, no gasta cupo por preguntar (ni guarda la IP).
    async fn bloqueado(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
        match mapa.get_mut(&ip) {
            Some(intentos) => Self::contar_vigentes(intentos, ahora, self.ventana) >= self.max_intentos,
            None => false,
        }
    }

    /// Registra un intento FALLIDO de verdad para esa IP. Se llama solo
    /// después de ver la respuesta real del handler (401), nunca antes.
    async fn registrar_fallo(&self, ip: IpAddr) {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
        Self::limpiar_si_crece(&mut mapa, ahora, self.ventana);
        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        intentos.retain(|t| ahora.duration_since(*t) < self.ventana);
        intentos.push(ahora);
    }

    /// Devuelve el último cupo gastado por esa IP (un registro que terminó en
    /// error del formulario no debe contar).
    async fn devolver_cupo(&self, ip: IpAddr) {
        let mut mapa = self.registro.lock().await;
        if let Some(intentos) = mapa.get_mut(&ip) {
            intentos.pop();
            if intentos.is_empty() {
                mapa.remove(&ip);
            }
        }
    }

    /// Modo simple: consulta y registra en un solo paso -- toda
    /// petición cuenta, sin importar la respuesta. Para rutas que
    /// nunca fallan pero igual conviene topar el volumen bruto.
    async fn permitir(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
        Self::limpiar_si_crece(&mut mapa, ahora, self.ventana);
        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        let actuales = Self::contar_vigentes(intentos, ahora, self.ventana);
        if actuales >= self.max_intentos {
            false
        } else {
            intentos.push(ahora);
            true
        }
    }
}

/// Saca la IP real del cliente desde X-Forwarded-For (lo pone Nginx). Si
/// no viene esa cabecera (pruebas locales sin Nginx), agrupa todo bajo una
/// IP genérica -- inofensivo en desarrollo.
///
/// Se toma el ÚLTIMO valor, no el primero: con `$proxy_add_x_forwarded_for`
/// Nginx AGREGA al final la IP real de quien se conectó, y todo lo anterior
/// lo puede escribir el propio cliente. Antes se tomaba el primero, así que
/// bastaba mandar una cabecera inventada distinta en cada intento para que
/// el límite de intentos nunca se activara. (Si Nginx reemplaza la cabecera
/// con `$remote_addr`, hay un solo valor y también es el correcto.)
fn extraer_ip(req: &Request) -> IpAddr {
    ip_de_cabecera(req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()))
}

fn ip_de_cabecera(valor: Option<&str>) -> IpAddr {
    valor
        .and_then(|v| v.rsplit(',').map(str::trim).find(|parte| !parte.is_empty()))
        .and_then(|ultima| ultima.parse::<IpAddr>().ok())
        .unwrap_or_else(|| IpAddr::from([0, 0, 0, 0]))
}

/// Middleware para login/registro/identificar: solo cuenta FALLOS
/// reales (401). Entrar bien, aunque sea muchas veces seguidas o
/// cambiando de negocio, nunca consume cupo.
pub async fn limitar_login(
    State(state): State<Arc<crate::AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let ip = extraer_ip(&req);

    if state.limitador_login.bloqueado(ip).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiados intentos fallidos. Espera unos minutos antes de volver a intentar.".into(),
        ));
    }

    let respuesta = next.run(req).await;

    if respuesta.status() == StatusCode::UNAUTHORIZED {
        state.limitador_login.registrar_fallo(ip).await;
    }

    Ok(respuesta)
}

/// Middleware para verificar-usuario: cuenta toda petición (esta ruta
/// nunca devuelve 401, así que contar solo fallos no protegería nada).
pub async fn limitar_verificar(
    State(state): State<Arc<crate::AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let ip = extraer_ip(&req);

    if !state.limitador_verificar.permitir(ip).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiadas consultas seguidas. Espera un momento.".into(),
        ));
    }

    Ok(next.run(req).await)
}
/// Middleware de GET /registro/consultar-ruc: cada consulta puede gastar
/// saldo de Factiliza y la ruta es pública, así que lleva su propio tope,
/// más estricto que verificar-usuario: 15 consultas cada 10 minutos por IP
/// (registrarse bien usa 1 o 2). Vive aquí y no en AppState para no
/// cambiar la estructura que usan las pruebas.
pub async fn limitar_consulta_ruc(req: Request, next: Next) -> Result<Response, (StatusCode, String)> {
    static LIMITADOR: std::sync::OnceLock<Arc<LimitadorIntentos>> = std::sync::OnceLock::new();
    let limitador = LIMITADOR.get_or_init(|| LimitadorIntentos::nuevo(15, Duration::from_secs(10 * 60)));

    if !limitador.permitir(extraer_ip(&req)).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiadas consultas de RUC seguidas. Espera unos minutos.".into(),
        ));
    }
    Ok(next.run(req).await)
}

/// Límite de negocios nuevos (POST /registro). Cada registro crea una base
/// en Turso y da días de prueba gratis, y el limitador de login no lo frenaba
/// (el registro nunca responde 401). Dos topes, en 24 horas:
///   - por IP: REGISTRO_MAX_POR_IP en el .env (3 por defecto);
///   - en total, sin importar la IP: REGISTRO_MAX_POR_DIA (20 por defecto),
///     para que cambiar de IP tampoco sirva.
/// El cupo se toma al entrar (dos registros a la vez no lo saltan) y se
/// devuelve si el registro termina en un error del formulario (4xx: usuario
/// repetido, datos incompletos), que no crea nada.
pub struct LimitadorRegistro {
    por_ip: Arc<LimitadorIntentos>,
    total: Arc<LimitadorIntentos>,
}

/// "IP" con la que se cuenta el total de registros (no es una IP real).
const CLAVE_TOTAL: IpAddr = IpAddr::V4(std::net::Ipv4Addr::BROADCAST);
const DIA: Duration = Duration::from_secs(24 * 60 * 60);

fn tope_del_env(nombre: &str, por_defecto: usize) -> usize {
    std::env::var(nombre)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(por_defecto)
}

impl LimitadorRegistro {
    pub fn nuevo() -> Arc<Self> {
        Self::con_topes(tope_del_env("REGISTRO_MAX_POR_IP", 3), tope_del_env("REGISTRO_MAX_POR_DIA", 20))
    }

    pub fn con_topes(max_por_ip: usize, max_total: usize) -> Arc<Self> {
        Arc::new(Self {
            por_ip: LimitadorIntentos::nuevo(max_por_ip, DIA),
            total: LimitadorIntentos::nuevo(max_total, DIA),
        })
    }

    /// Toma un cupo. Err con el mensaje si no hay.
    async fn tomar(&self, ip: IpAddr) -> Result<(), &'static str> {
        if !self.por_ip.permitir(ip).await {
            return Err("Ya se crearon varios negocios desde esta conexión hoy. Si necesitas otro, escríbenos por WhatsApp.");
        }
        if !self.total.permitir(CLAVE_TOTAL).await {
            self.por_ip.devolver_cupo(ip).await;
            return Err("Hoy no se pueden crear más negocios nuevos. Inténtalo mañana o escríbenos por WhatsApp.");
        }
        Ok(())
    }

    async fn devolver(&self, ip: IpAddr) {
        self.por_ip.devolver_cupo(ip).await;
        self.total.devolver_cupo(CLAVE_TOTAL).await;
    }
}

/// Middleware de POST /registro (ver LimitadorRegistro).
pub async fn limitar_registro(
    State(state): State<Arc<crate::AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let ip = extraer_ip(&req);
    state
        .limitador_registro
        .tomar(ip)
        .await
        .map_err(|mensaje| (StatusCode::TOO_MANY_REQUESTS, mensaje.to_string()))?;

    let respuesta = next.run(req).await;

    if respuesta.status().is_client_error() {
        state.limitador_registro.devolver(ip).await;
    }
    Ok(respuesta)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_ip_es_la_que_agrega_nginx() {
        // El cliente inventa "1.2.3.4"; Nginx agrega al final la real.
        assert_eq!(ip_de_cabecera(Some("1.2.3.4, 190.42.10.7")), "190.42.10.7".parse::<IpAddr>().unwrap());
        assert_eq!(ip_de_cabecera(Some("190.42.10.7")), "190.42.10.7".parse::<IpAddr>().unwrap());
        assert_eq!(ip_de_cabecera(Some(" 9.9.9.9 ,  8.8.8.8 ")), "8.8.8.8".parse::<IpAddr>().unwrap());
        assert_eq!(ip_de_cabecera(Some("2001:db8::1")), "2001:db8::1".parse::<IpAddr>().unwrap());
        assert_eq!(ip_de_cabecera(None), IpAddr::from([0, 0, 0, 0]));
        assert_eq!(ip_de_cabecera(Some("basura")), IpAddr::from([0, 0, 0, 0]));
    }

    #[tokio::test]
    async fn login_bloquea_a_los_cinco_fallos_y_no_guarda_ips_que_solo_preguntan() {
        let l = LimitadorIntentos::estricto();
        let ip: IpAddr = "190.42.10.7".parse().unwrap();
        for _ in 0..4 {
            l.registrar_fallo(ip).await;
            assert!(!l.bloqueado(ip).await);
        }
        l.registrar_fallo(ip).await;
        assert!(l.bloqueado(ip).await);
        // Otra IP no queda afectada, y preguntar no la guarda en memoria.
        let otra: IpAddr = "190.42.10.8".parse().unwrap();
        assert!(!l.bloqueado(otra).await);
        assert_eq!(l.registro.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn el_mapa_no_crece_sin_limite() {
        let l = LimitadorIntentos::nuevo(5, Duration::from_millis(1));
        for i in 0..6_000u32 {
            l.registrar_fallo(IpAddr::from(i.to_be_bytes())).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        l.registrar_fallo(IpAddr::from([10, 0, 0, 1])).await;
        assert!(l.registro.lock().await.len() <= 5_001);
    }

    #[tokio::test]
    async fn registro_por_ip_y_total() {
        let r = LimitadorRegistro::con_topes(2, 3);
        let a: IpAddr = "1.1.1.1".parse().unwrap();
        let b: IpAddr = "2.2.2.2".parse().unwrap();
        assert!(r.tomar(a).await.is_ok());
        assert!(r.tomar(a).await.is_ok());
        assert!(r.tomar(a).await.is_err(), "tope por IP");
        // Un error del formulario devuelve el cupo.
        r.devolver(a).await;
        assert!(r.tomar(a).await.is_ok());
        // Tope total: ya hay 2 de A; uno de B llega a 3 y el siguiente no entra.
        assert!(r.tomar(b).await.is_ok());
        let c: IpAddr = "3.3.3.3".parse().unwrap();
        assert!(r.tomar(c).await.is_err(), "tope total");
        // Al rechazarse por el total, C no gastó su cupo por IP.
        assert!(r.por_ip.registro.lock().await.get(&c).is_none());
    }
}
