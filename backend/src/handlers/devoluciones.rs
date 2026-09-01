use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;
use chrono::Local;

use crate::tenants::TenantDb;
use crate::models::devolucion::*;

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
            "SELECT id, folio, fecha_hora, total, metodo_pago FROM ventas WHERE id = ?1",
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

    let mut rows_det = conn
        .query(
            "SELECT dv.id, dv.producto_id, p.nombre, dv.cantidad, dv.precio_unitario, dv.total_linea
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

    Ok(Json(VentaParaDevolucion { venta_id, folio: folio_venta, fecha_hora, total, metodo_pago, productos, comprobante }))
}

pub async fn procesar_devolucion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevaDevolucion>,
) -> Result<Json<DevolucionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let fecha_actual = Local::now().format("%Y%m%d").to_string();
    let query_folio = format!(
        "SELECT COALESCE(MAX(CAST(substr(folio_devolucion,-4) AS INTEGER)),0)+1
         FROM devoluciones WHERE folio_devolucion LIKE 'DEV-{}%'", fecha_actual
    );
    let mut rows_folio = conn.query(&query_folio, ()).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let siguiente: i64 = match rows_folio.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(r) => r.get(0).unwrap_or(1),
        None => 1,
    };
    let folio_devolucion = format!("DEV-{}-{:04}", fecha_actual, siguiente);

    let mut monto_total = 0.0f64;

    for p in &payload.productos {
        let mut r1 = conn.query(
            "SELECT COALESCE(SUM(dd.cantidad_devuelta),0.0) FROM detalles_devolucion dd
             JOIN devoluciones d ON dd.devolucion_id = d.id
             WHERE dd.detalle_venta_id = ?1 AND d.estado = 'PROCESADA'",
            libsql::params![p.detalle_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let ya_devuelto: f64 = match r1.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(r) => r.get(0).unwrap_or(0.0), None => 0.0,
        };

        let mut r2 = conn.query(
            "SELECT cantidad, precio_unitario FROM detalles_venta WHERE id = ?1",
            libsql::params![p.detalle_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let row = match r2.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(r) => r,
            None => return Err((StatusCode::BAD_REQUEST, "Detalle de venta no encontrado".into())),
        };
        let cantidad_original: f64 = row.get(0).unwrap_or_default();
        let precio: f64 = row.get(1).unwrap_or_default();

        if ya_devuelto + p.cantidad > cantidad_original {
            return Err((StatusCode::BAD_REQUEST, format!(
                "No puedes devolver {} unidades. Disponibles: {}", p.cantidad, cantidad_original - ya_devuelto
            )));
        }
        monto_total += precio * p.cantidad;
    }

    let mut r_metodo = conn.query("SELECT metodo_pago FROM ventas WHERE id = ?1", libsql::params![payload.venta_id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let metodo_original: String = match r_metodo.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or_else(|_| "EFECTIVO".to_string()),
        None => "EFECTIVO".to_string(),
    };
    // El trigger de caja solo distingue EFECTIVO/TARJETA/TRANSFERENCIA — Yape/Plin se agrupa como transferencia
    let metodo_reembolso = if metodo_original == "YAPE_PLIN" { "TRANSFERENCIA" } else { metodo_original.as_str() };

    conn.execute(
        "INSERT INTO devoluciones (venta_original_id, folio_devolucion, monto_reembolsado, metodo_reembolso, motivo, usuario_id, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'PROCESADA')",
        libsql::params![payload.venta_id, folio_devolucion.clone(), monto_total, metodo_reembolso, payload.motivo.clone(), payload.usuario_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar devolución: {}", e)))?;

    let devolucion_id = conn.last_insert_rowid();

    for p in &payload.productos {
        let mut r = conn.query(
            "SELECT precio_unitario FROM detalles_venta WHERE id = ?1",
            libsql::params![p.detalle_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let precio: f64 = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get(0).unwrap_or_default(), None => 0.0,
        };
        let subtotal = precio * p.cantidad;

        conn.execute(
            "INSERT INTO detalles_devolucion (devolucion_id, producto_id, detalle_venta_id, venta_id, cantidad_devuelta, precio_unitario, subtotal, condicion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'REVENTA')",
            libsql::params![devolucion_id, p.producto_id, p.detalle_id, payload.venta_id, p.cantidad, precio, subtotal],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al insertar detalle: {}", e)))?;
    }

    Ok(Json(DevolucionResponse {
        success: true,
        message: "Devolución procesada exitosamente".into(),
        folio_devolucion: Some(folio_devolucion),
    }))
}