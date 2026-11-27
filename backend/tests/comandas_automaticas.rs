//! Módulo "Impresión de comandas" (COMANDAS, migración 0074) contra una base
//! local (schema.sql + todas las migraciones) y los handlers de verdad:
//!   - apagado (como siempre): la estación no toma nada y lo enviado queda
//!     como impreso, así al encenderlo no salen comandas viejas;
//!   - encendido: lo que el mozo envía sale una sola vez, una comanda por
//!     envío; lo pendiente no sale;
//!   - al cobrar la mesa, todo lo enviado queda entregado (la cocina no marca
//!     listo); sin el módulo, solo lo que estaba listo, como siempre.
//!
//!   cargo test --test comandas_automaticas

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    Json,
};
use minimarket_backend::handlers::{mesas, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

const SIN_MODULO: &str = "MESAS,SERVICIOS";
const CON_MODULO: &str = "MESAS,SERVICIOS,COMANDAS";

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("comandas_{}_{}.db", nombre, std::process::id()));
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
        "UPDATE configuracion_tienda SET rubro = 'RESTAURANTE', modo_negocio = 'RESTAURANTE', nombre_tienda = 'POLLERIA PRUEBA';
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Platos');
         INSERT INTO mesas (id, nombre) VALUES (1, 'Mesa 1'), (2, 'Mesa 2'), (3, 'Mesa 3');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, unidad_medida) VALUES
            (5001, 'POLLO', '1/4 de pollo', 18, 0, 900, 0, 'PLATO'),
            (5002, 'INKA', 'Inca Kola 500 ml', 2.5, 50, 900, 1, 'UNIDAD');",
    )
    .await
    .expect("datos de prueba");
    let t = Arc::new(TenantDb(Arc::new(db)));
    modulos_de(&t, modulos).await;
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    t
}

async fn modulos_de(t: &Arc<TenantDb>, modulos: &str) {
    t.0.connect()
        .unwrap()
        .execute("UPDATE configuracion_tienda SET modulos = ?1", libsql::params![modulos])
        .await
        .unwrap();
}

fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin de prueba".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn mesa_con(t: &Arc<TenantDb>, mesa: i64, producto: i64, cantidad: f64) -> i64 {
    let Json(p) = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"tipo": "MESA", "mesa_id": mesa})).unwrap()),
    )
    .await
    .expect("abrir pedido");
    agregar(t, p.resumen.id, producto, cantidad).await;
    p.resumen.id
}

async fn agregar(t: &Arc<TenantDb>, pedido: i64, producto: i64, cantidad: f64) {
    let _ = mesas::agregar_items(
        Extension(t.clone()),
        Extension(claims()),
        Path(pedido),
        Json(serde_json::from_value(json!({"items": [{"producto_id": producto, "cantidad": cantidad}]})).unwrap()),
    )
    .await
    .expect("agregar");
}

async fn enviar(t: &Arc<TenantDb>, pedido: i64) {
    let _ = mesas::enviar_a_preparar(Extension(t.clone()), Path(pedido)).await.expect("enviar");
}

async fn tomar(t: &Arc<TenantDb>) -> Vec<(i64, Vec<String>)> {
    let Json(c) = mesas::tomar_comandas(Extension(t.clone())).await.expect("tomar");
    c.into_iter()
        .map(|c| (c.pedido.id, c.items.into_iter().map(|i| i.nombre_producto).collect()))
        .collect()
}

async fn activo(t: &Arc<TenantDb>) -> bool {
    let Json(e) = mesas::estado_comandas(Extension(t.clone())).await.unwrap();
    e.activo
}

async fn cobrar(t: &Arc<TenantDb>, id: i64) {
    let Json(p) = mesas::obtener_pedido(Extension(t.clone()), Path(id)).await.unwrap();
    let lineas: Vec<serde_json::Value> = p
        .items
        .iter()
        .filter(|i| i.estado != "ANULADO")
        .map(|i| json!({"id": i.producto_id, "nombre": i.nombre_producto, "precio": i.precio_unitario, "cantidad": i.cantidad, "descuentoMonto": 0}))
        .collect();
    let total: f64 = p.items.iter().filter(|i| i.estado != "ANULADO").map(|i| i.subtotal).sum();
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": lineas, "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0,
        "usuario_id": 1, "cliente_id": null, "pedido_id": id
    }))
    .unwrap();
    let _ = ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.expect("cobro");
}

async fn sin_entregar(t: &Arc<TenantDb>, pedido: i64) -> i64 {
    let conn = t.0.connect().unwrap();
    let mut f = conn
        .query(
            "SELECT COUNT(*) FROM pedido_items WHERE pedido_id = ?1 AND estado = 'ENVIADO' AND fecha_entregado IS NULL",
            libsql::params![pedido],
        )
        .await
        .unwrap();
    f.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test]
async fn apagado_no_toma_nada_y_al_encender_no_salen_viejas() {
    let t = base("apagado", SIN_MODULO).await;
    assert!(!activo(&t).await);
    let p = mesa_con(&t, 1, 5001, 2.0).await;
    enviar(&t, p).await;
    assert!(tomar(&t).await.is_empty());

    // Se enciende el módulo: lo enviado antes ya salió en el equipo del mozo.
    modulos_de(&t, CON_MODULO).await;
    assert!(activo(&t).await);
    assert!(tomar(&t).await.is_empty());
}

#[tokio::test]
async fn encendido_cada_envio_sale_una_sola_vez() {
    let t = base("encendido", CON_MODULO).await;
    let a = mesa_con(&t, 1, 5001, 2.0).await;
    let b = mesa_con(&t, 2, 5002, 3.0).await;
    enviar(&t, a).await;
    enviar(&t, b).await;
    let primera = tomar(&t).await;
    assert_eq!(primera.len(), 2);
    assert!(primera.contains(&(a, vec!["1/4 de pollo".to_string()])));
    assert!(primera.contains(&(b, vec!["Inca Kola 500 ml".to_string()])));
    // Otra estación (o la misma recargada) ya no recibe esas comandas.
    assert!(tomar(&t).await.is_empty());

    // Lo que se agrega después y se vuelve a enviar sale solo eso.
    agregar(&t, a, 5002, 1.0).await;
    enviar(&t, a).await;
    assert_eq!(tomar(&t).await, vec![(a, vec!["Inca Kola 500 ml".to_string()])]);

    // Lo pendiente (sin enviar) nunca sale.
    agregar(&t, b, 5001, 1.0).await;
    assert!(tomar(&t).await.is_empty());
}

#[tokio::test]
async fn al_cobrar_con_el_modulo_todo_lo_enviado_queda_entregado() {
    let t = base("cobro", CON_MODULO).await;
    let p = mesa_con(&t, 1, 5001, 1.0).await;
    enviar(&t, p).await;
    let _ = tomar(&t).await;
    assert_eq!(sin_entregar(&t, p).await, 1);
    cobrar(&t, p).await;
    assert_eq!(sin_entregar(&t, p).await, 0);

    // Sin el módulo, como siempre: lo que la cocina no marcó listo sigue en Preparación.
    let t = base("cobro_sin", SIN_MODULO).await;
    let p = mesa_con(&t, 1, 5001, 1.0).await;
    enviar(&t, p).await;
    cobrar(&t, p).await;
    assert_eq!(sin_entregar(&t, p).await, 1);
}
