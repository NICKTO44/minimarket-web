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
    /// Solo la calle (sin departamento, provincia ni distrito), para los
    /// formularios que piden el distrito aparte (guía de remisión).
    pub calle: Option<String>,
    /// Ubigeo INEI de 6 dígitos del domicilio, si Factiliza lo manda.
    pub ubigeo: Option<String>,
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

/// Solo la calle del domicilio (el campo `direccion` de Factiliza), sin
/// espacios de más. `None` si no figura (vacía o "-").
fn calle_de(data: &serde_json::Value) -> Option<String> {
    let calle = data
        .get("direccion")
        .and_then(|v| v.as_str())
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    calle.chars().any(|c| c.is_alphanumeric()).then_some(calle)
}

/// Ubigeo de 6 dígitos del domicilio. Factiliza lo manda como texto
/// (`ubigeo_sunat` o `ubigeo`) o como lista ["15", "1501", "150101"]; se
/// toma el de 6 dígitos que aparezca. `None` si no hay ninguno válido.
fn ubigeo_de(data: &serde_json::Value) -> Option<String> {
    let valido = |s: &str| {
        let s = s.trim();
        (s.len() == 6 && s.chars().all(|c| c.is_ascii_digit()) && s != "000000").then(|| s.to_string())
    };
    for clave in ["ubigeo_sunat", "ubigeo"] {
        match data.get(clave) {
            Some(serde_json::Value::String(s)) => {
                if let Some(u) = valido(s) {
                    return Some(u);
                }
            }
            Some(serde_json::Value::Array(lista)) => {
                if let Some(u) = lista.iter().rev().filter_map(|v| v.as_str()).find_map(valido) {
                    return Some(u);
                }
            }
            _ => {}
        }
    }
    None
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
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None, calle: None, ubigeo: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None, calle: None, ubigeo: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_completo").and_then(|v| v.as_str()).map(|s| s.to_string());

    let direccion = direccion_de(&data);

    let calle = calle_de(&data);
    let ubigeo = ubigeo_de(&data);

    Some(ValidacionDocumento { existe: true, nombre_completo, estado: None, condicion: None, direccion, calle, ubigeo })
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
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None, calle: None, ubigeo: None });
    }

    let cuerpo: RespuestaFactiliza = resp.json().await.ok()?;
    if !cuerpo.success {
        return Some(ValidacionDocumento { existe: false, nombre_completo: None, estado: None, condicion: None, direccion: None, calle: None, ubigeo: None });
    }

    let data = cuerpo.data?;
    let nombre_completo = data.get("nombre_o_razon_social").and_then(|v| v.as_str()).map(|s| s.to_string());
    let estado = data.get("estado").and_then(|v| v.as_str()).map(|s| s.to_string());
    let condicion = data.get("condicion").and_then(|v| v.as_str()).map(|s| s.to_string());

    let direccion = direccion_de(&data);

    let calle = calle_de(&data);
    let ubigeo = ubigeo_de(&data);

    Some(ValidacionDocumento { existe: true, nombre_completo, estado, condicion, direccion, calle, ubigeo })
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

// ---------------------------------------------------------------------
// Consulta de RUC para el REGISTRO de negocios nuevos.
//
// Es aparte de `validar_ruc` (que usa el cajero y no se toca) porque aquí
// sí importa POR QUÉ falla: un RUC que Factiliza no encuentra bloquea el
// registro, pero Factiliza caído o sin saldo NO debe bloquear a nadie.
// ---------------------------------------------------------------------

/// Datos del RUC que se muestran en la tarjeta del registro.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DatosRucRegistro {
    pub razon_social: Option<String>,
    pub nombre_comercial: Option<String>,
    pub estado: Option<String>,
    pub condicion: Option<String>,
    pub direccion: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConsultaRucRegistro {
    /// Factiliza respondió con los datos del RUC.
    Encontrado(DatosRucRegistro),
    /// Factiliza respondió que el RUC no existe (400/404, o success:false).
    NoExiste,
    /// No se pudo consultar: sin token, timeout, red, saldo agotado,
    /// error del servidor o respuesta rara. No debe bloquear el registro.
    NoDisponible,
}

/// Texto de un campo, o `None` si viene vacío o como "-".
fn texto_de(data: &serde_json::Value, clave: &str) -> Option<String> {
    let t = data.get(clave)?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    (t.chars().any(|c| c.is_alphanumeric())).then_some(t)
}

/// Clasifica la respuesta de Factiliza. Separado de la petición para
/// poder probarlo sin red.
fn clasificar_respuesta_ruc(status: u16, cuerpo: Option<RespuestaFactiliza>) -> ConsultaRucRegistro {
    match status {
        200..=299 => {}
        // Factiliza responde 400/404 cuando el documento no existe.
        400 | 404 => return ConsultaRucRegistro::NoExiste,
        // 401/402/403/429/5xx...: problema de token, saldo o servidor.
        _ => return ConsultaRucRegistro::NoDisponible,
    }
    let Some(cuerpo) = cuerpo else { return ConsultaRucRegistro::NoDisponible };
    if !cuerpo.success {
        return ConsultaRucRegistro::NoExiste;
    }
    let Some(data) = cuerpo.data else { return ConsultaRucRegistro::NoDisponible };
    let razon_social = texto_de(&data, "nombre_o_razon_social");
    if razon_social.is_none() {
        // Éxito sin nombre: respuesta rara, mejor no concluir nada.
        return ConsultaRucRegistro::NoDisponible;
    }
    ConsultaRucRegistro::Encontrado(DatosRucRegistro {
        razon_social,
        nombre_comercial: texto_de(&data, "nombre_comercial"),
        estado: texto_de(&data, "estado"),
        condicion: texto_de(&data, "condicion"),
        direccion: direccion_de(&data),
    })
}

/// RUC ya consultados para el registro, con la hora de la consulta. La
/// tarjeta del formulario y el POST /registro preguntan por el mismo RUC
/// casi seguidos: así se paga una sola consulta a Factiliza.
type CacheRegistro = std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, ConsultaRucRegistro)>>;

fn cache_registro() -> &'static CacheRegistro {
    static CACHE: std::sync::OnceLock<CacheRegistro> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

const VIGENCIA_ENCONTRADO: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);
const VIGENCIA_NO_EXISTE: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const MAX_EN_CACHE: usize = 2_000;

/// Espera de la consulta del registro. Más larga que la del cajero
/// (TIMEOUT_SEGUNDOS): Factiliza contesta en ~1 s cuando el RUC existe, pero
/// tarda 5 a 7 s en responder 404 cuando NO existe (medido el 22-oct-2026).
/// Con 4 s, un RUC inventado se cortaba antes del "no existe" y pasaba como
/// "no se pudo consultar".
const TIMEOUT_REGISTRO_SEGUNDOS: u64 = 12;

/// Consulta un RUC (ya validado en formato) para el registro.
pub async fn consultar_ruc_registro(ruc: &str) -> ConsultaRucRegistro {
    if let Ok(cache) = cache_registro().lock() {
        if let Some((cuando, resultado)) = cache.get(ruc) {
            let vigencia = match resultado {
                ConsultaRucRegistro::NoExiste => VIGENCIA_NO_EXISTE,
                _ => VIGENCIA_ENCONTRADO,
            };
            if cuando.elapsed() < vigencia {
                return resultado.clone();
            }
        }
    }

    let Some(token) = token_factiliza() else { return ConsultaRucRegistro::NoDisponible };
    let url = format!("{}/v1/ruc/info/{}", base_factiliza(), ruc);
    let cliente = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_REGISTRO_SEGUNDOS))
        .build()
        .unwrap_or_default();
    let resultado = match cliente.get(&url).bearer_auth(token).send().await {
        Err(_) => ConsultaRucRegistro::NoDisponible,
        Ok(resp) => {
            let status = resp.status().as_u16();
            let cuerpo = resp.json::<RespuestaFactiliza>().await.ok();
            clasificar_respuesta_ruc(status, cuerpo)
        }
    };

    // "No disponible" no se recuerda: la próxima vez se vuelve a intentar.
    if !matches!(resultado, ConsultaRucRegistro::NoDisponible) {
        if let Ok(mut cache) = cache_registro().lock() {
            if cache.len() >= MAX_EN_CACHE {
                cache.retain(|_, (cuando, _)| cuando.elapsed() < VIGENCIA_NO_EXISTE);
            }
            cache.insert(ruc.to_string(), (std::time::Instant::now(), resultado.clone()));
        }
    }
    resultado
}

#[cfg(test)]
mod pruebas_registro {
    use super::*;
    use serde_json::json;

    fn cuerpo(v: serde_json::Value) -> Option<RespuestaFactiliza> {
        serde_json::from_value(v).ok()
    }

    #[test]
    fn ruc_encontrado_trae_sus_datos() {
        let r = clasificar_respuesta_ruc(
            200,
            cuerpo(json!({ "success": true, "data": {
                "nombre_o_razon_social": "INVERSIONES VERANE S.A.C.",
                "nombre_comercial": "-",
                "estado": "ACTIVO", "condicion": "NO HABIDO",
                "direccion": "AV. EL SOL 500", "direccion_completa": "AV. EL SOL 500, CUSCO - CUSCO - CUSCO"
            }})),
        );
        match r {
            ConsultaRucRegistro::Encontrado(d) => {
                assert_eq!(d.razon_social.as_deref(), Some("INVERSIONES VERANE S.A.C."));
                assert_eq!(d.nombre_comercial, None);
                assert_eq!(d.condicion.as_deref(), Some("NO HABIDO"));
                assert_eq!(d.direccion.as_deref(), Some("AV. EL SOL 500, CUSCO - CUSCO - CUSCO"));
            }
            otro => panic!("esperaba Encontrado, llegó {:?}", otro),
        }
    }

    #[test]
    fn no_existe_solo_cuando_factiliza_lo_dice() {
        assert!(matches!(clasificar_respuesta_ruc(400, None), ConsultaRucRegistro::NoExiste));
        assert!(matches!(clasificar_respuesta_ruc(404, None), ConsultaRucRegistro::NoExiste));
        assert!(matches!(
            clasificar_respuesta_ruc(200, cuerpo(json!({ "success": false }))),
            ConsultaRucRegistro::NoExiste
        ));
    }

    #[test]
    fn fallas_de_factiliza_no_bloquean() {
        for status in [401, 402, 403, 429, 500, 502, 503] {
            assert!(
                matches!(clasificar_respuesta_ruc(status, None), ConsultaRucRegistro::NoDisponible),
                "status {}",
                status
            );
        }
        assert!(matches!(clasificar_respuesta_ruc(200, None), ConsultaRucRegistro::NoDisponible));
        assert!(matches!(
            clasificar_respuesta_ruc(200, cuerpo(json!({ "success": true, "data": {} }))),
            ConsultaRucRegistro::NoDisponible
        ));
    }
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
    fn calle_y_ubigeo_para_la_guia() {
        let data = json!({
            "direccion": "PJ. JORGE  BASADRE NRO. 158",
            "direccion_completa": "PJ. JORGE BASADRE NRO. 158, LIMA - LIMA - SANTA ANITA",
            "ubigeo": ["15", "1501", "150137"],
        });
        assert_eq!(calle_de(&data).as_deref(), Some("PJ. JORGE BASADRE NRO. 158"));
        assert_eq!(ubigeo_de(&data).as_deref(), Some("150137"));
        assert_eq!(ubigeo_de(&json!({ "ubigeo_sunat": "080108" })).as_deref(), Some("080108"));
        assert_eq!(ubigeo_de(&json!({ "ubigeo": "150101" })).as_deref(), Some("150101"));
        assert_eq!(ubigeo_de(&json!({ "ubigeo": "-" })), None);
        assert_eq!(ubigeo_de(&json!({ "ubigeo": ["15", "1501", "-"] })), None);
        assert_eq!(ubigeo_de(&json!({})), None);
        assert_eq!(calle_de(&json!({ "direccion": "-" })), None);
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