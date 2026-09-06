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
    // --- Agregado para poder reconstruir el QR oficial de SUNAT al
    // reimprimir desde el historial, con los mismos datos reales que se
    // usaron al emitir (no inventados). ---
    pub hash: Option<String>,
    pub cliente_documento: Option<String>,
    pub ruc_emisor: Option<String>,
    /// Solo la fecha (YYYY-MM-DD), sin hora — el formato exacto que
    /// exige el QR de SUNAT.
    pub fecha_emision_corta: Option<String>,
}