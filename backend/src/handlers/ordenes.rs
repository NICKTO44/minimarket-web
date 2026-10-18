//! Módulo ORDENES_SERVICIO (migración 0040): órdenes de servicio del
//! taller, mecánicos y comisiones. Pensado para el rubro Lubricentro;
//! necesita el módulo VEHICULOS (cada orden es de un vehículo).
//!
//! Una orden no toca el stock ni la caja: es una lista de lo que se le va
//! poniendo al vehículo. Se cobra en el punto de venta como una venta normal
//! (con la placa y el kilometraje de la orden); al cobrarla queda COBRADA con
//! su venta, dentro de la misma transacción.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET  /ordenes?estado=            -> ABIERTAS (abiertas y terminadas, por defecto), COBRADAS o TODAS
//!   POST /ordenes                    -> nueva (crea el vehículo si no existía)
//!   GET  /ordenes/:id                -> la orden con sus ítems
//!   PUT  /ordenes/:id                -> mecánico, km, próximo servicio, observaciones
//!   PUT  /ordenes/:id/items          -> reemplaza lo que lleva
//!   POST /ordenes/:id/estado         -> TERMINADA, ABIERTA (reabrir) o ANULADA
//!   GET  /ordenes/comisiones?desde=&hasta=  -> por mecánico (solo administrador)
//!   GET  /mecanicos, POST /mecanicos, PUT /mecanicos/:id

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_ORDENES_SERVICIO};
use crate::handlers::vehiculos::{self, VehiculoVenta};
use crate::logica::tiempo;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Órdenes de servicio";
const MAX_ITEMS: usize = 200;

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") {
        (StatusCode::SERVICE_UNAVAILABLE, "Las órdenes de servicio aún no están listas en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, Fallo> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(malo(format!("{}: máximo {} letras.", campo, max))),
        otro => Ok(otro),
    }
}
fn redondear_2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_ORDENES_SERVICIO, NOMBRE_MODULO).await
}

// ---------------------------------------------------------------------
// Mecánicos
// ---------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Mecanico {
    #[serde(default)]
    pub id: Option<i64>,
    pub nombre: String,
    #[serde(default)]
    pub telefono: Option<String>,
    #[serde(default)]
    pub comision_porcentaje: f64,
    #[serde(default = "verdadero")]
    pub activo: bool,
}
fn verdadero() -> bool {
    true
}

fn validar_mecanico(m: Mecanico) -> Result<Mecanico, Fallo> {
    let nombre = texto(Some(&m.nombre), 80, "Nombre")?.ok_or_else(|| malo("Escribe el nombre del mecánico."))?;
    if !m.comision_porcentaje.is_finite() || !(0.0..=100.0).contains(&m.comision_porcentaje) {
        return Err(malo("La comisión es un porcentaje de 0 a 100."));
    }
    Ok(Mecanico {
        id: None,
        nombre,
        telefono: texto(m.telefono.as_deref(), 20, "Teléfono")?,
        comision_porcentaje: redondear_2(m.comision_porcentaje),
        activo: m.activo,
    })
}

/// GET /mecanicos — todos (los activos primero).
pub async fn mecanicos(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Mecanico>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT id, nombre, telefono, CAST(comision_porcentaje AS REAL), activo FROM mecanicos ORDER BY activo DESC, nombre",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(Mecanico {
            id: f.get(0).ok(),
            nombre: f.get(1).unwrap_or_default(),
            telefono: f.get(2).ok(),
            comision_porcentaje: f.get(3).unwrap_or(0.0),
            activo: f.get::<i64>(4).unwrap_or(1) == 1,
        });
    }
    Ok(Json(lista))
}

/// POST /mecanicos — solo el administrador.
pub async fn crear_mecanico(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(m): Json<Mecanico>,
) -> Result<Json<Mecanico>, Fallo> {
    exigir_admin(&claims)?;
    let m = validar_mecanico(m)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    conn.execute(
        "INSERT INTO mecanicos (nombre, telefono, comision_porcentaje, activo, fecha_creacion) VALUES (?1, ?2, ?3, ?4, ?5)",
        libsql::params![m.nombre.clone(), m.telefono.clone(), m.comision_porcentaje, m.activo as i64, tiempo::ahora_lima()],
    )
    .await
    .map_err(sin_migracion)?;
    Ok(Json(Mecanico { id: Some(conn.last_insert_rowid()), ..m }))
}

/// PUT /mecanicos/:id — solo el administrador (activo = false lo da de baja).
pub async fn actualizar_mecanico(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(m): Json<Mecanico>,
) -> Result<Json<Mecanico>, Fallo> {
    exigir_admin(&claims)?;
    let m = validar_mecanico(m)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let n = conn
        .execute(
            "UPDATE mecanicos SET nombre = ?1, telefono = ?2, comision_porcentaje = ?3, activo = ?4 WHERE id = ?5",
            libsql::params![m.nombre.clone(), m.telefono.clone(), m.comision_porcentaje, m.activo as i64, id],
        )
        .await
        .map_err(sin_migracion)?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, "El mecánico no existe.".into()));
    }
    Ok(Json(Mecanico { id: Some(id), ..m }))
}

async fn mecanico_activo(conn: &libsql::Connection, mecanico_id: Option<i64>) -> Result<(), Fallo> {
    let Some(id) = mecanico_id else { return Ok(()) };
    let mut filas = conn.query("SELECT activo FROM mecanicos WHERE id = ?1", libsql::params![id]).await.map_err(sin_migracion)?;
    match filas.next().await.map_err(e500)? {
        Some(f) if f.get::<i64>(0).unwrap_or(0) == 1 => Ok(()),
        Some(_) => Err(malo("Ese mecánico está dado de baja.")),
        None => Err(malo("El mecánico elegido no existe.")),
    }
}

// ---------------------------------------------------------------------
// Órdenes
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ItemOrden {
    pub id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub descuento: f64,
    pub total: f64,
    pub es_servicio: bool,
    pub paquete_id: Option<i64>,
    /// Lo que hay hoy del producto (para avisar antes de cobrar).
    pub stock: f64,
    pub controla_stock: bool,
    pub unidad_medida: String,
}

#[derive(Debug, Serialize)]
pub struct Orden {
    pub id: i64,
    pub numero: i64,
    pub estado: String,
    pub vehiculo_id: i64,
    pub placa: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub anio: Option<i64>,
    pub cliente_id: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub cliente_telefono: Option<String>,
    pub mecanico_id: Option<i64>,
    pub mecanico_nombre: Option<String>,
    pub km: Option<i64>,
    pub proximo_km: Option<i64>,
    pub proxima_fecha: Option<String>,
    pub observaciones: Option<String>,
    pub venta_id: Option<i64>,
    pub venta_folio: Option<String>,
    pub fecha_creacion: Option<String>,
    pub fecha_terminada: Option<String>,
    pub fecha_cobro: Option<String>,
    pub total: f64,
    pub items_cantidad: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ItemOrden>>,
}

const SELECT_ORDEN: &str = "SELECT o.id, o.numero, o.estado, o.vehiculo_id, o.placa, v.marca, v.modelo, v.anio,
            o.cliente_id, c.nombre_razon_social, c.numero_documento, c.telefono,
            o.mecanico_id, m.nombre, o.km, o.proximo_km, o.proxima_fecha, o.observaciones,
            o.venta_id, ve.folio, o.fecha_creacion, o.fecha_terminada, o.fecha_cobro,
            CAST(COALESCE((SELECT SUM(i.cantidad * i.precio_unitario - i.descuento) FROM orden_items i WHERE i.orden_id = o.id), 0) AS REAL),
            (SELECT COUNT(*) FROM orden_items i WHERE i.orden_id = o.id)
     FROM ordenes_servicio o
     LEFT JOIN vehiculos v ON v.id = o.vehiculo_id
     LEFT JOIN clientes c ON c.id = o.cliente_id
     LEFT JOIN mecanicos m ON m.id = o.mecanico_id
     LEFT JOIN ventas ve ON ve.id = o.venta_id";

fn orden_de(f: &libsql::Row) -> Orden {
    Orden {
        id: f.get(0).unwrap_or_default(),
        numero: f.get(1).unwrap_or_default(),
        estado: f.get(2).unwrap_or_default(),
        vehiculo_id: f.get(3).unwrap_or_default(),
        placa: f.get(4).unwrap_or_default(),
        marca: f.get(5).ok(),
        modelo: f.get(6).ok(),
        anio: f.get(7).ok(),
        cliente_id: f.get(8).ok(),
        cliente_nombre: f.get(9).ok(),
        cliente_documento: f.get(10).ok(),
        cliente_telefono: f.get(11).ok(),
        mecanico_id: f.get(12).ok(),
        mecanico_nombre: f.get(13).ok(),
        km: f.get(14).ok(),
        proximo_km: f.get(15).ok(),
        proxima_fecha: f.get(16).ok(),
        observaciones: f.get(17).ok(),
        venta_id: f.get(18).ok(),
        venta_folio: f.get(19).ok(),
        fecha_creacion: f.get(20).ok(),
        fecha_terminada: f.get(21).ok(),
        fecha_cobro: f.get(22).ok(),
        total: redondear_2(f.get(23).unwrap_or(0.0)),
        items_cantidad: f.get(24).unwrap_or(0),
        items: None,
    }
}

async fn leer_orden(conn: &libsql::Connection, id: i64) -> Result<Orden, Fallo> {
    let mut filas = conn
        .query(&format!("{} WHERE o.id = ?1", SELECT_ORDEN), libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let mut orden = match filas.next().await.map_err(e500)? {
        Some(f) => orden_de(&f),
        None => return Err((StatusCode::NOT_FOUND, "La orden no existe.".into())),
    };
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT i.id, i.producto_id, i.nombre, CAST(i.cantidad AS REAL), CAST(i.precio_unitario AS REAL), CAST(i.descuento AS REAL),
                    i.es_servicio, i.paquete_id, CAST(COALESCE(p.stock, 0) AS REAL), COALESCE(p.controla_stock, 1),
                    COALESCE(p.unidad_medida, 'UNIDAD')
             FROM orden_items i LEFT JOIN productos p ON p.id = i.producto_id
             WHERE i.orden_id = ?1 ORDER BY i.id",
            libsql::params![id],
        )
        .await
        .map_err(e500)?;
    let mut items = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(3).unwrap_or(0.0);
        let precio: f64 = f.get(4).unwrap_or(0.0);
        let descuento: f64 = f.get(5).unwrap_or(0.0);
        items.push(ItemOrden {
            id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            nombre: f.get(2).unwrap_or_default(),
            cantidad,
            precio_unitario: precio,
            descuento,
            total: redondear_2(redondear_2(cantidad * precio) - descuento),
            es_servicio: f.get::<i64>(6).unwrap_or(0) == 1,
            paquete_id: f.get(7).ok(),
            stock: f.get(8).unwrap_or(0.0),
            controla_stock: f.get::<i64>(9).unwrap_or(1) == 1,
            unidad_medida: f.get(10).unwrap_or_default(),
        });
    }
    orden.items = Some(items);
    Ok(orden)
}

#[derive(Debug, Deserialize)]
pub struct FiltroOrdenes {
    #[serde(default)]
    pub estado: Option<String>,
}

/// GET /ordenes?estado= — ABIERTAS (abiertas y terminadas; por defecto),
/// COBRADAS (las 200 últimas) o TODAS (las 300 últimas).
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<FiltroOrdenes>) -> Result<Json<Vec<Orden>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (donde, limite) = match q.estado.as_deref().unwrap_or("ABIERTAS") {
        "ABIERTAS" => ("o.estado IN ('ABIERTA', 'TERMINADA')", 500),
        "COBRADAS" => ("o.estado = 'COBRADA'", 200),
        "TODAS" => ("1", 300),
        _ => return Err(malo("Estado no válido.")),
    };
    let mut filas = conn
        .query(&format!("{} WHERE {} ORDER BY o.id DESC LIMIT {}", SELECT_ORDEN, donde, limite), ())
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(orden_de(&f));
    }
    Ok(Json(lista))
}

/// GET /ordenes/:id
pub async fn obtener(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Orden>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer_orden(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct NuevaOrden {
    pub placa: String,
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub anio: Option<i64>,
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub mecanico_id: Option<i64>,
    #[serde(default)]
    pub km: Option<i64>,
    #[serde(default)]
    pub observaciones: Option<String>,
}

/// POST /ordenes — una orden nueva. Si la placa no existía, se crea el
/// vehículo; si ya tiene una orden abierta, no se abre otra.
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(n): Json<NuevaOrden>,
) -> Result<Json<Orden>, Fallo> {
    let listo = vehiculos::revisar_venta(&VehiculoVenta {
        placa: n.placa.clone(),
        marca: n.marca.clone(),
        modelo: n.modelo.clone(),
        anio: n.anio,
        km: n.km,
        ..Default::default()
    })
    .map_err(malo)?;
    let observaciones = texto(n.observaciones.as_deref(), 500, "Observaciones")?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    mecanico_activo(&conn, n.mecanico_id).await?;
    if let Some(id) = n.cliente_id {
        let mut f = conn.query("SELECT 1 FROM clientes WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
        if f.next().await.map_err(e500)?.is_none() {
            return Err(malo("El cliente elegido no existe."));
        }
    }
    let tx = conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
    let resultado: Result<i64, Fallo> = async {
        let vehiculo_id = vehiculos::asegurar(&tx, &listo, n.cliente_id).await?;
        let mut filas = tx
            .query(
                "SELECT numero FROM ordenes_servicio WHERE vehiculo_id = ?1 AND estado IN ('ABIERTA', 'TERMINADA') LIMIT 1",
                libsql::params![vehiculo_id],
            )
            .await
            .map_err(sin_migracion)?;
        if let Some(f) = filas.next().await.map_err(e500)? {
            return Err((
                StatusCode::CONFLICT,
                format!("La placa {} ya tiene abierta la orden N.° {}.", listo.placa, f.get::<i64>(0).unwrap_or_default()),
            ));
        }
        drop(filas);
        // El dueño de la orden: el elegido o, si no, el del vehículo.
        tx.execute(
            "INSERT INTO ordenes_servicio (numero, vehiculo_id, placa, cliente_id, mecanico_id, km, observaciones, estado, usuario_id, fecha_creacion)
             VALUES ((SELECT COALESCE(MAX(numero), 0) + 1 FROM ordenes_servicio), ?1, ?2,
                     COALESCE(?3, (SELECT cliente_id FROM vehiculos WHERE id = ?1)), ?4, ?5, ?6, 'ABIERTA', ?7, ?8)",
            libsql::params![vehiculo_id, listo.placa.clone(), n.cliente_id, n.mecanico_id, listo.km, observaciones, claims.sub, tiempo::ahora_lima()],
        )
        .await
        .map_err(sin_migracion)?;
        Ok(tx.last_insert_rowid())
    }
    .await;
    let id = match resultado {
        Ok(id) => {
            tx.commit().await.map_err(e500)?;
            id
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    };
    Ok(Json(leer_orden(&conn, id).await?))
}

async fn estado_de(conn: &libsql::Connection, id: i64) -> Result<String, Fallo> {
    let mut filas = conn.query("SELECT estado FROM ordenes_servicio WHERE id = ?1", libsql::params![id]).await.map_err(sin_migracion)?;
    match filas.next().await.map_err(e500)? {
        Some(f) => Ok(f.get(0).unwrap_or_default()),
        None => Err((StatusCode::NOT_FOUND, "La orden no existe.".into())),
    }
}

fn editable(estado: &str) -> Result<(), Fallo> {
    match estado {
        "ABIERTA" | "TERMINADA" => Ok(()),
        "COBRADA" => Err((StatusCode::CONFLICT, "La orden ya se cobró: no se puede cambiar.".into())),
        _ => Err((StatusCode::CONFLICT, "La orden está anulada.".into())),
    }
}

#[derive(Debug, Deserialize)]
pub struct DatosOrden {
    #[serde(default)]
    pub mecanico_id: Option<i64>,
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub km: Option<i64>,
    #[serde(default)]
    pub proximo_km: Option<i64>,
    #[serde(default)]
    pub proxima_fecha: Option<String>,
    #[serde(default)]
    pub observaciones: Option<String>,
}

/// PUT /ordenes/:id — mecánico, dueño, kilometraje, próximo servicio y observaciones.
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(d): Json<DatosOrden>,
) -> Result<Json<Orden>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    editable(&estado_de(&conn, id).await?)?;
    let placa: String = {
        let mut f = conn.query("SELECT placa FROM ordenes_servicio WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
        f.next().await.map_err(e500)?.and_then(|f| f.get(0).ok()).unwrap_or_default()
    };
    // Mismas reglas que en la venta (km, próximo mayor que el actual, fecha futura).
    let listo = vehiculos::revisar_venta(&VehiculoVenta {
        placa,
        km: d.km,
        proximo_km: d.proximo_km,
        proxima_fecha: d.proxima_fecha.clone(),
        ..Default::default()
    })
    .map_err(malo)?;
    let observaciones = texto(d.observaciones.as_deref(), 500, "Observaciones")?;
    mecanico_activo(&conn, d.mecanico_id).await?;
    conn.execute(
        "UPDATE ordenes_servicio SET mecanico_id = ?1, cliente_id = ?2, km = ?3, proximo_km = ?4, proxima_fecha = ?5, observaciones = ?6
         WHERE id = ?7 AND estado IN ('ABIERTA', 'TERMINADA')",
        libsql::params![d.mecanico_id, d.cliente_id, listo.km, listo.proximo_km, listo.proxima_fecha, observaciones, id],
    )
    .await
    .map_err(sin_migracion)?;
    Ok(Json(leer_orden(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct ItemNuevo {
    pub producto_id: i64,
    pub cantidad: f64,
    #[serde(default)]
    pub descuento: Option<f64>,
    #[serde(default)]
    pub paquete_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ItemsOrden {
    pub items: Vec<ItemNuevo>,
}

/// PUT /ordenes/:id/items — reemplaza lo que lleva la orden. Nombre y precio
/// los pone el servidor (los del producto hoy); un producto que ya estaba
/// en la orden conserva el precio con que se agregó.
pub async fn guardar_items(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(p): Json<ItemsOrden>,
) -> Result<Json<Orden>, Fallo> {
    if p.items.len() > MAX_ITEMS {
        return Err(malo(format!("Máximo {} líneas por orden.", MAX_ITEMS)));
    }
    for i in &p.items {
        if !(i.cantidad > 0.0) || !i.cantidad.is_finite() || i.cantidad > 1_000_000.0 {
            return Err(malo("Cada línea necesita una cantidad mayor a 0."));
        }
        if i.descuento.is_some_and(|d| !d.is_finite() || d < 0.0) {
            return Err(malo("El descuento no es válido."));
        }
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    editable(&estado_de(&conn, id).await?)?;

    // Los productos (activos) y los precios con que ya estaban en la orden.
    let ids: Vec<String> = p.items.iter().map(|i| i.producto_id.to_string()).collect();
    let mut productos: HashMap<i64, (String, f64, bool)> = HashMap::new();
    if !ids.is_empty() {
        let mut filas = conn
            .query(
                &format!(
                    "SELECT id, nombre, CAST(precio AS REAL), COALESCE(controla_stock, 1) FROM productos WHERE activo = 1 AND id IN ({})",
                    ids.join(",")
                ),
                (),
            )
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            productos.insert(
                f.get(0).unwrap_or_default(),
                (f.get(1).unwrap_or_default(), f.get(2).unwrap_or(0.0), f.get::<i64>(3).unwrap_or(1) == 0),
            );
        }
    }
    let mut previos: HashMap<i64, f64> = HashMap::new();
    {
        let mut filas = conn
            .query("SELECT producto_id, CAST(precio_unitario AS REAL) FROM orden_items WHERE orden_id = ?1", libsql::params![id])
            .await
            .map_err(sin_migracion)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            previos.insert(f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0));
        }
    }
    let mut filas_nuevas = Vec::with_capacity(p.items.len());
    for i in &p.items {
        let Some((nombre, precio_hoy, es_servicio)) = productos.get(&i.producto_id) else {
            return Err(malo("Uno de los productos ya no existe o está desactivado. Vuelve a cargar la orden."));
        };
        let precio = previos.get(&i.producto_id).copied().unwrap_or(*precio_hoy);
        let sub = redondear_2(precio * i.cantidad);
        let descuento = redondear_2(i.descuento.unwrap_or(0.0)).min(sub);
        filas_nuevas.push((i.producto_id, nombre.clone(), i.cantidad, precio, descuento, *es_servicio, i.paquete_id));
    }

    let tx = conn.transaction().await.map_err(e500)?;
    let guardar: Result<(), Fallo> = async {
        // Revisa de nuevo dentro de la transacción: nadie la cobró mientras tanto.
        editable(&estado_de(&tx, id).await?)?;
        tx.execute("DELETE FROM orden_items WHERE orden_id = ?1", libsql::params![id]).await.map_err(e500)?;
        for parte in filas_nuevas.chunks(80) {
            const COL: usize = 8;
            let mut marcas = Vec::with_capacity(parte.len());
            let mut valores: Vec<libsql::Value> = Vec::with_capacity(parte.len() * COL);
            for (n, (pid, nombre, cantidad, precio, descuento, servicio, paquete)) in parte.iter().enumerate() {
                let b = n * COL;
                marcas.push(format!("({})", (1..=COL).map(|k| format!("?{}", b + k)).collect::<Vec<_>>().join(", ")));
                valores.extend([
                    libsql::Value::Integer(id),
                    libsql::Value::Integer(*pid),
                    libsql::Value::Text(nombre.clone()),
                    libsql::Value::Real(*cantidad),
                    libsql::Value::Real(*precio),
                    libsql::Value::Real(*descuento),
                    libsql::Value::Integer(*servicio as i64),
                    paquete.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null),
                ]);
            }
            tx.execute(
                &format!(
                    "INSERT INTO orden_items (orden_id, producto_id, nombre, cantidad, precio_unitario, descuento, es_servicio, paquete_id) VALUES {}",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(e500)?;
        }
        Ok(())
    }
    .await;
    match guardar {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    }
    Ok(Json(leer_orden(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct CambioEstado {
    pub estado: String,
    #[serde(default)]
    pub motivo: Option<String>,
}

/// POST /ordenes/:id/estado — TERMINADA (lista para cobrar), ABIERTA
/// (reabrir) o ANULADA (con motivo). COBRADA solo la pone la venta.
pub async fn cambiar_estado(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(c): Json<CambioEstado>,
) -> Result<Json<Orden>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    editable(&estado_de(&conn, id).await?)?;
    let ahora = tiempo::ahora_lima();
    let n = match c.estado.as_str() {
        "TERMINADA" => {
            conn.execute(
                "UPDATE ordenes_servicio SET estado = 'TERMINADA', fecha_terminada = ?1 WHERE id = ?2 AND estado IN ('ABIERTA', 'TERMINADA')",
                libsql::params![ahora, id],
            )
            .await
        }
        "ABIERTA" => {
            conn.execute(
                "UPDATE ordenes_servicio SET estado = 'ABIERTA', fecha_terminada = NULL WHERE id = ?1 AND estado IN ('ABIERTA', 'TERMINADA')",
                libsql::params![id],
            )
            .await
        }
        "ANULADA" => {
            let motivo = texto(c.motivo.as_deref(), 200, "Motivo")?.ok_or_else(|| malo("Escribe por qué se anula la orden."))?;
            conn.execute(
                "UPDATE ordenes_servicio SET estado = 'ANULADA', motivo_anulacion = ?1 WHERE id = ?2 AND estado IN ('ABIERTA', 'TERMINADA')",
                libsql::params![motivo, id],
            )
            .await
        }
        _ => return Err(malo("Estado no válido.")),
    }
    .map_err(sin_migracion)?;
    if n == 0 {
        return Err((StatusCode::CONFLICT, "La orden cambió mientras tanto. Vuelve a cargarla.".into()));
    }
    Ok(Json(leer_orden(&conn, id).await?))
}

// ---------------------------------------------------------------------
// Cobro (desde la venta)
// ---------------------------------------------------------------------

/// Antes de registrar la venta: la orden debe existir y seguir abierta o
/// terminada. Devuelve los precios con que se agregó cada producto (la
/// venta puede cobrarlos aunque el producto haya cambiado de precio).
pub async fn preparar_cobro(
    conn: &libsql::Connection,
    modulos: Option<&str>,
    orden_id: Option<i64>,
) -> Result<Option<(i64, HashMap<i64, Vec<f64>>)>, Fallo> {
    let Some(id) = orden_id else { return Ok(None) };
    if !modulos.is_some_and(|m| m.split(',').any(|x| x.trim() == MODULO_ORDENES_SERVICIO)) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ));
    }
    let estado = estado_de(conn, id).await?;
    if estado == "COBRADA" {
        return Err((StatusCode::CONFLICT, "Esta orden ya se cobró.".into()));
    }
    editable(&estado)?;
    let mut precios: HashMap<i64, Vec<f64>> = HashMap::new();
    let mut filas = conn
        .query("SELECT producto_id, CAST(precio_unitario AS REAL) FROM orden_items WHERE orden_id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        precios.entry(f.get(0).unwrap_or_default()).or_default().push(f.get(1).unwrap_or(-1.0));
    }
    Ok(Some((id, precios)))
}

/// Dentro de la transacción de la venta: la orden queda COBRADA con su
/// venta. Si otra caja la cobró un instante antes, la venta no entra.
pub async fn marcar_cobrada(conn: &libsql::Connection, orden_id: i64, venta_id: i64) -> Result<(), Fallo> {
    let n = conn
        .execute(
            "UPDATE ordenes_servicio SET estado = 'COBRADA', venta_id = ?1, fecha_cobro = ?2
             WHERE id = ?3 AND estado IN ('ABIERTA', 'TERMINADA')",
            libsql::params![venta_id, tiempo::ahora_lima(), orden_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo cerrar la orden: {}", e)))?;
    if n == 0 {
        return Err((StatusCode::CONFLICT, "Esta orden ya se cobró o se anuló. Revisa Órdenes de servicio.".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Comisiones
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Periodo {
    pub desde: String,
    pub hasta: String,
}

#[derive(Debug, Serialize)]
pub struct Comision {
    pub mecanico_id: Option<i64>,
    pub mecanico_nombre: String,
    pub comision_porcentaje: f64,
    pub ordenes: i64,
    /// Mano de obra cobrada (base de la comisión).
    pub mano_de_obra: f64,
    /// Repuestos y lo demás (solo informativo).
    pub repuestos: f64,
    pub comision: f64,
}

/// GET /ordenes/comisiones?desde=AAAA-MM-DD&hasta=AAAA-MM-DD — por mecánico,
/// de las órdenes cobradas en esos días cuya venta sigue vigente. Solo el
/// administrador.
pub async fn comisiones(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(q): Query<Periodo>,
) -> Result<Json<Vec<Comision>>, Fallo> {
    exigir_admin(&claims)?;
    if !tiempo::fecha_valida(&q.desde) || !tiempo::fecha_valida(&q.hasta) || q.desde > q.hasta {
        return Err(malo("Elige un periodo válido."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT o.mecanico_id, COALESCE(m.nombre, 'Sin mecánico'), CAST(COALESCE(m.comision_porcentaje, 0) AS REAL),
                    COUNT(DISTINCT o.id),
                    CAST(COALESCE(SUM(CASE WHEN i.es_servicio = 1 THEN i.cantidad * i.precio_unitario - i.descuento END), 0) AS REAL),
                    CAST(COALESCE(SUM(CASE WHEN i.es_servicio = 0 THEN i.cantidad * i.precio_unitario - i.descuento END), 0) AS REAL)
             FROM ordenes_servicio o
             JOIN ventas v ON v.id = o.venta_id AND COALESCE(v.estado, 'COMPLETADA') = 'COMPLETADA'
             LEFT JOIN mecanicos m ON m.id = o.mecanico_id
             LEFT JOIN orden_items i ON i.orden_id = o.id
             WHERE o.estado = 'COBRADA' AND substr(o.fecha_cobro, 1, 10) BETWEEN ?1 AND ?2
             GROUP BY o.mecanico_id
             ORDER BY 5 DESC",
            libsql::params![q.desde, q.hasta],
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let porcentaje: f64 = f.get(2).unwrap_or(0.0);
        let mano: f64 = redondear_2(f.get(4).unwrap_or(0.0));
        lista.push(Comision {
            mecanico_id: f.get(0).ok(),
            mecanico_nombre: f.get(1).unwrap_or_default(),
            comision_porcentaje: porcentaje,
            ordenes: f.get(3).unwrap_or(0),
            mano_de_obra: mano,
            repuestos: redondear_2(f.get(5).unwrap_or(0.0)),
            comision: redondear_2(mano * porcentaje / 100.0),
        });
    }
    Ok(Json(lista))
}
