//! Validaciones de FORMATO del formulario de registro de negocios.
//!
//! Solo revisan el texto que llega: no consultan la base de datos ni tocan
//! la creación del negocio, el login o los usuarios existentes. Se usan en
//! POST /registro como filtro de entrada, antes de crear nada.
//!
//! El frontend tiene una copia de estas mismas reglas
//! (frontend/src/utils/validacionRegistro.js) para avisar mientras se
//! escribe; si se cambia una, hay que cambiar la otra.

/// Prefijos de RUC que existen: 10 persona natural, 15/16/17 casos
/// especiales (no domiciliados, etc.), 20 persona jurídica.
const PREFIJOS_RUC: [&str; 5] = ["10", "15", "16", "17", "20"];

/// Pesos del dígito verificador del RUC (módulo 11, algoritmo de SUNAT).
const PESOS_RUC: [u32; 10] = [5, 4, 3, 2, 7, 6, 5, 4, 3, 2];

/// Mensaje para el usuario si el RUC no tiene un formato posible, o `None`
/// si el formato es correcto. No dice si el RUC existe en SUNAT: eso lo
/// responde la consulta a Factiliza.
pub fn error_formato_ruc(ruc: &str) -> Option<&'static str> {
    if ruc.is_empty() {
        return Some("El RUC es obligatorio.");
    }
    if !ruc.chars().all(|c| c.is_ascii_digit()) {
        return Some("El RUC solo lleva números.");
    }
    if ruc.len() != 11 {
        return Some("El RUC tiene 11 dígitos.");
    }
    if !PREFIJOS_RUC.contains(&&ruc[0..2]) {
        return Some("El RUC debe empezar con 10, 15, 16, 17 o 20.");
    }
    if !digito_verificador_correcto(ruc) {
        return Some("Ese número de RUC no es válido. Revisa que esté bien escrito.");
    }
    None
}

/// `ruc` ya trae 11 dígitos ASCII.
fn digito_verificador_correcto(ruc: &str) -> bool {
    let digitos: Vec<u32> = ruc.chars().filter_map(|c| c.to_digit(10)).collect();
    let suma: u32 = digitos[..10].iter().zip(PESOS_RUC).map(|(d, p)| d * p).sum();
    let esperado = match 11 - (suma % 11) {
        10 => 0,
        11 => 1,
        d => d,
    };
    digitos[10] == esperado
}

/// DNI que va dentro de un RUC de persona natural (prefijo 10): los dígitos
/// 3 al 10. `None` para cualquier otro RUC.
pub fn dni_de_ruc(ruc: &str) -> Option<&str> {
    (ruc.len() == 11 && ruc.starts_with("10") && ruc.chars().all(|c| c.is_ascii_digit())).then(|| &ruc[2..10])
}

// ---------------------------------------------------------------------
// Texto escrito al azar ("shfhsfshfh", "wjrhdgsjdgd", "shasjjsahasjs").
//
// No hay forma perfecta de saber si un nombre "tiene sentido", así que se
// buscan solo señales FUERTES de teclazos al azar, palabra por palabra, y
// se deja pasar todo lo demás. Se prefiere dejar pasar algo raro antes que
// rechazar un nombre real (apellidos quechuas, nombres extranjeros, marcas).
// ---------------------------------------------------------------------

const FILAS_TECLADO: [&str; 3] = ["qwertyuiop", "asdfghjklñ", "zxcvbnm"];

fn es_vocal(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

fn sin_tildes(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        otro => otro,
    }
}

/// Palabras (solo letras, en minúsculas y sin tildes) de un texto.
fn palabras(texto: &str) -> Vec<Vec<char>> {
    let mut resultado = Vec::new();
    let mut actual = Vec::new();
    for c in texto.chars().flat_map(char::to_lowercase).map(sin_tildes) {
        if c.is_alphabetic() {
            actual.push(c);
        } else if !actual.is_empty() {
            resultado.push(std::mem::take(&mut actual));
        }
    }
    if !actual.is_empty() {
        resultado.push(actual);
    }
    resultado
}

fn palabra_al_azar(p: &[char]) -> bool {
    let largo = p.len();
    if largo < 4 {
        // Siglas e iniciales ("SAC", "JR", "D&J") son normales.
        return false;
    }
    let vocales = p.iter().filter(|c| es_vocal(**c)).count();

    // 1. Ninguna vocal en una palabra de 4 letras o más: "shfhsfshfh".
    if vocales == 0 {
        return true;
    }

    // 2. Cinco consonantes seguidas o más ("Schmidt" tiene 4 y pasa).
    let mut seguidas = 0;
    for c in p {
        seguidas = if es_vocal(*c) { 0 } else { seguidas + 1 };
        if seguidas >= 5 {
            return true;
        }
    }

    // 3. La misma letra tres veces seguidas: "aaaa", "jjjhola".
    if p.windows(3).any(|w| w[0] == w[1] && w[1] == w[2]) {
        return true;
    }

    // 4. Un mismo par de letras repetido 3 veces seguidas: "jsjsjs", "asasas".
    if p.windows(6).any(|w| w[0] == w[2] && w[2] == w[4] && w[1] == w[3] && w[3] == w[5] && w[0] != w[1]) {
        return true;
    }

    // 5. Casi todo en una sola fila del teclado Y con pocas vocales:
    //    "shasjjsahasjs", "asdfgh". Las palabras reales que caen en una
    //    sola fila ("salsa", "territorio", "Torres") tienen más vocales.
    //    El umbral es bajo a propósito: "qwerty" pasa, pero "Torres" y
    //    "Tottus" (un tercio de vocales) no deben rechazarse.
    if largo >= 5 {
        let proporcion_vocales = vocales as f32 / largo as f32;
        let en_una_fila = FILAS_TECLADO.iter().any(|fila| {
            let en_fila = p.iter().filter(|c| fila.contains(**c)).count();
            en_fila as f32 / largo as f32 >= 0.8
        });
        if en_una_fila && proporcion_vocales < 0.3 {
            return true;
        }
    }

    false
}

/// ¿Parece un texto escrito al azar? Basta con una palabra sospechosa.
pub fn parece_al_azar(texto: &str) -> bool {
    palabras(texto).iter().any(|p| palabra_al_azar(p))
}

fn cantidad_letras(texto: &str) -> usize {
    texto.chars().filter(|c| c.is_alphabetic()).count()
}

/// Nombre del negocio o de la persona: tiene que tener letras y no parecer
/// escrito al azar. `campo` va en el mensaje ("El nombre del negocio...").
pub fn error_nombre(texto: &str, campo: &str) -> Option<String> {
    let texto = texto.trim();
    if cantidad_letras(texto) < 2 {
        return Some(format!("{} debe tener letras.", campo));
    }
    if parece_al_azar(texto) {
        return Some(format!("{} parece escrito al azar. Escríbelo como aparece en tus documentos.", campo));
    }
    None
}

/// Usuario elegido al registrarse. Solo se revisa que no sea un texto al
/// azar y que lleve letras; si está libre o no lo sigue decidiendo el
/// registro de siempre (no cambia nada del login).
pub fn error_usuario(usuario: &str) -> Option<&'static str> {
    let usuario = usuario.trim();
    if cantidad_letras(usuario) < 3 {
        return Some("El usuario debe tener al menos 3 letras.");
    }
    if parece_al_azar(usuario) {
        return Some("El usuario parece escrito al azar. Usa algo fácil de recordar, como el nombre de tu negocio.");
    }
    None
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn ruc_reales_pasan() {
        for ruc in ["20131312955", "20100047218", "20100070970", "20601030013", "10467793549"] {
            assert_eq!(error_formato_ruc(ruc), None, "debería pasar: {}", ruc);
        }
    }

    #[test]
    fn ruc_con_formato_imposible() {
        assert!(error_formato_ruc("").is_some());
        assert!(error_formato_ruc("shasjjsahasjs").is_some());
        assert!(error_formato_ruc("2013131295").is_some()); // 10 dígitos
        assert!(error_formato_ruc("201313129551").is_some()); // 12 dígitos
        assert!(error_formato_ruc("30131312955").is_some()); // prefijo inexistente
        assert!(error_formato_ruc("20131312956").is_some()); // dígito verificador
        assert!(error_formato_ruc("20 131312955").is_some());
        assert!(error_formato_ruc("12345678901").is_some());
    }

    #[test]
    fn dni_dentro_del_ruc() {
        assert_eq!(dni_de_ruc("10467793549"), Some("46779354"));
        assert_eq!(dni_de_ruc("20131312955"), None);
    }

    #[test]
    fn textos_al_azar_se_detectan() {
        for t in [
            "shfhsfshfh",
            "wjrhdgsjdgd",
            "shasjjsahasjs",
            "asdfgh",
            "jsjsjsjs",
            "aaaaaa",
            "Bodega xkcdfgh",
            "zxcvbn",
            "hjkl ñlkj",
        ] {
            assert!(parece_al_azar(t), "debería detectarse: {}", t);
        }
    }

    #[test]
    fn nombres_reales_pasan() {
        for t in [
            "Cristhian",
            "Quispe Mamani Juan Carlos",
            "Ccahuana Huaman",
            "Schmidt",
            "Shirley",
            "Jhon",
            "Inversiones Verane S.A.C.",
            "Cabañita",
            "Ferretería San José E.I.R.L.",
            "Bodega Don Pepe",
            "Salsa y Sazón",
            "Territorio Andino",
            "Pastelería Dulce Hogar",
            "Minimarket D&J",
            "Llallagua",
            "Wong",
            "Strauss",
            "verane",
            "cabanita",
            "ferreteria.sanjose",
            "jhoel123",
            "dani_pos",
            "Ayacucho",
            "Ollantaytambo",
            "Huancayo",
            "Torres",
            "Tottus",
            "Pretty",
            "Ttito",
        ] {
            assert!(!parece_al_azar(t), "no debería detectarse: {}", t);
        }
    }

    #[test]
    fn mensajes_de_campos() {
        assert!(error_nombre("", "El nombre").is_some());
        assert!(error_nombre("12345", "El nombre").is_some());
        assert!(error_nombre("wjrhdgsjdgd", "El nombre").is_some());
        assert_eq!(error_nombre("Juan Pérez", "El nombre"), None);

        assert!(error_usuario("ab").is_some());
        assert!(error_usuario("12345").is_some());
        assert!(error_usuario("shasjjsahasjs").is_some());
        assert_eq!(error_usuario("verane"), None);
        assert_eq!(error_usuario("tienda2026"), None);
    }
}
