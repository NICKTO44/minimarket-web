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
    /// ¿Se puede descargar el XML firmado y la constancia de SUNAT (CDR)?
    /// La pantalla de Comprobantes solo ofrece lo que existe.
    pub tiene_xml: bool,
    pub tiene_cdr: bool,
    /// 'SUNAT_DIRECTO' o 'FACTURALIBRE' (None = nota simple o base antigua).
    pub proveedor: Option<String>,
    /// Notas de crédito que corrigen este comprobante (emisión directa).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notas: Vec<crate::handlers::notas_credito::NotaResumen>,
    /// Anulación ante SUNAT: EN_PROCESO, ANULADO o RECHAZADA (None = vigente).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anulacion: Option<String>,
}