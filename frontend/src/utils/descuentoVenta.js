// Módulo "Descuento en la venta": descuento en % o en soles, a un producto
// del carrito o al total de la venta.
//
// Cada línea guarda:
//   descPropio      { tipo: '%' | 'S/', valor }  su propio descuento (o nada)
//   descTotalParte  lo que le tocó del descuento al total
//   descuentoMonto  propio + parte (lo que ya usan el total, el ticket y el servidor)
// Las líneas de un paquete no se tocan: su descuento es el del paquete.
import { redondear2, subtotalLinea } from './medidas';

export const TIPOS_DESCUENTO = ['%', 'S/'];

/** Monto en soles de un descuento { tipo, valor } sobre una base. */
export function montoDescuento(desc, base) {
  if (!desc || !(desc.valor > 0) || !(base > 0)) return 0;
  const monto = desc.tipo === '%' ? (base * Math.min(desc.valor, 100)) / 100 : desc.valor;
  return Math.min(redondear2(monto), redondear2(base));
}

/** Texto corto del descuento para mostrar en el carrito ("10%" o "S/ 5.00"). */
export const textoDescuento = (desc) => (desc ? (desc.tipo === '%' ? `${desc.valor}%` : `S/ ${Number(desc.valor).toFixed(2)}`) : '');

/**
 * Recalcula el descuento de cada línea (su propio % se ajusta si cambia la
 * cantidad) y reparte el descuento al total entre las líneas, en proporción a
 * lo que vale cada una; los céntimos que sobran van a la última. Devuelve el
 * mismo arreglo si nada cambió (para no volver a dibujar).
 */
export function aplicarDescuentos(carrito, descTotal) {
  const lineas = carrito.map((i) => {
    const sub = redondear2(subtotalLinea(i.precio, i.cantidad));
    if (i.paquete) return { i, sub, entra: false };
    // Sin descuento propio: lo que ya tenía (p. ej. el de la venta en tabla) menos su parte del total.
    const propio = i.descPropio
      ? montoDescuento(i.descPropio, sub)
      : Math.min(Math.max(0, redondear2((i.descuentoMonto || 0) - (i.descTotalParte || 0))), sub);
    return { i, sub, propio, entra: true };
  });
  const entran = lineas.filter((l) => l.entra && l.sub - l.propio > 0);
  const base = redondear2(entran.reduce((s, l) => s + l.sub - l.propio, 0));
  const monto = montoDescuento(descTotal, base);
  let repartido = 0;
  const partes = new Map();
  entran.forEach((l, n) => {
    const resto = l.sub - l.propio;
    const parte = n === entran.length - 1 ? redondear2(monto - repartido) : redondear2((monto * resto) / base);
    const ajustada = Math.min(Math.max(0, parte), redondear2(resto));
    repartido = redondear2(repartido + ajustada);
    partes.set(l.i, ajustada);
  });

  let cambio = false;
  const nuevo = lineas.map((l) => {
    if (!l.entra) return l.i;
    const parte = partes.get(l.i) || 0;
    const descuentoMonto = redondear2(l.propio + parte);
    if (descuentoMonto === (l.i.descuentoMonto || 0) && parte === (l.i.descTotalParte || 0)) return l.i;
    cambio = true;
    return { ...l.i, descuentoMonto, descTotalParte: parte };
  });
  return cambio ? nuevo : carrito;
}
