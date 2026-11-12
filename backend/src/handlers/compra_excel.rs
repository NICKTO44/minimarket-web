//! Compras por Excel (módulos COMPRA_EXCEL y VENTA_TABLA): lo que el sistema
//! recuerda del Excel de cada proveedor, para que la próxima compra con su
//! archivo entre sola. Leer el Excel se hace en el navegador; aquí solo se
//! guarda y se devuelve lo aprendido.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::handlers::rubros::{negocio, MODULO_COMPRA_EXCEL, MODULO_VENTA_TABLA};
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    let modulos = negocio(conn).await.modulos;
    if modulos.iter().any(|m| m == MODULO_COMPRA_EXCEL || m == MODULO_VENTA_TABLA) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Este negocio no tiene encendido el módulo \"Compras por Excel\". Actívalo en Configuración → Datos del negocio.".into(),
        ))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Enlace {
    /// "C:<código del proveedor>" o "N:<nombre normalizado>".
    pub clave: String,
    pub producto_id: i64,
}

#[derive(Serialize)]
pub struct ExcelDeProveedor {
    /// Qué columna es cada dato (lo guardado tal cual), o null si aún no hay.
    pub columnas: Option<serde_json::Value>,
    pub enlaces: Vec<Enlace>,
}

/// GET /compras/excel/proveedor/:id
pub async fn obtener(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(proveedor_id): Path<i64>,
) -> Result<Json<ExcelDeProveedor>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut columnas = None;
    let mut f = conn
        .query("SELECT columnas FROM proveedor_excel WHERE proveedor_id = ?1", libsql::params![proveedor_id])
        .await
        .map_err(e500)?;
    if let Some(fila) = f.next().await.map_err(e500)? {
        columnas = fila.get::<String>(0).ok().and_then(|t| serde_json::from_str(&t).ok());
    }
    // Solo enlaces a productos que siguen existiendo.
    let mut f = conn
        .query(
            "SELECT pp.clave, pp.producto_id FROM proveedor_productos pp
             JOIN productos p ON p.id = pp.producto_id
             WHERE pp.proveedor_id = ?1",
            libsql::params![proveedor_id],
        )
        .await
        .map_err(e500)?;
    let mut enlaces = Vec::new();
    while let Some(fila) = f.next().await.map_err(e500)? {
        enlaces.push(Enlace { clave: fila.get(0).unwrap_or_default(), producto_id: fila.get(1).unwrap_or_default() });
    }
    Ok(Json(ExcelDeProveedor { columnas, enlaces }))
}

#[derive(Deserialize)]
pub struct Aprender {
    #[serde(default)]
    pub columnas: Option<serde_json::Value>,
    #[serde(default)]
    pub enlaces: Vec<Enlace>,
}

/// Clave válida: "C:" o "N:" y algo de texto, sin pasarse de largo.
fn clave_valida(clave: &str) -> bool {
    (clave.starts_with("C:") || clave.starts_with("N:")) && clave.len() > 2 && clave.len() <= 200
}

/// POST /compras/excel/proveedor/:id — guarda las columnas y los enlaces
/// (uno nuevo reemplaza al anterior de la misma clave).
pub async fn aprender(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(proveedor_id): Path<i64>,
    Json(datos): Json<Aprender>,
) -> Result<StatusCode, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    if datos.enlaces.len() > 3000 {
        return Err((StatusCode::BAD_REQUEST, "Demasiados productos en una sola compra.".into()));
    }
    if datos.enlaces.iter().any(|e| !clave_valida(&e.clave)) {
        return Err((StatusCode::BAD_REQUEST, "Hay un código de proveedor no válido.".into()));
    }
    let mut f = conn
        .query("SELECT 1 FROM proveedores WHERE id = ?1", libsql::params![proveedor_id])
        .await
        .map_err(e500)?;
    if f.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "Ese proveedor no existe.".into()));
    }
    drop(f);

    let tx = conn.transaction().await.map_err(e500)?;
    if let Some(columnas) = &datos.columnas {
        let texto = columnas.to_string();
        if texto.len() > 4000 {
            return Err((StatusCode::BAD_REQUEST, "Columnas no válidas.".into()));
        }
        tx.execute(
            "INSERT INTO proveedor_excel (proveedor_id, columnas) VALUES (?1, ?2)
             ON CONFLICT(proveedor_id) DO UPDATE SET columnas = excluded.columnas, fecha_actualizacion = datetime('now')",
            libsql::params![proveedor_id, texto],
        )
        .await
        .map_err(e500)?;
    }
    for e in &datos.enlaces {
        // Solo si el producto existe.
        tx.execute(
            "INSERT INTO proveedor_productos (proveedor_id, clave, producto_id)
             SELECT ?1, ?2, id FROM productos WHERE id = ?3
             ON CONFLICT(proveedor_id, clave) DO UPDATE SET producto_id = excluded.producto_id, fecha_actualizacion = datetime('now')",
            libsql::params![proveedor_id, e.clave.clone(), e.producto_id],
        )
        .await
        .map_err(e500)?;
    }
    tx.commit().await.map_err(e500)?;
    Ok(StatusCode::NO_CONTENT)
}
