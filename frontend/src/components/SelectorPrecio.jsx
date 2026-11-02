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

// En la tarjeta del producto (antes de agregarlo): los 3 precios a la vista.
// Tocar uno agrega el producto con ese precio; tocar el resto de la tarjeta
// lo agrega con el Precio 1, como siempre. Va dentro del <button> de la
// tarjeta, por eso cada precio es un span con role="button" y no otro botón.
export function PreciosTarjeta({ opciones, onElegir, nombre }) {
  if (!opciones || opciones.length < 2) return null;
  const elegir = (e, precio) => {
    e.stopPropagation();
    e.preventDefault();
    onElegir(Number(precio));
  };
  return (
    <span className="precios-tarjeta" role="group" aria-label={`Precios de ${nombre}`}>
      {opciones.map((o) => (
        <span
          key={o.numero}
          role="button"
          tabIndex={0}
          className="precios-tarjeta-fila"
          title={`Agregar con el Precio ${o.numero}`}
          onClick={(e) => elegir(e, o.precio)}
          onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && elegir(e, o.precio)}
        >
          <span className="precios-tarjeta-numero">P{o.numero}</span>
          <span className="precios-tarjeta-monto">S/ {formatoPrecio(o.precio)}</span>
        </span>
      ))}
    </span>
  );
}
