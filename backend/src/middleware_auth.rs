use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    http::{StatusCode, header},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::tenants::TenantDb;
use crate::AppState;

pub async fn requiere_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    let jwt_secret = std::env::var("JWT_SECRET").expect("Falta JWT_SECRET en .env");

    let datos = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let claims = datos.claims;

    // Resuelve a qué tienda pertenece este usuario (de la caché en memoria
    // si ya se consultó antes) y abre una conexión a su base.
    let tienda = state
        .tiendas
        .resolver_por_id(claims.tienda_id)
        .await
        .map_err(|e| {
            eprintln!("❌ Error resolviendo tienda_id {}: {}", claims.tienda_id, e);
            StatusCode::UNAUTHORIZED
        })?;

    let db_tienda = state
        .tiendas
        .conectar(&tienda)
        .await
        .map_err(|e| {
            eprintln!("❌ Error conectando a la base de la tienda '{}': {}", tienda.identificador, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    req.extensions_mut().insert(claims);
    req.extensions_mut().insert(Arc::new(TenantDb(db_tienda)));

    Ok(next.run(req).await)
}