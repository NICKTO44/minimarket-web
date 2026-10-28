//! Boleta o factura de una venta cobrada como nota de venta
//! (POST /comprobantes/desde-nota), contra una base local con schema.sql y
//! todas las migraciones. Revisa las reglas antes de emitir; la emisión en sí
//! es la misma del punto de venta (aquí no hay proveedor configurado, así que
//! se detiene en "falta configurar", después de dejar el cliente en la venta).
//!
//!   cargo test --test desde_nota

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, Json};
use minimarket_backend::handlers::desde_nota;
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base() -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("desde_nota_{}.db", std::process::id()));
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
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Platos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock) VALUES (1, 'CH', 'Arroz chaufa', 30, 0, 900, 0);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social) VALUES
            (1, 'DNI', '12345678', 'Ana Huamán'), (2, 'RUC', '20123456789', 'EMPRESA SAC');
         INSERT INTO ventas (id, folio, cliente_id, subtotal, descuento, total, metodo_pago, usuario_id, estado) VALUES
            (10, 'V-1', NULL, 30, 0, 30, 'EFECTIVO', 1, 'COMPLETADA'),
            (11, 'V-2', 1, 30, 0, 30, 'EFECTIVO', 1, 'COMPLETADA'),
            (12, 'V-3', NULL, 30, 0, 30, 'EFECTIVO', 1, 'CANCELADA'),
            (13, 'V-4', NULL, 30, 0, 30, 'EFECTIVO', 1, 'COMPLETADA'),
            (14, 'V-5', NULL, 30, 0, 30, 'EFECTIVO', 1, 'COMPLETADA'),
            (15, 'V-6', NULL, 30, 0, 30, 'EFECTIVO', 1, 'COMPLETADA');
         INSERT INTO detalles_venta (id, venta_id, producto_id, cantidad, precio_unitario, subtotal, total_linea) VALUES
            (100, 10, 1, 1, 30, 30, 30), (101, 11, 1, 1, 30, 30, 30), (102, 12, 1, 1, 30, 30, 30),
            (103, 13, 1, 1, 30, 30, 30), (104, 14, 1, 1, 30, 30, 30), (105, 15, 1, 1, 30, 30, 30);
         INSERT INTO comprobantes_electronicos (venta_id, tipo, serie, numero, estado) VALUES (13, 'BOLETA', 'B001', 5, 'ACEPTADO');
         INSERT INTO comprobantes_electronicos (venta_id, tipo, serie, numero, estado) VALUES (15, 'BOLETA', 'B001', 6, 'RECHAZADO');
         INSERT INTO devoluciones (id, venta_original_id, folio_devolucion, usuario_id, monto_reembolsado, metodo_reembolso, motivo, estado)
            VALUES (1, 14, 'D-1', 1, 30, 'EFECTIVO', 'x', 'PROCESADA');
         INSERT INTO detalles_devolucion (devolucion_id, producto_id, detalle_venta_id, venta_id, cantidad_devuelta, precio_unitario, subtotal, condicion)
            VALUES (1, 1, 104, 14, 1, 30, 30, 'DEFECTUOSO');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

#[tokio::test]
async fn reglas_para_emitir_desde_una_nota_de_venta() {
    let t = base().await;
    let conn = t.0.connect().unwrap();
    let revisar = |venta: i64, tipo: &'static str, cliente: Option<i64>| {
        let conn = conn.clone();
        async move { desde_nota::revisar(&conn, venta, tipo, cliente).await }
    };

    // Boleta sin cliente: se puede.
    let r = revisar(10, "BOLETA", None).await.unwrap();
    assert_eq!(r.cliente_id, None);
    // Boleta: toma el cliente de la venta.
    let r = revisar(11, "BOLETA", None).await.unwrap();
    assert_eq!((r.documento.as_deref(), r.nombre.as_deref()), (Some("12345678"), Some("Ana Huamán")));
    // Factura: necesita RUC (el de la venta es un DNI).
    assert_eq!(revisar(11, "FACTURA", None).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(revisar(10, "FACTURA", None).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let r = revisar(11, "FACTURA", Some(2)).await.unwrap();
    assert_eq!(r.documento.as_deref(), Some("20123456789"));
    // Otro tipo, venta anulada, inexistente, ya con comprobante, con devolución.
    assert_eq!(revisar(10, "NOTA", None).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(revisar(12, "BOLETA", None).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(revisar(99, "BOLETA", None).await.err().map(|e| e.0), Some(StatusCode::NOT_FOUND));
    assert_eq!(revisar(13, "BOLETA", None).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(revisar(14, "BOLETA", None).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // Uno rechazado no cuenta: se puede volver a intentar.
    assert!(revisar(15, "BOLETA", None).await.is_ok());
    // Cliente que no existe.
    assert_eq!(revisar(10, "BOLETA", Some(77)).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // La emisión: sin proveedor configurado se detiene, pero el cliente ya quedó en la venta.
    let r = desde_nota::emitir(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"venta_id": 10, "tipo": "factura", "cliente_id": 2})).unwrap()),
    )
    .await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let mut f = conn.query("SELECT cliente_id FROM ventas WHERE id = 10", ()).await.unwrap();
    assert_eq!(f.next().await.unwrap().unwrap().get::<i64>(0).unwrap(), 2);
}
