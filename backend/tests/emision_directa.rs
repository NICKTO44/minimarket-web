//! Prueba de punta a punta de la emisión directa: base local con el schema
//! y todas las migraciones, una venta real, el handler de siempre
//! (POST /comprobantes) y un Lycet apuntando al ambiente beta de SUNAT.
//!
//! No corre con `cargo test`; se lanza así (con Lycet levantado aparte):
//!   LYCET_URL=http://127.0.0.1:8000 LYCET_TOKEN=123456 \
//!   cargo test --test emision_directa -- --ignored --nocapture

use std::sync::Arc;

use axum::{extract::Extension, Json};
use minimarket_backend::handlers::facturacion::emitir_comprobante;
use minimarket_backend::models::facturacion::EmitirComprobanteRequest;
use minimarket_backend::tenants::TenantDb;

async fn base_de_prueba() -> Arc<libsql::Database> {
    let ruta = std::env::temp_dir().join(format!("emision_directa_{}.db", std::process::id()));
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
    conn.execute_batch(
        "UPDATE configuracion_tienda SET ruc = '20161515648', razon_social = 'EMPRESA DE PRUEBA S.A.C.',
            nombre_tienda = 'VERANE PRUEBA', direccion = 'JR. MONTEVIDEO 752', ubigeo = '150101',
            departamento = 'LIMA', provincia = 'LIMA', distrito = 'LIMA',
            serie_boleta = 'BT01', serie_factura = 'FT01', facturacion_proveedor = 'SUNAT_DIRECTO';
         INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Cajero de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Ropa');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (901, 'POL-1', 'Polo algodón talla M', 59, 10, 900),
            (902, 'MED-1', 'Medias pack x3', 35.40, 10, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(db)
}

async fn venta(db: &libsql::Database, folio: &str) -> i64 {
    let conn = db.connect().unwrap();
    conn.execute(
        "INSERT INTO ventas (folio, subtotal, total, metodo_pago, usuario_id, estado) VALUES (?1, 153.40, 153.40, 'EFECTIVO', 1, 'COMPLETADA')",
        libsql::params![folio],
    )
    .await
    .unwrap();
    let id = conn.last_insert_rowid();
    conn.execute_batch(&format!(
        "INSERT INTO detalles_venta (venta_id, producto_id, cantidad, precio_unitario, subtotal, total_linea) VALUES
            ({id}, 901, 2, 59, 118, 118), ({id}, 902, 1, 35.40, 35.40, 35.40);"
    ))
    .await
    .unwrap();
    id
}

fn pedido(venta_id: i64, tipo: &str) -> Json<EmitirComprobanteRequest> {
    let factura = tipo == "FACTURA";
    Json(EmitirComprobanteRequest {
        venta_id,
        tipo: tipo.to_string(),
        cliente_documento: Some(if factura { "20000000001" } else { "12345678" }.to_string()),
        cliente_nombre: Some(if factura { "EMPRESA CLIENTE S.A.C." } else { "CLIENTE DE PRUEBA" }.to_string()),
        detraccion: None,
    })
}

#[tokio::test]
#[ignore]
async fn emitir_boletas_y_factura_de_punta_a_punta() {
    assert!(std::env::var("LYCET_URL").is_ok(), "falta LYCET_URL");
    let db = base_de_prueba().await;
    let tenant = Arc::new(TenantDb(db.clone()));

    // Dos boletas seguidas: números 1 y 2 de la serie BT01.
    for (i, folio) in ["V-1", "V-2"].iter().enumerate() {
        let id = venta(&db, folio).await;
        let Json(r) = emitir_comprobante(Extension(tenant.clone()), pedido(id, "BOLETA")).await.expect("emisión");
        println!("{}-{} {} {}", r.serie, r.numero, r.estado, r.mensaje);
        assert!(r.success, "{}", r.mensaje);
        assert_eq!(r.serie, "BT01");
        assert_eq!(r.numero, i as i64 + 1);
        assert!(r.hash.is_some());

        // Volver a emitir la misma venta no crea otro comprobante.
        let repetido = emitir_comprobante(Extension(tenant.clone()), pedido(id, "BOLETA")).await;
        assert_eq!(repetido.err().map(|e| e.0.as_u16()), Some(409));
    }

    // Factura: su propia serie, empieza en 1.
    let id = venta(&db, "V-3").await;
    let Json(r) = emitir_comprobante(Extension(tenant.clone()), pedido(id, "FACTURA")).await.expect("emisión");
    println!("{}-{} {} {}", r.serie, r.numero, r.estado, r.mensaje);
    assert!(r.success, "{}", r.mensaje);
    assert_eq!((r.serie.as_str(), r.numero), ("FT01", 1));

    // XML y CDR quedaron guardados en la base.
    let conn = db.connect().unwrap();
    let mut filas = conn
        .query(
            "SELECT COUNT(*) FROM comprobante_archivos WHERE xml LIKE '%<Invoice%' AND length(cdr_zip) > 100",
            (),
        )
        .await
        .unwrap();
    let guardados: i64 = filas.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(guardados, 3);
    drop(filas);
    drop(conn);

    // Las descargas de Comprobantes sirven el XML y el CDR guardados.
    use axum::extract::Path;
    use minimarket_backend::handlers::comprobantes::{descargar_cdr, descargar_xml};
    let xml = descargar_xml(Extension(tenant.clone()), Path(1)).await.expect("xml");
    assert_eq!(xml.headers()["content-type"], "application/xml");
    assert!(xml.headers()["content-disposition"].to_str().unwrap().contains("20161515648-03-BT01-1.xml"));
    let cdr = descargar_cdr(Extension(tenant.clone()), Path(1)).await.expect("cdr");
    assert_eq!(cdr.headers()["content-type"], "application/zip");
    assert!(cdr.headers()["content-disposition"].to_str().unwrap().contains("R-20161515648-03-BT01-1.zip"));

    // Sin conexión con Lycet: queda PENDIENTE con su número reservado, y al
    // volver a emitir se reenvía con el mismo número.
    let url = std::env::var("LYCET_URL").unwrap();
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    let id = venta(&db, "V-4").await;
    let Json(r) = emitir_comprobante(Extension(tenant.clone()), pedido(id, "BOLETA")).await.expect("emisión");
    println!("{}-{} {} {}", r.serie, r.numero, r.estado, r.mensaje);
    assert_eq!((r.estado.as_str(), r.numero), ("PENDIENTE", 3));
    assert!(!r.mensaje.contains("token"), "el mensaje no debe mostrar el token: {}", r.mensaje);
    std::env::set_var("LYCET_URL", url);
    let Json(r) = emitir_comprobante(Extension(tenant.clone()), pedido(id, "BOLETA")).await.expect("reenvío");
    println!("{}-{} {} {}", r.serie, r.numero, r.estado, r.mensaje);
    assert!(r.success, "{}", r.mensaje);
    assert_eq!(r.numero, 3);
}

/// Reenvío de pendientes (botón "Reenviar" y tarea automática) y avisos.
///   LYCET_URL=http://127.0.0.1:8000 LYCET_TOKEN=... \
///   cargo test --test emision_directa reenvios -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn reenvios_y_avisos() {
    use axum::extract::Path;
    use minimarket_backend::handlers::envios_sunat::{avisos, reenviar};

    let url = std::env::var("LYCET_URL").expect("falta LYCET_URL");
    let db = base_de_prueba().await;
    let tenant = Arc::new(TenantDb(db.clone()));

    // Sin conexión: una boleta y una factura quedan PENDIENTES.
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    let boleta = venta(&db, "R-1").await;
    let Json(b) = emitir_comprobante(Extension(tenant.clone()), pedido(boleta, "BOLETA")).await.expect("boleta");
    let factura = venta(&db, "R-2").await;
    let Json(f) = emitir_comprobante(Extension(tenant.clone()), pedido(factura, "FACTURA")).await.expect("factura");
    assert_eq!(b.estado, "PENDIENTE");
    assert_eq!(f.estado, "PENDIENTE");

    let Json(a) = avisos(Extension(tenant.clone())).await.expect("avisos");
    assert!(a.modo_directo);
    assert_eq!(a.pendientes, 2);
    assert_eq!(a.avisos[0].dias_restantes, 3);

    // Vuelve la conexión: "Reenviar" los manda con el mismo número.
    std::env::set_var("LYCET_URL", &url);
    for (id, nombre) in [(b.comprobante_id.unwrap(), "boleta"), (f.comprobante_id.unwrap(), "factura")] {
        let Json(r) = reenviar(Extension(tenant.clone()), Path(id)).await.expect("reenvío");
        println!("{} reenviada: {} {}", nombre, r.estado, r.mensaje);
        assert_eq!(r.estado, "ACEPTADO", "{}", r.mensaje);
        // Ya no está pendiente: un segundo reenvío se rechaza.
        let otra = reenviar(Extension(tenant.clone()), Path(id)).await;
        assert_eq!(otra.err().map(|e| e.0.as_u16()), Some(409));
    }
    let conn = db.connect().unwrap();
    let mut filas = conn
        .query("SELECT SUM(intentos), COUNT(ultimo_intento) FROM comprobantes_electronicos", ())
        .await
        .unwrap();
    let fila = filas.next().await.unwrap().unwrap();
    assert_eq!((fila.get::<i64>(0).unwrap(), fila.get::<i64>(1).unwrap()), (2, 2));
    drop(filas);

    drop(fila);
    let Json(a) = avisos(Extension(tenant.clone())).await.expect("avisos");
    assert_eq!(a.pendientes, 0);

    // Se perdió la respuesta del primer envío: el comprobante quedó
    // PENDIENTE aunque SUNAT sí lo recibió. Al reenviarlo igual, SUNAT
    // dice que ya lo tiene y queda aceptado.
    let id = b.comprobante_id.unwrap();
    conn.execute(
        "UPDATE comprobantes_electronicos SET estado = 'PENDIENTE', envio_incierto = 1, mensaje_sunat = 'sin respuesta' WHERE id = ?1",
        libsql::params![id],
    )
        .await
        .unwrap();
    drop(conn);
    let Json(r) = reenviar(Extension(tenant.clone()), Path(id)).await.expect("reenvío");
    println!("boleta reenviada otra vez: {} {}", r.estado, r.mensaje);
    assert_eq!(r.estado, "ACEPTADO", "{}", r.mensaje);
}

fn claims() -> minimarket_backend::models::auth::Claims {
    minimarket_backend::models::auth::Claims {
        sub: 1,
        username: "prueba".into(),
        rol_id: 1,
        nombre_completo: "Cajero de prueba".into(),
        tienda_id: 1,
        exp: usize::MAX,
    }
}

/// Notas de crédito contra SUNAT beta: parcial de boleta, total de factura,
/// boleta sin cliente y la nota automática de una devolución.
///   cargo test --test emision_directa notas -- --ignored --nocapture --test-threads=1
#[tokio::test]
#[ignore]
async fn notas_de_credito() {
    use axum::extract::Path;
    use minimarket_backend::handlers::devoluciones::procesar_devolucion;
    use minimarket_backend::handlers::notas_credito::{emitir, preparar, PedidoNota};
    use minimarket_backend::logica::notas_credito::LineaNota;
    use minimarket_backend::models::devolucion::{NuevaDevolucion, ProductoDevolver};

    assert!(std::env::var("LYCET_URL").is_ok(), "falta LYCET_URL");
    let db = base_de_prueba().await;
    let tenant = Arc::new(TenantDb(db.clone()));
    let emitir_venta = |folio: &'static str, tipo: &'static str| {
        let db = db.clone();
        let tenant = tenant.clone();
        async move {
            let id = venta(&db, folio).await;
            let Json(r) = emitir_comprobante(Extension(tenant), pedido(id, tipo)).await.expect("emisión");
            assert!(r.success, "{}", r.mensaje);
            (id, r.comprobante_id.unwrap())
        }
    };

    // Boleta: devolución de un polo (07).
    let (_, boleta) = emitir_venta("N-1", "BOLETA").await;
    let Json(p) = preparar(Extension(tenant.clone()), Path(boleta)).await.expect("preparar");
    assert_eq!((p.serie.as_str(), p.puede_total, p.lineas.len()), ("BC01", true, 2));
    let Json(n) = emitir(
        Extension(tenant.clone()),
        Extension(claims()),
        Path(boleta),
        Json(PedidoNota { motivo_codigo: "07".into(), motivo: Some("Cambio de talla".into()), lineas: vec![LineaNota { indice: 0, cantidad: 1.0 }] }),
    )
    .await
    .expect("nota parcial");
    println!("boleta {} -> nota {}-{} {} {} (S/ {})", n.documento_afectado, n.serie, n.numero, n.estado, n.mensaje, n.total);
    assert_eq!(n.estado, "ACEPTADO", "{}", n.mensaje);
    assert_eq!(n.total, 59.0);
    // Lo que queda: 1 polo y las medias; ya no se puede por el total.
    let Json(p) = preparar(Extension(tenant.clone()), Path(boleta)).await.expect("preparar");
    assert!(!p.puede_total);
    assert_eq!(p.acreditado, 59.0);
    assert_eq!(p.lineas[0].disponible, 1.0);
    let otra = emitir(
        Extension(tenant.clone()),
        Extension(claims()),
        Path(boleta),
        Json(PedidoNota { motivo_codigo: "07".into(), motivo: None, lineas: vec![LineaNota { indice: 0, cantidad: 2.0 }] }),
    )
    .await;
    assert_eq!(otra.err().map(|e| e.0.as_u16()), Some(400));

    // Factura: anulación de la operación (01), serie FC01.
    let (_, factura) = emitir_venta("N-2", "FACTURA").await;
    let Json(n) = emitir(
        Extension(tenant.clone()),
        Extension(claims()),
        Path(factura),
        Json(PedidoNota { motivo_codigo: "01".into(), motivo: Some("Error en el RUC del cliente".into()), lineas: vec![] }),
    )
    .await
    .expect("nota total");
    println!("factura {} -> nota {}-{} {} {} (S/ {})", n.documento_afectado, n.serie, n.numero, n.estado, n.mensaje, n.total);
    assert_eq!(n.estado, "ACEPTADO", "{}", n.mensaje);
    assert_eq!((n.serie.as_str(), n.numero, n.total), ("FC01", 1, 153.4));

    // Boleta sin cliente (CLIENTES VARIOS): devolución total (06).
    let id = venta(&db, "N-3").await;
    let mut sin_cliente = pedido(id, "BOLETA");
    sin_cliente.cliente_documento = None;
    sin_cliente.cliente_nombre = None;
    let Json(r) = emitir_comprobante(Extension(tenant.clone()), sin_cliente).await.expect("emisión");
    assert!(r.success, "{}", r.mensaje);
    let Json(n) = emitir(
        Extension(tenant.clone()),
        Extension(claims()),
        Path(r.comprobante_id.unwrap()),
        Json(PedidoNota { motivo_codigo: "06".into(), motivo: None, lineas: vec![] }),
    )
    .await
    .expect("nota sin cliente");
    println!("boleta sin cliente {} -> nota {}-{} {} {}", n.documento_afectado, n.serie, n.numero, n.estado, n.mensaje);
    assert_eq!(n.estado, "ACEPTADO", "{}", n.mensaje);

    // Devolución de las medias de N-1: la nota sale sola.
    let conn = db.connect().unwrap();
    let mut filas = conn
        .query("SELECT id, producto_id FROM detalles_venta WHERE venta_id = (SELECT id FROM ventas WHERE folio = 'N-1') ORDER BY id", ())
        .await
        .unwrap();
    let mut detalles = Vec::new();
    while let Some(f) = filas.next().await.unwrap() {
        detalles.push((f.get::<i64>(0).unwrap(), f.get::<i64>(1).unwrap()));
    }
    drop(filas);
    let venta_n1: i64 = {
        let mut f = conn.query("SELECT id FROM ventas WHERE folio = 'N-1'", ()).await.unwrap();
        f.next().await.unwrap().unwrap().get(0).unwrap()
    };
    drop(conn);
    let Json(d) = procesar_devolucion(
        Extension(tenant.clone()),
        Extension(claims()),
        Json(NuevaDevolucion {
            venta_id: venta_n1,
            productos: vec![ProductoDevolver { detalle_id: detalles[1].0, producto_id: detalles[1].1, cantidad: 1.0 }],
            motivo: "Medias con falla".into(),
            usuario_id: 1,
            metodo_reembolso: None,
        }),
    )
    .await
    .expect("devolución");
    let nota = d.nota_credito.expect("la venta tiene boleta directa");
    println!("devolución -> {:?}", nota);
    let nota = nota.nota.expect("nota emitida");
    assert_eq!(nota.estado, "ACEPTADO", "{}", nota.mensaje);
    assert_eq!((nota.serie.as_str(), nota.numero, nota.total), ("BC01", 3, 35.4));
}

/// Anulaciones contra SUNAT beta: resumen diario (boletas) y comunicación
/// de baja (factura). La venta queda devuelta (stock de vuelta).
///   cargo test --test emision_directa anulaciones -- --ignored --nocapture --test-threads=1
#[tokio::test]
#[ignore]
async fn anulaciones() {
    use axum::extract::Path;
    use minimarket_backend::handlers::anulaciones::{anular, consultar, preparar, PedidoAnulacion};

    assert!(std::env::var("LYCET_URL").is_ok(), "falta LYCET_URL");
    let db = base_de_prueba().await;
    let tenant = Arc::new(TenantDb(db.clone()));
    let stock = |db: Arc<libsql::Database>| async move {
        let conn = db.connect().unwrap();
        let mut f = conn.query("SELECT CAST(stock AS REAL) FROM productos WHERE id = 901", ()).await.unwrap();
        f.next().await.unwrap().unwrap().get::<f64>(0).unwrap()
    };

    let mut casos = Vec::new();
    for (folio, tipo, con_cliente) in [("A-1", "BOLETA", true), ("A-2", "BOLETA", false), ("A-3", "FACTURA", true)] {
        let id = venta(&db, folio).await;
        let mut p = pedido(id, tipo);
        if !con_cliente {
            p.cliente_documento = None;
            p.cliente_nombre = None;
        }
        let Json(r) = emitir_comprobante(Extension(tenant.clone()), p).await.expect("emisión");
        assert!(r.success, "{}", r.mensaje);
        casos.push((folio, r.comprobante_id.unwrap()));
    }

    for (folio, comprobante) in casos {
        let Json(info) = preparar(Extension(tenant.clone()), Path(comprobante)).await.expect("preparar");
        assert!(info.puede, "{:?}", info.motivo_no);
        assert_eq!(info.monto_devolver, 153.4);
        let antes = stock(db.clone()).await;
        let Json(r) = anular(
            Extension(tenant.clone()),
            Extension(claims()),
            Path(comprobante),
            Json(PedidoAnulacion { motivo: "Error al registrar la venta".into(), metodo_reembolso: None }),
        )
        .await
        .expect("anular");
        println!("{} {} -> {} {} (devolución {:?})", folio, info.tipo.unwrap_or_default(), r.identificador, r.anulacion, r.folio_devolucion);
        println!("   {}", r.mensaje);
        assert!(r.folio_devolucion.is_some());
        assert_eq!(stock(db.clone()).await, antes + 2.0, "vuelven los 2 polos");
        let mut estado = r.anulacion.clone();
        for _ in 0..5 {
            if estado != "EN_PROCESO" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            let Json(c) = consultar(Extension(tenant.clone()), Path(comprobante)).await.expect("consultar");
            println!("   consulta: {} {}", c.anulacion, c.mensaje);
            estado = c.anulacion;
        }
        assert_eq!(estado, "ANULADO");
        // Ya no se puede anular de nuevo ni emitir nota sobre lo anulado.
        let Json(otra) = preparar(Extension(tenant.clone()), Path(comprobante)).await.expect("preparar");
        assert!(!otra.puede);
        let nota = minimarket_backend::handlers::notas_credito::preparar(Extension(tenant.clone()), Path(comprobante)).await;
        assert_eq!(nota.err().map(|e| e.0.as_u16()), Some(400));
    }
}

/// Guía de remisión directa por la API de guías (en Lycet de prueba apunta
/// al ambiente de pruebas que trae configurado). Transporte privado y
/// público, con la factura de la venta como documento relacionado.
///   cargo test --test emision_directa guias -- --ignored --nocapture --test-threads=1
#[tokio::test]
#[ignore]
async fn guias_directas() {
    use axum::extract::Path;
    use minimarket_backend::handlers::guias::{consultar, crear};
    use minimarket_backend::logica::guias::{Chofer, DatosGuia, Direccion, Transportista};

    assert!(std::env::var("LYCET_URL").is_ok(), "falta LYCET_URL");
    let db = base_de_prueba().await;
    let tenant = Arc::new(TenantDb(db.clone()));
    db.connect()
        .unwrap()
        .execute_batch("UPDATE configuracion_tienda SET modulos = 'GUIAS', serie_guia = 'T001', ubigeo = '150101';")
        .await
        .unwrap();
    let id = venta(&db, "G-1").await;
    let Json(f) = emitir_comprobante(Extension(tenant.clone()), pedido(id, "FACTURA")).await.expect("factura");
    assert!(f.success, "{}", f.mensaje);

    let manana = minimarket_backend::logica::tiempo::hoy_lima_mas_dias(1);
    let base = DatosGuia {
        venta_id: Some(id),
        destinatario_tipo: "RUC".into(),
        destinatario_documento: "20000000001".into(),
        destinatario_nombre: "EMPRESA CLIENTE S.A.C.".into(),
        motivo: "01".into(),
        motivo_descripcion: None,
        modo: "PRIVADO".into(),
        fecha_traslado: manana,
        peso_total: 2.5,
        bultos: 1,
        partida: Direccion { ubigeo: "150101".into(), direccion: "JR. MONTEVIDEO 752".into() },
        llegada: Direccion { ubigeo: "150122".into(), direccion: "AV. LARCO 345".into() },
        transportista: None,
        chofer: Some(Chofer { documento: "41784439".into(), nombres: "JUAN".into(), apellidos: "PEREZ".into(), licencia: "Q41784439".into() }),
        placa: Some("ABC123".into()),
        observaciones: Some("Prueba de guía directa".into()),
        items: vec![],
    };
    let mut publico = base.clone();
    publico.modo = "PUBLICO".into();
    publico.chofer = None;
    publico.placa = None;
    publico.transportista = Some(Transportista { ruc: "20600000001".into(), nombre: "TRANSPORTES PRUEBA SAC".into(), mtc: None });

    for (nombre, datos) in [("privado", base), ("público", publico)] {
        let Json(g) = crear(Extension(tenant.clone()), Extension(claims()), Json(datos)).await.expect("crear guía");
        println!("guía {} ({}): {} · {:?}", g.numero, nombre, g.estado, g.mensaje);
        let mut estado = g.estado.clone();
        for _ in 0..5 {
            if estado != "ENVIADA" && estado != "REGISTRADA" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            let Json(c) = consultar(Extension(tenant.clone()), Path(g.id)).await.expect("consultar");
            println!("   consulta: {} · {:?} · qr {:?}", c.estado, c.mensaje, c.qr);
            estado = c.estado;
        }
        // El ambiente de prueba de guías (no es SUNAT) rechaza el transporte
        // público con 3617 aunque la guía esté completa: el privado sí se
        // verifica; el público se confirma con la primera guía real.
        if nombre == "privado" {
            assert_eq!(estado, "ACEPTADA");
        }
    }
}

/// Sin red: el botón "Reenviar" y la tarea automática no pueden mandar el
/// mismo documento dos veces a la vez; un envío colgado se libera solo.
#[tokio::test]
async fn reclamo_de_envios() {
    use minimarket_backend::handlers::envios_sunat::reclamar;

    let db = base_de_prueba().await;
    let conn = db.connect().unwrap();
    let ahora = minimarket_backend::logica::tiempo::ahora_lima();
    conn.execute(
        "INSERT INTO ventas (id, folio, subtotal, total, metodo_pago, usuario_id, estado) VALUES (50, 'C-1', 10, 10, 'EFECTIVO', 1, 'COMPLETADA')",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO comprobantes_electronicos (id, venta_id, tipo, proveedor, serie, numero, estado, mensaje_sunat, fecha_emision)
         VALUES (7, 50, 'BOLETA', 'SUNAT_DIRECTO', 'BT01', 99, 'PENDIENTE', 'Enviando a SUNAT...', ?1)",
        libsql::params![ahora],
    )
    .await
    .unwrap();
    let tomar = || reclamar(&conn, "comprobantes_electronicos", "estado = 'PENDIENTE'", "fecha_emision", "mensaje_sunat", 7);

    // El primer envío sigue en curso: nadie más lo toma.
    assert!(!tomar().await.unwrap());
    // El primer envío terminó sin respuesta: se puede reenviar de inmediato.
    conn.execute("UPDATE comprobantes_electronicos SET mensaje_sunat = 'Sin conexión' WHERE id = 7", ()).await.unwrap();
    assert!(tomar().await.unwrap());
    // Ese reenvío está en curso: un segundo no lo toma.
    assert!(!tomar().await.unwrap());
    // Un reenvío colgado (más de 90 segundos) se libera.
    conn.execute("UPDATE comprobantes_electronicos SET ultimo_intento = '2026-01-01 00:00:00' WHERE id = 7", ()).await.unwrap();
    assert!(tomar().await.unwrap());
    // Lo que ya no está pendiente nunca se toma.
    conn.execute("UPDATE comprobantes_electronicos SET estado = 'ACEPTADO', mensaje_sunat = 'ok' WHERE id = 7", ()).await.unwrap();
    assert!(!tomar().await.unwrap());
}
