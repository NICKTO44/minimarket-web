//! Nota de crédito electrónica (tipo 07) en el formato de Greenter.
//!
//! La nota se arma a partir del documento que se envió a SUNAT con el
//! comprobante original (guardado en comprobante_archivos): mismas líneas,
//! mismos valores unitarios, mismo cliente. Así cuadra siempre con lo que
//! SUNAT ya tiene, sin volver a calcular nada desde la venta.
//!
//! Sin red ni base de datos: eso está en handlers/notas_credito.rs.

use serde::Serialize;
use serde_json::{json, Value};

use super::sunat_directo::monto_en_letras;

/// Motivos de nota de crédito que ofrece el sistema (catálogo 09 de SUNAT).
pub const MOTIVOS: [(&str, &str); 3] = [
    ("01", "Anulación de la operación"),
    ("06", "Devolución total"),
    ("07", "Devolución por ítem"),
];

/// Descripción del motivo, o None si el código no es uno de los que se usan.
pub fn nombre_motivo(codigo: &str) -> Option<&'static str> {
    MOTIVOS.iter().find(|(c, _)| *c == codigo).map(|(_, n)| *n)
}

/// true si el motivo acredita el comprobante completo.
pub fn es_total(codigo: &str) -> bool {
    codigo == "01" || codigo == "06"
}

/// Una línea del comprobante original (por su posición) y cuánto de ella
/// se acredita.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct LineaNota {
    pub indice: usize,
    pub cantidad: f64,
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
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

/// "BM01-15" del comprobante original.
pub fn numero_documento(original: &Value) -> String {
    format!("{}-{}", texto(&original["serie"]), texto(&original["correlativo"]))
}

/// Total de un comprobante o nota (mtoImpVenta).
pub fn total_de(documento: &Value) -> f64 {
    round2(num(&documento["mtoImpVenta"]))
}

/// Cantidad acreditada de cada línea del original en una nota ya armada
/// (la nota conserva el código de producto de cada línea: P001, P002...).
pub fn cantidades_por_linea(nota: &Value) -> Vec<(usize, f64)> {
    nota["details"]
        .as_array()
        .map(|lineas| {
            lineas
                .iter()
                .filter_map(|l| {
                    let codigo = l["codProducto"].as_str()?;
                    let n: usize = codigo.trim_start_matches('P').parse().ok()?;
                    Some((n.checked_sub(1)?, num(&l["cantidad"])))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Línea del original con una cantidad menor: mismos valores unitarios,
/// importes recalculados como en el comprobante (valor = cantidad × valor
/// unitario; IGV = cantidad × precio − valor).
fn linea_parcial(original: &Value, cantidad: f64) -> Value {
    let mut l = original.clone();
    let valor_unitario = num(&original["mtoValorUnitario"]);
    let precio = num(&original["mtoPrecioUnitario"]);
    let gravado = original["tipAfeIgv"].as_str().unwrap_or("10").starts_with('1');
    let valor = round2(cantidad * valor_unitario);
    let igv = if gravado { round2(cantidad * precio - valor) } else { 0.0 };
    l["cantidad"] = json!(cantidad);
    l["mtoValorVenta"] = json!(valor);
    l["mtoBaseIgv"] = json!(valor);
    l["igv"] = json!(igv);
    l["totalImpuestos"] = json!(igv);
    l
}

/// Arma la nota de crédito. `fecha` es "AAAA-MM-DD" y `hora` "HH:MM:SS".
///
/// Con un motivo total (01, 06) se acreditan todas las líneas tal como
/// están y `lineas` no se usa. Con 07 se acredita solo lo indicado.
#[allow(clippy::too_many_arguments)]
pub fn armar_nota(
    original: &Value,
    serie: &str,
    numero: i64,
    fecha: &str,
    hora: &str,
    codigo_motivo: &str,
    descripcion_motivo: &str,
    lineas: &[LineaNota],
) -> Result<Value, String> {
    let detalles_originales = original["details"].as_array().ok_or("El comprobante original no tiene líneas.")?;
    let nombre = nombre_motivo(codigo_motivo).ok_or_else(|| format!("Motivo de nota de crédito no válido: {}", codigo_motivo))?;
    let descripcion = {
        let d = descripcion_motivo.trim();
        if d.is_empty() { nombre.to_string() } else { d.chars().take(250).collect() }
    };

    let (detalles, gravadas, exoneradas, inafectas, igv) = if es_total(codigo_motivo) {
        (
            detalles_originales.clone(),
            round2(num(&original["mtoOperGravadas"])),
            round2(num(&original["mtoOperExoneradas"])),
            round2(num(&original["mtoOperInafectas"])),
            round2(num(&original["mtoIGV"])),
        )
    } else {
        if lineas.is_empty() {
            return Err("Indica qué se devuelve.".to_string());
        }
        let mut detalles = Vec::new();
        let (mut gravadas, mut exoneradas, mut inafectas, mut igv) = (0.0, 0.0, 0.0, 0.0);
        let mut vistas = Vec::new();
        for l in lineas {
            let original_linea = detalles_originales
                .get(l.indice)
                .ok_or_else(|| format!("La línea {} no es del comprobante.", l.indice + 1))?;
            if vistas.contains(&l.indice) {
                return Err("Una línea está repetida.".to_string());
            }
            vistas.push(l.indice);
            let maximo = num(&original_linea["cantidad"]);
            if !(l.cantidad > 0.0) || !l.cantidad.is_finite() || l.cantidad > maximo + 1e-9 {
                return Err(format!(
                    "Cantidad no válida para \"{}\" (máximo {}).",
                    original_linea["descripcion"].as_str().unwrap_or("la línea"),
                    maximo
                ));
            }
            let linea = if (l.cantidad - maximo).abs() < 1e-9 { original_linea.clone() } else { linea_parcial(original_linea, l.cantidad) };
            let valor = num(&linea["mtoValorVenta"]);
            match linea["tipAfeIgv"].as_str().unwrap_or("10").chars().next() {
                Some('2') => exoneradas += valor,
                Some('3') => inafectas += valor,
                _ => gravadas += valor,
            }
            igv += num(&linea["igv"]);
            detalles.push(linea);
        }
        (detalles, round2(gravadas), round2(exoneradas), round2(inafectas), round2(igv))
    };

    let valor_venta = round2(gravadas + exoneradas + inafectas);
    let total = round2(valor_venta + igv);
    if total <= 0.0 {
        return Err("La nota de crédito no puede ser de S/ 0.".to_string());
    }

    Ok(json!({
        "ublVersion": "2.1",
        "tipoDoc": "07",
        "serie": serie,
        "correlativo": numero.to_string(),
        "fechaEmision": format!("{}T{}-05:00", fecha, hora),
        "tipDocAfectado": texto(&original["tipoDoc"]),
        "numDocfectado": numero_documento(original),
        "codMotivo": codigo_motivo,
        "desMotivo": descripcion,
        "tipoMoneda": "PEN",
        "company": original["company"].clone(),
        "client": original["client"].clone(),
        "mtoOperGravadas": gravadas,
        "mtoOperExoneradas": exoneradas,
        "mtoOperInafectas": inafectas,
        "mtoIGV": igv,
        "totalImpuestos": igv,
        "valorVenta": valor_venta,
        "subTotal": total,
        "mtoImpVenta": total,
        "details": detalles,
        "legends": [{ "code": "1000", "value": monto_en_letras(total) }],
    }))
}

/// Una línea del comprobante con lo que todavía se puede acreditar.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LineaDisponible {
    pub indice: usize,
    pub descripcion: String,
    pub cantidad: f64,
    pub disponible: f64,
    pub precio_unitario: f64,
}

/// Líneas del original menos lo ya acreditado en notas anteriores.
pub fn lineas_disponibles(original: &Value, notas_previas: &[Value]) -> Vec<LineaDisponible> {
    let mut acreditado: Vec<f64> = Vec::new();
    for nota in notas_previas {
        for (i, c) in cantidades_por_linea(nota) {
            if acreditado.len() <= i {
                acreditado.resize(i + 1, 0.0);
            }
            acreditado[i] += c;
        }
    }
    original["details"]
        .as_array()
        .map(|lineas| {
            lineas
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let cantidad = num(&l["cantidad"]);
                    let ya = acreditado.get(i).copied().unwrap_or(0.0);
                    LineaDisponible {
                        indice: i,
                        descripcion: l["descripcion"].as_str().unwrap_or("").to_string(),
                        cantidad,
                        disponible: ((cantidad - ya) * 1000.0).round().max(0.0) / 1000.0,
                        precio_unitario: num(&l["mtoPrecioUnitario"]),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::logica::facturacion::{DatosParaEmitir, ItemFactura};
    use crate::logica::igv::Afectacion;
    use crate::logica::sunat_directo::{armar_documento, Emisor};

    fn boleta() -> Value {
        let items = vec![
            ItemFactura { descripcion: "Polo algodón talla M".into(), cantidad: 2.0, precio_unitario: 59.0, unidad_medida: "UNIDAD".into(), afectacion: Afectacion::Gravado },
            ItemFactura { descripcion: "Medias pack x3".into(), cantidad: 1.0, precio_unitario: 35.40, unidad_medida: "UNIDAD".into(), afectacion: Afectacion::Gravado },
            ItemFactura { descripcion: "Libro".into(), cantidad: 1.0, precio_unitario: 25.0, unidad_medida: "UNIDAD".into(), afectacion: Afectacion::Exonerado },
        ];
        let lineas: Vec<(f64, Afectacion)> = items.iter().map(|i| (i.cantidad * i.precio_unitario, i.afectacion)).collect();
        let d = crate::logica::igv::desglosar(178.40, &lineas, 18.0);
        let datos = DatosParaEmitir {
            tipo: "BOLETA".into(),
            cliente_tipo_documento: Some("DNI".into()),
            cliente_documento: Some("12345678".into()),
            cliente_nombre: Some("CLIENTE".into()),
            cliente_direccion: None,
            subtotal: d.gravadas,
            igv: d.igv,
            total: d.total,
            tasa: d.tasa,
            exoneradas: d.exoneradas,
            inafectas: d.inafectas,
            items,
            detraccion: None,
            credito: None,
        };
        let emisor = Emisor { ruc: "20161515648".into(), razon_social: "EMPRESA".into(), direccion: "AV 1".into(), ubigeo: "080101".into(), ..Default::default() };
        armar_documento(&datos, &emisor, "BM01", 15, "2026-10-08", "10:00:00", "53100000")
    }

    #[test]
    fn nota_total_copia_el_comprobante() {
        let b = boleta();
        let n = armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "06", "", &[]).unwrap();
        assert_eq!(n["tipoDoc"], "07");
        assert_eq!(n["tipDocAfectado"], "03");
        assert_eq!(n["numDocfectado"], "BM01-15");
        assert_eq!(n["codMotivo"], "06");
        assert_eq!(n["desMotivo"], "Devolución total");
        assert_eq!(n["mtoImpVenta"], b["mtoImpVenta"]);
        assert_eq!(n["details"].as_array().unwrap().len(), 3);
        assert_eq!(n["client"], b["client"]);
    }

    #[test]
    fn nota_por_item_cuadra() {
        let b = boleta();
        let n = armar_nota(
            &b,
            "BC01",
            2,
            "2026-10-09",
            "11:00:00",
            "07",
            "Cambio de talla",
            &[LineaNota { indice: 0, cantidad: 1.0 }, LineaNota { indice: 2, cantidad: 1.0 }],
        )
        .unwrap();
        // Un polo (59) y el libro exonerado (25).
        assert_eq!(n["mtoImpVenta"], 84.0);
        assert_eq!(n["mtoOperExoneradas"], 25.0);
        assert_eq!(n["mtoOperGravadas"], 50.0);
        assert_eq!(n["mtoIGV"], 9.0);
        assert_eq!(n["details"][0]["cantidad"], 1.0);
        assert_eq!(n["details"][0]["codProducto"], "P001");
        assert_eq!(n["desMotivo"], "Cambio de talla");
        assert_eq!(n["legends"][0]["value"], "OCHENTA Y CUATRO CON 00/100 SOLES");
        assert_eq!(cantidades_por_linea(&n), vec![(0, 1.0), (2, 1.0)]);
    }

    #[test]
    fn nota_rechaza_lo_que_no_cuadra() {
        let b = boleta();
        assert!(armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "07", "", &[]).is_err());
        assert!(armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "07", "", &[LineaNota { indice: 0, cantidad: 3.0 }]).is_err());
        assert!(armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "07", "", &[LineaNota { indice: 9, cantidad: 1.0 }]).is_err());
        assert!(armar_nota(
            &b,
            "BC01",
            1,
            "2026-10-09",
            "11:00:00",
            "07",
            "",
            &[LineaNota { indice: 1, cantidad: 1.0 }, LineaNota { indice: 1, cantidad: 1.0 }]
        )
        .is_err());
        assert!(armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "99", "", &[]).is_err());
    }

    #[test]
    fn disponible_descuenta_notas_previas() {
        let b = boleta();
        let previa = armar_nota(&b, "BC01", 1, "2026-10-09", "11:00:00", "07", "", &[LineaNota { indice: 0, cantidad: 1.0 }]).unwrap();
        let d = lineas_disponibles(&b, &[previa]);
        assert_eq!(d[0].disponible, 1.0);
        assert_eq!(d[1].disponible, 1.0);
        assert_eq!(d[0].precio_unitario, 59.0);
        assert_eq!(d[0].descripcion, "Polo algodón talla M");
    }
}
