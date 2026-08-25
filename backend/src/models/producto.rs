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
}