use axum::{extract::{State, Query, Path}, Json, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;
use crate::models::cliente::*;

#[derive(Deserialize)]
pub struct BusquedaClientes {
    pub q: Option<String>,
}

pub async fn buscar_clientes(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BusquedaClientes>,
) -> Result<Json<Vec<Cliente>>, StatusCode> {
    let texto = params.q.unwrap_or_default();
    if texto.trim().len() < 2 {
        return Ok(Json(vec![]));
    }
    let filtro = format!("%{}%", texto);

    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Cliente>>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT id, tipo_documento, numero_documento, nombre_razon_social, telefono, email, direccion, activo
             FROM clientes WHERE activo = 1 ORDER BY nombre_razon_social ASC LIMIT 300",
            (),
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

pub async fn crear_cliente(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NuevoCliente>,
) -> Result<Json<Cliente>, StatusCode> {
    let conn = state.db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<ActualizarCliente>,
) -> Result<Json<ClienteResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ClienteResponse>, (StatusCode, String)> {
    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE clientes SET activo = 0, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    Ok(Json(ClienteResponse { success: true, message: "Cliente desactivado".into() }))
}