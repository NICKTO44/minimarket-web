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
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ruta = std::env::temp_dir().join(format!("compra_excel_{}_{}_{}.db", rubro, std::process::id(), n));
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

#[tokio::test]
async fn recuerda_el_excel_de_cada_proveedor() {
    use axum::extract::Path;
    use minimarket_backend::handlers::compra_excel;

    let t = base("BODEGA").await;
    t.0.connect()
        .unwrap()
        .execute_batch(
            "INSERT INTO categorias (id, nombre) VALUES (900, 'Medicamentos');
             INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
                (1001, 'PARA-500', 'Paracetamol 500mg', 10, 0, 900),
                (1002, 'IBU-400', 'Ibuprofeno 400mg', 8, 0, 900);
             INSERT INTO proveedores (id, nombre, tipo_documento) VALUES (7, 'Distribuidora Farma', 'RUC');",
        )
        .await
        .unwrap();
    let aprender = |cuerpo: serde_json::Value| {
        let t = t.clone();
        async move {
            compra_excel::aprender(Extension(t), Path(7), Json(serde_json::from_value(cuerpo).unwrap()))
                .await
                .map_err(|e| e.0)
        }
    };

    // Sin el módulo, no.
    assert_eq!(aprender(json!({"enlaces": []})).await.unwrap_err(), axum::http::StatusCode::FORBIDDEN);
    t.0.connect().unwrap().execute("UPDATE configuracion_tienda SET modulos = 'COMPRA_EXCEL'", ()).await.unwrap();

    // Columnas y enlaces; un producto que no existe se ignora.
    aprender(json!({
        "columnas": {"codigo": 0, "producto": 1, "cantidad": 3, "costo": 4},
        "enlaces": [
            {"clave": "C:A-77", "producto_id": 1001},
            {"clave": "N:ibuprofeno400mgx100", "producto_id": 1002},
            {"clave": "C:FANTASMA", "producto_id": 999}
        ]
    }))
    .await
    .expect("aprender");
    // Uno nuevo de la misma clave reemplaza al anterior.
    aprender(json!({"enlaces": [{"clave": "C:A-77", "producto_id": 1002}]})).await.expect("corregir");
    // Clave sin prefijo: no.
    assert_eq!(
        aprender(json!({"enlaces": [{"clave": "A-77", "producto_id": 1001}]})).await.unwrap_err(),
        axum::http::StatusCode::BAD_REQUEST
    );

    let r = compra_excel::obtener(Extension(t.clone()), Path(7)).await.expect("obtener").0;
    assert_eq!(r.columnas.unwrap()["cantidad"], 3);
    let mut enlaces: Vec<(String, i64)> = r.enlaces.into_iter().map(|e| (e.clave, e.producto_id)).collect();
    enlaces.sort();
    assert_eq!(enlaces, vec![("C:A-77".into(), 1002), ("N:ibuprofeno400mgx100".into(), 1002)]);

    // Si se borra el producto, su enlace ya no sale.
    t.0.connect().unwrap().execute("DELETE FROM productos WHERE id = 1002", ()).await.unwrap();
    let r = compra_excel::obtener(Extension(t.clone()), Path(7)).await.unwrap().0;
    assert!(r.enlaces.is_empty());

    // Proveedor que no existe.
    let e = compra_excel::aprender(Extension(t.clone()), Path(99), Json(serde_json::from_value(json!({"enlaces": []})).unwrap()))
        .await
        .unwrap_err();
    assert_eq!(e.0, axum::http::StatusCode::NOT_FOUND);
}
