//! Ganancias → dinero invertido en mercadería (stock × costo).
//!
//!   cargo test --test inventario_invertido

use std::sync::Arc;

use axum::extract::Extension;
use minimarket_backend::handlers::ganancias;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;

async fn negocio(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("invertido_{}_{}.db", nombre, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET modulos = ?1", libsql::params![modulos]).await.unwrap();
    conn.execute_batch(
        "INSERT INTO categorias (id, nombre) VALUES (1, 'Cervezas'), (2, 'Whisky');
         INSERT INTO productos (id, codigo, nombre, precio, precio_compra, stock, categoria_id, controla_stock, activo) VALUES
            (1, 'C1', 'Cusqueña lata', 7, 4.5, 100, 1, 1, 1),
            (2, 'C2', 'Pilsen lata', 6.5, 4, 0, 1, 1, 1),
            (3, 'W1', 'Johnnie Red', 80, 60, 10, 2, 1, 1),
            (4, 'W2', 'Sin precio', 50, NULL, 3, 2, 1, 1),
            (5, 'S1', 'Servicio', 10, 5, 99, 1, 0, 1),
            (6, 'X1', 'Inactivo', 10, 5, 99, 1, 1, 0);",
    )
    .await
    .unwrap();
    Arc::new(TenantDb(Arc::new(db)))
}

fn rol(rol_id: i64) -> Claims {
    Claims { sub: 1, username: "x".into(), rol_id, nombre_completo: "X".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

#[tokio::test]
async fn suma_stock_por_costo() {
    let t = negocio("suma", "GANANCIAS").await;
    let r = ganancias::inventario(Extension(t.clone()), Extension(rol(1))).await.expect("reporte").0;
    // 100 × 4.50 + 10 × 60 = 1 050; a venta: 700 + 800 = 1 500.
    assert_eq!(r.invertido, 1050.0);
    assert_eq!(r.valor_venta, 1500.0);
    assert_eq!(r.ganancia_por_realizar, 450.0);
    assert_eq!(r.productos_con_stock, 2);
    assert_eq!(r.sin_precio, 1);
    // De mayor a menor: Whisky (600) y luego Cervezas (450).
    assert_eq!(r.categorias[0].categoria, "Whisky");
    assert_eq!(r.categorias[0].invertido, 600.0);
    assert_eq!(r.categorias[1].invertido, 450.0);
}

#[tokio::test]
async fn solo_admin_y_con_el_modulo() {
    let t = negocio("permisos", "GANANCIAS").await;
    assert!(ganancias::inventario(Extension(t.clone()), Extension(rol(2))).await.is_err());
    let sin = negocio("sin_modulo", "").await;
    assert!(ganancias::inventario(Extension(sin), Extension(rol(1))).await.is_err());
}
