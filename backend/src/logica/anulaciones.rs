//! Anulación de comprobantes ante SUNAT, en el formato de Greenter.
//!
//!   - Factura: comunicación de baja (RA-AAAAMMDD-n).
//!   - Boleta: resumen diario con el comprobante en estado 3 "anulado"
//!     (RC-AAAAMMDD-n).
//!
//! Las dos se envían hasta 7 días calendario después de la emisión; pasado
//! ese plazo el comprobante se corrige con una nota de crédito. SUNAT las
//! procesa después: responde un ticket y la constancia se pide con él.
//!
//! Sin red ni base de datos: eso está en handlers/anulaciones.rs.

use serde_json::{json, Value};

/// Días calendario para anular desde la emisión del comprobante.
pub const DIAS_PLAZO_ANULACION: i64 = 7;

/// "BAJA" para facturas (serie con F), "RESUMEN" para boletas.
pub fn tipo_anulacion(serie: &str) -> &'static str {
    if serie.trim().to_uppercase().starts_with('F') { "BAJA" } else { "RESUMEN" }
}

/// Prefijo del identificador y ruta de Lycet de cada tipo.
pub fn prefijo_y_ruta(tipo: &str) -> (&'static str, &'static str) {
    if tipo == "BAJA" { ("RA", "voided") } else { ("RC", "summary") }
}

/// "RA-20261009-" (el número va al final).
pub fn prefijo_identificador(tipo: &str, fecha: &str) -> String {
    format!("{}-{}-", prefijo_y_ruta(tipo).0, fecha.replace('-', ""))
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn texto(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Fecha de emisión "AAAA-MM-DD" del documento enviado a SUNAT.
pub fn fecha_documento(original: &Value) -> String {
    original["fechaEmision"].as_str().unwrap_or("").chars().take(10).collect()
}

/// El número del identificador ("RA-20261009-3" -> 3).
pub fn correlativo_de(identificador: &str) -> String {
    identificador.rsplit('-').next().unwrap_or("1").to_string()
}

/// Comunicación de baja de una factura.
/// `hoy` es "AAAA-MM-DD" y `hora` "HH:MM:SS" (Perú).
pub fn armar_baja(original: &Value, identificador: &str, hoy: &str, hora: &str, motivo: &str) -> Value {
    json!({
        "correlativo": correlativo_de(identificador),
        "fecGeneracion": format!("{}T00:00:00-05:00", fecha_documento(original)),
        "fecComunicacion": format!("{}T{}-05:00", hoy, hora),
        "company": original["company"].clone(),
        "details": [{
            "tipoDoc": texto(&original["tipoDoc"]),
            "serie": texto(&original["serie"]),
            "correlativo": texto(&original["correlativo"]),
            "desMotivoBaja": motivo.trim().chars().take(100).collect::<String>().to_uppercase(),
        }],
    })
}

/// Resumen diario que anula una boleta (estado 3).
pub fn armar_resumen_anulacion(original: &Value, identificador: &str, hoy: &str, hora: &str) -> Value {
    let cliente_tipo = texto(&original["client"]["tipoDoc"]);
    let cliente_nro = texto(&original["client"]["numDoc"]);
    // Boleta sin cliente identificado: "0" (sin documento) con el número "-".
    let (cliente_tipo, cliente_nro) = if cliente_tipo.is_empty() || cliente_tipo == "-" {
        ("0".to_string(), "-".to_string())
    } else {
        (cliente_tipo, cliente_nro)
    };
    let porcentaje = original["details"]
        .as_array()
        .and_then(|d| d.iter().find(|l| l["tipAfeIgv"].as_str().unwrap_or("").starts_with('1')))
        .map(|l| num(&l["porcentajeIgv"]))
        .unwrap_or(18.0);
    json!({
        "correlativo": correlativo_de(identificador),
        "fecGeneracion": format!("{}T00:00:00-05:00", fecha_documento(original)),
        "fecResumen": format!("{}T{}-05:00", hoy, hora),
        "moneda": "PEN",
        "company": original["company"].clone(),
        "details": [{
            "tipoDoc": texto(&original["tipoDoc"]),
            "serieNro": format!("{}-{}", texto(&original["serie"]), texto(&original["correlativo"])),
            "clienteTipo": cliente_tipo,
            "clienteNro": cliente_nro,
            "estado": "3",
            "total": num(&original["mtoImpVenta"]),
            "mtoOperGravadas": num(&original["mtoOperGravadas"]),
            "mtoOperExoneradas": num(&original["mtoOperExoneradas"]),
            "mtoOperInafectas": num(&original["mtoOperInafectas"]),
            "mtoIGV": num(&original["mtoIGV"]),
            "porcentajeIgv": porcentaje,
        }],
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn original(tipo: &str, serie: &str, tipo_cliente: &str, cliente: &str) -> Value {
        json!({
            "tipoDoc": tipo, "serie": serie, "correlativo": "15",
            "fechaEmision": "2026-10-07T10:00:00-05:00",
            "company": { "ruc": "20161515648" },
            "client": { "tipoDoc": tipo_cliente, "numDoc": cliente, "rznSocial": "X" },
            "mtoOperGravadas": 130.0, "mtoOperExoneradas": 0.0, "mtoOperInafectas": 0.0, "mtoIGV": 23.4, "mtoImpVenta": 153.4,
            "details": [{ "tipAfeIgv": "10", "porcentajeIgv": 18.0 }],
        })
    }

    #[test]
    fn tipos() {
        assert_eq!(tipo_anulacion("FM01"), "BAJA");
        assert_eq!(tipo_anulacion("BM01"), "RESUMEN");
        assert_eq!(prefijo_identificador("BAJA", "2026-10-09"), "RA-20261009-");
        assert_eq!(prefijo_identificador("RESUMEN", "2026-10-09"), "RC-20261009-");
        assert_eq!(correlativo_de("RA-20261009-12"), "12");
    }

    #[test]
    fn baja_de_factura() {
        let b = armar_baja(&original("01", "FM01", "6", "20000000001"), "RA-20261009-2", "2026-10-09", "11:00:00", "error en el ruc");
        assert_eq!(b["correlativo"], "2");
        assert_eq!(b["fecGeneracion"], "2026-10-07T00:00:00-05:00");
        assert_eq!(b["fecComunicacion"], "2026-10-09T11:00:00-05:00");
        assert_eq!(b["details"][0]["serie"], "FM01");
        assert_eq!(b["details"][0]["correlativo"], "15");
        assert_eq!(b["details"][0]["desMotivoBaja"], "ERROR EN EL RUC");
    }

    #[test]
    fn resumen_de_boleta() {
        let r = armar_resumen_anulacion(&original("03", "BM01", "1", "12345678"), "RC-20261009-1", "2026-10-09", "11:00:00");
        assert_eq!(r["fecGeneracion"], "2026-10-07T00:00:00-05:00");
        assert_eq!(r["fecResumen"], "2026-10-09T11:00:00-05:00");
        let d = &r["details"][0];
        assert_eq!(d["serieNro"], "BM01-15");
        assert_eq!(d["estado"], "3");
        assert_eq!(d["clienteTipo"], "1");
        assert_eq!(d["total"], 153.4);
        assert_eq!(d["porcentajeIgv"], 18.0);
        let sin = armar_resumen_anulacion(&original("03", "BM01", "-", "-"), "RC-20261009-1", "2026-10-09", "11:00:00");
        assert_eq!(sin["details"][0]["clienteTipo"], "0");
    }
}
