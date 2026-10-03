// IGV: mismas reglas que el backend (src/logica/igv.rs).
// - Los precios son siempre precio final al público.
// - Cada producto es gravado, exonerado o inafecto (lo decide el producto
//   o, si no dice nada, su categoría).
// - La tasa es una por negocio (Configuración → Datos del negocio).
export const TASA_GENERAL = 18;
export const TASA_MYPE_RESTAURANTES = 10.5;

export const AFECTACIONES = [
  { valor: 'GRAVADO', label: 'Gravado', detalle: 'Paga IGV (lo normal)' },
  { valor: 'EXONERADO', label: 'Exonerado', detalle: 'No paga IGV: frutas, verduras, arroz, pescado, libros...' },
  { valor: 'INAFECTO', label: 'Inafecto', detalle: 'Fuera del IGV (casos poco comunes)' },
];

export function etiquetaAfectacion(valor) {
  return AFECTACIONES.find((a) => a.valor === valor)?.label || 'Gravado';
}

/** Tasa utilizable: mayor a 0 y hasta 30; cualquier otra cosa es la general. */
export function tasaValida(tasa) {
  const n = Number(tasa);
  return Number.isFinite(n) && n > 0 && n <= 30 ? n : TASA_GENERAL;
}

/** "18" o "10.5": la tasa sin ceros de más. */
export function etiquetaTasa(tasa) {
  return String(Number(tasaValida(tasa).toFixed(2)));
}

/**
 * Desglose para imprimir un comprobante. Usa lo que mandó el servidor
 * (tasa y totales por tipo de operación); si es un comprobante antiguo sin
 * esos datos, lo de siempre: todo gravado al 18 %.
 */
export function desgloseDeComprobante(datos, total) {
  const igv = datos?.igv ?? total - total / 1.18;
  if (datos?.op_gravadas == null) {
    return { tasa: TASA_GENERAL, gravadas: total - igv, exoneradas: 0, inafectas: 0, igv };
  }
  return {
    tasa: tasaValida(datos.igv_tasa),
    gravadas: datos.op_gravadas,
    exoneradas: datos.op_exoneradas || 0,
    inafectas: datos.op_inafectas || 0,
    igv,
  };
}
