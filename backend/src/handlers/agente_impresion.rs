use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::models::impresora::ImpresionResponse;
use crate::AppState;

#[derive(Deserialize)]
pub struct TokenQuery {
    token: String,
}

pub async fn agente_websocket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    let token_esperado = std::env::var("AGENTE_IMPRESION_TOKEN").unwrap_or_default();
    if token_esperado.is_empty() || query.token != token_esperado {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| manejar_agente(socket, state))
}

#[derive(Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum MensajeDeAgente {
    Resultado { id: u64, success: bool, message: String },
}

async fn manejar_agente(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    state.estado_impresion.registrar_agente(tx).await;
    println!("🖨️  Agente de impresión conectado");

    let mut tarea_envio = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let estado = state.clone();
    let mut tarea_lectura = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(texto) = msg {
                if let Ok(MensajeDeAgente::Resultado { id, success, message }) =
                    serde_json::from_str(&texto)
                {
                    estado
                        .estado_impresion
                        .resolver_respuesta(id, ImpresionResponse { success, message })
                        .await;
                }
            }
        }
    });

    tokio::select! {
        _ = &mut tarea_envio => {},
        _ = &mut tarea_lectura => {},
    }

    state.estado_impresion.desconectar_agente().await;
    println!("🖨️  Agente de impresión desconectado");
}