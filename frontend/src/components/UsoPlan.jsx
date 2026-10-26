import { useEffect, useState } from 'react';
import { api } from '../api/api';
import './UsoPlan.css';

const MESES = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'set', 'oct', 'nov', 'dic'];

function fecha(texto) {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(texto || '');
  return m ? `${Number(m[3])} ${MESES[Number(m[2]) - 1]}` : '—';
}

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;

/**
 * Boletas y facturas que el negocio lleva en su ciclo frente a su plan.
 * Si pasa del límite no se le corta: se le avisa lo que se sumará a su
 * siguiente mensualidad.
 */
export default function UsoPlan() {
  const [uso, setUso] = useState(null);
  const [error, setError] = useState('');

  useEffect(() => {
    let vivo = true;
    api
      .suscripcionUso()
      .then((u) => vivo && setUso(u))
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, []);

  if (error) return <p className="uso-plan-error">No se pudo leer tu consumo de comprobantes: {error}</p>;
  if (!uso) return <p className="uso-plan-cargando">Cargando tu consumo de comprobantes...</p>;

  const tono = uso.extra > 0 ? 'rojo' : uso.porcentaje >= 80 ? 'ambar' : 'verde';
  const quedan = Math.max(0, uso.limite - uso.usados);

  return (
    <section className="uso-plan">
      <div className="uso-plan-cabecera">
        <div>
          <h3>Comprobantes de este ciclo</h3>
          <p>
            {uso.plan} · {soles(uso.precio)} al mes · incluye {uso.limite} boletas y facturas
          </p>
        </div>
        <span className="uso-plan-ciclo">
          {fecha(uso.ciclo_inicio)} – {fecha(uso.ciclo_fin)}
        </span>
      </div>

      <p className="uso-plan-cifras">
        <strong className={`uso-plan-texto-${tono}`}>{uso.usados}</strong> de {uso.limite}
      </p>
      <div
        className="uso-plan-barra"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={uso.limite}
        aria-valuenow={uso.usados}
        aria-label="Comprobantes usados en el ciclo"
      >
        <span className={`uso-plan-relleno uso-plan-${tono}`} style={{ width: `${Math.min(100, uso.porcentaje)}%` }} />
      </div>
      <p className="uso-plan-detalle">
        {uso.boletas} boleta{uso.boletas === 1 ? '' : 's'} · {uso.facturas} factura{uso.facturas === 1 ? '' : 's'}
        {uso.extra === 0 && ` · te quedan ${quedan}`}
      </p>
      {uso.incluye_prueba && (
        <p className="uso-plan-nota">
          Tu primer mes incluye los comprobantes que emitiste en la prueba gratis (desde {fecha(uso.ciclo_inicio)}). Desde el
          próximo ciclo se cuenta solo el mes.
        </p>
      )}

      {uso.extra > 0 ? (
        <p className="uso-plan-aviso uso-plan-aviso-rojo">
          Superaste tu plan en {uso.extra} comprobante{uso.extra === 1 ? '' : 's'}. Puedes seguir emitiendo con normalidad; en tu siguiente
          mensualidad se sumarán {soles(uso.monto_extra)} ({soles(uso.precio_extra)} por cada comprobante extra), total{' '}
          {soles(uso.total_siguiente)}.
        </p>
      ) : (
        uso.porcentaje >= 80 && (
          <p className="uso-plan-aviso uso-plan-aviso-ambar">
            Estás cerca del límite de tu plan. Si lo pasas no se corta: cada comprobante extra cuesta {soles(uso.precio_extra)} y se suma
            a tu siguiente mensualidad.
          </p>
        )
      )}

      <p className="uso-plan-nota">
        Cuentan las boletas y facturas enviadas a SUNAT, también las anuladas. Las notas de crédito y las guías no cuentan.
      </p>
    </section>
  );
}
