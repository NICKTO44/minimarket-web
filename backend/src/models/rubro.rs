use serde::{Deserialize, Serialize};

/// Rubro del negocio y módulos encendidos.
#[derive(Debug, Serialize, Clone)]
pub struct Negocio {
    pub rubro: String,
    /// Incluye "MESAS" cuando el negocio atiende en mesas.
    pub modulos: Vec<String>,
    /// 'TIENDA' o 'RESTAURANTE': se mantiene para las pantallas de siempre.
    pub modo_negocio: String,
}

#[derive(Debug, Deserialize)]
pub struct GuardarNegocio {
    pub rubro: String,
    pub modulos: Vec<String>,
}
