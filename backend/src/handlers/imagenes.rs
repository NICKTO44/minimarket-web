use axum::{extract::{Extension, Path, Multipart}, Json, http::StatusCode};
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::tenants::TenantDb;
use crate::models::producto::ProductoResponse;
use crate::models::configuracion::AccionResponse;
use crate::logica::imagen::{self, Salida};

const ANCHO_MAXIMO: u32 = 800;
const CALIDAD_JPEG: u8 = 80;
const ANCHO_MAXIMO_LOGO: u32 = 400;

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
    guardar(&carpeta, &ruta, jpeg).await?;

    let url_publica = format!("/uploads/productos/{}/{}.jpg", claims.tienda_id, id);

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
    crate::middleware_auth::exigir_admin(&claims)?;
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
