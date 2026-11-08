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
    /// Código de barras propio (el del six pack o la caja), opcional
    /// (migración 0064). Al escanearlo entra esta presentación.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codigo_barras: Option<String>,
    /// Precio por mayor de la presentación (migración 0065), opcional: desde
    /// `mayor_desde` presentaciones en la misma línea (3 cajas), cada una
    /// vale `precio_mayor` (S/ 100 en vez de S/ 110). Van juntos o ninguno.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mayor_desde: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precio_mayor: Option<f64>,
}

/// Precio de cada presentación cuando la línea lleva `n` (el de mayor desde
/// `mayor_desde`). Mismo cálculo que el punto de venta (utils/presentaciones.js).
pub fn precio_presentacion_para(precio: f64, mayor: Option<(f64, f64)>, n: i64) -> f64 {
    match mayor {
        Some((desde, precio_mayor)) if n as f64 + 1e-9 >= desde => precio_mayor,
        _ => precio,
    }
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
    let mut codigos: Vec<String> = Vec::new();
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
        p.codigo_barras = p.codigo_barras.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
        if let Some(c) = &p.codigo_barras {
            if c.chars().count() > 40 || c.chars().any(char::is_whitespace) {
                return Err(format!("\"{}\": el código de barras no es válido (sin espacios, máximo 40).", p.nombre));
            }
            if codigos.contains(&c.to_lowercase()) {
                return Err(format!("El código de barras {} está repetido en dos presentaciones.", c));
            }
            codigos.push(c.to_lowercase());
        }
        match (p.mayor_desde, p.precio_mayor) {
            (None, None) => {}
            (Some(desde), Some(precio_mayor)) => {
                if !desde.is_finite() || desde < 2.0 || desde > 100_000.0 || (desde - desde.round()).abs() > 1e-9 {
                    return Err(format!("\"{}\": el precio por mayor va desde 2 o más (número entero).", p.nombre));
                }
                let precio_mayor = (precio_mayor * 100.0).round() / 100.0;
                if !(precio_mayor > 0.0) || !precio_mayor.is_finite() {
                    return Err(format!("\"{}\": el precio por mayor debe ser mayor a 0.", p.nombre));
                }
                if precio_mayor >= p.precio {
                    return Err(format!("\"{}\": el precio por mayor debe ser menor a su precio normal (S/ {:.2}).", p.nombre, p.precio));
                }
                p.mayor_desde = Some(desde.round());
                p.precio_mayor = Some(precio_mayor);
            }
            _ => {
                return Err(format!("\"{}\": completa \"desde cuántas\" y el precio por mayor, o deja los dos vacíos.", p.nombre));
            }
        }
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
    // Con el código de barras (migración 0064); sin ella, como antes.
    // Con el precio por mayor de la presentación (migración 0065); sin ella, vacío.
    let consulta = |codigo: &str| {
        format!(
            "SELECT pp.id, pp.producto_id, pp.nombre, CAST(pp.factor AS REAL), CAST(pp.precio AS REAL), {}
             FROM producto_presentaciones pp JOIN productos p ON p.id = pp.producto_id
             WHERE pp.activo = 1 AND p.activo = 1 {} ORDER BY pp.producto_id, pp.orden, pp.id",
            codigo, filtro
        )
    };
    let filas = match conn
        .query(&consulta("pp.codigo_barras, CAST(pp.mayor_desde AS REAL), CAST(pp.precio_mayor AS REAL)"), params())
        .await
    {
        Ok(f) => Ok(f),
        Err(_) => match conn.query(&consulta("pp.codigo_barras, NULL, NULL"), params()).await {
            Ok(f) => Ok(f),
            Err(_) => conn.query(&consulta("NULL, NULL, NULL"), params()).await,
        },
    };
    if let Ok(mut filas) = filas {
        while let Ok(Some(f)) = filas.next().await {
            opciones.presentaciones.push(Presentacion {
                id: f.get(0).ok(),
                producto_id: f.get(1).ok(),
                nombre: f.get(2).unwrap_or_default(),
                factor: f.get(3).unwrap_or(1.0),
                precio: f.get(4).unwrap_or(0.0),
                codigo_barras: f.get::<String>(5).ok(),
                mayor_desde: f.get::<f64>(6).ok(),
                precio_mayor: f.get::<f64>(7).ok(),
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

/// Un código de barras de presentación no puede ser el código de un
/// producto ni el de una presentación de OTRO producto (al escanearlo no se
/// sabría qué agregar). `producto_id`: el producto al que pertenecen.
pub async fn revisar_codigos(conn: &libsql::Connection, producto_id: i64, presentaciones: &[Presentacion]) -> Result<(), Fallo> {
    for p in presentaciones {
        let Some(codigo) = &p.codigo_barras else { continue };
        let mut filas = conn
            .query("SELECT nombre FROM productos WHERE lower(codigo) = lower(?1) LIMIT 1", libsql::params![codigo.clone()])
            .await
            .map_err(e500)?;
        if let Some(f) = filas.next().await.map_err(e500)? {
            let nombre: String = f.get(0).unwrap_or_default();
            return Err(malo(format!("El código {} ya es el código del producto \"{}\".", codigo, nombre)));
        }
        drop(filas);
        if let Ok(mut filas) = conn
            .query(
                "SELECT p.nombre, pp.nombre FROM producto_presentaciones pp JOIN productos p ON p.id = pp.producto_id
                 WHERE pp.activo = 1 AND p.activo = 1 AND pp.producto_id <> ?1 AND lower(pp.codigo_barras) = lower(?2) LIMIT 1",
                libsql::params![producto_id, codigo.clone()],
            )
            .await
        {
            if let Ok(Some(f)) = filas.next().await {
                let producto: String = f.get(0).unwrap_or_default();
                let pres: String = f.get(1).unwrap_or_default();
                return Err(malo(format!("El código {} ya es de \"{}\" de {}.", codigo, pres, producto)));
            }
        }
    }
    Ok(())
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
    revisar_codigos(&conn, id, &opciones.presentaciones).await?;
    let sin_migracion = |e: libsql::Error| {
        if e.to_string().contains("no such column") {
            (StatusCode::SERVICE_UNAVAILABLE, "Esta función de las presentaciones aún no está lista en este negocio. Intenta en unos minutos.".to_string())
        } else if e.to_string().contains("no such table") {
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
        // Sin código ni mayor, el INSERT de siempre (funciona aunque falte la 0064).
        let r = if p.mayor_desde.is_some() {
            conn.execute(
                "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden, codigo_barras, mayor_desde, precio_mayor)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                libsql::params![id, p.nombre.clone(), p.factor, p.precio, orden as i64, p.codigo_barras.clone(), p.mayor_desde, p.precio_mayor],
            )
            .await
        } else {
            match &p.codigo_barras {
            None => {
                conn.execute(
                    "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden) VALUES (?1, ?2, ?3, ?4, ?5)",
                    libsql::params![id, p.nombre.clone(), p.factor, p.precio, orden as i64],
                )
                .await
            }
            Some(codigo) => {
                conn.execute(
                    "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden, codigo_barras) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    libsql::params![id, p.nombre.clone(), p.factor, p.precio, orden as i64, codigo.clone()],
                )
                .await
            }
            }
        };
        r.map_err(sin_migracion)?;
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

// ---------------------------------------------------------------------
// Cargar presentaciones desde un Excel (pantalla Importar)
// ---------------------------------------------------------------------

/// Una fila del Excel de presentaciones, ya emparejada en el navegador.
#[derive(Debug, Deserialize)]
pub struct FilaPresentacion {
    pub fila: i64,
    /// Código del producto unidad al que pertenece (el que ya está en Productos).
    #[serde(default)]
    pub codigo_producto: String,
    #[serde(default)]
    pub nombre: String,
    pub factor: Option<f64>,
    pub precio: Option<f64>,
    #[serde(default)]
    pub codigo_barras: Option<String>,
    /// Precio por mayor de la presentación (opcional; vacío en una fila que
    /// actualiza una presentación conserva el que ya tenía).
    #[serde(default)]
    pub mayor_desde: Option<f64>,
    #[serde(default)]
    pub precio_mayor: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ImportarPresentaciones {
    pub filas: Vec<FilaPresentacion>,
    /// true: solo revisar (no guarda nada).
    #[serde(default)]
    pub solo_revisar: bool,
}

#[derive(Debug, Serialize)]
pub struct FilaPresentacionRevisada {
    pub fila: i64,
    /// "NUEVA" | "ACTUALIZA" (ya había una con ese nombre) | "ERROR"
    pub estado: &'static str,
    pub motivo: Option<String>,
    pub producto: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResultadoPresentaciones {
    pub nuevas: usize,
    pub actualizadas: usize,
    pub errores: usize,
    pub guardado: bool,
    pub filas: Vec<FilaPresentacionRevisada>,
}

const MAX_FILAS_PRESENTACIONES: usize = 2000;

/// POST /productos/presentaciones/importar — agrega (o actualiza por nombre)
/// las presentaciones de cada producto, sin tocar las demás que ya tenga ni
/// sus precios por mayor. Solo el administrador y con el módulo encendido.
/// Las filas con error no entran; las demás entran todas juntas.
pub async fn importar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<crate::models::auth::Claims>,
    Json(datos): Json<ImportarPresentaciones>,
) -> Result<Json<ResultadoPresentaciones>, Fallo> {
    crate::middleware_auth::exigir_admin(&claims)?;
    if datos.filas.is_empty() {
        return Err(malo("El archivo no tiene filas."));
    }
    if datos.filas.len() > MAX_FILAS_PRESENTACIONES {
        return Err(malo(format!("Máximo {} filas por archivo.", MAX_FILAS_PRESENTACIONES)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    crate::handlers::rubros::exigir_modulo(&conn, crate::handlers::rubros::MODULO_PRESENTACIONES, "Presentaciones").await?;

    // Productos por código (minúsculas) -> (id, nombre).
    let mut productos: HashMap<String, (i64, String)> = HashMap::new();
    let mut filas = conn.query("SELECT id, codigo, nombre FROM productos WHERE activo = 1", ()).await.map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        productos.insert(
            f.get::<String>(1).unwrap_or_default().trim().to_lowercase(),
            (f.get(0).unwrap_or_default(), f.get(2).unwrap_or_default()),
        );
    }
    drop(filas);

    let mut revisadas: Vec<FilaPresentacionRevisada> = Vec::new();
    // producto_id -> lo que tendrá al final (las de antes + las del archivo).
    let mut por_producto: HashMap<i64, Vec<Presentacion>> = HashMap::new();
    // producto_id -> índices en `revisadas` de sus filas.
    let mut filas_de: HashMap<i64, Vec<usize>> = HashMap::new();
    // Código de barras -> producto, para no repetir entre productos del archivo.
    let mut codigos_archivo: HashMap<String, i64> = HashMap::new();

    for f in &datos.filas {
        let error = |motivo: String, producto: Option<String>| FilaPresentacionRevisada { fila: f.fila, estado: "ERROR", motivo: Some(motivo), producto };
        let Some((pid, pnombre)) = productos.get(&f.codigo_producto.trim().to_lowercase()).cloned() else {
            revisadas.push(error(format!("No hay un producto con el código \"{}\".", f.codigo_producto.trim()), None));
            continue;
        };
        let nueva = Presentacion {
            id: None,
            producto_id: None,
            nombre: f.nombre.clone(),
            factor: f.factor.unwrap_or(0.0),
            precio: f.precio.unwrap_or(0.0),
            codigo_barras: f.codigo_barras.clone(),
            mayor_desde: f.mayor_desde,
            precio_mayor: f.precio_mayor,
        };
        // Revisa la fila sola (nombre, factor, precio, código).
        let sola = match validar(OpcionesVenta { presentaciones: vec![nueva], precios_mayor: vec![] }) {
            Ok(o) => o.presentaciones.into_iter().next().unwrap(),
            Err(e) => {
                revisadas.push(error(e, Some(pnombre)));
                continue;
            }
        };
        if let Some(c) = &sola.codigo_barras {
            if let Some(otro) = codigos_archivo.get(&c.to_lowercase()) {
                if *otro != pid {
                    revisadas.push(error(format!("El código de barras {} está en otra fila del archivo.", c), Some(pnombre)));
                    continue;
                }
            }
            if let Err((_, m)) = revisar_codigos(&conn, pid, std::slice::from_ref(&sola)).await {
                revisadas.push(error(m, Some(pnombre)));
                continue;
            }
            codigos_archivo.insert(c.to_lowercase(), pid);
        }
        if !por_producto.contains_key(&pid) {
            let mut antes = leer(&conn, Some(pid)).await.presentaciones;
            for a in antes.iter_mut() {
                a.id = None;
                a.producto_id = None;
            }
            por_producto.insert(pid, antes);
        }
        let lista = por_producto.get_mut(&pid).unwrap();
        let estado = match lista.iter().position(|a| a.nombre.to_lowercase() == sola.nombre.to_lowercase()) {
            Some(i) => {
                let mut sola = sola;
                // Sin precio por mayor en la fila: se queda el que tenía (si
                // aún vale con el precio nuevo; si no, la revisión de abajo avisa).
                if sola.mayor_desde.is_none() {
                    sola.mayor_desde = lista[i].mayor_desde;
                    sola.precio_mayor = lista[i].precio_mayor;
                }
                lista[i] = sola;
                "ACTUALIZA"
            }
            None => {
                lista.push(sola);
                "NUEVA"
            }
        };
        filas_de.entry(pid).or_default().push(revisadas.len());
        revisadas.push(FilaPresentacionRevisada { fila: f.fila, estado, motivo: None, producto: Some(pnombre) });
    }

    // Cada producto completo tiene que seguir siendo válido (máximo 12,
    // sin nombres ni códigos repetidos): si no, sus filas no entran.
    let mut validas: Vec<(i64, Vec<Presentacion>)> = Vec::new();
    for (pid, lista) in por_producto {
        match validar(OpcionesVenta { presentaciones: lista, precios_mayor: vec![] }) {
            Ok(o) => validas.push((pid, o.presentaciones)),
            Err(e) => {
                for i in filas_de.get(&pid).cloned().unwrap_or_default() {
                    revisadas[i].estado = "ERROR";
                    revisadas[i].motivo = Some(e.clone());
                }
            }
        }
    }

    let nuevas = revisadas.iter().filter(|r| r.estado == "NUEVA").count();
    let actualizadas = revisadas.iter().filter(|r| r.estado == "ACTUALIZA").count();
    let errores = revisadas.iter().filter(|r| r.estado == "ERROR").count();
    let mut guardado = false;
    if !datos.solo_revisar && !validas.is_empty() {
        let tx = conn.transaction().await.map_err(e500)?;
        for (pid, lista) in &validas {
            tx.execute("UPDATE producto_presentaciones SET activo = 0 WHERE producto_id = ?1 AND activo = 1", libsql::params![*pid])
                .await
                .map_err(e500)?;
            for (orden, p) in lista.iter().enumerate() {
                let r = if p.mayor_desde.is_some() {
                    tx.execute(
                        "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden, codigo_barras, mayor_desde, precio_mayor)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        libsql::params![*pid, p.nombre.clone(), p.factor, p.precio, orden as i64, p.codigo_barras.clone(), p.mayor_desde, p.precio_mayor],
                    )
                    .await
                } else {
                    tx.execute(
                        "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden, codigo_barras) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        libsql::params![*pid, p.nombre.clone(), p.factor, p.precio, orden as i64, p.codigo_barras.clone()],
                    )
                    .await
                };
                r
                .map_err(|e| {
                    if e.to_string().contains("no such column") {
                        (StatusCode::SERVICE_UNAVAILABLE, "Esta función de las presentaciones aún no está lista en este negocio. Intenta en unos minutos.".to_string())
                    } else {
                        e500(e)
                    }
                })?;
            }
        }
        tx.commit().await.map_err(e500)?;
        guardado = true;
    }
    revisadas.sort_by_key(|r| r.fila);
    Ok(Json(ResultadoPresentaciones { nuevas, actualizadas, errores, guardado, filas: revisadas }))
}

/// Para validar una venta: por producto, sus presentaciones activas
/// (id -> (nombre, factor, precio, mayor: (desde, precio por presentación)))
/// y sus escalas por mayor (desde, precio).
#[derive(Debug, Default)]
pub struct Especiales {
    pub presentaciones: HashMap<i64, (String, f64, f64, Option<(f64, f64)>)>,
    pub mayor: Vec<(f64, f64)>,
}

pub async fn especiales_de(conn: &libsql::Connection, ids: &[i64]) -> HashMap<i64, Especiales> {
    let mut mapa: HashMap<i64, Especiales> = HashMap::new();
    if ids.is_empty() {
        return mapa;
    }
    let lista = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let consulta = |mayor: &str| {
        format!(
            "SELECT id, producto_id, nombre, CAST(factor AS REAL), CAST(precio AS REAL), {}
             FROM producto_presentaciones WHERE activo = 1 AND producto_id IN ({})",
            mayor, lista
        )
    };
    // Sin la migración 0065, sin precio por mayor de presentación.
    let filas = match conn.query(&consulta("CAST(mayor_desde AS REAL), CAST(precio_mayor AS REAL)"), ()).await {
        Ok(f) => Ok(f),
        Err(_) => conn.query(&consulta("NULL, NULL"), ()).await,
    };
    if let Ok(mut filas) = filas {
        while let Ok(Some(f)) = filas.next().await {
            let (Ok(pid), Ok(producto)) = (f.get::<i64>(0), f.get::<i64>(1)) else { continue };
            mapa.entry(producto).or_default().presentaciones.insert(
                pid,
                (
                    f.get(2).unwrap_or_default(),
                    f.get(3).unwrap_or(1.0),
                    f.get(4).unwrap_or(0.0),
                    f.get::<f64>(5).ok().zip(f.get::<f64>(6).ok()),
                ),
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
        || especiales.presentaciones.values().any(|(_, factor, precio_pres, _)| {
            cantidad + 1e-9 >= *factor && (precio - precio_unitario_de(*precio_pres, *factor)).abs() < 1e-6
        })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn codigo_de_barras_de_la_presentacion() {
        let mut con = pres("Six pack", 6.0, 35.0);
        con.codigo_barras = Some(" 7753749007962 ".into());
        let mut vacio = pres("Caja x12", 12.0, 80.0);
        vacio.codigo_barras = Some("  ".into());
        let ok = validar(OpcionesVenta { presentaciones: vec![con.clone(), vacio], precios_mayor: vec![] }).unwrap();
        assert_eq!(ok.presentaciones[0].codigo_barras.as_deref(), Some("7753749007962"));
        assert_eq!(ok.presentaciones[1].codigo_barras, None);
        let mut otra = pres("Pack x4", 4.0, 25.0);
        otra.codigo_barras = Some("7753749007962".into());
        assert!(validar(OpcionesVenta { presentaciones: vec![con, otra], precios_mayor: vec![] }).unwrap_err().contains("repetido"));
    }

    fn pres(nombre: &str, factor: f64, precio: f64) -> Presentacion {
        Presentacion { id: None, producto_id: None, nombre: nombre.into(), factor, precio, codigo_barras: None, mayor_desde: None, precio_mayor: None }
    }

    #[test]
    fn caja_por_mayor_desde_tres() {
        let mut caja = pres("Caja x12", 12.0, 110.0);
        caja.mayor_desde = Some(3.0);
        caja.precio_mayor = Some(100.0);
        let ok = validar(OpcionesVenta { presentaciones: vec![caja.clone()], precios_mayor: vec![] }).unwrap();
        assert_eq!(ok.presentaciones[0].mayor_desde, Some(3.0));
        let mayor = Some((3.0, 100.0));
        assert_eq!(precio_presentacion_para(110.0, mayor, 2), 110.0);
        assert_eq!(precio_presentacion_para(110.0, mayor, 3), 100.0);
        assert_eq!(precio_presentacion_para(110.0, None, 5), 110.0);
        // Uno sin el otro, desde 1, o más caro que el normal: no.
        let mut a_medias = caja.clone();
        a_medias.precio_mayor = None;
        assert!(validar(OpcionesVenta { presentaciones: vec![a_medias], precios_mayor: vec![] }).is_err());
        let mut desde_uno = caja.clone();
        desde_uno.mayor_desde = Some(1.0);
        assert!(validar(OpcionesVenta { presentaciones: vec![desde_uno], precios_mayor: vec![] }).is_err());
        let mut caro = caja;
        caro.precio_mayor = Some(120.0);
        assert!(validar(OpcionesVenta { presentaciones: vec![caro], precios_mayor: vec![] }).is_err());
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
        e.presentaciones.insert(1, ("Rollo x 100 m".into(), 100.0, 85.0, None));
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
