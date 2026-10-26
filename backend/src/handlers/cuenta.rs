//! Contraseñas y sesiones de los usuarios de un negocio.
//!
//! - Cada usuario cambia su propia contraseña (PUT /mi-cuenta/clave) o
//!   cierra sus sesiones en los demás dispositivos
//!   (POST /mi-cuenta/cerrar-sesiones). El dispositivo desde el que lo hace
//!   recibe un JWT nuevo y sigue trabajando.
//! - El administrador restablece la contraseña de un trabajador
//!   (POST /usuarios/:id/clave).
//! - Si el que olvida la clave es el dueño, se la restablece Monspeet desde
//!   el panel (handlers/panel.rs), con la misma `fijar_clave`.
//!
//! Todo cambio sube `usuarios.token_version` (migración 0044): los JWT
//! anteriores de ese usuario dejan de servir (ver middleware_auth.rs).

use axum::{extract::{Extension, Path, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::auth::firmar_sesion;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;
use crate::AppState;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Largo mínimo de una contraseña nueva. Las que ya existían siguen
/// valiendo aunque sean más cortas.
pub const MINIMO_CLAVE: usize = 8;

/// Revisa una contraseña nueva antes de guardarla.
pub fn validar_clave_nueva(clave: &str) -> Result<(), Fallo> {
    if clave.chars().count() < MINIMO_CLAVE {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("La contraseña debe tener al menos {} caracteres.", MINIMO_CLAVE),
        ));
    }
    if clave.trim() != clave {
        return Err((StatusCode::BAD_REQUEST, "La contraseña no puede empezar ni terminar con espacios.".to_string()));
    }
    Ok(())
}

/// Sube la versión de la sesión del usuario y devuelve la nueva.
async fn subir_version(conn: &libsql::Connection, usuario_id: i64) -> Result<i64, Fallo> {
    let cambiados = conn
        .execute(
            "UPDATE usuarios SET token_version = token_version + 1 WHERE id = ?1",
            libsql::params![usuario_id],
        )
        .await
        .map_err(|_| interno("Este negocio aún no tiene la actualización de sesiones (migración 0044). Reinicia el backend."))?;
    if cambiados == 0 {
        return Err((StatusCode::NOT_FOUND, "Usuario no encontrado.".to_string()));
    }
    Ok(crate::tenants::version_token(conn, usuario_id).await)
}

/// Guarda la contraseña nueva de un usuario y cierra todas sus sesiones.
/// Devuelve la versión nueva de la sesión. La usan "mi cuenta", el
/// administrador y el panel.
pub async fn fijar_clave(conn: &libsql::Connection, usuario_id: i64, clave: &str) -> Result<i64, Fallo> {
    validar_clave_nueva(clave)?;
    let hash = bcrypt::hash(clave, bcrypt::DEFAULT_COST).map_err(interno)?;
    let cambiados = conn
        .execute(
            "UPDATE usuarios SET password_hash = ?1 WHERE id = ?2",
            libsql::params![hash, usuario_id],
        )
        .await
        .map_err(interno)?;
    if cambiados == 0 {
        return Err((StatusCode::NOT_FOUND, "Usuario no encontrado.".to_string()));
    }
    subir_version(conn, usuario_id).await
}

#[derive(Serialize)]
pub struct SesionRenovada {
    pub ok: bool,
    pub mensaje: String,
    /// JWT nuevo para el dispositivo actual (el anterior ya no sirve).
    pub token: String,
}

fn renovar(state: &AppState, mut claims: Claims, version: i64, mensaje: &str) -> Result<Json<SesionRenovada>, Fallo> {
    claims.tv = version;
    let token = firmar_sesion(state, claims).map_err(interno)?;
    Ok(Json(SesionRenovada { ok: true, mensaje: mensaje.to_string(), token }))
}

#[derive(Deserialize)]
pub struct CambiarMiClave {
    pub actual: String,
    pub nueva: String,
}

/// PUT /mi-cuenta/clave — cualquier usuario, con su contraseña actual.
pub async fn cambiar_mi_clave(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(datos): Json<CambiarMiClave>,
) -> Result<Json<SesionRenovada>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let mut filas = conn
        .query("SELECT password_hash FROM usuarios WHERE id = ?1 AND activo = 1", libsql::params![claims.sub])
        .await
        .map_err(interno)?;
    let hash: String = match filas.next().await.map_err(interno)? {
        Some(fila) => fila.get(0).unwrap_or_default(),
        None => return Err((StatusCode::UNAUTHORIZED, "Tu usuario ya no está activo.".to_string())),
    };
    drop(filas);

    if !bcrypt::verify(&datos.actual, &hash).unwrap_or(false) {
        return Err((StatusCode::BAD_REQUEST, "Tu contraseña actual no es correcta.".to_string()));
    }
    if datos.nueva == datos.actual {
        return Err((StatusCode::BAD_REQUEST, "La contraseña nueva debe ser distinta de la actual.".to_string()));
    }

    let version = fijar_clave(&conn, claims.sub, &datos.nueva).await?;
    state.tiendas.invalidar_usuario(claims.tienda_id, claims.sub).await;
    renovar(
        &state,
        claims,
        version,
        "Contraseña cambiada. Se cerró tu sesión en los demás dispositivos.",
    )
}

/// POST /mi-cuenta/cerrar-sesiones — cierra la sesión en todos los demás
/// dispositivos (por ejemplo, si dejó la sesión abierta en otra PC).
pub async fn cerrar_otras_sesiones(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SesionRenovada>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let version = subir_version(&conn, claims.sub).await?;
    state.tiendas.invalidar_usuario(claims.tienda_id, claims.sub).await;
    renovar(&state, claims, version, "Listo: se cerró tu sesión en los demás dispositivos.")
}

#[derive(Deserialize)]
pub struct RestablecerClave {
    pub nueva: String,
}

#[derive(Serialize)]
pub struct ClaveRestablecida {
    pub ok: bool,
    pub mensaje: String,
}

/// POST /usuarios/:id/clave — el administrador le pone una contraseña nueva
/// a un trabajador de su negocio. Su propia clave la cambia en "mi cuenta"
/// (que pide la actual).
pub async fn restablecer_clave_usuario(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(datos): Json<RestablecerClave>,
) -> Result<Json<ClaveRestablecida>, Fallo> {
    exigir_admin(&claims)?;
    if id == claims.sub {
        return Err((
            StatusCode::BAD_REQUEST,
            "Tu propia contraseña cámbiala desde \"Cambiar contraseña\" (te pide la actual).".to_string(),
        ));
    }
    let conn = tenant.0.connect().map_err(interno)?;
    fijar_clave(&conn, id, &datos.nueva).await?;
    state.tiendas.invalidar_usuario(claims.tienda_id, id).await;
    Ok(Json(ClaveRestablecida {
        ok: true,
        mensaje: "Contraseña restablecida. Si tenía la sesión abierta, tendrá que volver a entrar con la nueva.".to_string(),
    }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn largo_minimo() {
        assert!(validar_clave_nueva("1234567").is_err());
        assert!(validar_clave_nueva("12345678").is_ok());
        assert!(validar_clave_nueva("ñandú-ok").is_ok());
        assert!(validar_clave_nueva(" con-espacio").is_err());
        assert!(validar_clave_nueva("con-espacio ").is_err());
    }
}
