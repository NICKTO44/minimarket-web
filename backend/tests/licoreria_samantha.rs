//! Pedidos de la licorería (18-nov-2026):
//! - "Elegir precio por mayor" (MAYOR_ELEGIBLE): la caja por mayor desde la primera.
//! - "Cargos rápidos" (CARGOS_RAPIDOS): configuración y vasos gratis (descuento de línea).
//! - Ganancias por días y más vendidos por categoría.
//!
//!   cargo test --test licoreria_samantha

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    Json,
};
use minimarket_backend::handlers::{cajas, cargos_rapidos, ganancias, reportes, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::{json, Value};

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("licoreria_{}_{}.db", nombre, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET rubro = 'LICORERIA', modulos = ?1", libsql::params![modulos]).await.unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'sam', 'x', 'Samantha', 1);
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (2, 'caja', 'x', 'Cajera', 2);
         INSERT INTO categorias (id, nombre) VALUES (901, 'Cervezas'), (902, 'Descartables');
         INSERT INTO productos (id, codigo, nombre, precio, precio_compra, stock, categoria_id, unidad_medida) VALUES
            (1, 'CRI', 'Cerveza Cristal 650 ml', 7, 5, 400, 901, 'UNIDAD'),
            (2, 'PIL', 'Cerveza Pilsen 630 ml', 7, 5, 100, 901, 'UNIDAD'),
            (3, 'VAS', 'Vaso', 0.5, 0.1, 100, 902, 'UNIDAD'),
            (4, 'BOL', 'Bolsa pequeña negra', 0.1, 0.02, 100, 902, 'UNIDAD');
         INSERT INTO producto_presentaciones (id, producto_id, nombre, factor, precio, mayor_desde, precio_mayor) VALUES
            (10, 1, 'Caja x12', 12, 84, 3, 72);
         INSERT INTO producto_precios_mayor (producto_id, desde_cantidad, precio) VALUES (2, 6, 6.5);",
    )
    .await
    .expect("datos");
    let t = Arc::new(TenantDb(Arc::new(db)));
    cajas::abrir_caja(
        Extension(t.clone()),
        Extension(dueno()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    t
}

fn dueno() -> Claims {
    Claims { sub: 1, username: "sam".into(), rol_id: 1, nombre_completo: "Samantha".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

fn cajera() -> Claims {
    Claims { sub: 2, username: "caja".into(), rol_id: 2, nombre_completo: "Cajera".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn vender(t: &Arc<TenantDb>, productos: Value, total: f64) -> Result<String, String> {
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": productos, "total": total, "metodo_pago": "EFECTIVO",
        "monto_recibido": total, "cambio": 0, "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(dueno()), Json(venta))
        .await
        .map(|Json(v)| v.folio)
        .map_err(|e| e.1)
}

const MODULOS: &str = "PRESENTACIONES,PRECIO_MAYOR,GANANCIAS";

#[tokio::test]
async fn caja_por_mayor_desde_la_primera_solo_con_el_modulo() {
    // 1 caja de Cristal a S/ 72 (por mayor): 12 und a 6.00
    let caja_mayor = json!([{ "id": 1, "nombre": "Cristal", "precio": 6.0, "cantidad": 12, "descuentoMonto": 0, "presentacion_id": 10 }]);
    let sin = base("sin_modulo", MODULOS).await;
    let e = vender(&sin, caja_mayor.clone(), 72.0).await.expect_err("sin el módulo, 1 caja no va por mayor");
    assert!(e.contains("cambió"), "{}", e);
    // 3 cajas sí, como siempre.
    vender(&sin, json!([{ "id": 1, "nombre": "Cristal", "precio": 6.0, "cantidad": 36, "descuentoMonto": 0, "presentacion_id": 10 }]), 216.0)
        .await
        .expect("3 cajas por mayor");

    let con = base("con_modulo", &format!("{},MAYOR_ELEGIBLE", MODULOS)).await;
    vender(&con, caja_mayor, 72.0).await.expect("1 caja por mayor elegida");
    // Por menor sigue valiendo.
    vender(&con, json!([{ "id": 1, "nombre": "Cristal", "precio": 7.0, "cantidad": 12, "descuentoMonto": 0, "presentacion_id": 10 }]), 84.0)
        .await
        .expect("1 caja por menor");
    // Por unidad, la escala por mayor de la Pilsen (desde 6 a 6.50) con 2 unidades.
    vender(&con, json!([{ "id": 2, "nombre": "Pilsen", "precio": 6.5, "cantidad": 2, "descuentoMonto": 0 }]), 13.0)
        .await
        .expect("unidad por mayor elegida");
    // Un precio inventado no pasa.
    let e = vender(&con, json!([{ "id": 2, "nombre": "Pilsen", "precio": 5.0, "cantidad": 2, "descuentoMonto": 0 }]), 10.0)
        .await
        .expect_err("precio inventado");
    assert!(e.contains("cambió"), "{}", e);
    let e = vender(&sin, json!([{ "id": 2, "nombre": "Pilsen", "precio": 6.5, "cantidad": 2, "descuentoMonto": 0 }]), 13.0)
        .await
        .expect_err("sin el módulo, la unidad por mayor necesita llegar a 6");
    assert!(e.contains("cambió"), "{}", e);
}

#[tokio::test]
async fn cargos_rapidos_se_configuran_y_los_gratis_salen_del_stock() {
    let t = base("cargos", &format!("{},CARGOS_RAPIDOS", MODULOS)).await;
    let lista = json!([
        { "producto_id": 3, "gratis_por_venta": 2 },
        { "producto_id": 4, "gratis_por_venta": 0 },
        { "producto_id": 2, "auto_desde": "0:00", "auto_hasta": "5:00" }
    ]);
    let e = cargos_rapidos::guardar(Extension(t.clone()), Extension(cajera()), Json(serde_json::from_value(lista.clone()).unwrap()))
        .await
        .err()
        .expect("la cajera no configura");
    assert_eq!(e.0, axum::http::StatusCode::FORBIDDEN);
    let Json(guardada) = cargos_rapidos::guardar(Extension(t.clone()), Extension(dueno()), Json(serde_json::from_value(lista).unwrap()))
        .await
        .expect("guardar");
    assert_eq!(guardada.len(), 3);
    assert_eq!(guardada[2].auto_desde.as_deref(), Some("00:00"));
    let Json(leida) = cargos_rapidos::listar(Extension(t.clone())).await.expect("listar");
    assert_eq!(leida, guardada);

    // 2 cervezas + 4 vasos (2 gratis): el POS manda el descuento de los gratis.
    vender(
        &t,
        json!([
            { "id": 1, "nombre": "Cristal", "precio": 7.0, "cantidad": 2, "descuentoMonto": 0 },
            { "id": 3, "nombre": "Vaso", "precio": 0.5, "cantidad": 4, "descuentoMonto": 1.0 }
        ]),
        15.0,
    )
    .await
    .expect("venta con vasos gratis");
    let conn = t.0.connect().unwrap();
    let mut f = conn.query("SELECT CAST(stock AS REAL) FROM productos WHERE id = 3", ()).await.unwrap();
    assert_eq!(f.next().await.unwrap().unwrap().get::<f64>(0).unwrap(), 96.0, "salen los 4 vasos del stock");
    let mut f = conn.query("SELECT CAST(total AS REAL) FROM ventas ORDER BY id DESC LIMIT 1", ()).await.unwrap();
    assert_eq!(f.next().await.unwrap().unwrap().get::<f64>(0).unwrap(), 15.0);
}

#[tokio::test]
async fn ganancias_por_dias_y_mas_vendidos_por_categoria() {
    let t = base("reportes", MODULOS).await;
    vender(
        &t,
        json!([
            { "id": 1, "nombre": "Cristal", "precio": 7.0, "cantidad": 5, "descuentoMonto": 0 },
            { "id": 2, "nombre": "Pilsen", "precio": 7.0, "cantidad": 2, "descuentoMonto": 0 },
            { "id": 3, "nombre": "Vaso", "precio": 0.5, "cantidad": 4, "descuentoMonto": 0 }
        ]),
        51.0,
    )
    .await
    .expect("venta");
    let hoy = (chrono::Utc::now() - chrono::Duration::hours(5)).format("%Y-%m-%d").to_string();
    let Json(g) = ganancias::periodo(
        Extension(t.clone()),
        Extension(dueno()),
        Query(ganancias::FiltroPeriodo { desde: hoy.clone(), hasta: hoy.clone() }),
    )
    .await
    .expect("ganancia de hoy");
    assert_eq!(g.vendido, 51.0);
    assert_eq!(g.costo, 35.4); // 7 cervezas a 5 + 4 vasos a 0.10
    assert_eq!(g.ganancia, 15.6);
    assert_eq!(g.productos.len(), 3);
    let ayer = (chrono::Utc::now() - chrono::Duration::hours(29)).format("%Y-%m-%d").to_string();
    let Json(g) = ganancias::periodo(
        Extension(t.clone()),
        Extension(dueno()),
        Query(ganancias::FiltroPeriodo { desde: ayer.clone(), hasta: ayer }),
    )
    .await
    .expect("ganancia de ayer");
    assert_eq!(g.vendido, 0.0);
    let e = ganancias::periodo(
        Extension(t.clone()),
        Extension(cajera()),
        Query(ganancias::FiltroPeriodo { desde: hoy.clone(), hasta: hoy.clone() }),
    )
    .await
    .err()
    .expect("solo el administrador");
    assert_eq!(e.0, axum::http::StatusCode::FORBIDDEN);

    let Json(top) = reportes::mas_vendidos_por_categoria(
        Extension(t.clone()),
        Query(reportes::RangoFechas { fecha_inicio: hoy.clone(), fecha_fin: hoy }),
    )
    .await
    .expect("top por categoría");
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].categoria, "Cervezas");
    assert_eq!(top[0].productos[0].producto_nombre, "Cerveza Cristal 650 ml");
    assert_eq!(top[0].productos[1].producto_nombre, "Cerveza Pilsen 630 ml");
    assert_eq!(top[1].categoria, "Descartables");
}
