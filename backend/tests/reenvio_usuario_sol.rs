//! Botón "Reenviar" para los errores del usuario secundario SOL, sin red
//! (Lycet apagado): un comprobante en ERROR por el usuario SOL (clave mala,
//! usuario de baja o el 0111 de un usuario recién creado) vuelve a
//! PENDIENTE con su mismo número y se manda una vez; uno rechazado por su
//! contenido o con otro error no se puede reenviar.
//!
//!   cargo test --test reenvio_usuario_sol

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use minimarket_backend::handlers::envios_sunat::reenviar;
use minimarket_backend::tenants::TenantDb;

async fn base() -> Arc<TenantDb> {
    let ruta = std::env::temp_dir().join(format!("reenvio_usuario_sol_{}.db", std::process::id()));
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
    let ahora = minimarket_backend::logica::tiempo::ahora_lima();
    conn.execute_batch(&format!(
        "INSERT INTO usuarios (id, username, password_hash, nombre_completo, rol_id) VALUES (1, 'prueba', 'x', 'Cajero', 1);
         INSERT INTO ventas (id, folio, subtotal, total, metodo_pago, usuario_id, estado) VALUES
            (1, 'V-1', 1, 1, 'EFECTIVO', 1, 'COMPLETADA'), (2, 'V-2', 1, 1, 'EFECTIVO', 1, 'COMPLETADA'),
            (3, 'V-3', 1, 1, 'EFECTIVO', 1, 'COMPLETADA'), (4, 'V-4', 1, 1, 'EFECTIVO', 1, 'COMPLETADA');
         INSERT INTO comprobantes_electronicos (id, venta_id, tipo, proveedor, serie, numero, estado, mensaje_sunat, fecha_emision) VALUES
            (1, 1, 'BOLETA', 'SUNAT_DIRECTO', 'BM01', 1, 'ERROR',
             'SUNAT no aceptó el usuario secundario SOL (111: No tiene el perfil para enviar comprobantes electronicos - Detalle: Rejected by policy.). Revisa su clave y permisos y vuelve a registrar el negocio en el panel.', '{ahora}'),
            (2, 2, 'BOLETA', 'SUNAT_DIRECTO', 'BM01', 2, 'ERROR',
             'SUNAT ya tiene otro documento con esta serie y número (1033: x).', '{ahora}'),
            (3, 3, 'BOLETA', 'SUNAT_DIRECTO', 'BM01', 3, 'RECHAZADO', 'SUNAT respondió 2017: x', '{ahora}'),
            (4, 4, 'BOLETA', 'SUNAT_DIRECTO', 'BM01', 4, 'ERROR',
             'SUNAT no aceptó el usuario secundario SOL (102: Usuario o contrasena incorrectos). Revisa su clave y permisos, vuelve a registrar el negocio y usa «Reenviar».', '{ahora}');
         INSERT INTO comprobante_archivos (comprobante_id, documento) VALUES
            (1, '{{\"serie\":\"BM01\",\"correlativo\":\"1\",\"tipoDoc\":\"03\",\"company\":{{\"ruc\":\"10436589889\"}}}}'),
            (4, '{{\"serie\":\"BM01\",\"correlativo\":\"4\",\"tipoDoc\":\"03\",\"company\":{{\"ruc\":\"10436589889\"}}}}');"
    ))
    .await
    .expect("datos de prueba");
    Arc::new(TenantDb(Arc::new(db)))
}

async fn estado(t: &Arc<TenantDb>, id: i64) -> String {
    let conn = t.0.connect().unwrap();
    let mut f = conn.query("SELECT estado FROM comprobantes_electronicos WHERE id = ?1", libsql::params![id]).await.unwrap();
    f.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test]
async fn reenviar_errores_del_usuario_sol() {
    // Lycet apagado: el envío no llega y queda PENDIENTE (lo reenvía la tarea).
    std::env::set_var("LYCET_URL", "http://127.0.0.1:9");
    let t = base().await;

    // 0111 de antes y clave mala corregida: se vuelven a mandar.
    for id in [1, 4] {
        let r = reenviar(Extension(t.clone()), Path(id)).await.expect("se puede reenviar").0;
        assert_eq!(r.estado, "PENDIENTE");
        assert_eq!(estado(&t, id).await, "PENDIENTE");
    }

    // Otro error (1033) y un rechazo por contenido: no.
    for id in [2, 3] {
        let e = reenviar(Extension(t.clone()), Path(id)).await.err().expect("no se puede");
        assert_eq!(e.0, StatusCode::CONFLICT);
    }
    assert_eq!(estado(&t, 2).await, "ERROR");
    assert_eq!(estado(&t, 3).await, "RECHAZADO");
}
