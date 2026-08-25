use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Lote {
    pub id: i64,
    pub producto_id: i64,
    pub cantidad: f64,
    pub fecha_vencimiento: String,
    pub numero_lote: Option<String>,
    pub activo: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoLote {
    pub producto_id: i64,
    pub cantidad: f64,
    pub fecha_vencimiento: String,
    pub numero_lote: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct LoteAlerta {
    pub lote_id: i64,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub cantidad: f64,
    pub unidad_medida: String,
    pub fecha_vencimiento: String,
    pub dias_restantes: i64, // negativo = ya vencido
}

#[derive(Debug, Serialize)]
pub struct LoteAccionResponse {
    pub success: bool,
    pub message: String,

}
