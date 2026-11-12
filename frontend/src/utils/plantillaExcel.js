// Plantillas de Excel con formato para importar productos: encabezados de
// colores por grupo, precios en soles, fechas, listas desplegables, una ayuda
// al pararse en cada columna y una hoja de instrucciones. La primera hoja es
// la de los productos (la que lee "Importar productos"); los títulos son los
// mismos que ya reconoce utils/importacion.js.
import { escapar, letraColumna, zip } from './hojas';

const FILAS_CON_FORMATO = 1000; // filas preparadas (formato y listas)

// Colores de cada grupo de columnas (fondo del título).
export const GRUPOS = {
  OBLIGATORIO: { color: '166534', nombre: 'Obligatorio' },
  PRODUCTO: { color: '16A34A', nombre: 'Datos del producto' },
  LOTE: { color: 'EA580C', nombre: 'Lote y vencimiento' },
  FICHA: { color: '2563EB', nombre: 'Ficha del medicamento' },
  PRESENTACION: { color: '7C3AED', nombre: 'Presentaciones' },
  EXTRA: { color: '0F766E', nombre: 'Datos del rubro' },
};

// ---------------------------------------------------------------- estilos
// Índices de cellXfs (ver ESTILOS_XML).
const S = {
  NORMAL: 0,
  TEXTO: 1, // "@": un código "007501" no pierde sus ceros
  SOLES: 2,
  FECHA: 3,
  NUMERO: 4,
  TITULO_INSTR: 5,
  PARRAFO: 6,
  SUBTITULO: 7,
  NEGRITA: 8,
  CELDA_EJEMPLO: 9,
  SOLES_EJEMPLO: 10,
  FECHA_EJEMPLO: 11,
};
const ORDEN_GRUPOS = Object.keys(GRUPOS);
// El título de cada grupo va después de los fijos.
const estiloTitulo = (grupo) => 12 + ORDEN_GRUPOS.indexOf(grupo);
// La muestra de color de cada grupo en las instrucciones.
const estiloMuestra = (grupo) => 12 + ORDEN_GRUPOS.length + ORDEN_GRUPOS.indexOf(grupo);

function estilosXml() {
  const rellenos = ORDEN_GRUPOS.map((g) => `<fill><patternFill patternType="solid"><fgColor rgb="FF${GRUPOS[g].color}"/><bgColor indexed="64"/></patternFill></fill>`);
  const fijos = [
    '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>',
    '<xf numFmtId="49" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>',
    '<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>',
    '<xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>',
    '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>',
    '<xf numFmtId="0" fontId="3" fillId="0" borderId="0" xfId="0" applyFont="1"/>',
    '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1" vertical="top"/></xf>',
    '<xf numFmtId="0" fontId="4" fillId="0" borderId="0" xfId="0" applyFont="1"/>',
    '<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>',
    '<xf numFmtId="49" fontId="5" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyNumberFormat="1"/>',
    '<xf numFmtId="164" fontId="5" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyNumberFormat="1"/>',
    '<xf numFmtId="165" fontId="5" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyNumberFormat="1"/>',
  ];
  const titulos = ORDEN_GRUPOS.map(
    (_, i) =>
      `<xf numFmtId="0" fontId="2" fillId="${3 + i}" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>`
  );
  const muestras = ORDEN_GRUPOS.map(
    (_, i) => `<xf numFmtId="0" fontId="2" fillId="${3 + i}" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1"/>`
  );
  const xfs = [...fijos, ...titulos, ...muestras];
  return (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">' +
    '<numFmts count="2"><numFmt numFmtId="164" formatCode="&quot;S/ &quot;#,##0.00"/><numFmt numFmtId="165" formatCode="dd/mm/yyyy"/></numFmts>' +
    '<fonts count="6">' +
    '<font><sz val="11"/><name val="Calibri"/></font>' +
    '<font><b/><sz val="11"/><name val="Calibri"/></font>' +
    '<font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Calibri"/></font>' +
    '<font><b/><sz val="15"/><color rgb="FF14532D"/><name val="Calibri"/></font>' +
    '<font><b/><sz val="12"/><color rgb="FF166534"/><name val="Calibri"/></font>' +
    '<font><i/><sz val="11"/><color rgb="FF4B5563"/><name val="Calibri"/></font>' +
    '</fonts>' +
    `<fills count="${3 + ORDEN_GRUPOS.length}"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill>` +
    '<fill><patternFill patternType="solid"><fgColor rgb="FFF3F4F6"/><bgColor indexed="64"/></patternFill></fill>' +
    `${rellenos.join('')}</fills>` +
    '<borders count="2"><border><left/><right/><top/><bottom/><diagonal/></border>' +
    '<border><left style="thin"><color rgb="FFD1D5DB"/></left><right style="thin"><color rgb="FFD1D5DB"/></right><top style="thin"><color rgb="FFD1D5DB"/></top><bottom style="thin"><color rgb="FFD1D5DB"/></bottom><diagonal/></border></borders>' +
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>' +
    `<cellXfs count="${xfs.length}">${xfs.join('')}</cellXfs>` +
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>' +
    '</styleSheet>'
  );
}

const ESTILO_DE_TIPO = { texto: S.TEXTO, soles: S.SOLES, fecha: S.FECHA, numero: S.NUMERO, entero: S.NUMERO, lista: S.NORMAL };
const ESTILO_EJEMPLO = { soles: S.SOLES_EJEMPLO, fecha: S.FECHA_EJEMPLO };

// Fecha "AAAA-MM-DD" -> número de fecha de Excel.
function serialFecha(iso) {
  const [a, m, d] = String(iso).split('-').map(Number);
  return Math.round((Date.UTC(a, m - 1, d) - Date.UTC(1899, 11, 30)) / 86400000);
}

function celda(ref, valor, estilo) {
  const s = estilo ? ` s="${estilo}"` : '';
  if (valor === null || valor === undefined || valor === '') return estilo ? `<c r="${ref}"${s}/>` : '';
  if (typeof valor === 'number' && Number.isFinite(valor)) return `<c r="${ref}"${s}><v>${valor}</v></c>`;
  return `<c r="${ref}"${s} t="inlineStr"><is><t xml:space="preserve">${escapar(valor)}</t></is></c>`;
}

const recortar = (texto, max) => escapar(String(texto).slice(0, max));

/**
 * Hoja de productos. columnas: [{ titulo, grupo, tipo, ancho, ayuda, lista }]
 * (lista = nombre de una columna de la hoja Listas). filas: valores ya
 * cargados (exportar); las fechas como "AAAA-MM-DD".
 */
function hojaProductos(columnas, filas, rangosListas) {
  const cols = columnas
    .map((c, i) => `<col min="${i + 1}" max="${i + 1}" width="${c.ancho || 16}" customWidth="1" style="${ESTILO_DE_TIPO[c.tipo] ?? 0}"/>`)
    .join('');
  const titulo = `<row r="1" ht="34" customHeight="1">${columnas
    .map((c, i) => celda(`${letraColumna(i)}1`, c.titulo, estiloTitulo(c.grupo)))
    .join('')}</row>`;
  const cuerpo = filas
    .map((fila, f) => {
      const r = f + 2;
      return `<row r="${r}">${columnas
        .map((c, i) => {
          let v = fila[i];
          if (c.tipo === 'fecha' && typeof v === 'string' && /^\d{4}-\d{2}-\d{2}/.test(v)) v = serialFecha(v.slice(0, 10));
          if (c.tipo === 'texto' && typeof v === 'number') v = String(v);
          return celda(`${letraColumna(i)}${r}`, v, ESTILO_DE_TIPO[c.tipo] ?? 0);
        })
        .join('')}</row>`;
    })
    .join('');
  const ultima = FILAS_CON_FORMATO + 1;
  const validaciones = columnas
    .map((c, i) => {
      const rango = `${letraColumna(i)}2:${letraColumna(i)}${ultima}`;
      const ayuda = c.ayuda ? ` showInputMessage="1" promptTitle="${recortar(c.titulo.replace(' *', ''), 32)}" prompt="${recortar(c.ayuda, 250)}"` : '';
      if (c.tipo === 'lista' && rangosListas[c.lista]) {
        return `<dataValidation type="list" allowBlank="1" errorStyle="warning" showErrorMessage="1" errorTitle="No está en la lista" error="Elige uno de la lista. Si es nuevo, puedes dejarlo: se creará al importar."${ayuda} sqref="${rango}"><formula1>${rangosListas[c.lista]}</formula1></dataValidation>`;
      }
      if (c.tipo === 'fecha') {
        return `<dataValidation type="date" operator="between" allowBlank="1" errorStyle="warning" showErrorMessage="1" errorTitle="Fecha" error="Escribe la fecha así: 31/12/2027."${ayuda} sqref="${rango}"><formula1>36526</formula1><formula2>73051</formula2></dataValidation>`;
      }
      if (c.tipo === 'soles' || c.tipo === 'numero' || c.tipo === 'entero') {
        const tipo = c.tipo === 'entero' ? 'whole' : 'decimal';
        return `<dataValidation type="${tipo}" operator="greaterThanOrEqual" allowBlank="1" errorStyle="stop" showErrorMessage="1" errorTitle="Número" error="Escribe solo el número (sin letras), 0 o más."${ayuda} sqref="${rango}"><formula1>0</formula1></dataValidation>`;
      }
      return ayuda ? `<dataValidation type="none" allowBlank="1"${ayuda} sqref="${rango}"/>` : '';
    })
    .filter(Boolean);
  const dv = validaciones.length ? `<dataValidations count="${validaciones.length}">${validaciones.join('')}</dataValidations>` : '';
  const ultimaCol = letraColumna(columnas.length - 1);
  return (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">' +
    '<sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>' +
    '<sheetViews><sheetView workbookViewId="0" tabSelected="1"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>' +
    `<cols>${cols}</cols><sheetData>${titulo}${cuerpo}</sheetData>` +
    `<autoFilter ref="A1:${ultimaCol}${Math.max(2, filas.length + 1)}"/>${dv}` +
    '<pageMargins left="0.3" right="0.3" top="0.4" bottom="0.4" header="0.2" footer="0.2"/>' +
    '<pageSetup paperSize="9" orientation="landscape" fitToWidth="1" fitToHeight="0"/>' +
    '</worksheet>'
  );
}

/**
 * Hoja de instrucciones. bloques: [{ tipo: 'titulo'|'subtitulo'|'parrafo'|'grupos'|'tabla', ... }]
 */
function hojaInstrucciones(bloques, columnasEjemplo) {
  const filas = [];
  const unidas = []; // filas de párrafo: A..H en una sola celda
  const fila = (celdas, alto) => filas.push({ celdas, alto });
  for (const b of bloques) {
    if (b.tipo === 'titulo') fila([[b.texto, S.TITULO_INSTR]], 26);
    else if (b.tipo === 'subtitulo') {
      fila([]);
      fila([[b.texto, S.SUBTITULO]]);
    } else if (b.tipo === 'parrafo') {
      fila([[b.texto, S.PARRAFO]], b.alto || 30);
      unidas.push(filas.length);
    }
    else if (b.tipo === 'grupos') {
      for (const g of b.grupos) fila([[GRUPOS[g].nombre, estiloMuestra(g)], [b.textos[g] || '', S.PARRAFO]]);
    } else if (b.tipo === 'columnas') {
      for (const c of b.columnas) fila([[c.titulo.replace(' *', ''), estiloMuestra(c.grupo)], [c.ayuda || '', S.PARRAFO]], c.ayuda && c.ayuda.length > 90 ? 32 : undefined);
    } else if (b.tipo === 'ejemplo') {
      fila(columnasEjemplo.map((c) => [c.titulo, estiloTitulo(c.grupo)]), 34);
      for (const ej of b.filas) {
        fila(
          columnasEjemplo.map((c, i) => {
            let v = ej[i];
            if (c.tipo === 'fecha' && typeof v === 'string') v = serialFecha(v);
            return [v, ESTILO_EJEMPLO[c.tipo] ?? S.CELDA_EJEMPLO];
          })
        );
      }
    }
  }
  const anchos = [24, 70, ...columnasEjemplo.slice(2).map((c) => c.ancho || 16)];
  // Las columnas A y B de las instrucciones son más anchas que las del ejemplo.
  const cols = anchos.map((a, i) => `<col min="${i + 1}" max="${i + 1}" width="${Math.max(a, columnasEjemplo[i]?.ancho || 0)}" customWidth="1"/>`).join('');
  const cuerpo = filas
    .map(({ celdas, alto }, f) => {
      const r = f + 1;
      const ht = alto ? ` ht="${alto}" customHeight="1"` : '';
      return `<row r="${r}"${ht}>${celdas.map(([v, s], i) => celda(`${letraColumna(i)}${r}`, v, s)).join('')}</row>`;
    })
    .join('');
  return (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">' +
    '<sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>' +
    `<sheetViews><sheetView workbookViewId="0" showGridLines="0"/></sheetViews><cols>${cols}</cols><sheetData>${cuerpo}</sheetData>` +
    (unidas.length ? `<mergeCells count="${unidas.length}">${unidas.map((r) => `<mergeCell ref="A${r}:H${r}"/>`).join('')}</mergeCells>` : '') +
    '<pageMargins left="0.3" right="0.3" top="0.4" bottom="0.4" header="0.2" footer="0.2"/>' +
    '<pageSetup paperSize="9" orientation="landscape" fitToWidth="1" fitToHeight="0"/>' +
    '</worksheet>'
  );
}

/** Hoja oculta con las listas: { nombre: [valores] } -> XML y rangos "Listas!$A$2:$A$n". */
function hojaListas(listas) {
  const nombres = Object.keys(listas);
  const rangos = {};
  const largo = Math.max(1, ...nombres.map((n) => listas[n].length));
  const filas = [];
  filas.push(`<row r="1">${nombres.map((n, i) => celda(`${letraColumna(i)}1`, n, S.NEGRITA)).join('')}</row>`);
  for (let f = 0; f < largo; f += 1) {
    filas.push(
      `<row r="${f + 2}">${nombres.map((n, i) => celda(`${letraColumna(i)}${f + 2}`, listas[n][f] ?? '', 0)).join('')}</row>`
    );
  }
  nombres.forEach((n, i) => {
    if (listas[n].length) rangos[n] = `Listas!$${letraColumna(i)}$2:$${letraColumna(i)}$${listas[n].length + 1}`;
  });
  const xmlHoja =
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    `<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>${filas.join('')}</sheetData></worksheet>`;
  return { xml: xmlHoja, rangos };
}

/**
 * El .xlsx de la plantilla: hoja "Productos" (primera), "Instrucciones" y
 * "Listas" (oculta). { columnas, filas = [], instrucciones = [], ejemplos = [], listas = {} }
 */
export function libroPlantilla({ columnas, filas = [], instrucciones = [], ejemplos = [], listas = {} }) {
  const cabecera = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>';
  const principal = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
  const rel = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
  const { xml: listasXml, rangos } = hojaListas(listas);
  const bloques = [...instrucciones, ...(ejemplos.length ? [{ tipo: 'subtitulo', texto: 'Ejemplo de cómo llenarla' }, { tipo: 'ejemplo', filas: ejemplos }] : [])];
  const hojas = [
    ['Productos', hojaProductos(columnas, filas, rangos), ''],
    ['Instrucciones', hojaInstrucciones(bloques, columnas), ''],
    ['Listas', listasXml, ' state="hidden"'],
  ];
  return zip([
    [
      '[Content_Types].xml',
      `${cabecera}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>${hojas
        .map((_, i) => `<Override PartName="/xl/worksheets/sheet${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`)
        .join('')}<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>`,
    ],
    ['_rels/.rels', `${cabecera}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="${rel}/officeDocument" Target="xl/workbook.xml"/></Relationships>`],
    [
      'xl/workbook.xml',
      `${cabecera}<workbook xmlns="${principal}" xmlns:r="${rel}"><bookViews><workbookView activeTab="0"/></bookViews><sheets>${hojas
        .map(([nombre, , oculta], i) => `<sheet name="${nombre}" sheetId="${i + 1}"${oculta} r:id="rId${i + 1}"/>`)
        .join('')}</sheets></workbook>`,
    ],
    [
      'xl/_rels/workbook.xml.rels',
      `${cabecera}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">${hojas
        .map((_, i) => `<Relationship Id="rId${i + 1}" Type="${rel}/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`)
        .join('')}<Relationship Id="rId${hojas.length + 1}" Type="${rel}/styles" Target="styles.xml"/></Relationships>`,
    ],
    ['xl/styles.xml', estilosXml()],
    ...hojas.map(([, contenido], i) => [`xl/worksheets/sheet${i + 1}.xml`, contenido]),
  ]);
}
