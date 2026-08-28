use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use std::time::Duration;

use crate::models::impresora::{DatosImpresion, ImpresionResponse};
use crate::AppState;

pub async fn imprimir_boleta(
    State(state): State<Arc<AppState>>,
    Json(datos): Json<DatosImpresion>,
) -> Json<ImpresionResponse> {
    let receptor = match state.estado_impresion.solicitar_impresion(&datos).await {
        Ok(rx) => rx,
        Err(mensaje) => return Json(ImpresionResponse { success: false, message: mensaje }),
    };

    match tokio::time::timeout(Duration::from_secs(10), receptor).await {
        Ok(Ok(respuesta)) => Json(respuesta),
        Ok(Err(_)) => Json(ImpresionResponse {
            success: false,
            message: "El agente de impresión se desconectó antes de responder".into(),
        }),
        Err(_) => Json(ImpresionResponse {
            success: false,
            message: "La impresora no respondió a tiempo (revisa que esté encendida y conectada a la red)".into(),
        }),
    }
}

// Nota: se elimina el StatusCode import si no se usa en el futuro; lo dejo
// por si luego quieres devolver códigos HTTP distintos a 200 con error en el body.
#[allow(dead_code)]
fn _evitar_warning_status_code() -> StatusCode { StatusCode::OK }