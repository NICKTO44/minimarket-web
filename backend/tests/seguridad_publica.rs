//! Rutas públicas (sin login): enlace firmado de comprobantes y tope de
//! registros. Levanta el servidor de verdad en un puerto local, con la base
//! central en un archivo local. No toca Turso ni bases reales.
//!
//!   cargo test --test seguridad_publica

use std::sync::Arc;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use minimarket_backend::{crypto, estado_impresion, handlers, rate_limit, tenants, AppState};

const SECRETO: &str = "secreto-de-prueba-para-los-enlaces-0123456789";

async fn base_central() -> (libsql::Database, [u8; 32]) {
    static SIGUIENTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SIGUIENTE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ruta = std::env::temp_dir().join(format!("central_seguridad_{}_{}.db", std::process::id(), n));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.expect("base central local");
    let clave = [7u8; 32];
    let token = crypto::cifrar("token-falso", &clave).unwrap();
    let conn = db.connect().unwrap();
    conn.execute_batch(
        "CREATE TABLE tiendas (id INTEGER PRIMARY KEY, nombre_negocio TEXT, identificador TEXT UNIQUE,
            turso_db_url TEXT, turso_db_token TEXT, estado TEXT, fecha_vencimiento TEXT, slug TEXT);",
    )
    .await
    .unwrap();
    // La base del negocio apunta a un puerto donde no hay nada: si el
    // enlace pasa la firma, el handler intenta conectarse y falla (500).
    conn.execute(
        "INSERT INTO tiendas VALUES (42, 'Verane prueba', 'verane-e8de', 'http://127.0.0.1:9', ?1, 'ACTIVO', NULL, 'verane')",
        libsql::params![token],
    )
    .await
    .unwrap();
    (db, clave)
}

async fn levantar(max_por_ip: usize, max_total: usize) -> String {
    std::env::set_var("JWT_SECRET", SECRETO);
    let (central, clave) = base_central().await;
    let db = libsql::Builder::new_local(":memory:").build().await.unwrap();
    let state = Arc::new(AppState {
        db,
        tiendas: tenants::RegistroTiendas::nuevo(central, clave),
        estado_impresion: estado_impresion::EstadoImpresion::nuevo(),
        limitador_login: rate_limit::LimitadorIntentos::estricto(),
        limitador_verificar: rate_limit::LimitadorIntentos::laxo(),
        limitador_registro: rate_limit::LimitadorRegistro::con_topes(max_por_ip, max_total),
        jwt_secret: SECRETO.to_string(),
    });

    // Un registro "falso" con el mismo middleware de producción: responde 400
    // si el usuario es "repetido" (error del formulario) y 200 si no.
    async fn registro_falso(body: String) -> (StatusCode, &'static str) {
        if body.contains("repetido") {
            (StatusCode::BAD_REQUEST, "Ese nombre de usuario ya está en uso")
        } else {
            (StatusCode::OK, "{\"ok\":true}")
        }
    }

    let app = Router::new()
        .route(
            "/publico/comprobante/:identificador/:id",
            get(handlers::publico::ver_comprobante_publico),
        )
        .merge(
            Router::new()
                .route("/registro", post(registro_falso))
                .route_layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit::limitar_registro)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let direccion = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{}", direccion)
}

#[tokio::test]
async fn el_comprobante_publico_exige_la_firma_del_enlace() {
    let base = levantar(3, 20).await;
    let cliente = reqwest::Client::new();
    let clave = minimarket_backend::logica::enlace_publico::clave(42, 15).expect("clave");

    // Enlace antiguo (sin firma), firma de otro comprobante y firma inventada: 404.
    let otra = minimarket_backend::logica::enlace_publico::clave(42, 16).unwrap();
    for url in [
        format!("{base}/publico/comprobante/verane-e8de/15"),
        format!("{base}/publico/comprobante/verane-e8de/15?k={otra}"),
        format!("{base}/publico/comprobante/verane-e8de/15?k=AAAAAAAAAAAAAAAAAAAAAA"),
        format!("{base}/publico/comprobante/verane-e8de/16?k={clave}"),
        format!("{base}/publico/comprobante/otro-negocio/15?k={clave}"),
    ] {
        let r = cliente.get(&url).send().await.unwrap();
        assert_eq!(r.status(), 404, "{url}");
        let texto = r.text().await.unwrap();
        assert!(texto.contains("no es válido"), "mismo mensaje en todos los casos: {texto}");
    }

    // Con la firma correcta pasa el control y va a la base del negocio (que
    // aquí no existe): ya no es 404.
    let r = cliente
        .get(format!("{base}/publico/comprobante/verane-e8de/15?k={clave}"))
        .send()
        .await
        .unwrap();
    assert_ne!(r.status(), 404, "con la firma correcta no debe rechazarse");
}

#[tokio::test]
async fn el_registro_tiene_tope_por_ip_y_no_se_salta_cambiando_la_cabecera() {
    let base = levantar(2, 4).await;
    let cliente = reqwest::Client::new();
    let registrar = |xff: &'static str, cuerpo: &'static str| {
        cliente
            .post(format!("{base}/registro"))
            .header("x-forwarded-for", xff)
            .body(cuerpo)
            .send()
    };

    // Misma IP real (la que agrega Nginx al final) con "IPs" inventadas
    // delante: cuenta como la misma.
    assert_eq!(registrar("1.1.1.1, 190.0.0.1", "nuevo").await.unwrap().status(), 200);
    assert_eq!(registrar("2.2.2.2, 190.0.0.1", "nuevo").await.unwrap().status(), 200);
    assert_eq!(registrar("3.3.3.3, 190.0.0.1", "nuevo").await.unwrap().status(), 429);

    // Un error del formulario no gasta cupo.
    assert_eq!(registrar("190.0.0.2", "repetido").await.unwrap().status(), 400);
    assert_eq!(registrar("190.0.0.2", "repetido").await.unwrap().status(), 400);
    assert_eq!(registrar("190.0.0.2", "repetido").await.unwrap().status(), 400);
    assert_eq!(registrar("190.0.0.2", "nuevo").await.unwrap().status(), 200);

    // Tope total del día (4): ya van 3; uno más entra y el siguiente, de
    // otra IP, ya no.
    assert_eq!(registrar("190.0.0.3", "nuevo").await.unwrap().status(), 200);
    let r = registrar("190.0.0.4", "nuevo").await.unwrap();
    assert_eq!(r.status(), 429);
    assert!(r.text().await.unwrap().contains("Hoy no se pueden crear"));
}
