//! Ingreso rápido de productos (POST /compras/rapida, módulo INGRESO_RAPIDO):
//! productos nuevos o que ya existen entran al stock en un solo paso, sin
//! elegir proveedor. Sin el módulo, la ruta responde 403.
//!
//! Por dentro es una compra de siempre, al proveedor "COMPRA RÁPIDA" (se
//! crea solo, sin RUC ni DNI), ya recibida: sale en Compras, en los
//! reportes y, con el módulo de ganancias, promedia el costo. Usa los mismos
//! handlers que la pantalla de Proveedores (crear_compra y
//! recibir_mercaderia) para no tener dos maneras de hacer lo mismo.
//!
//! - Producto nuevo: nombre, unidad, categoría, precio de venta y, si no lo
//!   escriben o escanean, un código de 8 dígitos que no se repite. Entra sin
//!   vencimiento (si es perecible se marca después en Productos).
//! - Producto que ya existe: solo suma stock y actualiza su precio de
//!   compra. Si es perecible, necesita la fecha de vencimiento del lote.
//! - Por caja (9 botellas): el navegador manda la cantidad en unidades (18)
//!   y el costo por unidad. Con el módulo Presentaciones, un producto nuevo
//!   puede crear además su presentación "Caja x9" para venderla entera.

use axum::{extract::Extension, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::models::auth::Claims;
use crate::models::producto::NuevoProducto;
use crate::models::proveedor::{ItemCompra, ItemRecepcion, NuevaCompraRequest, RecibirMercaderiaRequest};
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

/// Proveedor de los ingresos rápidos (uno por negocio, se crea solo).
pub const PROVEEDOR_RAPIDO: &str = "COMPRA RÁPIDA";
const MAX_ITEMS: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ProductoNuevo {
    pub nombre: String,
    pub unidad_medida: String,
    pub categoria_id: i64,
    /// Precio de venta por unidad.
    pub precio: f64,
    /// Vacío: se genera uno.
    #[serde(default)]
    pub codigo: Option<String>,
}

/// Presentación para vender la caja entera (módulo Presentaciones).
#[derive(Debug, Deserialize)]
pub struct CajaParaVender {
    /// Unidades que trae la caja (9).
    pub unidades: f64,
    /// Precio de venta de la caja entera.
    pub precio: f64,
}

#[derive(Debug, Deserialize)]
pub struct ItemRapido {
    /// Un producto que ya existe (reponer)...
    #[serde(default)]
    pub producto_id: Option<i64>,
    /// ...o uno nuevo. Exactamente uno de los dos.
    #[serde(default)]
    pub nuevo: Option<ProductoNuevo>,
    /// En unidades del producto (2 cajas de 9 = 18).
    pub cantidad: f64,
    /// Costo por unidad.
    pub precio_compra: f64,
    /// Solo para un producto que ya existe y es perecible.
    #[serde(default)]
    pub fecha_vencimiento: Option<String>,
    #[serde(default)]
    pub vender_caja: Option<CajaParaVender>,
}

#[derive(Debug, Deserialize)]
pub struct IngresoRapido {
    pub items: Vec<ItemRapido>,
    /// EFECTIVO_CAJA (de la caja abierta), EFECTIVO (de otro lado) o TRANSFERENCIA.
    pub tipo_pago: String,
    #[serde(default)]
    pub notas: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResultadoIngreso {
    pub compra_id: i64,
    pub folio: String,
    pub creados: usize,
    pub repuestos: usize,
    pub mensaje: String,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}

fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Revisa todo lo que se puede revisar sin tocar la base: si algo está mal,
/// no se guarda nada.
pub fn validar(ingreso: &IngresoRapido, con_presentaciones: bool) -> Result<(), String> {
    if ingreso.items.is_empty() {
        return Err("Agrega al menos un producto.".into());
    }
    if ingreso.items.len() > MAX_ITEMS {
        return Err(format!("Son muchos productos de una vez: hasta {} por ingreso.", MAX_ITEMS));
    }
    if !["EFECTIVO_CAJA", "EFECTIVO", "TRANSFERENCIA"].contains(&ingreso.tipo_pago.as_str()) {
        return Err("Forma de pago no válida.".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut nombres = std::collections::HashSet::new();
    for (i, item) in ingreso.items.iter().enumerate() {
        let fila = i + 1;
        if !(item.cantidad > 0.0) || !item.cantidad.is_finite() {
            return Err(format!("Fila {}: la cantidad que entra debe ser mayor a 0.", fila));
        }
        if !(item.precio_compra >= 0.0) || !item.precio_compra.is_finite() {
            return Err(format!("Fila {}: el precio de compra no puede ser negativo.", fila));
        }
        match (item.producto_id, &item.nuevo) {
            (Some(id), None) => {
                if !ids.insert(id) {
                    return Err(format!("Fila {}: ese producto ya está en otra fila.", fila));
                }
                if item.vender_caja.is_some() {
                    return Err(format!("Fila {}: la caja para vender se crea solo con productos nuevos (en uno que ya existe, usa sus presentaciones).", fila));
                }
            }
            (None, Some(n)) => {
                let nombre = n.nombre.trim();
                if nombre.is_empty() {
                    return Err(format!("Fila {}: falta el nombre del producto.", fila));
                }
                if !nombres.insert(nombre.to_lowercase()) {
                    return Err(format!("Fila {}: \"{}\" está dos veces.", fila, nombre));
                }
                if !(n.precio > 0.0) || !n.precio.is_finite() {
                    return Err(format!("Fila {}: falta el precio de venta de \"{}\".", fila, nombre));
                }
                if !crate::handlers::productos::unidad_valida(&n.unidad_medida) {
                    return Err(format!("Fila {}: unidad de medida no reconocida.", fila));
                }
                if let Some(c) = &item.vender_caja {
                    if !con_presentaciones {
                        return Err("Para vender la caja entera enciende el módulo Presentaciones.".into());
                    }
                    if !(c.unidades >= 2.0) || c.unidades.fract() != 0.0 {
                        return Err(format!("Fila {}: la caja debe traer 2 unidades o más (un número entero).", fila));
                    }
                    if !(c.precio > 0.0) || !c.precio.is_finite() {
                        return Err(format!("Fila {}: falta el precio de venta de la caja.", fila));
                    }
                }
            }
            _ => return Err(format!("Fila {}: elige un producto que ya existe o escribe uno nuevo.", fila)),
        }
    }
    Ok(())
}

/// Código de 8 dígitos que no es de ningún producto ni presentación.
async fn codigo_libre(conn: &libsql::Connection) -> Result<String, Fallo> {
    for _ in 0..20 {
        let codigo = format!("{}", 20_000_000 + rand::random::<u32>() % 10_000_000);
        let mut f = conn
            .query("SELECT 1 FROM productos WHERE lower(codigo) = lower(?1) LIMIT 1", libsql::params![codigo.clone()])
            .await
            .map_err(e500)?;
        if f.next().await.map_err(e500)?.is_some() {
            continue;
        }
        drop(f);
        // Sin la migración de códigos de presentación, basta con productos.
        if let Ok(mut f) = conn
            .query("SELECT 1 FROM producto_presentaciones WHERE lower(codigo_barras) = lower(?1) LIMIT 1", libsql::params![codigo.clone()])
            .await
        {
            if matches!(f.next().await, Ok(Some(_))) {
                continue;
            }
        }
        return Ok(codigo);
    }
    Err(e500("No se pudo generar un código libre. Intenta de nuevo."))
}

/// El proveedor "COMPRA RÁPIDA" del negocio (lo crea la primera vez).
async fn proveedor_rapido(conn: &libsql::Connection) -> Result<i64, Fallo> {
    let mut f = conn
        .query("SELECT id FROM proveedores WHERE nombre = ?1 ORDER BY id LIMIT 1", libsql::params![PROVEEDOR_RAPIDO])
        .await
        .map_err(e500)?;
    if let Some(fila) = f.next().await.map_err(e500)? {
        return fila.get::<i64>(0).map_err(e500);
    }
    drop(f);
    conn.execute(
        "INSERT INTO proveedores (nombre, tipo_documento, notas) VALUES (?1, 'NINGUNO', 'Ingresos rápidos de productos (lo crea el sistema).')",
        libsql::params![PROVEEDOR_RAPIDO],
    )
    .await
    .map_err(e500)?;
    Ok(conn.last_insert_rowid())
}

/// Si algo falla después de crear productos nuevos, se borran (todavía no
/// tienen ventas ni compras): el ingreso queda como si no hubiera pasado.
async fn deshacer_productos(conn: &libsql::Connection, ids: &[i64]) {
    for id in ids {
        let _ = conn.execute("DELETE FROM producto_presentaciones WHERE producto_id = ?1", libsql::params![*id]).await;
        let _ = conn.execute("DELETE FROM productos WHERE id = ?1", libsql::params![*id]).await;
    }
}

/// POST /compras/rapida
pub async fn ingreso_rapido(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(ingreso): Json<IngresoRapido>,
) -> Result<Json<ResultadoIngreso>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    crate::handlers::rubros::exigir_modulo(&conn, crate::handlers::rubros::MODULO_INGRESO_RAPIDO, "Ingreso rápido").await?;
    let negocio = crate::handlers::rubros::negocio(&conn).await;
    let con_presentaciones = negocio.modulos.iter().any(|m| m == crate::handlers::rubros::MODULO_PRESENTACIONES);
    validar(&ingreso, con_presentaciones).map_err(malo)?;

    // Lo que ya existe: activo y, si es perecible, con un vencimiento válido.
    let hoy = crate::logica::tiempo::hoy_lima();
    for (i, item) in ingreso.items.iter().enumerate() {
        if let Some(id) = item.producto_id {
            let mut f = conn
                .query("SELECT nombre, lleva_vencimiento FROM productos WHERE id = ?1 AND activo = 1", libsql::params![id])
                .await
                .map_err(e500)?;
            let fila = f.next().await.map_err(e500)?.ok_or_else(|| malo(format!("Fila {}: ese producto ya no existe o está desactivado.", i + 1)))?;
            if fila.get::<i64>(1).unwrap_or(0) == 1 {
                let nombre: String = fila.get(0).unwrap_or_default();
                let fecha = item.fecha_vencimiento.as_deref().map(str::trim).unwrap_or("");
                if let Some(error) = crate::handlers::proveedores::revisar_vencimiento_recibido(&nombre, fecha, &hoy) {
                    return Err(malo(error));
                }
            }
        }
    }
    // Un nuevo con el nombre de uno que ya existe: se repone el que existe.
    for item in &ingreso.items {
        if let Some(n) = &item.nuevo {
            let mut f = conn
                .query(
                    "SELECT 1 FROM productos WHERE activo = 1 AND lower(trim(nombre)) = lower(?1) LIMIT 1",
                    libsql::params![n.nombre.trim().to_string()],
                )
                .await
                .map_err(e500)?;
            if f.next().await.map_err(e500)?.is_some() {
                return Err(malo(format!(
                    "Ya existe un producto llamado \"{}\". Búscalo en la fila para reponerlo en vez de crearlo otra vez.",
                    n.nombre.trim()
                )));
            }
        }
    }

    let proveedor_id = proveedor_rapido(&conn).await?;

    // Productos nuevos (stock 0: lo sube la compra al recibirse).
    let mut creados: Vec<i64> = Vec::new();
    let mut items_compra = Vec::with_capacity(ingreso.items.len());
    for item in &ingreso.items {
        let producto_id = match (item.producto_id, &item.nuevo) {
            (Some(id), _) => id,
            (None, Some(n)) => {
                let codigo = match n.codigo.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    Some(c) => c.to_string(),
                    None => match codigo_libre(&conn).await {
                        Ok(c) => c,
                        Err(e) => {
                            deshacer_productos(&conn, &creados).await;
                            return Err(e);
                        }
                    },
                };
                let nuevo = NuevoProducto {
                    codigo,
                    nombre: n.nombre.trim().to_string(),
                    descripcion: None,
                    precio: n.precio,
                    stock: 0.0,
                    stock_minimo: 0.0,
                    unidad_medida: n.unidad_medida.clone(),
                    categoria_id: n.categoria_id,
                    descuento_porcentaje: None,
                    lleva_vencimiento: Some(false),
                    imagen_url: None,
                    precio_compra: Some(item.precio_compra),
                    controla_stock: None,
                };
                let id = match crate::handlers::productos::agregar_producto(Extension(tenant.clone()), Json(nuevo)).await {
                    Ok(Json(r)) => r.producto_id.ok_or_else(|| e500("El producto no se creó."))?,
                    Err(e) => {
                        deshacer_productos(&conn, &creados).await;
                        return Err(e);
                    }
                };
                creados.push(id);
                if let Some(c) = &item.vender_caja {
                    let nombre = format!("Caja x{}", c.unidades as i64);
                    if let Err(e) = conn
                        .execute(
                            "INSERT INTO producto_presentaciones (producto_id, nombre, factor, precio, orden) VALUES (?1, ?2, ?3, ?4, 0)",
                            libsql::params![id, nombre, c.unidades, c.precio],
                        )
                        .await
                    {
                        deshacer_productos(&conn, &creados).await;
                        return Err(e500(e));
                    }
                }
                id
            }
            _ => unreachable!("validar() ya lo revisó"),
        };
        items_compra.push(ItemCompra {
            producto_id,
            cantidad: item.cantidad,
            precio_compra: item.precio_compra,
            precio_venta_sugerido: item.nuevo.as_ref().map(|n| n.precio),
        });
    }

    // La compra, igual que desde Proveedores.
    let compra = NuevaCompraRequest {
        proveedor_id,
        fecha_compra: hoy.clone(),
        items: items_compra,
        descuento: None,
        credito_aplicado: None,
        tipo_pago: ingreso.tipo_pago.clone(),
        fecha_vencimiento_pago: None,
        factura_numero: None,
        notas: Some(ingreso.notas.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Ingreso rápido".to_string())),
        usuario_id: claims.sub,
    };
    let creada = match crate::handlers::proveedores::crear_compra(Extension(tenant.clone()), Extension(claims.clone()), Json(compra)).await {
        Ok(Json(r)) => r,
        Err(e) => {
            deshacer_productos(&conn, &creados).await;
            return Err(e);
        }
    };
    let compra_id = creada.compra_id.ok_or_else(|| e500("La compra no se registró."))?;
    let folio = creada.folio.clone().unwrap_or_default();

    // Y se recibe completa en el mismo momento (sube el stock).
    let mut f = conn
        .query("SELECT id FROM detalles_compra WHERE compra_id = ?1 ORDER BY id", libsql::params![compra_id])
        .await
        .map_err(e500)?;
    let mut detalles = Vec::new();
    while let Some(fila) = f.next().await.map_err(e500)? {
        detalles.push(fila.get::<i64>(0).map_err(e500)?);
    }
    drop(f);
    let recepcion = RecibirMercaderiaRequest {
        compra_id,
        items: detalles
            .iter()
            .zip(&ingreso.items)
            .map(|(detalle_id, item)| ItemRecepcion {
                detalle_id: *detalle_id,
                cantidad_recibida: item.cantidad,
                cantidad_conforme: item.cantidad,
                fecha_vencimiento: item.fecha_vencimiento.clone(),
                numero_lote: None,
            })
            .collect(),
        notas_recepcion: Some("Ingreso rápido".to_string()),
    };
    if let Err((_, texto)) = crate::handlers::proveedores::recibir_mercaderia(Extension(tenant.clone()), Json(recepcion)).await {
        // La compra ya quedó (y si fue con la caja, su pago): no se borra a
        // escondidas. Se avisa para recibirla desde Compras.
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("La compra {} quedó registrada, pero no se pudo subir el stock ({}). Recíbela desde Proveedores → Compras.", folio, texto),
        ));
    }

    let repuestos = ingreso.items.len() - creados.len();
    let total: f64 = ingreso.items.iter().map(|i| i.cantidad * i.precio_compra).sum();
    let mut partes = Vec::new();
    if !creados.is_empty() {
        partes.push(format!("{} producto{} nuevo{}", creados.len(), if creados.len() == 1 { "" } else { "s" }, if creados.len() == 1 { "" } else { "s" }));
    }
    if repuestos > 0 {
        partes.push(format!("{} repuesto{}", repuestos, if repuestos == 1 { "" } else { "s" }));
    }
    let mut mensaje = format!("Ingreso {} registrado: {}. Total S/ {:.2}.", folio, partes.join(" y "), total);
    if ingreso.tipo_pago == "EFECTIVO_CAJA" && total > 0.0 {
        mensaje.push_str(" Se descontó del efectivo de la caja.");
    }
    Ok(Json(ResultadoIngreso { compra_id, folio, creados: creados.len(), repuestos, mensaje }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ingreso(json: serde_json::Value) -> IngresoRapido {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn revisa_antes_de_guardar() {
        let nuevo = serde_json::json!({"nombre": "Gaseosa 500 ml", "unidad_medida": "UNIDAD", "categoria_id": 1, "precio": 2.5});
        let ok = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [{"nuevo": nuevo, "cantidad": 18, "precio_compra": 1.5}]}));
        assert!(validar(&ok, false).is_ok());

        let sin_items = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": []}));
        assert!(validar(&sin_items, false).is_err());
        let credito = ingreso(serde_json::json!({"tipo_pago": "CREDITO", "items": [{"producto_id": 1, "cantidad": 1, "precio_compra": 1}]}));
        assert!(validar(&credito, false).unwrap_err().contains("Forma de pago"));
        let ninguno = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [{"cantidad": 1, "precio_compra": 1}]}));
        assert!(validar(&ninguno, false).is_err());
        let cero = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [{"producto_id": 1, "cantidad": 0, "precio_compra": 1}]}));
        assert!(validar(&cero, false).unwrap_err().contains("cantidad"));
        let repetido = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [
            {"producto_id": 1, "cantidad": 1, "precio_compra": 1}, {"producto_id": 1, "cantidad": 2, "precio_compra": 1}]}));
        assert!(validar(&repetido, false).is_err());
        let sin_precio = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [
            {"nuevo": {"nombre": "X", "unidad_medida": "UNIDAD", "categoria_id": 1, "precio": 0}, "cantidad": 1, "precio_compra": 1}]}));
        assert!(validar(&sin_precio, false).unwrap_err().contains("precio de venta"));

        // La caja para vender necesita el módulo Presentaciones y un entero >= 2.
        let caja = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [
            {"nuevo": nuevo, "cantidad": 18, "precio_compra": 1.5, "vender_caja": {"unidades": 9, "precio": 20}}]}));
        assert!(validar(&caja, false).unwrap_err().contains("Presentaciones"));
        assert!(validar(&caja, true).is_ok());
        let caja_mala = ingreso(serde_json::json!({"tipo_pago": "EFECTIVO", "items": [
            {"nuevo": nuevo, "cantidad": 18, "precio_compra": 1.5, "vender_caja": {"unidades": 1.5, "precio": 20}}]}));
        assert!(validar(&caja_mala, true).is_err());
    }
}
