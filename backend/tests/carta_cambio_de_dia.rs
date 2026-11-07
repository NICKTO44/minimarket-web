//! Carta del día al cambiar de día (12:00 a. m. en Perú): los platos de ayer
//! no se listan ni se pueden pedir ni vender; la carta fija sigue igual.
//!
//!   cargo test --test carta_cambio_de_dia

use std::sync::Arc;

use axum::{extract::{Extension, Path}, Json};
use minimarket_backend::handlers::{carta, mesas, productos, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn restaurante(nombre: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("carta_dia_{}_{}.db", nombre, std::process::id()));
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
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'RESTAURANTE', modo_negocio = 'RESTAURANTE', modulos = 'SERVICIOS', nombre_tienda = 'RESTO PRUEBA'",
        (),
    )
    .await
    .unwrap();
    let ayer = (chrono::Utc::now() - chrono::Duration::hours(5) - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    conn.execute_batch(&format!(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'admin', 'x', 'Admin', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Bebidas');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock) VALUES
            (5001, 'INKA', 'Inca Kola', 3.5, 100, 900, 1);
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, carta_fecha) VALUES
            (6001, 'CARTA-AYER-1', 'Seco de ayer', 14, 0, 900, 0, '{}');",
        ayer
    ))
    .await
    .expect("datos");
    Arc::new(TenantDb(Arc::new(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "admin".into(), rol_id: 1, nombre_completo: "Admin".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

#[tokio::test]
async fn los_platos_de_ayer_no_se_listan() {
    let t = restaurante("listar").await;
    carta::agregar_plato(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(json!({"nombre": "Ají de hoy", "precio": 12})).unwrap()))
        .await
        .expect("plato de hoy");

    let hoy: Vec<String> = carta::listar_carta(Extension(t.clone())).await.unwrap().0.into_iter().map(|p| p.nombre).collect();
    assert_eq!(hoy, vec!["Ají de hoy".to_string()]);

    let activos: Vec<String> = productos::listar_productos(Extension(t.clone())).await.unwrap().0.into_iter().map(|p| p.nombre).collect();
    assert!(activos.contains(&"Inca Kola".to_string()), "la carta fija sigue: {:?}", activos);
    assert!(activos.contains(&"Ají de hoy".to_string()), "{:?}", activos);
    assert!(!activos.contains(&"Seco de ayer".to_string()), "el plato de ayer no se lista: {:?}", activos);
}

#[tokio::test]
async fn un_plato_de_ayer_no_se_pide_en_mesa() {
    let t = restaurante("mesa").await;
    let pedido = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"tipo": "LLEVAR", "cliente_nombre": "Prueba"})).unwrap()),
    )
    .await
    .expect("pedido")
    .0;
    // Una pantalla abierta desde ayer todavía lo muestra y lo manda.
    let r = mesas::agregar_items(
        Extension(t.clone()),
        Extension(admin()),
        Path(pedido.resumen.id),
        Json(serde_json::from_value(json!({"items": [{"producto_id": 6001, "cantidad": 1}]})).unwrap()),
    )
    .await;
    assert!(r.is_err(), "no debería aceptar un plato de la carta de ayer");
}

#[tokio::test]
async fn un_plato_de_ayer_no_se_vende_en_el_pos() {
    let t = restaurante("pos").await;
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja");
    // La carta fija se vende normal.
    let fija: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 5001, "nombre": "Inca Kola", "precio": 3.5, "cantidad": 1, "descuentoMonto": 0}],
        "total": 3.5, "metodo_pago": "EFECTIVO", "monto_recibido": 3.5, "cambio": 0, "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(admin()), Json(fija)).await.expect("venta carta fija");
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 6001, "nombre": "Seco de ayer", "precio": 14, "cantidad": 1, "descuentoMonto": 0}],
        "total": 14, "metodo_pago": "EFECTIVO", "monto_recibido": 14, "cambio": 0, "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    let r = ventas::procesar_venta(Extension(t.clone()), Extension(admin()), Json(venta)).await;
    if let Err(e) = &r { println!("rechazo: {:?}", e); }
    assert!(r.is_err(), "no debería vender un plato de la carta de ayer");
}

async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja");
}

/// Una mesa que pidió antes de medianoche se cobra después sin problema.
#[tokio::test]
async fn la_mesa_de_anoche_se_cobra_despues_de_medianoche() {
    let t = restaurante("cobro").await;
    abrir_caja(&t).await;
    let plato = carta::agregar_plato(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(json!({"nombre": "Ají", "precio": 12})).unwrap()))
        .await
        .expect("plato")
        .0[0]
        .id;
    let pedido = mesas::abrir_pedido(
        Extension(t.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"tipo": "LLEVAR", "cliente_nombre": "Prueba"})).unwrap()),
    )
    .await
    .expect("pedido")
    .0
    .resumen
    .id;
    mesas::agregar_items(
        Extension(t.clone()),
        Extension(admin()),
        Path(pedido),
        Json(serde_json::from_value(json!({"items": [{"producto_id": plato, "cantidad": 2}]})).unwrap()),
    )
    .await
    .expect("se pide el plato de hoy");
    // Pasa la medianoche: el plato ya es "de ayer".
    let ayer = (chrono::Utc::now() - chrono::Duration::hours(5) - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    t.0.connect().unwrap().execute("UPDATE productos SET carta_fecha = ?1 WHERE id = ?2", libsql::params![ayer, plato]).await.unwrap();

    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": plato, "nombre": "Ají", "precio": 12, "cantidad": 2, "descuentoMonto": 0}],
        "total": 24, "metodo_pago": "EFECTIVO", "monto_recibido": 24, "cambio": 0, "usuario_id": 1, "cliente_id": null,
        "pedido_id": pedido
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(admin()), Json(venta)).await.expect("la mesa de anoche se cobra");
}

/// Una venta hecha sin internet ayer se sube hoy con su plato de ayer.
#[tokio::test]
async fn venta_sin_conexion_de_ayer_se_acepta() {
    let t = restaurante("offline").await;
    abrir_caja(&t).await;
    let ayer = (chrono::Utc::now() - chrono::Duration::hours(5) - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    let mut venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": 6001, "nombre": "Seco de ayer", "precio": 14, "cantidad": 1, "descuentoMonto": 0}],
        "total": 14, "metodo_pago": "EFECTIVO", "monto_recibido": 14, "cambio": 0, "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    venta.sin_conexion = Some(minimarket_backend::models::venta::VentaSinConexion { fecha_hora: format!("{} 21:30:00", ayer) });
    ventas::procesar_venta(Extension(t.clone()), Extension(admin()), Json(venta)).await.expect("venta sin conexión de ayer");
}

/// Tocar una mesa y salir sin pedir nada la deja libre; con algo pedido, no.
#[tokio::test]
async fn mesa_sin_pedido_queda_libre() {
    let t = restaurante("mesa_vacia").await;
    let conn = t.0.connect().unwrap();
    conn.execute("INSERT INTO mesas (id, nombre, activo) VALUES (901, 'Mesa prueba', 1)", ()).await.unwrap();
    let abrir = || async {
        mesas::abrir_pedido(
            Extension(t.clone()),
            Extension(admin()),
            Json(serde_json::from_value(json!({"tipo": "MESA", "mesa_id": 901})).unwrap()),
        )
        .await
        .expect("pedido")
        .0
        .resumen
        .id
    };
    let abiertos = || async {
        let mut f = t.0.connect().unwrap().query("SELECT COUNT(*) FROM pedidos WHERE mesa_id = 901 AND estado = 'ABIERTO'", ()).await.unwrap();
        f.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    };
    let vacio = abrir().await;
    mesas::descartar_vacio(Extension(t.clone()), Path(vacio)).await.expect("descartar");
    assert_eq!(abiertos().await, 0, "la mesa quedó libre");

    let con_algo = abrir().await;
    mesas::agregar_items(
        Extension(t.clone()),
        Extension(admin()),
        Path(con_algo),
        Json(serde_json::from_value(json!({"items": [{"producto_id": 5001, "cantidad": 1}]})).unwrap()),
    )
    .await
    .expect("item");
    mesas::descartar_vacio(Extension(t.clone()), Path(con_algo)).await.expect("descartar");
    assert_eq!(abiertos().await, 1, "un pedido con algo no se borra");
}

/// La categoría interna "Carta del día" no se asigna a un producto fijo.
#[tokio::test]
async fn categoria_de_la_carta_no_es_para_productos_fijos() {
    let t = restaurante("categoria").await;
    carta::agregar_plato(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(json!({"nombre": "Ají", "precio": 12})).unwrap()))
        .await
        .expect("plato");
    let mut f = t.0.connect().unwrap().query("SELECT id FROM categorias WHERE nombre = 'Carta del día'", ()).await.unwrap();
    let cat: i64 = f.next().await.unwrap().unwrap().get(0).unwrap();
    drop(f);
    let nuevo = |categoria: i64| {
        serde_json::from_value(json!({
            "codigo": format!("FIJO{}", categoria), "nombre": "Chicha", "descripcion": null, "precio": 12, "stock": 0,
            "stock_minimo": 0, "unidad_medida": "UNIDAD", "categoria_id": categoria
        }))
        .unwrap()
    };
    let r = productos::agregar_producto(Extension(t.clone()), Json(nuevo(cat))).await;
    assert!(r.is_err(), "no se crea un fijo en la categoría de la carta");
    productos::agregar_producto(Extension(t.clone()), Json(nuevo(900))).await.expect("en otra categoría sí");
}
