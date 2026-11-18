import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import { hoyLima } from '../../utils/formato';

const soles = (n) => `S/ ${(Number(n) || 0).toFixed(2)}`;
const porcentaje = (n) => (n == null ? '—' : `${n.toFixed(1)}%`);
const fecha = (f) => f.split('-').reverse().join('/');

const RANGOS = [
  { clave: 'HOY', label: 'Hoy', desde: 0, hasta: 0 },
  { clave: 'AYER', label: 'Ayer', desde: -1, hasta: -1 },
  { clave: 'SEMANA', label: '7 días', desde: -6, hasta: 0 },
];

/**
 * Ganancias de un día, una semana o las fechas que se elijan (pestaña
 * Ganancias → "Por días"). Solo la ganancia de las ventas: los gastos del
 * negocio y la merma son del mes y están en "Por mes".
 */
export default function GananciasPeriodo() {
  const [desde, setDesde] = useState(hoyLima());
  const [hasta, setHasta] = useState(hoyLima());
  const [datos, setDatos] = useState(null);
  const [error, setError] = useState(null);
  const clave = `${desde}|${hasta}`;
  const [cargado, setCargado] = useState('');

  useEffect(() => {
    let vigente = true;
    api
      .reportesGananciasPeriodo(desde, hasta)
      .then((d) => {
        if (!vigente) return;
        setDatos(d);
        setError(null);
        setCargado(`${desde}|${hasta}`);
      })
      .catch((e) => {
        if (!vigente) return;
        setError(e.message);
        setCargado(`${desde}|${hasta}`);
      });
    return () => {
      vigente = false;
    };
  }, [desde, hasta]);

  const elegir = (r) => {
    setDesde(hoyLima(r.desde));
    setHasta(hoyLima(r.hasta));
  };
  const activo = RANGOS.find((r) => hoyLima(r.desde) === desde && hoyLima(r.hasta) === hasta)?.clave;

  return (
    <div className="gp">
      <div className="gp-filtros">
        <div className="rep-rangos-rapidos">
          {RANGOS.map((r) => (
            <button key={r.clave} type="button" className={activo === r.clave ? 'activo' : ''} onClick={() => elegir(r)}>
              {r.label}
            </button>
          ))}
        </div>
        <div className="rep-fechas">
          <div className="rep-campo-fecha">
            <label>Desde</label>
            <input type="date" value={desde} max={hasta} onChange={(e) => e.target.value && setDesde(e.target.value)} />
          </div>
          <div className="rep-campo-fecha">
            <label>Hasta</label>
            <input type="date" value={hasta} min={desde} max={hoyLima()} onChange={(e) => e.target.value && setHasta(e.target.value)} />
          </div>
        </div>
      </div>

      {error && <p className="rep-mensaje">{error}</p>}
      {!datos || cargado !== clave ? (
        !error && <p className="rep-cargando">Cargando...</p>
      ) : (
        <>
          <p className="gp-titulo">
            {datos.desde === datos.hasta ? `Ganancia del ${fecha(datos.desde)}` : `Ganancia del ${fecha(datos.desde)} al ${fecha(datos.hasta)}`}
          </p>
          <div className="rep-tarjetas gan-tarjetas">
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Vendido</span>
              <strong className="rep-tarjeta-valor">{soles(datos.vendido)}</strong>
              {datos.sin_costo > 0.005 && <span className="rep-tarjeta-sub">{soles(datos.sin_costo)} sin precio de compra</span>}
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Costo de lo vendido</span>
              <strong className="rep-tarjeta-valor">{soles(datos.costo)}</strong>
              <span className="rep-tarjeta-sub">Lo que te costó esa mercadería</span>
            </div>
            <div className="rep-tarjeta rep-tarjeta-destacada">
              <span className="rep-tarjeta-label">Ganancia</span>
              <strong className="rep-tarjeta-valor">{soles(datos.ganancia)}</strong>
              {datos.devoluciones.monto > 0.005 && (
                <span className="rep-tarjeta-sub">Ya resta {soles(datos.devoluciones.monto)} devueltos</span>
              )}
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Margen</span>
              <strong className="rep-tarjeta-valor">{porcentaje(datos.margen)}</strong>
              <span className="rep-tarjeta-sub">De cada S/ 100 vendidos</span>
            </div>
          </div>

          <div className="rep-seccion">
            <h2>Por producto</h2>
            <div className="rep-tabla-wrapper">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Producto</th>
                    <th>Cantidad</th>
                    <th>Vendido</th>
                    <th>Costo</th>
                    <th>Ganancia</th>
                  </tr>
                </thead>
                <tbody>
                  {datos.productos.map((p) => (
                    <tr key={p.producto_id}>
                      <td>
                        {p.nombre}
                        <small className="gp-categoria">{p.categoria}</small>
                      </td>
                      <td>{Number(p.cantidad.toFixed(3))}</td>
                      <td>{soles(p.vendido)}</td>
                      <td>{p.costo == null ? 'Sin precio de compra' : soles(p.costo)}</td>
                      <td>{p.ganancia == null ? '—' : soles(p.ganancia)}</td>
                    </tr>
                  ))}
                  {datos.productos.length === 0 && (
                    <tr>
                      <td colSpan={5} className="rep-sin-resultados">
                        No hay ventas en este periodo.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            <p className="rep-nota">
              Es la ganancia de las ventas. Los gastos del negocio (alquiler, luz…) y la merma son del mes: los ves en "Por mes".
            </p>
          </div>
        </>
      )}
    </div>
  );
}
