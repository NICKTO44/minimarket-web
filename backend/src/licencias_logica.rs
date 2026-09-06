use chrono::{Months, NaiveDate, Utc};
use rand::Rng;

/// Alfabeto de 32 símbolos, sin caracteres que se confundan al leerlos
/// o escribirlos a mano (sin 0/O, sin 1/I/L).
const ALFABETO: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

pub fn hoy() -> NaiveDate {
    Utc::now().date_naive()
}

pub fn parsear_fecha(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Calcula la nueva fecha de vencimiento. Si el negocio todavía no
/// venció, extiende desde su fecha actual (no le "roba" los días que ya
/// tenía pagados). Si ya venció (o nunca tuvo fecha), extiende desde hoy.
pub fn calcular_nueva_fecha(actual: Option<&str>, cantidad: i64, unidad: &str) -> Result<String, String> {
    let base = match actual.and_then(parsear_fecha) {
        Some(fecha) if fecha >= hoy() => fecha,
        _ => hoy(),
    };

    let nueva = match unidad {
        "dia" | "dias" | "día" | "días" => base
            .checked_add_signed(chrono::Duration::days(cantidad))
            .ok_or_else(|| "Fecha fuera de rango".to_string())?,
        "mes" | "meses" => base
            .checked_add_months(Months::new(cantidad.max(0) as u32))
            .ok_or_else(|| "Fecha fuera de rango".to_string())?,
        "anio" | "anios" | "año" | "años" => base
            .checked_add_months(Months::new((cantidad.max(0) * 12) as u32))
            .ok_or_else(|| "Fecha fuera de rango".to_string())?,
        otro => return Err(format!("Unidad no reconocida: '{}' (usa dias, meses o anios)", otro)),
    };

    Ok(nueva.format("%Y-%m-%d").to_string())
}

/// Genera un código de activación aleatorio, formato "MNSP-XXXX-XXXX-XXXX"
/// (12 caracteres al azar de un alfabeto de 32 símbolos = ~60 bits de
/// entropía — no adivinable por fuerza bruta). No hay ningún algoritmo
/// de validación que "reversear": el código solo es válido si existe
/// como fila en la tabla `codigos_activacion` de la base central.
pub fn generar_codigo() -> String {
    let mut rng = rand::thread_rng();
    let mut bloque = || -> String {
        (0..4)
            .map(|_| ALFABETO[rng.gen_range(0..ALFABETO.len())] as char)
            .collect()
    };

    format!("MNSP-{}-{}-{}", bloque(), bloque(), bloque())
}