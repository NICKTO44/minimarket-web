import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import * as pdfjsLib from 'pdfjs-dist';
import pdfjsWorker from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { api, API_URL } from '../../api/api';
import './POS.css';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';
import EscanerCodigoBarras from '../../components/EscanerCodigoBarras';

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfjsWorker;

const DEBOUNCE_BUSQUEDA_VIVA_MS = 400;
const DURACION_MENSAJE_ESCANEO_MS = 2500;

const REGLAS_DOCUMENTO = {
  DNI: { maxLength: 8, soloNumeros: true, label: 'DNI (8 dígitos)' },
  CE: { maxLength: 12, soloNumeros: false, label: 'Carnet de Extranjería' },
  PASAPORTE: { maxLength: 12, soloNumeros: false, label: 'Pasaporte' },
  RUC: { maxLength: 11, soloNumeros: true, label: 'RUC (11 dígitos)' },
};

export default function POS({ usuario, nombreTienda = 'Mi Minimarket', direccion, telefono, ruc, identificadorNegocio }) {
  // El buscador solo se enfoca solo en desktop -- en celular, hacerlo
  // abre el teclado apenas se entra a la pantalla y tapa la grilla de
  // productos antes de que el usuario haya tocado nada. Se calcula una
  // sola vez al montar (no reactivo a resize), que es lo esperado para
  // un autoFocus: decide el comportamiento inicial de la pantalla.
  const [autoFocoBuscador] = useState(
    () => typeof window !== 'undefined' && window.matchMedia('(min-width: 900px)').matches
  );
  const [productos, setProductos] = useState([]);
  const [imagenesFallidas, setImagenesFallidas] = useState(() => new Set());
  const [busqueda, setBusqueda] = useState('');
  const buscadorRef = useRef(null);
  const [carrito, setCarrito] = useState([]);
  const [carritoAbierto, setCarritoAbierto] = useState(false);
  const [metodoPago, setMetodoPago] = useState('EFECTIVO');
  const [montoRecibido, setMontoRecibido] = useState('');
  const [cliente, setCliente] = useState(null);
  const [mostrarBusquedaCliente, setMostrarBusquedaCliente] = useState(false);
  const [busquedaCliente, setBusquedaCliente] = useState('');
  const [resultadosCliente, setResultadosCliente] = useState([]);
  const [buscandoCliente, setBuscandoCliente] = useState(false);
  const [sinResultadosCliente, setSinResultadosCliente] = useState(false);
  const [tipoComprobante, setTipoComprobante] = useState('BOLETA');
  const [procesando, setProcesando] = useState(false);
  const [mensaje, setMensaje] = useState(null);
  const [ultimaVentaParaImprimir, setUltimaVentaParaImprimir] = useState(null);
  const [mostrarModalVenta, setMostrarModalVenta] = useState(false);
  const [telefonoWhatsapp, setTelefonoWhatsapp] = useState('');
  const [pdfVisible, setPdfVisible] = useState(null);
  const [pdfPaginas, setPdfPaginas] = useState([]);
  const [pdfCargando, setPdfCargando] = useState(false);
  const [pdfError, setPdfError] = useState(null);
  const pdfContenedorRef = useRef(null);

  // Se consulta una sola vez al entrar al POS si FacturaLibre está
  // configurado -- así, si el cajero elige Boleta/Factura sin token
  // configurado, se avisa ANTES de crear la venta, en vez de crearla
  // igual y descubrir la falla recién al intentar emitir el
  // comprobante (lo que antes dejaba la venta ya registrada y el
  // carrito vacío, con riesgo de duplicar la venta si se reintentaba).
  const [facturacionConfigurada, setFacturacionConfigurada] = useState(true);

  useEffect(() => {
    api
      .configuracionObtener()
      .then((cfg) => {
        const listo = !!(cfg.facturalibre_token?.trim() && cfg.facturalibre_ruta?.trim());
        setFacturacionConfigurada(listo);
      })
      .catch(() => {
        // Si falla la consulta, se asume que sí está configurado --
        // no se quiere bloquear ventas por un problema de red al
        // cargar la pantalla; el chequeo real de todas formas ocurre
        // en procesarVenta antes de crear la venta.
        setFacturacionConfigurada(true);
      });
  }, []);

  const [nuevoTipoDocumento, setNuevoTipoDocumento] = useState('DNI');
  const [nuevoDocumento, setNuevoDocumento] = useState('');
  const [nuevoNombre, setNuevoNombre] = useState('');
  const [creandoCliente, setCreandoCliente] = useState(false);
  const [errorCliente, setErrorCliente] = useState('');
  const [consultandoDocumento, setConsultandoDocumento] = useState(false);
  const [nombreAutocompletado, setNombreAutocompletado] = useState(false);
  // --- Escáner de código de barras por cámara ---
  const [escanerAbierto, setEscanerAbierto] = useState(false);
  const [ultimoEscaneo, setUltimoEscaneo] = useState(null);

  useEffect(() => {
    api.productos().then(setProductos).catch((e) => setMensaje({ tipo: 'error', texto: e.message }));
  }, []);

  useEffect(() => {
    const texto = busquedaCliente.trim();
    if (texto.length < 2) {
      setResultadosCliente([]);
      return;
    }
    const timeout = setTimeout(() => {
      api
        .clientesBuscar(texto)
        .then(setResultadosCliente)
        .catch(() => {});
    }, DEBOUNCE_BUSQUEDA_VIVA_MS);
    return () => clearTimeout(timeout);
  }, [busquedaCliente]);

  const confirmarBusquedaCliente = useCallback(
    async (texto) => {
      const limpio0 = texto.trim();
      if (limpio0.length < 2) return;

      setBuscandoCliente(true);
      try {
        const resultados = await api.clientesBuscar(limpio0);
        setResultadosCliente(resultados);
        setSinResultadosCliente(resultados.length === 0);

        if (resultados.length === 0) {
          const soloDigitos = /^\d+$/.test(limpio0);
          let tipoSugerido = 'DNI';
          if (tipoComprobante === 'BOLETA') {
            tipoSugerido = soloDigitos && limpio0.length <= 8 ? 'DNI' : 'CE';
            setNuevoTipoDocumento(tipoSugerido);
          }
          const regla = REGLAS_DOCUMENTO[tipoComprobante === 'FACTURA' ? 'RUC' : tipoSugerido];
          let limpio = limpio0;
          if (regla.soloNumeros) limpio = limpio.replace(/\D/g, '');
          setNuevoDocumento(limpio.slice(0, regla.maxLength));
        }
      } catch {
        // silencioso
      } finally {
        setBuscandoCliente(false);
      }
    },
    [tipoComprobante]
  );

  useEffect(() => {
    setSinResultadosCliente(false);
  }, [busquedaCliente]);

  const manejarEnterBusquedaCliente = (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirmarBusquedaCliente(busquedaCliente);
    }
  };

  const cambiarTipoComprobante = (tipo) => {
    setTipoComprobante(tipo);
    setCliente(null);
    setMostrarBusquedaCliente(false);
    setBusquedaCliente('');
    setResultadosCliente([]);
    setSinResultadosCliente(false);
    setNuevoTipoDocumento('DNI');
    setNuevoDocumento('');
    setNuevoNombre('');
    setErrorCliente('');
  };

  const seleccionarCliente = (c) => {
    setCliente(c);
    setBusquedaCliente('');
    setResultadosCliente([]);
    setSinResultadosCliente(false);
    setNuevoDocumento('');
    setNuevoNombre('');
  };

  const quitarCliente = () => setCliente(null);

  const tipoDocumentoParaNuevo = tipoComprobante === 'FACTURA' ? 'RUC' : nuevoTipoDocumento;
  const reglaDocumento = REGLAS_DOCUMENTO[tipoDocumentoParaNuevo];

  // Autocompleta el nombre real (RENIEC/SUNAT) apenas el documento
  // alcanza su largo completo. Nunca bloquea ni marca error si falla
  // -- api.documentoConsultar ya devuelve existe:null en cualquier
  // problema (sin token, timeout, tipo no soportado como CE/PASAPORTE),
  // y aquí simplemente no se autocompleta nada en ese caso.
  useEffect(() => {
    setNombreAutocompletado(false);

    const documentoCompleto =
      (tipoDocumentoParaNuevo === 'DNI' || tipoDocumentoParaNuevo === 'RUC') &&
      nuevoDocumento.length === reglaDocumento.maxLength;

    if (!documentoCompleto) return;

    let cancelado = false;
    setConsultandoDocumento(true);

    api
      .documentoConsultar(tipoDocumentoParaNuevo, nuevoDocumento)
      .then((resultado) => {
        if (cancelado) return;
        if (resultado.existe === true && resultado.nombre) {
          setNuevoNombre(resultado.nombre);
          setNombreAutocompletado(true);
        }
      })
      .catch(() => {
        // Silencioso a propósito -- ver nota arriba.
      })
      .finally(() => {
        if (!cancelado) setConsultandoDocumento(false);
      });

    return () => {
      cancelado = true;
    };
  }, [nuevoDocumento, tipoDocumentoParaNuevo, reglaDocumento.maxLength]);

  const manejarCambioDocumento = (valor) => {
    let limpio = valor;
    if (reglaDocumento.soloNumeros) limpio = limpio.replace(/\D/g, '');
    setNuevoDocumento(limpio.slice(0, reglaDocumento.maxLength));
  };

  const cambiarTipoDocumentoNuevo = (tipo) => {
    setNuevoTipoDocumento(tipo);
    const regla = REGLAS_DOCUMENTO[tipo];
    let limpio = nuevoDocumento;
    if (regla.soloNumeros) limpio = limpio.replace(/\D/g, '');
    setNuevoDocumento(limpio.slice(0, regla.maxLength));
  };

  const registrarClienteNuevo = async () => {
    setErrorCliente('');
    if (!nuevoDocumento.trim() || !nuevoNombre.trim()) {
      setErrorCliente('Completa documento y nombre.');
      return;
    }
    if (reglaDocumento.soloNumeros && nuevoDocumento.length !== reglaDocumento.maxLength) {
      setErrorCliente(`${reglaDocumento.label} debe tener exactamente ${reglaDocumento.maxLength} dígitos.`);
      return;
    }
    setCreandoCliente(true);
    try {
      const nuevo = await api.clienteCrear({
        tipo_documento: tipoDocumentoParaNuevo,
        numero_documento: nuevoDocumento.trim(),
        nombre_razon_social: nuevoNombre.trim(),
      });
      seleccionarCliente(nuevo);
    } catch (e) {
      setErrorCliente(e.message);
    } finally {
      setCreandoCliente(false);
    }
  };

  const productosFiltrados = useMemo(() => {
    if (!busqueda.trim()) return productos;
    const q = busqueda.toLowerCase();
    return productos.filter((p) => p.nombre.toLowerCase().includes(q) || p.codigo.includes(q));
  }, [productos, busqueda]);

  const marcarImagenFallida = (id) => {
    setImagenesFallidas((prev) => {
      if (prev.has(id)) return prev;
      const next = new Set(prev);
      next.add(id);
      return next;
    });
  };

  const agregarAlCarrito = (producto) => {
    setCarrito((prev) => {
      const existe = prev.find((i) => i.id === producto.id);
      if (existe) {
        return prev.map((i) => (i.id === producto.id ? { ...i, cantidad: i.cantidad + 1 } : i));
      }
      return [...prev, { ...producto, cantidad: 1, descuentoMonto: 0 }];
    });
  };

  const manejarEnterBusquedaProducto = (e) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const texto = busqueda.trim();
    if (!texto) return;

    const porCodigoExacto = productos.find((p) => p.codigo.toLowerCase() === texto.toLowerCase());
    if (porCodigoExacto) {
      agregarAlCarrito(porCodigoExacto);
      setBusqueda('');
      return;
    }

    if (productosFiltrados.length === 1) {
      agregarAlCarrito(productosFiltrados[0]);
      setBusqueda('');
      return;
    }

    setMensaje({ tipo: 'error', texto: `No se encontró ningún producto con código o nombre "${texto}"` });
  };

  // Llamado por EscanerCodigoBarras cada vez que la cámara detecta un
  // código nuevo (ya filtrado de repeticiones por el propio escáner).
  // Reusa la misma búsqueda exacta por código que ya usa el buscador de
  // texto, para mantener un solo criterio de "qué cuenta como match".
  const manejarCodigoEscaneado = useCallback(
    (codigo) => {
      const producto = productos.find((p) => p.codigo.toLowerCase() === codigo.toLowerCase());
      if (producto) {
        agregarAlCarrito(producto);
        setUltimoEscaneo({ tipo: 'ok', texto: `${producto.nombre} agregado` });
      } else {
        setUltimoEscaneo({ tipo: 'error', texto: `Código "${codigo}" no encontrado` });
      }
    },
    [productos]
  );

  // El mensaje de "producto agregado / no encontrado" se borra solo tras
  // un par de segundos, para no acumular texto viejo mientras se sigue
  // escaneando.
  useEffect(() => {
    if (!ultimoEscaneo) return;
    const timeout = setTimeout(() => setUltimoEscaneo(null), DURACION_MENSAJE_ESCANEO_MS);
    return () => clearTimeout(timeout);
  }, [ultimoEscaneo]);

  const cambiarCantidad = (id, delta) => {
    setCarrito((prev) =>
      prev
        .map((i) => (i.id === id ? { ...i, cantidad: Math.max(1, i.cantidad + delta) } : i))
        .filter((i) => i.cantidad > 0)
    );
  };

  const quitarDelCarrito = (id) => setCarrito((prev) => prev.filter((i) => i.id !== id));

  const total = useMemo(
    () => carrito.reduce((sum, i) => sum + i.precio * i.cantidad - (i.descuentoMonto || 0), 0),
    [carrito]
  );

  const cambio = useMemo(() => {
    const recibido = parseFloat(montoRecibido) || 0;
    return metodoPago === 'EFECTIVO' ? Math.max(0, recibido - total) : 0;
  }, [montoRecibido, total, metodoPago]);

  const clienteEsObligatorio = tipoComprobante === 'BOLETA' || tipoComprobante === 'FACTURA';
  const clienteEsOpcionalVisible = tipoComprobante === 'NINGUNO';

  const puedeCobrar =
    carrito.length > 0 &&
    !procesando &&
    (!clienteEsObligatorio || cliente) &&
    (metodoPago !== 'EFECTIVO' || parseFloat(montoRecibido) >= total);

  const procesarVenta = async () => {
    if (tipoComprobante !== 'NINGUNO' && !facturacionConfigurada) {
      setMensaje({
        tipo: 'error',
        texto: 'Falta configurar el Token y la URL de FacturaLibre en Configuración antes de emitir Boleta o Factura. Cambia a "Nota simple" para continuar con esta venta, o completa la configuración primero.',
      });
      return; // no se crea la venta -- el carrito queda intacto
    }
    setProcesando(true);
    setMensaje(null);
    try {
      const resultado = await api.ventaCrear({
        productos: carrito.map((i) => ({
          id: i.id,
          nombre: i.nombre,
          precio: i.precio,
          cantidad: i.cantidad,
          descuentoMonto: i.descuentoMonto || 0,
        })),
        total,
        metodo_pago: metodoPago,
        monto_recibido: metodoPago === 'EFECTIVO' ? parseFloat(montoRecibido) || total : null,
        cambio: metodoPago === 'EFECTIVO' ? cambio : null,
        usuario_id: usuario.id,
        cliente_id: cliente?.id || null,
      });

      let comprobante = null;
      let errorComprobante = null;
      if (tipoComprobante !== 'NINGUNO') {
        try {
          comprobante = await api.comprobanteEmitir({
            venta_id: resultado.venta_id,
            tipo: tipoComprobante,
            cliente_documento: cliente?.numero_documento || null,
            cliente_nombre: cliente?.nombre_razon_social || null,
          });
        } catch (e) {
          errorComprobante = e.message;
        }
      }

      const datosVenta = {
        venta: {
          folio: resultado.folio,
          total,
          montoRecibido: metodoPago === 'EFECTIVO' ? parseFloat(montoRecibido) || total : null,
          cambio: metodoPago === 'EFECTIVO' ? cambio : null,
        },
        items: carrito.map((i) => ({ nombre: i.nombre, cantidad: i.cantidad, precio: i.precio })),
        comprobante,
        cliente,
      };

      setUltimaVentaParaImprimir(datosVenta);

      if (errorComprobante) {
        setMensaje({ tipo: 'error', texto: `Venta registrada, pero falló el comprobante: ${errorComprobante}` });
      } else {
        setMostrarModalVenta(true);
        setCarritoAbierto(false);
      }

      setCarrito([]);
      setMontoRecibido('');
      setCliente(null);
      setMostrarBusquedaCliente(false);
      setBusquedaCliente('');
      setTipoComprobante('BOLETA');
      setTelefonoWhatsapp('');
      api.productos().then(setProductos);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setProcesando(false);
    }
  };

  const placeholderBusqueda =
    tipoComprobante === 'FACTURA'
      ? 'RUC o razón social... (Enter si no aparece)'
      : 'Documento o nombre... (Enter si no aparece)';

  // Las boletas se imprimen SIEMPRE con nuestro propio ticket (Recibo,
  // formato angosto pensado para impresora térmica) — FacturaLibre solo
  // entrega un formato genérico A4, no apto para ticket. Las facturas sí
  // usan el PDF real de FacturaLibre (documento oficial en A4).
  const imprimirComprobante = () => {
    const comp = ultimaVentaParaImprimir?.comprobante;
    if (comp?.tipo === 'FACTURA' && comp?.enlace_pdf && comp?.comprobante_id) {
      setPdfVisible(api.comprobantePdfUrl(comp.comprobante_id));
    } else {
      window.print();
    }
  };

  // Renderiza cada página del PDF como imagen dentro del modal — igual
  // que en Comprobantes.jsx. No depende del visor nativo del navegador,
  // así que funciona igual en Android, iPhone y desktop.
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

  // Imprime solo las páginas renderizadas — no abre pestaña ni ventana
  // nueva. Requiere la misma regla @media print que ya tiene
  // Comprobantes.css (ver nota más abajo sobre POS.css).
  const imprimirPdfEmbebido = () => {
    window.print();
  };

  // Manda el comprobante (o un resumen, si es nota simple) por WhatsApp.
  // Usa SIEMPRE comp.enlace_pdf (el link público real de FacturaLibre) —
  // nunca api.comprobantePdfUrl(), que lleva el token de sesión en la
  // URL y no debe salir de la app.
  const enviarPorWhatsapp = () => {
    if (!ultimaVentaParaImprimir) return;
    const numero = telefonoWhatsapp.replace(/\D/g, '');
    if (numero.length < 9) {
      setMensaje({ tipo: 'error', texto: 'Ingresa un número de WhatsApp válido (9 dígitos).' });
      return;
    }
    const numeroConPais = numero.length === 9 ? `51${numero}` : numero;

    const comp = ultimaVentaParaImprimir.comprobante;
    const total = ultimaVentaParaImprimir.venta.total.toFixed(2);

    let texto;
    if (comp?.tipo === 'FACTURA' && comp?.enlace_pdf) {
      // Factura: el A4 oficial real de FacturaLibre.
      const numeroDoc = `${comp.serie}-${String(comp.numero).padStart(6, '0')}`;
      texto = `Hola! Aquí tienes tu factura ${numeroDoc} por S/ ${total}.\n\nPuedes verla aquí: ${comp.enlace_pdf}\n\n¡Gracias por tu compra!`;
    } else if (comp?.tipo === 'BOLETA' && comp?.comprobante_id && identificadorNegocio) {
      // Boleta: nuestra propia página pública, en formato ticket (no el
      // A4 genérico de FacturaLibre) — mismo QR real, otro formato.
      const numeroDoc = `${comp.serie}-${String(comp.numero).padStart(6, '0')}`;
      const urlPublica = `${window.location.origin}/boleta/${identificadorNegocio}/${comp.comprobante_id}`;
      texto = `Hola! Aquí tienes tu boleta ${numeroDoc} por S/ ${total}.\n\nPuedes verla aquí: ${urlPublica}\n\n¡Gracias por tu compra!`;
    } else {
      texto = `Hola! Gracias por tu compra. Total: S/ ${total} — Venta ${ultimaVentaParaImprimir.venta.folio}.`;
    }

    window.open(`https://wa.me/${numeroConPais}?text=${encodeURIComponent(texto)}`, '_blank');
  };

  const mostrarFormularioNuevo = sinResultadosCliente && !buscandoCliente;
  const mostrarSeccionCliente = clienteEsObligatorio || mostrarBusquedaCliente || cliente;

  return (
    <div className="pos-layout">
      <div className="pos-productos">
        <div className="pos-buscador-fila">
          <input
            ref={buscadorRef}
            className="pos-buscador"
            type="text"
            placeholder="Escanea o busca por nombre / código..."
            value={busqueda}
            onChange={(e) => setBusqueda(e.target.value)}
            onKeyDown={manejarEnterBusquedaProducto}
            autoFocus={autoFocoBuscador}
          />
          <button
            type="button"
            className="pos-boton-escanear"
            onClick={() => {
              setMensaje(null);
              setUltimoEscaneo(null);
              setEscanerAbierto(true);
            }}
            aria-label="Escanear código de barras"
          >
            📷
          </button>
        </div>
        <div className="pos-grid">
          {productosFiltrados.map((p) => (
            <button key={p.id} className="pos-producto-card" onClick={() => agregarAlCarrito(p)}>
              {p.imagen_url && !imagenesFallidas.has(p.id) ? (
                <img
                  className="pos-producto-imagen"
                  src={`${API_URL}${p.imagen_url}`}
                  alt={p.nombre}
                  onError={() => marcarImagenFallida(p.id)}
                />
              ) : (
                <div className="pos-producto-imagen pos-producto-imagen-vacia">📦</div>
              )}
              <span className="pos-producto-nombre">{p.nombre}</span>
              <span className="pos-producto-precio">S/ {p.precio.toFixed(2)}</span>
              <span className="pos-producto-stock">Stock: {p.stock}</span>
            </button>
          ))}
          {productosFiltrados.length === 0 && (
            <p className="pos-sin-resultados">No se encontraron productos</p>
          )}
        </div>
      </div>

      <button
        type="button"
        className="pos-fab-carrito"
        onClick={() => {
          setMensaje(null);
          setCarritoAbierto(true);
        }}
        aria-label="Abrir carrito"
      >
        <span className="pos-fab-carrito-icono">🛒</span>
        <span className="pos-fab-carrito-total">S/ {total.toFixed(2)}</span>
        {carrito.length > 0 && (
          <span className="pos-fab-carrito-badge">{carrito.length}</span>
        )}
      </button>

      <div className={`pos-carrito${carritoAbierto ? ' pos-carrito-abierto' : ''}`}>
        <div className="pos-carrito-header-movil">
          <h2>Cobrar</h2>
          <button
            type="button"
            className="pos-carrito-cerrar"
            onClick={() => {
              setMensaje(null);
              setCarritoAbierto(false);
            }}
            aria-label="Cerrar carrito"
          >
            ×
          </button>
        </div>

        <div className="pos-comprobante">
          <span className="pos-comprobante-label">Comprobante</span>
          <div className="pos-comprobante-opciones">
            <button
              className={tipoComprobante === 'BOLETA' ? 'activo' : ''}
              onClick={() => cambiarTipoComprobante('BOLETA')}
            >
              Boleta
            </button>
            <button
              className={tipoComprobante === 'FACTURA' ? 'activo' : ''}
              onClick={() => cambiarTipoComprobante('FACTURA')}
            >
              Factura
            </button>
            <button
              className={tipoComprobante === 'NINGUNO' ? 'activo' : ''}
              onClick={() => cambiarTipoComprobante('NINGUNO')}
            >
              Nota simple
            </button>
          </div>
          {tipoComprobante === 'NINGUNO' && (
            <p className="pos-aviso-nota-simple">
              ⚠️ Sin comprobante tributario. Emitir ventas reales sin boleta/factura puede constituir
              infracción ante SUNAT (evasión). El uso de esta opción es responsabilidad exclusiva del
              negocio.
            </p>
          )}
        </div>

        {clienteEsOpcionalVisible && !mostrarSeccionCliente && (
          <button className="pos-cliente-opcional" onClick={() => setMostrarBusquedaCliente(true)}>
            + Agregar cliente (opcional)
          </button>
        )}

        {mostrarSeccionCliente && (
          <div className="pos-cliente">
            {cliente ? (
              <div className="pos-cliente-seleccionado">
                <span>
                  {cliente.nombre_razon_social}
                  {cliente.numero_documento ? ` — ${cliente.numero_documento}` : ''}
                </span>
                <button onClick={quitarCliente}>×</button>
              </div>
            ) : (
              <>
                <input
                  type="text"
                  placeholder={placeholderBusqueda}
                  value={busquedaCliente}
                  onChange={(e) => setBusquedaCliente(e.target.value)}
                  onKeyDown={manejarEnterBusquedaCliente}
                  autoFocus
                />
                {resultadosCliente.length > 0 && (
                  <div className="pos-cliente-resultados">
                    {resultadosCliente.map((c) => (
                      <button key={c.id} onClick={() => seleccionarCliente(c)}>
                        {c.nombre_razon_social} {c.numero_documento ? `— ${c.numero_documento}` : ''}
                      </button>
                    ))}
                  </div>
                )}

                {mostrarFormularioNuevo && (
                  <div className="pos-cliente-nuevo">
                    <p className="pos-cliente-nuevo-aviso">No se encontró. Solo falta el nombre:</p>

                    {tipoComprobante === 'BOLETA' && (
                      <div className="pos-tipo-documento-opciones">
                        {['DNI', 'CE', 'PASAPORTE'].map((tipo) => (
                          <button
                            key={tipo}
                            className={nuevoTipoDocumento === tipo ? 'activo' : ''}
                            onClick={() => cambiarTipoDocumentoNuevo(tipo)}
                          >
                            {tipo === 'PASAPORTE' ? 'Pasaporte' : tipo}
                          </button>
                        ))}
                      </div>
                    )}

                    <input
                      type="text"
                      inputMode={reglaDocumento.soloNumeros ? 'numeric' : 'text'}
                      placeholder={reglaDocumento.label}
                      value={nuevoDocumento}
                      onChange={(e) => manejarCambioDocumento(e.target.value)}
                    />
                    <input
                      type="text"
                      placeholder={
                        consultandoDocumento
                          ? 'Buscando nombre...'
                          : tipoComprobante === 'FACTURA'
                            ? 'Razón social'
                            : 'Nombre completo'
                      }
                      value={nuevoNombre}
                      onChange={(e) => {
                        setNuevoNombre(e.target.value);
                        setNombreAutocompletado(false);
                      }}
                      autoFocus
                    />
                    {nombreAutocompletado && (
                      <p className="pos-cliente-nuevo-autocompletado">✓ Nombre obtenido de RENIEC/SUNAT</p>
                    )}
                    {errorCliente && <p className="pos-cliente-nuevo-error">{errorCliente}</p>}
                    <button
                      className="pos-cliente-nuevo-guardar"
                      onClick={registrarClienteNuevo}
                      disabled={creandoCliente}
                    >
                      {creandoCliente ? 'Guardando...' : 'Registrar y usar'}
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        )}

        <div className="pos-carrito-items">
          {carrito.length === 0 && <p className="pos-carrito-vacio">Carrito vacío</p>}
          {carrito.map((item) => (
            <div key={item.id} className="pos-carrito-item">
              <div className="pos-carrito-item-info">
                <span className="pos-carrito-item-nombre">{item.nombre}</span>
                <span className="pos-carrito-item-precio">S/ {item.precio.toFixed(2)} c/u</span>
              </div>
              <div className="pos-carrito-item-controles">
                <button onClick={() => cambiarCantidad(item.id, -1)}>−</button>
                <span>{item.cantidad}</span>
                <button onClick={() => cambiarCantidad(item.id, 1)}>+</button>
                <button className="pos-quitar" onClick={() => quitarDelCarrito(item.id)}>🗑</button>
              </div>
            </div>
          ))}
        </div>

        <div className="pos-resumen">
          <div className="pos-total-row">
            <span>Total</span>
            <span className="pos-total-monto">S/ {total.toFixed(2)}</span>
          </div>

          <div className="pos-metodo-pago">
            {['EFECTIVO', 'TARJETA', 'TRANSFERENCIA', 'YAPE_PLIN'].map((m) => (
              <button
                key={m}
                className={metodoPago === m ? 'activo' : ''}
                onClick={() => setMetodoPago(m)}
              >
                {m.replace('_', '/')}
              </button>
            ))}
          </div>

          {metodoPago === 'EFECTIVO' && (
            <div className="pos-efectivo">
              <input
                type="number"
                placeholder="Monto recibido"
                value={montoRecibido}
                onChange={(e) => setMontoRecibido(e.target.value)}
              />
              <span className="pos-cambio">Cambio: S/ {cambio.toFixed(2)}</span>
            </div>
          )}

          {mensaje && <p className={`pos-mensaje pos-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

          {ultimaVentaParaImprimir && !mostrarModalVenta && (
            <button className="pos-imprimir" onClick={imprimirComprobante}>
              Imprimir última boleta · {ultimaVentaParaImprimir.venta.folio}
            </button>
          )}

          <button className="pos-cobrar" disabled={!puedeCobrar} onClick={procesarVenta}>
            {procesando ? 'Procesando...' : `Cobrar S/ ${total.toFixed(2)}`}
          </button>
        </div>
      </div>

      {escanerAbierto && (
        <EscanerCodigoBarras
          onCodigoDetectado={manejarCodigoEscaneado}
          onCerrar={() => setEscanerAbierto(false)}
          ultimoResultado={ultimoEscaneo}
        />
      )}

      {mostrarModalVenta && ultimaVentaParaImprimir && (
        <div className="pos-venta-modal-overlay">
          <div className="pos-venta-modal">
            <div className="pos-venta-modal-check">✓</div>
            <h2>Venta registrada</h2>
            <p className="pos-venta-modal-folio">{ultimaVentaParaImprimir.venta.folio}</p>

            <p className="pos-venta-modal-tipo">
              {ultimaVentaParaImprimir.comprobante
                ? `${ultimaVentaParaImprimir.comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${ultimaVentaParaImprimir.comprobante.serie}-${String(ultimaVentaParaImprimir.comprobante.numero).padStart(6, '0')}`
                : 'Nota de venta'}
            </p>

            <div className="pos-venta-modal-linea">
              <span>Total</span>
              <strong>S/ {ultimaVentaParaImprimir.venta.total.toFixed(2)}</strong>
            </div>
            {ultimaVentaParaImprimir.venta.cambio != null && (
              <div className="pos-venta-modal-linea">
                <span>Cambio</span>
                <strong>S/ {ultimaVentaParaImprimir.venta.cambio.toFixed(2)}</strong>
              </div>
            )}

            <div className="pos-venta-modal-whatsapp">
              <input
                type="tel"
                placeholder="WhatsApp del cliente (opcional)"
                value={telefonoWhatsapp}
                onChange={(e) => setTelefonoWhatsapp(e.target.value)}
              />
              <button className="pos-venta-modal-whatsapp-boton" onClick={enviarPorWhatsapp}>
                📲 Enviar por WhatsApp
              </button>
            </div>

            <div className="pos-venta-modal-acciones">
              <button className="pos-venta-modal-imprimir" onClick={imprimirComprobante}>
                🖨 Imprimir
              </button>
              <button
                className="pos-venta-modal-cerrar"
                onClick={() => {
                  setMostrarModalVenta(false);
                  setCarritoAbierto(false);
                  buscadorRef.current?.focus();
                }}
              >
                Nueva venta
              </button>
            </div>
          </div>
        </div>
      )}

      {pdfVisible && (
        <div className="pos-pdf-modal-overlay">
          <div className="pos-pdf-modal">
            <div className="pos-pdf-modal-header pos-no-imprimir">
              <h2>Comprobante</h2>
              <button
                type="button"
                className="pos-carrito-cerrar"
                onClick={cerrarPdf}
                aria-label="Cerrar"
              >
                ×
              </button>
            </div>

            <div className="pos-pdf-paginas" ref={pdfContenedorRef}>
              {pdfCargando && <p className="pos-pdf-cargando pos-no-imprimir">Cargando vista previa...</p>}

              {pdfError && (
                <div className="pos-pdf-error pos-no-imprimir">
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
                    className="pos-pdf-pagina-img"
                  />
                ))}
            </div>

            <div className="pos-pdf-modal-acciones pos-no-imprimir">
              <button className="pos-pdf-modal-cerrar" onClick={cerrarPdf}>
                Cerrar
              </button>
              <button
                className="pos-pdf-modal-imprimir"
                onClick={imprimirPdfEmbebido}
                disabled={pdfCargando || !!pdfError || pdfPaginas.length === 0}
              >
                🖨 Imprimir
              </button>
            </div>
          </div>
        </div>
      )}

      {ultimaVentaParaImprimir && (
        <Recibo
          venta={ultimaVentaParaImprimir.venta}
          items={ultimaVentaParaImprimir.items}
          comprobante={ultimaVentaParaImprimir.comprobante}
          cliente={ultimaVentaParaImprimir.cliente}
          nombreTienda={nombreTienda}
          direccion={direccion}
          telefono={telefono}
          ruc={ruc}
          cajero={usuario.nombre}
        />
      )}
    </div>
  );
}