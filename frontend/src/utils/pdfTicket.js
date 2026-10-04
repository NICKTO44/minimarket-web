// Exportar una venta a PDF (nota de venta, boleta o factura) con el mismo
// contenido del ticket que se imprime (components/Recibo.jsx).
//
// El PDF se arma aquí mismo, sin librerías: una página de 80 mm de ancho y
// el alto que haga falta, texto en Courier (letra de ticket, que todo visor
// de PDF trae) y el QR de SUNAT dibujado con cuadraditos, así sale nítido a
// cualquier tamaño. Si cambia lo que muestra el ticket impreso, hay que
// reflejarlo también aquí.
import QRCode from 'qrcode';
import { desgloseDeComprobante, etiquetaTasa } from './igv';
import { formatoCantidad, subtotalLinea } from './medidas';
import { montoEnLetras } from './numeroALetras';
import { construirCadenaQrSunat } from './qrSunat';

const ANCHO = 226.77; // 80 mm en puntos
const MARGEN = 11;
const UTIL = ANCHO - MARGEN * 2;
const TAM = 8.5; // letra normal
const ALTO_LINEA = 1.32; // respecto al tamaño de letra
const ANCHO_LETRA = 0.6; // Courier: todas las letras miden lo mismo

// Letras de Windows-1252 que no están en Latin-1 (comillas curvas, rayas...).
const CP1252 = {
  '€': 0x80, '‚': 0x82, 'ƒ': 0x83, '„': 0x84, '…': 0x85, '†': 0x86, '‡': 0x87, 'ˆ': 0x88, '‰': 0x89,
  'Š': 0x8a, '‹': 0x8b, 'Œ': 0x8c, 'Ž': 0x8e, '‘': 0x91, '’': 0x92, '“': 0x93, '”': 0x94, '•': 0x95,
  '–': 0x96, '—': 0x97, '˜': 0x98, '™': 0x99, 'š': 0x9a, '›': 0x9b, 'œ': 0x9c, 'ž': 0x9e, 'Ÿ': 0x9f,
};

/** Texto -> bytes que la letra del PDF sabe dibujar (lo demás, "?"). */
function aBytes(texto) {
  const bytes = [];
  for (const letra of String(texto ?? '')) {
    const c = letra.codePointAt(0);
    if (c === 0x09 || c === 0x0a || c === 0x0d) bytes.push(0x20);
    else if (c >= 0x20 && c < 0x7f) bytes.push(c);
    else if (c >= 0xa0 && c <= 0xff) bytes.push(c);
    else if (CP1252[letra]) bytes.push(CP1252[letra]);
    else if (c >= 0x20) bytes.push(0x3f);
  }
  return bytes;
}

/** Bytes -> cadena literal de PDF, solo con caracteres ASCII. */
function literal(bytes) {
  let s = '';
  for (const b of bytes) {
    if (b === 0x28 || b === 0x29 || b === 0x5c) s += `\\${String.fromCharCode(b)}`;
    else if (b > 0x7e) s += `\\${b.toString(8).padStart(3, '0')}`;
    else s += String.fromCharCode(b);
  }
  return `(${s})`;
}

const n = (valor) => Number(valor.toFixed(2));

/** Parte un texto en líneas que entren en `ancho` puntos con letra `tam`. */
function partir(texto, tam, ancho = UTIL) {
  const maximo = Math.max(1, Math.floor(ancho / (tam * ANCHO_LETRA)));
  const lineas = [];
  let actual = '';
  for (const palabra of String(texto ?? '').split(/\s+/).filter(Boolean)) {
    let resto = palabra;
    // Una palabra más larga que la línea se corta.
    while (resto.length > maximo) {
      if (actual) {
        lineas.push(actual);
        actual = '';
      }
      lineas.push(resto.slice(0, maximo));
      resto = resto.slice(maximo);
    }
    if (!actual) actual = resto;
    else if (actual.length + 1 + resto.length <= maximo) actual += ` ${resto}`;
    else {
      lineas.push(actual);
      actual = resto;
    }
  }
  if (actual) lineas.push(actual);
  return lineas;
}

/**
 * Va apilando el ticket de arriba hacia abajo. Guarda cada trazo con su
 * distancia al borde superior; al final, con el alto total ya conocido, se
 * convierten a coordenadas de PDF (que se miden desde abajo).
 */
function crearLienzo() {
  const trazos = [];
  let y = MARGEN;
  const lienzo = {
    get y() {
      return y;
    },
    espacio(pt) {
      y += pt;
    },
    /** Una línea de texto: alineada a la izquierda, al centro o a la derecha. */
    texto(texto, { tam = TAM, negrita = false, alinear = 'izquierda' } = {}) {
      const bytes = aBytes(texto);
      const ancho = bytes.length * tam * ANCHO_LETRA;
      const x = alinear === 'centro' ? MARGEN + (UTIL - ancho) / 2 : alinear === 'derecha' ? ANCHO - MARGEN - ancho : MARGEN;
      trazos.push({ tipo: 'texto', x: Math.max(MARGEN, x), y: y + tam * 0.82, tam, negrita, bytes });
      y += tam * ALTO_LINEA;
    },
    /** Varias líneas si el texto no entra en una. */
    parrafo(texto, opciones = {}) {
      for (const linea of partir(texto, opciones.tam || TAM)) lienzo.texto(linea, opciones);
    },
    /** Etiqueta a la izquierda y valor a la derecha, en la misma línea. */
    fila(etiqueta, valor, opciones = {}) {
      const tam = opciones.tam || TAM;
      const maximo = Math.floor(UTIL / (tam * ANCHO_LETRA));
      const derecha = String(valor ?? '');
      // Si no entran los dos, la etiqueta va arriba y el valor debajo.
      if (String(etiqueta).length + 1 + derecha.length > maximo) {
        lienzo.parrafo(etiqueta, opciones);
        for (const linea of partir(derecha, tam)) lienzo.texto(linea, { ...opciones, alinear: 'derecha' });
        return;
      }
      const yFila = y;
      lienzo.texto(etiqueta, opciones);
      y = yFila;
      lienzo.texto(derecha, { ...opciones, alinear: 'derecha' });
    },
    linea({ doble = false } = {}) {
      y += 3;
      trazos.push({ tipo: 'linea', y, punteada: !doble });
      if (doble) trazos.push({ tipo: 'linea', y: y + 2, punteada: false });
      y += doble ? 6 : 4;
    },
    /** QR centrado, dibujado con cuadrados. `modulos` es la matriz de qrcode. */
    qr(modulos, lado) {
      trazos.push({ tipo: 'qr', modulos, lado, x: (ANCHO - lado) / 2, y: y + 2 });
      y += lado + 6;
    },
    terminar() {
      return { trazos, alto: y + MARGEN };
    },
  };
  return lienzo;
}

/** Trazos -> instrucciones de dibujo del PDF. */
function contenido(trazos, alto) {
  const ops = ['0 g', '0 G', '0.5 w'];
  for (const t of trazos) {
    if (t.tipo === 'texto') {
      ops.push(`BT /${t.negrita ? 'F2' : 'F1'} ${t.tam} Tf ${n(t.x)} ${n(alto - t.y)} Td ${literal(t.bytes)} Tj ET`);
    } else if (t.tipo === 'linea') {
      ops.push(`${t.punteada ? '[2 2] 0 d' : '[] 0 d'} ${MARGEN} ${n(alto - t.y)} m ${n(ANCHO - MARGEN)} ${n(alto - t.y)} l S`);
    } else if (t.tipo === 'qr') {
      const tamano = t.modulos.size;
      const celda = t.lado / tamano;
      for (let fila = 0; fila < tamano; fila++) {
        // Los cuadrados seguidos de una fila se dibujan como un solo rectángulo.
        let inicio = -1;
        for (let col = 0; col <= tamano; col++) {
          const lleno = col < tamano && t.modulos.get(fila, col);
          if (lleno && inicio < 0) inicio = col;
          if (!lleno && inicio >= 0) {
            ops.push(`${n(t.x + inicio * celda)} ${n(alto - t.y - (fila + 1) * celda)} ${n((col - inicio) * celda + 0.05)} ${n(celda + 0.05)} re`);
            inicio = -1;
          }
        }
      }
      ops.push('f');
    }
  }
  return ops.join('\n');
}

/** Arma el archivo PDF (una página) a partir de las instrucciones de dibujo. */
function armarPdf(dibujo, alto, titulo) {
  const objetos = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${n(ANCHO)} ${n(alto)}] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >>',
    `<< /Length ${dibujo.length} >>\nstream\n${dibujo}\nendstream`,
    `<< /Title ${literal(aBytes(titulo))} /Producer (Monspeet POS) >>`,
  ];
  let pdf = '%PDF-1.4\n';
  const posiciones = [];
  objetos.forEach((objeto, i) => {
    posiciones.push(pdf.length);
    pdf += `${i + 1} 0 obj\n${objeto}\nendobj\n`;
  });
  const inicioTabla = pdf.length;
  pdf += `xref\n0 ${objetos.length + 1}\n0000000000 65535 f \n`;
  for (const posicion of posiciones) pdf += `${String(posicion).padStart(10, '0')} 00000 n \n`;
  pdf += `trailer\n<< /Size ${objetos.length + 1} /Root 1 0 R /Info 7 0 R >>\nstartxref\n${inicioTabla}\n%%EOF\n`;
  // Todo el archivo es ASCII: un carácter = un byte.
  const bytes = new Uint8Array(pdf.length);
  for (let i = 0; i < pdf.length; i++) bytes[i] = pdf.charCodeAt(i);
  return bytes;
}

const soles = (monto) => `S/.${Number(monto).toFixed(2)}`;

/**
 * PDF del ticket de una venta. Recibe lo mismo que <Recibo>, más `fecha`
 * (texto ya listo para mostrar).
 * Devuelve { bytes, nombre } con el archivo y un nombre sugerido.
 */
export function pdfDeVenta({ venta, items, comprobante, cliente, nombreTienda, direccion, telefono, ruc, cajero, fecha }) {
  const esComprobante = !!comprobante;
  const numeroDocumento = comprobante ? `${comprobante.serie}-${String(comprobante.numero).padStart(6, '0')}` : null;
  const encabezado = comprobante
    ? comprobante.tipo === 'FACTURA'
      ? 'FACTURA ELECTRÓNICA'
      : 'BOLETA DE VENTA ELECTRÓNICA'
    : 'NOTA DE VENTA (sin comprobante tributario)';
  const total = comprobante?.total_venta ?? venta.total;
  const desglose = desgloseDeComprobante(comprobante, total);
  const detraccion = comprobante?.detraccion_monto > 0 ? comprobante : null;
  // Un comprobante que SUNAT no aceptó (rechazado o pendiente) se exporta
  // diciéndolo claro y sin QR: no debe pasar por uno válido.
  const noAceptado = esComprobante && comprobante.estado && comprobante.estado !== 'ACEPTADO' ? comprobante.estado : null;
  const centro = { alinear: 'centro' };

  const l = crearLienzo();
  for (const linea of partir(String(nombreTienda || '').toUpperCase(), 12)) l.texto(linea, { tam: 12, negrita: true, ...centro });
  if (direccion) for (const linea of partir(direccion, TAM)) l.texto(linea, centro);
  if (telefono) l.texto(`Tel: ${telefono}`, centro);
  if (ruc) l.texto(`RUC ${ruc}`, { negrita: true, ...centro });

  l.linea();
  for (const linea of partir(encabezado, 9.5)) l.texto(linea, { tam: 9.5, negrita: true, ...centro });
  if (numeroDocumento) l.texto(numeroDocumento, { tam: 11, negrita: true, ...centro });
  if (noAceptado) {
    for (const linea of partir(`${noAceptado} · SIN VALIDEZ TRIBUTARIA`, TAM)) l.texto(linea, { negrita: true, ...centro });
  }
  l.linea();

  if (cliente?.nombre_razon_social) {
    l.texto('ADQUIRIENTE', { negrita: true });
    if (cliente.numero_documento) l.fila('Doc.', cliente.numero_documento);
    l.parrafo(cliente.nombre_razon_social);
    if (cliente.direccion) l.parrafo(cliente.direccion);
    l.linea();
  }

  l.fila('Venta', venta.folio);
  if (fecha) l.fila('Fecha', fecha);
  l.fila('Moneda', 'SOLES');
  if (cajero) l.fila('Cajero', cajero);
  l.linea();

  for (const item of items) {
    l.parrafo(item.nombre, { negrita: true });
    l.fila(`${formatoCantidad(item.cantidad)} x ${soles(item.precio)}`, soles(subtotalLinea(item.precio, item.cantidad)));
    l.espacio(2);
  }
  l.linea();
  const unidades = formatoCantidad(items.reduce((suma, item) => suma + item.cantidad, 0));
  l.fila('Ítems', `${items.length} producto${items.length === 1 ? '' : 's'} · ${unidades} unid.`);

  if (esComprobante) {
    l.linea();
    l.fila('Op. Gravada', soles(desglose.gravadas));
    if (desglose.exoneradas > 0) l.fila('Op. Exonerada', soles(desglose.exoneradas));
    if (desglose.inafectas > 0) l.fila('Op. Inafecta', soles(desglose.inafectas));
    l.fila(`IGV (${etiquetaTasa(desglose.tasa)}%)`, soles(desglose.igv));
  }

  l.linea({ doble: true });
  l.fila('TOTAL', soles(total), { tam: 12, negrita: true });
  if (esComprobante) {
    l.espacio(2);
    l.parrafo(`SON: ${montoEnLetras(total)}`);
  }

  if (detraccion) {
    l.linea();
    l.parrafo('OPERACIÓN SUJETA A DETRACCIÓN', { negrita: true });
    l.fila(`Detracción (${formatoCantidad(detraccion.detraccion_porcentaje)}%)`, soles(detraccion.detraccion_monto));
    if (detraccion.detraccion_cuenta) l.fila('Cta. Bco. de la Nación', detraccion.detraccion_cuenta);
    l.fila('Neto a pagar', soles(total - detraccion.detraccion_monto));
  }

  l.linea({ doble: true });

  // QR oficial de SUNAT: solo con los datos reales completos, igual que el ticket.
  if (esComprobante && !noAceptado && comprobante.ruc_emisor && comprobante.hash && comprobante.fecha_emision) {
    const cadena = construirCadenaQrSunat({
      ruc: comprobante.ruc_emisor,
      tipoDocumento: comprobante.tipo === 'FACTURA' ? '01' : '03',
      serie: comprobante.serie,
      numero: comprobante.numero,
      igv: desglose.igv,
      total,
      fechaEmision: comprobante.fecha_emision,
      tipoDocCliente: comprobante.cliente_tipo_documento_codigo || '0',
      numDocCliente: comprobante.cliente_numero_documento || '-',
      hash: comprobante.hash,
    });
    l.qr(QRCode.create(cadena, { errorCorrectionLevel: 'M' }).modules, 96);
  }
  if (esComprobante && !noAceptado) {
    l.espacio(2);
    const aviso = 'Consulte este comprobante en el portal de SUNAT escaneando el código QR, o revise el documento oficial disponible en el sistema.';
    for (const linea of partir(aviso, 7)) {
      l.texto(linea, { tam: 7, ...centro });
    }
  }
  l.espacio(4);
  l.texto('¡Gracias por su compra!', { negrita: true, ...centro });

  const { trazos, alto } = l.terminar();
  const tipo = comprobante ? (comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta') : 'Nota de venta';
  const titulo = `${tipo} ${numeroDocumento || venta.folio}`;
  return {
    bytes: armarPdf(contenido(trazos, alto), alto, titulo),
    nombre: `${titulo.replace(/\s+/g, '-')}.pdf`,
  };
}

/** Descarga un archivo (bytes o Blob) con ese nombre. */
export function descargarArchivo(contenidoArchivo, nombre, tipo = 'application/pdf') {
  const blob = contenidoArchivo instanceof Blob ? contenidoArchivo : new Blob([contenidoArchivo], { type: tipo });
  const url = URL.createObjectURL(blob);
  const enlace = document.createElement('a');
  enlace.href = url;
  enlace.download = nombre;
  document.body.appendChild(enlace);
  enlace.click();
  enlace.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}
