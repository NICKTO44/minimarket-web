// Venta por medidas (módulo MEDIDAS): cantidades con decimales y cálculo de
// pies tablares. Los montos se redondean al céntimo por línea, igual que el
// servidor (handlers/ventas.rs), para que el total cobrado y el guardado
// coincidan siempre.

export const UNIDAD_PIE_TABLAR = 'PIE_TABLAR';

// Unidades que se venden por fracción (0.75 kg, 1/2 litro, 2.5 m): al
// agregarlas al carrito se pregunta cuánto se lleva.
const UNIDADES_FRACCIONADAS = new Set(['KG', 'GRAMO', 'LIBRA', 'ONZA', 'LITRO', 'ML', 'METRO']);
export const seVendeFraccionado = (unidad) => UNIDADES_FRACCIONADAS.has(unidad);
export const PIES_POR_METRO = 3.28084;

export const redondear2 = (n) => Math.round(n * 100) / 100;
const redondear3 = (n) => Math.round(n * 1000) / 1000;

/** Importe de una línea: precio × cantidad, al céntimo. */
export function subtotalLinea(precio, cantidad) {
  return redondear2(precio * cantidad);
}

/** "33.33", "24", "2.5": hasta 3 decimales, sin ceros de relleno. */
export function formatoCantidad(n) {
  return String(redondear3(Number(n) || 0));
}

/**
 * Lee una medida como la escribe un maderero: "2", "2.5", "2,5", "3/4" o
 * "1 1/2". Devuelve el número, o null si no se entiende o no es mayor a 0.
 */
export function leerMedida(texto) {
  const limpio = String(texto ?? '').trim().replace(',', '.').replace(/["']/g, '');
  if (!limpio) return null;
  const mixto = limpio.match(/^(\d+(?:\.\d+)?)\s+(\d+)\s*\/\s*(\d+)$/);
  const fraccion = limpio.match(/^(\d+)\s*\/\s*(\d+)$/);
  let valor;
  if (mixto) {
    if (Number(mixto[3]) === 0) return null;
    valor = Number(mixto[1]) + Number(mixto[2]) / Number(mixto[3]);
  } else if (fraccion) {
    if (Number(fraccion[2]) === 0) return null;
    valor = Number(fraccion[1]) / Number(fraccion[2]);
  } else if (/^\d*\.?\d+$/.test(limpio) || /^\d+\.$/.test(limpio)) {
    valor = Number(limpio);
  } else {
    return null;
  }
  return Number.isFinite(valor) && valor > 0 ? valor : null;
}

/** Cantidad escrita en el carrito ("37.5", "0,25"): mayor a 0, hasta 3 decimales. */
export function leerCantidad(texto) {
  const valor = leerMedida(texto);
  if (valor === null) return null;
  const redondeada = redondear3(valor);
  return redondeada > 0 ? redondeada : null;
}

/**
 * Pies tablares de una o varias piezas iguales:
 * espesor (pulgadas) × ancho (pulgadas) × largo (pies) ÷ 12 × piezas.
 * El largo puede venir en metros. Devuelve null si falta alguna medida.
 */
export function piesTablares({ espesor, ancho, largo, largoEn = 'PIES', piezas = 1 }) {
  const e = leerMedida(espesor);
  const a = leerMedida(ancho);
  const l = leerMedida(largo);
  const p = leerMedida(piezas);
  if (e === null || a === null || l === null || p === null || !Number.isInteger(p)) return null;
  const largoPies = largoEn === 'METROS' ? l * PIES_POR_METRO : l;
  const pies = redondear2(((e * a * largoPies) / 12) * p);
  return pies > 0 ? pies : null;
}

/** Texto que acompaña la línea en el carrito y en el comprobante. */
export function detalleMedidas({ espesor, ancho, largo, largoEn = 'PIES', piezas = 1 }) {
  const limpiar = (t) => String(t).trim().replace(',', '.').replace(/\s+/g, ' ');
  const p = leerMedida(piezas);
  return `${p} ${p === 1 ? 'pza' : 'pzas'} de ${limpiar(espesor)}" x ${limpiar(ancho)}" x ${limpiar(largo)} ${
    largoEn === 'METROS' ? 'm' : 'pies'
  }`;
}
