//! Notas de crédito de la emisión directa a SUNAT.
//!
//! Una nota corrige un comprobante (boleta o factura) que SUNAT ya aceptó:
//!   - desde Comprobantes, a mano (anulación, devolución total o por ítem);
//!   - sola, al registrar una devolución de una venta con comprobante;
//!   - en un cambio de prenda, si el cajero la pide.
//!
//! Igual que las boletas y facturas directas: el número se reserva antes de
//! enviar, el documento se guarda tal cual y, si SUNAT no responde, queda
//! PENDIENTE y se reenvía (botón o tarea automática, ver envios_sunat.rs).
//! El documento se arma en logica/notas_credito.rs.

use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::envios_sunat::{reenviar_documento, Lycet};
use crate::logica::notas_credito::{self as logica, LineaDisponible, LineaNota};
use crate::logica::sunat_directo::RespuestaSunat;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(m: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, m.into())
}

/// La base aún no tiene la migración 0024.
fn sin_migracion<E>(_: E) -> Fallo {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

/// El comprobante que se corrige.
struct Original {
    venta_id: i64,
    serie: String,
    numero: i64,
    documento: Value,
}

async fn comprobante_original(conn: &libsql::Connection, comprobante_id: i64) -> Result<Original, Fallo> {
    let mut filas = conn
        .query(
            "SELECT ce.venta_id, ce.serie, ce.numero, ce.estado, ce.proveedor, a.documento
             FROM comprobantes_electronicos ce
             LEFT JOIN comprobante_archivos a ON a.comprobante_id = ce.id
             WHERE ce.id = ?1",
            libsql::params![comprobante_id],
        )
        .await
        .map_err(interno)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Comprobante no encontrado.".to_string()))?;
    let serie: String = f.get(1).unwrap_or_default();
    let numero: i64 = f.get(2).unwrap_or_default();
    let estado: String = f.get(3).unwrap_or_default();
    let proveedor: String = f.get(4).unwrap_or_default();
    if proveedor != "SUNAT_DIRECTO" {
        return Err(malo("Las notas de crédito se emiten aquí solo para comprobantes emitidos directo a SUNAT."));
    }
    if estado == "PENDIENTE" {
        return Err(malo(format!(
            "{}-{} todavía no llega a SUNAT. Cuando quede aceptado se podrá emitir su nota de crédito.",
            serie, numero
        )));
    }
    if estado != "ACEPTADO" {
        return Err(malo(format!("{}-{} no fue aceptado por SUNAT: no necesita nota de crédito.", serie, numero)));
    }
    // Anulado o con la anulación en proceso (migración 0025; sin ella, no hay).
    if let Ok(mut a) = conn
        .query("SELECT anulacion FROM comprobantes_electronicos WHERE id = ?1", libsql::params![comprobante_id])
        .await
    {
        if let Ok(Some(fa)) = a.next().await {
            match fa.get::<String>(0).ok().as_deref() {
                Some("ANULADO") => return Err(malo(format!("{}-{} está anulado: no lleva nota de crédito.", serie, numero))),
                Some("EN_PROCESO") => {
                    return Err(malo(format!("{}-{} tiene una anulación en proceso con SUNAT.", serie, numero)))
                }
                _ => {}
            }
        }
    }
    let documento = f
        .get::<String>(5)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| interno(format!("{}-{} no tiene su documento guardado.", serie, numero)))?;
    Ok(Original { venta_id: f.get(0).unwrap_or_default(), serie, numero, documento })
}

/// Notas que cuentan para el comprobante (todas menos las rechazadas o con
/// error, que para SUNAT no existen): (documento, total).
async fn notas_vigentes(conn: &libsql::Connection, comprobante_id: i64) -> Result<Vec<(Value, f64)>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT documento, total FROM notas_credito
             WHERE comprobante_id = ?1 AND estado IN ('ACEPTADO', 'PENDIENTE')",
            libsql::params![comprobante_id],
        )
        .await
        .map_err(sin_migracion)?;
    let mut notas = Vec::new();
    while let Some(f) = filas.next().await.map_err(interno)? {
        let doc = f.get::<String>(0).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        notas.push((doc, f.get::<f64>(1).unwrap_or(0.0)));
    }
    Ok(notas)
}

/// Serie de la nota según el comprobante que corrige.
async fn serie_nota(conn: &libsql::Connection, serie_original: &str) -> Result<String, Fallo> {
    let de_factura = serie_original.trim().to_uppercase().starts_with('F');
    let mut filas = conn
        .query("SELECT serie_nc_boleta, serie_nc_factura FROM configuracion_tienda LIMIT 1", ())
        .await
        .map_err(sin_migracion)?;
    let (boleta, factura): (Option<String>, Option<String>) = match filas.next().await.map_err(interno)? {
        Some(f) => (f.get(0).ok(), f.get(1).ok()),
        None => (None, None),
    };
    let (elegida, letra, por_defecto) = if de_factura { (factura, 'F', "FC01") } else { (boleta, 'B', "BC01") };
    let serie = elegida.map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).unwrap_or_else(|| por_defecto.to_string());
    if !serie.starts_with(letra) || serie.len() != 4 {
        return Err(malo(format!(
            "La serie de notas de crédito {} no es válida: debe tener 4 caracteres y empezar con {}.",
            serie, letra
        )));
    }
    Ok(serie)
}

/// Reserva el siguiente número de la serie de notas (índice único).
async fn reservar_numero(
    conn: &libsql::Connection,
    comprobante_id: i64,
    venta_id: i64,
    devolucion_id: Option<i64>,
    serie: &str,
    codigo: &str,
    motivo: &str,
    usuario_id: Option<i64>,
    ahora: &str,
) -> Result<(i64, i64), Fallo> {
    let mut ultimo_error = String::new();
    for _ in 0..5 {
        let mut filas = match conn
            .query(
                "INSERT INTO notas_credito
                    (comprobante_id, venta_id, devolucion_id, serie, numero, motivo_codigo, motivo, total, estado, mensaje_sunat, usuario_id, fecha_emision)
                 SELECT ?1, ?2, ?3, ?4, COALESCE(MAX(numero), 0) + 1, ?5, ?6, 0, 'PENDIENTE', 'Enviando a SUNAT...', ?7, ?8
                 FROM notas_credito WHERE serie = ?4
                 RETURNING id, numero",
                libsql::params![comprobante_id, venta_id, devolucion_id, serie, codigo, motivo, usuario_id, ahora],
            )
            .await
        {
            Ok(f) => f,
            Err(e) => {
                ultimo_error = e.to_string();
                if ultimo_error.to_uppercase().contains("UNIQUE") {
                    continue;
                }
                break;
            }
        };
        let fila = filas.next().await.map_err(interno)?.ok_or_else(|| interno("No se pudo leer el número reservado"))?;
        let datos = (fila.get(0).map_err(interno)?, fila.get(1).map_err(interno)?);
        while filas.next().await.map_err(interno)?.is_some() {}
        return Ok(datos);
    }
    Err(interno(format!("No se pudo reservar el número de la nota de crédito: {}", ultimo_error)))
}

/// Guarda el resultado de un envío de nota.
pub async fn guardar_resultado(conn: &libsql::Connection, nota_id: i64, r: &RespuestaSunat, es_reintento: bool) -> Result<(), Fallo> {
    // Solo si sigue pendiente: un resultado definitivo de otro envío no se pisa.
    conn.execute(
        "UPDATE notas_credito SET estado = ?1, mensaje_sunat = ?2, hash = COALESCE(?3, hash),
                xml = COALESCE(?4, xml), cdr_zip = COALESCE(?5, cdr_zip) WHERE id = ?6 AND estado = 'PENDIENTE'",
        libsql::params![r.estado.como_texto(), r.mensaje.clone(), r.hash.clone(), r.xml.clone(), r.cdr_zip.clone(), nota_id],
    )
    .await
    .map_err(interno)?;
    if r.incierto {
        conn.execute("UPDATE notas_credito SET envio_incierto = 1 WHERE id = ?1", libsql::params![nota_id])
            .await
            .map_err(interno)?;
    }
    if es_reintento {
        conn.execute(
            "UPDATE notas_credito SET intentos = intentos + 1, ultimo_intento = ?1 WHERE id = ?2",
            libsql::params![crate::logica::tiempo::ahora_lima(), nota_id],
        )
        .await
        .map_err(interno)?;
    }
    Ok(())
}

#[derive(Serialize, Debug, Clone)]
pub struct NotaEmitida {
    pub id: i64,
    pub serie: String,
    pub numero: i64,
    pub estado: String,
    pub mensaje: String,
    pub total: f64,
    /// Comprobante que corrige ("BM01-15").
    pub documento_afectado: String,
    /// Valor resumen del XML firmado (va en el QR impreso).
    pub hash: Option<String>,
}

/// Emite una nota de crédito para un comprobante directo aceptado.
#[allow(clippy::too_many_arguments)]
pub async fn emitir_nota(
    conn: &libsql::Connection,
    lycet: &Lycet,
    comprobante_id: i64,
    codigo: &str,
    motivo: &str,
    lineas: &[LineaNota],
    devolucion_id: Option<i64>,
    usuario_id: Option<i64>,
) -> Result<NotaEmitida, Fallo> {
    // Revisar lo que queda por acreditar y reservar el número van juntos en
    // una transacción: dos notas a la vez (la de una devolución y una a
    // mano, o dos pestañas) no pueden acreditar lo mismo dos veces.
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(interno)?;
    let preparado: Result<(i64, i64, Value, f64, String, String), Fallo> = async {
        let c: &libsql::Connection = &tx;
        let original = comprobante_original(c, comprobante_id).await?;
        let afectado = format!("{}-{}", original.serie, original.numero);
        let previas = notas_vigentes(c, comprobante_id).await?;
        if previas.iter().any(|(d, _)| d.is_null()) {
            return Err(malo(format!("{} tiene una nota de crédito incompleta: revísala en Comprobantes.", afectado)));
        }
        if logica::es_total(codigo) && !previas.is_empty() {
            return Err(malo(format!(
                "{} ya tiene nota de crédito. Para lo que falta, usa \"Devolución por ítem\".",
                afectado
            )));
        }
        if !logica::es_total(codigo) {
            let docs: Vec<Value> = previas.iter().map(|(d, _)| d.clone()).collect();
            let disponibles = logica::lineas_disponibles(&original.documento, &docs);
            for l in lineas {
                let d = disponibles.get(l.indice).ok_or_else(|| malo("Una de las líneas no es del comprobante."))?;
                if l.cantidad > d.disponible + 1e-9 {
                    return Err(malo(format!(
                        "De \"{}\" solo queda {} por acreditar (lo demás ya tiene nota de crédito).",
                        d.descripcion, d.disponible
                    )));
                }
            }
        }
        let codigo_ok = logica::nombre_motivo(codigo).ok_or_else(|| malo("Elige el motivo de la nota de crédito."))?;
        let motivo = {
            let m = motivo.trim();
            if m.is_empty() { codigo_ok.to_string() } else { m.chars().take(250).collect::<String>() }
        };
        let serie = serie_nota(c, &original.serie).await?;
        let ahora = crate::logica::tiempo::ahora_lima();
        let (fecha, hora) = ahora.split_once(' ').unwrap_or((ahora.as_str(), "00:00:00"));

        // Se arma primero con un número de prueba: si algo no cuadra, no se
        // gasta un número de la serie.
        logica::armar_nota(&original.documento, &serie, 1, fecha, hora, codigo, &motivo, lineas).map_err(malo)?;

        let (id, numero) =
            reservar_numero(c, comprobante_id, original.venta_id, devolucion_id, &serie, codigo, &motivo, usuario_id, &ahora).await?;
        let documento = logica::armar_nota(&original.documento, &serie, numero, fecha, hora, codigo, &motivo, lineas).map_err(malo)?;
        let total = logica::total_de(&documento);
        c.execute(
            "UPDATE notas_credito SET documento = ?1, total = ?2 WHERE id = ?3",
            libsql::params![documento.to_string(), total, id],
        )
        .await
        .map_err(interno)?;
        Ok((id, numero, documento, total, serie, afectado))
    }
    .await;
    let (id, numero, documento, total, serie, afectado) = match preparado {
        Ok(datos) => {
            tx.commit().await.map_err(interno)?;
            datos
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    };

    let r = lycet.enviar("note", &documento).await;
    guardar_resultado(conn, id, &r, false).await?;
    Ok(NotaEmitida {
        id,
        serie,
        numero,
        estado: r.estado.como_texto().to_string(),
        mensaje: r.mensaje,
        total,
        documento_afectado: afectado,
        hash: r.hash,
    })
}

/// Resultado de la nota automática de una devolución o un cambio.
#[derive(Serialize, Debug, Clone, Default)]
pub struct NotaDeDevolucion {
    pub nota: Option<NotaEmitida>,
    /// Por qué no se emitió (o qué pasó), para mostrárselo al cajero.
    pub aviso: Option<String>,
}

/// Comprobante directo aceptado de una venta (id, "BM01-15"), si lo tiene.
pub async fn comprobante_directo_de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<(i64, String, String)> {
    let mut filas = conn
        .query(
            "SELECT id, serie || '-' || numero, estado FROM comprobantes_electronicos
             WHERE venta_id = ?1 AND proveedor = 'SUNAT_DIRECTO' AND estado IN ('ACEPTADO', 'PENDIENTE')
             ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .ok()?;
    let f = filas.next().await.ok()??;
    Some((f.get(0).ok()?, f.get(1).ok()?, f.get(2).ok()?))
}

/// Emite la nota de crédito de lo devuelto en una devolución (o en el
/// cambio de prenda) si la venta tiene un comprobante directo. None si la
/// venta no tiene comprobante directo (FacturaLibre o nota simple): no hay
/// nada que hacer.
pub async fn emitir_por_devolucion(
    conn: &libsql::Connection,
    devolucion_id: i64,
    usuario_id: Option<i64>,
) -> Option<NotaDeDevolucion> {
    let (venta_id, motivo): (i64, String) = {
        let mut filas = conn
            .query("SELECT venta_original_id, motivo FROM devoluciones WHERE id = ?1", libsql::params![devolucion_id])
            .await
            .ok()?;
        let f = filas.next().await.ok()??;
        (f.get(0).ok()?, f.get::<String>(1).unwrap_or_default())
    };
    let (comprobante_id, afectado, estado) = comprobante_directo_de_venta(conn, venta_id).await?;
    if estado == "PENDIENTE" {
        return Some(NotaDeDevolucion {
            nota: None,
            aviso: Some(format!(
                "{} todavía no llega a SUNAT. Cuando quede aceptado, emite su nota de crédito desde Comprobantes.",
                afectado
            )),
        });
    }
    let Some(lycet) = Lycet::desde_env() else {
        return Some(NotaDeDevolucion {
            nota: None,
            aviso: Some("La emisión directa no está configurada en el servidor: no se emitió la nota de crédito.".into()),
        });
    };

    // Posición de cada línea de la venta (el comprobante sigue el mismo orden).
    let mut posiciones: HashMap<i64, usize> = HashMap::new();
    if let Ok(mut filas) = conn.query("SELECT id FROM detalles_venta WHERE venta_id = ?1 ORDER BY id", libsql::params![venta_id]).await {
        let mut i = 0;
        while let Ok(Some(f)) = filas.next().await {
            if let Ok(id) = f.get::<i64>(0) {
                posiciones.insert(id, i);
            }
            i += 1;
        }
    }
    let mut lineas: Vec<LineaNota> = Vec::new();
    if let Ok(mut filas) = conn
        .query(
            "SELECT detalle_venta_id, CAST(cantidad_devuelta AS REAL) FROM detalles_devolucion WHERE devolucion_id = ?1",
            libsql::params![devolucion_id],
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            if let (Ok(detalle), Ok(cantidad)) = (f.get::<i64>(0), f.get::<f64>(1)) {
                if let Some(&indice) = posiciones.get(&detalle) {
                    match lineas.iter_mut().find(|l| l.indice == indice) {
                        Some(l) => l.cantidad += cantidad,
                        None => lineas.push(LineaNota { indice, cantidad }),
                    }
                }
            }
        }
    }
    if lineas.is_empty() {
        return Some(NotaDeDevolucion { nota: None, aviso: Some("No se pudo relacionar lo devuelto con el comprobante.".into()) });
    }

    // Todo devuelto y sin notas previas = devolución total (06).
    let previas = notas_vigentes(conn, comprobante_id).await.unwrap_or_default();
    let todo = previas.is_empty()
        && comprobante_original(conn, comprobante_id).await.ok().is_some_and(|o| {
            logica::lineas_disponibles(&o.documento, &[])
                .iter()
                .all(|d| lineas.iter().any(|l| l.indice == d.indice && (l.cantidad - d.cantidad).abs() < 1e-9))
        });
    let codigo = if todo { "06" } else { "07" };
    let motivo = if motivo.trim().is_empty() { logica::nombre_motivo(codigo).unwrap_or_default().to_string() } else { motivo };

    Some(match emitir_nota(conn, &lycet, comprobante_id, codigo, &motivo, &lineas, Some(devolucion_id), usuario_id).await {
        Ok(nota) => NotaDeDevolucion { nota: Some(nota), aviso: None },
        Err((_, e)) => NotaDeDevolucion { nota: None, aviso: Some(format!("No se emitió la nota de crédito: {}", e)) },
    })
}

// ===== Pantalla Comprobantes =====

#[derive(Serialize)]
pub struct Motivo {
    pub codigo: &'static str,
    pub nombre: &'static str,
}

#[derive(Serialize)]
pub struct PreparacionNota {
    pub documento: String,
    pub tipo: String,
    pub total: f64,
    /// Lo ya acreditado en notas anteriores (aceptadas o pendientes).
    pub acreditado: f64,
    pub lineas: Vec<LineaDisponible>,
    /// Se puede emitir una nota por el total (no hay notas anteriores).
    pub puede_total: bool,
    pub motivos: Vec<Motivo>,
    pub serie: String,
}

/// GET /comprobantes/:id/nota-credito — lo que se puede acreditar.
pub async fn preparar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<PreparacionNota>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let original = comprobante_original(&conn, id).await?;
    let previas = notas_vigentes(&conn, id).await?;
    let docs: Vec<Value> = previas.iter().map(|(d, _)| d.clone()).collect();
    let acreditado = (previas.iter().map(|(_, t)| t).sum::<f64>() * 100.0).round() / 100.0;
    Ok(Json(PreparacionNota {
        documento: format!("{}-{}", original.serie, original.numero),
        tipo: if original.documento["tipoDoc"] == "01" { "FACTURA" } else { "BOLETA" }.to_string(),
        total: logica::total_de(&original.documento),
        acreditado,
        lineas: logica::lineas_disponibles(&original.documento, &docs),
        puede_total: previas.is_empty(),
        motivos: logica::MOTIVOS.iter().map(|(codigo, nombre)| Motivo { codigo, nombre }).collect(),
        serie: serie_nota(&conn, &original.serie).await?,
    }))
}

#[derive(Deserialize)]
pub struct PedidoNota {
    pub motivo_codigo: String,
    #[serde(default)]
    pub motivo: Option<String>,
    #[serde(default)]
    pub lineas: Vec<LineaNota>,
}

/// POST /comprobantes/:id/nota-credito
pub async fn emitir(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(pedido): Json<PedidoNota>,
) -> Result<Json<NotaEmitida>, Fallo> {
    let lycet = Lycet::exigir()?;
    let conn = tenant.0.connect().map_err(interno)?;
    let nota = emitir_nota(
        &conn,
        &lycet,
        id,
        pedido.motivo_codigo.trim(),
        pedido.motivo.as_deref().unwrap_or(""),
        &pedido.lineas,
        None,
        Some(claims.sub),
    )
    .await?;
    Ok(Json(nota))
}

/// Reenvía una nota PENDIENTE y guarda el resultado.
pub async fn reenviar_nota(conn: &libsql::Connection, lycet: &Lycet, nota_id: i64) -> Result<RespuestaSunat, Fallo> {
    let mut filas = conn
        .query("SELECT serie, numero, estado, documento, envio_incierto FROM notas_credito WHERE id = ?1", libsql::params![nota_id])
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Nota de crédito no encontrada.".to_string()))?;
    let serie: String = f.get(0).unwrap_or_default();
    let numero: i64 = f.get(1).unwrap_or_default();
    let estado: String = f.get(2).unwrap_or_default();
    if estado != "PENDIENTE" {
        return Err((StatusCode::CONFLICT, format!("La nota {}-{} ya no está pendiente ({}).", serie, numero, estado)));
    }
    let documento: Value = f
        .get::<String>(3)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| interno(format!("La nota {}-{} no tiene su documento guardado.", serie, numero)))?;
    let incierto = f.get::<i64>(4).unwrap_or(0) == 1;
    drop(filas);
    if !super::envios_sunat::reclamar(conn, "notas_credito", "estado = 'PENDIENTE'", "fecha_emision", "mensaje_sunat", nota_id).await? {
        return Err(super::envios_sunat::en_curso(&format!("La nota {}-{}", serie, numero)));
    }
    let r = reenviar_documento(lycet, "note", &documento, incierto).await;
    guardar_resultado(conn, nota_id, &r, true).await?;
    Ok(r)
}

/// POST /notas-credito/:id/reenviar
pub async fn reenviar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<super::envios_sunat::ResultadoReenvio>, Fallo> {
    let lycet = Lycet::exigir()?;
    let conn = tenant.0.connect().map_err(interno)?;
    let r = reenviar_nota(&conn, &lycet, id).await?;
    Ok(Json(super::envios_sunat::ResultadoReenvio { estado: r.estado.como_texto().to_string(), mensaje: r.mensaje }))
}

/// GET /notas-credito/:id/documento — la nota tal como se envió (para imprimirla).
pub async fn documento(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Value>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let mut filas = conn
        .query("SELECT documento FROM notas_credito WHERE id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Nota de crédito no encontrada.".to_string()))?;
    f.get::<String>(0)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "La nota no tiene documento guardado.".to_string()))
}

async fn descargar(tenant: Arc<TenantDb>, id: i64, cdr: bool) -> Result<Response, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    let mut filas = conn
        .query(
            "SELECT serie, numero, estado, xml, cdr_zip, (SELECT ruc FROM configuracion_tienda LIMIT 1)
             FROM notas_credito WHERE id = ?1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Nota de crédito no encontrada.".to_string()))?;
    let serie: String = f.get(0).unwrap_or_default();
    let numero: i64 = f.get(1).unwrap_or_default();
    let estado: String = f.get(2).unwrap_or_default();
    if estado != "ACEPTADO" {
        return Err((StatusCode::CONFLICT, "Esta nota no fue aceptada por SUNAT: no tiene XML ni constancia (CDR).".into()));
    }
    let ruc: String = f.get(5).unwrap_or_default();
    let (bytes, tipo, extension) = if cdr {
        use base64::Engine;
        let b64: String = f.get(4).map_err(|_| (StatusCode::NOT_FOUND, "Esta nota no tiene constancia guardada.".to_string()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| interno("La constancia guardada no es válida."))?;
        (bytes, "application/zip", "zip")
    } else {
        let xml: String = f.get(3).map_err(|_| (StatusCode::NOT_FOUND, "Esta nota no tiene XML guardado.".to_string()))?;
        (xml.into_bytes(), "application/xml", "xml")
    };
    let nombre = format!("{}{}-07-{}-{}.{}", if cdr { "R-" } else { "" }, ruc, serie, numero, extension);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, tipo)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", nombre))
        .body(Body::from(bytes))
        .map(|r| r.into_response())
        .map_err(interno)
}

/// GET /notas-credito/:id/xml
pub async fn descargar_xml(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Response, Fallo> {
    descargar(tenant, id, false).await
}

/// GET /notas-credito/:id/cdr
pub async fn descargar_cdr(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Response, Fallo> {
    descargar(tenant, id, true).await
}

/// Nota en la lista de Comprobantes, debajo del comprobante que corrige.
#[derive(Serialize, Debug, Clone)]
pub struct NotaResumen {
    pub id: i64,
    pub serie: String,
    pub numero: i64,
    pub motivo: String,
    pub total: f64,
    pub estado: String,
    pub mensaje_sunat: Option<String>,
    pub fecha_emision: String,
    pub hash: Option<String>,
    pub tiene_xml: bool,
    pub tiene_cdr: bool,
}

/// Notas de crédito de varios comprobantes a la vez (vacío si la base aún
/// no tiene la migración 0024).
pub async fn notas_de(conn: &libsql::Connection, comprobantes: &[i64]) -> HashMap<i64, Vec<NotaResumen>> {
    let mut mapa: HashMap<i64, Vec<NotaResumen>> = HashMap::new();
    if comprobantes.is_empty() {
        return mapa;
    }
    let lista = comprobantes.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT comprobante_id, id, serie, numero, motivo, total, estado, mensaje_sunat, fecha_emision, hash,
                xml IS NOT NULL, cdr_zip IS NOT NULL
         FROM notas_credito WHERE comprobante_id IN ({}) ORDER BY id",
        lista
    );
    let Ok(mut filas) = conn.query(&sql, ()).await else { return mapa };
    while let Ok(Some(f)) = filas.next().await {
        let Ok(comprobante) = f.get::<i64>(0) else { continue };
        mapa.entry(comprobante).or_default().push(NotaResumen {
            id: f.get(1).unwrap_or_default(),
            serie: f.get(2).unwrap_or_default(),
            numero: f.get(3).unwrap_or_default(),
            motivo: f.get(4).unwrap_or_default(),
            total: f.get(5).unwrap_or_default(),
            estado: f.get(6).unwrap_or_default(),
            mensaje_sunat: f.get(7).ok(),
            fecha_emision: f.get(8).unwrap_or_default(),
            hash: f.get(9).ok(),
            tiene_xml: f.get::<i64>(10).unwrap_or(0) == 1,
            tiene_cdr: f.get::<i64>(11).unwrap_or(0) == 1,
        });
    }
    mapa
}
