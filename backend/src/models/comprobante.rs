use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ComprobanteResumen {
    pub id: Option<i64>,
    pub venta_id: i64,
    pub folio_venta: String,
    pub tipo: String,
    pub serie: Option<String>,
    pub numero: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub monto: f64,
    pub estado: Option<String>,
    pub fecha_emision: String,
    pub mensaje_sunat: Option<String>,
    pub enlace_pdf: Option<String>,
}