//! Cotizaciones / proformas (módulo COTIZACIONES).
//!
//! Una cotización es lo que se le ofrece al cliente: productos, medidas y
//! precios, con días de validez. No toca stock ni caja. Después se carga en
//! el punto de venta (con los precios cotizados) y, al cobrarla, queda
//! VENDIDA y enlazada a su venta. No es un comprobante de pago.

use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_COTIZACIONES};
use crate::logica::igv::round2;
use crate::logica::tiempo::ahora_lima;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

const VALIDEZ_POR_DEFECTO: i64 = 7;
const MAXIMO_ITEMS: usize = 200;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(m: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, m.into())
}
/// La base aún no tiene la migración 0014.
fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

#[derive(Debug, Deserialize)]
pub struct ItemNuevo {
    /// id del producto
    pub id: i64,
    pub cantidad: f64,
    /// Precio unitario ofrecido (precio final, con IGV).
    pub precio: f64,
    /// Medidas u opciones ("5 pzas de 2\" x 4\" x 10 pies").
    #[serde(default)]
    pub detalle: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NuevaCotizacion {
    #[serde(default)]
    pub cliente_id: Option<i64>,
    /// Nombre escrito a mano cuando el cliente no está registrado.
    #[serde(default)]
    pub cliente_nombre: Option<String>,
    #[serde(default)]
    pub validez_dias: Option<i64>,
    #[serde(default)]
    pub notas: Option<String>,
    pub items: Vec<ItemNuevo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ItemCotizacion {
    pub producto_id: i64,
    pub nombre: String,
    pub detalle: Option<String>,
    pub unidad_medida: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub total_linea: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Cotizacion {
    pub id: i64,
    pub numero: i64,
    pub cliente_id: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub total: f64,
    pub validez_dias: i64,
    pub notas: Option<String>,
    /// 'PENDIENTE' | 'VENDIDA' | 'ANULADA'
    pub estado: String,
    pub venta_id: Option<i64>,
    pub folio_venta: Option<String>,
    pub usuario: String,
    /// "2026-10-03 14:05:09" (hora de Perú)
    pub fecha: String,
    /// Último día en que vale el precio ("2026-10-10").
    pub vence: String,
    /// PENDIENTE y ya pasó su fecha.
    pub vencida: bool,
    pub cantidad_items: i64,
    /// Solo en el detalle (GET /cotizaciones/:id y al crear).
    pub items: Vec<ItemCotizacion>,
}

const COLUMNAS: &str = "c.id, c.numero, c.cliente_id, c.cliente_nombre, c.cliente_documento, CAST(c.total AS REAL),
    c.validez_dias, c.notas, c.estado, c.venta_id, v.folio, COALESCE(u.nombre_completo, ''), c.fecha,
    date(substr(c.fecha, 1, 10), '+' || c.validez_dias || ' days'),
    (SELECT COUNT(*) FROM cotizacion_items i WHERE i.cotizacion_id = c.id)
    FROM cotizaciones c
    LEFT JOIN ventas v ON v.id = c.venta_id
    LEFT JOIN usuarios u ON u.id = c.usuario_id";

fn de_fila(f: &libsql::Row, hoy: &str) -> Cotizacion {
    let estado: String = f.get(8).unwrap_or_default();
    let vence: String = f.get(13).unwrap_or_default();
    Cotizacion {
        id: f.get(0).unwrap_or_default(),
        numero: f.get(1).unwrap_or_default(),
        cliente_id: f.get::<i64>(2).ok(),
        cliente_nombre: f.get::<String>(3).ok(),
        cliente_documento: f.get::<String>(4).ok(),
        total: f.get(5).unwrap_or(0.0),
        validez_dias: f.get(6).unwrap_or(VALIDEZ_POR_DEFECTO),
        notas: f.get::<String>(7).ok(),
        vencida: estado == "PENDIENTE" && !vence.is_empty() && vence.as_str() < hoy,
        estado,
        venta_id: f.get::<i64>(9).ok(),
        folio_venta: f.get::<String>(10).ok(),
        usuario: f.get(11).unwrap_or_default(),
        fecha: f.get(12).unwrap_or_default(),
        vence,
        cantidad_items: f.get(14).unwrap_or(0),
        items: Vec::new(),
    }
}

async fn detalle(conn: &libsql::Connection, id: i64) -> Result<Cotizacion, (StatusCode, String)> {
    let hoy = crate::logica::tiempo::hoy_lima();
    let mut filas = conn
        .query(&format!("SELECT {} WHERE c.id = ?1", COLUMNAS), libsql::params![id])
        .await
        .map_err(actualizando)?;
    let mut cot = match filas.next().await.map_err(e500)? {
        Some(f) => de_fila(&f, &hoy),
        None => return Err((StatusCode::NOT_FOUND, "Esa cotización no existe.".to_string())),
    };
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT producto_id, nombre, detalle, COALESCE(unidad_medida, 'UNIDAD'), CAST(cantidad AS REAL),
                    CAST(precio_unitario AS REAL), CAST(total_linea AS REAL)
             FROM cotizacion_items WHERE cotizacion_id = ?1 ORDER BY id",
            libsql::params![id],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        cot.items.push(ItemCotizacion {
            producto_id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            detalle: f.get::<String>(2).ok(),
            unidad_medida: f.get(3).unwrap_or_default(),
            cantidad: f.get(4).unwrap_or(0.0),
            precio_unitario: f.get(5).unwrap_or(0.0),
            total_linea: f.get(6).unwrap_or(0.0),
        });
    }
    Ok(cot)
}

#[derive(Debug, Deserialize)]
pub struct Filtros {
    /// 'PENDIENTE' | 'VENDIDA' | 'ANULADA'; sin filtro = todas.
    pub estado: Option<String>,
}

/// GET /cotizaciones — las últimas 200, de la más nueva a la más antigua.
pub async fn listar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(filtros): Query<Filtros>,
) -> Resultado<Vec<Cotizacion>> {
    let conn = tenant.0.connect().map_err(e500)?;
    let hoy = crate::logica::tiempo::hoy_lima();
    let estado = filtros.estado.filter(|e| ["PENDIENTE", "VENDIDA", "ANULADA"].contains(&e.as_str()));
    let mut filas = match &estado {
        Some(e) => conn
            .query(&format!("SELECT {} WHERE c.estado = ?1 ORDER BY c.id DESC LIMIT 200", COLUMNAS), libsql::params![e.clone()])
            .await,
        None => conn.query(&format!("SELECT {} ORDER BY c.id DESC LIMIT 200", COLUMNAS), ()).await,
    }
    .map_err(actualizando)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(de_fila(&f, &hoy));
    }
    Ok(Json(lista))
}

/// GET /cotizaciones/:id — con sus líneas.
pub async fn obtener(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Resultado<Cotizacion> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(detalle(&conn, id).await?))
}

/// POST /cotizaciones — guarda lo que hay en el carrito como cotización.
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevaCotizacion>,
) -> Resultado<Cotizacion> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_COTIZACIONES, "Cotizaciones").await?;
    if payload.items.is_empty() {
        return Err(malo("La cotización no tiene productos."));
    }
    if payload.items.len() > MAXIMO_ITEMS {
        return Err(malo("La cotización tiene demasiadas líneas."));
    }
    let validez = payload.validez_dias.unwrap_or(VALIDEZ_POR_DEFECTO);
    if !(1..=365).contains(&validez) {
        return Err(malo("La validez debe estar entre 1 y 365 días."));
    }
    conn.query("SELECT 1 FROM cotizaciones LIMIT 1", ()).await.map_err(actualizando)?;

    // Nombre y unidad reales de cada producto (no los que mande el navegador).
    let mut lineas: Vec<ItemCotizacion> = Vec::new();
    for it in &payload.items {
        if !(it.cantidad > 0.0) || !it.cantidad.is_finite() {
            return Err(malo("Cada línea debe tener una cantidad mayor a 0."));
        }
        if !(it.precio >= 0.0) || !it.precio.is_finite() {
            return Err(malo("Hay un precio que no es válido."));
        }
        let mut filas = conn
            .query("SELECT nombre, unidad_medida FROM productos WHERE id = ?1", libsql::params![it.id])
            .await
            .map_err(e500)?;
        let (nombre, unidad): (String, String) = match filas.next().await.map_err(e500)? {
            Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_else(|_| "UNIDAD".to_string())),
            None => return Err(malo("Uno de los productos ya no existe.")),
        };
        lineas.push(ItemCotizacion {
            producto_id: it.id,
            nombre,
            detalle: it.detalle.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(|d| d.chars().take(120).collect()),
            unidad_medida: unidad,
            cantidad: it.cantidad,
            precio_unitario: it.precio,
            total_linea: round2(it.precio * it.cantidad),
        });
    }
    let total = round2(lineas.iter().map(|l| l.total_linea).sum());

    // Cliente registrado (se copian su nombre y documento) o nombre escrito.
    let (cliente_nombre, cliente_documento): (Option<String>, Option<String>) = match payload.cliente_id {
        Some(cid) => {
            let mut filas = conn
                .query("SELECT nombre_razon_social, numero_documento FROM clientes WHERE id = ?1", libsql::params![cid])
                .await
                .map_err(e500)?;
            match filas.next().await.map_err(e500)? {
                Some(f) => (f.get::<String>(0).ok(), f.get::<String>(1).ok()),
                None => return Err(malo("El cliente no existe.")),
            }
        }
        None => (
            payload
                .cliente_nombre
                .as_deref()
                .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|n| !n.is_empty())
                .map(|n| n.chars().take(120).collect()),
            None,
        ),
    };
    let notas = payload.notas.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(|n| n.chars().take(500).collect::<String>());

    // El número es correlativo del negocio y lo calcula la misma sentencia
    // que inserta, para que dos cajas a la vez no repitan número.
    let mut filas = conn
        .query(
            "INSERT INTO cotizaciones (numero, cliente_id, cliente_nombre, cliente_documento, total, validez_dias, notas, usuario_id, fecha)
             VALUES ((SELECT COALESCE(MAX(numero), 0) + 1 FROM cotizaciones), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             RETURNING id",
            libsql::params![payload.cliente_id, cliente_nombre, cliente_documento, total, validez, notas, claims.sub, ahora_lima()],
        )
        .await
        .map_err(e500)?;
    let id: i64 = filas
        .next()
        .await
        .map_err(e500)?
        .and_then(|f| f.get(0).ok())
        .ok_or_else(|| e500("No se pudo guardar la cotización."))?;
    // Se lee la respuesta hasta el final antes de seguir (ver ventas.rs).
    while filas.next().await.map_err(e500)?.is_some() {}
    drop(filas);

    for l in &lineas {
        conn.execute(
            "INSERT INTO cotizacion_items (cotizacion_id, producto_id, nombre, detalle, unidad_medida, cantidad, precio_unitario, total_linea)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            libsql::params![id, l.producto_id, l.nombre.clone(), l.detalle.clone(), l.unidad_medida.clone(), l.cantidad, l.precio_unitario, l.total_linea],
        )
        .await
        .map_err(e500)?;
    }
    Ok(Json(detalle(&conn, id).await?))
}

/// POST /cotizaciones/:id/anular — solo si sigue pendiente.
pub async fn anular(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Resultado<Cotizacion> {
    let conn = tenant.0.connect().map_err(e500)?;
    let cambiadas = conn
        .execute("UPDATE cotizaciones SET estado = 'ANULADA' WHERE id = ?1 AND estado = 'PENDIENTE'", libsql::params![id])
        .await
        .map_err(actualizando)?;
    let cot = detalle(&conn, id).await?;
    if cambiadas == 0 {
        return Err((StatusCode::CONFLICT, format!("La cotización ya está {}.", cot.estado.to_lowercase())));
    }
    Ok(Json(cot))
}

/// La venta que salió de una cotización la deja VENDIDA. Se llama después
/// de registrar la venta y nunca la hace fallar.
pub async fn marcar_vendida(conn: &libsql::Connection, cotizacion_id: i64, venta_id: i64) {
    let _ = conn
        .execute(
            "UPDATE cotizaciones SET estado = 'VENDIDA', venta_id = ?1 WHERE id = ?2 AND estado = 'PENDIENTE'",
            libsql::params![venta_id, cotizacion_id],
        )
        .await;
}
