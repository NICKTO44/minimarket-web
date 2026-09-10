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
    let url = format!("https://api.factiliza.com/v1/dni/info/{}", dni);

    let resp = cliente_con_timeout()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        // Factiliza responde 400 cuando el DNI no existe -- eso SÍ es
        // una señal real ("no existe"), no un fallo de infraestructura.
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_completo").and_then(|v| v.as_str()).map(|s| s.to_string());

    Some(ValidacionDocumento { existe: true, nombre_completo, estado: None, condicion: None })
}

/// Valida un RUC de 11 dígitos contra Factiliza. Mismo contrato que
/// validar_dni: `None` = no se pudo validar (seguir sin bloquear).
pub async fn validar_ruc(ruc: &str) -> Option<ValidacionDocumento> {
    let token = token_factiliza()?;
    let url = format!("https://api.factiliza.com/v1/ruc/info/{}", ruc);

    let resp = cliente_con_timeout()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_o_razon_social").and_then(|v| v.as_str()).map(|s| s.to_string());
    let estado = data.get("estado").and_then(|v| v.as_str()).map(|s| s.to_string());
    let condicion = data.get("condicion").and_then(|v| v.as_str()).map(|s| s.to_string());

    Some(ValidacionDocumento { existe: true, nombre_completo, estado, condicion })
}