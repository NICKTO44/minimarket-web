//! Guías de remisión emitidas directo a SUNAT (negocios en SUNAT_DIRECTO),
//! por la API de guías de SUNAT a través de Lycet (/api/v1/despatch).
//!
//! Igual que en la emisión directa de comprobantes, el número lo pone el
//! sistema: se reserva antes de enviar y la guía se guarda tal cual. SUNAT
//! responde con un ticket y la constancia se pide con él:
//!   REGISTRADA -> aún no llega a SUNAT (sin conexión): se reenvía igual.
//!   ENVIADA    -> SUNAT la recibió (ticket) y la está procesando.
//!   ACEPTADA / RECHAZADA / ERROR.
//! La pantalla de Guías y su formulario son los mismos (handlers/guias.rs);
//! aquí solo cambia a quién se envía.

use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::envios_sunat::Lycet;
use crate::logica::guias::{armar_guia_sunat, DatosGuia, DocumentoAfectado};
use crate::logica::sunat_directo::EstadoEnvio;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(m: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, m.into())
}

/// Campo de `datos_json` donde se guarda el QR de SUNAT (el mismo que usan
/// las guías de FacturaLibre, ver guias.rs).
pub const CAMPO_QR: &str = "qr_sunat";

/// Emite una guía directa: reserva el número, guarda la guía y la envía.
/// Devuelve el id de la guía. `datos` ya viene validado.
pub async fn crear(
    conn: &libsql::Connection,
    datos: &DatosGuia,
    serie: &str,
    documento: Option<&DocumentoAfectado>,
    usuario_id: i64,
) -> Result<i64, Fallo> {
    let lycet = Lycet::exigir()?;
    let emisor = super::facturacion_directa::leer_emisor(conn).await?;
    if let Some(falta) = emisor.dato_faltante() {
        return Err(malo(format!("Falta {} del negocio para emitir guías a SUNAT.", falta)));
    }
    let ahora = crate::logica::tiempo::ahora_lima();
    let (hoy, hora) = ahora.split_once(' ').unwrap_or((ahora.as_str(), "00:00:00"));
    let datos_json = serde_json::to_string(datos).unwrap_or_default();

    // El número sigue a todas las guías de la serie (también las que se
    // emitieron antes con FacturaLibre: SUNAT ya tiene esos números).
    let mut reservado: Option<(i64, i64)> = None;
    let mut ultimo = String::new();
    for _ in 0..5 {
        let mut filas = match conn
            .query(
                "INSERT INTO guias_remision (venta_id, serie, numero, proveedor, estado, mensaje, destinatario_nombre,
                                             destinatario_documento, llegada_direccion, fecha_traslado, datos_json, usuario_id, fecha)
                 SELECT ?1, ?2, COALESCE(MAX(numero), 0) + 1, 'SUNAT_DIRECTO', 'REGISTRADA', 'Enviando a SUNAT...', ?3, ?4, ?5, ?6, ?7, ?8, ?9
                 FROM guias_remision WHERE serie = ?2
                 RETURNING id, numero",
                libsql::params![
                    datos.venta_id,
                    serie,
                    datos.destinatario_nombre.clone(),
                    datos.destinatario_documento.clone(),
                    datos.llegada.direccion.clone(),
                    datos.fecha_traslado.clone(),
                    datos_json.clone(),
                    usuario_id,
                    ahora.clone()
                ],
            )
            .await
        {
            Ok(f) => f,
            Err(e) => {
                ultimo = e.to_string();
                if ultimo.to_uppercase().contains("UNIQUE") {
                    continue;
                }
                break;
            }
        };
        let f = filas.next().await.map_err(interno)?.ok_or_else(|| interno("No se pudo reservar el número de la guía"))?;
        reservado = Some((f.get(0).map_err(interno)?, f.get(1).map_err(interno)?));
        while filas.next().await.map_err(interno)?.is_some() {}
        break;
    }
    let (id, numero) = reservado.ok_or_else(|| interno(format!("No se pudo reservar el número de la guía: {}", ultimo)))?;

    let documento_sunat = armar_guia_sunat(datos, &emisor, serie, numero, hoy, hora, documento);
    conn.execute("UPDATE guias_remision SET documento = ?1 WHERE id = ?2", libsql::params![documento_sunat.to_string(), id])
        .await
        .map_err(interno)?;

    avanzar(conn, &lycet, id, false).await?;
    // SUNAT suele procesar la guía en segundos: una consulta más.
    if estado_de(conn, id).await.as_deref() == Some("ENVIADA") {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let _ = avanzar(conn, &lycet, id, false).await;
    }
    Ok(id)
}

async fn estado_de(conn: &libsql::Connection, id: i64) -> Option<String> {
    let mut f = conn.query("SELECT estado FROM guias_remision WHERE id = ?1", libsql::params![id]).await.ok()?;
    f.next().await.ok()??.get::<String>(0).ok()
}

/// true si la guía se emitió directo a SUNAT.
pub async fn es_directa(conn: &libsql::Connection, id: i64) -> bool {
    match conn.query("SELECT proveedor FROM guias_remision WHERE id = ?1", libsql::params![id]).await {
        Ok(mut f) => matches!(f.next().await, Ok(Some(fila)) if fila.get::<String>(0).ok().as_deref() == Some("SUNAT_DIRECTO")),
        Err(_) => false,
    }
}

/// Un paso de una guía directa que aún no termina: sin ticket se envía (el
/// mismo documento); con ticket se consulta. Guarda lo que responda SUNAT.
pub async fn avanzar(conn: &libsql::Connection, lycet: &Lycet, id: i64, es_reintento: bool) -> Result<(), Fallo> {
    let mut filas = conn
        .query("SELECT estado, ticket, documento, datos_json FROM guias_remision WHERE id = ?1", libsql::params![id])
        .await
        .map_err(interno)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Esa guía no existe.".to_string()))?;
    let estado: String = f.get(0).unwrap_or_default();
    let ticket: Option<String> = f.get::<String>(1).ok().filter(|t| !t.trim().is_empty());
    let documento: Value = f
        .get::<String>(2)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| interno("La guía no tiene su documento guardado."))?;
    let datos_json: Option<Value> = f.get::<String>(3).ok().and_then(|t| serde_json::from_str(&t).ok());
    drop(filas);
    if !matches!(estado.as_str(), "REGISTRADA" | "ENVIADA") {
        return Ok(());
    }
    // Reenviarla (sin ticket) la toma primero: el botón y la tarea
    // automática no la mandan dos veces a la vez.
    if es_reintento
        && ticket.is_none()
        && !super::envios_sunat::reclamar(
            conn,
            "guias_remision",
            "estado = 'REGISTRADA' AND (ticket IS NULL OR ticket = '')",
            "fecha",
            "mensaje",
            id,
        )
        .await?
    {
        return Err(super::envios_sunat::en_curso("La guía"));
    }
    let ruc = documento["company"]["ruc"].as_str().unwrap_or("").to_string();
    let ahora = crate::logica::tiempo::ahora_lima();
    if es_reintento || ticket.is_none() {
        let _ = conn
            .execute(
                "UPDATE guias_remision SET intentos = intentos + 1, ultimo_intento = ?1 WHERE id = ?2",
                libsql::params![ahora, id],
            )
            .await;
    }

    match ticket {
        None => {
            let envio = lycet.enviar_con_ticket("despatch", &documento).await;
            let (nuevo, mensaje) = match (&envio.ticket, envio.respuesta.estado) {
                (Some(_), _) => ("ENVIADA", "SUNAT recibió la guía; la está procesando.".to_string()),
                (None, EstadoEnvio::Rechazado) => ("RECHAZADA", envio.respuesta.mensaje.clone()),
                (None, EstadoEnvio::Error) => ("ERROR", envio.respuesta.mensaje.clone()),
                // Lycet responde 500 cuando SUNAT no acepta las credenciales API.
                (None, _) if envio.respuesta.mensaje.contains("HTTP 500") => (
                    "REGISTRADA",
                    format!(
                        "{} Si se repite, revisa en el panel las credenciales API de guías del negocio (ID y clave).",
                        envio.respuesta.mensaje
                    ),
                ),
                (None, _) => ("REGISTRADA", envio.respuesta.mensaje.clone()),
            };
            // Solo si nadie le puso ticket mientras tanto.
            conn.execute(
                "UPDATE guias_remision SET estado = ?1, mensaje = ?2, ticket = COALESCE(?3, ticket), xml = COALESCE(?4, xml)
                 WHERE id = ?5 AND estado = 'REGISTRADA' AND (ticket IS NULL OR ticket = '')",
                libsql::params![nuevo, mensaje, envio.ticket.clone(), envio.respuesta.xml.clone(), id],
            )
            .await
            .map_err(interno)?;
        }
        Some(t) => {
            let r = lycet.consultar_ticket("despatch", &ruc, &t).await;
            let nuevo = match r.estado {
                EstadoEnvio::Aceptado => "ACEPTADA",
                EstadoEnvio::Rechazado | EstadoEnvio::Error => "RECHAZADA",
                EstadoEnvio::Pendiente => "ENVIADA",
            };
            conn.execute(
                "UPDATE guias_remision SET estado = ?1, mensaje = ?2, cdr_zip = COALESCE(?3, cdr_zip) WHERE id = ?4 AND estado = 'ENVIADA'",
                libsql::params![nuevo, r.mensaje.clone(), r.cdr_zip.clone(), id],
            )
            .await
            .map_err(interno)?;
            // El enlace del QR que SUNAT pone en la constancia (para el ticket impreso).
            if let (Some(qr), Some(mut datos)) = (r.referencia.clone(), datos_json) {
                if datos.is_object() {
                    datos[CAMPO_QR] = Value::String(qr);
                    let _ = conn
                        .execute("UPDATE guias_remision SET datos_json = ?1 WHERE id = ?2", libsql::params![datos.to_string(), id])
                        .await;
                }
            }
        }
    }
    Ok(())
}

/// Guías directas de una lista: id -> (tiene XML, tiene constancia).
/// Vacío si la base aún no tiene la migración 0026.
pub async fn archivos_de(conn: &libsql::Connection, ids: &[i64]) -> HashMap<i64, (bool, bool)> {
    let mut mapa = HashMap::new();
    if ids.is_empty() {
        return mapa;
    }
    let lista = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, xml IS NOT NULL, cdr_zip IS NOT NULL FROM guias_remision WHERE proveedor = 'SUNAT_DIRECTO' AND id IN ({})",
        lista
    );
    if let Ok(mut filas) = conn.query(&sql, ()).await {
        while let Ok(Some(f)) = filas.next().await {
            if let Ok(id) = f.get::<i64>(0) {
                mapa.insert(id, (f.get::<i64>(1).unwrap_or(0) == 1, f.get::<i64>(2).unwrap_or(0) == 1));
            }
        }
    }
    mapa
}

async fn descargar(tenant: Arc<TenantDb>, id: i64, cdr: bool) -> Result<Response, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let mut filas = conn
        .query(
            "SELECT serie, numero, xml, cdr_zip, (SELECT ruc FROM configuracion_tienda LIMIT 1)
             FROM guias_remision WHERE id = ?1 AND proveedor = 'SUNAT_DIRECTO'",
            libsql::params![id],
        )
        .await
        .map_err(interno)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Esa guía no existe.".to_string()))?;
    let serie: String = f.get(0).unwrap_or_default();
    let numero: i64 = f.get(1).unwrap_or_default();
    let ruc: String = f.get(4).unwrap_or_default();
    let (bytes, tipo, extension) = if cdr {
        use base64::Engine;
        let b64: String = f.get(3).map_err(|_| (StatusCode::NOT_FOUND, "Esta guía no tiene constancia de SUNAT.".to_string()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| interno("La constancia guardada no es válida."))?;
        (bytes, "application/zip", "zip")
    } else {
        let xml: String = f.get(2).map_err(|_| (StatusCode::NOT_FOUND, "Esta guía no tiene XML guardado.".to_string()))?;
        (xml.into_bytes(), "application/xml", "xml")
    };
    let nombre = format!("{}{}-09-{}-{}.{}", if cdr { "R-" } else { "" }, ruc, serie, numero, extension);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, tipo)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", nombre))
        .body(Body::from(bytes))
        .map(|r| r.into_response())
        .map_err(interno)
}

/// GET /guias/:id/xml (guías directas)
pub async fn descargar_xml(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Response, Fallo> {
    descargar(tenant, id, false).await
}

/// GET /guias/:id/cdr (guías directas)
pub async fn descargar_cdr(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Response, Fallo> {
    descargar(tenant, id, true).await
}
