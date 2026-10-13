//! Venta, devolución, comprobante, abonos y caja contra datos manipulados y
//! doble clic. Base local (schema.sql + todas las migraciones), los handlers
//! de verdad. No toca Turso ni SUNAT (Lycet apunta a un puerto sin nada).
//!
//!   cargo test --test negocio_seguro

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{cajas, creditos, devoluciones, facturacion, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::caja::{AbrirCajaRequest, CerrarCajaRequest};
use minimarket_backend::models::devolucion::NuevaDevolucion;
use minimarket_backend::models::facturacion::EmitirComprobanteRequest;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<libsql::Database> {
    let ruta = std::env::temp_dir().join(format!("negocio_seguro_{}_{}.db", nombre, std::process::id()));
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
    for m in migraciones {
        conn.execute_batch(&std::fs::read_to_string(&m).unwrap())
            .await
            .unwrap_or_else(|e| panic!("{}: {}", m.display(), e));
    }
    conn.execute(
        "UPDATE configuracion_tienda SET ruc = '20161515648', razon_social = 'EMPRESA DE PRUEBA S.A.C.',
            nombre_tienda = 'VERANE PRUEBA', direccion = 'JR. MONTEVIDEO 752', ubigeo = '150101',
            departamento = 'LIMA', provincia = 'LIMA', distrito = 'LIMA',
            serie_boleta = 'BT01', serie_factura = 'FT01', facturacion_proveedor = 'SUNAT_DIRECTO', modulos = ?1",
        libsql::params![modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Cajero de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Ropa');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (901, 'POL-1', 'Polo algodón talla M', 59, 10, 900),
            (902, 'MED-1', 'Medias pack x3', 35.40, 10, 900);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social) VALUES (1, 'DNI', '12345678', 'Cliente de prueba');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(db)
}

fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Cajero de prueba".into(), tienda_id: 1, exp: usize::MAX }
}

fn venta(v: serde_json::Value) -> Json<NuevaVenta> {
    let mut base = json!({"total": 0, "metodo_pago": "EFECTIVO", "monto_recibido": null, "cambio": null, "usuario_id": 1, "cliente_id": null});
    for (k, x) in v.as_object().unwrap() {
        base[k] = x.clone();
    }
    Json(serde_json::from_value(base).expect("venta"))
}

fn linea(id: i64, precio: f64, cantidad: f64) -> serde_json::Value {
    json!({"id": id, "nombre": format!("Producto {}", id), "precio": precio, "cantidad": cantidad, "descuentoMonto": 0})
}

async fn vender(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<i64, (StatusCode, String)> {
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), venta(v)).await.map(|Json(r)| r.venta_id)
}

async fn abrir_caja(t: &Arc<TenantDb>) -> Result<i64, (StatusCode, String)> {
    cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .map(|Json(r)| r.caja_id.unwrap())
}

/// La base local de las pruebas es un archivo SQLite: si dos hilos escriben
/// a la vez, el segundo recibe "database is locked" al instante (en Turso el
/// servidor hace esperar al segundo). Aquí se reintenta como lo haría Turso;
/// cualquier otro error se devuelve tal cual.
async fn como_en_turso<T, F, Fut>(mut intento: F) -> Result<T, (StatusCode, String)>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, (StatusCode, String)>>,
{
    for _ in 0..200 {
        match intento().await {
            Err(e) if e.1.contains("locked") || e.1.contains("busy") => {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            otro => return otro,
        }
    }
    Err((StatusCode::INTERNAL_SERVER_ERROR, "la base local siguió bloqueada".into()))
}

async fn uno<T: libsql::params::IntoParams>(db: &libsql::Database, sql: &str, p: T) -> f64 {
    let conn = db.connect().unwrap();
    let mut f = conn.query(sql, p).await.unwrap();
    let fila = f.next().await.unwrap().unwrap();
    match fila.get_value(0).unwrap() {
        libsql::Value::Integer(i) => i as f64,
        libsql::Value::Real(r) => r,
        libsql::Value::Null => 0.0,
        otro => panic!("{:?}", otro),
    }
}

#[tokio::test]
async fn venta_total_y_precio_los_decide_el_servidor() {
    let db = base("venta", "COTIZACIONES").await;
    let t = Arc::new(TenantDb(db.clone()));
    abrir_caja(&t).await.unwrap();

    // Venta normal: el total coincide con 2 × 59 + 35.40.
    let id = vender(&t, json!({"productos": [linea(901, 59.0, 2.0), linea(902, 35.40, 1.0)], "total": 153.40})).await.unwrap();
    assert_eq!(uno(&db, "SELECT total FROM ventas WHERE id = ?1", [id]).await, 153.40);

    // Total inventado: no se registra nada.
    let e = vender(&t, json!({"productos": [linea(901, 59.0, 2.0)], "total": 10.0})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST, "{}", e.1);
    assert!(e.1.contains("no coincide"));

    // Precio inventado (con un total que cuadra con ese precio): rechazado.
    let e = vender(&t, json!({"productos": [linea(901, 1.0, 2.0)], "total": 2.0})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT, "{}", e.1);
    assert!(e.1.contains("Polo algodón talla M") && e.1.contains("59.00"), "{}", e.1);

    // Precio negativo: rechazado.
    let e = vender(&t, json!({"productos": [linea(901, 59.0, 1.0), linea(902, -35.40, 1.0)], "total": 23.60})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST, "{}", e.1);

    // Stock: solo bajó con la venta buena (10 - 2).
    assert_eq!(uno(&db, "SELECT stock FROM productos WHERE id = 901", ()).await, 8.0);
    assert_eq!(uno(&db, "SELECT COUNT(*) FROM ventas", ()).await, 1.0);

    // Cotización pendiente a 50 c/u: se puede vender a ese precio.
    let conn = db.connect().unwrap();
    conn.execute_batch(
        "INSERT INTO cotizaciones (id, numero, total, estado, usuario_id, fecha) VALUES (5, 1, 100, 'PENDIENTE', 1, '2026-10-11');
         INSERT INTO cotizacion_items (cotizacion_id, producto_id, nombre, cantidad, precio_unitario, total_linea)
            VALUES (5, 901, 'Polo', 2, 50, 100);",
    )
    .await
    .unwrap();
    vender(&t, json!({"productos": [linea(901, 50.0, 2.0)], "total": 100.0, "cotizacion_id": 5})).await.expect("venta de cotización");
    // Ya vendida, ese precio ya no vale.
    let e = vender(&t, json!({"productos": [linea(901, 50.0, 1.0)], "total": 50.0, "cotizacion_id": 5})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn con_precio_libre_el_cajero_puede_cambiarlo_pero_no_a_cero() {
    let db = base("precio_libre", "PRECIO_VENTA").await;
    let t = Arc::new(TenantDb(db.clone()));
    abrir_caja(&t).await.unwrap();
    vender(&t, json!({"productos": [linea(901, 55.0, 1.0)], "total": 55.0})).await.expect("precio cambiado");
    let e = vender(&t, json!({"productos": [linea(901, 0.0, 1.0)], "total": 0.0})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
}

fn devolver(venta_id: i64, items: &[(i64, i64, f64)]) -> NuevaDevolucion {
    serde_json::from_value(json!({
        "venta_id": venta_id,
        "productos": items.iter().map(|(d, p, c)| json!({"detalle_id": d, "producto_id": p, "cantidad": c})).collect::<Vec<_>>(),
        "motivo": "prueba",
        "usuario_id": 1
    }))
    .unwrap()
}

async fn detalles(db: &libsql::Database, venta_id: i64) -> Vec<i64> {
    let conn = db.connect().unwrap();
    let mut f = conn.query("SELECT id FROM detalles_venta WHERE venta_id = ?1 ORDER BY id", [venta_id]).await.unwrap();
    let mut ids = Vec::new();
    while let Some(r) = f.next().await.unwrap() {
        ids.push(r.get(0).unwrap());
    }
    ids
}

#[tokio::test]
async fn devoluciones_solo_de_lo_vendido_y_una_sola_vez() {
    let db = base("devolucion", "").await;
    let t = Arc::new(TenantDb(db.clone()));
    abrir_caja(&t).await.unwrap();
    let a = vender(&t, json!({"productos": [linea(901, 59.0, 2.0)], "total": 118.0})).await.unwrap();
    let b = vender(&t, json!({"productos": [linea(902, 35.40, 1.0)], "total": 35.40})).await.unwrap();
    let det_a = detalles(&db, a).await[0];
    let det_b = detalles(&db, b).await[0];
    let conn = db.connect().unwrap();

    // Una línea de OTRA venta: rechazado.
    let e = devoluciones::registrar_devolucion(&conn, 1, &devolver(a, &[(det_b, 902, 1.0)])).await.unwrap_err();
    assert!(e.1.contains("no es de esa venta"), "{}", e.1);
    // Cantidad 0 o negativa: rechazado.
    assert!(devoluciones::registrar_devolucion(&conn, 1, &devolver(a, &[(det_a, 901, 0.0)])).await.is_err());
    assert!(devoluciones::registrar_devolucion(&conn, 1, &devolver(a, &[(det_a, 901, -1.0)])).await.is_err());

    // El navegador dice que el producto es el 902: el stock vuelve al 901
    // (el de la venta) y se devuelve lo pagado (59).
    let stock_901 = uno(&db, "SELECT stock FROM productos WHERE id = 901", ()).await;
    let stock_902 = uno(&db, "SELECT stock FROM productos WHERE id = 902", ()).await;
    devoluciones::registrar_devolucion(&conn, 1, &devolver(a, &[(det_a, 902, 1.0)])).await.expect("devolución");
    assert_eq!(uno(&db, "SELECT stock FROM productos WHERE id = 901", ()).await, stock_901 + 1.0);
    assert_eq!(uno(&db, "SELECT stock FROM productos WHERE id = 902", ()).await, stock_902);
    assert_eq!(uno(&db, "SELECT monto_reembolsado FROM devoluciones ORDER BY id DESC LIMIT 1", ()).await, 59.0);

    // Doble clic sobre la última unidad: solo una pasa, se devuelve una vez.
    let c1 = db.connect().unwrap();
    let c2 = db.connect().unwrap();
    let p1 = devolver(a, &[(det_a, 901, 1.0)]);
    let p2 = devolver(a, &[(det_a, 901, 1.0)]);
    let (r1, r2) = tokio::join!(
        devoluciones::registrar_devolucion(&c1, 1, &p1),
        devoluciones::registrar_devolucion(&c2, 1, &p2)
    );
    assert_eq!(r1.is_ok() as u8 + r2.is_ok() as u8, 1, "{:?} / {:?}", r1.err(), r2.err());
    assert_eq!(
        uno(&db, "SELECT SUM(cantidad_devuelta) FROM detalles_devolucion WHERE detalle_venta_id = ?1", [det_a]).await,
        2.0
    );
    assert_eq!(uno(&db, "SELECT SUM(monto_reembolsado) FROM devoluciones WHERE venta_original_id = ?1", [a]).await, 118.0);

    // Ya devuelta completa: no se le emite comprobante.
    let e = facturacion::emitir_comprobante(Extension(t.clone()), pedido(a)).await.unwrap_err();
    assert!(e.1.contains("devolvió completa"), "{}", e.1);
}

fn pedido(venta_id: i64) -> Json<EmitirComprobanteRequest> {
    Json(EmitirComprobanteRequest {
        venta_id,
        tipo: "BOLETA".into(),
        cliente_documento: Some("12345678".into()),
        cliente_nombre: Some("CLIENTE DE PRUEBA".into()),
        detraccion: None,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn una_venta_un_solo_comprobante_aunque_lleguen_dos_pedidos_a_la_vez() {
    // Lycet en un puerto donde no hay nada: el envío falla y el comprobante
    // queda PENDIENTE con su número reservado.
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    std::env::set_var("LYCET_TOKEN", "x");
    let db = base("comprobante", "").await;
    let t = Arc::new(TenantDb(db.clone()));
    abrir_caja(&t).await.unwrap();
    const VENTAS: usize = 8;
    for i in 0..VENTAS {
        let id = vender(&t, json!({"productos": [linea(901, 59.0, 1.0)], "total": 59.0})).await.unwrap();
        // Dos pedidos en hilos distintos, de verdad al mismo tiempo.
        let tareas: Vec<_> = (0..2)
            .map(|_| {
                let t = t.clone();
                tokio::spawn(async move { facturacion::emitir_comprobante(Extension(t), pedido(id)).await.map(|j| j.0.numero) })
            })
            .collect();
        let mut resultados = Vec::new();
        for tarea in tareas {
            resultados.push(tarea.await.unwrap());
        }
        let n = uno(&db, "SELECT COUNT(*) FROM comprobantes_electronicos WHERE venta_id = ?1", [id]).await;
        assert_eq!(n, 1.0, "venta {}: {:?}", i, resultados);
    }
    // Números correlativos, sin saltos ni repetidos.
    assert_eq!(uno(&db, "SELECT COUNT(DISTINCT numero) FROM comprobantes_electronicos", ()).await, VENTAS as f64);
    assert_eq!(uno(&db, "SELECT MAX(numero) FROM comprobantes_electronicos", ()).await, VENTAS as f64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn abono_y_caja_con_doble_clic() {
    let db = base("abono", "CREDITO").await;
    let t = Arc::new(TenantDb(db.clone()));

    // Cuatro aperturas a la vez, en hilos distintos: una sola caja.
    let tareas: Vec<_> = (0..4)
        .map(|_| {
            let t = t.clone();
            tokio::spawn(async move { como_en_turso(|| abrir_caja(&t)).await })
        })
        .collect();
    let mut abiertas = Vec::new();
    let mut errores = Vec::new();
    for tarea in tareas {
        match tarea.await.unwrap() {
            Ok(id) => abiertas.push(id),
            Err(e) => errores.push(e.1),
        }
    }
    assert_eq!(abiertas.len(), 1, "se abrieron {:?}; errores: {:?}", abiertas, errores);
    assert_eq!(uno(&db, "SELECT COUNT(*) FROM cajas WHERE estado = 'ABIERTA'", ()).await, 1.0);
    let caja = abiertas[0];

    // Venta al crédito de 118 sin adelanto.
    vender(&t, json!({"productos": [linea(901, 59.0, 2.0)], "total": 118.0, "cliente_id": 1, "credito": {"adelanto": 0, "dias": 30}}))
        .await
        .expect("venta al crédito");
    let credito = uno(&db, "SELECT id FROM creditos", ()).await as i64;

    // Cuatro abonos por el saldo completo al mismo tiempo: entra uno.
    let tareas: Vec<_> = (0..4)
        .map(|_| {
            let t = t.clone();
            tokio::spawn(async move {
                como_en_turso(|| {
                    creditos::abonar(
                        Extension(t.clone()),
                        Extension(claims()),
                        Path(credito),
                        Json(serde_json::from_value(json!({"monto": 118.0, "metodo_pago": "EFECTIVO"})).unwrap()),
                    )
                })
                .await
                .is_ok()
            })
        })
        .collect();
    let mut buenos = 0;
    for tarea in tareas {
        buenos += tarea.await.unwrap() as u8;
    }
    assert_eq!(buenos, 1);
    assert_eq!(uno(&db, "SELECT abonado FROM creditos", ()).await, 118.0);
    assert_eq!(uno(&db, "SELECT COUNT(*) FROM credito_abonos", ()).await, 1.0);
    assert_eq!(uno(&db, "SELECT ingresos_total FROM cajas WHERE id = ?1", [caja]).await, 118.0);

    // Cerrar dos veces: el segundo no reescribe el cierre.
    let cerrar = |monto: f64| {
        cajas::cerrar_caja(
            Extension(t.clone()),
            Extension(claims()),
            Json(serde_json::from_value::<CerrarCajaRequest>(json!({"caja_id": caja, "usuario_id": 1, "usuario_rol_id": 1, "monto_contado": monto, "observaciones": null, "justificacion_diferencia": null})).unwrap()),
        )
    };
    let _ = cerrar(218.0).await.expect("cierre");
    assert!(cerrar(1.0).await.is_err());
    assert_eq!(uno(&db, "SELECT monto_final_contado FROM cajas WHERE id = ?1", [caja]).await, 218.0);
}

#[tokio::test]
async fn reporte_muestra_los_gastos_del_rango_a_admin_y_cajero() {
    use axum::extract::Query;
    use minimarket_backend::handlers::reportes;
    let db = base("reporte_gastos", "").await;
    let t = Arc::new(TenantDb(db.clone()));
    let conn = db.connect().unwrap();
    conn.execute_batch(
        "INSERT INTO categorias_gasto (id, nombre) VALUES (500, 'Pasajes');
         INSERT INTO gastos (fecha, categoria_id, descripcion, monto, metodo_pago, usuario_id) VALUES
            ('2026-10-12', 500, 'Taxi', 5, 'EFECTIVO', 1),
            ('2026-10-12', 500, 'Bolsas', 6, 'EFECTIVO_CAJA', 1),
            ('2026-10-11', 500, 'Ayer', 11, 'YAPE', 1);
         INSERT INTO gastos (fecha, categoria_id, descripcion, monto, metodo_pago, usuario_id, anulado) VALUES
            ('2026-10-12', 500, 'Anulado', 100, 'EFECTIVO', 1, 1);",
    )
    .await
    .expect("gastos de prueba");

    let pedir = |rol_id: i64, desde: &str, hasta: &str| {
        let mut c = claims();
        c.rol_id = rol_id;
        let rango: reportes::RangoFechas =
            serde_json::from_value(json!({"fecha_inicio": desde, "fecha_fin": hasta})).unwrap();
        reportes::estadisticas_completas(Extension(t.clone()), Extension(c), Query(rango))
    };

    // Solo el día 12: 5 + 6 (el anulado no cuenta).
    let Json(e) = pedir(1, "2026-10-12", "2026-10-12").await.unwrap();
    assert_eq!((e.gastos_cantidad, e.gastos_total), (Some(2), Some(11.0)));
    // Del 11 al 12: los tres.
    let Json(e) = pedir(1, "2026-10-11", "2026-10-12").await.unwrap();
    assert_eq!((e.gastos_cantidad, e.gastos_total), (Some(3), Some(22.0)));
    // El cajero también los ve.
    let Json(e) = pedir(2, "2026-10-12", "2026-10-12").await.unwrap();
    assert_eq!((e.gastos_cantidad, e.gastos_total), (Some(2), Some(11.0)));
    // Inventario no.
    let Json(e) = pedir(3, "2026-10-11", "2026-10-12").await.unwrap();
    assert_eq!((e.gastos_cantidad, e.gastos_total), (None, None));
    let texto = serde_json::to_string(&e).unwrap();
    assert!(!texto.contains("gastos"), "{texto}");
}
