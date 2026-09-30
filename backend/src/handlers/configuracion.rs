use axum::{extract::{Extension, Path, State}, Json, http::StatusCode};
use std::sync::Arc;

use crate::AppState;
use crate::middleware_auth::{exigir_admin, ROL_ADMIN};
use crate::models::auth::Claims;

use crate::tenants::TenantDb;
use crate::models::configuracion::*;

pub async fn obtener_configuracion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ConfiguracionTienda>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, nombre_tienda, direccion, telefono, email, ruc, moneda, iva_porcentaje,
                    facturalibre_token, facturalibre_ruta, codigo_producto_sunat_generico,
                    serie_boleta, serie_factura, logo_path, color_acento
             FROM configuracion_tienda LIMIT 1",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(row) => {
            let mut config = ConfiguracionTienda {
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
            serie_boleta: row.get(11).ok(),
            serie_factura: row.get(12).ok(),
            logo_path: row.get(13).ok(),
            color_acento: row.get(14).ok(),
            modo_negocio: if crate::handlers::mesas::modo_restaurante(&conn).await {
                "RESTAURANTE".to_string()
            } else {
                "TIENDA".to_string()
            },
            };
            // Todos los usuarios leen la configuración (nombre, RUC, series...),
            // pero el token de FacturaLibre es un secreto: solo lo ve el
            // administrador. Al cajero se le envía enmascarado para que el POS
            // siga sabiendo que la facturación está configurada.
            if claims.rol_id != ROL_ADMIN {
                if let Some(token) = &config.facturalibre_token {
                    if !token.trim().is_empty() {
                        config.facturalibre_token = Some("********".to_string());
                    }
                }
            }
            Ok(Json(config))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn actualizar_configuracion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<ActualizarConfiguracion>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE configuracion_tienda SET nombre_tienda=?1, direccion=?2, telefono=?3, email=?4,
            ruc=?5, moneda=?6, iva_porcentaje=?7, facturalibre_token=?8, facturalibre_ruta=?9,
            codigo_producto_sunat_generico=?10, serie_boleta=?11, serie_factura=?12,
            color_acento=?13,
            fecha_actualizacion = datetime('now','localtime')",
        libsql::params![
            payload.nombre_tienda.clone(), payload.direccion.clone(), payload.telefono.clone(),
            payload.email.clone(), payload.ruc.clone(), payload.moneda.clone(), payload.iva_porcentaje,
            payload.facturalibre_token.clone(), payload.facturalibre_ruta.clone(),
            payload.codigo_producto_sunat_generico.clone(), payload.serie_boleta.clone(), payload.serie_factura.clone(),
            payload.color_acento.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Configuración actualizada".into() }))
}

pub async fn listar_usuarios(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<UsuarioResumen>>, StatusCode> {
    if claims.rol_id != ROL_ADMIN {
        return Err(StatusCode::FORBIDDEN);
    }
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
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevoUsuario>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    exigir_admin(&claims)?;

    let username = payload.username.trim().to_string();
    if username.chars().count() < 3 || username.chars().any(|c| c.is_whitespace()) {
        return Err((StatusCode::BAD_REQUEST, "El usuario debe tener al menos 3 caracteres y no llevar espacios.".into()));
    }
    if payload.nombre_completo.trim().is_empty() || payload.password.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Completa usuario, contraseña y nombre completo.".into()));
    }

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 1. Los nombres de usuario son únicos en TODO Monspeet: el login busca
    //    el usuario en el índice central para saber a qué negocio entrar.
    let ya_existe = "Ya existe un usuario con ese nombre en tu negocio.".to_string();
    match state.tiendas.tienda_de_usuario(&username).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        Some(tienda_id) if tienda_id == claims.tienda_id => return Err((StatusCode::CONFLICT, ya_existe)),
        Some(_) => {
            return Err((StatusCode::CONFLICT, format!(
                "El usuario \"{}\" ya está en uso en Monspeet (los usuarios son únicos en todo el sistema). Prueba con otro, por ejemplo agregando el nombre de tu negocio.",
                username
            )));
        }
        None => {}
    }
    let mut r_local = conn
        .query("SELECT COUNT(*) FROM usuarios WHERE username = ?1", libsql::params![username.clone()])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let existe_local: i64 = match r_local.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(fila) => fila.get(0).unwrap_or(0),
        None => 0,
    };
    if existe_local > 0 {
        return Err((StatusCode::CONFLICT, ya_existe));
    }

    // 2. Crear el usuario en la base del negocio.
    let hash = bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al encriptar contraseña: {}", e)))?;

    conn.execute(
        "INSERT INTO usuarios (username, password_hash, nombre_completo, rol_id) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![username.clone(), hash, payload.nombre_completo.trim().to_string(), payload.rol_id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear usuario: {}", e)))?;
    let nuevo_id = conn.last_insert_rowid();

    // 3. Registrarlo en el índice central. Si falla, se deshace el paso 2
    //    para no dejar un usuario que no podría iniciar sesión.
    if let Err(e) = state.tiendas.registrar_usuario_en_indice(&username, claims.tienda_id).await {
        let _ = conn.execute("DELETE FROM usuarios WHERE id = ?1", libsql::params![nuevo_id]).await;
        eprintln!("❌ No se pudo registrar '{}' en usuarios_indice: {}", username, e);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "No se pudo registrar el usuario. Intenta de nuevo.".into()));
    }

    Ok(Json(AccionResponse { success: true, message: "Usuario creado exitosamente".into() }))
}

pub async fn desactivar_usuario(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    if id == claims.sub {
        return Err((StatusCode::BAD_REQUEST, "No puedes desactivar tu propio usuario.".into()));
    }

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r = conn
        .query("SELECT rol_id FROM usuarios WHERE id = ?1", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rol_id: i64 = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(fila) => fila.get(0).unwrap_or(0),
        None => return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".into())),
    };

    // Nunca dejar al negocio sin ningún administrador activo.
    if rol_id == ROL_ADMIN {
        let mut r2 = conn
            .query(
                "SELECT COUNT(*) FROM usuarios WHERE rol_id = ?1 AND activo = 1 AND id != ?2",
                libsql::params![ROL_ADMIN, id],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let otros_admins: i64 = match r2.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(fila) => fila.get(0).unwrap_or(0),
            None => 0,
        };
        if otros_admins == 0 {
            return Err((StatusCode::BAD_REQUEST, "Debe quedar al menos un administrador activo en el negocio.".into()));
        }
    }

    conn.execute(
        "UPDATE usuarios SET activo = 0 WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    // Pierde el acceso de inmediato, aunque tenga una sesión abierta.
    state.tiendas.invalidar_usuario(claims.tienda_id, id).await;

    Ok(Json(AccionResponse { success: true, message: "Usuario desactivado".into() }))
}

pub async fn reactivar_usuario(
    State(state): State<Arc<AppState>>,
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r = conn
        .query("SELECT username, activo FROM usuarios WHERE id = ?1", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (username, activo): (String, i64) = match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(fila) => (fila.get(0).unwrap_or_default(), fila.get(1).unwrap_or(1)),
        None => return Err((StatusCode::NOT_FOUND, "Usuario no encontrado".into())),
    };
    if activo == 1 {
        return Err((StatusCode::BAD_REQUEST, "Ese usuario ya está activo.".into()));
    }

    // Asegurar que pueda iniciar sesión: debe estar en el índice central
    // y a nombre de ESTE negocio.
    match state.tiendas.tienda_de_usuario(&username).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        Some(tienda_id) if tienda_id == claims.tienda_id => {}
        Some(_) => {
            return Err((StatusCode::CONFLICT, format!(
                "No se puede reactivar: el usuario \"{}\" ahora lo usa otro negocio en Monspeet. Crea un usuario nuevo para esta persona.",
                username
            )));
        }
        None => {
            state.tiendas.registrar_usuario_en_indice(&username, claims.tienda_id).await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo reactivar: {}", e)))?;
        }
    }

    conn.execute("UPDATE usuarios SET activo = 1 WHERE id = ?1", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al reactivar: {}", e)))?;

    state.tiendas.invalidar_usuario(claims.tienda_id, id).await;

    Ok(Json(AccionResponse { success: true, message: "Usuario reactivado".into() }))
}
