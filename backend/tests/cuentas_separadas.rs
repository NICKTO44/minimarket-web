//! Cuentas separadas en una mesa: un solo pedido y cada uno paga lo suyo
//! (POST /ventas con pedido_id + pedido_items), contra una base local con
//! schema.sql y todas las migraciones. Revisa:
//!   - se cobra una parte (una línea entera y 1 de 2 unidades de otra): la
//!     mesa sigue abierta con el resto y lo cobrado queda en su propio pedido
//!     COBRADO, con la venta, la mesa y el estado de cocina de siempre;
//!   - la última cuenta se cobra como siempre y libera la mesa;
//!   - elegir de más, unidades sueltas, montos que no cuadran o un producto
//!     que ya se cobró se rechazan sin tocar nada;
//!   - sin pedido_items, el cobro de siempre.
//!
//!   cargo test --test cuentas_separadas

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{mesas, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::mesa::PedidoDetalle;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("cuentas_{}_{}.db", nombre, std::process::id()));
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
        "UPDATE configuracion_tienda SET rubro = 'RESTAURANTE', modo_negocio = 'RESTAURANTE';
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin', 1);
         INSERT INTO mesas (id, nombre) VALUES (1, 'Mesa 1');
         INSERT INTO categorias (id, nombre) VALUES (900, 'Platos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock) VALUES
            (1, 'LOMO', 'Lomo saltado', 25, 0, 900, 0),
            (2, 'AJI', 'Ají de gallina', 20, 0, 900, 0),
            (3, 'CHI', 'Chicha morada', 5, 0, 900, 0);",
    )
    .await
    .expect("datos de prueba");
    let t = Arc::new(TenantDb(Arc::new(db)));
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    t
}

fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn pedido(t: &Arc<TenantDb>, id: i64) -> PedidoDetalle {
    let Json(p) = mesas::obtener_pedido(Extension(t.clone()), Path(id)).await.expect("pedido");
    p
}

/// Cobra lo elegido [(item_id, cantidad)] como lo manda el punto de venta:
/// esas líneas en el carrito y en pedido_items. None = el pedido entero.
async fn cobrar(t: &Arc<TenantDb>, id: i64, elegidos: Option<&[(i64, f64)]>) -> Result<i64, (StatusCode, String)> {
    let p = pedido(t, id).await;
    let activos: Vec<_> = p.items.iter().filter(|i| i.estado != "ANULADO").collect();
    let carrito: Vec<(i64, f64, f64)> = match elegidos {
        Some(e) => e
            .iter()
            .map(|(item, cant)| {
                let i = activos.iter().find(|i| i.id == *item).expect("línea");
                (i.producto_id, i.precio_unitario, *cant)
            })
            .collect(),
        None => activos.iter().map(|i| (i.producto_id, i.precio_unitario, i.cantidad)).collect(),
    };
    cobrar_con(t, id, &carrito, elegidos).await
}

/// El carrito [(producto_id, precio, cantidad)] y lo elegido van por separado
/// (para probar lo que no cuadra).
async fn cobrar_con(t: &Arc<TenantDb>, id: i64, carrito: &[(i64, f64, f64)], elegidos: Option<&[(i64, f64)]>) -> Result<i64, (StatusCode, String)> {
    let total: f64 = carrito.iter().map(|l| l.1 * l.2).sum();
    let productos: Vec<_> = carrito
        .iter()
        .map(|(pid, precio, cant)| json!({"id": pid, "nombre": "x", "precio": precio, "cantidad": cant, "descuentoMonto": 0}))
        .collect();
    let mut venta = json!({
        "productos": productos, "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0,
        "usuario_id": 1, "cliente_id": null, "pedido_id": id
    });
    if let Some(e) = elegidos {
        venta["pedido_items"] = json!(e.iter().map(|(item, cant)| json!({"item_id": item, "cantidad": cant})).collect::<Vec<_>>());
    }
    let venta: NuevaVenta = serde_json::from_value(venta).unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

async fn numero(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match f.next().await.unwrap().unwrap().get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        libsql::Value::Null => 0.0,
        otro => panic!("{}: {:?}", sql, otro),
    }
}

#[tokio::test]
async fn cada_uno_paga_lo_suyo_y_la_ultima_cuenta_libera_la_mesa() {
    let t = base("mesa").await;
    let Json(p) = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"tipo": "MESA", "mesa_id": 1, "cliente_nombre": null, "personas": 3})).unwrap()),
    )
    .await
    .expect("pedido abierto");
    let id = p.resumen.id;
    let _ = mesas::agregar_items(
        Extension(t.clone()),
        Extension(claims()),
        Path(id),
        Json(
            serde_json::from_value(json!({"items": [
                {"producto_id": 1, "cantidad": 1, "nota": "sin cebolla"},
                {"producto_id": 2, "cantidad": 1, "nota": null},
                {"producto_id": 3, "cantidad": 2, "nota": null}
            ]}))
            .unwrap(),
        ),
    )
    .await
    .expect("platos");
    let _ = mesas::enviar_a_preparar(Extension(t.clone()), Path(id)).await.expect("enviado");
    let p = pedido(&t, id).await;
    let (lomo, aji, chicha) = (p.items[0].id, p.items[1].id, p.items[2].id);
    // El lomo ya está listo en cocina.
    let _ = mesas::marcar_listo(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"item_ids": [lomo]})).unwrap()))
        .await
        .expect("listo");

    // Lo que no cuadra se rechaza sin tocar nada.
    assert_eq!(cobrar(&t, id, Some(&[(chicha, 3.0)])).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(cobrar(&t, id, Some(&[(chicha, 1.5)])).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(cobrar(&t, id, Some(&[(lomo, 1.0), (lomo, 1.0)])).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // El carrito (una chicha) no es lo elegido (el lomo).
    assert_eq!(cobrar_con(&t, id, &[(3, 5.0, 1.0)], Some(&[(lomo, 1.0)])).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(pedido(&t, id).await.resumen.total, 55.0);

    // Ana paga su lomo y una chicha: S/ 30.
    let venta_ana = cobrar(&t, id, Some(&[(lomo, 1.0), (chicha, 1.0)])).await.expect("cuenta de Ana");
    let p = pedido(&t, id).await;
    assert_eq!(p.estado, "ABIERTO", "la mesa sigue abierta");
    assert_eq!(p.resumen.total, 25.0, "queda el ají y una chicha");
    let quedan: Vec<_> = p.items.iter().map(|i| (i.id, i.cantidad)).collect();
    assert_eq!(quedan, vec![(aji, 1.0), (chicha, 1.0)]);
    assert_eq!(numero(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta_ana)).await, 30.0);

    // Lo de Ana está en su propio pedido cobrado: misma mesa, su venta, la
    // nota y el estado de cocina (el lomo listo se da por entregado).
    let separado = numero(&t, &format!("SELECT pedido_id FROM ventas WHERE id = {}", venta_ana)).await as i64;
    assert_ne!(separado, id);
    let s = pedido(&t, separado).await;
    assert_eq!(s.estado, "COBRADO");
    assert_eq!(s.resumen.mesa_id, Some(1));
    assert_eq!(numero(&t, &format!("SELECT venta_id FROM pedidos WHERE id = {}", separado)).await as i64, venta_ana);
    assert_eq!(s.items.len(), 2);
    assert_eq!(s.items[0].nota.as_deref(), Some("sin cebolla"));
    assert!(s.items[0].fecha_entregado.is_some());
    assert!(s.items.iter().all(|i| i.estado == "ENVIADO"));
    assert_eq!(s.items[1].cantidad, 1.0);

    // Lo ya cobrado no se puede volver a cobrar.
    assert_eq!(cobrar_con(&t, id, &[(1, 25.0, 1.0)], Some(&[(lomo, 1.0)])).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));

    // La última cuenta (todo lo que queda) libera la mesa, con o sin pedido_items.
    let venta_luis = cobrar(&t, id, Some(&[(aji, 1.0), (chicha, 1.0)])).await.expect("cuenta de Luis");
    let p = pedido(&t, id).await;
    assert_eq!(p.estado, "COBRADO");
    assert_eq!(numero(&t, &format!("SELECT venta_id FROM pedidos WHERE id = {}", id)).await as i64, venta_luis);
    assert_eq!(numero(&t, "SELECT COUNT(*) FROM pedidos WHERE mesa_id = 1 AND estado = 'ABIERTO'").await, 0.0);
    assert_eq!(numero(&t, "SELECT SUM(total) FROM ventas").await, 55.0);
}

#[tokio::test]
async fn sin_cuenta_separada_el_cobro_de_siempre() {
    let t = base("siempre").await;
    let Json(p) = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"tipo": "MESA", "mesa_id": 1, "cliente_nombre": null, "personas": 2})).unwrap()),
    )
    .await
    .unwrap();
    let id = p.resumen.id;
    let _ = mesas::agregar_items(
        Extension(t.clone()),
        Extension(claims()),
        Path(id),
        Json(serde_json::from_value(json!({"items": [{"producto_id": 1, "cantidad": 2, "nota": null}]})).unwrap()),
    )
    .await
    .unwrap();
    // Cobrar menos sin decir qué: se rechaza como siempre.
    assert_eq!(cobrar_con(&t, id, &[(1, 25.0, 1.0)], None).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    cobrar(&t, id, None).await.expect("cobro completo");
    assert_eq!(pedido(&t, id).await.estado, "COBRADO");
    assert_eq!(numero(&t, "SELECT COUNT(*) FROM pedidos").await, 1.0, "no se crea ningún pedido aparte");
}
