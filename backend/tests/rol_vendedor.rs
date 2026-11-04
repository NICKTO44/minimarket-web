//! Rol VENDEDOR (migración 0060): lo mismo que el administrador, menos
//! Configuración y Suscripción. El administrador y los demás roles quedan
//! igual que siempre.
//!
//!   cargo test --test rol_vendedor

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{igv, mesas, unidades};
use minimarket_backend::middleware_auth::{es_admin, exigir_admin, exigir_solo_admin, recordar_rol};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::tenants::TenantDb;

/// Cada prueba usa su propio "negocio" (tienda_id) en la caché de roles.
async fn base(nombre: &str) -> (Arc<TenantDb>, i64) {
    let ruta = std::env::temp_dir().join(format!("rol_vendedor_{}_{}.db", nombre, std::process::id()));
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
    // La 0060 se puede volver a correr: no duplica el rol.
    conn.execute_batch(include_str!("../migraciones/0060_rol_vendedor.sql")).await.expect("0060 otra vez");
    conn.execute_batch("INSERT INTO categorias (id, nombre) VALUES (900, 'Filtros');").await.unwrap();
    let mut f = conn.query("SELECT id, (SELECT COUNT(*) FROM roles WHERE nombre = 'VENDEDOR') FROM roles WHERE nombre = 'VENDEDOR'", ()).await.unwrap();
    let fila = f.next().await.unwrap().expect("rol VENDEDOR");
    assert_eq!(fila.get::<i64>(1).unwrap(), 1, "un solo rol VENDEDOR");
    let id = fila.get::<i64>(0).unwrap();
    (Arc::new(TenantDb(Arc::new(db))), id)
}

fn claims(tienda_id: i64, rol_id: i64) -> Claims {
    Claims { sub: 7, username: "u".into(), rol_id, nombre_completo: "Usuario".into(), tienda_id, exp: usize::MAX, tv: 0 }
}

#[test]
fn permisos_del_vendedor() {
    // Lo que deja la caché de roles el control de cada petición.
    recordar_rol(9001, 2, "CAJERO");
    recordar_rol(9001, 4, "VENDEDOR");
    let admin = claims(9001, 1);
    let cajero = claims(9001, 2);
    let vendedor = claims(9001, 4);
    // El mismo id en otro negocio puede ser otro rol: no es vendedor.
    let otro_negocio = claims(9002, 4);

    assert!(es_admin(&admin) && exigir_admin(&admin).is_ok() && exigir_solo_admin(&admin).is_ok());
    assert!(es_admin(&vendedor) && exigir_admin(&vendedor).is_ok());
    assert_eq!(exigir_solo_admin(&vendedor).unwrap_err().0, StatusCode::FORBIDDEN);
    assert!(!es_admin(&cajero) && exigir_admin(&cajero).is_err() && exigir_solo_admin(&cajero).is_err());
    assert!(!es_admin(&otro_negocio) && exigir_admin(&otro_negocio).is_err());
}

#[tokio::test]
async fn el_vendedor_hace_lo_del_admin_menos_configuracion() {
    let (t, vendedor_id) = base("handlers").await;
    recordar_rol(9101, vendedor_id, "VENDEDOR");
    recordar_rol(9101, 2, "CAJERO");
    let admin = claims(9101, 1);
    let vendedor = claims(9101, vendedor_id);
    let cajero = claims(9101, 2);

    // Lo del administrador (cambiar el IGV de una categoría): sí.
    let igv_de = |c: Claims| {
        let t = t.clone();
        async move {
            igv::cambiar_igv_categoria(
                Extension(t),
                Extension(c),
                Path(900),
                Json(serde_json::from_value(serde_json::json!({"afectacion_igv": "EXONERADO"})).unwrap()),
            )
            .await
            .map(|_| ())
        }
    };
    assert!(igv_de(vendedor.clone()).await.is_ok());
    assert!(igv_de(admin.clone()).await.is_ok());
    assert_eq!(igv_de(cajero.clone()).await.unwrap_err().0, StatusCode::FORBIDDEN);

    // Configuración (unidades del negocio): no.
    let unidades_de = |c: Claims| {
        let t = t.clone();
        async move {
            unidades::guardar_unidades(
                Extension(t),
                Extension(c),
                Json(serde_json::from_value(serde_json::json!({"activas": ["UNIDAD", "KG"]})).unwrap()),
            )
            .await
            .map(|_| ())
        }
    };
    assert_eq!(unidades_de(vendedor.clone()).await.unwrap_err().0, StatusCode::FORBIDDEN);
    assert!(unidades_de(admin.clone()).await.is_ok());

    // Roles para crear usuarios (Configuración): solo el administrador, y ve "VENDEDOR".
    assert_eq!(mesas::listar_roles(Extension(t.clone()), Extension(vendedor.clone())).await.unwrap_err().0, StatusCode::FORBIDDEN);
    let Json(roles) = mesas::listar_roles(Extension(t.clone()), Extension(admin.clone())).await.unwrap();
    let nombres: Vec<String> = roles.iter().map(|r| r.nombre.clone()).collect();
    assert!(nombres.contains(&"VENDEDOR".to_string()), "{:?}", nombres);
    assert!(nombres.contains(&"CAJERO".to_string()));
}
