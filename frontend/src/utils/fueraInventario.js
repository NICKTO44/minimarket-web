// Módulo "Vender algo que no está en inventario": al reimprimir, la venta
// (GET /ventas/:folio) trae lo del inventario y, aparte, `fuera_inventario`
// con lo de afuera y el pago tal como lo hizo el cliente. El comprobante
// lleva las dos cosas y el total completo. Una venta de siempre no trae el
// campo y todo queda igual.

/** Lo de afuera de una venta leída del servidor (null si no tiene). */
export const fueraDeVenta = (detalle) => detalle?.fuera_inventario || null;

/**
 * La venta como la ve el cliente: total completo y, en un pago mixto, las
 * dos partes como las pagó. Sin nada de afuera devuelve la misma venta.
 */
export function ventaDelCliente(detalle) {
  const fuera = fueraDeVenta(detalle);
  if (!detalle || !fuera) return detalle;
  const mixto = detalle.metodo_pago === 'MIXTO' && fuera.pago_efectivo != null && fuera.pago_otro != null;
  return {
    ...detalle,
    total: Math.round((Number(detalle.total) + Number(fuera.total)) * 100) / 100,
    ...(mixto ? { pago_efectivo: fuera.pago_efectivo, pago_otro: fuera.pago_otro } : {}),
  };
}

/** Las líneas de afuera, con la forma de `detalle.productos` (van después). */
export const lineasFuera = (detalle) =>
  (fueraDeVenta(detalle)?.lineas || []).map((l) => ({
    nombre: l.nombre,
    cantidad: l.cantidad,
    precio_unitario: l.precio_unitario,
    subtotal: l.total,
    codigo: l.codigo || '',
    unidad_medida: l.unidad_medida,
    fueraInventario: true,
  }));

/** ¿Esta línea del carrito del punto de venta es de afuera? */
export const esFueraInventario = (item) => !!item?.externo;

/** Lo de afuera del carrito, como lo espera el servidor. */
export const fueraParaEnviar = (carrito) =>
  carrito
    .filter(esFueraInventario)
    .map((i) => ({ nombre: i.nombre, cantidad: i.cantidad, precio: i.precio, unidad_medida: i.unidad_medida, codigo: i.codigo || null }));
