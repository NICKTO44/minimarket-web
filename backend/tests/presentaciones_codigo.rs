//! Presentaciones con código de barras y carga desde Excel (migración 0064).
//!
//!   cargo test --test presentaciones_codigo

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::{presentaciones, productos};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn licoreria(nombre: &str, con_modulo: bool) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("pres_codigo_{}_{}.db", nombre, std::process::id()));
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
        conn.execute_batch(&std::fs::read_to_string(m).unwrap()).await.unwrap_or_else(|e| panic!("{}: {}", m.display(), e));
    }
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'BODEGA', modulos = ?1",
        libsql::params![if con_modulo { "PRESENTACIONES,PRECIO_VENTA" } else { "" }],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'admin', 'x', 'Admin', 1), (2, 'cajero', 'x', 'Cajero', 2);
         INSERT INTO categorias (id, nombre) VALUES (10, 'Cervezas');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1, '7753749007955', 'CERVEZA CUSQUEÑA TRIGO LATA 473 ML', 7, 125, 10),
            (2, '7753749002035', 'CERVEZA PILSEN LATA 473 ML', 6.5, 360, 10);",
    )
    .await
    .unwrap();
    Arc::new(TenantDb(Arc::new(db)))
}

fn rol(rol_id: i64) -> Claims {
    Claims { sub: rol_id, username: "x".into(), rol_id, nombre_completo: "X".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

fn filas(v: serde_json::Value, solo_revisar: bool) -> Json<presentaciones::ImportarPresentaciones> {
    Json(serde_json::from_value(json!({ "filas": v, "solo_revisar": solo_revisar })).unwrap())
}

#[tokio::test]
async fn carga_presentaciones_con_codigo_desde_el_excel() {
    let t = licoreria("carga", true).await;
    let archivo = json!([
        {"fila": 2, "codigo_producto": "7753749007955", "nombre": "Six pack x6", "factor": 6, "precio": 35, "codigo_barras": "7753749007962"},
        {"fila": 3, "codigo_producto": "7753749002035", "nombre": "Six pack x6", "factor": 6, "precio": 34, "codigo_barras": "7753749008693"},
        {"fila": 4, "codigo_producto": "NOEXISTE", "nombre": "Six pack x6", "factor": 6, "precio": 30},
        {"fila": 5, "codigo_producto": "7753749002035", "nombre": "Caja x12", "factor": 12, "precio": 0}
    ]);
    // Revisar no guarda nada.
    let r = presentaciones::importar(Extension(t.clone()), Extension(rol(1)), filas(archivo.clone(), true)).await.unwrap().0;
    assert_eq!((r.nuevas, r.actualizadas, r.errores, r.guardado), (2, 0, 2, false));
    assert!(presentaciones::todas(Extension(t.clone())).await.unwrap().0.presentaciones.is_empty());

    // Guardar: entran las válidas.
    let r = presentaciones::importar(Extension(t.clone()), Extension(rol(1)), filas(archivo.clone(), false)).await.unwrap().0;
    assert!(r.guardado);
    let todas = presentaciones::todas(Extension(t.clone())).await.unwrap().0.presentaciones;
    assert_eq!(todas.len(), 2);
    let cusq = todas.iter().find(|p| p.producto_id == Some(1)).unwrap();
    assert_eq!((cusq.factor, cusq.precio, cusq.codigo_barras.as_deref()), (6.0, 35.0, Some("7753749007962")));

    // Volver a cargar con otro precio: actualiza por nombre, no duplica.
    let otra = json!([{"fila": 2, "codigo_producto": "7753749007955", "nombre": "six pack x6", "factor": 6, "precio": 36, "codigo_barras": "7753749007962"}]);
    let r = presentaciones::importar(Extension(t.clone()), Extension(rol(1)), filas(otra, false)).await.unwrap().0;
    assert_eq!((r.nuevas, r.actualizadas), (0, 1));
    let todas = presentaciones::todas(Extension(t.clone())).await.unwrap().0.presentaciones;
    assert_eq!(todas.len(), 2);
    assert_eq!(todas.iter().find(|p| p.producto_id == Some(1)).unwrap().precio, 36.0);
}

#[tokio::test]
async fn el_codigo_no_se_cruza_con_productos_ni_otras_presentaciones() {
    let t = licoreria("cruce", true).await;
    // El código de un producto no puede ser de una presentación.
    let r = presentaciones::importar(
        Extension(t.clone()),
        Extension(rol(1)),
        filas(json!([{"fila": 2, "codigo_producto": "7753749007955", "nombre": "Six pack x6", "factor": 6, "precio": 35, "codigo_barras": "7753749002035"}]), false),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(r.errores, 1);
    assert!(r.filas[0].motivo.as_deref().unwrap().contains("código del producto"));

    // Ni el de otra presentación; y un producto nuevo tampoco puede usarlo.
    presentaciones::importar(
        Extension(t.clone()),
        Extension(rol(1)),
        filas(json!([{"fila": 2, "codigo_producto": "7753749007955", "nombre": "Six pack x6", "factor": 6, "precio": 35, "codigo_barras": "7753749007962"}]), false),
    )
    .await
    .unwrap();
    let r = presentaciones::importar(
        Extension(t.clone()),
        Extension(rol(1)),
        filas(json!([{"fila": 2, "codigo_producto": "7753749002035", "nombre": "Six pack x6", "factor": 6, "precio": 34, "codigo_barras": "7753749007962"}]), false),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(r.errores, 1);
    let nuevo = serde_json::from_value(json!({
        "codigo": "7753749007962", "nombre": "Otro", "descripcion": null, "precio": 5, "stock": 0,
        "stock_minimo": 0, "unidad_medida": "UNIDAD", "categoria_id": 10
    }))
    .unwrap();
    assert!(productos::agregar_producto(Extension(t.clone()), Json(nuevo)).await.is_err());
}

#[tokio::test]
async fn solo_admin_y_con_el_modulo() {
    let t = licoreria("permisos", true).await;
    let f = json!([{"fila": 2, "codigo_producto": "7753749007955", "nombre": "Six pack x6", "factor": 6, "precio": 35}]);
    assert!(presentaciones::importar(Extension(t.clone()), Extension(rol(2)), filas(f.clone(), true)).await.is_err());
    let sin = licoreria("sin_modulo", false).await;
    assert!(presentaciones::importar(Extension(sin.clone()), Extension(rol(1)), filas(f, true)).await.is_err());
}
