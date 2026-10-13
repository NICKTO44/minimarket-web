//! El sistema como app instalable (PWA), con el nombre y el logo de cada negocio.
//!
//! Desde la dirección propia del negocio (monspeet.pages.dev/verane), el
//! frontend enlaza el manifiesto de aquí: al instalar, la app queda con el
//! nombre del negocio, su color y su logo como ícono. Sin logo, el ícono
//! general de Monspeet (que está en el frontend).
//!
//! - GET /publico/app/:direccion/manifest?origen=https://...  — PÚBLICA.
//! - GET /publico/app/:direccion/icono/:tam                   — PÚBLICA, PNG cuadrado.
//!
//! `origen` es la dirección del frontend: la app tiene que abrir ahí (el
//! navegador solo acepta un manifiesto cuya app abre en el mismo sitio que
//! la página que lo enlaza, así que un origen ajeno no sirve para nada).

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::handlers::direcciones::{negocio_publico, NegocioPublico};
use crate::AppState;

/// Tamaños de ícono que se sirven (los que piden Android, Chrome e iPhone).
const TAMANOS: &[u32] = &[180, 192, 512];

/// Color de la barra de la app si el negocio no eligió uno.
const COLOR_POR_DEFECTO: &str = "#16a34a";

#[derive(Deserialize)]
pub struct ParamsManifiesto {
    pub origen: Option<String>,
}

/// "https://monspeet.pages.dev/" -> "https://monspeet.pages.dev". `None` si
/// no es un origen (con ruta, espacios, comillas, otro protocolo...).
fn origen_valido(origen: &str) -> Option<String> {
    let origen = origen.trim().trim_end_matches('/');
    let resto = origen
        .strip_prefix("https://")
        .or_else(|| origen.strip_prefix("http://localhost"))
        .or_else(|| origen.strip_prefix("http://127.0.0.1"))?;
    let valido = !resto.contains('/')
        && resto.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        && (origen.starts_with("http://") || !resto.is_empty());
    valido.then(|| origen.to_string())
}

/// "#16a34a" o "#fff" -> tal cual; cualquier otra cosa -> el color por defecto.
fn color_valido(color: Option<&str>) -> String {
    match color.map(str::trim) {
        Some(c) if (c.len() == 7 || c.len() == 4) && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()) => {
            c.to_string()
        }
        _ => COLOR_POR_DEFECTO.to_string(),
    }
}

/// Archivo del logo en disco ("/uploads/tienda/7/logo.png" -> "uploads/tienda/7/logo.png").
/// Solo dentro de uploads/tienda/, nunca con "..".
fn archivo_logo(logo_url: Option<&str>) -> Option<String> {
    let ruta = logo_url?.trim().trim_start_matches('/');
    (ruta.starts_with("uploads/tienda/") && !ruta.contains("..") && !ruta.contains('\\')).then(|| ruta.to_string())
}

/// Nombre corto para debajo del ícono (el teléfono corta los largos).
fn nombre_corto(nombre: &str) -> String {
    let nombre = nombre.trim();
    if nombre.chars().count() <= 12 {
        return nombre.to_string();
    }
    let primera = nombre.split_whitespace().next().unwrap_or(nombre);
    primera.chars().take(12).collect()
}

/// Arma el manifiesto. Separado del handler para probarlo sin servidor.
pub fn armar_manifiesto(negocio: &NegocioPublico, origen: &str, con_logo: bool) -> serde_json::Value {
    let ruta_app = format!("/{}", negocio.slug);
    let iconos: Vec<serde_json::Value> = if con_logo {
        [192u32, 512]
            .iter()
            .map(|t| {
                json!({
                    "src": format!("/publico/app/{}/icono/{}", negocio.slug, t),
                    "sizes": format!("{t}x{t}"),
                    "type": "image/png",
                    "purpose": "any"
                })
            })
            .collect()
    } else {
        [192u32, 512]
            .iter()
            .map(|t| json!({ "src": format!("{origen}/icono-{t}.png"), "sizes": format!("{t}x{t}"), "type": "image/png", "purpose": "any" }))
            .collect()
    };
    json!({
        "id": ruta_app,
        "name": negocio.nombre_negocio,
        "short_name": nombre_corto(&negocio.nombre_negocio),
        "description": format!("{} · Monspeet POS", negocio.nombre_negocio),
        "lang": "es-PE",
        "start_url": format!("{origen}{ruta_app}"),
        "scope": format!("{origen}{ruta_app}"),
        "display": "standalone",
        "background_color": "#ffffff",
        "theme_color": color_valido(negocio.color_acento.as_deref()),
        "icons": iconos
    })
}

pub async fn manifiesto(
    State(state): State<Arc<AppState>>,
    Path(direccion): Path<String>,
    Query(params): Query<ParamsManifiesto>,
) -> Result<Response, (StatusCode, String)> {
    let origen = params
        .origen
        .as_deref()
        .and_then(origen_valido)
        .ok_or((StatusCode::BAD_REQUEST, "Falta el origen del sistema (?origen=https://...).".to_string()))?;
    let Json(negocio) = negocio_publico(State(state), Path(direccion)).await?;
    let con_logo = archivo_logo(negocio.logo_url.as_deref()).is_some_and(|f| std::path::Path::new(&f).is_file());
    let cuerpo = armar_manifiesto(&negocio, &origen, con_logo).to_string();
    Ok((
        [
            (header::CONTENT_TYPE, "application/manifest+json; charset=utf-8"),
            // Sin caché: si el negocio cambia su nombre o logo, se ve al reinstalar.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        cuerpo,
    )
        .into_response())
}

/// El logo centrado en un cuadrado blanco de `tam` px (ocupa el 80 %).
pub fn icono_cuadrado(logo: &image::DynamicImage, tam: u32) -> Result<Vec<u8>, String> {
    let interior = tam * 8 / 10;
    let escalado = logo.resize(interior, interior, image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut lienzo = image::RgbaImage::from_pixel(tam, tam, image::Rgba([255, 255, 255, 255]));
    let x = (tam - escalado.width()) / 2;
    let y = (tam - escalado.height()) / 2;
    image::imageops::overlay(&mut lienzo, &escalado, x as i64, y as i64);
    let mut salida = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(lienzo)
        .write_to(&mut salida, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(salida.into_inner())
}

pub async fn icono(
    State(state): State<Arc<AppState>>,
    Path((direccion, tam)): Path<(String, u32)>,
) -> Result<Response, (StatusCode, String)> {
    if !TAMANOS.contains(&tam) {
        return Err((StatusCode::NOT_FOUND, "Tamaño de ícono no disponible.".to_string()));
    }
    let Json(negocio) = negocio_publico(State(state), Path(direccion)).await?;
    let sin_logo = || (StatusCode::NOT_FOUND, "Este negocio no tiene logo.".to_string());
    let archivo = archivo_logo(negocio.logo_url.as_deref()).ok_or_else(sin_logo)?;
    let png = tokio::task::spawn_blocking(move || {
        let logo = image::open(&archivo).map_err(|_| "sin logo".to_string())?;
        icono_cuadrado(&logo, tam)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|_| sin_logo())?;
    Ok((
        [(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")],
        png,
    )
        .into_response())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn verane(logo: Option<&str>, color: Option<&str>) -> NegocioPublico {
        NegocioPublico {
            identificador: "verane-e8de".into(),
            slug: "verane".into(),
            nombre_negocio: "VERANE".into(),
            logo_url: logo.map(String::from),
            color_acento: color.map(String::from),
        }
    }

    #[test]
    fn solo_acepta_origenes() {
        assert_eq!(origen_valido("https://monspeet.pages.dev/").as_deref(), Some("https://monspeet.pages.dev"));
        assert_eq!(origen_valido("http://localhost:5173").as_deref(), Some("http://localhost:5173"));
        assert_eq!(origen_valido("https://a.com/ruta"), None);
        assert_eq!(origen_valido("http://otro.com"), None);
        assert_eq!(origen_valido("https://a.com\"x"), None);
        assert_eq!(origen_valido("javascript:alert(1)"), None);
        assert_eq!(origen_valido("https://"), None);
    }

    #[test]
    fn manifiesto_del_negocio() {
        let m = armar_manifiesto(&verane(Some("/uploads/tienda/3/logo.png"), Some("#7c3aed")), "https://monspeet.pages.dev", true);
        assert_eq!(m["name"], "VERANE");
        assert_eq!(m["id"], "/verane");
        assert_eq!(m["start_url"], "https://monspeet.pages.dev/verane");
        assert_eq!(m["scope"], "https://monspeet.pages.dev/verane");
        assert_eq!(m["display"], "standalone");
        assert_eq!(m["theme_color"], "#7c3aed");
        assert_eq!(m["icons"][1]["src"], "/publico/app/verane/icono/512");
        assert_eq!(m["icons"][1]["sizes"], "512x512");
    }

    #[test]
    fn sin_logo_usa_el_icono_general() {
        let m = armar_manifiesto(&verane(None, Some("rojo")), "http://localhost:5173", false);
        assert_eq!(m["icons"][0]["src"], "http://localhost:5173/icono-192.png");
        assert_eq!(m["theme_color"], COLOR_POR_DEFECTO);
    }

    #[test]
    fn logo_solo_dentro_de_uploads() {
        assert_eq!(archivo_logo(Some("/uploads/tienda/3/logo.png")).as_deref(), Some("uploads/tienda/3/logo.png"));
        assert_eq!(archivo_logo(Some("/uploads/tienda/../../etc/passwd")), None);
        assert_eq!(archivo_logo(Some("/etc/passwd")), None);
        assert_eq!(archivo_logo(None), None);
    }

    #[test]
    fn nombre_corto_para_el_icono() {
        assert_eq!(nombre_corto("VERANE"), "VERANE");
        assert_eq!(nombre_corto("Cabañita Restobar Criollo"), "Cabañita");
    }

    #[test]
    fn icono_cuadrado_con_logo_ancho() {
        let logo = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(400, 100, image::Rgba([0, 0, 0, 255])));
        let png = icono_cuadrado(&logo, 192).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert_eq!((img.width(), img.height()), (192, 192));
        // Esquina blanca, centro con el logo.
        assert_eq!(img.to_rgba8().get_pixel(0, 0).0, [255, 255, 255, 255]);
        assert_eq!(img.to_rgba8().get_pixel(96, 96).0, [0, 0, 0, 255]);
    }
}
