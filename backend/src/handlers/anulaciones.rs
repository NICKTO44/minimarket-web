//! Anular un comprobante emitido directo a SUNAT (lo pueden hacer el cajero
//! y el administrador).
//!
//! Anular es dejar la venta sin efecto: se devuelve lo que quedaba de la
//! venta con una devolución de siempre (vuelve el stock y sale el dinero
//! por el mismo medio, ver devoluciones.rs) y se pide a SUNAT la baja de la
//! factura o el resumen de anulación de la boleta. Solo dentro de los 7
//! días de la emisión; después se usa una nota de crédito.
//!
//! SUNAT responde con un ticket: la anulación queda EN_PROCESO hasta que el
//! ticket da la constancia (se consulta al momento, con el botón o en la
//! tarea automática de envios_sunat.rs).

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::envios_sunat::{dias_entre, Lycet};
use crate::logica::anulaciones::{self as logica, DIAS_PLAZO_ANULACION};
use crate::logica::sunat_directo::{EstadoEnvio, RespuestaSunat};
use crate::models::auth::Claims;
use crate::models::devolucion::{NuevaDevolucion, ProductoDevolver};
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(m: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, m.into())
}

fn sin_migracion<E>(_: E) -> Fallo {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

/// Un producto de la venta que todavía no se devolvió.
#[derive(Serialize, Debug, Clone)]
pub struct PorDevolver {
    pub detalle_id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub cantidad: f64,
    pub monto: f64,
}

/// Lo que se revisa antes de anular.
struct Revision {
    venta_id: i64,
    documento: String,
    tipo: &'static str,
    fecha_documento: String,
    original: Value,
    dias_restantes: i64,
    por_devolver: Vec<PorDevolver>,
    metodo_pago: String,
    pago_otro_metodo: Option<String>,
    /// Estado de anulación antes de empezar (None o RECHAZADA).
    anulacion_previa: Option<String>,
}

/// Revisa si el comprobante se puede anular. Err con el motivo si no.
async fn revisar(conn: &libsql::Connection, comprobante_id: i64) -> Result<Revision, Fallo> {
    let mut filas = conn
        .query(
            "SELECT ce.venta_id, ce.serie, ce.numero, ce.estado, ce.proveedor, ce.anulacion, a.documento,
                    v.metodo_pago, v.pago_otro_metodo
             FROM comprobantes_electronicos ce
             JOIN ventas v ON v.id = ce.venta_id
             LEFT JOIN comprobante_archivos a ON a.comprobante_id = ce.id
             WHERE ce.id = ?1",
            libsql::params![comprobante_id],
        )
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Comprobante no encontrado.".to_string()))?;
    let venta_id: i64 = f.get(0).unwrap_or_default();
    let documento = format!("{}-{}", f.get::<String>(1).unwrap_or_default(), f.get::<i64>(2).unwrap_or_default());
    let estado: String = f.get(3).unwrap_or_default();
    let proveedor: String = f.get(4).unwrap_or_default();
    let anulacion: Option<String> = f.get(5).ok();
    let original: Option<Value> = f.get::<String>(6).ok().and_then(|t| serde_json::from_str(&t).ok());
    let metodo_pago: String = f.get(7).unwrap_or_default();
    let pago_otro_metodo: Option<String> = f.get(8).ok();
    drop(filas);

    if proveedor != "SUNAT_DIRECTO" {
        return Err(malo("Solo se anulan aquí los comprobantes emitidos directo a SUNAT."));
    }
    match anulacion.as_deref() {
        Some("ANULADO") => return Err(malo(format!("{} ya está anulado.", documento))),
        Some("EN_PROCESO") => return Err(malo(format!("La anulación de {} ya se pidió; SUNAT la está procesando.", documento))),
        Some("VERIFICAR") => {
            return Err(malo(format!(
                "No se sabe si la anulación de {} llegó a SUNAT: revísala en SUNAT Operaciones en Línea.",
                documento
            )))
        }
        _ => {}
    }
    if estado != "ACEPTADO" {
        return Err(malo(format!("{} no fue aceptado por SUNAT: no hay nada que anular.", documento)));
    }
    let original = original.ok_or_else(|| interno(format!("{} no tiene su documento guardado.", documento)))?;
    let fecha_documento = logica::fecha_documento(&original);
    let transcurridos = dias_entre(&fecha_documento, &crate::logica::tiempo::hoy_lima()).unwrap_or(0);
    let dias_restantes = DIAS_PLAZO_ANULACION - transcurridos;
    if dias_restantes < 0 {
        return Err(malo(format!(
            "Pasaron más de {} días desde la emisión de {}: ya no se puede anular. Usa una nota de crédito.",
            DIAS_PLAZO_ANULACION, documento
        )));
    }
    if let Ok(mut n) = conn
        .query(
            "SELECT COUNT(*) FROM notas_credito WHERE comprobante_id = ?1 AND estado IN ('ACEPTADO', 'PENDIENTE')",
            libsql::params![comprobante_id],
        )
        .await
    {
        if let Ok(Some(f)) = n.next().await {
            if f.get::<i64>(0).unwrap_or(0) > 0 {
                return Err(malo(format!(
                    "{} ya tiene nota de crédito: no se puede anular. Para lo que falta, emite otra nota de crédito.",
                    documento
                )));
            }
        }
    }

    // Lo que queda de la venta (lo vendido menos lo ya devuelto).
    let mut por_devolver = Vec::new();
    let mut filas = conn
        .query(
            "SELECT dv.id, dv.producto_id, COALESCE(dv.nombre_producto, p.nombre), CAST(dv.cantidad AS REAL),
                    CAST(dv.precio_unitario AS REAL),
                    CAST(COALESCE((SELECT SUM(dd.cantidad_devuelta) FROM detalles_devolucion dd
                                    JOIN devoluciones d ON d.id = dd.devolucion_id
                                   WHERE dd.detalle_venta_id = dv.id AND d.estado = 'PROCESADA'), 0) AS REAL)
             FROM detalles_venta dv JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1 ORDER BY dv.id",
            libsql::params![venta_id],
        )
        .await
        .map_err(interno)?;
    while let Some(f) = filas.next().await.map_err(interno)? {
        let cantidad = f.get::<f64>(3).unwrap_or(0.0) - f.get::<f64>(5).unwrap_or(0.0);
        if cantidad > 1e-9 {
            let precio = f.get::<f64>(4).unwrap_or(0.0);
            por_devolver.push(PorDevolver {
                detalle_id: f.get(0).unwrap_or_default(),
                producto_id: f.get(1).unwrap_or_default(),
                nombre: f.get(2).unwrap_or_default(),
                cantidad,
                monto: (cantidad * precio * 100.0).round() / 100.0,
            });
        }
    }

    Ok(Revision {
        venta_id,
        tipo: logica::tipo_anulacion(&documento),
        documento,
        fecha_documento,
        original,
        dias_restantes,
        por_devolver,
        metodo_pago,
        pago_otro_metodo,
        anulacion_previa: anulacion,
    })
}

#[derive(Serialize)]
pub struct InfoAnulacion {
    pub puede: bool,
    /// Por qué no se puede (si no se puede).
    pub motivo_no: Option<String>,
    pub documento: Option<String>,
    /// "BAJA" (factura) o "RESUMEN" (boleta).
    pub tipo: Option<String>,
    pub dias_restantes: Option<i64>,
    /// Lo que se devolverá al anular (stock y dinero).
    pub por_devolver: Vec<PorDevolver>,
    pub monto_devolver: f64,
    pub metodo_pago: Option<String>,
}

/// GET /comprobantes/:id/anulacion — si se puede anular y qué se devuelve.
pub async fn preparar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<InfoAnulacion>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    Ok(Json(match revisar(&conn, id).await {
        Ok(r) => InfoAnulacion {
            puede: true,
            motivo_no: None,
            documento: Some(r.documento),
            tipo: Some(r.tipo.to_string()),
            dias_restantes: Some(r.dias_restantes),
            monto_devolver: (r.por_devolver.iter().map(|p| p.monto).sum::<f64>() * 100.0).round() / 100.0,
            por_devolver: r.por_devolver,
            metodo_pago: Some(r.metodo_pago),
        },
        Err((codigo, motivo)) if codigo == StatusCode::BAD_REQUEST => InfoAnulacion {
            puede: false,
            motivo_no: Some(motivo),
            documento: None,
            tipo: None,
            dias_restantes: None,
            por_devolver: Vec::new(),
            monto_devolver: 0.0,
            metodo_pago: None,
        },
        Err(e) => return Err(e),
    }))
}

#[derive(Deserialize)]
pub struct PedidoAnulacion {
    pub motivo: String,
    /// Solo ventas MIXTO: cómo se devuelve el dinero ("EFECTIVO" o el otro medio).
    #[serde(default)]
    pub metodo_reembolso: Option<String>,
}

#[derive(Serialize)]
pub struct ResultadoAnulacion {
    /// Estado de la anulación: EN_PROCESO, ANULADO o RECHAZADA.
    pub anulacion: String,
    pub mensaje: String,
    pub identificador: String,
    pub folio_devolucion: Option<String>,
}

/// Reserva el identificador del día (RA-AAAAMMDD-n / RC-AAAAMMDD-n) con la fila.
#[allow(clippy::too_many_arguments)]
async fn reservar(
    conn: &libsql::Connection,
    tipo: &str,
    comprobante_id: i64,
    fecha_documento: &str,
    motivo: &str,
    devolucion_id: Option<i64>,
    usuario_id: i64,
    ahora: &str,
) -> Result<(i64, String), Fallo> {
    let prefijo = logica::prefijo_identificador(tipo, &ahora[..10]);
    let mut ultimo = String::new();
    for _ in 0..5 {
        let mut filas = match conn
            .query(
                "INSERT INTO bajas_sunat (tipo, identificador, comprobante_id, fecha_documento, motivo, estado, mensaje, devolucion_id, usuario_id, fecha)
                 SELECT ?1, ?2 || (COALESCE(MAX(CAST(substr(identificador, length(?2) + 1) AS INTEGER)), 0) + 1),
                        ?3, ?4, ?5, 'PENDIENTE', 'Enviando a SUNAT...', ?6, ?7, ?8
                 FROM bajas_sunat WHERE identificador LIKE ?2 || '%'
                 RETURNING id, identificador",
                libsql::params![tipo, prefijo.clone(), comprobante_id, fecha_documento, motivo, devolucion_id, usuario_id, ahora],
            )
            .await
        {
            Ok(f) => f,
            Err(e) => {
                ultimo = e.to_string();
                if ultimo.to_uppercase().contains("UNIQUE") {
                    continue;
                }
                break;
            }
        };
        let f = filas.next().await.map_err(interno)?.ok_or_else(|| interno("No se pudo reservar la anulación"))?;
        let datos = (f.get(0).map_err(interno)?, f.get(1).map_err(interno)?);
        while filas.next().await.map_err(interno)?.is_some() {}
        return Ok(datos);
    }
    Err(interno(format!("No se pudo registrar la anulación: {}", ultimo)))
}

/// POST /comprobantes/:id/anular
pub async fn anular(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(pedido): Json<PedidoAnulacion>,
) -> Result<Json<ResultadoAnulacion>, Fallo> {
    let motivo = pedido.motivo.trim().to_string();
    if motivo.chars().count() < 3 {
        return Err(malo("Escribe el motivo de la anulación."));
    }
    let lycet = Lycet::exigir()?;
    let conn = tenant.0.connect().map_err(interno)?;
    let r = revisar(&conn, id).await?;

    // Se toma el comprobante antes de devolver nada: dos personas (o dos
    // pestañas) no pueden anular la misma venta a la vez y devolverla dos veces.
    let tomado = conn
        .execute(
            "UPDATE comprobantes_electronicos SET anulacion = 'EN_PROCESO'
             WHERE id = ?1 AND (anulacion IS NULL OR anulacion = 'RECHAZADA')",
            libsql::params![id],
        )
        .await
        .map_err(interno)?;
    if tomado == 0 {
        return Err((StatusCode::CONFLICT, format!("{} ya se está anulando.", r.documento)));
    }
    let soltar = || async {
        let _ = conn
            .execute(
                "UPDATE comprobantes_electronicos SET anulacion = ?1 WHERE id = ?2",
                libsql::params![r.anulacion_previa.clone(), id],
            )
            .await;
    };

    // 1. La venta queda sin efecto: vuelve el stock y sale el dinero.
    let folio_devolucion = if r.por_devolver.is_empty() {
        None
    } else {
        // Venta al crédito: lo devuelto se descuenta de la deuda.
        let metodo = if r.metodo_pago == "MIXTO" && r.pago_otro_metodo.as_deref() == Some(crate::handlers::creditos::METODO_CREDITO) {
            Some(crate::handlers::creditos::METODO_CREDITO.to_string())
        } else {
            pedido.metodo_reembolso.clone()
        };
        let devolucion = NuevaDevolucion {
            venta_id: r.venta_id,
            productos: r
                .por_devolver
                .iter()
                .map(|p| ProductoDevolver { detalle_id: p.detalle_id, producto_id: p.producto_id, cantidad: p.cantidad })
                .collect(),
            motivo: format!("Anulación de {}: {}", r.documento, motivo).chars().take(250).collect(),
            usuario_id: claims.sub,
            metodo_reembolso: metodo,
        };
        match super::devoluciones::registrar_devolucion(&conn, claims.sub, &devolucion).await {
            Ok(d) => Some(d),
            Err(e) => {
                soltar().await;
                return Err(e);
            }
        }
    };

    // 2. El pedido de anulación a SUNAT.
    let ahora = crate::logica::tiempo::ahora_lima();
    let (hoy, hora) = ahora.split_once(' ').unwrap_or((ahora.as_str(), "00:00:00"));
    let (baja_id, identificador) =
        match reservar(&conn, r.tipo, id, &r.fecha_documento, &motivo, folio_devolucion.as_ref().map(|d| d.0), claims.sub, &ahora).await {
            Ok(b) => b,
            Err((codigo, e)) => {
                soltar().await;
                let devuelta = folio_devolucion.as_ref().map(|d| format!(" La venta ya se devolvió ({}): emite una nota de crédito por el total.", d.1));
                return Err((codigo, format!("{}{}", e, devuelta.unwrap_or_default())));
            }
        };
    let documento = if r.tipo == "BAJA" {
        logica::armar_baja(&r.original, &identificador, hoy, hora, &motivo)
    } else {
        logica::armar_resumen_anulacion(&r.original, &identificador, hoy, hora)
    };
    conn.execute("UPDATE bajas_sunat SET documento = ?1 WHERE id = ?2", libsql::params![documento.to_string(), baja_id])
        .await
        .map_err(interno)?;

    let (anulacion, mensaje) = avanzar(&conn, &lycet, baja_id, false).await?;
    // SUNAT suele procesar el ticket en segundos: una consulta más. Solo con
    // ticket: sin él, la anulación se reenvía después (tarea automática).
    let con_ticket = match conn
        .query("SELECT 1 FROM bajas_sunat WHERE id = ?1 AND ticket IS NOT NULL AND ticket <> ''", libsql::params![baja_id])
        .await
    {
        Ok(mut f) => matches!(f.next().await, Ok(Some(_))),
        Err(_) => false,
    };
    let (anulacion, mensaje) = if anulacion == "EN_PROCESO" && con_ticket {
        tokio::time::sleep(Duration::from_secs(3)).await;
        avanzar(&conn, &lycet, baja_id, false).await.unwrap_or((anulacion, mensaje))
    } else {
        (anulacion, mensaje)
    };
    Ok(Json(ResultadoAnulacion { anulacion, mensaje, identificador, folio_devolucion: folio_devolucion.map(|d| d.1) }))
}

/// Un paso de una anulación pendiente: si no tiene ticket, la envía (el
/// mismo documento); si lo tiene, lo consulta. Devuelve el estado de la
/// anulación del comprobante (EN_PROCESO, ANULADO o RECHAZADA) y el mensaje.
pub async fn avanzar(conn: &libsql::Connection, lycet: &Lycet, baja_id: i64, es_reintento: bool) -> Result<(String, String), Fallo> {
    let mut filas = conn
        .query(
            "SELECT tipo, comprobante_id, ticket, estado, documento, identificador, devolucion_id, usuario_id
             FROM bajas_sunat WHERE id = ?1",
            libsql::params![baja_id],
        )
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Anulación no encontrada.".to_string()))?;
    let tipo: String = f.get(0).unwrap_or_default();
    let comprobante_id: i64 = f.get(1).unwrap_or_default();
    let ticket: Option<String> = f.get::<String>(2).ok().filter(|t| !t.trim().is_empty());
    let estado: String = f.get(3).unwrap_or_default();
    let documento: Value = f
        .get::<String>(4)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| interno("La anulación no tiene su documento guardado."))?;
    let identificador: String = f.get(5).unwrap_or_default();
    let devolucion_id: Option<i64> = f.get(6).ok();
    let usuario_id: Option<i64> = f.get(7).ok();
    drop(filas);
    // Migración 0027: si un envío anterior se cortó sin respuesta.
    let incierto_previo = match conn.query("SELECT envio_incierto FROM bajas_sunat WHERE id = ?1", libsql::params![baja_id]).await {
        Ok(mut f) => matches!(f.next().await, Ok(Some(fila)) if fila.get::<i64>(0).unwrap_or(0) == 1),
        Err(_) => false,
    };
    if estado != "PENDIENTE" {
        return Err((StatusCode::CONFLICT, format!("La anulación {} ya terminó ({}).", identificador, estado)));
    }
    // Reenviarla (sin ticket) la toma primero: el botón y la tarea
    // automática no la mandan dos veces a la vez.
    if es_reintento
        && ticket.is_none()
        && !super::envios_sunat::reclamar(
            conn,
            "bajas_sunat",
            "estado = 'PENDIENTE' AND (ticket IS NULL OR ticket = '')",
            "fecha",
            "mensaje",
            baja_id,
        )
        .await?
    {
        return Err(super::envios_sunat::en_curso(&format!("La anulación {}", identificador)));
    }
    let ruta = logica::prefijo_y_ruta(&tipo).1;
    let ruc = documento["company"]["ruc"].as_str().unwrap_or("").to_string();
    let ahora = crate::logica::tiempo::ahora_lima();

    let (ticket, mut respuesta): (Option<String>, RespuestaSunat) = match ticket {
        Some(t) => {
            let r = lycet.consultar_ticket(ruta, &ruc, &t).await;
            (Some(t), r)
        }
        None => {
            let envio = lycet.enviar_con_ticket(ruta, &documento).await;
            if let Some(t) = &envio.ticket {
                conn.execute(
                    "UPDATE bajas_sunat SET ticket = ?1, xml = COALESCE(?2, xml) WHERE id = ?3 AND (ticket IS NULL OR ticket = '')",
                    libsql::params![t.clone(), envio.respuesta.xml.clone(), baja_id],
                )
                .await
                .map_err(interno)?;
            }
            (envio.ticket, envio.respuesta)
        }
    };

    if es_reintento || ticket.is_none() {
        let _ = conn
            .execute(
                "UPDATE bajas_sunat SET intentos = intentos + 1, ultimo_intento = ?1 WHERE id = ?2",
                libsql::params![ahora, baja_id],
            )
            .await;
    }
    if respuesta.incierto {
        let _ = conn.execute("UPDATE bajas_sunat SET envio_incierto = 1 WHERE id = ?1", libsql::params![baja_id]).await;
    }
    // Un envío anterior pudo llegar a SUNAT: si ahora SUNAT rechaza el
    // reenvío (por ejemplo, por repetido), no se sabe si la anulación quedó.
    // No se da por rechazada ni se emite la nota: hay que revisarla en SUNAT.
    let verificar = ticket.is_none()
        && incierto_previo
        && matches!(respuesta.estado, EstadoEnvio::Rechazado | EstadoEnvio::Error);
    if verificar {
        respuesta.mensaje = format!(
            "No se sabe si la anulación llegó a SUNAT (un envío anterior se cortó y el reenvío respondió: {}). Revísala en SUNAT Operaciones en Línea antes de emitir una nota de crédito.",
            respuesta.mensaje
        );
    }
    let estado_baja = if verificar { "VERIFICAR" } else { respuesta.estado.como_texto() };
    // Solo si sigue pendiente (un resultado definitivo no se pisa), y un
    // rechazo al enviar no reemplaza a un envío que sí obtuvo su ticket.
    let cambiadas = conn
        .execute(
            "UPDATE bajas_sunat SET estado = ?1, mensaje = ?2, cdr_zip = COALESCE(?3, cdr_zip)
             WHERE id = ?4 AND estado = 'PENDIENTE' AND (?5 = 1 OR ticket IS NULL OR ticket = '' OR ?1 = 'PENDIENTE')",
            libsql::params![
                estado_baja,
                respuesta.mensaje.clone(),
                respuesta.cdr_zip.clone(),
                baja_id,
                i64::from(ticket.is_some())
            ],
        )
        .await
        .map_err(interno)?;
    let anulacion = match respuesta.estado {
        _ if verificar => "VERIFICAR",
        EstadoEnvio::Aceptado => "ANULADO",
        EstadoEnvio::Rechazado | EstadoEnvio::Error => "RECHAZADA",
        EstadoEnvio::Pendiente => "EN_PROCESO",
    };
    if cambiadas == 0 {
        // Otro envío ya dejó el resultado: se informa lo que quedó guardado.
        let mut f = conn
            .query(
                "SELECT COALESCE(ce.anulacion, 'EN_PROCESO'), COALESCE(b.mensaje, '')
                 FROM bajas_sunat b JOIN comprobantes_electronicos ce ON ce.id = b.comprobante_id WHERE b.id = ?1",
                libsql::params![baja_id],
            )
            .await
            .map_err(interno)?;
        return Ok(match f.next().await.map_err(interno)? {
            Some(fila) => (fila.get(0).unwrap_or_default(), fila.get(1).unwrap_or_default()),
            None => ("EN_PROCESO".to_string(), String::new()),
        });
    }
    conn.execute(
        "UPDATE comprobantes_electronicos SET anulacion = ?1 WHERE id = ?2",
        libsql::params![anulacion, comprobante_id],
    )
    .await
    .map_err(interno)?;

    let mensaje = match respuesta.estado {
        _ if verificar => respuesta.mensaje,
        EstadoEnvio::Aceptado => format!("SUNAT aceptó la anulación ({}).", identificador),
        EstadoEnvio::Pendiente if ticket.is_some() => "SUNAT está procesando la anulación. Se consulta sola en unos minutos.".to_string(),
        EstadoEnvio::Pendiente => respuesta.mensaje,
        // SUNAT no aceptó la baja, pero la venta ya se devolvió (stock y
        // dinero): la nota de crédito por el total la deja sin efecto.
        EstadoEnvio::Rechazado | EstadoEnvio::Error => {
            let motivo = format!("Anulación de la operación ({})", identificador);
            match super::notas_credito::emitir_nota(conn, lycet, comprobante_id, "01", &motivo, &[], devolucion_id, usuario_id).await {
                Ok(n) => format!(
                    "SUNAT no aceptó la anulación ({}). Como la venta ya se devolvió, se emitió la nota de crédito {}-{} por S/ {:.2} ({}).",
                    respuesta.mensaje,
                    n.serie,
                    n.numero,
                    n.total,
                    n.estado.to_lowercase()
                ),
                Err((_, e)) => format!(
                    "SUNAT no aceptó la anulación ({}). La venta ya se devolvió: emite una nota de crédito por el total desde Comprobantes. ({})",
                    respuesta.mensaje, e
                ),
            }
        }
    };
    Ok((anulacion.to_string(), mensaje))
}

/// POST /comprobantes/:id/anulacion/consultar — botón "Consultar" de una
/// anulación en proceso.
pub async fn consultar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<ResultadoAnulacion>, Fallo> {
    let lycet = Lycet::exigir()?;
    let conn = tenant.0.connect().map_err(interno)?;
    let mut filas = conn
        .query(
            "SELECT id, identificador FROM bajas_sunat WHERE comprobante_id = ?1 AND estado = 'PENDIENTE' ORDER BY id DESC LIMIT 1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::CONFLICT, "Este comprobante no tiene una anulación en proceso.".to_string()))?;
    let baja_id: i64 = f.get(0).map_err(interno)?;
    let identificador: String = f.get(1).unwrap_or_default();
    drop(filas);
    let (anulacion, mensaje) = avanzar(&conn, &lycet, baja_id, true).await?;
    Ok(Json(ResultadoAnulacion { anulacion, mensaje, identificador, folio_devolucion: None }))
}

/// Estado de anulación de varios comprobantes (vacío si la base aún no
/// tiene la migración 0025).
pub async fn anulaciones_de(conn: &libsql::Connection, comprobantes: &[i64]) -> HashMap<i64, String> {
    let mut mapa = HashMap::new();
    if comprobantes.is_empty() {
        return mapa;
    }
    let lista = comprobantes.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let sql = format!("SELECT id, anulacion FROM comprobantes_electronicos WHERE anulacion IS NOT NULL AND id IN ({})", lista);
    if let Ok(mut filas) = conn.query(&sql, ()).await {
        while let Ok(Some(f)) = filas.next().await {
            if let (Ok(id), Ok(a)) = (f.get::<i64>(0), f.get::<String>(1)) {
                mapa.insert(id, a);
            }
        }
    }
    mapa
}
