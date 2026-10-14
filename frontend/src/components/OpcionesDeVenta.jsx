import { Plus, Trash2 } from 'lucide-react';
import { abreviaturaUnidad } from '../utils/unidades';
import { numeroDeTexto as numero, precioUnitarioDe } from '../utils/presentaciones';
import { formatoPrecio } from '../utils/medidas';
import './OpcionesDeVenta.css';

/**
 * Sección del formulario de producto: presentaciones (módulo PRESENTACIONES)
 * y precio por mayor (módulo PRECIO_MAYOR). Cada lista se muestra solo con
 * su módulo encendido.
 */
export default function OpcionesDeVenta({ opciones, onCambiar, unidad, precioNormal, conPresentaciones, conPrecioMayor }) {
  const abrev = abreviaturaUnidad(unidad);
  const cambiarFila = (lista, i, campo, valor) =>
    onCambiar({ ...opciones, [lista]: opciones[lista].map((f, j) => (j === i ? { ...f, [campo]: valor } : f)) });
  const agregar = (lista, fila) => onCambiar({ ...opciones, [lista]: [...opciones[lista], fila] });
  const quitar = (lista, i) => onCambiar({ ...opciones, [lista]: opciones[lista].filter((_, j) => j !== i) });

  return (
    <div className="odv">
      {conPresentaciones && (
        <section className="odv-seccion">
          <h3>Presentaciones</h3>
          <p className="odv-ayuda">
            Otras formas de vender este producto, con su precio. El stock sigue en {abrev}: vender 1 rollo de 100 {abrev}{' '}
            descuenta 100 {abrev}.
          </p>
          {opciones.presentaciones.map((f, i) => {
            const factor = numero(f.factor);
            const precio = numero(f.precio);
            return (
              <div className="odv-fila" key={i}>
                <label className="odv-campo odv-campo-nombre">
                  <span>Nombre</span>
                  <input placeholder="Rollo x 100 m" maxLength={60} value={f.nombre} onChange={(e) => cambiarFila('presentaciones', i, 'nombre', e.target.value)} />
                </label>
                <label className="odv-campo">
                  <span>Lleva ({abrev})</span>
                  <input inputMode="decimal" placeholder="100" value={f.factor} onChange={(e) => cambiarFila('presentaciones', i, 'factor', e.target.value)} />
                </label>
                <label className="odv-campo">
                  <span>Precio S/</span>
                  <input inputMode="decimal" placeholder="85.00" value={f.precio} onChange={(e) => cambiarFila('presentaciones', i, 'precio', e.target.value)} />
                </label>
                <button type="button" className="odv-quitar" onClick={() => quitar('presentaciones', i)} aria-label="Quitar presentación">
                  <Trash2 size={16} />
                </button>
                {factor > 0 && precio > 0 && (
                  <p className="odv-nota">
                    Sale a S/ {formatoPrecio(precioUnitarioDe(precio, factor))} por {abrev}
                  </p>
                )}
              </div>
            );
          })}
          <button type="button" className="odv-agregar" onClick={() => agregar('presentaciones', { nombre: '', factor: '', precio: '' })}>
            <Plus size={15} /> Agregar presentación
          </button>
        </section>
      )}

      {conPrecioMayor && (
        <section className="odv-seccion">
          <h3>Precio por mayor</h3>
          <p className="odv-ayuda">
            Precio por {abrev} cuando el cliente lleva esa cantidad o más
            {precioNormal > 0 ? ` (el normal es S/ ${formatoPrecio(precioNormal)})` : ''}. Se aplica solo al vender.
          </p>
          {opciones.precios_mayor.map((f, i) => (
            <div className="odv-fila odv-fila-mayor" key={i}>
              <label className="odv-campo">
                <span>Desde ({abrev})</span>
                <input inputMode="decimal" placeholder="50" value={f.desde} onChange={(e) => cambiarFila('precios_mayor', i, 'desde', e.target.value)} />
              </label>
              <label className="odv-campo">
                <span>Precio por {abrev} S/</span>
                <input inputMode="decimal" placeholder="0.80" value={f.precio} onChange={(e) => cambiarFila('precios_mayor', i, 'precio', e.target.value)} />
              </label>
              <button type="button" className="odv-quitar" onClick={() => quitar('precios_mayor', i)} aria-label="Quitar precio por mayor">
                <Trash2 size={16} />
              </button>
            </div>
          ))}
          <button type="button" className="odv-agregar" onClick={() => agregar('precios_mayor', { desde: '', precio: '' })}>
            <Plus size={15} /> Agregar precio por mayor
          </button>
        </section>
      )}
    </div>
  );
}
