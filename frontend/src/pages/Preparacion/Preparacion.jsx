import { useCallback, useEffect, useRef, useState } from 'react';
import { Bike, Check, ChefHat, RotateCcw, ShoppingBag, UtensilsCrossed, Volume2, VolumeX } from 'lucide-react';
import { api } from '../../api/api';
import { prepararSonido, sonarNuevoPedido } from '../../utils/sonido';
import './Preparacion.css';

// Cada cuánto se revisa si llegaron pedidos nuevos.
const REFRESCO_MS = 5000;

// Color del ticket según lo que lleva esperando.
function nivelEspera(minutos) {
  if (minutos >= 15) return 'rojo';
  if (minutos >= 8) return 'ambar';
  return 'verde';
}

function titulo(t) {
  if (t.tipo === 'MESA') return t.mesa_nombre || 'Mesa';
  const base = t.tipo === 'DELIVERY' ? 'Delivery' : 'Para llevar';
  return t.cliente_nombre ? `${base} · ${t.cliente_nombre}` : base;
}

function IconoTipo({ tipo }) {
  if (tipo === 'DELIVERY') return <Bike size={16} />;
  if (tipo === 'LLEVAR') return <ShoppingBag size={16} />;
  return <UtensilsCrossed size={16} />;
}

/**
 * Pantalla de barra / cocina: lo que los mozos mandaron a preparar. Al
 * tocar "Listo", al mozo de esa mesa (o al cajero, si es para llevar) le
 * llega el aviso para que lo entregue.
 */
export default function Preparacion() {
  const [tickets, setTickets] = useState([]);
  const [vista, setVista] = useState('PREPARANDO');
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [sonido, setSonido] = useState(() => {
    try {
      return localStorage.getItem('monspeet_sonido_preparacion') !== 'no';
    } catch {
      return true;
    }
  });
  const vistos = useRef(null); // ids de líneas ya vistas (para sonar solo con lo nuevo)
  const sonidoRef = useRef(sonido);
  useEffect(() => {
    sonidoRef.current = sonido;
  }, [sonido]);

  const recibir = useCallback((lista) => {
    const ids = lista.flatMap((t) => t.items.filter((i) => !i.listo).map((i) => i.id));
    if (vistos.current && ids.some((id) => !vistos.current.has(id)) && sonidoRef.current) {
      sonarNuevoPedido();
    }
    vistos.current = new Set([...(vistos.current || []), ...ids]);
    setTickets(lista);
    setError(null);
  }, []);

  const cargar = useCallback(
    () =>
      api
        .preparacion()
        .then(recibir)
        .catch((e) => setError(e.message))
        .finally(() => setCargando(false)),
    [recibir]
  );

  useEffect(() => {
    cargar();
    const intervalo = setInterval(cargar, REFRESCO_MS);
    return () => clearInterval(intervalo);
  }, [cargar]);

  const cambiarSonido = () => {
    const nuevo = !sonido;
    setSonido(nuevo);
    if (nuevo) prepararSonido();
    try {
      localStorage.setItem('monspeet_sonido_preparacion', nuevo ? 'si' : 'no');
    } catch {
      // sin almacenamiento: solo dura esta sesión
    }
  };

  // Se marca al instante en pantalla y se guarda en segundo plano.
  const marcar = (ids, listo) => {
    prepararSonido();
    setTickets((lista) =>
      lista.map((t) => ({ ...t, items: t.items.map((i) => (ids.includes(i.id) ? { ...i, listo, minutos_listo: 0 } : i)) }))
    );
    api
      .preparacionListo(ids, listo)
      .then(recibir)
      .catch((e) => {
        setError(e.message);
        cargar();
      });
  };

  // "Ya se entregó": la barra entregó en el mostrador, o limpia un pedido
  // que quedó listo sin que nadie lo marcara (p. ej. una mesa ya pagada).
  const entregar = (ids) => {
    setTickets((lista) =>
      lista.map((t) => ({ ...t, items: t.items.filter((i) => !ids.includes(i.id)) })).filter((t) => t.items.length)
    );
    api
      .preparacionEntregado(ids)
      .then(recibir)
      .catch((e) => {
        setError(e.message);
        cargar();
      });
  };

  const preparando = tickets
    .map((t) => ({ ...t, items: t.items.filter((i) => !i.listo) }))
    .filter((t) => t.items.length > 0);
  const listos = tickets
    .map((t) => ({ ...t, items: t.items.filter((i) => i.listo) }))
    .filter((t) => t.items.length > 0);
  const lineasListas = listos.reduce((s, t) => s + t.items.length, 0);
  const visibles = vista === 'PREPARANDO' ? preparando : listos;

  return (
    <div className="prep-layout">
      <div className="prep-top">
        <h1>
          <ChefHat size={22} /> Preparación
        </h1>
        <div className="prep-tabs">
          <button type="button" className={vista === 'PREPARANDO' ? 'activo' : ''} onClick={() => setVista('PREPARANDO')}>
            Por preparar ({preparando.length})
          </button>
          <button type="button" className={vista === 'LISTOS' ? 'activo' : ''} onClick={() => setVista('LISTOS')}>
            Listos sin entregar ({lineasListas})
          </button>
          <button type="button" className="prep-sonido" onClick={cambiarSonido} title="Sonido al llegar un pedido">
            {sonido ? <Volume2 size={16} /> : <VolumeX size={16} />}
          </button>
        </div>
      </div>

      {error && <p className="prep-error">{error}</p>}
      {cargando && <p className="prep-vacio">Cargando...</p>}
      {!cargando && visibles.length === 0 && (
        <p className="prep-vacio">
          {vista === 'PREPARANDO'
            ? 'No hay nada por preparar. Los pedidos aparecen aquí cuando el mozo los envía.'
            : 'Nada pendiente de entrega.'}
        </p>
      )}

      <div className="prep-grid">
        {visibles.map((t) => {
          const espera = Math.max(...t.items.map((i) => (vista === 'PREPARANDO' ? i.minutos_espera : i.minutos_listo)));
          const nivel = vista === 'PREPARANDO' ? nivelEspera(espera) : 'listo';
          return (
            <div key={t.pedido_id} className="prep-ticket">
              <div className={`prep-ticket-top prep-${nivel}`}>
                <span className="prep-ticket-titulo">
                  <IconoTipo tipo={t.tipo} /> {titulo(t)}
                </span>
                <span className="prep-ticket-tiempo">{espera} min</span>
              </div>
              <div className="prep-ticket-meta">
                #{t.pedido_id}
                {t.mesero ? ` · ${t.mesero.split(' ')[0]}` : ''}
                {t.cobrado && <span className="prep-pagado">Pagado</span>}
              </div>
              {t.items.map((i) => (
                <div key={i.id} className="prep-item">
                  <span className="prep-item-cantidad">{i.cantidad}×</span>
                  <span className="prep-item-texto">
                    {i.nombre_producto}
                    {i.opciones && <span className="prep-item-opciones">{i.opciones}</span>}
                    {i.nota && <span className="prep-item-nota">» {i.nota}</span>}
                  </span>
                  {vista === 'PREPARANDO' ? (
                    <button type="button" className="prep-item-listo" onClick={() => marcar([i.id], true)}>
                      <Check size={14} /> Listo
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="prep-item-deshacer"
                      onClick={() => marcar([i.id], false)}
                      title="Se marcó por error: vuelve a preparación"
                    >
                      <RotateCcw size={14} />
                    </button>
                  )}
                </div>
              ))}
              {vista === 'PREPARANDO' ? (
                <button type="button" className="prep-todo-listo" onClick={() => marcar(t.items.map((i) => i.id), true)}>
                  <Check size={16} /> Todo listo
                </button>
              ) : (
                <div className="prep-pie-listo">
                  <span className="prep-esperando">
                    Esperando que el {t.tipo === 'MESA' ? 'mozo' : 'cajero'} lo entregue
                  </span>
                  <button type="button" className="prep-entregado" onClick={() => entregar(t.items.map((i) => i.id))}>
                    <Check size={15} /> Ya se entregó
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
