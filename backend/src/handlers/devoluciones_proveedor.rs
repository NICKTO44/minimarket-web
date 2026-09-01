use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use std::sync::Arc;
use chrono::Local;
use serde::Deserialize;

use crate::tenants::TenantDb;
use crate::models::proveedor::*;

pub async fn registrar_devolucion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<RegistrarDevolucionProveedorRequest>,
) -> Result<Json<DevolucionProveedorResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r_estado = conn.query("SELECT estado, proveedor_id FROM compras WHERE id = ?1", libsql::params![payload.compra_id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (estado_compra, proveedor_id): (String, i64) = match r_estado.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => (row.get(0).unwrap_or_default(), row.get(1).unwrap_or_default()),
        None => return Err((StatusCode::NOT_FOUND, "Compra no encontrada".into())),
    };

    if estado_compra != "RECIBIDA" && estado_compra != "PARCIAL" {
        return Err((StatusCode::BAD_REQUEST, format!(
            "Solo se puede devolver de compras RECIBIDA o PARCIAL. Estado actual: {}", estado_compra
        )));
    }

    if payload.items.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Debes indicar al menos un producto a devolver".into()));
    }

    let monto_total: f64 = payload.items.iter().map(|i| i.precio_compra * i.cantidad_devuelta).sum();

    let fecha = Local::now().format("%Y%m%d").to_string();
    let query_folio = format!(
        "SELECT COALESCE(MAX(CAST(substr(folio,-4) AS INTEGER)),0)+1 FROM devoluciones_proveedor WHERE folio LIKE 'DP-{}%'", fecha
    );
    let mut rf = conn.query(&query_folio, ()).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let siguiente: i64 = match rf.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(1), None => 1,
    };
    let folio = format!("DP-{}-{:04}", fecha, siguiente);

    conn.execute(
        "INSERT INTO devoluciones_proveedor (compra_id, proveedor_id, folio, motivo, detalle_motivo, monto_devolucion, estado, usuario_id, notas)
         VALUES (?1,?2,?3,?4,?5,?6,'PENDIENTE',?7,?8)",
        libsql::params![
            payload.compra_id, proveedor_id, folio.clone(), payload.motivo.clone(),
            payload.detalle_motivo.clone(), monto_total, payload.usuario_id, payload.notas.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al registrar devolución: {}", e)))?;

    let devolucion_id = conn.last_insert_rowid();

    for item in &payload.items {
        let subtotal_item = item.precio_compra * item.cantidad_devuelta;
        conn.execute(
            "INSERT INTO detalles_devolucion_proveedor (devolucion_proveedor_id, detalle_compra_id, producto_id, cantidad_devuelta, precio_compra, subtotal, motivo_item)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            libsql::params![
                devolucion_id, item.detalle_compra_id, item.producto_id,
                item.cantidad_devuelta, item.precio_compra, subtotal_item, item.motivo_item.clone()
            ],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar item: {}", e)))?;
    }

    Ok(Json(DevolucionProveedorResponse {
        success: true,
        message: format!("Devolución {} registrada por S/ {:.2}. Queda PENDIENTE hasta que resuelvas con el proveedor.", folio, monto_total),
        devolucion_id: Some(devolucion_id),
        folio: Some(folio),
        credito_disponible: None,
    }))
}

pub async fn resolver_devolucion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(payload): Json<ResolverDevolucionProveedorRequest>,
) -> Result<Json<DevolucionProveedorResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r1 = conn.query("SELECT estado FROM devoluciones_proveedor WHERE id = ?1", libsql::params![id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let estado_actual: String = match r1.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or_default(),
        None => return Err((StatusCode::NOT_FOUND, "Devolución no encontrada".into())),
    };

    if estado_actual != "PENDIENTE" {
        return Err((StatusCode::BAD_REQUEST, format!("Esta devolución ya fue resuelta (estado: {})", estado_actual)));
    }

    if payload.estado == "ACEPTADA" && payload.tipo_resolucion.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Debes indicar el tipo de resolución (CREDITO, REEMBOLSO o CAMBIO)".into()));
    }

    conn.execute(
        "UPDATE devoluciones_proveedor SET estado = ?1, tipo_resolucion = ?2, notas = COALESCE(?3, notas), fecha_resolucion = datetime('now','localtime') WHERE id = ?4",
        libsql::params![payload.estado.clone(), payload.tipo_resolucion.clone(), payload.notas.clone(), id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al resolver: {}", e)))?;

    let mut r2 = conn.query(
        "SELECT p.credito_disponible FROM proveedores p JOIN devoluciones_proveedor d ON d.proveedor_id = p.id WHERE d.id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let credito: f64 = match r2.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(0.0), None => 0.0,
    };

    let message = match payload.estado.as_str() {
        "ACEPTADA" => match payload.tipo_resolucion.as_deref() {
            Some("CREDITO") => format!("Devolución aceptada. Crédito disponible actualizado: S/ {:.2}.", credito),
            Some("REEMBOLSO") => "Devolución aceptada. El proveedor realizará el reembolso.".into(),
            Some("CAMBIO") => "Devolución aceptada. El proveedor enviará mercadería de reemplazo.".into(),
            _ => "Devolución aceptada.".into(),
        },
        "RECHAZADA" => "Devolución rechazada por el proveedor.".into(),
        _ => "Estado actualizado.".into(),
    };

    Ok(Json(DevolucionProveedorResponse {
        success: true,
        message,
        devolucion_id: Some(id),
        folio: None,
        credito_disponible: Some(credito),
    }))
}

#[derive(Deserialize)]
pub struct FiltroCompra {
    pub compra_id: Option<i64>,
}

pub async fn listar_devoluciones(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(params): Query<FiltroCompra>,
) -> Result<Json<Vec<DevolucionProveedorResumen>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (query, filtra) = match params.compra_id {
        Some(_) => (
            "SELECT d.id, d.compra_id, p.nombre, d.folio, d.fecha, d.motivo, d.monto_devolucion, d.estado, d.tipo_resolucion
             FROM devoluciones_proveedor d JOIN proveedores p ON d.proveedor_id = p.id
             WHERE d.compra_id = ?1 ORDER BY d.fecha DESC",
            true,
        ),
        None => (
            "SELECT d.id, d.compra_id, p.nombre, d.folio, d.fecha, d.motivo, d.monto_devolucion, d.estado, d.tipo_resolucion
             FROM devoluciones_proveedor d JOIN proveedores p ON d.proveedor_id = p.id
             ORDER BY d.fecha DESC LIMIT 50",
            false,
        ),
    };

    let mut rows = if filtra {
        conn.query(query, libsql::params![params.compra_id.unwrap()]).await
    } else {
        conn.query(query, ()).await
    }.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut devoluciones = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        devoluciones.push(DevolucionProveedorResumen {
            id: row.get(0).unwrap_or_default(),
            compra_id: row.get(1).unwrap_or_default(),
            proveedor_nombre: row.get(2).unwrap_or_default(),
            folio: row.get(3).unwrap_or_default(),
            fecha: row.get(4).unwrap_or_default(),
            motivo: row.get(5).unwrap_or_default(),
            monto_devolucion: row.get(6).unwrap_or_default(),
            estado: row.get(7).unwrap_or_default(),
            tipo_resolucion: row.get(8).ok(),
        });
    }

    Ok(Json(devoluciones))
}