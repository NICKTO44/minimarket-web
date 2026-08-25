use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use jsonwebtoken::{encode, EncodingKey, Header};
use chrono::{Utc, Duration};

use crate::AppState;
use crate::models::auth::Claims;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub usuario: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UsuarioSesion {
    pub id: i64,
    pub username: String,
    pub nombre_completo: String,
    pub rol_id: i64,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub token: String,
    pub usuario: UsuarioSesion,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    if payload.usuario.trim().is_empty() || payload.password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, password_hash, nombre_completo, rol_id FROM usuarios WHERE username = ?1 AND activo = 1",
            libsql::params![payload.usuario.clone()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = match rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(r) => r,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let id: i64 = row.get(0).unwrap_or_default();
    let hash: String = row.get(1).unwrap_or_default();
    let nombre_completo: String = row.get(2).unwrap_or_default();
    let rol_id: i64 = row.get(3).unwrap_or_default();

    let valido = bcrypt::verify(&payload.password, &hash).unwrap_or(false);
    if !valido {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let jwt_secret = std::env::var("JWT_SECRET").expect("Falta JWT_SECRET en .env");
    let exp = (Utc::now() + Duration::hours(12)).timestamp() as usize;

    let claims = Claims {
        sub: id,
        username: payload.usuario.clone(),
        rol_id,
        nombre_completo: nombre_completo.clone(),
        exp,
    };

    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(jwt_secret.as_bytes()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse {
        ok: true,
        token,
        usuario: UsuarioSesion {
            id,
            username: payload.usuario,
            nombre_completo,
            rol_id,
        },
    }))
}