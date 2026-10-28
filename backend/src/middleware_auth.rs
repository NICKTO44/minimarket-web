use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    http::{Method, StatusCode, header},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::models::auth::Claims;
use crate::tenants::{NivelAcceso, TenantDb};
use crate::AppState;

/// Saca el token del query string (?token=...) — usado solo por el <iframe>
/// del PDF embebido, que no puede mandar cabeceras personalizadas.
fn extraer_token_de_query(req: &Request) -> Option<String> {
    req.uri().query().and_then(|q| {
        q.split('&').find_map(|par| {
            let mut it = par.splitn(2, '=');
            let clave = it.next()?;
            let valor = it.next()?;
            (clave == "token").then(|| valor.to_string())
        })
    })
}

pub async fn requiere_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let token = match auth_header.or_else(|| extraer_token_de_query(&req)) {
        Some(t) if !t.is_empty() => t,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    // Antes: std::env::var("JWT_SECRET").expect(...) en cada petición.
    // Ahora: ya viene cargado una sola vez en AppState desde el arranque.
    let datos = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let claims = datos.claims;

    // Resuelve a qué tienda pertenece este usuario (de la caché en memoria
    // si ya se consultó antes).
    let tienda = state
        .tiendas
        .resolver_por_id(claims.tienda_id)
        .await
        .map_err(|e| {
            eprintln!("❌ Error resolviendo tienda_id {}: {}", claims.tienda_id, e);
            StatusCode::UNAUTHORIZED
        })?;

    // Control de suscripción: tres niveles.
    //  - Bloqueado: ni entrar puede (casos extremos, no el flujo normal
    //    de "no pagó todavía").
    //  - SoloLectura: puede seguir viendo su información (peticiones
    //    GET), pero cualquier escritura (procesar venta, editar algo)
    //    se corta con 402 Payment Required — EXCEPTO canjear un código
    //    de activación, que es justo el mecanismo para salir de este
    //    estado por sí mismo.
    //  - Completo: sin restricciones.
    // Cambiar la propia contraseña o cerrar las otras sesiones también se
    // puede en modo lectura: es seguridad de la cuenta, no uso del sistema.
    let es_canje_de_codigo = req.uri().path() == "/suscripcion/canjear-codigo"
        || es_de(req.uri().path(), "/mi-cuenta");

    match tienda.nivel_acceso() {
        NivelAcceso::Bloqueado(motivo) => {
            eprintln!("⛔ Acceso bloqueado a tienda '{}': {}", tienda.identificador, motivo);
            return Err(StatusCode::FORBIDDEN);
        }
        NivelAcceso::SoloLectura(motivo)
            if !es_canje_de_codigo && req.method() != Method::GET && req.method() != Method::HEAD =>
        {
            eprintln!("💳 Escritura bloqueada (modo lectura) a tienda '{}': {}", tienda.identificador, motivo);
            return Err(StatusCode::PAYMENT_REQUIRED);
        }
        _ => {}
    }

    // Antes: se reconstruía el Database (Builder::new_remote + build)
    // en cada petición. Ahora: se reutiliza el ya armado para esa tienda,
    // si existe en caché.
    let db_tienda = state
        .tiendas
        .conectar_cacheado(&tienda)
        .await
        .map_err(|e| {
            eprintln!("❌ Error conectando a la base de la tienda '{}': {}", tienda.identificador, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // ¿El usuario sigue activo? Un usuario desactivado pierde el acceso
    // aunque su JWT siga vigente (401 -> el frontend lo manda al login).
    // El rol también sale de la base, no solo del token. Si la consulta
    // falla por un problema de red, no se bloquea la operación (mismo
    // criterio que con servicios externos): se deja pasar y se registra.
    let mut claims = claims;
    match state.tiendas.estado_usuario(claims.tienda_id, claims.sub, &db_tienda).await {
        // La versión de la sesión subió (cambió o le restablecieron la
        // contraseña, o cerró sus otras sesiones): este JWT ya no vale.
        Ok(Some((true, _, version))) if version != claims.tv => {
            eprintln!("🔑 Sesión anterior de {} en '{}' cerrada (versión {} != {})", claims.username, tienda.identificador, claims.tv, version);
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(Some((true, rol_id, _))) => claims.rol_id = rol_id,
        Ok(Some((false, _, _))) | Ok(None) => {
            eprintln!("🚫 Usuario {} de la tienda '{}' desactivado o inexistente", claims.username, tienda.identificador);
            return Err(StatusCode::UNAUTHORIZED);
        }
        Err(e) => eprintln!("⚠️  No se pudo verificar si el usuario {} está activo: {}", claims.username, e),
    }

    // Roles con acceso limitado: el Mesero solo maneja mesas y pedidos;
    // Preparación (barra/cocina) solo su pantalla; Almacén solo productos,
    // stock, proveedores y reportes.
    // Se bloquea en el servidor, no solo se esconde en el menú.
    if let Some(rol) = nombre_rol_cacheado(claims.tienda_id, claims.rol_id, &db_tienda).await {
        if !ruta_permitida(&rol, req.method(), req.uri().path()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    req.extensions_mut().insert(claims);
    req.extensions_mut().insert(Arc::new(TenantDb(db_tienda)));

    Ok(next.run(req).await)
}

/// (tienda_id, rol_id) -> nombre del rol. Los roles casi nunca cambian, así
/// que se consulta una sola vez por negocio y rol (no en cada petición).
fn cache_roles() -> &'static Mutex<HashMap<(i64, i64), String>> {
    static CACHE: OnceLock<Mutex<HashMap<(i64, i64), String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn nombre_rol_cacheado(tienda_id: i64, rol_id: i64, db: &libsql::Database) -> Option<String> {
    // El administrador (rol 1) nunca tiene restricciones: ni se consulta.
    if rol_id == ROL_ADMIN {
        return None;
    }
    if let Some(nombre) = cache_roles().lock().ok()?.get(&(tienda_id, rol_id)).cloned() {
        return Some(nombre);
    }
    let conn = db.connect().ok()?;
    let nombre = crate::handlers::mesas::nombre_rol(&conn, rol_id).await?;
    cache_roles().lock().ok()?.insert((tienda_id, rol_id), nombre.clone());
    Some(nombre)
}

/// Qué puede usar cada rol limitado. Cualquier otro rol: todo (como antes).
/// "Carta de hoy": la arman el mozo y la barra/cocina (el handler además
/// impide que el cajero la cambie).
fn es_carta(ruta: &str) -> bool {
    ruta == "/carta-dia" || ruta.starts_with("/carta-dia/")
}

/// true si la ruta es esa sección o algo dentro de ella ("/productos",
/// "/productos/5/lotes"), pero no otra que solo empieza igual.
fn es_de(ruta: &str, seccion: &str) -> bool {
    ruta.strip_prefix(seccion).is_some_and(|resto| resto.is_empty() || resto.starts_with('/'))
}

fn ruta_permitida(rol: &str, metodo: &Method, ruta: &str) -> bool {
    let lectura = metodo == Method::GET || metodo == Method::HEAD;
    // Lo mínimo para que la app cargue (nombre del negocio, aviso de pago),
    // y la propia cuenta (cambiar su contraseña, cerrar sus otras sesiones).
    let basico = (lectura && (ruta == "/configuracion" || ruta == "/suscripcion/estado"))
        || es_de(ruta, "/mi-cuenta");
    match rol {
        "MESERO" => {
            basico
                || ruta == "/pedidos"
                || ruta.starts_with("/pedidos/")
                || ruta == "/preparacion/entregado"
                || es_carta(ruta)
                || (lectura
                    && matches!(ruta, "/mesas" | "/productos" | "/categorias" | "/modificadores" | "/preparacion"))
        }
        // También puede marcar "entregado": en local chico la barra entrega
        // en el mostrador, y así limpia lo que quedó pendiente.
        "PREPARACION" => {
            basico
                || (lectura && ruta == "/preparacion")
                || ruta == "/preparacion/listo"
                || ruta == "/preparacion/entregado"
                || es_carta(ruta)
        }
        // Almacén (rol INVENTARIO): lleva los productos, el stock y los
        // proveedores, y mira los reportes de ventas. No vende, no toca la
        // caja ni ve clientes o comprobantes. Lo que dentro de estas rutas
        // es solo del administrador (importar, cambiar el IGV, el reporte
        // de ganancias) lo sigue cortando cada handler.
        "INVENTARIO" => {
            basico
                || es_de(ruta, "/productos")
                || es_de(ruta, "/categorias")
                || es_de(ruta, "/modelos")
                || es_de(ruta, "/lotes")
                || es_de(ruta, "/proveedores")
                || es_de(ruta, "/compras")
                || es_de(ruta, "/devoluciones-proveedor")
                // "/ganancias/costos": el costo promedio que muestra el
                // formulario del producto (con el módulo de ganancias).
                || (lectura && (matches!(ruta, "/unidades" | "/ganancias/costos") || es_de(ruta, "/reportes")))
                // Fichas de repuesto y compatibilidades (lubricentro), para
                // la lista de Productos.
                || (lectura && es_de(ruta, "/repuestos"))
                // Producción del día y mermas (pastelería, panadería). Las
                // recetas las guarda solo el administrador (lo corta el handler).
                || es_de(ruta, "/produccion")
                || es_de(ruta, "/mermas")
                || (lectura && es_de(ruta, "/formulas"))
        }
        // Repartidor (dark kitchen, módulo DELIVERY): sus entregas y el cobro
        // contra entrega delante del cliente (el punto de venta con el pedido
        // cargado: carta, clientes para la boleta, la venta y su comprobante
        // para enviarlo por WhatsApp). Qué pedido puede cobrar lo revisa
        // ventas.rs (solo los suyos, en camino y contra entrega).
        "REPARTIDOR" => {
            basico
                || es_de(ruta, "/reparto")
                || (metodo == Method::POST && ruta == "/ventas")
                || (metodo == Method::POST && ruta == "/clientes")
                || (lectura
                    && (matches!(ruta, "/productos" | "/categorias" | "/unidades" | "/clientes" | "/documentos/consultar" | "/negocio")
                        || es_de(ruta, "/pedidos")
                        || es_de(ruta, "/comprobantes")))
        }
        _ => true,
    }
}

/// Nombre del rol (con la misma caché que el control de rutas), para los
/// handlers que deciden algo según el rol. None = administrador.
pub async fn rol_de(claims: &Claims, db: &libsql::Database) -> Option<String> {
    nombre_rol_cacheado(claims.tienda_id, claims.rol_id, db).await
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn permisos_por_rol() {
        let g = Method::GET;
        let p = Method::POST;
        assert!(ruta_permitida("MESERO", &g, "/mesas"));
        assert!(ruta_permitida("MESERO", &p, "/pedidos/5/items"));
        assert!(ruta_permitida("MESERO", &p, "/preparacion/entregado"));
        assert!(!ruta_permitida("MESERO", &p, "/preparacion/listo"));
        assert!(!ruta_permitida("MESERO", &p, "/ventas"));
        assert!(!ruta_permitida("MESERO", &g, "/reportes/ventas"));
        assert!(!ruta_permitida("MESERO", &p, "/mesas"));
        assert!(ruta_permitida("PREPARACION", &g, "/preparacion"));
        assert!(ruta_permitida("PREPARACION", &p, "/preparacion/listo"));
        assert!(!ruta_permitida("PREPARACION", &g, "/mesas"));
        assert!(!ruta_permitida("PREPARACION", &g, "/productos"));
        assert!(!ruta_permitida("PREPARACION", &p, "/pedidos/5/items"));
        assert!(ruta_permitida("PREPARACION", &p, "/preparacion/entregado"));
        assert!(ruta_permitida("PREPARACION", &g, "/configuracion"));
        assert!(!ruta_permitida("PREPARACION", &axum::http::Method::PUT, "/configuracion"));
        assert!(ruta_permitida("MESERO", &p, "/carta-dia"));
        assert!(ruta_permitida("MESERO", &Method::PUT, "/carta-dia/7"));
        assert!(ruta_permitida("PREPARACION", &g, "/carta-dia"));
        assert!(ruta_permitida("PREPARACION", &Method::PUT, "/carta-dia/7"));
        assert!(ruta_permitida("PREPARACION", &p, "/carta-dia/7/quitar"));
        assert!(!ruta_permitida("PREPARACION", &p, "/carta-diaria"));
        assert!(ruta_permitida("CAJERO", &p, "/ventas"));
        assert!(ruta_permitida("CAJERO", &p, "/cajas/abrir"));
        assert!(ruta_permitida("CAJERO", &g, "/clientes/todos"));
        // Repartidor: sus entregas y el cobro en la puerta; nada más.
        assert!(ruta_permitida("REPARTIDOR", &g, "/reparto"));
        assert!(ruta_permitida("REPARTIDOR", &p, "/reparto/5/salir"));
        assert!(ruta_permitida("REPARTIDOR", &p, "/ventas"));
        assert!(ruta_permitida("REPARTIDOR", &g, "/pedidos/5"));
        assert!(ruta_permitida("REPARTIDOR", &g, "/comprobantes/9/documento"));
        assert!(ruta_permitida("REPARTIDOR", &p, "/clientes"));
        assert!(!ruta_permitida("REPARTIDOR", &p, "/pedidos/5/items"));
        assert!(!ruta_permitida("REPARTIDOR", &g, "/delivery"));
        assert!(!ruta_permitida("REPARTIDOR", &p, "/delivery/5/asignar"));
        assert!(!ruta_permitida("REPARTIDOR", &g, "/ventas"));
        assert!(!ruta_permitida("REPARTIDOR", &g, "/reportes/ventas"));
        assert!(!ruta_permitida("REPARTIDOR", &p, "/cajas/abrir"));
        assert!(!ruta_permitida("REPARTIDOR", &p, "/productos"));
        // Su propia cuenta: todos los roles.
        for rol in ["MESERO", "PREPARACION", "INVENTARIO", "CAJERO", "REPARTIDOR"] {
            assert!(ruta_permitida(rol, &Method::PUT, "/mi-cuenta/clave"), "{}", rol);
            assert!(ruta_permitida(rol, &p, "/mi-cuenta/cerrar-sesiones"), "{}", rol);
        }
        assert!(!ruta_permitida("MESERO", &p, "/mi-cuenta-falsa"));
        // Restablecer la clave de otro: no es "su cuenta".
        assert!(!ruta_permitida("MESERO", &p, "/usuarios/3/clave"));
        assert!(!ruta_permitida("INVENTARIO", &p, "/usuarios/3/clave"));
    }

    /// Almacén: todo lo que usan Productos, Stock y Lotes, Proveedores y
    /// Reportes; nada de vender, caja, clientes, comprobantes ni ajustes.
    #[test]
    fn permisos_de_almacen() {
        let g = Method::GET;
        let p = Method::POST;
        let u = Method::PUT;
        let d = Method::DELETE;
        let puede = [
            // La app carga
            (&g, "/configuracion"),
            (&g, "/suscripcion/estado"),
            // Productos
            (&g, "/productos"),
            (&p, "/productos"),
            (&u, "/productos/8"),
            (&d, "/productos/8"),
            (&p, "/productos/8/desactivar"),
            (&p, "/productos/8/reactivar"),
            (&g, "/productos/desactivados"),
            (&g, "/productos/stock-bajo"),
            (&p, "/productos/8/imagen"),
            (&g, "/productos/8/lotes"),
            (&g, "/categorias"),
            (&p, "/categorias"),
            (&p, "/modelos"),
            (&u, "/modelos/3"),
            (&p, "/modelos/3/imagen/8"),
            (&g, "/unidades"),
            (&g, "/ganancias/costos"),
            // Stock y lotes
            (&p, "/lotes"),
            (&g, "/lotes/por-vencer"),
            (&p, "/lotes/4/descartar"),
            // Proveedores
            (&g, "/proveedores"),
            (&p, "/proveedores"),
            (&g, "/compras"),
            (&p, "/compras"),
            (&g, "/compras/6"),
            (&p, "/compras/recibir"),
            (&g, "/devoluciones-proveedor"),
            (&p, "/devoluciones-proveedor"),
            (&p, "/devoluciones-proveedor/2/resolver"),
            // Reportes
            (&g, "/reportes/ventas"),
            (&g, "/reportes/productos-vendidos"),
            (&g, "/reportes/estadisticas"),
        ];
        for (metodo, ruta) in puede {
            assert!(ruta_permitida("INVENTARIO", metodo, ruta), "debería poder: {} {}", metodo, ruta);
        }
        let no_puede = [
            (&p, "/ventas"),
            (&g, "/ventas/V-1"),
            (&p, "/devoluciones"),
            (&g, "/cajas/abierta"),
            (&p, "/cajas/abrir"),
            (&p, "/cajas/movimiento"),
            (&g, "/clientes"),
            (&g, "/clientes/todos"),
            (&g, "/comprobantes"),
            (&g, "/comprobantes/4/pdf"),
            (&g, "/cotizaciones"),
            (&g, "/creditos"),
            (&g, "/guias"),
            (&g, "/mesas"),
            (&p, "/pedidos"),
            (&g, "/preparacion"),
            (&g, "/carta-dia"),
            (&g, "/usuarios"),
            (&p, "/usuarios"),
            (&g, "/roles"),
            (&g, "/negocio"),
            (&u, "/configuracion"),
            (&u, "/configuracion/negocio"),
            (&u, "/configuracion/unidades"),
            (&p, "/configuracion/logo"),
            (&p, "/suscripcion/canjear-codigo"),
            (&p, "/impresora/imprimir"),
            (&p, "/ganancias/costos"),
            (&g, "/ganancias"),
            (&g, "/igv/venta/3"),
            (&g, "/documentos/consultar"),
            (&g, "/gastos"),
            (&p, "/gastos"),
            (&g, "/gastos/categorias"),
            (&g, "/cajas/movimientos"),
            // Empiezan parecido a una ruta permitida, pero son otra cosa.
            (&g, "/productos-secretos"),
            (&p, "/reportes/ventas"),
            (&u, "/unidades"),
        ];
        for (metodo, ruta) in no_puede {
            assert!(!ruta_permitida("INVENTARIO", metodo, ruta), "no debería poder: {} {}", metodo, ruta);
        }
    }
}

/// rol_id del administrador del negocio (tabla roles).
pub const ROL_ADMIN: i64 = 1;

/// Corta la petición si quien la hace no es administrador. El rol sale del
/// JWT (firmado por el backend), nunca de lo que mande el navegador.
/// Configuración y Suscripción son solo para el administrador; el cajero
/// usa todo lo demás.
pub fn exigir_admin(claims: &Claims) -> Result<(), (StatusCode, String)> {
    if claims.rol_id != ROL_ADMIN {
        return Err((StatusCode::FORBIDDEN, "Solo el administrador del negocio puede hacer esto.".to_string()));
    }
    Ok(())
}
