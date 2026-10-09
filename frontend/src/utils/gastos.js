// Formas de pago y comprobantes del módulo Gastos (los mismos códigos que
// valida el backend en handlers/gastos.rs).

export const METODOS_GASTO = [
  { valor: 'EFECTIVO_CAJA', etiqueta: 'Efectivo de la caja', corto: 'Caja' },
  { valor: 'EFECTIVO', etiqueta: 'Efectivo (fuera de caja)', corto: 'Efectivo' },
  { valor: 'YAPE', etiqueta: 'Yape', corto: 'Yape' },
  { valor: 'PLIN', etiqueta: 'Plin', corto: 'Plin' },
  { valor: 'TRANSFERENCIA', etiqueta: 'Transferencia', corto: 'Transferencia' },
  { valor: 'TARJETA', etiqueta: 'Tarjeta', corto: 'Tarjeta' },
  { valor: 'OTRO', etiqueta: 'Otro', corto: 'Otro' },
];

export const COMPROBANTES_GASTO = [
  { valor: 'BOLETA', etiqueta: 'Boleta' },
  { valor: 'FACTURA', etiqueta: 'Factura' },
  { valor: 'RECIBO', etiqueta: 'Recibo' },
  { valor: 'TICKET', etiqueta: 'Ticket' },
  { valor: 'OTRO', etiqueta: 'Otro' },
];

export const etiquetaMetodo = (valor, corto = false) => {
  const m = METODOS_GASTO.find((x) => x.valor === valor);
  return m ? (corto ? m.corto : m.etiqueta) : valor;
};

export const etiquetaComprobante = (valor) => COMPROBANTES_GASTO.find((x) => x.valor === valor)?.etiqueta || valor;

export const soles = (valor) =>
  `S/ ${Number(valor || 0).toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
