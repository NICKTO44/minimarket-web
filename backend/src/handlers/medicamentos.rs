//! Módulo FICHA_MEDICAMENTO (migración 0034): los datos de un producto que
//! es medicamento. Pensado para el rubro Farmacia / Botica; sirve a
//! cualquier negocio que lo encienda.
//!
//! La ficha vive en su propia tabla (producto_medicamento): un producto sin
//! ficha es un producto común. Nada de esto cambia cómo se vende, cobra o
//! descuenta stock; el control de recetas y las alternativas son módulos
//! aparte que leen esta ficha.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET    /medicamentos/fichas        -> todas, para el punto de venta y Productos
//!   PUT    /productos/:id/ficha        -> crea o reemplaza la de un producto
//!   DELETE /productos/:id/ficha        -> el producto deja de ser medicamento

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_FICHA_MEDICAMENTO};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Ficha del medicamento";

/// Condiciones de venta (como en el registro sanitario).
pub const CONDICIONES_VENTA: &[&str] = &["SIN_RECETA", "CON_RECETA", "RECETA_RETENIDA"];

/// Formas farmacéuticas. Debe coincidir con frontend/src/utils/medicamentos.js.
pub const FORMAS: &[&str] = &[
    "TABLETA", "TABLETA_RECUBIERTA", "CAPSULA", "JARABE", "SUSPENSION", "SOLUCION", "GOTAS",
    "INYECTABLE", "CREMA", "UNGUENTO", "GEL", "OVULO", "SUPOSITORIO", "INHALADOR", "POLVO",
    "PARCHE", "COLIRIO", "SPRAY", "OTRA",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ficha {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    pub principio_activo: String,
    #[serde(default)]
    pub concentracion: Option<String>,
    #[serde(default)]
    pub forma_farmaceutica: Option<String>,
    #[serde(default)]
    pub laboratorio: Option<String>,
    #[serde(default)]
    pub registro_sanitario: Option<String>,
    pub condicion_venta: String,
    #[serde(default)]
    pub es_generico: bool,
    /// Solo al leer: para comparar equivalentes (módulo ALTERNATIVAS).
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub principio_clave: Option<String>,
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub concentracion_clave: Option<String>,
    /// Solo en la importación: lo que dijo la columna "Genérico" del archivo
    /// (None = el archivo no lo indica y, al actualizar, se deja como estaba).
    #[serde(skip)]
    pub generico_del_archivo: Option<bool>,
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

/// Minúsculas y sin tildes.
fn plano(texto: &str) -> String {
    texto
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

/// "Paracetamol + Cafeína" y "cafeina+paracetamol" -> "cafeina + paracetamol".
pub fn clave_principio(principio: &str) -> String {
    let mut partes: Vec<String> = plano(principio)
        .split(['+', '/', ','])
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect();
    partes.sort();
    partes.dedup();
    partes.join(" + ")
}

/// "500 MG" -> "500mg"; "250 mg / 5 ml" -> "250mg/5ml"; la coma decimal como punto.
pub fn clave_concentracion(concentracion: &str) -> String {
    plano(concentracion).chars().filter(|c| !c.is_whitespace()).map(|c| if c == ',' { '.' } else { c }).collect()
}

/// Revisa y ordena lo que manda el formulario del producto.
pub fn validar(f: Ficha) -> Result<Ficha, String> {
    let principio_activo = texto(Some(&f.principio_activo), 150, "Principio activo")?
        .filter(|p| p.chars().count() >= 3)
        .ok_or("Escribe el principio activo (DCI), por ejemplo \"Paracetamol\".")?;
    let condicion_venta = f.condicion_venta.trim().to_uppercase();
    if !CONDICIONES_VENTA.contains(&condicion_venta.as_str()) {
        return Err("Elige la condición de venta: sin receta, con receta o receta retenida.".into());
    }
    let forma_farmaceutica = match texto(f.forma_farmaceutica.as_deref(), 30, "Forma farmacéutica")? {
        Some(forma) => {
            let forma = forma.to_uppercase();
            if !FORMAS.contains(&forma.as_str()) {
                return Err("Forma farmacéutica no válida.".into());
            }
            Some(forma)
        }
        None => None,
    };
    Ok(Ficha {
        producto_id: None,
        principio_activo,
        concentracion: texto(f.concentracion.as_deref(), 40, "Concentración")?,
        forma_farmaceutica,
        laboratorio: texto(f.laboratorio.as_deref(), 80, "Laboratorio")?,
        registro_sanitario: texto(f.registro_sanitario.as_deref(), 30, "Registro sanitario")?.map(|r| r.to_uppercase()),
        condicion_venta,
        es_generico: f.es_generico,
        principio_clave: None,
        concentracion_clave: None,
        generico_del_archivo: f.generico_del_archivo,
    })
}

/// La ficha tal como viene de un archivo (importación desde Excel): textos
/// libres, sin códigos ("Tableta", "con receta").
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FichaImportada {
    #[serde(default)]
    pub principio_activo: Option<String>,
    #[serde(default)]
    pub concentracion: Option<String>,
    #[serde(default)]
    pub forma_farmaceutica: Option<String>,
    #[serde(default)]
    pub laboratorio: Option<String>,
    #[serde(default)]
    pub registro_sanitario: Option<String>,
    #[serde(default)]
    pub condicion_venta: Option<String>,
    /// "Sí" / "No" (vacío = no se indica).
    #[serde(default)]
    pub es_generico: Option<String>,
}

/// "Sí", "si", "x", "genérico" -> Some(true); "No", "marca" -> Some(false);
/// vacío -> None (no se indica). Err si no se entiende.
pub fn generico_desde_texto(texto: &str) -> Result<Option<bool>, String> {
    let t: String = plano(texto).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    match t.as_str() {
        "" => Ok(None),
        "si" | "s" | "x" | "1" | "true" | "yes" | "generico" | "gen" | "g" => Ok(Some(true)),
        "no" | "n" | "0" | "false" | "marca" | "demarca" | "comercial" => Ok(Some(false)),
        _ => Err(format!("Genérico no reconocido: \"{}\" (escribe Sí o No)", texto.trim())),
    }
}

/// "Tableta", "TAB", "tabletas recubiertas" -> "TABLETA"/"TABLETA_RECUBIERTA".
/// None si no se reconoce.
pub fn forma_desde_texto(texto: &str) -> Option<&'static str> {
    let t: String = plano(texto).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if t.is_empty() {
        return None;
    }
    if let Some(f) = FORMAS.iter().find(|f| f.to_lowercase().replace('_', "") == t) {
        return Some(f);
    }
    let empieza = |prefijos: &[&str]| prefijos.iter().any(|p| t.starts_with(p));
    Some(match () {
        _ if t.contains("recubierta") => "TABLETA_RECUBIERTA",
        _ if empieza(&["tab", "comp", "pastilla"]) => "TABLETA",
        _ if empieza(&["cap"]) => "CAPSULA",
        _ if empieza(&["jbe", "jarabe"]) => "JARABE",
        _ if empieza(&["susp"]) => "SUSPENSION",
        _ if empieza(&["sol"]) => "SOLUCION",
        _ if empieza(&["got"]) => "GOTAS",
        _ if empieza(&["iny", "amp", "vial"]) => "INYECTABLE",
        _ if empieza(&["crem"]) => "CREMA",
        _ if empieza(&["ungu", "pomada"]) => "UNGUENTO",
        _ if empieza(&["gel"]) => "GEL",
        _ if empieza(&["ovulo"]) => "OVULO",
        _ if empieza(&["supos"]) => "SUPOSITORIO",
        _ if empieza(&["inhal", "aerosol"]) => "INHALADOR",
        _ if empieza(&["polvo"]) => "POLVO",
        _ if empieza(&["parche"]) => "PARCHE",
        _ if empieza(&["colir", "oftal"]) => "COLIRIO",
        _ if empieza(&["spray", "nebul"]) => "SPRAY",
        _ if empieza(&["otr"]) => "OTRA",
        _ => return None,
    })
}

/// "con receta", "Receta médica", "retenida", "controlado", "libre", "" ->
/// condición de venta. None si no se reconoce.
pub fn condicion_desde_texto(texto: &str) -> Option<&'static str> {
    let t: String = plano(texto).chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    match t.as_str() {
        "" | "no" | "sinreceta" | "libre" | "ventalibre" | "otc" | "sinrecetamedica" | "ventasinreceta" => Some("SIN_RECETA"),
        "si" | "conreceta" | "receta" | "recetamedica" | "conrecetamedica" | "ventaconreceta" | "rx" => Some("CON_RECETA"),
        _ if t.contains("retenid") || t.contains("especial") || t.contains("control") || t.contains("psicotrop") || t.contains("estupefac") => {
            Some("RECETA_RETENIDA")
        }
        _ if t.starts_with("sin") => Some("SIN_RECETA"),
        _ if t.contains("receta") => Some("CON_RECETA"),
        _ => None,
    }
}

/// Ficha de una fila importada. Ok(None) si la fila no trae principio activo
/// ni ningún otro dato de la ficha (es un producto común).
pub fn ficha_desde_importada(f: &FichaImportada) -> Result<Option<Ficha>, String> {
    let lleno = |v: &Option<String>| v.as_deref().is_some_and(|t| !t.trim().is_empty());
    let principio = f.principio_activo.as_deref().map(str::trim).unwrap_or("");
    if principio.is_empty() {
        if [&f.concentracion, &f.forma_farmaceutica, &f.laboratorio, &f.registro_sanitario].iter().any(|v| lleno(v)) {
            return Err("Trae datos del medicamento pero falta el principio activo".into());
        }
        return Ok(None);
    }
    let forma = match f.forma_farmaceutica.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(texto) => Some(forma_desde_texto(texto).ok_or_else(|| format!("Forma farmacéutica no reconocida: {}", texto))?.to_string()),
        None => None,
    };
    let condicion = condicion_desde_texto(f.condicion_venta.as_deref().unwrap_or(""))
        .ok_or_else(|| format!("Condición de venta no reconocida: {}", f.condicion_venta.as_deref().unwrap_or("")))?;
    let generico = generico_desde_texto(f.es_generico.as_deref().unwrap_or(""))?;
    validar(Ficha {
        producto_id: None,
        principio_activo: principio.to_string(),
        concentracion: f.concentracion.clone(),
        forma_farmaceutica: forma,
        laboratorio: f.laboratorio.clone(),
        registro_sanitario: f.registro_sanitario.clone(),
        condicion_venta: condicion.to_string(),
        es_generico: generico.unwrap_or(false),
        principio_clave: None,
        concentracion_clave: None,
        generico_del_archivo: generico,
    })
    .map(Some)
}

/// Guarda (o reemplaza) fichas en una sola sentencia, dentro de una
/// transacción. Cada ítem: (id del producto si ya existe, código si es nuevo,
/// ficha ya validada).
pub async fn guardar_varias(conn: &libsql::Connection, fichas: &[(Option<i64>, String, Ficha)]) -> Result<(), String> {
    const COLUMNAS: usize = 11;
    let ahora = tiempo::ahora_lima();
    for tanda in fichas.chunks(80) {
        let mut marcas = Vec::with_capacity(tanda.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(tanda.len() * COLUMNAS);
        let texto = |v: &Option<String>| v.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null);
        for (i, (id, codigo, f)) in tanda.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
            valores.extend([
                id.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null),
                libsql::Value::Text(codigo.clone()),
                libsql::Value::Text(f.principio_activo.clone()),
                libsql::Value::Text(clave_principio(&f.principio_activo)),
                texto(&f.concentracion),
                texto(&f.concentracion.as_deref().map(clave_concentracion)),
                texto(&f.forma_farmaceutica),
                texto(&f.laboratorio),
                texto(&f.registro_sanitario),
                libsql::Value::Text(f.condicion_venta.clone()),
                // Sin la columna "Genérico" en el archivo: NULL (se deja como estaba).
                f.generico_del_archivo.map(|g| libsql::Value::Integer(g as i64)).unwrap_or(libsql::Value::Null),
            ]);
        }
        let sql = format!(
            "WITH v(pid, codigo, principio, pclave, conc, cclave, forma, lab, registro, condicion, generico) AS (VALUES {})
             INSERT INTO producto_medicamento (producto_id, principio_activo, principio_clave, concentracion, concentracion_clave,
                forma_farmaceutica, laboratorio, registro_sanitario, condicion_venta, es_generico, fecha_actualizacion)
             SELECT p.id, v.principio, v.pclave, v.conc, v.cclave, v.forma, v.lab, v.registro, v.condicion, COALESCE(v.generico, -1), '{}'
             FROM v JOIN productos p ON (v.pid IS NOT NULL AND p.id = v.pid) OR (v.pid IS NULL AND p.codigo = v.codigo)
             WHERE 1
             ON CONFLICT(producto_id) DO UPDATE SET
                principio_activo = excluded.principio_activo, principio_clave = excluded.principio_clave,
                concentracion = excluded.concentracion, concentracion_clave = excluded.concentracion_clave,
                forma_farmaceutica = excluded.forma_farmaceutica, laboratorio = excluded.laboratorio,
                registro_sanitario = excluded.registro_sanitario, condicion_venta = excluded.condicion_venta,
                es_generico = CASE WHEN excluded.es_generico = -1 THEN producto_medicamento.es_generico ELSE excluded.es_generico END,
                fecha_actualizacion = excluded.fecha_actualizacion",
            marcas.join(", "),
            ahora
        );
        conn.execute(&sql, libsql::params_from_iter(valores))
            .await
            .map_err(|e| format!("no se pudieron guardar las fichas de medicamento: {}", e))?;
    }
    // -1 marcó "el archivo no dice si es genérico": las fichas nuevas quedan
    // como no genéricas (las que ya existían conservaron su valor arriba).
    conn.execute("UPDATE producto_medicamento SET es_generico = 0 WHERE es_generico = -1", ())
        .await
        .map_err(|e| format!("no se pudieron guardar las fichas de medicamento: {}", e))?;
    Ok(())
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_FICHA_MEDICAMENTO, NOMBRE_MODULO).await
}

fn sin_migracion(e: libsql::Error) -> Fallo {
    if e.to_string().contains("no such table") {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "La ficha del medicamento aún no está lista en este negocio. Intenta en unos minutos.".to_string(),
        )
    } else {
        e500(e)
    }
}

/// GET /medicamentos/fichas — las fichas de los productos activos.
pub async fn todas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Ficha>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut fichas = Vec::new();
    // Sin la migración 0034 no hay tabla: lista vacía, como si nada.
    if let Ok(mut filas) = conn
        .query(
            "SELECT m.producto_id, m.principio_activo, m.concentracion, m.forma_farmaceutica, m.laboratorio,
                    m.registro_sanitario, m.condicion_venta, m.es_generico, m.principio_clave, m.concentracion_clave
             FROM producto_medicamento m JOIN productos p ON p.id = m.producto_id
             WHERE p.activo = 1
             ORDER BY m.producto_id",
            (),
        )
        .await
    {
        while let Some(f) = filas.next().await.map_err(e500)? {
            fichas.push(Ficha {
                producto_id: f.get(0).ok(),
                principio_activo: f.get(1).unwrap_or_default(),
                concentracion: f.get(2).ok(),
                forma_farmaceutica: f.get(3).ok(),
                laboratorio: f.get(4).ok(),
                registro_sanitario: f.get(5).ok(),
                condicion_venta: f.get(6).unwrap_or_else(|_| "SIN_RECETA".into()),
                es_generico: f.get::<i64>(7).unwrap_or(0) == 1,
                principio_clave: f.get(8).ok(),
                concentracion_clave: f.get(9).ok(),
                generico_del_archivo: None,
            });
        }
    }
    Ok(Json(fichas))
}

/// PUT /productos/:id/ficha — crea o reemplaza la ficha del producto.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(ficha): Json<Ficha>,
) -> Result<Json<Ficha>, Fallo> {
    let ficha = validar(ficha).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    conn.execute(
        "INSERT INTO producto_medicamento (producto_id, principio_activo, principio_clave, concentracion, concentracion_clave,
            forma_farmaceutica, laboratorio, registro_sanitario, condicion_venta, es_generico, fecha_actualizacion)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(producto_id) DO UPDATE SET
            principio_activo = excluded.principio_activo, principio_clave = excluded.principio_clave,
            concentracion = excluded.concentracion, concentracion_clave = excluded.concentracion_clave,
            forma_farmaceutica = excluded.forma_farmaceutica, laboratorio = excluded.laboratorio,
            registro_sanitario = excluded.registro_sanitario, condicion_venta = excluded.condicion_venta,
            es_generico = excluded.es_generico, fecha_actualizacion = excluded.fecha_actualizacion",
        libsql::params![
            id,
            ficha.principio_activo.clone(),
            clave_principio(&ficha.principio_activo),
            ficha.concentracion.clone(),
            ficha.concentracion.as_deref().map(clave_concentracion),
            ficha.forma_farmaceutica.clone(),
            ficha.laboratorio.clone(),
            ficha.registro_sanitario.clone(),
            ficha.condicion_venta.clone(),
            if ficha.es_generico { 1 } else { 0 },
            tiempo::ahora_lima()
        ],
    )
    .await
    .map_err(sin_migracion)?;
    Ok(Json(Ficha { producto_id: Some(id), ..ficha }))
}

/// DELETE /productos/:id/ficha — el producto deja de ser medicamento.
pub async fn quitar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<StatusCode, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    conn.execute("DELETE FROM producto_medicamento WHERE producto_id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ficha(principio: &str, condicion: &str) -> Ficha {
        Ficha {
            producto_id: None,
            principio_activo: principio.into(),
            concentracion: Some(" 500  mg ".into()),
            forma_farmaceutica: Some("tableta".into()),
            laboratorio: None,
            registro_sanitario: Some("en-01234".into()),
            condicion_venta: condicion.into(),
            es_generico: true,
            principio_clave: None,
            concentracion_clave: None,
            generico_del_archivo: None,
        }
    }

    #[test]
    fn generico_desde_el_archivo() {
        assert_eq!(generico_desde_texto(""), Ok(None));
        assert_eq!(generico_desde_texto("  "), Ok(None));
        for si in ["Sí", "si", "SI", "x", "1", "Genérico"] {
            assert_eq!(generico_desde_texto(si), Ok(Some(true)), "{si}");
        }
        for no in ["No", "n", "0", "Marca", "de marca"] {
            assert_eq!(generico_desde_texto(no), Ok(Some(false)), "{no}");
        }
        assert!(generico_desde_texto("tal vez").is_err());
        // Llega a la ficha y queda marcado como dato del archivo.
        let f = FichaImportada { principio_activo: Some("Paracetamol".into()), es_generico: Some("sí".into()), ..Default::default() };
        let ficha = ficha_desde_importada(&f).unwrap().unwrap();
        assert!(ficha.es_generico);
        assert_eq!(ficha.generico_del_archivo, Some(true));
        let sin = FichaImportada { principio_activo: Some("Paracetamol".into()), ..Default::default() };
        assert_eq!(ficha_desde_importada(&sin).unwrap().unwrap().generico_del_archivo, None);
    }

    #[test]
    fn claves_para_buscar_y_comparar() {
        assert_eq!(clave_principio("Paracetamol + Cafeína"), "cafeina + paracetamol");
        assert_eq!(clave_principio("cafeina+PARACETAMOL"), "cafeina + paracetamol");
        assert_eq!(clave_concentracion("250 mg / 5 ml"), "250mg/5ml");
        assert_eq!(clave_concentracion("0,5 MG"), "0.5mg");
    }

    #[test]
    fn textos_de_un_archivo() {
        assert_eq!(forma_desde_texto("Tabletas"), Some("TABLETA"));
        assert_eq!(forma_desde_texto("TAB. RECUBIERTA"), Some("TABLETA_RECUBIERTA"));
        assert_eq!(forma_desde_texto("Cápsula"), Some("CAPSULA"));
        assert_eq!(forma_desde_texto("Jbe"), Some("JARABE"));
        assert_eq!(forma_desde_texto("Ungüento"), Some("UNGUENTO"));
        assert_eq!(forma_desde_texto("algo raro"), None);
        assert_eq!(condicion_desde_texto(""), Some("SIN_RECETA"));
        assert_eq!(condicion_desde_texto("Venta libre"), Some("SIN_RECETA"));
        assert_eq!(condicion_desde_texto("Con receta médica"), Some("CON_RECETA"));
        assert_eq!(condicion_desde_texto("SI"), Some("CON_RECETA"));
        assert_eq!(condicion_desde_texto("Receta retenida"), Some("RECETA_RETENIDA"));
        assert_eq!(condicion_desde_texto("controlado"), Some("RECETA_RETENIDA"));
        assert_eq!(condicion_desde_texto("quizás"), None);
        let vacia = FichaImportada::default();
        assert!(ficha_desde_importada(&vacia).unwrap().is_none());
        let sin_principio = FichaImportada { laboratorio: Some("Genfar".into()), ..Default::default() };
        assert!(ficha_desde_importada(&sin_principio).is_err());
        let buena = FichaImportada {
            principio_activo: Some("Amoxicilina".into()),
            forma_farmaceutica: Some("caps".into()),
            condicion_venta: Some("receta".into()),
            ..Default::default()
        };
        let f = ficha_desde_importada(&buena).unwrap().unwrap();
        assert_eq!((f.forma_farmaceutica.as_deref(), f.condicion_venta.as_str()), (Some("CAPSULA"), "CON_RECETA"));
    }

    #[test]
    fn valida_y_limpia() {
        let f = validar(ficha("  Paracetamol ", "con_receta")).unwrap();
        assert_eq!(f.principio_activo, "Paracetamol");
        assert_eq!(f.concentracion.as_deref(), Some("500 mg"));
        assert_eq!(f.forma_farmaceutica.as_deref(), Some("TABLETA"));
        assert_eq!(f.registro_sanitario.as_deref(), Some("EN-01234"));
        assert_eq!(f.condicion_venta, "CON_RECETA");
        assert!(validar(ficha("Pa", "SIN_RECETA")).is_err());
        assert!(validar(ficha("Paracetamol", "LIBRE")).is_err());
        let mut mala = ficha("Paracetamol", "SIN_RECETA");
        mala.forma_farmaceutica = Some("PASTILLA".into());
        assert!(validar(mala).is_err());
    }
}
