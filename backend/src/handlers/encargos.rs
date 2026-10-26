//! Módulo ENCARGOS (migración 0042): pedidos por encargo. Pensado para el
//! rubro Pastelería / Panadería (tortas personalizadas, bocaditos para un
//! evento), pero lo puede encender cualquier negocio.
//!
//! Flujo:
//!   1. Se registra el pedido: cliente, fecha y hora de entrega, recojo o
//!      delivery, lo que lleva (con el precio acordado y el detalle: sabor,
//!      relleno...), dedicatoria y, si se quiere, una foto del diseño.
//!   2. El cliente deja un ADELANTO (uno o varios). No es una venta ni lleva
//!      comprobante: queda en el historial del pedido y, si es en efectivo,
//!      entra a la caja abierta como ingreso (así el arqueo cuadra).
//!   3. El pedido pasa por PENDIENTE → EN_PREPARACION → LISTO.
//!   4. Al entregarlo se cobra en el punto de venta como una venta normal por
//!      el TOTAL: ahí sale la boleta o factura completa y el stock. El
//!      adelanto ya recibido se descuenta y solo se cobra el saldo: la venta
//!      se guarda como MIXTO con la parte ya pagada en 'ADELANTO' (la caja no
//!      la vuelve a contar). El pedido queda ENTREGADO con su venta, dentro
//!      de la misma transacción.
//!   5. Si se cancela, se puede devolver lo adelantado (en efectivo sale de
//!      la caja) o dejarlo retenido.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET  /encargos?estado=&cliente_id=&buscar=  -> ACTIVOS (por defecto), ENTREGADOS, CANCELADOS o TODOS
//!   POST /encargos                              -> nuevo (con adelanto opcional)
//!   GET  /encargos/:id                          -> con ítems y pagos
//!   PUT  /encargos/:id                          -> cambia datos e ítems
//!   POST /encargos/:id/estado                   -> PENDIENTE, EN_PREPARACION o LISTO
//!   POST /encargos/:id/adelantos                -> otro adelanto
//!   POST /encargos/:id/cancelar                 -> cancela (y devuelve o no lo adelantado)
//!   POST /encargos/:id/foto, DELETE /encargos/:id/foto

use axum::{
    extract::{Extension, Multipart, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_ENCARGOS};
use crate::logica::tiempo;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Pedidos por encargo";
const MAX_ITEMS: usize = 50;
/// Valor de ventas.pago_otro_metodo con la parte que el cliente ya pagó de adelanto.
pub const METODO_ADELANTO: &str = "ADELANTO";
const METODOS: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE", "PLIN"];
const ACTIVOS: &[&str] = &["PENDIENTE", "EN_PREPARACION", "LISTO"];

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") || texto.contains("no such column") {
        (StatusCode::SERVICE_UNAVAILABLE, "Los pedidos por encargo aún no están listos en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, Fallo> {
    let limpio = valor.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(malo(format!("{}: máximo {} letras.", campo, max))),
        otro => Ok(otro),
    }
}
fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_ENCARGOS, NOMBRE_MODULO).await
}

// ---------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ItemEncargo {
    pub id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub total: f64,
    pub detalle: Option<String>,
    /// Del producto, para cargarlo en el carrito.
    pub unidad_medida: String,
    pub controla_stock: bool,
}

#[derive(Debug, Serialize)]
pub struct PagoEncargo {
    pub id: i64,
    pub tipo: String,
    pub monto: f64,
    pub metodo_pago: String,
    pub nota: Option<String>,
    pub usuario: Option<String>,
    pub fecha: String,
}

#[derive(Debug, Serialize)]
pub struct Encargo {
    pub id: i64,
    pub numero: i64,
    pub cliente_id: Option<i64>,
    pub cliente_nombre: String,
    pub cliente_telefono: Option<String>,
    pub cliente_documento: Option<String>,
    pub fecha_entrega: String,
    pub hora_entrega: Option<String>,
    pub entrega: String,
    pub direccion_entrega: Option<String>,
    pub dedicatoria: Option<String>,
    pub notas: Option<String>,
    pub foto_url: Option<String>,
    pub total: f64,
    pub adelantado: f64,
    pub devuelto: f64,
    /// Lo que falta cobrar: total − (adelantado − devuelto).
    pub saldo: f64,
    pub estado: String,
    pub venta_id: Option<i64>,
    pub folio_venta: Option<String>,
    pub motivo_cancelacion: Option<String>,
    pub usuario: Option<String>,
    pub fecha_creacion: Option<String>,
    pub fecha_entregado: Option<String>,
    pub items: Vec<ItemEncargo>,
    /// Solo en el detalle (GET /encargos/:id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagos: Option<Vec<PagoEncargo>>,
}

const SELECT_ENCARGO: &str = "SELECT e.id, e.numero, e.cliente_id, e.cliente_nombre, e.cliente_telefono, c.numero_documento,
        e.fecha_entrega, e.hora_entrega, e.entrega, e.direccion_entrega, e.dedicatoria, e.notas, e.foto_url,
        CAST(e.total AS REAL), CAST(e.adelantado AS REAL), CAST(e.devuelto AS REAL), e.estado, e.venta_id, v.folio,
        e.motivo_cancelacion, u.nombre_completo, e.fecha_creacion, e.fecha_entregado
     FROM encargos e
     LEFT JOIN clientes c ON c.id = e.cliente_id
     LEFT JOIN ventas v ON v.id = e.venta_id
     LEFT JOIN usuarios u ON u.id = e.usuario_id";

fn encargo_de(f: &libsql::Row) -> Encargo {
    let total: f64 = f.get(13).unwrap_or(0.0);
    let adelantado: f64 = f.get(14).unwrap_or(0.0);
    let devuelto: f64 = f.get(15).unwrap_or(0.0);
    Encargo {
        id: f.get(0).unwrap_or_default(),
        numero: f.get(1).unwrap_or_default(),
        cliente_id: f.get(2).ok(),
        cliente_nombre: f.get(3).unwrap_or_default(),
        cliente_telefono: f.get(4).ok(),
        cliente_documento: f.get(5).ok(),
        fecha_entrega: f.get(6).unwrap_or_default(),
        hora_entrega: f.get(7).ok(),
        entrega: f.get(8).unwrap_or_else(|_| "RECOJO".to_string()),
        direccion_entrega: f.get(9).ok(),
        dedicatoria: f.get(10).ok(),
        notas: f.get(11).ok(),
        foto_url: f.get(12).ok(),
        total,
        adelantado,
        devuelto,
        saldo: r2((total - (adelantado - devuelto)).max(0.0)),
        estado: f.get(16).unwrap_or_default(),
        venta_id: f.get(17).ok(),
        folio_venta: f.get(18).ok(),
        motivo_cancelacion: f.get(19).ok(),
        usuario: f.get(20).ok(),
        fecha_creacion: f.get(21).ok(),
        fecha_entregado: f.get(22).ok(),
        items: Vec::new(),
        pagos: None,
    }
}

/// Ítems de varios pedidos a la vez (una sola consulta).
async fn items_de(conn: &libsql::Connection, ids: &[i64]) -> Result<HashMap<i64, Vec<ItemEncargo>>, Fallo> {
    let mut mapa: HashMap<i64, Vec<ItemEncargo>> = HashMap::new();
    if ids.is_empty() {
        return Ok(mapa);
    }
    let marcas = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let mut filas = conn
        .query(
            &format!(
                "SELECT i.encargo_id, i.id, i.producto_id, i.nombre, CAST(i.cantidad AS REAL), CAST(i.precio_unitario AS REAL), i.detalle,
                        COALESCE(p.unidad_medida, 'UNIDAD'), COALESCE(p.controla_stock, 1)
                 FROM encargo_items i LEFT JOIN productos p ON p.id = i.producto_id
                 WHERE i.encargo_id IN ({}) ORDER BY i.id",
                marcas
            ),
            (),
        )
        .await
        .map_err(sin_migracion)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(4).unwrap_or(0.0);
        let precio: f64 = f.get(5).unwrap_or(0.0);
        mapa.entry(f.get(0).unwrap_or_default()).or_default().push(ItemEncargo {
            id: f.get(1).unwrap_or_default(),
            producto_id: f.get(2).unwrap_or_default(),
            nombre: f.get(3).unwrap_or_default(),
            cantidad,
            precio_unitario: precio,
            total: r2(precio * cantidad),
            detalle: f.get(6).ok(),
            unidad_medida: f.get(7).unwrap_or_else(|_| "UNIDAD".to_string()),
            controla_stock: f.get::<i64>(8).unwrap_or(1) == 1,
        });
    }
    Ok(mapa)
}

async fn leer(conn: &libsql::Connection, id: i64) -> Result<Encargo, Fallo> {
    let mut filas = conn
        .query(&format!("{} WHERE e.id = ?1", SELECT_ENCARGO), libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let mut encargo = match filas.next().await.map_err(e500)? {
        Some(f) => encargo_de(&f),
        None => return Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
    };
    drop(filas);
    encargo.items = items_de(conn, &[id]).await?.remove(&id).unwrap_or_default();
    let mut filas = conn
        .query(
            "SELECT p.id, p.tipo, CAST(p.monto AS REAL), p.metodo_pago, p.nota, u.nombre_completo, COALESCE(p.fecha, '')
             FROM encargo_pagos p LEFT JOIN usuarios u ON u.id = p.usuario_id
             WHERE p.encargo_id = ?1 ORDER BY p.id",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let mut pagos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        pagos.push(PagoEncargo {
            id: f.get(0).unwrap_or_default(),
            tipo: f.get(1).unwrap_or_default(),
            monto: f.get(2).unwrap_or(0.0),
            metodo_pago: f.get(3).unwrap_or_default(),
            nota: f.get(4).ok(),
            usuario: f.get(5).ok(),
            fecha: f.get(6).unwrap_or_default(),
        });
    }
    encargo.pagos = Some(pagos);
    Ok(encargo)
}

#[derive(Debug, Deserialize)]
pub struct FiltroEncargos {
    #[serde(default)]
    pub estado: Option<String>,
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub buscar: Option<String>,
}

/// GET /encargos — los activos ordenados por fecha de entrega (los
/// atrasados primero); los entregados y cancelados, los más recientes.
pub async fn listar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(q): Query<FiltroEncargos>,
) -> Result<Json<Vec<Encargo>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (donde, orden, limite) = match q.estado.as_deref().unwrap_or("ACTIVOS") {
        "ACTIVOS" => ("e.estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')", "e.fecha_entrega, COALESCE(e.hora_entrega, '99'), e.id", 500),
        "ENTREGADOS" => ("e.estado = 'ENTREGADO'", "e.fecha_entrega DESC, e.id DESC", 200),
        "CANCELADOS" => ("e.estado = 'CANCELADO'", "e.id DESC", 200),
        "TODOS" => ("1", "e.id DESC", 300),
        _ => return Err(malo("Estado no válido.")),
    };
    let mut condiciones = vec![donde.to_string()];
    let mut valores: Vec<libsql::Value> = Vec::new();
    if let Some(cliente) = q.cliente_id {
        valores.push(cliente.into());
        condiciones.push(format!("e.cliente_id = ?{}", valores.len()));
    }
    if let Some(buscar) = q.buscar.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        let patron = format!("%{}%", buscar.chars().take(60).collect::<String>().to_lowercase());
        valores.push(patron.into());
        let n = valores.len();
        condiciones.push(format!(
            "(lower(e.cliente_nombre) LIKE ?{n} OR COALESCE(e.cliente_telefono, '') LIKE ?{n} OR CAST(e.numero AS TEXT) = ?{m})",
            n = n,
            m = n + 1
        ));
        valores.push(buscar.trim_start_matches('#').to_string().into());
    }
    let sql = format!("{} WHERE {} ORDER BY {} LIMIT {}", SELECT_ENCARGO, condiciones.join(" AND "), orden, limite);
    let mut filas = conn.query(&sql, libsql::params_from_iter(valores)).await.map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(encargo_de(&f));
    }
    drop(filas);
    let ids: Vec<i64> = lista.iter().map(|e| e.id).collect();
    let mut items = items_de(&conn, &ids).await?;
    for e in lista.iter_mut() {
        e.items = items.remove(&e.id).unwrap_or_default();
    }
    Ok(Json(lista))
}

/// GET /encargos/:id
pub async fn obtener(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer(&conn, id).await?))
}

// ---------------------------------------------------------------------
// Crear y editar
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ItemNuevo {
    pub producto_id: i64,
    pub cantidad: f64,
    pub precio_unitario: f64,
    #[serde(default)]
    pub detalle: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Adelanto {
    pub monto: f64,
    #[serde(default)]
    pub metodo_pago: Option<String>,
    #[serde(default)]
    pub nota: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DatosEncargo {
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub cliente_nombre: Option<String>,
    #[serde(default)]
    pub cliente_telefono: Option<String>,
    pub fecha_entrega: String,
    #[serde(default)]
    pub hora_entrega: Option<String>,
    #[serde(default)]
    pub entrega: Option<String>,
    #[serde(default)]
    pub direccion_entrega: Option<String>,
    #[serde(default)]
    pub dedicatoria: Option<String>,
    #[serde(default)]
    pub notas: Option<String>,
    pub items: Vec<ItemNuevo>,
    /// Solo al crear: lo que deja el cliente al hacer el pedido.
    #[serde(default)]
    pub adelanto: Option<Adelanto>,
}

/// Datos revisados, listos para guardar.
struct Listo {
    cliente_id: Option<i64>,
    cliente_nombre: String,
    cliente_telefono: Option<String>,
    fecha_entrega: String,
    hora_entrega: Option<String>,
    entrega: String,
    direccion_entrega: Option<String>,
    dedicatoria: Option<String>,
    notas: Option<String>,
    /// (producto_id, nombre, cantidad, precio, detalle)
    items: Vec<(i64, String, f64, f64, Option<String>)>,
    total: f64,
}

fn hora_valida(h: &str) -> bool {
    let partes: Vec<&str> = h.split(':').collect();
    partes.len() == 2
        && partes[0].len() == 2
        && partes[1].len() == 2
        && partes[0].parse::<u32>().is_ok_and(|x| x < 24)
        && partes[1].parse::<u32>().is_ok_and(|x| x < 60)
}

/// Revisa lo que manda la pantalla (sin escribir nada). Los nombres de los
/// productos y del cliente se toman de la base.
async fn revisar(conn: &libsql::Connection, d: &DatosEncargo, es_nuevo: bool) -> Result<Listo, Fallo> {
    let fecha = d.fecha_entrega.trim().to_string();
    if !tiempo::fecha_valida(&fecha) {
        return Err(malo("Elige la fecha de entrega."));
    }
    if es_nuevo && fecha < tiempo::hoy_lima() {
        return Err(malo("La fecha de entrega ya pasó."));
    }
    let hora = texto(d.hora_entrega.as_deref(), 5, "Hora")?;
    if hora.as_deref().is_some_and(|h| !hora_valida(h)) {
        return Err(malo("La hora de entrega no es válida (HH:MM)."));
    }
    let entrega = d.entrega.as_deref().unwrap_or("RECOJO").trim().to_uppercase();
    if entrega != "RECOJO" && entrega != "DELIVERY" {
        return Err(malo("La entrega debe ser RECOJO o DELIVERY."));
    }
    let direccion = texto(d.direccion_entrega.as_deref(), 250, "Dirección")?;
    if entrega == "DELIVERY" && direccion.is_none() {
        return Err(malo("Para delivery escribe la dirección de entrega."));
    }
    let dedicatoria = texto(d.dedicatoria.as_deref(), 200, "Dedicatoria")?;
    let notas = texto(d.notas.as_deref(), 1000, "Notas")?;
    let mut telefono = texto(d.cliente_telefono.as_deref(), 20, "Teléfono")?;
    let mut nombre = texto(d.cliente_nombre.as_deref(), 150, "Nombre del cliente")?;

    if let Some(id) = d.cliente_id {
        let mut f = conn
            .query("SELECT nombre_razon_social, telefono FROM clientes WHERE id = ?1", libsql::params![id])
            .await
            .map_err(e500)?;
        match f.next().await.map_err(e500)? {
            Some(c) => {
                nombre = nombre.or_else(|| c.get::<String>(0).ok());
                telefono = telefono.or_else(|| c.get::<String>(1).ok().filter(|t| !t.trim().is_empty()));
            }
            None => return Err(malo("El cliente elegido no existe.")),
        }
    }
    let cliente_nombre = nombre.ok_or_else(|| malo("Escribe el nombre del cliente."))?;

    if d.items.is_empty() {
        return Err(malo("Agrega lo que lleva el pedido."));
    }
    if d.items.len() > MAX_ITEMS {
        return Err(malo(format!("Un pedido puede llevar hasta {} productos.", MAX_ITEMS)));
    }
    let ids: Vec<String> = d.items.iter().map(|i| i.producto_id.to_string()).collect();
    let mut nombres: HashMap<i64, String> = HashMap::new();
    let mut filas = conn
        .query(
            &format!("SELECT id, nombre FROM productos WHERE activo = 1 AND id IN ({})", ids.join(",")),
            (),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        nombres.insert(f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default());
    }
    drop(filas);
    let mut items = Vec::new();
    let mut total = 0.0;
    for i in &d.items {
        let nombre = nombres
            .get(&i.producto_id)
            .cloned()
            .ok_or_else(|| malo("Un producto del pedido no existe o está desactivado. Quítalo y vuelve a agregarlo."))?;
        if !(i.cantidad > 0.0) || !i.cantidad.is_finite() || i.cantidad > 100_000.0 {
            return Err(malo(format!("La cantidad de {} no es válida.", nombre)));
        }
        if !(i.precio_unitario >= 0.0) || !i.precio_unitario.is_finite() || i.precio_unitario > 1_000_000.0 {
            return Err(malo(format!("El precio de {} no es válido.", nombre)));
        }
        let detalle = texto(i.detalle.as_deref(), 500, "Detalle")?;
        let precio = (i.precio_unitario * 1_000_000.0).round() / 1_000_000.0;
        total += r2(precio * i.cantidad);
        items.push((i.producto_id, nombre, i.cantidad, precio, detalle));
    }
    Ok(Listo {
        cliente_id: d.cliente_id,
        cliente_nombre,
        cliente_telefono: telefono,
        fecha_entrega: fecha,
        hora_entrega: hora,
        entrega: entrega.clone(),
        direccion_entrega: if entrega == "DELIVERY" { direccion } else { None },
        dedicatoria,
        notas,
        items,
        total: r2(total),
    })
}

async fn guardar_items(conn: &libsql::Connection, id: i64, items: &[(i64, String, f64, f64, Option<String>)]) -> Result<(), Fallo> {
    conn.execute("DELETE FROM encargo_items WHERE encargo_id = ?1", libsql::params![id]).await.map_err(e500)?;
    for (producto_id, nombre, cantidad, precio, detalle) in items {
        conn.execute(
            "INSERT INTO encargo_items (encargo_id, producto_id, nombre, cantidad, precio_unitario, detalle) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![id, *producto_id, nombre.clone(), *cantidad, *precio, detalle.clone()],
        )
        .await
        .map_err(e500)?;
    }
    Ok(())
}

/// La caja abierta del negocio (primero la de quien cobra), como al vender.
async fn caja_abierta(conn: &libsql::Connection, usuario_id: i64) -> Result<Option<i64>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY (usuario_id = ?1) DESC, id DESC LIMIT 1",
            libsql::params![usuario_id],
        )
        .await
        .map_err(e500)?;
    Ok(filas.next().await.map_err(e500)?.and_then(|f| f.get::<i64>(0).ok()))
}

fn metodo_valido(metodo: Option<&str>) -> Result<String, Fallo> {
    let m = metodo.unwrap_or("EFECTIVO").trim().to_uppercase();
    let m = if m == "YAPE_PLIN" { "YAPE".to_string() } else { m };
    if METODOS.contains(&m.as_str()) {
        Ok(m)
    } else {
        Err(malo("Método de pago no válido."))
    }
}

/// Registra un adelanto (dentro de la transacción de quien llama). El
/// saldo se revisa en la misma sentencia que lo suma: dos adelantos a la vez
/// no pueden pasar del total. En efectivo exige caja abierta y entra a ella
/// como ingreso.
async fn registrar_adelanto(conn: &libsql::Connection, id: i64, numero: i64, cliente: &str, a: &Adelanto, usuario_id: i64) -> Result<(), Fallo> {
    let monto = r2(a.monto);
    if !(monto > 0.0) || !monto.is_finite() {
        return Err(malo("El adelanto debe ser mayor a 0."));
    }
    let metodo = metodo_valido(a.metodo_pago.as_deref())?;
    let nota = texto(a.nota.as_deref(), 200, "Nota")?;
    let caja = if metodo == "EFECTIVO" {
        Some(caja_abierta(conn, usuario_id).await?.ok_or_else(|| malo("Abre una caja para recibir el adelanto en efectivo."))?)
    } else {
        None
    };
    let n = conn
        .execute(
            "UPDATE encargos SET adelantado = adelantado + ?1, fecha_actualizacion = ?2
             WHERE id = ?3 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')
               AND total - (adelantado - devuelto) >= ?1 - 0.005",
            libsql::params![monto, tiempo::ahora_lima(), id],
        )
        .await
        .map_err(e500)?;
    if n == 0 {
        return Err((
            StatusCode::CONFLICT,
            "El adelanto es mayor que lo que falta pagar, o el pedido ya se entregó o canceló.".into(),
        ));
    }
    conn.execute(
        "INSERT INTO encargo_pagos (encargo_id, tipo, monto, metodo_pago, caja_id, nota, usuario_id, fecha)
         VALUES (?1, 'ADELANTO', ?2, ?3, ?4, ?5, ?6, ?7)",
        libsql::params![id, monto, metodo.clone(), caja, nota, usuario_id, tiempo::ahora_lima()],
    )
    .await
    .map_err(e500)?;
    if let Some(caja) = caja {
        let motivo = format!("Adelanto pedido N.° {} · {}", numero, cliente).chars().take(200).collect::<String>();
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

/// Corre `trabajo` en una transacción: o se guarda todo o nada.
macro_rules! en_transaccion {
    ($conn:expr, |$tx:ident| $cuerpo:block) => {{
        let tx = $conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
        let resultado: Result<_, Fallo> = async {
            let $tx: &libsql::Connection = &tx;
            $cuerpo
        }
        .await;
        match resultado {
            Ok(v) => {
                tx.commit().await.map_err(e500)?;
                v
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(e);
            }
        }
    }};
}

/// POST /encargos — pedido nuevo (con su adelanto, si dejó).
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(d): Json<DatosEncargo>,
) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let l = revisar(&conn, &d, true).await?;
    if let Some(a) = &d.adelanto {
        if r2(a.monto) > l.total + 0.005 {
            return Err(malo(format!("El adelanto (S/ {:.2}) es mayor que el total (S/ {:.2}).", a.monto, l.total)));
        }
    }
    let id = en_transaccion!(conn, |tx| {
        let ahora = tiempo::ahora_lima();
        tx.execute(
            "INSERT INTO encargos (numero, cliente_id, cliente_nombre, cliente_telefono, fecha_entrega, hora_entrega, entrega,
                                   direccion_entrega, dedicatoria, notas, total, estado, usuario_id, fecha_creacion, fecha_actualizacion)
             VALUES ((SELECT COALESCE(MAX(numero), 0) + 1 FROM encargos), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'PENDIENTE', ?11, ?12, ?12)",
            libsql::params![
                l.cliente_id, l.cliente_nombre.clone(), l.cliente_telefono.clone(), l.fecha_entrega.clone(), l.hora_entrega.clone(),
                l.entrega.clone(), l.direccion_entrega.clone(), l.dedicatoria.clone(), l.notas.clone(), l.total, claims.sub, ahora
            ],
        )
        .await
        .map_err(sin_migracion)?;
        let id = tx.last_insert_rowid();
        guardar_items(tx, id, &l.items).await?;
        if let Some(a) = d.adelanto.as_ref().filter(|a| a.monto > 0.0) {
            let mut f = tx.query("SELECT numero FROM encargos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
            let numero: i64 = f.next().await.map_err(e500)?.and_then(|f| f.get(0).ok()).unwrap_or_default();
            drop(f);
            registrar_adelanto(tx, id, numero, &l.cliente_nombre, a, claims.sub).await?;
        }
        Ok(id)
    });
    Ok(Json(leer(&conn, id).await?))
}

async fn estado_y_numero(conn: &libsql::Connection, id: i64) -> Result<(String, i64, String), Fallo> {
    let mut filas = conn
        .query("SELECT estado, numero, cliente_nombre FROM encargos WHERE id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    match filas.next().await.map_err(e500)? {
        Some(f) => Ok((f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default(), f.get(2).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
    }
}

fn activo(estado: &str) -> Result<(), Fallo> {
    match estado {
        "ENTREGADO" => Err((StatusCode::CONFLICT, "El pedido ya se entregó: no se puede cambiar.".into())),
        "CANCELADO" => Err((StatusCode::CONFLICT, "El pedido está cancelado.".into())),
        _ => Ok(()),
    }
}

/// PUT /encargos/:id — cambia los datos y lo que lleva. El total no puede
/// quedar por debajo de lo ya adelantado.
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(d): Json<DatosEncargo>,
) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado, _, _) = estado_y_numero(&conn, id).await?;
    activo(&estado)?;
    let l = revisar(&conn, &d, false).await?;
    en_transaccion!(conn, |tx| {
        let n = tx
            .execute(
                "UPDATE encargos SET cliente_id = ?1, cliente_nombre = ?2, cliente_telefono = ?3, fecha_entrega = ?4, hora_entrega = ?5,
                        entrega = ?6, direccion_entrega = ?7, dedicatoria = ?8, notas = ?9, total = ?10, fecha_actualizacion = ?11
                 WHERE id = ?12 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO') AND adelantado - devuelto <= ?10 + 0.005",
                libsql::params![
                    l.cliente_id, l.cliente_nombre.clone(), l.cliente_telefono.clone(), l.fecha_entrega.clone(), l.hora_entrega.clone(),
                    l.entrega.clone(), l.direccion_entrega.clone(), l.dedicatoria.clone(), l.notas.clone(), l.total, tiempo::ahora_lima(), id
                ],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            return Err((
                StatusCode::CONFLICT,
                format!("El total (S/ {:.2}) no puede ser menor que lo que el cliente ya adelantó.", l.total),
            ));
        }
        guardar_items(tx, id, &l.items).await?;
        Ok(())
    });
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct CambioEstado {
    pub estado: String,
}

/// POST /encargos/:id/estado — PENDIENTE, EN_PREPARACION o LISTO. ENTREGADO
/// solo se llega cobrándolo en el punto de venta; CANCELADO, con /cancelar.
pub async fn cambiar_estado(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(c): Json<CambioEstado>,
) -> Result<Json<Encargo>, Fallo> {
    let estado = c.estado.trim().to_uppercase();
    if !ACTIVOS.contains(&estado.as_str()) {
        return Err(malo("Para entregarlo, cóbralo en el punto de venta; para cancelarlo, usa Cancelar."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let n = conn
        .execute(
            "UPDATE encargos SET estado = ?1, fecha_actualizacion = ?2 WHERE id = ?3 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')",
            libsql::params![estado, tiempo::ahora_lima(), id],
        )
        .await
        .map_err(sin_migracion)?;
    if n == 0 {
        let (actual, _, _) = estado_y_numero(&conn, id).await?;
        activo(&actual)?;
    }
    Ok(Json(leer(&conn, id).await?))
}

/// POST /encargos/:id/adelantos — otro adelanto (o el saldo completo antes
/// de la entrega).
pub async fn agregar_adelanto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(a): Json<Adelanto>,
) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado, numero, cliente) = estado_y_numero(&conn, id).await?;
    activo(&estado)?;
    en_transaccion!(conn, |tx| { registrar_adelanto(tx, id, numero, &cliente, &a, claims.sub).await });
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct Cancelacion {
    pub motivo: String,
    /// true: se le devuelve al cliente lo adelantado; false: queda retenido.
    #[serde(default)]
    pub devolver: bool,
    #[serde(default)]
    pub metodo_devolucion: Option<String>,
}

/// POST /encargos/:id/cancelar — en efectivo, la devolución sale de la caja
/// abierta (como retiro) y solo si alcanza el efectivo.
pub async fn cancelar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(c): Json<Cancelacion>,
) -> Result<Json<Encargo>, Fallo> {
    let motivo = texto(Some(&c.motivo), 250, "Motivo")?.ok_or_else(|| malo("Escribe el motivo de la cancelación."))?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado, numero, cliente) = estado_y_numero(&conn, id).await?;
    activo(&estado)?;
    let metodo = if c.devolver { Some(metodo_valido(c.metodo_devolucion.as_deref())?) } else { None };
    en_transaccion!(conn, |tx| {
        let mut f = tx
            .query("SELECT CAST(adelantado - devuelto AS REAL) FROM encargos WHERE id = ?1", libsql::params![id])
            .await
            .map_err(e500)?;
        let pagado = r2(f.next().await.map_err(e500)?.and_then(|f| f.get::<f64>(0).ok()).unwrap_or(0.0));
        drop(f);
        let devolver = if metodo.is_some() { pagado } else { 0.0 };
        let n = tx
            .execute(
                "UPDATE encargos SET estado = 'CANCELADO', motivo_cancelacion = ?1, devuelto = devuelto + ?2, fecha_actualizacion = ?3
                 WHERE id = ?4 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')",
                libsql::params![motivo.clone(), devolver, tiempo::ahora_lima(), id],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            return Err((StatusCode::CONFLICT, "El pedido ya se entregó o se canceló.".into()));
        }
        if let (Some(metodo), true) = (metodo.as_deref(), devolver > 0.0) {
            let mut caja_id = None;
            if metodo == "EFECTIVO" {
                let caja = caja_abierta(tx, claims.sub).await?.ok_or_else(|| malo("Abre una caja para devolver el adelanto en efectivo."))?;
                let disponible = crate::handlers::cajas::efectivo_en_caja(tx, caja).await?;
                crate::handlers::cajas::alcanza_efectivo(disponible, devolver).map_err(malo)?;
                let detalle = format!("Devolución adelanto pedido N.° {} · {}", numero, cliente).chars().take(200).collect::<String>();
                tx.execute(
                    "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, 'RETIRO', ?2, ?3, ?4)",
                    libsql::params![caja, devolver, detalle, claims.sub],
                )
                .await
                .map_err(e500)?;
                tx.execute("UPDATE cajas SET retiros_total = retiros_total + ?1 WHERE id = ?2", libsql::params![devolver, caja])
                    .await
                    .map_err(e500)?;
                caja_id = Some(caja);
            }
            tx.execute(
                "INSERT INTO encargo_pagos (encargo_id, tipo, monto, metodo_pago, caja_id, nota, usuario_id, fecha)
                 VALUES (?1, 'DEVOLUCION', ?2, ?3, ?4, ?5, ?6, ?7)",
                libsql::params![id, devolver, metodo.to_string(), caja_id, motivo.clone(), claims.sub, tiempo::ahora_lima()],
            )
            .await
            .map_err(e500)?;
        }
        Ok(())
    });
    Ok(Json(leer(&conn, id).await?))
}

// ---------------------------------------------------------------------
// Foto de referencia
// ---------------------------------------------------------------------

/// POST /encargos/:id/foto — una foto del diseño (la anterior se reemplaza).
/// El nombre del archivo lleva una parte al azar: la carpeta de imágenes se
/// sirve sin sesión y la foto de un cliente no debe poder adivinarse.
pub async fn subir_foto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado, _, _) = estado_y_numero(&conn, id).await?;
    activo(&estado)?;
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(campo) = multipart.next_field().await.map_err(|e| malo(e.to_string()))? {
        if campo.name() == Some("imagen") {
            bytes = Some(campo.bytes().await.map_err(|e| malo(e.to_string()))?.to_vec());
        }
    }
    let bytes = bytes.ok_or_else(|| malo("No se recibió ninguna imagen."))?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(malo("La foto no puede pesar más de 10 MB."));
    }
    // Revisa medidas y formato antes de abrirla, con tope de memoria, en un
    // hilo aparte y de a una (ver logica/imagen.rs).
    let jpeg = crate::logica::imagen::procesar(bytes, 1200, crate::logica::imagen::Salida::Jpeg(82))
        .await
        .map_err(|e| (e.status(), e.mensaje()))?;
    let carpeta = format!("uploads/encargos/{}", claims.tienda_id);
    tokio::fs::create_dir_all(&carpeta).await.map_err(|e| e500(format!("No se pudo crear la carpeta de fotos: {}", e)))?;
    let azar: String = {
        use rand::Rng;
        let mut r = rand::thread_rng();
        (0..20).map(|_| format!("{:x}", r.gen::<u8>() & 0x0f)).collect()
    };
    let archivo = format!("{}-{}.jpg", id, azar);
    let ruta = format!("{}/{}", carpeta, archivo);
    tokio::fs::write(&ruta, jpeg).await.map_err(|e| e500(format!("No se pudo guardar la foto: {}", e)))?;
    let url = format!("/uploads/encargos/{}/{}", claims.tienda_id, archivo);
    let anterior = foto_actual(&conn, id).await;
    conn.execute(
        "UPDATE encargos SET foto_url = ?1, fecha_actualizacion = ?2 WHERE id = ?3",
        libsql::params![url, tiempo::ahora_lima(), id],
    )
    .await
    .map_err(e500)?;
    borrar_archivo(anterior.as_deref(), claims.tienda_id);
    Ok(Json(leer(&conn, id).await?))
}

/// DELETE /encargos/:id/foto
pub async fn quitar_foto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<Encargo>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let anterior = foto_actual(&conn, id).await;
    conn.execute("UPDATE encargos SET foto_url = NULL WHERE id = ?1", libsql::params![id]).await.map_err(sin_migracion)?;
    borrar_archivo(anterior.as_deref(), claims.tienda_id);
    Ok(Json(leer(&conn, id).await?))
}

async fn foto_actual(conn: &libsql::Connection, id: i64) -> Option<String> {
    let mut f = conn.query("SELECT foto_url FROM encargos WHERE id = ?1", libsql::params![id]).await.ok()?;
    f.next().await.ok()??.get::<String>(0).ok()
}

/// Borra la foto anterior, solo si es de la carpeta de este negocio.
fn borrar_archivo(url: Option<&str>, tienda_id: i64) {
    let Some(url) = url else { return };
    let prefijo = format!("/uploads/encargos/{}/", tienda_id);
    if let Some(nombre) = url.strip_prefix(&prefijo) {
        if !nombre.is_empty() && !nombre.contains('/') && !nombre.contains("..") {
            let _ = std::fs::remove_file(format!("uploads/encargos/{}/{}", tienda_id, nombre));
        }
    }
}

// ---------------------------------------------------------------------
// Entrega: se cobra en el punto de venta (handlers/ventas.rs)
// ---------------------------------------------------------------------

/// Lo que la venta necesita del pedido que se entrega.
pub struct CobroEncargo {
    pub id: i64,
    /// Lo que ya pagó (adelantos − devoluciones): no se vuelve a cobrar.
    pub adelanto: f64,
    /// producto -> precios acordados (se aceptan en la venta).
    pub precios: HashMap<i64, Vec<f64>>,
}

/// Antes de la venta: el pedido debe estar activo y el módulo encendido.
/// Una venta normal (sin pedido) no hace nada aquí.
pub async fn preparar_cobro(
    conn: &libsql::Connection,
    modulos: Option<&str>,
    encargo_id: Option<i64>,
) -> Result<Option<CobroEncargo>, Fallo> {
    let Some(id) = encargo_id else { return Ok(None) };
    if !modulos.is_some_and(|m| m.split(',').any(|x| x.trim() == MODULO_ENCARGOS)) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ));
    }
    let mut filas = conn
        .query("SELECT estado, CAST(adelantado - devuelto AS REAL) FROM encargos WHERE id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let (estado, adelanto): (String, f64) = match filas.next().await.map_err(e500)? {
        Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or(0.0)),
        None => return Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
    };
    drop(filas);
    activo(&estado)?;
    let mut precios: HashMap<i64, Vec<f64>> = HashMap::new();
    let mut filas = conn
        .query("SELECT producto_id, CAST(precio_unitario AS REAL) FROM encargo_items WHERE encargo_id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        precios.entry(f.get(0).unwrap_or_default()).or_default().push(f.get(1).unwrap_or(-1.0));
    }
    Ok(Some(CobroEncargo { id, adelanto: r2(adelanto.max(0.0)), precios }))
}

/// Dentro de la transacción de la venta: el pedido queda ENTREGADO con su
/// venta. Si otra caja lo cobró un instante antes (o le agregaron un
/// adelanto mientras tanto), la venta no entra.
pub async fn marcar_entregado(conn: &libsql::Connection, cobro: &CobroEncargo, venta_id: i64) -> Result<(), Fallo> {
    let n = conn
        .execute(
            "UPDATE encargos SET estado = 'ENTREGADO', venta_id = ?1, fecha_entregado = ?2, fecha_actualizacion = ?2
             WHERE id = ?3 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')
               AND abs((adelantado - devuelto) - ?4) < 0.005",
            libsql::params![venta_id, tiempo::ahora_lima(), cobro.id, cobro.adelanto],
        )
        .await
        .map_err(|e| e500(format!("No se pudo cerrar el pedido: {}", e)))?;
    if n == 0 {
        return Err((
            StatusCode::CONFLICT,
            "El pedido cambió mientras cobrabas (ya se entregó, se canceló o recibió otro adelanto). Vuelve a abrirlo desde Pedidos.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn horas() {
        assert!(hora_valida("09:30"));
        assert!(hora_valida("23:59"));
        assert!(!hora_valida("24:00"));
        assert!(!hora_valida("9:30"));
        assert!(!hora_valida("09:3"));
        assert!(!hora_valida("ab:cd"));
    }

    #[test]
    fn metodos() {
        assert_eq!(metodo_valido(None).unwrap(), "EFECTIVO");
        assert_eq!(metodo_valido(Some("yape")).unwrap(), "YAPE");
        assert_eq!(metodo_valido(Some("YAPE_PLIN")).unwrap(), "YAPE");
        assert!(metodo_valido(Some("CREDITO")).is_err());
        assert!(metodo_valido(Some("ADELANTO")).is_err());
    }
}
