//! La plantilla de Excel de farmacia (frontend/src/utils/plantillasProductos.js)
//! ofrece listas para elegir: el servidor debe entender cada opción tal cual.
//!
//!   cargo test --test plantilla_farmacia

use minimarket_backend::handlers::medicamentos::{condicion_desde_texto, forma_desde_texto, generico_desde_texto};

#[test]
fn cada_opcion_de_las_listas_se_reconoce() {
    // Las mismas etiquetas que FORMAS_FARMACEUTICAS (frontend/src/utils/medicamentos.js).
    for forma in ["Tableta","Tableta recubierta","Cápsula","Jarabe","Suspensión","Solución","Gotas","Inyectable","Crema","Ungüento","Gel","Óvulo","Supositorio","Inhalador","Polvo","Parche","Colirio","Spray","Otra"] {
        assert!(forma_desde_texto(forma).is_some(), "forma no reconocida: {}", forma);
    }
    assert_eq!(condicion_desde_texto("Sin receta"), Some("SIN_RECETA"));
    assert_eq!(condicion_desde_texto("Con receta"), Some("CON_RECETA"));
    assert_eq!(condicion_desde_texto("Receta retenida"), Some("RECETA_RETENIDA"));
    assert_eq!(generico_desde_texto("Sí"), Ok(Some(true)));
    assert_eq!(generico_desde_texto("No"), Ok(Some(false)));
    assert_eq!(forma_desde_texto("Tableta recubierta"), Some("TABLETA_RECUBIERTA"));
}

#[tokio::test]
async fn lotes_vigentes_para_exportar() {
    use axum::extract::Extension;
    use minimarket_backend::handlers::lotes;
    use minimarket_backend::tenants::TenantDb;
    use std::sync::Arc;

    let ruta = std::env::temp_dir().join(format!("plantilla_farmacia_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&ruta);
    let db = libsql::Builder::new_local(&ruta).build().await.unwrap();
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
    conn.execute_batch(
        "INSERT INTO categorias (id, nombre) VALUES (1, 'Antialérgicos');
         INSERT INTO productos (id, codigo, nombre, precio, stock, categoria_id, lleva_vencimiento) VALUES
            (10, 'LORA-10', 'Loratadina 10 mg', 0.4, 0, 1, 1);
         INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, numero_lote, activo) VALUES
            (10, 200, '2028-03-31', 'LR230', 1),
            (10, 100, '2027-09-30', 'LR221', 1),
            (10, 0, '2027-01-31', 'VACIO', 1),
            (10, 50, '2027-05-31', 'BAJA', 0);",
    )
    .await
    .unwrap();
    let t = Arc::new(TenantDb(Arc::new(db)));
    let lista = lotes::lotes_vigentes(Extension(t)).await.unwrap().0;
    let resumen: Vec<(String, String, f64)> =
        lista.iter().map(|l| (l.numero_lote.clone().unwrap_or_default(), l.fecha_vencimiento.clone(), l.cantidad)).collect();
    // Solo los activos con stock, del que vence primero al último.
    assert_eq!(resumen, vec![("LR221".into(), "2027-09-30".into(), 100.0), ("LR230".into(), "2028-03-31".into(), 200.0)]);
}
