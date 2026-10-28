use serde::{Deserialize, Serialize};

/// Rubro del negocio y módulos encendidos.
#[derive(Debug, Serialize, Clone)]
pub struct Negocio {
    pub rubro: String,
    /// Incluye "MESAS" cuando el negocio atiende en mesas.
    pub modulos: Vec<String>,
    /// 'TIENDA' o 'RESTAURANTE': se mantiene para las pantallas de siempre.
    pub modo_negocio: String,
    /// El rubro quedó elegido (al crear la cuenta): ya no se cambia desde el
    /// sistema; solo se encienden o apagan módulos. false = negocio antiguo
    /// que nunca eligió rubro: puede elegirlo una vez.
    pub rubro_fijo: bool,
}

#[derive(Debug, Deserialize)]
pub struct GuardarNegocio {
    pub rubro: String,
    pub modulos: Vec<String>,
}
