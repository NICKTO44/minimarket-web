//! Módulo COSTEO (margen por plato, solo restaurante), contra una base local
//! (schema.sql + todas las migraciones) y los handlers de verdad. Revisa:
//!   - costo por receta (g, ml, unidades; rinde), costo fijo, precio de
//!     compra y % por categoría o general;
//!   - IGV: general 18 %, 10.5 %, Nuevo RUS, compras con factura y líneas
//!     exoneradas, con la tasa congelada en cada venta;
//!   - descuentos, devoluciones, envíos del delivery aparte, gastos
//!     registrados o aproximados y punto de equilibrio;
//!   - que todo cuadre con lo cobrado;
//!   - alertas y precio sugerido; insumo en uso no se borra;
//!   - sin el módulo, o fuera de un restaurante, no se puede usar.
//!
//!   cargo test --test costeo

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{costeo, rubros, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("costeo_{}_{}.db", nombre, std::process::id()));
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
    let modo = if rubro == "RESTAURANTE" { "RESTAURANTE" } else { "TIENDA" };
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modo_negocio = ?2, modulos = ?3, iva_porcentaje = 18, nombre_tienda = 'RESTAURANTE PRUEBA'",
        libsql::params![rubro, modo, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (2, 'caja', 'x', 'Cajero', 2);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Platos'), (901, 'Bebidas'), (902, 'Ensaladas');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, unidad_medida, precio_compra, afectacion_igv) VALUES
            (5001, 'CHAUFA', 'Arroz chaufa', 30, 0, 900, 0, 'PLATO', NULL, NULL),
            (5002, 'LOMO', 'Lomo saltado', 25, 0, 900, 0, 'PLATO', NULL, NULL),
            (5003, 'INKA', 'Inca Kola 500 ml', 2.5, 100, 901, 1, 'UNIDAD', 1.5, NULL),
            (5004, 'ENSA', 'Ensalada de frutas', 12, 0, 902, 0, 'PLATO', NULL, 'EXONERADO'),
            (5005, 'CEVI', 'Ceviche', 35, 0, 900, 0, 'PLATO', NULL, NULL),
            (5099, '_DLV_ENVIO', 'Envío delivery', 5, 0, 900, 0, 'UNIDAD', NULL, NULL);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn restaurante(nombre: &str) -> Arc<TenantDb> {
    base(nombre, "RESTAURANTE", "SERVICIOS,COSTEO").await
}

fn admin() -> Claims {
    Claims { sub: 1, username: "prueba".into(), rol_id: 1, nombre_completo: "Admin de prueba".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}
fn cajero() -> Claims {
    Claims { sub: 2, username: "caja".into(), rol_id: 2, nombre_completo: "Cajero".into(), tienda_id: 1, exp: usize::MAX, tv: 0 }
}

async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(admin()),
        Json(minimarket_backend::models::caja::AbrirCajaRequest { usuario_id: 1, numero_caja: None, monto_inicial: 100.0, observaciones: None }),
    )
    .await
    .expect("caja abierta");
}

/// Una venta en el punto de venta: (producto, precio, cantidad, descuento).
async fn vender(t: &Arc<TenantDb>, lineas: &[(i64, f64, f64, f64)]) -> i64 {
    let productos: Vec<serde_json::Value> = lineas
        .iter()
        .map(|(id, precio, cantidad, desc)| json!({"id": id, "nombre": "x", "precio": precio, "cantidad": cantidad, "descuentoMonto": desc}))
        .collect();
    let total: f64 = lineas.iter().map(|(_, p, c, d)| p * c - d).sum();
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": productos, "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0,
        "usuario_id": 1, "cliente_id": null
    }))
    .unwrap();
    ventas::procesar_venta(Extension(t.clone()), Extension(admin()), Json(venta)).await.map(|Json(r)| r.venta_id).expect("venta")
}

async fn exec(t: &Arc<TenantDb>, sql: &str) {
    t.0.connect().unwrap().execute_batch(sql).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
}

async fn reporte(t: &Arc<TenantDb>) -> costeo::ReporteCosteo {
    let Json(r) = costeo::reporte(Extension(t.clone()), Extension(admin()), Query(costeo::FiltroMes { mes: None })).await.expect("reporte");
    r
}

async fn carta(t: &Arc<TenantDb>) -> costeo::Carta {
    let Json(c) = costeo::carta(Extension(t.clone()), Extension(admin())).await.expect("carta");
    c
}

async fn configurar(t: &Arc<TenantDb>, datos: serde_json::Value) -> Result<costeo::VistaConfig, (StatusCode, String)> {
    costeo::guardar_config(Extension(t.clone()), Extension(admin()), Json(serde_json::from_value(datos).unwrap())).await.map(|Json(v)| v)
}

async fn insumo(t: &Arc<TenantDb>, nombre: &str, unidad: &str, precio: f64, con_igv: bool) -> i64 {
    let Json(i) = costeo::crear_insumo(
        Extension(t.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"nombre": nombre, "unidad": unidad, "precio": precio, "con_igv": con_igv})).unwrap()),
    )
    .await
    .expect("insumo");
    i.id
}

async fn guardar_plato(t: &Arc<TenantDb>, id: i64, datos: serde_json::Value) -> Result<costeo::DetallePlato, (StatusCode, String)> {
    costeo::guardar_plato(Extension(t.clone()), Extension(admin()), Path(id), Json(serde_json::from_value(datos).unwrap()))
        .await
        .map(|Json(d)| d)
}

fn cerca(a: f64, b: f64, que: &str) {
    assert!((a - b).abs() < 0.011, "{}: {} ≠ {}", que, a, b);
}

fn plato<'a>(r: &'a costeo::ReporteCosteo, id: i64) -> &'a costeo::PlatoMes {
    r.platos.iter().find(|p| p.producto_id == id).unwrap_or_else(|| panic!("plato {}", id))
}

/// Carta de prueba: chaufa con costo fijo 18; lomo con receta; Inca Kola con
/// precio de compra; ensalada (exonerada) por % de su categoría; ceviche por
/// el % general.
async fn costear(t: &Arc<TenantDb>) {
    let carne = insumo(t, "Carne de res", "KG", 32.0, true).await;
    let papa = insumo(t, "Papa amarilla", "KG", 3.0, false).await;
    let aceite = insumo(t, "Aceite", "LITRO", 9.0, true).await;
    let huevo = insumo(t, "Huevo", "UNIDAD", 0.5, true).await;
    guardar_plato(t, 5001, json!({"costo_fijo": 18})).await.unwrap();
    // Una tanda de 2 porciones: 400 g carne 12.80, 500 g papa 1.50, 80 ml aceite 0.72, 2 huevos 1.00 -> 16.02 / 2 = 8.01.
    let d = guardar_plato(
        t,
        5002,
        json!({"rinde": 2, "lineas": [
            {"insumo_id": carne, "cantidad": 400}, {"insumo_id": papa, "cantidad": 500},
            {"insumo_id": aceite, "cantidad": 80}, {"insumo_id": huevo, "cantidad": 2}
        ]}),
    )
    .await
    .unwrap();
    cerca(d.costo_receta.unwrap(), 16.02, "receta del lomo");
    cerca(d.plato.costo, 8.01, "porción del lomo");
    assert_eq!(d.plato.metodo, "RECETA");
    configurar(
        t,
        json!({"regimen": "GENERAL", "compras_con_factura": false, "costo_porcentaje": 35, "margen_objetivo": 65,
               "categorias": [{"categoria_id": 902, "costo_porcentaje": 30}]}),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn margen_por_plato_cuadra_con_lo_cobrado() {
    let t = restaurante("cuadra").await;
    abrir_caja(&t).await;
    costear(&t).await;

    // Ventas: 2 chaufa, 2 lomo (uno con S/ 5 de descuento), 3 Inca Kola,
    // 1 ensalada (exonerada), 1 ceviche y un envío de delivery.
    vender(&t, &[(5001, 30.0, 2.0, 0.0), (5003, 2.5, 3.0, 0.0)]).await;
    vender(&t, &[(5002, 25.0, 1.0, 0.0), (5004, 12.0, 1.0, 0.0)]).await;
    vender(&t, &[(5002, 25.0, 1.0, 5.0), (5005, 35.0, 1.0, 0.0), (5099, 5.0, 1.0, 0.0)]).await;

    let r = reporte(&t).await;
    let s = &r.resumen;
    // Lo cobrado: 60 + 7.5 + 25 + 12 + 20 + 35 + 5 = 164.50, igual que las ventas.
    cerca(s.ventas, 164.5, "ventas");
    let cobrado: f64 = {
        let conn = t.0.connect().unwrap();
        let mut f = conn.query("SELECT CAST(SUM(total) AS REAL) FROM ventas WHERE estado = 'COMPLETADA'", ()).await.unwrap();
        let v = f.next().await.unwrap().unwrap().get(0).unwrap();
        v
    };
    cerca(s.ventas, cobrado, "cuadra con las ventas");

    // Chaufa: 60 / 1.18 = 50.85; costo 36; margen 29.2 % (el ejemplo del cliente).
    let chaufa = plato(&r, 5001);
    cerca(chaufa.venta_sin_igv, 50.85, "chaufa sin IGV");
    cerca(chaufa.costo, 36.0, "chaufa costo");
    assert_eq!(chaufa.margen, Some(29.2));
    assert!(chaufa.bajo_objetivo && chaufa.exacto && chaufa.metodo == "FIJO");
    cerca(chaufa.precio_promedio.unwrap(), 30.0, "precio promedio");

    // Lomo: 45 cobrado (el descuento cuenta) / 1.18 = 38.14; 2 × 8.01 = 16.02.
    let lomo = plato(&r, 5002);
    cerca(lomo.venta, 45.0, "lomo con descuento");
    cerca(lomo.costo, 16.02, "lomo costo");
    cerca(lomo.costo_unitario.unwrap(), 8.01, "lomo unitario");
    cerca(lomo.precio_promedio.unwrap(), 22.5, "lomo precio promedio");

    // Inca Kola por su precio de compra: 3 × 1.50.
    let inka = plato(&r, 5003);
    cerca(inka.costo, 4.5, "inca costo");
    assert_eq!(inka.metodo, "COMPRA");

    // Ensalada exonerada: no se le quita IGV; costo 30 % de su categoría.
    let ensalada = plato(&r, 5004);
    cerca(ensalada.venta_sin_igv, 12.0, "ensalada sin IGV");
    cerca(ensalada.costo, 3.6, "ensalada costo");
    assert!(!ensalada.exacto && ensalada.metodo == "CATEGORIA");
    assert_eq!(ensalada.margen, Some(70.0));

    // Ceviche sin nada: 35 % general de 29.66.
    let ceviche = plato(&r, 5005);
    cerca(ceviche.costo, 35.0 / 1.18 * 0.35, "ceviche costo");
    assert_eq!(ceviche.metodo, "GENERAL");

    // El envío no es un plato: va aparte, sin costo.
    assert!(r.platos.iter().all(|p| p.producto_id != 5099));
    cerca(s.envios_sin_igv, 5.0 / 1.18, "envíos");

    // Todo suma: platos + envíos = ventas sin IGV; ventas − IGV = sin IGV.
    let suma_platos: f64 = r.platos.iter().map(|p| p.venta_sin_igv).sum();
    cerca(suma_platos, s.platos_sin_igv, "suma de platos");
    cerca(s.platos_sin_igv + s.envios_sin_igv, s.ventas_sin_igv, "platos + envíos");
    cerca(s.ventas - s.igv, s.ventas_sin_igv, "IGV");
    let costo: f64 = r.platos.iter().map(|p| p.costo).sum();
    cerca(costo, s.costo, "suma de costos");
    cerca(s.ganancia_platos, s.platos_sin_igv - s.costo, "ganancia de platos");
    cerca(s.ganancia_bruta, s.ganancia_platos + s.envios_sin_igv, "ganancia bruta");
    let participacion: f64 = r.platos.iter().filter_map(|p| p.participacion).sum();
    assert!((participacion - 100.0).abs() < 0.3, "participación {}", participacion);
    assert_eq!(s.gastos_fuente, "NINGUNO");
    assert_eq!(s.punto_equilibrio, None);
    // Parte aproximada del costo: ensalada + ceviche.
    let aprox = 3.6 + 35.0 / 1.18 * 0.35;
    assert!((s.costo_estimado.unwrap() - aprox / s.costo * 100.0).abs() < 0.1);

    // Comprando con factura recupera el IGV de lo que compra (no de la papa).
    configurar(&t, json!({"regimen": "GENERAL", "compras_con_factura": true, "costo_porcentaje": 35, "margen_objetivo": 65,
                          "categorias": [{"categoria_id": 902, "costo_porcentaje": 30}]}))
        .await
        .unwrap();
    let r = reporte(&t).await;
    cerca(plato(&r, 5001).costo, 36.0 / 1.18, "chaufa con factura");
    assert_eq!(plato(&r, 5001).margen, Some(40.0));
    let porcion = (12.8 / 1.18 + 1.5 + 0.72 / 1.18 + 1.0 / 1.18) / 2.0;
    cerca(plato(&r, 5002).costo, 2.0 * porcion, "lomo con factura");
    // Los % no cambian (ya son sobre el precio sin IGV).
    cerca(plato(&r, 5004).costo, 3.6, "ensalada con factura");

    // Nuevo RUS: no cobra IGV; lo cobrado es todo del negocio y no recupera nada.
    configurar(&t, json!({"regimen": "RUS", "compras_con_factura": true, "costo_porcentaje": 35, "margen_objetivo": 65}))
        .await
        .unwrap();
    let r = reporte(&t).await;
    cerca(r.resumen.ventas_sin_igv, 164.5, "RUS sin IGV");
    cerca(r.resumen.igv, 0.0, "RUS IGV");
    assert_eq!(plato(&r, 5001).margen, Some(40.0));
    cerca(plato(&r, 5002).costo, 16.02, "lomo RUS");
    // El % de la categoría se conserva aunque no venga en lo guardado.
    assert_eq!(plato(&r, 5004).metodo, "CATEGORIA");
}

#[tokio::test]
async fn tasa_congelada_devoluciones_gastos_y_equilibrio() {
    let t = restaurante("tasa").await;
    abrir_caja(&t).await;
    costear(&t).await;

    // Una venta a 18 % y, después de pasar a 10.5 % (restaurante MYPE), otra.
    vender(&t, &[(5001, 30.0, 1.0, 0.0)]).await;
    exec(&t, "UPDATE configuracion_tienda SET iva_porcentaje = 10.5").await;
    let segunda = vender(&t, &[(5001, 30.0, 2.0, 0.0)]).await;
    let r = reporte(&t).await;
    cerca(plato(&r, 5001).venta_sin_igv, 30.0 / 1.18 + 60.0 / 1.105, "cada venta con su tasa");
    assert_eq!(r.tasa_igv, 10.5);

    // Devuelven un chaufa de la segunda venta (no vuelve a venderse): baja lo
    // cobrado y la cantidad, pero el costo del plato hecho se queda.
    exec(
        &t,
        &format!(
            "INSERT INTO devoluciones (id, venta_original_id, folio_devolucion, fecha_hora, usuario_id, monto_reembolsado, metodo_reembolso, motivo, estado)
             VALUES (1, {v}, 'DEV-1', datetime('now'), 1, 30, 'EFECTIVO', 'Prueba', 'PROCESADA');
             INSERT INTO detalles_devolucion (devolucion_id, producto_id, detalle_venta_id, venta_id, cantidad_devuelta, precio_unitario, subtotal, condicion)
             VALUES (1, 5001, (SELECT id FROM detalles_venta WHERE venta_id = {v}), {v}, 1, 30, 30, 'DEFECTUOSO');",
            v = segunda
        ),
    )
    .await;
    let r = reporte(&t).await;
    let chaufa = plato(&r, 5001);
    cerca(chaufa.cantidad, 2.0, "cantidad neta");
    cerca(chaufa.venta, 60.0, "cobrado neto");
    cerca(chaufa.venta_sin_igv, 30.0 / 1.18 + 30.0 / 1.105, "sin IGV neto");
    cerca(chaufa.costo, 54.0, "el costo de los 3 platos hechos");
    cerca(r.resumen.devoluciones, 30.0, "devoluciones");
    cerca(r.resumen.ventas, 60.0, "ventas netas");

    // Gastos aproximados de la configuración, y luego los registrados (ganan).
    configurar(&t, json!({"regimen": "GENERAL", "compras_con_factura": true, "costo_porcentaje": 35, "margen_objetivo": 65, "gastos_mensuales": 3000}))
        .await
        .unwrap();
    let r = reporte(&t).await;
    assert_eq!(r.resumen.gastos_fuente, "ESTIMADOS");
    cerca(r.resumen.gastos, 3000.0, "gastos aproximados");
    let s = &r.resumen;
    cerca(s.ganancia_neta, s.ganancia_bruta - 3000.0, "ganancia neta");
    // Punto de equilibrio: lo que deja cada sol vendido cubre los gastos.
    let pe = s.punto_equilibrio.expect("punto de equilibrio");
    // (los totales ya vienen redondeados a céntimos: tolerancia de 0.01 %)
    let cubre = pe * (s.ventas_sin_igv / s.ventas) * (s.ganancia_bruta / s.ventas_sin_igv);
    assert!((cubre - 3000.0).abs() < 0.3, "equilibrio {}", cubre);

    let hoy = {
        let conn = t.0.connect().unwrap();
        let mut f = conn.query("SELECT date('now', '-5 hours')", ()).await.unwrap();
        f.next().await.unwrap().unwrap().get::<String>(0).unwrap()
    };
    exec(
        &t,
        &format!(
            "INSERT INTO gastos (fecha, categoria_id, descripcion, monto, metodo_pago, usuario_id)
             VALUES ('{}', (SELECT id FROM categorias_gasto WHERE nombre = 'Alquiler'), 'Alquiler del local', 1200, 'EFECTIVO', 1)",
            hoy
        ),
    )
    .await;
    let r = reporte(&t).await;
    assert_eq!(r.resumen.gastos_fuente, "REGISTRADOS");
    cerca(r.resumen.gastos, 1200.0, "gastos registrados");
}

#[tokio::test]
async fn carta_alertas_insumos_y_permisos() {
    let t = restaurante("carta").await;
    costear(&t).await;
    let c = carta(&t).await;
    // El envío interno no es de la carta.
    assert!(c.platos.iter().all(|p| p.producto_id != 5099));
    assert_eq!(c.platos.len(), 5);
    // Un plato de la "Carta de hoy" de ayer ya no sale; el de hoy sí.
    exec(
        &t,
        "INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, controla_stock, carta_fecha) VALUES
            (5201, 'CARTA-AYER', 'Menú de ayer', 10, 0, 900, 0, date('now', '-5 hours', '-1 day')),
            (5202, 'CARTA-HOY', 'Menú de hoy', 10, 0, 900, 0, date('now', '-5 hours'))",
    )
    .await;
    let c = carta(&t).await;
    assert!(c.platos.iter().all(|p| p.producto_id != 5201));
    assert!(c.platos.iter().any(|p| p.producto_id == 5202));
    assert_eq!(c.platos.len(), 6);
    let chaufa = c.platos.iter().find(|p| p.producto_id == 5001).unwrap();
    assert_eq!(chaufa.margen, Some(29.2));
    assert!(chaufa.bajo_objetivo);
    // 18 / 0.35 × 1.18 = 60.69 -> S/ 61.00
    assert_eq!(chaufa.precio_sugerido, Some(61.0));
    let ensalada = c.platos.iter().find(|p| p.producto_id == 5004).unwrap();
    assert!(!ensalada.bajo_objetivo && ensalada.precio_sugerido.is_none());
    assert_eq!(c.estimados, 3); // ensalada, ceviche y el menú de hoy
    // La Inca Kola (se compra y revende) está debajo del 65 % general...
    assert!(c.platos.iter().find(|p| p.producto_id == 5003).unwrap().bajo_objetivo);
    // ...pero con un objetivo de 25 % para Bebidas ya no; el % de costo de Ensaladas se conserva.
    configurar(&t, json!({"regimen": "GENERAL", "compras_con_factura": false, "costo_porcentaje": 35, "margen_objetivo": 65,
                          "categorias": [{"categoria_id": 901, "costo_porcentaje": null, "margen_objetivo": 25}]}))
        .await
        .unwrap();
    let c = carta(&t).await;
    let inka = c.platos.iter().find(|p| p.producto_id == 5003).unwrap();
    assert!(!inka.bajo_objetivo && inka.objetivo == 25.0);
    assert_eq!(c.platos.iter().find(|p| p.producto_id == 5004).unwrap().metodo, "CATEGORIA");
    let Json(v) = costeo::obtener_config(Extension(t.clone()), Extension(admin())).await.unwrap();
    let bebidas = v.categorias.iter().find(|x| x.categoria_id == 901).unwrap();
    assert_eq!((bebidas.costo_porcentaje, bebidas.margen_objetivo), (None, Some(25.0)));
    assert_eq!(c.bajo_objetivo, c.platos.iter().filter(|p| p.bajo_objetivo).count());

    // Subir el precio de la carne sube el costo del lomo.
    let Json(lista) = costeo::insumos(Extension(t.clone()), Extension(admin())).await.unwrap();
    let carne = lista.iter().find(|i| i.nombre == "Carne de res").unwrap();
    assert_eq!(carne.platos, 1);
    costeo::actualizar_insumo(
        Extension(t.clone()),
        Extension(admin()),
        Path(carne.id),
        Json(serde_json::from_value(json!({"nombre": "Carne de res", "unidad": "KG", "precio": 40, "con_igv": true})).unwrap()),
    )
    .await
    .unwrap();
    let lomo = carta(&t).await.platos.into_iter().find(|p| p.producto_id == 5002).unwrap();
    cerca(lomo.costo, (16.0 + 1.5 + 0.72 + 1.0) / 2.0, "lomo con carne a 40");

    // Un insumo que está en una receta no se borra; uno libre sí.
    let r = costeo::borrar_insumo(Extension(t.clone()), Extension(admin()), Path(carne.id)).await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let sal = insumo(&t, "Sal", "KG", 2.0, true).await;
    assert_eq!(costeo::borrar_insumo(Extension(t.clone()), Extension(admin()), Path(sal)).await.unwrap(), StatusCode::NO_CONTENT);
    // Nombre repetido, insumo repetido en la receta, cantidades malas.
    let repetido = costeo::crear_insumo(
        Extension(t.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"nombre": " carne de RES ", "unidad": "KG", "precio": 30})).unwrap()),
    )
    .await;
    assert_eq!(repetido.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let doble = guardar_plato(&t, 5005, json!({"lineas": [{"insumo_id": carne.id, "cantidad": 10}, {"insumo_id": carne.id, "cantidad": 5}]})).await;
    assert_eq!(doble.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    let cero = guardar_plato(&t, 5005, json!({"lineas": [{"insumo_id": carne.id, "cantidad": 0}]})).await;
    assert_eq!(cero.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(guardar_plato(&t, 5099, json!({"costo_fijo": 1})).await.err().map(|e| e.0), Some(StatusCode::NOT_FOUND));
    assert_eq!(configurar(&t, json!({"regimen": "OTRO", "compras_con_factura": false, "costo_porcentaje": 35, "margen_objetivo": 65})).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));
    assert_eq!(configurar(&t, json!({"regimen": "RUS", "compras_con_factura": false, "costo_porcentaje": 0, "margen_objetivo": 65})).await.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // Quitarle la receta y el costo: vuelve al % aproximado.
    let d = guardar_plato(&t, 5002, json!({"lineas": []})).await.unwrap();
    assert_eq!(d.plato.metodo, "GENERAL");

    // Solo el administrador.
    let r = costeo::carta(Extension(t.clone()), Extension(cajero())).await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
}

#[tokio::test]
async fn solo_restaurante_con_el_modulo() {
    // Restaurante sin el módulo: 403.
    let t = base("apagado", "RESTAURANTE", "SERVICIOS").await;
    assert_eq!(costeo::carta(Extension(t.clone()), Extension(admin())).await.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));

    // Una bodega no puede encenderlo, ni usarlo aunque lo tenga guardado.
    let b = base("bodega", "BODEGA", "COSTEO").await;
    assert_eq!(costeo::carta(Extension(b.clone()), Extension(admin())).await.err().map(|e| e.0), Some(StatusCode::FORBIDDEN));
    let r = rubros::guardar_negocio(
        Extension(b.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"rubro": "BODEGA", "modulos": ["COSTEO"]})).unwrap()),
    )
    .await;
    assert_eq!(r.err().map(|e| e.0), Some(StatusCode::BAD_REQUEST));

    // El restaurante sí puede encenderlo.
    let Json(n) = rubros::guardar_negocio(
        Extension(t.clone()),
        Extension(admin()),
        Json(serde_json::from_value(json!({"rubro": "RESTAURANTE", "modulos": ["MESAS", "SERVICIOS", "COSTEO"]})).unwrap()),
    )
    .await
    .expect("encender");
    assert!(n.modulos.iter().any(|m| m == "COSTEO"));
    assert!(costeo::carta(Extension(t.clone()), Extension(admin())).await.is_ok());
}
