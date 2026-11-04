// Módulo "Comprobante formato clásico" (COMPROBANTE_CLASICO): solo cambia
// cómo se imprimen boletas, facturas y notas (A4, A5, ticket y su PDF).
// - Los datos del cliente van en un recuadro: R.U.C./DNI, señores,
//   vendedor, dirección y teléfono.
// - Sin la hora de emisión.
// - El pago solo con su medio ("Efectivo", "Yape / Plin"), sin montos.
// App.jsx lo enciende según los módulos del negocio; apagado (lo de
// siempre) nada de lo impreso cambia.
let activo = false;

export function usarComprobanteClasico(valor) {
  activo = !!valor;
}

export const comprobanteClasico = () => activo;

/** "4/11/2026, 13:54:00" o "04/11/2026 13:54" -> solo la fecha. */
export function soloFecha(texto) {
  return String(texto || '').split(/,?\s+(?=\d{1,2}:\d{2})/)[0];
}
