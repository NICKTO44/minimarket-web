//! Módulo PAQUETES (migración 0041): varios productos a un solo precio.
//! Pensado para el lubricentro ("Cambio de aceite completo"); sirve a
//! cualquier negocio que lo encienda.
//!
//! El paquete no es un producto ni tiene stock: al venderlo, el punto de
//! venta pone cada producto en su línea, con su precio de siempre y un
//! descuento por línea hasta llegar al precio del paquete. Así el
//! comprobante detalla lo que se llevó y el stock se descuenta de cada
//! producto como en cualquier venta; el servidor de ventas no cambia.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET    /paquetes        -> los activos, con sus productos y el precio de cada uno hoy
//!   POST   /paquetes        -> nuevo (solo administrador)
//!   PUT    /paquetes/:id    -> cambia nombre, precio o productos (solo administrador)
//!   DELETE /paquetes/:id    -> lo da de baja (solo administrador)

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_PAQUETES};
use crate::logica::tiempo;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Paquetes de productos";
const MAX_ITEMS: usize = 30;

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") {
        (StatusCode::SERVICE_UNAVAILABLE, "Los paquetes aún no están listos en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn redondear_2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_PAQUETES, NOMBRE_MODULO).await
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemPaquete {
    pub producto_id: i64,
    pub cantidad: f64,
    /// Solo al leer.
    #[serde(default, skip_deserializing)]
    pub nombre: String,
    #[serde(default, skip_deserializing)]
    pub precio: f64,
    #[serde(default, skip_deserializing)]
    pub unidad_medida: String,
    #[serde(default, skip_deserializing)]
    pub controla_stock: bool,
    #[serde(default, skip_deserializing)]
    pub activo: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Paquete {
    #[serde(default, skip_deserializing)]
    pub id: i64,
    pub nombre: String,
    pub precio: f64,
    pub items: Vec<ItemPaquete>,
    /// Solo al leer: lo que suman sus productos a su precio de hoy.
    #[serde(default, skip_deserializing)]
    pub precio_normal: f64,
}

/// GET /paquetes — los activos.
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Paquete>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query("SELECT id, nombre, CAST(precio AS REAL) FROM paquetes WHERE activo = 1 ORDER BY nombre", ())
        .await
        .map_err(sin_migracion)?;
    let mut paquetes: Vec<Paquete> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        paquetes.push(Paquete {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            precio: f.get(2).unwrap_or(0.0),
            items: Vec::new(),
            precio_normal: 0.0,
        });
    }
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT i.paquete_id, i.producto_id, CAST(i.cantidad AS REAL), COALESCE(p.nombre, ''), CAST(COALESCE(p.precio, 0) AS REAL),
                    COALESCE(p.unidad_medida, 'UNIDAD'), COALESCE(p.controla_stock, 1), COALESCE(p.activo, 0)
             FROM paquete_items i JOIN paquetes q ON q.id = i.paquete_id AND q.activo = 1
             LEFT JOIN productos p ON p.id = i.producto_id
             ORDER BY i.paquete_id, i.id",
            (),
        )
        .await
        .map_err(e500)?;
    let posicion: HashMap<i64, usize> = paquetes.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let Some(i) = posicion.get(&f.get::<i64>(0).unwrap_or_default()) else { continue };
        let item = ItemPaquete {
            producto_id: f.get(1).unwrap_or_default(),
            cantidad: f.get(2).unwrap_or(0.0),
            nombre: f.get(3).unwrap_or_default(),
            precio: f.get(4).unwrap_or(0.0),
            unidad_medida: f.get(5).unwrap_or_default(),
            controla_stock: f.get::<i64>(6).unwrap_or(1) == 1,
            activo: f.get::<i64>(7).unwrap_or(0) == 1,
        };
        paquetes[*i].precio_normal = redondear_2(paquetes[*i].precio_normal + redondear_2(item.precio * item.cantidad));
        paquetes[*i].items.push(item);
    }
    Ok(Json(paquetes))
}

/// Revisa el paquete: nombre, precio, productos activos sin repetir y que
/// el precio no pase de lo que suman sus productos (el paquete es un ahorro).
async fn revisar(conn: &libsql::Connection, p: &Paquete) -> Result<(String, f64, Vec<(i64, f64)>), Fallo> {
    let nombre: String = p.nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if nombre.chars().count() < 3 || nombre.chars().count() > 80 {
        return Err(malo("El nombre del paquete va de 3 a 80 letras."));
    }
    if !p.precio.is_finite() || p.precio <= 0.0 || p.precio >= 10_000_000.0 {
        return Err(malo("El precio del paquete debe ser mayor a 0."));
    }
    if p.items.is_empty() {
        return Err(malo("Agrega al menos un producto al paquete."));
    }
    if p.items.len() > MAX_ITEMS {
        return Err(malo(format!("Máximo {} productos por paquete.", MAX_ITEMS)));
    }
    let mut items: Vec<(i64, f64)> = Vec::new();
    for i in &p.items {
        if !(i.cantidad > 0.0) || !i.cantidad.is_finite() || i.cantidad > 100_000.0 {
            return Err(malo("Cada producto del paquete necesita una cantidad mayor a 0."));
        }
        match items.iter_mut().find(|(id, _)| *id == i.producto_id) {
            Some(previo) => previo.1 += i.cantidad,
            None => items.push((i.producto_id, i.cantidad)),
        }
    }
    let ids = items.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(&format!("SELECT id, CAST(precio AS REAL) FROM productos WHERE activo = 1 AND id IN ({})", ids), ())
        .await
        .map_err(e500)?;
    let mut precios: HashMap<i64, f64> = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        precios.insert(f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0));
    }
    let mut suma = 0.0;
    for (id, cantidad) in &items {
        let precio = precios.get(id).ok_or_else(|| malo("Uno de los productos no existe o está desactivado."))?;
        suma += redondear_2(precio * cantidad);
    }
    let precio = redondear_2(p.precio);
    if precio > redondear_2(suma) + 0.001 {
        return Err(malo(format!(
            "El precio del paquete (S/ {:.2}) no puede ser mayor que lo que suman sus productos (S/ {:.2}).",
            precio, suma
        )));
    }
    Ok((nombre, precio, items))
}

async fn guardar_items(conn: &libsql::Connection, paquete_id: i64, items: &[(i64, f64)]) -> Result<(), Fallo> {
    conn.execute("DELETE FROM paquete_items WHERE paquete_id = ?1", libsql::params![paquete_id]).await.map_err(e500)?;
    let mut marcas = Vec::with_capacity(items.len());
    let mut valores: Vec<libsql::Value> = Vec::with_capacity(items.len() * 3);
    for (n, (id, cantidad)) in items.iter().enumerate() {
        let b = n * 3;
        marcas.push(format!("(?{}, ?{}, ?{})", b + 1, b + 2, b + 3));
        valores.extend([libsql::Value::Integer(paquete_id), libsql::Value::Integer(*id), libsql::Value::Real(*cantidad)]);
    }
    conn.execute(
        &format!("INSERT INTO paquete_items (paquete_id, producto_id, cantidad) VALUES {}", marcas.join(", ")),
        libsql::params_from_iter(valores),
    )
    .await
    .map_err(e500)?;
    Ok(())
}

/// POST /paquetes — nuevo (solo el administrador).
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(p): Json<Paquete>,
) -> Result<StatusCode, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (nombre, precio, items) = revisar(&conn, &p).await?;
    let tx = conn.transaction().await.map_err(e500)?;
    let ahora = tiempo::ahora_lima();
    let r: Result<(), Fallo> = async {
        tx.execute(
            "INSERT INTO paquetes (nombre, precio, activo, fecha_creacion, fecha_actualizacion) VALUES (?1, ?2, 1, ?3, ?3)",
            libsql::params![nombre, precio, ahora],
        )
        .await
        .map_err(sin_migracion)?;
        let id = tx.last_insert_rowid();
        guardar_items(&tx, id, &items).await
    }
    .await;
    match r {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    }
    Ok(StatusCode::CREATED)
}

/// PUT /paquetes/:id — cambia nombre, precio y productos (solo el administrador).
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(p): Json<Paquete>,
) -> Result<StatusCode, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (nombre, precio, items) = revisar(&conn, &p).await?;
    let tx = conn.transaction().await.map_err(e500)?;
    let r: Result<(), Fallo> = async {
        let n = tx
            .execute(
                "UPDATE paquetes SET nombre = ?1, precio = ?2, fecha_actualizacion = ?3 WHERE id = ?4 AND activo = 1",
                libsql::params![nombre, precio, tiempo::ahora_lima(), id],
            )
            .await
            .map_err(sin_migracion)?;
        if n == 0 {
            return Err((StatusCode::NOT_FOUND, "El paquete no existe.".into()));
        }
        guardar_items(&tx, id, &items).await
    }
    .await;
    match r {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /paquetes/:id — lo da de baja (las ventas hechas no cambian).
pub async fn quitar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<StatusCode, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    conn.execute("UPDATE paquetes SET activo = 0, fecha_actualizacion = ?1 WHERE id = ?2", libsql::params![tiempo::ahora_lima(), id])
        .await
        .map_err(sin_migracion)?;
    Ok(StatusCode::NO_CONTENT)
}
