//! Cambio de prenda (módulo CAMBIOS, rubro Ropa y calzado).
//!
//! El cliente trae algo que compró y se lleva otra cosa en la misma
//! operación. Por dentro son dos movimientos de siempre, hechos juntos:
//!   1. una DEVOLUCIÓN de lo que trajo (vuelve al stock, salvo que tenga falla);
//!   2. una VENTA de lo que se lleva, por su precio completo.
//! Las dos van por el mismo medio de pago, así la caja solo se mueve por la
//! diferencia: devuelve S/ 50 y se lleva S/ 70 en efectivo = entran S/ 20.
//! Si lo nuevo vale menos, la diferencia sale de la caja hacia el cliente.
//! La tabla `cambios` (migración 0017) deja unidas las tres cosas.
//!
//! La venta la registra handlers/ventas.rs (procesar_venta con `cambio`);
//! aquí está lo propio del cambio: revisar lo que se devuelve, registrarlo y
//! el plazo de cambio del negocio.

use axum::{extract::{Extension, Path}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::creditos::METODO_CREDITO;
use crate::handlers::rubros::{exigir_modulo, MODULO_CAMBIOS};
use crate::logica::igv::round2;
use crate::logica::tiempo::{ahora_lima, hoy_lima};
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::models::devolucion::ComprobanteInfo;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

pub const NOMBRE_MODULO: &str = "Cambio de prenda";
/// Días para cambiar cuando el negocio no eligió otro plazo.
const DIAS_POR_DEFECTO: i64 = 7;
const DIAS_MAXIMO: i64 = 365;
/// Medios por los que se cobra (o se devuelve) la diferencia de un cambio.
const METODOS: &[&str] = &["EFECTIVO", "TARJETA", "TRANSFERENCIA", "YAPE_PLIN"];

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn malo(m: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, m.into())
}
/// La base aún no tiene la migración 0017.
fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "Aún no disponible: el sistema se está actualizando. Intenta en un minuto.".to_string())
}

// ---------------------------------------------------------------------------
// Lo que llega con la venta
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CambioItem {
    /// Línea de la venta original que el cliente devuelve.
    pub detalle_id: i64,
    pub cantidad: f64,
    /// true = la prenda tiene falla: no vuelve al stock.
    #[serde(default)]
    pub con_falla: bool,
}

#[derive(Debug, Deserialize)]
pub struct CambioVenta {
    /// Venta original (de donde sale lo que se devuelve).
    pub venta_id: i64,
    pub productos: Vec<CambioItem>,
    #[serde(default)]
    pub motivo: Option<String>,
    /// Emisión directa: el cajero pidió la nota de crédito de lo devuelto
    /// (si la venta original tiene boleta o factura aceptada).
    #[serde(default)]
    pub emitir_nota_credito: bool,
}

/// Línea revisada: (detalle_id, producto_id, cantidad, valor unitario, subtotal, con falla).
type Linea = (i64, i64, f64, f64, f64, bool);

/// Un cambio ya revisado, listo para registrarse cuando la venta nueva exista.
pub struct CambioListo {
    pub venta_id: i64,
    pub folio_original: String,
    lineas: Vec<Linea>,
    /// Lo que vale lo devuelto (lo que el cliente pagó por eso).
    pub valor: f64,
    motivo: String,
}

/// Lo que se le responde al punto de venta junto con la venta.
#[derive(Debug, Serialize)]
pub struct CambioHecho {
    pub folio_devolucion: String,
    pub folio_original: String,
    pub valor_devuelto: f64,
    pub total_nuevo: f64,
    /// Positivo: el cliente pagó esa diferencia. Negativo: se le devolvió.
    pub diferencia: f64,
    #[serde(skip)]
    pub devolucion_id: i64,
    /// La nota de crédito de lo devuelto, si el cajero la pidió.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nota_credito: Option<crate::handlers::notas_credito::NotaDeDevolucion>,
}

/// Una línea de la venta original con lo que aún se puede cambiar.
struct LineaOriginal {
    detalle_id: i64,
    producto_id: i64,
    nombre: String,
    cantidad: f64,
    devuelto: f64,
    /// Lo que el cliente pagó por cada unidad (ya con su descuento).
    valor_unitario: f64,
}

struct VentaOriginal {
    folio: String,
    fecha_hora: String,
    total: f64,
    al_credito: bool,
    /// Último día para cambiar ("2026-10-10"); None si el negocio no pone plazo.
    limite: Option<String>,
    lineas: Vec<LineaOriginal>,
}

/// Días de plazo: NULL = 7; 0 = sin plazo.
fn dias_de(valor: Option<i64>) -> i64 {
    valor.filter(|d| (0..=DIAS_MAXIMO).contains(d)).unwrap_or(DIAS_POR_DEFECTO)
}

async fn dias_cambio(conn: &libsql::Connection) -> i64 {
    let mut dias = None;
    if let Ok(mut filas) = conn.query("SELECT cambio_dias FROM configuracion_tienda LIMIT 1", ()).await {
        if let Ok(Some(f)) = filas.next().await {
            dias = f.get::<i64>(0).ok();
        }
    }
    dias_de(dias)
}

/// Lee la venta original con sus líneas y lo ya devuelto de cada una.
async fn venta_original(conn: &libsql::Connection, venta_id: i64) -> Result<VentaOriginal, (StatusCode, String)> {
    let mut filas = conn
        .query(
            "SELECT dv.id, dv.producto_id, COALESCE(dv.nombre_producto, p.nombre),
                    CAST(dv.cantidad AS REAL), CAST(dv.total_linea AS REAL),
                    CAST(COALESCE((SELECT SUM(dd.cantidad_devuelta) FROM detalles_devolucion dd
                                     JOIN devoluciones d ON d.id = dd.devolucion_id
                                    WHERE dd.detalle_venta_id = dv.id AND d.estado = 'PROCESADA'), 0) AS REAL),
                    v.folio, v.fecha_hora, CAST(v.total AS REAL), v.estado, v.pago_otro_metodo,
                    (SELECT cambio_dias FROM configuracion_tienda LIMIT 1),
                    date(v.fecha_hora, '-5 hours')
             FROM detalles_venta dv
             JOIN ventas v ON v.id = dv.venta_id
             JOIN productos p ON p.id = dv.producto_id
             WHERE dv.venta_id = ?1
             ORDER BY dv.id",
            libsql::params![venta_id],
        )
        .await
        .map_err(actualizando)?;

    let mut venta: Option<VentaOriginal> = None;
    while let Some(f) = filas.next().await.map_err(e500)? {
        if venta.is_none() {
            let estado: String = f.get(9).unwrap_or_default();
            if estado != "COMPLETADA" {
                return Err(malo("Esa venta está anulada: no se puede cambiar nada de ella."));
            }
            let dias = dias_de(f.get::<i64>(11).ok());
            let dia_venta: Option<String> = f.get::<String>(12).ok();
            let limite = match (dias, dia_venta) {
                (d, Some(dia)) if d > 0 => chrono::NaiveDate::parse_from_str(&dia, "%Y-%m-%d")
                    .ok()
                    .map(|fecha| (fecha + chrono::Duration::days(d)).format("%Y-%m-%d").to_string()),
                _ => None,
            };
            venta = Some(VentaOriginal {
                folio: f.get(6).unwrap_or_default(),
                fecha_hora: f.get(7).unwrap_or_default(),
                total: f.get(8).unwrap_or(0.0),
                al_credito: f.get::<String>(10).ok().as_deref() == Some(METODO_CREDITO),
                limite,
                lineas: Vec::new(),
            });
        }
        let cantidad: f64 = f.get(3).unwrap_or(0.0);
        let total_linea: f64 = f.get(4).unwrap_or(0.0);
        if let Some(v) = venta.as_mut() {
            v.lineas.push(LineaOriginal {
                detalle_id: f.get(0).unwrap_or_default(),
                producto_id: f.get(1).unwrap_or_default(),
                nombre: f.get(2).unwrap_or_default(),
                cantidad,
                devuelto: f.get(5).unwrap_or(0.0),
                valor_unitario: if cantidad > 0.0 { total_linea / cantidad } else { 0.0 },
            });
        }
    }
    venta.ok_or((StatusCode::NOT_FOUND, "No se encontró la venta original.".to_string()))
}

/// Revisa un cambio ANTES de registrar la venta nueva: el módulo, la venta
/// original y que cada cantidad devuelta todavía se pueda devolver.
pub async fn preparar(
    conn: &libsql::Connection,
    cambio: &CambioVenta,
    metodo_pago: &str,
) -> Result<CambioListo, (StatusCode, String)> {
    exigir_modulo(conn, MODULO_CAMBIOS, NOMBRE_MODULO).await?;
    if !METODOS.contains(&metodo_pago) {
        return Err(malo("En un cambio, la diferencia se cobra o se devuelve por un solo medio: efectivo, tarjeta, transferencia o Yape/Plin."));
    }
    if cambio.productos.is_empty() {
        return Err(malo("Indica qué prenda devuelve el cliente."));
    }
    let original = venta_original(conn, cambio.venta_id).await?;
    if original.al_credito {
        return Err(malo("Esa venta fue al crédito. Registra la devolución en Devoluciones (se descuenta de la deuda) y luego la venta nueva."));
    }

    let mut lineas: Vec<Linea> = Vec::with_capacity(cambio.productos.len());
    let mut valor = 0.0;
    for item in &cambio.productos {
        if !(item.cantidad > 0.0) || !item.cantidad.is_finite() {
            return Err(malo("La cantidad que se devuelve debe ser mayor a 0."));
        }
        if lineas.iter().any(|l| l.0 == item.detalle_id) {
            return Err(malo("Una prenda de la venta original está repetida en el cambio."));
        }
        let linea = original
            .lineas
            .iter()
            .find(|l| l.detalle_id == item.detalle_id)
            .ok_or_else(|| malo("Una de las prendas no es de esa venta."))?;
        let disponible = linea.cantidad - linea.devuelto;
        if item.cantidad > disponible + 1e-6 {
            return Err(malo(format!(
                "De \"{}\" solo se puede cambiar {} (ya se devolvió o cambió el resto).",
                linea.nombre,
                round2(disponible.max(0.0))
            )));
        }
        let subtotal = round2(linea.valor_unitario * item.cantidad);
        valor += subtotal;
        lineas.push((linea.detalle_id, linea.producto_id, item.cantidad, linea.valor_unitario, subtotal, item.con_falla));
    }

    let motivo = cambio.motivo.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(|m| m.chars().take(200).collect::<String>());
    Ok(CambioListo {
        venta_id: cambio.venta_id,
        folio_original: original.folio,
        lineas,
        valor: round2(valor),
        motivo: motivo.unwrap_or_else(|| "Cambio de prenda".to_string()),
    })
}

/// Registra la devolución de lo que el cliente trajo y deja unido el cambio
/// con la venta nueva. Se llama DENTRO de la transacción de la venta: si algo
/// falla aquí, tampoco queda la venta. La devolución va por el mismo medio que la venta
/// nueva: así la caja queda movida solo por la diferencia.
pub async fn registrar(
    conn: &libsql::Connection,
    listo: &CambioListo,
    venta_nueva_id: i64,
    folio_nuevo: &str,
    total_nuevo: f64,
    metodo_pago: &str,
    usuario_id: i64,
) -> Result<CambioHecho, (StatusCode, String)> {
    // El trigger de caja solo distingue EFECTIVO / TARJETA / TRANSFERENCIA.
    let metodo_reembolso = if metodo_pago == "YAPE_PLIN" { "TRANSFERENCIA" } else { metodo_pago };
    let fecha = crate::logica::tiempo::hoy_lima_compacto();
    let motivo = format!("{} · se llevó {}", listo.motivo, folio_nuevo);

    let (devolucion_id, folio_devolucion): (i64, String) = {
        let mut filas = conn
            .query(
                "INSERT INTO devoluciones (venta_original_id, folio_devolucion, monto_reembolsado, metodo_reembolso, motivo, usuario_id, estado)
                 VALUES (?2,
                         (SELECT 'DEV-' || ?1 || '-' || printf('%04d', COALESCE(MAX(CAST(substr(folio_devolucion, -4) AS INTEGER)), 0) + 1)
                            FROM devoluciones WHERE folio_devolucion LIKE 'DEV-' || ?1 || '%'),
                         ?3, ?4, ?5, ?6, 'PROCESADA')
                 RETURNING id, folio_devolucion",
                libsql::params![fecha, listo.venta_id, listo.valor, metodo_reembolso, motivo, usuario_id],
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al registrar la devolución del cambio: {}", e)))?;
        let datos: (i64, String) = match filas.next().await.map_err(e500)? {
            Some(f) => (f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default()),
            None => return Err((StatusCode::INTERNAL_SERVER_ERROR, "No se pudo registrar la devolución del cambio.".into())),
        };
        // Se lee la respuesta hasta el final antes de seguir (ver ventas.rs).
        while filas.next().await.map_err(e500)?.is_some() {}
        datos
    };

    // Lo devuelto en buen estado vuelve al stock (trigger de detalles_devolucion).
    const COLUMNAS: usize = 8;
    let mut marcas = Vec::with_capacity(listo.lineas.len());
    let mut valores: Vec<libsql::Value> = Vec::with_capacity(listo.lineas.len() * COLUMNAS);
    for (i, (detalle_id, producto_id, cantidad, valor_unitario, subtotal, con_falla)) in listo.lineas.iter().enumerate() {
        let b = i * COLUMNAS;
        marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
        valores.extend([
            libsql::Value::Integer(devolucion_id),
            libsql::Value::Integer(*producto_id),
            libsql::Value::Integer(*detalle_id),
            libsql::Value::Integer(listo.venta_id),
            libsql::Value::Real(*cantidad),
            libsql::Value::Real(*valor_unitario),
            libsql::Value::Real(*subtotal),
            libsql::Value::Text(if *con_falla { "DEFECTUOSO" } else { "REVENTA" }.to_string()),
        ]);
    }
    conn.execute(
        &format!(
            "INSERT INTO detalles_devolucion (devolucion_id, producto_id, detalle_venta_id, venta_id, cantidad_devuelta, precio_unitario, subtotal, condicion)
             VALUES {}",
            marcas.join(", ")
        ),
        libsql::params_from_iter(valores),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al registrar lo devuelto en el cambio: {}", e)))?;

    let diferencia = round2(total_nuevo - listo.valor);
    // Va en la misma transacción de la venta (ventas.rs): se guarda con ella.
    conn
        .execute(
            "INSERT INTO cambios (venta_original_id, venta_nueva_id, devolucion_id, valor_devuelto, total_nuevo, diferencia, metodo, usuario_id, fecha)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            libsql::params![listo.venta_id, venta_nueva_id, devolucion_id, listo.valor, total_nuevo, diferencia, metodo_pago, usuario_id, ahora_lima()],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al registrar el cambio: {}", e)))?;

    Ok(CambioHecho {
        folio_devolucion,
        folio_original: listo.folio_original.clone(),
        valor_devuelto: listo.valor,
        total_nuevo: round2(total_nuevo),
        diferencia,
        devolucion_id,
        nota_credito: None,
    })
}

// ---------------------------------------------------------------------------
// Pantalla "Cambio de prenda"
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LineaParaCambio {
    pub detalle_id: i64,
    pub producto_id: i64,
    pub nombre: String,
    pub cantidad: f64,
    /// Lo que aún se puede cambiar (vendido menos lo ya devuelto).
    pub disponible: f64,
    /// Lo que el cliente pagó por cada unidad, con su descuento.
    pub valor_unitario: f64,
}

#[derive(Debug, Serialize)]
pub struct VentaParaCambio {
    pub venta_id: i64,
    pub folio: String,
    pub fecha_hora: String,
    pub total: f64,
    pub comprobante: Option<ComprobanteInfo>,
    pub productos: Vec<LineaParaCambio>,
    /// Días de plazo del negocio (0 = sin plazo).
    pub plazo_dias: i64,
    /// Último día para cambiar, "AAAA-MM-DD" (None = sin plazo).
    pub limite: Option<String>,
    pub fuera_de_plazo: bool,
    /// Venta al crédito: no se cambia desde aquí.
    pub al_credito: bool,
    /// La venta tiene boleta o factura directa aceptada por SUNAT: el cajero
    /// puede emitir la nota de crédito de lo que el cliente devuelve.
    pub nota_credito_posible: bool,
}

/// GET /cambios/venta/:identificador — la venta (por folio o número de
/// boleta/factura) con lo que todavía se puede cambiar de cada prenda.
pub async fn venta_para_cambio(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(identificador): Path<String>,
) -> Resultado<VentaParaCambio> {
    {
        let conn = tenant.0.connect().map_err(e500)?;
        exigir_modulo(&conn, MODULO_CAMBIOS, NOMBRE_MODULO).await?;
    }
    // La misma búsqueda de Devoluciones: folio de venta o serie-número.
    let Json(base) = crate::handlers::devoluciones::buscar_venta_para_devolucion(Extension(tenant.clone()), Path(identificador)).await?;
    let conn = tenant.0.connect().map_err(e500)?;
    let original = venta_original(&conn, base.venta_id).await?;
    let fuera_de_plazo = original.limite.as_deref().map(|limite| hoy_lima().as_str() > limite).unwrap_or(false);
    Ok(Json(VentaParaCambio {
        venta_id: base.venta_id,
        folio: original.folio,
        fecha_hora: original.fecha_hora,
        total: original.total,
        comprobante: base.comprobante,
        productos: original
            .lineas
            .into_iter()
            .map(|l| LineaParaCambio {
                detalle_id: l.detalle_id,
                producto_id: l.producto_id,
                nombre: l.nombre,
                cantidad: l.cantidad,
                disponible: round2((l.cantidad - l.devuelto).max(0.0)),
                // Sin redondear: el punto de venta redondea cada línea igual que aquí.
                valor_unitario: l.valor_unitario,
            })
            .collect(),
        plazo_dias: dias_cambio(&conn).await,
        limite: original.limite,
        fuera_de_plazo,
        al_credito: original.al_credito,
        nota_credito_posible: crate::handlers::notas_credito::comprobante_directo_de_venta(&conn, base.venta_id)
            .await
            .is_some_and(|(_, _, estado)| estado == "ACEPTADO"),
    }))
}

#[derive(Debug, Serialize)]
pub struct ConfigCambios {
    /// Días que el cliente tiene para cambiar (0 = sin plazo).
    pub dias: i64,
}

#[derive(Debug, Deserialize)]
pub struct GuardarCambios {
    pub dias: i64,
}

/// GET /cambios/config — el plazo de cambio (se imprime en el ticket).
pub async fn obtener_config(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<ConfigCambios> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_CAMBIOS, NOMBRE_MODULO).await?;
    Ok(Json(ConfigCambios { dias: dias_cambio(&conn).await }))
}

/// PUT /configuracion/cambios — el administrador cambia el plazo.
pub async fn guardar_config(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarCambios>,
) -> Resultado<ConfigCambios> {
    exigir_admin(&claims)?;
    if !(0..=DIAS_MAXIMO).contains(&payload.dias) {
        return Err(malo(format!("El plazo de cambio va de 0 (sin plazo) a {} días.", DIAS_MAXIMO)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_CAMBIOS, NOMBRE_MODULO).await?;
    conn.execute("UPDATE configuracion_tienda SET cambio_dias = ?1", libsql::params![payload.dias])
        .await
        .map_err(actualizando)?;
    Ok(Json(ConfigCambios { dias: payload.dias }))
}
