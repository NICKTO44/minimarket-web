//! Yape y Plin por separado (migración 0019).
//!
//! El punto de venta manda "YAPE" o "PLIN". La venta se sigue guardando
//! como siempre (`metodo_pago = 'YAPE_PLIN'`, que es lo que admite la tabla
//! y lo que conocen los triggers de caja) y la billetera queda aparte, en
//! `ventas.billetera`. Lo cobrado por cada una se suma en la caja abierta
//! (`cajas.ventas_yape`, `ventas_plin`, `ventas_yape_plin`).
//!
//! Nada de aquí puede hacer fallar una venta ni una devolución: si la base
//! todavía no tiene las columnas, la venta queda como "Yape/Plin" a secas,
//! igual que antes.

/// Valor que se guarda en `ventas.metodo_pago` y `ventas.pago_otro_metodo`.
pub const YAPE_PLIN: &str = "YAPE_PLIN";

/// Método tal como llega de la pantalla -> (método que se guarda, billetera).
/// "YAPE" -> ("YAPE_PLIN", Some("YAPE")); cualquier otro queda igual.
pub fn separar(metodo: &str) -> (&str, Option<&'static str>) {
    match metodo {
        "YAPE" => (YAPE_PLIN, Some("YAPE")),
        "PLIN" => (YAPE_PLIN, Some("PLIN")),
        otro => (otro, None),
    }
}

/// Columna de `cajas` donde se suma lo cobrado por esa billetera.
fn columna(billetera: Option<&str>) -> &'static str {
    match billetera {
        Some("YAPE") => "ventas_yape",
        Some("PLIN") => "ventas_plin",
        _ => "ventas_yape_plin",
    }
}

/// La caja abierta de quien opera (primero la propia), igual que la eligen
/// los triggers de caja.
fn caja_de(usuario_id: i64) -> String {
    format!("(SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY (usuario_id = {}) DESC, id DESC LIMIT 1)", usuario_id)
}

/// Deja anotada la billetera de una venta recién registrada y suma lo
/// cobrado por ella en la caja abierta. `monto` es la parte de la venta que
/// fue por Yape/Plin (toda la venta, o la parte digital de un pago mixto).
/// Solo números y nombres fijos: se escriben directo en la sentencia.
pub async fn anotar_venta(conn: &libsql::Connection, venta_id: i64, usuario_id: i64, billetera: Option<&str>, monto: f64) {
    if !(monto > 0.0) || !monto.is_finite() {
        return;
    }
    let marcar = match billetera {
        Some(b @ ("YAPE" | "PLIN")) => format!("UPDATE ventas SET billetera = '{}' WHERE id = {};", b, venta_id),
        _ => String::new(),
    };
    let _ = conn
        .execute_batch(&format!(
            "{marcar}
             UPDATE cajas SET {col} = {col} + {monto} WHERE id = {caja};",
            marcar = marcar,
            col = columna(billetera),
            monto = monto,
            caja = caja_de(usuario_id)
        ))
        .await;
}

/// Una devolución por Yape/Plin: sale de la misma línea de la caja abierta
/// (el trigger ya la restó de `ventas_transferencia`).
pub async fn descontar(conn: &libsql::Connection, usuario_id: i64, billetera: Option<&str>, monto: f64) {
    if !(monto > 0.0) || !monto.is_finite() {
        return;
    }
    let _ = conn
        .execute(
            &format!(
                "UPDATE cajas SET {col} = {col} - {monto} WHERE id = {caja}",
                col = columna(billetera),
                monto = monto,
                caja = caja_de(usuario_id)
            ),
            (),
        )
        .await;
}

/// Billetera con la que se cobró una venta. None si no tiene (venta
/// anterior al cambio) o si la base aún no tiene la columna.
pub async fn de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<String> {
    let mut filas = conn.query("SELECT billetera FROM ventas WHERE id = ?1", libsql::params![venta_id]).await.ok()?;
    let fila = filas.next().await.ok()??;
    fila.get::<String>(0).ok().filter(|b| b == "YAPE" || b == "PLIN")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn separa_yape_y_plin() {
        assert_eq!(separar("YAPE"), ("YAPE_PLIN", Some("YAPE")));
        assert_eq!(separar("PLIN"), ("YAPE_PLIN", Some("PLIN")));
        assert_eq!(separar("YAPE_PLIN"), ("YAPE_PLIN", None));
        assert_eq!(separar("EFECTIVO"), ("EFECTIVO", None));
        assert_eq!((columna(Some("YAPE")), columna(Some("PLIN")), columna(None), columna(Some("x"))), ("ventas_yape", "ventas_plin", "ventas_yape_plin", "ventas_yape_plin"));
    }
}
