//! Módulo VENTA_SIN_CONEXION contra una base local (schema.sql + todas las
//! migraciones) y los handlers de verdad.
//!
//! Lo que se hizo sin internet entra una sola vez (aunque el equipo lo mande
//! de nuevo), con su hora real y el precio que pagó el cliente; la boleta
//! sale con la fecha de la venta. Un negocio sin el módulo no puede usarlo y
//! una venta de siempre no guarda nada nuevo.
//!
//!   cargo test --test sin_conexion

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, Json};
use minimarket_backend::handlers::{sin_conexion, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::{json, Value};

const CON_MODULO: &str = "SERVICIOS,VENTA_SIN_CONEXION";
const SIN_MODULO: &str = "SERVICIOS";

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("sin_conexion_{}_{}.db", nombre, std::process::id()));
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
    // La 0061 se puede volver a correr (el aplicador real puede reintentar).
    conn.execute_batch(include_str!("../migraciones/0061_venta_sin_conexion.sql")).await.expect("0061 otra vez");
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'LUBRICENTRO', modulos = ?1, nombre_tienda = 'MOTOS PRUEBA',
            ruc = '20161515648', razon_social = 'EMPRESA DE PRUEBA S.A.C.', direccion = 'AV. DE PRUEBA 123', ubigeo = '150101',
            departamento = 'LIMA', provincia = 'LIMA', distrito = 'LIMA',
            serie_boleta = 'BT01', serie_factura = 'FT01', facturacion_proveedor = 'SUNAT_DIRECTO'",
        libsql::params![modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Aceites');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'ACE-20W50', 'Aceite 20W50 moto 1L', 30, 10, 900);
         INSERT INTO categorias_gasto (id, nombre) VALUES (77, 'Movilidad');",
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

fn venta(cantidad: f64, precio: f64) -> Value {
    json!({
        "productos": [{"id": 1001, "nombre": "Aceite", "precio": precio, "cantidad": cantidad, "descuentoMonto": 0}],
        "total": precio * cantidad, "metodo_pago": "EFECTIVO", "monto_recibido": precio * cantidad, "cambio": 0,
        "usuario_id": 1, "cliente_id": null
    })
}

async fn subir(t: &Arc<TenantDb>, uuid: &str, tipo: &str, fecha: &str, datos: Value) -> Result<sin_conexion::Resultado, (StatusCode, String)> {
    let op: sin_conexion::Operacion =
        serde_json::from_value(json!({"uuid": uuid, "tipo": tipo, "fecha_local": fecha, "datos": datos})).unwrap();
    sin_conexion::subir_operacion(Extension(t.clone()), Extension(claims()), Json(op)).await.map(|Json(r)| r)
}

async fn texto(t: &Arc<TenantDb>, sql: &str) -> String {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match filas.next().await.unwrap() {
        Some(f) => match f.get_value(0).unwrap() {
            libsql::Value::Text(s) => s,
            libsql::Value::Integer(n) => n.to_string(),
            libsql::Value::Real(n) => format!("{}", n),
            libsql::Value::Null => "NULL".into(),
            otro => format!("{:?}", otro),
        },
        None => "SIN FILAS".into(),
    }
}

fn sin_lycet() {
    // Lycet que no responde: el comprobante queda PENDIENTE con su documento.
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    std::env::set_var("LYCET_TOKEN", "x");
}

#[tokio::test]
async fn sin_modulo_no_se_puede() {
    let t = base("sin_modulo", SIN_MODULO).await;
    abrir_caja(&t).await;
    let e = subir(&t, "aaaaaaaa-0001", "VENTA", "2026-11-02 09:00:00", json!({"venta": venta(1.0, 30.0)})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "0");
}

#[tokio::test]
async fn venta_de_siempre_no_guarda_nada_nuevo() {
    let t = base("de_siempre", CON_MODULO).await;
    abrir_caja(&t).await;
    let v: NuevaVenta = serde_json::from_value(venta(1.0, 30.0)).unwrap();
    let _ = ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(v)).await.expect("venta");
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM operaciones_sin_conexion").await, "0");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "9");
}

#[tokio::test]
async fn venta_sin_conexion_entra_una_sola_vez_con_su_fecha() {
    sin_lycet();
    let t = base("una_vez", CON_MODULO).await;
    abrir_caja(&t).await;
    // Mientras no había internet, alguien subió el precio a 35: la venta se
    // cobró a 30 y así queda.
    t.0.connect().unwrap().execute("UPDATE productos SET precio = 35 WHERE id = 1001", ()).await.unwrap();

    let datos = json!({
        "venta": venta(2.0, 30.0),
        "comprobante": {"tipo": "BOLETA", "cliente_documento": null, "cliente_nombre": null}
    });
    let r = subir(&t, "venta-0001-abcd", "VENTA", "2026-11-02 09:15:00", datos.clone()).await.expect("subida");
    assert_eq!(r.estado, "HECHA", "{}", r.mensaje);
    assert!(!r.repetida);
    let venta_id = r.venta_id.unwrap();
    assert_eq!(texto(&t, &format!("SELECT fecha_hora FROM ventas WHERE id = {}", venta_id)).await, "2026-11-02 09:15:00");
    assert_eq!(texto(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta_id)).await, "60");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "8");
    assert_eq!(texto(&t, "SELECT total_ventas FROM cajas WHERE estado = 'ABIERTA'").await, "60");
    // La boleta, con la fecha de la venta.
    let comprobante = r.comprobante.expect("boleta");
    assert_eq!(comprobante["estado"], "PENDIENTE");
    assert_eq!(comprobante["fecha_emision"], "2026-11-02");
    assert_eq!(texto(&t, "SELECT fecha_emision FROM comprobantes_electronicos").await, "2026-11-02 09:15:00");
    let documento = texto(&t, "SELECT documento FROM comprobante_archivos").await;
    let documento: Value = serde_json::from_str(&documento).unwrap();
    assert!(documento["fechaEmision"].as_str().unwrap().starts_with("2026-11-02T09:15:00"), "{}", documento["fechaEmision"]);

    // El equipo la vuelve a mandar (se cortó la respuesta): no entra otra vez.
    let otra = subir(&t, "venta-0001-abcd", "VENTA", "2026-11-02 09:15:00", datos).await.expect("repetida");
    assert!(otra.repetida);
    assert_eq!(otra.venta_id, Some(venta_id));
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "1");
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM comprobantes_electronicos").await, "1");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "8");
}

#[tokio::test]
async fn venta_que_ya_entro_por_internet_no_se_duplica() {
    sin_lycet();
    let t = base("ya_entro", CON_MODULO).await;
    abrir_caja(&t).await;
    // El punto de venta la mandó con internet, pero la respuesta se perdió y
    // el equipo la guardó para subirla después.
    let mut v = venta(1.0, 30.0);
    v["uuid_local"] = json!("venta-0002-abcd");
    let v: NuevaVenta = serde_json::from_value(v).unwrap();
    let online = ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(v)).await.expect("venta").0;
    assert_eq!(texto(&t, "SELECT estado FROM operaciones_sin_conexion WHERE uuid = 'venta-0002-abcd'").await, "VENTA_HECHA");

    let r = subir(
        &t,
        "venta-0002-abcd",
        "VENTA",
        "2026-11-04 08:00:00",
        json!({"venta": venta(1.0, 30.0), "comprobante": {"tipo": "BOLETA"}}),
    )
    .await
    .expect("subida");
    assert_eq!(r.venta_id, Some(online.venta_id));
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "1");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "9");
    // Su boleta (que nunca se había emitido) sí se emite.
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM comprobantes_electronicos").await, "1");

    // Y el mismo código otra vez desde el punto de venta tampoco entra.
    let mut v = venta(1.0, 30.0);
    v["uuid_local"] = json!("venta-0002-abcd");
    let v: NuevaVenta = serde_json::from_value(v).unwrap();
    let e = ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(v)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "1");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "9");
}

#[tokio::test]
async fn sin_stock_no_se_guarda_nada_y_se_reintenta() {
    let t = base("sin_stock", CON_MODULO).await;
    abrir_caja(&t).await;
    // Otro equipo vendió casi todo mientras este no tenía internet.
    t.0.connect().unwrap().execute("UPDATE productos SET stock = 1 WHERE id = 1001", ()).await.unwrap();
    let datos = json!({"venta": venta(3.0, 30.0)});
    let e = subir(&t, "venta-0003-abcd", "VENTA", "2026-11-03 10:00:00", datos.clone()).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!(e.1.contains("Stock insuficiente"), "{}", e.1);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "0");
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM operaciones_sin_conexion").await, "0");

    // Se corrige el stock y se reintenta: entra.
    t.0.connect().unwrap().execute("UPDATE productos SET stock = 5 WHERE id = 1001", ()).await.unwrap();
    let r = subir(&t, "venta-0003-abcd", "VENTA", "2026-11-03 10:00:00", datos).await.expect("reintento");
    assert_eq!(r.estado, "HECHA");
    assert_eq!(texto(&t, "SELECT stock FROM productos WHERE id = 1001").await, "2");
}

#[tokio::test]
async fn cliente_registrado_sin_internet() {
    sin_lycet();
    let t = base("cliente", CON_MODULO).await;
    abrir_caja(&t).await;
    let datos = |uuid: &str| {
        let _ = uuid;
        json!({
            "venta": venta(1.0, 30.0),
            "comprobante": {"tipo": "BOLETA", "cliente_documento": "45678912", "cliente_nombre": "JUAN PEREZ"},
            "cliente_nuevo": {"tipo_documento": "DNI", "numero_documento": "45678912", "nombre_razon_social": "JUAN PEREZ"}
        })
    };
    let a = subir(&t, "venta-0004-abcd", "VENTA", "2026-11-03 11:00:00", datos("a")).await.expect("primera");
    let b = subir(&t, "venta-0005-abcd", "VENTA", "2026-11-03 11:30:00", datos("b")).await.expect("segunda");
    // El mismo cliente: no se duplica.
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM clientes WHERE numero_documento = '45678912'").await, "1");
    assert_eq!(a.cliente_id, b.cliente_id);
    assert_eq!(
        texto(&t, &format!("SELECT cliente_id FROM ventas WHERE id = {}", a.venta_id.unwrap())).await,
        a.cliente_id.unwrap().to_string()
    );
}

#[tokio::test]
async fn lo_que_no_se_vende_sin_conexion() {
    let t = base("no_se_vende", CON_MODULO).await;
    abrir_caja(&t).await;
    let mut v = venta(1.0, 30.0);
    v["credito"] = json!({"adelanto": 0, "adelanto_metodo": "EFECTIVO", "dias": 30});
    let e = subir(&t, "venta-0006-abcd", "VENTA", "2026-11-03 12:00:00", json!({"venta": v})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!(e.1.contains("crédito"), "{}", e.1);
    let e = subir(&t, "corto", "VENTA", "2026-11-03 12:00:00", json!({"venta": venta(1.0, 30.0)})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM ventas").await, "0");
}

#[tokio::test]
async fn reloj_adelantado_usa_la_hora_del_servidor() {
    let t = base("reloj", CON_MODULO).await;
    abrir_caja(&t).await;
    let r = subir(&t, "venta-0007-abcd", "VENTA", "2099-01-01 10:00:00", json!({"venta": venta(1.0, 30.0)})).await.expect("venta");
    let fecha = texto(&t, &format!("SELECT fecha_hora FROM ventas WHERE id = {}", r.venta_id.unwrap())).await;
    assert!(fecha.as_str() < "2099", "{}", fecha);
}

#[tokio::test]
async fn abrir_caja_sin_conexion() {
    let t = base("caja", CON_MODULO).await;
    let r = subir(&t, "caja-0001-abcd", "ABRIR_CAJA", "2026-11-03 07:30:00", json!({"monto_inicial": 50})).await.expect("apertura");
    assert_eq!(r.estado, "HECHA");
    let caja = r.caja_id.unwrap();
    assert_eq!(texto(&t, &format!("SELECT fecha_apertura FROM cajas WHERE id = {}", caja)).await, "2026-11-03 07:30:00");
    assert_eq!(texto(&t, &format!("SELECT monto_inicial FROM cajas WHERE id = {}", caja)).await, "50");
    // Otra vez el mismo código: no abre otra.
    let otra = subir(&t, "caja-0001-abcd", "ABRIR_CAJA", "2026-11-03 07:30:00", json!({"monto_inicial": 50})).await.expect("repetida");
    assert!(otra.repetida);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM cajas").await, "1");
    // Otra apertura con la caja ya abierta: las ventas entran en esa.
    let r2 = subir(&t, "caja-0002-abcd", "ABRIR_CAJA", "2026-11-03 08:00:00", json!({"monto_inicial": 10})).await.expect("ya abierta");
    assert_eq!(r2.caja_id, Some(caja));
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM cajas").await, "1");
    // Las ventas que vienen después entran en esa caja.
    subir(&t, "venta-0008-abcd", "VENTA", "2026-11-03 09:00:00", json!({"venta": venta(1.0, 30.0)})).await.expect("venta");
    assert_eq!(texto(&t, &format!("SELECT total_ventas FROM cajas WHERE id = {}", caja)).await, "30");
}

#[tokio::test]
async fn gasto_de_caja_sin_conexion() {
    let t = base("gasto", CON_MODULO).await;
    abrir_caja(&t).await;
    let gasto = json!({"categoria_id": 77, "descripcion": "Mototaxi al proveedor", "monto": 8, "metodo_pago": "EFECTIVO_CAJA", "fecha": null});
    let r = subir(&t, "gasto-0001-abcd", "GASTO", "2026-11-03 15:00:00", gasto.clone()).await.expect("gasto");
    assert_eq!(r.estado, "HECHA", "{}", r.mensaje);
    let id = r.gasto_id.unwrap();
    assert_eq!(texto(&t, &format!("SELECT fecha FROM gastos WHERE id = {}", id)).await, "2026-11-03");
    assert_eq!(texto(&t, "SELECT gastos_total FROM cajas WHERE estado = 'ABIERTA'").await, "8");
    let otra = subir(&t, "gasto-0001-abcd", "GASTO", "2026-11-03 15:00:00", gasto).await.expect("repetida");
    assert!(otra.repetida);
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM gastos").await, "1");
    assert_eq!(texto(&t, "SELECT gastos_total FROM cajas WHERE estado = 'ABIERTA'").await, "8");
    // Un gasto que no se puede (más de lo que hay en caja): no queda nada y
    // se puede reintentar con el mismo código.
    let grande = json!({"categoria_id": 77, "descripcion": "Grande", "monto": 5000, "metodo_pago": "EFECTIVO_CAJA", "fecha": null});
    assert!(subir(&t, "gasto-0002-abcd", "GASTO", "2026-11-03 15:10:00", grande).await.is_err());
    assert_eq!(texto(&t, "SELECT COUNT(*) FROM operaciones_sin_conexion WHERE uuid = 'gasto-0002-abcd'").await, "0");
}
