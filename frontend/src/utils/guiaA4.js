// Guía de remisión en hoja A4, con el mismo diseño que la factura y la
// boleta en A4 (utils/facturaA4.js): logo, recuadro del RUC, tarjetas y
// tabla con el color de acento del negocio. Se imprime desde un iframe
// oculto y en el diálogo de impresión se elige "Guardar como PDF", igual
// que el A4 de la factura.
//
// Mismo contenido que el ticket (components/GuiaImprimible.jsx) y su PDF de
// 80 mm (pdfDeGuia en utils/pdfTicket.js): si cambia uno, revisar los otros.

import QRCode from 'qrcode';
import { aclarar, acentoImprimible, escapar, imprimirHtml, iniciales } from './facturaA4';
import { fechaCorta } from './formato';
import { formatoCantidad } from './medidas';
import { etiquetaUnidad } from './unidades';
import { NOMBRE_DOCUMENTO, lugarDeGuia, motivoDeGuia, numeroConCeros } from './guiaTexto';

/**
 * Imprime la guía en A4.
 * emisor = { nombreComercial, razonSocial, ruc, direccion, telefono, email, logo, color }
 */
export async function imprimirGuiaA4({ guia, motivos, emisor }) {
  const datos = guia?.datos;
  if (!datos) throw new Error('Esta guía no tiene sus datos guardados para armar el A4.');
  if (guia.estado !== 'ACEPTADA') throw new Error('Solo una guía aceptada por SUNAT tiene representación impresa.');
  const ubigeos = await import('../data/ubigeos').then((m) => m.UBIGEOS).catch(() => null);
  // El QR de SUNAT, si llegó: un enlace (se dibuja) o la imagen ya hecha.
  let qr = null;
  if (guia.qr?.startsWith('data:image/')) qr = guia.qr;
  else if (guia.qr) qr = await QRCode.toDataURL(guia.qr, { margin: 0, scale: 6 }).catch(() => null);
  imprimirHtml(htmlGuiaA4({ guia, datos, motivos, emisor, ubigeos, qr }));
}

function htmlGuiaA4({ guia, datos, motivos, emisor = {}, ubigeos, qr }) {
  const numero = numeroConCeros(guia.numero);
  const acento = acentoImprimible(emisor.color);
  const suave = aclarar(acento, 0.08);
  const medio = aclarar(acento, 0.18);
  const nombreComercial = emisor.nombreComercial || emisor.razonSocial || '';
  const mostrarRazon = emisor.razonSocial && emisor.razonSocial.trim() !== nombreComercial.trim();
  const contacto = [emisor.telefono ? `Tel. ${escapar(emisor.telefono)}` : '', emisor.email ? escapar(emisor.email) : '']
    .filter(Boolean)
    .join('<span class="punto">·</span>');
  const marca = emisor.logo
    ? `<img class="logo" src="${escapar(emisor.logo)}" alt="">`
    : `<div class="monograma">${escapar(iniciales(nombreComercial))}</div>`;

  const lugar = (punto) => escapar(lugarDeGuia(punto, ubigeos));
  const motivo = motivoDeGuia(datos, motivos);
  const privado = datos.modo !== 'PUBLICO';
  const items = datos.items || [];
  const fila = (etiqueta, valor) => (valor ? `<dt>${etiqueta}</dt><dd>${escapar(valor)}</dd>` : '');

  const comprobante = guia.comprobante
    ? fila('Comprobante', numeroConCeros(guia.comprobante))
    : fila('Venta', guia.folio_venta);

  const transporte = privado
    ? `${fila('Modalidad', 'Transporte privado')}
       ${fila('Placa', datos.placa)}
       ${datos.chofer ? fila('Conductor', `${datos.chofer.nombres || ''} ${datos.chofer.apellidos || ''}`.trim()) : ''}
       ${datos.chofer ? fila('DNI', datos.chofer.documento) : ''}
       ${datos.chofer ? fila('Licencia', datos.chofer.licencia) : ''}`
    : `${fila('Modalidad', 'Transporte público')}
       ${datos.transportista ? fila('Transportista', datos.transportista.nombre) : ''}
       ${datos.transportista ? fila('RUC', datos.transportista.ruc) : ''}
       ${datos.transportista ? fila('Registro MTC', datos.transportista.mtc) : ''}`;

  const filasItems = items
    .map(
      (it, i) => `<tr>
        <td class="n">${i + 1}</td>
        <td class="desc">${escapar(it.descripcion)}${it.codigo ? `<span class="codigo">Cód. ${escapar(it.codigo)}</span>` : ''}</td>
        <td class="num fuerte">${formatoCantidad(it.cantidad)}</td>
        <td class="unidad">${escapar(etiquetaUnidad(it.unidad) || 'Unidad')}</td>
      </tr>`
    )
    .join('');

  const consulta = qr
    ? 'Consulte este documento en SUNAT escaneando el código QR.'
    : 'Consulte este documento en SUNAT con el RUC del remitente y el número de la guía.';

  return `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>Guia ${escapar(numero)}</title>
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

  .documento { border: 0.45mm solid var(--acento); border-radius: 3mm; overflow: hidden; text-align: center; }
  .documento .ruc { padding: 2.6mm 2mm 2.2mm; font-size: 10.5pt; font-weight: 700; letter-spacing: 0.03em; }
  .documento .tipo {
    background: var(--acento); color: #fff; padding: 2mm 2mm; font-size: 8.4pt; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.07em; line-height: 1.3;
  }
  .documento .numero { padding: 2.6mm 2mm 2.8mm; font-size: 15.5pt; font-weight: 800; letter-spacing: 0.01em; color: var(--acento); }

  .franja { height: 1.2mm; background: linear-gradient(90deg, var(--acento), var(--medio)); border-radius: 1mm; margin: 6mm 0 5mm; }

  .datos { display: grid; grid-template-columns: 1fr 1fr; gap: 5mm; margin-bottom: 5mm; }
  .tarjeta { background: var(--suave); border-radius: 2.5mm; padding: 3.5mm 4.5mm; page-break-inside: avoid; }
  .etiqueta { font-size: 7.6pt; font-weight: 700; color: var(--acento); text-transform: uppercase; letter-spacing: 0.08em; margin-bottom: 1.2mm; }
  .etiqueta.separado { margin-top: 3mm; }
  .dato-grande { font-size: 11pt; font-weight: 700; margin-bottom: 0.6mm; }
  .fechas { margin: 0; display: grid; grid-template-columns: auto 1fr; column-gap: 4mm; row-gap: 1.1mm; }
  .fechas dt { color: var(--tenue); }
  .fechas dd { margin: 0; text-align: right; font-weight: 700; overflow-wrap: anywhere; }

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

  .resumen { display: flex; justify-content: flex-end; gap: 8mm; margin-top: 3mm; font-size: 8.8pt; color: var(--tenue); }
  .resumen strong { color: var(--tinta); }
  .aparte { margin-top: 4mm; page-break-inside: avoid; }
  .aparte h3 { font-size: 9.3pt; margin: 0 0 1.5mm; }
  .aparte p { margin: 0; }

  .legal {
    display: flex; gap: 5mm; align-items: center; margin-top: 9mm; padding: 4mm;
    border: 0.25mm solid var(--regla); border-radius: 2.5mm; page-break-inside: avoid;
  }
  .legal img { width: 25mm; height: 25mm; image-rendering: pixelated; flex: none; }
  .legal p { margin: 0 0 1.2mm; font-size: 8.4pt; }
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
      <div class="tipo">Guía de remisión electrónica<br>remitente</div>
      <div class="numero">${escapar(numero)}</div>
    </div>
  </header>

  <div class="franja"></div>

  <section class="datos">
    <div class="tarjeta">
      <div class="etiqueta">Destinatario</div>
      <div class="dato-grande">${escapar(datos.destinatario_nombre)}</div>
      <div>${escapar(NOMBRE_DOCUMENTO[datos.destinatario_tipo] || 'Doc.')} <strong>${escapar(datos.destinatario_documento)}</strong></div>
    </div>
    <div class="tarjeta">
      <dl class="fechas">
        ${fila('Fecha de emisión', fechaCorta(guia.fecha))}
        ${fila('Inicio de traslado', fechaCorta(datos.fecha_traslado))}
        ${fila('Motivo', motivo)}
        ${comprobante}
      </dl>
    </div>
    <div class="tarjeta">
      <div class="etiqueta">Punto de partida</div>
      <div>${lugar(datos.partida)}</div>
      <div class="etiqueta separado">Punto de llegada</div>
      <div>${lugar(datos.llegada)}</div>
    </div>
    <div class="tarjeta">
      <div class="etiqueta">Transporte</div>
      <dl class="fechas">
        ${transporte}
        ${fila('Peso bruto total', `${formatoCantidad(datos.peso_total)} kg`)}
        ${fila('Bultos', datos.bultos != null ? String(datos.bultos) : '')}
      </dl>
    </div>
  </section>

  <table class="items">
    <thead>
      <tr><th>#</th><th>Bienes a trasladar</th><th class="num">Cant.</th><th>Unidad</th></tr>
    </thead>
    <tbody>${filasItems}</tbody>
  </table>
  <div class="resumen"><span>Ítems: <strong>${items.length}</strong></span></div>

  ${datos.observaciones ? `<div class="aparte"><h3>Observaciones</h3><p>${escapar(datos.observaciones)}</p></div>` : ''}

  <footer class="legal">
    ${qr ? `<img src="${escapar(qr)}" alt="Código QR de SUNAT">` : ''}
    <div>
      <p><strong>Representación impresa de la guía de remisión electrónica remitente.</strong></p>
      <p class="tenue">${consulta}</p>
    </div>
  </footer>
  <div class="firma">Emitido con Monspeet POS</div>
</div>
</body>
</html>`;
}

// Para las pruebas: el HTML sin imprimir.
export { htmlGuiaA4 };
