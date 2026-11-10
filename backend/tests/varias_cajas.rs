//! Módulo VARIAS_CAJAS: cada vendedor abre su caja (una por computadora) sin
//! cerrar la del otro, vende solo en la suya y el administrador ve todas.
//! Apagado, una sola caja por negocio como siempre.
//!
//!   cargo test --test varias_cajas

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::{cajas, gastos, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::caja::{AbrirCajaRequest, CerrarCajaRequest};
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("varias_cajas_{}_{}.db", nombre, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET rubro = 'LUBRICENTRO', modulos = ?1", libsql::params![modulos])
        .await
        .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES
            (1, 'admin', 'x', 'Dueño', 1),
            (2, 'leonardo', 'x', 'Leonardo', 2),
            (3, 'yenifer', 'x', 'Yenifer', 2);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Filtros');
         INSERT INTO categorias_gasto (id, nombre) VALUES (77, 'Pagos de prueba');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'FIL-WIX', 'Filtro de aceite', 25, 100, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn usuario(id: i64) -> Claims {
    let rol_id = if id == 1 { 1 } else { 2 };
    Claims { sub: id, username: format!("u{}", id), rol_id, nombre_completo: format!("Usuario {}", id), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir(t: &Arc<TenantDb>, id: i64, monto: f64) -> Result<i64, String> {
    cajas::abrir_caja(
        Extension(t.clone()),
        Extension(usuario(id)),
        Json(AbrirCajaRequest { usuario_id: id, numero_caja: None, monto_inicial: monto, observaciones: None }),
    )
    .await
    .map(|r| r.0.caja_id.unwrap())
    .map_err(|e| e.1)
}

async fn vender(t: &Arc<TenantDb>, id: i64, cantidad: f64) -> Result<(), String> {
    let total = 25.0 * cantidad;
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 1001, "nombre": "Filtro de aceite", "precio": 25, "cantidad": cantidad, "descuentoMonto": 0}],
        "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0, "usuario_id": id, "cliente_id": null
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(usuario(id)), Json(venta)).await.map(|_| ()).map_err(|e| e.1)
}

/// (total_ventas, ventas_efectivo, gastos_total) de una caja.
async fn totales(t: &Arc<TenantDb>, caja: i64) -> (f64, f64, f64) {
    let conn = t.0.connect().unwrap();
    let mut f = conn
        .query(
            "SELECT CAST(total_ventas AS REAL), CAST(ventas_efectivo AS REAL), CAST(gastos_total AS REAL) FROM cajas WHERE id = ?1",
            libsql::params![caja],
        )
        .await
        .unwrap();
    let f = f.next().await.unwrap().unwrap();
    (f.get(0).unwrap(), f.get(1).unwrap(), f.get(2).unwrap())
}

#[tokio::test]
async fn dos_vendedores_cada_uno_con_su_caja() {
    let t = base("dos", "SERVICIOS,VARIAS_CAJAS").await;

    // Cada uno abre la suya sin cerrar la del otro.
    let caja_leo = abrir(&t, 2, 50.0).await.expect("caja de Leonardo");
    let caja_yen = abrir(&t, 3, 80.0).await.expect("caja de Yenifer");
    assert_ne!(caja_leo, caja_yen);
    // Pero nadie abre dos.
    let e = abrir(&t, 2, 10.0).await.unwrap_err();
    assert!(e.contains("Ya tienes una caja abierta"), "{}", e);

    // Cada venta entra en la caja de quien cobró.
    vender(&t, 2, 2.0).await.expect("venta de Leonardo");
    vender(&t, 3, 1.0).await.expect("venta de Yenifer");
    vender(&t, 3, 1.0).await.expect("otra de Yenifer");
    assert_eq!(totales(&t, caja_leo).await, (50.0, 50.0, 0.0));
    assert_eq!(totales(&t, caja_yen).await, (50.0, 50.0, 0.0));

    // Cada uno ve solo su caja.
    let ve = |id| {
        let t = t.clone();
        async move { cajas::obtener_caja_abierta(Extension(t), Extension(usuario(id))).await.0.map(|c| c.id) }
    };
    assert_eq!(ve(2).await, Some(caja_leo));
    assert_eq!(ve(3).await, Some(caja_yen));
    // El dueño no abrió una: no tiene caja y no puede vender en la de otro.
    assert_eq!(ve(1).await, None);
    let e = vender(&t, 1, 1.0).await.unwrap_err();
    assert!(e.contains("Abre tu caja"), "{}", e);

    // El gasto de caja sale de la caja de quien lo registra.
    let gasto: gastos::NuevoGasto = serde_json::from_value(json!({
        "categoria_id": 77, "descripcion": "Almuerzo", "monto": 10, "metodo_pago": "EFECTIVO_CAJA",
        "fecha": "2026-01-01", "desde_caja": true
    }))
    .unwrap();
    let _ = gastos::registrar(Extension(t.clone()), Extension(usuario(3)), Json(gasto)).await.expect("gasto de Yenifer");
    assert_eq!(totales(&t, caja_yen).await.2, 10.0);
    assert_eq!(totales(&t, caja_leo).await.2, 0.0);
    // Y sus movimientos no se mezclan.
    assert_eq!(cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(usuario(3))).await.unwrap().0.len(), 1);
    assert!(cajas::movimientos_caja_abierta(Extension(t.clone()), Extension(usuario(2))).await.unwrap().0.is_empty());

    // El administrador ve las dos; un vendedor no puede listarlas.
    let todas = cajas::listar_cajas_abiertas(Extension(t.clone()), Extension(usuario(1))).await.expect("lista").0;
    assert_eq!(todas.iter().map(|c| c.id).collect::<Vec<_>>(), vec![caja_leo, caja_yen]);
    assert!(cajas::listar_cajas_abiertas(Extension(t.clone()), Extension(usuario(2))).await.is_err());

    // Un vendedor no cierra la caja del otro; el administrador sí.
    let cerrar = |quien: i64, caja: i64| {
        let t = t.clone();
        async move {
            cajas::cerrar_caja(
                Extension(t),
                Extension(usuario(quien)),
                Json(CerrarCajaRequest {
                    caja_id: caja,
                    usuario_id: quien,
                    usuario_rol_id: 0,
                    monto_contado: 100.0,
                    observaciones: None,
                    justificacion_diferencia: None,
                }),
            )
            .await
        }
    };
    assert!(cerrar(3, caja_leo).await.is_err());
    let _ = cerrar(1, caja_leo).await.expect("el admin cierra la de Leonardo");
    // La de Yenifer sigue abierta y vendiendo.
    vender(&t, 3, 1.0).await.expect("Yenifer sigue vendiendo");
    assert_eq!(totales(&t, caja_yen).await.0, 75.0);
    // Leonardo abre una nueva aunque la de Yenifer siga abierta.
    abrir(&t, 2, 0.0).await.expect("nueva caja de Leonardo");
}

#[tokio::test]
async fn sin_el_modulo_una_sola_caja_como_siempre() {
    let t = base("una", "SERVICIOS").await;
    let caja = abrir(&t, 2, 50.0).await.expect("caja");
    let e = abrir(&t, 3, 10.0).await.unwrap_err();
    assert!(e.contains("Ya hay una caja abierta"), "{}", e);
    // Cualquiera vende en la caja del negocio y la ve.
    vender(&t, 3, 1.0).await.expect("venta en la caja del negocio");
    vender(&t, 1, 1.0).await.expect("el admin también");
    assert_eq!(totales(&t, caja).await.0, 50.0);
    let vista = cajas::obtener_caja_abierta(Extension(t.clone()), Extension(usuario(3))).await.0;
    assert_eq!(vista.map(|c| c.id), Some(caja));
}
