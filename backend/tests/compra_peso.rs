//! Compra por peso (POST /compras/peso, módulo COMPRA_PESO) contra una base
//! local (schema.sql + todas las migraciones) y los handlers de verdad:
//!   - sube el stock de cada material, queda su último precio de compra y se
//!     descuenta del efectivo de la caja; devuelve lo del ticket;
//!   - no se paga más de lo que hay en la caja, ni sin caja abierta;
//!   - sin el módulo responde 403.
//!
//!   cargo test --test compra_peso

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, Json};
use minimarket_backend::handlers::{cajas, compra_peso};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("compra_peso_{}_{}.db", nombre, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET rubro = 'BODEGA', modulos = ?1", libsql::params![modulos.to_string()])
        .await
        .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'admin', 'x', 'Dueño', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Metales');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, precio_compra, unidad_medida) VALUES
            (1, 'COB', 'Cobre brillante', 42, 0, 900, 38, 'KG'),
            (2, 'CAR', 'Cartón', 0.4, 10, 900, 0.2, 'KG');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "admin".into(), rol_id: 1, nombre_completo: "Dueño".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir_caja(t: &Arc<TenantDb>, monto: f64) -> i64 {
    let r = cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: monto, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    r.0.caja_id.expect("id de la caja")
}

async fn comprar(t: &Arc<TenantDb>, cuerpo: serde_json::Value) -> Result<compra_peso::CompraPesoHecha, (StatusCode, String)> {
    compra_peso::registrar(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(cuerpo).unwrap()))
        .await
        .map(|r| r.0)
}

async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query(sql, ()).await.unwrap();
    match f.next().await.unwrap().unwrap().get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: {:?}", sql, otro),
    }
}

#[tokio::test]
async fn compra_sube_stock_y_descuenta_caja() {
    let t = base("ok", "COMPRA_PESO").await;
    let caja = abrir_caja(&t, 100.0).await;

    // 2,35 kg de cobre a S/ 40 (hoy subió) + 12,5 kg de cartón a S/ 0,20.
    let hecha = comprar(
        &t,
        json!({"persona": "Don Lucho", "lineas": [
            {"producto_id": 1, "cantidad": 2.35, "precio": 40},
            {"producto_id": 2, "cantidad": 12.5, "precio": 0.2}
        ]}),
    )
    .await
    .expect("compra");
    assert_eq!(hecha.total, 96.5);
    assert_eq!(hecha.lineas.len(), 2);
    assert_eq!(hecha.lineas[0].nombre, "Cobre brillante");
    assert_eq!(hecha.lineas[0].subtotal, 94.0);
    assert_eq!(hecha.persona.as_deref(), Some("Don Lucho"));
    assert!(!hecha.folio.is_empty());

    // Stock y último precio de compra.
    assert!((real(&t, "SELECT stock FROM productos WHERE id = 1").await - 2.35).abs() < 1e-9);
    assert!((real(&t, "SELECT stock FROM productos WHERE id = 2").await - 22.5).abs() < 1e-9);
    assert_eq!(real(&t, "SELECT precio_compra FROM productos WHERE id = 1").await, 40.0);

    // La caja: de S/ 100 quedan S/ 3,50.
    let conn = t.0.connect().unwrap();
    let efectivo = cajas::efectivo_en_caja(&conn, caja).await.unwrap();
    assert!((efectivo - 3.5).abs() < 0.001, "efectivo {}", efectivo);

    // No se paga más de lo que hay en la caja.
    let e = comprar(&t, json!({"lineas": [{"producto_id": 1, "cantidad": 1, "precio": 40}]})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!((real(&t, "SELECT stock FROM productos WHERE id = 1").await - 2.35).abs() < 1e-9);
}

#[tokio::test]
async fn sin_caja_abierta_no_compra() {
    let t = base("sincaja", "COMPRA_PESO").await;
    let e = comprar(&t, json!({"lineas": [{"producto_id": 2, "cantidad": 1, "precio": 0.2}]})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!(e.1.contains("caja"), "{}", e.1);
}

#[tokio::test]
async fn pagado_con_plata_de_otro_lado_no_toca_la_caja() {
    let t = base("otrolado", "COMPRA_PESO").await;
    let caja = abrir_caja(&t, 0.0).await;
    let hecha = comprar(&t, json!({"tipo_pago": "EFECTIVO", "lineas": [{"producto_id": 1, "cantidad": 2.45, "precio": 0.2}]}))
        .await
        .expect("compra con efectivo de otro lado");
    assert_eq!(hecha.total, 0.49);
    assert!((real(&t, "SELECT stock FROM productos WHERE id = 1").await - 2.45).abs() < 1e-9);
    let conn = t.0.connect().unwrap();
    assert!(cajas::efectivo_en_caja(&conn, caja).await.unwrap().abs() < 0.001);
}

#[tokio::test]
async fn sin_el_modulo_responde_403() {
    let t = base("apagado", "").await;
    let _ = abrir_caja(&t, 100.0).await;
    let e = comprar(&t, json!({"lineas": [{"producto_id": 2, "cantidad": 1, "precio": 0.2}]})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
}
