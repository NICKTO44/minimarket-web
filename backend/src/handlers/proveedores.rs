use axum::{extract::{State, Path}, Json, http::StatusCode};
use std::sync::Arc;
use chrono::Local;

use crate::AppState;
use crate::models::proveedor::*;

pub async fn obtener_proveedores(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Proveedor>>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NuevoProveedor>,
) -> Result<Json<Proveedor>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NuevaCompraRequest>,
) -> Result<Json<CompraResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let fecha = Local::now().format("%Y%m%d").to_string();
    let query_folio = format!(
        "SELECT COALESCE(MAX(CAST(substr(folio,-4) AS INTEGER)),0)+1 FROM compras WHERE folio LIKE 'C-{}%'", fecha
    );
    let mut rf = conn.query(&query_folio, ()).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let siguiente: i64 = match rf.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(1), None => 1,
    };
    let folio = format!("C-{}-{:04}", fecha, siguiente);

    let (saldo_pendiente, monto_pagado, estado_pago) = if payload.tipo_pago == "CREDITO" {
        (total, 0.0, "PENDIENTE")
    } else {
        (0.0, total, "PAGADO")
    };

    conn.execute(
        "INSERT INTO compras (folio, proveedor_id, fecha_compra, subtotal, descuento, credito_aplicado, total,
            tipo_pago, monto_pagado, saldo_pendiente, fecha_vencimiento_pago, estado, estado_pago, usuario_id, factura_numero, notas)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'PENDIENTE',?12,?13,?14,?15)",
        libsql::params![
            folio.clone(), payload.proveedor_id, payload.fecha_compra.clone(), subtotal, descuento,
            credito_aplicado, total, payload.tipo_pago.clone(), monto_pagado, saldo_pendiente,
            payload.fecha_vencimiento_pago.clone(), estado_pago, payload.usuario_id,
            payload.factura_numero.clone(), payload.notas.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear compra: {}", e)))?;

    let compra_id = conn.last_insert_rowid();

    for item in &payload.items {
        let subtotal_item = item.precio_compra * item.cantidad;
        conn.execute(
            "INSERT INTO detalles_compra (compra_id, producto_id, cantidad, cantidad_recibida, cantidad_conforme, precio_compra, precio_venta_sugerido, subtotal)
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?6)",
            libsql::params![compra_id, item.producto_id, item.cantidad, item.precio_compra, item.precio_venta_sugerido, subtotal_item],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar item: {}", e)))?;
    }

    Ok(Json(CompraResponse { success: true, message: "Compra registrada exitosamente".into(), compra_id: Some(compra_id), folio: Some(folio) }))
}

pub async fn recibir_mercaderia(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecibirMercaderiaRequest>,
) -> Result<Json<CompraResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
                        conn.execute(
                            "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, compra_id) VALUES (?1, ?2, ?3, ?4)",
                            libsql::params![producto_id, conforme, fecha_venc.clone(), payload.compra_id],
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
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CompraResumen>>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<CompraDetalle>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
        "SELECT dc.id, dc.producto_id, p.nombre, p.lleva_vencimiento, dc.cantidad, dc.cantidad_recibida, dc.cantidad_conforme, dc.precio_compra
         FROM detalles_compra dc JOIN productos p ON p.id = dc.producto_id
         WHERE dc.compra_id = ?1 ORDER BY dc.id",
        libsql::params![id],
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut items = Vec::new();
    while let Some(row) = r2.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        items.push(DetalleCompraItem {
            id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
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