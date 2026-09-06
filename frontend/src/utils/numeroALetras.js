const UNIDADES = [
  '', 'UNO', 'DOS', 'TRES', 'CUATRO', 'CINCO', 'SEIS', 'SIETE', 'OCHO', 'NUEVE',
  'DIEZ', 'ONCE', 'DOCE', 'TRECE', 'CATORCE', 'QUINCE', 'DIECISÉIS', 'DIECISIETE', 'DIECIOCHO', 'DIECINUEVE',
];
const VEINTES = [
  'VEINTE', 'VEINTIUNO', 'VEINTIDÓS', 'VEINTITRÉS', 'VEINTICUATRO', 'VEINTICINCO',
  'VEINTISÉIS', 'VEINTISIETE', 'VEINTIOCHO', 'VEINTINUEVE',
];
const DECENAS = [
  '', '', 'VEINTE', 'TREINTA', 'CUARENTA', 'CINCUENTA', 'SESENTA', 'SETENTA', 'OCHENTA', 'NOVENTA',
];
const CENTENAS = [
  '', 'CIENTO', 'DOSCIENTOS', 'TRESCIENTOS', 'CUATROCIENTOS', 'QUINIENTOS',
  'SEISCIENTOS', 'SETECIENTOS', 'OCHOCIENTOS', 'NOVECIENTOS',
];

function convertirGrupo(n) {
  if (n === 0) return '';
  if (n === 100) return 'CIEN';

  let texto = '';
  const centena = Math.floor(n / 100);
  const resto = n % 100;

  if (centena > 0) texto += CENTENAS[centena] + ' ';

  if (resto < 20) {
    texto += UNIDADES[resto];
  } else if (resto < 30) {
    texto += VEINTES[resto - 20];
  } else {
    const decena = Math.floor(resto / 10);
    const unidad = resto % 10;
    texto += DECENAS[decena];
    if (unidad > 0) texto += ' Y ' + UNIDADES[unidad];
  }

  return texto.trim();
}

function convertirEntero(n) {
  if (n === 0) return 'CERO';

  const millones = Math.floor(n / 1000000);
  const miles = Math.floor((n % 1000000) / 1000);
  const resto = n % 1000;

  let partes = [];

  if (millones > 0) {
    partes.push(millones === 1 ? 'UN MILLÓN' : `${convertirGrupo(millones)} MILLONES`);
  }
  if (miles > 0) {
    partes.push(miles === 1 ? 'MIL' : `${convertirGrupo(miles)} MIL`);
  }
  if (resto > 0) {
    partes.push(convertirGrupo(resto));
  }

  return partes.join(' ').trim();
}

/**
 * Convierte un monto en soles a su representación en letras, formato
 * estándar de comprobantes peruanos: "DOSCIENTOS CON 00/100 SOLES".
 * Puramente matemático/textual — no depende de ningún dato oficial de
 * SUNAT ni de FacturaLibre, así que no hay riesgo de generar algo
 * incorrecto que aparente ser un dato fiscal validado.
 */
export function montoEnLetras(monto) {
  const valor = Math.abs(Number(monto) || 0);
  const parteEntera = Math.floor(valor);
  const centavos = Math.round((valor - parteEntera) * 100);

  const enteroTexto = convertirEntero(parteEntera) || 'CERO';
  const centavosTexto = String(centavos).padStart(2, '0');

  return `${enteroTexto} CON ${centavosTexto}/100 SOLES`;
}