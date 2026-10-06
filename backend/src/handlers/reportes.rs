use axum::{extract::{Extension, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::reporte::*;

// Las fechas de la base están en UTC (así las guarda el servidor). Los
// reportes cortan el día en hora de Perú (UTC-5): una venta de las 8 p. m.
// pertenece a ese día, no al siguiente.

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

    // El método sale con Yape y Plin por separado (migración 0019); si la
    // base aún no tiene la columna, como siempre ("YAPE_PLIN").
    let consulta = |metodo: &str, otro: &str| {
        format!(
            "SELECT v.id, v.folio, v.fecha_hora, v.total, {}, u.nombre_completo, v.estado,
                    v.pago_efectivo, v.pago_otro, {}
             FROM ventas v JOIN usuarios u ON v.usuario_id = u.id
             WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2
             ORDER BY v.fecha_hora DESC",
            metodo, otro
        )
    };
    let mut rows = match conn
        .query(
            &consulta(
                "CASE WHEN v.metodo_pago = 'YAPE_PLIN' THEN COALESCE(v.billetera, 'YAPE_PLIN') ELSE v.metodo_pago END",
                "CASE WHEN v.pago_otro_metodo = 'YAPE_PLIN' THEN COALESCE(v.billetera, 'YAPE_PLIN') ELSE v.pago_otro_metodo END",
            ),
            libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
        )
        .await
    {
        Ok(r) => r,
        Err(_) => conn
            .query(&consulta("v.metodo_pago", "v.pago_otro_metodo"), libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()])
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };

    let mut ventas = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        ventas.push(VentaResumen {
            id: row.get(0).unwrap_or_default(),
            folio: row.get(1).unwrap_or_default(),
            fecha_hora: row.get(2).unwrap_or_default(),
            total: row.get(3).unwrap_or_default(),
            metodo_pago: row.get(4).unwrap_or_default(),
            pago_efectivo: row.get::<Option<f64>>(7).ok().flatten(),
            pago_otro: row.get::<Option<f64>>(8).ok().flatten(),
            pago_otro_metodo: row.get::<Option<String>>(9).ok().flatten(),
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
             WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA'
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
         FROM ventas WHERE date(fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND estado = 'COMPLETADA'",
        libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (ventas_cantidad, ventas_total, ticket_promedio) = match r1.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => (row.get(0).unwrap_or(0), row.get(1).unwrap_or(0.0), row.get(2).unwrap_or(0.0)),
        None => (0, 0.0, 0.0),
    };

    let mut r2 = conn.query(
        "SELECT COUNT(*), COALESCE(SUM(monto_reembolsado),0.0)
         FROM devoluciones WHERE date(fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND estado = 'PROCESADA'",
        libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (devoluciones_cantidad, devoluciones_total) = match r2.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => (row.get(0).unwrap_or(0), row.get(1).unwrap_or(0.0)),
        None => (0, 0.0),
    };

    let por_metodo = pagos_por_metodo(&conn, &rango).await;

    Ok(Json(EstadisticasCompletas {
        ventas_cantidad,
        ventas_total,
        ticket_promedio,
        devoluciones_cantidad,
        devoluciones_total,
        total_neto: ventas_total - devoluciones_total,
        por_metodo,
    }))
}

/// Lo vendido en el período repartido por medio de pago. Las ventas mixtas
/// se parten: su efectivo va a Efectivo y el resto a su otro medio (o a
/// "CREDITO" si fue al crédito). Yape y Plin salen por separado; las ventas
/// de antes del cambio quedan como "YAPE_PLIN". Es un detalle del reporte:
/// si algo falla, va vacío y el resto del reporte sale igual.
async fn pagos_por_metodo(conn: &libsql::Connection, rango: &RangoFechas) -> Vec<PagoPorMetodo> {
    let consulta = |billetera: &str| {
        format!(
            "SELECT metodo, COUNT(*), CAST(SUM(monto) AS REAL) FROM (
                 SELECT CASE WHEN v.metodo_pago = 'YAPE_PLIN' THEN {b} ELSE v.metodo_pago END AS metodo, v.total AS monto
                   FROM ventas v
                  WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA' AND v.metodo_pago <> 'MIXTO'
                 UNION ALL
                 SELECT 'EFECTIVO', v.pago_efectivo
                   FROM ventas v
                  WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA' AND v.metodo_pago = 'MIXTO'
                    AND COALESCE(v.pago_efectivo, 0) > 0
                 UNION ALL
                 SELECT CASE WHEN v.pago_otro_metodo = 'YAPE_PLIN' THEN {b} ELSE COALESCE(v.pago_otro_metodo, 'MIXTO') END, v.pago_otro
                   FROM ventas v
                  WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA' AND v.metodo_pago = 'MIXTO'
                    AND COALESCE(v.pago_otro, 0) > 0
             ) GROUP BY metodo ORDER BY SUM(monto) DESC",
            b = billetera
        )
    };
    let parametros = || libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()];
    let mut filas = match conn.query(&consulta("COALESCE(v.billetera, 'YAPE_PLIN')"), parametros()).await {
        Ok(f) => f,
        // Base que aún no tiene la columna de la billetera.
        Err(_) => match conn.query(&consulta("'YAPE_PLIN'"), parametros()).await {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        },
    };
    let mut lista = Vec::new();
    while let Ok(Some(f)) = filas.next().await {
        lista.push(PagoPorMetodo {
            metodo: f.get(0).unwrap_or_default(),
            cantidad: f.get(1).unwrap_or(0),
            monto: f.get(2).unwrap_or(0.0),
        });
    }
    lista
}