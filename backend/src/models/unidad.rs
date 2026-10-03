use serde::{Deserialize, Serialize};

/// Cuántos productos activos usan una unidad (esa no se puede apagar).
#[derive(Debug, Serialize)]
pub struct UnidadEnUso {
    pub unidad: String,
    pub productos: i64,
}

#[derive(Debug, Serialize)]
pub struct UnidadesNegocio {
    /// Códigos activos, en el orden del catálogo. Siempre incluye UNIDAD y
    /// las que ya usan productos.
    pub activas: Vec<String>,
    pub en_uso: Vec<UnidadEnUso>,
}

#[derive(Debug, Deserialize)]
pub struct GuardarUnidades {
    pub activas: Vec<String>,
}
