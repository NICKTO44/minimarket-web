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

/**
 * Fecha y hora de una venta en hora de Perú. La base guarda la hora en UTC
 * ("2026-10-04 03:11:14"); aquí se muestra como "3/10/2026, 10:11:14 p. m.".
 */
export function fechaHoraLima(texto) {
  const t = String(texto || '').trim();
  if (!t) return '';
  const conZona = /[zZ]$|[+-]\d\d:?\d\d$/.test(t);
  const fecha = new Date(conZona ? t : `${t.replace(' ', 'T')}Z`);
  return Number.isNaN(fecha.getTime()) ? t : fecha.toLocaleString('es-PE', { timeZone: 'America/Lima' });
}

/**
 * La fecha de hoy en Perú ("2026-10-04"), más o menos N días. Perú es UTC-5
 * todo el año. No se usa la fecha del navegador en UTC: a las 8 p. m. en
 * Perú ya sería "mañana".
 */
export function hoyLima(dias = 0) {
  return new Date(Date.now() - 5 * 3600 * 1000 + dias * 86400 * 1000).toISOString().slice(0, 10);
}
