use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Lote {
    pub id: i64,
    pub producto_id: i64,
    pub cantidad: f64,
    pub fecha_vencimiento: String,
    pub numero_lote: Option<String>,
    pub activo: bool,
    /// Ya pasó su fecha (hora de Lima): no se vende.
    pub vencido: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoLote {
    pub producto_id: i64,
    pub cantidad: f64,
    pub fecha_vencimiento: String,
    pub numero_lote: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct LoteAlerta {
    pub lote_id: i64,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub cantidad: f64,
    pub unidad_medida: String,
    pub fecha_vencimiento: String,
    pub dias_restantes: i64, // negativo = ya vencido
}

#[derive(Debug, Serialize)]
pub struct LoteAccionResponse {
    pub success: bool,
    pub message: String,

}

/// Un lote descartado, tal como quedó en mermas_lote.
#[derive(Debug, Serialize)]
pub struct MermaLote {
    pub id: i64,
    pub fecha_hora: String,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub numero_lote: Option<String>,
    pub fecha_vencimiento: String,
    pub cantidad: f64,
    pub costo_unitario: Option<f64>,
    /// cantidad × costo, None si no tenía costo.
    pub costo_total: Option<f64>,
    pub motivo: String,
    pub usuario_nombre: Option<String>,
}

/// Reporte de productos vencidos: lo descartado en el rango y lo que está
/// vencido hoy y todavía no se descartó.
#[derive(Debug, Serialize)]
pub struct ReporteMermas {
    pub desde: String,
    pub hasta: String,
    /// false = la base aún no tiene la migración 0032.
    pub disponible: bool,
    pub descartados: Vec<MermaLote>,
    pub total_costo: f64,
    pub sin_costo: i64,
    pub vencidos_sin_descartar: Vec<LoteAlerta>,
    /// Lo que los clientes devolvieron como "Vencido" en el rango (pantalla
    /// Devoluciones). No vuelve al stock; su costo ya se descuenta en
    /// Ganancias como parte de la devolución.
    pub devueltos_vencidos: Vec<DevueltoVencido>,
    pub total_costo_devueltos: f64,
}

#[derive(Debug, Serialize)]
pub struct DevueltoVencido {
    /// Hora de Lima, "AAAA-MM-DD HH:MM:SS".
    pub fecha_hora: String,
    pub folio_devolucion: String,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub unidad_medida: String,
    pub cantidad: f64,
    /// Lo que se le devolvió al cliente.
    pub monto: f64,
    /// cantidad × costo, None si no tenía costo.
    pub costo_total: Option<f64>,
}

/// Una venta donde salió parte de un lote (tabla venta_lotes, 0033).
#[derive(Debug, Serialize)]
pub struct VentaDeLote {
    pub venta_id: i64,
    pub folio: String,
    /// Hora de Lima, "AAAA-MM-DD HH:MM:SS".
    pub fecha_hora: String,
    pub estado: String,
    pub cantidad: f64,
    pub cliente_nombre: Option<String>,
    pub cliente_documento: Option<String>,
    pub cliente_telefono: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VentasDeLote {
    pub lote_id: i64,
    pub producto_nombre: String,
    pub numero_lote: Option<String>,
    pub fecha_vencimiento: String,
    /// false = la base aún no tiene la migración 0033.
    pub disponible: bool,
    pub ventas: Vec<VentaDeLote>,
    pub total_vendido: f64,
}
