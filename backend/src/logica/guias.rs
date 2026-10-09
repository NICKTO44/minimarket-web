//! Guía de remisión remitente electrónica: validación del formulario y
//! armado del documento para FacturaLibre (formato FacturadorPro,
//! POST /api/dispatches). Aquí no hay red ni base de datos: eso está en
//! handlers/guias.rs. Los nombres de campo salen de los ejemplos "Guia
//! Remisión Transporte Público / Privado" de la documentación oficial
//! (https://documenter.getpostman.com/view/6435177/TVRrUPuD).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::logica::facturacion::{codigo_tipo_documento_identidad, unidad_sunat};
use crate::logica::tiempo::fecha_valida;

/// Motivos de traslado que ofrece el sistema (catálogo 20 de SUNAT).
pub const MOTIVOS: &[(&str, &str)] = &[
    ("01", "Venta"),
    ("04", "Traslado entre establecimientos de la misma empresa"),
    ("13", "Otros"),
];

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Direccion {
    /// Ubigeo INEI del distrito (6 dígitos).
    pub ubigeo: String,
    pub direccion: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Transportista {
    pub ruc: String,
    pub nombre: String,
    /// Registro MTC de la empresa de transporte (opcional).
    #[serde(default)]
    pub mtc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Chofer {
    /// DNI del conductor.
    pub documento: String,
    pub nombres: String,
    pub apellidos: String,
    pub licencia: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ItemGuia {
    pub codigo: String,
    pub descripcion: String,
    pub cantidad: f64,
    /// Unidad del sistema ('KG', 'PIE_TABLAR', 'UNIDAD'...).
    pub unidad: String,
}

/// El formulario de una guía, tal como lo llena el usuario.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DatosGuia {
    /// Venta que se está despachando (de ahí salen los ítems y el comprobante).
    #[serde(default)]
    pub venta_id: Option<i64>,
    /// 'DNI' | 'RUC' | 'CE' | 'PASAPORTE'
    pub destinatario_tipo: String,
    pub destinatario_documento: String,
    pub destinatario_nombre: String,
    /// Código del motivo (ver MOTIVOS). '01' = venta.
    pub motivo: String,
    #[serde(default)]
    pub motivo_descripcion: Option<String>,
    /// 'PUBLICO' (empresa de transporte) | 'PRIVADO' (vehículo propio).
    pub modo: String,
    /// "AAAA-MM-DD"
    pub fecha_traslado: String,
    /// Peso bruto total en kilos.
    pub peso_total: f64,
    pub bultos: i64,
    pub partida: Direccion,
    pub llegada: Direccion,
    #[serde(default)]
    pub transportista: Option<Transportista>,
    #[serde(default)]
    pub chofer: Option<Chofer>,
    /// Placa del vehículo (transporte privado).
    #[serde(default)]
    pub placa: Option<String>,
    #[serde(default)]
    pub observaciones: Option<String>,
    #[serde(default)]
    pub items: Vec<ItemGuia>,
}

fn solo_digitos(t: &str, n: usize) -> bool {
    t.len() == n && t.chars().all(|c| c.is_ascii_digit())
}

fn limpio(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Placa como la pide SUNAT: mayúsculas, sin guion ni espacios.
pub fn normalizar_placa(placa: &str) -> String {
    placa.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

/// Revisa el formulario y lo devuelve normalizado (espacios, mayúsculas).
/// El error es un texto para mostrarle al usuario.
pub fn validar(datos: &DatosGuia, hoy: &str) -> Result<DatosGuia, String> {
    let mut d = datos.clone();

    d.destinatario_documento = d.destinatario_documento.trim().to_string();
    d.destinatario_nombre = limpio(&d.destinatario_nombre);
    match d.destinatario_tipo.as_str() {
        "DNI" if !solo_digitos(&d.destinatario_documento, 8) => return Err("El DNI del destinatario debe tener 8 dígitos.".into()),
        "RUC" if !solo_digitos(&d.destinatario_documento, 11) => return Err("El RUC del destinatario debe tener 11 dígitos.".into()),
        "DNI" | "RUC" => {}
        "CE" | "PASAPORTE" if d.destinatario_documento.is_empty() => return Err("Falta el documento del destinatario.".into()),
        "CE" | "PASAPORTE" => {}
        _ => return Err("La guía necesita el DNI o RUC del destinatario.".into()),
    }
    if d.destinatario_nombre.is_empty() {
        return Err("Falta el nombre del destinatario.".into());
    }

    if !MOTIVOS.iter().any(|(c, _)| *c == d.motivo) {
        return Err("Motivo de traslado no válido.".into());
    }
    d.motivo_descripcion = d.motivo_descripcion.as_deref().map(limpio).filter(|t| !t.is_empty());
    if d.motivo == "13" && d.motivo_descripcion.is_none() {
        return Err("Describe el motivo del traslado.".into());
    }

    if !fecha_valida(&d.fecha_traslado) {
        return Err("La fecha de traslado no es válida.".into());
    }
    if d.fecha_traslado.as_str() < hoy {
        return Err("La fecha de traslado no puede ser anterior a hoy.".into());
    }
    if !(d.peso_total > 0.0) || !d.peso_total.is_finite() {
        return Err("Escribe el peso total en kilos (mayor a 0).".into());
    }
    if d.bultos < 1 {
        return Err("El número de bultos debe ser al menos 1.".into());
    }

    for (nombre, dir) in [("partida", &mut d.partida), ("llegada", &mut d.llegada)] {
        dir.ubigeo = dir.ubigeo.trim().to_string();
        dir.direccion = limpio(&dir.direccion);
        if !solo_digitos(&dir.ubigeo, 6) {
            return Err(format!("Elige el distrito del punto de {}.", nombre));
        }
        if dir.direccion.is_empty() {
            return Err(format!("Falta la dirección del punto de {}.", nombre));
        }
    }

    match d.modo.as_str() {
        "PUBLICO" => {
            let mut t = d.transportista.clone().ok_or("Faltan los datos de la empresa de transporte.")?;
            t.ruc = t.ruc.trim().to_string();
            t.nombre = limpio(&t.nombre);
            t.mtc = t.mtc.as_deref().map(|m| m.trim().to_uppercase()).filter(|m| !m.is_empty());
            if !solo_digitos(&t.ruc, 11) {
                return Err("El RUC de la empresa de transporte debe tener 11 dígitos.".into());
            }
            if t.nombre.is_empty() {
                return Err("Falta la razón social de la empresa de transporte.".into());
            }
            d.transportista = Some(t);
            d.chofer = None;
            d.placa = None;
        }
        "PRIVADO" => {
            let mut c = d.chofer.clone().ok_or("Faltan los datos del conductor.")?;
            c.documento = c.documento.trim().to_string();
            c.nombres = limpio(&c.nombres).to_uppercase();
            c.apellidos = limpio(&c.apellidos).to_uppercase();
            c.licencia = c.licencia.chars().filter(|x| x.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
            if !solo_digitos(&c.documento, 8) {
                return Err("El DNI del conductor debe tener 8 dígitos.".into());
            }
            if c.nombres.is_empty() || c.apellidos.is_empty() {
                return Err("Faltan los nombres y apellidos del conductor.".into());
            }
            if c.licencia.len() < 9 || c.licencia.len() > 10 {
                return Err("La licencia de conducir tiene 9 o 10 caracteres (por ejemplo Q41784439).".into());
            }
            let placa = normalizar_placa(d.placa.as_deref().unwrap_or(""));
            if placa.len() < 6 || placa.len() > 8 {
                return Err("Escribe la placa del vehículo (6 caracteres, por ejemplo A1Y298).".into());
            }
            d.chofer = Some(c);
            d.placa = Some(placa);
            d.transportista = None;
        }
        _ => return Err("Elige transporte público o privado.".into()),
    }

    d.observaciones = d.observaciones.as_deref().map(limpio).filter(|t| !t.is_empty()).map(|t| t.chars().take(250).collect());
    if d.items.is_empty() {
        return Err("La guía no tiene productos.".into());
    }
    for it in &mut d.items {
        it.descripcion = limpio(&it.descripcion);
        if it.descripcion.is_empty() || !(it.cantidad > 0.0) || !it.cantidad.is_finite() {
            return Err("Hay un producto sin descripción o sin cantidad.".into());
        }
    }
    Ok(d)
}

/// Datos del negocio que emite la guía.
pub struct Emisor {
    pub ubigeo: String,
    pub direccion: String,
    pub correo: Option<String>,
    pub telefono: Option<String>,
}

/// Comprobante de la venta que se despacha: (serie, número, 'FACTURA' | 'BOLETA').
pub type DocumentoAfectado = (String, i64, String);

/// Arma el documento para POST /api/dispatches. `datos` debe venir de `validar`.
pub fn armar_payload(
    datos: &DatosGuia,
    emisor: &Emisor,
    serie: &str,
    hoy: &str,
    hora: &str,
    documento: Option<&DocumentoAfectado>,
) -> serde_json::Value {
    let mut datos_emisor = json!({
        "codigo_pais": "PE",
        "ubigeo": emisor.ubigeo,
        "direccion": emisor.direccion,
        "codigo_del_domicilio_fiscal": "0000",
    });
    if let Some(c) = emisor.correo.as_deref().filter(|c| c.contains('@')) {
        datos_emisor["correo_electronico"] = json!(c);
    }
    if let Some(t) = emisor.telefono.as_deref().filter(|t| !t.trim().is_empty()) {
        datos_emisor["telefono"] = json!(t);
    }

    let privado = datos.modo == "PRIVADO";
    let motivo_texto = datos
        .motivo_descripcion
        .clone()
        .unwrap_or_else(|| MOTIVOS.iter().find(|(c, _)| *c == datos.motivo).map(|(_, n)| n.to_string()).unwrap_or_default());
    let items: Vec<_> = datos
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            json!({
                "codigo_interno": if it.codigo.trim().is_empty() { format!("P{:03}", i + 1) } else { it.codigo.trim().to_string() },
                "cantidad": it.cantidad,
                "descripcion": it.descripcion,
                "unidad_de_medida": unidad_sunat(&it.unidad),
                // Cuentas con el sistema de FacturaLibre: si el producto no
                // está en su catálogo, que lo cree en vez de rechazar la guía.
                "crear_si_no_existe": true,
            })
        })
        .collect();

    let mut doc = json!({
        "serie_documento": serie,
        "numero_documento": "#",
        "fecha_de_emision": hoy,
        "hora_de_emision": hora,
        "codigo_tipo_documento": "09",
        "datos_del_emisor": datos_emisor,
        "datos_del_cliente_o_receptor": {
            "codigo_tipo_documento_identidad": codigo_tipo_documento_identidad(&datos.destinatario_tipo),
            "numero_documento": datos.destinatario_documento,
            "apellidos_y_nombres_o_razon_social": datos.destinatario_nombre,
            "codigo_pais": "PE",
            "ubigeo": datos.llegada.ubigeo,
            "direccion": datos.llegada.direccion,
        },
        "observaciones": datos.observaciones.clone().unwrap_or_default(),
        "orden_pedido_externo": "",
        "codigo_modo_transporte": if privado { "02" } else { "01" },
        "codigo_motivo_traslado": datos.motivo,
        "descripcion_motivo_traslado": motivo_texto,
        "fecha_de_traslado": datos.fecha_traslado,
        "fecha_entrega_bienes_transportista": datos.fecha_traslado,
        "codigo_de_puerto": "",
        "indicador_de_transbordo": false,
        "unidad_peso_total": "KGM",
        "peso_total": datos.peso_total,
        "numero_de_bultos": datos.bultos,
        "numero_de_contenedor": "",
        "traslado_categoria_m1l": false,
        "numero_de_placa": if privado { datos.placa.clone().unwrap_or_default() } else { String::new() },
        "direccion_partida": {
            "ubigeo": datos.partida.ubigeo,
            "direccion": datos.partida.direccion,
            "codigo_del_domicilio_fiscal": "0000",
        },
        "direccion_llegada": {
            "ubigeo": datos.llegada.ubigeo,
            "direccion": datos.llegada.direccion,
            "codigo_del_domicilio_fiscal": "0000",
        },
        "items": items,
    });

    if privado {
        if let Some(c) = &datos.chofer {
            doc["chofer"] = json!({
                "codigo_tipo_documento_identidad": "1",
                "numero_documento": c.documento,
                "nombres": c.nombres,
                "apellidos": c.apellidos,
                "numero_licencia": c.licencia,
            });
        }
        doc["vehiculo"] = json!({ "numero_de_placa": datos.placa.clone().unwrap_or_default() });
    } else if let Some(t) = &datos.transportista {
        let mut transportista = json!({
            "codigo_tipo_documento_identidad": "6",
            "numero_documento": t.ruc,
            "apellidos_y_nombres_o_razon_social": t.nombre,
        });
        if let Some(mtc) = &t.mtc {
            transportista["numero_mtc"] = json!(mtc);
        }
        doc["transportista"] = transportista;
    }

    if let Some((serie_doc, numero_doc, tipo)) = documento {
        doc["documento_afectado"] = json!({
            "serie_documento": serie_doc,
            "numero_documento": numero_doc.to_string(),
            "codigo_tipo_documento": if tipo == "FACTURA" { "01" } else { "03" },
        });
    }
    doc
}

/// Arma la guía para la emisión directa (formato de Greenter, guía 2022 de
/// la API de SUNAT). `datos` debe venir de `validar`; `emisor` son los datos
/// del negocio como en las boletas y facturas directas.
#[allow(clippy::too_many_arguments)]
pub fn armar_guia_sunat(
    datos: &DatosGuia,
    emisor: &crate::logica::sunat_directo::Emisor,
    serie: &str,
    numero: i64,
    hoy: &str,
    hora: &str,
    documento: Option<&DocumentoAfectado>,
) -> Value {
    let privado = datos.modo == "PRIVADO";
    let motivo_texto = datos
        .motivo_descripcion
        .clone()
        .unwrap_or_else(|| MOTIVOS.iter().find(|(c, _)| *c == datos.motivo).map(|(_, n)| n.to_string()).unwrap_or_default());
    let detalles: Vec<Value> = datos
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            json!({
                "codigo": if it.codigo.trim().is_empty() { format!("P{:03}", i + 1) } else { it.codigo.trim().to_string() },
                "descripcion": it.descripcion,
                "unidad": unidad_sunat(&it.unidad),
                "cantidad": it.cantidad,
            })
        })
        .collect();

    let mut envio = json!({
        "codTraslado": datos.motivo,
        "desTraslado": motivo_texto,
        "modTraslado": if privado { "02" } else { "01" },
        "fecTraslado": format!("{}T00:00:00-05:00", datos.fecha_traslado),
        "pesoTotal": datos.peso_total,
        "undPesoTotal": "KGM",
        "numBultos": datos.bultos,
        "llegada": { "ubigueo": datos.llegada.ubigeo, "direccion": datos.llegada.direccion },
        "partida": { "ubigueo": datos.partida.ubigeo, "direccion": datos.partida.direccion },
    });
    if privado {
        if let Some(c) = &datos.chofer {
            envio["choferes"] = json!([{
                "tipo": "Principal",
                "tipoDoc": "1",
                "nroDoc": c.documento,
                "licencia": c.licencia,
                "nombres": c.nombres,
                "apellidos": c.apellidos,
            }]);
        }
        envio["vehiculo"] = json!({ "placa": datos.placa.clone().unwrap_or_default() });
    } else if let Some(t) = &datos.transportista {
        let mut transportista = json!({ "tipoDoc": "6", "numDoc": t.ruc, "rznSocial": t.nombre });
        if let Some(mtc) = &t.mtc {
            transportista["nroMtc"] = json!(mtc);
        }
        envio["transportista"] = transportista;
    }

    let nombre_comercial = if emisor.nombre_comercial.trim().is_empty() { emisor.razon_social.trim() } else { emisor.nombre_comercial.trim() };
    let mut doc = json!({
        "version": "2022",
        "tipoDoc": "09",
        "serie": serie,
        "correlativo": numero.to_string(),
        "fechaEmision": format!("{}T{}-05:00", hoy, hora),
        "company": {
            "ruc": emisor.ruc.trim(),
            "razonSocial": emisor.razon_social.trim(),
            "nombreComercial": nombre_comercial,
            "address": {
                "ubigueo": emisor.ubigeo.trim(),
                "codigoPais": "PE",
                "departamento": emisor.departamento.trim(),
                "provincia": emisor.provincia.trim(),
                "distrito": emisor.distrito.trim(),
                "urbanizacion": "-",
                "direccion": emisor.direccion.trim(),
                "codLocal": "0000",
            },
        },
        "destinatario": {
            "tipoDoc": codigo_tipo_documento_identidad(&datos.destinatario_tipo),
            "numDoc": datos.destinatario_documento,
            "rznSocial": datos.destinatario_nombre,
        },
        "envio": envio,
        "details": detalles,
    });
    if let Some(o) = datos.observaciones.as_deref().filter(|o| !o.is_empty()) {
        doc["observacion"] = json!(o);
    }
    if let Some((serie_doc, numero_doc, tipo)) = documento {
        let factura = tipo == "FACTURA";
        doc["addDocs"] = json!([{
            "tipoDesc": if factura { "Factura" } else { "Boleta de Venta" },
            "tipo": if factura { "01" } else { "03" },
            "nro": format!("{}-{}", serie_doc, numero_doc),
            "emisor": emisor.ruc.trim(),
        }]);
    }
    doc
}

/// La URL base de la cuenta ("https://x.pro.facturalibre.org") a partir de
/// la URL de comprobantes que el negocio ya tiene configurada
/// (".../api/documents").
pub fn url_base(ruta_documentos: &str) -> String {
    let r = ruta_documentos.trim().trim_end_matches('/');
    r.strip_suffix("/api/documents").unwrap_or(r).trim_end_matches('/').to_string()
}

/// Estado de la guía según la respuesta de consulta de ticket
/// (data.state_type_id: "05" aceptada, "09" rechazada; otro = en proceso).
pub fn estado_de_ticket(state_type_id: Option<&str>) -> &'static str {
    match state_type_id {
        Some("05") => "ACEPTADA",
        Some("09") => "RECHAZADA",
        _ => "ENVIADA",
    }
}

/// El código QR de la guía, si viene en la respuesta de FacturaLibre.
///
/// En la guía electrónica el QR lo genera SUNAT al aceptarla (no se puede
/// armar aquí como el de una boleta). No está documentado en qué campo lo
/// entrega FacturaLibre, así que se busca en toda la respuesta:
/// - el enlace que va dentro del QR (campo "qr_url", o cualquier enlace de
///   SUNAT a su QR, "...sunat.gob.pe/...qr..."): se devuelve tal cual, y con
///   él se dibuja el código;
/// - o la imagen del QR ya hecha (campo cuyo nombre contiene "qr", en
///   base64): se devuelve como "data:image/png;base64,...".
/// None si no viene ninguno: el ticket se imprime sin QR.
pub fn qr_de_respuesta(r: &Value) -> Option<String> {
    fn enlace(nombre: &str, v: &Value) -> Option<String> {
        match v {
            Value::String(t) => {
                let m = t.trim().to_lowercase();
                let es_qr = nombre == "qr_url" || (m.contains("sunat.gob.pe") && m.contains("qr"));
                (m.starts_with("http") && es_qr && t.len() < 600).then(|| t.trim().to_string())
            }
            Value::Array(lista) => lista.iter().find_map(|v| enlace(nombre, v)),
            Value::Object(campos) => campos.iter().find_map(|(nombre, v)| enlace(nombre, v)),
            _ => None,
        }
    }
    fn imagen(v: &Value) -> Option<String> {
        match v {
            Value::Array(lista) => lista.iter().find_map(imagen),
            Value::Object(campos) => campos.iter().find_map(|(nombre, valor)| match valor {
                Value::String(t) if nombre.to_lowercase().contains("qr") => {
                    let t = t.trim();
                    if t.len() > 20_000 {
                        None
                    } else if t.starts_with("data:image/") {
                        Some(t.to_string())
                    } else if t.len() > 200 && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')) {
                        Some(format!("data:image/png;base64,{}", t))
                    } else {
                        None
                    }
                }
                otro => imagen(otro),
            }),
            _ => None,
        }
    }
    enlace("", r).or_else(|| imagen(r))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn qr_de_la_respuesta() {
        // Enlace de SUNAT, en cualquier campo y a cualquier profundidad.
        let sunat = "https://e-factura.sunat.gob.pe/v1/contribuyente/gre/comprobantes/descargaqr?hashqr=abc123";
        let r = serde_json::json!({"success": true, "data": {"state_type_id": "05", "otro": {"qr_url": sunat}}, "links": {"pdf": "https://x/pdf"}});
        assert_eq!(qr_de_respuesta(&r).as_deref(), Some(sunat));
        let r = serde_json::json!({"success": true, "data": {"qr_url": "https://otro.pe/consulta/abc", "state_type_id": "05"}});
        assert_eq!(qr_de_respuesta(&r).as_deref(), Some("https://otro.pe/consulta/abc"));
        // La imagen ya hecha, en base64.
        let b64 = "iVBORw0KGgo".repeat(30);
        let r = serde_json::json!({"success": true, "data": {"qr": b64}});
        assert_eq!(qr_de_respuesta(&r), Some(format!("data:image/png;base64,{}", b64)));
        let r = serde_json::json!({"data": {"qr": format!("data:image/png;base64,{}", b64)}});
        assert_eq!(qr_de_respuesta(&r), Some(format!("data:image/png;base64,{}", b64)));
        // Nada que sirva: enlaces que no son de SUNAT, textos cortos, otros campos.
        let r = serde_json::json!({"success": true, "data": {"state_type_id": "05", "qr": "", "qr_url": null, "hash": b64},
                                   "links": {"pdf": "https://x.facturalibre.org/downloads/qr/1", "cdr": "https://x/cdr"}, "message": "ACEPTADA"});
        assert_eq!(qr_de_respuesta(&r), None);
        assert_eq!(qr_de_respuesta(&Value::Null), None);
    }

    fn base() -> DatosGuia {
        DatosGuia {
            venta_id: Some(7),
            destinatario_tipo: "RUC".into(),
            destinatario_documento: "20123456789".into(),
            destinatario_nombre: "  Constructora   Andina SAC ".into(),
            motivo: "01".into(),
            motivo_descripcion: None,
            modo: "PRIVADO".into(),
            fecha_traslado: "2026-10-04".into(),
            peso_total: 850.5,
            bultos: 3,
            partida: Direccion { ubigeo: "080105".into(), direccion: "Av. La Cultura 1200".into() },
            llegada: Direccion { ubigeo: "080101".into(), direccion: "Calle Saphi 300".into() },
            transportista: None,
            chofer: Some(Chofer { documento: "41784439".into(), nombres: "juan".into(), apellidos: "perez".into(), licencia: "q-41784439".into() }),
            placa: Some("a1y-298".into()),
            observaciones: None,
            items: vec![ItemGuia { codigo: "M1".into(), descripcion: "Tornillo (5 pzas de 2\" x 4\" x 10 pies)".into(), cantidad: 33.33, unidad: "PIE_TABLAR".into() }],
        }
    }
    fn emisor() -> Emisor {
        Emisor { ubigeo: "080105".into(), direccion: "Av. La Cultura 1200".into(), correo: Some("ventas@maderera.pe".into()), telefono: Some("984000000".into()) }
    }

    #[test]
    fn privado_valida_normaliza_y_arma() {
        let d = validar(&base(), "2026-10-03").unwrap();
        assert_eq!(d.destinatario_nombre, "Constructora Andina SAC");
        assert_eq!(d.placa.as_deref(), Some("A1Y298"));
        assert_eq!(d.chofer.as_ref().unwrap().licencia, "Q41784439");
        let doc = armar_payload(&d, &emisor(), "T001", "2026-10-03", "10:00:00", Some(&("F001".into(), 190, "FACTURA".into())));
        assert_eq!(doc["codigo_tipo_documento"], "09");
        assert_eq!(doc["numero_documento"], "#");
        assert_eq!(doc["codigo_modo_transporte"], "02");
        assert_eq!(doc["codigo_motivo_traslado"], "01");
        assert_eq!(doc["descripcion_motivo_traslado"], "Venta");
        assert_eq!(doc["unidad_peso_total"], "KGM");
        assert_eq!(doc["peso_total"], 850.5);
        assert_eq!(doc["numero_de_placa"], "A1Y298");
        assert_eq!(doc["vehiculo"]["numero_de_placa"], "A1Y298");
        assert_eq!(doc["chofer"]["numero_licencia"], "Q41784439");
        assert_eq!(doc["chofer"]["nombres"], "JUAN");
        assert!(doc.get("transportista").is_none());
        assert_eq!(doc["datos_del_cliente_o_receptor"]["codigo_tipo_documento_identidad"], "6");
        assert_eq!(doc["datos_del_cliente_o_receptor"]["ubigeo"], "080101");
        assert_eq!(doc["direccion_partida"]["ubigeo"], "080105");
        assert_eq!(doc["direccion_llegada"]["codigo_del_domicilio_fiscal"], "0000");
        assert_eq!(doc["datos_del_emisor"]["correo_electronico"], "ventas@maderera.pe");
        assert_eq!(doc["items"][0]["unidad_de_medida"], "NIU");
        assert_eq!(doc["items"][0]["cantidad"], 33.33);
        assert_eq!(doc["documento_afectado"]["numero_documento"], "190");
        assert_eq!(doc["documento_afectado"]["codigo_tipo_documento"], "01");
    }

    #[test]
    fn publico_usa_transportista() {
        let mut g = base();
        g.modo = "PUBLICO".into();
        g.transportista = Some(Transportista { ruc: "20100686814".into(), nombre: "Olva Courier SAC".into(), mtc: Some(" 1518996cng ".into()) });
        let d = validar(&g, "2026-10-03").unwrap();
        assert!(d.chofer.is_none() && d.placa.is_none());
        let doc = armar_payload(&d, &emisor(), "T001", "2026-10-03", "10:00:00", None);
        assert_eq!(doc["codigo_modo_transporte"], "01");
        assert_eq!(doc["transportista"]["numero_documento"], "20100686814");
        assert_eq!(doc["transportista"]["numero_mtc"], "1518996CNG");
        assert_eq!(doc["numero_de_placa"], "");
        assert!(doc.get("chofer").is_none() && doc.get("vehiculo").is_none() && doc.get("documento_afectado").is_none());
    }

    #[test]
    fn rechaza_lo_incompleto() {
        let hoy = "2026-10-03";
        let caso = |f: &dyn Fn(&mut DatosGuia)| {
            let mut g = base();
            f(&mut g);
            validar(&g, hoy).unwrap_err()
        };
        assert!(caso(&|g| g.destinatario_documento = "2012".into()).contains("RUC"));
        assert!(caso(&|g| g.destinatario_tipo = "SIN_DOCUMENTO".into()).contains("DNI o RUC"));
        assert!(caso(&|g| g.fecha_traslado = "2026-10-02".into()).contains("anterior"));
        assert!(caso(&|g| g.peso_total = 0.0).contains("peso"));
        assert!(caso(&|g| g.llegada.ubigeo = "".into()).contains("llegada"));
        assert!(caso(&|g| g.placa = None).contains("placa"));
        assert!(caso(&|g| g.chofer.as_mut().unwrap().licencia = "123".into()).contains("licencia"));
        assert!(caso(&|g| g.chofer = None).contains("conductor"));
        assert!(caso(&|g| { g.modo = "PUBLICO".into(); }).contains("empresa de transporte"));
        assert!(caso(&|g| g.motivo = "13".into()).contains("Describe"));
        assert!(caso(&|g| g.items.clear()).contains("productos"));
    }

    #[test]
    fn url_y_estado() {
        assert_eq!(url_base("https://demo1.pro.facturalibre.org/api/documents"), "https://demo1.pro.facturalibre.org");
        assert_eq!(url_base("https://demo1.pro.facturalibre.org/api/documents/"), "https://demo1.pro.facturalibre.org");
        assert_eq!(url_base("https://x.proapi.facturalibre.org"), "https://x.proapi.facturalibre.org");
        assert_eq!((estado_de_ticket(Some("05")), estado_de_ticket(Some("09")), estado_de_ticket(Some("01")), estado_de_ticket(None)), ("ACEPTADA", "RECHAZADA", "ENVIADA", "ENVIADA"));
    }

    #[test]
    fn guia_directa_greenter() {
        let d = validar(&base(), "2026-10-04").unwrap();
        let emisor = crate::logica::sunat_directo::Emisor {
            ruc: "20161515648".into(),
            razon_social: "EMPRESA SAC".into(),
            direccion: "AV 1".into(),
            ubigeo: "080101".into(),
            ..Default::default()
        };
        let documento = ("FM01".to_string(), 12, "FACTURA".to_string());
        let g = armar_guia_sunat(&d, &emisor, "T001", 5, "2026-10-04", "09:30:00", Some(&documento));
        assert_eq!(g["version"], "2022");
        assert_eq!(g["tipoDoc"], "09");
        assert_eq!(g["correlativo"], "5");
        assert_eq!(g["destinatario"]["tipoDoc"], "6");
        assert_eq!(g["envio"]["modTraslado"], "02");
        assert_eq!(g["envio"]["vehiculo"]["placa"], "A1Y298");
        assert_eq!(g["envio"]["choferes"][0]["licencia"], "Q41784439");
        assert_eq!(g["envio"]["partida"]["ubigueo"], "080105");
        assert_eq!(g["envio"]["fecTraslado"], "2026-10-04T00:00:00-05:00");
        assert_eq!(g["addDocs"][0]["nro"], "FM01-12");
        assert_eq!(g["addDocs"][0]["tipo"], "01");
        assert_eq!(g["details"][0]["codigo"], "M1");
        assert!(g["envio"].get("transportista").is_none());

        let mut publico = base();
        publico.modo = "PUBLICO".into();
        publico.transportista = Some(Transportista { ruc: "20600000001".into(), nombre: "TRANSPORTES SAC".into(), mtc: Some("abc123".into()) });
        let d = validar(&publico, "2026-10-04").unwrap();
        let g = armar_guia_sunat(&d, &emisor, "T001", 6, "2026-10-04", "09:30:00", None);
        assert_eq!(g["envio"]["modTraslado"], "01");
        assert_eq!(g["envio"]["transportista"]["nroMtc"], "ABC123");
        assert!(g["envio"].get("choferes").is_none());
        assert!(g.get("addDocs").is_none());
    }
}
