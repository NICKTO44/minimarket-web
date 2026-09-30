use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VentaResumen {
    pub id: i64,
    pub folio: String,
    pub fecha_hora: String,
    pub total: f64,
    pub metodo_pago: String,
    // Solo con metodo_pago = "MIXTO"
    pub pago_efectivo: Option<f64>,
    pub pago_otro: Option<f64>,
    pub pago_otro_metodo: Option<String>,
    pub cajero: String,
    pub estado: String,
}

#[derive(Debug, Serialize)]
pub struct ProductoVendido {
    pub producto_nombre: String,
    pub cantidad_vendida: f64,
    pub total_vendido: f64,
}

#[derive(Debug, Serialize)]
pub struct EstadisticasCompletas {
    pub ventas_cantidad: i64,
    pub ventas_total: f64,
    pub ticket_promedio: f64,
    pub devoluciones_cantidad: i64,
    pub devoluciones_total: f64,
    pub total_neto: f64,
}