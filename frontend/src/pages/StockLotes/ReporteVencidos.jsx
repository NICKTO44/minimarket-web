import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import { TIPO_XLSX, escribirXlsx } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { fechaCorta, hoyLima } from '../../utils/formato';

// Reporte de productos vencidos (solo administrador): los lotes que se
// descartaron en un rango de fechas, quién y cuánto se perdió, y los que
// hoy están vencidos y todavía no se descartan. Sirve para revisar o
// auditar el inventario. La hora de cada descarte ya viene en hora de Lima.

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const cantidadTexto = (n) => (Number.isInteger(n) ? n : Number(Number(n).toFixed(2)));
const MOTIVOS = { VENCIDO: 'Vencido', POR_VENCER: 'Antes de vencer' };

export default function ReporteVencidos() {
  const [desde, setDesde] = useState(`${hoyLima().slice(0, 7)}-01`);
  const [hasta, setHasta] = useState(hoyLima());
  const [reporte, setReporte] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);

  const cargar = () => {
    setCargando(true);
    setError(null);
    api
      .lotesMermas(desde, hasta)
      .then(setReporte)
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
  }, [desde, hasta]);

  const exportar = () => {
    if (!reporte) return;
    const filas = [
      ['Fecha y hora', 'Producto', 'N.° de lote', 'Vencía', 'Cantidad', 'Unidad', 'Costo unit. (S/)', 'Costo total (S/)', 'Motivo', 'Descartado por'],
      ...reporte.devueltos_vencidos.map((d) => [
        d.fecha_hora,
        d.producto_nombre,
        '',
        '',
        d.cantidad,
        d.unidad_medida,
        d.costo_total != null && d.cantidad ? Number((d.costo_total / d.cantidad).toFixed(4)) : '',
        d.costo_total ?? '',
        `Devuelto por cliente (${d.folio_devolucion})`,
        '',
      ]),
      ...reporte.descartados.map((m) => [
        m.fecha_hora,
        m.producto_nombre,
        m.numero_lote || '',
        m.fecha_vencimiento.slice(0, 10),
        m.cantidad,
        m.unidad_medida,
        m.costo_unitario ?? '',
        m.costo_total ?? '',
        MOTIVOS[m.motivo] || m.motivo,
        m.usuario_nombre || '',
      ]),
    ];
    descargarArchivo(
      escribirXlsx(filas, { hoja: 'Vencidos', anchos: [20, 36, 14, 12, 10, 10, 14, 14, 16, 22] }),
      `Productos-vencidos-${reporte.desde}-a-${reporte.hasta}.xlsx`,
      { tipo: TIPO_XLSX }
    );
  };

  return (
    <div className="sl-reporte">
      <div className="sl-reporte-filtros">
        <label>
          Desde
          <input type="date" value={desde} max={hasta} onChange={(e) => e.target.value && setDesde(e.target.value)} />
        </label>
        <label>
          Hasta
          <input type="date" value={hasta} min={desde} max={hoyLima()} onChange={(e) => e.target.value && setHasta(e.target.value)} />
        </label>
        <button
          type="button"
          className="sl-boton-exportar"
          onClick={exportar}
          disabled={!reporte || reporte.descartados.length + reporte.devueltos_vencidos.length === 0}
        >
          Descargar Excel
        </button>
      </div>

      {error && <p className="sl-mensaje sl-mensaje-error">{error}</p>}

      {cargando && !reporte ? (
        <p className="sl-cargando">Cargando...</p>
      ) : reporte && (
        <>
          {!reporte.disponible && (
            <p className="sl-mensaje sl-mensaje-error">
              El reporte se está preparando en este negocio. Los descartes se registran desde la próxima actualización.
            </p>
          )}

          <div className="sl-resumen">
            <div className="sl-resumen-dato">
              <span>Lotes descartados</span>
              <strong>{reporte.descartados.length}</strong>
            </div>
            <div className="sl-resumen-dato">
              <span>Pérdida al costo</span>
              <strong>{soles(reporte.total_costo + reporte.total_costo_devueltos)}</strong>
              {reporte.total_costo_devueltos > 0.005 && (
                <small>
                  {soles(reporte.total_costo)} descartado · {soles(reporte.total_costo_devueltos)} devuelto por clientes
                </small>
              )}
              {reporte.sin_costo > 0 && <small>{reporte.sin_costo} lote(s) sin precio de compra, no suman</small>}
            </div>
            <div className={`sl-resumen-dato${reporte.vencidos_sin_descartar.length > 0 ? ' sl-resumen-alerta' : ''}`}>
              <span>Vencidos sin descartar hoy</span>
              <strong>{reporte.vencidos_sin_descartar.length}</strong>
            </div>
          </div>

          <h2 className="sl-reporte-titulo">
            Descartados del {fechaCorta(reporte.desde)} al {fechaCorta(reporte.hasta)}
          </h2>
          <p className="sl-reporte-nota">
            Lo descartado se resta de la ganancia neta del mes en Reportes → Ganancias (si ese reporte está activo).
          </p>
          {reporte.descartados.length === 0 ? (
            <p className="sl-reporte-vacio">No se descartó ningún lote en estas fechas.</p>
          ) : (
            <div className="sl-tabla-caja">
              <table className="sl-tabla">
                <thead>
                  <tr>
                    <th>Fecha</th>
                    <th>Producto</th>
                    <th>Lote</th>
                    <th>Vencía</th>
                    <th className="sl-num">Cantidad</th>
                    <th className="sl-num">Costo</th>
                    <th>Motivo</th>
                    <th>Por</th>
                  </tr>
                </thead>
                <tbody>
                  {reporte.descartados.map((m) => (
                    <tr key={m.id}>
                      <td>
                        {fechaCorta(m.fecha_hora)} <small>{m.fecha_hora.slice(11, 16)}</small>
                      </td>
                      <td>{m.producto_nombre}</td>
                      <td>{m.numero_lote || '—'}</td>
                      <td>{fechaCorta(m.fecha_vencimiento)}</td>
                      <td className="sl-num">
                        {cantidadTexto(m.cantidad)} {m.unidad_medida.toLowerCase()}
                      </td>
                      <td className="sl-num">{m.costo_total == null ? '—' : soles(m.costo_total)}</td>
                      <td>
                        <span className={`sl-motivo sl-motivo-${m.motivo.toLowerCase()}`}>{MOTIVOS[m.motivo] || m.motivo}</span>
                      </td>
                      <td>{m.usuario_nombre || '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {reporte.devueltos_vencidos.length > 0 && (
            <>
              <h2 className="sl-reporte-titulo">Devueltos por clientes como vencidos</h2>
              <p className="sl-reporte-nota">Registrados en Devoluciones con condición "Vencido". No volvieron al stock.</p>
              <div className="sl-tabla-caja">
                <table className="sl-tabla">
                  <thead>
                    <tr>
                      <th>Fecha</th>
                      <th>Devolución</th>
                      <th>Producto</th>
                      <th className="sl-num">Cantidad</th>
                      <th className="sl-num">Devuelto al cliente</th>
                      <th className="sl-num">Costo</th>
                    </tr>
                  </thead>
                  <tbody>
                    {reporte.devueltos_vencidos.map((d, i) => (
                      <tr key={`${d.folio_devolucion}-${i}`}>
                        <td>
                          {fechaCorta(d.fecha_hora)} <small>{d.fecha_hora.slice(11, 16)}</small>
                        </td>
                        <td>{d.folio_devolucion}</td>
                        <td>{d.producto_nombre}</td>
                        <td className="sl-num">
                          {cantidadTexto(d.cantidad)} {d.unidad_medida.toLowerCase()}
                        </td>
                        <td className="sl-num">{soles(d.monto)}</td>
                        <td className="sl-num">{d.costo_total == null ? '—' : soles(d.costo_total)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}

          <h2 className="sl-reporte-titulo">Vencidos que siguen en el inventario</h2>
          {reporte.vencidos_sin_descartar.length === 0 ? (
            <p className="sl-reporte-vacio">No hay lotes vencidos pendientes de descartar.</p>
          ) : (
            <>
              <p className="sl-reporte-nota">
                No se pueden vender. Descártalos en la pestaña "Por vencer" para que queden en este reporte.
              </p>
              <div className="sl-tabla-caja">
                <table className="sl-tabla">
                  <thead>
                    <tr>
                      <th>Producto</th>
                      <th>Venció</th>
                      <th className="sl-num">Cantidad</th>
                      <th className="sl-num">Hace</th>
                    </tr>
                  </thead>
                  <tbody>
                    {reporte.vencidos_sin_descartar.map((l) => (
                      <tr key={l.lote_id}>
                        <td>{l.producto_nombre}</td>
                        <td>{fechaCorta(l.fecha_vencimiento)}</td>
                        <td className="sl-num">
                          {cantidadTexto(l.cantidad)} {l.unidad_medida.toLowerCase()}
                        </td>
                        <td className="sl-num">{Math.abs(l.dias_restantes)} d</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
