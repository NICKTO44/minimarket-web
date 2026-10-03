//! Guía de remisión remitente electrónica: validación del formulario y
//! armado del documento para FacturaLibre (formato FacturadorPro,
//! POST /api/dispatches). Aquí no hay red ni base de datos: eso está en
//! handlers/guias.rs. Los nombres de campo salen de los ejemplos "Guia
//! Remisión Transporte Público / Privado" de la documentación oficial
//! (https://documenter.getpostman.com/view/6435177/TVRrUPuD).

use serde::{Deserialize, Serialize};
use serde_json::json;

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

#[cfg(test)]
mod pruebas {
    use super::*;

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
}
