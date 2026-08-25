use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ConfiguracionTienda {
    pub id: i64,
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub ruc: Option<String>,
    pub moneda: String,
    pub iva_porcentaje: f64,
    pub facturalibre_token: Option<String>,
    pub facturalibre_ruta: Option<String>,
    pub codigo_producto_sunat_generico: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActualizarConfiguracion {
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub ruc: Option<String>,
    pub moneda: String,
    pub iva_porcentaje: f64,
    pub facturalibre_token: Option<String>,
    pub facturalibre_ruta: Option<String>,
    pub codigo_producto_sunat_generico: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsuarioResumen {
    pub id: i64,
    pub username: String,
    pub nombre_completo: String,
    pub rol_id: i64,
    pub rol_nombre: String,
    pub activo: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoUsuario {
    pub username: String,
    pub password: String,
    pub nombre_completo: String,
    pub rol_id: i64,
}

#[derive(Debug, Serialize)]
pub struct AccionResponse {
    pub success: bool,
    pub message: String,
}