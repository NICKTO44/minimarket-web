// Módulo "Códigos de barras" (CODIGOS_BARRAS): genera códigos aleatorios en
// tandas y arma las etiquetas para imprimir. Todo es del navegador: no se
// guarda nada en el servidor. Para no repetir, se compara contra los
// productos del inventario (activos y desactivados) y contra un historial
// de lo ya generado en este equipo (localStorage, por negocio).
//
// EAN-13 con prefijo 20–29 (uso interno de tiendas: no choca con códigos de
// fábrica) o Code 128 con letras. Las barras se dibujan en SVG; probado con
// un lector (zbar) sobre las etiquetas impresas.

import { escapar } from './facturaA4';

// ---------- EAN-13 ----------
const EAN_L = ['0001101', '0011001', '0010011', '0111101', '0100011', '0110001', '0101111', '0111011', '0110111', '0001011'];
const EAN_G = ['0100111', '0110011', '0011011', '0100001', '0011101', '0111001', '0000101', '0010001', '0001001', '0010111'];
const EAN_R = ['1110010', '1100110', '1101100', '1000010', '1011100', '1001110', '1010000', '1000100', '1001000', '1110100'];
const EAN_PARIDAD = ['LLLLLL', 'LLGLGG', 'LLGGLG', 'LLGGGL', 'LGLLGG', 'LGGLLG', 'LGGGLL', 'LGLGLG', 'LGLGGL', 'LGGLGL'];

/** Dígito de control de un EAN-13 a partir de sus 12 primeros dígitos. */
export function digitoControl(doce) {
  let suma = 0;
  for (let i = 0; i < 12; i++) suma += Number(doce[i]) * (i % 2 ? 3 : 1);
  return String((10 - (suma % 10)) % 10);
}

function modulosEan13(codigo) {
  const paridad = EAN_PARIDAD[Number(codigo[0])];
  let m = '101';
  for (let i = 1; i <= 6; i++) m += (paridad[i - 1] === 'L' ? EAN_L : EAN_G)[Number(codigo[i])];
  m += '01010';
  for (let i = 7; i <= 12; i++) m += EAN_R[Number(codigo[i])];
  return `${m}101`;
}

// ---------- Code 128 (juego B: letras, números y signos) ----------
// Los 106 patrones (0–105), tomados de la librería python-barcode.
const C128 = [
  '11011001100', '11001101100', '11001100110', '10010011000', '10010001100', '10001001100',
  '10011001000', '10011000100', '10001100100', '11001001000', '11001000100', '11000100100',
  '10110011100', '10011011100', '10011001110', '10111001100', '10011101100', '10011100110',
  '11001110010', '11001011100', '11001001110', '11011100100', '11001110100', '11101101110',
  '11101001100', '11100101100', '11100100110', '11101100100', '11100110100', '11100110010',
  '11011011000', '11011000110', '11000110110', '10100011000', '10001011000', '10001000110',
  '10110001000', '10001101000', '10001100010', '11010001000', '11000101000', '11000100010',
  '10110111000', '10110001110', '10001101110', '10111011000', '10111000110', '10001110110',
  '11101110110', '11010001110', '11000101110', '11011101000', '11011100010', '11011101110',
  '11101011000', '11101000110', '11100010110', '11101101000', '11101100010', '11100011010',
  '11101111010', '11001000010', '11110001010', '10100110000', '10100001100', '10010110000',
  '10010000110', '10000101100', '10000100110', '10110010000', '10110000100', '10011010000',
  '10011000010', '10000110100', '10000110010', '11000010010', '11001010000', '11110111010',
  '11000010100', '10001111010', '10100111100', '10010111100', '10010011110', '10111100100',
  '10011110100', '10011110010', '11110100100', '11110010100', '11110010010', '11011011110',
  '11011110110', '11110110110', '10101111000', '10100011110', '10001011110', '10111101000',
  '10111100010', '11110101000', '11110100010', '10111011110', '10111101110', '11101011110',
  '11110101110', '11010000100', '11010010000', '11010011100',
];
const C128_STOP = '1100011101011';

function modulosCode128(texto) {
  const valores = [104]; // START B
  for (const letra of texto) valores.push(letra.charCodeAt(0) - 32);
  let suma = valores[0];
  for (let i = 1; i < valores.length; i++) suma += valores[i] * i;
  valores.push(suma % 103);
  return valores.map((v) => C128[v]).join('') + C128_STOP;
}

/** Las barras como SVG (se estira al ancho de la etiqueta). */
export function svgDeCodigo(codigo, tipo) {
  const modulos = tipo === 'EAN13' ? modulosEan13(codigo) : modulosCode128(codigo);
  const margen = tipo === 'EAN13' ? 9 : 10; // zona en blanco a cada lado
  let barras = '';
  for (let i = 0; i < modulos.length; ) {
    if (modulos[i] === '1') {
      let j = i;
      while (j < modulos.length && modulos[j] === '1') j++;
      barras += `<rect x="${margen + i}" y="0" width="${j - i}" height="40"/>`;
      i = j;
    } else i++;
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${modulos.length + margen * 2} 40" preserveAspectRatio="none" shape-rendering="crispEdges" fill="#000">${barras}</svg>`;
}

// ---------- Generar ----------
export const TIPOS = {
  EAN13: { label: 'EAN-13 (solo números, recomendado)', prefijo: '20' },
  CODE128: { label: 'Code 128 (letras y números)', prefijo: 'MS' },
};

// Sin I, O, 0 ni 1 para que no se confundan al leerlos.
const LETRAS = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

function azar(n) {
  const a = new Uint32Array(1);
  crypto.getRandomValues(a);
  return a[0] % n;
}

/** null si el prefijo sirve; si no, el motivo. */
export function errorDePrefijo(tipo, prefijo) {
  if (tipo === 'EAN13' && !/^\d{0,4}$/.test(prefijo)) return 'En EAN-13 el prefijo solo lleva números (hasta 4).';
  if (tipo === 'CODE128' && !/^[A-Z0-9]{0,6}$/.test(prefijo)) return 'El prefijo solo lleva letras y números (hasta 6).';
  return null;
}

function unCodigo(tipo, prefijo, largo) {
  let s = prefijo;
  if (tipo === 'EAN13') {
    while (s.length < 12) s += azar(10);
    return s + digitoControl(s);
  }
  for (let i = 0; i < largo; i++) s += LETRAS[azar(LETRAS.length)];
  return s;
}

/**
 * Una tanda de códigos nuevos. ocupados: Set de códigos que no se pueden
 * repetir (en MAYÚSCULAS). Devuelve null si con ese prefijo ya no alcanzan.
 */
export function generarTanda({ tipo, prefijo, largo = 8, cantidad, ocupados }) {
  const nuevos = new Set();
  for (let intentos = 0; nuevos.size < cantidad && intentos < cantidad * 200; intentos++) {
    const c = unCodigo(tipo, prefijo, largo);
    if (!ocupados.has(c) && !nuevos.has(c)) nuevos.add(c);
  }
  return nuevos.size < cantidad ? null : [...nuevos];
}

// ---------- Historial en este equipo (por negocio) ----------
const clave = (negocio) => `monspeet-codigos-barras:${negocio || 'negocio'}`;

export function leerHistorial(negocio) {
  try {
    const g = JSON.parse(localStorage.getItem(clave(negocio)) || 'null');
    return { codigos: Array.isArray(g?.codigos) ? g.codigos : [], tanda: Number(g?.tanda) || 0 };
  } catch {
    return { codigos: [], tanda: 0 };
  }
}

export function guardarHistorial(negocio, historial) {
  try {
    localStorage.setItem(clave(negocio), JSON.stringify(historial));
  } catch {
    // Sin espacio o en modo privado: solo se pierde el historial.
  }
}

export function borrarHistorial(negocio) {
  try {
    localStorage.removeItem(clave(negocio));
  } catch {
    // nada
  }
}

// ---------- Papel ----------
// hoja: tamaño de @page. rollo: una etiqueta por página (etiquetadora).
// alto 'auto' (ticketera): el largo del papel depende de cuántas etiquetas hay.
export const PAPELES = [
  { valor: 'A4_4', label: 'Hoja A4 – 4 columnas (48 × 25 mm)', hoja: 'A4', margen: 8, columnas: 4, ancho: 48.5, alto: 25 },
  { valor: 'A4_3', label: 'Hoja A4 – 3 columnas (65 × 30 mm)', hoja: 'A4', margen: 8, columnas: 3, ancho: 64.6, alto: 30 },
  { valor: 'A4_5', label: 'Hoja A4 – 5 columnas (38 × 21 mm)', hoja: 'A4', margen: 8, columnas: 5, ancho: 38.8, alto: 21.2 },
  { valor: 'ROLLO_50X25', label: 'Etiquetadora – rollo 50 × 25 mm', hoja: '50mm 25mm', margen: 0, columnas: 1, ancho: 50, alto: 25, rollo: true },
  { valor: 'ROLLO_40X30', label: 'Etiquetadora – rollo 40 × 30 mm', hoja: '40mm 30mm', margen: 0, columnas: 1, ancho: 40, alto: 30, rollo: true },
  { valor: 'ROLLO_40X25', label: 'Etiquetadora – rollo 40 × 25 mm', hoja: '40mm 25mm', margen: 0, columnas: 1, ancho: 40, alto: 25, rollo: true },
  { valor: 'TICKET_58', label: 'Ticketera 58 mm (una debajo de otra)', hoja: '58mm', margen: 2, columnas: 1, ancho: 48, alto: 24, ticket: true },
  { valor: 'TICKET_80', label: 'Ticketera 80 mm (dos por fila)', hoja: '80mm', margen: 2, columnas: 2, ancho: 36, alto: 22, ticket: true },
];

export const papelDe = (valor) => PAPELES.find((p) => p.valor === valor) || PAPELES[0];

/** Etiquetas por hoja A4 (null si no es A4). */
export function etiquetasPorHoja(papel) {
  return papel.hoja === 'A4' ? papel.columnas * Math.floor((297 - 2 * papel.margen) / papel.alto) : null;
}

/** CSS de la hoja de etiquetas (se usa en la vista previa y al imprimir). */
export function cssEtiquetas(papel) {
  return `
.ce-hoja { display: grid; grid-template-columns: repeat(${papel.columnas}, ${papel.ancho}mm); justify-content: start; }
.ce-eti { box-sizing: border-box; width: ${papel.ancho}mm; height: ${papel.alto}mm; padding: 1.2mm 1.5mm; display: flex; flex-direction: column; align-items: center; justify-content: center; overflow: hidden; background: #fff; color: #000; break-inside: avoid; page-break-inside: avoid; }
.ce-eti.ce-borde { border: 0.2mm solid #999; }
.ce-eti svg { display: block; width: 100%; flex: 1 1 auto; min-height: 0; }
.ce-texto { max-width: 100%; font: 700 2.6mm/1.1 Arial, Helvetica, sans-serif; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ce-numero { font: 2.9mm/1.1 Consolas, 'Courier New', monospace; letter-spacing: 0.4mm; }`;
}

/** Las etiquetas (HTML), cada código repetido `copias` veces. */
export function htmlEtiquetas({ codigos, tipo, copias = 1, texto = '', borde = false }) {
  const arriba = texto ? `<div class="ce-texto">${escapar(texto)}</div>` : '';
  const clase = `ce-eti${borde ? ' ce-borde' : ''}`;
  return codigos
    .map((c) => {
      const una = `<div class="${clase}">${arriba}${svgDeCodigo(c, tipo)}<div class="ce-numero">${escapar(c)}</div></div>`;
      return una.repeat(copias);
    })
    .join('');
}

/** El documento completo para imprimir (va a imprimirHtml de facturaA4.js). */
export function documentoParaImprimir({ papel, codigos, tipo, copias, texto, borde }) {
  let tamano = papel.hoja;
  if (papel.ticket) {
    const filas = Math.ceil((codigos.length * copias) / papel.columnas);
    tamano = `${papel.hoja} ${Math.ceil(filas * papel.alto + papel.margen * 2 + 2)}mm`;
  }
  const porPagina = papel.rollo ? '.ce-eti { break-after: page; page-break-after: always; } .ce-eti:last-child { break-after: auto; page-break-after: auto; }' : '';
  return `<!DOCTYPE html><html lang="es"><head><meta charset="utf-8"><title>Códigos de barras</title><style>
@page { size: ${tamano}; margin: ${papel.margen}mm; }
html, body { margin: 0; padding: 0; background: #fff; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
${cssEtiquetas(papel)}
${porPagina}
</style></head><body><div class="ce-hoja">${htmlEtiquetas({ codigos, tipo, copias, texto, borde })}</div></body></html>`;
}
