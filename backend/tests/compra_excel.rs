//! Módulo "Compras por Excel": viene con el rubro Farmacia y cualquier otro
//! negocio lo puede encender (no es de un solo rubro).
//!
//!   cargo test --test compra_excel

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::rubros;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(rubro: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("compra_excel_{}_{}.db", rubro, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET rubro = ?1, modulos = ''", libsql::params![rubro]).await.unwrap();
    Arc::new(TenantDb(Arc::new(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "admin".into(), rol_id: 1, nombre_completo: "Dueño".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

#[test]
fn farmacia_nueva_arranca_con_compras_por_excel() {
    assert!(rubros::modulos_del_rubro("FARMACIA").iter().any(|m| m == "COMPRA_EXCEL"));
    // Los demás rubros no lo traen solo.
    assert!(!rubros::modulos_del_rubro("BODEGA").iter().any(|m| m == "COMPRA_EXCEL"));
}

#[tokio::test]
async fn cualquier_negocio_lo_puede_encender() {
    for rubro in ["BODEGA", "FARMACIA"] {
        let t = base(rubro).await;
        let pedido = serde_json::from_value(json!({"rubro": rubro, "modulos": ["COMPRA_EXCEL"]})).unwrap();
        let negocio = rubros::guardar_negocio(Extension(t.clone()), Extension(admin()), Json(pedido))
            .await
            .unwrap_or_else(|e| panic!("{}: {}", rubro, e.1))
            .0;
        assert!(negocio.modulos.iter().any(|m| m == "COMPRA_EXCEL"), "{}", rubro);
    }
}
