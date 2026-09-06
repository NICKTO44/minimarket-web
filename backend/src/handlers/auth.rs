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
    /// true si el negocio está en modo lectura (no puede procesar
    /// ventas ni modificar nada) — el frontend debe mostrar un aviso
    /// persistente y deshabilitar las acciones de escritura de una vez,
    /// sin esperar a que el backend rechace cada intento.
    pub modo_lectura: bool,
    /// Mensaje para mostrar al usuario cuando `modo_lectura` es true.
    pub aviso: Option<String>,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    if payload.usuario.trim().is_empty() || payload.password.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Completa usuario y contraseña.".to_string()));
    }

    // 1. Resolver a qué negocio pertenece este login.
    let tienda = match payload.tienda.as_deref() {
        Some(identificador) if !identificador.trim().is_empty() => {
            state.tiendas.buscar_por_identificador(identificador).await
        }
        _ => state.tiendas.buscar_por_usuario(&payload.usuario).await,
    }
    .map_err(|_| (StatusCode::UNAUTHORIZED, "Usuario o contraseña incorrectos".to_string()))?;

    // 2. Control de suscripción. "Modo lectura" (no pagó todavía) SÍ
    // permite iniciar sesión — solo "Bloqueado" (casos extremos) impide
    // entrar del todo.
    let (modo_lectura, aviso) = match tienda.nivel_acceso() {
        NivelAcceso::Bloqueado(motivo) => return Err((StatusCode::FORBIDDEN, motivo)),
        NivelAcceso::SoloLectura(motivo) => (true, Some(motivo)),
        NivelAcceso::Completo => (false, None),
    };

    // 3. Conectarse a la base de ESE negocio y validar las credenciales ahí.
    //    Se usa la versión cacheada: así la conexión que se arma aquí en
    //    el login queda lista para reutilizarse en las peticiones
    //    autenticadas que vengan después de esta misma sesión, en vez de
    //    construirse dos veces (una en el login, otra en la primera
    //    petición del middleware).
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
        },
        tienda: TiendaSesion {
            identificador: tienda.identificador,
            nombre_negocio: tienda.nombre_negocio,
        },
        modo_lectura,
        aviso,
    }))
}