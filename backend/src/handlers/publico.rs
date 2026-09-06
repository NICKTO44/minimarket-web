use axum::{extract::{State, Path}, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct ItemPublico {
    pub nombre: String,
    pub cantidad: f64,
    pub precio: f64,
}

#[derive(Serialize)]
pub struct ComprobantePublicoResponse {
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub ruc_emisor: Option<String>,
    pub tipo: String,
    pub serie: Option<String>,
    pub numero: Option<i64>,
    pub hash: Option<String>,
    pub fecha_emision: Option<String>,
    pub igv: f64,
    pub total: f64,
    pub folio_venta: String,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub items: Vec<ItemPublico>,
}

/// Ruta PÚBLICA (sin login) para que un cliente final vea su comprobante
/// desde un link de WhatsApp. Deliberadamente restringida:
///   - Solo devuelve los datos de ESE comprobante puntual, identificado
///     por su id — nunca lista ni permite explorar otros.
///   - No expone la dirección del cliente (solo nombre y documento, que
///     de todas formas ya aparecen impresos en cualquier boleta física).
///   - No incluye nada de configuración interna del negocio (token de
///     FacturaLibre, otros usuarios, otras ventas, etc.) — solo nombre,
///     dirección y teléfono de la tienda, que también son públicos en
///     cualquier boleta física.
pub async fn ver_comprobante_publico(
    State(state): State<Arc<AppState>>,
    Path((identificador, comprobante_id)): Path<(String, i64)>,
) -> Result<Json<ComprobantePublicoResponse>, (StatusCode, String)> {
    let tienda = state
        .tiendas
        .buscar_por_identificador(&identificador)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Negocio no encontrado".to_string()))?;

    let db = state
        .tiendas
        .conectar_cacheado(&tienda)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "No se pudo conectar al negocio".to_string()))?;

    let conn = db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r = conn
        .query(
            "SELECT ce.tipo, ce.serie, ce.numero, ce.hash, substr(ce.fecha_emision,1,10),
                    ce.cliente_documento, ce.cliente_nombre, v.id, v.folio, v.total
             FROM comprobantes_electronicos ce
             JOIN ventas v ON v.id = ce.venta_id
             WHERE ce.id = ?1",
            libsql::params![comprobante_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row,
        None => return Err((StatusCode::NOT_FOUND, "Comprobante no encontrado".to_string())),
    };

    let tipo: String = row.get(0).unwrap_or_default();
    let serie: Option<String> = row.get(1).ok();
    let numero: Option<i64> = row.get(2).ok();
    let hash: Option<String> = row.get(3).ok();
    let fecha_emision: Option<String> = row.get(4).ok();
    let cliente_documento: Option<String> = row.get(5).ok();
    let cliente_nombre: Option<String> = row.get(6).ok();
    let venta_id: i64 = row.get(7).unwrap_or_default();
    let folio_venta: String = row.get(8).unwrap_or_default();
    let total: f64 = row.get(9).unwrap_or_default();

    let igv = total - (total / 1.18);

    let mut ri = conn
        .query(
            "SELECT p.nombre, dv.cantidad, dv.precio_unitario
             FROM detalles_venta dv JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1",
            libsql::params![venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut items = Vec::new();
    while let Some(row) = ri.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        items.push(ItemPublico {
            nombre: row.get(0).unwrap_or_default(),
            cantidad: row.get(1).unwrap_or_default(),
            precio: row.get(2).unwrap_or_default(),
        });
    }

    let mut rcfg = conn
        .query("SELECT nombre_tienda, direccion, telefono, ruc FROM configuracion_tienda LIMIT 1", ())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (nombre_tienda, direccion, telefono, ruc_emisor): (String, Option<String>, Option<String>, Option<String>) =
        match rcfg.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => (
                row.get(0).unwrap_or_else(|_| tienda.nombre_negocio.clone()),
                row.get(1).ok(),
                row.get(2).ok(),
                row.get(3).ok(),
            ),
            None => (tienda.nombre_negocio.clone(), None, None, None),
        };

    Ok(Json(ComprobantePublicoResponse {
        nombre_tienda,
        direccion,
        telefono,
        ruc_emisor,
        tipo,
        serie,
        numero,
        hash,
        fecha_emision,
        igv,
        total,
        folio_venta,
        cliente_nombre,
        cliente_documento,
        items,
    }))
}