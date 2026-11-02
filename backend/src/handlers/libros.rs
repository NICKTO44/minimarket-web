//! Módulo FICHA_LIBRO (migración 0057): autor, editorial, serie, año y
//! edición de un producto que es libro. Pensado para el rubro Librería;
//! sirve a cualquier negocio que lo encienda.
//!
//! La ficha vive en su propia tabla: un producto sin ficha es un producto
//! común (cuadernos, lapiceros, fotocopias). Nada de esto cambia cómo se
//! vende, cobra o descuenta stock. El ISBN va como código del producto.
//! La búsqueda por autor o editorial se hace en el navegador.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET    /libros/fichas                 -> todas, para el punto de venta y Productos
//!   GET    /libros/ventas-por-editorial   -> ventas de libros por editorial (rango de fechas)
//!   PUT    /productos/:id/libro           -> crea o reemplaza la de un producto
//!   DELETE /productos/:id/libro           -> el producto deja de tener ficha

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::reportes::RangoFechas;
use crate::handlers::rubros::{exigir_modulo, MODULO_FICHA_LIBRO};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Ficha del libro";
/// Año más antiguo que se acepta (la imprenta de Gutenberg).
pub const ANIO_MINIMO: i64 = 1450;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FichaLibro {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    #[serde(default)]
    pub autor: Option<String>,
    #[serde(default)]
    pub editorial: Option<String>,
    #[serde(default)]
    pub serie: Option<String>,
    #[serde(default)]
    pub anio: Option<i64>,
    #[serde(default)]
    pub edicion: Option<String>,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Espacios de más fuera; vacío = None; máximo `max` letras.
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, String> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(format!("{}: máximo {} letras.", campo, max)),
        otro => Ok(otro),
    }
}

/// El año más reciente que se acepta: el que viene (preventas).
fn anio_maximo() -> i64 {
    i64::from(tiempo::ahora_lima_fecha_hora().year()) + 1
}

/// Revisa y limpia la ficha: textos sin espacios de más y año razonable.
pub fn validar(f: FichaLibro) -> Result<FichaLibro, String> {
    let autor = texto(f.autor.as_deref(), 150, "Autor")?;
    let editorial = texto(f.editorial.as_deref(), 80, "Editorial")?;
    let serie = texto(f.serie.as_deref(), 80, "Serie o colección")?;
    let edicion = texto(f.edicion.as_deref(), 40, "Edición")?;
    let anio = match f.anio {
        None | Some(0) => None,
        Some(a) if (ANIO_MINIMO..=anio_maximo()).contains(&a) => Some(a),
        Some(_) => return Err(format!("Año: escribe un año entre {} y {} (o déjalo vacío).", ANIO_MINIMO, anio_maximo())),
    };
    Ok(FichaLibro { producto_id: None, autor, editorial, serie, anio, edicion })
}

/// true si la ficha no dice nada (equivale a no tener ficha).
pub fn vacia(f: &FichaLibro) -> bool {
    f.autor.is_none() && f.editorial.is_none() && f.serie.is_none() && f.anio.is_none() && f.edicion.is_none()
}

/// La ficha tal como viene de un archivo (importación desde Excel).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LibroImportado {
    #[serde(default)]
    pub autor: Option<String>,
    #[serde(default)]
    pub editorial: Option<String>,
    #[serde(default)]
    pub serie: Option<String>,
    /// "2010", "2010.0" (Excel) o vacío.
    #[serde(default)]
    pub anio: Option<String>,
    #[serde(default)]
    pub edicion: Option<String>,
}

/// "2010", " 2010 ", "2010.0" -> 2010. Vacío -> None.
fn anio_desde_texto(texto: &str) -> Result<Option<i64>, String> {
    let t = texto.trim();
    if t.is_empty() || t == "-" {
        return Ok(None);
    }
    let sin_decimales = t.strip_suffix(".0").unwrap_or(t);
    sin_decimales
        .parse::<i64>()
        .map(Some)
        .map_err(|_| format!("Año no reconocido: \"{}\" (escribe el año, p. ej. 2010)", t))
}

/// Ficha de una fila importada. Ok(None) si la fila no trae ningún dato del
/// libro (es un producto común).
pub fn ficha_desde_importada(f: &LibroImportado) -> Result<Option<FichaLibro>, String> {
    let ficha = validar(FichaLibro {
        producto_id: None,
        autor: f.autor.clone(),
        editorial: f.editorial.clone(),
        serie: f.serie.clone(),
        anio: anio_desde_texto(f.anio.as_deref().unwrap_or(""))?,
        edicion: f.edicion.clone(),
    })?;
    Ok(if vacia(&ficha) { None } else { Some(ficha) })
}

/// Importación: la ficha de cada fila (en el orden de `revisadas`). Una fila
/// con datos del libro mal escritos pasa a ERROR; una sin datos queda en
/// None y no toca lo que el producto ya tenía.
pub fn revisar_importados(
    filas: &[crate::handlers::importacion::FilaImportada],
    revisadas: &mut [crate::handlers::importacion::Revisada],
    usar: bool,
) -> Vec<Option<FichaLibro>> {
    let mut salida = vec![None; revisadas.len()];
    if !usar {
        return salida;
    }
    for (i, (f, r)) in filas.iter().zip(revisadas.iter_mut()).enumerate() {
        if r.padre.is_some() {
            continue;
        }
        let Some(libro) = f.libro.as_ref() else { continue };
        match ficha_desde_importada(libro) {
            Ok(ficha) => salida[i] = ficha,
            Err(e) if r.estado != "ERROR" => {
                r.estado = "ERROR";
                r.motivo = Some(e);
            }
            Err(_) => {}
        }
    }
    salida
}

/// Guarda (o reemplaza) fichas dentro de la transacción que reciba. Una
/// ficha vacía borra la que hubiera.
pub async fn guardar_en(conn: &libsql::Connection, fichas: &[(i64, FichaLibro)]) -> Result<(), String> {
    let ahora = tiempo::ahora_lima();
    for tanda in fichas.chunks(80) {
        let vacias: Vec<String> = tanda.iter().filter(|(_, f)| vacia(f)).map(|(id, _)| id.to_string()).collect();
        if !vacias.is_empty() {
            conn.execute(&format!("DELETE FROM producto_libro WHERE producto_id IN ({})", vacias.join(",")), ())
                .await
                .map_err(|e| format!("no se pudo quitar la ficha del libro: {}", e))?;
        }
        let llenas: Vec<&(i64, FichaLibro)> = tanda.iter().filter(|(_, f)| !vacia(f)).collect();
        if llenas.is_empty() {
            continue;
        }
        const COLUMNAS: usize = 7;
        let mut marcas = Vec::with_capacity(llenas.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(llenas.len() * COLUMNAS);
        let txt = |v: &Option<String>| v.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null);
        for (i, (id, f)) in llenas.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
            valores.extend([
                libsql::Value::Integer(*id),
                txt(&f.autor),
                txt(&f.editorial),
                txt(&f.serie),
                f.anio.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null),
                txt(&f.edicion),
                libsql::Value::Text(ahora.clone()),
            ]);
        }
        conn.execute(
            &format!(
                "INSERT INTO producto_libro (producto_id, autor, editorial, serie, anio, edicion, fecha_actualizacion)
                 VALUES {}
                 ON CONFLICT(producto_id) DO UPDATE SET
                    autor = excluded.autor, editorial = excluded.editorial, serie = excluded.serie,
                    anio = excluded.anio, edicion = excluded.edicion,
                    fecha_actualizacion = excluded.fecha_actualizacion",
                marcas.join(", ")
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(|e| format!("no se pudo guardar la ficha del libro: {}", e))?;
    }
    Ok(())
}

/// Las fichas de los productos activos. Sin la migración 0057: vacío.
pub async fn leer_todas(conn: &libsql::Connection) -> Result<Vec<FichaLibro>, Fallo> {
    let mut fichas = Vec::new();
    let Ok(mut filas) = conn
        .query(
            "SELECT l.producto_id, l.autor, l.editorial, l.serie, l.anio, l.edicion
             FROM producto_libro l JOIN productos p ON p.id = l.producto_id
             WHERE p.activo = 1 ORDER BY l.producto_id",
            (),
        )
        .await
    else {
        return Ok(fichas);
    };
    while let Some(f) = filas.next().await.map_err(e500)? {
        fichas.push(FichaLibro {
            producto_id: Some(f.get(0).unwrap_or_default()),
            autor: f.get(1).ok(),
            editorial: f.get(2).ok(),
            serie: f.get(3).ok(),
            anio: f.get::<i64>(4).ok().filter(|a| *a > 0),
            edicion: f.get(5).ok(),
        });
    }
    Ok(fichas)
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_FICHA_LIBRO, NOMBRE_MODULO).await
}

fn sin_migracion(e: String) -> Fallo {
    if e.contains("no such table") {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "La ficha del libro aún no está lista en este negocio. Intenta en unos minutos.".to_string(),
        )
    } else {
        e500(e)
    }
}

/// GET /libros/fichas — las fichas de los productos activos.
pub async fn todas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<FichaLibro>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer_todas(&conn).await?))
}

/// PUT /productos/:id/libro — crea o reemplaza la ficha del producto.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(ficha): Json<FichaLibro>,
) -> Result<Json<FichaLibro>, Fallo> {
    let ficha = validar(ficha).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    let tx = conn.transaction().await.map_err(e500)?;
    guardar_en(&tx, &[(id, ficha.clone())]).await.map_err(sin_migracion)?;
    tx.commit().await.map_err(e500)?;
    Ok(Json(FichaLibro { producto_id: Some(id), ..ficha }))
}

/// DELETE /productos/:id/libro — el producto deja de tener ficha.
pub async fn quitar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<StatusCode, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let tx = conn.transaction().await.map_err(e500)?;
    guardar_en(&tx, &[(id, FichaLibro::default())]).await.map_err(sin_migracion)?;
    tx.commit().await.map_err(e500)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct VentaEditorial {
    pub editorial: String,
    /// Títulos distintos vendidos.
    pub titulos: i64,
    pub unidades: f64,
    pub total: f64,
}

/// GET /libros/ventas-por-editorial?fecha_inicio=&fecha_fin= — ventas
/// completadas de libros (productos con ficha), agrupadas por editorial.
/// Las fechas se cortan en hora de Perú, como los demás reportes.
pub async fn ventas_por_editorial(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(rango): Query<RangoFechas>,
) -> Result<Json<Vec<VentaEditorial>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let Ok(mut filas) = conn
        .query(
            "SELECT MAX(COALESCE(NULLIF(TRIM(l.editorial), ''), 'Sin editorial')),
                    COUNT(DISTINCT dv.producto_id),
                    CAST(SUM(dv.cantidad) AS REAL),
                    CAST(SUM(dv.total_linea) AS REAL)
             FROM detalles_venta dv
             JOIN ventas v ON v.id = dv.venta_id
             JOIN producto_libro l ON l.producto_id = dv.producto_id
             WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA'
             GROUP BY LOWER(COALESCE(TRIM(l.editorial), ''))
             ORDER BY SUM(dv.total_linea) DESC",
            libsql::params![rango.fecha_inicio.clone(), rango.fecha_fin.clone()],
        )
        .await
    else {
        return Ok(Json(Vec::new()));
    };
    let mut salida = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        salida.push(VentaEditorial {
            editorial: f.get(0).unwrap_or_default(),
            titulos: f.get(1).unwrap_or_default(),
            unidades: f.get(2).unwrap_or_default(),
            total: f.get(3).unwrap_or_default(),
        });
    }
    Ok(Json(salida))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn valida_y_limpia() {
        let f = validar(FichaLibro {
            producto_id: Some(9),
            autor: Some("  Mario   Vargas Llosa ".into()),
            editorial: Some("Alfaguara".into()),
            serie: Some("  ".into()),
            anio: Some(2010),
            edicion: None,
        })
        .unwrap();
        assert_eq!(f.autor.as_deref(), Some("Mario Vargas Llosa"));
        assert_eq!(f.serie, None);
        assert_eq!(f.anio, Some(2010));
        assert_eq!(f.producto_id, None);
        assert_eq!(validar(FichaLibro { anio: Some(0), ..f.clone() }).unwrap().anio, None);
        assert!(validar(FichaLibro { anio: Some(1200), ..f.clone() }).is_err());
        assert!(validar(FichaLibro { anio: Some(3000), ..f.clone() }).is_err());
        assert!(validar(FichaLibro { editorial: Some("x".repeat(81)), ..f }).is_err());
    }

    #[test]
    fn vacia_es_sin_ficha() {
        assert!(vacia(&validar(FichaLibro::default()).unwrap()));
        assert!(!vacia(&FichaLibro { editorial: Some("SM".into()), ..Default::default() }));
    }

    #[test]
    fn fila_importada() {
        assert_eq!(ficha_desde_importada(&LibroImportado::default()), Ok(None));
        let f = ficha_desde_importada(&LibroImportado {
            autor: Some("J. K. Rowling".into()),
            editorial: Some("Salamandra".into()),
            serie: Some("Harry Potter".into()),
            anio: Some("1999.0".into()),
            edicion: None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(f.anio, Some(1999));
        assert_eq!(f.serie.as_deref(), Some("Harry Potter"));
        assert!(ficha_desde_importada(&LibroImportado { anio: Some("hace tiempo".into()), ..Default::default() }).is_err());
    }
}
