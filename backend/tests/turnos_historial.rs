//! Cajas por turno (farmacia): cada cajero abre y cierra su caja, uno después
//! del otro. El Historial de caja trae de cada turno la hora de cierre, lo
//! cobrado en efectivo, Yape y Plin, y la diferencia del cierre.
//!
//!   cargo test --test turnos_historial

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    Json,
};
use minimarket_backend::handlers::{cajas, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::caja::{AbrirCajaRequest, CerrarCajaRequest};
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base() -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("turnos_historial_{}.db", std::process::id()));
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
            (2, 'cattya', 'x', 'Cattya', 2),
            (3, 'rocio', 'x', 'Rocío', 2);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Medicamentos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'PARA-500', 'Paracetamol 500mg', 10, 500, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn usuario(id: i64) -> Claims {
    let rol_id = if id == 1 { 1 } else { 2 };
    Claims { sub: id, username: format!("u{}", id), rol_id, nombre_completo: format!("Usuario {}", id), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir(t: &Arc<TenantDb>, id: i64, monto: f64) -> i64 {
    cajas::abrir_caja(
        Extension(t.clone()),
        Extension(usuario(id)),
        Json(AbrirCajaRequest { usuario_id: id, numero_caja: None, monto_inicial: monto, observaciones: None }),
    )
    .await
    .map(|r| r.0.caja_id.unwrap())
    .map_err(|e| e.1)
    .expect("abrir caja")
}

async fn vender(t: &Arc<TenantDb>, id: i64, cantidad: f64, metodo: &str) {
    let total = 10.0 * cantidad;
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 1001, "nombre": "Paracetamol 500mg", "precio": 10, "cantidad": cantidad, "descuentoMonto": 0}],
        "total": total, "metodo_pago": metodo, "monto_recibido": total, "cambio": 0, "usuario_id": id, "cliente_id": null
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(usuario(id)), Json(venta))
        .await
        .map_err(|e| e.1)
        .expect("venta");
}

async fn cerrar(t: &Arc<TenantDb>, id: i64, caja: i64, contado: f64) {
    let _ = cajas::cerrar_caja(
        Extension(t.clone()),
        Extension(usuario(id)),
        Json(CerrarCajaRequest {
            caja_id: caja,
            usuario_id: id,
            usuario_rol_id: 0,
            monto_contado: contado,
            observaciones: None,
            justificacion_diferencia: Some("Prueba".into()),
        }),
    )
    .await
    .map_err(|e| e.1)
    .expect("cerrar caja");
}

#[tokio::test]
async fn historial_por_turno_con_efectivo_yape_y_plin() {
    let t = base().await;

    // Turno de Cattya: efectivo, Yape y Plin; al cerrar cuenta S/ 1 menos.
    let caja_a = abrir(&t, 2, 50.0).await;
    vender(&t, 2, 3.0, "EFECTIVO").await; // 30
    vender(&t, 2, 2.0, "YAPE").await; // 20
    vender(&t, 2, 1.0, "PLIN").await; // 10
    cerrar(&t, 2, caja_a, 50.0 + 30.0 - 1.0).await;

    // Turno de Rocío: cuadra exacto.
    let caja_b = abrir(&t, 3, 40.0).await;
    vender(&t, 3, 4.0, "EFECTIVO").await; // 40
    vender(&t, 3, 1.0, "YAPE").await; // 10
    cerrar(&t, 3, caja_b, 40.0 + 40.0).await;

    // Cattya vuelve y deja su turno abierto.
    let caja_c = abrir(&t, 2, 20.0).await;
    vender(&t, 2, 1.0, "PLIN").await;

    let hoy = (chrono::Utc::now() - chrono::Duration::hours(5)).format("%Y-%m-%d").to_string();
    let rango = serde_json::from_value(json!({"fecha_inicio": hoy, "fecha_fin": hoy})).unwrap();
    let lista = cajas::listar_cajas(Extension(t.clone()), Query(rango)).await.expect("historial").0;
    assert_eq!(lista.len(), 3);

    let de = |id: i64| lista.iter().find(|c| c.id == id).expect("caja en historial");

    let a = de(caja_a);
    assert_eq!(a.usuario_nombre, "Cattya");
    assert!(a.fecha_cierre.is_some(), "turno cerrado trae su hora de cierre");
    assert_eq!(a.estado, "CERRADA");
    assert_eq!(a.total_ventas, 60.0);
    assert_eq!(a.ventas_efectivo, 30.0);
    assert_eq!(a.ventas_yape, 20.0);
    assert_eq!(a.ventas_plin, 10.0);
    assert_eq!(a.numero_transacciones, 3);
    assert_eq!(a.diferencia, Some(-1.0));

    let b = de(caja_b);
    assert_eq!(b.usuario_nombre, "Rocío");
    assert_eq!(b.total_ventas, 50.0);
    assert_eq!(b.ventas_efectivo, 40.0);
    assert_eq!(b.ventas_yape, 10.0);
    assert_eq!(b.ventas_plin, 0.0);
    assert_eq!(b.diferencia, Some(0.0));

    let c = de(caja_c);
    assert_eq!(c.usuario_nombre, "Cattya");
    assert!(c.fecha_cierre.is_none(), "turno abierto sin hora de cierre");
    assert_eq!(c.diferencia, None);
    assert_eq!(c.ventas_plin, 10.0);

    // Para revisar la pantalla con estos mismos datos.
    if let Ok(ruta) = std::env::var("HISTORIAL_JSON") {
        std::fs::write(ruta, serde_json::to_string(&lista).unwrap()).unwrap();
    }
}
