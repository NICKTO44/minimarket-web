import { useCallback, useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { api } from '../api/api';
import './AvisosYape.css';

// Cada cuánto se pregunta si llegó un Yape o Plin nuevo.
const REFRESCO_MS = 6000;
// Cuánto queda a la vista cada aviso antes de irse solo.
const DURACION_MS = 15000;
// Tiempo extra si la página estuvo congelada (impresión, otra pestaña).
const DURACION_AL_VOLVER_MS = 8000;
// Avisos a la vista a la vez (los más nuevos).
const MAXIMO = 3;

const nombreBilletera = (b) => (b === 'YAPE' ? 'Yape' : b === 'PLIN' ? 'Plin' : 'Pago');

function soles(monto) {
  return `S/ ${Number(monto).toFixed(2)}`;
}

/**
 * Aviso flotante y discreto de cada Yape o Plin que manda el celular del
 * Yape (módulo AVISOS_PAGO), en cualquier pantalla del sistema. Sin sonido:
 * el sonido lo da el celular de la tienda con la app Monspeet.
 * Al entrar al sistema no salta nada viejo; solo lo que llega después.
 */
export default function AvisosYape() {
  const [avisos, setAvisos] = useState([]);
  const ultimoVisto = useRef(null);
  const temporizadores = useRef(new Map());

  const quitar = useCallback((id) => {
    setAvisos((lista) => lista.filter((a) => a.id !== id));
    const t = temporizadores.current.get(id);
    if (t) clearTimeout(t);
    temporizadores.current.delete(id);
  }, []);

  // Quita el aviso cuando se cumple su tiempo. Si el temporizador llega tarde
  // es que la página estuvo congelada (diálogo de impresión, pestaña oculta):
  // entonces no se vio, y se deja unos segundos más en vez de quitarlo.
  const programar = useCallback(
    (id, ms) => {
      const vence = Date.now() + ms;
      temporizadores.current.set(
        id,
        setTimeout(() => {
          if (Date.now() - vence > 1500) programar(id, DURACION_AL_VOLVER_MS);
          else quitar(id);
        }, ms)
      );
    },
    [quitar]
  );

  const revisar = useCallback(() => {
    if (document.hidden) return;
    api
      .avisosPagosHoy()
      .then((pagos) => {
        const lista = Array.isArray(pagos) ? pagos : [];
        const mayor = lista.reduce((m, p) => Math.max(m, p.id), 0);
        if (ultimoVisto.current === null) {
          ultimoVisto.current = mayor;
          return;
        }
        const nuevos = lista
          .filter((p) => p.id > ultimoVisto.current && p.reconocido && p.monto != null)
          .sort((a, b) => a.id - b.id);
        ultimoVisto.current = Math.max(ultimoVisto.current, mayor);
        if (nuevos.length === 0) return;

        setAvisos((actuales) => [...nuevos.reverse(), ...actuales].slice(0, MAXIMO));
        nuevos.forEach((p) => programar(p.id, DURACION_MS));
      })
      .catch(() => {
        // sin conexión por un momento: se reintenta en la siguiente vuelta
      });
  }, [programar]);

  useEffect(() => {
    revisar();
    const intervalo = setInterval(revisar, REFRESCO_MS);
    const alVolver = () => {
      if (!document.hidden) revisar();
    };
    document.addEventListener('visibilitychange', alVolver);
    const mapa = temporizadores.current;
    return () => {
      clearInterval(intervalo);
      document.removeEventListener('visibilitychange', alVolver);
      mapa.forEach((t) => clearTimeout(t));
      mapa.clear();
    };
  }, [revisar]);

  if (avisos.length === 0) return null;

  return (
    <div className="avisos-yape" role="status" aria-live="polite">
      {avisos.map((p) => (
        <div key={p.id} className={`aviso-yape aviso-yape-${(p.billetera || '').toLowerCase()}`}>
          <span className="aviso-yape-punto" aria-hidden="true" />
          <div className="aviso-yape-cuerpo">
            <p className="aviso-yape-titulo">
              {nombreBilletera(p.billetera)} recibido · <strong>{soles(p.monto)}</strong>
            </p>
            <p className="aviso-yape-texto">
              {p.nombre || 'Sin nombre'}
              {p.codigo_seguridad ? <> · código <strong>{p.codigo_seguridad}</strong></> : null}
              {p.hora ? <> · {p.hora}</> : null}
            </p>
          </div>
          <button type="button" className="aviso-yape-cerrar" onClick={() => quitar(p.id)} aria-label="Cerrar aviso">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
