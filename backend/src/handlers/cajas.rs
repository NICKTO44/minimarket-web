use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::auth::Claims;
use crate::models::caja::{AbrirCajaRequest, CerrarCajaRequest, MovimientoCajaRequest, CajaResponse};
use crate::models::caja::CajaEstado;

/// Módulo VARIAS_CAJAS en la lista de módulos tal como se guarda ("A,B,C").
pub fn varias_cajas_en(modulos: &str) -> bool {
    modulos.split(',').any(|m| m.trim() == crate::handlers::rubros::MODULO_VARIAS_CAJAS)
}

/// ¿El negocio tiene encendido VARIAS_CAJAS? Apagado: una sola caja abierta
/// por negocio y todos venden en ella (como siempre).
pub async fn varias_cajas(conn: &libsql::Connection) -> bool {
    crate::handlers::rubros::negocio(conn)
        .await
        .modulos
        .iter()
        .any(|m| m == crate::handlers::rubros::MODULO_VARIAS_CAJAS)
}

/// La caja abierta donde trabaja un usuario: la suya y, sin VARIAS_CAJAS,
/// si no tiene, la del negocio. Es la misma regla de los triggers de caja.
/// Con VARIAS_CAJAS solo cuenta la suya.
pub async fn caja_de_usuario(conn: &libsql::Connection, usuario_id: i64, varias: bool) -> Result<Option<i64>, (StatusCode, String)> {
    let sql = if varias {
        "SELECT id FROM cajas WHERE estado = 'ABIERTA' AND usuario_id = ?1 ORDER BY id DESC LIMIT 1"
    } else {
        "SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY (usuario_id = ?1) DESC, id DESC LIMIT 1"
    };
    let mut filas = conn
        .query(sql, libsql::params![usuario_id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(match filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(f) => f.get::<i64>(0).ok(),
        None => None,
    })
}

pub async fn abrir_caja(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<AbrirCajaRequest>,
) -> Result<Json<CajaResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if crate::handlers::mesas::es_mesero(&conn, claims.rol_id).await {
        return Err((StatusCode::FORBIDDEN, "Este usuario no maneja caja (mesero o barra/cocina).".into()));
    }

    // Con VARIAS_CAJAS cada usuario tiene su caja: solo se revisa la suya.
    let varias = varias_cajas(&conn).await;
    let mut rows = conn
        .query(
            "SELECT c.id, u.nombre_completo FROM cajas c
             JOIN usuarios u ON u.id = c.usuario_id
             WHERE c.estado = 'ABIERTA' AND (?1 = 0 OR c.usuario_id = ?2)",
            libsql::params![varias as i64, claims.sub],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(row) = rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        if varias {
            return Err((StatusCode::BAD_REQUEST, "Ya tienes una caja abierta. Ciérrala antes de abrir otra.".into()));
        }
        let cajero: String = row.get(1).unwrap_or_default();
        return Err((StatusCode::BAD_REQUEST, format!(
            "Ya hay una caja abierta en el sistema (cajero: {}). Debe cerrarse primero.", cajero
        )));
    }
    drop(rows);

    if payload.monto_inicial < 0.0 {
        return Err((StatusCode::BAD_REQUEST, "El monto inicial no puede ser negativo".into()));
    }

    // La misma sentencia revisa que no haya otra abierta: con un doble clic
    // (o dos cajeros a la vez) antes podían quedar DOS cajas abiertas. Con
    // VARIAS_CAJAS, que este usuario no tenga otra abierta.
    let abiertas = conn.execute(
        "INSERT INTO cajas (usuario_id, numero_caja, turno, monto_inicial, observaciones_apertura, fecha_apertura, hora_apertura)
         SELECT ?1, ?2, 'GENERAL', ?3, ?4, datetime('now','localtime'), strftime('%H:%M:%S','now','localtime')
         WHERE NOT EXISTS (SELECT 1 FROM cajas WHERE estado = 'ABIERTA' AND (?5 = 0 OR usuario_id = ?1))",
        libsql::params![claims.sub, payload.numero_caja.unwrap_or(1), payload.monto_inicial, payload.observaciones.clone(), varias as i64],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al abrir caja: {}", e)))?;
    if abiertas == 0 {
        let texto = if varias { "Ya tienes una caja abierta. Ciérrala antes de abrir otra." } else { "Ya hay una caja abierta en el sistema. Debe cerrarse primero." };
        return Err((StatusCode::BAD_REQUEST, texto.into()));
    }

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
    if caja_usuario_id != claims.sub && !crate::middleware_auth::es_admin(&claims) {
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

    // Solo si sigue abierta: un doble clic ya no vuelve a escribir el cierre.
    let cerradas = conn.execute(
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
         WHERE id = ?7 AND estado = 'ABIERTA'",
        libsql::params![
            payload.monto_contado, payload.observaciones.clone(), efectivo_esperado,
            diferencia, estado_diferencia, payload.justificacion_diferencia.clone(), payload.caja_id
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al cerrar caja: {}", e)))?;
    if cerradas == 0 {
        return Err((StatusCode::BAD_REQUEST, "Caja no encontrada o ya está cerrada".into()));
    }

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

/// Formas de pago de un movimiento de caja que NO salen del efectivo de la
/// caja (migración 0063). Se registran en el turno, sin tocar el cuadre.
pub const METODOS_FUERA_DE_CAJA: [&str; 6] = ["EFECTIVO", "YAPE", "PLIN", "TRANSFERENCIA", "TARJETA", "OTRO"];

/// Forma de pago de un retiro: None = efectivo de la caja (como siempre);
/// Some(metodo) = pagado de otro modo. Solo el retiro la elige.
pub fn metodo_de_retiro(tipo: &str, metodo: Option<&str>) -> Result<Option<String>, String> {
    let metodo = metodo.map(|m| m.trim().to_uppercase()).filter(|m| !m.is_empty() && m != "EFECTIVO_CAJA");
    match metodo {
        None => Ok(None),
        Some(_) if tipo != "RETIRO" => Err("Solo el retiro puede tener otra forma de pago.".to_string()),
        // Un retiro de "efectivo de otro lado" no es un retiro de esta caja.
        Some(m) if m == "EFECTIVO" || !METODOS_FUERA_DE_CAJA.contains(&m.as_str()) => {
            Err("Forma de pago no válida.".to_string())
        }
        Some(m) => Ok(Some(m)),
    }
}

/// Error al guardar un movimiento con forma de pago: si la base aún no tiene
/// la columna, se explica.
pub fn sin_migracion_0063<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    let texto = e.to_string();
    if texto.contains("metodo_pago") {
        (
            StatusCode::BAD_REQUEST,
            "Este negocio todavía no tiene la forma de pago en la caja (migración 0063). Reinicia el backend para aplicarla.".to_string(),
        )
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, texto)
    }
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

    // Forma de pago (migración 0063): solo el retiro la elige; lo que no es
    // efectivo de la caja no toca el cuadre.
    let metodo = metodo_de_retiro(&payload.tipo, payload.metodo_pago.as_deref()).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Some(metodo) = metodo {
        conn.execute(
            "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id, metodo_pago) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![payload.caja_id, payload.tipo.clone(), payload.monto, payload.motivo.clone(), claims.sub, metodo],
        )
        .await
        .map_err(sin_migracion_0063)?;
        return Ok(Json(CajaResponse {
            success: true,
            message: "Movimiento registrado".into(),
            caja_id: Some(payload.caja_id),
        }));
    }

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
    /// None = efectivo de la caja. Otra forma de pago (migración 0063): solo
    /// registro del turno, no descuenta el efectivo esperado.
    pub metodo_pago: Option<String>,
}

/// GET /cajas/movimientos — gastos, retiros e ingresos de la caja abierta,
/// del más reciente al más antiguo.
pub async fn movimientos_caja_abierta(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<MovimientoCaja>>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // La caja que ve este usuario (la suya; sin VARIAS_CAJAS, la del negocio).
    // Con varias cajas abiertas no se mezclan los movimientos de las demás.
    let caja_id = match caja_de_usuario(&conn, claims.sub, varias_cajas(&conn).await).await? {
        Some(id) => id,
        None => return Ok(Json(Vec::new())),
    };
    // La base (Turso) guarda la hora en UTC: se muestra la de Perú.
    const COLUMNAS: &str = "m.id, m.tipo, CAST(m.monto AS REAL), m.motivo,
         COALESCE(strftime('%H:%M', m.fecha_hora, '-5 hours'), ''), COALESCE(u.nombre_completo, '')";
    const DESDE: &str = "FROM movimientos_caja m
         JOIN cajas c ON c.id = m.caja_id AND c.estado = 'ABIERTA' AND c.id = ?1
         LEFT JOIN usuarios u ON u.id = m.usuario_id";
    // Con el número de gasto (migración 0022) y la forma de pago (0063); si
    // la base aún no las tiene, la consulta más simple que funcione.
    const GASTO: &str = "(SELECT g.id FROM gastos g WHERE g.movimiento_caja_id = m.id)";
    let mut filas = None;
    for extra in [format!("{}, m.metodo_pago", GASTO), format!("{}, NULL", GASTO), "NULL, NULL".to_string()] {
        if let Ok(f) = conn
            .query(&format!("SELECT {}, {} {} ORDER BY m.id DESC", COLUMNAS, extra, DESDE), libsql::params![caja_id])
            .await
        {
            filas = Some(f);
            break;
        }
    }
    let mut filas = filas.ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No se pudieron leer los movimientos de la caja.".to_string()))?;
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
            metodo_pago: f.get(7).unwrap_or(None),
        });
    }
    Ok(Json(lista))
}

/// Columnas de la migración 0019, al final de cada consulta de cajas.
const BILLETERAS: &str = "CAST(c.ventas_yape AS REAL), CAST(c.ventas_plin AS REAL), CAST(c.ventas_yape_plin AS REAL), c.detalle_billeteras";

/// Cajas abiertas como las ve el punto de venta: `filtro` va después de
/// "WHERE c.estado = 'ABIERTA'" y puede usar ?1.
async fn leer_cajas_abiertas(conn: &libsql::Connection, filtro: &str, param: i64) -> Option<Vec<CajaEstado>> {
    // Con Yape y Plin por separado (migración 0019); si la base aún no tiene
    // esas columnas, la consulta de siempre (y van en cero).
    const COLUMNAS: &str = "c.id, u.nombre_completo, c.monto_inicial, c.ventas_efectivo,
                    c.ventas_tarjeta, c.ventas_transferencia, c.total_ventas, c.numero_transacciones,
                    c.devoluciones_monto, c.retiros_total, c.ingresos_total, c.gastos_total, c.fecha_apertura";
    let desde = format!("FROM cajas c JOIN usuarios u ON u.id = c.usuario_id WHERE c.estado = 'ABIERTA' {}", filtro);
    let con_billeteras = format!("SELECT {}, {} {}", COLUMNAS, BILLETERAS, desde);
    let mut rows = match conn.query(&con_billeteras, libsql::params![param]).await {
        Ok(r) => r,
        Err(_) => conn.query(&format!("SELECT {} {}", COLUMNAS, desde), libsql::params![param]).await.ok()?,
    };

    let mut cajas = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        cajas.push(CajaEstado {
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
        });
    }
    Some(cajas)
}

/// GET /cajas/abierta — la caja de quien pregunta. Sin VARIAS_CAJAS, la del
/// negocio (la suya primero, como los triggers). Con VARIAS_CAJAS, solo la
/// suya: si no abrió una, None y el punto de venta le pide abrirla.
pub async fn obtener_caja_abierta(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Json<Option<CajaEstado>> {
    let conn = match tenant.0.connect() {
        Ok(c) => c,
        Err(_) => return Json(None),
    };
    let filtro = if varias_cajas(&conn).await {
        "AND c.usuario_id = ?1 ORDER BY c.id DESC LIMIT 1"
    } else {
        "ORDER BY (c.usuario_id = ?1) DESC, c.id DESC LIMIT 1"
    };
    Json(leer_cajas_abiertas(&conn, filtro, claims.sub).await.and_then(|c| c.into_iter().next()))
}

/// GET /cajas/abiertas — todas las cajas abiertas del negocio (para que el
/// administrador vea y cierre las de cada vendedor con VARIAS_CAJAS).
pub async fn listar_cajas_abiertas(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<CajaEstado>>, (StatusCode, String)> {
    if !crate::middleware_auth::es_admin(&claims) {
        return Err((StatusCode::FORBIDDEN, "Solo el administrador ve todas las cajas abiertas.".into()));
    }
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // ?1 no se usa aquí, pero la consulta siempre lleva un parámetro.
    leer_cajas_abiertas(&conn, "AND ?1 = ?1 ORDER BY c.id", 0)
        .await
        .map(Json)
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No se pudieron leer las cajas abiertas.".into()))
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
             WHERE date(c.fecha_apertura, '-5 hours') BETWEEN ?1 AND ?2
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
    // Con el número de gasto (0022) y la forma de pago (0063), si las tiene.
    const GASTO: &str = "(SELECT g.id FROM gastos g WHERE g.movimiento_caja_id = m.id)";
    let mut filas = None;
    for extra in [format!("{}, m.metodo_pago", GASTO), format!("{}, NULL", GASTO), "NULL, NULL".to_string()] {
        if let Ok(f) = conn
            .query(&format!("SELECT {}, {} {} ORDER BY m.id", MOV, extra, MOV_DESDE), libsql::params![id])
            .await
        {
            filas = Some(f);
            break;
        }
    }
    let mut filas = filas.ok_or_else(|| e500("No se pudieron leer los movimientos de la caja."))?;
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
            metodo_pago: m.get(7).unwrap_or(None),
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
    use super::{alcanza_efectivo, metodo_de_retiro};

    #[test]
    fn forma_de_pago_del_retiro() {
        // Sin forma de pago o efectivo de la caja: como siempre.
        assert_eq!(metodo_de_retiro("RETIRO", None), Ok(None));
        assert_eq!(metodo_de_retiro("RETIRO", Some("efectivo_caja")), Ok(None));
        assert_eq!(metodo_de_retiro("INGRESO", Some(" ")), Ok(None));
        assert_eq!(metodo_de_retiro("RETIRO", Some("yape")), Ok(Some("YAPE".to_string())));
        assert!(metodo_de_retiro("RETIRO", Some("CHEQUE")).is_err());
        assert!(metodo_de_retiro("RETIRO", Some("EFECTIVO")).is_err());
        // El ingreso y el gasto (este va por el módulo Gastos) no la eligen aquí.
        assert!(metodo_de_retiro("INGRESO", Some("YAPE")).is_err());
        assert!(metodo_de_retiro("GASTO", Some("YAPE")).is_err());
    }

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
