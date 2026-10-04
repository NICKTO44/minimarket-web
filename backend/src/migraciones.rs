use libsql::Builder;
use std::path::Path;

use crate::tenants::TiendaConexion;

/// Aplica, en orden, todas las migraciones de `carpeta` que todavía no
/// estén registradas en la tabla `_migraciones_aplicadas` de esa tienda.
/// Devuelve los nombres de las migraciones que se aplicaron en esta
/// corrida (vacío si ya estaba al día). Si una migración falla, se
/// detiene ahí mismo para esa tienda — no sigue con las siguientes
/// migraciones de esa misma base, para no dejarla en un estado a medias
/// más allá de lo que ya causó el error.
pub async fn aplicar_migraciones_a_tienda(
    tienda: &TiendaConexion,
    carpeta: &Path,
) -> Result<Vec<String>, String> {
    let db = Builder::new_remote(tienda.db_url.clone(), tienda.db_token.clone())
        .build()
        .await
        .map_err(|e| format!("no se pudo conectar a la base: {}", e))?;
    let conn = db.connect().map_err(|e| e.to_string())?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migraciones_aplicadas (
            nombre TEXT PRIMARY KEY,
            fecha_aplicada TEXT DEFAULT (datetime('now', 'localtime'))
        );",
    )
    .await
    .map_err(|e| format!("no se pudo preparar la tabla de control: {}", e))?;

    let mut archivos: Vec<_> = std::fs::read_dir(carpeta)
        .map_err(|e| format!("no se pudo leer la carpeta de migraciones ({}): {}", carpeta.display(), e))?
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.path())
        .filter(|ruta| ruta.extension().map(|ext| ext == "sql").unwrap_or(false))
        .collect();
    // Orden alfabético == orden numérico si los archivos usan prefijo
    // de 4 dígitos (0001_, 0002_, ...).
    archivos.sort();

    let mut aplicadas = Vec::new();

    for ruta in archivos {
        let nombre = ruta
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("desconocido")
            .to_string();

        let mut filas = conn
            .query(
                "SELECT COUNT(*) FROM _migraciones_aplicadas WHERE nombre = ?1",
                libsql::params![nombre.clone()],
            )
            .await
            .map_err(|e| format!("{}: error consultando el control de migraciones: {}", nombre, e))?;

        let ya_aplicada: i64 = match filas.next().await.map_err(|e| e.to_string())? {
            Some(fila) => fila.get(0).unwrap_or(0),
            None => 0,
        };

        if ya_aplicada > 0 {
            continue;
        }

        let contenido = std::fs::read_to_string(&ruta)
            .map_err(|e| format!("{}: no se pudo leer el archivo: {}", nombre, e))?;

        conn.execute_batch(&contenido)
            .await
            .map_err(|e| format!("{}: error aplicando la migración: {}", nombre, e))?;

        conn.execute(
            "INSERT INTO _migraciones_aplicadas (nombre) VALUES (?1)",
            libsql::params![nombre.clone()],
        )
        .await
        .map_err(|e| format!("{}: se aplicó pero no se pudo registrar en el control: {}", nombre, e))?;

        aplicadas.push(nombre);
    }

    Ok(aplicadas)
}
/// Solo LEE: ventas que quedaron registradas sin ningún producto. Es la
/// huella de una venta que se cortó a mitad de camino antes de que todo se
/// guardara en una sola transacción (ver handlers/ventas.rs). Devuelve
/// (folio, fecha, total) de las últimas 20. No modifica nada.
pub async fn ventas_sin_productos(tienda: &TiendaConexion) -> Result<Vec<(String, String, f64)>, String> {
    let db = Builder::new_remote(tienda.db_url.clone(), tienda.db_token.clone())
        .build()
        .await
        .map_err(|e| format!("no se pudo conectar a la base: {}", e))?;
    let conn = db.connect().map_err(|e| e.to_string())?;
    let mut filas = conn
        .query(
            "SELECT v.folio, v.fecha_hora, CAST(v.total AS REAL) FROM ventas v
             WHERE v.estado = 'COMPLETADA'
               AND NOT EXISTS (SELECT 1 FROM detalles_venta d WHERE d.venta_id = v.id)
             ORDER BY v.id DESC LIMIT 20",
            (),
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut ventas = Vec::new();
    while let Some(f) = filas.next().await.map_err(|e| e.to_string())? {
        ventas.push((f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default(), f.get(2).unwrap_or(0.0)));
    }
    Ok(ventas)
}
