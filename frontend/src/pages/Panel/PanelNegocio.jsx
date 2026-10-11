import { useState } from 'react';
import { ArrowLeft, CalendarClock, FileCheck2 } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { confirmar } from '../../utils/confirmar';
import PanelFacturacion from './PanelFacturacion';
import { fechaCorta, fechaCortaUtc, situacion, soles, textoDias, tonoUso, UNIDADES } from './formato';

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
            {negocio.slug && ` · ${window.location.host}/${negocio.slug}`}
            {negocio.usuarios?.length > 0 && ` · usuarios: ${negocio.usuarios.join(', ')}`}
            {negocio.fecha_creacion && ` · cliente desde ${fechaCortaUtc(negocio.fecha_creacion)}`}
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
      <UsoDelPlan uso={negocio.uso} />

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

      <DireccionDelNegocio negocio={negocio} onCambio={onCambio} />

      {aviso && <p className={`pnl-aviso pnl-aviso-${aviso.tipo} pnl-ancho`}>{aviso.texto}</p>}
    </div>
  );
}

/** Dirección propia del negocio en el sistema: <dominio>/<direccion>. */
function DireccionDelNegocio({ negocio, onCambio }) {
  const [direccion, setDireccion] = useState(negocio.slug || '');
  const [aviso, setAviso] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const limpia = direccion
    .trim()
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  const enlace = `${window.location.origin}/${negocio.slug || ''}`;

  const guardar = async () => {
    const ok = await confirmar({
      titulo: '¿Cambiar la dirección?',
      mensaje: 'El negocio entrará por la dirección nueva. La anterior deja de funcionar (los enlaces de boletas no cambian).',
      detalle: { etiqueta: negocio.nombre, valor: `${window.location.host}/${limpia}` },
      textoConfirmar: 'Cambiar',
      tipo: 'normal',
      icono: 'tiempo',
    });
    if (!ok) return;
    setOcupado(true);
    setAviso(null);
    try {
      const r = await panelApi.cambiarDireccion(negocio.id, limpia);
      setDireccion(r.slug);
      setAviso({ tipo: 'ok', texto: r.mensaje });
      await onCambio();
    } catch (e) {
      setAviso({ tipo: 'error', texto: e.message });
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pnl-caja pnl-ancho">
      <h2>Dirección del negocio</h2>
      <p className="pnl-nota">
        {negocio.slug ? (
          <>
            Entra por{' '}
            <a href={enlace} target="_blank" rel="noreferrer">
              {window.location.host}/{negocio.slug}
            </a>
            : el login sale con su logo y su color.
          </>
        ) : (
          'Todavía no tiene dirección propia (se le asigna al reiniciar el servidor).'
        )}
      </p>
      <div className="pnl-linea">
        <input
          value={direccion}
          maxLength={30}
          onChange={(e) => setDireccion(e.target.value)}
          placeholder="verane"
          aria-label="Dirección del negocio"
        />
        <button
          type="button"
          className="pnl-boton"
          disabled={ocupado || limpia.length < 3 || limpia === negocio.slug}
          onClick={guardar}
        >
          Guardar dirección
        </button>
      </div>
      {limpia && limpia !== direccion.trim() && <p className="pnl-nota">Quedará como: {window.location.host}/{limpia}</p>}
      {aviso && <p className={`pnl-aviso pnl-aviso-${aviso.tipo}`}>{aviso.texto}</p>}
    </div>
  );
}

/** Boletas y facturas del ciclo frente al plan, y lo que cobrar al renovar. */
function UsoDelPlan({ uso }) {
  if (!uso) {
    return (
      <div className="pnl-caja pnl-ancho">
        <h2>Comprobantes del ciclo</h2>
        <p className="pnl-nota">No se pudo leer la base de este negocio.</p>
      </div>
    );
  }
  const tono = tonoUso(uso);
  return (
    <div className="pnl-caja pnl-ancho pnl-uso pnl-uso-grande">
      <h2>
        Comprobantes del ciclo · {uso.plan} ({soles(uso.precio)})
      </h2>
      <span className="pnl-uso-cifras">
        <strong className={tono !== 'verde' ? `pnl-texto-${tono}` : ''}>{uso.usados}</strong> de {uso.limite} boletas y facturas ·
        del {fechaCorta(uso.ciclo_inicio)} al {fechaCorta(uso.ciclo_fin)}
      </span>
      <span className="pnl-uso-barra" aria-hidden="true">
        <span className={`pnl-uso-relleno pnl-uso-${tono}`} style={{ width: `${Math.min(100, uso.porcentaje)}%` }} />
      </span>
      {uso.extra > 0 && (
        <p className="pnl-uso-aviso">
          Pasó su plan: {uso.extra} comprobante{uso.extra === 1 ? '' : 's'} extra × {soles(uso.precio_extra)} = {soles(uso.monto_extra)}. En la
          siguiente renovación cobra {soles(uso.total_siguiente)}.
        </p>
      )}
      <dl className="pnl-uso-detalle">
        <div>
          <dt>Boletas</dt>
          <dd>{uso.boletas}</dd>
        </div>
        <div>
          <dt>Facturas</dt>
          <dd>{uso.facturas}</dd>
        </div>
        <div>
          <dt>Próxima mensualidad</dt>
          <dd className={uso.extra > 0 ? 'pnl-texto-rojo' : ''}>{soles(uso.total_siguiente)}</dd>
        </div>
      </dl>
      <p className="pnl-nota">
        Cuentan las boletas y facturas enviadas a SUNAT (también las anuladas). No cuentan notas de crédito, guías, rechazadas ni con error.
        Si pasa del límite no se le corta: sigue emitiendo.
      </p>
    </div>
  );
}
