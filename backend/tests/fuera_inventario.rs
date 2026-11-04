//! Módulo VENTA_FUERA_INVENTARIO contra una base local (schema.sql + todas
//! las migraciones) y los handlers de verdad.
//!
//! Lo de afuera solo existe en el comprobante: la venta (y por eso la caja,
//! reportes, ganancias, stock y créditos) guarda solo lo del inventario. Un
//! negocio sin el módulo no puede mandarlo y vende igual que siempre.
//!
//!   cargo test --test fuera_inventario

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{comprobantes, devoluciones, facturacion, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::{json, Value};

const CON_MODULO: &str = "SERVICIOS,CREDITO,VENTA_TABLA,TRES_PRECIOS,VENTA_FUERA_INVENTARIO";
const SIN_MODULO: &str = "SERVICIOS,CREDITO,VENTA_TABLA,TRES_PRECIOS";

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("fuera_inventario_{}_{}.db", nombre, std::process::id()));
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
    // La 0059 se puede volver a correr (el aplicador real puede reintentar).
    conn.execute_batch(include_str!("../migraciones/0059_venta_fuera_inventario.sql")).await.expect("0059 otra vez");
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'LUBRICENTRO', modulos = ?1, nombre_tienda = 'REPUESTOS PRUEBA',
            ruc = '20161515648', razon_social = 'EMPRESA DE PRUEBA S.A.C.', direccion = 'JR. MONTEVIDEO 752', ubigeo = '150101',
            departamento = 'LIMA', provincia = 'LIMA', distrito = 'LIMA',
            serie_boleta = 'BT01', serie_factura = 'FT01', facturacion_proveedor = 'SUNAT_DIRECTO'",
        libsql::params![modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Filtros');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'FIL-WIX', 'Filtro de aceite Wix 51348', 25, 10, 900);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'RUC', '20000000001', 'EMPRESA CLIENTE S.A.C.', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
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

async fn vender(t: &Arc<TenantDb>, v: Value) -> Result<i64, (StatusCode, String)> {
    let mut base = json!({"productos": [], "total": 0, "metodo_pago": "EFECTIVO", "monto_recibido": null, "cambio": null, "usuario_id": 1, "cliente_id": null});
    for (k, x) in v.as_object().unwrap() {
        base[k] = x.clone();
    }
    let venta: NuevaVenta = serde_json::from_value(base).expect("venta");
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

fn filtro(cantidad: f64) -> Value {
    json!({"id": 1001, "nombre": "Filtro", "precio": 25, "cantidad": cantidad, "descuentoMonto": 0})
}

fn bomba() -> Value {
    json!({"nombre": "Bomba de agua Toyota", "cantidad": 1, "precio": 120, "unidad_medida": "UNIDAD", "codigo": "BA-77"})
}

async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match filas.next().await.unwrap().unwrap_or_else(|| panic!("sin filas: {}", sql)).get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        libsql::Value::Null => f64::NAN,
        otro => panic!("{}: no es número: {:?}", sql, otro),
    }
}

/// (efectivo, tarjeta, transferencia, total) de la caja abierta.
async fn caja(t: &Arc<TenantDb>) -> (f64, f64, f64, f64) {
    (
        real(t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await,
        real(t, "SELECT ventas_tarjeta FROM cajas WHERE estado = 'ABIERTA'").await,
        real(t, "SELECT ventas_transferencia FROM cajas WHERE estado = 'ABIERTA'").await,
        real(t, "SELECT total_ventas FROM cajas WHERE estado = 'ABIERTA'").await,
    )
}

/// Emite el comprobante con un Lycet que no responde: queda PENDIENTE con
/// el documento guardado, que es lo que se revisa (lo que iría a SUNAT).
async fn documento_emitido(t: &Arc<TenantDb>, venta_id: i64, tipo: &str) -> Value {
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    std::env::set_var("LYCET_TOKEN", "x");
    let factura = tipo == "FACTURA";
    let pedido = Json(minimarket_backend::models::facturacion::EmitirComprobanteRequest {
        venta_id,
        tipo: tipo.to_string(),
        cliente_documento: Some(if factura { "20000000001" } else { "12345678" }.to_string()),
        cliente_nombre: Some(if factura { "EMPRESA CLIENTE S.A.C." } else { "CLIENTE DE PRUEBA" }.to_string()),
        detraccion: None,
        fecha_hora: None,
    });
    let Json(r) = facturacion::emitir_comprobante(Extension(t.clone()), pedido).await.expect("emisión");
    assert_eq!(r.estado, "PENDIENTE", "{}", r.mensaje);
    let conn = t.0.connect().unwrap();
    let mut filas = conn
        .query(
            "SELECT a.documento FROM comprobante_archivos a JOIN comprobantes_electronicos c ON c.id = a.comprobante_id WHERE c.venta_id = ?1",
            libsql::params![venta_id],
        )
        .await
        .unwrap();
    let doc: String = filas.next().await.unwrap().expect("documento guardado").get(0).unwrap();
    serde_json::from_str(&doc).unwrap()
}

#[tokio::test]
async fn sin_el_modulo_no_se_puede_y_la_venta_normal_sigue_igual() {
    let t = base("sin_modulo", SIN_MODULO).await;
    abrir_caja(&t).await;
    let e = vender(&t, json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "monto_recibido": 200, "cambio": 55}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!(e.1.contains("no tiene encendido"), "{}", e.1);
    // Nada quedó guardado.
    assert_eq!(real(&t, "SELECT COUNT(*) FROM ventas").await, 0.0);
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 1001").await, 10.0);

    // La venta de siempre: idéntica.
    let id = vender(&t, json!({"productos": [filtro(2.0)], "total": 50, "monto_recibido": 50, "cambio": 0})).await.unwrap();
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {id}")).await, 50.0);
    assert_eq!(caja(&t).await, (50.0, 0.0, 0.0, 50.0));
    assert_eq!(real(&t, "SELECT COUNT(*) FROM venta_fuera_inventario").await, 0.0);
}

#[tokio::test]
async fn efectivo_a_caja_solo_lo_del_inventario() {
    let t = base("efectivo", CON_MODULO).await;
    abrir_caja(&t).await;
    // Filtro S/ 25 + bomba de afuera S/ 120 = S/ 145; paga con 200.
    let id = vender(&t, json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "monto_recibido": 200, "cambio": 55}))
        .await
        .unwrap();
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {id}")).await, 25.0);
    assert_eq!(real(&t, &format!("SELECT subtotal FROM ventas WHERE id = {id}")).await, 25.0);
    assert_eq!(caja(&t).await, (25.0, 0.0, 0.0, 25.0));
    // Stock: solo el filtro.
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 1001").await, 9.0);
    assert_eq!(real(&t, &format!("SELECT COUNT(*) FROM detalles_venta WHERE venta_id = {id}")).await, 1.0);
    // Lo de afuera, aparte.
    assert_eq!(real(&t, &format!("SELECT total FROM venta_fuera_inventario WHERE venta_id = {id}")).await, 120.0);
    assert_eq!(real(&t, &format!("SELECT COUNT(*) FROM venta_fuera_inventario_linea WHERE venta_id = {id}")).await, 1.0);

    // El total que manda el navegador tiene que cuadrar con lo de afuera.
    let e = vender(&t, json!({"productos": [filtro(1.0)], "total": 25, "fuera_inventario": [bomba()], "monto_recibido": 200, "cambio": 55}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(caja(&t).await, (25.0, 0.0, 0.0, 25.0));
}

#[tokio::test]
async fn mixto_lo_de_afuera_sale_primero_del_efectivo() {
    let t = base("mixto", CON_MODULO).await;
    abrir_caja(&t).await;
    // Filtros S/ 50 + bomba S/ 120 = S/ 170: 100 en efectivo + 70 por Yape.
    let id = vender(
        &t,
        json!({"productos": [filtro(2.0)], "total": 170, "fuera_inventario": [bomba()], "metodo_pago": "MIXTO",
               "pago_efectivo": 100, "pago_otro": 70, "pago_otro_metodo": "YAPE", "monto_recibido": 100, "cambio": 0}),
    )
    .await
    .unwrap();
    // A la venta y a la caja: S/ 0 en efectivo y S/ 50 por Yape.
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {id}")).await, 50.0);
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM ventas WHERE id = {id}")).await, 0.0);
    assert_eq!(real(&t, &format!("SELECT pago_otro FROM ventas WHERE id = {id}")).await, 50.0);
    assert_eq!(caja(&t).await, (0.0, 0.0, 50.0, 50.0));
    // El pago del cliente, tal cual, para reimprimir.
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM venta_fuera_inventario WHERE venta_id = {id}")).await, 100.0);
    assert_eq!(real(&t, &format!("SELECT pago_otro FROM venta_fuera_inventario WHERE venta_id = {id}")).await, 70.0);

    // 160 efectivo + 10 tarjeta: lo de afuera cabe en el efectivo.
    vender(
        &t,
        json!({"productos": [filtro(2.0)], "total": 170, "fuera_inventario": [bomba()], "metodo_pago": "MIXTO",
               "pago_efectivo": 160, "pago_otro": 10, "pago_otro_metodo": "TARJETA", "monto_recibido": 160, "cambio": 0}),
    )
    .await
    .unwrap();
    assert_eq!(caja(&t).await, (40.0, 10.0, 50.0, 100.0));

    // Un mixto que no cuadra con el total completo se rechaza.
    let e = vender(
        &t,
        json!({"productos": [filtro(2.0)], "total": 170, "fuera_inventario": [bomba()], "metodo_pago": "MIXTO",
               "pago_efectivo": 20, "pago_otro": 30, "pago_otro_metodo": "TARJETA", "monto_recibido": 20, "cambio": 0}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(caja(&t).await, (40.0, 10.0, 50.0, 100.0));
}

#[tokio::test]
async fn venta_solo_de_afuera_queda_en_cero_para_el_sistema() {
    let t = base("solo", CON_MODULO).await;
    abrir_caja(&t).await;
    let id = vender(&t, json!({"total": 120, "fuera_inventario": [bomba()], "monto_recibido": 120, "cambio": 0})).await.unwrap();
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {id}")).await, 0.0);
    assert_eq!(caja(&t).await, (0.0, 0.0, 0.0, 0.0));
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 1001").await, 10.0);

    // Su boleta: por los S/ 120 de afuera.
    let doc = documento_emitido(&t, id, "BOLETA").await;
    assert_eq!(doc["mtoImpVenta"], 120.0);
    assert_eq!(doc["details"].as_array().unwrap().len(), 1);
    assert_eq!(doc["details"][0]["descripcion"], "Bomba de agua Toyota");

    // Cobrada por Yape: a caja no entra nada, pero queda que fue Yape (el
    // comprobante dice "Yape" y no "Yape/Plin").
    let yape = vender(&t, json!({"total": 120, "fuera_inventario": [bomba()], "metodo_pago": "YAPE"})).await.unwrap();
    assert_eq!(caja(&t).await, (0.0, 0.0, 0.0, 0.0));
    assert_eq!(real(&t, "SELECT ventas_yape FROM cajas WHERE estado = 'ABIERTA'").await, 0.0);
    let conn = t.0.connect().unwrap();
    let mut f = conn.query("SELECT billetera FROM ventas WHERE id = ?1", libsql::params![yape]).await.unwrap();
    assert_eq!(f.next().await.unwrap().unwrap().get::<String>(0).unwrap(), "YAPE");

    // Al crédito no: no queda nada del inventario que deber.
    let e = vender(&t, json!({"total": 120, "fuera_inventario": [bomba()], "cliente_id": 1, "credito": {"adelanto": 0}}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn credito_el_sistema_debe_el_inventario_y_la_factura_el_total() {
    let t = base("credito", CON_MODULO).await;
    abrir_caja(&t).await;
    let id = vender(
        &t,
        json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "cliente_id": 1, "credito": {"adelanto": 0, "dias": 30}}),
    )
    .await
    .unwrap();
    // Créditos: solo el filtro.
    assert_eq!(real(&t, &format!("SELECT total FROM creditos WHERE venta_id = {id}")).await, 25.0);
    assert_eq!(caja(&t).await, (0.0, 0.0, 0.0, 25.0));

    // La factura al crédito: el total completo y la cuota por todo lo que debe.
    let doc = documento_emitido(&t, id, "FACTURA").await;
    assert_eq!(doc["mtoImpVenta"], 145.0);
    assert_eq!(doc["formaPago"]["tipo"], "Credito");
    assert_eq!(doc["cuotas"][0]["monto"], 145.0);
    let lineas = doc["details"].as_array().unwrap();
    assert_eq!(lineas.len(), 2);
    assert_eq!(lineas[1]["descripcion"], "Bomba de agua Toyota");
    assert_eq!(lineas[1]["mtoPrecioUnitario"], 120.0);
}

#[tokio::test]
async fn comprobantes_y_reimpresion_llevan_el_total_completo() {
    let t = base("comprobantes", CON_MODULO).await;
    abrir_caja(&t).await;
    let id = vender(&t, json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "monto_recibido": 145, "cambio": 0}))
        .await
        .unwrap();
    let normal = vender(&t, json!({"productos": [filtro(1.0)], "total": 25, "monto_recibido": 25, "cambio": 0})).await.unwrap();

    let doc = documento_emitido(&t, id, "BOLETA").await;
    assert_eq!(doc["mtoImpVenta"], 145.0);
    // IGV de las dos líneas (gravadas al 18 %): 145 / 1.18.
    assert_eq!(doc["mtoOperGravadas"], 122.88);

    let filtros: comprobantes::FiltrosComprobante = serde_json::from_value(json!({})).unwrap();
    let Json(lista) = comprobantes::listar_comprobantes(Extension(t.clone()), Extension(claims()), Query(filtros)).await.unwrap();
    let con = lista.iter().find(|c| c.venta_id == id).unwrap();
    assert_eq!(con.monto, 145.0);
    assert_eq!(con.fuera_inventario, Some(120.0));
    let sin = lista.iter().find(|c| c.venta_id == normal).unwrap();
    assert_eq!(sin.monto, 25.0);
    assert_eq!(sin.fuera_inventario, None);

    // Reimpresión (y Devoluciones): lo del inventario en `productos`, lo de afuera aparte.
    let folio: String = {
        let conn = t.0.connect().unwrap();
        let mut f = conn.query("SELECT folio FROM ventas WHERE id = ?1", libsql::params![id]).await.unwrap();
        f.next().await.unwrap().unwrap().get(0).unwrap()
    };
    let Json(v) = devoluciones::buscar_venta_para_devolucion(Extension(t.clone()), Path(folio)).await.unwrap();
    assert_eq!(v.total, 25.0);
    assert_eq!(v.productos.len(), 1);
    let f = v.fuera_inventario.expect("lo de afuera");
    assert_eq!(f.total, 120.0);
    assert_eq!(f.lineas[0].codigo.as_deref(), Some("BA-77"));

    // Una venta de siempre no trae el campo.
    let folio_normal: String = {
        let conn = t.0.connect().unwrap();
        let mut f = conn.query("SELECT folio FROM ventas WHERE id = ?1", libsql::params![normal]).await.unwrap();
        f.next().await.unwrap().unwrap().get(0).unwrap()
    };
    let Json(v) = devoluciones::buscar_venta_para_devolucion(Extension(t.clone()), Path(folio_normal)).await.unwrap();
    assert!(serde_json::to_value(&v).unwrap().get("fuera_inventario").is_none());
}

#[tokio::test]
async fn no_se_mezcla_con_cotizacion_ni_orden() {
    let t = base("mezcla", CON_MODULO).await;
    abrir_caja(&t).await;
    let e = vender(&t, json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "cotizacion_id": 1}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM ventas").await, 0.0);
}

#[tokio::test]
async fn reportes_y_caja_no_cuentan_la_venta_solo_de_afuera() {
    use minimarket_backend::handlers::reportes;
    let t = base("reportes", CON_MODULO).await;
    abrir_caja(&t).await;
    // Una normal (filtro S/ 25 en efectivo), una solo de afuera y una mixta con algo de afuera.
    let normal = vender(&t, json!({"productos": [filtro(1.0)], "total": 25, "monto_recibido": 25, "cambio": 0})).await.unwrap();
    let solo = vender(&t, json!({"total": 120, "fuera_inventario": [bomba()], "monto_recibido": 120, "cambio": 0})).await.unwrap();
    let mixta = vender(&t, json!({"productos": [filtro(1.0)], "total": 145, "fuera_inventario": [bomba()], "monto_recibido": 145, "cambio": 0}))
        .await
        .unwrap();

    // Caja: 2 ventas (S/ 50), no 3.
    assert_eq!(real(&t, "SELECT numero_transacciones FROM cajas WHERE estado = 'ABIERTA'").await, 2.0);
    assert_eq!(real(&t, "SELECT total_ventas FROM cajas WHERE estado = 'ABIERTA'").await, 50.0);
    assert_eq!(real(&t, "SELECT ticket_promedio FROM cajas WHERE estado = 'ABIERTA'").await, 25.0);

    // Reportes: la lista y las cuentas sin la venta solo de afuera.
    let hoy: String = {
        let conn = t.0.connect().unwrap();
        let mut f = conn.query("SELECT date(fecha_hora, '-5 hours') FROM ventas WHERE id = ?1", libsql::params![normal]).await.unwrap();
        f.next().await.unwrap().unwrap().get(0).unwrap()
    };
    let rango = || Query(serde_json::from_value::<reportes::RangoFechas>(json!({"fecha_inicio": hoy, "fecha_fin": hoy})).unwrap());
    let Json(lista) = reportes::ventas_por_rango(Extension(t.clone()), rango()).await.unwrap();
    let ids: Vec<i64> = lista.iter().map(|v| v.id).collect();
    assert!(ids.contains(&normal) && ids.contains(&mixta), "{:?}", ids);
    assert!(!ids.contains(&solo), "la venta solo de afuera no sale en Reportes");

    let Json(e) = reportes::estadisticas_completas(Extension(t.clone()), Extension(claims()), rango()).await.unwrap();
    assert_eq!(e.ventas_cantidad, 2);
    assert_eq!(e.ventas_total, 50.0);
    assert_eq!(e.ticket_promedio, 25.0);
    let efectivo = e.por_metodo.iter().find(|p| p.metodo == "EFECTIVO").unwrap();
    assert_eq!((efectivo.cantidad, efectivo.monto), (2, 50.0));

    // Su boleta sigue en Comprobantes.
    let filtros: comprobantes::FiltrosComprobante = serde_json::from_value(json!({})).unwrap();
    let Json(lista) = comprobantes::listar_comprobantes(Extension(t.clone()), Extension(claims()), Query(filtros)).await.unwrap();
    assert_eq!(lista.iter().find(|c| c.venta_id == solo).map(|c| c.monto), Some(120.0));
}
