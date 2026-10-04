// Elegir talla y color en el punto de venta (módulo "Tallas y colores").
// Cada botón es una talla/color con SU precio y SU stock; al tocarlo entra
// al carrito. Las agotadas se ven tachadas y no se pueden elegir.
import { useEffect } from 'react';
import { abreviaturaUnidad } from '../utils/unidades';
import './PantallaModulo.css';
import './SelectorTalla.css';

export default function SelectorTalla({ grupo, enCarrito = {}, onElegir, onCerrar }) {
  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  // Con colores: un bloque por color con sus tallas. Sin colores: un solo
  // bloque con las tallas. Sin tallas: un botón por color.
  const conTallas = grupo.tallas.length > 0;
  const bloques = conTallas && grupo.colores.length > 0
    ? grupo.colores.map((color) => ({
        titulo: color,
        variantes: grupo.variantes.filter((v) => (v.color || '') === color),
      }))
    : [{ titulo: null, variantes: grupo.variantes }];
  const mismoPrecio = grupo.precioMin === grupo.precioMax;

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pm-modal st-modal" role="dialog" aria-modal="true" aria-label={`Elegir talla de ${grupo.nombre}`}>
        <div className="pm-modal-cabecera">
          <h2>{grupo.nombre}</h2>
          <p>
            {conTallas ? 'Elige la talla' : 'Elige el color'}
            {mismoPrecio ? ` · S/ ${grupo.precioMin.toFixed(2)}` : ' · cada talla tiene su precio'}
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          {bloques.map((bloque) => (
            <div key={bloque.titulo || 'unico'} className="st-bloque">
              {bloque.titulo && <span className="st-color">{bloque.titulo}</span>}
              <div className="st-tallas">
                {bloque.variantes.map((v) => {
                  const controla = v.controla_stock !== false;
                  const yaPedidas = enCarrito[v.id] || 0;
                  const disponible = controla ? v.stock - yaPedidas : Infinity;
                  const agotada = controla && disponible <= 0;
                  return (
                    <button
                      key={v.id}
                      type="button"
                      className={`st-talla${agotada ? ' agotada' : ''}`}
                      disabled={agotada}
                      onClick={() => onElegir(v)}
                      aria-label={`${v.nombre}, S/ ${v.precio.toFixed(2)}${agotada ? ', agotada' : ''}`}
                    >
                      <span className="st-talla-nombre">{conTallas ? v.talla : v.color}</span>
                      <span className="st-talla-precio">S/ {v.precio.toFixed(2)}</span>
                      <span className="st-talla-stock">
                        {agotada
                          ? v.stock > 0
                            ? 'Ya en el carrito'
                            : 'Agotada'
                          : controla
                            ? `${v.stock} ${abreviaturaUnidad(v.unidad_medida)}`
                            : 'Disponible'}
                      </span>
                      {yaPedidas > 0 && <span className="st-talla-carrito">{yaPedidas}</span>}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton-secundario" onClick={onCerrar}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
