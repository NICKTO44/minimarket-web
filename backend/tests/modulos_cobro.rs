//! Módulos de la licorería: "Pedir con cuánto paga" (SIN_VUELTO, al revés) y
//! "Créditos por cliente" (necesita Crédito; viene en Lavandería).
//!
//!   cargo test --test modulos_cobro

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::rubros;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(rubro: &str) -> Arc<TenantDb> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ruta = std::env::temp_dir().join(format!("modulos_cobro_{}_{}_{}.db", rubro, std::process::id(), n));
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

async fn guardar(t: &Arc<TenantDb>, rubro: &str, modulos: &[&str]) -> Result<Vec<String>, String> {
    let pedido = serde_json::from_value(json!({"rubro": rubro, "modulos": modulos})).unwrap();
    rubros::guardar_negocio(Extension(t.clone()), Extension(admin()), Json(pedido)).await.map(|n| n.0.modulos).map_err(|e| e.1)
}

#[test]
fn lavanderia_trae_creditos_por_cliente() {
    let m = rubros::modulos_del_rubro("LAVANDERIA");
    assert!(m.iter().any(|x| x == "CREDITOS_POR_CLIENTE"));
    assert!(!rubros::modulos_del_rubro("BODEGA").iter().any(|x| x == "CREDITOS_POR_CLIENTE"));
    // "Pedir con cuánto paga" no se guarda en ningún rubro: está encendido para todos.
    assert!(!rubros::modulos_del_rubro("BODEGA").iter().any(|x| x == "SIN_VUELTO"));
}

#[tokio::test]
async fn licoreria_apaga_el_vuelto_y_enciende_creditos_por_cliente() {
    let t = base("BODEGA").await;
    let m = guardar(&t, "BODEGA", &["SIN_VUELTO", "CREDITO", "CREDITOS_POR_CLIENTE"]).await.expect("guardar");
    for x in ["SIN_VUELTO", "CREDITO", "CREDITOS_POR_CLIENTE"] {
        assert!(m.iter().any(|y| y == x), "{}", x);
    }
    // Sin Crédito no se puede.
    let e = guardar(&t, "BODEGA", &["CREDITOS_POR_CLIENTE"]).await.unwrap_err();
    assert!(e.contains("CREDITO"), "{}", e);
}

#[tokio::test]
async fn descuento_en_la_venta_se_enciende_en_cualquier_rubro() {
    // Ningún rubro lo trae; cualquiera lo puede encender y se guarda.
    for rubro in ["BODEGA", "FARMACIA", "LUBRICENTRO"] {
        assert!(!rubros::modulos_del_rubro(rubro).iter().any(|x| x == "DESCUENTO_VENTA"), "{}", rubro);
        let t = base(rubro).await;
        let m = guardar(&t, rubro, &["DESCUENTO_VENTA"]).await.expect("guardar");
        assert!(m.iter().any(|x| x == "DESCUENTO_VENTA"), "{}", rubro);
    }
}
