use serde::{Deserialize, Serialize};

// ---------- Mesas ----------

#[derive(Debug, Serialize)]
pub struct MesaEstado {
    pub id: i64,
    pub nombre: String,
    pub zona: String,
    pub capacidad: i64,
    pub orden: i64,
    /// Pedido abierto en esta mesa (None = mesa libre).
    pub pedido: Option<PedidoResumen>,
}

#[derive(Debug, Deserialize)]
pub struct MesaPayload {
    pub nombre: String,
    pub zona: Option<String>,
    pub capacidad: Option<i64>,
    pub orden: Option<i64>,
}

// ---------- Pedidos ----------

#[derive(Debug, Serialize, Clone)]
pub struct PedidoResumen {
    pub id: i64,
    pub tipo: String,
    pub mesa_id: Option<i64>,
    pub mesa_nombre: Option<String>,
    pub cliente_nombre: Option<String>,
    pub personas: Option<i64>,
    pub usuario_id: i64,
    pub mesero: Option<String>,
    pub fecha_apertura: String,
    pub total: f64,
    pub cantidad_items: f64,
    pub pendientes: i64,
    /// Minutos desde que se abrió (lo calcula la base, con su propio reloj).
    pub minutos_abierto: i64,
    /// Líneas que barra/cocina marcó LISTAS y aún no se entregan.
    pub listos: i64,
    /// Líneas enviadas que todavía se están preparando.
    pub preparando: i64,
}

#[derive(Debug, Serialize)]
pub struct ItemPedido {
    pub id: i64,
    pub producto_id: i64,
    pub nombre_producto: String,
    pub opciones: Option<String>,
    pub nota: Option<String>,
    pub cantidad: f64,
    pub precio_unitario: f64,
    pub subtotal: f64,
    pub estado: String,
    pub fecha_creacion: String,
    pub fecha_envio: Option<String>,
    pub fecha_listo: Option<String>,
    pub fecha_entregado: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PedidoDetalle {
    #[serde(flatten)]
    pub resumen: PedidoResumen,
    pub estado: String,
    pub notas: Option<String>,
    pub items: Vec<ItemPedido>,
}

#[derive(Debug, Deserialize)]
pub struct NuevoPedido {
    pub tipo: String,
    pub mesa_id: Option<i64>,
    pub cliente_nombre: Option<String>,
    pub personas: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct NuevoItemPedido {
    pub producto_id: i64,
    pub cantidad: f64,
    #[serde(default)]
    pub opcion_ids: Vec<i64>,
    pub nota: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgregarItems {
    pub items: Vec<NuevoItemPedido>,
}

#[derive(Debug, Deserialize)]
pub struct CambiarCantidad {
    pub cantidad: f64,
    /// Si viene, reemplaza la nota de la línea ("" la borra).
    #[serde(default)]
    pub nota: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MotivoPayload {
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MoverPedido {
    pub mesa_id: i64,
}

/// Lo que se imprime en barra/cocina: solo lo que se acaba de enviar.
#[derive(Debug, Serialize)]
pub struct Comanda {
    pub pedido: PedidoResumen,
    pub items: Vec<ItemPedido>,
    pub fecha: String,
}

// ---------- Modificadores ----------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpcionModificador {
    pub id: Option<i64>,
    pub nombre: String,
    pub precio_extra: f64,
}

#[derive(Debug, Serialize)]
pub struct GrupoModificador {
    pub id: i64,
    pub nombre: String,
    pub obligatorio: bool,
    pub multiple: bool,
    pub opciones: Vec<OpcionModificador>,
    pub producto_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct GrupoPayload {
    pub nombre: String,
    pub obligatorio: bool,
    pub multiple: bool,
    pub opciones: Vec<OpcionModificador>,
    #[serde(default)]
    pub producto_ids: Vec<i64>,
}

// ---------- Preparación (barra / cocina) ----------

#[derive(Debug, Serialize)]
pub struct ItemPreparacion {
    pub id: i64,
    pub nombre_producto: String,
    pub opciones: Option<String>,
    pub nota: Option<String>,
    pub cantidad: f64,
    pub listo: bool,
    /// Minutos desde que se mandó a preparar.
    pub minutos_espera: i64,
    /// Minutos desde que se marcó listo (0 si aún no).
    pub minutos_listo: i64,
}

/// Un pedido con lo que está en preparación o listo sin entregar.
#[derive(Debug, Serialize)]
pub struct TicketPreparacion {
    pub pedido_id: i64,
    pub tipo: String,
    pub mesa_nombre: Option<String>,
    pub cliente_nombre: Option<String>,
    pub usuario_id: i64,
    pub mesero: Option<String>,
    /// true si ya se cobró (típico en "para llevar": se paga antes).
    pub cobrado: bool,
    pub items: Vec<ItemPreparacion>,
}

#[derive(Debug, Deserialize)]
pub struct MarcarListo {
    pub item_ids: Vec<i64>,
    /// false = deshacer (se marcó listo por error).
    #[serde(default = "verdadero")]
    pub listo: bool,
}

fn verdadero() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct MarcarEntregado {
    pub item_ids: Vec<i64>,
}

// ---------- Tipo de negocio ----------

#[derive(Debug, Deserialize)]
pub struct ModoNegocioPayload {
    pub modo_negocio: String,
}

#[derive(Debug, Serialize)]
pub struct RolResumen {
    pub id: i64,
    pub nombre: String,
}

#[derive(Debug, Serialize)]
pub struct Respuesta {
    pub success: bool,
    pub message: String,
}
