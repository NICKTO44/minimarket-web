use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductoVenta {
    pub id: i64,
    pub nombre: String,
    pub precio: f64,
    pub cantidad: f64,
    #[serde(rename = "descuentoMonto")]
    pub descuento_monto: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct NuevaVenta {
    pub productos: Vec<ProductoVenta>,
    pub total: f64,
    pub metodo_pago: String,
    pub monto_recibido: Option<f64>,
    pub cambio: Option<f64>,
    pub usuario_id: i64,
    pub cliente_id: Option<i64>,
    // Solo para metodo_pago = "MIXTO" (efectivo + un medio digital).
    // Opcionales para que el frontend anterior siga funcionando igual.
    #[serde(default)]
    pub pago_efectivo: Option<f64>,
    #[serde(default)]
    pub pago_otro: Option<f64>,
    #[serde(default)]
    pub pago_otro_metodo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VentaResult {
    pub venta_id: i64,
    pub folio: String,
}