//! Módulo COSTEO ("Margen por plato"), solo para el rubro Restaurante
//! (migración 0047). Viene apagado; lo enciende el administrador.
//!
//! Un plato preparado no tiene precio de compra: su costo sale de
//!   1. su RECETA (insumos con su precio por kg, litro o unidad), exacto;
//!   2. un COSTO FIJO que el dueño escribe ("me cuesta S/ 18"), exacto;
//!   3. el PRECIO DE COMPRA del producto (una gaseosa que se compra), exacto;
//!   4. el % de costo de su CATEGORÍA, aproximado;
//!   5. el % de costo GENERAL del negocio, aproximado.
//! Así ningún plato queda "con 100 % de ganancia" por no tener costo.
//!
//! IGV: el margen se mide sobre el precio SIN IGV, porque el IGV no es del
//! negocio.
//!   - Régimen GENERAL (o NO_SE): lo cobrado se divide entre (1 + tasa) en las
//!     líneas gravadas, con la tasa congelada en cada venta (18 % o 10.5 %
//!     del restaurante MYPE). Exonerado e inafecto no se dividen.
//!   - Nuevo RUS: no cobra IGV; lo cobrado es todo del negocio.
//!   - Si compra sus insumos con factura (y no es RUS), el IGV de la compra
//!     lo recupera: el costo se toma sin ese 18 % (salvo los insumos marcados
//!     sin IGV, como verduras frescas).
//!
//! El reporte del mes usa el costo de HOY de cada plato (no se congela).
//! Los cobros de envío del delivery (productos internos "_...") salen
//! aparte: no son platos y no tienen costo de insumos.
//!
//! Rutas (solo administrador, solo con el módulo encendido):
//!   GET/PUT  /costeo/config
//!   GET      /costeo/carta
//!   GET/POST /costeo/insumos          PUT/DELETE /costeo/insumos/:id
//!   GET/PUT  /costeo/platos/:producto_id
//!   GET      /costeo/reporte?mes=AAAA-MM

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{negocio, MODULO_COSTEO};
use crate::logica::igv::tasa_valida;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);
type Resultado<T> = Result<Json<T>, Fallo>;

pub const NOMBRE_MODULO: &str = "Margen por plato";
/// IGV de lo que se compra a los proveedores (el 10.5 % es solo para lo que
/// vende el restaurante MYPE).
pub const IGV_COMPRAS: f64 = 18.0;
const RUBRO_RESTAURANTE: &str = "RESTAURANTE";
const MAX_LINEAS: usize = 60;
const MAX_PRECIO: f64 = 100_000.0;

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn mal(texto: &str) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") || texto.contains("no such column") {
        (StatusCode::SERVICE_UNAVAILABLE, "El margen por plato aún no está listo en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Solo un restaurante con el módulo encendido.
async fn exigir_costeo(conn: &libsql::Connection) -> Result<(), Fallo> {
    let n = negocio(conn).await;
    if n.rubro != RUBRO_RESTAURANTE {
        return Err((StatusCode::FORBIDDEN, format!("\"{}\" es solo para restaurantes.", NOMBRE_MODULO)));
    }
    if !n.modulos.iter().any(|m| m == MODULO_COSTEO) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Cálculo (sin base de datos)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// GENERAL, RUS o NO_SE.
    pub regimen: String,
    pub compras_con_factura: bool,
    /// Costo aproximado de un plato sin receta ni costo, en % de su precio sin IGV.
    pub costo_porcentaje: f64,
    /// Margen mínimo deseado, en % del precio sin IGV.
    pub margen_objetivo: f64,
    /// Gastos fijos aproximados del mes (si no usa el módulo Gastos).
    pub gastos_mensuales: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config { regimen: "NO_SE".into(), compras_con_factura: false, costo_porcentaje: 35.0, margen_objetivo: 65.0, gastos_mensuales: 0.0 }
    }
}

impl Config {
    /// ¿Lo que cobra lleva IGV? (todo menos el Nuevo RUS)
    pub fn cobra_igv(&self) -> bool {
        self.regimen != "RUS"
    }
    /// ¿Recupera el IGV de lo que compra?
    pub fn recupera_igv(&self) -> bool {
        self.cobra_igv() && self.compras_con_factura
    }
}

/// ¿La línea es gravada? (sin afectación guardada = gravada)
pub fn gravado(afectacion: Option<&str>) -> bool {
    match afectacion.map(|a| a.trim().to_uppercase()) {
        None => true,
        Some(a) => a.is_empty() || a == "GRAVADO",
    }
}

/// Lo que queda para el negocio de un monto cobrado (sin el IGV).
pub fn sin_igv(monto: f64, es_gravado: bool, tasa: f64, cobra_igv: bool) -> f64 {
    if cobra_igv && es_gravado {
        monto / (1.0 + tasa / 100.0)
    } else {
        monto
    }
}

/// Lo que de verdad le cuesta algo que pagó: sin el IGV si lo recupera.
pub fn costo_efectivo(pagado: f64, recupera: bool) -> f64 {
    if recupera {
        pagado / (1.0 + IGV_COMPRAS / 100.0)
    } else {
        pagado
    }
}

/// Cuántas unidades de receta (g, ml, unidad) trae la unidad de compra.
pub fn factor_unidad(unidad: &str) -> f64 {
    match unidad {
        "KG" | "LITRO" => 1000.0,
        _ => 1.0,
    }
}

/// Costo pagado de una línea de receta: precio por kg/litro/unidad y la
/// cantidad en g/ml/unidades.
pub fn costo_linea(precio: f64, unidad: &str, cantidad: f64) -> f64 {
    precio * cantidad / factor_unidad(unidad)
}

/// De dónde salió el costo de un plato.
pub const RECETA: &str = "RECETA";
pub const FIJO: &str = "FIJO";
pub const COMPRA: &str = "COMPRA";
pub const CATEGORIA: &str = "CATEGORIA";
pub const GENERAL: &str = "GENERAL";

pub fn es_exacto(metodo: &str) -> bool {
    matches!(metodo, RECETA | FIJO | COMPRA)
}

/// Lo que se sabe del costo de un plato.
#[derive(Debug, Clone, Default)]
pub struct DatosCosto {
    /// Costo efectivo de una porción según su receta (None = sin receta).
    pub receta: Option<f64>,
    /// Costo que escribió el dueño, como lo paga.
    pub costo_fijo: Option<f64>,
    /// Precio de compra (o costo promedio) del producto, como lo paga.
    pub compra: Option<f64>,
    /// % de costo de su categoría.
    pub categoria_porcentaje: Option<f64>,
}

/// Cómo se calcula el costo de un plato.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Regla {
    /// Costo efectivo por unidad.
    Unitario(f64, &'static str),
    /// % del precio sin IGV.
    Porcentaje(f64, &'static str),
}

impl Regla {
    pub fn metodo(&self) -> &'static str {
        match self {
            Regla::Unitario(_, m) | Regla::Porcentaje(_, m) => m,
        }
    }
    /// Costo de lo vendido: `cantidad` unidades por `venta_sin_igv`.
    pub fn costo(&self, cantidad: f64, venta_sin_igv: f64) -> f64 {
        match self {
            Regla::Unitario(c, _) => c * cantidad,
            Regla::Porcentaje(p, _) => venta_sin_igv * p / 100.0,
        }
    }
}

pub fn regla(d: &DatosCosto, cfg: &Config) -> Regla {
    if let Some(r) = d.receta.filter(|r| r.is_finite() && *r > 0.0) {
        return Regla::Unitario(r, RECETA);
    }
    if let Some(f) = d.costo_fijo.filter(|f| f.is_finite() && *f > 0.0) {
        return Regla::Unitario(costo_efectivo(f, cfg.recupera_igv()), FIJO);
    }
    if let Some(c) = d.compra.filter(|c| c.is_finite() && *c > 0.0) {
        return Regla::Unitario(costo_efectivo(c, cfg.recupera_igv()), COMPRA);
    }
    if let Some(p) = d.categoria_porcentaje.filter(|p| *p > 0.0 && *p < 100.0) {
        return Regla::Porcentaje(p, CATEGORIA);
    }
    Regla::Porcentaje(cfg.costo_porcentaje, GENERAL)
}

/// Margen en % del precio sin IGV (None si no hay precio).
pub fn margen(precio_sin_igv: f64, costo: f64) -> Option<f64> {
    (precio_sin_igv > 0.005).then(|| (precio_sin_igv - costo) / precio_sin_igv * 100.0)
}

/// Precio de carta (con IGV si corresponde) para ganar el margen objetivo,
/// redondeado hacia arriba a S/ 0.50.
pub fn precio_sugerido(costo: f64, objetivo: f64, es_gravado: bool, tasa: f64, cobra_igv: bool) -> Option<f64> {
    if !(costo > 0.0) || !(objetivo < 100.0) {
        return None;
    }
    let sin = costo / (1.0 - objetivo / 100.0);
    let con = if cobra_igv && es_gravado { sin * (1.0 + tasa / 100.0) } else { sin };
    Some(((con - 1e-9) * 2.0).ceil() / 2.0)
}

/// Debajo del objetivo (con medio punto de tolerancia por redondeo).
pub fn bajo_objetivo(margen: Option<f64>, objetivo: f64) -> bool {
    margen.map(|m| m < objetivo - 0.05).unwrap_or(false)
}

// ---------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------

pub async fn leer_config(conn: &libsql::Connection) -> Result<Config, Fallo> {
    let mut filas = conn
        .query(
            "SELECT regimen, compras_con_factura, CAST(costo_porcentaje AS REAL), CAST(margen_objetivo AS REAL), CAST(gastos_mensuales AS REAL)
             FROM costeo_config WHERE id = 1",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let base = Config::default();
    Ok(match filas.next().await.map_err(e500)? {
        Some(f) => Config {
            regimen: f.get::<String>(0).unwrap_or(base.regimen),
            compras_con_factura: f.get::<i64>(1).unwrap_or(0) == 1,
            costo_porcentaje: f.get(2).unwrap_or(base.costo_porcentaje),
            margen_objetivo: f.get(3).unwrap_or(base.margen_objetivo),
            gastos_mensuales: f.get(4).unwrap_or(0.0),
        },
        None => base,
    })
}

/// Por categoría: (% de costo aproximado, margen objetivo); None = el general.
async fn ajustes_categoria(conn: &libsql::Connection) -> Result<HashMap<i64, (Option<f64>, Option<f64>)>, Fallo> {
    let mut filas = conn
        .query("SELECT categoria_id, CAST(costo_porcentaje AS REAL), CAST(margen_objetivo AS REAL) FROM costeo_categorias", ())
        .await
        .map_err(sin_migracion)?;
    let mut mapa = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        mapa.insert(f.get::<i64>(0).unwrap_or_default(), (f.get::<f64>(1).ok(), f.get::<f64>(2).ok()));
    }
    Ok(mapa)
}

/// Costo efectivo de una porción de cada plato con receta.
async fn costos_por_receta(conn: &libsql::Connection, cfg: &Config) -> Result<HashMap<i64, f64>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT r.producto_id, CAST(i.precio AS REAL), i.unidad, i.con_igv, CAST(r.cantidad AS REAL), CAST(COALESCE(pl.rinde, 1) AS REAL)
             FROM costeo_receta r
             JOIN costeo_insumos i ON i.id = r.insumo_id
             LEFT JOIN costeo_platos pl ON pl.producto_id = r.producto_id",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut total: HashMap<i64, f64> = HashMap::new();
    let mut rinde: HashMap<i64, f64> = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let producto: i64 = f.get(0).unwrap_or_default();
        let unidad: String = f.get(2).unwrap_or_default();
        let pagado = costo_linea(f.get(1).unwrap_or(0.0), &unidad, f.get(4).unwrap_or(0.0));
        let con_igv = f.get::<i64>(3).unwrap_or(1) == 1;
        *total.entry(producto).or_default() += costo_efectivo(pagado, cfg.recupera_igv() && con_igv);
        rinde.insert(producto, f.get::<f64>(5).unwrap_or(1.0).max(1e-9));
    }
    Ok(total.into_iter().map(|(p, t)| (p, t / rinde.get(&p).copied().unwrap_or(1.0))).collect())
}

async fn costos_fijos(conn: &libsql::Connection) -> Result<HashMap<i64, f64>, Fallo> {
    let mut filas = conn
        .query("SELECT producto_id, CAST(costo_fijo AS REAL) FROM costeo_platos WHERE costo_fijo > 0", ())
        .await
        .map_err(sin_migracion)?;
    let mut mapa = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        mapa.insert(f.get::<i64>(0).unwrap_or_default(), f.get::<f64>(1).unwrap_or(0.0));
    }
    Ok(mapa)
}

#[derive(Debug, Clone)]
struct Producto {
    nombre: String,
    categoria_id: Option<i64>,
    categoria: String,
    precio: f64,
    afectacion: Option<String>,
    compra: Option<f64>,
    activo: bool,
    interno: bool,
    /// Plato de la "Carta de hoy" de otro día (ya no se vende).
    carta_pasada: bool,
}

async fn productos(conn: &libsql::Connection) -> Result<HashMap<i64, Producto>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT p.id, p.nombre, p.categoria_id, COALESCE(c.nombre, 'Sin categoría'), CAST(COALESCE(p.precio, 0) AS REAL),
                    COALESCE(p.afectacion_igv, c.afectacion_igv),
                    CAST(CASE WHEN p.costo_promedio > 0 THEN p.costo_promedio WHEN p.precio_compra > 0 THEN p.precio_compra END AS REAL),
                    COALESCE(p.activo, 1), COALESCE(p.codigo, ''),
                    CASE WHEN p.carta_fecha IS NOT NULL AND p.carta_fecha <> ?1 THEN 1 ELSE 0 END
             FROM productos p LEFT JOIN categorias c ON c.id = p.categoria_id",
            libsql::params![crate::handlers::carta::hoy_lima()],
        )
        .await
        .map_err(e500)?;
    let mut mapa = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        mapa.insert(
            f.get::<i64>(0).unwrap_or_default(),
            Producto {
                nombre: f.get(1).unwrap_or_default(),
                categoria_id: f.get::<i64>(2).ok(),
                categoria: f.get(3).unwrap_or_default(),
                precio: f.get(4).unwrap_or(0.0),
                afectacion: f.get::<String>(5).ok(),
                compra: f.get::<f64>(6).ok(),
                activo: f.get::<i64>(7).unwrap_or(1) == 1,
                interno: f.get::<String>(8).unwrap_or_default().starts_with('_'),
                carta_pasada: f.get::<i64>(9).unwrap_or(0) == 1,
            },
        );
    }
    Ok(mapa)
}

/// Todo lo necesario para costear: configuración, tasa y reglas por plato.
struct Costeo {
    cfg: Config,
    tasa: f64,
    productos: HashMap<i64, Producto>,
    recetas: HashMap<i64, f64>,
    fijos: HashMap<i64, f64>,
    categorias: HashMap<i64, (Option<f64>, Option<f64>)>,
}

impl Costeo {
    async fn cargar(conn: &libsql::Connection) -> Result<Self, Fallo> {
        let cfg = leer_config(conn).await?;
        let recetas = costos_por_receta(conn, &cfg).await?;
        Ok(Costeo {
            tasa: crate::handlers::igv::tasa_negocio(conn).await,
            fijos: costos_fijos(conn).await?,
            categorias: ajustes_categoria(conn).await?,
            productos: productos(conn).await?,
            recetas,
            cfg,
        })
    }
    fn regla_de(&self, producto_id: i64) -> Regla {
        let p = self.productos.get(&producto_id);
        regla(
            &DatosCosto {
                receta: self.recetas.get(&producto_id).copied(),
                costo_fijo: self.fijos.get(&producto_id).copied(),
                compra: p.and_then(|p| p.compra),
                categoria_porcentaje: self.de_categoria(p).and_then(|c| c.0),
            },
            &self.cfg,
        )
    }
    fn de_categoria(&self, p: Option<&Producto>) -> Option<(Option<f64>, Option<f64>)> {
        p.and_then(|p| p.categoria_id).and_then(|c| self.categorias.get(&c).copied())
    }
    /// Margen que se quiere en ese plato: el de su categoría o el general.
    fn objetivo_de(&self, producto_id: i64) -> f64 {
        self.de_categoria(self.productos.get(&producto_id)).and_then(|c| c.1).unwrap_or(self.cfg.margen_objetivo)
    }
}

// ---------------------------------------------------------------------
// Configuración
// ---------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PorcentajeCategoria {
    pub categoria_id: i64,
    #[serde(default)]
    pub nombre: String,
    /// None = usa el % general.
    pub costo_porcentaje: Option<f64>,
    /// None = usa el margen objetivo general.
    #[serde(default)]
    pub margen_objetivo: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct VistaConfig {
    #[serde(flatten)]
    pub config: Config,
    /// Tasa de IGV del negocio (Configuración → IGV): 18 o 10.5.
    pub tasa_igv: f64,
    pub categorias: Vec<PorcentajeCategoria>,
}

#[derive(Debug, Deserialize)]
pub struct GuardarConfig {
    pub regimen: String,
    pub compras_con_factura: bool,
    pub costo_porcentaje: f64,
    pub margen_objetivo: f64,
    #[serde(default)]
    pub gastos_mensuales: f64,
    #[serde(default)]
    pub categorias: Vec<PorcentajeCategoria>,
}

async fn vista_config(conn: &libsql::Connection) -> Result<VistaConfig, Fallo> {
    let config = leer_config(conn).await?;
    let ajustes = ajustes_categoria(conn).await?;
    let mut filas = conn.query("SELECT id, nombre FROM categorias ORDER BY nombre COLLATE NOCASE", ()).await.map_err(e500)?;
    let mut categorias = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        let (costo_porcentaje, margen_objetivo) = ajustes.get(&id).copied().unwrap_or((None, None));
        categorias.push(PorcentajeCategoria { categoria_id: id, nombre: f.get(1).unwrap_or_default(), costo_porcentaje, margen_objetivo });
    }
    Ok(VistaConfig { config, tasa_igv: crate::handlers::igv::tasa_negocio(conn).await, categorias })
}

/// GET /costeo/config
pub async fn obtener_config(Extension(tenant): Extension<Arc<TenantDb>>, Extension(claims): Extension<Claims>) -> Resultado<VistaConfig> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    Ok(Json(vista_config(&conn).await?))
}

fn porcentaje_valido(p: f64) -> bool {
    p.is_finite() && p > 0.0 && p < 100.0
}

/// PUT /costeo/config
pub async fn guardar_config(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(datos): Json<GuardarConfig>,
) -> Resultado<VistaConfig> {
    exigir_admin(&claims)?;
    let regimen = datos.regimen.trim().to_uppercase();
    if !matches!(regimen.as_str(), "GENERAL" | "RUS" | "NO_SE") {
        return Err(mal("Régimen no válido."));
    }
    if !porcentaje_valido(datos.costo_porcentaje) {
        return Err(mal("El costo aproximado debe estar entre 1 % y 99 % del precio."));
    }
    if !(datos.margen_objetivo.is_finite() && datos.margen_objetivo >= 0.0 && datos.margen_objetivo < 100.0) {
        return Err(mal("El margen objetivo debe estar entre 0 % y 99 %."));
    }
    if !(datos.gastos_mensuales.is_finite() && datos.gastos_mensuales >= 0.0 && datos.gastos_mensuales <= 100_000_000.0) {
        return Err(mal("Los gastos del mes no son válidos."));
    }
    if datos.categorias.iter().any(|c| c.costo_porcentaje.map(|p| !porcentaje_valido(p)).unwrap_or(false)) {
        return Err(mal("El % de costo de cada categoría debe estar entre 1 % y 99 %."));
    }
    if datos.categorias.iter().any(|c| c.margen_objetivo.map(|m| !(m.is_finite() && m >= 0.0 && m < 100.0)).unwrap_or(false)) {
        return Err(mal("El margen objetivo de cada categoría debe estar entre 0 % y 99 %."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    let tx = conn.transaction().await.map_err(e500)?;
    tx.execute(
        "INSERT INTO costeo_config (id, regimen, compras_con_factura, costo_porcentaje, margen_objetivo, gastos_mensuales, fecha_actualizacion)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET regimen = excluded.regimen, compras_con_factura = excluded.compras_con_factura,
           costo_porcentaje = excluded.costo_porcentaje, margen_objetivo = excluded.margen_objetivo,
           gastos_mensuales = excluded.gastos_mensuales, fecha_actualizacion = excluded.fecha_actualizacion",
        libsql::params![
            regimen,
            datos.compras_con_factura as i64,
            r2(datos.costo_porcentaje),
            r2(datos.margen_objetivo),
            r2(datos.gastos_mensuales)
        ],
    )
    .await
    .map_err(sin_migracion)?;
    for c in &datos.categorias {
        if c.costo_porcentaje.is_none() && c.margen_objetivo.is_none() {
            tx.execute("DELETE FROM costeo_categorias WHERE categoria_id = ?1", libsql::params![c.categoria_id]).await.map_err(e500)?;
        } else {
            tx.execute(
                "INSERT INTO costeo_categorias (categoria_id, costo_porcentaje, margen_objetivo) VALUES (?1, ?2, ?3)
                 ON CONFLICT(categoria_id) DO UPDATE SET costo_porcentaje = excluded.costo_porcentaje, margen_objetivo = excluded.margen_objetivo",
                libsql::params![c.categoria_id, c.costo_porcentaje.map(r2), c.margen_objetivo.map(r2)],
            )
            .await
            .map_err(e500)?;
        }
    }
    tx.commit().await.map_err(e500)?;
    Ok(Json(vista_config(&conn).await?))
}

// ---------------------------------------------------------------------
// Insumos
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Insumo {
    pub id: i64,
    pub nombre: String,
    pub unidad: String,
    pub precio: f64,
    pub con_igv: bool,
    /// En cuántos platos se usa.
    pub platos: i64,
}

#[derive(Debug, Deserialize)]
pub struct GuardarInsumo {
    pub nombre: String,
    pub unidad: String,
    pub precio: f64,
    #[serde(default = "verdadero")]
    pub con_igv: bool,
}
fn verdadero() -> bool {
    true
}

fn validar_insumo(d: &GuardarInsumo) -> Result<(String, String), Fallo> {
    let nombre = d.nombre.trim().to_string();
    if nombre.is_empty() || nombre.chars().count() > 80 {
        return Err(mal("Escribe el nombre del insumo (hasta 80 letras)."));
    }
    let unidad = d.unidad.trim().to_uppercase();
    if !matches!(unidad.as_str(), "KG" | "LITRO" | "UNIDAD") {
        return Err(mal("La unidad debe ser kilo, litro o unidad."));
    }
    if !(d.precio.is_finite() && d.precio > 0.0 && d.precio <= MAX_PRECIO) {
        return Err(mal("Escribe el precio que pagas por kilo, litro o unidad."));
    }
    Ok((nombre, unidad))
}

async fn leer_insumos(conn: &libsql::Connection, solo: Option<i64>) -> Result<Vec<Insumo>, Fallo> {
    let filtro = solo.map(|id| format!("AND i.id = {}", id)).unwrap_or_default();
    let mut filas = conn
        .query(
            &format!(
                "SELECT i.id, i.nombre, i.unidad, CAST(i.precio AS REAL), i.con_igv,
                        (SELECT COUNT(DISTINCT r.producto_id) FROM costeo_receta r WHERE r.insumo_id = i.id)
                 FROM costeo_insumos i WHERE i.activo = 1 {} ORDER BY i.nombre COLLATE NOCASE",
                filtro
            ),
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(Insumo {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            unidad: f.get(2).unwrap_or_default(),
            precio: f.get(3).unwrap_or(0.0),
            con_igv: f.get::<i64>(4).unwrap_or(1) == 1,
            platos: f.get(5).unwrap_or(0),
        });
    }
    Ok(lista)
}

async fn nombre_repetido(conn: &libsql::Connection, nombre: &str, otro_que: i64) -> Result<bool, Fallo> {
    let mut f = conn
        .query(
            "SELECT 1 FROM costeo_insumos WHERE activo = 1 AND lower(trim(nombre)) = lower(?1) AND id <> ?2 LIMIT 1",
            libsql::params![nombre, otro_que],
        )
        .await
        .map_err(sin_migracion)?;
    Ok(f.next().await.map_err(e500)?.is_some())
}

/// GET /costeo/insumos
pub async fn insumos(Extension(tenant): Extension<Arc<TenantDb>>, Extension(claims): Extension<Claims>) -> Resultado<Vec<Insumo>> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    Ok(Json(leer_insumos(&conn, None).await?))
}

/// POST /costeo/insumos
pub async fn crear_insumo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(datos): Json<GuardarInsumo>,
) -> Resultado<Insumo> {
    exigir_admin(&claims)?;
    let (nombre, unidad) = validar_insumo(&datos)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    if nombre_repetido(&conn, &nombre, 0).await? {
        return Err(mal("Ya tienes un insumo con ese nombre."));
    }
    conn.execute(
        "INSERT INTO costeo_insumos (nombre, unidad, precio, con_igv, fecha_actualizacion) VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        libsql::params![nombre, unidad, r4(datos.precio), datos.con_igv as i64],
    )
    .await
    .map_err(sin_migracion)?;
    let id = conn.last_insert_rowid();
    leer_insumos(&conn, Some(id)).await?.into_iter().next().map(Json).ok_or_else(|| e500("insumo"))
}

/// PUT /costeo/insumos/:id — al cambiar el precio cambia el costo de todos
/// los platos que lo usan.
pub async fn actualizar_insumo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(datos): Json<GuardarInsumo>,
) -> Resultado<Insumo> {
    exigir_admin(&claims)?;
    let (nombre, unidad) = validar_insumo(&datos)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    if nombre_repetido(&conn, &nombre, id).await? {
        return Err(mal("Ya tienes un insumo con ese nombre."));
    }
    let cambiados = conn
        .execute(
            "UPDATE costeo_insumos SET nombre = ?1, unidad = ?2, precio = ?3, con_igv = ?4, fecha_actualizacion = datetime('now')
             WHERE id = ?5 AND activo = 1",
            libsql::params![nombre, unidad, r4(datos.precio), datos.con_igv as i64, id],
        )
        .await
        .map_err(sin_migracion)?;
    if cambiados == 0 {
        return Err((StatusCode::NOT_FOUND, "Ese insumo no existe.".to_string()));
    }
    leer_insumos(&conn, Some(id)).await?.into_iter().next().map(Json).ok_or_else(|| e500("insumo"))
}

/// DELETE /costeo/insumos/:id — no se borra uno que está en una receta.
pub async fn borrar_insumo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<StatusCode, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    let Some(insumo) = leer_insumos(&conn, Some(id)).await?.into_iter().next() else {
        return Err((StatusCode::NOT_FOUND, "Ese insumo no existe.".to_string()));
    };
    if insumo.platos > 0 {
        return Err(mal(&format!(
            "\"{}\" está en la receta de {} plato{}. Quítalo de esas recetas primero.",
            insumo.nombre,
            insumo.platos,
            if insumo.platos == 1 { "" } else { "s" }
        )));
    }
    conn.execute("UPDATE costeo_insumos SET activo = 0, fecha_actualizacion = datetime('now') WHERE id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------
// Carta: costo y margen de cada plato hoy
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PlatoCarta {
    pub producto_id: i64,
    pub nombre: String,
    pub categoria_id: Option<i64>,
    pub categoria: String,
    /// Precio de carta (como se cobra).
    pub precio: f64,
    pub precio_sin_igv: f64,
    /// Costo efectivo de una porción.
    pub costo: f64,
    pub metodo: String,
    pub exacto: bool,
    pub ganancia: f64,
    pub margen: Option<f64>,
    /// Margen que se quiere en este plato (el de su categoría o el general).
    pub objetivo: f64,
    pub bajo_objetivo: bool,
    /// Precio de carta para llegar al margen objetivo (solo si está debajo).
    pub precio_sugerido: Option<f64>,
    pub costo_fijo: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Carta {
    pub config: Config,
    pub tasa_igv: f64,
    pub platos: Vec<PlatoCarta>,
    pub bajo_objetivo: usize,
    pub estimados: usize,
}

fn plato_carta(c: &Costeo, id: i64, p: &Producto) -> PlatoCarta {
    let es_gravado = gravado(p.afectacion.as_deref());
    let precio_sin = sin_igv(p.precio, es_gravado, c.tasa, c.cfg.cobra_igv());
    let regla = c.regla_de(id);
    let costo = regla.costo(1.0, precio_sin);
    let m = margen(precio_sin, costo);
    let objetivo = c.objetivo_de(id);
    let bajo = bajo_objetivo(m, objetivo);
    PlatoCarta {
        producto_id: id,
        nombre: p.nombre.clone(),
        categoria_id: p.categoria_id,
        categoria: p.categoria.clone(),
        precio: r2(p.precio),
        precio_sin_igv: r2(precio_sin),
        costo: r2(costo),
        metodo: regla.metodo().to_string(),
        exacto: es_exacto(regla.metodo()),
        ganancia: r2(precio_sin - costo),
        margen: m.map(r1),
        bajo_objetivo: bajo,
        objetivo,
        precio_sugerido: if bajo { precio_sugerido(costo, objetivo, es_gravado, c.tasa, c.cfg.cobra_igv()) } else { None },
        costo_fijo: c.fijos.get(&id).copied(),
    }
}

/// GET /costeo/carta — los platos activos con su costo y margen de hoy. Los
/// de la "Carta de hoy" de días pasados no salen (ya no se venden; lo que se
/// vendió de ellos sí sale en el reporte del mes).
pub async fn carta(Extension(tenant): Extension<Arc<TenantDb>>, Extension(claims): Extension<Claims>) -> Resultado<Carta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    let c = Costeo::cargar(&conn).await?;
    let mut platos: Vec<PlatoCarta> =
        c.productos.iter().filter(|(_, p)| p.activo && !p.interno && !p.carta_pasada).map(|(id, p)| plato_carta(&c, *id, p)).collect();
    platos.sort_by(|a, b| {
        a.categoria.to_lowercase().cmp(&b.categoria.to_lowercase()).then_with(|| a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase()))
    });
    let bajo = platos.iter().filter(|p| p.bajo_objetivo).count();
    let estimados = platos.iter().filter(|p| !p.exacto).count();
    Ok(Json(Carta { config: c.cfg.clone(), tasa_igv: c.tasa, platos, bajo_objetivo: bajo, estimados }))
}

// ---------------------------------------------------------------------
// Receta / costo de un plato
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LineaReceta {
    pub insumo_id: i64,
    pub nombre: String,
    pub unidad: String,
    pub precio: f64,
    pub con_igv: bool,
    /// En g (insumo por kg), ml (por litro) o unidades.
    pub cantidad: f64,
    /// Lo que se paga por esa cantidad.
    pub costo_pagado: f64,
    /// Lo que pesa en el costo (sin IGV si se recupera).
    pub costo: f64,
}

#[derive(Debug, Serialize)]
pub struct DetallePlato {
    #[serde(flatten)]
    pub plato: PlatoCarta,
    pub rinde: f64,
    pub nota: Option<String>,
    pub lineas: Vec<LineaReceta>,
    /// Costo de toda la receta (antes de dividir entre lo que rinde).
    pub costo_receta: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct LineaGuardar {
    pub insumo_id: i64,
    pub cantidad: f64,
}

#[derive(Debug, Deserialize)]
pub struct GuardarPlato {
    pub costo_fijo: Option<f64>,
    #[serde(default = "uno")]
    pub rinde: f64,
    pub nota: Option<String>,
    #[serde(default)]
    pub lineas: Vec<LineaGuardar>,
}
fn uno() -> f64 {
    1.0
}

async fn detalle_plato(conn: &libsql::Connection, producto_id: i64) -> Result<DetallePlato, Fallo> {
    let c = Costeo::cargar(conn).await?;
    let p = c.productos.get(&producto_id).filter(|p| !p.interno).ok_or((StatusCode::NOT_FOUND, "Ese plato no existe.".to_string()))?;
    let plato = plato_carta(&c, producto_id, p);
    let (mut rinde, mut nota) = (1.0, None);
    let mut f = conn
        .query("SELECT CAST(rinde AS REAL), nota FROM costeo_platos WHERE producto_id = ?1", libsql::params![producto_id])
        .await
        .map_err(sin_migracion)?;
    if let Some(fila) = f.next().await.map_err(e500)? {
        rinde = fila.get(0).unwrap_or(1.0);
        nota = fila.get::<String>(1).ok().filter(|n| !n.is_empty());
    }
    drop(f);
    let mut filas = conn
        .query(
            "SELECT i.id, i.nombre, i.unidad, CAST(i.precio AS REAL), i.con_igv, CAST(r.cantidad AS REAL)
             FROM costeo_receta r JOIN costeo_insumos i ON i.id = r.insumo_id
             WHERE r.producto_id = ?1 ORDER BY r.id",
            libsql::params![producto_id],
        )
        .await
        .map_err(sin_migracion)?;
    let mut lineas = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let unidad: String = f.get(2).unwrap_or_default();
        let precio: f64 = f.get(3).unwrap_or(0.0);
        let con_igv = f.get::<i64>(4).unwrap_or(1) == 1;
        let cantidad: f64 = f.get(5).unwrap_or(0.0);
        let pagado = costo_linea(precio, &unidad, cantidad);
        lineas.push(LineaReceta {
            insumo_id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            unidad,
            precio,
            con_igv,
            cantidad,
            costo_pagado: r4(pagado),
            costo: r4(costo_efectivo(pagado, c.cfg.recupera_igv() && con_igv)),
        });
    }
    let costo_receta = (!lineas.is_empty()).then(|| r2(lineas.iter().map(|l| l.costo).sum::<f64>()));
    Ok(DetallePlato { plato, rinde, nota, lineas, costo_receta })
}

/// GET /costeo/platos/:producto_id
pub async fn plato(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(producto_id): Path<i64>,
) -> Resultado<DetallePlato> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    Ok(Json(detalle_plato(&conn, producto_id).await?))
}

/// PUT /costeo/platos/:producto_id — receta y/o costo fijo. Sin líneas ni
/// costo, el plato vuelve al % aproximado.
pub async fn guardar_plato(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(producto_id): Path<i64>,
    Json(datos): Json<GuardarPlato>,
) -> Resultado<DetallePlato> {
    exigir_admin(&claims)?;
    if let Some(c) = datos.costo_fijo {
        if !(c.is_finite() && c >= 0.0 && c <= MAX_PRECIO) {
            return Err(mal("El costo del plato no es válido."));
        }
    }
    if !(datos.rinde.is_finite() && datos.rinde > 0.0 && datos.rinde <= 10_000.0) {
        return Err(mal("Escribe cuántas porciones salen con esta receta."));
    }
    if datos.lineas.len() > MAX_LINEAS {
        return Err(mal("Una receta puede tener hasta 60 insumos."));
    }
    if datos.lineas.iter().any(|l| !(l.cantidad.is_finite() && l.cantidad > 0.0 && l.cantidad <= 1_000_000.0)) {
        return Err(mal("Escribe la cantidad de cada insumo (en gramos, mililitros o unidades)."));
    }
    let mut vistos = std::collections::HashSet::new();
    if datos.lineas.iter().any(|l| !vistos.insert(l.insumo_id)) {
        return Err(mal("Un insumo está dos veces en la receta: junta las cantidades en una sola línea."));
    }
    let nota = datos.nota.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(|n| n.chars().take(300).collect::<String>());

    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;
    let mut f = conn
        .query("SELECT COALESCE(codigo, '') FROM productos WHERE id = ?1", libsql::params![producto_id])
        .await
        .map_err(e500)?;
    match f.next().await.map_err(e500)? {
        Some(fila) if !fila.get::<String>(0).unwrap_or_default().starts_with('_') => {}
        _ => return Err((StatusCode::NOT_FOUND, "Ese plato no existe.".to_string())),
    }
    drop(f);
    if !datos.lineas.is_empty() {
        let ids = datos.lineas.iter().map(|l| l.insumo_id.to_string()).collect::<Vec<_>>().join(",");
        let mut f = conn
            .query(&format!("SELECT COUNT(*) FROM costeo_insumos WHERE activo = 1 AND id IN ({})", ids), ())
            .await
            .map_err(sin_migracion)?;
        let encontrados: i64 = f.next().await.map_err(e500)?.and_then(|x| x.get(0).ok()).unwrap_or(0);
        if encontrados as usize != datos.lineas.len() {
            return Err(mal("Uno de los insumos ya no existe. Recarga la pantalla."));
        }
    }

    let tx = conn.transaction().await.map_err(e500)?;
    tx.execute(
        "INSERT INTO costeo_platos (producto_id, costo_fijo, rinde, nota, fecha_actualizacion) VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(producto_id) DO UPDATE SET costo_fijo = excluded.costo_fijo, rinde = excluded.rinde, nota = excluded.nota,
           fecha_actualizacion = excluded.fecha_actualizacion",
        libsql::params![producto_id, datos.costo_fijo.filter(|c| *c > 0.0).map(r4), datos.rinde, nota],
    )
    .await
    .map_err(sin_migracion)?;
    tx.execute("DELETE FROM costeo_receta WHERE producto_id = ?1", libsql::params![producto_id]).await.map_err(e500)?;
    for l in &datos.lineas {
        tx.execute(
            "INSERT INTO costeo_receta (producto_id, insumo_id, cantidad) VALUES (?1, ?2, ?3)",
            libsql::params![producto_id, l.insumo_id, r4(l.cantidad)],
        )
        .await
        .map_err(e500)?;
    }
    tx.commit().await.map_err(e500)?;
    Ok(Json(detalle_plato(&conn, producto_id).await?))
}

// ---------------------------------------------------------------------
// Reporte del mes
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FiltroMes {
    pub mes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlatoMes {
    pub producto_id: i64,
    pub nombre: String,
    pub categoria: String,
    pub cantidad: f64,
    /// Precio promedio cobrado (con IGV si lo lleva).
    pub precio_promedio: Option<f64>,
    /// Lo cobrado, menos devoluciones.
    pub venta: f64,
    pub venta_sin_igv: f64,
    /// Costo efectivo de una porción.
    pub costo_unitario: Option<f64>,
    pub costo: f64,
    pub ganancia: f64,
    pub margen: Option<f64>,
    /// Parte de la ganancia de todos los platos, en %.
    pub participacion: Option<f64>,
    pub metodo: String,
    pub exacto: bool,
    pub objetivo: f64,
    pub bajo_objetivo: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct ResumenMes {
    /// Todo lo cobrado (platos + envíos), menos devoluciones.
    pub ventas: f64,
    /// IGV dentro de lo cobrado (es de SUNAT, no del negocio).
    pub igv: f64,
    pub ventas_sin_igv: f64,
    pub platos_sin_igv: f64,
    /// Cobros de envío del delivery, sin IGV.
    pub envios_sin_igv: f64,
    /// Costo de los platos vendidos.
    pub costo: f64,
    /// % del costo que es aproximado (platos sin receta ni costo).
    pub costo_estimado: Option<f64>,
    /// Costo / venta de platos sin IGV, en %.
    pub food_cost: Option<f64>,
    /// Platos menos su costo.
    pub ganancia_platos: f64,
    pub margen_platos: Option<f64>,
    /// Ganancia de platos + envíos.
    pub ganancia_bruta: f64,
    pub gastos: f64,
    /// REGISTRADOS (módulo Gastos), ESTIMADOS (los de la configuración) o NINGUNO.
    pub gastos_fuente: String,
    pub ganancia_neta: f64,
    pub margen_neto: Option<f64>,
    /// Lo que hay que vender en el mes (cobrado, con IGV) para cubrir los gastos.
    pub punto_equilibrio: Option<f64>,
    pub devoluciones: f64,
    pub platos_bajo_objetivo: usize,
}

#[derive(Debug, Serialize)]
pub struct ReporteCosteo {
    pub mes: String,
    pub config: Config,
    pub tasa_igv: f64,
    pub resumen: ResumenMes,
    pub platos: Vec<PlatoMes>,
}

fn primer_dia(mes: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{}-01", mes), "%Y-%m-%d").ok()
}
fn mes_siguiente(dia: NaiveDate) -> NaiveDate {
    if dia.month() == 12 {
        NaiveDate::from_ymd_opt(dia.year() + 1, 1, 1).unwrap_or(dia)
    } else {
        NaiveDate::from_ymd_opt(dia.year(), dia.month() + 1, 1).unwrap_or(dia)
    }
}
/// El instante (UTC, como guarda la base) en que empieza ese día en Perú.
fn inicio_utc(dia: NaiveDate) -> String {
    format!("{} 05:00:00", dia.format("%Y-%m-%d"))
}

#[derive(Default, Clone)]
struct Acumulado {
    cantidad: f64,
    venta: f64,
    venta_sin: f64,
    /// Lo que se devuelve y no vuelve a venderse no recupera su costo.
    cantidad_costo: f64,
    venta_sin_costo: f64,
}

/// GET /costeo/reporte?mes=2026-10
pub async fn reporte(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(filtro): Query<FiltroMes>,
) -> Resultado<ReporteCosteo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_costeo(&conn).await?;

    let hoy = (Utc::now() - Duration::hours(5)).date_naive();
    let mes_actual = NaiveDate::from_ymd_opt(hoy.year(), hoy.month(), 1).unwrap_or(hoy);
    let mes = match filtro.mes.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(t) => primer_dia(t).ok_or_else(|| mal("Mes no válido. Usa el formato AAAA-MM."))?,
        None => mes_actual,
    };
    if mes > mes_actual {
        return Err(mal("Ese mes todavía no empieza."));
    }
    let siguiente = mes_siguiente(mes);
    let (desde, hasta) = (inicio_utc(mes), inicio_utc(siguiente));
    let c = Costeo::cargar(&conn).await?;
    let cobra = c.cfg.cobra_igv();

    let mut por_producto: HashMap<i64, Acumulado> = HashMap::new();
    // 1. Lo vendido, agrupado por afectación y tasa (para quitar el IGV bien).
    let mut filas = conn
        .query(
            "SELECT dv.producto_id, dv.afectacion_igv, CAST(v.igv_tasa AS REAL),
                    CAST(SUM(dv.cantidad) AS REAL), CAST(SUM(dv.total_linea) AS REAL)
             FROM detalles_venta dv JOIN ventas v ON v.id = dv.venta_id
             WHERE v.estado = 'COMPLETADA' AND v.fecha_hora >= ?1 AND v.fecha_hora < ?2
             GROUP BY dv.producto_id, dv.afectacion_igv, v.igv_tasa",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        let afectacion = f.get::<String>(1).ok();
        let tasa = tasa_valida(f.get::<f64>(2).ok().or(Some(c.tasa)));
        let (cantidad, venta): (f64, f64) = (f.get(3).unwrap_or(0.0), f.get(4).unwrap_or(0.0));
        let sin = sin_igv(venta, gravado(afectacion.as_deref()), tasa, cobra);
        let a = por_producto.entry(id).or_default();
        a.cantidad += cantidad;
        a.venta += venta;
        a.venta_sin += sin;
        a.cantidad_costo += cantidad;
        a.venta_sin_costo += sin;
    }
    drop(filas);

    // 2. Devoluciones del mes: restan lo cobrado; el costo solo se recupera
    //    si lo devuelto vuelve a venderse.
    let mut devuelto = 0.0;
    let mut filas = conn
        .query(
            "SELECT dd.producto_id, dv.afectacion_igv, CAST(v.igv_tasa AS REAL), dd.condicion,
                    CAST(SUM(dd.cantidad_devuelta) AS REAL), CAST(SUM(dd.subtotal) AS REAL)
             FROM detalles_devolucion dd
             JOIN devoluciones d ON d.id = dd.devolucion_id
             LEFT JOIN detalles_venta dv ON dv.id = dd.detalle_venta_id
             LEFT JOIN ventas v ON v.id = dd.venta_id
             WHERE d.estado = 'PROCESADA' AND d.fecha_hora >= ?1 AND d.fecha_hora < ?2
             GROUP BY dd.producto_id, dv.afectacion_igv, v.igv_tasa, dd.condicion",
            libsql::params![desde, hasta],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        let afectacion = f.get::<String>(1).ok();
        let tasa = tasa_valida(f.get::<f64>(2).ok().or(Some(c.tasa)));
        let reventa = f.get::<String>(3).map(|x| x == "REVENTA").unwrap_or(false);
        let (cantidad, monto): (f64, f64) = (f.get(4).unwrap_or(0.0), f.get(5).unwrap_or(0.0));
        let sin = sin_igv(monto, gravado(afectacion.as_deref()), tasa, cobra);
        devuelto += monto;
        let a = por_producto.entry(id).or_default();
        a.cantidad -= cantidad;
        a.venta -= monto;
        a.venta_sin -= sin;
        if reventa {
            a.cantidad_costo -= cantidad;
            a.venta_sin_costo -= sin;
        }
    }
    drop(filas);

    // 3. Cada plato con su costo; los envíos aparte.
    let mut r = ResumenMes { devoluciones: r2(devuelto), ..Default::default() };
    let mut platos = Vec::new();
    let mut costo_aprox = 0.0;
    for (id, a) in &por_producto {
        let p = c.productos.get(id);
        r.ventas += a.venta;
        r.ventas_sin_igv += a.venta_sin;
        if p.map(|p| p.interno).unwrap_or(false) {
            r.envios_sin_igv += a.venta_sin;
            continue;
        }
        let regla = c.regla_de(*id);
        let costo = regla.costo(a.cantidad_costo, a.venta_sin_costo);
        let exacto = es_exacto(regla.metodo());
        if !exacto {
            costo_aprox += costo;
        }
        r.platos_sin_igv += a.venta_sin;
        r.costo += costo;
        let m = margen(a.venta_sin, costo);
        let objetivo = c.objetivo_de(*id);
        let bajo = bajo_objetivo(m, objetivo);
        platos.push(PlatoMes {
            producto_id: *id,
            nombre: p.map(|p| p.nombre.clone()).unwrap_or_else(|| "Producto eliminado".into()),
            categoria: p.map(|p| p.categoria.clone()).unwrap_or_else(|| "Sin categoría".into()),
            cantidad: r4(a.cantidad),
            precio_promedio: (a.cantidad > 1e-9).then(|| r2(a.venta / a.cantidad)),
            venta: r2(a.venta),
            venta_sin_igv: r2(a.venta_sin),
            costo_unitario: match regla {
                Regla::Unitario(u, _) => Some(r2(u)),
                Regla::Porcentaje(..) => (a.cantidad_costo > 1e-9).then(|| r2(costo / a.cantidad_costo)),
            },
            costo: r2(costo),
            ganancia: r2(a.venta_sin - costo),
            margen: m.map(r1),
            participacion: None,
            metodo: regla.metodo().to_string(),
            exacto,
            objetivo,
            bajo_objetivo: bajo,
        });
    }
    // Una venta y su devolución completa en el mismo mes no dejan fila.
    platos.retain(|p| p.cantidad.abs() > 1e-9 || p.venta.abs() > 0.005 || p.costo.abs() > 0.005);

    r.ganancia_platos = r.platos_sin_igv - r.costo;
    for p in &mut platos {
        p.participacion = (r.ganancia_platos > 0.005).then(|| r1(p.ganancia / r.ganancia_platos * 100.0));
    }
    platos.sort_by(|a, b| b.ganancia.partial_cmp(&a.ganancia).unwrap_or(std::cmp::Ordering::Equal).then(a.nombre.cmp(&b.nombre)));
    r.platos_bajo_objetivo = platos.iter().filter(|p| p.bajo_objetivo).count();

    // 4. Gastos: los registrados en el módulo Gastos; si no hay, los aproximados.
    let ultimo = siguiente.pred_opt().unwrap_or(mes).format("%Y-%m-%d").to_string();
    let registrados: f64 = crate::handlers::gastos::totales_por_mes(&conn, &mes.format("%Y-%m-%d").to_string(), &ultimo)
        .await
        .into_iter()
        .map(|(_, t)| t)
        .sum();
    (r.gastos, r.gastos_fuente) = if registrados > 0.005 {
        (registrados, "REGISTRADOS".into())
    } else if c.cfg.gastos_mensuales > 0.005 {
        (c.cfg.gastos_mensuales, "ESTIMADOS".into())
    } else {
        (0.0, "NINGUNO".into())
    };

    r.igv = r.ventas - r.ventas_sin_igv;
    r.costo_estimado = (r.costo > 0.005).then(|| r1(costo_aprox / r.costo * 100.0));
    r.food_cost = (r.platos_sin_igv > 0.005).then(|| r1(r.costo / r.platos_sin_igv * 100.0));
    r.margen_platos = margen(r.platos_sin_igv, r.costo).map(r1);
    r.ganancia_bruta = r.ganancia_platos + r.envios_sin_igv;
    r.ganancia_neta = r.ganancia_bruta - r.gastos;
    r.margen_neto = (r.ventas_sin_igv > 0.005).then(|| r1(r.ganancia_neta / r.ventas_sin_igv * 100.0));
    // Vender X (sin IGV) deja X × margen bruto; X = gastos / margen. Se pasa a
    // lo cobrado con la misma proporción de IGV del mes.
    r.punto_equilibrio = (r.gastos > 0.005 && r.ganancia_bruta > 0.005 && r.ventas_sin_igv > 0.005)
        .then(|| r2(r.gastos / (r.ganancia_bruta / r.ventas_sin_igv) * (r.ventas / r.ventas_sin_igv)));
    for v in [
        &mut r.ventas,
        &mut r.igv,
        &mut r.ventas_sin_igv,
        &mut r.platos_sin_igv,
        &mut r.envios_sin_igv,
        &mut r.costo,
        &mut r.ganancia_platos,
        &mut r.ganancia_bruta,
        &mut r.gastos,
        &mut r.ganancia_neta,
    ] {
        *v = r2(*v);
    }

    Ok(Json(ReporteCosteo { mes: mes.format("%Y-%m").to_string(), config: c.cfg.clone(), tasa_igv: c.tasa, resumen: r, platos }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cfg(regimen: &str, factura: bool) -> Config {
        Config { regimen: regimen.into(), compras_con_factura: factura, ..Config::default() }
    }

    #[test]
    fn arroz_chaufa_del_cliente() {
        // Precio 30, costo 18: sin IGV de por medio, 40 %.
        let rus = cfg("RUS", true);
        let precio = sin_igv(30.0, true, 18.0, rus.cobra_igv());
        let r = regla(&DatosCosto { costo_fijo: Some(18.0), ..Default::default() }, &rus);
        assert_eq!(r.metodo(), FIJO);
        assert!((margen(precio, r.costo(1.0, precio)).unwrap() - 40.0).abs() < 1e-9);

        // Régimen general 18 % comprando en el mercado (sin factura): 29.2 %.
        let gen = cfg("GENERAL", false);
        let precio = sin_igv(30.0, true, 18.0, gen.cobra_igv());
        let costo = regla(&DatosCosto { costo_fijo: Some(18.0), ..Default::default() }, &gen).costo(1.0, precio);
        assert!((margen(precio, costo).unwrap() - 29.2).abs() < 0.05);

        // Con factura recupera el IGV de la compra: vuelve a 40 %.
        let gen_f = cfg("GENERAL", true);
        let costo = regla(&DatosCosto { costo_fijo: Some(18.0), ..Default::default() }, &gen_f).costo(1.0, precio);
        assert!((margen(precio, costo).unwrap() - 40.0).abs() < 1e-9);

        // Restaurante MYPE 10.5 % sin factura: 33.7 %.
        let precio = sin_igv(30.0, true, 10.5, gen.cobra_igv());
        assert!((margen(precio, 18.0).unwrap() - 33.7).abs() < 0.05);
        // "No sé" se calcula como general.
        assert!(cfg("NO_SE", false).cobra_igv() && !cfg("NO_SE", false).recupera_igv() && !cfg("RUS", true).recupera_igv());
    }

    #[test]
    fn orden_de_las_reglas() {
        let c = cfg("RUS", false);
        let todo = DatosCosto { receta: Some(7.0), costo_fijo: Some(9.0), compra: Some(11.0), categoria_porcentaje: Some(30.0) };
        assert_eq!(regla(&todo, &c), Regla::Unitario(7.0, RECETA));
        assert_eq!(regla(&DatosCosto { receta: None, ..todo.clone() }, &c), Regla::Unitario(9.0, FIJO));
        assert_eq!(regla(&DatosCosto { receta: None, costo_fijo: Some(0.0), ..todo.clone() }, &c), Regla::Unitario(11.0, COMPRA));
        assert_eq!(regla(&DatosCosto { categoria_porcentaje: Some(30.0), ..Default::default() }, &c), Regla::Porcentaje(30.0, CATEGORIA));
        assert_eq!(regla(&DatosCosto::default(), &c), Regla::Porcentaje(35.0, GENERAL));
        // Estimado por %: el margen es 100 − %.
        let r = regla(&DatosCosto::default(), &c);
        assert!((margen(20.0, r.costo(2.0, 20.0)).unwrap() - 65.0).abs() < 1e-9);
        assert!(es_exacto(RECETA) && es_exacto(FIJO) && es_exacto(COMPRA) && !es_exacto(CATEGORIA) && !es_exacto(GENERAL));
    }

    #[test]
    fn receta_en_gramos_y_precio_sugerido() {
        // Arroz a S/ 4.20 el kilo, 200 g -> 0.84; aceite S/ 9 el litro, 30 ml -> 0.27; 2 huevos a 0.50.
        assert!((costo_linea(4.2, "KG", 200.0) - 0.84).abs() < 1e-9);
        assert!((costo_linea(9.0, "LITRO", 30.0) - 0.27).abs() < 1e-9);
        assert!((costo_linea(0.5, "UNIDAD", 2.0) - 1.0).abs() < 1e-9);
        // Costo 18 (sin IGV), objetivo 65 %, 18 %: 18/0.35 = 51.43 -> 60.69 -> 61.00
        assert_eq!(precio_sugerido(18.0, 65.0, true, 18.0, true), Some(61.0));
        // RUS: 51.43 -> 51.50; exonerado igual.
        assert_eq!(precio_sugerido(18.0, 65.0, true, 18.0, false), Some(51.5));
        assert_eq!(precio_sugerido(18.0, 65.0, false, 18.0, true), Some(51.5));
        assert_eq!(precio_sugerido(0.0, 65.0, true, 18.0, true), None);
        // Exacto en 0.50 no sube.
        assert_eq!(precio_sugerido(3.5, 30.0, true, 18.0, false), Some(5.0));
        assert!(bajo_objetivo(Some(40.0), 65.0) && !bajo_objetivo(Some(64.96), 65.0) && !bajo_objetivo(None, 65.0));
        assert!(gravado(None) && gravado(Some("gravado")) && !gravado(Some("EXONERADO")) && !gravado(Some("INAFECTO")));
    }
}
