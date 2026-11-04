//! Módulo VENTA_FUERA_INVENTARIO: vender algo que el negocio consigue afuera
//! y no está en su inventario (migración 0059).
//!
//! Regla del módulo: lo de afuera NO entra al sistema. La venta guarda solo
//! lo del inventario (su total, su pago y por eso la caja, reportes,
//! ganancias, stock y créditos). Lo de afuera se guarda aparte, en sus
//! propias tablas, y solo lo leen:
//!   - el comprobante (igv::desglose_venta: boleta/factura a SUNAT, ticket,
//!     A4/A5 y la consulta pública), que lleva el total completo;
//!   - la lista de Comprobantes (el monto completo, con su marca);
//!   - la reimpresión (buscar_venta_para_devolucion lo trae aparte).
//!
//! Un negocio sin el módulo no puede mandar líneas de afuera, y una venta
//! sin ellas no hace nada distinto: estas tablas simplemente no tienen filas
//! para ella. Ante cualquier error de lectura se responde "no hay nada de
//! afuera", como antes de existir el módulo.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::handlers::rubros::MODULO_VENTA_FUERA_INVENTARIO;
use crate::logica::igv::Afectacion;

/// Máximo de líneas de afuera en una venta.
const MAX_LINEAS: usize = 50;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Una línea de afuera tal como la manda el punto de venta.
#[derive(Debug, Clone, Deserialize)]
pub struct LineaFuera {
    pub nombre: String,
    pub cantidad: f64,
    /// Precio unitario con IGV.
    pub precio: f64,
    #[serde(default)]
    pub unidad_medida: Option<String>,
    #[serde(default)]
    pub codigo: Option<String>,
}

/// Línea ya revisada, lista para guardar.
#[derive(Debug, Clone, PartialEq)]
pub struct LineaLista {
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub total_linea: f64,
    pub unidad_medida: String,
    pub codigo: Option<String>,
}

/// Lo de afuera de una venta, revisado.
#[derive(Debug, Clone, PartialEq)]
pub struct FueraLista {
    pub lineas: Vec<LineaLista>,
    pub total: f64,
}

/// ¿El negocio tiene el módulo encendido?
pub fn encendido_en(modulos: Option<&str>) -> bool {
    modulos.is_some_and(|m| m.split(',').any(|x| x.trim() == MODULO_VENTA_FUERA_INVENTARIO))
}

/// Revisa las líneas de afuera de una venta. None (o lista vacía) = venta de
/// siempre: Ok(None) y nada cambia.
pub fn preparar(modulos: Option<&str>, lineas: Option<&[LineaFuera]>) -> Result<Option<FueraLista>, String> {
    let Some(lineas) = lineas.filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    if !encendido_en(modulos) {
        return Err("Este negocio no tiene encendido \"Vender algo que no está en inventario\".".into());
    }
    if lineas.len() > MAX_LINEAS {
        return Err(format!("Una venta puede llevar hasta {} cosas que no están en inventario.", MAX_LINEAS));
    }
    let mut listas = Vec::with_capacity(lineas.len());
    let mut total = 0.0;
    for l in lineas {
        let nombre: String = l.nombre.trim().chars().take(200).collect();
        if nombre.is_empty() {
            return Err("Escribe el nombre de lo que vendes fuera de inventario.".into());
        }
        if !l.cantidad.is_finite() || l.cantidad <= 0.0 {
            return Err(format!("La cantidad de \"{}\" debe ser mayor a 0.", nombre));
        }
        if !l.precio.is_finite() || l.precio <= 0.0 {
            return Err(format!("El precio de \"{}\" debe ser mayor a 0.", nombre));
        }
        let unidad = l
            .unidad_medida
            .as_deref()
            .map(|u| u.trim().to_uppercase())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "UNIDAD".to_string());
        if unidad.len() > 20 || !unidad.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            return Err(format!("La unidad de \"{}\" no es válida.", nombre));
        }
        let codigo = l
            .codigo
            .as_deref()
            .map(|c| c.trim().chars().take(40).collect::<String>())
            .filter(|c| !c.is_empty());
        let total_linea = r2(l.precio * l.cantidad);
        total += total_linea;
        listas.push(LineaLista {
            nombre,
            cantidad: l.cantidad,
            precio_unitario: l.precio,
            total_linea,
            unidad_medida: unidad,
            codigo,
        });
    }
    Ok(Some(FueraLista { lineas: listas, total: r2(total) }))
}

/// Pago mixto: el cliente pagó el total completo en dos partes. Lo de afuera
/// se descuenta primero de la parte de abajo (`pago_efectivo`, que es el
/// efectivo; la otra parte nunca es efectivo) y, si no alcanza, de la otra.
/// Devuelve las dos partes que quedan para el inventario (lo que va a caja).
pub fn descontar_del_mixto(pago_efectivo: f64, pago_otro: f64, externo: f64) -> (f64, f64) {
    let de_abajo = externo.min(pago_efectivo).max(0.0);
    let resto = (externo - de_abajo).max(0.0);
    (r2(pago_efectivo - de_abajo), r2((pago_otro - resto).max(0.0)))
}

/// Guarda lo de afuera de la venta. Va dentro de la transacción de la venta:
/// si falla, la venta no entra (el comprobante lo necesita).
pub async fn registrar(
    conn: &libsql::Connection,
    venta_id: i64,
    lista: &FueraLista,
    pago_original: (Option<f64>, Option<f64>),
) -> Result<(), (StatusCode, String)> {
    let e500 = |e: libsql::Error| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error al guardar lo de fuera de inventario: {}", e));
    conn.execute(
        "INSERT INTO venta_fuera_inventario (venta_id, total, pago_efectivo, pago_otro) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![
            venta_id,
            lista.total,
            pago_original.0.map(libsql::Value::from).unwrap_or(libsql::Value::Null),
            pago_original.1.map(libsql::Value::from).unwrap_or(libsql::Value::Null)
        ],
    )
    .await
    .map_err(e500)?;
    for l in &lista.lineas {
        conn.execute(
            "INSERT INTO venta_fuera_inventario_linea (venta_id, nombre, cantidad, precio_unitario, total_linea, unidad_medida, codigo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            libsql::params![
                venta_id,
                l.nombre.clone(),
                l.cantidad,
                l.precio_unitario,
                l.total_linea,
                l.unidad_medida.clone(),
                l.codigo.clone().map(libsql::Value::from).unwrap_or(libsql::Value::Null)
            ],
        )
        .await
        .map_err(e500)?;
    }
    Ok(())
}

/// Una línea de afuera para imprimir.
#[derive(Debug, Clone, Serialize)]
pub struct LineaImpresa {
    pub nombre: String,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub total: f64,
    pub unidad_medida: String,
    pub codigo: Option<String>,
}

/// Lo de afuera de una venta, para reimprimir su comprobante.
#[derive(Debug, Clone, Serialize)]
pub struct FueraDeVenta {
    pub total: f64,
    /// Pago mixto como lo hizo el cliente (por el total del comprobante).
    pub pago_efectivo: Option<f64>,
    pub pago_otro: Option<f64>,
    pub lineas: Vec<LineaImpresa>,
}

async fn lineas_de(conn: &libsql::Connection, venta_id: i64) -> Vec<LineaImpresa> {
    let Ok(mut filas) = conn
        .query(
            "SELECT nombre, CAST(cantidad AS REAL), CAST(precio_unitario AS REAL), CAST(total_linea AS REAL),
                    COALESCE(unidad_medida, 'UNIDAD'), codigo
             FROM venta_fuera_inventario_linea WHERE venta_id = ?1 ORDER BY id",
            libsql::params![venta_id],
        )
        .await
    else {
        return Vec::new();
    };
    let mut lineas = Vec::new();
    while let Ok(Some(f)) = filas.next().await {
        lineas.push(LineaImpresa {
            nombre: f.get(0).unwrap_or_default(),
            cantidad: f.get(1).unwrap_or(0.0),
            precio_unitario: f.get(2).unwrap_or(0.0),
            total: f.get(3).unwrap_or(0.0),
            unidad_medida: f.get(4).unwrap_or_else(|_| "UNIDAD".to_string()),
            codigo: f.get::<String>(5).ok().filter(|c| !c.is_empty()),
        });
    }
    lineas
}

/// Lo de afuera de una venta. None si no llevó nada (o ante cualquier error).
pub async fn de_venta(conn: &libsql::Connection, venta_id: i64) -> Option<FueraDeVenta> {
    let mut filas = conn
        .query(
            "SELECT CAST(total AS REAL), CAST(pago_efectivo AS REAL), CAST(pago_otro AS REAL)
             FROM venta_fuera_inventario WHERE venta_id = ?1",
            libsql::params![venta_id],
        )
        .await
        .ok()?;
    let f = filas.next().await.ok()??;
    let total: f64 = f.get(0).ok()?;
    let pago_efectivo: Option<f64> = f.get(1).ok();
    let pago_otro: Option<f64> = f.get(2).ok();
    drop(filas);
    Some(FueraDeVenta { total, pago_efectivo, pago_otro, lineas: lineas_de(conn, venta_id).await })
}

/// Total de lo de afuera de una venta (0 si no llevó nada).
pub async fn total_de(conn: &libsql::Connection, venta_id: i64) -> f64 {
    let Ok(mut filas) = conn
        .query("SELECT CAST(total AS REAL) FROM venta_fuera_inventario WHERE venta_id = ?1", libsql::params![venta_id])
        .await
    else {
        return 0.0;
    };
    match filas.next().await {
        Ok(Some(f)) => f.get::<f64>(0).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Totales de afuera de varias ventas: venta_id -> total. Solo las que
/// llevaron algo; vacío ante cualquier error.
pub async fn totales_de(conn: &libsql::Connection, ventas: &[i64]) -> HashMap<i64, f64> {
    let mut mapa = HashMap::new();
    if ventas.is_empty() {
        return mapa;
    }
    // Los ids son enteros: se escriben directo en la lista.
    let lista = ventas.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let Ok(mut filas) = conn
        .query(
            &format!("SELECT venta_id, CAST(total AS REAL) FROM venta_fuera_inventario WHERE venta_id IN ({})", lista),
            (),
        )
        .await
    else {
        return mapa;
    };
    while let Ok(Some(f)) = filas.next().await {
        if let (Ok(id), Ok(total)) = (f.get::<i64>(0), f.get::<f64>(1)) {
            mapa.insert(id, total);
        }
    }
    mapa
}

/// Ventas COMPLETADAS de un rango de fechas (de Lima, como en Reportes) que
/// solo llevaron cosas de afuera: para el sistema valen S/ 0 y no tienen
/// productos, así que Reportes y Resumen no las muestran ni las cuentan (su
/// comprobante está en Comprobantes). Devuelve (venta_id, medio de pago como
/// lo agrupa Reportes; None en un pago mixto, que ahí no cuenta). Vacío ante
/// cualquier error o si no hay ninguna (lo normal).
pub async fn solo_fuera_en_rango(conn: &libsql::Connection, desde: &str, hasta: &str) -> Vec<(i64, Option<String>)> {
    let consulta = |billetera: &str| {
        format!(
            "SELECT v.id, CASE WHEN v.metodo_pago = 'MIXTO' THEN NULL
                               WHEN v.metodo_pago = 'YAPE_PLIN' THEN {} ELSE v.metodo_pago END
             FROM venta_fuera_inventario f JOIN ventas v ON v.id = f.venta_id
             WHERE date(v.fecha_hora, '-5 hours') BETWEEN ?1 AND ?2 AND v.estado = 'COMPLETADA'
               AND NOT EXISTS (SELECT 1 FROM detalles_venta d WHERE d.venta_id = v.id)",
            billetera
        )
    };
    let parametros = || libsql::params![desde.to_string(), hasta.to_string()];
    let filas = match conn.query(&consulta("COALESCE(v.billetera, 'YAPE_PLIN')"), parametros()).await {
        Ok(f) => Ok(f),
        Err(_) => conn.query(&consulta("'YAPE_PLIN'"), parametros()).await,
    };
    let Ok(mut filas) = filas else {
        return Vec::new();
    };
    let mut ventas = Vec::new();
    while let Ok(Some(f)) = filas.next().await {
        if let Ok(id) = f.get::<i64>(0) {
            ventas.push((id, f.get::<String>(1).ok()));
        }
    }
    ventas
}

/// Las líneas de afuera para el comprobante, después de las del inventario.
/// Son gravadas: el precio que se escribe ya lleva IGV. Vacío si no hay.
pub async fn lineas_para_comprobante(conn: &libsql::Connection, venta_id: i64) -> Vec<crate::handlers::igv::LineaVenta> {
    lineas_de(conn, venta_id)
        .await
        .into_iter()
        .map(|l| crate::handlers::igv::LineaVenta {
            descripcion: l.nombre,
            cantidad: l.cantidad,
            precio_unitario: l.precio_unitario,
            unidad_medida: l.unidad_medida,
            total: l.total,
            afectacion: Afectacion::Gravado,
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn linea(nombre: &str, cantidad: f64, precio: f64) -> LineaFuera {
        LineaFuera { nombre: nombre.into(), cantidad, precio, unidad_medida: None, codigo: None }
    }

    #[test]
    fn sin_lineas_no_cambia_nada() {
        assert_eq!(preparar(None, None), Ok(None));
        assert_eq!(preparar(Some("CREDITO"), Some(&[])), Ok(None));
    }

    #[test]
    fn sin_el_modulo_no_se_puede() {
        assert!(preparar(Some("CREDITO,VENTA_TABLA"), Some(&[linea("Bomba", 1.0, 120.0)])).is_err());
    }

    #[test]
    fn revisa_y_suma() {
        let l = preparar(
            Some("CREDITO,VENTA_FUERA_INVENTARIO"),
            Some(&[
                linea("  Bomba de agua  ", 1.0, 120.0),
                LineaFuera { nombre: "Faja".into(), cantidad: 2.0, precio: 15.5, unidad_medida: Some("juego".into()), codigo: Some(" F-1 ".into()) },
            ]),
        )
        .unwrap()
        .unwrap();
        assert_eq!(l.total, 151.0);
        assert_eq!(l.lineas[0].nombre, "Bomba de agua");
        assert_eq!(l.lineas[0].unidad_medida, "UNIDAD");
        assert_eq!(l.lineas[1].unidad_medida, "JUEGO");
        assert_eq!(l.lineas[1].codigo.as_deref(), Some("F-1"));
        assert_eq!(l.lineas[1].total_linea, 31.0);
    }

    #[test]
    fn rechaza_lo_invalido() {
        let m = Some("VENTA_FUERA_INVENTARIO");
        assert!(preparar(m, Some(&[linea("  ", 1.0, 10.0)])).is_err());
        assert!(preparar(m, Some(&[linea("X", 0.0, 10.0)])).is_err());
        assert!(preparar(m, Some(&[linea("X", 1.0, 0.0)])).is_err());
        assert!(preparar(m, Some(&[linea("X", f64::NAN, 10.0)])).is_err());
        let mala = LineaFuera { nombre: "X".into(), cantidad: 1.0, precio: 1.0, unidad_medida: Some("KG; DROP".into()), codigo: None };
        assert!(preparar(m, Some(&[mala])).is_err());
    }

    #[test]
    fn mixto_primero_del_efectivo() {
        // Total 170 (filtro 50 + afuera 120): 100 efectivo + 70 Yape.
        assert_eq!(descontar_del_mixto(100.0, 70.0, 120.0), (0.0, 50.0));
        // 150 efectivo + 20 Yape: alcanza con el efectivo.
        assert_eq!(descontar_del_mixto(150.0, 20.0, 120.0), (30.0, 20.0));
        // Exacto.
        assert_eq!(descontar_del_mixto(120.0, 50.0, 120.0), (0.0, 50.0));
    }
}
