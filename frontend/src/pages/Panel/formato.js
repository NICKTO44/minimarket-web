// Formatos y etiquetas que comparten las pantallas del panel.

const MESES = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'set', 'oct', 'nov', 'dic'];

/** "2026-10-11" (o "2026-10-11 09:30:00") → "11 oct 2026". */
export function fechaCorta(texto) {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(texto || '');
  if (!m) return '—';
  return `${Number(m[3])} ${MESES[Number(m[2]) - 1]} ${m[1]}`;
}

/** Texto de los días que quedan de suscripción. */
export function textoDias(dias) {
  if (dias === null || dias === undefined) return 'Sin vencimiento';
  if (dias < 0) return dias === -1 ? 'Venció ayer' : `Venció hace ${-dias} días`;
  if (dias === 0) return 'Vence hoy';
  if (dias === 1) return 'Vence mañana';
  return `Quedan ${dias} días`;
}

/** Situación de la suscripción para pintarla: { texto, tono }. */
export function situacion(negocio) {
  if (negocio.estado === 'SUSPENDIDO') return { texto: 'Suspendido', tono: 'rojo' };
  if (negocio.estado === 'RESTRINGIDO') return { texto: 'Modo lectura', tono: 'ambar' };
  const d = negocio.dias_restantes;
  if (d !== null && d !== undefined && d < 0) return { texto: 'Vencido', tono: 'rojo' };
  if (d !== null && d !== undefined && d <= 7) return { texto: 'Por vencer', tono: 'ambar' };
  return { texto: 'Activo', tono: 'verde' };
}

/** Modo de facturación para pintarlo: { texto, tono }. */
export function modoFacturacion(f) {
  if (!f) return { texto: '—', tono: 'gris' };
  if (f.modo === 'DIRECTO') {
    const beta = f.ambiente === 'BETA';
    return { texto: beta ? 'Directo · beta' : 'Directo a SUNAT', tono: beta ? 'azul' : 'verde' };
  }
  if (f.modo === 'FACTURALIBRE') return { texto: 'FacturaLibre', tono: 'violeta' };
  return { texto: 'Sin facturación', tono: 'gris' };
}

export const UNIDADES = [
  { valor: 'dias', singular: 'día', plural: 'días' },
  { valor: 'meses', singular: 'mes', plural: 'meses' },
  { valor: 'anios', singular: 'año', plural: 'años' },
];

/** "MES", 3 → "3 meses". */
export function duracion(cantidad, unidad) {
  const clave = { DIA: 'dias', MES: 'meses', ANIO: 'anios' }[unidad] || unidad;
  const u = UNIDADES.find((x) => x.valor === clave);
  if (!u) return `${cantidad} ${unidad}`;
  return `${cantidad} ${cantidad === 1 ? u.singular : u.plural}`;
}
