//! IGV de una venta leído de la base: la tasa del negocio, las líneas con
//! su afectación y el desglose. Lo usan la emisión electrónica, la boleta
//! pública y la reimpresión. El cálculo en sí está en logica/igv.rs.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::logica::igv::{afectacion_valida, desglosar, tasa_valida, Afectacion, Desglose};
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

/// Tasa de IGV que el negocio configuró (18 si no configuró nada válido).
pub async fn tasa_negocio(conn: &libsql::Connection) -> f64 {
    let leida = match conn.query("SELECT CAST(iva_porcentaje AS REAL) FROM configuracion_tienda LIMIT 1", ()).await {
        Ok(mut filas) => match filas.next().await {
            Ok(Some(f)) => f.get::<f64>(0).ok(),
            _ => None,
        },
        Err(_) => None,
    };
    tasa_valida(leida)
}

/// Una línea de una venta, con lo necesario para el comprobante.
#[derive(Debug, Clone)]
pub struct LineaVenta {
    pub descripcion: String,
    pub cantidad: f64,
    /// Precio unitario realmente cobrado (total de la línea / cantidad).
    pub precio_unitario: f64,
    pub unidad_medida: String,
    /// Total de la línea con IGV incluido y descuentos aplicados.
    pub total: f64,
    pub afectacion: Afectacion,
}

/// Líneas y desglose de IGV de una venta.
///
/// Usa lo congelado al cobrar (ventas.igv_tasa, detalles_venta.afectacion_igv).
/// Para ventas anteriores a la migración 0012 (o si la base aún no la tiene)
/// cae en la configuración actual: producto → categoría → gravado, y la tasa
/// del negocio.
pub async fn desglose_venta(conn: &libsql::Connection, venta_id: i64) -> Result<(Vec<LineaVenta>, Desglose), String> {
    // Total cobrado y tasa congelada.
    let (total, tasa_congelada): (f64, Option<f64>) = {
        let con_tasa = conn
            .query("SELECT CAST(total AS REAL), CAST(igv_tasa AS REAL) FROM ventas WHERE id = ?1", libsql::params![venta_id])
            .await;
        let (mut filas, hay_tasa) = match con_tasa {
            Ok(filas) => (filas, true),
            Err(_) => (
                conn.query("SELECT CAST(total AS REAL) FROM ventas WHERE id = ?1", libsql::params![venta_id])
                    .await
                    .map_err(|e| e.to_string())?,
                false,
            ),
        };
        match filas.next().await.map_err(|e| e.to_string())? {
            Some(f) => (f.get::<f64>(0).unwrap_or(0.0), if hay_tasa { f.get::<f64>(1).ok() } else { None }),
            None => return Err("Venta no encontrada".to_string()),
        }
    };
    let tasa = match tasa_congelada {
        Some(t) => tasa_valida(Some(t)),
        None => tasa_negocio(conn).await,
    };

    const BASE: &str = "SELECT COALESCE(dv.nombre_producto, p.nombre), CAST(dv.cantidad AS REAL),
                               CAST(dv.precio_unitario AS REAL), COALESCE(dv.unidad_medida, p.unidad_medida),
                               CAST(dv.total_linea AS REAL)";
    const DESDE: &str = " FROM detalles_venta dv JOIN productos p ON p.id = dv.producto_id";
    const FILTRO: &str = " WHERE dv.venta_id = ?1 ORDER BY dv.id";
    let con_afectacion = format!(
        "{}, COALESCE(dv.afectacion_igv, p.afectacion_igv, c.afectacion_igv){} LEFT JOIN categorias c ON c.id = p.categoria_id{}",
        BASE, DESDE, FILTRO
    );
    let (mut filas, hay_afectacion) = match conn.query(&con_afectacion, libsql::params![venta_id]).await {
        Ok(filas) => (filas, true),
        Err(_) => (
            conn.query(&format!("{}{}{}", BASE, DESDE, FILTRO), libsql::params![venta_id])
                .await
                .map_err(|e| e.to_string())?,
            false,
        ),
    };

    let mut lineas = Vec::new();
    while let Some(f) = filas.next().await.map_err(|e| e.to_string())? {
        let cantidad: f64 = f.get(1).unwrap_or(0.0);
        let precio_lista: f64 = f.get(2).unwrap_or(0.0);
        let total_linea: f64 = f.get::<f64>(4).unwrap_or(cantidad * precio_lista);
        lineas.push(LineaVenta {
            descripcion: f.get(0).unwrap_or_default(),
            cantidad,
            // Sin descuento se manda el precio de lista tal cual (evita
            // decimales de más al dividir); con descuento, el realmente cobrado.
            precio_unitario: if cantidad <= 0.0 || (total_linea - cantidad * precio_lista).abs() < 0.0051 {
                precio_lista
            } else {
                total_linea / cantidad
            },
            unidad_medida: f.get(3).unwrap_or_default(),
            total: total_linea,
            afectacion: if hay_afectacion { Afectacion::desde(f.get::<String>(5).ok().as_deref()) } else { Afectacion::Gravado },
        });
    }

    let partes: Vec<(f64, Afectacion)> = lineas.iter().map(|l| (l.total, l.afectacion)).collect();
    Ok((lineas, desglosar(total, &partes, tasa)))
}

/// Deja congelado en la venta cómo se cobró: la tasa del negocio y la
/// afectación de cada línea. Se llama después de registrar la venta y NO
/// puede hacerla fallar: si la base aún no tiene la migración 0012 no pasa
/// nada (desglose_venta usará la configuración actual).
pub async fn congelar_en_venta(conn: &libsql::Connection, venta_id: i64) {
    congelar_con_tasa(conn, venta_id, tasa_negocio(conn).await).await;
}

/// Igual que `congelar_en_venta`, cuando quien llama ya leyó la tasa del
/// negocio (la venta la trae en su primera consulta: un viaje menos a la base).
pub async fn congelar_con_tasa(conn: &libsql::Connection, venta_id: i64, tasa: f64) {
    let _ = conn
        .execute_batch(&format!(
            "UPDATE ventas SET igv_tasa = {tasa} WHERE id = {id};
             UPDATE detalles_venta SET afectacion_igv = COALESCE(
                 (SELECT COALESCE(p.afectacion_igv, c.afectacion_igv)
                    FROM productos p LEFT JOIN categorias c ON c.id = p.categoria_id
                   WHERE p.id = detalles_venta.producto_id),
                 'GRAVADO')
              WHERE venta_id = {id};",
            tasa = tasa,
            id = venta_id
        ))
        .await;
}

/// Desglose de IGV de una venta, para imprimir o reimprimir su ticket.
#[derive(Debug, Serialize)]
pub struct DesgloseRespuesta {
    pub igv_tasa: f64,
    pub op_gravadas: f64,
    pub op_exoneradas: f64,
    pub op_inafectas: f64,
    pub igv: f64,
    pub total: f64,
    /// Detracción con la que se emitió la factura de esta venta (si la tuvo).
    pub detraccion_porcentaje: Option<f64>,
    pub detraccion_monto: Option<f64>,
    pub detraccion_cuenta: Option<String>,
}

/// Detracción guardada en el comprobante de una venta. None si no tuvo, o
/// si la base aún no tiene la migración 0013.
async fn detraccion_de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<(f64, f64, String)> {
    let mut filas = conn
        .query(
            "SELECT CAST(detraccion_porcentaje AS REAL), CAST(detraccion_monto AS REAL), COALESCE(detraccion_cuenta, '')
             FROM comprobantes_electronicos
             WHERE venta_id = ?1 AND detraccion_monto IS NOT NULL ORDER BY id DESC LIMIT 1",
            libsql::params![venta_id],
        )
        .await
        .ok()?;
    let f = filas.next().await.ok()??;
    Some((f.get::<f64>(0).ok()?, f.get::<f64>(1).ok()?, f.get::<String>(2).unwrap_or_default()))
}

/// GET /igv/venta/:id
pub async fn obtener_desglose(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(venta_id): Path<i64>,
) -> Result<Json<DesgloseRespuesta>, (StatusCode, String)> {
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (_, d) = desglose_venta(&conn, venta_id).await.map_err(|e| {
        if e == "Venta no encontrada" { (StatusCode::NOT_FOUND, e) } else { (StatusCode::INTERNAL_SERVER_ERROR, e) }
    })?;
    let detraccion = detraccion_de_venta(&conn, venta_id).await;
    Ok(Json(DesgloseRespuesta {
        detraccion_porcentaje: detraccion.as_ref().map(|x| x.0),
        detraccion_monto: detraccion.as_ref().map(|x| x.1),
        detraccion_cuenta: detraccion.as_ref().map(|x| x.2.clone()),
        igv_tasa: d.tasa,
        op_gravadas: d.gravadas,
        op_exoneradas: d.exoneradas,
        op_inafectas: d.inafectas,
        igv: d.igv,
        total: d.total,
    }))
}

// ============================================================
// IGV por categoría
// ============================================================

#[derive(Debug, Deserialize)]
pub struct CambiarIgvCategoria {
    pub afectacion_igv: String,
}

#[derive(Debug, Serialize)]
pub struct CategoriaIgv {
    pub id: i64,
    pub afectacion_igv: String,
    /// Productos activos de la categoría que heredan este valor.
    pub productos: i64,
}

/// PUT /categorias/:id/igv — el administrador marca la categoría como
/// GRAVADO, EXONERADO o INAFECTO. Los productos que no tengan un valor
/// propio lo heredan (también los que se creen después).
pub async fn cambiar_igv_categoria(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<CambiarIgvCategoria>,
) -> Result<Json<CategoriaIgv>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let valor = payload.afectacion_igv.trim().to_uppercase();
    if !afectacion_valida(&valor) {
        return Err((StatusCode::BAD_REQUEST, "Valor de IGV no válido.".to_string()));
    }
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cambiadas = conn
        .execute("UPDATE categorias SET afectacion_igv = ?1 WHERE id = ?2", libsql::params![valor.clone(), id])
        .await
        .map_err(|_| (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string()))?;
    if cambiadas == 0 {
        return Err((StatusCode::NOT_FOUND, "Categoría no encontrada.".to_string()));
    }
    let mut filas = conn
        .query(
            "SELECT COUNT(*) FROM productos WHERE categoria_id = ?1 AND activo = 1 AND afectacion_igv IS NULL",
            libsql::params![id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let productos = match filas.next().await {
        Ok(Some(f)) => f.get::<i64>(0).unwrap_or(0),
        _ => 0,
    };
    Ok(Json(CategoriaIgv { id, afectacion_igv: valor, productos }))
}

/// PUT /productos/:id/igv — excepción por producto. 'HEREDAR' quita la
/// excepción (vuelve a usar el IGV de su categoría).
pub async fn cambiar_igv_producto(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<CambiarIgvCategoria>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    exigir_admin(&claims)?;
    let valor = payload.afectacion_igv.trim().to_uppercase();
    let guardar: Option<String> = if valor == "HEREDAR" || valor.is_empty() {
        None
    } else if afectacion_valida(&valor) {
        Some(valor)
    } else {
        return Err((StatusCode::BAD_REQUEST, "Valor de IGV no válido.".to_string()));
    };
    let conn = tenant.0.connect().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cambiados = conn
        .execute("UPDATE productos SET afectacion_igv = ?1 WHERE id = ?2", libsql::params![guardar.clone(), id])
        .await
        .map_err(|_| (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string()))?;
    if cambiados == 0 {
        return Err((StatusCode::NOT_FOUND, "Producto no encontrado.".to_string()));
    }
    Ok(Json(serde_json::json!({ "id": id, "afectacion_propia": guardar })))
}
