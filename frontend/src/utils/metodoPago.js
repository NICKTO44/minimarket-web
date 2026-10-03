// Nombres y descripción de los métodos de pago, compartidos por POS,
// ticket (Recibo), Reportes y Devoluciones para que todos muestren lo
// mismo. Un pago MIXTO = efectivo + un medio digital (ver migración
// 0004_pago_mixto_y_caja_por_cajero.sql).

export const METODOS_OTRO_MIXTO = ['YAPE_PLIN', 'TARJETA', 'TRANSFERENCIA'];

const NOMBRES = {
  EFECTIVO: 'Efectivo',
  TARJETA: 'Tarjeta',
  TRANSFERENCIA: 'Transferencia',
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
