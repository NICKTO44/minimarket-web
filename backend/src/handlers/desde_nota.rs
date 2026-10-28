//! Emitir la boleta o la factura de una venta que se cobró como NOTA DE
//! VENTA (sin comprobante). Pasa seguido: el cliente paga y después pide su
//! factura. La venta no cambia (mismos productos, montos, caja y stock);
//! solo se le emite su comprobante, con fecha de hoy, por el mismo camino del
//! punto de venta (handlers/facturacion.rs).
//!
//!   POST /comprobantes/desde-nota  { venta_id, tipo: BOLETA | FACTURA, cliente_id? }
//!
//! Reglas:
//!   - la venta está COMPLETADA y no tiene comprobante (uno RECHAZADO no
//!     cuenta: se puede volver a intentar);
//!   - no tiene devoluciones (el comprobante saldría por más de lo vendido);
//!   - factura: con un cliente con RUC (el elegido o el de la venta);
//!   - el cliente elegido queda en la venta (reportes, créditos...).

use axum::{extract::Extension, http::StatusCode, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::models::facturacion::{ComprobanteResponse, EmitirComprobanteRequest};
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn mal(texto: &str) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.to_string())
}

#[derive(Debug, Deserialize)]
pub struct DesdeNota {
    pub venta_id: i64,
    pub tipo: String,
    #[serde(default)]
    pub cliente_id: Option<i64>,
}

/// Lo que se revisa antes de emitir (sin escribir nada).
#[derive(Debug, PartialEq)]
pub struct Revision {
    pub cliente_id: Option<i64>,
    pub documento: Option<String>,
    pub nombre: Option<String>,
}

/// La primera fila de una consulta.
async fn uno(conn: &libsql::Connection, sql: &str, params: impl libsql::params::IntoParams) -> Result<Option<libsql::Row>, Fallo> {
    let mut f = conn.query(sql, params).await.map_err(e500)?;
    f.next().await.map_err(e500)
}

/// Revisa la venta y el cliente. `tipo` ya en mayúsculas.
pub async fn revisar(conn: &libsql::Connection, venta_id: i64, tipo: &str, cliente_elegido: Option<i64>) -> Result<Revision, Fallo> {
    if tipo != "BOLETA" && tipo != "FACTURA" {
        return Err(mal("Elige boleta o factura."));
    }
    let Some(v) = uno(conn, "SELECT COALESCE(estado, 'COMPLETADA'), cliente_id FROM ventas WHERE id = ?1", libsql::params![venta_id]).await? else {
        return Err((StatusCode::NOT_FOUND, "Esa venta no existe.".to_string()));
    };
    if v.get::<String>(0).unwrap_or_default() != "COMPLETADA" {
        return Err(mal("Esa venta está anulada: no se le puede emitir comprobante."));
    }
    let cliente_venta: Option<i64> = v.get(1).ok();
    let ya: i64 = uno(
        conn,
        "SELECT COUNT(*) FROM comprobantes_electronicos WHERE venta_id = ?1 AND COALESCE(estado, '') <> 'RECHAZADO'",
        libsql::params![venta_id],
    )
    .await?
    .and_then(|f| f.get(0).ok())
    .unwrap_or(0);
    if ya > 0 {
        return Err((StatusCode::CONFLICT, "Esta venta ya tiene su boleta o factura: búscala en la lista.".to_string()));
    }
    let devoluciones: i64 = uno(
        conn,
        "SELECT COUNT(*) FROM detalles_devolucion dd JOIN devoluciones d ON d.id = dd.devolucion_id
         WHERE dd.venta_id = ?1 AND d.estado = 'PROCESADA'",
        libsql::params![venta_id],
    )
    .await?
    .and_then(|f| f.get(0).ok())
    .unwrap_or(0);
    if devoluciones > 0 {
        return Err(mal("Esta venta tiene una devolución: ya no se le puede emitir boleta o factura."));
    }
    let cliente_id = cliente_elegido.or(cliente_venta);
    let (documento, nombre) = match cliente_id {
        Some(id) => match uno(conn, "SELECT numero_documento, nombre_razon_social FROM clientes WHERE id = ?1", libsql::params![id]).await? {
            Some(c) => (
                c.get::<String>(0).ok().map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
                c.get::<String>(1).ok().filter(|n| !n.trim().is_empty()),
            ),
            None => return Err(mal("El cliente elegido no existe.")),
        },
        None => (None, None),
    };
    if tipo == "FACTURA" && !documento.as_deref().is_some_and(|d| d.len() == 11 && d.chars().all(|c| c.is_ascii_digit())) {
        return Err(mal("Para emitir factura elige un cliente con RUC (11 dígitos)."));
    }
    Ok(Revision { cliente_id, documento, nombre })
}

/// POST /comprobantes/desde-nota
pub async fn emitir(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(p): Json<DesdeNota>,
) -> Result<Json<ComprobanteResponse>, Fallo> {
    let tipo = p.tipo.trim().to_uppercase();
    let conn = tenant.0.connect().map_err(e500)?;
    let r = revisar(&conn, p.venta_id, &tipo, p.cliente_id).await?;
    // El cliente queda en la venta (la dirección fiscal de la factura se lee de ahí).
    if let Some(id) = r.cliente_id {
        conn.execute("UPDATE ventas SET cliente_id = ?1 WHERE id = ?2", libsql::params![id, p.venta_id]).await.map_err(e500)?;
    }
    drop(conn);
    crate::handlers::facturacion::emitir_comprobante_con_enlace(
        Extension(tenant),
        Extension(claims),
        Json(EmitirComprobanteRequest {
            venta_id: p.venta_id,
            tipo,
            cliente_documento: r.documento,
            cliente_nombre: r.nombre,
            detraccion: None,
        }),
    )
    .await
}
