//! Foto 2 del producto (módulo CATALOGO, migración 0066) contra una base
//! local (schema.sql + todas las migraciones) y los handlers de verdad:
//! subir, reemplazar, listar y quitar la foto 2; que la foto 1 no cambie, y
//! que un negocio sin el módulo reciba 403.
//!
//!   cargo test --test foto_2

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Extension, FromRequest, Multipart, Path, Request},
    http::StatusCode,
};
use minimarket_backend::handlers::imagenes;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("foto2_{}_{}.db", nombre, std::process::id()));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.expect("base local");
    let conn = db.connect().unwrap();
    conn.execute_batch(include_str!("../schema.sql")).await.expect("schema.sql");
    let mut migraciones: Vec<_> = std::fs::read_dir("migraciones")
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false))
        .collect();
    migraciones.sort();
    for m in &migraciones {
        conn.execute_batch(&std::fs::read_to_string(m).unwrap())
            .await
            .unwrap_or_else(|e| panic!("{}: {}", m.display(), e));
    }
    // Idempotente: aplicarla otra vez no rompe nada.
    conn.execute_batch(include_str!("../migraciones/0066_foto_2_producto.sql")).await.expect("0066 otra vez");
    conn.execute("UPDATE configuracion_tienda SET rubro = 'LUBRICENTRO', modulos = ?1", libsql::params![modulos])
        .await
        .unwrap();
    conn.execute_batch(
        "INSERT INTO categorias (id, nombre) VALUES (900, 'Suspensión');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, imagen_url) VALUES
            (2001, 'MSR-31002L', 'Trapecio LH', 100, 10, 900, '/uploads/productos/990001/2001.jpg'),
            (2002, 'MSR-31003L', 'Trapecio Chevy LH', 70, 5, 900, NULL);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn claims(tienda: i64) -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": 1, "nombre_completo": "Admin", "tienda_id": tienda, "exp": usize::MAX
    }))
    .unwrap()
}

fn jpeg(ancho: u32, alto: u32, tono: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(ancho, alto, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, tono]));
    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&img).unwrap();
    buf
}

async fn formulario(bytes: &[u8]) -> Multipart {
    let limite = "LIMITEFOTO2";
    let mut cuerpo = format!(
        "--{l}\r\nContent-Disposition: form-data; name=\"imagen\"; filename=\"f.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n",
        l = limite
    )
    .into_bytes();
    cuerpo.extend_from_slice(bytes);
    cuerpo.extend_from_slice(format!("\r\n--{}--\r\n", limite).as_bytes());
    let req = Request::builder()
        .method("POST")
        .header("content-type", format!("multipart/form-data; boundary={}", limite))
        .body(Body::from(cuerpo))
        .unwrap();
    Multipart::from_request(req, &()).await.unwrap()
}

async fn subir(t: &Arc<TenantDb>, tienda: i64, id: i64, bytes: &[u8]) -> Result<(), (StatusCode, String)> {
    imagenes::subir_foto_2(Extension(t.clone()), Extension(claims(tienda)), Path(id), formulario(bytes).await)
        .await
        .map(|_| ())
}

async fn lista(t: &Arc<TenantDb>) -> Result<Vec<(i64, i64, String)>, (StatusCode, String)> {
    imagenes::listar_fotos_extra(Extension(t.clone()))
        .await
        .map(|axum::Json(l)| l.into_iter().map(|f| (f.producto_id, f.orden, f.url)).collect())
}

async fn imagen_url(t: &Arc<TenantDb>, id: i64) -> Option<String> {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query("SELECT imagen_url FROM productos WHERE id = ?1", libsql::params![id]).await.unwrap();
    f.next().await.unwrap().unwrap().get::<String>(0).ok()
}

#[tokio::test]
async fn foto_2_completa() {
    let tienda = 990001;
    let carpeta = format!("uploads/productos/{}", tienda);
    let _ = std::fs::remove_dir_all(&carpeta);
    let t = base("con", "FICHA_REPUESTO,CATALOGO").await;

    assert!(lista(&t).await.unwrap().is_empty(), "sin fotos 2 al inicio");

    // Subir: archivo, miniatura y fila.
    subir(&t, tienda, 2001, &jpeg(2000, 1500, 10)).await.expect("sube la foto 2");
    assert!(std::path::Path::new(&format!("{}/2001_2.jpg", carpeta)).exists());
    assert!(std::path::Path::new(&format!("{}/2001_2_m.jpg", carpeta)).exists(), "con miniatura");
    let foto = image::open(format!("{}/2001_2.jpg", carpeta)).unwrap();
    assert_eq!((foto.width(), foto.height()), (800, 600), "se achica igual que la foto 1");
    let l = lista(&t).await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!((l[0].0, l[0].1), (2001, 2));
    assert!(l[0].2.starts_with(&format!("/uploads/productos/{}/2001_2.jpg?v=", tienda)), "{}", l[0].2);

    // La foto 1 no cambia, y no se crea su archivo.
    assert_eq!(imagen_url(&t, 2001).await.as_deref(), Some("/uploads/productos/990001/2001.jpg"));
    assert_eq!(imagen_url(&t, 2002).await, None);
    assert!(!std::path::Path::new(&format!("{}/2001.jpg", carpeta)).exists());

    // Reemplazar: sigue una sola fila.
    subir(&t, tienda, 2001, &jpeg(600, 900, 200)).await.expect("reemplaza");
    assert_eq!(lista(&t).await.unwrap().len(), 1);
    let foto = image::open(format!("{}/2001_2.jpg", carpeta)).unwrap();
    assert_eq!((foto.width(), foto.height()), (533, 800), "es la nueva (vertical, achicada)");

    // Producto que no existe, o algo que no es imagen.
    assert_eq!(subir(&t, tienda, 9999, &jpeg(100, 100, 0)).await.unwrap_err().0, StatusCode::NOT_FOUND);
    assert_eq!(subir(&t, tienda, 2002, b"no soy una foto").await.unwrap_err().0, StatusCode::BAD_REQUEST);
    assert!(!std::path::Path::new(&format!("{}/2002_2.jpg", carpeta)).exists());

    // Quitar: fila y archivos fuera; la foto 1 sigue.
    imagenes::quitar_foto_2(Extension(t.clone()), Extension(claims(tienda)), Path(2001)).await.expect("quita");
    assert!(lista(&t).await.unwrap().is_empty());
    assert!(!std::path::Path::new(&format!("{}/2001_2.jpg", carpeta)).exists());
    assert!(!std::path::Path::new(&format!("{}/2001_2_m.jpg", carpeta)).exists());
    assert_eq!(imagen_url(&t, 2001).await.as_deref(), Some("/uploads/productos/990001/2001.jpg"));
    // Quitar otra vez no falla.
    imagenes::quitar_foto_2(Extension(t.clone()), Extension(claims(tienda)), Path(2001)).await.expect("quitar dos veces");

    // Un producto borrado deja de salir en la lista.
    subir(&t, tienda, 2002, &jpeg(300, 300, 50)).await.unwrap();
    let conn = t.0.connect().unwrap();
    conn.execute("DELETE FROM productos WHERE id = 2002", ()).await.unwrap();
    assert!(lista(&t).await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&carpeta);
}

#[tokio::test]
async fn sin_el_modulo_catalogo_no_hay_foto_2() {
    let tienda = 990002;
    let t = base("sin", "FICHA_REPUESTO").await;
    assert_eq!(lista(&t).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(subir(&t, tienda, 2001, &jpeg(100, 100, 0)).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(
        imagenes::quitar_foto_2(Extension(t.clone()), Extension(claims(tienda)), Path(2001)).await.unwrap_err().0,
        StatusCode::FORBIDDEN
    );
    assert!(!std::path::Path::new(&format!("uploads/productos/{}", tienda)).exists(), "no escribe nada");
}
