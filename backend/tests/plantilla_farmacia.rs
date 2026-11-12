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
