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

    /// Solo CONSULTA si la IP está bloqueada en este momento -- no
    /// registra nada, no gasta cupo por preguntar.
    async fn bloqueado(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        Self::contar_vigentes(intentos, ahora, self.ventana) >= self.max_intentos
    }

    /// Registra un intento FALLIDO de verdad para esa IP. Se llama solo
    /// después de ver la respuesta real del handler (401), nunca antes.
    async fn registrar_fallo(&self, ip: IpAddr) {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        intentos.retain(|t| ahora.duration_since(*t) < self.ventana);
        intentos.push(ahora);
    }

    /// Modo simple: consulta y registra en un solo paso -- toda
    /// petición cuenta, sin importar la respuesta. Para rutas que
    /// nunca fallan pero igual conviene topar el volumen bruto.
    async fn permitir(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();
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

/// Saca la IP real del cliente desde X-Forwarded-For (configurado en
/// Nginx). Si no viene esa cabecera (pruebas locales sin Nginx),
/// agrupa todo bajo una IP genérica -- inofensivo en desarrollo, y en
/// producción esa cabecera siempre está presente.
fn extraer_ip(req: &Request) -> IpAddr {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|primera| primera.trim().parse::<IpAddr>().ok())
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