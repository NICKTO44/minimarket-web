use axum::{extract::Extension, Json, http::StatusCode};
use chrono::Local;

use crate::models::venta::{NuevaVenta, VentaResult};
use crate::tenants::TenantDb;
use crate::models::auth::Claims;
use crate::handlers::mesas::es_mesero;

const METODOS_PAGO_VALIDOS: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE_PLIN", "MIXTO"];
const METODOS_OTRO_MIXTO: &[&str] = &["TARJETA", "TRANSFERENCIA", "YAPE_PLIN"];

fn redondear_2(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

/// Valida el reparto de un pago MIXTO y devuelve (pago_efectivo,
/// pago_otro, pago_otro_metodo) listos para guardar. Para cualquier otro
/// método devuelve (None, None, None): esas columnas solo se usan en
/// ventas mixtas y el trigger de caja las ignora en el resto.
fn validar_pago(payload: &NuevaVenta) -> Result<(Option<f64>, Option<f64>, Option<String>), String> {
    if !METODOS_PAGO_VALIDOS.contains(&payload.metodo_pago.as_str()) {
        return Err(format!("Método de pago no válido: {}", payload.metodo_pago));
    }
    if payload.metodo_pago != "MIXTO" {
        return Ok((None, None, None));
    }

    let efectivo = redondear_2(payload.pago_efectivo.unwrap_or(0.0));
    let otro = redondear_2(payload.pago_otro.unwrap_or(0.0));
    let metodo_otro = payload.pago_otro_metodo.clone().unwrap_or_default();

    if !METODOS_OTRO_MIXTO.contains(&metodo_otro.as_str()) {
        return Err("En un pago mixto, el otro medio debe ser Tarjeta, Transferencia o Yape/Plin".into());
    }
    if efectivo <= 0.0 || otro <= 0.0 {
        return Err("En un pago mixto, la parte en efectivo y la del otro medio deben ser mayores a cero".into());
    }
    // Comparación en céntimos: tolera 1 céntimo por redondeo, nada más.
    if ((efectivo + otro - payload.total) * 100.0).round().abs() > 1.0 {
        return Err(format!(
            "El pago mixto no cuadra: efectivo S/ {:.2} + otro medio S/ {:.2} debe sumar el total S/ {:.2}",
            efectivo, otro, payload.total
        ));
    }
    if let Some(recibido) = payload.monto_recibido {
        if recibido + 0.005 < efectivo {
            return Err(format!(
                "El efectivo recibido (S/ {:.2}) no alcanza para la parte en efectivo (S/ {:.2})",
                recibido, efectivo
            ));
        }
    }
    Ok((Some(efectivo), Some(otro), Some(metodo_otro)))
}

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

/// Datos reales del producto al momento de vender: (stock, lleva_vencimiento,
/// nombre, unidad, controla_stock). Si la migración 0007 todavía no llegó a
/// esta base (columna controla_stock inexistente), se asume que controla
/// stock, como siempre -- así una venta nunca falla por eso.
async fn datos_producto_para_venta(
    conn: &libsql::Connection,
    producto_id: i64,
) -> Result<Option<(f64, bool, Option<String>, Option<String>, bool)>, String> {
    let consulta = conn
        .query(
            "SELECT stock, lleva_vencimiento, nombre, unidad_medida, controla_stock FROM productos WHERE id = ?1",
            libsql::params![producto_id],
        )
        .await;
    let (mut rows, con_columna) = match consulta {
        Ok(rows) => (rows, true),
        Err(_) => (
            conn.query(
                "SELECT stock, lleva_vencimiento, nombre, unidad_medida FROM productos WHERE id = ?1",
                libsql::params![producto_id],
            )
            .await
            .map_err(|e| e.to_string())?,
            false,
        ),
    };
    match rows.next().await.map_err(|e| e.to_string())? {
        Some(row) => Ok(Some((
            row.get(0).unwrap_or(0.0),
            row.get::<i64>(1).unwrap_or(0) == 1,
            row.get(2).ok(),
            row.get(3).ok(),
            if con_columna { row.get::<i64>(4).unwrap_or(1) == 1 } else { true },
        ))),
        None => Ok(None),
    }
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
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevaVenta>,
) -> Result<Json<VentaResult>, (StatusCode, String)> {
    // El autor de la venta es el usuario de la sesión (JWT), no el
    // usuario_id que manda el navegador.
    let usuario_id = claims.sub;
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Mesero y Preparación (barra/cocina) no cobran.
    if es_mesero(&conn, claims.rol_id).await {
        return Err((StatusCode::FORBIDDEN, "Este usuario no cobra (mesero o barra/cocina). Pide al cajero que cobre la cuenta.".into()));
    }

    // 1. Verificar caja abierta
    let mut rows_caja = conn
        .query(
            "SELECT id FROM cajas WHERE usuario_id = ?1 AND estado = 'ABIERTA'",
            libsql::params![usuario_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows_caja.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Debes abrir una caja antes de procesar ventas".into()));
    }

    // 1b. Validar método de pago (y el reparto si es MIXTO) antes de
    // tocar stock o insertar nada.
    let (pago_efectivo, pago_otro, pago_otro_metodo) =
        validar_pago(&payload).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // 2. Validar stock disponible por producto (perecible o no). De paso
    // se toma el nombre y la unidad REALES del producto (no los que manda
    // el frontend) para guardarlos en la venta tal como son hoy. Los
    // productos preparados al momento (controla_stock = 0, p. ej. un café)
    // no se validan ni se descuentan.
    // producto_id -> (nombre, unidad, lleva_vencimiento, controla_stock)
    let mut datos_producto: std::collections::HashMap<i64, (String, String, bool, bool)> = std::collections::HashMap::new();
    for p in &payload.productos {
        let datos = datos_producto_para_venta(&conn, p.id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

        if let Some((stock, lleva_vencimiento, nombre, unidad, controla_stock)) = datos {
            let nombre_actual = nombre.unwrap_or_else(|| p.nombre.clone());
            let unidad_actual = unidad.unwrap_or_else(|| "UNIDAD".to_string());
            datos_producto.insert(p.id, (nombre_actual, unidad_actual, lleva_vencimiento, controla_stock));
            if controla_stock && stock < p.cantidad {
                return Err((StatusCode::BAD_REQUEST, format!(
                    "Stock insuficiente para {} (disponible: {}, solicitado: {})",
                    p.nombre, stock, p.cantidad
                )));
            }
        } else {
            return Err((StatusCode::BAD_REQUEST, format!("Producto {} no encontrado", p.nombre)));
        }
    }

    // 2b. Cobro de un pedido de mesa: debe seguir abierto y lo que se cobra
    // debe ser exactamente lo que tiene el pedido (si alguien agregó algo
    // mientras se cobraba, se pide volver a abrirlo).
    if let Some(pedido_id) = payload.pedido_id {
        let mut rows = conn
            .query(
                "SELECT p.estado,
                        CAST(COALESCE((SELECT SUM(i.cantidad * i.precio_unitario) FROM pedido_items i
                                  WHERE i.pedido_id = p.id AND i.estado != 'ANULADO'), 0) AS REAL)
                 FROM pedidos p WHERE p.id = ?1",
                libsql::params![pedido_id],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let (estado, total_pedido): (String, f64) =
            match rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                Some(row) => (row.get(0).unwrap_or_default(), row.get(1).unwrap_or(0.0)),
                None => return Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
            };
        if estado != "ABIERTO" {
            return Err((StatusCode::CONFLICT, "Este pedido ya fue cobrado o anulado.".into()));
        }
        let total_cobrado: f64 = payload.productos.iter().map(|p| p.precio * p.cantidad).sum();
        if ((total_cobrado - total_pedido) * 100.0).round().abs() > 1.0 {
            return Err((StatusCode::CONFLICT, "El pedido cambió mientras cobrabas. Vuelve a abrir la mesa para cobrar lo actualizado.".into()));
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
        "INSERT INTO ventas (folio, cliente_id, subtotal, descuento, total, metodo_pago, monto_recibido, cambio, usuario_id, estado,
                             pago_efectivo, pago_otro, pago_otro_metodo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'COMPLETADA', ?10, ?11, ?12)",
        libsql::params![
            folio.clone(), payload.cliente_id, subtotal, descuento_total, payload.total,
            payload.metodo_pago.clone(), payload.monto_recibido, payload.cambio, usuario_id,
            pago_efectivo, pago_otro, pago_otro_metodo
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

        let (lleva_vencimiento, controla_stock) = datos_producto
            .get(&p.id)
            .map(|d| (d.2, d.3))
            .unwrap_or((false, true));

        if !controla_stock {
            // Preparado al momento: no hay stock que descontar.
        } else if lleva_vencimiento {
            descontar_stock_fefo(&conn, p.id, p.cantidad).await
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        } else {
            descontar_stock_simple(&conn, p.id, p.cantidad).await
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        }

        // Nombre y unidad congelados al momento de la venta (migración 0005):
        // si después se renombra el producto, esta venta sigue mostrando
        // el nombre con el que se vendió, igual que su comprobante SUNAT.
        let (nombre_base, unidad_venta) = datos_producto
            .get(&p.id)
            .map(|d| (d.0.clone(), d.1.clone()))
            .unwrap_or_else(|| (p.nombre.clone(), "UNIDAD".to_string()));
        // En pedidos de mesa, las opciones van con el nombre:
        // "Capuchino (Grande, Leche de almendras)".
        let nombre_venta = match p.detalle.as_deref().map(str::trim) {
            Some(detalle) if !detalle.is_empty() => {
                format!("{} ({})", nombre_base, detalle.chars().take(120).collect::<String>())
            }
            _ => nombre_base,
        };

        conn.execute(
            "INSERT INTO detalles_venta (venta_id, producto_id, cantidad, precio_unitario, subtotal, descuento_linea, total_linea,
                                         nombre_producto, unidad_medida)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            libsql::params![venta_id, p.id, p.cantidad, p.precio, sub, desc, total_linea, nombre_venta, unidad_venta],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar detalle: {}", e)))?;
    }

    // 7. Si fue el cobro de un pedido de mesa: se cierra y la mesa queda libre.
    if let Some(pedido_id) = payload.pedido_id {
        conn.execute("UPDATE ventas SET pedido_id = ?1 WHERE id = ?2", libsql::params![pedido_id, venta_id])
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        conn.execute(
            "UPDATE pedidos SET estado = 'COBRADO', venta_id = ?1, fecha_cierre = datetime('now', 'localtime')
             WHERE id = ?2 AND estado = 'ABIERTO'",
            libsql::params![venta_id, pedido_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Si una MESA paga, lo que barra/cocina ya tenía LISTO se da por
        // entregado (ya lo consumieron). Lo que aún se prepara sigue en
        // Preparación y el mozo recibe su aviso. En para llevar/delivery no
        // se toca: ahí se paga antes y se entrega después.
        // La venta ya está registrada: si esto falla (p. ej. la base aún no
        // tiene la migración 0008) no se revierte nada, solo se ignora.
        let _ = conn
            .execute(
                "UPDATE pedido_items SET fecha_entregado = datetime('now', 'localtime')
                 WHERE pedido_id = ?1 AND estado = 'ENVIADO' AND fecha_listo IS NOT NULL
                   AND fecha_entregado IS NULL
                   AND (SELECT tipo FROM pedidos WHERE id = ?1) = 'MESA'",
                libsql::params![pedido_id],
            )
            .await;
    }

    Ok(Json(VentaResult { venta_id, folio }))
}