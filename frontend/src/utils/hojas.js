// Leer y escribir hojas de cálculo en el navegador, sin librerías.
//
//   leerHoja(archivo)  -> filas (cada una, una lista de textos)
//   escribirXlsx(filas) -> bytes de un .xlsx
//
// Lee Excel moderno (.xlsx), CSV (coma, punto y coma o tabulador) y los
// ".xls" que en realidad son una tabla HTML (así exportan muchos sistemas).
// El .xls binario antiguo no se lee: se pide guardarlo como .xlsx o CSV.

/** Error con un mensaje que se le puede mostrar tal cual al usuario. */
export class ErrorDeArchivo extends Error {}

// ---------------------------------------------------------------- ZIP

const le16 = (b, i) => b[i] | (b[i + 1] << 8);
const le32 = (b, i) => (b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24)) >>> 0;

async function descomprimir(bytes) {
  if (typeof DecompressionStream === 'undefined') {
    throw new ErrorDeArchivo('Este navegador no puede abrir archivos de Excel. Actualízalo, o guarda el archivo como CSV y súbelo de nuevo.');
  }
  const flujo = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(flujo).arrayBuffer());
}

/** Archivos de un ZIP: nombre -> función que devuelve sus bytes. */
function entradasZip(b) {
  // "Fin del directorio central": se busca desde el final.
  let fin = -1;
  for (let i = b.length - 22; i >= Math.max(0, b.length - 66000); i--) {
    if (le32(b, i) === 0x06054b50) {
      fin = i;
      break;
    }
  }
  if (fin < 0) throw new ErrorDeArchivo('El archivo de Excel está dañado o incompleto.');
  const total = le16(b, fin + 10);
  let p = le32(b, fin + 16);
  const entradas = new Map();
  const texto = new TextDecoder();
  for (let n = 0; n < total && p + 46 <= b.length; n++) {
    if (le32(b, p) !== 0x02014b50) break;
    const metodo = le16(b, p + 10);
    const comprimido = le32(b, p + 20);
    const largoNombre = le16(b, p + 28);
    const largoExtra = le16(b, p + 30);
    const largoComentario = le16(b, p + 32);
    const local = le32(b, p + 42);
    const nombre = texto.decode(b.subarray(p + 46, p + 46 + largoNombre));
    entradas.set(nombre, async () => {
      const inicio = local + 30 + le16(b, local + 26) + le16(b, local + 28);
      const datos = b.subarray(inicio, inicio + comprimido);
      if (metodo === 0) return datos;
      if (metodo === 8) return descomprimir(datos);
      throw new ErrorDeArchivo('El archivo de Excel usa una compresión que no se puede leer. Guárdalo de nuevo como .xlsx o CSV.');
    });
    p += 46 + largoNombre + largoExtra + largoComentario;
  }
  return entradas;
}

// ---------------------------------------------------------------- XLSX (leer)

const xml = (bytes) => new DOMParser().parseFromString(new TextDecoder().decode(bytes), 'application/xml');
const hijos = (nodo, nombre) => [...nodo.getElementsByTagNameNS('*', nombre)];

/** Un número de Excel como texto: 7750012000123 (no 7.75E12), 35.9 (no 35.900000000000006). */
function textoDeNumero(crudo) {
  const n = Number(crudo);
  if (!Number.isFinite(n)) return String(crudo).trim();
  return String(Number(n.toPrecision(15)));
}

/** "BC12" -> 54 (columna, empezando en 0). */
function columnaDe(referencia) {
  let n = 0;
  for (const c of referencia) {
    const codigo = c.charCodeAt(0);
    if (codigo < 65 || codigo > 90) break;
    n = n * 26 + (codigo - 64);
  }
  return n - 1;
}

async function leerXlsx(bytes) {
  const zip = entradasZip(bytes);
  const leer = async (nombre) => (zip.has(nombre) ? zip.get(nombre)() : null);

  // La primera hoja del libro.
  let ruta = 'xl/worksheets/sheet1.xml';
  const libro = await leer('xl/workbook.xml');
  const relaciones = await leer('xl/_rels/workbook.xml.rels');
  if (libro && relaciones) {
    const primera = hijos(xml(libro), 'sheet')[0];
    const idRelacion = primera && [...primera.attributes].find((a) => a.localName === 'id')?.value;
    const relacion = hijos(xml(relaciones), 'Relationship').find((r) => r.getAttribute('Id') === idRelacion);
    const destino = relacion?.getAttribute('Target');
    if (destino) ruta = destino.startsWith('/') ? destino.slice(1) : `xl/${destino.replace(/^\.\//, '')}`;
  }
  const hoja = await leer(ruta);
  if (!hoja) throw new ErrorDeArchivo('No se encontró ninguna hoja con datos en el archivo de Excel.');

  // Textos compartidos (un <si> puede traer varios <t>; los <rPh> son lectura fonética).
  const textos = [];
  const compartidos = await leer('xl/sharedStrings.xml');
  if (compartidos) {
    for (const si of hijos(xml(compartidos), 'si')) {
      textos.push(
        hijos(si, 't')
          .filter((t) => t.parentNode.localName !== 'rPh')
          .map((t) => t.textContent)
          .join('')
      );
    }
  }

  const filas = [];
  for (const row of hijos(xml(hoja), 'row')) {
    const indice = Number(row.getAttribute('r')) - 1;
    const fila = [];
    let siguiente = 0;
    for (const c of hijos(row, 'c')) {
      const referencia = c.getAttribute('r');
      const columna = referencia ? columnaDe(referencia) : siguiente;
      siguiente = columna + 1;
      const tipo = c.getAttribute('t');
      const v = hijos(c, 'v')[0]?.textContent ?? '';
      let valor;
      if (tipo === 's') valor = textos[Number(v)] ?? '';
      else if (tipo === 'inlineStr') valor = hijos(c, 't').map((t) => t.textContent).join('');
      else if (tipo === 'str' || tipo === 'e') valor = tipo === 'e' ? '' : v;
      else if (tipo === 'b') valor = v === '1' ? 'SI' : 'NO';
      else valor = v === '' ? '' : textoDeNumero(v);
      fila[columna] = String(valor).trim();
    }
    filas[Number.isFinite(indice) && indice >= 0 ? indice : filas.length] = fila;
  }
  return filas;
}

// ---------------------------------------------------------------- CSV y HTML

function decodificar(bytes) {
  // UTF-8 si es válido; si no, el de Windows (así guarda Excel los CSV en español).
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return new TextDecoder('windows-1252').decode(bytes);
  }
}

function separadorDe(texto) {
  const muestra = texto.split(/\r?\n/).slice(0, 10).join('\n').replace(/"[^"]*"/g, '');
  const cuenta = (c) => muestra.split(c).length - 1;
  return [';', '\t', ',', '|'].map((c) => [c, cuenta(c)]).sort((a, b) => b[1] - a[1])[0][0];
}

function leerCsv(texto) {
  const separador = separadorDe(texto);
  const filas = [];
  let fila = [];
  let celda = '';
  let entreComillas = false;
  for (let i = 0; i < texto.length; i++) {
    const c = texto[i];
    if (entreComillas) {
      if (c === '"' && texto[i + 1] === '"') {
        celda += '"';
        i++;
      } else if (c === '"') entreComillas = false;
      else celda += c;
    } else if (c === '"' && celda === '') entreComillas = true;
    else if (c === separador) {
      fila.push(celda.trim());
      celda = '';
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && texto[i + 1] === '\n') i++;
      fila.push(celda.trim());
      filas.push(fila);
      fila = [];
      celda = '';
    } else celda += c;
  }
  if (celda !== '' || fila.length) {
    fila.push(celda.trim());
    filas.push(fila);
  }
  return filas;
}

function leerTablaHtml(texto) {
  const documento = new DOMParser().parseFromString(texto, 'text/html');
  const tablas = [...documento.querySelectorAll('table')].sort((a, b) => b.rows.length - a.rows.length);
  if (!tablas.length) return [];
  return [...tablas[0].rows].map((tr) => [...tr.cells].map((td) => td.textContent.replace(/\s+/g, ' ').trim()));
}

// ---------------------------------------------------------------- Leer

/**
 * Lee la primera hoja del archivo. Devuelve sus filas como listas de textos
 * (las filas y celdas vacías quedan como huecos o cadenas vacías).
 */
export async function leerHoja(archivo) {
  if (archivo.size > 15 * 1024 * 1024) throw new ErrorDeArchivo('El archivo pesa más de 15 MB. Divídelo en partes.');
  const bytes = new Uint8Array(await archivo.arrayBuffer());
  if (bytes.length === 0) throw new ErrorDeArchivo('El archivo está vacío.');

  let filas;
  if (bytes[0] === 0x50 && bytes[1] === 0x4b) {
    filas = await leerXlsx(bytes);
  } else if (bytes[0] === 0xd0 && bytes[1] === 0xcf && bytes[2] === 0x11 && bytes[3] === 0xe0) {
    throw new ErrorDeArchivo(
      'Es un Excel de formato antiguo (.xls). Ábrelo en Excel, usa "Guardar como" y elige "Libro de Excel (.xlsx)" o "CSV", y sube ese archivo.'
    );
  } else {
    const crudo = decodificar(bytes);
    const texto = crudo.charCodeAt(0) === 0xfeff ? crudo.slice(1) : crudo; // sin la marca BOM
    filas = /<table[\s>]/i.test(texto.slice(0, 200000)) ? leerTablaHtml(texto) : leerCsv(texto);
  }
  // Huecos -> filas y celdas vacías, para que los índices sean los del archivo.
  return Array.from(filas, (f) => Array.from(f || [], (c) => (c == null ? '' : String(c))));
}

// ---------------------------------------------------------------- XLSX (escribir)

let tablaCrc = null;
function crc32(bytes) {
  if (!tablaCrc) {
    tablaCrc = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      tablaCrc[n] = c >>> 0;
    }
  }
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) crc = tablaCrc[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** ZIP sin comprimir (Excel lo abre igual). archivos: [[nombre, texto]] */
function zip(archivos) {
  const codificar = new TextEncoder();
  const partes = [];
  const central = [];
  let posicion = 0;
  for (const [nombre, contenido] of archivos) {
    const n = codificar.encode(nombre);
    const d = codificar.encode(contenido);
    const crc = crc32(d);
    const cabecera = new DataView(new ArrayBuffer(30));
    cabecera.setUint32(0, 0x04034b50, true);
    cabecera.setUint16(4, 20, true);
    cabecera.setUint16(6, 0x0800, true); // nombres en UTF-8
    cabecera.setUint32(14, crc, true);
    cabecera.setUint32(18, d.length, true);
    cabecera.setUint32(22, d.length, true);
    cabecera.setUint16(26, n.length, true);
    partes.push(new Uint8Array(cabecera.buffer), n, d);

    const dir = new DataView(new ArrayBuffer(46));
    dir.setUint32(0, 0x02014b50, true);
    dir.setUint16(4, 20, true);
    dir.setUint16(6, 20, true);
    dir.setUint16(8, 0x0800, true);
    dir.setUint32(16, crc, true);
    dir.setUint32(20, d.length, true);
    dir.setUint32(24, d.length, true);
    dir.setUint16(28, n.length, true);
    dir.setUint32(42, posicion, true);
    central.push(new Uint8Array(dir.buffer), n);
    posicion += 30 + n.length + d.length;
  }
  const largoCentral = central.reduce((s, p) => s + p.length, 0);
  const fin = new DataView(new ArrayBuffer(22));
  fin.setUint32(0, 0x06054b50, true);
  fin.setUint16(8, archivos.length, true);
  fin.setUint16(10, archivos.length, true);
  fin.setUint32(12, largoCentral, true);
  fin.setUint32(16, posicion, true);
  const todo = [...partes, ...central, new Uint8Array(fin.buffer)];
  const salida = new Uint8Array(todo.reduce((s, p) => s + p.length, 0));
  let i = 0;
  for (const p of todo) {
    salida.set(p, i);
    i += p.length;
  }
  return salida;
}

const escapar = (texto) =>
  String(texto)
    // Caracteres de control que XML no admite
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

function letraColumna(indice) {
  let letras = '';
  for (let n = indice + 1; n > 0; n = Math.floor((n - 1) / 26)) letras = String.fromCharCode(65 + ((n - 1) % 26)) + letras;
  return letras;
}

/**
 * Un .xlsx de una sola hoja. La primera fila va en negrita (títulos).
 * Los valores de tipo número se guardan como número; lo demás, como texto
 * (así un código "007501" no pierde sus ceros).
 */
export function escribirXlsx(filas, { hoja = 'Productos', anchos = [] } = {}) {
  const cuerpo = filas
    .map((fila, f) => {
      const celdas = fila
        .map((valor, c) => {
          if (valor === null || valor === undefined || valor === '') return '';
          const ref = `${letraColumna(c)}${f + 1}`;
          const estilo = f === 0 ? ' s="1"' : '';
          if (typeof valor === 'number' && Number.isFinite(valor)) return `<c r="${ref}"${estilo}><v>${valor}</v></c>`;
          return `<c r="${ref}"${estilo} t="inlineStr"><is><t xml:space="preserve">${escapar(valor)}</t></is></c>`;
        })
        .join('');
      return `<row r="${f + 1}">${celdas}</row>`;
    })
    .join('');
  const columnas = anchos.length
    ? `<cols>${anchos.map((a, i) => `<col min="${i + 1}" max="${i + 1}" width="${a}" customWidth="1"/>`).join('')}</cols>`
    : '';
  const cabeceraXml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>';
  const principal = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
  const relacionesNs = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
  return zip([
    [
      '[Content_Types].xml',
      `${cabeceraXml}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>`,
    ],
    [
      '_rels/.rels',
      `${cabeceraXml}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="${relacionesNs}/officeDocument" Target="xl/workbook.xml"/></Relationships>`,
    ],
    [
      'xl/workbook.xml',
      `${cabeceraXml}<workbook xmlns="${principal}" xmlns:r="${relacionesNs}"><bookViews><workbookView/></bookViews><sheets><sheet name="${escapar(hoja).slice(0, 31)}" sheetId="1" r:id="rId1"/></sheets></workbook>`,
    ],
    [
      'xl/_rels/workbook.xml.rels',
      `${cabeceraXml}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="${relacionesNs}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="${relacionesNs}/styles" Target="styles.xml"/></Relationships>`,
    ],
    [
      'xl/styles.xml',
      `${cabeceraXml}<styleSheet xmlns="${principal}"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>`,
    ],
    [
      'xl/worksheets/sheet1.xml',
      `${cabeceraXml}<worksheet xmlns="${principal}"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>${columnas}<sheetData>${cuerpo}</sheetData></worksheet>`,
    ],
  ]);
}

export const TIPO_XLSX = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
