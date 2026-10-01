use serde::{Deserialize, Serialize};

/// Un plato de la "Carta de hoy".
#[derive(Debug, Serialize)]
pub struct PlatoCarta {
    pub id: i64,
    pub nombre: String,
    pub precio: f64,
    pub agotado: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoPlato {
    pub nombre: String,
    pub precio: f64,
}

/// Lo que no venga se deja como está.
#[derive(Debug, Deserialize)]
pub struct ActualizarPlato {
    pub nombre: Option<String>,
    pub precio: Option<f64>,
    pub agotado: Option<bool>,
}
