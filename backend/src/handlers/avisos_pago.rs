//! Módulo "Avisos de Yape y Plin" (AVISOS_PAGO, migración 0071).
//!
//! Para que el que atiende sepa al instante que un Yape llegó de verdad, sin
//! fiarse de capturas. Funciona con la app Monspeet (Android):
//! - En el celular donde llegan los Yape (modo YAPE), la app lee la
//!   notificación de Yape/Plin y la manda aquí (`POST /dispositivo/pagos`).
//! - En el celular de la tienda (modo TIENDA), la app espera con
//!   `GET /dispositivo/pagos/esperar` (long polling) y lo dice en voz alta.
//!
//! Los celulares no inician sesión como un usuario: el administrador los
//! vincula una vez (`POST /avisos-pago/dispositivos`) y cada uno recibe su
//! clave `tienda.dispositivo.secreto`. Aquí se guarda solo la huella SHA-256.
//! Quitar el celular desde Configuración corta su acceso al tiro.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration as StdDuration;

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use chrono::{Duration, NaiveDateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::handlers::rubros::MODULO_AVISOS_PAGO;
use crate::logica::plan::{plan_permite_modulo, MENSAJE_SOLO_COMERCIAL};
use crate::middleware_auth::exigir_solo_admin;
use crate::models::auth::Claims;
use crate::tenants::{NivelAcceso, TenantDb};
use crate::AppState;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn sin_acceso() -> Fallo {
    (StatusCode::UNAUTHORIZED, "Este celular ya no está vinculado. Vincúlalo otra vez desde la app.".into())
}

pub const MODO_YAPE: &str = "YAPE";
pub const MODO_TIENDA: &str = "TIENDA";

/// Cuánto espera como máximo `esperar` antes de responder vacío.
const ESPERA_MAXIMA: StdDuration = StdDuration::from_secs(25);

// ---------------------------------------------------------------------------
// Lectura de la notificación
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Lectura {
    pub billetera: &'static str,
    pub monto: Option<f64>,
    pub nombre: Option<String>,
    pub codigo: Option<String>,
    /// true solo si parece un pago RECIBIDO (con monto). Un "te solicitó un
    /// pago" (estafa conocida) o un pago que hizo el propio dueño no lo es.
    pub reconocido: bool,
}

fn una_linea(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Primer monto después de "S/" ("S/ 15", "S/. 1,250.50", "S/15.").
fn leer_monto(texto: &str) -> Option<(f64, usize)> {
    let mut desde = 0;
    while let Some(p) = texto[desde..].find("S/") {
        let inicio = desde + p;
        let resto = &texto[inicio + 2..];
        let resto_sin = resto.trim_start_matches(|c: char| c == '.' || c.is_whitespace());
        let saltados = resto.len() - resto_sin.len();
        let numero: String = resto_sin.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
        let fin = inicio + 2 + saltados + numero.len();
        let limpio = numero.trim_end_matches(['.', ',']).replace(',', "");
        if let Ok(v) = limpio.parse::<f64>() {
            if v > 0.0 {
                return Some(((v * 100.0).round() / 100.0, fin));
            }
        }
        desde = inicio + 2;
    }
    None
}

fn recortar_nombre(n: &str) -> Option<String> {
    let n = n.trim().trim_start_matches(['¡', '!', ':', '-', ',']).trim();
    let n: String = n.chars().take(60).collect();
    let n = n.trim().to_string();
    (!n.is_empty() && n.chars().any(|c| c.is_alphabetic())).then_some(n)
}

/// "Yape! JUAN P. te envió un pago..." -> "JUAN P."
fn nombre_antes_de(texto: &str) -> Option<String> {
    let bajo = texto.to_lowercase();
    const MARCAS: [&str; 7] =
        [" te envió", " te envio", " te yapeó", " te yapeo", " te ha enviado", " te transfirió", " te transfirio"];
    let pos = MARCAS.iter().filter_map(|m| bajo.find(m)).min()?;
    // `bajo` y `texto` pueden diferir en largo con letras raras: se corta
    // por caracteres, no por bytes.
    let n_chars = bajo[..pos].chars().count();
    let antes: String = texto.chars().take(n_chars).collect();
    let antes = antes.rsplit(['!', '¡', ':']).next().unwrap_or(&antes);
    recortar_nombre(antes)
}

/// "Recibiste S/ 10.00 de JUAN PEREZ por Plin" -> "JUAN PEREZ"
fn nombre_despues_de(texto: &str, desde: usize) -> Option<String> {
    let resto = &texto[desde..];
    let bajo = resto.to_lowercase();
    let p = bajo.find(" de ")?;
    let tras = &resto[p + 4..];
    let tras_bajo = tras.to_lowercase();
    let fin = [" por ", " a través", " mediante", " con ", ". ", ", "]
        .iter()
        .filter_map(|m| tras_bajo.find(m))
        .min()
        .unwrap_or(tras.len());
    recortar_nombre(tras[..fin].trim_end_matches('.'))
}

fn leer_codigo(texto: &str) -> Option<String> {
    let bajo = texto.to_lowercase();
    let p = bajo.find("segur")?;
    let digitos: String = bajo[p..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (2..=6).contains(&digitos.len()).then_some(digitos)
}

pub fn leer(paquete: &str, titulo: &str, texto: &str) -> Lectura {
    let titulo = una_linea(titulo);
    let texto = una_linea(texto);
    let todo = format!("{} {}", titulo, texto);
    let bajo = todo.to_lowercase();
    let billetera = if paquete.to_lowercase().contains("yape") || bajo.contains("yape") {
        "YAPE"
    } else if bajo.contains("plin") {
        "PLIN"
    } else {
        "OTRO"
    };
    let monto_en_texto = leer_monto(&texto);
    let monto = monto_en_texto.map(|m| m.0).or_else(|| leer_monto(&titulo).map(|m| m.0));
    let nombre = nombre_antes_de(&texto)
        .or_else(|| monto_en_texto.and_then(|(_, fin)| nombre_despues_de(&texto, fin)))
        .or_else(|| nombre_antes_de(&titulo));
    let codigo = leer_codigo(&todo);
    const NO_ES_PAGO_RECIBIDO: [&str; 7] =
        ["solicit", "pagaste", "enviaste", "yapeaste", "cobr", "rechaz", "pendiente"];
    const ES_PAGO_RECIBIDO: [&str; 8] =
        ["te envi", "te yape", "recibiste", "te transfiri", "te ha enviado", "abono", "recibido", "te pag"];
    let reconocido = monto.is_some()
        && !NO_ES_PAGO_RECIBIDO.iter().any(|p| bajo.contains(p))
        && ES_PAGO_RECIBIDO.iter().any(|p| bajo.contains(p));
    Lectura { billetera, monto, nombre, codigo, reconocido }
}

// ---------------------------------------------------------------------------
// Claves de los celulares
// ---------------------------------------------------------------------------

fn sha256(t: &str) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, t.as_bytes());
    d.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
}

fn secreto_nuevo() -> String {
    let mut b = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

/// "12.5.ab34…" -> (tienda 12, dispositivo 5, secreto)
fn partes_clave(clave: &str) -> Option<(i64, i64, &str)> {
    let mut it = clave.splitn(3, '.');
    let t = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    let s = it.next().filter(|s| s.len() >= 32)?;
    Some((t, d, s))
}

struct Dispositivo {
    id: i64,
    tienda_id: i64,
    nombre: String,
    modo: String,
    negocio: String,
}

async fn modulo_encendido(conn: &libsql::Connection) -> bool {
    let Ok(mut f) = conn.query("SELECT modulos FROM configuracion_tienda LIMIT 1", ()).await else { return false };
    match f.next().await {
        Ok(Some(fila)) => fila
            .get::<String>(0)
            .map(|m| m.split(',').any(|x| x.trim() == MODULO_AVISOS_PAGO))
            .unwrap_or(false),
        _ => false,
    }
}

/// Cuánto se recuerda el plan de un negocio: la app del celular pregunta cada
/// 25 s y así no se consulta la base central en cada vuelta. Al cambiarle el
/// plan, se nota en a lo más este tiempo.
const PLAN_EN_MEMORIA: StdDuration = StdDuration::from_secs(60);

fn planes_recordados() -> &'static Mutex<HashMap<i64, (bool, std::time::Instant)>> {
    static M: OnceLock<Mutex<HashMap<i64, (bool, std::time::Instant)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// ¿El plan del negocio incluye los avisos de Yape y Plin? Solo desde el
/// plan Comercial (Comercial y Pro). Si el plan no se pudo leer (la base
/// central no respondió un momento) no se corta el servicio.
async fn plan_incluye_avisos(state: &AppState, tienda_id: i64) -> bool {
    if let Some((incluye, cuando)) = planes_recordados().lock().unwrap().get(&tienda_id) {
        if cuando.elapsed() < PLAN_EN_MEMORIA {
            return *incluye;
        }
    }
    match state.tiendas.plan(tienda_id).await {
        Some(plan) => {
            let incluye = plan_permite_modulo(Some(&plan), MODULO_AVISOS_PAGO);
            planes_recordados().lock().unwrap().insert(tienda_id, (incluye, std::time::Instant::now()));
            incluye
        }
        None => true,
    }
}

fn solo_plan_comercial() -> Fallo {
    (StatusCode::FORBIDDEN, MENSAJE_SOLO_COMERCIAL.into())
}

/// Valida la clave del celular (cabecera Authorization: Bearer <clave>).
async fn dispositivo(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(libsql::Connection, Dispositivo), Fallo> {
    let clave = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(sin_acceso)?;
    let (tienda_id, disp_id, _) = partes_clave(clave).ok_or_else(sin_acceso)?;
    let tienda = state.tiendas.resolver_por_id(tienda_id).await.map_err(|_| sin_acceso())?;
    if let NivelAcceso::Bloqueado(m) = tienda.nivel_acceso() {
        return Err((StatusCode::FORBIDDEN, m));
    }
    let db = state.tiendas.conectar_cacheado(&tienda).await.map_err(e500)?;
    let conn = db.connect().map_err(e500)?;
    let mut filas = conn
        .query(
            "SELECT nombre, modo FROM avisos_dispositivos WHERE id = ?1 AND token_hash = ?2 AND activo = 1",
            libsql::params![disp_id, sha256(clave)],
        )
        .await
        .map_err(e500)?;
    let fila = filas.next().await.map_err(e500)?.ok_or_else(sin_acceso)?;
    if !modulo_encendido(&conn).await {
        return Err((StatusCode::FORBIDDEN, "El módulo \"Avisos de Yape y Plin\" está apagado en este negocio.".into()));
    }
    if !plan_incluye_avisos(state, tienda_id).await {
        return Err(solo_plan_comercial());
    }
    let _ = conn
        .execute("UPDATE avisos_dispositivos SET ultimo_uso = datetime('now') WHERE id = ?1", libsql::params![disp_id])
        .await;
    Ok((
        conn,
        Dispositivo {
            id: disp_id,
            tienda_id,
            nombre: fila.get(0).unwrap_or_default(),
            modo: fila.get(1).unwrap_or_default(),
            negocio: tienda.nombre_negocio.clone(),
        },
    ))
}

// ---------------------------------------------------------------------------
// Aviso a los que esperan (long polling), por negocio
// ---------------------------------------------------------------------------

fn avisadores() -> &'static Mutex<HashMap<i64, Arc<tokio::sync::Notify>>> {
    static M: OnceLock<Mutex<HashMap<i64, Arc<tokio::sync::Notify>>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn avisador(tienda_id: i64) -> Arc<tokio::sync::Notify> {
    let mut m = avisadores().lock().unwrap_or_else(|e| e.into_inner());
    m.entry(tienda_id).or_insert_with(|| Arc::new(tokio::sync::Notify::new())).clone()
}

// ---------------------------------------------------------------------------
// Pagos
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug, Clone)]
pub struct Pago {
    pub id: i64,
    pub billetera: String,
    pub monto: Option<f64>,
    pub nombre: Option<String>,
    pub codigo_seguridad: Option<String>,
    pub reconocido: bool,
    pub texto: String,
    /// "AAAA-MM-DD HH:MM:SS" en UTC, como en la base.
    pub recibido_en: String,
    /// "HH:MM" en Perú.
    pub hora: String,
    pub dispositivo: Option<String>,
}

fn hora_lima(utc: &str) -> String {
    NaiveDateTime::parse_from_str(utc, "%Y-%m-%d %H:%M:%S")
        .map(|t| (t - Duration::hours(5)).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Inicio del día de hoy en Perú, en UTC ("AAAA-MM-DD 05:00:00").
fn inicio_de_hoy() -> String {
    format!("{} 05:00:00", (Utc::now() - Duration::hours(5)).format("%Y-%m-%d"))
}

const SQL_PAGO: &str = "SELECT p.id, p.billetera, p.monto, p.nombre, p.codigo_seguridad, p.reconocido, p.texto,
        p.recibido_en, d.nombre
   FROM pagos_recibidos p LEFT JOIN avisos_dispositivos d ON d.id = p.dispositivo_id";

async fn pagos(conn: &libsql::Connection, filtro: &str, params: Vec<libsql::Value>) -> Result<Vec<Pago>, Fallo> {
    let mut filas = conn.query(&format!("{} {}", SQL_PAGO, filtro), params).await.map_err(e500)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let recibido_en: String = f.get(7).unwrap_or_default();
        lista.push(Pago {
            id: f.get(0).unwrap_or_default(),
            billetera: f.get(1).unwrap_or_default(),
            monto: f.get(2).ok(),
            nombre: f.get(3).ok(),
            codigo_seguridad: f.get(4).ok(),
            reconocido: f.get::<i64>(5).unwrap_or(0) == 1,
            texto: f.get(6).unwrap_or_default(),
            hora: hora_lima(&recibido_en),
            recibido_en,
            dispositivo: f.get(8).ok(),
        });
    }
    Ok(lista)
}

async fn pagos_de_hoy(conn: &libsql::Connection) -> Result<Vec<Pago>, Fallo> {
    pagos(conn, "WHERE p.recibido_en >= ?1 ORDER BY p.id DESC LIMIT 300", vec![inicio_de_hoy().into()]).await
}

#[derive(Deserialize)]
pub struct NotificacionEntrante {
    #[serde(default)]
    pub paquete: String,
    #[serde(default)]
    pub titulo: String,
    #[serde(default)]
    pub texto: String,
    /// Momento en que Android publicó la notificación (ms). Sirve para no
    /// guardar dos veces la misma si la app la reenvía.
    #[serde(default)]
    pub publicado_ms: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct RespuestaRecibir {
    pub ok: bool,
    pub nuevo: bool,
    pub pago: Option<Pago>,
}

/// POST /dispositivo/pagos — la app del celular del Yape manda una
/// notificación de pago.
pub async fn recibir(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(n): Json<NotificacionEntrante>,
) -> Result<Json<RespuestaRecibir>, Fallo> {
    let (conn, disp) = dispositivo(&state, &headers).await?;
    if disp.modo != MODO_YAPE {
        return Err((StatusCode::FORBIDDEN, "Este celular está vinculado como celular de la tienda.".into()));
    }
    let paquete: String = n.paquete.trim().chars().take(120).collect();
    let titulo: String = n.titulo.chars().take(300).collect();
    let texto: String = n.texto.chars().take(1000).collect();
    if titulo.trim().is_empty() && texto.trim().is_empty() {
        return Err(malo("La notificación llegó vacía."));
    }
    let lectura = leer(&paquete, &titulo, &texto);
    let completo = una_linea(&format!("{} · {}", titulo, texto));
    // Misma notificación (mismo texto) en la misma ventana de 2 minutos = repetida.
    let ventana = n.publicado_ms.unwrap_or_else(|| Utc::now().timestamp_millis()) / 120_000;
    let huella = sha256(&format!("{}|{}|{}", paquete, completo, ventana));
    let insertados = conn
        .execute(
            "INSERT OR IGNORE INTO pagos_recibidos
               (dispositivo_id, billetera, monto, nombre, codigo_seguridad, reconocido, paquete, texto, huella)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            libsql::params![
                disp.id,
                lectura.billetera,
                lectura.monto,
                lectura.nombre.clone(),
                lectura.codigo.clone(),
                if lectura.reconocido { 1 } else { 0 },
                paquete,
                completo,
                huella.clone()
            ],
        )
        .await
        .map_err(e500)?;
    let pago = pagos(&conn, "WHERE p.huella = ?1", vec![huella.into()]).await?.into_iter().next();
    if insertados > 0 {
        avisador(disp.tienda_id).notify_waiters();
    }
    Ok(Json(RespuestaRecibir { ok: true, nuevo: insertados > 0, pago }))
}

#[derive(Deserialize)]
pub struct FiltroEsperar {
    /// Último id que ya tiene la app. Negativo: solo quiere saber el último
    /// (al abrir la app, para no repetir en voz alta lo de antes).
    #[serde(default)]
    pub despues: i64,
}

#[derive(Serialize)]
pub struct RespuestaEsperar {
    pub ultimo_id: i64,
    pub pagos: Vec<Pago>,
}

async fn ultimo_id(conn: &libsql::Connection) -> Result<i64, Fallo> {
    let mut f = conn.query("SELECT COALESCE(MAX(id), 0) FROM pagos_recibidos", ()).await.map_err(e500)?;
    Ok(f.next().await.map_err(e500)?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(0))
}

/// GET /dispositivo/pagos/esperar?despues=N — responde apenas haya un pago
/// con id > N, o vacío a los 25 s (la app vuelve a llamar).
pub async fn esperar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(f): Query<FiltroEsperar>,
) -> Result<Json<RespuestaEsperar>, Fallo> {
    let (conn, disp) = dispositivo(&state, &headers).await?;
    if f.despues < 0 {
        return Ok(Json(RespuestaEsperar { ultimo_id: ultimo_id(&conn).await?, pagos: vec![] }));
    }
    let aviso = avisador(disp.tienda_id);
    let fin = tokio::time::Instant::now() + ESPERA_MAXIMA;
    loop {
        // Se registra la espera ANTES de consultar: un pago que entre justo
        // entre la consulta y la espera no se pierde.
        let notificado = aviso.notified();
        tokio::pin!(notificado);
        notificado.as_mut().enable();
        let nuevos = pagos(&conn, "WHERE p.id > ?1 ORDER BY p.id ASC LIMIT 50", vec![f.despues.into()]).await?;
        if !nuevos.is_empty() {
            let ultimo = nuevos.last().map(|p| p.id).unwrap_or(f.despues);
            return Ok(Json(RespuestaEsperar { ultimo_id: ultimo, pagos: nuevos }));
        }
        // Con otra instancia del servidor (despliegue blue-green) el aviso
        // en memoria no llega: se vuelve a mirar la base cada 5 s.
        let paso = tokio::time::Instant::now() + StdDuration::from_secs(5);
        if tokio::time::timeout_at(paso.min(fin), notificado).await.is_err() && tokio::time::Instant::now() >= fin {
            return Ok(Json(RespuestaEsperar { ultimo_id: f.despues, pagos: vec![] }));
        }
    }
}

#[derive(Serialize, Debug)]
pub struct DatosDispositivo {
    pub negocio: String,
    pub nombre: String,
    pub modo: String,
}

/// GET /dispositivo/yo — la app comprueba que su clave sigue valiendo.
pub async fn yo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DatosDispositivo>, Fallo> {
    let (_, d) = dispositivo(&state, &headers).await?;
    Ok(Json(DatosDispositivo { negocio: d.negocio, nombre: d.nombre, modo: d.modo }))
}

/// GET /dispositivo/pagos/hoy — la lista de la app.
pub async fn hoy_dispositivo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<Pago>>, Fallo> {
    let (conn, _) = dispositivo(&state, &headers).await?;
    Ok(Json(pagos_de_hoy(&conn).await?))
}

// ---------------------------------------------------------------------------
// Desde Monspeet (usuario con sesión)
// ---------------------------------------------------------------------------

/// GET /avisos-pago/hoy — pagos recibidos hoy (cualquier usuario).
pub async fn hoy(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<Pago>>, Fallo> {
    if !plan_incluye_avisos(&state, claims.tienda_id).await {
        return Err(solo_plan_comercial());
    }
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(pagos_de_hoy(&conn).await?))
}

#[derive(Deserialize)]
pub struct NuevoDispositivo {
    pub nombre: String,
    pub modo: String,
}

#[derive(Serialize)]
pub struct DispositivoCreado {
    pub id: i64,
    pub nombre: String,
    pub modo: String,
    /// Se muestra una sola vez: la app la guarda.
    pub clave: String,
    pub negocio: String,
}

/// POST /avisos-pago/dispositivos — vincula un celular (solo administrador).
pub async fn vincular(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(n): Json<NuevoDispositivo>,
) -> Result<Json<DispositivoCreado>, Fallo> {
    exigir_solo_admin(&claims)?;
    let modo = n.modo.trim().to_uppercase();
    if modo != MODO_YAPE && modo != MODO_TIENDA {
        return Err(malo("El modo debe ser YAPE o TIENDA."));
    }
    let nombre: String = n.nombre.trim().chars().take(60).collect();
    if nombre.is_empty() {
        return Err(malo("Ponle un nombre al celular (por ejemplo \"Celular de caja\")."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    if !modulo_encendido(&conn).await {
        return Err(malo("Primero enciende el módulo \"Avisos de Yape y Plin\" en Configuración → Módulos."));
    }
    if !plan_incluye_avisos(&state, claims.tienda_id).await {
        return Err(solo_plan_comercial());
    }
    let mut activos = conn.query("SELECT COUNT(*) FROM avisos_dispositivos WHERE activo = 1", ()).await.map_err(e500)?;
    let n_activos: i64 = activos.next().await.map_err(e500)?.and_then(|f| f.get(0).ok()).unwrap_or(0);
    if n_activos >= 20 {
        return Err(malo("Ya hay 20 celulares vinculados. Quita alguno antes de agregar otro."));
    }
    // Primero una fila con huella provisional para tener el id (va en la clave).
    let provisional = sha256(&secreto_nuevo());
    conn.execute(
        "INSERT INTO avisos_dispositivos (nombre, modo, token_hash, creado_por) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![nombre.clone(), modo.clone(), provisional.clone(), claims.sub],
    )
    .await
    .map_err(e500)?;
    let id = conn.last_insert_rowid();
    let clave = format!("{}.{}.{}", claims.tienda_id, id, secreto_nuevo());
    conn.execute(
        "UPDATE avisos_dispositivos SET token_hash = ?1 WHERE id = ?2",
        libsql::params![sha256(&clave), id],
    )
    .await
    .map_err(e500)?;
    let negocio = state
        .tiendas
        .resolver_por_id(claims.tienda_id)
        .await
        .map(|t| t.nombre_negocio)
        .unwrap_or_default();
    Ok(Json(DispositivoCreado { id, nombre, modo, clave, negocio }))
}

#[derive(Serialize)]
pub struct DispositivoVinculado {
    pub id: i64,
    pub nombre: String,
    pub modo: String,
    pub creado_en: String,
    pub ultimo_uso: Option<String>,
}

/// GET /avisos-pago/dispositivos — celulares vinculados (solo administrador).
pub async fn dispositivos(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<DispositivoVinculado>>, Fallo> {
    exigir_solo_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query(
            "SELECT id, nombre, modo, creado_en, ultimo_uso FROM avisos_dispositivos WHERE activo = 1 ORDER BY id",
            (),
        )
        .await
        .map_err(e500)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(DispositivoVinculado {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            modo: f.get(2).unwrap_or_default(),
            creado_en: f.get(3).unwrap_or_default(),
            ultimo_uso: f.get(4).ok(),
        });
    }
    Ok(Json(lista))
}

/// DELETE /avisos-pago/dispositivos/:id — quita el celular (su clave deja
/// de valer al tiro).
pub async fn quitar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<StatusCode, Fallo> {
    exigir_solo_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    conn.execute("UPDATE avisos_dispositivos SET activo = 0 WHERE id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn lee_yape() {
        let l = leer(
            "com.bcp.innovacxion.yapeapp",
            "Confirmación de Pago",
            "Yape! JUAN P. te envió un pago por S/ 15. El cód. de seguridad es: 427",
        );
        assert_eq!(l.billetera, "YAPE");
        assert_eq!(l.monto, Some(15.0));
        assert_eq!(l.nombre.as_deref(), Some("JUAN P."));
        assert_eq!(l.codigo.as_deref(), Some("427"));
        assert!(l.reconocido);

        let l = leer("com.bcp.innovacxion.yapeapp", "", "¡Yape! Carlos Quispe M. te yapeó S/ 1,250.50");
        assert_eq!(l.monto, Some(1250.5));
        assert_eq!(l.nombre.as_deref(), Some("Carlos Quispe M."));
        assert!(l.reconocido);
    }

    #[test]
    fn lee_plin() {
        let l = leer("com.bbva.nxt_peru", "Plin", "Recibiste S/ 10.00 de MARIA LOPEZ por Plin.");
        assert_eq!(l.billetera, "PLIN");
        assert_eq!(l.monto, Some(10.0));
        assert_eq!(l.nombre.as_deref(), Some("MARIA LOPEZ"));
        assert!(l.reconocido);

        let l = leer("pe.com.interbank.mobilebanking", "", "ANA RUIZ te envió S/. 7.5 con Plin");
        assert_eq!(l.billetera, "PLIN");
        assert_eq!(l.monto, Some(7.5));
        assert_eq!(l.nombre.as_deref(), Some("ANA RUIZ"));
    }

    #[test]
    fn no_confunde_estafas_ni_pagos_propios() {
        // "Te solicitó un pago": la estafa del cobro.
        assert!(!leer("com.bcp.innovacxion.yapeapp", "", "LUIS R. ha solicitado un pago por S/100").reconocido);
        assert!(!leer("com.bcp.innovacxion.yapeapp", "", "Yapeaste S/ 20 a TIENDA X").reconocido);
        assert!(!leer("com.bcp.innovacxion.yapeapp", "Yape", "¡Activa tus promos de hoy!").reconocido);
        assert_eq!(leer("x", "", "Sin monto, te envió algo").monto, None);
    }

    #[test]
    fn clave() {
        assert_eq!(partes_clave("12.5.0123456789abcdef0123456789abcdef"), Some((12, 5, "0123456789abcdef0123456789abcdef")));
        assert_eq!(partes_clave("12.5.corta"), None);
        assert_eq!(partes_clave("x.5.0123456789abcdef0123456789abcdef"), None);
        assert_eq!(hora_lima("2026-11-19 01:23:45"), "20:23");
    }
}
