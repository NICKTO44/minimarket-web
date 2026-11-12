//! Compras con el comprobante del proveedor (factura, boleta o nota de venta)
//! y la sección "Compras del período" de Reportes (solo el administrador).
//!
//!   cargo test --test compras_comprobante

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    Json,
};
use minimarket_backend::handlers::{proveedores, reportes};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base() -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("compras_comprobante_{}.db", std::process::id()));
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
        "UPDATE configuracion_tienda SET rubro = 'FARMACIA';
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES
            (1, 'admin', 'x', 'Dueño', 1),
            (2, 'cattya', 'x', 'Cattya', 2);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Medicamentos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'PARA-500', 'Paracetamol 500mg', 10, 0, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn usuario(id: i64) -> Claims {
    let rol_id = if id == 1 { 1 } else { 2 };
    Claims { sub: id, username: format!("u{}", id), rol_id, nombre_completo: format!("Usuario {}", id), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn comprar(t: &Arc<TenantDb>, proveedor: i64, extra: serde_json::Value) -> Result<i64, String> {
    let mut cuerpo = json!({
        "proveedor_id": proveedor, "fecha_compra": "2026-11-12",
        "items": [{"producto_id": 1001, "cantidad": 10, "precio_compra": 4}],
        "tipo_pago": "EFECTIVO", "usuario_id": 1
    });
    cuerpo.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let pedido = serde_json::from_value(cuerpo).unwrap();
    proveedores::crear_compra(Extension(t.clone()), Extension(usuario(1)), Json(pedido))
        .await
        .map(|r| r.0.compra_id.unwrap())
        .map_err(|e| e.1)
}

#[tokio::test]
async fn compra_con_comprobante_y_reporte_de_compras() {
    let t = base().await;

    let nuevo = serde_json::from_value(json!({
        "nombre": "DISTRIBUIDORA FARMA S.A.C.", "direccion": "AV. CUSCO 123", "tipo_documento": "RUC",
        "numero_documento": "20123456789"
    }))
    .unwrap();
    let prov = proveedores::agregar_proveedor(Extension(t.clone()), Json(nuevo)).await.expect("proveedor").0;
    assert_eq!(prov.direccion.as_deref(), Some("AV. CUSCO 123"));

    // Factura con su serie y número (se guarda en mayúsculas, sin espacios).
    let con_factura = comprar(&t, prov.id, json!({"tipo_comprobante": "factura", "factura_numero": " f001-00001234 "}))
        .await
        .expect("compra con factura");
    // Nota de venta sin número, y una compra sin comprobante (como antes).
    let con_nota = comprar(&t, prov.id, json!({"tipo_comprobante": "NOTA_VENTA"})).await.expect("compra con nota");
    let sin = comprar(&t, prov.id, json!({})).await.expect("compra sin comprobante");
    // Tipo desconocido: no entra.
    let e = comprar(&t, prov.id, json!({"tipo_comprobante": "RECIBO"})).await.unwrap_err();
    assert!(e.contains("comprobante no válido"), "{}", e);

    let lista = proveedores::listar_compras(Extension(t.clone())).await.expect("lista").0;
    let de = |id: i64| lista.iter().find(|c| c.id == id).unwrap();
    assert_eq!(de(con_factura).tipo_comprobante.as_deref(), Some("FACTURA"));
    assert_eq!(de(con_factura).factura_numero.as_deref(), Some("F001-00001234"));
    assert_eq!(de(con_nota).tipo_comprobante.as_deref(), Some("NOTA_VENTA"));
    assert_eq!(de(con_nota).factura_numero, None);
    assert_eq!(de(sin).tipo_comprobante, None);

    // Reportes: el administrador ve las compras del período, con su comprobante.
    let rango = || serde_json::from_value(json!({"fecha_inicio": "2026-11-01", "fecha_fin": "2026-11-30"})).unwrap();
    let compras = reportes::compras_por_rango(Extension(t.clone()), Extension(usuario(1)), Query(rango()))
        .await
        .expect("reporte de compras")
        .0;
    assert_eq!(compras.len(), 3);
    let f = compras.iter().find(|c| c.id == con_factura).unwrap();
    assert_eq!(f.proveedor, "DISTRIBUIDORA FARMA S.A.C.");
    assert_eq!(f.proveedor_documento.as_deref(), Some("20123456789"));
    assert_eq!(f.factura_numero.as_deref(), Some("F001-00001234"));
    assert_eq!(f.total, 40.0);
    assert_eq!(f.registrado_por, "Dueño");

    // Fuera del período no sale nada.
    let otro_mes = serde_json::from_value(json!({"fecha_inicio": "2026-10-01", "fecha_fin": "2026-10-31"})).unwrap();
    let vacio = reportes::compras_por_rango(Extension(t.clone()), Extension(usuario(1)), Query(otro_mes)).await.unwrap().0;
    assert!(vacio.is_empty());

    // Una cajera no lo puede ver.
    let e = reportes::compras_por_rango(Extension(t.clone()), Extension(usuario(2)), Query(rango())).await.err().unwrap();
    assert_eq!(e.0, axum::http::StatusCode::FORBIDDEN);
}
