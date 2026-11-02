import { formatoPrecio } from '../utils/medidas';
import './SelectorPrecio.css';

// Módulo "3 precios de venta": en una línea del carrito, los precios del
// producto (P1, P2, P3) para que el vendedor elija cuál cobrar con un toque.
// `opciones`: [{ numero, precio }] (de opcionesDePrecio); `actual`: el
// precio de la línea ahora.
export default function SelectorPrecio({ opciones, actual, onElegir, nombre }) {
  if (!opciones || opciones.length < 2) return null;
  return (
    <div className="selector-precio" role="group" aria-label={`Precio de ${nombre}`}>
      {opciones.map((o) => {
        const elegido = Math.abs(Number(actual) - Number(o.precio)) < 0.005;
        return (
          <button
            key={o.numero}
            type="button"
            className={`selector-precio-opcion${elegido ? ' elegido' : ''}`}
            aria-pressed={elegido}
            onClick={() => !elegido && onElegir(Number(o.precio))}
            title={`Cobrar el Precio ${o.numero}`}
          >
            <span className="selector-precio-numero">P{o.numero}</span> S/ {formatoPrecio(o.precio)}
          </button>
        );
      })}
    </div>
  );
}
