use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cliente {
    pub id: i64,
    pub tipo_documento: String,
    pub numero_documento: Option<String>,
    pub nombre_razon_social: String,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub direccion: Option<String>,
    pub activo: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoCliente {
    pub tipo_documento: String,
    pub numero_documento: Option<String>,
    pub nombre_razon_social: String,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub direccion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActualizarCliente {
    pub tipo_documento: String,
    pub numero_documento: Option<String>,
    pub nombre_razon_social: String,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub direccion: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClienteResponse {
    pub success: bool,
    pub message: String,
}