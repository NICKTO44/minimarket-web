//! Módulo COMPATIBILIDAD (migración 0038): con qué vehículos (marca,
//! modelo, años, motor) es compatible cada repuesto. Pensado para el rubro
//! Lubricentro / Repuestos.
//!
//! La búsqueda por vehículo del punto de venta se hace en el navegador con
//! la lista completa (como las fichas de farmacia): así filtra al instante,
//! sin ir al servidor por cada letra. Nada de esto cambia cómo se vende.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET /repuestos/compatibilidades         -> todas
//!   PUT /productos/:id/compatibilidad       -> reemplaza la lista de un producto

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_COMPATIBILIDAD};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Compatibilidad con vehículos";
/// Vehículos por producto, como máximo.
pub const MAX_POR_PRODUCTO: usize = 80;
const ANIO_MINIMO: i64 = 1950;

/// Marcas de dos palabras (para leer "Mercedes Benz Sprinter 2015").
const MARCAS_DOBLES: &[&str] = &[
    "mercedes benz", "land rover", "great wall", "alfa romeo", "aston martin", "royal enfield", "harley davidson",
    "rolls royce", "mini cooper", "dong feng", "king long",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compatibilidad {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    pub marca: String,
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub anio_desde: Option<i64>,
    #[serde(default)]
    pub anio_hasta: Option<i64>,
    #[serde(default)]
    pub motor: Option<String>,
    #[serde(default)]
    pub nota: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GuardarCompatibilidad {
    #[serde(default)]
    pub lista: Vec<Compatibilidad>,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, String> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(format!("{}: máximo {} letras.", campo, max)),
        otro => Ok(otro),
    }
}

/// Minúsculas, sin tildes y sin espacios de más ("  TOYOTA " -> "toyota").
pub fn clave(texto: &str) -> String {
    texto
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            otro => otro,
        })
        .collect()
}

fn anio_maximo() -> i64 {
    tiempo::hoy_lima().get(0..4).and_then(|a| a.parse::<i64>().ok()).unwrap_or(2026) + 2
}

/// Revisa una fila: marca obligatoria, años dentro de lo razonable y en orden.
pub fn validar_una(c: Compatibilidad) -> Result<Compatibilidad, String> {
    let marca = texto(Some(&c.marca), 40, "Marca del vehículo")?.ok_or("Escribe la marca del vehículo (Toyota, Honda...).")?;
    let modelo = texto(c.modelo.as_deref(), 60, "Modelo")?;
    let maximo = anio_maximo();
    for a in [c.anio_desde, c.anio_hasta].into_iter().flatten() {
        if !(ANIO_MINIMO..=maximo).contains(&a) {
            return Err(format!("Año no válido: {} (de {} a {}).", a, ANIO_MINIMO, maximo));
        }
    }
    // Solo "hasta": se entiende como ese único año.
    let (anio_desde, anio_hasta) = match (c.anio_desde, c.anio_hasta) {
        (None, Some(h)) => (Some(h), Some(h)),
        (Some(d), Some(h)) if d > h => (Some(h), Some(d)),
        otro => otro,
    };
    Ok(Compatibilidad {
        producto_id: None,
        marca,
        modelo,
        anio_desde,
        anio_hasta,
        motor: texto(c.motor.as_deref(), 40, "Motor")?,
        nota: texto(c.nota.as_deref(), 80, "Nota")?,
    })
}

/// Revisa la lista de un producto y quita las filas repetidas.
pub fn validar(lista: Vec<Compatibilidad>) -> Result<Vec<Compatibilidad>, String> {
    let mut salida: Vec<Compatibilidad> = Vec::new();
    for c in lista {
        let c = validar_una(c)?;
        let igual = |o: &Compatibilidad| {
            clave(&o.marca) == clave(&c.marca)
                && o.modelo.as_deref().map(clave) == c.modelo.as_deref().map(clave)
                && o.anio_desde == c.anio_desde
                && o.anio_hasta == c.anio_hasta
                && o.motor.as_deref().map(clave) == c.motor.as_deref().map(clave)
        };
        if !salida.iter().any(igual) {
            salida.push(c);
        }
    }
    if salida.len() > MAX_POR_PRODUCTO {
        return Err(format!("Máximo {} vehículos por producto.", MAX_POR_PRODUCTO));
    }
    Ok(salida)
}

/// "2005-2015", "2005/2015", "2005", "2005+" o "2005-" -> (desde, hasta).
fn anios_de(token: &str) -> Option<(Option<i64>, Option<i64>)> {
    let cuatro = |t: &str| (t.len() == 4 && t.chars().all(|c| c.is_ascii_digit())).then(|| t.parse::<i64>().ok()).flatten();
    if let Some(a) = cuatro(token) {
        return Some((Some(a), Some(a)));
    }
    if let Some(base) = token.strip_suffix('+').or_else(|| token.strip_suffix('-')) {
        return cuatro(base).map(|a| (Some(a), None));
    }
    let (d, h) = token.split_once(['-', '/'])?;
    Some((Some(cuatro(d)?), Some(cuatro(h)?)))
}

/// Lee una celda como "Toyota Hilux 2005-2015 2.5; Nissan Frontier 2010;
/// Honda CG 150" (un vehículo por cada punto y coma o salto de línea): la
/// primera palabra es la marca (o dos: "Mercedes Benz"), luego el modelo,
/// los años y lo que sigue a los años es el motor. "2005 a 2015" también vale.
pub fn desde_texto(texto: &str) -> Result<Vec<Compatibilidad>, String> {
    let mut salida = Vec::new();
    for parte in texto.split([';', '\n', '|']).map(str::trim).filter(|p| !p.is_empty()) {
        let mut palabras: Vec<String> = parte.split_whitespace().map(str::to_string).collect();
        // "2005 a 2015" / "2005 al 2015" -> "2005-2015".
        let mut i = 0;
        while i + 2 < palabras.len() {
            if anios_de(&palabras[i]).is_some()
                && matches!(clave(&palabras[i + 1]).as_str(), "a" | "al" | "hasta")
                && anios_de(&palabras[i + 2]).is_some()
            {
                let junto = format!("{}-{}", palabras[i], palabras[i + 2]);
                palabras.splice(i..i + 3, [junto]);
            }
            i += 1;
        }
        let largo_marca = if palabras.len() >= 2 && MARCAS_DOBLES.contains(&clave(&format!("{} {}", palabras[0], palabras[1])).as_str()) {
            2
        } else {
            1
        };
        let marca = palabras[..largo_marca.min(palabras.len())].join(" ");
        let resto = &palabras[largo_marca.min(palabras.len())..];
        let (modelo, anios, motor) = match resto.iter().position(|p| anios_de(p).is_some()) {
            Some(i) => (resto[..i].join(" "), anios_de(&resto[i]), resto[i + 1..].join(" ")),
            None => (resto.join(" "), None, String::new()),
        };
        let (anio_desde, anio_hasta) = anios.unwrap_or((None, None));
        let fila = validar_una(Compatibilidad {
            producto_id: None,
            marca,
            modelo: Some(modelo),
            anio_desde,
            anio_hasta,
            motor: Some(motor),
            nota: None,
        })
        .map_err(|e| format!("Compatible con \"{}\": {}", parte, e))?;
        salida.push(fila);
    }
    validar(salida)
}

/// Reemplaza la lista de cada producto dentro de la transacción que reciba.
pub async fn guardar_en(conn: &libsql::Connection, listas: &[(i64, Vec<Compatibilidad>)]) -> Result<(), String> {
    for tanda in listas.chunks(80) {
        let ids: String = tanda.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
        conn.execute(&format!("DELETE FROM producto_compatibilidad WHERE producto_id IN ({})", ids), ())
            .await
            .map_err(|e| format!("no se pudo guardar la compatibilidad: {}", e))?;
        let filas: Vec<(i64, &Compatibilidad)> = tanda.iter().flat_map(|(id, l)| l.iter().map(move |c| (*id, c))).collect();
        for parte in filas.chunks(80) {
            const COL: usize = 9;
            let mut marcas = Vec::with_capacity(parte.len());
            let mut valores: Vec<libsql::Value> = Vec::with_capacity(parte.len() * COL);
            let txt = |v: &Option<String>| v.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null);
            let num = |v: Option<i64>| v.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null);
            for (i, (id, c)) in parte.iter().enumerate() {
                let b = i * COL;
                marcas.push(format!("({})", (1..=COL).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
                valores.extend([
                    libsql::Value::Integer(*id),
                    libsql::Value::Text(c.marca.clone()),
                    libsql::Value::Text(clave(&c.marca)),
                    txt(&c.modelo),
                    txt(&c.modelo.as_deref().map(clave)),
                    num(c.anio_desde),
                    num(c.anio_hasta),
                    txt(&c.motor),
                    txt(&c.nota),
                ]);
            }
            conn.execute(
                &format!(
                    "INSERT INTO producto_compatibilidad (producto_id, marca, marca_clave, modelo, modelo_clave, anio_desde, anio_hasta, motor, nota)
                     VALUES {}",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(|e| format!("no se pudo guardar la compatibilidad: {}", e))?;
        }
    }
    Ok(())
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_COMPATIBILIDAD, NOMBRE_MODULO).await
}

/// GET /repuestos/compatibilidades — de los productos activos.
pub async fn todas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Compatibilidad>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut lista = Vec::new();
    // Sin la migración 0038 no hay tabla: lista vacía.
    if let Ok(mut filas) = conn
        .query(
            "SELECT c.producto_id, c.marca, c.modelo, c.anio_desde, c.anio_hasta, c.motor, c.nota
             FROM producto_compatibilidad c JOIN productos p ON p.id = c.producto_id
             WHERE p.activo = 1 ORDER BY c.producto_id, c.id",
            (),
        )
        .await
    {
        while let Some(f) = filas.next().await.map_err(e500)? {
            lista.push(Compatibilidad {
                producto_id: f.get(0).ok(),
                marca: f.get(1).unwrap_or_default(),
                modelo: f.get(2).ok(),
                anio_desde: f.get(3).ok(),
                anio_hasta: f.get(4).ok(),
                motor: f.get(5).ok(),
                nota: f.get(6).ok(),
            });
        }
    }
    Ok(Json(lista))
}

/// PUT /productos/:id/compatibilidad — reemplaza la lista del producto
/// (una lista vacía la borra).
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(payload): Json<GuardarCompatibilidad>,
) -> Result<Json<Vec<Compatibilidad>>, Fallo> {
    let lista = validar(payload.lista).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    let tx = conn.transaction().await.map_err(e500)?;
    guardar_en(&tx, &[(id, lista.clone())]).await.map_err(|e| {
        if e.contains("no such table") {
            (StatusCode::SERVICE_UNAVAILABLE, "La compatibilidad aún no está lista en este negocio. Intenta en unos minutos.".to_string())
        } else {
            e500(e)
        }
    })?;
    tx.commit().await.map_err(e500)?;
    Ok(Json(lista.into_iter().map(|c| Compatibilidad { producto_id: Some(id), ..c }).collect()))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn una(marca: &str, modelo: &str, d: Option<i64>, h: Option<i64>, motor: Option<&str>) -> Compatibilidad {
        Compatibilidad {
            producto_id: None,
            marca: marca.into(),
            modelo: Some(modelo.into()).filter(|m: &String| !m.is_empty()),
            anio_desde: d,
            anio_hasta: h,
            motor: motor.map(Into::into),
            nota: None,
        }
    }

    #[test]
    fn lee_una_celda() {
        let l = desde_texto("Toyota Hilux 2005-2015 2.5 D-4D; Nissan Frontier 2010\nHonda CG 150 ; Mercedes Benz Sprinter 2012+").unwrap();
        assert_eq!(l.len(), 4);
        assert_eq!(l[0], una("Toyota", "Hilux", Some(2005), Some(2015), Some("2.5 D-4D")));
        assert_eq!(l[1], una("Nissan", "Frontier", Some(2010), Some(2010), None));
        assert_eq!(l[2], una("Honda", "CG 150", None, None, None));
        assert_eq!(l[3], una("Mercedes Benz", "Sprinter", Some(2012), None, None));
        let a = desde_texto("Kia Rio 2012 al 2017").unwrap();
        assert_eq!(a[0], una("Kia", "Rio", Some(2012), Some(2017), None));
        assert!(desde_texto("").unwrap().is_empty());
        assert!(desde_texto("Toyota Corolla 1800").is_err());
    }

    #[test]
    fn valida_y_ordena() {
        let l = validar(vec![
            una(" toyota ", "Yaris", Some(2015), Some(2010), None),
            una("Toyota", "yaris", Some(2010), Some(2015), None),
            una("Kia", "", None, Some(2014), None),
        ])
        .unwrap();
        // Años al revés se ordenan; la repetida (mayúsculas aparte) sale una vez.
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].anio_desde, l[0].anio_hasta), (Some(2010), Some(2015)));
        assert_eq!((l[1].anio_desde, l[1].anio_hasta), (Some(2014), Some(2014)));
        assert!(validar(vec![una("", "Hilux", None, None, None)]).is_err());
    }
}
