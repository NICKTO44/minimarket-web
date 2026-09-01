use axum::{extract::{State, Query}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppState;

/// Esquema completo de una instalación nueva, incrustado en el binario en
/// tiempo de compilación (por eso el Dockerfile necesita copiar schema.sql
/// antes de correr `cargo build`).
const SCHEMA_SQL: &str = include_str!("../../schema.sql");

#[derive(Deserialize)]
pub struct RegistroRequest {
    pub nombre_negocio: String,
    /// Nombre de la persona que se está registrando (el súper admin).
    pub nombre_completo: String,
    pub usuario: String,
    pub password: String,
    pub ruc: Option<String>,
}

#[derive(Serialize)]
pub struct RegistroResponse {
    pub ok: bool,
    pub identificador: String,
}

#[derive(Deserialize)]
pub struct VerificarUsuarioQuery {
    pub usuario: String,
}

#[derive(Serialize)]
pub struct VerificarUsuarioResponse {
    pub disponible: bool,
}

fn quitar_acentos(texto: &str) -> String {
    texto
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        })
        .collect()
}

fn slugify(texto: &str) -> String {
    let base: String = texto
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let colapsado = base
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if colapsado.is_empty() { "negocio".to_string() } else { colapsado }
}

fn generar_sufijo() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}", nanos % 0xFFFF)
}

/// Crea la base de datos nueva en Turso (vía Platform API) y devuelve su URL
/// de conexión (libsql://...).
async fn crear_base_turso(nombre_db: &str) -> Result<String, String> {
    let api_token = std::env::var("TURSO_API_TOKEN").map_err(|_| "Falta TURSO_API_TOKEN en .env".to_string())?;
    let org = std::env::var("TURSO_ORG").map_err(|_| "Falta TURSO_ORG en .env".to_string())?;

    let cliente = reqwest::Client::new();
    let resp = cliente
        .post(format!("https://api.turso.tech/v1/organizations/{}/databases", org))
        .bearer_auth(&api_token)
        .json(&serde_json::json!({ "name": nombre_db, "group": "default" }))
        .send()
        .await
        .map_err(|e| format!("Error creando base en Turso: {}", e))?;

    if !resp.status().is_success() {
        let texto = resp.text().await.unwrap_or_default();
        return Err(format!("Turso rechazó la creación de la base: {}", texto));
    }

    #[derive(Deserialize)]
    struct DatabaseInfo {
        #[serde(rename = "Hostname")]
        hostname: String,
    }
    #[derive(Deserialize)]
    struct CrearDbResponse {
        database: DatabaseInfo,
    }

    let datos: CrearDbResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(format!("libsql://{}", datos.database.hostname))
}

/// Crea un token de acceso (full-access, sin expiración) para esa base.
async fn crear_token_turso(nombre_db: &str) -> Result<String, String> {
    let api_token = std::env::var("TURSO_API_TOKEN").map_err(|_| "Falta TURSO_API_TOKEN en .env".to_string())?;
    let org = std::env::var("TURSO_ORG").map_err(|_| "Falta TURSO_ORG en .env".to_string())?;

    let cliente = reqwest::Client::new();
    let resp = cliente
        .post(format!(
            "https://api.turso.tech/v1/organizations/{}/databases/{}/auth/tokens",
            org, nombre_db
        ))
        .bearer_auth(&api_token)
        .send()
        .await
        .map_err(|e| format!("Error creando token en Turso: {}", e))?;

    if !resp.status().is_success() {
        let texto = resp.text().await.unwrap_or_default();
        return Err(format!("Turso rechazó la creación del token: {}", texto));
    }

    #[derive(Deserialize)]
    struct CrearTokenResponse {
        jwt: String,
    }

    let datos: CrearTokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(datos.jwt)
}

/// Chequeo en vivo para el formulario de registro: ¿este usuario ya existe
/// en algún negocio de la plataforma?
pub async fn verificar_usuario(
    State(state): State<Arc<AppState>>,
    Query(params): Query<VerificarUsuarioQuery>,
) -> Json<VerificarUsuarioResponse> {
    let disponible = state.tiendas.buscar_por_usuario(&params.usuario).await.is_err();
    Json(VerificarUsuarioResponse { disponible })
}

pub async fn registrar_negocio(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegistroRequest>,
) -> Result<Json<RegistroResponse>, (StatusCode, String)> {
    if payload.nombre_negocio.trim().is_empty()
        || payload.nombre_completo.trim().is_empty()
        || payload.usuario.trim().is_empty()
        || payload.password.len() < 6
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Completa todos los campos (la contraseña debe tener al menos 6 caracteres)".into(),
        ));
    }

    // 1. El usuario tiene que estar libre en toda la plataforma.
    if state.tiendas.buscar_por_usuario(&payload.usuario).await.is_ok() {
        return Err((StatusCode::BAD_REQUEST, "Ese nombre de usuario ya está en uso".into()));
    }

    // 2. Identificador único del negocio (slug + sufijo corto).
    let base_slug = slugify(&quitar_acentos(&payload.nombre_negocio));
    let identificador = format!("{}-{}", base_slug, generar_sufijo());

    // 3. Crear la base nueva en Turso + su token de acceso.
    let db_url = crear_base_turso(&identificador)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let db_token = crear_token_turso(&identificador)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // 4. Conectarse a la base nueva y correr el schema completo.
    let db_nueva = libsql::Builder::new_remote(db_url.clone(), db_token.clone())
        .build()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let conn_nueva = db_nueva
        .connect()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn_nueva
        .execute_batch(SCHEMA_SQL)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error creando las tablas: {}", e)))?;

    // 5. Nombre del negocio + RUC (ya existe una fila por defecto, del schema).
    conn_nueva
        .execute(
            "UPDATE configuracion_tienda SET nombre_tienda = ?1, ruc = ?2 WHERE id = 1",
            libsql::params![payload.nombre_negocio.clone(), payload.ruc.clone()],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 6. Crear al súper admin dentro de la base nueva (rol_id 1 = ADMIN).
    let hash = bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    conn_nueva
        .execute(
            "INSERT INTO usuarios (username, password_hash, nombre_completo, rol_id) VALUES (?1, ?2, ?3, 1)",
            libsql::params![payload.usuario.clone(), hash, payload.nombre_completo.clone()],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 7. Guardar el negocio + el índice de usuario en la base central.
    //    El token se cifra antes de guardarlo — nunca queda en texto
    //    plano en la base central.
    let token_cifrado = state
        .tiendas
        .cifrar_token(&db_token)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let conn_central = state
        .tiendas
        .conexion_central()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    conn_central
        .execute(
            "INSERT INTO tiendas (nombre_negocio, identificador, ruc, turso_db_url, turso_db_token)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            libsql::params![
                payload.nombre_negocio.clone(),
                identificador.clone(),
                payload.ruc.clone(),
                db_url,
                token_cifrado
            ],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tienda_id = conn_central.last_insert_rowid();

    conn_central
        .execute(
            "INSERT INTO usuarios_indice (usuario, tienda_id) VALUES (?1, ?2)",
            libsql::params![payload.usuario.clone(), tienda_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RegistroResponse { ok: true, identificador }))
}