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

/// Saca el token del query string (?token=...) — usado solo por el <iframe>
/// del PDF embebido, que no puede mandar cabeceras personalizadas.
fn extraer_token_de_query(req: &Request) -> Option<String> {
    req.uri().query().and_then(|q| {
        q.split('&').find_map(|par| {
            let mut it = par.splitn(2, '=');
            let clave = it.next()?;
            let valor = it.next()?;
            (clave == "token").then(|| valor.to_string())
        })
    })
}

pub async fn requiere_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let token = match auth_header.or_else(|| extraer_token_de_query(&req)) {
        Some(t) if !t.is_empty() => t,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    // Antes: std::env::var("JWT_SECRET").expect(...) en cada petición.
    // Ahora: ya viene cargado una sola vez en AppState desde el arranque.
    let datos = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let claims = datos.claims;

    // Resuelve a qué tienda pertenece este usuario (de la caché en memoria
    // si ya se consultó antes).
    let tienda = state
        .tiendas
        .resolver_por_id(claims.tienda_id)
        .await
        .map_err(|e| {
            eprintln!("❌ Error resolviendo tienda_id {}: {}", claims.tienda_id, e);
            StatusCode::UNAUTHORIZED
        })?;

    // Antes: se reconstruía el Database (Builder::new_remote + build)
    // en cada petición. Ahora: se reutiliza el ya armado para esa tienda,
    // si existe en caché.
    let db_tienda = state
        .tiendas
        .conectar_cacheado(&tienda)
        .await
        .map_err(|e| {
            eprintln!("❌ Error conectando a la base de la tienda '{}': {}", tienda.identificador, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    req.extensions_mut().insert(claims);
    req.extensions_mut().insert(Arc::new(TenantDb(db_tienda)));

    Ok(next.run(req).await)
}