use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::devolucion::*;
use crate::models::auth::Claims;

pub async fn buscar_venta_para_devolucion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(identificador): Path<String>,
) -> Result<Json<VentaParaDevolucion>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let texto = identificador.trim().to_uppercase();

    let mut venta_id_opt: Option<i64> = None;

    let mut rows = conn
        .query("SELECT id FROM ventas WHERE folio = ?1 AND estado = 'COMPLETADA'", libsql::params![texto.clone()])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(row) = rows.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        venta_id_opt = Some(row.get(0).unwrap_or_default());
    }

    if venta_id_opt.is_none() {
        if let Some((serie, numero_str)) = texto.rsplit_once('-') {
            let numero_limpio = numero_str.trim_start_matches('0');
            let numero_parseado: i64 = if numero_limpio.is_empty() { 0 } else {
                numero_limpio.parse().unwrap_or(-1)
            };
            if numero_parseado >= 0 {
                let mut rows2 = conn
                    .query(
                        "SELECT venta_id FROM comprobantes_electronicos WHERE serie = ?1 AND numero = ?2",
                        libsql::params![serie.to_string(), numero_parseado],
                    )
                    .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                if let Some(row) = rows2.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                    venta_id_opt = Some(row.get(0).unwrap_or_default());
                }
            }
        }
    }

    let venta_id = match venta_id_opt {
        Some(id) => id,
        None => return Err((StatusCode::NOT_FOUND, "No se encontró ninguna venta con ese folio o número de comprobante".into())),
    };

    let mut rv = conn
        .query(
            "SELECT id, folio, fecha_hora, total, metodo_pago, pago_efectivo, pago_otro, pago_otro_metodo FROM ventas WHERE id = ?1",
            libsql::params![venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match rv.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Venta no encontrada".into())),
    };

    let folio_venta: String = row.get(1).unwrap_or_default();
    let fecha_hora: String = row.get(2).unwrap_or_default();
    let total: f64 = row.get(3).unwrap_or_default();
    let metodo_pago: String = row.get(4).unwrap_or_default();
    let pago_efectivo: Option<f64> = row.get(5).ok().flatten();
    let pago_otro: Option<f64> = row.get(6).ok().flatten();
    let pago_otro_metodo: Option<String> = row.get(7).ok().flatten();
    // Yape y Plin por separado: si la venta guardó su billetera, se muestra
    // esa ("YAPE" o "PLIN") en vez de "YAPE_PLIN".
    let (metodo_pago, pago_otro_metodo) = match crate::handlers::billeteras::de_venta(&conn, venta_id).await {
        Some(billetera) => {
            let detallar = |m: String| if m == crate::handlers::billeteras::YAPE_PLIN { billetera.clone() } else { m };
            (detallar(metodo_pago), pago_otro_metodo.map(detallar))
        }
        None => (metodo_pago, pago_otro_metodo),
    };

    let mut rows_det = conn
        .query(
            "SELECT dv.id, dv.producto_id, COALESCE(dv.nombre_producto, p.nombre), dv.cantidad, dv.precio_unitario, dv.total_linea
             FROM detalles_venta dv JOIN productos p ON dv.producto_id = p.id
             WHERE dv.venta_id = ?1 ORDER BY dv.id",
            libsql::params![venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut productos = Vec::new();
    while let Some(r) = rows_det.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        productos.push(ProductoVentaDetalle {
            detalle_id: r.get(0).unwrap_or_default(),
            producto_id: r.get(1).unwrap_or_default(),
            nombre: r.get(2).unwrap_or_default(),
            cantidad: r.get(3).unwrap_or_default(),
            precio_unitario: r.get(4).unwrap_or_default(),
            subtotal: r.get(5).unwrap_or_default(),
        });
    }

    let mut rc = conn
        .query(
            "SELECT tipo, serie, numero FROM comprobantes_electronicos WHERE venta_id = ?1 ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let comprobante = match rc.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => Some(ComprobanteInfo {
            tipo: r.get(0).unwrap_or_default(),
            serie: r.get(1).unwrap_or_default(),
            numero: r.get(2).unwrap_or_default(),
        }),
        None => None,
    };

    let pago_efectivo_metodo = if metodo_pago == "MIXTO" { medio_segunda_parte(&conn, venta_id).await } else { None };

    Ok(Json(VentaParaDevolucion {
        venta_id, folio: folio_venta, fecha_hora, total, metodo_pago,
        pago_efectivo, pago_otro, pago_otro_metodo, pago_efectivo_metodo,
        productos, comprobante,
    }))
}

/// Medio de la parte `pago_efectivo` de una venta mixta (migración 0028):
/// None = efectivo, o si la base aún no tiene la columna.
pub async fn medio_segunda_parte(conn: &libsql::Connection, venta_id: i64) -> Option<String> {
    let mut filas = conn
        .query("SELECT pago_efectivo_metodo FROM ventas WHERE id = ?1", libsql::params![venta_id])
        .await
        .ok()?;
    let fila = filas.next().await.ok()??;
    fila.get::<String>(0).ok().filter(|m| !m.is_empty() && m != "EFECTIVO")
}

pub async fn procesar_devolucion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevaDevolucion>,
) -> Result<Json<DevolucionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (devolucion_id, folio_devolucion) = registrar_devolucion(&conn, claims.sub, &payload).await?;

    // Si la venta tiene boleta o factura emitida directo a SUNAT, la nota de
    // crédito de lo devuelto sale sola. La devolución ya quedó registrada:
    // si la nota falla, se avisa y se puede emitir desde Comprobantes.
    let nota_credito = crate::handlers::notas_credito::emitir_por_devolucion(&conn, devolucion_id, Some(claims.sub)).await;

    Ok(Json(DevolucionResponse {
        success: true,
        message: "Devolución procesada exitosamente".into(),
        folio_devolucion: Some(folio_devolucion),
        nota_credito,
    }))
}

/// Registra una devolución: revisa las cantidades, devuelve el dinero por el
/// medio que corresponde (la caja y Yape/Plin) y el stock (trigger de
/// detalles_devolucion). La usan Devoluciones y la anulación de un
/// comprobante. Devuelve (id, folio) de la devolución.
///
/// Todo va en UNA transacción: la revisión de lo ya devuelto y lo que se
/// escribe. Antes un doble clic podía devolver el dinero dos veces, y un
/// corte a mitad dejaba la caja movida sin el stock (o al revés).
pub async fn registrar_devolucion(
    conn: &libsql::Connection,
    usuario_id: i64,
    payload: &NuevaDevolucion,
) -> Result<(i64, String), (StatusCode, String)> {
    if payload.productos.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Elige qué productos se devuelven.".into()));
    }
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo iniciar la devolución: {}", e)))?;
    let resultado = registrar_en(&tx, usuario_id, payload).await;
    match resultado {
        Ok(hecho) => {
            tx.commit().await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("No se pudo confirmar la devolución (revisa en el historial si quedó registrada): {}", e),
                )
            })?;
            Ok(hecho)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

/// Una línea de la venta con lo que aún se puede devolver.
struct LineaVendida {
    detalle_id: i64,
    producto_id: i64,
    nombre: String,
    cantidad: f64,
    devuelto: f64,
    /// Lo que el cliente pagó por cada unidad (con su descuento).
    valor_unitario: f64,
}

/// Lo que se devuelve, ya revisado: (detalle_id, producto_id, cantidad,
/// valor unitario, subtotal) y el monto total.
fn revisar_lineas(
    lineas: &[LineaVendida],
    pedido: &[ProductoDevolver],
) -> Result<(Vec<(i64, i64, f64, f64, f64)>, f64), String> {
    let mut revisadas: Vec<(i64, i64, f64, f64, f64)> = Vec::with_capacity(pedido.len());
    let mut monto = 0.0f64;
    for p in pedido {
        if !(p.cantidad > 0.0) || !p.cantidad.is_finite() {
            return Err("La cantidad a devolver debe ser mayor a 0.".into());
        }
        if revisadas.iter().any(|r| r.0 == p.detalle_id) {
            return Err("Un producto está repetido en la devolución.".into());
        }
        // El producto y el precio salen de la venta, no del navegador.
        let linea = lineas
            .iter()
            .find(|l| l.detalle_id == p.detalle_id)
            .ok_or_else(|| "Uno de los productos no es de esa venta.".to_string())?;
        let disponible = linea.cantidad - linea.devuelto;
        if p.cantidad > disponible + 1e-6 {
            return Err(format!(
                "De \"{}\" no puedes devolver {}. Disponibles: {}",
                linea.nombre,
                p.cantidad,
                redondear(disponible.max(0.0))
            ));
        }
        let subtotal = redondear(linea.valor_unitario * p.cantidad);
        monto += subtotal;
        revisadas.push((linea.detalle_id, linea.producto_id, p.cantidad, linea.valor_unitario, subtotal));
    }
    Ok((revisadas, redondear(monto)))
}

fn redondear(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

async fn registrar_en(
    conn: &libsql::Connection,
    usuario_id: i64,
    payload: &NuevaDevolucion,
) -> Result<(i64, String), (StatusCode, String)> {
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());

    // La venta: tiene que existir y estar vigente.
    let mut r_venta = conn
        .query("SELECT estado, metodo_pago, pago_otro_metodo FROM ventas WHERE id = ?1", libsql::params![payload.venta_id])
        .await
        .map_err(e500)?;
    let (estado, metodo_original, otro_metodo_original): (String, String, Option<String>) =
        match r_venta.next().await.map_err(e500)? {
            Some(row) => (
                row.get(0).unwrap_or_else(|_| "COMPLETADA".to_string()),
                row.get(1).unwrap_or_else(|_| "EFECTIVO".to_string()),
                row.get::<Option<String>>(2).ok().flatten(),
            ),
            None => return Err((StatusCode::NOT_FOUND, "Venta no encontrada".into())),
        };
    drop(r_venta);
    if estado != "COMPLETADA" {
        return Err((StatusCode::BAD_REQUEST, "Esa venta no está vigente: no se puede devolver nada de ella.".into()));
    }

    // Sus líneas, con lo ya devuelto de cada una.
    let mut r_lineas = conn
        .query(
            "SELECT dv.id, dv.producto_id, COALESCE(dv.nombre_producto, p.nombre, ''),
                    CAST(dv.cantidad AS REAL),
                    CAST(COALESCE(dv.total_linea, dv.cantidad * dv.precio_unitario) AS REAL),
                    CAST(COALESCE((SELECT SUM(dd.cantidad_devuelta) FROM detalles_devolucion dd
                                     JOIN devoluciones d ON d.id = dd.devolucion_id
                                    WHERE dd.detalle_venta_id = dv.id AND d.estado = 'PROCESADA'), 0) AS REAL)
             FROM detalles_venta dv LEFT JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1",
            libsql::params![payload.venta_id],
        )
        .await
        .map_err(e500)?;
    let mut lineas: Vec<LineaVendida> = Vec::new();
    while let Some(f) = r_lineas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(3).unwrap_or(0.0);
        let total_linea: f64 = f.get(4).unwrap_or(0.0);
        lineas.push(LineaVendida {
            detalle_id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            nombre: f.get(2).unwrap_or_default(),
            cantidad,
            devuelto: f.get(5).unwrap_or(0.0),
            valor_unitario: if cantidad > 0.0 { total_linea / cantidad } else { 0.0 },
        });
    }
    drop(r_lineas);
    let (devueltas, monto_total) =
        revisar_lineas(&lineas, &payload.productos).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let fecha_actual = crate::logica::tiempo::hoy_lima_compacto();
    let mut rows_folio = conn
        .query(
            "SELECT COALESCE(MAX(CAST(substr(folio_devolucion,-4) AS INTEGER)),0)+1
             FROM devoluciones WHERE folio_devolucion LIKE 'DEV-' || ?1 || '%'",
            libsql::params![fecha_actual.clone()],
        )
        .await
        .map_err(e500)?;
    let siguiente: i64 = match rows_folio.next().await.map_err(e500)? {
        Some(r) => r.get(0).unwrap_or(1),
        None => 1,
    };
    drop(rows_folio);
    let folio_devolucion = format!("DEV-{}-{:04}", fecha_actual, siguiente);

    // En una venta MIXTO el cajero elige cómo devuelve el dinero: en
    // efectivo o por el mismo medio digital con el que se pagó la otra
    // parte. En el resto, se devuelve por el mismo método de la venta.
    // Mixto entre dos medios (migración 0028): la parte de `pago_efectivo`
    // pudo ser Yape, Plin, tarjeta o transferencia; también se puede
    // devolver por ese medio. Su billetera, si fue Yape o Plin, va aparte.
    let mut billetera_elegida: Option<String> = None;
    let metodo_para_caja = if metodo_original == "MIXTO" {
        let segundo = medio_segunda_parte(conn, payload.venta_id).await;
        // Sin elección, por el medio de esa parte (efectivo en un mixto de siempre).
        let elegido_crudo = payload
            .metodo_reembolso
            .clone()
            .unwrap_or_else(|| segundo.clone().unwrap_or_else(|| "EFECTIVO".to_string()));
        // "YAPE" o "PLIN" es el mismo medio que se guardó como 'YAPE_PLIN'.
        let elegido = crate::handlers::billeteras::separar(&elegido_crudo).0.to_string();
        let otro = otro_metodo_original.clone().unwrap_or_default();
        if segundo.as_deref() == Some(elegido_crudo.as_str()) {
            if elegido_crudo == "YAPE" || elegido_crudo == "PLIN" {
                billetera_elegida = Some(elegido_crudo.clone());
            }
            elegido
        } else if elegido == "EFECTIVO" {
            "EFECTIVO".to_string()
        } else if !otro.is_empty() && elegido == otro {
            // Venta al crédito: lo devuelto se descuenta de la deuda (si la
            // deuda pendiente no alcanza, hay que devolver en efectivo).
            if otro == crate::handlers::creditos::METODO_CREDITO {
                crate::handlers::creditos::descontar_devolucion(conn, payload.venta_id, monto_total).await?;
            }
            otro
        } else {
            let segunda = match segundo.as_deref() {
                Some(s) => format!(", por {}", s.replace('_', "/")),
                None => String::new(),
            };
            return Err((StatusCode::BAD_REQUEST, format!(
                "En esta venta mixta solo se puede devolver en efectivo, por {}{}",
                otro.replace('_', "/"),
                segunda
            )));
        }
    } else {
        metodo_original.clone()
    };
    // El trigger de caja solo distingue EFECTIVO/TARJETA/TRANSFERENCIA — Yape/Plin se agrupa como transferencia
    let metodo_reembolso = if metodo_para_caja == "YAPE_PLIN" { "TRANSFERENCIA" } else { metodo_para_caja.as_str() };

    conn.execute(
        "INSERT INTO devoluciones (venta_original_id, folio_devolucion, monto_reembolsado, metodo_reembolso, motivo, usuario_id, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'PROCESADA')",
        libsql::params![payload.venta_id, folio_devolucion.clone(), monto_total, metodo_reembolso, payload.motivo.clone(), usuario_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar devolución: {}", e)))?;

    let devolucion_id = conn.last_insert_rowid();

    // Devuelto por Yape o Plin: sale también de su línea en la caja (el
    // trigger ya lo restó del total de transferencias). No puede fallar.
    if metodo_para_caja == crate::handlers::billeteras::YAPE_PLIN {
        let billetera = match billetera_elegida {
            Some(b) => Some(b),
            None => crate::handlers::billeteras::de_venta(conn, payload.venta_id).await,
        };
        crate::handlers::billeteras::descontar(conn, usuario_id, billetera.as_deref(), monto_total).await;
    }

    for (detalle_id, producto_id, cantidad, valor_unitario, subtotal) in &devueltas {
        conn.execute(
            "INSERT INTO detalles_devolucion (devolucion_id, producto_id, detalle_venta_id, venta_id, cantidad_devuelta, precio_unitario, subtotal, condicion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'REVENTA')",
            libsql::params![devolucion_id, *producto_id, *detalle_id, payload.venta_id, *cantidad, *valor_unitario, *subtotal],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar detalle: {}", e)))?;
    }

    Ok((devolucion_id, folio_devolucion))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn linea(detalle_id: i64, cantidad: f64, total_linea: f64, devuelto: f64) -> LineaVendida {
        LineaVendida {
            detalle_id,
            producto_id: detalle_id * 10,
            nombre: format!("Producto {}", detalle_id),
            cantidad,
            devuelto,
            valor_unitario: total_linea / cantidad,
        }
    }

    fn pedir(detalle_id: i64, producto_id: i64, cantidad: f64) -> ProductoDevolver {
        ProductoDevolver { detalle_id, producto_id, cantidad }
    }

    #[test]
    fn producto_y_precio_salen_de_la_venta() {
        // Línea con descuento: 2 × 50 = 100, se cobró 90 -> cada uno 45.
        let lineas = [linea(1, 2.0, 90.0, 0.0), linea(2, 3.0, 80.0, 0.0)];
        // El navegador manda otro producto_id: se usa el de la venta.
        let (r, monto) = revisar_lineas(&lineas, &[pedir(1, 999, 1.0), pedir(2, 20, 1.0)]).unwrap();
        assert_eq!(r[0], (1, 10, 1.0, 45.0, 45.0));
        assert_eq!(r[1].4, 26.67);
        assert_eq!(monto, 71.67);
    }

    #[test]
    fn rechaza_lo_que_no_corresponde() {
        let lineas = [linea(1, 2.0, 100.0, 1.0)];
        assert!(revisar_lineas(&lineas, &[pedir(7, 10, 1.0)]).unwrap_err().contains("no es de esa venta"));
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, 0.0)]).is_err());
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, -1.0)]).is_err());
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, f64::NAN)]).is_err());
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, 2.0)]).unwrap_err().contains("Disponibles: 1"));
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, 0.5), pedir(1, 10, 0.5)]).unwrap_err().contains("repetido"));
        assert!(revisar_lineas(&lineas, &[pedir(1, 10, 1.0)]).is_ok());
    }
}
