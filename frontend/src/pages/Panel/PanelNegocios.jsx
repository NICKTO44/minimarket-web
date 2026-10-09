import { useEffect, useMemo, useState } from 'react';
import { ChevronRight, RefreshCw, Search } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { fechaCorta, modoFacturacion, situacion, textoDias } from './formato';

/** Todos los negocios, con su suscripción y su modo de facturación. */
export default function PanelNegocios({ onAbrir }) {
  const [negocios, setNegocios] = useState(null);
  const [error, setError] = useState('');
  const [buscar, setBuscar] = useState('');
  const [filtro, setFiltro] = useState('TODOS');
  const [cargando, setCargando] = useState(false);

  const cargar = async () => {
    setCargando(true);
    setError('');
    try {
      setNegocios(await panelApi.negocios());
    } catch (e) {
      setError(e.message);
    } finally {
      setCargando(false);
    }
  };

  useEffect(() => {
    let vivo = true;
    panelApi
      .negocios()
      .then((lista) => vivo && setNegocios(lista))
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, []);

  const conteo = useMemo(() => {
    const lista = negocios || [];
    const tono = (n) => situacion(n).tono;
    return {
      TODOS: lista.length,
      AL_DIA: lista.filter((n) => tono(n) === 'verde').length,
      ATENCION: lista.filter((n) => tono(n) !== 'verde').length,
      DIRECTO: lista.filter((n) => n.facturacion?.modo === 'DIRECTO').length,
    };
  }, [negocios]);

  const visibles = useMemo(() => {
    const texto = buscar.trim().toLowerCase();
    return (negocios || [])
      .filter((n) => {
        if (filtro === 'AL_DIA') return situacion(n).tono === 'verde';
        if (filtro === 'ATENCION') return situacion(n).tono !== 'verde';
        if (filtro === 'DIRECTO') return n.facturacion?.modo === 'DIRECTO';
        return true;
      })
      .filter(
        (n) =>
          !texto ||
          [n.nombre, n.identificador, n.facturacion?.ruc, ...(n.usuarios || [])].some((v) => (v || '').toLowerCase().includes(texto))
      );
  }, [negocios, buscar, filtro]);

  const FILTROS = [
    ['TODOS', 'Todos'],
    ['AL_DIA', 'Al día'],
    ['ATENCION', 'Requieren atención'],
    ['DIRECTO', 'Emisión directa'],
  ];

  return (
    <section>
      <div className="pnl-titulo">
        <div>
          <h1>Negocios</h1>
          <p>Suscripción y facturación de cada cliente de Monspeet.</p>
        </div>
        <button type="button" className="pnl-boton" onClick={cargar} disabled={cargando}>
          <RefreshCw size={15} className={cargando ? 'pnl-girando' : ''} /> Actualizar
        </button>
      </div>

      <div className="pnl-resumen">
        {FILTROS.map(([clave, texto]) => (
          <button
            key={clave}
            type="button"
            className={`pnl-tarjeta${filtro === clave ? ' activo' : ''}${clave === 'ATENCION' && conteo.ATENCION > 0 ? ' alerta' : ''}`}
            onClick={() => setFiltro(clave)}
          >
            <strong>{negocios ? conteo[clave] : '—'}</strong>
            <span>{texto}</span>
          </button>
        ))}
      </div>

      <label className="pnl-buscar">
        <Search size={16} />
        <input value={buscar} onChange={(e) => setBuscar(e.target.value)} placeholder="Buscar por nombre, identificador, RUC o usuario" />
      </label>

      {error && <p className="pnl-error">{error}</p>}
      {!negocios && !error && <p className="pnl-tenue">Cargando negocios...</p>}

      {negocios && (
        <div className="pnl-tabla" role="table" aria-label="Negocios">
          <div className="pnl-fila pnl-fila-cabecera" role="row">
            <span role="columnheader">Negocio</span>
            <span role="columnheader">Suscripción</span>
            <span role="columnheader">Vence</span>
            <span role="columnheader">Facturación</span>
            <span aria-hidden="true" />
          </div>
          {visibles.map((n) => {
            const s = situacion(n);
            const f = modoFacturacion(n.facturacion);
            return (
              <button key={n.id} type="button" className="pnl-fila" role="row" onClick={() => onAbrir(n)}>
                <span className="pnl-negocio" role="cell">
                  <strong>{n.nombre}</strong>
                  <small>
                    {n.identificador}
                    {n.facturacion?.ruc ? ` · RUC ${n.facturacion.ruc}` : ''}
                  </small>
                </span>
                <span role="cell">
                  <span className={`pnl-chip pnl-chip-${s.tono}`}>{s.texto}</span>
                </span>
                <span className="pnl-vence" role="cell">
                  <strong>{n.fecha_vencimiento ? fechaCorta(n.fecha_vencimiento) : '—'}</strong>
                  <small className={s.tono !== 'verde' ? `pnl-texto-${s.tono}` : ''}>{textoDias(n.dias_restantes)}</small>
                </span>
                <span role="cell">
                  <span className={`pnl-chip pnl-chip-${f.tono}`}>{f.texto}</span>
                </span>
                <ChevronRight size={18} className="pnl-flecha" aria-hidden="true" />
              </button>
            );
          })}
          {visibles.length === 0 && <p className="pnl-tenue pnl-vacio">Ningún negocio coincide.</p>}
        </div>
      )}
    </section>
  );
}
