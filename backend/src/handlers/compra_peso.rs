//! Compra por peso (POST /compras/peso, módulo COMPRA_PESO): recicladoras y
//! chatarrerías que COMPRAN material por kilo a quien lo trae (cobre, cartón,
//! fierro...), le pagan en el momento y le dan un ticket. Sin el módulo, la
//! ruta responde 403.
//!
//! Por dentro es un Ingreso rápido (handlers/compra_rapida.rs) de productos
//! que ya existen, al proveedor "COMPRA POR PESO" y pagado con el efectivo de
//! la caja abierta: sube el stock de cada material, queda su último precio de
//! compra (así sale bien la ganancia al venderlo) y se descuenta de la caja.
//! Devuelve lo necesario para imprimir el ticket de la compra.

use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::compra_rapida::{ingresar, IngresoRapido, ItemRapido};
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

/// Proveedor de las compras por peso (uno por negocio, se crea solo).
pub const PROVEEDOR_PESO: &str = "COMPRA POR PESO";
const MAX_LINEAS: usize = 50;

#[derive(Debug, Deserialize)]
pub struct LineaPeso {
    pub producto_id: i64,
    /// Kilos (o la unidad del material), con decimales.
    pub cantidad: f64,
    /// Lo que se paga por kilo en esta compra (puede ser distinto del de ayer).
    pub precio: f64,
}

#[derive(Debug, Deserialize)]
pub struct CompraPeso {
    pub lineas: Vec<LineaPeso>,
    /// Quien trajo el material (opcional): sale en el ticket y en la compra.
    #[serde(default)]
    pub persona: Option<String>,
    /// Con qué se paga: EFECTIVO_CAJA (por defecto: sale de la caja abierta),
    /// EFECTIVO (plata de otro lado, no toca la caja) o TRANSFERENCIA
    /// (Yape, Plin, banco).
    #[serde(default)]
    pub tipo_pago: Option<String>,
}

/// Formas de pago de una compra por peso.
pub const PAGOS: [&str; 3] = ["EFECTIVO_CAJA", "EFECTIVO", "TRANSFERENCIA"];

fn tipo_pago(compra: &CompraPeso) -> String {
    compra.tipo_pago.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("EFECTIVO_CAJA").to_uppercase()
}

#[derive(Debug, Serialize)]
pub struct LineaTicket {
    pub nombre: String,
    pub unidad: String,
    pub cantidad: f64,
    pub precio: f64,
    pub subtotal: f64,
}

#[derive(Debug, Serialize)]
pub struct CompraPesoHecha {
    pub compra_id: i64,
    pub folio: String,
    pub fecha: String,
    pub persona: Option<String>,
    pub lineas: Vec<LineaTicket>,
    pub total: f64,
    pub tipo_pago: String,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn redondear(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Revisa lo que llega antes de tocar la base.
pub fn validar(compra: &CompraPeso) -> Result<(), String> {
    if compra.lineas.is_empty() {
        return Err("Agrega al menos un material.".into());
    }
    if !PAGOS.contains(&tipo_pago(compra).as_str()) {
        return Err("Forma de pago no válida.".into());
    }
    if compra.lineas.len() > MAX_LINEAS {
        return Err(format!("Son muchos materiales en una sola compra: hasta {}.", MAX_LINEAS));
    }
    let mut vistos = std::collections::HashSet::new();
    for (i, l) in compra.lineas.iter().enumerate() {
        let fila = i + 1;
        if !(l.cantidad > 0.0) || !l.cantidad.is_finite() {
            return Err(format!("Material {}: el peso debe ser mayor a 0.", fila));
        }
        if !(l.precio >= 0.0) || !l.precio.is_finite() {
            return Err(format!("Material {}: el precio no puede ser negativo.", fila));
        }
        if !vistos.insert(l.producto_id) {
            return Err(format!("Material {}: ese material ya está en la compra (suma el peso en una sola línea).", fila));
        }
    }
    Ok(())
}

/// POST /compras/peso
pub async fn registrar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(compra): Json<CompraPeso>,
) -> Result<Json<CompraPesoHecha>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    crate::handlers::rubros::exigir_modulo(&conn, crate::handlers::rubros::MODULO_COMPRA_PESO, "Compra por peso").await?;
    validar(&compra).map_err(malo)?;

    // Nombre y unidad de cada material, para el ticket (y que exista).
    let mut lineas = Vec::with_capacity(compra.lineas.len());
    for (i, l) in compra.lineas.iter().enumerate() {
        let mut f = conn
            .query("SELECT nombre, unidad_medida FROM productos WHERE id = ?1 AND activo = 1", libsql::params![l.producto_id])
            .await
            .map_err(e500)?;
        let fila = f
            .next()
            .await
            .map_err(e500)?
            .ok_or_else(|| malo(format!("Material {}: ya no existe o está desactivado.", i + 1)))?;
        lineas.push(LineaTicket {
            nombre: fila.get(0).unwrap_or_default(),
            unidad: fila.get(1).unwrap_or_else(|_| "KG".into()),
            cantidad: l.cantidad,
            precio: l.precio,
            subtotal: redondear(l.cantidad * l.precio),
        });
    }

    let persona = compra.persona.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(|p| p.chars().take(80).collect::<String>());
    let ingreso = IngresoRapido {
        items: compra
            .lineas
            .iter()
            .map(|l| ItemRapido {
                producto_id: Some(l.producto_id),
                nuevo: None,
                cantidad: l.cantidad,
                precio_compra: l.precio,
                fecha_vencimiento: None,
                vender_caja: None,
            })
            .collect(),
        tipo_pago: tipo_pago(&compra),
        notas: Some(match &persona {
            Some(p) => format!("Compra por peso a {}", p),
            None => "Compra por peso".to_string(),
        }),
    };
    let proveedor_id = crate::handlers::compra_rapida::proveedor_del_sistema(
        &conn,
        PROVEEDOR_PESO,
        "Compras de material por peso (lo crea el sistema).",
    )
    .await?;
    let hecho = ingresar(tenant, claims, ingreso, proveedor_id, "Compra por peso").await?;

    let total = redondear(lineas.iter().map(|l| l.subtotal).sum());
    Ok(Json(CompraPesoHecha {
        compra_id: hecho.compra_id,
        folio: hecho.folio,
        fecha: crate::logica::tiempo::ahora_lima(),
        persona,
        lineas,
        total,
        tipo_pago: tipo_pago(&compra),
    }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn compra(json: serde_json::Value) -> CompraPeso {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn revisa_antes_de_guardar() {
        assert!(validar(&compra(serde_json::json!({"lineas": [{"producto_id": 1, "cantidad": 2.35, "precio": 38}]}))).is_ok());
        assert!(validar(&compra(serde_json::json!({"lineas": []}))).is_err());
        assert!(validar(&compra(serde_json::json!({"tipo_pago": "EFECTIVO", "lineas": [{"producto_id": 1, "cantidad": 1, "precio": 2}]}))).is_ok());
        assert!(validar(&compra(serde_json::json!({"tipo_pago": "CREDITO", "lineas": [{"producto_id": 1, "cantidad": 1, "precio": 2}]}))).is_err());
        assert!(validar(&compra(serde_json::json!({"lineas": [{"producto_id": 1, "cantidad": 0, "precio": 38}]}))).is_err());
        assert!(validar(&compra(serde_json::json!({"lineas": [{"producto_id": 1, "cantidad": 1, "precio": -1}]}))).is_err());
        assert!(validar(&compra(serde_json::json!({"lineas": [
            {"producto_id": 1, "cantidad": 1, "precio": 2}, {"producto_id": 1, "cantidad": 3, "precio": 2}
        ]})))
        .is_err());
    }
}
