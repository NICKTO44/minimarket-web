//! Envíos a SUNAT de la emisión directa, a través de Lycet: el envío en sí
//! (con respuesta inmediata o con ticket), la consulta de un comprobante en
//! SUNAT, el reenvío de los que quedaron PENDIENTES y la tarea que los
//! reintenta sola cada 10 minutos.
//!
//! La interpretación de las respuestas (qué es aceptado, rechazado o
//! pendiente) está en logica/sunat_directo.rs, sin red ni base de datos.
//!
//! La tarea automática solo corre si el .env tiene, además de LYCET_URL:
//!   SUNAT_REINTENTOS=1
//! (opcional: SUNAT_REINTENTOS_MINUTOS=10, cada cuánto repasa; mínimo 1).
//! Así un backend levantado en otra máquina (por ejemplo, en local contra
//! las mismas bases) no reenvía en paralelo con el del servidor.

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use chrono::NaiveDateTime;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::logica::sunat_directo::{
    self, ConsultaCdr, EstadoEnvio, RespuestaSunat, RespuestaTicket, CODIGO_YA_REGISTRADO, DIAS_PLAZO_ENVIO,
};
use crate::tenants::{TenantDb, TiendaConexion};
use crate::AppState;

type Fallo = (StatusCode, String);

fn interno<E: ToString>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

// ===== Cliente de Lycet =====

/// Segundos que se espera la respuesta de Lycet (y de SUNAT detrás) en cada intento.
const TIEMPO_POR_INTENTO: u64 = 20;

/// Servidor de Lycet configurado en el .env (LYCET_URL, LYCET_TOKEN).
pub struct Lycet {
    url: String,
    token: String,
    cliente: reqwest::Client,
}

impl Lycet {
    pub fn desde_env() -> Option<Lycet> {
        let (url, token) = super::facturacion_directa::servidor_lycet()?;
        // 20 s por intento: SUNAT responde en 1–3 s; desde el servidor, a veces
        // una conexión se queda colgada sin respuesta y la siguiente responde
        // al instante (ver enviar_primero). Dos intentos caben antes de que el
        // proxy web corte la petición de la caja (60 s).
        let cliente = reqwest::Client::builder().timeout(Duration::from_secs(TIEMPO_POR_INTENTO)).build().ok()?;
        Some(Lycet { url, token, cliente })
    }

    /// El mismo error que se muestra cuando falta la configuración.
    pub fn exigir() -> Result<Lycet, Fallo> {
        Lycet::desde_env().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "La emisión directa a SUNAT no está configurada en el servidor (falta LYCET_URL).".to_string(),
        ))
    }

    /// POST /api/v1/{ruta}/send. Devuelve (HTTP, cuerpo) o el error de red
    /// (sin la URL, que lleva el token).
    async fn post(&self, ruta: &str, documento: &Value) -> Result<(u16, String), String> {
        let r = self
            .cliente
            .post(format!("{}/api/v1/{}/send", self.url, ruta))
            .query(&[("token", self.token.as_str())])
            .json(documento)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        let status = r.status().as_u16();
        Ok((status, r.text().await.unwrap_or_default()))
    }

    async fn get(&self, ruta: &str, consulta: &[(&str, &str)]) -> Result<(u16, String), String> {
        let r = self
            .cliente
            .get(format!("{}/api/v1/{}", self.url, ruta))
            .query(&[("token", self.token.as_str())])
            .query(consulta)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        let status = r.status().as_u16();
        Ok((status, r.text().await.unwrap_or_default()))
    }

    /// Envía un comprobante con respuesta inmediata: `ruta` = "invoice"
    /// (boleta y factura) o "note" (notas de crédito y débito). Nunca falla:
    /// sin conexión, el resultado es PENDIENTE.
    pub async fn enviar(&self, ruta: &str, documento: &Value) -> RespuestaSunat {
        match self.post(ruta, documento).await {
            Ok((status, texto)) => sunat_directo::leer_respuesta(status, &texto),
            Err(e) => sin_conexion(e),
        }
    }

    /// Envía un documento que SUNAT procesa después: `ruta` = "summary"
    /// (resumen diario), "voided" (comunicación de baja) o "despatch"
    /// (guía de remisión).
    pub async fn enviar_con_ticket(&self, ruta: &str, documento: &Value) -> RespuestaTicket {
        match self.post(ruta, documento).await {
            Ok((status, texto)) => sunat_directo::leer_envio_con_ticket(status, &texto),
            Err(e) => RespuestaTicket { ticket: None, respuesta: sin_conexion(e) },
        }
    }

    /// Pregunta por un ticket: PENDIENTE mientras SUNAT lo procesa.
    pub async fn consultar_ticket(&self, ruta: &str, ruc: &str, ticket: &str) -> RespuestaSunat {
        match self.get(&format!("{}/status", ruta), &[("ticket", ticket), ("ruc", ruc)]).await {
            Ok((status, texto)) => sunat_directo::leer_estado_ticket(status, &texto),
            Err(e) => RespuestaSunat::pendiente(format!("No se pudo consultar el ticket ({}). Se volverá a consultar.", e)),
        }
    }

    /// Pregunta a SUNAT si tiene una factura o nota de factura.
    pub async fn consultar_cdr(&self, ruc: &str, tipo: &str, serie: &str, numero: &str) -> ConsultaCdr {
        match self
            .get("invoice/status", &[("ruc", ruc), ("tipo", tipo), ("serie", serie), ("numero", numero)])
            .await
        {
            Ok((status, texto)) => sunat_directo::leer_consulta_cdr(status, &texto),
            Err(e) => ConsultaCdr::NoSePudo(e),
        }
    }
}

fn sin_conexion(detalle: String) -> RespuestaSunat {
    RespuestaSunat::incierta(format!(
        "No se pudo conectar con el servicio de emisión ({}). Quedó pendiente con su número y se reenviará solo.",
        detalle
    ))
}

/// Reenvía un documento que quedó PENDIENTE, exactamente como se guardó.
///
/// `incierto`: algún envío anterior de este documento se cortó sin
/// respuesta, así que pudo haber llegado a SUNAT. Solo entonces:
///   - si es factura o nota de factura, antes se pregunta a SUNAT si ya lo
///     tiene (así no se manda dos veces);
///   - si SUNAT responde que ya estaba registrado (1033), el comprobante es
///     este y queda aceptado.
/// Sin envío incierto, un 1033 es otro documento con el mismo número: queda
/// con error (ver sunat_directo::leer_respuesta).
pub async fn reenviar_documento(lycet: &Lycet, ruta: &str, documento: &Value, incierto: bool) -> RespuestaSunat {
    let texto = |clave: &str| match documento.get(clave) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let serie = texto("serie");
    let ruc = documento["company"]["ruc"].as_str().unwrap_or("").to_string();

    if incierto && sunat_directo::se_puede_consultar(&serie) {
        if let ConsultaCdr::Encontrado(r) = lycet.consultar_cdr(&ruc, &texto("tipoDoc"), &serie, &texto("correlativo")).await {
            return r;
        }
    }

    let mut r = lycet.enviar(ruta, documento).await;
    if incierto && r.codigo == Some(CODIGO_YA_REGISTRADO) {
        r.estado = EstadoEnvio::Aceptado;
        r.mensaje = "SUNAT ya lo tenía registrado: un envío anterior sí llegó.".to_string();
    }
    r
}

/// Primer envío de una boleta, factura o nota (`ruta` = "invoice" o "note"),
/// con un reintento inmediato si el primero se quedó sin respuesta.
///
/// Desde el servidor, la conexión con SUNAT producción a veces se queda
/// colgada y el intento siguiente responde en menos de un segundo. El
/// reintento es un reenvío "incierto" (el primero pudo haber llegado): una
/// factura se consulta antes y un 1033 cuenta como aceptado, así que no se
/// duplica nada. Si el reintento tampoco responde, queda PENDIENTE y la tarea
/// automática lo reenvía después.
pub async fn enviar_primero(lycet: &Lycet, ruta: &str, documento: &Value) -> RespuestaSunat {
    let primero = lycet.enviar(ruta, documento).await;
    if !primero.incierto {
        return primero;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let mut segundo = reenviar_documento(lycet, ruta, documento, true).await;
    // El primer envío pudo haber llegado: queda anotado para los reenvíos.
    segundo.incierto = true;
    segundo
}

/// Hora de Perú de hace 90 segundos: un envío en curso (dos intentos de 20 s) dura menos.
fn corte_envio_en_curso() -> String {
    (chrono::Utc::now() - chrono::Duration::hours(5) - chrono::Duration::seconds(90)).format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Mensajes que marcan un envío en curso (el primero y los reenvíos).
pub const ENVIANDO: &str = "Enviando a SUNAT...";
pub const REENVIANDO: &str = "Reenviando a SUNAT...";

/// Toma un documento para reenviarlo: si sigue pendiente (`condicion`) y no
/// hay otro envío en curso, lo marca "Reenviando a SUNAT..." (en
/// `columna_mensaje`) con la hora en `ultimo_intento`. Un envío en curso que
/// lleva más de 90 segundos (contados desde `ultimo_intento` o, si no hay,
/// desde `columna_fecha`) se da por cortado. false = otro envío está en
/// curso: el botón y la tarea automática no lo mandan dos veces a la vez.
pub async fn reclamar(
    conn: &libsql::Connection,
    tabla: &str,
    condicion: &str,
    columna_fecha: &str,
    columna_mensaje: &str,
    id: i64,
) -> Result<bool, Fallo> {
    let ahora = crate::logica::tiempo::ahora_lima();
    let corte = corte_envio_en_curso();
    let sql = format!(
        "UPDATE {tabla} SET ultimo_intento = ?1, {columna_mensaje} = ?4
         WHERE id = ?2 AND {condicion}
           AND (COALESCE({columna_mensaje}, '') NOT IN (?4, ?5) OR COALESCE(ultimo_intento, {columna_fecha}, '') < ?3)"
    );
    let cambiadas = conn.execute(&sql, libsql::params![ahora, id, corte, REENVIANDO, ENVIANDO]).await.map_err(interno)?;
    Ok(cambiadas > 0)
}

/// El error de "otro envío en curso".
pub fn en_curso(que: &str) -> Fallo {
    (StatusCode::CONFLICT, format!("{} se está enviando a SUNAT en este momento. Espera un minuto y vuelve a consultar.", que))
}

// ===== Boletas y facturas pendientes =====

/// Guarda el resultado de un envío de boleta o factura.
pub async fn guardar_resultado(
    conn: &libsql::Connection,
    comprobante_id: i64,
    r: &RespuestaSunat,
    es_reintento: bool,
) -> Result<(), Fallo> {
    let ahora = crate::logica::tiempo::ahora_lima();
    // Solo si sigue pendiente: un resultado definitivo de otro envío no se pisa.
    conn.execute(
        "UPDATE comprobantes_electronicos SET estado = ?1, mensaje_sunat = ?2, hash = COALESCE(?3, hash)
         WHERE id = ?4 AND estado = 'PENDIENTE'",
        libsql::params![r.estado.como_texto(), r.mensaje.clone(), r.hash.clone(), comprobante_id],
    )
    .await
    .map_err(interno)?;
    // Columnas de la migración 0023: si faltaran, el estado ya quedó.
    if es_reintento {
        let _ = conn
            .execute(
                "UPDATE comprobantes_electronicos SET intentos = intentos + 1, ultimo_intento = ?1 WHERE id = ?2",
                libsql::params![ahora.clone(), comprobante_id],
            )
            .await;
    }
    if r.incierto {
        let _ = conn
            .execute("UPDATE comprobantes_electronicos SET envio_incierto = 1 WHERE id = ?1", libsql::params![comprobante_id])
            .await;
    }
    let _ = conn
        .execute(
            "UPDATE comprobante_archivos SET xml = COALESCE(?1, xml), cdr_zip = COALESCE(?2, cdr_zip), actualizado = ?3
             WHERE comprobante_id = ?4",
            libsql::params![r.xml.clone(), r.cdr_zip.clone(), ahora, comprobante_id],
        )
        .await;
    Ok(())
}

/// Documento guardado de un comprobante directo.
pub async fn documento_guardado(conn: &libsql::Connection, comprobante_id: i64) -> Result<Option<Value>, Fallo> {
    let mut filas = conn
        .query("SELECT documento FROM comprobante_archivos WHERE comprobante_id = ?1", libsql::params![comprobante_id])
        .await
        .map_err(interno)?;
    let texto: Option<String> = match filas.next().await.map_err(interno)? {
        Some(f) => f.get(0).ok(),
        None => None,
    };
    Ok(texto.and_then(|t| serde_json::from_str(&t).ok()))
}

/// Reenvía una boleta o factura PENDIENTE y guarda el resultado.
pub async fn reenviar_comprobante(conn: &libsql::Connection, lycet: &Lycet, comprobante_id: i64) -> Result<RespuestaSunat, Fallo> {
    let mut filas = conn
        .query(
            "SELECT serie, numero, estado, proveedor FROM comprobantes_electronicos WHERE id = ?1",
            libsql::params![comprobante_id],
        )
        .await
        .map_err(interno)?;
    let fila = filas.next().await.map_err(interno)?.ok_or((StatusCode::NOT_FOUND, "Comprobante no encontrado.".to_string()))?;
    let serie: String = fila.get(0).unwrap_or_default();
    let numero: i64 = fila.get(1).unwrap_or_default();
    let estado: String = fila.get(2).unwrap_or_default();
    let proveedor: String = fila.get(3).unwrap_or_default();
    if proveedor != "SUNAT_DIRECTO" {
        return Err((StatusCode::BAD_REQUEST, "Solo se reenvían aquí los comprobantes emitidos directo a SUNAT.".to_string()));
    }
    if estado != "PENDIENTE" {
        return Err((StatusCode::CONFLICT, format!("El comprobante {}-{} ya no está pendiente ({}).", serie, numero, estado)));
    }
    drop(filas);
    let documento = documento_guardado(conn, comprobante_id)
        .await?
        .ok_or_else(|| interno(format!("El comprobante {}-{} no tiene su documento guardado.", serie, numero)))?;
    let incierto = match conn
        .query("SELECT envio_incierto FROM comprobantes_electronicos WHERE id = ?1", libsql::params![comprobante_id])
        .await
    {
        Ok(mut f) => matches!(f.next().await, Ok(Some(fila)) if fila.get::<i64>(0).unwrap_or(0) == 1),
        Err(_) => false,
    };
    if !reclamar(conn, "comprobantes_electronicos", "estado = 'PENDIENTE'", "fecha_emision", "mensaje_sunat", comprobante_id).await? {
        return Err(en_curso(&format!("El comprobante {}-{}", serie, numero)));
    }

    let r = reenviar_documento(lycet, "invoice", &documento, incierto).await;
    guardar_resultado(conn, comprobante_id, &r, true).await?;
    Ok(r)
}

#[derive(Serialize)]
pub struct ResultadoReenvio {
    pub estado: String,
    pub mensaje: String,
}

/// POST /comprobantes/:id/reenviar — botón "Reenviar" de Comprobantes.
pub async fn reenviar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Result<Json<ResultadoReenvio>, Fallo> {
    let lycet = Lycet::exigir()?;
    let conn = tenant.0.connect().map_err(interno)?;
    let r = reenviar_comprobante(&conn, &lycet, id).await?;
    Ok(Json(ResultadoReenvio { estado: r.estado.como_texto().to_string(), mensaje: r.mensaje }))
}

// ===== Avisos (plazos de SUNAT) =====

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Aviso {
    /// "PENDIENTE" (comprobante sin enviar), más adelante bajas y guías.
    pub tipo: String,
    /// "BM01-15"
    pub documento: String,
    pub mensaje: String,
    /// Días calendario que quedan del plazo de SUNAT (negativo = vencido).
    pub dias_restantes: i64,
}

#[derive(Serialize, Default)]
pub struct AvisosSunat {
    /// El negocio emite directo a SUNAT (si no, todo lo demás va vacío).
    pub modo_directo: bool,
    pub pendientes: usize,
    pub avisos: Vec<Aviso>,
}

/// Días calendario entre dos fechas "AAAA-MM-DD..." (solo la parte de la fecha).
pub fn dias_entre(desde: &str, hasta: &str) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(desde.get(..10)?, "%Y-%m-%d").ok()?;
    let h = chrono::NaiveDate::parse_from_str(hasta.get(..10)?, "%Y-%m-%d").ok()?;
    Some((h - d).num_days())
}

pub async fn es_modo_directo(conn: &libsql::Connection) -> bool {
    match conn.query("SELECT facturacion_proveedor FROM configuracion_tienda LIMIT 1", ()).await {
        Ok(mut filas) => matches!(filas.next().await, Ok(Some(f)) if f.get::<String>(0).ok().as_deref() == Some("SUNAT_DIRECTO")),
        Err(_) => false,
    }
}

/// Comprobantes directos que siguen sin llegar a SUNAT, con su plazo.
async fn avisos_de(conn: &libsql::Connection) -> Result<AvisosSunat, Fallo> {
    let mut resultado = AvisosSunat { modo_directo: es_modo_directo(conn).await, ..Default::default() };
    let hoy = crate::logica::tiempo::hoy_lima();
    let mut filas = conn
        .query(
            "SELECT serie, numero, fecha_emision, mensaje_sunat FROM comprobantes_electronicos
             WHERE proveedor = 'SUNAT_DIRECTO' AND estado = 'PENDIENTE' ORDER BY id",
            (),
        )
        .await
        .map_err(interno)?;
    let mut agregar = |serie: String, numero: i64, fecha: String, mensaje: String| {
        let transcurridos = dias_entre(&fecha, &hoy).unwrap_or(0);
        resultado.pendientes += 1;
        resultado.avisos.push(Aviso {
            tipo: "PENDIENTE".to_string(),
            documento: format!("{}-{}", serie, numero),
            mensaje,
            dias_restantes: DIAS_PLAZO_ENVIO - transcurridos,
        });
    };
    while let Some(f) = filas.next().await.map_err(interno)? {
        agregar(f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default(), f.get(2).unwrap_or_default(), f.get(3).unwrap_or_default());
    }
    // Notas de crédito pendientes (migración 0024; sin ella, no hay).
    if let Ok(mut filas) = conn
        .query("SELECT serie, numero, fecha_emision, mensaje_sunat FROM notas_credito WHERE estado = 'PENDIENTE' ORDER BY id", ())
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            agregar(f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default(), f.get(2).unwrap_or_default(), f.get(3).unwrap_or_default());
        }
    }
    // Anulaciones que todavía no llegan a SUNAT (sin ticket; migración 0025).
    if let Ok(mut filas) = conn
        .query(
            "SELECT b.identificador, b.fecha_documento, b.mensaje, ce.serie || '-' || ce.numero
             FROM bajas_sunat b JOIN comprobantes_electronicos ce ON ce.id = b.comprobante_id
             WHERE b.estado = 'PENDIENTE' AND (b.ticket IS NULL OR b.ticket = '') ORDER BY b.id",
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let fecha: String = f.get(1).unwrap_or_default();
            resultado.pendientes += 1;
            resultado.avisos.push(Aviso {
                tipo: "ANULACION".to_string(),
                documento: f.get::<String>(3).unwrap_or_default(),
                mensaje: format!("Anulación sin enviar: {}", f.get::<String>(2).unwrap_or_default()),
                dias_restantes: crate::logica::anulaciones::DIAS_PLAZO_ANULACION - dias_entre(&fecha, &hoy).unwrap_or(0),
            });
        }
    }
    resultado.avisos.sort_by_key(|a| a.dias_restantes);
    Ok(resultado)
}

/// GET /sunat/avisos — lo que todavía no llega a SUNAT, para el aviso de
/// la pantalla Comprobantes.
pub async fn avisos(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<AvisosSunat>, Fallo> {
    let conn = tenant.0.connect().map_err(interno)?;
    Ok(Json(avisos_de(&conn).await?))
}

// ===== Tarea automática =====

/// Minutos a esperar desde el último intento antes del siguiente: 10, 20,
/// 40, 80 y luego cada 2 horas. Los recién emitidos (menos de 2 minutos)
/// no se tocan: puede que su primer envío siga en curso.
pub fn toca_reintentar(emitido: &str, ultimo: Option<&str>, intentos: i64, ahora: &str) -> bool {
    let leer = |t: &str| NaiveDateTime::parse_from_str(t.get(..19).unwrap_or(t), "%Y-%m-%d %H:%M:%S").ok();
    let (Some(ahora), Some(emitido)) = (leer(ahora), leer(emitido)) else {
        return true;
    };
    if (ahora - emitido).num_minutes() < 2 {
        return false;
    }
    let Some(ultimo) = ultimo.and_then(leer) else {
        return true;
    };
    let espera = (10_i64 << intentos.clamp(0, 4)).min(120);
    (ahora - ultimo).num_minutes() >= espera
}

/// Arranca la tarea que cada 10 minutos reintenta lo pendiente de los
/// negocios en modo directo. No hace nada si falta SUNAT_REINTENTOS=1 o
/// LYCET_URL.
pub fn iniciar_tarea(state: Arc<AppState>) {
    if !std::env::var("SUNAT_REINTENTOS").is_ok_and(|v| v.trim() == "1") {
        println!("ℹ️  Reintentos automáticos a SUNAT apagados (falta SUNAT_REINTENTOS=1).");
        return;
    }
    if Lycet::desde_env().is_none() {
        println!("ℹ️  Reintentos automáticos a SUNAT apagados (falta LYCET_URL).");
        return;
    }
    let minutos = std::env::var("SUNAT_REINTENTOS_MINUTOS")
        .ok()
        .and_then(|m| m.trim().parse::<u64>().ok())
        .unwrap_or(10)
        .max(1);
    println!("🔁 Reintentos automáticos a SUNAT: cada {} minuto(s).", minutos);
    tokio::spawn(async move {
        // El primer repaso, un minuto después de arrancar (las migraciones
        // corren al mismo tiempo y deben terminar antes).
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut intervalo = tokio::time::interval(Duration::from_secs(60 * minutos));
        loop {
            intervalo.tick().await;
            let Some(lycet) = Lycet::desde_env() else { continue };
            let tiendas = match state.tiendas.listar_todas().await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("⚠️  Reintentos SUNAT: no se pudo listar los negocios: {}", e);
                    continue;
                }
            };
            for tienda in &tiendas {
                if let Err(e) = repasar_tienda(&state, tienda, &lycet).await {
                    eprintln!("⚠️  Reintentos SUNAT — {} ({}): {}", tienda.nombre_negocio, tienda.identificador, e);
                }
            }
        }
    });
}

/// Un repaso de un negocio: reenvía sus pendientes que ya toca reintentar.
async fn repasar_tienda(state: &AppState, tienda: &TiendaConexion, lycet: &Lycet) -> Result<(), String> {
    let db = state.tiendas.conectar_cacheado(tienda).await?;
    let conn = db.connect().map_err(|e| e.to_string())?;
    if !es_modo_directo(&conn).await {
        return Ok(());
    }
    let ahora = crate::logica::tiempo::ahora_lima();

    let mut por_reenviar = Vec::new();
    let mut filas = conn
        .query(
            "SELECT id, fecha_emision, ultimo_intento, intentos FROM comprobantes_electronicos
             WHERE proveedor = 'SUNAT_DIRECTO' AND estado = 'PENDIENTE' ORDER BY id LIMIT 30",
            (),
        )
        .await
        .map_err(|e| e.to_string())?;
    while let Some(f) = filas.next().await.map_err(|e| e.to_string())? {
        let id: i64 = f.get(0).map_err(|e| e.to_string())?;
        let emitido: String = f.get(1).unwrap_or_default();
        let ultimo: Option<String> = f.get(2).ok();
        let intentos: i64 = f.get(3).unwrap_or(0);
        if toca_reintentar(&emitido, ultimo.as_deref(), intentos, &ahora) {
            por_reenviar.push(id);
        }
    }

    // Notas de crédito pendientes (migración 0024; sin ella, no hay).
    let mut notas_por_reenviar = Vec::new();
    if let Ok(mut filas) = conn
        .query(
            "SELECT id, fecha_emision, ultimo_intento, intentos FROM notas_credito
             WHERE estado = 'PENDIENTE' ORDER BY id LIMIT 30",
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let Ok(id) = f.get::<i64>(0) else { continue };
            let emitido: String = f.get(1).unwrap_or_default();
            let ultimo: Option<String> = f.get(2).ok();
            if toca_reintentar(&emitido, ultimo.as_deref(), f.get(3).unwrap_or(0), &ahora) {
                notas_por_reenviar.push(id);
            }
        }
    }
    for id in notas_por_reenviar {
        match super::notas_credito::reenviar_nota(&conn, lycet, id).await {
            Ok(r) => println!(
                "🔁 {} ({}) — nota de crédito {}: {} · {}",
                tienda.nombre_negocio,
                tienda.identificador,
                id,
                r.estado.como_texto(),
                r.mensaje
            ),
            Err((_, e)) => eprintln!("⚠️  {} ({}) — nota de crédito {}: {}", tienda.nombre_negocio, tienda.identificador, id, e),
        }
    }

    // Anulaciones pendientes: con ticket se consulta y sin ticket se vuelve a
    // enviar, con la misma espera creciente que los comprobantes.
    let mut bajas = Vec::new();
    if let Ok(mut filas) = conn
        .query(
            "SELECT id, fecha, ultimo_intento, intentos FROM bajas_sunat WHERE estado = 'PENDIENTE' ORDER BY id LIMIT 30",
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let Ok(id) = f.get::<i64>(0) else { continue };
            let fecha: String = f.get(1).unwrap_or_default();
            let ultimo: Option<String> = f.get(2).ok();
            // Con ticket o sin él, con espera creciente (un ticket que SUNAT
            // no resuelve no se consulta cada 10 minutos para siempre).
            if toca_reintentar(&fecha, ultimo.as_deref(), f.get(3).unwrap_or(0), &ahora) {
                bajas.push(id);
            }
        }
    }
    for id in bajas {
        match super::anulaciones::avanzar(&conn, lycet, id, true).await {
            Ok((estado, mensaje)) => {
                println!("🔁 {} ({}) — anulación {}: {} · {}", tienda.nombre_negocio, tienda.identificador, id, estado, mensaje)
            }
            Err((_, e)) => eprintln!("⚠️  {} ({}) — anulación {}: {}", tienda.nombre_negocio, tienda.identificador, id, e),
        }
    }

    // Guías directas sin terminar (migración 0026): con ticket se consultan
    // y sin ticket se reenvían, con la misma espera creciente.
    let mut guias = Vec::new();
    if let Ok(mut filas) = conn
        .query(
            "SELECT id, fecha, ultimo_intento, intentos FROM guias_remision
             WHERE proveedor = 'SUNAT_DIRECTO' AND estado IN ('REGISTRADA', 'ENVIADA') ORDER BY id LIMIT 30",
            (),
        )
        .await
    {
        while let Ok(Some(f)) = filas.next().await {
            let Ok(id) = f.get::<i64>(0) else { continue };
            let fecha: String = f.get(1).unwrap_or_default();
            let ultimo: Option<String> = f.get(2).ok();
            if toca_reintentar(&fecha, ultimo.as_deref(), f.get(3).unwrap_or(0), &ahora) {
                guias.push(id);
            }
        }
    }
    for id in guias {
        if let Err((_, e)) = super::guias_directas::avanzar(&conn, lycet, id, true).await {
            eprintln!("⚠️  {} ({}) — guía {}: {}", tienda.nombre_negocio, tienda.identificador, id, e);
        }
    }

    for id in por_reenviar {
        match reenviar_comprobante(&conn, lycet, id).await {
            Ok(r) => println!(
                "🔁 {} ({}) — comprobante {}: {} · {}",
                tienda.nombre_negocio,
                tienda.identificador,
                id,
                r.estado.como_texto(),
                r.mensaje
            ),
            Err((_, e)) => eprintln!("⚠️  {} ({}) — comprobante {}: {}", tienda.nombre_negocio, tienda.identificador, id, e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn espera_entre_reintentos() {
        let ahora = "2026-10-09 12:00:00";
        // Recién emitido: su primer envío puede seguir en curso.
        assert!(!toca_reintentar("2026-10-09 11:59:00", None, 0, ahora));
        // Emitido hace rato y nunca reintentado.
        assert!(toca_reintentar("2026-10-09 11:50:00", None, 0, ahora));
        // Primer reintento hace 5 minutos: espera 10.
        assert!(!toca_reintentar("2026-10-09 11:00:00", Some("2026-10-09 11:55:00"), 1, ahora));
        // Un intento: 20 minutos.
        assert!(toca_reintentar("2026-10-09 11:00:00", Some("2026-10-09 11:39:00"), 1, ahora));
        // Muchos intentos: cada 2 horas como máximo.
        assert!(!toca_reintentar("2026-10-08 11:00:00", Some("2026-10-09 10:30:00"), 9, ahora));
        assert!(toca_reintentar("2026-10-08 11:00:00", Some("2026-10-09 10:00:00"), 9, ahora));
    }

    #[test]
    fn dias_de_plazo() {
        assert_eq!(dias_entre("2026-10-09 23:59:00", "2026-10-09"), Some(0));
        assert_eq!(dias_entre("2026-10-06 08:00:00", "2026-10-09"), Some(3));
        assert_eq!(dias_entre("2026-09-30", "2026-10-02"), Some(2));
        assert_eq!(dias_entre("", "2026-10-02"), None);
    }
}
