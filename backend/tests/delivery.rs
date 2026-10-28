//! Módulo DELIVERY (dark kitchen): pedidos por WhatsApp sobre los pedidos del
//! restaurante, contra una base local (schema.sql + todas las migraciones) y
//! los handlers de verdad. Revisa:
//!   - contra entrega: cliente, platos y envío en un solo formulario, comanda
//!     al registrar (sin el envío), agregar y quitar platos, listo, despacho
//!     con "paga con" y vuelto, por rendir, cobro con el envío y resumen;
//!   - adelantado: espera el pago fuera de cocina, se edita libre, al cobrarse
//!     entra a cocina; ya cobrado no cambian platos ni envío;
//!   - recojo; no se cobra solo el envío; anulado;
//!   - un restaurante sin el módulo recibe 403 y sus pedidos siguen igual.
//!
//!   cargo test --test delivery

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{delivery, mesas, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("delivery_{}_{}.db", nombre, std::process::id()));
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
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'RESTAURANTE', modo_negocio = 'RESTAURANTE', modulos = ?1, nombre_tienda = 'DARK KITCHEN PRUEBA'",
        libsql::params![modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Platos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, unidad_medida) VALUES
            (5001, 'LOMO', 'Lomo saltado', 25, 0, 900, 0, 'PLATO'),
            (5002, 'INKA', 'Inca Kola 500 ml', 2.5, 50, 900, 1, 'UNIDAD');
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'DNI', '12345678', 'Ana Huamán', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn dark_kitchen(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "SERVICIOS,DELIVERY").await
}

fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin de prueba".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
}

async fn crear(t: &Arc<TenantDb>, datos: serde_json::Value) -> Result<delivery::PedidoDelivery, (StatusCode, String)> {
    delivery::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(datos).unwrap()))
        .await
        .map(|Json(p)| p)
}

async fn editar(t: &Arc<TenantDb>, id: i64, datos: serde_json::Value) -> Result<delivery::PedidoDelivery, (StatusCode, String)> {
    delivery::actualizar(Extension(t.clone()), Extension(claims()), Path(id), Json(serde_json::from_value(datos).unwrap()))
        .await
        .map(|Json(p)| p)
}

async fn despachar(t: &Arc<TenantDb>, id: i64, quien: &str) -> Result<delivery::PedidoDelivery, (StatusCode, String)> {
    despachar_con(t, id, quien, None).await
}

async fn despachar_con(t: &Arc<TenantDb>, id: i64, quien: &str, paga_con: Option<f64>) -> Result<delivery::PedidoDelivery, (StatusCode, String)> {
    delivery::despachar(Extension(t.clone()), Path(id), Json(serde_json::from_value(json!({ "repartidor": quien, "paga_con": paga_con })).unwrap()))
        .await
        .map(|Json(p)| p)
}

async fn entregar(t: &Arc<TenantDb>, id: i64) -> Result<delivery::PedidoDelivery, (StatusCode, String)> {
    delivery::entregar(Extension(t.clone()), Path(id)).await.map(|Json(p)| p)
}

async fn tablero(t: &Arc<TenantDb>) -> delivery::Tablero {
    let Json(x) = delivery::tablero(Extension(t.clone())).await.expect("tablero");
    x
}

/// El cobro del punto de venta: las líneas del pedido tal cual (POS.jsx, carritoDePedido).
async fn cobrar(t: &Arc<TenantDb>, id: i64, metodo: &str) -> Result<i64, (StatusCode, String)> {
    let Json(p) = mesas::obtener_pedido(Extension(t.clone()), Path(id)).await.unwrap();
    let lineas: Vec<serde_json::Value> = p
        .items
        .iter()
        .filter(|i| i.estado != "ANULADO")
        .map(|i| json!({"id": i.producto_id, "nombre": i.nombre_producto, "precio": i.precio_unitario, "cantidad": i.cantidad, "descuentoMonto": 0}))
        .collect();
    let total: f64 = p.items.iter().filter(|i| i.estado != "ANULADO").map(|i| i.subtotal).sum();
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": lineas, "total": total, "metodo_pago": metodo, "monto_recibido": total, "cambio": 0,
        "usuario_id": 1, "cliente_id": null, "pedido_id": id
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match filas.next().await.unwrap().unwrap().get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: {:?}", sql, otro),
    }
}

fn lomo_e_inka() -> serde_json::Value {
    json!([{"producto_id": 5001, "cantidad": 1, "nota": "sin cebolla"}, {"producto_id": 5002, "cantidad": 2}])
}

#[tokio::test]
async fn contra_entrega_va_a_cocina_sale_con_vuelto_y_se_rinde() {
    let t = dark_kitchen("contra").await;
    abrir_caja(&t).await;

    // Sin platos no entra.
    assert_eq!(
        crear(&t, json!({"cliente_id": 1, "direccion": "Av. La Cultura 123", "pago": "CONTRA_ENTREGA", "costo_envio": 5})).await.err().map(|e| e.0),
        Some(StatusCode::BAD_REQUEST)
    );

    // 1. Todo en un formulario: cliente, platos y envío. Contra entrega: va a cocina ya.
    let p = crear(
        &t,
        json!({
            "cliente_id": 1, "direccion": "Av. La Cultura 123", "referencia": "Puerta verde",
            "pago": "CONTRA_ENTREGA", "costo_envio": 5, "agregar": lomo_e_inka()
        }),
    )
    .await
    .expect("pedido");
    assert_eq!(p.cliente_nombre.as_deref(), Some("Ana Huamán"));
    assert_eq!(p.telefono.as_deref(), Some("987654321"));
    assert_eq!(p.total, 35.0);
    assert_eq!(p.fase, "EN_COCINA");
    assert_eq!(p.lineas.len(), 2, "el envío no es un plato");
    assert_eq!(p.comanda.len(), 2, "la comanda sale al registrar, sin el envío");
    assert_eq!(p.preparando, 2);
    let Json(prep) = mesas::listar_preparacion(Extension(t.clone())).await.unwrap();
    assert_eq!(prep.iter().find(|x| x.pedido_id == p.id).expect("en cocina").items.len(), 2);

    // 2. Se agrega una gaseosa más: va a cocina en una comanda nueva.
    let x = editar(
        &t,
        p.id,
        json!({"cliente_id": 1, "direccion": "Av. La Cultura 123", "pago": "CONTRA_ENTREGA", "costo_envio": 5,
               "agregar": [{"producto_id": 5002, "cantidad": 1}]}),
    )
    .await
    .unwrap();
    assert_eq!(x.comanda.len(), 1);
    assert_eq!(x.total, 37.5);
    // Quitar algo que ya está en cocina pide el motivo.
    let gaseosa = x.lineas.iter().find(|l| l.producto_id == 5002 && l.cantidad == 1.0).unwrap().id;
    let quitar = json!({"cliente_id": 1, "direccion": "Av. La Cultura 123", "pago": "CONTRA_ENTREGA", "costo_envio": 5, "quitar": [gaseosa]});
    assert_eq!(editar(&t, p.id, quitar.clone()).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let mut con_motivo = quitar.clone();
    con_motivo["motivo_quitar"] = json!("El cliente ya no la quiere");
    assert_eq!(editar(&t, p.id, con_motivo).await.unwrap().total, 35.0);

    // 3. Cocina termina y sale con Luis: el cliente paga con 50 (vuelto 15).
    assert_eq!(delivery::listo(Extension(t.clone()), Path(p.id)).await.unwrap().0.fase, "LISTO");
    assert_eq!(despachar_con(&t, p.id, "Luis", Some(20.0)).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST), "no alcanza");
    let x = despachar_con(&t, p.id, "Luis", Some(50.0)).await.expect("despacho");
    assert_eq!(x.fase, "EN_CAMINO");
    assert_eq!(x.vuelto, Some(15.0));
    let Json(prep) = mesas::listar_preparacion(Extension(t.clone())).await.unwrap();
    assert!(prep.iter().all(|x| x.pedido_id != p.id));
    // Ya salió: los platos no cambian.
    assert_eq!(
        editar(&t, p.id, json!({"cliente_id": 1, "direccion": "X", "pago": "CONTRA_ENTREGA", "costo_envio": 5, "agregar": lomo_e_inka()})).await.err().map(|e| e.0),
        Some(StatusCode::CONFLICT)
    );

    // 4. Entregado sin cobrar: por rendir.
    let x = entregar(&t, p.id).await.expect("entregado");
    assert_eq!(x.fase, "POR_RENDIR");
    assert_eq!(tablero(&t).await.nombres_recientes, vec!["Luis".to_string()]);

    // 5. Luis rinde: se cobra en el punto de venta, con el envío en la venta.
    let venta = cobrar(&t, p.id, "EFECTIVO").await.expect("cobro");
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta)).await, 35.0);
    assert_eq!(real(&t, &format!("SELECT COUNT(*) FROM detalles_venta WHERE venta_id = {}", venta)).await, 3.0);
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 35.0);
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 5002").await, 48.0);
    let x = tablero(&t).await.pedidos.into_iter().find(|x| x.id == p.id).unwrap();
    assert_eq!(x.fase, "ENTREGADO");
    assert_eq!(x.venta_id, Some(venta));
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos WHERE codigo = '_DLV_ENVIO'").await, 1.0);

    // 6. Resumen del día: por medio, por repartidor y envíos.
    let Json(r) = delivery::resumen(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(r.pedidos, 1);
    assert_eq!(r.total, 35.0);
    assert_eq!(r.envios, 5.0);
    assert_eq!(r.por_metodo[0].nombre, "EFECTIVO");
    assert_eq!(r.por_repartidor[0].nombre, "Luis");
    assert_eq!(r.filas[0].folio.is_some(), true);
}

#[tokio::test]
async fn adelantado_espera_el_pago_y_entra_a_cocina_al_cobrar() {
    let t = dark_kitchen("adelantado").await;
    abrir_caja(&t).await;

    // Sin celular o sin dirección no entra un delivery.
    for datos in [
        json!({"cliente_nombre": "Rosa", "direccion": "Jr. Lima 1", "pago": "ADELANTADO", "agregar": lomo_e_inka()}),
        json!({"cliente_nombre": "Rosa", "telefono": "999888777", "pago": "ADELANTADO", "agregar": lomo_e_inka()}),
    ] {
        assert_eq!(crear(&t, datos).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    }

    // 1. Adelantado por Yape: espera el pago, no va a cocina.
    let datos = json!({"cliente_nombre": "Rosa", "telefono": "999888777", "direccion": "Jr. Lima 1", "pago": "ADELANTADO",
                       "metodo_pago": "YAPE", "costo_envio": 4, "agregar": [{"producto_id": 5001, "cantidad": 2}]});
    let p = crear(&t, datos.clone()).await.unwrap();
    assert_eq!(p.fase, "ESPERANDO_PAGO");
    assert!(p.comanda.is_empty());
    assert_eq!(p.pendientes, 1);
    let Json(prep) = mesas::listar_preparacion(Extension(t.clone())).await.unwrap();
    assert!(prep.iter().all(|x| x.pedido_id != p.id), "no llega a cocina sin pago");
    assert_eq!(despachar(&t, p.id, "Luis").await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(delivery::listo(Extension(t.clone()), Path(p.id)).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));

    // 2. Mientras espera el pago se edita libre: otro plato, envío a 6, quitar uno.
    let mut cambio = datos.clone();
    cambio["costo_envio"] = json!(6);
    cambio["agregar"] = json!([{"producto_id": 5002, "cantidad": 1}]);
    let x = editar(&t, p.id, cambio).await.unwrap();
    assert!(x.comanda.is_empty(), "sigue esperando el pago");
    assert_eq!(x.total, 58.5);
    let lomo = x.lineas.iter().find(|l| l.producto_id == 5001).unwrap().id;
    let mut quitar = datos.clone();
    quitar["costo_envio"] = json!(6);
    quitar["agregar"] = json!([]);
    quitar["quitar"] = json!([lomo]);
    let x = editar(&t, p.id, quitar).await.unwrap();
    assert_eq!(x.total, 8.5, "sin motivo: aún no estaba en cocina");
    // No queda sin platos.
    let gaseosa = x.lineas[0].id;
    let mut vacio = datos.clone();
    vacio["costo_envio"] = json!(6);
    vacio["agregar"] = json!([]);
    vacio["quitar"] = json!([gaseosa]);
    assert_eq!(editar(&t, p.id, vacio).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // 3. Confirma el pago: se cobra (sale el comprobante) y recién va a cocina.
    cobrar(&t, p.id, "YAPE").await.expect("cobro por Yape");
    let x = tablero(&t).await.pedidos.into_iter().find(|x| x.id == p.id).unwrap();
    assert!(x.cobrado);
    assert_eq!(x.fase, "EN_COCINA");
    assert_eq!(x.pendientes, 0);
    assert_eq!(x.preparando, 1);
    let Json(prep) = mesas::listar_preparacion(Extension(t.clone())).await.unwrap();
    assert!(prep.iter().any(|x| x.pedido_id == p.id), "entró a cocina al pagar");
    // Ya cobrado: no cambian platos ni envío; la dirección sí.
    let mut otra = datos.clone();
    otra["costo_envio"] = json!(9);
    otra["agregar"] = json!([]);
    assert_eq!(editar(&t, p.id, otra).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    let mut dir = datos.clone();
    dir["costo_envio"] = json!(6);
    dir["agregar"] = json!([]);
    dir["direccion"] = json!("Jr. Lima 100");
    assert_eq!(editar(&t, p.id, dir).await.unwrap().direccion.as_deref(), Some("Jr. Lima 100"));

    // 4. Sale (paga con no aplica: ya pagó), se deshace y vuelve a salir.
    let x = despachar_con(&t, p.id, "Pedro", Some(100.0)).await.expect("sale");
    assert_eq!(x.paga_con, None);
    let Json(x) = delivery::volver(Extension(t.clone()), Path(p.id)).await.unwrap();
    assert_eq!(x.fase, "LISTO", "la comida ya estaba hecha: vuelve como lista para salir");
    despachar(&t, p.id, "Luis").await.expect("sale otra vez");
    assert_eq!(entregar(&t, p.id).await.unwrap().fase, "ENTREGADO");
    let Json(r) = delivery::resumen(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(r.por_metodo[0].nombre, "YAPE");
}

#[tokio::test]
async fn recojo_y_cobro_solo_del_envio() {
    let t = dark_kitchen("recojo").await;
    abrir_caja(&t).await;

    // Recojo: sin envío, va a cocina, no se despacha y no se entrega sin cobrar.
    let r = crear(&t, json!({"cliente_nombre": "Juan", "entrega": "RECOJO", "pago": "CONTRA_ENTREGA", "costo_envio": 5,
                              "agregar": [{"producto_id": 5002, "cantidad": 1}]}))
        .await
        .unwrap();
    assert_eq!(r.costo_envio, 0.0);
    assert_eq!(r.comanda.len(), 1);
    assert_eq!(despachar(&t, r.id, "Luis").await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(entregar(&t, r.id).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    cobrar(&t, r.id, "EFECTIVO").await.unwrap();
    assert_eq!(entregar(&t, r.id).await.unwrap().fase, "ENTREGADO");

    // Un pedido delivery al que le anularon los platos (desde Mesas): no se cobra solo el envío.
    let p = crear(&t, json!({"cliente_nombre": "X", "telefono": "1", "direccion": "Y", "pago": "CONTRA_ENTREGA", "costo_envio": 5,
                              "agregar": [{"producto_id": 5002, "cantidad": 1}]}))
        .await
        .unwrap();
    let plato = p.lineas[0].id;
    let _ = mesas::quitar_item(
        Extension(t.clone()),
        Extension(claims()),
        Path((p.id, plato)),
        Json(serde_json::from_value(json!({"motivo": "error"})).unwrap()),
    )
    .await
    .unwrap();
    let (codigo, mensaje) = cobrar(&t, p.id, "EFECTIVO").await.err().expect("no cobra solo el envío");
    assert_eq!(codigo, StatusCode::BAD_REQUEST);
    assert!(mensaje.contains("envío"), "{mensaje}");

    // Anulado: deja el tablero.
    let _ = mesas::anular_pedido(Extension(t.clone()), Extension(claims()), Path(p.id), Json(serde_json::from_value(json!({"motivo": "Canceló"})).unwrap()))
        .await
        .unwrap();
    assert!(tablero(&t).await.pedidos.iter().all(|x| x.id != p.id));
}

#[tokio::test]
async fn restaurante_sin_el_modulo() {
    let t = base("sin_modulo", "SERVICIOS").await;
    abrir_caja(&t).await;
    assert_eq!(delivery::tablero(Extension(t.clone())).await.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
    assert_eq!(
        crear(&t, json!({"cliente_nombre": "X", "telefono": "1", "direccion": "Y", "pago": "CONTRA_ENTREGA", "agregar": lomo_e_inka()})).await.err().map(|e| e.0),
        Some(StatusCode::FORBIDDEN)
    );
    // Sus pedidos de siempre (delivery desde Mesas) funcionan igual: se agrega,
    // se cobra con lo pendiente sin enviar y lo pendiente no pasa solo a cocina.
    let Json(p) = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"tipo": "DELIVERY", "cliente_nombre": "Ana"})).unwrap()),
    )
    .await
    .unwrap();
    let _ = mesas::agregar_items(Extension(t.clone()), Extension(claims()), Path(p.resumen.id), Json(serde_json::from_value(json!({"items": lomo_e_inka()})).unwrap()))
        .await
        .unwrap();
    cobrar(&t, p.resumen.id, "EFECTIVO").await.expect("cobro de siempre");
    assert_eq!(real(&t, &format!("SELECT COUNT(*) FROM pedido_items WHERE pedido_id = {} AND estado = 'PENDIENTE'", p.resumen.id)).await, 2.0);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM pedido_delivery").await, 0.0);
    assert_eq!(minimarket_backend::handlers::rubros::faltante(&["DELIVERY".to_string()]), Some(("DELIVERY", "MESAS")));
}

async fn rol_repartidor(t: &Arc<TenantDb>) -> i64 {
    real(t, "SELECT id FROM roles WHERE nombre = 'REPARTIDOR'").await as i64
}

async fn cobrar_como(t: &Arc<TenantDb>, quien: Claims, id: i64, metodo: &str) -> Result<i64, (StatusCode, String)> {
    let Json(p) = mesas::obtener_pedido(Extension(t.clone()), Path(id)).await.unwrap();
    let lineas: Vec<serde_json::Value> = p
        .items
        .iter()
        .filter(|i| i.estado != "ANULADO")
        .map(|i| json!({"id": i.producto_id, "nombre": i.nombre_producto, "precio": i.precio_unitario, "cantidad": i.cantidad, "descuentoMonto": 0}))
        .collect();
    let total: f64 = p.items.iter().filter(|i| i.estado != "ANULADO").map(|i| i.subtotal).sum();
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": lineas, "total": total, "metodo_pago": metodo, "monto_recibido": total, "cambio": 0,
        "usuario_id": quien.sub, "cliente_id": null, "pedido_id": id
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(quien), Json(venta)).await.map(|Json(r)| r.venta_id)
}

#[tokio::test]
async fn repartidor_con_su_cuenta_cobra_en_la_puerta_y_rinde() {
    let t = dark_kitchen("repartidor").await;
    abrir_caja(&t).await;
    let rol = rol_repartidor(&t).await;
    let conn = t.0.connect().unwrap();
    conn.execute(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (2, 'luis', 'x', 'Luis Quispe', ?1), (3, 'pedro', 'x', 'Pedro Ccori', ?1)",
        libsql::params![rol],
    )
    .await
    .unwrap();
    let luis = Claims { sub: 2, username: "luis".into(), rol_id: rol, nombre_completo: "Luis Quispe".into(), tienda_id: 1, exp: usize::MAX, tv: 0 };

    // El rol solo se ofrece con el módulo encendido.
    let Json(roles) = mesas::listar_roles(Extension(t.clone()), Extension(claims())).await.unwrap();
    assert!(roles.iter().any(|r| r.nombre == "REPARTIDOR"));
    let sin = base("rep_sin_modulo", "SERVICIOS").await;
    let Json(roles) = mesas::listar_roles(Extension(sin.clone()), Extension(claims())).await.unwrap();
    assert!(roles.iter().all(|r| r.nombre != "REPARTIDOR"));
    assert!(roles.iter().any(|r| r.nombre == "MESERO"));

    // 1. Dos pedidos contra entrega y uno adelantado, asignados a Luis en orden.
    let a = crear(&t, json!({"cliente_id": 1, "direccion": "Av. La Cultura 123", "pago": "CONTRA_ENTREGA", "costo_envio": 5, "agregar": lomo_e_inka()})).await.unwrap();
    let b = crear(&t, json!({"cliente_nombre": "Rosa", "telefono": "999", "direccion": "Jr. Lima 1", "pago": "CONTRA_ENTREGA", "agregar": [{"producto_id": 5002, "cantidad": 4}]})).await.unwrap();
    let c = crear(&t, json!({"cliente_nombre": "Juan", "telefono": "988", "direccion": "Jr. Puno 2", "pago": "ADELANTADO", "metodo_pago": "YAPE", "agregar": [{"producto_id": 5001, "cantidad": 1}]})).await.unwrap();
    let asignar = |id: i64, r: Option<i64>| {
        let t = t.clone();
        async move { delivery::asignar(Extension(t), Path(id), Json(serde_json::from_value(json!({"repartidor_id": r})).unwrap())).await.map(|Json(p)| p) }
    };
    assert_eq!(asignar(a.id, Some(1)).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST), "el admin no es repartidor");
    let x = asignar(a.id, Some(2)).await.unwrap();
    assert_eq!((x.repartidor_id, x.orden_reparto, x.repartidor.as_deref()), (Some(2), Some(1), Some("Luis Quispe")));
    assert_eq!(asignar(b.id, Some(2)).await.unwrap().orden_reparto, Some(2));
    assert_eq!(asignar(c.id, Some(2)).await.unwrap().orden_reparto, Some(3));
    // Reasignar a Pedro y devolverlo: vuelve con el siguiente orden.
    assert_eq!(asignar(b.id, Some(3)).await.unwrap().orden_reparto, Some(1));
    assert_eq!(asignar(b.id, Some(2)).await.unwrap().orden_reparto, Some(4));
    let Json(tab) = delivery::tablero(Extension(t.clone())).await.unwrap();
    assert_eq!(tab.repartidores.len(), 2);

    // 2. Luis ve sus entregas en orden.
    let mis = || {
        let t = t.clone();
        let luis = luis.clone();
        async move { delivery::mis_entregas(Extension(t), Extension(luis)).await.unwrap().0 }
    };
    let m = mis().await;
    assert_eq!(m.pedidos.iter().map(|p| p.id).collect::<Vec<_>>(), vec![a.id, c.id, b.id]);

    // 3. No cobra antes de salir, ni algo que no es suyo, ni una venta suelta.
    assert_eq!(cobrar_como(&t, luis.clone(), a.id, "EFECTIVO").await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    let ajeno = crear(&t, json!({"cliente_nombre": "X", "telefono": "1", "direccion": "Y", "pago": "CONTRA_ENTREGA", "agregar": [{"producto_id": 5002, "cantidad": 1}]})).await.unwrap();
    assert_eq!(cobrar_como(&t, luis.clone(), ajeno.id, "EFECTIVO").await.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
    let suelta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 5002, "nombre": "Inca", "precio": 2.5, "cantidad": 1, "descuentoMonto": 0}], "total": 2.5,
        "metodo_pago": "EFECTIVO", "monto_recibido": 2.5, "cambio": 0, "usuario_id": 2, "cliente_id": null
    }))
    .unwrap();
    assert_eq!(
        ventas::procesar_venta(Extension(t.clone()), Extension(luis.clone()), Json(suelta)).await.err().map(|e| e.0),
        Some(StatusCode::FORBIDDEN)
    );
    // Tampoco sale con un pedido de otro.
    let r = delivery::reparto_salir(Extension(t.clone()), Extension(luis.clone()), Path(ajeno.id), Json(serde_json::from_value(json!({})).unwrap())).await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));

    // 4. Sale con el primero (paga con 50), y en la puerta no lo marca entregado sin cobrar.
    let x = delivery::reparto_salir(Extension(t.clone()), Extension(luis.clone()), Path(a.id), Json(serde_json::from_value(json!({"paga_con": 50})).unwrap()))
        .await
        .unwrap()
        .0;
    assert_eq!((x.fase.as_str(), x.vuelto), ("EN_CAMINO", Some(15.0)));
    let r = delivery::reparto_entregar(Extension(t.clone()), Extension(luis.clone()), Path(a.id)).await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    // Cobra delante del cliente: queda entregado.
    let venta = cobrar_como(&t, luis.clone(), a.id, "EFECTIVO").await.expect("cobro en la puerta");
    assert_eq!(real(&t, &format!("SELECT usuario_id FROM ventas WHERE id = {}", venta)).await, 2.0);
    let x = mis().await.pedidos.into_iter().find(|p| p.id == a.id).unwrap();
    assert_eq!(x.fase, "ENTREGADO");

    // 5. El adelantado: lo cobra el cajero; Luis sale y solo lo entrega.
    assert_eq!(
        delivery::reparto_salir(Extension(t.clone()), Extension(luis.clone()), Path(c.id), Json(serde_json::from_value(json!({})).unwrap())).await.err().map(|e| e.0),
        Some(StatusCode::CONFLICT),
        "adelantado sin pagar no sale"
    );
    cobrar(&t, c.id, "YAPE").await.unwrap();
    delivery::reparto_salir(Extension(t.clone()), Extension(luis.clone()), Path(c.id), Json(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(cobrar_como(&t, luis.clone(), c.id, "EFECTIVO").await.err().map(|e| e.0), Some(StatusCode::CONFLICT), "ya pagado");
    assert_eq!(delivery::reparto_entregar(Extension(t.clone()), Extension(luis.clone()), Path(c.id)).await.unwrap().0.fase, "ENTREGADO");

    // 6. El segundo contra entrega lo paga por Yape en la puerta.
    delivery::reparto_salir(Extension(t.clone()), Extension(luis.clone()), Path(b.id), Json(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    cobrar_como(&t, luis.clone(), b.id, "YAPE").await.unwrap();

    // 7. Por rendir: Luis trae S/ 35 en efectivo (lo de Yape ya está en la cuenta).
    let Json(tab) = delivery::tablero(Extension(t.clone())).await.unwrap();
    let r = tab.rendicion.iter().find(|r| r.repartidor_id == 2).expect("por rendir");
    assert_eq!((r.efectivo, r.digital, r.pedidos.len()), (35.0, 10.0, 2));
    assert!(tab.pedidos.iter().all(|p| p.fase != "POR_RENDIR"), "lo cobrado en la puerta no queda por cobrar");
    let Json(rendido) = delivery::rendir(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"repartidor_id": 2})).unwrap()))
        .await
        .unwrap();
    assert_eq!((rendido.pedidos, rendido.efectivo), (2, 35.0));
    let Json(tab) = delivery::tablero(Extension(t.clone())).await.unwrap();
    assert!(tab.rendicion.is_empty());
    assert_eq!(
        delivery::rendir(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"repartidor_id": 2})).unwrap())).await.err().map(|e| e.0),
        Some(StatusCode::CONFLICT)
    );
    // Todo entró a la única caja abierta del negocio.
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 35.0);
}
