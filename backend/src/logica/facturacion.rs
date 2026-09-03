use serde::Deserialize;
use serde_json::json;

pub struct ResultadoEmision {
    pub aceptado: bool,
    pub serie: String,
    pub numero: i64,
    pub mensaje: String,
    pub enlace_pdf: Option<String>,
    pub enlace_cdr: Option<String>,
    pub external_id: Option<String>,
    pub hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ItemFactura {
    pub descripcion: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub unidad_medida: String,
}

pub struct DatosParaEmitir {
    pub tipo: String,
    pub cliente_tipo_documento: Option<String>,
    pub cliente_documento: Option<String>,
    pub cliente_nombre: Option<String>,
    pub cliente_direccion: Option<String>,
    pub subtotal: f64,
    pub igv: f64,
    pub total: f64,
    pub items: Vec<ItemFactura>,
}

fn unidad_sunat(unidad_medida: &str) -> &'static str {
    match unidad_medida {
        "KG" => "KGM",
        "GRAMO" => "GRM",
        "LITRO" => "LTR",
        "ML" => "MLT",
        "PAQUETE" => "NIU",
        _ => "NIU",
    }
}

/// Catálogo 06 de SUNAT (tipos de documento de identidad).
fn codigo_tipo_documento_identidad(tipo: &str) -> &'static str {
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

/// Llama a la API real de FacturaLibre.org (compatible FacturadorPro).
/// Contrato de campos confirmado con la documentación oficial completa.
pub async fn emitir_facturalibre(
    datos: &DatosParaEmitir,
    token: &str,
    ruta: &str,
    codigo_producto_sunat: &str,
) -> ResultadoEmision {
    let serie = if datos.tipo == "FACTURA" { "F001" } else { "B001" };
    let codigo_tipo_doc = if datos.tipo == "FACTURA" { "01" } else { "03" };
    let ahora = chrono::Local::now();
    let hoy = ahora.format("%Y-%m-%d").to_string();
    let hora_actual = ahora.format("%H:%M:%S").to_string();

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
            let valor_unitario = it.precio_unitario / 1.18;
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
                "codigo_tipo_afectacion_igv": "10",
                "total_base_igv": round2(total_base_igv),
                "porcentaje_igv": 18,
                "total_igv": round2(total_igv_item),
                "total_impuestos": round2(total_igv_item),
                "total_valor_item": round2(total_base_igv),
                "total_item": round2(total_item),
            })
        })
        .collect();

    let payload = json!({
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
            "total_operaciones_inafectas": 0.00,
            "total_operaciones_exoneradas": 0.00,
            "total_operaciones_gratuitas": 0.00,
            "total_igv": round2(datos.igv),
            "total_impuestos": round2(datos.igv),
            "total_valor": round2(datos.subtotal),
            "total_venta": round2(datos.total),
        },
        "items": items_json,
    });

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
        },
    }
}