//! Módulo VENTA_SIN_CONEXION: vender sin internet.
//!
//! Sin conexión, el punto de venta guarda en el equipo cada operación (una
//! venta, la apertura de la caja o un gasto de caja) con un código único y
//! la hora en que se hizo. Al volver internet las sube una por una, en orden,
//! a POST /sin-conexion/operaciones. Aquí cada una pasa por el MISMO código
//! de siempre (ventas::procesar_venta, cajas::abrir_caja, gastos::registrar,
//! facturacion::emitir_comprobante_con_enlace), con tres diferencias:
//!
//! - La venta queda con su hora real y se respeta el precio que pagó el
//!   cliente (el equipo no podía saber si alguien lo cambió).
//! - La boleta o factura sale con la fecha y hora de la venta.
//! - El código único queda en `operaciones_sin_conexion` (migración 0061):
//!   si el equipo vuelve a mandar la misma operación (se cortó la respuesta,
//!   se cargó un respaldo...), no entra otra vez; se le responde lo mismo.
//!   En una venta el código se guarda en la MISMA transacción de la venta.
//!
//! Si algo no se puede hacer (por ejemplo, otro equipo vendió el último y
//! no alcanza el stock), se responde el error y no se guarda nada: el equipo
//! la deja "por revisar" para reintentarla después de corregir.
//!
//! Un negocio sin el módulo recibe 403 y no cambia nada de lo de siempre.

use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(texto: &str) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.to_string())
}

/// Mensaje con el que `registrar_venta` avisa que ese código ya entró.
const YA_REGISTRADA: &str = "Esta venta ya se había subido.";

const NOMBRE_MODULO: &str = "Venta sin conexión";

/// Códigos que pone el equipo: de 8 a 64 letras, números o guiones.
pub fn uuid_valido(uuid: &str) -> bool {
    (8..=64).contains(&uuid.len()) && uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// La hora en que se hizo la operación, tal como la guardó el equipo
/// ("AAAA-MM-DD HH:MM:SS", hora de Perú). Si no se entiende, o el reloj del
/// equipo estaba adelantado, se usa la hora del servidor: nunca una fecha
/// futura (SUNAT la rechazaría).
pub fn fecha_de_la_operacion(fecha_local: &str, ahora: &str) -> String {
    const FORMATO: &str = "%Y-%m-%d %H:%M:%S";
    let texto = fecha_local.trim().replace('T', " ");
    let texto: String = texto.chars().take(19).collect();
    match chrono::NaiveDateTime::parse_from_str(&texto, FORMATO) {
        Ok(f) if f.format(FORMATO).to_string().as_str() <= ahora => f.format(FORMATO).to_string(),
        _ => ahora.to_string(),
    }
}

/// Dentro de la transacción de la venta (ventas::procesar_venta): guarda el
/// código de la venta y, si se hizo sin internet, le pone su hora real. Si
/// el código ya estaba, la venta NO entra (CONFLICT con YA_REGISTRADA).
pub async fn registrar_venta(
    conn: &libsql::Connection,
    uuid: &str,
    venta_id: i64,
    usuario_id: i64,
    fecha_hora: Option<&str>,
) -> Result<(), Fallo> {
    if !uuid_valido(uuid) {
        return Err(malo("El código de la venta no es válido."));
    }
    let insertado = conn
        .execute(
            "INSERT INTO operaciones_sin_conexion (uuid, tipo, estado, venta_id, usuario_id, fecha_local, recibida)
             VALUES (?1, 'VENTA', 'VENTA_HECHA', ?2, ?3, ?4, ?5)",
            libsql::params![
                uuid,
                venta_id,
                usuario_id,
                fecha_hora.map(str::to_string),
                crate::logica::tiempo::ahora_lima()
            ],
        )
        .await;
    if let Err(e) = insertado {
        let texto = e.to_string();
        if texto.to_uppercase().contains("UNIQUE") || texto.to_uppercase().contains("PRIMARY KEY") {
            return Err((StatusCode::CONFLICT, YA_REGISTRADA.to_string()));
        }
        if texto.contains("no such table") {
            return Err((
                StatusCode::BAD_REQUEST,
                "Este negocio todavía no tiene la actualización de \"Venta sin conexión\" (migración 0061).".to_string(),
            ));
        }
        return Err(e500(texto));
    }
    if let Some(fecha) = fecha_hora {
        conn.execute("UPDATE ventas SET fecha_hora = ?1 WHERE id = ?2", libsql::params![fecha, venta_id])
            .await
            .map_err(e500)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------
// POST /sin-conexion/operaciones
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Operacion {
    pub uuid: String,
    /// 'VENTA', 'ABRIR_CAJA' o 'GASTO'.
    pub tipo: String,
    /// Cuándo se hizo en el equipo: "AAAA-MM-DD HH:MM:SS" (hora de Perú).
    pub fecha_local: String,
    pub datos: Value,
}

/// Boleta o factura que se pidió con la venta.
#[derive(Deserialize)]
struct ComprobantePedido {
    tipo: String,
    #[serde(default)]
    cliente_documento: Option<String>,
    #[serde(default)]
    cliente_nombre: Option<String>,
    #[serde(default)]
    detraccion: Option<bool>,
}

/// Cliente registrado en el equipo sin internet (todavía no existe aquí).
#[derive(Deserialize)]
struct ClienteNuevo {
    tipo_documento: String,
    numero_documento: String,
    nombre_razon_social: String,
    #[serde(default)]
    direccion: Option<String>,
}

#[derive(Deserialize)]
struct DatosVenta {
    venta: Value,
    #[serde(default)]
    comprobante: Option<ComprobantePedido>,
    #[serde(default)]
    cliente_nuevo: Option<ClienteNuevo>,
}

#[derive(Deserialize)]
struct DatosAbrirCaja {
    monto_inicial: f64,
    #[serde(default)]
    observaciones: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Resultado {
    pub uuid: String,
    pub tipo: String,
    /// 'HECHA' o 'VENTA_HECHA' (la venta entró, pero su boleta o factura no
    /// se pudo emitir: el equipo la vuelve a mandar y se intenta otra vez).
    pub estado: String,
    /// true = ya se había subido antes (no entró otra vez).
    #[serde(default)]
    pub repetida: bool,
    pub mensaje: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venta_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cliente_id: Option<i64>,
    /// Lo mismo que responde POST /comprobantes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comprobante: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comprobante_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caja_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gasto_id: Option<i64>,
}

/// Lo que ya hay de un código: (tipo, estado, venta_id, resultado).
async fn leer(conn: &libsql::Connection, uuid: &str) -> Result<Option<(String, String, Option<i64>, Option<String>)>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT tipo, estado, venta_id, resultado FROM operaciones_sin_conexion WHERE uuid = ?1",
            libsql::params![uuid],
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("no such table") {
                malo("Este negocio todavía no tiene la actualización de \"Venta sin conexión\" (migración 0061).")
            } else {
                e500(e)
            }
        })?;
    Ok(match filas.next().await.map_err(e500)? {
        Some(f) => Some((
            f.get::<String>(0).unwrap_or_default(),
            f.get::<String>(1).unwrap_or_default(),
            f.get::<i64>(2).ok(),
            f.get::<String>(3).ok(),
        )),
        None => None,
    })
}

async fn guardar_resultado(conn: &libsql::Connection, r: &Resultado) -> Result<(), Fallo> {
    let texto = serde_json::to_string(r).map_err(e500)?;
    conn.execute(
        "UPDATE operaciones_sin_conexion SET estado = ?1, resultado = ?2, venta_id = COALESCE(?3, venta_id),
                gasto_id = COALESCE(?4, gasto_id), caja_id = COALESCE(?5, caja_id)
         WHERE uuid = ?6",
        libsql::params![r.estado.clone(), texto, r.venta_id, r.gasto_id, r.caja_id, r.uuid.clone()],
    )
    .await
    .map_err(e500)?;
    Ok(())
}

/// Aparta el código antes de abrir la caja o registrar el gasto (en una
/// venta lo guarda la transacción de la venta). false = ya estaba.
async fn apartar(conn: &libsql::Connection, op: &Operacion, usuario_id: i64, fecha: &str) -> Result<bool, Fallo> {
    match conn
        .execute(
            "INSERT INTO operaciones_sin_conexion (uuid, tipo, estado, usuario_id, fecha_local, recibida)
             VALUES (?1, ?2, 'EN_PROCESO', ?3, ?4, ?5)",
            libsql::params![op.uuid.clone(), op.tipo.clone(), usuario_id, fecha, crate::logica::tiempo::ahora_lima()],
        )
        .await
    {
        Ok(_) => Ok(true),
        Err(e) if e.to_string().to_uppercase().contains("UNIQUE") || e.to_string().to_uppercase().contains("PRIMARY KEY") => Ok(false),
        Err(e) => Err(e500(e)),
    }
}

async fn soltar(conn: &libsql::Connection, uuid: &str) {
    let _ = conn
        .execute("DELETE FROM operaciones_sin_conexion WHERE uuid = ?1 AND estado = 'EN_PROCESO'", libsql::params![uuid])
        .await;
}

/// Respuesta para un código que ya entró.
fn repetida(uuid: &str, tipo: &str, resultado: Option<String>) -> Resultado {
    let mut r: Resultado = resultado
        .as_deref()
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_else(|| Resultado {
            uuid: uuid.to_string(),
            tipo: tipo.to_string(),
            estado: "HECHA".into(),
            mensaje: "Ya se había subido.".into(),
            ..Default::default()
        });
    r.repetida = true;
    r
}

pub async fn subir_operacion(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(op): Json<Operacion>,
) -> Result<Json<Resultado>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    crate::handlers::rubros::exigir_modulo(&conn, crate::handlers::rubros::MODULO_VENTA_SIN_CONEXION, NOMBRE_MODULO).await?;
    if !uuid_valido(&op.uuid) {
        return Err(malo("El código de la operación no es válido."));
    }
    let fecha = fecha_de_la_operacion(&op.fecha_local, &crate::logica::tiempo::ahora_lima());
    let previo = leer(&conn, &op.uuid).await?;
    if let Some((tipo, _, _, _)) = &previo {
        if tipo != &op.tipo {
            return Err((StatusCode::CONFLICT, "Ese código ya se usó en otra operación.".into()));
        }
    }

    let r = match op.tipo.as_str() {
        "VENTA" => subir_venta(&conn, &tenant, &claims, &op, &fecha, previo).await?,
        "ABRIR_CAJA" => {
            if let Some((tipo, estado, _, resultado)) = previo {
                return terminada_o_pendiente(&op.uuid, &tipo, &estado, resultado);
            }
            subir_apertura(&conn, &tenant, &claims, &op, &fecha).await?
        }
        "GASTO" => {
            if let Some((tipo, estado, _, resultado)) = previo {
                return terminada_o_pendiente(&op.uuid, &tipo, &estado, resultado);
            }
            subir_gasto(&conn, &tenant, &claims, &op, &fecha).await?
        }
        _ => return Err(malo("Tipo de operación no válido.")),
    };
    Ok(Json(r))
}

/// Una apertura de caja o un gasto que ya llegó antes.
fn terminada_o_pendiente(uuid: &str, tipo: &str, estado: &str, resultado: Option<String>) -> Result<Json<Resultado>, Fallo> {
    if estado == "EN_PROCESO" {
        // Se cortó a mitad de guardarse: no se sabe si quedó. Mejor revisar a
        // mano que registrarlo dos veces.
        return Err((
            StatusCode::CONFLICT,
            "Esta operación se cortó a mitad de guardarse. Revisa en Caja si quedó registrada antes de volver a hacerla.".into(),
        ));
    }
    Ok(Json(repetida(uuid, tipo, resultado)))
}

async fn subir_venta(
    conn: &libsql::Connection,
    tenant: &Arc<TenantDb>,
    claims: &Claims,
    op: &Operacion,
    fecha: &str,
    previo: Option<(String, String, Option<i64>, Option<String>)>,
) -> Result<Resultado, Fallo> {
    let datos: DatosVenta =
        serde_json::from_value(op.datos.clone()).map_err(|e| malo(&format!("La venta guardada no se entiende: {}", e)))?;

    // ¿Ya entró? Terminada: se responde lo mismo. Con la venta adentro pero
    // sin su comprobante: se intenta emitirlo otra vez.
    let (venta_id, folio, cliente_id) = match previo {
        Some((tipo, estado, _, resultado)) if estado == "HECHA" => return Ok(repetida(&op.uuid, &tipo, resultado)),
        Some((_, _, Some(venta_id), resultado)) => {
            let anterior: Option<Resultado> = resultado.as_deref().and_then(|t| serde_json::from_str(t).ok());
            let folio = folio_de(conn, venta_id).await?;
            (venta_id, folio, anterior.and_then(|a| a.cliente_id))
        }
        _ => crear_venta(conn, tenant, claims, op, fecha, &datos).await?,
    };

    let mut r = Resultado {
        uuid: op.uuid.clone(),
        tipo: "VENTA".into(),
        estado: "HECHA".into(),
        mensaje: format!("Venta {} registrada.", folio),
        venta_id: Some(venta_id),
        folio: Some(folio.clone()),
        cliente_id,
        ..Default::default()
    };

    if let Some(c) = &datos.comprobante {
        let tipo = c.tipo.trim().to_uppercase();
        if tipo == "BOLETA" || tipo == "FACTURA" {
            let pedido = crate::models::facturacion::EmitirComprobanteRequest {
                venta_id,
                tipo,
                cliente_documento: c.cliente_documento.clone(),
                cliente_nombre: c.cliente_nombre.clone(),
                detraccion: c.detraccion,
                fecha_hora: Some(fecha.to_string()),
            };
            match crate::handlers::facturacion::emitir_comprobante_con_enlace(
                Extension(tenant.clone()),
                Extension(claims.clone()),
                Json(pedido),
            )
            .await
            {
                Ok(Json(comprobante)) => {
                    r.mensaje = format!("Venta {} registrada; {} {}-{} {}.", folio, comprobante.tipo.to_lowercase(), comprobante.serie, comprobante.numero, estado_legible(&comprobante.estado));
                    r.comprobante = Some(serde_json::to_value(&comprobante).map_err(e500)?);
                }
                Err((_, error)) => match comprobante_ya_emitido(conn, claims, venta_id).await? {
                    // Ya tenía uno (se emitió en una subida anterior que se cortó).
                    Some(existente) => r.comprobante = Some(existente),
                    None => {
                        r.estado = "VENTA_HECHA".into();
                        r.mensaje = format!("Venta {} registrada, pero falló la boleta o factura: {}", folio, error);
                        r.comprobante_error = Some(error);
                    }
                },
            }
        }
    }
    guardar_resultado(conn, &r).await?;
    Ok(r)
}

fn estado_legible(estado: &str) -> &'static str {
    match estado {
        "ACEPTADO" => "aceptada por SUNAT",
        "PENDIENTE" => "pendiente de envío a SUNAT",
        "RECHAZADO" => "rechazada por SUNAT",
        _ => "con error",
    }
}

async fn folio_de(conn: &libsql::Connection, venta_id: i64) -> Result<String, Fallo> {
    let mut filas = conn.query("SELECT folio FROM ventas WHERE id = ?1", libsql::params![venta_id]).await.map_err(e500)?;
    Ok(match filas.next().await.map_err(e500)? {
        Some(f) => f.get::<String>(0).unwrap_or_default(),
        None => String::new(),
    })
}

/// La boleta o factura que ya tiene esta venta, con lo mínimo para verla.
async fn comprobante_ya_emitido(conn: &libsql::Connection, claims: &Claims, venta_id: i64) -> Result<Option<Value>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT id, tipo, serie, numero, estado FROM comprobantes_electronicos
             WHERE venta_id = ?1 AND estado IN ('ACEPTADO', 'PENDIENTE') ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .map_err(e500)?;
    Ok(match filas.next().await.map_err(e500)? {
        Some(f) => {
            let id: i64 = f.get(0).unwrap_or_default();
            let estado: String = f.get(4).unwrap_or_default();
            Some(serde_json::json!({
                "success": estado == "ACEPTADO",
                "comprobante_id": id,
                "tipo": f.get::<String>(1).unwrap_or_default(),
                "serie": f.get::<String>(2).unwrap_or_default(),
                "numero": f.get::<i64>(3).unwrap_or_default(),
                "estado": estado,
                "mensaje": "Ya estaba emitido.",
                "clave_publica": crate::logica::enlace_publico::clave(claims.tienda_id, id),
            }))
        }
        None => None,
    })
}

/// Lo que no se vende sin conexión (necesita datos al día del servidor).
fn revisar_venta(venta: &Value) -> Result<(), Fallo> {
    let lleva = |campo: &str| venta.get(campo).is_some_and(|v| !v.is_null());
    for (campo, nombre) in [
        ("credito", "una venta al crédito"),
        ("cotizacion_id", "una cotización"),
        ("cambio_prenda", "un cambio de prenda"),
        ("pedido_id", "un pedido de mesa"),
        ("encargo_id", "un pedido por encargo"),
        ("orden_servicio_id", "una orden de servicio"),
    ] {
        if lleva(campo) {
            return Err(malo(&format!("Sin conexión no se puede cobrar {}.", nombre)));
        }
    }
    Ok(())
}

async fn crear_venta(
    conn: &libsql::Connection,
    tenant: &Arc<TenantDb>,
    claims: &Claims,
    op: &Operacion,
    fecha: &str,
    datos: &DatosVenta,
) -> Result<(i64, String, Option<i64>), Fallo> {
    revisar_venta(&datos.venta)?;
    let mut venta: crate::models::venta::NuevaVenta =
        serde_json::from_value(datos.venta.clone()).map_err(|e| malo(&format!("La venta guardada no se entiende: {}", e)))?;

    // Cliente que se registró sin internet: si ya existe (por su documento)
    // se usa ese; si no, se crea.
    if let Some(nuevo) = &datos.cliente_nuevo {
        venta.cliente_id = Some(cliente_para(conn, nuevo).await?);
    }
    let cliente_id = venta.cliente_id;
    venta.uuid_local = Some(op.uuid.clone());
    venta.sin_conexion = Some(crate::models::venta::VentaSinConexion { fecha_hora: fecha.to_string() });

    match crate::handlers::ventas::procesar_venta(Extension(tenant.clone()), Extension(claims.clone()), Json(venta)).await {
        Ok(Json(v)) => Ok((v.venta_id, v.folio, cliente_id)),
        // Otro envío de la misma venta entró justo antes.
        Err((StatusCode::CONFLICT, texto)) if texto == YA_REGISTRADA => match leer(conn, &op.uuid).await? {
            Some((_, _, Some(venta_id), _)) => Ok((venta_id, folio_de(conn, venta_id).await?, cliente_id)),
            _ => Err((StatusCode::CONFLICT, texto)),
        },
        Err(e) => Err(e),
    }
}

async fn cliente_para(conn: &libsql::Connection, c: &ClienteNuevo) -> Result<i64, Fallo> {
    let tipo = c.tipo_documento.trim().to_uppercase();
    let documento = c.numero_documento.trim().to_string();
    let nombre = c.nombre_razon_social.trim().to_string();
    if documento.is_empty() || nombre.is_empty() {
        return Err(malo("Al cliente nuevo le falta su documento o su nombre."));
    }
    let mut filas = conn
        .query(
            "SELECT id, activo FROM clientes WHERE tipo_documento = ?1 AND numero_documento = ?2 ORDER BY activo DESC, id LIMIT 1",
            libsql::params![tipo.clone(), documento.clone()],
        )
        .await
        .map_err(e500)?;
    if let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).map_err(e500)?;
        let activo: i64 = f.get(1).unwrap_or(1);
        drop(filas);
        if activo != 1 {
            conn.execute(
                "UPDATE clientes SET activo = 1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(e500)?;
        }
        return Ok(id);
    }
    drop(filas);
    let direccion = c.direccion.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    conn.execute(
        "INSERT INTO clientes (tipo_documento, numero_documento, nombre_razon_social, direccion) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![tipo, documento, nombre, direccion],
    )
    .await
    .map_err(e500)?;
    Ok(conn.last_insert_rowid())
}

async fn subir_apertura(
    conn: &libsql::Connection,
    tenant: &Arc<TenantDb>,
    claims: &Claims,
    op: &Operacion,
    fecha: &str,
) -> Result<Resultado, Fallo> {
    let datos: DatosAbrirCaja = serde_json::from_value(op.datos.clone()).map_err(|_| malo("La apertura de caja guardada no se entiende."))?;
    if !apartar(conn, op, claims.sub, fecha).await? {
        let previo = leer(conn, &op.uuid).await?;
        let (tipo, estado, _, resultado) = previo.unwrap_or_default();
        return terminada_o_pendiente(&op.uuid, &tipo, &estado, resultado).map(|Json(r)| r);
    }
    let mut r = Resultado { uuid: op.uuid.clone(), tipo: op.tipo.clone(), estado: "HECHA".into(), ..Default::default() };

    // Si ya hay una abierta (la dejaron abierta, o la abrió otro equipo), las
    // ventas entran en esa: no hace falta abrir otra.
    let mut filas = conn
        .query("SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY id DESC LIMIT 1", ())
        .await
        .map_err(e500)?;
    let abierta: Option<i64> = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).ok(),
        None => None,
    };
    drop(filas);
    if let Some(id) = abierta {
        r.caja_id = Some(id);
        r.mensaje = "Ya había una caja abierta: las ventas entran en esa.".into();
    } else {
        let pedido = crate::models::caja::AbrirCajaRequest {
            usuario_id: claims.sub,
            numero_caja: Some(1),
            monto_inicial: datos.monto_inicial,
            observaciones: datos.observaciones.clone(),
        };
        match crate::handlers::cajas::abrir_caja(Extension(tenant.clone()), Extension(claims.clone()), Json(pedido)).await {
            Ok(Json(caja)) => {
                r.caja_id = caja.caja_id;
                r.mensaje = "Caja abierta.".into();
                // Queda con la hora en que se abrió en el equipo.
                if let Some(id) = caja.caja_id {
                    let hora = fecha.split_once(' ').map(|(_, h)| h).unwrap_or("00:00:00");
                    let _ = conn
                        .execute(
                            "UPDATE cajas SET fecha_apertura = ?1, hora_apertura = ?2 WHERE id = ?3",
                            libsql::params![fecha.to_string(), hora.to_string(), id],
                        )
                        .await;
                }
            }
            Err(e) => {
                soltar(conn, &op.uuid).await;
                return Err(e);
            }
        }
    }
    guardar_resultado(conn, &r).await?;
    Ok(r)
}

async fn subir_gasto(
    conn: &libsql::Connection,
    tenant: &Arc<TenantDb>,
    claims: &Claims,
    op: &Operacion,
    fecha: &str,
) -> Result<Resultado, Fallo> {
    let gasto: crate::handlers::gastos::NuevoGasto =
        serde_json::from_value(op.datos.clone()).map_err(|_| malo("El gasto guardado no se entiende."))?;
    let de_caja = gasto.metodo_pago == "EFECTIVO_CAJA";
    if !apartar(conn, op, claims.sub, fecha).await? {
        let previo = leer(conn, &op.uuid).await?;
        let (tipo, estado, _, resultado) = previo.unwrap_or_default();
        return terminada_o_pendiente(&op.uuid, &tipo, &estado, resultado).map(|Json(r)| r);
    }
    match crate::handlers::gastos::registrar(Extension(tenant.clone()), Extension(claims.clone()), Json(gasto)).await {
        Ok(Json(g)) => {
            // Un gasto de caja siempre se registra "hoy": queda con el día en
            // que se hizo en el equipo.
            if de_caja {
                let dia: String = fecha.chars().take(10).collect();
                let _ = conn.execute("UPDATE gastos SET fecha = ?1 WHERE id = ?2", libsql::params![dia, g.id]).await;
            }
            let r = Resultado {
                uuid: op.uuid.clone(),
                tipo: op.tipo.clone(),
                estado: "HECHA".into(),
                mensaje: g.mensaje.clone(),
                gasto_id: Some(g.id),
                caja_id: g.caja_id,
                ..Default::default()
            };
            guardar_resultado(conn, &r).await?;
            Ok(r)
        }
        Err(e) => {
            soltar(conn, &op.uuid).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn codigos() {
        assert!(uuid_valido("3f2a9c1e-7b4d-4e8f-9a6b-1c2d3e4f5a6b"));
        assert!(!uuid_valido("corto"));
        assert!(!uuid_valido("tiene espacios 1234"));
        assert!(!uuid_valido("x'); DROP TABLE ventas;--"));
        assert!(!uuid_valido(&"a".repeat(65)));
    }

    #[test]
    fn fecha_de_la_venta() {
        let ahora = "2026-11-04 10:00:00";
        assert_eq!(fecha_de_la_operacion("2026-11-02 08:15:30", ahora), "2026-11-02 08:15:30");
        assert_eq!(fecha_de_la_operacion("2026-11-02T08:15:30", ahora), "2026-11-02 08:15:30");
        // Reloj del equipo adelantado o texto raro: la hora del servidor.
        assert_eq!(fecha_de_la_operacion("2026-11-05 08:00:00", ahora), ahora);
        assert_eq!(fecha_de_la_operacion("ayer", ahora), ahora);
        assert_eq!(fecha_de_la_operacion("", ahora), ahora);
    }
}
