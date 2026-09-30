import { useCallback, useEffect, useMemo, useState } from 'react';
import { Bell, Bike, Clock, Plus, RefreshCw, ShoppingBag, Users } from 'lucide-react';
import { api } from '../../api/api';
import PedidoMesa from './PedidoMesa';
import DialogoTexto from './DialogoTexto';
import { tiempoAbierto } from '../../utils/mesas';
import './Mesas.css';

// Cada cuánto se refresca el mapa solo (otros meseros abren y cobran mesas,
// barra/cocina marca pedidos listos).
const REFRESCO_MS = 8000;

// Etiqueta de estado de una mesa ocupada: lo más urgente primero.
function EstadoPedido({ p }) {
  if (p.listos > 0) {
    return (
      <span className="mesa-card-listo">
        <Bell size={11} /> {p.listos} {p.listos === 1 ? 'listo' : 'listos'}
      </span>
    );
  }
  if (p.pendientes > 0) return <span className="mesa-card-alerta">{p.pendientes} por enviar</span>;
  if (p.preparando > 0) return <span className="mesa-card-preparando">{p.preparando} preparando</span>;
  return null;
}

/**
 * Pantalla principal de una cafetería / restaurante: el mapa de mesas.
 * Tocar una mesa libre la abre; tocar una ocupada muestra su pedido.
 * El cobro lo hace el POS de siempre (onCobrar).
 */
export default function Mesas({ usuario, nombreTienda, onCobrar, abrirPedido, onAbrirPedidoUsado }) {
  const [mesas, setMesas] = useState([]);
  const [abiertos, setAbiertos] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [pedidoId, setPedidoId] = useState(null);
  // Pedido que se pidió abrir desde afuera (aviso de "listo", o volver de
  // un cobro cancelado). Se aplica una sola vez por pedido de apertura.
  const [aperturaVista, setAperturaVista] = useState(null);
  if (abrirPedido && abrirPedido.n !== aperturaVista) {
    setAperturaVista(abrirPedido.n);
    setPedidoId(abrirPedido.id);
  }
  const [abriendo, setAbriendo] = useState(null);
  const [dialogo, setDialogo] = useState(null);

  const cargar = useCallback(() => {
    return Promise.all([api.mesas(), api.pedidosAbiertos()])
      .then(([m, p]) => {
        setMesas(m);
        setAbiertos(p);
        setError(null);
      })
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, []);

  useEffect(() => {
    if (abrirPedido) onAbrirPedidoUsado?.();
  }, [abrirPedido, onAbrirPedidoUsado]);

  useEffect(() => {
    if (pedidoId) return undefined;
    cargar();
    const intervalo = setInterval(cargar, REFRESCO_MS);
    const alVolver = () => document.visibilityState === 'visible' && cargar();
    document.addEventListener('visibilitychange', alVolver);
    return () => {
      clearInterval(intervalo);
      document.removeEventListener('visibilitychange', alVolver);
    };
  }, [pedidoId, cargar]);

  const zonas = useMemo(() => {
    const mapa = new Map();
    mesas.forEach((m) => {
      if (!mapa.has(m.zona)) mapa.set(m.zona, []);
      mapa.get(m.zona).push(m);
    });
    return [...mapa.entries()];
  }, [mesas]);

  const sinMesa = abiertos.filter((p) => p.tipo !== 'MESA');
  const ocupadas = mesas.filter((m) => m.pedido).length;

  const abrirMesa = async (mesa) => {
    if (mesa.pedido) {
      setPedidoId(mesa.pedido.id);
      return;
    }
    setAbriendo(mesa.id);
    try {
      const pedido = await api.pedidoAbrir({ tipo: 'MESA', mesa_id: mesa.id });
      setPedidoId(pedido.id);
    } catch (e) {
      setError(e.message);
    } finally {
      setAbriendo(null);
    }
  };

  const nuevoSinMesa = (tipo) => {
    setDialogo({
      titulo: tipo === 'DELIVERY' ? 'Nuevo delivery' : 'Nuevo pedido para llevar',
      mensaje: 'Nombre del cliente, para llamarlo cuando esté listo (opcional).',
      placeholder: 'Ej. Ana',
      textoConfirmar: 'Abrir pedido',
      alConfirmar: async (nombre) => {
        const pedido = await api.pedidoAbrir({ tipo, cliente_nombre: nombre || null });
        setPedidoId(pedido.id);
      },
    });
  };

  if (pedidoId) {
    return (
      <PedidoMesa
        pedidoId={pedidoId}
        usuario={usuario}
        nombreTienda={nombreTienda}
        mesas={mesas}
        onVolver={() => setPedidoId(null)}
        onCobrar={onCobrar}
      />
    );
  }

  return (
    <div className="mesas-layout">
      <div className="mesas-header">
        <div>
          <h1>Mesas</h1>
          <p className="mesas-subtitulo">
            {ocupadas} de {mesas.length} ocupadas
            {sinMesa.length > 0 && ` · ${sinMesa.length} para llevar`}
          </p>
        </div>
        <div className="mesas-header-acciones">
          <button type="button" className="mesas-boton-secundario" onClick={cargar} aria-label="Actualizar">
            <RefreshCw size={16} />
          </button>
          <button type="button" className="mesas-boton-secundario" onClick={() => nuevoSinMesa('DELIVERY')}>
            <Bike size={16} /> Delivery
          </button>
          <button type="button" className="mesas-boton-primario" onClick={() => nuevoSinMesa('LLEVAR')}>
            <ShoppingBag size={16} /> Para llevar
          </button>
        </div>
      </div>

      <div className="mesas-leyenda">
        <span><i className="mesas-punto mesas-punto-libre" /> Libre</span>
        <span><i className="mesas-punto mesas-punto-ocupada" /> Ocupada</span>
        <span><i className="mesas-punto mesas-punto-pendiente" /> Falta enviar a preparar</span>
        <span><i className="mesas-punto mesas-punto-listo" /> Listo para entregar</span>
      </div>

      {error && <p className="mesas-error">{error}</p>}
      {cargando && <p className="mesas-vacio">Cargando mesas...</p>}

      {!cargando && mesas.length === 0 && (
        <p className="mesas-vacio">
          Todavía no hay mesas. El administrador puede agregarlas en Configuración → Mesas y opciones.
        </p>
      )}

      <div className="mesas-contenido">
        {sinMesa.length > 0 && (
          <section>
            <h2 className="mesas-zona-titulo">Para llevar y delivery</h2>
            <div className="mesas-grid">
              {sinMesa.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={`mesa-card ocupada${p.listos ? ' listo' : ''}`}
                  onClick={() => setPedidoId(p.id)}
                >
                  <span className="mesa-card-top">
                    <span className="mesa-card-nombre">
                      {p.tipo === 'DELIVERY' ? <Bike size={16} /> : <ShoppingBag size={16} />}
                      {p.cliente_nombre || `#${p.id}`}
                    </span>
                    <EstadoPedido p={p} />
                  </span>
                  <span className="mesa-card-total">S/ {p.total.toFixed(2)}</span>
                  <span className="mesa-card-meta">
                    <Clock size={12} /> {tiempoAbierto(p.minutos_abierto)}
                    {p.mesero ? ` · ${p.mesero.split(' ')[0]}` : ''}
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}

        {zonas.map(([zona, lista]) => (
          <section key={zona}>
            <h2 className="mesas-zona-titulo">{zona}</h2>
            <div className="mesas-grid">
              {lista.map((m) => {
                const p = m.pedido;
                return (
                  <button
                    key={m.id}
                    type="button"
                    className={`mesa-card${p ? ' ocupada' : ''}${p?.pendientes ? ' pendiente' : ''}${p?.listos ? ' listo' : ''}`}
                    onClick={() => abrirMesa(m)}
                    disabled={abriendo === m.id}
                  >
                    <span className="mesa-card-top">
                      <span className="mesa-card-nombre">{m.nombre}</span>
                      {p && <EstadoPedido p={p} />}
                    </span>
                    {p ? (
                      <>
                        <span className="mesa-card-total">S/ {p.total.toFixed(2)}</span>
                        <span className="mesa-card-meta">
                          <Clock size={12} /> {tiempoAbierto(p.minutos_abierto)}
                          {p.mesero ? ` · ${p.mesero.split(' ')[0]}` : ''}
                        </span>
                      </>
                    ) : (
                      <>
                        <span className="mesa-card-libre">
                          <Plus size={14} /> {abriendo === m.id ? 'Abriendo...' : 'Abrir mesa'}
                        </span>
                        <span className="mesa-card-meta">
                          <Users size={12} /> {m.capacidad} personas
                        </span>
                      </>
                    )}
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>

      {dialogo && <DialogoTexto {...dialogo} onCerrar={() => setDialogo(null)} />}
    </div>
  );
}
