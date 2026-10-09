import { useEffect, useState } from 'react';
import { ReceiptText, X } from 'lucide-react';
import { api } from '../api/api';
import { imprimirNotaCredito } from '../utils/notaCredito';
import './FormularioGasto.css';

const r2 = (n) => Math.round(n * 100) / 100;

/**
 * Emitir una nota de crédito para una boleta o factura emitida directo a
 * SUNAT (pantalla Comprobantes). comprobante = fila de la lista.
 */
export default function FormularioNotaCredito({ comprobante, onEmitida, onCerrar }) {
  const [prep, setPrep] = useState(null);
  const [motivo, setMotivo] = useState('07');
  const [descripcion, setDescripcion] = useState('');
  // indice -> cantidad (texto)
  const [cantidades, setCantidades] = useState({});
  const [error, setError] = useState('');
  const [enviando, setEnviando] = useState(false);
  const [hecha, setHecha] = useState(null);

  useEffect(() => {
    let vivo = true;
    api
      .notaCreditoPreparar(comprobante.id)
      .then((p) => vivo && setPrep(p))
      .catch((e) => vivo && setError(e.message));
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => {
      vivo = false;
      window.removeEventListener('keydown', alTeclear);
    };
  }, [comprobante.id, onCerrar]);

  const total = motivo !== '07';
  const lineas = (prep?.lineas || [])
    .map((l) => ({ ...l, elegida: Math.min(parseFloat(cantidades[l.indice]) || 0, l.disponible) }))
    .filter((l) => l.elegida > 0);
  const monto = !prep ? 0 : total ? r2(prep.total) : r2(lineas.reduce((s, l) => s + l.elegida * l.precio_unitario, 0));
  const valido = prep && (total ? prep.puede_total : lineas.length > 0);
  const nombreDoc = `${comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${comprobante.serie}-${comprobante.numero}`;

  const emitir = async (e) => {
    e.preventDefault();
    if (!valido || enviando) return;
    setEnviando(true);
    setError('');
    try {
      const nota = await api.notaCreditoEmitir(comprobante.id, {
        motivo_codigo: motivo,
        motivo: descripcion.trim() || null,
        lineas: total ? [] : lineas.map((l) => ({ indice: l.indice, cantidad: l.elegida })),
      });
      setHecha(nota);
      onEmitida?.(nota);
    } catch (err) {
      setError(err.message);
    } finally {
      setEnviando(false);
    }
  };

  const imprimir = (formato) => imprimirNotaCredito(hecha, formato).catch((e) => setError(e.message));

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="fg-modal" onSubmit={emitir} role="dialog" aria-modal="true" aria-labelledby="nc-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <ReceiptText size={18} />
          </span>
          <div>
            <h2 id="nc-titulo">Nota de crédito</h2>
            <p>
              Corrige la {nombreDoc}
              {prep ? ` (S/ ${prep.total.toFixed(2)})` : ''}.
            </p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>

        <div className="fg-cuerpo">
          {!prep && !error && <p className="fg-tenue">Cargando…</p>}

          {hecha ? (
            <>
              <p className={`fg-nota${hecha.estado === 'ACEPTADO' ? '' : ' fg-nota-error'}`} style={{ margin: 0 }}>
                <strong>
                  Nota {hecha.serie}-{hecha.numero} por S/ {hecha.total.toFixed(2)}:{' '}
                  {hecha.estado === 'ACEPTADO' ? 'aceptada por SUNAT' : hecha.estado.toLowerCase()}.
                </strong>{' '}
                {hecha.estado === 'ACEPTADO' ? '' : hecha.mensaje}
              </p>
              {hecha.estado === 'ACEPTADO' && (
                <div className="fg-chips">
                  <button type="button" onClick={() => imprimir('ticket')}>
                    🖨 Ticket 80 mm
                  </button>
                  <button type="button" onClick={() => imprimir('a4')}>
                    🖨 Hoja A4
                  </button>
                </div>
              )}
            </>
          ) : (
            prep && (
              <>
                {prep.acreditado > 0 && (
                  <p className="fg-tenue">Ya tiene notas de crédito por S/ {prep.acreditado.toFixed(2)}.</p>
                )}
                <div className="fg-campo">
                  <span>Motivo</span>
                  <div className="fg-chips" role="radiogroup" aria-label="Motivo">
                    {prep.motivos
                      .slice()
                      .reverse()
                      .map((m) => {
                        const bloqueado = m.codigo !== '07' && !prep.puede_total;
                        return (
                          <button
                            type="button"
                            key={m.codigo}
                            role="radio"
                            aria-checked={motivo === m.codigo}
                            className={motivo === m.codigo ? 'activo' : ''}
                            disabled={bloqueado}
                            title={bloqueado ? 'Ya tiene una nota: solo se puede acreditar por ítem lo que falta' : undefined}
                            onClick={() => setMotivo(m.codigo)}
                          >
                            {m.nombre}
                          </button>
                        );
                      })}
                  </div>
                </div>

                {motivo === '07' ? (
                  <div className="fg-campo">
                    <span>¿Qué se acredita?</span>
                    {prep.lineas.map((l) => (
                      <label key={l.indice} className="nc-linea">
                        <span className="nc-desc">
                          {l.descripcion}
                          <small>
                            S/ {l.precio_unitario.toFixed(2)} c/u · {l.disponible === l.cantidad ? `vendido ${l.cantidad}` : `quedan ${l.disponible} de ${l.cantidad}`}
                          </small>
                        </span>
                        <input
                          inputMode="decimal"
                          placeholder="0"
                          disabled={l.disponible <= 0}
                          value={cantidades[l.indice] ?? ''}
                          onChange={(e) => setCantidades((c) => ({ ...c, [l.indice]: e.target.value.replace(/[^\d.]/g, '') }))}
                          aria-label={`Cantidad de ${l.descripcion}`}
                        />
                      </label>
                    ))}
                  </div>
                ) : (
                  <p className="fg-tenue">
                    {motivo === '01'
                      ? 'Anula toda la operación: para SUNAT la venta queda sin efecto.'
                      : 'Acredita todo lo vendido en este comprobante.'}
                  </p>
                )}

                <label className="fg-campo">
                  <span>Detalle del motivo (opcional)</span>
                  <input
                    value={descripcion}
                    maxLength={250}
                    placeholder={motivo === '01' ? 'Ej. Error en los datos del cliente' : 'Ej. Cambio de talla'}
                    onChange={(e) => setDescripcion(e.target.value)}
                  />
                </label>

                <p className="fg-nota" style={{ margin: 0 }}>
                  Solo corrige el comprobante ante SUNAT: no devuelve stock ni mueve la caja. Si el cliente devuelve productos,
                  regístralo en Devoluciones (ahí la nota sale sola).
                </p>
              </>
            )
          )}

          {error && <p className="fg-error">{error}</p>}
        </div>

        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar}>
            {hecha ? 'Cerrar' : 'Cancelar'}
          </button>
          {!hecha && (
            <button type="submit" className="fg-boton fg-boton-principal" disabled={!valido || enviando}>
              {enviando ? 'Enviando a SUNAT…' : monto > 0 ? `Emitir nota por S/ ${monto.toFixed(2)}` : 'Emitir nota'}
            </button>
          )}
        </footer>
      </form>
    </div>
  );
}
