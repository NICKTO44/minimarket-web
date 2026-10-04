//! Tallas y colores (módulo VARIANTES, rubro Ropa y calzado).
//!
//! Un MODELO ("Polo básico") se vende en varias tallas y colores. Cada
//! talla/color es un producto normal de la tabla `productos`, con su propio
//! código de barras, precio y stock: así el punto de venta, las compras, las
//! devoluciones, los reportes y los comprobantes funcionan igual que con
//! cualquier producto. Lo que los une es `modelo_id` (migración 0017).
//!
//! Aquí solo se crea y se edita un modelo completo de una vez (la cuadrícula
//! de tallas). Quitar una talla es eliminar o desactivar ese producto con las
//! rutas de siempre, que cuidan su historial.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::handlers::ganancias;
use crate::handlers::productos::unidad_valida;
use crate::handlers::rubros::{exigir_modulo, MODULO_VARIANTES};
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

const NOMBRE_MODULO: &str = "Tallas y colores";
const MAXIMO_VARIANTES: usize = 200;
const FILAS_POR_TANDA: usize = 100;
/// Separa el modelo de su talla y su color en el nombre de venta.
const SEPARADOR: &str = " · ";

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(m: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, m.into())
}
/// La base aún no tiene la migración 0017.
fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

#[derive(Debug, Deserialize)]
pub struct VarianteEntrada {
    /// id del producto si esa talla/color ya existe; None = es nueva.
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub talla: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Código de barras propio. Vacío en una variante nueva = se genera uno.
    #[serde(default)]
    pub codigo: Option<String>,
    pub precio: f64,
    /// En una variante nueva, su stock inicial (None = 0). En una que ya
    /// existe, None = no tocar el stock (pudo venderse mientras se editaba).
    #[serde(default)]
    pub stock: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct GuardarModelo {
    pub nombre: String,
    pub categoria_id: i64,
    #[serde(default)]
    pub descripcion: Option<String>,
    #[serde(default)]
    pub unidad_medida: Option<String>,
    #[serde(default)]
    pub precio_compra: Option<f64>,
    #[serde(default)]
    pub stock_minimo: Option<f64>,
    pub variantes: Vec<VarianteEntrada>,
}

#[derive(Debug, Serialize)]
pub struct VarianteGuardada {
    pub id: i64,
    pub codigo: String,
    pub nombre: String,
}

#[derive(Debug, Serialize)]
pub struct ModeloGuardado {
    pub modelo_id: i64,
    pub nombre: String,
    pub variantes: Vec<VarianteGuardada>,
}

/// Nombre con el que se vende una talla/color: "Polo básico · M · Negro".
pub fn nombre_variante(modelo: &str, talla: &str, color: &str) -> String {
    let mut nombre = modelo.trim().to_string();
    for parte in [talla.trim(), color.trim()] {
        if !parte.is_empty() {
            nombre.push_str(SEPARADOR);
            nombre.push_str(parte);
        }
    }
    nombre
}

/// Una variante ya revisada.
struct Limpia {
    id: Option<i64>,
    talla: String,
    color: String,
    codigo: String,
    precio: f64,
    stock: Option<f64>,
}

struct ModeloLimpio {
    nombre: String,
    descripcion: Option<String>,
    unidad: String,
    precio_compra: f64,
    stock_minimo: f64,
    variantes: Vec<Limpia>,
}

fn texto(valor: &Option<String>) -> String {
    valor.as_deref().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Revisa todo lo que no necesita la base.
fn validar(payload: &GuardarModelo) -> Result<ModeloLimpio, String> {
    let nombre = payload.nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if nombre.is_empty() {
        return Err("Escribe el nombre del modelo (por ejemplo: Polo básico).".into());
    }
    if nombre.chars().count() > 120 {
        return Err("El nombre del modelo es demasiado largo (máximo 120 letras).".into());
    }
    let unidad = payload.unidad_medida.clone().unwrap_or_else(|| "UNIDAD".to_string());
    if !unidad_valida(&unidad) {
        return Err(format!("Unidad de medida no reconocida: {}", unidad));
    }
    let precio_compra = payload.precio_compra.unwrap_or(0.0);
    let stock_minimo = payload.stock_minimo.unwrap_or(0.0);
    if !precio_compra.is_finite() || precio_compra < 0.0 || !stock_minimo.is_finite() || stock_minimo < 0.0 {
        return Err("El precio de compra y el stock mínimo no pueden ser negativos.".into());
    }
    if payload.variantes.is_empty() {
        return Err("Agrega al menos una talla o un color.".into());
    }
    if payload.variantes.len() > MAXIMO_VARIANTES {
        return Err(format!("Un modelo admite hasta {} combinaciones de talla y color.", MAXIMO_VARIANTES));
    }

    let mut combinaciones = HashSet::new();
    let mut codigos = HashSet::new();
    let mut variantes = Vec::with_capacity(payload.variantes.len());
    for v in &payload.variantes {
        let (talla, color) = (texto(&v.talla), texto(&v.color));
        if talla.is_empty() && color.is_empty() {
            return Err("Cada fila necesita una talla o un color.".into());
        }
        if talla.chars().count() > 20 || color.chars().count() > 30 {
            return Err("La talla admite hasta 20 letras y el color hasta 30.".into());
        }
        let etiqueta = nombre_variante("", &talla, &color);
        let etiqueta = etiqueta.trim_start_matches(SEPARADOR);
        if !v.precio.is_finite() || v.precio <= 0.0 {
            return Err(format!("Falta el precio de {} (debe ser mayor a 0).", etiqueta));
        }
        if let Some(stock) = v.stock {
            if !stock.is_finite() || stock < 0.0 {
                return Err(format!("El stock de {} no puede ser negativo.", etiqueta));
            }
        }
        if !combinaciones.insert((talla.to_lowercase(), color.to_lowercase())) {
            return Err(format!("{} está repetido.", etiqueta));
        }
        let codigo = texto(&v.codigo);
        if codigo.chars().count() > 60 {
            return Err(format!("El código de {} es demasiado largo.", etiqueta));
        }
        if !codigo.is_empty() && !codigos.insert(codigo.to_lowercase()) {
            return Err(format!("El código {} está en más de una fila: cada talla y color lleva el suyo.", codigo));
        }
        if codigo.is_empty() && v.id.is_some() {
            return Err(format!("{} no puede quedar sin código.", etiqueta));
        }
        variantes.push(Limpia { id: v.id, talla, color, codigo, precio: v.precio, stock: v.stock });
    }

    Ok(ModeloLimpio {
        nombre,
        descripcion: Some(texto(&payload.descripcion)).filter(|d| !d.is_empty()),
        unidad,
        precio_compra,
        stock_minimo,
        variantes,
    })
}

/// La categoría existe y ningún OTRO modelo se llama igual.
async fn revisar_modelo(
    conn: &libsql::Connection,
    nombre: &str,
    categoria_id: i64,
    modelo_id: i64,
) -> Result<(), (StatusCode, String)> {
    // En una sola consulta: si la categoría existe ('C') y los nombres de los
    // demás modelos ('M'). Se comparan aquí porque lower() de SQLite no
    // entiende tildes ("BÁSICO" y "básico" le parecen distintos).
    let mut filas = conn
        .query(
            "SELECT 'C', CAST(COUNT(*) AS TEXT) FROM categorias WHERE id = ?1
             UNION ALL
             SELECT DISTINCT 'M', modelo_nombre FROM productos
              WHERE modelo_id IS NOT NULL AND modelo_id != ?2 AND activo = 1 AND modelo_nombre IS NOT NULL",
            libsql::params![categoria_id, modelo_id],
        )
        .await
        .map_err(actualizando)?;
    let (mut categorias, mut repetidos) = (0, 0);
    let buscado = nombre.to_lowercase();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let (tipo, valor): (String, String) = (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default());
        if tipo == "C" {
            categorias = valor.parse::<i64>().unwrap_or(0);
        } else if valor.to_lowercase() == buscado {
            repetidos += 1;
        }
    }
    if categorias == 0 {
        return Err(malo("Elige la categoría del modelo."));
    }
    if repetidos > 0 {
        return Err((StatusCode::CONFLICT, format!("Ya existe un modelo llamado \"{}\". Ábrelo para agregarle tallas o colores.", nombre)));
    }
    Ok(())
}

/// Ningún código puede estar ya en otro producto (activo o desactivado).
/// `propios`: código -> id del producto que ya lo tiene dentro de este modelo.
async fn revisar_codigos(
    conn: &libsql::Connection,
    variantes: &[Limpia],
) -> Result<(), (StatusCode, String)> {
    let codigos: Vec<libsql::Value> = variantes
        .iter()
        .filter(|v| !v.codigo.is_empty())
        .map(|v| libsql::Value::Text(v.codigo.clone()))
        .collect();
    if codigos.is_empty() {
        return Ok(());
    }
    let marcas = (1..=codigos.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(
            &format!("SELECT id, codigo, nombre, activo FROM productos WHERE codigo IN ({})", marcas),
            libsql::params_from_iter(codigos),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        let codigo: String = f.get(1).unwrap_or_default();
        // El código es de esa misma variante: no cambió.
        if variantes.iter().any(|v| v.id == Some(id) && v.codigo == codigo) {
            continue;
        }
        let nombre: String = f.get(2).unwrap_or_default();
        let activo: i64 = f.get(3).unwrap_or(1);
        return Err((
            StatusCode::CONFLICT,
            if activo == 0 {
                format!("El código {} pertenece a \"{}\", que está desactivado. Reactívalo o usa otro código.", codigo, nombre)
            } else {
                format!("El código {} ya lo usa \"{}\". Cada talla y color lleva su propio código.", codigo, nombre)
            },
        ));
    }
    Ok(())
}

/// Inserta las variantes nuevas de un modelo (las que no traen id).
async fn insertar_nuevas(
    conn: &libsql::Connection,
    modelo_id: i64,
    modelo: &ModeloLimpio,
    categoria_id: i64,
) -> Result<(), (StatusCode, String)> {
    const COLUMNAS: usize = 13;
    let nuevas: Vec<&Limpia> = modelo.variantes.iter().filter(|v| v.id.is_none()).collect();
    for tanda in nuevas.chunks(FILAS_POR_TANDA) {
        let mut marcas = Vec::with_capacity(tanda.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(tanda.len() * COLUMNAS);
        for (i, v) in tanda.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
            valores.extend([
                libsql::Value::Text(v.codigo.clone()),
                libsql::Value::Text(nombre_variante(&modelo.nombre, &v.talla, &v.color)),
                modelo.descripcion.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null),
                libsql::Value::Real(v.precio),
                libsql::Value::Real(v.stock.unwrap_or(0.0)),
                libsql::Value::Real(modelo.stock_minimo),
                libsql::Value::Text(modelo.unidad.clone()),
                libsql::Value::Integer(categoria_id),
                libsql::Value::Real(modelo.precio_compra),
                libsql::Value::Integer(modelo_id),
                libsql::Value::Text(modelo.nombre.clone()),
                libsql::Value::Text(v.talla.clone()),
                libsql::Value::Text(v.color.clone()),
            ]);
        }
        conn.execute(
            &format!(
                "INSERT INTO productos (codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida,
                                        categoria_id, precio_compra, modelo_id, modelo_nombre, talla, color)
                 VALUES {}",
                marcas.join(", ")
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudieron guardar las tallas: {}", e)))?;
    }
    Ok(())
}

/// A las variantes nuevas sin código se les pone uno propio del modelo
/// ("M12-01", "M12-02"...), saltando los que ya estén usados.
async fn completar_codigos(
    conn: &libsql::Connection,
    modelo_id: i64,
    variantes: &mut [Limpia],
) -> Result<(), (StatusCode, String)> {
    if variantes.iter().all(|v| !v.codigo.is_empty()) {
        return Ok(());
    }
    let prefijo = format!("M{}-", modelo_id);
    let mut usados: HashSet<String> = variantes.iter().map(|v| v.codigo.to_lowercase()).collect();
    let mut filas = conn
        .query("SELECT codigo FROM productos WHERE codigo LIKE ?1", libsql::params![format!("{}%", prefijo)])
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        usados.insert(f.get::<String>(0).unwrap_or_default().to_lowercase());
    }
    let mut siguiente = 1;
    for v in variantes.iter_mut().filter(|v| v.codigo.is_empty()) {
        loop {
            let candidato = format!("{}{:02}", prefijo, siguiente);
            siguiente += 1;
            if usados.insert(candidato.to_lowercase()) {
                v.codigo = candidato;
                break;
            }
        }
    }
    Ok(())
}

/// Las variantes del modelo como quedaron en la base.
async fn guardado(conn: &libsql::Connection, modelo_id: i64, nombre: String) -> Resultado<ModeloGuardado> {
    let mut filas = conn
        .query("SELECT id, codigo, nombre FROM productos WHERE modelo_id = ?1 ORDER BY id", libsql::params![modelo_id])
        .await
        .map_err(e500)?;
    let mut variantes = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        variantes.push(VarianteGuardada {
            id: f.get(0).unwrap_or_default(),
            codigo: f.get(1).unwrap_or_default(),
            nombre: f.get(2).unwrap_or_default(),
        });
    }
    Ok(Json(ModeloGuardado { modelo_id, nombre, variantes }))
}

/// POST /modelos — crea un modelo con todas sus tallas y colores.
pub async fn crear_modelo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<GuardarModelo>,
) -> Resultado<ModeloGuardado> {
    let mut modelo = validar(&payload).map_err(malo)?;
    if modelo.variantes.iter().any(|v| v.id.is_some()) {
        return Err(malo("Un modelo nuevo no puede traer tallas que ya existen."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_VARIANTES, NOMBRE_MODULO).await?;
    // Reporte de ganancias encendido: el precio de compra es obligatorio.
    let con_ganancias = ganancias::activo(&conn).await;
    if con_ganancias {
        ganancias::exigir_precio(true, Some(modelo.precio_compra))?;
    }
    revisar_modelo(&conn, &modelo.nombre, payload.categoria_id, 0).await?;
    revisar_codigos(&conn, &modelo.variantes).await?;

    let modelo_id: i64 = {
        let mut filas = conn.query("SELECT COALESCE(MAX(modelo_id), 0) + 1 FROM productos", ()).await.map_err(actualizando)?;
        match filas.next().await.map_err(e500)? {
            Some(f) => f.get(0).unwrap_or(1),
            None => 1,
        }
    };
    completar_codigos(&conn, modelo_id, &mut modelo.variantes).await?;
    insertar_nuevas(&conn, modelo_id, &modelo, payload.categoria_id).await?;
    if con_ganancias {
        ganancias::conciliar(&conn, &ganancias::Foto::new(), &ganancias::Donde::Modelo(modelo_id), "MODELO", None, true).await;
    }
    guardado(&conn, modelo_id, modelo.nombre).await
}

/// PUT /modelos/:id — cambia los datos del modelo, los de sus tallas y
/// agrega las tallas o colores nuevos. Las que no vienen no se tocan.
pub async fn actualizar_modelo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(modelo_id): Path<i64>,
    Json(payload): Json<GuardarModelo>,
) -> Resultado<ModeloGuardado> {
    let mut modelo = validar(&payload).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_VARIANTES, NOMBRE_MODULO).await?;

    // Cómo está hoy cada variante: id -> (código, precio, talla, color).
    let mut actuales: HashMap<i64, (String, f64, String, String)> = HashMap::new();
    let mut filas = conn
        .query(
            "SELECT id, codigo, CAST(precio AS REAL), COALESCE(talla, ''), COALESCE(color, '')
             FROM productos WHERE modelo_id = ?1",
            libsql::params![modelo_id],
        )
        .await
        .map_err(actualizando)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        actuales.insert(
            f.get(0).unwrap_or_default(),
            (f.get(1).unwrap_or_default(), f.get(2).unwrap_or(0.0), f.get(3).unwrap_or_default(), f.get(4).unwrap_or_default()),
        );
    }
    if actuales.is_empty() {
        return Err((StatusCode::NOT_FOUND, "Ese modelo no existe.".into()));
    }
    if modelo.variantes.iter().any(|v| v.id.map(|id| !actuales.contains_key(&id)).unwrap_or(false)) {
        return Err(malo("Una de las tallas no pertenece a este modelo. Vuelve a abrirlo."));
    }
    // Una talla/color nueva no puede repetir una que ya existe y no vino en el formulario.
    for nueva in modelo.variantes.iter().filter(|v| v.id.is_none()) {
        let repetida = actuales.iter().any(|(id, (_, _, talla, color))| {
            !modelo.variantes.iter().any(|v| v.id == Some(*id))
                && talla.to_lowercase() == nueva.talla.to_lowercase()
                && color.to_lowercase() == nueva.color.to_lowercase()
        });
        if repetida {
            return Err(malo(format!(
                "{} ya existe en este modelo (si está desactivada, reactívala desde Desactivados).",
                nombre_variante("", &nueva.talla, &nueva.color).trim_start_matches(SEPARADOR)
            )));
        }
    }

    revisar_modelo(&conn, &modelo.nombre, payload.categoria_id, modelo_id).await?;
    revisar_codigos(&conn, &modelo.variantes).await?;

    // Reporte de ganancias encendido: precio de compra obligatorio, y se
    // anota cómo estaba cada talla para recalcular su costo promedio.
    let antes_ganancias = if ganancias::activo(&conn).await {
        ganancias::exigir_precio(true, Some(modelo.precio_compra))?;
        Some(ganancias::foto(&conn, &ganancias::Donde::Modelo(modelo_id)).await)
    } else {
        None
    };

    // Solo se escribe lo que cambió en cada talla (un viaje por cada una).
    for v in &modelo.variantes {
        let Some(id) = v.id else { continue };
        let (codigo, precio, talla, color) = &actuales[&id];
        let cambio_datos = *codigo != v.codigo || (*precio - v.precio).abs() > 1e-9 || *talla != v.talla || *color != v.color;
        if cambio_datos {
            conn.execute(
                "UPDATE productos SET codigo = ?1, precio = ?2, talla = ?3, color = ?4 WHERE id = ?5",
                libsql::params![v.codigo.clone(), v.precio, v.talla.clone(), v.color.clone(), id],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar: {}", e)))?;
        }
        if let Some(stock) = v.stock {
            conn.execute("UPDATE productos SET stock = ?1 WHERE id = ?2", libsql::params![stock, id])
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar el stock: {}", e)))?;
        }
    }

    completar_codigos(&conn, modelo_id, &mut modelo.variantes).await?;
    insertar_nuevas(&conn, modelo_id, &modelo, payload.categoria_id).await?;

    // Lo que comparten todas: nombre del modelo, categoría, unidad, costo. El
    // nombre de venta de cada una se arma de nuevo con su talla y color.
    conn.execute(
        "UPDATE productos SET
            modelo_nombre = ?1,
            nombre = ?1
                || CASE WHEN COALESCE(talla, '') != '' THEN ?7 || talla ELSE '' END
                || CASE WHEN COALESCE(color, '') != '' THEN ?7 || color ELSE '' END,
            descripcion = ?2, categoria_id = ?3, unidad_medida = ?4, precio_compra = ?5, stock_minimo = ?6,
            fecha_actualizacion = datetime('now','localtime')
         WHERE modelo_id = ?8",
        libsql::params![
            modelo.nombre.clone(), modelo.descripcion.clone(), payload.categoria_id, modelo.unidad.clone(),
            modelo.precio_compra, modelo.stock_minimo, SEPARADOR, modelo_id
        ],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al actualizar el modelo: {}", e)))?;

    if let Some(antes) = &antes_ganancias {
        ganancias::conciliar(&conn, antes, &ganancias::Donde::Modelo(modelo_id), "MODELO", None, true).await;
    }

    guardado(&conn, modelo_id, modelo.nombre).await
}

/// POST /modelos/:id/imagen/:producto_id — la foto que se subió a una
/// talla pasa a ser la de todo el modelo.
pub async fn compartir_imagen(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path((modelo_id, producto_id)): Path<(i64, i64)>,
) -> Resultado<serde_json::Value> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_VARIANTES, NOMBRE_MODULO).await?;
    let cambiadas = conn
        .execute(
            "UPDATE productos SET imagen_url = (SELECT o.imagen_url FROM productos o WHERE o.id = ?2 AND o.modelo_id = ?1)
             WHERE modelo_id = ?1
               AND EXISTS (SELECT 1 FROM productos o WHERE o.id = ?2 AND o.modelo_id = ?1 AND o.imagen_url IS NOT NULL)",
            libsql::params![modelo_id, producto_id],
        )
        .await
        .map_err(actualizando)?;
    Ok(Json(serde_json::json!({ "success": true, "actualizadas": cambiadas })))
}
