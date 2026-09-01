use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64, // id del usuario
    pub username: String,
    pub rol_id: i64,
    pub nombre_completo: String,
    pub tienda_id: i64,
    pub exp: usize,
}