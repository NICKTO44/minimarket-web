use axum::{extract::{Extension, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::reporte::*;

#[derive(Deserialize)]
pub struct RangoFechas {
    pub fecha_inicio: String,
    pub fecha_fin: String,
}

pub async fn ventas_por_rango(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(rango): Query<RangoFechas>,
) -> Result<Json<Vec<VentaResumen>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT v.id, v.folio, v.fecha_hora, v.total, v.metodo_pago, u.nombre_completo, v.estado
             FROM ventas v JOIN usuarios u ON v.usuario_id = u.id
             WHERE date(v.fecha_hora) BETWEEN ?1 AND ?2
             ORDER BY v.fecha_hora DESC",
            libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut ventas = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        ventas.push(VentaResumen {
            id: row.get(0).unwrap_or_default(),
            folio: row.get(1).unwrap_or_default(),
            fecha_hora: row.get(2).unwrap_or_default(),
            total: row.get(3).unwrap_or_default(),
            metodo_pago: row.get(4).unwrap_or_default(),
            cajero: row.get(5).unwrap_or_default(),
            estado: row.get(6).unwrap_or_default(),
        });
    }

    Ok(Json(ventas))
}

pub async fn productos_mas_vendidos(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(rango): Query<RangoFechas>,
) -> Result<Json<Vec<ProductoVendido>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT p.nombre, SUM(dv.cantidad), SUM(dv.total_linea)
             FROM detalles_venta dv
             JOIN productos p ON dv.producto_id = p.id
             JOIN ventas v ON dv.venta_id = v.id
             WHERE date(v.fecha_hora) BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA'
             GROUP BY p.id, p.nombre
             ORDER BY SUM(dv.cantidad) DESC
             LIMIT 10",
            libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut productos = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        productos.push(ProductoVendido {
            producto_nombre: row.get(0).unwrap_or_default(),
            cantidad_vendida: row.get(1).unwrap_or_default(),
            total_vendido: row.get(2).unwrap_or_default(),
        });
    }

    Ok(Json(productos))
}

pub async fn estadisticas_completas(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(rango): Query<RangoFechas>,
) -> Result<Json<EstadisticasCompletas>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut r1 = conn.query(
        "SELECT COUNT(*), COALESCE(SUM(total),0.0), COALESCE(AVG(total),0.0)
         FROM ventas WHERE date(fecha_hora) BETWEEN ?1 AND ?2 AND estado = 'COMPLETADA'",
        libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (ventas_cantidad, ventas_total, ticket_promedio) = match r1.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => (row.get(0).unwrap_or(0), row.get(1).unwrap_or(0.0), row.get(2).unwrap_or(0.0)),
        None => (0, 0.0, 0.0),
    };

    let mut r2 = conn.query(
        "SELECT COUNT(*), COALESCE(SUM(monto_reembolsado),0.0)
         FROM devoluciones WHERE date(fecha_hora) BETWEEN ?1 AND ?2 AND estado = 'PROCESADA'",
        libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (devoluciones_cantidad, devoluciones_total) = match r2.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => (row.get(0).unwrap_or(0), row.get(1).unwrap_or(0.0)),
        None => (0, 0.0),
    };

    Ok(Json(EstadisticasCompletas {
        ventas_cantidad,
        ventas_total,
        ticket_promedio,
        devoluciones_cantidad,
        devoluciones_total,
        total_neto: ventas_total - devoluciones_total,
    }))
}