use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::producto::*;

pub async fn listar_productos(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Producto>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|e| {
        eprintln!("❌ Error conectando en listar_productos: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut rows = conn
        .query(
            "SELECT p.id, p.codigo, p.nombre, p.descripcion, p.precio, p.stock, p.stock_minimo,
                    p.unidad_medida, p.categoria_id, c.nombre, p.descuento_porcentaje,
                    p.lleva_vencimiento, p.imagen_url, p.activo, p.precio_compra
             FROM productos p
             LEFT JOIN categorias c ON p.categoria_id = c.id
             WHERE p.activo = 1
             ORDER BY p.nombre",
            (),
        )
        .await
        .map_err(|e| {
            eprintln!("❌ Error en el SELECT de listar_productos: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut productos = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        productos.push(Producto {
            id: row.get(0).unwrap_or_default(),
            codigo: row.get(1).unwrap_or_default(),
            nombre: row.get(2).unwrap_or_default(),
            descripcion: row.get(3).ok(),
            precio: row.get(4).unwrap_or_default(),
            stock: row.get(5).unwrap_or_default(),
            stock_minimo: row.get(6).unwrap_or_default(),
            unidad_medida: row.get(7).unwrap_or_default(),
            categoria_id: row.get(8).unwrap_or_default(),
            categoria_nombre: row.get(9).ok(),
            descuento_porcentaje: row.get(10).unwrap_or(0.0),
            lleva_vencimiento: row.get::<i64>(11).unwrap_or(0) == 1,
            imagen_url: row.get(12).ok(),
            activo: row.get::<i64>(13).unwrap_or(1) == 1,
            precio_compra: row.get(14).unwrap_or(0.0),
        });
    }

    Ok(Json(productos))
}

pub async fn productos_stock_bajo(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Producto>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT p.id, p.codigo, p.nombre, p.stock, p.stock_minimo, p.unidad_medida, c.nombre
             FROM productos p
             LEFT JOIN categorias c ON p.categoria_id = c.id
             WHERE p.activo = 1 AND p.stock <= p.stock_minimo
             ORDER BY (p.stock - p.stock_minimo), p.nombre",
            (),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut productos = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        productos.push(Producto {
            id: row.get(0).unwrap_or_default(),
            codigo: row.get(1).unwrap_or_default(),
            nombre: row.get(2).unwrap_or_default(),
            descripcion: None,
            precio: 0.0,
            stock: row.get(3).unwrap_or_default(),
            stock_minimo: row.get(4).unwrap_or_default(),
            unidad_medida: row.get(5).unwrap_or_default(),
            categoria_id: 0,
            categoria_nombre: row.get(6).ok(),
            descuento_porcentaje: 0.0,
            lleva_vencimiento: false,
            imagen_url: None,
            activo: true,
            precio_compra: 0.0,
        });
    }

    Ok(Json(productos))
}

pub async fn obtener_categorias(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Categoria>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query("SELECT id, nombre FROM categorias WHERE activo = 1 ORDER BY nombre", ())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut categorias = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        categorias.push(Categoria {
            id: row.get(0).unwrap_or_default(),
            nombre: row.get(1).unwrap_or_default(),
        });
    }

    Ok(Json(categorias))
}

// Si el producto es perecible (lleva_vencimiento), el stock inicial se
// fuerza a 0 — el trigger de lotes lo calcula solo apenas se cree el
// primer lote. Mismo patrón defensivo que usaba Lubricentro con
// productos que tienen variantes/tallas.
pub async fn agregar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoProducto>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let lleva_vencimiento = payload.lleva_vencimiento.unwrap_or(false);
    let stock_inicial = if lleva_vencimiento { 0.0 } else { payload.stock };

    conn.execute(
        "INSERT INTO productos (codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida,
            categoria_id, descuento_porcentaje, lleva_vencimiento, imagen_url, precio_compra)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        libsql::params![
            payload.codigo.clone(), payload.nombre.clone(), payload.descripcion.clone(),
            payload.precio, stock_inicial, payload.stock_minimo, payload.unidad_medida.clone(),
            payload.categoria_id, payload.descuento_porcentaje.unwrap_or(0.0),
            if lleva_vencimiento { 1 } else { 0 },
            payload.imagen_url.clone(), payload.precio_compra.unwrap_or(0.0)
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al agregar producto (¿código duplicado?): {}", e)))?;

    let producto_id = conn.last_insert_rowid();

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto agregado exitosamente".into(),
        producto_id: Some(producto_id),
    }))
}

// Si el producto es perecible, el stock del formulario se IGNORA — se
// mantiene el que ya está calculado por los lotes, para no desincronizar
// el stock real con lo que dice FEFO. Mismo patrón que usaba Lubricentro
// con productos de variantes en actualizar_producto.
pub async fn actualizar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(payload): Json<ActualizarProducto>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let lleva_vencimiento = payload.lleva_vencimiento.unwrap_or(false);

    let stock_a_guardar = if lleva_vencimiento {
        let mut r = conn.query("SELECT stock FROM productos WHERE id = ?1", libsql::params![id])
            .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match r.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get(0).unwrap_or(0.0),
            None => 0.0,
        }
    } else {
        payload.stock
    };

    conn.execute(
        "UPDATE productos SET codigo=?1, nombre=?2, descripcion=?3, precio=?4, stock=?5,
            stock_minimo=?6, unidad_medida=?7, categoria_id=?8, descuento_porcentaje=?9,
            lleva_vencimiento=?10, imagen_url=?11, precio_compra=?12,
            fecha_actualizacion = datetime('now','localtime')
         WHERE id = ?13",
        libsql::params![
            payload.codigo.clone(), payload.nombre.clone(), payload.descripcion.clone(),
            payload.precio, stock_a_guardar, payload.stock_minimo, payload.unidad_medida.clone(),
            payload.categoria_id, payload.descuento_porcentaje.unwrap_or(0.0),
            if lleva_vencimiento { 1 } else { 0 },
            payload.imagen_url.clone(), payload.precio_compra.unwrap_or(0.0), id
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto actualizado exitosamente".into(),
        producto_id: Some(id),
    }))
}

// Misma protección que Lubricentro: solo se elimina si el producto nunca
// se usó en una venta o una compra. Si ya tiene historial real, en vez de
// borrar se debe desactivar (activo = 0) — eso lo maneja el frontend
// llamando a este mismo endpoint, que devuelve success:false si no se
// puede borrar, y el frontend decide ofrecer "desactivar" en su lugar.
pub async fn eliminar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut r1 = conn.query("SELECT COUNT(*) FROM detalles_venta WHERE producto_id = ?1", libsql::params![id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let usado_ventas: i64 = match r1.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(0), None => 0,
    };

    let mut r2 = conn.query("SELECT COUNT(*) FROM detalles_compra WHERE producto_id = ?1", libsql::params![id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let usado_compras: i64 = match r2.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => row.get(0).unwrap_or(0), None => 0,
    };

    if usado_ventas > 0 || usado_compras > 0 {
        return Ok(Json(ProductoResponse {
            success: false,
            message: "Este producto ya tiene historial de ventas o compras, no se puede eliminar. Puedes desactivarlo en su lugar.".into(),
            producto_id: Some(id),
        }));
    }

    conn.execute("DELETE FROM productos WHERE id = ?1", libsql::params![id])
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al eliminar: {}", e)))?;

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto eliminado".into(),
        producto_id: Some(id),
    }))
}

pub async fn desactivar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "UPDATE productos SET activo = 0, fecha_actualizacion = datetime('now','localtime') WHERE id = 1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto desactivado".into(),
        producto_id: Some(id),
    }))
}