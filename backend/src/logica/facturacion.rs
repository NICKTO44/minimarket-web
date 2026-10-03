use serde::Deserialize;
use serde_json::json;

use crate::logica::detraccion::DetraccionFactura;
use crate::logica::igv::{valor_sin_igv, Afectacion};

pub struct ResultadoEmision {
    pub aceptado: bool,
    pub serie: String,
    pub numero: i64,
    pub mensaje: String,
    pub enlace_pdf: Option<String>,
    pub enlace_cdr: Option<String>,
    pub external_id: Option<String>,
    pub hash: Option<String>,
    /// Fecha de emisión exacta que se le mandó a FacturaLibre — se
    /// devuelve para que el QR use la MISMA fecha que quedó en el
    /// documento real, nunca una recalculada por separado (evita
    /// desincronías si la emisión ocurre justo al filo de medianoche).
    pub fecha_emision: String,
}

#[derive(Debug, Clone)]
pub struct ItemFactura {
    pub descripcion: String,
    pub cantidad: f64,
    /// Precio unitario cobrado, con IGV incluido si la línea es gravada.
    pub precio_unitario: f64,
    pub unidad_medida: String,
    /// Gravado, exonerado o inafecto (catálogo 07 de SUNAT).
    pub afectacion: Afectacion,
}

pub struct DatosParaEmitir {
    pub tipo: String,
    pub cliente_tipo_documento: Option<String>,
    pub cliente_documento: Option<String>,
    pub cliente_nombre: Option<String>,
    pub cliente_direccion: Option<String>,
    /// Total de operaciones gravadas (valor de venta, sin IGV).
    pub subtotal: f64,
    pub igv: f64,
    pub total: f64,
    /// Tasa de IGV del negocio en esta venta (18, 10.5...).
    pub tasa: f64,
    pub exoneradas: f64,
    pub inafectas: f64,
    pub items: Vec<ItemFactura>,
    /// Some = factura sujeta a detracción (SPOT). None = comprobante normal.
    pub detraccion: Option<DetraccionFactura>,
    /// Some = factura de una venta al crédito: (saldo pendiente, fecha de
    /// vencimiento "AAAA-MM-DD"). None = pago al contado.
    pub credito: Option<(f64, String)>,
}

/// La tasa como la espera la API: entera si no tiene decimales (18), con
/// decimales si los tiene (10.5).
fn porcentaje_json(tasa: f64) -> serde_json::Value {
    if tasa.fract() == 0.0 { json!(tasa as i64) } else { json!(tasa) }
}

/// Porcentaje que se declara en una línea exonerada o inafecta (su IGV es
/// siempre 0). La documentación del formato FacturadorPro lo pide fijo en
/// 18; otra documentación del mismo formato usa 0. PENDIENTE de confirmar
/// con un comprobante de prueba en el modo demo de FacturaLibre antes de
/// usar productos exonerados en producción.
const PORCENTAJE_LINEA_SIN_IGV: i64 = 18;

pub fn unidad_sunat(unidad_medida: &str) -> &'static str {
    match unidad_medida {
        "KG" => "KGM",
        "GRAMO" => "GRM",
        "LITRO" => "LTR",
        "ML" => "MLT",
        "PAQUETE" => "NIU",
        _ => "NIU",
    }
}

/// Catálogo 06 de SUNAT (tipos de documento de identidad). Pública para
/// que el handler la reuse al armar el ComprobanteResponse — así el
/// código que va al QR es exactamente el mismo que se mandó a
/// FacturaLibre, sin riesgo de que se desincronicen dos copias de esta
/// lógica.
pub fn codigo_tipo_documento_identidad(tipo: &str) -> &'static str {
    match tipo {
        "DNI" => "1",
        "CE" => "4",
        "RUC" => "6",
        "PASAPORTE" => "7",
        _ => "0",
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

// ===== Respuesta real de la API (confirmado con la documentación oficial:
// https://pdfcoffee.com/documentacion-api-rest-pdf-free.html — mismo motor
// que usa FacturaLibre por debajo). Viene anidada en data/links/response,
// no plana como se asumió originalmente. =====

#[derive(Debug, Deserialize, Default)]
struct RespuestaFacturaLibre {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<DatosRespuesta>,
    #[serde(default)]
    links: Option<LinksRespuesta>,
    // "response" a veces viene como objeto {code,description,notes} y a
    // veces como arreglo vacío [] — se deja como Value crudo para no
    // romper el parseo con ninguno de los dos formatos.
    #[serde(default)]
    response: Option<serde_json::Value>,
    // Para las respuestas de error simples, tipo {"success":false,"message":"..."}
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DatosRespuesta {
    #[serde(default)]
    number: Option<String>, // ej. "B001-4" — hay que partirlo en serie + número
    #[serde(default)]
    external_id: Option<String>,
    #[serde(default)]
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LinksRespuesta {
    #[serde(default)]
    pdf: Option<String>,
    #[serde(default)]
    cdr: Option<String>,
}

/// Arma el documento que se envía a FacturaLibre (formato FacturadorPro).
/// Separado de la llamada HTTP para poder probarlo sin emitir nada.
pub fn armar_payload(
    datos: &DatosParaEmitir,
    codigo_producto_sunat: &str,
    serie: &str,
    hoy: &str,
    hora_actual: &str,
) -> serde_json::Value {
    let codigo_tipo_doc = if datos.tipo == "FACTURA" { "01" } else { "03" };

    let tipo_doc_cliente = if datos.tipo == "FACTURA" {
        "RUC"
    } else {
        datos.cliente_tipo_documento.as_deref().unwrap_or("DNI")
    };

    // La API pide el desglose de IGV a nivel de cada ítem, no solo a nivel
    // de documento — nuestros precios ya incluyen IGV (precio de venta al
    // público), así que se calcula el valor sin IGV de cada línea aquí.
    let items_json: Vec<_> = datos
        .items
        .iter()
        .enumerate()
        .map(|(idx, it)| {
            // Gravado: el precio incluye IGV y se desglosa con la tasa del
            // negocio. Exonerado/inafecto: el precio ES el valor, IGV 0.
            let gravado = it.afectacion == Afectacion::Gravado;
            let valor_unitario = valor_sin_igv(it.precio_unitario, it.afectacion, datos.tasa);
            let total_item = it.cantidad * it.precio_unitario;
            let total_base_igv = it.cantidad * valor_unitario;
            let total_igv_item = total_item - total_base_igv;

            json!({
                "codigo_interno": format!("P{:03}", idx + 1),
                "descripcion": it.descripcion,
                "codigo_producto_sunat": codigo_producto_sunat,
                "unidad_de_medida": unidad_sunat(&it.unidad_medida),
                "cantidad": it.cantidad,
                "valor_unitario": round2(valor_unitario),
                "codigo_tipo_precio": "01",
                "precio_unitario": it.precio_unitario,
                "codigo_tipo_afectacion_igv": it.afectacion.codigo_sunat(),
                "total_base_igv": round2(total_base_igv),
                "porcentaje_igv": if gravado { porcentaje_json(datos.tasa) } else { json!(PORCENTAJE_LINEA_SIN_IGV) },
                "total_igv": round2(total_igv_item),
                "total_impuestos": round2(total_igv_item),
                "total_valor_item": round2(total_base_igv),
                "total_item": round2(total_item),
            })
        })
        .collect();

    let mut documento = json!({
        "serie_documento": serie,
        "numero_documento": "#",
        "fecha_de_emision": hoy,
        "hora_de_emision": hora_actual,
        "codigo_tipo_operacion": "0101",
        "codigo_tipo_documento": codigo_tipo_doc,
        "codigo_tipo_moneda": "PEN",
        "fecha_de_vencimiento": hoy,
        "datos_del_cliente_o_receptor": {
            "codigo_tipo_documento_identidad": codigo_tipo_documento_identidad(tipo_doc_cliente),
            "numero_documento": datos.cliente_documento.clone().unwrap_or_default(),
            "apellidos_y_nombres_o_razon_social": datos.cliente_nombre.clone().unwrap_or_else(|| "Cliente varios".to_string()),
            "direccion": datos.cliente_direccion.clone().unwrap_or_default(),
        },
        "totales": {
            "total_exportacion": 0.00,
            "total_operaciones_gravadas": round2(datos.subtotal),
            "total_operaciones_inafectas": round2(datos.inafectas),
            "total_operaciones_exoneradas": round2(datos.exoneradas),
            "total_operaciones_gratuitas": 0.00,
            "total_igv": round2(datos.igv),
            "total_impuestos": round2(datos.igv),
            "total_valor": round2(datos.subtotal + datos.exoneradas + datos.inafectas),
            "total_venta": round2(datos.total),
        },
        "items": items_json,
    });

    // Operación sujeta a detracción (formato del ejemplo "Factura Gravada -
    // Detracción - Pago Contado" de la documentación de FacturaLibre): tipo
    // de operación 1001, leyenda 2006 y el bloque con el depósito. Sin
    // detracción el documento queda exactamente igual que siempre.
    if let Some(d) = &datos.detraccion {
        documento["codigo_tipo_operacion"] = json!("1001");
        documento["codigo_condicion_de_pago"] = json!("01");
        documento["totales"]["total_pendiente_pago"] = json!(round2(datos.total - d.monto));
        documento["leyendas"] = json!([{ "codigo": "2006", "valor": "Operación sujeta a detracción" }]);
        documento["detraccion"] = json!({
            "codigo_tipo_detraccion": d.codigo,
            "porcentaje": d.porcentaje,
            "monto": round2(d.monto),
            // Catálogo 59 de SUNAT: 001 = depósito en cuenta.
            "codigo_metodo_pago": "001",
            "cuenta_bancaria": d.cuenta,
        });
    }

    // Factura al crédito (ejemplo "Factura Gravada - Pago Credito" de la
    // documentación de FacturaLibre): condición 02 y una cuota por lo que
    // queda por pagar. Con detracción, la cuota es el saldo menos el
    // depósito, igual que en su ejemplo "Detracción - Pago Credito".
    if datos.tipo == "FACTURA" {
        if let Some((saldo, vence)) = &datos.credito {
            let detraccion = datos.detraccion.as_ref().map(|d| d.monto).unwrap_or(0.0);
            let cuota = round2(saldo - detraccion);
            if cuota > 0.0 && vence.as_str() > hoy {
                documento["codigo_condicion_de_pago"] = json!("02");
                documento["fecha_de_vencimiento"] = json!(vence);
                documento["cuotas"] = json!([{ "fecha": vence, "codigo_tipo_moneda": "PEN", "monto": cuota }]);
            }
        }
    }
    documento
}

/// Llama a la API real de FacturaLibre.org (compatible FacturadorPro).
/// Contrato de campos confirmado con la documentación oficial completa.
pub async fn emitir_facturalibre(
    datos: &DatosParaEmitir,
    token: &str,
    ruta: &str,
    codigo_producto_sunat: &str,
    serie_boleta: &str,
    serie_factura: &str,
) -> ResultadoEmision {
    let serie = if datos.tipo == "FACTURA" { serie_factura } else { serie_boleta };
    let ahora = chrono::Local::now();
    let hoy = ahora.format("%Y-%m-%d").to_string();
    let hora_actual = ahora.format("%H:%M:%S").to_string();
    let payload = armar_payload(datos, codigo_producto_sunat, serie, &hoy, &hora_actual);

    let cliente = reqwest::Client::new();
    let respuesta = cliente.post(ruta).bearer_auth(token).json(&payload).send().await;

    let resp = match respuesta {
        Ok(r) => r,
        Err(e) => {
            return ResultadoEmision {
                aceptado: false,
                serie: serie.to_string(),
                numero: 0,
                mensaje: format!("No se pudo conectar con FacturaLibre: {}", e),
                enlace_pdf: None,
                enlace_cdr: None,
                external_id: None,
                hash: None,
                fecha_emision: hoy.clone(),
            }
        }
    };

    let exitoso_http = resp.status().is_success();
    let texto = resp.text().await.unwrap_or_default();

    match serde_json::from_str::<RespuestaFacturaLibre>(&texto) {
        Ok(r) => {
            let aceptado = exitoso_http && r.success;

            let (serie_out, numero_out) = r
                .data
                .as_ref()
                .and_then(|d| d.number.as_ref())
                .and_then(|n| n.rsplit_once('-'))
                .map(|(s, n)| (s.to_string(), n.parse::<i64>().unwrap_or(0)))
                .unwrap_or_else(|| (serie.to_string(), 0));

            let enlace_pdf = r.links.as_ref().and_then(|l| l.pdf.clone());
            let enlace_cdr = r.links.as_ref().and_then(|l| l.cdr.clone());
            let external_id = r.data.as_ref().and_then(|d| d.external_id.clone());
            let hash = r.data.as_ref().and_then(|d| d.hash.clone());

            // "response" puede venir como objeto {code, description, notes}
            // (caso normal) o como arreglo vacío [] (algunos ejemplos de la
            // doc oficial) — si es objeto, sacamos la descripción de SUNAT.
            let descripcion_sunat = r
                .response
                .as_ref()
                .and_then(|v| v.as_object())
                .and_then(|obj| obj.get("description"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mensaje = descripcion_sunat.or(r.message).unwrap_or_else(|| {
                if aceptado {
                    "Comprobante emitido correctamente".to_string()
                } else {
                    format!("FacturaLibre respondió con error: {}", texto)
                }
            });

            ResultadoEmision {
                aceptado,
                serie: serie_out,
                numero: numero_out,
                mensaje,
                enlace_pdf,
                enlace_cdr,
                external_id,
                hash,
                fecha_emision: hoy,
            }
        }
        Err(_) => ResultadoEmision {
            aceptado: false,
            serie: serie.to_string(),
            numero: 0,
            mensaje: format!("Respuesta inesperada de FacturaLibre (revisar formato): {}", texto),
            enlace_pdf: None,
            enlace_cdr: None,
            external_id: None,
            hash: None,
            fecha_emision: hoy,
        },
    }
}