//! Reporte de ganancias (módulo GANANCIAS).
//!
//! Viene APAGADO en todos los negocios; lo enciende el administrador en
//! Configuración. Apagado, nada de este archivo se ejecuta: no se guarda
//! costo en las ventas, no se calcula promedio y el precio de compra sigue
//! siendo opcional.
//!
//! Encendido:
//!   - el precio de compra es obligatorio en los productos que controlan stock;
//!   - cada producto lleva un COSTO PROMEDIO: cuando entra mercadería se
//!     promedia lo que había con lo que entró (no hace falta saber de qué
//!     compra salió cada unidad);
//!   - cada venta guarda el costo que tenía el producto ese día;
//!   - cada ingreso de mercadería queda anotado con lo que costó.
//!
//! Todo va con IGV incluido: lo que el negocio pagó contra lo que cobró.

use axum::{extract::{Extension, Query}, http::StatusCode, Json};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::MODULO_GANANCIAS;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

const NOMBRE_MODULO: &str = "Reporte de ganancias";

/// true si la lista de módulos guardada ("CREDITO,GANANCIAS") lo incluye.
pub fn encendido_en(modulos: &str) -> bool {
    modulos.split(',').any(|m| m.trim() == MODULO_GANANCIAS)
}

/// ¿El negocio tiene encendido el reporte de ganancias? Ante cualquier
/// duda (base sin la columna, error de lectura) la respuesta es no.
pub async fn activo(conn: &libsql::Connection) -> bool {
    let Ok(mut filas) = conn.query("SELECT modulos FROM configuracion_tienda LIMIT 1", ()).await else {
        return false;
    };
    match filas.next().await {
        Ok(Some(f)) => f.get::<String>(0).map(|m| encendido_en(&m)).unwrap_or(false),
        _ => false,
    }
}

/// Con el módulo encendido, un producto que controla stock no se guarda
/// sin precio de compra.
pub fn exigir_precio(controla_stock: bool, precio_compra: Option<f64>) -> Result<(), (StatusCode, String)> {
    if controla_stock && !(precio_compra.unwrap_or(0.0) > 0.0) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Falta el precio de compra. Con el reporte de ganancias activo es obligatorio en cada producto: escribe lo que te cuesta, con IGV.".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Costo promedio: se compara cómo estaba el producto antes de guardar con
// cómo quedó. Así las pantallas que ya existían (producto, modelo, lote,
// recepción de compra) no cambian su forma de guardar.
// ---------------------------------------------------------------------

/// Qué productos mirar.
pub enum Donde {
    Producto(i64),
    Modelo(i64),
    /// Los productos de una compra a proveedor.
    Compra(i64),
    /// Una lista de productos (importación desde Excel).
    Lista(Vec<i64>),
}

impl Donde {
    fn filtro(&self) -> String {
        match self {
            Donde::Producto(id) => format!("id = {}", id),
            Donde::Modelo(id) => format!("modelo_id = {}", id),
            Donde::Compra(id) => format!("id IN (SELECT producto_id FROM detalles_compra WHERE compra_id = {})", id),
            Donde::Lista(ids) if ids.is_empty() => "0 = 1".to_string(),
            Donde::Lista(ids) => format!("id IN ({})", ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fila {
    pub stock: f64,
    pub precio_compra: f64,
    /// Costo promedio guardado (None = aún no se calculó).
    pub costo: Option<f64>,
    pub controla_stock: bool,
}

impl Fila {
    /// Lo que cuesta hoy una unidad: el promedio, o el precio de compra si
    /// todavía no hay promedio. None = no se sabe.
    pub fn costo_vigente(&self) -> Option<f64> {
        self.costo.filter(|c| *c > 0.0).or(Some(self.precio_compra).filter(|p| *p > 0.0))
    }
}

pub type Foto = HashMap<i64, Fila>;

/// Cómo están ahora esos productos. Si algo falla devuelve vacío.
pub async fn foto(conn: &libsql::Connection, donde: &Donde) -> Foto {
    let mut productos = Foto::new();
    let consulta = format!(
        "SELECT id, CAST(stock AS REAL), CAST(COALESCE(precio_compra, 0) AS REAL), CAST(costo_promedio AS REAL),
                COALESCE(controla_stock, 1)
         FROM productos WHERE {}",
        donde.filtro()
    );
    let Ok(mut filas) = conn.query(&consulta, ()).await else {
        return productos;
    };
    while let Ok(Some(f)) = filas.next().await {
        productos.insert(
            f.get::<i64>(0).unwrap_or_default(),
            Fila {
                stock: f.get(1).unwrap_or(0.0),
                precio_compra: f.get(2).unwrap_or(0.0),
                costo: f.get::<f64>(3).ok(),
                controla_stock: f.get::<i64>(4).unwrap_or(1) == 1,
            },
        );
    }
    productos
}

/// Costo promedio después de que entran `entra` unidades a `precio`,
/// habiendo `habia` unidades a `costo_antes`.
pub fn promediar(habia: f64, costo_antes: Option<f64>, entra: f64, precio: f64) -> f64 {
    let habia = habia.max(0.0);
    match costo_antes {
        Some(costo) if habia > 0.0 => (habia * costo + entra * precio) / (habia + entra),
        _ => precio,
    }
}

fn redondear_4(valor: f64) -> f64 {
    (valor * 10_000.0).round() / 10_000.0
}

/// Después de guardar: actualiza el costo promedio y anota lo que entró.
///
/// - Producto nuevo: su costo es su precio de compra y su stock inicial
///   cuenta como una entrada.
/// - Subió el stock: lo que entró se promedia con lo que había, al precio
///   de compra que quedó en el producto.
/// - Solo cambió el precio de compra (`corregir`): es una corrección y el
///   costo pasa a ser ese precio. En un modelo con tallas, si a alguna talla
///   le entró mercadería el cambio de precio es por esa compra y las demás
///   conservan su costo.
///
/// No hace fallar a quien llama: el producto ya quedó guardado.
pub async fn conciliar(
    conn: &libsql::Connection,
    antes: &Foto,
    donde: &Donde,
    origen: &str,
    referencia: Option<i64>,
    corregir: bool,
) {
    conciliar_con(conn, antes, donde, origen, referencia, corregir, true).await;
}

/// Igual que `conciliar`, pero pudiendo NO anotar lo que entró como compra
/// del mes. Lo usa la importación desde Excel: el stock que se carga ya
/// estaba en la tienda (viene de otro sistema), no es mercadería comprada hoy.
pub async fn conciliar_con(
    conn: &libsql::Connection,
    antes: &Foto,
    donde: &Donde,
    origen: &str,
    referencia: Option<i64>,
    corregir: bool,
    anotar_entradas: bool,
) {
    let despues = foto(conn, donde).await;
    let entro = |id: &i64, d: &Fila| match antes.get(id) {
        Some(a) => d.stock - a.stock,
        None => d.stock,
    };
    let hubo_entrada = despues.iter().any(|(id, d)| d.controla_stock && entro(id, d) > 1e-9);
    let es_modelo = matches!(donde, Donde::Modelo(_));

    let mut sentencias: Vec<String> = Vec::new();
    let mut ids: Vec<&i64> = despues.keys().collect();
    ids.sort_unstable();
    for id in ids {
        let d = &despues[id];
        let precio = d.precio_compra;
        let mut nuevo_costo: Option<f64> = None;
        let mut entrada: Option<(f64, f64)> = None;

        match antes.get(id) {
            None => {
                if precio > 0.0 {
                    nuevo_costo = Some(precio);
                    if d.controla_stock && d.stock > 1e-9 {
                        entrada = Some((d.stock, precio));
                    }
                }
            }
            Some(a) => {
                let costo_antes = a.costo_vigente();
                let cantidad = d.stock - a.stock;
                let cambio_precio = (precio - a.precio_compra).abs() > 1e-9;
                if d.controla_stock && cantidad > 1e-9 {
                    let precio_entrada = if precio > 0.0 { precio } else { costo_antes.unwrap_or(0.0) };
                    if precio_entrada > 0.0 {
                        nuevo_costo = Some(promediar(a.stock, costo_antes, cantidad, precio_entrada));
                        entrada = Some((cantidad, precio_entrada));
                    }
                } else if cambio_precio && corregir && !(es_modelo && hubo_entrada) {
                    nuevo_costo = Some(precio).filter(|p| *p > 0.0);
                } else if cambio_precio && a.costo.is_none() {
                    // El precio de compra cambió por una compra que no movió
                    // este producto: su costo sigue siendo el de antes.
                    nuevo_costo = costo_antes;
                }
            }
        }

        if let Some(costo) = nuevo_costo.filter(|c| c.is_finite() && *c > 0.0) {
            sentencias.push(format!("UPDATE productos SET costo_promedio = {} WHERE id = {};\n", redondear_4(costo), id));
        }
        if let Some((cantidad, costo)) = entrada.filter(|(c, p)| anotar_entradas && c.is_finite() && p.is_finite()) {
            sentencias.push(format!(
                "INSERT INTO entradas_stock (producto_id, cantidad, costo_unitario, origen, referencia_id) VALUES ({}, {}, {}, '{}', {});\n",
                id,
                cantidad,
                redondear_4(costo),
                origen,
                referencia.map(|r| r.to_string()).unwrap_or_else(|| "NULL".to_string())
            ));
        }
    }
    // Por tandas: una importación puede traer miles de productos.
    for tanda in sentencias.chunks(200) {
        if let Err(e) = conn.execute_batch(&tanda.concat()).await {
            eprintln!("⚠️  Ganancias: no se pudo actualizar el costo promedio ({}): {}", origen, e);
            break;
        }
    }
}

/// Deja guardado en cada línea de la venta lo que costaba el producto en
/// ese momento. Se llama dentro de la transacción de la venta y solo con el
/// módulo encendido. No puede hacer fallar la venta.
pub async fn congelar_costo(conn: &libsql::Connection, venta_id: i64) {
    let _ = conn
        .execute(
            &format!(
                "UPDATE detalles_venta SET costo_unitario = (
                     SELECT CASE WHEN p.costo_promedio > 0 THEN p.costo_promedio
                                 WHEN p.precio_compra > 0 THEN p.precio_compra END
                       FROM productos p WHERE p.id = detalles_venta.producto_id)
                  WHERE venta_id = {}",
                venta_id
            ),
            (),
        )
        .await;
}

// ---------------------------------------------------------------------
// Costos para la pantalla de Productos
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CostoProducto {
    pub producto_id: i64,
    /// Costo promedio vigente (None = el producto no tiene precio de compra).
    pub costo_promedio: Option<f64>,
}

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn actualizando<E>(_: E) -> (StatusCode, String) {
    (StatusCode::CONFLICT, "El sistema se está actualizando. Intenta de nuevo en un minuto.".to_string())
}

async fn exigir_modulo(conn: &libsql::Connection) -> Result<(), (StatusCode, String)> {
    if activo(conn).await {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", NOMBRE_MODULO),
        ))
    }
}

/// GET /ganancias/costos — costo promedio de cada producto activo, para
/// mostrarlo en el formulario del producto.
pub async fn costos(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Vec<CostoProducto>> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;
    let mut filas = conn
        .query(
            "SELECT id, CAST(CASE WHEN costo_promedio > 0 THEN costo_promedio
                                  WHEN precio_compra > 0 THEN precio_compra END AS REAL)
             FROM productos WHERE activo = 1",
            (),
        )
        .await
        .map_err(actualizando)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(CostoProducto { producto_id: f.get(0).unwrap_or_default(), costo_promedio: f.get::<f64>(1).ok() });
    }
    Ok(Json(lista))
}

// ---------------------------------------------------------------------
// Reporte
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FiltroMes {
    /// "2026-10". Sin él, el mes actual en Perú.
    pub mes: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MesGanancia {
    pub mes: String,
    /// Todo lo vendido en el mes, menos las devoluciones.
    pub vendido: f64,
    /// La parte de lo vendido que tiene costo conocido (entra en la ganancia).
    pub vendido_con_costo: f64,
    /// Lo que costó esa mercadería.
    pub costo: f64,
    pub ganancia: f64,
    /// Ganancia sobre lo vendido con costo, en %. None si no hubo ventas.
    pub margen: Option<f64>,
    /// Vendido de productos sin precio de compra (fuera de la ganancia).
    pub sin_costo: f64,
    /// Lo que se gastó en mercadería que entró ese mes.
    pub compras: f64,
    /// Gastos del negocio del mes (alquiler, luz, sueldos...; módulo Gastos).
    pub gastos: f64,
    /// Ganancia de las ventas menos los gastos del negocio.
    pub ganancia_neta: f64,
    /// Parte del costo se calculó con el precio de compra de hoy (ventas
    /// anteriores a encender el módulo).
    pub estimado: bool,
}

#[derive(Debug, Serialize)]
pub struct ProductoGanancia {
    pub producto_id: i64,
    pub nombre: String,
    /// Nombre del modelo si es una talla o color ("Polo básico").
    pub modelo: Option<String>,
    pub categoria: String,
    pub unidad: String,
    pub cantidad: f64,
    pub vendido: f64,
    /// None = se vendió sin precio de compra.
    pub costo: Option<f64>,
    pub ganancia: Option<f64>,
    pub estimado: bool,
}

#[derive(Debug, Serialize)]
pub struct DevolucionesMes {
    pub monto: f64,
    /// Costo de lo devuelto que volvió al stock.
    pub costo_recuperado: f64,
}

#[derive(Debug, Serialize)]
pub struct ReporteGanancias {
    pub mes: String,
    /// Los 12 meses que terminan en `mes`, del más antiguo al más nuevo.
    pub meses: Vec<MesGanancia>,
    /// Lo vendido en `mes`, producto por producto (sin restar devoluciones).
    pub productos: Vec<ProductoGanancia>,
    pub devoluciones: DevolucionesMes,
    /// Productos activos que controlan stock y no tienen precio de compra.
    pub productos_sin_precio: i64,
}

/// Costo de una línea de venta: el que quedó guardado al vender o, si no
/// hay, el costo que tiene hoy el producto.
const COSTO: &str = "COALESCE(dv.costo_unitario, CASE WHEN p.costo_promedio > 0 THEN p.costo_promedio WHEN p.precio_compra > 0 THEN p.precio_compra END)";
const SIN_STOCK: &str = "COALESCE(p.controla_stock, 1) = 0";

/// Primer día de un mes "AAAA-MM".
fn primer_dia(mes: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{}-01", mes), "%Y-%m-%d").ok()
}

fn sumar_meses(dia: NaiveDate, meses: i32) -> NaiveDate {
    let total = dia.year() * 12 + dia.month0() as i32 + meses;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap_or(dia)
}

/// El instante (en UTC, como guarda la base) en que empieza ese día en Perú.
fn inicio_utc(dia: NaiveDate) -> String {
    format!("{} 05:00:00", dia.format("%Y-%m-%d"))
}

fn redondear_2(valor: f64) -> f64 {
    (valor * 100.0).round() / 100.0
}

/// GET /reportes/ganancias?mes=2026-10 — solo el administrador, y solo con
/// el módulo encendido.
pub async fn reporte(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(filtro): Query<FiltroMes>,
) -> Resultado<ReporteGanancias> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn).await?;

    let hoy = (Utc::now() - Duration::hours(5)).date_naive();
    let mes_actual = NaiveDate::from_ymd_opt(hoy.year(), hoy.month(), 1).unwrap_or(hoy);
    let mes = match filtro.mes.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(texto) => primer_dia(texto).ok_or((StatusCode::BAD_REQUEST, "Mes no válido. Usa el formato AAAA-MM.".to_string()))?,
        None => mes_actual,
    };
    if mes > mes_actual {
        return Err((StatusCode::BAD_REQUEST, "Ese mes todavía no empieza.".to_string()));
    }
    let clave = |d: NaiveDate| d.format("%Y-%m").to_string();
    let desde = inicio_utc(sumar_meses(mes, -11));
    let hasta = inicio_utc(sumar_meses(mes, 1));
    let desde_mes = inicio_utc(mes);

    // 12 meses en cero, que luego se llenan.
    let mut meses: Vec<MesGanancia> = (0..12)
        .map(|i| MesGanancia {
            mes: clave(sumar_meses(mes, i - 11)),
            vendido: 0.0,
            vendido_con_costo: 0.0,
            costo: 0.0,
            ganancia: 0.0,
            margen: None,
            sin_costo: 0.0,
            compras: 0.0,
            gastos: 0.0,
            ganancia_neta: 0.0,
            estimado: false,
        })
        .collect();
    let posicion: HashMap<String, usize> = meses.iter().enumerate().map(|(i, m)| (m.mes.clone(), i)).collect();

    // 1. Ventas por mes (el mes es el de Perú: la base guarda UTC).
    let mut filas = conn
        .query(
            &format!(
                "SELECT strftime('%Y-%m', v.fecha_hora, '-5 hours'),
                        CAST(COALESCE(SUM(dv.total_linea), 0) AS REAL),
                        CAST(COALESCE(SUM(CASE WHEN {costo} > 0 OR {sin_stock} THEN dv.total_linea ELSE 0 END), 0) AS REAL),
                        CAST(COALESCE(SUM(CASE WHEN {costo} > 0 THEN dv.cantidad * {costo} ELSE 0 END), 0) AS REAL),
                        COALESCE(SUM(CASE WHEN dv.costo_unitario IS NULL AND {costo} > 0 THEN 1 ELSE 0 END), 0)
                 FROM detalles_venta dv
                 JOIN ventas v ON v.id = dv.venta_id
                 JOIN productos p ON p.id = dv.producto_id
                 WHERE v.estado = 'COMPLETADA' AND v.fecha_hora >= ?1 AND v.fecha_hora < ?2
                 GROUP BY 1",
                costo = COSTO,
                sin_stock = SIN_STOCK
            ),
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(actualizando)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        if let Some(i) = posicion.get(&f.get::<String>(0).unwrap_or_default()) {
            let m = &mut meses[*i];
            m.vendido = f.get(1).unwrap_or(0.0);
            m.vendido_con_costo = f.get(2).unwrap_or(0.0);
            m.costo = f.get(3).unwrap_or(0.0);
            m.estimado = f.get::<i64>(4).unwrap_or(0) > 0;
        }
    }
    drop(filas);

    // 2. Devoluciones por mes: restan lo vendido; el costo solo se recupera
    //    si el producto volvió al stock.
    let mut devoluciones = DevolucionesMes { monto: 0.0, costo_recuperado: 0.0 };
    let mut filas = conn
        .query(
            &format!(
                "SELECT strftime('%Y-%m', d.fecha_hora, '-5 hours'),
                        CAST(COALESCE(SUM(dd.subtotal), 0) AS REAL),
                        CAST(COALESCE(SUM(CASE WHEN {costo} > 0 OR {sin_stock} THEN dd.subtotal ELSE 0 END), 0) AS REAL),
                        CAST(COALESCE(SUM(CASE WHEN dd.condicion = 'REVENTA' AND {costo} > 0 THEN dd.cantidad_devuelta * {costo} ELSE 0 END), 0) AS REAL)
                 FROM detalles_devolucion dd
                 JOIN devoluciones d ON d.id = dd.devolucion_id
                 JOIN productos p ON p.id = dd.producto_id
                 LEFT JOIN detalles_venta dv ON dv.id = dd.detalle_venta_id
                 WHERE d.estado = 'PROCESADA' AND d.fecha_hora >= ?1 AND d.fecha_hora < ?2
                 GROUP BY 1",
                costo = COSTO,
                sin_stock = SIN_STOCK
            ),
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(actualizando)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let clave_mes = f.get::<String>(0).unwrap_or_default();
        if let Some(i) = posicion.get(&clave_mes) {
            let (monto, monto_con_costo, costo): (f64, f64, f64) =
                (f.get(1).unwrap_or(0.0), f.get(2).unwrap_or(0.0), f.get(3).unwrap_or(0.0));
            let m = &mut meses[*i];
            m.vendido -= monto;
            m.vendido_con_costo -= monto_con_costo;
            m.costo -= costo;
            if *i == 11 {
                devoluciones = DevolucionesMes { monto: redondear_2(monto), costo_recuperado: redondear_2(costo) };
            }
        }
    }
    drop(filas);

    // 3. Mercadería que entró cada mes.
    let mut filas = conn
        .query(
            "SELECT strftime('%Y-%m', fecha_hora, '-5 hours'), CAST(COALESCE(SUM(cantidad * costo_unitario), 0) AS REAL)
             FROM entradas_stock WHERE fecha_hora >= ?1 AND fecha_hora < ?2 GROUP BY 1",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
        .map_err(actualizando)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        if let Some(i) = posicion.get(&f.get::<String>(0).unwrap_or_default()) {
            meses[*i].compras = redondear_2(f.get(1).unwrap_or(0.0));
        }
    }
    drop(filas);

    // 3b. Gastos del negocio de cada mes (la fecha del gasto ya es de Perú).
    let primer_dia_rango = sumar_meses(mes, -11).format("%Y-%m-%d").to_string();
    let ultimo_dia_mes = sumar_meses(mes, 1).pred_opt().unwrap_or(mes).format("%Y-%m-%d").to_string();
    for (clave_mes, total) in crate::handlers::gastos::totales_por_mes(&conn, &primer_dia_rango, &ultimo_dia_mes).await {
        if let Some(i) = posicion.get(&clave_mes) {
            meses[*i].gastos = redondear_2(total);
        }
    }

    for m in &mut meses {
        m.vendido = redondear_2(m.vendido);
        m.vendido_con_costo = redondear_2(m.vendido_con_costo);
        m.costo = redondear_2(m.costo);
        m.sin_costo = redondear_2(m.vendido - m.vendido_con_costo);
        m.ganancia = redondear_2(m.vendido_con_costo - m.costo);
        m.ganancia_neta = redondear_2(m.ganancia - m.gastos);
        m.margen = Some(m.vendido_con_costo).filter(|v| *v > 0.005).map(|v| (m.ganancia / v * 1000.0).round() / 10.0);
    }

    // 4. El mes elegido, producto por producto.
    let mut productos = Vec::new();
    let mut filas = conn
        .query(
            &format!(
                "SELECT p.id, p.nombre, p.modelo_nombre, COALESCE(c.nombre, 'Sin categoría'), COALESCE(p.unidad_medida, 'UNIDAD'),
                        CAST(SUM(dv.cantidad) AS REAL), CAST(SUM(dv.total_linea) AS REAL),
                        CAST(SUM(CASE WHEN {costo} > 0 THEN dv.cantidad * {costo} ELSE 0 END) AS REAL),
                        SUM(CASE WHEN {costo} > 0 OR {sin_stock} THEN 0 ELSE 1 END),
                        SUM(CASE WHEN dv.costo_unitario IS NULL AND {costo} > 0 THEN 1 ELSE 0 END)
                 FROM detalles_venta dv
                 JOIN ventas v ON v.id = dv.venta_id
                 JOIN productos p ON p.id = dv.producto_id
                 LEFT JOIN categorias c ON c.id = p.categoria_id
                 WHERE v.estado = 'COMPLETADA' AND v.fecha_hora >= ?1 AND v.fecha_hora < ?2
                 GROUP BY p.id
                 ORDER BY 7 DESC",
                costo = COSTO,
                sin_stock = SIN_STOCK
            ),
            libsql::params![desde_mes, hasta],
        )
        .await
        .map_err(actualizando)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let vendido: f64 = f.get(6).unwrap_or(0.0);
        let costo: f64 = f.get(7).unwrap_or(0.0);
        let sin_costo = f.get::<i64>(8).unwrap_or(0) > 0;
        productos.push(ProductoGanancia {
            producto_id: f.get(0).unwrap_or_default(),
            nombre: f.get(1).unwrap_or_default(),
            modelo: f.get::<String>(2).ok().filter(|m| !m.is_empty()),
            categoria: f.get(3).unwrap_or_default(),
            unidad: f.get(4).unwrap_or_default(),
            cantidad: f.get(5).unwrap_or(0.0),
            vendido: redondear_2(vendido),
            costo: if sin_costo { None } else { Some(redondear_2(costo)) },
            ganancia: if sin_costo { None } else { Some(redondear_2(vendido - costo)) },
            estimado: f.get::<i64>(9).unwrap_or(0) > 0,
        });
    }
    drop(filas);

    // 5. Productos que siguen sin precio de compra.
    let mut filas = conn
        .query(
            "SELECT COUNT(*) FROM productos
             WHERE activo = 1 AND COALESCE(controla_stock, 1) = 1 AND NOT (COALESCE(precio_compra, 0) > 0)",
            (),
        )
        .await
        .map_err(e500)?;
    let productos_sin_precio: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };

    Ok(Json(ReporteGanancias { mes: clave(mes), meses, productos, devoluciones, productos_sin_precio }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn promedio_ponderado() {
        // 6 polos a 20 y entran 10 a 24 -> 22.50
        assert!((promediar(6.0, Some(20.0), 10.0, 24.0) - 22.5).abs() < 1e-9);
        // 6 a 20 y entran 20 a 24 -> 23.0769
        assert!((promediar(6.0, Some(20.0), 20.0, 24.0) - 23.076923).abs() < 1e-5);
        // Sin stock o sin costo previo: vale el precio de lo que entra.
        assert_eq!(promediar(0.0, Some(20.0), 10.0, 24.0), 24.0);
        assert_eq!(promediar(5.0, None, 10.0, 24.0), 24.0);
        assert_eq!(promediar(-2.0, Some(20.0), 10.0, 24.0), 24.0);
    }

    #[test]
    fn meses_y_dia_de_peru() {
        let enero = primer_dia("2026-01").unwrap();
        assert_eq!(sumar_meses(enero, -1).format("%Y-%m").to_string(), "2025-12");
        assert_eq!(sumar_meses(enero, -11).format("%Y-%m").to_string(), "2025-02");
        assert_eq!(sumar_meses(primer_dia("2026-12").unwrap(), 1).format("%Y-%m").to_string(), "2027-01");
        assert_eq!(inicio_utc(enero), "2026-01-01 05:00:00");
        assert!(primer_dia("2026-13").is_none() && primer_dia("octubre").is_none());
        assert!(encendido_en("CREDITO,GANANCIAS") && encendido_en(" GANANCIAS ") && !encendido_en("CREDITO,GUIAS") && !encendido_en(""));
    }
}
