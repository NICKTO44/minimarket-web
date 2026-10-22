//! Registra en la base central un negocio cuya base YA EXISTE en Turso.
//! Herramienta de DESARROLLO: sirve para dar de alta en la central de
//! desarrollo las copias de las demos. No entra en la imagen de producción
//! (el Dockerfile copia solo los binarios del servidor).
//!
//!   cargo run --bin importar_negocio -- --id 15 --nombre "farme" \
//!       --identificador farme-3449 --base farme-3449 --slug farme
//!
//! Usa el .env de donde se ejecuta: la central (CENTRAL_DATABASE_URL), la
//! organización de Turso (TURSO_API_TOKEN, TURSO_ORG) y la clave para cifrar
//! el token (CENTRAL_ENCRYPTION_KEY). El token de la base lo pide a la API de
//! Turso, igual que el registro de negocios: nadie tiene que copiarlo a mano.
//!
//! Nunca modifica ni borra nada: si el id o el identificador ya existen en
//! la central, se detiene sin escribir.

use std::collections::HashMap;

use libsql::Builder;
use minimarket_backend::crypto;
use serde::Deserialize;

const AYUDA: &str = "Uso:
  cargo run --bin importar_negocio -- --id <n> --nombre <texto> --identificador <texto> --base <base en Turso> [--slug <texto>] [--ruc <texto>]";

fn requerido(args: &HashMap<String, String>, clave: &str) -> String {
    match args.get(clave) {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => {
            eprintln!("Falta --{}.\n\n{}", clave, AYUDA);
            std::process::exit(1);
        }
    }
}

fn var(nombre: &str) -> String {
    std::env::var(nombre).unwrap_or_else(|_| {
        eprintln!("Falta {} en el .env", nombre);
        std::process::exit(1);
    })
}

fn salir(mensaje: impl std::fmt::Display) -> ! {
    eprintln!("❌ {}", mensaje);
    std::process::exit(1);
}

/// "--clave valor" -> {clave: valor}
fn leer_argumentos() -> HashMap<String, String> {
    let lista: Vec<String> = std::env::args().skip(1).collect();
    let mut mapa = HashMap::new();
    let mut i = 0;
    while i < lista.len() {
        match lista[i].strip_prefix("--") {
            Some(clave) if i + 1 < lista.len() => {
                mapa.insert(clave.to_string(), lista[i + 1].clone());
                i += 2;
            }
            _ => salir(format!("Argumento no reconocido: {}\n\n{}", lista[i], AYUDA)),
        }
    }
    mapa
}

/// Host de una URL libsql://host (sin el esquema).
fn host_de(url: &str) -> &str {
    url.split("://").nth(1).unwrap_or(url).trim_end_matches('/')
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = leer_argumentos();

    let id: i64 = requerido(&args, "id").parse().unwrap_or_else(|_| salir("--id debe ser un número"));
    let nombre = requerido(&args, "nombre");
    let identificador = requerido(&args, "identificador");
    let base = requerido(&args, "base");
    let slug = args.get("slug").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let ruc = args.get("ruc").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    let central_url = var("CENTRAL_DATABASE_URL");
    let central_token = var("CENTRAL_AUTH_TOKEN");
    let api_token = var("TURSO_API_TOKEN");
    let org = var("TURSO_ORG");
    let clave = crypto::cargar_clave_desde_env();

    println!("Central:      {}", host_de(&central_url));
    println!("Organización: {}", org);

    // La central y la organización tienen que ser de la misma cuenta. Si el
    // .env quedó a medio cambiar (central de una cuenta, API de otra), se
    // detiene antes de escribir nada.
    if !host_de(&central_url).contains(&format!("-{}.", org)) {
        salir(format!(
            "La central ({}) no pertenece a la organización '{}'. Revisa el .env: CENTRAL_DATABASE_URL y TURSO_ORG deben ser de la misma cuenta.",
            host_de(&central_url),
            org
        ));
    }

    let central = Builder::new_remote(central_url, central_token)
        .build()
        .await
        .unwrap_or_else(|e| salir(format!("No se pudo conectar a la central: {}", e)));
    let conn = central.connect().unwrap_or_else(|e| salir(e));

    // 1. ¿Ya está registrado? Entonces no se hace nada.
    let mut filas = conn
        .query(
            "SELECT id, identificador FROM tiendas WHERE id = ?1 OR identificador = ?2",
            libsql::params![id, identificador.clone()],
        )
        .await
        .unwrap_or_else(|e| salir(format!("No se pudo consultar la central: {}", e)));
    if let Some(fila) = filas.next().await.unwrap_or_else(|e| salir(e)) {
        let id_existente: i64 = fila.get(0).unwrap_or_default();
        let ident_existente: String = fila.get(1).unwrap_or_default();
        println!(
            "⏭️  Ya existe en la central (id {}, {}). No se escribió nada.",
            id_existente, ident_existente
        );
        return;
    }

    // 2. Datos de la base en Turso: tiene que existir en esta organización.
    let cliente = reqwest::Client::new();
    let api = format!("https://api.turso.tech/v1/organizations/{}/databases/{}", org, base);

    #[derive(Deserialize)]
    struct InfoBase {
        #[serde(rename = "Hostname")]
        hostname: String,
    }
    #[derive(Deserialize)]
    struct RespuestaBase {
        database: InfoBase,
    }
    let resp = cliente
        .get(&api)
        .bearer_auth(&api_token)
        .send()
        .await
        .unwrap_or_else(|e| salir(format!("No se pudo consultar Turso: {}", e)));
    if resp.status().as_u16() == 404 {
        salir(format!("La base '{}' no existe en la organización '{}'.", base, org));
    }
    if !resp.status().is_success() {
        salir(format!("Turso respondió {} al consultar la base: {}", resp.status(), resp.text().await.unwrap_or_default()));
    }
    let info: RespuestaBase = resp.json().await.unwrap_or_else(|e| salir(format!("Respuesta rara de Turso: {}", e)));
    let db_url = format!("libsql://{}", info.database.hostname);

    // 3. Token propio para esa base (como hace el registro de negocios).
    #[derive(Deserialize)]
    struct RespuestaToken {
        jwt: String,
    }
    let resp = cliente
        .post(format!("{}/auth/tokens", api))
        .bearer_auth(&api_token)
        .send()
        .await
        .unwrap_or_else(|e| salir(format!("No se pudo pedir el token: {}", e)));
    if !resp.status().is_success() {
        salir(format!("Turso no dio el token ({}): {}", resp.status(), resp.text().await.unwrap_or_default()));
    }
    let token = resp
        .json::<RespuestaToken>()
        .await
        .unwrap_or_else(|e| salir(format!("Respuesta rara de Turso: {}", e)))
        .jwt;

    // 4. Comprobar que con ese token la base responde y tiene usuarios.
    let db_negocio = Builder::new_remote(db_url.clone(), token.clone())
        .build()
        .await
        .unwrap_or_else(|e| salir(format!("No se pudo conectar a la base del negocio: {}", e)));
    let conn_negocio = db_negocio.connect().unwrap_or_else(|e| salir(e));
    let mut r = conn_negocio
        .query("SELECT COUNT(*) FROM usuarios", ())
        .await
        .unwrap_or_else(|e| salir(format!("La base no parece de un negocio (sin tabla usuarios): {}", e)));
    let usuarios: i64 = match r.next().await.unwrap_or_else(|e| salir(e)) {
        Some(f) => f.get(0).unwrap_or(0),
        None => 0,
    };

    // 5. Registrar en la central, sin vencimiento.
    let token_cifrado = crypto::cifrar(&token, &clave).unwrap_or_else(|e| salir(e));
    conn.execute(
        "INSERT INTO tiendas (id, nombre_negocio, identificador, ruc, turso_db_url, turso_db_token, estado, fecha_vencimiento, slug)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ACTIVO', NULL, ?7)",
        libsql::params![id, nombre.clone(), identificador.clone(), ruc, db_url.clone(), token_cifrado, slug.clone()],
    )
    .await
    .unwrap_or_else(|e| salir(format!("No se pudo insertar en la central: {}", e)));

    println!(
        "✅ {} ({}) registrado con id {}. Base: {} · {} usuario(s) · slug: {}",
        nombre,
        identificador,
        id,
        host_de(&db_url),
        usuarios,
        slug.as_deref().unwrap_or("se asigna al arrancar")
    );
    println!("   Sus usuarios entran al índice central la próxima vez que arranque el backend.");
}
