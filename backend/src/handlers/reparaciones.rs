//! Módulo REPARACIONES (migración 0048), solo para el rubro Tecnología /
//! Reparaciones: celulares, laptops, impresoras...
//!
//! Cada orden de reparación es un ENCARGO por dentro (handlers/encargos.rs):
//! cliente, fecha prometida, presupuesto (repuestos del stock y mano de obra
//! escrita en la orden), adelantos sin comprobante y, al entregar, cobro del
//! saldo en el punto de venta con la boleta completa. La tabla reparaciones
//! agrega lo del equipo y del taller.
//!
//! Flujo:
//!   1. RECIBIDO: el cliente deja el equipo. Se anota tipo, marca, modelo,
//!      N.° de serie o IMEI, lo que trae (cargador, funda), cómo llega, la
//!      falla que cuenta y, si la da, la clave de desbloqueo. Sale el ticket
//!      de recepción. Si el equipo vuelve en garantía, se avisa.
//!   2. DIAGNOSTICO: el técnico revisa y anota lo que encontró.
//!   3. ESPERANDO_APROBACION: se arma el presupuesto (repuestos + mano de
//!      obra) y se le manda al cliente por WhatsApp.
//!   4. EN_REPARACION: el cliente aprobó.
//!   5. LISTO (o NO_REPARADO): se le avisa por WhatsApp.
//!   6. Se entrega cobrando en el punto de venta (o, si no se cobra nada,
//!      "devolver sin cobro"). La garantía corre desde la entrega.
//!   La clave de desbloqueo se borra al entregar o cancelar.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET  /reparaciones?estado=&buscar=      POST /reparaciones
//!   GET  /reparaciones/:id                  PUT  /reparaciones/:id
//!   POST /reparaciones/:id/etapa            POST /reparaciones/:id/devolver
//!   GET  /reparaciones/garantia?serie=      GET  /reparaciones/tecnicos
//!   Adelantos, cancelar y foto: las rutas de /encargos/:id/...

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::encargos::{self, DatosEncargo, Encargo};
use crate::handlers::rubros::{negocio, MODULO_REPARACIONES, RUBRO_TECNOLOGIA};
use crate::logica::tiempo;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Órdenes de reparación";
pub const TIPOS_EQUIPO: &[&str] = &["CELULAR", "LAPTOP", "PC", "TABLET", "IMPRESORA", "CONSOLA", "OTRO"];
pub const ETAPAS: &[&str] = &["RECIBIDO", "DIAGNOSTICO", "ESPERANDO_APROBACION", "EN_REPARACION", "LISTO", "NO_REPARADO"];
const MAX_GARANTIA: i64 = 730;

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion(e: impl std::fmt::Display) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") || texto.contains("no such column") {
        (StatusCode::SERVICE_UNAVAILABLE, "Las órdenes de reparación aún no están listas en este negocio. Intenta en unos minutos.".to_string())
    } else {
        e500(texto)
    }
}
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, Fallo> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(malo(format!("{}: máximo {} letras.", campo, max))),
        otro => Ok(otro),
    }
}
fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// N.° de serie o IMEI sin espacios ni guiones y en mayúsculas, para
/// encontrar el mismo equipo aunque se escriba distinto.
pub fn serie_normal(serie: &str) -> String {
    serie.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_uppercase()).collect()
}

/// Estado del encargo que corresponde a cada etapa del taller.
pub fn estado_de_etapa(etapa: &str) -> &'static str {
    match etapa {
        "EN_REPARACION" => "EN_PREPARACION",
        "LISTO" | "NO_REPARADO" => "LISTO",
        _ => "PENDIENTE",
    }
}

/// Hasta qué día corre la garantía (fecha de entrega + días).
pub fn garantia_hasta(fecha_entregado: Option<&str>, dias: i64) -> Option<String> {
    if dias <= 0 {
        return None;
    }
    let dia = NaiveDate::parse_from_str(fecha_entregado?.get(..10)?, "%Y-%m-%d").ok()?;
    Some((dia + Duration::days(dias)).format("%Y-%m-%d").to_string())
}

/// Solo el rubro Tecnología con el módulo encendido.
async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    let n = negocio(conn).await;
    if n.rubro != RUBRO_TECNOLOGIA {
        return Err((StatusCode::FORBIDDEN, format!("\"{}\" es solo para el rubro Tecnología / Reparaciones.", NOMBRE_MODULO)));
    }
    if !n.modulos.iter().any(|m| m == MODULO_REPARACIONES) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ));
    }
    Ok(())
}

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

// ---------------------------------------------------------------------
// Lectura
// ---------------------------------------------------------------------

#[derive(Debug, Serialize, Clone, Default)]
pub struct DatosEquipo {
    pub tipo_equipo: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub serie: Option<String>,
    pub color: Option<String>,
    pub accesorios: Option<String>,
    pub clave: Option<String>,
    pub estado_fisico: Option<String>,
    pub falla: String,
}

#[derive(Debug, Serialize)]
pub struct Reparacion {
    #[serde(flatten)]
    pub orden: Encargo,
    #[serde(flatten)]
    pub equipo: DatosEquipo,
    pub etapa: String,
    pub diagnostico: Option<String>,
    pub tecnico: Option<String>,
    pub garantia_dias: i64,
    /// Entregada con garantía: hasta qué día (AAAA-MM-DD).
    pub garantia_hasta: Option<String>,
    pub en_garantia: bool,
    pub aprobado_en: Option<String>,
}

/// Una fila del listado (sin ítems ni pagos).
#[derive(Debug, Serialize)]
pub struct FilaReparacion {
    pub id: i64,
    pub numero: i64,
    pub cliente_nombre: String,
    pub cliente_telefono: Option<String>,
    pub fecha_entrega: String,
    pub estado: String,
    pub etapa: String,
    pub tipo_equipo: String,
    pub marca: Option<String>,
    pub modelo: Option<String>,
    pub serie: Option<String>,
    pub falla: String,
    pub tecnico: Option<String>,
    pub total: f64,
    pub saldo: f64,
    pub fecha_creacion: Option<String>,
    pub fecha_entregado: Option<String>,
    pub garantia_hasta: Option<String>,
    pub en_garantia: bool,
}

const SELECT_FILA: &str = "SELECT e.id, e.numero, e.cliente_nombre, e.cliente_telefono, e.fecha_entrega, e.estado,
        r.etapa, r.tipo_equipo, r.marca, r.modelo, r.serie, r.falla, r.tecnico,
        CAST(e.total AS REAL), CAST(e.adelantado - e.devuelto AS REAL), e.fecha_creacion, e.fecha_entregado, r.garantia_dias
     FROM encargos e JOIN reparaciones r ON r.encargo_id = e.id";

fn fila_de(f: &libsql::Row) -> FilaReparacion {
    let total: f64 = f.get(13).unwrap_or(0.0);
    let pagado: f64 = f.get(14).unwrap_or(0.0);
    let estado: String = f.get(5).unwrap_or_default();
    let entregado: Option<String> = f.get(16).ok();
    let hasta = if estado == "ENTREGADO" { garantia_hasta(entregado.as_deref(), f.get(17).unwrap_or(0)) } else { None };
    FilaReparacion {
        id: f.get(0).unwrap_or_default(),
        numero: f.get(1).unwrap_or_default(),
        cliente_nombre: f.get(2).unwrap_or_default(),
        cliente_telefono: f.get(3).ok(),
        fecha_entrega: f.get(4).unwrap_or_default(),
        estado,
        etapa: f.get(6).unwrap_or_default(),
        tipo_equipo: f.get(7).unwrap_or_default(),
        marca: f.get(8).ok(),
        modelo: f.get(9).ok(),
        serie: f.get(10).ok(),
        falla: f.get(11).unwrap_or_default(),
        tecnico: f.get(12).ok(),
        total,
        saldo: r2((total - pagado).max(0.0)),
        fecha_creacion: f.get(15).ok(),
        fecha_entregado: entregado,
        en_garantia: hasta.as_deref().is_some_and(|h| h >= tiempo::hoy_lima().as_str()),
        garantia_hasta: hasta,
    }
}

async fn leer(conn: &libsql::Connection, id: i64) -> Result<Reparacion, Fallo> {
    let mut f = conn
        .query(
            "SELECT tipo_equipo, marca, modelo, serie, color, accesorios, clave, estado_fisico, falla, diagnostico, etapa, tecnico,
                    garantia_dias, aprobado_en
             FROM reparaciones WHERE encargo_id = ?1",
            libsql::params![id],
        )
        .await
        .map_err(sin_migracion)?;
    let Some(r) = f.next().await.map_err(e500)? else {
        return Err((StatusCode::NOT_FOUND, "La orden de reparación no existe.".into()));
    };
    let equipo = DatosEquipo {
        tipo_equipo: r.get(0).unwrap_or_default(),
        marca: r.get(1).ok(),
        modelo: r.get(2).ok(),
        serie: r.get(3).ok(),
        color: r.get(4).ok(),
        accesorios: r.get(5).ok(),
        clave: r.get(6).ok(),
        estado_fisico: r.get(7).ok(),
        falla: r.get(8).unwrap_or_default(),
    };
    let (diagnostico, etapa, tecnico, garantia_dias, aprobado_en) =
        (r.get::<String>(9).ok(), r.get::<String>(10).unwrap_or_default(), r.get::<String>(11).ok(), r.get::<i64>(12).unwrap_or(0), r.get::<String>(13).ok());
    drop(f);
    let orden = encargos::leer(conn, id).await?;
    let hasta = if orden.estado == "ENTREGADO" { garantia_hasta(orden.fecha_entregado.as_deref(), garantia_dias) } else { None };
    Ok(Reparacion {
        orden,
        equipo,
        etapa,
        diagnostico,
        tecnico,
        garantia_dias,
        en_garantia: hasta.as_deref().is_some_and(|h| h >= tiempo::hoy_lima().as_str()),
        garantia_hasta: hasta,
        aprobado_en,
    })
}

#[derive(Debug, Deserialize)]
pub struct Filtro {
    #[serde(default)]
    pub estado: Option<String>,
    #[serde(default)]
    pub buscar: Option<String>,
}

/// GET /reparaciones — las del taller por fecha prometida (atrasadas
/// primero); las entregadas y canceladas, las más recientes.
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<Filtro>) -> Result<Json<Vec<FilaReparacion>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let (donde, orden, limite) = match q.estado.as_deref().unwrap_or("ACTIVOS") {
        "ACTIVOS" => ("e.estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')", "e.fecha_entrega, COALESCE(e.hora_entrega, '99'), e.id", 500),
        "ENTREGADOS" => ("e.estado = 'ENTREGADO'", "e.fecha_entregado DESC, e.id DESC", 200),
        "CANCELADOS" => ("e.estado = 'CANCELADO'", "e.id DESC", 200),
        "TODOS" => ("1", "e.id DESC", 300),
        _ => return Err(malo("Estado no válido.")),
    };
    let mut condiciones = vec![donde.to_string()];
    let mut valores: Vec<libsql::Value> = Vec::new();
    if let Some(buscar) = q.buscar.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        let b: String = buscar.chars().take(60).collect();
        valores.push(format!("%{}%", b.to_lowercase()).into());
        valores.push(b.trim_start_matches('#').to_string().into());
        valores.push(format!("%{}%", serie_normal(&b)).into());
        condiciones.push(
            "(lower(e.cliente_nombre) LIKE ?1 OR COALESCE(e.cliente_telefono, '') LIKE ?1 OR CAST(e.numero AS TEXT) = ?2
              OR lower(COALESCE(r.marca, '') || ' ' || COALESCE(r.modelo, '')) LIKE ?1
              OR (length(?3) >= 6 AND COALESCE(r.serie, '') LIKE ?3))"
                .to_string(),
        );
    }
    let sql = format!("{} WHERE {} ORDER BY {} LIMIT {}", SELECT_FILA, condiciones.join(" AND "), orden, limite);
    let mut filas = conn.query(&sql, libsql::params_from_iter(valores)).await.map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(fila_de(&f));
    }
    Ok(Json(lista))
}

/// GET /reparaciones/:id
pub async fn obtener(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Reparacion>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct FiltroGarantia {
    pub serie: String,
    #[serde(default)]
    pub excluir: Option<i64>,
}

/// GET /reparaciones/garantia?serie= — reparaciones entregadas del mismo
/// equipo (por N.° de serie o IMEI), con su garantía.
pub async fn garantia(Extension(tenant): Extension<Arc<TenantDb>>, Query(q): Query<FiltroGarantia>) -> Result<Json<Vec<FilaReparacion>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let serie = serie_normal(&q.serie);
    if serie.len() < 4 {
        return Ok(Json(Vec::new()));
    }
    let mut filas = conn
        .query(
            &format!("{} WHERE r.serie = ?1 AND e.estado = 'ENTREGADO' AND e.id <> ?2 ORDER BY e.fecha_entregado DESC LIMIT 10", SELECT_FILA),
            libsql::params![serie, q.excluir.unwrap_or(0)],
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(fila_de(&f));
    }
    Ok(Json(lista))
}

/// GET /reparaciones/tecnicos — los técnicos de las últimas órdenes, para
/// elegirlos de un toque.
pub async fn tecnicos(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<String>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT tecnico FROM reparaciones WHERE tecnico IS NOT NULL AND tecnico <> ''
             GROUP BY lower(tecnico) ORDER BY MAX(encargo_id) DESC LIMIT 12",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(f.get::<String>(0).unwrap_or_default());
    }
    Ok(Json(lista))
}

// ---------------------------------------------------------------------
// Crear y editar
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EquipoNuevo {
    pub tipo_equipo: String,
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub serie: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub accesorios: Option<String>,
    #[serde(default)]
    pub clave: Option<String>,
    #[serde(default)]
    pub estado_fisico: Option<String>,
    pub falla: String,
}

#[derive(Debug, Deserialize)]
pub struct DatosReparacion {
    /// Cliente, fecha prometida, notas, presupuesto (items) y adelanto.
    #[serde(flatten)]
    pub orden: DatosEncargo,
    pub equipo: EquipoNuevo,
    #[serde(default)]
    pub diagnostico: Option<String>,
    #[serde(default)]
    pub tecnico: Option<String>,
    #[serde(default)]
    pub garantia_dias: Option<i64>,
}

struct Taller {
    equipo: DatosEquipo,
    diagnostico: Option<String>,
    tecnico: Option<String>,
    garantia_dias: i64,
}

fn revisar_taller(d: &DatosReparacion) -> Result<Taller, Fallo> {
    let tipo = d.equipo.tipo_equipo.trim().to_uppercase();
    if !TIPOS_EQUIPO.contains(&tipo.as_str()) {
        return Err(malo("Elige qué equipo es (celular, laptop, impresora...)."));
    }
    let falla = texto(Some(&d.equipo.falla), 500, "Falla")?.ok_or_else(|| malo("Escribe qué falla tiene el equipo (lo que cuenta el cliente)."))?;
    let serie = texto(d.equipo.serie.as_deref(), 40, "N.° de serie o IMEI")?.map(|s| serie_normal(&s)).filter(|s| !s.is_empty());
    let garantia_dias = d.garantia_dias.unwrap_or(0);
    if !(0..=MAX_GARANTIA).contains(&garantia_dias) {
        return Err(malo("La garantía va de 0 a 730 días."));
    }
    Ok(Taller {
        equipo: DatosEquipo {
            tipo_equipo: tipo,
            marca: texto(d.equipo.marca.as_deref(), 60, "Marca")?,
            modelo: texto(d.equipo.modelo.as_deref(), 60, "Modelo")?,
            serie,
            color: texto(d.equipo.color.as_deref(), 30, "Color")?,
            accesorios: texto(d.equipo.accesorios.as_deref(), 200, "Lo que deja con el equipo")?,
            // La clave se guarda tal cual (un patrón puede llevar espacios).
            clave: d.equipo.clave.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(|c| c.chars().take(60).collect()),
            estado_fisico: texto(d.equipo.estado_fisico.as_deref(), 300, "Cómo llega")?,
            falla,
        },
        diagnostico: texto(d.diagnostico.as_deref(), 1000, "Diagnóstico")?,
        tecnico: texto(d.tecnico.as_deref(), 60, "Técnico")?,
        garantia_dias,
    })
}

/// POST /reparaciones — el cliente deja el equipo (con adelanto, si deja).
pub async fn crear(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(d): Json<DatosReparacion>,
) -> Result<Json<Reparacion>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let taller = revisar_taller(&d)?;
    let l = encargos::revisar_con(&conn, &d.orden, true, true).await?;
    if let Some(a) = &d.orden.adelanto {
        if r2(a.monto) > l.total + 0.005 {
            return Err(malo(if l.total > 0.0 {
                format!("El adelanto (S/ {:.2}) es mayor que el presupuesto (S/ {:.2}).", a.monto, l.total)
            } else {
                "Para recibir un adelanto, primero pon el presupuesto (repuestos o mano de obra).".to_string()
            }));
        }
    }
    let e = &taller.equipo;
    let id = en_transaccion!(conn, |tx| {
        let ahora = tiempo::ahora_lima();
        tx.execute(
            "INSERT INTO encargos (numero, cliente_id, cliente_nombre, cliente_telefono, fecha_entrega, hora_entrega, entrega,
                                   notas, total, estado, usuario_id, fecha_creacion, fecha_actualizacion)
             VALUES ((SELECT COALESCE(MAX(numero), 0) + 1 FROM encargos), ?1, ?2, ?3, ?4, ?5, 'RECOJO', ?6, ?7, 'PENDIENTE', ?8, ?9, ?9)",
            libsql::params![
                l.cliente_id, l.cliente_nombre.clone(), l.cliente_telefono.clone(), l.fecha_entrega.clone(), l.hora_entrega.clone(),
                l.notas.clone(), l.total, claims.sub, ahora.clone()
            ],
        )
        .await
        .map_err(sin_migracion)?;
        let id = tx.last_insert_rowid();
        encargos::guardar_items(tx, id, &l.items).await?;
        tx.execute(
            "INSERT INTO reparaciones (encargo_id, tipo_equipo, marca, modelo, serie, color, accesorios, clave, estado_fisico, falla,
                                       diagnostico, etapa, tecnico, garantia_dias, fecha_actualizacion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'RECIBIDO', ?12, ?13, ?14)",
            libsql::params![
                id, e.tipo_equipo.clone(), e.marca.clone(), e.modelo.clone(), e.serie.clone(), e.color.clone(), e.accesorios.clone(),
                e.clave.clone(), e.estado_fisico.clone(), e.falla.clone(), taller.diagnostico.clone(), taller.tecnico.clone(),
                taller.garantia_dias, ahora
            ],
        )
        .await
        .map_err(sin_migracion)?;
        if let Some(a) = d.orden.adelanto.as_ref().filter(|a| a.monto > 0.0) {
            let (_, numero, _) = encargos::estado_y_numero(tx, id).await?;
            encargos::registrar_adelanto(tx, id, numero, &l.cliente_nombre, a, claims.sub).await?;
        }
        Ok(id)
    });
    Ok(Json(leer(&conn, id).await?))
}

/// PUT /reparaciones/:id — datos del cliente y del equipo, diagnóstico,
/// presupuesto, técnico y garantía. El presupuesto no puede quedar por
/// debajo de lo ya adelantado.
pub async fn actualizar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(d): Json<DatosReparacion>,
) -> Result<Json<Reparacion>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    leer(&conn, id).await?;
    let (estado, _, _) = encargos::estado_y_numero(&conn, id).await?;
    encargos::activo(&estado)?;
    let taller = revisar_taller(&d)?;
    let l = encargos::revisar_con(&conn, &d.orden, false, true).await?;
    let e = &taller.equipo;
    en_transaccion!(conn, |tx| {
        let ahora = tiempo::ahora_lima();
        let n = tx
            .execute(
                "UPDATE encargos SET cliente_id = ?1, cliente_nombre = ?2, cliente_telefono = ?3, fecha_entrega = ?4, hora_entrega = ?5,
                        notas = ?6, total = ?7, fecha_actualizacion = ?8
                 WHERE id = ?9 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO') AND adelantado - devuelto <= ?7 + 0.005",
                libsql::params![
                    l.cliente_id, l.cliente_nombre.clone(), l.cliente_telefono.clone(), l.fecha_entrega.clone(), l.hora_entrega.clone(),
                    l.notas.clone(), l.total, ahora.clone(), id
                ],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            return Err((
                StatusCode::CONFLICT,
                format!("El presupuesto (S/ {:.2}) no puede ser menor que lo que el cliente ya adelantó.", l.total),
            ));
        }
        encargos::guardar_items(tx, id, &l.items).await?;
        tx.execute(
            "UPDATE reparaciones SET tipo_equipo = ?1, marca = ?2, modelo = ?3, serie = ?4, color = ?5, accesorios = ?6, clave = ?7,
                    estado_fisico = ?8, falla = ?9, diagnostico = ?10, tecnico = ?11, garantia_dias = ?12, fecha_actualizacion = ?13
             WHERE encargo_id = ?14",
            libsql::params![
                e.tipo_equipo.clone(), e.marca.clone(), e.modelo.clone(), e.serie.clone(), e.color.clone(), e.accesorios.clone(),
                e.clave.clone(), e.estado_fisico.clone(), e.falla.clone(), taller.diagnostico.clone(), taller.tecnico.clone(),
                taller.garantia_dias, ahora, id
            ],
        )
        .await
        .map_err(e500)?;
        Ok(())
    });
    Ok(Json(leer(&conn, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct CambioEtapa {
    pub etapa: String,
}

/// POST /reparaciones/:id/etapa — avanza (o retrocede) la etapa del taller.
/// Entregar se hace cobrando en el punto de venta o con /devolver.
pub async fn cambiar_etapa(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(c): Json<CambioEtapa>,
) -> Result<Json<Reparacion>, Fallo> {
    let etapa = c.etapa.trim().to_uppercase();
    if !ETAPAS.contains(&etapa.as_str()) {
        return Err(malo("Etapa no válida."));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    leer(&conn, id).await?;
    en_transaccion!(conn, |tx| {
        let ahora = tiempo::ahora_lima();
        let n = tx
            .execute(
                "UPDATE encargos SET estado = ?1, fecha_actualizacion = ?2 WHERE id = ?3 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO')",
                libsql::params![estado_de_etapa(&etapa), ahora.clone(), id],
            )
            .await
            .map_err(e500)?;
        if n == 0 {
            let (actual, _, _) = encargos::estado_y_numero(tx, id).await?;
            encargos::activo(&actual)?;
        }
        tx.execute(
            "UPDATE reparaciones SET etapa = ?1, fecha_actualizacion = ?2,
                    aprobado_en = CASE WHEN ?1 = 'EN_REPARACION' AND aprobado_en IS NULL THEN ?2 ELSE aprobado_en END
             WHERE encargo_id = ?3",
            libsql::params![etapa.clone(), ahora, id],
        )
        .await
        .map_err(e500)?;
        Ok(())
    });
    Ok(Json(leer(&conn, id).await?))
}

/// POST /reparaciones/:id/devolver — entrega el equipo sin cobrar nada (no
/// se reparó y no se cobra la revisión). Solo si el presupuesto está en cero
/// y no dejó adelanto; si hay algo que cobrar, se entrega en el punto de venta.
pub async fn devolver(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<Json<Reparacion>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let r = leer(&conn, id).await?;
    encargos::activo(&r.orden.estado)?;
    if r.orden.total > 0.005 || r.orden.adelantado - r.orden.devuelto > 0.005 {
        return Err(malo(
            "Esta orden tiene algo que cobrar o un adelanto: entrégala cobrando en el punto de venta (o cancélala y devuelve el adelanto).",
        ));
    }
    let ahora = tiempo::ahora_lima();
    let n = conn
        .execute(
            "UPDATE encargos SET estado = 'ENTREGADO', fecha_entregado = ?1, fecha_actualizacion = ?1
             WHERE id = ?2 AND estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO') AND total < 0.005 AND adelantado - devuelto < 0.005",
            libsql::params![ahora, id],
        )
        .await
        .map_err(e500)?;
    if n == 0 {
        return Err((StatusCode::CONFLICT, "La orden cambió mientras tanto. Vuelve a abrirla.".into()));
    }
    encargos::olvidar_clave(&conn, id).await;
    Ok(Json(leer(&conn, id).await?))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn serie_y_etapas() {
        assert_eq!(serie_normal(" 3567-8901 2345 678 "), "356789012345678");
        assert_eq!(serie_normal("abc-123"), "ABC123");
        assert_eq!(estado_de_etapa("RECIBIDO"), "PENDIENTE");
        assert_eq!(estado_de_etapa("ESPERANDO_APROBACION"), "PENDIENTE");
        assert_eq!(estado_de_etapa("EN_REPARACION"), "EN_PREPARACION");
        assert_eq!(estado_de_etapa("NO_REPARADO"), "LISTO");
        assert_eq!(garantia_hasta(Some("2026-10-28 15:00:00"), 30).as_deref(), Some("2026-11-27"));
        assert_eq!(garantia_hasta(Some("2026-10-28"), 0), None);
        assert_eq!(garantia_hasta(None, 30), None);
    }
}
