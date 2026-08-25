use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AbrirCajaRequest {
    pub usuario_id: i64,
    pub numero_caja: Option<i64>,
    pub monto_inicial: f64,
    pub observaciones: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CerrarCajaRequest {
    pub caja_id: i64,
    pub usuario_id: i64,
    pub usuario_rol_id: i64, // 1 = Admin
    pub monto_contado: f64,
    pub observaciones: Option<String>,
    pub justificacion_diferencia: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MovimientoCajaRequest {
    pub caja_id: i64,
    pub tipo: String, // RETIRO | INGRESO | GASTO
    pub monto: f64,
    pub motivo: String,
    pub usuario_id: i64,
}

#[derive(Debug, Serialize)]
pub struct CajaResponse {
    pub success: bool,
    pub message: String,
    pub caja_id: Option<i64>,
}
#[derive(Debug, Serialize)]
pub struct CajaEstado {
    pub id: i64,
    pub usuario_nombre: String,
    pub monto_inicial: f64,
    pub ventas_efectivo: f64,
    pub ventas_tarjeta: f64,
    pub ventas_transferencia: f64,
    pub total_ventas: f64,
    pub numero_transacciones: i64,
    pub devoluciones_monto: f64,
    pub retiros_total: f64,
    pub ingresos_total: f64,
    pub gastos_total: f64,
    pub fecha_apertura: String,
}
#[derive(Debug, Serialize)]
pub struct CajaHistorial {
    pub id: i64,
    pub usuario_nombre: String,
    pub fecha_apertura: String,
    pub fecha_cierre: Option<String>,
    pub estado: String,
    pub monto_inicial: f64,
    pub monto_contado: Option<f64>,
    pub diferencia: Option<f64>,
    pub total_ventas: f64,
    pub ventas_efectivo: f64,
    pub ventas_tarjeta: f64,
    pub ventas_transferencia: f64,
    pub numero_transacciones: i64,
    pub devoluciones_monto: f64,
}