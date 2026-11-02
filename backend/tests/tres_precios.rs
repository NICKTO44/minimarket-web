//! Módulo "3 precios de venta" (TRES_PRECIOS) contra una base local
//! (schema.sql + todas las migraciones) y los handlers de verdad: guardar los
//! precios, vender y cotizar a Precio 2 o 3, importar desde Excel, y que un
//! negocio sin el módulo siga exactamente igual.
//!
//!   cargo test --test tres_precios

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{cotizaciones, importacion, precios_venta, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

const MODULOS_LUBRICENTRO: &str =
    "SERVICIOS,MEDIDAS,COTIZACIONES,CREDITO,PRESENTACIONES,PRECIO_MAYOR,FICHA_REPUESTO,COMPATIBILIDAD,VEHICULOS,ORDENES_SERVICIO,PAQUETES,TRES_PRECIOS";

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("tres_precios_{}_{}.db", nombre, std::process::id()));
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
    // La migración 0056 es idempotente: aplicarla otra vez no rompe nada.
    conn.execute_batch(include_str!("../migraciones/0056_tres_precios.sql")).await.expect("0056 otra vez");
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'REPUESTOS PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Suspensión');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (2001, 'MSR-31002L', 'Masaru trapecio delantero inferior LH', 100, 10, 900),
            (2002, 'MSR-31003L', 'Masaru trapecio Chevy Taxi LH', 70, 5, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

/// Por serde: así el test sigue compilando si Claims gana campos con valor por defecto.
fn claims() -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": 1, "nombre_completo": "Admin de prueba", "tienda_id": 1, "exp": usize::MAX
    }))
    .unwrap()
}

async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"usuario_id": 1, "numero_caja": null, "monto_inicial": 100.0, "observaciones": null})).unwrap()),
    )
    .await
    .expect("caja abierta");
}

async fn vender(t: &Arc<TenantDb>, id: i64, precio: f64, cantidad: f64) -> Result<i64, (StatusCode, String)> {
    let total = precio * cantidad;
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": id, "nombre": format!("Producto {}", id), "precio": precio, "cantidad": cantidad, "descuentoMonto": 0}],
        "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0.0, "usuario_id": 1, "cliente_id": null
    }))
    .expect("venta");
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match filas.next().await.unwrap().unwrap_or_else(|| panic!("sin filas: {}", sql)).get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: no es número: {:?}", sql, otro),
    }
}

fn precios(dos: Option<f64>, tres: Option<f64>) -> Json<precios_venta::PreciosVenta> {
    Json(precios_venta::PreciosVenta { producto_id: None, precio_2: dos, precio_3: tres })
}

#[tokio::test]
async fn guardar_y_vender_a_cada_precio() {
    let t = base("vender", "LUBRICENTRO", MODULOS_LUBRICENTRO).await;
    let _ = precios_venta::guardar(Extension(t.clone()), Path(2001), precios(Some(120.0), Some(80.0))).await.unwrap();
    let Json(todos) = precios_venta::todos(Extension(t.clone())).await.unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!((todos[0].producto_id, todos[0].precio_2, todos[0].precio_3), (Some(2001), Some(120.0), Some(80.0)));

    abrir_caja(&t).await;
    // Precio 1 (el del producto), 2 y 3: los tres valen y el stock baja igual.
    for precio in [100.0, 120.0, 80.0] {
        let venta = vender(&t, 2001, precio, 1.0).await.unwrap();
        let unitario = real(&t, &format!("SELECT CAST(precio_unitario AS REAL) FROM detalles_venta WHERE venta_id = {}", venta)).await;
        assert_eq!(unitario, precio);
    }
    assert_eq!(real(&t, "SELECT CAST(stock AS REAL) FROM productos WHERE id = 2001").await, 7.0);
    // Cualquier otro precio sigue rechazado (sin el módulo "Cambiar precio al vender").
    let e = vender(&t, 2001, 95.0, 1.0).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
    // El Precio 2 de un producto no sirve para otro.
    let e = vender(&t, 2002, 120.0, 1.0).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);

    // Quitar los precios (vacíos o 0): vuelve a valer solo el Precio 1.
    let _ = precios_venta::guardar(Extension(t.clone()), Path(2001), precios(Some(0.0), None)).await.unwrap();
    let Json(todos) = precios_venta::todos(Extension(t.clone())).await.unwrap();
    assert!(todos.is_empty());
    assert_eq!(vender(&t, 2001, 120.0, 1.0).await.unwrap_err().0, StatusCode::CONFLICT);

    // Un precio negativo no se guarda; un producto que no existe, tampoco.
    let e = precios_venta::guardar(Extension(t.clone()), Path(2001), precios(Some(-5.0), None)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    let e = precios_venta::guardar(Extension(t.clone()), Path(9999), precios(Some(5.0), None)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cotizar_a_precio_2() {
    let t = base("cotizar", "LUBRICENTRO", MODULOS_LUBRICENTRO).await;
    let _ = precios_venta::guardar(Extension(t.clone()), Path(2001), precios(Some(120.0), None)).await.unwrap();
    let cotizar = |precio: f64| {
        serde_json::from_value::<cotizaciones::NuevaCotizacion>(json!({"items": [{"id": 2001, "cantidad": 2, "precio": precio}]})).unwrap()
    };
    let _ = cotizaciones::crear(Extension(t.clone()), Extension(claims()), Json(cotizar(120.0))).await.unwrap();
    let e = cotizaciones::crear(Extension(t.clone()), Extension(claims()), Json(cotizar(130.0))).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn importar_precio_2_y_3() {
    let t = base("importar", "LUBRICENTRO", MODULOS_LUBRICENTRO).await;
    let filas = json!({"filas": [
        {"fila": 2, "codigo": "MSR-31002R", "nombre": "Masaru trapecio RH", "precio": 100, "stock": 4,
         "precios": {"precio_2": 120, "precio_3": 90}},
        {"fila": 3, "codigo": "MSR-31003L", "nombre": "Masaru trapecio Chevy Taxi LH", "precio": 70,
         "precios": {"precio_2": 80}},
        {"fila": 4, "codigo": "MAL-1", "nombre": "Precio 2 mal escrito", "precio": 10, "precios": {"precio_2": -3}},
        {"fila": 5, "codigo": "SIN-1", "nombre": "Sin precios extra", "precio": 15, "precios": {}}
    ], "existentes": "ACTUALIZAR"});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert!(r.guardado);
    assert_eq!((r.creados, r.actualizados, r.errores), (2, 1, 1));
    let nuevo = real(&t, "SELECT id FROM productos WHERE codigo = 'MSR-31002R'").await as i64;
    assert_eq!(real(&t, &format!("SELECT precio_2 FROM producto_precios_venta WHERE producto_id = {}", nuevo)).await, 120.0);
    assert_eq!(real(&t, &format!("SELECT precio_3 FROM producto_precios_venta WHERE producto_id = {}", nuevo)).await, 90.0);
    assert_eq!(real(&t, "SELECT precio_2 FROM producto_precios_venta WHERE producto_id = 2002").await, 80.0);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM producto_precios_venta").await, 2.0);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos WHERE codigo = 'MAL-1'").await, 0.0);
}

#[tokio::test]
async fn otro_rubro_sigue_igual() {
    // Una tienda de ropa como VERANE: sin el módulo.
    let t = base("ropa", "ROPA", "CREDITO,VARIANTES,CAMBIOS").await;
    let e = precios_venta::todos(Extension(t.clone())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = precios_venta::guardar(Extension(t.clone()), Path(2001), precios(Some(120.0), None)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    // Aunque hubiera una fila en la tabla, sin el módulo ese precio no vale.
    t.0.connect()
        .unwrap()
        .execute("INSERT INTO producto_precios_venta (producto_id, precio_2) VALUES (2001, 120)", ())
        .await
        .unwrap();
    abrir_caja(&t).await;
    assert_eq!(vender(&t, 2001, 120.0, 1.0).await.unwrap_err().0, StatusCode::CONFLICT);
    // Un archivo con esas columnas se importa como siempre: se ignoran.
    let filas = json!({"filas": [{"fila": 2, "codigo": "POL-1", "nombre": "Polo", "precio": 30, "precios": {"precio_2": 25}}]});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert_eq!((r.creados, r.errores), (1, 0));
    assert_eq!(real(&t, "SELECT COUNT(*) FROM producto_precios_venta").await, 1.0);
    // La venta de siempre, igual.
    let venta = vender(&t, 2001, 100.0, 2.0).await.unwrap();
    assert_eq!(real(&t, &format!("SELECT CAST(total AS REAL) FROM ventas WHERE id = {}", venta)).await, 200.0);
}
