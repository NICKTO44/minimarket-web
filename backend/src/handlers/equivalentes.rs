//! Módulo EQUIVALENTES (migración 0058): el dueño elige a mano qué productos
//! son equivalentes (resuelven lo mismo). Pensado para el rubro Lubricentro /
//! Repuestos; sirve a cualquier negocio que lo encienda.
//!
//! Los equivalentes forman un GRUPO: si A es equivalente de B y B de C, los
//! tres lo son entre sí, en los dos sentidos. Al agregar un producto se une
//! su grupo; al quitarlo sale del grupo. Nada de esto cambia cómo se vende,
//! cobra o descuenta stock: el punto de venta y Productos solo los muestran.
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET /equivalentes                 -> [{ producto_id, grupo }] de productos activos
//!   PUT /productos/:id/equivalentes   -> { ids } = los equivalentes de ese producto

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_EQUIVALENTES};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Equivalentes";
/// Tope de productos en un grupo (de sobra para repuestos).
pub const MAXIMO_GRUPO: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Equivalente {
    pub producto_id: i64,
    pub grupo: i64,
}

#[derive(Debug, Deserialize)]
pub struct EquivalentesDe {
    #[serde(default)]
    pub ids: Vec<i64>,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn sin_migracion<E: std::fmt::Display>(e: E) -> Fallo {
    let texto = e.to_string();
    if texto.contains("no such table") {
        (StatusCode::SERVICE_UNAVAILABLE, "Los equivalentes aún no están listos en este negocio. Intenta en unos minutos.".into())
    } else {
        e500(texto)
    }
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_EQUIVALENTES, NOMBRE_MODULO).await
}

/// Cómo queda el grupo del producto `id` si sus equivalentes pasan a ser
/// `elegidos`. `grupos` = grupo actual de cada producto que está en uno.
/// Devuelve (los que quedan en el grupo, los que salen de él). Puro: se
/// prueba sin base.
pub fn nuevo_grupo(id: i64, elegidos: &[i64], grupos: &HashMap<i64, i64>) -> (BTreeSet<i64>, BTreeSet<i64>) {
    let miembros_de = |g: i64| grupos.iter().filter(move |(_, gg)| **gg == g).map(|(p, _)| *p);
    let antes: BTreeSet<i64> = grupos.get(&id).map(|g| miembros_de(*g).collect()).unwrap_or_default();
    let elegidos: BTreeSet<i64> = elegidos.iter().copied().filter(|p| *p != id).collect();
    let mut queda: BTreeSet<i64> = BTreeSet::from([id]);
    for p in &elegidos {
        queda.insert(*p);
        // Lo que ya era equivalente de un elegido (de otro grupo) también lo es ahora.
        if let Some(g) = grupos.get(p) {
            if Some(g) != grupos.get(&id) {
                queda.extend(miembros_de(*g));
            }
        }
    }
    // Los del grupo de antes que ya no están elegidos salen.
    let salen: BTreeSet<i64> = antes.iter().copied().filter(|p| *p != id && !elegidos.contains(p)).collect();
    for p in &salen {
        queda.remove(p);
    }
    if queda.len() < 2 {
        queda.clear();
    }
    let mut salen = salen;
    if queda.is_empty() && antes.contains(&id) {
        salen.insert(id);
    }
    (queda, salen)
}

async fn leer(conn: &libsql::Connection) -> Result<Vec<Equivalente>, Fallo> {
    let mut filas = conn
        .query(
            "SELECT e.producto_id, e.grupo FROM producto_equivalente e JOIN productos p ON p.id = e.producto_id
             WHERE p.activo = 1 ORDER BY e.grupo, e.producto_id",
            (),
        )
        .await
        .map_err(sin_migracion)?;
    let mut lista = Vec::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        lista.push(Equivalente { producto_id: f.get(0).unwrap_or_default(), grupo: f.get(1).unwrap_or_default() });
    }
    // Un grupo con un solo producto activo no muestra nada.
    let mut cuenta: HashMap<i64, usize> = HashMap::new();
    for e in &lista {
        *cuenta.entry(e.grupo).or_default() += 1;
    }
    lista.retain(|e| cuenta.get(&e.grupo).copied().unwrap_or(0) >= 2);
    Ok(lista)
}

/// GET /equivalentes
pub async fn todos(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<Equivalente>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer(&conn).await?))
}

/// PUT /productos/:id/equivalentes — `ids` son TODOS los equivalentes que
/// debe tener el producto (vacío = no tiene). Devuelve la lista completa.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(p): Json<EquivalentesDe>,
) -> Result<Json<Vec<Equivalente>>, Fallo> {
    let elegidos: BTreeSet<i64> = p.ids.iter().copied().filter(|x| *x != id).collect();
    if elegidos.len() >= MAXIMO_GRUPO {
        return Err(malo(format!("Máximo {} equivalentes por producto.", MAXIMO_GRUPO - 1)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;

    // El producto y los elegidos deben existir y estar activos.
    let todos_ids: Vec<String> = std::iter::once(id).chain(elegidos.iter().copied()).map(|x| x.to_string()).collect();
    let mut filas = conn
        .query(&format!("SELECT id FROM productos WHERE activo = 1 AND id IN ({})", todos_ids.join(",")), ())
        .await
        .map_err(e500)?;
    let mut activos = BTreeSet::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        activos.insert(f.get::<i64>(0).unwrap_or_default());
    }
    drop(filas);
    if !activos.contains(&id) {
        return Err((StatusCode::NOT_FOUND, "El producto no existe o está desactivado.".into()));
    }
    if elegidos.iter().any(|x| !activos.contains(x)) {
        return Err(malo("Uno de los equivalentes ya no existe o está desactivado. Vuelve a cargar la pantalla."));
    }

    let tx = conn.transaction().await.map_err(e500)?;
    let hecho: Result<(), Fallo> = async {
        let mut grupos: HashMap<i64, i64> = HashMap::new();
        {
            let mut filas = tx.query("SELECT producto_id, grupo FROM producto_equivalente", ()).await.map_err(sin_migracion)?;
            while let Some(f) = filas.next().await.map_err(e500)? {
                grupos.insert(f.get(0).unwrap_or_default(), f.get(1).unwrap_or_default());
            }
        }
        let (queda, salen) = nuevo_grupo(id, &elegidos.iter().copied().collect::<Vec<_>>(), &grupos);
        if queda.len() > MAXIMO_GRUPO {
            return Err(malo(format!("Un grupo de equivalentes puede tener hasta {} productos.", MAXIMO_GRUPO)));
        }
        if !salen.is_empty() {
            let lista = salen.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            tx.execute(&format!("DELETE FROM producto_equivalente WHERE producto_id IN ({})", lista), ()).await.map_err(e500)?;
            // Un grupo viejo que quedó con un solo producto se deshace.
            tx.execute(
                "DELETE FROM producto_equivalente WHERE grupo IN
                   (SELECT grupo FROM producto_equivalente GROUP BY grupo HAVING COUNT(*) < 2)",
                (),
            )
            .await
            .map_err(e500)?;
        }
        if let Some(grupo) = queda.iter().next().copied() {
            let ahora = tiempo::ahora_lima();
            let mut marcas = Vec::with_capacity(queda.len());
            let mut valores: Vec<libsql::Value> = Vec::with_capacity(queda.len() * 3);
            for (i, p) in queda.iter().enumerate() {
                marcas.push(format!("(?{}, ?{}, ?{})", i * 3 + 1, i * 3 + 2, i * 3 + 3));
                valores.extend([libsql::Value::Integer(*p), libsql::Value::Integer(grupo), libsql::Value::Text(ahora.clone())]);
            }
            tx.execute(
                &format!(
                    "INSERT INTO producto_equivalente (producto_id, grupo, fecha_actualizacion) VALUES {}
                     ON CONFLICT(producto_id) DO UPDATE SET grupo = excluded.grupo, fecha_actualizacion = excluded.fecha_actualizacion",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(e500)?;
            // Grupos que se unieron a este quedan vacíos o con uno solo: se limpian.
            tx.execute(
                "DELETE FROM producto_equivalente WHERE grupo IN
                   (SELECT grupo FROM producto_equivalente GROUP BY grupo HAVING COUNT(*) < 2)",
                (),
            )
            .await
            .map_err(e500)?;
        }
        Ok(())
    }
    .await;
    match hecho {
        Ok(()) => tx.commit().await.map_err(e500)?,
        Err(e) => {
            let _ = tx.rollback().await;
            return Err(e);
        }
    }
    Ok(Json(leer(&conn).await?))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn g(pares: &[(i64, i64)]) -> HashMap<i64, i64> {
        pares.iter().copied().collect()
    }

    #[test]
    fn agregar_a_uno_sin_grupo() {
        let (queda, salen) = nuevo_grupo(1, &[2, 3], &g(&[]));
        assert_eq!(queda, BTreeSet::from([1, 2, 3]));
        assert!(salen.is_empty());
    }

    #[test]
    fn agregar_uno_que_ya_tiene_grupo_trae_a_todos() {
        // 2 y 5 ya eran equivalentes: al agregar 2 a 1, también entra 5.
        let (queda, _) = nuevo_grupo(1, &[2], &g(&[(2, 2), (5, 2)]));
        assert_eq!(queda, BTreeSet::from([1, 2, 5]));
    }

    #[test]
    fn quitar_uno_lo_saca_del_grupo() {
        let (queda, salen) = nuevo_grupo(1, &[2], &g(&[(1, 1), (2, 1), (3, 1)]));
        assert_eq!(queda, BTreeSet::from([1, 2]));
        assert_eq!(salen, BTreeSet::from([3]));
    }

    #[test]
    fn quitar_todos_deshace_el_grupo() {
        let (queda, salen) = nuevo_grupo(1, &[], &g(&[(1, 1), (2, 1)]));
        assert!(queda.is_empty());
        assert_eq!(salen, BTreeSet::from([1, 2]));
    }

    #[test]
    fn el_mismo_producto_no_es_su_equivalente() {
        let (queda, _) = nuevo_grupo(1, &[1], &g(&[]));
        assert!(queda.is_empty());
    }
}
