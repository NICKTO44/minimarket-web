import { useState } from 'react';
import { TASA_GENERAL, TASA_MYPE_RESTAURANTES } from '../../utils/igv';
import './SelectorTasaIgv.css';

/**
 * Tasa de IGV del negocio. Se aplica a todas sus ventas gravadas (los
 * precios siguen siendo precio final al público). El sistema no decide el
 * impuesto: aplica lo que el administrador elija aquí.
 */
export default function SelectorTasaIgv({ valor, onCambiar }) {
  const numero = Number(valor);
  // "Otra tasa" queda elegida mientras se escribe, aunque el número
  // coincida un momento con 18 o 10.5.
  const [otraElegida, setOtraElegida] = useState(false);
  const modo = otraElegida
    ? 'OTRA'
    : numero === TASA_GENERAL
      ? 'GENERAL'
      : numero === TASA_MYPE_RESTAURANTES
        ? 'MYPE'
        : 'OTRA';

  const elegir = (nuevoModo) => {
    setOtraElegida(nuevoModo === 'OTRA');
    if (nuevoModo === 'GENERAL') onCambiar(TASA_GENERAL);
    if (nuevoModo === 'MYPE') onCambiar(TASA_MYPE_RESTAURANTES);
  };

  const opcion = (id, titulo, detalle, extra = null) => (
    <div
      className={`tasa-opcion${modo === id ? ' activa' : ''}`}
      role="radio"
      aria-checked={modo === id}
      tabIndex={0}
      onClick={() => elegir(id)}
      onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && elegir(id)}
    >
      <span className="tasa-radio" aria-hidden="true" />
      <span className="tasa-texto">
        <strong>{titulo}</strong>
        <span>{detalle}</span>
        {extra}
      </span>
    </div>
  );

  return (
    <div className="cfg-campo tasa-igv">
      <label>Impuesto (IGV)</label>
      <p className="cfg-nota-moneda">
        La tasa se aplica a todas las ventas gravadas del negocio. Tus precios siguen siendo el precio final al
        público.
      </p>
      <div role="radiogroup" aria-label="Tasa de IGV">
        {opcion('GENERAL', '18 % — Tasa general', 'La de casi todos los negocios.')}
        {opcion(
          'MYPE',
          '10.5 % — MYPE de restaurantes y hoteles',
          'Solo si tu negocio está inscrito en el padrón de SUNAT para esta tasa.'
        )}
        {opcion(
          'OTRA',
          'Otra tasa',
          'Si la norma cambia (por ejemplo, 15 % en 2027).',
          modo === 'OTRA' && (
            <span className="tasa-otra">
              <input
                type="number"
                inputMode="decimal"
                min="0.1"
                max="30"
                step="0.1"
                value={valor}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => onCambiar(e.target.value)}
                aria-label="Otra tasa de IGV en porcentaje"
              />
              %
            </span>
          )
        )}
      </div>
      {modo !== 'GENERAL' && (
        <p className="tasa-aviso">
          <strong>Confírmalo con tu contador.</strong> Si usas una tasa distinta a 18 % sin estar inscrito en el padrón
          de SUNAT, tus comprobantes saldrán observados.
        </p>
      )}
    </div>
  );
}
