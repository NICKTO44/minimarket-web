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

/**
 * Medios de la otra parte de un pago mixto (migración 0028). Antes era
 * siempre efectivo; ahora puede ser, p. ej., mitad Yape y mitad Plin.
 */
export const METODOS_SEGUNDO_MIXTO = ['EFECTIVO', 'YAPE', 'PLIN', 'TARJETA', 'TRANSFERENCIA'];

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
  // Pedido por encargo: lo que el cliente ya adelantó (handlers/encargos.rs).
  ADELANTO: 'Adelanto',
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
 * "Mixto: Efectivo S/ 30.00 + Yape S/ 20.00" o "Mixto: Plin S/ 30.00 +
 * Yape S/ 20.00". Acepta los campos del backend (pago_efectivo,
 * pago_otro, pago_otro_metodo y pago_efectivo_metodo; sin este último, la
 * primera parte es efectivo).
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
  const segundo = nombreMetodo(venta.pago_efectivo_metodo || 'EFECTIVO');
  return `Mixto: ${segundo} S/ ${Number(venta.pago_efectivo).toFixed(2)} + ${nombreMetodo(venta.pago_otro_metodo)} S/ ${Number(venta.pago_otro).toFixed(2)}`;
}

/**
 * Cómo pagó el cliente, para imprimirlo en el ticket, la boleta/factura A4
 * y la nota de venta. Acepta la venta del POS (metodoPago, pagoSegundo,
 * pagoOtro...) o la del servidor (metodo_pago, pago_efectivo, pago_otro...).
 * Devuelve { titulo, partes: [{ nombre, monto }] } o null:
 *  - Un solo medio: { titulo: 'Yape', partes: [] }.
 *  - Mixto: { titulo: 'Pago mixto', partes: [Yape S/ 50, Plin S/ 85] }.
 *  - Al crédito recién vendido (venta.credito): null, ya tiene su bloque.
 */
export function pagoParaImprimir(venta) {
  if (!venta) return null;
  const metodo = venta.metodoPago ?? venta.metodo_pago;
  if (!metodo) return null;
  const otro = Number(venta.pagoOtro ?? venta.pago_otro);
  const otroMetodo = venta.pagoOtroMetodo ?? venta.pago_otro_metodo;
  const segundo = Number(venta.pagoSegundo ?? venta.pago_efectivo);
  const segundoMetodo = venta.pagoSegundoMetodo ?? venta.pago_efectivo_metodo ?? 'EFECTIVO';
  if (venta.credito) return null;
  if (metodo === METODO_CREDITO || otroMetodo === METODO_CREDITO) {
    const adelanto = segundo > 0 ? [{ nombre: `Adelanto (${nombreMetodo(segundoMetodo)})`, monto: segundo }] : [];
    return { titulo: 'Al crédito', partes: adelanto };
  }
  if (metodo !== 'MIXTO') return { titulo: nombreMetodo(metodo), partes: [] };
  const partes = otroMetodo
    ? [
        { nombre: nombreMetodo(segundoMetodo), monto: segundo },
        { nombre: nombreMetodo(otroMetodo), monto: otro },
      ].filter((p) => Number.isFinite(p.monto) && p.monto > 0)
    : [];
  return { titulo: 'Pago mixto', partes };
}

/** El pago en una sola línea: "Yape" o "Pago mixto: Yape S/ 50.00 + Plin S/ 85.00". */
export function textoPago(pago) {
  if (!pago) return '';
  if (!pago.partes.length) return pago.titulo;
  return `${pago.titulo}: ${pago.partes.map((p) => `${p.nombre} S/ ${p.monto.toFixed(2)}`).join(' + ')}`;
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

/**
 * Cómo se repartió el pago de una venta: [{ metodo, monto }]. Sigue la misma
 * regla que "Ventas por método" del servidor (pagos_por_metodo): un pago
 * mixto son dos partes, la de pago_efectivo (con su medio; sin medio =
 * efectivo) y la de pago_otro; al crédito, la segunda parte es 'CREDITO'.
 */
export function partesDelPago(venta) {
  if (!venta) return [];
  const total = Number(venta.total) || 0;
  if (venta.metodo_pago !== 'MIXTO') return [{ metodo: venta.metodo_pago, monto: total }];
  if (venta.pago_efectivo == null || venta.pago_otro == null) return [{ metodo: 'MIXTO', monto: total }];
  return [
    { metodo: venta.pago_efectivo_metodo || 'EFECTIVO', monto: Number(venta.pago_efectivo) || 0 },
    { metodo: venta.pago_otro_metodo || 'MIXTO', monto: Number(venta.pago_otro) || 0 },
  ].filter((p) => p.monto > 0);
}
