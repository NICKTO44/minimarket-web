use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct EmitirComprobanteRequest {
    pub venta_id: i64,
    pub tipo: String,
    pub cliente_documento: Option<String>,
    pub cliente_nombre: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ComprobanteResponse {
    pub success: bool,
    pub comprobante_id: Option<i64>,
    pub tipo: String,
    pub serie: String,
    pub numero: i64,
    pub estado: String,
    pub mensaje: String,
    pub enlace_pdf: Option<String>,
}