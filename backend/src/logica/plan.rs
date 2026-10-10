//! Plan de Monspeet y comprobantes usados en el ciclo.
//!
//! Cada negocio paga una mensualidad que incluye un número de boletas y
//! facturas por ciclo. Si emite más, NO se le corta: sigue emitiendo y lo
//! extra se cobra en la siguiente mensualidad.
//!
//! El ciclo va del día de su vencimiento de un mes al mismo día del mes
//! siguiente (si vence el 9, su ciclo es del 9 al 8). Sin fecha de
//! vencimiento se usa el mes calendario.
//!
//! Se configura en el .env del backend (todos opcionales):
//!   PLAN_NOMBRE=Plan básico
//!   PLAN_PRECIO=49            (mensualidad, S/)
//!   PLAN_LIMITE=250           (boletas + facturas incluidas por ciclo)
//!   PLAN_PRECIO_EXTRA=0.20    (S/ por cada comprobante sobre el límite)
//!
//! Cuentan las boletas y facturas que se mandaron a SUNAT (aceptadas o aún
//! pendientes de respuesta), también las que después se anularon. No
//! cuentan las rechazadas, las que quedaron en error, las notas de crédito
//! ni las guías.

use chrono::{Datelike, Months, NaiveDate};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub nombre: String,
    pub precio: f64,
    pub limite: i64,
    pub precio_extra: f64,
}

fn numero_env(variable: &str, por_defecto: f64) -> f64 {
    std::env::var(variable)
        .ok()
        .and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or(por_defecto)
}

impl Plan {
    pub fn desde_env() -> Plan {
        let nombre = std::env::var("PLAN_NOMBRE").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Plan {
            nombre: nombre.unwrap_or_else(|| "Plan básico".to_string()),
            precio: numero_env("PLAN_PRECIO", 49.0),
            limite: numero_env("PLAN_LIMITE", 250.0).round() as i64,
            precio_extra: numero_env("PLAN_PRECIO_EXTRA", 0.20),
        }
    }
}

/// Suma `meses` (puede ser negativo) a `fecha`. Si el día no existe en ese
/// mes (31 de setiembre), queda en el último día del mes.
fn mover_meses(fecha: NaiveDate, meses: i64) -> NaiveDate {
    if meses >= 0 {
        fecha.checked_add_months(Months::new(meses as u32)).unwrap_or(fecha)
    } else {
        fecha.checked_sub_months(Months::new((-meses) as u32)).unwrap_or(fecha)
    }
}

/// Ciclo que contiene `hoy`: (primer día, primer día del siguiente ciclo).
/// Se calcula siempre desde la fecha de vencimiento (no paso a paso), así
/// un vencimiento el 31 no se va corriendo mes a mes.
pub fn ciclo(vencimiento: Option<NaiveDate>, hoy: NaiveDate) -> (NaiveDate, NaiveDate) {
    let ancla = match vencimiento {
        Some(v) => v,
        None => NaiveDate::from_ymd_opt(hoy.year(), hoy.month(), 1).unwrap_or(hoy),
    };
    let mut meses = (hoy.year() as i64 - ancla.year() as i64) * 12 + (hoy.month() as i64 - ancla.month() as i64);
    if mover_meses(ancla, meses) > hoy {
        meses -= 1;
    }
    (mover_meses(ancla, meses), mover_meses(ancla, meses + 1))
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct UsoPlan {
    pub plan: String,
    pub precio: f64,
    pub limite: i64,
    pub precio_extra: f64,
    /// Primer y último día del ciclo ("2026-10-09", "2026-11-08").
    pub ciclo_inicio: String,
    pub ciclo_fin: String,
    pub boletas: i64,
    pub facturas: i64,
    pub usados: i64,
    /// Comprobantes sobre el límite (0 si no pasó).
    pub extra: i64,
    pub monto_extra: f64,
    /// Mensualidad + extra: lo que pagaría en la siguiente renovación.
    pub total_siguiente: f64,
    /// 0–100 o más.
    pub porcentaje: f64,
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

pub fn calcular(plan: &Plan, inicio: NaiveDate, siguiente: NaiveDate, boletas: i64, facturas: i64) -> UsoPlan {
    let usados = boletas + facturas;
    let extra = (usados - plan.limite).max(0);
    let monto_extra = round2(extra as f64 * plan.precio_extra);
    let porcentaje = if plan.limite > 0 { round2(usados as f64 * 100.0 / plan.limite as f64) } else { 0.0 };
    UsoPlan {
        plan: plan.nombre.clone(),
        precio: plan.precio,
        limite: plan.limite,
        precio_extra: plan.precio_extra,
        ciclo_inicio: inicio.format("%Y-%m-%d").to_string(),
        ciclo_fin: siguiente.pred_opt().unwrap_or(siguiente).format("%Y-%m-%d").to_string(),
        boletas,
        facturas,
        usados,
        extra,
        monto_extra,
        total_siguiente: round2(plan.precio + monto_extra),
        porcentaje,
    }
}

/// Cuenta las boletas y facturas del ciclo actual en la base del negocio.
pub async fn uso_del_negocio(conn: &libsql::Connection, fecha_vencimiento: Option<&str>) -> Result<UsoPlan, String> {
    let hoy = NaiveDate::parse_from_str(&crate::logica::tiempo::hoy_lima(), "%Y-%m-%d").map_err(|e| e.to_string())?;
    let vence = fecha_vencimiento.and_then(|f| NaiveDate::parse_from_str(f.get(..10).unwrap_or(f), "%Y-%m-%d").ok());
    let (inicio, siguiente) = ciclo(vence, hoy);
    let mut filas = conn
        .query(
            "SELECT COALESCE(SUM(CASE WHEN tipo = 'BOLETA' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN tipo = 'FACTURA' THEN 1 ELSE 0 END), 0)
             FROM comprobantes_electronicos
             WHERE estado IN ('ACEPTADO', 'PENDIENTE')
               AND COALESCE(proveedor, 'MOCK') <> 'MOCK'
               AND substr(fecha_emision, 1, 10) >= ?1
               AND substr(fecha_emision, 1, 10) < ?2",
            libsql::params![inicio.format("%Y-%m-%d").to_string(), siguiente.format("%Y-%m-%d").to_string()],
        )
        .await
        .map_err(|e| e.to_string())?;
    let (boletas, facturas) = match filas.next().await.map_err(|e| e.to_string())? {
        Some(f) => (f.get::<i64>(0).unwrap_or(0), f.get::<i64>(1).unwrap_or(0)),
        None => (0, 0),
    };
    Ok(calcular(&Plan::desde_env(), inicio, siguiente, boletas, facturas))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn f(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn plan() -> Plan {
        Plan { nombre: "Plan básico".into(), precio: 49.0, limite: 250, precio_extra: 0.20 }
    }

    #[test]
    fn ciclo_desde_el_dia_de_vencimiento() {
        // Vence el 9-nov: hoy 10-oct está en el ciclo 9-oct → 8-nov.
        assert_eq!(ciclo(Some(f("2026-11-09")), f("2026-10-10")), (f("2026-10-09"), f("2026-11-09")));
        // El mismo día del vencimiento empieza el ciclo nuevo.
        assert_eq!(ciclo(Some(f("2026-11-09")), f("2026-11-09")), (f("2026-11-09"), f("2026-12-09")));
        // Un día antes sigue en el anterior.
        assert_eq!(ciclo(Some(f("2026-11-09")), f("2026-11-08")), (f("2026-10-09"), f("2026-11-09")));
    }

    #[test]
    fn ciclo_con_varios_meses_pagados_o_vencido() {
        // Pagó hasta marzo: el ciclo sigue siendo mensual, el de este mes.
        assert_eq!(ciclo(Some(f("2027-03-15")), f("2026-10-20")), (f("2026-10-15"), f("2026-11-15")));
        // Ya venció en agosto: los ciclos siguen corriendo el mismo día.
        assert_eq!(ciclo(Some(f("2026-08-05")), f("2026-10-04")), (f("2026-09-05"), f("2026-10-05")));
    }

    #[test]
    fn ciclo_fin_de_mes_no_se_corre() {
        // Vence el 31: en febrero queda en el 28, pero marzo vuelve al 31.
        assert_eq!(ciclo(Some(f("2026-12-31")), f("2027-03-10")), (f("2027-02-28"), f("2027-03-31")));
        assert_eq!(ciclo(Some(f("2026-12-31")), f("2027-03-31")), (f("2027-03-31"), f("2027-04-30")));
    }

    #[test]
    fn sin_vencimiento_es_el_mes_calendario() {
        assert_eq!(ciclo(None, f("2026-10-10")), (f("2026-10-01"), f("2026-11-01")));
    }

    #[test]
    fn dentro_del_plan_no_hay_extra() {
        let u = calcular(&plan(), f("2026-10-09"), f("2026-11-09"), 120, 12);
        assert_eq!((u.usados, u.extra, u.monto_extra, u.total_siguiente), (132, 0, 0.0, 49.0));
        assert_eq!(u.ciclo_fin, "2026-11-08");
        assert_eq!(u.porcentaje, 52.8);
    }

    #[tokio::test]
    async fn cuenta_solo_boletas_y_facturas_enviadas() {
        let db = libsql::Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            "CREATE TABLE comprobantes_electronicos (id INTEGER PRIMARY KEY, tipo TEXT, proveedor TEXT, estado TEXT, fecha_emision TEXT)",
            (),
        )
        .await
        .unwrap();
        let hoy = crate::logica::tiempo::hoy_lima();
        let filas = [
            ("BOLETA", "SUNAT_DIRECTO", "ACEPTADO", hoy.clone()),
            ("BOLETA", "SUNAT_DIRECTO", "PENDIENTE", hoy.clone()),
            ("FACTURA", "FACTURALIBRE", "ACEPTADO", format!("{} 10:00:00", hoy)),
            ("BOLETA", "SUNAT_DIRECTO", "RECHAZADO", hoy.clone()),
            ("BOLETA", "SUNAT_DIRECTO", "ERROR", hoy.clone()),
            ("BOLETA", "MOCK", "ACEPTADO", hoy.clone()),
            ("BOLETA", "SUNAT_DIRECTO", "ACEPTADO", "2020-01-01".to_string()),
        ];
        for (tipo, proveedor, estado, fecha) in filas {
            conn.execute(
                "INSERT INTO comprobantes_electronicos (tipo, proveedor, estado, fecha_emision) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![tipo, proveedor, estado, fecha],
            )
            .await
            .unwrap();
        }
        let u = uso_del_negocio(&conn, None).await.unwrap();
        assert_eq!((u.boletas, u.facturas, u.usados), (2, 1, 3));
    }

    #[test]
    fn sobre_el_plan_se_cobra_cada_extra() {
        let u = calcular(&plan(), f("2026-10-09"), f("2026-11-09"), 250, 18);
        assert_eq!((u.usados, u.extra), (268, 18));
        assert_eq!(u.monto_extra, 3.6);
        assert_eq!(u.total_siguiente, 52.6);
    }
}
