//! Rubro Librería y módulo "Ficha del libro" (FICHA_LIBRO) contra una base
//! local (schema.sql + todas las migraciones) y los handlers de verdad:
//! guardar y quitar la ficha, importarla desde Excel, el reporte de ventas
//! por editorial, y que un negocio sin el módulo siga exactamente igual.
//!
//!   cargo test --test libros

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use minimarket_backend::handlers::{importacion, libros, reportes, rubros, ventas};
use minimarket_backend::models::auth::Claims;
use minimarket_backend::models::venta::NuevaVenta;
use minimarket_backend::tenants::TenantDb;
use serde_json::json;

async fn base(nombre: &str, rubro: &str, modulos: &str) -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("libros_{}_{}.db", nombre, std::process::id()));
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
    // La migración 0057 es idempotente: aplicarla otra vez no rompe nada.
    conn.execute_batch(include_str!("../migraciones/0057_ficha_libro.sql")).await.expect("0057 otra vez");
    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, nombre_tienda = 'LIBRERIA PRUEBA'",
        libsql::params![rubro, modulos],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Admin de prueba', 1);
         INSERT INTO categorias (id, nombre) VALUES (900, 'Novela'), (901, 'Útiles');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id) VALUES
            (3001, '9788420471839', 'La ciudad y los perros', 59, 10, 900),
            (3002, '9788498383621', 'Harry Potter y la piedra filosofal', 69, 10, 900),
            (3003, 'CUA-A4-100', 'Cuaderno A4 100 hojas', 6.5, 50, 901);",
    )
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

fn claims() -> Claims {
    serde_json::from_value(json!({
        "sub": 1, "username": "prueba", "rol_id": 1, "nombre_completo": "Admin de prueba", "tienda_id": 1, "exp": usize::MAX
    }))
    .unwrap()
}

async fn abrir_caja(t: &Arc<TenantDb>) {
    let _ = minimarket_backend::handlers::cajas::abrir_caja(
        Extension(t.clone()),
        Extension(claims()),
        Json(serde_json::from_value(json!({"usuario_id": 1, "numero_caja": null, "monto_inicial": 100.0, "observaciones": null})).unwrap()),
    )
    .await
    .expect("caja abierta");
}

async fn vender(t: &Arc<TenantDb>, id: i64, precio: f64, cantidad: f64) -> i64 {
    let total = precio * cantidad;
    let venta: NuevaVenta = serde_json::from_value(json!({
        "productos": [{"id": id, "nombre": format!("Producto {}", id), "precio": precio, "cantidad": cantidad, "descuentoMonto": 0}],
        "total": total, "metodo_pago": "EFECTIVO", "monto_recibido": total, "cambio": 0.0, "usuario_id": 1, "cliente_id": null
    }))
    .expect("venta");
    ventas::procesar_venta(Extension(t.clone()), Extension(claims()), Json(venta)).await.map(|Json(r)| r.venta_id).expect("venta")
}

async fn real(t: &Arc<TenantDb>, sql: &str) -> f64 {
    let conn = t.0.connect().unwrap();
    let mut filas = conn.query(sql, ()).await.unwrap_or_else(|e| panic!("{}: {}", sql, e));
    match filas.next().await.unwrap().unwrap_or_else(|| panic!("sin filas: {}", sql)).get_value(0).unwrap() {
        libsql::Value::Real(n) => n,
        libsql::Value::Integer(n) => n as f64,
        otro => panic!("{}: no es número: {:?}", sql, otro),
    }
}

fn ficha(autor: &str, editorial: &str, serie: Option<&str>, anio: Option<i64>) -> Json<libros::FichaLibro> {
    Json(libros::FichaLibro {
        producto_id: None,
        autor: Some(autor.into()),
        editorial: Some(editorial.into()),
        serie: serie.map(Into::into),
        anio,
        edicion: None,
    })
}

fn modulos_libreria() -> String {
    rubros::modulos_del_rubro("LIBRERIA").join(",")
}

#[test]
fn el_rubro_libreria_existe() {
    assert!(rubros::rubro_valido("LIBRERIA"));
    let modulos = rubros::modulos_del_rubro("LIBRERIA");
    for m in ["FICHA_LIBRO", "SERVICIOS", "COTIZACIONES", "PRESENTACIONES", "PRECIO_MAYOR", "CREDITO"] {
        assert!(modulos.iter().any(|x| x == m), "falta {}", m);
    }
    let unidades = rubros::unidades_del_rubro("LIBRERIA").unwrap();
    for u in ["UNIDAD", "DOCENA", "CIENTO", "PAQUETE"] {
        assert!(unidades.split(',').any(|x| x == u), "falta la unidad {}", u);
    }
}

#[tokio::test]
async fn guardar_leer_y_quitar_la_ficha() {
    let t = base("ficha", "LIBRERIA", &modulos_libreria()).await;
    let Json(g) = libros::guardar(Extension(t.clone()), Path(3001), ficha("Mario  Vargas Llosa ", "Alfaguara", None, Some(1963)))
        .await
        .unwrap();
    assert_eq!(g.autor.as_deref(), Some("Mario Vargas Llosa"));
    let _ = libros::guardar(Extension(t.clone()), Path(3002), ficha("J. K. Rowling", "Salamandra", Some("Harry Potter"), Some(1999)))
        .await
        .unwrap();
    let Json(todas) = libros::todas(Extension(t.clone())).await.unwrap();
    assert_eq!(todas.len(), 2);
    assert_eq!(todas[1].serie.as_deref(), Some("Harry Potter"));

    // Reemplazar y quitar.
    let _ = libros::guardar(Extension(t.clone()), Path(3001), ficha("Mario Vargas Llosa", "Seix Barral", None, None)).await.unwrap();
    let Json(todas) = libros::todas(Extension(t.clone())).await.unwrap();
    assert_eq!(todas[0].editorial.as_deref(), Some("Seix Barral"));
    assert_eq!(todas[0].anio, None);
    let _ = libros::quitar(Extension(t.clone()), Path(3001)).await.unwrap();
    assert_eq!(real(&t, "SELECT COUNT(*) FROM producto_libro").await, 1.0);

    // Mal escrito o de un producto que no existe: no se guarda.
    let e = libros::guardar(Extension(t.clone()), Path(3001), ficha("X", "Y", None, Some(1200))).await.unwrap_err();
    assert_eq!(e.0, StatusCode::BAD_REQUEST);
    let e = libros::guardar(Extension(t.clone()), Path(9999), ficha("X", "Y", None, None)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::NOT_FOUND);
    // La ficha no cambia el producto.
    assert_eq!(real(&t, "SELECT CAST(precio AS REAL) FROM productos WHERE id = 3002").await, 69.0);
}

#[tokio::test]
async fn importar_con_autor_y_editorial() {
    let t = base("importar", "LIBRERIA", &modulos_libreria()).await;
    let filas = json!({"filas": [
        {"fila": 2, "codigo": "9788401352836", "nombre": "Cien años de soledad", "precio": 65, "stock": 3,
         "libro": {"autor": "Gabriel García Márquez", "editorial": "Sudamericana", "anio": "1967.0"}},
        {"fila": 3, "codigo": "9788498383621", "nombre": "Harry Potter y la piedra filosofal", "precio": 72,
         "libro": {"autor": "J. K. Rowling", "editorial": "Salamandra", "serie": "Harry Potter"}},
        {"fila": 4, "codigo": "MAL-1", "nombre": "Año mal escrito", "precio": 10, "libro": {"anio": "hace mucho"}},
        {"fila": 5, "codigo": "LAP-1", "nombre": "Lapicero azul", "precio": 1.5, "libro": {}}
    ], "existentes": "ACTUALIZAR"});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert!(r.guardado);
    assert_eq!((r.creados, r.actualizados, r.errores), (2, 1, 1));
    let nuevo = real(&t, "SELECT id FROM productos WHERE codigo = '9788401352836'").await as i64;
    assert_eq!(real(&t, &format!("SELECT anio FROM producto_libro WHERE producto_id = {}", nuevo)).await, 1967.0);
    // El que ya existía: se actualiza su ficha (y su precio, como siempre con "Actualizar").
    let Json(todas) = libros::todas(Extension(t.clone())).await.unwrap();
    let hp = todas.iter().find(|f| f.producto_id == Some(3002)).expect("ficha del existente");
    assert_eq!(hp.serie.as_deref(), Some("Harry Potter"));
    assert_eq!(real(&t, "SELECT CAST(precio AS REAL) FROM productos WHERE id = 3002").await, 72.0);
    // El lapicero (sin datos del libro) entra como producto común; el mal escrito no entra.
    assert_eq!(real(&t, "SELECT COUNT(*) FROM producto_libro").await, 2.0);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos WHERE codigo = 'LAP-1'").await, 1.0);
    assert_eq!(real(&t, "SELECT COUNT(*) FROM productos WHERE codigo = 'MAL-1'").await, 0.0);
}

#[tokio::test]
async fn ventas_por_editorial() {
    let t = base("editorial", "LIBRERIA", &modulos_libreria()).await;
    let _ = libros::guardar(Extension(t.clone()), Path(3001), ficha("Mario Vargas Llosa", "Alfaguara", None, None)).await.unwrap();
    let _ = libros::guardar(Extension(t.clone()), Path(3002), ficha("J. K. Rowling", "alfaguara ", None, None)).await.unwrap();
    abrir_caja(&t).await;
    vender(&t, 3001, 59.0, 2.0).await;
    vender(&t, 3002, 69.0, 1.0).await;
    vender(&t, 3003, 6.5, 4.0).await; // cuaderno: no es libro, no cuenta
    let hoy = minimarket_backend::logica::tiempo::hoy_lima();
    let rango: reportes::RangoFechas = serde_json::from_value(json!({"fecha_inicio": hoy, "fecha_fin": hoy})).unwrap();
    let Json(filas) = libros::ventas_por_editorial(Extension(t.clone()), Query(rango)).await.unwrap();
    // "Alfaguara" y "alfaguara " son la misma editorial.
    assert_eq!(filas.len(), 1);
    assert_eq!(filas[0].titulos, 2);
    assert_eq!(filas[0].unidades, 3.0);
    assert_eq!(filas[0].total, 187.0);
}

#[tokio::test]
async fn otro_rubro_sigue_igual() {
    // Una tienda de ropa como VERANE: sin el módulo.
    let t = base("ropa", "ROPA", "CREDITO,VARIANTES,CAMBIOS").await;
    let e = libros::todas(Extension(t.clone())).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    let e = libros::guardar(Extension(t.clone()), Path(3001), ficha("X", "Y", None, None)).await.unwrap_err();
    assert_eq!(e.0, StatusCode::FORBIDDEN);
    // Un archivo con esas columnas se importa como siempre: se ignoran.
    let filas = json!({"filas": [{"fila": 2, "codigo": "POL-1", "nombre": "Polo", "precio": 30, "libro": {"autor": "Nadie"}}]});
    let Json(r) = importacion::importar_productos(Extension(t.clone()), Extension(claims()), Json(serde_json::from_value(filas).unwrap()))
        .await
        .unwrap();
    assert_eq!((r.creados, r.errores), (1, 0));
    assert_eq!(real(&t, "SELECT COUNT(*) FROM producto_libro").await, 0.0);
    abrir_caja(&t).await;
    let venta = vender(&t, 3003, 6.5, 2.0).await;
    assert_eq!(real(&t, &format!("SELECT CAST(total AS REAL) FROM ventas WHERE id = {}", venta)).await, 13.0);
}
