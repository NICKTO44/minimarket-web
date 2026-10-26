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
//! Prueba gratis: lo que el negocio emitió en sus días de prueba se suma a
//! su PRIMER ciclo pagado (ese ciclo cuenta desde el día en que se
//! registró). Desde el segundo ciclo cuenta normal. Ver `ciclo_con_prueba`.
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

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime};
use serde::Serialize;

/// Días de prueba gratis de un negocio nuevo (los usa también el registro).
pub const DIAS_PRUEBA_GRATIS: i64 = 15;

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

/// Día de registro en Perú a partir de `tiendas.fecha_creacion`, que la base
/// central guarda en UTC ("2026-10-09 21:29:23" -> 9 de octubre en Lima).
pub fn registro_en_lima(fecha_creacion: &str) -> Option<NaiveDate> {
    let texto = fecha_creacion.trim();
    if let Ok(utc) = NaiveDateTime::parse_from_str(texto.get(..19).unwrap_or(texto), "%Y-%m-%d %H:%M:%S") {
        return Some((utc - Duration::hours(5)).date());
    }
    NaiveDate::parse_from_str(texto.get(..10).unwrap_or(texto), "%Y-%m-%d").ok()
}

/// Ciclo que contiene `hoy`, con la prueba gratis: si este es el primer
/// ciclo pagado (el ciclo anterior empezó antes de que terminara la prueba),
/// cuenta desde el día de registro, para que lo emitido en la prueba se sume
/// a este ciclo. Devuelve (primer día, primer día del siguiente ciclo, si
/// incluye la prueba).
///
/// Solo para negocios con vencimiento: sin vencimiento (demos) se cuenta el
/// mes calendario, como siempre.
pub fn ciclo_con_prueba(
    vencimiento: Option<NaiveDate>,
    registro: Option<NaiveDate>,
    hoy: NaiveDate,
) -> (NaiveDate, NaiveDate, bool) {
    let (inicio, siguiente) = ciclo(vencimiento, hoy);
    if let (Some(_), Some(registro)) = (vencimiento, registro) {
        let fin_prueba = registro + Duration::days(DIAS_PRUEBA_GRATIS);
        let inicio_anterior = mover_meses(inicio, -1);
        // registro < inicio: la prueba quedó antes de este ciclo (durante la
        // prueba el ciclo ya la incluye). inicio_anterior < fin_prueba: no
        // hubo un ciclo pagado completo entre la prueba y este.
        if registro < inicio && inicio_anterior < fin_prueba {
            return (registro, siguiente, true);
        }
    }
    (inicio, siguiente, false)
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
    /// true en el primer ciclo pagado: ciclo_inicio es el día de registro
    /// y el conteo incluye lo emitido en la prueba gratis.
    pub incluye_prueba: bool,
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
        incluye_prueba: false,
    }
}

/// Cuenta las boletas y facturas del ciclo actual en la base del negocio.
/// `fecha_creacion` es la de `tiendas` (base central, UTC); si no se tiene,
/// se cuenta el ciclo sin la prueba, como antes.
pub async fn uso_del_negocio(
    conn: &libsql::Connection,
    fecha_vencimiento: Option<&str>,
    fecha_creacion: Option<&str>,
) -> Result<UsoPlan, String> {
    let hoy = NaiveDate::parse_from_str(&crate::logica::tiempo::hoy_lima(), "%Y-%m-%d").map_err(|e| e.to_string())?;
    let vence = fecha_vencimiento.and_then(|f| NaiveDate::parse_from_str(f.get(..10).unwrap_or(f), "%Y-%m-%d").ok());
    let registro = fecha_creacion.and_then(registro_en_lima);
    let (inicio, siguiente, incluye_prueba) = ciclo_con_prueba(vence, registro, hoy);
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
    let mut uso = calcular(&Plan::desde_env(), inicio, siguiente, boletas, facturas);
    uso.incluye_prueba = incluye_prueba;
    Ok(uso)
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
        let u = uso_del_negocio(&conn, None, None).await.unwrap();
        assert_eq!((u.boletas, u.facturas, u.usados), (2, 1, 3));
        assert!(!u.incluye_prueba);
    }

    #[test]
    fn registro_en_hora_de_peru() {
        // 21:29 UTC del 9 = 16:29 en Lima del 9.
        assert_eq!(registro_en_lima("2026-10-09 21:29:23"), Some(f("2026-10-09")));
        // 03:44 UTC del 14 = 22:44 en Lima del 13.
        assert_eq!(registro_en_lima("2026-10-14 03:44:37"), Some(f("2026-10-13")));
        assert_eq!(registro_en_lima("2026-10-09"), Some(f("2026-10-09")));
        assert_eq!(registro_en_lima("basura"), None);
    }

    /// VERANE: se registró el 9-oct, la prueba terminó el 24 y renovó
    /// (ya vencida) hasta el 25-nov.
    #[test]
    fn primer_ciclo_pagado_suma_la_prueba() {
        let vence = Some(f("2026-11-25"));
        let registro = Some(f("2026-10-09"));
        // Primer ciclo pagado (25-oct al 24-nov): cuenta desde el 9-oct.
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-10-26")), (f("2026-10-09"), f("2026-11-25"), true));
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-11-24")), (f("2026-10-09"), f("2026-11-25"), true));
        // Segundo ciclo (renovó otro mes): normal, del 25 al 24.
        let vence = Some(f("2026-12-25"));
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-11-25")), (f("2026-11-25"), f("2026-12-25"), false));
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-12-10")), (f("2026-11-25"), f("2026-12-25"), false));
    }

    /// La Cabañita: se registró el 13-oct (prueba hasta el 28) y pagó antes
    /// de que terminara, hasta el 28-nov.
    #[test]
    fn pago_antes_de_terminar_la_prueba() {
        let vence = Some(f("2026-11-28"));
        let registro = Some(f("2026-10-13"));
        // Todavía en la prueba: el ciclo (28-set al 27-oct) ya la incluye.
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-10-26")), (f("2026-09-28"), f("2026-10-28"), false));
        // Primer ciclo pagado (28-oct al 27-nov): desde el 13-oct.
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-10-28")), (f("2026-10-13"), f("2026-11-28"), true));
        // Segundo ciclo: normal.
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-11-28")), (f("2026-11-28"), f("2026-12-28"), false));
    }

    /// Renovó varios días después de que venciera la prueba (en modo
    /// lectura no pudo emitir): el primer ciclo igual suma la prueba.
    #[test]
    fn renovo_tarde() {
        let registro = Some(f("2026-10-09"));
        // Prueba hasta el 24-oct; renovó el 14-nov hasta el 14-dic.
        let vence = Some(f("2026-12-14"));
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-11-20")), (f("2026-10-09"), f("2026-12-14"), true));
        // Su segundo ciclo, normal.
        let vence = Some(f("2027-01-14"));
        assert_eq!(ciclo_con_prueba(vence, registro, f("2026-12-14")), (f("2026-12-14"), f("2027-01-14"), false));
    }

    #[test]
    fn sin_vencimiento_o_sin_registro_no_cambia() {
        // Demo (sin vencimiento): mes calendario, aunque se registrara hace poco.
        assert_eq!(
            ciclo_con_prueba(None, Some(f("2026-09-20")), f("2026-10-10")),
            (f("2026-10-01"), f("2026-11-01"), false)
        );
        // Sin fecha de registro: como antes.
        assert_eq!(
            ciclo_con_prueba(Some(f("2026-11-25")), None, f("2026-10-26")),
            (f("2026-10-25"), f("2026-11-25"), false)
        );
        // Negocio antiguo (registrado hace meses): ciclo normal.
        assert_eq!(
            ciclo_con_prueba(Some(f("2026-11-25")), Some(f("2026-06-01")), f("2026-10-26")),
            (f("2026-10-25"), f("2026-11-25"), false)
        );
    }

    /// Lo emitido en la prueba entra al conteo del primer ciclo pagado, y lo
    /// de antes del registro (no debería existir) no.
    #[tokio::test]
    async fn conteo_con_la_prueba() {
        let db = libsql::Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            "CREATE TABLE comprobantes_electronicos (id INTEGER PRIMARY KEY, tipo TEXT, proveedor TEXT, estado TEXT, fecha_emision TEXT)",
            (),
        )
        .await
        .unwrap();
        let hoy = NaiveDate::parse_from_str(&crate::logica::tiempo::hoy_lima(), "%Y-%m-%d").unwrap();
        let dia = |n: i64| (hoy + Duration::days(n)).format("%Y-%m-%d").to_string();
        // Se registró hace 17 días (prueba hasta hace 2) y renovó ayer por un mes.
        let registro = dia(-17);
        let vence = (mover_meses(hoy, 1) - Duration::days(1)).format("%Y-%m-%d").to_string();
        for (tipo, fecha) in [
            ("FACTURA", dia(-16)),
            ("BOLETA", dia(-10)),
            ("BOLETA", format!("{} 18:00:00", dia(-3))),
            ("BOLETA", dia(0)),
            ("BOLETA", dia(-40)),
        ] {
            conn.execute(
                "INSERT INTO comprobantes_electronicos (tipo, proveedor, estado, fecha_emision) VALUES (?1, 'SUNAT_DIRECTO', 'ACEPTADO', ?2)",
                libsql::params![tipo, fecha],
            )
            .await
            .unwrap();
        }
        let sin = uso_del_negocio(&conn, Some(&vence), None).await.unwrap();
        assert_eq!(sin.usados, 1, "sin la prueba: solo el de hoy");
        let con = uso_del_negocio(&conn, Some(&vence), Some(&format!("{} 15:00:00", registro))).await.unwrap();
        assert_eq!((con.boletas, con.facturas, con.usados), (3, 1, 4));
        assert!(con.incluye_prueba);
        assert_eq!(con.ciclo_inicio, registro);
    }

    #[test]
    fn sobre_el_plan_se_cobra_cada_extra() {
        let u = calcular(&plan(), f("2026-10-09"), f("2026-11-09"), 250, 18);
        assert_eq!((u.usados, u.extra), (268, 18));
        assert_eq!(u.monto_extra, 3.6);
        assert_eq!(u.total_siguiente, 52.6);
    }
}
