//! Módulo "Equivalentes" (EQUIVALENTES) contra una base local (schema.sql +
//! todas las migraciones) y los handlers de verdad: el dueño elige los
//! equivalentes, forman un grupo (en los dos sentidos y en cadena), se
//! quitan, y un negocio sin el módulo no ve nada (403).
//!
//!   cargo test --test equivalentes

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::equivalentes::{self, Equivalente, EquivalentesDe};
use minimarket_backend::handlers::importacion;
use minimarket_backend::models::auth::Claims;
use serde_json::json;
use minimarket_backend::tenants::TenantDb;

async fn base(nombre: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("equivalentes_{}_{}.db", nombre, std::process::id()));
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
    // La migración 0058 es idempotente: aplicarla otra vez no rompe nada.
    conn.execute_batch(include_str!("../migraciones/0058_equivalentes.sql")).await.expect("0058 otra vez");
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = 'LUBRICENTRO', modulos = ?1, nombre_tienda = 'REPUESTOS PRUEBA'",
        libsql::params![modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Trapecios');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (1, 'MSR-31002L', 'Trapecio LH Masaru', 120, 5, 900),
            (2, 'ATK-31002L', 'Trapecio LH Atsuki', 110, 0, 900),
            (3, 'GEN-31002L', 'Trapecio LH genérico', 90, 3, 900),
            (4, 'MSR-31007L', 'Trapecio LH Sail Masaru', 120, 2, 900),
            (5, 'ATK-31007L', 'Trapecio LH Sail Atsuki', 100, 4, 900);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn guardar(t: &Arc<TenantDb>, id: i64, ids: &[i64]) -> Result<Vec<Equivalente>, (StatusCode, String)> {
    equivalentes::guardar(Extension(t.clone()), Path(id), Json(EquivalentesDe { ids: ids.to_vec() }))
        .await
        .map(|Json(v)| v)
}

/// Los equivalentes de `id` según la lista que devuelve el servidor.
fn de(lista: &[Equivalente], id: i64) -> BTreeSet<i64> {
    let Some(grupo) = lista.iter().find(|e| e.producto_id == id).map(|e| e.grupo) else {
        return BTreeSet::new();
    };
    lista.iter().filter(|e| e.grupo == grupo && e.producto_id != id).map(|e| e.producto_id).collect()
}

#[tokio::test]
async fn en_los_dos_sentidos_y_en_cadena() {
    let t = base("grupo", "FICHA_REPUESTO,EQUIVALENTES").await;
    // En el 1 se agrega el 2: el 2 también muestra al 1.
    let l = guardar(&t, 1, &[2]).await.unwrap();
    assert_eq!(de(&l, 1), BTreeSet::from([2]));
    assert_eq!(de(&l, 2), BTreeSet::from([1]));
    // En el 2 se agrega el 3: el 1 también lo ve (en cadena).
    let l = guardar(&t, 2, &[1, 3]).await.unwrap();
    assert_eq!(de(&l, 1), BTreeSet::from([2, 3]));
    assert_eq!(de(&l, 3), BTreeSet::from([1, 2]));
    // Otro grupo (4 y 5) se une al agregar el 4 en el 1.
    guardar(&t, 4, &[5]).await.unwrap();
    let l = guardar(&t, 1, &[2, 3, 4]).await.unwrap();
    assert_eq!(de(&l, 1), BTreeSet::from([2, 3, 4, 5]));
    assert_eq!(de(&l, 5), BTreeSet::from([1, 2, 3, 4]));
    // GET devuelve lo mismo.
    let Json(todos) = equivalentes::todos(Extension(t.clone())).await.unwrap();
    assert_eq!(de(&todos, 3), BTreeSet::from([1, 2, 4, 5]));
}

#[tokio::test]
async fn quitar_saca_del_grupo() {
    let t = base("quitar", "EQUIVALENTES").await;
    guardar(&t, 1, &[2, 3]).await.unwrap();
    // Desde el 1 se quita el 3: el 3 se queda sin equivalentes; 1 y 2 siguen.
    let l = guardar(&t, 1, &[2]).await.unwrap();
    assert_eq!(de(&l, 1), BTreeSet::from([2]));
    assert!(de(&l, 3).is_empty());
    // Se quita el último: el grupo se deshace.
    let l = guardar(&t, 2, &[]).await.unwrap();
    assert!(l.is_empty());
}

#[tokio::test]
async fn un_desactivado_no_aparece_y_no_se_agrega() {
    let t = base("desactivado", "EQUIVALENTES").await;
    guardar(&t, 1, &[2, 3]).await.unwrap();
    t.0.connect().unwrap().execute("UPDATE productos SET activo = 0 WHERE id = 3", ()).await.unwrap();
    let Json(l) = equivalentes::todos(Extension(t.clone())).await.unwrap();
    assert_eq!(de(&l, 1), BTreeSet::from([2]));
    let e = guardar(&t, 1, &[3]).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    let e = guardar(&t, 99, &[1]).await.unwrap_err();
    assert_eq!(e.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sin_el_modulo_no_se_ve_nada() {
    let t = base("sin_modulo", "FICHA_REPUESTO,TRES_PRECIOS,VENTA_TABLA").await;
    let e = equivalentes::todos(Extension(t.clone())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = guardar(&t, 1, &[2]).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
}

fn claims() -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": 1, "nombre_completo": "Admin de prueba", "tienda_id": 1, "exp": usize::MAX
    }))
    .unwrap()
}

/// Reimportar con "Actualizar" un Excel sin "Ubicación" ni "Garantía" (el de
/// repuestos ya no las trae) no borra lo que la ficha tenía.
#[tokio::test]
async fn importar_sin_ubicacion_no_la_borra() {
    let t = base("importar", "FICHA_REPUESTO,TRES_PRECIOS,EQUIVALENTES").await;
    let conn = t.0.connect().unwrap();
    conn.execute_batch(
        "INSERT INTO producto_repuesto (producto_id, marca, numero_parte, ubicacion, garantia_dias) VALUES (1, 'MASARU', '716CV01B200', 'Estante 3', 90);
         INSERT INTO producto_codigos_alternos (producto_id, codigo, codigo_clave) VALUES (1, 'W712', 'W712');",
    )
    .await
    .expect("ficha previa");
    let filas = json!({"filas": [
        {"fila": 2, "codigo": "MSR-31002L", "nombre": "Trapecio LH Masaru", "precio": 125,
         "repuesto": {"marca": "MASARU JAPAN", "numero_parte": "716CV01B200"}}
    ], "existentes": "ACTUALIZAR"});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert!(r.guardado);
    let mut f = conn
        .query("SELECT marca, ubicacion, garantia_dias FROM producto_repuesto WHERE producto_id = 1", ())
        .await
        .unwrap();
    let fila = f.next().await.unwrap().unwrap();
    // La marca cambió (venía en el archivo); ubicación y garantía se mantienen.
    assert_eq!(fila.get::<String>(0).unwrap(), "MASARU JAPAN");
    assert_eq!(fila.get::<String>(1).unwrap(), "Estante 3");
    assert_eq!(fila.get::<i64>(2).unwrap(), 90);
    drop(f);
    let mut c = conn.query("SELECT COUNT(*) FROM producto_codigos_alternos WHERE producto_id = 1", ()).await.unwrap();
    assert_eq!(c.next().await.unwrap().unwrap().get::<i64>(0).unwrap(), 1);
}
