// Cálculos del Ingreso rápido de productos (pages/Inventario/IngresoRapido.jsx).

// "1,50" o "1.50" -> 1.5; vacío o texto -> NaN.
export const num = (v) => {
  const n = parseFloat(String(v ?? '').trim().replace(',', '.'));
  return Number.isFinite(n) ? n : NaN;
};
export const soles = (n) => `S/ ${(Number.isFinite(n) ? n : 0).toFixed(2)}`;
export const redondear = (n, d = 6) => Math.round(n * 10 ** d) / 10 ** d;

/** Cantidad en unidades y costo por unidad de una fila (NaN si falta algo). */
export function cantidadYCosto(f) {
  if (f.porCaja) {
    const unidades = num(f.unidadesCaja);
    const cajas = num(f.cajas);
    const precioCaja = num(f.precioCaja);
    return { cantidad: unidades * cajas, costo: unidades > 0 ? redondear(precioCaja / unidades) : NaN };
  }
  return { cantidad: num(f.cantidad), costo: num(f.precioCompra) };
}

/** Lo que falta en una fila para guardarla, o null si está lista. */
export function faltaEnFila(f, conPresentaciones) {
  const { cantidad, costo } = cantidadYCosto(f);
  if (!f.producto) {
    if (!f.busqueda.trim()) return 'Escribe el nombre del producto o búscalo.';
    if (!f.categoria) return 'Elige la categoría.';
    if (!(num(f.precioVenta) > 0)) return 'Falta el precio de venta.';
  } else if (f.producto.lleva_vencimiento && !f.vencimiento) {
    return 'Es perecible: pon la fecha de vencimiento del lote.';
  }
  if (f.porCaja) {
    if (!(num(f.unidadesCaja) > 0)) return 'Pon cuántas unidades trae la caja.';
    if (!(num(f.cajas) > 0)) return 'Pon cuántas cajas entran.';
    if (!(num(f.precioCaja) >= 0)) return 'Pon el precio de compra de la caja.';
    if (f.venderCaja && conPresentaciones && !f.producto) {
      if (num(f.unidadesCaja) < 2 || !Number.isInteger(num(f.unidadesCaja))) return 'Para vender la caja, debe traer 2 unidades o más (entero).';
      if (!(num(f.precioVentaCaja) > 0)) return 'Pon el precio de venta de la caja.';
    }
  } else {
    if (!(cantidad > 0)) return 'Pon cuánto entra.';
    if (!(costo >= 0)) return 'Pon el precio de compra.';
  }
  return null;
}

/** El cuerpo de POST /compras/rapida. */
export function armarIngreso(filas, tipoPago, conPresentaciones) {
  return {
    tipo_pago: tipoPago,
    items: filas.map((f) => {
      const { cantidad, costo } = cantidadYCosto(f);
      const item = { cantidad: redondear(cantidad, 3), precio_compra: costo };
      if (f.producto) {
        item.producto_id = f.producto.id;
        if (f.producto.lleva_vencimiento) item.fecha_vencimiento = f.vencimiento;
      } else {
        item.nuevo = {
          nombre: f.busqueda.trim(),
          unidad_medida: f.unidad,
          categoria_id: Number(f.categoria),
          precio: num(f.precioVenta),
          ...(f.codigo.trim() ? { codigo: f.codigo.trim() } : {}),
        };
        if (f.porCaja && f.venderCaja && conPresentaciones) {
          item.vender_caja = { unidades: num(f.unidadesCaja), precio: num(f.precioVentaCaja) };
        }
      }
      return item;
    }),
  };
}
