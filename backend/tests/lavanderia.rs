//! Rubro Lavandería: el módulo ENCARGOS usado como órdenes de lavado,
//! contra una base local (schema.sql + todas las migraciones) y los
//! handlers de verdad. Revisa el catálogo del rubro (módulos y la unidad
//! Prenda) y una orden con kilos y prendas: el mismo servicio con precios
//! distintos puestos al recibir la ropa, delivery sin adelanto y la entrega
//! con el detalle de cada prenda en la boleta. Y el cierre de mes: entregar
//! al crédito (lo adelantado en la orden cuenta como pagado y no entra dos
//! veces a la caja), el estado de cuenta del mes y un pago que cubre varias
//! cuentas desde la más antigua.
//!
//!   cargo test --test lavanderia

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{creditos, encargos, rubros, unidades, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("lavanderia_{}_{}.db", nombre, std::process::id()));
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
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'LAVANDERIA PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    // Los servicios de una lavandería: sin stock, por kilo o por prenda.
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Lavado');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, unidad_medida) VALUES
            (4001, 'LAV-KG', 'Lavado por kilo', 5, 0, 900, 0, 'KG'),
            (4002, 'LAV-SECO', 'Lavado al seco', 20, 0, 900, 0, 'PRENDA'),
            (4003, 'PLANCHADO', 'Planchado', 3, 0, 900, 0, 'PRENDA');
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'DNI', '12345678', 'Ana Huamán', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn lavanderia(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "LAVANDERIA", &rubros::modulos_del_rubro("LAVANDERIA").join(",")).await
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
async fn texto(t: &Arc<TenantDb>, sql: &str) -> String {
    match valor(t, sql).await {
        libsql::Value::Text(x) => x,
        otro => panic!("{}: no es texto: {:?}", sql, otro),
    }
}

#[test]
fn catalogo_del_rubro() {
    assert!(rubros::rubro_valido("LAVANDERIA"));
    assert_eq!(rubros::modulos_del_rubro("LAVANDERIA"), vec!["SERVICIOS", "MEDIDAS", "ENCARGOS", "CREDITO"]);
    let unidades_rubro = rubros::unidades_del_rubro("LAVANDERIA").unwrap();
    for u in ["UNIDAD", "KG", "PRENDA"] {
        assert!(unidades_rubro.split(',').any(|x| x == u), "{u}");
    }
    // Todas las unidades del rubro existen en el catálogo.
    for u in unidades_rubro.split(',') {
        assert!(unidades::es_valida(u), "{u}");
    }
    // Nada que no sea de lavandería (ni producción ni insumos de pastelería).
    assert!(!rubros::modulos_del_rubro("LAVANDERIA").iter().any(|m| m == "PRODUCCION" || m == "INSUMOS"));
}

#[tokio::test]
async fn orden_por_kilo_y_por_prenda_con_el_detalle_en_la_boleta() {
    let t = lavanderia("flujo").await;
    abrir_caja(&t).await;

    // 1. La ropa llega: 4.5 kg de ropa de color, un terno y una casaca al
    //    seco con precios distintos (se ponen al recibirlas), y un planchado.
    //    Delivery y sin adelanto (lo más común: se paga al recoger).
    let Json(e) = encargos::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(
            serde_json::from_value(json!({
                "cliente_id": 1,
                "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(2),
                "entrega": "DELIVERY",
                "direccion_entrega": "Av. La Cultura 123, Wanchaq",
                "notas": "Llamar antes de llegar",
                "items": [
                    {"producto_id": 4001, "cantidad": 4.5, "precio_unitario": 5, "detalle": "Ropa de color"},
                    {"producto_id": 4002, "cantidad": 1, "precio_unitario": 25, "detalle": "Terno 2 piezas, lana, azul marino"},
                    {"producto_id": 4002, "cantidad": 1, "precio_unitario": 30, "detalle": "Casaca de cuero, mancha en la manga"},
                    {"producto_id": 4003, "cantidad": 2, "precio_unitario": 3}
                ]
            }))
            .unwrap(),
        ),
    )
    .await
    .expect("orden de lavado");
    assert_eq!(e.total, 83.5);
    assert_eq!(e.adelantado, 0.0);
    assert_eq!(e.saldo, 83.5);
    assert_eq!(e.items.len(), 4);
    assert_eq!(e.items[0].cantidad, 4.5);
    assert_eq!(e.items[0].unidad_medida, "KG");
    assert_eq!(e.items[1].unidad_medida, "PRENDA");
    assert_eq!(e.items[2].detalle.as_deref(), Some("Casaca de cuero, mancha en la manga"));
    // Sin adelanto no entra nada a la caja ni hay venta.
    assert_eq!(real(&t, "SELECT COUNT(*) FROM ventas").await, 0.0);

    // 2. En lavado → lista para recoger.
    for estado in ["EN_PREPARACION", "LISTO"] {
        let Json(x) = encargos::cambiar_estado(Extension(t.clone()), Path(e.id), Json(serde_json::from_value(json!({"estado": estado})).unwrap()))
            .await
            .unwrap();
        assert_eq!(x.estado, estado);
    }

    // 3. Entrega: el punto de venta manda cada línea con su detalle
    //    (POS.jsx, carritoDeEncargo) y se cobra todo en efectivo.
    let venta = vender(
        &t,
        json!({
            "productos": [
                {"id": 4001, "nombre": "Lavado por kilo", "precio": 5, "cantidad": 4.5, "descuentoMonto": 0, "detalle": "Ropa de color"},
                {"id": 4002, "nombre": "Lavado al seco", "precio": 25, "cantidad": 1, "descuentoMonto": 0, "detalle": "Terno 2 piezas, lana, azul marino"},
                {"id": 4002, "nombre": "Lavado al seco", "precio": 30, "cantidad": 1, "descuentoMonto": 0, "detalle": "Casaca de cuero, mancha en la manga"},
                {"id": 4003, "nombre": "Planchado", "precio": 3, "cantidad": 2, "descuentoMonto": 0}
            ],
            "total": 83.5, "encargo_id": e.id, "cliente_id": 1, "monto_recibido": 100, "cambio": 16.5
        }),
    )
    .await
    .expect("entrega");
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta)).await, 83.5);
    assert_eq!(real(&t, &format!("SELECT cantidad FROM detalles_venta WHERE venta_id = {} AND producto_id = 4001", venta)).await, 4.5);
    // El detalle va en el nombre de la línea (boleta y ticket).
    assert_eq!(
        texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND precio_unitario = 25", venta)).await,
        "Lavado al seco (Terno 2 piezas, lana, azul marino)"
    );
    assert_eq!(
        texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND producto_id = 4003", venta)).await,
        "Planchado"
    );
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 83.5);

    let Json(e) = encargos::obtener(Extension(t.clone()), Path(e.id)).await.unwrap();
    assert_eq!(e.estado, "ENTREGADO");
    assert_eq!(e.venta_id, Some(venta));
    // Se cobró al entregar: el ticket dice PAGADO, no saldo.
    assert!(!e.a_cuenta);
}

#[tokio::test]
async fn otro_rubro_sin_ordenes_de_lavado() {
    // Una bodega no tiene el módulo: las órdenes responden 403.
    let t = base("bodega", "BODEGA", "").await;
    let Err((codigo, _)) = encargos::obtener(Extension(t.clone()), Path(1)).await else {
        panic!("una bodega no debe ver órdenes de lavado");
    };
    assert_eq!(codigo, StatusCode::FORBIDDEN);
}

async fn orden(t: &Arc<TenantDb>, adelanto: Option<f64>) -> encargos::Encargo {
    let mut cuerpo = json!({
        "cliente_id": 1,
        "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1),
        "entrega": "RECOJO",
        "items": [
            {"producto_id": 4001, "cantidad": 4.5, "precio_unitario": 5, "detalle": "Sábanas del hotel"},
            {"producto_id": 4002, "cantidad": 1, "precio_unitario": 25, "detalle": "Terno, lana, azul"}
        ]
    });
    if let Some(m) = adelanto {
        cuerpo["adelanto"] = json!({"monto": m, "metodo_pago": "EFECTIVO"});
    }
    let Json(e) = encargos::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(cuerpo).unwrap()))
        .await
        .expect("orden");
    e
}

fn lineas_orden() -> serde_json::Value {
    json!([
        {"id": 4001, "nombre": "Lavado por kilo", "precio": 5, "cantidad": 4.5, "descuentoMonto": 0, "detalle": "Sábanas del hotel"},
        {"id": 4002, "nombre": "Lavado al seco", "precio": 25, "cantidad": 1, "descuentoMonto": 0, "detalle": "Terno, lana, azul"}
    ])
}

#[tokio::test]
async fn cierre_de_mes_entrega_a_cuenta_estado_y_pago_de_varias_cuentas() {
    let t = lavanderia("cierre").await;
    abrir_caja(&t).await;

    // 1. Orden de 47.50 con 10 de adelanto en efectivo (entra a la caja ese día).
    let e = orden(&t, Some(10.0)).await;
    assert_eq!(e.total, 47.5);
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 10.0);

    // No se puede dejar a cuenta si con lo adelantado ya no queda saldo.
    let (codigo, _) = vender(
        &t,
        json!({"productos": lineas_orden(), "total": 47.5, "encargo_id": e.id, "cliente_id": 1, "credito": {"adelanto": 37.5}}),
    )
    .await
    .unwrap_err();
    assert_eq!(codigo, StatusCode::BAD_REQUEST);

    // 2. Se entrega A CUENTA (nota de venta, sin comprobante): debe 37.50.
    let venta1 = vender(
        &t,
        json!({"productos": lineas_orden(), "total": 47.5, "encargo_id": e.id, "cliente_id": 1, "credito": {"adelanto": 0, "dias": 30}}),
    )
    .await
    .expect("entrega a cuenta");
    assert_eq!(texto(&t, &format!("SELECT pago_otro_metodo FROM ventas WHERE id = {}", venta1)).await, "CREDITO");
    assert_eq!(real(&t, &format!("SELECT total FROM creditos WHERE venta_id = {}", venta1)).await, 47.5);
    assert_eq!(real(&t, &format!("SELECT abonado FROM creditos WHERE venta_id = {}", venta1)).await, 10.0);
    assert_eq!(
        texto(&t, &format!("SELECT a.metodo_pago FROM credito_abonos a JOIN creditos c ON c.id = a.credito_id WHERE c.venta_id = {}", venta1)).await,
        "ADELANTO"
    );
    // El adelanto no entra otra vez a la caja ni se cuenta como venta en efectivo.
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 10.0);
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);
    let Json(e) = encargos::obtener(Extension(t.clone()), Path(e.id)).await.unwrap();
    assert_eq!(e.estado, "ENTREGADO");
    // Entregada al crédito: el ticket dice que quedó a cuenta.
    assert!(e.a_cuenta);

    // 3. Otra orden del mismo cliente, sin adelanto, también a cuenta: 47.50 más.
    let e2 = orden(&t, None).await;
    let venta2 = vender(
        &t,
        json!({"productos": lineas_orden(), "total": 47.5, "encargo_id": e2.id, "cliente_id": 1, "credito": {"adelanto": 0}}),
    )
    .await
    .expect("segunda entrega a cuenta");

    // 4. Estado de cuenta del mes: las dos órdenes con sus prendas; debe 85.
    let hoy = minimarket_backend::logica::tiempo::hoy_lima();
    let periodo: creditos::PeriodoEstado =
        serde_json::from_value(json!({"desde": format!("{}-01", &hoy[..7]), "hasta": hoy})).unwrap();
    let Json(estado) = creditos::estado_cuenta(Extension(t.clone()), Path(1), Query(periodo)).await.expect("estado de cuenta");
    assert_eq!(estado.cliente_nombre, "Ana Huamán");
    assert_eq!(estado.creditos.len(), 2);
    assert_eq!(estado.total, 95.0);
    assert_eq!(estado.pagado, 10.0);
    assert_eq!(estado.saldo, 85.0);
    assert_eq!(estado.saldo_total, 85.0);
    assert_eq!(estado.saldo_anterior, 0.0);
    assert_eq!(estado.creditos[0].venta_id, venta1);
    assert!(estado.creditos[0].productos.iter().any(|p| p.nombre == "Lavado al seco (Terno, lana, azul)"));
    // Sin fechas: todo lo pendiente.
    let Json(pendiente) = creditos::estado_cuenta(Extension(t.clone()), Path(1), Query(serde_json::from_value(json!({})).unwrap()))
        .await
        .unwrap();
    assert_eq!(pendiente.creditos.len(), 2);
    // Fechas al revés o una sola: error.
    let malo: creditos::PeriodoEstado = serde_json::from_value(json!({"desde": hoy})).unwrap();
    assert!(creditos::estado_cuenta(Extension(t.clone()), Path(1), Query(malo)).await.is_err());

    // 5. Paga más de lo que debe: no se registra nada.
    let pago = |monto: f64| -> creditos::NuevoAbono {
        serde_json::from_value(json!({"monto": monto, "metodo_pago": "EFECTIVO", "nota": "Pago de octubre"})).unwrap()
    };
    assert!(creditos::pagar_cliente(Extension(t.clone()), Extension(claims()), Path(1), Json(pago(85.01))).await.is_err());
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 10.0);

    // 6. Paga 50 en efectivo: cubre la primera orden (37.50) y 12.50 de la segunda.
    let Json(r) = creditos::pagar_cliente(Extension(t.clone()), Extension(claims()), Path(1), Json(pago(50.0))).await.expect("pago");
    assert_eq!(r.pagado, 50.0);
    assert_eq!(r.aplicado.len(), 2);
    assert_eq!(r.aplicado[0].monto, 37.5);
    assert_eq!(r.aplicado[0].saldo_restante, 0.0);
    assert_eq!(r.aplicado[1].monto, 12.5);
    assert_eq!(r.saldo_total, 35.0);
    assert_eq!(texto(&t, &format!("SELECT estado FROM creditos WHERE venta_id = {}", venta1)).await, "PAGADO");
    assert_eq!(texto(&t, &format!("SELECT estado FROM creditos WHERE venta_id = {}", venta2)).await, "PENDIENTE");
    // Entró a la caja como ingreso (10 del adelanto + 50 del pago).
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 60.0);

    // 7. Paga el resto por Yape: queda al día y no toca el efectivo.
    let yape: creditos::NuevoAbono = serde_json::from_value(json!({"monto": 35, "metodo_pago": "YAPE"})).unwrap();
    let Json(r) = creditos::pagar_cliente(Extension(t.clone()), Extension(claims()), Path(1), Json(yape)).await.expect("resto");
    assert_eq!(r.saldo_total, 0.0);
    assert_eq!(real(&t, "SELECT ingresos_total FROM cajas WHERE estado = 'ABIERTA'").await, 60.0);
    let sin_deuda: creditos::NuevoAbono = serde_json::from_value(json!({"monto": 1, "metodo_pago": "EFECTIVO"})).unwrap();
    let Err((codigo, _)) = creditos::pagar_cliente(Extension(t.clone()), Extension(claims()), Path(1), Json(sin_deuda)).await else {
        panic!("sin deuda no se paga");
    };
    assert_eq!(codigo, StatusCode::CONFLICT);
}

#[tokio::test]
async fn prendas_escritas_en_la_orden_sin_llenar_servicios() {
    let t = lavanderia("escritas").await;
    abrir_caja(&t).await;
    let productos_antes = real(&t, "SELECT COUNT(*) FROM productos").await;

    // 1. Cliente nuevo: un servicio de la lista y tres prendas escritas en la
    //    misma orden (dos por prenda y una por kilo), cada una con su precio.
    let Json(e) = encargos::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(
            serde_json::from_value(json!({
                "cliente_nombre": "Rosa Quispe",
                "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1),
                "items": [
                    {"producto_id": 4003, "cantidad": 1, "precio_unitario": 3},
                    {"cobro": "PRENDA", "nombre": "Lavado de terno negro", "cantidad": 1, "precio_unitario": 28, "detalle": "Mancha en la solapa"},
                    {"cobro": "PRENDA", "nombre": "Planchado de camisa", "cantidad": 3, "precio_unitario": 2.5},
                    {"cobro": "KG", "nombre": "Ropa de cama", "cantidad": 3.2, "precio_unitario": 6}
                ]
            }))
            .unwrap(),
        ),
    )
    .await
    .expect("orden con prendas escritas");
    assert_eq!(e.total, 57.7);
    assert_eq!(e.items[1].nombre, "Lavado de terno negro");
    assert!(e.items[1].libre && !e.items[0].libre);
    assert_eq!(e.items[1].unidad_medida, "PRENDA");
    assert_eq!(e.items[3].unidad_medida, "KG");
    assert!(!e.items[3].controla_stock);
    // Solo se crean los dos servicios de respaldo, una vez.
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos").await, productos_antes + 2.0);

    // 2. Otra orden con otra prenda escrita: no se crea nada más.
    let Json(otra) = encargos::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(
            serde_json::from_value(json!({
                "cliente_nombre": "Luis",
                "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1),
                "items": [{"cobro": "PRENDA", "nombre": "Edredón 2 plazas", "cantidad": 1, "precio_unitario": 35}]
            }))
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos").await, productos_antes + 2.0);
    assert_eq!(otra.items[0].producto_id, e.items[1].producto_id);

    // Sin nombre o sin cobro no entra.
    for item in [
        json!({"cobro": "PRENDA", "nombre": "  ", "cantidad": 1, "precio_unitario": 5}),
        json!({"cobro": "LITRO", "nombre": "Casaca", "cantidad": 1, "precio_unitario": 5}),
    ] {
        let r = encargos::crear(
            Extension(t.clone()),
            Extension(claims()),
            Json(serde_json::from_value(json!({"cliente_nombre": "X", "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1), "items": [item]})).unwrap()),
        )
        .await;
        assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    }

    // 3. Editar la orden: la prenda escrita vuelve con su respaldo y un
    //    nombre corregido.
    let respaldo_prenda = e.items[1].producto_id;
    let respaldo_kg = e.items[3].producto_id;
    let Json(e) = encargos::actualizar(
        Extension(t.clone()),
        Path(e.id),
        Json(
            serde_json::from_value(json!({
                "cliente_nombre": "Rosa Quispe",
                "fecha_entrega": e.fecha_entrega,
                "items": [
                    {"producto_id": 4003, "cantidad": 1, "precio_unitario": 3},
                    {"producto_id": respaldo_prenda, "nombre": "Lavado de terno negro 3 piezas", "cantidad": 1, "precio_unitario": 30, "detalle": "Mancha en la solapa"},
                    {"producto_id": respaldo_prenda, "nombre": "Planchado de camisa", "cantidad": 3, "precio_unitario": 2.5},
                    {"producto_id": respaldo_kg, "nombre": "Ropa de cama", "cantidad": 3.2, "precio_unitario": 6}
                ]
            }))
            .unwrap(),
        ),
    )
    .await
    .expect("editar");
    assert_eq!(e.total, 59.7);
    assert_eq!(e.items[1].nombre, "Lavado de terno negro 3 piezas");

    // 4. Entrega: en la venta (boleta / nota de venta) sale lo escrito, no
    //    el nombre del respaldo. El POS manda lo escrito como detalle.
    let venta = vender(
        &t,
        json!({
            "productos": [
                {"id": 4003, "nombre": "Planchado", "precio": 3, "cantidad": 1, "descuentoMonto": 0},
                {"id": respaldo_prenda, "nombre": "Lavado de terno negro 3 piezas", "precio": 30, "cantidad": 1, "descuentoMonto": 0, "detalle": "Mancha en la solapa"},
                {"id": respaldo_prenda, "nombre": "Planchado de camisa", "precio": 2.5, "cantidad": 3, "descuentoMonto": 0},
                {"id": respaldo_kg, "nombre": "Ropa de cama", "precio": 6, "cantidad": 3.2, "descuentoMonto": 0}
            ],
            "total": 59.7, "encargo_id": e.id, "monto_recibido": 60, "cambio": 0.3
        }),
    )
    .await
    .expect("entrega con prendas escritas");
    assert_eq!(
        texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND precio_unitario = 30", venta)).await,
        "Lavado de terno negro 3 piezas (Mancha en la solapa)"
    );
    assert_eq!(
        texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND producto_id = {}", venta, respaldo_kg)).await,
        "Ropa de cama"
    );
    // Un nombre que no está en la orden no se acepta: sale el del respaldo.
    let Json(x) = encargos::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"cliente_nombre": "Y", "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1),
            "items": [{"cobro": "PRENDA", "nombre": "Casaca", "cantidad": 1, "precio_unitario": 12}]})).unwrap()),
    )
    .await
    .unwrap();
    let v2 = vender(&t, json!({"productos": [{"id": respaldo_prenda, "nombre": "Otra cosa", "precio": 12, "cantidad": 1, "descuentoMonto": 0}],
        "total": 12, "encargo_id": x.id, "monto_recibido": 12, "cambio": 0})).await.unwrap();
    assert_eq!(texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {}", v2)).await, "Servicio por prenda");
    // Un servicio de la lista sigue saliendo como siempre.
    assert_eq!(texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND producto_id = 4003", venta)).await, "Planchado");
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta)).await, 59.7);
}

#[tokio::test]
async fn pasteleria_no_puede_escribir_prendas() {
    // Otro rubro con pedidos por encargo: sigue exigiendo un producto.
    let t = base("pasteleria", "PASTELERIA", &rubros::modulos_del_rubro("PASTELERIA").join(",")).await;
    let r = encargos::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(
            serde_json::from_value(json!({
                "cliente_nombre": "X",
                "fecha_entrega": minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1),
                "items": [{"cobro": "PRENDA", "nombre": "Torta", "cantidad": 1, "precio_unitario": 50}]
            }))
            .unwrap(),
        ),
    )
    .await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos WHERE codigo LIKE '\\_LAV%' ESCAPE '\\'").await, 0.0);
}
