//! Emisión directa a SUNAT para los negocios con
//! configuracion_tienda.facturacion_proveedor = 'SUNAT_DIRECTO'.
//!
//! A diferencia de FacturaLibre, aquí el número del comprobante lo pone el
//! sistema: se reserva ANTES de enviar (fila PENDIENTE con un índice único
//! por serie y número), se arma el documento, se guarda tal cual y recién
//! entonces se manda a Lycet. Si SUNAT no responde, el comprobante queda
//! PENDIENTE con su número y su documento guardados, y volver a emitir la
//! misma venta lo reenvía igual (mismo número, misma fecha), sin crear otro.
//!
//! El servidor de Lycet se configura en el .env:
//!   LYCET_URL=http://127.0.0.1:8000   (sin barra final)
//!   LYCET_TOKEN=...                   (el CLIENT_TOKEN de Lycet)

use axum::{http::StatusCode, Json};
use serde_json::Value;

use crate::logica::detraccion::DetraccionFactura;
use crate::logica::facturacion::DatosParaEmitir;
use crate::logica::igv::Desglose;
use crate::logica::sunat_directo::{self, Emisor, EstadoEnvio};
use crate::models::facturacion::ComprobanteResponse;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Lo que el handler de siempre ya calculó para esta venta.
pub struct Pedido<'a> {
    pub venta_id: i64,
    /// Serie que corresponde al tipo (la de boleta o la de factura).
    pub serie: String,
    pub codigo_producto_sunat: String,
    pub desglose: &'a Desglose,
    pub detraccion: Option<DetraccionFactura>,
}

/// Dirección y token de Lycet, del .env.
pub fn servidor_lycet() -> Option<(String, String)> {
    let url = std::env::var("LYCET_URL").ok()?.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return None;
    }
    Some((url, std::env::var("LYCET_TOKEN").unwrap_or_default().trim().to_string()))
}

/// Datos del negocio como emisor.
pub async fn leer_emisor(conn: &libsql::Connection) -> Result<Emisor, Fallo> {
    let mut filas = conn
        .query(
            "SELECT ruc, razon_social, nombre_tienda, direccion, ubigeo, departamento, provincia, distrito
             FROM configuracion_tienda LIMIT 1",
            (),
        )
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Este negocio todavía no tiene la actualización para emitir directo a SUNAT (migración 0020).".to_string(),
            )
        })?;
    let fila = filas
        .next()
        .await
        .map_err(interno)?
        .ok_or((StatusCode::BAD_REQUEST, "El negocio no tiene configuración.".to_string()))?;
    let texto = |i: i32| fila.get::<String>(i).unwrap_or_default();
    Ok(Emisor {
        ruc: texto(0),
        razon_social: texto(1),
        nombre_comercial: texto(2),
        direccion: texto(3),
        ubigeo: texto(4),
        departamento: texto(5),
        provincia: texto(6),
        distrito: texto(7),
    })
}

/// Comprobante directo de esta venta que ya fue aceptado o que quedó
/// pendiente de envío: (id, serie, numero, estado).
async fn comprobante_existente(conn: &libsql::Connection, venta_id: i64) -> Result<Option<(i64, String, i64, String)>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT id, serie, numero, estado FROM comprobantes_electronicos
             WHERE venta_id = ?1 AND proveedor = 'SUNAT_DIRECTO' AND estado IN ('ACEPTADO', 'PENDIENTE')
             ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .map_err(interno)?;
    Ok(match filas.next().await.map_err(interno)? {
        Some(f) => Some((
            f.get(0).map_err(interno)?,
            f.get::<String>(1).unwrap_or_default(),
            f.get::<i64>(2).unwrap_or_default(),
            f.get::<String>(3).unwrap_or_default(),
        )),
        None => None,
    })
}

/// Reserva el siguiente número de la serie con una fila PENDIENTE.
/// El índice único (serie, numero) impide que dos emisiones simultáneas se
/// lleven el mismo: la que choca vuelve a intentar con el siguiente.
async fn reservar_numero(
    conn: &libsql::Connection,
    venta_id: i64,
    datos: &DatosParaEmitir,
    serie: &str,
    fecha_hora: &str,
) -> Result<(i64, i64), Fallo> {
    let mut ultimo_error = String::new();
    for _ in 0..5 {
        let insertado = conn
            .execute(
                "INSERT INTO comprobantes_electronicos
                    (venta_id, tipo, proveedor, serie, numero, cliente_documento, cliente_nombre, estado, mensaje_sunat, fecha_emision)
                 SELECT ?1, ?2, 'SUNAT_DIRECTO', ?3, COALESCE(MAX(numero), 0) + 1, ?4, ?5, 'PENDIENTE', 'Enviando a SUNAT...', ?6
                 FROM comprobantes_electronicos WHERE serie = ?3",
                libsql::params![
                    venta_id,
                    datos.tipo.clone(),
                    serie,
                    datos.cliente_documento.clone(),
                    datos.cliente_nombre.clone(),
                    fecha_hora
                ],
            )
            .await;
        match insertado {
            Ok(_) => {
                let id = conn.last_insert_rowid();
                let mut filas = conn
                    .query("SELECT numero FROM comprobantes_electronicos WHERE id = ?1", libsql::params![id])
                    .await
                    .map_err(interno)?;
                let numero: i64 = match filas.next().await.map_err(interno)? {
                    Some(f) => f.get(0).map_err(interno)?,
                    None => return Err(interno("No se pudo leer el número reservado")),
                };
                return Ok((id, numero));
            }
            Err(e) => {
                ultimo_error = e.to_string();
                if !ultimo_error.to_uppercase().contains("UNIQUE") {
                    break;
                }
            }
        }
    }
    Err((StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo reservar el número del comprobante: {}", ultimo_error)))
}

pub async fn emitir(
    conn: &libsql::Connection,
    pedido: Pedido<'_>,
    datos: DatosParaEmitir,
) -> Result<Json<ComprobanteResponse>, Fallo> {
    let lycet = super::envios_sunat::Lycet::exigir()?;

    let emisor = leer_emisor(conn).await?;
    if let Some(falta) = emisor.dato_faltante() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Falta {} del negocio para emitir comprobantes a SUNAT.", falta),
        ));
    }

    // ¿Esta venta ya tiene un comprobante directo?
    let (comprobante_id, serie, numero, documento, es_nuevo) = match comprobante_existente(conn, pedido.venta_id).await? {
        Some((_, serie, numero, estado)) if estado == "ACEPTADO" => {
            return Err((
                StatusCode::CONFLICT,
                format!("Esta venta ya tiene el comprobante {}-{} aceptado por SUNAT.", serie, numero),
            ));
        }
        Some((id, serie, numero, _pendiente)) => {
            // Reenvío: el mismo documento que se guardó la primera vez.
            let documento: Value = super::envios_sunat::documento_guardado(conn, id)
                .await?
                .ok_or_else(|| interno(format!("El comprobante pendiente {}-{} no tiene su documento guardado.", serie, numero)))?;
            (id, serie, numero, documento, false)
        }
        None => {
            let ahora = crate::logica::tiempo::ahora_lima(); // "AAAA-MM-DD HH:MM:SS"
            let (fecha, hora) = ahora.split_once(' ').unwrap_or((ahora.as_str(), "00:00:00"));
            let (id, numero) = reservar_numero(conn, pedido.venta_id, &datos, &pedido.serie, &ahora).await?;
            let documento = sunat_directo::armar_documento(
                &datos,
                &emisor,
                &pedido.serie,
                numero,
                fecha,
                hora,
                &pedido.codigo_producto_sunat,
            );
            conn.execute(
                "INSERT INTO comprobante_archivos (comprobante_id, documento, actualizado) VALUES (?1, ?2, ?3)",
                libsql::params![id, documento.to_string(), ahora.clone()],
            )
            .await
            .map_err(interno)?;
            (id, pedido.serie.clone(), numero, documento, true)
        }
    };

    // Primer envío directo. Volver a emitir una venta pendiente es un
    // reenvío como el del botón (ver envios_sunat::reenviar_comprobante).
    let respuesta = if es_nuevo {
        let r = lycet.enviar("invoice", &documento).await;
        super::envios_sunat::guardar_resultado(conn, comprobante_id, &r, false).await?;
        r
    } else {
        super::envios_sunat::reenviar_comprobante(conn, &lycet, comprobante_id).await?
    };

    if es_nuevo {
        if let Some(d) = &pedido.detraccion {
            let _ = conn
                .execute(
                    "UPDATE comprobantes_electronicos SET detraccion_porcentaje = ?1, detraccion_monto = ?2, detraccion_cuenta = ?3 WHERE id = ?4",
                    libsql::params![d.porcentaje, d.monto, d.cuenta.clone(), comprobante_id],
                )
                .await;
        }
    }

    // La fecha que quedó en el documento (también en un reenvío), para que
    // el QR use exactamente la misma.
    let fecha_emision = documento["fechaEmision"].as_str().map(|f| f.chars().take(10).collect::<String>());
    let (tipo_doc_cliente, num_doc_cliente) = sunat_directo::documento_adquirente(&datos);
    let d = pedido.desglose;

    Ok(Json(ComprobanteResponse {
        success: respuesta.estado == EstadoEnvio::Aceptado,
        comprobante_id: Some(comprobante_id),
        tipo: datos.tipo.clone(),
        serie,
        numero,
        estado: respuesta.estado.como_texto().to_string(),
        mensaje: respuesta.mensaje,
        enlace_pdf: None,
        hash: respuesta.hash,
        ruc_emisor: Some(emisor.ruc),
        fecha_emision,
        igv: d.igv,
        total_venta: d.total,
        igv_tasa: d.tasa,
        op_gravadas: d.gravadas,
        op_exoneradas: d.exoneradas,
        op_inafectas: d.inafectas,
        cliente_tipo_documento_codigo: tipo_doc_cliente,
        cliente_numero_documento: num_doc_cliente,
        detraccion_porcentaje: pedido.detraccion.as_ref().map(|x| x.porcentaje),
        detraccion_monto: pedido.detraccion.as_ref().map(|x| x.monto),
        detraccion_cuenta: pedido.detraccion.as_ref().map(|x| x.cuenta.clone()),
    }))
}
