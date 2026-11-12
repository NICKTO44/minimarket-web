const NOMBRE_COMPROBANTE_COMPRA = { FACTURA: 'Factura', BOLETA: 'Boleta', NOTA_VENTA: 'Nota de venta' };

// "Factura F001-123", "Boleta", o "—" si la compra no lo anotó.
export function textoComprobanteCompra(c) {
  const tipo = NOMBRE_COMPROBANTE_COMPRA[c?.tipo_comprobante];
  const numero = c?.factura_numero;
  if (tipo && numero) return `${tipo} ${numero}`;
  return tipo || numero || '—';
}
