import { useState, useEffect } from 'react';
import { api } from '../../api/api';
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

  const totalMes = cajas.reduce((sum, c) => sum + c.total_ventas, 0);
  const netoMes = cajas.reduce((sum, c) => sum + (c.total_ventas - c.devoluciones_monto), 0);
  const diasConCaja = cajas.length;

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

      {mensaje && <p className="hcaja-mensaje">{mensaje}</p>}

      {cargando ? (
        <p className="hcaja-cargando">Cargando...</p>
      ) : (
        <div className="hcaja-tabla-wrapper">
          <table className="hcaja-tabla">
            <thead>
              <tr>
                <th>Fecha</th>
                <th>Cajero</th>
                <th>Monto inicial</th>
                <th>Ventas</th>
                <th>Diferencia</th>
                <th>Estado</th>
              </tr>
            </thead>
            <tbody>
              {cajas.map((c) => (
                <tr key={c.id}>
                  <td>
                    {new Date(c.fecha_apertura).toLocaleDateString('es-PE', { day: '2-digit', month: 'short' })}
                    <span className="hcaja-hora">{new Date(c.fecha_apertura).toLocaleTimeString('es-PE', { hour: '2-digit', minute: '2-digit' })}</span>
                  </td>
                  <td>{c.usuario_nombre}</td>
                  <td>S/ {c.monto_inicial.toFixed(2)}</td>
                  <td>S/ {c.total_ventas.toFixed(2)} <span className="hcaja-transacciones">({c.numero_transacciones})</span></td>
                  <td>
                    {c.diferencia !== null && c.diferencia !== undefined ? (
                      <span className={`hcaja-diferencia hcaja-diferencia-${estadoDiferencia(c.diferencia)}`}>
                        {c.diferencia >= 0 ? '+' : ''}S/ {c.diferencia.toFixed(2)}
                      </span>
                    ) : (
                      <span className="hcaja-diferencia hcaja-diferencia-sin-cerrar">—</span>
                    )}
                  </td>
                  <td>
                    <span className={`hcaja-badge hcaja-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                  </td>
                </tr>
              ))}
              {cajas.length === 0 && (
                <tr>
                  <td colSpan={6} className="hcaja-sin-resultados">
                    No hubo ninguna caja abierta en {MESES[mes]} {anio}.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}