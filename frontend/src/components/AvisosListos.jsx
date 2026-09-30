import { useCallback, useEffect, useRef, useState } from 'react';
import { Bell, Check, X } from 'lucide-react';
import { api } from '../api/api';
import { esMesero } from '../utils/menu';
import { sonarListo, vibrar } from '../utils/sonido';
import './AvisosListos.css';

// Cada cuánto se revisa si barra/cocina marcó algo como listo.
const REFRESCO_MS = 5000;

function titulo(t) {
  if (t.tipo === 'MESA') return t.mesa_nombre || 'Mesa';
  const base = t.tipo === 'DELIVERY' ? 'Delivery' : 'Para llevar';
  return t.cliente_nombre ? `${base} · ${t.cliente_nombre}` : base;
}

function resumenItems(items) {
  return items
    .map((i) => `${i.cantidad}× ${i.nombre_producto}${i.opciones ? ` (${i.opciones})` : ''}`)
    .join(', ');
}

/**
 * Avisos de "pedido listo" para el mozo y el cajero, en cualquier pantalla.
 *  - Mozo: lo de los pedidos que él tomó.
 *  - Cajero / administrador: para llevar y delivery (los llama por su
 *    nombre), y las mesas que él mismo abrió.
 * Suena y vibra con cada producto que barra/cocina marca como listo.
 */
export default function AvisosListos({ usuario, onAbrirPedido, onConteo }) {
  const [avisos, setAvisos] = useState([]);
  const vistos = useRef(null);
  const mesero = esMesero(usuario);

  const esMio = useCallback(
    (t) => (mesero ? t.usuario_id === usuario.id : t.tipo !== 'MESA' || t.usuario_id === usuario.id),
    [mesero, usuario.id]
  );

  const revisar = useCallback(() => {
    api
      .preparacion()
      .then((tickets) => {
        const mios = tickets
          .filter(esMio)
          .map((t) => ({ ...t, items: t.items.filter((i) => i.listo) }))
          .filter((t) => t.items.length > 0);
        onConteo?.(mios.length);

        const primeraVez = vistos.current === null;
        const antes = vistos.current || new Set();
        const nuevos = mios.filter((t) => t.items.some((i) => !antes.has(i.id)));
        vistos.current = new Set(mios.flatMap((t) => t.items.map((i) => i.id)));

        // Se muestran los pedidos con algo recién listo, y se mantienen los
        // avisos abiertos mientras sigan sin entregarse. Al entrar al sistema
        // no salta nada viejo (para eso está el número en "Mesas").
        const conNovedad = new Set(nuevos.map((t) => t.pedido_id));
        setAvisos((actuales) =>
          primeraVez
            ? []
            : mios.filter((t) => conNovedad.has(t.pedido_id) || actuales.some((a) => a.pedido_id === t.pedido_id)).slice(0, 4)
        );

        if (!primeraVez && nuevos.length > 0) {
          sonarListo();
          vibrar();
        }
      })
      .catch(() => {
        // sin conexión por un momento: se reintenta en la siguiente vuelta
      });
  }, [esMio, onConteo]);

  useEffect(() => {
    revisar();
    const intervalo = setInterval(revisar, REFRESCO_MS);
    return () => clearInterval(intervalo);
  }, [revisar]);

  const entregar = (aviso) => {
    setAvisos((lista) => lista.filter((a) => a.pedido_id !== aviso.pedido_id));
    api
      .preparacionEntregado(aviso.items.map((i) => i.id))
      .then(revisar)
      .catch(revisar);
  };

  const cerrar = (aviso) => setAvisos((lista) => lista.filter((a) => a.pedido_id !== aviso.pedido_id));

  if (avisos.length === 0) return null;

  return (
    <div className="avisos-listos" role="status" aria-live="polite">
      {avisos.map((a) => (
        <div key={a.pedido_id} className="aviso-listo">
          <span className="aviso-listo-icono">
            <Bell size={18} />
          </span>
          <div className="aviso-listo-texto">
            <strong>
              {titulo(a)}: {a.tipo === 'MESA' ? 'listo para entregar' : 'listo · llamar al cliente'}
            </strong>
            <span>{resumenItems(a.items)}</span>
            <div className="aviso-listo-acciones">
              <button type="button" className="aviso-listo-entregado" onClick={() => entregar(a)}>
                <Check size={14} /> Entregado
              </button>
              {a.tipo === 'MESA' && !a.cobrado && onAbrirPedido && (
                <button
                  type="button"
                  onClick={() => {
                    cerrar(a);
                    onAbrirPedido(a.pedido_id);
                  }}
                >
                  Ver mesa
                </button>
              )}
            </div>
          </div>
          <button type="button" className="aviso-listo-cerrar" onClick={() => cerrar(a)} aria-label="Cerrar aviso">
            <X size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}
