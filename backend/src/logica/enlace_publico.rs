//! Clave de los enlaces públicos de comprobantes
//! (monspeet.pages.dev/boleta/<identificador>/<id>?k=<clave>).
//!
//! Antes bastaba el id del comprobante, que es correlativo: cambiando el
//! número se podían ver los comprobantes de otros clientes del negocio. Ahora
//! el enlace lleva además una firma HMAC-SHA256 del negocio y del comprobante,
//! hecha con una clave que solo tiene el servidor. Sin esa firma la página
//! pública no muestra nada.
//!
//! No necesita columnas nuevas ni migración: la clave se calcula, no se
//! guarda, así que sirve igual para los comprobantes ya emitidos.
//!
//! La clave del servidor sale de ENLACES_SECRET en el .env o, si no está, de
//! JWT_SECRET. Si se cambia esa clave, los enlaces enviados antes dejan de
//! funcionar (el comprobante sigue en el sistema y se puede reenviar).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::hmac;
use std::sync::OnceLock;

/// Bytes de la firma que van en el enlace (16 = 128 bits, 22 caracteres).
const BYTES_CLAVE: usize = 16;

fn llave() -> Option<&'static hmac::Key> {
    static LLAVE: OnceLock<Option<hmac::Key>> = OnceLock::new();
    LLAVE
        .get_or_init(|| {
            let secreto = std::env::var("ENLACES_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| std::env::var("JWT_SECRET").ok())
                .filter(|s| !s.trim().is_empty())?;
            Some(hmac::Key::new(hmac::HMAC_SHA256, secreto.as_bytes()))
        })
        .as_ref()
}

fn firmar(llave: &hmac::Key, tienda_id: i64, comprobante_id: i64) -> String {
    let mensaje = format!("monspeet-comprobante-publico:{}:{}", tienda_id, comprobante_id);
    let firma = hmac::sign(llave, mensaje.as_bytes());
    URL_SAFE_NO_PAD.encode(&firma.as_ref()[..BYTES_CLAVE])
}

/// Clave del enlace público de ese comprobante. None solo si el servidor no
/// tiene ninguna clave configurada (no debería pasar: JWT_SECRET es obligatoria).
pub fn clave(tienda_id: i64, comprobante_id: i64) -> Option<String> {
    llave().map(|l| firmar(l, tienda_id, comprobante_id))
}

/// true si `recibida` es la clave correcta de ese comprobante. La comparación
/// no se corta en el primer carácter distinto (no da pistas por el tiempo).
pub fn es_valida(tienda_id: i64, comprobante_id: i64, recibida: &str) -> bool {
    match llave() {
        Some(l) => iguales(firmar(l, tienda_id, comprobante_id).as_bytes(), recibida.trim().as_bytes()),
        None => false,
    }
}

fn iguales(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn llave_de_prueba() -> hmac::Key {
        hmac::Key::new(hmac::HMAC_SHA256, b"clave de prueba")
    }

    #[test]
    fn la_firma_es_estable_y_depende_del_negocio_y_del_comprobante() {
        let l = llave_de_prueba();
        let a = firmar(&l, 7, 15);
        assert_eq!(a, firmar(&l, 7, 15));
        assert_eq!(a.len(), 22);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, firmar(&l, 7, 16), "otro comprobante, otra clave");
        assert_ne!(a, firmar(&l, 8, 15), "otro negocio, otra clave");
        let otra = hmac::Key::new(hmac::HMAC_SHA256, b"otra clave");
        assert_ne!(a, firmar(&otra, 7, 15), "otra clave del servidor, otra firma");
    }

    #[test]
    fn comparacion() {
        assert!(iguales(b"abc", b"abc"));
        assert!(!iguales(b"abc", b"abd"));
        assert!(!iguales(b"abc", b"ab"));
        assert!(!iguales(b"", b"a"));
    }
}
