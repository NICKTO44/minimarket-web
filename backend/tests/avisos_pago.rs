//! Módulo "Avisos de Yape y Plin" (AVISOS_PAGO): vincular celulares, recibir
//! la notificación del Yape y avisar al celular de la tienda.
//!
//!   cargo test --test avisos_pago

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use minimarket_backend::handlers::avisos_pago::{self, FiltroEsperar, NotificacionEntrante, NuevoDispositivo};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;
use minimarket_backend::{crypto, estado_impresion, rate_limit, tenants, AppState};

async fn local(nombre: &str) -> libsql::Database {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ruta = std::env::temp_dir().join(format!("avisos_{}_{}_{}.db", nombre, std::process::id(), n));
    let _ = std::fs::remove_file(&ruta);
    libsql::Builder::new_local(&ruta).build().await.unwrap()
}

async fn montar(modulos: &str) -> (Arc<AppState>, Arc<TenantDb>) {
    let central = local("central").await;
    let clave = [7u8; 32];
    let token = crypto::cifrar("token-falso", &clave).unwrap();
    central
        .connect()
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE tiendas (id INTEGER PRIMARY KEY, nombre_negocio TEXT, identificador TEXT UNIQUE,
                turso_db_url TEXT, turso_db_token TEXT, estado TEXT, fecha_vencimiento TEXT, slug TEXT);
             INSERT INTO tiendas VALUES (42, 'Licorería prueba', 'lico-42', 'http://127.0.0.1:9', '{}', 'ACTIVO', NULL, 'lico');",
            token
        ))
        .await
        .unwrap();

    let db = local(&format!("negocio_{}", modulos.len())).await;
    let conn = db.connect().unwrap();
    conn.execute_batch(include_str!("../schema.sql")).await.unwrap();
    let mut migraciones: Vec<_> = std::fs::read_dir("migraciones")
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false))
        .collect();
    migraciones.sort();
    for m in &migraciones {
        conn.execute_batch(&std::fs::read_to_string(m).unwrap()).await.unwrap();
    }
    conn.execute("UPDATE configuracion_tienda SET modulos = ?1", libsql::params![modulos]).await.unwrap();
    let db = Arc::new(db);

    let state = Arc::new(AppState {
        db: libsql::Builder::new_local(":memory:").build().await.unwrap(),
        tiendas: tenants::RegistroTiendas::nuevo(central, clave),
        estado_impresion: estado_impresion::EstadoImpresion::nuevo(),
        limitador_login: rate_limit::LimitadorIntentos::estricto(),
        limitador_verificar: rate_limit::LimitadorIntentos::laxo(),
        limitador_registro: rate_limit::LimitadorRegistro::con_topes(5, 50),
        jwt_secret: "x".into(),
    });
    state.tiendas.usar_conexion(42, db.clone()).await;
    (state, Arc::new(TenantDb(db)))
}

fn admin() -> Claims {
    Claims { sub: 1, username: "sam".into(), rol_id: 1, nombre_completo: "Sam".into(), tienda_id: 42, exp: usize::MAX, tv: 0 }
}
fn cajera() -> Claims {
    Claims { sub: 2, username: "caja".into(), rol_id: 2, ..admin() }
}

fn con_clave(clave: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", clave)).unwrap());
    h
}

async fn vincular(state: &Arc<AppState>, t: &Arc<TenantDb>, claims: Claims, modo: &str) -> Result<String, (StatusCode, String)> {
    avisos_pago::vincular(
        State(state.clone()),
        Extension(t.clone()),
        Extension(claims),
        Json(NuevoDispositivo { nombre: format!("Celular {}", modo), modo: modo.into() }),
    )
    .await
    .map(|r| r.0.clave)
}

fn notif(texto: &str, ms: i64) -> Json<NotificacionEntrante> {
    Json(NotificacionEntrante {
        paquete: "com.bcp.innovacxion.yapeapp".into(),
        titulo: "Confirmación de Pago".into(),
        texto: texto.into(),
        publicado_ms: Some(ms),
    })
}

#[tokio::test]
async fn yape_llega_a_la_tienda_al_instante() {
    let (state, t) = montar("AVISOS_PAGO").await;

    // Solo el administrador vincula celulares.
    assert_eq!(vincular(&state, &t, cajera(), "YAPE").await.unwrap_err().0, StatusCode::FORBIDDEN);
    let yape = vincular(&state, &t, admin(), "YAPE").await.unwrap();
    let tienda = vincular(&state, &t, admin(), "TIENDA").await.unwrap();
    assert!(vincular(&state, &t, admin(), "OTRO").await.is_err());

    let yo = avisos_pago::yo(State(state.clone()), con_clave(&tienda)).await.unwrap().0;
    assert_eq!((yo.negocio.as_str(), yo.modo.as_str()), ("Licorería prueba", "TIENDA"));

    // Al abrir, la tienda solo pide el último id (no repite lo de antes).
    let inicio = avisos_pago::esperar(State(state.clone()), con_clave(&tienda), Query(FiltroEsperar { despues: -1 }))
        .await
        .unwrap()
        .0;
    assert_eq!(inicio.ultimo_id, 0);

    // La tienda queda esperando; el Yape llega 300 ms después.
    let st = state.clone();
    let tk = tienda.clone();
    let espera = tokio::spawn(async move {
        let t0 = Instant::now();
        let r = avisos_pago::esperar(State(st), con_clave(&tk), Query(FiltroEsperar { despues: 0 })).await.unwrap().0;
        (r, t0.elapsed())
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let r = avisos_pago::recibir(
        State(state.clone()),
        con_clave(&yape),
        notif("Yape! CARLOS P. te envió un pago por S/ 2. El cód. de seguridad es: 518", 1_700_000_000_000),
    )
    .await
    .unwrap()
    .0;
    assert!(r.nuevo);
    let (llego, tardo) = espera.await.unwrap();
    assert!(tardo < Duration::from_secs(3), "tardó {:?}", tardo);
    assert_eq!(llego.pagos.len(), 1);
    let p = &llego.pagos[0];
    assert_eq!((p.monto, p.nombre.as_deref(), p.codigo_seguridad.as_deref()), (Some(2.0), Some("CARLOS P."), Some("518")));
    assert!(p.reconocido);
    assert_eq!(p.dispositivo.as_deref(), Some("Celular YAPE"));

    // La misma notificación otra vez (reenvío): no se repite.
    let otra = avisos_pago::recibir(
        State(state.clone()),
        con_clave(&yape),
        notif("Yape! CARLOS P. te envió un pago por S/ 2. El cód. de seguridad es: 518", 1_700_000_030_000),
    )
    .await
    .unwrap()
    .0;
    assert!(!otra.nuevo);

    // La estafa del "solicitó un pago" se guarda, pero no como pago.
    let estafa = avisos_pago::recibir(
        State(state.clone()),
        con_clave(&yape),
        notif("LUIS R. ha solicitado un pago por S/ 100", 1_700_000_100_000),
    )
    .await
    .unwrap()
    .0;
    assert!(!estafa.pago.unwrap().reconocido);

    // El celular de la tienda no puede mandar pagos.
    let e = avisos_pago::recibir(State(state.clone()), con_clave(&tienda), notif("x te envió S/ 1", 1)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);

    // La lista de hoy (app y Monspeet web) trae los dos.
    let hoy = avisos_pago::hoy(Extension(t.clone())).await.unwrap().0;
    assert_eq!(hoy.len(), 2);
    let hoy_app = avisos_pago::hoy_dispositivo(State(state.clone()), con_clave(&tienda)).await.unwrap().0;
    assert_eq!(hoy_app.len(), 2);

    // Sin pagos nuevos, esperar responde vacío (a los 25 s; aquí se corta antes).
    let ultimo = hoy[0].id;
    let vacia = tokio::time::timeout(
        Duration::from_millis(500),
        avisos_pago::esperar(State(state.clone()), con_clave(&tienda), Query(FiltroEsperar { despues: ultimo })),
    )
    .await;
    assert!(vacia.is_err(), "no debía responder sin pagos nuevos");

    // Quitar el celular: su clave deja de valer.
    let lista = avisos_pago::dispositivos(Extension(t.clone()), Extension(admin())).await.unwrap().0;
    assert_eq!(lista.len(), 2);
    let id_yape = lista.iter().find(|d| d.modo == "YAPE").unwrap().id;
    avisos_pago::quitar(Extension(t.clone()), Extension(admin()), Path(id_yape)).await.unwrap();
    let e = avisos_pago::recibir(State(state.clone()), con_clave(&yape), notif("Yape! A te envió S/ 1", 9)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::UNAUTHORIZED);

    // Clave inventada o de otro celular: fuera.
    let falsa = format!("42.{}.{}", id_yape + 1, "0".repeat(48));
    assert_eq!(avisos_pago::yo(State(state.clone()), con_clave(&falsa)).await.unwrap_err().0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sin_el_modulo_no_se_vincula_ni_se_recibe() {
    let (state, t) = montar("").await;
    assert!(vincular(&state, &t, admin(), "YAPE").await.is_err());
}
