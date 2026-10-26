use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64, // id del usuario
    pub username: String,
    pub rol_id: i64,
    pub nombre_completo: String,
    pub tienda_id: i64,
    pub exp: usize,
    /// Versión de la sesión (usuarios.token_version) al iniciar sesión. Los
    /// JWT emitidos antes de que existiera no lo traen: se leen como 0.
    #[serde(default)]
    pub tv: i64,
}