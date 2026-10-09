// Comprobante electrónico en hoja A4 (factura, boleta o nota de crédito),
// con diseño propio. Al final, el mismo comprobante en ticket de 80 mm.
//
// Con FacturaLibre el A4 era su PDF; con la emisión directa a SUNAT lo arma
// el sistema. Se genera como una página HTML independiente (sus propios
// estilos y su propio @page A4) y se imprime desde un iframe oculto, así no
// choca con el @page de 80 mm del ticket. Desde el diálogo de impresión
// también se puede "Guardar como PDF".
//
// Pensado para imprimirse en una impresora de oficina, incluso en blanco y
// negro: todo el texto es oscuro y el único elemento de color es el bloque
// del RUC y el número, con el color de acento del negocio.

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
  const credito = datos.formaPago === 'Crédito' && cuotas.length > 0;
  const nombreComercial = emisor.nombreComercial || emisor.razonSocial || '';
  const mostrarRazon = emisor.razonSocial && emisor.razonSocial.trim() !== nombreComercial.trim();

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
      (it) => `<tr>
        <td class="num">${cantidad(it.cantidad)}</td>
        <td class="unidad">${escapar(it.unidad || 'Unidad')}</td>
        <td class="desc">${escapar(it.descripcion)}${it.codigo ? `<span class="codigo">${escapar(it.codigo)}</span>` : ''}</td>
        <td class="num">${soles(it.precioUnitario)}</td>
        <td class="num">${soles(it.importe)}</td>
      </tr>`
    )
    .join('');

  const bloqueCliente = cliente
    ? `<div class="dato-grande">${escapar(cliente.nombre || 'Clientes varios')}</div>
       ${cliente.numeroDocumento ? `<div>${escapar(cliente.tipoDocumento || 'Doc.')} ${escapar(cliente.numeroDocumento)}</div>` : ''}
       ${cliente.direccion ? `<div class="tenue">${escapar(cliente.direccion)}</div>` : ''}`
    : `<div class="dato-grande">Clientes varios</div>`;

  const bloqueCuotas = credito
    ? `<div class="aparte">
         <h3>Pago al crédito</h3>
         <table class="mini">
           <tr><th>Cuota</th><th>Vence</th><th class="num">Monto</th></tr>
           ${cuotas
             .map((c, i) => `<tr><td>${i + 1}</td><td>${escapar(c.fecha)}</td><td class="num">S/ ${soles(c.monto)}</td></tr>`)
             .join('')}
         </table>
         <p class="tenue">Monto neto pendiente de pago: S/ ${soles(cuotas.reduce((s, c) => s + Number(c.monto || 0), 0))}</p>
       </div>`
    : '';

  const bloqueDetraccion =
    detraccion && detraccion.monto > 0
      ? `<div class="aparte">
           <h3>Operación sujeta a detracción</h3>
           <p>Detracción (${String(detraccion.porcentaje).replace('.', ',')} %): S/ ${soles(detraccion.monto)}<br>
           ${detraccion.cuenta ? `Cta. Banco de la Nación: ${escapar(detraccion.cuenta)}<br>` : ''}
           Neto a pagar: S/ ${soles(totales.total - detraccion.monto)}</p>
         </div>`
      : '';

  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>${escapar(nombreTipo)} ${escapar(numero)}</title>
<style>
  @page { size: A4; margin: 14mm 14mm 16mm; }
  * { box-sizing: border-box; }
  :root { --acento: ${acento}; --tinta: #1f2622; --tenue: #59625d; --regla: #d5dbd7; }
  html, body { margin: 0; padding: 0; background: #fff; }
  body {
    font-family: "Helvetica Neue", Helvetica, Arial, "Liberation Sans", sans-serif;
    font-size: 9.5pt; line-height: 1.4; color: var(--tinta);
    font-variant-numeric: tabular-nums;
    -webkit-print-color-adjust: exact; print-color-adjust: exact;
  }
  .hoja { width: 182mm; margin: 0 auto; }
  .tenue { color: var(--tenue); }

  /* Cabecera: el negocio a la izquierda, el comprobante a la derecha. */
  .cabecera { display: flex; justify-content: space-between; align-items: stretch; gap: 10mm; }
  .negocio { display: flex; gap: 4mm; align-items: flex-start; min-width: 0; }
  .logo { width: 22mm; height: 22mm; object-fit: contain; }
  .nombre-comercial { font-size: 17pt; font-weight: 700; line-height: 1.1; margin: 0 0 1mm; letter-spacing: -0.01em; }
  .razon { font-weight: 600; margin-bottom: 1.5mm; }
  .negocio p { margin: 0; }

  /* El elemento con identidad: bloque del documento con la barra de acento. */
  .documento {
    flex: 0 0 66mm; border-left: 2.2mm solid var(--acento);
    padding: 2.5mm 0 2.5mm 5mm; display: flex; flex-direction: column; justify-content: center;
  }
  .documento .ruc { font-size: 10.5pt; font-weight: 600; }
  .documento .tipo { font-size: 10.5pt; margin-top: 1mm; }
  .documento .numero { font-size: 19pt; font-weight: 700; color: var(--acento); letter-spacing: -0.01em; margin-top: 1.5mm; line-height: 1; white-space: nowrap; }

  .datos { display: grid; grid-template-columns: 1fr 64mm; gap: 10mm; margin: 8mm 0 6mm; padding: 4mm 0; border-top: 0.3mm solid var(--tinta); border-bottom: 0.3mm solid var(--regla); }
  .datos h2 { font-size: 8.5pt; font-weight: 600; color: var(--tenue); margin: 0 0 1mm; }
  .datos h2.separado { margin-top: 3.5mm; }
  .dato-grande { font-size: 11pt; font-weight: 600; }
  .fechas { margin: 0; display: grid; grid-template-columns: auto 1fr; column-gap: 4mm; row-gap: 0.8mm; }
  .fechas dt { color: var(--tenue); }
  .fechas dd { margin: 0; text-align: right; font-weight: 600; }

  table { border-collapse: collapse; width: 100%; }
  .items th { font-size: 8.5pt; font-weight: 600; color: var(--tenue); text-align: left; padding: 0 2mm 1.8mm; border-bottom: 0.3mm solid var(--tinta); }
  .items td { padding: 2mm; border-bottom: 0.2mm solid var(--regla); vertical-align: top; }
  .items tr { page-break-inside: avoid; }
  .items .num, .items th.num { text-align: right; white-space: nowrap; }
  .items .unidad { color: var(--tenue); white-space: nowrap; }
  .items .desc { width: 100%; }
  .codigo { display: block; font-size: 8pt; color: var(--tenue); }

  .pie-items { display: grid; grid-template-columns: 1fr 64mm; gap: 10mm; margin-top: 5mm; page-break-inside: avoid; }
  .letras { margin: 0 0 4mm; }
  .letras strong { font-weight: 600; }
  .totales { align-self: start; }
  .totales th { text-align: left; font-weight: normal; color: var(--tenue); padding: 1mm 0; }
  .totales td { text-align: right; padding: 1mm 0; }
  .totales .total th, .totales .total td { font-size: 13pt; font-weight: 700; color: var(--tinta); border-top: 0.3mm solid var(--tinta); padding-top: 2.5mm; }
  .aparte { margin-bottom: 4mm; }
  .aparte h3 { font-size: 9.5pt; margin: 0 0 1.5mm; }
  .aparte p { margin: 1.5mm 0 0; }
  .mini th { text-align: left; font-weight: 600; color: var(--tenue); padding: 0.8mm 3mm 0.8mm 0; border-bottom: 0.2mm solid var(--regla); }
  .mini td { padding: 0.8mm 3mm 0.8mm 0; }
  .mini .num { text-align: right; padding-right: 0; }

  .legal { display: flex; gap: 5mm; align-items: center; margin-top: 9mm; padding-top: 4mm; border-top: 0.2mm solid var(--regla); page-break-inside: avoid; }
  .legal img { width: 24mm; height: 24mm; image-rendering: pixelated; }
  .legal p { margin: 0 0 1mm; font-size: 8.5pt; }
  .hash { word-break: break-all; }
</style>
</head>
<body>
<div class="hoja">
  <header class="cabecera">
    <div class="negocio">
      ${emisor.logo ? `<img class="logo" src="${escapar(emisor.logo)}" alt="">` : ''}
      <div>
        <p class="nombre-comercial">${escapar(nombreComercial)}</p>
        ${mostrarRazon ? `<p class="razon">${escapar(emisor.razonSocial)}</p>` : ''}
        ${emisor.direccion ? `<p class="tenue">${escapar(emisor.direccion)}</p>` : ''}
        ${emisor.telefono ? `<p class="tenue">Teléfono ${escapar(emisor.telefono)}</p>` : ''}
        ${emisor.email ? `<p class="tenue">${escapar(emisor.email)}</p>` : ''}
      </div>
    </div>
    <div class="documento">
      <div class="ruc">RUC ${escapar(emisor.ruc)}</div>
      <div class="tipo">${nombreTipo}</div>
      <div class="numero">${escapar(numero)}</div>
    </div>
  </header>

  <section class="datos">
    <div>
      <h2>Cliente</h2>
      ${bloqueCliente}
      ${
        referencia
          ? `<h2 class="separado">Documento que modifica</h2>
             <div class="dato-grande">${escapar(referencia.tipo)} ${escapar(referencia.documento)}</div>
             <div>Motivo: ${escapar(referencia.motivo)}</div>`
          : ''
      }
    </div>
    <dl class="fechas">
      <dt>Emisión</dt><dd>${escapar(comprobante.fechaEmision)}${comprobante.horaEmision ? ` ${escapar(comprobante.horaEmision)}` : ''}</dd>
      ${credito ? `<dt>Vencimiento</dt><dd>${escapar(cuotas[cuotas.length - 1].fecha)}</dd>` : ''}
      <dt>Moneda</dt><dd>Soles</dd>
      ${esFactura ? `<dt>Forma de pago</dt><dd>${escapar(datos.formaPago || 'Contado')}</dd>` : ''}
    </dl>
  </section>

  <table class="items">
    <thead>
      <tr><th class="num">Cant.</th><th>Unidad</th><th>Descripción</th><th class="num">P. unit.</th><th class="num">Importe</th></tr>
    </thead>
    <tbody>${filasItems}</tbody>
  </table>

  <section class="pie-items">
    <div>
      <p class="letras">Son: <strong>${escapar(montoEnLetras(totales.total))}</strong></p>
      ${bloqueCuotas}
      ${bloqueDetraccion}
      ${datos.observacion ? `<div class="aparte"><h3>Observación</h3><p>${escapar(datos.observacion)}</p></div>` : ''}
    </div>
    <table class="totales">
      ${filasTotales}
      <tr class="total"><th>Total</th><td>S/ ${soles(totales.total)}</td></tr>
    </table>
  </section>

  <footer class="legal">
    ${qr ? `<img src="${escapar(qr)}" alt="Código QR de SUNAT">` : ''}
    <div>
      <p>Representación impresa de la ${nombreTipo.toLowerCase()}. Consulte su validez en sunat.gob.pe.</p>
      ${comprobante.hash ? `<p class="tenue hash">Valor resumen: ${escapar(comprobante.hash)}</p>` : ''}
      <p class="tenue">Emitido con Monspeet POS</p>
    </div>
  </footer>
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
  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>${escapar(nombreTipo)} ${escapar(numero)}</title>
<style>
  @page { size: 80mm auto; margin: 0; }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: #fff; color: #000; }
  body { width: 72mm; margin: 0 auto; padding: 3mm 0 6mm; font-family: Arial, Helvetica, sans-serif; font-size: 11px; line-height: 1.35; }
  .centro { text-align: center; }
  .logo { max-width: 34mm; max-height: 18mm; display: block; margin: 0 auto 2mm; }
  .nombre { font-size: 15px; font-weight: 700; }
  .tipo { margin-top: 2mm; padding: 1.5mm 0; border-top: 1px dashed #000; border-bottom: 1px dashed #000; font-weight: 700; }
  p { margin: 0.6mm 0; }
  table { width: 100%; border-collapse: collapse; }
  .items th { text-align: left; border-bottom: 1px solid #000; font-weight: 700; padding: 1mm 0; }
  .items td { vertical-align: top; padding: 1mm 0; }
  .num { text-align: right; white-space: nowrap; }
  .totales { margin-top: 1.5mm; border-top: 1px solid #000; }
  .totales th { text-align: left; font-weight: normal; padding: 0.4mm 0; }
  .totales td { text-align: right; padding: 0.4mm 0; }
  .totales .total th, .totales .total td { font-size: 13px; font-weight: 700; }
  .bloque { margin-top: 2mm; padding-top: 1.5mm; border-top: 1px dashed #000; }
  .qr { width: 26mm; height: 26mm; display: block; margin: 2mm auto 1mm; image-rendering: pixelated; }
  .pequeno { font-size: 9.5px; }
  .hash { word-break: break-all; }
</style>
</head>
<body>
  <div class="centro">
    ${emisor.logo ? `<img class="logo" src="${escapar(emisor.logo)}" alt="">` : ''}
    <div class="nombre">${escapar(nombreComercial)}</div>
    ${mostrarRazon ? `<p>${escapar(emisor.razonSocial)}</p>` : ''}
    <p>RUC ${escapar(emisor.ruc)}</p>
    ${emisor.direccion ? `<p>${escapar(emisor.direccion)}</p>` : ''}
    ${emisor.telefono ? `<p>Tel. ${escapar(emisor.telefono)}</p>` : ''}
    <div class="tipo">${escapar(nombreTipo.toUpperCase())}<br>${escapar(numero)}</div>
  </div>
  <p>Fecha: ${escapar(comprobante.fechaEmision)}${comprobante.horaEmision ? ` ${escapar(comprobante.horaEmision)}` : ''}</p>
  <p>Cliente: ${escapar(cliente?.nombre || 'Clientes varios')}</p>
  ${cliente?.numeroDocumento ? `<p>${escapar(cliente.tipoDocumento || 'Doc.')}: ${escapar(cliente.numeroDocumento)}</p>` : ''}
  ${cliente?.direccion ? `<p>Dirección: ${escapar(cliente.direccion)}</p>` : ''}
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
            (it) => `<tr><td>${cantidad(it.cantidad)}</td><td>${escapar(it.descripcion)}<br><span class="pequeno">P. unit. ${soles(it.precioUnitario)}</span></td><td class="num">${soles(it.importe)}</td></tr>`
          )
          .join('')}
      </tbody>
    </table>
    <table class="totales">
      ${fila('Op. gravada', totales.gravadas)}
      ${totales.exoneradas > 0 ? fila('Op. exonerada', totales.exoneradas) : ''}
      ${totales.inafectas > 0 ? fila('Op. inafecta', totales.inafectas) : ''}
      ${fila(`IGV ${String(totales.tasa ?? 18).replace('.', ',')} %`, totales.igv)}
      <tr class="total"><th>TOTAL</th><td>S/ ${soles(totales.total)}</td></tr>
    </table>
    <p>Son: ${escapar(montoEnLetras(totales.total))}</p>
  </div>
  <div class="centro bloque">
    ${qr ? `<img class="qr" src="${escapar(qr)}" alt="Código QR de SUNAT">` : ''}
    <p class="pequeno">Representación impresa de la ${escapar(nombreTipo.toLowerCase())}. Consulte su validez en sunat.gob.pe.</p>
    ${comprobante.hash ? `<p class="pequeno hash">Valor resumen: ${escapar(comprobante.hash)}</p>` : ''}
    <p class="pequeno">Emitido con Monspeet POS</p>
  </div>
</body>
</html>`;
}
