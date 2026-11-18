// Módulo "Cargos rápidos" (vaso, bolsa, servicio de madrugada).
// - Los primeros `gratis_por_venta` de cada venta van gratis: la línea lleva
//   ese descuento (el stock baja por todos, así el inventario cuadra).
// - Con horario (`auto_desde`–`auto_hasta`, hora de Perú), el cargo se agrega
//   solo una vez por venta mientras haya algo más en el carrito.
import { redondear2 } from './medidas';

/** "HH:MM" -> minutos del día. */
const minutos = (hhmm) => {
  const [h, m] = String(hhmm || '').split(':').map(Number);
  return h * 60 + m;
};

/** ¿La hora "HH:MM" cae en el horario? (cruza la medianoche si desde > hasta). Fin no incluido. */
export function enHorario(cargo, hhmm) {
  if (!cargo?.auto_desde || !cargo?.auto_hasta) return false;
  const ahora = minutos(hhmm);
  const desde = minutos(cargo.auto_desde);
  const hasta = minutos(cargo.auto_hasta);
  return desde < hasta ? ahora >= desde && ahora < hasta : ahora >= desde || ahora < hasta;
}

/** Marca de descuento de los gratis (la reconoce aplicarDescuentos como descuento propio). */
const descGratis = (monto, n) => ({ tipo: 'S/', valor: monto, gratis: n });

/**
 * Pone los gratis en cada línea suelta de un cargo con gratis por venta.
 * Las de presentación (un paquete de vasos) se cobran completas.
 */
export function conGratis(lineas, cargos) {
  if (!cargos?.length) return lineas;
  const gratisDe = new Map(cargos.filter((c) => c.gratis_por_venta > 0).map((c) => [c.producto_id, c.gratis_por_venta]));
  if (gratisDe.size === 0) return lineas;
  let cambio = false;
  const nuevo = lineas.map((l) => {
    const gratis = gratisDe.get(l.id);
    if (!gratis || l.presentacion || l.paquete) return l;
    const n = Math.min(l.cantidad, gratis);
    const monto = redondear2(n * l.precio);
    if (l.descPropio?.gratis === n && l.descPropio?.valor === monto) return l;
    cambio = true;
    return { ...l, descPropio: n > 0 ? descGratis(monto, n) : null, descuentoMonto: monto + (l.descTotalParte || 0) };
  });
  return cambio ? nuevo : lineas;
}

/**
 * Agrega los cargos con horario que tocan ahora, una vez por venta: solo si
 * el carrito ya tiene algo más y el vendedor no lo quitó en esta venta.
 */
export function conAutomaticos(lineas, cargos, productos, hhmm, quitados) {
  if (!cargos?.length || lineas.length === 0) return lineas;
  const automaticos = new Set(cargos.filter((c) => c.auto_desde && c.auto_hasta).map((c) => c.producto_id));
  if (!lineas.some((l) => !automaticos.has(l.id))) return lineas;
  const faltan = cargos.filter(
    (c) => automaticos.has(c.producto_id) && enHorario(c, hhmm) && !quitados.has(c.producto_id) && !lineas.some((l) => l.id === c.producto_id)
  );
  if (faltan.length === 0) return lineas;
  const extra = faltan
    .map((c) => productos.find((p) => p.id === c.producto_id))
    .filter(Boolean)
    .map((p) => ({ ...p, cantidad: 1, descuentoMonto: 0, cargoAutomatico: true }));
  return extra.length ? [...lineas, ...extra] : lineas;
}

/** Texto corto del cargo para el botón y la configuración. */
export const textoGratis = (n) => (n > 0 ? `${n} gratis por venta` : '');
