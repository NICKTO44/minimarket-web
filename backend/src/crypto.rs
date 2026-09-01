use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};

/// Cifra un texto con AES-256-GCM. El resultado es un string en base64 que
/// contiene el nonce (12 bytes, uno distinto cada vez) seguido del texto
/// cifrado — todo junto, para no necesitar una columna aparte para el nonce.
pub fn cifrar(texto: &str, clave: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new(clave.into());
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let cifrado = cipher
        .encrypt(&nonce, texto.as_bytes())
        .map_err(|e| format!("Error cifrando: {}", e))?;

    let mut combinado = nonce.to_vec();
    combinado.extend_from_slice(&cifrado);
    Ok(STANDARD.encode(combinado))
}

/// Descifra un texto generado por `cifrar`.
pub fn descifrar(texto_cifrado: &str, clave: &[u8; 32]) -> Result<String, String> {
    let combinado = STANDARD
        .decode(texto_cifrado)
        .map_err(|e| format!("Error decodificando base64: {}", e))?;

    if combinado.len() < 12 {
        return Err("Dato cifrado inválido (muy corto)".into());
    }

    let (nonce_bytes, cifrado) = combinado.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new(clave.into());
    let texto = cipher
        .decrypt(nonce, cifrado)
        .map_err(|e| format!("Error descifrando (¿la clave CENTRAL_ENCRYPTION_KEY es incorrecta?): {}", e))?;

    String::from_utf8(texto).map_err(|e| format!("Resultado descifrado no es texto válido: {}", e))
}

/// Lee CENTRAL_ENCRYPTION_KEY del .env (debe ser base64 de exactamente 32
/// bytes) y la deja lista para usar. Falla rápido al arrancar si está mal
/// puesta, en vez de fallar más tarde a mitad de una petición.
pub fn cargar_clave_desde_env() -> [u8; 32] {
    let texto = std::env::var("CENTRAL_ENCRYPTION_KEY")
        .expect("Falta CENTRAL_ENCRYPTION_KEY en .env (genera una con: openssl rand -base64 32)");
    let bytes = STANDARD
        .decode(&texto)
        .expect("CENTRAL_ENCRYPTION_KEY no es un base64 válido");
    bytes
        .try_into()
        .expect("CENTRAL_ENCRYPTION_KEY debe decodificar a exactamente 32 bytes (usa: openssl rand -base64 32)")
}