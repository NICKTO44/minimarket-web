//! Contraseñas y sesiones (handlers/cuenta.rs, panel y token_version).
//! Base local (schema.sql + todas las migraciones), los handlers de verdad.
//! No toca Turso.
//!
//!   cargo test --test contrasenas

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use minimarket_backend::handlers::{configuracion, cuenta};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::configuracion::NuevoUsuario;
use minimarket_backend::tenants::{self, TenantDb};
use minimarket_backend::{estado_impresion, rate_limit, AppState};

const SECRETO: &str = "secreto-de-prueba-para-las-sesiones-0123456789";
const CLAVE_ADMIN: &str = "clave-del-dueno-1";
const CLAVE_CAJERO: &str = "clave-del-cajero-1";

async fn base(nombre: &str) -> Arc<libsql::Database> {
    let ruta = std::env::temp_dir().join(format!("contrasenas_{}_{}.db", nombre, std::process::id()));
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
    // Costo bajo solo para que el test no tarde (bcrypt::verify lee el costo
    // del propio hash).
    let admin = bcrypt::hash(CLAVE_ADMIN, 4).unwrap();
    let cajero = bcrypt::hash(CLAVE_CAJERO, 4).unwrap();
    conn.execute(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES
            (1, 'dueno', ?1, 'Dueño de prueba', 1),
            (2, 'cajero', ?2, 'Cajero de prueba', 2)",
        libsql::params![admin, cajero],
    )
    .await
    .expect("usuarios de prueba");
    Arc::new(db)
}

async fn estado() -> Arc<AppState> {
    let central = libsql::Builder::new_local(":memory:").build().await.unwrap();
    let db = libsql::Builder::new_local(":memory:").build().await.unwrap();
    Arc::new(AppState {
        db,
        tiendas: tenants::RegistroTiendas::nuevo(central, [7u8; 32]),
        estado_impresion: estado_impresion::EstadoImpresion::nuevo(),
        limitador_login: rate_limit::LimitadorIntentos::estricto(),
        limitador_verificar: rate_limit::LimitadorIntentos::laxo(),
        limitador_registro: rate_limit::LimitadorRegistro::nuevo(),
        jwt_secret: SECRETO.to_string(),
    })
}

fn claims(sub: i64, rol_id: i64) -> Claims {
    Claims {
        sub,
        username: if sub == 1 { "dueno".into() } else { "cajero".into() },
        rol_id,
        nombre_completo: "Prueba".into(),
        tienda_id: 1,
        exp: usize::MAX,
        tv: 0,
    }
}

async fn version(db: &libsql::Database, id: i64) -> i64 {
    tenants::version_token(&db.connect().unwrap(), id).await
}

async fn hash_de(db: &libsql::Database, id: i64) -> String {
    let conn = db.connect().unwrap();
    let mut f = conn.query("SELECT password_hash FROM usuarios WHERE id = ?1", libsql::params![id]).await.unwrap();
    f.next().await.unwrap().unwrap().get(0).unwrap()
}

fn tv_del_token(token: &str) -> i64 {
    decode::<Claims>(token, &DecodingKey::from_secret(SECRETO.as_bytes()), &Validation::default())
        .expect("token válido")
        .claims
        .tv
}

/// Los JWT que ya están en uso (sin "tv") se siguen aceptando como versión 0,
/// la misma que tienen todos los usuarios después de la migración 0044.
#[tokio::test]
async fn jwt_viejo_sin_version_sigue_valiendo() {
    #[derive(serde::Serialize)]
    struct ClaimsViejos {
        sub: i64,
        username: String,
        rol_id: i64,
        nombre_completo: String,
        tienda_id: i64,
        exp: usize,
    }
    let viejo = ClaimsViejos {
        sub: 1,
        username: "dueno".into(),
        rol_id: 1,
        nombre_completo: "Dueño".into(),
        tienda_id: 1,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    let token = encode(&Header::default(), &viejo, &EncodingKey::from_secret(SECRETO.as_bytes())).unwrap();
    assert_eq!(tv_del_token(&token), 0);

    let db = base("jwt_viejo").await;
    assert_eq!(version(&db, 1).await, 0);
    let state = estado().await;
    let (activo, _, tv) = state.tiendas.estado_usuario(1, 1, &db).await.unwrap().unwrap();
    assert!(activo);
    assert_eq!(tv, 0, "recién migrado, coincide con el JWT viejo: nadie pierde la sesión");
}

/// Un negocio al que todavía no se le aplicó la 0044 sigue funcionando.
#[tokio::test]
async fn negocio_sin_migracion_0044() {
    let ruta = std::env::temp_dir().join(format!("contrasenas_sin0044_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.unwrap();
    let conn = db.connect().unwrap();
    conn.execute_batch(
        "CREATE TABLE usuarios (id INTEGER PRIMARY KEY, activo INTEGER DEFAULT 1, rol_id INTEGER);
         INSERT INTO usuarios (id, activo, rol_id) VALUES (1, 1, 2);",
    )
    .await
    .unwrap();
    assert_eq!(tenants::version_token(&conn, 1).await, 0);
    let state = estado().await;
    assert_eq!(state.tiendas.estado_usuario(9, 1, &db).await.unwrap(), Some((true, 2, 0)));
}

#[tokio::test]
async fn cambiar_mi_clave() {
    let db = base("mi_clave").await;
    let state = estado().await;
    let t = Arc::new(TenantDb(db.clone()));
    let pedir = |actual: &str, nueva: &str| {
        Json(serde_json::from_value::<cuenta::CambiarMiClave>(serde_json::json!({"actual": actual, "nueva": nueva})).unwrap())
    };

    // Clave actual equivocada: no cambia nada.
    let e = cuenta::cambiar_mi_clave(State(state.clone()), Extension(t.clone()), Extension(claims(2, 2)), pedir("otra", "nueva-clave-9"))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert_eq!(version(&db, 2).await, 0);

    // Muy corta, o igual a la actual.
    for nueva in ["corta", CLAVE_CAJERO] {
        let e = cuenta::cambiar_mi_clave(State(state.clone()), Extension(t.clone()), Extension(claims(2, 2)), pedir(CLAVE_CAJERO, nueva))
            .await
            .err()
            .unwrap();
        assert_eq!(e.0, StatusCode::BAD_REQUEST, "{}", nueva);
    }
    assert_eq!(version(&db, 2).await, 0);

    // Bien: clave nueva guardada, versión 1 y un JWT nuevo con esa versión.
    let Json(r) = cuenta::cambiar_mi_clave(State(state.clone()), Extension(t.clone()), Extension(claims(2, 2)), pedir(CLAVE_CAJERO, "nueva-clave-9"))
        .await
        .expect("cambio de clave");
    assert!(r.ok);
    assert_eq!(version(&db, 2).await, 1);
    assert_eq!(tv_del_token(&r.token), 1);
    let hash = hash_de(&db, 2).await;
    assert!(bcrypt::verify("nueva-clave-9", &hash).unwrap());
    assert!(!bcrypt::verify(CLAVE_CAJERO, &hash).unwrap());

    // La sesión vieja (tv 0) ya no coincide con la base: el middleware la corta.
    let (_, _, tv) = state.tiendas.estado_usuario(1, 2, &db).await.unwrap().unwrap();
    assert_ne!(tv, claims(2, 2).tv);
}

#[tokio::test]
async fn cerrar_otras_sesiones() {
    let db = base("cerrar").await;
    let state = estado().await;
    let t = Arc::new(TenantDb(db.clone()));
    for esperada in 1..=2 {
        let Json(r) = cuenta::cerrar_otras_sesiones(State(state.clone()), Extension(t.clone()), Extension(claims(2, 2)))
            .await
            .expect("cerrar sesiones");
        assert_eq!(tv_del_token(&r.token), esperada);
        assert_eq!(version(&db, 2).await, esperada);
    }
    // La clave no cambió.
    assert!(bcrypt::verify(CLAVE_CAJERO, &hash_de(&db, 2).await).unwrap());
}

#[tokio::test]
async fn admin_restablece_la_clave_de_un_trabajador() {
    let db = base("restablecer").await;
    let state = estado().await;
    let t = Arc::new(TenantDb(db.clone()));
    let nueva = |clave: &str| Json(serde_json::from_value::<cuenta::RestablecerClave>(serde_json::json!({"nueva": clave})).unwrap());

    // El cajero no puede restablecer claves.
    let e = cuenta::restablecer_clave_usuario(State(state.clone()), Extension(t.clone()), Extension(claims(2, 2)), Path(1), nueva("se-adueña-1"))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    assert!(bcrypt::verify(CLAVE_ADMIN, &hash_de(&db, 1).await).unwrap());

    // El admin no se restablece a sí mismo por aquí (eso pide la actual).
    let e = cuenta::restablecer_clave_usuario(State(state.clone()), Extension(t.clone()), Extension(claims(1, 1)), Path(1), nueva("sin-pedir-la-actual"))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);

    // Muy corta.
    let e = cuenta::restablecer_clave_usuario(State(state.clone()), Extension(t.clone()), Extension(claims(1, 1)), Path(2), nueva("1234"))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);

    // Usuario que no existe.
    let e = cuenta::restablecer_clave_usuario(State(state.clone()), Extension(t.clone()), Extension(claims(1, 1)), Path(99), nueva("clave-valida-1"))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::NOT_FOUND);

    // Bien.
    let _ = cuenta::restablecer_clave_usuario(State(state.clone()), Extension(t.clone()), Extension(claims(1, 1)), Path(2), nueva("clave-nueva-del-cajero"))
        .await
        .expect("restablecer");
    assert!(bcrypt::verify("clave-nueva-del-cajero", &hash_de(&db, 2).await).unwrap());
    assert_eq!(version(&db, 2).await, 1);
    assert_eq!(version(&db, 1).await, 0, "el admin no pierde su sesión");
}

/// Lo mismo que usa el panel para el dueño que olvidó su clave.
#[tokio::test]
async fn fijar_clave_del_panel() {
    let db = base("panel").await;
    let conn = db.connect().unwrap();
    assert!(cuenta::fijar_clave(&conn, 1, "corta").await.is_err());
    assert_eq!(cuenta::fijar_clave(&conn, 1, "clave-que-puso-monspeet").await.unwrap(), 1);
    assert!(bcrypt::verify("clave-que-puso-monspeet", &hash_de(&db, 1).await).unwrap());
}

/// Al crear usuarios, la clave también debe tener el largo mínimo (antes
/// aceptaba hasta una de un carácter).
#[tokio::test]
async fn crear_usuario_con_clave_corta() {
    let db = base("crear").await;
    let state = estado().await;
    let t = Arc::new(TenantDb(db.clone()));
    let datos = NuevoUsuario {
        username: "mesero1".into(),
        password: "123".into(),
        nombre_completo: "Mesero".into(),
        rol_id: 2,
    };
    let e = configuracion::crear_usuario(State(state), Extension(t), Extension(claims(1, 1)), Json(datos))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    assert!(e.1.contains("8"), "{}", e.1);
}
