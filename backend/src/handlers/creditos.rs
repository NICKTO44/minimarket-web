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

    // El saldo se descuenta con la condición en la misma sentencia: si otro
    // abono (doble clic, otra caja) entró mientras tanto y ya no alcanza, no
    // se toca nada. Antes la revisión de arriba y la escritura iban por
    // separado y dos abonos simultáneos podían pasar los dos.
    let actualizados = conn
        .execute(
            "UPDATE creditos SET abonado = abonado + ?1,
                    estado = CASE WHEN total - (abonado + ?1) - devuelto <= 0.005 THEN 'PAGADO' ELSE 'PENDIENTE' END
             WHERE id = ?2 AND estado = 'PENDIENTE' AND total - abonado - devuelto >= ?1 - 0.005",
            libsql::params![monto, credito_id],
        )
        .await
        .map_err(e500)?;
    if actualizados == 0 {
        return Err((
            StatusCode::CONFLICT,
            "El saldo de este crédito cambió (¿se registró otro abono?). Vuelve a abrirlo para ver el saldo actual.".to_string(),
        ));
    }
    conn.execute(
        "INSERT INTO credito_abonos (credito_id, monto, metodo_pago, caja_id, nota, usuario_id, fecha) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        libsql::params![credito_id, monto, metodo, caja_id, nota.map(|n| n.chars().take(200).collect::<String>()), usuario_id, ahora_lima()],
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

/// Encargo (pedido de pastelería u orden de lavado) entregado al crédito:
/// lo que el cliente ya adelantó en el pedido entra como un abono ya pagado.
/// No pasa por la caja: ese dinero entró el día del adelanto (encargos.rs).
pub const METODO_ADELANTO_PEDIDO: &str = "ADELANTO";

pub async fn abonar_adelanto_previo(
    conn: &libsql::Connection,
    credito_id: i64,
    monto: f64,
    numero_pedido: i64,
    usuario_id: i64,
) -> Result<(), (StatusCode, String)> {
    let monto = round2(monto);
    if monto <= 0.0 {
        return Ok(());
    }
    let actualizados = conn
        .execute(
            "UPDATE creditos SET abonado = abonado + ?1,
                    estado = CASE WHEN total - (abonado + ?1) - devuelto <= 0.005 THEN 'PAGADO' ELSE 'PENDIENTE' END
             WHERE id = ?2 AND total - abonado - devuelto >= ?1 - 0.005",
            libsql::params![monto, credito_id],
        )
        .await
        .map_err(e500)?;
    if actualizados == 0 {
        return Err(malo("Lo adelantado en el pedido es mayor que el total de la venta."));
    }
    conn.execute(
        "INSERT INTO credito_abonos (credito_id, monto, metodo_pago, caja_id, nota, usuario_id, fecha) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
        libsql::params![
            credito_id,
            monto,
            METODO_ADELANTO_PEDIDO,
            format!("Adelanto del pedido N.° {}", numero_pedido),
            usuario_id,
            ahora_lima()
        ],
    )
    .await
    .map_err(e500)?;
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
    // El abono, su movimiento de caja y el saldo van juntos o no va nada.
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(e500)?;
    match registrar_abono(&tx, id, payload.monto, &payload.metodo_pago, nota, claims.sub).await {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    }
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

// ============================================================
// Cierre de mes: estado de cuenta del cliente y pago de varias cuentas
// ============================================================
//
// Clientes que se llevan a cuenta durante el mes (una lavandería con
// hoteles, una bodega con su fiado) y pagan todo a fin de mes. Funciona
// igual con nota de venta, boleta o factura: el crédito es de la venta.

#[derive(Debug, Deserialize)]
pub struct PeriodoEstado {
    /// AAAA-MM-DD. Con desde y hasta: las ventas a cuenta de esas fechas
    /// (pagadas o no) y el saldo anterior. Sin fechas: todo lo pendiente.
    pub desde: Option<String>,
    pub hasta: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EstadoCuenta {
    pub cliente_id: i64,
    pub cliente_nombre: String,
    pub cliente_documento: Option<String>,
    pub cliente_telefono: Option<String>,
    pub desde: Option<String>,
    pub hasta: Option<String>,
    /// Lo pendiente de ventas anteriores a `desde` (0 sin periodo).
    pub saldo_anterior: f64,
    /// Del periodo (o de lo pendiente): total, pagado y saldo.
    pub total: f64,
    pub pagado: f64,
    pub saldo: f64,
    /// Todo lo que debe hoy, sin importar la fecha.
    pub saldo_total: f64,
    pub vencido: f64,
    /// Cada venta a cuenta con lo que se llevó y sus pagos (la más antigua primero).
    pub creditos: Vec<Credito>,
}

fn fecha_valida(f: &str) -> bool {
    f.len() == 10 && chrono::NaiveDate::parse_from_str(f, "%Y-%m-%d").is_ok()
}

async fn cliente_de(conn: &libsql::Connection, cliente_id: i64) -> Result<(String, Option<String>, Option<String>), (StatusCode, String)> {
    let mut filas = conn
        .query("SELECT nombre_razon_social, numero_documento, telefono FROM clientes WHERE id = ?1", libsql::params![cliente_id])
        .await
        .map_err(e500)?;
    match filas.next().await.map_err(e500)? {
        Some(f) => Ok((f.get(0).unwrap_or_default(), f.get::<String>(1).ok(), f.get::<String>(2).ok())),
        None => Err((StatusCode::NOT_FOUND, "Ese cliente no existe.".to_string())),
    }
}

/// GET /creditos/estado-cuenta/:cliente_id?desde=&hasta= — para imprimirlo
/// o mandarlo por WhatsApp a fin de mes.
pub async fn estado_cuenta(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(cliente_id): Path<i64>,
    Query(periodo): Query<PeriodoEstado>,
) -> Resultado<EstadoCuenta> {
    let conn = tenant.0.connect().map_err(e500)?;
    conn.query("SELECT 1 FROM creditos LIMIT 1", ()).await.map_err(actualizando)?;
    let (cliente_nombre, cliente_documento, cliente_telefono) = cliente_de(&conn, cliente_id).await?;
    let rango = match (periodo.desde.as_deref(), periodo.hasta.as_deref()) {
        (Some(d), Some(h)) => {
            if !fecha_valida(d) || !fecha_valida(h) || d > h {
                return Err(malo("Revisa las fechas del estado de cuenta."));
            }
            Some((d.to_string(), h.to_string()))
        }
        (None, None) => None,
        _ => return Err(malo("Indica desde y hasta, o ninguna de las dos.")),
    };

    let (sql, params) = match &rango {
        Some((d, h)) => (
            format!(
                "SELECT {} WHERE cr.cliente_id = ?1 AND substr(cr.fecha, 1, 10) BETWEEN ?2 AND ?3 ORDER BY cr.fecha, cr.id LIMIT 500",
                COLUMNAS
            ),
            vec![libsql::Value::Integer(cliente_id), libsql::Value::Text(d.clone()), libsql::Value::Text(h.clone())],
        ),
        None => (
            format!("SELECT {} WHERE cr.cliente_id = ?1 AND cr.estado = 'PENDIENTE' ORDER BY cr.fecha, cr.id LIMIT 500", COLUMNAS),
            vec![libsql::Value::Integer(cliente_id)],
        ),
    };
    let mut ids = Vec::new();
    let mut filas = conn.query(&sql, libsql::params_from_iter(params)).await.map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        ids.push(f.get::<i64>(0).unwrap_or_default());
    }
    drop(filas);
    let mut creditos = Vec::with_capacity(ids.len());
    for id in ids {
        creditos.push(detalle(&conn, id).await?);
    }

    let saldo_anterior = match &rango {
        Some((d, _)) => {
            let mut filas = conn
                .query(
                    "SELECT CAST(COALESCE(SUM(total - abonado - devuelto), 0) AS REAL) FROM creditos
                     WHERE cliente_id = ?1 AND estado = 'PENDIENTE' AND substr(fecha, 1, 10) < ?2",
                    libsql::params![cliente_id, d.clone()],
                )
                .await
                .map_err(e500)?;
            filas.next().await.map_err(e500)?.and_then(|f| f.get::<f64>(0).ok()).unwrap_or(0.0)
        }
        None => 0.0,
    };
    let Json(deuda) = deuda_cliente(Extension(tenant.clone()), Path(cliente_id)).await?;
    let total = round2(creditos.iter().map(|c| c.total).sum());
    let saldo = round2(creditos.iter().map(|c| c.saldo).sum());
    Ok(Json(EstadoCuenta {
        cliente_id,
        cliente_nombre,
        cliente_documento,
        cliente_telefono,
        desde: rango.as_ref().map(|r| r.0.clone()),
        hasta: rango.as_ref().map(|r| r.1.clone()),
        saldo_anterior: round2(saldo_anterior),
        total,
        pagado: round2(total - saldo),
        saldo,
        saldo_total: deuda.saldo,
        vencido: deuda.vencido,
        creditos,
    }))
}

#[derive(Debug, Serialize)]
pub struct PagoAplicado {
    pub credito_id: i64,
    pub folio: String,
    pub monto: f64,
    pub saldo_restante: f64,
}

#[derive(Debug, Serialize)]
pub struct ResultadoPagoCliente {
    pub pagado: f64,
    pub aplicado: Vec<PagoAplicado>,
    /// Lo que sigue debiendo después del pago.
    pub saldo_total: f64,
}

/// POST /creditos/cliente/:id/pagar — el cliente paga varias cuentas de una
/// vez (el cierre de mes). Se reparte desde la más antigua; cada parte es un
/// abono normal (en efectivo entra a la caja). Todo o nada.
pub async fn pagar_cliente(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(cliente_id): Path<i64>,
    Json(payload): Json<NuevoAbono>,
) -> Resultado<ResultadoPagoCliente> {
    let monto = round2(payload.monto);
    if !(monto > 0.0) || !monto.is_finite() {
        return Err(malo("El monto del pago debe ser mayor a 0."));
    }
    if !METODOS_ABONO.contains(&payload.metodo_pago.as_str()) {
        return Err(malo("Método de pago no válido."));
    }
    let nota = payload.nota.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let conn = tenant.0.connect().map_err(e500)?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(e500)?;
    let resultado: Result<_, (StatusCode, String)> = async {
        let mut pendientes: Vec<(i64, String, f64)> = Vec::new();
        let mut filas = tx
            .query(
                "SELECT cr.id, COALESCE(v.folio, ''), CAST(cr.total - cr.abonado - cr.devuelto AS REAL)
                 FROM creditos cr LEFT JOIN ventas v ON v.id = cr.venta_id
                 WHERE cr.cliente_id = ?1 AND cr.estado = 'PENDIENTE' ORDER BY cr.fecha, cr.id",
                libsql::params![cliente_id],
            )
            .await
            .map_err(actualizando)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let saldo = round2(f.get::<f64>(2).unwrap_or(0.0));
            if saldo > 0.0 {
                pendientes.push((f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default(), saldo));
            }
        }
        drop(filas);
        let deuda = round2(pendientes.iter().map(|p| p.2).sum());
        if deuda <= 0.0 {
            return Err((StatusCode::CONFLICT, "Este cliente no tiene cuentas por pagar.".to_string()));
        }
        if monto > deuda + 0.005 {
            return Err(malo(format!("El pago (S/ {:.2}) es mayor que lo que debe (S/ {:.2}).", monto, deuda)));
        }
        let mut queda = monto;
        let mut aplicado = Vec::new();
        for (id, folio, saldo) in pendientes {
            if queda <= 0.005 {
                break;
            }
            let parte = round2(queda.min(saldo));
            registrar_abono(&tx, id, parte, &payload.metodo_pago, nota.or(Some("Pago de cuenta")), claims.sub).await?;
            queda = round2(queda - parte);
            aplicado.push(PagoAplicado { credito_id: id, folio, monto: parte, saldo_restante: round2(saldo - parte) });
        }
        Ok((aplicado, round2(deuda - monto)))
    }
    .await;
    match resultado {
        Ok((aplicado, saldo_total)) => {
            tx.commit().await.map_err(e500)?;
            Ok(Json(ResultadoPagoCliente { pagado: monto, aplicado, saldo_total: saldo_total.max(0.0) }))
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}
