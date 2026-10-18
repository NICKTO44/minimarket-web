//! Módulo VEHICULOS (migración 0039): los vehículos de los clientes por
//! placa, el kilometraje y el próximo servicio de cada venta, el historial
//! por placa y los recordatorios. Pensado para el rubro Lubricentro.
//!
//! La placa de una venta vive en su propia tabla (venta_vehiculo): una venta
//! sin placa es exactamente la de siempre. Con placa, se guarda dentro de la
//! misma transacción de la venta: entra con la venta o no entra.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET  /vehiculos?buscar=               -> lista (placa, marca, modelo, dueño)
//!   GET  /vehiculos/placa/:placa          -> el vehículo de esa placa (404 si no hay)
//!   GET  /vehiculos/recordatorios?dias=   -> a los que les toca su próximo servicio
//!   POST /vehiculos                       -> nuevo
//!   PUT  /vehiculos/:id                   -> corrige sus datos
//!   GET  /vehiculos/:id/historial         -> sus ventas, con lo que se le puso

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_VEHICULOS};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Vehículos y kilometraje";
const KM_MAXIMO: i64 = 9_999_999;

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") {
        (StatusCode::SERVICE_UNAVAILABLE, "Los vehículos aún no están listos en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}

fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, String> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(format!("{}: máximo {} letras.", campo, max)),
        otro => Ok(otro),
    }
}

/// "abc-123", "ABC 123" -> "ABC123".
pub fn clave_placa(placa: &str) -> String {
    placa.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// La placa como se muestra: en mayúsculas y, si se escribió sin guion y
/// tiene 6 letras y números (el formato de Perú), con guion en medio
/// ("abc123" -> "ABC-123"). Err si no parece una placa.
pub fn placa_valida(placa: &str) -> Result<String, String> {
    let clave = clave_placa(placa);
    if !(4..=8).contains(&clave.len()) {
        return Err("Escribe la placa del vehículo (por ejemplo ABC-123).".into());
    }
    let escrita: String = placa.trim().to_uppercase().split_whitespace().collect::<Vec<_>>().join("-");
    if escrita.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-')) {
        return Err("La placa solo lleva letras, números y guion.".into());
    }
    Ok(if !escrita.contains('-') && clave.len() == 6 { format!("{}-{}", &clave[..3], &clave[3..]) } else { escrita })
}

fn km_valido(km: Option<i64>, campo: &str) -> Result<Option<i64>, String> {
    match km {
        Some(k) if !(0..=KM_MAXIMO).contains(&k) => Err(format!("{}: escribe un kilometraje entre 0 y {}.", campo, KM_MAXIMO)),
        otro => Ok(otro),
    }
}

fn anio_valido(anio: Option<i64>) -> Result<Option<i64>, String> {
    let maximo = tiempo::hoy_lima().get(0..4).and_then(|a| a.parse::<i64>().ok()).unwrap_or(2026) + 2;
    match anio {
        Some(a) if !(1950..=maximo).contains(&a) => Err(format!("Año del vehículo no válido (de 1950 a {}).", maximo)),
        otro => Ok(otro),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatosVehiculo {
    pub placa: String,
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub anio: Option<i64>,
    #[serde(default)]
    pub motor: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub notas: Option<String>,
}

/// Revisa y ordena los datos del vehículo.
pub fn validar(d: DatosVehiculo) -> Result<DatosVehiculo, String> {
    Ok(DatosVehiculo {
        placa: placa_valida(&d.placa)?,
        cliente_id: d.cliente_id.filter(|id| *id > 0),
        marca: texto(d.marca.as_deref(), 40, "Marca")?,
        modelo: texto(d.modelo.as_deref(), 60, "Modelo")?,
        anio: anio_valido(d.anio)?,
        motor: texto(d.motor.as_deref(), 40, "Motor")?,
        color: texto(d.color.as_deref(), 30, "Color")?,
        notas: texto(d.notas.as_deref(), 300, "Notas")?,
    })
}

#[derive(Debug, Serialize)]
pub struct Vehiculo {
    pub id: i64,
    pub placa: String,
    pub cliente_id: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub cliente_telefono: Option<String>,
    pub cliente_documento: Option<String>,
    pub cliente_tipo_documento: Option<String>,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub anio: Option<i64>,
    pub motor: Option<String>,
    pub color: Option<String>,
    pub notas: Option<String>,
    pub km_ultimo: Option<i64>,
    /// Del último servicio (venta con placa).
    pub ultimo_servicio: Option<String>,
    pub proximo_km: Option<i64>,
    pub proxima_fecha: Option<String>,
    pub servicios: i64,
}

const SELECT_VEHICULO: &str = "SELECT v.id, v.placa, v.cliente_id, c.nombre_razon_social, c.telefono, v.marca, v.modelo, v.anio,
            v.motor, v.color, v.notas, v.km_ultimo,
            u.fecha, u.proximo_km, u.proxima_fecha,
            (SELECT COUNT(*) FROM venta_vehiculo x JOIN ventas vx ON vx.id = x.venta_id
              WHERE x.vehiculo_id = v.id AND COALESCE(vx.estado, 'COMPLETADA') = 'COMPLETADA'),
            c.numero_documento, c.tipo_documento
     FROM vehiculos v
     LEFT JOIN clientes c ON c.id = v.cliente_id
     LEFT JOIN venta_vehiculo u ON u.venta_id = (
        SELECT x.venta_id FROM venta_vehiculo x JOIN ventas vx ON vx.id = x.venta_id
         WHERE x.vehiculo_id = v.id AND COALESCE(vx.estado, 'COMPLETADA') = 'COMPLETADA'
         ORDER BY x.venta_id DESC LIMIT 1)";

fn vehiculo_de(f: &libsql::Row) -> Vehiculo {
    Vehiculo {
        id: f.get(0).unwrap_or_default(),
        placa: f.get(1).unwrap_or_default(),
        cliente_id: f.get(2).ok(),
        cliente_nombre: f.get(3).ok(),
        cliente_telefono: f.get(4).ok(),
        marca: f.get(5).ok(),
        modelo: f.get(6).ok(),
        anio: f.get(7).ok(),
        motor: f.get(8).ok(),
        color: f.get(9).ok(),
        notas: f.get(10).ok(),
        km_ultimo: f.get(11).ok(),
        ultimo_servicio: f.get(12).ok(),
        proximo_km: f.get(13).ok(),
        proxima_fecha: f.get(14).ok(),
        servicios: f.get(15).unwrap_or(0),
        cliente_documento: f.get(16).ok(),
        cliente_tipo_documento: f.get(17).ok(),
    }
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_VEHICULOS, NOMBRE_MODULO).await
}

async fn uno_por(conn: &libsql::Connection, donde: &str, valor: libsql::Value) -> Result<Option<Vehiculo>, Fallo> {
    let mut filas = conn
        .query(&format!("{} WHERE {} LIMIT 1", SELECT_VEHICULO, donde), libsql::params![valor])
        .await
        .map_err(sin_migracion)?;
    Ok(filas.next().await.map_err(e500)?.map(|f| vehiculo_de(&f)))
}

#[derive(Debug, Deserialize)]
pub struct Busqueda {
    #[serde(default)]
    pub buscar: Option<String>,
}

/// GET /vehiculos?buscar= — por placa, marca, modelo o dueño (los 200 más recientes).
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<Busqueda>) -> Result<Json<Vec<Vehiculo>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let buscar = q.buscar.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(|b| b.to_lowercase());
    let (filtro, valores): (&str, Vec<libsql::Value>) = match &buscar {
        Some(b) => {
            // La placa se compara sin guiones; si lo buscado no tiene letras
            // ni números, solo se busca por los demás datos.
            let placa = clave_placa(b);
            let por_placa = if placa.is_empty() { libsql::Value::Null } else { libsql::Value::Text(format!("%{}%", placa)) };
            (
                " WHERE v.activo = 1 AND (v.placa_clave LIKE ?1 OR lower(COALESCE(v.marca,'')) LIKE ?2 OR lower(COALESCE(v.modelo,'')) LIKE ?2
                     OR lower(COALESCE(c.nombre_razon_social,'')) LIKE ?2 OR COALESCE(c.numero_documento,'') LIKE ?2)",
                vec![por_placa, libsql::Value::Text(format!("%{}%", b))],
            )
        }
        None => (" WHERE v.activo = 1", vec![]),
    };
    let mut filas = conn
        .query(
            &format!("{}{} ORDER BY COALESCE(v.fecha_actualizacion, v.fecha_creacion) DESC, v.id DESC LIMIT 200", SELECT_VEHICULO, filtro),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(vehiculo_de(&f));
    }
    Ok(Json(lista))
}

/// GET /vehiculos/placa/:placa — el vehículo de esa placa.
pub async fn por_placa(Extension(tenant): Extension<Arc<TenantDb>>, Path(placa): Path<String>) -> Result<Json<Vehiculo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let clave = clave_placa(&placa);
    uno_por(&conn, "v.placa_clave = ?1", libsql::Value::Text(clave))
        .await?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "No hay un vehículo con esa placa.".into()))
}

async fn cliente_existe(conn: &libsql::Connection, cliente_id: Option<i64>) -> Result<(), Fallo> {
    let Some(id) = cliente_id else { return Ok(()) };
    let mut filas = conn.query("SELECT 1 FROM clientes WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err(malo("El cliente elegido no existe."));
    }
    Ok(())
}

/// POST /vehiculos — un vehículo nuevo (409 si la placa ya está).
pub async fn crear(Extension(tenant): Extension<Arc<TenantDb>>, Json(datos): Json<DatosVehiculo>) -> Result<Json<Vehiculo>, Fallo> {
    let d = validar(datos).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    cliente_existe(&conn, d.cliente_id).await?;
    let ahora = tiempo::ahora_lima();
    let r = conn
        .execute(
            "INSERT INTO vehiculos (placa, placa_clave, cliente_id, marca, modelo, anio, motor, color, notas, fecha_creacion, fecha_actualizacion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            libsql::params![
                d.placa.clone(),
                clave_placa(&d.placa),
                d.cliente_id,
                d.marca,
                d.modelo,
                d.anio,
                d.motor,
                d.color,
                d.notas,
                ahora
            ],
        )
        .await;
    if let Err(e) = r {
        return Err(if e.to_string().contains("UNIQUE") {
            (StatusCode::CONFLICT, format!("Ya hay un vehículo con la placa {}.", d.placa))
        } else {
            sin_migracion(e)
        });
    }
    let id = conn.last_insert_rowid();
    uno_por(&conn, "v.id = ?1", libsql::Value::Integer(id)).await?.map(Json).ok_or_else(|| e500("No se pudo leer el vehículo."))
}

/// PUT /vehiculos/:id — corrige los datos (también la placa, si no choca con otra).
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(datos): Json<DatosVehiculo>,
) -> Result<Json<Vehiculo>, Fallo> {
    let d = validar(datos).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    cliente_existe(&conn, d.cliente_id).await?;
    let r = conn
        .execute(
            "UPDATE vehiculos SET placa = ?1, placa_clave = ?2, cliente_id = ?3, marca = ?4, modelo = ?5, anio = ?6, motor = ?7,
                    color = ?8, notas = ?9, fecha_actualizacion = ?10
             WHERE id = ?11",
            libsql::params![
                d.placa.clone(),
                clave_placa(&d.placa),
                d.cliente_id,
                d.marca,
                d.modelo,
                d.anio,
                d.motor,
                d.color,
                d.notas,
                tiempo::ahora_lima(),
                id
            ],
        )
        .await;
    match r {
        Err(e) if e.to_string().contains("UNIQUE") => {
            return Err((StatusCode::CONFLICT, format!("Ya hay otro vehículo con la placa {}.", d.placa)));
        }
        Err(e) => return Err(sin_migracion(e)),
        Ok(0) => return Err((StatusCode::NOT_FOUND, "El vehículo no existe.".into())),
        Ok(_) => {}
    }
    uno_por(&conn, "v.id = ?1", libsql::Value::Integer(id)).await?.map(Json).ok_or_else(|| e500("No se pudo leer el vehículo."))
}

#[derive(Debug, Serialize)]
pub struct ItemServicio {
    pub nombre: String,
    pub cantidad: f64,
    pub unidad_medida: String,
}

#[derive(Debug, Serialize)]
pub struct Servicio {
    pub venta_id: i64,
    pub folio: String,
    pub fecha: String,
    pub total: f64,
    pub anulada: bool,
    pub km: Option<i64>,
    pub proximo_km: Option<i64>,
    pub proxima_fecha: Option<String>,
    pub observacion: Option<String>,
    pub items: Vec<ItemServicio>,
}

/// GET /vehiculos/:id/historial — sus ventas, de la más reciente a la más
/// antigua (las 100 últimas), con lo que se le puso en cada una.
pub async fn historial(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Vec<Servicio>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT vv.venta_id, v.folio, COALESCE(vv.fecha, v.fecha_hora), CAST(v.total AS REAL), COALESCE(v.estado, 'COMPLETADA'),
                    vv.km, vv.proximo_km, vv.proxima_fecha, vv.observacion
             FROM venta_vehiculo vv JOIN ventas v ON v.id = vv.venta_id
             WHERE vv.vehiculo_id = ?1 ORDER BY vv.venta_id DESC LIMIT 100",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let mut servicios: Vec<Servicio> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        servicios.push(Servicio {
            venta_id: f.get(0).unwrap_or_default(),
            folio: f.get(1).unwrap_or_default(),
            fecha: f.get(2).unwrap_or_default(),
            total: f.get(3).unwrap_or(0.0),
            anulada: f.get::<String>(4).unwrap_or_default() != "COMPLETADA",
            km: f.get(5).ok(),
            proximo_km: f.get(6).ok(),
            proxima_fecha: f.get(7).ok(),
            observacion: f.get(8).ok(),
            items: Vec::new(),
        });
    }
    drop(filas);
    if !servicios.is_empty() {
        let ids = servicios.iter().map(|s| s.venta_id.to_string()).collect::<Vec<_>>().join(",");
        let mut filas = conn
            .query(
                &format!(
                    "SELECT dv.venta_id, COALESCE(dv.nombre_producto, p.nombre), CAST(dv.cantidad AS REAL), COALESCE(dv.unidad_medida, p.unidad_medida)
                     FROM detalles_venta dv JOIN productos p ON p.id = dv.producto_id
                     WHERE dv.venta_id IN ({}) ORDER BY dv.id",
                    ids
                ),
                (),
            )
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let venta: i64 = f.get(0).unwrap_or_default();
            if let Some(s) = servicios.iter_mut().find(|s| s.venta_id == venta) {
                s.items.push(ItemServicio {
                    nombre: f.get(1).unwrap_or_default(),
                    cantidad: f.get(2).unwrap_or(0.0),
                    unidad_medida: f.get(3).unwrap_or_default(),
                });
            }
        }
    }
    Ok(Json(servicios))
}

#[derive(Debug, Deserialize)]
pub struct Recordatorios {
    /// Días hacia adelante (por defecto 7). Los ya vencidos (hasta 60 días
    /// atrás) también salen: todavía vale la pena avisarles.
    #[serde(default)]
    pub dias: Option<i64>,
}

/// GET /vehiculos/recordatorios — los vehículos cuyo próximo servicio (el
/// de su última venta) cae entre hace 60 días y dentro de `dias` días.
pub async fn recordatorios(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(q): Query<Recordatorios>,
) -> Result<Json<Vec<Vehiculo>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let dias = q.dias.unwrap_or(7).clamp(0, 90);
    let mut filas = conn
        .query(
            &format!(
                "{} WHERE v.activo = 1 AND u.proxima_fecha IS NOT NULL AND u.proxima_fecha BETWEEN ?1 AND ?2
                 ORDER BY u.proxima_fecha, v.placa LIMIT 300",
                SELECT_VEHICULO
            ),
            libsql::params![tiempo::hoy_lima_mas_dias(-60), tiempo::hoy_lima_mas_dias(dias)],
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(vehiculo_de(&f));
    }
    Ok(Json(lista))
}

// ---------------------------------------------------------------------
// En la venta
// ---------------------------------------------------------------------

/// La placa y el kilometraje que manda el punto de venta con la venta.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VehiculoVenta {
    pub placa: String,
    /// Para un vehículo nuevo (o completar uno que no los tenía).
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub anio: Option<i64>,
    #[serde(default)]
    pub km: Option<i64>,
    #[serde(default)]
    pub proximo_km: Option<i64>,
    /// "AAAA-MM-DD".
    #[serde(default)]
    pub proxima_fecha: Option<String>,
    #[serde(default)]
    pub observacion: Option<String>,
}

/// Lo que se guarda con la venta, ya revisado.
#[derive(Debug, Clone)]
pub struct VehiculoListo {
    pub placa: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub anio: Option<i64>,
    pub km: Option<i64>,
    pub proximo_km: Option<i64>,
    pub proxima_fecha: Option<String>,
    pub observacion: Option<String>,
}

/// Revisa la placa y el kilometraje antes de registrar la venta. Sin placa
/// en la venta: None (la venta de siempre). Con placa y el módulo apagado: 403.
pub fn preparar(modulos: Option<&str>, vehiculo: Option<&VehiculoVenta>) -> Result<Option<VehiculoListo>, Fallo> {
    let Some(v) = vehiculo else { return Ok(None) };
    if !modulos.is_some_and(|m| m.split(',').any(|x| x.trim() == MODULO_VEHICULOS)) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ));
    }
    revisar_venta(v).map(Some).map_err(malo)
}

/// Revisa placa, kilometraje y próximo servicio (sin mirar el módulo).
pub fn revisar_venta(v: &VehiculoVenta) -> Result<VehiculoListo, String> {
    let km = km_valido(v.km, "Kilometraje")?;
    let proximo_km = km_valido(v.proximo_km, "Próximo cambio")?;
    if let (Some(a), Some(p)) = (km, proximo_km) {
        if p <= a {
            return Err("El kilometraje del próximo cambio debe ser mayor al de hoy.".into());
        }
    }
    let proxima_fecha = v.proxima_fecha.as_deref().map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
    if let Some(f) = &proxima_fecha {
        if !tiempo::fecha_valida(f) {
            return Err("La fecha del próximo servicio no es válida.".into());
        }
        if f.as_str() <= tiempo::hoy_lima().as_str() {
            return Err("La fecha del próximo servicio debe ser después de hoy.".into());
        }
    }
    Ok(VehiculoListo {
        placa: placa_valida(&v.placa)?,
        marca: texto(v.marca.as_deref(), 40, "Marca")?,
        modelo: texto(v.modelo.as_deref(), 60, "Modelo")?,
        anio: anio_valido(v.anio)?,
        km,
        proximo_km,
        proxima_fecha,
        observacion: texto(v.observacion.as_deref(), 300, "Observación")?,
    })
}

/// Crea el vehículo de esa placa si no existe, o completa lo que le
/// faltaba (dueño, marca, modelo, año) y sube su kilometraje. Devuelve su id.
pub async fn asegurar(conn: &libsql::Connection, v: &VehiculoListo, cliente_id: Option<i64>) -> Result<i64, Fallo> {
    let ahora = tiempo::ahora_lima();
    let mut filas = conn
        .query(
            "INSERT INTO vehiculos (placa, placa_clave, cliente_id, marca, modelo, anio, km_ultimo, fecha_creacion, fecha_actualizacion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(placa_clave) DO UPDATE SET
                cliente_id = COALESCE(vehiculos.cliente_id, excluded.cliente_id),
                marca = COALESCE(vehiculos.marca, excluded.marca),
                modelo = COALESCE(vehiculos.modelo, excluded.modelo),
                anio = COALESCE(vehiculos.anio, excluded.anio),
                km_ultimo = CASE WHEN excluded.km_ultimo IS NOT NULL AND (vehiculos.km_ultimo IS NULL OR excluded.km_ultimo > vehiculos.km_ultimo)
                                 THEN excluded.km_ultimo ELSE vehiculos.km_ultimo END,
                activo = 1,
                fecha_actualizacion = excluded.fecha_actualizacion
             RETURNING id",
            libsql::params![v.placa.clone(), clave_placa(&v.placa), cliente_id, v.marca.clone(), v.modelo.clone(), v.anio, v.km, ahora],
        )
        .await
        .map_err(|e| {
            let f = sin_migracion(e);
            (f.0, format!("No se pudo guardar el vehículo: {}", f.1))
        })?;
    let id = match filas.next().await.map_err(e500)? {
        Some(f) => f.get::<i64>(0).map_err(e500)?,
        None => return Err(e500("No se pudo guardar el vehículo.")),
    };
    // La respuesta se lee hasta el final (dentro de una transacción remota).
    while filas.next().await.map_err(e500)?.is_some() {}
    Ok(id)
}

/// Dentro de la transacción de la venta: asegura el vehículo y anota la
/// placa y el kilometraje de la venta. Si falla, la venta no se registra.
pub async fn registrar(conn: &libsql::Connection, v: &VehiculoListo, venta_id: i64, cliente_id: Option<i64>) -> Result<(), Fallo> {
    let vehiculo_id = asegurar(conn, v, cliente_id).await?;
    conn.execute(
        "INSERT INTO venta_vehiculo (venta_id, vehiculo_id, placa, km, proximo_km, proxima_fecha, observacion, fecha)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        libsql::params![
            venta_id,
            vehiculo_id,
            v.placa.clone(),
            v.km,
            v.proximo_km,
            v.proxima_fecha.clone(),
            v.observacion.clone(),
            tiempo::ahora_lima()
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar la placa de la venta: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn placas() {
        assert_eq!(placa_valida("abc123"), Ok("ABC-123".into()));
        assert_eq!(placa_valida(" abc-123 "), Ok("ABC-123".into()));
        assert_eq!(placa_valida("1234-AB"), Ok("1234-AB".into()));
        assert_eq!(placa_valida("a1b 234"), Ok("A1B-234".into()));
        assert_eq!(clave_placa("A1B-234"), "A1B234");
        assert!(placa_valida("AB").is_err());
        assert!(placa_valida("ABC/123").is_err());
    }

    #[test]
    fn la_venta_con_placa() {
        let con = Some("SERVICIOS,VEHICULOS");
        assert!(preparar(con, None).unwrap().is_none());
        let v = VehiculoVenta { placa: "abc123".into(), km: Some(85000), proximo_km: Some(90000), ..Default::default() };
        let listo = preparar(con, Some(&v)).unwrap().unwrap();
        assert_eq!(listo.placa, "ABC-123");
        // Sin el módulo, una placa en la venta no entra.
        assert_eq!(preparar(Some("CREDITO"), Some(&v)).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(preparar(None, Some(&v)).unwrap_err().0, StatusCode::FORBIDDEN);
        let mal = VehiculoVenta { proximo_km: Some(80000), ..v.clone() };
        assert!(preparar(con, Some(&mal)).is_err());
        let ayer = VehiculoVenta { proxima_fecha: Some(tiempo::hoy_lima_mas_dias(-1)), ..v.clone() };
        assert!(preparar(con, Some(&ayer)).is_err());
        let luego = VehiculoVenta { proxima_fecha: Some(tiempo::hoy_lima_mas_dias(90)), ..v };
        assert!(preparar(con, Some(&luego)).is_ok());
    }
}
