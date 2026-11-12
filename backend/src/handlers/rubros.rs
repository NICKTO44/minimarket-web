//! Rubro del negocio y módulos.
//!
//! El sistema tiene un núcleo universal (ventas, caja, productos, clientes,
//! reportes...) y módulos que se encienden por negocio. El RUBRO es una
//! plantilla: al elegirlo se encienden los módulos que le tocan y se cargan
//! sus unidades sugeridas; después el administrador puede encender o apagar
//! módulos sueltos. Los nombres de pantalla por rubro ("Carta" en vez de
//! "Productos") viven en el frontend (utils/rubros.js), que debe coincidir
//! con este catálogo.

use axum::{extract::Extension, Json, http::StatusCode};
use std::sync::Arc;

use crate::handlers::mesas::{aplicar_modo_negocio, modo_restaurante};
use crate::middleware_auth::exigir_solo_admin;
use crate::models::auth::Claims;
use crate::models::rubro::*;
use crate::tenants::TenantDb;

type Resultado<T> = Result<Json<T>, (StatusCode, String)>;

/// Atención en mesas: Mesas, Preparación, Carta de hoy y opciones de producto.
pub const MODULO_MESAS: &str = "MESAS";
/// Vender cosas que no descuentan stock (un servicio, algo preparado al momento).
pub const MODULO_SERVICIOS: &str = "SERVICIOS";
/// Venta por medidas: cantidad con decimales en el punto de venta y
/// calculadora de pie tablar (maderera).
pub const MODULO_MEDIDAS: &str = "MEDIDAS";
/// Detracción (SPOT) en facturas que superan el monto mínimo. Sus datos
/// (porcentaje, código, cuenta) están en handlers/detraccion.rs.
pub const MODULO_DETRACCION: &str = "DETRACCION";
/// Cotizaciones / proformas que luego se cargan en el punto de venta.
pub const MODULO_COTIZACIONES: &str = "COTIZACIONES";
/// Ventas al crédito con abonos (cuentas por cobrar).
pub const MODULO_CREDITO: &str = "CREDITO";
/// Guía de remisión remitente electrónica.
pub const MODULO_GUIAS: &str = "GUIAS";
/// Tallas y colores: un modelo con una fila por talla/color, cada una con
/// su código de barras, precio y stock (handlers/variantes.rs).
pub const MODULO_VARIANTES: &str = "VARIANTES";
/// Cambio de prenda: el cliente devuelve algo y se lleva otra cosa en la
/// misma operación (handlers/cambios.rs).
pub const MODULO_CAMBIOS: &str = "CAMBIOS";
/// Reporte de ganancias: costo promedio por producto, costo guardado en
/// cada venta y precio de compra obligatorio (handlers/ganancias.rs). No
/// pertenece a ningún rubro: viene apagado y lo enciende quien lo quiera.
pub const MODULO_GANANCIAS: &str = "GANANCIAS";
/// Cambiar el precio de un producto en el carrito, solo para esa venta. El
/// servidor siempre guardó en la venta el precio que manda el punto de
/// venta (así funcionan las cotizaciones); el módulo solo muestra el campo
/// para escribirlo. Tampoco pertenece a un rubro: viene apagado.
pub const MODULO_PRECIO_VENTA: &str = "PRECIO_VENTA";
/// Presentaciones: el mismo producto en rollo, caja, ciento... con su
/// precio; el stock sigue en la unidad del producto (handlers/presentaciones.rs).
pub const MODULO_PRESENTACIONES: &str = "PRESENTACIONES";
/// Precio por mayor: otro precio por unidad desde cierta cantidad.
pub const MODULO_PRECIO_MAYOR: &str = "PRECIO_MAYOR";
/// Farmacia / Botica: principio activo, concentración, forma, laboratorio,
/// registro sanitario y condición de venta de cada medicamento
/// (handlers/medicamentos.rs).
pub const MODULO_FICHA_MEDICAMENTO: &str = "FICHA_MEDICAMENTO";
/// Farmacia: no se cobra un medicamento con receta sin ella, y libro de
/// recetas (handlers/recetas.rs). Necesita FICHA_MEDICAMENTO.
pub const MODULO_RECETAS: &str = "RECETAS";
/// Farmacia: en el punto de venta, los productos con el mismo principio
/// activo, concentración y forma (para un agotado o uno más barato). Se
/// calcula en el navegador con las fichas. Necesita FICHA_MEDICAMENTO.
pub const MODULO_ALTERNATIVAS: &str = "ALTERNATIVAS";
/// Lubricentro / Repuestos: marca, n.° de parte, códigos alternos (y con
/// ellos las equivalencias), ubicación y garantía (handlers/repuestos.rs).
pub const MODULO_FICHA_REPUESTO: &str = "FICHA_REPUESTO";
/// Lubricentro / Repuestos: con qué vehículos (marca, modelo, años, motor)
/// es compatible cada repuesto, y búsqueda por vehículo en el punto de
/// venta (handlers/compatibilidad.rs).
pub const MODULO_COMPATIBILIDAD: &str = "COMPATIBILIDAD";
/// Lubricentro / Repuestos: Precio 2 y Precio 3 de referencia por producto;
/// el vendedor elige cuál cobrar en el punto de venta (handlers/precios_venta.rs).
pub const MODULO_TRES_PRECIOS: &str = "TRES_PRECIOS";
/// Lubricentro: punto de venta en tabla (buscador con lista, una fila por
/// producto). Solo cambia la pantalla: el cobro y la venta son los de siempre.
pub const MODULO_VENTA_TABLA: &str = "VENTA_TABLA";
/// Librería: autor, editorial, serie, año y edición de los libros, para
/// buscarlos y filtrarlos en el punto de venta (handlers/libros.rs).
pub const MODULO_FICHA_LIBRO: &str = "FICHA_LIBRO";
/// Repuestos: el dueño elige a mano qué productos son equivalentes (forman
/// un grupo); Productos y el punto de venta los muestran al buscar
/// (handlers/equivalentes.rs).
pub const MODULO_EQUIVALENTES: &str = "EQUIVALENTES";
/// Menú de navegación arriba (barra horizontal) en vez del lateral, solo en
/// computadoras y laptops. Es solo de pantalla: cualquier rubro puede usarlo.
pub const MODULO_MENU_ARRIBA: &str = "MENU_ARRIBA";
/// Vender algo que el negocio consigue afuera y no está en su inventario.
/// Solo sale en el comprobante (boleta, factura o nota de venta); no entra a
/// caja, reportes, ganancias, stock ni créditos (handlers/fuera_inventario.rs).
pub const MODULO_VENTA_FUERA_INVENTARIO: &str = "VENTA_FUERA_INVENTARIO";
/// Vender sin internet: el punto de venta guarda en el equipo lo que se
/// vende (y la apertura de caja y los gastos de caja) y lo sube al volver
/// la conexión; la boleta o factura se emite entonces con la fecha real de
/// la venta (handlers/sin_conexion.rs). Sirve para cualquier rubro.
pub const MODULO_VENTA_SIN_CONEXION: &str = "VENTA_SIN_CONEXION";
/// Comprobante impreso en formato clásico (A4, A5 y ticket): los datos del
/// cliente en recuadro (R.U.C./DNI, señores, vendedor, dirección, teléfono),
/// sin la hora y con el pago solo por su medio ("Yape / Plin"). Solo cambia
/// lo impreso, no lo que se envía a SUNAT. Sirve para cualquier rubro.
pub const MODULO_COMPROBANTE_CLASICO: &str = "COMPROBANTE_CLASICO";
/// Logo del negocio (el de Configuración) arriba del ticket de 80/58 mm:
/// boleta, factura y nota de venta, al imprimir, al reimprimir y en su PDF.
/// Solo cambia lo impreso. Apagado, el ticket sale como siempre (sin logo).
/// Sirve para cualquier rubro.
pub const MODULO_LOGO_TICKET: &str = "LOGO_TICKET";
/// Impresión directa: al cobrar se imprime solo, sin preguntar el formato
/// (boleta y nota de venta en ticket de 80/58 mm, factura en hoja A4). Solo
/// es del frontend. Sirve para cualquier rubro.
pub const MODULO_IMPRESION_DIRECTA: &str = "IMPRESION_DIRECTA";
/// Catálogo de productos en tarjetas con foto, solo para mirar (sin
/// vender ni editar), para que los vendedores revisen rápido. Solo es una
/// pantalla del frontend: lee lo mismo que Productos. Sirve para cualquier
/// rubro.
pub const MODULO_CATALOGO: &str = "CATALOGO";
/// Lubricentro: vehículos de los clientes (placa), kilometraje y próximo
/// cambio en la venta, historial por placa y recordatorios
/// (handlers/vehiculos.rs).
pub const MODULO_VEHICULOS: &str = "VEHICULOS";
/// Lubricentro: órdenes de servicio por vehículo con productos y mano de
/// obra, mecánicos y comisiones (handlers/ordenes.rs). Necesita VEHICULOS.
pub const MODULO_ORDENES_SERVICIO: &str = "ORDENES_SERVICIO";
/// Paquetes: varios productos a un precio ("Cambio de aceite completo").
/// En la venta entran sus productos con un descuento por línea
/// (handlers/paquetes.rs). No cambia cómo se cobra.
pub const MODULO_PAQUETES: &str = "PAQUETES";
/// Pedidos por encargo (pastelería) u órdenes de lavado (lavandería):
/// fecha de entrega, detalle por línea con su precio, foto y adelantos; se
/// entrega cobrando el saldo en el punto de venta (handlers/encargos.rs).
pub const MODULO_ENCARGOS: &str = "ENCARGOS";
/// Producción del día y mermas (pastelería, panadería): lo producido entra
/// al stock sin ser una compra; lo perdido sale con su costo
/// (handlers/produccion.rs).
pub const MODULO_PRODUCCION: &str = "PRODUCCION";
/// Recetas de producción con sus insumos: costo de cada producto y
/// descuento de insumos al producir (handlers/produccion.rs). Necesita PRODUCCION.
pub const MODULO_INSUMOS: &str = "INSUMOS";
/// Dark kitchen: pedidos por WhatsApp / delivery con datos de entrega,
/// pago adelantado o contra entrega y repartidor (handlers/delivery.rs).
/// Necesita la atención en mesas (sus pedidos, comandas y cocina).
pub const MODULO_DELIVERY: &str = "DELIVERY";
/// Restaurante: margen por plato. Costo de cada plato por receta, costo
/// fijo o un % aproximado, margen sin IGV, alertas y descarga en Excel
/// (handlers/costeo.rs). Solo para el rubro Restaurante.
pub const MODULO_COSTEO: &str = "COSTEO";

/// Tecnología: órdenes de reparación (celulares, laptops, impresoras) con
/// datos del equipo, diagnóstico, presupuesto, técnico y garantía. Cada orden
/// es un encargo: adelanto sin comprobante y cobro del saldo en el punto de
/// venta (handlers/reparaciones.rs). Solo para el rubro Tecnología.
pub const MODULO_REPARACIONES: &str = "REPARACIONES";

/// Varias cajas abiertas a la vez: cada usuario que cobra abre la suya (por
/// ejemplo, una por computadora) y vende solo en ella; el administrador ve y
/// puede cerrar todas (handlers/cajas.rs). Apagado, una sola caja por
/// negocio, como siempre. Sirve para cualquier rubro.
pub const MODULO_VARIAS_CAJAS: &str = "VARIAS_CAJAS";
/// Ingreso rápido de productos: nuevos o para reponer, en un paso y sin
/// elegir proveedor; queda como compra ya recibida al proveedor "COMPRA
/// RÁPIDA" (handlers/compra_rapida.rs). Sirve para cualquier rubro.
pub const MODULO_INGRESO_RAPIDO: &str = "INGRESO_RAPIDO";
/// Compras por Excel: exportar la lista de productos (o los que están en su
/// stock mínimo), llenar cantidad y costo, y cargarla en Nueva compra de un
/// golpe. Solo es de pantalla (la compra se registra como siempre). Viene
/// encendido en Farmacia; sirve para cualquier rubro.
pub const MODULO_COMPRA_EXCEL: &str = "COMPRA_EXCEL";
/// Al revés: encendido = NO pedir "con cuánto paga" en efectivo (se cobra el
/// total exacto). En Configuración se muestra como "Pedir con cuánto paga"
/// encendido para todos; solo se guarda cuando el negocio lo apaga.
pub const MODULO_SIN_VUELTO: &str = "SIN_VUELTO";
/// Créditos por cliente: pestaña con lo que debe cada cliente (sumando sus
/// boletas) y su estado de cuenta para cobrarle todo junto. Necesita CREDITO.
/// Viene encendido en Lavandería; cualquier rubro lo puede encender.
pub const MODULO_CREDITOS_POR_CLIENTE: &str = "CREDITOS_POR_CLIENTE";
/// Descuento en la venta: en % o en soles, a un producto o al total (el
/// total se reparte entre las líneas). Solo es de pantalla: el servidor ya
/// acepta el descuento de cada línea. Cualquier rubro lo puede encender.
pub const MODULO_DESCUENTO_VENTA: &str = "DESCUENTO_VENTA";

pub const RUBRO_TECNOLOGIA: &str = "TECNOLOGIA";

/// (módulo, único rubro donde se puede encender).
const SOLO_EN_RUBRO: &[(&str, &str)] = &[
    (MODULO_COSTEO, RUBRO_RESTAURANTE),
    (MODULO_REPARACIONES, RUBRO_TECNOLOGIA),
    (MODULO_TRES_PRECIOS, "LUBRICENTRO"),
    (MODULO_VENTA_TABLA, "LUBRICENTRO"),
];

/// (módulo, el módulo que necesita encendido).
const DEPENDENCIAS: &[(&str, &str)] = &[
    (MODULO_RECETAS, MODULO_FICHA_MEDICAMENTO),
    (MODULO_ALTERNATIVAS, MODULO_FICHA_MEDICAMENTO),
    (MODULO_ORDENES_SERVICIO, MODULO_VEHICULOS),
    (MODULO_INSUMOS, MODULO_PRODUCCION),
    (MODULO_DELIVERY, MODULO_MESAS),
    (MODULO_CREDITOS_POR_CLIENTE, MODULO_CREDITO),
];

const MODULOS: &[&str] = &[
    MODULO_MESAS,
    MODULO_SERVICIOS,
    MODULO_MEDIDAS,
    MODULO_DETRACCION,
    MODULO_COTIZACIONES,
    MODULO_CREDITO,
    MODULO_GUIAS,
    MODULO_VARIANTES,
    MODULO_CAMBIOS,
    MODULO_GANANCIAS,
    MODULO_PRECIO_VENTA,
    MODULO_PRESENTACIONES,
    MODULO_PRECIO_MAYOR,
    MODULO_FICHA_MEDICAMENTO,
    MODULO_RECETAS,
    MODULO_ALTERNATIVAS,
    MODULO_FICHA_REPUESTO,
    MODULO_COMPATIBILIDAD,
    MODULO_TRES_PRECIOS,
    MODULO_VENTA_TABLA,
    MODULO_FICHA_LIBRO,
    MODULO_EQUIVALENTES,
    MODULO_MENU_ARRIBA,
    MODULO_VENTA_FUERA_INVENTARIO,
    MODULO_VENTA_SIN_CONEXION,
    MODULO_COMPROBANTE_CLASICO,
    MODULO_LOGO_TICKET,
    MODULO_IMPRESION_DIRECTA,
    MODULO_CATALOGO,
    MODULO_VEHICULOS,
    MODULO_ORDENES_SERVICIO,
    MODULO_PAQUETES,
    MODULO_ENCARGOS,
    MODULO_PRODUCCION,
    MODULO_INSUMOS,
    MODULO_DELIVERY,
    MODULO_COSTEO,
    MODULO_REPARACIONES,
    MODULO_VARIAS_CAJAS,
    MODULO_INGRESO_RAPIDO,
    MODULO_COMPRA_EXCEL,
    MODULO_SIN_VUELTO,
    MODULO_CREDITOS_POR_CLIENTE,
    MODULO_DESCUENTO_VENTA,
];

/// Rubro de un negocio que no eligió ninguno y no atiende en mesas.
pub const RUBRO_GENERAL: &str = "GENERAL";
const RUBRO_RESTAURANTE: &str = "RESTAURANTE";

/// (rubro, módulos que enciende, unidades sugeridas; vacío = las 20 de siempre)
const RUBROS: &[(&str, &[&str], &[&str])] = &[
    ("BODEGA", &[], &[]),
    (
        RUBRO_RESTAURANTE,
        &[MODULO_MESAS, MODULO_SERVICIOS],
        &["UNIDAD", "PLATO", "PORCION", "ENTERO", "MEDIO", "CUARTO", "VASO", "TAZA", "JARRA", "COPA", "BOTELLA"],
    ),
    (
        "FERRETERIA",
        &[MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_GUIAS, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR],
        &[
            "UNIDAD", "KG", "LITRO", "PAQUETE", "CAJA", "DOCENA", "PAR", "METRO", "GALON", "BOLSA", "ROLLO", "MILLAR", "JUEGO",
            "PIEZA", "PLANCHA", "CIENTO", "VARILLA", "BALDE", "LATA", "PLIEGO", "TUBO",
        ],
    ),
    (
        "MADERERA",
        &[MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_DETRACCION, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_GUIAS],
        &["UNIDAD", "PIE_TABLAR", "PIEZA", "PLANCHA", "METRO", "M2", "M3", "KG", "GALON", "CAJA", "MILLAR"],
    ),
    (
        "ROPA",
        &[MODULO_CREDITO, MODULO_VARIANTES, MODULO_CAMBIOS],
        &["UNIDAD", "PAR", "DOCENA", "PAQUETE", "CAJA", "JUEGO"],
    ),
    (
        "FARMACIA",
        &[MODULO_FICHA_MEDICAMENTO, MODULO_RECETAS, MODULO_ALTERNATIVAS, MODULO_PRESENTACIONES, MODULO_COMPRA_EXCEL],
        &[
            "UNIDAD", "TABLETA", "CAPSULA", "BLISTER", "CAJA", "FRASCO", "AMPOLLA", "VIAL", "SOBRE", "TUBO",
            "SUPOSITORIO", "OVULO", "PARCHE", "ML", "GRAMO", "PAQUETE",
        ],
    ),
    (
        "LUBRICENTRO",
        &[
            MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR,
            MODULO_FICHA_REPUESTO, MODULO_COMPATIBILIDAD, MODULO_VEHICULOS, MODULO_ORDENES_SERVICIO, MODULO_PAQUETES,
            MODULO_TRES_PRECIOS,
        ],
        &[
            "UNIDAD", "LITRO", "GALON", "ML", "KG", "PAR", "JUEGO", "CAJA", "PAQUETE", "BALDE", "LATA", "TUBO", "BOTELLA",
            "METRO", "ROLLO", "PIEZA",
        ],
    ),
    (
        "PASTELERIA",
        &[
            MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_COTIZACIONES, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR, MODULO_ENCARGOS,
            MODULO_PRODUCCION, MODULO_INSUMOS,
        ],
        &[
            "UNIDAD", "PORCION", "ENTERO", "MEDIO", "CUARTO", "DOCENA", "CIENTO", "KG", "GRAMO", "PAQUETE", "CAJA", "BOLSA",
            "TAZA", "VASO", "BOTELLA",
        ],
    ),
    (
        "LAVANDERIA",
        &[MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_ENCARGOS, MODULO_CREDITO, MODULO_CREDITOS_POR_CLIENTE],
        &["UNIDAD", "KG", "PRENDA", "PAR", "JUEGO", "PIEZA"],
    ),
    (
        RUBRO_TECNOLOGIA,
        &[MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRECIO_MAYOR, MODULO_REPARACIONES],
        &["UNIDAD", "PAQUETE", "CAJA", "PAR", "JUEGO", "METRO", "ROLLO", "PIEZA"],
    ),
    (
        "LIBRERIA",
        &[MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR, MODULO_FICHA_LIBRO],
        &[
            "UNIDAD", "PAQUETE", "CAJA", "DOCENA", "CIENTO", "MILLAR", "PLIEGO", "JUEGO", "ROLLO", "BOLSA", "SOBRE", "PIEZA",
            "METRO",
        ],
    ),
    (RUBRO_GENERAL, &[], &[]),
];

/// El primer módulo de la lista que necesita otro que no está encendido.
pub fn faltante(modulos: &[String]) -> Option<(&'static str, &'static str)> {
    DEPENDENCIAS
        .iter()
        .find(|(m, necesita)| modulos.iter().any(|x| x == m) && !modulos.iter().any(|x| x == necesita))
        .copied()
}

fn plantilla(rubro: &str) -> Option<&'static (&'static str, &'static [&'static str], &'static [&'static str])> {
    RUBROS.iter().find(|r| r.0 == rubro)
}

pub fn rubro_valido(rubro: &str) -> bool {
    plantilla(rubro).is_some()
}

/// Módulos con los que arranca un negocio nuevo de ese rubro.
pub fn modulos_del_rubro(rubro: &str) -> Vec<String> {
    plantilla(rubro).map(|r| r.1.iter().map(|m| m.to_string()).collect()).unwrap_or_default()
}

/// Unidades con las que arranca ese rubro ("A,B,C"), o None si usa las de siempre.
pub fn unidades_del_rubro(rubro: &str) -> Option<String> {
    plantilla(rubro).filter(|r| !r.2.is_empty()).map(|r| r.2.join(","))
}

/// Los módulos extra (todos menos MESAS) como se guardan en la base.
pub fn modulos_extra_csv(modulos: &[String]) -> String {
    MODULOS
        .iter()
        .filter(|m| **m != MODULO_MESAS && modulos.iter().any(|x| x == *m))
        .copied()
        .collect::<Vec<_>>()
        .join(",")
}

fn e500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Rubro y módulos del negocio. Si la base aún no tiene la migración 0011,
/// o el negocio nunca eligió, se deduce de si atiende en mesas (así todo se
/// ve igual que antes).
pub async fn negocio(conn: &libsql::Connection) -> Negocio {
    let mesas = modo_restaurante(conn).await;
    let (mut rubro, mut extra): (Option<String>, Option<Vec<String>>) = (None, None);
    if let Ok(mut filas) = conn.query("SELECT rubro, modulos FROM configuracion_tienda LIMIT 1", ()).await {
        if let Ok(Some(f)) = filas.next().await {
            rubro = f.get::<String>(0).ok().filter(|r| rubro_valido(r));
            extra = f.get::<String>(1).ok().map(|texto| {
                texto.split(',').map(|m| m.trim().to_string()).filter(|m| MODULOS.contains(&m.as_str())).collect()
            });
        }
    }
    let rubro_fijo = rubro.is_some();
    let rubro = rubro.unwrap_or_else(|| if mesas { RUBRO_RESTAURANTE } else { RUBRO_GENERAL }.to_string());
    // Nunca configurado: el restaurante ya vendía productos "preparados" sin stock.
    let extra = extra.unwrap_or_else(|| if mesas { vec![MODULO_SERVICIOS.to_string()] } else { Vec::new() });
    let modulos = MODULOS
        .iter()
        .filter(|m| if **m == MODULO_MESAS { mesas } else { extra.iter().any(|x| x == *m) })
        .map(|m| m.to_string())
        .collect();
    Negocio { rubro, modulos, modo_negocio: if mesas { "RESTAURANTE" } else { "TIENDA" }.to_string(), rubro_fijo }
}

/// Corta con 403 si el negocio no tiene encendido ese módulo.
pub async fn exigir_modulo(conn: &libsql::Connection, modulo: &str, nombre: &str) -> Result<(), (StatusCode, String)> {
    if negocio(conn).await.modulos.iter().any(|m| m == modulo) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!("Este negocio no tiene encendido el módulo \"{}\". Actívalo en Configuración → Datos del negocio.", nombre),
        ))
    }
}

/// GET /negocio — rubro y módulos (lo lee cualquier usuario del negocio).
pub async fn obtener_negocio(Extension(tenant): Extension<Arc<TenantDb>>) -> Resultado<Negocio> {
    let conn = tenant.0.connect().map_err(e500)?;
    Ok(Json(negocio(&conn).await))
}

/// PUT /configuracion/negocio — el administrador cambia el rubro y/o los
/// módulos. Encender o apagar MESAS pasa por las mismas reglas de siempre
/// (mesas de ejemplo al activarlo; no se apaga con pedidos abiertos).
pub async fn guardar_negocio(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<GuardarNegocio>,
) -> Resultado<Negocio> {
    exigir_solo_admin(&claims)?;
    let rubro = payload.rubro.trim().to_uppercase();
    if !rubro_valido(&rubro) {
        return Err((StatusCode::BAD_REQUEST, "Rubro no válido.".to_string()));
    }
    if let Some(malo) = payload.modulos.iter().find(|m| !MODULOS.contains(&m.as_str())) {
        return Err((StatusCode::BAD_REQUEST, format!("Módulo no válido: {}", malo)));
    }
    if let Some((modulo, necesita)) = faltante(&payload.modulos) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("El módulo {} necesita encendido también el módulo {}.", modulo, necesita),
        ));
    }
    if let Some((modulo, _)) = SOLO_EN_RUBRO.iter().find(|(m, r)| *r != rubro && payload.modulos.iter().any(|x| x == m)) {
        return Err((StatusCode::BAD_REQUEST, format!("El módulo {} es solo para el rubro Restaurante.", modulo)));
    }
    let conn = tenant.0.connect().map_err(e500)?;
    let actualizando = || (StatusCode::CONFLICT, "Aún no se puede guardar: el sistema se está actualizando. Intenta en un minuto.".to_string());
    // Si la base todavía no tiene la migración 0011, no se toca nada.
    conn.query("SELECT rubro FROM configuracion_tienda LIMIT 1", ()).await.map_err(|_| actualizando())?;
    // El rubro se elige al crear la cuenta y no se cambia desde el sistema
    // (solo los módulos). Un negocio antiguo que nunca eligió puede hacerlo una vez.
    let actual = negocio(&conn).await;
    if actual.rubro_fijo && actual.rubro != rubro {
        return Err((
            StatusCode::BAD_REQUEST,
            "El rubro del negocio se eligió al crear la cuenta y no se cambia. Puedes encender o apagar los módulos que necesites.".to_string(),
        ));
    }

    let quiere_mesas = payload.modulos.iter().any(|m| m == MODULO_MESAS);
    if quiere_mesas != modo_restaurante(&conn).await {
        aplicar_modo_negocio(&conn, if quiere_mesas { "RESTAURANTE" } else { "TIENDA" }).await?;
    }

    conn.execute(
        "UPDATE configuracion_tienda SET rubro = ?1, modulos = ?2, fecha_actualizacion = datetime('now','localtime')",
        libsql::params![rubro, modulos_extra_csv(&payload.modulos)],
    )
    .await
    .map_err(|_| actualizando())?;

    Ok(Json(negocio(&conn).await))
}
