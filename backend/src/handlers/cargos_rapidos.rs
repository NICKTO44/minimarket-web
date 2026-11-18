//! Módulo "Cargos rápidos" (CARGOS_RAPIDOS, migración 0070).
//!
//! Productos que salen como botón al cobrar: vasos, bolsas, el servicio de
//! madrugada. Cada uno puede llevar:
//! - `gratis_por_venta`: los primeros N de cada venta van gratis (el POS los
//!   pone como descuento de la línea: el stock baja por todos).
//! - `auto_desde` / `auto_hasta` ("HH:MM", hora de Perú): dentro de ese
//!   horario el POS lo agrega solo, una vez por venta.
//!
//! El cálculo vive en el POS; aquí solo se guarda la configuración.

use std::sync::Arc;

use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::middleware_auth::exigir_solo_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CargoRapido {
    pub producto_id: i64,
    #[serde(default)]
    pub gratis_por_venta: f64,
    #[serde(default)]
    pub auto_desde: Option<String>,
    #[serde(default)]
    pub auto_hasta: Option<String>,
}

/// "5:00" / "05:00" -> "05:00"; vacío -> None; otra cosa -> error.
pub fn hora(h: &Option<String>) -> Result<Option<String>, String> {
    let Some(t) = h.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    let (hh, mm) = t.split_once(':').ok_or_else(|| format!("\"{}\" no es una hora (usa HH:MM).", t))?;
    match (hh.parse::<u32>(), mm.parse::<u32>()) {
        (Ok(h), Ok(m)) if h < 24 && m < 60 && mm.len() == 2 => Ok(Some(format!("{:02}:{:02}", h, m))),
        _ => Err(format!("\"{}\" no es una hora (usa HH:MM).", t)),
    }
}

/// Revisa y normaliza la lista (sin repetidos, gratis >= 0, horario completo).
pub fn validar(lista: Vec<CargoRapido>) -> Result<Vec<CargoRapido>, String> {
    if lista.len() > 12 {
        return Err("Son demasiados cargos rápidos: hasta 12.".into());
    }
    let mut vistos = std::collections::HashSet::new();
    let mut salida = Vec::new();
    for c in lista {
        if !vistos.insert(c.producto_id) {
            return Err("Un producto está repetido en los cargos rápidos.".into());
        }
        if !c.gratis_por_venta.is_finite() || c.gratis_por_venta < 0.0 || c.gratis_por_venta > 1000.0 {
            return Err("Los gratis por venta deben ser un número de 0 a 1000.".into());
        }
        let desde = hora(&c.auto_desde)?;
        let hasta = hora(&c.auto_hasta)?;
        if desde.is_some() != hasta.is_some() {
            return Err("Para agregarlo solo, pon la hora de inicio y la de fin.".into());
        }
        if desde.is_some() && desde == hasta {
            return Err("La hora de inicio y la de fin no pueden ser iguales.".into());
        }
        salida.push(CargoRapido { producto_id: c.producto_id, gratis_por_venta: c.gratis_por_venta, auto_desde: desde, auto_hasta: hasta });
    }
    Ok(salida)
}

async fn leer(conn: &libsql::Connection) -> Result<Vec<CargoRapido>, Fallo> {
    let mut f = conn
        .query(
            "SELECT c.producto_id, CAST(c.gratis_por_venta AS REAL), c.auto_desde, c.auto_hasta
             FROM cargos_rapidos c JOIN productos p ON p.id = c.producto_id
             WHERE COALESCE(p.activo, 1) = 1 ORDER BY c.orden, c.producto_id",
            (),
        )
        .await
        .map_err(e500)?;
    let mut lista = Vec::new();
    while let Some(r) = f.next().await.map_err(e500)? {
        lista.push(CargoRapido {
            producto_id: r.get(0).unwrap_or(0),
            gratis_por_venta: r.get(1).unwrap_or(0.0),
            auto_desde: r.get(2).unwrap_or(None),
            auto_hasta: r.get(3).unwrap_or(None),
        });
    }
    Ok(lista)
}

/// GET /cargos-rapidos (lo usa el POS: cualquier usuario).
pub async fn listar(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<CargoRapido>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(leer(&conn).await?))
}

/// PUT /configuracion/cargos-rapidos (solo el administrador): reemplaza la lista.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(lista): Json<Vec<CargoRapido>>,
) -> Result<Json<Vec<CargoRapido>>, Fallo> {
    exigir_solo_admin(&claims)?;
    let lista = validar(lista).map_err(|m| (StatusCode::BAD_REQUEST, m))?;
    let conn = tenant.0.connect().map_err(e500)?;
    let tx = conn.transaction().await.map_err(e500)?;
    tx.execute("DELETE FROM cargos_rapidos", ()).await.map_err(e500)?;
    for (orden, c) in lista.iter().enumerate() {
        tx.execute(
            "INSERT INTO cargos_rapidos (producto_id, orden, gratis_por_venta, auto_desde, auto_hasta)
             SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM productos WHERE id = ?1)",
            libsql::params![c.producto_id, orden as i64, c.gratis_por_venta, c.auto_desde.clone(), c.auto_hasta.clone()],
        )
        .await
        .map_err(e500)?;
    }
    tx.commit().await.map_err(e500)?;
    Ok(Json(leer(&conn).await?))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn c(id: i64, gratis: f64, desde: Option<&str>, hasta: Option<&str>) -> CargoRapido {
        CargoRapido { producto_id: id, gratis_por_venta: gratis, auto_desde: desde.map(String::from), auto_hasta: hasta.map(String::from) }
    }

    #[test]
    fn horas_y_reglas() {
        assert_eq!(hora(&Some("0:00".into())).unwrap().as_deref(), Some("00:00"));
        assert_eq!(hora(&Some(" 05:00 ".into())).unwrap().as_deref(), Some("05:00"));
        assert_eq!(hora(&Some("".into())).unwrap(), None);
        assert!(hora(&Some("25:00".into())).is_err());
        assert!(hora(&Some("5".into())).is_err());
        let ok = validar(vec![c(1, 2.0, None, None), c(2, 0.0, Some("0:00"), Some("5:00"))]).unwrap();
        assert_eq!(ok[1].auto_desde.as_deref(), Some("00:00"));
        assert!(validar(vec![c(1, 0.0, None, None), c(1, 1.0, None, None)]).unwrap_err().contains("repetido"));
        assert!(validar(vec![c(1, -1.0, None, None)]).is_err());
        assert!(validar(vec![c(1, 0.0, Some("00:00"), None)]).unwrap_err().contains("fin"));
        assert!(validar(vec![c(1, 0.0, Some("05:00"), Some("5:00"))]).is_err());
    }
}
