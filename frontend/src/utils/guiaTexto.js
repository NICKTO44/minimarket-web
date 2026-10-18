// Textos de una guía de remisión que comparten el ticket impreso
// (components/GuiaImprimible.jsx) y su PDF (utils/pdfTicket.js), para que
// los dos digan exactamente lo mismo.

export const NOMBRE_DOCUMENTO = { RUC: 'RUC', DNI: 'DNI', CE: 'C.E.', PASAPORTE: 'Pasaporte' };

/** "T001-75" -> "T001-000075", como el resto de tickets. */
export function numeroConCeros(numero) {
  const [serie, correlativo] = String(numero || '').split('-');
  return correlativo ? `${serie}-${correlativo.padStart(6, '0')}` : serie;
}

/** Motivo del traslado: el de la lista, o el que se escribió en "Otros" (13). */
export function motivoDeGuia(datos, motivos) {
  return (
    (datos.motivo === '13' && datos.motivo_descripcion) ||
    motivos?.find((m) => m.codigo === datos.motivo)?.nombre ||
    datos.motivo_descripcion ||
    ''
  );
}

/** "Jr. Lima 123, Wanchaq - Cusco - Cusco" (sin la lista de distritos, solo la dirección). */
export function lugarDeGuia(punto, ubigeos) {
  const u = ubigeos?.find((x) => x.ubigeo === punto?.ubigeo);
  const direccion = punto?.direccion || '';
  return u ? `${direccion}, ${u.distrito} - ${u.provincia} - ${u.departamento}` : direccion;
}
