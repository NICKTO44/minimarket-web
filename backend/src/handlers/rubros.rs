//! Rubro del negocio y módulos.
//!
//! El sistema tiene un núcleo universal (ventas, caja, productos, clientes,
//! reportes...) y módulos que se encienden por negocio. El RUBRO es una
//! plantilla: al elegirlo se encienden los módulos que le tocan y se cargan
//! sus unidades sugeridas; después el administrador puede encender o apagar
//! módulos sueltos. Los nombres de pantalla por rubro ("Carta" en vez de
//! "Productos") viven en el frontend (utils/rubros.js), que debe coincidir
//! con este catálogo.

use axum::{extract::Extension, Json, http::StatusCode};
use std::sync::Arc;

use crate::handlers::mesas::{aplicar_modo_negocio, modo_restaurante};
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::models::rubro::*;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

/// Atención en mesas: Mesas, Preparación, Carta de hoy y opciones de producto.
pub const MODULO_MESAS: &str = "MESAS";
/// Vender cosas que no descuentan stock (un servicio, algo preparado al momento).
pub const MODULO_SERVICIOS: &str = "SERVICIOS";
/// Venta por medidas: cantidad con decimales en el punto de venta y
/// calculadora de pie tablar (maderera).
pub const MODULO_MEDIDAS: &str = "MEDIDAS";
/// Detracción (SPOT) en facturas que superan el monto mínimo. Sus datos
/// (porcentaje, código, cuenta) están en handlers/detraccion.rs.
pub const MODULO_DETRACCION: &str = "DETRACCION";
/// Cotizaciones / proformas que luego se cargan en el punto de venta.
pub const MODULO_COTIZACIONES: &str = "COTIZACIONES";
/// Ventas al crédito con abonos (cuentas por cobrar).
pub const MODULO_CREDITO: &str = "CREDITO";
/// Guía de remisión remitente electrónica.
pub const MODULO_GUIAS: &str = "GUIAS";
/// Tallas y colores: un modelo con una fila por talla/color, cada una con
/// su código de barras, precio y stock (handlers/variantes.rs).
pub const MODULO_VARIANTES: &str = "VARIANTES";
/// Cambio de prenda: el cliente devuelve algo y se lleva otra cosa en la
/// misma operación (handlers/cambios.rs).
pub const MODULO_CAMBIOS: &str = "CAMBIOS";

const MODULOS: &[&str] = &[
    MODULO_MESAS,
    MODULO_SERVICIOS,
    MODULO_MEDIDAS,
    MODULO_DETRACCION,
    MODULO_COTIZACIONES,
    MODULO_CREDITO,
    MODULO_GUIAS,
    MODULO_VARIANTES,
    MODULO_CAMBIOS,
];

/// Rubro de un negocio que no eligió ninguno y no atiende en mesas.
pub const RUBRO_GENERAL: &str = "GENERAL";
const RUBRO_RESTAURANTE: &str = "RESTAURANTE";

/// (rubro, módulos que enciende, unidades sugeridas; vacío = las 20 de siempre)
const RUBROS: &[(&str, &[&str], &[&str])] = &[
    ("BODEGA", &[], &[]),
    (
        RUBRO_RESTAURANTE,
        &[MODULO_MESAS, MODULO_SERVICIOS],
        &["UNIDAD", "PLATO", "PORCION", "ENTERO", "MEDIO", "CUARTO", "VASO", "TAZA", "JARRA", "COPA", "BOTELLA"],
    ),
    (
        "FERRETERIA",
        &[MODULO_SERVICIOS],
        &["UNIDAD", "KG", "LITRO", "PAQUETE", "CAJA", "DOCENA", "PAR", "METRO", "GALON", "BOLSA", "ROLLO", "MILLAR", "JUEGO", "PIEZA", "PLANCHA"],
    ),
    (
        "MADERERA",
        &[MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_DETRACCION, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_GUIAS],
        &["UNIDAD", "PIE_TABLAR", "PIEZA", "PLANCHA", "METRO", "M2", "M3", "KG", "GALON", "CAJA", "MILLAR"],
    ),
    (
        "ROPA",
        &[MODULO_CREDITO, MODULO_VARIANTES, MODULO_CAMBIOS],
        &["UNIDAD", "PAR", "DOCENA", "PAQUETE", "CAJA", "JUEGO"],
    ),
    (RUBRO_GENERAL, &[], &[]),
];

fn plantilla(rubro: &str) -> Option<&'static (&'static str, &'static [&'static str], &'static [&'static str])> {
    RUBROS.iter().find(|r| r.0 == rubro)
}

pub fn rubro_valido(rubro: &str) -> bool {
    plantilla(rubro).is_some()
}

/// Módulos con los que arranca un negocio nuevo de ese rubro.
pub fn modulos_del_rubro(rubro: &str) -> Vec<String> {
    plantilla(rubro).map(|r| r.1.iter().map(|m| m.to_string()).collect()).unwrap_or_default()
}

/// Unidades con las que arranca ese rubro ("A,B,C"), o None si usa las de siempre.
pub fn unidades_del_rubro(rubro: &str) -> Option<String> {
    plantilla(rubro).filter(|r| !r.2.is_empty()).map(|r| r.2.join(","))
}

/// Los módulos extra (todos menos MESAS) como se guardan en la base.
pub fn modulos_extra_csv(modulos: &[String]) -> String {
    MODULOS
        .iter()
        .filter(|m| **m != MODULO_MESAS && modulos.iter().any(|x| x == *m))
        .copied()
        .collect::<Vec<_>>()
        .join(",")
}

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Rubro y módulos del negocio. Si la base aún no tiene la migración 0011,
/// o el negocio nunca eligió, se deduce de si atiende en mesas (así todo se
/// ve igual que antes).
pub async fn negocio(conn: &libsql::Connection) -> Negocio {
    let mesas = modo_restaurante(conn).await;
    let (mut rubro, mut extra): (Option<String>, Option<Vec<String>>) = (None, None);
    if let Ok(mut filas) = conn.query("SELECT rubro, modulos FROM configuracion_tienda LIMIT 1", ()).await {
        if let Ok(Some(f)) = filas.next().await {
            rubro = f.get::<String>(0).ok().filter(|r| rubro_valido(r));
            extra = f.get::<String>(1).ok().map(|texto| {
                texto.split(',').map(|m| m.trim().to_string()).filter(|m| MODULOS.contains(&m.as_str())).collect()
            });
        }
    }
    let rubro = rubro.unwrap_or_else(|| if mesas { RUBRO_RESTAURANTE } else { RUBRO_GENERAL }.to_string());
    // Nunca configurado: el restaurante ya vendía productos "preparados" sin stock.
    let extra = extra.unwrap_or_else(|| if mesas { vec![MODULO_SERVICIOS.to_string()] } else { Vec::new() });
    let modulos = MODULOS
        .iter()
        .filter(|m| if **m == MODULO_MESAS { mesas } else { extra.iter().any(|x| x == *m) })
        .map(|m| m.to_string())
        .collect();
    Negocio { rubro, modulos, modo_negocio: if mesas { "RESTAURANTE" } else { "TIENDA" }.to_string() }
}

/// Corta con 403 si el negocio no tiene encendido ese módulo.
pub async fn exigir_modulo(conn: &libsql::Connection, modulo: &str, nombre: &str) -> Result<(), (StatusCode, String)> {
    if negocio(conn).await.modulos.iter().any(|m| m == modulo) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", nombre),
        ))
    }
}

/// GET /negocio — rubro y módulos (lo lee cualquier usuario del negocio).
pub async fn obtener_negocio(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Negocio> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(negocio(&conn).await))
}

/// PUT /configuracion/negocio — el administrador cambia el rubro y/o los
/// módulos. Encender o apagar MESAS pasa por las mismas reglas de siempre
/// (mesas de ejemplo al activarlo; no se apaga con pedidos abiertos).
pub async fn guardar_negocio(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarNegocio>,
) -> Resultado<Negocio> {
    exigir_admin(&claims)?;
    let rubro = payload.rubro.trim().to_uppercase();
    if !rubro_valido(&rubro) {
        return Err((StatusCode::BAD_REQUEST, "Rubro no válido.".to_string()));
    }
    if let Some(malo) = payload.modulos.iter().find(|m| !MODULOS.contains(&m.as_str())) {
        return Err((StatusCode::BAD_REQUEST, format!("Módulo no válido: {}", malo)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    let actualizando = || (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string());
    // Si la base todavía no tiene la migración 0011, no se toca nada.
    conn.query("SELECT rubro FROM configuracion_tienda LIMIT 1", ()).await.map_err(|_| actualizando())?;

    let quiere_mesas = payload.modulos.iter().any(|m| m == MODULO_MESAS);
    if quiere_mesas != modo_restaurante(&conn).await {
        aplicar_modo_negocio(&conn, if quiere_mesas { "RESTAURANTE" } else { "TIENDA" }).await?;
    }

    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![rubro, modulos_extra_csv(&payload.modulos)],
    )
    .await
    .map_err(|_| actualizando())?;

    Ok(Json(negocio(&conn).await))
}
