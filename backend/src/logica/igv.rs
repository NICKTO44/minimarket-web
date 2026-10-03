//! Cálculo del IGV de una venta.
//!
//! Reglas (las mismas en el frontend, utils/igv.js):
//! - Los precios son SIEMPRE precio final al público (con IGV incluido).
//! - Cada línea es GRAVADA, EXONERADA o INAFECTA (catálogo 07 de SUNAT:
//!   10, 20 y 30). Lo decide el producto o, si no dice nada, su categoría.
//! - La tasa es una sola por negocio (18 % general; 10.5 % MYPE de
//!   restaurantes y hoteles inscritas en el padrón de SUNAT).
//! - El sistema no decide el impuesto: aplica lo que el negocio configuró.

use std::collections::HashMap;

/// Tasa general del IGV (16 % IGV + 2 % IPM).
pub const TASA_GENERAL: f64 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Afectacion {
    Gravado,
    Exonerado,
    Inafecto,
}

impl Afectacion {
    /// Lo guardado en la base. Vacío, NULL o desconocido = gravado.
    pub fn desde(texto: Option<&str>) -> Self {
        match texto.map(str::trim) {
            Some("EXONERADO") => Afectacion::Exonerado,
            Some("INAFECTO") => Afectacion::Inafecto,
            _ => Afectacion::Gravado,
        }
    }

    pub fn como_texto(self) -> &'static str {
        match self {
            Afectacion::Gravado => "GRAVADO",
            Afectacion::Exonerado => "EXONERADO",
            Afectacion::Inafecto => "INAFECTO",
        }
    }

    /// Código del catálogo 07 de SUNAT (operación onerosa).
    pub fn codigo_sunat(self) -> &'static str {
        match self {
            Afectacion::Gravado => "10",
            Afectacion::Exonerado => "20",
            Afectacion::Inafecto => "30",
        }
    }
}

/// true si el texto es un valor que se puede guardar.
pub fn afectacion_valida(texto: &str) -> bool {
    matches!(texto, "GRAVADO" | "EXONERADO" | "INAFECTO")
}

/// Tasa utilizable: entre 0 y 30 (sin incluir 0). Cualquier otra cosa
/// (NULL, 0, un valor absurdo) se trata como la tasa general.
pub fn tasa_valida(tasa: Option<f64>) -> f64 {
    match tasa {
        Some(t) if t > 0.0 && t <= 30.0 => t,
        _ => TASA_GENERAL,
    }
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Totales de una venta, como los pide SUNAT.
#[derive(Debug, Clone, PartialEq)]
pub struct Desglose {
    pub tasa: f64,
    /// Valor de venta de lo gravado (sin IGV).
    pub gravadas: f64,
    pub exoneradas: f64,
    pub inafectas: f64,
    pub igv: f64,
    pub total: f64,
}

/// Desglosa una venta a partir del total cobrado y de sus líneas
/// (total de la línea con IGV incluido, afectación).
///
/// Lo gravado se obtiene por diferencia (total − exonerado − inafecto) para
/// que gravadas + igv + exoneradas + inafectas sea SIEMPRE exactamente el
/// total cobrado, sin céntimos sueltos por redondeo.
pub fn desglosar(total: f64, lineas: &[(f64, Afectacion)], tasa: f64) -> Desglose {
    let mut por_tipo: HashMap<&'static str, f64> = HashMap::new();
    for (monto, afectacion) in lineas {
        *por_tipo.entry(afectacion.como_texto()).or_insert(0.0) += monto;
    }
    let total = round2(total);
    let exoneradas = round2(*por_tipo.get("EXONERADO").unwrap_or(&0.0)).min(total).max(0.0);
    let inafectas = round2(*por_tipo.get("INAFECTO").unwrap_or(&0.0)).min(round2(total - exoneradas)).max(0.0);
    let gravado_con_igv = round2(total - exoneradas - inafectas).max(0.0);
    let gravadas = round2(gravado_con_igv / (1.0 + tasa / 100.0));
    let igv = round2(gravado_con_igv - gravadas);
    Desglose { tasa, gravadas, exoneradas, inafectas, igv, total }
}

/// Valor sin IGV de un monto con IGV incluido, según su afectación.
pub fn valor_sin_igv(monto: f64, afectacion: Afectacion, tasa: f64) -> f64 {
    match afectacion {
        Afectacion::Gravado => monto / (1.0 + tasa / 100.0),
        _ => monto,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use Afectacion::*;

    #[test]
    fn todo_gravado_al_18_es_igual_que_antes() {
        // Antes: igv = total - total/1.18.
        let d = desglosar(118.0, &[(118.0, Gravado)], 18.0);
        assert_eq!((d.gravadas, d.igv, d.exoneradas, d.inafectas, d.total), (100.0, 18.0, 0.0, 0.0, 118.0));
        let d = desglosar(25.80, &[(25.80, Gravado)], 18.0);
        assert_eq!(d.igv, round2(25.80 - 25.80 / 1.18));
    }

    #[test]
    fn bodega_mezcla_gravado_y_exonerado() {
        // Gaseosa 11.80 gravada + papa 5.00 + arroz 9.00 exonerados.
        let d = desglosar(25.80, &[(11.80, Gravado), (5.00, Exonerado), (9.00, Exonerado)], 18.0);
        assert_eq!((d.gravadas, d.exoneradas, d.igv, d.total), (10.0, 14.0, 1.8, 25.8));
    }

    #[test]
    fn restaurante_mype_10_5() {
        let d = desglosar(22.10, &[(22.10, Gravado)], 10.5);
        assert_eq!((d.gravadas, d.igv), (20.0, 2.1));
    }

    #[test]
    fn siempre_cuadra_con_el_total() {
        // Montos incómodos: la suma de las partes debe ser el total exacto.
        for (total, lineas, tasa) in [
            (33.33, vec![(10.01, Gravado), (13.32, Exonerado), (10.00, Inafecto)], 18.0),
            (0.10, vec![(0.10, Gravado)], 18.0),
            (999.99, vec![(333.33, Gravado), (333.33, Gravado), (333.33, Exonerado)], 10.5),
            (7.77, vec![(7.77, Exonerado)], 18.0),
        ] {
            let d = desglosar(total, &lineas, tasa);
            assert_eq!(round2(d.gravadas + d.igv + d.exoneradas + d.inafectas), round2(total), "{:?}", d);
            assert!(d.gravadas >= 0.0 && d.igv >= 0.0);
        }
    }

    #[test]
    fn con_descuento_el_total_manda() {
        // Líneas suman 30 pero se cobró 27 (descuento): lo exonerado se
        // respeta y lo gravado sale por diferencia.
        let d = desglosar(27.0, &[(20.0, Gravado), (10.0, Exonerado)], 18.0);
        assert_eq!((d.exoneradas, round2(d.gravadas + d.igv)), (10.0, 17.0));
    }

    #[test]
    fn textos_y_tasas() {
        assert_eq!(Afectacion::desde(None), Gravado);
        assert_eq!(Afectacion::desde(Some("")), Gravado);
        assert_eq!(Afectacion::desde(Some("EXONERADO")), Exonerado);
        assert_eq!(Afectacion::desde(Some("cualquier cosa")), Gravado);
        assert_eq!(Exonerado.codigo_sunat(), "20");
        assert_eq!(tasa_valida(None), 18.0);
        assert_eq!(tasa_valida(Some(0.0)), 18.0);
        assert_eq!(tasa_valida(Some(99.0)), 18.0);
        assert_eq!(tasa_valida(Some(10.5)), 10.5);
        assert!(afectacion_valida("INAFECTO") && !afectacion_valida("HEREDAR"));
    }
}
