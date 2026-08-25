use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;
use crate::models::comprobante::ComprobanteResumen;

#[derive(Deserialize)]
pub struct FiltrosComprobante {
    pub tipo: Option<String>,
    pub estado: Option<String>,
}

pub async fn listar_comprobantes(
    State(state): State<Arc<AppState>>,
    Query(filtros): Query<FiltrosComprobante>,
) -> Result<Json<Vec<ComprobanteResumen>>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut sql = String::from(
        "SELECT ce.id, v.id, v.folio, COALESCE(ce.tipo, 'NINGUNO'), ce.serie, ce.numero,
                c.nombre_razon_social, v.total, ce.estado, v.fecha_hora
         FROM ventas v
         LEFT JOIN comprobantes_electronicos ce ON ce.venta_id = v.id
         LEFT JOIN clientes c ON c.id = v.cliente_id
         WHERE v.estado = 'COMPLETADA'",
    );

    let mut idx = 1;
    if filtros.tipo.is_some() {
        sql.push_str(&format!(" AND COALESCE(ce.tipo, 'NINGUNO') = ?{}", idx));
        idx += 1;
    }
    if filtros.estado.is_some() {
        sql.push_str(&format!(" AND ce.estado = ?{}", idx));
    }
    sql.push_str(" ORDER BY v.fecha_hora DESC LIMIT 100");

    let mut rows = match (&filtros.tipo, &filtros.estado) {
        (Some(t), Some(e)) => conn.query(&sql, libsql::params![t.clone(), e.clone()]).await,
        (Some(t), None) => conn.query(&sql, libsql::params![t.clone()]).await,
        (None, Some(e)) => conn.query(&sql, libsql::params![e.clone()]).await,
        (None, None) => conn.query(&sql, ()).await,
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut comprobantes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        comprobantes.push(ComprobanteResumen {
            id: row.get(0).ok(),
            venta_id: row.get(1).unwrap_or_default(),
            folio_venta: row.get(2).unwrap_or_default(),
            tipo: row.get(3).unwrap_or_default(),
            serie: row.get(4).ok(),
            numero: row.get(5).ok(),
            cliente_nombre: row.get(6).ok(),
            monto: row.get(7).unwrap_or_default(),
            estado: row.get(8).ok(),
            fecha_emision: row.get(9).unwrap_or_default(),
        });
    }

    Ok(Json(comprobantes))
}