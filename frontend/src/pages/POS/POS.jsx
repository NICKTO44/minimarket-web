import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import * as pdfjsLib from 'pdfjs-dist/legacy/build/pdf.mjs';
import pdfjsWorker from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import { api, API_URL } from '../../api/api';
import './POS.css';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';
import EscanerCodigoBarras from '../../components/EscanerCodigoBarras';
import { ChevronRight, ShoppingCart } from 'lucide-react';
import { METODOS_OTRO_MIXTO, nombreMetodo } from '../../utils/metodoPago';
import { tituloPedido } from '../../utils/mesas';
import { abreviaturaUnidad } from '../../utils/unidades';
import { UNIDAD_PIE_TABLAR, formatoCantidad, leerCantidad, subtotalLinea } from '../../utils/medidas';
import CalculadoraPieTablar from '../../components/CalculadoraPieTablar';
import CotizacionImprimible from '../../components/CotizacionImprimible';
import { numeroCotizacion } from '../../utils/formato';
import '../../components/PantallaModulo.css';
import SelectorTalla from '../../components/SelectorTalla';
import { agruparPorModelo } from '../../utils/variantes';

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfjsWorker;

const DEBOUNCE_BUSQUEDA_VIVA_MS = 400;
const DURACION_MENSAJE_ESCANEO_MS = 2500;

const REGLAS_DOCUMENTO = {
  DNI: { maxLength: 8, soloNumeros: true, label: 'DNI (8 dígitos)' },
  CE: { maxLength: 12, soloNumeros: false, label: 'Carnet de Extranjería' },
  PASAPORTE: { maxLength: 12, soloNumeros: false, label: 'Pasaporte' },
  RUC: { maxLength: 11, soloNumeros: true, label: 'RUC (11 dígitos)' },
};

// "26 und", "2.5 kg" -- el stock puede tener decimales (productos por peso).
function etiquetaStock(producto) {
  const cantidad = Number.isInteger(producto.stock)
    ? producto.stock
    : Number(producto.stock.toFixed(2));
  return `${cantidad} ${abreviaturaUnidad(producto.unidad_medida)}`;
}

const redondear2 = (n) => Math.round(n * 100) / 100;
const redondear3 = (n) => Math.round(n * 1000) / 1000;

// Identifica una línea del carrito. Un producto normal tiene una sola
// línea (su id); las líneas de un pedido de mesa y las de venta por
// medidas llevan clave propia porque el mismo producto puede repetirse.
const claveDe = (item) => item.clave || item.id;
// Línea agregada con la calculadora de pie tablar: su cantidad son los
// pies calculados, no se cambia con − y +.
const esLineaMedida = (item) => typeof item.clave === 'string' && item.clave.startsWith('medida-');

/**
 * Cantidad del carrito que se puede escribir (módulo "Venta por medidas"):
 * 0.5 kg, 37.5 pies. Se aplica al salir del campo o con Enter; si lo
 * escrito no sirve, vuelve a la cantidad anterior.
 */
function CantidadEditable({ valor, onCambiar, etiqueta }) {
  const [texto, setTexto] = useState(null);
  const aplicar = () => {
    if (texto === null) return;
    const nueva = leerCantidad(texto);
    if (nueva !== null && nueva !== valor) onCambiar(nueva);
    setTexto(null);
  };
  return (
    <input
      className="pos-cantidad-input"
      inputMode="decimal"
      value={texto ?? formatoCantidad(valor)}
      onChange={(e) => setTexto(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={aplicar}
      onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      aria-label={etiqueta}
    />
  );
}

// Precio de una línea del carrito que se puede cambiar SOLO para esta venta
// (módulo "Cambiar precio al vender"). El producto conserva su precio.
function leerPrecio(texto) {
  const precio = Math.round(parseFloat(String(texto).trim().replace(',', '.')) * 100) / 100;
  return Number.isFinite(precio) && precio > 0 && precio < 10000000 ? precio : null;
}

function PrecioEditable({ valor, onCambiar, etiqueta }) {
  const [texto, setTexto] = useState(null);
  const aplicar = () => {
    if (texto === null) return;
    const nuevo = leerPrecio(texto);
    if (nuevo !== null && nuevo !== valor) onCambiar(nuevo);
    setTexto(null);
  };
  return (
    <input
      className="pos-cantidad-input pos-precio-input"
      inputMode="decimal"
      value={texto ?? valor.toFixed(2)}
      onChange={(e) => setTexto(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={aplicar}
      onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      aria-label={etiqueta}
    />
  );
}

// Líneas de un pedido de mesa convertidas en carrito del POS. Cada línea
// es independiente (el mismo café puede ir con opciones distintas), por
// eso lleva su propia clave; "detalle" son las opciones elegidas.
function carritoDePedido(pedido) {
  return pedido.items.map((i) => ({
    id: i.producto_id,
    clave: `pedido-${i.id}`,
    nombre: i.nombre_producto,
    precio: i.precio_unitario,
    cantidad: i.cantidad,
    descuentoMonto: 0,
    detalle: i.opciones || null,
  }));
}

// Líneas de una cotización convertidas en carrito: van con el precio que
// se le ofreció al cliente y su cantidad fija (clave "medida-...").
function carritoDeCotizacion(cotizacion) {
  return cotizacion.items.map((i, idx) => ({
    id: i.producto_id,
    clave: `medida-cot-${cotizacion.id}-${idx}`,
    nombre: i.nombre,
    precio: i.precio_unitario,
    cantidad: i.cantidad,
    descuentoMonto: 0,
    detalle: i.detalle || null,
    unidad_medida: i.unidad_medida,
  }));
}

// Plazos que se ofrecen al vender al crédito (días).
const PLAZOS_CREDITO = [7, 15, 30, 45, 60];
const METODOS_ADELANTO = ['EFECTIVO', 'YAPE_PLIN', 'TRANSFERENCIA', 'TARJETA'];

// Opciones del selector "Ordenar por" encima de la grilla del POS.
const OPCIONES_ORDEN = [
  { valor: 'nombre', label: 'Nombre (A-Z)' },
  { valor: 'precio-asc', label: 'Precio: menor a mayor' },
  { valor: 'precio-desc', label: 'Precio: mayor a menor' },
];

export default function POS({
  usuario,
  nombreTienda = 'Mi Minimarket',
  direccion,
  telefono,
  ruc,
  identificadorNegocio,
  // Módulo "Venta por medidas": cantidad con decimales y calculadora de
  // pie tablar. Apagado, el punto de venta es el de siempre.
  medidas = false,
  // Módulo "Cotizaciones": botón para guardar el carrito como cotización.
  cotizaciones = false,
  // Módulo "Ventas al crédito": método de pago "Crédito".
  credito = false,
  // Cotización que se abrió desde su pantalla para venderla.
  cotizacionACargar = null,
  onCotizacionUsada,
  // Módulo "Tallas y colores": las tallas de un modelo van en una sola
  // tarjeta y se elige la talla al tocarla. Apagado, la grilla es la de siempre.
  variantes = false,
  // Módulo "Cambiar precio al vender": el precio de cada línea del carrito
  // se puede escribir, solo para esa venta. Apagado, el precio es fijo.
  precioEditable = false,
  // Módulo "Cambio de prenda": plazo de cambio que se imprime en el ticket.
  cambios = false,
  // Cambio de prenda en curso (viene de Devoluciones): lo que el cliente
  // devuelve; lo que se agregue al carrito es lo que se lleva.
  cambioEnCurso = null,
  onCambioTerminado,
  // Cafetería / Restaurante: pedido de una mesa que se está cobrando.
  pedidoACobrar = null,
  onCancelarCobroPedido,
  onPedidoCobrado,
  onVolverAMesas,
}) {
  const cobrandoPedido = !!pedidoACobrar;
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
  const [ordenPrecio, setOrdenPrecio] = useState('nombre');
  // Categoría elegida para ver solo sus productos (null = todas).
  const [categoriaFiltro, setCategoriaFiltro] = useState(null);
  const buscadorRef = useRef(null);
  const [carrito, setCarrito] = useState(() =>
    pedidoACobrar ? carritoDePedido(pedidoACobrar) : cotizacionACargar ? carritoDeCotizacion(cotizacionACargar) : []
  );
  // Cotización que se está vendiendo (al cobrar queda como vendida).
  const [cotizacionEnVenta, setCotizacionEnVenta] = useState(() =>
    !pedidoACobrar && cotizacionACargar ? { id: cotizacionACargar.id, numero: cotizacionACargar.numero } : null
  );
  // Ventana "Guardar cotización" ({ nombre, validez, notas }) y la recién guardada.
  const [nuevaCotizacion, setNuevaCotizacion] = useState(null);
  const [guardandoCotizacion, setGuardandoCotizacion] = useState(false);
  const [cotizacionGuardada, setCotizacionGuardada] = useState(null);
  // Venta al crédito: adelanto de hoy (opcional), con qué lo paga y plazo.
  const [creditoAdelanto, setCreditoAdelanto] = useState('');
  const [creditoAdelantoMetodo, setCreditoAdelantoMetodo] = useState('EFECTIVO');
  const [creditoDias, setCreditoDias] = useState(30);
  const [deudaCliente, setDeudaCliente] = useState(null);
  const [carritoAbierto, setCarritoAbierto] = useState(false);
  const [metodoPago, setMetodoPago] = useState('EFECTIVO');
  const [montoRecibido, setMontoRecibido] = useState('');
  // Pago MIXTO = efectivo + un medio digital. El cajero escribe cuánto se
  // pagó por el medio digital; la parte en efectivo se calcula sola.
  const [mixtoOtroMetodo, setMixtoOtroMetodo] = useState('YAPE_PLIN');
  const [mixtoMontoOtro, setMixtoMontoOtro] = useState('');
  const [cliente, setCliente] = useState(() =>
    !pedidoACobrar && cotizacionACargar?.cliente_id
      ? {
          id: cotizacionACargar.cliente_id,
          nombre_razon_social: cotizacionACargar.cliente_nombre,
          numero_documento: cotizacionACargar.cliente_documento,
        }
      : null
  );
  const [mostrarBusquedaCliente, setMostrarBusquedaCliente] = useState(false);
  const [busquedaCliente, setBusquedaCliente] = useState('');
  const [resultadosCliente, setResultadosCliente] = useState([]);
  const [buscandoCliente, setBuscandoCliente] = useState(false);
  const [sinResultadosCliente, setSinResultadosCliente] = useState(false);
  const [tipoComprobante, setTipoComprobante] = useState('BOLETA');
  // Producto por pie tablar cuya calculadora está abierta (módulo Medidas).
  const [productoAMedir, setProductoAMedir] = useState(null);
  // Modelo del que se está eligiendo la talla (módulo "Tallas y colores").
  const [modeloAElegir, setModeloAElegir] = useState(null);
  // Días que el cliente tiene para cambiar (módulo "Cambio de prenda").
  const [diasCambio, setDiasCambio] = useState(0);
  // Detracción del negocio (null = no la usa o aún no carga) y si esta
  // factura la lleva (el cajero puede excluir una venta que no está sujeta).
  const [detraccion, setDetraccion] = useState(null);
  const [conDetraccion, setConDetraccion] = useState(true);
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

  // Solo los negocios con el módulo de detracción la tienen "lista"
  // (encendida y con su cuenta). Si la consulta falla, se sigue sin ella.
  useEffect(() => {
    api
      .detraccion()
      .then((d) => setDetraccion(d?.lista ? d : null))
      .catch(() => setDetraccion(null));
  }, []);

  // Plazo de cambio para el ticket: solo lo pide un negocio con ese módulo.
  useEffect(() => {
    if (!cambios) return;
    api
      .cambiosConfig()
      .then((c) => setDiasCambio(c?.dias || 0))
      .catch(() => {});
  }, [cambios]);

  const [nuevoTipoDocumento, setNuevoTipoDocumento] = useState('DNI');
  const [nuevoDocumento, setNuevoDocumento] = useState('');
  const [nuevoNombre, setNuevoNombre] = useState('');
  const [creandoCliente, setCreandoCliente] = useState(false);
  const [errorCliente, setErrorCliente] = useState('');
  const [consultandoDocumento, setConsultandoDocumento] = useState(false);
  const [nombreAutocompletado, setNombreAutocompletado] = useState(false);
  // Dirección del cliente nuevo: la que trae la consulta del documento
  // (sugerida) o la que escribe el cajero, que manda si la hay.
  const [direccionSugerida, setDireccionSugerida] = useState('');
  const [direccionEscrita, setDireccionEscrita] = useState(null);
  const nuevaDireccion = direccionEscrita ?? direccionSugerida;
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
    setDireccionEscrita(null);
    setErrorCliente('');
  };

  const seleccionarCliente = (c) => {
    setCliente(c);
    setBusquedaCliente('');
    setResultadosCliente([]);
    setSinResultadosCliente(false);
    setNuevoDocumento('');
    setNuevoNombre('');
    setDireccionEscrita(null);
  };

  const quitarCliente = () => setCliente(null);

  const tipoDocumentoParaNuevo = tipoComprobante === 'FACTURA' ? 'RUC' : nuevoTipoDocumento;
  const reglaDocumento = REGLAS_DOCUMENTO[tipoDocumentoParaNuevo];

  // Autocompleta el nombre real y la dirección (RENIEC/SUNAT) apenas el
  // documento alcanza su largo completo. Nunca bloquea ni marca error si falla
  // -- api.documentoConsultar ya devuelve existe:null en cualquier
  // problema (sin token, timeout, tipo no soportado como CE/PASAPORTE),
  // y aquí simplemente no se autocompleta nada en ese caso.
  useEffect(() => {
    setNombreAutocompletado(false);
    setDireccionSugerida('');

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
        if (resultado.existe === true && resultado.direccion) setDireccionSugerida(resultado.direccion);
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
        direccion: nuevaDireccion.trim() || null,
      });
      seleccionarCliente(nuevo);
    } catch (e) {
      setErrorCliente(e.message);
    } finally {
      setCreandoCliente(false);
    }
  };

  // Categorías que tienen productos, por nombre. Salen de los mismos
  // productos ya cargados (no se pide nada más al servidor).
  const categoriasPos = useMemo(() => {
    const porId = new Map();
    for (const p of productos) {
      if (p.categoria_id != null && p.categoria_nombre && !porId.has(p.categoria_id)) {
        porId.set(p.categoria_id, p.categoria_nombre);
      }
    }
    return [...porId].map(([id, nombre]) => ({ id, nombre })).sort((a, b) => a.nombre.localeCompare(b.nombre, 'es'));
  }, [productos]);
  // Si la categoría elegida ya no tiene productos, se vuelve a "Todas".
  const categoriaActiva = categoriasPos.some((c) => c.id === categoriaFiltro) ? categoriaFiltro : null;

  const productosFiltrados = useMemo(() => {
    const q = busqueda.trim().toLowerCase();
    const deCategoria = categoriaActiva == null ? productos : productos.filter((p) => p.categoria_id === categoriaActiva);
    const base = q
      ? deCategoria.filter((p) => p.nombre.toLowerCase().includes(q) || p.codigo.toLowerCase().includes(q))
      : deCategoria;

    // El orden por nombre ya viene del backend (ORDER BY p.nombre), así
    // que para esa opción no hace falta reordenar en el frontend -- solo
    // se reordena cuando el usuario elige un orden por precio.
    if (ordenPrecio === 'precio-asc') {
      return [...base].sort((a, b) => a.precio - b.precio);
    }
    if (ordenPrecio === 'precio-desc') {
      return [...base].sort((a, b) => b.precio - a.precio);
    }
    // Restaurante: los platos de la carta de hoy van primero.
    if (base.some((p) => p.carta_dia)) {
      return [...base].sort((a, b) => Number(!!b.carta_dia) - Number(!!a.carta_dia));
    }
    return base;
  }, [productos, busqueda, ordenPrecio, categoriaActiva]);

  // Tallas y colores: una tarjeta por modelo (con las tallas que pasaron el
  // buscador). Sin el módulo, una tarjeta por producto, como siempre.
  const tarjetas = useMemo(
    () => (variantes ? agruparPorModelo(productosFiltrados) : productosFiltrados),
    [variantes, productosFiltrados]
  );

  // Abre el selector con TODAS las tallas del modelo, no solo las buscadas.
  const elegirTallaDe = (grupo) => {
    if (cobrandoPedido) {
      setMensaje({
        tipo: 'error',
        texto: `Estás cobrando ${tituloPedido(pedidoACobrar)}. Para agregar algo, vuelve a la mesa y agrégalo al pedido.`,
      });
      return;
    }
    const completo = agruparPorModelo(productos).find((g) => g.esModelo && g.modelo_id === grupo.modelo_id) || grupo;
    if (completo.variantes.length === 1) agregarAlCarrito(completo.variantes[0]);
    else setModeloAElegir(completo);
  };

  const marcarImagenFallida = (id) => {
    setImagenesFallidas((prev) => {
      if (prev.has(id)) return prev;
      const next = new Set(prev);
      next.add(id);
      return next;
    });
  };

  const agregarAlCarrito = (producto) => {
    if (cobrandoPedido) {
      setMensaje({
        tipo: 'error',
        texto: `Estás cobrando ${tituloPedido(pedidoACobrar)}. Para agregar algo, vuelve a la mesa y agrégalo al pedido.`,
      });
      return;
    }
    if (producto.agotado) {
      setMensaje({ tipo: 'error', texto: `"${producto.nombre}" se agotó.` });
      return;
    }
    // Madera por pie tablar: se abre la calculadora de medidas.
    if (medidas && producto.unidad_medida === UNIDAD_PIE_TABLAR) {
      setProductoAMedir(producto);
      return;
    }
    setCarrito((prev) => {
      const existe = prev.find((i) => !i.clave && i.id === producto.id);
      if (existe) {
        return prev.map((i) => (i === existe ? { ...i, cantidad: redondear3(i.cantidad + 1) } : i));
      }
      return [...prev, { ...producto, cantidad: 1, descuentoMonto: 0 }];
    });
  };

  // La calculadora devolvió pies y medidas: entra como línea propia (la
  // misma madera puede ir varias veces con medidas distintas).
  const agregarMedida = ({ cantidad, detalle }) => {
    const producto = productoAMedir;
    setProductoAMedir(null);
    if (!producto) return;
    setCarrito((prev) => [
      ...prev,
      {
        ...producto,
        clave: `medida-${Date.now()}-${prev.length}`,
        cantidad,
        descuentoMonto: 0,
        detalle,
      },
    ]);
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

    // Tallas y colores: lo escrito deja un solo modelo -> se elige su talla.
    if (variantes && tarjetas.length === 1 && tarjetas[0].esModelo) {
      elegirTallaDe(tarjetas[0]);
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

  // − y +: de uno en uno. Con cantidades con decimales (0.5 kg) el − no
  // baja de lo que hay si quedaría en cero o menos.
  const cambiarCantidad = (clave, delta) => {
    setCarrito((prev) =>
      prev.map((i) => {
        if (claveDe(i) !== clave) return i;
        const nueva = redondear3(i.cantidad + delta);
        return { ...i, cantidad: nueva > 0 ? nueva : Math.min(i.cantidad, 1) };
      })
    );
  };

  // Cantidad escrita a mano (módulo "Venta por medidas").
  const fijarCantidad = (clave, cantidad) => {
    setCarrito((prev) => prev.map((i) => (claveDe(i) === clave ? { ...i, cantidad } : i)));
  };

  // Precio escrito a mano para esta venta (módulo "Cambiar precio al
  // vender"). Se recuerda el precio del producto para mostrarlo y poder
  // volver a él; al producto no se le cambia nada.
  const fijarPrecio = (clave, precio) => {
    setCarrito((prev) =>
      prev.map((i) => {
        if (claveDe(i) !== clave) return i;
        const original = i.precioOriginal ?? i.precio;
        return { ...i, precio, precioOriginal: precio === original ? undefined : original };
      })
    );
  };

  const quitarDelCarrito = (clave) => setCarrito((prev) => prev.filter((i) => claveDe(i) !== clave));

  // Cada línea se redondea al céntimo (igual que el servidor): con
  // cantidades enteras da lo mismo que antes; con decimales evita totales
  // como 144.9855.
  const total = useMemo(
    () => redondear2(carrito.reduce((sum, i) => sum + subtotalLinea(i.precio, i.cantidad) - (i.descuentoMonto || 0), 0)),
    [carrito]
  );

  // Factura sujeta a detracción: negocio con el módulo listo y total mayor
  // al mínimo. La boleta nunca la lleva.
  const detraccionAplicable = !!detraccion && tipoComprobante === 'FACTURA' && total > detraccion.minimo;
  const montoDetraccion = detraccionAplicable ? redondear2((total * detraccion.porcentaje) / 100) : 0;

  // Cambio de prenda: lo devuelto va a favor del cliente y solo se cobra (o
  // se devuelve) la diferencia. Sin cambio en curso, se cobra el total.
  const enCambio = !!cambioEnCurso && !cobrandoPedido;
  const aFavor = enCambio ? cambioEnCurso.valor : 0;
  const aCobrar = enCambio ? redondear2(Math.max(0, total - aFavor)) : total;
  const aDevolver = enCambio ? redondear2(Math.max(0, aFavor - total)) : 0;

  const esMixto = metodoPago === 'MIXTO';
  const esCredito = metodoPago === 'CREDITO';
  const adelantoNum = redondear2(parseFloat(String(creditoAdelanto).replace(',', '.')) || 0);
  const adelantoValido = adelantoNum >= 0 && adelantoNum < total;

  // Cuánto debe ya ese cliente, para verlo antes de darle otro crédito.
  const clienteIdCredito = esCredito ? cliente?.id : null;
  useEffect(() => {
    if (!clienteIdCredito) return;
    api
      .creditoDeudaCliente(clienteIdCredito)
      .then(setDeudaCliente)
      .catch(() => setDeudaCliente(null));
  }, [clienteIdCredito]);
  const deudaActual = esCredito && cliente && deudaCliente?.cliente_id === cliente.id ? deudaCliente : null;
  const recibidoNum = parseFloat(montoRecibido) || 0;
  const mixtoOtro = redondear2(parseFloat(mixtoMontoOtro) || 0);
  const mixtoEfectivo = redondear2(Math.max(0, total - mixtoOtro));

  const cambio = useMemo(() => {
    const recibido = parseFloat(montoRecibido) || 0;
    if (metodoPago === 'EFECTIVO') return Math.max(0, recibido - aCobrar);
    if (metodoPago === 'MIXTO') return redondear2(Math.max(0, recibido - mixtoEfectivo));
    return 0;
  }, [montoRecibido, aCobrar, metodoPago, mixtoEfectivo]);

  // Estado del pago mixto: null = listo para cobrar; si no, un texto
  // que explica qué falta (solo se muestra como error cuando ya hay
  // algo escrito, para no regañar antes de tiempo).
  const estadoMixto = (() => {
    if (!esMixto) return { listo: true, error: null };
    const nombreOtro = nombreMetodo(mixtoOtroMetodo);
    if (mixtoOtro <= 0) return { listo: false, error: null };
    if (mixtoOtro >= total) {
      return { listo: false, error: `El monto por ${nombreOtro} debe ser menor al total. Si pagó todo por ${nombreOtro}, usa ese botón.` };
    }
    if (montoRecibido === '') return { listo: false, error: null };
    if (recibidoNum + 0.005 < mixtoEfectivo) {
      return { listo: false, error: `Faltan S/ ${(mixtoEfectivo - recibidoNum).toFixed(2)} en efectivo para completar el pago.` };
    }
    return { listo: true, error: null };
  })();

  // Al crédito siempre hace falta saber quién debe, aunque sea nota simple.
  const clienteEsObligatorio = tipoComprobante === 'BOLETA' || tipoComprobante === 'FACTURA' || esCredito;
  const clienteEsOpcionalVisible = tipoComprobante === 'NINGUNO' && !esCredito;

  const puedeCobrar =
    carrito.length > 0 &&
    !procesando &&
    (!clienteEsObligatorio || cliente) &&
    (metodoPago !== 'EFECTIVO' || (enCambio && aCobrar === 0) || parseFloat(montoRecibido) >= aCobrar) &&
    (!esCredito || adelantoValido) &&
    estadoMixto.listo;

  // Guarda lo que hay en el carrito como cotización (no toca stock ni caja).
  const guardarCotizacion = async () => {
    setGuardandoCotizacion(true);
    setMensaje(null);
    try {
      const guardada = await api.cotizacionCrear({
        cliente_id: cliente?.id || null,
        cliente_nombre: cliente ? null : nuevaCotizacion.nombre.trim() || null,
        validez_dias: nuevaCotizacion.validez,
        notas: nuevaCotizacion.notas.trim() || null,
        items: carrito.map((i) => ({ id: i.id, cantidad: i.cantidad, precio: i.precio, detalle: i.detalle || null })),
      });
      setNuevaCotizacion(null);
      setCotizacionGuardada(guardada);
      setCarrito([]);
      setCliente(null);
      setMostrarBusquedaCliente(false);
      setBusquedaCliente('');
      setCarritoAbierto(false);
    } catch (e) {
      setNuevaCotizacion(null);
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoCotizacion(false);
    }
  };

  // Deja de vender la cotización cargada (sigue pendiente en su pantalla).
  const quitarCotizacion = () => {
    setCotizacionEnVenta(null);
    setCarrito([]);
    setCliente(null);
    onCotizacionUsada?.();
  };

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
          detalle: i.detalle || null,
        })),
        total,
        // Al crédito el servidor ignora el método: el pago queda pendiente.
        metodo_pago: esCredito ? 'EFECTIVO' : metodoPago,
        credito: esCredito ? { adelanto: adelantoNum, adelanto_metodo: creditoAdelantoMetodo, dias: creditoDias } : null,
        cotizacion_id: cotizacionEnVenta?.id || null,
        // Cambio de prenda: lo que el cliente devuelve de su venta anterior.
        ...(enCambio
          ? {
              cambio_prenda: {
                venta_id: cambioEnCurso.venta_id,
                productos: cambioEnCurso.items.map((i) => ({ detalle_id: i.detalle_id, cantidad: i.cantidad, con_falla: !!i.con_falla })),
                motivo: cambioEnCurso.motivo || null,
              },
            }
          : {}),
        monto_recibido:
          metodoPago === 'EFECTIVO' ? parseFloat(montoRecibido) || aCobrar : esMixto ? recibidoNum : null,
        cambio: metodoPago === 'EFECTIVO' || esMixto ? cambio : null,
        usuario_id: usuario.id,
        cliente_id: cliente?.id || null,
        pago_efectivo: esMixto ? mixtoEfectivo : null,
        pago_otro: esMixto ? mixtoOtro : null,
        pago_otro_metodo: esMixto ? mixtoOtroMetodo : null,
        pedido_id: pedidoACobrar?.id || null,
      });

      // La venta ya quedó registrada: se muestra de inmediato para seguir
      // con el siguiente cliente. La boleta o factura se emite en segundo
      // plano (SUNAT puede tardar unos segundos) y, cuando llega, se agrega
      // a esta venta para poder imprimirla.
      const emiteComprobante = tipoComprobante !== 'NINGUNO';
      if (emiteComprobante) {
        const folio = resultado.folio;
        const paraEstaVenta = (cambios) => (actual) => (actual?.venta.folio === folio ? { ...actual, ...cambios } : actual);
        api
          .comprobanteEmitir({
            venta_id: resultado.venta_id,
            tipo: tipoComprobante,
            cliente_documento: cliente?.numero_documento || null,
            cliente_nombre: cliente?.nombre_razon_social || null,
            // Solo se manda cuando la factura podía llevar detracción: el
            // cajero la dejó marcada o la excluyó para esta venta.
            ...(detraccionAplicable ? { detraccion: conDetraccion } : {}),
          })
          .then((comprobante) => setUltimaVentaParaImprimir(paraEstaVenta({ comprobante, comprobantePendiente: false })))
          .catch((e) => {
            setUltimaVentaParaImprimir(paraEstaVenta({ comprobantePendiente: false, errorComprobante: e.message }));
            setMensaje({ tipo: 'error', texto: `Venta ${folio} registrada, pero falló el comprobante: ${e.message}` });
          });
      }

      const datosVenta = {
        venta: {
          folio: resultado.folio,
          total,
          montoRecibido:
            metodoPago === 'EFECTIVO' ? parseFloat(montoRecibido) || aCobrar : esMixto ? recibidoNum : null,
          cambio: metodoPago === 'EFECTIVO' || esMixto ? cambio : null,
          metodoPago,
          // Cambio de prenda: lo devuelto, lo que valía y la diferencia.
          cambioPrenda: enCambio
            ? {
                folioOriginal: cambioEnCurso.folio,
                items: cambioEnCurso.items,
                valor: aFavor,
                aCobrar,
                aDevolver,
                folioDevolucion: resultado.cambio_prenda?.folio_devolucion || null,
              }
            : null,
          pagoOtro: esMixto ? mixtoOtro : null,
          pagoOtroMetodo: esMixto ? mixtoOtroMetodo : null,
          credito: esCredito
            ? { adelanto: adelantoNum, adelantoMetodo: creditoAdelantoMetodo, saldo: redondear2(total - adelantoNum), dias: creditoDias }
            : null,
        },
        items: carrito.map((i) => ({
          nombre: i.detalle ? `${i.nombre} (${i.detalle})` : i.nombre,
          cantidad: i.cantidad,
          precio: i.precio,
        })),
        comprobante: null,
        // true mientras la boleta/factura se está emitiendo en segundo plano.
        comprobantePendiente: emiteComprobante,
        tipoComprobante,
        errorComprobante: null,
        cliente,
        deMesa: cobrandoPedido,
      };

      // El cambio quedó hecho: la pantalla vuelve a ser una venta normal.
      if (enCambio) onCambioTerminado?.();

      // El pedido quedó cobrado y su mesa libre.
      if (cobrandoPedido) onPedidoCobrado?.();

      setUltimaVentaParaImprimir(datosVenta);
      setMostrarModalVenta(true);
      setCarritoAbierto(false);

      setCarrito([]);
      setMontoRecibido('');
      // Un pago mixto es excepcional: la siguiente venta vuelve a Efectivo
      // para que nadie cobre en mixto por accidente.
      setMixtoMontoOtro('');
      // Lo mismo con el crédito.
      setMetodoPago((actual) => (actual === 'MIXTO' || actual === 'CREDITO' ? 'EFECTIVO' : actual));
      setCreditoAdelanto('');
      setCreditoAdelantoMetodo('EFECTIVO');
      setCreditoDias(30);
      if (cotizacionEnVenta) {
        setCotizacionEnVenta(null);
        onCotizacionUsada?.();
      }
      setCliente(null);
      setMostrarBusquedaCliente(false);
      setBusquedaCliente('');
      setTipoComprobante('BOLETA');
      setConDetraccion(true);
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
            onChange={(e) => {
              setBusqueda(e.target.value);
              // Al escribir se busca en todo el negocio, no solo en la
              // categoría que estaba elegida.
              if (e.target.value.trim()) setCategoriaFiltro(null);
            }}
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
        <div className={`pos-orden-fila${categoriasPos.length > 1 ? ' pos-orden-fila-categorias' : ''}`}>
          {/* Filtro por categoría: solo si el negocio tiene más de una. */}
          {categoriasPos.length > 1 && (
            <div className="pos-categorias" role="group" aria-label="Filtrar por categoría">
              <button
                type="button"
                className={`pos-categoria${categoriaActiva == null ? ' activo' : ''}`}
                aria-pressed={categoriaActiva == null}
                onClick={() => setCategoriaFiltro(null)}
              >
                Todas
              </button>
              {categoriasPos.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className={`pos-categoria${categoriaActiva === c.id ? ' activo' : ''}`}
                  aria-pressed={categoriaActiva === c.id}
                  onClick={() => {
                    setCategoriaFiltro(c.id);
                    setBusqueda('');
                  }}
                >
                  {c.nombre}
                </button>
              ))}
            </div>
          )}
          <div className="pos-orden-grupo">
            <label htmlFor="pos-orden-select" className="pos-orden-label">
              Ordenar por
            </label>
            <select
              id="pos-orden-select"
              className="pos-orden-select"
              value={ordenPrecio}
              onChange={(e) => setOrdenPrecio(e.target.value)}
            >
              {OPCIONES_ORDEN.map((op) => (
                <option key={op.valor} value={op.valor}>
                  {op.label}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="pos-grid">
          {tarjetas.map((p) =>
            p.esModelo ? (
              <button key={p.id} className="pos-producto-card pos-producto-modelo" onClick={() => elegirTallaDe(p)}>
                {p.imagen_url && !imagenesFallidas.has(p.id) ? (
                  <img
                    className="pos-producto-imagen"
                    src={`${API_URL}${p.imagen_url}`}
                    alt={p.nombre}
                    onError={() => marcarImagenFallida(p.id)}
                  />
                ) : (
                  <div className="pos-producto-imagen pos-producto-imagen-vacia">👕</div>
                )}
                <span className={`pos-modelo-etiqueta${p.stockTotal <= 0 ? ' agotado' : ''}`}>
                  {p.stockTotal <= 0
                    ? 'Agotado'
                    : `${p.variantes.length} ${p.tallas.length > 0 ? 'tallas' : 'colores'}`}
                </span>
                <span className="pos-producto-nombre">{p.nombre}</span>
                <span className="pos-producto-fila">
                  <span className="pos-producto-precio">
                    {p.precioMin !== p.precioMax && <small>desde </small>}S/ {p.precioMin.toFixed(2)}
                  </span>
                </span>
              </button>
            ) : (
              <button
                key={p.id}
                className={`pos-producto-card${p.agotado ? ' pos-producto-agotado' : ''}`}
                aria-disabled={p.agotado || undefined}
                onClick={() => agregarAlCarrito(p)}
              >
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
                <span className="pos-producto-fila">
                  <span className="pos-producto-precio">S/ {p.precio.toFixed(2)}</span>
                  {p.agotado && <span className="pos-producto-stock pos-producto-stock-bajo">Agotado</span>}
                  {/* Rojo con el mismo criterio que el reporte de stock bajo
                      (stock <= stock_minimo, ver productos.rs). */}
                  {p.controla_stock !== false && (
                    <span
                      className={`pos-producto-stock${p.stock <= p.stock_minimo ? ' pos-producto-stock-bajo' : ''}`}
                      title={`Stock: ${p.stock}`}
                    >
                      {etiquetaStock(p)}
                    </span>
                  )}
                </span>
              </button>
            )
          )}
          {tarjetas.length === 0 && (
            <p className="pos-sin-resultados">No se encontraron productos</p>
          )}
        </div>
      </div>

      {/* Solo en celular: barra del carrito encima de la navegación inferior.
          Aparece cuando hay productos; al tocarla se abre la pantalla de cobro. */}
      {carrito.length > 0 && (
        <button
          type="button"
          className="pos-fab-carrito"
          onClick={() => {
            setMensaje(null);
            setCarritoAbierto(true);
          }}
          aria-label="Abrir carrito para cobrar"
        >
          <ShoppingCart size={18} strokeWidth={2} />
          <span className="pos-fab-carrito-badge">{carrito.length}</span>
          <span className="pos-fab-carrito-texto">{carrito.length === 1 ? 'producto' : 'productos'}</span>
          <span className="pos-fab-carrito-total">
            S/ {total.toFixed(2)} · Cobrar
            <ChevronRight size={16} strokeWidth={2.5} />
          </span>
        </button>
      )}

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

        {cotizacionEnVenta && !cobrandoPedido && (
          <div className="pos-cobro-pedido">
            <div>
              <span className="pos-cobro-pedido-etiqueta">Vendiendo</span>
              <strong>Cotización N° {numeroCotizacion(cotizacionEnVenta.numero)}</strong>
            </div>
            <button type="button" onClick={quitarCotizacion}>
              Quitar
            </button>
          </div>
        )}

        {cobrandoPedido && (
          <div className="pos-cobro-pedido">
            <div>
              <span className="pos-cobro-pedido-etiqueta">Cobrando</span>
              <strong>{tituloPedido(pedidoACobrar)}</strong>
            </div>
            <button type="button" onClick={onCancelarCobroPedido}>
              Volver a la mesa
            </button>
          </div>
        )}

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
          {detraccionAplicable && (
            <label className={`pos-detraccion${conDetraccion ? '' : ' pos-detraccion-apagada'}`}>
              <input type="checkbox" checked={conDetraccion} onChange={(e) => setConDetraccion(e.target.checked)} />
              <span>
                <strong>
                  Sujeta a detracción ({formatoCantidad(detraccion.porcentaje)} %): S/ {montoDetraccion.toFixed(2)}
                </strong>
                {conDetraccion
                  ? ` El cliente deposita ese monto en tu cuenta del Banco de la Nación (${detraccion.cuenta}) y te paga S/ ${(total - montoDetraccion).toFixed(2)}.`
                  : ' Desmarcada: esta factura saldrá sin detracción.'}
              </span>
            </label>
          )}
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
                    <input
                      type="text"
                      placeholder={
                        consultandoDocumento
                          ? 'Buscando dirección...'
                          : tipoComprobante === 'FACTURA'
                            ? 'Dirección fiscal (opcional)'
                            : 'Dirección (opcional)'
                      }
                      value={nuevaDireccion}
                      onChange={(e) => setDireccionEscrita(e.target.value)}
                    />
                    {nombreAutocompletado && (
                      <p className="pos-cliente-nuevo-autocompletado">
                        {direccionSugerida && direccionEscrita === null
                          ? '✓ Nombre y dirección obtenidos de RENIEC/SUNAT'
                          : '✓ Nombre obtenido de RENIEC/SUNAT'}
                      </p>
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

        {enCambio && (
          <div className="pos-cambio-aviso">
            <div className="pos-cambio-aviso-cabecera">
              <strong>Cambio de prenda · {cambioEnCurso.folio}</strong>
              <button type="button" onClick={() => onCambioTerminado?.()} disabled={procesando}>
                Cancelar cambio
              </button>
            </div>
            {cambioEnCurso.items.map((i) => (
              <div key={i.detalle_id} className="pos-cambio-aviso-linea">
                <span>
                  Devuelve: {formatoCantidad(i.cantidad)} × {i.nombre}
                  {i.con_falla ? ' (con falla)' : ''}
                </span>
                <span>S/ {(i.valor_unitario * i.cantidad).toFixed(2)}</span>
              </div>
            ))}
            <p>Agrega abajo lo que el cliente se lleva.</p>
          </div>
        )}

        <div className="pos-carrito-items">
          {carrito.length === 0 && <p className="pos-carrito-vacio">{enCambio ? 'Elige la prenda nueva' : 'Carrito vacío'}</p>}
          {carrito.map((item) => (
            <div key={claveDe(item)} className="pos-carrito-item">
              <div className="pos-carrito-item-info">
                <span className="pos-carrito-item-nombre">{item.nombre}</span>
                {item.detalle && <span className="pos-carrito-item-detalle">{item.detalle}</span>}
                {precioEditable && !cobrandoPedido ? (
                  <span className="pos-carrito-item-precio pos-precio-editable">
                    S/{' '}
                    <PrecioEditable
                      valor={item.precio}
                      onCambiar={(precio) => fijarPrecio(claveDe(item), precio)}
                      etiqueta={`Precio de ${item.nombre} para esta venta`}
                    />{' '}
                    c/u
                    {medidas && ` · S/ ${subtotalLinea(item.precio, item.cantidad).toFixed(2)}`}
                    {item.precioOriginal != null && (
                      <button
                        type="button"
                        className="pos-precio-original"
                        onClick={() => fijarPrecio(claveDe(item), item.precioOriginal)}
                        title="Volver al precio del producto"
                      >
                        antes S/ {item.precioOriginal.toFixed(2)} ↺
                      </button>
                    )}
                  </span>
                ) : (
                  <span className="pos-carrito-item-precio">
                    {cobrandoPedido ? `${item.cantidad} × ` : ''}S/ {item.precio.toFixed(2)} c/u
                    {medidas && !cobrandoPedido && ` · S/ ${subtotalLinea(item.precio, item.cantidad).toFixed(2)}`}
                  </span>
                )}
              </div>
              {!cobrandoPedido &&
                (esLineaMedida(item) ? (
                  <div className="pos-carrito-item-controles pos-carrito-item-medida">
                    <span>
                      {formatoCantidad(item.cantidad)} {abreviaturaUnidad(item.unidad_medida)}
                    </span>
                    <button className="pos-quitar" onClick={() => quitarDelCarrito(claveDe(item))}>🗑</button>
                  </div>
                ) : (
                  <div className={`pos-carrito-item-controles${medidas ? ' pos-carrito-item-editable' : ''}`}>
                    <button onClick={() => cambiarCantidad(claveDe(item), -1)}>−</button>
                    {medidas ? (
                      <CantidadEditable
                        valor={item.cantidad}
                        onCambiar={(cantidad) => fijarCantidad(claveDe(item), cantidad)}
                        etiqueta={`Cantidad de ${item.nombre}`}
                      />
                    ) : (
                      <span>{item.cantidad}</span>
                    )}
                    <button onClick={() => cambiarCantidad(claveDe(item), 1)}>+</button>
                    <button className="pos-quitar" onClick={() => quitarDelCarrito(claveDe(item))}>🗑</button>
                  </div>
                ))}
            </div>
          ))}
        </div>

        <div className={`pos-resumen${esMixto ? ' pos-resumen-mixto' : ''}`}>
          {enCambio ? (
            <div className="pos-cambio-totales">
              <div>
                <span>Lo que se lleva</span>
                <span>S/ {total.toFixed(2)}</span>
              </div>
              <div>
                <span>A favor por lo devuelto</span>
                <span>− S/ {aFavor.toFixed(2)}</span>
              </div>
              {carrito.length > 0 && (
                <div className="pos-total-row">
                  <span>{aDevolver > 0 ? 'Se devuelve al cliente' : 'Diferencia a cobrar'}</span>
                  <span className="pos-total-monto">S/ {(aDevolver > 0 ? aDevolver : aCobrar).toFixed(2)}</span>
                </div>
              )}
            </div>
          ) : (
            <div className="pos-total-row">
              <span>Total</span>
              <span className="pos-total-monto">S/ {total.toFixed(2)}</span>
            </div>
          )}

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
            {!enCambio && (
              <button
                className={`pos-metodo-mixto${esMixto ? ' activo' : ''}`}
                onClick={() => setMetodoPago('MIXTO')}
              >
                {esMixto ? 'MIXTO · EFECTIVO + OTRO' : '+ MIXTO · EFECTIVO + OTRO'}
              </button>
            )}
            {credito && !enCambio && (
              <button className={`pos-metodo-mixto${esCredito ? ' activo' : ''}`} onClick={() => setMetodoPago('CREDITO')}>
                {esCredito ? 'CRÉDITO · PAGA DESPUÉS' : '+ CRÉDITO · PAGA DESPUÉS'}
              </button>
            )}
          </div>

          {esCredito && (
            <div className="pos-mixto">
              <span className="pos-mixto-titulo">Venta al crédito</span>
              {!cliente && <p className="pos-mixto-error">Elige arriba al cliente que va a deber.</p>}
              {deudaActual && deudaActual.saldo > 0 && (
                <p className="pos-credito-deuda">
                  Este cliente ya debe S/ {deudaActual.saldo.toFixed(2)}
                  {deudaActual.vencido > 0 ? ` (S/ ${deudaActual.vencido.toFixed(2)} vencido)` : ''}.
                </p>
              )}

              <div className="pos-mixto-paso">
                <span className="pos-mixto-label">1. ¿Deja un adelanto hoy? (opcional)</span>
                <div className={`pos-mixto-campo${creditoAdelanto !== '' && !adelantoValido ? ' error' : ''}`}>
                  <span>S/</span>
                  <input
                    type="number"
                    inputMode="decimal"
                    min="0"
                    step="0.01"
                    placeholder="0.00"
                    value={creditoAdelanto}
                    onChange={(e) => setCreditoAdelanto(e.target.value)}
                    aria-label="Adelanto"
                  />
                </div>
                {adelantoNum > 0 && (
                  <div className="pos-mixto-chips">
                    {METODOS_ADELANTO.map((m) => (
                      <button key={m} type="button" className={creditoAdelantoMetodo === m ? 'activo' : ''} onClick={() => setCreditoAdelantoMetodo(m)}>
                        {nombreMetodo(m)}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <div className="pos-mixto-paso">
                <span className="pos-mixto-label">2. Plazo para pagar</span>
                <div className="pos-mixto-chips">
                  {PLAZOS_CREDITO.map((d) => (
                    <button key={d} type="button" className={creditoDias === d ? 'activo' : ''} onClick={() => setCreditoDias(d)}>
                      {d} días
                    </button>
                  ))}
                </div>
              </div>

              <div className="pos-mixto-calculado">
                <span>Queda debiendo</span>
                <strong>S/ {Math.max(0, total - adelantoNum).toFixed(2)}</strong>
              </div>
              {creditoAdelanto !== '' && !adelantoValido && (
                <p className="pos-mixto-error">El adelanto debe ser menor al total. Si paga todo hoy, usa otro método.</p>
              )}
            </div>
          )}

          {esMixto && (
            <div className="pos-mixto">
              <span className="pos-mixto-titulo">Pago mixto</span>

              <div className="pos-mixto-paso">
                <span className="pos-mixto-label">1. ¿Con qué pagó la otra parte?</span>
                <div className="pos-mixto-chips">
                  {METODOS_OTRO_MIXTO.map((m) => (
                    <button
                      key={m}
                      type="button"
                      className={mixtoOtroMetodo === m ? 'activo' : ''}
                      onClick={() => setMixtoOtroMetodo(m)}
                    >
                      {nombreMetodo(m)}
                    </button>
                  ))}
                </div>
              </div>

              <div className="pos-mixto-paso">
                <span className="pos-mixto-label">
                  <span>2. Monto por {nombreMetodo(mixtoOtroMetodo)}</span>
                  <button
                    type="button"
                    className="pos-mixto-rapido"
                    onClick={() => setMixtoMontoOtro((Math.round(total * 50) / 100).toFixed(2))}
                  >
                    Mitad
                  </button>
                </span>
                <div className={`pos-mixto-campo${mixtoOtro >= total && mixtoOtro > 0 ? ' error' : ''}`}>
                  <span>S/</span>
                  <input
                    type="number"
                    inputMode="decimal"
                    min="0"
                    step="0.01"
                    placeholder="0.00"
                    value={mixtoMontoOtro}
                    onChange={(e) => setMixtoMontoOtro(e.target.value)}
                  />
                </div>
              </div>

              <div className="pos-mixto-calculado">
                <span>
                  3. Parte en efectivo <small>(automático)</small>
                </span>
                <strong>S/ {mixtoEfectivo.toFixed(2)}</strong>
              </div>

              <div className="pos-mixto-paso">
                <span className="pos-mixto-label">
                  <span>4. Efectivo que entregó el cliente</span>
                  <button
                    type="button"
                    className="pos-mixto-rapido"
                    onClick={() => setMontoRecibido(mixtoEfectivo.toFixed(2))}
                    disabled={mixtoOtro <= 0 || mixtoOtro >= total}
                  >
                    Exacto
                  </button>
                </span>
                <div
                  className={`pos-mixto-campo${
                    montoRecibido !== '' && recibidoNum + 0.005 < mixtoEfectivo ? ' error' : ''
                  }`}
                >
                  <span>S/</span>
                  <input
                    type="number"
                    inputMode="decimal"
                    min="0"
                    step="0.01"
                    placeholder="0.00"
                    value={montoRecibido}
                    onChange={(e) => setMontoRecibido(e.target.value)}
                  />
                </div>
              </div>

              {cambio > 0 && <span className="pos-cambio">Cambio: S/ {cambio.toFixed(2)}</span>}

              {total > 0 && mixtoOtro > 0 && mixtoOtro < total && (
                <>
                  <div className="pos-mixto-barra">
                    <div className="pos-mixto-barra-otro" style={{ width: `${(mixtoOtro / total) * 100}%` }} />
                    <div
                      className="pos-mixto-barra-efectivo"
                      style={{ width: `${(Math.min(recibidoNum, mixtoEfectivo) / total) * 100}%` }}
                    />
                  </div>
                  <div className="pos-mixto-leyenda">
                    <span>
                      <i className="pos-mixto-punto-otro" />
                      {nombreMetodo(mixtoOtroMetodo)} S/ {mixtoOtro.toFixed(2)}
                    </span>
                    <span>
                      <i className="pos-mixto-punto-efectivo" />
                      Efectivo S/ {mixtoEfectivo.toFixed(2)}
                    </span>
                  </div>
                </>
              )}

              {estadoMixto.error && <p className="pos-mixto-error">{estadoMixto.error}</p>}
              {estadoMixto.listo && <p className="pos-mixto-ok">✓ Cuadra con el total</p>}
            </div>
          )}

          {metodoPago === 'EFECTIVO' && !(enCambio && (aCobrar === 0 || carrito.length === 0)) && (
            <div className="pos-efectivo">
              <input
                type="number"
                placeholder="Monto recibido"
                value={montoRecibido}
                onChange={(e) => setMontoRecibido(e.target.value)}
              />
              <span className="pos-cambio">{enCambio ? 'Vuelto' : 'Cambio'}: S/ {cambio.toFixed(2)}</span>
            </div>
          )}
          {enCambio && carrito.length > 0 && aDevolver > 0 && (
            <p className="pos-cambio-nota">
              Lo nuevo vale menos: entrega S/ {aDevolver.toFixed(2)} al cliente por {nombreMetodo(metodoPago)}.
            </p>
          )}

          {mensaje && <p className={`pos-mensaje pos-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

          {ultimaVentaParaImprimir && !mostrarModalVenta && (
            <button className="pos-imprimir" onClick={imprimirComprobante} disabled={ultimaVentaParaImprimir.comprobantePendiente}>
              {ultimaVentaParaImprimir.comprobantePendiente
                ? `Emitiendo comprobante · ${ultimaVentaParaImprimir.venta.folio}`
                : `Imprimir última boleta · ${ultimaVentaParaImprimir.venta.folio}`}
            </button>
          )}

          {cotizaciones && carrito.length > 0 && !cobrandoPedido && !cotizacionEnVenta && !enCambio && (
            <button
              type="button"
              className="pos-cotizar"
              onClick={() => setNuevaCotizacion({ nombre: '', validez: 7, notas: '' })}
              disabled={procesando}
            >
              Guardar como cotización
            </button>
          )}

          <button className="pos-cobrar" disabled={!puedeCobrar} onClick={procesarVenta}>
            {procesando
              ? 'Procesando...'
              : enCambio
                ? carrito.length === 0
                  ? 'Elige lo que se lleva'
                  : aDevolver > 0
                  ? `Hacer el cambio · devolver S/ ${aDevolver.toFixed(2)}`
                  : aCobrar > 0
                    ? `Hacer el cambio · cobrar S/ ${aCobrar.toFixed(2)}`
                    : 'Hacer el cambio'
                : esCredito
                  ? `Registrar crédito S/ ${total.toFixed(2)}`
                  : `Cobrar S/ ${total.toFixed(2)}`}
          </button>
        </div>
      </div>

      {nuevaCotizacion && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !guardandoCotizacion && setNuevaCotizacion(null)}>
          <div className="pm-modal pm-modal-chico" role="dialog" aria-modal="true" aria-label="Guardar cotización">
            <div className="pm-modal-cabecera">
              <h2>Guardar cotización</h2>
              <p>
                {carrito.length} {carrito.length === 1 ? 'línea' : 'líneas'} · S/ {total.toFixed(2)}. No descuenta stock ni entra a caja.
              </p>
            </div>
            <div className="pm-modal-cuerpo">
              <div className="pm-campos">
                <label className="pm-campo pm-campo-ancho">
                  <span>Cliente</span>
                  {cliente ? (
                    <input value={cliente.nombre_razon_social} readOnly />
                  ) : (
                    <input
                      autoFocus
                      placeholder="Nombre del cliente (opcional)"
                      maxLength={120}
                      value={nuevaCotizacion.nombre}
                      onChange={(e) => setNuevaCotizacion({ ...nuevaCotizacion, nombre: e.target.value })}
                    />
                  )}
                </label>
                <div className="pm-campo pm-campo-ancho">
                  <span>Válida por</span>
                  <div className="pm-opciones">
                    {[7, 15, 30].map((d) => (
                      <button
                        key={d}
                        type="button"
                        className={nuevaCotizacion.validez === d ? 'activo' : ''}
                        onClick={() => setNuevaCotizacion({ ...nuevaCotizacion, validez: d })}
                      >
                        {d} días
                      </button>
                    ))}
                  </div>
                </div>
                <label className="pm-campo pm-campo-ancho">
                  <span>Nota (opcional)</span>
                  <input
                    placeholder="Incluye corte, entrega en obra..."
                    maxLength={500}
                    value={nuevaCotizacion.notas}
                    onChange={(e) => setNuevaCotizacion({ ...nuevaCotizacion, notas: e.target.value })}
                  />
                </label>
              </div>
            </div>
            <div className="pm-modal-pie">
              <button className="pm-boton-secundario" onClick={() => setNuevaCotizacion(null)} disabled={guardandoCotizacion}>
                Cancelar
              </button>
              <button className="pm-boton" onClick={guardarCotizacion} disabled={guardandoCotizacion}>
                {guardandoCotizacion ? 'Guardando...' : 'Guardar cotización'}
              </button>
            </div>
          </div>
        </div>
      )}

      {cotizacionGuardada && (
        <div className="pos-venta-modal-overlay">
          <div className="pos-venta-modal">
            <div className="pos-venta-modal-check">✓</div>
            <h2>Cotización guardada</h2>
            <p className="pos-venta-modal-folio">N° {numeroCotizacion(cotizacionGuardada.numero)}</p>
            <p className="pos-venta-modal-tipo">{cotizacionGuardada.cliente_nombre || 'Sin cliente'}</p>
            <div className="pos-venta-modal-linea">
              <span>Total</span>
              <strong>S/ {cotizacionGuardada.total.toFixed(2)}</strong>
            </div>
            <div className="pos-venta-modal-linea">
              <span>Válida por</span>
              <strong>{cotizacionGuardada.validez_dias} días</strong>
            </div>
            <div className="pos-venta-modal-acciones">
              <button className="pos-venta-modal-imprimir" onClick={() => window.print()}>
                Imprimir cotización
              </button>
              <button className="pos-venta-modal-cerrar" onClick={() => setCotizacionGuardada(null)}>
                Cerrar
              </button>
            </div>
          </div>
        </div>
      )}

      {productoAMedir && (
        <CalculadoraPieTablar
          producto={productoAMedir}
          yaEnCarrito={carrito.filter((i) => i.id === productoAMedir.id).reduce((s, i) => s + i.cantidad, 0)}
          onAgregar={agregarMedida}
          onCerrar={() => setProductoAMedir(null)}
        />
      )}

      {modeloAElegir && (
        <SelectorTalla
          grupo={modeloAElegir}
          enCarrito={Object.fromEntries(
            modeloAElegir.variantes.map((v) => [
              v.id,
              carrito.filter((i) => i.id === v.id).reduce((suma, i) => suma + i.cantidad, 0),
            ])
          )}
          onElegir={(variante) => {
            agregarAlCarrito(variante);
            setModeloAElegir(null);
          }}
          onCerrar={() => setModeloAElegir(null)}
        />
      )}

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
            <h2>{ultimaVentaParaImprimir.venta.cambioPrenda ? 'Cambio registrado' : 'Venta registrada'}</h2>
            <p className="pos-venta-modal-folio">{ultimaVentaParaImprimir.venta.folio}</p>

            <p className="pos-venta-modal-tipo">
              {ultimaVentaParaImprimir.comprobante
                ? `${ultimaVentaParaImprimir.comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${ultimaVentaParaImprimir.comprobante.serie}-${String(ultimaVentaParaImprimir.comprobante.numero).padStart(6, '0')}`
                : ultimaVentaParaImprimir.comprobantePendiente
                  ? `Emitiendo ${ultimaVentaParaImprimir.tipoComprobante === 'FACTURA' ? 'factura' : 'boleta'}…`
                  : ultimaVentaParaImprimir.errorComprobante
                    ? 'Sin comprobante'
                    : 'Nota de venta'}
            </p>
            {ultimaVentaParaImprimir.errorComprobante && (
              <p className="pos-venta-modal-error">
                La venta quedó registrada, pero no se pudo emitir el comprobante: {ultimaVentaParaImprimir.errorComprobante}
              </p>
            )}

            <div className="pos-venta-modal-linea">
              <span>{ultimaVentaParaImprimir.venta.cambioPrenda ? 'Lo que se lleva' : 'Total'}</span>
              <strong>S/ {ultimaVentaParaImprimir.venta.total.toFixed(2)}</strong>
            </div>
            {ultimaVentaParaImprimir.venta.cambioPrenda && (
              <>
                <div className="pos-venta-modal-linea">
                  <span>A favor por lo devuelto</span>
                  <strong>− S/ {ultimaVentaParaImprimir.venta.cambioPrenda.valor.toFixed(2)}</strong>
                </div>
                <div className="pos-venta-modal-linea">
                  <span>{ultimaVentaParaImprimir.venta.cambioPrenda.aDevolver > 0 ? 'Devuelto al cliente' : 'Diferencia cobrada'}</span>
                  <strong>
                    S/{' '}
                    {(ultimaVentaParaImprimir.venta.cambioPrenda.aDevolver > 0
                      ? ultimaVentaParaImprimir.venta.cambioPrenda.aDevolver
                      : ultimaVentaParaImprimir.venta.cambioPrenda.aCobrar
                    ).toFixed(2)}
                  </strong>
                </div>
              </>
            )}
            {ultimaVentaParaImprimir.venta.cambio != null && (
              <div className="pos-venta-modal-linea">
                <span>{ultimaVentaParaImprimir.venta.cambioPrenda ? 'Vuelto' : 'Cambio'}</span>
                <strong>S/ {ultimaVentaParaImprimir.venta.cambio.toFixed(2)}</strong>
              </div>
            )}
            {ultimaVentaParaImprimir.venta.credito && (
              <div className="pos-venta-modal-linea">
                <span>Al crédito · queda debiendo</span>
                <strong>S/ {ultimaVentaParaImprimir.venta.credito.saldo.toFixed(2)}</strong>
              </div>
            )}

            <div className="pos-venta-modal-whatsapp">
              <input
                type="tel"
                placeholder="WhatsApp del cliente (opcional)"
                value={telefonoWhatsapp}
                onChange={(e) => setTelefonoWhatsapp(e.target.value)}
              />
              <button
                className="pos-venta-modal-whatsapp-boton"
                onClick={enviarPorWhatsapp}
                disabled={ultimaVentaParaImprimir.comprobantePendiente}
              >
                📲 Enviar por WhatsApp
              </button>
            </div>

            <div className="pos-venta-modal-acciones">
              <button
                className="pos-venta-modal-imprimir"
                onClick={imprimirComprobante}
                disabled={ultimaVentaParaImprimir.comprobantePendiente}
              >
                {ultimaVentaParaImprimir.comprobantePendiente ? 'Emitiendo comprobante…' : '🖨 Imprimir'}
              </button>
              <button
                className="pos-venta-modal-cerrar"
                onClick={() => {
                  setMostrarModalVenta(false);
                  setCarritoAbierto(false);
                  if (ultimaVentaParaImprimir.deMesa && onVolverAMesas) {
                    onVolverAMesas();
                    return;
                  }
                  buscadorRef.current?.focus();
                }}
              >
                {ultimaVentaParaImprimir.deMesa && onVolverAMesas ? 'Volver a mesas' : 'Nueva venta'}
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

      {/* Mientras se muestra una cotización recién guardada, lo que se
          imprime es la cotización y no el ticket de la última venta. */}
      {cotizacionGuardada && (
        <CotizacionImprimible cotizacion={cotizacionGuardada} nombreTienda={nombreTienda} direccion={direccion} telefono={telefono} ruc={ruc} />
      )}

      {ultimaVentaParaImprimir && !cotizacionGuardada && (
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
          diasCambio={cambios ? diasCambio : 0}
        />
      )}
    </div>
  );
}