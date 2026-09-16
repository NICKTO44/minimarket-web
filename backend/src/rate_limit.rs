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

const MAX_INTENTOS: usize = 5;
const VENTANA: Duration = Duration::from_secs(15 * 60); // 15 minutos

/// Limitador de intentos FALLIDOS por IP, en memoria. Solo cuenta
/// cuando el login/registro/identificación realmente falla (401) --
/// un login exitoso nunca gasta cupo, sin importar cuántas veces entres
/// bien seguidas o cambies de negocio.
pub struct LimitadorIntentos {
    registro: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl LimitadorIntentos {
    pub fn nuevo() -> Arc<Self> {
        Arc::new(Self {
            registro: Mutex::new(HashMap::new()),
        })
    }

    /// Solo CONSULTA si la IP está bloqueada en este momento -- no
    /// registra nada, no gasta cupo por preguntar.
    async fn bloqueado(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();

        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        intentos.retain(|t| ahora.duration_since(*t) < VENTANA);

        intentos.len() >= MAX_INTENTOS
    }

    /// Registra un intento FALLIDO de verdad (401) para esa IP. Se
    /// llama solo después de ver la respuesta real del handler, nunca
    /// antes.
    async fn registrar_fallo(&self, ip: IpAddr) {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();

        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        intentos.retain(|t| ahora.duration_since(*t) < VENTANA);
        intentos.push(ahora);
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

pub async fn limitar_intentos(
    State(state): State<Arc<crate::AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let ip = extraer_ip(&req);

    // Antes de dejar pasar la petición: ¿ya está bloqueada esta IP por
    // fallos anteriores? Esto es solo lectura, no cuenta como intento.
    if state.limitador_login.bloqueado(ip).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiados intentos fallidos. Espera unos minutos antes de volver a intentar.".into(),
        ));
    }

    let respuesta = next.run(req).await;

    // Recién AQUÍ, viendo la respuesta real del handler: si fue 401
    // (credenciales incorrectas, usuario no encontrado), se cuenta como
    // fallo de verdad. Cualquier otra respuesta (200 login exitoso, 400
    // datos incompletos, etc.) no gasta cupo.
    if respuesta.status() == StatusCode::UNAUTHORIZED {
        state.limitador_login.registrar_fallo(ip).await;
    }

    Ok(respuesta)
}