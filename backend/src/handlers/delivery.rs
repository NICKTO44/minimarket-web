//! Módulo DELIVERY (migración 0045): pedidos por WhatsApp / delivery, pensado
//! para una dark kitchen (cocina sin local: todo es delivery o recojo).
//!
//! Va encima de los pedidos del restaurante (handlers/mesas.rs): un pedido de
//! delivery es un `pedidos` tipo 'DELIVERY' con sus platos en `pedido_items`
//! (misma validación de precios y opciones: mesas::insertar_items), la cocina
//! de siempre (Preparación) y el cobro en el punto de venta (POST /ventas con
//! pedido_id). Aquí van los datos de la entrega y el control del pago.
//!
//! Flujo (todo se registra en un solo formulario: cliente, platos y envío):
//!   - ADELANTADO: el pedido queda ESPERANDO_PAGO y no va a cocina. Al
//!     confirmar el pago se cobra en el punto de venta (sale la nota de venta
//!     o boleta, que viaja con el pedido) y recién ahí los platos pasan a
//!     cocina (ventas.rs, paso 7). No se despacha sin cobrar.
//!   - CONTRA_ENTREGA: va a cocina al registrarse. Sale sin cobrar (al
//!     despachar se puede anotar con cuánto paga, para el vuelto); al
//!     entregarse queda POR_RENDIR hasta que se cobra en el punto de venta.
//!   - RECOJO: sin envío ni repartidor; va a cocina y se cobra al recoger.
//!   Fases: ESPERANDO_PAGO -> EN_COCINA -> LISTO -> EN_CAMINO -> POR_RENDIR -> ENTREGADO.
//!
//! El costo de envío es una línea más del pedido, sobre un servicio interno
//! "Delivery" (código _DLV_ENVIO, sin stock): entra al cobro, a la boleta y a
//! la caja. Nace entregada, así que no sale en la comanda ni en Preparación.
//! Un pedido no se guarda ni se cobra sin platos.
//!
//! Solo con el modo restaurante y el módulo encendidos; si no, 403.
//!
//! Rutas:
//!   GET  /delivery                 -> tablero (en curso, por rendir y terminado hoy) + repartidores
//!   GET  /delivery/resumen?fecha=  -> lo cobrado ese día, con totales por medio, repartidor y envíos
//!   POST /delivery                 -> pedido nuevo con sus platos
//!   PUT  /delivery/:id             -> datos, pago, envío y platos (agregar / quitar)
//!   POST /delivery/:id/listo       -> cocina terminó
//!   POST /delivery/:id/despachar   -> sale con el repartidor
//!   POST /delivery/:id/volver      -> deshace el despacho
//!   POST /delivery/:id/entregar    -> entregado
//!   POST /delivery/:id/asignar     -> asigna a un repartidor (cuenta propia, con su orden del día)
//!   POST /delivery/rendir          -> el cajero recibió el efectivo que cobró un repartidor
//!   GET  /reparto, POST /reparto/:id/salir, POST /reparto/:id/entregar -> el repartidor con su cuenta
//!
//! Repartidor (rol REPARTIDOR, migración 0046): ve solo sus entregas. En
//! contra entrega cobra delante del cliente desde su cuenta (POST /ventas con
//! el pedido: sale el comprobante y se envía por WhatsApp) y con eso queda
//! entregado; ese efectivo queda "por rendir" hasta que el cajero lo recibe.
//! Un pedido ya pagado solo lo marca entregado.

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::mesas::{insertar_items, modo_restaurante};
use crate::handlers::rubros::{negocio, MODULO_DELIVERY};
use crate::logica::tiempo;
use crate::models::auth::Claims;
use crate::models::mesa::NuevoItemPedido;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const CANALES: &[&str] = &["WHATSAPP", "LLAMADA", "REDES", "OTRO"];
const METODOS: &[&str] = &["EFECTIVO", "YAPE", "PLIN", "TRANSFERENCIA", "TARJETA"];
/// Servicio interno con el que el costo de envío entra al pedido y a la venta.
pub const CODIGO_ENVIO: &str = "_DLV_ENVIO";
const NOMBRE_ENVIO: &str = "Delivery";
const CATEGORIA_ENVIO: &str = "Delivery";
const MAX_PLATOS: usize = 60;

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn conflicto(texto: impl Into<String>) -> Fallo {
    (StatusCode::CONFLICT, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let t = e.to_string();
    if t.contains("no such table") || t.contains("no such column") {
        (StatusCode::CONFLICT, "El sistema se está actualizando. Intenta en un minuto.".into())
    } else {
        e500(t)
    }
}
fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, Fallo> {
    let limpio = valor.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(malo(format!("{}: máximo {} letras.", campo, max))),
        otro => Ok(otro),
    }
}

/// Modo restaurante + módulo DELIVERY encendidos.
async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    if modo_restaurante(conn).await && negocio(conn).await.modulos.iter().any(|m| m == MODULO_DELIVERY) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Este negocio no tiene encendido el módulo \"Pedidos delivery\". Actívalo en Configuración → Datos del negocio.".into(),
        ))
    }
}

/// Transacción inmediata: el cuerpo devuelve Result; si falla, nada queda a medias.
macro_rules! en_transaccion {
    ($conn:expr, |$tx:ident| $cuerpo:block) => {{
        let $tx = $conn.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await.map_err(e500)?;
        let resultado: Result<_, Fallo> = async { $cuerpo }.await;
        match resultado {
            Ok(v) => {
                $tx.commit().await.map_err(e500)?;
                v
            }
            Err(e) => {
                let _ = $tx.rollback().await;
                return Err(e);
            }
        }
    }};
}

// ---------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LineaDelivery {
    pub id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub opciones: Option<String>,
    pub nota: Option<String>,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub subtotal: f64,
    /// PENDIENTE (aún no va a cocina) o ENVIADO.
    pub estado: String,
}

/// Línea de la comanda para imprimir (mismos campos que TicketMesa).
#[derive(Debug, Serialize)]
pub struct ItemComanda {
    pub id: i64,
    pub nombre_producto: String,
    pub opciones: Option<String>,
    pub nota: Option<String>,
    pub cantidad: f64,
}

#[derive(Debug, Serialize)]
pub struct PedidoDelivery {
    pub id: i64,
    pub cliente_id: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub telefono: Option<String>,
    pub entrega: String,
    pub direccion: Option<String>,
    pub referencia: Option<String>,
    pub canal: String,
    pub pago: String,
    pub metodo_pago: Option<String>,
    pub paga_con: Option<f64>,
    /// paga_con − total (lo que lleva el repartidor de vuelto).
    pub vuelto: Option<f64>,
    pub costo_envio: f64,
    pub repartidor: Option<String>,
    /// Usuario repartidor asignado (cuenta propia) y su orden de entrega del día.
    pub repartidor_id: Option<i64>,
    pub orden_reparto: Option<i64>,
    /// El cajero ya recibió el efectivo que cobró el repartidor.
    pub rendido: bool,
    /// Lo guardado: EN_COCINA, EN_CAMINO o ENTREGADO.
    pub estado: String,
    /// Para la pantalla: ESPERANDO_PAGO, EN_COCINA, LISTO, EN_CAMINO, POR_RENDIR o ENTREGADO.
    pub fase: String,
    /// ¿Ya se cobró en el punto de venta?
    pub cobrado: bool,
    pub venta_id: Option<i64>,
    pub folio: Option<String>,
    pub notas: Option<String>,
    pub total: f64,
    /// Cocina (sin contar el envío): sin enviar, preparando y listos.
    pub pendientes: i64,
    pub preparando: i64,
    pub listos: i64,
    pub minutos: i64,
    pub fecha_apertura: String,
    pub fecha_salida: Option<String>,
    pub fecha_entrega: Option<String>,
    /// Pedido de delivery abierto desde Mesas antes de encender el módulo.
    pub sin_datos: bool,
    pub lineas: Vec<LineaDelivery>,
    /// Lo que acaba de pasar a cocina con esta acción (para imprimir la comanda).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comanda: Vec<ItemComanda>,
}

#[derive(Debug, Serialize)]
pub struct Repartidor {
    pub id: i64,
    pub nombre: String,
}

#[derive(Debug, Serialize)]
pub struct CobroPorRendir {
    pub pedido_id: i64,
    pub cliente_nombre: Option<String>,
    pub efectivo: f64,
    pub total: f64,
}

/// Lo que un repartidor cobró en la puerta y aún no entrega al cajero.
#[derive(Debug, Serialize)]
pub struct Rendicion {
    pub repartidor_id: i64,
    pub nombre: String,
    /// Efectivo que debe entregar.
    pub efectivo: f64,
    /// Lo que cobró por Yape, Plin u otro medio (ya está en la cuenta del negocio).
    pub digital: f64,
    pub pedidos: Vec<CobroPorRendir>,
}

#[derive(Debug, Serialize)]
pub struct Tablero {
    pub pedidos: Vec<PedidoDelivery>,
    /// Nombres usados en los últimos 30 días (para despachar sin cuenta de repartidor).
    pub nombres_recientes: Vec<String>,
    /// Usuarios con rol Repartidor (para asignar).
    pub repartidores: Vec<Repartidor>,
    pub rendicion: Vec<Rendicion>,
}

// Los conteos de cocina no cuentan el envío (nace entregado).
const SELECT_PEDIDO: &str = "
    SELECT p.id, d.cliente_id, p.cliente_nombre, c.numero_documento, d.telefono, COALESCE(d.entrega, 'DELIVERY'),
           d.direccion, d.referencia, COALESCE(d.canal, 'WHATSAPP'), COALESCE(d.pago, 'CONTRA_ENTREGA'), d.metodo_pago,
           CAST(d.paga_con AS REAL), CAST(COALESCE(d.costo_envio, 0) AS REAL), d.repartidor, COALESCE(d.estado, 'EN_COCINA'),
           p.estado, p.venta_id, v.folio, p.notas,
           CAST(COALESCE((SELECT SUM(i.cantidad * i.precio_unitario) FROM pedido_items i
                          WHERE i.pedido_id = p.id AND i.estado != 'ANULADO'), 0) AS REAL),
           (SELECT COUNT(*) FROM pedido_items i WHERE i.pedido_id = p.id AND i.estado = 'PENDIENTE'),
           (SELECT COUNT(*) FROM pedido_items i WHERE i.pedido_id = p.id AND i.estado = 'ENVIADO'
                AND i.fecha_listo IS NULL AND i.fecha_entregado IS NULL),
           (SELECT COUNT(*) FROM pedido_items i WHERE i.pedido_id = p.id AND i.estado = 'ENVIADO'
                AND i.fecha_listo IS NOT NULL AND i.fecha_entregado IS NULL),
           CAST((julianday(COALESCE(d.fecha_entrega, datetime('now', 'localtime'))) - julianday(p.fecha_apertura)) * 1440 AS INTEGER),
           p.fecha_apertura, d.fecha_salida, d.fecha_entrega, d.pedido_id IS NULL,
           d.repartidor_id, d.orden_reparto, d.rendido_en IS NOT NULL
    FROM pedidos p
    LEFT JOIN pedido_delivery d ON d.pedido_id = p.id
    LEFT JOIN clientes c ON c.id = d.cliente_id
    LEFT JOIN ventas v ON v.id = p.venta_id";

fn pedido_de(f: &libsql::Row) -> PedidoDelivery {
    let total = r2(f.get(19).unwrap_or(0.0));
    let paga_con: Option<f64> = f.get(11).ok();
    let cobrado = f.get::<String>(15).map(|e| e == "COBRADO").unwrap_or(false);
    PedidoDelivery {
        id: f.get(0).unwrap_or_default(),
        cliente_id: f.get(1).ok(),
        cliente_nombre: f.get(2).ok(),
        cliente_documento: f.get(3).ok(),
        telefono: f.get(4).ok(),
        entrega: f.get(5).unwrap_or_else(|_| "DELIVERY".into()),
        direccion: f.get(6).ok(),
        referencia: f.get(7).ok(),
        canal: f.get(8).unwrap_or_else(|_| "WHATSAPP".into()),
        pago: f.get(9).unwrap_or_else(|_| "CONTRA_ENTREGA".into()),
        metodo_pago: f.get(10).ok(),
        paga_con,
        vuelto: paga_con.filter(|p| !cobrado && *p > 0.0).map(|p| r2((p - total).max(0.0))),
        costo_envio: r2(f.get(12).unwrap_or(0.0)),
        repartidor: f.get(13).ok(),
        estado: f.get(14).unwrap_or_else(|_| "EN_COCINA".into()),
        fase: String::new(),
        cobrado,
        venta_id: f.get(16).ok(),
        folio: f.get(17).ok(),
        notas: f.get(18).ok(),
        total,
        pendientes: f.get(20).unwrap_or(0),
        preparando: f.get(21).unwrap_or(0),
        listos: f.get(22).unwrap_or(0),
        minutos: f.get::<i64>(23).unwrap_or(0).max(0),
        fecha_apertura: f.get(24).unwrap_or_default(),
        fecha_salida: f.get(25).ok(),
        fecha_entrega: f.get(26).ok(),
        sin_datos: f.get::<i64>(27).unwrap_or(0) == 1,
        repartidor_id: f.get(28).ok(),
        orden_reparto: f.get(29).ok(),
        rendido: f.get::<i64>(30).unwrap_or(0) == 1,
        lineas: Vec::new(),
        comanda: Vec::new(),
    }
}

fn fase_de(p: &PedidoDelivery) -> &'static str {
    match p.estado.as_str() {
        "EN_CAMINO" => "EN_CAMINO",
        "ENTREGADO" if !p.cobrado => "POR_RENDIR",
        "ENTREGADO" => "ENTREGADO",
        _ if p.pago == "ADELANTADO" && !p.cobrado => "ESPERANDO_PAGO",
        _ if p.pendientes == 0 && p.preparando == 0 && !p.lineas.is_empty() => "LISTO",
        _ => "EN_COCINA",
    }
}

/// Las líneas (sin el envío) de varios pedidos, en una consulta; y su fase.
async fn completar(conn: &libsql::Connection, pedidos: &mut [PedidoDelivery]) -> Result<(), Fallo> {
    if !pedidos.is_empty() {
        let ids = pedidos.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(",");
        let mut filas = conn
            .query(
                &format!(
                    "SELECT i.pedido_id, i.id, i.producto_id, i.nombre_producto, i.opciones, i.nota, CAST(i.cantidad AS REAL),
                            CAST(i.precio_unitario AS REAL), i.estado
                     FROM pedido_items i LEFT JOIN productos pr ON pr.id = i.producto_id
                     WHERE i.pedido_id IN ({}) AND i.estado != 'ANULADO' AND COALESCE(pr.codigo, '') != ?1
                     ORDER BY i.id",
                    ids
                ),
                libsql::params![CODIGO_ENVIO],
            )
            .await
            .map_err(e500)?;
        let mut mapa: HashMap<i64, Vec<LineaDelivery>> = HashMap::new();
        while let Some(f) = filas.next().await.map_err(e500)? {
            let cantidad: f64 = f.get(6).unwrap_or(0.0);
            let precio: f64 = f.get(7).unwrap_or(0.0);
            mapa.entry(f.get(0).unwrap_or_default()).or_default().push(LineaDelivery {
                id: f.get(1).unwrap_or_default(),
                producto_id: f.get(2).unwrap_or_default(),
                nombre: f.get(3).unwrap_or_default(),
                opciones: f.get(4).ok(),
                nota: f.get(5).ok(),
                cantidad,
                precio_unitario: precio,
                subtotal: r2(cantidad * precio),
                estado: f.get(8).unwrap_or_default(),
            });
        }
        for p in pedidos.iter_mut() {
            p.lineas = mapa.remove(&p.id).unwrap_or_default();
        }
    }
    for p in pedidos.iter_mut() {
        p.fase = fase_de(p).to_string();
    }
    Ok(())
}

async fn leer(conn: &libsql::Connection, id: i64) -> Result<PedidoDelivery, Fallo> {
    let mut filas = conn
        .query(&format!("{} WHERE p.id = ?1 AND p.tipo = 'DELIVERY'", SELECT_PEDIDO), libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    let mut p = match filas.next().await.map_err(e500)? {
        Some(f) => pedido_de(&f),
        None => return Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
    };
    drop(filas);
    completar(conn, std::slice::from_mut(&mut p)).await?;
    Ok(p)
}

/// GET /delivery — lo que está en curso, lo que falta cobrar y lo terminado hoy.
pub async fn tablero(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Tablero>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            &format!(
                "{} WHERE p.tipo = 'DELIVERY' AND p.estado != 'ANULADO'
                   AND (p.estado = 'ABIERTO' OR COALESCE(d.estado, 'EN_COCINA') != 'ENTREGADO'
                        OR date(COALESCE(d.fecha_entrega, p.fecha_cierre, p.fecha_apertura)) = date('now', 'localtime'))
                 ORDER BY p.fecha_apertura, p.id",
                SELECT_PEDIDO
            ),
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut pedidos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        pedidos.push(pedido_de(&f));
    }
    drop(filas);
    completar(&conn, &mut pedidos).await?;
    Ok(Json(Tablero {
        pedidos,
        nombres_recientes: nombres_recientes(&conn).await?,
        repartidores: usuarios_repartidores(&conn).await?,
        rendicion: rendicion(&conn).await?,
    }))
}

/// Usuarios activos con rol Repartidor.
async fn usuarios_repartidores(conn: &libsql::Connection) -> Result<Vec<Repartidor>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT u.id, u.nombre_completo FROM usuarios u JOIN roles r ON r.id = u.rol_id
             WHERE r.nombre = 'REPARTIDOR' AND COALESCE(u.activo, 1) = 1 ORDER BY u.nombre_completo",
            (),
        )
        .await
        .map_err(e500)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(Repartidor { id: f.get(0).unwrap_or_default(), nombre: f.get(1).unwrap_or_default() });
    }
    Ok(lista)
}

/// Lo que cada repartidor cobró en la puerta (ventas hechas con su cuenta) y
/// aún no rinde al cajero. Solo el efectivo se entrega; lo digital se muestra.
async fn rendicion(conn: &libsql::Connection) -> Result<Vec<Rendicion>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT d.repartidor_id, COALESCE(u.nombre_completo, d.repartidor, 'Repartidor'), p.id, p.cliente_nombre,
                    v.metodo_pago, CAST(v.total AS REAL), CAST(v.pago_efectivo AS REAL), v.pago_efectivo_metodo,
                    CAST(v.pago_otro AS REAL), v.pago_otro_metodo
             FROM pedido_delivery d
             JOIN pedidos p ON p.id = d.pedido_id
             JOIN ventas v ON v.id = p.venta_id
             LEFT JOIN usuarios u ON u.id = d.repartidor_id
             WHERE d.repartidor_id IS NOT NULL AND v.usuario_id = d.repartidor_id AND d.rendido_en IS NULL
               AND p.estado = 'COBRADO' AND COALESCE(v.estado, 'COMPLETADA') != 'CANCELADA'
             ORDER BY v.fecha_hora",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista: Vec<Rendicion> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let total = r2(f.get(5).unwrap_or(0.0));
        let metodo: String = f.get(4).unwrap_or_default();
        let efectivo = match metodo.as_str() {
            "EFECTIVO" => total,
            "MIXTO" => {
                let primero = if f.get::<String>(7).map(|m| m == "EFECTIVO").unwrap_or(true) { f.get(6).unwrap_or(0.0) } else { 0.0 };
                let segundo = if f.get::<String>(9).map(|m| m == "EFECTIVO").unwrap_or(false) { f.get(8).unwrap_or(0.0) } else { 0.0 };
                r2(primero + segundo)
            }
            _ => 0.0,
        };
        let id: i64 = f.get(0).unwrap_or_default();
        let nombre: String = f.get(1).unwrap_or_default();
        let fila = CobroPorRendir { pedido_id: f.get(2).unwrap_or_default(), cliente_nombre: f.get(3).ok(), efectivo, total };
        match lista.iter_mut().find(|r| r.repartidor_id == id) {
            Some(r) => {
                r.efectivo = r2(r.efectivo + efectivo);
                r.digital = r2(r.digital + total - efectivo);
                r.pedidos.push(fila);
            }
            None => lista.push(Rendicion { repartidor_id: id, nombre, efectivo, digital: r2(total - efectivo), pedidos: vec![fila] }),
        }
    }
    Ok(lista)
}

async fn nombres_recientes(conn: &libsql::Connection) -> Result<Vec<String>, Fallo> {
    let mut lista = Vec::new();
    let mut filas = conn
        .query(
            "SELECT repartidor FROM pedido_delivery
             WHERE repartidor IS NOT NULL AND fecha_salida >= datetime('now', 'localtime', '-30 days')
             GROUP BY repartidor ORDER BY MAX(fecha_salida) DESC LIMIT 12",
            (),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        if let Ok(r) = f.get::<String>(0) {
            lista.push(r);
        }
    }
    Ok(lista)
}

// ---------------------------------------------------------------------
// Resumen del día
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FiltroResumen {
    #[serde(default)]
    pub fecha: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParteDePago {
    /// EFECTIVO, YAPE, PLIN, YAPE_PLIN (sin separar), TRANSFERENCIA o TARJETA.
    pub metodo: String,
    pub monto: f64,
}

#[derive(Debug, Serialize)]
pub struct FilaResumen {
    pub id: i64,
    pub hora: String,
    pub cliente_nombre: Option<String>,
    pub entrega: String,
    pub pago: String,
    pub repartidor: Option<String>,
    pub folio: Option<String>,
    pub total: f64,
    pub costo_envio: f64,
    pub partes: Vec<ParteDePago>,
}

#[derive(Debug, Serialize)]
pub struct Total {
    pub nombre: String,
    pub pedidos: i64,
    pub monto: f64,
}

#[derive(Debug, Serialize)]
pub struct Resumen {
    pub fecha: String,
    pub pedidos: i64,
    pub total: f64,
    pub envios: f64,
    pub por_metodo: Vec<Total>,
    pub por_repartidor: Vec<Total>,
    pub filas: Vec<FilaResumen>,
}

/// GET /delivery/resumen?fecha=YYYY-MM-DD — los pedidos delivery cobrados ese
/// día (hoy por defecto), con lo que entró por cada medio de pago, lo que
/// llevó cada repartidor y lo cobrado en envíos.
pub async fn resumen(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<FiltroResumen>) -> Result<Json<Resumen>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let fecha = q.fecha.map(|f| f.trim().to_string()).filter(|f| tiempo::fecha_valida(f)).unwrap_or_else(tiempo::hoy_lima);
    let mut filas = conn
        .query(
            "SELECT p.id, v.fecha_hora, p.cliente_nombre, COALESCE(d.entrega, 'DELIVERY'), COALESCE(d.pago, 'CONTRA_ENTREGA'),
                    d.repartidor, v.folio, CAST(v.total AS REAL), CAST(COALESCE(d.costo_envio, 0) AS REAL),
                    v.metodo_pago, v.billetera, CAST(v.pago_efectivo AS REAL), v.pago_efectivo_metodo,
                    CAST(v.pago_otro AS REAL), v.pago_otro_metodo
             FROM pedidos p
             JOIN ventas v ON v.id = p.venta_id
             LEFT JOIN pedido_delivery d ON d.pedido_id = p.id
             WHERE p.tipo = 'DELIVERY' AND p.estado = 'COBRADO' AND COALESCE(v.estado, 'COMPLETADA') != 'CANCELADA'
               AND date(v.fecha_hora) = ?1
             ORDER BY v.fecha_hora, p.id",
            libsql::params![fecha.clone()],
        )
        .await
        .map_err(sin_migracion)?;
    let billetera = |metodo: Option<String>, billetera: &Option<String>| -> String {
        match metodo.as_deref() {
            Some("YAPE_PLIN") => billetera.clone().filter(|b| b == "YAPE" || b == "PLIN").unwrap_or_else(|| "YAPE_PLIN".into()),
            Some(m) if !m.is_empty() => m.to_string(),
            _ => "EFECTIVO".into(),
        }
    };
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let total = r2(f.get(7).unwrap_or(0.0));
        let metodo: String = f.get(9).unwrap_or_else(|_| "EFECTIVO".into());
        let bill: Option<String> = f.get(10).ok();
        let partes = if metodo == "MIXTO" {
            let primero: f64 = f.get(11).unwrap_or(0.0);
            let segundo: f64 = f.get(13).unwrap_or(0.0);
            [
                (billetera(f.get(12).ok(), &None), primero),
                (billetera(f.get(14).ok(), &bill), segundo),
            ]
            .into_iter()
            .filter(|(_, m)| *m > 0.0)
            .map(|(metodo, monto)| ParteDePago { metodo, monto: r2(monto) })
            .collect()
        } else {
            vec![ParteDePago { metodo: billetera(Some(metodo), &bill), monto: total }]
        };
        let hora: String = f.get(1).unwrap_or_default();
        lista.push(FilaResumen {
            id: f.get(0).unwrap_or_default(),
            hora: hora.get(11..16).unwrap_or("").to_string(),
            cliente_nombre: f.get(2).ok(),
            entrega: f.get(3).unwrap_or_else(|_| "DELIVERY".into()),
            pago: f.get(4).unwrap_or_else(|_| "CONTRA_ENTREGA".into()),
            repartidor: f.get(5).ok(),
            folio: f.get(6).ok(),
            total,
            costo_envio: r2(f.get(8).unwrap_or(0.0)),
            partes,
        });
    }
    let mut por_metodo: Vec<Total> = Vec::new();
    let mut por_repartidor: Vec<Total> = Vec::new();
    let sumar = |lista: &mut Vec<Total>, nombre: &str, monto: f64| match lista.iter_mut().find(|t| t.nombre == nombre) {
        Some(t) => {
            t.pedidos += 1;
            t.monto = r2(t.monto + monto);
        }
        None => lista.push(Total { nombre: nombre.to_string(), pedidos: 1, monto: r2(monto) }),
    };
    for fila in &lista {
        for parte in &fila.partes {
            sumar(&mut por_metodo, &parte.metodo, parte.monto);
        }
        let quien = match (&fila.repartidor, fila.entrega.as_str()) {
            (_, "RECOJO") => "Recojo en el local".to_string(),
            (Some(r), _) => r.clone(),
            (None, _) => "Sin repartidor".to_string(),
        };
        sumar(&mut por_repartidor, &quien, fila.total);
    }
    por_metodo.sort_by(|a, b| b.monto.total_cmp(&a.monto));
    por_repartidor.sort_by(|a, b| b.monto.total_cmp(&a.monto));
    Ok(Json(Resumen {
        fecha,
        pedidos: lista.len() as i64,
        total: r2(lista.iter().map(|f| f.total).sum()),
        envios: r2(lista.iter().map(|f| f.costo_envio).sum()),
        por_metodo,
        por_repartidor,
        filas: lista,
    }))
}

// ---------------------------------------------------------------------
// Crear y editar
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DatosDelivery {
    #[serde(default)]
    pub cliente_id: Option<i64>,
    #[serde(default)]
    pub cliente_nombre: Option<String>,
    #[serde(default)]
    pub telefono: Option<String>,
    #[serde(default)]
    pub entrega: Option<String>,
    #[serde(default)]
    pub direccion: Option<String>,
    #[serde(default)]
    pub referencia: Option<String>,
    #[serde(default)]
    pub canal: Option<String>,
    pub pago: String,
    #[serde(default)]
    pub metodo_pago: Option<String>,
    #[serde(default)]
    pub costo_envio: Option<f64>,
    #[serde(default)]
    pub notas: Option<String>,
    /// Platos nuevos (al crear: todos los del pedido).
    #[serde(default)]
    pub agregar: Vec<NuevoItemPedido>,
    /// Al editar: líneas que se quitan. Una que ya está en cocina se anula con motivo.
    #[serde(default)]
    pub quitar: Vec<i64>,
    #[serde(default)]
    pub motivo_quitar: Option<String>,
}

struct Listo {
    cliente_id: Option<i64>,
    cliente_nombre: String,
    telefono: Option<String>,
    entrega: String,
    direccion: Option<String>,
    referencia: Option<String>,
    canal: String,
    pago: String,
    metodo_pago: Option<String>,
    costo_envio: f64,
    notas: Option<String>,
}

async fn revisar(conn: &libsql::Connection, d: &DatosDelivery) -> Result<Listo, Fallo> {
    let entrega = d.entrega.as_deref().unwrap_or("DELIVERY").trim().to_uppercase();
    if entrega != "DELIVERY" && entrega != "RECOJO" {
        return Err(malo("La entrega debe ser DELIVERY o RECOJO."));
    }
    let canal = d.canal.as_deref().unwrap_or("WHATSAPP").trim().to_uppercase();
    if !CANALES.contains(&canal.as_str()) {
        return Err(malo("Canal no válido."));
    }
    let pago = d.pago.trim().to_uppercase();
    if pago != "ADELANTADO" && pago != "CONTRA_ENTREGA" {
        return Err(malo("Elige si paga por adelantado o contra entrega."));
    }
    let metodo_pago = d.metodo_pago.as_deref().map(|m| m.trim().to_uppercase()).filter(|m| !m.is_empty());
    if metodo_pago.as_deref().is_some_and(|m| !METODOS.contains(&m)) {
        return Err(malo("Medio de pago no válido."));
    }
    let costo_envio = d.costo_envio.unwrap_or(0.0);
    if !(costo_envio >= 0.0) || !costo_envio.is_finite() || costo_envio > 10_000.0 {
        return Err(malo("El costo de envío no es válido."));
    }
    let direccion = texto(d.direccion.as_deref(), 250, "Dirección")?;
    if entrega == "DELIVERY" && direccion.is_none() {
        return Err(malo("Escribe la dirección de entrega."));
    }
    let referencia = texto(d.referencia.as_deref(), 250, "Referencia")?;
    let notas = texto(d.notas.as_deref(), 500, "Notas")?;
    let mut telefono = texto(d.telefono.as_deref(), 20, "Celular")?;
    let mut nombre = texto(d.cliente_nombre.as_deref(), 60, "Nombre del cliente")?;
    if let Some(id) = d.cliente_id {
        let mut f = conn
            .query("SELECT nombre_razon_social, telefono FROM clientes WHERE id = ?1", libsql::params![id])
            .await
            .map_err(e500)?;
        match f.next().await.map_err(e500)? {
            Some(c) => {
                nombre = nombre.or_else(|| c.get::<String>(0).ok().map(|n| n.chars().take(60).collect()));
                telefono = telefono.or_else(|| c.get::<String>(1).ok().filter(|t| !t.trim().is_empty()));
            }
            None => return Err(malo("El cliente elegido no existe.")),
        }
    }
    let cliente_nombre = nombre.ok_or_else(|| malo("Escribe el nombre del cliente."))?;
    if entrega == "DELIVERY" && telefono.is_none() {
        return Err(malo("Escribe el celular del cliente (para coordinar la entrega)."));
    }
    if d.agregar.len() > MAX_PLATOS {
        return Err(malo(format!("Un pedido puede llevar hasta {} platos a la vez.", MAX_PLATOS)));
    }
    Ok(Listo {
        cliente_id: d.cliente_id,
        cliente_nombre,
        telefono,
        entrega: entrega.clone(),
        direccion: if entrega == "DELIVERY" { direccion } else { None },
        referencia: if entrega == "DELIVERY" { referencia } else { None },
        canal,
        pago,
        metodo_pago,
        costo_envio: if entrega == "DELIVERY" { r2(costo_envio) } else { 0.0 },
        notas,
    })
}

/// Id del servicio interno del envío (lo crea la primera vez; sin stock).
async fn producto_envio(conn: &libsql::Connection) -> Result<i64, Fallo> {
    conn.execute(
        "INSERT OR IGNORE INTO categorias (nombre, descripcion) VALUES (?1, 'Creada por Pedidos delivery')",
        libsql::params![CATEGORIA_ENVIO],
    )
    .await
    .map_err(e500)?;
    conn.execute(
        "INSERT OR IGNORE INTO productos (codigo, nombre, descripcion, precio, stock, stock_minimo, unidad_medida, categoria_id)
         VALUES (?1, ?2, 'Uso interno de Pedidos delivery: el costo de envío de cada pedido.', 1, 0, 0, 'UNIDAD',
                 (SELECT id FROM categorias WHERE nombre = ?3))",
        libsql::params![CODIGO_ENVIO, NOMBRE_ENVIO, CATEGORIA_ENVIO],
    )
    .await
    .map_err(e500)?;
    conn.execute("UPDATE productos SET activo = 1, controla_stock = 0 WHERE codigo = ?1", libsql::params![CODIGO_ENVIO])
        .await
        .map_err(e500)?;
    let mut f = conn.query("SELECT id FROM productos WHERE codigo = ?1", libsql::params![CODIGO_ENVIO]).await.map_err(e500)?;
    f.next().await.map_err(e500)?.and_then(|f| f.get::<i64>(0).ok()).ok_or_else(|| e500("No se pudo preparar el envío."))
}

/// Deja la línea del envío del pedido con ese costo (0 = sin línea). La
/// línea nace entregada: no aparece en la comanda ni en Preparación.
async fn poner_envio(conn: &libsql::Connection, pedido_id: i64, costo: f64, usuario_id: i64) -> Result<(), Fallo> {
    let producto = producto_envio(conn).await?;
    conn.execute(
        "UPDATE pedido_items SET estado = 'ANULADO', motivo_anulacion = 'Cambió el costo de envío'
         WHERE pedido_id = ?1 AND producto_id = ?2 AND estado != 'ANULADO'",
        libsql::params![pedido_id, producto],
    )
    .await
    .map_err(e500)?;
    if costo > 0.0 {
        conn.execute(
            "INSERT INTO pedido_items (pedido_id, producto_id, nombre_producto, cantidad, precio_unitario, estado, usuario_id,
                                       fecha_envio, fecha_listo, fecha_entregado)
             VALUES (?1, ?2, ?3, 1, ?4, 'ENVIADO', ?5, datetime('now', 'localtime'), datetime('now', 'localtime'), datetime('now', 'localtime'))",
            libsql::params![pedido_id, producto, NOMBRE_ENVIO, costo, usuario_id],
        )
        .await
        .map_err(e500)?;
    }
    Ok(())
}

/// Platos activos del pedido (sin contar el envío).
async fn cuantos_platos(conn: &libsql::Connection, pedido_id: i64) -> Result<i64, Fallo> {
    let mut f = conn
        .query(
            "SELECT COUNT(*) FROM pedido_items i LEFT JOIN productos pr ON pr.id = i.producto_id
             WHERE i.pedido_id = ?1 AND i.estado != 'ANULADO' AND COALESCE(pr.codigo, '') != ?2",
            libsql::params![pedido_id, CODIGO_ENVIO],
        )
        .await
        .map_err(e500)?;
    Ok(f.next().await.map_err(e500)?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(0))
}

/// Manda a cocina lo que esté pendiente y devuelve esas líneas (la comanda).
async fn a_cocina(conn: &libsql::Connection, pedido_id: i64) -> Result<Vec<ItemComanda>, Fallo> {
    let mut filas = conn
        .query(
            "UPDATE pedido_items SET estado = 'ENVIADO', fecha_envio = datetime('now', 'localtime')
             WHERE pedido_id = ?1 AND estado = 'PENDIENTE'
             RETURNING id, nombre_producto, opciones, nota, CAST(cantidad AS REAL)",
            libsql::params![pedido_id],
        )
        .await
        .map_err(e500)?;
    let mut comanda = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        comanda.push(ItemComanda {
            id: f.get(0).unwrap_or_default(),
            nombre_producto: f.get(1).unwrap_or_default(),
            opciones: f.get(2).ok(),
            nota: f.get(3).ok(),
            cantidad: f.get(4).unwrap_or(0.0),
        });
    }
    comanda.sort_by_key(|i| i.id);
    Ok(comanda)
}

/// POST /delivery — pedido nuevo con sus platos y su envío. Contra entrega
/// (o recojo) va a cocina enseguida; adelantado espera el pago.
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(d): Json<DatosDelivery>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let l = revisar(&conn, &d).await?;
    if d.agregar.is_empty() {
        return Err(malo("Agrega los platos del pedido."));
    }
    let (id, comanda) = en_transaccion!(conn, |tx| {
        tx.execute(
            "INSERT INTO pedidos (tipo, cliente_nombre, usuario_id, notas) VALUES ('DELIVERY', ?1, ?2, ?3)",
            libsql::params![l.cliente_nombre.clone(), claims.sub, l.notas.clone()],
        )
        .await
        .map_err(e500)?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO pedido_delivery (pedido_id, cliente_id, telefono, entrega, direccion, referencia, canal, pago, metodo_pago, costo_envio)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            libsql::params![
                id, l.cliente_id, l.telefono.clone(), l.entrega.clone(), l.direccion.clone(), l.referencia.clone(),
                l.canal.clone(), l.pago.clone(), l.metodo_pago.clone(), l.costo_envio
            ],
        )
        .await
        .map_err(sin_migracion)?;
        insertar_items(&tx, id, &d.agregar, claims.sub).await?;
        poner_envio(&tx, id, l.costo_envio, claims.sub).await?;
        let comanda = if l.pago == "ADELANTADO" { Vec::new() } else { a_cocina(&tx, id).await? };
        Ok((id, comanda))
    });
    let mut p = leer(&conn, id).await?;
    p.comanda = comanda;
    Ok(Json(p))
}

/// Estado del pedido base y de su entrega, para las reglas.
async fn estados(conn: &libsql::Connection, id: i64) -> Result<(String, Option<String>, String, String), Fallo> {
    let mut f = conn
        .query(
            "SELECT p.estado, d.estado, COALESCE(d.pago, 'CONTRA_ENTREGA'), COALESCE(d.entrega, 'DELIVERY')
             FROM pedidos p LEFT JOIN pedido_delivery d ON d.pedido_id = p.id
             WHERE p.id = ?1 AND p.tipo = 'DELIVERY'",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    match f.next().await.map_err(e500)? {
        Some(r) => Ok((r.get(0).unwrap_or_default(), r.get(1).ok(), r.get(2).unwrap_or_default(), r.get(3).unwrap_or_default())),
        None => Err((StatusCode::NOT_FOUND, "El pedido no existe.".into())),
    }
}

/// PUT /delivery/:id — datos de entrega y pago, envío y platos. Lo nuevo va a
/// cocina si el pedido ya está en cocina (contra entrega, ya cobrado o con
/// platos enviados); en un adelantado que espera el pago, espera con lo demás.
/// Después de cobrar ya no cambian los platos, el envío ni la forma de pago.
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(d): Json<DatosDelivery>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado_pedido, estado_entrega, _, _) = estados(&conn, id).await?;
    if estado_pedido == "ANULADO" {
        return Err(conflicto("Este pedido fue anulado."));
    }
    if estado_entrega.as_deref() == Some("ENTREGADO") {
        return Err(conflicto("Este pedido ya se entregó."));
    }
    let l = revisar(&conn, &d).await?;
    let actual = leer(&conn, id).await?;
    let cobrado = estado_pedido != "ABIERTO";
    let cambia_envio = (actual.costo_envio - l.costo_envio).abs() > 0.004;
    if cobrado && (cambia_envio || !d.agregar.is_empty() || !d.quitar.is_empty()) {
        return Err(conflicto("El pedido ya se cobró: los platos y el envío ya no se pueden cambiar. Para algo más, registra otro pedido."));
    }
    if cobrado && l.pago != actual.pago {
        return Err(conflicto("El pedido ya se cobró: no se puede cambiar la forma de pago."));
    }
    if actual.estado != "EN_COCINA" && (!d.agregar.is_empty() || !d.quitar.is_empty()) {
        return Err(conflicto("El pedido ya salió: sus platos ya no se pueden cambiar."));
    }
    let motivo = texto(d.motivo_quitar.as_deref(), 140, "Motivo")?;
    for item in &d.quitar {
        match actual.lineas.iter().find(|x| x.id == *item) {
            None => return Err(conflicto("Uno de los platos ya no está en el pedido. Vuelve a abrirlo.")),
            Some(x) if x.estado != "PENDIENTE" && motivo.is_none() => {
                return Err(malo(format!("\"{}\" ya está en cocina: escribe el motivo para quitarlo.", x.nombre)));
            }
            _ => {}
        }
    }
    let ya_en_cocina = actual.lineas.iter().any(|x| x.estado == "ENVIADO");
    let comanda = en_transaccion!(conn, |tx| {
        tx.execute("UPDATE pedidos SET cliente_nombre = ?1, notas = ?2 WHERE id = ?3", libsql::params![l.cliente_nombre.clone(), l.notas.clone(), id])
            .await
            .map_err(e500)?;
        tx.execute(
            "INSERT INTO pedido_delivery (pedido_id, cliente_id, telefono, entrega, direccion, referencia, canal, pago, metodo_pago, costo_envio)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(pedido_id) DO UPDATE SET cliente_id = excluded.cliente_id, telefono = excluded.telefono,
                 entrega = excluded.entrega, direccion = excluded.direccion, referencia = excluded.referencia,
                 canal = excluded.canal, pago = excluded.pago, metodo_pago = excluded.metodo_pago,
                 costo_envio = excluded.costo_envio,
                 paga_con = CASE WHEN excluded.pago = 'CONTRA_ENTREGA' THEN paga_con END",
            libsql::params![
                id, l.cliente_id, l.telefono.clone(), l.entrega.clone(), l.direccion.clone(), l.referencia.clone(),
                l.canal.clone(), l.pago.clone(), l.metodo_pago.clone(), l.costo_envio
            ],
        )
        .await
        .map_err(sin_migracion)?;
        for item in &d.quitar {
            let borradas = tx
                .execute("DELETE FROM pedido_items WHERE id = ?1 AND pedido_id = ?2 AND estado = 'PENDIENTE'", libsql::params![*item, id])
                .await
                .map_err(e500)?;
            if borradas == 0 {
                tx.execute(
                    "UPDATE pedido_items SET estado = 'ANULADO', motivo_anulacion = ?1 WHERE id = ?2 AND pedido_id = ?3 AND estado = 'ENVIADO'",
                    libsql::params![motivo.clone(), *item, id],
                )
                .await
                .map_err(e500)?;
            }
        }
        if !d.agregar.is_empty() {
            insertar_items(&tx, id, &d.agregar, claims.sub).await?;
        }
        if cambia_envio {
            poner_envio(&tx, id, l.costo_envio, claims.sub).await?;
        }
        if cuantos_platos(&tx, id).await? == 0 {
            return Err(malo("El pedido debe tener al menos un plato. Si el cliente lo canceló, anula el pedido."));
        }
        // Va a cocina lo pendiente si el pedido ya está en cocina, o si dejó de
        // ser adelantado (contra entrega: se prepara sin esperar el pago).
        let en_cocina = cobrado || l.pago == "CONTRA_ENTREGA" || ya_en_cocina;
        Ok(if en_cocina { a_cocina(&tx, id).await? } else { Vec::new() })
    });
    let mut p = leer(&conn, id).await?;
    p.comanda = comanda;
    Ok(Json(p))
}

// ---------------------------------------------------------------------
// Recorrido: listo, despachar, volver, entregar
// ---------------------------------------------------------------------

/// POST /delivery/:id/listo — cocina terminó (si no usan la pantalla de Preparación).
pub async fn listo(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let p = leer(&conn, id).await?;
    if p.fase == "ESPERANDO_PAGO" {
        return Err(conflicto("Este pedido aún espera el pago: no ha entrado a cocina."));
    }
    if p.estado != "EN_COCINA" {
        return Err(conflicto("Este pedido ya salió."));
    }
    conn.execute(
        "UPDATE pedido_items SET fecha_listo = datetime('now', 'localtime')
         WHERE pedido_id = ?1 AND estado = 'ENVIADO' AND fecha_listo IS NULL AND fecha_entregado IS NULL",
        libsql::params![id],
    )
    .await
    .map_err(e500)?;
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct Despacho {
    /// Sin cuenta de repartidor: el nombre de quien lo lleva.
    #[serde(default)]
    pub repartidor: Option<String>,
    /// O el usuario repartidor (si no estaba asignado, queda asignado).
    #[serde(default)]
    pub repartidor_id: Option<i64>,
    /// Contra entrega en efectivo: con cuánto paga el cliente (para el vuelto).
    #[serde(default)]
    pub paga_con: Option<f64>,
}

/// Nombre de un usuario con rol Repartidor activo.
async fn nombre_repartidor(conn: &libsql::Connection, usuario_id: i64) -> Result<String, Fallo> {
    let mut f = conn
        .query(
            "SELECT u.nombre_completo FROM usuarios u JOIN roles r ON r.id = u.rol_id
             WHERE u.id = ?1 AND r.nombre = 'REPARTIDOR' AND COALESCE(u.activo, 1) = 1",
            libsql::params![usuario_id],
        )
        .await
        .map_err(e500)?;
    f.next()
        .await
        .map_err(e500)?
        .and_then(|r| r.get::<String>(0).ok())
        .ok_or_else(|| malo("Ese usuario no es un repartidor activo."))
}

/// Sale en camino: reglas comunes al despacho del cajero y al "salgo" del repartidor.
async fn salir(conn: &libsql::Connection, id: i64, repartidor: Option<(Option<i64>, String)>, paga_con: Option<f64>) -> Result<(), Fallo> {
    let (estado_pedido, estado_entrega, pago, entrega) = estados(conn, id).await?;
    let Some(estado_entrega) = estado_entrega else {
        return Err(conflicto("Completa los datos de entrega del pedido antes de despacharlo."));
    };
    if estado_pedido == "ANULADO" {
        return Err(conflicto("Este pedido fue anulado."));
    }
    if entrega != "DELIVERY" {
        return Err(conflicto("Este pedido es para recoger: no se despacha, se entrega en el local."));
    }
    if estado_entrega != "EN_COCINA" {
        return Err(conflicto("Este pedido ya salió."));
    }
    if pago == "ADELANTADO" && estado_pedido != "COBRADO" {
        return Err(conflicto("Este pedido es con pago adelantado y aún no se cobró. Confirma el pago antes de despacharlo."));
    }
    let p = leer(conn, id).await?;
    if p.lineas.is_empty() {
        return Err(malo("El pedido no tiene platos."));
    }
    if p.pendientes > 0 {
        return Err(conflicto("Hay platos que aún no entran a cocina."));
    }
    let (repartidor_id, nombre) = match repartidor {
        Some(r) => r,
        None => match (p.repartidor_id, p.repartidor.clone()) {
            (Some(rid), Some(nombre)) => (Some(rid), nombre),
            _ => return Err(malo("Elige quién lleva el pedido.")),
        },
    };
    let paga_con = paga_con.filter(|x| *x > 0.0 && x.is_finite() && *x <= 100_000.0).map(r2);
    if paga_con.is_some_and(|x| x + 0.004 < p.total) {
        return Err(malo(format!("Con S/ {:.2} no alcanza: el total es S/ {:.2}.", paga_con.unwrap_or(0.0), p.total)));
    }
    let paga_con = if estado_pedido == "COBRADO" { None } else { paga_con };
    // Asignado al salir (si no lo estaba): su orden es el siguiente del día.
    let orden = match (repartidor_id, p.repartidor_id == repartidor_id) {
        (Some(rid), false) => Some(siguiente_orden(conn, rid).await?),
        _ => p.orden_reparto,
    };
    let n = conn
        .execute(
            "UPDATE pedido_delivery SET estado = 'EN_CAMINO', repartidor = ?1, repartidor_id = ?2, orden_reparto = ?3, paga_con = ?4,
                    fecha_salida = datetime('now', 'localtime')
             WHERE pedido_id = ?5 AND estado = 'EN_COCINA'",
            libsql::params![nombre, repartidor_id, orden, paga_con, id],
        )
        .await
        .map_err(e500)?;
    if n == 0 {
        return Err(conflicto("Este pedido ya salió."));
    }
    // La comida salió: deja Preparación (aunque cocina no la haya marcado lista).
    conn.execute(
        "UPDATE pedido_items SET fecha_listo = COALESCE(fecha_listo, datetime('now', 'localtime')), fecha_entregado = datetime('now', 'localtime')
         WHERE pedido_id = ?1 AND estado = 'ENVIADO' AND fecha_entregado IS NULL",
        libsql::params![id],
    )
    .await
    .map_err(e500)?;
    Ok(())
}

/// Orden de entrega siguiente para ese repartidor en el día.
async fn siguiente_orden(conn: &libsql::Connection, repartidor_id: i64) -> Result<i64, Fallo> {
    let mut f = conn
        .query(
            "SELECT COALESCE(MAX(d.orden_reparto), 0) + 1 FROM pedido_delivery d JOIN pedidos p ON p.id = d.pedido_id
             WHERE d.repartidor_id = ?1 AND date(p.fecha_apertura) = date('now', 'localtime')",
            libsql::params![repartidor_id],
        )
        .await
        .map_err(e500)?;
    Ok(f.next().await.map_err(e500)?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(1))
}

/// POST /delivery/:id/despachar — el cajero lo despacha: con el repartidor
/// asignado, con un usuario repartidor o con el nombre de quien lo lleva.
pub async fn despachar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(d): Json<Despacho>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let repartidor = match (d.repartidor_id, texto(d.repartidor.as_deref(), 60, "Repartidor")?) {
        (Some(rid), _) => Some((Some(rid), nombre_repartidor(&conn, rid).await?)),
        (None, Some(nombre)) => Some((None, nombre)),
        (None, None) => None,
    };
    salir(&conn, id, repartidor, d.paga_con).await?;
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct Asignacion {
    /// None = quitar la asignación.
    #[serde(default)]
    pub repartidor_id: Option<i64>,
}

/// POST /delivery/:id/asignar — el cajero le asigna el pedido a un
/// repartidor (le aparece en "Mis entregas" con su orden del día).
pub async fn asignar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(d): Json<Asignacion>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (estado_pedido, estado_entrega, _, entrega) = estados(&conn, id).await?;
    if estado_pedido == "ANULADO" {
        return Err(conflicto("Este pedido fue anulado."));
    }
    if estado_entrega.as_deref() != Some("EN_COCINA") {
        return Err(conflicto("Este pedido ya salió: no se puede cambiar su repartidor."));
    }
    if entrega != "DELIVERY" {
        return Err(conflicto("Este pedido es para recoger: no lleva repartidor."));
    }
    match d.repartidor_id {
        Some(rid) => {
            let nombre = nombre_repartidor(&conn, rid).await?;
            let actual = leer(&conn, id).await?;
            if actual.repartidor_id != Some(rid) {
                let orden = siguiente_orden(&conn, rid).await?;
                conn.execute(
                    "UPDATE pedido_delivery SET repartidor_id = ?1, repartidor = ?2, orden_reparto = ?3 WHERE pedido_id = ?4",
                    libsql::params![rid, nombre, orden, id],
                )
                .await
                .map_err(e500)?;
            }
        }
        None => {
            conn.execute(
                "UPDATE pedido_delivery SET repartidor_id = NULL, repartidor = NULL, orden_reparto = NULL WHERE pedido_id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(e500)?;
        }
    }
    Ok(Json(leer(&conn, id).await?))
}

/// POST /delivery/:id/volver — se despachó por error: vuelve a cocina (sigue
/// asignado al mismo repartidor).
pub async fn volver(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let n = conn
        .execute(
            "UPDATE pedido_delivery SET estado = 'EN_COCINA', paga_con = NULL, fecha_salida = NULL,
                    repartidor = CASE WHEN repartidor_id IS NULL THEN NULL ELSE repartidor END
             WHERE pedido_id = ?1 AND estado = 'EN_CAMINO'",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    if n == 0 {
        return Err(conflicto("Este pedido no está en camino."));
    }
    Ok(Json(leer(&conn, id).await?))
}

/// Entregado: reglas comunes al cajero y al repartidor.
async fn marcar_entregado(conn: &libsql::Connection, id: i64, por_repartidor: bool) -> Result<(), Fallo> {
    let (estado_pedido, estado_entrega, _, entrega) = estados(conn, id).await?;
    let Some(estado_entrega) = estado_entrega else {
        return Err(conflicto("Completa los datos de entrega del pedido antes de entregarlo."));
    };
    if estado_pedido == "ANULADO" {
        return Err(conflicto("Este pedido fue anulado."));
    }
    if estado_entrega == "ENTREGADO" {
        return Err(conflicto("Este pedido ya se entregó."));
    }
    if entrega == "DELIVERY" && estado_entrega != "EN_CAMINO" {
        return Err(conflicto("Primero despacha el pedido con su repartidor."));
    }
    // En la puerta, el repartidor cobra (y con eso queda entregado): no lo
    // marca entregado sin cobrar.
    if por_repartidor && estado_pedido != "COBRADO" {
        return Err(conflicto("Este pedido se cobra al entregarlo: toca \"Cobrar\" delante del cliente."));
    }
    if entrega == "RECOJO" {
        let p = leer(conn, id).await?;
        if p.lineas.is_empty() || p.pendientes > 0 {
            return Err(conflicto("Hay platos que aún no entran a cocina."));
        }
        if estado_pedido != "COBRADO" {
            return Err(conflicto("Cobra el pedido antes de entregarlo."));
        }
    }
    conn.execute(
        "UPDATE pedido_delivery SET estado = 'ENTREGADO', fecha_entrega = datetime('now', 'localtime') WHERE pedido_id = ?1",
        libsql::params![id],
    )
    .await
    .map_err(e500)?;
    conn.execute(
        "UPDATE pedido_items SET fecha_listo = COALESCE(fecha_listo, datetime('now', 'localtime')), fecha_entregado = datetime('now', 'localtime')
         WHERE pedido_id = ?1 AND estado = 'ENVIADO' AND fecha_entregado IS NULL",
        libsql::params![id],
    )
    .await
    .map_err(e500)?;
    Ok(())
}

/// POST /delivery/:id/entregar — el cajero lo marca entregado (un recojo se
/// cobra antes; un delivery contra entrega sin cobrar queda por cobrar).
pub async fn entregar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    marcar_entregado(&conn, id, false).await?;
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct Rendir {
    pub repartidor_id: i64,
}

#[derive(Debug, Serialize)]
pub struct Rendido {
    pub pedidos: i64,
    pub efectivo: f64,
}

/// POST /delivery/rendir — el cajero recibió el efectivo que cobró ese
/// repartidor: sus cobros pendientes quedan rendidos.
pub async fn rendir(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(d): Json<Rendir>,
) -> Result<Json<Rendido>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let pendiente = rendicion(&conn).await?.into_iter().find(|r| r.repartidor_id == d.repartidor_id);
    let Some(pendiente) = pendiente else {
        return Err(conflicto("Ese repartidor no tiene nada por rendir."));
    };
    let ids = pendiente.pedidos.iter().map(|p| p.pedido_id.to_string()).collect::<Vec<_>>().join(",");
    conn.execute(
        &format!(
            "UPDATE pedido_delivery SET rendido_en = datetime('now', 'localtime'), rendido_por = ?1
             WHERE pedido_id IN ({}) AND rendido_en IS NULL",
            ids
        ),
        libsql::params![claims.sub],
    )
    .await
    .map_err(e500)?;
    Ok(Json(Rendido { pedidos: pendiente.pedidos.len() as i64, efectivo: pendiente.efectivo }))
}

// ---------------------------------------------------------------------
// El repartidor con su cuenta: "Mis entregas" (/reparto)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct MisEntregas {
    pub nombre: String,
    pub pedidos: Vec<PedidoDelivery>,
}

/// GET /reparto — los pedidos asignados al usuario: por salir y en camino
/// (en su orden de entrega) y lo entregado hoy.
pub async fn mis_entregas(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MisEntregas>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            &format!(
                "{} WHERE p.tipo = 'DELIVERY' AND p.estado != 'ANULADO' AND d.repartidor_id = ?1
                   AND (d.estado != 'ENTREGADO' OR date(d.fecha_entrega) = date('now', 'localtime'))
                 ORDER BY CASE d.estado WHEN 'EN_CAMINO' THEN 0 WHEN 'EN_COCINA' THEN 1 ELSE 2 END, d.orden_reparto, p.id",
                SELECT_PEDIDO
            ),
            libsql::params![claims.sub],
        )
        .await
        .map_err(sin_migracion)?;
    let mut pedidos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        pedidos.push(pedido_de(&f));
    }
    drop(filas);
    completar(&conn, &mut pedidos).await?;
    Ok(Json(MisEntregas { nombre: claims.nombre_completo.clone(), pedidos }))
}

/// El pedido debe ser de ese repartidor.
async fn exigir_suyo(conn: &libsql::Connection, id: i64, usuario_id: i64) -> Result<(), Fallo> {
    let mut f = conn
        .query("SELECT repartidor_id FROM pedido_delivery WHERE pedido_id = ?1", libsql::params![id])
        .await
        .map_err(sin_migracion)?;
    match f.next().await.map_err(e500)?.and_then(|r| r.get::<i64>(0).ok()) {
        Some(rid) if rid == usuario_id => Ok(()),
        _ => Err((StatusCode::FORBIDDEN, "Este pedido no está asignado a ti.".into())),
    }
}

#[derive(Debug, Deserialize)]
pub struct Salida {
    #[serde(default)]
    pub paga_con: Option<f64>,
}

/// POST /reparto/:id/salir — el repartidor sale con su pedido.
pub async fn reparto_salir(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(d): Json<Salida>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    exigir_suyo(&conn, id, claims.sub).await?;
    salir(&conn, id, None, d.paga_con).await?;
    Ok(Json(leer(&conn, id).await?))
}

/// POST /reparto/:id/entregar — entregó un pedido ya pagado (el contra
/// entrega se cobra y con eso queda entregado).
pub async fn reparto_entregar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<PedidoDelivery>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    exigir_suyo(&conn, id, claims.sub).await?;
    marcar_entregado(&conn, id, true).await?;
    Ok(Json(leer(&conn, id).await?))
}

/// Antes de una venta hecha por un repartidor: solo puede cobrar un pedido
/// suyo, contra entrega, en camino y sin cobrar. Cualquier otra venta, no.
pub async fn exigir_cobro_repartidor(conn: &libsql::Connection, pedido_id: Option<i64>, usuario_id: i64) -> Result<(), Fallo> {
    let prohibido = |m: &str| (StatusCode::FORBIDDEN, m.to_string());
    let Some(id) = pedido_id else {
        return Err(prohibido("El repartidor solo cobra los pedidos que lleva."));
    };
    let mut f = conn
        .query(
            "SELECT d.repartidor_id, d.estado, d.pago, p.estado FROM pedido_delivery d JOIN pedidos p ON p.id = d.pedido_id
             WHERE d.pedido_id = ?1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let Some(r) = f.next().await.map_err(e500)? else {
        return Err(prohibido("El repartidor solo cobra los pedidos que lleva."));
    };
    if r.get::<i64>(0).ok() != Some(usuario_id) {
        return Err(prohibido("Este pedido no está asignado a ti."));
    }
    if r.get::<String>(3).unwrap_or_default() != "ABIERTO" {
        return Err(conflicto("Este pedido ya se cobró."));
    }
    if r.get::<String>(1).unwrap_or_default() != "EN_CAMINO" {
        return Err(conflicto("Toca \"Salgo\" antes de cobrar este pedido."));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Para el cobro (ventas.rs)
// ---------------------------------------------------------------------

/// Antes de cobrar un pedido: si es un delivery, debe tener platos (no se
/// cobra solo el envío). Un pedido que no es delivery no se toca.
pub async fn revisar_cobro(conn: &libsql::Connection, pedido_id: i64) -> Result<(), Fallo> {
    let Ok(mut f) = conn.query("SELECT 1 FROM pedido_delivery WHERE pedido_id = ?1", libsql::params![pedido_id]).await else {
        return Ok(());
    };
    if f.next().await.ok().flatten().is_none() {
        return Ok(());
    }
    drop(f);
    if cuantos_platos(conn, pedido_id).await? == 0 {
        return Err(malo("Este pedido no tiene platos: no se puede cobrar solo el envío."));
    }
    Ok(())
}

/// Dentro del cobro: un delivery adelantado que esperaba el pago entra a
/// cocina al cobrarse. Si no es delivery, no hace nada.
pub async fn al_cobrar(conn: &libsql::Connection, pedido_id: i64, usuario_id: i64) {
    // Si la base aún no tiene la migración 0045, no hay deliveries: se ignora.
    let _ = conn.execute(
        "UPDATE pedido_items SET estado = 'ENVIADO', fecha_envio = datetime('now', 'localtime')
         WHERE pedido_id = ?1 AND estado = 'PENDIENTE'
           AND EXISTS (SELECT 1 FROM pedido_delivery d WHERE d.pedido_id = ?1)",
        libsql::params![pedido_id],
    )
    .await;
    // Lo cobró su repartidor delante del cliente: queda entregado.
    let _ = conn
        .execute(
            "UPDATE pedido_delivery SET estado = 'ENTREGADO', fecha_entrega = datetime('now', 'localtime')
             WHERE pedido_id = ?1 AND estado = 'EN_CAMINO' AND repartidor_id = ?2",
            libsql::params![pedido_id, usuario_id],
        )
        .await;
}
