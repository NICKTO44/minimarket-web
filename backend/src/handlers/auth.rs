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
    /// Identificador único del negocio (ej. "bodega-juan"), si el navegador
    /// ya lo recuerda de un login anterior. Si viene vacío o ausente, se
    /// asume que es el primer login de este dispositivo, y se busca el
    /// usuario en el índice central (ahí solo está el súper admin de cada
    /// negocio, que es el único registrado ahí).
    pub tienda: Option<String>,
}

#[derive(Serialize)]
pub struct UsuarioSesion {
    pub id: i64,
    pub username: String,
    pub nombre_completo: String,
    pub rol_id: i64,
}

#[derive(Serialize)]
pub struct TiendaSesion {
    pub identificador: String,
    pub nombre_negocio: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub token: String,
    pub usuario: UsuarioSesion,
    pub tienda: TiendaSesion,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    if payload.usuario.trim().is_empty() || payload.password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 1. Resolver a qué negocio pertenece este login.
    let tienda = match payload.tienda.as_deref() {
        Some(identificador) if !identificador.trim().is_empty() => {
            state.tiendas.buscar_por_identificador(identificador).await
        }
        _ => state.tiendas.buscar_por_usuario(&payload.usuario).await,
    }
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // 2. Conectarse a la base de ESE negocio y validar las credenciales ahí.
    let db_tienda = state
        .tiendas
        .conectar(&tienda)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let conn = db_tienda.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
        tienda_id: tienda.id,
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
        tienda: TiendaSesion {
            identificador: tienda.identificador,
            nombre_negocio: tienda.nombre_negocio,
        },
    }))
}