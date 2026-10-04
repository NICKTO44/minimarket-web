use axum::{extract::Extension, Json, http::StatusCode};
use chrono::Local;

use crate::models::venta::{NuevaVenta, VentaResult};
use crate::tenants::TenantDb;
use crate::models::auth::Claims;

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

/// true si el error es que la base (Turso) perdió la sesión abierta entre
/// un paso y otro ("stream not found"): no es un error de datos, se puede
/// volver a intentar.
fn es_corte_de_sesion(mensaje: &str) -> bool {
    let m = mensaje.to_lowercase();
    m.contains("stream not found") || m.contains("stream closed") || m.contains("stream expired") || m.contains("invalid baton")
}

// Descuenta stock por FEFO (primero lo que vence antes) de TODOS los
// productos perecibles de la venta en dos viajes a la base, sin importar
// cuántos sean: uno lee sus lotes y otro los descuenta. Va dentro de la
// transacción de la venta, que debe ser corta.
// `pedidos`: (producto_id, cantidad vendida) ya sumada por producto.
async fn descontar_stock_fefo(conn: &libsql::Connection, pedidos: &[(i64, f64)]) -> Result<(), String> {
    if pedidos.is_empty() {
        return Ok(());
    }
    // Los ids son enteros: se escriben directo en la lista.
    let productos = pedidos.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
    let mut rows = conn
        .query(
            &format!(
                "SELECT id, producto_id, CAST(cantidad AS REAL) FROM lotes_producto
                 WHERE producto_id IN ({}) AND activo = 1 AND cantidad > 0
                 ORDER BY producto_id, fecha_vencimiento ASC, id ASC",
                productos
            ),
            (),
        )
        .await
        .map_err(|e| format!("Error al leer lotes: {}", e))?;

    // producto_id -> sus lotes (id, cantidad), del que vence antes al que vence después.
    let mut lotes: std::collections::HashMap<i64, Vec<(i64, f64)>> = std::collections::HashMap::new();
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        lotes
            .entry(row.get::<i64>(1).unwrap_or_default())
            .or_default()
            .push((row.get(0).unwrap_or_default(), row.get(2).unwrap_or(0.0)));
    }

    // (lote_id, cuánto se le descuenta)
    let mut descuentos: Vec<(i64, f64)> = Vec::new();
    for (producto_id, cantidad_requerida) in pedidos {
        let del_producto = lotes.get(producto_id).map(Vec::as_slice).unwrap_or(&[]);
        let disponible_total: f64 = del_producto.iter().map(|(_, c)| c).sum();
        if disponible_total + 1e-6 < *cantidad_requerida {
            return Err(format!(
                "Stock insuficiente por vencimiento (disponible: {}, solicitado: {})",
                disponible_total, cantidad_requerida
            ));
        }
        let mut restante = *cantidad_requerida;
        for (lote_id, cantidad_lote) in del_producto {
            if restante <= 1e-9 {
                break;
            }
            let a_descontar = cantidad_lote.min(restante);
            descuentos.push((*lote_id, a_descontar));
            restante -= a_descontar;
        }
    }
    if descuentos.is_empty() {
        return Ok(());
    }

    // Solo números ya validados: se escriben directo en la sentencia. El
    // trigger de lotes recalcula el stock de cada producto.
    let casos: String = descuentos.iter().map(|(id, cantidad)| format!(" WHEN {} THEN {}", id, cantidad)).collect();
    let lista: String = descuentos.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
    conn.execute(
        &format!("UPDATE lotes_producto SET cantidad = MAX(cantidad - CASE id{} ELSE 0 END, 0) WHERE id IN ({})", casos, lista),
        (),
    )
    .await
    .map_err(|e| format!("Error al descontar lote: {}", e))?;
    Ok(())
}

/// Datos reales de los productos al momento de vender, todos en una sola
/// consulta: id -> (stock, lleva_vencimiento, nombre, unidad, controla_stock).
/// Si la migración 0007 todavía no llegó a esta base (columna controla_stock
/// inexistente), se asume que controlan stock, como siempre -- así una
/// venta nunca falla por eso.
async fn datos_productos_para_venta(
    conn: &libsql::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, (f64, bool, Option<String>, Option<String>, bool)>, String> {
    let mut datos = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(datos);
    }
    // Los ids son enteros: se escriben directo en la lista.
    let lista = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let consulta = conn
        .query(
            &format!("SELECT id, stock, lleva_vencimiento, nombre, unidad_medida, controla_stock FROM productos WHERE id IN ({})", lista),
            (),
        )
        .await;
    let (mut rows, con_columna) = match consulta {
        Ok(rows) => (rows, true),
        Err(_) => (
            conn.query(
                &format!("SELECT id, stock, lleva_vencimiento, nombre, unidad_medida FROM productos WHERE id IN ({})", lista),
                (),
            )
            .await
            .map_err(|e| e.to_string())?,
            false,
        ),
    };
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        datos.insert(
            row.get::<i64>(0).unwrap_or_default(),
            (
                row.get(1).unwrap_or(0.0),
                row.get::<i64>(2).unwrap_or(0) == 1,
                row.get(3).ok(),
                row.get(4).ok(),
                if con_columna { row.get::<i64>(5).unwrap_or(1) == 1 } else { true },
            ),
        );
    }
    Ok(datos)
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

    // Cada consulta es un viaje a la base (0.1–0.2 s desde Perú), así que
    // una venta hace los menos posibles: 6 sin importar cuántos productos
    // lleve. En el primero se lee todo lo previo: el rol de quien cobra, la
    // caja abierta y la tasa de IGV del negocio.
    //
    // Caja: el negocio tiene una sola abierta a la vez y cualquiera que
    // cobra (admin o cajero) vende en ella, la haya abierto quien la haya
    // abierto. Es la misma regla de los triggers de caja (migración 0004):
    // primero la caja propia; si no, la abierta.
    //
    // De paso se leen los módulos del negocio, solo para saber si tiene
    // encendido el reporte de ganancias (ahí la venta guarda además el costo
    // de cada producto). Si la base no tuviera esa columna, se lee como
    // siempre y la venta sigue igual.
    const PREVIO: &str = "SELECT (SELECT nombre FROM roles WHERE id = ?1),
                        (SELECT id FROM cajas WHERE estado = 'ABIERTA'
                          ORDER BY (usuario_id = ?2) DESC, id DESC LIMIT 1),
                        (SELECT CAST(iva_porcentaje AS REAL) FROM configuracion_tienda LIMIT 1)";
    let (rol, caja_abierta, tasa_igv, con_ganancias): (Option<String>, Option<i64>, Option<f64>, bool) = {
        let con_modulos = format!("{}, (SELECT modulos FROM configuracion_tienda LIMIT 1)", PREVIO);
        let mut filas = match conn.query(&con_modulos, libsql::params![claims.rol_id, usuario_id]).await {
            Ok(filas) => filas,
            Err(_) => conn
                .query(PREVIO, libsql::params![claims.rol_id, usuario_id])
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        };
        match filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(f) => (
                f.get::<String>(0).ok(),
                f.get::<i64>(1).ok(),
                f.get::<f64>(2).ok(),
                f.get::<String>(3).map(|m| crate::handlers::ganancias::encendido_en(&m)).unwrap_or(false),
            ),
            None => (None, None, None, false),
        }
    };

    // Mesero y Preparación (barra/cocina) no cobran.
    if matches!(rol.as_deref(), Some("MESERO") | Some("PREPARACION")) {
        return Err((StatusCode::FORBIDDEN, "Este usuario no cobra (mesero o barra/cocina). Pide al cajero que cobre la cuenta.".into()));
    }

    // 1. Verificar caja abierta.
    if caja_abierta.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Debes abrir una caja antes de procesar ventas".into()));
    }

    // 1b. Validar método de pago (y el reparto si es MIXTO) antes de
    // tocar stock o insertar nada.
    // Venta al crédito (módulo CREDITO): el pago queda pendiente. Se guarda
    // como MIXTO con efectivo 0 y todo el total en 'CREDITO' (ver migración
    // 0015), así la caja no cuenta dinero que todavía no entró.
    let credito = match &payload.credito {
        Some(c) => Some(crate::handlers::creditos::validar_venta(&conn, c, payload.cliente_id, payload.total).await?),
        None => None,
    };
    let (metodo_pago, pago_efectivo, pago_otro, pago_otro_metodo, monto_recibido, cambio) = if credito.is_some() {
        (
            "MIXTO".to_string(),
            Some(0.0),
            Some(redondear_2(payload.total)),
            Some(crate::handlers::creditos::METODO_CREDITO.to_string()),
            None,
            None,
        )
    } else {
        let (efectivo, otro, metodo_otro) = validar_pago(&payload).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        (payload.metodo_pago.clone(), efectivo, otro, metodo_otro, payload.monto_recibido, payload.cambio)
    };

    // 1c. Cambio de prenda (módulo CAMBIOS): se revisa lo que el cliente
    // devuelve antes de registrar nada. La venta es por el total de lo que
    // se lleva; la devolución se registra después, por el mismo medio.
    let cambio_prenda = match &payload.cambio_prenda {
        Some(c) => {
            if credito.is_some() || payload.pedido_id.is_some() || payload.cotizacion_id.is_some() {
                return Err((StatusCode::BAD_REQUEST, "Un cambio de prenda no se puede combinar con crédito, un pedido de mesa ni una cotización.".into()));
            }
            Some(crate::handlers::cambios::preparar(&conn, c, &metodo_pago).await?)
        }
        None => None,
    };

    // 2. Validar stock disponible por producto (perecible o no). De paso
    // se toma el nombre y la unidad REALES del producto (no los que manda
    // el frontend) para guardarlos en la venta tal como son hoy. Los
    // productos preparados al momento (controla_stock = 0, p. ej. un café)
    // no se validan ni se descuentan.
    // producto_id -> (nombre, unidad, lleva_vencimiento, controla_stock)
    let mut datos_producto: std::collections::HashMap<i64, (String, String, bool, bool)> = std::collections::HashMap::new();
    // El mismo producto puede venir en varias líneas (venta por medidas:
    // tablas de distinto tamaño de la misma madera). El stock se compara
    // contra la suma de todas sus líneas.
    let mut pedido_por_producto: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for p in &payload.productos {
        if !(p.cantidad > 0.0) || !p.cantidad.is_finite() {
            return Err((StatusCode::BAD_REQUEST, format!("La cantidad de {} debe ser mayor a 0", p.nombre)));
        }
        *pedido_por_producto.entry(p.id).or_insert(0.0) += p.cantidad;
    }
    let mut ids: Vec<i64> = pedido_por_producto.keys().copied().collect();
    ids.sort_unstable();
    let mut leidos = datos_productos_para_venta(&conn, &ids)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    for p in &payload.productos {
        // Se saca del mapa la primera vez; si el producto se repite en otra
        // línea ya quedó en `datos_producto` y pasó la validación de stock.
        if datos_producto.contains_key(&p.id) {
            continue;
        }
        if let Some((stock, lleva_vencimiento, nombre, unidad, controla_stock)) = leidos.remove(&p.id) {
            let nombre_actual = nombre.unwrap_or_else(|| p.nombre.clone());
            let unidad_actual = unidad.unwrap_or_else(|| "UNIDAD".to_string());
            datos_producto.insert(p.id, (nombre_actual, unidad_actual, lleva_vencimiento, controla_stock));
            let solicitado = pedido_por_producto.get(&p.id).copied().unwrap_or(p.cantidad);
            // Margen mínimo por los decimales (33.33 + 66.67 no siempre da 100 exacto).
            if controla_stock && stock + 1e-6 < solicitado {
                return Err((StatusCode::BAD_REQUEST, format!(
                    "Stock insuficiente para {} (disponible: {}, solicitado: {})",
                    p.nombre, stock, redondear_2(solicitado)
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

    // 3. El folio del día ("V-20261003-0007") lo calcula la misma sentencia
    // que inserta la venta: un viaje menos y sin riesgo de que dos cajas
    // tomen el mismo número.
    let fecha_actual = Local::now().format("%Y%m%d").to_string();

    // 4. Calcular subtotal y descuento
    let mut subtotal = 0.0f64;
    let mut descuento_total = 0.0f64;
    for p in &payload.productos {
        // Al céntimo: con cantidades con decimales (37.5 pies a S/ 4.35),
        // precio × cantidad trae más de dos decimales; cada línea se cobra
        // redondeada, igual que la muestra el punto de venta.
        let sub = redondear_2(p.precio * p.cantidad);
        let desc = p.descuento_monto.unwrap_or(0.0).max(0.0).min(sub);
        subtotal += sub;
        descuento_total += desc;
    }

    // ------------------------------------------------------------------
    // Desde aquí se ESCRIBE, y todo va en una sola transacción: la venta,
    // el stock, los detalles, el crédito, el pedido de mesa y el cambio de
    // prenda se guardan juntos o no se guarda nada. Así un corte con la base
    // a mitad de camino nunca deja una venta cobrada sin productos ni stock.
    // Si el corte ocurre antes de confirmar, se intenta una vez más.
    // ------------------------------------------------------------------
    let mut intento = 0;
    let (venta_id, folio, cambio_prenda) = loop {
        intento += 1;
        let tx = conn
            .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo iniciar la venta: {}", e)))?;
        let resultado: Result<(i64, String, Option<crate::handlers::cambios::CambioHecho>), (StatusCode, String)> = async {
            // Dentro de la transacción TODO pasa por `tx` (con el nombre
            // `conn` para que ningún paso use por error la conexión de afuera).
            let conn: &libsql::Connection = &tx;

            // 5. Insertar venta
            let (venta_id, folio): (i64, String) = {
                let mut filas = conn
                    .query(
                        "INSERT INTO ventas (folio, cliente_id, subtotal, descuento, total, metodo_pago, monto_recibido, cambio, usuario_id, estado,
                                             pago_efectivo, pago_otro, pago_otro_metodo)
                         VALUES ((SELECT 'V-' || ?1 || '-' || printf('%04d', COALESCE(MAX(CAST(substr(folio, -4) AS INTEGER)), 0) + 1)
                                    FROM ventas WHERE folio LIKE 'V-' || ?1 || '%'),
                                 ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'COMPLETADA', ?10, ?11, ?12)
                         RETURNING id, folio",
                        libsql::params![
                            fecha_actual.clone(), payload.cliente_id, subtotal, descuento_total, payload.total,
                            metodo_pago.clone(), monto_recibido, cambio, usuario_id,
                            pago_efectivo, pago_otro, pago_otro_metodo.clone()
                        ],
                    )
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar venta: {}", e)))?;
                let datos: (i64, String) = match filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                    Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default()),
                    None => return Err((StatusCode::INTERNAL_SERVER_ERROR, "No se pudo registrar la venta.".into())),
                };
                // La respuesta se lee hasta el final antes de seguir: si se suelta a
                // medias, el servidor de la base puede dar por perdida la sesión.
                while filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_some() {}
                datos
            };

            // 6. Descontar stock e insertar los detalles.
            // Con lo vendido de cada producto ya sumado: los que tienen
            // vencimiento, por lotes (FEFO); el resto, todos en UNA sentencia.
            let mut a_descontar: Vec<(i64, f64)> = Vec::new();
            let mut perecibles: Vec<(i64, f64)> = Vec::new();
            for p in &payload.productos {
                let (lleva_vencimiento, controla_stock) = datos_producto.get(&p.id).map(|d| (d.2, d.3)).unwrap_or((false, true));
                if !controla_stock {
                    // Preparado al momento: no hay stock que descontar.
                } else if lleva_vencimiento {
                    match perecibles.iter_mut().find(|(id, _)| *id == p.id) {
                        Some(previo) => previo.1 += p.cantidad,
                        None => perecibles.push((p.id, p.cantidad)),
                    }
                } else if let Some(previo) = a_descontar.iter_mut().find(|(id, _)| *id == p.id) {
                    previo.1 += p.cantidad;
                } else {
                    a_descontar.push((p.id, p.cantidad));
                }
            }
            descontar_stock_fefo(conn, &perecibles).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
            if !a_descontar.is_empty() {
                // Solo números (ids enteros y cantidades ya validadas como finitas):
                // se escriben directo en la sentencia.
                let casos: String = a_descontar.iter().map(|(id, cantidad)| format!(" WHEN {} THEN {}", id, cantidad)).collect();
                let lista: String = a_descontar.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
                conn.execute(
                    &format!("UPDATE productos SET stock = stock - CASE id{} ELSE 0 END WHERE id IN ({})", casos, lista),
                    (),
                )
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error al descontar stock: {}", e)))?;
            }

            // Detalles: un INSERT de varias filas (por tandas, por el límite de
            // parámetros de SQLite).
            const COLUMNAS_DETALLE: usize = 9;
            const FILAS_POR_TANDA: usize = 100;
            for tanda in payload.productos.chunks(FILAS_POR_TANDA) {
                let mut marcas: Vec<String> = Vec::with_capacity(tanda.len());
                let mut valores: Vec<libsql::Value> = Vec::with_capacity(tanda.len() * COLUMNAS_DETALLE);
                for (i, p) in tanda.iter().enumerate() {
                    let sub = redondear_2(p.precio * p.cantidad);
                    let desc = p.descuento_monto.unwrap_or(0.0).max(0.0).min(sub);
                    let total_linea = sub - desc;

                    // Nombre y unidad congelados al momento de la venta (migración 0005):
                    // si después se renombra el producto, esta venta sigue mostrando
                    // el nombre con el que se vendió, igual que su comprobante SUNAT.
                    let (nombre_base, unidad_venta) = datos_producto
                        .get(&p.id)
                        .map(|d| (d.0.clone(), d.1.clone()))
                        .unwrap_or_else(|| (p.nombre.clone(), "UNIDAD".to_string()));
                    // Las opciones de un pedido de mesa o las medidas van con el
                    // nombre: "Capuchino (Grande, Leche de almendras)".
                    let nombre_venta = match p.detalle.as_deref().map(str::trim) {
                        Some(detalle) if !detalle.is_empty() => {
                            format!("{} ({})", nombre_base, detalle.chars().take(120).collect::<String>())
                        }
                        _ => nombre_base,
                    };

                    let b = i * COLUMNAS_DETALLE;
                    marcas.push(format!(
                        "(?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{})",
                        b + 1, b + 2, b + 3, b + 4, b + 5, b + 6, b + 7, b + 8, b + 9
                    ));
                    valores.extend([
                        libsql::Value::Integer(venta_id),
                        libsql::Value::Integer(p.id),
                        libsql::Value::Real(p.cantidad),
                        libsql::Value::Real(p.precio),
                        libsql::Value::Real(sub),
                        libsql::Value::Real(desc),
                        libsql::Value::Real(total_linea),
                        libsql::Value::Text(nombre_venta),
                        libsql::Value::Text(unidad_venta),
                    ]);
                }
                conn.execute(
                    &format!(
                        "INSERT INTO detalles_venta (venta_id, producto_id, cantidad, precio_unitario, subtotal, descuento_linea, total_linea,
                                                     nombre_producto, unidad_medida)
                         VALUES {}",
                        marcas.join(", ")
                    ),
                    libsql::params_from_iter(valores),
                )
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar detalle: {}", e)))?;
            }

            // 6b. Deja congelado cómo se cobró el IGV (tasa del negocio y qué líneas
            //     son gravadas, exoneradas o inafectas). No puede hacer fallar la venta.
            //     La tasa ya se leyó en la primera consulta.
            crate::handlers::igv::congelar_con_tasa(&conn, venta_id, crate::logica::igv::tasa_valida(tasa_igv)).await;

            // 6b-2. Reporte de ganancias (solo si el negocio lo tiene encendido):
            //     queda guardado lo que costaba cada producto al venderlo. No
            //     puede hacer fallar la venta.
            if con_ganancias {
                crate::handlers::ganancias::congelar_costo(&conn, venta_id).await;
            }

            // 6c. Venta al crédito: se abre la cuenta por cobrar y, si el cliente
            //     dejó un adelanto, entra como su primer abono.
            if let (Some((adelanto, adelanto_metodo, vence)), Some(cliente_id)) = (&credito, payload.cliente_id) {
                crate::handlers::creditos::crear_para_venta(
                    &conn, venta_id, cliente_id, payload.total, vence, *adelanto, adelanto_metodo, usuario_id,
                )
                .await?;
            }

            // 6d. Si salió de una cotización, queda vendida (no puede hacer fallar la venta).
            if let Some(cotizacion_id) = payload.cotizacion_id {
                crate::handlers::cotizaciones::marcar_vendida(&conn, cotizacion_id, venta_id).await;
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

            // 8. Cambio de prenda: entra la devolución de lo que el cliente trajo.
            let cambio_hecho = match &cambio_prenda {
                Some(listo) => Some(
                    crate::handlers::cambios::registrar(&conn, listo, venta_id, &folio, payload.total, &metodo_pago, usuario_id).await?,
                ),
                None => None,
            };

            Ok((venta_id, folio, cambio_hecho))
        }
        .await;

        match resultado {
            Ok(datos) => {
                tx.commit().await.map_err(|e| {
                    eprintln!("❌ Venta sin confirmar (COMMIT): {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "No se pudo confirmar la venta por un corte con la base de datos. Antes de cobrar otra vez, revisa en Comprobantes si quedó registrada.".to_string(),
                    )
                })?;
                break datos;
            }
            Err(error) => {
                // Nada de lo anterior queda guardado.
                let _ = tx.rollback().await;
                if intento < 2 && es_corte_de_sesion(&error.1) {
                    eprintln!("⚠️  Corte con la base durante una venta, se reintenta: {}", error.1);
                    continue;
                }
                if es_corte_de_sesion(&error.1) {
                    eprintln!("❌ Venta no registrada por corte con la base: {}", error.1);
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Se cortó la conexión con la base de datos y la venta NO se registró. Vuelve a cobrar.".to_string(),
                    ));
                }
                return Err(error);
            }
        }
    };

    Ok(Json(VentaResult { venta_id, folio, cambio_prenda }))
}