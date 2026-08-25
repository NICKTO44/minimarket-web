use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ComprobanteInfo {
    pub tipo: String,
    pub serie: String,
    pub numero: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductoVentaDetalle {
    pub detalle_id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub subtotal: f64,
}

#[derive(Debug, Serialize)]
pub struct VentaParaDevolucion {
    pub venta_id: i64,
    pub folio: String,
    pub fecha_hora: String,
    pub total: f64,
    pub metodo_pago: String,
    pub productos: Vec<ProductoVentaDetalle>,
    pub comprobante: Option<ComprobanteInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ProductoDevolver {
    pub detalle_id: i64,
    pub producto_id: i64,
    pub cantidad: f64,
}

#[derive(Debug, Deserialize)]
pub struct NuevaDevolucion {
    pub venta_id: i64,
    pub productos: Vec<ProductoDevolver>,
    pub motivo: String,
    pub usuario_id: i64,
}

#[derive(Debug, Serialize)]
pub struct DevolucionResponse {
    pub success: bool,
    pub message: String,
    pub folio_devolucion: Option<String>,
}