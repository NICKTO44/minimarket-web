import { useState, useEffect, useRef, useCallback } from 'react';
import * as pdfjsLib from 'pdfjs-dist/legacy/build/pdf.mjs';
import pdfjsWorker from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import { api, API_URL } from '../../api/api';
import { datosA4DeDocumento, imprimirComprobanteA4 } from '../../utils/facturaA4';
import Recibo from '../../components/Recibo';
import FormularioNotaCredito from '../../components/FormularioNotaCredito';
import FormularioAnulacion from '../../components/FormularioAnulacion';
import { imprimirNotaCredito } from '../../utils/notaCredito';
import '../../components/Recibo.css';
import { fechaHoraLima, hoyLima } from '../../utils/formato';
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
  // Configuración del negocio: razón social, logo y color para el A4.
  const [cfgEmisor, setCfgEmisor] = useState(null);
  // Emisión directa: lo que todavía no llega a SUNAT y la fila que se está
  // reenviando.
  const [avisos, setAvisos] = useState(null);
  const [reenviando, setReenviando] = useState(null);
  // Comprobante para el que se está emitiendo una nota de crédito.
  const [notaPara, setNotaPara] = useState(null);
  const cerrarNota = useCallback(() => setNotaPara(null), []);
  // Comprobante que se está anulando.
  const [anularPara, setAnularPara] = useState(null);
  const cerrarAnular = useCallback(() => setAnularPara(null), []);

  useEffect(() => {
    api
      .configuracionObtener()
      .then(setCfgEmisor)
      .catch(() => setCfgEmisor(null));
  }, []);

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
    api
      .sunatAvisos()
      .then(setAvisos)
      .catch(() => setAvisos(null));
  };

  // Fila o nota que se reenvía: { clave, nombre, enviar }.
  const reenviarAlgo = async (clave, nombre, enviar) => {
    setReenviando(clave);
    setMensaje(null);
    try {
      const r = await enviar();
      setMensaje(
        r.estado === 'ACEPTADO'
          ? { tipo: 'ok', texto: `${nombre}: aceptado por SUNAT.` }
          : { tipo: r.estado === 'PENDIENTE' ? 'aviso' : 'error', texto: `${nombre}: ${r.mensaje}` }
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setReenviando(null);
      cargar();
    }
  };
  const reenviar = (comp) => reenviarAlgo(comp.id, `${comp.serie}-${comp.numero}`, () => api.comprobanteReenviar(comp.id));
  const reenviarNota = (n) => reenviarAlgo(`nc-${n.id}`, `Nota ${n.serie}-${n.numero}`, () => api.notaCreditoReenviar(n.id));

  const consultarAnulacion = (c) =>
    reenviarAlgo(`an-${c.id}`, `Anulación de ${c.serie}-${c.numero}`, async () => {
      const r = await api.comprobanteAnulacionConsultar(c.id);
      const estado = r.anulacion === 'ANULADO' ? 'ACEPTADO' : r.anulacion === 'EN_PROCESO' ? 'PENDIENTE' : 'RECHAZADO';
      return { estado, mensaje: r.mensaje };
    });

  const imprimirNota = (n, formato) => {
    setMensaje(null);
    imprimirNotaCredito(n, formato, cfgEmisor, telefono).catch((e) =>
      setMensaje({ tipo: 'error', texto: `No se pudo preparar la nota: ${e.message}` })
    );
  };

  const descargarNota = async (n, cual) => {
    setMensaje(null);
    try {
      const { blob, extension } = await api.notaCreditoArchivo(n.id, cual);
      const nombre = `${cual === 'cdr' ? 'R-' : ''}${[cfgEmisor?.ruc || ruc, '07', n.serie, n.numero].filter(Boolean).join('-')}.${extension}`;
      descargarArchivo(blob, nombre, { tipo: extension === 'zip' ? 'application/zip' : 'application/xml' });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
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

    // Sin documento: la emisión directa envía "-" a SUNAT (catálogo 06,
    // ventas menores); el QR debe llevar lo mismo que el XML.
    const tipoDocCliente =
      comp.tipo === 'FACTURA'
        ? '6'
        : comp.cliente_documento
          ? comp.cliente_documento.length === 11
            ? '6'
            : '1'
          : comp.proveedor === 'SUNAT_DIRECTO'
            ? '-'
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

  // Factura con hoja A4: la de FacturaLibre (su PDF oficial) o la emitida
  // directo a SUNAT y aceptada (nuestro diseño, utils/facturaA4.js).
  const a4Propio = (comp) => comp.proveedor === 'SUNAT_DIRECTO' && comp.estado === 'ACEPTADO' && !comp.enlace_pdf;
  const facturaConA4 = (comp) => comp.tipo === 'FACTURA' && !!comp.id && (!!comp.enlace_pdf || a4Propio(comp));

  // Nuestro A4, armado con el documento que se envió a SUNAT.
  const imprimirA4Propio = async (comp) => {
    try {
      const documento = await api.comprobanteDocumento(comp.id);
      const datos = await datosA4DeDocumento(documento, {
        hash: comp.hash,
        telefono: cfgEmisor?.telefono || telefono,
        email: cfgEmisor?.email,
        logo: cfgEmisor?.logo_path ? `${API_URL}${cfgEmisor.logo_path}` : null,
        color: cfgEmisor?.color_acento,
      });
      imprimirComprobanteA4(datos);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: `No se pudo preparar la hoja A4: ${e.message}` });
    }
  };

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
  const verA4 = (comp) => (comp.enlace_pdf ? setPdfVisible(api.comprobantePdfUrl(comp.id)) : imprimirA4Propio(comp));

  const reimprimir = (evento, comp) => {
    setMensaje(null);
    // La factura se puede imprimir en ticket de 80 mm o en A4: se pregunta.
    if (facturaConA4(comp)) {
      abrirMenu(evento, comp, [
        { cual: 'ticket', titulo: 'Ticket 80 mm', detalle: 'Para la impresora térmica, igual que la boleta' },
        {
          cual: 'a4',
          titulo: 'Hoja A4',
          detalle: comp.enlace_pdf ? 'El PDF de FacturaLibre, como hasta ahora' : 'Con el logo del negocio',
        },
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
      if (facturaConA4(comp) && comp.enlace_pdf && !enTicket) {
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
    const opciones = a4Propio(comp) && comp.tipo === 'FACTURA'
      ? [
          { cual: 'a4', titulo: 'PDF A4', detalle: 'Se abre para imprimir: elige "Guardar como PDF"' },
          { cual: 'pdf80', titulo: 'PDF ticket 80 mm', detalle: 'La misma factura en formato ticket' },
        ]
      : facturaConA4(comp)
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

      {avisos?.pendientes > 0 && <AvisoSunat avisos={avisos} />}

      {anularPara && <FormularioAnulacion comprobante={anularPara} onHecho={() => cargar()} onCerrar={cerrarAnular} />}

      {notaPara && (
        <FormularioNotaCredito
          comprobante={notaPara}
          onEmitida={() => cargar()}
          onCerrar={cerrarNota}
        />
      )}

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
                    {c.notas?.map((n) => (
                      <div key={n.id} className="comp-nota">
                        <span className="comp-tipo comp-tipo-nota">Nota de crédito</span> {n.serie}-{String(n.numero).padStart(6, '0')}
                        <span className={`comp-badge comp-badge-${n.estado.toLowerCase()}`}>{n.estado}</span>
                        <span className="comp-nota-monto">− S/ {n.total.toFixed(2)}</span>
                        <span className="comp-nota-motivo">{n.motivo}</span>
                        {n.estado !== 'ACEPTADO' && n.mensaje_sunat && <span className="comp-motivo">{n.mensaje_sunat}</span>}
                        <span className="comp-nota-acciones">
                          {n.estado === 'ACEPTADO' && (
                            <>
                              <button type="button" onClick={() => imprimirNota(n, 'ticket')}>
                                🖨 Ticket
                              </button>
                              <button type="button" onClick={() => imprimirNota(n, 'a4')}>
                                A4
                              </button>
                              {n.tiene_xml && (
                                <button type="button" onClick={() => descargarNota(n, 'xml')}>
                                  XML
                                </button>
                              )}
                              {n.tiene_cdr && (
                                <button type="button" onClick={() => descargarNota(n, 'cdr')}>
                                  CDR
                                </button>
                              )}
                            </>
                          )}
                          {n.estado === 'PENDIENTE' && (
                            <button type="button" onClick={() => reenviarNota(n)} disabled={reenviando === `nc-${n.id}`}>
                              {reenviando === `nc-${n.id}` ? 'Enviando…' : '↻ Reenviar'}
                            </button>
                          )}
                        </span>
                      </div>
                    ))}
                  </td>
                  <td className="comp-folio-venta">{c.folio_venta}</td>
                  <td>{c.cliente_nombre || '—'}</td>
                  <td>S/ {c.monto.toFixed(2)}</td>
                  <td>
                    {c.estado ? (
                      <>
                        <span className="comp-badges">
                          <span className={`comp-badge comp-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                          {c.anulacion && (
                            <span className={`comp-badge comp-badge-anulacion-${c.anulacion.toLowerCase()}`}>
                              {
                                {
                                  ANULADO: 'ANULADO',
                                  EN_PROCESO: 'ANULANDO…',
                                  VERIFICAR: 'REVISAR ANULACIÓN EN SUNAT',
                                }[c.anulacion] || 'BAJA RECHAZADA'
                              }
                            </span>
                          )}
                        </span>
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
                    {c.proveedor === 'SUNAT_DIRECTO' && c.id && ['ACEPTADO', 'PENDIENTE'].includes(c.estado) && (
                      <div className="comp-acciones-sunat">
                        {c.estado === 'ACEPTADO' && vigente(c) && (
                          <button className="comp-boton-imprimir comp-boton-nota" onClick={() => setNotaPara(c)}>
                            Nota de crédito
                          </button>
                        )}
                        {puedeAnular(c) && (
                          <button className="comp-boton-imprimir comp-boton-anular" onClick={() => setAnularPara(c)}>
                            Anular
                          </button>
                        )}
                        {c.anulacion === 'EN_PROCESO' && (
                          <button
                            className="comp-boton-imprimir comp-boton-reenviar"
                            onClick={() => consultarAnulacion(c)}
                            disabled={reenviando === `an-${c.id}`}
                          >
                            {reenviando === `an-${c.id}` ? 'Consultando…' : '↻ Consultar anulación'}
                          </button>
                        )}
                        {c.estado === 'PENDIENTE' && (
                          <button
                            className="comp-boton-imprimir comp-boton-reenviar"
                            onClick={() => reenviar(c)}
                            disabled={reenviando === c.id}
                          >
                            {reenviando === c.id ? 'Enviando…' : '↻ Reenviar'}
                          </button>
                        )}
                      </div>
                    )}
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
          razonSocial={cfgEmisor?.razon_social}
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

/** Aviso de comprobantes que todavía no llegan a SUNAT (emisión directa). */
function AvisoSunat({ avisos }) {
  const urgente = avisos.avisos[0];
  const vencido = urgente && urgente.dias_restantes < 0;
  const plazo = !urgente
    ? ''
    : vencido
      ? `${urgente.documento} ya pasó el plazo de envío de SUNAT.`
      : urgente.dias_restantes === 0
        ? `${urgente.documento} vence hoy.`
        : `Al más antiguo (${urgente.documento}) le ${urgente.dias_restantes === 1 ? 'queda 1 día' : `quedan ${urgente.dias_restantes} días`} de plazo.`;
  return (
    <div className={`comp-aviso-sunat${vencido || urgente?.dias_restantes <= 1 ? ' comp-aviso-urgente' : ''}`} role="status">
      <strong>
        {avisos.pendientes === 1
          ? '1 comprobante todavía no llega a SUNAT.'
          : `${avisos.pendientes} comprobantes todavía no llegan a SUNAT.`}
      </strong>{' '}
      El sistema los reenvía solo cada 10 minutos; también puedes usar «Reenviar». {plazo}
    </div>
  );
}

/** El comprobante sigue vigente ante SUNAT (no anulado ni anulándose). */
function vigente(c) {
  return !c.anulacion || c.anulacion === 'RECHAZADA';
}

/** Días calendario entre dos fechas "AAAA-MM-DD". */
function diasEntre(desde, hasta) {
  const d = Date.parse(`${String(desde).slice(0, 10)}T00:00:00Z`);
  const h = Date.parse(`${String(hasta).slice(0, 10)}T00:00:00Z`);
  return Number.isNaN(d) || Number.isNaN(h) ? 0 : Math.round((h - d) / 86400000);
}

/**
 * Se puede anular (el servidor lo vuelve a revisar): emitido directo y
 * aceptado, vigente, sin notas de crédito y dentro de los 7 días.
 */
function puedeAnular(c) {
  return (
    c.proveedor === 'SUNAT_DIRECTO' &&
    c.estado === 'ACEPTADO' &&
    vigente(c) &&
    !(c.notas || []).some((n) => n.estado === 'ACEPTADO' || n.estado === 'PENDIENTE') &&
    diasEntre(c.fecha_emision_corta, hoyLima()) <= 7
  );
}
