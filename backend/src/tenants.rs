use chrono::Utc;
use libsql::Builder;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::crypto;

/// Cuánto tiempo se confía en la caché de metadatos de una tienda antes
/// de volver a consultar la base central. Corto a propósito: así, si
/// suspendes o restringes un negocio con una consulta SQL directa, el
/// cambio se aplica solo (a más tardar en este tiempo) sin necesidad de
/// reiniciar el backend.
const TTL_CACHE_TIENDA: Duration = Duration::from_secs(60);

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
    /// 'ACTIVO', 'RESTRINGIDO' o 'SUSPENDIDO'. Ver `NivelAcceso`.
    pub estado: String,
    /// Fecha límite de la suscripción (formato "YYYY-MM-DD"), o `None`
    /// si el negocio no tiene fecha de vencimiento (acceso indefinido).
    pub fecha_vencimiento: Option<String>,
}

/// Qué tanto puede hacer un negocio ahora mismo en el sistema.
pub enum NivelAcceso {
    /// Todo permitido, sin restricciones.
    Completo,
    /// Puede iniciar sesión y ver información (peticiones GET), pero no
    /// puede procesar ventas ni modificar nada. Trae el motivo para
    /// mostrárselo al usuario (ej. "tu suscripción venció").
    SoloLectura(String),
    /// Bloqueo total, ni iniciar sesión puede. Reservado para casos
    /// extremos, no para el flujo normal de "no pagó todavía".
    Bloqueado(String),
}

impl TiendaConexion {
    /// Determina el nivel de acceso actual del negocio, según su estado
    /// y fecha de vencimiento. Se llama tanto en el login como en cada
    /// petición autenticada.
    pub fn nivel_acceso(&self) -> NivelAcceso {
        if self.estado == "SUSPENDIDO" {
            return NivelAcceso::Bloqueado(
                "Esta cuenta fue suspendida. Contacta al soporte para más información.".to_string(),
            );
        }

        if self.estado == "RESTRINGIDO" {
            return NivelAcceso::SoloLectura(
                "Tu cuenta está en modo lectura: puedes ver tu información, pero no procesar ventas ni hacer cambios hasta regularizar tu suscripción.".to_string(),
            );
        }

        if let Some(fecha_limite) = &self.fecha_vencimiento {
            let hoy = Utc::now().format("%Y-%m-%d").to_string();
            // Comparación de strings en formato YYYY-MM-DD es válida
            // lexicográficamente (mismo orden que cronológico).
            if fecha_limite.as_str() < hoy.as_str() {
                return NivelAcceso::SoloLectura(
                    "Tu suscripción venció. Puedes seguir viendo tu información, pero no procesar ventas hasta renovar tu plan.".to_string(),
                );
            }
        }

        NivelAcceso::Completo
    }
}

/// Envoltorio simple para poder inyectar la conexión de la tienda de la
/// petición actual como `Extension` en los handlers. Envuelve un
/// `Arc<Database>` (no un `Database` propio) porque ese mismo handle se
/// comparte entre todas las peticiones de una tienda — ver
/// `RegistroTiendas::conectar_cacheado`. Los handlers no necesitan
/// cambiar nada: `tenant.0.connect()` sigue funcionando igual, Rust
/// atraviesa el Arc automáticamente.
pub struct TenantDb(pub Arc<libsql::Database>);

/// Registro central de negocios: sabe encontrar a qué base pertenece cada
/// usuario/identificador, cachea el resultado en memoria por un tiempo
/// corto (no para siempre) para no golpear la base central en cada
/// petición autenticada, cachea también las conexiones ya armadas a cada
/// base de tenant, y cifra/descifra los tokens de cada tienda con una
/// clave que solo vive en el .env.
pub struct RegistroTiendas {
    central_db: libsql::Database,
    cache: RwLock<HashMap<i64, (TiendaConexion, Instant)>>,
    conexiones: RwLock<HashMap<i64, Arc<libsql::Database>>>,
    clave_cifrado: [u8; 32],
}

impl RegistroTiendas {
    pub fn nuevo(central_db: libsql::Database, clave_cifrado: [u8; 32]) -> Self {
        Self {
            central_db,
            cache: RwLock::new(HashMap::new()),
            conexiones: RwLock::new(HashMap::new()),
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
            estado: row.get(5).unwrap_or_else(|_| "ACTIVO".to_string()),
            fecha_vencimiento: row.get(6).unwrap_or(None),
        })
    }

    async fn guardar_en_cache(&self, tienda: TiendaConexion) -> TiendaConexion {
        self.cache.write().await.insert(tienda.id, (tienda.clone(), Instant::now()));
        tienda
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
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token, estado, fecha_vencimiento
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
        Ok(self.guardar_en_cache(tienda).await)
    }

    /// Busca a qué tienda pertenece un nombre de usuario, vía la tabla
    /// `usuarios_indice` (solo tiene una fila por negocio: el súper admin
    /// que lo registró). Se usa en el primer login de un dispositivo nuevo.
    pub async fn buscar_por_usuario(&self, usuario: &str) -> Result<TiendaConexion, String> {
        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT t.id, t.nombre_negocio, t.identificador, t.turso_db_url, t.turso_db_token, t.estado, t.fecha_vencimiento
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
        Ok(self.guardar_en_cache(tienda).await)
    }

    /// Resuelve una tienda por su id, usando la caché en memoria si ya se
    /// consultó hace menos de `TTL_CACHE_TIENDA`. Es lo que usa el
    /// middleware en cada petición autenticada (el JWT ya trae el
    /// tienda_id). Pasado ese tiempo, vuelve a consultar la base central
    /// — así un cambio de estado (suspender, restringir, reactivar) se
    /// aplica solo, sin reiniciar el backend.
    pub async fn resolver_por_id(&self, tienda_id: i64) -> Result<TiendaConexion, String> {
        if let Some((tienda, insertado)) = self.cache.read().await.get(&tienda_id) {
            if insertado.elapsed() < TTL_CACHE_TIENDA {
                return Ok(tienda.clone());
            }
        }

        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token, estado, fecha_vencimiento
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
        Ok(self.guardar_en_cache(tienda).await)
    }

    /// Lista TODOS los negocios registrados en la plataforma, con sus
    /// tokens ya descifrados. Solo pensado para herramientas de
    /// administración que necesitan recorrer cada base de tenant (por
    /// ejemplo, el comando de migraciones) — nunca se expone vía HTTP.
    pub async fn listar_todas(&self) -> Result<Vec<TiendaConexion>, String> {
        let conn = self.central_db.connect().map_err(|e| e.to_string())?;
        let mut rows = conn
            .query(
                "SELECT id, nombre_negocio, identificador, turso_db_url, turso_db_token, estado, fecha_vencimiento
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

    /// Abre una conexión NUEVA (sin caché) a la base de esa tienda. Se
    /// mantiene disponible para casos puntuales que de verdad necesitan
    /// una conexión fresca, pero el camino caliente de cada petición
    /// autenticada debe usar `conectar_cacheado` en su lugar.
    pub async fn conectar(&self, tienda: &TiendaConexion) -> Result<libsql::Database, String> {
        Builder::new_remote(tienda.db_url.clone(), tienda.db_token.clone())
            .build()
            .await
            .map_err(|e| e.to_string())
    }

    /// Igual que `conectar`, pero reutiliza el `Database` ya armado para
    /// esa tienda si existe en caché, en vez de reconstruirlo en cada
    /// petición. Es lo que debe usar el middleware de autenticación.
    pub async fn conectar_cacheado(&self, tienda: &TiendaConexion) -> Result<Arc<libsql::Database>, String> {
        if let Some(db) = self.conexiones.read().await.get(&tienda.id) {
            return Ok(db.clone());
        }

        let db = self.conectar(tienda).await?;
        let db = Arc::new(db);

        self.conexiones.write().await.insert(tienda.id, db.clone());
        Ok(db)
    }

    /// Descarta la conexión cacheada de una tienda (por ejemplo, si se
    /// rotó su token de Turso y la conexión vieja ya no sirve). La
    /// siguiente petición la reconstruye desde cero con los datos
    /// actuales de `tiendas`.
    pub async fn invalidar_conexion(&self, tienda_id: i64) {
        self.conexiones.write().await.remove(&tienda_id);
    }

    /// Descarta la caché de METADATOS (estado, fecha_vencimiento, etc.)
    /// de una tienda. Se usa justo después de canjear un código de
    /// activación, para que la siguiente petición vea el nuevo estado
    /// de inmediato — sin esperar a que expire el TTL de 60 segundos.
    pub async fn invalidar_metadata(&self, tienda_id: i64) {
        self.cache.write().await.remove(&tienda_id);
    }

    /// Conexión directa a la base central (para el endpoint de registro,
    /// que necesita insertar la tienda nueva y su usuario en el índice).
    pub fn conexion_central(&self) -> Result<libsql::Connection, String> {
        self.central_db.connect().map_err(|e| e.to_string())
    }
}   