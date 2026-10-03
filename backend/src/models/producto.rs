use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct Producto {
    pub id: i64,
    pub codigo: String,
    pub nombre: String,
    pub descripcion: Option<String>,
    pub precio: f64,
    pub stock: f64,
    pub stock_minimo: f64,
    pub unidad_medida: String,
    pub categoria_id: i64,
    pub categoria_nombre: Option<String>,
    pub descuento_porcentaje: f64,
    pub lleva_vencimiento: bool,
    pub imagen_url: Option<String>,
    pub activo: bool,
    pub precio_compra: f64,
    /// false = preparado al momento (café, jugo): se vende sin stock.
    pub controla_stock: bool,
    /// true = plato de la "Carta de hoy" (solo existe ese día).
    pub carta_dia: bool,
    /// true = se acabó: se ve en gris y no se puede pedir.
    pub agotado: bool,
    /// IGV que le toca al venderlo: 'GRAVADO', 'EXONERADO' o 'INAFECTO'
    /// (el propio si lo tiene; si no, el de su categoría).
    pub afectacion_igv: String,
    /// Valor propio del producto; None = hereda el de su categoría.
    pub afectacion_propia: Option<String>,
    pub afectacion_categoria: String,
}

#[derive(Debug, Deserialize)]
pub struct NuevoProducto {
    pub codigo: String,
    pub nombre: String,
    pub descripcion: Option<String>,
    pub precio: f64,
    pub stock: f64,
    pub stock_minimo: f64,
    pub unidad_medida: String,
    pub categoria_id: i64,
    pub descuento_porcentaje: Option<f64>,
    pub lleva_vencimiento: Option<bool>,
    pub imagen_url: Option<String>,
    pub precio_compra: Option<f64>,
    /// Solo lo manda el frontend en modo Cafetería/Restaurante.
    #[serde(default)]
    pub controla_stock: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ActualizarProducto {
    pub codigo: String,
    pub nombre: String,
    pub descripcion: Option<String>,
    pub precio: f64,
    pub stock: f64,
    pub stock_minimo: f64,
    pub unidad_medida: String,
    pub categoria_id: i64,
    pub descuento_porcentaje: Option<f64>,
    pub lleva_vencimiento: Option<bool>,
    pub imagen_url: Option<String>,
    pub precio_compra: Option<f64>,
    #[serde(default)]
    pub controla_stock: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ProductoResponse {
    pub success: bool,
    pub message: String,
    pub producto_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Categoria {
    pub id: i64,
    pub nombre: String,
    /// 'GRAVADO', 'EXONERADO' o 'INAFECTO': lo heredan sus productos.
    pub afectacion_igv: String,
}
#[derive(Debug, Deserialize)]
pub struct NuevaCategoria {
    pub nombre: String,
    pub descripcion: Option<String>,
}