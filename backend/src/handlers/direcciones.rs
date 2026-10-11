//! Dirección propia de cada negocio: monspeet.pages.dev/verane
//!
//! Es un nombre corto, único en todo Monspeet, guardado en la base central
//! (`tiendas.slug`). Solo sirve para mostrar el login con el logo y el color
//! del negocio y para que la dirección del navegador diga de quién es; el
//! acceso sigue siendo con usuario y contraseña.
//!
//! - Al arrancar el servidor se crea la columna si falta y se le da una
//!   dirección a cada negocio que no tenga (su identificador sin el sufijo:
//!   "verane-e8de" -> "verane"; si ya está tomada, "verane-2").
//! - Al registrarse un negocio nuevo se le asigna la suya.
//! - Desde el panel se puede cambiar.
//!
//! Los enlaces de boletas (/boleta/<identificador>/<id>) no cambian.

use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

/// Primeras partes de la dirección que usa el propio sistema: ningún
/// negocio puede llamarse así.
const RESERVADAS: &[&str] = &[
    "panel", "boleta", "registro", "login", "api", "assets", "images", "img", "static", "public",
    "admin", "app", "www", "monspeet", "soporte", "ayuda", "favicon.svg", "icons.svg", "index.html",
];

const LARGO_MIN: usize = 3;
const LARGO_MAX: usize = 30;

/// Texto libre -> dirección: minúsculas, sin tildes ni ñ, y lo demás como
/// guion ("Bodega Doña Ñusta" -> "bodega-dona-nusta").
pub fn normalizar(texto: &str) -> String {
    let mut salida = String::new();
    for c in texto.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        };
        if c.is_ascii_alphanumeric() {
            salida.push(c);
        } else if !salida.ends_with('-') && !salida.is_empty() {
            salida.push('-');
        }
    }
    let salida = salida.trim_end_matches('-');
    salida.chars().take(LARGO_MAX).collect::<String>().trim_end_matches('-').to_string()
}

/// Ok si se puede usar como dirección; si no, por qué.
pub fn validar(direccion: &str) -> Result<(), String> {
    let largo = direccion.chars().count();
    if !(LARGO_MIN..=LARGO_MAX).contains(&largo) {
        return Err(format!("La dirección debe tener entre {} y {} caracteres.", LARGO_MIN, LARGO_MAX));
    }
    if !direccion.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || direccion.starts_with('-')
        || direccion.ends_with('-')
        || direccion.contains("--")
    {
        return Err("Solo letras minúsculas, números y guiones (sin guion al inicio ni al final).".into());
    }
    if RESERVADAS.contains(&direccion) {
        return Err(format!("\"{}\" está reservada por el sistema.", direccion));
    }
    Ok(())
}

/// "verane-e8de" -> "verane" (el sufijo de 4 caracteres que agrega el registro).
fn base_desde_identificador(identificador: &str) -> String {
    let sin_sufijo = match identificador.rsplit_once('-') {
        Some((base, sufijo)) if sufijo.len() == 4 && sufijo.chars().all(|c| c.is_ascii_hexdigit()) => base,
        _ => identificador,
    };
    normalizar(sin_sufijo)
}

async fn libre(conn: &libsql::Connection, direccion: &str, excepto_id: i64) -> Result<bool, String> {
    let mut filas = conn
        .query(
            "SELECT 1 FROM tiendas WHERE (slug = ?1 OR identificador = ?1) AND id <> ?2 LIMIT 1",
            libsql::params![direccion, excepto_id],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(filas.next().await.map_err(|e| e.to_string())?.is_none())
}

/// Da una dirección libre al negocio (a partir de su identificador) y la
/// devuelve. No toca a un negocio que ya tiene una.
pub async fn asignar(conn: &libsql::Connection, tienda_id: i64, identificador: &str) -> Result<String, String> {
    if let Some(actual) = de_tienda(conn, tienda_id).await {
        return Ok(actual);
    }
    let mut base = base_desde_identificador(identificador);
    if validar(&base).is_err() {
        base = normalizar(identificador);
    }
    if validar(&base).is_err() {
        base = format!("negocio-{}", tienda_id);
    }
    for n in 1..100 {
        let candidata = if n == 1 {
            base.clone()
        } else {
            let sufijo = format!("-{}", n);
            let corte: String = base.chars().take(LARGO_MAX - sufijo.len()).collect();
            format!("{}{}", corte.trim_end_matches('-'), sufijo)
        };
        if validar(&candidata).is_ok() && libre(conn, &candidata, tienda_id).await? {
            conn.execute(
                "UPDATE tiendas SET slug = ?1 WHERE id = ?2 AND (slug IS NULL OR slug = '')",
                libsql::params![candidata.clone(), tienda_id],
            )
            .await
            .map_err(|e| e.to_string())?;
            return Ok(candidata);
        }
    }
    Err(format!("No se encontró una dirección libre para el negocio {}", tienda_id))
}

async fn de_tienda(conn: &libsql::Connection, tienda_id: i64) -> Option<String> {
    let mut filas = conn.query("SELECT slug FROM tiendas WHERE id = ?1", libsql::params![tienda_id]).await.ok()?;
    let fila = filas.next().await.ok()??;
    fila.get::<String>(0).ok().filter(|s| !s.is_empty())
}

/// La dirección de un negocio por su identificador. None si no tiene o si
/// la base central aún no tiene la columna.
pub async fn de_identificador(conn: &libsql::Connection, identificador: &str) -> Option<String> {
    let mut filas = conn
        .query("SELECT slug FROM tiendas WHERE identificador = ?1", libsql::params![identificador])
        .await
        .ok()?;
    let fila = filas.next().await.ok()??;
    fila.get::<String>(0).ok().filter(|s| !s.is_empty())
}

/// Al arrancar: crea la columna y su índice si faltan, y da dirección a los
/// negocios que no tienen. Solo agrega; nunca cambia una dirección ya puesta.
pub async fn preparar_central(conn: &libsql::Connection) -> Result<usize, String> {
    if let Err(e) = conn.execute("ALTER TABLE tiendas ADD COLUMN slug TEXT", ()).await {
        let e = e.to_string();
        if !e.contains("duplicate column") {
            return Err(e);
        }
    }
    conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_tiendas_slug ON tiendas(slug)", ())
        .await
        .map_err(|e| e.to_string())?;

    let mut pendientes: Vec<(i64, String)> = Vec::new();
    let mut filas = conn
        .query("SELECT id, identificador FROM tiendas WHERE slug IS NULL OR slug = '' ORDER BY id", ())
        .await
        .map_err(|e| e.to_string())?;
    while let Some(f) = filas.next().await.map_err(|e| e.to_string())? {
        pendientes.push((f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default()));
    }
    drop(filas);
    let mut asignadas = 0;
    for (id, identificador) in pendientes {
        match asignar(conn, id, &identificador).await {
            Ok(d) => {
                println!("  🔗 {} -> /{}", identificador, d);
                asignadas += 1;
            }
            Err(e) => eprintln!("  ⚠️  {}: no se pudo asignar su dirección: {}", identificador, e),
        }
    }
    Ok(asignadas)
}

/// Lo que se muestra en el login de la dirección de un negocio.
#[derive(Serialize)]
pub struct NegocioPublico {
    pub identificador: String,
    pub slug: String,
    pub nombre_negocio: String,
    pub logo_url: Option<String>,
    pub color_acento: Option<String>,
}

/// GET /publico/negocio/:direccion — PÚBLICA. Acepta la dirección
/// ("verane") o el identificador ("verane-e8de"). Solo nombre, logo y color:
/// lo mismo que ya se ve en el login después de escribir el usuario.
pub async fn negocio_publico(
    State(state): State<Arc<AppState>>,
    Path(direccion): Path<String>,
) -> Result<Json<NegocioPublico>, (StatusCode, String)> {
    let no_existe = || (StatusCode::NOT_FOUND, "No existe un negocio con esa dirección.".to_string());
    let direccion = direccion.trim().to_lowercase();
    if direccion.is_empty() || direccion.len() > 60 {
        return Err(no_existe());
    }
    let central = state.tiendas.conexion_central().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let (identificador, slug): (String, Option<String>) = {
        let mut filas = central
            .query(
                "SELECT identificador, slug FROM tiendas WHERE slug = ?1 OR identificador = ?1 ORDER BY (slug = ?1) DESC LIMIT 1",
                libsql::params![direccion.clone()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let f = filas
            .next()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(no_existe)?;
        (f.get(0).unwrap_or_default(), f.get::<String>(1).ok().filter(|s| !s.is_empty()))
    };
    let tienda = state.tiendas.buscar_por_identificador(&identificador).await.map_err(|_| no_existe())?;
    let (logo_url, color_acento) = match state.tiendas.conectar_cacheado(&tienda).await.and_then(|db| db.connect().map_err(|e| e.to_string())) {
        Ok(conn) => crate::handlers::auth::leer_identidad_visual(&conn).await,
        Err(_) => (None, None),
    };
    Ok(Json(NegocioPublico {
        slug: slug.unwrap_or_else(|| tienda.identificador.clone()),
        identificador: tienda.identificador,
        nombre_negocio: tienda.nombre_negocio,
        logo_url,
        color_acento,
    }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn normaliza_y_valida() {
        assert_eq!(normalizar("Bodega Doña Ñusta"), "bodega-dona-nusta");
        assert_eq!(normalizar("  VERANE  "), "verane");
        assert_eq!(normalizar("Café & Té!!"), "cafe-te");
        assert_eq!(base_desde_identificador("verane-e8de"), "verane");
        assert_eq!(base_desde_identificador("minimarket-original"), "minimarket-original");
        assert_eq!(base_desde_identificador("cafeteria-2642"), "cafeteria");
        assert!(validar("verane").is_ok());
        assert!(validar("bodega-2").is_ok());
        assert!(validar("panel").is_err());
        assert!(validar("ab").is_err());
        assert!(validar("-verane").is_err());
        assert!(validar("Verane").is_err());
        assert!(validar("ver--ane").is_err());
        assert!(normalizar(&"a".repeat(50)).len() <= LARGO_MAX);
    }

    #[tokio::test]
    async fn asigna_direcciones_sin_repetir() {
        let db = libsql::Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute_batch(
            "CREATE TABLE tiendas (id INTEGER PRIMARY KEY, nombre_negocio TEXT, identificador TEXT UNIQUE);
             INSERT INTO tiendas VALUES (1, 'Verane', 'verane-e8de'), (2, 'Verane 2', 'verane-a1b2'),
                                        (3, 'Panel', 'panel-0f0f'), (4, 'Original', 'minimarket-original');",
        )
        .await
        .unwrap();
        assert_eq!(preparar_central(&conn).await.unwrap(), 4);
        assert_eq!(de_identificador(&conn, "verane-e8de").await.as_deref(), Some("verane"));
        assert_eq!(de_identificador(&conn, "verane-a1b2").await.as_deref(), Some("verane-2"));
        // "panel" está reservada: se usa el identificador completo.
        assert_eq!(de_identificador(&conn, "panel-0f0f").await.as_deref(), Some("panel-0f0f"));
        assert_eq!(de_identificador(&conn, "minimarket-original").await.as_deref(), Some("minimarket-original"));
        // Segunda vez: la columna ya existe y nadie cambia.
        assert_eq!(preparar_central(&conn).await.unwrap(), 0);
        assert_eq!(asignar(&conn, 1, "verane-e8de").await.unwrap(), "verane");
    }
}
