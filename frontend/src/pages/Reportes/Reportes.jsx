import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import './Reportes.css';

const hoy = () => new Date().toISOString().slice(0, 10);

const RANGOS_RAPIDOS = [
  { label: 'Hoy', dias: 0 },
  { label: '7 días', dias: 6 },
  { label: '30 días', dias: 29 },
];

export default function Reportes() {
  const [fechaInicio, setFechaInicio] = useState(hoy());
  const [fechaFin, setFechaFin] = useState(hoy());
  const [estadisticas, setEstadisticas] = useState(null);
  const [productosVendidos, setProductosVendidos] = useState([]);
  const [ventas, setVentas] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);

  const cargar = () => {
    setCargando(true);
    setMensaje(null);
    Promise.all([
      api.reportesEstadisticas(fechaInicio, fechaFin),
      api.reportesProductosVendidos(fechaInicio, fechaFin),
      api.reportesVentas(fechaInicio, fechaFin),
    ])
      .then(([e, p, v]) => {
        setEstadisticas(e);
        setProductosVendidos(p);
        setVentas(v);
      })
      .catch((err) => setMensaje({ tipo: 'error', texto: err.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fechaInicio, fechaFin]);

  const aplicarRangoRapido = (dias) => {
    const fin = new Date();
    const inicio = new Date();
    inicio.setDate(inicio.getDate() - dias);
    setFechaInicio(inicio.toISOString().slice(0, 10));
    setFechaFin(fin.toISOString().slice(0, 10));
  };

  return (
    <div className="rep-layout">
      <div className="rep-header">
        <h1>Reportes</h1>
        <div className="rep-rangos-rapidos">
          {RANGOS_RAPIDOS.map((r) => (
            <button key={r.label} onClick={() => aplicarRangoRapido(r.dias)}>
              {r.label}
            </button>
          ))}
        </div>
      </div>

      <div className="rep-fechas">
        <div className="rep-campo-fecha">
          <label>Desde</label>
          <input type="date" value={fechaInicio} onChange={(e) => setFechaInicio(e.target.value)} max={fechaFin} />
        </div>
        <div className="rep-campo-fecha">
          <label>Hasta</label>
          <input type="date" value={fechaFin} onChange={(e) => setFechaFin(e.target.value)} min={fechaInicio} max={hoy()} />
        </div>
      </div>

      {mensaje && <p className="rep-mensaje">{mensaje.texto}</p>}

      {cargando ? (
        <p className="rep-cargando">Cargando...</p>
      ) : (
        <>
          {estadisticas && (
            <div className="rep-tarjetas">
              <div className="rep-tarjeta">
                <span className="rep-tarjeta-label">Ventas</span>
                <strong className="rep-tarjeta-valor">{estadisticas.ventas_cantidad}</strong>
                <span className="rep-tarjeta-sub">S/ {estadisticas.ventas_total.toFixed(2)}</span>
              </div>
              <div className="rep-tarjeta">
                <span className="rep-tarjeta-label">Ticket promedio</span>
                <strong className="rep-tarjeta-valor">S/ {estadisticas.ticket_promedio.toFixed(2)}</strong>
              </div>
              <div className="rep-tarjeta rep-tarjeta-negativa">
                <span className="rep-tarjeta-label">Devoluciones</span>
                <strong className="rep-tarjeta-valor">{estadisticas.devoluciones_cantidad}</strong>
                <span className="rep-tarjeta-sub">- S/ {estadisticas.devoluciones_total.toFixed(2)}</span>
              </div>
              <div className="rep-tarjeta rep-tarjeta-destacada">
                <span className="rep-tarjeta-label">Total neto</span>
                <strong className="rep-tarjeta-valor">S/ {estadisticas.total_neto.toFixed(2)}</strong>
              </div>
            </div>
          )}

          <div className="rep-seccion">
            <h2>Productos más vendidos</h2>
            <div className="rep-tabla-wrapper">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Producto</th>
                    <th>Cantidad</th>
                    <th>Total vendido</th>
                  </tr>
                </thead>
                <tbody>
                  {productosVendidos.map((p, idx) => (
                    <tr key={idx}>
                      <td>{p.producto_nombre}</td>
                      <td>{p.cantidad_vendida}</td>
                      <td>S/ {p.total_vendido.toFixed(2)}</td>
                    </tr>
                  ))}
                  {productosVendidos.length === 0 && (
                    <tr>
                      <td colSpan={3} className="rep-sin-resultados">
                        No hay ventas en este período.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>

          <div className="rep-seccion">
            <h2>Ventas del período ({ventas.length})</h2>
            <div className="rep-tabla-wrapper">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Folio</th>
                    <th>Fecha</th>
                    <th>Cajero</th>
                    <th>Método</th>
                    <th>Estado</th>
                    <th>Total</th>
                  </tr>
                </thead>
                <tbody>
                  {ventas.map((v) => (
                    <tr key={v.id}>
                      <td>{v.folio}</td>
                      <td>{new Date(v.fecha_hora).toLocaleString('es-PE')}</td>
                      <td>{v.cajero}</td>
                      <td>{v.metodo_pago.replace('_', '/')}</td>
                      <td>
                        <span className={`rep-badge rep-badge-${v.estado.toLowerCase()}`}>{v.estado}</span>
                      </td>
                      <td>S/ {v.total.toFixed(2)}</td>
                    </tr>
                  ))}
                  {ventas.length === 0 && (
                    <tr>
                      <td colSpan={6} className="rep-sin-resultados">
                        No hay ventas en este período.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </div>
  );
}