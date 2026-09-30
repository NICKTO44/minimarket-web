//! Módulo "Cafetería / Restaurante" (atención en mesas).
//!
//! Solo funciona si el negocio lo activó (configuracion_tienda.modo_negocio
//! = 'RESTAURANTE'); si no, cada endpoint responde 403 y el resto del
//! sistema sigue igual que siempre. El cobro NO está aquí: un pedido se
//! cobra con el POS de siempre (POST /ventas con pedido_id), así la caja,
//! el pago mixto y las boletas/facturas funcionan exactamente igual.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::models::mesa::*;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn error(codigo: StatusCode, mensaje: impl Into<String>) -> (StatusCode, String) {
    (codigo, mensaje.into())
}

fn redondear_2(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

/// Texto opcional: recorta espacios y lo deja en None si queda vacío.
fn texto_limpio(valor: &Option<String>, maximo: usize) -> Option<String> {
    valor
        .as_ref()
        .map(|t| t.trim().chars().take(maximo).collect::<String>())
        .filter(|t| !t.is_empty())
}

// ============================================================
// Ayudantes compartidos (también los usa ventas.rs y registro.rs)
// ============================================================

/// true si el negocio tiene activado el modo Cafetería / Restaurante.
/// Si la columna todavía no existe (migración 0007 pendiente), es false.
pub async fn modo_restaurante(conn: &libsql::Connection) -> bool {
    let Ok(mut filas) = conn
        .query("SELECT COALESCE(modo_negocio, 'TIENDA') FROM configuracion_tienda LIMIT 1", ())
        .await
    else {
        return false;
    };
    match filas.next().await {
        Ok(Some(fila)) => fila.get::<String>(0).map(|m| m == "RESTAURANTE").unwrap_or(false),
        _ => false,
    }
}

async fn exigir_modulo(conn: &libsql::Connection) -> Result<(), (StatusCode, String)> {
    if modo_restaurante(conn).await {
        Ok(())
    } else {
        Err(error(
            StatusCode::FORBIDDEN,
            "La atención en mesas no está activada. Actívala en Configuración → Tipo de negocio.",
        ))
    }
}

/// Nombre del rol (ADMIN, CAJERO, MESERO, PREPARACION...). El id puede
/// variar entre negocios, por eso se compara por nombre.
pub async fn nombre_rol(conn: &libsql::Connection, rol_id: i64) -> Option<String> {
    let mut filas = conn.query("SELECT nombre FROM roles WHERE id = ?1", libsql::params![rol_id]).await.ok()?;
    filas.next().await.ok()??.get::<String>(0).ok()
}

/// true si el rol NO maneja dinero: Mesero (toma pedidos) y Preparación
/// (barra/cocina). No cobran ni abren caja.
pub async fn es_mesero(conn: &libsql::Connection, rol_id: i64) -> bool {
    matches!(nombre_rol(conn, rol_id).await.as_deref(), Some("MESERO") | Some("PREPARACION"))
}

/// Mesas de ejemplo para un negocio que recién activa el módulo (se
/// pueden renombrar o quitar en Configuración).
pub async fn sembrar_mesas_iniciales(conn: &libsql::Connection) -> Result<(), String> {
    let mut filas = conn
        .query("SELECT COUNT(*) FROM mesas", ())
        .await
        .map_err(|e| e.to_string())?;
    let existentes: i64 = match filas.next().await.map_err(|e| e.to_string())? {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };
    if existentes > 0 {
        return Ok(());
    }
    for n in 1..=6 {
        conn.execute(
            "INSERT INTO mesas (nombre, zona, capacidad, orden) VALUES (?1, 'Salón', 4, ?2)",
            libsql::params![format!("Mesa {}", n), n],
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============================================================
// Lecturas de pedidos
// ============================================================

// Ojo: los totales se fuerzan a REAL (0.0, no 0). libsql entra en pánico
// si se lee como f64 un valor que la base devolvió como entero, y un
// pedido sin productos suma 0 entero.
const SQL_RESUMEN: &str = "
    SELECT p.id, p.tipo, p.mesa_id, m.nombre, p.cliente_nombre, p.personas, p.usuario_id,
           u.nombre_completo, p.fecha_apertura,
           CAST(COALESCE(SUM(CASE WHEN i.estado != 'ANULADO' THEN i.cantidad * i.precio_unitario END), 0) AS REAL),
           CAST(COALESCE(SUM(CASE WHEN i.estado != 'ANULADO' THEN i.cantidad END), 0) AS REAL),
           COALESCE(SUM(CASE WHEN i.estado = 'PENDIENTE' THEN 1 ELSE 0 END), 0),
           p.estado, p.notas,
           CAST((julianday('now', 'localtime') - julianday(p.fecha_apertura)) * 1440 AS INTEGER),
           COALESCE(SUM(CASE WHEN i.estado = 'ENVIADO' AND i.fecha_listo IS NOT NULL AND i.fecha_entregado IS NULL THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN i.estado = 'ENVIADO' AND i.fecha_listo IS NULL THEN 1 ELSE 0 END), 0)
    FROM pedidos p
    LEFT JOIN mesas m ON m.id = p.mesa_id
    LEFT JOIN usuarios u ON u.id = p.usuario_id
    LEFT JOIN pedido_items i ON i.pedido_id = p.id";

fn fila_a_resumen(fila: &libsql::Row) -> PedidoResumen {
    PedidoResumen {
        id: fila.get(0).unwrap_or_default(),
        tipo: fila.get(1).unwrap_or_else(|_| "MESA".into()),
        mesa_id: fila.get(2).ok(),
        mesa_nombre: fila.get(3).ok(),
        cliente_nombre: fila.get(4).ok(),
        personas: fila.get(5).ok(),
        usuario_id: fila.get(6).unwrap_or_default(),
        mesero: fila.get(7).ok(),
        fecha_apertura: fila.get(8).unwrap_or_default(),
        total: redondear_2(fila.get(9).unwrap_or(0.0)),
        cantidad_items: fila.get(10).unwrap_or(0.0),
        pendientes: fila.get(11).unwrap_or(0),
        minutos_abierto: fila.get::<i64>(14).unwrap_or(0).max(0),
        listos: fila.get(15).unwrap_or(0),
        preparando: fila.get(16).unwrap_or(0),
    }
}

async fn resumenes_abiertos(conn: &libsql::Connection) -> Result<Vec<PedidoResumen>, (StatusCode, String)> {
    let sql = format!("{} WHERE p.estado = 'ABIERTO' GROUP BY p.id ORDER BY p.fecha_apertura, p.id", SQL_RESUMEN);
    let mut filas = conn.query(&sql, ()).await.map_err(e500)?;
    let mut lista = Vec::new();
    while let Some(fila) = filas.next().await.map_err(e500)? {
        lista.push(fila_a_resumen(&fila));
    }
    Ok(lista)
}

async fn items_de_pedido(
    conn: &libsql::Connection,
    pedido_id: i64,
    solo_estado: Option<&str>,
) -> Result<Vec<ItemPedido>, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT id, producto_id, nombre_producto, opciones, nota, CAST(cantidad AS REAL), CAST(precio_unitario AS REAL), estado,
                    fecha_creacion, fecha_envio, fecha_listo, fecha_entregado
             FROM pedido_items
             WHERE pedido_id = ?1 AND (?2 IS NULL OR estado = ?2)
             ORDER BY id",
            libsql::params![pedido_id, solo_estado.map(|s| s.to_string())],
        )
        .await
        .map_err(e500)?;
    let mut items = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(5).unwrap_or(0.0);
        let precio: f64 = f.get(6).unwrap_or(0.0);
        items.push(ItemPedido {
            id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            nombre_producto: f.get(2).unwrap_or_default(),
            opciones: f.get(3).ok(),
            nota: f.get(4).ok(),
            cantidad,
            precio_unitario: precio,
            subtotal: redondear_2(cantidad * precio),
            estado: f.get(7).unwrap_or_default(),
            fecha_creacion: f.get(8).unwrap_or_default(),
            fecha_envio: f.get(9).ok(),
            fecha_listo: f.get(10).ok(),
            fecha_entregado: f.get(11).ok(),
        });
    }
    Ok(items)
}

/// Pedido con todas sus líneas en UNA sola consulta (antes eran dos). Cada
/// viaje a la base cuesta ~0.1-0.2 s desde Perú, así que se cuentan.
async fn detalle_pedido(conn: &libsql::Connection, pedido_id: i64) -> Result<PedidoDetalle, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT p.id, p.tipo, p.mesa_id, m.nombre, p.cliente_nombre, p.personas, p.usuario_id,
                    u.nombre_completo, p.fecha_apertura, p.estado, p.notas,
                    CAST((julianday('now', 'localtime') - julianday(p.fecha_apertura)) * 1440 AS INTEGER),
                    i.id, i.producto_id, i.nombre_producto, i.opciones, i.nota,
                    CAST(i.cantidad AS REAL), CAST(i.precio_unitario AS REAL), i.estado,
                    i.fecha_creacion, i.fecha_envio, i.fecha_listo, i.fecha_entregado
             FROM pedidos p
             LEFT JOIN mesas m ON m.id = p.mesa_id
             LEFT JOIN usuarios u ON u.id = p.usuario_id
             LEFT JOIN pedido_items i ON i.pedido_id = p.id
             WHERE p.id = ?1
             ORDER BY i.id",
            libsql::params![pedido_id],
        )
        .await
        .map_err(e500)?;

    let mut cabecera: Option<(PedidoResumen, String, Option<String>)> = None;
    let mut items = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        if cabecera.is_none() {
            let resumen = PedidoResumen {
                id: f.get(0).unwrap_or_default(),
                tipo: f.get(1).unwrap_or_else(|_| "MESA".into()),
                mesa_id: f.get(2).ok(),
                mesa_nombre: f.get(3).ok(),
                cliente_nombre: f.get(4).ok(),
                personas: f.get(5).ok(),
                usuario_id: f.get(6).unwrap_or_default(),
                mesero: f.get(7).ok(),
                fecha_apertura: f.get(8).unwrap_or_default(),
                total: 0.0,
                cantidad_items: 0.0,
                pendientes: 0,
                minutos_abierto: f.get::<i64>(11).unwrap_or(0).max(0),
                listos: 0,
                preparando: 0,
            };
            cabecera = Some((resumen, f.get(9).unwrap_or_default(), f.get(10).ok()));
        }
        // Pedido sin líneas: el LEFT JOIN trae i.id en NULL.
        let Ok(item_id) = f.get::<i64>(12) else { continue };
        let cantidad: f64 = f.get(17).unwrap_or(0.0);
        let precio: f64 = f.get(18).unwrap_or(0.0);
        items.push(ItemPedido {
            id: item_id,
            producto_id: f.get(13).unwrap_or_default(),
            nombre_producto: f.get(14).unwrap_or_default(),
            opciones: f.get(15).ok(),
            nota: f.get(16).ok(),
            cantidad,
            precio_unitario: precio,
            subtotal: redondear_2(cantidad * precio),
            estado: f.get(19).unwrap_or_default(),
            fecha_creacion: f.get(20).unwrap_or_default(),
            fecha_envio: f.get(21).ok(),
            fecha_listo: f.get(22).ok(),
            fecha_entregado: f.get(23).ok(),
        });
    }

    let (mut resumen, estado, notas) =
        cabecera.ok_or_else(|| error(StatusCode::NOT_FOUND, "El pedido no existe."))?;
    let activos = items.iter().filter(|i| i.estado != "ANULADO");
    resumen.total = redondear_2(activos.clone().map(|i| i.cantidad * i.precio_unitario).sum());
    resumen.cantidad_items = activos.map(|i| i.cantidad).sum();
    resumen.pendientes = items.iter().filter(|i| i.estado == "PENDIENTE").count() as i64;
    let enviados = items.iter().filter(|i| i.estado == "ENVIADO" && i.fecha_entregado.is_none());
    resumen.listos = enviados.clone().filter(|i| i.fecha_listo.is_some()).count() as i64;
    resumen.preparando = enviados.filter(|i| i.fecha_listo.is_none()).count() as i64;
    Ok(PedidoDetalle { resumen, estado, notas, items })
}

/// Módulo activo + pedido ABIERTO en UNA consulta (antes eran dos).
async fn exigir_modulo_y_pedido(conn: &libsql::Connection, pedido_id: i64) -> Result<(), (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT COALESCE((SELECT modo_negocio FROM configuracion_tienda LIMIT 1), 'TIENDA'),
                    (SELECT estado FROM pedidos WHERE id = ?1)",
            libsql::params![pedido_id],
        )
        .await
        .map_err(e500)?;
    let fila = filas.next().await.map_err(e500)?.ok_or_else(|| e500("sin respuesta"))?;
    let modo: String = fila.get(0).unwrap_or_default();
    if modo != "RESTAURANTE" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "La atención en mesas no está activada. Actívala en Configuración → Tipo de negocio.",
        ));
    }
    match fila.get::<String>(1).ok().as_deref() {
        Some("ABIERTO") => Ok(()),
        Some("COBRADO") => Err(error(StatusCode::CONFLICT, "Este pedido ya fue cobrado.")),
        Some(_) => Err(error(StatusCode::CONFLICT, "Este pedido fue anulado.")),
        None => Err(error(StatusCode::NOT_FOUND, "El pedido no existe.")),
    }
}

fn marcadores(desde: usize, cantidad: usize) -> String {
    (desde..desde + cantidad).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(", ")
}

// ============================================================
// Mesas
// ============================================================

pub async fn listar_mesas(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<MesaEstado>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;

    let abiertos = resumenes_abiertos(&conn).await?;
    let mut por_mesa: HashMap<i64, PedidoResumen> = HashMap::new();
    for p in abiertos {
        if let Some(mesa_id) = p.mesa_id {
            por_mesa.insert(mesa_id, p);
        }
    }

    let mut filas = conn
        .query(
            "SELECT id, nombre, zona, COALESCE(capacidad, 4), COALESCE(orden, 0) FROM mesas
             WHERE activo = 1 ORDER BY zona, orden, id",
            (),
        )
        .await
        .map_err(e500)?;
    let mut mesas = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        mesas.push(MesaEstado {
            id,
            nombre: f.get(1).unwrap_or_default(),
            zona: f.get(2).unwrap_or_else(|_| "Salón".into()),
            capacidad: f.get(3).unwrap_or(4),
            orden: f.get(4).unwrap_or(0),
            pedido: por_mesa.remove(&id),
        });
    }
    Ok(Json(mesas))
}

fn validar_mesa(payload: &MesaPayload) -> Result<(String, String, i64, i64), (StatusCode, String)> {
    let nombre = payload.nombre.trim().chars().take(40).collect::<String>();
    if nombre.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Ponle un nombre a la mesa (por ejemplo \"Mesa 7\")."));
    }
    let zona = texto_limpio(&payload.zona, 30).unwrap_or_else(|| "Salón".into());
    let capacidad = payload.capacidad.unwrap_or(4).clamp(1, 50);
    let orden = payload.orden.unwrap_or(0);
    Ok((nombre, zona, capacidad, orden))
}

pub async fn crear_mesa(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<MesaPayload>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let (nombre, zona, capacidad, orden) = validar_mesa(&payload)?;
    conn.execute(
        "INSERT INTO mesas (nombre, zona, capacidad, orden) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![nombre, zona, capacidad, orden],
    )
    .await
    .map_err(e500)?;
    Ok(Json(Respuesta { success: true, message: "Mesa agregada".into() }))
}

pub async fn actualizar_mesa(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<MesaPayload>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let (nombre, zona, capacidad, orden) = validar_mesa(&payload)?;
    let filas = conn
        .execute(
            "UPDATE mesas SET nombre = ?1, zona = ?2, capacidad = ?3, orden = ?4 WHERE id = ?5 AND activo = 1",
            libsql::params![nombre, zona, capacidad, orden, id],
        )
        .await
        .map_err(e500)?;
    if filas == 0 {
        return Err(error(StatusCode::NOT_FOUND, "La mesa no existe."));
    }
    Ok(Json(Respuesta { success: true, message: "Mesa actualizada".into() }))
}

pub async fn desactivar_mesa(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT COUNT(*) FROM pedidos WHERE mesa_id = ?1 AND estado = 'ABIERTO'",
            libsql::params![id],
        )
        .await
        .map_err(e500)?;
    let abiertos: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };
    if abiertos > 0 {
        return Err(error(StatusCode::CONFLICT, "Esa mesa tiene un pedido abierto. Cóbralo o anúlalo antes de quitarla."));
    }
    // Se desactiva (no se borra) para que los pedidos antiguos conserven su mesa.
    conn.execute("UPDATE mesas SET activo = 0 WHERE id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    Ok(Json(Respuesta { success: true, message: "Mesa quitada".into() }))
}

// ============================================================
// Pedidos
// ============================================================

pub async fn listar_pedidos_abiertos(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<PedidoResumen>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    Ok(Json(resumenes_abiertos(&conn).await?))
}

pub async fn obtener_pedido(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    Ok(Json(detalle_pedido(&conn, id).await?))
}

/// Abre un pedido. Si la mesa ya tiene uno abierto (dos meseros tocaron la
/// misma mesa), devuelve ese mismo en vez de fallar.
pub async fn abrir_pedido(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<NuevoPedido>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;

    let tipo = payload.tipo.trim().to_uppercase();
    if !["MESA", "LLEVAR", "DELIVERY"].contains(&tipo.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Tipo de pedido no válido."));
    }
    let personas = payload.personas.map(|p| p.clamp(1, 99));
    let cliente = texto_limpio(&payload.cliente_nombre, 60);

    let mesa_id = if tipo == "MESA" {
        let mesa_id = payload
            .mesa_id
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Elige una mesa."))?;
        let mut filas = conn
            .query("SELECT COUNT(*) FROM mesas WHERE id = ?1 AND activo = 1", libsql::params![mesa_id])
            .await
            .map_err(e500)?;
        let existe: i64 = match filas.next().await.map_err(e500)? {
            Some(f) => f.get(0).unwrap_or(0),
            None => 0,
        };
        if existe == 0 {
            return Err(error(StatusCode::NOT_FOUND, "La mesa no existe."));
        }
        let mut filas = conn
            .query(
                "SELECT id FROM pedidos WHERE mesa_id = ?1 AND estado = 'ABIERTO' LIMIT 1",
                libsql::params![mesa_id],
            )
            .await
            .map_err(e500)?;
        if let Some(f) = filas.next().await.map_err(e500)? {
            let existente: i64 = f.get(0).unwrap_or_default();
            return Ok(Json(detalle_pedido(&conn, existente).await?));
        }
        Some(mesa_id)
    } else {
        None
    };

    let insertado = conn
        .execute(
            "INSERT INTO pedidos (tipo, mesa_id, cliente_nombre, personas, usuario_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            libsql::params![tipo, mesa_id, cliente, personas, claims.sub],
        )
        .await;

    let pedido_id = match insertado {
        Ok(_) => conn.last_insert_rowid(),
        Err(e) => {
            // Choque con el índice único (otro mesero abrió la mesa en el
            // mismo instante): se devuelve el pedido que ganó.
            if let Some(mesa_id) = mesa_id {
                let mut filas = conn
                    .query(
                        "SELECT id FROM pedidos WHERE mesa_id = ?1 AND estado = 'ABIERTO' LIMIT 1",
                        libsql::params![mesa_id],
                    )
                    .await
                    .map_err(e500)?;
                if let Some(f) = filas.next().await.map_err(e500)? {
                    let existente: i64 = f.get(0).unwrap_or_default();
                    return Ok(Json(detalle_pedido(&conn, existente).await?));
                }
            }
            return Err(e500(e));
        }
    };

    Ok(Json(detalle_pedido(&conn, pedido_id).await?))
}

/// Agrega productos al pedido. El precio lo calcula el servidor (precio del
/// producto + extras de los modificadores elegidos); nunca se confía en un
/// precio enviado por el navegador.
pub async fn agregar_items(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(pedido_id): Path<i64>,
    Json(payload): Json<AgregarItems>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;

    if payload.items.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "No hay productos para agregar."));
    }
    if payload.items.len() > 100 {
        return Err(error(StatusCode::BAD_REQUEST, "Demasiados productos en un solo envío."));
    }
    for item in &payload.items {
        if !(item.cantidad > 0.0 && item.cantidad <= 999.0) {
            return Err(error(StatusCode::BAD_REQUEST, "La cantidad debe estar entre 1 y 999."));
        }
    }

    // Todo en pocas consultas, sin importar cuántos productos vengan:
    // 1) productos + los grupos de opciones que les aplican (un JOIN),
    // 2) las opciones elegidas, 3) un solo INSERT con todas las líneas.
    let mut ids_productos: Vec<i64> = payload.items.iter().map(|i| i.producto_id).collect();
    ids_productos.sort_unstable();
    ids_productos.dedup();

    let mut filas = conn
        .query(
            &format!(
                "SELECT p.id, p.nombre, CAST(p.precio AS REAL), p.activo,
                        g.id, g.nombre, g.obligatorio, g.multiple
                 FROM productos p
                 LEFT JOIN producto_grupos_modificador pg ON pg.producto_id = p.id
                 LEFT JOIN grupos_modificadores g ON g.id = pg.grupo_id AND g.activo = 1
                 WHERE p.id IN ({})
                 ORDER BY p.id, g.orden, g.id",
                marcadores(1, ids_productos.len())
            ),
            libsql::params_from_iter(ids_productos.clone()),
        )
        .await
        .map_err(e500)?;
    // producto_id -> (nombre, precio, activo, grupos[(id, nombre, obligatorio, multiple)])
    let mut productos: HashMap<i64, (String, f64, bool, Vec<(i64, String, bool, bool)>)> = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        let entrada = productos.entry(id).or_insert_with(|| {
            (f.get(1).unwrap_or_default(), f.get(2).unwrap_or(0.0), f.get::<i64>(3).unwrap_or(0) == 1, Vec::new())
        });
        if let Ok(grupo_id) = f.get::<i64>(4) {
            entrada.3.push((
                grupo_id,
                f.get(5).unwrap_or_default(),
                f.get::<i64>(6).unwrap_or(0) == 1,
                f.get::<i64>(7).unwrap_or(0) == 1,
            ));
        }
    }

    let mut ids_opciones: Vec<i64> = payload.items.iter().flat_map(|i| i.opcion_ids.iter().copied()).collect();
    ids_opciones.sort_unstable();
    ids_opciones.dedup();
    // opcion_id -> (grupo_id, nombre, precio_extra)
    let mut opciones: HashMap<i64, (i64, String, f64)> = HashMap::new();
    if !ids_opciones.is_empty() {
        let mut filas = conn
            .query(
                &format!(
                    "SELECT id, grupo_id, nombre, CAST(precio_extra AS REAL) FROM opciones_modificador
                     WHERE activo = 1 AND id IN ({})",
                    marcadores(1, ids_opciones.len())
                ),
                libsql::params_from_iter(ids_opciones.clone()),
            )
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            opciones.insert(
                f.get(0).unwrap_or_default(),
                (f.get(1).unwrap_or_default(), f.get(2).unwrap_or_default(), f.get(3).unwrap_or(0.0)),
            );
        }
    }

    // Validar TODO antes de insertar, para no dejar el pedido a medias.
    let mut valores: Vec<libsql::Value> = Vec::new();
    for item in &payload.items {
        let (nombre, precio, activo, grupos) = productos
            .get(&item.producto_id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "Uno de los productos ya no existe."))?;
        if !*activo {
            return Err(error(StatusCode::BAD_REQUEST, format!("\"{}\" está desactivado.", nombre)));
        }

        let mut elegidas_por_grupo: HashMap<i64, Vec<(String, f64)>> = HashMap::new();
        let mut vistas = std::collections::HashSet::new();
        for opcion_id in &item.opcion_ids {
            if !vistas.insert(*opcion_id) {
                continue;
            }
            let (grupo_id, texto, precio_extra) = opciones
                .get(opcion_id)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Una de las opciones elegidas ya no existe."))?;
            if !grupos.iter().any(|g| g.0 == *grupo_id) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("Una de las opciones elegidas no corresponde a \"{}\".", nombre),
                ));
            }
            elegidas_por_grupo.entry(*grupo_id).or_default().push((texto.clone(), *precio_extra));
        }

        let mut textos = Vec::new();
        let mut extras = 0.0;
        for (grupo_id, grupo_nombre, obligatorio, multiple) in grupos {
            let elegidas = elegidas_por_grupo.get(grupo_id).cloned().unwrap_or_default();
            if *obligatorio && elegidas.is_empty() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("Elige {} para \"{}\".", grupo_nombre.to_lowercase(), nombre),
                ));
            }
            if !*multiple && elegidas.len() > 1 {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    format!("En \"{}\" solo se puede elegir una opción.", grupo_nombre),
                ));
            }
            for (texto, precio_extra) in elegidas {
                extras += precio_extra;
                textos.push(texto);
            }
        }

        let opciones_texto = if textos.is_empty() { None } else { Some(textos.join(", ")) };
        valores.push(pedido_id.into());
        valores.push(item.producto_id.into());
        valores.push(nombre.clone().into());
        valores.push(opciones_texto.into());
        valores.push(texto_limpio(&item.nota, 140).into());
        valores.push(item.cantidad.into());
        valores.push(redondear_2(precio + extras).into());
        valores.push(claims.sub.into());
    }

    let filas_sql = (0..payload.items.len())
        .map(|n| format!("({})", marcadores(n * 8 + 1, 8)))
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute(
        &format!(
            "INSERT INTO pedido_items (pedido_id, producto_id, nombre_producto, opciones, nota, cantidad, precio_unitario, usuario_id)
             VALUES {}",
            filas_sql
        ),
        libsql::params_from_iter(valores),
    )
    .await
    .map_err(e500)?;

    Ok(Json(detalle_pedido(&conn, pedido_id).await?))
}

/// Estado actual de una línea del pedido.
async fn estado_item(
    conn: &libsql::Connection,
    pedido_id: i64,
    item_id: i64,
) -> Result<String, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT estado FROM pedido_items WHERE id = ?1 AND pedido_id = ?2",
            libsql::params![item_id, pedido_id],
        )
        .await
        .map_err(e500)?;
    match filas.next().await.map_err(e500)? {
        Some(f) => Ok(f.get(0).unwrap_or_default()),
        None => Err(error(StatusCode::NOT_FOUND, "Ese producto ya no está en el pedido.")),
    }
}

/// Cambia la cantidad (y opcionalmente la nota) de una línea que TODAVÍA
/// no se mandó a preparar.
pub async fn cambiar_cantidad_item(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path((pedido_id, item_id)): Path<(i64, i64)>,
    Json(payload): Json<CambiarCantidad>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;
    if !(payload.cantidad > 0.0 && payload.cantidad <= 999.0) {
        return Err(error(StatusCode::BAD_REQUEST, "La cantidad debe estar entre 1 y 999."));
    }
    // El UPDATE solo toca líneas PENDIENTES; si no cambió nada, recién ahí
    // se averigua por qué (así el caso normal cuesta un viaje menos).
    let filas = match &payload.nota {
        Some(_) => conn
            .execute(
                "UPDATE pedido_items SET cantidad = ?1, nota = ?2 WHERE id = ?3 AND pedido_id = ?4 AND estado = 'PENDIENTE'",
                libsql::params![payload.cantidad, texto_limpio(&payload.nota, 140), item_id, pedido_id],
            )
            .await
            .map_err(e500)?,
        None => conn
            .execute(
                "UPDATE pedido_items SET cantidad = ?1 WHERE id = ?2 AND pedido_id = ?3 AND estado = 'PENDIENTE'",
                libsql::params![payload.cantidad, item_id, pedido_id],
            )
            .await
            .map_err(e500)?,
    };
    if filas == 0 {
        estado_item(&conn, pedido_id, item_id).await?;
        return Err(error(
            StatusCode::CONFLICT,
            "Ese producto ya se mandó a preparar. Para pedir más, agrégalo de nuevo.",
        ));
    }
    Ok(Json(detalle_pedido(&conn, pedido_id).await?))
}

/// Quita una línea. Si aún no se preparaba, se borra. Si ya salió en una
/// comanda, se ANULA con motivo (queda el rastro) y el mesero no puede
/// hacerlo: lo hace el cajero o el administrador.
pub async fn quitar_item(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path((pedido_id, item_id)): Path<(i64, i64)>,
    Json(payload): Json<MotivoPayload>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;

    // Lo normal es quitar algo que aún no se preparaba: se intenta borrar
    // directo y solo si no era PENDIENTE se revisa qué es.
    let borradas = conn
        .execute(
            "DELETE FROM pedido_items WHERE id = ?1 AND pedido_id = ?2 AND estado = 'PENDIENTE'",
            libsql::params![item_id, pedido_id],
        )
        .await
        .map_err(e500)?;
    if borradas == 0 && estado_item(&conn, pedido_id, item_id).await? == "ENVIADO" {
        if es_mesero(&conn, claims.rol_id).await {
            return Err(error(
                StatusCode::FORBIDDEN,
                "Ese producto ya se está preparando. Pide al cajero o al administrador que lo anule.",
            ));
        }
        let motivo = texto_limpio(&payload.motivo, 140)
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Escribe el motivo de la anulación."))?;
        conn.execute(
            "UPDATE pedido_items SET estado = 'ANULADO', motivo_anulacion = ?1 WHERE id = ?2 AND pedido_id = ?3",
            libsql::params![motivo, item_id, pedido_id],
        )
        .await
        .map_err(e500)?;
    }
    Ok(Json(detalle_pedido(&conn, pedido_id).await?))
}

/// Manda a preparar todo lo PENDIENTE y devuelve la comanda para imprimir.
pub async fn enviar_a_preparar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(pedido_id): Path<i64>,
) -> Resultado<Comanda> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;

    // Un solo UPDATE marca todo lo pendiente y devuelve qué líneas fueron
    // (si alguien agrega otra justo después, sale en la siguiente comanda).
    let mut filas = conn
        .query(
            "UPDATE pedido_items SET estado = 'ENVIADO', fecha_envio = datetime('now', 'localtime')
             WHERE pedido_id = ?1 AND estado = 'PENDIENTE'
             RETURNING id, fecha_envio",
            libsql::params![pedido_id],
        )
        .await
        .map_err(e500)?;
    let mut ids: Vec<i64> = Vec::new();
    let mut ahora = String::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        ids.push(f.get(0).unwrap_or_default());
        ahora = f.get(1).unwrap_or_default();
    }
    if ids.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "No hay productos nuevos para enviar."));
    }

    let detalle = detalle_pedido(&conn, pedido_id).await?;
    let items = detalle.items.into_iter().filter(|i| ids.contains(&i.id)).collect();
    Ok(Json(Comanda { pedido: detalle.resumen, items, fecha: ahora }))
}

/// Pasa el pedido a otra mesa libre (el cliente se cambió de mesa).
pub async fn mover_pedido(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(pedido_id): Path<i64>,
    Json(payload): Json<MoverPedido>,
) -> Resultado<PedidoDetalle> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;

    let mut filas = conn
        .query("SELECT COUNT(*) FROM mesas WHERE id = ?1 AND activo = 1", libsql::params![payload.mesa_id])
        .await
        .map_err(e500)?;
    let existe: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };
    if existe == 0 {
        return Err(error(StatusCode::NOT_FOUND, "La mesa no existe."));
    }
    let mut filas = conn
        .query(
            "SELECT COUNT(*) FROM pedidos WHERE mesa_id = ?1 AND estado = 'ABIERTO' AND id != ?2",
            libsql::params![payload.mesa_id, pedido_id],
        )
        .await
        .map_err(e500)?;
    let ocupada: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };
    if ocupada > 0 {
        return Err(error(StatusCode::CONFLICT, "Esa mesa está ocupada. Elige una mesa libre."));
    }
    conn.execute(
        "UPDATE pedidos SET tipo = 'MESA', mesa_id = ?1 WHERE id = ?2",
        libsql::params![payload.mesa_id, pedido_id],
    )
    .await
    .map_err(|_| error(StatusCode::CONFLICT, "Esa mesa se acaba de ocupar. Elige otra."))?;
    Ok(Json(detalle_pedido(&conn, pedido_id).await?))
}

/// Anula el pedido completo (el cliente se fue sin consumir, error, etc.).
/// Si ya se preparó algo, solo cajero/administrador y con motivo.
pub async fn anular_pedido(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(pedido_id): Path<i64>,
    Json(payload): Json<MotivoPayload>,
) -> Resultado<Respuesta> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo_y_pedido(&conn, pedido_id).await?;

    let enviados = items_de_pedido(&conn, pedido_id, Some("ENVIADO")).await?;
    let motivo = texto_limpio(&payload.motivo, 140);
    if !enviados.is_empty() {
        if es_mesero(&conn, claims.rol_id).await {
            return Err(error(
                StatusCode::FORBIDDEN,
                "Este pedido ya tiene productos en preparación. Solo el cajero o el administrador puede anularlo.",
            ));
        }
        if motivo.is_none() {
            return Err(error(StatusCode::BAD_REQUEST, "Escribe el motivo de la anulación."));
        }
    }
    conn.execute(
        "UPDATE pedidos SET estado = 'ANULADO', motivo_anulacion = ?1, fecha_cierre = datetime('now', 'localtime')
         WHERE id = ?2 AND estado = 'ABIERTO'",
        libsql::params![motivo, pedido_id],
    )
    .await
    .map_err(e500)?;
    Ok(Json(Respuesta { success: true, message: "Pedido anulado".into() }))
}

// ============================================================
// Modificadores (Tamaño, Tipo de leche, Extras...)
// ============================================================

pub async fn listar_modificadores(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<GrupoModificador>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;

    let mut filas = conn
        .query(
            "SELECT id, nombre, obligatorio, multiple FROM grupos_modificadores WHERE activo = 1 ORDER BY orden, id",
            (),
        )
        .await
        .map_err(e500)?;
    let mut grupos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        grupos.push(GrupoModificador {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            obligatorio: f.get::<i64>(2).unwrap_or(0) == 1,
            multiple: f.get::<i64>(3).unwrap_or(0) == 1,
            opciones: Vec::new(),
            producto_ids: Vec::new(),
        });
    }

    let mut filas = conn
        .query(
            "SELECT grupo_id, id, nombre, CAST(precio_extra AS REAL) FROM opciones_modificador WHERE activo = 1 ORDER BY orden, id",
            (),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let grupo_id: i64 = f.get(0).unwrap_or_default();
        if let Some(g) = grupos.iter_mut().find(|g| g.id == grupo_id) {
            g.opciones.push(OpcionModificador {
                id: f.get(1).ok(),
                nombre: f.get(2).unwrap_or_default(),
                precio_extra: f.get(3).unwrap_or(0.0),
            });
        }
    }

    let mut filas = conn
        .query("SELECT grupo_id, producto_id FROM producto_grupos_modificador", ())
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let grupo_id: i64 = f.get(0).unwrap_or_default();
        if let Some(g) = grupos.iter_mut().find(|g| g.id == grupo_id) {
            g.producto_ids.push(f.get(1).unwrap_or_default());
        }
    }

    Ok(Json(grupos))
}

fn validar_grupo(payload: &GrupoPayload) -> Result<(String, Vec<OpcionModificador>), (StatusCode, String)> {
    let nombre = payload.nombre.trim().chars().take(40).collect::<String>();
    if nombre.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Ponle un nombre al grupo (por ejemplo \"Tamaño\")."));
    }
    let mut opciones = Vec::new();
    for o in &payload.opciones {
        let nombre_opcion = o.nombre.trim().chars().take(40).collect::<String>();
        if nombre_opcion.is_empty() {
            continue;
        }
        if !(o.precio_extra >= 0.0 && o.precio_extra <= 9999.0) {
            return Err(error(StatusCode::BAD_REQUEST, "El precio extra no puede ser negativo."));
        }
        opciones.push(OpcionModificador { id: o.id, nombre: nombre_opcion, precio_extra: redondear_2(o.precio_extra) });
    }
    if opciones.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Agrega al menos una opción (por ejemplo \"Grande\")."));
    }
    Ok((nombre, opciones))
}

async fn guardar_opciones_y_productos(
    conn: &libsql::Connection,
    grupo_id: i64,
    opciones: &[OpcionModificador],
    producto_ids: &[i64],
) -> Result<(), (StatusCode, String)> {
    // Opciones: las que traen id se actualizan, las nuevas se insertan y
    // las que ya no vienen se desactivan (no se borran: pedidos antiguos).
    let mut conservadas: Vec<i64> = Vec::new();
    for (orden, o) in opciones.iter().enumerate() {
        let actualizada = match o.id {
            Some(id) => conn
                .execute(
                    "UPDATE opciones_modificador SET nombre = ?1, precio_extra = ?2, orden = ?3, activo = 1
                     WHERE id = ?4 AND grupo_id = ?5",
                    libsql::params![o.nombre.clone(), o.precio_extra, orden as i64, id, grupo_id],
                )
                .await
                .map_err(e500)?
                > 0,
            None => false,
        };
        if actualizada {
            conservadas.push(o.id.unwrap_or_default());
        } else {
            conn.execute(
                "INSERT INTO opciones_modificador (grupo_id, nombre, precio_extra, orden) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![grupo_id, o.nombre.clone(), o.precio_extra, orden as i64],
            )
            .await
            .map_err(e500)?;
            conservadas.push(conn.last_insert_rowid());
        }
    }
    let lista = conservadas.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    conn.execute(
        &format!(
            "UPDATE opciones_modificador SET activo = 0 WHERE grupo_id = ?1 AND id NOT IN ({})",
            if lista.is_empty() { "0".to_string() } else { lista }
        ),
        libsql::params![grupo_id],
    )
    .await
    .map_err(e500)?;

    // Productos que usan este grupo: se reemplaza la lista completa.
    conn.execute("DELETE FROM producto_grupos_modificador WHERE grupo_id = ?1", libsql::params![grupo_id])
        .await
        .map_err(e500)?;
    for producto_id in producto_ids {
        conn.execute(
            "INSERT OR IGNORE INTO producto_grupos_modificador (producto_id, grupo_id)
             SELECT id, ?2 FROM productos WHERE id = ?1",
            libsql::params![*producto_id, grupo_id],
        )
        .await
        .map_err(e500)?;
    }
    Ok(())
}

pub async fn crear_grupo_modificador(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GrupoPayload>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let (nombre, opciones) = validar_grupo(&payload)?;
    conn.execute(
        "INSERT INTO grupos_modificadores (nombre, obligatorio, multiple) VALUES (?1, ?2, ?3)",
        libsql::params![nombre, payload.obligatorio as i64, payload.multiple as i64],
    )
    .await
    .map_err(e500)?;
    let grupo_id = conn.last_insert_rowid();
    guardar_opciones_y_productos(&conn, grupo_id, &opciones, &payload.producto_ids).await?;
    Ok(Json(Respuesta { success: true, message: "Opciones guardadas".into() }))
}

pub async fn actualizar_grupo_modificador(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<GrupoPayload>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let (nombre, opciones) = validar_grupo(&payload)?;
    let filas = conn
        .execute(
            "UPDATE grupos_modificadores SET nombre = ?1, obligatorio = ?2, multiple = ?3 WHERE id = ?4 AND activo = 1",
            libsql::params![nombre, payload.obligatorio as i64, payload.multiple as i64, id],
        )
        .await
        .map_err(e500)?;
    if filas == 0 {
        return Err(error(StatusCode::NOT_FOUND, "Ese grupo de opciones no existe."));
    }
    guardar_opciones_y_productos(&conn, id, &opciones, &payload.producto_ids).await?;
    Ok(Json(Respuesta { success: true, message: "Opciones guardadas".into() }))
}

pub async fn desactivar_grupo_modificador(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    conn.execute("UPDATE grupos_modificadores SET activo = 0 WHERE id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    conn.execute("DELETE FROM producto_grupos_modificador WHERE grupo_id = ?1", libsql::params![id])
        .await
        .map_err(e500)?;
    Ok(Json(Respuesta { success: true, message: "Grupo eliminado".into() }))
}

// ============================================================
// Tipo de negocio y roles
// ============================================================

/// Activa o desactiva la atención en mesas. Al activarlo por primera vez
/// se crean 6 mesas de ejemplo. No se puede volver a "Tienda" con pedidos
/// abiertos (quedarían cuentas sin cobrar escondidas).
pub async fn cambiar_modo_negocio(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<ModoNegocioPayload>,
) -> Resultado<Respuesta> {
    exigir_admin(&claims)?;
    let modo = payload.modo_negocio.trim().to_uppercase();
    if modo != "TIENDA" && modo != "RESTAURANTE" {
        return Err(error(StatusCode::BAD_REQUEST, "Tipo de negocio no válido."));
    }
    let conn = tenant.0.connect().map_err(e500)?;

    if modo == "TIENDA" {
        let mut filas = conn
            .query("SELECT COUNT(*) FROM pedidos WHERE estado = 'ABIERTO'", ())
            .await
            .map_err(e500)?;
        let abiertos: i64 = match filas.next().await.map_err(e500)? {
            Some(f) => f.get(0).unwrap_or(0),
            None => 0,
        };
        if abiertos > 0 {
            return Err(error(
                StatusCode::CONFLICT,
                format!("Hay {} pedido(s) abierto(s). Cóbralos o anúlalos antes de desactivar la atención en mesas.", abiertos),
            ));
        }
    }

    conn.execute(
        "UPDATE configuracion_tienda SET modo_negocio = ?1, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![modo.clone()],
    )
    .await
    .map_err(e500)?;

    if modo == "RESTAURANTE" {
        sembrar_mesas_iniciales(&conn).await.map_err(e500)?;
    }

    let mensaje = if modo == "RESTAURANTE" {
        "Atención en mesas activada"
    } else {
        "Atención en mesas desactivada"
    };
    Ok(Json(Respuesta { success: true, message: mensaje.into() }))
}

/// Roles disponibles para crear usuarios (el Mesero solo aparece si el
/// negocio atiende en mesas).
pub async fn listar_roles(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
) -> Resultado<Vec<RolResumen>> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    let restaurante = modo_restaurante(&conn).await;
    let mut filas = conn
        .query("SELECT id, nombre FROM roles WHERE activo = 1 ORDER BY id", ())
        .await
        .map_err(e500)?;
    let mut roles = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let nombre: String = f.get(1).unwrap_or_default();
        if (nombre == "MESERO" || nombre == "PREPARACION") && !restaurante {
            continue;
        }
        roles.push(RolResumen { id: f.get(0).unwrap_or_default(), nombre });
    }
    Ok(Json(roles))
}

// ============================================================
// Preparación (barra / cocina) y entrega
// ============================================================

/// Lo que está en preparación o listo sin entregar, agrupado por pedido
/// (el que lleva más tiempo esperando va primero). Incluye pedidos ya
/// cobrados: en "para llevar" se suele pagar antes de preparar.
/// La usan la pantalla Preparación y los avisos al mozo/cajero.
async fn tickets_preparacion(conn: &libsql::Connection) -> Result<Vec<TicketPreparacion>, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT p.id, p.tipo, m.nombre, p.cliente_nombre, p.usuario_id, u.nombre_completo, p.estado,
                    i.id, i.nombre_producto, i.opciones, i.nota, CAST(i.cantidad AS REAL), i.fecha_listo,
                    CAST((julianday('now', 'localtime') - julianday(i.fecha_envio)) * 1440 AS INTEGER),
                    CAST((julianday('now', 'localtime') - julianday(COALESCE(i.fecha_listo, i.fecha_envio))) * 1440 AS INTEGER)
             FROM pedido_items i
             JOIN pedidos p ON p.id = i.pedido_id
             LEFT JOIN mesas m ON m.id = p.mesa_id
             LEFT JOIN usuarios u ON u.id = p.usuario_id
             WHERE i.estado = 'ENVIADO' AND i.fecha_entregado IS NULL AND p.estado != 'ANULADO'
               AND p.fecha_apertura >= datetime('now', 'localtime', '-1 day')
             ORDER BY i.fecha_envio, i.id",
            (),
        )
        .await
        .map_err(e500)?;

    let mut tickets: Vec<TicketPreparacion> = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let pedido_id: i64 = f.get(0).unwrap_or_default();
        let item = ItemPreparacion {
            id: f.get(7).unwrap_or_default(),
            nombre_producto: f.get(8).unwrap_or_default(),
            opciones: f.get(9).ok(),
            nota: f.get(10).ok(),
            cantidad: f.get(11).unwrap_or(0.0),
            listo: f.get::<String>(12).is_ok(),
            minutos_espera: f.get::<i64>(13).unwrap_or(0).max(0),
            minutos_listo: f.get::<i64>(14).unwrap_or(0).max(0),
        };
        match tickets.iter_mut().find(|t| t.pedido_id == pedido_id) {
            Some(t) => t.items.push(item),
            None => tickets.push(TicketPreparacion {
                pedido_id,
                tipo: f.get(1).unwrap_or_else(|_| "MESA".into()),
                mesa_nombre: f.get(2).ok(),
                cliente_nombre: f.get(3).ok(),
                usuario_id: f.get(4).unwrap_or_default(),
                mesero: f.get(5).ok(),
                cobrado: f.get::<String>(6).map(|e| e == "COBRADO").unwrap_or(false),
                items: vec![item],
            }),
        }
    }
    Ok(tickets)
}

pub async fn listar_preparacion(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<TicketPreparacion>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    Ok(Json(tickets_preparacion(&conn).await?))
}

fn validar_ids(ids: &[i64]) -> Result<(), (StatusCode, String)> {
    if ids.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "No se indicó ningún producto."));
    }
    if ids.len() > 200 {
        return Err(error(StatusCode::BAD_REQUEST, "Demasiados productos a la vez."));
    }
    Ok(())
}

/// Barra/cocina marca productos como LISTOS (o lo deshace si se equivocó).
/// El mozo no: él solo entrega.
pub async fn marcar_listo(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<MarcarListo>,
) -> Resultado<Vec<TicketPreparacion>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    validar_ids(&payload.item_ids)?;
    if nombre_rol(&conn, claims.rol_id).await.as_deref() == Some("MESERO") {
        return Err(error(StatusCode::FORBIDDEN, "Lo marca como listo quien lo prepara (barra o cocina)."));
    }

    let mut valores: Vec<libsql::Value> = payload.item_ids.iter().map(|id| (*id).into()).collect();
    let sql = if payload.listo {
        valores.push(claims.sub.into());
        format!(
            "UPDATE pedido_items SET fecha_listo = datetime('now', 'localtime'), listo_por = ?{}
             WHERE estado = 'ENVIADO' AND fecha_entregado IS NULL AND fecha_listo IS NULL AND id IN ({})",
            payload.item_ids.len() + 1,
            marcadores(1, payload.item_ids.len())
        )
    } else {
        format!(
            "UPDATE pedido_items SET fecha_listo = NULL, listo_por = NULL
             WHERE estado = 'ENVIADO' AND fecha_entregado IS NULL AND id IN ({})",
            marcadores(1, payload.item_ids.len())
        )
    };
    conn.execute(&sql, libsql::params_from_iter(valores)).await.map_err(e500)?;
    Ok(Json(tickets_preparacion(&conn).await?))
}

/// El mozo (o el cajero, en para llevar/delivery) entrega lo que ya está
/// listo. Solo pasa a ENTREGADO lo que barra/cocina marcó como listo.
pub async fn marcar_entregado(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<MarcarEntregado>,
) -> Resultado<Vec<TicketPreparacion>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    validar_ids(&payload.item_ids)?;
    conn.execute(
        &format!(
            "UPDATE pedido_items SET fecha_entregado = datetime('now', 'localtime')
             WHERE estado = 'ENVIADO' AND fecha_listo IS NOT NULL AND fecha_entregado IS NULL AND id IN ({})",
            marcadores(1, payload.item_ids.len())
        ),
        libsql::params_from_iter(payload.item_ids.clone()),
    )
    .await
    .map_err(e500)?;
    Ok(Json(tickets_preparacion(&conn).await?))
}
