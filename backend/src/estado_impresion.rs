use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use axum::extract::ws::Message;
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};

use crate::models::impresora::{DatosImpresion, ImpresionResponse};

/// Estado compartido para comunicarse con el agente de impresión conectado
/// por WebSocket (el celular con Termux dentro de la red de la tienda).
pub struct EstadoImpresion {
    conexion_agente: Mutex<Option<UnboundedSender<Message>>>,
    pendientes: Mutex<HashMap<u64, oneshot::Sender<ImpresionResponse>>>,
    siguiente_id: AtomicU64,
}

impl EstadoImpresion {
    pub fn nuevo() -> Arc<Self> {
        Arc::new(Self {
            conexion_agente: Mutex::new(None),
            pendientes: Mutex::new(HashMap::new()),
            siguiente_id: AtomicU64::new(1),
        })
    }

    pub async fn registrar_agente(&self, tx: UnboundedSender<Message>) {
        *self.conexion_agente.lock().await = Some(tx);
    }

    pub async fn desconectar_agente(&self) {
        *self.conexion_agente.lock().await = None;
    }

    /// Arma la solicitud de impresión, se la manda al agente conectado y
    /// devuelve un receptor que se resuelve cuando llega la respuesta
    /// (o el llamador decide dejar de esperar con un timeout).
    pub async fn solicitar_impresion(
        &self,
        datos: &DatosImpresion,
    ) -> Result<oneshot::Receiver<ImpresionResponse>, String> {
        let id = self.siguiente_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        let sobre = serde_json::json!({
            "tipo": "imprimir",
            "id": id,
            "datos": datos,
        });

        self.pendientes.lock().await.insert(id, tx);

        let conexion = self.conexion_agente.lock().await;
        match conexion.as_ref() {
            Some(sender) => {
                if sender.send(Message::Text(sobre.to_string())).is_err() {
                    self.pendientes.lock().await.remove(&id);
                    return Err("El agente de impresión se desconectó justo ahora, intenta de nuevo".into());
                }
            }
            None => {
                self.pendientes.lock().await.remove(&id);
                return Err("No hay ningún agente de impresión conectado en este momento".into());
            }
        }

        Ok(rx)
    }

    pub async fn resolver_respuesta(&self, id: u64, respuesta: ImpresionResponse) {
        if let Some(tx) = self.pendientes.lock().await.remove(&id) {
            let _ = tx.send(respuesta);
        }
    }
}