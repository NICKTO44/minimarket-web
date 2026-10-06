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

use crate::handlers::ganancias;
use crate::handlers::productos::unidad_valida;
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
}

#[derive(Debug, Deserialize)]
pub struct Importacion {
    pub filas: Vec<FilaImportada>,
    /// true = solo decir cómo quedaría (vista previa); no guarda nada.
    #[serde(default)]
    pub solo_revisar: bool,
    /// Qué hacer con los códigos que ya existen: "SALTAR" (por defecto) o
    /// "ACTUALIZAR" (su precio y, si el archivo lo trae, su stock).
    pub existentes: Option<String>,
    /// Categoría para las filas que no traen una.
    pub categoria_defecto: Option<String>,
    /// Unidad para las filas que no traen una.
    pub unidad_defecto: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FilaRevisada {
    pub fila: i64,
    /// "LISTO" | "EXISTE" | "ERROR"
    pub estado: &'static str,
    pub motivo: Option<String>,
    /// La categoría en la que quedaría.
    pub categoria: String,
    pub categoria_nueva: bool,
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
}

/// Revisa todas las filas sin tocar la base.
///
/// `categorias`: clave -> nombre de las que ya existen.
pub fn revisar(
    filas: &[FilaImportada],
    existentes: &HashMap<String, Existente>,
    categorias: &HashMap<String, String>,
    categoria_defecto: &str,
    unidad_defecto: &str,
    exige_precio_compra: bool,
) -> Vec<Revisada> {
    // Primera fila en la que aparece cada código del archivo.
    let mut vistos: HashMap<String, i64> = HashMap::new();
    // Categorías nuevas: se usa el nombre tal como vino la primera vez.
    let mut nuevas: HashMap<String, String> = HashMap::new();

    filas
        .iter()
        .map(|f| {
            let codigo = limpio(&f.codigo);
            let nombre = limpio(&f.nombre);
            let descripcion = f.descripcion.as_deref().map(limpio).filter(|d| !d.is_empty());
            let precio = f.precio.unwrap_or(0.0);
            let stock_minimo = f.stock_minimo.unwrap_or(5.0);
            let precio_compra = f.precio_compra.unwrap_or(0.0);
            let unidad = f.unidad.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(unidad_defecto).to_uppercase();

            let pedida = f.categoria.as_deref().map(limpio).filter(|c| !c.is_empty()).unwrap_or_else(|| limpio(categoria_defecto));
            let clave_categoria = clave(&pedida);
            let (categoria, categoria_nueva) = match categorias.get(&clave_categoria) {
                Some(nombre) => (nombre.clone(), false),
                None => (nuevas.entry(clave_categoria).or_insert_with(|| pedida.clone()).clone(), true),
            };

            let numero_malo = |n: f64, tope: f64| !n.is_finite() || n < 0.0 || n >= tope;
            let clave_codigo = codigo.to_lowercase();
            let repetido = if codigo.is_empty() { None } else { vistos.get(&clave_codigo).copied() };
            if !codigo.is_empty() && repetido.is_none() {
                vistos.insert(clave_codigo.clone(), f.fila);
            }
            let existente = existentes.get(&clave_codigo).copied();

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
            } else if let Some(primera) = repetido {
                Some(format!("Código repetido en la fila {}", primera))
            } else if exige_precio_compra && existente.is_none() && !(precio_compra > 0.0) {
                Some("Falta el precio de compra (el reporte de ganancias está activo)".to_string())
            } else {
                None
            };

            let (estado, motivo) = match (error, existente) {
                (Some(e), _) => ("ERROR", Some(e)),
                (None, Some(e)) => (
                    "EXISTE",
                    Some(if e.activo { "Ya existe ese código".to_string() } else { "Ya existe ese código (producto desactivado)".to_string() }),
                ),
                (None, None) => ("LISTO", None),
            };

            Revisada {
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
            }
        })
        .collect()
}

/// POST /productos/importar
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
            .query("SELECT id, codigo, COALESCE(activo, 1), COALESCE(lleva_vencimiento, 0) FROM productos", ())
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let id: i64 = f.get(0).unwrap_or_default();
            maximo_id = maximo_id.max(id);
            existentes.insert(
                f.get::<String>(1).unwrap_or_default().trim().to_lowercase(),
                Existente { id, activo: f.get::<i64>(2).unwrap_or(1) == 1, por_lotes: f.get::<i64>(3).unwrap_or(0) == 1 },
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

    let revisadas = revisar(&payload.filas, &existentes, &nombres_categorias, &categoria_defecto, &unidad_defecto, con_ganancias);

    let nuevos: Vec<&Revisada> = revisadas.iter().filter(|r| r.estado == "LISTO").collect();
    let a_actualizar: Vec<&Revisada> = if actualizar { revisadas.iter().filter(|r| r.estado == "EXISTE").collect() } else { Vec::new() };
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
            .map(|r| FilaRevisada { fila: r.fila, estado: r.estado, motivo: r.motivo.clone(), categoria: r.categoria.clone(), categoria_nueva: r.categoria_nueva })
            .collect(),
        guardado: false,
        creados: 0,
        actualizados: 0,
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
        const COLUMNAS: usize = 9;
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
                    libsql::Value::Real(r.stock.unwrap_or(0.0)),
                    libsql::Value::Real(r.stock_minimo),
                    libsql::Value::Text(r.unidad.clone()),
                    libsql::Value::Integer(categoria),
                    libsql::Value::Real(r.precio_compra),
                ]);
            }
            tx.execute(
                &format!(
                    "INSERT INTO productos (codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida, categoria_id, precio_compra)
                     VALUES {}",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(|e| format!("no se pudieron guardar los productos: {}", e))?;
        }

        // 3. Los que ya existían: precio y, si el archivo lo trae, stock y
        //    precio de compra. Solo números e ids: van directo en la sentencia.
        for tanda in a_actualizar.chunks(FILAS_POR_TANDA) {
            let mut sentencias = String::new();
            for r in tanda {
                let Some(e) = r.existente else { continue };
                let mut cambios = vec![format!("precio = {}", r.precio)];
                if let (Some(stock), false) = (r.stock, e.por_lotes) {
                    cambios.push(format!("stock = {}", stock));
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
    Ok(Json(resultado))
}
