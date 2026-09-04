import { useState, useEffect, useRef } from 'react';
import * as pdfjsLib from 'pdfjs-dist';
import pdfjsWorker from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { api } from '../../api/api';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';
import './Comprobantes.css';

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfjsWorker;

export default function Comprobantes({ usuario, nombreTienda = 'Mi Minimarket' }) {
  const [comprobantes, setComprobantes] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [filtroTipo, setFiltroTipo] = useState('');
  const [filtroEstado, setFiltroEstado] = useState('');
  const [mensaje, setMensaje] = useState(null);
  const [ventaParaImprimir, setVentaParaImprimir] = useState(null);

  // --- Visor de PDF embebido (reemplaza al iframe) ---
  const [pdfVisible, setPdfVisible] = useState(null);
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

  const cerrarPdf = () => {
    setPdfVisible(null);
    setPdfPaginas([]);
    setPdfError(null);
  };

  // Imprime solo las páginas renderizadas (ver regla @media print en
  // Comprobantes.css) — no abre pestaña ni ventana nueva.
  const imprimirPdfEmbebido = () => {
    window.print();
  };

  const reimprimir = async (comp) => {
    setMensaje(null);

    // Si FacturaLibre emitió de verdad el comprobante (aceptado, con su
    // propio PDF oficial con logo/QR), lo mostramos incrustado en el
    // modal, renderizado con pdf.js.
    if (comp.enlace_pdf && comp.id) {
      setPdfVisible(api.comprobantePdfUrl(comp.id));
      return;
    }

    // Sin PDF real (nota simple, o comprobante rechazado sin documento
    // válido) — usamos nuestro ticket propio como respaldo.
    try {
      const detalle = await api.ventaParaDevolucion(comp.folio_venta);
      setVentaParaImprimir({
        venta: { folio: detalle.folio, total: detalle.total, montoRecibido: null, cambio: null },
        items: detalle.productos.map((p) => ({ nombre: p.nombre, cantidad: p.cantidad, precio: p.precio_unitario })),
        comprobante: detalle.comprobante,
        cliente: detalle.cliente || null,
      });
      setTimeout(() => window.print(), 200);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
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
                <tr key={c.venta_id}>
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
                  <td>{new Date(c.fecha_emision).toLocaleString('es-PE')}</td>
                  <td>
                    <button className="comp-boton-imprimir" onClick={() => reimprimir(c)}>
                      🖨 Imprimir
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

      {pdfVisible && (
        <div className="comp-pdf-modal-overlay">
          <div className="comp-pdf-modal">
            <div className="comp-pdf-modal-header comp-no-imprimir">
              <h2>Comprobante</h2>
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

              {pdfError && (
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
              <button
                className="comp-pdf-modal-imprimir"
                onClick={imprimirPdfEmbebido}
                disabled={pdfCargando || !!pdfError || pdfPaginas.length === 0}
              >
                🖨 Imprimir
              </button>
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
          cajero={usuario?.nombre || ''}
        />
      )}
    </div>
  );
}