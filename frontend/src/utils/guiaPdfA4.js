// Guía de remisión en hoja A4 como ARCHIVO PDF (botón "PDF A4"), para
// guardarla o enviarla sin pasar por el diálogo de impresión. Es el mismo
// diseño que la hoja que se imprime (utils/guiaA4.js, en HTML): logo,
// recuadro del RUC, tarjetas y tabla con el color de acento del negocio.
// Si cambia uno, conviene cambiar el otro.
//
// Se dibuja con jsPDF (texto real, se puede seleccionar y pesa poco). La
// librería se carga solo al tocar el botón.

import QRCode from 'qrcode';
import { aclarar, acentoImprimible, iniciales } from './facturaA4';
import { fechaCorta } from './formato';
import { formatoCantidad } from './medidas';
import { etiquetaUnidad } from './unidades';
import { NOMBRE_DOCUMENTO, lugarDeGuia, motivoDeGuia, numeroConCeros } from './guiaTexto';

// Medidas en mm (A4: 210 × 297), las mismas que el A4 en HTML.
const PAGINA_ALTO = 297;
const IZQ = 13;
const ANCHO = 184;
const ARRIBA = 12;
const ABAJO = 14;
const PT = 0.3528; // 1 punto en mm

const TINTA = '#1c2420';
const TENUE = '#5d6762';
const REGLA = '#dfe4e1';

const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

/**
 * Arma el PDF y devuelve { bytes, nombre }.
 * emisor = { nombreComercial, razonSocial, ruc, direccion, telefono, email, logo (URL), color }
 */
export async function pdfGuiaA4({ guia, motivos, emisor = {} }) {
  const datos = guia?.datos;
  if (!datos) throw new Error('Esta guía no tiene sus datos guardados para armar el PDF.');
  if (guia.estado !== 'ACEPTADA') throw new Error('Solo una guía aceptada por SUNAT tiene representación impresa.');
  const [{ jsPDF }, ubigeos, logo, qr] = await Promise.all([
    import('jspdf'),
    import('../data/ubigeos').then((m) => m.UBIGEOS).catch(() => null),
    emisor.logo ? imagenPng(emisor.logo).catch(() => null) : null,
    qrDeGuia(guia.qr),
  ]);
  return armarPdfGuiaA4({ jsPDF, guia, motivos, emisor, ubigeos, logo, qr });
}

/** El QR de SUNAT, si llegó: un enlace (se dibuja) o la imagen ya hecha. */
async function qrDeGuia(qr) {
  if (!qr) return null;
  if (qr.startsWith('data:image/')) return imagenPng(qr).catch(() => null);
  const dataUrl = await QRCode.toDataURL(qr, { margin: 0, scale: 8 }).catch(() => null);
  return dataUrl ? { dataUrl, ancho: 1, alto: 1 } : null;
}

/**
 * Cualquier imagen (URL o data URL, PNG/JPG/WebP) -> { dataUrl PNG, ancho, alto }.
 * Se baja con fetch y se pasa por un canvas: así jsPDF siempre recibe PNG.
 */
async function imagenPng(url) {
  const fuente = url.startsWith('data:') ? url : URL.createObjectURL(await (await fetch(url)).blob());
  try {
    const img = await new Promise((ok, mal) => {
      const i = new Image();
      i.onload = () => ok(i);
      i.onerror = mal;
      i.src = fuente;
    });
    const lienzo = document.createElement('canvas');
    lienzo.width = img.naturalWidth;
    lienzo.height = img.naturalHeight;
    lienzo.getContext('2d').drawImage(img, 0, 0);
    return { dataUrl: lienzo.toDataURL('image/png'), ancho: img.naturalWidth, alto: img.naturalHeight };
  } finally {
    if (fuente !== url) URL.revokeObjectURL(fuente);
  }
}

/** El dibujo en sí (sin cargar nada): también lo usan las pruebas. */
export function armarPdfGuiaA4({ jsPDF, guia, motivos, emisor = {}, ubigeos, logo, qr }) {
  const datos = guia.datos;
  const numero = numeroConCeros(guia.numero);
  const acento = acentoImprimible(emisor.color);
  const suave = aclarar(acento, 0.08);
  const nombreComercial = emisor.nombreComercial || emisor.razonSocial || '';
  const mostrarRazon = emisor.razonSocial && emisor.razonSocial.trim() !== nombreComercial.trim();
  const motivo = motivoDeGuia(datos, motivos);
  const privado = datos.modo !== 'PUBLICO';
  const items = datos.items || [];

  const doc = new jsPDF({ unit: 'mm', format: 'a4', compress: true });
  doc.setProperties({ title: `Guía de remisión ${numero}`, creator: 'Monspeet POS' });

  // --- Ayudas de dibujo ---
  const letra = (tam, negrita = false, color = TINTA) => {
    doc.setFont('helvetica', negrita ? 'bold' : 'normal');
    doc.setFontSize(tam);
    doc.setTextColor(...rgb(color));
  };
  const relleno = (color) => doc.setFillColor(...rgb(color));
  const trazo = (color, grosor) => {
    doc.setDrawColor(...rgb(color));
    doc.setLineWidth(grosor);
  };
  const altoLinea = (tam, interlineado = 1.42) => tam * PT * interlineado;
  const lineas = (texto, ancho) => doc.splitTextToSize(String(texto ?? ''), ancho);
  // Texto de varias líneas desde `y` (borde superior); devuelve el alto usado.
  const parrafo = (texto, x, y, ancho, tam, opciones = {}) => {
    const { negrita = false, color = TINTA, alinear = 'left', interlineado = 1.42, dibujar = true } = opciones;
    letra(tam, negrita, color);
    const partes = lineas(texto, ancho);
    const paso = altoLinea(tam, interlineado);
    if (dibujar) {
      const xTexto = alinear === 'right' ? x + ancho : alinear === 'center' ? x + ancho / 2 : x;
      partes.forEach((linea, i) => doc.text(linea, xTexto, y + i * paso + tam * PT * 0.95, { align: alinear }));
    }
    return partes.length * paso;
  };
  const altoTarjetaPad = 3.5;
  const anchoTarjetaPad = 4.5;

  // --- Cabecera: negocio a la izquierda, recuadro del RUC a la derecha ---
  const anchoRecuadro = 64;
  const xRecuadro = IZQ + ANCHO - anchoRecuadro;
  const ladoMarca = logo ? 26 : 22;
  const xTextoNegocio = IZQ + ladoMarca + 5;
  const anchoTextoNegocio = xRecuadro - 8 - xTextoNegocio;

  const bloqueNegocio = (y, dibujar) => {
    let alto = parrafo(nombreComercial, xTextoNegocio, y, anchoTextoNegocio, 19, { negrita: true, interlineado: 1.08, dibujar });
    alto += 1.2;
    if (mostrarRazon) {
      alto += parrafo(emisor.razonSocial.toUpperCase(), xTextoNegocio, y + alto, anchoTextoNegocio, 8.6, { negrita: true, dibujar });
      alto += 0.6;
    }
    if (emisor.direccion) alto += parrafo(emisor.direccion, xTextoNegocio, y + alto, anchoTextoNegocio, 8.6, { color: TENUE, dibujar });
    const contacto = [emisor.telefono ? `Tel. ${emisor.telefono}` : '', emisor.email || ''].filter(Boolean).join('   ·   ');
    if (contacto) alto += parrafo(contacto, xTextoNegocio, y + alto, anchoTextoNegocio, 8.6, { color: TENUE, dibujar });
    return alto;
  };

  const tipoDoc = ['GUÍA DE REMISIÓN ELECTRÓNICA', 'REMITENTE'];
  const altoRuc = 9;
  const altoTipo = 4 + tipoDoc.length * altoLinea(8.4, 1.3);
  const altoNumero = 11;
  const altoRecuadro = altoRuc + altoTipo + altoNumero;
  const altoNegocio = Math.max(bloqueNegocio(0, false), ladoMarca);
  const altoCabecera = Math.max(altoNegocio, altoRecuadro);

  // Negocio, centrado verticalmente con el recuadro.
  const yNegocio = ARRIBA + (altoCabecera - bloqueNegocio(0, false)) / 2;
  const yMarca = ARRIBA + (altoCabecera - ladoMarca) / 2;
  if (logo) {
    const escala = Math.min(ladoMarca / logo.ancho, ladoMarca / logo.alto);
    const w = logo.ancho * escala;
    const h = logo.alto * escala;
    doc.addImage(logo.dataUrl, 'PNG', IZQ + (ladoMarca - w) / 2, yMarca + (ladoMarca - h) / 2, w, h);
  } else {
    relleno(acento);
    doc.circle(IZQ + ladoMarca / 2, yMarca + ladoMarca / 2, ladoMarca / 2, 'F');
    letra(17, true, '#ffffff');
    doc.text(iniciales(nombreComercial), IZQ + ladoMarca / 2, yMarca + ladoMarca / 2 + 17 * PT * 0.35, { align: 'center' });
  }
  bloqueNegocio(yNegocio, true);

  // Recuadro del RUC.
  const yRecuadro = ARRIBA + (altoCabecera - altoRecuadro) / 2;
  const centroRecuadro = xRecuadro + anchoRecuadro / 2;
  doc.saveGraphicsState();
  relleno(acento);
  doc.rect(xRecuadro, yRecuadro + altoRuc, anchoRecuadro, altoTipo, 'F');
  doc.restoreGraphicsState();
  trazo(acento, 0.45);
  doc.roundedRect(xRecuadro, yRecuadro, anchoRecuadro, altoRecuadro, 3, 3, 'S');
  letra(10.5, true);
  doc.text(`R.U.C. ${emisor.ruc || ''}`, centroRecuadro, yRecuadro + 6, { align: 'center' });
  letra(8.4, true, '#ffffff');
  tipoDoc.forEach((linea, i) =>
    doc.text(linea, centroRecuadro, yRecuadro + altoRuc + 2 + 8.4 * PT * 0.95 + i * altoLinea(8.4, 1.3), { align: 'center' })
  );
  letra(15.5, true, acento);
  doc.text(numero, centroRecuadro, yRecuadro + altoRuc + altoTipo + 7.6, { align: 'center' });

  // Franja de color.
  let y = ARRIBA + altoCabecera + 6;
  relleno(acento);
  doc.roundedRect(IZQ, y, ANCHO, 1.2, 0.6, 0.6, 'F');
  y += 1.2 + 5;

  // --- Tarjetas (2 columnas) ---
  const anchoTarjeta = (ANCHO - 5) / 2;
  const interior = anchoTarjeta - anchoTarjetaPad * 2;

  const etiqueta = (texto, x, yy, dibujar) => {
    if (dibujar) {
      letra(7.6, true, acento);
      doc.setCharSpace(0.25);
      doc.text(texto.toUpperCase(), x, yy + 7.6 * PT * 0.95);
      doc.setCharSpace(0);
    }
    return altoLinea(7.6, 1.3) + 1.2;
  };
  // Pares etiqueta / valor (como el <dl> del A4): valor en negrita a la derecha.
  const pares = (filas, x, yy, dibujar) => {
    let alto = 0;
    for (const [nombre, valor] of filas) {
      if (!valor) continue;
      letra(9.3, false, TENUE);
      const anchoNombre = doc.getTextWidth(nombre) + 4;
      const altoValor = parrafo(valor, x + anchoNombre, yy + alto, interior - anchoNombre, 9.3, { negrita: true, alinear: 'right', dibujar });
      if (dibujar) {
        letra(9.3, false, TENUE);
        doc.text(nombre, x, yy + alto + 9.3 * PT * 0.95);
      }
      alto += Math.max(altoValor, altoLinea(9.3)) + 0.4;
    }
    return alto;
  };

  const tarjetaDestinatario = (x, yy, dibujar) => {
    let alto = etiqueta('Destinatario', x, yy, dibujar);
    alto += parrafo(datos.destinatario_nombre, x, yy + alto, interior, 11, { negrita: true, dibujar });
    alto += 0.6;
    const tipo = NOMBRE_DOCUMENTO[datos.destinatario_tipo] || 'Doc.';
    if (dibujar) {
      letra(9.3);
      doc.text(`${tipo} `, x, yy + alto + 9.3 * PT * 0.95);
      const ancho = doc.getTextWidth(`${tipo} `);
      letra(9.3, true);
      doc.text(String(datos.destinatario_documento || ''), x + ancho, yy + alto + 9.3 * PT * 0.95);
    }
    return alto + altoLinea(9.3);
  };
  const tarjetaFechas = (x, yy, dibujar) =>
    pares(
      [
        ['Fecha de emisión', fechaCorta(guia.fecha)],
        ['Inicio de traslado', fechaCorta(datos.fecha_traslado)],
        ['Motivo', motivo],
        guia.comprobante ? ['Comprobante', numeroConCeros(guia.comprobante)] : ['Venta', guia.folio_venta],
      ],
      x,
      yy,
      dibujar
    );
  const tarjetaPuntos = (x, yy, dibujar) => {
    let alto = etiqueta('Punto de partida', x, yy, dibujar);
    alto += parrafo(lugarDeGuia(datos.partida, ubigeos), x, yy + alto, interior, 9.3, { dibujar });
    alto += 3;
    alto += etiqueta('Punto de llegada', x, yy + alto, dibujar);
    alto += parrafo(lugarDeGuia(datos.llegada, ubigeos), x, yy + alto, interior, 9.3, { dibujar });
    return alto;
  };
  const tarjetaTransporte = (x, yy, dibujar) => {
    const chofer = datos.chofer;
    const transportista = datos.transportista;
    const filas = privado
      ? [
          ['Modalidad', 'Transporte privado'],
          ['Placa', datos.placa],
          ['Conductor', chofer ? `${chofer.nombres || ''} ${chofer.apellidos || ''}`.trim() : ''],
          ['DNI', chofer?.documento],
          ['Licencia', chofer?.licencia],
        ]
      : [
          ['Modalidad', 'Transporte público'],
          ['Transportista', transportista?.nombre],
          ['RUC', transportista?.ruc],
          ['Registro MTC', transportista?.mtc],
        ];
    filas.push(['Peso bruto total', `${formatoCantidad(datos.peso_total)} kg`]);
    filas.push(['Bultos', datos.bultos != null ? String(datos.bultos) : '']);
    const alto = etiqueta('Transporte', x, yy, dibujar);
    return alto + pares(filas, x, yy + alto, dibujar);
  };

  const filaTarjetas = (izquierda, derecha) => {
    const altoIzq = izquierda(0, 0, false);
    const altoDer = derecha(0, 0, false);
    const alto = Math.max(altoIzq, altoDer) + altoTarjetaPad * 2;
    relleno(suave);
    doc.roundedRect(IZQ, y, anchoTarjeta, alto, 2.5, 2.5, 'F');
    doc.roundedRect(IZQ + anchoTarjeta + 5, y, anchoTarjeta, alto, 2.5, 2.5, 'F');
    izquierda(IZQ + anchoTarjetaPad, y + altoTarjetaPad, true);
    derecha(IZQ + anchoTarjeta + 5 + anchoTarjetaPad, y + altoTarjetaPad, true);
    y += alto + 5;
  };
  filaTarjetas(tarjetaDestinatario, tarjetaFechas);
  filaTarjetas(tarjetaPuntos, tarjetaTransporte);

  // --- Tabla de bienes (sin precios) ---
  const col = { n: 9, cant: 22, unidad: 26 };
  const xDesc = IZQ + col.n;
  const anchoDesc = ANCHO - col.n - col.cant - col.unidad;
  const xCantDer = xDesc + anchoDesc + col.cant - 2.5;
  const xUnidad = xDesc + anchoDesc + col.cant + 2.5;
  const limite = PAGINA_ALTO - ABAJO;

  const cabeceraTabla = () => {
    relleno(acento);
    doc.roundedRect(IZQ, y, ANCHO, 7.4, 2, 2, 'F');
    letra(7.8, true, '#ffffff');
    doc.setCharSpace(0.2);
    const yTexto = y + 4.8;
    doc.text('#', IZQ + 2.5, yTexto);
    doc.text('BIENES A TRASLADAR', xDesc + 2.5, yTexto);
    doc.text('CANT.', xCantDer, yTexto, { align: 'right' });
    doc.text('UNIDAD', xUnidad, yTexto);
    doc.setCharSpace(0);
    y += 7.4;
  };
  cabeceraTabla();

  items.forEach((it, i) => {
    const altoNombre = parrafo(it.descripcion, 0, 0, anchoDesc - 5, 9.3, { dibujar: false });
    const altoCodigo = it.codigo ? altoLinea(7.6) + 0.4 : 0;
    const alto = Math.max(altoNombre + altoCodigo, altoLinea(9.3)) + 5.2;
    if (y + alto > limite) {
      doc.addPage();
      y = ARRIBA;
      cabeceraTabla();
    }
    if (i % 2 === 1) {
      relleno('#fafbfa');
      doc.rect(IZQ, y, ANCHO, alto, 'F');
    }
    const yTexto = y + 2.6;
    letra(9.3, false, TENUE);
    doc.text(String(i + 1), IZQ + 2.5, yTexto + 9.3 * PT * 0.95);
    parrafo(it.descripcion, xDesc + 2.5, yTexto, anchoDesc - 5, 9.3);
    if (it.codigo) parrafo(`Cód. ${it.codigo}`, xDesc + 2.5, yTexto + altoNombre + 0.4, anchoDesc - 5, 7.6, { color: TENUE });
    letra(9.3, true);
    doc.text(formatoCantidad(it.cantidad), xCantDer, yTexto + 9.3 * PT * 0.95, { align: 'right' });
    letra(9.3, false, TENUE);
    doc.text(etiquetaUnidad(it.unidad) || 'Unidad', xUnidad, yTexto + 9.3 * PT * 0.95);
    trazo(REGLA, 0.25);
    doc.line(IZQ, y + alto, IZQ + ANCHO, y + alto);
    y += alto;
  });

  y += 3;
  letra(8.8, true);
  doc.text(String(items.length), IZQ + ANCHO, y + 8.8 * PT * 0.95, { align: 'right' });
  const anchoCuenta = doc.getTextWidth(String(items.length));
  letra(8.8, false, TENUE);
  doc.text('Ítems: ', IZQ + ANCHO - anchoCuenta, y + 8.8 * PT * 0.95, { align: 'right' });
  y += altoLinea(8.8);

  // --- Observaciones ---
  if (datos.observaciones) {
    const altoObs = altoLinea(9.3) + 1.5 + parrafo(datos.observaciones, IZQ, 0, ANCHO, 9.3, { dibujar: false });
    y += 4;
    if (y + altoObs > limite) {
      doc.addPage();
      y = ARRIBA;
    }
    parrafo('Observaciones', IZQ, y, ANCHO, 9.3, { negrita: true });
    y += altoLinea(9.3) + 1.5;
    y += parrafo(datos.observaciones, IZQ, y, ANCHO, 9.3);
  }

  // --- Pie legal (con el QR de SUNAT, si llegó) ---
  const consulta = qr
    ? 'Consulte este documento en SUNAT escaneando el código QR.'
    : 'Consulte este documento en SUNAT con el RUC del remitente y el número de la guía.';
  const ladoQr = 25;
  const xLegalTexto = IZQ + 4 + (qr ? ladoQr + 5 : 0);
  const anchoLegalTexto = IZQ + ANCHO - 4 - xLegalTexto;
  const textoLegal = (yy, dibujar) => {
    let alto = parrafo('Representación impresa de la guía de remisión electrónica remitente.', xLegalTexto, yy, anchoLegalTexto, 8.4, {
      negrita: true,
      dibujar,
    });
    alto += 1.2;
    alto += parrafo(consulta, xLegalTexto, yy + alto, anchoLegalTexto, 8.4, { color: TENUE, dibujar });
    return alto;
  };
  const altoLegal = Math.max(textoLegal(0, false), qr ? ladoQr : 0) + 8;
  y += 9;
  if (y + altoLegal + 8 > limite) {
    doc.addPage();
    y = ARRIBA;
  }
  trazo(REGLA, 0.25);
  doc.roundedRect(IZQ, y, ANCHO, altoLegal, 2.5, 2.5, 'S');
  if (qr) doc.addImage(qr.dataUrl, 'PNG', IZQ + 4, y + (altoLegal - ladoQr) / 2, ladoQr, ladoQr);
  textoLegal(y + (altoLegal - textoLegal(0, false)) / 2, true);
  y += altoLegal + 3;
  letra(7.6, false, '#9aa39e');
  doc.setCharSpace(0.15);
  doc.text('Emitido con Monspeet POS', IZQ + ANCHO / 2, y + 7.6 * PT * 0.95, { align: 'center' });
  doc.setCharSpace(0);

  return {
    bytes: doc.output('arraybuffer'),
    nombre: `Guia-${numero}-A4.pdf`,
  };
}
