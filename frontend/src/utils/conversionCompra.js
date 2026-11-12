// Comprar por caja (o blíster) y que entre por unidad: en farmacia la caja
// de 100 tabletas se compra entera, pero se vende por tableta, blíster o caja.
// El stock y el costo se llevan siempre en la unidad del producto (la
// tableta); la caja y el blíster son sus presentaciones (cuántas lleva).
import { normalizar } from './importacion';

const r6 = (n) => Math.round(Number(n) * 1e6) / 1e6;

/** Map producto_id -> presentaciones [{ nombre, factor, precio }] (de api.opcionesVenta). */
export function presentacionesPorProducto(opciones) {
  const mapa = new Map();
  for (const p of opciones?.presentaciones || []) {
    if (!(Number(p.factor) > 1)) continue;
    if (!mapa.has(p.producto_id)) mapa.set(p.producto_id, []);
    mapa.get(p.producto_id).push({ nombre: p.nombre, factor: Number(p.factor), precio: Number(p.precio) });
  }
  return mapa;
}

// Cómo escriben los proveedores la caja y el blíster.
const COMO_CAJA = ['cj', 'cja', 'cjs', 'caj', 'caja', 'cajas', 'cx', 'box', 'bx'];
const COMO_BLISTER = ['bl', 'bls', 'blt', 'blis', 'blister', 'blisters', 'blist', 'tira', 'tiras'];
// Unidad suelta: no se convierte.
const COMO_UNIDAD = ['', 'und', 'un', 'u', 'unid', 'unidad', 'unidades', 'tab', 'tabs', 'tableta', 'tabletas', 'cap', 'caps', 'capsula',
  'capsulas', 'comp', 'comprimido', 'comprimidos', 'pza', 'pieza', 'nb', 'niu'];

/**
 * La presentación del producto que corresponde a lo que dice la columna
 * Unidad del Excel ("CJ", "Caja x100", "BL"…), o null si es la unidad
 * suelta o no se sabe. Con un número ("Caja x100") se busca la de ese factor.
 */
export function presentacionDeTexto(texto, lista) {
  if (!lista?.length) return null;
  const t = normalizar(texto);
  if (COMO_UNIDAD.includes(t)) return null;
  const numero = Number((String(texto).match(/(\d+)/) || [])[1]);
  const exacta = lista.find((p) => normalizar(p.nombre) === t);
  if (exacta) return exacta;
  const letras = t.replace(/\d+/g, '').replace(/x$/, '');
  const tipo = COMO_CAJA.includes(letras) ? 'caja' : COMO_BLISTER.includes(letras) ? 'blister' : null;
  if (!tipo) return null;
  const delTipo = lista.filter((p) => normalizar(p.nombre).startsWith(tipo));
  if (numero > 1) return delTipo.find((p) => p.factor === numero) || lista.find((p) => p.factor === numero) || null;
  // Sin número: si hay una sola de ese tipo, esa; si no, la más grande (la caja).
  if (delTipo.length === 1) return delTipo[0];
  return delTipo.sort((a, b) => b.factor - a.factor)[0] || null;
}

/**
 * Un ítem comprado en una presentación -> en unidades del producto:
 * 10 cajas x100 a S/ 9.50 = 1000 unidades a S/ 0.095. Guarda cómo se compró
 * (compradoComo) para mostrarlo.
 */
export function aUnidades(item, presentacion) {
  if (!presentacion) return item;
  const como = item.compradoComo || { nombre: presentacion.nombre, factor: presentacion.factor, cantidad: item.cantidad, precio: item.precio_compra };
  return {
    ...item,
    cantidad: r6(como.cantidad * presentacion.factor),
    precio_compra: r6(como.precio / presentacion.factor),
    compradoComo: { ...como, nombre: presentacion.nombre, factor: presentacion.factor },
  };
}

/** Convierte los ítems que traen una unidad de presentación ("CJ"). Devuelve { items, convertidos }. */
export function convertirItems(items, porProducto) {
  let convertidos = 0;
  const salida = items.map((it) => {
    const pres = presentacionDeTexto(it.unidad, porProducto.get(it.producto_id));
    if (!pres) return it;
    convertidos += 1;
    return aUnidades(it, pres);
  });
  return { items: salida, convertidos };
}

/**
 * Para un producto nuevo que llega por caja: cuántas unidades trae (del
 * nombre "… X 100 TAB") y en qué unidad se vende (tableta, cápsula…).
 */
export function adivinarSuelto(nombre, unidadTexto) {
  const n = normalizar(nombre);
  const t = normalizar(unidadTexto);
  const porCaja = COMO_CAJA.includes(t.replace(/\d+/g, '')) || /\bx\s*\d+/i.test(nombre);
  const factor = Number((String(nombre).match(/x\s*(\d{2,4})\b/i) || [])[1]) || null;
  const unidad = /cap/.test(n) ? 'CAPSULA' : /tab|comp/.test(n) ? 'TABLETA' : /sobre/.test(n) ? 'SOBRE' : /ampolla|amp\b/.test(n) ? 'AMPOLLA' : 'UNIDAD';
  return { suelto: porCaja && factor > 1, factor, unidad };
}
