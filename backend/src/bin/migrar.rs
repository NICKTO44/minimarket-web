use std::path::PathBuf;

use libsql::Builder;
use minimarket_backend::{crypto, migraciones, tenants::RegistroTiendas};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // Uso:
    //   ./migrar                       -> aplica a TODOS los negocios registrados
    //   ./migrar bodega-juan-83af      -> aplica solo a ese negocio (para probar antes)
    let filtro_identificador = std::env::args().nth(1);

    let central_db_url = std::env::var("CENTRAL_DATABASE_URL").expect("Falta CENTRAL_DATABASE_URL en .env");
    let central_db_token = std::env::var("CENTRAL_AUTH_TOKEN").expect("Falta CENTRAL_AUTH_TOKEN en .env");
    let clave_cifrado = crypto::cargar_clave_desde_env();

    let central_db = Builder::new_remote(central_db_url, central_db_token)
        .build()
        .await
        .expect("No se pudo conectar a la base central");

    let registro = RegistroTiendas::nuevo(central_db, clave_cifrado);

    let todas = registro
        .listar_todas()
        .await
        .expect("No se pudo listar los negocios desde la base central");

    let tiendas: Vec<_> = match &filtro_identificador {
        Some(identificador) => todas
            .into_iter()
            .filter(|t| &t.identificador == identificador)
            .collect(),
        None => todas,
    };

    if tiendas.is_empty() {
        match &filtro_identificador {
            Some(id) => println!("No se encontró ningún negocio con identificador \"{}\".", id),
            None => println!("No hay negocios registrados todavía."),
        }
        return;
    }

    // Carpeta de migraciones junto al binario (se copia al mismo lugar
    // en el Dockerfile).
    let carpeta_migraciones = PathBuf::from("migraciones");

    println!("Aplicando migraciones a {} negocio(s)...\n", tiendas.len());

    let mut ok = 0;
    let mut fallos = 0;

    for tienda in &tiendas {
        match migraciones::aplicar_migraciones_a_tienda(tienda, &carpeta_migraciones).await {
            Ok(aplicadas) if aplicadas.is_empty() => {
                println!("✅ {} ({}) — ya estaba al día", tienda.nombre_negocio, tienda.identificador);
                ok += 1;
            }
            Ok(aplicadas) => {
                println!(
                    "✅ {} ({}) — {} migración(es) nueva(s): {}",
                    tienda.nombre_negocio,
                    tienda.identificador,
                    aplicadas.len(),
                    aplicadas.join(", ")
                );
                ok += 1;
            }
            Err(e) => {
                println!("❌ {} ({}) — FALLÓ: {}", tienda.nombre_negocio, tienda.identificador, e);
                fallos += 1;
            }
        }
    }

    println!("\nResumen: {} correcto(s), {} con error(es).", ok, fallos);

    if fallos > 0 {
        std::process::exit(1);
    }
}