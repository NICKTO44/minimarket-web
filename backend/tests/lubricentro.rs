//! Rubro Lubricentro / Repuestos contra una base local (schema.sql + todas
//! las migraciones) y los handlers de verdad. Revisa también que un negocio
//! de otro rubro no vea nada nuevo (403) y que su venta siga igual.
//!
//!   cargo test --test lubricentro

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{compatibilidad, importacion, repuestos, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

const MODULOS_LUBRICENTRO: &str = "SERVICIOS,MEDIDAS,COTIZACIONES,CREDITO,PRESENTACIONES,PRECIO_MAYOR,FICHA_REPUESTO,COMPATIBILIDAD,VEHICULOS,ORDENES_SERVICIO,PAQUETES";

pub async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("lubricentro_{}_{}.db", nombre, std::process::id()));
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
    // Las migraciones nuevas son idempotentes: aplicarlas otra vez no rompe nada.
    for m in migraciones.iter().filter(|m| {
        let n = m.file_name().unwrap().to_string_lossy();
        n.as_ref() >= "0037"
    }) {
        conn.execute_batch(&std::fs::read_to_string(m).unwrap())
            .await
            .unwrap_or_else(|e| panic!("{} (segunda vez): {}", m.display(), e));
    }
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'LUBRICENTRO PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Filtros'), (901, 'Servicios');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1001, 'FIL-WIX', 'Filtro de aceite Wix 51348', 25, 10, 900),
            (1002, 'FIL-FRAM', 'Filtro de aceite Fram PH3614', 22, 0, 900),
            (1003, 'ACE-15W40', 'Aceite 15W40 galón', 95, 6, 900);
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock) VALUES
            (1004, 'SERV-CAMBIO', 'Mano de obra cambio de aceite', 20, 0, 901, 0);
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'DNI', '12345678', 'Juan Pérez', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

pub async fn lubricentro(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "LUBRICENTRO", MODULOS_LUBRICENTRO).await
}

pub fn claims() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin de prueba".into(), tienda_id: 1, exp: usize::MAX }
}

pub async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
}

pub async fn vender(t: &Arc<TenantDb>, v: serde_json::Value) -> Result<i64, (StatusCode, String)> {
    let mut base = json!({"total": 0, "metodo_pago": "EFECTIVO", "monto_recibido": null, "cambio": null, "usuario_id": 1, "cliente_id": null});
    for (k, x) in v.as_object().unwrap() {
        base[k] = x.clone();
    }
    let venta: NuevaVenta = serde_json::from_value(base).expect("venta");
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id)
}

pub fn linea(id: i64, precio: f64, cantidad: f64) -> serde_json::Value {
    json!({"id": id, "nombre": format!("Producto {}", id), "precio": precio, "cantidad": cantidad, "descuentoMonto": 0})
}

pub async fn valor(t: &Arc<TenantDb>, sql: &str) -> libsql::Value {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    filas.next().await.unwrap().unwrap_or_else(|| panic!("sin filas: {}", sql)).get_value(0).unwrap()
}
pub async fn entero(t: &Arc<TenantDb>, sql: &str) -> i64 {
    match valor(t, sql).await {
        libsql::Value::Integer(n) => n,
        otro => panic!("{}: no es entero: {:?}", sql, otro),
    }
}
pub async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    match valor(t, sql).await {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: no es número: {:?}", sql, otro),
    }
}
pub async fn texto(t: &Arc<TenantDb>, sql: &str) -> String {
    match valor(t, sql).await {
        libsql::Value::Text(x) => x,
        otro => panic!("{}: no es texto: {:?}", sql, otro),
    }
}

fn ficha(v: serde_json::Value) -> Json<repuestos::FichaRepuesto> {
    Json(serde_json::from_value(v).unwrap())
}

#[tokio::test]
async fn ficha_del_repuesto_y_equivalencias() {
    let t = lubricentro("ficha").await;
    let Json(f) = repuestos::guardar(
        Extension(t.clone()),
        Path(1001),
        ficha(json!({"marca": " Wix ", "numero_parte": "51348", "ubicacion": "Estante 3", "garantia_dias": 90,
                     "codigos": [{"codigo": "ph-3614", "fabricante": "Fram"}, {"codigo": "W712/75"}]})),
    )
    .await
    .unwrap();
    assert_eq!(f.marca.as_deref(), Some("Wix"));
    let _ = repuestos::guardar(Extension(t.clone()), Path(1002), ficha(json!({"marca": "Fram", "numero_parte": "PH3614"})))
        .await
        .unwrap();
    let Json(todas) = repuestos::todas(Extension(t.clone())).await.unwrap();
    assert_eq!(todas.len(), 2);
    let wix = todas.iter().find(|f| f.producto_id == Some(1001)).unwrap();
    assert_eq!(wix.codigos.len(), 2);
    assert_eq!(wix.garantia_dias, Some(90));
    // La clave sin guiones permite encontrarlo por "PH3614".
    let clave = texto(&t, "SELECT codigo_clave FROM producto_codigos_alternos WHERE producto_id = 1001 ORDER BY id LIMIT 1").await;
    assert_eq!(clave, "PH3614");

    // Reemplazar quita los códigos que ya no están.
    let _ = repuestos::guardar(Extension(t.clone()), Path(1001), ficha(json!({"marca": "Wix", "codigos": [{"codigo": "51348"}]})))
        .await
        .unwrap();
    let n = entero(&t, "SELECT COUNT(*) FROM producto_codigos_alternos WHERE producto_id = 1001").await;
    assert_eq!(n, 1);
    // Quitar la ficha borra todo.
    repuestos::quitar(Extension(t.clone()), Path(1001)).await.unwrap();
    let n = entero(&t, "SELECT COUNT(*) FROM producto_repuesto WHERE producto_id = 1001").await;
    assert_eq!(n, 0);
    // Un producto que no existe.
    let e = repuestos::guardar(Extension(t.clone()), Path(99999), ficha(json!({"marca": "X"}))).await.unwrap_err();
    assert_eq!(e.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn compatibilidad_por_producto() {
    let t = lubricentro("compat").await;
    let lista = json!({"lista": [
        {"marca": "Toyota", "modelo": "Hilux", "anio_desde": 2005, "anio_hasta": 2015, "motor": "2.5"},
        {"marca": "toyota", "modelo": "hilux", "anio_desde": 2015, "anio_hasta": 2005, "motor": "2.5"},
        {"marca": "Nissan", "modelo": "Frontier"}
    ]});
    let Json(guardada) =
        compatibilidad::guardar(Extension(t.clone()), Path(1001), Json(serde_json::from_value(lista).unwrap())).await.unwrap();
    assert_eq!(guardada.len(), 2, "la repetida (años al revés) sale una vez");
    let Json(todas) = compatibilidad::todas(Extension(t.clone())).await.unwrap();
    assert_eq!(todas.len(), 2);
    let clave = texto(&t, "SELECT modelo_clave FROM producto_compatibilidad WHERE marca = 'Toyota'").await;
    assert_eq!(clave, "hilux");
    // Una lista vacía borra.
    let _ = compatibilidad::guardar(Extension(t.clone()), Path(1001), Json(serde_json::from_value(json!({"lista": []})).unwrap()))
        .await
        .unwrap();
    let n = entero(&t, "SELECT COUNT(*) FROM producto_compatibilidad").await;
    assert_eq!(n, 0);
    let mala = json!({"lista": [{"marca": "Kia", "anio_desde": 1800}]});
    let e = compatibilidad::guardar(Extension(t.clone()), Path(1001), Json(serde_json::from_value(mala).unwrap())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn importar_con_ficha_y_compatibilidad() {
    let t = lubricentro("importar").await;
    let filas = json!({"filas": [
        {"fila": 2, "codigo": "FIL-NUEVO", "nombre": "Filtro de aire Hilux", "precio": 45, "stock": 3,
         "repuesto": {"marca": "Bosch", "numero_parte": "17801-0C010", "codigos_alternos": "CA10171 (Fram)", "garantia": "3 meses"},
         "compatible": "Toyota Hilux 2005-2015; Toyota Fortuner 2008"},
        {"fila": 3, "codigo": "FIL-WIX", "nombre": "Filtro de aceite Wix 51348", "precio": 26,
         "repuesto": {"marca": "Wix", "ubicacion": "Estante 1"}},
        {"fila": 4, "codigo": "MAL-1", "nombre": "Con compatibilidad mal escrita", "precio": 10, "compatible": "Toyota Corolla 1800"}
    ], "existentes": "ACTUALIZAR"});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert!(r.guardado);
    assert_eq!((r.creados, r.actualizados, r.errores), (1, 1, 1));
    let id = entero(&t, "SELECT id FROM productos WHERE codigo = 'FIL-NUEVO'").await;
    let marca = texto(&t, &format!("SELECT marca FROM producto_repuesto WHERE producto_id = {}", id)).await;
    assert_eq!(marca, "Bosch");
    let garantia = entero(&t, &format!("SELECT garantia_dias FROM producto_repuesto WHERE producto_id = {}", id)).await;
    assert_eq!(garantia, 90);
    let compat = entero(&t, &format!("SELECT COUNT(*) FROM producto_compatibilidad WHERE producto_id = {}", id)).await;
    assert_eq!(compat, 2);
    let ubicacion = texto(&t, "SELECT ubicacion FROM producto_repuesto WHERE producto_id = 1001").await;
    assert_eq!(ubicacion, "Estante 1");
    let mal = entero(&t, "SELECT COUNT(*) FROM productos WHERE codigo = 'MAL-1'").await;
    assert_eq!(mal, 0);
}

#[tokio::test]
async fn otro_rubro_no_ve_nada_nuevo() {
    // Una tienda de ropa como VERANE: sus módulos, sin ninguno del lubricentro.
    let t = base("ropa", "ROPA", "CREDITO,VARIANTES,CAMBIOS").await;
    let e = repuestos::todas(Extension(t.clone())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = repuestos::guardar(Extension(t.clone()), Path(1001), ficha(json!({"marca": "X"}))).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = compatibilidad::todas(Extension(t.clone())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    // Un archivo con esas columnas se importa como siempre: se ignoran.
    let filas = json!({"filas": [
        {"fila": 2, "codigo": "POL-1", "nombre": "Polo", "precio": 30, "repuesto": {"marca": "X"}, "compatible": "basura 1800"}
    ]});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert_eq!((r.creados, r.errores), (1, 0));
    let n = entero(&t, "SELECT COUNT(*) FROM producto_repuesto").await;
    assert_eq!(n, 0);
    // La venta de siempre, igual.
    abrir_caja(&t).await;
    let venta = vender(&t, json!({"productos": [linea(1001, 25.0, 2.0)], "total": 50.0, "monto_recibido": 50.0, "cambio": 0.0}))
        .await
        .unwrap();
    let total = real(&t, &format!("SELECT CAST(total AS REAL) FROM ventas WHERE id = {}", venta)).await;
    assert_eq!(total, 50.0);
    let stock = real(&t, "SELECT CAST(stock AS REAL) FROM productos WHERE id = 1001").await;
    assert_eq!(stock, 8.0);
}

// ---------------------------------------------------------------------
// Vehículos, órdenes de servicio y paquetes
// ---------------------------------------------------------------------

use minimarket_backend::handlers::{ordenes, paquetes, vehiculos};

fn q<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> axum::extract::Query<T> {
    axum::extract::Query(serde_json::from_value(v).unwrap())
}

fn hoy_mas(dias: i64) -> String {
    minimarket_backend::logica::tiempo::hoy_lima_mas_dias(dias)
}

#[tokio::test]
async fn venta_con_placa_y_kilometraje() {
    let t = lubricentro("placa").await;
    abrir_caja(&t).await;
    let proxima = hoy_mas(90);
    let venta = vender(
        &t,
        json!({
            "productos": [linea(1003, 95.0, 1.0), linea(1004, 20.0, 1.0)], "total": 115.0, "monto_recibido": 115.0, "cambio": 0.0,
            "cliente_id": 1,
            "vehiculo": {"placa": "abc123", "marca": "Toyota", "modelo": "Hilux", "km": 85000, "proximo_km": 90000, "proxima_fecha": proxima}
        }),
    )
    .await
    .unwrap();
    let placa = texto(&t, &format!("SELECT placa FROM venta_vehiculo WHERE venta_id = {}", venta)).await;
    assert_eq!(placa, "ABC-123");
    let km = entero(&t, "SELECT km_ultimo FROM vehiculos WHERE placa_clave = 'ABC123'").await;
    assert_eq!(km, 85000);
    let dueno = entero(&t, "SELECT cliente_id FROM vehiculos WHERE placa_clave = 'ABC123'").await;
    assert_eq!(dueno, 1);

    // Segunda visita con menos km (error al escribir): el último no baja.
    vender(
        &t,
        json!({"productos": [linea(1004, 20.0, 1.0)], "total": 20.0, "monto_recibido": 20.0, "cambio": 0.0,
               "vehiculo": {"placa": "ABC-123", "km": 80000}}),
    )
    .await
    .unwrap();
    let km = entero(&t, "SELECT km_ultimo FROM vehiculos WHERE placa_clave = 'ABC123'").await;
    assert_eq!(km, 85000);

    // Historial y recordatorios.
    let Json(lista) = vehiculos::listar(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({"buscar": "abc-1"})).unwrap()))
        .await
        .unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].servicios, 2);
    let id = lista[0].id;
    let Json(historial) = vehiculos::historial(Extension(t.clone()), Path(id)).await.unwrap();
    assert_eq!(historial.len(), 2);
    assert_eq!(historial[1].items.len(), 2);
    // La última visita no dejó próximo servicio: no sale en recordatorios.
    let Json(r) = vehiculos::recordatorios(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({"dias": 120})).unwrap()))
        .await
        .unwrap();
    assert!(r.is_empty());

    // Un kilometraje imposible no deja la venta a medias: no entra nada.
    let antes = entero(&t, "SELECT COUNT(*) FROM ventas").await;
    let e = vender(
        &t,
        json!({"productos": [linea(1004, 20.0, 1.0)], "total": 20.0, "monto_recibido": 20.0, "cambio": 0.0,
               "vehiculo": {"placa": "ABC-123", "km": 90000, "proximo_km": 85000}}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, antes);
}

#[tokio::test]
async fn recordatorio_del_proximo_servicio() {
    let t = lubricentro("recordatorio").await;
    abrir_caja(&t).await;
    vender(
        &t,
        json!({"productos": [linea(1004, 20.0, 1.0)], "total": 20.0, "monto_recibido": 20.0, "cambio": 0.0, "cliente_id": 1,
               "vehiculo": {"placa": "XYZ-987", "proxima_fecha": hoy_mas(5)}}),
    )
    .await
    .unwrap();
    let Json(r) = vehiculos::recordatorios(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({"dias": 7})).unwrap()))
        .await
        .unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].cliente_telefono.as_deref(), Some("987654321"));
    let Json(r) = vehiculos::recordatorios(Extension(t.clone()), axum::extract::Query(serde_json::from_value(json!({"dias": 2})).unwrap()))
        .await
        .unwrap();
    assert!(r.is_empty());
}

#[tokio::test]
async fn crear_y_corregir_vehiculo() {
    let t = lubricentro("vehiculo").await;
    let Json(v) = vehiculos::crear(
        Extension(t.clone()),
        Json(serde_json::from_value(json!({"placa": "m1n 234", "marca": "Honda", "modelo": "CG 150", "anio": 2019, "cliente_id": 1})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(v.placa, "M1N-234");
    let e = vehiculos::crear(Extension(t.clone()), Json(serde_json::from_value(json!({"placa": "M1N-234"})).unwrap()))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
    let Json(v2) = vehiculos::actualizar(
        Extension(t.clone()),
        Path(v.id),
        Json(serde_json::from_value(json!({"placa": "M1N-234", "marca": "Honda", "modelo": "CB 190"})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(v2.modelo.as_deref(), Some("CB 190"));
    assert_eq!(v2.cliente_id, None);
    let Json(p) = vehiculos::por_placa(Extension(t.clone()), Path("m1n234".into())).await.unwrap();
    assert_eq!(p.id, v.id);
}

async fn mecanico(t: &Arc<TenantDb>, nombre: &str, comision: f64) -> i64 {
    let Json(m) = ordenes::crear_mecanico(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"nombre": nombre, "comision_porcentaje": comision})).unwrap()),
    )
    .await
    .unwrap();
    m.id.unwrap()
}

#[tokio::test]
async fn orden_de_servicio_de_principio_a_fin() {
    let t = lubricentro("orden").await;
    abrir_caja(&t).await;
    let pedro = mecanico(&t, "Pedro", 30.0).await;
    let Json(o) = ordenes::crear(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"placa": "bcd-456", "marca": "Kia", "modelo": "Rio", "cliente_id": 1, "km": 40000,
                                           "mecanico_id": pedro, "observaciones": "Cambio de aceite y revisar frenos"})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!((o.numero, o.estado.as_str(), o.placa.as_str()), (1, "ABIERTA", "BCD-456"));
    // Otra orden para la misma placa mientras la primera sigue abierta: no.
    let e = ordenes::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"placa": "BCD456"})).unwrap()))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);

    let Json(o) = ordenes::guardar_items(
        Extension(t.clone()),
        Path(o.id),
        Json(serde_json::from_value(json!({"items": [
            {"producto_id": 1003, "cantidad": 1}, {"producto_id": 1001, "cantidad": 1}, {"producto_id": 1004, "cantidad": 1, "descuento": 5}
        ]})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(o.total, 135.0);
    let items = o.items.as_ref().unwrap();
    assert!(items.iter().find(|i| i.producto_id == 1004).unwrap().es_servicio);

    // El precio del aceite sube después de agregarlo: la orden se cobra al de antes.
    t.0.connect().unwrap().execute("UPDATE productos SET precio = 99 WHERE id = 1003", ()).await.unwrap();
    let Json(o) = ordenes::actualizar(
        Extension(t.clone()),
        Path(o.id),
        Json(serde_json::from_value(json!({"mecanico_id": pedro, "cliente_id": 1, "km": 40000, "proximo_km": 45000,
                                           "proxima_fecha": hoy_mas(120), "observaciones": "Listo"})).unwrap()),
    )
    .await
    .unwrap();
    let Json(o) = ordenes::cambiar_estado(Extension(t.clone()), Path(o.id), Json(serde_json::from_value(json!({"estado": "TERMINADA"})).unwrap()))
        .await
        .unwrap();
    assert_eq!(o.estado, "TERMINADA");

    let cobro = json!({
        "productos": [
            linea(1003, 95.0, 1.0), linea(1001, 25.0, 1.0),
            {"id": 1004, "nombre": "Mano de obra", "precio": 20.0, "cantidad": 1.0, "descuentoMonto": 5.0}
        ],
        "total": 135.0, "monto_recibido": 135.0, "cambio": 0.0, "cliente_id": 1, "orden_servicio_id": o.id,
        "vehiculo": {"placa": "BCD-456", "km": 40000, "proximo_km": 45000}
    });
    let venta = vender(&t, cobro.clone()).await.unwrap();
    let estado = texto(&t, &format!("SELECT estado FROM ordenes_servicio WHERE id = {}", o.id)).await;
    assert_eq!(estado, "COBRADA");
    assert_eq!(entero(&t, &format!("SELECT venta_id FROM ordenes_servicio WHERE id = {}", o.id)).await, venta);
    assert_eq!(real(&t, "SELECT CAST(stock AS REAL) FROM productos WHERE id = 1003").await, 5.0);
    assert_eq!(texto(&t, &format!("SELECT placa FROM venta_vehiculo WHERE venta_id = {}", venta)).await, "BCD-456");

    // Cobrarla otra vez (doble clic u otra caja): no entra otra venta.
    let antes = entero(&t, "SELECT COUNT(*) FROM ventas").await;
    let e = vender(&t, cobro).await.unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, antes);
    // Ya no se puede cambiar.
    let e = ordenes::guardar_items(Extension(t.clone()), Path(o.id), Json(serde_json::from_value(json!({"items": []})).unwrap()))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);

    // Comisión: 30% de la mano de obra cobrada (20 - 5 = 15).
    let Json(c) = ordenes::comisiones(
        Extension(t.clone()),
        Extension(claims()),
        axum::extract::Query(serde_json::from_value(json!({"desde": hoy_mas(-1), "hasta": hoy_mas(1)})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].mano_de_obra, c[0].repuestos, c[0].comision), (15.0, 120.0, 4.5));

    // Sin precio de orden, el precio viejo de un producto no pasa.
    let e = vender(&t, json!({"productos": [linea(1003, 95.0, 1.0)], "total": 95.0, "monto_recibido": 95.0, "cambio": 0.0}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn orden_anulada_no_se_cobra() {
    let t = lubricentro("anulada").await;
    abrir_caja(&t).await;
    let Json(o) = ordenes::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"placa": "QWE-111"})).unwrap()))
        .await
        .unwrap();
    let e = ordenes::cambiar_estado(Extension(t.clone()), Path(o.id), Json(serde_json::from_value(json!({"estado": "ANULADA"})).unwrap()))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST, "anular pide motivo");
    let _ = ordenes::cambiar_estado(
        Extension(t.clone()),
        Path(o.id),
        Json(serde_json::from_value(json!({"estado": "ANULADA", "motivo": "El cliente se fue"})).unwrap()),
    )
    .await
    .unwrap();
    let e = vender(
        &t,
        json!({"productos": [linea(1004, 20.0, 1.0)], "total": 20.0, "monto_recibido": 20.0, "cambio": 0.0, "orden_servicio_id": o.id}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, StatusCode::CONFLICT);
    // Anulada, la placa puede abrir otra.
    let _ = ordenes::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(json!({"placa": "QWE111"})).unwrap()))
        .await
        .unwrap();
}

#[tokio::test]
async fn paquetes_de_productos() {
    let t = lubricentro("paquetes").await;
    let crear = |v: serde_json::Value| {
        let t = t.clone();
        async move { paquetes::crear(Extension(t), Extension(claims()), Json(serde_json::from_value(v).unwrap())).await }
    };
    crear(json!({"nombre": "Cambio de aceite completo", "precio": 130,
                 "items": [{"producto_id": 1003, "cantidad": 1}, {"producto_id": 1001, "cantidad": 1}, {"producto_id": 1004, "cantidad": 1}]}))
        .await
        .unwrap();
    // Más caro que sus productos (95 + 25 + 20 = 140): no es un paquete.
    let e = crear(json!({"nombre": "Caro", "precio": 150, "items": [{"producto_id": 1003, "cantidad": 1}, {"producto_id": 1001, "cantidad": 1}, {"producto_id": 1004, "cantidad": 1}]}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    let e = crear(json!({"nombre": "Vacío", "precio": 10, "items": []})).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    let Json(lista) = paquetes::listar(Extension(t.clone())).await.unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!((lista[0].precio, lista[0].precio_normal, lista[0].items.len()), (130.0, 140.0, 3));

    // Vendido como lo arma el punto de venta: cada producto con su descuento.
    abrir_caja(&t).await;
    let venta = vender(
        &t,
        json!({"productos": [
            {"id": 1003, "nombre": "Aceite", "precio": 95.0, "cantidad": 1.0, "descuentoMonto": 6.79},
            {"id": 1001, "nombre": "Filtro", "precio": 25.0, "cantidad": 1.0, "descuentoMonto": 1.79},
            {"id": 1004, "nombre": "Mano de obra", "precio": 20.0, "cantidad": 1.0, "descuentoMonto": 1.42}
        ], "total": 130.0, "monto_recibido": 130.0, "cambio": 0.0}),
    )
    .await
    .unwrap();
    assert_eq!(real(&t, &format!("SELECT CAST(total AS REAL) FROM ventas WHERE id = {}", venta)).await, 130.0);
    assert_eq!(real(&t, "SELECT CAST(stock AS REAL) FROM productos WHERE id = 1001").await, 9.0);
    // El comprobante (FacturaLibre o SUNAT directo) sale de este desglose:
    // cada línea con su precio ya rebajado y el total del paquete.
    let conn = t.0.connect().unwrap();
    let (lineas, d) = minimarket_backend::handlers::igv::desglose_venta(&conn, venta).await.unwrap();
    let suma: f64 = lineas.iter().map(|l| (l.cantidad * l.precio_unitario * 100.0).round() / 100.0).sum();
    assert!((suma - 130.0).abs() < 0.011, "suma de líneas {}", suma);
    assert!((d.total - 130.0).abs() < 0.001);
    assert!((lineas[0].precio_unitario - 88.21).abs() < 0.001, "aceite a {}", lineas[0].precio_unitario);

    let id = lista[0].id;
    paquetes::quitar(Extension(t.clone()), Extension(claims()), Path(id)).await.unwrap();
    let Json(lista) = paquetes::listar(Extension(t.clone())).await.unwrap();
    assert!(lista.is_empty());
}

#[tokio::test]
async fn otro_rubro_tampoco_ve_vehiculos_ordenes_ni_paquetes() {
    let t = base("ropa2", "ROPA", "CREDITO,VARIANTES,CAMBIOS").await;
    assert_eq!(vehiculos::listar(Extension(t.clone()), q(json!({}))).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(vehiculos::recordatorios(Extension(t.clone()), q(json!({}))).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(ordenes::listar(Extension(t.clone()), q(json!({}))).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(ordenes::mecanicos(Extension(t.clone())).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert_eq!(paquetes::listar(Extension(t.clone())).await.unwrap_err().0, StatusCode::FORBIDDEN);
    // Una venta con placa o con orden en un negocio sin el módulo: no entra nada.
    abrir_caja(&t).await;
    let e = vender(
        &t,
        json!({"productos": [linea(1001, 25.0, 1.0)], "total": 25.0, "monto_recibido": 25.0, "cambio": 0.0, "vehiculo": {"placa": "ABC-123"}}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = vender(&t, json!({"productos": [linea(1001, 25.0, 1.0)], "total": 25.0, "monto_recibido": 25.0, "cambio": 0.0, "orden_servicio_id": 1}))
        .await
        .unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    assert_eq!(entero(&t, "SELECT COUNT(*) FROM ventas").await, 0);
}
