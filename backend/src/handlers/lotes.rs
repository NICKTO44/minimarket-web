use axum::{extract::{Extension, Path, Query}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;

use crate::handlers::ganancias;
use crate::logica::tiempo;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;
use crate::models::lote::{VentaDeLote, VentasDeLote, DevueltoVencido, Lote, NuevoLote, LoteAlerta, LoteAccionResponse, MermaLote, ReporteMermas};
use crate::middleware_auth::exigir_admin;

pub async fn agregar_lote(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Json(payload): Json<NuevoLote>,
) -> Result<Json<Lote>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Reporte de ganancias encendido: el lote es mercadería que entra al
    // precio de compra del producto. Apagado, nada de esto se ejecuta.
    let donde_ganancias = ganancias::Donde::Producto(payload.producto_id);
    let antes_ganancias = if ganancias::activo(&conn).await {
        Some(ganancias::foto(&conn, &donde_ganancias).await)
    } else {
        None
    };

    let numero_lote = payload
        .numero_lote
        .as_deref()
        .map(|n| n.trim().to_uppercase())
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(40).collect::<String>());
    conn.execute(
        "INSERT INTO lotes_producto (producto_id, cantidad, fecha_vencimiento, numero_lote) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![payload.producto_id, payload.cantidad, payload.fecha_vencimiento.clone(), numero_lote.clone()],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = conn.last_insert_rowid();

    if let Some(antes) = &antes_ganancias {
        ganancias::conciliar(&conn, antes, &donde_ganancias, "LOTE", Some(id), false).await;
    }

    let vencido = payload.fecha_vencimiento.get(..10).map(|f| f < tiempo::hoy_lima().as_str()).unwrap_or(false);
    Ok(Json(Lote {
        id,
        vencido,
        producto_id: payload.producto_id,
        cantidad: payload.cantidad,
        fecha_vencimiento: payload.fecha_vencimiento,
        numero_lote,
        activo: true,
    }))
}

pub async fn obtener_lotes_de_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(producto_id): Path<i64>,
) -> Result<Json<Vec<Lote>>, StatusCode> {
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

      let mut rows = conn
        .query(
            "SELECT id, producto_id, cantidad, fecha_vencimiento, numero_lote, activo,
                    CASE WHEN date(fecha_vencimiento) < ?2 THEN 1 ELSE 0 END
             FROM lotes_producto WHERE producto_id = ?1 AND activo = 1 AND cantidad > 0
             ORDER BY fecha_vencimiento ASC",
            libsql::params![producto_id, tiempo::hoy_lima()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut lotes = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        lotes.push(Lote {
            id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
            cantidad: row.get(2).unwrap_or_default(),
            fecha_vencimiento: row.get(3).unwrap_or_default(),
            numero_lote: row.get(4).ok(),
            activo: row.get::<i64>(5).unwrap_or(1) == 1,
            vencido: row.get::<i64>(6).unwrap_or(0) == 1,
        });
    }

    Ok(Json(lotes))
}

#[derive(Deserialize)]
pub struct HorizonteQuery {
    pub dias: Option<i64>,
}

// Trae todos los lotes de TODOS los productos que vencen dentro del
// horizonte indicado (o ya vencidos, con dias_restantes negativo) —
// misma lógica que Lubricentro, pero a nivel de todo el negocio, no
// de un solo producto. Por defecto, 15 días de horizonte.
pub async fn lotes_por_vencer(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Query(params): Query<HorizonteQuery>,
) -> Result<Json<Vec<LoteAlerta>>, StatusCode> {
    let dias_horizonte = params.dias.unwrap_or(15);
    let conn = tenant.0.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = conn
        .query(
            "SELECT l.id, l.producto_id, p.nombre, l.cantidad, p.unidad_medida, l.fecha_vencimiento,
                    CAST(julianday(date(l.fecha_vencimiento)) - julianday(?2) AS INTEGER)
             FROM lotes_producto l
             JOIN productos p ON p.id = l.producto_id
             WHERE l.activo = 1 AND l.cantidad > 0
               AND julianday(date(l.fecha_vencimiento)) - julianday(?2) <= ?1
             ORDER BY l.fecha_vencimiento ASC",
            libsql::params![dias_horizonte, tiempo::hoy_lima()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut alertas = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        alertas.push(LoteAlerta {
            lote_id: row.get(0).unwrap_or_default(),
            producto_id: row.get(1).unwrap_or_default(),
            producto_nombre: row.get(2).unwrap_or_default(),
            cantidad: row.get(3).unwrap_or_default(),
            unidad_medida: row.get(4).unwrap_or_default(),
            fecha_vencimiento: row.get(5).unwrap_or_default(),
            dias_restantes: row.get(6).unwrap_or_default(),
        });
    }

    Ok(Json(alertas))
}

/// Descartar un lote (merma): sale del stock y queda registrado para el
/// reporte de productos vencidos y la auditoría. Todo en una transacción:
///   - el lote queda en cantidad 0 e inactivo (el trigger recalcula el stock);
///   - una fila en mermas_lote con la foto del lote (migración 0032);
///   - un movimiento 'MERMA' en movimientos_inventario.
/// Si la base todavía no tiene la 0032, se descarta igual y deja el
/// movimiento; solo falta la fila del reporte.
pub async fn descartar_lote(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(lote_id): Path<i64>,
) -> Result<Json<LoteAccionResponse>, (StatusCode, String)> {
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al descartar: {}", e));
    let conn = tenant.0.connect().map_err(e500)?;

    // Foto del lote y su producto. El costo promedio llegó con la 0018:
    // sin esa columna se usa solo el precio de compra.
    const SQL_FOTO: &str = "SELECT l.producto_id, CAST(l.cantidad AS REAL), l.fecha_vencimiento, l.numero_lote,
                p.nombre, p.unidad_medida, CAST(p.stock AS REAL),
                CASE WHEN date(l.fecha_vencimiento) < ?2 THEN 'VENCIDO' ELSE 'POR_VENCER' END,
                COSTO
         FROM lotes_producto l JOIN productos p ON p.id = l.producto_id
         WHERE l.id = ?1 AND l.activo = 1";
    let hoy = tiempo::hoy_lima();
    let con_promedio = SQL_FOTO.replace(
        "COSTO",
        "CASE WHEN COALESCE(p.costo_promedio, 0) > 0 THEN CAST(p.costo_promedio AS REAL)
              WHEN COALESCE(p.precio_compra, 0) > 0 THEN CAST(p.precio_compra AS REAL) END",
    );
    let solo_precio = SQL_FOTO.replace(
        "COSTO",
        "CASE WHEN COALESCE(p.precio_compra, 0) > 0 THEN CAST(p.precio_compra AS REAL) END",
    );
    let mut filas = match conn.query(&con_promedio, libsql::params![lote_id, hoy.clone()]).await {
        Ok(f) => f,
        Err(_) => conn.query(&solo_precio, libsql::params![lote_id, hoy.clone()]).await.map_err(e500)?,
    };
    let Some(f) = filas.next().await.map_err(e500)? else {
        return Err((StatusCode::NOT_FOUND, "Ese lote ya fue descartado o no existe.".into()));
    };
    let producto_id: i64 = f.get(0).unwrap_or_default();
    let cantidad: f64 = f.get(1).unwrap_or(0.0);
    let fecha_vencimiento: String = f.get(2).unwrap_or_default();
    let numero_lote: Option<String> = f.get(3).ok();
    let nombre: String = f.get(4).unwrap_or_default();
    let unidad: String = f.get(5).unwrap_or_else(|_| "UNIDAD".into());
    let stock_antes: f64 = f.get(6).unwrap_or(0.0);
    let motivo: String = f.get(7).unwrap_or_else(|_| "VENCIDO".into());
    let costo: Option<f64> = f.get(8).ok();
    while filas.next().await.map_err(e500)?.is_some() {}

    let hay_reporte = match conn
        .query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'mermas_lote'", ())
        .await
    {
        Ok(mut r) => matches!(r.next().await, Ok(Some(_))),
        Err(_) => false,
    };
    let ahora = tiempo::ahora_lima();
    let texto_motivo = if motivo == "VENCIDO" { "Lote vencido descartado" } else { "Lote descartado antes de vencer" };
    let referencia = numero_lote.clone().map(|n| format!("Lote {}", n)).unwrap_or_else(|| format!("Lote #{}", lote_id));

    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(e500)?;
    let resultado: Result<(), libsql::Error> = async {
        let cambiados = tx
            .execute("UPDATE lotes_producto SET activo = 0, cantidad = 0 WHERE id = ?1 AND activo = 1", libsql::params![lote_id])
            .await?;
        if cambiados == 0 {
            return Ok(());
        }
        if hay_reporte {
            tx.execute(
                "INSERT INTO mermas_lote (lote_id, producto_id, producto_nombre, unidad_medida, numero_lote, fecha_vencimiento,
                    cantidad, costo_unitario, motivo, usuario_id, usuario_nombre, fecha_hora)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                libsql::params![
                    lote_id, producto_id, nombre.clone(), unidad.clone(), numero_lote.clone(), fecha_vencimiento.clone(),
                    cantidad, costo, motivo.clone(), claims.sub, claims.nombre_completo.clone(), ahora.clone()
                ],
            )
            .await?;
        }
        // stock_nuevo: lo que dejó el trigger de lotes. La hora la pone la
        // tabla (como en sus demás movimientos, que vienen de triggers).
        tx.execute(
            "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo,
                usuario_id, referencia, motivo)
             VALUES (?1, 'MERMA', ?2, ?3, (SELECT stock FROM productos WHERE id = ?1), ?4, ?5, ?6)",
            libsql::params![producto_id, cantidad, stock_antes, claims.sub, referencia.clone(), texto_motivo],
        )
        .await?;
        Ok(())
    }
    .await;
    match resultado {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e500(e));
        }
    }

    Ok(Json(LoteAccionResponse {
        success: true,
        message: format!("Lote descartado: {} {} de \"{}\" quedan en el reporte de vencidos.", cantidad, unidad, nombre),
    }))
}

#[derive(Deserialize)]
pub struct FiltroMermas {
    pub desde: Option<String>,
    pub hasta: Option<String>,
}

/// GET /lotes/mermas?desde=AAAA-MM-DD&hasta=AAAA-MM-DD — reporte de
/// productos vencidos, para revisar o auditar el inventario. Solo admin.
/// Sin fechas: el mes en curso (hora de Lima).
pub async fn reporte_mermas(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(filtro): Query<FiltroMermas>,
) -> Result<Json<ReporteMermas>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let malo = |m: &str| (StatusCode::BAD_REQUEST, m.to_string());
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let hoy = tiempo::hoy_lima();
    let limpio = |v: &Option<String>| v.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let desde = limpio(&filtro.desde).unwrap_or_else(|| format!("{}-01", &hoy[..7]));
    let hasta = limpio(&filtro.hasta).unwrap_or_else(|| hoy.clone());
    if !tiempo::fecha_valida(&desde) || !tiempo::fecha_valida(&hasta) {
        return Err(malo("Fecha no válida."));
    }
    if desde > hasta {
        return Err(malo("La fecha inicial es posterior a la final."));
    }
    let conn = tenant.0.connect().map_err(e500)?;

    // Descartados en el rango (sin la 0032 la tabla no existe: lista vacía).
    let mut descartados = Vec::new();
    let mut disponible = true;
    match conn
        .query(
            "SELECT id, fecha_hora, producto_id, producto_nombre, unidad_medida, numero_lote, fecha_vencimiento,
                    CAST(cantidad AS REAL), CAST(costo_unitario AS REAL), motivo, usuario_nombre
             FROM mermas_lote
             WHERE substr(fecha_hora, 1, 10) BETWEEN ?1 AND ?2
             ORDER BY fecha_hora DESC, id DESC",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
    {
        Ok(mut filas) => {
            while let Some(f) = filas.next().await.map_err(e500)? {
                let cantidad: f64 = f.get(7).unwrap_or(0.0);
                let costo_unitario: Option<f64> = f.get(8).ok();
                descartados.push(MermaLote {
                    id: f.get(0).unwrap_or_default(),
                    fecha_hora: f.get(1).unwrap_or_default(),
                    producto_id: f.get(2).unwrap_or_default(),
                    producto_nombre: f.get(3).unwrap_or_default(),
                    unidad_medida: f.get(4).unwrap_or_default(),
                    numero_lote: f.get(5).ok(),
                    fecha_vencimiento: f.get(6).unwrap_or_default(),
                    cantidad,
                    costo_unitario,
                    costo_total: costo_unitario.map(|c| (c * cantidad * 100.0).round() / 100.0),
                    motivo: f.get(9).unwrap_or_default(),
                    usuario_nombre: f.get(10).ok(),
                });
            }
        }
        Err(_) => disponible = false,
    }
    let total_costo = (descartados.iter().filter_map(|m| m.costo_total).sum::<f64>() * 100.0).round() / 100.0;
    let sin_costo = descartados.iter().filter(|m| m.costo_total.is_none()).count() as i64;

    // Lo que hoy está vencido y sigue en el inventario (no se vende).
    let mut vencidos_sin_descartar = Vec::new();
    let mut filas = conn
        .query(
            "SELECT l.id, l.producto_id, p.nombre, CAST(l.cantidad AS REAL), p.unidad_medida, l.fecha_vencimiento,
                    CAST(julianday(date(l.fecha_vencimiento)) - julianday(?1) AS INTEGER)
             FROM lotes_producto l JOIN productos p ON p.id = l.producto_id
             WHERE l.activo = 1 AND l.cantidad > 0 AND date(l.fecha_vencimiento) < ?1
             ORDER BY l.fecha_vencimiento ASC",
            libsql::params![hoy],
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        vencidos_sin_descartar.push(LoteAlerta {
            lote_id: f.get(0).unwrap_or_default(),
            producto_id: f.get(1).unwrap_or_default(),
            producto_nombre: f.get(2).unwrap_or_default(),
            cantidad: f.get(3).unwrap_or_default(),
            unidad_medida: f.get(4).unwrap_or_default(),
            fecha_vencimiento: f.get(5).unwrap_or_default(),
            dias_restantes: f.get(6).unwrap_or_default(),
        });
    }

    // Devueltos por clientes como vencidos (devoluciones guarda la hora en
    // UTC). Costo: el que quedó en la venta, si no el del producto hoy. Las
    // columnas de costo llegaron con la 0018: sin ellas, el precio de compra.
    const SQL_DEVUELTOS: &str = "SELECT strftime('%Y-%m-%d %H:%M:%S', d.fecha_hora, '-5 hours'), d.folio_devolucion,
                p.id, p.nombre, COALESCE(p.unidad_medida, 'UNIDAD'),
                CAST(dd.cantidad_devuelta AS REAL), CAST(dd.subtotal AS REAL), COSTO
         FROM detalles_devolucion dd
         JOIN devoluciones d ON d.id = dd.devolucion_id
         JOIN productos p ON p.id = dd.producto_id
         LEFT JOIN detalles_venta dv ON dv.id = dd.detalle_venta_id
         WHERE dd.condicion = 'VENCIDO' AND d.estado = 'PROCESADA'
           AND date(d.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2
         ORDER BY d.fecha_hora DESC, dd.id DESC";
    let con_costo = SQL_DEVUELTOS.replace(
        "COSTO",
        "CAST(COALESCE(dv.costo_unitario, CASE WHEN p.costo_promedio > 0 THEN p.costo_promedio
                                         WHEN p.precio_compra > 0 THEN p.precio_compra END) AS REAL)",
    );
    let solo_precio = SQL_DEVUELTOS.replace("COSTO", "CAST(CASE WHEN p.precio_compra > 0 THEN p.precio_compra END AS REAL)");
    let mut filas = match conn.query(&con_costo, libsql::params![desde.clone(), hasta.clone()]).await {
        Ok(f) => f,
        Err(_) => conn.query(&solo_precio, libsql::params![desde.clone(), hasta.clone()]).await.map_err(e500)?,
    };
    let mut devueltos_vencidos = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let cantidad: f64 = f.get(5).unwrap_or(0.0);
        let costo: Option<f64> = f.get::<f64>(7).ok().filter(|c| *c > 0.0);
        devueltos_vencidos.push(DevueltoVencido {
            fecha_hora: f.get(0).unwrap_or_default(),
            folio_devolucion: f.get(1).unwrap_or_default(),
            producto_id: f.get(2).unwrap_or_default(),
            producto_nombre: f.get(3).unwrap_or_default(),
            unidad_medida: f.get(4).unwrap_or_default(),
            cantidad,
            monto: f.get(6).unwrap_or(0.0),
            costo_total: costo.map(|c| (c * cantidad * 100.0).round() / 100.0),
        });
    }
    let total_costo_devueltos =
        (devueltos_vencidos.iter().filter_map(|d| d.costo_total).sum::<f64>() * 100.0).round() / 100.0;

    Ok(Json(ReporteMermas {
        desde,
        hasta,
        disponible,
        descartados,
        total_costo,
        sin_costo,
        vencidos_sin_descartar,
        devueltos_vencidos,
        total_costo_devueltos,
    }))
}

/// GET /lotes/:id/ventas — a quién se vendió un lote (por ejemplo, ante una
/// alerta de retiro). Solo admin: muestra datos de clientes. Las ventas
/// anteriores a la migración 0033 no quedaron anotadas.
pub async fn ventas_de_lote(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(lote_id): Path<i64>,
) -> Result<Json<VentasDeLote>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let conn = tenant.0.connect().map_err(e500)?;

    let mut filas = conn
        .query(
            "SELECT p.nombre, l.numero_lote, l.fecha_vencimiento
             FROM lotes_producto l JOIN productos p ON p.id = l.producto_id WHERE l.id = ?1",
            libsql::params![lote_id],
        )
        .await
        .map_err(e500)?;
    let Some(f) = filas.next().await.map_err(e500)? else {
        return Err((StatusCode::NOT_FOUND, "Lote no encontrado.".into()));
    };
    let producto_nombre: String = f.get(0).unwrap_or_default();
    let numero_lote: Option<String> = f.get(1).ok();
    let fecha_vencimiento: String = f.get(2).unwrap_or_default();
    while filas.next().await.map_err(e500)?.is_some() {}

    let mut ventas = Vec::new();
    let mut disponible = true;
    match conn
        .query(
            "SELECT v.id, v.folio, strftime('%Y-%m-%d %H:%M:%S', v.fecha_hora, '-5 hours'), COALESCE(v.estado, 'COMPLETADA'),
                    CAST(SUM(vl.cantidad) AS REAL), c.nombre_razon_social, c.numero_documento, c.telefono
             FROM venta_lotes vl
             JOIN ventas v ON v.id = vl.venta_id
             LEFT JOIN clientes c ON c.id = v.cliente_id
             WHERE vl.lote_id = ?1
             GROUP BY v.id
             ORDER BY v.fecha_hora DESC, v.id DESC",
            libsql::params![lote_id],
        )
        .await
    {
        Ok(mut filas) => {
            while let Some(f) = filas.next().await.map_err(e500)? {
                ventas.push(VentaDeLote {
                    venta_id: f.get(0).unwrap_or_default(),
                    folio: f.get(1).unwrap_or_default(),
                    fecha_hora: f.get(2).unwrap_or_default(),
                    estado: f.get(3).unwrap_or_default(),
                    cantidad: f.get(4).unwrap_or(0.0),
                    cliente_nombre: f.get::<String>(5).ok().filter(|t| !t.trim().is_empty()),
                    cliente_documento: f.get::<String>(6).ok().filter(|t| !t.trim().is_empty()),
                    cliente_telefono: f.get::<String>(7).ok().filter(|t| !t.trim().is_empty()),
                });
            }
        }
        Err(_) => disponible = false,
    }
    let total_vendido = ventas.iter().filter(|v| v.estado == "COMPLETADA").map(|v| v.cantidad).sum();

    Ok(Json(VentasDeLote { lote_id, producto_nombre, numero_lote, fecha_vencimiento, disponible, ventas, total_vendido }))
}
