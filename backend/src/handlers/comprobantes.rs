use axum::{extract::{Extension, Query, Path}, http::{StatusCode, header}, response::{IntoResponse, Response}, body::Body, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::comprobante::ComprobanteResumen;

#[derive(Deserialize)]
pub struct FiltrosComprobante {
    pub tipo: Option<String>,
    pub estado: Option<String>,
}

pub async fn listar_comprobantes(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(filtros): Query<FiltrosComprobante>,
) -> Result<Json<Vec<ComprobanteResumen>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // El RUC del emisor es el mismo para toda la tienda — se trae una
    // sola vez, no por cada fila, y se copia en cada comprobante para
    // que el frontend tenga todo lo que necesita para el QR sin tener
    // que pedirlo aparte.
    let mut r_ruc = conn
        .query("SELECT ruc FROM configuracion_tienda LIMIT 1", ())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ruc_emisor: Option<String> = match r_ruc.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => row.get(0).ok(),
        None => None,
    };

    // Los enlaces del XML y del CDR van al final de la consulta. Si una base
    // no los tuviera, se repite la consulta de siempre (sin ellos) y la
    // pantalla simplemente no ofrece esas descargas.
    let armar_sql = |con_archivos: bool| {
        let mut sql = format!(
            "SELECT ce.id, v.id, v.folio, COALESCE(ce.tipo, 'NINGUNO'), ce.serie, ce.numero,
                c.nombre_razon_social, v.total, ce.estado, v.fecha_hora, ce.mensaje_sunat, ce.enlace_pdf,
                ce.hash, ce.cliente_documento, substr(COALESCE(ce.fecha_emision, v.fecha_hora), 1, 10){}
         FROM ventas v
         LEFT JOIN comprobantes_electronicos ce ON ce.venta_id = v.id
         LEFT JOIN clientes c ON c.id = v.cliente_id
         WHERE v.estado = 'COMPLETADA'",
            if con_archivos { ", ce.enlace_xml, ce.enlace_cdr" } else { "" }
        );

        let mut idx = 1;
        if filtros.tipo.is_some() {
            sql.push_str(&format!(" AND COALESCE(ce.tipo, 'NINGUNO') = ?{}", idx));
            idx += 1;
        }
        if filtros.estado.is_some() {
            sql.push_str(&format!(" AND ce.estado = ?{}", idx));
        }
        sql.push_str(" ORDER BY v.fecha_hora DESC LIMIT 100");
        sql
    };

    let consultar = |sql: String| {
        let conn = &conn;
        let filtros = &filtros;
        async move {
            match (&filtros.tipo, &filtros.estado) {
                (Some(t), Some(e)) => conn.query(&sql, libsql::params![t.clone(), e.clone()]).await,
                (Some(t), None) => conn.query(&sql, libsql::params![t.clone()]).await,
                (None, Some(e)) => conn.query(&sql, libsql::params![e.clone()]).await,
                (None, None) => conn.query(&sql, ()).await,
            }
        }
    };

    let mut rows = match consultar(armar_sql(true)).await {
        Ok(rows) => rows,
        Err(_) => consultar(armar_sql(false)).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };

    let mut comprobantes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        let enlace_pdf: Option<String> = row.get(11).ok();
        let enlace_cdr = limpio(row.get(16).ok());
        let enlace_xml = enlace_xml_de(row.get(15).ok(), enlace_cdr.as_deref(), enlace_pdf.as_deref());
        comprobantes.push(ComprobanteResumen {
            id: row.get(0).ok(),
            venta_id: row.get(1).unwrap_or_default(),
            folio_venta: row.get(2).unwrap_or_default(),
            tipo: row.get(3).unwrap_or_default(),
            serie: row.get(4).ok(),
            numero: row.get(5).ok(),
            cliente_nombre: row.get(6).ok(),
            monto: row.get(7).unwrap_or_default(),
            estado: row.get(8).ok(),
            fecha_emision: row.get(9).unwrap_or_default(),
            mensaje_sunat: row.get(10).ok(),
            enlace_pdf,
            hash: row.get(12).ok(),
            cliente_documento: row.get(13).ok(),
            ruc_emisor: ruc_emisor.clone(),
            fecha_emision_corta: row.get(14).ok(),
            tiene_xml: enlace_xml.is_some(),
            tiene_cdr: enlace_cdr.is_some(),
        });
    }

    Ok(Json(comprobantes))
}

/// Puente para el PDF: en vez de que el navegador le pida el PDF directo
/// a FacturaLibre (que fuerza descarga por sus propias cabeceras), se lo
/// pide este backend y se lo re-entrega con cabeceras propias que sí
/// permiten mostrarlo embebido (Content-Disposition: inline).
pub async fn descargar_pdf(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut rows = conn
        .query("SELECT enlace_pdf FROM comprobantes_electronicos WHERE id = ?1", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let enlace: Option<String> = match rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).ok(),
        None => return Err((StatusCode::NOT_FOUND, "Comprobante no encontrado".into())),
    };

    let enlace = enlace.ok_or((StatusCode::NOT_FOUND, "Este comprobante no tiene PDF disponible".into()))?;

    let cliente = reqwest::Client::new();
    let resp = cliente
        .get(&enlace)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("No se pudo obtener el PDF de FacturaLibre: {}", e)))?;

    if !resp.status().is_success() {
        return Err((StatusCode::BAD_GATEWAY, "FacturaLibre no devolvió el PDF correctamente".into()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error leyendo el PDF: {}", e)))?;

    let respuesta = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::CONTENT_DISPOSITION, "inline; filename=\"comprobante.pdf\"")
        .body(Body::from(bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(respuesta.into_response())
}

// ---------------------------------------------------------------------
// XML firmado y constancia de SUNAT (CDR): los archivos que piden los
// contadores y algunos clientes. Igual que con el PDF, el servidor los
// trae de FacturaLibre y los entrega él, así el navegador los recibe como
// un archivo normal del sistema.
// ---------------------------------------------------------------------

/// Un enlace vacío ("") cuenta como que no hay enlace.
fn limpio(enlace: Option<String>) -> Option<String> {
    enlace.map(|e| e.trim().to_string()).filter(|e| !e.is_empty())
}

/// Enlace del XML firmado. Los comprobantes emitidos antes de que el
/// sistema guardara ese enlace no lo tienen: se deduce del enlace del CDR
/// o del PDF, pero solo cuando siguen la ruta estándar de FacturaLibre
/// (…/downloads/document/<tipo>/<id>), que es la misma para los tres
/// archivos. Si la ruta es otra, no se inventa nada.
pub fn enlace_xml_de(xml: Option<String>, cdr: Option<&str>, pdf: Option<&str>) -> Option<String> {
    if let Some(xml) = limpio(xml) {
        return Some(xml);
    }
    for (enlace, tramo) in [(cdr, "/downloads/document/cdr/"), (pdf, "/downloads/document/pdf/")] {
        if let Some(enlace) = enlace.map(str::trim) {
            if enlace.starts_with("http") && enlace.matches(tramo).count() == 1 {
                return Some(enlace.replace(tramo, "/downloads/document/xml/"));
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum Archivo {
    Xml,
    Cdr,
}

/// Qué es lo que llegó, mirando sus primeros bytes: FacturaLibre entrega
/// XML, y el CDR puede venir como XML o comprimido (ZIP). Una página web
/// (por ejemplo un aviso de error) no es ninguno de los dos.
fn tipo_de_contenido(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"PK") {
        return Some(("application/zip", "zip"));
    }
    let sin_bom = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let inicio: String = String::from_utf8_lossy(&sin_bom[..sin_bom.len().min(300)]).trim_start().to_lowercase();
    if inicio.starts_with('<') && !inicio.starts_with("<!doctype html") && !inicio.starts_with("<html") {
        return Some(("application/xml", "xml"));
    }
    None
}

async fn descargar_archivo(tenant: Arc<TenantDb>, id: i64, archivo: Archivo) -> Result<Response, (StatusCode, String)> {
    // La base se consulta y se suelta antes de salir a buscar el archivo.
    let (tipo, serie, numero, estado, enlace_xml, enlace_cdr, ruc) = {
        let interno = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        let conn = tenant.0.connect().map_err(interno)?;
        let sql = |columna_xml: &str| {
            format!(
                "SELECT tipo, serie, numero, estado, {}, enlace_cdr, enlace_pdf,
                        (SELECT ruc FROM configuracion_tienda LIMIT 1)
                 FROM comprobantes_electronicos WHERE id = ?1",
                columna_xml
            )
        };
        // Igual que en la lista: si la base no guardara el enlace del XML,
        // se trabaja con los enlaces de siempre.
        let mut filas = match conn.query(&sql("enlace_xml"), libsql::params![id]).await {
            Ok(filas) => filas,
            Err(_) => conn.query(&sql("NULL"), libsql::params![id]).await.map_err(interno)?,
        };
        let fila = filas
            .next()
            .await
            .map_err(interno)?
            .ok_or((StatusCode::NOT_FOUND, "Comprobante no encontrado".to_string()))?;

        let enlace_cdr = limpio(fila.get(5).ok());
        let enlace_pdf: Option<String> = fila.get(6).ok();
        let enlace_xml = enlace_xml_de(fila.get(4).ok(), enlace_cdr.as_deref(), enlace_pdf.as_deref());
        (
            fila.get::<String>(0).unwrap_or_default(),
            fila.get::<String>(1).unwrap_or_default(),
            fila.get::<i64>(2).unwrap_or_default(),
            fila.get::<String>(3).unwrap_or_default(),
            enlace_xml,
            enlace_cdr,
            limpio(fila.get(7).ok()),
        )
    };

    if estado != "ACEPTADO" {
        return Err((
            StatusCode::CONFLICT,
            "Este comprobante no fue aceptado por SUNAT: no tiene XML ni constancia (CDR).".into(),
        ));
    }

    let (enlace, que) = match archivo {
        Archivo::Xml => (enlace_xml, "el XML"),
        Archivo::Cdr => (enlace_cdr, "la constancia de SUNAT (CDR)"),
    };
    let enlace = enlace.ok_or((StatusCode::NOT_FOUND, format!("Este comprobante no tiene {} disponible.", que)))?;

    let no_llego = || match archivo {
        Archivo::Xml => (StatusCode::BAD_GATEWAY, "FacturaLibre no entregó el XML. Intenta de nuevo en un momento.".to_string()),
        Archivo::Cdr => (
            StatusCode::BAD_GATEWAY,
            "La constancia de SUNAT (CDR) de este comprobante todavía no está disponible. Intenta más tarde.".to_string(),
        ),
    };

    let cliente = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let resp = cliente
        .get(&enlace)
        .send()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "No se pudo conectar con FacturaLibre. Intenta de nuevo en un momento.".to_string()))?;
    if !resp.status().is_success() {
        return Err(no_llego());
    }
    let bytes = resp.bytes().await.map_err(|_| no_llego())?;
    let (tipo_mime, extension) = tipo_de_contenido(&bytes).ok_or_else(no_llego)?;

    // Nombre con el que SUNAT identifica al comprobante:
    // RUC-tipo-serie-número (y "R-" delante para la constancia).
    let codigo_tipo = if tipo == "FACTURA" { "01" } else { "03" };
    let base = match ruc {
        Some(ruc) => format!("{}-{}-{}-{}", ruc, codigo_tipo, serie, numero),
        None => format!("{}-{}-{}", codigo_tipo, serie, numero),
    };
    let nombre = format!("{}{}.{}", if archivo == Archivo::Cdr { "R-" } else { "" }, base, extension);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, tipo_mime)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", nombre))
        .body(Body::from(bytes))
        .map(|r| r.into_response())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /comprobantes/:id/xml — XML firmado del comprobante.
pub async fn descargar_xml(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    descargar_archivo(tenant, id, Archivo::Xml).await
}

/// GET /comprobantes/:id/cdr — constancia de recepción de SUNAT.
pub async fn descargar_cdr(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    descargar_archivo(tenant, id, Archivo::Cdr).await
}
