import { useEffect, useState } from 'react';
import { Ban, X } from 'lucide-react';
import { api } from '../api/api';
import './FormularioGasto.css';

/**
 * Anular una boleta o factura emitida directo a SUNAT (cajero y
 * administrador). Devuelve la venta (stock y dinero) y pide la baja a SUNAT.
 */
export default function FormularioAnulacion({ comprobante, onHecho, onCerrar }) {
  const [info, setInfo] = useState(null);
  const [motivo, setMotivo] = useState('');
  const [error, setError] = useState('');
  const [enviando, setEnviando] = useState(false);
  const [hecho, setHecho] = useState(null);

  useEffect(() => {
    let vivo = true;
    api
      .comprobanteAnulacion(comprobante.id)
      .then((i) => vivo && setInfo(i))
      .catch((e) => vivo && setError(e.message));
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => {
      vivo = false;
      window.removeEventListener('keydown', alTeclear);
    };
  }, [comprobante.id, onCerrar]);

  const nombreDoc = `${comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${comprobante.serie}-${comprobante.numero}`;
  const valido = info?.puede && motivo.trim().length >= 3;

  const anular = async (e) => {
    e.preventDefault();
    if (!valido || enviando) return;
    setEnviando(true);
    setError('');
    try {
      const r = await api.comprobanteAnular(comprobante.id, { motivo: motivo.trim() });
      setHecho(r);
      onHecho?.(r);
    } catch (err) {
      setError(err.message);
    } finally {
      setEnviando(false);
    }
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="fg-modal" onSubmit={anular} role="dialog" aria-modal="true" aria-labelledby="an-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono fg-icono-peligro">
            <Ban size={18} />
          </span>
          <div>
            <h2 id="an-titulo">Anular {nombreDoc}</h2>
            <p>La venta queda sin efecto y se pide la anulación a SUNAT.</p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>

        <div className="fg-cuerpo">
          {!info && !error && <p className="fg-tenue">Revisando…</p>}

          {hecho ? (
            <p className={`fg-nota${hecho.anulacion === 'RECHAZADA' ? ' fg-nota-error' : ''}`} style={{ margin: 0 }}>
              <strong>
                {hecho.anulacion === 'ANULADO'
                  ? `${nombreDoc} quedó anulado.`
                  : hecho.anulacion === 'EN_PROCESO'
                    ? 'Anulación enviada; SUNAT la está procesando.'
                    : hecho.anulacion === 'VERIFICAR'
                      ? 'Hay que revisar la anulación en SUNAT.'
                      : 'SUNAT no aceptó la anulación.'}
              </strong>{' '}
              {hecho.mensaje}
              {hecho.folio_devolucion && ` Devolución ${hecho.folio_devolucion}: el stock volvió y el dinero salió de la caja.`}

            </p>
          ) : (
            info &&
            (info.puede ? (
              <>
                <p className="fg-tenue" style={{ margin: 0 }}>
                  {info.tipo === 'BAJA'
                    ? 'Se enviará la comunicación de baja de la factura.'
                    : 'Se enviará el resumen diario con la boleta anulada.'}{' '}
                  {info.dias_restantes === 0
                    ? 'Hoy es el último día para anularla.'
                    : `Quedan ${info.dias_restantes} ${info.dias_restantes === 1 ? 'día' : 'días'} para anularla.`}
                </p>
                {info.por_devolver.length > 0 && (
                  <div className="fg-campo">
                    <span>Se devuelve</span>
                    {info.por_devolver.map((p) => (
                      <div key={p.detalle_id} className="nc-linea">
                        <span className="nc-desc">
                          {p.nombre}
                          <small>Cantidad {p.cantidad}</small>
                        </span>
                        <strong>S/ {p.monto.toFixed(2)}</strong>
                      </div>
                    ))}
                    <p className="fg-nota" style={{ margin: '4px 0 0' }}>
                      Vuelve al stock y se devuelven S/ {info.monto_devolver.toFixed(2)} al cliente por el mismo medio de pago
                      {info.metodo_pago === 'MIXTO' ? ' (en una venta mixta, en efectivo)' : ''}.
                    </p>
                  </div>
                )}
                <label className="fg-campo">
                  <span>Motivo de la anulación</span>
                  <input
                    autoFocus
                    value={motivo}
                    maxLength={100}
                    placeholder="Ej. Error en el RUC del cliente"
                    onChange={(e) => setMotivo(e.target.value)}
                  />
                </label>
              </>
            ) : (
              <p className="fg-nota fg-nota-error" style={{ margin: 0 }}>
                {info.motivo_no}
              </p>
            ))
          )}

          {error && <p className="fg-error">{error}</p>}
        </div>

        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar}>
            {hecho || (info && !info.puede) ? 'Cerrar' : 'Cancelar'}
          </button>
          {!hecho && info?.puede && (
            <button type="submit" className="fg-boton fg-boton-peligro" disabled={!valido || enviando}>
              {enviando ? 'Anulando…' : 'Anular comprobante'}
            </button>
          )}
        </footer>
      </form>
    </div>
  );
}
