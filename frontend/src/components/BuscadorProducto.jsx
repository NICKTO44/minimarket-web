import { useEffect, useMemo, useRef, useState } from 'react';
import { MAXIMO_VISIBLES, buscarProductos } from '../utils/buscarProductos';
import './BuscadorProducto.css';

/**
 * Buscador de un producto del inventario: se escribe nombre o código y salen
 * los que coinciden; con ▾ (o la flecha abajo) se ve la lista completa.
 * Flechas ↑ ↓ para moverse, Enter para elegir, Esc para cerrar.
 */
export default function BuscadorProducto({ productos, valor, onElegir, placeholder = 'Buscar producto por nombre o código…' }) {
  const elegido = useMemo(() => productos.find((p) => String(p.id) === String(valor)) || null, [productos, valor]);
  const [texto, setTexto] = useState('');
  const [abierto, setAbierto] = useState(false);
  const [activo, setActivo] = useState(0);
  const caja = useRef(null);
  const lista = useRef(null);
  const entrada = useRef(null);

  const resultados = useMemo(() => buscarProductos(productos, texto), [productos, texto]);
  const visibles = resultados.slice(0, MAXIMO_VISIBLES);

  // Cerrar al hacer clic fuera.
  useEffect(() => {
    if (!abierto) return undefined;
    const fuera = (e) => caja.current && !caja.current.contains(e.target) && setAbierto(false);
    document.addEventListener('mousedown', fuera);
    return () => document.removeEventListener('mousedown', fuera);
  }, [abierto]);

  // Que la opción marcada con las flechas se vea.
  useEffect(() => {
    lista.current?.children[activo]?.scrollIntoView?.({ block: 'nearest' });
  }, [activo]);

  const elegir = (p) => {
    onElegir(p ? String(p.id) : '');
    setTexto('');
    setAbierto(false);
  };

  const teclas = (e) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (!abierto) setAbierto(true);
      else setActivo((a) => Math.min(a + 1, visibles.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActivo((a) => Math.max(a - 1, 0));
    } else if (e.key === 'Enter') {
      if (abierto && visibles[activo]) {
        e.preventDefault();
        elegir(visibles[activo]);
      }
    } else if (e.key === 'Escape') {
      setAbierto(false);
    }
  };

  const nombreDe = (p) => (p.codigo ? `${p.codigo} — ${p.nombre}` : p.nombre);

  return (
    <div className="bp" ref={caja}>
      <div className={`bp-campo${elegido && !abierto ? ' bp-con-elegido' : ''}`}>
        <span className="bp-lupa" aria-hidden="true">🔍</span>
        <input
          ref={entrada}
          type="text"
          role="combobox"
          aria-expanded={abierto}
          aria-label="Producto existente"
          placeholder={elegido ? nombreDe(elegido) : placeholder}
          value={abierto ? texto : elegido ? nombreDe(elegido) : texto}
          onFocus={() => {
            setAbierto(true);
            setActivo(0);
          }}
          onChange={(e) => {
            setTexto(e.target.value);
            setActivo(0);
            setAbierto(true);
          }}
          onKeyDown={teclas}
        />
        {elegido && (
          <button type="button" className="bp-quitar" aria-label="Quitar producto" onClick={() => elegir(null)}>
            ×
          </button>
        )}
        <button
          type="button"
          className="bp-flecha"
          aria-label="Ver todos los productos"
          onClick={() => {
            setAbierto((a) => !a);
            setActivo(0);
            entrada.current?.focus();
          }}
        >
          ▾
        </button>
      </div>
      {abierto && (
        <div className="bp-menu">
          {visibles.length === 0 ? (
            <p className="bp-vacio">No hay productos con «{texto}».</p>
          ) : (
            <ul ref={lista} role="listbox">
              {visibles.map((p, i) => (
                <li
                  key={p.id}
                  role="option"
                  aria-selected={i === activo}
                  className={`${i === activo ? 'bp-activo' : ''}${String(p.id) === String(valor) ? ' bp-elegido' : ''}`}
                  onMouseEnter={() => setActivo(i)}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    elegir(p);
                  }}
                >
                  <span className="bp-nombre">{p.nombre}</span>
                  <span className="bp-detalle">
                    {p.codigo && <span>{p.codigo}</span>}
                    {p.controla_stock !== false && p.stock != null && <span>Stock {Number(p.stock)}</span>}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {resultados.length > MAXIMO_VISIBLES && (
            <p className="bp-mas">
              Hay {resultados.length} productos: se muestran {MAXIMO_VISIBLES}. Escribe más para encontrarlo.
            </p>
          )}
        </div>
      )}
    </div>
  );
}
