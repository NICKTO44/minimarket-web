// Comprobante electrónico en hoja A4 (factura, boleta o nota de crédito),
// con diseño propio. Al final, el mismo comprobante en ticket de 80 mm.
//
// Con FacturaLibre el A4 era su PDF; con la emisión directa a SUNAT lo arma
// el sistema. Se genera como una página HTML independiente (sus propios
// estilos y su propio @page A4) y se imprime desde un iframe oculto, así no
// choca con el @page de 80 mm del ticket. Desde el diálogo de impresión
// también se puede "Guardar como PDF".
//
// Diseño: logo del negocio (o un círculo con su inicial si no subió uno),
// recuadro del RUC, cabecera de la tabla y caja del total con el color de
// acento del negocio. El texto es oscuro, así que también se lee bien en una
// impresora en blanco y negro.

import QRCode from 'qrcode';
import { montoEnLetras } from './numeroALetras';
import { construirCadenaQrSunat } from './qrSunat';

const COLOR_POR_DEFECTO = '#0f7b37'; // verde Monspeet, versión oscura (imprime bien)

function escapar(texto) {
  return String(texto ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

const soles = (n) =>
  Number(n || 0).toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

const cantidad = (n) =>
  Number(n || 0).toLocaleString('es-PE', { minimumFractionDigits: 0, maximumFractionDigits: 3 });

/** Un color de acento legible sobre blanco: si es muy claro, se oscurece. */
function acentoImprimible(hex) {
  // '#4338ca' es el índigo que la migración 0003 dejó por defecto: nadie lo
  // eligió (igual que en utils/tema.js), así que se usa el verde Monspeet.
  const elegido = /^#[0-9a-f]{6}$/i.test(hex || '') && hex.toLowerCase() !== '#4338ca';
  const limpio = elegido ? hex : COLOR_POR_DEFECTO;
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(limpio.slice(i, i + 2), 16));
  const luz = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  if (luz <= 0.55) return limpio;
  const f = 0.55 / luz;
  return '#' + [r, g, b].map((v) => Math.round(v * f).toString(16).padStart(2, '0')).join('');
}

/** El acento mezclado con blanco (f = cuánto acento queda, de 0 a 1): fondos suaves. */
function aclarar(hex, f) {
  return (
    '#' +
    [1, 3, 5]
      .map((i) => Math.round(255 - (255 - parseInt(hex.slice(i, i + 2), 16)) * f).toString(16).padStart(2, '0'))
      .join('')
  );
}

/**
 * Iniciales para el círculo que reemplaza al logo cuando el negocio no subió
 * uno: "VERANE" -> "V", "Mi Minimarket" -> "MM".
 */
function iniciales(nombre) {
  const palabras = String(nombre || '')
    .replace(/[^\p{L}\p{N} ]/gu, ' ')
    .split(/\s+/)
    .filter((p) => p.length > 1 || /\d/.test(p));
  if (palabras.length === 0) return '?';
  if (palabras.length === 1) return palabras[0][0].toUpperCase();
  return (palabras[0][0] + palabras[1][0]).toUpperCase();
}

/**
 * datos = {
 *   emisor: { nombreComercial, razonSocial, ruc, direccion, telefono, email, logo, color },
 *   comprobante: { tipo: 'FACTURA'|'BOLETA'|'NOTA_CREDITO', serie, numero, fechaEmision, horaEmision, hash },
 *   referencia: { documento: 'BM01-15', tipo: 'Boleta'|'Factura', motivo } | null (solo notas),
 *   cliente: { tipoDocumento: 'RUC'|'DNI'|..., numeroDocumento, nombre, direccion } | null,
 *   items: [{ cantidad, unidad, codigo, descripcion, precioUnitario, importe }],
 *   totales: { gravadas, exoneradas, inafectas, igv, tasa, total },
 *   formaPago: 'Contado' | 'Crédito',
 *   cuotas: [{ fecha, monto }],
 *   detraccion: { porcentaje, monto, cuenta } | null,
 *   observacion,
 *   qr: dataURL de la imagen del QR (opcional),
 * }
 */
export function htmlComprobanteA4(datos) {
  const { emisor = {}, comprobante = {}, cliente, items = [], totales = {}, cuotas = [], detraccion, qr } = datos;
  const esFactura = comprobante.tipo === 'FACTURA';
  const nombreTipo = nombreDelTipo(comprobante.tipo);
  const referencia = datos.referencia;
  const numero = `${comprobante.serie}-${String(comprobante.numero).padStart(8, '0')}`;
  const acento = acentoImprimible(emisor.color);
  const suave = aclarar(acento, 0.08);
  const medio = aclarar(acento, 0.18);
  const credito = datos.formaPago === 'Crédito' && cuotas.length > 0;
  const nombreComercial = emisor.nombreComercial || emisor.razonSocial || '';
  const mostrarRazon = emisor.razonSocial && emisor.razonSocial.trim() !== nombreComercial.trim();
  const contacto = [emisor.telefono ? `Tel. ${escapar(emisor.telefono)}` : '', emisor.email ? escapar(emisor.email) : '']
    .filter(Boolean)
    .join('<span class="punto">·</span>');

  const marca = emisor.logo
    ? `<img class="logo" src="${escapar(emisor.logo)}" alt="">`
    : `<div class="monograma">${escapar(iniciales(nombreComercial))}</div>`;

  const filasTotales = [
    ['Op. gravada', totales.gravadas, true],
    ['Op. exonerada', totales.exoneradas, totales.exoneradas > 0],
    ['Op. inafecta', totales.inafectas, totales.inafectas > 0],
    [`IGV ${String(totales.tasa ?? 18).replace('.', ',')} %`, totales.igv, true],
  ]
    .filter(([, , ver]) => ver)
    .map(([etiqueta, monto]) => `<tr><th>${etiqueta}</th><td>S/ ${soles(monto)}</td></tr>`)
    .join('');

  const filasItems = items
    .map(
      (it, i) => `<tr>
        <td class="n">${i + 1}</td>
        <td class="desc">${escapar(it.descripcion)}${it.codigo ? `<span class="codigo">Cód. ${escapar(it.codigo)}</span>` : ''}</td>
        <td class="num">${cantidad(it.cantidad)}</td>
        <td class="unidad">${escapar(it.unidad || 'Unidad')}</td>
        <td class="num">${soles(it.precioUnitario)}</td>
        <td class="num fuerte">${soles(it.importe)}</td>
      </tr>`
    )
    .join('');

  const bloqueCliente = `
    <div class="tarjeta">
      <div class="etiqueta">${referencia ? 'Cliente' : esFactura ? 'Señor(es)' : 'Cliente'}</div>
      <div class="dato-grande">${escapar(cliente?.nombre || 'Clientes varios')}</div>
      ${cliente?.numeroDocumento ? `<div>${escapar(cliente.tipoDocumento || 'Doc.')} <strong>${escapar(cliente.numeroDocumento)}</strong></div>` : ''}
      ${cliente?.direccion ? `<div class="tenue">${escapar(cliente.direccion)}</div>` : ''}
      ${
        referencia
          ? `<div class="etiqueta separado">Documento que modifica</div>
             <div class="dato-grande">${escapar(referencia.tipo)} ${escapar(referencia.documento)}</div>
             <div>Motivo: ${escapar(referencia.motivo)}</div>`
          : ''
      }
    </div>`;

  const bloqueFechas = `
    <div class="tarjeta">
      <dl class="fechas">
        <dt>Fecha de emisión</dt><dd>${escapar(comprobante.fechaEmision)}</dd>
        ${comprobante.horaEmision ? `<dt>Hora</dt><dd>${escapar(comprobante.horaEmision)}</dd>` : ''}
        ${credito ? `<dt>Vencimiento</dt><dd>${escapar(cuotas[cuotas.length - 1].fecha)}</dd>` : ''}
        <dt>Moneda</dt><dd>Soles (PEN)</dd>
        ${esFactura ? `<dt>Forma de pago</dt><dd>${escapar(datos.formaPago || 'Contado')}</dd>` : ''}
      </dl>
    </div>`;

  const bloqueCuotas = credito
    ? `<div class="aparte"><h3>Pago al crédito</h3>
         <table class="mini"><tr><th>Cuota</th><th>Vence</th><th class="num">Monto</th></tr>
         ${cuotas.map((c, i) => `<tr><td>${i + 1}</td><td>${escapar(c.fecha)}</td><td class="num">S/ ${soles(c.monto)}</td></tr>`).join('')}
         </table>
         <p class="tenue">Monto neto pendiente de pago: S/ ${soles(cuotas.reduce((s, c) => s + Number(c.monto || 0), 0))}</p></div>`
    : '';
  const bloqueDetraccion =
    detraccion && detraccion.monto > 0
      ? `<div class="aparte"><h3>Operación sujeta a detracción</h3>
           <p>Detracción (${String(detraccion.porcentaje).replace('.', ',')} %): S/ ${soles(detraccion.monto)}<br>
           ${detraccion.cuenta ? `Cta. Banco de la Nación: ${escapar(detraccion.cuenta)}<br>` : ''}
           Neto a pagar: S/ ${soles(totales.total - detraccion.monto)}</p></div>`
      : '';

  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>${escapar(nombreTipo)} ${escapar(numero)}</title>
<style>
  @page { size: A4; margin: 12mm 13mm 14mm; }
  * { box-sizing: border-box; }
  :root { --acento: ${acento}; --suave: ${suave}; --medio: ${medio}; --tinta: #1c2420; --tenue: #5d6762; --regla: #dfe4e1; }
  html, body { margin: 0; padding: 0; background: #fff; }
  body {
    font-family: "Helvetica Neue", Helvetica, Arial, "Liberation Sans", sans-serif;
    font-size: 9.3pt; line-height: 1.42; color: var(--tinta);
    font-variant-numeric: tabular-nums;
    -webkit-print-color-adjust: exact; print-color-adjust: exact;
  }
  .hoja { width: 184mm; margin: 0 auto; }
  .tenue { color: var(--tenue); }
  .punto { margin: 0 1.6mm; color: var(--regla); }

  .cabecera { display: grid; grid-template-columns: 1fr 64mm; gap: 8mm; align-items: center; }
  .negocio { display: flex; gap: 5mm; align-items: center; min-width: 0; }
  .logo { width: 26mm; height: 26mm; object-fit: contain; flex: none; }
  .monograma {
    width: 22mm; height: 22mm; flex: none; border-radius: 50%;
    display: grid; place-items: center; background: var(--acento); color: #fff;
    font-size: 17pt; font-weight: 700; letter-spacing: 0.02em;
  }
  .nombre-comercial { font-size: 19pt; font-weight: 800; line-height: 1.05; margin: 0 0 1.2mm; letter-spacing: -0.015em; }
  .razon { font-size: 8.6pt; font-weight: 600; text-transform: uppercase; letter-spacing: 0.02em; margin: 0 0 1.2mm; }
  .negocio p { margin: 0; }
  .negocio .linea { font-size: 8.6pt; color: var(--tenue); }

  /* Recuadro del RUC: el formato que el cliente peruano reconoce, más fino. */
  .documento { border: 0.45mm solid var(--acento); border-radius: 3mm; overflow: hidden; text-align: center; }
  .documento .ruc { padding: 2.6mm 2mm 2.2mm; font-size: 10.5pt; font-weight: 700; letter-spacing: 0.03em; }
  .documento .tipo {
    background: var(--acento); color: #fff; padding: 2mm 2mm; font-size: 8.8pt; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.09em;
  }
  .documento .numero { padding: 2.6mm 2mm 2.8mm; font-size: 15.5pt; font-weight: 800; letter-spacing: 0.01em; color: var(--acento); }

  .franja { height: 1.2mm; background: linear-gradient(90deg, var(--acento), var(--medio)); border-radius: 1mm; margin: 6mm 0 5mm; }

  .datos { display: grid; grid-template-columns: 1fr 64mm; gap: 8mm; margin-bottom: 6mm; }
  .tarjeta { background: var(--suave); border-radius: 2.5mm; padding: 3.5mm 4.5mm; }
  .etiqueta { font-size: 7.6pt; font-weight: 700; color: var(--acento); text-transform: uppercase; letter-spacing: 0.08em; margin-bottom: 1.2mm; }
  .etiqueta.separado { margin-top: 3mm; }
  .dato-grande { font-size: 11pt; font-weight: 700; margin-bottom: 0.6mm; }
  .fechas { margin: 0; display: grid; grid-template-columns: auto 1fr; column-gap: 4mm; row-gap: 1.1mm; }
  .fechas dt { color: var(--tenue); }
  .fechas dd { margin: 0; text-align: right; font-weight: 700; }

  table { border-collapse: collapse; width: 100%; }
  .items thead th {
    background: var(--acento); color: #fff; font-size: 7.8pt; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em;
    text-align: left; padding: 2.3mm 2.5mm;
  }
  .items thead th:first-child { border-radius: 2mm 0 0 2mm; }
  .items thead th:last-child { border-radius: 0 2mm 2mm 0; }
  .items td { padding: 2.6mm 2.5mm; border-bottom: 0.25mm solid var(--regla); vertical-align: top; }
  .items tbody tr:nth-child(even) td { background: #fafbfa; }
  .items tr { page-break-inside: avoid; }
  .items .num, .items th.num { text-align: right; white-space: nowrap; }
  .items .n { color: var(--tenue); width: 7mm; }
  .items .unidad { color: var(--tenue); white-space: nowrap; }
  .items .desc { width: 100%; }
  .items .fuerte { font-weight: 700; }
  .codigo { display: block; font-size: 7.6pt; color: var(--tenue); margin-top: 0.4mm; }

  .pie-items { display: grid; grid-template-columns: 1fr 70mm; gap: 8mm; margin-top: 5mm; page-break-inside: avoid; }
  .letras { background: var(--suave); border-radius: 2.5mm; padding: 3mm 4mm; margin: 0 0 4mm; }
  .letras .etiqueta { margin-bottom: 0.6mm; }
  .letras strong { font-weight: 700; }
  .totales { align-self: start; }
  .totales th { text-align: left; font-weight: normal; color: var(--tenue); padding: 1.2mm 0; }
  .totales td { text-align: right; padding: 1.2mm 0; font-weight: 600; }
  .total-caja {
    display: flex; justify-content: space-between; align-items: center; margin-top: 2mm;
    background: var(--acento); color: #fff; border-radius: 2.5mm; padding: 3mm 4mm;
  }
  .total-caja span { font-size: 9pt; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .total-caja strong { font-size: 15pt; font-weight: 800; }
  .aparte { margin-bottom: 4mm; }
  .aparte h3 { font-size: 9.3pt; margin: 0 0 1.5mm; }
  .aparte p { margin: 1.5mm 0 0; }
  .mini th { text-align: left; font-weight: 600; color: var(--tenue); padding: 0.8mm 3mm 0.8mm 0; border-bottom: 0.2mm solid var(--regla); }
  .mini td { padding: 0.8mm 3mm 0.8mm 0; }
  .mini .num { text-align: right; padding-right: 0; }

  .legal {
    display: flex; gap: 5mm; align-items: center; margin-top: 9mm; padding: 4mm;
    border: 0.25mm solid var(--regla); border-radius: 2.5mm; page-break-inside: avoid;
  }
  .legal img { width: 25mm; height: 25mm; image-rendering: pixelated; flex: none; }
  .legal p { margin: 0 0 1.2mm; font-size: 8.4pt; }
  .legal .gracias { font-size: 10.5pt; font-weight: 700; color: var(--acento); margin-bottom: 1.6mm; }
  .hash { word-break: break-all; }
  .firma { margin-top: 3mm; text-align: center; font-size: 7.6pt; color: #9aa39e; letter-spacing: 0.04em; }
</style>
</head>
<body>
<div class="hoja">
  <header class="cabecera">
    <div class="negocio">
      ${marca}
      <div>
        <p class="nombre-comercial">${escapar(nombreComercial)}</p>
        ${mostrarRazon ? `<p class="razon">${escapar(emisor.razonSocial)}</p>` : ''}
        ${emisor.direccion ? `<p class="linea">${escapar(emisor.direccion)}</p>` : ''}
        ${contacto ? `<p class="linea">${contacto}</p>` : ''}
      </div>
    </div>
    <div class="documento">
      <div class="ruc">R.U.C. ${escapar(emisor.ruc)}</div>
      <div class="tipo">${nombreTipo}</div>
      <div class="numero">${escapar(numero)}</div>
    </div>
  </header>

  <div class="franja"></div>

  <section class="datos">${bloqueCliente}${bloqueFechas}</section>

  <table class="items">
    <thead>
      <tr><th>#</th><th>Descripción</th><th class="num">Cant.</th><th>Unidad</th><th class="num">P. unit.</th><th class="num">Importe</th></tr>
    </thead>
    <tbody>${filasItems}</tbody>
  </table>

  <section class="pie-items">
    <div>
      <div class="letras"><div class="etiqueta">Importe en letras</div><strong>${escapar(montoEnLetras(totales.total))}</strong></div>
      ${bloqueCuotas}
      ${bloqueDetraccion}
      ${datos.observacion ? `<div class="aparte"><h3>Observación</h3><p>${escapar(datos.observacion)}</p></div>` : ''}
    </div>
    <div class="totales">
      <table>${filasTotales}</table>
      <div class="total-caja"><span>Total</span><strong>S/ ${soles(totales.total)}</strong></div>
    </div>
  </section>

  <footer class="legal">
    ${qr ? `<img src="${escapar(qr)}" alt="Código QR de SUNAT">` : ''}
    <div>
      <p class="gracias">¡Gracias por su preferencia!</p>
      <p>Representación impresa de la ${nombreTipo.toLowerCase()}. Consulte su validez en sunat.gob.pe.</p>
      ${comprobante.hash ? `<p class="tenue hash">Valor resumen: ${escapar(comprobante.hash)}</p>` : ''}
    </div>
  </footer>
  <div class="firma">Emitido con Monspeet POS</div>
</div>
</body>
</html>`;
}

function nombreDelTipo(tipo) {
  if (tipo === 'FACTURA') return 'Factura electrónica';
  if (tipo === 'NOTA_CREDITO') return 'Nota de crédito electrónica';
  return 'Boleta de venta electrónica';
}

/** Imprime el A4 desde un iframe oculto (no toca la página actual). */
export function imprimirComprobanteA4(datos) {
  imprimirHtml(htmlComprobanteA4(datos));
}

/** Imprime el ticket de 80 mm desde un iframe oculto. */
export function imprimirComprobanteTicket(datos) {
  imprimirHtml(htmlComprobanteTicket(datos));
}

function imprimirHtml(html) {
  const marco = document.createElement('iframe');
  marco.setAttribute('aria-hidden', 'true');
  marco.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;visibility:hidden;';
  document.body.appendChild(marco);
  const doc = marco.contentWindow.document;
  doc.open();
  doc.write(html);
  doc.close();
  const imprimir = () => {
    marco.contentWindow.focus();
    marco.contentWindow.print();
    setTimeout(() => marco.remove(), 1000);
  };
  // Espera a que carguen el logo y el QR antes de imprimir.
  const imagenes = Array.from(doc.images);
  Promise.all(
    imagenes.map((img) => (img.complete ? null : new Promise((ok) => { img.onload = ok; img.onerror = ok; })))
  ).then(() => setTimeout(imprimir, 50));
}

// ===== Desde el documento enviado a SUNAT (emisión directa) =====

const NOMBRE_UNIDAD = { NIU: 'Unidad', KGM: 'Kg', GRM: 'g', LTR: 'L', MLT: 'mL', ZZ: 'Servicio' };
const NOMBRE_DOCUMENTO = { 6: 'RUC', 1: 'DNI', 4: 'C.E.', 7: 'Pasaporte', 0: 'Doc.' };

/** "2026-10-09T10:42:00-05:00" -> { fecha: "09/10/2026", hora: "10:42", iso: "2026-10-09" } */
function partirFecha(texto) {
  const [iso = '', resto = ''] = String(texto || '').split('T');
  const [a, m, d] = iso.split('-');
  return { fecha: d ? `${d}/${m}/${a}` : iso, hora: resto.slice(0, 5), iso };
}

const r2 = (n) => Math.round(Number(n || 0) * 100) / 100;

/**
 * Datos del A4 a partir del documento que se envió a SUNAT (GET
 * /comprobantes/:id/documento) más lo que el documento no trae: el hash,
 * el logo, el color y los datos de contacto del negocio.
 * extra = { hash, logo, color, telefono, email }
 */
export async function datosA4DeDocumento(doc, extra = {}) {
  const empresa = doc.company || {};
  const dir = empresa.address || {};
  const ubicacion = [dir.distrito, dir.provincia, dir.departamento].filter((x) => x && x !== '-').join(' - ');
  const fecha = partirFecha(doc.fechaEmision);
  const tipo = doc.tipoDoc === '01' ? 'FACTURA' : doc.tipoDoc === '07' ? 'NOTA_CREDITO' : 'BOLETA';
  const cli = doc.client || {};
  const sinDocumento = !cli.numDoc || cli.numDoc === '-';
  const detalles = doc.details || [];
  const gravado = detalles.find((d) => d.tipAfeIgv === '10');

  let qr = null;
  if (extra.hash) {
    const cadena = construirCadenaQrSunat({
      ruc: empresa.ruc,
      tipoDocumento: doc.tipoDoc,
      serie: doc.serie,
      numero: doc.correlativo,
      igv: doc.mtoIGV,
      total: doc.mtoImpVenta,
      fechaEmision: fecha.iso,
      tipoDocCliente: cli.tipoDoc || '-',
      numDocCliente: cli.numDoc || '-',
      hash: extra.hash,
    });
    qr = await QRCode.toDataURL(cadena, { margin: 0, scale: 6 }).catch(() => null);
  }

  return {
    emisor: {
      nombreComercial: empresa.nombreComercial,
      razonSocial: empresa.razonSocial,
      ruc: empresa.ruc,
      direccion: [dir.direccion, ubicacion].filter(Boolean).join(', '),
      telefono: extra.telefono,
      email: extra.email,
      logo: extra.logo,
      color: extra.color,
    },
    comprobante: { tipo, serie: doc.serie, numero: doc.correlativo, fechaEmision: fecha.fecha, horaEmision: fecha.hora, hash: extra.hash },
    referencia:
      doc.tipoDoc === '07'
        ? { documento: doc.numDocfectado, tipo: doc.tipDocAfectado === '01' ? 'Factura' : 'Boleta', motivo: doc.desMotivo }
        : null,
    cliente: sinDocumento && (!cli.rznSocial || cli.rznSocial === 'CLIENTES VARIOS')
      ? null
      : {
          tipoDocumento: NOMBRE_DOCUMENTO[cli.tipoDoc] || 'Doc.',
          numeroDocumento: sinDocumento ? null : cli.numDoc,
          nombre: cli.rznSocial,
          direccion: cli.address?.direccion,
        },
    items: detalles.map((d) => ({
      cantidad: d.cantidad,
      unidad: NOMBRE_UNIDAD[d.unidad] || d.unidad,
      descripcion: d.descripcion,
      precioUnitario: d.mtoPrecioUnitario,
      importe: r2(Number(d.mtoValorVenta || 0) + Number(d.igv || 0)),
    })),
    totales: {
      gravadas: doc.mtoOperGravadas,
      exoneradas: doc.mtoOperExoneradas,
      inafectas: doc.mtoOperInafectas,
      igv: doc.mtoIGV,
      tasa: gravado?.porcentajeIgv ?? 18,
      total: doc.mtoImpVenta,
    },
    formaPago: doc.formaPago?.tipo === 'Credito' ? 'Crédito' : 'Contado',
    cuotas: (doc.cuotas || []).map((c) => ({ fecha: partirFecha(c.fechaPago).fecha, monto: c.monto })),
    detraccion: doc.detraccion
      ? { porcentaje: doc.detraccion.percent, monto: doc.detraccion.mount, cuenta: doc.detraccion.ctaBanco }
      : null,
    qr,
  };
}

// ===== Ticket de 80 mm =====

/**
 * El mismo comprobante (los mismos datos que el A4) en un ticket de 80 mm
 * para la impresora térmica: todo en negro, una columna.
 */
export function htmlComprobanteTicket(datos) {
  const { emisor = {}, comprobante = {}, cliente, items = [], totales = {}, referencia, qr } = datos;
  const nombreTipo = nombreDelTipo(comprobante.tipo);
  const numero = `${comprobante.serie}-${String(comprobante.numero).padStart(8, '0')}`;
  const nombreComercial = emisor.nombreComercial || emisor.razonSocial || '';
  const mostrarRazon = emisor.razonSocial && emisor.razonSocial.trim() !== nombreComercial.trim();
  const fila = (etiqueta, valor) => `<tr><th>${etiqueta}</th><td>S/ ${soles(valor)}</td></tr>`;
  const marca = emisor.logo
    ? `<img class="logo" src="${escapar(emisor.logo)}" alt="">`
    : `<div class="monograma">${escapar(iniciales(nombreComercial))}</div>`;
  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>${escapar(nombreTipo)} ${escapar(numero)}</title>
<style>
  @page { size: 80mm auto; margin: 0; }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: #fff; color: #000; }
  body { width: 72mm; margin: 0 auto; padding: 4mm 0 7mm; font-family: Arial, Helvetica, sans-serif; font-size: 11px; line-height: 1.38; }
  .centro { text-align: center; }
  .logo { max-width: 40mm; max-height: 22mm; display: block; margin: 0 auto 2mm; }
  .monograma {
    width: 14mm; height: 14mm; margin: 0 auto 2mm; border-radius: 50%; background: #000; color: #fff;
    display: grid; place-items: center; font-size: 17px; font-weight: 700;
  }
  .nombre { font-size: 17px; font-weight: 800; letter-spacing: 0.01em; }
  .razon { font-size: 10px; font-weight: 700; text-transform: uppercase; }
  p { margin: 0.7mm 0; }
  .tipo { margin: 3mm 0 2.5mm; border: 1.5px solid #000; border-radius: 2mm; overflow: hidden; }
  .tipo .ruc { padding: 1.2mm 0; font-weight: 700; }
  .tipo .nom { background: #000; color: #fff; padding: 1.2mm 0; font-weight: 700; font-size: 10.5px; letter-spacing: 0.06em; }
  .tipo .numero { padding: 1.4mm 0; font-size: 15px; font-weight: 800; }
  .datos p { display: flex; gap: 2mm; }
  .datos b { flex: none; width: 15mm; }
  table { width: 100%; border-collapse: collapse; }
  .items th { text-align: left; border-top: 1px solid #000; border-bottom: 1px solid #000; font-weight: 700; padding: 1mm 0; font-size: 10px; text-transform: uppercase; }
  .items td { vertical-align: top; padding: 1.3mm 0; border-bottom: 1px dotted #888; }
  .items th:first-child, .items td:first-child { width: 9mm; padding-right: 1.5mm; }
  .num { text-align: right; white-space: nowrap; }
  .totales { margin-top: 1.5mm; }
  .totales th { text-align: left; font-weight: normal; padding: 0.4mm 0; }
  .totales td { text-align: right; padding: 0.4mm 0; }
  .total { margin-top: 1.5mm; display: flex; justify-content: space-between; background: #000; color: #fff; padding: 1.8mm 2.5mm; border-radius: 1.5mm; font-size: 14px; font-weight: 800; }
  .bloque { margin-top: 2.5mm; padding-top: 2mm; border-top: 1px dashed #000; }
  .qr { width: 28mm; height: 28mm; display: block; margin: 2mm auto 1mm; image-rendering: pixelated; }
  .pequeno { font-size: 9.5px; }
  .gracias { font-size: 12.5px; font-weight: 800; margin-top: 2mm; }
  .hash { word-break: break-all; }
</style>
</head>
<body>
  <div class="centro">
    ${marca}
    <div class="nombre">${escapar(nombreComercial)}</div>
    ${mostrarRazon ? `<p class="razon">${escapar(emisor.razonSocial)}</p>` : ''}
    ${emisor.direccion ? `<p>${escapar(emisor.direccion)}</p>` : ''}
    ${emisor.telefono ? `<p>Tel. ${escapar(emisor.telefono)}</p>` : ''}
    <div class="tipo">
      <div class="ruc">R.U.C. ${escapar(emisor.ruc)}</div>
      <div class="nom">${escapar(nombreTipo.toUpperCase())}</div>
      <div class="numero">${escapar(numero)}</div>
    </div>
  </div>
  <div class="datos">
    <p><b>Fecha</b><span>${escapar(comprobante.fechaEmision)}${comprobante.horaEmision ? ` ${escapar(comprobante.horaEmision)}` : ''}</span></p>
    <p><b>Cliente</b><span>${escapar(cliente?.nombre || 'Clientes varios')}</span></p>
    ${cliente?.numeroDocumento ? `<p><b>${escapar(cliente.tipoDocumento || 'Doc.')}</b><span>${escapar(cliente.numeroDocumento)}</span></p>` : ''}
    ${cliente?.direccion ? `<p><b>Dirección</b><span>${escapar(cliente.direccion)}</span></p>` : ''}
  </div>
  ${
    referencia
      ? `<div class="bloque"><p><strong>Modifica a:</strong> ${escapar(referencia.tipo)} ${escapar(referencia.documento)}</p>
         <p><strong>Motivo:</strong> ${escapar(referencia.motivo)}</p></div>`
      : ''
  }
  <div class="bloque">
    <table class="items">
      <thead><tr><th>Cant.</th><th>Descripción</th><th class="num">Importe</th></tr></thead>
      <tbody>
        ${items
          .map(
            (it) => `<tr><td>${cantidad(it.cantidad)}</td><td>${escapar(it.descripcion)}<br><span class="pequeno">P. unit. S/ ${soles(it.precioUnitario)}</span></td><td class="num">${soles(it.importe)}</td></tr>`
          )
          .join('')}
      </tbody>
    </table>
    <table class="totales">
      ${fila('Op. gravada', totales.gravadas)}
      ${totales.exoneradas > 0 ? fila('Op. exonerada', totales.exoneradas) : ''}
      ${totales.inafectas > 0 ? fila('Op. inafecta', totales.inafectas) : ''}
      ${fila(`IGV ${String(totales.tasa ?? 18).replace('.', ',')} %`, totales.igv)}
    </table>
    <div class="total"><span>TOTAL</span><span>S/ ${soles(totales.total)}</span></div>
    <p class="pequeno" style="margin-top:1.5mm">Son: ${escapar(montoEnLetras(totales.total))}</p>
  </div>
  <div class="centro bloque">
    ${qr ? `<img class="qr" src="${escapar(qr)}" alt="Código QR de SUNAT">` : ''}
    <p class="pequeno">Representación impresa de la ${escapar(nombreTipo.toLowerCase())}. Consulte su validez en sunat.gob.pe.</p>
    ${comprobante.hash ? `<p class="pequeno hash">Valor resumen: ${escapar(comprobante.hash)}</p>` : ''}
    <p class="gracias">¡Gracias por su compra!</p>
    <p class="pequeno">Emitido con Monspeet POS</p>
  </div>
</body>
</html>`;
}
