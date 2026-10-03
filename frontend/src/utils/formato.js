// Formatos de texto compartidos por cotizaciones, créditos y guías.

/** "2026-10-18" o "2026-10-18 14:05:09" -> "18/10/2026" */
export function fechaCorta(texto) {
  const [a, m, d] = String(texto || '').slice(0, 10).split('-');
  return a && m && d ? `${d}/${m}/${a}` : '';
}

/** Número de cotización con ceros: 12 -> "000012". */
export function numeroCotizacion(numero) {
  return String(numero ?? '').padStart(6, '0');
}
