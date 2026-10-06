//! Ventas al crédito y abonos (módulo CREDITO): cuentas por cobrar.
//!
//! La venta al crédito es una venta normal del punto de venta cuyo pago
//! queda pendiente (ver migración 0015). Cada pago posterior es un ABONO:
//! baja el saldo y, si es en efectivo, entra a la caja abierta como
//! ingreso. El adelanto del día de la venta es simplemente el primer abono.

use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_CREDITO};
use crate::logica::igv::round2;
use crate::logica::tiempo::{ahora_lima, hoy_lima, hoy_lima_mas_dias};
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

/// Valor de ventas.pago_otro_metodo que identifica una venta al crédito.
pub const METODO_CREDITO: &str = "CREDITO";
pub const DIAS_POR_DEFECTO: i64 = 30;
// Yape y Plin van por separado; "YAPE_PLIN" queda por las pantallas que
// todavía no los distinguen.
const METODOS_ABONO: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE", "PLIN", "YAPE_PLIN"];

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(m: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, m.into())
}
/// La base aún no tiene la migración 0015.
fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

/// Lo que manda el punto de venta al cobrar "Crédito".
#[derive(Debug, Deserialize, Clone)]
pub struct CreditoVenta {
    /// Lo que el cliente paga hoy (0 = nada). Debe ser menor al total.
    #[serde(default)]
    pub adelanto: f64,
    /// Con qué paga el adelanto (EFECTIVO por defecto).
    #[serde(default)]
    pub adelanto_metodo: Option<String>,
    /// Días de plazo (30 por defecto).
    #[serde(default)]
    pub dias: Option<i64>,
}

/// Valida una venta al crédito ANTES de registrar nada. Devuelve
/// (adelanto, método del adelanto, fecha de vencimiento).
pub async fn validar_venta(
    conn: &libsql::Connection,
    credito: &CreditoVenta,
    cliente_id: Option<i64>,
    total: f64,
) -> Result<(f64, String, String), (StatusCode, String)> {
    exigir_modulo(conn, MODULO_CREDITO, "Ventas al crédito").await?;
    conn.query("SELECT 1 FROM creditos LIMIT 1", ()).await.map_err(actualizando)?;
    let cliente_id = cliente_id.ok_or_else(|| malo("Para vender al crédito elige al cliente."))?;
    let mut filas = conn.query("SELECT 1 FROM clientes WHERE id = ?1 AND activo = 1", libsql::params![cliente_id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err(malo("El cliente no existe o está desactivado."));
    }
    let adelanto = round2(credito.adelanto);
    if !(adelanto >= 0.0) || !adelanto.is_finite() {
        return Err(malo("El adelanto no es válido."));
    }
    if adelanto >= round2(total) {
        return Err(malo("El adelanto debe ser menor al total. Si paga todo hoy, cobra con un método normal."));
    }
    let metodo = credito.adelanto_metodo.clone().unwrap_or_else(|| "EFECTIVO".to_string());
    if !METODOS_ABONO.contains(&metodo.as_str()) {
        return Err(malo("Método de pago del adelanto no válido."));
    }
    let dias = credito.dias.unwrap_or(DIAS_POR_DEFECTO);
    if !(1..=365).contains(&dias) {
        return Err(malo("El plazo debe estar entre 1 y 365 días."));
    }
    Ok((adelanto, metodo, hoy_lima_mas_dias(dias)))
}

/// Crea el crédito de una venta recién registrada y, si hubo adelanto, su
/// primer abono. Devuelve el id del crédito.
pub async fn crear_para_venta(
    conn: &libsql::Connection,
    venta_id: i64,
    cliente_id: i64,
    total: f64,
    vence: &str,
    adelanto: f64,
    adelanto_metodo: &str,
    usuario_id: i64,
) -> Result<i64, (StatusCode, String)> {
    conn.execute(
        "INSERT INTO creditos (venta_id, cliente_id, total, vence, usuario_id, fecha) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        libsql::params![venta_id, cliente_id, round2(total), vence, usuario_id, ahora_lima()],
    )
    .await
    .map_err(e500)?;
    let credito_id = conn.last_insert_rowid();
    if adelanto > 0.0 {
        registrar_abono(conn, credito_id, adelanto, adelanto_metodo, Some("Adelanto"), usuario_id).await?;
    }
    Ok(credito_id)
}

/// Saldo pendiente de un crédito: (total, abonado, devuelto, estado).
async fn montos(conn: &libsql::Connection, credito_id: i64) -> Result<(f64, f64, f64, String), (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT CAST(total AS REAL), CAST(abonado AS REAL), CAST(devuelto AS REAL), estado FROM creditos WHERE id = ?1",
            libsql::params![credito_id],
        )
        .await
        .map_err(actualizando)?;
    match filas.next().await.map_err(e500)? {
        Some(f) => Ok((f.get(0).unwrap_or(0.0), f.get(1).unwrap_or(0.0), f.get(2).unwrap_or(0.0), f.get(3).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, "Ese crédito no existe.".to_string())),
    }
}

/// Registra un pago del cliente. En efectivo exige caja abierta y entra a
/// ella como ingreso (así el arqueo cuadra); los demás medios no tocan caja.
pub async fn registrar_abono(
    conn: &libsql::Connection,
    credito_id: i64,
    monto: f64,
    metodo: &str,
    nota: Option<&str>,
    usuario_id: i64,
) -> Result<(), (StatusCode, String)> {
    let monto = round2(monto);
    if !(monto > 0.0) || !monto.is_finite() {
        return Err(malo("El monto del abono debe ser mayor a 0."));
    }
    if !METODOS_ABONO.contains(&metodo) {
        return Err(malo("Método de pago no válido."));
    }
    let (total, abonado, devuelto, _) = montos(conn, credito_id).await?;
    let saldo = round2(total - abonado - devuelto);
    if saldo <= 0.0 {
        return Err((StatusCode::CONFLICT, "Este crédito ya está pagado.".to_string()));
    }
    if monto > saldo + 0.005 {
        return Err(malo(format!("El abono (S/ {:.2}) es mayor que el saldo (S/ {:.2}).", monto, saldo)));
    }

    let mut caja_id: Option<i64> = None;
    if metodo == "EFECTIVO" {
        // La caja abierta del negocio (primero la propia), igual que al vender.
        let mut filas = conn
            .query(
                "SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY (usuario_id = ?1) DESC, id DESC LIMIT 1",
                libsql::params![usuario_id],
            )
            .await
            .map_err(e500)?;
        caja_id = filas.next().await.map_err(e500)?.and_then(|f| f.get::<i64>(0).ok());
        if caja_id.is_none() {
            return Err(malo("Abre una caja para recibir el abono en efectivo."));
        }
    }

    conn.execute(
        "INSERT INTO credito_abonos (credito_id, monto, metodo_pago, caja_id, nota, usuario_id, fecha) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        libsql::params![credito_id, monto, metodo, caja_id, nota.map(|n| n.chars().take(200).collect::<String>()), usuario_id, ahora_lima()],
    )
    .await
    .map_err(e500)?;
    conn.execute(
        "UPDATE creditos SET abonado = abonado + ?1,
                estado = CASE WHEN total - (abonado + ?1) - devuelto <= 0.005 THEN 'PAGADO' ELSE 'PENDIENTE' END
         WHERE id = ?2",
        libsql::params![monto, credito_id],
    )
    .await
    .map_err(e500)?;

    if let Some(caja) = caja_id {
        let mut filas = conn
            .query(
                "SELECT COALESCE(v.folio, ''), COALESCE(c.nombre_razon_social, '') FROM creditos cr
                 LEFT JOIN ventas v ON v.id = cr.venta_id LEFT JOIN clientes c ON c.id = cr.cliente_id WHERE cr.id = ?1",
                libsql::params![credito_id],
            )
            .await
            .map_err(e500)?;
        let (folio, cliente): (String, String) = match filas.next().await.map_err(e500)? {
            Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default()),
            None => (String::new(), String::new()),
        };
        drop(filas);
        let motivo = format!("Abono de crédito {} · {}", folio, cliente).chars().take(200).collect::<String>();
        conn.execute(
            "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, 'INGRESO', ?2, ?3, ?4)",
            libsql::params![caja, monto, motivo, usuario_id],
        )
        .await
        .map_err(e500)?;
        conn.execute("UPDATE cajas SET ingresos_total = ingresos_total + ?1 WHERE id = ?2", libsql::params![monto, caja])
            .await
            .map_err(e500)?;
    }
    Ok(())
}

/// Una devolución de una venta al crédito baja la deuda en vez de devolver
/// dinero. Devuelve Err si la deuda pendiente es menor que lo devuelto.
pub async fn descontar_devolucion(conn: &libsql::Connection, venta_id: i64, monto: f64) -> Result<(), (StatusCode, String)> {
    let mut filas = conn.query("SELECT id FROM creditos WHERE venta_id = ?1", libsql::params![venta_id]).await.map_err(actualizando)?;
    let credito_id: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or_default(),
        None => return Err(malo("Esta venta no tiene un crédito pendiente.")),
    };
    drop(filas);
    let (total, abonado, devuelto, _) = montos(conn, credito_id).await?;
    let saldo = round2(total - abonado - devuelto);
    let monto = round2(monto);
    if monto > saldo + 0.005 {
        return Err(malo(format!(
            "Lo devuelto (S/ {:.2}) es mayor que la deuda pendiente (S/ {:.2}). Devuelve en efectivo.",
            monto, saldo
        )));
    }
    conn.execute(
        "UPDATE creditos SET devuelto = devuelto + ?1,
                estado = CASE WHEN total - abonado - (devuelto + ?1) <= 0.005 THEN 'PAGADO' ELSE 'PENDIENTE' END
         WHERE id = ?2",
        libsql::params![monto, credito_id],
    )
    .await
    .map_err(e500)?;
    Ok(())
}

/// Crédito de una venta con saldo pendiente: (saldo, vence). Lo usa la
/// factura para salir "al crédito" con su cuota. None si no hay, o si la
/// base aún no tiene la migración 0015.
pub async fn pendiente_de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<(f64, String)> {
    let mut filas = conn
        .query(
            "SELECT CAST(total - abonado - devuelto AS REAL), COALESCE(vence, '') FROM creditos WHERE venta_id = ?1",
            libsql::params![venta_id],
        )
        .await
        .ok()?;
    let f = filas.next().await.ok()??;
    let saldo = round2(f.get::<f64>(0).ok()?);
    if saldo <= 0.0 {
        return None;
    }
    let vence: String = f.get(1).unwrap_or_default();
    Some((saldo, if vence.is_empty() { hoy_lima_mas_dias(DIAS_POR_DEFECTO) } else { vence }))
}

// ============================================================
// Pantalla "Créditos"
// ============================================================

#[derive(Debug, Serialize, Clone)]
pub struct Abono {
    pub id: i64,
    pub monto: f64,
    pub metodo_pago: String,
    pub nota: Option<String>,
    pub usuario: String,
    pub fecha: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct LineaCredito {
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub total_linea: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Credito {
    pub id: i64,
    pub venta_id: i64,
    pub folio: String,
    pub cliente_id: i64,
    pub cliente_nombre: String,
    pub cliente_documento: Option<String>,
    pub cliente_telefono: Option<String>,
    pub total: f64,
    pub abonado: f64,
    pub devuelto: f64,
    pub saldo: f64,
    pub vence: Option<String>,
    /// PENDIENTE y ya pasó su fecha.
    pub vencido: bool,
    /// 'PENDIENTE' | 'PAGADO'
    pub estado: String,
    pub fecha: String,
    /// Solo en el detalle.
    pub abonos: Vec<Abono>,
    pub productos: Vec<LineaCredito>,
}

const COLUMNAS: &str = "cr.id, cr.venta_id, COALESCE(v.folio, ''), cr.cliente_id, COALESCE(c.nombre_razon_social, ''),
    c.numero_documento, c.telefono, CAST(cr.total AS REAL), CAST(cr.abonado AS REAL), CAST(cr.devuelto AS REAL),
    cr.vence, cr.estado, cr.fecha
    FROM creditos cr
    LEFT JOIN ventas v ON v.id = cr.venta_id
    LEFT JOIN clientes c ON c.id = cr.cliente_id";

fn de_fila(f: &libsql::Row, hoy: &str) -> Credito {
    let (total, abonado, devuelto): (f64, f64, f64) = (f.get(7).unwrap_or(0.0), f.get(8).unwrap_or(0.0), f.get(9).unwrap_or(0.0));
    let vence: Option<String> = f.get::<String>(10).ok();
    let estado: String = f.get(11).unwrap_or_default();
    Credito {
        id: f.get(0).unwrap_or_default(),
        venta_id: f.get(1).unwrap_or_default(),
        folio: f.get(2).unwrap_or_default(),
        cliente_id: f.get(3).unwrap_or_default(),
        cliente_nombre: f.get(4).unwrap_or_default(),
        cliente_documento: f.get::<String>(5).ok(),
        cliente_telefono: f.get::<String>(6).ok(),
        total,
        abonado,
        devuelto,
        saldo: round2(total - abonado - devuelto).max(0.0),
        vencido: estado == "PENDIENTE" && vence.as_deref().map(|v| v < hoy).unwrap_or(false),
        vence,
        estado,
        fecha: f.get(12).unwrap_or_default(),
        abonos: Vec::new(),
        productos: Vec::new(),
    }
}

#[derive(Debug, Serialize)]
pub struct ResumenCreditos {
    /// Suma de los saldos pendientes.
    pub por_cobrar: f64,
    /// De eso, lo que ya pasó su fecha.
    pub vencido: f64,
    pub clientes_con_deuda: i64,
    pub creditos: Vec<Credito>,
}

#[derive(Debug, Deserialize)]
pub struct Filtros {
    /// 'PENDIENTE' (por defecto) | 'PAGADO' | 'TODOS'
    pub estado: Option<String>,
    pub cliente_id: Option<i64>,
}

/// GET /creditos — cuentas por cobrar (pendientes primero las más antiguas).
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>, Query(filtros): Query<Filtros>) -> Resultado<ResumenCreditos> {
    let conn = tenant.0.connect().map_err(e500)?;
    let hoy = hoy_lima();
    let estado = filtros.estado.unwrap_or_else(|| "PENDIENTE".to_string());
    let mut sql = format!("SELECT {} WHERE 1 = 1", COLUMNAS);
    if estado == "PENDIENTE" || estado == "PAGADO" {
        sql.push_str(&format!(" AND cr.estado = '{}'", estado));
    }
    if let Some(cid) = filtros.cliente_id {
        sql.push_str(&format!(" AND cr.cliente_id = {}", cid));
    }
    sql.push_str(if estado == "PENDIENTE" { " ORDER BY COALESCE(cr.vence, '9999'), cr.id LIMIT 500" } else { " ORDER BY cr.id DESC LIMIT 300" });
    let mut filas = conn.query(&sql, ()).await.map_err(actualizando)?;
    let mut creditos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        creditos.push(de_fila(&f, &hoy));
    }
    drop(filas);

    // Los totales son de TODO lo pendiente, sin importar el filtro.
    let mut filas = conn
        .query(
            "SELECT CAST(COALESCE(SUM(total - abonado - devuelto), 0) AS REAL),
                    CAST(COALESCE(SUM(CASE WHEN vence < ?1 THEN total - abonado - devuelto ELSE 0 END), 0) AS REAL),
                    COUNT(DISTINCT cliente_id)
             FROM creditos WHERE estado = 'PENDIENTE'",
            libsql::params![hoy.clone()],
        )
        .await
        .map_err(e500)?;
    let (por_cobrar, vencido, clientes): (f64, f64, i64) = match filas.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or(0.0), f.get(1).unwrap_or(0.0), f.get(2).unwrap_or(0)),
        None => (0.0, 0.0, 0),
    };
    Ok(Json(ResumenCreditos { por_cobrar: round2(por_cobrar), vencido: round2(vencido), clientes_con_deuda: clientes, creditos }))
}

async fn detalle(conn: &libsql::Connection, id: i64) -> Result<Credito, (StatusCode, String)> {
    let hoy = hoy_lima();
    let mut filas = conn.query(&format!("SELECT {} WHERE cr.id = ?1", COLUMNAS), libsql::params![id]).await.map_err(actualizando)?;
    let mut credito = match filas.next().await.map_err(e500)? {
        Some(f) => de_fila(&f, &hoy),
        None => return Err((StatusCode::NOT_FOUND, "Ese crédito no existe.".to_string())),
    };
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT a.id, CAST(a.monto AS REAL), a.metodo_pago, a.nota, COALESCE(u.nombre_completo, ''), a.fecha
             FROM credito_abonos a LEFT JOIN usuarios u ON u.id = a.usuario_id WHERE a.credito_id = ?1 ORDER BY a.id",
            libsql::params![id],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        credito.abonos.push(Abono {
            id: f.get(0).unwrap_or_default(),
            monto: f.get(1).unwrap_or(0.0),
            metodo_pago: f.get(2).unwrap_or_default(),
            nota: f.get::<String>(3).ok(),
            usuario: f.get(4).unwrap_or_default(),
            fecha: f.get(5).unwrap_or_default(),
        });
    }
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT COALESCE(dv.nombre_producto, p.nombre, ''), CAST(dv.cantidad AS REAL), CAST(dv.precio_unitario AS REAL), CAST(dv.total_linea AS REAL)
             FROM detalles_venta dv LEFT JOIN productos p ON p.id = dv.producto_id WHERE dv.venta_id = ?1 ORDER BY dv.id",
            libsql::params![credito.venta_id],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        credito.productos.push(LineaCredito {
            nombre: f.get(0).unwrap_or_default(),
            cantidad: f.get(1).unwrap_or(0.0),
            precio_unitario: f.get(2).unwrap_or(0.0),
            total_linea: f.get(3).unwrap_or(0.0),
        });
    }
    Ok(credito)
}

/// GET /creditos/:id — con sus abonos y lo que se llevó.
pub async fn obtener(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Resultado<Credito> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(detalle(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct NuevoAbono {
    pub monto: f64,
    pub metodo_pago: String,
    #[serde(default)]
    pub nota: Option<String>,
}

/// POST /creditos/:id/abonos — el cliente paga todo o una parte.
pub async fn abonar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<NuevoAbono>,
) -> Resultado<Credito> {
    let conn = tenant.0.connect().map_err(e500)?;
    let nota = payload.nota.as_deref().map(str::trim).filter(|n| !n.is_empty());
    registrar_abono(&conn, id, payload.monto, &payload.metodo_pago, nota, claims.sub).await?;
    Ok(Json(detalle(&conn, id).await?))
}

#[derive(Debug, Serialize)]
pub struct DeudaCliente {
    pub cliente_id: i64,
    pub saldo: f64,
    pub creditos_pendientes: i64,
    pub vencido: f64,
}

/// GET /creditos/cliente/:id — cuánto debe ese cliente (lo muestra el punto
/// de venta antes de darle otro crédito).
pub async fn deuda_cliente(Extension(tenant): Extension<Arc<TenantDb>>, Path(cliente_id): Path<i64>) -> Resultado<DeudaCliente> {
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query(
            "SELECT CAST(COALESCE(SUM(total - abonado - devuelto), 0) AS REAL), COUNT(*),
                    CAST(COALESCE(SUM(CASE WHEN vence < ?2 THEN total - abonado - devuelto ELSE 0 END), 0) AS REAL)
             FROM creditos WHERE cliente_id = ?1 AND estado = 'PENDIENTE'",
            libsql::params![cliente_id, hoy_lima()],
        )
        .await
        .map_err(actualizando)?;
    let (saldo, cantidad, vencido): (f64, i64, f64) = match filas.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or(0.0), f.get(1).unwrap_or(0), f.get(2).unwrap_or(0.0)),
        None => (0.0, 0, 0.0),
    };
    Ok(Json(DeudaCliente { cliente_id, saldo: round2(saldo), creditos_pendientes: cantidad, vencido: round2(vencido) }))
}
