//! "Carta de hoy" (módulo Cafetería / Restaurante).
//!
//! Cada mañana el mozo, la barra/cocina o el administrador escriben los
//! platos del día (nombre + precio, S/ 10 por defecto en la pantalla). Cada
//! plato se guarda como un producto normal con carta_fecha = hoy y sin
//! control de stock, así pedidos, comandas, cobro, caja y reportes
//! funcionan igual. Al día siguiente deja de listarse solo.
//! El cajero solo cobra: puede ver la carta pero no cambiarla.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use chrono::{Duration, Utc};
use std::sync::Arc;

use crate::handlers::mesas::{modo_restaurante, nombre_rol};
use crate::models::auth::Claims;
use crate::models::carta::*;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

const CATEGORIA_CARTA: &str = "Carta del día";
const PRECIO_MAXIMO: f64 = 9999.0;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn error(codigo: StatusCode, mensaje: impl Into<String>) -> (StatusCode, String) {
    (codigo, mensaje.into())
}

/// Fecha de hoy en Perú (UTC-5 todo el año, sin horario de verano). Se
/// calcula aquí y no con 'localtime' de SQLite porque el servidor y la
/// base trabajan en UTC: a las 8 p. m. en Lima ya sería "mañana".
pub fn hoy_lima() -> String {
    (Utc::now() - Duration::hours(5)).format("%Y-%m-%d").to_string()
}

async fn exigir_modulo(conn: &libsql::Connection) -> Result<(), (StatusCode, String)> {
    if modo_restaurante(conn).await {
        Ok(())
    } else {
        Err(error(
            StatusCode::FORBIDDEN,
            "La carta del día es del modo Cafetería / Restaurante. Actívalo en Configuración → Tipo de negocio.",
        ))
    }
}

/// Todos pueden armar la carta menos el cajero (solo cobra).
async fn exigir_puede_editar(conn: &libsql::Connection, claims: &Claims) -> Result<(), (StatusCode, String)> {
    if nombre_rol(conn, claims.rol_id).await.as_deref() == Some("CAJERO") {
        return Err(error(StatusCode::FORBIDDEN, "La carta del día la arma el mozo, la cocina o el administrador."));
    }
    Ok(())
}

fn nombre_valido(nombre: &str) -> Result<String, (StatusCode, String)> {
    let limpio: String = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpio.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Escribe el nombre del plato."));
    }
    Ok(limpio.chars().take(80).collect())
}

fn precio_valido(precio: f64) -> Result<f64, (StatusCode, String)> {
    if !(precio > 0.0 && precio <= PRECIO_MAXIMO) {
        return Err(error(StatusCode::BAD_REQUEST, "El precio debe ser mayor a 0."));
    }
    Ok((precio * 100.0).round() / 100.0)
}

async fn platos_de_hoy(conn: &libsql::Connection) -> Result<Vec<PlatoCarta>, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT id, nombre, CAST(precio AS REAL), COALESCE(agotado, 0)
             FROM productos WHERE carta_fecha = ?1 AND activo = 1 ORDER BY id",
            libsql::params![hoy_lima()],
        )
        .await
        .map_err(e500)?;
    let mut platos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        platos.push(PlatoCarta {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            precio: f.get(2).unwrap_or(0.0),
            agotado: f.get::<i64>(3).unwrap_or(0) == 1,
        });
    }
    Ok(platos)
}

/// GET /carta-dia — los platos de hoy (cualquiera del negocio puede verlos).
pub async fn listar_carta(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<PlatoCarta>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    Ok(Json(platos_de_hoy(&conn).await?))
}

/// POST /carta-dia — agrega un plato a la carta de hoy.
pub async fn agregar_plato(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevoPlato>,
) -> Resultado<Vec<PlatoCarta>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    exigir_puede_editar(&conn, &claims).await?;
    let nombre = nombre_valido(&payload.nombre)?;
    let precio = precio_valido(payload.precio)?;
    let hoy = hoy_lima();

    // La categoría "Carta del día" se crea recién con el primer plato (así
    // un negocio que no usa la carta no la ve en su lista de categorías).
    let mut filas = conn
        .query(
            "INSERT INTO categorias (nombre, descripcion) VALUES (?1, 'Platos de la carta de hoy')
             ON CONFLICT(nombre) DO UPDATE SET activo = 1
             RETURNING id",
            libsql::params![CATEGORIA_CARTA],
        )
        .await
        .map_err(e500)?;
    let categoria_id: i64 = filas
        .next()
        .await
        .map_err(e500)?
        .and_then(|f| f.get(0).ok())
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "No se pudo preparar la categoría de la carta."))?;
    // Se lee la respuesta hasta el final antes de seguir (ver ventas.rs).
    while filas.next().await.map_err(e500)?.is_some() {}
    drop(filas);

    // El código es interno (nadie lo escanea): fecha + un número al azar.
    // Si chocara con otro (casi imposible), se reintenta una vez.
    for intento in 0..2 {
        let codigo = format!("CARTA-{}-{:06}", hoy.replace('-', ""), rand::random::<u32>() % 1_000_000);
        let resultado = conn
            .execute(
                "INSERT INTO productos (codigo, nombre, precio, stock, stock_minimo, unidad_medida, categoria_id,
                                        controla_stock, carta_fecha, agotado)
                 VALUES (?1, ?2, ?3, 0, 0, 'PLATO', ?4, 0, ?5, 0)",
                libsql::params![codigo, nombre.clone(), precio, categoria_id, hoy.clone()],
            )
            .await;
        match resultado {
            Ok(_) => break,
            Err(e) if intento == 0 && e.to_string().contains("UNIQUE") => continue,
            Err(e) => return Err(e500(e)),
        }
    }
    Ok(Json(platos_de_hoy(&conn).await?))
}

/// PUT /carta-dia/:id — corrige nombre/precio o marca agotado/disponible.
/// Solo platos de la carta de HOY.
pub async fn actualizar_plato(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<ActualizarPlato>,
) -> Resultado<Vec<PlatoCarta>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    exigir_puede_editar(&conn, &claims).await?;
    let nombre = payload.nombre.as_deref().map(nombre_valido).transpose()?;
    let precio = payload.precio.map(precio_valido).transpose()?;
    let agotado = payload.agotado.map(|a| if a { 1_i64 } else { 0 });

    let mut filas = conn
        .query(
            "UPDATE productos SET
                nombre = COALESCE(?1, nombre),
                precio = COALESCE(?2, precio),
                agotado = COALESCE(?3, agotado),
                fecha_actualizacion = datetime('now', 'localtime')
             WHERE id = ?4 AND carta_fecha = ?5 AND activo = 1
             RETURNING id",
            libsql::params![nombre, precio, agotado, id, hoy_lima()],
        )
        .await
        .map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err(error(StatusCode::NOT_FOUND, "Ese plato no está en la carta de hoy."));
    }
    drop(filas);
    Ok(Json(platos_de_hoy(&conn).await?))
}

/// POST /carta-dia/:id/quitar — saca un plato de la carta de hoy (p. ej.
/// mal escrito). Si ya se pidió o vendió, no se borra: se oculta, para que
/// pedidos, ventas y reportes sigan intactos.
pub async fn quitar_plato(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Resultado<Vec<PlatoCarta>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    exigir_puede_editar(&conn, &claims).await?;
    let hoy = hoy_lima();

    let borrado = conn
        .execute(
            "DELETE FROM productos
             WHERE id = ?1 AND carta_fecha = ?2
               AND NOT EXISTS (SELECT 1 FROM pedido_items WHERE producto_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM detalles_venta WHERE producto_id = ?1)",
            libsql::params![id, hoy.clone()],
        )
        .await
        .map_err(e500)?;
    if borrado == 0 {
        let ocultado = conn
            .execute(
                "UPDATE productos SET activo = 0, fecha_actualizacion = datetime('now', 'localtime')
                 WHERE id = ?1 AND carta_fecha = ?2",
                libsql::params![id, hoy],
            )
            .await
            .map_err(e500)?;
        if ocultado == 0 {
            return Err(error(StatusCode::NOT_FOUND, "Ese plato no está en la carta de hoy."));
        }
    }
    Ok(Json(platos_de_hoy(&conn).await?))
}
