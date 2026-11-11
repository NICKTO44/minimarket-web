//! Ingreso rápido de productos (POST /compras/rapida), contra una base
//! local (schema.sql + todas las migraciones) y los handlers de verdad.
//!
//!   cargo test --test compra_rapida

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::{cajas, compra_rapida};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("compra_rapida_{}_{}.db", nombre, std::process::id()));
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
    // El módulo Ingreso rápido siempre encendido, salvo en la prueba que lo apaga.
    let modulos = if modulos == "APAGADO" { String::new() } else if modulos.is_empty() { "INGRESO_RAPIDO".to_string() } else { format!("INGRESO_RAPIDO,{}", modulos) };
    conn.execute("UPDATE configuracion_tienda SET rubro = 'BODEGA', modulos = ?1", libsql::params![modulos]).await.unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'admin', 'x', 'Dueño', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Bebidas');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, precio_compra) VALUES
            (1001, '7750001', 'Inka Kola 500 ml', 2.5, 4, 900, 1.4);
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, lleva_vencimiento) VALUES
            (1002, '7750002', 'Yogurt fresa', 4, 0, 900, 1);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "admin".into(), rol_id: 1, nombre_completo: "Dueño".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn ingresar(t: &Arc<TenantDb>, cuerpo: serde_json::Value) -> Result<compra_rapida::ResultadoIngreso, String> {
    compra_rapida::ingreso_rapido(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(cuerpo).unwrap()))
        .await
        .map(|r| r.0)
        .map_err(|e| e.1)
}

async fn uno<T: libsql::params::IntoParams>(t: &Arc<TenantDb>, sql: &str, p: T) -> Option<libsql::Row> {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query(sql, p).await.unwrap();
    f.next().await.unwrap()
}

async fn contar(t: &Arc<TenantDb>, sql: &str) -> i64 {
    uno(t, sql, ()).await.unwrap().get(0).unwrap()
}

#[tokio::test]
async fn nuevos_y_reponer_en_un_paso() {
    let t = base("completo", "PRESENTACIONES").await;
    let r = ingresar(
        &t,
        json!({"tipo_pago": "EFECTIVO", "items": [
            // Nuevo: 2 cajas de 9 botellas a S/ 13.50 la caja, y se vende también la caja.
            {"nuevo": {"nombre": "Coca Cola 500 ml", "unidad_medida": "UNIDAD", "categoria_id": 900, "precio": 2.5},
             "cantidad": 18, "precio_compra": 1.5, "vender_caja": {"unidades": 9, "precio": 20}},
            // Reponer uno que ya existe.
            {"producto_id": 1001, "cantidad": 12, "precio_compra": 1.45}
        ]}),
    )
    .await
    .expect("ingreso");
    assert_eq!((r.creados, r.repuestos), (1, 1));
    assert!(r.mensaje.contains("S/ 44.40"), "{}", r.mensaje);

    // El nuevo: código de 8 dígitos, stock 18, costo 1.50 y su "Caja x9".
    let p = uno(&t, "SELECT id, codigo, CAST(stock AS REAL), CAST(precio_compra AS REAL), CAST(precio AS REAL) FROM productos WHERE nombre = 'Coca Cola 500 ml'", ())
        .await
        .expect("producto nuevo");
    let id: i64 = p.get(0).unwrap();
    let codigo: String = p.get(1).unwrap();
    assert!(codigo.len() == 8 && codigo.chars().all(|c| c.is_ascii_digit()), "{}", codigo);
    assert_eq!((p.get::<f64>(2).unwrap(), p.get::<f64>(3).unwrap(), p.get::<f64>(4).unwrap()), (18.0, 1.5, 2.5));
    let caja = uno(&t, "SELECT nombre, CAST(factor AS REAL), CAST(precio AS REAL) FROM producto_presentaciones WHERE producto_id = ?1 AND activo = 1", libsql::params![id])
        .await
        .expect("presentación");
    assert_eq!((caja.get::<String>(0).unwrap(), caja.get::<f64>(1).unwrap(), caja.get::<f64>(2).unwrap()), ("Caja x9".to_string(), 9.0, 20.0));

    // El que ya existía: 4 + 12 y su nuevo precio de compra.
    let e = uno(&t, "SELECT CAST(stock AS REAL), CAST(precio_compra AS REAL) FROM productos WHERE id = 1001", ()).await.unwrap();
    assert_eq!((e.get::<f64>(0).unwrap(), e.get::<f64>(1).unwrap()), (16.0, 1.45));

    // Es una compra recibida, del proveedor "COMPRA RÁPIDA" sin documento.
    let c = uno(&t, "SELECT c.estado, CAST(c.total AS REAL), p.nombre, p.tipo_documento FROM compras c JOIN proveedores p ON p.id = c.proveedor_id WHERE c.id = ?1", libsql::params![r.compra_id])
        .await
        .unwrap();
    assert_eq!(c.get::<String>(0).unwrap(), "RECIBIDA");
    assert!((c.get::<f64>(1).unwrap() - 44.4).abs() < 0.001);
    assert_eq!(c.get::<String>(2).unwrap(), compra_rapida::PROVEEDOR_RAPIDO);
    assert_eq!(c.get::<String>(3).unwrap(), "NINGUNO");

    // Las filas leídas en la prueba se sueltan (si no, la base local queda bloqueada).
    drop((p, caja, e, c));

    // Otro ingreso usa el mismo proveedor (no crea otro).
    ingresar(&t, json!({"tipo_pago": "TRANSFERENCIA", "items": [{"producto_id": 1001, "cantidad": 1, "precio_compra": 1.45}]}))
        .await
        .expect("segundo ingreso");
    assert_eq!(contar(&t, "SELECT COUNT(*) FROM proveedores").await, 1);
}

#[tokio::test]
async fn si_algo_esta_mal_no_queda_nada() {
    let t = base("errores", "").await;
    let antes = contar(&t, "SELECT COUNT(*) FROM productos").await;

    // Un nombre que ya existe: hay que reponerlo, no crearlo otra vez.
    let e = ingresar(
        &t,
        json!({"tipo_pago": "EFECTIVO", "items": [
            {"nuevo": {"nombre": "Galleta nueva", "unidad_medida": "UNIDAD", "categoria_id": 900, "precio": 1}, "cantidad": 5, "precio_compra": 0.5},
            {"nuevo": {"nombre": "inka kola 500 ML", "unidad_medida": "UNIDAD", "categoria_id": 900, "precio": 2.5}, "cantidad": 5, "precio_compra": 1}
        ]}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("Ya existe"), "{}", e);

    // Un perecible que ya existe necesita su vencimiento.
    let e = ingresar(&t, json!({"tipo_pago": "EFECTIVO", "items": [{"producto_id": 1002, "cantidad": 6, "precio_compra": 2}]}))
        .await
        .unwrap_err();
    assert!(e.contains("vencimiento"), "{}", e);

    // La caja para vender sin el módulo Presentaciones.
    let e = ingresar(
        &t,
        json!({"tipo_pago": "EFECTIVO", "items": [
            {"nuevo": {"nombre": "Agua 625 ml", "unidad_medida": "UNIDAD", "categoria_id": 900, "precio": 1.5},
             "cantidad": 15, "precio_compra": 0.8, "vender_caja": {"unidades": 15, "precio": 18}}
        ]}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("Presentaciones"), "{}", e);

    // Con la caja sin abrir: la compra no entra y el producto nuevo se borra.
    let e = ingresar(
        &t,
        json!({"tipo_pago": "EFECTIVO_CAJA", "items": [
            {"nuevo": {"nombre": "Galleta nueva", "unidad_medida": "UNIDAD", "categoria_id": 900, "precio": 1}, "cantidad": 5, "precio_compra": 0.5}
        ]}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("caja"), "{}", e);

    assert_eq!(contar(&t, "SELECT COUNT(*) FROM productos").await, antes);
    assert_eq!(contar(&t, "SELECT COUNT(*) FROM compras").await, 0);
}

#[tokio::test]
async fn pagado_con_la_caja_y_perecible_con_fecha() {
    let t = base("caja", "").await;
    let _ = cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja");
    let vence = (chrono::Utc::now() + chrono::Duration::days(20)).format("%Y-%m-%d").to_string();
    ingresar(&t, json!({"tipo_pago": "EFECTIVO_CAJA", "items": [{"producto_id": 1002, "cantidad": 6, "precio_compra": 2, "fecha_vencimiento": vence}]}))
        .await
        .expect("ingreso con la caja");
    // S/ 12 salieron de la caja y el yogurt entró por su lote.
    assert_eq!(contar(&t, "SELECT CAST(gastos_total AS INTEGER) FROM cajas WHERE estado = 'ABIERTA'").await, 12);
    assert_eq!(contar(&t, "SELECT CAST(stock AS INTEGER) FROM productos WHERE id = 1002").await, 6);
    assert_eq!(contar(&t, "SELECT COUNT(*) FROM lotes_producto WHERE producto_id = 1002").await, 1);
}

#[tokio::test]
async fn sin_el_modulo_no_se_puede() {
    let t = base("apagado", "APAGADO").await;
    let e = ingresar(&t, json!({"tipo_pago": "EFECTIVO", "items": [{"producto_id": 1001, "cantidad": 1, "precio_compra": 1}]}))
        .await
        .unwrap_err();
    assert!(e.contains("Ingreso rápido"), "{}", e);
    assert_eq!(contar(&t, "SELECT COUNT(*) FROM compras").await, 0);
}
