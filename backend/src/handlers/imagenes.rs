use axum::{extract::{State, Path, Multipart}, Json, http::StatusCode};
use std::sync::Arc;

use crate::AppState;
use crate::models::producto::ProductoResponse;

const ANCHO_MAXIMO: u32 = 800;
const CALIDAD_JPEG: u8 = 80;

pub async fn subir_imagen_producto(
    State(state): State<Arc<AppState>>,
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

    let img = image::load_from_memory(&bytes)
        .map_err(|_| (StatusCode::BAD_REQUEST, "El archivo no es una imagen válida".to_string()))?;

    let redimensionada = img.resize(ANCHO_MAXIMO, ANCHO_MAXIMO, image::imageops::FilterType::Lanczos3);

    std::fs::create_dir_all("uploads/productos")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo crear la carpeta de imágenes: {}", e)))?;

    let ruta = format!("uploads/productos/{}.jpg", id);
    let mut salida = std::fs::File::create(&ruta)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar el archivo: {}", e)))?;

    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut salida, CALIDAD_JPEG);
    encoder
        .encode_image(&redimensionada)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al comprimir la imagen: {}", e)))?;

    let url_publica = format!("/uploads/productos/{}.jpg", id);

    let conn = state.db.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
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