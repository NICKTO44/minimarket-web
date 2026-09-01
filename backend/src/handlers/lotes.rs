use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;

use crate::tenants::TenantDb;
use crate::models::lote::{Lote, NuevoLote, LoteAlerta, LoteAccionResponse};

pub async fn agregar_lote(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoLote>,
) -> Result<Json<Lote>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    conn.execute(
        "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, numero_lote) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![payload.producto_id, payload.cantidad, payload.fecha_vencimiento.clone(), payload.numero_lote.clone()],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = conn.last_insert_rowid();

    Ok(Json(Lote {
        id,
        producto_id: payload.producto_id,
        cantidad: payload.cantidad,
        fecha_vencimiento: payload.fecha_vencimiento,
        numero_lote: payload.numero_lote,
        activo: true,
    }))
}

pub async fn obtener_lotes_de_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(producto_id): Path<i64>,
) -> Result<Json<Vec<Lote>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

      let mut rows = conn
        .query(
            "SELECT id, producto_id, cantidad, fecha_vencimiento, numero_lote, activo
             FROM lotes_producto WHERE producto_id = ?1 AND activo = 1 AND cantidad > 0
             ORDER BY fecha_vencimiento ASC",
            libsql::params![producto_id],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut lotes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        lotes.push(Lote {
            id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
            cantidad: row.get(2).unwrap_or_default(),
            fecha_vencimiento: row.get(3).unwrap_or_default(),
            numero_lote: row.get(4).ok(),
            activo: row.get::<i64>(5).unwrap_or(1) == 1,
        });
    }

    Ok(Json(lotes))
}

#[derive(Deserialize)]
pub struct HorizonteQuery {
    pub dias: Option<i64>,
}

// Trae todos los lotes de TODOS los productos que vencen dentro del
// horizonte indicado (o ya vencidos, con dias_restantes negativo) —
// misma lógica que Lubricentro, pero a nivel de todo el negocio, no
// de un solo producto. Por defecto, 15 días de horizonte.
pub async fn lotes_por_vencer(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(params): Query<HorizonteQuery>,
) -> Result<Json<Vec<LoteAlerta>>, StatusCode> {
    let dias_horizonte = params.dias.unwrap_or(15);
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT l.id, l.producto_id, p.nombre, l.cantidad, p.unidad_medida, l.fecha_vencimiento,
                    CAST(julianday(l.fecha_vencimiento) - julianday('now','localtime') AS INTEGER)
             FROM lotes_producto l
             JOIN productos p ON p.id = l.producto_id
             WHERE l.activo = 1 AND l.cantidad > 0
               AND julianday(l.fecha_vencimiento) - julianday('now','localtime') <= ?1
             ORDER BY l.fecha_vencimiento ASC",
            libsql::params![dias_horizonte],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut alertas = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        alertas.push(LoteAlerta {
            lote_id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
            producto_nombre: row.get(2).unwrap_or_default(),
            cantidad: row.get(3).unwrap_or_default(),
            unidad_medida: row.get(4).unwrap_or_default(),
            fecha_vencimiento: row.get(5).unwrap_or_default(),
            dias_restantes: row.get(6).unwrap_or_default(),
        });
    }

    Ok(Json(alertas))
}

pub async fn descartar_lote(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(lote_id): Path<i64>,
) -> Result<Json<LoteAccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE lotes_producto SET activo = 0, cantidad = 0 WHERE id = ?1",
        libsql::params![lote_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al descartar: {}", e)))?;

    Ok(Json(LoteAccionResponse {
        success: true,
        message: "Lote descartado".into(),
    }))
}