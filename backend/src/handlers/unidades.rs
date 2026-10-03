//! Unidades de venta por negocio.
//!
//! El catálogo completo vive en el código (no en un CHECK de SQLite): para
//! agregar una unidad basta con sumarla aquí y en el frontend
//! (utils/unidades.js) y redesplegar, sin tocar ninguna base.
//! Cada negocio elige en Configuración → Unidades cuáles usa; esas son
//! las que ve al crear un producto. Si nunca eligió, ve las 20 de siempre.

use axum::{extract::Extension, Json, http::StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::models::unidad::*;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

/// Las de siempre (tiendas, bodegas, ferreterías...).
pub const UNIDADES_CLASICAS: &[&str] = &[
    "UNIDAD", "KG", "GRAMO", "LITRO", "ML", "PAQUETE", "CAJA", "DOCENA",
    "PAR", "METRO", "GALON", "BOLSA", "ONZA", "LIBRA", "ROLLO", "YARDA",
    "MILLAR", "JUEGO", "SACO", "TONELADA",
];

/// Las de cafetería / restaurante.
pub const UNIDADES_RESTAURANTE: &[&str] = &[
    "PLATO", "PORCION", "ENTERO", "MEDIO", "CUARTO", "VASO", "TAZA", "JARRA", "COPA", "BOTELLA",
];

/// La unidad base: nunca se puede apagar.
const UNIDAD_BASE: &str = "UNIDAD";

fn catalogo() -> impl Iterator<Item = &'static str> {
    UNIDADES_CLASICAS.iter().chain(UNIDADES_RESTAURANTE.iter()).copied()
}

/// true si la unidad existe en el catálogo (esté activa o no en el negocio).
pub fn es_valida(unidad: &str) -> bool {
    catalogo().any(|u| u == unidad)
}

/// Con lo que arranca un negocio nuevo de cafetería / restaurante.
pub fn recomendadas_restaurante() -> String {
    std::iter::once(UNIDAD_BASE).chain(UNIDADES_RESTAURANTE.iter().copied()).collect::<Vec<_>>().join(",")
}

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Lo que el negocio guardó (None si nunca configuró, o si la base aún no
/// tiene la migración 0010).
async fn guardadas(conn: &libsql::Connection) -> Option<Vec<String>> {
    let mut filas = conn.query("SELECT unidades_activas FROM configuracion_tienda LIMIT 1", ()).await.ok()?;
    let texto: String = filas.next().await.ok()??.get(0).ok()?;
    let lista: Vec<String> = texto.split(',').map(|u| u.trim().to_string()).filter(|u| es_valida(u)).collect();
    if lista.is_empty() { None } else { Some(lista) }
}

/// Productos activos por unidad. Los platos de la carta del día no cuentan:
/// no eligen unidad de la lista (siempre son PLATO).
async fn en_uso(conn: &libsql::Connection) -> Result<HashMap<String, i64>, (StatusCode, String)> {
    const BASE: &str = "SELECT unidad_medida, COUNT(*) FROM productos WHERE activo = 1";
    let mut filas = match conn.query(&format!("{} AND carta_fecha IS NULL GROUP BY unidad_medida", BASE), ()).await {
        Ok(filas) => filas,
        Err(_) => conn.query(&format!("{} GROUP BY unidad_medida", BASE), ()).await.map_err(e500)?,
    };
    let mut mapa = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        if let (Ok(unidad), Ok(cantidad)) = (f.get::<String>(0), f.get::<i64>(1)) {
            mapa.insert(unidad, cantidad);
        }
    }
    Ok(mapa)
}

fn armar(elegidas: &[String], uso: &HashMap<String, i64>) -> UnidadesNegocio {
    let activas = catalogo()
        .filter(|u| *u == UNIDAD_BASE || elegidas.iter().any(|e| e == u) || uso.contains_key(*u))
        .map(String::from)
        .collect();
    let en_uso = catalogo()
        .filter_map(|u| uso.get(u).map(|n| UnidadEnUso { unidad: u.to_string(), productos: *n }))
        .collect();
    UnidadesNegocio { activas, en_uso }
}

async fn estado(conn: &libsql::Connection) -> Result<UnidadesNegocio, (StatusCode, String)> {
    let elegidas = guardadas(conn)
        .await
        .unwrap_or_else(|| UNIDADES_CLASICAS.iter().map(|u| u.to_string()).collect());
    Ok(armar(&elegidas, &en_uso(conn).await?))
}

/// GET /unidades — las unidades que el negocio usa (para el formulario de
/// productos) y cuántos productos usan cada una.
pub async fn listar_unidades(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<UnidadesNegocio> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(estado(&conn).await?))
}

/// PUT /configuracion/unidades — el administrador elige sus unidades.
/// UNIDAD y las que ya usan productos quedan activas aunque no vengan.
pub async fn guardar_unidades(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarUnidades>,
) -> Resultado<UnidadesNegocio> {
    exigir_admin(&claims)?;
    if let Some(mala) = payload.activas.iter().find(|u| !es_valida(u)) {
        return Err((StatusCode::BAD_REQUEST, format!("Unidad no válida: {}", mala)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    let resultado = armar(&payload.activas, &en_uso(&conn).await?);
    conn.execute(
        "UPDATE configuracion_tienda SET unidades_activas = ?1, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![resultado.activas.join(",")],
    )
    .await
    .map_err(|_| (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string()))?;
    Ok(Json(resultado))
}
