use axum::{extract::{State, Extension}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;
use crate::models::auth::Claims;
use crate::licencias_logica::{calcular_nueva_fecha, hoy, parsear_fecha};
use crate::tenants::NivelAcceso;

#[derive(Serialize)]
pub struct EstadoSuscripcionResponse {
    pub estado: String,
    pub fecha_vencimiento: Option<String>,
    /// Negativo si ya venció hace esos días. `None` si nunca tiene
    /// vencimiento (acceso indefinido).
    pub dias_restantes: Option<i64>,
    pub modo_lectura: bool,
}

/// Consulta de solo lectura — siempre disponible, incluso en modo
/// lectura, para que el negocio pueda ver cuánto le queda sin tener que
/// esperar a que algo le falle primero.
pub async fn estado_suscripcion(
    Extension(claims): Extension<Claims>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<EstadoSuscripcionResponse>, (StatusCode, String)> {
    let tienda = state
        .tiendas
        .resolver_por_id(claims.tienda_id)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "No se pudo consultar tu suscripción.".to_string()))?;

    let dias_restantes = tienda
        .fecha_vencimiento
        .as_deref()
        .and_then(parsear_fecha)
        .map(|fecha| (fecha - hoy()).num_days());

    let modo_lectura = matches!(tienda.nivel_acceso(), NivelAcceso::SoloLectura(_));

    Ok(Json(EstadoSuscripcionResponse {
        estado: tienda.estado.clone(),
        fecha_vencimiento: tienda.fecha_vencimiento.clone(),
        dias_restantes,
        modo_lectura,
    }))
}

#[derive(Deserialize)]
pub struct CanjearCodigoRequest {
    pub codigo: String,
}

#[derive(Serialize)]
pub struct CanjearCodigoResponse {
    pub ok: bool,
    pub fecha_vencimiento: String,
    pub mensaje: String,
}

pub async fn canjear_codigo(
    Extension(claims): Extension<Claims>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CanjearCodigoRequest>,
) -> Result<Json<CanjearCodigoResponse>, (StatusCode, String)> {
    let codigo = payload.codigo.trim().to_uppercase();
    if codigo.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Ingresa un código.".to_string()));
    }

    let conn = state
        .tiendas
        .conexion_central()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error de conexión.".to_string()))?;

    // Canje atómico: esta consulta hace la validación Y el marcado como
    // "usado" en un solo paso. Si dos peticiones llegaran con el mismo
    // código al mismo tiempo, solo una puede afectar una fila — eso es
    // justo lo que garantiza que nunca se use dos veces.
    let filas = conn
        .execute(
            "UPDATE codigos_activacion
             SET usado = 1, usado_por_tienda_id = ?1, fecha_uso = datetime('now', 'localtime')
             WHERE codigo = ?2 AND usado = 0
               AND (fecha_expira_si_no_se_usa IS NULL OR fecha_expira_si_no_se_usa >= date('now'))",
            libsql::params![claims.tienda_id, codigo.clone()],
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error validando el código.".to_string()))?;

    if filas == 0 {
        return Err((StatusCode::BAD_REQUEST, "Código inválido, ya usado, o vencido.".to_string()));
    }

    // Traer la duración de ESTE código para calcular la nueva fecha de
    // vencimiento del negocio.
    let mut rows = conn
        .query(
            "SELECT duracion_cantidad, duracion_unidad FROM codigos_activacion WHERE codigo = ?1",
            libsql::params![codigo.clone()],
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error leyendo el código.".to_string()))?;

    let (cantidad, unidad): (i64, String) = match rows.next().await.ok().flatten() {
        Some(row) => (row.get(0).unwrap_or(1), row.get(1).unwrap_or_else(|_| "MES".to_string())),
        None => (1, "MES".to_string()),
    };

    // Fecha de vencimiento actual del negocio, para sumar desde ahí
    // (no "robarle" los días que ya tenía pagados).
    let mut rows_tienda = conn
        .query("SELECT fecha_vencimiento FROM tiendas WHERE id = ?1", libsql::params![claims.tienda_id])
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Error leyendo tu negocio.".to_string()))?;

    let actual: Option<String> = match rows_tienda.next().await.ok().flatten() {
        Some(row) => row.get(0).unwrap_or(None),
        None => None,
    };

    let nueva_fecha = calcular_nueva_fecha(actual.as_deref(), cantidad, &unidad.to_lowercase())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    conn.execute(
        "UPDATE tiendas SET estado = 'ACTIVO', fecha_vencimiento = ?1 WHERE id = ?2",
        libsql::params![nueva_fecha.clone(), claims.tienda_id],
    )
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "El código se canjeó, pero no se pudo activar tu negocio. Contacta a soporte.".to_string(),
        )
    })?;

    // Para que el desbloqueo se vea DE INMEDIATO (no hasta que expire
    // la caché de 60s), se descarta el dato viejo en memoria ahora.
    state.tiendas.invalidar_metadata(claims.tienda_id).await;

    Ok(Json(CanjearCodigoResponse {
        ok: true,
        fecha_vencimiento: nueva_fecha,
        mensaje: "¡Código canjeado! Tu cuenta ya está activa.".to_string(),
    }))
}