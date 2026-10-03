use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use jsonwebtoken::{encode, EncodingKey, Header};
use chrono::{Utc, Duration};

use crate::AppState;
use crate::models::auth::Claims;
use crate::tenants::NivelAcceso;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub usuario: String,
    pub password: String,
    pub tienda: Option<String>,
}

#[derive(Serialize)]
pub struct UsuarioSesion {
    pub id: i64,
    pub username: String,
    pub nombre_completo: String,
    pub rol_id: i64,
    /// Nombre del rol (ADMIN, CAJERO, MESERO...): el id del Mesero puede
    /// variar entre negocios, el nombre no.
    pub rol_nombre: Option<String>,
}

#[derive(Serialize)]
pub struct TiendaSesion {
    pub identificador: String,
    pub nombre_negocio: String,
    pub logo_url: Option<String>,
    pub color_acento: Option<String>,
    /// 'TIENDA' o 'RESTAURANTE' (atención en mesas).
    pub modo_negocio: String,
    /// Rubro y módulos encendidos (ver handlers/rubros.rs).
    pub rubro: String,
    pub modulos: Vec<String>,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub token: String,
    pub usuario: UsuarioSesion,
    pub tienda: TiendaSesion,
    pub modo_lectura: bool,
    pub aviso: Option<String>,
}

async fn leer_identidad_visual(conn: &libsql::Connection) -> (Option<String>, Option<String>) {
    let resultado = conn
        .query("SELECT logo_path, color_acento FROM configuracion_tienda LIMIT 1", ())
        .await;

    let mut rows = match resultado {
        Ok(r) => r,
        Err(_) => return (None, None),
    };

    match rows.next().await {
        Ok(Some(row)) => (row.get(0).ok(), row.get(1).ok()),
        _ => (None, None),
    }
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    if payload.usuario.trim().is_empty() || payload.password.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Completa usuario y contraseña.".to_string()));
    }

    let tienda = match payload.tienda.as_deref() {
        Some(identificador) if !identificador.trim().is_empty() => {
            state.tiendas.buscar_por_identificador(identificador).await
        }
        _ => state.tiendas.buscar_por_usuario(&payload.usuario).await,
    }
    .map_err(|_| (StatusCode::UNAUTHORIZED, "Usuario o contraseña incorrectos".to_string()))?;

    let (modo_lectura, aviso) = match tienda.nivel_acceso() {
        NivelAcceso::Bloqueado(motivo) => return Err((StatusCode::FORBIDDEN, motivo)),
        NivelAcceso::SoloLectura(motivo) => (true, Some(motivo)),
        NivelAcceso::Completo => (false, None),
    };

    let db_tienda = state
        .tiendas
        .conectar_cacheado(&tienda)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "No se pudo conectar a tu negocio.".to_string()))?;

    let conn = db_tienda
        .connect()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "No se pudo conectar a tu negocio.".to_string()))?;

    let mut rows = conn
        .query(
            "SELECT id, password_hash, nombre_completo, rol_id FROM usuarios WHERE username = ?1 AND activo = 1",
            libsql::params![payload.usuario.clone()],
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error consultando el usuario.".to_string()))?;

    let row = match rows
        .next()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error consultando el usuario.".to_string()))?
    {
        Some(r) => r,
        None => return Err((StatusCode::UNAUTHORIZED, "Usuario o contraseña incorrectos".to_string())),
    };

    let id: i64 = row.get(0).unwrap_or_default();
    let hash: String = row.get(1).unwrap_or_default();
    let nombre_completo: String = row.get(2).unwrap_or_default();
    let rol_id: i64 = row.get(3).unwrap_or_default();

    let valido = bcrypt::verify(&payload.password, &hash).unwrap_or(false);
    if !valido {
        return Err((StatusCode::UNAUTHORIZED, "Usuario o contraseña incorrectos".to_string()));
    }

    let (logo_url, color_acento) = leer_identidad_visual(&conn).await;
    let negocio = crate::handlers::rubros::negocio(&conn).await;

    let rol_nombre: Option<String> = match conn
        .query("SELECT nombre FROM roles WHERE id = ?1", libsql::params![rol_id])
        .await
    {
        Ok(mut filas) => match filas.next().await {
            Ok(Some(fila)) => fila.get(0).ok(),
            _ => None,
        },
        Err(_) => None,
    };

    let exp = (Utc::now() + Duration::hours(12)).timestamp() as usize;

    let claims = Claims {
        sub: id,
        username: payload.usuario.clone(),
        rol_id,
        nombre_completo: nombre_completo.clone(),
        tienda_id: tienda.id,
        exp,
    };

    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(state.jwt_secret.as_bytes()))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "No se pudo generar la sesión.".to_string()))?;

    Ok(Json(LoginResponse {
        ok: true,
        token,
        usuario: UsuarioSesion {
            id,
            username: payload.usuario,
            nombre_completo,
            rol_id,
            rol_nombre,
        },
        tienda: TiendaSesion {
            identificador: tienda.identificador,
            nombre_negocio: tienda.nombre_negocio,
            logo_url,
            color_acento,
            modo_negocio: negocio.modo_negocio,
            rubro: negocio.rubro,
            modulos: negocio.modulos,
        },
        modo_lectura,
        aviso,
    }))
}

#[derive(Deserialize)]
pub struct IdentificarUsuarioRequest {
    pub usuario: String,
}

#[derive(Serialize)]
pub struct IdentificarUsuarioResponse {
    pub ok: bool,
    pub tienda: TiendaSesion,
}

pub async fn identificar_usuario(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IdentificarUsuarioRequest>,
) -> Result<Json<IdentificarUsuarioResponse>, (StatusCode, String)> {
    if payload.usuario.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Escribe tu usuario.".to_string()));
    }

    let tienda = state
        .tiendas
        .buscar_por_usuario(&payload.usuario)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Usuario o contraseña incorrectos".to_string()))?;

    // Si la base del negocio no respondiera, se muestra como una tienda sin
    // módulos extra (igual que antes); el login real lo corrige.
    let sin_datos = || crate::models::rubro::Negocio {
        rubro: crate::handlers::rubros::RUBRO_GENERAL.to_string(),
        modulos: Vec::new(),
        modo_negocio: "TIENDA".to_string(),
    };
    let (logo_url, color_acento, negocio) = match state.tiendas.conectar_cacheado(&tienda).await {
        Ok(db) => match db.connect() {
            Ok(conn) => {
                let (logo, color) = leer_identidad_visual(&conn).await;
                (logo, color, crate::handlers::rubros::negocio(&conn).await)
            }
            Err(_) => (None, None, sin_datos()),
        },
        Err(_) => (None, None, sin_datos()),
    };

    Ok(Json(IdentificarUsuarioResponse {
        ok: true,
        tienda: TiendaSesion {
            identificador: tienda.identificador,
            nombre_negocio: tienda.nombre_negocio,
            logo_url,
            color_acento,
            modo_negocio: negocio.modo_negocio,
            rubro: negocio.rubro,
            modulos: negocio.modulos,
        },
    }))
}