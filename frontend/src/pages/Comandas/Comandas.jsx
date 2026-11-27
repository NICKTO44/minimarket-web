import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { CheckCircle2, Printer, PrinterCheck, Volume2, VolumeX } from 'lucide-react';
import { api } from '../../api/api';
import { prepararSonido, sonarNuevoPedido } from '../../utils/sonido';
import { tituloPedido } from '../../utils/mesas';
import { etiquetaTicket } from '../../utils/papelTicket';
import TicketMesa from '../../components/TicketMesa';
import { imprimirTicket } from '../../utils/imprimirTicket';
import './Comandas.css';

// Este equipo es la estación de comandas: se guarda en ESTE equipo (como el
// ancho del papel), porque solo la computadora de la ticketera de cocina
// imprime. En los demás equipos la pantalla solo explica cómo activarla.
const CLAVE_ESTACION = 'monspeet_estacion_comandas';
const CLAVE_SONIDO = 'monspeet_sonido_comandas';
const REVISAR_MS = 4000;
// Pausa entre una comanda y la siguiente (Chrome en modo de impresión
// directa no espera a que la ticketera termine).
const PAUSA_ENTRE_COMANDAS_MS = 1500;
const HISTORIAL_MAXIMO = 30;

function leer(clave, porDefecto) {
  try {
    const v = localStorage.getItem(clave);
    return v === null ? porDefecto : v === 'si';
  } catch {
    return porDefecto;
  }
}

function guardar(clave, valor) {
  try {
    localStorage.setItem(clave, valor ? 'si' : 'no');
  } catch {
    // sin almacenamiento: solo dura esta sesión
  }
}

function horaCorta(fecha) {
  // "2026-11-26 12:30:05" -> "12:30"
  const m = /\s(\d{2}:\d{2})/.exec(fecha || '');
  return m ? m[1] : '';
}

const PRUEBA = {
  pedido: { id: 0, tipo: 'LLEVAR', cliente_nombre: 'Prueba de impresión', mesero: null },
  items: [
    { id: 1, cantidad: 1, nombre_producto: 'Comanda de prueba', opciones: null, nota: 'Si salió en cocina, ya está listo' },
  ],
  fecha: '',
  prueba: true,
};

/**
 * Módulo "Impresión de comandas": la estación que imprime sola, en la
 * ticketera de este equipo, cada comanda que envían los mozos (cada una sale
 * una sola vez: el servidor la marca al entregarla). Reemplaza a
 * Preparación: la cocina no marca "listo".
 */
export default function Comandas() {
  const [estacion, setEstacion] = useState(() => leer(CLAVE_ESTACION, false));
  const [sonido, setSonido] = useState(() => leer(CLAVE_SONIDO, true));
  const [ticket, setTicket] = useState(null);
  const [historial, setHistorial] = useState([]);
  const [error, setError] = useState(null);
  const [ultimaRevision, setUltimaRevision] = useState(null);
  const cola = useRef([]);
  const imprimiendo = useRef(false);
  const tomando = useRef(false);
  const numero = useRef(0);
  const sonidoRef = useRef(sonido);
  useEffect(() => {
    sonidoRef.current = sonido;
  }, [sonido]);

  // Saca la siguiente comanda de la cola y la dibuja (el efecto de abajo la imprime).
  const siguiente = useCallback(() => {
    const c = cola.current.shift();
    if (!c) {
      imprimiendo.current = false;
      setTicket(null);
      return;
    }
    imprimiendo.current = true;
    setTicket({ ...c, clave: ++numero.current });
  }, []);

  useEffect(() => {
    if (!ticket) return undefined;
    let pausa;
    const t = setTimeout(() => {
      imprimirTicket();
      pausa = setTimeout(siguiente, PAUSA_ENTRE_COMANDAS_MS);
    }, 80);
    return () => {
      clearTimeout(t);
      clearTimeout(pausa);
    };
  }, [ticket, siguiente]);

  const imprimir = useCallback(
    (comandas) => {
      cola.current.push(...comandas);
      if (!imprimiendo.current) siguiente();
    },
    [siguiente]
  );

  useEffect(() => {
    if (!estacion) return undefined;
    const revisar = () => {
      if (tomando.current) return;
      tomando.current = true;
      api
        .comandasTomar()
        .then((comandas) => {
          setError(null);
          setUltimaRevision(new Date());
          if (!comandas.length) return;
          if (sonidoRef.current) sonarNuevoPedido();
          setHistorial((h) => [...comandas.slice().reverse(), ...h].slice(0, HISTORIAL_MAXIMO));
          imprimir(comandas);
        })
        .catch((e) => setError(e.message))
        .finally(() => {
          tomando.current = false;
        });
    };
    revisar();
    const intervalo = setInterval(revisar, REVISAR_MS);
    return () => clearInterval(intervalo);
  }, [estacion, imprimir]);

  const cambiarEstacion = (valor) => {
    prepararSonido();
    setEstacion(valor);
    guardar(CLAVE_ESTACION, valor);
    if (!valor) setUltimaRevision(null);
  };

  const cambiarSonido = () => {
    const nuevo = !sonido;
    if (nuevo) prepararSonido();
    setSonido(nuevo);
    guardar(CLAVE_SONIDO, nuevo);
  };

  return (
    <div className="cmd-layout">
      <div className="cmd-top">
        <h1>
          <Printer size={22} /> Comandas
        </h1>
        <button type="button" className="cmd-sonido" onClick={cambiarSonido} title="Sonido al imprimir una comanda">
          {sonido ? <Volume2 size={16} /> : <VolumeX size={16} />}
        </button>
      </div>

      {estacion ? (
        <div className="cmd-card cmd-activa">
          <div className="cmd-estado">
            <PrinterCheck size={28} />
            <div>
              <strong>Este equipo imprime las comandas</strong>
              <span>
                Deja esta ventana abierta. Cada pedido que envíe un mozo sale solo en la ticketera de este equipo (
                {etiquetaTicket()}).
                {ultimaRevision && ` Última revisión: ${ultimaRevision.toLocaleTimeString('es-PE')}.`}
              </span>
            </div>
          </div>
          <div className="cmd-acciones">
            <button type="button" className="cmd-secundario" onClick={() => imprimir([PRUEBA])}>
              Imprimir una prueba
            </button>
            <button type="button" className="cmd-secundario" onClick={() => cambiarEstacion(false)}>
              Dejar de imprimir aquí
            </button>
          </div>
        </div>
      ) : (
        <div className="cmd-card">
          <p className="cmd-texto">
            Activa esta opción <strong>solo en la computadora que tiene la ticketera de cocina</strong> (conectada por USB
            o por red). Desde ese momento, cada pedido que envíen los mozos desde su celular se imprime solo allí.
          </p>
          <button type="button" className="cmd-principal" onClick={() => cambiarEstacion(true)}>
            <Printer size={18} /> Imprimir comandas en este equipo
          </button>
          <p className="cmd-nota">
            Para que imprima sin preguntar, abre esta ventana en Chrome con la impresión directa activada y la ticketera
            de cocina como impresora. El ancho del papel se elige en Configuración → Impresora de tickets.
          </p>
        </div>
      )}

      {error && <p className="cmd-error">No se pudo revisar si hay comandas nuevas: {error}. Se vuelve a intentar solo.</p>}

      {estacion && (
        <div className="cmd-card">
          <h3 className="cmd-subtitulo">Impresas en este equipo</h3>
          {historial.length === 0 ? (
            <p className="cmd-vacio">Aún no llega ninguna comanda. Aparecen aquí apenas un mozo envía un pedido.</p>
          ) : (
            <div className="cmd-lista">
              {historial.map((c, i) => (
                <div key={`${c.pedido.id}-${c.fecha}-${i}`} className="cmd-fila">
                  <CheckCircle2 size={16} className="cmd-ok" />
                  <span className="cmd-hora">{horaCorta(c.fecha)}</span>
                  <span className="cmd-destino">
                    {tituloPedido(c.pedido)}
                    {c.pedido.mesero ? ` · ${c.pedido.mesero.split(' ')[0]}` : ''}
                  </span>
                  <span className="cmd-platos">
                    {c.items.map((it) => `${it.cantidad}× ${it.nombre_producto}`).join(', ')}
                  </span>
                  <button type="button" onClick={() => imprimir([c])} title="Volver a imprimir esta comanda">
                    <Printer size={14} /> Reimprimir
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {ticket &&
        createPortal(
          <TicketMesa key={ticket.clave} tipo="COMANDA" pedido={ticket.pedido} items={ticket.items} />,
          document.body
        )}
    </div>
  );
}
