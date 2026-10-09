//! Panel de Monspeet: la pantalla del dueño del sistema (no de un negocio).
//!
//! Desde aquí se ven todos los negocios, se manejan sus suscripciones
//! (renovar, restringir, suspender, códigos de activación) y su facturación
//! electrónica (datos del emisor, certificado, usuario SOL, modo de emisión).
//!
//! Se entra con un usuario y clave propios que viven solo en el .env:
//!   PANEL_USUARIO=...
//!   PANEL_CLAVE=...      (mínimo 10 caracteres)
//! Si faltan, el panel no existe en ese servidor (responde 404). La sesión
//! es un JWT distinto al de los negocios: un token de negocio no sirve aquí
//! ni uno del panel sirve en las rutas de un negocio.
//!
//! Las suscripciones usan exactamente la misma lógica que el comando
//! `licencias` y que el canje de códigos (licencias_logica).

use axum::{
    extract::{Multipart, Path, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::facturacion_directa::{leer_emisor, servidor_lycet};
use crate::licencias_logica::{calcular_nueva_fecha, generar_codigo, hoy, normalizar_unidad, parsear_fecha};
use crate::logica::alta_sunat::{self, AltaHecha, Chequeo, CAMPOS};
use crate::logica::certificado;
use crate::tenants::{NivelAcceso, TiendaConexion};
use crate::AppState;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}

// ---------------------------------------------------------------------
// Acceso
// ---------------------------------------------------------------------

/// Duración de la sesión del panel.
const HORAS_SESION: i64 = 8;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PanelClaims {
    /// Siempre true: es lo que distingue este token del de un negocio.
    pub panel: bool,
    pub usuario: String,
    pub exp: usize,
}

/// Usuario y clave del panel, del .env. None = panel apagado.
fn credenciales() -> Option<(String, String)> {
    let usuario = std::env::var("PANEL_USUARIO").ok()?.trim().to_string();
    let clave = std::env::var("PANEL_CLAVE").ok()?.trim().to_string();
    (!usuario.is_empty() && !clave.is_empty()).then_some((usuario, clave))
}

/// Comparación que tarda lo mismo acierte o no (no deja adivinar la clave
/// letra por letra midiendo tiempos).
fn iguales(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diferencia = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diferencia |= (*a.get(i).unwrap_or(&0) ^ *b.get(i).unwrap_or(&0)) as usize;
    }
    diferencia == 0
}

const PANEL_APAGADO: &str = "El panel no está activado en este servidor (faltan PANEL_USUARIO y PANEL_CLAVE en el .env).";

#[derive(Deserialize)]
pub struct LoginPanel {
    pub usuario: String,
    pub clave: String,
}

#[derive(Serialize)]
pub struct SesionPanel {
    pub token: String,
    pub usuario: String,
}

pub async fn login(State(state): State<Arc<AppState>>, Json(datos): Json<LoginPanel>) -> Result<Json<SesionPanel>, Fallo> {
    let (usuario, clave) = credenciales().ok_or((StatusCode::NOT_FOUND, PANEL_APAGADO.to_string()))?;
    if clave.chars().count() < 10 {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "PANEL_CLAVE es muy corta: usa al menos 10 caracteres en el .env.".to_string(),
        ));
    }
    let usuario_ok = iguales(datos.usuario.trim(), &usuario);
    let clave_ok = iguales(datos.clave.trim(), &clave);
    if !(usuario_ok && clave_ok) {
        return Err((StatusCode::UNAUTHORIZED, "Usuario o clave del panel incorrectos.".to_string()));
    }
    let exp = (chrono::Utc::now() + chrono::Duration::hours(HORAS_SESION)).timestamp() as usize;
    let claims = PanelClaims { panel: true, usuario: usuario.clone(), exp };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.jwt_secret.as_bytes())).map_err(interno)?;
    Ok(Json(SesionPanel { token, usuario }))
}

/// Middleware de las rutas /panel/*: exige la sesión del panel.
pub async fn requiere_panel(State(state): State<Arc<AppState>>, mut req: Request, next: Next) -> Result<Response, StatusCode> {
    if credenciales().is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let datos = decode::<PanelClaims>(token, &DecodingKey::from_secret(state.jwt_secret.as_bytes()), &Validation::default())
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if !datos.claims.panel {
        return Err(StatusCode::UNAUTHORIZED);
    }
    req.extensions_mut().insert(datos.claims);
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------
// Negocios
// ---------------------------------------------------------------------

#[derive(Serialize, Default)]
pub struct FacturacionResumen {
    /// "DIRECTO", "FACTURALIBRE" o "SIN_CONFIGURAR".
    pub modo: String,
    pub ruc: Option<String>,
    pub ambiente: Option<String>,
    pub cert_vence: Option<String>,
    /// Si no se pudo leer la base del negocio.
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct NegocioPanel {
    pub id: i64,
    pub nombre: String,
    pub identificador: String,
    pub estado: String,
    pub fecha_vencimiento: Option<String>,
    pub fecha_creacion: Option<String>,
    /// Negativo si ya venció. None = sin vencimiento.
    pub dias_restantes: Option<i64>,
    /// "COMPLETO", "LECTURA" o "BLOQUEADO": lo que el negocio puede hacer hoy.
    pub acceso: String,
    pub motivo: Option<String>,
    pub usuarios: Vec<String>,
    pub facturacion: FacturacionResumen,
}

fn acceso_de(tienda: &TiendaConexion) -> (String, Option<String>) {
    match tienda.nivel_acceso() {
        NivelAcceso::Completo => ("COMPLETO".to_string(), None),
        NivelAcceso::SoloLectura(m) => ("LECTURA".to_string(), Some(m)),
        NivelAcceso::Bloqueado(m) => ("BLOQUEADO".to_string(), Some(m)),
    }
}

fn dias_restantes(fecha: Option<&str>) -> Option<i64> {
    fecha.and_then(parsear_fecha).map(|f| (f - hoy()).num_days())
}

async fn texto(conn: &libsql::Connection, consulta: &str) -> Option<Vec<Option<String>>> {
    let mut filas = conn.query(consulta, ()).await.ok()?;
    let fila = filas.next().await.ok()??;
    Some((0..fila.column_count()).map(|i| fila.get::<String>(i).ok()).collect())
}

fn no_vacio(v: Option<&Option<String>>) -> Option<String> {
    v.cloned().flatten().filter(|s| !s.trim().is_empty())
}

async fn resumen_facturacion(state: &AppState, tienda: &TiendaConexion) -> FacturacionResumen {
    let conn = match state.tiendas.conectar_cacheado(tienda).await.and_then(|db| db.connect().map_err(|e| e.to_string())) {
        Ok(c) => c,
        Err(e) => return FacturacionResumen { modo: "SIN_CONFIGURAR".into(), error: Some(e), ..Default::default() },
    };
    let Some(base) = texto(&conn, "SELECT facturacion_proveedor, facturalibre_token, facturalibre_ruta, ruc FROM configuracion_tienda LIMIT 1").await
    else {
        return FacturacionResumen { modo: "SIN_CONFIGURAR".into(), error: Some("No se pudo leer su configuración.".into()), ..Default::default() };
    };
    let modo = if base[0].as_deref() == Some("SUNAT_DIRECTO") {
        "DIRECTO"
    } else if no_vacio(base.get(1)).is_some() && no_vacio(base.get(2)).is_some() {
        "FACTURALIBRE"
    } else {
        "SIN_CONFIGURAR"
    };
    let alta = texto(&conn, "SELECT sunat_ambiente, sunat_cert_vence FROM configuracion_tienda LIMIT 1").await.unwrap_or_default();
    FacturacionResumen {
        modo: modo.to_string(),
        ruc: no_vacio(base.get(3)),
        ambiente: no_vacio(alta.first()),
        cert_vence: no_vacio(alta.get(1)),
        error: None,
    }
}

pub async fn listar_negocios(State(state): State<Arc<AppState>>) -> Result<Json<Vec<NegocioPanel>>, Fallo> {
    let tiendas = state.tiendas.listar_todas().await.map_err(interno)?;
    let central = state.tiendas.conexion_central().map_err(interno)?;

    let mut creacion: HashMap<i64, String> = HashMap::new();
    let mut filas = central.query("SELECT id, fecha_creacion FROM tiendas", ()).await.map_err(interno)?;
    while let Some(f) = filas.next().await.map_err(interno)? {
        if let (Ok(id), Ok(fecha)) = (f.get::<i64>(0), f.get::<String>(1)) {
            creacion.insert(id, fecha);
        }
    }
    let mut usuarios: HashMap<i64, Vec<String>> = HashMap::new();
    let mut filas = central.query("SELECT tienda_id, usuario FROM usuarios_indice ORDER BY usuario", ()).await.map_err(interno)?;
    while let Some(f) = filas.next().await.map_err(interno)? {
        if let (Ok(id), Ok(u)) = (f.get::<i64>(0), f.get::<String>(1)) {
            usuarios.entry(id).or_default().push(u);
        }
    }

    // La facturación se lee de la base de cada negocio, todas a la vez.
    let resumenes = futures_util::future::join_all(tiendas.iter().map(|t| resumen_facturacion(&state, t))).await;

    let negocios = tiendas
        .iter()
        .zip(resumenes)
        .map(|(t, facturacion)| {
            let (acceso, motivo) = acceso_de(t);
            NegocioPanel {
                id: t.id,
                nombre: t.nombre_negocio.clone(),
                identificador: t.identificador.clone(),
                estado: t.estado.clone(),
                fecha_vencimiento: t.fecha_vencimiento.clone(),
                fecha_creacion: creacion.get(&t.id).cloned(),
                dias_restantes: dias_restantes(t.fecha_vencimiento.as_deref()),
                acceso,
                motivo,
                usuarios: usuarios.remove(&t.id).unwrap_or_default(),
                facturacion,
            }
        })
        .collect();
    Ok(Json(negocios))
}

// ---------------------------------------------------------------------
// Suscripción (misma lógica que `licencias`)
// ---------------------------------------------------------------------

#[derive(Serialize)]
pub struct SuscripcionCambiada {
    pub estado: String,
    pub fecha_vencimiento: Option<String>,
    pub mensaje: String,
}

async fn datos_suscripcion(state: &AppState, id: i64) -> Result<(String, Option<String>, String), Fallo> {
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    let mut filas = conn
        .query("SELECT estado, fecha_vencimiento, nombre_negocio FROM tiendas WHERE id = ?1", libsql::params![id])
        .await
        .map_err(interno)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Negocio no encontrado.".to_string()))?;
    Ok((f.get(0).unwrap_or_else(|_| "ACTIVO".into()), f.get(1).unwrap_or(None), f.get(2).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct Renovar {
    pub cantidad: i64,
    pub unidad: String,
}

/// Suma tiempo a la suscripción y la deja ACTIVA (igual que `licencias activar`
/// y que canjear un código): si aún no venció, suma desde su fecha actual.
pub async fn renovar(State(state): State<Arc<AppState>>, Path(id): Path<i64>, Json(datos): Json<Renovar>) -> Result<Json<SuscripcionCambiada>, Fallo> {
    if !(1..=120).contains(&datos.cantidad) {
        return Err(malo("La cantidad debe estar entre 1 y 120."));
    }
    let unidad = normalizar_unidad(&datos.unidad).ok_or_else(|| malo("Unidad no válida: usa días, meses o años."))?;
    let (_, actual, nombre) = datos_suscripcion(&state, id).await?;
    let nueva = calcular_nueva_fecha(actual.as_deref(), datos.cantidad, &unidad.to_lowercase()).map_err(malo)?;
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    conn.execute(
        "UPDATE tiendas SET estado = 'ACTIVO', fecha_vencimiento = ?1 WHERE id = ?2",
        libsql::params![nueva.clone(), id],
    )
    .await
    .map_err(interno)?;
    state.tiendas.invalidar_metadata(id).await;
    Ok(Json(SuscripcionCambiada {
        estado: "ACTIVO".into(),
        mensaje: format!("{} activo hasta el {}.", nombre, nueva),
        fecha_vencimiento: Some(nueva),
    }))
}

#[derive(Deserialize)]
pub struct CambiarEstado {
    pub estado: String,
}

/// ACTIVO, RESTRINGIDO (modo lectura) o SUSPENDIDO (sin acceso), igual que
/// `licencias reactivar / restringir / suspender`. No toca la fecha.
pub async fn cambiar_estado(State(state): State<Arc<AppState>>, Path(id): Path<i64>, Json(datos): Json<CambiarEstado>) -> Result<Json<SuscripcionCambiada>, Fallo> {
    let estado = datos.estado.trim().to_uppercase();
    if !matches!(estado.as_str(), "ACTIVO" | "RESTRINGIDO" | "SUSPENDIDO") {
        return Err(malo("Estado no válido."));
    }
    let (_, fecha, nombre) = datos_suscripcion(&state, id).await?;
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    conn.execute("UPDATE tiendas SET estado = ?1 WHERE id = ?2", libsql::params![estado.clone(), id])
        .await
        .map_err(interno)?;
    state.tiendas.invalidar_metadata(id).await;
    let mensaje = match estado.as_str() {
        "ACTIVO" => format!("{} reactivado.", nombre),
        "RESTRINGIDO" => format!("{} en modo lectura: ve su información pero no vende.", nombre),
        _ => format!("{} suspendido: ya no puede entrar.", nombre),
    };
    Ok(Json(SuscripcionCambiada { estado, fecha_vencimiento: fecha, mensaje }))
}

#[derive(Deserialize)]
pub struct FijarVencimiento {
    /// "AAAA-MM-DD", o null para dejarlo sin vencimiento.
    pub fecha: Option<String>,
}

/// Corrige la fecha de vencimiento a mano (por ejemplo, si se renovó dos
/// veces por error). No cambia el estado.
pub async fn fijar_vencimiento(State(state): State<Arc<AppState>>, Path(id): Path<i64>, Json(datos): Json<FijarVencimiento>) -> Result<Json<SuscripcionCambiada>, Fallo> {
    let fecha = match datos.fecha.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => Some(parsear_fecha(f).ok_or_else(|| malo("Fecha no válida (AAAA-MM-DD)."))?.format("%Y-%m-%d").to_string()),
        None => None,
    };
    let (estado, _, nombre) = datos_suscripcion(&state, id).await?;
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    conn.execute("UPDATE tiendas SET fecha_vencimiento = ?1 WHERE id = ?2", libsql::params![fecha.clone(), id])
        .await
        .map_err(interno)?;
    state.tiendas.invalidar_metadata(id).await;
    let mensaje = match &fecha {
        Some(f) => format!("{} vence el {}.", nombre, f),
        None => format!("{} queda sin fecha de vencimiento.", nombre),
    };
    Ok(Json(SuscripcionCambiada { estado, fecha_vencimiento: fecha, mensaje }))
}

// ---------------------------------------------------------------------
// Códigos de activación (los que el negocio canjea en Suscripción)
// ---------------------------------------------------------------------

#[derive(Serialize)]
pub struct CodigoPanel {
    pub codigo: String,
    pub cantidad: i64,
    pub unidad: String,
    pub usado: bool,
    pub negocio: Option<String>,
    pub fecha_creacion: Option<String>,
    pub fecha_uso: Option<String>,
    pub caduca: Option<String>,
    /// Sin usar y todavía canjeable.
    pub vigente: bool,
}

pub async fn listar_codigos(State(state): State<Arc<AppState>>) -> Result<Json<Vec<CodigoPanel>>, Fallo> {
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    let mut filas = conn
        .query(
            "SELECT c.codigo, c.duracion_cantidad, c.duracion_unidad, c.usado, t.nombre_negocio,
                    c.fecha_creacion, c.fecha_uso, c.fecha_expira_si_no_se_usa
             FROM codigos_activacion c LEFT JOIN tiendas t ON t.id = c.usado_por_tienda_id
             ORDER BY c.id DESC LIMIT 50",
            (),
        )
        .await
        .map_err(interno)?;
    let hoy_txt = hoy().format("%Y-%m-%d").to_string();
    let mut codigos = Vec::new();
    while let Some(f) = filas.next().await.map_err(interno)? {
        let usado = f.get::<i64>(3).unwrap_or(0) == 1;
        let caduca: Option<String> = f.get(7).unwrap_or(None);
        codigos.push(CodigoPanel {
            codigo: f.get(0).unwrap_or_default(),
            cantidad: f.get(1).unwrap_or(0),
            unidad: f.get(2).unwrap_or_default(),
            usado,
            negocio: f.get(4).unwrap_or(None),
            fecha_creacion: f.get(5).unwrap_or(None),
            fecha_uso: f.get(6).unwrap_or(None),
            vigente: !usado && caduca.as_deref().map_or(true, |c| c >= hoy_txt.as_str()),
            caduca,
        });
    }
    Ok(Json(codigos))
}

#[derive(Deserialize)]
pub struct NuevoCodigo {
    pub cantidad: i64,
    pub unidad: String,
    pub dias_para_caducar: Option<i64>,
}

/// Igual que `licencias generar-codigo`.
pub async fn generar_codigo_panel(State(state): State<Arc<AppState>>, Json(datos): Json<NuevoCodigo>) -> Result<Json<CodigoPanel>, Fallo> {
    if !(1..=120).contains(&datos.cantidad) {
        return Err(malo("La cantidad debe estar entre 1 y 120."));
    }
    let unidad = normalizar_unidad(&datos.unidad).ok_or_else(|| malo("Unidad no válida: usa días, meses o años."))?;
    let dias = datos.dias_para_caducar.unwrap_or(30).clamp(1, 365);
    let caduca = (hoy() + chrono::Duration::days(dias)).format("%Y-%m-%d").to_string();
    let codigo = generar_codigo();
    let conn = state.tiendas.conexion_central().map_err(interno)?;
    conn.execute(
        "INSERT INTO codigos_activacion (codigo, duracion_cantidad, duracion_unidad, fecha_expira_si_no_se_usa)
         VALUES (?1, ?2, ?3, ?4)",
        libsql::params![codigo.clone(), datos.cantidad, unidad, caduca.clone()],
    )
    .await
    .map_err(interno)?;
    Ok(Json(CodigoPanel {
        codigo,
        cantidad: datos.cantidad,
        unidad: unidad.to_string(),
        usado: false,
        negocio: None,
        fecha_creacion: Some(crate::logica::tiempo::ahora_lima()),
        fecha_uso: None,
        caduca: Some(caduca),
        vigente: true,
    }))
}

// ---------------------------------------------------------------------
// Facturación de un negocio
// ---------------------------------------------------------------------

async fn conexion_negocio(state: &AppState, id: i64) -> Result<libsql::Connection, Fallo> {
    let tienda = state.tiendas.resolver_por_id(id).await.map_err(|e| (StatusCode::NOT_FOUND, e))?;
    let db = state.tiendas.conectar_cacheado(&tienda).await.map_err(interno)?;
    db.connect().map_err(interno)
}

#[derive(Serialize, Default)]
pub struct AltaGuardada {
    pub usuario_sol: Option<String>,
    pub ambiente: Option<String>,
    pub cert_titular: Option<String>,
    pub cert_vence: Option<String>,
    pub fecha: Option<String>,
    /// ID de las credenciales API de SUNAT para guías (migración 0026).
    pub gre_client_id: Option<String>,
}

#[derive(Serialize)]
pub struct FacturacionDetalle {
    /// "DIRECTO", "FACTURALIBRE" o "SIN_CONFIGURAR".
    pub modo: String,
    /// Datos del emisor: campo -> valor.
    pub datos: Map<String, Value>,
    /// Qué falta para poder emitir directo (None = completo).
    pub falta: Option<String>,
    pub alta: AltaGuardada,
    /// Este negocio aún no tiene la migración 0021 (reinicia el backend).
    pub migracion_pendiente: bool,
    /// ¿El backend sabe dónde está Lycet (LYCET_URL)?
    pub lycet_configurado: bool,
    pub facturalibre_configurado: bool,
    /// Comprobantes emitidos directo, por estado.
    pub emitidos: Map<String, Value>,
}

pub async fn detalle_facturacion(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<FacturacionDetalle>, Fallo> {
    let conn = conexion_negocio(&state, id).await?;

    let columnas: Vec<&str> = CAMPOS.iter().map(|(c, _)| *c).collect();
    let consulta = format!("SELECT {} FROM configuracion_tienda LIMIT 1", columnas.join(", "));
    let valores = texto(&conn, &consulta)
        .await
        .ok_or_else(|| malo("Este negocio aún no tiene la actualización para emitir directo (migración 0020). Reinicia el backend."))?;
    let mut datos = Map::new();
    for (campo, valor) in columnas.iter().zip(valores) {
        datos.insert(campo.to_string(), Value::String(valor.unwrap_or_default()));
    }

    let base = texto(&conn, "SELECT facturacion_proveedor, facturalibre_token, facturalibre_ruta FROM configuracion_tienda LIMIT 1")
        .await
        .unwrap_or_default();
    let facturalibre_configurado = no_vacio(base.get(1)).is_some() && no_vacio(base.get(2)).is_some();
    let modo = if base.first().cloned().flatten().as_deref() == Some("SUNAT_DIRECTO") {
        "DIRECTO"
    } else if facturalibre_configurado {
        "FACTURALIBRE"
    } else {
        "SIN_CONFIGURAR"
    };

    let alta_fila = texto(
        &conn,
        "SELECT sunat_usuario_sol, sunat_ambiente, sunat_cert_titular, sunat_cert_vence, sunat_alta_fecha FROM configuracion_tienda LIMIT 1",
    )
    .await;
    let migracion_pendiente = alta_fila.is_none();
    let alta = alta_fila
        .map(|f| AltaGuardada {
            usuario_sol: no_vacio(f.first()),
            ambiente: no_vacio(f.get(1)),
            cert_titular: no_vacio(f.get(2)),
            cert_vence: no_vacio(f.get(3)),
            fecha: no_vacio(f.get(4)),
            gre_client_id: None,
        })
        .unwrap_or_default();
    let mut alta = alta;
    alta.gre_client_id = texto(&conn, "SELECT sunat_gre_client_id FROM configuracion_tienda LIMIT 1")
        .await
        .and_then(|f| no_vacio(f.first()));

    let falta = match leer_emisor(&conn).await {
        Ok(e) => e.dato_faltante().map(|f| format!("Falta {}.", f)),
        Err((_, e)) => Some(e),
    };

    let mut emitidos = Map::new();
    if let Ok(mut filas) = conn
        .query("SELECT estado, COUNT(*) FROM comprobantes_electronicos WHERE proveedor = 'SUNAT_DIRECTO' GROUP BY estado", ())
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            if let (Ok(estado), Ok(n)) = (f.get::<String>(0), f.get::<i64>(1)) {
                emitidos.insert(estado, Value::from(n));
            }
        }
    }

    Ok(Json(FacturacionDetalle {
        modo: modo.to_string(),
        datos,
        falta,
        alta,
        migracion_pendiente,
        lycet_configurado: servidor_lycet().is_some(),
        facturalibre_configurado,
        emitidos,
    }))
}

/// Guarda los datos del emisor que llegan (solo esos). Valida todos antes
/// de guardar: o se guardan todos o ninguno.
pub async fn guardar_datos(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(datos): Json<HashMap<String, String>>,
) -> Result<Json<Value>, Fallo> {
    let mut validos = Vec::new();
    let mut errores = Vec::new();
    for (campo, _) in CAMPOS {
        if let Some(valor) = datos.get(campo) {
            match alta_sunat::validar_dato(campo, valor) {
                Ok(v) => validos.push((campo, v)),
                Err(e) => errores.push(e),
            }
        }
    }
    if let Some(desconocido) = datos.keys().find(|k| !CAMPOS.iter().any(|(c, _)| c == k)) {
        errores.push(format!("Campo desconocido: {}", desconocido));
    }
    if !errores.is_empty() {
        return Err(malo(errores.join(" · ")));
    }
    if validos.is_empty() {
        return Err(malo("No hay nada que guardar."));
    }
    let conn = conexion_negocio(&state, id).await?;
    let asignaciones: Vec<String> = validos.iter().enumerate().map(|(i, (c, _))| format!("{} = ?{}", c, i + 1)).collect();
    let valores: Vec<libsql::Value> = validos.iter().map(|(_, v)| libsql::Value::Text(v.clone())).collect();
    conn.execute(&format!("UPDATE configuracion_tienda SET {}", asignaciones.join(", ")), valores)
        .await
        .map_err(interno)?;
    Ok(Json(serde_json::json!({ "ok": true, "guardados": validos.len() })))
}

/// Alta en Lycet con el certificado subido (multipart):
/// certificado (archivo), clave_certificado, usuario_sol, clave_sol, ambiente.
pub async fn dar_de_alta(State(state): State<Arc<AppState>>, Path(id): Path<i64>, mut partes: Multipart) -> Result<Json<AltaHecha>, Fallo> {
    let mut archivo: Option<Vec<u8>> = None;
    let mut campos: HashMap<String, String> = HashMap::new();
    while let Some(parte) = partes.next_field().await.map_err(|e| malo(format!("Formulario no válido: {}", e)))? {
        let nombre = parte.name().unwrap_or_default().to_string();
        if nombre == "certificado" {
            let bytes = parte.bytes().await.map_err(|e| malo(format!("No se pudo leer el archivo: {}", e)))?;
            archivo = Some(bytes.to_vec());
        } else {
            campos.insert(nombre, parte.text().await.unwrap_or_default());
        }
    }
    let archivo = archivo.ok_or_else(|| malo("Falta el archivo del certificado (.p12, .pfx o .pem)."))?;
    let campo = |n: &str| campos.get(n).cloned().unwrap_or_default();

    let cert = certificado::leer(&archivo, &campo("clave_certificado")).map_err(malo)?;
    if cert.descripcion.vence < crate::logica::tiempo::hoy_lima() {
        return Err(malo(format!("Ese certificado venció el {}.", cert.descripcion.vence)));
    }
    let conn = conexion_negocio(&state, id).await?;
    let (gre_id, gre_secreto) = (campo("gre_client_id"), campo("gre_client_secret"));
    if gre_id.trim().is_empty() != gre_secreto.trim().is_empty() {
        return Err(malo("Para las guías hacen falta las dos credenciales API de SUNAT: el ID y la clave."));
    }
    let guias = (!gre_id.trim().is_empty()).then_some((gre_id.as_str(), gre_secreto.as_str()));
    let hecha = alta_sunat::registrar(&conn, &cert, &campo("usuario_sol"), &campo("clave_sol"), &campo("ambiente"), guias)
        .await
        .map_err(malo)?;
    Ok(Json(hecha))
}

#[derive(Deserialize)]
pub struct CambiarModo {
    /// "DIRECTO" o "FACTURALIBRE".
    pub modo: String,
}

pub async fn cambiar_modo(State(state): State<Arc<AppState>>, Path(id): Path<i64>, Json(datos): Json<CambiarModo>) -> Result<Json<Value>, Fallo> {
    let conn = conexion_negocio(&state, id).await?;
    let valor = match datos.modo.trim().to_uppercase().as_str() {
        "DIRECTO" | "SUNAT_DIRECTO" => {
            if servidor_lycet().is_none() {
                return Err(malo("Falta LYCET_URL en el .env del backend."));
            }
            let emisor = leer_emisor(&conn).await?;
            if let Some(falta) = emisor.dato_faltante() {
                return Err(malo(format!("Falta {} para emitir directo.", falta)));
            }
            "SUNAT_DIRECTO"
        }
        "FACTURALIBRE" => "FACTURALIBRE",
        _ => return Err(malo("Modo no válido.")),
    };
    conn.execute("UPDATE configuracion_tienda SET facturacion_proveedor = ?1", libsql::params![valor])
        .await
        .map_err(interno)?;
    Ok(Json(serde_json::json!({ "ok": true, "modo": if valor == "SUNAT_DIRECTO" { "DIRECTO" } else { "FACTURALIBRE" } })))
}

pub async fn probar_conexion(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Result<Json<Vec<Chequeo>>, Fallo> {
    let conn = conexion_negocio(&state, id).await?;
    Ok(Json(alta_sunat::probar(&conn).await))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn compara_claves_sin_atajos() {
        assert!(iguales("clave-larga-1", "clave-larga-1"));
        assert!(!iguales("clave-larga-1", "clave-larga-2"));
        assert!(!iguales("clave", "clave-larga"));
        assert!(!iguales("", "x"));
    }

    /// Un token de negocio no sirve como token del panel (y al revés).
    #[test]
    fn tokens_no_se_cruzan() {
        let secreto = b"secreto-de-prueba";
        let negocio = crate::models::auth::Claims {
            sub: 1,
            username: "admin".into(),
            rol_id: 1,
            nombre_completo: "Admin".into(),
            tienda_id: 6,
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
        };
        let t_negocio = encode(&Header::default(), &negocio, &EncodingKey::from_secret(secreto)).unwrap();
        assert!(decode::<PanelClaims>(&t_negocio, &DecodingKey::from_secret(secreto), &Validation::default()).is_err());

        let panel = PanelClaims { panel: true, usuario: "yo".into(), exp: negocio.exp };
        let t_panel = encode(&Header::default(), &panel, &EncodingKey::from_secret(secreto)).unwrap();
        assert!(decode::<crate::models::auth::Claims>(&t_panel, &DecodingKey::from_secret(secreto), &Validation::default()).is_err());
    }
}
