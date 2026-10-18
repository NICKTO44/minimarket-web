import { useEffect } from 'react';
import { FileCheck2 } from 'lucide-react';
import { fechaCorta } from '../../utils/formato';
import { etiquetaCondicion } from '../../utils/medicamentos';
import { nombreMetodo } from '../../utils/metodoPago';
import '../../components/CalculadoraPieTablar.css';
import './DetalleReceta.css';

const cantidadTexto = (n) => (Number.isInteger(n) ? n : Number(Number(n).toFixed(2)));
const ESTADO_SUNAT = { ACEPTADO: 'Aceptado por SUNAT', PENDIENTE: 'Pendiente de envío', RECHAZADO: 'Rechazado', ERROR: 'Con error' };

/** Una fila del detalle: no se muestra si no hay nada que mostrar (salvo `siempre`). */
function Dato({ etiqueta, valor, siempre = false }) {
  if (!valor && !siempre) return null;
  return (
    <div className="dr-dato">
      <dt>{etiqueta}</dt>
      <dd>{valor || '—'}</dd>
    </div>
  );
}

/**
 * Detalle de una venta con receta del Libro de recetas: los datos de la
 * receta tal como se anotaron, los medicamentos y la venta con su
 * comprobante.
 */
export default function DetalleReceta({ receta: r, onCerrar }) {
  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const sinDatos = ![r.medico_nombre, r.medico_colegiatura, r.paciente_nombre, r.paciente_documento, r.numero_receta, r.fecha_receta].some(Boolean);
  const anulada = r.estado_venta === 'CANCELADA';
  const comprobante = r.comprobante;

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pt-modal dr-modal" role="dialog" aria-modal="true" aria-label={`Receta de la venta ${r.folio || r.venta_id}`}>
        <header className="pt-cabecera">
          <span className="pt-icono">
            <FileCheck2 size={18} />
          </span>
          <div>
            <h2>{r.retenida ? 'Receta retenida' : 'Venta con receta'}</h2>
            <p>
              {r.folio || `Venta #${r.venta_id}`} · {fechaCorta(r.fecha_hora)} {r.fecha_hora.slice(11, 16)}
            </p>
          </div>
        </header>

        {anulada && (
          <p className="dr-aviso dr-aviso-anulada">
            Venta anulada{r.motivo_anulacion ? `: ${r.motivo_anulacion}` : ''}. Los medicamentos volvieron al stock.
          </p>
        )}

        <section className="dr-seccion">
          <h3>Receta</h3>
          {sinDatos ? (
            <p className="dr-aviso">
              Al cobrar se confirmó que el cliente presentó la receta, pero no se anotaron sus datos (en "con receta" son
              opcionales; en "receta retenida" son obligatorios).
            </p>
          ) : (
            <dl className="dr-datos">
              <Dato etiqueta="N.° de receta" valor={r.numero_receta} siempre />
              <Dato etiqueta="Fecha de la receta" valor={r.fecha_receta && fechaCorta(r.fecha_receta)} siempre />
              <Dato etiqueta="Médico" valor={r.medico_nombre} siempre />
              <Dato etiqueta="CMP" valor={r.medico_colegiatura} siempre />
              <Dato etiqueta="Paciente" valor={r.paciente_nombre} siempre />
              <Dato etiqueta="DNI / documento" valor={r.paciente_documento} siempre />
            </dl>
          )}
          {r.retenida && !anulada && <p className="dr-nota">La receta física queda archivada en el establecimiento.</p>}
        </section>

        <section className="dr-seccion">
          <h3>Medicamentos con receta</h3>
          <ul className="dr-productos">
            {r.productos.map((p, i) => (
              <li key={i}>
                <span>
                  {cantidadTexto(p.cantidad)} × {p.producto_nombre}
                </span>
                <small className={p.condicion_venta === 'RECETA_RETENIDA' ? 'dr-retenida' : ''}>{etiquetaCondicion(p.condicion_venta)}</small>
              </li>
            ))}
          </ul>
        </section>

        <section className="dr-seccion">
          <h3>Venta</h3>
          <dl className="dr-datos">
            <Dato etiqueta="Total de la venta" valor={r.total != null ? `S/ ${Number(r.total).toFixed(2)}` : null} />
            <Dato etiqueta="Pago" valor={r.metodo_pago && nombreMetodo(r.metodo_pago)} />
            <Dato etiqueta="Cliente" valor={[r.cliente_nombre, r.cliente_documento].filter(Boolean).join(' · ')} />
            <Dato etiqueta="Atendió" valor={r.usuario_nombre} siempre />
            <Dato
              etiqueta="Comprobante"
              valor={
                comprobante
                  ? `${comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} ${comprobante.serie || ''}-${String(comprobante.numero ?? '').padStart(6, '0')} · ${
                      ESTADO_SUNAT[comprobante.estado] || comprobante.estado
                    }`
                  : 'Sin comprobante electrónico (nota de venta)'
              }
              siempre
            />
          </dl>
          {comprobante && <p className="dr-nota">Para verlo o reimprimirlo, búscalo en Comprobantes.</p>}
        </section>

        <div className="pt-acciones dr-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
