import { useEffect } from 'react';
import { Boxes } from 'lucide-react';
import { formatoCantidad, formatoPrecio } from '../utils/medidas';
import { tienePrecioMayor } from '../utils/presentaciones';
import { abreviaturaUnidad, etiquetaUnidad } from '../utils/unidades';
import './CalculadoraPieTablar.css';
import './ElegirPresentacion.css';

/**
 * Producto con presentaciones (módulo PRESENTACIONES): antes de agregarlo se
 * elige cómo se lleva. "Por metro" sigue el camino de siempre; una
 * presentación ("Rollo x 100 m") entra en metros con su precio.
 *
 * Con `mayorElegible` (módulo MAYOR_ELEGIBLE), lo que tiene precio por mayor
 * sale dos veces: "por menor" y "por mayor"; el vendedor elige y el de mayor
 * se cobra desde la primera.
 */
export default function ElegirPresentacion({
  producto,
  presentaciones,
  escalas = [],
  disponible,
  onSuelto,
  onPresentacion,
  mayorElegible = false,
  onPorMayor,
  onCerrar,
}) {
  const unidad = abreviaturaUnidad(producto.unidad_medida);
  const controlaStock = producto.controla_stock !== false;
  // Escalas "por mayor" del producto (no las que salen de una presentación).
  const escalasMayor = escalas.filter((e) => e.origen === 'mayor');

  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pt-modal" role="dialog" aria-modal="true" aria-label={`Cómo se vende ${producto.nombre}`}>
        <header className="pt-cabecera">
          <span className="pt-icono">
            <Boxes size={18} />
          </span>
          <div>
            <h2>{producto.nombre}</h2>
            <p>
              ¿Cómo lo lleva?
              {controlaStock && ` · quedan ${formatoCantidad(Math.max(0, disponible))} ${unidad}`}
            </p>
          </div>
        </header>

        <div className="ep-opciones">
          <button type="button" className="ep-opcion" onClick={onSuelto} autoFocus>
            <span className="ep-nombre">
              Por {etiquetaUnidad(producto.unidad_medida).toLowerCase()}
              {mayorElegible && escalasMayor.length > 0 && ' · por menor'}
              {escalas.length > 0 && !(mayorElegible && escalasMayor.length > 0) && (
                <small>
                  {escalas.map((e) => `desde ${formatoCantidad(e.desde)} ${unidad}: S/ ${formatoPrecio(e.precio)}`).join(' · ')}
                </small>
              )}
            </span>
            <span className="ep-precio">S/ {formatoPrecio(producto.precio)}</span>
          </button>
          {mayorElegible &&
            escalasMayor.map((e) => (
              <button type="button" key={`mayor-${e.desde}`} className="ep-opcion ep-opcion-mayor" onClick={() => onPorMayor?.(e)}>
                <span className="ep-nombre">
                  Por {etiquetaUnidad(producto.unidad_medida).toLowerCase()} · por mayor
                  <small>
                    sin llegar a {formatoCantidad(e.desde)} {unidad}
                  </small>
                </span>
                <span className="ep-precio">S/ {formatoPrecio(e.precio)}</span>
              </button>
            ))}
          {presentaciones.flatMap((p) => {
            const alcanza = !controlaStock || disponible + 1e-6 >= p.factor;
            const conMayor = mayorElegible && tienePrecioMayor(p);
            const normal = (
              <button
                type="button"
                key={p.id}
                className="ep-opcion"
                disabled={!alcanza}
                onClick={() => onPresentacion(p)}
                title={alcanza ? undefined : 'No alcanza el stock'}
              >
                <span className="ep-nombre">
                  {p.nombre}
                  {conMayor && ' · por menor'}
                  <small>
                    {formatoCantidad(p.factor)} {unidad}
                    {!conMayor && p.mayor_desde > 0 && p.precio_mayor > 0 && ` · desde ${p.mayor_desde}: S/ ${Number(p.precio_mayor).toFixed(2)} c/u`}
                    {conMayor && ` · desde ${p.mayor_desde} cajas baja solo a S/ ${Number(p.precio_mayor).toFixed(2)}`}
                    {!alcanza && ' · no alcanza el stock'}
                  </small>
                </span>
                <span className="ep-precio">S/ {Number(p.precio).toFixed(2)}</span>
              </button>
            );
            if (!conMayor) return [normal];
            return [
              normal,
              <button
                type="button"
                key={`${p.id}-mayor`}
                className="ep-opcion ep-opcion-mayor"
                disabled={!alcanza}
                onClick={() => onPresentacion(p, true)}
              >
                <span className="ep-nombre">
                  {p.nombre} · por mayor
                  <small>
                    {formatoCantidad(p.factor)} {unidad}
                    {!alcanza && ' · no alcanza el stock'}
                  </small>
                </span>
                <span className="ep-precio">S/ {Number(p.precio_mayor).toFixed(2)}</span>
              </button>,
            ];
          })}
        </div>

        <div className="pt-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            Cancelar
          </button>
        </div>
      </div>
    </div>
  );
}
