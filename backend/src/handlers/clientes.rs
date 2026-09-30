use axum::{extract::{Extension, Query, Path}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::cliente::*;

#[derive(Deserialize)]
pub struct BusquedaClientes {
    pub q: Option<String>,
}

pub async fn buscar_clientes(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(params): Query<BusquedaClientes>,
) -> Result<Json<Vec<Cliente>>, StatusCode> {
    let texto = params.q.unwrap_or_default();
    if texto.trim().len() < 2 {
        return Ok(Json(vec![]));
    }
    let filtro = format!("%{}%", texto);

    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = conn
        .query(
            "SELECT id, tipo_documento, numero_documento, nombre_razon_social, telefono, email, direccion, activo
             FROM clientes
             WHERE activo = 1 AND (nombre_razon_social LIKE ?1 OR numero_documento LIKE ?1)
             ORDER BY nombre_razon_social ASC LIMIT 15",
            libsql::params![filtro],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut clientes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        clientes.push(Cliente {
            id: row.get(0).unwrap_or_default(),
            tipo_documento: row.get(1).unwrap_or_default(),
            numero_documento: row.get(2).ok(),
            nombre_razon_social: row.get(3).unwrap_or_default(),
            telefono: row.get(4).ok(),
            email: row.get(5).ok(),
            direccion: row.get(6).ok(),
            activo: row.get::<i64>(7).unwrap_or(1) == 1,
        });
    }

    Ok(Json(clientes))
}

pub async fn listar_clientes(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Cliente>>, StatusCode> {
    consultar_clientes(&tenant, 1).await.map(Json)
}

/// Clientes desactivados: ya no salen en las búsquedas del POS, pero se
/// pueden ver aparte y reactivar.
pub async fn listar_clientes_desactivados(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Cliente>>, StatusCode> {
    consultar_clientes(&tenant, 0).await.map(Json)
}

async fn consultar_clientes(tenant: &TenantDb, activo: i64) -> Result<Vec<Cliente>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, tipo_documento, numero_documento, nombre_razon_social, telefono, email, direccion, activo
             FROM clientes WHERE activo = ?1 ORDER BY nombre_razon_social ASC LIMIT 300",
            libsql::params![activo],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut clientes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        clientes.push(Cliente {
            id: row.get(0).unwrap_or_default(),
            tipo_documento: row.get(1).unwrap_or_default(),
            numero_documento: row.get(2).ok(),
            nombre_razon_social: row.get(3).unwrap_or_default(),
            telefono: row.get(4).ok(),
            email: row.get(5).ok(),
            direccion: row.get(6).ok(),
            activo: row.get::<i64>(7).unwrap_or(1) == 1,
        });
    }

    Ok(clientes)
}

pub async fn crear_cliente(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoCliente>,
) -> Result<Json<Cliente>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Documento repetido: no se crean clientes duplicados.
    // - Si ya existe ACTIVO: mensaje claro.
    // - Si existe DESACTIVADO: se reactiva con los datos nuevos (así el
    //   cajero puede seguir vendiendo desde el POS sin pedir ayuda) y
    //   conserva su historial de compras.
    let documento = payload.numero_documento.clone().unwrap_or_default().trim().to_string();
    if !documento.is_empty() {
        let mut r = conn
            .query(
                "SELECT id, nombre_razon_social, activo FROM clientes
                 WHERE tipo_documento = ?1 AND numero_documento = ?2 ORDER BY activo DESC, id LIMIT 1",
                libsql::params![payload.tipo_documento.clone(), documento.clone()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if let Some(fila) = r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            let id_existente: i64 = fila.get(0).unwrap_or_default();
            let nombre_existente: String = fila.get(1).unwrap_or_default();
            let activo: i64 = fila.get(2).unwrap_or(1);
            if activo == 1 {
                return Err((StatusCode::CONFLICT, format!(
                    "Ya existe un cliente con {} {}: \"{}\".",
                    payload.tipo_documento, documento, nombre_existente
                )));
            }
            conn.execute(
                "UPDATE clientes SET activo = 1, nombre_razon_social = ?1, telefono = ?2, email = ?3, direccion = ?4,
                        fecha_actualizacion = datetime('now','localtime')
                 WHERE id = ?5",
                libsql::params![
                    payload.nombre_razon_social.clone(),
                    payload.telefono.clone(),
                    payload.email.clone(),
                    payload.direccion.clone(),
                    id_existente
                ],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            return Ok(Json(Cliente {
                id: id_existente,
                tipo_documento: payload.tipo_documento,
                numero_documento: Some(documento),
                nombre_razon_social: payload.nombre_razon_social,
                telefono: payload.telefono,
                email: payload.email,
                direccion: payload.direccion,
                activo: true,
            }));
        }
    }

    conn.execute(
        "INSERT INTO clientes (tipo_documento, numero_documento, nombre_razon_social, telefono, email, direccion)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        libsql::params![
            payload.tipo_documento.clone(),
            payload.numero_documento.clone(),
            payload.nombre_razon_social.clone(),
            payload.telefono.clone(),
            payload.email.clone(),
            payload.direccion.clone()
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear cliente: {}", e)))?;

    let id = conn.last_insert_rowid();

    Ok(Json(Cliente {
        id,
        tipo_documento: payload.tipo_documento,
        numero_documento: payload.numero_documento,
        nombre_razon_social: payload.nombre_razon_social,
        telefono: payload.telefono,
        email: payload.email,
        direccion: payload.direccion,
        activo: true,
    }))
}

pub async fn actualizar_cliente(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(payload): Json<ActualizarCliente>,
) -> Result<Json<ClienteResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE clientes SET tipo_documento=?1, numero_documento=?2, nombre_razon_social=?3,
            telefono=?4, email=?5, direccion=?6, fecha_actualizacion = datetime('now','localtime')
         WHERE id = ?7",
        libsql::params![
            payload.tipo_documento.clone(), payload.numero_documento.clone(), payload.nombre_razon_social.clone(),
            payload.telefono.clone(), payload.email.clone(), payload.direccion.clone(), id
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;

    Ok(Json(ClienteResponse { success: true, message: "Cliente actualizado".into() }))
}

pub async fn desactivar_cliente(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ClienteResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE clientes SET activo = 0, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    Ok(Json(ClienteResponse { success: true, message: "Cliente desactivado".into() }))
}

/// Vuelve a activar un cliente: reaparece en las búsquedas del POS.
pub async fn reactivar_cliente(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ClienteResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let filas = conn.execute(
        "UPDATE clientes SET activo = 1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1 AND activo = 0",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al reactivar: {}", e)))?;

    if filas == 0 {
        return Err((StatusCode::NOT_FOUND, "El cliente no existe o ya está activo".into()));
    }

    Ok(Json(ClienteResponse { success: true, message: "Cliente reactivado".into() }))
}
