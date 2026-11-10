//! Forma de pago en el gasto y el retiro de la Caja (migración 0063), contra
//! una base local (schema.sql + todas las migraciones) y los handlers de verdad.
//!
//! Pedido de un cliente: si paga a su trabajador por Yape desde la Caja,
//! tiene que quedar que fue por Yape, y el efectivo esperado no cambia.
//!
//!   cargo test --test gasto_metodo_caja

use std::sync::Arc;

use axum::{extract::{Extension, Path}, Json};
use minimarket_backend::handlers::{cajas, gastos};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("gasto_metodo_{}_{}.db", nombre, std::process::id()));
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
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES
            (1, 'admin', 'x', 'Admin de prueba', 1),
            (2, 'cajero', 'x', 'Cajero de prueba', 2);
         INSERT INTO categorias_gasto (id, nombre) VALUES (77, 'Pagos de prueba');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "admin".into(), rol_id: 1, nombre_completo: "Admin".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

fn cajero() -> Claims {
    Claims { sub: 2, username: "cajero".into(), rol_id: 2, nombre_completo: "Cajero".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir_caja(t: &Arc<TenantDb>) -> i64 {
    let _ = cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    let conn = t.0.connect().unwrap();
    let mut f = conn.query("SELECT id FROM cajas WHERE estado = 'ABIERTA'", ()).await.unwrap();
    f.next().await.unwrap().unwrap().get(0).unwrap()
}

/// (retiros_total, gastos_total) de la caja.
async fn totales(t: &Arc<TenantDb>, caja: i64) -> (f64, f64) {
    let conn = t.0.connect().unwrap();
    let mut f = conn
        .query("SELECT CAST(retiros_total AS REAL), CAST(gastos_total AS REAL) FROM cajas WHERE id = ?1", libsql::params![caja])
        .await
        .unwrap();
    let f = f.next().await.unwrap().unwrap();
    (f.get(0).unwrap(), f.get(1).unwrap())
}

fn gasto(metodo: &str, monto: f64) -> gastos::NuevoGasto {
    serde_json::from_value(json!({
        "categoria_id": 77, "descripcion": "Pago a Juan", "monto": monto,
        "metodo_pago": metodo, "fecha": "2026-01-01", "desde_caja": true
    }))
    .unwrap()
}

#[tokio::test]
async fn cajero_paga_por_yape_desde_la_caja() {
    let t = base("yape").await;
    let caja = abrir_caja(&t).await;

    // Más que el efectivo de la caja (100): por Yape igual se acepta.
    let r = gastos::registrar(Extension(t.clone()), Extension(cajero()), Json(gasto("YAPE", 150.0))).await.expect("gasto por Yape");
    assert_eq!(r.0.caja_id, None);
    assert!(r.0.mensaje.contains("Yape"), "{}", r.0.mensaje);
    // De hoy, aunque mande otra fecha.
    assert_ne!(r.0.fecha, "2026-01-01");
    // No toca el cuadre.
    assert_eq!(totales(&t, caja).await, (0.0, 0.0));

    // Aparece en el turno con su forma de pago.
    let movs = cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(cajero())).await.expect("movimientos").0;
    assert_eq!(movs.len(), 1);
    assert_eq!(movs[0].tipo, "GASTO");
    assert_eq!(movs[0].metodo_pago.as_deref(), Some("YAPE"));
    assert_eq!(movs[0].gasto_id, Some(r.0.id));

    // En efectivo de la caja: como siempre, descuenta.
    let _ = gastos::registrar(Extension(t.clone()), Extension(cajero()), Json(gasto("EFECTIVO_CAJA", 30.0))).await.expect("gasto de caja");
    assert_eq!(totales(&t, caja).await, (0.0, 30.0));
    let movs = cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(cajero())).await.unwrap().0;
    assert_eq!(movs[0].metodo_pago, None);

    // Al anular el de Yape se quita del turno y el cuadre no cambia.
    let _ = gastos::anular(Extension(t.clone()), Extension(admin()), Path(r.0.id), Json(gastos::Anulacion { motivo: "error".into() }))
        .await
        .expect("anular");
    let movs = cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(cajero())).await.unwrap().0;
    assert_eq!(movs.len(), 1);
    assert_eq!(totales(&t, caja).await, (0.0, 30.0));
}

#[tokio::test]
async fn cajero_sin_caja_abierta_no_registra() {
    let t = base("sin_caja").await;
    let error = gastos::registrar(Extension(t.clone()), Extension(cajero()), Json(gasto("YAPE", 10.0))).await.err().unwrap();
    assert!(error.1.contains("caja abierta"), "{}", error.1);
}

#[tokio::test]
async fn admin_desde_gastos_no_va_al_turno() {
    let t = base("admin").await;
    abrir_caja(&t).await;
    let mut d = gasto("TRANSFERENCIA", 900.0);
    d.desde_caja = false;
    let r = gastos::registrar(Extension(t.clone()), Extension(admin()), Json(d)).await.expect("gasto");
    assert_eq!(r.0.fecha, "2026-01-01");
    assert!(cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(cajero())).await.unwrap().0.is_empty());
}

#[tokio::test]
async fn retiro_por_yape_no_toca_el_efectivo() {
    let t = base("retiro").await;
    let caja = abrir_caja(&t).await;
    let mov = |metodo: Option<&str>, monto: f64| {
        serde_json::from_value::<minimarket_backend::models::caja::MovimientoCajaRequest>(json!({
            "caja_id": caja, "tipo": "RETIRO", "monto": monto, "motivo": "Retiro del dueño",
            "usuario_id": 2, "metodo_pago": metodo
        }))
        .unwrap()
    };
    // Por Yape, aunque supere el efectivo de la caja.
    let _ = cajas::registrar_movimiento(Extension(t.clone()), Extension(cajero()), Json(mov(Some("YAPE"), 500.0))).await.expect("retiro Yape");
    assert_eq!(totales(&t, caja).await, (0.0, 0.0));
    // En efectivo: como siempre, y no más de lo que hay.
    assert!(cajas::registrar_movimiento(Extension(t.clone()), Extension(cajero()), Json(mov(None, 500.0))).await.is_err());
    let _ = cajas::registrar_movimiento(Extension(t.clone()), Extension(cajero()), Json(mov(Some("EFECTIVO_CAJA"), 40.0))).await.expect("retiro");
    assert_eq!(totales(&t, caja).await, (40.0, 0.0));

    let movs = cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(cajero())).await.unwrap().0;
    assert_eq!(movs.iter().map(|m| m.metodo_pago.clone()).collect::<Vec<_>>(), vec![None, Some("YAPE".to_string())]);
    // Forma de pago que no corresponde.
    assert!(cajas::registrar_movimiento(Extension(t.clone()), Extension(cajero()), Json(mov(Some("CHEQUE"), 5.0))).await.is_err());
}
