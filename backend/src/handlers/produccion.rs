//! Módulos PRODUCCION e INSUMOS (migración 0043). Pensados para el rubro
//! Pastelería / Panadería, pero los puede encender cualquier negocio que
//! fabrique lo que vende.
//!
//! PRODUCCION ("Producción y mermas"):
//!   - Producción del día: "salieron 200 panes". El stock del producto sube
//!     sin hacerlo como una compra (si lleva vencimiento, entra como un lote
//!     nuevo con su fecha).
//!   - Mermas: lo que no se vendió, se malogró o venció. El stock baja y queda
//!     el costo de lo perdido.
//!   - Resumen del periodo por producto: producido, vendido y merma.
//!
//! INSUMOS ("Recetas e insumos", necesita PRODUCCION):
//!   - Receta de producción de cada producto: cuánto rinde y qué insumos
//!     lleva (los insumos son productos normales: harina, huevos...).
//!   - Costo de hacer cada producto (con el costo promedio o el precio de
//!     compra de cada insumo) y su margen frente al precio de venta.
//!   - Al registrar una producción con receta se descuentan los insumos.
//!
//! Todo movimiento de stock queda en movimientos_inventario (ENTRADA,
//! SALIDA o MERMA). Una producción o una merma se pueden anular (solo el
//! administrador): el stock vuelve a como estaba.
//!
//! Rutas (con sesión, y solo con su módulo encendido):
//!   GET  /produccion?desde=&hasta=          POST /produccion
//!   POST /produccion/:id/anular             GET  /produccion/resumen?desde=&hasta=
//!   GET  /mermas?desde=&hasta=              POST /mermas
//!   POST /mermas/:id/anular
//!   GET  /formulas                          GET/PUT/DELETE /formulas/:producto_id

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::ganancias::promediar;
use crate::handlers::rubros::{exigir_modulo, negocio, MODULO_INSUMOS, MODULO_PRODUCCION};
use crate::logica::tiempo;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_PRODUCCION: &str = "Producción y mermas";
const NOMBRE_INSUMOS: &str = "Recetas e insumos";
const MAX_INSUMOS: usize = 60;
const MAX_CANTIDAD: f64 = 1_000_000.0;
const MOTIVOS_MERMA: &[&str] = &["NO_VENDIDO", "MALOGRADO", "VENCIDO", "OTRO"];

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") || texto.contains("no such column") {
        (StatusCode::SERVICE_UNAVAILABLE, "La producción aún no está lista en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, Fallo> {
    let limpio = valor.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(malo(format!("{}: máximo {} letras.", campo, max))),
        otro => Ok(otro),
    }
}
fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}
fn r4(n: f64) -> f64 {
    (n * 10_000.0).round() / 10_000.0
}
/// Cantidad legible en los mensajes ("2.5", "200").
fn cant(n: f64) -> String {
    let t = format!("{:.3}", n);
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}
fn cantidad_valida(c: f64) -> bool {
    c > 0.0 && c.is_finite() && c <= MAX_CANTIDAD
}

/// ¿Tiene encendidas las recetas de producción (módulo INSUMOS)?
async fn con_insumos(conn: &libsql::Connection) -> bool {
    negocio(conn).await.modulos.iter().any(|m| m == MODULO_INSUMOS)
}

/// Corre el cuerpo en una transacción: o se guarda todo o nada.
macro_rules! en_transaccion {
    ($conn:expr, |$tx:ident| $cuerpo:block) => {{
        let tx = $conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
        let resultado: Result<_, Fallo> = async {
            let $tx: &libsql::Connection = &tx;
            $cuerpo
        }
        .await;
        match resultado {
            Ok(v) => {
                tx.commit().await.map_err(e500)?;
                v
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }
    }};
}

// ---------------------------------------------------------------------
// Productos y stock
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DatosProducto {
    id: i64,
    nombre: String,
    unidad: String,
    lleva_vencimiento: bool,
    controla_stock: bool,
    stock: f64,
    /// Costo de una unidad: el promedio o, si no hay, el precio de compra.
    costo: Option<f64>,
    /// Solo el costo promedio guardado (para promediar al producir).
    costo_promedio: Option<f64>,
    precio: f64,
    activo: bool,
}

async fn datos_productos(conn: &libsql::Connection, ids: &[i64]) -> Result<HashMap<i64, DatosProducto>, Fallo> {
    let mut mapa = HashMap::new();
    if ids.is_empty() {
        return Ok(mapa);
    }
    let lista = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(
            &format!(
                "SELECT id, nombre, COALESCE(unidad_medida, 'UNIDAD'), COALESCE(lleva_vencimiento, 0), COALESCE(controla_stock, 1),
                        CAST(stock AS REAL),
                        CAST(CASE WHEN COALESCE(costo_promedio, 0) > 0 THEN costo_promedio
                                  WHEN COALESCE(precio_compra, 0) > 0 THEN precio_compra END AS REAL),
                        CAST(CASE WHEN COALESCE(costo_promedio, 0) > 0 THEN costo_promedio END AS REAL),
                        CAST(precio AS REAL), COALESCE(activo, 1)
                 FROM productos WHERE id IN ({})",
                lista
            ),
            (),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        mapa.insert(
            id,
            DatosProducto {
                id,
                nombre: f.get(1).unwrap_or_default(),
                unidad: f.get(2).unwrap_or_else(|_| "UNIDAD".to_string()),
                lleva_vencimiento: f.get::<i64>(3).unwrap_or(0) == 1,
                controla_stock: f.get::<i64>(4).unwrap_or(1) == 1,
                stock: f.get(5).unwrap_or(0.0),
                costo: f.get::<f64>(6).ok().filter(|c| *c > 0.0),
                costo_promedio: f.get::<f64>(7).ok().filter(|c| *c > 0.0),
                precio: f.get(8).unwrap_or(0.0),
                activo: f.get::<i64>(9).unwrap_or(1) == 1,
            },
        );
    }
    Ok(mapa)
}

async fn stock_de(conn: &libsql::Connection, producto_id: i64) -> Result<f64, Fallo> {
    let mut f = conn.query("SELECT CAST(stock AS REAL) FROM productos WHERE id = ?1", libsql::params![producto_id]).await.map_err(e500)?;
    Ok(f.next().await.map_err(e500)?.and_then(|f| f.get::<f64>(0).ok()).unwrap_or(0.0))
}

/// Lo que hay para descontar: con vencimiento, la suma de sus lotes (sin los
/// vencidos, salvo `incluir_vencidos`); si no, su stock.
async fn disponible(conn: &libsql::Connection, p: &DatosProducto, incluir_vencidos: bool) -> Result<f64, Fallo> {
    if !p.lleva_vencimiento {
        return stock_de(conn, p.id).await;
    }
    let mut f = conn
        .query(
            "SELECT CAST(COALESCE(SUM(cantidad), 0) AS REAL) FROM lotes_producto
             WHERE producto_id = ?1 AND activo = 1 AND cantidad > 0 AND (?2 = 1 OR date(fecha_vencimiento) >= ?3)",
            libsql::params![p.id, incluir_vencidos as i64, tiempo::hoy_lima()],
        )
        .await
        .map_err(e500)?;
    Ok(f.next().await.map_err(e500)?.and_then(|f| f.get::<f64>(0).ok()).unwrap_or(0.0))
}

/// Saca `cantidad` del stock de un producto (dentro de la transacción de
/// quien llama). Con vencimiento, por lotes del que vence antes (FEFO).
/// `parcial`: si no alcanza, saca lo que hay; si no, falla.
/// Devuelve (lo que se sacó, de qué lotes "id:cantidad,...").
async fn sacar(
    conn: &libsql::Connection,
    p: &DatosProducto,
    cantidad: f64,
    incluir_vencidos: bool,
    parcial: bool,
) -> Result<(f64, Option<String>), Fallo> {
    if !p.controla_stock {
        return Ok((0.0, None));
    }
    let hay = disponible(conn, p, incluir_vencidos).await?;
    if hay + 1e-6 < cantidad && !parcial {
        return Err((
            StatusCode::CONFLICT,
            format!("No alcanza el stock de {}: hay {} {} y se necesitan {}.", p.nombre, cant(hay.max(0.0)), p.unidad.to_lowercase(), cant(cantidad)),
        ));
    }
    let sacar = cantidad.min(hay.max(0.0));
    if sacar <= 1e-9 {
        return Ok((0.0, None));
    }
    if !p.lleva_vencimiento {
        conn.execute(
            "UPDATE productos SET stock = MAX(stock - ?1, 0), fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
            libsql::params![sacar, p.id],
        )
        .await
        .map_err(e500)?;
        return Ok((sacar, None));
    }
    let mut filas = conn
        .query(
            "SELECT id, CAST(cantidad AS REAL) FROM lotes_producto
             WHERE producto_id = ?1 AND activo = 1 AND cantidad > 0 AND (?2 = 1 OR date(fecha_vencimiento) >= ?3)
             ORDER BY fecha_vencimiento ASC, id ASC",
            libsql::params![p.id, incluir_vencidos as i64, tiempo::hoy_lima()],
        )
        .await
        .map_err(e500)?;
    let mut lotes: Vec<(i64, f64)> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lotes.push((f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0)));
    }
    drop(filas);
    let mut restante = sacar;
    let mut usados: Vec<(i64, f64)> = Vec::new();
    for (lote, hay_lote) in lotes {
        if restante <= 1e-9 {
            break;
        }
        let x = hay_lote.min(restante);
        conn.execute("UPDATE lotes_producto SET cantidad = MAX(cantidad - ?1, 0) WHERE id = ?2", libsql::params![x, lote])
            .await
            .map_err(e500)?;
        usados.push((lote, x));
        restante -= x;
    }
    let texto = usados.iter().map(|(l, c)| format!("{}:{}", l, c)).collect::<Vec<_>>().join(",");
    Ok((sacar - restante.max(0.0), Some(texto)))
}

/// Devuelve al stock lo que salió con `sacar` (al anular).
async fn devolver(conn: &libsql::Connection, producto_id: i64, cantidad: f64, lotes: Option<&str>) -> Result<(), Fallo> {
    if cantidad <= 1e-9 {
        return Ok(());
    }
    match lotes.filter(|l| !l.is_empty()) {
        Some(lista) => {
            for par in lista.split(',') {
                let mut partes = par.split(':');
                let (Some(lote), Some(c)) = (partes.next().and_then(|x| x.parse::<i64>().ok()), partes.next().and_then(|x| x.parse::<f64>().ok())) else {
                    continue;
                };
                conn.execute(
                    "UPDATE lotes_producto SET cantidad = cantidad + ?1, activo = 1 WHERE id = ?2 AND producto_id = ?3",
                    libsql::params![c, lote, producto_id],
                )
                .await
                .map_err(e500)?;
            }
        }
        None => {
            conn.execute(
                "UPDATE productos SET stock = stock + ?1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
                libsql::params![cantidad, producto_id],
            )
            .await
            .map_err(e500)?;
        }
    }
    Ok(())
}

/// Un movimiento en el kárdex (movimientos_inventario). El stock nuevo es
/// el que quedó (lo recalculan los triggers de lotes).
async fn movimiento(
    conn: &libsql::Connection,
    producto_id: i64,
    tipo: &str,
    cantidad: f64,
    stock_antes: f64,
    usuario_id: i64,
    referencia: &str,
    motivo: &str,
) -> Result<(), Fallo> {
    if cantidad <= 1e-9 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo, usuario_id, referencia, motivo)
         VALUES (?1, ?2, ?3, ?4, (SELECT stock FROM productos WHERE id = ?1), ?5, ?6, ?7)",
        libsql::params![producto_id, tipo, cantidad, stock_antes, usuario_id, referencia, motivo],
    )
    .await
    .map_err(e500)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Rango {
    #[serde(default)]
    pub desde: Option<String>,
    #[serde(default)]
    pub hasta: Option<String>,
}

/// Sin fechas: hoy (hora de Lima).
fn rango(q: &Rango) -> Result<(String, String), Fallo> {
    let hoy = tiempo::hoy_lima();
    let limpio = |v: &Option<String>| v.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let desde = limpio(&q.desde).unwrap_or_else(|| hoy.clone());
    let hasta = limpio(&q.hasta).unwrap_or_else(|| desde.clone().max(hoy.clone()));
    if !tiempo::fecha_valida(&desde) || !tiempo::fecha_valida(&hasta) {
        return Err(malo("Fecha no válida."));
    }
    if desde > hasta {
        return Err(malo("La fecha inicial es posterior a la final."));
    }
    Ok((desde, hasta))
}

// ---------------------------------------------------------------------
// Recetas de producción (módulo INSUMOS)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct InsumoFormula {
    pub insumo_id: i64,
    pub nombre: String,
    pub unidad_medida: String,
    pub cantidad: f64,
    pub costo_unitario: Option<f64>,
    pub costo: Option<f64>,
    pub stock: f64,
}

#[derive(Debug, Serialize)]
pub struct Formula {
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub precio: f64,
    pub rinde: f64,
    pub notas: Option<String>,
    pub insumos: Vec<InsumoFormula>,
    /// Costo de toda la receta (None si a algún insumo le falta el costo).
    pub costo_total: Option<f64>,
    /// Costo de una unidad del producto.
    pub costo_unitario: Option<f64>,
    /// Precio − costo de una unidad, y en % del precio.
    pub margen: Option<f64>,
    pub margen_porcentaje: Option<f64>,
    /// Insumos sin costo (sin precio de compra): no se puede calcular.
    pub sin_costo: Vec<String>,
}

/// Recetas de los productos pedidos (todas si `solo` es None).
async fn leer_formulas(conn: &libsql::Connection, solo: Option<i64>) -> Result<Vec<Formula>, Fallo> {
    let mut filas = match solo {
        Some(id) => conn.query("SELECT producto_id, CAST(rinde AS REAL), notas FROM formulas WHERE producto_id = ?1", libsql::params![id]).await,
        None => conn.query("SELECT producto_id, CAST(rinde AS REAL), notas FROM formulas", ()).await,
    }
    .map_err(sin_migracion)?;
    let mut base: Vec<(i64, f64, Option<String>)> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        base.push((f.get(0).unwrap_or_default(), f.get(1).unwrap_or(1.0), f.get(2).ok()));
    }
    drop(filas);
    if base.is_empty() {
        return Ok(Vec::new());
    }
    let ids = base.iter().map(|b| b.0.to_string()).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(
            &format!("SELECT producto_id, insumo_id, CAST(cantidad AS REAL) FROM formula_insumos WHERE producto_id IN ({}) ORDER BY id", ids),
            (),
        )
        .await
        .map_err(e500)?;
    let mut lineas: HashMap<i64, Vec<(i64, f64)>> = HashMap::new();
    let mut todos: Vec<i64> = base.iter().map(|b| b.0).collect();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let insumo: i64 = f.get(1).unwrap_or_default();
        todos.push(insumo);
        lineas.entry(f.get(0).unwrap_or_default()).or_default().push((insumo, f.get(2).unwrap_or(0.0)));
    }
    drop(filas);
    todos.sort_unstable();
    todos.dedup();
    let datos = datos_productos(conn, &todos).await?;
    let mut lista = Vec::new();
    for (producto_id, rinde, notas) in base {
        let Some(p) = datos.get(&producto_id) else { continue };
        let mut insumos = Vec::new();
        let mut sin_costo = Vec::new();
        let mut total = 0.0;
        for (insumo_id, cantidad) in lineas.remove(&producto_id).unwrap_or_default() {
            let Some(i) = datos.get(&insumo_id) else { continue };
            if i.costo.is_none() {
                sin_costo.push(i.nombre.clone());
            }
            let costo = i.costo.map(|c| r4(c * cantidad));
            total += costo.unwrap_or(0.0);
            insumos.push(InsumoFormula {
                insumo_id,
                nombre: i.nombre.clone(),
                unidad_medida: i.unidad.clone(),
                cantidad,
                costo_unitario: i.costo,
                costo,
                stock: i.stock,
            });
        }
        let costo_total = if sin_costo.is_empty() && !insumos.is_empty() { Some(r2(total)) } else { None };
        let costo_unitario = costo_total.map(|t| r4(t / rinde));
        let margen = costo_unitario.map(|c| r2(p.precio - c));
        let margen_porcentaje = margen.filter(|_| p.precio > 0.0).map(|m| r2(m / p.precio * 100.0));
        lista.push(Formula {
            producto_id,
            producto_nombre: p.nombre.clone(),
            unidad_medida: p.unidad.clone(),
            precio: p.precio,
            rinde,
            notas,
            insumos,
            costo_total,
            costo_unitario,
            margen,
            margen_porcentaje,
            sin_costo,
        });
    }
    lista.sort_by(|a, b| a.producto_nombre.to_lowercase().cmp(&b.producto_nombre.to_lowercase()));
    Ok(lista)
}

/// GET /formulas — todas las recetas de producción con su costo.
pub async fn formulas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Formula>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_INSUMOS, NOMBRE_INSUMOS).await?;
    Ok(Json(leer_formulas(&conn, None).await?))
}

/// GET /formulas/:producto_id
pub async fn formula(Extension(tenant): Extension<Arc<TenantDb>>, Path(producto_id): Path<i64>) -> Result<Json<Formula>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_INSUMOS, NOMBRE_INSUMOS).await?;
    leer_formulas(&conn, Some(producto_id))
        .await?
        .pop()
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Ese producto aún no tiene receta.".into()))
}

#[derive(Debug, Deserialize)]
pub struct LineaFormula {
    pub insumo_id: i64,
    pub cantidad: f64,
}

#[derive(Debug, Deserialize)]
pub struct DatosFormula {
    pub rinde: f64,
    #[serde(default)]
    pub notas: Option<String>,
    pub insumos: Vec<LineaFormula>,
}

/// PUT /formulas/:producto_id — crea o reemplaza la receta. Solo el
/// administrador (muestra costos).
pub async fn guardar_formula(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(producto_id): Path<i64>,
    Json(d): Json<DatosFormula>,
) -> Result<Json<Formula>, Fallo> {
    exigir_admin(&claims)?;
    if !cantidad_valida(d.rinde) {
        return Err(malo("Escribe cuánto rinde la receta (mayor a 0)."));
    }
    let notas = texto(d.notas.as_deref(), 1000, "Notas")?;
    if d.insumos.is_empty() {
        return Err(malo("Agrega los insumos que lleva."));
    }
    if d.insumos.len() > MAX_INSUMOS {
        return Err(malo(format!("Una receta puede llevar hasta {} insumos.", MAX_INSUMOS)));
    }
    // El mismo insumo dos veces se suma.
    let mut juntos: Vec<(i64, f64)> = Vec::new();
    for l in &d.insumos {
        if !cantidad_valida(l.cantidad) {
            return Err(malo("Revisa las cantidades de los insumos (mayores a 0)."));
        }
        if l.insumo_id == producto_id {
            return Err(malo("Un producto no puede ser insumo de sí mismo."));
        }
        match juntos.iter_mut().find(|(i, _)| *i == l.insumo_id) {
            Some(x) => x.1 += l.cantidad,
            None => juntos.push((l.insumo_id, l.cantidad)),
        }
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_INSUMOS, NOMBRE_INSUMOS).await?;
    let mut ids: Vec<i64> = juntos.iter().map(|(i, _)| *i).collect();
    ids.push(producto_id);
    let datos = datos_productos(&conn, &ids).await?;
    if !datos.get(&producto_id).is_some_and(|p| p.activo) {
        return Err((StatusCode::NOT_FOUND, "El producto no existe o está desactivado.".into()));
    }
    if juntos.iter().any(|(i, _)| !datos.contains_key(i)) {
        return Err(malo("Un insumo de la receta no existe. Quítalo y vuelve a agregarlo."));
    }
    en_transaccion!(conn, |tx| {
        tx.execute(
            "INSERT INTO formulas (producto_id, rinde, notas, fecha_actualizacion) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(producto_id) DO UPDATE SET rinde = excluded.rinde, notas = excluded.notas, fecha_actualizacion = excluded.fecha_actualizacion",
            libsql::params![producto_id, d.rinde, notas.clone(), tiempo::ahora_lima()],
        )
        .await
        .map_err(sin_migracion)?;
        tx.execute("DELETE FROM formula_insumos WHERE producto_id = ?1", libsql::params![producto_id]).await.map_err(e500)?;
        for (insumo, cantidad) in &juntos {
            tx.execute(
                "INSERT INTO formula_insumos (producto_id, insumo_id, cantidad) VALUES (?1, ?2, ?3)",
                libsql::params![producto_id, *insumo, *cantidad],
            )
            .await
            .map_err(e500)?;
        }
        Ok(())
    });
    leer_formulas(&conn, Some(producto_id)).await?.pop().map(Json).ok_or_else(|| e500("No se pudo leer la receta."))
}

/// DELETE /formulas/:producto_id — solo el administrador.
pub async fn borrar_formula(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(producto_id): Path<i64>,
) -> Result<StatusCode, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_INSUMOS, NOMBRE_INSUMOS).await?;
    en_transaccion!(conn, |tx| {
        tx.execute("DELETE FROM formula_insumos WHERE producto_id = ?1", libsql::params![producto_id]).await.map_err(sin_migracion)?;
        tx.execute("DELETE FROM formulas WHERE producto_id = ?1", libsql::params![producto_id]).await.map_err(e500)?;
        Ok(())
    });
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------
// Producción
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct InsumoUsado {
    pub insumo_id: i64,
    pub nombre: String,
    pub unidad_medida: String,
    pub cantidad: f64,
    pub faltante: f64,
    pub costo_unitario: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Produccion {
    pub id: i64,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub cantidad: f64,
    pub fecha_vencimiento: Option<String>,
    pub con_receta: bool,
    pub costo_unitario: Option<f64>,
    pub costo_total: Option<f64>,
    pub nota: Option<String>,
    pub estado: String,
    pub usuario_nombre: Option<String>,
    pub fecha_hora: String,
    pub anulada_por: Option<String>,
    pub insumos: Vec<InsumoUsado>,
}

async fn leer_producciones(conn: &libsql::Connection, donde: &str, valores: Vec<libsql::Value>) -> Result<Vec<Produccion>, Fallo> {
    let mut filas = conn
        .query(
            &format!(
                "SELECT id, producto_id, producto_nombre, COALESCE(unidad_medida, 'UNIDAD'), CAST(cantidad AS REAL), fecha_vencimiento, con_receta,
                        CAST(costo_unitario AS REAL), CAST(costo_total AS REAL), nota, estado, usuario_nombre, fecha_hora, anulada_por
                 FROM producciones WHERE {} ORDER BY fecha_hora DESC, id DESC LIMIT 500",
                donde
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(Produccion {
            id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            producto_nombre: f.get(2).unwrap_or_default(),
            unidad_medida: f.get(3).unwrap_or_default(),
            cantidad: f.get(4).unwrap_or(0.0),
            fecha_vencimiento: f.get(5).ok(),
            con_receta: f.get::<i64>(6).unwrap_or(0) == 1,
            costo_unitario: f.get(7).ok(),
            costo_total: f.get(8).ok(),
            nota: f.get(9).ok(),
            estado: f.get(10).unwrap_or_default(),
            usuario_nombre: f.get(11).ok(),
            fecha_hora: f.get(12).unwrap_or_default(),
            anulada_por: f.get(13).ok(),
            insumos: Vec::new(),
        });
    }
    drop(filas);
    if lista.is_empty() {
        return Ok(lista);
    }
    let ids = lista.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(
            &format!(
                "SELECT produccion_id, insumo_id, insumo_nombre, COALESCE(unidad_medida, 'UNIDAD'), CAST(cantidad AS REAL), CAST(faltante AS REAL),
                        CAST(costo_unitario AS REAL)
                 FROM produccion_insumos WHERE produccion_id IN ({}) ORDER BY id",
                ids
            ),
            (),
        )
        .await
        .map_err(e500)?;
    let mut por: HashMap<i64, Vec<InsumoUsado>> = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        por.entry(f.get(0).unwrap_or_default()).or_default().push(InsumoUsado {
            insumo_id: f.get(1).unwrap_or_default(),
            nombre: f.get(2).unwrap_or_default(),
            unidad_medida: f.get(3).unwrap_or_default(),
            cantidad: f.get(4).unwrap_or(0.0),
            faltante: f.get(5).unwrap_or(0.0),
            costo_unitario: f.get(6).ok(),
        });
    }
    for p in lista.iter_mut() {
        p.insumos = por.remove(&p.id).unwrap_or_default();
    }
    Ok(lista)
}

/// GET /produccion?desde=&hasta= — lo producido (por defecto hoy).
pub async fn listar_produccion(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<Rango>) -> Result<Json<Vec<Produccion>>, Fallo> {
    let (desde, hasta) = rango(&q)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    Ok(Json(leer_producciones(&conn, "substr(fecha_hora, 1, 10) BETWEEN ?1 AND ?2", vec![desde.into(), hasta.into()]).await?))
}

#[derive(Debug, Deserialize)]
pub struct NuevaProduccion {
    pub producto_id: i64,
    pub cantidad: f64,
    /// Obligatoria si el producto lleva vencimiento: entra como lote.
    #[serde(default)]
    pub fecha_vencimiento: Option<String>,
    #[serde(default)]
    pub nota: Option<String>,
    /// Descontar los insumos de su receta (si tiene y está encendido el módulo).
    #[serde(default = "verdadero")]
    pub usar_receta: bool,
    /// Registrar aunque no alcance algún insumo (se descuenta lo que haya).
    #[serde(default)]
    pub forzar: bool,
}
fn verdadero() -> bool {
    true
}

/// POST /produccion — registra lo producido: sube el stock del producto y,
/// con receta, descuenta sus insumos y calcula el costo.
pub async fn registrar_produccion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(n): Json<NuevaProduccion>,
) -> Result<Json<Produccion>, Fallo> {
    if !cantidad_valida(n.cantidad) {
        return Err(malo("Escribe cuánto se produjo (mayor a 0)."));
    }
    let nota = texto(n.nota.as_deref(), 300, "Nota")?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    let producto = datos_productos(&conn, &[n.producto_id])
        .await?
        .remove(&n.producto_id)
        .filter(|p| p.activo)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "El producto no existe o está desactivado.".to_string()))?;
    if !producto.controla_stock {
        return Err(malo(format!(
            "{} se vende sin controlar stock: actívale el control de stock en Productos para registrar su producción.",
            producto.nombre
        )));
    }
    let vencimiento = if producto.lleva_vencimiento {
        let f = n.fecha_vencimiento.as_deref().map(str::trim).unwrap_or("").to_string();
        if !tiempo::fecha_valida(&f) {
            return Err(malo(format!("{} lleva vencimiento: elige hasta cuándo dura lo producido.", producto.nombre)));
        }
        if f < tiempo::hoy_lima() {
            return Err(malo("La fecha de vencimiento ya pasó."));
        }
        Some(f)
    } else {
        None
    };

    // La receta: lo que se necesita de cada insumo para esta cantidad.
    let receta = if n.usar_receta && con_insumos(&conn).await { leer_formulas(&conn, Some(producto.id)).await?.pop() } else { None };
    let necesidades: Vec<(i64, f64)> = receta
        .as_ref()
        .map(|r| r.insumos.iter().map(|i| (i.insumo_id, r4(i.cantidad * n.cantidad / r.rinde))).collect())
        .unwrap_or_default();
    let insumos = datos_productos(&conn, &necesidades.iter().map(|(i, _)| *i).collect::<Vec<_>>()).await?;

    // Antes de tocar nada: ¿alcanzan los insumos?
    if !n.forzar {
        let mut faltan = Vec::new();
        for (id, necesita) in &necesidades {
            let Some(i) = insumos.get(id).filter(|i| i.controla_stock) else { continue };
            let hay = disponible(&conn, i, false).await?;
            if hay + 1e-6 < *necesita {
                faltan.push(format!("{} (hay {}, se necesitan {} {})", i.nombre, cant(hay.max(0.0)), cant(*necesita), i.unidad.to_lowercase()));
            }
        }
        if !faltan.is_empty() {
            return Err((
                StatusCode::CONFLICT,
                format!("No alcanza el stock de: {}. Registra la compra de esos insumos o registra la producción igual.", faltan.join("; ")),
            ));
        }
    }

    // Costo: el de los insumos de la receta (si todos tienen costo).
    let todos_con_costo = necesidades.iter().all(|(id, _)| insumos.get(id).and_then(|i| i.costo).is_some());
    let costo_total = (receta.is_some() && !necesidades.is_empty() && todos_con_costo).then(|| {
        r2(necesidades.iter().map(|(id, c)| c * insumos.get(id).and_then(|i| i.costo).unwrap_or(0.0)).sum::<f64>())
    });
    let costo_unitario = costo_total.map(|t| r4(t / n.cantidad));
    let ahora = tiempo::ahora_lima();

    let id = en_transaccion!(conn, |tx| {
        tx.execute(
            "INSERT INTO producciones (producto_id, producto_nombre, unidad_medida, cantidad, fecha_vencimiento, con_receta, costo_unitario,
                                       costo_total, nota, estado, usuario_id, usuario_nombre, fecha_hora)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'REGISTRADA', ?10, ?11, ?12)",
            libsql::params![
                producto.id, producto.nombre.clone(), producto.unidad.clone(), n.cantidad, vencimiento.clone(), receta.is_some() as i64,
                costo_unitario, costo_total, nota.clone(), claims.sub, claims.nombre_completo.clone(), ahora.clone()
            ],
        )
        .await
        .map_err(sin_migracion)?;
        let id = tx.last_insert_rowid();
        let referencia = format!("Producción N.° {}", id);

        // Insumos: salen de su stock (los vencidos no se usan).
        for (insumo_id, necesita) in &necesidades {
            let Some(i) = insumos.get(insumo_id) else { continue };
            let antes = stock_de(tx, i.id).await?;
            let (salio, lotes) = sacar(tx, i, *necesita, false, n.forzar).await?;
            let faltante = if i.controla_stock { r4((necesita - salio).max(0.0)) } else { 0.0 };
            tx.execute(
                "INSERT INTO produccion_insumos (produccion_id, insumo_id, insumo_nombre, unidad_medida, cantidad, faltante, costo_unitario, lotes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                libsql::params![id, i.id, i.nombre.clone(), i.unidad.clone(), r4(salio), faltante, i.costo, lotes],
            )
            .await
            .map_err(e500)?;
            movimiento(tx, i.id, "SALIDA", salio, antes, claims.sub, &referencia, &format!("Insumo para {}", producto.nombre)).await?;
        }

        // El producto entra a su stock (con vencimiento, como un lote nuevo).
        let antes = stock_de(tx, producto.id).await?;
        if let Some(fecha) = &vencimiento {
            tx.execute(
                "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, numero_lote) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![producto.id, n.cantidad, fecha.clone(), format!("PROD-{}", id)],
            )
            .await
            .map_err(e500)?;
            let lote = tx.last_insert_rowid();
            tx.execute("UPDATE producciones SET lote_id = ?1 WHERE id = ?2", libsql::params![lote, id]).await.map_err(e500)?;
        } else {
            tx.execute(
                "UPDATE productos SET stock = stock + ?1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
                libsql::params![n.cantidad, producto.id],
            )
            .await
            .map_err(e500)?;
        }
        movimiento(tx, producto.id, "ENTRADA", n.cantidad, antes, claims.sub, &referencia, "Producción").await?;

        // Costo promedio del producto con lo que costó esta producción.
        if let Some(costo) = costo_unitario.filter(|c| *c > 0.0) {
            let nuevo = promediar(antes, producto.costo_promedio, n.cantidad, costo);
            if nuevo.is_finite() && nuevo > 0.0 {
                tx.execute("UPDATE productos SET costo_promedio = ?1 WHERE id = ?2", libsql::params![r4(nuevo), producto.id])
                    .await
                    .map_err(e500)?;
            }
        }
        Ok(id)
    });
    leer_producciones(&conn, "id = ?1", vec![id.into()]).await?.pop().map(Json).ok_or_else(|| e500("No se pudo leer la producción."))
}

/// POST /produccion/:id/anular — solo el administrador. El producto sale
/// del stock (si ya se vendió, no se puede: se registra una merma) y los
/// insumos vuelven a su stock.
pub async fn anular_produccion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<Produccion>, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    let mut f = conn
        .query(
            "SELECT producto_id, CAST(cantidad AS REAL), lote_id, estado, producto_nombre FROM producciones WHERE id = ?1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let (producto_id, cantidad, lote_id, estado, nombre): (i64, f64, Option<i64>, String, String) = match f.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0), f.get(2).ok(), f.get(3).unwrap_or_default(), f.get(4).unwrap_or_default()),
        None => return Err((StatusCode::NOT_FOUND, "Esa producción no existe.".into())),
    };
    drop(f);
    if estado != "REGISTRADA" {
        return Err((StatusCode::CONFLICT, "Esa producción ya está anulada.".into()));
    }
    let mut f = conn
        .query("SELECT insumo_id, CAST(cantidad AS REAL), lotes FROM produccion_insumos WHERE produccion_id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    let mut usados: Vec<(i64, f64, Option<String>)> = Vec::new();
    while let Some(x) = f.next().await.map_err(e500)? {
        usados.push((x.get(0).unwrap_or_default(), x.get(1).unwrap_or(0.0), x.get(2).ok()));
    }
    drop(f);
    let referencia = format!("Anulación de producción N.° {}", id);
    en_transaccion!(conn, |tx| {
        let antes = stock_de(tx, producto_id).await?;
        let n = match lote_id {
            Some(lote) => {
                tx.execute(
                    "UPDATE lotes_producto SET cantidad = cantidad - ?1 WHERE id = ?2 AND activo = 1 AND cantidad >= ?1 - 0.000001",
                    libsql::params![cantidad, lote],
                )
                .await
                .map_err(e500)?
            }
            None => tx
                .execute(
                    "UPDATE productos SET stock = MAX(stock - ?1, 0) WHERE id = ?2 AND stock >= ?1 - 0.000001",
                    libsql::params![cantidad, producto_id],
                )
                .await
                .map_err(e500)?,
        };
        if n == 0 {
            return Err((
                StatusCode::CONFLICT,
                format!("Ya se vendió o se usó parte de lo producido de {}: no se puede anular. Si se perdió, regístralo como merma.", nombre),
            ));
        }
        movimiento(tx, producto_id, "SALIDA", cantidad, antes, claims.sub, &referencia, "Producción anulada").await?;
        for (insumo, c, lotes) in &usados {
            let antes = stock_de(tx, *insumo).await?;
            devolver(tx, *insumo, *c, lotes.as_deref()).await?;
            movimiento(tx, *insumo, "ENTRADA", *c, antes, claims.sub, &referencia, "Insumo devuelto").await?;
        }
        let n = tx
            .execute(
                "UPDATE producciones SET estado = 'ANULADA', anulada_por = ?1, fecha_anulacion = ?2 WHERE id = ?3 AND estado = 'REGISTRADA'",
                libsql::params![claims.nombre_completo.clone(), tiempo::ahora_lima(), id],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            return Err((StatusCode::CONFLICT, "Esa producción ya está anulada.".into()));
        }
        Ok(())
    });
    leer_producciones(&conn, "id = ?1", vec![id.into()]).await?.pop().map(Json).ok_or_else(|| e500("No se pudo leer la producción."))
}

// ---------------------------------------------------------------------
// Mermas
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Merma {
    pub id: i64,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub cantidad: f64,
    pub motivo: String,
    pub nota: Option<String>,
    pub costo_unitario: Option<f64>,
    pub costo_total: Option<f64>,
    pub estado: String,
    pub usuario_nombre: Option<String>,
    pub fecha_hora: String,
    pub anulada_por: Option<String>,
}

async fn leer_mermas(conn: &libsql::Connection, donde: &str, valores: Vec<libsql::Value>) -> Result<Vec<Merma>, Fallo> {
    let mut filas = conn
        .query(
            &format!(
                "SELECT id, producto_id, producto_nombre, COALESCE(unidad_medida, 'UNIDAD'), CAST(cantidad AS REAL), motivo, nota,
                        CAST(costo_unitario AS REAL), estado, usuario_nombre, fecha_hora, anulada_por
                 FROM mermas_registradas WHERE {} ORDER BY fecha_hora DESC, id DESC LIMIT 500",
                donde
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(4).unwrap_or(0.0);
        let costo: Option<f64> = f.get(7).ok();
        lista.push(Merma {
            id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            producto_nombre: f.get(2).unwrap_or_default(),
            unidad_medida: f.get(3).unwrap_or_default(),
            cantidad,
            motivo: f.get(5).unwrap_or_default(),
            nota: f.get(6).ok(),
            costo_unitario: costo,
            costo_total: costo.map(|c| r2(c * cantidad)),
            estado: f.get(8).unwrap_or_default(),
            usuario_nombre: f.get(9).ok(),
            fecha_hora: f.get(10).unwrap_or_default(),
            anulada_por: f.get(11).ok(),
        });
    }
    Ok(lista)
}

/// GET /mermas?desde=&hasta= — las mermas (por defecto hoy).
pub async fn listar_mermas(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<Rango>) -> Result<Json<Vec<Merma>>, Fallo> {
    let (desde, hasta) = rango(&q)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    Ok(Json(leer_mermas(&conn, "substr(fecha_hora, 1, 10) BETWEEN ?1 AND ?2", vec![desde.into(), hasta.into()]).await?))
}

#[derive(Debug, Deserialize)]
pub struct NuevaMerma {
    pub producto_id: i64,
    pub cantidad: f64,
    pub motivo: String,
    #[serde(default)]
    pub nota: Option<String>,
}

/// POST /mermas — lo que se pierde sale del stock (con vencimiento, primero
/// de los lotes que vencen antes, incluidos los ya vencidos).
pub async fn registrar_merma(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(n): Json<NuevaMerma>,
) -> Result<Json<Merma>, Fallo> {
    if !cantidad_valida(n.cantidad) {
        return Err(malo("Escribe cuánto se perdió (mayor a 0)."));
    }
    let motivo = n.motivo.trim().to_uppercase();
    if !MOTIVOS_MERMA.contains(&motivo.as_str()) {
        return Err(malo("Motivo no válido."));
    }
    let nota = texto(n.nota.as_deref(), 300, "Nota")?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    let producto = datos_productos(&conn, &[n.producto_id])
        .await?
        .remove(&n.producto_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "El producto no existe.".to_string()))?;
    if !producto.controla_stock {
        return Err(malo(format!("{} se vende sin controlar stock: no hay stock que descontar.", producto.nombre)));
    }
    let id = en_transaccion!(conn, |tx| {
        let antes = stock_de(tx, producto.id).await?;
        let (_, lotes) = sacar(tx, &producto, n.cantidad, true, false).await?;
        tx.execute(
            "INSERT INTO mermas_registradas (producto_id, producto_nombre, unidad_medida, cantidad, motivo, nota, costo_unitario, lotes,
                                             estado, usuario_id, usuario_nombre, fecha_hora)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'REGISTRADA', ?9, ?10, ?11)",
            libsql::params![
                producto.id, producto.nombre.clone(), producto.unidad.clone(), n.cantidad, motivo.clone(), nota.clone(), producto.costo,
                lotes, claims.sub, claims.nombre_completo.clone(), tiempo::ahora_lima()
            ],
        )
        .await
        .map_err(sin_migracion)?;
        let id = tx.last_insert_rowid();
        let texto_motivo = match motivo.as_str() {
            "NO_VENDIDO" => "Merma: no se vendió",
            "MALOGRADO" => "Merma: se malogró",
            "VENCIDO" => "Merma: venció",
            _ => "Merma",
        };
        movimiento(tx, producto.id, "MERMA", n.cantidad, antes, claims.sub, &format!("Merma N.° {}", id), texto_motivo).await?;
        Ok(id)
    });
    leer_mermas(&conn, "id = ?1", vec![id.into()]).await?.pop().map(Json).ok_or_else(|| e500("No se pudo leer la merma."))
}

/// POST /mermas/:id/anular — solo el administrador: lo descontado vuelve al stock.
pub async fn anular_merma(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<Merma>, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    let mut f = conn
        .query("SELECT producto_id, CAST(cantidad AS REAL), lotes, estado FROM mermas_registradas WHERE id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let (producto_id, cantidad, lotes, estado): (i64, f64, Option<String>, String) = match f.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0), f.get(2).ok(), f.get(3).unwrap_or_default()),
        None => return Err((StatusCode::NOT_FOUND, "Esa merma no existe.".into())),
    };
    drop(f);
    if estado != "REGISTRADA" {
        return Err((StatusCode::CONFLICT, "Esa merma ya está anulada.".into()));
    }
    en_transaccion!(conn, |tx| {
        let n = tx
            .execute(
                "UPDATE mermas_registradas SET estado = 'ANULADA', anulada_por = ?1, fecha_anulacion = ?2 WHERE id = ?3 AND estado = 'REGISTRADA'",
                libsql::params![claims.nombre_completo.clone(), tiempo::ahora_lima(), id],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            return Err((StatusCode::CONFLICT, "Esa merma ya está anulada.".into()));
        }
        let antes = stock_de(tx, producto_id).await?;
        devolver(tx, producto_id, cantidad, lotes.as_deref()).await?;
        movimiento(tx, producto_id, "ENTRADA", cantidad, antes, claims.sub, &format!("Anulación de merma N.° {}", id), "Merma anulada").await?;
        Ok(())
    });
    leer_mermas(&conn, "id = ?1", vec![id.into()]).await?.pop().map(Json).ok_or_else(|| e500("No se pudo leer la merma."))
}

// ---------------------------------------------------------------------
// Resumen: producido, vendido y merma por producto
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FilaResumen {
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub producido: f64,
    pub vendido: f64,
    pub merma: f64,
    pub stock: f64,
    pub costo_produccion: Option<f64>,
    pub costo_merma: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Resumen {
    pub desde: String,
    pub hasta: String,
    pub filas: Vec<FilaResumen>,
    pub costo_produccion: f64,
    pub costo_merma: f64,
    /// Producciones con costo desconocido (sin receta o insumos sin costo).
    pub producciones_sin_costo: i64,
}

/// GET /produccion/resumen?desde=&hasta= — por cada producto producido o
/// con merma en esos días: cuánto se produjo, se vendió y se perdió.
pub async fn resumen(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<Rango>) -> Result<Json<Resumen>, Fallo> {
    let (desde, hasta) = rango(&q)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_PRODUCCION, NOMBRE_PRODUCCION).await?;
    let mut filas_por: HashMap<i64, FilaResumen> = HashMap::new();
    let mut sin_costo = 0i64;

    let mut f = conn
        .query(
            "SELECT producto_id, CAST(SUM(cantidad) AS REAL), CAST(SUM(costo_total) AS REAL), SUM(CASE WHEN costo_total IS NULL THEN 1 ELSE 0 END)
             FROM producciones WHERE estado = 'REGISTRADA' AND substr(fecha_hora, 1, 10) BETWEEN ?1 AND ?2 GROUP BY producto_id",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(sin_migracion)?;
    while let Some(x) = f.next().await.map_err(e500)? {
        let id: i64 = x.get(0).unwrap_or_default();
        let faltan: i64 = x.get(3).unwrap_or(0);
        sin_costo += faltan;
        let fila = filas_por.entry(id).or_insert_with(|| vacia(id));
        fila.producido = x.get(1).unwrap_or(0.0);
        fila.costo_produccion = if faltan == 0 { x.get::<f64>(2).ok().map(r2) } else { None };
    }
    drop(f);
    let mut f = conn
        .query(
            "SELECT producto_id, CAST(SUM(cantidad) AS REAL), CAST(SUM(cantidad * costo_unitario) AS REAL),
                    SUM(CASE WHEN costo_unitario IS NULL THEN 1 ELSE 0 END)
             FROM mermas_registradas WHERE estado = 'REGISTRADA' AND substr(fecha_hora, 1, 10) BETWEEN ?1 AND ?2 GROUP BY producto_id",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(e500)?;
    while let Some(x) = f.next().await.map_err(e500)? {
        let id: i64 = x.get(0).unwrap_or_default();
        let fila = filas_por.entry(id).or_insert_with(|| vacia(id));
        fila.merma = x.get(1).unwrap_or(0.0);
        fila.costo_merma = if x.get::<i64>(3).unwrap_or(0) == 0 { x.get::<f64>(2).ok().map(r2) } else { None };
    }
    drop(f);
    let ids: Vec<i64> = filas_por.keys().copied().collect();
    if !ids.is_empty() {
        let lista = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        // Ventas vigentes de esos días (la venta guarda la hora en UTC).
        let mut f = conn
            .query(
                &format!(
                    "SELECT dv.producto_id, CAST(SUM(dv.cantidad) AS REAL) FROM detalles_venta dv JOIN ventas v ON v.id = dv.venta_id
                     WHERE v.estado = 'COMPLETADA' AND date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND dv.producto_id IN ({})
                     GROUP BY dv.producto_id",
                    lista
                ),
                libsql::params![desde.clone(), hasta.clone()],
            )
            .await
            .map_err(e500)?;
        while let Some(x) = f.next().await.map_err(e500)? {
            if let Some(fila) = filas_por.get_mut(&x.get::<i64>(0).unwrap_or_default()) {
                fila.vendido = x.get(1).unwrap_or(0.0);
            }
        }
        drop(f);
        for (id, p) in datos_productos(&conn, &ids).await? {
            if let Some(fila) = filas_por.get_mut(&id) {
                fila.producto_nombre = p.nombre;
                fila.unidad_medida = p.unidad;
                fila.stock = p.stock;
            }
        }
    }
    let mut filas: Vec<FilaResumen> = filas_por.into_values().collect();
    filas.sort_by(|a, b| a.producto_nombre.to_lowercase().cmp(&b.producto_nombre.to_lowercase()));
    let costo_produccion = r2(filas.iter().filter_map(|f| f.costo_produccion).sum());
    let costo_merma = r2(filas.iter().filter_map(|f| f.costo_merma).sum());
    Ok(Json(Resumen { desde, hasta, filas, costo_produccion, costo_merma, producciones_sin_costo: sin_costo }))
}

fn vacia(id: i64) -> FilaResumen {
    FilaResumen {
        producto_id: id,
        producto_nombre: String::new(),
        unidad_medida: "UNIDAD".into(),
        producido: 0.0,
        vendido: 0.0,
        merma: 0.0,
        stock: 0.0,
        costo_produccion: None,
        costo_merma: None,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn cantidades_legibles() {
        assert_eq!(cant(2.5), "2.5");
        assert_eq!(cant(200.0), "200");
        assert_eq!(cant(0.125), "0.125");
        assert!(!cantidad_valida(0.0));
        assert!(!cantidad_valida(f64::NAN));
        assert!(cantidad_valida(0.5));
    }
}
