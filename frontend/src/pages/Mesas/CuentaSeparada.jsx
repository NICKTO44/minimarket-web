import { useEffect, useState } from 'react';
import { Check, Minus, Plus, ReceiptText } from 'lucide-react';

/**
 * Cuentas separadas: un solo pedido y cada uno paga lo suyo. Se marca qué
 * paga esta persona (de una línea de varios iguales, cuántos) y se cobra
 * solo eso; la mesa sigue abierta con el resto hasta la última cuenta.
 */
export default function CuentaSeparada({ items, ocupado, onPrecuenta, onCobrar, onCerrar }) {
  // item.id -> cuántos paga esta persona.
  const [elegidos, setElegidos] = useState({});

  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const poner = (item, n) => setElegidos((a) => ({ ...a, [item.id]: Math.max(0, Math.min(item.cantidad, n)) }));
  const seleccion = items
    .filter((i) => (elegidos[i.id] || 0) > 0)
    .map((i) => ({ item: i, cantidad: elegidos[i.id] }));
  const total = seleccion.reduce((s, x) => s + x.cantidad * x.item.precio_unitario, 0);
  const totalMesa = items.reduce((s, i) => s + i.subtotal, 0);
  const queda = Math.max(0, totalMesa - total);

  return (
    <div className="mesas-modal-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="mesas-modal" role="dialog" aria-modal="true" aria-labelledby="cs-titulo">
        <h2 id="cs-titulo">Cuentas separadas</h2>
        <p className="mesas-modal-mensaje">
          Marca lo que paga esta persona y cóbralo. La mesa sigue abierta con el resto para la siguiente cuenta.
        </p>

        <div className="cs-lista">
          {items.map((item) => {
            const n = elegidos[item.id] || 0;
            const entero = Number.isInteger(item.cantidad);
            return (
              <div key={item.id} className={`cs-fila${n > 0 ? ' elegida' : ''}`}>
                <div className="cs-info">
                  <span className="cs-nombre">
                    {item.cantidad > 1 && <span className="cs-cant">{item.cantidad}×</span>}
                    {item.nombre_producto}
                  </span>
                  {item.opciones && <span className="pedido-item-opciones">{item.opciones}</span>}
                  {item.nota && <span className="pedido-item-nota">“{item.nota}”</span>}
                  <span className="pedido-item-precio">S/ {item.precio_unitario.toFixed(2)} c/u</span>
                </div>
                {item.cantidad > 1 && entero ? (
                  <div className="pedido-item-controles">
                    <button type="button" onClick={() => poner(item, n - 1)} disabled={n === 0} aria-label="Menos">
                      <Minus size={15} />
                    </button>
                    <span>
                      {n}
                      <small className="cs-de"> de {item.cantidad}</small>
                    </span>
                    <button type="button" onClick={() => poner(item, n + 1)} disabled={n >= item.cantidad} aria-label="Más">
                      <Plus size={15} />
                    </button>
                  </div>
                ) : (
                  <button
                    type="button"
                    className={`cs-marca${n > 0 ? ' activa' : ''}`}
                    onClick={() => poner(item, n > 0 ? 0 : item.cantidad)}
                    aria-pressed={n > 0}
                    aria-label={n > 0 ? 'Quitar de esta cuenta' : 'Paga esta persona'}
                  >
                    {n > 0 && <Check size={16} />}
                  </button>
                )}
              </div>
            );
          })}
        </div>

        <div className="cs-total">
          <span>
            Paga esta persona
            <small>Queda en la mesa: S/ {queda.toFixed(2)}</small>
          </span>
          <strong>S/ {total.toFixed(2)}</strong>
        </div>

        <div className="mesas-modal-acciones cs-acciones">
          <button type="button" className="mesas-boton-secundario" onClick={onCerrar}>
            Cancelar
          </button>
          <button
            type="button"
            className="mesas-boton-secundario"
            onClick={() => onPrecuenta(seleccion)}
            disabled={seleccion.length === 0}
          >
            <ReceiptText size={15} /> Precuenta
          </button>
          <button
            type="button"
            className="mesas-boton-primario"
            onClick={() => onCobrar(seleccion)}
            disabled={ocupado || seleccion.length === 0}
          >
            Cobrar S/ {total.toFixed(2)}
          </button>
        </div>
      </div>
    </div>
  );
}
