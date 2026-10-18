use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;

use crate::handlers::ganancias;
use crate::tenants::TenantDb;
use crate::models::proveedor::*;
use crate::models::auth::Claims;

pub async fn obtener_proveedores(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Proveedor>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, nombre, contacto, telefono, email, direccion, tipo_documento, numero_documento, credito_disponible, activo
             FROM proveedores WHERE activo = 1 ORDER BY nombre",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut proveedores = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        proveedores.push(Proveedor {
            id: row.get(0).unwrap_or_default(),
            nombre: row.get(1).unwrap_or_default(),
            contacto: row.get(2).ok(),
            telefono: row.get(3).ok(),
            email: row.get(4).ok(),
            direccion: row.get(5).ok(),
            tipo_documento: row.get(6).unwrap_or_default(),
            numero_documento: row.get(7).ok(),
            credito_disponible: row.get(8).unwrap_or(0.0),
            activo: row.get::<i64>(9).unwrap_or(1) == 1,
        });
    }

    Ok(Json(proveedores))
}

pub async fn agregar_proveedor(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoProveedor>,
) -> Result<Json<Proveedor>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tipo_doc = payload.tipo_documento.clone().unwrap_or_else(|| "RUC".to_string());

    conn.execute(
        "INSERT INTO proveedores (nombre, contacto, telefono, email, direccion, tipo_documento, numero_documento)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        libsql::params![
            payload.nombre.clone(), payload.contacto.clone(), payload.telefono.clone(),
            payload.email.clone(), payload.direccion.clone(), tipo_doc.clone(), payload.numero_documento.clone()
        ],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = conn.last_insert_rowid();

    Ok(Json(Proveedor {
        id, nombre: payload.nombre, contacto: payload.contacto, telefono: payload.telefono,
        email: payload.email, direccion: payload.direccion, tipo_documento: tipo_doc,
        numero_documento: payload.numero_documento, credito_disponible: 0.0, activo: true,
    }))
}

pub async fn crear_compra(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevaCompraRequest>,
) -> Result<Json<CompraResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());

    // "EFECTIVO_CAJA": se paga con la plata de la caja abierta. Se guarda
    // como EFECTIVO y la compra queda enlazada a la caja y a su movimiento.
    let con_caja = payload.tipo_pago == "EFECTIVO_CAJA";
    let tipo_pago = if con_caja { "EFECTIVO".to_string() } else { payload.tipo_pago.clone() };
    if !["EFECTIVO", "TRANSFERENCIA", "CREDITO", "MIXTO"].contains(&tipo_pago.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Forma de pago no válida.".into()));
    }
    if payload.items.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Agrega al menos un producto a la compra.".into()));
    }
    if payload.items.iter().any(|i| !(i.cantidad > 0.0) || !(i.precio_compra >= 0.0)) {
        return Err((StatusCode::BAD_REQUEST, "Cada producto necesita una cantidad mayor a 0 y un precio que no sea negativo.".into()));
    }

    let subtotal: f64 = payload.items.iter().map(|i| i.precio_compra * i.cantidad).sum();
    let descuento = payload.descuento.unwrap_or(0.0);
    let credito_aplicado = payload.credito_aplicado.unwrap_or(0.0);
    let total = subtotal - descuento - credito_aplicado;

    // Validar crédito disponible del proveedor
    if credito_aplicado > 0.0 {
        let mut r = conn.query(
            "SELECT credito_disponible FROM proveedores WHERE id = ?1",
            libsql::params![payload.proveedor_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let credito_disp: f64 = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get(0).unwrap_or(0.0), None => 0.0,
        };
        if credito_aplicado > credito_disp + 0.01 {
            return Err((StatusCode::BAD_REQUEST, format!(
                "El crédito a aplicar S/ {:.2} supera el disponible S/ {:.2}", credito_aplicado, credito_disp
            )));
        }
    }

    let fecha = crate::logica::tiempo::hoy_lima_compacto();
    let query_folio = format!(
        "SELECT COALESCE(MAX(CAST(substr(folio,-4) AS INTEGER)),0)+1 FROM compras WHERE folio LIKE 'C-{}%'", fecha
    );
    let mut rf = conn.query(&query_folio, ()).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let siguiente: i64 = match rf.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(1), None => 1,
    };
    let folio = format!("C-{}-{:04}", fecha, siguiente);

    let (saldo_pendiente, monto_pagado, estado_pago) = if tipo_pago == "CREDITO" {
        (total, 0.0, "PENDIENTE")
    } else {
        (0.0, total, "PAGADO")
    };

    // Con la plata de la caja: la caja abierta (la más reciente, como en
    // Gastos) y que alcance el efectivo. Se revisa antes de guardar nada.
    let monto_caja = (total * 100.0).round() / 100.0;
    let caja: Option<(i64, String)> = if con_caja && monto_caja > 0.0 {
        let mut filas = conn
            .query(
                "SELECT c.id, COALESCE(u.nombre_completo, '') FROM cajas c LEFT JOIN usuarios u ON u.id = c.usuario_id
                 WHERE c.estado = 'ABIERTA' ORDER BY c.id DESC LIMIT 1",
                (),
            )
            .await
            .map_err(e500)?;
        let (id, cajero): (i64, String) = match filas.next().await.map_err(e500)? {
            Some(f) => (f.get(0).map_err(e500)?, f.get(1).unwrap_or_default()),
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "No hay una caja abierta. Abre la caja o elige otra forma de pago (efectivo de otro lado, transferencia o crédito).".into(),
                ))
            }
        };
        drop(filas);
        let disponible = crate::handlers::cajas::efectivo_en_caja(&conn, id).await?;
        if monto_caja > disponible + 0.005 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "En la caja solo hay S/ {:.2} en efectivo y la compra es de S/ {:.2}. Si la pagas con plata de otro lado, elige \"Efectivo (no de la caja)\".",
                    disponible.max(0.0),
                    monto_caja
                ),
            ));
        }
        Some((id, cajero))
    } else {
        None
    };
    let nombre_proveedor: String = {
        let mut filas = conn
            .query("SELECT nombre FROM proveedores WHERE id = ?1", libsql::params![payload.proveedor_id])
            .await
            .map_err(e500)?;
        match filas.next().await.map_err(e500)? {
            Some(f) => f.get(0).unwrap_or_default(),
            None => return Err((StatusCode::BAD_REQUEST, "El proveedor no existe.".into())),
        }
    };

    // Todo junto: la compra, sus productos y (si va con la caja) el
    // movimiento de caja. O entra todo, o nada.
    let tx = conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
    let guardado: Result<(i64, String), (StatusCode, String)> = async {
        tx.execute(
            "INSERT INTO compras (folio, proveedor_id, fecha_compra, subtotal, descuento, credito_aplicado, total,
                tipo_pago, monto_pagado, saldo_pendiente, fecha_vencimiento_pago, estado, estado_pago, usuario_id, factura_numero, notas)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'PENDIENTE',?12,?13,?14,?15)",
            libsql::params![
                folio.clone(), payload.proveedor_id, payload.fecha_compra.clone(), subtotal, descuento,
                credito_aplicado, total, tipo_pago.clone(), monto_pagado, saldo_pendiente,
                payload.fecha_vencimiento_pago.clone(), estado_pago, claims.sub,
                payload.factura_numero.clone(), payload.notas.clone()
            ],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear compra: {}", e)))?;

        let compra_id = tx.last_insert_rowid();

        for item in &payload.items {
            let subtotal_item = item.precio_compra * item.cantidad;
            // Nombre congelado al registrar la compra (migración 0006).
            let mut r_nombre = tx
                .query("SELECT nombre FROM productos WHERE id = ?1", libsql::params![item.producto_id])
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let nombre_producto: Option<String> = match r_nombre.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                Some(fila) => fila.get(0).ok(),
                None => None,
            };
            tx.execute(
                "INSERT INTO detalles_compra (compra_id, producto_id, cantidad, cantidad_recibida, cantidad_conforme, precio_compra, precio_venta_sugerido, subtotal, nombre_producto)
                 VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?6, ?7)",
                libsql::params![compra_id, item.producto_id, item.cantidad, item.precio_compra, item.precio_venta_sugerido, subtotal_item, nombre_producto],
            ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar item: {}", e)))?;
        }

        let mut mensaje = "Compra registrada exitosamente".to_string();
        if let Some((caja_id, cajero)) = &caja {
            let motivo = format!("Pago de compra {} a {}", folio, nombre_proveedor);
            tx.execute(
                "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, 'GASTO', ?2, ?3, ?4)",
                libsql::params![*caja_id, monto_caja, motivo, claims.sub],
            )
            .await
            .map_err(e500)?;
            let movimiento_id = tx.last_insert_rowid();
            tx.execute(
                "UPDATE cajas SET gastos_total = COALESCE(gastos_total, 0) + ?1 WHERE id = ?2",
                libsql::params![monto_caja, *caja_id],
            )
            .await
            .map_err(e500)?;
            // Sin la migración 0036 no hay dónde enlazarla: no se guarda nada.
            tx.execute(
                "UPDATE compras SET caja_id = ?1, movimiento_caja_id = ?2 WHERE id = ?3",
                libsql::params![*caja_id, movimiento_id, compra_id],
            )
            .await
            .map_err(|e| {
                if e.to_string().contains("no such column") {
                    (StatusCode::CONFLICT, "Aún no se puede pagar compras con la caja: el sistema se está actualizando. Intenta en un minuto o elige otra forma de pago.".to_string())
                } else {
                    e500(e)
                }
            })?;
            mensaje = if cajero.is_empty() {
                format!("Compra registrada. S/ {:.2} descontados del efectivo de la caja.", monto_caja)
            } else {
                format!("Compra registrada. S/ {:.2} descontados del efectivo de la caja de {}.", monto_caja, cajero)
            };
        }
        Ok((compra_id, mensaje))
    }
    .await;
    let (compra_id, mensaje) = match guardado {
        Ok(listo) => {
            tx.commit().await.map_err(e500)?;
            listo
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    };

    Ok(Json(CompraResponse { success: true, message: mensaje, compra_id: Some(compra_id), folio: Some(folio) }))
}

/// Vencimiento de un lote que llega en una compra: None si está bien, o el
/// mensaje para el usuario. `fecha` es "AAAA-MM-DD" (input date); `hoy`, Lima.
pub fn revisar_vencimiento_recibido(nombre: &str, fecha: &str, hoy: &str) -> Option<String> {
    if fecha.is_empty() {
        return Some(format!("Falta la fecha de vencimiento del lote para \"{}\".", nombre));
    }
    if !crate::logica::tiempo::fecha_valida(fecha) {
        return Some(format!("La fecha de vencimiento de \"{}\" no es válida.", nombre));
    }
    if fecha < hoy {
        let dma = fecha.split('-').rev().collect::<Vec<_>>().join("/");
        return Some(format!("El lote de \"{}\" ya venció ({}): no se puede recibir como mercadería para vender.", nombre, dma));
    }
    None
}

pub async fn recibir_mercaderia(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<RecibirMercaderiaRequest>,
) -> Result<Json<CompraResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Reporte de ganancias encendido: se anota cómo estaban los productos de
    // la compra para promediar su costo con lo que entra. Apagado, nada.
    let donde_ganancias = ganancias::Donde::Compra(payload.compra_id);
    let antes_ganancias = if ganancias::activo(&conn).await {
        Some(ganancias::foto(&conn, &donde_ganancias).await)
    } else {
        None
    };

    // Un perecible necesita una fecha de vencimiento real y que no haya
    // pasado: el stock de esos productos solo existe en sus lotes (sin fecha
    // se perdería) y un lote vencido no se puede vender. Se revisa antes de
    // tocar nada.
    let hoy = crate::logica::tiempo::hoy_lima();
    for item in &payload.items {
        let conforme = item.cantidad_conforme.min(item.cantidad_recibida);
        if conforme <= 0.0 {
            continue;
        }
        let mut r = conn.query(
            "SELECT COALESCE(dc.nombre_producto, p.nombre), p.lleva_vencimiento FROM detalles_compra dc
             JOIN productos p ON p.id = dc.producto_id WHERE dc.id = ?1",
            libsql::params![item.detalle_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if let Some(row) = r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            if row.get::<i64>(1).unwrap_or(0) == 1 {
                let nombre: String = row.get(0).unwrap_or_default();
                let fecha = item.fecha_vencimiento.as_deref().map(str::trim).unwrap_or("");
                if let Some(error) = revisar_vencimiento_recibido(&nombre, fecha, &hoy) {
                    return Err((StatusCode::BAD_REQUEST, error));
                }
            }
        }
    }

    for item in &payload.items {
        let conforme = item.cantidad_conforme.min(item.cantidad_recibida);

        conn.execute(
            "UPDATE detalles_compra SET cantidad_recibida = ?1, cantidad_conforme = ?2 WHERE id = ?3",
            libsql::params![item.cantidad_recibida, conforme, item.detalle_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar item: {}", e)))?;

        if conforme > 0.0 {
            let mut r = conn.query(
                "SELECT dc.producto_id, dc.precio_compra, p.lleva_vencimiento FROM detalles_compra dc
                 JOIN productos p ON p.id = dc.producto_id WHERE dc.id = ?1",
                libsql::params![item.detalle_id],
            ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if let Some(row) = r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                let producto_id: i64 = row.get(0).unwrap_or_default();
                let precio_compra: f64 = row.get(1).unwrap_or_default();
                let lleva_vencimiento: bool = row.get::<i64>(2).unwrap_or(0) == 1;

                // Actualiza el precio de compra de referencia en el producto
                conn.execute(
                    "UPDATE productos SET precio_compra = ?1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
                    libsql::params![precio_compra, producto_id],
                ).await.ok();

                // Si lleva vencimiento, crear un lote nuevo (el trigger de compras
                // solo actualiza stock de productos SIN vencimiento)
                if lleva_vencimiento {
                    if let Some(fecha_venc) = &item.fecha_vencimiento {
                        let numero_lote = item
                            .numero_lote
                            .as_deref()
                            .map(|n| n.trim().to_uppercase())
                            .filter(|n| !n.is_empty())
                            .map(|n| n.chars().take(40).collect::<String>());
                        conn.execute(
                            "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, compra_id, numero_lote) VALUES (?1, ?2, ?3, ?4, ?5)",
                            libsql::params![producto_id, conforme, fecha_venc.clone(), payload.compra_id, numero_lote],
                        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear lote: {}", e)))?;
                    }
                }
            }
        }
    }

    let mut r_total = conn.query("SELECT COUNT(*) FROM detalles_compra WHERE compra_id = ?1", libsql::params![payload.compra_id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_items: i64 = match r_total.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(0), None => 0,
    };

    let mut r_completos = conn.query(
        "SELECT COUNT(*) FROM detalles_compra WHERE compra_id = ?1 AND cantidad_conforme >= cantidad",
        libsql::params![payload.compra_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let items_completos: i64 = match r_completos.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(0), None => 0,
    };

    let nuevo_estado = if items_completos == total_items { "RECIBIDA" } else { "PARCIAL" };

    // Esta actualización dispara los triggers trg_after_compra_recibida y trg_recalcular_total_compra
    conn.execute(
        "UPDATE compras SET estado = ?1, fecha_recepcion = datetime('now','localtime'), notas_recepcion = ?2, fecha_actualizacion = datetime('now','localtime') WHERE id = ?3",
        libsql::params![nuevo_estado, payload.notas_recepcion.clone(), payload.compra_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar compra: {}", e)))?;

    // El stock ya subió (por el disparador de la compra o por los lotes).
    if let Some(antes) = &antes_ganancias {
        ganancias::conciliar(&conn, antes, &donde_ganancias, "COMPRA", Some(payload.compra_id), false).await;
    }

    // Desglose: faltante = nunca llegó (pedido - recibido).
    // Dañado = llegó pero en mal estado (recibido - conforme).
    // Ambos se le reclaman al proveedor, pero por vías distintas:
    // lo dañado se puede devolver físicamente (módulo Devoluciones a
    // Proveedor); lo faltante no existe físicamente, se regulariza
    // hablando directo con el proveedor — el sistema solo informa.
    let mut r_faltantes = conn.query(
        "SELECT COALESCE(SUM(cantidad - cantidad_recibida), 0) FROM detalles_compra WHERE compra_id = ?1",
        libsql::params![payload.compra_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_faltantes: f64 = match r_faltantes.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(0.0), None => 0.0,
    };

    let mut r_danados = conn.query(
        "SELECT COALESCE(SUM(cantidad_recibida - cantidad_conforme), 0) FROM detalles_compra WHERE compra_id = ?1",
        libsql::params![payload.compra_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_danados: f64 = match r_danados.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(0.0), None => 0.0,
    };

    let total_reclamar = total_faltantes + total_danados;
    let base_msg = if nuevo_estado == "RECIBIDA" { "recibida completamente" } else { "recibida parcialmente" };

    let message = if total_reclamar > 0.0 {
        format!(
            "Mercadería {}. {} unidad(es) por resolver con el proveedor: {} dañada(s) (regístralas en Devoluciones a Proveedor) + {} extraviada(s) (nunca llegaron — regulariza directo con el proveedor, no hay nada físico que devolver).",
            base_msg, total_reclamar, total_danados, total_faltantes
        )
    } else {
        format!("Mercadería {}. Stock actualizado correctamente.", base_msg)
    };

    Ok(Json(CompraResponse {
        success: true,
        message,
        compra_id: Some(payload.compra_id),
        folio: None,
    }))
}

pub async fn listar_compras(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<CompraResumen>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT c.id, c.folio, p.nombre, c.fecha_compra, c.total, c.estado, c.estado_pago,
                    COALESCE((SELECT SUM(dc.cantidad_recibida - dc.cantidad_conforme) FROM detalles_compra dc WHERE dc.compra_id = c.id), 0.0) as danados,
                    COALESCE((SELECT SUM(ddp.cantidad_devuelta) FROM detalles_devolucion_proveedor ddp
                              JOIN devoluciones_proveedor dp ON ddp.devolucion_proveedor_id = dp.id
                              WHERE dp.compra_id = c.id), 0.0) as ya_devueltos,
                    COALESCE((SELECT SUM(dc.cantidad - dc.cantidad_recibida) FROM detalles_compra dc WHERE dc.compra_id = c.id), 0.0) as faltantes
             FROM compras c JOIN proveedores p ON c.proveedor_id = p.id
             ORDER BY c.fecha_creacion DESC LIMIT 50",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut compras = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        compras.push(CompraResumen {
            id: row.get(0).unwrap_or_default(),
            folio: row.get(1).unwrap_or_default(),
            proveedor_nombre: row.get(2).unwrap_or_default(),
            fecha_compra: row.get(3).unwrap_or_default(),
            total: row.get(4).unwrap_or_default(),
            estado: row.get(5).unwrap_or_default(),
            estado_pago: row.get(6).unwrap_or_default(),
            unidades_danadas: row.get(7).unwrap_or(0.0),
            unidades_ya_devueltas: row.get(8).unwrap_or(0.0),
            unidades_faltantes: row.get(9).unwrap_or(0.0),
        });
    }

    Ok(Json(compras))
}

pub async fn detalle_compra(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<CompraDetalle>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut r1 = conn.query(
        "SELECT c.id, c.folio, p.nombre, c.estado FROM compras c JOIN proveedores p ON c.proveedor_id = p.id WHERE c.id = ?1",
        libsql::params![id],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (compra_id, folio, proveedor_nombre, estado) = match r1.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => (
            row.get(0).unwrap_or_default(),
            row.get(1).unwrap_or_default(),
            row.get(2).unwrap_or_default(),
            row.get(3).unwrap_or_default(),
        ),
        None => return Err(StatusCode::NOT_FOUND),
    };

    let mut r2 = conn.query(
        "SELECT dc.id, dc.producto_id, COALESCE(dc.nombre_producto, p.nombre), p.lleva_vencimiento, dc.cantidad, dc.cantidad_recibida, dc.cantidad_conforme, dc.precio_compra, p.codigo
         FROM detalles_compra dc JOIN productos p ON p.id = dc.producto_id
         WHERE dc.compra_id = ?1 ORDER BY dc.id",
        libsql::params![id],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut items = Vec::new();
    while let Some(row) = r2.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        items.push(DetalleCompraItem {
            id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
            codigo: row.get::<String>(8).ok().filter(|c| !c.trim().is_empty()),
            producto_nombre: row.get(2).unwrap_or_default(),
            lleva_vencimiento: row.get::<i64>(3).unwrap_or(0) == 1,
            cantidad: row.get(4).unwrap_or_default(),
            cantidad_recibida: row.get(5).unwrap_or_default(),
            cantidad_conforme: row.get(6).unwrap_or_default(),
            precio_compra: row.get(7).unwrap_or_default(),
        });
    }

    Ok(Json(CompraDetalle { id: compra_id, folio, proveedor_nombre, estado, items }))
}

#[cfg(test)]
mod pruebas {
    use super::revisar_vencimiento_recibido as revisar;

    #[test]
    fn el_lote_recibido_necesita_un_vencimiento_vigente() {
        let hoy = "2026-10-17";
        assert!(revisar("Panadol", "", hoy).unwrap().starts_with("Falta la fecha"));
        assert!(revisar("Panadol", "2026-02-30", hoy).unwrap().contains("no es válida"));
        assert!(revisar("Panadol", "2026-10-16", hoy).unwrap().contains("ya venció (16/10/2026)"));
        assert_eq!(revisar("Panadol", "2026-10-17", hoy), None);
        assert_eq!(revisar("Panadol", "2028-06-30", hoy), None);
    }
}
