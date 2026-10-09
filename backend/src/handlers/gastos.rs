//! Módulo Gastos: lo que el negocio paga y que no es mercadería (alquiler,
//! luz, sueldos, movilidad...). La mercadería va por Compras.
//!
//! - El administrador registra, consulta y anula gastos, y maneja las
//!   categorías.
//! - Un cajero solo puede registrar un gasto pagado con el efectivo de la
//!   caja abierta (desde la pantalla Caja).
//! - Un gasto con efectivo de caja crea su movimiento en movimientos_caja y
//!   suma a cajas.gastos_total: el efectivo esperado al cerrar ya lo descuenta.
//!   Si se anula con la caja todavía abierta, se devuelve al cuadre.
//! - Nada se borra: un error se anula con su motivo.

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::logica::tiempo;
use crate::middleware_auth::{exigir_admin, ROL_ADMIN};
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}

/// Error de una consulta: si es porque la tabla aún no existe, se explica.
fn sin_migracion<E: std::fmt::Display>(e: E) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") {
        (
            StatusCode::BAD_REQUEST,
            "Este negocio todavía no tiene el módulo de gastos (migración 0022). Reinicia el backend para aplicarla.".to_string(),
        )
    } else {
        e500(texto)
    }
}

pub const METODOS: [&str; 7] = ["EFECTIVO_CAJA", "EFECTIVO", "YAPE", "PLIN", "TRANSFERENCIA", "TARJETA", "OTRO"];
pub const COMPROBANTES: [&str; 5] = ["BOLETA", "FACTURA", "RECIBO", "TICKET", "OTRO"];

fn redondear_2(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

fn limpio(texto: &Option<String>) -> Option<String> {
    texto.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

// ---------------------------------------------------------------------
// Categorías
// ---------------------------------------------------------------------

#[derive(Serialize)]
pub struct CategoriaGasto {
    pub id: i64,
    pub nombre: String,
    pub activo: bool,
}

pub async fn listar_categorias(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<CategoriaGasto>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query("SELECT id, nombre, activo FROM categorias_gasto ORDER BY orden, nombre", ())
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(CategoriaGasto {
            id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            activo: f.get::<i64>(2).unwrap_or(1) == 1,
        });
    }
    Ok(Json(lista))
}

#[derive(Deserialize)]
pub struct NuevaCategoria {
    pub nombre: String,
}

pub async fn crear_categoria(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(datos): Json<NuevaCategoria>,
) -> Result<Json<CategoriaGasto>, Fallo> {
    exigir_admin(&claims)?;
    let nombre = datos.nombre.trim().to_string();
    if nombre.is_empty() || nombre.chars().count() > 60 {
        return Err(malo("Escribe un nombre de hasta 60 letras."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query("SELECT id FROM categorias_gasto WHERE lower(nombre) = lower(?1)", libsql::params![nombre.clone()])
        .await
        .map_err(sin_migracion)?;
    if filas.next().await.map_err(e500)?.is_some() {
        return Err(malo("Ya existe una categoría con ese nombre."));
    }
    drop(filas);
    conn.execute("INSERT INTO categorias_gasto (nombre) VALUES (?1)", libsql::params![nombre.clone()])
        .await
        .map_err(e500)?;
    Ok(Json(CategoriaGasto { id: conn.last_insert_rowid(), nombre, activo: true }))
}

#[derive(Deserialize)]
pub struct CambioCategoria {
    pub nombre: Option<String>,
    pub activo: Option<bool>,
}

pub async fn actualizar_categoria(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(datos): Json<CambioCategoria>,
) -> Result<Json<serde_json::Value>, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    if let Some(nombre) = datos.nombre.as_deref().map(str::trim) {
        if nombre.is_empty() || nombre.chars().count() > 60 {
            return Err(malo("Escribe un nombre de hasta 60 letras."));
        }
        let mut filas = conn
            .query(
                "SELECT id FROM categorias_gasto WHERE lower(nombre) = lower(?1) AND id <> ?2",
                libsql::params![nombre, id],
            )
            .await
            .map_err(sin_migracion)?;
        if filas.next().await.map_err(e500)?.is_some() {
            return Err(malo("Ya existe una categoría con ese nombre."));
        }
        drop(filas);
        conn.execute("UPDATE categorias_gasto SET nombre = ?1 WHERE id = ?2", libsql::params![nombre, id])
            .await
            .map_err(e500)?;
    }
    if let Some(activo) = datos.activo {
        conn.execute("UPDATE categorias_gasto SET activo = ?1 WHERE id = ?2", libsql::params![activo as i64, id])
            .await
            .map_err(sin_migracion)?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------
// Registrar
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct NuevoGasto {
    /// "AAAA-MM-DD". Sin ella, hoy. Con efectivo de caja siempre es hoy.
    pub fecha: Option<String>,
    pub categoria_id: i64,
    pub descripcion: String,
    pub monto: f64,
    pub metodo_pago: String,
    pub pagado_a: Option<String>,
    pub comprobante_tipo: Option<String>,
    pub comprobante_numero: Option<String>,
}

/// Lo que se guarda, ya validado.
#[derive(Debug, PartialEq)]
pub struct GastoValido {
    pub fecha: String,
    pub descripcion: String,
    pub monto: f64,
    pub metodo_pago: String,
    pub pagado_a: Option<String>,
    pub comprobante_tipo: Option<String>,
    pub comprobante_numero: Option<String>,
}

/// Valida un gasto nuevo. `es_admin`: un cajero solo registra gastos con
/// efectivo de caja. `hoy`: "AAAA-MM-DD" en Perú.
pub fn validar(datos: &NuevoGasto, es_admin: bool, hoy: &str) -> Result<GastoValido, String> {
    let metodo = datos.metodo_pago.trim().to_uppercase();
    if !METODOS.contains(&metodo.as_str()) {
        return Err("Forma de pago no válida.".to_string());
    }
    if !es_admin && metodo != "EFECTIVO_CAJA" {
        return Err("Solo el administrador registra gastos que no salen de la caja.".to_string());
    }
    let descripcion = datos.descripcion.trim().to_string();
    if descripcion.is_empty() {
        return Err("Escribe en qué se gastó (por ejemplo: \"Recibo de luz de octubre\").".to_string());
    }
    if descripcion.chars().count() > 200 {
        return Err("La descripción es muy larga (máximo 200 letras).".to_string());
    }
    if !datos.monto.is_finite() || datos.monto < 0.01 {
        return Err("El monto debe ser mayor a cero.".to_string());
    }
    if datos.monto > 1_000_000.0 {
        return Err("El monto es demasiado grande.".to_string());
    }
    // El efectivo de la caja sale hoy; lo demás puede ser de otro día
    // (un recibo que se pagó ayer), pero nunca futuro.
    let fecha = if metodo == "EFECTIVO_CAJA" {
        hoy.to_string()
    } else {
        match limpio(&datos.fecha) {
            None => hoy.to_string(),
            Some(f) if !tiempo::fecha_valida(&f) => return Err("Fecha no válida.".to_string()),
            Some(f) if f.as_str() > hoy => return Err("La fecha no puede ser futura.".to_string()),
            Some(f) => f,
        }
    };
    let comprobante_tipo = limpio(&datos.comprobante_tipo).map(|t| t.to_uppercase());
    if let Some(t) = &comprobante_tipo {
        if !COMPROBANTES.contains(&t.as_str()) {
            return Err("Tipo de comprobante no válido.".to_string());
        }
    }
    let comprobante_numero = if comprobante_tipo.is_some() { limpio(&datos.comprobante_numero) } else { None };
    Ok(GastoValido {
        fecha,
        descripcion,
        monto: redondear_2(datos.monto),
        metodo_pago: metodo,
        pagado_a: limpio(&datos.pagado_a),
        comprobante_tipo,
        comprobante_numero,
    })
}

#[derive(Serialize)]
pub struct GastoRegistrado {
    pub id: i64,
    pub fecha: String,
    pub monto: f64,
    pub caja_id: Option<i64>,
    pub mensaje: String,
}

pub async fn registrar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(datos): Json<NuevoGasto>,
) -> Result<Json<GastoRegistrado>, Fallo> {
    let es_admin = claims.rol_id == ROL_ADMIN;
    let gasto = validar(&datos, es_admin, &tiempo::hoy_lima()).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;

    // La categoría tiene que existir y estar activa.
    let mut filas = conn
        .query(
            "SELECT nombre FROM categorias_gasto WHERE id = ?1 AND activo = 1",
            libsql::params![datos.categoria_id],
        )
        .await
        .map_err(sin_migracion)?;
    let categoria: String = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or_default(),
        None => return Err(malo("Elige una categoría.")),
    };
    drop(filas);

    // Con efectivo de caja: la caja abierta, y su movimiento.
    let mut caja_id: Option<i64> = None;
    let mut movimiento_id: Option<i64> = None;
    if gasto.metodo_pago == "EFECTIVO_CAJA" {
        let mut filas = conn
            .query("SELECT id FROM cajas WHERE estado = 'ABIERTA' ORDER BY id DESC LIMIT 1", ())
            .await
            .map_err(e500)?;
        let id: i64 = match filas.next().await.map_err(e500)? {
            Some(f) => f.get(0).map_err(e500)?,
            None => return Err(malo("No hay una caja abierta. Abre la caja o elige otra forma de pago.")),
        };
        drop(filas);
        // No se puede sacar más efectivo del que hay en la caja.
        let disponible = crate::handlers::cajas::efectivo_en_caja(&conn, id).await?;
        crate::handlers::cajas::alcanza_efectivo(disponible, gasto.monto).map_err(malo)?;
        let motivo = format!("{}: {}", categoria, gasto.descripcion);
        conn.execute(
            "INSERT INTO movimientos_caja (caja_id, tipo, monto, motivo, usuario_id) VALUES (?1, 'GASTO', ?2, ?3, ?4)",
            libsql::params![id, gasto.monto, motivo, claims.sub],
        )
        .await
        .map_err(e500)?;
        movimiento_id = Some(conn.last_insert_rowid());
        conn.execute(
            "UPDATE cajas SET gastos_total = COALESCE(gastos_total, 0) + ?1 WHERE id = ?2",
            libsql::params![gasto.monto, id],
        )
        .await
        .map_err(e500)?;
        caja_id = Some(id);
    }

    let insertado = conn
        .execute(
            "INSERT INTO gastos (fecha, categoria_id, descripcion, monto, metodo_pago, pagado_a,
                                 comprobante_tipo, comprobante_numero, caja_id, movimiento_caja_id, usuario_id, registrado)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            libsql::params![
                gasto.fecha.clone(),
                datos.categoria_id,
                gasto.descripcion.clone(),
                gasto.monto,
                gasto.metodo_pago.clone(),
                gasto.pagado_a.clone(),
                gasto.comprobante_tipo.clone(),
                gasto.comprobante_numero.clone(),
                caja_id,
                movimiento_id,
                claims.sub,
                tiempo::ahora_lima()
            ],
        )
        .await;
    if let Err(e) = insertado {
        // Si no se pudo guardar el gasto, la caja vuelve a como estaba.
        if let (Some(caja), Some(mov)) = (caja_id, movimiento_id) {
            let _ = conn.execute("DELETE FROM movimientos_caja WHERE id = ?1", libsql::params![mov]).await;
            let _ = conn
                .execute(
                    "UPDATE cajas SET gastos_total = COALESCE(gastos_total, 0) - ?1 WHERE id = ?2",
                    libsql::params![gasto.monto, caja],
                )
                .await;
        }
        return Err(sin_migracion(e));
    }
    let id = conn.last_insert_rowid();

    let mensaje = if caja_id.is_some() {
        format!("Gasto de S/ {:.2} registrado y descontado del efectivo de la caja.", gasto.monto)
    } else {
        format!("Gasto de S/ {:.2} registrado.", gasto.monto)
    };
    Ok(Json(GastoRegistrado { id, fecha: gasto.fecha, monto: gasto.monto, caja_id, mensaje }))
}

// ---------------------------------------------------------------------
// Consultar
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FiltroGastos {
    /// "AAAA-MM-DD" (inclusive). Sin ellas, el mes actual.
    pub desde: Option<String>,
    pub hasta: Option<String>,
    pub categoria_id: Option<i64>,
    pub metodo_pago: Option<String>,
    /// Incluir los anulados en la lista (nunca en los totales).
    pub anulados: Option<bool>,
}

#[derive(Serialize)]
pub struct GastoFila {
    pub id: i64,
    pub fecha: String,
    pub categoria_id: i64,
    pub categoria: String,
    pub descripcion: String,
    pub monto: f64,
    pub metodo_pago: String,
    pub pagado_a: Option<String>,
    pub comprobante_tipo: Option<String>,
    pub comprobante_numero: Option<String>,
    pub caja_id: Option<i64>,
    pub usuario: String,
    pub registrado: String,
    pub anulado: bool,
    pub motivo_anulacion: Option<String>,
}

#[derive(Serialize)]
pub struct TotalGrupo {
    pub clave: String,
    pub total: f64,
    pub cantidad: i64,
}

#[derive(Serialize)]
pub struct ReporteGastos {
    pub desde: String,
    pub hasta: String,
    pub total: f64,
    pub cantidad: i64,
    pub por_categoria: Vec<TotalGrupo>,
    pub por_metodo: Vec<TotalGrupo>,
    pub gastos: Vec<GastoFila>,
}

/// Primer y último día del mes de `hoy`.
fn mes_de(hoy: &str) -> (String, String) {
    let inicio = format!("{}-01", &hoy[..7]);
    let fecha = chrono::NaiveDate::parse_from_str(&inicio, "%Y-%m-%d").unwrap_or_default();
    let siguiente = fecha.checked_add_months(chrono::Months::new(1)).unwrap_or(fecha);
    let fin = siguiente.pred_opt().unwrap_or(fecha).format("%Y-%m-%d").to_string();
    (inicio, fin)
}

pub async fn listar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(filtro): Query<FiltroGastos>,
) -> Result<Json<ReporteGastos>, Fallo> {
    exigir_admin(&claims)?;
    let (mes_inicio, mes_fin) = mes_de(&tiempo::hoy_lima());
    let desde = limpio(&filtro.desde).unwrap_or(mes_inicio);
    let hasta = limpio(&filtro.hasta).unwrap_or(mes_fin);
    if !tiempo::fecha_valida(&desde) || !tiempo::fecha_valida(&hasta) {
        return Err(malo("Fechas no válidas."));
    }
    if desde > hasta {
        return Err(malo("La fecha inicial es posterior a la final."));
    }
    let metodo = limpio(&filtro.metodo_pago).map(|m| m.to_uppercase());
    if let Some(m) = &metodo {
        if !METODOS.contains(&m.as_str()) {
            return Err(malo("Forma de pago no válida."));
        }
    }

    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query(
            "SELECT g.id, g.fecha, g.categoria_id, COALESCE(c.nombre, '—'), g.descripcion, CAST(g.monto AS REAL),
                    g.metodo_pago, g.pagado_a, g.comprobante_tipo, g.comprobante_numero, g.caja_id,
                    COALESCE(u.nombre_completo, ''), g.registrado, g.anulado, g.motivo_anulacion
             FROM gastos g
             LEFT JOIN categorias_gasto c ON c.id = g.categoria_id
             LEFT JOIN usuarios u ON u.id = g.usuario_id
             WHERE g.fecha BETWEEN ?1 AND ?2
               AND (?3 IS NULL OR g.categoria_id = ?3)
               AND (?4 IS NULL OR g.metodo_pago = ?4)
             ORDER BY g.fecha DESC, g.id DESC
             LIMIT 2000",
            libsql::params![desde.clone(), hasta.clone(), filtro.categoria_id, metodo.clone()],
        )
        .await
        .map_err(sin_migracion)?;

    let incluir_anulados = filtro.anulados.unwrap_or(false);
    let mut gastos = Vec::new();
    let mut total = 0.0;
    let mut cantidad = 0;
    let mut por_categoria: Vec<TotalGrupo> = Vec::new();
    let mut por_metodo: Vec<TotalGrupo> = Vec::new();
    let sumar = |grupos: &mut Vec<TotalGrupo>, clave: &str, monto: f64| match grupos.iter_mut().find(|g| g.clave == clave) {
        Some(g) => {
            g.total += monto;
            g.cantidad += 1;
        }
        None => grupos.push(TotalGrupo { clave: clave.to_string(), total: monto, cantidad: 1 }),
    };
    while let Some(f) = filas.next().await.map_err(e500)? {
        let fila = GastoFila {
            id: f.get(0).unwrap_or_default(),
            fecha: f.get(1).unwrap_or_default(),
            categoria_id: f.get(2).unwrap_or_default(),
            categoria: f.get(3).unwrap_or_default(),
            descripcion: f.get(4).unwrap_or_default(),
            monto: f.get(5).unwrap_or(0.0),
            metodo_pago: f.get(6).unwrap_or_default(),
            pagado_a: f.get(7).unwrap_or(None),
            comprobante_tipo: f.get(8).unwrap_or(None),
            comprobante_numero: f.get(9).unwrap_or(None),
            caja_id: f.get(10).unwrap_or(None),
            usuario: f.get(11).unwrap_or_default(),
            registrado: f.get(12).unwrap_or_default(),
            anulado: f.get::<i64>(13).unwrap_or(0) == 1,
            motivo_anulacion: f.get(14).unwrap_or(None),
        };
        if !fila.anulado {
            total += fila.monto;
            cantidad += 1;
            sumar(&mut por_categoria, &fila.categoria, fila.monto);
            sumar(&mut por_metodo, &fila.metodo_pago, fila.monto);
        }
        if !fila.anulado || incluir_anulados {
            gastos.push(fila);
        }
    }
    for g in por_categoria.iter_mut().chain(por_metodo.iter_mut()) {
        g.total = redondear_2(g.total);
    }
    por_categoria.sort_by(|a, b| b.total.total_cmp(&a.total));
    por_metodo.sort_by(|a, b| b.total.total_cmp(&a.total));

    Ok(Json(ReporteGastos { desde, hasta, total: redondear_2(total), cantidad, por_categoria, por_metodo, gastos }))
}

/// Total de gastos (no anulados) por mes "AAAA-MM", entre dos fechas
/// "AAAA-MM-DD" (inclusive). Lo usa el reporte de Ganancias. Si el negocio
/// aún no tiene la tabla, devuelve vacío.
pub async fn totales_por_mes(conn: &libsql::Connection, desde: &str, hasta: &str) -> Vec<(String, f64)> {
    let Ok(mut filas) = conn
        .query(
            "SELECT substr(fecha, 1, 7), CAST(COALESCE(SUM(monto), 0) AS REAL)
             FROM gastos WHERE anulado = 0 AND fecha BETWEEN ?1 AND ?2 GROUP BY 1",
            libsql::params![desde, hasta],
        )
        .await
    else {
        return Vec::new();
    };
    let mut lista = Vec::new();
    while let Ok(Some(f)) = filas.next().await {
        lista.push((f.get::<String>(0).unwrap_or_default(), f.get::<f64>(1).unwrap_or(0.0)));
    }
    lista
}

// ---------------------------------------------------------------------
// Anular
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Anulacion {
    pub motivo: String,
}

pub async fn anular(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(datos): Json<Anulacion>,
) -> Result<Json<serde_json::Value>, Fallo> {
    exigir_admin(&claims)?;
    let motivo = datos.motivo.trim().to_string();
    if motivo.is_empty() {
        return Err(malo("Escribe por qué se anula."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    let mut filas = conn
        .query(
            "SELECT g.anulado, CAST(g.monto AS REAL), g.caja_id, g.movimiento_caja_id, c.estado
             FROM gastos g LEFT JOIN cajas c ON c.id = g.caja_id WHERE g.id = ?1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let f = filas.next().await.map_err(e500)?.ok_or((StatusCode::NOT_FOUND, "Gasto no encontrado.".to_string()))?;
    let ya_anulado = f.get::<i64>(0).unwrap_or(0) == 1;
    let monto: f64 = f.get(1).unwrap_or(0.0);
    let caja_id: Option<i64> = f.get(2).unwrap_or(None);
    let movimiento_id: Option<i64> = f.get(3).unwrap_or(None);
    let estado_caja: Option<String> = f.get(4).unwrap_or(None);
    drop(filas);
    if ya_anulado {
        return Err(malo("Ese gasto ya estaba anulado."));
    }

    // Solo cambia si sigue sin anular (dos clics a la vez no lo hacen dos veces).
    let cambiados = conn
        .execute(
            "UPDATE gastos SET anulado = 1, anulado_por = ?1, motivo_anulacion = ?2, fecha_anulacion = ?3
             WHERE id = ?4 AND anulado = 0",
            libsql::params![claims.sub, motivo, tiempo::ahora_lima(), id],
        )
        .await
        .map_err(e500)?;
    if cambiados == 0 {
        return Err(malo("Ese gasto ya estaba anulado."));
    }

    let mensaje = match (caja_id, estado_caja.as_deref()) {
        (Some(caja), Some("ABIERTA")) => {
            if let Some(mov) = movimiento_id {
                conn.execute("DELETE FROM movimientos_caja WHERE id = ?1", libsql::params![mov]).await.map_err(e500)?;
            }
            conn.execute(
                "UPDATE cajas SET gastos_total = MAX(0, COALESCE(gastos_total, 0) - ?1) WHERE id = ?2",
                libsql::params![monto, caja],
            )
            .await
            .map_err(e500)?;
            "Gasto anulado. Como la caja sigue abierta, el monto vuelve al efectivo esperado."
        }
        (Some(_), _) => "Gasto anulado. Su caja ya se cerró: el cuadre de ese turno no cambia.",
        _ => "Gasto anulado.",
    };
    Ok(Json(serde_json::json!({ "ok": true, "mensaje": mensaje })))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn nuevo(metodo: &str, monto: f64, fecha: Option<&str>) -> NuevoGasto {
        NuevoGasto {
            fecha: fecha.map(str::to_string),
            categoria_id: 1,
            descripcion: "  Recibo de luz  ".into(),
            monto,
            metodo_pago: metodo.into(),
            pagado_a: Some(" ".into()),
            comprobante_tipo: None,
            comprobante_numero: Some("E001-1".into()),
        }
    }

    #[test]
    fn valida_lo_basico() {
        let hoy = "2026-10-09";
        let g = validar(&nuevo("yape", 45.555, Some("2026-10-01")), true, hoy).unwrap();
        assert_eq!(g.metodo_pago, "YAPE");
        assert_eq!(g.monto, 45.56);
        assert_eq!(g.fecha, "2026-10-01");
        assert_eq!(g.descripcion, "Recibo de luz");
        assert_eq!(g.pagado_a, None);
        // Sin tipo de comprobante no se guarda el número suelto.
        assert_eq!(g.comprobante_numero, None);

        assert!(validar(&nuevo("YAPE", 0.0, None), true, hoy).is_err());
        assert!(validar(&nuevo("CHEQUE", 10.0, None), true, hoy).is_err());
        assert!(validar(&nuevo("YAPE", 10.0, Some("2026-10-10")), true, hoy).unwrap_err().contains("futura"));
        assert!(validar(&nuevo("YAPE", 10.0, Some("10/10/2026")), true, hoy).is_err());
    }

    #[test]
    fn efectivo_de_caja_es_de_hoy_y_cajero_solo_ese() {
        let hoy = "2026-10-09";
        let g = validar(&nuevo("EFECTIVO_CAJA", 10.0, Some("2026-09-01")), false, hoy).unwrap();
        assert_eq!(g.fecha, hoy);
        assert!(validar(&nuevo("YAPE", 10.0, None), false, hoy).unwrap_err().contains("administrador"));
    }

    #[test]
    fn comprobante() {
        let hoy = "2026-10-09";
        let mut d = nuevo("TRANSFERENCIA", 100.0, None);
        d.comprobante_tipo = Some("factura".into());
        let g = validar(&d, true, hoy).unwrap();
        assert_eq!(g.comprobante_tipo.as_deref(), Some("FACTURA"));
        assert_eq!(g.comprobante_numero.as_deref(), Some("E001-1"));
        d.comprobante_tipo = Some("VALE".into());
        assert!(validar(&d, true, hoy).is_err());
    }

    #[test]
    fn mes_actual() {
        assert_eq!(mes_de("2026-10-09"), ("2026-10-01".to_string(), "2026-10-31".to_string()));
        assert_eq!(mes_de("2026-02-15"), ("2026-02-01".to_string(), "2026-02-28".to_string()));
        assert_eq!(mes_de("2026-12-31"), ("2026-12-01".to_string(), "2026-12-31".to_string()));
    }
}
