//! Emisión directa a SUNAT, sin proveedor intermedio.
//!
//! El documento se arma aquí en el formato de Greenter y se manda a Lycet
//! (una API REST de código abierto sobre Greenter que corre en nuestro
//! propio servidor). Lycet genera el XML UBL 2.1, lo firma con el
//! certificado del negocio, lo envía a SUNAT y devuelve el XML firmado, el
//! hash y la constancia (CDR). Cada negocio queda registrado en Lycet con su
//! RUC, su certificado y su usuario secundario SOL (binario `sunat`).
//!
//! Aquí no hay red ni base de datos: eso está en
//! handlers/facturacion_directa.rs. Así se puede probar sin emitir nada.

use serde_json::{json, Value};

use crate::logica::facturacion::{codigo_tipo_documento_identidad, unidad_sunat, DatosParaEmitir};
use crate::logica::igv::{valor_sin_igv, Afectacion};

/// Datos del negocio que van en el comprobante como emisor.
#[derive(Debug, Clone, Default)]
pub struct Emisor {
    pub ruc: String,
    pub razon_social: String,
    pub nombre_comercial: String,
    pub direccion: String,
    /// Ubigeo INEI de 6 dígitos del domicilio fiscal.
    pub ubigeo: String,
    pub departamento: String,
    pub provincia: String,
    pub distrito: String,
}

impl Emisor {
    /// Qué dato falta para poder emitir (None = está completo).
    pub fn dato_faltante(&self) -> Option<&'static str> {
        let vacio = |s: &str| s.trim().is_empty();
        if self.ruc.trim().len() != 11 {
            return Some("el RUC del negocio");
        }
        if vacio(&self.razon_social) {
            return Some("la razón social");
        }
        if vacio(&self.direccion) {
            return Some("la dirección fiscal");
        }
        if self.ubigeo.trim().len() != 6 {
            return Some("el ubigeo de la dirección fiscal");
        }
        None
    }
}

/// Valor que SUNAT usa en el campo del tipo de documento del adquirente
/// cuando la boleta no lleva cliente identificado (catálogo 06:
/// "VARIOS - VENTAS MENORES A S/.700.00 Y OTROS").
pub const SIN_DOCUMENTO: &str = "-";

/// Código del tipo de comprobante (catálogo 01).
pub fn codigo_tipo_comprobante(tipo: &str) -> &'static str {
    if tipo == "FACTURA" { "01" } else { "03" }
}

/// Tipo y número de documento del adquirente tal como van en el XML (y,
/// por lo mismo, en el QR del comprobante impreso).
pub fn documento_adquirente(datos: &DatosParaEmitir) -> (String, String) {
    let numero = datos.cliente_documento.as_deref().map(str::trim).unwrap_or_default();
    if datos.tipo == "FACTURA" {
        return ("6".to_string(), numero.to_string());
    }
    if numero.is_empty() {
        return (SIN_DOCUMENTO.to_string(), SIN_DOCUMENTO.to_string());
    }
    let tipo = datos.cliente_tipo_documento.as_deref().unwrap_or("DNI");
    (codigo_tipo_documento_identidad(tipo).to_string(), numero.to_string())
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Precio o valor unitario: SUNAT acepta hasta 10 decimales; con 6 no se
/// pierde nada y el XML queda legible.
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

/// Arma el comprobante (boleta o factura) en el formato de Greenter.
///
/// `fecha` es "AAAA-MM-DD" y `hora` "HH:MM:SS", ambas de Perú.
pub fn armar_documento(
    datos: &DatosParaEmitir,
    emisor: &Emisor,
    serie: &str,
    numero: i64,
    fecha: &str,
    hora: &str,
    codigo_producto_sunat: &str,
) -> Value {
    let es_factura = datos.tipo == "FACTURA";

    let detalles: Vec<Value> = datos
        .items
        .iter()
        .enumerate()
        .map(|(idx, it)| {
            let gravado = it.afectacion == Afectacion::Gravado;
            let valor_unitario = valor_sin_igv(it.precio_unitario, it.afectacion, datos.tasa);
            let valor_venta = round2(it.cantidad * valor_unitario);
            let igv = if gravado { round2(it.cantidad * it.precio_unitario - valor_venta) } else { 0.0 };
            json!({
                "unidad": unidad_sunat(&it.unidad_medida),
                "cantidad": it.cantidad,
                "codProducto": format!("P{:03}", idx + 1),
                "codProdSunat": codigo_producto_sunat,
                "descripcion": it.descripcion,
                "mtoValorUnitario": round6(valor_unitario),
                "mtoValorVenta": valor_venta,
                "mtoBaseIgv": valor_venta,
                "porcentajeIgv": if gravado { datos.tasa } else { 0.0 },
                "igv": igv,
                "tipAfeIgv": it.afectacion.codigo_sunat(),
                "totalImpuestos": igv,
                "mtoPrecioUnitario": round6(it.precio_unitario),
            })
        })
        .collect();

    let (tipo_doc_cliente, num_doc_cliente) = documento_adquirente(datos);
    let nombre_cliente = datos
        .cliente_nombre
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(if es_factura { "-" } else { "CLIENTES VARIOS" })
        .to_string();
    let mut cliente = json!({
        "tipoDoc": tipo_doc_cliente,
        "numDoc": num_doc_cliente,
        "rznSocial": nombre_cliente,
    });
    if let Some(dir) = datos.cliente_direccion.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        cliente["address"] = json!({ "codigoPais": "PE", "direccion": dir });
    }

    let valor_venta = round2(datos.subtotal + datos.exoneradas + datos.inafectas);
    let total = round2(datos.total);

    let mut documento = json!({
        "ublVersion": "2.1",
        "tipoOperacion": "0101",
        "tipoDoc": codigo_tipo_comprobante(&datos.tipo),
        "serie": serie,
        "correlativo": numero.to_string(),
        "fechaEmision": format!("{}T{}-05:00", fecha, hora),
        "tipoMoneda": "PEN",
        "company": {
            "ruc": emisor.ruc.trim(),
            "razonSocial": emisor.razon_social.trim(),
            "nombreComercial": if emisor.nombre_comercial.trim().is_empty() { emisor.razon_social.trim() } else { emisor.nombre_comercial.trim() },
            "address": {
                "ubigueo": emisor.ubigeo.trim(),
                "codigoPais": "PE",
                "departamento": emisor.departamento.trim(),
                "provincia": emisor.provincia.trim(),
                "distrito": emisor.distrito.trim(),
                "urbanizacion": "-",
                "direccion": emisor.direccion.trim(),
                // Catálogo de establecimientos de SUNAT: 0000 = domicilio fiscal.
                "codLocal": "0000",
            },
        },
        "client": cliente,
        "mtoOperGravadas": round2(datos.subtotal),
        "mtoOperExoneradas": round2(datos.exoneradas),
        "mtoOperInafectas": round2(datos.inafectas),
        "mtoIGV": round2(datos.igv),
        "totalImpuestos": round2(datos.igv),
        "valorVenta": valor_venta,
        "subTotal": total,
        "mtoImpVenta": total,
        "details": detalles,
        "legends": [{ "code": "1000", "value": monto_en_letras(total) }],
    });

    if es_factura {
        documento["formaPago"] = json!({ "moneda": "PEN", "tipo": "Contado" });
    }

    // Operación sujeta a detracción (SPOT): tipo de operación 1001, leyenda
    // 2006 y el depósito en la cuenta del Banco de la Nación.
    if let Some(d) = &datos.detraccion {
        documento["tipoOperacion"] = json!("1001");
        documento["detraccion"] = json!({
            "codBienDetraccion": d.codigo,
            // Catálogo 59: 001 = depósito en cuenta.
            "codMedioPago": "001",
            "ctaBanco": d.cuenta,
            "percent": d.porcentaje,
            "mount": round2(d.monto),
        });
        documento["legends"]
            .as_array_mut()
            .expect("legends es un arreglo")
            .push(json!({ "code": "2006", "value": "Operación sujeta a detracción" }));
    }

    // Factura de una venta al crédito: una cuota por lo que queda por
    // pagar (menos la detracción, que el cliente deposita aparte). Si ya no
    // queda saldo o la fecha ya pasó, sale al contado.
    if es_factura {
        if let Some((saldo, vence)) = &datos.credito {
            let detraccion = datos.detraccion.as_ref().map(|d| d.monto).unwrap_or(0.0);
            let cuota = round2(saldo - detraccion);
            if cuota > 0.0 && vence.as_str() > fecha {
                let vence_iso = format!("{}T00:00:00-05:00", vence);
                documento["formaPago"] = json!({ "moneda": "PEN", "tipo": "Credito", "monto": cuota });
                documento["cuotas"] = json!([{ "moneda": "PEN", "monto": cuota, "fechaPago": vence_iso }]);
                documento["fecVencimiento"] = json!(vence_iso);
            }
        }
    }

    documento
}

// ===== Monto en letras (leyenda 1000) =====

const UNIDADES: [&str; 20] = [
    "", "UNO", "DOS", "TRES", "CUATRO", "CINCO", "SEIS", "SIETE", "OCHO", "NUEVE", "DIEZ", "ONCE", "DOCE",
    "TRECE", "CATORCE", "QUINCE", "DIECISÉIS", "DIECISIETE", "DIECIOCHO", "DIECINUEVE",
];
const VEINTES: [&str; 10] = [
    "VEINTE", "VEINTIUNO", "VEINTIDÓS", "VEINTITRÉS", "VEINTICUATRO", "VEINTICINCO", "VEINTISÉIS",
    "VEINTISIETE", "VEINTIOCHO", "VEINTINUEVE",
];
const DECENAS: [&str; 10] =
    ["", "", "VEINTE", "TREINTA", "CUARENTA", "CINCUENTA", "SESENTA", "SETENTA", "OCHENTA", "NOVENTA"];
const CENTENAS: [&str; 10] = [
    "", "CIENTO", "DOSCIENTOS", "TRESCIENTOS", "CUATROCIENTOS", "QUINIENTOS", "SEISCIENTOS", "SETECIENTOS",
    "OCHOCIENTOS", "NOVECIENTOS",
];

/// 1..=999 en letras.
fn grupo(n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    if n == 100 {
        return "CIEN".to_string();
    }
    let mut partes = Vec::new();
    let centena = (n / 100) as usize;
    let resto = (n % 100) as usize;
    if centena > 0 {
        partes.push(CENTENAS[centena].to_string());
    }
    if resto > 0 {
        if resto < 20 {
            partes.push(UNIDADES[resto].to_string());
        } else if resto < 30 {
            partes.push(VEINTES[resto - 20].to_string());
        } else {
            let unidad = resto % 10;
            if unidad > 0 {
                partes.push(format!("{} Y {}", DECENAS[resto / 10], UNIDADES[unidad]));
            } else {
                partes.push(DECENAS[resto / 10].to_string());
            }
        }
    }
    partes.join(" ")
}

/// Delante de "MIL" o "MILLONES" se dice "UN" y "VEINTIÚN", no "UNO".
fn apocopar(texto: String) -> String {
    if let Some(base) = texto.strip_suffix("VEINTIUNO") {
        format!("{}VEINTIÚN", base)
    } else if let Some(base) = texto.strip_suffix("UNO") {
        format!("{}UN", base)
    } else {
        texto
    }
}

fn entero_en_letras(n: u64) -> String {
    if n == 0 {
        return "CERO".to_string();
    }
    let millones = n / 1_000_000;
    let miles = (n % 1_000_000) / 1000;
    let resto = n % 1000;
    let mut partes = Vec::new();
    if millones > 0 {
        partes.push(if millones == 1 { "UN MILLÓN".to_string() } else { format!("{} MILLONES", apocopar(grupo(millones))) });
    }
    if miles > 0 {
        partes.push(if miles == 1 { "MIL".to_string() } else { format!("{} MIL", apocopar(grupo(miles))) });
    }
    if resto > 0 {
        partes.push(grupo(resto));
    }
    partes.join(" ")
}

/// "CIENTO CINCUENTA Y TRES CON 40/100 SOLES" — mismo formato que el
/// ticket (frontend/src/utils/numeroALetras.js).
pub fn monto_en_letras(monto: f64) -> String {
    let centimos_totales = (monto.abs() * 100.0).round() as u64;
    format!("{} CON {:02}/100 SOLES", entero_en_letras(centimos_totales / 100), centimos_totales % 100)
}

// ===== Respuesta de Lycet / SUNAT =====

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EstadoEnvio {
    /// SUNAT lo aceptó (con o sin observaciones).
    Aceptado,
    /// SUNAT lo rechazó: el comprobante no existe para SUNAT.
    Rechazado,
    /// No hubo respuesta definitiva (sin conexión, SUNAT caído, error
    /// temporal): se reintenta con el mismo número.
    Pendiente,
    /// Ni siquiera se envió: faltan o están mal datos del documento.
    Error,
}

impl EstadoEnvio {
    /// Valor de la columna comprobantes_electronicos.estado.
    pub fn como_texto(self) -> &'static str {
        match self {
            EstadoEnvio::Aceptado => "ACEPTADO",
            EstadoEnvio::Rechazado => "RECHAZADO",
            EstadoEnvio::Pendiente => "PENDIENTE",
            EstadoEnvio::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RespuestaSunat {
    pub estado: EstadoEnvio,
    pub mensaje: String,
    pub hash: Option<String>,
    /// XML firmado tal como se envió.
    pub xml: Option<String>,
    /// Constancia de recepción (ZIP) en base64, tal como la entrega SUNAT.
    pub cdr_zip: Option<String>,
    /// Código de SUNAT de la respuesta (0, 1033, 2017, 4252...), si lo hubo.
    pub codigo: Option<i64>,
    /// Referencia de la constancia (en las guías, el enlace que va en su QR).
    pub referencia: Option<String>,
    /// El envío se cortó sin una respuesta clara (sin conexión, error del
    /// servicio): el documento pudo haber llegado a SUNAT.
    pub incierto: bool,
}

impl RespuestaSunat {
    /// Respuesta sin nada definitivo: queda PENDIENTE con este mensaje.
    pub fn pendiente(mensaje: String) -> Self {
        RespuestaSunat {
            estado: EstadoEnvio::Pendiente,
            mensaje,
            hash: None,
            xml: None,
            cdr_zip: None,
            codigo: None,
            referencia: None,
            incierto: false,
        }
    }

    /// Sin respuesta clara: queda PENDIENTE y el envío cuenta como incierto.
    pub fn incierta(mensaje: String) -> Self {
        RespuestaSunat { incierto: true, ..RespuestaSunat::pendiente(mensaje) }
    }
}

/// SUNAT ya tenía registrado un comprobante con ese tipo, serie y número.
/// Solo si un envío anterior de ESTE documento se cortó sin respuesta
/// (envío incierto) significa que llegó y el comprobante es este; si no,
/// SUNAT tiene otro documento con ese número (ver envios_sunat).
pub const CODIGO_YA_REGISTRADO: i64 = 1033;

/// Excepciones de SUNAT por el usuario SOL (clave incorrecta, usuario de
/// baja o suspendido, sin perfil para enviar): reintentar no las arregla.
pub const CODIGOS_CREDENCIALES: [i64; 4] = [102, 103, 104, 111];

/// Código de respuesta de SUNAT como número ("0", "2017", "4252"...).
fn codigo_num(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::String(s) => s.trim().parse().ok(),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

/// Interpreta la respuesta HTTP de Lycet al enviar un comprobante.
///
/// Códigos de SUNAT: 0 = aceptado; 4000 o más = aceptado con
/// observaciones; 2000–3999 = rechazado; 0100–1999 = excepción (no se
/// procesó, se puede volver a enviar).
pub fn leer_respuesta(status: u16, texto: &str) -> RespuestaSunat {
    let mut r = RespuestaSunat::pendiente(String::new());

    let valor: Value = match serde_json::from_str(texto) {
        Ok(v) => v,
        Err(_) => {
            r.mensaje = format!("El servicio de emisión respondió algo inesperado (HTTP {}). Se reintentará.", status);
            r.incierto = true;
            return r;
        }
    };

    // Lycet valida el documento antes de enviarlo: 400 con una lista de
    // {field, message}. No llegó a SUNAT.
    if status == 400 {
        let detalle = valor
            .as_array()
            .map(|errores| {
                errores
                    .iter()
                    .filter_map(|e| {
                        let campo = e.get("field").and_then(Value::as_str).unwrap_or("");
                        let mensaje = e.get("message").and_then(Value::as_str)?;
                        Some(if campo.is_empty() { mensaje.to_string() } else { format!("{}: {}", campo, mensaje) })
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| texto.chars().take(300).collect());
        r.estado = EstadoEnvio::Error;
        r.mensaje = format!("El comprobante tiene datos inválidos y no se envió a SUNAT: {}", detalle);
        return r;
    }

    if !(200..300).contains(&status) {
        r.mensaje = format!("El servicio de emisión no respondió bien (HTTP {}). Se reintentará.", status);
        r.incierto = true;
        return r;
    }

    let mut r = leer_resultado(valor.get("sunatResponse").unwrap_or(&Value::Null));
    r.hash = valor.get("hash").and_then(Value::as_str).map(str::to_string);
    r.xml = valor.get("xml").and_then(Value::as_str).map(str::to_string);
    r
}

/// Interpreta un resultado de Greenter tal como lo serializa Lycet: el
/// `sunatResponse` de un envío o la respuesta de una consulta de ticket
/// ({success, cdrZip, cdrResponse: {code, description, notes}, error:
/// {code, message}}).
fn leer_resultado(sunat: &Value) -> RespuestaSunat {
    let mut r = RespuestaSunat::pendiente(String::new());
    r.cdr_zip = sunat.get("cdrZip").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);

    if let Some(cdr) = sunat.get("cdrResponse").filter(|c| !c.is_null()) {
        let codigo = codigo_num(cdr.get("code")).unwrap_or(-1);
        let descripcion = cdr.get("description").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let notas: Vec<String> = cdr
            .get("notes")
            .and_then(Value::as_array)
            .map(|n| n.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        r.codigo = Some(codigo);
        r.referencia = cdr.get("reference").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
        r.estado = if codigo == 0 || codigo >= 4000 {
            EstadoEnvio::Aceptado
        } else if (2000..4000).contains(&codigo) {
            EstadoEnvio::Rechazado
        } else {
            EstadoEnvio::Pendiente
        };
        r.mensaje = if notas.is_empty() { descripcion } else { format!("{} (Observaciones: {})", descripcion, notas.join("; ")) };
        if r.mensaje.is_empty() {
            r.mensaje = format!("Respuesta de SUNAT con código {}", codigo);
        }
        return r;
    }

    if let Some(error) = sunat.get("error").filter(|e| !e.is_null()) {
        let codigo = codigo_num(error.get("code"));
        let mensaje = error.get("message").and_then(Value::as_str).unwrap_or("").trim();
        r.codigo = codigo;
        r.estado = match codigo {
            Some(c) if (2000..4000).contains(&c) => EstadoEnvio::Rechazado,
            // Nunca se resuelven solos: reintentar no sirve.
            Some(CODIGO_YA_REGISTRADO) => EstadoEnvio::Error,
            Some(c) if CODIGOS_CREDENCIALES.contains(&c) => EstadoEnvio::Error,
            _ => EstadoEnvio::Pendiente,
        };
        r.mensaje = match codigo {
            Some(CODIGO_YA_REGISTRADO) => format!(
                "SUNAT ya tiene otro documento con esta serie y número (1033: {}). Si esta serie se usó antes en otro sistema, cambia la serie en el panel.",
                mensaje
            ),
            Some(c) if CODIGOS_CREDENCIALES.contains(&c) => format!(
                "SUNAT no aceptó el usuario secundario SOL ({}: {}). Revisa su clave y permisos y vuelve a registrar el negocio en el panel.",
                c, mensaje
            ),
            Some(c) => format!("SUNAT respondió {}: {}", c, mensaje),
            None => format!("SUNAT no respondió: {}", mensaje),
        };
        if r.estado == EstadoEnvio::Pendiente {
            r.mensaje.push_str(". Se reintentará con el mismo número.");
        }
        return r;
    }

    r.mensaje = "SUNAT no devolvió constancia. Se reintentará con el mismo número.".to_string();
    r
}

/// Respuesta de los envíos que SUNAT procesa después (resumen diario,
/// comunicación de baja, guía de remisión): primero entrega un ticket y la
/// constancia se pide más tarde con ese ticket.
#[derive(Debug, Clone)]
pub struct RespuestaTicket {
    /// Ticket de SUNAT si lo recibió.
    pub ticket: Option<String>,
    /// Sin ticket: por qué (Error = datos inválidos, Rechazado = SUNAT no lo
    /// recibió por un error del documento, Pendiente = se vuelve a enviar).
    pub respuesta: RespuestaSunat,
}

/// Interpreta la respuesta de Lycet al enviar un documento con ticket.
pub fn leer_envio_con_ticket(status: u16, texto: &str) -> RespuestaTicket {
    let mut respuesta = leer_respuesta(status, texto);
    let ticket = serde_json::from_str::<Value>(texto)
        .ok()
        .filter(|_| (200..300).contains(&status))
        .and_then(|v| {
            let s = v.get("sunatResponse")?;
            match s.get("ticket")? {
                Value::String(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            }
        });
    if ticket.is_some() {
        respuesta.estado = EstadoEnvio::Pendiente;
        respuesta.mensaje = "Recibido por SUNAT; se está procesando.".to_string();
    }
    RespuestaTicket { ticket, respuesta }
}

/// Interpreta la consulta de un ticket. Mientras SUNAT lo procesa (código
/// 98) queda PENDIENTE; con la constancia queda aceptado o rechazado.
pub fn leer_estado_ticket(status: u16, texto: &str) -> RespuestaSunat {
    if !(200..300).contains(&status) {
        return RespuestaSunat::pendiente(format!("No se pudo consultar el ticket (HTTP {}). Se volverá a consultar.", status));
    }
    match serde_json::from_str::<Value>(texto) {
        Ok(v) => {
            let mut r = leer_resultado(&v);
            if r.estado == EstadoEnvio::Pendiente && codigo_num(v.get("code")) == Some(98) {
                r.mensaje = "SUNAT todavía lo está procesando. Se volverá a consultar.".to_string();
            }
            r
        }
        Err(_) => RespuestaSunat::pendiente("Respuesta inesperada al consultar el ticket. Se volverá a consultar.".to_string()),
    }
}

/// Resultado de preguntarle a SUNAT por un comprobante (servicio de
/// consulta de CDR; solo facturas y sus notas, serie que empieza con F).
#[derive(Debug, Clone)]
pub enum ConsultaCdr {
    /// SUNAT lo tiene: aceptado o rechazado, con su constancia si la dio.
    Encontrado(RespuestaSunat),
    /// SUNAT no lo tiene (código 0011): hay que enviarlo.
    NoExiste,
    /// No se pudo saber (sin conexión, credenciales, servicio caído).
    NoSePudo(String),
}

/// Interpreta la respuesta de Lycet a `GET /api/v1/invoice/status`.
///
/// Códigos del servicio de consulta: 0001 existe y está aceptado; 0002
/// existe pero rechazado; 0003 existe pero de baja; 0011 no existe; el
/// resto son errores de la consulta misma.
pub fn leer_consulta_cdr(status: u16, texto: &str) -> ConsultaCdr {
    if !(200..300).contains(&status) {
        let detalle = serde_json::from_str::<Value>(texto)
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {}", status));
        return ConsultaCdr::NoSePudo(detalle);
    }
    let v: Value = match serde_json::from_str(texto) {
        Ok(v) => v,
        Err(_) => return ConsultaCdr::NoSePudo("respuesta inesperada".to_string()),
    };
    if v.get("cdrResponse").is_some_and(|c| !c.is_null()) {
        return ConsultaCdr::Encontrado(leer_resultado(&v));
    }
    let codigo = v.get("code").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let mensaje = v.get("message").and_then(Value::as_str).unwrap_or("").trim().to_string();
    match codigo.as_str() {
        "0011" => ConsultaCdr::NoExiste,
        "0001" | "0003" => ConsultaCdr::Encontrado(RespuestaSunat {
            estado: EstadoEnvio::Aceptado,
            mensaje: if mensaje.is_empty() { "SUNAT ya lo tenía registrado.".to_string() } else { mensaje },
            hash: None,
            xml: None,
            cdr_zip: None,
            codigo: Some(0),
            referencia: None,
            incierto: false,
        }),
        "0002" => ConsultaCdr::Encontrado(RespuestaSunat {
            estado: EstadoEnvio::Rechazado,
            mensaje: if mensaje.is_empty() { "SUNAT lo tiene como rechazado.".to_string() } else { mensaje },
            hash: None,
            xml: None,
            cdr_zip: None,
            codigo: None,
            referencia: None,
            incierto: false,
        }),
        _ => {
            let error = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string);
            ConsultaCdr::NoSePudo(error.unwrap_or(if mensaje.is_empty() { format!("código {}", codigo) } else { mensaje }))
        }
    }
}

/// Solo las facturas y sus notas (serie con F) se pueden consultar en
/// SUNAT por tipo, serie y número.
pub fn se_puede_consultar(serie: &str) -> bool {
    serie.trim().to_uppercase().starts_with('F')
}

/// Días calendario que SUNAT da para enviar facturas y boletas desde su
/// emisión (RS 000003-2023/SUNAT para facturas). Se avisa antes de llegar.
pub const DIAS_PLAZO_ENVIO: i64 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logica::detraccion::DetraccionFactura;
    use crate::logica::facturacion::ItemFactura;

    fn emisor() -> Emisor {
        Emisor {
            ruc: "20161515648".into(),
            razon_social: "EMPRESA DE PRUEBA S.A.C.".into(),
            nombre_comercial: "TIENDA PRUEBA".into(),
            direccion: "AV. DE PRUEBA 123".into(),
            ubigeo: "080101".into(),
            departamento: "CUSCO".into(),
            provincia: "CUSCO".into(),
            distrito: "CUSCO".into(),
        }
    }

    fn item(desc: &str, cantidad: f64, precio: f64, afectacion: Afectacion) -> ItemFactura {
        ItemFactura { descripcion: desc.into(), cantidad, precio_unitario: precio, unidad_medida: "UNIDAD".into(), afectacion }
    }

    /// Boleta de 2 polos de S/ 59 y unas medias de S/ 35.40 (IGV 18 %).
    fn boleta() -> DatosParaEmitir {
        let lineas = [(118.0, Afectacion::Gravado), (35.40, Afectacion::Gravado)];
        let d = crate::logica::igv::desglosar(153.40, &lineas, 18.0);
        DatosParaEmitir {
            tipo: "BOLETA".into(),
            cliente_tipo_documento: Some("DNI".into()),
            cliente_documento: Some("12345678".into()),
            cliente_nombre: Some("CLIENTE DE PRUEBA".into()),
            cliente_direccion: None,
            subtotal: d.gravadas,
            igv: d.igv,
            total: d.total,
            tasa: d.tasa,
            exoneradas: d.exoneradas,
            inafectas: d.inafectas,
            items: vec![
                item("Polo algodón talla M", 2.0, 59.0, Afectacion::Gravado),
                item("Medias deportivas pack x3", 1.0, 35.40, Afectacion::Gravado),
            ],
            detraccion: None,
            credito: None,
        }
    }

    #[test]
    fn letras() {
        assert_eq!(monto_en_letras(153.40), "CIENTO CINCUENTA Y TRES CON 40/100 SOLES");
        assert_eq!(monto_en_letras(0.0), "CERO CON 00/100 SOLES");
        assert_eq!(monto_en_letras(100.0), "CIEN CON 00/100 SOLES");
        assert_eq!(monto_en_letras(1001.5), "MIL UNO CON 50/100 SOLES");
        assert_eq!(monto_en_letras(21000.0), "VEINTIÚN MIL CON 00/100 SOLES");
        assert_eq!(monto_en_letras(31500.0), "TREINTA Y UN MIL QUINIENTOS CON 00/100 SOLES");
        assert_eq!(monto_en_letras(2_000_000.0), "DOS MILLONES CON 00/100 SOLES");
        // El redondeo de céntimos no puede dar "100/100".
        assert_eq!(monto_en_letras(9.999), "DIEZ CON 00/100 SOLES");
        assert_eq!(monto_en_letras(374.0), "TRESCIENTOS SETENTA Y CUATRO CON 00/100 SOLES");
    }

    #[test]
    fn boleta_cuadra() {
        let doc = armar_documento(&boleta(), &emisor(), "BM01", 7, "2026-10-08", "15:04:05", "50000000");
        assert_eq!(doc["tipoDoc"], "03");
        assert_eq!(doc["serie"], "BM01");
        assert_eq!(doc["correlativo"], "7");
        assert_eq!(doc["fechaEmision"], "2026-10-08T15:04:05-05:00");
        assert_eq!(doc["client"]["tipoDoc"], "1");
        assert!(doc.get("formaPago").is_none());
        assert_eq!(doc["mtoImpVenta"], 153.4);
        // La suma de las líneas cuadra con los totales.
        let lineas = doc["details"].as_array().unwrap();
        let base: f64 = lineas.iter().map(|l| l["mtoValorVenta"].as_f64().unwrap()).sum();
        let igv: f64 = lineas.iter().map(|l| l["igv"].as_f64().unwrap()).sum();
        assert!((base - doc["mtoOperGravadas"].as_f64().unwrap()).abs() < 0.02);
        assert!((igv - doc["mtoIGV"].as_f64().unwrap()).abs() < 0.02);
        assert_eq!(doc["legends"][0]["value"], "CIENTO CINCUENTA Y TRES CON 40/100 SOLES");
    }

    #[test]
    fn boleta_sin_cliente() {
        let mut d = boleta();
        d.cliente_documento = None;
        d.cliente_nombre = None;
        let doc = armar_documento(&d, &emisor(), "BM01", 1, "2026-10-08", "10:00:00", "50000000");
        assert_eq!(doc["client"]["tipoDoc"], "-");
        assert_eq!(doc["client"]["numDoc"], "-");
        assert_eq!(doc["client"]["rznSocial"], "CLIENTES VARIOS");
    }

    #[test]
    fn factura_al_credito_con_detraccion() {
        let mut d = boleta();
        d.tipo = "FACTURA".into();
        d.cliente_documento = Some("20000000001".into());
        d.detraccion = Some(DetraccionFactura { codigo: "037".into(), porcentaje: 12.0, monto: 18.41, cuenta: "00-000-000000".into() });
        d.credito = Some((153.40, "2026-11-08".into()));
        let doc = armar_documento(&d, &emisor(), "FM01", 3, "2026-10-08", "10:00:00", "50000000");
        assert_eq!(doc["tipoDoc"], "01");
        assert_eq!(doc["client"]["tipoDoc"], "6");
        assert_eq!(doc["tipoOperacion"], "1001");
        assert_eq!(doc["formaPago"]["tipo"], "Credito");
        assert_eq!(doc["cuotas"][0]["monto"], 134.99);
        assert_eq!(doc["detraccion"]["mount"], 18.41);
        assert_eq!(doc["legends"][1]["code"], "2006");
    }

    #[test]
    fn credito_vencido_sale_al_contado() {
        let mut d = boleta();
        d.tipo = "FACTURA".into();
        d.credito = Some((50.0, "2026-10-08".into()));
        let doc = armar_documento(&d, &emisor(), "FM01", 3, "2026-10-08", "10:00:00", "50000000");
        assert_eq!(doc["formaPago"]["tipo"], "Contado");
        assert!(doc.get("cuotas").is_none());
    }

    #[test]
    fn emisor_incompleto() {
        let mut e = emisor();
        assert_eq!(e.dato_faltante(), None);
        e.ubigeo = "".into();
        assert_eq!(e.dato_faltante(), Some("el ubigeo de la dirección fiscal"));
    }

    #[test]
    fn respuestas() {
        let ok = r#"{"xml":"<x/>","hash":"abc","sunatResponse":{"success":true,"cdrZip":"UEs=","cdrResponse":{"id":"BM01-1","code":"0","description":"La Boleta numero BM01-1, ha sido aceptada","notes":[]}}}"#;
        let r = leer_respuesta(200, ok);
        assert_eq!(r.estado, EstadoEnvio::Aceptado);
        assert_eq!(r.hash.as_deref(), Some("abc"));
        assert_eq!(r.cdr_zip.as_deref(), Some("UEs="));

        let obs = r#"{"sunatResponse":{"success":true,"cdrResponse":{"code":"4252","description":"Aceptada","notes":["4252 - dato observado"]}}}"#;
        assert_eq!(leer_respuesta(200, obs).estado, EstadoEnvio::Aceptado);

        let rechazo = r#"{"sunatResponse":{"success":false,"error":{"code":"2017","message":"El numero de documento de identidad del receptor debe ser RUC"}}}"#;
        let r = leer_respuesta(200, rechazo);
        assert_eq!(r.estado, EstadoEnvio::Rechazado);
        assert!(r.mensaje.contains("2017"));

        let excepcion = r#"{"sunatResponse":{"success":false,"error":{"code":"0109","message":"El sistema no puede responder su solicitud"}}}"#;
        assert_eq!(leer_respuesta(200, excepcion).estado, EstadoEnvio::Pendiente);

        let invalido = r#"[{"field":"client.numDoc","message":"This value should not be blank."}]"#;
        let r = leer_respuesta(400, invalido);
        assert_eq!(r.estado, EstadoEnvio::Error);
        assert!(r.mensaje.contains("client.numDoc"));

        assert_eq!(leer_respuesta(502, "<html>").estado, EstadoEnvio::Pendiente);

        let ya = r#"{"sunatResponse":{"success":false,"error":{"code":"1033","message":"El comprobante fue registrado previamente con otros datos"}}}"#;
        let r = leer_respuesta(200, ya);
        assert_eq!(r.estado, EstadoEnvio::Error);
        assert_eq!(r.codigo, Some(CODIGO_YA_REGISTRADO));
        assert!(!r.incierto);
        let sol = r#"{"sunatResponse":{"success":false,"error":{"code":"0102","message":"Usuario o contrasena incorrectos"}}}"#;
        assert_eq!(leer_respuesta(200, sol).estado, EstadoEnvio::Error);
        assert!(leer_respuesta(502, "<html>").incierto);
        assert!(!leer_respuesta(200, excepcion).incierto);
    }

    #[test]
    fn tickets() {
        let con_ticket = r#"{"xml":"<x/>","hash":"h","sunatResponse":{"success":true,"ticket":"1700000000123"}}"#;
        let t = leer_envio_con_ticket(200, con_ticket);
        assert_eq!(t.ticket.as_deref(), Some("1700000000123"));
        assert_eq!(t.respuesta.estado, EstadoEnvio::Pendiente);
        assert_eq!(t.respuesta.xml.as_deref(), Some("<x/>"));

        let sin = r#"{"sunatResponse":{"success":false,"error":{"code":"2223","message":"El documento ya fue informado"}}}"#;
        let t = leer_envio_con_ticket(200, sin);
        assert!(t.ticket.is_none());
        assert_eq!(t.respuesta.estado, EstadoEnvio::Rechazado);

        let caido = leer_envio_con_ticket(502, "<html>");
        assert!(caido.ticket.is_none());
        assert_eq!(caido.respuesta.estado, EstadoEnvio::Pendiente);

        let procesando = r#"{"success":false,"code":"98","error":{"code":"98","message":"En proceso"}}"#;
        let r = leer_estado_ticket(200, procesando);
        assert_eq!(r.estado, EstadoEnvio::Pendiente);
        assert!(r.mensaje.contains("procesando"));

        let ok = r#"{"success":true,"code":"0","cdrZip":"UEs=","cdrResponse":{"id":"RA-20261009-1","code":"0","description":"La Comunicacion de baja RA-20261009-1, ha sido aceptada","notes":[]}}"#;
        let r = leer_estado_ticket(200, ok);
        assert_eq!(r.estado, EstadoEnvio::Aceptado);
        assert_eq!(r.cdr_zip.as_deref(), Some("UEs="));

        let mal = r#"{"success":true,"code":"99","cdrZip":"UEs=","cdrResponse":{"code":"2375","description":"Fecha de emision de la boleta no coincide","notes":[]}}"#;
        assert_eq!(leer_estado_ticket(200, mal).estado, EstadoEnvio::Rechazado);
    }

    #[test]
    fn consulta_cdr() {
        let existe = r#"{"success":true,"code":"0001","message":"El comprobante existe y está aceptado.","cdrZip":"UEs=","cdrResponse":{"code":"0","description":"La Factura numero FM01-5, ha sido aceptada","notes":[]}}"#;
        match leer_consulta_cdr(200, existe) {
            ConsultaCdr::Encontrado(r) => {
                assert_eq!(r.estado, EstadoEnvio::Aceptado);
                assert_eq!(r.cdr_zip.as_deref(), Some("UEs="));
            }
            otro => panic!("{:?}", otro),
        }
        let no = r#"{"success":true,"code":"0011","message":"El comprobante de pago electrónico no existe"}"#;
        assert!(matches!(leer_consulta_cdr(200, no), ConsultaCdr::NoExiste));
        let rech = r#"{"success":true,"code":"0002","message":"existe pero está rechazado"}"#;
        assert!(matches!(leer_consulta_cdr(200, rech), ConsultaCdr::Encontrado(RespuestaSunat { estado: EstadoEnvio::Rechazado, .. })));
        let sin_cred = r#"{"message":"No se encontraron credenciales para el RUC indicado"}"#;
        assert!(matches!(leer_consulta_cdr(400, sin_cred), ConsultaCdr::NoSePudo(m) if m.contains("credenciales")));
        assert!(se_puede_consultar("FM01"));
        assert!(se_puede_consultar("fc01"));
        assert!(!se_puede_consultar("BM01"));
    }
}

/// Prueba contra SUNAT de verdad (ambiente beta) a través de un Lycet
/// levantado aparte. No corre con `cargo test`; se lanza así:
///   LYCET_URL=http://127.0.0.1:8000 LYCET_TOKEN=123456 \
///   cargo test --lib sunat_directo::beta -- --ignored --nocapture
/// El Lycet de prueba usa el RUC 20161515648 con el usuario MODDATOS.
#[cfg(test)]
mod beta {
    use super::*;
    use crate::logica::detraccion::DetraccionFactura;
    use crate::logica::facturacion::ItemFactura;

    fn emisor() -> Emisor {
        Emisor {
            ruc: "20161515648".into(),
            razon_social: "EMPRESA DE PRUEBA S.A.C.".into(),
            nombre_comercial: "TIENDA PRUEBA".into(),
            direccion: "AV. DE PRUEBA 123".into(),
            ubigeo: "080101".into(),
            departamento: "CUSCO".into(),
            provincia: "CUSCO".into(),
            distrito: "CUSCO".into(),
        }
    }

    fn venta(tipo: &str, tasa: f64, items: Vec<ItemFactura>) -> DatosParaEmitir {
        let lineas: Vec<(f64, Afectacion)> = items.iter().map(|i| (i.cantidad * i.precio_unitario, i.afectacion)).collect();
        let total: f64 = lineas.iter().map(|l| l.0).sum();
        let d = crate::logica::igv::desglosar(total, &lineas, tasa);
        DatosParaEmitir {
            tipo: tipo.into(),
            cliente_tipo_documento: Some(if tipo == "FACTURA" { "RUC" } else { "DNI" }.into()),
            cliente_documento: Some(if tipo == "FACTURA" { "20000000001" } else { "12345678" }.into()),
            cliente_nombre: Some(if tipo == "FACTURA" { "EMPRESA CLIENTE S.A.C." } else { "CLIENTE DE PRUEBA" }.into()),
            cliente_direccion: if tipo == "FACTURA" { Some("JR. PRUEBA 456, LIMA".into()) } else { None },
            subtotal: d.gravadas,
            igv: d.igv,
            total: d.total,
            tasa: d.tasa,
            exoneradas: d.exoneradas,
            inafectas: d.inafectas,
            items,
            detraccion: None,
            credito: None,
        }
    }

    fn it(desc: &str, cant: f64, precio: f64, unidad: &str, af: Afectacion) -> ItemFactura {
        ItemFactura { descripcion: desc.into(), cantidad: cant, precio_unitario: precio, unidad_medida: unidad.into(), afectacion: af }
    }

    #[tokio::test]
    #[ignore]
    async fn emitir_en_beta() {
        let url = std::env::var("LYCET_URL").expect("falta LYCET_URL");
        let token = std::env::var("LYCET_TOKEN").unwrap_or_default();
        let base = (chrono::Utc::now().timestamp() % 1_000_000) as i64;
        let hoy = crate::logica::tiempo::hoy_lima();
        let vence = crate::logica::tiempo::hoy_lima_mas_dias(30);
        let ropa = || vec![
            it("Polo algodón talla M", 2.0, 59.0, "UNIDAD", Afectacion::Gravado),
            it("Medias deportivas pack x3", 1.0, 35.40, "UNIDAD", Afectacion::Gravado),
        ];

        let mut casos: Vec<(&str, &str, DatosParaEmitir)> = Vec::new();
        casos.push(("boleta con DNI", "BM01", venta("BOLETA", 18.0, ropa())));
        let mut sin_cliente = venta("BOLETA", 18.0, ropa());
        sin_cliente.cliente_documento = None;
        sin_cliente.cliente_nombre = None;
        casos.push(("boleta sin cliente", "BM01", sin_cliente));
        casos.push(("boleta gravada + exonerada + inafecta, con kilos", "BM01", venta("BOLETA", 18.0, vec![
            it("Polo algodón", 1.0, 59.0, "UNIDAD", Afectacion::Gravado),
            it("Libro escolar", 1.0, 25.0, "UNIDAD", Afectacion::Exonerado),
            it("Papa amarilla", 0.75, 4.20, "KG", Afectacion::Inafecto),
        ])));
        casos.push(("boleta IGV 10.5 %", "BM01", venta("BOLETA", 10.5, vec![it("Menú del día", 3.0, 15.0, "UNIDAD", Afectacion::Gravado)])));
        casos.push(("factura al contado", "FM01", venta("FACTURA", 18.0, ropa())));
        let mut credito = venta("FACTURA", 18.0, ropa());
        credito.credito = Some((credito.total, vence.clone()));
        casos.push(("factura al crédito", "FM01", credito));
        let mut detra = venta("FACTURA", 18.0, vec![it("Servicio de confección de uniformes", 1.0, 1200.0, "UNIDAD", Afectacion::Gravado)]);
        detra.detraccion = Some(DetraccionFactura { codigo: "037".into(), porcentaje: 12.0, monto: 144.0, cuenta: "00-123-456789".into() });
        detra.credito = Some((detra.total, vence.clone()));
        casos.push(("factura con detracción al crédito", "FM01", detra));

        let cliente = reqwest::Client::new();
        let mut fallas = 0;
        for (i, (nombre, serie, datos)) in casos.into_iter().enumerate() {
            let doc = armar_documento(&datos, &emisor(), serie, base + i as i64, &hoy, "10:00:00", "53100000");
            let resp = cliente
                .post(format!("{}/api/v1/invoice/send?token={}", url, token))
                .json(&doc)
                .send()
                .await
                .expect("Lycet no responde");
            let status = resp.status().as_u16();
            let texto = resp.text().await.unwrap_or_default();
            let r = leer_respuesta(status, &texto);
            println!("{:<48} {:<10} {}", nombre, r.estado.como_texto(), r.mensaje);
            if r.estado != EstadoEnvio::Aceptado {
                fallas += 1;
                println!("   documento: {}", doc);
            }
        }
        assert_eq!(fallas, 0, "hubo comprobantes no aceptados");
    }
}
