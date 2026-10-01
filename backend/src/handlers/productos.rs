use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::producto::*;

/// Unidades de medida soportadas -- vive aquí, no en un CHECK de SQLite,
/// para que agregar una nueva en el futuro sea solo un cambio de código
/// + redeploy, sin tocar la estructura de ninguna base de tenant nunca
/// más.
const UNIDADES_VALIDAS: &[&str] = &[
    "UNIDAD", "KG", "GRAMO", "LITRO", "ML", "PAQUETE", "CAJA", "DOCENA",
    "PAR", "METRO", "GALON", "BOLSA", "ONZA", "LIBRA", "ROLLO", "YARDA",
    "MILLAR", "JUEGO", "SACO", "TONELADA",
];

fn unidad_valida(unidad: &str) -> bool {
    UNIDADES_VALIDAS.contains(&unidad)
}

pub async fn listar_productos(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Producto>>, StatusCode> {
    consultar_productos(&tenant, 1).await.map(Json)
}

/// Productos desactivados (activo = 0): los que se "archivaron" porque ya
/// tenían ventas o compras. Se listan aparte en Inventario para poder
/// reactivarlos.
pub async fn listar_productos_desactivados(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Producto>>, StatusCode> {
    consultar_productos(&tenant, 0).await.map(Json)
}

async fn consultar_productos(tenant: &TenantDb, activo: i64) -> Result<Vec<Producto>, StatusCode> {
    let conn = tenant.0.connect().map_err(|e| {
        eprintln!("❌ Error conectando en listar_productos: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // controla_stock llegó con la migración 0007 y carta_fecha/agotado con
    // la 0009; si esta base todavía no las tiene, se lista igual que antes.
    const SQL_BASE: &str = "SELECT p.id, p.codigo, p.nombre, p.descripcion, p.precio, p.stock, p.stock_minimo,
                    p.unidad_medida, p.categoria_id, c.nombre, p.descuento_porcentaje,
                    p.lleva_vencimiento, p.imagen_url, p.activo, p.precio_compra";
    const SQL_DESDE: &str = " FROM productos p
             LEFT JOIN categorias c ON p.categoria_id = c.id
             WHERE p.activo = ?1";
    const SQL_ORDEN: &str = " ORDER BY p.nombre";
    // Platos de la carta del día: solo los de hoy y solo entre los activos
    // (los de días pasados no aparecen ni en "desactivados").
    let hoy = crate::handlers::carta::hoy_lima();
    let con_carta = format!(
        "{}, p.controla_stock, p.carta_fecha IS NOT NULL, COALESCE(p.agotado, 0){} AND (p.carta_fecha IS NULL OR (?1 = 1 AND p.carta_fecha = ?2)){}",
        SQL_BASE, SQL_DESDE, SQL_ORDEN
    );
    let (mut rows, columnas) = match conn.query(&con_carta, libsql::params![activo, hoy]).await {
        Ok(rows) => (rows, 2),
        Err(_) => match conn
            .query(&format!("{}, p.controla_stock{}{}", SQL_BASE, SQL_DESDE, SQL_ORDEN), libsql::params![activo])
            .await
        {
            Ok(rows) => (rows, 1),
            Err(_) => (
                conn.query(&format!("{}{}{}", SQL_BASE, SQL_DESDE, SQL_ORDEN), libsql::params![activo])
                    .await
                    .map_err(|e| {
                        eprintln!("❌ Error en el SELECT de listar_productos: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?,
                0,
            ),
        },
    };

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
            controla_stock: if columnas >= 1 { row.get::<i64>(15).unwrap_or(1) == 1 } else { true },
            carta_dia: columnas >= 2 && row.get::<i64>(16).unwrap_or(0) == 1,
            agotado: columnas >= 2 && row.get::<i64>(17).unwrap_or(0) == 1,
        });
    }

    Ok(productos)
}

pub async fn productos_stock_bajo(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<Producto>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Los preparados al momento (controla_stock = 0) no tienen stock que
    // reponer, así que no se listan como "stock bajo".
    const SQL_STOCK_BAJO: &str = "SELECT p.id, p.codigo, p.nombre, p.stock, p.stock_minimo, p.unidad_medida, c.nombre
             FROM productos p
             LEFT JOIN categorias c ON p.categoria_id = c.id
             WHERE p.activo = 1 AND p.stock <= p.stock_minimo";
    const SQL_ORDEN: &str = " ORDER BY (p.stock - p.stock_minimo), p.nombre";
    let mut rows = match conn
        .query(&format!("{} AND p.controla_stock = 1{}", SQL_STOCK_BAJO, SQL_ORDEN), ())
        .await
    {
        Ok(rows) => rows,
        Err(_) => conn
            .query(&format!("{}{}", SQL_STOCK_BAJO, SQL_ORDEN), ())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };

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
            controla_stock: true,
            carta_dia: false,
            agotado: false,
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

/// Nuevo -- antes no existía forma de crear categorías desde la
/// interfaz, todo negocio dependía de las 10 categorías de minimarket
/// que schema.sql insertaba por defecto. Ahora cada negocio arma las
/// suyas propias, sin importar su rubro.
pub async fn crear_categoria(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevaCategoria>,
) -> Result<Json<Categoria>, (StatusCode, String)> {
    if payload.nombre.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "El nombre de la categoría es obligatorio.".into()));
    }

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn.execute(
        "INSERT INTO categorias (nombre, descripcion) VALUES (?1, ?2)",
        libsql::params![payload.nombre.trim().to_string(), payload.descripcion.clone()],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al crear categoría (¿nombre duplicado?): {}", e)))?;

    let id = conn.last_insert_rowid();

    Ok(Json(Categoria { id, nombre: payload.nombre.trim().to_string() }))
}

// Si el producto es perecible (lleva_vencimiento), el stock inicial se
// fuerza a 0 — el trigger de lotes lo calcula solo apenas se cree el
// primer lote. Mismo patrón defensivo que usaba Lubricentro con
// productos que tienen variantes/tallas.
pub async fn agregar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoProducto>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    if !unidad_valida(&payload.unidad_medida) {
        return Err((StatusCode::BAD_REQUEST, format!("Unidad de medida no reconocida: {}", payload.unidad_medida)));
    }

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Código repetido: mensaje claro en vez del error técnico de la base.
    // Si pertenece a un producto desactivado, lo correcto es reactivarlo
    // (conserva su historial) en lugar de crear un duplicado.
    let mut r_codigo = conn
        .query("SELECT nombre, activo FROM productos WHERE codigo = ?1", libsql::params![payload.codigo.clone()])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(fila) = r_codigo.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        let nombre_existente: String = fila.get(0).unwrap_or_default();
        let activo: i64 = fila.get(1).unwrap_or(1);
        let mensaje = if activo == 0 {
            format!(
                "El código {} pertenece a \"{}\", que está desactivado. Reactívalo desde Inventario → Desactivados en lugar de crear uno nuevo.",
                payload.codigo, nombre_existente
            )
        } else {
            format!("Ya existe un producto con el código {}: \"{}\".", payload.codigo, nombre_existente)
        };
        return Err((StatusCode::CONFLICT, mensaje));
    }

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

    if let Some(controla) = payload.controla_stock {
        conn.execute(
            "UPDATE productos SET controla_stock = ?1 WHERE id = ?2",
            libsql::params![controla as i64, producto_id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

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
    if !unidad_valida(&payload.unidad_medida) {
        return Err((StatusCode::BAD_REQUEST, format!("Unidad de medida no reconocida: {}", payload.unidad_medida)));
    }

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let lleva_vencimiento = payload.lleva_vencimiento.unwrap_or(false);

    // Traemos el estado actual del producto una sola vez: sirve tanto para
    // no desincronizar el stock de perecibles (ya existía) como para NO
    // pisar imagen_url con NULL cuando el payload no trae una foto nueva
    // (bug reportado 2026-09-13: al editar solo el nombre, se borraba la
    // imagen porque el UPDATE siempre escribía payload.imagen_url tal cual,
    // y el frontend no reenvía la imagen existente si el usuario no eligió
    // una nueva).
    let mut r_actual = conn.query(
        "SELECT stock, imagen_url FROM productos WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (stock_actual, imagen_actual): (f64, Option<String>) =
        match r_actual.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => (row.get(0).unwrap_or(0.0), row.get(1).ok()),
            None => (0.0, None),
        };

    let stock_a_guardar = if lleva_vencimiento {
        stock_actual
    } else {
        payload.stock
    };

    // Solo reemplazamos la imagen si el payload trae una URL nueva y no
    // vacía; si no, conservamos la que ya estaba guardada.
    let imagen_a_guardar = match payload.imagen_url.clone() {
        Some(nueva) if !nueva.trim().is_empty() => Some(nueva),
        _ => imagen_actual,
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
            imagen_a_guardar, payload.precio_compra.unwrap_or(0.0), id
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;

    if let Some(controla) = payload.controla_stock {
        conn.execute(
            "UPDATE productos SET controla_stock = ?1 WHERE id = ?2",
            libsql::params![controla as i64, id],
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

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
        "UPDATE productos SET activo = 0, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al desactivar: {}", e)))?;

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto desactivado".into(),
        producto_id: Some(id),
    }))
}

/// Vuelve a activar un producto desactivado: reaparece en el POS y en el
/// inventario con su mismo código, stock e historial.
pub async fn reactivar_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let filas = conn.execute(
        "UPDATE productos SET activo = 1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1 AND activo = 0",
        libsql::params![id],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al reactivar: {}", e)))?;

    if filas == 0 {
        return Err((StatusCode::NOT_FOUND, "El producto no existe o ya está activo".into()));
    }

    Ok(Json(ProductoResponse {
        success: true,
        message: "Producto reactivado".into(),
        producto_id: Some(id),
    }))
}
