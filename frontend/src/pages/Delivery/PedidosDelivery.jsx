import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  BarChart3,
  Bike,
  Check,
  ChefHat,
  Clock,
  Hourglass,
  MapPin,
  MessageCircle,
  Minus,
  Pencil,
  Plus,
  Printer,
  RefreshCw,
  Search,
  Store,
  Trash2,
  Undo2,
  UtensilsCrossed,
  Wallet,
  X,
} from 'lucide-react';
import { api } from '../../api/api';
import Mesas from '../Mesas/Mesas';
import ModalOpciones from '../Mesas/ModalOpciones';
import TicketMesa from '../../components/TicketMesa';
import { tiempoAbierto } from '../../utils/mesas';
import { enlaceWhatsapp } from '../../utils/vehiculos';
import { confirmar } from '../../utils/confirmar';
import { imprimirTicket } from '../../utils/imprimirTicket';
import { nombreMetodo as nombreMetodoPago } from '../../utils/metodoPago';
import { esRespaldoLavanderia } from '../../utils/textosEncargos';
import '../../components/PantallaModulo.css';
import '../Mesas/Mesas.css';
import './PedidosDelivery.css';

// Módulo DELIVERY (dark kitchen): pedidos por WhatsApp sobre los pedidos del
// restaurante. Cliente, platos y envío se registran en un solo formulario;
// la cocina es la de siempre (Preparación) y el cobro es el del punto de
// venta. Backend: handlers/delivery.rs.
//   Adelantado:     Esperando pago -> (Confirmar pago = cobro, sale el comprobante) -> En cocina -> Listo -> En camino -> Entregado
//   Contra entrega: En cocina (al registrar) -> Listo -> En camino (paga con / vuelto) -> Por rendir -> (cobro) -> Entregado
//   Recojo:         En cocina -> Listo -> (cobro) -> Entregado

const REFRESCO_MS = 8000;
const METODOS_ADELANTO = [
  ['YAPE', 'Yape'],
  ['PLIN', 'Plin'],
  ['TRANSFERENCIA', 'Transferencia'],
];
const CLAVE_IMPRIMIR = 'monspeet-delivery-comanda';
const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const numeroDe = (texto) => {
  const n = parseFloat(String(texto ?? '').replace(',', '.'));
  return Number.isFinite(n) ? n : null;
};
const nombreMetodo = (m) => (m === 'YAPE_PLIN' ? 'Yape/Plin (sin separar)' : nombreMetodoPago(m));
const primerNombre = (p) => (p.cliente_nombre || '').split(' ')[0] || '';
const minutosDesde = (fecha) => {
  if (!fecha) return null;
  const t = new Date(fecha.replace(' ', 'T') + '-05:00').getTime();
  return Number.isFinite(t) ? Math.max(0, Math.round((Date.now() - t) / 60000)) : null;
};
const hoyLima = () => new Date(Date.now() - 5 * 3600e3).toISOString().slice(0, 10);
const leerClave = () => {
  try {
    return sessionStorage.getItem(CLAVE_IMPRIMIR);
  } catch {
    return null;
  }
};
const guardarClave = (valor) => {
  try {
    if (valor) sessionStorage.setItem(CLAVE_IMPRIMIR, valor);
    else sessionStorage.removeItem(CLAVE_IMPRIMIR);
  } catch {
    // Sin almacenamiento: la comanda se imprime con el botón.
  }
};

/** Cómo se paga, en una línea: lo que más importa en cada tarjeta. */
function EstadoPago({ p }) {
  if (p.cobrado) {
    return (
      <span className="dk-pago dk-pago-ok">
        <Check size={12} /> Pagado{p.folio ? ` · ${p.folio}` : ''}
      </span>
    );
  }
  if (p.pago === 'ADELANTADO') {
    return (
      <span className="dk-pago dk-pago-falta">
        <Hourglass size={12} /> Espera su pago{p.metodo_pago ? ` por ${nombreMetodo(p.metodo_pago)}` : ''}
      </span>
    );
  }
  return (
    <span className="dk-pago dk-pago-entrega">
      <Wallet size={12} /> {p.entrega === 'RECOJO' ? 'Paga al recoger' : 'Paga al recibir'} {soles(p.total)}
      {p.paga_con ? ` · con ${soles(p.paga_con)}, vuelto ${soles(p.vuelto)}` : ''}
    </span>
  );
}

/** Mensajes de WhatsApp: confirmación del pedido y "va en camino". */
function mensajeConfirmacion(p, tienda) {
  const lineas = p.lineas.map((l) => `• ${l.cantidad} ${l.nombre}${l.opciones ? ` (${l.opciones})` : ''} · ${soles(l.subtotal)}`);
  const pago = p.cobrado
    ? 'Pago: recibido ✅'
    : p.pago === 'ADELANTADO'
      ? `Para empezar a prepararlo, envíanos tu pago${p.metodo_pago ? ` por ${nombreMetodo(p.metodo_pago)}` : ''} y la captura por aquí 🙏`
      : p.entrega === 'RECOJO'
        ? 'Pago: al recoger'
        : 'Pago: al recibir tu pedido';
  return [
    `Hola ${primerNombre(p)} 👋`,
    `Recibimos tu pedido N° ${p.id} en ${tienda}:`,
    '',
    ...lineas,
    ...(p.costo_envio > 0 ? [`• Delivery · ${soles(p.costo_envio)}`] : []),
    '',
    `Total: ${soles(p.total)}`,
    pago,
    p.entrega === 'DELIVERY'
      ? `Entrega en: ${p.direccion}${p.referencia ? ` (${p.referencia})` : ''}`
      : 'Lo recoges en el local. Te avisamos cuando esté listo.',
    '',
    '¡Gracias por tu pedido!',
  ].join('\n');
}

function mensajeEnCamino(p, tienda) {
  return [
    `Hola ${primerNombre(p)} 🛵`,
    `Tu pedido N° ${p.id} de ${tienda} ya va en camino${p.repartidor ? ` con ${p.repartidor}` : ''}.`,
    p.cobrado ? 'Ya está pagado ✅' : `Total a pagar: ${soles(p.total)}${p.paga_con ? ` (te llevamos ${soles(p.vuelto)} de vuelto)` : ''}`,
    '¡Que lo disfrutes!',
  ].join('\n');
}

function mensajeListoRecojo(p, tienda) {
  return [`Hola ${primerNombre(p)} 👋`, `Tu pedido N° ${p.id} de ${tienda} ya está listo para recoger.`, `Total: ${soles(p.total)}`].join('\n');
}

// Pedido para el ticket de comanda (TicketMesa).
const pedidoTicket = (p) => ({ id: p.id, tipo: 'DELIVERY', cliente_nombre: p.cliente_nombre });

export default function PedidosDelivery({ usuario, nombreTienda, onCobrar, abrirPedido, onAbrirPedidoUsado, onIrACarta }) {
  const [tablero, setTablero] = useState({ pedidos: [], repartidores: [], nombres_recientes: [], rendicion: [] });
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [formulario, setFormulario] = useState(null); // null | 'nuevo' | pedido
  const [despachando, setDespachando] = useState(null);
  const [verMesas, setVerMesas] = useState(false);
  const [verResumen, setVerResumen] = useState(false);
  const [ocupado, setOcupado] = useState(null);
  const [mesasRestaurante, setMesasRestaurante] = useState([]);
  const [ticket, setTicket] = useState(null);
  const numeroTicket = useRef(0);
  const tienda = nombreTienda || 'nuestra cocina';

  const imprimirComanda = useCallback((p, items) => {
    if (!items?.length) return;
    setTicket({ pedido: pedidoTicket(p), items, clave: ++numeroTicket.current });
  }, []);
  useEffect(() => {
    if (!ticket) return undefined;
    const t = setTimeout(() => imprimirTicket(), 80);
    return () => clearTimeout(t);
  }, [ticket]);

  // Volver de un cobro cancelado: se abre la edición de ese pedido.
  const [aperturaVista, setAperturaVista] = useState(null);
  if (abrirPedido && abrirPedido.n !== aperturaVista) {
    setAperturaVista(abrirPedido.n);
  }
  useEffect(() => {
    if (abrirPedido) onAbrirPedidoUsado?.();
  }, [abrirPedido, onAbrirPedidoUsado]);

  const cargar = useCallback(
    () =>
      api
        .deliveryTablero()
        .then((t) => {
          setTablero(t);
          setError(null);
          // Volvemos de confirmar el pago de un adelantado: ya entró a cocina,
          // sale su comanda (una sola vez).
          const pendiente = Number(leerClave());
          if (pendiente) {
            const p = t.pedidos.find((x) => x.id === pendiente);
            if (p && p.cobrado) {
              guardarClave(null);
              imprimirComanda(
                p,
                p.lineas.map((l) => ({ id: l.id, nombre_producto: l.nombre, opciones: l.opciones, nota: l.nota, cantidad: l.cantidad }))
              );
            } else if (!p) guardarClave(null);
          }
        })
        .catch((e) => setError(e.message))
        .finally(() => setCargando(false)),
    [imprimirComanda]
  );

  useEffect(() => {
    if (verMesas) return undefined;
    cargar();
    const intervalo = setInterval(cargar, REFRESCO_MS);
    const alVolver = () => document.visibilityState === 'visible' && cargar();
    document.addEventListener('visibilitychange', alVolver);
    return () => {
      clearInterval(intervalo);
      document.removeEventListener('visibilitychange', alVolver);
    };
  }, [verMesas, cargar]);

  // Las mesas solo se muestran si el negocio también atiende en local.
  useEffect(() => {
    api
      .mesas()
      .then((m) => setMesasRestaurante(m || []))
      .catch(() => setMesasRestaurante([]));
  }, []);

  const grupos = useMemo(() => {
    const p = tablero.pedidos;
    // Entregados sin cobrar (sin cuenta de repartidor): se cobran aquí.
    const faltaCobrar = p.filter((x) => x.fase === 'POR_RENDIR');
    const rendicion = tablero.rendicion || [];
    return {
      esperando: p.filter((x) => x.fase === 'ESPERANDO_PAGO'),
      cocina: p.filter((x) => x.fase === 'EN_COCINA' || x.fase === 'LISTO'),
      camino: p.filter((x) => x.fase === 'EN_CAMINO'),
      faltaCobrar,
      rendicion,
      porRendirTotal: rendicion.reduce((s, r) => s + r.efectivo, 0) + faltaCobrar.reduce((s, x) => s + x.total, 0),
      entregadosHoy: p.filter((x) => x.fase === 'ENTREGADO').length,
    };
  }, [tablero.pedidos, tablero.rendicion]);

  const recibirEfectivo = async (r) => {
    const ok = await confirmar({
      titulo: `¿Recibiste ${soles(r.efectivo)} de ${r.nombre}?`,
      mensaje: `Es el efectivo que cobró en ${r.pedidos.length} ${r.pedidos.length === 1 ? 'pedido' : 'pedidos'}. Cuéntalo antes de confirmar.`,
      textoConfirmar: 'Sí, lo recibí',
    });
    if (ok) accion(`rendir-${r.repartidor_id}`, () => api.deliveryRendir(r.repartidor_id));
  };
  const hayCuentas = (tablero.repartidores || []).length > 0;

  const accion = async (id, fn) => {
    setOcupado(id);
    setError(null);
    try {
      await fn();
      await cargar();
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(null);
    }
  };

  // Se cobra en el punto de venta exactamente lo que quedó en el pedido.
  const cobrar = (p) =>
    accion(p.id, async () => {
      const actual = await api.pedido(p.id);
      const lineas = actual.items.filter((i) => i.estado !== 'ANULADO');
      if (p.lineas.length === 0) throw new Error('El pedido no tiene platos.');
      // Adelantado: al volver del cobro sale la comanda (recién entra a cocina).
      guardarClave(p.fase === 'ESPERANDO_PAGO' ? String(p.id) : null);
      onCobrar({ ...actual, items: lineas });
    });

  const anular = async (p) => {
    const ok = await confirmar({
      titulo: `¿Anular el pedido N° ${p.id}?`,
      mensaje: p.fase === 'ESPERANDO_PAGO' ? 'Aún no entró a cocina.' : 'Ya está en cocina: avísales que no lo preparen.',
      textoConfirmar: 'Anular pedido',
      icono: 'aviso',
    });
    if (!ok) return;
    accion(p.id, () => api.pedidoAnular(p.id, p.fase === 'ESPERANDO_PAGO' ? null : 'Cliente canceló el pedido'));
  };

  if (verMesas) {
    return (
      <div className="dk-con-mesas">
        <button type="button" className="mesas-boton-secundario dk-volver-tablero" onClick={() => setVerMesas(false)}>
          <Bike size={16} /> Volver a pedidos delivery
        </button>
        <Mesas usuario={usuario} nombreTienda={nombreTienda} onCobrar={onCobrar} onIrACarta={onIrACarta} />
      </div>
    );
  }

  const lineasComanda = (p) =>
    p.lineas.filter((l) => l.estado === 'ENVIADO').map((l) => ({ id: l.id, nombre_producto: l.nombre, opciones: l.opciones, nota: l.nota, cantidad: l.cantidad }));

  const tarjeta = (p) => {
    const salida = minutosDesde(p.fecha_salida);
    const mensaje =
      p.fase === 'EN_CAMINO'
        ? mensajeEnCamino(p, tienda)
        : p.fase === 'LISTO' && p.entrega === 'RECOJO'
          ? mensajeListoRecojo(p, tienda)
          : mensajeConfirmacion(p, tienda);
    const whatsapp = enlaceWhatsapp(p.telefono, mensaje);
    const enCocina = p.fase === 'EN_COCINA' || p.fase === 'LISTO';
    return (
      <article key={p.id} className={`dk-tarjeta dk-fase-${p.fase.toLowerCase()}`}>
        <header className="dk-tarjeta-cabecera">
          <span className="dk-tarjeta-titulo">
            N° {p.id} · {p.cliente_nombre || 'Sin nombre'}
          </span>
          <span className="dk-tarjeta-tiempo">
            <Clock size={12} /> {p.fase === 'EN_CAMINO' && salida != null ? `salió hace ${tiempoAbierto(salida)}` : tiempoAbierto(p.minutos)}
          </span>
        </header>
        <div className="dk-tarjeta-entrega">
          {p.entrega === 'RECOJO' ? (
            <span>
              <Store size={13} /> Recoge en el local
            </span>
          ) : (
            <span>
              <MapPin size={13} /> {p.direccion || 'Sin dirección'}
              {p.referencia ? ` · ${p.referencia}` : ''}
            </span>
          )}
          {p.telefono && <span className="dk-tenue">{p.telefono}</span>}
          {p.repartidor && (p.fase === 'EN_CAMINO' || p.repartidor_id) && (
            <span className="dk-asignado">
              <Bike size={13} /> {p.orden_reparto ? `${p.orden_reparto}° de ` : ''}
              {p.repartidor}
            </span>
          )}
        </div>
        {enCocina && p.entrega === 'DELIVERY' && hayCuentas && (
          <label className="dk-asignar">
            <span>Repartidor</span>
            <select
              value={p.repartidor_id || ''}
              disabled={ocupado === p.id}
              onChange={(e) => accion(p.id, () => api.deliveryAsignar(p.id, e.target.value ? Number(e.target.value) : null))}
            >
              <option value="">Sin asignar</option>
              {tablero.repartidores.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.nombre}
                </option>
              ))}
            </select>
          </label>
        )}
        <ul className="dk-lineas">
          {p.lineas.map((l) => (
            <li key={l.id}>
              {l.cantidad} × {l.nombre}
              {l.opciones && <small> ({l.opciones})</small>}
              {l.nota && <small className="dk-nota"> · {l.nota}</small>}
            </li>
          ))}
          {p.costo_envio > 0 && <li className="dk-tenue">Delivery · {soles(p.costo_envio)}</li>}
        </ul>
        {p.notas && <p className="dk-tenue dk-notas">{p.notas}</p>}
        <div className="dk-tarjeta-estado">
          {p.fase === 'LISTO' && <span className="mesa-card-listo">Listo para salir</span>}
          {p.fase === 'EN_COCINA' && <span className="mesa-card-preparando">Preparando</span>}
          <strong className="dk-total">{soles(p.total)}</strong>
        </div>
        <EstadoPago p={p} />
        {p.sin_datos && <p className="dk-aviso">Abierto desde Mesas: completa sus datos de entrega.</p>}
        <div className="dk-acciones">
          {p.fase === 'ESPERANDO_PAGO' && (
            <button type="button" className="mesas-boton-primario" disabled={ocupado === p.id} onClick={() => cobrar(p)}>
              <Wallet size={14} /> Confirmar pago
            </button>
          )}
          {p.fase === 'EN_COCINA' && (
            <button type="button" className="mesas-boton-primario" disabled={ocupado === p.id} onClick={() => accion(p.id, () => api.deliveryListo(p.id))}>
              <Check size={14} /> Listo
            </button>
          )}
          {enCocina && p.entrega === 'DELIVERY' && (
            <button
              type="button"
              className={p.fase === 'LISTO' ? 'mesas-boton-primario' : 'mesas-boton-secundario'}
              disabled={ocupado === p.id || p.sin_datos}
              onClick={() => setDespachando(p)}
            >
              <Bike size={14} /> Despachar
            </button>
          )}
          {enCocina && p.entrega === 'RECOJO' && !p.cobrado && (
            <button type="button" className="mesas-boton-primario" disabled={ocupado === p.id} onClick={() => cobrar(p)}>
              <Wallet size={14} /> Cobrar
            </button>
          )}
          {enCocina && p.entrega === 'RECOJO' && p.cobrado && (
            <button type="button" className="mesas-boton-primario" disabled={ocupado === p.id} onClick={() => accion(p.id, () => api.deliveryEntregar(p.id))}>
              <Check size={14} /> Entregado
            </button>
          )}
          {p.fase === 'EN_CAMINO' && p.repartidor_id && !p.cobrado && (
            <span className="dk-tenue dk-lo-cobra">Lo cobra {p.repartidor?.split(' ')[0]} en la puerta</span>
          )}
          {p.fase === 'EN_CAMINO' && (
            <>
              {!(p.repartidor_id && !p.cobrado) && (
                <button type="button" className="mesas-boton-primario" disabled={ocupado === p.id} onClick={() => accion(p.id, () => api.deliveryEntregar(p.id))}>
                  <Check size={14} /> Entregado
                </button>
              )}
              <button
                type="button"
                className="mesas-boton-secundario"
                disabled={ocupado === p.id}
                onClick={() => accion(p.id, () => api.deliveryVolver(p.id))}
                aria-label="Volver a cocina"
                title="Volver a cocina"
              >
                <Undo2 size={14} />
              </button>
            </>
          )}
          {enCocina && (
            <button type="button" className="mesas-boton-secundario" onClick={() => imprimirComanda(p, lineasComanda(p))} aria-label="Imprimir comanda" title="Imprimir comanda">
              <Printer size={14} />
            </button>
          )}
          {whatsapp && (
            <a className="mesas-boton-secundario dk-enlace" href={whatsapp} target="_blank" rel="noreferrer">
              <MessageCircle size={14} /> {p.fase === 'EN_CAMINO' ? 'Avisar' : p.fase === 'LISTO' && p.entrega === 'RECOJO' ? 'Avisar' : 'Confirmar'}
            </a>
          )}
          {(p.fase === 'ESPERANDO_PAGO' || enCocina) && (
            <button type="button" className="mesas-boton-secundario" onClick={() => setFormulario(p)} aria-label="Editar pedido" title="Editar pedido">
              <Pencil size={14} />
            </button>
          )}
          {(p.fase === 'ESPERANDO_PAGO' || (enCocina && !p.cobrado)) && (
            <button type="button" className="mesas-boton-secundario dk-peligro" onClick={() => anular(p)} aria-label="Anular pedido" title="Anular pedido">
              <Trash2 size={14} />
            </button>
          )}
        </div>
      </article>
    );
  };

  const columna = (titulo, Icono, lista, vacio) => (
    <section>
      <h2 className="mesas-zona-titulo">
        <Icono size={15} /> {titulo} ({lista.length})
      </h2>
      {lista.length === 0 && <p className="dk-tenue">{vacio}</p>}
      {lista.map(tarjeta)}
    </section>
  );

  return (
    <div className="mesas-layout dk-layout">
      <div className="mesas-header">
        <div>
          <h1>Pedidos delivery</h1>
          <p className="mesas-subtitulo">
            {grupos.esperando.length > 0 && `${grupos.esperando.length} esperan pago · `}
            {grupos.cocina.length} en cocina · {grupos.camino.length} en camino
            {grupos.porRendirTotal > 0 && ` · ${soles(grupos.porRendirTotal)} por rendir`}
            {grupos.entregadosHoy > 0 && ` · ${grupos.entregadosHoy} entregados hoy`}
          </p>
        </div>
        <div className="mesas-header-acciones">
          <button type="button" className="mesas-boton-secundario" onClick={cargar} aria-label="Actualizar">
            <RefreshCw size={16} />
          </button>
          <button type="button" className="mesas-boton-secundario" onClick={() => setVerResumen(true)}>
            <BarChart3 size={16} /> Resumen del día
          </button>
          {mesasRestaurante.length > 0 && (
            <button type="button" className="mesas-boton-secundario" onClick={() => setVerMesas(true)}>
              <UtensilsCrossed size={16} /> Mesas
            </button>
          )}
          <button type="button" className="mesas-boton-primario" onClick={() => setFormulario('nuevo')}>
            <Plus size={16} /> Nuevo pedido
          </button>
        </div>
      </div>

      {error && <p className="mesas-error">{error}</p>}
      {cargando && <p className="mesas-vacio">Cargando pedidos...</p>}

      {(grupos.rendicion.length > 0 || grupos.faltaCobrar.length > 0) && (
        <section className="dk-rendicion" aria-label="Por rendir">
          <h2 className="mesas-zona-titulo">
            <Wallet size={15} /> Por rendir
          </h2>
          {grupos.rendicion.map((r) => (
            <div key={r.repartidor_id} className="dk-rendicion-fila">
              <span>
                <Bike size={14} /> <strong>{r.nombre}</strong>
                {r.efectivo > 0 ? (
                  <>
                    {' '}
                    cobró <strong>{soles(r.efectivo)}</strong> en efectivo
                  </>
                ) : (
                  ' no tiene efectivo por entregar'
                )}
                {r.digital > 0 && ` · ${soles(r.digital)} por Yape/Plin (ya en tu cuenta)`} · {r.pedidos.length}{' '}
                {r.pedidos.length === 1 ? 'pedido' : 'pedidos'}
              </span>
              <span className="dk-rendicion-pedidos">
                <span className="dk-tenue">{r.pedidos.map((x) => `N° ${x.pedido_id}${x.efectivo > 0 ? ` ${soles(x.efectivo)}` : ''}`).join(' · ')}</span>
                <button type="button" className="mesas-boton-primario" disabled={ocupado === `rendir-${r.repartidor_id}`} onClick={() => recibirEfectivo(r)}>
                  <Check size={14} /> {r.efectivo > 0 ? `Recibí ${soles(r.efectivo)}` : 'Listo, revisado'}
                </button>
              </span>
            </div>
          ))}
          {grupos.faltaCobrar.length > 0 && (
            <div className="dk-rendicion-fila">
              <span>
                <Wallet size={14} /> <strong>Entregados sin cobrar</strong> (los despachó caja): cóbralos cuando vuelva el repartidor
              </span>
              <span className="dk-rendicion-pedidos">
                {grupos.faltaCobrar.map((x) => (
                  <button key={x.id} type="button" className="mesas-boton-secundario" disabled={ocupado === x.id} onClick={() => cobrar(x)}>
                    N° {x.id} · {x.cliente_nombre} · {soles(x.total)}
                    {x.repartidor ? ` · ${x.repartidor}` : ''} — Cobrar
                  </button>
                ))}
              </span>
            </div>
          )}
        </section>
      )}

      {!cargando && tablero.pedidos.length === 0 && (
        <p className="mesas-vacio">No hay pedidos todavía. Toca “Nuevo pedido” cuando te escriban por WhatsApp.</p>
      )}

      <div className={`dk-columnas${grupos.esperando.length > 0 ? ' dk-columnas-3' : ''}`}>
        {grupos.esperando.length > 0 && columna('Esperando pago', Hourglass, grupos.esperando, '')}
        {columna('En cocina', ChefHat, grupos.cocina, 'Nada en cocina.')}
        {columna('En camino', Bike, grupos.camino, 'Ningún pedido en camino.')}
      </div>

      {formulario && (
        <FormularioDelivery
          pedido={formulario === 'nuevo' ? null : formulario}
          onCerrar={() => setFormulario(null)}
          onGuardado={(p) => {
            setFormulario(null);
            // Lo que acaba de entrar a cocina sale en su comanda.
            if (p.comanda?.length) imprimirComanda(p, p.comanda);
            cargar();
          }}
        />
      )}
      {despachando && (
        <DialogoDespacho
          pedido={despachando}
          cuentas={tablero.repartidores || []}
          nombres={tablero.nombres_recientes || []}
          onCerrar={() => setDespachando(null)}
          onListo={() => {
            setDespachando(null);
            cargar();
          }}
        />
      )}
      {verResumen && <ResumenDia onCerrar={() => setVerResumen(false)} />}
      {ticket &&
        createPortal(
          <TicketMesa key={ticket.clave} tipo="COMANDA" pedido={ticket.pedido} items={ticket.items} nombreTienda={nombreTienda} />,
          document.body
        )}
    </div>
  );
}

// ---------------------------------------------------------------------
// Formulario: cliente, platos, entrega y pago (todo en uno)
// ---------------------------------------------------------------------

let siguienteClave = 1;

function FormularioDelivery({ pedido, onCerrar, onGuardado }) {
  const editando = !!pedido;
  const cobrado = !!pedido?.cobrado;
  const [datos, setDatos] = useState(() => ({
    cliente: pedido?.cliente_id
      ? { id: pedido.cliente_id, nombre_razon_social: pedido.cliente_nombre, numero_documento: pedido.cliente_documento }
      : null,
    cliente_nombre: pedido && !pedido.cliente_id ? pedido.cliente_nombre || '' : '',
    telefono: pedido?.telefono || '',
    entrega: pedido?.entrega || 'DELIVERY',
    direccion: pedido?.direccion || '',
    referencia: pedido?.referencia || '',
    pago: pedido?.pago || 'CONTRA_ENTREGA',
    metodo_pago: pedido?.metodo_pago || 'YAPE',
    costo_envio: pedido ? String(pedido.costo_envio || '') : '',
    notas: pedido?.notas || '',
  }));
  // Líneas que ya estaban (al editar) y las que se quitan.
  const existentes = pedido?.lineas || [];
  const [quitar, setQuitar] = useState([]);
  const [motivo, setMotivo] = useState('');
  // Platos nuevos: { clave, producto_id, nombre, opcion_ids, opciones, nota, cantidad, precio }
  const [nuevos, setNuevos] = useState([]);
  const [productos, setProductos] = useState([]);
  const [grupos, setGrupos] = useState([]);
  const [buscarPlato, setBuscarPlato] = useState('');
  const [conOpciones, setConOpciones] = useState(null);
  const [buscar, setBuscar] = useState('');
  const [clientes, setClientes] = useState([]);
  const [consultas, setConsultas] = useState({});
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const cambiar = (campo, valor) => setDatos((d) => ({ ...d, [campo]: valor }));
  const salio = editando && !['ESPERANDO_PAGO', 'EN_COCINA', 'LISTO'].includes(pedido.fase);
  const platosEditables = !cobrado && !salio;

  useEffect(() => {
    Promise.all([api.productos(), api.modificadores()])
      .then(([p, g]) => {
        setProductos((p || []).filter((x) => !esRespaldoLavanderia(x) && x.activo !== false && x.activo !== 0));
        setGrupos(g || []);
      })
      .catch(() => {});
  }, []);

  const gruposPorProducto = useMemo(() => {
    const mapa = new Map();
    grupos.forEach((g) =>
      (g.producto_ids || []).forEach((pid) => {
        if (!mapa.has(pid)) mapa.set(pid, []);
        mapa.get(pid).push(g);
      })
    );
    return mapa;
  }, [grupos]);

  const encontrados = useMemo(() => {
    const q = buscarPlato.trim().toLowerCase();
    const disponibles = productos.filter((p) => !p.agotado);
    if (q.length < 1) return [];
    return disponibles.filter((p) => p.nombre.toLowerCase().includes(q) || String(p.codigo || '').toLowerCase().includes(q)).slice(0, 8);
  }, [productos, buscarPlato]);
  // Botones rápidos: la carta de hoy primero y luego los demás (hasta 12).
  const rapidos = useMemo(
    () =>
      productos
        .filter((p) => !p.agotado)
        .sort((a, b) => Number(!!b.carta_dia) - Number(!!a.carta_dia))
        .slice(0, 12),
    [productos]
  );

  const agregarLinea = (producto, eleccion) => {
    const gruposProducto = gruposPorProducto.get(producto.id) || [];
    const elegidas = gruposProducto.flatMap((g) => g.opciones.filter((o) => (eleccion?.opcion_ids || []).includes(o.id)));
    const extras = elegidas.reduce((s, o) => s + o.precio_extra, 0);
    setNuevos((lista) => [
      ...lista,
      {
        clave: siguienteClave++,
        producto_id: producto.id,
        nombre: producto.nombre,
        opcion_ids: eleccion?.opcion_ids || [],
        opciones: elegidas.map((o) => o.nombre).join(', '),
        nota: eleccion?.nota || '',
        cantidad: eleccion?.cantidad || 1,
        precio: producto.precio + extras,
      },
    ]);
  };
  const elegirPlato = (producto) => {
    setBuscarPlato('');
    if ((gruposPorProducto.get(producto.id) || []).length > 0) setConOpciones(producto);
    else agregarLinea(producto);
  };
  const cambiarNuevo = (clave, campo, valor) => setNuevos((l) => l.map((x) => (x.clave === clave ? { ...x, [campo]: valor } : x)));

  // Cliente: búsqueda, o un DNI/RUC que aún no es cliente (RENIEC/SUNAT).
  useEffect(() => {
    const q = buscar.trim();
    if (q.length < 2) return undefined;
    const temporizador = setTimeout(() => {
      api
        .clientesBuscar(q)
        .then((r) => setClientes((r || []).slice(0, 6)))
        .catch(() => setClientes([]));
    }, 250);
    return () => clearTimeout(temporizador);
  }, [buscar]);
  const documento = useMemo(() => {
    const q = buscar.trim();
    if (/^\d{8}$/.test(q)) return { tipo: 'DNI', numero: q };
    if (/^\d{11}$/.test(q)) return { tipo: 'RUC', numero: q };
    return null;
  }, [buscar]);
  useEffect(() => {
    if (!documento || consultas[documento.numero]) return undefined;
    let cancelado = false;
    api
      .documentoConsultar(documento.tipo, documento.numero)
      .then((r) => {
        if (cancelado) return;
        setConsultas((c) => ({ ...c, [documento.numero]: r?.existe === true && r?.nombre ? { nombre: r.nombre, direccion: r.direccion || null } : { nombre: null } }));
      })
      .catch(() => !cancelado && setConsultas((c) => ({ ...c, [documento.numero]: { nombre: null } })));
    return () => {
      cancelado = true;
    };
  }, [documento, consultas]);
  const yaEsCliente = !!documento && clientes.some((c) => c.numero_documento === documento.numero);
  const consultado = documento && !yaEsCliente ? consultas[documento.numero] : null;
  const elegirCliente = (c) => {
    setDatos((d) => ({ ...d, cliente: c, cliente_nombre: '', telefono: d.telefono || c.telefono || '', direccion: d.direccion || c.direccion || '' }));
    setBuscar('');
    setClientes([]);
  };
  const registrarCliente = async (nombre, direccion) => {
    setOcupado(true);
    setError(null);
    try {
      const nuevo = await api.clienteCrear({
        tipo_documento: documento.tipo,
        numero_documento: documento.numero,
        nombre_razon_social: nombre.trim(),
        telefono: datos.telefono.trim() || null,
        direccion: direccion || null,
      });
      elegirCliente(nuevo);
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const delivery = datos.entrega === 'DELIVERY';
  const envio = delivery ? numeroDe(datos.costo_envio) || 0 : 0;
  const quedan = existentes.filter((l) => !quitar.includes(l.id));
  const totalPlatos =
    quedan.reduce((s, l) => s + l.subtotal, 0) + nuevos.reduce((s, l) => s + (numeroDe(l.cantidad) || 0) * l.precio, 0);
  const total = Math.round((totalPlatos + envio) * 100) / 100;
  const quitaEnviados = existentes.some((l) => quitar.includes(l.id) && l.estado === 'ENVIADO');
  const vaACocina = cobrado || datos.pago === 'CONTRA_ENTREGA' || existentes.some((l) => l.estado === 'ENVIADO');

  const guardar = async () => {
    setError(null);
    if (!datos.cliente && !datos.cliente_nombre.trim()) return setError('Escribe el nombre del cliente.');
    if (delivery && !datos.telefono.trim()) return setError('Escribe el celular del cliente.');
    if (delivery && !datos.direccion.trim()) return setError('Escribe la dirección de entrega.');
    if (quedan.length + nuevos.length === 0) return setError('Agrega los platos del pedido.');
    if (nuevos.some((l) => !(numeroDe(l.cantidad) > 0))) return setError('Revisa las cantidades.');
    if (envio < 0) return setError('Revisa el costo de envío.');
    if (quitaEnviados && !motivo.trim()) return setError('Escribe el motivo para quitar lo que ya está en cocina.');
    const cuerpo = {
      cliente_id: datos.cliente?.id || null,
      cliente_nombre: datos.cliente ? null : datos.cliente_nombre.trim(),
      telefono: datos.telefono.trim() || null,
      entrega: datos.entrega,
      direccion: delivery ? datos.direccion.trim() : null,
      referencia: delivery ? datos.referencia.trim() || null : null,
      canal: 'WHATSAPP',
      pago: datos.pago,
      metodo_pago: datos.pago === 'ADELANTADO' ? datos.metodo_pago : null,
      costo_envio: envio,
      notas: datos.notas.trim() || null,
      agregar: nuevos.map((l) => ({ producto_id: l.producto_id, cantidad: numeroDe(l.cantidad), opcion_ids: l.opcion_ids, nota: l.nota.trim() || null })),
      quitar,
      motivo_quitar: motivo.trim() || null,
    };
    setOcupado(true);
    try {
      const p = editando ? await api.deliveryActualizar(pedido.id, cuerpo) : await api.deliveryCrear(cuerpo);
      onGuardado(p);
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const textoGuardar = ocupado
    ? 'Guardando...'
    : !editando
      ? datos.pago === 'ADELANTADO'
        ? 'Registrar (espera el pago)'
        : 'Registrar y enviar a cocina'
      : nuevos.length > 0 && vaACocina
        ? 'Guardar y enviar lo nuevo a cocina'
        : 'Guardar cambios';

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal pm-modal-ancho dk-formulario" role="dialog" aria-modal="true" aria-label={editando ? 'Editar pedido' : 'Nuevo pedido'}>
        <div className="pm-modal-cabecera">
          <h2>{editando ? `Pedido N° ${pedido.id}` : 'Nuevo pedido delivery'}</h2>
          <p>Cliente, platos, entrega y pago en un solo paso.</p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

          <p className="pm-seccion">Cliente</p>
          {datos.cliente ? (
            <div className="dk-cliente-elegido">
              <span>
                <strong>{datos.cliente.nombre_razon_social}</strong>
                {datos.cliente.numero_documento && <small> · {datos.cliente.numero_documento}</small>}
              </span>
              <button type="button" className="pm-boton-secundario" onClick={() => cambiar('cliente', null)}>
                Cambiar
              </button>
            </div>
          ) : (
            <div className="pm-campos">
              <label className="pm-campo pm-campo-ancho">
                <span>Buscar cliente o escribir su DNI / RUC (opcional)</span>
                <input value={buscar} placeholder="Nombre, DNI o RUC" onChange={(e) => setBuscar(e.target.value)} />
              </label>
              {buscar.trim().length >= 2 && clientes.length > 0 && (
                <div className="pm-campo-ancho">
                  {clientes.map((c) => (
                    <button key={c.id} type="button" className="pm-fila" onClick={() => elegirCliente(c)}>
                      <span className="pm-fila-principal">
                        <span className="pm-fila-titulo">{c.nombre_razon_social}</span>
                        <span className="pm-fila-detalle" style={{ display: 'block' }}>
                          {[c.numero_documento, c.telefono].filter(Boolean).join(' · ')}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              )}
              {documento && !yaEsCliente && !consultado && <p className="pm-subtitulo pm-campo-ancho">Consultando {documento.tipo === 'RUC' ? 'SUNAT' : 'RENIEC'}…</p>}
              {consultado?.nombre && (
                <div className="pm-campo-ancho dk-cliente-elegido">
                  <span>
                    <strong>{consultado.nombre}</strong>
                    <small>
                      {' '}
                      · {documento.tipo} {documento.numero}
                    </small>
                  </span>
                  <button type="button" className="pm-boton" disabled={ocupado} onClick={() => registrarCliente(consultado.nombre, consultado.direccion)}>
                    Usar este cliente
                  </button>
                </div>
              )}
              {consultado && !consultado.nombre && <p className="pm-subtitulo pm-campo-ancho">No se encontró ese documento: escribe el nombre abajo.</p>}
              <label className="pm-campo">
                <span>Nombre</span>
                <input value={datos.cliente_nombre} maxLength={60} onChange={(e) => cambiar('cliente_nombre', e.target.value)} />
              </label>
            </div>
          )}
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Celular (WhatsApp)</span>
              <input inputMode="tel" value={datos.telefono} maxLength={20} onChange={(e) => cambiar('telefono', e.target.value.replace(/[^\d+ ]/g, ''))} />
            </label>
          </div>

          <p className="pm-seccion">Platos</p>
          {!platosEditables && <p className="pm-subtitulo">{cobrado ? 'Ya se cobró: los platos no cambian.' : 'Ya salió: los platos no cambian.'}</p>}
          {platosEditables && rapidos.length > 0 && (
            <div className="pm-opciones dk-rapidos" aria-label="Agregar un plato">
              {rapidos.map((p) => (
                <button key={p.id} type="button" onClick={() => elegirPlato(p)}>
                  <Plus size={13} /> {p.nombre}
                </button>
              ))}
            </div>
          )}
          {platosEditables && (
            <div className="pm-campo dk-buscar-plato">
              <Search size={15} />
              <input value={buscarPlato} placeholder="Buscar plato o bebida" onChange={(e) => setBuscarPlato(e.target.value)} />
            </div>
          )}
          {encontrados.map((p) => (
            <button key={p.id} type="button" className="pm-fila" onClick={() => elegirPlato(p)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">{p.nombre}</span>
                {(gruposPorProducto.get(p.id) || []).length > 0 && (
                  <span className="pm-fila-detalle" style={{ display: 'block' }}>
                    Con opciones
                  </span>
                )}
              </span>
              <span className="pm-fila-monto">
                <strong>{soles(p.precio)}</strong>
              </span>
            </button>
          ))}
          <div className="dk-platos">
            {existentes.map((l) => {
              const quitado = quitar.includes(l.id);
              return (
                <div key={l.id} className={`dk-plato${quitado ? ' dk-plato-quitado' : ''}`}>
                  <span className="dk-plato-nombre">
                    {l.cantidad} × {l.nombre}
                    {l.opciones && <small> ({l.opciones})</small>}
                    {l.nota && <small className="dk-nota"> · {l.nota}</small>}
                    {l.estado === 'ENVIADO' && <small className="dk-en-cocina"> · en cocina</small>}
                  </span>
                  <span className="dk-plato-precio">{soles(l.subtotal)}</span>
                  {platosEditables && (
                    <button
                      type="button"
                      className="pm-boton-secundario"
                      onClick={() => setQuitar((q) => (quitado ? q.filter((x) => x !== l.id) : [...q, l.id]))}
                      aria-label={quitado ? `Dejar ${l.nombre}` : `Quitar ${l.nombre}`}
                    >
                      {quitado ? <Undo2 size={14} /> : <Trash2 size={14} />}
                    </button>
                  )}
                </div>
              );
            })}
            {nuevos.map((l) => (
              <div key={l.clave} className="dk-plato dk-plato-nuevo">
                <span className="dk-plato-nombre">
                  {l.nombre}
                  {l.opciones && <small> ({l.opciones})</small>}
                  <input
                    className="dk-plato-nota"
                    value={l.nota}
                    maxLength={140}
                    placeholder="Nota (sin cebolla, bien cocido...)"
                    onChange={(e) => cambiarNuevo(l.clave, 'nota', e.target.value)}
                  />
                </span>
                <span className="dk-cantidad">
                  <button type="button" aria-label="Menos" onClick={() => cambiarNuevo(l.clave, 'cantidad', Math.max(1, (numeroDe(l.cantidad) || 1) - 1))}>
                    <Minus size={13} />
                  </button>
                  <input inputMode="numeric" value={l.cantidad} onChange={(e) => cambiarNuevo(l.clave, 'cantidad', e.target.value.replace(/[^\d]/g, ''))} />
                  <button type="button" aria-label="Más" onClick={() => cambiarNuevo(l.clave, 'cantidad', (numeroDe(l.cantidad) || 0) + 1)}>
                    <Plus size={13} />
                  </button>
                </span>
                <span className="dk-plato-precio">{soles((numeroDe(l.cantidad) || 0) * l.precio)}</span>
                <button type="button" className="pm-boton-secundario" onClick={() => setNuevos((n) => n.filter((x) => x.clave !== l.clave))} aria-label={`Quitar ${l.nombre}`}>
                  <X size={14} />
                </button>
              </div>
            ))}
            {existentes.length + nuevos.length === 0 && <p className="pm-vacio">Toca un plato de arriba o búscalo.</p>}
          </div>
          {quitaEnviados && (
            <div className="pm-campos">
              <label className="pm-campo pm-campo-ancho">
                <span>Motivo para quitar lo que ya está en cocina</span>
                <input value={motivo} maxLength={140} placeholder="El cliente ya no lo quiere" onChange={(e) => setMotivo(e.target.value)} />
              </label>
            </div>
          )}

          <p className="pm-seccion">Entrega</p>
          <div className="pm-opciones">
            {[
              ['DELIVERY', 'Delivery a domicilio'],
              ['RECOJO', 'Recoge en el local'],
            ].map(([valor, label]) => (
              <button key={valor} type="button" className={datos.entrega === valor ? 'activo' : ''} disabled={cobrado} onClick={() => cambiar('entrega', valor)}>
                {label}
              </button>
            ))}
          </div>
          {delivery && (
            <div className="pm-campos">
              <label className="pm-campo pm-campo-ancho">
                <span>Dirección</span>
                <input value={datos.direccion} maxLength={250} placeholder="Av. La Cultura 123, Wanchaq" onChange={(e) => cambiar('direccion', e.target.value)} />
              </label>
              <label className="pm-campo pm-campo-ancho">
                <span>Referencia (opcional)</span>
                <input value={datos.referencia} maxLength={250} placeholder="Frente al parque, puerta verde" onChange={(e) => cambiar('referencia', e.target.value)} />
              </label>
              <label className="pm-campo">
                <span>Costo de envío (S/)</span>
                <input inputMode="decimal" value={datos.costo_envio} placeholder="0.00" disabled={cobrado} onChange={(e) => cambiar('costo_envio', e.target.value)} />
              </label>
            </div>
          )}

          <p className="pm-seccion">Pago</p>
          <div className="pm-opciones">
            {[
              ['CONTRA_ENTREGA', delivery ? 'Contra entrega (al repartidor)' : 'Paga al recoger'],
              ['ADELANTADO', 'Adelantado (antes de preparar)'],
            ].map(([valor, label]) => (
              <button key={valor} type="button" disabled={cobrado} className={datos.pago === valor ? 'activo' : ''} onClick={() => cambiar('pago', valor)}>
                {label}
              </button>
            ))}
          </div>
          {datos.pago === 'ADELANTADO' && (
            <div className="pm-opciones dk-metodos">
              {METODOS_ADELANTO.map(([valor, label]) => (
                <button key={valor} type="button" disabled={cobrado} className={datos.metodo_pago === valor ? 'activo' : ''} onClick={() => cambiar('metodo_pago', valor)}>
                  {label}
                </button>
              ))}
            </div>
          )}
          <p className="pm-subtitulo">
            {datos.pago === 'ADELANTADO'
              ? 'No entra a cocina hasta que confirmes su pago: ahí se cobra, sale la nota de venta o boleta (va con el pedido) y recién se prepara.'
              : delivery
                ? 'Entra a cocina al registrarlo. El repartidor cobra al entregar; al despachar puedes anotar con cuánto paga para el vuelto.'
                : 'Entra a cocina al registrarlo y se cobra cuando lo recoja.'}
          </p>

          <div className="pm-campos">
            <label className="pm-campo pm-campo-ancho">
              <span>Notas para cocina o el repartidor (opcional)</span>
              <textarea rows={2} maxLength={500} value={datos.notas} onChange={(e) => cambiar('notas', e.target.value)} />
            </label>
          </div>

          <div className="pm-total">
            <span>
              Total{envio > 0 ? ` (incluye envío ${soles(envio)})` : ''}
            </span>
            <strong>{soles(total)}</strong>
          </div>
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cancelar
          </button>
          <button className="pm-boton" onClick={guardar} disabled={ocupado}>
            {textoGuardar}
          </button>
        </div>
      </div>
      {conOpciones && (
        <div className="dk-sobre">
          <ModalOpciones
            producto={conOpciones}
            grupos={gruposPorProducto.get(conOpciones.id) || []}
            onCerrar={() => setConOpciones(null)}
            onAgregar={(eleccion) => {
              agregarLinea(conOpciones, eleccion);
              setConOpciones(null);
            }}
          />
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------
// Despacho: quién lo lleva y, si cobra al entregar, con cuánto paga
// ---------------------------------------------------------------------

function DialogoDespacho({ pedido, cuentas, nombres, onCerrar, onListo }) {
  // Con cuentas de repartidor se elige una; sin cuentas, se escribe el nombre.
  const [cuenta, setCuenta] = useState(pedido.repartidor_id || (cuentas.length === 1 ? cuentas[0].id : null));
  const [repartidor, setRepartidor] = useState(cuentas.length > 0 ? '' : nombres[0] || '');
  const [pagaCon, setPagaCon] = useState('');
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const cobraAlEntregar = !pedido.cobrado;
  const monto = numeroDe(pagaCon);
  const vuelto = monto > 0 ? Math.round((monto - pedido.total) * 100) / 100 : null;
  const despachar = async () => {
    if (!cuenta && !repartidor.trim()) return setError(cuentas.length > 0 ? 'Elige quién lleva el pedido.' : 'Escribe quién lleva el pedido.');
    if (monto > 0 && vuelto < 0) return setError(`Con ${soles(monto)} no alcanza: el total es ${soles(pedido.total)}.`);
    setOcupado(true);
    setError(null);
    try {
      await api.deliveryDespachar(pedido.id, cuenta ? { repartidor_id: cuenta } : { repartidor: repartidor.trim() }, cobraAlEntregar && monto > 0 ? monto : null);
      onListo();
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };
  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal" role="dialog" aria-modal="true" aria-label="Despachar pedido">
        <div className="pm-modal-cabecera">
          <h2>Despachar pedido N° {pedido.id}</h2>
          <p>
            {pedido.cliente_nombre} · {pedido.direccion}
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          {pedido.fase === 'EN_COCINA' && <p className="dk-aviso">Cocina aún no lo marcó como listo.</p>}
          <EstadoPago p={pedido} />
          {cuentas.length > 0 && (
            <div className="pm-opciones dk-metodos">
              {cuentas.map((r) => (
                <button key={r.id} type="button" className={cuenta === r.id ? 'activo' : ''} onClick={() => setCuenta(r.id)}>
                  {r.nombre}
                </button>
              ))}
            </div>
          )}
          {cuentas.length > 0 && cuenta && !pedido.cobrado && (
            <p className="pm-subtitulo">Le aparece en su cuenta y lo cobra él en la puerta.</p>
          )}
          {cuentas.length === 0 && nombres.length > 0 && (
            <div className="pm-opciones dk-metodos">
              {nombres.map((r) => (
                <button key={r} type="button" className={repartidor === r ? 'activo' : ''} onClick={() => setRepartidor(r)}>
                  {r}
                </button>
              ))}
            </div>
          )}
          <div className="pm-campos">
            {cuentas.length === 0 && (
              <label className="pm-campo pm-campo-ancho">
                <span>Repartidor</span>
                <input value={repartidor} maxLength={60} placeholder="Nombre de quien lo lleva" onChange={(e) => setRepartidor(e.target.value)} />
              </label>
            )}
            {cobraAlEntregar && (
              <label className="pm-campo">
                <span>Si paga en efectivo: ¿con cuánto? (opcional)</span>
                <input inputMode="decimal" value={pagaCon} placeholder={`Total ${soles(pedido.total)}`} onChange={(e) => setPagaCon(e.target.value)} />
              </label>
            )}
          </div>
          {cobraAlEntregar && vuelto != null && vuelto >= 0 && (
            <p className="pm-subtitulo">
              El repartidor lleva <strong>{soles(vuelto)}</strong> de vuelto.
            </p>
          )}
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cancelar
          </button>
          <button className="pm-boton" onClick={despachar} disabled={ocupado}>
            {ocupado ? 'Despachando...' : 'Sale en camino'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Resumen del día: lo cobrado, por medio de pago, por repartidor y envíos
// ---------------------------------------------------------------------

function ResumenDia({ onCerrar }) {
  const [fecha, setFecha] = useState(hoyLima());
  const [datos, setDatos] = useState(null);
  const [error, setError] = useState(null);
  useEffect(() => {
    let cancelado = false;
    api
      .deliveryResumen(fecha)
      .then((r) => {
        if (!cancelado) {
          setDatos(r);
          setError(null);
        }
      })
      .catch((e) => !cancelado && setError(e.message));
    return () => {
      cancelado = true;
    };
  }, [fecha]);
  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Resumen del día">
        <div className="pm-modal-cabecera dk-resumen-cabecera">
          <div>
            <h2>Resumen de pedidos delivery</h2>
            <p>Lo cobrado en el día, con cada pedido.</p>
          </div>
          <input type="date" value={fecha} max={hoyLima()} onChange={(e) => e.target.value && setFecha(e.target.value)} aria-label="Fecha" />
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          {!datos && !error && <p className="pm-subtitulo">Cargando…</p>}
          {datos && (
            <>
              <div className="dk-cifras">
                <div>
                  <span>Pedidos</span>
                  <strong>{datos.pedidos}</strong>
                </div>
                <div>
                  <span>Total cobrado</span>
                  <strong>{soles(datos.total)}</strong>
                </div>
                <div>
                  <span>En envíos</span>
                  <strong>{soles(datos.envios)}</strong>
                </div>
                <div>
                  <span>Promedio por pedido</span>
                  <strong>{soles(datos.pedidos ? datos.total / datos.pedidos : 0)}</strong>
                </div>
              </div>
              {datos.pedidos === 0 ? (
                <p className="pm-vacio">No hay pedidos cobrados ese día.</p>
              ) : (
                <>
                  <div className="dk-resumen-grupos">
                    <section>
                      <p className="pm-seccion">Por medio de pago</p>
                      {datos.por_metodo.map((m) => (
                        <div key={m.nombre} className="dk-resumen-fila">
                          <span>
                            {nombreMetodo(m.nombre)} <small>· {m.pedidos}</small>
                          </span>
                          <strong>{soles(m.monto)}</strong>
                        </div>
                      ))}
                    </section>
                    <section>
                      <p className="pm-seccion">Por repartidor</p>
                      {datos.por_repartidor.map((m) => (
                        <div key={m.nombre} className="dk-resumen-fila">
                          <span>
                            {m.nombre} <small>· {m.pedidos}</small>
                          </span>
                          <strong>{soles(m.monto)}</strong>
                        </div>
                      ))}
                    </section>
                  </div>
                  <p className="pm-seccion">Pedidos</p>
                  <div className="dk-tabla-envoltura">
                    <table className="dk-tabla">
                      <thead>
                        <tr>
                          <th>Hora</th>
                          <th>N°</th>
                          <th>Cliente</th>
                          <th>Entrega</th>
                          <th>Pago</th>
                          <th>Comprobante</th>
                          <th className="pm-num">Total</th>
                        </tr>
                      </thead>
                      <tbody>
                        {datos.filas.map((f) => (
                          <tr key={f.id}>
                            <td>{f.hora}</td>
                            <td>{f.id}</td>
                            <td>{f.cliente_nombre}</td>
                            <td>{f.entrega === 'RECOJO' ? 'Recojo' : f.repartidor || '—'}</td>
                            <td>
                              {f.partes.map((x) => nombreMetodo(x.metodo)).join(' + ')}
                              <small className="dk-tenue"> · {f.pago === 'ADELANTADO' ? 'adelantado' : 'al entregar'}</small>
                            </td>
                            <td>{f.folio}</td>
                            <td className="pm-num">{soles(f.total)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              )}
            </>
          )}
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton" onClick={onCerrar}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
