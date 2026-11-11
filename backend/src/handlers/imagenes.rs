use axum::{extract::{Extension, Path, Multipart}, Json, http::StatusCode};
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::tenants::TenantDb;
use crate::models::producto::ProductoResponse;
use crate::models::configuracion::AccionResponse;
use crate::logica::imagen::{self, Salida};
use crate::handlers::rubros::{exigir_modulo, MODULO_CATALOGO};

const ANCHO_MAXIMO: u32 = 800;
const CALIDAD_JPEG: u8 = 80;
const ANCHO_MAXIMO_LOGO: u32 = 400;
// Miniatura para el Catálogo (y cualquier lista con muchas fotos): unos
// 15-30 KB en vez de 60-120 KB. Se guarda junto a la foto como {id}_m.jpg.
const LADO_MINIATURA: u32 = 400;
const CALIDAD_MINIATURA: u8 = 76;

/// "uploads/productos/7/15.jpg" -> "uploads/productos/7/15_m.jpg"
pub fn ruta_miniatura(ruta: &str) -> Option<String> {
    ruta.strip_suffix(".jpg").filter(|r| !r.ends_with("_m")).map(|r| format!("{}_m.jpg", r))
}

pub async fn subir_imagen_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<ProductoResponse>, (StatusCode, String)> {
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("imagen") {
            bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec(),
            );
        }
    }

    let bytes = bytes.ok_or((StatusCode::BAD_REQUEST, "No se recibió ninguna imagen".to_string()))?;

    if bytes.len() > 10 * 1024 * 1024 {
        return Err((StatusCode::BAD_REQUEST, "La imagen no puede pesar más de 10MB".into()));
    }

    // El producto tiene que existir en este negocio antes de guardar nada
    // (antes se escribía el archivo con cualquier id).
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut filas = conn
        .query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if filas.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.is_none() {
        return Err((StatusCode::NOT_FOUND, "Ese producto no existe.".to_string()));
    }
    drop(filas);

    // Revisa medidas y formato antes de abrirla, con tope de memoria, en un
    // hilo aparte y de a una (ver logica/imagen.rs).
    let jpeg = imagen::procesar(bytes, ANCHO_MAXIMO, Salida::Jpeg(CALIDAD_JPEG))
        .await
        .map_err(|e| (e.status(), e.mensaje()))?;

    let carpeta = format!("uploads/productos/{}", claims.tienda_id);
    let ruta = format!("{}/{}.jpg", carpeta, id);
    // La miniatura sale de la foto ya achicada (barato). Si falla, la foto
    // se guarda igual y el catálogo usa la grande.
    let miniatura = imagen::procesar(jpeg.clone(), LADO_MINIATURA, Salida::Jpeg(CALIDAD_MINIATURA)).await.ok();
    guardar(&carpeta, &ruta, jpeg).await?;
    if let (Some(m), Some(ruta_m)) = (miniatura, ruta_miniatura(&ruta)) {
        let _ = guardar(&carpeta, &ruta_m, m).await;
    }

    // ?v= como en la foto 2: el navegador guarda las fotos de productos por
    // 30 días (main.rs, cache_de_fotos), así que al reemplazarla cambia la
    // dirección y nadie sigue viendo la anterior. El archivo es el mismo.
    let version = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let url_publica = format!("/uploads/productos/{}/{}.jpg?v={}", claims.tienda_id, id, version);

    conn.execute(
        "UPDATE productos SET imagen_url = ?1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
        libsql::params![url_publica.clone(), id],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al guardar en base de datos: {}", e)))?;

    Ok(Json(ProductoResponse {
        success: true,
        message: "Imagen subida y optimizada correctamente".into(),
        producto_id: Some(id),
    }))
}

// Logo del negocio, para la pantalla de login. Mismo patrón que
// subir_imagen_producto, pero guarda PNG (no JPEG) para conservar
// transparencia, y no depende de un :id -- un negocio tiene un solo
// logo, siempre pisa el mismo archivo.
pub async fn subir_logo_tienda(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    crate::middleware_auth::exigir_solo_admin(&claims)?;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("logo") {
            bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                    .to_vec(),
            );
        }
    }

    let bytes = bytes.ok_or((StatusCode::BAD_REQUEST, "No se recibió ningún logo".to_string()))?;

    if bytes.len() > 5 * 1024 * 1024 {
        return Err((StatusCode::BAD_REQUEST, "El logo no puede pesar más de 5MB".into()));
    }

    // PNG para conservar la transparencia (ver logica/imagen.rs).
    let png = imagen::procesar(bytes, ANCHO_MAXIMO_LOGO, Salida::Png)
        .await
        .map_err(|e| (e.status(), e.mensaje()))?;

    let carpeta = format!("uploads/tienda/{}", claims.tienda_id);
    let ruta = format!("{}/logo.png", carpeta);
    guardar(&carpeta, &ruta, png).await?;

    let url_publica = format!("/uploads/tienda/{}/logo.png", claims.tienda_id);

    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    conn.execute(
        "UPDATE configuracion_tienda SET logo_path = ?1, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![url_publica.clone()],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al guardar en base de datos: {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Logo actualizado".into() }))
}

/// Escribe el archivo ya procesado. Se escribe aparte y luego se renombra:
/// quien esté viendo la foto nunca recibe un archivo a medio escribir.
async fn guardar(carpeta: &str, ruta: &str, contenido: Vec<u8>) -> Result<(), (StatusCode, String)> {
    tokio::fs::create_dir_all(carpeta)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo crear la carpeta de imágenes: {}", e)))?;
    let temporal = format!("{}.subiendo", ruta);
    tokio::fs::write(&temporal, contenido)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar el archivo: {}", e)))?;
    tokio::fs::rename(&temporal, ruta)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar el archivo: {}", e)))
}

/// Crea las miniaturas que falten (fotos subidas antes de que existieran).
/// Corre en segundo plano al arrancar, de a una foto y sin apuro: en el
/// siguiente arranque solo revisa (ya están). Devuelve cuántas creó.
pub async fn crear_miniaturas_faltantes(base: &std::path::Path) -> usize {
    let mut creadas = 0;
    let Ok(mut negocios) = tokio::fs::read_dir(base).await else { return 0 };
    while let Ok(Some(negocio)) = negocios.next_entry().await {
        let Ok(mut fotos) = tokio::fs::read_dir(negocio.path()).await else { continue };
        while let Ok(Some(foto)) = fotos.next_entry().await {
            let ruta = foto.path().to_string_lossy().to_string();
            let Some(ruta_m) = ruta_miniatura(&ruta) else { continue };
            if tokio::fs::metadata(&ruta_m).await.is_ok() {
                continue;
            }
            let Ok(bytes) = tokio::fs::read(&ruta).await else { continue };
            if let Ok(m) = imagen::procesar(bytes, LADO_MINIATURA, Salida::Jpeg(CALIDAD_MINIATURA)).await {
                let carpeta = negocio.path().to_string_lossy().to_string();
                if guardar(&carpeta, &ruta_m, m).await.is_ok() {
                    creadas += 1;
                }
            }
            // Deja respirar al servidor (1 GB) entre foto y foto.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    creadas
}

// ===== Foto 2 del producto (módulo CATALOGO, migración 0066) =====
//
// La foto 1 sigue siendo productos.imagen_url (subir_imagen_producto, sin
// cambios). La foto 2 vive en la tabla producto_imagenes (orden = 2) y en
// uploads/productos/{tienda}/{id}_2.jpg, con su miniatura {id}_2_m.jpg
// (crear_miniaturas_faltantes también la cubre). En el Catálogo se pasa de
// una a otra con puntitos.

const NOMBRE_MODULO_CATALOGO: &str = "Catálogo";

#[derive(serde::Serialize)]
pub struct FotoExtra {
    pub producto_id: i64,
    pub orden: i64,
    pub url: String,
}

fn e500<E: ToString>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Lee los bytes del campo `campo` del formulario.
async fn leer_campo(multipart: &mut Multipart, campo: &str) -> Result<Option<Vec<u8>>, (StatusCode, String)> {
    let mut bytes = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))? {
        if field.name() == Some(campo) {
            bytes = Some(field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?.to_vec());
        }
    }
    Ok(bytes)
}

/// GET /productos/fotos-extra — las fotos 2 de los productos que tienen una.
/// Sin la migración 0066 todavía: lista vacía (nada se rompe).
pub async fn listar_fotos_extra(
    Extension(tenant): Extension<Arc<TenantDb>>,
) -> Result<Json<Vec<FotoExtra>>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_CATALOGO, NOMBRE_MODULO_CATALOGO).await?;
    let mut lista = Vec::new();
    let Ok(mut filas) = conn
        .query(
            "SELECT pi.producto_id, pi.orden, pi.url FROM producto_imagenes pi
             JOIN productos p ON p.id = pi.producto_id
             ORDER BY pi.producto_id, pi.orden",
            (),
        )
        .await
    else {
        return Ok(Json(lista));
    };
    while let Ok(Some(f)) = filas.next().await {
        if let (Ok(producto_id), Ok(orden), Ok(url)) = (f.get::<i64>(0), f.get::<i64>(1), f.get::<String>(2)) {
            lista.push(FotoExtra { producto_id, orden, url });
        }
    }
    Ok(Json(lista))
}

/// POST /productos/:id/foto-2 — sube (o reemplaza) la foto 2. Mismo
/// procesamiento que la foto 1: se achica a 800 px, con miniatura.
pub async fn subir_foto_2(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_CATALOGO, NOMBRE_MODULO_CATALOGO).await?;

    let bytes = leer_campo(&mut multipart, "imagen")
        .await?
        .ok_or((StatusCode::BAD_REQUEST, "No se recibió ninguna imagen".to_string()))?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err((StatusCode::BAD_REQUEST, "La imagen no puede pesar más de 10MB".into()));
    }

    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "Ese producto no existe.".to_string()));
    }
    drop(filas);

    let jpeg = imagen::procesar(bytes, ANCHO_MAXIMO, Salida::Jpeg(CALIDAD_JPEG))
        .await
        .map_err(|e| (e.status(), e.mensaje()))?;
    let carpeta = format!("uploads/productos/{}", claims.tienda_id);
    let ruta = format!("{}/{}_2.jpg", carpeta, id);
    let miniatura = imagen::procesar(jpeg.clone(), LADO_MINIATURA, Salida::Jpeg(CALIDAD_MINIATURA)).await.ok();
    guardar(&carpeta, &ruta, jpeg).await?;
    if let (Some(m), Some(ruta_m)) = (miniatura, ruta_miniatura(&ruta)) {
        let _ = guardar(&carpeta, &ruta_m, m).await;
    }

    // ?v= para que el navegador no siga mostrando la foto 2 anterior.
    let version = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let url_publica = format!("/uploads/productos/{}/{}_2.jpg?v={}", claims.tienda_id, id, version);
    conn.execute(
        "INSERT INTO producto_imagenes (producto_id, orden, url, fecha_actualizacion)
         VALUES (?1, 2, ?2, datetime('now','localtime'))
         ON CONFLICT(producto_id, orden) DO UPDATE SET url = excluded.url, fecha_actualizacion = excluded.fecha_actualizacion",
        libsql::params![id, url_publica],
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar la foto 2: {}", e)))?;

    Ok(Json(AccionResponse { success: true, message: "Foto 2 guardada".into() }))
}

/// DELETE /productos/:id/foto-2 — quita la foto 2 (la foto 1 no se toca).
pub async fn quitar_foto_2(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<AccionResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_CATALOGO, NOMBRE_MODULO_CATALOGO).await?;
    conn.execute("DELETE FROM producto_imagenes WHERE producto_id = ?1 AND orden = 2", libsql::params![id])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo quitar la foto 2: {}", e)))?;
    let carpeta = format!("uploads/productos/{}", claims.tienda_id);
    let _ = tokio::fs::remove_file(format!("{}/{}_2.jpg", carpeta, id)).await;
    let _ = tokio::fs::remove_file(format!("{}/{}_2_m.jpg", carpeta, id)).await;
    Ok(Json(AccionResponse { success: true, message: "Foto 2 quitada".into() }))
}

#[cfg(test)]
mod tests_miniatura {
    use super::ruta_miniatura;

    #[test]
    fn nombre_de_la_miniatura() {
        assert_eq!(ruta_miniatura("uploads/productos/7/15.jpg").as_deref(), Some("uploads/productos/7/15_m.jpg"));
        assert_eq!(ruta_miniatura("uploads/productos/7/15_m.jpg"), None);
        assert_eq!(ruta_miniatura("uploads/productos/7/15.jpg.subiendo"), None);
        assert_eq!(ruta_miniatura("uploads/tienda/7/logo.png"), None);
        assert_eq!(ruta_miniatura("uploads/productos/7/15_2.jpg").as_deref(), Some("uploads/productos/7/15_2_m.jpg"));
    }
}
