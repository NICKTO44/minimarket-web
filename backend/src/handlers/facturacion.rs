use axum::{extract::State, Json, http::StatusCode};
use std::sync::Arc;

use crate::AppState;
use crate::models::facturacion::*;
use crate::logica::facturacion::{emitir_facturalibre, DatosParaEmitir, ItemFactura};

pub async fn emitir_comprobante(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EmitirComprobanteRequest>,
) -> Result<Json<ComprobanteResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r = conn
        .query("SELECT total, subtotal FROM ventas WHERE id = ?1", libsql::params![payload.venta_id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (total, subtotal): (f64, f64) = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => (row.get(0).unwrap_or_default(), row.get(1).unwrap_or_default()),
        None => return Err((StatusCode::NOT_FOUND, "Venta no encontrada".into())),
    };
    let _ = subtotal;

    let igv = total - (total / 1.18);
    let base_sin_igv = total - igv;

    let mut ri = conn
        .query(
            "SELECT p.nombre, dv.cantidad, dv.precio_unitario, p.unidad_medida
             FROM detalles_venta dv JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1",
            libsql::params![payload.venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut items = Vec::new();
    while let Some(row) = ri.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        items.push(ItemFactura {
            descripcion: row.get(0).unwrap_or_default(),
            cantidad: row.get(1).unwrap_or_default(),
            precio_unitario: row.get(2).unwrap_or_default(),
            unidad_medida: row.get(3).unwrap_or_default(),
        });
    }

    let mut cliente_direccion: Option<String> = None;
    let mut r_venta_cliente = conn
        .query("SELECT cliente_id FROM ventas WHERE id = ?1", libsql::params![payload.venta_id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(row) = r_venta_cliente.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        let cliente_id: Option<i64> = row.get(0).ok();
        if let Some(cid) = cliente_id {
            let mut rc = conn
                .query("SELECT direccion FROM clientes WHERE id = ?1", libsql::params![cid])
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if let Some(rowc) = rc.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
                cliente_direccion = rowc.get(0).ok();
            }
        }
    }

    // Igual que en Lubricentro: código genérico fijo en código, no en
    // configuración de usuario — el cajero/administrador nunca necesita
    // tocarlo.
    const CODIGO_PRODUCTO_SUNAT_GENERICO: &str = "50000000";

    let mut rcfg = conn
        .query("SELECT facturalibre_token, facturalibre_ruta FROM configuracion_tienda LIMIT 1", ())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (token, ruta): (Option<String>, Option<String>) =
        match rcfg.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => (row.get(0).ok(), row.get(1).ok()),
            None => (None, None),
        };

    let codigo_sunat = CODIGO_PRODUCTO_SUNAT_GENERICO;

    let (token, ruta) = match (token, ruta) {
        (Some(t), Some(r)) if !t.trim().is_empty() && !r.trim().is_empty() => (t, r),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Falta configurar el Token y la URL de FacturaLibre en Configuración antes de emitir comprobantes.".into(),
            ))
        }
    };

    let tipo_doc_cliente = if payload.tipo == "FACTURA" {
        Some("RUC".to_string())
    } else {
        payload.cliente_documento.as_ref().map(|d| {
            if d.len() == 11 { "RUC".to_string() } else { "DNI".to_string() }
        })
    };

    let datos = DatosParaEmitir {
        tipo: payload.tipo.clone(),
        cliente_tipo_documento: tipo_doc_cliente,
        cliente_documento: payload.cliente_documento.clone(),
        cliente_nombre: payload.cliente_nombre.clone(),
        cliente_direccion,
        subtotal: base_sin_igv,
        igv,
        total,
        items,
    };

    let resultado = emitir_facturalibre(&datos, &token, &ruta, codigo_sunat).await;

    let numero = if resultado.numero > 0 {
        resultado.numero
    } else {
        let mut rn = conn
            .query(
                "SELECT COALESCE(MAX(numero),0)+1 FROM comprobantes_electronicos WHERE serie = ?1",
                libsql::params![resultado.serie.clone()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match rn.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get(0).unwrap_or(1),
            None => 1,
        }
    };

    let estado = if resultado.aceptado { "ACEPTADO" } else { "RECHAZADO" };

    conn.execute(
        "INSERT INTO comprobantes_electronicos
            (venta_id, tipo, proveedor, serie, numero, cliente_documento, cliente_nombre, estado, mensaje_sunat, enlace_cdr, external_id, hash)
         VALUES (?1, ?2, 'FACTURALIBRE', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        libsql::params![
            payload.venta_id, payload.tipo.clone(), resultado.serie.clone(), numero,
            payload.cliente_documento.clone(), payload.cliente_nombre.clone(), estado, resultado.mensaje.clone(),
            resultado.enlace_cdr.clone(), resultado.external_id.clone(), resultado.hash.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al guardar comprobante: {}", e)))?;

    let comprobante_id = conn.last_insert_rowid();

    Ok(Json(ComprobanteResponse {
        success: resultado.aceptado,
        comprobante_id: Some(comprobante_id),
        tipo: payload.tipo,
        serie: resultado.serie,
        numero,
        estado: estado.to_string(),
        mensaje: resultado.mensaje,
    }))
}