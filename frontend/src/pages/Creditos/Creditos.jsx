import { useCallback, useEffect, useState } from 'react';
import { HandCoins, Printer } from 'lucide-react';
import { api } from '../../api/api';
import { METODOS_DE_ABONO, nombreMetodo } from '../../utils/metodoPago';
import { formatoCantidad } from '../../utils/medidas';
import { fechaCorta } from '../../utils/formato';
import '../../components/PantallaModulo.css';
import '../../components/Recibo.css';

const FILTROS = [
  { valor: 'PENDIENTE', label: 'Por cobrar' },
  { valor: 'PAGADO', label: 'Pagados' },
  { valor: 'TODOS', label: 'Todos' },
];
const METODOS = METODOS_DE_ABONO;

function EstadoCredito({ credito }) {
  if (credito.estado === 'PAGADO') return <span className="pm-chip pm-chip-ok">Pagado</span>;
  if (credito.vencido) return <span className="pm-chip pm-chip-mal">Vencido</span>;
  return <span className="pm-chip pm-chip-aviso">Pendiente</span>;
}

/** Estado de cuenta de un crédito para la ticketera (solo al imprimir). */
function CreditoImprimible({ credito, nombreTienda, direccion, telefono, ruc }) {
  if (!credito) return null;
  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      {ruc && <div className="recibo-centro recibo-dato-tienda recibo-ruc">RUC {ruc}</div>}
      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">ESTADO DE CUENTA</div>
      <div className="recibo-centro recibo-comprobante-numero">Venta {credito.folio}</div>
      <div className="recibo-linea"></div>
      <div className="recibo-seccion-titulo">CLIENTE</div>
      {credito.cliente_documento && (
        <div className="recibo-fila-meta">
          <span>Doc.</span>
          <span>{credito.cliente_documento}</span>
        </div>
      )}
      <div className="recibo-cliente-nombre">{credito.cliente_nombre}</div>
      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Fecha de venta</span>
        <span>{fechaCorta(credito.fecha)}</span>
      </div>
      {credito.vence && (
        <div className="recibo-fila-meta">
          <span>Vence</span>
          <span>{fechaCorta(credito.vence)}</span>
        </div>
      )}
      <div className="recibo-linea"></div>
      {credito.productos.map((p, i) => (
        <div key={i} className="recibo-item">
          <div className="recibo-item-nombre">{p.nombre}</div>
          <div className="recibo-item-detalle">
            <span>
              {formatoCantidad(p.cantidad)} x S/.{p.precio_unitario.toFixed(2)}
            </span>
            <span>S/.{p.total_linea.toFixed(2)}</span>
          </div>
        </div>
      ))}
      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Total de la venta</span>
        <span>S/.{credito.total.toFixed(2)}</span>
      </div>
      {credito.abonos.map((a) => (
        <div key={a.id} className="recibo-fila-meta">
          <span>
            {fechaCorta(a.fecha)} {nombreMetodo(a.metodo_pago)}
          </span>
          <span>-S/.{a.monto.toFixed(2)}</span>
        </div>
      ))}
      {credito.devuelto > 0 && (
        <div className="recibo-fila-meta">
          <span>Devoluciones</span>
          <span>-S/.{credito.devuelto.toFixed(2)}</span>
        </div>
      )}
      <div className="recibo-linea-doble"></div>
      <div className="recibo-total">
        <span>SALDO</span>
        <span>S/.{credito.saldo.toFixed(2)}</span>
      </div>
      <div className="recibo-linea-doble"></div>
      <div className="recibo-centro recibo-disclaimer">Estado de cuenta informativo. No es un comprobante de pago.</div>
    </div>
  );
}

/**
 * Créditos (módulo CREDITO): cuentas por cobrar. Las ventas al crédito se
 * hacen en el punto de venta (método "Crédito"); aquí se ve cuánto debe
 * cada cliente y se registran los abonos. Un abono en efectivo entra a la
 * caja abierta como ingreso.
 */
export default function Creditos({ nombreTienda, direccion, telefono, ruc }) {
  const [filtro, setFiltro] = useState('PENDIENTE');
  const [datos, setDatos] = useState(null);
  const [error, setError] = useState(null);
  const [abierto, setAbierto] = useState(null);
  const [monto, setMonto] = useState('');
  const [metodo, setMetodo] = useState('EFECTIVO');
  const [nota, setNota] = useState('');
  const [errorAbono, setErrorAbono] = useState(null);
  const [ocupado, setOcupado] = useState(false);

  const cargar = useCallback(
    () =>
      api
        .creditos(filtro)
        .then((d) => {
          setDatos(d);
          setError(null);
        })
        .catch((e) => setError(e.message)),
    [filtro]
  );

  useEffect(() => {
    cargar();
  }, [cargar]);

  const abrir = async (id) => {
    setError(null);
    try {
      const credito = await api.credito(id);
      setAbierto(credito);
      setMonto('');
      setMetodo('EFECTIVO');
      setNota('');
      setErrorAbono(null);
    } catch (e) {
      setError(e.message);
    }
  };

  const montoNum = Math.round((parseFloat(String(monto).replace(',', '.')) || 0) * 100) / 100;
  const puedeAbonar = abierto && abierto.saldo > 0 && montoNum > 0 && montoNum <= abierto.saldo + 0.005 && !ocupado;

  const abonar = async (e) => {
    e.preventDefault();
    if (!puedeAbonar) return;
    setOcupado(true);
    setErrorAbono(null);
    try {
      const actualizado = await api.creditoAbonar(abierto.id, { monto: montoNum, metodo_pago: metodo, nota: nota.trim() || null });
      setAbierto(actualizado);
      setMonto('');
      setNota('');
      cargar();
    } catch (err) {
      setErrorAbono(err.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <HandCoins size={22} /> Créditos
          </h1>
          <p className="pm-subtitulo">Lo que te deben tus clientes. Para vender al crédito, elige “Crédito” al cobrar en el punto de venta.</p>
        </div>
        <div className="pm-filtros" role="tablist">
          {FILTROS.map((f) => (
            <button key={f.valor} role="tab" aria-selected={filtro === f.valor} className={filtro === f.valor ? 'activo' : ''} onClick={() => setFiltro(f.valor)}>
              {f.label}
            </button>
          ))}
        </div>
      </header>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      {datos && (
        <div className="pm-resumen">
          <div className="pm-dato">
            <span>Por cobrar</span>
            <strong>S/ {datos.por_cobrar.toFixed(2)}</strong>
          </div>
          <div className={`pm-dato${datos.vencido > 0 ? ' pm-dato-alerta' : ''}`}>
            <span>Vencido</span>
            <strong>S/ {datos.vencido.toFixed(2)}</strong>
          </div>
          <div className="pm-dato">
            <span>Clientes con deuda</span>
            <strong>{datos.clientes_con_deuda}</strong>
          </div>
        </div>
      )}

      <div className="pm-tarjeta">
        {!datos ? (
          <p className="pm-vacio">{error ? 'No se pudo cargar.' : 'Cargando...'}</p>
        ) : datos.creditos.length === 0 ? (
          <p className="pm-vacio">{filtro === 'PENDIENTE' ? 'Nadie te debe: no hay créditos por cobrar.' : 'No hay créditos en esta lista.'}</p>
        ) : (
          datos.creditos.map((c) => (
            <button key={c.id} className="pm-fila" onClick={() => abrir(c.id)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">
                  {c.cliente_nombre} <EstadoCredito credito={c} />
                </span>
                <span className="pm-fila-detalle">
                  {c.folio} · {fechaCorta(c.fecha)}
                  {c.vence ? ` · vence ${fechaCorta(c.vence)}` : ''} · total S/ {c.total.toFixed(2)}
                </span>
              </span>
              <span className="pm-fila-monto">
                <strong>S/ {c.saldo.toFixed(2)}</strong>
                <span>{c.estado === 'PAGADO' ? 'pagado' : 'saldo'}</span>
              </span>
            </button>
          ))
        )}
      </div>

      {abierto && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setAbierto(null)}>
          <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Crédito">
            <div className="pm-modal-cabecera">
              <h2>
                {abierto.cliente_nombre} <EstadoCredito credito={abierto} />
              </h2>
              <p>
                Venta {abierto.folio} · {fechaCorta(abierto.fecha)}
                {abierto.vence ? ` · vence ${fechaCorta(abierto.vence)}` : ''}
                {abierto.cliente_telefono ? ` · Tel. ${abierto.cliente_telefono}` : ''}
              </p>
            </div>
            <div className="pm-modal-cuerpo">
              <div className="pm-resumen">
                <div className="pm-dato">
                  <span>Total</span>
                  <strong>S/ {abierto.total.toFixed(2)}</strong>
                </div>
                <div className="pm-dato">
                  <span>Pagado</span>
                  <strong>S/ {(abierto.abonado + abierto.devuelto).toFixed(2)}</strong>
                </div>
                <div className={`pm-dato${abierto.vencido ? ' pm-dato-alerta' : ''}`}>
                  <span>Saldo</span>
                  <strong>S/ {abierto.saldo.toFixed(2)}</strong>
                </div>
              </div>

              {abierto.saldo > 0 && (
                <form onSubmit={abonar}>
                  <h3 className="pm-seccion">Registrar abono</h3>
                  <div className="pm-campos">
                    <label className="pm-campo">
                      <span>Monto (S/)</span>
                      <input inputMode="decimal" placeholder="0.00" value={monto} onChange={(e) => setMonto(e.target.value)} />
                      <small>
                        <button type="button" className="cred-todo" onClick={() => setMonto(abierto.saldo.toFixed(2))}>
                          Paga todo: S/ {abierto.saldo.toFixed(2)}
                        </button>
                      </small>
                    </label>
                    <label className="pm-campo">
                      <span>Con qué paga</span>
                      <select value={metodo} onChange={(e) => setMetodo(e.target.value)}>
                        {METODOS.map((m) => (
                          <option key={m} value={m}>
                            {nombreMetodo(m)}
                          </option>
                        ))}
                      </select>
                      <small>{metodo === 'EFECTIVO' ? 'Entra a la caja abierta como ingreso.' : 'No entra al efectivo de caja.'}</small>
                    </label>
                    <label className="pm-campo pm-campo-ancho">
                      <span>Nota (opcional)</span>
                      <input value={nota} maxLength={200} onChange={(e) => setNota(e.target.value)} placeholder="N° de operación, quién pagó..." />
                    </label>
                  </div>
                  {montoNum > abierto.saldo + 0.005 && <p className="pm-mensaje pm-mensaje-error" style={{ marginTop: 10 }}>El abono no puede ser mayor que el saldo.</p>}
                  {errorAbono && <p className="pm-mensaje pm-mensaje-error" style={{ marginTop: 10 }}>{errorAbono}</p>}
                  <button type="submit" className="pm-boton" style={{ marginTop: 10, width: '100%' }} disabled={!puedeAbonar}>
                    {ocupado ? 'Guardando...' : `Registrar abono${montoNum > 0 ? ` de S/ ${montoNum.toFixed(2)}` : ''}`}
                  </button>
                </form>
              )}

              <h3 className="pm-seccion">Pagos recibidos</h3>
              {abierto.abonos.length === 0 && abierto.devuelto === 0 ? (
                <p className="pm-subtitulo">Aún no hay pagos.</p>
              ) : (
                <table className="pm-lineas">
                  <tbody>
                    {abierto.abonos.map((a) => (
                      <tr key={a.id}>
                        <td>
                          {fechaCorta(a.fecha)} · {nombreMetodo(a.metodo_pago)}
                          <small>
                            {a.nota ? `${a.nota} · ` : ''}
                            {a.usuario}
                          </small>
                        </td>
                        <td className="pm-num">S/ {a.monto.toFixed(2)}</td>
                      </tr>
                    ))}
                    {abierto.devuelto > 0 && (
                      <tr>
                        <td>Devoluciones descontadas de la deuda</td>
                        <td className="pm-num">S/ {abierto.devuelto.toFixed(2)}</td>
                      </tr>
                    )}
                  </tbody>
                </table>
              )}

              <h3 className="pm-seccion">Lo que se llevó</h3>
              <table className="pm-lineas">
                <tbody>
                  {abierto.productos.map((p, i) => (
                    <tr key={i}>
                      <td>{p.nombre}</td>
                      <td className="pm-num">
                        {formatoCantidad(p.cantidad)} × S/ {p.precio_unitario.toFixed(2)}
                      </td>
                      <td className="pm-num">S/ {p.total_linea.toFixed(2)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="pm-modal-pie">
              <button className="pm-boton-secundario" onClick={() => window.print()}>
                <Printer size={15} /> Imprimir estado de cuenta
              </button>
              <button className="pm-boton-secundario" onClick={() => setAbierto(null)}>
                Cerrar
              </button>
            </div>
          </div>
        </div>
      )}

      <CreditoImprimible credito={abierto} nombreTienda={nombreTienda} direccion={direccion} telefono={telefono} ruc={ruc} />
    </div>
  );
}
