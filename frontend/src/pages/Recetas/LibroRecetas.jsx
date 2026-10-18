import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import { TIPO_XLSX, escribirXlsx } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { etiquetaCondicion } from '../../utils/medicamentos';
import DetalleReceta from './DetalleReceta';
import '../StockLotes/StockLotes.css';
import './LibroRecetas.css';

const cantidadTexto = (n) => (Number.isInteger(n) ? n : Number(Number(n).toFixed(2)));
const productosTexto = (r) => r.productos.map((p) => `${cantidadTexto(p.cantidad)} × ${p.producto_nombre}`).join('; ');

// Libro de recetas (módulo RECETAS, solo administrador): cada venta de
// medicamentos con receta, con los datos que se anotaron al cobrar. Las de
// receta retenida van marcadas. Sirve para revisar o para una inspección.
export default function LibroRecetas() {
  const [desde, setDesde] = useState(`${hoyLima().slice(0, 7)}-01`);
  const [hasta, setHasta] = useState(hoyLima());
  const [soloRetenidas, setSoloRetenidas] = useState(false);
  const [libro, setLibro] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [abierta, setAbierta] = useState(null);

  useEffect(() => {
    setCargando(true);
    setError(null);
    api
      .recetasLibro(desde, hasta)
      .then(setLibro)
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, [desde, hasta]);

  const recetas = (libro?.recetas || []).filter((r) => !soloRetenidas || r.retenida);

  const exportar = () => {
    const filas = [
      [
        'Fecha y hora', 'Venta', 'Estado', 'Tipo', 'Medicamentos', 'Médico', 'CMP', 'Paciente', 'Documento',
        'N.° de receta', 'Fecha de receta', 'Atendió', 'Comprobante',
      ],
      ...recetas.map((r) => [
        r.fecha_hora,
        r.folio || `#${r.venta_id}`,
        r.estado_venta === 'CANCELADA' ? 'Anulada' : 'Completada',
        r.retenida ? 'Receta retenida' : 'Con receta',
        productosTexto(r),
        r.medico_nombre || '',
        r.medico_colegiatura || '',
        r.paciente_nombre || '',
        r.paciente_documento || '',
        r.numero_receta || '',
        r.fecha_receta || '',
        r.usuario_nombre || '',
        r.comprobante ? `${r.comprobante.serie || ''}-${String(r.comprobante.numero ?? '').padStart(6, '0')}` : '',
      ]),
    ];
    descargarArchivo(
      escribirXlsx(filas, { hoja: 'Recetas', anchos: [20, 20, 12, 16, 48, 28, 12, 28, 12, 14, 14, 22, 16] }),
      `Libro-de-recetas-${desde}-a-${hasta}.xlsx`,
      { tipo: TIPO_XLSX }
    );
  };

  return (
    <div className="sl-layout">
      <div className="sl-header">
        <h1>Libro de recetas</h1>
      </div>
      <p className="sl-subtitulo">
        Ventas de medicamentos que piden receta, con los datos anotados al cobrar.
      </p>

      <div className="sl-reporte-filtros">
        <label>
          Desde
          <input type="date" value={desde} max={hasta} onChange={(e) => e.target.value && setDesde(e.target.value)} />
        </label>
        <label>
          Hasta
          <input type="date" value={hasta} min={desde} max={hoyLima()} onChange={(e) => e.target.value && setHasta(e.target.value)} />
        </label>
        <label className="lr-check">
          <input type="checkbox" checked={soloRetenidas} onChange={(e) => setSoloRetenidas(e.target.checked)} />
          Solo receta retenida
        </label>
        <button type="button" className="sl-boton-exportar" onClick={exportar} disabled={recetas.length === 0}>
          Descargar Excel
        </button>
      </div>

      {error && <p className="sl-mensaje sl-mensaje-error">{error}</p>}

      {cargando && !libro ? (
        <p className="sl-cargando">Cargando...</p>
      ) : libro && !libro.disponible ? (
        <p className="sl-mensaje sl-mensaje-error">El libro de recetas se está preparando en este negocio. Intenta en unos minutos.</p>
      ) : recetas.length === 0 ? (
        <p className="sl-reporte-vacio">
          No hay ventas con receta del {fechaCorta(desde)} al {fechaCorta(hasta)}
          {soloRetenidas ? ' de receta retenida' : ''}.
        </p>
      ) : (
        <div className="lr-lista">
          {recetas.map((r) => (
            <article
              key={r.id}
              className={`lr-receta${r.retenida ? ' lr-retenida' : ''}${r.estado_venta === 'CANCELADA' ? ' lr-anulada' : ''}`}
              role="button"
              tabIndex={0}
              onClick={() => setAbierta(r)}
              onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), setAbierta(r))}
              aria-label={`Ver el detalle de la receta de ${r.folio || `la venta ${r.venta_id}`}`}
            >
              <header>
                <span className="lr-folio">{r.folio || `Venta #${r.venta_id}`}</span>
                <span className="lr-fecha">
                  {fechaCorta(r.fecha_hora)} {r.fecha_hora.slice(11, 16)}
                </span>
                {r.retenida && <span className="lr-etiqueta lr-etiqueta-retenida">Receta retenida</span>}
                {r.estado_venta === 'CANCELADA' && <span className="lr-etiqueta">Venta anulada</span>}
                <span className="lr-ver">Ver detalle ›</span>
              </header>
              <ul className="lr-productos">
                {r.productos.map((p, i) => (
                  <li key={i}>
                    {cantidadTexto(p.cantidad)} × {p.producto_nombre}
                    <small>{etiquetaCondicion(p.condicion_venta)}</small>
                  </li>
                ))}
              </ul>
              <dl className="lr-datos">
                <div>
                  <dt>Médico</dt>
                  <dd>
                    {r.medico_nombre || '—'}
                    {r.medico_colegiatura && ` · CMP ${r.medico_colegiatura}`}
                  </dd>
                </div>
                <div>
                  <dt>Paciente</dt>
                  <dd>
                    {r.paciente_nombre || '—'}
                    {r.paciente_documento && ` · ${r.paciente_documento}`}
                  </dd>
                </div>
                <div>
                  <dt>Receta</dt>
                  <dd>
                    {r.numero_receta ? `N.° ${r.numero_receta}` : '—'}
                    {r.fecha_receta && ` · ${fechaCorta(r.fecha_receta)}`}
                  </dd>
                </div>
                <div>
                  <dt>Atendió</dt>
                  <dd>{r.usuario_nombre || '—'}</dd>
                </div>
              </dl>
            </article>
          ))}
        </div>
      )}
      {abierta && <DetalleReceta receta={abierta} onCerrar={() => setAbierta(null)} />}
    </div>
  );
}
