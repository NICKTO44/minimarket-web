import { useEffect, useState } from 'react';
import { Check, Copy, Plus } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { duracion, fechaCorta, UNIDADES } from './formato';

/**
 * Códigos de activación: el cliente los canjea en su pantalla Suscripción
 * (un solo uso, para un solo negocio). Mismos códigos que genera el
 * comando `licencias generar-codigo`.
 */
export default function PanelCodigos() {
  const [codigos, setCodigos] = useState(null);
  const [error, setError] = useState('');
  const [cantidad, setCantidad] = useState('1');
  const [unidad, setUnidad] = useState('meses');
  const [caduca, setCaduca] = useState('30');
  const [nuevo, setNuevo] = useState(null);
  const [copiado, setCopiado] = useState('');
  const [generando, setGenerando] = useState(false);

  const cargar = () =>
    panelApi
      .codigos()
      .then(setCodigos)
      .catch((e) => setError(e.message));

  useEffect(() => {
    cargar();
  }, []);

  const generar = async (e) => {
    e.preventDefault();
    setGenerando(true);
    setError('');
    try {
      const c = await panelApi.generarCodigo(Number(cantidad), unidad, Number(caduca) || 30);
      setNuevo(c);
      await cargar();
    } catch (err) {
      setError(err.message);
    } finally {
      setGenerando(false);
    }
  };

  const copiar = async (codigo) => {
    try {
      await navigator.clipboard.writeText(codigo);
      setCopiado(codigo);
      setTimeout(() => setCopiado(''), 1600);
    } catch {
      // sin portapapeles: el código queda a la vista para copiarlo a mano
    }
  };

  const n = Number(cantidad);
  const valido = Number.isInteger(n) && n >= 1 && n <= 120;

  return (
    <section>
      <div className="pnl-titulo">
        <div>
          <h1>Códigos de activación</h1>
          <p>Se los mandas al cliente por WhatsApp y él los canjea en Suscripción. Cada uno sirve una sola vez.</p>
        </div>
      </div>

      <form className="pnl-caja pnl-generar" onSubmit={generar}>
        <label className="pnl-campo">
          <span>Activa por</span>
          <div className="pnl-linea">
            <input className="pnl-input-corto" inputMode="numeric" value={cantidad} onChange={(e) => setCantidad(e.target.value.replace(/\D/g, ''))} />
            <select value={unidad} onChange={(e) => setUnidad(e.target.value)}>
              {UNIDADES.map((u) => (
                <option key={u.valor} value={u.valor}>
                  {u.plural}
                </option>
              ))}
            </select>
          </div>
        </label>
        <label className="pnl-campo">
          <span>Caduca si no lo usan en</span>
          <div className="pnl-linea">
            <input className="pnl-input-corto" inputMode="numeric" value={caduca} onChange={(e) => setCaduca(e.target.value.replace(/\D/g, ''))} />
            <span className="pnl-tenue">días</span>
          </div>
        </label>
        <button type="submit" className="pnl-boton pnl-boton-principal" disabled={!valido || generando}>
          <Plus size={15} /> {generando ? 'Generando...' : 'Generar código'}
        </button>
      </form>

      {nuevo && (
        <div className="pnl-codigo-nuevo">
          <span>Código nuevo · {duracion(nuevo.cantidad, nuevo.unidad)}</span>
          <strong>{nuevo.codigo}</strong>
          <button type="button" className="pnl-boton" onClick={() => copiar(nuevo.codigo)}>
            {copiado === nuevo.codigo ? <Check size={15} /> : <Copy size={15} />} {copiado === nuevo.codigo ? 'Copiado' : 'Copiar'}
          </button>
        </div>
      )}

      {error && <p className="pnl-error">{error}</p>}
      {!codigos && !error && <p className="pnl-tenue">Cargando códigos...</p>}

      {codigos && (
        <div className="pnl-tabla pnl-tabla-codigos" role="table" aria-label="Códigos">
          <div className="pnl-fila pnl-fila-cabecera" role="row">
            <span role="columnheader">Código</span>
            <span role="columnheader">Activa por</span>
            <span role="columnheader">Estado</span>
            <span role="columnheader">Creado</span>
          </div>
          {codigos.map((c) => (
            <div key={c.codigo} className="pnl-fila" role="row">
              <span role="cell" className="pnl-codigo">
                <code>{c.codigo}</code>
                {c.vigente && (
                  <button type="button" onClick={() => copiar(c.codigo)} aria-label={`Copiar ${c.codigo}`}>
                    {copiado === c.codigo ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                )}
              </span>
              <span role="cell">{duracion(c.cantidad, c.unidad)}</span>
              <span role="cell">
                {c.usado ? (
                  <span className="pnl-chip pnl-chip-gris">Usado por {c.negocio || 'un negocio'}{c.fecha_uso ? ` · ${fechaCorta(c.fecha_uso)}` : ''}</span>
                ) : c.vigente ? (
                  <span className="pnl-chip pnl-chip-verde">Disponible hasta {fechaCorta(c.caduca)}</span>
                ) : (
                  <span className="pnl-chip pnl-chip-rojo">Caducó sin usar</span>
                )}
              </span>
              <span role="cell" className="pnl-tenue">{fechaCorta(c.fecha_creacion)}</span>
            </div>
          ))}
          {codigos.length === 0 && <p className="pnl-tenue pnl-vacio">Todavía no hay códigos.</p>}
        </div>
      )}
    </section>
  );
}
