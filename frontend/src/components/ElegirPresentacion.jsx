import { useEffect } from 'react';
import { Boxes } from 'lucide-react';
import { formatoCantidad, formatoPrecio } from '../utils/medidas';
import { abreviaturaUnidad, etiquetaUnidad } from '../utils/unidades';
import './CalculadoraPieTablar.css';
import './ElegirPresentacion.css';

/**
 * Producto con presentaciones (módulo PRESENTACIONES): antes de agregarlo se
 * elige cómo se lleva. "Por metro" sigue el camino de siempre; una
 * presentación ("Rollo x 100 m") entra en metros con su precio.
 */
export default function ElegirPresentacion({ producto, presentaciones, escalas = [], disponible, onSuelto, onPresentacion, onCerrar }) {
  const unidad = abreviaturaUnidad(producto.unidad_medida);
  const controlaStock = producto.controla_stock !== false;

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
              {escalas.length > 0 && (
                <small>
                  {escalas.map((e) => `desde ${formatoCantidad(e.desde)} ${unidad}: S/ ${formatoPrecio(e.precio)}`).join(' · ')}
                </small>
              )}
            </span>
            <span className="ep-precio">S/ {formatoPrecio(producto.precio)}</span>
          </button>
          {presentaciones.map((p) => {
            const alcanza = !controlaStock || disponible + 1e-6 >= p.factor;
            return (
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
                  <small>
                    {formatoCantidad(p.factor)} {unidad}
                    {!alcanza && ' · no alcanza el stock'}
                  </small>
                </span>
                <span className="ep-precio">S/ {Number(p.precio).toFixed(2)}</span>
              </button>
            );
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
