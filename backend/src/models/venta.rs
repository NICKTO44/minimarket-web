use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductoVenta {
    pub id: i64,
    pub nombre: String,
    pub precio: f64,
    pub cantidad: f64,
    #[serde(rename = "descuentoMonto")]
    pub descuento_monto: Option<f64>,
    /// Opciones y nota de un pedido de mesa ("Grande, Leche de almendras").
    /// Se agrega al nombre guardado en la venta y en el comprobante.
    #[serde(default)]
    pub detalle: Option<String>,
    /// Vendido en una presentación del producto ("Rollo x 100 m", migración
    /// 0030). La línea ya llega en la unidad del producto (100 m a precio /
    /// factor); el servidor la revisa y le pone el nombre de la presentación.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentacion_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct NuevaVenta {
    pub productos: Vec<ProductoVenta>,
    pub total: f64,
    pub metodo_pago: String,
    pub monto_recibido: Option<f64>,
    pub cambio: Option<f64>,
    pub usuario_id: i64,
    pub cliente_id: Option<i64>,
    // Solo para metodo_pago = "MIXTO" (efectivo + un medio digital).
    // Opcionales para que el frontend anterior siga funcionando igual.
    #[serde(default)]
    pub pago_efectivo: Option<f64>,
    #[serde(default)]
    pub pago_otro: Option<f64>,
    #[serde(default)]
    pub pago_otro_metodo: Option<String>,
    /// Con qué se pagó la parte de `pago_efectivo` (migración 0028): None o
    /// "EFECTIVO" = efectivo, como siempre; si no, "YAPE", "PLIN", "TARJETA"
    /// o "TRANSFERENCIA" (p. ej. mitad Yape y mitad Plin).
    #[serde(default)]
    pub pago_efectivo_metodo: Option<String>,
    /// Pedido de mesa que se está cobrando (módulo Cafetería/Restaurante).
    /// None en una venta normal del POS.
    #[serde(default)]
    pub pedido_id: Option<i64>,
    /// Venta al crédito (módulo CREDITO): el pago queda pendiente y se
    /// cobra con abonos. Con esto se ignoran metodo_pago y el pago mixto.
    #[serde(default)]
    pub credito: Option<crate::handlers::creditos::CreditoVenta>,
    /// Cotización de la que salió esta venta: al cobrarla queda VENDIDA.
    #[serde(default)]
    pub cotizacion_id: Option<i64>,
    /// Cambio de prenda (módulo CAMBIOS): lo que el cliente devuelve de una
    /// venta anterior. Los productos de esta venta son lo que se lleva.
    #[serde(default)]
    pub cambio_prenda: Option<crate::handlers::cambios::CambioVenta>,
    /// Receta de los medicamentos que la piden (módulo RECETAS).
    #[serde(default)]
    pub receta: Option<crate::handlers::recetas::RecetaVenta>,
}

#[derive(Debug, Serialize)]
pub struct VentaResult {
    pub venta_id: i64,
    pub folio: String,
    /// Solo cuando la venta fue un cambio de prenda.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cambio_prenda: Option<crate::handlers::cambios::CambioHecho>,
}