//! Módulo RECETAS (migración 0035): control de recetas en la venta de
//! medicamentos. Necesita el módulo FICHA_MEDICAMENTO, que dice qué
//! condición de venta tiene cada producto.
//!
//! - "Con receta médica": no se cobra sin que quien vende confirme que el
//!   cliente presentó la receta. Médico, colegiatura, paciente y demás son
//!   opcionales.
//! - "Receta médica retenida" (psicotrópicos y estupefacientes): no se
//!   cobra sin médico, colegiatura, paciente, documento y N.° de receta.
//!
//! La revisión se hace antes de la transacción de la venta (`preparar`) y la
//! constancia se guarda dentro de ella (`registrar`): si la venta falla, no
//! queda receta suelta; si la venta entra, entra con su receta.
//!
//! Rutas: GET /recetas?desde=&hasta= (libro de recetas, solo administrador).

use axum::{
    extract::{Extension, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_RECETAS};
use crate::logica::tiempo;
use crate::middleware_auth::exigir_admin;
use crate::models::auth::Claims;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Control de recetas";

/// Datos de la receta que manda el punto de venta.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecetaVenta {
    /// Quien vende confirma que el cliente presentó la receta.
    #[serde(default)]
    pub presentada: bool,
    #[serde(default)]
    pub medico_nombre: Option<String>,
    /// CMP del médico (o la colegiatura del odontólogo u obstetra).
    #[serde(default)]
    pub medico_colegiatura: Option<String>,
    #[serde(default)]
    pub paciente_nombre: Option<String>,
    #[serde(default)]
    pub paciente_documento: Option<String>,
    #[serde(default)]
    pub numero_receta: Option<String>,
    /// AAAA-MM-DD.
    #[serde(default)]
    pub fecha_receta: Option<String>,
}

/// Un medicamento de la venta que pide receta.
#[derive(Debug, Clone, PartialEq)]
pub struct ConReceta {
    pub producto_id: i64,
    pub nombre: String,
    pub condicion: String,
    pub cantidad: f64,
}

/// Lo revisado, listo para guardarse dentro de la transacción.
#[derive(Debug)]
pub struct RecetaLista {
    datos: RecetaVenta,
    retenida: bool,
    productos: Vec<ConReceta>,
    /// false = la base aún no tiene la 0035: se exige igual, pero no se guarda.
    guardar: bool,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Espacios de más fuera; vacío = None; recortado a `max` letras.
fn limpio(valor: &Option<String>, max: usize) -> Option<String> {
    valor
        .as_deref()
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|v| !v.is_empty())
        .map(|v| v.chars().take(max).collect())
}

fn lista_nombres(productos: &[ConReceta], condicion: &str) -> String {
    productos.iter().filter(|p| p.condicion == condicion).map(|p| format!("\"{}\"", p.nombre)).collect::<Vec<_>>().join(", ")
}

/// Revisa la receta contra los medicamentos que la piden. Devuelve los datos
/// limpios o el mensaje para quien vende. `hoy`: AAAA-MM-DD de Lima.
pub fn validar(receta: Option<&RecetaVenta>, productos: &[ConReceta], hoy: &str) -> Result<(RecetaVenta, bool), String> {
    let retenida = productos.iter().any(|p| p.condicion == "RECETA_RETENIDA");
    let receta = match receta {
        Some(r) if r.presentada => r,
        _ => {
            let nombres = productos.iter().map(|p| format!("\"{}\"", p.nombre)).collect::<Vec<_>>().join(", ");
            return Err(format!(
                "{} se vende con receta médica. Confirma que el cliente presentó la receta antes de cobrar.",
                nombres
            ));
        }
    };
    let datos = RecetaVenta {
        presentada: true,
        medico_nombre: limpio(&receta.medico_nombre, 100),
        medico_colegiatura: limpio(&receta.medico_colegiatura, 15).map(|c| c.to_uppercase()),
        paciente_nombre: limpio(&receta.paciente_nombre, 100),
        paciente_documento: limpio(&receta.paciente_documento, 15).map(|d| d.to_uppercase()),
        numero_receta: limpio(&receta.numero_receta, 30).map(|n| n.to_uppercase()),
        fecha_receta: limpio(&receta.fecha_receta, 10),
    };
    if let Some(fecha) = &datos.fecha_receta {
        if !tiempo::fecha_valida(fecha) {
            return Err("La fecha de la receta no es válida.".into());
        }
        if fecha.as_str() > hoy {
            return Err("La fecha de la receta no puede ser futura.".into());
        }
    }
    if let Some(c) = &datos.medico_colegiatura {
        if !c.chars().all(|x| x.is_ascii_alphanumeric() || x == '-') {
            return Err("La colegiatura del médico (CMP) solo lleva letras y números.".into());
        }
    }
    if let Some(d) = &datos.paciente_documento {
        if !d.chars().all(|x| x.is_ascii_alphanumeric()) {
            return Err("El documento del paciente solo lleva letras y números.".into());
        }
    }
    if retenida {
        let faltan: Vec<&str> = [
            (datos.medico_nombre.is_none(), "médico"),
            (datos.medico_colegiatura.is_none(), "CMP"),
            (datos.paciente_nombre.is_none(), "paciente"),
            (datos.paciente_documento.is_none(), "documento del paciente"),
            (datos.numero_receta.is_none(), "N.° de receta"),
        ]
        .iter()
        .filter(|(falta, _)| *falta)
        .map(|(_, campo)| *campo)
        .collect();
        if !faltan.is_empty() {
            return Err(format!(
                "{} es de receta retenida: completa {} antes de cobrar.",
                lista_nombres(productos, "RECETA_RETENIDA"),
                faltan.join(", ")
            ));
        }
    }
    Ok((datos, retenida))
}

/// true si la lista de módulos guardada ("FICHA_MEDICAMENTO,RECETAS") lo incluye.
pub fn encendido_en(modulos: &str) -> bool {
    modulos.split(',').any(|m| m.trim() == MODULO_RECETAS)
}

/// Antes de la venta: qué medicamentos del carrito piden receta y si la
/// receta que llegó alcanza. None = no hay nada que exigir (módulo apagado,
/// ningún medicamento con receta o base sin la ficha).
/// `pedidos`: (producto_id, cantidad) ya sumada por producto.
pub async fn preparar(
    conn: &libsql::Connection,
    modulos: Option<&str>,
    receta: Option<&RecetaVenta>,
    pedidos: &[(i64, f64)],
) -> Result<Option<RecetaLista>, Fallo> {
    if !modulos.is_some_and(encendido_en) || pedidos.is_empty() {
        return Ok(None);
    }
    let lista = pedidos.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
    let Ok(mut filas) = conn
        .query(
            &format!(
                "SELECT m.producto_id, p.nombre, m.condicion_venta,
                        (SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'venta_recetas')
                 FROM producto_medicamento m JOIN productos p ON p.id = m.producto_id
                 WHERE m.producto_id IN ({}) AND m.condicion_venta IN ('CON_RECETA', 'RECETA_RETENIDA')
                 ORDER BY p.nombre",
                lista
            ),
            (),
        )
        .await
    else {
        // Sin la 0034 no hay fichas: nada que exigir.
        return Ok(None);
    };
    let mut productos = Vec::new();
    let mut guardar = false;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let producto_id: i64 = f.get(0).unwrap_or_default();
        guardar = f.get::<i64>(3).unwrap_or(0) > 0;
        productos.push(ConReceta {
            producto_id,
            nombre: f.get(1).unwrap_or_default(),
            condicion: f.get(2).unwrap_or_default(),
            cantidad: pedidos.iter().find(|(id, _)| *id == producto_id).map(|(_, c)| *c).unwrap_or(0.0),
        });
    }
    if productos.is_empty() {
        return Ok(None);
    }
    let (datos, retenida) = validar(receta, &productos, &tiempo::hoy_lima()).map_err(malo)?;
    Ok(Some(RecetaLista { datos, retenida, productos, guardar }))
}

/// Dentro de la transacción de la venta: guarda la constancia.
pub async fn registrar(
    conn: &libsql::Connection,
    lista: &RecetaLista,
    venta_id: i64,
    usuario_id: i64,
    usuario_nombre: &str,
) -> Result<(), Fallo> {
    if !lista.guardar {
        return Ok(());
    }
    let d = &lista.datos;
    let mut filas = conn
        .query(
            "INSERT INTO venta_recetas (venta_id, retenida, medico_nombre, medico_colegiatura, paciente_nombre,
                paciente_documento, numero_receta, fecha_receta, usuario_id, usuario_nombre, fecha_hora)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) RETURNING id",
            libsql::params![
                venta_id,
                if lista.retenida { 1 } else { 0 },
                d.medico_nombre.clone(),
                d.medico_colegiatura.clone(),
                d.paciente_nombre.clone(),
                d.paciente_documento.clone(),
                d.numero_receta.clone(),
                d.fecha_receta.clone(),
                usuario_id,
                usuario_nombre.to_string(),
                tiempo::ahora_lima()
            ],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar la receta: {}", e)))?;
    let receta_id: i64 = match filas.next().await.map_err(e500)? {
        Some(f) => f.get(0).map_err(e500)?,
        None => return Err((StatusCode::INTERNAL_SERVER_ERROR, "No se pudo guardar la receta.".into())),
    };
    // Se lee la respuesta hasta el final antes de seguir (como en la venta).
    while filas.next().await.map_err(e500)?.is_some() {}
    let mut marcas = Vec::new();
    let mut valores: Vec<libsql::Value> = Vec::new();
    for (i, p) in lista.productos.iter().enumerate() {
        let b = i * 5;
        marcas.push(format!("(?{}, ?{}, ?{}, ?{}, ?{})", b + 1, b + 2, b + 3, b + 4, b + 5));
        valores.extend([
            libsql::Value::Integer(receta_id),
            libsql::Value::Integer(p.producto_id),
            libsql::Value::Text(p.nombre.clone()),
            libsql::Value::Text(p.condicion.clone()),
            libsql::Value::Real(p.cantidad),
        ]);
    }
    conn.execute(
        &format!(
            "INSERT INTO venta_receta_productos (receta_id, producto_id, producto_nombre, condicion_venta, cantidad) VALUES {}",
            marcas.join(", ")
        ),
        libsql::params_from_iter(valores),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("No se pudo guardar la receta: {}", e)))?;
    Ok(())
}

// ---------------------------------------------------------------------
// Libro de recetas
// ---------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FiltroLibro {
    pub desde: Option<String>,
    pub hasta: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProductoDeReceta {
    pub producto_nombre: String,
    pub condicion_venta: String,
    pub cantidad: f64,
}

#[derive(Debug, Serialize)]
pub struct RecetaDelLibro {
    pub id: i64,
    pub fecha_hora: String,
    pub venta_id: i64,
    pub folio: Option<String>,
    /// COMPLETADA o CANCELADA (anulada).
    pub estado_venta: Option<String>,
    pub retenida: bool,
    pub medico_nombre: Option<String>,
    pub medico_colegiatura: Option<String>,
    pub paciente_nombre: Option<String>,
    pub paciente_documento: Option<String>,
    pub numero_receta: Option<String>,
    pub fecha_receta: Option<String>,
    pub usuario_nombre: Option<String>,
    pub productos: Vec<ProductoDeReceta>,
    /// La venta, para el detalle: total, cómo se pagó, a quién y su comprobante.
    pub total: Option<f64>,
    pub metodo_pago: Option<String>,
    pub motivo_anulacion: Option<String>,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub comprobante: Option<ComprobanteDeReceta>,
}

#[derive(Debug, Serialize)]
pub struct ComprobanteDeReceta {
    pub id: i64,
    /// BOLETA o FACTURA
    pub tipo: String,
    pub serie: Option<String>,
    pub numero: Option<i64>,
    /// PENDIENTE, ACEPTADO, RECHAZADO o ERROR
    pub estado: String,
}

#[derive(Debug, Serialize)]
pub struct LibroRecetas {
    pub desde: String,
    pub hasta: String,
    /// false = la base aún no tiene la migración 0035.
    pub disponible: bool,
    pub recetas: Vec<RecetaDelLibro>,
}

/// GET /recetas?desde=AAAA-MM-DD&hasta=AAAA-MM-DD — libro de recetas. Solo
/// el administrador (tiene datos de pacientes). Sin fechas: el mes en curso.
pub async fn libro(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Query(filtro): Query<FiltroLibro>,
) -> Result<Json<LibroRecetas>, Fallo> {
    exigir_admin(&claims)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir_modulo(&conn, MODULO_RECETAS, NOMBRE_MODULO).await?;
    let hoy = tiempo::hoy_lima();
    let fecha = |v: &Option<String>| v.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let desde = fecha(&filtro.desde).unwrap_or_else(|| format!("{}-01", &hoy[..7]));
    let hasta = fecha(&filtro.hasta).unwrap_or_else(|| hoy.clone());
    if !tiempo::fecha_valida(&desde) || !tiempo::fecha_valida(&hasta) {
        return Err(malo("Fecha no válida."));
    }
    if desde > hasta {
        return Err(malo("La fecha inicial es posterior a la final."));
    }

    let mut recetas: Vec<RecetaDelLibro> = Vec::new();
    let Ok(mut filas) = conn
        .query(
            "SELECT r.id, r.fecha_hora, r.venta_id, v.folio, v.estado, r.retenida, r.medico_nombre, r.medico_colegiatura,
                    r.paciente_nombre, r.paciente_documento, r.numero_receta, r.fecha_receta, r.usuario_nombre,
                    CAST(v.total AS REAL), v.metodo_pago, v.motivo_cancelacion,
                    COALESCE(NULLIF(ce.cliente_nombre, ''), c.nombre_razon_social),
                    COALESCE(NULLIF(ce.cliente_documento, ''), c.numero_documento),
                    ce.id, ce.tipo, ce.serie, ce.numero, ce.estado
             FROM venta_recetas r
             LEFT JOIN ventas v ON v.id = r.venta_id
             LEFT JOIN clientes c ON c.id = v.cliente_id
             LEFT JOIN comprobantes_electronicos ce
                    ON ce.id = (SELECT MAX(x.id) FROM comprobantes_electronicos x WHERE x.venta_id = r.venta_id)
             WHERE substr(r.fecha_hora, 1, 10) BETWEEN ?1 AND ?2
             ORDER BY r.fecha_hora DESC, r.id DESC",
            libsql::params![desde.clone(), hasta.clone()],
        )
        .await
    else {
        return Ok(Json(LibroRecetas { desde, hasta, disponible: false, recetas }));
    };
    while let Some(f) = filas.next().await.map_err(e500)? {
        recetas.push(RecetaDelLibro {
            id: f.get(0).unwrap_or_default(),
            fecha_hora: f.get(1).unwrap_or_default(),
            venta_id: f.get(2).unwrap_or_default(),
            folio: f.get(3).ok(),
            estado_venta: f.get(4).ok(),
            retenida: f.get::<i64>(5).unwrap_or(0) == 1,
            medico_nombre: f.get(6).ok(),
            medico_colegiatura: f.get(7).ok(),
            paciente_nombre: f.get(8).ok(),
            paciente_documento: f.get(9).ok(),
            numero_receta: f.get(10).ok(),
            fecha_receta: f.get(11).ok(),
            usuario_nombre: f.get(12).ok(),
            productos: Vec::new(),
            total: f.get(13).ok(),
            metodo_pago: f.get(14).ok(),
            motivo_anulacion: f.get(15).ok(),
            cliente_nombre: f.get(16).ok(),
            cliente_documento: f.get(17).ok(),
            comprobante: f.get::<i64>(18).ok().map(|id| ComprobanteDeReceta {
                id,
                tipo: f.get(19).unwrap_or_default(),
                serie: f.get(20).ok(),
                numero: f.get(21).ok(),
                estado: f.get(22).unwrap_or_default(),
            }),
        });
    }
    drop(filas);

    if !recetas.is_empty() {
        let ids = recetas.iter().map(|r| r.id.to_string()).collect::<Vec<_>>().join(",");
        let mut filas = conn
            .query(
                &format!(
                    "SELECT receta_id, producto_nombre, condicion_venta, CAST(cantidad AS REAL)
                     FROM venta_receta_productos WHERE receta_id IN ({}) ORDER BY id",
                    ids
                ),
                (),
            )
            .await
            .map_err(e500)?;
        while let Some(f) = filas.next().await.map_err(e500)? {
            let receta_id: i64 = f.get(0).unwrap_or_default();
            if let Some(r) = recetas.iter_mut().find(|r| r.id == receta_id) {
                r.productos.push(ProductoDeReceta {
                    producto_nombre: f.get(1).unwrap_or_default(),
                    condicion_venta: f.get(2).unwrap_or_default(),
                    cantidad: f.get(3).unwrap_or(0.0),
                });
            }
        }
    }

    Ok(Json(LibroRecetas { desde, hasta, disponible: true, recetas }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn producto(nombre: &str, condicion: &str) -> ConReceta {
        ConReceta { producto_id: 1, nombre: nombre.into(), condicion: condicion.into(), cantidad: 1.0 }
    }

    fn receta_completa() -> RecetaVenta {
        RecetaVenta {
            presentada: true,
            medico_nombre: Some(" Dra.  Ana Quispe ".into()),
            medico_colegiatura: Some("cmp-12345".into()),
            paciente_nombre: Some("Juan Pérez".into()),
            paciente_documento: Some("12345678".into()),
            numero_receta: Some("r-001".into()),
            fecha_receta: Some("2026-10-10".into()),
        }
    }

    #[test]
    fn con_receta_pide_solo_la_confirmacion() {
        let productos = [producto("Amoxicilina 500 mg", "CON_RECETA")];
        assert!(validar(None, &productos, "2026-10-17").is_err());
        let sin_confirmar = RecetaVenta { presentada: false, ..receta_completa() };
        assert!(validar(Some(&sin_confirmar), &productos, "2026-10-17").is_err());
        let solo_confirmada = RecetaVenta { presentada: true, ..Default::default() };
        let (datos, retenida) = validar(Some(&solo_confirmada), &productos, "2026-10-17").unwrap();
        assert!(!retenida);
        assert!(datos.medico_nombre.is_none());
    }

    #[test]
    fn retenida_pide_todos_los_datos() {
        let productos = [producto("Clonazepam 2 mg", "RECETA_RETENIDA"), producto("Amoxicilina", "CON_RECETA")];
        let incompleta = RecetaVenta { numero_receta: None, ..receta_completa() };
        let error = validar(Some(&incompleta), &productos, "2026-10-17").unwrap_err();
        assert!(error.contains("Clonazepam") && error.contains("N.° de receta") && !error.contains("Amoxicilina"));
        let (datos, retenida) = validar(Some(&receta_completa()), &productos, "2026-10-17").unwrap();
        assert!(retenida);
        assert_eq!(datos.medico_nombre.as_deref(), Some("Dra. Ana Quispe"));
        assert_eq!(datos.medico_colegiatura.as_deref(), Some("CMP-12345"));
        assert_eq!(datos.numero_receta.as_deref(), Some("R-001"));
    }

    #[test]
    fn fechas_y_formatos() {
        let productos = [producto("Amoxicilina", "CON_RECETA")];
        let futura = RecetaVenta { fecha_receta: Some("2026-10-18".into()), ..receta_completa() };
        assert!(validar(Some(&futura), &productos, "2026-10-17").is_err());
        let mal_documento = RecetaVenta { paciente_documento: Some("123.456".into()), ..receta_completa() };
        assert!(validar(Some(&mal_documento), &productos, "2026-10-17").is_err());
    }

    #[test]
    fn modulo_en_la_lista() {
        assert!(encendido_en("FICHA_MEDICAMENTO, RECETAS"));
        assert!(!encendido_en("FICHA_MEDICAMENTO,RECETAS_X"));
    }
}
