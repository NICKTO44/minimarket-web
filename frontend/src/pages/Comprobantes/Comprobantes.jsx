import { useState, useEffect, useRef } from 'react';
import * as pdfjsLib from 'pdfjs-dist/legacy/build/pdf.mjs';
import pdfjsWorker from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import { api } from '../../api/api';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';
import { fechaHoraLima } from '../../utils/formato';
import { descargarArchivo, pdfDeVenta } from '../../utils/pdfTicket';
import './Comprobantes.css';

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfjsWorker;

export default function Comprobantes({ usuario, nombreTienda = 'Mi Minimarket', direccion, telefono, ruc, identificadorNegocio }) {
  const [comprobantes, setComprobantes] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [filtroTipo, setFiltroTipo] = useState('');
  const [filtroEstado, setFiltroEstado] = useState('');
  const [mensaje, setMensaje] = useState(null);
  const [ventaParaImprimir, setVentaParaImprimir] = useState(null);
  // Fila cuyo archivo se está preparando (para no pedirlo dos veces).
  const [exportando, setExportando] = useState(null);
  // Menú "Descargar" abierto: { comp, opciones, estilo }.
  const [menuDescarga, setMenuDescarga] = useState(null);
  // En celular, el XML o el CDR ya listo para compartir o guardar:
  // { archivo, titulo, nota }
  const [archivoListo, setArchivoListo] = useState(null);
  // En celular el PDF se abre aquí dentro (no en el navegador): el archivo
  // que se está viendo, para compartirlo o guardarlo. { archivo, url }
  const [pdfArchivo, setPdfArchivo] = useState(null);

  // --- Visor de PDF embebido (reemplaza al iframe) ---
  const [pdfVisible, setPdfVisible] = useState(null);
  const [enviandoWhatsapp, setEnviandoWhatsapp] = useState(null);
  const [telefonoWhatsapp, setTelefonoWhatsapp] = useState('');
  const [pdfPaginas, setPdfPaginas] = useState([]);
  const [pdfCargando, setPdfCargando] = useState(false);
  const [pdfError, setPdfError] = useState(null);
  const pdfContenedorRef = useRef(null);

  const cargar = () => {
    setCargando(true);
    const filtros = {};
    if (filtroTipo) filtros.tipo = filtroTipo;
    if (filtroEstado) filtros.estado = filtroEstado;
    api
      .comprobantesListar(filtros)
      .then(setComprobantes)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filtroTipo, filtroEstado]);

  // Renderiza cada página del PDF como imagen dentro del modal.
  // No depende del visor nativo del navegador, por eso funciona igual
  // en Android, iPhone y desktop.
  const renderizarPdf = async (url) => {
    setPdfCargando(true);
    setPdfError(null);
    setPdfPaginas([]);
    try {
      const documento = await pdfjsLib.getDocument({ url }).promise;
      const anchoContenedor = pdfContenedorRef.current?.clientWidth || 380;
      const paginasRenderizadas = [];

      for (let numPagina = 1; numPagina <= documento.numPages; numPagina++) {
        const pagina = await documento.getPage(numPagina);
        const viewportBase = pagina.getViewport({ scale: 1 });
        // *2 para que se vea nítido en pantallas retina/alta densidad
        const escala = (anchoContenedor / viewportBase.width) * 2;
        const viewport = pagina.getViewport({ scale: escala });

        const canvas = document.createElement('canvas');
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        const contexto = canvas.getContext('2d');
        await pagina.render({ canvasContext: contexto, viewport }).promise;

        paginasRenderizadas.push(canvas.toDataURL('image/png'));
      }

      setPdfPaginas(paginasRenderizadas);
    } catch (e) {
      console.error('Error renderizando PDF:', e);
      setPdfError('No se pudo cargar la vista previa del comprobante.');
    } finally {
      setPdfCargando(false);
    }
  };

  useEffect(() => {
    if (pdfVisible) {
      renderizarPdf(pdfVisible);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pdfVisible]);

  // El menú de descarga se cierra con Escape o si cambia el tamaño de la
  // ventana (quedaría lejos de su botón).
  useEffect(() => {
    if (!menuDescarga) return undefined;
    const cerrar = () => setMenuDescarga(null);
    const tecla = (e) => e.key === 'Escape' && cerrar();
    window.addEventListener('keydown', tecla);
    window.addEventListener('resize', cerrar);
    return () => {
      window.removeEventListener('keydown', tecla);
      window.removeEventListener('resize', cerrar);
    };
  }, [menuDescarga]);

  const cerrarPdf = () => {
    if (pdfArchivo) URL.revokeObjectURL(pdfArchivo.url);
    setPdfArchivo(null);
    setPdfVisible(null);
    setPdfPaginas([]);
    setPdfError(null);
  };

  // Una venta puede tener boleta y factura: cada una es su propia fila.
  const claveFila = (comp) => `${comp.venta_id}-${comp.id ?? 0}`;

  // Celular o tableta (pantalla táctil o angosta): ahí una descarga directa
  // saca al usuario del sistema y no le dice dónde quedó el archivo.
  const enCelular = () => window.matchMedia('(pointer: coarse), (max-width: 899px)').matches;

  // El PDF ya listo: en computadora se descarga; en celular se abre en esta
  // misma pantalla, con un botón para compartirlo o guardarlo.
  const entregarPdf = (contenido, nombre) => {
    if (!enCelular()) {
      descargarArchivo(contenido, nombre);
      return;
    }
    const archivo = new File([contenido], nombre, { type: 'application/pdf' });
    const url = URL.createObjectURL(archivo);
    setPdfArchivo({ archivo, url });
    setPdfVisible(url);
  };

  // ¿Este celular puede abrir su menú de compartir con un archivo?
  const puedeCompartir = (archivo) => {
    try {
      return !!navigator.canShare && navigator.canShare({ files: [archivo] });
    } catch {
      return false;
    }
  };

  // Menú de compartir del celular: WhatsApp, correo, "Guardar en Archivos"...
  // Se llama directo desde el toque del botón (el celular lo exige).
  // nuevaPestana: solo para el PDF, que el celular abriría encima del sistema.
  const compartirArchivo = async (archivo, nuevaPestana) => {
    if (!puedeCompartir(archivo)) {
      descargarArchivo(archivo, archivo.name, { nuevaPestana });
      return;
    }
    try {
      await navigator.share({ files: [archivo], title: archivo.name });
    } catch (e) {
      // AbortError = la persona cerró el menú sin elegir: no es un error.
      if (e?.name !== 'AbortError') descargarArchivo(archivo, archivo.name, { nuevaPestana });
    }
  };

  const compartirPdf = () => {
    if (pdfArchivo) compartirArchivo(pdfArchivo.archivo, true);
  };

  // Imprime solo las páginas renderizadas (ver regla @media print en
  // Comprobantes.css) — no abre pestaña ni ventana nueva.
  const imprimirPdfEmbebido = () => {
    window.print();
  };

  const abrirEnvioWhatsapp = (comp) => {
    setTelefonoWhatsapp('');
    setEnviandoWhatsapp(comp);
  };

  // Usa SIEMPRE comp.enlace_pdf (el link público real de FacturaLibre) —
  // nunca api.comprobantePdfUrl(), que lleva el token de sesión en la
  // URL y no debe salir de la app.
  const confirmarEnvioWhatsapp = () => {
    if (!enviandoWhatsapp) return;
    const numero = telefonoWhatsapp.replace(/\D/g, '');
    if (numero.length < 9) {
      setMensaje({ tipo: 'error', texto: 'Ingresa un número de WhatsApp válido (9 dígitos).' });
      return;
    }
    const numeroConPais = numero.length === 9 ? `51${numero}` : numero;
    const comp = enviandoWhatsapp;

    let texto;
    if (comp.tipo === 'FACTURA' && comp.id && identificadorNegocio) {
      // Factura: la página pública en formato ticket (80 mm), como la
      // boleta, y debajo el A4 oficial de FacturaLibre si existe.
      const numeroDoc = `${comp.serie}-${String(comp.numero).padStart(6, '0')}`;
      const urlPublica = `${window.location.origin}/boleta/${identificadorNegocio}/${comp.id}`;
      const lineaA4 = comp.enlace_pdf ? `\n\nVersión A4 (PDF): ${comp.enlace_pdf}` : '';
      texto = `Hola! Aquí tienes tu factura ${numeroDoc} por S/ ${comp.monto.toFixed(2)}.\n\nPuedes verla aquí: ${urlPublica}${lineaA4}\n\n¡Gracias por tu compra!`;
    } else if (comp.tipo === 'FACTURA' && comp.enlace_pdf) {
      const numeroDoc = `${comp.serie}-${String(comp.numero).padStart(6, '0')}`;
      texto = `Hola! Aquí tienes tu factura ${numeroDoc} por S/ ${comp.monto.toFixed(2)}.\n\nPuedes verla aquí: ${comp.enlace_pdf}\n\n¡Gracias por tu compra!`;
    } else if (comp.tipo === 'BOLETA' && comp.id && identificadorNegocio) {
      const numeroDoc = `${comp.serie}-${String(comp.numero).padStart(6, '0')}`;
      const urlPublica = `${window.location.origin}/boleta/${identificadorNegocio}/${comp.id}`;
      texto = `Hola! Aquí tienes tu boleta ${numeroDoc} por S/ ${comp.monto.toFixed(2)}.\n\nPuedes verla aquí: ${urlPublica}\n\n¡Gracias por tu compra!`;
    } else {
      texto = `Hola! Gracias por tu compra. Total: S/ ${comp.monto.toFixed(2)} — Venta ${comp.folio_venta}.`;
    }

    window.open(`https://wa.me/${numeroConPais}?text=${encodeURIComponent(texto)}`, '_blank');
    setEnviandoWhatsapp(null);
  };

  // Datos del ticket propio de una venta (nota simple, boleta, o comprobante
  // sin PDF oficial), con los datos reales que ya trae la fila (hash, RUC
  // emisor, fecha) para que el QR salga correcto. Lo usan Imprimir y PDF.
  const datosDelTicket = async (comp) => {
    const detalle = await api.ventaParaDevolucion(comp.folio_venta);
    // Desglose de IGV como se cobró (tasa, gravado, exonerado). Si el
    // servidor aún no lo ofrece, se imprime como siempre (todo al 18 %).
    const igvVenta = await api.igvVenta(comp.venta_id).catch(() => null);

    const tipoDocCliente =
      comp.tipo === 'FACTURA'
        ? '6'
        : comp.cliente_documento
          ? comp.cliente_documento.length === 11
            ? '6'
            : '1'
          : '0';

    // El cliente sale de la fila (la venta no lo detalla). En una factura
    // se busca además su dirección fiscal entre los clientes guardados; si
    // no aparece, el ticket sale sin dirección.
    let cliente = detalle.cliente || null;
    if (!cliente && comp.cliente_nombre) {
      cliente = { nombre_razon_social: comp.cliente_nombre, numero_documento: comp.cliente_documento || null };
      if (comp.tipo === 'FACTURA' && comp.cliente_documento) {
        const guardados = await api.clientesBuscar(comp.cliente_documento).catch(() => []);
        const guardado = Array.isArray(guardados)
          ? guardados.find((c) => c.numero_documento === comp.cliente_documento)
          : null;
        if (guardado?.direccion) cliente.direccion = guardado.direccion;
      }
    }

    return {
      // Fecha y hora de emisión (no la del momento en que se reimprime).
      fecha: fechaHoraLima(comp.fecha_emision),
      // metodoPago: para la "Forma de pago" de la factura (contado o crédito).
      venta: { folio: detalle.folio, total: detalle.total, montoRecibido: null, cambio: null, metodoPago: detalle.metodo_pago },
      items: detalle.productos.map((p) => ({ nombre: p.nombre, cantidad: p.cantidad, precio: p.precio_unitario })),
      comprobante: comp.id
        ? {
            tipo: comp.tipo,
            serie: comp.serie,
            numero: comp.numero,
            estado: comp.estado,
            hash: comp.hash,
            ruc_emisor: comp.ruc_emisor,
            fecha_emision: comp.fecha_emision_corta,
            igv: igvVenta?.igv ?? comp.monto - comp.monto / 1.18,
            total_venta: comp.monto,
            igv_tasa: igvVenta?.igv_tasa,
            op_gravadas: igvVenta?.op_gravadas,
            op_exoneradas: igvVenta?.op_exoneradas,
            op_inafectas: igvVenta?.op_inafectas,
            detraccion_porcentaje: igvVenta?.detraccion_porcentaje,
            detraccion_monto: igvVenta?.detraccion_monto,
            detraccion_cuenta: igvVenta?.detraccion_cuenta,
            cliente_tipo_documento_codigo: tipoDocCliente,
            cliente_numero_documento: comp.cliente_documento || '-',
          }
        : null,
      cliente,
    };
  };

  // Factura que FacturaLibre emitió de verdad: tiene su PDF oficial en A4.
  const facturaConA4 = (comp) => comp.tipo === 'FACTURA' && !!comp.enlace_pdf && !!comp.id;

  // Nuestro ticket de 80 mm (boleta, nota simple, y también la factura).
  const imprimirTicket = async (comp) => {
    try {
      setVentaParaImprimir(await datosDelTicket(comp));
      setTimeout(() => window.print(), 200);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  // El PDF oficial en A4, incrustado en el modal (renderizado con pdf.js).
  const verA4 = (comp) => setPdfVisible(api.comprobantePdfUrl(comp.id));

  const reimprimir = (evento, comp) => {
    setMensaje(null);
    // La factura se puede imprimir en ticket de 80 mm o en A4: se pregunta.
    if (facturaConA4(comp)) {
      abrirMenu(evento, comp, [
        { cual: 'ticket', titulo: 'Ticket 80 mm', detalle: 'Para la impresora térmica, igual que la boleta' },
        { cual: 'a4', titulo: 'Hoja A4', detalle: 'El PDF de FacturaLibre, como hasta ahora' },
      ]);
      return;
    }
    // Boleta, o comprobante sin PDF real (nota simple, o rechazado) —
    // usamos nuestro ticket propio.
    imprimirTicket(comp);
  };

  // Descarga el comprobante como archivo PDF: el mismo documento que sale
  // con "Imprimir". La factura aceptada baja su PDF oficial de FacturaLibre;
  // la boleta y la nota simple, nuestro ticket.
  // enTicket: la factura en nuestro ticket de 80 mm en vez de su A4.
  const exportarPdf = async (comp, enTicket = false) => {
    if (exportando) return;
    setMensaje(null);
    setExportando(claveFila(comp));
    try {
      if (facturaConA4(comp) && !enTicket) {
        const respuesta = await fetch(api.comprobantePdfUrl(comp.id));
        if (!respuesta.ok) throw new Error('No se pudo descargar el PDF de la factura. Intenta de nuevo en un momento.');
        entregarPdf(await respuesta.blob(), `Factura-${comp.serie}-${String(comp.numero).padStart(6, '0')}.pdf`);
        return;
      }
      const datos = await datosDelTicket(comp);
      const { bytes, nombre } = pdfDeVenta({
        ...datos,
        // La fila ya trae al cliente: va en el PDF aunque la venta no lo detalle.
        cliente:
          datos.cliente ||
          (comp.cliente_nombre ? { nombre_razon_social: comp.cliente_nombre, numero_documento: comp.cliente_documento || null } : null),
        nombreTienda,
        direccion,
        telefono,
        ruc,
        cajero: usuario?.nombre || '',
      });
      entregarPdf(bytes, nombre);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setExportando(null);
    }
  };

  // XML firmado o constancia de SUNAT (CDR), tal como los entrega
  // FacturaLibre. Llevan el nombre con el que SUNAT identifica al
  // comprobante: RUC-tipo-serie-número (con "R-" delante en la constancia).
  const descargarOficial = async (comp, cual) => {
    if (exportando) return;
    setMensaje(null);
    setExportando(claveFila(comp));
    try {
      const { blob, extension } = await api.comprobanteArchivo(comp.id, cual);
      const partes = [comp.ruc_emisor, comp.tipo === 'FACTURA' ? '01' : '03', comp.serie, comp.numero].filter(Boolean);
      const nombre = `${cual === 'cdr' ? 'R-' : ''}${partes.join('-')}.${extension}`;
      const tipo = extension === 'zip' ? 'application/zip' : 'application/xml';
      if (!enCelular()) {
        descargarArchivo(blob, nombre, { tipo });
        return;
      }
      setArchivoListo({
        archivo: new File([blob], nombre, { type: tipo }),
        titulo: cual === 'cdr' ? 'Constancia de SUNAT (CDR)' : 'XML del comprobante',
        nota:
          cual === 'cdr'
            ? 'Es la respuesta de SUNAT que confirma que el comprobante fue aceptado.'
            : 'Es el archivo firmado que piden los contadores. Se abre con su sistema contable.',
      });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setExportando(null);
    }
  };

  // Lo que se puede descargar de una fila. El PDF siempre; el XML y la
  // constancia solo de boletas y facturas aceptadas por SUNAT.
  const opcionesDescarga = (comp) => {
    const opciones = facturaConA4(comp)
      ? [
          { cual: 'pdf', titulo: 'PDF A4', detalle: 'El de FacturaLibre, para imprimir o enviar' },
          { cual: 'pdf80', titulo: 'PDF ticket 80 mm', detalle: 'La misma factura en formato ticket' },
        ]
      : [
          {
            cual: 'pdf',
            titulo: 'PDF',
            detalle: comp.tipo === 'NINGUNO' ? 'La nota de venta' : 'Para imprimir o enviar al cliente',
          },
        ];
    // tiene_xml llega con el servidor que ya ofrece estas descargas.
    if (comp.id && comp.estado === 'ACEPTADO' && comp.tiene_xml !== undefined) {
      opciones.push({
        cual: 'xml',
        titulo: 'XML',
        detalle: comp.tiene_xml ? 'Archivo firmado, para el contador' : 'No disponible en este comprobante',
        falta: !comp.tiene_xml,
      });
      opciones.push({
        cual: 'cdr',
        titulo: 'CDR',
        detalle: comp.tiene_cdr ? 'Constancia de aceptación de SUNAT' : 'SUNAT todavía no la entrega',
        falta: !comp.tiene_cdr,
      });
    }
    return opciones;
  };

  // "Descargar": con una sola opción baja el PDF de frente; con varias abre
  // el menú pegado al botón (en celular, una hoja desde abajo).
  const abrirDescarga = (evento, comp) => {
    const opciones = opcionesDescarga(comp);
    if (opciones.length === 1) {
      exportarPdf(comp);
      return;
    }
    abrirMenu(evento, comp, opciones);
  };

  // Menú de opciones pegado al botón que se tocó (lo usan Descargar e
  // Imprimir de una factura).
  const abrirMenu = (evento, comp, opciones) => {
    const boton = evento.currentTarget.getBoundingClientRect();
    const alto = 58 * opciones.length + 20;
    const haciaArriba = boton.bottom + alto + 8 > window.innerHeight && boton.top > alto + 8;
    setMenuDescarga({
      comp,
      opciones,
      estilo: {
        right: Math.max(8, window.innerWidth - boton.right),
        ...(haciaArriba ? { bottom: window.innerHeight - boton.top + 6 } : { top: boton.bottom + 6 }),
      },
    });
  };

  const elegirDescarga = (cual) => {
    const comp = menuDescarga.comp;
    setMenuDescarga(null);
    if (cual === 'pdf') exportarPdf(comp);
    else if (cual === 'pdf80') exportarPdf(comp, true);
    else if (cual === 'ticket') imprimirTicket(comp);
    else if (cual === 'a4') verA4(comp);
    else descargarOficial(comp, cual);
  };

  return (
    <div className="comp-layout">
      <h1>Comprobantes</h1>
      <p className="comp-subtitulo">Historial de boletas y facturas electrónicas emitidas.</p>

      <div className="comp-filtros">
        <select value={filtroTipo} onChange={(e) => setFiltroTipo(e.target.value)}>
          <option value="">Todos los tipos</option>
          <option value="BOLETA">Boleta</option>
          <option value="FACTURA">Factura</option>
          <option value="NINGUNO">Nota simple</option>
        </select>
        <select value={filtroEstado} onChange={(e) => setFiltroEstado(e.target.value)}>
          <option value="">Todos los estados</option>
          <option value="ACEPTADO">Aceptado</option>
          <option value="RECHAZADO">Rechazado</option>
          <option value="PENDIENTE">Pendiente</option>
        </select>
      </div>

      {mensaje && <p className={`comp-mensaje comp-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {cargando ? (
        <p className="comp-cargando">Cargando...</p>
      ) : (
        <div className="comp-tabla-wrapper">
          <table className="comp-tabla">
            <thead>
              <tr>
                <th>Comprobante</th>
                <th>Venta</th>
                <th>Cliente</th>
                <th>Monto</th>
                <th>Estado</th>
                <th>Fecha</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {comprobantes.map((c) => (
                <tr key={claveFila(c)}>
                  <td>
                    {c.tipo === 'NINGUNO' ? (
                      <span className="comp-tipo comp-tipo-ninguno">Nota simple</span>
                    ) : (
                      <>
                        <span className="comp-tipo">{c.tipo === 'FACTURA' ? 'Factura' : 'Boleta'}</span>{' '}
                        {c.serie}-{String(c.numero).padStart(6, '0')}
                      </>
                    )}
                  </td>
                  <td className="comp-folio-venta">{c.folio_venta}</td>
                  <td>{c.cliente_nombre || '—'}</td>
                  <td>S/ {c.monto.toFixed(2)}</td>
                  <td>
                    {c.estado ? (
                      <>
                        <span className={`comp-badge comp-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                        {c.mensaje_sunat && <p className="comp-motivo">{c.mensaje_sunat}</p>}
                      </>
                    ) : (
                      <span className="comp-badge comp-badge-ninguno">Sin comprobante</span>
                    )}
                  </td>
                  <td>{fechaHoraLima(c.fecha_emision)}</td>
                  <td>
                    <button
                      className="comp-boton-imprimir"
                      onClick={(e) => reimprimir(e, c)}
                      aria-haspopup={facturaConA4(c) ? 'menu' : undefined}
                    >
                      🖨 Imprimir{facturaConA4(c) ? ' ▾' : ''}
                    </button>
                    <button
                      className="comp-boton-imprimir comp-boton-pdf"
                      onClick={(e) => abrirDescarga(e, c)}
                      disabled={exportando === claveFila(c)}
                      aria-label={`Descargar la venta ${c.folio_venta}`}
                      aria-haspopup={opcionesDescarga(c).length > 1 ? 'menu' : undefined}
                    >
                      {exportando === claveFila(c)
                        ? 'Preparando…'
                        : `⬇ Descargar${opcionesDescarga(c).length > 1 ? ' ▾' : ''}`}
                    </button>
                    <button className="comp-boton-whatsapp" onClick={() => abrirEnvioWhatsapp(c)}>
                      📲
                    </button>
                  </td>
                </tr>
              ))}
              {comprobantes.length === 0 && (
                <tr>
                  <td colSpan={7} className="comp-sin-resultados">
                    No hay comprobantes que coincidan.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {menuDescarga && (
        <div className="comp-descarga-fondo" onClick={() => setMenuDescarga(null)}>
          <div className="comp-descarga-menu" role="menu" style={menuDescarga.estilo} onClick={(e) => e.stopPropagation()}>
            <p className="comp-descarga-titulo">
              {menuDescarga.comp.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} {menuDescarga.comp.serie}-
              {String(menuDescarga.comp.numero).padStart(6, '0')}
            </p>
            {menuDescarga.opciones.map((o) => (
              <button
                key={o.cual}
                type="button"
                role="menuitem"
                className="comp-descarga-opcion"
                disabled={o.falta}
                onClick={() => elegirDescarga(o.cual)}
              >
                <strong>{o.titulo}</strong>
                <span>{o.detalle}</span>
              </button>
            ))}
            <button type="button" className="comp-descarga-cancelar" onClick={() => setMenuDescarga(null)}>
              Cancelar
            </button>
          </div>
        </div>
      )}

      {archivoListo && (
        <div className="comp-whatsapp-modal-overlay" onClick={() => setArchivoListo(null)}>
          <div className="comp-whatsapp-modal comp-archivo-listo" onClick={(e) => e.stopPropagation()}>
            <h2>{archivoListo.titulo}</h2>
            <p className="comp-archivo-nombre">{archivoListo.archivo.name}</p>
            <p className="comp-whatsapp-modal-nota">{archivoListo.nota}</p>
            <div className="comp-whatsapp-modal-acciones">
              <button className="comp-whatsapp-modal-cancelar" onClick={() => setArchivoListo(null)}>
                Cerrar
              </button>
              <button className="comp-pdf-modal-imprimir" onClick={() => compartirArchivo(archivoListo.archivo, false)}>
                {puedeCompartir(archivoListo.archivo) ? '📤 Compartir o guardar' : '⬇ Guardar en el celular'}
              </button>
            </div>
          </div>
        </div>
      )}

      {pdfVisible && (
        <div className="comp-pdf-modal-overlay">
          <div className="comp-pdf-modal">
            <div className="comp-pdf-modal-header comp-no-imprimir">
              <h2>{pdfArchivo ? pdfArchivo.archivo.name : 'Comprobante'}</h2>
              <button
                type="button"
                className="comp-pdf-modal-x"
                onClick={cerrarPdf}
                aria-label="Cerrar"
              >
                ×
              </button>
            </div>

            <div className="comp-pdf-paginas" ref={pdfContenedorRef}>
              {pdfCargando && <p className="comp-pdf-cargando comp-no-imprimir">Cargando vista previa...</p>}

              {pdfError && pdfArchivo && (
                <div className="comp-pdf-error comp-no-imprimir">
                  <p>
                    Tu PDF está listo, pero este celular no puede mostrar la vista previa. Toca «
                    {puedeCompartir(pdfArchivo.archivo) ? 'Compartir o guardar' : 'Descargar'}» para enviarlo o guardarlo.
                  </p>
                </div>
              )}

              {pdfError && !pdfArchivo && (
                <div className="comp-pdf-error comp-no-imprimir">
                  <p>{pdfError}</p>
                  <a href={pdfVisible} target="_blank" rel="noopener noreferrer">
                    Abrir el comprobante en una pestaña aparte
                  </a>
                </div>
              )}

              {!pdfCargando &&
                !pdfError &&
                pdfPaginas.map((imagenPagina, indice) => (
                  <img
                    key={indice}
                    src={imagenPagina}
                    alt={`Página ${indice + 1} del comprobante`}
                    className="comp-pdf-pagina-img"
                  />
                ))}
            </div>

            <div className="comp-pdf-modal-acciones comp-no-imprimir">
              <button className="comp-pdf-modal-cerrar" onClick={cerrarPdf}>
                Cerrar
              </button>
              {pdfArchivo ? (
                <button className="comp-pdf-modal-imprimir" onClick={compartirPdf}>
                  {puedeCompartir(pdfArchivo.archivo) ? '📤 Compartir o guardar' : '⬇ Descargar'}
                </button>
              ) : (
                <button
                  className="comp-pdf-modal-imprimir"
                  onClick={imprimirPdfEmbebido}
                  disabled={pdfCargando || !!pdfError || pdfPaginas.length === 0}
                >
                  🖨 Imprimir
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {ventaParaImprimir && (
        <Recibo
          venta={ventaParaImprimir.venta}
          items={ventaParaImprimir.items}
          comprobante={ventaParaImprimir.comprobante}
          cliente={ventaParaImprimir.cliente}
          nombreTienda={nombreTienda}
          direccion={direccion}
          telefono={telefono}
          ruc={ruc}
          cajero={usuario?.nombre || ''}
          fecha={ventaParaImprimir.fecha}
        />
      )}

      {enviandoWhatsapp && (
        <div className="comp-whatsapp-modal-overlay" onClick={() => setEnviandoWhatsapp(null)}>
          <div className="comp-whatsapp-modal" onClick={(e) => e.stopPropagation()}>
            <h2>Enviar por WhatsApp</h2>
            <p className="comp-whatsapp-modal-nota">
              {enviandoWhatsapp.tipo === 'FACTURA' ? 'Factura' : enviandoWhatsapp.tipo === 'NINGUNO' ? 'Venta' : 'Boleta'}{' '}
              {enviandoWhatsapp.serie && `${enviandoWhatsapp.serie}-${String(enviandoWhatsapp.numero).padStart(6, '0')}`}
              {' — S/ '}
              {enviandoWhatsapp.monto.toFixed(2)}
            </p>
            <input
              type="tel"
              placeholder="Número de WhatsApp del cliente"
              value={telefonoWhatsapp}
              onChange={(e) => setTelefonoWhatsapp(e.target.value)}
              autoFocus
            />
            <div className="comp-whatsapp-modal-acciones">
              <button className="comp-whatsapp-modal-cancelar" onClick={() => setEnviandoWhatsapp(null)}>
                Cancelar
              </button>
              <button className="comp-whatsapp-modal-enviar" onClick={confirmarEnvioWhatsapp}>
                📲 Enviar
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}