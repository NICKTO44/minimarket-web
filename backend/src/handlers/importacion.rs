//! Importar productos desde un archivo (Excel o CSV) de otro sistema.
//!
//! El navegador lee el archivo, empareja sus columnas y manda aquí las
//! filas ya ordenadas. El servidor es quien decide qué fila está lista, cuál
//! ya existe y cuál tiene un error; y solo guarda cuando se le pide
//! (`solo_revisar: false`), todo en una sola operación: o entra todo lo
//! válido, o no entra nada.
//!
//! Solo el administrador. No toca ventas, caja ni comprobantes.

use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate};

use crate::handlers::ganancias;
use crate::handlers::medicamentos::{self, Ficha, FichaImportada};
use crate::handlers::rubros::MODULO_FICHA_MEDICAMENTO;
use crate::handlers::productos::unidad_valida;
use crate::logica::tiempo;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

/// Máximo de filas por archivo.
pub const MAXIMO_FILAS: usize = 5000;
const FILAS_POR_TANDA: usize = 100;

#[derive(Debug, Deserialize, Clone)]
pub struct FilaImportada {
    /// Número de fila en el archivo, para decirle al usuario dónde mirar.
    pub fila: i64,
    #[serde(default)]
    pub codigo: String,
    #[serde(default)]
    pub nombre: String,
    pub precio: Option<f64>,
    /// None = el archivo no trae stock: un producto nuevo entra con 0 y a
    /// uno que ya existe no se le toca el stock.
    pub stock: Option<f64>,
    pub categoria: Option<String>,
    pub unidad: Option<String>,
    pub precio_compra: Option<f64>,
    pub stock_minimo: Option<f64>,
    pub descripcion: Option<String>,
    /// Datos del medicamento (solo con el módulo FICHA_MEDICAMENTO).
    #[serde(default)]
    pub ficha: Option<FichaImportada>,
    /// N.° de lote del empaque (opcional).
    #[serde(default)]
    pub lote: Option<String>,
    /// Fecha de vencimiento tal como vino ("31/12/2027", "2027-12-31",
    /// "12/2027", "dic-2027" o el número de fecha de Excel). Con ella el
    /// producto es perecible y el Stock de la fila es la cantidad de ese
    /// lote; el mismo código en varias filas = varios lotes.
    #[serde(default)]
    pub vencimiento: Option<String>,
    /// Ficha del repuesto (solo con el módulo FICHA_REPUESTO).
    #[serde(default)]
    pub repuesto: Option<crate::handlers::repuestos::RepuestoImportado>,
    /// Columna "Compatible con" (solo con el módulo COMPATIBILIDAD):
    /// "Toyota Hilux 2005-2015; Nissan Frontier 2010".
    #[serde(default)]
    pub compatible: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Importacion {
    pub filas: Vec<FilaImportada>,
    /// true = solo decir cómo quedaría (vista previa); no guarda nada.
    #[serde(default)]
    pub solo_revisar: bool,
    /// Qué hacer con los códigos que ya existen: "SALTAR" (por defecto) o
    /// "ACTUALIZAR" (su precio y, si el archivo lo trae, su stock o sus lotes).
    pub existentes: Option<String>,
    /// Categoría para las filas que no traen una.
    pub categoria_defecto: Option<String>,
    /// Unidad para las filas que no traen una.
    pub unidad_defecto: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FilaRevisada {
    pub fila: i64,
    /// "LISTO" | "EXISTE" | "ERROR" | "LOTE" (otro lote del producto de una
    /// fila anterior: entra si entra ese producto).
    pub estado: &'static str,
    pub motivo: Option<String>,
    /// La categoría en la que quedaría.
    pub categoria: String,
    pub categoria_nueva: bool,
    /// Vencimiento ya leído ("2027-12-31"), para que se vea cómo se entendió.
    pub vencimiento: Option<String>,
    pub lote: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResultadoImportacion {
    pub listos: usize,
    pub existentes: usize,
    pub errores: usize,
    /// Categorías que no existían y se crean con la importación.
    pub categorias_nuevas: Vec<String>,
    pub filas: Vec<FilaRevisada>,
    /// true si se guardó (solo_revisar = false y había algo que guardar).
    pub guardado: bool,
    pub creados: usize,
    pub actualizados: usize,
    /// Lotes de los productos nuevos.
    pub lotes_nuevos: usize,
    /// Lotes de los que ya existen (solo se cargan al elegir "Actualizar").
    pub lotes_existentes: usize,
    /// Lotes que se guardaron (nuevos o con su cantidad actualizada).
    pub lotes_guardados: usize,
}

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo<T: Into<String>>(mensaje: T) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, mensaje.into())
}

/// Texto sin espacios de más.
fn limpio(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Para comparar nombres sin que importen mayúsculas, tildes ni espacios:
/// "Polos", "polos" y " PÓLOS " son lo mismo.
pub fn clave(texto: &str) -> String {
    limpio(texto)
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

/// Un producto que ya está en la base.
#[derive(Clone, Copy)]
pub struct Existente {
    pub id: i64,
    pub activo: bool,
    /// Con vencimiento: su stock lo llevan los lotes, no se pisa.
    pub por_lotes: bool,
    pub stock: f64,
}

/// Una fila ya revisada, con sus valores finales.
pub struct Revisada {
    pub fila: i64,
    pub estado: &'static str,
    pub motivo: Option<String>,
    pub codigo: String,
    pub nombre: String,
    pub descripcion: Option<String>,
    pub precio: f64,
    pub stock: Option<f64>,
    pub stock_minimo: f64,
    pub precio_compra: f64,
    pub unidad: String,
    /// Nombre de la categoría tal como quedará (la que ya existe, o la nueva).
    pub categoria: String,
    pub categoria_nueva: bool,
    pub existente: Option<Existente>,
    /// Ficha del medicamento (solo con el módulo y si la fila la trae).
    pub ficha: Option<Ficha>,
    /// El producto queda perecible (lleva lotes con vencimiento).
    pub perecible: bool,
    /// Lote de esta fila: (n.° de lote, vencimiento "AAAA-MM-DD"); la
    /// cantidad es `stock`.
    pub lote: Option<(Option<String>, String)>,
    /// Fila "LOTE": índice (en la lista revisada) de la fila de su producto.
    pub padre: Option<usize>,
}

const MESES: [(&str, u32); 14] = [
    ("ene", 1), ("jan", 1), ("feb", 2), ("mar", 3), ("abr", 4), ("apr", 4), ("may", 5),
    ("jun", 6), ("jul", 7), ("ago", 8), ("aug", 8), ("set", 9), ("sep", 9), ("oct", 10),
];
const MESES_FIN: [(&str, u32); 3] = [("nov", 11), ("dic", 12), ("dec", 12)];

fn ultimo_dia(anio: i32, mes: u32) -> Option<NaiveDate> {
    let (a, m) = if mes == 12 { (anio + 1, 1) } else { (anio, mes + 1) };
    NaiveDate::from_ymd_opt(a, m, 1).and_then(|d| d.pred_opt())
}

/// Fecha de vencimiento escrita como venga en un archivo:
/// "31/12/2027", "31-12-27", "2027-12-31", "2027-12-31 00:00:00",
/// "20271231", "12/2027" o "dic-2027" (sin día = último día del mes, como
/// se lee en los empaques) y el número de fecha de Excel (46752).
/// Día/mes/año como en Perú; solo si el segundo número pasa de 12 se
/// entiende mes/día. None si no es una fecha clara.
pub fn fecha_de_archivo(texto: &str) -> Option<NaiveDate> {
    let t = clave(texto);
    // "2027-12-31 00:00:00" / "2027-12-31T00:00:00"
    let t = match t.find(|c: char| c == ' ' || c == 't') {
        Some(i) if i >= 8 && t[..i].chars().all(|c| c.is_ascii_digit() || c == '-' || c == '/') => t[..i].to_string(),
        _ => t,
    };
    let anio_de = |a: &str| -> Option<i32> {
        let n: i32 = a.parse().ok()?;
        match a.len() {
            2 => Some(2000 + n),
            4 => Some(n),
            _ => None,
        }
    };
    let fecha = if !t.is_empty() && t.matches('.').count() <= 1 && t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        // Número de Excel (días desde 1899-12-30) o "20271231".
        if t.len() == 8 && !t.contains('.') {
            NaiveDate::parse_from_str(&t, "%Y%m%d").ok()
        } else {
            let n: f64 = t.parse().ok()?;
            if !(20_000.0..=80_000.0).contains(&n) {
                return None;
            }
            NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_signed(chrono::Duration::days(n.floor() as i64))
        }
    } else if t.chars().any(|c| c.is_alphabetic()) {
        // "dic-2027", "31 dic 2027", "diciembre 2027"
        let partes: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()).collect();
        let mes_de = |p: &str| MESES.iter().chain(MESES_FIN.iter()).find(|(m, _)| p.starts_with(m)).map(|(_, n)| *n);
        let (dia, mes, anio) = match partes.as_slice() {
            [m, a] => (None, mes_de(m)?, anio_de(a)?),
            [d, m, a] => (Some(d.parse::<u32>().ok()?), mes_de(m)?, anio_de(a)?),
            _ => return None,
        };
        match dia {
            Some(d) => NaiveDate::from_ymd_opt(anio, mes, d),
            None => ultimo_dia(anio, mes),
        }
    } else {
        let partes: Vec<&str> = t.split(|c| c == '/' || c == '-' || c == '.').collect();
        let numero = |p: &str| -> Option<u32> { if p.is_empty() || p.len() > 4 { None } else { p.parse().ok() } };
        match partes.as_slice() {
            [a, m, d] if a.len() == 4 => NaiveDate::from_ymd_opt(anio_de(a)?, numero(m)?, numero(d)?),
            [d, m, a] => {
                let (d, m) = (numero(d)?, numero(m)?);
                let (d, m) = if m > 12 && d <= 12 { (m, d) } else { (d, m) };
                NaiveDate::from_ymd_opt(anio_de(a)?, m, d)
            }
            [m, a] if a.len() == 4 => ultimo_dia(anio_de(a)?, numero(m)?),
            [a, m] if a.len() == 4 => ultimo_dia(anio_de(a)?, numero(m)?),
            _ => None,
        }
    }?;
    (2000..=2100).contains(&fecha.year()).then_some(fecha)
}

fn dia_mes_anio(fecha: &str) -> String {
    NaiveDate::parse_from_str(fecha, "%Y-%m-%d").map(|f| f.format("%d/%m/%Y").to_string()).unwrap_or_else(|_| fecha.to_string())
}

/// Revisa todas las filas sin tocar la base.
///
/// `categorias`: clave -> nombre de las que ya existen. `hoy`: "AAAA-MM-DD"
/// (Lima), para no cargar lotes ya vencidos.
#[allow(clippy::too_many_arguments)]
pub fn revisar(
    filas: &[FilaImportada],
    existentes: &HashMap<String, Existente>,
    categorias: &HashMap<String, String>,
    categoria_defecto: &str,
    unidad_defecto: &str,
    exige_precio_compra: bool,
    con_ficha: bool,
    hoy: &str,
) -> Vec<Revisada> {
    // Primera fila en la que aparece cada código del archivo: (fila, índice, trae vencimiento).
    let mut vistos: HashMap<String, (i64, usize, bool)> = HashMap::new();
    // Lotes ya vistos de cada producto (índice de su fila): (n.° de lote, vencimiento, fila).
    let mut lotes_vistos: HashMap<usize, Vec<(Option<String>, String, i64)>> = HashMap::new();
    // Categorías nuevas: se usa el nombre tal como vino la primera vez.
    let mut nuevas: HashMap<String, String> = HashMap::new();
    let mut salida: Vec<Revisada> = Vec::with_capacity(filas.len());

    for (indice, f) in filas.iter().enumerate() {
        let codigo = limpio(&f.codigo);
        let nombre = limpio(&f.nombre);
        let descripcion = f.descripcion.as_deref().map(limpio).filter(|d| !d.is_empty());
        let precio = f.precio.unwrap_or(0.0);
        let stock_minimo = f.stock_minimo.unwrap_or(5.0);
        let precio_compra = f.precio_compra.unwrap_or(0.0);
        let unidad = f.unidad.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(unidad_defecto).to_uppercase();

        let numero_malo = |n: f64, tope: f64| !n.is_finite() || n < 0.0 || n >= tope;
        let clave_codigo = codigo.to_lowercase();
        let existente = existentes.get(&clave_codigo).copied();

        // Lote y vencimiento de la fila.
        let numero_lote = f.lote.as_deref().map(|l| limpio(l).to_uppercase()).filter(|l| !l.is_empty());
        let texto_vencimiento = f.vencimiento.as_deref().map(str::trim).filter(|v| !v.is_empty());
        let vencimiento: Result<Option<String>, String> = match texto_vencimiento {
            Some(t) => fecha_de_archivo(t)
                .map(|d| Some(d.format("%Y-%m-%d").to_string()))
                .ok_or_else(|| format!("Vencimiento no reconocido: \"{}\" (escríbelo como 31/12/2027)", t)),
            None => Ok(None),
        };
        let vence = vencimiento.clone().ok().flatten();
        let error_lote: Option<String> = if let Err(e) = &vencimiento {
            Some(e.clone())
        } else if numero_lote.as_ref().is_some_and(|l| l.chars().count() > 40) {
            Some("El n.° de lote es demasiado largo (máximo 40 letras)".to_string())
        } else if numero_lote.is_some() && vence.is_none() {
            Some(format!("Falta el vencimiento del lote {}", numero_lote.clone().unwrap_or_default()))
        } else if let Some(v) = &vence {
            if v.as_str() < hoy {
                Some(format!("Vencido el {}: un producto vencido no se carga al stock", dia_mes_anio(v)))
            } else if !f.stock.is_some_and(|s| s > 0.0) {
                Some("Falta la cantidad del lote (columna Stock)".to_string())
            } else {
                None
            }
        } else {
            None
        };

        let repetido = if codigo.is_empty() { None } else { vistos.get(&clave_codigo).copied() };

        // ---- Otra fila del mismo código: otro lote de ese producto.
        if let Some((fila_padre, padre, padre_con_vencimiento)) = repetido {
            let con_vencimiento = texto_vencimiento.is_some();
            let error = if !(padre_con_vencimiento && con_vencimiento) {
                Some(if padre_con_vencimiento {
                    format!("Código repetido en la fila {}: si es otro lote de ese producto, pon su vencimiento", fila_padre)
                } else {
                    format!("Código repetido en la fila {}", fila_padre)
                })
            } else if let Some(e) = error_lote {
                Some(e)
            } else if f.stock.map(|s| numero_malo(s, 1_000_000_000.0)).unwrap_or(false) {
                Some("La cantidad no puede ser negativa".to_string())
            } else {
                let v = vence.clone().unwrap_or_default();
                let previos = lotes_vistos.entry(padre).or_default();
                let choque = previos.iter().find(|(l, fecha, _)| (*l == numero_lote && *fecha == v) || (numero_lote.is_some() && *l == numero_lote));
                match choque {
                    Some((_, _, fila)) => Some(format!("Lote repetido: ya está en la fila {}", fila)),
                    None => {
                        previos.push((numero_lote.clone(), v, f.fila));
                        None
                    }
                }
            };
            let base = &salida[padre];
            salida.push(Revisada {
                fila: f.fila,
                estado: if error.is_some() { "ERROR" } else { "LOTE" },
                motivo: Some(error.unwrap_or_else(|| format!("Otro lote del producto de la fila {}", fila_padre))),
                codigo: base.codigo.clone(),
                nombre: base.nombre.clone(),
                descripcion: None,
                precio: base.precio,
                stock: f.stock,
                stock_minimo: base.stock_minimo,
                precio_compra: base.precio_compra,
                unidad: base.unidad.clone(),
                categoria: base.categoria.clone(),
                categoria_nueva: base.categoria_nueva,
                existente: base.existente,
                ficha: None,
                perecible: true,
                lote: vence.map(|v| (numero_lote, v)),
                padre: Some(padre),
            });
            continue;
        }
        if !codigo.is_empty() {
            vistos.insert(clave_codigo.clone(), (f.fila, indice, texto_vencimiento.is_some()));
        }

        let pedida = f.categoria.as_deref().map(limpio).filter(|c| !c.is_empty()).unwrap_or_else(|| limpio(categoria_defecto));
        let clave_categoria = clave(&pedida);
        let (categoria, categoria_nueva) = match categorias.get(&clave_categoria) {
            Some(nombre) => (nombre.clone(), false),
            None => (nuevas.entry(clave_categoria).or_insert_with(|| pedida.clone()).clone(), true),
        };
        // Sin el módulo, lo que traiga el archivo de la ficha se ignora.
        let ficha = if con_ficha {
            f.ficha.as_ref().map(medicamentos::ficha_desde_importada).unwrap_or(Ok(None))
        } else {
            Ok(None)
        };
        let es_medicamento = matches!(ficha, Ok(Some(_)));
        // Un medicamento siempre va por lotes: su stock necesita vencimiento.
        let medicamento_sin_vencimiento = es_medicamento && vence.is_none() && f.stock.is_some_and(|s| s > 0.0);

        let error = if codigo.is_empty() {
            Some("Falta el código".to_string())
        } else if codigo.chars().count() > 60 {
            Some("El código es demasiado largo (máximo 60 letras)".to_string())
        } else if nombre.is_empty() {
            Some("Falta el nombre".to_string())
        } else if nombre.chars().count() > 200 {
            Some("El nombre es demasiado largo (máximo 200 letras)".to_string())
        } else if f.precio.is_none() {
            Some("Falta el precio".to_string())
        } else if !precio.is_finite() || precio <= 0.0 || precio >= 10_000_000.0 {
            Some("El precio debe ser mayor a 0".to_string())
        } else if f.stock.map(|s| numero_malo(s, 1_000_000_000.0)).unwrap_or(false) {
            Some("El stock no puede ser negativo".to_string())
        } else if numero_malo(stock_minimo, 1_000_000_000.0) || numero_malo(precio_compra, 10_000_000.0) {
            Some("El stock mínimo y el precio de compra no pueden ser negativos".to_string())
        } else if !unidad_valida(&unidad) {
            Some(format!("Unidad no reconocida: {}", unidad))
        } else if categoria.chars().count() > 60 {
            Some("El nombre de la categoría es demasiado largo (máximo 60 letras)".to_string())
        } else if exige_precio_compra && existente.is_none() && !(precio_compra > 0.0) {
            Some("Falta el precio de compra (el reporte de ganancias está activo)".to_string())
        } else if let Err(e) = &ficha {
            Some(e.clone())
        } else if let Some(e) = error_lote {
            Some(e)
        } else if medicamento_sin_vencimiento {
            Some("Es medicamento: su stock necesita el vencimiento (y el n.° de lote)".to_string())
        } else {
            None
        };
        if error.is_none() {
            if let Some(v) = &vence {
                lotes_vistos.entry(indice).or_default().push((numero_lote.clone(), v.clone(), f.fila));
            }
        }

        let (estado, motivo) = match (error, existente) {
            (Some(e), _) => ("ERROR", Some(e)),
            (None, Some(e)) => {
                let mut texto = if e.activo { "Ya existe ese código".to_string() } else { "Ya existe ese código (producto desactivado)".to_string() };
                if e.por_lotes && vence.is_none() && f.stock.is_some_and(|s| s > 0.0) {
                    texto.push_str(" · su stock va por lotes: para cargarlo pon el vencimiento");
                }
                ("EXISTE", Some(texto))
            }
            (None, None) => ("LISTO", None),
        };

        salida.push(Revisada {
            fila: f.fila,
            estado,
            motivo,
            codigo,
            nombre,
            descripcion,
            precio,
            stock: f.stock,
            stock_minimo,
            precio_compra,
            unidad,
            categoria,
            categoria_nueva,
            existente,
            // Medicamento: perecible si es nuevo, o si ya existe y no le queda
            // stock sin lote (con stock suelto, pasa al cargar sus lotes).
            perecible: vence.is_some()
                || (es_medicamento
                    && match existente {
                        None => true,
                        Some(e) => e.por_lotes || f.stock.map(|s| s <= 0.0).unwrap_or(e.stock <= 0.0),
                    }),
            ficha: ficha.ok().flatten(),
            lote: vence.map(|v| (numero_lote, v)),
            padre: None,
        });
    }

    // Un lote extra entra solo si entra su producto.
    for i in 0..salida.len() {
        if let Some(padre) = salida[i].padre {
            if salida[i].estado == "LOTE" && salida[padre].estado == "ERROR" {
                let fila = salida[padre].fila;
                salida[i].estado = "ERROR";
                salida[i].motivo = Some(format!("El producto de la fila {} tiene un error", fila));
            }
        }
    }
    salida
}

/// Un lote que entra con la importación.
pub struct LoteImportado {
    /// El producto que ya existía; None = uno nuevo (se busca por código).
    pub producto_id: Option<i64>,
    pub codigo: String,
    pub numero: Option<String>,
    /// "AAAA-MM-DD"
    pub vence: String,
    pub cantidad: f64,
}

/// Guarda los lotes: actualiza la cantidad del que ya existe (activo, mismo
/// n.° de lote y vencimiento) y crea los demás. Por tandas, dentro de la
/// transacción de la importación.
pub async fn guardar_lotes(conn: &libsql::Connection, lotes: &[LoteImportado]) -> Result<(), String> {
    const COLUMNAS: usize = 5;
    let ahora = tiempo::ahora_lima();
    for tanda in lotes.chunks(80) {
        let mut marcas = Vec::with_capacity(tanda.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(tanda.len() * COLUMNAS);
        for (i, l) in tanda.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
            valores.extend([
                l.producto_id.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null),
                libsql::Value::Text(l.codigo.clone()),
                l.numero.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null),
                libsql::Value::Text(l.vence.clone()),
                libsql::Value::Real(l.cantidad),
            ]);
        }
        let valores_cte = format!(
            "v(pid, codigo, numero, vence, cantidad) AS (VALUES {}),
             d AS (SELECT p.id AS producto_id, v.numero, v.vence, v.cantidad
                   FROM v JOIN productos p ON (v.pid IS NOT NULL AND p.id = v.pid) OR (v.pid IS NULL AND p.codigo = v.codigo))",
            marcas.join(", ")
        );
        // El mismo lote: activo, mismo vencimiento y mismo n.° (o ambos sin n.°).
        const MISMO: &str = "d.producto_id = l.producto_id AND d.vence = date(l.fecha_vencimiento)
                             AND COALESCE(d.numero, '') = COALESCE(l.numero_lote, '')";
        let actualizar = format!(
            "WITH {valores_cte}
             UPDATE lotes_producto AS l SET cantidad = (SELECT d.cantidad FROM d WHERE {MISMO})
             WHERE l.activo = 1
               AND EXISTS (SELECT 1 FROM d WHERE {MISMO})
               AND l.id = (SELECT MIN(x.id) FROM lotes_producto x
                           WHERE x.producto_id = l.producto_id AND x.activo = 1
                             AND date(x.fecha_vencimiento) = date(l.fecha_vencimiento)
                             AND COALESCE(x.numero_lote, '') = COALESCE(l.numero_lote, ''))"
        );
        conn.execute(&actualizar, libsql::params_from_iter(valores.clone()))
            .await
            .map_err(|e| format!("no se pudieron actualizar los lotes: {}", e))?;
        let crear = format!(
            "WITH {valores_cte}
             INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, numero_lote, fecha_ingreso)
             SELECT d.producto_id, d.cantidad, d.vence, d.numero, '{ahora}' FROM d
             WHERE NOT EXISTS (SELECT 1 FROM lotes_producto l WHERE l.activo = 1 AND {MISMO})"
        );
        conn.execute(&crear, libsql::params_from_iter(valores))
            .await
            .map_err(|e| format!("no se pudieron guardar los lotes: {}", e))?;
    }
    Ok(())
}

/// POST /productos/importar
/// Lubricentro: lee la ficha del repuesto y la compatibilidad de cada fila
/// (una por fila, en el mismo orden que `revisadas`). Una fila con esos
/// datos mal escritos pasa a ERROR, como cualquier otro error del archivo.
/// Una fila vacía en esas columnas queda en None: no toca lo que ya había.
pub fn revisar_lubricentro(
    filas: &[FilaImportada],
    revisadas: &mut [Revisada],
    con_repuesto: bool,
    con_compatibilidad: bool,
) -> (
    Vec<Option<crate::handlers::repuestos::FichaRepuesto>>,
    Vec<Option<Vec<crate::handlers::compatibilidad::Compatibilidad>>>,
) {
    let mut fichas = vec![None; revisadas.len()];
    let mut listas = vec![None; revisadas.len()];
    if !con_repuesto && !con_compatibilidad {
        return (fichas, listas);
    }
    for (i, (f, r)) in filas.iter().zip(revisadas.iter_mut()).enumerate() {
        if r.padre.is_some() {
            continue;
        }
        let mut error: Option<String> = None;
        if con_repuesto {
            match f.repuesto.as_ref().map(crate::handlers::repuestos::ficha_desde_importada).unwrap_or(Ok(None)) {
                Ok(ficha) => fichas[i] = ficha,
                Err(e) => error = Some(e),
            }
        }
        if con_compatibilidad && error.is_none() {
            let texto = f.compatible.as_deref().unwrap_or("").trim();
            if !texto.is_empty() {
                match crate::handlers::compatibilidad::desde_texto(texto) {
                    Ok(lista) => listas[i] = Some(lista),
                    Err(e) => error = Some(e),
                }
            }
        }
        if let (Some(e), true) = (error, r.estado != "ERROR") {
            r.estado = "ERROR";
            r.motivo = Some(e);
        }
    }
    // Un lote extra entra solo si entra su producto.
    for i in 0..revisadas.len() {
        if let Some(padre) = revisadas[i].padre {
            if revisadas[i].estado == "LOTE" && revisadas[padre].estado == "ERROR" {
                let fila = revisadas[padre].fila;
                revisadas[i].estado = "ERROR";
                revisadas[i].motivo = Some(format!("El producto de la fila {} tiene un error", fila));
            }
        }
    }
    (fichas, listas)
}

/// código (en minúsculas) -> id, de los productos con esos códigos.
async fn ids_por_codigo(conn: &libsql::Connection, codigos: Vec<String>) -> Result<HashMap<String, i64>, String> {
    let mut ids = HashMap::new();
    for tanda in codigos.chunks(200) {
        let marcas: Vec<String> = (1..=tanda.len()).map(|n| format!("?{}", n)).collect();
        let mut filas = conn
            .query(
                &format!("SELECT id, codigo FROM productos WHERE codigo IN ({})", marcas.join(",")),
                libsql::params_from_iter(tanda.iter().cloned().map(libsql::Value::Text)),
            )
            .await
            .map_err(|e| format!("no se pudieron leer los productos nuevos: {}", e))?;
        while let Some(f) = filas.next().await.map_err(|e| e.to_string())? {
            ids.insert(f.get::<String>(1).unwrap_or_default().to_lowercase(), f.get::<i64>(0).unwrap_or_default());
        }
    }
    Ok(ids)
}

pub async fn importar_productos(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<Importacion>,
) -> Resultado<ResultadoImportacion> {
    exigir_admin(&claims)?;
    if payload.filas.is_empty() {
        return Err(malo("El archivo no tiene productos."));
    }
    if payload.filas.len() > MAXIMO_FILAS {
        return Err(malo(format!(
            "El archivo trae {} filas y el máximo por importación es {}. Divídelo en partes.",
            payload.filas.len(),
            MAXIMO_FILAS
        )));
    }
    let actualizar = match payload.existentes.as_deref().unwrap_or("SALTAR") {
        "SALTAR" => false,
        "ACTUALIZAR" => true,
        _ => return Err(malo("Opción no válida para los productos que ya existen.")),
    };
    let categoria_defecto = payload.categoria_defecto.as_deref().map(limpio).filter(|c| !c.is_empty()).unwrap_or_else(|| "General".to_string());
    let unidad_defecto = payload.unidad_defecto.as_deref().map(|u| u.trim().to_uppercase()).filter(|u| !u.is_empty()).unwrap_or_else(|| "UNIDAD".to_string());
    if !unidad_valida(&unidad_defecto) {
        return Err(malo(format!("Unidad no reconocida: {}", unidad_defecto)));
    }

    let conn = tenant.0.connect().map_err(e500)?;

    // Lo que ya hay: códigos de TODOS los productos (también los
    // desactivados: su código sigue ocupado) y las categorías.
    let mut existentes: HashMap<String, Existente> = HashMap::new();
    let mut maximo_id: i64 = 0;
    {
        let mut filas = conn
            .query("SELECT id, codigo, COALESCE(activo, 1), COALESCE(lleva_vencimiento, 0), CAST(COALESCE(stock, 0) AS REAL) FROM productos", ())
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let id: i64 = f.get(0).unwrap_or_default();
            maximo_id = maximo_id.max(id);
            existentes.insert(
                f.get::<String>(1).unwrap_or_default().trim().to_lowercase(),
                Existente {
                    id,
                    activo: f.get::<i64>(2).unwrap_or(1) == 1,
                    por_lotes: f.get::<i64>(3).unwrap_or(0) == 1,
                    stock: f.get::<f64>(4).unwrap_or(0.0),
                },
            );
        }
    }
    // clave -> (id, nombre, activa)
    let mut categorias_db: HashMap<String, (i64, String, bool)> = HashMap::new();
    {
        let mut filas = conn.query("SELECT id, nombre, COALESCE(activo, 1) FROM categorias", ()).await.map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let nombre: String = f.get(1).unwrap_or_default();
            categorias_db.insert(clave(&nombre), (f.get(0).unwrap_or_default(), nombre, f.get::<i64>(2).unwrap_or(1) == 1));
        }
    }
    let nombres_categorias: HashMap<String, String> = categorias_db.iter().map(|(k, v)| (k.clone(), v.1.clone())).collect();
    let con_ganancias = ganancias::activo(&conn).await;
    // Ficha del medicamento: solo con el módulo encendido y la tabla lista
    // (migración 0034). Si el archivo la trae y la tabla no está, no se
    // importa nada en vez de perder esos datos.
    let con_ficha = crate::handlers::rubros::negocio(&conn).await.modulos.iter().any(|m| m == MODULO_FICHA_MEDICAMENTO);
    if con_ficha
        && payload.filas.iter().any(|f| f.ficha.is_some())
        && conn.query("SELECT 1 FROM producto_medicamento LIMIT 1", ()).await.is_err()
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "La ficha del medicamento aún no está lista en este negocio. Intenta en unos minutos.".to_string(),
        ));
    }

    let hoy = tiempo::hoy_lima();
    let mut revisadas = revisar(&payload.filas, &existentes, &nombres_categorias, &categoria_defecto, &unidad_defecto, con_ganancias, con_ficha, &hoy);

    // Lubricentro / Repuestos: ficha del repuesto y compatibilidades. Solo
    // con el módulo encendido y si el archivo trae esas columnas; si no, la
    // importación es exactamente la de siempre.
    let modulos = crate::handlers::rubros::negocio(&conn).await.modulos;
    let con_repuesto = modulos.iter().any(|m| m == crate::handlers::rubros::MODULO_FICHA_REPUESTO)
        && payload.filas.iter().any(|f| f.repuesto.is_some());
    let con_compatibilidad = modulos.iter().any(|m| m == crate::handlers::rubros::MODULO_COMPATIBILIDAD)
        && payload.filas.iter().any(|f| f.compatible.is_some());
    for (tabla, usar) in [("producto_repuesto", con_repuesto), ("producto_compatibilidad", con_compatibilidad)] {
        if usar && conn.query(&format!("SELECT 1 FROM {} LIMIT 1", tabla), ()).await.is_err() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Los datos de repuestos aún no están listos en este negocio. Intenta en unos minutos.".to_string(),
            ));
        }
    }
    let (fichas_repuesto, compatibilidades) = revisar_lubricentro(&payload.filas, &mut revisadas, con_repuesto, con_compatibilidad);

    let nuevos: Vec<&Revisada> = revisadas.iter().filter(|r| r.estado == "LISTO").collect();
    let a_actualizar: Vec<&Revisada> = if actualizar { revisadas.iter().filter(|r| r.estado == "EXISTE").collect() } else { Vec::new() };
    // Lotes: los de cada fila de producto y los de sus filas "LOTE".
    let estado_producto = |r: &Revisada| match r.padre {
        Some(p) => revisadas[p].estado,
        None => r.estado,
    };
    let con_lote = |estado: &str| revisadas.iter().filter(|r| r.lote.is_some() && r.estado != "ERROR" && estado_producto(r) == estado).count();
    let lotes_nuevos = con_lote("LISTO");
    let lotes_existentes = con_lote("EXISTE");
    let lotes_a_guardar: Vec<&Revisada> = revisadas
        .iter()
        .filter(|r| r.lote.is_some() && r.estado != "ERROR")
        .filter(|r| match estado_producto(r) {
            "LISTO" => true,
            "EXISTE" => actualizar,
            _ => false,
        })
        .collect();
    let mut categorias_nuevas: Vec<String> = Vec::new();
    for r in nuevos.iter().filter(|r| r.categoria_nueva) {
        if !categorias_nuevas.iter().any(|c| clave(c) == clave(&r.categoria)) {
            categorias_nuevas.push(r.categoria.clone());
        }
    }

    let mut resultado = ResultadoImportacion {
        listos: nuevos.len(),
        existentes: revisadas.iter().filter(|r| r.estado == "EXISTE").count(),
        errores: revisadas.iter().filter(|r| r.estado == "ERROR").count(),
        categorias_nuevas: categorias_nuevas.clone(),
        filas: revisadas
            .iter()
            .map(|r| FilaRevisada {
                fila: r.fila,
                estado: r.estado,
                motivo: r.motivo.clone(),
                categoria: r.categoria.clone(),
                categoria_nueva: r.categoria_nueva,
                vencimiento: r.lote.as_ref().map(|l| l.1.clone()),
                lote: r.lote.as_ref().and_then(|l| l.0.clone()),
            })
            .collect(),
        guardado: false,
        creados: 0,
        actualizados: 0,
        lotes_nuevos,
        lotes_existentes,
        lotes_guardados: 0,
    };

    if payload.solo_revisar || (nuevos.is_empty() && a_actualizar.is_empty()) {
        return Ok(Json(resultado));
    }

    // Reporte de ganancias encendido: se anota cómo estaban los productos
    // que se van a actualizar, para recalcular su costo promedio.
    let donde_actualizados = ganancias::Donde::Lista(a_actualizar.iter().filter_map(|r| r.existente.map(|e| e.id)).collect());
    let antes_ganancias = if con_ganancias && !a_actualizar.is_empty() {
        Some(ganancias::foto(&conn, &donde_actualizados).await)
    } else {
        None
    };

    // ---- Guardar: todo en una transacción ----
    let tx = conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
    let guardar: Result<(), String> = async {
        // 1. Categorías que faltan (o que estaban desactivadas).
        let mut id_categoria: HashMap<String, i64> = categorias_db.iter().map(|(k, v)| (k.clone(), v.0)).collect();
        for nombre in &categorias_nuevas {
            tx.execute("INSERT INTO categorias (nombre) VALUES (?1)", libsql::params![nombre.clone()])
                .await
                .map_err(|e| format!("no se pudo crear la categoría \"{}\": {}", nombre, e))?;
            id_categoria.insert(clave(nombre), tx.last_insert_rowid());
        }
        let a_reactivar: Vec<i64> = nuevos
            .iter()
            .filter_map(|r| categorias_db.get(&clave(&r.categoria)))
            .filter(|c| !c.2)
            .map(|c| c.0)
            .collect();
        if !a_reactivar.is_empty() {
            let lista = a_reactivar.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
            tx.execute(&format!("UPDATE categorias SET activo = 1 WHERE id IN ({})", lista), ())
                .await
                .map_err(|e| format!("no se pudo reactivar una categoría: {}", e))?;
        }

        // 2. Productos nuevos, por tandas.
        const COLUMNAS: usize = 10;
        for tanda in nuevos.chunks(FILAS_POR_TANDA) {
            let mut marcas = Vec::with_capacity(tanda.len());
            let mut valores: Vec<libsql::Value> = Vec::with_capacity(tanda.len() * COLUMNAS);
            for (i, r) in tanda.iter().enumerate() {
                let b = i * COLUMNAS;
                marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
                let categoria = *id_categoria.get(&clave(&r.categoria)).ok_or_else(|| format!("no se encontró la categoría \"{}\"", r.categoria))?;
                valores.extend([
                    libsql::Value::Text(r.codigo.clone()),
                    libsql::Value::Text(r.nombre.clone()),
                    r.descripcion.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null),
                    libsql::Value::Real(r.precio),
                    // Perecible: su stock lo ponen los lotes (más abajo).
                    libsql::Value::Real(if r.perecible { 0.0 } else { r.stock.unwrap_or(0.0) }),
                    libsql::Value::Real(r.stock_minimo),
                    libsql::Value::Text(r.unidad.clone()),
                    libsql::Value::Integer(categoria),
                    libsql::Value::Real(r.precio_compra),
                    libsql::Value::Integer(if r.perecible { 1 } else { 0 }),
                ]);
            }
            tx.execute(
                &format!(
                    "INSERT INTO productos (codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida, categoria_id, precio_compra, lleva_vencimiento)
                     VALUES {}",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(|e| format!("no se pudieron guardar los productos: {}", e))?;
        }

        // 3. Los que ya existían: precio y, si el archivo lo trae, stock y
        //    precio de compra. Si la fila trae vencimiento, el producto pasa a
        //    perecible y su stock lo ponen sus lotes. Solo números e ids: van
        //    directo en la sentencia.
        for tanda in a_actualizar.chunks(FILAS_POR_TANDA) {
            let mut sentencias = String::new();
            for r in tanda {
                let Some(e) = r.existente else { continue };
                let mut cambios = vec![format!("precio = {}", r.precio)];
                if r.perecible && !e.por_lotes {
                    cambios.push("lleva_vencimiento = 1".to_string());
                }
                if let (Some(stock), false, false) = (r.stock, e.por_lotes || r.perecible, r.lote.is_some()) {
                    cambios.push(format!("stock = {}", stock));
                } else if r.perecible && !e.por_lotes && r.lote.is_none() {
                    // Medicamento sin stock que pasa a perecible: queda en 0 hasta su primer lote.
                    cambios.push("stock = 0".to_string());
                }
                if r.precio_compra > 0.0 {
                    cambios.push(format!("precio_compra = {}", r.precio_compra));
                }
                sentencias.push_str(&format!(
                    "UPDATE productos SET {}, fecha_actualizacion = datetime('now','localtime') WHERE id = {};\n",
                    cambios.join(", "),
                    e.id
                ));
            }
            if !sentencias.is_empty() {
                tx.execute_batch(&sentencias).await.map_err(|e| format!("no se pudieron actualizar los productos: {}", e))?;
            }
        }

        // 4. Fichas de medicamento: las de los nuevos (por su código) y, al
        //    actualizar, las de los que ya existían (por su id).
        let fichas: Vec<(Option<i64>, String, Ficha)> = nuevos
            .iter()
            .chain(a_actualizar.iter())
            .filter_map(|r| r.ficha.clone().map(|f| (r.existente.map(|e| e.id), r.codigo.clone(), f)))
            .collect();
        if !fichas.is_empty() {
            medicamentos::guardar_varias(&tx, &fichas).await?;
        }

        // 4b. Lubricentro: ficha del repuesto y compatibilidades de los
        //     nuevos y, al actualizar, de los que ya existían. Una fila que no
        //     trae esos datos no toca lo que el producto ya tenía.
        if con_repuesto || con_compatibilidad {
            let guardar_fila = |r: &Revisada| r.padre.is_none() && (r.estado == "LISTO" || (actualizar && r.estado == "EXISTE"));
            let ids = ids_por_codigo(&tx, revisadas.iter().filter(|r| guardar_fila(r) && r.existente.is_none()).map(|r| r.codigo.clone()).collect()).await?;
            let id_de = |r: &Revisada| r.existente.map(|e| e.id).or_else(|| ids.get(&r.codigo.to_lowercase()).copied());
            let fichas: Vec<(i64, crate::handlers::repuestos::FichaRepuesto)> = revisadas
                .iter()
                .zip(fichas_repuesto.iter())
                .filter(|(r, _)| guardar_fila(r))
                .filter_map(|(r, f)| Some((id_de(r)?, f.clone()?)))
                .collect();
            if !fichas.is_empty() {
                crate::handlers::repuestos::guardar_en(&tx, &fichas).await?;
            }
            let listas: Vec<(i64, Vec<crate::handlers::compatibilidad::Compatibilidad>)> = revisadas
                .iter()
                .zip(compatibilidades.iter())
                .filter(|(r, _)| guardar_fila(r))
                .filter_map(|(r, l)| Some((id_de(r)?, l.clone()?)))
                .collect();
            if !listas.is_empty() {
                crate::handlers::compatibilidad::guardar_en(&tx, &listas).await?;
            }
        }

        // 5. Lotes con su vencimiento (el stock lo recalculan los triggers).
        //    Un lote que el producto ya tiene (mismo n.° y vencimiento) toma
        //    la cantidad del archivo en vez de duplicarse: importar dos veces
        //    el mismo archivo deja lo mismo.
        let lotes: Vec<LoteImportado> = lotes_a_guardar
            .iter()
            .filter_map(|r| {
                let (numero, vence) = r.lote.clone()?;
                Some(LoteImportado {
                    producto_id: r.existente.map(|e| e.id),
                    codigo: r.codigo.clone(),
                    numero,
                    vence,
                    cantidad: r.stock.unwrap_or(0.0),
                })
            })
            .collect();
        guardar_lotes(&tx, &lotes).await?;
        Ok(())
    }
    .await;

    match guardar {
        Ok(()) => tx.commit().await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo confirmar la importación: {}. Revisa en Productos si entraron antes de repetirla.", e))
        })?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("No se importó nada: {}.", e)));
        }
    }

    // Reporte de ganancias: el costo de lo nuevo arranca en su precio de
    // compra; lo actualizado se promedia. El stock que se carga ya estaba en
    // la tienda, así que no se anota como compra del mes.
    if con_ganancias {
        let _ = conn
            .execute(
                &format!("UPDATE productos SET costo_promedio = precio_compra WHERE id > {} AND precio_compra > 0 AND costo_promedio IS NULL", maximo_id),
                (),
            )
            .await;
        if let Some(antes) = &antes_ganancias {
            ganancias::conciliar_con(&conn, antes, &donde_actualizados, "IMPORTACION", None, true, false).await;
        }
    }

    resultado.guardado = true;
    resultado.creados = nuevos.len();
    resultado.actualizados = a_actualizar.len();
    resultado.lotes_guardados = lotes_a_guardar.len();
    Ok(Json(resultado))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn f(fecha: &str) -> Option<String> {
        fecha_de_archivo(fecha).map(|d| d.format("%Y-%m-%d").to_string())
    }

    #[test]
    fn lee_las_fechas_de_vencimiento() {
        assert_eq!(f("31/12/2027").as_deref(), Some("2027-12-31"));
        assert_eq!(f("31-12-27").as_deref(), Some("2027-12-31"));
        assert_eq!(f("5.3.2028").as_deref(), Some("2028-03-05"));
        assert_eq!(f("2027-12-31").as_deref(), Some("2027-12-31"));
        assert_eq!(f("2027/02/01").as_deref(), Some("2027-02-01"));
        assert_eq!(f("2027-12-31 00:00:00").as_deref(), Some("2027-12-31"));
        assert_eq!(f("2027-12-31T00:00:00").as_deref(), Some("2027-12-31"));
        assert_eq!(f("20271231").as_deref(), Some("2027-12-31"));
        // Mes y año: último día del mes (así se lee en los empaques).
        assert_eq!(f("12/2027").as_deref(), Some("2027-12-31"));
        assert_eq!(f("02-2028").as_deref(), Some("2028-02-29"));
        assert_eq!(f("2027-11").as_deref(), Some("2027-11-30"));
        assert_eq!(f("DIC-2027").as_deref(), Some("2027-12-31"));
        assert_eq!(f("Set 2027").as_deref(), Some("2027-09-30"));
        assert_eq!(f("diciembre 2027").as_deref(), Some("2027-12-31"));
        assert_eq!(f("15 feb 2028").as_deref(), Some("2028-02-15"));
        // Número de fecha de Excel.
        assert_eq!(f("46752").as_deref(), Some("2027-12-31"));
        // Mes/día solo si no hay otra lectura posible.
        assert_eq!(f("12/31/2027").as_deref(), Some("2027-12-31"));
        assert_eq!(f("03/04/2027").as_deref(), Some("2027-04-03"));
        // Lo que no es una fecha clara.
        for malo in ["", "hola", "31/02/2027", "13/13/2027", "12/27", "1500", "2027", "31/12/1999", "32-01-2027"] {
            assert_eq!(f(malo), None, "{malo}");
        }
    }

    fn fila(n: i64, codigo: &str, stock: Option<f64>, lote: Option<&str>, vence: Option<&str>) -> FilaImportada {
        FilaImportada {
            fila: n,
            codigo: codigo.into(),
            nombre: format!("Producto {codigo}"),
            precio: Some(2.0),
            stock,
            categoria: None,
            unidad: None,
            precio_compra: None,
            stock_minimo: None,
            descripcion: None,
            ficha: None,
            lote: lote.map(Into::into),
            vencimiento: vence.map(Into::into),
            repuesto: None,
            compatible: None,
        }
    }

    #[test]
    fn repuesto_y_compatibilidad_del_archivo() {
        use crate::handlers::repuestos::RepuestoImportado;
        let mut a = fila(2, "FIL-1", Some(4.0), None, None);
        a.repuesto = Some(RepuestoImportado { marca: Some("Wix".into()), codigos_alternos: Some("PH3614".into()), ..Default::default() });
        a.compatible = Some("Toyota Hilux 2005-2015".into());
        let mut b = fila(3, "FIL-2", None, None, None);
        b.repuesto = Some(RepuestoImportado { garantia: Some("para siempre".into()), ..Default::default() });
        let mut c = fila(4, "FIL-3", None, None, None);
        c.compatible = Some("Toyota Corolla 1800".into());
        let d = fila(5, "ACE-1", None, None, None);
        let filas = vec![a, b, c, d];
        let mut r = revisar_simple(&filas, &HashMap::new(), false);
        let (fichas, listas) = revisar_lubricentro(&filas, &mut r, true, true);
        assert_eq!(r[0].estado, "LISTO");
        assert_eq!(fichas[0].as_ref().unwrap().codigos.len(), 1);
        assert_eq!(listas[0].as_ref().unwrap()[0].modelo.as_deref(), Some("Hilux"));
        assert_eq!(r[1].estado, "ERROR");
        assert!(r[1].motivo.as_deref().unwrap().starts_with("Garantía"));
        assert_eq!(r[2].estado, "ERROR");
        // Sin datos de repuesto: producto común, nada que tocar.
        assert_eq!((r[3].estado, fichas[3].is_none(), listas[3].is_none()), ("LISTO", true, true));
        // Sin los módulos, el archivo se revisa como siempre.
        let mut r2 = revisar_simple(&filas, &HashMap::new(), false);
        let (f2, l2) = revisar_lubricentro(&filas, &mut r2, false, false);
        assert!(r2.iter().all(|x| x.estado == "LISTO") && f2.iter().all(Option::is_none) && l2.iter().all(Option::is_none));
    }

    fn revisar_simple(filas: &[FilaImportada], existentes: &HashMap<String, Existente>, con_ficha: bool) -> Vec<Revisada> {
        revisar(filas, existentes, &HashMap::new(), "General", "UNIDAD", false, con_ficha, "2026-10-17")
    }

    #[test]
    fn varias_filas_del_mismo_codigo_son_varios_lotes() {
        let filas = vec![
            fila(2, "A", Some(10.0), Some("l1"), Some("30/06/2027")),
            fila(3, "A", Some(4.0), Some("L2"), Some("12/2027")),
            fila(4, "A", Some(1.0), None, Some("2028-01-31")),
            fila(5, "A", Some(1.0), Some("L1"), Some("31/07/2027")), // mismo n.° con otra fecha
            fila(6, "A", Some(2.0), None, None),                     // sin vencimiento: repetido
            fila(7, "A", None, Some("L9"), Some("31/12/2027")),      // sin cantidad
            fila(8, "A", Some(2.0), Some("L8"), Some("01/01/2026")), // vencido
        ];
        let r = revisar_simple(&filas, &HashMap::new(), false);
        assert_eq!(r[0].estado, "LISTO");
        assert!(r[0].perecible);
        assert_eq!(r[0].lote, Some((Some("L1".into()), "2027-06-30".into())));
        assert_eq!((r[1].estado, r[1].padre), ("LOTE", Some(0)));
        assert_eq!(r[1].lote, Some((Some("L2".into()), "2027-12-31".into())));
        assert_eq!(r[2].estado, "LOTE");
        assert_eq!(r[3].estado, "ERROR");
        assert!(r[3].motivo.as_deref().unwrap().contains("fila 2"));
        assert_eq!(r[4].estado, "ERROR");
        assert!(r[4].motivo.as_deref().unwrap().contains("pon su vencimiento"));
        assert!(r[5].motivo.as_deref().unwrap().contains("cantidad"));
        assert!(r[6].motivo.as_deref().unwrap().contains("Vencido el 01/01/2026"));
    }

    #[test]
    fn sin_vencimiento_el_codigo_repetido_sigue_siendo_error() {
        let filas = vec![fila(2, "B", Some(3.0), None, None), fila(3, "B", Some(3.0), None, Some("31/12/2027"))];
        let r = revisar_simple(&filas, &HashMap::new(), false);
        assert_eq!(r[0].estado, "LISTO");
        assert!(!r[0].perecible);
        assert_eq!(r[1].estado, "ERROR");
        assert_eq!(r[1].motivo.as_deref(), Some("Código repetido en la fila 2"));
    }

    #[test]
    fn el_lote_extra_sigue_a_su_producto() {
        let mut malo = fila(2, "C", Some(3.0), Some("L1"), Some("31/12/2027"));
        malo.precio = None;
        let filas = vec![malo, fila(3, "C", Some(3.0), Some("L2"), Some("31/12/2028"))];
        let r = revisar_simple(&filas, &HashMap::new(), false);
        assert_eq!(r[0].estado, "ERROR");
        assert_eq!(r[1].estado, "ERROR");
        assert_eq!(r[1].motivo.as_deref(), Some("El producto de la fila 2 tiene un error"));
        // Lote sin vencimiento y vencimiento ilegible.
        let r = revisar_simple(&[fila(2, "D", Some(1.0), Some("L1"), None), fila(3, "E", Some(1.0), None, Some("pronto"))], &HashMap::new(), false);
        assert_eq!(r[0].motivo.as_deref(), Some("Falta el vencimiento del lote L1"));
        assert!(r[1].motivo.as_deref().unwrap().starts_with("Vencimiento no reconocido"));
    }

    #[test]
    fn los_lotes_de_un_producto_que_ya_existe() {
        let mut existentes = HashMap::new();
        existentes.insert("x".to_string(), Existente { id: 7, activo: true, por_lotes: true, stock: 3.0 });
        let filas = vec![
            fila(2, "X", Some(5.0), Some("L1"), Some("31/12/2027")),
            fila(3, "X", Some(2.0), Some("L2"), Some("31/12/2028")),
        ];
        let r = revisar_simple(&filas, &existentes, false);
        assert_eq!(r[0].estado, "EXISTE");
        assert_eq!(r[1].estado, "LOTE");
        assert_eq!(r[1].existente.map(|e| e.id), Some(7));
        // Perecible sin vencimiento en el archivo: se avisa que su stock no se carga así.
        let r = revisar_simple(&[fila(2, "X", Some(5.0), None, None)], &existentes, false);
        assert!(r[0].motivo.as_deref().unwrap().contains("va por lotes"));
        // Con stock 0 no hay nada que cargar: no se avisa.
        let r = revisar_simple(&[fila(2, "X", Some(0.0), None, None)], &existentes, false);
        assert_eq!(r[0].motivo.as_deref(), Some("Ya existe ese código"));
    }

    #[test]
    fn un_medicamento_con_stock_necesita_vencimiento() {
        let ficha = FichaImportada { principio_activo: Some("Paracetamol".into()), ..Default::default() };
        let mut con_stock = fila(2, "M1", Some(10.0), None, None);
        con_stock.ficha = Some(ficha.clone());
        let mut sin_stock = fila(3, "M2", None, None, None);
        sin_stock.ficha = Some(ficha.clone());
        let mut con_lote = fila(4, "M3", Some(10.0), Some("A1"), Some("31/12/2027"));
        con_lote.ficha = Some(ficha);
        let r = revisar_simple(&[con_stock.clone(), sin_stock, con_lote], &HashMap::new(), true);
        assert_eq!(r[0].estado, "ERROR");
        assert!(r[0].motivo.as_deref().unwrap().starts_with("Es medicamento"));
        // Sin stock entra igual, ya como perecible (sus compras pedirán vencimiento).
        assert_eq!((r[1].estado, r[1].perecible), ("LISTO", true));
        assert_eq!((r[2].estado, r[2].perecible), ("LISTO", true));
        // Uno que ya existe: pasa a perecible solo si no le queda stock suelto.
        let mut existentes = HashMap::new();
        existentes.insert("m1".to_string(), Existente { id: 1, activo: true, por_lotes: false, stock: 0.0 });
        existentes.insert("m2".to_string(), Existente { id: 2, activo: true, por_lotes: false, stock: 8.0 });
        let mut cero = con_stock.clone();
        cero.stock = Some(0.0);
        let mut m2 = cero.clone();
        m2.codigo = "M2".into();
        m2.stock = None;
        let r = revisar_simple(&[cero, m2], &existentes, true);
        assert_eq!((r[0].estado, r[0].perecible), ("EXISTE", true));
        assert_eq!((r[1].estado, r[1].perecible), ("EXISTE", false));
        // Sin el módulo la ficha no cuenta y no se exige nada.
        let r = revisar_simple(&[con_stock], &HashMap::new(), false);
        assert_eq!((r[0].estado, r[0].perecible), ("LISTO", false));
    }
}
