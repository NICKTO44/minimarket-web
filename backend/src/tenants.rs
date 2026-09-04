use libsql::Builder;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::crypto;

/// Datos de conexión de un negocio, resueltos desde la base central.
/// `db_token` aquí siempre está en texto plano (ya descifrado) — el
/// cifrado solo existe en la columna de la base central, nunca en memoria
/// más tiempo del necesario.
#[derive(Clone)]
pub struct TiendaConexion {
    pub id: i64,
    pub nombre_negocio: String,
    pub identificador: String,
    pub db_url: String,
    pub db_token: String,
}

/// Envoltorio simple para poder inyectar la conexión de la tienda de la
/// petición actual como `Extension` en los handlers.
pub struct TenantDb(pub libsql::Database);

/// Registro central de negocios: sabe encontrar a qué base pertenece cada
/// usuario/identificador, cachea el resultado en memoria para no golpear
/// la base central en cada petición autenticada, y cifra/descifra los
/// tokens de cada tienda con una clave que solo vive en el .env.
pub struct RegistroTiendas {
    central_db: libsql::Database,
    cache: RwLock<HashMap<i64, TiendaConexion>>,
    clave_cifrado: [u8; 32],
}

impl RegistroTiendas {
    pub fn nuevo(central_db: libsql::Database, clave_cifrado: [u8; 32]) -> Self {
        Self {
            central_db,
            cache: RwLock::new(HashMap::new()),
            clave_cifrado,
        }
    }

    fn fila_a_tienda(&self, row: &libsql::Row) -> Result<TiendaConexion, String> {
        let token_cifrado: String = row.get(4).unwrap_or_default();
        let db_token = crypto::descifrar(&token_cifrado, &self.clave_cifrado)?;

        Ok(TiendaConexion {
            id: row.get(0).unwrap_or_default(),
            nombre_negocio: row.get(1).unwrap_or_default(),
            identificador: row.get(2).unwrap_or_default(),
            db_url: row.get(3).unwrap_or_default(),
            db_token,
        })
    }

    /// Cifra un token de tienda, listo para guardar en la base central.
    /// Usado por el endpoint de registro al crear un negocio nuevo.
    pub fn cifrar_token(&self, token: &str) -> Result<String, String> {
        crypto::cifrar(token, &self.clave_cifrado)
    }

    /// Busca la tienda por su identificador único (ej. "bodega-juan").
    /// Se usa cuando el navegador ya recuerda a qué negocio pertenece.
    pub async fn buscar_por_identificador(&self, identificador: &str) -> Result<TiendaConexion, String> {
        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token
                 FROM tiendas WHERE identificador = ?1",
                libsql::params![identificador],
            )
            .await
            .map_err(|e| e.to_string())?;

        let row = rows
            .next()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Negocio no encontrado".to_string())?;

        let tienda = self.fila_a_tienda(&row)?;
        self.cache.write().await.insert(tienda.id, tienda.clone());
        Ok(tienda)
    }

    /// Busca a qué tienda pertenece un nombre de usuario, vía la tabla
    /// `usuarios_indice` (solo tiene una fila por negocio: el súper admin
    /// que lo registró). Se usa en el primer login de un dispositivo nuevo.
    pub async fn buscar_por_usuario(&self, usuario: &str) -> Result<TiendaConexion, String> {
        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT t.id, t.nombre_negocio, t.identificador, t.turso_db_url, t.turso_db_token
                 FROM usuarios_indice u
                 JOIN tiendas t ON t.id = u.tienda_id
                 WHERE u.usuario = ?1",
                libsql::params![usuario],
            )
            .await
            .map_err(|e| e.to_string())?;

        let row = rows
            .next()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Usuario no encontrado".to_string())?;

        let tienda = self.fila_a_tienda(&row)?;
        self.cache.write().await.insert(tienda.id, tienda.clone());
        Ok(tienda)
    }

    /// Resuelve una tienda por su id, usando la caché en memoria si ya se
    /// consultó antes. Es lo que usa el middleware en cada petición
    /// autenticada (el JWT ya trae el tienda_id).
    pub async fn resolver_por_id(&self, tienda_id: i64) -> Result<TiendaConexion, String> {
        if let Some(t) = self.cache.read().await.get(&tienda_id) {
            return Ok(t.clone());
        }

        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token
                 FROM tiendas WHERE id = ?1",
                libsql::params![tienda_id],
            )
            .await
            .map_err(|e| e.to_string())?;

        let row = rows
            .next()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Negocio no encontrado".to_string())?;

        let tienda = self.fila_a_tienda(&row)?;
        self.cache.write().await.insert(tienda.id, tienda.clone());
        Ok(tienda)
    }

    /// Lista TODOS los negocios registrados en la plataforma, con sus
    /// tokens ya descifrados. Solo pensado para herramientas de
    /// administración que necesitan recorrer cada base de tenant (por
    /// ejemplo, el comando de migraciones) — nunca se expone vía HTTP.
    pub async fn listar_todas(&self) -> Result<Vec<TiendaConexion>, String> {
        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token
                 FROM tiendas ORDER BY id",
                (),
            )
            .await
            .map_err(|e| e.to_string())?;

        let mut tiendas = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            tiendas.push(self.fila_a_tienda(&row)?);
        }

        Ok(tiendas)
    }

    /// Abre una conexión real (libsql::Database) a la base de esa tienda.
    pub async fn conectar(&self, tienda: &TiendaConexion) -> Result<libsql::Database, String> {
        Builder::new_remote(tienda.db_url.clone(), tienda.db_token.clone())
            .build()
            .await
            .map_err(|e| e.to_string())
    }

    /// Conexión directa a la base central (para el endpoint de registro,
    /// que necesita insertar la tienda nueva y su usuario en el índice).
    pub fn conexion_central(&self) -> Result<libsql::Connection, String> {
        self.central_db.connect().map_err(|e| e.to_string())
    }
}