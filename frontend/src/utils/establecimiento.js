// Establecimiento anexo (Panel → Facturación, migración 0062): el negocio
// vende desde un local registrado en su Ficha RUC. Todas las impresiones
// (A4, A5, ticket de 80 mm y su PDF, guías) ponen "Sucursal: <dirección>"
// antes de la dirección fiscal. App.jsx lo fija con la configuración del
// negocio; sin establecimiento anexo nada de lo impreso cambia.
let sucursal = '';

export function usarEstablecimiento(direccion) {
  sucursal = String(direccion || '').trim();
}

/** Dirección del establecimiento anexo, o '' si emite desde el domicilio fiscal. */
export const sucursalActual = () => sucursal;

/**
 * Sucursal de un comprobante ya emitido (documento enviado a SUNAT): solo si
 * salió con un código de anexo; los emitidos desde el domicilio fiscal (0000)
 * se imprimen como siempre.
 */
export function sucursalDelDocumento(doc) {
  const codigo = String(doc?.company?.address?.codLocal || '0000');
  return codigo !== '0000' ? sucursal : '';
}
