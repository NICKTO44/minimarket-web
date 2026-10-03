use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct EmitirComprobanteRequest {
    pub venta_id: i64,
    pub tipo: String,
    pub cliente_documento: Option<String>,
    pub cliente_nombre: Option<String>,
    /// Solo facturas con el módulo de detracción: Some(false) = el cajero
    /// indicó que esta venta no está sujeta. Sin el campo se aplica la regla
    /// del negocio (total mayor al mínimo).
    #[serde(default)]
    pub detraccion: Option<bool>,
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
    // --- Agregado para poder armar en el frontend el QR oficial de
    // SUNAT (formato RS 193-2020/SUNAT), usando el mismo "valor resumen"
    // (hash) que FacturaLibre usó para firmar el documento real — no un
    // dato inventado ni recalculado. ---
    pub hash: Option<String>,
    pub ruc_emisor: Option<String>,
    pub fecha_emision: Option<String>,
    pub igv: f64,
    pub total_venta: f64,
    /// Desglose de IGV de la venta (para imprimir el ticket igual que el
    /// comprobante): tasa del negocio y totales por tipo de operación.
    pub igv_tasa: f64,
    pub op_gravadas: f64,
    pub op_exoneradas: f64,
    pub op_inafectas: f64,
    /// Código SUNAT del tipo de documento del adquirente (catálogo 06:
    /// "1" DNI, "6" RUC, "0" sin documento).
    pub cliente_tipo_documento_codigo: String,
    pub cliente_numero_documento: String,
    /// Detracción enviada en esta factura (None = no lleva).
    pub detraccion_porcentaje: Option<f64>,
    pub detraccion_monto: Option<f64>,
    pub detraccion_cuenta: Option<String>,
}