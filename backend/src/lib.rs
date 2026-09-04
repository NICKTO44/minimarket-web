use std::sync::Arc;

pub mod models;
pub mod handlers;
pub mod logica;
pub mod middleware_auth;
pub mod estado_impresion;
pub mod tenants;
pub mod rate_limit;
pub mod crypto;
pub mod migraciones;

pub struct AppState {
    pub db: libsql::Database,
    pub tiendas: tenants::RegistroTiendas,
    pub estado_impresion: Arc<estado_impresion::EstadoImpresion>,
    pub limitador_login: Arc<rate_limit::LimitadorIntentos>,
    /// Leído del .env una sola vez al arrancar, en vez de en cada
    /// petición autenticada (como hacía antes middleware_auth.rs).
    pub jwt_secret: String,
}