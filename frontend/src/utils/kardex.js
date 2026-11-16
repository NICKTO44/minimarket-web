// Kárdex (módulo Ajustes y Kárdex): cómo se arma la tabla.

export const TIPOS_KARDEX = {
  VENTA: 'Venta',
  ENTRADA: 'Entrada',
  DEVOLUCION: 'Devolución',
  SALIDA: 'Salida',
  AJUSTE: 'Ajuste',
  MERMA: 'Merma',
};

const r3 = (n) => Math.round(n * 1000) / 1000;
const SIN_REGISTRO = 'Cambios no anotados (ventas o importaciones de antes del kárdex)';

/** Filas del kárdex (de la más nueva a la más antigua) con los huecos sin registro y el saldo inicial. */
export function filasDelKardex(kardex, { hastaHoy = true } = {}) {
  if (!kardex || kardex.movimientos.length === 0) return [];
  const movs = kardex.movimientos;
  const hueco = (clave, desde, hasta) => {
    const d = r3(hasta - desde);
    return d === 0
      ? []
      : [{ clave, hueco: true, etiqueta: 'Sin registro', entrada: d > 0 ? d : 0, salida: d < 0 ? -d : 0, saldo: hasta, motivo: SIN_REGISTRO }];
  };
  const filas = hastaHoy ? hueco('hoy', movs[0].saldo, kardex.stock_actual) : [];
  movs.forEach((m, i) => {
    filas.push({ ...m, clave: m.id, fecha: m.fecha_hora, etiqueta: TIPOS_KARDEX[m.tipo] || m.tipo });
    const anterior = movs[i + 1];
    if (anterior) filas.push(...hueco(`h${m.id}`, anterior.saldo, m.stock_anterior));
  });
  const primero = movs[movs.length - 1];
  if (r3(primero.stock_anterior) !== 0) {
    filas.push({ clave: 'inicial', hueco: true, etiqueta: 'Saldo inicial', saldo: primero.stock_anterior, motivo: 'Lo que había antes del primer movimiento anotado' });
  }
  return filas;
}

