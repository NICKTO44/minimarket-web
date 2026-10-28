import { useCallback, useEffect, useMemo, useState } from 'react';
import { Bike, Check, ChefHat, Clock, MapPin, MessageCircle, Navigation, Phone, RefreshCw, Wallet } from 'lucide-react';
import { api } from '../../api/api';
import { tiempoAbierto } from '../../utils/mesas';
import { enlaceWhatsapp } from '../../utils/vehiculos';
import '../../components/PantallaModulo.css';
import '../Mesas/Mesas.css';
import './PedidosDelivery.css';

// "Mis entregas": lo que ve el repartidor con su cuenta (rol REPARTIDOR,
// módulo Pedidos delivery). Sus pedidos en el orden que le asignó el cajero.
//  - Ya pagado (adelantado): lleva el comprobante impreso y solo marca Entregado.
//  - Contra entrega: delante del cliente toca Cobrar (el punto de venta con el
//    pedido cargado: elige cómo pagó, nota de venta o boleta, y el comprobante
//    se envía por WhatsApp). Con eso queda entregado.
// Backend: handlers/delivery.rs (/reparto).

const REFRESCO_MS = 15000;
const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const numeroDe = (texto) => {
  const n = parseFloat(String(texto ?? '').replace(',', '.'));
  return Number.isFinite(n) ? n : null;
};
const minutosDesde = (fecha) => {
  if (!fecha) return null;
  const t = new Date(fecha.replace(' ', 'T') + '-05:00').getTime();
  return Number.isFinite(t) ? Math.max(0, Math.round((Date.now() - t) / 60000)) : null;
};
const enlaceMapa = (p) => `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(p.direccion || '')}`;
const enlaceLlamar = (telefono) => {
  const n = String(telefono || '').replace(/[^\d+]/g, '');
  return n ? `tel:${n}` : null;
};

/** Qué hacer con el pago, en grande. */
function QueCobrar({ p }) {
  if (p.cobrado) {
    return (
      <div className="dkr-cobro dkr-cobro-ok">
        <Check size={18} />
        <div>
          <strong>YA PAGADO: no cobres</strong>
          <span>Entrega el comprobante impreso{p.folio ? ` (${p.folio})` : ''}.</span>
        </div>
      </div>
    );
  }
  if (p.pago === 'ADELANTADO') {
    return (
      <div className="dkr-cobro dkr-cobro-espera">
        <Clock size={18} />
        <div>
          <strong>Aún no pagó</strong>
          <span>No salgas hasta que caja confirme el pago.</span>
        </div>
      </div>
    );
  }
  return (
    <div className="dkr-cobro dkr-cobro-cobrar">
      <Wallet size={18} />
      <div>
        <strong>COBRAR {soles(p.total)}</strong>
        <span>{p.paga_con ? `Paga con ${soles(p.paga_con)}: lleva ${soles(p.vuelto)} de vuelto.` : 'Al entregar, en efectivo, Yape o Plin.'}</span>
      </div>
    </div>
  );
}

export default function MisEntregas({ nombreTienda, onCobrar }) {
  const [datos, setDatos] = useState({ nombre: '', pedidos: [] });
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(null);
  const [saliendo, setSaliendo] = useState(null);
  const tienda = nombreTienda || 'la cocina';

  const cargar = useCallback(
    () =>
      api
        .repartoMisEntregas()
        .then((d) => {
          setDatos(d);
          setError(null);
        })
        .catch((e) => setError(e.message))
        .finally(() => setCargando(false)),
    []
  );

  useEffect(() => {
    cargar();
    const intervalo = setInterval(cargar, REFRESCO_MS);
    const alVolver = () => document.visibilityState === 'visible' && cargar();
    document.addEventListener('visibilitychange', alVolver);
    return () => {
      clearInterval(intervalo);
      document.removeEventListener('visibilitychange', alVolver);
    };
  }, [cargar]);

  const grupos = useMemo(() => {
    const p = datos.pedidos;
    return {
      camino: p.filter((x) => x.estado === 'EN_CAMINO'),
      porSalir: p.filter((x) => x.estado === 'EN_COCINA'),
      entregados: p.filter((x) => x.estado === 'ENTREGADO'),
    };
  }, [datos.pedidos]);

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

  // Cobro en la puerta: el punto de venta con el pedido tal cual.
  const cobrar = (p) =>
    accion(p.id, async () => {
      const actual = await api.pedido(p.id);
      onCobrar({ ...actual, items: actual.items.filter((i) => i.estado !== 'ANULADO') });
    });

  const tarjeta = (p) => {
    const salida = minutosDesde(p.fecha_salida);
    const llamar = enlaceLlamar(p.telefono);
    const whatsapp = enlaceWhatsapp(
      p.telefono,
      `Hola ${(p.cliente_nombre || '').split(' ')[0]} 🛵 Soy ${datos.nombre.split(' ')[0] || 'el repartidor'} de ${tienda}, ya voy con tu pedido N° ${p.id}.${
        p.cobrado ? '' : ` Total: ${soles(p.total)}.`
      }`
    );
    const puedeSalir = p.pago !== 'ADELANTADO' || p.cobrado;
    return (
      <article key={p.id} className={`dkr-tarjeta${p.estado === 'ENTREGADO' ? ' dkr-hecha' : ''}`}>
        <header className="dkr-cabecera">
          {p.orden_reparto != null && p.estado !== 'ENTREGADO' && <span className="dkr-orden">{p.orden_reparto}°</span>}
          <div className="dkr-titulo">
            <strong>Pedido N° {p.id}</strong>
            <span>{p.cliente_nombre}</span>
          </div>
          <span className="dk-tarjeta-tiempo">
            <Clock size={12} /> {p.estado === 'EN_CAMINO' && salida != null ? `salí hace ${tiempoAbierto(salida)}` : tiempoAbierto(p.minutos)}
          </span>
        </header>

        {p.estado !== 'ENTREGADO' && (
          <>
            <p className="dkr-direccion">
              <MapPin size={18} /> {p.direccion}
            </p>
            {p.referencia && <p className="dkr-referencia">{p.referencia}</p>}
            <ul className="dk-lineas">
              {p.lineas.map((l) => (
                <li key={l.id}>
                  {l.cantidad} × {l.nombre}
                  {l.opciones && <small> ({l.opciones})</small>}
                </li>
              ))}
            </ul>
            {p.notas && <p className="dk-tenue dk-notas">{p.notas}</p>}
            {p.estado === 'EN_COCINA' && (
              <p className={p.fase === 'LISTO' ? 'dkr-cocina dkr-cocina-listo' : 'dkr-cocina'}>
                <ChefHat size={14} /> {p.fase === 'LISTO' ? 'Listo: recógelo en cocina' : p.fase === 'ESPERANDO_PAGO' ? 'Aún no entra a cocina' : 'En cocina, preparando'}
              </p>
            )}
            <QueCobrar p={p} />
            <div className="dkr-contacto">
              <a className="mesas-boton-secundario dk-enlace" href={enlaceMapa(p)} target="_blank" rel="noreferrer">
                <Navigation size={15} /> Mapa
              </a>
              {llamar && (
                <a className="mesas-boton-secundario dk-enlace" href={llamar}>
                  <Phone size={15} /> Llamar
                </a>
              )}
              {whatsapp && (
                <a className="mesas-boton-secundario dk-enlace" href={whatsapp} target="_blank" rel="noreferrer">
                  <MessageCircle size={15} /> WhatsApp
                </a>
              )}
            </div>
            <div className="dkr-acciones">
              {p.estado === 'EN_COCINA' && (
                <button
                  type="button"
                  className="mesas-boton-primario dkr-boton"
                  disabled={ocupado === p.id || !puedeSalir || p.pendientes > 0}
                  onClick={() => (p.cobrado ? accion(p.id, () => api.repartoSalir(p.id, null)) : setSaliendo(p))}
                >
                  <Bike size={18} /> Salgo con este pedido
                </button>
              )}
              {p.estado === 'EN_CAMINO' && !p.cobrado && (
                <button type="button" className="mesas-boton-primario dkr-boton dkr-boton-cobrar" disabled={ocupado === p.id} onClick={() => cobrar(p)}>
                  <Wallet size={18} /> Cobrar {soles(p.total)}
                </button>
              )}
              {p.estado === 'EN_CAMINO' && p.cobrado && (
                <button type="button" className="mesas-boton-primario dkr-boton" disabled={ocupado === p.id} onClick={() => accion(p.id, () => api.repartoEntregar(p.id))}>
                  <Check size={18} /> Entregado
                </button>
              )}
            </div>
          </>
        )}
        {p.estado === 'ENTREGADO' && (
          <p className="dk-tenue">
            {p.direccion} · {soles(p.total)} · {p.folio || ''}
          </p>
        )}
      </article>
    );
  };

  return (
    <div className="mesas-layout dk-layout dkr-layout">
      <div className="mesas-header">
        <div>
          <h1>Mis entregas</h1>
          <p className="mesas-subtitulo">
            {datos.nombre} · {grupos.porSalir.length} por salir · {grupos.camino.length} en camino · {grupos.entregados.length} entregados hoy
          </p>
        </div>
        <div className="mesas-header-acciones">
          <button type="button" className="mesas-boton-secundario" onClick={cargar} aria-label="Actualizar">
            <RefreshCw size={16} />
          </button>
        </div>
      </div>

      {error && <p className="mesas-error">{error}</p>}
      {cargando && <p className="mesas-vacio">Cargando...</p>}
      {!cargando && datos.pedidos.length === 0 && <p className="mesas-vacio">No tienes pedidos asignados. Caja te los asigna cuando estén listos.</p>}

      {grupos.camino.length > 0 && (
        <section>
          <h2 className="mesas-zona-titulo">
            <Bike size={15} /> En camino ({grupos.camino.length})
          </h2>
          {grupos.camino.map(tarjeta)}
        </section>
      )}
      {grupos.porSalir.length > 0 && (
        <section>
          <h2 className="mesas-zona-titulo">
            <ChefHat size={15} /> Por salir ({grupos.porSalir.length})
          </h2>
          {grupos.porSalir.map(tarjeta)}
        </section>
      )}
      {grupos.entregados.length > 0 && (
        <section>
          <h2 className="mesas-zona-titulo">
            <Check size={15} /> Entregados hoy ({grupos.entregados.length})
          </h2>
          {grupos.entregados.map(tarjeta)}
        </section>
      )}

      {saliendo && (
        <DialogoSalida
          pedido={saliendo}
          onCerrar={() => setSaliendo(null)}
          onListo={() => {
            setSaliendo(null);
            cargar();
          }}
        />
      )}
    </div>
  );
}

/** Contra entrega: con cuánto paga el cliente, si ya lo sabe (para el vuelto). */
function DialogoSalida({ pedido, onCerrar, onListo }) {
  const [pagaCon, setPagaCon] = useState(pedido.paga_con ? String(pedido.paga_con) : '');
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const monto = numeroDe(pagaCon);
  const vuelto = monto > 0 ? Math.round((monto - pedido.total) * 100) / 100 : null;
  const salir = async () => {
    if (monto > 0 && vuelto < 0) return setError(`Con ${soles(monto)} no alcanza: el total es ${soles(pedido.total)}.`);
    setOcupado(true);
    setError(null);
    try {
      await api.repartoSalir(pedido.id, monto > 0 ? monto : null);
      onListo();
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };
  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal" role="dialog" aria-modal="true" aria-label="Salir con el pedido">
        <div className="pm-modal-cabecera">
          <h2>Pedido N° {pedido.id}</h2>
          <p>Cobras {soles(pedido.total)} al entregar.</p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          <div className="pm-campos">
            <label className="pm-campo pm-campo-ancho">
              <span>Si paga en efectivo: ¿con cuánto? (opcional)</span>
              <input inputMode="decimal" value={pagaCon} placeholder={`Ej. ${Math.ceil(pedido.total / 10) * 10}`} onChange={(e) => setPagaCon(e.target.value)} />
            </label>
          </div>
          {vuelto != null && vuelto >= 0 && (
            <p className="pm-subtitulo">
              Lleva <strong>{soles(vuelto)}</strong> de vuelto.
            </p>
          )}
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cancelar
          </button>
          <button className="pm-boton" onClick={salir} disabled={ocupado}>
            {ocupado ? 'Saliendo...' : 'Salgo en camino'}
          </button>
        </div>
      </div>
    </div>
  );
}
