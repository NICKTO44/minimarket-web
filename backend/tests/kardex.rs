//! Módulo "Ajustes y Kárdex": ajuste de inventario (por producto y por lote)
//! y kárdex de cada producto con ventas, lotes y ajustes.
//!
//!   cargo test --test kardex

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    Json,
};
use minimarket_backend::handlers::{kardex, lotes, rubros, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("kardex_{}_{}.db", nombre, std::process::id()));
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
    conn.execute("UPDATE configuracion_tienda SET rubro = 'FARMACIA', modulos = ?1", libsql::params![modulos]).await.unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'kevin', 'x', 'Kevin', 1);
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (2, 'caja', 'x', 'Cajera', 2);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Analgésicos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, unidad_medida) VALUES
            (1, 'ALC', 'Alcohol 70° 250 ml', 4.5, 24, 900, 'FRASCO');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, unidad_medida, lleva_vencimiento) VALUES
            (2, 'PAR', 'Paracetamol 500 mg', 0.3, 0, 900, 'TABLETA', 1);
         INSERT INTO lotes_producto (id, producto_id, cantidad, fecha_vencimiento, numero_lote) VALUES
            (10, 2, 100, '2027-05-01', 'L-100'),
            (11, 2, 50, '2027-09-01', 'L-200');",
    )
    .await
    .expect("datos");
    Arc::new(TenantDb(Arc::new(db)))
}

fn dueno() -> Claims {
    Claims { sub: 1, username: "kevin".into(), rol_id: 1, nombre_completo: "Kevin".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

fn cajera() -> Claims {
    Claims { sub: 2, username: "caja".into(), rol_id: 2, nombre_completo: "Cajera".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn numero(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query(sql, ()).await.unwrap();
    match f.next().await.unwrap().unwrap().get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: {:?}", sql, otro),
    }
}

async fn ver(t: &Arc<TenantDb>, id: i64) -> kardex::Kardex {
    kardex::ver(Extension(t.clone()), Extension(dueno()), Path(id), Query(kardex::Rango { desde: None, hasta: None }))
        .await
        .map(|Json(k)| k)
        .unwrap_or_else(|e| panic!("kárdex: {:?}", e))
}

async fn ajustar(t: &Arc<TenantDb>, quien: Claims, cuerpo: serde_json::Value) -> Result<kardex::ResultadoAjuste, (axum::http::StatusCode, String)> {
    kardex::ajustar(Extension(t.clone()), Extension(quien), Json(serde_json::from_value(cuerpo).unwrap())).await.map(|Json(r)| r)
}

#[tokio::test]
async fn farmacia_lo_trae_y_cualquiera_lo_enciende() {
    assert!(rubros::modulos_del_rubro("FARMACIA").iter().any(|m| m == "AJUSTES_KARDEX"));
    assert!(!rubros::modulos_del_rubro("BODEGA").iter().any(|m| m == "AJUSTES_KARDEX"));
}

#[tokio::test]
async fn sin_modulo_o_sin_ser_admin_no_entra() {
    let t = base("sin_modulo", "PRESENTACIONES").await;
    let e = ajustar(&t, dueno(), json!({"lineas": [{"producto_id": 1, "stock_contado": 20}]})).await.err().expect("bloqueado");
    assert_eq!(e.0, axum::http::StatusCode::FORBIDDEN);
    assert!(e.1.contains("Ajustes y Kárdex"), "{}", e.1);

    let t = base("cajera", "AJUSTES_KARDEX").await;
    let e = ajustar(&t, cajera(), json!({"lineas": [{"producto_id": 1, "stock_contado": 20}]})).await.err().expect("bloqueado");
    assert_eq!(e.0, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 1").await, 24.0);
}

#[tokio::test]
async fn ajuste_por_producto_y_por_lote_queda_en_el_kardex() {
    let t = base("ajuste", "AJUSTES_KARDEX").await;
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 2").await, 150.0);

    let r = ajustar(
        &t,
        dueno(),
        json!({
            "motivo": "Conteo físico",
            "lineas": [
                {"producto_id": 1, "stock_contado": 21},
                {"producto_id": 2, "numero_lote": "l-100", "stock_contado": 97, "motivo": "Rotura"},
                {"producto_id": 2, "lote_id": 11, "stock_contado": 50}
            ]
        }),
    )
    .await
    .expect("ajuste");
    assert_eq!(r.ajustados.len(), 2);
    assert_eq!(r.sin_cambio, 1);
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 1").await, 21.0);
    assert_eq!(numero(&t, "SELECT cantidad FROM lotes_producto WHERE id = 10").await, 97.0);
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 2").await, 147.0);

    let k = ver(&t, 1).await;
    assert_eq!(k.stock_actual, 21.0);
    let m = &k.movimientos[0];
    assert_eq!((m.tipo.as_str(), m.salida, m.stock_anterior, m.saldo), ("AJUSTE", 3.0, 24.0, 21.0));
    assert_eq!(m.usuario.as_deref(), Some("Kevin"));
    assert_eq!(m.motivo.as_deref(), Some("Conteo físico"));

    let k = ver(&t, 2).await;
    let m = &k.movimientos[0];
    assert_eq!((m.salida, m.stock_anterior, m.saldo), (3.0, 150.0, 147.0));
    assert_eq!(m.referencia.as_deref(), Some("Lote L-100"));
    assert_eq!(m.motivo.as_deref(), Some("Rotura"));

    // Subir el stock también se puede; un lote en 0 queda inactivo.
    ajustar(&t, dueno(), json!({"lineas": [{"producto_id": 1, "stock_contado": 30}, {"producto_id": 2, "lote_id": 11, "stock_contado": 0}]}))
        .await
        .expect("segundo ajuste");
    let k = ver(&t, 1).await;
    assert_eq!((k.movimientos[0].entrada, k.movimientos[0].saldo), (9.0, 30.0));
    assert_eq!(numero(&t, "SELECT activo FROM lotes_producto WHERE id = 11").await, 0.0);
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 2").await, 97.0);
}

#[tokio::test]
async fn un_error_no_deja_nada_a_medias() {
    let t = base("todo_o_nada", "AJUSTES_KARDEX").await;
    let e = ajustar(
        &t,
        dueno(),
        json!({"lineas": [{"producto_id": 1, "stock_contado": 5}, {"producto_id": 2, "numero_lote": "NO-EXISTE", "stock_contado": 3}]}),
    )
    .await
    .err()
    .expect("lote inexistente");
    assert!(e.1.contains("NO-EXISTE"), "{}", e.1);
    assert_eq!(numero(&t, "SELECT stock FROM productos WHERE id = 1").await, 24.0);
    // Sin lote, un producto con vencimiento no se ajusta.
    let e = ajustar(&t, dueno(), json!({"lineas": [{"producto_id": 2, "stock_contado": 3}]})).await.err().expect("sin lote");
    assert!(e.1.contains("qué lote"), "{}", e.1);
    let e = ajustar(&t, dueno(), json!({"lineas": [{"producto_id": 1, "stock_contado": -1}]})).await.err().expect("negativo");
    assert!(e.1.contains("negativo"), "{}", e.1);
}

#[tokio::test]
async fn la_venta_y_el_lote_agregado_quedan_en_el_kardex() {
    let t = base("venta", "AJUSTES_KARDEX").await;
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [
            {"id": 1, "nombre": "Alcohol", "precio": 4.5, "cantidad": 2, "descuentoMonto": 0},
            {"id": 2, "nombre": "Paracetamol", "precio": 0.3, "cantidad": 10, "descuentoMonto": 0},
            {"id": 1, "nombre": "Alcohol", "precio": 4.5, "cantidad": 1, "descuentoMonto": 0}
        ],
        "total": 16.5, "metodo_pago": "EFECTIVO", "monto_recibido": 20, "cambio": 3.5, "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(dueno()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
    let Json(v) = ventas::procesar_venta(Extension(t.clone()), Extension(dueno()), Json(venta)).await.expect("venta");

    let k = ver(&t, 1).await;
    assert_eq!(k.movimientos.len(), 1);
    let m = &k.movimientos[0];
    assert_eq!((m.tipo.as_str(), m.salida, m.stock_anterior, m.saldo), ("VENTA", 3.0, 24.0, 21.0));
    assert_eq!(m.referencia.as_deref(), Some(v.folio.as_str()));

    let k = ver(&t, 2).await;
    assert_eq!((k.movimientos[0].salida, k.movimientos[0].saldo), (10.0, 140.0));

    let nuevo = serde_json::from_value(json!({"producto_id": 2, "cantidad": 30, "fecha_vencimiento": "2028-01-01", "numero_lote": "l-300"})).unwrap();
    lotes::agregar_lote(Extension(t.clone()), Some(Extension(dueno())), Json(nuevo)).await.expect("lote");
    let k = ver(&t, 2).await;
    let m = &k.movimientos[0];
    assert_eq!((m.tipo.as_str(), m.entrada, m.stock_anterior, m.saldo), ("ENTRADA", 30.0, 140.0, 170.0));
    assert_eq!(m.referencia.as_deref(), Some("Lote L-300"));
}
