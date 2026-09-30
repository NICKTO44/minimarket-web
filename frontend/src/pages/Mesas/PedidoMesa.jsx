import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowLeft,
  ArrowRightLeft,
  Ban,
  Bell,
  Check,
  CheckCheck,
  ChefHat,
  ChevronRight,
  Minus,
  Plus,
  Printer,
  ReceiptText,
  Send,
  StickyNote,
  Trash2,
  X,
} from 'lucide-react';
import { api, API_URL } from '../../api/api';
import { confirmar } from '../../utils/confirmar';
import { esMesero } from '../../utils/menu';
import { tiempoAbierto, tituloPedido } from '../../utils/mesas';
import TicketMesa from '../../components/TicketMesa';
import ModalOpciones from './ModalOpciones';
import DialogoTexto from './DialogoTexto';
import { usePedidoOptimista } from './usePedidoOptimista';

const TODAS = '__todas__';

// Cada cuánto se trae lo último (barra/cocina marca productos listos).
const REFRESCO_MS = 6000;

/**
 * Pedido de una mesa (o para llevar): a la izquierda la carta, a la
 * derecha la cuenta. Lo nuevo queda "Por enviar" hasta que se manda a
 * preparar (sale la comanda); lo enviado ya no se edita, solo se anula.
 */
export default function PedidoMesa({ pedidoId, usuario, nombreTienda, mesas, onVolver, onCobrar }) {
  const mesero = esMesero(usuario);
  const [productos, setProductos] = useState([]);
  const [grupos, setGrupos] = useState([]);
  const [busqueda, setBusqueda] = useState('');
  const [categoria, setCategoria] = useState(TODAS);
  const [error, setError] = useState(null);
  const [aviso, setAviso] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const [productoConOpciones, setProductoConOpciones] = useState(null);
  const [dialogo, setDialogo] = useState(null);
  const [ticket, setTicket] = useState(null);
  const [moverAbierto, setMoverAbierto] = useState(false);
  const [cuentaAbierta, setCuentaAbierta] = useState(false);
  const [imagenesFallidas, setImagenesFallidas] = useState(() => new Set());
  // Cambia en cada impresión para volver a dibujar (e imprimir) el ticket.
  const numeroTicket = useRef(0);

  // Los cambios se ven al instante y se guardan en segundo plano.
  const {
    detalle,
    cargar,
    refrescar,
    guardando,
    sincronizar,
    agregar,
    cambiarCantidad,
    cambiarNota,
    quitarPendiente,
    aplicarServidor,
  } = usePedidoOptimista(pedidoId, setError);

  useEffect(() => {
    Promise.all([cargar(), api.productos(), api.modificadores()])
      .then(([, p, g]) => {
        setProductos(p);
        setGrupos(g);
      })
      .catch((e) => setError(e.message));
  }, [cargar]);

  useEffect(() => {
    const intervalo = setInterval(() => refrescar().catch(() => {}), REFRESCO_MS);
    return () => clearInterval(intervalo);
  }, [refrescar]);

  // Imprime el ticket apenas se dibuja (comanda o precuenta).
  useEffect(() => {
    if (!ticket) return undefined;
    const t = setTimeout(() => window.print(), 80);
    return () => clearTimeout(t);
  }, [ticket]);

  useEffect(() => {
    if (!aviso) return undefined;
    const t = setTimeout(() => setAviso(null), 2500);
    return () => clearTimeout(t);
  }, [aviso]);

  const gruposPorProducto = useMemo(() => {
    const mapa = new Map();
    grupos.forEach((g) =>
      g.producto_ids.forEach((pid) => {
        if (!mapa.has(pid)) mapa.set(pid, []);
        mapa.get(pid).push(g);
      })
    );
    return mapa;
  }, [grupos]);

  const categorias = useMemo(() => {
    const set = new Set(productos.map((p) => p.categoria_nombre).filter(Boolean));
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [productos]);

  const productosVisibles = useMemo(() => {
    const q = busqueda.trim().toLowerCase();
    return productos.filter(
      (p) =>
        (categoria === TODAS || p.categoria_nombre === categoria) &&
        (!q || p.nombre.toLowerCase().includes(q) || p.codigo.toLowerCase().includes(q))
    );
  }, [productos, busqueda, categoria]);

  const items = detalle?.items || [];
  const pendientes = items.filter((i) => i.estado === 'PENDIENTE');
  const enviados = items.filter((i) => i.estado === 'ENVIADO');
  // Etapas de lo enviado: en preparación -> listo (barra/cocina) -> entregado.
  const preparando = enviados.filter((i) => !i.fecha_listo);
  const listos = enviados.filter((i) => i.fecha_listo && !i.fecha_entregado);
  const entregados = enviados.filter((i) => i.fecha_entregado);
  const anulados = items.filter((i) => i.estado === 'ANULADO');
  const activos = [...enviados, ...pendientes];
  const total = activos.reduce((s, i) => s + i.subtotal, 0);
  const unidades = activos.reduce((s, i) => s + i.cantidad, 0);

  // Para acciones que sí esperan al servidor (enviar, mover, anular).
  const ejecutar = async (accion, textoOk) => {
    setOcupado(true);
    setError(null);
    try {
      await sincronizar();
      const r = await accion();
      if (textoOk) setAviso(textoOk);
      return r;
    } catch (e) {
      setError(e.message);
      return null;
    } finally {
      setOcupado(false);
    }
  };

  const tocarProducto = (producto) => {
    setError(null);
    if (gruposPorProducto.has(producto.id)) {
      setProductoConOpciones(producto);
      return;
    }
    // Sin opciones: si ya hay una línea igual sin enviar, se le suma 1.
    const igual = pendientes.find((i) => i.producto_id === producto.id && !i.opciones && !i.nota);
    if (igual) {
      cambiarCantidad(igual, igual.cantidad + 1);
    } else {
      agregar({
        producto_id: producto.id,
        nombre_producto: producto.nombre,
        precio_unitario: producto.precio,
        cantidad: 1,
        opcion_ids: [],
        nota: null,
      });
    }
  };

  // Desde la ventana de opciones: el texto y el precio se arman aquí para
  // mostrarlos al instante; el servidor los vuelve a calcular al guardar.
  const agregarConOpciones = (producto, item) => {
    const elegidas = (gruposPorProducto.get(producto.id) || []).flatMap((g) =>
      g.opciones.filter((o) => item.opcion_ids.includes(o.id))
    );
    agregar({
      producto_id: producto.id,
      nombre_producto: producto.nombre,
      precio_unitario: producto.precio + elegidas.reduce((s, o) => s + o.precio_extra, 0),
      opciones: elegidas.map((o) => o.nombre).join(', ') || null,
      cantidad: item.cantidad,
      opcion_ids: item.opcion_ids,
      nota: item.nota,
    });
    setProductoConOpciones(null);
  };

  const cambiarCantidadLinea = (item, cantidad) => {
    if (cantidad < 1) {
      quitar(item);
      return;
    }
    cambiarCantidad(item, cantidad);
  };

  const quitar = (item) => {
    if (item.estado === 'PENDIENTE') {
      quitarPendiente(item);
      return;
    }
    setDialogo({
      titulo: `Anular ${item.cantidad} × ${item.nombre_producto}`,
      mensaje: 'Ya se mandó a preparar. Queda registrado quién lo anuló y por qué.',
      placeholder: 'Motivo (ej. el cliente cambió de pedido)',
      obligatorio: true,
      peligro: true,
      textoConfirmar: 'Anular',
      alConfirmar: async (motivo) => {
        await sincronizar();
        aplicarServidor(await api.pedidoQuitarItem(pedidoId, item.id, motivo));
      },
    });
  };

  const editarNota = (item) => {
    setDialogo({
      titulo: `Nota para ${item.nombre_producto}`,
      mensaje: 'Sale en la comanda de barra/cocina.',
      placeholder: 'Ej. sin azúcar, bien caliente',
      valorInicial: item.nota || '',
      textoConfirmar: 'Guardar nota',
      alConfirmar: async (nota) => cambiarNota(item, nota),
    });
  };

  const enviar = async () => {
    const comanda = await ejecutar(() => api.pedidoEnviar(pedidoId), 'Enviado a preparar');
    if (!comanda) return;
    setTicket({ tipo: 'COMANDA', pedido: comanda.pedido, items: comanda.items, clave: ++numeroTicket.current });
    cargar().catch(() => {});
  };

  const entregar = async (lineas) => {
    const r = await ejecutar(() => api.preparacionEntregado(lineas.map((i) => i.id)), 'Entregado');
    if (r) cargar().catch(() => {});
  };

  const imprimirPrecuenta = () => {
    setTicket({ tipo: 'PRECUENTA', pedido: detalle, items: activos, clave: ++numeroTicket.current });
  };

  const reimprimirComanda = () => {
    setTicket({ tipo: 'COMANDA', pedido: detalle, items: enviados, clave: ++numeroTicket.current });
  };

  const anularPedido = () => {
    const hayEnviados = enviados.length > 0;
    setDialogo({
      titulo: `¿Anular ${tituloPedido(detalle)}?`,
      mensaje: hayEnviados
        ? 'Ya se preparó parte del pedido. Escribe el motivo; queda registrado.'
        : 'La mesa queda libre y el pedido se descarta.',
      placeholder: 'Motivo',
      obligatorio: hayEnviados,
      peligro: true,
      textoConfirmar: 'Anular pedido',
      alConfirmar: async (motivo) => {
        await sincronizar();
        await api.pedidoAnular(pedidoId, motivo);
        onVolver();
      },
    });
  };

  const moverA = async (mesa) => {
    const d = await ejecutar(() => api.pedidoMover(pedidoId, mesa.id), `Pedido movido a ${mesa.nombre}`);
    if (d) {
      aplicarServidor(d);
      setMoverAbierto(false);
    }
  };

  // Antes de salir se termina de guardar lo que falte.
  const volver = () => {
    sincronizar().finally(onVolver);
  };

  const cobrar = async () => {
    if (activos.length === 0) return;
    if (pendientes.length > 0) {
      const ok = await confirmar({
        titulo: `Hay ${pendientes.length} producto(s) sin enviar a preparar`,
        mensaje: 'Si igual se cobran (por ejemplo, una bebida embotellada), continúa. Si no, envíalos primero.',
        textoConfirmar: 'Cobrar igual',
        icono: 'aviso',
      });
      if (!ok) return;
    }
    // Se cobra exactamente lo que quedó guardado en el servidor.
    const actual = await ejecutar(() => api.pedido(pedidoId));
    if (!actual) return;
    const lineas = actual.items.filter((i) => i.estado !== 'ANULADO');
    if (lineas.length === 0) return;
    onCobrar({ ...actual, items: lineas });
  };

  if (!detalle) {
    return (
      <div className="pedido-layout">
        <p className="mesas-vacio">{error || 'Cargando pedido...'}</p>
        {error && (
          <button type="button" className="mesas-boton-secundario" onClick={onVolver}>
            <ArrowLeft size={16} /> Volver a mesas
          </button>
        )}
      </div>
    );
  }

  const mesasLibres = mesas.filter((m) => !m.pedido && m.id !== detalle.mesa_id);

  const filaItem = (item) => {
    const editable = item.estado === 'PENDIENTE';
    const esListo = item.estado === 'ENVIADO' && item.fecha_listo && !item.fecha_entregado;
    const puedeAnular = item.estado === 'ENVIADO' && !item.fecha_entregado && !mesero;
    return (
      <div
        key={item.id}
        className={`pedido-item pedido-item-${item.estado.toLowerCase()}${item.fecha_entregado ? ' pedido-item-entregado' : ''}`}
      >
        <div className="pedido-item-info">
          <span className="pedido-item-nombre">
            {!editable && <span className="pedido-item-cant-fija">{item.cantidad}×</span>}
            {item.nombre_producto}
          </span>
          {item.opciones && <span className="pedido-item-opciones">{item.opciones}</span>}
          {item.nota && <span className="pedido-item-nota">“{item.nota}”</span>}
          <span className="pedido-item-precio">
            S/ {item.precio_unitario.toFixed(2)} c/u · <strong>S/ {item.subtotal.toFixed(2)}</strong>
          </span>
        </div>
        {editable && (
          <div className="pedido-item-controles">
            <button type="button" onClick={() => editarNota(item)} aria-label="Nota" title="Nota para cocina">
              <StickyNote size={15} />
            </button>
            <button type="button" onClick={() => cambiarCantidadLinea(item, item.cantidad - 1)} aria-label="Menos">
              {item.cantidad === 1 ? <Trash2 size={15} /> : <Minus size={15} />}
            </button>
            <span>{item.cantidad}</span>
            <button type="button" onClick={() => cambiarCantidadLinea(item, item.cantidad + 1)} aria-label="Más">
              <Plus size={15} />
            </button>
          </div>
        )}
        {esListo && (
          <button type="button" className="pedido-item-entregar" onClick={() => entregar([item])} disabled={ocupado}>
            <Check size={14} /> Entregado
          </button>
        )}
        {puedeAnular && (
          <button type="button" className="pedido-item-anular" onClick={() => quitar(item)} title="Anular">
            <X size={15} />
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="pedido-layout">
      {/* ---------- Carta ---------- */}
      <div className="pedido-carta">
        <div className="pedido-carta-top">
          <button type="button" className="mesas-boton-secundario pedido-volver" onClick={volver}>
            <ArrowLeft size={16} /> Mesas
          </button>
          <input
            className="mesas-input pedido-buscador"
            placeholder="Buscar producto..."
            value={busqueda}
            onChange={(e) => setBusqueda(e.target.value)}
          />
        </div>
        <div className="pedido-categorias">
          <button type="button" className={categoria === TODAS ? 'activo' : ''} onClick={() => setCategoria(TODAS)}>
            Todo
          </button>
          {categorias.map((c) => (
            <button key={c} type="button" className={categoria === c ? 'activo' : ''} onClick={() => setCategoria(c)}>
              {c}
            </button>
          ))}
        </div>
        <div className="pedido-productos">
          {productosVisibles.map((p) => {
            const tieneOpciones = gruposPorProducto.has(p.id);
            const conImagen = p.imagen_url && !imagenesFallidas.has(p.id);
            return (
              <button key={p.id} type="button" className="pedido-producto" onClick={() => tocarProducto(p)}>
                {conImagen ? (
                  <img
                    src={`${API_URL}${p.imagen_url}`}
                    alt=""
                    onError={() => setImagenesFallidas((s) => new Set(s).add(p.id))}
                  />
                ) : (
                  <span className="pedido-producto-inicial">{p.nombre.charAt(0).toUpperCase()}</span>
                )}
                <span className="pedido-producto-nombre">{p.nombre}</span>
                <span className="pedido-producto-fila">
                  <span className="pedido-producto-precio">S/ {p.precio.toFixed(2)}</span>
                  {tieneOpciones && <span className="pedido-producto-opciones">opciones</span>}
                </span>
              </button>
            );
          })}
          {productosVisibles.length === 0 && <p className="mesas-vacio">No hay productos con ese nombre.</p>}
        </div>
      </div>

      {/* Celular: barra para ver la cuenta */}
      <button type="button" className="pedido-barra-movil" onClick={() => setCuentaAbierta(true)}>
        <span className="pedido-barra-titulo">{tituloPedido(detalle)}</span>
        <span className="pedido-barra-detalle">
          {pendientes.length > 0 && <span className="mesa-card-alerta">{pendientes.length} por enviar</span>}
          S/ {total.toFixed(2)} <ChevronRight size={16} />
        </span>
      </button>

      {/* ---------- Cuenta ---------- */}
      <div className={`pedido-cuenta${cuentaAbierta ? ' abierta' : ''}`}>
        <div className="pedido-cuenta-header">
          <div>
            <h2>{tituloPedido(detalle)}</h2>
            <span className="pedido-cuenta-meta">
              #{detalle.id} · {tiempoAbierto(detalle.minutos_abierto)}
              {detalle.mesero ? ` · ${detalle.mesero}` : ''}
              {guardando && <span className="pedido-guardando"> · Guardando…</span>}
            </span>
          </div>
          <button type="button" className="pedido-cerrar-movil" onClick={() => setCuentaAbierta(false)} aria-label="Cerrar">
            <X size={20} />
          </button>
        </div>

        <div className="pedido-cuenta-items">
          {activos.length === 0 && (
            <p className="pedido-vacio">Toca los productos de la carta para agregarlos al pedido.</p>
          )}
          {listos.length > 0 && (
            <div className="pedido-bloque-listo">
              <span className="pedido-seccion pedido-seccion-listo">
                <Bell size={13} /> Listo para entregar
              </span>
              {listos.map(filaItem)}
              {listos.length > 1 && (
                <button type="button" className="pedido-entregar-todo" onClick={() => entregar(listos)} disabled={ocupado}>
                  <CheckCheck size={16} /> Entregar todo lo listo
                </button>
              )}
            </div>
          )}
          {pendientes.length > 0 && (
            <>
              <span className="pedido-seccion pedido-seccion-pendiente">Por enviar a preparar</span>
              {pendientes.map(filaItem)}
            </>
          )}
          {preparando.length > 0 && (
            <>
              <span className="pedido-seccion pedido-seccion-preparando">
                <ChefHat size={13} /> En preparación
              </span>
              {preparando.map(filaItem)}
            </>
          )}
          {entregados.length > 0 && (
            <>
              <span className="pedido-seccion">
                <Check size={13} /> Entregado
              </span>
              {entregados.map(filaItem)}
            </>
          )}
          {anulados.length > 0 && (
            <details className="pedido-anulados">
              <summary>{anulados.length} anulado(s)</summary>
              {anulados.map(filaItem)}
            </details>
          )}
        </div>

        <div className="pedido-cuenta-pie">
          {error && <p className="mesas-error">{error}</p>}
          {aviso && <p className="mesas-aviso">{aviso}</p>}
          <div className="pedido-total">
            <span>
              Total <small>({unidades} {unidades === 1 ? 'producto' : 'productos'})</small>
            </span>
            <strong>S/ {total.toFixed(2)}</strong>
          </div>

          <button
            type="button"
            className="mesas-boton-primario pedido-enviar"
            onClick={enviar}
            disabled={ocupado || pendientes.length === 0}
          >
            <Send size={16} /> Enviar a preparar{pendientes.length > 0 ? ` (${pendientes.length})` : ''}
          </button>

          <div className="pedido-acciones">
            <button type="button" onClick={imprimirPrecuenta} disabled={activos.length === 0}>
              <ReceiptText size={15} /> Precuenta
            </button>
            <button type="button" onClick={reimprimirComanda} disabled={enviados.length === 0}>
              <Printer size={15} /> Comanda
            </button>
            {detalle.tipo === 'MESA' && (
              <button type="button" onClick={() => setMoverAbierto(true)}>
                <ArrowRightLeft size={15} /> Mover
              </button>
            )}
            {!(mesero && enviados.length > 0) && (
              <button type="button" className="peligro" onClick={anularPedido}>
                <Ban size={15} /> Anular
              </button>
            )}
          </div>

          {!mesero && (
            <button
              type="button"
              className="pedido-cobrar"
              onClick={cobrar}
              disabled={ocupado || activos.length === 0}
            >
              Cobrar S/ {total.toFixed(2)}
            </button>
          )}
        </div>
      </div>

      {productoConOpciones && (
        <ModalOpciones
          producto={productoConOpciones}
          grupos={gruposPorProducto.get(productoConOpciones.id) || []}
          onCerrar={() => setProductoConOpciones(null)}
          onAgregar={(item) => agregarConOpciones(productoConOpciones, item)}
        />
      )}

      {moverAbierto && (
        <div className="mesas-modal-velo" onMouseDown={(e) => e.target === e.currentTarget && setMoverAbierto(false)}>
          <div className="mesas-modal mesas-modal-chico">
            <h2>Mover a otra mesa</h2>
            <p className="mesas-modal-mensaje">El pedido completo pasa a la mesa que elijas.</p>
            {mesasLibres.length === 0 ? (
              <p className="mesas-vacio">No hay mesas libres.</p>
            ) : (
              <div className="pedido-mover-grid">
                {mesasLibres.map((m) => (
                  <button key={m.id} type="button" onClick={() => moverA(m)} disabled={ocupado}>
                    {m.nombre}
                    <small>{m.zona}</small>
                  </button>
                ))}
              </div>
            )}
            <div className="mesas-modal-acciones">
              <button type="button" className="mesas-boton-secundario" onClick={() => setMoverAbierto(false)}>
                Cancelar
              </button>
            </div>
          </div>
        </div>
      )}

      {dialogo && <DialogoTexto {...dialogo} onCerrar={() => setDialogo(null)} />}

      {ticket && (
        <TicketMesa
          key={ticket.clave}
          tipo={ticket.tipo}
          pedido={ticket.pedido}
          items={ticket.items}
          nombreTienda={nombreTienda}
        />
      )}
    </div>
  );
}
