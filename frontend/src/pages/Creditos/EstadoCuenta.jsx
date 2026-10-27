import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { MessageCircle, Printer } from 'lucide-react';
import { api } from '../../api/api';
import { METODOS_DE_ABONO, nombreMetodo } from '../../utils/metodoPago';
import { formatoCantidad } from '../../utils/medidas';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { enlaceWhatsapp } from '../../utils/vehiculos';
import { imprimirTicket } from '../../utils/imprimirTicket';

const MESES = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'];

/** Primer y último día del mes de `fecha` (AAAA-MM-DD), corrido `mover` meses. */
function rangoMes(fecha, mover = 0) {
  const [a, m] = fecha.split('-').map(Number);
  const inicio = new Date(Date.UTC(a, m - 1 + mover, 1));
  const fin = new Date(Date.UTC(inicio.getUTCFullYear(), inicio.getUTCMonth() + 1, 0));
  const texto = (d) => d.toISOString().slice(0, 10);
  return { desde: texto(inicio), hasta: texto(fin), nombre: `${MESES[inicio.getUTCMonth()]} ${inicio.getUTCFullYear()}` };
}

const PERIODOS = [
  { valor: 'PENDIENTE', label: 'Todo lo pendiente' },
  { valor: 'MES', label: 'Este mes' },
  { valor: 'ANTERIOR', label: 'Mes anterior' },
];

function rangoDe(periodo) {
  if (periodo === 'MES') return rangoMes(hoyLima());
  if (periodo === 'ANTERIOR') return rangoMes(hoyLima(), -1);
  return null;
}

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;

/** El mensaje de WhatsApp del cierre de mes (corto: los primeros 25 consumos). */
function mensajeEstado(estado, nombreTienda, rango) {
  // A una empresa (RUC) se le saluda con su razón social; a una persona, por su nombre.
  const empresa = String(estado.cliente_documento || '').length === 11;
  const nombre = empresa ? estado.cliente_nombre : estado.cliente_nombre.split(' ')[0];
  const lineas = estado.creditos.slice(0, 25).map((c) => `• ${fechaCorta(c.fecha)} ${c.folio}: ${soles(c.total)}${c.saldo < c.total ? ` (debe ${soles(c.saldo)})` : ''}`);
  if (estado.creditos.length > 25) lineas.push(`• y ${estado.creditos.length - 25} más`);
  return [
    `Hola ${nombre}, te envío tu estado de cuenta de ${nombreTienda || 'nuestro negocio'}${rango ? ` de ${rango.nombre}` : ''}:`,
    '',
    ...lineas,
    '',
    ...(estado.saldo_anterior > 0 ? [`Saldo anterior: ${soles(estado.saldo_anterior)}`] : []),
    `Total: ${soles(estado.total)}`,
    ...(estado.pagado > 0 ? [`Pagado: ${soles(estado.pagado)}`] : []),
    `*Total por pagar: ${soles(estado.saldo_total)}*`,
    '',
    '¡Gracias por tu preferencia!',
  ].join('\n');
}

/** Estado de cuenta para la ticketera (solo al imprimir). */
function EstadoImprimible({ estado, rango, nombreTienda, direccion, telefono, ruc }) {
  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
      {ruc && <div className="recibo-centro recibo-dato-tienda recibo-ruc">RUC {ruc}</div>}
      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">ESTADO DE CUENTA</div>
      <div className="recibo-centro recibo-comprobante-numero">{rango ? rango.nombre.toUpperCase() : `AL ${fechaCorta(hoyLima())}`}</div>
      <div className="recibo-linea"></div>
      <div className="recibo-seccion-titulo">CLIENTE</div>
      {estado.cliente_documento && (
        <div className="recibo-fila-meta">
          <span>Doc.</span>
          <span>{estado.cliente_documento}</span>
        </div>
      )}
      <div className="recibo-cliente-nombre">{estado.cliente_nombre}</div>
      <div className="recibo-linea"></div>
      {estado.creditos.map((c) => (
        <div key={c.id} className="recibo-item">
          <div className="recibo-item-detalle">
            <span>
              <strong>
                {fechaCorta(c.fecha)} · {c.folio}
              </strong>
            </span>
            <span>S/.{c.total.toFixed(2)}</span>
          </div>
          {c.productos.map((p, i) => (
            <div key={i} className="recibo-item-detalle">
              <span>
                {formatoCantidad(p.cantidad)} {p.nombre}
              </span>
              <span>{p.total_linea.toFixed(2)}</span>
            </div>
          ))}
          {c.saldo < c.total && (
            <div className="recibo-item-detalle">
              <span>Pagado</span>
              <span>-{(c.total - c.saldo).toFixed(2)}</span>
            </div>
          )}
        </div>
      ))}
      <div className="recibo-linea-doble"></div>
      {estado.saldo_anterior > 0 && (
        <div className="recibo-fila-meta">
          <span>Saldo anterior</span>
          <span>S/.{estado.saldo_anterior.toFixed(2)}</span>
        </div>
      )}
      <div className="recibo-fila-meta">
        <span>Total {rango ? 'del mes' : 'pendiente'}</span>
        <span>S/.{estado.total.toFixed(2)}</span>
      </div>
      {estado.pagado > 0 && (
        <div className="recibo-fila-meta">
          <span>Pagado</span>
          <span>-S/.{estado.pagado.toFixed(2)}</span>
        </div>
      )}
      <div className="recibo-total">
        <span>POR PAGAR</span>
        <span>S/.{estado.saldo_total.toFixed(2)}</span>
      </div>
      <div className="recibo-linea-doble"></div>
      <div className="recibo-centro recibo-disclaimer">Estado de cuenta informativo. No es un comprobante de pago.</div>
    </div>
  );
}

/**
 * Cierre de mes de un cliente que se lleva a cuenta (módulo CREDITO): todo
 * lo que consumió en el mes (o todo lo pendiente) con sus prendas o
 * productos, lo pagado y el saldo; se imprime o se manda por WhatsApp y se
 * registra el pago de varias cuentas a la vez (desde la más antigua).
 * Funciona igual si las ventas fueron con nota de venta, boleta o factura.
 */
export default function EstadoCuenta({ clienteId, nombreTienda, direccion, telefono, ruc, onCerrar, onPagado }) {
  const [periodo, setPeriodo] = useState('PENDIENTE');
  const [estado, setEstado] = useState(null);
  const [error, setError] = useState(null);
  const [monto, setMonto] = useState('');
  const [metodo, setMetodo] = useState('EFECTIVO');
  const [nota, setNota] = useState('');
  const [ocupado, setOcupado] = useState(false);
  const [aviso, setAviso] = useState(null);
  const [imprimiendo, setImprimiendo] = useState(false);
  const rango = rangoDe(periodo);

  const cargar = useCallback(() => {
    const r = rangoDe(periodo);
    api
      .creditoEstadoCuenta(clienteId, r?.desde, r?.hasta)
      .then((d) => {
        setEstado(d);
        setError(null);
      })
      .catch((e) => setError(e.message));
  }, [clienteId, periodo]);

  useEffect(() => {
    cargar();
  }, [cargar]);

  useEffect(() => {
    if (!imprimiendo) return undefined;
    const t = setTimeout(() => {
      imprimirTicket();
      setImprimiendo(false);
    }, 80);
    return () => clearTimeout(t);
  }, [imprimiendo]);

  const montoNum = Math.round((parseFloat(String(monto).replace(',', '.')) || 0) * 100) / 100;
  const puedePagar = estado && estado.saldo_total > 0 && montoNum > 0 && montoNum <= estado.saldo_total + 0.005 && !ocupado;

  const pagar = async (e) => {
    e.preventDefault();
    if (!puedePagar) return;
    setOcupado(true);
    setError(null);
    setAviso(null);
    try {
      const r = await api.creditoPagarCliente(clienteId, { monto: montoNum, metodo_pago: metodo, nota: nota.trim() || null });
      setAviso(
        `Pago de ${soles(r.pagado)} registrado en ${r.aplicado.length} ${r.aplicado.length === 1 ? 'cuenta' : 'cuentas'}.` +
          (r.saldo_total > 0 ? ` Sigue debiendo ${soles(r.saldo_total)}.` : ' Quedó al día.')
      );
      setMonto('');
      setNota('');
      cargar();
      onPagado?.();
    } catch (err) {
      setError(err.message);
    } finally {
      setOcupado(false);
    }
  };

  const whatsapp = estado && enlaceWhatsapp(estado.cliente_telefono, mensajeEstado(estado, nombreTienda, rango));

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Estado de cuenta">
        <div className="pm-modal-cabecera">
          <h2>Estado de cuenta{estado ? ` · ${estado.cliente_nombre}` : ''}</h2>
          <p>
            {estado?.cliente_documento ? `${estado.cliente_documento} · ` : ''}
            {estado?.cliente_telefono ? `Tel. ${estado.cliente_telefono} · ` : ''}
            Para el cierre de mes: lo que se llevó a cuenta, lo pagado y lo que debe.
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          <div className="pm-opciones" role="tablist" aria-label="Periodo">
            {PERIODOS.map((p) => (
              <button key={p.valor} type="button" role="tab" aria-selected={periodo === p.valor} className={periodo === p.valor ? 'activo' : ''} onClick={() => setPeriodo(p.valor)}>
                {p.label}
                {p.valor !== 'PENDIENTE' ? ` (${rangoDe(p.valor).nombre})` : ''}
              </button>
            ))}
          </div>

          {error && <p className="pm-mensaje pm-mensaje-error" style={{ marginTop: 10 }}>{error}</p>}
          {aviso && <p className="pm-mensaje pm-mensaje-exito" style={{ marginTop: 10 }}>{aviso}</p>}

          {!estado ? (
            <p className="pm-vacio">{error ? 'No se pudo cargar.' : 'Cargando...'}</p>
          ) : (
            <>
              <div className="pm-resumen" style={{ marginTop: 12 }}>
                {rango && (
                  <div className="pm-dato">
                    <span>Saldo anterior</span>
                    <strong>{soles(estado.saldo_anterior)}</strong>
                  </div>
                )}
                <div className="pm-dato">
                  <span>{rango ? `Consumo de ${rango.nombre}` : 'Total pendiente'}</span>
                  <strong>{soles(estado.total)}</strong>
                </div>
                <div className="pm-dato">
                  <span>Pagado</span>
                  <strong>{soles(estado.pagado)}</strong>
                </div>
                <div className={`pm-dato${estado.vencido > 0 ? ' pm-dato-alerta' : ''}`}>
                  <span>Debe hoy (todo)</span>
                  <strong>{soles(estado.saldo_total)}</strong>
                </div>
              </div>

              <h3 className="pm-seccion">{rango ? `Lo que se llevó en ${rango.nombre}` : 'Cuentas pendientes'}</h3>
              {estado.creditos.length === 0 ? (
                <p className="pm-vacio">{rango ? 'No hay ventas a cuenta en este mes.' : 'No debe nada.'}</p>
              ) : (
                <table className="pm-lineas">
                  <tbody>
                    {estado.creditos.map((c) => (
                      <tr key={c.id}>
                        <td>
                          {fechaCorta(c.fecha)} · {c.folio}
                          <small>{c.productos.map((p) => `${formatoCantidad(p.cantidad)} ${p.nombre}`).join(' · ')}</small>
                        </td>
                        <td className="pm-num">{soles(c.total)}</td>
                        <td className="pm-num">{c.saldo > 0 ? `debe ${soles(c.saldo)}` : 'pagado'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}

              {estado.saldo_total > 0 && (
                <form onSubmit={pagar}>
                  <h3 className="pm-seccion">Registrar pago</h3>
                  <div className="pm-campos">
                    <label className="pm-campo">
                      <span>Monto (S/)</span>
                      <input inputMode="decimal" placeholder="0.00" value={monto} onChange={(e) => setMonto(e.target.value)} />
                      <small>
                        <button type="button" className="cred-todo" onClick={() => setMonto(estado.saldo_total.toFixed(2))}>
                          Paga todo: {soles(estado.saldo_total)}
                        </button>
                      </small>
                    </label>
                    <label className="pm-campo">
                      <span>Con qué paga</span>
                      <select value={metodo} onChange={(e) => setMetodo(e.target.value)}>
                        {METODOS_DE_ABONO.map((m) => (
                          <option key={m} value={m}>
                            {nombreMetodo(m)}
                          </option>
                        ))}
                      </select>
                      <small>{metodo === 'EFECTIVO' ? 'Entra a la caja abierta como ingreso.' : 'No entra al efectivo de caja.'}</small>
                    </label>
                    <label className="pm-campo pm-campo-ancho">
                      <span>Nota (opcional)</span>
                      <input value={nota} maxLength={200} onChange={(e) => setNota(e.target.value)} placeholder="Pago de octubre, N° de operación..." />
                    </label>
                  </div>
                  <p className="pm-subtitulo">Se aplica primero a las cuentas más antiguas.</p>
                  {montoNum > estado.saldo_total + 0.005 && <p className="pm-mensaje pm-mensaje-error">El pago no puede ser mayor que lo que debe.</p>}
                  <button type="submit" className="pm-boton" style={{ marginTop: 10, width: '100%' }} disabled={!puedePagar}>
                    {ocupado ? 'Guardando...' : `Registrar pago${montoNum > 0 ? ` de ${soles(montoNum)}` : ''}`}
                  </button>
                </form>
              )}
            </>
          )}
        </div>
        <div className="pm-modal-pie">
          {estado && (
            <button className="pm-boton-secundario" onClick={() => setImprimiendo(true)}>
              <Printer size={15} /> Imprimir
            </button>
          )}
          {whatsapp && (
            <a className="pm-boton-secundario" href={whatsapp} target="_blank" rel="noreferrer" style={{ textDecoration: 'none' }}>
              <MessageCircle size={15} /> Enviar por WhatsApp
            </a>
          )}
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cerrar
          </button>
        </div>
      </div>
      {estado &&
        imprimiendo &&
        createPortal(
          <EstadoImprimible estado={estado} rango={rango} nombreTienda={nombreTienda} direccion={direccion} telefono={telefono} ruc={ruc} />,
          document.body
        )}
    </div>
  );
}
