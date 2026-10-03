//! Fecha y hora de Perú (UTC-5 todo el año, sin horario de verano).
//! El servidor y la base (Turso) trabajan en UTC, así que 'localtime' de
//! SQLite no sirve: lo nuevo guarda la hora calculada aquí.

use chrono::{Duration, NaiveDate, Utc};

/// "2026-10-03 14:05:09"
pub fn ahora_lima() -> String {
    (Utc::now() - Duration::hours(5)).format("%Y-%m-%d %H:%M:%S").to_string()
}

/// "2026-10-03"
pub fn hoy_lima() -> String {
    (Utc::now() - Duration::hours(5)).format("%Y-%m-%d").to_string()
}

/// La fecha de hoy en Perú más N días ("2026-11-02").
pub fn hoy_lima_mas_dias(dias: i64) -> String {
    (Utc::now() - Duration::hours(5) + Duration::days(dias)).format("%Y-%m-%d").to_string()
}

/// true si el texto es una fecha "AAAA-MM-DD" real.
pub fn fecha_valida(texto: &str) -> bool {
    NaiveDate::parse_from_str(texto, "%Y-%m-%d").is_ok()
}
