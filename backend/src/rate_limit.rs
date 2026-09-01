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

/// Limitador de intentos por IP, en memoria. Pensado solo para los
/// endpoints públicos sin sesión (/login, /registro,
/// /registro/verificar-usuario) — el resto de la API ya está protegida
/// por el JWT y no lo necesita.
pub struct LimitadorIntentos {
    registro: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl LimitadorIntentos {
    pub fn nuevo() -> Arc<Self> {
        Arc::new(Self {
            registro: Mutex::new(HashMap::new()),
        })
    }

    /// Devuelve true si la IP todavía tiene intentos disponibles en la
    /// ventana actual (y registra este intento); false si ya se pasó.
    async fn permitir(&self, ip: IpAddr) -> bool {
        let mut mapa = self.registro.lock().await;
        let ahora = Instant::now();

        let intentos = mapa.entry(ip).or_insert_with(Vec::new);
        // Descarta los intentos que ya salieron de la ventana de 15 min.
        intentos.retain(|t| ahora.duration_since(*t) < VENTANA);

        if intentos.len() >= MAX_INTENTOS {
            false
        } else {
            intentos.push(ahora);
            true
        }
    }
}

/// Saca la IP real del cliente desde la cabecera X-Forwarded-For que ya
/// configuramos en Nginx (toma la primera de la lista, que es la del
/// visitante original). Si no viene esa cabecera (por ejemplo, pruebas en
/// local sin pasar por Nginx), agrupa todo bajo una IP genérica — inofensivo
/// en desarrollo, y en producción esa cabecera siempre va a estar presente.
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

    if !state.limitador_login.permitir(ip).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Demasiados intentos. Espera unos minutos antes de volver a intentar.".into(),
        ));
    }

    Ok(next.run(req).await)
}