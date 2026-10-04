use axum::{extract::Extension, Json, http::StatusCode};
use std::sync::Arc;

use crate::tenants::TenantDb;
use crate::models::facturacion::*;
use crate::logica::facturacion::{emitir_facturalibre, codigo_tipo_documento_identidad, DatosParaEmitir, ItemFactura};

pub async fn emitir_comprobante(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<EmitirComprobanteRequest>,
) -> Result<Json<ComprobanteResponse>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Líneas y desglose de IGV tal como se cobró la venta: tasa del
    // negocio y qué líneas son gravadas, exoneradas o inafectas.
    let (lineas, desglose) = crate::handlers::igv::desglose_venta(&conn, payload.venta_id)
        .await
        .map_err(|e| {
            if e == "Venta no encontrada" { (StatusCode::NOT_FOUND, e) } else { (StatusCode::INTERNAL_SERVER_ERROR, e) }
        })?;
    let total = desglose.total;
    let igv = desglose.igv;
    let base_sin_igv = desglose.gravadas;

    let items: Vec<ItemFactura> = lineas
        .into_iter()
        .map(|l| ItemFactura {
            descripcion: l.descripcion,
            cantidad: l.cantidad,
            precio_unitario: l.precio_unitario,
            unidad_medida: l.unidad_medida,
            afectacion: l.afectacion,
        })
        .collect();

    // Dirección del cliente de la venta (una sola consulta).
    let mut cliente_direccion: Option<String> = None;
    let mut r_cliente = conn
        .query(
            "SELECT c.direccion FROM ventas v JOIN clientes c ON c.id = v.cliente_id WHERE v.id = ?1",
            libsql::params![payload.venta_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(row) = r_cliente.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        cliente_direccion = row.get(0).ok();
    }
    drop(r_cliente);

    // Valor de respaldo si el tenant no tiene nada guardado en
    // codigo_producto_sunat_generico (columna vacía/NULL) — antes este
    // era el único valor posible para todos los tenants; ahora es solo
    // el "por defecto" cuando nadie configuró uno específico.
    const CODIGO_PRODUCTO_SUNAT_RESPALDO: &str = "50000000";

    // Se agrega codigo_producto_sunat_generico a la consulta — existía
    // en la tabla desde el schema original pero nunca se leía; el
    // sistema siempre mandaba la constante fija sin importar lo que
    // tuviera guardado cada tenant.
    // Configuración de emisión y, en la misma consulta, la de detracción
    // (un viaje a la base en vez de cuatro). Si la base aún no tiene las
    // columnas nuevas (migraciones 0011/0013), se lee como antes.
    const COLUMNAS_EMISION: &str = "facturalibre_token, facturalibre_ruta, ruc, serie_boleta, serie_factura, codigo_producto_sunat_generico";
    let completa = conn
        .query(
            &format!(
                "SELECT {}, modulos, CAST(detraccion_porcentaje AS REAL), detraccion_codigo,
                        CAST(detraccion_minimo AS REAL), detraccion_cuenta
                 FROM configuracion_tienda LIMIT 1",
                COLUMNAS_EMISION
            ),
            (),
        )
        .await;
    let (mut rcfg, con_detraccion) = match completa {
        Ok(filas) => (filas, true),
        Err(_) => (
            conn.query(&format!("SELECT {} FROM configuracion_tienda LIMIT 1", COLUMNAS_EMISION), ())
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
            false,
        ),
    };

    let mut cfg_detraccion_leida = None;
    let (token, ruta, ruc_emisor, serie_boleta, serie_factura, codigo_sunat_cfg): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = match rcfg.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        Some(row) => {
            if con_detraccion {
                cfg_detraccion_leida = Some(crate::handlers::detraccion::desde_columnas(
                    row.get::<String>(6).ok(),
                    row.get::<f64>(7).ok(),
                    row.get::<String>(8).ok(),
                    row.get::<f64>(9).ok(),
                    row.get::<String>(10).ok(),
                ));
            }
            (row.get(0).ok(), row.get(1).ok(), row.get(2).ok(), row.get(3).ok(), row.get(4).ok(), row.get(5).ok())
        }
        None => (None, None, None, None, None, None),
    };
    drop(rcfg);

    let serie_boleta = serie_boleta.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "B001".to_string());
    let serie_factura = serie_factura.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "F001".to_string());
    let codigo_sunat = codigo_sunat_cfg
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| CODIGO_PRODUCTO_SUNAT_RESPALDO.to_string());

    let (token, ruta) = match (token, ruta) {
        (Some(t), Some(r)) if !t.trim().is_empty() && !r.trim().is_empty() => (t, r),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Falta configurar el Token y la URL de FacturaLibre en Configuración antes de emitir comprobantes.".into(),
            ))
        }
    };

    let tipo_doc_cliente = if payload.tipo == "FACTURA" {
        Some("RUC".to_string())
    } else {
        payload.cliente_documento.as_ref().map(|d| {
            if d.len() == 11 { "RUC".to_string() } else { "DNI".to_string() }
        })
    };

    // Detracción: solo facturas de un negocio que la tiene encendida y con
    // su cuenta configurada, cuando el total supera el mínimo.
    let cfg_detraccion = match cfg_detraccion_leida {
        Some(cfg) => cfg,
        None => crate::handlers::detraccion::configuracion(&conn).await,
    };
    let detraccion = crate::logica::detraccion::calcular(&payload.tipo, total, &cfg_detraccion, payload.detraccion);

    let datos = DatosParaEmitir {
        tipo: payload.tipo.clone(),
        cliente_tipo_documento: tipo_doc_cliente.clone(),
        cliente_documento: payload.cliente_documento.clone(),
        cliente_nombre: payload.cliente_nombre.clone(),
        cliente_direccion,
        subtotal: base_sin_igv,
        igv,
        total,
        tasa: desglose.tasa,
        exoneradas: desglose.exoneradas,
        inafectas: desglose.inafectas,
        items,
        detraccion: detraccion.clone(),
        // Venta al crédito con saldo pendiente: la factura sale "al crédito".
        // (Solo la factura lleva forma de pago; en boleta no se consulta.)
        credito: if payload.tipo == "FACTURA" {
            crate::handlers::creditos::pendiente_de_venta(&conn, payload.venta_id).await
        } else {
            None
        },
    };

    let resultado = emitir_facturalibre(&datos, &token, &ruta, &codigo_sunat, &serie_boleta, &serie_factura).await;

    let numero = if resultado.numero > 0 {
        resultado.numero
    } else {
        let mut rn = conn
            .query(
                "SELECT COALESCE(MAX(numero),0)+1 FROM comprobantes_electronicos WHERE serie = ?1",
                libsql::params![resultado.serie.clone()],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match rn.next().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
            Some(row) => row.get(0).unwrap_or(1),
            None => 1,
        }
    };

    let estado = if resultado.aceptado { "ACEPTADO" } else { "RECHAZADO" };

    conn.execute(
        "INSERT INTO comprobantes_electronicos
            (venta_id, tipo, proveedor, serie, numero, cliente_documento, cliente_nombre, estado, mensaje_sunat, enlace_pdf, enlace_cdr, external_id, hash)
         VALUES (?1, ?2, 'FACTURALIBRE', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        libsql::params![
            payload.venta_id, payload.tipo.clone(), resultado.serie.clone(), numero,
            payload.cliente_documento.clone(), payload.cliente_nombre.clone(), estado, resultado.mensaje.clone(),
            resultado.enlace_pdf.clone(), resultado.enlace_cdr.clone(), resultado.external_id.clone(), resultado.hash.clone()
        ],
    ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al guardar comprobante: {}", e)))?;

    let comprobante_id = conn.last_insert_rowid();

    // El enlace del XML firmado va aparte y sin fallar: el comprobante ya
    // quedó registrado y nada de la emisión depende de este dato (solo la
    // descarga del XML desde Comprobantes).
    if let Some(enlace_xml) = resultado.enlace_xml.clone() {
        let _ = conn
            .execute(
                "UPDATE comprobantes_electronicos SET enlace_xml = ?1 WHERE id = ?2",
                libsql::params![enlace_xml, comprobante_id],
            )
            .await;
    }

    // Se guarda la detracción enviada para reimprimir el ticket igual. Va
    // aparte y sin fallar: una base sin la migración 0013 nunca llega aquí
    // con detracción, y si algo falla el comprobante ya quedó registrado.
    if let Some(d) = &detraccion {
        let _ = conn
            .execute(
                "UPDATE comprobantes_electronicos SET detraccion_porcentaje = ?1, detraccion_monto = ?2, detraccion_cuenta = ?3 WHERE id = ?4",
                libsql::params![d.porcentaje, d.monto, d.cuenta.clone(), comprobante_id],
            )
            .await;
    }

    let cliente_tipo_documento_codigo = match &tipo_doc_cliente {
        Some(t) => codigo_tipo_documento_identidad(t).to_string(),
        None => "0".to_string(),
    };
    let cliente_numero_documento = payload
        .cliente_documento
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "-".to_string());

    Ok(Json(ComprobanteResponse {
        success: resultado.aceptado,
        comprobante_id: Some(comprobante_id),
        tipo: payload.tipo,
        serie: resultado.serie,
        numero,
        estado: estado.to_string(),
        mensaje: resultado.mensaje,
        enlace_pdf: resultado.enlace_pdf,
        hash: resultado.hash,
        ruc_emisor,
        fecha_emision: Some(resultado.fecha_emision),
        igv,
        total_venta: total,
        igv_tasa: desglose.tasa,
        op_gravadas: desglose.gravadas,
        op_exoneradas: desglose.exoneradas,
        op_inafectas: desglose.inafectas,
        cliente_tipo_documento_codigo,
        cliente_numero_documento,
        detraccion_porcentaje: detraccion.as_ref().map(|d| d.porcentaje),
        detraccion_monto: detraccion.as_ref().map(|d| d.monto),
        detraccion_cuenta: detraccion.as_ref().map(|d| d.cuenta.clone()),
    }))
}