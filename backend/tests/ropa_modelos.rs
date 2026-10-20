//! Ropa: convertir un producto creado sin tallas en un modelo con tallas
//! (POST /modelos/desde-producto/:id) y agregarle más tallas después.
//!
//!   cargo test --test ropa_modelos

use std::sync::Arc;

use axum::{extract::{Extension, Path}, http::StatusCode, Json};
use minimarket_backend::handlers::variantes;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("ropa_{}_{}.db", nombre, std::process::id()));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.unwrap();
    let conn = db.connect().unwrap();
    conn.execute_batch(include_str!("../schema.sql")).await.unwrap();
    let mut m: Vec<_> = std::fs::read_dir("migraciones").unwrap().filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false)).collect();
    m.sort();
    for f in &m {
        conn.execute_batch(&std::fs::read_to_string(f).unwrap()).await.unwrap();
    }
    conn.execute("UPDATE configuracion_tienda SET rubro = 'ROPA', modulos = ?1", libsql::params![modulos]).await.unwrap();
    conn.execute_batch(
        "INSERT INTO categorias (id, nombre) VALUES (900, 'Polos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, precio_compra) VALUES
            (1, 'POLO-AZ', 'Polo oversize', 45, 6, 900, 20),
            (2, 'GORRA', 'Gorra', 25, 3, 900, 10);",
    )
    .await
    .unwrap();
    Arc::new(TenantDb(Arc::new(db)))
}

async fn convertir(t: &Arc<TenantDb>, id: i64, v: serde_json::Value) -> Result<variantes::ModeloGuardado, (StatusCode, String)> {
    variantes::modelo_desde_producto(Extension(t.clone()), Path(id), Json(serde_json::from_value(v).unwrap()))
        .await
        .map(|Json(m)| m)
}

async fn texto(t: &Arc<TenantDb>, sql: &str) -> String {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query(sql, ()).await.unwrap();
    f.next().await.unwrap().unwrap().get::<String>(0).unwrap()
}

#[tokio::test]
async fn producto_sin_tallas_pasa_a_modelo_y_recibe_mas_tallas() {
    let t = base("convertir", "CREDITO,VARIANTES,CAMBIOS").await;
    assert!(convertir(&t, 1, json!({})).await.is_err());
    let m = convertir(&t, 1, json!({"talla": " m ", "color": "Azul"})).await.expect("convertido");
    assert_eq!(m.variantes.len(), 1);
    assert_eq!(m.variantes[0].id, 1);
    assert_eq!(m.variantes[0].nombre, "Polo oversize · M · Azul");
    assert_eq!(texto(&t, "SELECT codigo FROM productos WHERE id = 1").await, "POLO-AZ");
    // Ya es parte de un modelo.
    assert_eq!(convertir(&t, 1, json!({"talla": "L"})).await.unwrap_err().0, StatusCode::CONFLICT);

    // Se le agregan tallas con el formulario de siempre; el stock de la M no se toca.
    let Json(m2) = variantes::actualizar_modelo(
        Extension(t.clone()),
        Path(m.modelo_id),
        Json(serde_json::from_value(json!({
            "nombre": "Polo oversize", "categoria_id": 900, "precio_compra": 20,
            "variantes": [
                {"id": 1, "talla": "M", "color": "Azul", "codigo": "POLO-AZ", "precio": 45},
                {"talla": "L", "color": "Azul", "precio": 48, "stock": 0},
                {"talla": "S", "color": "Negro", "precio": 45, "stock": 0}
            ]
        })).unwrap()),
    )
    .await
    .expect("tallas nuevas");
    assert_eq!(m2.variantes.len(), 3);
    assert_eq!(texto(&t, "SELECT CAST(stock AS TEXT) FROM productos WHERE id = 1").await, "6.0");

    // Otro producto con el mismo nombre de un modelo existente no se convierte.
    let conn = t.0.connect().unwrap();
    conn.execute("INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES (30, 'X', 'Polo oversize', 10, 0, 900)", ())
        .await
        .unwrap();
    assert_eq!(convertir(&t, 30, json!({"talla": "XL"})).await.unwrap_err().0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn sin_el_modulo_no_se_convierte() {
    let t = base("sin-modulo", "").await;
    assert_eq!(convertir(&t, 2, json!({"talla": "U"})).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(texto(&t, "SELECT nombre FROM productos WHERE id = 2").await, "Gorra");
}
