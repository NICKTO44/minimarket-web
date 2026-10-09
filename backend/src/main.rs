use axum::{
    extract::DefaultBodyLimit,
    http::HeaderValue,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer, Any};
use tower_http::services::ServeDir;
use libsql::Builder;

use minimarket_backend::{crypto, estado_impresion, handlers, middleware_auth, migraciones, rate_limit, tenants, AppState};

async fn health() -> &'static str {
    "minimarket-backend OK"
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("TURSO_DATABASE_URL").expect("Falta TURSO_DATABASE_URL en .env");
    let db_token = std::env::var("TURSO_AUTH_TOKEN").expect("Falta TURSO_AUTH_TOKEN en .env");
    let jwt_secret = std::env::var("JWT_SECRET").expect("Falta JWT_SECRET en .env (agrega una clave larga y aleatoria)");
    std::env::var("AGENTE_IMPRESION_TOKEN").expect("Falta AGENTE_IMPRESION_TOKEN en .env (token para el agente de impresión)");

    let central_db_url = std::env::var("CENTRAL_DATABASE_URL").expect("Falta CENTRAL_DATABASE_URL en .env");
    let central_db_token = std::env::var("CENTRAL_AUTH_TOKEN").expect("Falta CENTRAL_AUTH_TOKEN en .env");
    let clave_cifrado = crypto::cargar_clave_desde_env();

    std::fs::create_dir_all("uploads/productos").ok();

    let db = Builder::new_remote(db_url, db_token)
        .build()
        .await
        .expect("No se pudo conectar a Turso");

    let conn = db.connect().expect("No se pudo abrir conexión");
    conn.query("SELECT 1", ())
        .await
        .expect("La conexión a Turso no respondió");
    println!("Conectado a Turso correctamente");

    let central_db = Builder::new_remote(central_db_url, central_db_token)
        .build()
        .await
        .expect("No se pudo conectar a la base central");

    let conn_central = central_db.connect().expect("No se pudo abrir conexión a la base central");
    conn_central
        .query("SELECT 1", ())
        .await
        .expect("La base central no respondió");
    println!("Conectado a la base central correctamente");

    let state = Arc::new(AppState {
        db,
        tiendas: tenants::RegistroTiendas::nuevo(central_db, clave_cifrado),
        estado_impresion: estado_impresion::EstadoImpresion::nuevo(),
        limitador_login: rate_limit::LimitadorIntentos::estricto(),
        limitador_verificar: rate_limit::LimitadorIntentos::laxo(),
        jwt_secret,
    });

    // Migraciones automáticas para TODOS los negocios ya existentes --
    // corre en segundo plano, sin bloquear el arranque del servidor ni
    // el chequeo de salud de deploy.sh. Cada vez que el contenedor se
    // reconstruye (cada deploy), cualquier negocio que ya exista queda
    // al día solo con las migraciones nuevas que haya en migraciones/
    // -- ya no hace falta correr ./migrar a mano, ni para negocios
    // viejos ni para los que se registren después (esos ya quedan al
    // día en el momento de crearse, en registro.rs).
    {
        let state_migraciones = state.clone();
        tokio::spawn(async move {
            println!("🔄 Aplicando migraciones pendientes a los negocios existentes...");
            match state_migraciones.tiendas.listar_todas().await {
                Ok(tiendas) => {
                    let mut ok = 0;
                    let mut fallos = 0;
                    for tienda in &tiendas {
                        match migraciones::aplicar_migraciones_a_tienda(tienda, std::path::Path::new("migraciones")).await {
                            Ok(aplicadas) if aplicadas.is_empty() => ok += 1,
                            Ok(aplicadas) => {
                                println!(
                                    "  ✅ {} ({}) — {} migración(es) nueva(s): {}",
                                    tienda.nombre_negocio,
                                    tienda.identificador,
                                    aplicadas.len(),
                                    aplicadas.join(", ")
                                );
                                ok += 1;
                            }
                            Err(e) => {
                                eprintln!("  ❌ {} ({}) — FALLÓ: {}", tienda.nombre_negocio, tienda.identificador, e);
                                fallos += 1;
                            }
                        }
                    }
                    println!("🔄 Migraciones automáticas: {} al día, {} con error(es).", ok, fallos);

                    // Revisión (solo lectura): ventas que quedaron sin
                    // productos por un corte a mitad de una venta. No toca
                    // nada; solo avisa en el registro para decidir qué hacer.
                    let mut incompletas = 0;
                    for tienda in &tiendas {
                        if let Ok(ventas) = migraciones::ventas_sin_productos(tienda).await {
                            for (folio, fecha, total) in &ventas {
                                println!(
                                    "  ⚠️  {} ({}) — venta sin productos: {} · {} · S/ {:.2}",
                                    tienda.nombre_negocio, tienda.identificador, folio, fecha, total
                                );
                            }
                            incompletas += ventas.len();
                        }
                    }
                    println!("🔎 Ventas sin productos (cortadas a mitad): {}.", incompletas);

                    // Índice central de usuarios: registra a los cajeros que
                    // se crearon antes de que crear_usuario los agregara
                    // (sin esto no pueden iniciar sesión en un dispositivo
                    // nuevo). Solo agrega filas, nunca borra ni modifica.
                    for tienda in &tiendas {
                        match state_migraciones.tiendas.sincronizar_indice_usuarios(tienda).await {
                            Ok((agregados, conflictos)) => {
                                if agregados > 0 {
                                    println!("  👤 {} ({}) — {} usuario(s) agregado(s) al índice de login", tienda.nombre_negocio, tienda.identificador, agregados);
                                }
                                for nombre in conflictos {
                                    eprintln!(
                                        "  ⚠️  {} ({}) — el usuario '{}' ya lo usa otro negocio; no podrá entrar desde un dispositivo nuevo hasta que se le cambie el nombre",
                                        tienda.nombre_negocio, tienda.identificador, nombre
                                    );
                                }
                            }
                            Err(e) => eprintln!("  ❌ {} ({}) — no se pudo sincronizar el índice de usuarios: {}", tienda.nombre_negocio, tienda.identificador, e),
                        }
                    }
                }
                Err(e) => eprintln!("⚠️  No se pudo listar los negocios para aplicar migraciones automáticas: {}", e),
            }
        });
    }

    // Reintento automático de lo que quedó pendiente con SUNAT (solo con
    // SUNAT_REINTENTOS=1 y LYCET_URL en el .env; ver handlers/envios_sunat.rs).
    handlers::envios_sunat::iniciar_tarea(state.clone());

    let origenes_permitidos = AllowOrigin::list([
        HeaderValue::from_static("https://frontend-sigma-three-23.vercel.app"),
        HeaderValue::from_static("http://localhost:5173"),
    ]);

    let cors = CorsLayer::new()
        .allow_origin(origenes_permitidos)
        .allow_methods(Any)
        .allow_headers(Any);

    let rutas_sensibles = Router::new()
        .route("/login", post(handlers::auth::login))
        .route("/login/identificar", post(handlers::auth::identificar_usuario))
        .route("/registro", post(handlers::registro::registrar_negocio))
        .route("/panel/login", post(handlers::panel::login))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit::limitar_login));

    let rutas_verificacion = Router::new()
        .route("/registro/verificar-usuario", get(handlers::registro::verificar_usuario))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit::limitar_verificar));

    let rutas_autenticacion = rutas_sensibles.merge(rutas_verificacion);

    let rutas_publicas = Router::new()
        .route("/", get(health))
        .route("/agente-impresion/ws", get(handlers::agente_impresion::agente_websocket))
        .route("/publico/comprobante/:identificador/:id", get(handlers::publico::ver_comprobante_publico))
        .nest_service("/uploads", ServeDir::new("uploads"))
        .merge(rutas_autenticacion);

    let rutas_protegidas = Router::new()
        .route("/productos", get(handlers::productos::listar_productos))
        .route("/productos", post(handlers::productos::agregar_producto))
        .route("/productos/stock-bajo", get(handlers::productos::productos_stock_bajo))
        // Importar desde Excel/CSV: hasta 5,000 filas en una sola petición.
        .route(
            "/productos/importar",
            post(handlers::importacion::importar_productos).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route("/productos/desactivados", get(handlers::productos::listar_productos_desactivados))
        .route("/productos/:id", axum::routing::put(handlers::productos::actualizar_producto))
        .route("/productos/:id", axum::routing::delete(handlers::productos::eliminar_producto))
        .route("/productos/:id/desactivar", post(handlers::productos::desactivar_producto))
        .route("/productos/:id/reactivar", post(handlers::productos::reactivar_producto))
        // Axum corta por defecto en 2 MB y una foto de celular pesa más: el
        // handler ya valida su propio máximo (10 MB imagen, 5 MB logo).
        .route(
            "/productos/:id/imagen",
            post(handlers::imagenes::subir_imagen_producto).layer(DefaultBodyLimit::max(12 * 1024 * 1024)),
        )
        .route("/categorias", get(handlers::productos::obtener_categorias))
        .route("/categorias", post(handlers::productos::crear_categoria))
        .route("/categorias/:id/igv", axum::routing::put(handlers::igv::cambiar_igv_categoria))
        .route("/igv/venta/:id", get(handlers::igv::obtener_desglose))
        .route("/productos/:id/igv", axum::routing::put(handlers::igv::cambiar_igv_producto))
        .route("/clientes", get(handlers::clientes::buscar_clientes))
        .route("/documentos/consultar", get(handlers::documentos::consultar_documento))
        .route("/clientes", post(handlers::clientes::crear_cliente))
        .route("/clientes/todos", get(handlers::clientes::listar_clientes))
        .route("/clientes/:id", axum::routing::put(handlers::clientes::actualizar_cliente))
        .route("/clientes/:id/desactivar", post(handlers::clientes::desactivar_cliente))
        .route("/clientes/desactivados", get(handlers::clientes::listar_clientes_desactivados))
        .route("/clientes/:id/reactivar", post(handlers::clientes::reactivar_cliente))
        .route("/ventas", post(handlers::ventas::procesar_venta))
        .route("/ventas/:identificador", get(handlers::devoluciones::buscar_venta_para_devolucion))
        .route("/devoluciones", post(handlers::devoluciones::procesar_devolucion))
        .route("/lotes", post(handlers::lotes::agregar_lote))
        .route("/productos/:id/lotes", get(handlers::lotes::obtener_lotes_de_producto))
        .route("/lotes/por-vencer", get(handlers::lotes::lotes_por_vencer))
        .route("/lotes/:id/descartar", post(handlers::lotes::descartar_lote))
        .route("/cajas/abrir", post(handlers::cajas::abrir_caja))
        .route("/cajas/cerrar", post(handlers::cajas::cerrar_caja))
        .route("/cajas/movimiento", post(handlers::cajas::registrar_movimiento))
        .route("/cajas/abierta", get(handlers::cajas::obtener_caja_abierta))
        .route("/cajas/movimientos", get(handlers::cajas::movimientos_caja_abierta))
        .route("/cajas/:id/detalle", get(handlers::cajas::detalle_caja))
        // Gastos del negocio (alquiler, luz, sueldos...); ver handlers/gastos.rs.
        .route("/gastos", get(handlers::gastos::listar).post(handlers::gastos::registrar))
        .route("/gastos/:id/anular", post(handlers::gastos::anular))
        .route("/gastos/categorias", get(handlers::gastos::listar_categorias).post(handlers::gastos::crear_categoria))
        .route("/gastos/categorias/:id", axum::routing::put(handlers::gastos::actualizar_categoria))
        .route("/cajas", get(handlers::cajas::listar_cajas))
        .route("/proveedores", get(handlers::proveedores::obtener_proveedores))
        .route("/proveedores", post(handlers::proveedores::agregar_proveedor))
        .route("/compras", post(handlers::proveedores::crear_compra))
        .route("/compras", get(handlers::proveedores::listar_compras))
        .route("/compras/:id", get(handlers::proveedores::detalle_compra))
        .route("/compras/recibir", post(handlers::proveedores::recibir_mercaderia))
        .route("/devoluciones-proveedor", post(handlers::devoluciones_proveedor::registrar_devolucion))
        .route("/devoluciones-proveedor", get(handlers::devoluciones_proveedor::listar_devoluciones))
        .route("/devoluciones-proveedor/:id/resolver", post(handlers::devoluciones_proveedor::resolver_devolucion))
        .route("/reportes/ventas", get(handlers::reportes::ventas_por_rango))
        .route("/reportes/productos-vendidos", get(handlers::reportes::productos_mas_vendidos))
        .route("/reportes/estadisticas", get(handlers::reportes::estadisticas_completas))
        .route("/reportes/ganancias", get(handlers::ganancias::reporte))
        .route("/ganancias/costos", get(handlers::ganancias::costos))
        .route("/comprobantes", post(handlers::facturacion::emitir_comprobante))
        .route("/comprobantes", get(handlers::comprobantes::listar_comprobantes))
        .route("/comprobantes/:id/pdf", get(handlers::comprobantes::descargar_pdf))
        .route("/comprobantes/:id/xml", get(handlers::comprobantes::descargar_xml))
        .route("/comprobantes/:id/cdr", get(handlers::comprobantes::descargar_cdr))
        .route("/comprobantes/:id/documento", get(handlers::comprobantes::documento_enviado))
        // Emisión directa: reenviar un pendiente y avisos de plazos de SUNAT.
        .route("/comprobantes/:id/reenviar", post(handlers::envios_sunat::reenviar))
        .route("/sunat/avisos", get(handlers::envios_sunat::avisos))
        // Notas de crédito (emisión directa); ver handlers/notas_credito.rs.
        .route(
            "/comprobantes/:id/nota-credito",
            get(handlers::notas_credito::preparar).post(handlers::notas_credito::emitir),
        )
        .route("/notas-credito/:id/reenviar", post(handlers::notas_credito::reenviar))
        // Anulación (baja de facturas, resumen diario de boletas); ver handlers/anulaciones.rs.
        .route("/comprobantes/:id/anulacion", get(handlers::anulaciones::preparar))
        .route("/comprobantes/:id/anular", post(handlers::anulaciones::anular))
        .route("/comprobantes/:id/anulacion/consultar", post(handlers::anulaciones::consultar))
        .route("/notas-credito/:id/documento", get(handlers::notas_credito::documento))
        .route("/notas-credito/:id/xml", get(handlers::notas_credito::descargar_xml))
        .route("/notas-credito/:id/cdr", get(handlers::notas_credito::descargar_cdr))
        .route("/impresora/imprimir", post(handlers::impresora::imprimir_boleta))
        .route("/configuracion", get(handlers::configuracion::obtener_configuracion))
        .route("/configuracion", axum::routing::put(handlers::configuracion::actualizar_configuracion))
        .route(
            "/configuracion/logo",
            post(handlers::imagenes::subir_logo_tienda).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route("/usuarios", get(handlers::configuracion::listar_usuarios))
        .route("/usuarios", post(handlers::configuracion::crear_usuario))
        .route("/usuarios/:id/desactivar", post(handlers::configuracion::desactivar_usuario))
        .route("/usuarios/:id/reactivar", post(handlers::configuracion::reactivar_usuario))
        .route("/configuracion/modo-negocio", axum::routing::put(handlers::mesas::cambiar_modo_negocio))
        .route("/roles", get(handlers::mesas::listar_roles))
        // Módulo Cafetería / Restaurante (responde 403 si el negocio no lo activó)
        .route("/mesas", get(handlers::mesas::listar_mesas))
        .route("/mesas", post(handlers::mesas::crear_mesa))
        .route("/mesas/:id", axum::routing::put(handlers::mesas::actualizar_mesa))
        .route("/mesas/:id/desactivar", post(handlers::mesas::desactivar_mesa))
        .route("/pedidos", post(handlers::mesas::abrir_pedido))
        .route("/pedidos/abiertos", get(handlers::mesas::listar_pedidos_abiertos))
        .route("/pedidos/:id", get(handlers::mesas::obtener_pedido))
        .route("/pedidos/:id/items", post(handlers::mesas::agregar_items))
        .route("/pedidos/:id/items/:item_id", axum::routing::put(handlers::mesas::cambiar_cantidad_item))
        .route("/pedidos/:id/items/:item_id/quitar", post(handlers::mesas::quitar_item))
        .route("/pedidos/:id/enviar", post(handlers::mesas::enviar_a_preparar))
        .route("/pedidos/:id/mover", post(handlers::mesas::mover_pedido))
        .route("/pedidos/:id/anular", post(handlers::mesas::anular_pedido))
        .route("/preparacion", get(handlers::mesas::listar_preparacion))
        .route("/preparacion/listo", post(handlers::mesas::marcar_listo))
        .route("/preparacion/entregado", post(handlers::mesas::marcar_entregado))
        .route("/modificadores", get(handlers::mesas::listar_modificadores))
        .route("/modificadores", post(handlers::mesas::crear_grupo_modificador))
        .route("/modificadores/:id", axum::routing::put(handlers::mesas::actualizar_grupo_modificador))
        .route("/modificadores/:id/desactivar", post(handlers::mesas::desactivar_grupo_modificador))
        .route("/negocio", get(handlers::rubros::obtener_negocio))
        .route("/configuracion/negocio", axum::routing::put(handlers::rubros::guardar_negocio))
        .route("/detraccion", get(handlers::detraccion::obtener_detraccion))
        .route("/cotizaciones", get(handlers::cotizaciones::listar).post(handlers::cotizaciones::crear))
        .route("/cotizaciones/:id", get(handlers::cotizaciones::obtener))
        .route("/cotizaciones/:id/anular", post(handlers::cotizaciones::anular))
        .route("/creditos", get(handlers::creditos::listar))
        .route("/creditos/cliente/:id", get(handlers::creditos::deuda_cliente))
        .route("/creditos/:id", get(handlers::creditos::obtener))
        .route("/creditos/:id/abonos", post(handlers::creditos::abonar))
        .route("/guias", get(handlers::guias::listar).post(handlers::guias::crear))
        .route("/guias/config", get(handlers::guias::obtener_config))
        .route("/guias/venta/:folio", get(handlers::guias::venta_para_guia))
        .route("/guias/:id", get(handlers::guias::detalle))
        .route("/guias/:id/consultar", post(handlers::guias::consultar))
        .route("/guias/:id/xml", get(handlers::guias_directas::descargar_xml))
        .route("/guias/:id/cdr", get(handlers::guias_directas::descargar_cdr))
        .route("/configuracion/guias", axum::routing::put(handlers::guias::guardar_config))
        .route("/configuracion/detraccion", axum::routing::put(handlers::detraccion::guardar_detraccion))
        // Ropa y calzado: tallas y colores, y cambio de prenda.
        .route("/modelos", post(handlers::variantes::crear_modelo))
        .route("/modelos/:id", axum::routing::put(handlers::variantes::actualizar_modelo))
        .route("/modelos/:id/imagen/:producto_id", post(handlers::variantes::compartir_imagen))
        .route("/cambios/config", get(handlers::cambios::obtener_config))
        .route("/cambios/venta/:identificador", get(handlers::cambios::venta_para_cambio))
        .route("/configuracion/cambios", axum::routing::put(handlers::cambios::guardar_config))
        .route("/unidades", get(handlers::unidades::listar_unidades))
        .route("/configuracion/unidades", axum::routing::put(handlers::unidades::guardar_unidades))
        .route("/carta-dia", get(handlers::carta::listar_carta))
        .route("/carta-dia", post(handlers::carta::agregar_plato))
        .route("/carta-dia/:id", axum::routing::put(handlers::carta::actualizar_plato))
        .route("/carta-dia/:id/quitar", post(handlers::carta::quitar_plato))
        .route("/suscripcion/canjear-codigo", post(handlers::suscripcion::canjear_codigo))
        .route("/suscripcion/estado", get(handlers::suscripcion::estado_suscripcion))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), middleware_auth::requiere_auth));

    // Panel de Monspeet (el dueño del sistema, no un negocio). Su propio
    // login y su propio middleware; apagado si faltan PANEL_USUARIO y
    // PANEL_CLAVE en el .env. Ver handlers/panel.rs.
    let rutas_panel = Router::new()
        .route("/panel/negocios", get(handlers::panel::listar_negocios))
        .route("/panel/negocios/:id/renovar", post(handlers::panel::renovar))
        .route("/panel/negocios/:id/estado", post(handlers::panel::cambiar_estado))
        .route("/panel/negocios/:id/vencimiento", axum::routing::put(handlers::panel::fijar_vencimiento))
        .route("/panel/negocios/:id/facturacion", get(handlers::panel::detalle_facturacion))
        .route("/panel/negocios/:id/facturacion/datos", axum::routing::put(handlers::panel::guardar_datos))
        .route(
            "/panel/negocios/:id/facturacion/alta",
            post(handlers::panel::dar_de_alta).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/panel/negocios/:id/facturacion/modo", axum::routing::put(handlers::panel::cambiar_modo))
        .route("/panel/negocios/:id/facturacion/probar", post(handlers::panel::probar_conexion))
        .route("/panel/codigos", get(handlers::panel::listar_codigos).post(handlers::panel::generar_codigo_panel))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), handlers::panel::requiere_panel));

    let app = rutas_publicas
        .merge(rutas_protegidas)
        .merge(rutas_panel)
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Backend corriendo en http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}