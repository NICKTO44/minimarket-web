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

    let mut sql = String::from(
        "SELECT ce.id, v.id, v.folio, COALESCE(ce.tipo, 'NINGUNO'), ce.serie, ce.numero,
                c.nombre_razon_social, v.total, ce.estado, v.fecha_hora, ce.mensaje_sunat, ce.enlace_pdf
         FROM ventas v
         LEFT JOIN comprobantes_electronicos ce ON ce.venta_id = v.id
         LEFT JOIN clientes c ON c.id = v.cliente_id
         WHERE v.estado = 'COMPLETADA'",
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

    let mut rows = match (&filtros.tipo, &filtros.estado) {
        (Some(t), Some(e)) => conn.query(&sql, libsql::params![t.clone(), e.clone()]).await,
        (Some(t), None) => conn.query(&sql, libsql::params![t.clone()]).await,
        (None, Some(e)) => conn.query(&sql, libsql::params![e.clone()]).await,
        (None, None) => conn.query(&sql, ()).await,
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut comprobantes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
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
            enlace_pdf: row.get(11).ok(),
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