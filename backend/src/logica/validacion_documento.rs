use serde::Deserialize;

/// Resultado de intentar validar un documento contra Factiliza. `Some`
/// solo cuando la consulta se completó con éxito -- `None` cubre TODOS
/// los casos de "no se pudo confirmar" (token faltante, timeout, error
/// de red, 4xx/5xx de Factiliza, JSON inesperado): a propósito no se
/// distingue el motivo aquí, porque en ningún caso debe bloquear al
/// cajero. El único caso que sí es una señal clara es
/// `Some(ValidacionDocumento { existe: false, .. })`, cuando Factiliza
/// respondió con éxito pero el documento no es válido/no existe.
#[derive(Debug)]
pub struct ValidacionDocumento {
    pub existe: bool,
    pub nombre_completo: Option<String>,
    pub estado: Option<String>,
    pub condicion: Option<String>,
    /// Dirección (fiscal en el RUC) tal como la tiene SUNAT/RENIEC, con
    /// departamento, provincia y distrito. `None` si no figura.
    pub direccion: Option<String>,
}

/// Dirección que manda Factiliza. Viene en `direccion_completa` ("calle,
/// DEPARTAMENTO - PROVINCIA - DISTRITO") y suelta en `direccion`. Muchos
/// RUC de persona natural y casi todos los DNI no la tienen: ahí llega
/// vacía o como "-", y no debe guardarse como si fuera una dirección.
fn direccion_de(data: &serde_json::Value) -> Option<String> {
    let texto = |clave: &str| {
        data.get(clave).and_then(|v| v.as_str()).map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default()
    };
    let calle = texto("direccion");
    if !calle.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }
    let completa = texto("direccion_completa");
    Some(if completa.chars().any(|c| c.is_alphanumeric()) { completa } else { calle })
}

#[derive(Debug, Deserialize, Default)]
struct RespuestaFactiliza {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

const TIMEOUT_SEGUNDOS: u64 = 4;

fn token_factiliza() -> Option<String> {
    std::env::var("FACTILIZA_TOKEN").ok().filter(|t| !t.trim().is_empty())
}

/// Servidor de Factiliza. Solo se cambia (variable FACTILIZA_URL) para
/// las pruebas, que levantan uno simulado.
fn base_factiliza() -> String {
    std::env::var("FACTILIZA_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://api.factiliza.com".to_string())
}

fn cliente_con_timeout() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SEGUNDOS))
        .build()
        .unwrap_or_default()
}

/// Valida un DNI de 8 dígitos contra Factiliza. Nunca falla "hacia
/// arriba" -- cualquier problema (sin token, timeout, red caída,
/// respuesta rara) devuelve `None`, y quien llama debe tratarlo como
/// "no se pudo validar, continuar de todas formas".
pub async fn validar_dni(dni: &str) -> Option<ValidacionDocumento> {
    let token = token_factiliza()?;
    let url = format!("{}/v1/dni/info/{}", base_factiliza(), dni);

    let resp = cliente_con_timeout()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        // Factiliza responde 400 cuando el DNI no existe -- eso SÍ es
        // una señal real ("no existe"), no un fallo de infraestructura.
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_completo").and_then(|v| v.as_str()).map(|s| s.to_string());

    let direccion = direccion_de(&data);

    Some(ValidacionDocumento { existe: true, nombre_completo, estado: None, condicion: None, direccion })
}

/// Valida un RUC de 11 dígitos contra Factiliza. Mismo contrato que
/// validar_dni: `None` = no se pudo validar (seguir sin bloquear).
pub async fn validar_ruc(ruc: &str) -> Option<ValidacionDocumento> {
    let token = token_factiliza()?;
    let url = format!("{}/v1/ruc/info/{}", base_factiliza(), ruc);

    let resp = cliente_con_timeout()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_o_razon_social").and_then(|v| v.as_str()).map(|s| s.to_string());
    let estado = data.get("estado").and_then(|v| v.as_str()).map(|s| s.to_string());
    let condicion = data.get("condicion").and_then(|v| v.as_str()).map(|s| s.to_string());

    let direccion = direccion_de(&data);

    Some(ValidacionDocumento { existe: true, nombre_completo, estado, condicion, direccion })
}

/// RUC ya consultados en este arranque -> su dirección fiscal (o que no
/// tiene). Evita repetir la consulta en cada factura del mismo cliente.
fn direcciones_consultadas() -> &'static std::sync::Mutex<std::collections::HashMap<String, Option<String>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Option<String>>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Dirección fiscal de un RUC, para la factura de un cliente que quedó
/// guardado sin dirección. `None` si no figura o si no se pudo consultar
/// (en ese caso no se recuerda nada y la próxima vez se vuelve a intentar).
pub async fn direccion_fiscal(ruc: &str) -> Option<String> {
    if ruc.len() != 11 || !ruc.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if let Some(guardada) = direcciones_consultadas().lock().ok()?.get(ruc).cloned() {
        return guardada;
    }
    let consulta = validar_ruc(ruc).await?;
    // Solo se recuerda una respuesta buena: "no existe" también es lo que
    // llega cuando Factiliza está caído o se acabaron las consultas del mes.
    if consulta.existe {
        if let Ok(mut cache) = direcciones_consultadas().lock() {
            cache.insert(ruc.to_string(), consulta.direccion.clone());
        }
    }
    consulta.direccion
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use serde_json::json;

    #[test]
    fn direccion_del_ruc_con_departamento() {
        let data = json!({
            "nombre_o_razon_social": "AGROLIGHT PERU S.A.C.",
            "direccion": "PJ. JORGE BASADRE NRO. 158 URB. POP LA UNIVERSAL 2DA ET.",
            "direccion_completa": "PJ. JORGE BASADRE NRO. 158 URB. POP LA UNIVERSAL 2DA ET., LIMA - LIMA - SANTA ANITA",
        });
        assert_eq!(
            direccion_de(&data).as_deref(),
            Some("PJ. JORGE BASADRE NRO. 158 URB. POP LA UNIVERSAL 2DA ET., LIMA - LIMA - SANTA ANITA")
        );
    }

    #[test]
    fn sin_direccion_no_inventa_una() {
        // Persona natural sin domicilio publicado: SUNAT manda "-".
        assert_eq!(direccion_de(&json!({ "direccion": "-", "direccion_completa": "-, CUSCO - CUSCO - WANCHAQ" })), None);
        assert_eq!(direccion_de(&json!({ "direccion": "", "direccion_completa": "" })), None);
        assert_eq!(direccion_de(&json!({ "direccion": "  ", "direccion_completa": ", LIMA - LIMA - LIMA" })), None);
        assert_eq!(direccion_de(&json!({ "direccion": null })), None);
        assert_eq!(direccion_de(&json!({})), None);
    }

    #[test]
    fn solo_la_calle_y_espacios_de_mas() {
        assert_eq!(direccion_de(&json!({ "direccion": "AV.  EL SOL   500 " })).as_deref(), Some("AV. EL SOL 500"));
        assert_eq!(
            direccion_de(&json!({ "direccion": "CASERIO PUÑA", "direccion_completa": "" })).as_deref(),
            Some("CASERIO PUÑA")
        );
    }
}