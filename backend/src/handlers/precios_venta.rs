//! Módulo TRES_PRECIOS (migración 0056): además de su precio de venta
//! (Precio 1), cada producto puede tener un Precio 2 y un Precio 3 de
//! referencia. En el punto de venta el vendedor elige cuál cobrar, y el
//! servidor acepta cualquiera de los tres como precio válido de la línea.
//! Pensado para el rubro Lubricentro / Repuestos.
//!
//! Los precios extra viven en su propia tabla: un producto sin fila se vende
//! solo a su precio, como siempre. Nada de esto cambia cómo se descuenta el
//! stock ni cómo se arma el comprobante (sale con el precio elegido).
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET /precios-venta                 -> todos, para el punto de venta y Productos
//!   PUT /productos/:id/precios-venta   -> guarda los de un producto (vacíos = los quita)

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_TRES_PRECIOS};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "3 precios de venta";
/// Tope de un precio (igual de generoso que el de un producto).
const PRECIO_MAXIMO: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreciosVenta {
    #[serde(default)]
    pub producto_id: Option<i64>,
    #[serde(default)]
    pub precio_2: Option<f64>,
    #[serde(default)]
    pub precio_3: Option<f64>,
}

impl PreciosVenta {
    pub fn vacios(&self) -> bool {
        self.precio_2.is_none() && self.precio_3.is_none()
    }
}

fn e500<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Un precio escrito: vacío o 0 = sin ese precio; negativo o absurdo = error.
fn un_precio(valor: Option<f64>, nombre: &str) -> Result<Option<f64>, String> {
    match valor {
        None => Ok(None),
        Some(v) if !v.is_finite() || v < 0.0 => Err(format!("El {} no es válido.", nombre)),
        Some(v) if v == 0.0 => Ok(None),
        Some(v) if v > PRECIO_MAXIMO => Err(format!("El {} es demasiado alto.", nombre)),
        Some(v) => Ok(Some((v * 100.0).round() / 100.0)),
    }
}

/// Revisa los precios: positivos, con dos decimales. 0 o vacío = no tiene.
pub fn validar(p: PreciosVenta) -> Result<PreciosVenta, String> {
    Ok(PreciosVenta {
        producto_id: p.producto_id,
        precio_2: un_precio(p.precio_2, "Precio 2")?,
        precio_3: un_precio(p.precio_3, "Precio 3")?,
    })
}

/// Guarda (o reemplaza) los precios dentro de la transacción que reciba.
/// Unos precios vacíos borran la fila del producto.
pub async fn guardar_en(conn: &libsql::Connection, precios: &[(i64, PreciosVenta)]) -> Result<(), String> {
    let ahora = tiempo::ahora_lima();
    for tanda in precios.chunks(100) {
        let vacios: Vec<String> = tanda.iter().filter(|(_, p)| p.vacios()).map(|(id, _)| id.to_string()).collect();
        if !vacios.is_empty() {
            conn.execute(&format!("DELETE FROM producto_precios_venta WHERE producto_id IN ({})", vacios.join(",")), ())
                .await
                .map_err(|e| format!("no se pudieron quitar los precios 2 y 3: {}", e))?;
        }
        let llenos: Vec<&(i64, PreciosVenta)> = tanda.iter().filter(|(_, p)| !p.vacios()).collect();
        if llenos.is_empty() {
            continue;
        }
        const COLUMNAS: usize = 4;
        let mut marcas = Vec::with_capacity(llenos.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(llenos.len() * COLUMNAS);
        let real = |v: Option<f64>| v.map(libsql::Value::Real).unwrap_or(libsql::Value::Null);
        for (i, (id, p)) in llenos.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("(?{}, ?{}, ?{}, ?{})", b + 1, b + 2, b + 3, b + 4));
            valores.extend([libsql::Value::Integer(*id), real(p.precio_2), real(p.precio_3), libsql::Value::Text(ahora.clone())]);
        }
        conn.execute(
            &format!(
                "INSERT INTO producto_precios_venta (producto_id, precio_2, precio_3, fecha_actualizacion) VALUES {}
                 ON CONFLICT(producto_id) DO UPDATE SET
                    precio_2 = excluded.precio_2, precio_3 = excluded.precio_3,
                    fecha_actualizacion = excluded.fecha_actualizacion",
                marcas.join(", ")
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(|e| format!("no se pudieron guardar los precios 2 y 3: {}", e))?;
    }
    Ok(())
}

/// Importación de Excel: revisa el Precio 2 y 3 de cada fila (una por fila,
/// en el mismo orden que `revisadas`). Un precio mal escrito pasa la fila a
/// ERROR; una fila sin precios queda en None y no toca lo que ya había.
pub fn revisar_importados(
    filas: &[crate::handlers::importacion::FilaImportada],
    revisadas: &mut [crate::handlers::importacion::Revisada],
    usar: bool,
) -> Vec<Option<PreciosVenta>> {
    let mut salida = vec![None; revisadas.len()];
    if !usar {
        return salida;
    }
    for (i, (f, r)) in filas.iter().zip(revisadas.iter_mut()).enumerate() {
        if r.padre.is_some() {
            continue;
        }
        let Some(p) = f.precios.clone() else { continue };
        match validar(p) {
            Ok(p) if !p.vacios() => salida[i] = Some(p),
            Ok(_) => {}
            Err(e) if r.estado != "ERROR" => {
                r.estado = "ERROR";
                r.motivo = Some(e);
            }
            Err(_) => {}
        }
    }
    salida
}

/// Los precios 2 y 3 de esos productos (solo los que tengan alguno), para
/// revisar el precio de una venta. Sin la migración 0056: vacío.
pub async fn de_productos(conn: &libsql::Connection, ids: &[i64]) -> HashMap<i64, Vec<f64>> {
    let mut mapa: HashMap<i64, Vec<f64>> = HashMap::new();
    if ids.is_empty() {
        return mapa;
    }
    let lista = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT producto_id, CAST(precio_2 AS REAL), CAST(precio_3 AS REAL) FROM producto_precios_venta WHERE producto_id IN ({})",
                lista
            ),
            (),
        )
        .await
    else {
        return mapa;
    };
    while let Ok(Some(f)) = filas.next().await {
        let id: i64 = f.get(0).unwrap_or_default();
        let precios: Vec<f64> = [f.get::<f64>(1).ok(), f.get::<f64>(2).ok()].into_iter().flatten().filter(|p| *p > 0.0).collect();
        if !precios.is_empty() {
            mapa.insert(id, precios);
        }
    }
    mapa
}

/// Todos los precios guardados de productos activos. Sin la migración: vacío.
pub async fn leer_todos(conn: &libsql::Connection) -> Result<Vec<PreciosVenta>, Fallo> {
    let mut lista = Vec::new();
    let Ok(mut filas) = conn
        .query(
            "SELECT v.producto_id, CAST(v.precio_2 AS REAL), CAST(v.precio_3 AS REAL)
             FROM producto_precios_venta v JOIN productos p ON p.id = v.producto_id
             WHERE p.activo = 1 ORDER BY v.producto_id",
            (),
        )
        .await
    else {
        return Ok(lista);
    };
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(PreciosVenta {
            producto_id: f.get(0).ok(),
            precio_2: f.get::<f64>(1).ok().filter(|p| *p > 0.0),
            precio_3: f.get::<f64>(2).ok().filter(|p| *p > 0.0),
        });
    }
    Ok(lista)
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_TRES_PRECIOS, NOMBRE_MODULO).await
}

/// GET /precios-venta — los precios 2 y 3 de los productos activos.
pub async fn todos(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<PreciosVenta>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer_todos(&conn).await?))
}

/// PUT /productos/:id/precios-venta — guarda (o quita) los precios 2 y 3.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(precios): Json<PreciosVenta>,
) -> Result<Json<PreciosVenta>, Fallo> {
    let precios = validar(precios).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    guardar_en(&conn, &[(id, precios.clone())]).await.map_err(|e| {
        if e.contains("no such table") {
            (StatusCode::SERVICE_UNAVAILABLE, "Los 3 precios aún no están listos en este negocio. Intenta en unos minutos.".to_string())
        } else {
            e500(e)
        }
    })?;
    Ok(Json(PreciosVenta { producto_id: Some(id), ..precios }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn p(dos: Option<f64>, tres: Option<f64>) -> PreciosVenta {
        PreciosVenta { producto_id: None, precio_2: dos, precio_3: tres }
    }

    #[test]
    fn validar_precios() {
        assert_eq!(validar(p(Some(120.0), Some(80.0))).unwrap(), p(Some(120.0), Some(80.0)));
        assert_eq!(validar(p(Some(0.0), None)).unwrap(), p(None, None));
        assert_eq!(validar(p(Some(10.555), None)).unwrap(), p(Some(10.56), None));
        assert!(validar(p(Some(-1.0), None)).is_err());
        assert!(validar(p(None, Some(f64::NAN))).is_err());
        assert!(validar(p(None, Some(5_000_000.0))).is_err());
        assert!(p(None, None).vacios());
    }
}
