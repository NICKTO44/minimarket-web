use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemBoleta {
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub subtotal: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatosImpresion {
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub items: Vec<ItemBoleta>,
    pub total: f64,
    pub efectivo: Option<f64>,
    pub cambio: Option<f64>,
    pub numero_boleta: Option<String>,
    pub cajero: Option<String>,
    pub impresora_ip: String,
    pub impresora_puerto: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImpresionResponse {
    pub success: bool,
    pub message: String,
}