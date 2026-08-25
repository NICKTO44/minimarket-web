use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct Proveedor {
    pub id: i64,
    pub nombre: String,
    pub contacto: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub direccion: Option<String>,
    pub tipo_documento: String,
    pub numero_documento: Option<String>,
    pub credito_disponible: f64,
    pub activo: bool,
}

#[derive(Debug, Deserialize)]
pub struct NuevoProveedor {
    pub nombre: String,
    pub contacto: Option<String>,
    pub telefono: Option<String>,
    pub email: Option<String>,
    pub direccion: Option<String>,
    pub tipo_documento: Option<String>,
    pub numero_documento: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemCompra {
    pub producto_id: i64,
    pub cantidad: f64,
    pub precio_compra: f64,
    pub precio_venta_sugerido: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct NuevaCompraRequest {
    pub proveedor_id: i64,
    pub fecha_compra: String,
    pub items: Vec<ItemCompra>,
    pub descuento: Option<f64>,
    pub credito_aplicado: Option<f64>,
    pub tipo_pago: String, // EFECTIVO | TRANSFERENCIA | CREDITO | MIXTO
    pub fecha_vencimiento_pago: Option<String>,
    pub factura_numero: Option<String>,
    pub notas: Option<String>,
    pub usuario_id: i64,
}

#[derive(Debug, Serialize)]
pub struct CompraResponse {
    pub success: bool,
    pub message: String,
    pub compra_id: Option<i64>,
    pub folio: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ItemRecepcion {
    pub detalle_id: i64,
    pub cantidad_recibida: f64,
    pub cantidad_conforme: f64,
    // Si el producto lleva vencimiento, se necesita esta fecha para crear el lote
    pub fecha_vencimiento: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecibirMercaderiaRequest {
    pub compra_id: i64,
    pub items: Vec<ItemRecepcion>,
    pub notas_recepcion: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct CompraResumen {
    pub id: i64,
    pub folio: String,
    pub proveedor_nombre: String,
    pub fecha_compra: String,
    pub total: f64,
    pub estado: String,
    pub estado_pago: String,
    pub unidades_danadas: f64,
    pub unidades_ya_devueltas: f64,
    pub unidades_faltantes: f64,
}

#[derive(Debug, Serialize)]
pub struct DetalleCompraItem {
    pub id: i64,
    pub producto_id: i64,
    pub producto_nombre: String,
    pub lleva_vencimiento: bool,
    pub cantidad: f64,
    pub cantidad_recibida: f64,
    pub cantidad_conforme: f64,
    pub precio_compra: f64,
}

#[derive(Debug, Serialize)]
pub struct CompraDetalle {
    pub id: i64,
    pub folio: String,
    pub proveedor_nombre: String,
    pub estado: String,
    pub items: Vec<DetalleCompraItem>,
}
#[derive(Debug, Deserialize)]
pub struct ItemDevolucionProveedor {
    pub detalle_compra_id: i64,
    pub producto_id: i64,
    pub cantidad_devuelta: f64,
    pub precio_compra: f64,
    pub motivo_item: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegistrarDevolucionProveedorRequest {
    pub compra_id: i64,
    pub motivo: String, // DAÑADO | DEFECTUOSO | PRODUCTO_INCORRECTO | VENCIDO | OTRO
    pub detalle_motivo: Option<String>,
    pub items: Vec<ItemDevolucionProveedor>,
    pub notas: Option<String>,
    pub usuario_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ResolverDevolucionProveedorRequest {
    pub estado: String, // ACEPTADA | RECHAZADA
    pub tipo_resolucion: Option<String>, // CREDITO | REEMBOLSO | CAMBIO
    pub notas: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DevolucionProveedorResumen {
    pub id: i64,
    pub compra_id: i64,
    pub proveedor_nombre: String,
    pub folio: String,
    pub fecha: String,
    pub motivo: String,
    pub monto_devolucion: f64,
    pub estado: String,
    pub tipo_resolucion: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DevolucionProveedorResponse {
    pub success: bool,
    pub message: String,
    pub devolucion_id: Option<i64>,
    pub folio: Option<String>,
    pub credito_disponible: Option<f64>,
}