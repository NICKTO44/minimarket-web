use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ConfiguracionTienda {
    pub id: i64,
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub ruc: Option<String>,
    pub moneda: String,
    pub iva_porcentaje: f64,
    pub facturalibre_token: Option<String>,
    pub facturalibre_ruta: Option<String>,
    pub codigo_producto_sunat_generico: Option<String>,
    pub serie_boleta: Option<String>,
    pub serie_factura: Option<String>,
    pub logo_path: Option<String>,
    pub color_acento: Option<String>,
    /// 'TIENDA' (sistema de siempre) o 'RESTAURANTE' (atención en mesas).
    pub modo_negocio: String,
    /// Rubro del negocio (BODEGA, RESTAURANTE, FERRETERIA, MADERERA, GENERAL).
    pub rubro: String,
    /// Módulos encendidos (MESAS, SERVICIOS...).
    pub modulos: Vec<String>,
    /// Razón social exacta (va en los comprobantes junto al nombre comercial).
    pub razon_social: Option<String>,
    /// 'SUNAT_DIRECTO' = emite directo a SUNAT; otro valor = FacturaLibre.
    pub facturacion_proveedor: Option<String>,
    /// Establecimiento anexo (migración 0062): código y dirección para
    /// imprimir "Sucursal: ...". None = emite desde el domicilio fiscal.
    pub codigo_establecimiento: Option<String>,
    pub sucursal: Option<String>,
    /// Domicilio fiscal completo para imprimir en los comprobantes:
    /// "AV. ... URB. EL TREBOL - LIMA - HUARAL - HUARAL" (la calle más el
    /// departamento, provincia y distrito de la emisión directa). None si
    /// el negocio no tiene esos datos: se imprime `direccion` como siempre.
    pub direccion_completa: Option<String>,
}

/// La dirección con "DEPARTAMENTO - PROVINCIA - DISTRITO" (como la Ficha
/// RUC). None si falta alguno de los tres o si la dirección ya los trae.
pub fn direccion_completa(direccion: &str, departamento: &str, provincia: &str, distrito: &str) -> Option<String> {
    let (direccion, dep, prov, dist) = (direccion.trim(), departamento.trim(), provincia.trim(), distrito.trim());
    if direccion.is_empty() || dep.is_empty() || prov.is_empty() || dist.is_empty() {
        return None;
    }
    let mayus = direccion.to_uppercase();
    if mayus.contains(&dep.to_uppercase()) && mayus.contains(&dist.to_uppercase()) {
        return None;
    }
    Some(format!("{} - {} - {} - {}", direccion, dep.to_uppercase(), prov.to_uppercase(), dist.to_uppercase()))
}

#[cfg(test)]
mod pruebas {
    use super::direccion_completa;

    #[test]
    fn domicilio_fiscal_completo() {
        assert_eq!(
            direccion_completa("AV. LOS ALAMOS NRO. 120 URB. EL TREBOL", "LIMA", "HUARAL", "HUARAL").as_deref(),
            Some("AV. LOS ALAMOS NRO. 120 URB. EL TREBOL - LIMA - HUARAL - HUARAL")
        );
        // Ya escrita completa, o sin los datos de la emisión: como siempre.
        assert_eq!(direccion_completa("JR. X 1 - LIMA - HUARAL - HUARAL", "LIMA", "HUARAL", "HUARAL"), None);
        assert_eq!(direccion_completa("JR. X 1", "", "", ""), None);
        assert_eq!(direccion_completa("", "LIMA", "HUARAL", "HUARAL"), None);
    }
}

#[derive(Debug, Deserialize)]
pub struct ActualizarConfiguracion {
    pub nombre_tienda: String,
    pub direccion: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub ruc: Option<String>,
    pub moneda: String,
    pub iva_porcentaje: f64,
    pub facturalibre_token: Option<String>,
    pub facturalibre_ruta: Option<String>,
    pub codigo_producto_sunat_generico: Option<String>,
    pub serie_boleta: Option<String>,
    pub serie_factura: Option<String>,
    pub color_acento: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsuarioResumen {
    pub id: i64,
    pub username: String,
    pub nombre_completo: String,
    pub rol_id: i64,
    pub rol_nombre: String,
    pub activo: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoUsuario {
    pub username: String,
    pub password: String,
    pub nombre_completo: String,
    pub rol_id: i64,
}

#[derive(Debug, Serialize)]
pub struct AccionResponse {
    pub success: bool,
    pub message: String,
}