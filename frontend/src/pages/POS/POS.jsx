import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { api, API_URL } from '../../api/api';
import './POS.css';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';

const DEBOUNCE_BUSQUEDA_VIVA_MS = 400;

const REGLAS_DOCUMENTO = {
  DNI: { maxLength: 8, soloNumeros: true, label: 'DNI (8 dígitos)' },
  CE: { maxLength: 12, soloNumeros: false, label: 'Carnet de Extranjería' },
  PASAPORTE: { maxLength: 12, soloNumeros: false, label: 'Pasaporte' },
  RUC: { maxLength: 11, soloNumeros: true, label: 'RUC (11 dígitos)' },
};

export default function POS({ usuario, nombreTienda = 'Mi Minimarket' }) {
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
  const [pdfVisible, setPdfVisible] = useState(null);
  const iframePdfRef = useRef(null);

  const [nuevoTipoDocumento, setNuevoTipoDocumento] = useState('DNI');
  const [nuevoDocumento, setNuevoDocumento] = useState('');
  const [nuevoNombre, setNuevoNombre] = useState('');
  const [creandoCliente, setCreandoCliente] = useState(false);
  const [errorCliente, setErrorCliente] = useState('');

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

  // Si FacturaLibre emitió de verdad el comprobante (con su PDF oficial
  // con logo/QR), lo mostramos incrustado en un modal propio del sistema
  // (nada de pestaña/ventana nueva, que se cruza con el flujo del POS) —
  // desde ahí mismo se imprime con un botón. Sin PDF real (nota simple),
  // caemos al ticket casero de siempre con window.print().
  const imprimirComprobante = () => {
    const comp = ultimaVentaParaImprimir?.comprobante;
    if (comp?.enlace_pdf && comp?.comprobante_id) {
      setPdfVisible(api.comprobantePdfUrl(comp.comprobante_id));
    } else {
      window.print();
    }
  };

  const imprimirPdfEmbebido = () => {
    iframePdfRef.current?.contentWindow?.print();
  };

  const mostrarFormularioNuevo = sinResultadosCliente && !buscandoCliente;
  const mostrarSeccionCliente = clienteEsObligatorio || mostrarBusquedaCliente || cliente;

  return (
    <div className="pos-layout">
      <div className="pos-productos">
        <input
          ref={buscadorRef}
          className="pos-buscador"
          type="text"
          placeholder="Escanea o busca por nombre / código..."
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
          onKeyDown={manejarEnterBusquedaProducto}
          autoFocus
        />
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
                      placeholder={tipoComprobante === 'FACTURA' ? 'Razón social' : 'Nombre completo'}
                      value={nuevoNombre}
                      onChange={(e) => setNuevoNombre(e.target.value)}
                      autoFocus
                    />
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
            <div className="pos-pdf-modal-header">
              <h2>Comprobante</h2>
              <button
                type="button"
                className="pos-carrito-cerrar"
                onClick={() => setPdfVisible(null)}
                aria-label="Cerrar"
              >
                ×
              </button>
            </div>
            <p className="pos-pdf-modal-ayuda">
              Para imprimir, usa el ícono 🖨 que trae el visor de PDF arriba del documento.
            </p>
            <p className="pos-pdf-modal-movil">
              📄 Toca "Imprimir" abajo para ver el comprobante y enviarlo a imprimir.
            </p>
            <iframe ref={iframePdfRef} src={pdfVisible} title="Comprobante" />
            <div className="pos-pdf-modal-acciones">
              <button className="pos-pdf-modal-cerrar" onClick={() => setPdfVisible(null)}>
                Cerrar
              </button>
              <a
                className="pos-pdf-modal-imprimir"
                href={pdfVisible}
                target="_blank"
                rel="noopener noreferrer"
              >
                🖨 Imprimir
              </a>
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
          cajero={usuario.nombre}
        />
      )}
    </div>
  );
}