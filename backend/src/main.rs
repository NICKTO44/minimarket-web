use axum::{
    http::HeaderValue,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer, Any};
use tower_http::services::ServeDir;
use libsql::Builder;

mod models;
mod handlers;
mod logica;
mod middleware_auth;
mod estado_impresion;
mod tenants;
mod rate_limit;
mod crypto;

pub struct AppState {
    pub db: libsql::Database,
    pub tiendas: tenants::RegistroTiendas,
    pub estado_impresion: Arc<estado_impresion::EstadoImpresion>,
    pub limitador_login: Arc<rate_limit::LimitadorIntentos>,
}

async fn health() -> &'static str {
    "minimarket-backend OK"
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("TURSO_DATABASE_URL").expect("Falta TURSO_DATABASE_URL en .env");
    let db_token = std::env::var("TURSO_AUTH_TOKEN").expect("Falta TURSO_AUTH_TOKEN en .env");
    std::env::var("JWT_SECRET").expect("Falta JWT_SECRET en .env (agrega una clave larga y aleatoria)");
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
        limitador_login: rate_limit::LimitadorIntentos::nuevo(),
    });

    // Solo estos dos orígenes pueden llamar a la API — tu frontend real
    // en Vercel, y tu entorno de desarrollo local. Cuando tengas dominio
    // propio para minimarket-web, se agrega aquí.
    let origenes_permitidos = AllowOrigin::list([
        HeaderValue::from_static("https://frontend-sigma-three-23.vercel.app"),
        HeaderValue::from_static("http://localhost:5173"),
    ]);

    let cors = CorsLayer::new()
        .allow_origin(origenes_permitidos)
        .allow_methods(Any)
        .allow_headers(Any);

    let rutas_autenticacion = Router::new()
        .route("/login", post(handlers::auth::login))
        .route("/registro", post(handlers::registro::registrar_negocio))
        .route("/registro/verificar-usuario", get(handlers::registro::verificar_usuario))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit::limitar_intentos));

    let rutas_publicas = Router::new()
        .route("/", get(health))
        .route("/agente-impresion/ws", get(handlers::agente_impresion::agente_websocket))
        .nest_service("/uploads", ServeDir::new("uploads"))
        .merge(rutas_autenticacion);

    let rutas_protegidas = Router::new()
        .route("/productos", get(handlers::productos::listar_productos))
        .route("/productos", post(handlers::productos::agregar_producto))
        .route("/productos/stock-bajo", get(handlers::productos::productos_stock_bajo))
        .route("/productos/:id", axum::routing::put(handlers::productos::actualizar_producto))
        .route("/productos/:id", axum::routing::delete(handlers::productos::eliminar_producto))
        .route("/productos/:id/desactivar", post(handlers::productos::desactivar_producto))
        .route("/productos/:id/imagen", post(handlers::imagenes::subir_imagen_producto))
        .route("/categorias", get(handlers::productos::obtener_categorias))
        .route("/clientes", get(handlers::clientes::buscar_clientes))
        .route("/clientes", post(handlers::clientes::crear_cliente))
        .route("/clientes/todos", get(handlers::clientes::listar_clientes))
        .route("/clientes/:id", axum::routing::put(handlers::clientes::actualizar_cliente))
        .route("/clientes/:id/desactivar", post(handlers::clientes::desactivar_cliente))
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
        .route("/comprobantes", post(handlers::facturacion::emitir_comprobante))
        .route("/comprobantes", get(handlers::comprobantes::listar_comprobantes))
        .route("/comprobantes/:id/pdf", get(handlers::comprobantes::descargar_pdf))
        .route("/impresora/imprimir", post(handlers::impresora::imprimir_boleta))
        .route("/configuracion", get(handlers::configuracion::obtener_configuracion))
        .route("/configuracion", axum::routing::put(handlers::configuracion::actualizar_configuracion))
        .route("/usuarios", get(handlers::configuracion::listar_usuarios))
        .route("/usuarios", post(handlers::configuracion::crear_usuario))
        .route("/usuarios/:id/desactivar", post(handlers::configuracion::desactivar_usuario))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), middleware_auth::requiere_auth));

    let app = rutas_publicas
        .merge(rutas_protegidas)
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Backend corriendo en http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}