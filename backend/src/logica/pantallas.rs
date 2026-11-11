//! Pantallas por usuario: el administrador elige qué pantallas NO ve cada
//! usuario (Configuración → Usuarios → Permisos). Se guarda la lista de
//! **ocultas** en `usuarios.pantallas_ocultas` (migración 0067):
//!   - NULL o vacía = el usuario ve lo de su rol, como siempre. Así nadie
//!     cambia hasta que el administrador lo decida.
//!   - Guardar ocultas (y no permitidas) hace que una pantalla de un módulo
//!     que se encienda después aparezca sola para todos, sin chocar.
//!
//! El rol sigue mandando primero (`ruta_permitida` en middleware_auth.rs):
//! esto solo quita, nunca da más de lo que el rol permite. El administrador
//! nunca tiene pantallas ocultas.
//!
//! El servidor bloquea solo lo **propio** de cada pantalla oculta (lo que
//! ninguna otra pantalla usa). Lo compartido (leer productos y clientes,
//! la caja abierta, el PDF de un comprobante) queda libre para que las
//! pantallas visibles nunca fallen.

use axum::http::Method;

/// Pantallas que el administrador puede ocultar (ids de utils/menu.js).
pub const CONFIGURABLES: &[&str] = &[
    "POS", "RESUMEN", "CAJA", "HISTORIAL_CAJA", "CLIENTES", "COTIZACIONES", "CREDITOS", "GUIAS",
    "ORDENES", "VEHICULOS", "ENCARGOS", "REPARACIONES", "MESAS", "PREPARACION", "CARTA",
    "PRODUCTOS", "CATALOGO", "STOCK", "PROVEEDORES", "DEVOLUCIONES", "PRODUCCION",
    "COMPROBANTES", "REPORTES",
];

/// Deja solo ids conocidos, sin repetir. Con Punto de venta visible, la
/// Caja también: para vender hay que abrir la caja propia.
pub fn normalizar(ocultas: &[String]) -> Vec<String> {
    let mut limpias: Vec<String> = Vec::new();
    for p in ocultas {
        let p = p.trim().to_ascii_uppercase();
        if CONFIGURABLES.contains(&p.as_str()) && !limpias.contains(&p) {
            limpias.push(p);
        }
    }
    if !limpias.iter().any(|p| p == "POS") {
        limpias.retain(|p| p != "CAJA");
    }
    limpias
}

/// Lee la columna (JSON `["REPORTES", ...]`). Cualquier cosa rara = nada oculto.
pub fn desde_texto(texto: Option<&str>) -> Vec<String> {
    texto
        .and_then(|t| serde_json::from_str::<Vec<String>>(t).ok())
        .map(|v| normalizar(&v))
        .unwrap_or_default()
}

fn segmentos(ruta: &str) -> Vec<&str> {
    ruta.split('/').filter(|s| !s.is_empty()).collect()
}

/// `ruta` empieza con `patron` ("*" = un segmento cualquiera).
fn empieza(ruta: &[&str], patron: &str) -> bool {
    let p = segmentos(patron);
    ruta.len() >= p.len() && p.iter().zip(ruta).all(|(a, b)| *a == "*" || a == b)
}

/// `ruta` es exactamente `patron`.
fn igual(ruta: &[&str], patron: &str) -> bool {
    segmentos(patron).len() == ruta.len() && empieza(ruta, patron)
}

/// ¿Esta petición es propia de una pantalla que el usuario tiene oculta?
pub fn bloqueada(ocultas: &[String], metodo: &Method, ruta: &str) -> bool {
    if ocultas.is_empty() {
        return false;
    }
    let oculta = |p: &str| ocultas.iter().any(|o| o == p);
    let r = segmentos(ruta);
    let lectura = metodo == Method::GET || metodo == Method::HEAD;
    let escritura = !lectura;

    // Punto de venta: cobrar. (Leer productos y clientes lo usan otras pantallas.)
    // (Con el módulo Venta sin conexión, las ventas guardadas sin internet
    // también se suben por /sin-conexion.)
    if oculta("POS") && ((metodo == Method::POST && igual(&r, "/ventas")) || (escritura && empieza(&r, "/sin-conexion"))) {
        return true;
    }
    // Caja (solo puede estar oculta si el Punto de venta también lo está):
    // abrir, cerrar y sus movimientos. La caja abierta se sigue pudiendo
    // consultar (Resumen).
    if oculta("CAJA")
        && ["/cajas/abrir", "/cajas/cerrar", "/cajas/movimiento", "/cajas/movimientos", "/cajas/abiertas"]
            .iter()
            .any(|p| empieza(&r, p))
    {
        return true;
    }
    // Historial de caja: la lista de cierres y el detalle de cada uno.
    if oculta("HISTORIAL_CAJA") && lectura && (igual(&r, "/cajas") || igual(&r, "/cajas/*/detalle")) {
        return true;
    }
    // Clientes: editar, desactivar, reactivar, ver desactivados. Buscar y
    // crear siguen libres (el punto de venta los usa).
    if oculta("CLIENTES")
        && ((metodo == Method::PUT && igual(&r, "/clientes/*"))
            || empieza(&r, "/clientes/*/desactivar")
            || empieza(&r, "/clientes/*/reactivar")
            || empieza(&r, "/clientes/desactivados"))
    {
        return true;
    }
    // Cotizaciones: todo menos crear una desde el punto de venta.
    if oculta("COTIZACIONES") && empieza(&r, "/cotizaciones") && !(metodo == Method::POST && igual(&r, "/cotizaciones")) {
        return true;
    }
    if oculta("CREDITOS") && empieza(&r, "/creditos") {
        return true;
    }
    if oculta("GUIAS") && empieza(&r, "/guias") {
        return true;
    }
    if oculta("ORDENES") && empieza(&r, "/ordenes") {
        return true;
    }
    // Vehículos: todo menos buscar por placa (lo usa el punto de venta).
    if oculta("VEHICULOS") && empieza(&r, "/vehiculos") && !empieza(&r, "/vehiculos/placa") {
        return true;
    }
    if oculta("ENCARGOS") && empieza(&r, "/encargos") {
        return true;
    }
    if oculta("REPARACIONES") && empieza(&r, "/reparaciones") {
        return true;
    }
    // Productos: crear, editar, borrar, importar (Proveedores también crea
    // productos en una compra: solo se bloquea si las dos están ocultas).
    if oculta("PRODUCTOS")
        && oculta("PROVEEDORES")
        && escritura
        && ["/productos", "/categorias", "/modelos", "/repuestos", "/medicamentos", "/libros/fichas", "/precios-venta"]
            .iter()
            .any(|p| empieza(&r, p))
    {
        return true;
    }
    // Stock y lotes: descartar lotes vencidos.
    if oculta("STOCK") && empieza(&r, "/lotes/*/descartar") {
        return true;
    }
    // Proveedores y compras. El ingreso rápido es de la pantalla Productos.
    if oculta("PROVEEDORES")
        && (empieza(&r, "/proveedores") || empieza(&r, "/devoluciones-proveedor") || empieza(&r, "/compras"))
        && !(!oculta("PRODUCTOS") && igual(&r, "/compras/rapida"))
    {
        return true;
    }
    // Devoluciones de clientes y cambios.
    if oculta("DEVOLUCIONES") && escritura && (igual(&r, "/devoluciones") || empieza(&r, "/cambios")) {
        return true;
    }
    if oculta("PRODUCCION") && (empieza(&r, "/produccion") || empieza(&r, "/mermas")) {
        return true;
    }
    // Comprobantes: la lista, anular y reenviar. El PDF y el documento de
    // uno siguen libres (el punto de venta reimprime).
    if oculta("COMPROBANTES")
        && ((lectura && igual(&r, "/comprobantes"))
            || empieza(&r, "/comprobantes/*/anular")
            || empieza(&r, "/comprobantes/*/anulacion")
            || empieza(&r, "/comprobantes/*/reenviar")
            || empieza(&r, "/notas-credito/*/reenviar"))
    {
        return true;
    }
    // Reportes. Las estadísticas y lo más vendido también los usa Resumen:
    // solo se bloquean si las dos pantallas están ocultas.
    if oculta("REPORTES") {
        if empieza(&r, "/reportes/ventas") || empieza(&r, "/reportes/ganancias") || empieza(&r, "/costeo") {
            return true;
        }
        if oculta("RESUMEN") && empieza(&r, "/reportes") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn o(v: &[&str]) -> Vec<String> {
        normalizar(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn sin_ocultas_no_bloquea_nada() {
        assert!(!bloqueada(&[], &Method::POST, "/ventas"));
        assert!(!bloqueada(&[], &Method::GET, "/reportes/ventas"));
    }

    #[test]
    fn la_caja_va_con_el_punto_de_venta() {
        assert_eq!(o(&["CAJA", "REPORTES"]), vec!["REPORTES".to_string()]);
        assert_eq!(o(&["pos", "CAJA", "CAJA", "NADA"]), vec!["POS".to_string(), "CAJA".to_string()]);
        // Con el punto de venta visible, abrir la caja nunca se bloquea.
        assert!(!bloqueada(&o(&["CAJA"]), &Method::POST, "/cajas/abrir"));
        assert!(bloqueada(&o(&["POS", "CAJA"]), &Method::POST, "/cajas/abrir"));
        assert!(bloqueada(&o(&["POS"]), &Method::POST, "/ventas"));
        // La caja abierta se puede consultar igual (Resumen).
        assert!(!bloqueada(&o(&["POS", "CAJA"]), &Method::GET, "/cajas/abierta"));
    }

    #[test]
    fn lo_compartido_queda_libre() {
        let todo: Vec<String> = CONFIGURABLES.iter().map(|s| s.to_string()).collect();
        let todo = normalizar(&todo);
        for (m, ruta) in [
            (Method::GET, "/productos"),
            (Method::GET, "/clientes"),
            (Method::POST, "/clientes"),
            (Method::GET, "/configuracion"),
            (Method::GET, "/cajas/abierta"),
            (Method::GET, "/comprobantes/12/pdf"),
            (Method::GET, "/comprobantes/12/documento"),
            (Method::GET, "/vehiculos/placa/ABC123"),
            (Method::GET, "/suscripcion/estado"),
            (Method::PUT, "/mi-cuenta/clave"),
        ] {
            assert!(!bloqueada(&todo, &m, ruta), "{} {}", m, ruta);
        }
    }

    #[test]
    fn bloquea_lo_propio_de_cada_pantalla() {
        let casos: &[(&[&str], Method, &str)] = &[
            (&["POS", "CAJA"], Method::POST, "/sin-conexion/operaciones"),
            (&["HISTORIAL_CAJA"], Method::GET, "/cajas"),
            (&["HISTORIAL_CAJA"], Method::GET, "/cajas/5/detalle"),
            (&["CLIENTES"], Method::PUT, "/clientes/3"),
            (&["CLIENTES"], Method::POST, "/clientes/3/desactivar"),
            (&["COTIZACIONES"], Method::GET, "/cotizaciones"),
            (&["CREDITOS"], Method::POST, "/creditos/4/abonos"),
            (&["GUIAS"], Method::GET, "/guias"),
            (&["ORDENES"], Method::GET, "/ordenes/2"),
            (&["VEHICULOS"], Method::GET, "/vehiculos/recordatorios"),
            (&["ENCARGOS"], Method::GET, "/encargos"),
            (&["REPARACIONES"], Method::GET, "/reparaciones"),
            (&["PRODUCTOS", "PROVEEDORES"], Method::POST, "/productos"),
            (&["STOCK"], Method::POST, "/lotes/9/descartar"),
            (&["PROVEEDORES"], Method::GET, "/compras"),
            (&["PROVEEDORES"], Method::GET, "/compras/tipo-cambio"),
            (&["DEVOLUCIONES"], Method::POST, "/devoluciones"),
            (&["PRODUCCION"], Method::GET, "/produccion"),
            (&["COMPROBANTES"], Method::GET, "/comprobantes"),
            (&["COMPROBANTES"], Method::POST, "/comprobantes/7/anular"),
            (&["REPORTES"], Method::GET, "/reportes/ventas"),
            (&["REPORTES", "RESUMEN"], Method::GET, "/reportes/estadisticas"),
        ];
        for (ocultas, m, ruta) in casos {
            assert!(bloqueada(&o(ocultas), m, ruta), "{:?} {} {}", ocultas, m, ruta);
        }
    }

    #[test]
    fn una_pantalla_no_rompe_a_otra() {
        // Proveedores visible: sigue creando productos en una compra.
        assert!(!bloqueada(&o(&["PRODUCTOS"]), &Method::POST, "/productos"));
        // Productos visible: su ingreso rápido sigue andando sin Proveedores.
        assert!(!bloqueada(&o(&["PROVEEDORES"]), &Method::POST, "/compras/rapida"));
        assert!(bloqueada(&o(&["PROVEEDORES", "PRODUCTOS"]), &Method::POST, "/compras/rapida"));
        // Resumen visible: sus estadísticas siguen aunque Reportes no.
        assert!(!bloqueada(&o(&["REPORTES"]), &Method::GET, "/reportes/estadisticas"));
        // El punto de venta crea cotizaciones aunque la pantalla no esté.
        assert!(!bloqueada(&o(&["COTIZACIONES"]), &Method::POST, "/cotizaciones"));
        // Devoluciones de proveedor no son las de clientes.
        assert!(!bloqueada(&o(&["DEVOLUCIONES"]), &Method::POST, "/devoluciones-proveedor"));
    }

    #[test]
    fn lee_la_columna_con_cuidado() {
        assert!(desde_texto(None).is_empty());
        assert!(desde_texto(Some("basura")).is_empty());
        assert_eq!(desde_texto(Some(r#"["REPORTES","CAJA"]"#)), vec!["REPORTES".to_string()]);
    }
}
