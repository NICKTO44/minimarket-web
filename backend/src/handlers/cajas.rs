use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::auth::Claims;
use crate::middleware_auth::ROL_ADMIN;
use crate::models::caja::{AbrirCajaRequest, CerrarCajaRequest, MovimientoCajaRequest, CajaResponse};
use crate::models::caja::CajaEstado;

pub async fn abrir_caja(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<AbrirCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if crate::handlers::mesas::es_mesero(&conn, claims.rol_id).await {
        return Err((StatusCode::FORBIDDEN, "Este usuario no maneja caja (mesero o barra/cocina).".into()));
    }

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
        libsql::params![claims.sub, payload.numero_caja.unwrap_or(1), payload.monto_inicial, payload.observaciones.clone()],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al abrir caja: {}", e)))?;

    let caja_id = conn.last_insert_rowid();

    // Esta caja nace con Yape y Plin ya separados (migración 0019). Si la
    // base aún no tiene la columna, no pasa nada.
    let _ = conn.execute("UPDATE cajas SET detalle_billeteras = 1 WHERE id = ?1", libsql::params![caja_id]).await;

    Ok(Json(CajaResponse {
        success: true,
        message: "Caja abierta exitosamente".into(),
        caja_id: Some(caja_id),
    }))
}

pub async fn cerrar_caja(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CerrarCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    // Quién cierra y con qué rol sale del JWT, no del navegador.
    if caja_usuario_id != claims.sub && claims.rol_id != ROL_ADMIN {
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

/// Efectivo que debería haber ahora en la caja (el mismo cálculo del
/// cierre): monto inicial + ventas en efectivo + ingresos − retiros − gastos.
pub async fn efectivo_en_caja(conn: &libsql::Connection, caja_id: i64) -> Result<f64, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT CAST(COALESCE(monto_inicial, 0) + COALESCE(ventas_efectivo, 0) + COALESCE(ingresos_total, 0)
                         - COALESCE(retiros_total, 0) - COALESCE(gastos_total, 0) AS REAL)
             FROM cajas WHERE id = ?1",
            libsql::params![caja_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(f) => Ok(f.get::<f64>(0).unwrap_or(0.0)),
        None => Err((StatusCode::BAD_REQUEST, "La caja no existe".into())),
    }
}

/// No se puede sacar de la caja más efectivo del que hay: el efectivo
/// esperado nunca queda negativo. Lo que se pagó con plata de otro lado
/// (o por Yape) se registra en Gastos con esa forma de pago.
pub fn alcanza_efectivo(disponible: f64, monto: f64) -> Result<(), String> {
    if monto > disponible + 0.005 {
        let hay = disponible.max(0.0);
        return Err(format!(
            "En la caja solo hay S/ {:.2} en efectivo y quieres sacar S/ {:.2}. Si lo pagaste con plata de otro lado o por Yape/Plin, regístralo en Gastos con esa forma de pago.",
            hay, monto
        ));
    }
    Ok(())
}

pub async fn registrar_movimiento(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<MovimientoCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    drop(rows);

    if payload.tipo != "INGRESO" {
        let disponible = efectivo_en_caja(&conn, payload.caja_id).await?;
        alcanza_efectivo(disponible, payload.monto).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    conn.execute(
        "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        libsql::params![payload.caja_id, payload.tipo.clone(), payload.monto, payload.motivo.clone(), claims.sub],
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

#[derive(serde::Serialize)]
pub struct MovimientoCaja {
    pub id: i64,
    /// RETIRO | INGRESO | GASTO
    pub tipo: String,
    pub monto: f64,
    pub motivo: String,
    pub hora: String,
    pub usuario: String,
    /// Si es un gasto registrado en el módulo Gastos, su número.
    pub gasto_id: Option<i64>,
}

/// GET /cajas/movimientos — gastos, retiros e ingresos de la caja abierta,
/// del más reciente al más antiguo.
pub async fn movimientos_caja_abierta(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<MovimientoCaja>>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // La base (Turso) guarda la hora en UTC: se muestra la de Perú.
    const COLUMNAS: &str = "m.id, m.tipo, CAST(m.monto AS REAL), m.motivo,
         COALESCE(strftime('%H:%M', m.fecha_hora, '-5 hours'), ''), COALESCE(u.nombre_completo, '')";
    const DESDE: &str = "FROM movimientos_caja m
         JOIN cajas c ON c.id = m.caja_id AND c.estado = 'ABIERTA'
         LEFT JOIN usuarios u ON u.id = m.usuario_id";
    // Con el número de gasto (migración 0022); sin ella, la consulta simple.
    let mut filas = match conn
        .query(
            &format!(
                "SELECT {}, (SELECT g.id FROM gastos g WHERE g.movimiento_caja_id = m.id) {} ORDER BY m.id DESC",
                COLUMNAS, DESDE
            ),
            (),
        )
        .await
    {
        Ok(f) => f,
        Err(_) => conn
            .query(&format!("SELECT {}, NULL {} ORDER BY m.id DESC", COLUMNAS, DESDE), ())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    };
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        lista.push(MovimientoCaja {
            id: f.get(0).unwrap_or_default(),
            tipo: f.get(1).unwrap_or_default(),
            monto: f.get(2).unwrap_or(0.0),
            motivo: f.get(3).unwrap_or_default(),
            hora: f.get(4).unwrap_or_default(),
            usuario: f.get(5).unwrap_or_default(),
            gasto_id: f.get(6).unwrap_or(None),
        });
    }
    Ok(Json(lista))
}

/// Columnas de la migración 0019, al final de cada consulta de cajas.
const BILLETERAS: &str = "CAST(c.ventas_yape AS REAL), CAST(c.ventas_plin AS REAL), CAST(c.ventas_yape_plin AS REAL), c.detalle_billeteras";

pub async fn obtener_caja_abierta(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Json<Option<CajaEstado>> {
    let conn = match tenant.0.connect() {
        Ok(c) => c,
        Err(_) => return Json(None),
    };

    // Con Yape y Plin por separado (migración 0019); si la base aún no tiene
    // esas columnas, la consulta de siempre (y van en cero).
    const COLUMNAS: &str = "c.id, u.nombre_completo, c.monto_inicial, c.ventas_efectivo,
                    c.ventas_tarjeta, c.ventas_transferencia, c.total_ventas, c.numero_transacciones,
                    c.devoluciones_monto, c.retiros_total, c.ingresos_total, c.gastos_total, c.fecha_apertura";
    const DESDE: &str = "FROM cajas c JOIN usuarios u ON u.id = c.usuario_id WHERE c.estado = 'ABIERTA'";
    let con_billeteras = format!("SELECT {}, {} {}", COLUMNAS, BILLETERAS, DESDE);
    let mut rows = match conn.query(&con_billeteras, ()).await {
        Ok(r) => r,
        Err(_) => match conn.query(&format!("SELECT {} {}", COLUMNAS, DESDE), ()).await {
            Ok(r) => r,
            Err(_) => return Json(None),
        },
    };

    match rows.next().await {
        Ok(Some(row)) => Json(Some(CajaEstado {
            id: row.get(0).unwrap_or_default(),
            usuario_nombre: row.get(1).unwrap_or_default(),
            monto_inicial: row.get(2).unwrap_or_default(),
            ventas_efectivo: row.get(3).unwrap_or_default(),
            ventas_tarjeta: row.get(4).unwrap_or_default(),
            ventas_transferencia: row.get(5).unwrap_or_default(),
            ventas_yape: row.get(13).unwrap_or_default(),
            ventas_plin: row.get(14).unwrap_or_default(),
            ventas_yape_plin: row.get(15).unwrap_or_default(),
            detalle_billeteras: row.get::<i64>(16).unwrap_or(0) == 1,
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
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(params): Query<RangoFechas>,
) -> Result<Json<Vec<crate::models::caja::CajaHistorial>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    const COLUMNAS: &str = "c.id, u.nombre_completo, c.fecha_apertura, c.fecha_cierre, c.estado,
                    c.monto_inicial, c.monto_final_contado, c.diferencia, c.total_ventas,
                    c.ventas_efectivo, c.ventas_tarjeta, c.ventas_transferencia,
                    c.numero_transacciones, c.devoluciones_monto";
    const DESDE: &str = "FROM cajas c JOIN usuarios u ON u.id = c.usuario_id
             WHERE date(c.fecha_apertura) BETWEEN ?1 AND ?2
             ORDER BY c.fecha_apertura DESC";
    // Con Yape y Plin por separado; si la base aún no tiene esas columnas,
    // la consulta de siempre.
    let mut rows = match conn
        .query(
            &format!("SELECT {}, {} {}", COLUMNAS, BILLETERAS, DESDE),
            libsql::params![params.fecha_inicio.clone(), params.fecha_fin.clone()],
        )
        .await
    {
        Ok(r) => r,
        Err(_) => conn
            .query(&format!("SELECT {} {}", COLUMNAS, DESDE), libsql::params![params.fecha_inicio, params.fecha_fin])
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };

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
            ventas_yape: row.get(14).unwrap_or_default(),
            ventas_plin: row.get(15).unwrap_or_default(),
            ventas_yape_plin: row.get(16).unwrap_or_default(),
            detalle_billeteras: row.get::<i64>(17).unwrap_or(0) == 1,
            numero_transacciones: row.get(12).unwrap_or_default(),
            devoluciones_monto: row.get(13).unwrap_or_default(),
        });
    }

    Ok(Json(cajas))
}

#[derive(serde::Serialize)]
pub struct DetalleCaja {
    /// Los mismos datos que la caja abierta (ventas por forma de pago,
    /// ingresos, retiros, gastos...).
    #[serde(flatten)]
    pub caja: CajaEstado,
    pub estado: String,
    pub fecha_cierre: Option<String>,
    pub observaciones_apertura: Option<String>,
    pub observaciones_cierre: Option<String>,
    /// Del cierre: lo que debía haber, lo que se contó y la diferencia.
    pub efectivo_esperado: Option<f64>,
    pub monto_contado: Option<f64>,
    pub diferencia: Option<f64>,
    pub estado_diferencia: Option<String>,
    pub justificacion_diferencia: Option<String>,
    pub movimientos: Vec<MovimientoCaja>,
}

fn e500<E: ToString>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// GET /cajas/:id/detalle — una caja (abierta o cerrada) con todos sus
/// totales, el cuadre del cierre y sus gastos, retiros e ingresos.
pub async fn detalle_caja(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<DetalleCaja>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(e500)?;

    const COLUMNAS: &str = "c.id, COALESCE(u.nombre_completo, ''), CAST(COALESCE(c.monto_inicial, 0) AS REAL),
        CAST(COALESCE(c.ventas_efectivo, 0) AS REAL), CAST(COALESCE(c.ventas_tarjeta, 0) AS REAL),
        CAST(COALESCE(c.ventas_transferencia, 0) AS REAL), CAST(COALESCE(c.total_ventas, 0) AS REAL),
        COALESCE(c.numero_transacciones, 0), CAST(COALESCE(c.devoluciones_monto, 0) AS REAL),
        CAST(COALESCE(c.retiros_total, 0) AS REAL), CAST(COALESCE(c.ingresos_total, 0) AS REAL),
        CAST(COALESCE(c.gastos_total, 0) AS REAL), c.fecha_apertura,
        c.estado, c.fecha_cierre, c.observaciones_apertura, c.observaciones_cierre,
        CAST(c.efectivo_esperado AS REAL), CAST(c.monto_final_contado AS REAL), CAST(c.diferencia AS REAL),
        c.estado_diferencia, c.justificacion_diferencia";
    const DESDE: &str = "FROM cajas c LEFT JOIN usuarios u ON u.id = c.usuario_id WHERE c.id = ?1";
    // Con Yape y Plin por separado (columnas 22 a 25); sin la migración 0019,
    // la consulta de siempre.
    let (mut filas, con_billeteras) = match conn
        .query(&format!("SELECT {}, {} {}", COLUMNAS, BILLETERAS, DESDE), libsql::params![id])
        .await
    {
        Ok(f) => (f, true),
        Err(_) => (
            conn.query(&format!("SELECT {} {}", COLUMNAS, DESDE), libsql::params![id]).await.map_err(e500)?,
            false,
        ),
    };
    let f = filas
        .next()
        .await
        .map_err(e500)?
        .ok_or((StatusCode::NOT_FOUND, "Esa caja no existe.".to_string()))?;
    let real = |i: i32| f.get::<f64>(i).unwrap_or(0.0);
    let opcional_real = |i: i32| f.get::<Option<f64>>(i).unwrap_or(None);
    let opcional_texto = |i: i32| f.get::<Option<String>>(i).unwrap_or(None).filter(|t| !t.trim().is_empty());
    let caja = CajaEstado {
        id: f.get(0).unwrap_or_default(),
        usuario_nombre: f.get(1).unwrap_or_default(),
        monto_inicial: real(2),
        ventas_efectivo: real(3),
        ventas_tarjeta: real(4),
        ventas_transferencia: real(5),
        total_ventas: real(6),
        numero_transacciones: f.get(7).unwrap_or_default(),
        devoluciones_monto: real(8),
        retiros_total: real(9),
        ingresos_total: real(10),
        gastos_total: real(11),
        fecha_apertura: f.get(12).unwrap_or_default(),
        ventas_yape: if con_billeteras { real(22) } else { 0.0 },
        ventas_plin: if con_billeteras { real(23) } else { 0.0 },
        ventas_yape_plin: if con_billeteras { real(24) } else { 0.0 },
        detalle_billeteras: con_billeteras && f.get::<i64>(25).unwrap_or(0) == 1,
    };
    let estado: String = f.get(13).unwrap_or_default();
    let fecha_cierre = opcional_texto(14);
    let observaciones_apertura = opcional_texto(15);
    let observaciones_cierre = opcional_texto(16);
    let efectivo_esperado = opcional_real(17);
    let monto_contado = opcional_real(18);
    let diferencia = opcional_real(19);
    let estado_diferencia = opcional_texto(20);
    let justificacion_diferencia = opcional_texto(21);
    drop(filas);

    // Gastos, retiros e ingresos de esta caja, en el orden en que pasaron.
    // La base (Turso) guarda la hora en UTC: se muestra la de Perú.
    const MOV: &str = "m.id, m.tipo, CAST(m.monto AS REAL), m.motivo,
         COALESCE(strftime('%H:%M', m.fecha_hora, '-5 hours'), ''), COALESCE(u.nombre_completo, '')";
    const MOV_DESDE: &str = "FROM movimientos_caja m LEFT JOIN usuarios u ON u.id = m.usuario_id WHERE m.caja_id = ?1";
    let mut filas = match conn
        .query(
            &format!(
                "SELECT {}, (SELECT g.id FROM gastos g WHERE g.movimiento_caja_id = m.id) {} ORDER BY m.id",
                MOV, MOV_DESDE
            ),
            libsql::params![id],
        )
        .await
    {
        Ok(f) => f,
        Err(_) => conn
            .query(&format!("SELECT {}, NULL {} ORDER BY m.id", MOV, MOV_DESDE), libsql::params![id])
            .await
            .map_err(e500)?,
    };
    let mut movimientos = Vec::new();
    while let Some(m) = filas.next().await.map_err(e500)? {
        movimientos.push(MovimientoCaja {
            id: m.get(0).unwrap_or_default(),
            tipo: m.get(1).unwrap_or_default(),
            monto: m.get(2).unwrap_or(0.0),
            motivo: m.get(3).unwrap_or_default(),
            hora: m.get(4).unwrap_or_default(),
            usuario: m.get(5).unwrap_or_default(),
            gasto_id: m.get(6).unwrap_or(None),
        });
    }

    Ok(Json(DetalleCaja {
        caja,
        estado,
        fecha_cierre,
        observaciones_apertura,
        observaciones_cierre,
        efectivo_esperado,
        monto_contado,
        diferencia,
        estado_diferencia,
        justificacion_diferencia,
        movimientos,
    }))
}

#[cfg(test)]
mod pruebas {
    use super::alcanza_efectivo;

    #[test]
    fn no_se_saca_mas_efectivo_del_que_hay() {
        assert!(alcanza_efectivo(27.5, 27.5).is_ok());
        assert!(alcanza_efectivo(27.5, 10.0).is_ok());
        // La venta fue por Yape: en la caja no hay efectivo.
        let error = alcanza_efectivo(0.0, 10.0).unwrap_err();
        assert!(error.contains("S/ 0.00") && error.contains("S/ 10.00"));
        // Una caja que ya quedó en negativo no muestra "hay S/ -12".
        assert!(alcanza_efectivo(-12.0, 1.0).unwrap_err().contains("S/ 0.00"));
    }
}
