use axum::{extract::Extension, Json, http::StatusCode};

use crate::models::venta::{NuevaVenta, VentaResult};
use crate::tenants::TenantDb;
use crate::models::auth::Claims;

const METODOS_PAGO_VALIDOS: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE_PLIN", "MIXTO"];
const METODOS_OTRO_MIXTO: &[&str] = &["TARJETA", "TRANSFERENCIA", "YAPE_PLIN"];

fn redondear_2(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

/// Medios posibles de la parte guardada en `pago_efectivo` de un mixto
/// (migración 0028). Efectivo se guarda como NULL.
const METODOS_SEGUNDO_MIXTO: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE", "PLIN"];

/// Reparto validado de un pago MIXTO, listo para guardar.
#[derive(Debug, Default, PartialEq)]
struct Reparto {
    /// La parte "de abajo" (antes siempre efectivo).
    pago_efectivo: Option<f64>,
    pago_otro: Option<f64>,
    pago_otro_metodo: Option<String>,
    /// Medio de `pago_efectivo`: None = efectivo; si no, TARJETA,
    /// TRANSFERENCIA, YAPE o PLIN.
    pago_efectivo_metodo: Option<String>,
}

/// Valida el reparto de un pago MIXTO. Para cualquier otro método devuelve
/// todo vacío: esas columnas solo se usan en ventas mixtas y el trigger de
/// caja las ignora en el resto. `billetera` es la de la parte de arriba
/// cuando fue Yape o Plin (llega ya como 'YAPE_PLIN').
fn validar_pago(payload: &NuevaVenta, billetera: Option<&str>) -> Result<Reparto, String> {
    if !METODOS_PAGO_VALIDOS.contains(&payload.metodo_pago.as_str()) {
        return Err(format!("Método de pago no válido: {}", payload.metodo_pago));
    }
    if payload.metodo_pago != "MIXTO" {
        return Ok(Reparto::default());
    }

    let efectivo = redondear_2(payload.pago_efectivo.unwrap_or(0.0));
    let otro = redondear_2(payload.pago_otro.unwrap_or(0.0));
    let metodo_otro = payload.pago_otro_metodo.clone().unwrap_or_default();
    let segundo = payload
        .pago_efectivo_metodo
        .as_deref()
        .map(|m| m.trim().to_uppercase())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "EFECTIVO".to_string());

    if !METODOS_OTRO_MIXTO.contains(&metodo_otro.as_str()) {
        return Err("En un pago mixto, el otro medio debe ser Tarjeta, Transferencia o Yape/Plin".into());
    }
    if !METODOS_SEGUNDO_MIXTO.contains(&segundo.as_str()) {
        return Err(format!("Medio de pago no válido en el pago mixto: {}", segundo));
    }
    // Los dos medios tienen que ser distintos (Yape + Yape no es mixto).
    let mismo = segundo == metodo_otro
        || (metodo_otro == crate::handlers::billeteras::YAPE_PLIN && billetera == Some(segundo.as_str()));
    if mismo {
        return Err("En un pago mixto los dos medios deben ser distintos".into());
    }
    if efectivo <= 0.0 || otro <= 0.0 {
        return Err("En un pago mixto, las dos partes deben ser mayores a cero".into());
    }
    // Comparación en céntimos: tolera 1 céntimo por redondeo, nada más.
    if ((efectivo + otro - payload.total) * 100.0).round().abs() > 1.0 {
        return Err(format!(
            "El pago mixto no cuadra: S/ {:.2} + S/ {:.2} debe sumar el total S/ {:.2}",
            efectivo, otro, payload.total
        ));
    }
    let es_efectivo = segundo == "EFECTIVO";
    if es_efectivo {
        if let Some(recibido) = payload.monto_recibido {
            if recibido + 0.005 < efectivo {
                return Err(format!(
                    "El efectivo recibido (S/ {:.2}) no alcanza para la parte en efectivo (S/ {:.2})",
                    recibido, efectivo
                ));
            }
        }
    }
    Ok(Reparto {
        pago_efectivo: Some(efectivo),
        pago_otro: Some(otro),
        pago_otro_metodo: Some(metodo_otro),
        pago_efectivo_metodo: if es_efectivo { None } else { Some(segundo) },
    })
}

/// Revisa las líneas de la venta y calcula el total EN EL SERVIDOR, igual que
/// el punto de venta: cada línea al céntimo (precio × cantidad) menos su
/// descuento. Antes se guardaba el total que mandaba el navegador sin
/// compararlo, y ese total es el que usan la caja y el comprobante.
/// Devuelve (subtotal, descuento, total).
fn calcular_total(productos: &[crate::models::venta::ProductoVenta], total_enviado: f64) -> Result<(f64, f64, f64), String> {
    if productos.is_empty() {
        return Err("La venta no tiene productos.".into());
    }
    let mut subtotal = 0.0f64;
    let mut descuento = 0.0f64;
    for p in productos {
        if !(p.cantidad > 0.0) || !p.cantidad.is_finite() {
            return Err(format!("La cantidad de {} debe ser mayor a 0", p.nombre));
        }
        if !(p.precio >= 0.0) || !p.precio.is_finite() {
            return Err(format!("El precio de {} no es válido.", p.nombre));
        }
        if let Some(d) = p.descuento_monto {
            if !d.is_finite() || d < 0.0 {
                return Err(format!("El descuento de {} no es válido.", p.nombre));
            }
        }
        let sub = redondear_2(p.precio * p.cantidad);
        subtotal += sub;
        descuento += p.descuento_monto.unwrap_or(0.0).min(sub);
    }
    let total = redondear_2(subtotal - descuento);
    // Tolera 1 céntimo de redondeo, nada más.
    if !total_enviado.is_finite() || ((total_enviado - total) * 100.0).round().abs() > 1.0 {
        return Err(format!(
            "El total de la venta (S/ {:.2}) no coincide con la suma de los productos (S/ {:.2}). Vuelve a cargar la página e inténtalo de nuevo.",
            total_enviado, total
        ));
    }
    Ok((subtotal, descuento, total))
}

/// ¿Ese precio de línea está permitido? Sin el módulo "Cambiar precio al
/// vender", el precio es el del producto; si la venta sale de una
/// cotización, también vale el precio que se le cotizó al cliente.
fn precio_permitido(precio: f64, precio_producto: f64, precios_cotizados: &[f64]) -> bool {
    let igual = |a: f64, b: f64| (a - b).abs() < 1e-6;
    igual(precio, precio_producto) || precios_cotizados.iter().any(|c| igual(precio, *c))
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
// Si la base tiene la tabla venta_lotes (migración 0033), anota de qué lote
// salió cada cantidad, en el mismo viaje que el descuento.
async fn descontar_stock_fefo(conn: &libsql::Connection, venta_id: i64, pedidos: &[(i64, f64)]) -> Result<(), String> {
    if pedidos.is_empty() {
        return Ok(());
    }
    // Los ids son enteros: se escriben directo en la lista.
    let productos = pedidos.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
    // Un lote vencido NO se vende: vale hasta el día de su vencimiento
    // inclusive (hora de Lima). Se leen también los vencidos solo para
    // poder decir cuántos hay en el mensaje de error.
    let hoy = crate::handlers::carta::hoy_lima();
    let mut rows = conn
        .query(
            &format!(
                "SELECT l.id, l.producto_id, CAST(l.cantidad AS REAL), p.nombre,
                        CASE WHEN date(l.fecha_vencimiento) < ?1 THEN 1 ELSE 0 END,
                        (SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'venta_lotes')
                 FROM lotes_producto l JOIN productos p ON p.id = l.producto_id
                 WHERE l.producto_id IN ({}) AND l.activo = 1 AND l.cantidad > 0
                 ORDER BY l.producto_id, l.fecha_vencimiento ASC, l.id ASC",
                productos
            ),
            libsql::params![hoy],
        )
        .await
        .map_err(|e| format!("Error al leer lotes: {}", e))?;

    // producto_id -> sus lotes VIGENTES (id, cantidad), del que vence antes
    // al que vence después; aparte, cuánto hay vencido y el nombre.
    let mut lotes: std::collections::HashMap<i64, Vec<(i64, f64)>> = std::collections::HashMap::new();
    let mut vencido: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    let mut nombres: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut anotar_lotes = false;
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        anotar_lotes = row.get::<i64>(5).unwrap_or(0) > 0;
        let producto_id = row.get::<i64>(1).unwrap_or_default();
        let cantidad = row.get::<f64>(2).unwrap_or(0.0);
        if let Ok(nombre) = row.get::<String>(3) {
            nombres.entry(producto_id).or_insert(nombre);
        }
        if row.get::<i64>(4).unwrap_or(0) == 1 {
            *vencido.entry(producto_id).or_insert(0.0) += cantidad;
        } else {
            lotes.entry(producto_id).or_default().push((row.get(0).unwrap_or_default(), cantidad));
        }
    }

    // (lote_id, cuánto se le descuenta, producto_id)
    let mut descuentos: Vec<(i64, f64, i64)> = Vec::new();
    for (producto_id, cantidad_requerida) in pedidos {
        let del_producto = lotes.get(producto_id).map(Vec::as_slice).unwrap_or(&[]);
        let disponible_total: f64 = del_producto.iter().map(|(_, c)| c).sum();
        if disponible_total + 1e-6 < *cantidad_requerida {
            let nombre = nombres.get(producto_id).cloned().unwrap_or_else(|| format!("Producto #{}", producto_id));
            let vencidos = vencido.get(producto_id).copied().unwrap_or(0.0);
            return Err(if vencidos > 1e-9 {
                format!(
                    "\"{}\": solo hay {} sin vencer (solicitado: {}). Hay {} en lotes vencidos que no se pueden vender; descártalos en Stock y lotes.",
                    nombre, redondear_2(disponible_total), redondear_2(*cantidad_requerida), redondear_2(vencidos)
                )
            } else {
                format!(
                    "\"{}\": stock insuficiente por vencimiento (disponible: {}, solicitado: {})",
                    nombre, redondear_2(disponible_total), redondear_2(*cantidad_requerida)
                )
            });
        }
        let mut restante = *cantidad_requerida;
        for (lote_id, cantidad_lote) in del_producto {
            if restante <= 1e-9 {
                break;
            }
            let a_descontar = cantidad_lote.min(restante);
            descuentos.push((*lote_id, a_descontar, *producto_id));
            restante -= a_descontar;
        }
    }
    if descuentos.is_empty() {
        return Ok(());
    }

    // Solo números ya validados: se escriben directo en la sentencia. El
    // trigger de lotes recalcula el stock de cada producto.
    let casos: String = descuentos.iter().map(|(id, cantidad, _)| format!(" WHEN {} THEN {}", id, cantidad)).collect();
    let lista: String = descuentos.iter().map(|(id, _, _)| id.to_string()).collect::<Vec<_>>().join(",");
    let mut sql = format!("UPDATE lotes_producto SET cantidad = MAX(cantidad - CASE id{} ELSE 0 END, 0) WHERE id IN ({});", casos, lista);
    if anotar_lotes {
        let filas: Vec<String> = descuentos
            .iter()
            .filter(|(_, cantidad, _)| *cantidad > 0.0)
            .map(|(lote_id, cantidad, producto_id)| format!("({}, {}, {}, {})", venta_id, producto_id, lote_id, cantidad))
            .collect();
        if !filas.is_empty() {
            sql.push_str(&format!(
                "\nINSERT INTO venta_lotes (venta_id, producto_id, lote_id, cantidad) VALUES {};",
                filas.join(", ")
            ));
        }
    }
    conn.execute_batch(&sql).await.map_err(|e| format!("Error al descontar lote: {}", e))?;
    Ok(())
}

/// Datos reales de los productos al momento de vender, todos en una sola
/// consulta: id -> (stock, lleva_vencimiento, nombre, unidad, controla_stock, precio).
/// Si la migración 0007 todavía no llegó a esta base (columna controla_stock
/// inexistente), se asume que controlan stock, como siempre -- así una
/// venta nunca falla por eso.
async fn datos_productos_para_venta(
    conn: &libsql::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, (f64, bool, Option<String>, Option<String>, bool, f64)>, String> {
    let mut datos = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(datos);
    }
    // Los ids son enteros: se escriben directo en la lista.
    let lista = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let consulta = conn
        .query(
            &format!("SELECT id, stock, lleva_vencimiento, nombre, unidad_medida, CAST(precio AS REAL), controla_stock FROM productos WHERE id IN ({})", lista),
            (),
        )
        .await;
    let (mut rows, con_columna) = match consulta {
        Ok(rows) => (rows, true),
        Err(_) => (
            conn.query(
                &format!("SELECT id, stock, lleva_vencimiento, nombre, unidad_medida, CAST(precio AS REAL) FROM productos WHERE id IN ({})", lista),
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
                if con_columna { row.get::<i64>(6).unwrap_or(1) == 1 } else { true },
                row.get::<f64>(5).unwrap_or(0.0),
            ),
        );
    }
    Ok(datos)
}

pub async fn procesar_venta(
    Extension(tenant): Extension<std::sync::Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(mut payload): Json<NuevaVenta>,
) -> Result<Json<VentaResult>, (StatusCode, String)> {
    // El autor de la venta es el usuario de la sesión (JWT), no el
    // usuario_id que manda el navegador.
    let usuario_id = claims.sub;

    // Yape y Plin llegan por separado ("YAPE" / "PLIN"), solos o como la
    // parte digital de un pago mixto. Se guardan como siempre ('YAPE_PLIN')
    // y la billetera queda aparte (migración 0019); todo lo que sigue ve
    // el método de siempre.
    let billetera: Option<&'static str> = {
        let (metodo, directa) = crate::handlers::billeteras::separar(&payload.metodo_pago);
        if directa.is_some() {
            payload.metodo_pago = metodo.to_string();
            directa
        } else if payload.metodo_pago == "MIXTO" {
            let (otro, del_mixto) = match payload.pago_otro_metodo.as_deref() {
                Some(m) => {
                    let (o, b) = crate::handlers::billeteras::separar(m);
                    (Some(o.to_string()), b)
                }
                None => (None, None),
            };
            if del_mixto.is_some() {
                payload.pago_otro_metodo = otro;
            }
            del_mixto
        } else {
            None
        }
    };
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
    let (rol, caja_abierta, tasa_igv, modulos): (Option<String>, Option<i64>, Option<f64>, Option<String>) = {
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
                f.get::<String>(3).ok(),
            ),
            None => (None, None, None, None),
        }
    };
    let con_ganancias = modulos.as_deref().is_some_and(crate::handlers::ganancias::encendido_en);
    // Módulo "Cambiar precio al vender": el cajero puede poner otro precio.
    let precio_libre = modulos
        .as_deref()
        .is_some_and(|m| m.split(',').any(|x| x.trim() == crate::handlers::rubros::MODULO_PRECIO_VENTA));

    // Mesero y Preparación (barra/cocina) no cobran.
    if matches!(rol.as_deref(), Some("MESERO") | Some("PREPARACION")) {
        return Err((StatusCode::FORBIDDEN, "Este usuario no cobra (mesero o barra/cocina). Pide al cajero que cobre la cuenta.".into()));
    }

    // 1. Verificar caja abierta.
    if caja_abierta.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Debes abrir una caja antes de procesar ventas".into()));
    }

    // 1a. Total calculado aquí, no el del navegador. Todo lo que sigue
    // (crédito, pago mixto, caja, comprobante) usa este total.
    let (subtotal, descuento_total, total_servidor) =
        calcular_total(&payload.productos, payload.total).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    payload.total = total_servidor;

    // 1b. Validar método de pago (y el reparto si es MIXTO) antes de
    // tocar stock o insertar nada.
    // Venta al crédito (módulo CREDITO): el pago queda pendiente. Se guarda
    // como MIXTO con efectivo 0 y todo el total en 'CREDITO' (ver migración
    // 0015), así la caja no cuenta dinero que todavía no entró.
    let credito = match &payload.credito {
        Some(c) => Some(crate::handlers::creditos::validar_venta(&conn, c, payload.cliente_id, payload.total).await?),
        None => None,
    };
    // 1b-0. Entrega de un pedido por encargo (módulo ENCARGOS): la venta es
    // por el total y lo que el cliente ya adelantó no se vuelve a cobrar. Se
    // guarda como MIXTO: la parte ya pagada en 'ADELANTO' (la caja no la
    // cuenta: entró como ingreso el día del adelanto) y el saldo por el medio
    // elegido. Una venta normal no hace nada aquí.
    if payload.encargo_id.is_some()
        && (credito.is_some()
            || payload.pedido_id.is_some()
            || payload.cotizacion_id.is_some()
            || payload.cambio_prenda.is_some()
            || payload.orden_servicio_id.is_some())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Un pedido por encargo se entrega solo: sin crédito, pedido de mesa, cotización, cambio de prenda ni orden de servicio.".into(),
        ));
    }
    let encargo_cobro = crate::handlers::encargos::preparar_cobro(&conn, modulos.as_deref(), payload.encargo_id).await?;
    let adelanto_encargo = encargo_cobro.as_ref().map(|c| c.adelanto).filter(|a| *a > 0.0);
    if let Some(adelanto) = adelanto_encargo {
        if adelanto > payload.total + 0.005 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "El total de la venta (S/ {:.2}) es menor que lo que el cliente ya adelantó (S/ {:.2}).",
                    payload.total, adelanto
                ),
            ));
        }
        if !["EFECTIVO", "TARJETA", "TRANSFERENCIA", crate::handlers::billeteras::YAPE_PLIN].contains(&payload.metodo_pago.as_str()) {
            return Err((StatusCode::BAD_REQUEST, "Cobra el saldo del pedido con un solo medio de pago.".into()));
        }
    }

    let (metodo_pago, pago_efectivo, pago_otro, pago_otro_metodo, pago_efectivo_metodo, monto_recibido, cambio) = if credito.is_some() {
        (
            "MIXTO".to_string(),
            Some(0.0),
            Some(redondear_2(payload.total)),
            Some(crate::handlers::creditos::METODO_CREDITO.to_string()),
            None,
            None,
            None,
        )
    } else if let Some(adelanto) = adelanto_encargo {
        let saldo = redondear_2((payload.total - adelanto).max(0.0));
        // El medio del saldo va en `pago_efectivo_metodo` (None = efectivo).
        let medio_saldo = match payload.metodo_pago.as_str() {
            "EFECTIVO" => None,
            m if m == crate::handlers::billeteras::YAPE_PLIN => Some(billetera.unwrap_or("TRANSFERENCIA").to_string()),
            otro => Some(otro.to_string()),
        };
        let (recibido, vuelto) = if medio_saldo.is_none() {
            if let Some(r) = payload.monto_recibido {
                if r + 0.005 < saldo {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("El efectivo recibido (S/ {:.2}) no alcanza para el saldo (S/ {:.2}).", r, saldo),
                    ));
                }
            }
            (payload.monto_recibido, payload.cambio)
        } else {
            (None, None)
        };
        (
            "MIXTO".to_string(),
            Some(saldo),
            Some(adelanto),
            Some(crate::handlers::encargos::METODO_ADELANTO.to_string()),
            medio_saldo,
            recibido,
            vuelto,
        )
    } else {
        let r = validar_pago(&payload, billetera).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        // Mixto sin efectivo (p. ej. Yape + Plin): no hay efectivo recibido ni vuelto.
        let (recibido, vuelto) = if r.pago_efectivo_metodo.is_some() {
            (None, None)
        } else {
            (payload.monto_recibido, payload.cambio)
        };
        (payload.metodo_pago.clone(), r.pago_efectivo, r.pago_otro, r.pago_otro_metodo, r.pago_efectivo_metodo, recibido, vuelto)
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

    // 2a. Control de recetas (módulo RECETAS, farmacia): los medicamentos que
    // piden receta no se cobran sin ella. Se revisa antes de tocar nada; un
    // negocio sin el módulo no hace esta consulta.
    let receta_lista = {
        let pedidos: Vec<(i64, f64)> = ids.iter().map(|id| (*id, pedido_por_producto[id])).collect();
        crate::handlers::recetas::preparar(&conn, modulos.as_deref(), payload.receta.as_ref(), &pedidos).await?
    };
    // 2a-2. Placa y kilometraje (módulo VEHICULOS): se revisan antes de
    // tocar nada. Una venta sin placa no hace nada aquí.
    let vehiculo_listo = crate::handlers::vehiculos::preparar(modulos.as_deref(), payload.vehiculo.as_ref())?;
    // 2a-3. Cobro de una orden de servicio (módulo ORDENES_SERVICIO): debe
    // seguir abierta o terminada. Una venta normal no hace nada aquí.
    if payload.orden_servicio_id.is_some()
        && (payload.pedido_id.is_some() || payload.cotizacion_id.is_some() || payload.cambio_prenda.is_some())
    {
        return Err((StatusCode::BAD_REQUEST, "Una orden de servicio se cobra sola: sin pedido de mesa, cotización ni cambio de prenda.".into()));
    }
    let orden_cobro =
        crate::handlers::ordenes::preparar_cobro(&conn, modulos.as_deref(), payload.orden_servicio_id).await?;
    let mut leidos = datos_productos_para_venta(&conn, &ids)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Precio de cada línea: el del producto, salvo con el módulo "Cambiar
    // precio al vender" o al cobrar un pedido de mesa (sus precios los puso
    // el servidor, con las opciones, y el total se compara con el pedido).
    // Una venta que sale de una cotización pendiente puede llevar el precio
    // cotizado. Antes el navegador podía mandar cualquier precio.
    let revisar_precios = !precio_libre && payload.pedido_id.is_none();

    // Presentaciones y precio por mayor (migración 0030, handlers/presentaciones.rs).
    // Solo se consultan si la venta los usa: una venta común hace las mismas
    // consultas de siempre.
    let mut especiales: Option<std::collections::HashMap<i64, crate::handlers::presentaciones::Especiales>> = None;
    if payload.productos.iter().any(|p| p.presentacion_id.is_some()) {
        let esp = crate::handlers::presentaciones::especiales_de(&conn, &ids).await;
        for p in payload.productos.iter_mut() {
            let Some(pid) = p.presentacion_id else { continue };
            let Some((nombre_pres, factor, precio_pres)) =
                esp.get(&p.id).and_then(|e| e.presentaciones.get(&pid)).cloned()
            else {
                return Err((
                    StatusCode::CONFLICT,
                    format!("La presentación de {} cambió. Quítalo del carrito y vuelve a agregarlo.", p.nombre),
                ));
            };
            let Some(n) = crate::handlers::presentaciones::presentaciones_en(p.cantidad, factor) else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("La cantidad de {} no corresponde a \"{}\". Vuelve a agregarlo.", p.nombre, nombre_pres),
                ));
            };
            let precio_esperado = crate::handlers::presentaciones::precio_unitario_de(precio_pres, factor);
            if revisar_precios && (p.precio - precio_esperado).abs() >= 1e-6 {
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "El precio de {} ({}) cambió: ahora es S/ {:.2}. Quítalo del carrito y vuelve a agregarlo.",
                        p.nombre, nombre_pres, precio_pres
                    ),
                ));
            }
            // El nombre en la venta y el comprobante: "Cable (2 × Rollo x 100 m)".
            p.detalle = Some(format!("{} × {}", n, nombre_pres));
        }
        especiales = Some(esp);
    }

    let mut cotizados: std::collections::HashMap<i64, Vec<f64>> = std::collections::HashMap::new();
    if revisar_precios {
        if let Some(cotizacion_id) = payload.cotizacion_id {
            let mut filas = conn
                .query(
                    "SELECT ci.producto_id, CAST(ci.precio_unitario AS REAL) FROM cotizacion_items ci
                     JOIN cotizaciones c ON c.id = ci.cotizacion_id
                     WHERE c.id = ?1 AND c.estado = 'PENDIENTE'",
                    libsql::params![cotizacion_id],
                )
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            while let Some(f) = filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                cotizados.entry(f.get::<i64>(0).unwrap_or_default()).or_default().push(f.get::<f64>(1).unwrap_or(-1.0));
            }
        }
        // Una orden de servicio se cobra con los precios con que se agregó cada cosa.
        if let Some((_, precios)) = &orden_cobro {
            for (producto, lista) in precios {
                cotizados.entry(*producto).or_default().extend(lista.iter().copied());
            }
        }
        // Un pedido por encargo, con los precios acordados con el cliente.
        if let Some(c) = &encargo_cobro {
            for (producto, lista) in &c.precios {
                cotizados.entry(*producto).or_default().extend(lista.iter().copied());
            }
        }
        for p in &payload.productos {
            // Las de presentación ya se revisaron arriba.
            if p.presentacion_id.is_some() {
                continue;
            }
            let Some(datos) = leidos.get(&p.id) else { continue };
            let precio_producto = datos.5;
            let de_cotizacion = cotizados.get(&p.id).map(Vec::as_slice).unwrap_or(&[]);
            if precio_permitido(p.precio, precio_producto, de_cotizacion) {
                continue;
            }
            // Precio por mayor (o el de una presentación cuando se lleva su
            // cantidad o más): se consulta solo si el precio no es el de siempre.
            if especiales.is_none() {
                especiales = Some(crate::handlers::presentaciones::especiales_de(&conn, &ids).await);
            }
            let por_mayor = especiales
                .as_ref()
                .and_then(|e| e.get(&p.id))
                .is_some_and(|e| crate::handlers::presentaciones::precio_por_volumen_valido(p.precio, p.cantidad, e));
            if !por_mayor {
                let nombre = datos.2.clone().unwrap_or_else(|| p.nombre.clone());
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "El precio de {} cambió: ahora es S/ {:.2}. Quítalo del carrito y vuelve a agregarlo.",
                        nombre, precio_producto
                    ),
                ));
            }
        }
    } else if precio_libre {
        // Con precio libre, igual nada en cero (salvo que el producto valga 0).
        for p in &payload.productos {
            if p.precio <= 0.0 && leidos.get(&p.id).is_some_and(|d| d.5 > 0.0) {
                return Err((StatusCode::BAD_REQUEST, format!("El precio de {} debe ser mayor a 0.", p.nombre)));
            }
        }
    }
    for p in &payload.productos {
        // Se saca del mapa la primera vez; si el producto se repite en otra
        // línea ya quedó en `datos_producto` y pasó la validación de stock.
        if datos_producto.contains_key(&p.id) {
            continue;
        }
        if let Some((stock, lleva_vencimiento, nombre, unidad, controla_stock, _precio)) = leidos.remove(&p.id) {
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
    let fecha_actual = crate::logica::tiempo::hoy_lima_compacto();

    // 4. Subtotal, descuento y total ya se calcularon en el paso 1a (cada
    // línea al céntimo, igual que el punto de venta).

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
                // La columna del medio de la segunda parte (migración 0028)
                // solo se nombra cuando hace falta: una venta de siempre
                // entra igual aunque la base aún no la tenga. El trigger de
                // caja la lee al insertar, por eso va en el mismo INSERT.
                let (columna_segundo, valor_segundo) = if pago_efectivo_metodo.is_some() {
                    (", pago_efectivo_metodo", ", ?13")
                } else {
                    ("", "")
                };
                let sql = format!(
                    "INSERT INTO ventas (folio, cliente_id, subtotal, descuento, total, metodo_pago, monto_recibido, cambio, usuario_id, estado,
                                         pago_efectivo, pago_otro, pago_otro_metodo{columna_segundo})
                     VALUES ((SELECT 'V-' || ?1 || '-' || printf('%04d', COALESCE(MAX(CAST(substr(folio, -4) AS INTEGER)), 0) + 1)
                                FROM ventas WHERE folio LIKE 'V-' || ?1 || '%'),
                             ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'COMPLETADA', ?10, ?11, ?12{valor_segundo})
                     RETURNING id, folio"
                );
                let mut parametros = vec![
                    libsql::Value::from(fecha_actual.clone()),
                    payload.cliente_id.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                    libsql::Value::from(subtotal),
                    libsql::Value::from(descuento_total),
                    libsql::Value::from(payload.total),
                    libsql::Value::from(metodo_pago.clone()),
                    monto_recibido.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                    cambio.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                    libsql::Value::from(usuario_id),
                    pago_efectivo.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                    pago_otro.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                    pago_otro_metodo.clone().map(libsql::Value::from).unwrap_or(libsql::Value::Null),
                ];
                if let Some(m) = &pago_efectivo_metodo {
                    parametros.push(libsql::Value::from(m.clone()));
                }
                let mut filas = conn
                    .query(&sql, libsql::params_from_iter(parametros))
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
            descontar_stock_fefo(conn, venta_id, &perecibles).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
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

            // 6b-3. Yape y Plin por separado: queda anotada la billetera y lo
            //     cobrado por ella en la caja. Solo en ventas con Yape/Plin, y
            //     no puede hacer fallar la venta.
            let por_billetera = if metodo_pago == crate::handlers::billeteras::YAPE_PLIN {
                payload.total
            } else if metodo_pago == "MIXTO" && pago_otro_metodo.as_deref() == Some(crate::handlers::billeteras::YAPE_PLIN) {
                pago_otro.unwrap_or(0.0)
            } else {
                0.0
            };
            if por_billetera > 0.0 {
                crate::handlers::billeteras::anotar_venta(&conn, venta_id, usuario_id, billetera, por_billetera).await;
            }
            // Mixto cuya otra parte también fue por Yape o Plin (p. ej. Yape +
            // Plin): esa parte suma en la línea de su billetera.
            if let Some(b @ ("YAPE" | "PLIN")) = pago_efectivo_metodo.as_deref() {
                crate::handlers::billeteras::sumar_en_caja(&conn, usuario_id, Some(b), pago_efectivo.unwrap_or(0.0)).await;
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

            // 6e. Constancia de la receta (módulo RECETAS): entra con la venta o no entra.
            if let Some(lista) = &receta_lista {
                crate::handlers::recetas::registrar(&conn, lista, venta_id, usuario_id, &claims.nombre_completo).await?;
            }

            // 6f-0. Orden de servicio cobrada: queda COBRADA con esta venta (si otra
            //     caja la cobró antes, esta venta no entra).
            if let Some((orden_id, _)) = &orden_cobro {
                crate::handlers::ordenes::marcar_cobrada(&conn, *orden_id, venta_id).await?;
            }

            // 6f-1. Pedido por encargo entregado: queda ENTREGADO con esta venta
            //     (si otra caja lo entregó antes, esta venta no entra).
            if let Some(c) = &encargo_cobro {
                crate::handlers::encargos::marcar_entregado(&conn, c, venta_id).await?;
            }

            // 6f. Placa y kilometraje (módulo VEHICULOS): entran con la venta o no entra.
            if let Some(v) = &vehiculo_listo {
                crate::handlers::vehiculos::registrar(&conn, v, venta_id, payload.cliente_id).await?;
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
            // Lo devuelto en el cambio sale por el mismo medio: si fue Yape o
            // Plin, también de su línea en la caja.
            if let Some(hecho) = &cambio_hecho {
                if metodo_pago == crate::handlers::billeteras::YAPE_PLIN {
                    crate::handlers::billeteras::descontar(&conn, usuario_id, billetera, hecho.valor_devuelto).await;
                }
            }

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

    // Cambio de prenda con nota de crédito pedida por el cajero: va después
    // de confirmar la venta (la nota no deshace nada si falla; se avisa).
    let mut cambio_prenda = cambio_prenda;
    if let Some(hecho) = cambio_prenda.as_mut() {
        if payload.cambio_prenda.as_ref().is_some_and(|c| c.emitir_nota_credito) {
            hecho.nota_credito =
                crate::handlers::notas_credito::emitir_por_devolucion(&conn, hecho.devolucion_id, Some(usuario_id)).await;
        }
    }

    Ok(Json(VentaResult { venta_id, folio, cambio_prenda }))
}
#[cfg(test)]
mod pruebas_pago {
    use super::*;

    fn venta(json: serde_json::Value) -> NuevaVenta {
        let mut base = serde_json::json!({
            "productos": [], "subtotal": 50.0, "descuento": 0.0, "total": 50.0,
            "metodo_pago": "MIXTO", "monto_recibido": null, "cambio": null, "usuario_id": 1, "cliente_id": null
        });
        for (k, v) in json.as_object().unwrap() {
            base[k] = v.clone();
        }
        serde_json::from_value(base).expect("venta de prueba")
    }

    #[test]
    fn mixto_de_siempre_sigue_igual() {
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "YAPE_PLIN", "monto_recibido": 20.0}));
        let r = validar_pago(&v, Some("YAPE")).unwrap();
        assert_eq!(r.pago_efectivo_metodo, None);
        assert_eq!((r.pago_efectivo, r.pago_otro), (Some(20.0), Some(30.0)));
        // Efectivo que no alcanza.
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "TARJETA", "monto_recibido": 10.0}));
        assert!(validar_pago(&v, None).is_err());
    }

    #[test]
    fn yape_mas_plin() {
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "YAPE_PLIN", "pago_efectivo_metodo": "PLIN"}));
        let r = validar_pago(&v, Some("YAPE")).unwrap();
        assert_eq!(r.pago_efectivo_metodo.as_deref(), Some("PLIN"));
        // "EFECTIVO" explícito es lo mismo que nada.
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "TARJETA", "pago_efectivo_metodo": "EFECTIVO"}));
        assert_eq!(validar_pago(&v, None).unwrap().pago_efectivo_metodo, None);
    }

    #[test]
    fn rechaza_mismo_medio_y_descuadre() {
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "YAPE_PLIN", "pago_efectivo_metodo": "YAPE"}));
        assert!(validar_pago(&v, Some("YAPE")).is_err());
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "TARJETA", "pago_efectivo_metodo": "TARJETA"}));
        assert!(validar_pago(&v, None).is_err());
        let v = venta(serde_json::json!({"pago_efectivo": 10.0, "pago_otro": 30.0, "pago_otro_metodo": "TARJETA", "pago_efectivo_metodo": "PLIN"}));
        assert!(validar_pago(&v, None).is_err());
        let v = venta(serde_json::json!({"pago_efectivo": 20.0, "pago_otro": 30.0, "pago_otro_metodo": "TARJETA", "pago_efectivo_metodo": "CREDITO"}));
        assert!(validar_pago(&v, None).is_err());
    }
}

#[cfg(test)]
mod pruebas_total {
    use super::*;
    use crate::models::venta::ProductoVenta;

    fn linea(precio: f64, cantidad: f64) -> ProductoVenta {
        ProductoVenta { id: 1, nombre: "Polo".into(), precio, cantidad, descuento_monto: None, detalle: None, presentacion_id: None }
    }

    #[test]
    fn calcula_igual_que_el_punto_de_venta() {
        // 37.5 pies a S/ 4.35 = 163.125 -> 163.13 por línea; 3 × 26.666667 = 80.00.
        let l = [linea(4.35, 37.5), linea(26.666667, 3.0), linea(59.0, 2.0)];
        let (sub, desc, total) = calcular_total(&l, 361.13).unwrap();
        assert_eq!((redondear_2(sub), desc, total), (361.13, 0.0, 361.13));
        // Un céntimo de diferencia se tolera; el total que se guarda es el del servidor.
        assert_eq!(calcular_total(&l, 361.12).unwrap().2, 361.13);
    }

    #[test]
    fn rechaza_total_inventado_y_precios_raros() {
        let l = [linea(100.0, 1.0)];
        assert!(calcular_total(&l, 10.0).unwrap_err().contains("no coincide"));
        assert!(calcular_total(&l, f64::NAN).is_err());
        assert!(calcular_total(&[linea(-5.0, 1.0)], -5.0).is_err());
        assert!(calcular_total(&[linea(f64::INFINITY, 1.0)], 0.0).is_err());
        assert!(calcular_total(&[linea(10.0, 0.0)], 0.0).is_err());
        assert!(calcular_total(&[], 0.0).is_err());
        let mut con_desc = linea(10.0, 1.0);
        con_desc.descuento_monto = Some(-3.0);
        assert!(calcular_total(&[con_desc], 13.0).is_err());
    }

    #[test]
    fn descuento_por_linea_como_antes() {
        let mut l = linea(50.0, 2.0);
        l.descuento_monto = Some(10.0);
        assert_eq!(calcular_total(&[l], 90.0).unwrap(), (100.0, 10.0, 90.0));
        // Un descuento mayor que la línea se limita a la línea.
        let mut l = linea(5.0, 1.0);
        l.descuento_monto = Some(8.0);
        assert_eq!(calcular_total(&[l], 0.0).unwrap().2, 0.0);
    }

    #[test]
    fn precio_del_producto_o_el_cotizado() {
        assert!(precio_permitido(59.0, 59.0, &[]));
        assert!(precio_permitido(26.666667, 26.666667, &[]));
        assert!(!precio_permitido(1.0, 59.0, &[]));
        assert!(!precio_permitido(65.0, 59.0, &[]));
        assert!(precio_permitido(55.0, 59.0, &[55.0]));
        assert!(!precio_permitido(50.0, 59.0, &[55.0]));
    }
}
