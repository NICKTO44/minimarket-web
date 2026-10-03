//! Detracción (SPOT): el comprador deposita un porcentaje del total de la
//! factura en la cuenta del vendedor en el Banco de la Nación. Aquí solo
//! está el cálculo; los datos del negocio se leen en handlers/detraccion.rs.
//!
//! El sistema no decide el impuesto: aplica el porcentaje, el código y el
//! monto mínimo que el administrador configuró (por defecto los de la
//! madera: 4 %, código 008, operaciones mayores a S/ 700).

use crate::logica::igv::round2;

pub const PORCENTAJE_POR_DEFECTO: f64 = 4.0;
pub const CODIGO_POR_DEFECTO: &str = "008";
pub const MINIMO_POR_DEFECTO: f64 = 700.0;
/// Tope para atajar un porcentaje mal escrito (las tasas vigentes van de 1.5 % a 15 %).
pub const PORCENTAJE_MAXIMO: f64 = 30.0;

/// Datos de detracción del negocio.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigDetraccion {
    /// El módulo DETRACCION está encendido.
    pub activa: bool,
    pub porcentaje: f64,
    /// Catálogo 54 de SUNAT ("008" madera, "009" arena y piedra...).
    pub codigo: String,
    /// Se aplica cuando el total de la factura SUPERA este monto.
    pub minimo: f64,
    /// Cuenta de detracciones en el Banco de la Nación ("" = sin configurar).
    pub cuenta: String,
}

impl ConfigDetraccion {
    /// Encendida y con cuenta: ya se puede aplicar.
    pub fn lista(&self) -> bool {
        self.activa && !self.cuenta.trim().is_empty()
    }
}

/// Lo que va en la factura cuando la operación está sujeta a detracción.
#[derive(Debug, Clone, PartialEq)]
pub struct DetraccionFactura {
    pub codigo: String,
    pub porcentaje: f64,
    /// Sobre el total con IGV incluido, con dos decimales.
    pub monto: f64,
    pub cuenta: String,
}

pub fn porcentaje_valido(p: f64) -> bool {
    p > 0.0 && p <= PORCENTAJE_MAXIMO
}

/// Código del catálogo 54: exactamente tres dígitos.
pub fn codigo_valido(c: &str) -> bool {
    c.len() == 3 && c.chars().all(|x| x.is_ascii_digit())
}

/// Cuenta bancaria: dígitos, con guiones o espacios opcionales.
pub fn cuenta_valida(c: &str) -> bool {
    let digitos = c.chars().filter(|x| x.is_ascii_digit()).count();
    c.len() <= 30 && digitos >= 6 && c.chars().all(|x| x.is_ascii_digit() || x == '-' || x == ' ')
}

/// La detracción de un comprobante, o None si no le toca.
///
/// Solo en FACTURA (la boleta no sustenta costo ni crédito fiscal y queda
/// fuera), con el módulo encendido y la cuenta configurada, cuando el total
/// supera el mínimo. `pedida = Some(false)` es el cajero indicando que esta
/// venta no está sujeta (por ejemplo, una factura sin madera).
pub fn calcular(tipo: &str, total: f64, cfg: &ConfigDetraccion, pedida: Option<bool>) -> Option<DetraccionFactura> {
    if tipo != "FACTURA" || !cfg.lista() || pedida == Some(false) {
        return None;
    }
    if round2(total) <= round2(cfg.minimo) {
        return None;
    }
    Some(DetraccionFactura {
        codigo: cfg.codigo.clone(),
        porcentaje: cfg.porcentaje,
        monto: round2(total * cfg.porcentaje / 100.0),
        cuenta: cfg.cuenta.trim().to_string(),
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cfg(activa: bool, cuenta: &str) -> ConfigDetraccion {
        ConfigDetraccion { activa, porcentaje: 4.0, codigo: "008".into(), minimo: 700.0, cuenta: cuenta.into() }
    }

    #[test]
    fn madera_4_por_ciento_sobre_el_total_con_igv() {
        // Ejemplo de la documentación de FacturaLibre: 1180 → 47.20.
        let d = calcular("FACTURA", 1180.0, &cfg(true, "00-123-456789"), None).unwrap();
        assert_eq!((d.monto, d.porcentaje, d.codigo.as_str()), (47.2, 4.0, "008"));
    }

    #[test]
    fn solo_si_supera_el_minimo() {
        let c = cfg(true, "00-123-456789");
        assert!(calcular("FACTURA", 700.0, &c, None).is_none());
        assert!(calcular("FACTURA", 700.01, &c, None).is_some());
    }

    #[test]
    fn nunca_en_boleta_ni_sin_configurar() {
        assert!(calcular("BOLETA", 5000.0, &cfg(true, "00-123-456789"), None).is_none());
        assert!(calcular("FACTURA", 5000.0, &cfg(false, "00-123-456789"), None).is_none());
        assert!(calcular("FACTURA", 5000.0, &cfg(true, "  "), None).is_none());
    }

    #[test]
    fn el_cajero_puede_excluir_una_venta() {
        let c = cfg(true, "00-123-456789");
        assert!(calcular("FACTURA", 5000.0, &c, Some(false)).is_none());
        assert!(calcular("FACTURA", 5000.0, &c, Some(true)).is_some());
        // Pedirla no la fuerza por debajo del mínimo.
        assert!(calcular("FACTURA", 500.0, &c, Some(true)).is_none());
    }

    #[test]
    fn porcentaje_cambiable() {
        let mut c = cfg(true, "00-123-456789");
        c.porcentaje = 10.0;
        c.codigo = "009".into();
        let d = calcular("FACTURA", 1000.0, &c, None).unwrap();
        assert_eq!((d.monto, d.codigo.as_str()), (100.0, "009"));
    }

    #[test]
    fn validaciones() {
        assert!(porcentaje_valido(4.0) && porcentaje_valido(1.5) && !porcentaje_valido(0.0) && !porcentaje_valido(40.0));
        assert!(codigo_valido("008") && !codigo_valido("8") && !codigo_valido("0O8"));
        assert!(cuenta_valida("00-123-456789") && cuenta_valida("00123456789") && !cuenta_valida("abc") && !cuenta_valida("12"));
    }
}
