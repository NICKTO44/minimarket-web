use serde::Deserialize;
use serde_json::json;

pub struct ResultadoEmision {
    pub aceptado: bool,
    pub serie: String,
    pub numero: i64,
    pub mensaje: String,
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

#[derive(Debug, Deserialize)]
struct RespuestaFacturaLibre {
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    serie_documento: Option<String>,
    #[serde(default)]
    numero_documento: Option<serde_json::Value>,
    #[serde(default)]
    enlace_del_cdr: Option<String>,
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default)]
    mensaje: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Llama a la API real de FacturaLibre.org. Contrato de campos confirmado
/// contra la documentación Postman de la cuenta:
/// https://documenter.getpostman.com/view/6435177/TVRrUPuD
/// Si FacturaLibre devuelve un error de validación de campo, revisar esa
/// documentación (mismo proceso que reveló la falta de
/// "fecha_de_vencimiento" en la integración de Lubricentro).
pub async fn emitir_facturalibre(
    datos: &DatosParaEmitir,
    token: &str,
    ruta: &str,
    codigo_producto_sunat: &str,
) -> ResultadoEmision {
    let serie = if datos.tipo == "FACTURA" { "F001" } else { "B001" };
    let hoy = chrono::Local::now().format("%Y-%m-%d").to_string();

    let tipo_doc_cliente = if datos.tipo == "FACTURA" {
        "RUC"
    } else {
        datos.cliente_tipo_documento.as_deref().unwrap_or("DNI")
    };

    let items_json: Vec<_> = datos
        .items
        .iter()
        .map(|it| {
            json!({
                "codigo_producto_sunat": codigo_producto_sunat,
                "descripcion": it.descripcion,
                "unidad_de_medida": unidad_sunat(&it.unidad_medida),
                "cantidad": it.cantidad,
                "precio_unitario": it.precio_unitario,
                "valor_venta": it.cantidad * it.precio_unitario,
            })
        })
        .collect();

    let payload = json!({
        "serie_documento": serie,
        "numero_documento": "#",
        "fecha_de_emision": hoy,
        "fecha_de_vencimiento": hoy,
        "datos_del_cliente_o_receptor": {
            "tipo_de_documento": tipo_doc_cliente,
            "numero_de_documento": datos.cliente_documento.clone().unwrap_or_default(),
            "nombre_o_razon_social": datos.cliente_nombre.clone().unwrap_or_else(|| "Cliente varios".to_string()),
            "direccion": datos.cliente_direccion.clone().unwrap_or_default(),
        },
        "totales": {
            "subtotal": datos.subtotal,
            "igv": datos.igv,
            "total": datos.total,
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
            let numero: i64 = r
                .numero_documento
                .as_ref()
                .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .unwrap_or(0);

            let external_id = r.id.as_ref().map(|v| v.to_string());
            let aceptado = exitoso_http && r.success.unwrap_or(exitoso_http);
            let mensaje = r.mensaje.or(r.message).or(r.error).unwrap_or_else(|| {
                if aceptado {
                    "Comprobante emitido correctamente".to_string()
                } else {
                    format!("FacturaLibre respondió con error: {}", texto)
                }
            });

            ResultadoEmision {
                aceptado,
                serie: r.serie_documento.unwrap_or_else(|| serie.to_string()),
                numero,
                mensaje,
                enlace_cdr: r.enlace_del_cdr,
                external_id,
                hash: r.hash,
            }
        }
        Err(_) => ResultadoEmision {
            aceptado: false,
            serie: serie.to_string(),
            numero: 0,
            mensaje: format!("Respuesta inesperada de FacturaLibre (revisar formato): {}", texto),
            enlace_cdr: None,
            external_id: None,
            hash: None,
        },
    }
}