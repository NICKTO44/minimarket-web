use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;
use crate::models::caja::{AbrirCajaRequest, CerrarCajaRequest, MovimientoCajaRequest, CajaResponse};
use crate::models::caja::CajaEstado;

pub async fn abrir_caja(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AbrirCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut rows = conn
        .query(
            "SELECT c.id, u.nombre_completo FROM cajas c
             JOIN usuarios u ON u.id = c.usuario_id
             WHERE c.estado = 'ABIERTA'",
            (),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        let cajero: String = row.get(1).unwrap_or_default();
        return Err((StatusCode::BAD_REQUEST, format!(
            "Ya hay una caja abierta en el sistema (cajero: {}). Debe cerrarse primero.", cajero
        )));
    }

    if payload.monto_inicial < 0.0 {
        return Err((StatusCode::BAD_REQUEST, "El monto inicial no puede ser negativo".into()));
    }

    conn.execute(
        "INSERT INTO cajas (usuario_id, numero_caja, turno, monto_inicial, observaciones_apertura, fecha_apertura, hora_apertura)
         VALUES (?1, ?2, 'GENERAL', ?3, ?4, datetime('now','localtime'), strftime('%H:%M:%S','now','localtime'))",
        libsql::params![payload.usuario_id, payload.numero_caja.unwrap_or(1), payload.monto_inicial, payload.observaciones.clone()],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al abrir caja: {}", e)))?;

    let caja_id = conn.last_insert_rowid();

    Ok(Json(CajaResponse {
        success: true,
        message: "Caja abierta exitosamente".into(),
        caja_id: Some(caja_id),
    }))
}

pub async fn cerrar_caja(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CerrarCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut rows = conn
        .query(
            "SELECT usuario_id, monto_inicial, ventas_efectivo, retiros_total, gastos_total, ingresos_total
             FROM cajas WHERE id = ?1 AND estado = 'ABIERTA'",
            libsql::params![payload.caja_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r,
        None => return Err((StatusCode::BAD_REQUEST, "Caja no encontrada o ya está cerrada".into())),
    };

    let caja_usuario_id: i64 = row.get(0).unwrap_or_default();
    let monto_inicial: f64 = row.get(1).unwrap_or_default();
    let ventas_efectivo: f64 = row.get(2).unwrap_or_default();
    let retiros_total: f64 = row.get(3).unwrap_or_default();
    let gastos_total: f64 = row.get(4).unwrap_or_default();
    let ingresos_total: f64 = row.get(5).unwrap_or_default();

    if caja_usuario_id != payload.usuario_id && payload.usuario_rol_id != 1 {
        return Err((StatusCode::FORBIDDEN, "Solo el cajero que abrió la caja o un administrador pueden cerrarla".into()));
    }

    let efectivo_esperado = monto_inicial + ventas_efectivo + ingresos_total - retiros_total - gastos_total;
    let diferencia = payload.monto_contado - efectivo_esperado;

    let estado_diferencia = if diferencia.abs() < 0.01 {
        "SIN_DIFERENCIA"
    } else if diferencia.abs() <= 10.0 {
        "ACEPTABLE"
    } else {
        "SIGNIFICATIVA"
    };

    conn.execute(
        "UPDATE cajas SET
            fecha_cierre = datetime('now','localtime'),
            hora_cierre = strftime('%H:%M:%S','now','localtime'),
            monto_final_contado = ?1,
            observaciones_cierre = ?2,
            efectivo_esperado = ?3,
            diferencia = ?4,
            estado_diferencia = ?5,
            justificacion_diferencia = ?6,
            estado = 'CERRADA'
         WHERE id = ?7",
        libsql::params![
            payload.monto_contado, payload.observaciones.clone(), efectivo_esperado,
            diferencia, estado_diferencia, payload.justificacion_diferencia.clone(), payload.caja_id
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al cerrar caja: {}", e)))?;

    Ok(Json(CajaResponse {
        success: true,
        message: "Caja cerrada exitosamente".into(),
        caja_id: Some(payload.caja_id),
    }))
}

pub async fn registrar_movimiento(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MovimientoCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !["RETIRO", "INGRESO", "GASTO"].contains(&payload.tipo.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Tipo de movimiento no válido".into()));
    }
    if payload.monto <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, "El monto debe ser mayor a 0".into()));
    }

    let mut rows = conn
        .query("SELECT id FROM cajas WHERE id = ?1 AND estado = 'ABIERTA'", libsql::params![payload.caja_id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_none() {
        return Err((StatusCode::BAD_REQUEST, "La caja no existe o ya está cerrada".into()));
    }

    conn.execute(
        "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        libsql::params![payload.caja_id, payload.tipo.clone(), payload.monto, payload.motivo.clone(), payload.usuario_id],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al registrar movimiento: {}", e)))?;

    let campo = match payload.tipo.as_str() {
        "RETIRO" => "retiros_total",
        "INGRESO" => "ingresos_total",
        _ => "gastos_total",
    };
    let query = format!("UPDATE cajas SET {} = {} + ?1 WHERE id = ?2", campo, campo);
    conn.execute(&query, libsql::params![payload.monto, payload.caja_id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar caja: {}", e)))?;

    Ok(Json(CajaResponse {
        success: true,
        message: "Movimiento registrado".into(),
        caja_id: Some(payload.caja_id),
    }))
}

pub async fn obtener_caja_abierta(
    State(state): State<Arc<AppState>>,
) -> Json<Option<CajaEstado>> {
    let conn = match state.db.connect() {
        Ok(c) => c,
        Err(_) => return Json(None),
    };

    let mut rows = match conn
        .query(
            "SELECT c.id, u.nombre_completo, c.monto_inicial, c.ventas_efectivo,
                    c.ventas_tarjeta, c.ventas_transferencia, c.total_ventas, c.numero_transacciones,
                    c.devoluciones_monto, c.retiros_total, c.ingresos_total, c.gastos_total, c.fecha_apertura
             FROM cajas c JOIN usuarios u ON u.id = c.usuario_id
             WHERE c.estado = 'ABIERTA'",
            (),
        )
        .await
    {
        Ok(r) => r,
        Err(_) => return Json(None),
    };

    match rows.next().await {
        Ok(Some(row)) => Json(Some(CajaEstado {
            id: row.get(0).unwrap_or_default(),
            usuario_nombre: row.get(1).unwrap_or_default(),
            monto_inicial: row.get(2).unwrap_or_default(),
            ventas_efectivo: row.get(3).unwrap_or_default(),
            ventas_tarjeta: row.get(4).unwrap_or_default(),
            ventas_transferencia: row.get(5).unwrap_or_default(),
            total_ventas: row.get(6).unwrap_or_default(),
            numero_transacciones: row.get(7).unwrap_or_default(),
            devoluciones_monto: row.get(8).unwrap_or_default(),
            retiros_total: row.get(9).unwrap_or_default(),
            ingresos_total: row.get(10).unwrap_or_default(),
            gastos_total: row.get(11).unwrap_or_default(),
            fecha_apertura: row.get(12).unwrap_or_default(),
        })),
        _ => Json(None),
    }
}

#[derive(Deserialize)]
pub struct RangoFechas {
    pub fecha_inicio: String,
    pub fecha_fin: String,
}

pub async fn listar_cajas(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RangoFechas>,
) -> Result<Json<Vec<crate::models::caja::CajaHistorial>>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT c.id, u.nombre_completo, c.fecha_apertura, c.fecha_cierre, c.estado,
                    c.monto_inicial, c.monto_final_contado, c.diferencia, c.total_ventas,
                    c.ventas_efectivo, c.ventas_tarjeta, c.ventas_transferencia,
                    c.numero_transacciones, c.devoluciones_monto
             FROM cajas c JOIN usuarios u ON u.id = c.usuario_id
             WHERE date(c.fecha_apertura) BETWEEN ?1 AND ?2
             ORDER BY c.fecha_apertura DESC",
            libsql::params![params.fecha_inicio, params.fecha_fin],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut cajas = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        cajas.push(crate::models::caja::CajaHistorial {
            id: row.get(0).unwrap_or_default(),
            usuario_nombre: row.get(1).unwrap_or_default(),
            fecha_apertura: row.get(2).unwrap_or_default(),
            fecha_cierre: row.get(3).ok(),
            estado: row.get(4).unwrap_or_default(),
            monto_inicial: row.get(5).unwrap_or_default(),
            monto_contado: row.get(6).ok(),
            diferencia: row.get(7).ok(),
            total_ventas: row.get(8).unwrap_or_default(),
            ventas_efectivo: row.get(9).unwrap_or_default(),
            ventas_tarjeta: row.get(10).unwrap_or_default(),
            ventas_transferencia: row.get(11).unwrap_or_default(),
            numero_transacciones: row.get(12).unwrap_or_default(),
            devoluciones_monto: row.get(13).unwrap_or_default(),
        });
    }

    Ok(Json(cajas))
}