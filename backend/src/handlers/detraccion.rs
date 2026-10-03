//! Detracción del negocio (módulo DETRACCION): leer y guardar el
//! porcentaje, el código, el monto mínimo y la cuenta del Banco de la
//! Nación. El cálculo está en logica/detraccion.rs y se aplica al emitir
//! una factura (handlers/facturacion.rs).

use axum::{extract::Extension, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{negocio, MODULO_DETRACCION};
use crate::logica::detraccion::*;
use crate::logica::igv::round2;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Arma la configuración a partir de las columnas de configuracion_tienda
/// (`modulos` y las `detraccion_*`). Lo que falte o no sea válido toma el
/// valor por defecto; sin cuenta, la detracción no se aplica.
pub fn desde_columnas(
    modulos: Option<String>,
    porcentaje: Option<f64>,
    codigo: Option<String>,
    minimo: Option<f64>,
    cuenta: Option<String>,
) -> ConfigDetraccion {
    ConfigDetraccion {
        activa: modulos.map(|m| m.split(',').any(|x| x.trim() == MODULO_DETRACCION)).unwrap_or(false),
        porcentaje: porcentaje.filter(|p| porcentaje_valido(*p)).unwrap_or(PORCENTAJE_POR_DEFECTO),
        codigo: codigo.filter(|c| codigo_valido(c)).unwrap_or_else(|| CODIGO_POR_DEFECTO.to_string()),
        minimo: minimo.filter(|m| *m >= 0.0).unwrap_or(MINIMO_POR_DEFECTO),
        cuenta: cuenta.map(|c| c.trim().to_string()).unwrap_or_default(),
    }
}

/// Datos de detracción del negocio, en una sola consulta. Si la base aún no
/// tiene la migración 0013 devuelve los valores por defecto y sin cuenta (o
/// sea: no se aplica).
pub async fn configuracion(conn: &libsql::Connection) -> ConfigDetraccion {
    let consulta = conn
        .query(
            "SELECT modulos, CAST(detraccion_porcentaje AS REAL), detraccion_codigo, CAST(detraccion_minimo AS REAL), detraccion_cuenta
             FROM configuracion_tienda LIMIT 1",
            (),
        )
        .await;
    if let Ok(mut filas) = consulta {
        if let Ok(Some(f)) = filas.next().await {
            return desde_columnas(f.get::<String>(0).ok(), f.get::<f64>(1).ok(), f.get::<String>(2).ok(), f.get::<f64>(3).ok(), f.get::<String>(4).ok());
        }
    }
    // Base sin la 0013: el módulo puede estar encendido, pero no hay cuenta.
    let activa = negocio(conn).await.modulos.iter().any(|m| m == MODULO_DETRACCION);
    let mut cfg = desde_columnas(None, None, None, None, None);
    cfg.activa = activa;
    cfg
}

#[derive(Debug, Serialize)]
pub struct DetraccionRespuesta {
    /// El módulo está encendido en este negocio.
    pub activa: bool,
    /// Encendido y con cuenta: las facturas que superen el mínimo la llevan.
    pub lista: bool,
    pub porcentaje: f64,
    pub codigo: String,
    pub minimo: f64,
    pub cuenta: String,
}

fn respuesta(cfg: ConfigDetraccion) -> DetraccionRespuesta {
    DetraccionRespuesta {
        activa: cfg.activa,
        lista: cfg.lista(),
        porcentaje: cfg.porcentaje,
        codigo: cfg.codigo,
        minimo: cfg.minimo,
        cuenta: cfg.cuenta,
    }
}

/// GET /detraccion — la lee el punto de venta para avisar antes de cobrar.
pub async fn obtener_detraccion(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<DetraccionRespuesta> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(respuesta(configuracion(&conn).await)))
}

#[derive(Debug, Deserialize)]
pub struct GuardarDetraccion {
    pub porcentaje: f64,
    pub codigo: String,
    pub minimo: f64,
    /// Vacía = todavía no se aplica.
    #[serde(default)]
    pub cuenta: String,
}

/// PUT /configuracion/detraccion — solo el administrador.
pub async fn guardar_detraccion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarDetraccion>,
) -> Resultado<DetraccionRespuesta> {
    exigir_admin(&claims)?;
    let malo = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
    if !porcentaje_valido(payload.porcentaje) {
        return Err(malo("El porcentaje de detracción debe ser mayor a 0 y hasta 30."));
    }
    let codigo = payload.codigo.trim().to_string();
    if !codigo_valido(&codigo) {
        return Err(malo("El código de detracción son tres dígitos (por ejemplo 008 para madera)."));
    }
    if !(payload.minimo >= 0.0 && payload.minimo <= 1_000_000.0) {
        return Err(malo("El monto mínimo no es válido."));
    }
    let cuenta = payload.cuenta.trim().to_string();
    if !cuenta.is_empty() && !cuenta_valida(&cuenta) {
        return Err(malo("La cuenta de detracciones solo lleva números y guiones."));
    }

    let conn = tenant.0.connect().map_err(e500)?;
    conn.execute(
        "UPDATE configuracion_tienda SET detraccion_porcentaje = ?1, detraccion_codigo = ?2,
                detraccion_minimo = ?3, detraccion_cuenta = ?4, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![round2(payload.porcentaje), codigo, round2(payload.minimo), cuenta],
    )
    .await
    .map_err(|_| (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string()))?;

    Ok(Json(respuesta(configuracion(&conn).await)))
}
