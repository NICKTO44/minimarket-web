//! Presentaciones y precio por mayor (migración 0030). Pensado para
//! ferretería, sirve a cualquier rubro que lo encienda.
//!
//! - Presentación: otra forma de vender el producto ("Rollo x 100 m" de un
//!   cable que se vende por metro). Tiene su factor (100) y su precio (S/ 85).
//!   En la venta entra como 100 m a S/ 0.85 (precio / factor, a 6
//!   decimales, como ya se vende media docena): el stock, las devoluciones,
//!   la caja, los reportes y SUNAT siguen en la unidad del producto.
//! - Precio por mayor: desde cierta cantidad, otro precio por unidad.
//!
//! Rutas (con sesión):
//!   GET /productos/opciones-venta       -> todo, para el punto de venta
//!   GET /productos/:id/presentaciones   -> las de un producto
//!   PUT /productos/:id/presentaciones   -> reemplaza las de un producto
//!
//! Si la base todavía no tiene la migración 0030, las lecturas devuelven
//! listas vacías: el sistema funciona igual que antes.

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

/// Máximo de presentaciones y de escalas por mayor de un producto.
const MAX_PRESENTACIONES: usize = 12;
const MAX_ESCALAS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Presentacion {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    pub nombre: String,
    /// Cuántas unidades del producto lleva (100 m en un rollo).
    pub factor: f64,
    /// Precio de la presentación entera.
    pub precio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecioMayor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    /// Desde esta cantidad (en la unidad del producto) vale `precio`.
    pub desde: f64,
    /// Precio por unidad del producto.
    pub precio: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OpcionesVenta {
    #[serde(default)]
    pub presentaciones: Vec<Presentacion>,
    #[serde(default)]
    pub precios_mayor: Vec<PrecioMayor>,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Precio por unidad de una línea vendida en presentación: el mismo cálculo
/// en el punto de venta y aquí (6 decimales, como la docena).
pub fn precio_unitario_de(presentacion_precio: f64, factor: f64) -> f64 {
    (presentacion_precio / factor * 1e6).round() / 1e6
}

/// Cuántas presentaciones son `cantidad` unidades (Some solo si es un número
/// entero de presentaciones, 1 o más).
pub fn presentaciones_en(cantidad: f64, factor: f64) -> Option<i64> {
    if !(factor > 0.0) || !cantidad.is_finite() {
        return None;
    }
    let n = cantidad / factor;
    let entero = n.round();
    ((n - entero).abs() < 1e-6 && entero >= 1.0).then_some(entero as i64)
}

/// Revisa y ordena lo que manda el formulario del producto.
pub fn validar(mut opciones: OpcionesVenta) -> Result<OpcionesVenta, String> {
    if opciones.presentaciones.len() > MAX_PRESENTACIONES {
        return Err(format!("Máximo {} presentaciones por producto.", MAX_PRESENTACIONES));
    }
    if opciones.precios_mayor.len() > MAX_ESCALAS {
        return Err(format!("Máximo {} precios por mayor por producto.", MAX_ESCALAS));
    }
    let mut nombres: Vec<String> = Vec::new();
    for p in opciones.presentaciones.iter_mut() {
        p.nombre = p.nombre.split_whitespace().collect::<Vec<_>>().join(" ");
        if p.nombre.is_empty() {
            return Err("Cada presentación necesita un nombre (por ejemplo \"Rollo x 100 m\").".into());
        }
        if p.nombre.chars().count() > 60 {
            return Err(format!("El nombre \"{}\" es muy largo (máximo 60 letras).", p.nombre));
        }
        let clave = p.nombre.to_lowercase();
        if nombres.contains(&clave) {
            return Err(format!("La presentación \"{}\" está repetida.", p.nombre));
        }
        nombres.push(clave);
        if !(p.factor > 0.0) || !p.factor.is_finite() || p.factor > 1_000_000.0 {
            return Err(format!("\"{}\": la cantidad que lleva debe ser mayor a 0.", p.nombre));
        }
        if !(p.precio > 0.0) || !p.precio.is_finite() || p.precio > 10_000_000.0 {
            return Err(format!("\"{}\": el precio debe ser mayor a 0.", p.nombre));
        }
        p.factor = (p.factor * 1e6).round() / 1e6;
        p.precio = (p.precio * 100.0).round() / 100.0;
        p.id = None;
        p.producto_id = None;
    }
    let mut desdes: Vec<f64> = Vec::new();
    for m in opciones.precios_mayor.iter_mut() {
        if !(m.desde > 0.0) || !m.desde.is_finite() {
            return Err("La cantidad de cada precio por mayor debe ser mayor a 0.".into());
        }
        if !(m.precio > 0.0) || !m.precio.is_finite() {
            return Err("Cada precio por mayor debe ser mayor a 0.".into());
        }
        m.desde = (m.desde * 1000.0).round() / 1000.0;
        m.precio = (m.precio * 1e6).round() / 1e6;
        if desdes.iter().any(|d| (d - m.desde).abs() < 1e-9) {
            return Err(format!("Hay dos precios por mayor desde {}.", m.desde));
        }
        desdes.push(m.desde);
        m.producto_id = None;
    }
    opciones.precios_mayor.sort_by(|a, b| a.desde.total_cmp(&b.desde));
    Ok(opciones)
}

async fn leer(conn: &libsql::Connection, solo_producto: Option<i64>) -> OpcionesVenta {
    let mut opciones = OpcionesVenta::default();
    // ?1 = 0: todos los productos.
    let filtro = "AND (?1 = 0 OR pp.producto_id = ?1)";
    let producto = solo_producto.unwrap_or(0);
    let params = || libsql::params![producto];
    // Sin la migración 0030 estas consultas fallan: listas vacías, como antes.
    if let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT pp.id, pp.producto_id, pp.nombre, CAST(pp.factor AS REAL), CAST(pp.precio AS REAL)
                 FROM producto_presentaciones pp JOIN productos p ON p.id = pp.producto_id
                 WHERE pp.activo = 1 AND p.activo = 1 {} ORDER BY pp.producto_id, pp.orden, pp.id",
                filtro
            ),
            params(),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            opciones.presentaciones.push(Presentacion {
                id: f.get(0).ok(),
                producto_id: f.get(1).ok(),
                nombre: f.get(2).unwrap_or_default(),
                factor: f.get(3).unwrap_or(1.0),
                precio: f.get(4).unwrap_or(0.0),
            });
        }
    }
    if let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT pp.producto_id, CAST(pp.desde_cantidad AS REAL), CAST(pp.precio AS REAL)
                 FROM producto_precios_mayor pp JOIN productos p ON p.id = pp.producto_id
                 WHERE p.activo = 1 {} ORDER BY pp.producto_id, pp.desde_cantidad",
                filtro
            ),
            params(),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            opciones.precios_mayor.push(PrecioMayor {
                producto_id: f.get(0).ok(),
                desde: f.get(1).unwrap_or(0.0),
                precio: f.get(2).unwrap_or(0.0),
            });
        }
    }
    opciones
}

/// GET /productos/opciones-venta — todas las presentaciones y precios por
/// mayor del negocio, para el punto de venta.
pub async fn todas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<OpcionesVenta>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(leer(&conn, None).await))
}

/// GET /productos/:id/presentaciones
pub async fn del_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<OpcionesVenta>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    let mut opciones = leer(&conn, Some(id)).await;
    for p in opciones.presentaciones.iter_mut() {
        p.producto_id = None;
    }
    for m in opciones.precios_mayor.iter_mut() {
        m.producto_id = None;
    }
    Ok(Json(opciones))
}

/// PUT /productos/:id/presentaciones — reemplaza las presentaciones y los
/// precios por mayor del producto por los que manda el formulario.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(opciones): Json<OpcionesVenta>,
) -> Result<Json<OpcionesVenta>, Fallo> {
    let opciones = validar(opciones).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    let sin_migracion = |e: libsql::Error| {
        if e.to_string().contains("no such table") {
            (StatusCode::SERVICE_UNAVAILABLE, "Las presentaciones aún no están listas en este negocio. Intenta en unos minutos.".to_string())
        } else {
            e500(e)
        }
    };
    // Las de antes se desactivan (no se borran): una venta en curso que ya
    // tenga una en el carrito recibe "la presentación cambió" y no un error raro.
    conn.execute("UPDATE producto_presentaciones SET activo = 0 WHERE producto_id = ?1 AND activo = 1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    for (orden, p) in opciones.presentaciones.iter().enumerate() {
        conn.execute(
            "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden) VALUES (?1, ?2, ?3, ?4, ?5)",
            libsql::params![id, p.nombre.clone(), p.factor, p.precio, orden as i64],
        )
        .await
        .map_err(sin_migracion)?;
    }
    conn.execute("DELETE FROM producto_precios_mayor WHERE producto_id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    for m in &opciones.precios_mayor {
        conn.execute(
            "INSERT INTO producto_precios_mayor (producto_id, desde_cantidad, precio) VALUES (?1, ?2, ?3)",
            libsql::params![id, m.desde, m.precio],
        )
        .await
        .map_err(sin_migracion)?;
    }
    del_producto(Extension(tenant), Path(id)).await
}

/// Para validar una venta: por producto, sus presentaciones activas
/// (id -> (nombre, factor, precio)) y sus escalas por mayor (desde, precio).
#[derive(Debug, Default)]
pub struct Especiales {
    pub presentaciones: HashMap<i64, (String, f64, f64)>,
    pub mayor: Vec<(f64, f64)>,
}

pub async fn especiales_de(conn: &libsql::Connection, ids: &[i64]) -> HashMap<i64, Especiales> {
    let mut mapa: HashMap<i64, Especiales> = HashMap::new();
    if ids.is_empty() {
        return mapa;
    }
    let lista = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    if let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT id, producto_id, nombre, CAST(factor AS REAL), CAST(precio AS REAL)
                 FROM producto_presentaciones WHERE activo = 1 AND producto_id IN ({})",
                lista
            ),
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let (Ok(pid), Ok(producto)) = (f.get::<i64>(0), f.get::<i64>(1)) else { continue };
            mapa.entry(producto).or_default().presentaciones.insert(
                pid,
                (f.get(2).unwrap_or_default(), f.get(3).unwrap_or(1.0), f.get(4).unwrap_or(0.0)),
            );
        }
    }
    if let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT producto_id, CAST(desde_cantidad AS REAL), CAST(precio AS REAL)
                 FROM producto_precios_mayor WHERE producto_id IN ({})",
                lista
            ),
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let Ok(producto) = f.get::<i64>(0) else { continue };
            mapa.entry(producto).or_default().mayor.push((f.get(1).unwrap_or(0.0), f.get(2).unwrap_or(0.0)));
        }
    }
    mapa
}

/// ¿Vale ese precio por la escala por mayor para esa cantidad?
pub fn precio_por_mayor_valido(precio: f64, cantidad: f64, escalas: &[(f64, f64)]) -> bool {
    escalas.iter().any(|(desde, p)| cantidad + 1e-9 >= *desde && (precio - p).abs() < 1e-6)
}

/// Precio por volumen de una línea vendida suelta (por metro, por unidad):
/// vale una escala por mayor del producto, o el precio por unidad de una de
/// sus presentaciones si la línea lleva al menos lo que trae esa
/// presentación (150 m de cable con "Rollo x 100 m a S/ 85" -> S/ 0.85 el m).
pub fn precio_por_volumen_valido(precio: f64, cantidad: f64, especiales: &Especiales) -> bool {
    precio_por_mayor_valido(precio, cantidad, &especiales.mayor)
        || especiales.presentaciones.values().any(|(_, factor, precio_pres)| {
            cantidad + 1e-9 >= *factor && (precio - precio_unitario_de(*precio_pres, *factor)).abs() < 1e-6
        })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pres(nombre: &str, factor: f64, precio: f64) -> Presentacion {
        Presentacion { id: None, producto_id: None, nombre: nombre.into(), factor, precio }
    }

    #[test]
    fn precio_unitario_como_la_docena() {
        assert_eq!(precio_unitario_de(85.0, 100.0), 0.85);
        assert_eq!(precio_unitario_de(25.0, 12.0), 2.083333);
        assert_eq!(precio_unitario_de(18.0, 0.25), 72.0);
        // La línea sale al céntimo: 12 × 2.083333 = 24.999996 -> S/ 25.00
        assert_eq!((precio_unitario_de(25.0, 12.0) * 12.0 * 100.0).round() / 100.0, 25.0);
        assert_eq!((precio_unitario_de(10.0, 7.0) * 7.0 * 100.0).round() / 100.0, 10.0);
    }

    #[test]
    fn cuantas_presentaciones() {
        assert_eq!(presentaciones_en(200.0, 100.0), Some(2));
        assert_eq!(presentaciones_en(0.25, 0.25), Some(1));
        assert_eq!(presentaciones_en(150.0, 100.0), None);
        assert_eq!(presentaciones_en(0.0, 100.0), None);
        assert_eq!(presentaciones_en(100.0, 0.0), None);
    }

    #[test]
    fn valida_el_formulario() {
        let ok = validar(OpcionesVenta {
            presentaciones: vec![pres("  Rollo   x 100 m ", 100.0, 85.0), pres("Caja x 12", 12.0, 25.0)],
            precios_mayor: vec![PrecioMayor { producto_id: None, desde: 100.0, precio: 0.75 }, PrecioMayor { producto_id: None, desde: 50.0, precio: 0.8 }],
        })
        .unwrap();
        assert_eq!(ok.presentaciones[0].nombre, "Rollo x 100 m");
        assert_eq!(ok.precios_mayor[0].desde, 50.0);
        assert!(validar(OpcionesVenta { presentaciones: vec![pres("", 1.0, 1.0)], ..Default::default() }).is_err());
        assert!(validar(OpcionesVenta { presentaciones: vec![pres("Caja", 0.0, 1.0)], ..Default::default() }).is_err());
        assert!(validar(OpcionesVenta { presentaciones: vec![pres("Caja", 12.0, 0.0)], ..Default::default() }).is_err());
        assert!(validar(OpcionesVenta { presentaciones: vec![pres("Caja", 12.0, 5.0), pres("caja", 6.0, 3.0)], ..Default::default() }).is_err());
        assert!(validar(OpcionesVenta {
            precios_mayor: vec![PrecioMayor { producto_id: None, desde: 10.0, precio: 1.0 }, PrecioMayor { producto_id: None, desde: 10.0, precio: 0.9 }],
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn la_presentacion_da_precio_por_volumen() {
        let mut e = Especiales::default();
        e.presentaciones.insert(1, ("Rollo x 100 m".into(), 100.0, 85.0));
        e.mayor.push((300.0, 0.8));
        assert!(precio_por_volumen_valido(0.85, 150.0, &e));
        assert!(precio_por_volumen_valido(0.85, 100.0, &e));
        assert!(!precio_por_volumen_valido(0.85, 99.0, &e));
        assert!(precio_por_volumen_valido(0.8, 300.0, &e));
        assert!(!precio_por_volumen_valido(0.8, 150.0, &e));
    }

    #[test]
    fn escala_por_mayor() {
        let escalas = [(50.0, 0.8), (100.0, 0.75)];
        assert!(precio_por_mayor_valido(0.8, 50.0, &escalas));
        assert!(precio_por_mayor_valido(0.75, 120.0, &escalas));
        assert!(precio_por_mayor_valido(0.8, 120.0, &escalas));
        assert!(!precio_por_mayor_valido(0.8, 49.0, &escalas));
        assert!(!precio_por_mayor_valido(0.7, 200.0, &escalas));
    }
}
