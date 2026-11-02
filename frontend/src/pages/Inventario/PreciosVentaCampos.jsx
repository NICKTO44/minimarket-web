import './PreciosVentaCampos.css';

// Sección "Precio 2 y Precio 3" del formulario de producto (módulo "3
// precios de venta"). El Precio 1 es el "Precio de venta" de arriba. Vacío
// o 0 = el producto no tiene ese precio.
export default function PreciosVentaCampos({ campos, onCambiar }) {
  const cambiar = (campo, valor) => onCambiar({ ...campos, [campo]: valor });
  return (
    <div className="inv-ficha-caja">
      <p className="inv-lote-titulo">Precio 2 y Precio 3</p>
      <p className="inv-lote-nota">
        Opcional. El Precio 1 es el precio de venta. Al vender, el vendedor ve los tres y elige cuál cobrar.
      </p>
      <div className="inv-ficha-campos">
        <div className="inv-campo">
          <label>Precio 2 (S/)</label>
          <input
            type="number"
            min="0"
            step="0.01"
            inputMode="decimal"
            value={campos.precio_2}
            placeholder="Ej. precio para talleres"
            onChange={(e) => cambiar('precio_2', e.target.value)}
          />
        </div>
        <div className="inv-campo">
          <label>Precio 3 (S/)</label>
          <input
            type="number"
            min="0"
            step="0.01"
            inputMode="decimal"
            value={campos.precio_3}
            placeholder="Ej. precio por mayor"
            onChange={(e) => cambiar('precio_3', e.target.value)}
          />
        </div>
      </div>
    </div>
  );
}

// Columna de precios del listado de productos (módulo "3 precios de venta"):
// P1 / P2 / P3 uno debajo del otro. Sin precios extra, solo el de siempre.
export function PreciosCelda({ precio, extra }) {
  if (!extra) return <>S/ {Number(precio).toFixed(2)}</>;
  const filas = [
    ['P1', precio],
    ['P2', extra.precio_2],
    ['P3', extra.precio_3],
  ].filter(([, valor]) => Number(valor) > 0);
  return (
    <span className="inv-precios-tres">
      {filas.map(([etiqueta, valor]) => (
        <span key={etiqueta} className="inv-precios-tres-fila">
          <span className="inv-precios-tres-etiqueta">{etiqueta}</span> S/ {Number(valor).toFixed(2)}
        </span>
      ))}
    </span>
  );
}
