use axum::{extract::Extension, Json, http::StatusCode};
use chrono::Local;

use crate::models::venta::{NuevaVenta, VentaResult};
use crate::tenants::TenantDb;

// Descuenta stock por FEFO: recorre lotes activos ordenados por fecha de
// vencimiento y va restando hasta cubrir la cantidad vendida.
async fn descontar_stock_fefo(
    conn: &libsql::Connection,
    producto_id: i64,
    cantidad_requerida: f64,
) -> Result<(), String> {
    let mut rows = conn
        .query(
            "SELECT id, cantidad FROM lotes_producto
             WHERE producto_id = ?1 AND activo = 1 AND cantidad > 0
             ORDER BY fecha_vencimiento ASC",
            libsql::params![producto_id],
        )
        .await
        .map_err(|e| format!("Error al leer lotes: {}", e))?;

    let mut lotes: Vec<(i64, f64)> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        lotes.push((row.get(0).unwrap_or_default(), row.get(1).unwrap_or_default()));
    }

    let disponible_total: f64 = lotes.iter().map(|(_, c)| c).sum();
    if disponible_total < cantidad_requerida {
        return Err(format!(
            "Stock insuficiente por vencimiento (disponible: {}, solicitado: {})",
            disponible_total, cantidad_requerida
        ));
    }

    let mut restante = cantidad_requerida;
    for (lote_id, cantidad_lote) in lotes {
        if restante <= 0.0 { break; }
        let a_descontar = if cantidad_lote >= restante { restante } else { cantidad_lote };
        conn.execute(
            "UPDATE lotes_producto SET cantidad = cantidad - ?1 WHERE id = ?2",
            libsql::params![a_descontar, lote_id],
        )
        .await
        .map_err(|e| format!("Error al descontar lote: {}", e))?;
        restante -= a_descontar;
    }

    Ok(())
}

// Para productos SIN vencimiento: descuenta directo de productos.stock
async fn descontar_stock_simple(
    conn: &libsql::Connection,
    producto_id: i64,
    cantidad: f64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE productos SET stock = stock - ?1 WHERE id = ?2",
        libsql::params![cantidad, producto_id],
    )
    .await
    .map_err(|e| format!("Error al descontar stock: {}", e))?;
    Ok(())
}

pub async fn procesar_venta(
    Extension(tenant): Extension<std::sync::Arc<TenantDb>>,
    Json(payload): Json<NuevaVenta>,
) -> Result<Json<VentaResult>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 1. Verificar caja abierta
    let mut rows_caja = conn
        .query(
            "SELECT id FROM cajas WHERE usuario_id = ?1 AND estado = 'ABIERTA'",
            libsql::params![payload.usuario_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows_caja.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Debes abrir una caja antes de procesar ventas".into()));
    }

    // 2. Validar stock disponible por producto (perecible o no)
    for p in &payload.productos {
        let mut rows = conn
            .query(
                "SELECT stock, lleva_vencimiento FROM productos WHERE id = ?1",
                libsql::params![p.id],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(row) = rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            let stock: f64 = row.get(0).unwrap_or(0.0);
            if stock < p.cantidad {
                return Err((StatusCode::BAD_REQUEST, format!(
                    "Stock insuficiente para {} (disponible: {}, solicitado: {})",
                    p.nombre, stock, p.cantidad
                )));
            }
        } else {
            return Err((StatusCode::BAD_REQUEST, format!("Producto {} no encontrado", p.nombre)));
        }
    }

    // 3. Generar folio único del día
    let fecha_actual = Local::now().format("%Y%m%d").to_string();
    let folio_query = format!(
        "SELECT COALESCE(MAX(CAST(substr(folio, -4) AS INTEGER)), 0) + 1
         FROM ventas WHERE folio LIKE 'V-{}%'",
        fecha_actual
    );
    let mut rows_folio = conn.query(&folio_query, ()).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let siguiente: i64 = match rows_folio.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(1),
        None => 1,
    };
    let folio = format!("V-{}-{:04}", fecha_actual, siguiente);

    // 4. Calcular subtotal y descuento
    let mut subtotal = 0.0f64;
    let mut descuento_total = 0.0f64;
    for p in &payload.productos {
        let sub = p.precio * p.cantidad;
        let desc = p.descuento_monto.unwrap_or(0.0).max(0.0).min(sub);
        subtotal += sub;
        descuento_total += desc;
    }

    // 5. Insertar venta
    conn.execute(
        "INSERT INTO ventas (folio, cliente_id, subtotal, descuento, total, metodo_pago, monto_recibido, cambio, usuario_id, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'COMPLETADA')",
        libsql::params![
            folio.clone(), payload.cliente_id, subtotal, descuento_total, payload.total,
            payload.metodo_pago.clone(), payload.monto_recibido, payload.cambio, payload.usuario_id
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar venta: {}", e)))?;

    let venta_id = conn.last_insert_rowid();

    // 6. Insertar detalles + descontar stock
    for p in &payload.productos {
        let sub = p.precio * p.cantidad;
        let desc = p.descuento_monto.unwrap_or(0.0).max(0.0).min(sub);
        let total_linea = sub - desc;

        let mut rows = conn
            .query("SELECT lleva_vencimiento FROM productos WHERE id = ?1", libsql::params![p.id])
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let lleva_vencimiento: bool = match rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get::<i64>(0).unwrap_or(0) == 1,
            None => false,
        };

        if lleva_vencimiento {
            descontar_stock_fefo(&conn, p.id, p.cantidad).await
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        } else {
            descontar_stock_simple(&conn, p.id, p.cantidad).await
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        }

        conn.execute(
            "INSERT INTO detalles_venta (venta_id, producto_id, cantidad, precio_unitario, subtotal, descuento_linea, total_linea)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            libsql::params![venta_id, p.id, p.cantidad, p.precio, sub, desc, total_linea],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar detalle: {}", e)))?;
    }

    Ok(Json(VentaResult { venta_id, folio }))
}