use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::logica::validacion_documento::{validar_dni, validar_ruc};

#[derive(Deserialize)]
pub struct ConsultaDocumento {
    pub tipo: String,
    pub numero: String,
}

#[derive(Serialize)]
pub struct ConsultaDocumentoResponse {
    /// None = no se pudo validar (sin token, timeout, error de red, tipo
    /// de documento sin soporte de validación como CE/PASAPORTE) --
    /// el frontend debe tratar esto como "seguir sin autocompletar",
    /// nunca como un error que bloquee el formulario.
    pub existe: Option<bool>,
    pub nombre: Option<String>,
}

/// Siempre responde 200 -- nunca falla "hacia arriba" ni con un
/// StatusCode de error, porque este endpoint es una ayuda opcional al
/// formulario, no un paso obligatorio. Cualquier problema (Factiliza
/// caído, sin token configurado, tipo de documento no soportado) se
/// traduce en `existe: None`, y el frontend simplemente no autocompleta.
pub async fn consultar_documento(Query(params): Query<ConsultaDocumento>) -> Json<ConsultaDocumentoResponse> {
    let resultado = match params.tipo.as_str() {
        "DNI" if params.numero.len() == 8 => validar_dni(&params.numero).await,
        "RUC" if params.numero.len() == 11 => validar_ruc(&params.numero).await,
        _ => None,
    };

    match resultado {
        Some(v) => Json(ConsultaDocumentoResponse { existe: Some(v.existe), nombre: v.nombre_completo }),
        None => Json(ConsultaDocumentoResponse { existe: None, nombre: None }),
    }
}