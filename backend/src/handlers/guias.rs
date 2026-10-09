//! Guía de remisión remitente electrónica (módulo GUIAS). Los negocios en
//! emisión directa (SUNAT_DIRECTO) la emiten por Lycet: ver
//! guias_directas.rs. El resto, por FacturaLibre, como se describe aquí.
//!
//! FacturaLibre la emite en tres pasos: se crea (/api/dispatches), se envía
//! a SUNAT (/api/dispatches/send) y se consulta su ticket
//! (/api/dispatches/status_ticket). SUNAT puede tardar en responder: la
//! guía queda ENVIADA y se vuelve a consultar con POST /guias/:id/consultar.
//! Si FacturaLibre no acepta crearla, no se guarda nada y se devuelve su
//! mensaje para corregir el formulario.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_GUIAS};
use crate::logica::guias::*;
use crate::logica::tiempo::{ahora_lima, hoy_lima};
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

const SERIE_POR_DEFECTO: &str = "T001";

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(m: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, m.into())
}
/// La base aún no tiene la migración 0016.
fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

// ============================================================
// Configuración
// ============================================================

struct Config {
    token: String,
    ruta: String,
    serie: String,
    ubigeo: String,
    direccion: String,
    correo: Option<String>,
    telefono: Option<String>,
    ultimo: Option<Value>,
    /// El negocio emite directo a SUNAT (las guías van por Lycet).
    directo: bool,
}

async fn leer_config(conn: &libsql::Connection) -> Result<Config, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT facturalibre_token, facturalibre_ruta, serie_guia, ubigeo, direccion, email, telefono, guia_ultimo
             FROM configuracion_tienda LIMIT 1",
            (),
        )
        .await
        .map_err(actualizando)?;
    let f = filas.next().await.map_err(e500)?.ok_or_else(|| e500("El negocio no tiene configuración."))?;
    let texto = |i: i32| f.get::<String>(i).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    Ok(Config {
        token: texto(0).unwrap_or_default(),
        ruta: texto(1).unwrap_or_default(),
        serie: texto(2).unwrap_or_else(|| SERIE_POR_DEFECTO.to_string()),
        ubigeo: texto(3).unwrap_or_default(),
        direccion: texto(4).unwrap_or_default(),
        correo: texto(5),
        telefono: texto(6),
        ultimo: texto(7).and_then(|t| serde_json::from_str(&t).ok()),
        directo: crate::handlers::envios_sunat::es_modo_directo(conn).await,
    })
}

#[derive(Debug, Serialize)]
pub struct ConfigGuias {
    pub serie: String,
    /// Ubigeo y dirección del local: el punto de partida por defecto.
    pub ubigeo: String,
    pub direccion: String,
    /// Se pueden emitir guías: FacturaLibre (token y URL) configurado, o el
    /// negocio emite directo a SUNAT.
    pub facturacion_lista: bool,
    /// Las guías van directo a SUNAT (no por FacturaLibre).
    pub directo: bool,
    /// Datos de transporte de la última guía, para no volver a escribirlos.
    pub ultimo: Option<Value>,
    pub motivos: Vec<Motivo>,
}

#[derive(Debug, Serialize)]
pub struct Motivo {
    pub codigo: String,
    pub nombre: String,
}

fn config_publica(c: Config) -> ConfigGuias {
    ConfigGuias {
        serie: c.serie,
        ubigeo: c.ubigeo,
        direccion: c.direccion,
        facturacion_lista: c.directo || (!c.token.is_empty() && !c.ruta.is_empty()),
        directo: c.directo,
        ultimo: c.ultimo,
        motivos: MOTIVOS.iter().map(|(c, n)| Motivo { codigo: c.to_string(), nombre: n.to_string() }).collect(),
    }
}

/// GET /guias/config
pub async fn obtener_config(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<ConfigGuias> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(config_publica(leer_config(&conn).await?)))
}

#[derive(Debug, Deserialize)]
pub struct GuardarConfig {
    pub serie: String,
    pub ubigeo: String,
}

/// PUT /configuracion/guias — serie de guías y ubigeo del local (solo admin).
pub async fn guardar_config(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarConfig>,
) -> Resultado<ConfigGuias> {
    exigir_admin(&claims)?;
    let serie = payload.serie.trim().to_uppercase();
    // Las guías de remitente electrónicas usan series que empiezan con T.
    if serie.len() != 4 || !serie.starts_with('T') || !serie.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(malo("La serie de guías tiene 4 caracteres y empieza con T (por ejemplo T001)."));
    }
    let ubigeo = payload.ubigeo.trim().to_string();
    if !(ubigeo.len() == 6 && ubigeo.chars().all(|c| c.is_ascii_digit())) {
        return Err(malo("Elige el distrito de tu local."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    conn.execute("UPDATE configuracion_tienda SET serie_guia = ?1, ubigeo = ?2", libsql::params![serie, ubigeo])
        .await
        .map_err(actualizando)?;
    Ok(Json(config_publica(leer_config(&conn).await?)))
}

// ============================================================
// Datos de una venta para llenar la guía
// ============================================================

#[derive(Debug, Serialize)]
pub struct VentaParaGuia {
    pub venta_id: i64,
    pub folio: String,
    pub fecha: String,
    pub total: f64,
    pub destinatario_tipo: Option<String>,
    pub destinatario_documento: Option<String>,
    pub destinatario_nombre: Option<String>,
    pub destinatario_direccion: Option<String>,
    /// "F001-190" si la venta tiene comprobante aceptado.
    pub comprobante: Option<String>,
    pub items: Vec<ItemGuia>,
    /// Guías ya emitidas para esta venta.
    pub guias: Vec<Guia>,
}

async fn items_de_venta(conn: &libsql::Connection, venta_id: i64) -> Result<Vec<ItemGuia>, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT COALESCE(p.codigo, ''), COALESCE(dv.nombre_producto, p.nombre, ''), CAST(dv.cantidad AS REAL),
                    COALESCE(dv.unidad_medida, p.unidad_medida, 'UNIDAD'), COALESCE(p.controla_stock, 1)
             FROM detalles_venta dv LEFT JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1 ORDER BY dv.id",
            libsql::params![venta_id],
        )
        .await
        .map_err(e500)?;
    let mut items = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        // Los servicios (corte, flete) no viajan: no van en la guía.
        if f.get::<i64>(4).unwrap_or(1) == 0 {
            continue;
        }
        items.push(ItemGuia {
            codigo: f.get(0).unwrap_or_default(),
            descripcion: f.get(1).unwrap_or_default(),
            cantidad: f.get(2).unwrap_or(0.0),
            unidad: f.get(3).unwrap_or_default(),
        });
    }
    Ok(items)
}

async fn documento_de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<DocumentoAfectado> {
    let mut filas = conn
        .query(
            "SELECT serie, numero, tipo FROM comprobantes_electronicos
             WHERE venta_id = ?1 AND estado = 'ACEPTADO' ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .ok()?;
    let f = filas.next().await.ok()??;
    Some((f.get::<String>(0).ok()?, f.get::<i64>(1).ok()?, f.get::<String>(2).ok()?))
}

/// GET /guias/venta/:folio — busca la venta por su folio (o por su id) y
/// devuelve lo necesario para llenar la guía.
pub async fn venta_para_guia(Extension(tenant): Extension<Arc<TenantDb>>, Path(folio): Path<String>) -> Resultado<VentaParaGuia> {
    let conn = tenant.0.connect().map_err(e500)?;
    let folio = folio.trim().to_string();
    let mut filas = conn
        .query(
            "SELECT v.id, v.folio, v.fecha_hora, CAST(v.total AS REAL), c.tipo_documento, c.numero_documento,
                    c.nombre_razon_social, c.direccion
             FROM ventas v LEFT JOIN clientes c ON c.id = v.cliente_id
             WHERE v.estado = 'COMPLETADA' AND (v.folio = ?1 OR CAST(v.id AS TEXT) = ?1) LIMIT 1",
            libsql::params![folio],
        )
        .await
        .map_err(e500)?;
    let f = filas.next().await.map_err(e500)?.ok_or((StatusCode::NOT_FOUND, "No se encontró esa venta.".to_string()))?;
    let venta_id: i64 = f.get(0).unwrap_or_default();
    let venta = VentaParaGuia {
        venta_id,
        folio: f.get(1).unwrap_or_default(),
        fecha: f.get(2).unwrap_or_default(),
        total: f.get(3).unwrap_or(0.0),
        destinatario_tipo: f.get::<String>(4).ok(),
        destinatario_documento: f.get::<String>(5).ok(),
        destinatario_nombre: f.get::<String>(6).ok(),
        destinatario_direccion: f.get::<String>(7).ok(),
        comprobante: documento_de_venta(&conn, venta_id).await.map(|(s, n, _)| format!("{}-{}", s, n)),
        items: items_de_venta(&conn, venta_id).await?,
        guias: listar_donde(&conn, "WHERE g.venta_id = ?1", Some(venta_id)).await.unwrap_or_default(),
    };
    Ok(Json(venta))
}

// ============================================================
// Listado
// ============================================================

#[derive(Debug, Serialize, Clone)]
pub struct Guia {
    pub id: i64,
    pub venta_id: Option<i64>,
    pub folio_venta: Option<String>,
    /// "T001-75"
    pub numero: String,
    /// 'REGISTRADA' | 'ENVIADA' | 'ACEPTADA' | 'RECHAZADA'
    pub estado: String,
    pub mensaje: Option<String>,
    pub enlace_pdf: Option<String>,
    pub enlace_xml: Option<String>,
    pub enlace_cdr: Option<String>,
    pub destinatario_nombre: String,
    pub destinatario_documento: String,
    pub llegada_direccion: String,
    pub fecha_traslado: String,
    pub usuario: String,
    pub fecha: String,
    /// El formulario con que se emitió (transporte, direcciones, peso,
    /// ítems), para imprimirla en ticket. Solo viene al pedir una guía, no
    /// en la lista.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datos: Option<Value>,
    /// Comprobante de la venta ("F001-128"), si tiene uno aceptado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comprobante: Option<String>,
    /// QR de SUNAT, si FacturaLibre lo entregó al aceptarse: un enlace (con
    /// el que se dibuja el código) o la imagen ("data:image/...").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr: Option<String>,
    /// Emitida directo a SUNAT: el XML y la constancia se bajan de
    /// /guias/:id/xml y /guias/:id/cdr (no hay enlaces de FacturaLibre).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub directa: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tiene_xml: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tiene_cdr: bool,
}

/// Campo de `datos_json` donde se guarda el QR de SUNAT (no es del formulario).
const CAMPO_QR: &str = "qr_sunat";

async fn listar_donde(conn: &libsql::Connection, donde: &str, venta_id: Option<i64>) -> Result<Vec<Guia>, (StatusCode, String)> {
    let sql = format!(
        "SELECT g.id, g.venta_id, v.folio, COALESCE(g.serie, ''), COALESCE(g.numero, 0), g.estado, g.mensaje,
                g.enlace_pdf, g.enlace_xml, g.enlace_cdr, COALESCE(g.destinatario_nombre, ''),
                COALESCE(g.destinatario_documento, ''), COALESCE(g.llegada_direccion, ''), COALESCE(g.fecha_traslado, ''),
                COALESCE(u.nombre_completo, ''), g.fecha
         FROM guias_remision g
         LEFT JOIN ventas v ON v.id = g.venta_id
         LEFT JOIN usuarios u ON u.id = g.usuario_id
         {} ORDER BY g.id DESC LIMIT 200",
        donde
    );
    let mut filas = match venta_id {
        Some(id) => conn.query(&sql, libsql::params![id]).await,
        None => conn.query(&sql, ()).await,
    }
    .map_err(actualizando)?;
    let mut guias = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        guias.push(Guia {
            id: f.get(0).unwrap_or_default(),
            venta_id: f.get::<i64>(1).ok(),
            folio_venta: f.get::<String>(2).ok(),
            numero: format!("{}-{}", f.get::<String>(3).unwrap_or_default(), f.get::<i64>(4).unwrap_or(0)),
            estado: f.get(5).unwrap_or_default(),
            mensaje: f.get::<String>(6).ok(),
            enlace_pdf: f.get::<String>(7).ok(),
            enlace_xml: f.get::<String>(8).ok(),
            enlace_cdr: f.get::<String>(9).ok(),
            destinatario_nombre: f.get(10).unwrap_or_default(),
            destinatario_documento: f.get(11).unwrap_or_default(),
            llegada_direccion: f.get(12).unwrap_or_default(),
            fecha_traslado: f.get(13).unwrap_or_default(),
            usuario: f.get(14).unwrap_or_default(),
            fecha: f.get(15).unwrap_or_default(),
            datos: None,
            comprobante: None,
            qr: None,
            directa: false,
            tiene_xml: false,
            tiene_cdr: false,
        });
    }
    drop(filas);
    let ids: Vec<i64> = guias.iter().map(|g| g.id).collect();
    let directas = super::guias_directas::archivos_de(conn, &ids).await;
    for g in guias.iter_mut() {
        if let Some((xml, cdr)) = directas.get(&g.id) {
            g.directa = true;
            g.tiene_xml = *xml;
            g.tiene_cdr = *cdr;
        }
    }
    Ok(guias)
}

/// El formulario guardado de una guía (None si no se puede leer).
async fn datos_guardados(conn: &libsql::Connection, id: i64) -> Option<Value> {
    let mut filas = conn.query("SELECT datos_json FROM guias_remision WHERE id = ?1", libsql::params![id]).await.ok()?;
    let texto = filas.next().await.ok()??.get::<String>(0).ok()?;
    serde_json::from_str::<Value>(&texto).ok().filter(Value::is_object)
}

/// Una guía con todo su detalle. Lo que es solo para imprimirla (datos,
/// comprobante, QR) nunca hace fallar la consulta: si no se puede leer, no viene.
async fn una(conn: &libsql::Connection, id: i64) -> Result<Guia, (StatusCode, String)> {
    let mut guia = listar_donde(conn, "WHERE g.id = ?1", Some(id))
        .await?
        .into_iter()
        .next()
        .ok_or((StatusCode::NOT_FOUND, "Esa guía no existe.".to_string()))?;
    if let Some(mut datos) = datos_guardados(conn, id).await {
        if let Some(campos) = datos.as_object_mut() {
            guia.qr = campos.remove(CAMPO_QR).and_then(|v| v.as_str().map(|t| t.to_string())).filter(|t| !t.is_empty());
        }
        guia.datos = Some(datos);
    }
    if let Some(venta_id) = guia.venta_id {
        guia.comprobante = documento_de_venta(conn, venta_id).await.map(|(s, n, _)| format!("{}-{}", s, n));
    }
    Ok(guia)
}

/// GET /guias/:id — una guía con su detalle completo (para imprimirla).
pub async fn detalle(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Resultado<Guia> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(una(&conn, id).await?))
}

/// Guarda el QR de SUNAT junto al formulario de la guía. Sin fallar: el QR
/// solo sirve para el ticket impreso.
async fn guardar_qr(conn: &libsql::Connection, id: i64, qr: &str) {
    let Some(mut datos) = datos_guardados(conn, id).await else { return };
    datos[CAMPO_QR] = Value::String(qr.to_string());
    let _ = conn.execute("UPDATE guias_remision SET datos_json = ?1 WHERE id = ?2", libsql::params![datos.to_string(), id]).await;
}

/// GET /guias — las últimas 200.
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<Guia>> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(listar_donde(&conn, "", None).await?))
}

// ============================================================
// Emisión
// ============================================================

/// Llama a un endpoint de FacturaLibre y devuelve el JSON de respuesta.
/// Err = no hubo respuesta utilizable (red caída, respuesta que no es JSON).
async fn llamar(url: &str, token: &str, cuerpo: &Value) -> Result<Value, String> {
    let respuesta = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(cuerpo)
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con FacturaLibre: {}", e))?;
    let texto = respuesta.text().await.map_err(|e| format!("FacturaLibre no respondió: {}", e))?;
    serde_json::from_str::<Value>(&texto)
        .map_err(|_| format!("FacturaLibre respondió algo inesperado: {}", texto.chars().take(300).collect::<String>()))
}

fn exito(r: &Value) -> bool {
    r.get("success").and_then(Value::as_bool).unwrap_or(false)
}

fn mensaje_de(r: &Value) -> String {
    r.get("message")
        .and_then(Value::as_str)
        .map(|m| m.to_string())
        .unwrap_or_else(|| r.to_string().chars().take(300).collect())
}

/// Envía la guía a SUNAT (si aún no se envió) y consulta su ticket.
/// Actualiza la fila con lo que responda; nunca devuelve error por lo que
/// diga SUNAT: eso queda en `estado` y `mensaje`.
async fn enviar_y_consultar(conn: &libsql::Connection, id: i64, base: &str, token: &str) -> Result<(), (StatusCode, String)> {
    let mut filas = conn
        .query("SELECT COALESCE(external_id, ''), estado FROM guias_remision WHERE id = ?1", libsql::params![id])
        .await
        .map_err(actualizando)?;
    let (external_id, estado): (String, String) = match filas.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default()),
        None => return Err((StatusCode::NOT_FOUND, "Esa guía no existe.".to_string())),
    };
    drop(filas);
    if external_id.is_empty() || estado == "ACEPTADA" {
        return Ok(());
    }
    let cuerpo = serde_json::json!({ "external_id": external_id });

    // Paso 2: enviar a SUNAT. Se repite si quedó solo registrada o fue rechazada.
    if estado != "ENVIADA" {
        match llamar(&format!("{}/api/dispatches/send", base), token, &cuerpo).await {
            Ok(r) if exito(&r) => {
                conn.execute("UPDATE guias_remision SET estado = 'ENVIADA', mensaje = ?1 WHERE id = ?2", libsql::params![mensaje_de(&r), id])
                    .await
                    .map_err(e500)?;
            }
            Ok(r) => {
                conn.execute("UPDATE guias_remision SET mensaje = ?1 WHERE id = ?2", libsql::params![mensaje_de(&r), id]).await.map_err(e500)?;
                return Ok(());
            }
            Err(e) => {
                conn.execute("UPDATE guias_remision SET mensaje = ?1 WHERE id = ?2", libsql::params![e, id]).await.map_err(e500)?;
                return Ok(());
            }
        }
    }

    // Paso 3: consultar el ticket.
    match llamar(&format!("{}/api/dispatches/status_ticket", base), token, &cuerpo).await {
        Ok(r) if exito(&r) => {
            let nuevo = estado_de_ticket(r.pointer("/data/state_type_id").and_then(Value::as_str));
            let enlace = |k: &str| r.pointer(&format!("/links/{}", k)).and_then(Value::as_str).map(|s| s.to_string());
            conn.execute(
                "UPDATE guias_remision SET estado = ?1, mensaje = ?2,
                        enlace_pdf = COALESCE(?3, enlace_pdf), enlace_xml = COALESCE(?4, enlace_xml), enlace_cdr = COALESCE(?5, enlace_cdr)
                 WHERE id = ?6",
                libsql::params![nuevo, mensaje_de(&r), enlace("pdf"), enlace("xml"), enlace("cdr"), id],
            )
            .await
            .map_err(e500)?;
            if let Some(qr) = qr_de_respuesta(&r) {
                guardar_qr(conn, id, &qr).await;
            }
        }
        Ok(r) => {
            conn.execute("UPDATE guias_remision SET mensaje = ?1 WHERE id = ?2", libsql::params![mensaje_de(&r), id]).await.map_err(e500)?;
        }
        Err(e) => {
            conn.execute("UPDATE guias_remision SET mensaje = ?1 WHERE id = ?2", libsql::params![e, id]).await.map_err(e500)?;
        }
    }
    Ok(())
}

/// POST /guias — emite una guía de remisión.
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(mut payload): Json<DatosGuia>,
) -> Resultado<Guia> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_GUIAS, "Guías de remisión").await?;
    let cfg = leer_config(&conn).await?;
    conn.query("SELECT 1 FROM guias_remision LIMIT 1", ()).await.map_err(actualizando)?;
    if !cfg.directo && (cfg.token.is_empty() || cfg.ruta.is_empty()) {
        return Err(malo("Falta configurar el Token y la URL de FacturaLibre en Configuración antes de emitir guías."));
    }
    if cfg.ubigeo.is_empty() || cfg.direccion.is_empty() {
        return Err(malo("Falta la dirección y el distrito de tu local (Guías → Datos de mi local)."));
    }

    // Sin ítems escritos, van los de la venta.
    if payload.items.is_empty() {
        if let Some(venta_id) = payload.venta_id {
            payload.items = items_de_venta(&conn, venta_id).await?;
        }
    }
    let hoy = hoy_lima();
    let datos = validar(&payload, &hoy).map_err(malo)?;
    let documento = match datos.venta_id {
        Some(venta_id) => documento_de_venta(&conn, venta_id).await,
        None => None,
    };
    // Emisión directa: la guía va a SUNAT por Lycet (guias_directas.rs).
    if cfg.directo {
        conn.query("SELECT ticket FROM guias_remision LIMIT 1", ()).await.map_err(actualizando)?;
        let id = super::guias_directas::crear(&conn, &datos, &cfg.serie, documento.as_ref(), claims.sub).await?;
        recordar_transporte(&conn, &datos).await;
        return Ok(Json(una(&conn, id).await?));
    }

    let emisor = Emisor { ubigeo: cfg.ubigeo.clone(), direccion: cfg.direccion.clone(), correo: cfg.correo.clone(), telefono: cfg.telefono.clone() };
    let hora = ahora_lima().chars().skip(11).collect::<String>();
    let cuerpo = armar_payload(&datos, &emisor, &cfg.serie, &hoy, &hora, documento.as_ref());
    let base = url_base(&cfg.ruta);

    // Paso 1: crear la guía en FacturaLibre. Si no la acepta, no se guarda
    // nada y se muestra su mensaje para corregir el formulario.
    let r = llamar(&format!("{}/api/dispatches", base), &cfg.token, &cuerpo).await.map_err(malo)?;
    if !exito(&r) {
        return Err(malo(format!("FacturaLibre no aceptó la guía: {}", mensaje_de(&r))));
    }
    let numero_completo = r.pointer("/data/number").and_then(Value::as_str).unwrap_or_default().to_string();
    let (serie, numero) = numero_completo
        .rsplit_once('-')
        .map(|(s, n)| (s.to_string(), n.parse::<i64>().unwrap_or(0)))
        .unwrap_or((cfg.serie.clone(), 0));
    let external_id = r.pointer("/data/external_id").and_then(Value::as_str).unwrap_or_default().to_string();

    conn.execute(
        "INSERT INTO guias_remision (venta_id, serie, numero, external_id, estado, mensaje, destinatario_nombre,
                                     destinatario_documento, llegada_direccion, fecha_traslado, datos_json, usuario_id, fecha)
         VALUES (?1, ?2, ?3, ?4, 'REGISTRADA', 'Creada; falta enviarla a SUNAT', ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        libsql::params![
            datos.venta_id, serie, numero, external_id, datos.destinatario_nombre.clone(), datos.destinatario_documento.clone(),
            datos.llegada.direccion.clone(), datos.fecha_traslado.clone(),
            serde_json::to_string(&datos).unwrap_or_default(), claims.sub, ahora_lima()
        ],
    )
    .await
    .map_err(e500)?;
    let id = conn.last_insert_rowid();

    recordar_transporte(&conn, &datos).await;

    enviar_y_consultar(&conn, id, &base, &cfg.token).await?;
    Ok(Json(una(&conn, id).await?))
}

/// Se recuerdan los datos de transporte para la próxima guía (sin fallar).
async fn recordar_transporte(conn: &libsql::Connection, datos: &DatosGuia) {
    let ultimo = serde_json::json!({
        "modo": datos.modo, "transportista": datos.transportista, "chofer": datos.chofer, "placa": datos.placa,
    });
    let _ = conn.execute("UPDATE configuracion_tienda SET guia_ultimo = ?1", libsql::params![ultimo.to_string()]).await;
}

/// POST /guias/:id/consultar — reintenta el envío y consulta la respuesta
/// de SUNAT de una guía que quedó registrada, enviada o rechazada.
pub async fn consultar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Resultado<Guia> {
    let conn = tenant.0.connect().map_err(e500)?;
    if super::guias_directas::es_directa(&conn, id).await {
        let lycet = super::envios_sunat::Lycet::exigir()?;
        super::guias_directas::avanzar(&conn, &lycet, id, true).await?;
        return Ok(Json(una(&conn, id).await?));
    }
    let cfg = leer_config(&conn).await?;
    if cfg.token.is_empty() || cfg.ruta.is_empty() {
        return Err(malo("Falta configurar FacturaLibre en Configuración."));
    }
    enviar_y_consultar(&conn, id, &url_base(&cfg.ruta), &cfg.token).await?;
    Ok(Json(una(&conn, id).await?))
}
