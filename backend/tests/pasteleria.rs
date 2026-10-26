//! Rubro Pastelería / Panadería: pedidos por encargo (módulo ENCARGOS)
//! contra una base local (schema.sql + todas las migraciones) y los handlers
//! de verdad. Revisa la caja (el adelanto entra una sola vez), la entrega
//! cobrando el saldo, la cancelación y que un negocio de otro rubro no vea
//! nada nuevo.
//!
//!   cargo test --test pasteleria

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{encargos, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

const MODULOS_PASTELERIA: &str = "SERVICIOS,MEDIDAS,COTIZACIONES,PRESENTACIONES,PRECIO_MAYOR,ENCARGOS,PRODUCCION,INSUMOS";

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("pasteleria_{}_{}.db", nombre, std::process::id()));
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
    // La migración nueva es idempotente: aplicarla otra vez no rompe nada.
    for m in migraciones.iter().filter(|m| m.file_name().unwrap().to_string_lossy().as_ref() >= "0042") {
        // Un ADD COLUMN no se puede repetir: el aplicador real
        // (src/migraciones.rs) salta el "duplicate column name".
        if let Err(e) = conn.execute_batch(&std::fs::read_to_string(m).unwrap()).await {
            assert!(e.to_string().contains("duplicate column name"), "{} (segunda vez): {}", m.display(), e);
        }
    }
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'PASTELERIA PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Tortas'), (901, 'Accesorios');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock) VALUES
            (2001, 'TOR-PERS', 'Torta personalizada', 80, 0, 900, 0);
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (2002, 'VELA', 'Vela de número', 5, 50, 901),
            (2003, 'ALF', 'Alfajor', 2.5, 100, 900);
         INSERT INTO categorias (id, nombre) VALUES (902, 'Insumos'), (903, 'Pan');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, unidad_medida, precio_compra) VALUES
            (3001, 'HAR', 'Harina', 4, 50, 902, 'KG', 3.5),
            (3002, 'HUE', 'Huevo', 0.6, 100, 902, 'UNIDAD', 0.5),
            (3003, 'MAN', 'Mantequilla', 25, 1, 902, 'KG', 20),
            (3004, 'ESE', 'Esencia de vainilla', 8, 3, 902, 'UNIDAD', 0),
            (3010, 'PAN', 'Pan francés', 0.5, 0, 903, 'UNIDAD', 0);
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, unidad_medida, lleva_vencimiento) VALUES
            (3011, 'PAS', 'Pastel de manzana', 6, 0, 903, 'UNIDAD', 1);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'DNI', '12345678', 'María Quispe', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn pasteleria(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "PASTELERIA", MODULOS_PASTELERIA).await
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

async fn vender(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<i64, (StatusCode, String)> {
    let mut base = json!({"total": 0, "metodo_pago": "EFECTIVO", "monto_recibido": null, "cambio": null, "usuario_id": 1, "cliente_id": null});
    for (k, x) in v.as_object().unwrap() {
        base[k] = x.clone();
    }
    let venta: NuevaVenta = serde_json::from_value(base).expect("venta");
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

fn linea(id: i64, precio: f64, cantidad: f64) -> serde_json::Value {
    json!({"id": id, "nombre": format!("Producto {}", id), "precio": precio, "cantidad": cantidad, "descuentoMonto": 0})
}

async fn valor(t: &Arc<TenantDb>, sql: &str) -> libsql::Value {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    filas.next().await.unwrap().unwrap_or_else(|| panic!("sin filas: {}", sql)).get_value(0).unwrap()
}
async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    match valor(t, sql).await {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: no es número: {:?}", sql, otro),
    }
}
async fn entero(t: &Arc<TenantDb>, sql: &str) -> i64 {
    match valor(t, sql).await {
        libsql::Value::Integer(n) => n,
        otro => panic!("{}: no es entero: {:?}", sql, otro),
    }
}
async fn texto(t: &Arc<TenantDb>, sql: &str) -> String {
    match valor(t, sql).await {
        libsql::Value::Text(x) => x,
        otro => panic!("{}: no es texto: {:?}", sql, otro),
    }
}

fn manana() -> String {
    minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1)
}

async fn crear(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<encargos::Encargo, (StatusCode, String)> {
    let mut base = json!({
        "cliente_id": 1,
        "fecha_entrega": manana(),
        "hora_entrega": "16:30",
        "entrega": "RECOJO",
        "dedicatoria": "¡Feliz cumpleaños, Sofía!",
        "items": [
            {"producto_id": 2001, "cantidad": 1, "precio_unitario": 100, "detalle": "Chocolate, relleno de manjar, 20 porciones"},
            {"producto_id": 2002, "cantidad": 2, "precio_unitario": 5}
        ]
    });
    for (k, x) in v.as_object().unwrap() {
        base[k] = x.clone();
    }
    encargos::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(base).unwrap())).await.map(|Json(e)| e)
}

async fn adelanto(t: &Arc<TenantDb>, id: i64, monto: f64, metodo: &str) -> Result<encargos::Encargo, (StatusCode, String)> {
    encargos::agregar_adelanto(
        Extension(t.clone()),
        Extension(claims()),
        Path(id),
        Json(serde_json::from_value(json!({"monto": monto, "metodo_pago": metodo})).unwrap()),
    )
    .await
    .map(|Json(e)| e)
}

fn lineas_del_pedido() -> serde_json::Value {
    json!([linea(2001, 100.0, 1.0), linea(2002, 5.0, 2.0)])
}

#[tokio::test]
async fn pedido_con_adelanto_y_entrega_cobrando_el_saldo() {
    let t = pasteleria("flujo").await;
    abrir_caja(&t).await;

    // 1. Pedido de 110 con 30 de adelanto en efectivo: entra a la caja como ingreso.
    let e = crear(&t, json!({"adelanto": {"monto": 30, "metodo_pago": "EFECTIVO"}})).await.expect("pedido");
    assert_eq!(e.numero, 1);
    assert_eq!(e.cliente_nombre, "María Quispe");
    assert_eq!(e.cliente_telefono.as_deref(), Some("987654321"));
    assert_eq!(e.total, 110.0);
    assert_eq!(e.adelantado, 30.0);
    assert_eq!(e.saldo, 80.0);
    assert_eq!(e.estado, "PENDIENTE");
    assert_eq!(e.items[0].nombre, "Torta personalizada");
    assert_eq!(e.items[0].detalle.as_deref(), Some("Chocolate, relleno de manjar, 20 porciones"));
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 30.0);
    assert!(texto(&t, "SELECT motivo FROM movimientos_caja ORDER BY id DESC LIMIT 1").await.contains("Adelanto pedido N.° 1"));
    // Ni venta ni comprobante por el adelanto.
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, 0);

    // 2. Otro adelanto por Yape: no toca el efectivo de la caja.
    let e = adelanto(&t, e.id, 20.0, "YAPE").await.expect("adelanto yape");
    assert_eq!(e.adelantado, 50.0);
    assert_eq!(e.saldo, 60.0);
    assert_eq!(e.pagos.as_ref().unwrap().len(), 2);
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 30.0);

    // No se puede adelantar más de lo que falta.
    let (codigo, _) = adelanto(&t, e.id, 60.01, "EFECTIVO").await.unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);

    // 3. Preparación.
    for estado in ["EN_PREPARACION", "LISTO"] {
        let Json(x) = encargos::cambiar_estado(Extension(t.clone()), Path(e.id), Json(serde_json::from_value(json!({"estado": estado})).unwrap()))
            .await
            .unwrap();
        assert_eq!(x.estado, estado);
    }
    // ENTREGADO solo cobrándolo.
    assert!(encargos::cambiar_estado(Extension(t.clone()), Path(e.id), Json(serde_json::from_value(json!({"estado": "ENTREGADO"})).unwrap()))
        .await
        .is_err());

    // 4. Entrega: venta por el TOTAL (precio acordado 100, aunque el producto
    //    vale 80) y se cobran solo los 60 del saldo en efectivo.
    let venta = vender(
        &t,
        json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "cliente_id": 1, "monto_recibido": 100, "cambio": 40}),
    )
    .await
    .expect("entrega");
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta)).await, 110.0);
    assert_eq!(texto(&t, &format!("SELECT metodo_pago FROM ventas WHERE id = {}", venta)).await, "MIXTO");
    assert_eq!(texto(&t, &format!("SELECT pago_otro_metodo FROM ventas WHERE id = {}", venta)).await, "ADELANTO");
    assert_eq!(real(&t, &format!("SELECT pago_otro FROM ventas WHERE id = {}", venta)).await, 50.0);
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM ventas WHERE id = {}", venta)).await, 60.0);
    assert_eq!(real(&t, &format!("SELECT precio_unitario FROM detalles_venta WHERE venta_id = {} AND producto_id = 2001", venta)).await, 100.0);
    // Caja: el saldo en efectivo entra como venta en efectivo; el adelanto no se cuenta dos veces.
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 60.0);
    assert_eq!(real(&t, "SELECT ventas_tarjeta + ventas_transferencia FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);
    assert_eq!(real(&t, "SELECT total_ventas FROM cajas WHERE estado = 'ABIERTA'").await, 110.0);
    // 100 de inicio + 30 de adelanto en efectivo + 60 del saldo.
    let esperado = minimarket_backend::handlers::cajas::efectivo_en_caja(&t.0.connect().unwrap(), 1).await.unwrap();
    assert_eq!(esperado, 190.0);
    // El stock de las velas bajó; la torta (sin control de stock) no.
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 2002").await, 48.0);

    let Json(e) = encargos::obtener(Extension(t.clone()), Path(e.id)).await.unwrap();
    assert_eq!(e.estado, "ENTREGADO");
    assert_eq!(e.venta_id, Some(venta));
    assert!(e.folio_venta.is_some());

    // 5. No se entrega dos veces ni se cambia después.
    let antes = entero(&t, "SELECT COUNT(*) FROM ventas").await;
    let (codigo, _) = vender(&t, json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "monto_recibido": 100}))
        .await
        .unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, antes);
    assert_eq!(adelanto(&t, e.id, 1.0, "EFECTIVO").await.unwrap_err().0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn saldo_por_yape_y_pedido_pagado_completo() {
    let t = pasteleria("yape").await;
    abrir_caja(&t).await;

    // Saldo por Yape: va a la línea de transferencias, no al efectivo.
    let e = crear(&t, json!({"adelanto": {"monto": 40, "metodo_pago": "EFECTIVO"}})).await.unwrap();
    let venta = vender(&t, json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "metodo_pago": "YAPE"}))
        .await
        .expect("entrega por yape");
    assert_eq!(texto(&t, &format!("SELECT pago_efectivo_metodo FROM ventas WHERE id = {}", venta)).await, "YAPE");
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);
    assert_eq!(real(&t, "SELECT ventas_transferencia FROM cajas WHERE estado = 'ABIERTA'").await, 70.0);

    // Pagado completo antes de la entrega: la venta no cobra nada más.
    let e = crear(&t, json!({"adelanto": {"monto": 110, "metodo_pago": "TRANSFERENCIA"}})).await.unwrap();
    assert_eq!(e.saldo, 0.0);
    let venta = vender(&t, json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "monto_recibido": 0}))
        .await
        .expect("entrega pagada");
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM ventas WHERE id = {}", venta)).await, 0.0);
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);

    // Al entregar se puede agregar algo más (otra vela): se cobra junto al saldo.
    let e = crear(&t, json!({"adelanto": {"monto": 50, "metodo_pago": "EFECTIVO"}})).await.unwrap();
    let venta = vender(
        &t,
        json!({"productos": [linea(2001, 100.0, 1.0), linea(2002, 5.0, 3.0)], "total": 115, "encargo_id": e.id, "monto_recibido": 65}),
    )
    .await
    .expect("entrega con algo más");
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM ventas WHERE id = {}", venta)).await, 65.0);

    // Un precio que no es ni el del producto ni el acordado no pasa.
    let e = crear(&t, json!({})).await.unwrap();
    let (codigo, _) = vender(&t, json!({"productos": [linea(2001, 90.0, 1.0)], "total": 90, "encargo_id": e.id, "monto_recibido": 90}))
        .await
        .unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);
    // La venta no puede ser por menos de lo adelantado.
    let e = crear(&t, json!({"adelanto": {"monto": 100, "metodo_pago": "YAPE"}})).await.unwrap();
    let (codigo, _) = vender(&t, json!({"productos": [linea(2002, 5.0, 2.0)], "total": 10, "encargo_id": e.id, "monto_recibido": 0}))
        .await
        .unwrap_err();
    assert_eq!(codigo, StatusCode::BAD_REQUEST);
    // Ni con pago mixto ni al crédito.
    let (codigo, _) = vender(
        &t,
        json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "metodo_pago": "MIXTO", "pago_efectivo": 5, "pago_otro": 5, "pago_otro_metodo": "TARJETA"}),
    )
    .await
    .unwrap_err();
    assert_eq!(codigo, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn validaciones_edicion_y_lista() {
    let t = pasteleria("validar").await;
    abrir_caja(&t).await;
    let hoy = minimarket_backend::logica::tiempo::hoy_lima();
    let ayer = minimarket_backend::logica::tiempo::hoy_lima_mas_dias(-1);

    assert!(crear(&t, json!({"fecha_entrega": ayer})).await.unwrap_err().1.contains("ya pasó"));
    assert!(crear(&t, json!({"entrega": "DELIVERY"})).await.unwrap_err().1.contains("dirección"));
    assert!(crear(&t, json!({"hora_entrega": "25:00"})).await.is_err());
    assert!(crear(&t, json!({"items": []})).await.is_err());
    assert!(crear(&t, json!({"cliente_id": null})).await.unwrap_err().1.contains("nombre del cliente"));
    assert!(crear(&t, json!({"adelanto": {"monto": 111, "metodo_pago": "EFECTIVO"}})).await.is_err());
    assert!(crear(&t, json!({"adelanto": {"monto": 10, "metodo_pago": "CREDITO"}})).await.is_err());
    // Nada de lo rechazado quedó guardado (ni el adelanto en la caja).
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM encargos").await, 0);
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);

    // Sin cliente registrado: con nombre y teléfono escritos; para hoy, con delivery.
    let e = crear(
        &t,
        json!({"cliente_id": null, "cliente_nombre": "  Rosa  ", "cliente_telefono": "999888777", "fecha_entrega": hoy,
               "entrega": "DELIVERY", "direccion_entrega": "Av. Sol 123, Wanchaq",
               "adelanto": {"monto": 60, "metodo_pago": "EFECTIVO"}}),
    )
    .await
    .unwrap();
    assert_eq!(e.cliente_nombre, "Rosa");
    assert_eq!(e.direccion_entrega.as_deref(), Some("Av. Sol 123, Wanchaq"));

    // Editar: el total no puede quedar por debajo de lo adelantado.
    let datos = |precio: f64| {
        Json(
            serde_json::from_value::<encargos::DatosEncargo>(json!({
                "cliente_nombre": "Rosa", "fecha_entrega": hoy, "entrega": "RECOJO",
                "items": [{"producto_id": 2001, "cantidad": 1, "precio_unitario": precio}]
            }))
            .unwrap(),
        )
    };
    let (codigo, _) = encargos::actualizar(Extension(t.clone()), Path(e.id), datos(50.0)).await.unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);
    let Json(editado) = encargos::actualizar(Extension(t.clone()), Path(e.id), datos(150.0)).await.unwrap();
    assert_eq!(editado.total, 150.0);
    assert_eq!(editado.saldo, 90.0);
    assert_eq!(editado.items.len(), 1);
    assert_eq!(editado.direccion_entrega, None);

    // Lista: activos por fecha de entrega; búsqueda por nombre o número.
    crear(&t, json!({})).await.unwrap();
    let lista = |estado: &str, buscar: Option<&str>| {
        serde_json::from_value::<encargos::FiltroEncargos>(json!({"estado": estado, "buscar": buscar})).unwrap()
    };
    let Json(activos) = encargos::listar(Extension(t.clone()), Query(lista("ACTIVOS", None))).await.unwrap();
    assert_eq!(activos.len(), 2);
    assert_eq!(activos[0].fecha_entrega, hoy);
    assert!(!activos[0].items.is_empty());
    let Json(rosa) = encargos::listar(Extension(t.clone()), Query(lista("ACTIVOS", Some("ros")))).await.unwrap();
    assert_eq!(rosa.len(), 1);
    let Json(num) = encargos::listar(Extension(t.clone()), Query(lista("TODOS", Some("#2")))).await.unwrap();
    assert_eq!(num.len(), 1);
    assert_eq!(num[0].numero, 2);
    let Json(del_cliente) = encargos::listar(
        Extension(t.clone()),
        Query(serde_json::from_value(json!({"estado": "TODOS", "cliente_id": 1})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(del_cliente.len(), 1);
}

#[tokio::test]
async fn cancelar_devolviendo_o_reteniendo_el_adelanto() {
    let t = pasteleria("cancelar").await;
    abrir_caja(&t).await;
    let cancelar = |id: i64, v: serde_json::Value| {
        let t = t.clone();
        async move {
            encargos::cancelar(Extension(t), Extension(claims()), Path(id), Json(serde_json::from_value(v).unwrap()))
                .await
                .map(|Json(e)| e)
        }
    };

    // Devolución en efectivo: sale de la caja como retiro.
    let e = crear(&t, json!({"adelanto": {"monto": 40, "metodo_pago": "EFECTIVO"}})).await.unwrap();
    assert!(cancelar(e.id, json!({"motivo": "  "})).await.is_err());
    let c = cancelar(e.id, json!({"motivo": "El cliente ya no la quiere", "devolver": true})).await.unwrap();
    assert_eq!(c.estado, "CANCELADO");
    assert_eq!(c.devuelto, 40.0);
    assert_eq!(real(&t, "SELECT retiros_total FROM cajas WHERE estado = 'ABIERTA'").await, 40.0);
    assert_eq!(c.pagos.as_ref().unwrap().last().unwrap().tipo, "DEVOLUCION");
    // Ya cancelado: nada más.
    assert_eq!(cancelar(e.id, json!({"motivo": "otra vez"})).await.unwrap_err().0, StatusCode::CONFLICT);
    let (codigo, _) = vender(&t, json!({"productos": lineas_del_pedido(), "total": 110, "encargo_id": e.id, "monto_recibido": 110}))
        .await
        .unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);

    // Retenido: no sale dinero de la caja.
    let e = crear(&t, json!({"adelanto": {"monto": 30, "metodo_pago": "EFECTIVO"}})).await.unwrap();
    let c = cancelar(e.id, json!({"motivo": "No vino a recoger", "devolver": false})).await.unwrap();
    assert_eq!(c.devuelto, 0.0);
    assert_eq!(real(&t, "SELECT retiros_total FROM cajas WHERE estado = 'ABIERTA'").await, 40.0);

    // No se devuelve más efectivo del que hay en la caja.
    let e = crear(&t, json!({"adelanto": {"monto": 100, "metodo_pago": "YAPE"}})).await.unwrap();
    let conn = t.0.connect().unwrap();
    let hay = minimarket_backend::handlers::cajas::efectivo_en_caja(&conn, 1).await.unwrap();
    assert_eq!(hay, 130.0);
    conn.execute("UPDATE cajas SET retiros_total = retiros_total + 100 WHERE estado = 'ABIERTA'", ()).await.unwrap();
    let (codigo, mensaje) = cancelar(e.id, json!({"motivo": "x", "devolver": true, "metodo_devolucion": "EFECTIVO"})).await.unwrap_err();
    assert_eq!(codigo, StatusCode::BAD_REQUEST, "{}", mensaje);
    assert_eq!(texto(&t, &format!("SELECT estado FROM encargos WHERE id = {}", e.id)).await, "PENDIENTE");
    // Por Yape sí (no toca la caja).
    let c = cancelar(e.id, json!({"motivo": "x", "devolver": true, "metodo_devolucion": "YAPE"})).await.unwrap();
    assert_eq!(c.devuelto, 100.0);
}

#[tokio::test]
async fn otro_rubro_no_ve_nada_nuevo() {
    let t = base("bodega", "BODEGA", "").await;
    abrir_caja(&t).await;
    let (codigo, _) = crear(&t, json!({})).await.unwrap_err();
    assert_eq!(codigo, StatusCode::FORBIDDEN);
    let (codigo, _) = encargos::listar(Extension(t.clone()), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap_err();
    assert_eq!(codigo, StatusCode::FORBIDDEN);
    // Una venta con encargo_id tampoco pasa.
    let (codigo, _) = vender(&t, json!({"productos": [linea(2002, 5.0, 1.0)], "total": 5, "encargo_id": 1, "monto_recibido": 5}))
        .await
        .unwrap_err();
    assert_eq!(codigo, StatusCode::FORBIDDEN);
    // Su venta de siempre sigue igual.
    let venta = vender(&t, json!({"productos": [linea(2002, 5.0, 2.0)], "total": 10, "monto_recibido": 20, "cambio": 10}))
        .await
        .expect("venta normal");
    assert_eq!(texto(&t, &format!("SELECT metodo_pago FROM ventas WHERE id = {}", venta)).await, "EFECTIVO");
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 10.0);
}

#[tokio::test]
async fn rubro_pasteleria_en_el_catalogo() {
    use minimarket_backend::handlers::rubros;
    assert!(rubros::rubro_valido("PASTELERIA"));
    let modulos = rubros::modulos_del_rubro("PASTELERIA");
    assert!(modulos.iter().any(|m| m == "ENCARGOS"));
    assert!(rubros::unidades_del_rubro("PASTELERIA").unwrap().contains("PORCION"));
}


// ---------------------------------------------------------------------
// Fase 2 y 3: producción, mermas y recetas de producción
// ---------------------------------------------------------------------

use minimarket_backend::handlers::produccion;

fn cajero() -> Claims {
    Claims { rol_id: 2, ..claims() }
}

async fn receta(t: &Arc<TenantDb>, producto: i64, v: serde_json::Value, c: Claims) -> Result<produccion::Formula, (StatusCode, String)> {
    produccion::guardar_formula(Extension(t.clone()), Extension(c), Path(producto), Json(serde_json::from_value(v).unwrap()))
        .await
        .map(|Json(f)| f)
}

async fn producir(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<produccion::Produccion, (StatusCode, String)> {
    produccion::registrar_produccion(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(v).unwrap()))
        .await
        .map(|Json(p)| p)
}

async fn merma(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<produccion::Merma, (StatusCode, String)> {
    produccion::registrar_merma(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(v).unwrap()))
        .await
        .map(|Json(m)| m)
}

async fn stock(t: &Arc<TenantDb>, id: i64) -> f64 {
    real(t, &format!("SELECT stock FROM productos WHERE id = {}", id)).await
}

fn hoy() -> Query<produccion::Rango> {
    Query(serde_json::from_value(json!({})).unwrap())
}

const RECETA_PAN: &str = r#"{"rinde": 50, "notas": "Hornear 20 min a 180°", "insumos": [
    {"insumo_id": 3001, "cantidad": 2}, {"insumo_id": 3002, "cantidad": 4}, {"insumo_id": 3003, "cantidad": 0.25}]}"#;

#[tokio::test]
async fn receta_de_produccion_con_costo_y_margen() {
    let t = pasteleria("receta").await;
    let pan: serde_json::Value = serde_json::from_str(RECETA_PAN).unwrap();
    // Solo el administrador guarda recetas.
    assert_eq!(receta(&t, 3010, pan.clone(), cajero()).await.unwrap_err().0, StatusCode::FORBIDDEN);
    let f = receta(&t, 3010, pan.clone(), claims()).await.expect("receta");
    // 2 kg × 3.50 + 4 × 0.50 + 0.25 kg × 20 = 14 por 50 panes.
    assert_eq!(f.costo_total, Some(14.0));
    assert_eq!(f.costo_unitario, Some(0.28));
    assert_eq!(f.margen, Some(0.22));
    assert_eq!(f.margen_porcentaje, Some(44.0));
    assert_eq!(f.insumos.len(), 3);
    assert_eq!(f.notas.as_deref(), Some("Hornear 20 min a 180°"));

    // Un insumo sin costo (sin precio de compra): no se puede calcular.
    let f = receta(&t, 3011, json!({"rinde": 8, "insumos": [{"insumo_id": 3001, "cantidad": 1}, {"insumo_id": 3004, "cantidad": 1}]}), claims())
        .await
        .unwrap();
    assert_eq!(f.costo_total, None);
    assert_eq!(f.sin_costo, vec!["Esencia de vainilla".to_string()]);

    // Errores.
    assert!(receta(&t, 3010, json!({"rinde": 0, "insumos": [{"insumo_id": 3001, "cantidad": 1}]}), claims()).await.is_err());
    assert!(receta(&t, 3010, json!({"rinde": 10, "insumos": []}), claims()).await.is_err());
    assert!(receta(&t, 3010, json!({"rinde": 10, "insumos": [{"insumo_id": 3010, "cantidad": 1}]}), claims()).await.is_err());
    assert!(receta(&t, 3010, json!({"rinde": 10, "insumos": [{"insumo_id": 99999, "cantidad": 1}]}), claims()).await.is_err());
    // El mismo insumo dos veces se suma; guardar reemplaza la receta.
    let f = receta(&t, 3010, json!({"rinde": 10, "insumos": [{"insumo_id": 3001, "cantidad": 1}, {"insumo_id": 3001, "cantidad": 1}]}), claims())
        .await
        .unwrap();
    assert_eq!(f.insumos.len(), 1);
    assert_eq!(f.insumos[0].cantidad, 2.0);

    let Json(todas) = produccion::formulas(Extension(t.clone())).await.unwrap();
    assert_eq!(todas.len(), 2);
    produccion::borrar_formula(Extension(t.clone()), Extension(claims()), Path(3011)).await.unwrap();
    assert!(produccion::formula(Extension(t.clone()), Path(3011)).await.is_err());
}

#[tokio::test]
async fn produccion_descuenta_insumos_y_calcula_el_costo() {
    let t = pasteleria("producir").await;
    receta(&t, 3010, serde_json::from_str(RECETA_PAN).unwrap(), claims()).await.unwrap();

    // 100 panes: dos tandas de la receta.
    let p = producir(&t, json!({"producto_id": 3010, "cantidad": 100})).await.expect("producción");
    assert!(p.con_receta);
    assert_eq!(p.costo_total, Some(28.0));
    assert_eq!(p.costo_unitario, Some(0.28));
    assert_eq!(p.insumos.len(), 3);
    assert_eq!(stock(&t, 3010).await, 100.0);
    assert_eq!(stock(&t, 3001).await, 46.0);
    assert_eq!(stock(&t, 3002).await, 92.0);
    assert_eq!(stock(&t, 3003).await, 0.5);
    // Costo promedio del pan y el kárdex.
    assert_eq!(real(&t, "SELECT costo_promedio FROM productos WHERE id = 3010").await, 0.28);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM movimientos_inventario WHERE tipo_movimiento = 'SALIDA'").await, 3);
    assert_eq!(texto(&t, "SELECT motivo FROM movimientos_inventario WHERE producto_id = 3010").await, "Producción");
    // No es una compra ni una venta.
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, 0);

    // 300 panes piden 1.5 kg de mantequilla y quedan 0.5: no pasa, y nada cambia.
    let (codigo, mensaje) = producir(&t, json!({"producto_id": 3010, "cantidad": 300})).await.unwrap_err();
    assert_eq!(codigo, StatusCode::CONFLICT);
    assert!(mensaje.contains("Mantequilla"), "{}", mensaje);
    assert_eq!(stock(&t, 3010).await, 100.0);
    assert_eq!(stock(&t, 3001).await, 46.0);
    // Registrado igual: se descuenta lo que hay y queda el faltante anotado.
    let p = producir(&t, json!({"producto_id": 3010, "cantidad": 300, "forzar": true})).await.unwrap();
    assert_eq!(stock(&t, 3003).await, 0.0);
    let mant = p.insumos.iter().find(|i| i.insumo_id == 3003).unwrap();
    assert_eq!((mant.cantidad, mant.faltante), (0.5, 1.0));
    assert_eq!(stock(&t, 3010).await, 400.0);

    // Sin usar la receta: solo sube el producto.
    let p = producir(&t, json!({"producto_id": 3010, "cantidad": 10, "usar_receta": false})).await.unwrap();
    assert!(!p.con_receta);
    assert_eq!(p.costo_total, None);
    assert_eq!(stock(&t, 3001).await, 34.0);

    // Producto sin control de stock y cantidades malas.
    assert!(producir(&t, json!({"producto_id": 2001, "cantidad": 1})).await.unwrap_err().1.contains("sin controlar stock"));
    assert!(producir(&t, json!({"producto_id": 3010, "cantidad": 0})).await.is_err());

    // Con vencimiento: pide la fecha y entra como lote.
    assert!(producir(&t, json!({"producto_id": 3011, "cantidad": 8})).await.unwrap_err().1.contains("vencimiento"));
    let p = producir(&t, json!({"producto_id": 3011, "cantidad": 8, "fecha_vencimiento": manana()})).await.unwrap();
    assert_eq!(stock(&t, 3011).await, 8.0);
    assert_eq!(texto(&t, "SELECT numero_lote FROM lotes_producto WHERE producto_id = 3011").await, format!("PROD-{}", p.id));

    let Json(lista) = produccion::listar_produccion(Extension(t.clone()), hoy()).await.unwrap();
    assert_eq!(lista.len(), 4);
}

#[tokio::test]
async fn mermas_anulaciones_y_resumen_del_dia() {
    let t = pasteleria("mermas").await;
    abrir_caja(&t).await;
    receta(&t, 3010, serde_json::from_str(RECETA_PAN).unwrap(), claims()).await.unwrap();
    let p = producir(&t, json!({"producto_id": 3010, "cantidad": 100})).await.unwrap();

    // Se venden 30 panes.
    vender(&t, json!({"productos": [linea(3010, 0.5, 30.0)], "total": 15, "monto_recibido": 20, "cambio": 5})).await.expect("venta");
    assert_eq!(stock(&t, 3010).await, 70.0);

    // Merma: 20 que no se vendieron, al costo del pan.
    let m = merma(&t, json!({"producto_id": 3010, "cantidad": 20, "motivo": "NO_VENDIDO", "nota": "Fin del día"})).await.unwrap();
    assert_eq!(m.costo_unitario, Some(0.28));
    assert_eq!(m.costo_total, Some(5.6));
    assert_eq!(stock(&t, 3010).await, 50.0);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM movimientos_inventario WHERE tipo_movimiento = 'MERMA'").await, 1);
    // Más de lo que hay, motivo malo, producto sin stock que controlar.
    assert_eq!(merma(&t, json!({"producto_id": 3010, "cantidad": 51, "motivo": "MALOGRADO"})).await.unwrap_err().0, StatusCode::CONFLICT);
    assert!(merma(&t, json!({"producto_id": 3010, "cantidad": 1, "motivo": "ROBO"})).await.is_err());
    assert!(merma(&t, json!({"producto_id": 2001, "cantidad": 1, "motivo": "OTRO"})).await.is_err());

    // Resumen del día.
    let Json(r) = produccion::resumen(Extension(t.clone()), hoy()).await.unwrap();
    let pan = r.filas.iter().find(|f| f.producto_id == 3010).unwrap();
    assert_eq!((pan.producido, pan.vendido, pan.merma, pan.stock), (100.0, 30.0, 20.0, 50.0));
    assert_eq!(pan.costo_produccion, Some(28.0));
    assert_eq!(r.costo_merma, 5.6);

    // Anular la merma: el pan vuelve.
    let anular_merma = |id| produccion::anular_merma(Extension(t.clone()), Extension(claims()), Path(id));
    assert_eq!(
        produccion::anular_merma(Extension(t.clone()), Extension(cajero()), Path(m.id)).await.unwrap_err().0,
        StatusCode::FORBIDDEN
    );
    let Json(m) = anular_merma(m.id).await.unwrap();
    assert_eq!(m.estado, "ANULADA");
    assert_eq!(stock(&t, 3010).await, 70.0);
    assert!(anular_merma(m.id).await.is_err());

    // Anular la producción: ya se vendieron 30 de los 100, no se puede.
    let anular = |id| produccion::anular_produccion(Extension(t.clone()), Extension(claims()), Path(id));
    assert_eq!(anular(p.id).await.unwrap_err().0, StatusCode::CONFLICT);
    // Otra producción recién hecha sí: el pan sale y los insumos vuelven.
    let antes = (stock(&t, 3001).await, stock(&t, 3002).await, stock(&t, 3003).await);
    let p2 = producir(&t, json!({"producto_id": 3010, "cantidad": 50})).await.unwrap();
    assert_eq!(stock(&t, 3010).await, 120.0);
    let Json(p2) = anular(p2.id).await.unwrap();
    assert_eq!(p2.estado, "ANULADA");
    assert_eq!(stock(&t, 3010).await, 70.0);
    assert_eq!((stock(&t, 3001).await, stock(&t, 3002).await, stock(&t, 3003).await), antes);
    let Json(r) = produccion::resumen(Extension(t.clone()), hoy()).await.unwrap();
    assert_eq!(r.filas.iter().find(|f| f.producto_id == 3010).unwrap().producido, 100.0);

    // Con vencimiento: la merma sale primero del lote que vence antes (aunque ya venció).
    let conn = t.0.connect().unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento) VALUES (3011, 3, '{}');",
        minimarket_backend::logica::tiempo::hoy_lima_mas_dias(-1)
    ))
    .await
    .unwrap();
    let pp = producir(&t, json!({"producto_id": 3011, "cantidad": 8, "fecha_vencimiento": manana()})).await.unwrap();
    assert_eq!(stock(&t, 3011).await, 11.0);
    let m = merma(&t, json!({"producto_id": 3011, "cantidad": 4, "motivo": "VENCIDO"})).await.unwrap();
    assert_eq!(real(&t, "SELECT cantidad FROM lotes_producto WHERE producto_id = 3011 ORDER BY fecha_vencimiento LIMIT 1").await, 0.0);
    assert_eq!(stock(&t, 3011).await, 7.0);
    // Anulada, cada lote recupera lo suyo.
    let _ = anular_merma(m.id).await.unwrap();
    assert_eq!(stock(&t, 3011).await, 11.0);
    assert_eq!(real(&t, "SELECT cantidad FROM lotes_producto WHERE producto_id = 3011 ORDER BY fecha_vencimiento LIMIT 1").await, 3.0);
    // La producción con lote se anula sacando ese lote.
    let _ = anular(pp.id).await.unwrap();
    assert_eq!(stock(&t, 3011).await, 3.0);
}

#[tokio::test]
async fn produccion_otro_rubro_y_dependencias() {
    let t = base("prod-bodega", "BODEGA", "").await;
    let (codigo, _) = producir(&t, json!({"producto_id": 2002, "cantidad": 1})).await.unwrap_err();
    assert_eq!(codigo, StatusCode::FORBIDDEN);
    assert_eq!(merma(&t, json!({"producto_id": 2002, "cantidad": 1, "motivo": "OTRO"})).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(produccion::formulas(Extension(t.clone())).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(stock(&t, 2002).await, 50.0);

    // Solo producción (sin recetas): produce sin tocar insumos aunque haya receta guardada.
    let t = base("prod-sola", "PASTELERIA", "PRODUCCION").await;
    assert_eq!(receta(&t, 3010, serde_json::from_str(RECETA_PAN).unwrap(), claims()).await.unwrap_err().0, StatusCode::FORBIDDEN);
    let p = producir(&t, json!({"producto_id": 3010, "cantidad": 10})).await.unwrap();
    assert!(!p.con_receta);
    assert_eq!(stock(&t, 3001).await, 50.0);

    // Recetas necesita Producción encendida.
    use minimarket_backend::handlers::rubros;
    assert_eq!(rubros::faltante(&["INSUMOS".to_string()]), Some(("INSUMOS", "PRODUCCION")));
    assert!(rubros::modulos_del_rubro("PASTELERIA").iter().any(|m| m == "PRODUCCION"));
}
