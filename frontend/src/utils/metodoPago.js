// Nombres y descripción de los métodos de pago, compartidos por POS,
// ticket (Recibo), Reportes y Devoluciones para que todos muestren lo
// mismo. Un pago MIXTO = efectivo + un medio digital (ver migración
// 0004_pago_mixto_y_caja_por_cajero.sql).
//
// Yape y Plin van por separado (migración 0019): cada uno tiene su botón
// y su línea en caja y reportes. 'YAPE_PLIN' queda solo para mostrar las
// ventas de antes, cuando eran un solo medio.

/** Botones del punto de venta, en el orden en que se muestran. */
export const METODOS_DE_COBRO = ['EFECTIVO', 'YAPE', 'PLIN', 'TARJETA', 'TRANSFERENCIA'];

export const METODOS_OTRO_MIXTO = ['YAPE', 'PLIN', 'TARJETA', 'TRANSFERENCIA'];

/** Con qué se puede pagar un adelanto o un abono de un crédito. */
export const METODOS_DE_ABONO = ['EFECTIVO', 'YAPE', 'PLIN', 'TRANSFERENCIA', 'TARJETA'];

const NOMBRES = {
  EFECTIVO: 'Efectivo',
  TARJETA: 'Tarjeta',
  TRANSFERENCIA: 'Transferencia',
  YAPE: 'Yape',
  PLIN: 'Plin',
  YAPE_PLIN: 'Yape/Plin',
  MIXTO: 'Mixto',
  CREDITO: 'Crédito',
};

// Una venta al crédito se guarda como MIXTO con pago_otro_metodo =
// 'CREDITO' (ver migración 0015_creditos.sql).
export const METODO_CREDITO = 'CREDITO';

/** true si la venta (campos del backend) fue al crédito. */
export function esVentaAlCredito(venta) {
  return venta?.metodo_pago === 'MIXTO' && venta?.pago_otro_metodo === METODO_CREDITO;
}

export function nombreMetodo(metodo) {
  return NOMBRES[metodo] || (metodo || '').replace('_', '/');
}

/**
 * Texto corto del pago de una venta. Para MIXTO incluye el reparto:
 * "Mixto: Efectivo S/ 30.00 + Yape/Plin S/ 20.00". Acepta los campos
 * del backend (pago_efectivo, pago_otro, pago_otro_metodo).
 */
export function describirPago(venta) {
  if (!venta) return '';
  if (esVentaAlCredito(venta)) return 'Al crédito';
  if (venta.metodo_pago !== 'MIXTO') {
    return (venta.metodo_pago || '').replace('_', '/');
  }
  if (venta.pago_efectivo == null || venta.pago_otro == null) {
    return 'MIXTO';
  }
  return `Mixto: Efectivo S/ ${Number(venta.pago_efectivo).toFixed(2)} + ${nombreMetodo(venta.pago_otro_metodo)} S/ ${Number(venta.pago_otro).toFixed(2)}`;
}

const centimos = (n) => Math.round((Number(n) || 0) * 100) / 100;

/**
 * Lo vendido en una caja por medio de pago, con Yape y Plin por separado.
 * Acepta la caja tal como la manda el servidor. `ventas_transferencia`
 * junta transferencia, Yape y Plin; lo de cada billetera llega aparte.
 *  - Caja abierta con el cambio ya instalado: Transferencia, Yape y Plin.
 *  - Caja que ya estaba abierta al llegar el cambio: Yape y Plin cuentan
 *    desde ese momento; lo anterior sigue junto en una línea aparte,
 *    porque no se puede saber qué parte fue de cada uno.
 *  - Servidor que aún no manda el detalle: una sola línea, como antes.
 * Devuelve [{ clave, label, valor }].
 */
export function ventasDeCajaPorMetodo(caja) {
  if (!caja) return [];
  const yape = centimos(caja.ventas_yape);
  const plin = centimos(caja.ventas_plin);
  const sinSeparar = centimos(caja.ventas_yape_plin);
  const transferencias = centimos(caja.ventas_transferencia);
  const filas = [
    { clave: 'EFECTIVO', label: 'Efectivo', valor: centimos(caja.ventas_efectivo) },
    { clave: 'TARJETA', label: 'Tarjeta', valor: centimos(caja.ventas_tarjeta) },
  ];
  // Servidor que todavía no manda el detalle: una sola línea, como antes.
  if (caja.ventas_yape === undefined || caja.ventas_yape === null) {
    filas.push({ clave: 'TRANSFERENCIA', label: 'Transferencia / Yape / Plin', valor: transferencias });
    return filas;
  }
  if (!caja.detalle_billeteras) {
    const resto = centimos(transferencias - yape - plin);
    // Caja que ya estaba abierta cuando se separaron: lo nuevo va por
    // billetera (desde cero) y lo anterior queda junto en su propia línea.
    filas.push({ clave: 'YAPE', label: 'Yape', valor: yape });
    filas.push({ clave: 'PLIN', label: 'Plin', valor: plin });
    filas.push({ clave: 'TRANSFERENCIA', label: 'Transferencia y Yape/Plin de antes', valor: resto });
    return filas;
  }
  filas.push({ clave: 'YAPE', label: 'Yape', valor: yape });
  filas.push({ clave: 'PLIN', label: 'Plin', valor: plin });
  filas.push({ clave: 'TRANSFERENCIA', label: 'Transferencia', valor: centimos(transferencias - yape - plin - sinSeparar) });
  if (sinSeparar !== 0) filas.push({ clave: 'YAPE_PLIN', label: 'Yape/Plin (sin separar)', valor: sinSeparar });
  return filas;
}
