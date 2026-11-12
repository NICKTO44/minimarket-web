import { useState, useEffect, useCallback } from 'react';
import { Eye } from 'lucide-react';
import { api } from '../../api/api';
import DetalleCaja from './DetalleCaja';
import { diaLima, horaLima, textoDiferencia } from '../../utils/formato';
import './HistorialCaja.css';

const MESES = [
  'Enero', 'Febrero', 'Marzo', 'Abril', 'Mayo', 'Junio',
  'Julio', 'Agosto', 'Septiembre', 'Octubre', 'Noviembre', 'Diciembre',
];

function primerDiaDelMes(anio, mes) {
  return `${anio}-${String(mes + 1).padStart(2, '0')}-01`;
}
function ultimoDiaDelMes(anio, mes) {
  const ultimo = new Date(anio, mes + 1, 0).getDate();
  return `${anio}-${String(mes + 1).padStart(2, '0')}-${String(ultimo).padStart(2, '0')}`;
}

export default function HistorialCaja() {
  const ahora = new Date();
  const [anio, setAnio] = useState(ahora.getFullYear());
  const [mes, setMes] = useState(ahora.getMonth());
  const [cajas, setCajas] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);
  // Caja abierta en la ventana de detalle.
  const [detalleId, setDetalleId] = useState(null);
  // Cajero elegido para ver solo sus turnos ('TODOS' = todos).
  const [cajeroFiltro, setCajeroFiltro] = useState('TODOS');
  const cerrarDetalle = useCallback(() => setDetalleId(null), []);

  const cargar = () => {
    setCargando(true);
    setMensaje(null);
    api
      .cajasListar(primerDiaDelMes(anio, mes), ultimoDiaDelMes(anio, mes))
      .then(setCajas)
      .catch((e) => setMensaje(e.message))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anio, mes]);

  const cambiarMes = (delta) => {
    let nuevoMes = mes + delta;
    let nuevoAnio = anio;
    if (nuevoMes < 0) {
      nuevoMes = 11;
      nuevoAnio -= 1;
    } else if (nuevoMes > 11) {
      nuevoMes = 0;
      nuevoAnio += 1;
    }
    setMes(nuevoMes);
    setAnio(nuevoAnio);
  };

  // Yape y Plin de un turno. Cajas de antes de separarlos: lo cobrado por
  // billetera va junto en Yape/Plin.
  const yapeDe = (c) => Number(c.ventas_yape) || 0;
  const plinDe = (c) => Number(c.ventas_plin) || 0;
  const juntoDe = (c) => Number(c.ventas_yape_plin) || 0;
  const efectivoDe = (c) => Number(c.ventas_efectivo) || 0;
  const otrosDe = (c) =>
    Math.max(0, (Number(c.total_ventas) || 0) - efectivoDe(c) - yapeDe(c) - plinDe(c) - juntoDe(c));

  const cajeros = [...new Set(cajas.map((c) => c.usuario_nombre).filter(Boolean))].sort();
  const cajasVisibles = cajeroFiltro === 'TODOS' ? cajas : cajas.filter((c) => c.usuario_nombre === cajeroFiltro);

  const totalMes = cajasVisibles.reduce((sum, c) => sum + c.total_ventas, 0);
  const netoMes = cajasVisibles.reduce((sum, c) => sum + (c.total_ventas - c.devoluciones_monto), 0);
  const diasConCaja = cajasVisibles.length;

  // Resumen por cajero para comparar sus turnos del mes.
  const porCajero = cajeros.map((nombre) => {
    const suyas = cajas.filter((c) => c.usuario_nombre === nombre);
    const cerradas = suyas.filter((c) => c.diferencia !== null && c.diferencia !== undefined);
    const vendido = suyas.reduce((s, c) => s + (Number(c.total_ventas) || 0), 0);
    return {
      nombre,
      turnos: suyas.length,
      abiertas: suyas.length - cerradas.length,
      vendido,
      promedio: suyas.length ? vendido / suyas.length : 0,
      efectivo: suyas.reduce((s, c) => s + efectivoDe(c), 0),
      yape: suyas.reduce((s, c) => s + yapeDe(c), 0),
      plin: suyas.reduce((s, c) => s + plinDe(c), 0),
      junto: suyas.reduce((s, c) => s + juntoDe(c), 0),
      otros: suyas.reduce((s, c) => s + otrosDe(c), 0),
      diferencia: cerradas.reduce((s, c) => s + (Number(c.diferencia) || 0), 0),
      conDiferencia: cerradas.filter((c) => Math.abs(Number(c.diferencia) || 0) >= 0.01).length,
    };
  });
  const hayJunto = cajasVisibles.some((c) => juntoDe(c) > 0.004);
  const hayOtros = cajasVisibles.some((c) => otrosDe(c) > 0.004);
  const soles = (n) => `S/ ${(Number(n) || 0).toFixed(2)}`;

  const estadoDiferencia = (dif) => {
    if (dif === null || dif === undefined) return 'sin-cerrar';
    if (Math.abs(dif) < 0.01) return 'ok';
    if (Math.abs(dif) <= 10) return 'aceptable';
    return 'alerta';
  };

  return (
    <div className="hcaja-layout">
      <div className="hcaja-header">
        <h2>Historial de caja</h2>
        <div className="hcaja-selector-mes">
          <button onClick={() => cambiarMes(-1)}>‹</button>
          <span>{MESES[mes]} {anio}</span>
          <button onClick={() => cambiarMes(1)}>›</button>
        </div>
      </div>

      <div className="hcaja-resumen-mes">
        <div className="hcaja-resumen-item">
          <span>Sesiones de caja</span>
          <strong>{diasConCaja}</strong>
        </div>
        <div className="hcaja-resumen-item">
          <span>Total vendido bruto</span>
          <strong>S/ {totalMes.toFixed(2)}</strong>
        </div>
        <div className="hcaja-resumen-item hcaja-resumen-destacado">
          <span>Total neto del mes</span>
          <strong>S/ {netoMes.toFixed(2)}</strong>
        </div>
      </div>

      {cajeros.length > 0 && (
        <div className="hcaja-filtro" role="group" aria-label="Filtrar por cajero">
          <span className="hcaja-filtro-titulo">Cajero</span>
          {['TODOS', ...cajeros].map((n) => (
            <button
              key={n}
              type="button"
              className={`hcaja-chip${cajeroFiltro === n ? ' activo' : ''}`}
              aria-pressed={cajeroFiltro === n}
              onClick={() => setCajeroFiltro(n)}
            >
              {n === 'TODOS' ? 'Todos' : n}
            </button>
          ))}
        </div>
      )}

      {porCajero.length > 0 && (
        <div className="hcaja-por-cajero">
          <h3>Resumen por cajero</h3>
          <div className="hcaja-tabla-wrapper">
            <table className="hcaja-tabla">
              <thead>
                <tr>
                  <th>Cajero</th>
                  <th>Turnos</th>
                  <th>Vendido</th>
                  <th>Promedio por turno</th>
                  <th>Efectivo</th>
                  <th>Yape</th>
                  <th>Plin</th>
                  {porCajero.some((r) => r.junto > 0.004) && <th>Yape/Plin</th>}
                  {porCajero.some((r) => r.otros > 0.004) && <th>Tarjeta / Transf.</th>}
                  <th>Diferencias</th>
                </tr>
              </thead>
              <tbody>
                {porCajero.map((r) => (
                  <tr
                    key={r.nombre}
                    className={`hcaja-fila-cajero${cajeroFiltro === r.nombre ? ' activo' : ''}`}
                    onClick={() => setCajeroFiltro(cajeroFiltro === r.nombre ? 'TODOS' : r.nombre)}
                    title="Ver solo sus turnos"
                  >
                    <td><strong>{r.nombre}</strong></td>
                    <td>
                      {r.turnos}
                      {r.abiertas > 0 && <span className="hcaja-hora">{r.abiertas} sin cerrar</span>}
                    </td>
                    <td>{soles(r.vendido)}</td>
                    <td>{soles(r.promedio)}</td>
                    <td>{soles(r.efectivo)}</td>
                    <td>{soles(r.yape)}</td>
                    <td>{soles(r.plin)}</td>
                    {porCajero.some((x) => x.junto > 0.004) && <td>{soles(r.junto)}</td>}
                    {porCajero.some((x) => x.otros > 0.004) && <td>{soles(r.otros)}</td>}
                    <td>
                      <span className={`hcaja-diferencia hcaja-diferencia-${estadoDiferencia(r.turnos - r.abiertas > 0 ? r.diferencia : null)}`}>
                        {r.turnos - r.abiertas > 0 ? textoDiferencia(r.diferencia) : '—'}
                      </span>
                      {r.conDiferencia > 0 && (
                        <span className="hcaja-hora">
                          {r.conDiferencia} {r.conDiferencia === 1 ? 'turno descuadrado' : 'turnos descuadrados'}
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {mensaje && <p className="hcaja-mensaje">{mensaje}</p>}

      {cargando ? (
        <p className="hcaja-cargando">Cargando...</p>
      ) : (
        <div className="hcaja-tabla-wrapper">
          <table className="hcaja-tabla">
            <thead>
              <tr>
                <th>Apertura</th>
                <th>Cierre</th>
                <th>Cajero</th>
                <th>Monto inicial</th>
                <th>Ventas</th>
                <th>Efectivo</th>
                <th>Yape</th>
                <th>Plin</th>
                {hayJunto && <th>Yape/Plin</th>}
                {hayOtros && <th>Tarjeta / Transf.</th>}
                <th>Diferencia</th>
                <th>Estado</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {cajasVisibles.map((c) => (
                <tr key={c.id}>
                  <td>
                    {diaLima(c.fecha_apertura, { day: '2-digit', month: 'short' })}
                    <span className="hcaja-hora">{horaLima(c.fecha_apertura, { hour: '2-digit', minute: '2-digit' })}</span>
                  </td>
                  <td>
                    {c.fecha_cierre ? (
                      <>
                        {diaLima(c.fecha_cierre, { day: '2-digit', month: 'short' })}
                        <span className="hcaja-hora">{horaLima(c.fecha_cierre, { hour: '2-digit', minute: '2-digit' })}</span>
                      </>
                    ) : (
                      <span className="hcaja-hora">En turno</span>
                    )}
                  </td>
                  <td>{c.usuario_nombre}</td>
                  <td>S/ {c.monto_inicial.toFixed(2)}</td>
                  <td>S/ {c.total_ventas.toFixed(2)} <span className="hcaja-transacciones">({c.numero_transacciones})</span></td>
                  <td>{soles(efectivoDe(c))}</td>
                  <td>{soles(yapeDe(c))}</td>
                  <td>{soles(plinDe(c))}</td>
                  {hayJunto && <td>{soles(juntoDe(c))}</td>}
                  {hayOtros && <td>{soles(otrosDe(c))}</td>}
                  <td>
                    {c.diferencia !== null && c.diferencia !== undefined ? (
                      <span className={`hcaja-diferencia hcaja-diferencia-${estadoDiferencia(c.diferencia)}`}>
                        {textoDiferencia(c.diferencia)}
                      </span>
                    ) : (
                      <span className="hcaja-diferencia hcaja-diferencia-sin-cerrar">—</span>
                    )}
                  </td>
                  <td>
                    <span className={`hcaja-badge hcaja-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                  </td>
                  <td>
                    <button type="button" className="hcaja-ver" onClick={() => setDetalleId(c.id)}>
                      <Eye size={14} /> Ver
                    </button>
                  </td>
                </tr>
              ))}
              {cajasVisibles.length === 0 && (
                <tr>
                  <td colSpan={11 + (hayJunto ? 1 : 0) + (hayOtros ? 1 : 0)} className="hcaja-sin-resultados">
                    {cajeroFiltro === 'TODOS'
                      ? `No hubo ninguna caja abierta en ${MESES[mes]} ${anio}.`
                      : `${cajeroFiltro} no abrió caja en ${MESES[mes]} ${anio}.`}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {detalleId && <DetalleCaja cajaId={detalleId} onCerrar={cerrarDetalle} />}
    </div>
  );
}