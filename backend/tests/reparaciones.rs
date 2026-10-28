//! Rubro Tecnología / Reparaciones: módulo REPARACIONES (órdenes de
//! reparación sobre los encargos), contra una base local (schema.sql + todas
//! las migraciones) y los handlers de verdad. Revisa:
//!   - el catálogo del rubro; el módulo solo existe en ese rubro;
//!   - recibir un celular sin presupuesto, diagnóstico, presupuesto con un
//!     repuesto del stock y mano de obra escrita, aprobación, adelanto, listo
//!     y entrega cobrando el saldo en el punto de venta (boleta completa,
//!     stock del repuesto y caja sin contar dos veces el adelanto);
//!   - la clave de desbloqueo se borra al entregar;
//!   - garantía por N.° de serie / IMEI y búsqueda;
//!   - devolver sin cobro (no se reparó) y cancelar con devolución;
//!   - el rubro ya elegido no se cambia desde el sistema; los módulos sí.
//!
//!   cargo test --test reparaciones

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{encargos, reparaciones, rubros, unidades, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("reparaciones_{}_{}.db", nombre, std::process::id()));
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
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'TECNO PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Repuestos'), (901, 'Accesorios');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, unidad_medida) VALUES
            (6001, 'PANT-A12', 'Pantalla Samsung A12', 120, 3, 900, 1, 'UNIDAD'),
            (6002, 'CARG-C', 'Cargador USB-C 20W', 35, 10, 901, 1, 'UNIDAD'),
            (6003, 'REV', 'Revisión / diagnóstico', 20, 0, 900, 0, 'UNIDAD');
         INSERT INTO clientes (id, tipo_documento, numero_documento, nombre_razon_social, telefono) VALUES
            (1, 'DNI', '12345678', 'Luis Quispe', '987654321');",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn taller(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "TECNOLOGIA", &rubros::modulos_del_rubro("TECNOLOGIA").join(",")).await
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

fn hoy_mas(dias: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(5) + chrono::Duration::days(dias)).format("%Y-%m-%d").to_string()
}

fn celular() -> serde_json::Value {
    json!({
        "cliente_id": 1, "fecha_entrega": hoy_mas(2), "items": [],
        "equipo": {
            "tipo_equipo": "CELULAR", "marca": "Samsung", "modelo": "Galaxy A12", "serie": "3567-8901 2345 678",
            "color": "Negro", "accesorios": "Con funda, sin cargador", "clave": "1234", "estado_fisico": "Pantalla rota, esquina golpeada",
            "falla": "No da imagen después de una caída"
        }
    })
}

async fn crear(t: &Arc<TenantDb>, datos: serde_json::Value) -> Result<reparaciones::Reparacion, (StatusCode, String)> {
    reparaciones::crear(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(datos).unwrap())).await.map(|Json(r)| r)
}
async fn editar(t: &Arc<TenantDb>, id: i64, datos: serde_json::Value) -> Result<reparaciones::Reparacion, (StatusCode, String)> {
    reparaciones::actualizar(Extension(t.clone()), Path(id), Json(serde_json::from_value(datos).unwrap())).await.map(|Json(r)| r)
}
async fn etapa(t: &Arc<TenantDb>, id: i64, e: &str) -> Result<reparaciones::Reparacion, (StatusCode, String)> {
    reparaciones::cambiar_etapa(Extension(t.clone()), Path(id), Json(serde_json::from_value(json!({ "etapa": e })).unwrap()))
        .await
        .map(|Json(r)| r)
}
async fn listar(t: &Arc<TenantDb>, estado: &str, buscar: &str) -> Vec<reparaciones::FilaReparacion> {
    let Json(l) = reparaciones::listar(
        Extension(t.clone()),
        Query(serde_json::from_value(json!({"estado": estado, "buscar": buscar})).unwrap()),
    )
    .await
    .expect("listar");
    l
}

#[test]
fn catalogo_del_rubro() {
    assert!(rubros::rubro_valido("TECNOLOGIA"));
    let modulos = rubros::modulos_del_rubro("TECNOLOGIA");
    assert!(modulos.iter().any(|m| m == "REPARACIONES"));
    // Nada de otros rubros (ni encargos de pastelería, ni producción, ni costeo).
    assert!(!modulos.iter().any(|m| m == "ENCARGOS" || m == "PRODUCCION" || m == "COSTEO"));
    for u in rubros::unidades_del_rubro("TECNOLOGIA").unwrap().split(',') {
        assert!(unidades::es_valida(u), "{u}");
    }
}

#[tokio::test]
async fn reparacion_completa_hasta_la_entrega_con_garantia() {
    let t = taller("flujo").await;
    abrir_caja(&t).await;

    // 1. Llega el celular: sin presupuesto todavía. No se puede dejar adelanto sin presupuesto.
    let mut con_adelanto = celular();
    con_adelanto["adelanto"] = json!({"monto": 50, "metodo_pago": "EFECTIVO"});
    assert_eq!(crear(&t, con_adelanto).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let r = crear(&t, celular()).await.expect("recibir");
    assert_eq!((r.etapa.as_str(), r.orden.estado.as_str(), r.orden.total), ("RECIBIDO", "PENDIENTE", 0.0));
    assert_eq!(r.equipo.serie.as_deref(), Some("356789012345678"));
    assert_eq!(r.equipo.clave.as_deref(), Some("1234"));
    // Sin falla no entra.
    let mut sin_falla = celular();
    sin_falla["equipo"]["falla"] = json!("  ");
    assert_eq!(crear(&t, sin_falla).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // Las órdenes de reparación no se mezclan con los encargos.
    let Json(enc) = encargos::listar(Extension(t.clone()), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert!(enc.is_empty());

    // 2. Diagnóstico y presupuesto: la pantalla del stock + mano de obra escrita.
    etapa(&t, r.orden.id, "DIAGNOSTICO").await.unwrap();
    let mut d = celular();
    d["diagnostico"] = json!("Display dañado; placa bien");
    d["tecnico"] = json!("Pedro");
    d["garantia_dias"] = json!(30);
    d["items"] = json!([
        {"producto_id": 6001, "cantidad": 1, "precio_unitario": 120},
        {"producto_id": null, "nombre": "Mano de obra: cambio de pantalla", "cobro": "SERVICIO", "cantidad": 1, "precio_unitario": 40}
    ]);
    let r = editar(&t, r.orden.id, d.clone()).await.expect("presupuesto");
    assert_eq!(r.orden.total, 160.0);
    assert!(r.orden.items[1].libre);
    let r = etapa(&t, r.orden.id, "ESPERANDO_APROBACION").await.unwrap();
    assert_eq!(r.orden.estado, "PENDIENTE");

    // 3. Aprueba y deja S/ 60 de adelanto (sin comprobante, entra a la caja).
    let r = etapa(&t, r.orden.id, "EN_REPARACION").await.unwrap();
    assert_eq!(r.orden.estado, "EN_PREPARACION");
    assert!(r.aprobado_en.is_some());
    let Json(_) = encargos::agregar_adelanto(
        Extension(t.clone()),
        Extension(claims()),
        Path(r.orden.id),
        Json(serde_json::from_value(json!({"monto": 60, "metodo_pago": "EFECTIVO"})).unwrap()),
    )
    .await
    .expect("adelanto");
    // El presupuesto no puede bajar de lo adelantado.
    let mut barato = d.clone();
    barato["items"] = json!([{"producto_id": null, "nombre": "Ajuste", "cobro": "SERVICIO", "cantidad": 1, "precio_unitario": 30}]);
    assert_eq!(editar(&t, r.orden.id, barato).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));

    // 4. Listo.
    let r = etapa(&t, r.orden.id, "LISTO").await.unwrap();
    assert_eq!((r.orden.estado.as_str(), r.orden.saldo), ("LISTO", 100.0));
    let servicio = r.orden.items[1].producto_id;

    // 5. Entrega cobrando el saldo en el punto de venta (como manda POS.jsx).
    let venta = vender(
        &t,
        json!({
            "productos": [
                {"id": 6001, "nombre": "Pantalla Samsung A12", "precio": 120, "cantidad": 1, "descuentoMonto": 0},
                {"id": servicio, "nombre": "Mano de obra: cambio de pantalla", "precio": 40, "cantidad": 1, "descuentoMonto": 0}
            ],
            "total": 160, "encargo_id": r.orden.id, "cliente_id": 1, "monto_recibido": 100, "cambio": 0
        }),
    )
    .await
    .expect("entrega");
    assert_eq!(real(&t, &format!("SELECT total FROM ventas WHERE id = {}", venta)).await, 160.0);
    assert_eq!(texto(&t, &format!("SELECT pago_otro_metodo FROM ventas WHERE id = {}", venta)).await, "ADELANTO");
    assert_eq!(real(&t, &format!("SELECT pago_efectivo FROM ventas WHERE id = {}", venta)).await, 100.0);
    // En la boleta sale lo escrito, no "Servicio técnico".
    assert_eq!(
        texto(&t, &format!("SELECT nombre_producto FROM detalles_venta WHERE venta_id = {} AND producto_id = {}", venta, servicio)).await,
        "Mano de obra: cambio de pantalla"
    );
    // El repuesto baja del stock.
    assert_eq!(real(&t, "SELECT stock FROM productos WHERE id = 6001").await, 2.0);
    // Caja: 60 del adelanto (ingreso) + 100 de la venta; el adelanto no se cuenta dos veces.
    assert_eq!(real(&t, "SELECT ventas_efectivo FROM cajas WHERE estado = 'ABIERTA'").await, 100.0);

    let Json(r) = reparaciones::obtener(Extension(t.clone()), Path(r.orden.id)).await.unwrap();
    assert_eq!(r.orden.estado, "ENTREGADO");
    // La clave de desbloqueo ya no se guarda.
    assert_eq!(r.equipo.clave, None);
    // Garantía de 30 días desde hoy.
    assert_eq!(r.garantia_hasta.as_deref(), Some(hoy_mas(30).as_str()));
    assert!(r.en_garantia);

    // 6. Vuelve el mismo equipo (IMEI escrito distinto): sale en garantía.
    let Json(previas) = reparaciones::garantia(
        Extension(t.clone()),
        Query(serde_json::from_value(json!({"serie": "356789 012345678"})).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(previas.len(), 1);
    assert!(previas[0].en_garantia);
    // Búsqueda por IMEI, por modelo y por cliente.
    assert_eq!(listar(&t, "TODOS", "0123456").await.len(), 1);
    assert_eq!(listar(&t, "TODOS", "galaxy a12").await.len(), 1);
    assert_eq!(listar(&t, "ENTREGADOS", "luis").await.len(), 1);
    assert!(listar(&t, "ACTIVOS", "").await.is_empty());

    // Ya entregada no se edita ni cambia de etapa.
    assert_eq!(etapa(&t, r.orden.id, "EN_REPARACION").await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    assert_eq!(editar(&t, r.orden.id, d).await.err().map(|e| e.0), Some(StatusCode::CONFLICT));
    let Json(tecnicos) = reparaciones::tecnicos(Extension(t.clone())).await.unwrap();
    assert_eq!(tecnicos, vec!["Pedro".to_string()]);
}

#[tokio::test]
async fn no_se_reparo_devolver_sin_cobro_y_cancelar() {
    let t = taller("devolver").await;
    abrir_caja(&t).await;

    // No tiene arreglo y no se cobra la revisión: se devuelve sin venta.
    let r = crear(&t, celular()).await.unwrap();
    etapa(&t, r.orden.id, "NO_REPARADO").await.unwrap();
    let Json(r) = reparaciones::devolver(Extension(t.clone()), Path(r.orden.id)).await.expect("devolver");
    assert_eq!(r.orden.estado, "ENTREGADO");
    assert_eq!(r.orden.venta_id, None);
    assert_eq!(r.equipo.clave, None);

    // Con algo que cobrar (la revisión), no se devuelve sin cobro.
    let r = crear(&t, celular()).await.unwrap();
    let mut d = celular();
    d["items"] = json!([{"producto_id": 6003, "cantidad": 1, "precio_unitario": 20}]);
    let r = editar(&t, r.orden.id, d).await.unwrap();
    assert_eq!(reparaciones::devolver(Extension(t.clone()), Path(r.orden.id)).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // Cancelar con devolución del adelanto: la clave se borra.
    let Json(_) = encargos::agregar_adelanto(
        Extension(t.clone()),
        Extension(claims()),
        Path(r.orden.id),
        Json(serde_json::from_value(json!({"monto": 20, "metodo_pago": "EFECTIVO"})).unwrap()),
    )
    .await
    .unwrap();
    let Json(_) = encargos::cancelar(
        Extension(t.clone()),
        Extension(claims()),
        Path(r.orden.id),
        Json(serde_json::from_value(json!({"motivo": "El cliente no quiso", "devolver": true, "metodo_devolucion": "EFECTIVO"})).unwrap()),
    )
    .await
    .expect("cancelar");
    let Json(r) = reparaciones::obtener(Extension(t.clone()), Path(r.orden.id)).await.unwrap();
    assert_eq!(r.orden.estado, "CANCELADO");
    assert_eq!(r.equipo.clave, None);
}

#[tokio::test]
async fn solo_en_tecnologia_y_rubro_fijo() {
    // Mano de obra escrita: no en una pastelería con encargos.
    let p = base("pasteleria", "PASTELERIA", &rubros::modulos_del_rubro("PASTELERIA").join(",")).await;
    let r = encargos::crear(
        Extension(p.clone()),
        Extension(claims()),
        Json(
            serde_json::from_value(json!({
                "cliente_nombre": "Ana", "fecha_entrega": hoy_mas(1),
                "items": [{"producto_id": null, "nombre": "Algo", "cobro": "SERVICIO", "cantidad": 1, "precio_unitario": 10}]
            }))
            .unwrap(),
        ),
    )
    .await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // Las reparaciones no existen fuera del rubro, aunque el módulo quede guardado.
    let b = base("bodega", "BODEGA", "REPARACIONES").await;
    assert_eq!(
        reparaciones::listar(Extension(b.clone()), Query(serde_json::from_value(json!({})).unwrap())).await.err().map(|e| e.0),
        Some(StatusCode::FORBIDDEN)
    );
    let guardar = |t: Arc<TenantDb>, rubro: &'static str, modulos: Vec<&'static str>| async move {
        rubros::guardar_negocio(
            Extension(t),
            Extension(claims()),
            Json(serde_json::from_value(json!({"rubro": rubro, "modulos": modulos})).unwrap()),
        )
        .await
    };
    // Una bodega no puede encender reparaciones.
    assert_eq!(guardar(b.clone(), "BODEGA", vec!["REPARACIONES"]).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // El rubro elegido al crear la cuenta no se cambia...
    assert_eq!(guardar(b.clone(), "TECNOLOGIA", vec!["REPARACIONES"]).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // ...pero los módulos sí.
    let Json(n) = guardar(b.clone(), "BODEGA", vec!["CREDITO", "COTIZACIONES"]).await.expect("módulos");
    assert!(n.rubro_fijo && n.rubro == "BODEGA" && n.modulos.iter().any(|m| m == "CREDITO"));
    // Un negocio antiguo que nunca eligió rubro puede elegirlo una vez.
    let viejo = base("viejo", "", "").await;
    viejo.0.connect().unwrap().execute("UPDATE configuracion_tienda SET rubro = NULL", ()).await.unwrap();
    let Json(n) = guardar(viejo.clone(), "TECNOLOGIA", vec!["REPARACIONES"]).await.expect("elegir una vez");
    assert!(n.rubro_fijo && n.rubro == "TECNOLOGIA");
    assert_eq!(guardar(viejo.clone(), "BODEGA", vec![]).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    // En el taller funciona.
    let t = taller("ok").await;
    assert!(reparaciones::listar(Extension(t.clone()), Query(serde_json::from_value(json!({})).unwrap())).await.is_ok());
}
