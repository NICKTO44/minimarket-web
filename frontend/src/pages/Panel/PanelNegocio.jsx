import { useState } from 'react';
import { ArrowLeft, CalendarClock, FileCheck2 } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { confirmar } from '../../utils/confirmar';
import PanelFacturacion from './PanelFacturacion';
import { fechaCorta, situacion, textoDias, UNIDADES } from './formato';

/** Ficha de un negocio: su suscripción y su facturación electrónica. */
export default function PanelNegocio({ negocio, onVolver, onActualizado }) {
  const [pestana, setPestana] = useState('SUSCRIPCION');
  const s = situacion(negocio);

  // Vuelve a leer el negocio después de un cambio (estado, fecha, modo).
  const refrescar = async () => {
    try {
      const lista = await panelApi.negocios();
      const actual = lista.find((n) => n.id === negocio.id);
      if (actual) onActualizado(actual);
    } catch {
      // si falla, la ficha sigue mostrando lo último que se leyó
    }
  };

  return (
    <section>
      <button type="button" className="pnl-volver" onClick={onVolver}>
        <ArrowLeft size={16} /> Negocios
      </button>

      <div className="pnl-ficha-cabecera">
        <div>
          <h1>{negocio.nombre}</h1>
          <p>
            {negocio.identificador}
            {negocio.usuarios?.length > 0 && ` · usuarios: ${negocio.usuarios.join(', ')}`}
            {negocio.fecha_creacion && ` · cliente desde ${fechaCorta(negocio.fecha_creacion)}`}
          </p>
        </div>
        <span className={`pnl-chip pnl-chip-${s.tono} pnl-chip-grande`}>{s.texto}</span>
      </div>

      <div className="pnl-pestanas" role="tablist">
        <button type="button" role="tab" aria-selected={pestana === 'SUSCRIPCION'} className={pestana === 'SUSCRIPCION' ? 'activo' : ''} onClick={() => setPestana('SUSCRIPCION')}>
          <CalendarClock size={16} /> Suscripción
        </button>
        <button type="button" role="tab" aria-selected={pestana === 'FACTURACION'} className={pestana === 'FACTURACION' ? 'activo' : ''} onClick={() => setPestana('FACTURACION')}>
          <FileCheck2 size={16} /> Facturación electrónica
        </button>
      </div>

      {pestana === 'SUSCRIPCION' && <Suscripcion negocio={negocio} onCambio={refrescar} />}
      {pestana === 'FACTURACION' && <PanelFacturacion negocio={negocio} onCambio={refrescar} />}
    </section>
  );
}

const RAPIDOS = [
  [1, 'meses', '+1 mes'],
  [3, 'meses', '+3 meses'],
  [6, 'meses', '+6 meses'],
  [1, 'anios', '+1 año'],
];

function Suscripcion({ negocio, onCambio }) {
  const [cantidad, setCantidad] = useState('1');
  const [unidad, setUnidad] = useState('meses');
  const [fecha, setFecha] = useState(negocio.fecha_vencimiento || '');
  const [aviso, setAviso] = useState(null); // { tipo: 'ok'|'error', texto }
  const [ocupado, setOcupado] = useState(false);
  const s = situacion(negocio);

  const hacer = async (accion) => {
    setOcupado(true);
    setAviso(null);
    try {
      const r = await accion();
      setAviso({ tipo: 'ok', texto: r.mensaje });
      if (r.fecha_vencimiento !== undefined) setFecha(r.fecha_vencimiento || '');
      await onCambio();
    } catch (e) {
      setAviso({ tipo: 'error', texto: e.message });
    } finally {
      setOcupado(false);
    }
  };

  const renovar = async (n, u) => {
    const etiqueta = UNIDADES.find((x) => x.valor === u);
    const texto = `${n} ${n === 1 ? etiqueta.singular : etiqueta.plural}`;
    const vencido = negocio.dias_restantes === null || negocio.dias_restantes < 0;
    const ok = await confirmar({
      titulo: `¿Renovar ${texto}?`,
      mensaje: vencido
        ? 'Como ya venció (o no tenía fecha), se cuenta desde hoy.'
        : 'Se suma desde su fecha actual: no pierde los días que ya tenía pagados.',
      detalle: { etiqueta: negocio.nombre, valor: `+ ${texto}` },
      textoConfirmar: 'Renovar',
      tipo: 'normal',
      icono: 'tiempo',
    });
    if (ok) hacer(() => panelApi.renovar(negocio.id, n, u));
  };

  const cambiarEstado = async (estado) => {
    if (estado === negocio.estado) return;
    const textos = {
      ACTIVO: ['¿Reactivar el negocio?', 'Vuelve a usar el sistema normalmente (si su fecha no venció).', 'Reactivar', 'normal', 'reactivar'],
      RESTRINGIDO: ['¿Pasar a modo lectura?', 'Podrá entrar y ver su información, pero no vender ni cambiar nada.', 'Modo lectura', 'peligro', 'aviso'],
      SUSPENDIDO: ['¿Suspender el negocio?', 'Nadie del negocio podrá entrar al sistema hasta que lo reactives.', 'Suspender', 'peligro', 'usuario'],
    }[estado];
    const ok = await confirmar({
      titulo: textos[0],
      mensaje: textos[1],
      detalle: { etiqueta: 'Negocio', valor: negocio.nombre },
      textoConfirmar: textos[2],
      tipo: textos[3],
      icono: textos[4],
    });
    if (ok) hacer(() => panelApi.cambiarEstado(negocio.id, estado));
  };

  const guardarFecha = async (nueva) => {
    const ok = await confirmar({
      titulo: nueva ? '¿Cambiar la fecha de vencimiento?' : '¿Dejarlo sin vencimiento?',
      mensaje: nueva ? 'Se reemplaza la fecha actual por esta, sin sumar nada.' : 'No vencerá nunca hasta que le pongas una fecha.',
      detalle: { etiqueta: negocio.nombre, valor: nueva ? fechaCorta(nueva) : 'Sin vencimiento' },
      textoConfirmar: 'Guardar',
      tipo: 'normal',
      icono: 'tiempo',
    });
    if (ok) hacer(() => panelApi.fijarVencimiento(negocio.id, nueva || null));
  };

  const n = Number(cantidad);
  const cantidadValida = Number.isInteger(n) && n >= 1 && n <= 120;

  return (
    <div className="pnl-rejilla">
      <div className="pnl-caja">
        <h2>Estado actual</h2>
        <div className="pnl-estado-grande">
          <div>
            <span>Vence</span>
            <strong>{negocio.fecha_vencimiento ? fechaCorta(negocio.fecha_vencimiento) : 'Sin vencimiento'}</strong>
            {negocio.fecha_vencimiento && <small className={`pnl-texto-${s.tono}`}>{textoDias(negocio.dias_restantes)}</small>}
          </div>
          <div>
            <span>Puede</span>
            <strong>{negocio.acceso === 'COMPLETO' ? 'Usar todo' : negocio.acceso === 'LECTURA' ? 'Solo ver' : 'Nada (bloqueado)'}</strong>
            {negocio.motivo && <small>{negocio.motivo}</small>}
          </div>
        </div>

        <h3>Estado de la cuenta</h3>
        <div className="pnl-segmentos" role="radiogroup" aria-label="Estado de la cuenta">
          {[
            ['ACTIVO', 'Activo'],
            ['RESTRINGIDO', 'Modo lectura'],
            ['SUSPENDIDO', 'Suspendido'],
          ].map(([valor, texto]) => (
            <button
              key={valor}
              type="button"
              role="radio"
              aria-checked={negocio.estado === valor}
              className={`${negocio.estado === valor ? `activo pnl-seg-${valor.toLowerCase()}` : ''}`}
              disabled={ocupado}
              onClick={() => cambiarEstado(valor)}
            >
              {texto}
            </button>
          ))}
        </div>
        <p className="pnl-nota">
          "Modo lectura" es lo mismo que pasa solo cuando vence: entra pero no vende. "Suspendido" no deja ni entrar.
        </p>
      </div>

      <div className="pnl-caja">
        <h2>Renovar</h2>
        <p className="pnl-nota">Igual que canjear un código: suma tiempo y deja la cuenta activa.</p>
        <div className="pnl-rapidos">
          {RAPIDOS.map(([c, u, texto]) => (
            <button key={texto} type="button" className="pnl-boton" disabled={ocupado} onClick={() => renovar(c, u)}>
              {texto}
            </button>
          ))}
        </div>
        <div className="pnl-linea">
          <input
            className="pnl-input-corto"
            inputMode="numeric"
            value={cantidad}
            onChange={(e) => setCantidad(e.target.value.replace(/\D/g, ''))}
            aria-label="Cantidad"
          />
          <select value={unidad} onChange={(e) => setUnidad(e.target.value)} aria-label="Unidad">
            {UNIDADES.map((u) => (
              <option key={u.valor} value={u.valor}>
                {u.plural}
              </option>
            ))}
          </select>
          <button type="button" className="pnl-boton pnl-boton-principal" disabled={ocupado || !cantidadValida} onClick={() => renovar(n, unidad)}>
            Renovar
          </button>
        </div>

        <h3>Corregir la fecha</h3>
        <div className="pnl-linea">
          <input type="date" value={fecha} onChange={(e) => setFecha(e.target.value)} aria-label="Fecha de vencimiento" />
          <button type="button" className="pnl-boton" disabled={ocupado || !fecha || fecha === negocio.fecha_vencimiento} onClick={() => guardarFecha(fecha)}>
            Guardar fecha
          </button>
          <button type="button" className="pnl-boton pnl-boton-texto" disabled={ocupado || !negocio.fecha_vencimiento} onClick={() => guardarFecha(null)}>
            Sin vencimiento
          </button>
        </div>
      </div>

      {aviso && <p className={`pnl-aviso pnl-aviso-${aviso.tipo} pnl-ancho`}>{aviso.texto}</p>}
    </div>
  );
}
