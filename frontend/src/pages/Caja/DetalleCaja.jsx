import { useEffect, useState } from 'react';
import { Printer, Wallet, X } from 'lucide-react';
import { api } from '../../api/api';
import { ventasDeCajaPorMetodo } from '../../utils/metodoPago';
import '../../components/FormularioGasto.css';
import './Caja.css';

const NOMBRE_MOVIMIENTO = { GASTO: 'Gasto', RETIRO: 'Retiro', INGRESO: 'Ingreso' };

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const fechaHora = (texto) => (texto ? new Date(texto).toLocaleString('es-PE') : '—');

function estadoDiferencia(dif) {
  if (Math.abs(dif) < 0.01) return 'sin-diferencia';
  if (Math.abs(dif) <= 10) return 'aceptable';
  return 'significativa';
}

/** Cómo quedó una caja del historial: lo mismo que se ve con la caja abierta, más su cierre. */
export default function DetalleCaja({ cajaId, onCerrar }) {
  const [caja, setCaja] = useState(null);
  const [error, setError] = useState('');

  useEffect(() => {
    let vivo = true;
    api
      .cajaDetalle(cajaId)
      .then((c) => vivo && setCaja(c))
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, [cajaId]);

  useEffect(() => {
    const tecla = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', tecla);
    return () => window.removeEventListener('keydown', tecla);
  }, [onCerrar]);

  // Si la caja se cerró, el efectivo esperado es el que quedó guardado en el
  // cierre; si sigue abierta, se calcula igual que en la pantalla Caja.
  const esperado = caja
    ? caja.efectivo_esperado ??
      caja.monto_inicial + caja.ventas_efectivo + caja.ingresos_total - caja.retiros_total - caja.gastos_total
    : 0;
  const cerrada = caja?.estado === 'CERRADA';

  return (
    <div className="fg-velo dcaja-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="fg-modal dcaja-modal" role="dialog" aria-modal="true" aria-labelledby="dcaja-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <Wallet size={18} />
          </span>
          <div>
            <h2 id="dcaja-titulo">Caja N.° {cajaId}</h2>
            <p>{caja ? `${caja.usuario_nombre} · ${cerrada ? 'Cerrada' : 'Abierta'}` : 'Cargando…'}</p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>

        <div className="fg-cuerpo dcaja-cuerpo">
          {error && <p className="fg-error">{error}</p>}
          {!caja && !error && <p className="caja-cargando">Cargando…</p>}

          {caja && (
            <>
              <div className="caja-resumen-fila">
                <span>Apertura</span>
                <strong>{fechaHora(caja.fecha_apertura)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>Cierre</span>
                <strong>{cerrada ? fechaHora(caja.fecha_cierre) : 'Sigue abierta'}</strong>
              </div>
              {caja.observaciones_apertura && <p className="dcaja-nota">Al abrir: {caja.observaciones_apertura}</p>}

              <div className="caja-separador"></div>

              <div className="caja-seccion-titulo">Ventas por método de pago</div>
              {ventasDeCajaPorMetodo(caja).map((m) => (
                <div key={m.clave} className="caja-resumen-fila">
                  <span>{m.label}</span>
                  <strong>{soles(m.valor)}</strong>
                </div>
              ))}
              <div className="caja-resumen-fila caja-resumen-total">
                <span>Total vendido bruto ({caja.numero_transacciones} ventas)</span>
                <strong>{soles(caja.total_ventas)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>Venta neta (descontando devoluciones)</span>
                <strong>{soles(caja.total_ventas - caja.devoluciones_monto)}</strong>
              </div>
              {caja.devoluciones_monto > 0 && (
                <div className="caja-resumen-fila caja-fila-negativa">
                  <span>Devoluciones</span>
                  <strong>- {soles(caja.devoluciones_monto)}</strong>
                </div>
              )}

              <div className="caja-separador"></div>

              <div className="caja-seccion-titulo">Movimientos de efectivo</div>
              <div className="caja-resumen-fila">
                <span>Monto inicial</span>
                <strong>{soles(caja.monto_inicial)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>{caja.devoluciones_monto > 0 ? 'Ventas en efectivo (ya sin devoluciones)' : 'Ventas en efectivo'}</span>
                <strong>+ {soles(caja.ventas_efectivo)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>Ingresos manuales</span>
                <strong>+ {soles(caja.ingresos_total)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>Retiros</span>
                <strong>- {soles(caja.retiros_total)}</strong>
              </div>
              <div className="caja-resumen-fila">
                <span>Gastos</span>
                <strong>- {soles(caja.gastos_total)}</strong>
              </div>
              <div className="caja-resumen-fila caja-resumen-total">
                <span>Efectivo esperado</span>
                <strong>{soles(esperado)}</strong>
              </div>

              {caja.movimientos.length > 0 ? (
                <ul className="caja-mov-lista" aria-label="Movimientos de la caja">
                  {caja.movimientos.map((m) => (
                    <li key={m.id}>
                      <span className={`caja-mov-tipo caja-mov-${m.tipo.toLowerCase()}`}>{NOMBRE_MOVIMIENTO[m.tipo] || m.tipo}</span>
                      <span className="caja-mov-motivo">
                        {m.motivo}
                        <small>
                          {m.hora}
                          {m.usuario ? ` · ${m.usuario}` : ''}
                          {m.gasto_id ? ` · Gasto N.° ${m.gasto_id}` : ''}
                        </small>
                      </span>
                      <strong>
                        {m.tipo === 'INGRESO' ? '+' : '−'} {soles(m.monto)}
                      </strong>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="dcaja-nota">No hubo gastos, retiros ni ingresos en esta caja.</p>
              )}

              {cerrada && (
                <>
                  <div className="caja-separador"></div>
                  <div className="caja-seccion-titulo">Cuadre al cerrar</div>
                  <div className="caja-resumen-fila">
                    <span>Efectivo esperado</span>
                    <strong>{soles(esperado)}</strong>
                  </div>
                  <div className="caja-resumen-fila">
                    <span>Efectivo contado</span>
                    <strong>{caja.monto_contado != null ? soles(caja.monto_contado) : '—'}</strong>
                  </div>
                  {caja.diferencia != null && (
                    <div className={`caja-diferencia caja-diferencia-${estadoDiferencia(caja.diferencia)}`}>
                      <span>Diferencia</span>
                      <strong>
                        {caja.diferencia >= 0 ? '+' : ''}
                        {soles(caja.diferencia)}
                      </strong>
                    </div>
                  )}
                  {caja.justificacion_diferencia && <p className="dcaja-nota">Justificación: {caja.justificacion_diferencia}</p>}
                  {caja.observaciones_cierre && <p className="dcaja-nota">Al cerrar: {caja.observaciones_cierre}</p>}
                </>
              )}
            </>
          )}
        </div>

        <footer className="fg-acciones dcaja-acciones">
          <button type="button" className="fg-boton" onClick={() => window.print()} disabled={!caja}>
            <Printer size={16} /> Imprimir
          </button>
          <button type="button" className="fg-boton fg-boton-principal" onClick={onCerrar}>
            Cerrar
          </button>
        </footer>
      </div>
    </div>
  );
}
