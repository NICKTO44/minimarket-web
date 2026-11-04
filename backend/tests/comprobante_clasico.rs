//! Módulo COMPROBANTE_CLASICO: la venta leída para reimprimir trae quién la
//! hizo y el teléfono del cliente (solo para el comprobante impreso). Sin
//! cliente ni teléfono, esos campos no salen y todo queda como siempre.
//!
//!   cargo test --test comprobante_clasico

use std::sync::Arc;

use axum::extract::{Extension, Path};
use minimarket_backend::handlers::devoluciones::buscar_venta_para_devolucion;
use minimarket_backend::tenants::TenantDb;

async fn base() -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("comprobante_clasico_{}.db", std::process::id()));
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
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'luis', 'x', 'Luis Vendedor', 1);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (5, 'RUC', '20614876094', 'CASA DE AMORTIGUADORES E.I.R.L.', '983016291'),
            (6, 'DNI', '12345678', 'SIN TELEFONO', NULL);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Repuestos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES (1001, 'AMO-1', 'Amortiguador', 80, 10, 900);
         INSERT INTO ventas (id, folio, cliente_id, subtotal, total, metodo_pago, usuario_id) VALUES
            (1, 'V-1', 5, 80, 80, 'EFECTIVO', 1),
            (2, 'V-2', NULL, 80, 80, 'EFECTIVO', 1),
            (3, 'V-3', 6, 80, 80, 'EFECTIVO', 1);
         INSERT INTO detalles_venta (venta_id, producto_id, cantidad, precio_unitario, subtotal, total_linea) VALUES
            (1, 1001, 1, 80, 80, 80), (2, 1001, 1, 80, 80, 80), (3, 1001, 1, 80, 80, 80);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn leer(t: &Arc<TenantDb>, folio: &str) -> serde_json::Value {
    let venta = buscar_venta_para_devolucion(Extension(t.clone()), Path(folio.to_string())).await.expect("venta").0;
    serde_json::to_value(venta).unwrap()
}

#[tokio::test]
async fn trae_vendedor_y_telefono_solo_si_existen() {
    let t = base().await;

    let con_todo = leer(&t, "V-1").await;
    assert_eq!(con_todo["vendedor"], "Luis Vendedor");
    assert_eq!(con_todo["cliente_telefono"], "983016291");

    let sin_cliente = leer(&t, "V-2").await;
    assert_eq!(sin_cliente["vendedor"], "Luis Vendedor");
    assert!(sin_cliente.get("cliente_telefono").is_none());

    let sin_telefono = leer(&t, "V-3").await;
    assert!(sin_telefono.get("cliente_telefono").is_none());
    // Lo de siempre sigue igual.
    assert_eq!(sin_telefono["folio"], "V-3");
    assert_eq!(sin_telefono["productos"].as_array().unwrap().len(), 1);
}
