//! Alta de un negocio en la emisión directa: lo que hacen igual el panel de
//! Monspeet y el comando `sunat`.
//!
//! Dar de alta = registrar en Lycet, bajo el RUC del negocio, su certificado
//! y su usuario secundario SOL. Desde ahí cada comprobante que Monspeet manda
//! con ese RUC sale firmado con ese certificado y entregado con ese usuario.
//! En la base del negocio solo queda lo que sirve para mostrar (usuario SOL
//! sin clave, ambiente, titular y vencimiento del certificado).

use serde::Serialize;
use serde_json::json;

use crate::handlers::facturacion_directa::{leer_emisor, servidor_lycet};
use crate::logica::certificado::{self, CertificadoListo};
use crate::logica::facturacion::{DatosParaEmitir, ItemFactura};
use crate::logica::igv::Afectacion;
use crate::logica::{sunat_directo, tiempo};

pub const FE_PRODUCCION: &str = "https://e-factura.sunat.gob.pe/ol-ti-itcpfegem/billService";
pub const FE_BETA: &str = "https://e-beta.sunat.gob.pe/ol-ti-itcpfegem-beta/billService";

/// Datos del emisor que se pueden editar (columna de configuracion_tienda,
/// descripción).
pub const CAMPOS: [(&str, &str); 12] = [
    ("ruc", "RUC del negocio (11 dígitos)"),
    ("razon_social", "razón social exacta como figura en SUNAT"),
    ("nombre_tienda", "nombre comercial"),
    ("direccion", "dirección fiscal"),
    ("ubigeo", "ubigeo INEI de la dirección fiscal (6 dígitos)"),
    ("departamento", "departamento"),
    ("provincia", "provincia"),
    ("distrito", "distrito"),
    ("serie_boleta", "serie de boletas, ej. BM01"),
    ("serie_factura", "serie de facturas, ej. FM01"),
    ("serie_nc_boleta", "serie de notas de crédito de boletas, ej. BC01"),
    ("serie_nc_factura", "serie de notas de crédito de facturas, ej. FC01"),
];

/// Valida un dato del emisor y lo devuelve como se guarda.
pub fn validar_dato(campo: &str, valor: &str) -> Result<String, String> {
    if !CAMPOS.iter().any(|(c, _)| *c == campo) {
        return Err(format!("Campo desconocido: {}", campo));
    }
    let valor = valor.trim();
    let valor = if campo.starts_with("serie_") { valor.to_uppercase() } else { valor.to_string() };
    let digitos = |n: usize| valor.len() == n && valor.chars().all(|c| c.is_ascii_digit());
    let serie = |letra: char| {
        valor.len() == 4 && valor.starts_with(letra) && valor.chars().all(|c| c.is_ascii_alphanumeric())
    };
    let valido = match campo {
        "ruc" => digitos(11),
        "ubigeo" => digitos(6),
        "serie_boleta" | "serie_nc_boleta" => serie('B'),
        "serie_factura" | "serie_nc_factura" => serie('F'),
        _ => !valor.is_empty(),
    };
    if !valido {
        let ayuda = match campo {
            "ruc" => "debe tener 11 dígitos",
            "ubigeo" => "debe tener 6 dígitos",
            "serie_boleta" => "4 caracteres que empiezan con B, ej. BM01",
            "serie_factura" => "4 caracteres que empiezan con F, ej. FM01",
            "serie_nc_boleta" => "4 caracteres que empiezan con B, ej. BC01",
            "serie_nc_factura" => "4 caracteres que empiezan con F, ej. FC01",
            _ => "no puede quedar vacío",
        };
        return Err(format!("{}: {}", campo.replace('_', " "), ayuda));
    }
    Ok(valor)
}

/// 'BETA' o 'PRODUCCION' (acepta "beta", "produccion", "producción").
pub fn normalizar_ambiente(ambiente: &str) -> Option<&'static str> {
    match ambiente.trim().to_lowercase().as_str() {
        "beta" | "pruebas" => Some("BETA"),
        "produccion" | "producción" => Some("PRODUCCION"),
        _ => None,
    }
}

/// Resultado de un alta correcta.
#[derive(Debug, Serialize)]
pub struct AltaHecha {
    pub ruc: String,
    pub razon_social: String,
    pub ambiente: String,
    pub cert_titular: String,
    pub cert_vence: String,
    /// Aviso si el certificado no menciona el RUC del negocio.
    pub aviso: Option<String>,
}

/// Registra al negocio en Lycet y lo pasa a emisión directa.
/// `guias`: credenciales API de SUNAT del negocio (client_id, client_secret)
/// para emitir guías de remisión; None si no las tiene todavía.
pub async fn registrar(
    conn: &libsql::Connection,
    cert: &CertificadoListo,
    usuario_sol: &str,
    clave_sol: &str,
    ambiente: &str,
    guias: Option<(&str, &str)>,
) -> Result<AltaHecha, String> {
    let ambiente = normalizar_ambiente(ambiente).ok_or("El ambiente debe ser beta o producción.")?;
    let fe_url = if ambiente == "BETA" { FE_BETA } else { FE_PRODUCCION };
    let (lycet_url, lycet_token) =
        servidor_lycet().ok_or("Falta LYCET_URL en el .env del backend (el servidor de emisión).")?;

    let emisor = leer_emisor(conn).await.map_err(|(_, e)| e)?;
    if let Some(falta) = emisor.dato_faltante() {
        return Err(format!("Falta {} del negocio. Complétalo antes de dar de alta.", falta));
    }

    let usuario_sol = usuario_sol.trim().to_uppercase();
    // Si escribieron el usuario con el RUC delante, se quita (Lycet lo pide junto).
    let usuario_sol = usuario_sol.strip_prefix(emisor.ruc.trim()).unwrap_or(&usuario_sol).to_string();
    if usuario_sol.is_empty() || usuario_sol.len() > 8 || !usuario_sol.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("El usuario secundario SOL debe tener hasta 8 letras o números (ej. MONSPEET).".to_string());
    }
    if clave_sol.trim().is_empty() {
        return Err("Falta la clave SOL del usuario secundario.".to_string());
    }

    let mut cuerpo = json!({
        "SOL_USER": format!("{}{}", emisor.ruc.trim(), usuario_sol),
        "SOL_PASS": clave_sol.trim(),
        "certificate": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, cert.pem.as_bytes()),
        "FE_URL": fe_url,
    });
    let guias = guias.map(|(id, secreto)| (id.trim(), secreto.trim())).filter(|(id, s)| !id.is_empty() && !s.is_empty());
    if let Some((id, secreto)) = guias {
        cuerpo["CLIENT_ID"] = json!(id);
        cuerpo["CLIENT_SECRET"] = json!(secreto);
    }
    let respuesta = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .put(format!("{}/api/v1/configuration/company/{}", lycet_url, emisor.ruc.trim()))
        .query(&[("token", lycet_token.as_str())])
        .json(&cuerpo)
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con Lycet ({}): {}", lycet_url, e.without_url()))?;
    let status = respuesta.status();
    if !status.is_success() {
        let texto = respuesta.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => "Lycet rechazó la clave: LYCET_TOKEN del backend no coincide con CLIENT_TOKEN de Lycet.".to_string(),
            _ => format!("Lycet respondió {}: {}", status, texto.chars().take(300).collect::<String>()),
        });
    }

    let d = &cert.descripcion;
    // Lo que se muestra en el panel. Si el negocio aún no tiene la
    // migración 0021, el alta igual quedó hecha en Lycet.
    let _ = conn
        .execute(
            "UPDATE configuracion_tienda SET sunat_usuario_sol = ?1, sunat_ambiente = ?2, sunat_cert_titular = ?3,
                    sunat_cert_vence = ?4, sunat_cert_serie = ?5, sunat_alta_fecha = ?6",
            libsql::params![usuario_sol, ambiente, d.titular.clone(), d.vence.clone(), d.serie.clone(), tiempo::ahora_lima()],
        )
        .await;
    // El ID de las credenciales de guías (el secreto queda solo en Lycet).
    // Migración 0026; sin ella, el alta igual quedó hecha.
    let _ = conn
        .execute(
            "UPDATE configuracion_tienda SET sunat_gre_client_id = ?1",
            libsql::params![guias.map(|(id, _)| id.to_string())],
        )
        .await;
    conn.execute("UPDATE configuracion_tienda SET facturacion_proveedor = 'SUNAT_DIRECTO'", ())
        .await
        .map_err(|e| format!("Se registró en Lycet, pero no se pudo activar el modo directo: {}", e))?;

    let aviso = (!d.menciona_ruc(&emisor.ruc)).then(|| {
        format!(
            "El certificado está a nombre de \"{}\" y no menciona el RUC {}. Si es de otro negocio, SUNAT rechazará los comprobantes.",
            d.titular, emisor.ruc
        )
    });

    Ok(AltaHecha {
        ruc: emisor.ruc,
        razon_social: emisor.razon_social,
        ambiente: ambiente.to_string(),
        cert_titular: d.titular.clone(),
        cert_vence: d.vence.clone(),
        aviso,
    })
}

/// Un punto de la revisión de "Probar conexión".
#[derive(Debug, Serialize)]
pub struct Chequeo {
    /// "ok", "aviso" o "error".
    pub estado: &'static str,
    pub titulo: String,
    pub detalle: String,
}

fn chequeo(estado: &'static str, titulo: &str, detalle: impl Into<String>) -> Chequeo {
    Chequeo { estado, titulo: titulo.to_string(), detalle: detalle.into() }
}

async fn texto_config(conn: &libsql::Connection, columna: &str) -> String {
    let consulta = format!("SELECT {} FROM configuracion_tienda LIMIT 1", columna);
    match conn.query(&consulta, ()).await {
        Ok(mut filas) => match filas.next().await {
            Ok(Some(f)) => f.get::<String>(0).unwrap_or_default(),
            _ => String::new(),
        },
        Err(_) => String::new(),
    }
}

/// Revisa, sin enviar nada a SUNAT, que el negocio esté listo para emitir:
/// le pide a Lycet que firme una boleta de prueba (sin enviarla) y mira con
/// qué certificado la firmó. Lo único que no se puede probar sin emitir de
/// verdad es la clave SOL: esa la confirma el primer comprobante.
pub async fn probar(conn: &libsql::Connection) -> Vec<Chequeo> {
    let mut chequeos = Vec::new();

    let emisor = match leer_emisor(conn).await {
        Ok(e) => e,
        Err((_, e)) => {
            chequeos.push(chequeo("error", "Datos del emisor", e));
            return chequeos;
        }
    };
    match emisor.dato_faltante() {
        None => chequeos.push(chequeo("ok", "Datos del emisor", format!("{} · RUC {}", emisor.razon_social, emisor.ruc))),
        Some(falta) => {
            chequeos.push(chequeo("error", "Datos del emisor", format!("Falta {}.", falta)));
            return chequeos;
        }
    }

    let Some((url, token)) = servidor_lycet() else {
        chequeos.push(chequeo("error", "Servidor de emisión (Lycet)", "Falta LYCET_URL en el .env del backend."));
        return chequeos;
    };

    // Boleta de prueba de S/ 1.18: Lycet solo la firma, no la manda.
    let datos = DatosParaEmitir {
        tipo: "BOLETA".to_string(),
        cliente_tipo_documento: None,
        cliente_documento: None,
        cliente_nombre: None,
        cliente_direccion: None,
        subtotal: 1.0,
        igv: 0.18,
        total: 1.18,
        tasa: 18.0,
        exoneradas: 0.0,
        inafectas: 0.0,
        items: vec![ItemFactura {
            descripcion: "PRUEBA DE CONEXION".to_string(),
            cantidad: 1.0,
            precio_unitario: 1.18,
            unidad_medida: "UNIDAD".to_string(),
            afectacion: Afectacion::Gravado,
        }],
        detraccion: None,
        credito: None,
    };
    let documento = sunat_directo::armar_documento(&datos, &emisor, "B999", 1, &tiempo::hoy_lima(), "12:00:00", "");
    let respuesta = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build() {
        Ok(c) => c.post(format!("{}/api/v1/invoice/xml", url)).query(&[("token", token.as_str())]).json(&documento).send().await,
        Err(e) => {
            chequeos.push(chequeo("error", "Servidor de emisión (Lycet)", e.to_string()));
            return chequeos;
        }
    };
    let xml = match respuesta {
        Err(e) => {
            chequeos.push(chequeo(
                "error",
                "Servidor de emisión (Lycet)",
                format!("No responde en {} ({}). ¿Está encendido?", url, e.without_url()),
            ));
            return chequeos;
        }
        Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
            chequeos.push(chequeo(
                "error",
                "Servidor de emisión (Lycet)",
                "Responde, pero rechaza la clave: LYCET_TOKEN no coincide con CLIENT_TOKEN de Lycet.",
            ));
            return chequeos;
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let texto = r.text().await.unwrap_or_default();
            chequeos.push(chequeo(
                "error",
                "Servidor de emisión (Lycet)",
                format!("Respondió {}: {}", status, texto.chars().take(200).collect::<String>()),
            ));
            return chequeos;
        }
        Ok(r) => r.text().await.unwrap_or_default(),
    };
    chequeos.push(chequeo("ok", "Servidor de emisión (Lycet)", format!("Responde en {} y acepta la clave.", url)));

    // ¿Con qué certificado firmó?
    let firmado = certificado::certificado_del_xml(&xml).and_then(|der| certificado::describir(&der).ok());
    let Some(firmado) = firmado else {
        chequeos.push(chequeo("error", "Certificado", "Lycet no devolvió un XML firmado."));
        return chequeos;
    };
    let serie_guardada = texto_config(conn, "sunat_cert_serie").await;
    if !serie_guardada.is_empty() && serie_guardada != firmado.serie {
        chequeos.push(chequeo(
            "error",
            "Certificado de este negocio",
            format!(
                "Lycet firma con otro certificado (\"{}\"): este RUC no está registrado en Lycet o se registró otro. Vuelve a darlo de alta.",
                firmado.titular
            ),
        ));
        return chequeos;
    }
    if serie_guardada.is_empty() && !firmado.menciona_ruc(&emisor.ruc) {
        chequeos.push(chequeo(
            "aviso",
            "Certificado de este negocio",
            format!(
                "Lycet firma con \"{}\", que no menciona el RUC {}. Si nunca diste de alta este negocio, Lycet está usando su certificado por defecto.",
                firmado.titular, emisor.ruc
            ),
        ));
    } else {
        chequeos.push(chequeo("ok", "Certificado de este negocio", format!("Lycet firma con el certificado de \"{}\".", firmado.titular)));
    }

    let hoy = tiempo::hoy_lima();
    let en_30 = tiempo::hoy_lima_mas_dias(30);
    if firmado.vence < hoy {
        chequeos.push(chequeo("error", "Vigencia del certificado", format!("Venció el {}. Hay que renovarlo y volver a darlo de alta.", firmado.vence)));
    } else if firmado.vence < en_30 {
        chequeos.push(chequeo("aviso", "Vigencia del certificado", format!("Vence pronto: {}.", firmado.vence)));
    } else {
        chequeos.push(chequeo("ok", "Vigencia del certificado", format!("Vigente hasta el {}.", firmado.vence)));
    }

    let modo = texto_config(conn, "facturacion_proveedor").await;
    if modo == "SUNAT_DIRECTO" {
        let ambiente = texto_config(conn, "sunat_ambiente").await;
        let ambiente = match ambiente.as_str() {
            "BETA" => "beta (pruebas, sin valor legal)",
            "PRODUCCION" => "producción",
            _ => "ambiente no registrado (alta hecha con el comando antiguo)",
        };
        chequeos.push(chequeo("ok", "Modo de emisión", format!("Directo a SUNAT, en {}.", ambiente)));
    } else {
        chequeos.push(chequeo(
            "aviso",
            "Modo de emisión",
            "Todavía no emite directo: actívalo en \"Directo a SUNAT\" para que sus comprobantes usen todo esto.",
        ));
    }
    chequeos.push(chequeo(
        "aviso",
        "Clave SOL",
        "No se puede probar sin emitir: la confirma el primer comprobante (si está mal, SUNAT responde error 0102 o 0104).",
    ));
    chequeos
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn valida_datos_del_emisor() {
        assert_eq!(validar_dato("serie_boleta", " bm01 ").unwrap(), "BM01");
        assert!(validar_dato("serie_boleta", "FM01").is_err());
        assert!(validar_dato("serie_factura", "F-01").is_err());
        assert!(validar_dato("ruc", "1077220535").is_err());
        assert_eq!(validar_dato("ruc", "10772205355").unwrap(), "10772205355");
        assert!(validar_dato("ubigeo", "08010").is_err());
        assert!(validar_dato("razon_social", "  ").is_err());
        assert!(validar_dato("clave", "x").is_err());
    }

    #[test]
    fn ambientes() {
        assert_eq!(normalizar_ambiente("Beta"), Some("BETA"));
        assert_eq!(normalizar_ambiente("producción"), Some("PRODUCCION"));
        assert_eq!(normalizar_ambiente("otro"), None);
    }
}
