//! Importar productos con "Actualizar todo" (existentes = ACTUALIZAR_TODO)
//! contra una base local (schema.sql + todas las migraciones) y el handler de
//! verdad: actualiza nombre, descripción, categoría, unidad, stock mínimo,
//! precio y stock de los que ya existen; lo que el archivo no trae no se
//! borra; y "ACTUALIZAR" sigue sin tocar el nombre.
//!
//!   cargo test --test importar_actualizar_todo

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::importacion;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("imp_todo_{}_{}.db", nombre, std::process::id()));
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
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Suspensión');
         INSERT INTO productos (id, codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida, categoria_id) VALUES
            (2001, 'MSR-31002L', 'Trapecio LH', 'Aveo 2008', 100, 10, 2, 'UNIDAD', 900),
            (2002, 'MSR-31003L', 'Trapecio Chevy', 'Chevy Taxi', 70, 5, 2, 'UNIDAD', 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn claims() -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": 1, "nombre_completo": "Admin", "tienda_id": 1, "exp": usize::MAX
    }))
    .unwrap()
}

async fn importar(t: &Arc<TenantDb>, filas: serde_json::Value, existentes: &str) -> importacion::ResultadoImportacion {
    let payload: importacion::Importacion = serde_json::from_value(json!({
        "filas": filas, "solo_revisar": false, "existentes": existentes
    }))
    .unwrap();
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(payload)).await.expect("importa");
    r
}

/// (nombre, descripcion, precio, stock, stock_minimo, unidad, categoria)
async fn producto(t: &Arc<TenantDb>, id: i64) -> (String, Option<String>, f64, f64, f64, String, String) {
    let conn = t.0.connect().unwrap();
    let mut f = conn
        .query(
            "SELECT p.nombre, p.descripcion, CAST(p.precio AS REAL), CAST(p.stock AS REAL), CAST(p.stock_minimo AS REAL), p.unidad_medida, c.nombre
             FROM productos p JOIN categorias c ON c.id = p.categoria_id WHERE p.id = ?1",
            libsql::params![id],
        )
        .await
        .unwrap();
    let r = f.next().await.unwrap().unwrap();
    (
        r.get(0).unwrap(),
        r.get::<String>(1).ok(),
        r.get(2).unwrap(),
        r.get(3).unwrap(),
        r.get(4).unwrap(),
        r.get(5).unwrap(),
        r.get(6).unwrap(),
    )
}

#[tokio::test]
async fn actualizar_todo_cambia_lo_que_trae_el_archivo() {
    let t = base("todo").await;
    let r = importar(
        &t,
        json!([
            // Todo cambia, con comillas en el nombre y categoría nueva.
            {"fila": 2, "codigo": "msr-31002l", "nombre": "Trapecio delantero inferior LH \"Masaru\"", "descripcion": "Aveo 2008-2014; Optra",
             "precio": 120.5, "stock": 7, "stock_minimo": 3, "unidad": "PIEZA", "categoria": "Dirección"},
            // Solo nombre y precio: descripción, categoría, unidad, stock y mínimo se quedan.
            {"fila": 3, "codigo": "MSR-31003L", "nombre": "Trapecio Chevy Taxi LH", "precio": 75},
            // Uno nuevo, como siempre.
            {"fila": 4, "codigo": "NUEVO-1", "nombre": "Rótula", "precio": 30, "categoria": "Suspensión"}
        ]),
        "ACTUALIZAR_TODO",
    )
    .await;
    assert!(r.guardado);
    assert_eq!((r.creados, r.actualizados), (1, 2));
    assert!(r.categorias_nuevas.iter().any(|c| c == "Dirección"), "{:?}", r.categorias_nuevas);

    assert_eq!(
        producto(&t, 2001).await,
        (
            "Trapecio delantero inferior LH \"Masaru\"".to_string(),
            Some("Aveo 2008-2014; Optra".to_string()),
            120.5,
            7.0,
            3.0,
            "PIEZA".to_string(),
            "Dirección".to_string()
        )
    );
    assert_eq!(
        producto(&t, 2002).await,
        ("Trapecio Chevy Taxi LH".to_string(), Some("Chevy Taxi".to_string()), 75.0, 5.0, 2.0, "UNIDAD".to_string(), "Suspensión".to_string())
    );
}

#[tokio::test]
async fn actualizar_precio_y_stock_sigue_sin_tocar_el_nombre() {
    let t = base("solo").await;
    let r = importar(
        &t,
        json!([{"fila": 2, "codigo": "MSR-31002L", "nombre": "Otro nombre", "descripcion": "Otra", "precio": 99, "stock": 1, "categoria": "Dirección"}]),
        "ACTUALIZAR",
    )
    .await;
    assert_eq!(r.actualizados, 1);
    assert!(r.categorias_nuevas.is_empty());
    assert_eq!(
        producto(&t, 2001).await,
        ("Trapecio LH".to_string(), Some("Aveo 2008".to_string()), 99.0, 1.0, 2.0, "UNIDAD".to_string(), "Suspensión".to_string())
    );
}

#[tokio::test]
async fn saltar_no_toca_nada() {
    let t = base("saltar").await;
    let r = importar(&t, json!([{"fila": 2, "codigo": "MSR-31002L", "nombre": "Otro", "precio": 1}]), "SALTAR").await;
    assert_eq!(r.actualizados, 0);
    assert_eq!(producto(&t, 2001).await.0, "Trapecio LH");
}

#[test]
fn sql_por_tanda_solo_lo_que_vino() {
    let (sql, valores) = importacion::sql_actualizar_textos(&[
        importacion::CambiosTexto { id: 5, nombre: "A".into(), descripcion: None, categoria_id: None, unidad: None, stock_minimo: None },
        importacion::CambiosTexto { id: 6, nombre: "B".into(), descripcion: Some("d".into()), categoria_id: None, unidad: None, stock_minimo: Some(1.0) },
    ]);
    assert!(sql.contains("nombre = CASE id WHEN ?1 THEN ?2 WHEN ?3 THEN ?4 ELSE nombre END"), "{}", sql);
    assert!(sql.contains("descripcion = CASE id WHEN ?5 THEN ?6 ELSE descripcion END"), "{}", sql);
    assert!(!sql.contains("categoria_id"), "{}", sql);
    assert!(sql.contains("stock_minimo = CASE id WHEN ?7 THEN ?8 ELSE stock_minimo END"), "{}", sql);
    assert!(sql.ends_with("WHERE id IN (5,6)"), "{}", sql);
    assert_eq!(valores.len(), 8);
}
