//! Módulo "Ajustes y Kárdex" (AJUSTES_KARDEX).
//!
//! - Kárdex: todas las entradas y salidas de un producto (tabla
//!   movimientos_inventario), con el stock como quedó después de cada una.
//!   Las compras, devoluciones y mermas ya se anotaban; desde este módulo
//!   también las ventas, los lotes agregados a mano y los ajustes. Las ventas
//!   se anotan siempre (módulo encendido o no), para que al encenderlo el
//!   kárdex ya traiga la historia.
//! - Ajuste de inventario: después de contar, se pone el stock real de cada
//!   producto (o de cada lote, si lleva vencimiento) con su motivo, y queda
//!   anotado quién lo hizo.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::handlers::rubros::{negocio, MODULO_AJUSTES_KARDEX};
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

const CERO: f64 = 0.000_001;

async fn exigir(conn: &libsql::Connection, claims: &Claims) -> Result<(), Fallo> {
    exigir_admin(claims)?;
    if negocio(conn).await.modulos.iter().any(|m| m == MODULO_AJUSTES_KARDEX) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Este negocio no tiene encendido el módulo \"Ajustes y Kárdex\". Actívalo en Configuración → Módulos.".into(),
        ))
    }
}

/// Anota en el kárdex lo vendido (dentro de la transacción de la venta, ya
/// descontado el stock). Una fila por producto con lo vendido sumado. Si
/// falla no corta la venta: el kárdex es informativo.
pub async fn anotar_venta<'a>(
    conn: &libsql::Connection,
    venta_id: i64,
    folio: &str,
    usuario_id: i64,
    vendidos: impl Iterator<Item = &'a (i64, f64)>,
) {
    let mut suma: BTreeMap<i64, f64> = BTreeMap::new();
    for (id, cantidad) in vendidos {
        if cantidad.is_finite() && *cantidad > 0.0 {
            *suma.entry(*id).or_insert(0.0) += cantidad;
        }
    }
    if suma.is_empty() {
        return;
    }
    // Solo números (ids enteros y cantidades finitas) van en la sentencia.
    let casos: String = suma.iter().map(|(id, c)| format!(" WHEN {} THEN {}", id, c)).collect();
    let lista: String = suma.keys().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo, venta_id, usuario_id, referencia, motivo)
         SELECT id, 'VENTA', CASE id{c} END, stock + CASE id{c} END, stock, ?1, ?2, ?3, 'Venta'
         FROM productos WHERE id IN ({l})",
        c = casos,
        l = lista
    );
    if let Err(e) = conn.execute(&sql, libsql::params![venta_id, usuario_id, folio]).await {
        eprintln!("kárdex: no se anotó la venta {}: {}", venta_id, e);
    }
}

/// Anota un movimiento suelto (lote agregado a mano, edición del stock...).
/// Tampoco corta lo que se estaba haciendo si falla.
#[allow(clippy::too_many_arguments)]
pub async fn anotar(
    conn: &libsql::Connection,
    producto_id: i64,
    tipo: &str,
    cantidad: f64,
    stock_anterior: f64,
    stock_nuevo: f64,
    usuario_id: Option<i64>,
    referencia: Option<String>,
    motivo: &str,
) {
    if (stock_nuevo - stock_anterior).abs() < CERO && cantidad.abs() < CERO {
        return;
    }
    if let Err(e) = conn
        .execute(
            "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo, usuario_id, referencia, motivo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            libsql::params![producto_id, tipo, cantidad, stock_anterior, stock_nuevo, usuario_id, referencia, motivo],
        )
        .await
    {
        eprintln!("kárdex: no se anotó el movimiento de {}: {}", producto_id, e);
    }
}

/// Stock actual de un producto (0 si no existe).
pub async fn stock_de(conn: &libsql::Connection, producto_id: i64) -> f64 {
    match conn.query("SELECT CAST(stock AS REAL) FROM productos WHERE id = ?1", libsql::params![producto_id]).await {
        Ok(mut f) => {
            let v = match f.next().await {
                Ok(Some(r)) => r.get::<f64>(0).unwrap_or(0.0),
                _ => 0.0,
            };
            while let Ok(Some(_)) = f.next().await {}
            v
        }
        Err(_) => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Kárdex de un producto
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Rango {
    pub desde: Option<String>,
    pub hasta: Option<String>,
}

#[derive(Serialize)]
pub struct Movimiento {
    pub id: i64,
    pub fecha_hora: String,
    pub tipo: String,
    pub entrada: f64,
    pub salida: f64,
    pub stock_anterior: f64,
    pub saldo: f64,
    pub referencia: Option<String>,
    pub motivo: Option<String>,
    pub usuario: Option<String>,
}

#[derive(Serialize)]
pub struct Kardex {
    pub producto_id: i64,
    pub nombre: String,
    pub codigo: Option<String>,
    pub unidad_medida: String,
    pub stock_actual: f64,
    /// Del más nuevo al más antiguo.
    pub movimientos: Vec<Movimiento>,
}

fn fecha_valida(f: &Option<String>) -> Option<String> {
    f.as_deref()
        .map(str::trim)
        .filter(|f| f.len() == 10 && f.chars().all(|c| c.is_ascii_digit() || c == '-'))
        .map(String::from)
}

/// Qué tipos suman al stock (los AJUSTE llevan su signo en la cantidad).
fn es_entrada(tipo: &str) -> bool {
    matches!(tipo, "ENTRADA" | "DEVOLUCION")
}

/// GET /kardex/:producto_id?desde=AAAA-MM-DD&hasta=AAAA-MM-DD
pub async fn ver(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(producto_id): Path<i64>,
    Query(rango): Query<Rango>,
) -> Result<Json<Kardex>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn, &claims).await?;

    let (nombre, codigo, unidad_medida, stock_actual) = {
        let mut f = conn
            .query(
                "SELECT nombre, codigo, unidad_medida, CAST(stock AS REAL) FROM productos WHERE id = ?1",
                libsql::params![producto_id],
            )
            .await
            .map_err(e500)?;
        let r = f
            .next()
            .await
            .map_err(e500)?
            .ok_or((StatusCode::NOT_FOUND, "Ese producto no existe.".to_string()))?;
        let v = (
            r.get::<String>(0).unwrap_or_default(),
            r.get::<Option<String>>(1).unwrap_or(None),
            r.get::<String>(2).unwrap_or_default(),
            r.get::<f64>(3).unwrap_or(0.0),
        );
        while f.next().await.map_err(e500)?.is_some() {}
        v
    };

    let desde = fecha_valida(&rango.desde).unwrap_or_else(|| "0000-00-00".into());
    let hasta = fecha_valida(&rango.hasta).unwrap_or_else(|| "9999-12-31".into());
    let mut f = conn
        .query(
            "SELECT m.id, m.fecha_hora, m.tipo_movimiento, CAST(m.cantidad AS REAL), CAST(m.stock_anterior AS REAL),
                    CAST(m.stock_nuevo AS REAL), m.referencia, m.motivo, u.nombre_completo,
                    (SELECT folio FROM ventas WHERE id = m.venta_id), m.compra_id
             FROM movimientos_inventario m LEFT JOIN usuarios u ON u.id = m.usuario_id
             WHERE m.producto_id = ?1 AND substr(m.fecha_hora, 1, 10) BETWEEN ?2 AND ?3
             ORDER BY m.fecha_hora DESC, m.id DESC LIMIT 2000",
            libsql::params![producto_id, desde, hasta],
        )
        .await
        .map_err(e500)?;
    let mut movimientos = Vec::new();
    while let Some(r) = f.next().await.map_err(e500)? {
        let tipo = r.get::<String>(2).unwrap_or_default();
        let cantidad = r.get::<f64>(3).unwrap_or(0.0);
        let anterior = r.get::<f64>(4).unwrap_or(0.0);
        let saldo = r.get::<f64>(5).unwrap_or(0.0);
        // Entrada o salida: por el tipo; un ajuste, por su signo.
        let (entrada, salida) = if tipo == "AJUSTE" {
            if cantidad >= 0.0 { (cantidad, 0.0) } else { (0.0, -cantidad) }
        } else if es_entrada(&tipo) {
            (cantidad.abs(), 0.0)
        } else {
            (0.0, cantidad.abs())
        };
        let folio = r.get::<Option<String>>(9).unwrap_or(None);
        let compra = r.get::<Option<i64>>(10).unwrap_or(None);
        let referencia = folio
            .or(r.get::<Option<String>>(6).unwrap_or(None))
            .or(compra.map(|c| format!("Compra #{}", c)));
        movimientos.push(Movimiento {
            id: r.get(0).unwrap_or(0),
            fecha_hora: r.get::<String>(1).unwrap_or_default(),
            tipo,
            entrada,
            salida,
            stock_anterior: anterior,
            saldo,
            referencia,
            motivo: r.get::<Option<String>>(7).unwrap_or(None),
            usuario: r.get::<Option<String>>(8).unwrap_or(None),
        });
    }
    Ok(Json(Kardex { producto_id, nombre, codigo, unidad_medida, stock_actual, movimientos }))
}

// ---------------------------------------------------------------------------
// Ajuste de inventario
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LineaAjuste {
    pub producto_id: i64,
    /// Productos con vencimiento: el lote contado (por id o por su número).
    pub lote_id: Option<i64>,
    pub numero_lote: Option<String>,
    pub stock_contado: f64,
    pub motivo: Option<String>,
}

#[derive(Deserialize)]
pub struct NuevoAjuste {
    pub lineas: Vec<LineaAjuste>,
    /// Motivo general (si la línea no trae el suyo).
    pub motivo: Option<String>,
}

#[derive(Serialize)]
pub struct Ajustado {
    pub producto_id: i64,
    pub nombre: String,
    pub numero_lote: Option<String>,
    pub antes: f64,
    pub despues: f64,
    pub diferencia: f64,
}

#[derive(Serialize)]
pub struct ResultadoAjuste {
    pub ajustados: Vec<Ajustado>,
    /// Líneas cuyo conteo ya coincidía con el sistema.
    pub sin_cambio: usize,
}

fn motivo_limpio(m: Option<&str>) -> String {
    let m = m.map(str::trim).filter(|m| !m.is_empty()).unwrap_or("Conteo físico");
    m.chars().take(120).collect()
}

/// Redondeo a 3 decimales (kilos, metros) para no anotar diferencias de 0.0000001.
fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// POST /inventario/ajustes — todo o nada (una transacción).
pub async fn ajustar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(n): Json<NuevoAjuste>,
) -> Result<Json<ResultadoAjuste>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn, &claims).await?;
    if n.lineas.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No hay nada que ajustar.".into()));
    }
    if n.lineas.len() > 5000 {
        return Err((StatusCode::BAD_REQUEST, "Son demasiadas líneas: ajústalas en partes de hasta 5000.".into()));
    }
    for l in &n.lineas {
        if !l.stock_contado.is_finite() || l.stock_contado < 0.0 {
            return Err((StatusCode::BAD_REQUEST, "El stock contado no puede ser negativo.".into()));
        }
    }

    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await
        .map_err(e500)?;
    let resultado: Result<ResultadoAjuste, Fallo> = async {
        let conn: &libsql::Connection = &tx;
        let mut ajustados = Vec::new();
        let mut sin_cambio = 0;
        for l in &n.lineas {
            let motivo = motivo_limpio(l.motivo.as_deref().or(n.motivo.as_deref()));
            let (nombre, lleva_vencimiento, controla_stock, stock) = {
                let mut f = conn
                    .query(
                        "SELECT nombre, COALESCE(lleva_vencimiento, 0), COALESCE(controla_stock, 1), CAST(stock AS REAL)
                         FROM productos WHERE id = ?1",
                        libsql::params![l.producto_id],
                    )
                    .await
                    .map_err(e500)?;
                let r = f
                    .next()
                    .await
                    .map_err(e500)?
                    .ok_or((StatusCode::BAD_REQUEST, format!("El producto {} ya no existe.", l.producto_id)))?;
                let v = (
                    r.get::<String>(0).unwrap_or_default(),
                    r.get::<i64>(1).unwrap_or(0) == 1,
                    r.get::<i64>(2).unwrap_or(1) == 1,
                    r.get::<f64>(3).unwrap_or(0.0),
                );
                while f.next().await.map_err(e500)?.is_some() {}
                v
            };
            if !controla_stock {
                return Err((StatusCode::BAD_REQUEST, format!("\"{}\" no lleva stock: no se puede ajustar.", nombre)));
            }
            let contado = r3(l.stock_contado);

            if lleva_vencimiento {
                // Por lote: el stock del producto lo recalcula el trigger.
                let numero = l.numero_lote.as_deref().map(|x| x.trim().to_uppercase()).filter(|x| !x.is_empty());
                let (sql, valor): (&str, libsql::Value) = match (l.lote_id, &numero) {
                    (Some(id), _) => (
                        "SELECT id, CAST(cantidad AS REAL), numero_lote FROM lotes_producto WHERE id = ?2 AND producto_id = ?1",
                        id.into(),
                    ),
                    (None, Some(num)) => (
                        "SELECT id, CAST(cantidad AS REAL), numero_lote FROM lotes_producto
                         WHERE producto_id = ?1 AND UPPER(TRIM(numero_lote)) = ?2 ORDER BY activo DESC, id DESC LIMIT 1",
                        num.clone().into(),
                    ),
                    (None, None) => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            format!("\"{}\" lleva vencimiento: indica qué lote contaste.", nombre),
                        ))
                    }
                };
                let (lote_id, antes, numero_lote) = {
                    let mut f = conn.query(sql, libsql::params![l.producto_id, valor]).await.map_err(e500)?;
                    let r = f.next().await.map_err(e500)?.ok_or((
                        StatusCode::BAD_REQUEST,
                        format!(
                            "\"{}\": no existe el lote {}. Un lote nuevo se agrega en Productos (con su vencimiento).",
                            nombre,
                            numero.clone().unwrap_or_default()
                        ),
                    ))?;
                    let v = (r.get::<i64>(0).unwrap_or(0), r.get::<f64>(1).unwrap_or(0.0), r.get::<Option<String>>(2).unwrap_or(None));
                    while f.next().await.map_err(e500)?.is_some() {}
                    v
                };
                let diferencia = r3(contado - antes);
                if diferencia.abs() < CERO {
                    sin_cambio += 1;
                    continue;
                }
                conn.execute(
                    "UPDATE lotes_producto SET cantidad = ?1, activo = CASE WHEN ?1 > 0 THEN 1 ELSE 0 END WHERE id = ?2",
                    libsql::params![contado, lote_id],
                )
                .await
                .map_err(e500)?;
                let despues = stock_de(conn, l.producto_id).await;
                let referencia = Some(match &numero_lote {
                    Some(nl) => format!("Lote {}", nl),
                    None => format!("Lote #{}", lote_id),
                });
                conn.execute(
                    "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo, usuario_id, referencia, motivo)
                     VALUES (?1, 'AJUSTE', ?2, ?3, ?4, ?5, ?6, ?7)",
                    libsql::params![l.producto_id, diferencia, stock, despues, claims.sub, referencia, motivo.clone()],
                )
                .await
                .map_err(e500)?;
                ajustados.push(Ajustado { producto_id: l.producto_id, nombre, numero_lote, antes, despues: contado, diferencia });
            } else {
                let diferencia = r3(contado - stock);
                if diferencia.abs() < CERO {
                    sin_cambio += 1;
                    continue;
                }
                conn.execute(
                    "UPDATE productos SET stock = ?1, fecha_actualizacion = datetime('now','localtime') WHERE id = ?2",
                    libsql::params![contado, l.producto_id],
                )
                .await
                .map_err(e500)?;
                conn.execute(
                    "INSERT INTO movimientos_inventario (producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo, usuario_id, motivo)
                     VALUES (?1, 'AJUSTE', ?2, ?3, ?4, ?5, ?6)",
                    libsql::params![l.producto_id, diferencia, stock, contado, claims.sub, motivo.clone()],
                )
                .await
                .map_err(e500)?;
                ajustados.push(Ajustado { producto_id: l.producto_id, nombre, numero_lote: None, antes: stock, despues: contado, diferencia });
            }
        }
        Ok(ResultadoAjuste { ajustados, sin_cambio })
    }
    .await;

    match resultado {
        Ok(r) => {
            tx.commit().await.map_err(e500)?;
            Ok(Json(r))
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}
