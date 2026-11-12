import { useState } from 'react';
import { TIPOS_DESCUENTO } from '../utils/descuentoVenta';
import './DescuentoVenta.css';

/**
 * Editor chico de un descuento: % o S/ y el valor. "Aplicar" lo deja,
 * "Quitar" lo borra (solo si ya había uno).
 */
export default function EditorDescuento({ actual, maximo, onAplicar, onQuitar, onCerrar }) {
  const [tipo, setTipo] = useState(actual?.tipo || '%');
  const [valor, setValor] = useState(actual ? String(actual.valor) : '');
  const n = parseFloat(valor);
  const error =
    valor === '' || !(n > 0)
      ? null
      : tipo === '%' && n > 100
        ? 'No puede pasar de 100%.'
        : tipo === 'S/' && maximo != null && n > maximo
          ? `No puede pasar de S/ ${maximo.toFixed(2)}.`
          : null;
  const listo = n > 0 && !error;
  const aplicar = () => listo && onAplicar({ tipo, valor: Math.round(n * 100) / 100 });

  return (
    <div className="desc-editor" onClick={(e) => e.stopPropagation()}>
      <div className="desc-editor-fila">
        <div className="desc-editor-tipos" role="group" aria-label="Tipo de descuento">
          {TIPOS_DESCUENTO.map((t) => (
            <button key={t} type="button" className={tipo === t ? 'activo' : ''} onClick={() => setTipo(t)}>
              {t}
            </button>
          ))}
        </div>
        <input
          type="number"
          inputMode="decimal"
          min="0"
          step="0.01"
          autoFocus
          placeholder={tipo === '%' ? 'Ej. 10' : 'Ej. 5.00'}
          aria-label="Valor del descuento"
          value={valor}
          onChange={(e) => setValor(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') aplicar();
            if (e.key === 'Escape') onCerrar();
          }}
        />
        <button type="button" className="desc-editor-aplicar" disabled={!listo} onClick={aplicar}>
          Aplicar
        </button>
      </div>
      {error && <p className="desc-editor-error">{error}</p>}
      <div className="desc-editor-pie">
        {actual && (
          <button type="button" className="desc-editor-quitar" onClick={onQuitar}>
            Quitar descuento
          </button>
        )}
        <button type="button" className="desc-editor-cerrar" onClick={onCerrar}>
          Cancelar
        </button>
      </div>
    </div>
  );
}
