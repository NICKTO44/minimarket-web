//! Pantallas por usuario: guardar, leer y quién puede.
//!   cargo test --test pantallas

use std::sync::Arc;

use axum::{extract::{Extension, Path}, Json};
use minimarket_backend::handlers::configuracion;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("pantallas_{}_{}.db", nombre, std::process::id()));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.unwrap();
    let conn = db.connect().unwrap();
    conn.execute_batch(include_str!("../schema.sql")).await.unwrap();
    let mut migraciones: Vec<_> = std::fs::read_dir("migraciones").unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false)).collect();
    migraciones.sort();
    for m in &migraciones {
        conn.execute_batch(&std::fs::read_to_string(m).unwrap()).await.unwrap_or_else(|e| panic!("{}: {}", m.display(), e));
    }
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'admin', 'x', 'Dueño', 1);
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (2, 'juan', 'x', 'Juan', 2);",
    )
    .await
    .unwrap();
    Arc::new(TenantDb(Arc::new(db)))
}

fn quien(id: i64, rol: i64) -> Claims {
    Claims { sub: id, username: "x".into(), rol_id: rol, nombre_completo: "x".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn guardar(t: &Arc<TenantDb>, por: Claims, id: i64, ocultas: serde_json::Value) -> Result<Vec<String>, String> {
    configuracion::guardar_pantallas(Extension(t.clone()), Extension(por), Path(id), Json(serde_json::from_value(json!({"ocultas": ocultas})).unwrap()))
        .await
        .map(|r| r.0)
        .map_err(|e| e.1)
}

#[tokio::test]
async fn el_admin_elige_las_pantallas_de_un_cajero() {
    let t = base("elige").await;
    // Al inicio, como siempre: nada oculto.
    let mias = configuracion::mis_pantallas(Extension(t.clone()), Extension(quien(2, 2))).await.unwrap().0;
    assert!(mias.is_empty());

    // Le oculta Reportes e Historial; "CAJA" se ignora porque tiene Punto de venta.
    let g = guardar(&t, quien(1, 1), 2, json!(["REPORTES", "HISTORIAL_CAJA", "CAJA", "INVENTADA"])).await.unwrap();
    assert_eq!(g, vec!["REPORTES".to_string(), "HISTORIAL_CAJA".to_string()]);
    let mias = configuracion::mis_pantallas(Extension(t.clone()), Extension(quien(2, 2))).await.unwrap().0;
    assert_eq!(mias, g);
    let lista = configuracion::listar_usuarios(Extension(t.clone()), Extension(quien(1, 1))).await.unwrap().0;
    assert_eq!(lista.iter().find(|u| u.id == 2).unwrap().pantallas_ocultas, g);
    assert!(lista.iter().find(|u| u.id == 1).unwrap().pantallas_ocultas.is_empty());

    // Vacía: vuelve a ver todo (queda NULL, como un usuario nuevo).
    assert!(guardar(&t, quien(1, 1), 2, json!([])).await.unwrap().is_empty());
    let conn = t.0.connect().unwrap();
    let f = conn.query("SELECT pantallas_ocultas FROM usuarios WHERE id = 2", ()).await.unwrap().next().await.unwrap().unwrap();
    assert_eq!(f.get::<Option<String>>(0).unwrap(), None);
}

#[tokio::test]
async fn solo_el_admin_y_nunca_al_admin() {
    let t = base("quien").await;
    assert!(guardar(&t, quien(2, 2), 2, json!(["REPORTES"])).await.unwrap_err().contains("administrador"));
    assert!(guardar(&t, quien(1, 1), 1, json!(["REPORTES"])).await.unwrap_err().contains("siempre ve"));
    assert!(guardar(&t, quien(1, 1), 99, json!([])).await.unwrap_err().contains("no existe"));
    // El administrador nunca tiene ocultas.
    assert!(configuracion::mis_pantallas(Extension(t.clone()), Extension(quien(1, 1))).await.unwrap().0.is_empty());
}
