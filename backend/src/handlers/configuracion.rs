use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::configuracion::*;

pub async fn obtener_configuracion(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<ConfiguracionTienda>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, nombre_tienda, direccion, telefono, email, ruc, moneda, iva_porcentaje,
                    facturalibre_token, facturalibre_ruta, codigo_producto_sunat_generico
             FROM configuracion_tienda LIMIT 1",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => Ok(Json(ConfiguracionTienda {
            id: row.get(0).unwrap_or_default(),
            nombre_tienda: row.get(1).unwrap_or_default(),
            direccion: row.get(2).ok(),
            telefono: row.get(3).ok(),
            email: row.get(4).ok(),
            ruc: row.get(5).ok(),
            moneda: row.get(6).unwrap_or_else(|_| "PEN".to_string()),
            iva_porcentaje: row.get(7).unwrap_or(18.0),
            facturalibre_token: row.get(8).ok(),
            facturalibre_ruta: row.get(9).ok(),
            codigo_producto_sunat_generico: row.get(10).ok(),
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn actualizar_configuracion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<ActualizarConfiguracion>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE configuracion_tienda SET nombre_tienda=?1, direccion=?2, telefono=?3, email=?4,
            ruc=?5, moneda=?6, iva_porcentaje=?7, facturalibre_token=?8, facturalibre_ruta=?9,
            codigo_producto_sunat_generico=?10, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![
            payload.nombre_tienda.clone(), payload.direccion.clone(), payload.telefono.clone(),
            payload.email.clone(), payload.ruc.clone(), payload.moneda.clone(), payload.iva_porcentaje,
            payload.facturalibre_token.clone(), payload.facturalibre_ruta.clone(), payload.codigo_producto_sunat_generico.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Configuración actualizada".into() }))
}

pub async fn listar_usuarios(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<UsuarioResumen>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT u.id, u.username, u.nombre_completo, u.rol_id, r.nombre, u.activo
             FROM usuarios u JOIN roles r ON r.id = u.rol_id
             ORDER BY u.nombre_completo ASC",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut usuarios = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        usuarios.push(UsuarioResumen {
            id: row.get(0).unwrap_or_default(),
            username: row.get(1).unwrap_or_default(),
            nombre_completo: row.get(2).unwrap_or_default(),
            rol_id: row.get(3).unwrap_or_default(),
            rol_nombre: row.get(4).unwrap_or_default(),
            activo: row.get::<i64>(5).unwrap_or(1) == 1,
        });
    }

    Ok(Json(usuarios))
}

pub async fn crear_usuario(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoUsuario>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let hash = bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al encriptar contraseña: {}", e)))?;

    conn.execute(
        "INSERT INTO usuarios (username, password_hash, nombre_completo, rol_id) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![payload.username.clone(), hash, payload.nombre_completo.clone(), payload.rol_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear usuario (¿username duplicado?): {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Usuario creado exitosamente".into() }))
}

pub async fn desactivar_usuario(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE usuarios SET activo = 0 WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Usuario desactivado".into() }))
}