//! Orden de la lista de productos (migración 0072) contra una base local
//! (schema.sql + todas las migraciones) y los handlers de verdad: por nombre
//! como siempre; con la casilla marcada, por código interno y por número;
//! solo el administrador la cambia.
//!
//!   cargo test --test orden_productos

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, Json};
use minimarket_backend::handlers::{configuracion, productos};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("orden_{}_{}.db", nombre, std::process::id()));
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
        "INSERT INTO categorias (id, nombre) VALUES (900, 'Repuestos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1, '25012-S', 'Amortiguador', 10, 1, 900),
            (2, '19001', 'Bocamaza', 10, 1, 900),
            (3, 'ABC', 'Cable', 10, 1, 900),
            (4, '9001', 'Disco', 10, 1, 900),
            (5, '25012', 'Embrague', 10, 1, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn claims(rol: i64) -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": rol, "nombre_completo": "Prueba", "tienda_id": 1, "exp": usize::MAX
    }))
    .unwrap()
}

async fn codigos(t: &Arc<TenantDb>) -> Vec<String> {
    let Json(lista) = productos::listar_productos(Extension(t.clone())).await.expect("listar");
    lista.into_iter().map(|p| p.codigo).collect()
}

#[tokio::test]
async fn por_nombre_como_siempre_y_por_codigo_al_marcar() {
    let t = base("marcar").await;
    // Por defecto: por nombre, igual que antes.
    assert_eq!(codigos(&t).await, ["25012-S", "19001", "ABC", "9001", "25012"]);
    let Json(o) = configuracion::obtener_orden_productos(Extension(t.clone())).await.unwrap();
    assert!(!o.por_codigo);

    // Marcada: por número, 25012 antes que 25012-S, y los que no empiezan con número al final.
    let Json(o) = configuracion::guardar_orden_productos(
        Extension(t.clone()),
        Extension(claims(1)),
        Json(configuracion::OrdenProductos { por_codigo: true }),
    )
    .await
    .expect("guardar");
    assert!(o.por_codigo);
    assert_eq!(codigos(&t).await, ["9001", "19001", "25012", "25012-S", "ABC"]);

    // Desmarcada: vuelve a nombre.
    let _ = configuracion::guardar_orden_productos(
        Extension(t.clone()),
        Extension(claims(1)),
        Json(configuracion::OrdenProductos { por_codigo: false }),
    )
    .await
    .unwrap();
    assert_eq!(codigos(&t).await, ["25012-S", "19001", "ABC", "9001", "25012"]);
}

#[tokio::test]
async fn solo_el_administrador_la_cambia() {
    let t = base("cajero").await;
    let r = configuracion::guardar_orden_productos(
        Extension(t.clone()),
        Extension(claims(2)),
        Json(configuracion::OrdenProductos { por_codigo: true }),
    )
    .await;
    assert_eq!(r.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(codigos(&t).await, ["25012-S", "19001", "ABC", "9001", "25012"]);
}
