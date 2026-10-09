//! Certificado digital del negocio (el CDT gratuito de SUNAT u otro).
//!
//! SUNAT lo entrega como archivo .p12 / .pfx protegido con una clave. Lycet
//! lo necesita en PEM: el certificado y la clave privada, sin cifrar. Aquí se
//! convierte solo (sin openssl), y de paso se lee a nombre de quién está,
//! hasta cuándo vale y su número de serie, para mostrarlo en el panel y para
//! comprobar después que Lycet firma con este certificado y no con otro.
//!
//! También acepta un .pem ya convertido (certificado + clave privada).

use base64::Engine;
use p12_keystore::{KeyStore, Pkcs12ImportPolicy};

/// Lo que se sabe de un certificado.
#[derive(Debug, Clone, PartialEq)]
pub struct Descripcion {
    /// Nombre del titular (CN), o el sujeto completo si no tiene CN.
    pub titular: String,
    /// Sujeto completo ("CN=..., serialNumber=..."): ahí suele ir el RUC.
    pub sujeto: String,
    /// Último día de validez, "AAAA-MM-DD".
    pub vence: String,
    /// Número de serie del certificado (hexadecimal, "0a:01:9b...").
    pub serie: String,
}

impl Descripcion {
    /// ¿El certificado menciona este RUC? Los certificados de SUNAT lo
    /// llevan en el sujeto; si no aparece, probablemente es de otro negocio.
    pub fn menciona_ruc(&self, ruc: &str) -> bool {
        let ruc = ruc.trim();
        !ruc.is_empty() && self.sujeto.contains(ruc)
    }
}

/// Certificado listo para registrar en Lycet.
#[derive(Debug, Clone)]
pub struct CertificadoListo {
    /// Certificado(s) + clave privada en PEM, sin cifrar.
    pub pem: String,
    pub descripcion: Descripcion,
}

fn a_pem(etiqueta: &str, der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut texto = format!("-----BEGIN {}-----\n", etiqueta);
    for linea in b64.as_bytes().chunks(64) {
        texto.push_str(std::str::from_utf8(linea).unwrap_or_default());
        texto.push('\n');
    }
    texto.push_str(&format!("-----END {}-----\n", etiqueta));
    texto
}

/// Primer bloque "-----BEGIN <etiqueta>-----" de un PEM, ya decodificado.
fn primer_bloque_pem(texto: &str, etiqueta: &str) -> Option<Vec<u8>> {
    let inicio = format!("-----BEGIN {}-----", etiqueta);
    let fin = format!("-----END {}-----", etiqueta);
    let desde = texto.find(&inicio)? + inicio.len();
    let hasta = desde + texto[desde..].find(&fin)?;
    let b64: String = texto[desde..hasta].chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// Titular, vencimiento y serie de un certificado en DER.
pub fn describir(der: &[u8]) -> Result<Descripcion, String> {
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|_| "El archivo no contiene un certificado válido.".to_string())?;
    let sujeto = cert.subject().to_string();
    let titular = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| sujeto.clone());
    let segundos = cert.validity().not_after.timestamp();
    let vence = chrono::DateTime::from_timestamp(segundos, 0)
        .map(|f| f.date_naive().format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    Ok(Descripcion {
        titular,
        sujeto,
        vence,
        serie: cert.raw_serial_as_string(),
    })
}

/// Lee el archivo que se subió (.p12/.pfx con su clave, o .pem) y lo deja
/// listo para Lycet.
pub fn leer(archivo: &[u8], clave: &str) -> Result<CertificadoListo, String> {
    if archivo.is_empty() {
        return Err("El archivo del certificado está vacío.".to_string());
    }

    // ¿Es un PEM (texto)?
    if let Ok(texto) = std::str::from_utf8(archivo) {
        if texto.contains("-----BEGIN") {
            if texto.contains("ENCRYPTED PRIVATE KEY") || texto.contains("Proc-Type: 4,ENCRYPTED") {
                return Err("La clave privada del .pem está cifrada. Sube el .p12 original con su clave.".to_string());
            }
            if !texto.contains("PRIVATE KEY") {
                return Err("El .pem no trae la clave privada. Sube el .p12 original con su clave.".to_string());
            }
            let der = primer_bloque_pem(texto, "CERTIFICATE")
                .ok_or_else(|| "El .pem no trae el certificado.".to_string())?;
            return Ok(CertificadoListo {
                pem: texto.trim().to_string() + "\n",
                descripcion: describir(&der)?,
            });
        }
    }

    // .p12 / .pfx
    let almacen = KeyStore::from_pkcs12(archivo, clave, Pkcs12ImportPolicy::Relaxed).map_err(|_| {
        "No se pudo abrir el certificado: revisa la clave del certificado, o que el archivo sea el .p12/.pfx correcto."
            .to_string()
    })?;
    let (_, cadena) = almacen
        .private_key_chain()
        .ok_or_else(|| "El certificado no trae la clave privada.".to_string())?;
    let certificados = cadena.certs();
    let propio = certificados
        .first()
        .ok_or_else(|| "El archivo trae la clave privada pero no el certificado.".to_string())?;

    let mut pem = String::new();
    for cert in certificados {
        pem.push_str(&a_pem("CERTIFICATE", cert.as_der()));
    }
    pem.push_str(&a_pem("PRIVATE KEY", cadena.key().as_der()));

    Ok(CertificadoListo {
        pem,
        descripcion: describir(propio.as_der())?,
    })
}

/// El certificado con que se firmó un XML (la etiqueta X509Certificate de
/// la firma), en DER. Sirve para comprobar con qué certificado firma Lycet.
pub fn certificado_del_xml(xml: &str) -> Option<Vec<u8>> {
    let inicio = xml.find("X509Certificate>")? + "X509Certificate>".len();
    let fin = inicio + xml[inicio..].find('<')?;
    let b64: String = xml[inicio..fin].chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const MODERNO: &[u8] = include_bytes!("../../tests/recursos/prueba-moderno.p12");
    const LEGACY: &[u8] = include_bytes!("../../tests/recursos/prueba-legacy.p12");
    const PEM: &[u8] = include_bytes!("../../tests/recursos/prueba.pem");

    fn revisar(listo: &CertificadoListo) {
        assert_eq!(listo.descripcion.titular, "PRUEBA SAC");
        assert_eq!(listo.descripcion.vence, "2027-11-13");
        assert!(listo.descripcion.menciona_ruc("20161515648"));
        assert!(!listo.descripcion.menciona_ruc("10772205355"));
        assert!(listo.pem.contains("BEGIN CERTIFICATE"));
        assert!(listo.pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn convierte_p12_moderno_y_antiguo() {
        let a = leer(MODERNO, "abc123").unwrap();
        let b = leer(LEGACY, "abc123").unwrap();
        revisar(&a);
        revisar(&b);
        assert_eq!(a.descripcion.serie, b.descripcion.serie);
    }

    #[test]
    fn clave_incorrecta() {
        assert!(leer(MODERNO, "otra").unwrap_err().contains("clave"));
    }

    #[test]
    fn acepta_pem() {
        let p = leer(PEM, "").unwrap();
        revisar(&p);
        assert_eq!(p.descripcion.serie, leer(MODERNO, "abc123").unwrap().descripcion.serie);
    }

    #[test]
    fn pem_sin_clave_privada() {
        let solo_cert = std::str::from_utf8(PEM).unwrap().split("-----BEGIN PRIVATE KEY").next().unwrap().to_string();
        assert!(leer(solo_cert.as_bytes(), "").unwrap_err().contains("clave privada"));
    }

    #[test]
    fn lee_el_certificado_de_un_xml_firmado() {
        let der = primer_bloque_pem(std::str::from_utf8(PEM).unwrap(), "CERTIFICATE").unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
        let xml = format!("<ds:X509Data><ds:X509Certificate>{}\n</ds:X509Certificate></ds:X509Data>", b64);
        assert_eq!(certificado_del_xml(&xml).unwrap(), der);
    }
}
