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
