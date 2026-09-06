/**
 * Arma el contenido del código QR según el formato oficial de SUNAT
 * (RS 193-2020/SUNAT), campos separados por "|":
 *
 *   RUC | TIPO DOC | SERIE | NÚMERO | MTO TOTAL IGV | MTO TOTAL |
 *   FECHA EMISIÓN | TIPO DOC ADQUIRENTE | NÚM. DOC ADQUIRENTE |
 *   VALOR RESUMEN | VALOR DE LA FIRMA |
 *
 * "VALOR RESUMEN" es el hash real que FacturaLibre devolvió al firmar el
 * comprobante — nunca se inventa ni se recalcula. "VALOR DE LA FIRMA" se
 * deja vacío porque hoy no lo capturamos de la respuesta de FacturaLibre
 * (la propia norma de SUNAT indica dejar vacíos los campos sin
 * información disponible, en vez de inventar un valor).
 */
export function construirCadenaQrSunat({
  ruc,
  tipoDocumento, // '01' Factura, '03' Boleta (catálogo 01 de SUNAT)
  serie,
  numero,
  igv,
  total,
  fechaEmision, // formato "YYYY-MM-DD", tal cual lo devuelve el backend
  tipoDocCliente, // catálogo 06: '1' DNI, '6' RUC, '0' sin documento
  numDocCliente,
  hash,
}) {
  const numeroConCeros = String(numero).padStart(8, '0');

  const campos = [
    ruc,
    tipoDocumento,
    serie,
    numeroConCeros,
    Number(igv).toFixed(2),
    Number(total).toFixed(2),
    fechaEmision,
    tipoDocCliente,
    numDocCliente,
    hash,
    '', // valor de la firma — vacío, ver nota arriba
  ];

  return campos.join('|') + '|';
}