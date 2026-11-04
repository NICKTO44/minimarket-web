import { useMemo, useState } from 'react';
import { buscarParaEquivalentes } from '../../utils/equivalentes';
import { formatoCantidad } from '../../utils/medidas';
import './EquivalentesCampo.css';

// Sección "Equivalentes" del formulario de producto (módulo EQUIVALENTES).
// El dueño busca y agrega a mano los productos que resuelven lo mismo. Al
// agregar uno que ya tenía equivalentes, entran también los suyos: son un
// mismo grupo y se ven entre sí.
export default function EquivalentesCampo({ productos, repuestos, propioId, elegidos, onAgregar, onQuitar }) {
  const [buscar, setBuscar] = useState('');
  const [resaltado, setResaltado] = useState(0);
  const porId = useMemo(() => new Map(productos.map((p) => [p.id, p])), [productos]);
  const excluir = useMemo(() => new Set([propioId, ...elegidos].filter((x) => x != null)), [propioId, elegidos]);
  const encontrados = useMemo(() => buscarParaEquivalentes(productos, buscar, repuestos, excluir), [productos, buscar, repuestos, excluir]);
  const lista = elegidos.map((id) => porId.get(id)).filter(Boolean);

  const agregar = (p) => {
    onAgregar(p.id);
    setBuscar('');
    setResaltado(0);
  };
  const tecla = (e) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const paso = e.key === 'ArrowDown' ? 1 : -1;
      setResaltado((i) => Math.min(Math.max(0, i + paso), Math.max(0, encontrados.length - 1)));
    } else if (e.key === 'Enter') {
      // Enter agrega el resaltado y no guarda el formulario.
      e.preventDefault();
      const p = encontrados[Math.min(resaltado, encontrados.length - 1)];
      if (p) agregar(p);
    } else if (e.key === 'Escape') {
      setBuscar('');
    }
  };
  const detalle = (p) => [repuestos?.get(p.id)?.marca, p.descripcion].filter(Boolean).join(' · ');
  const stock = (p) => (p.controla_stock === false ? null : Number(p.stock) || 0);

  return (
    <div className="inv-ficha-caja eqv-caja">
      <p className="inv-lote-titulo">Equivalentes</p>
      <p className="inv-lote-nota">
        Productos que resuelven lo mismo. Salen debajo de este producto al buscar en Productos y en el punto de venta. Si agregas uno que
        ya tenía equivalentes, entran también los suyos.
      </p>
      <div className="eqv-buscador">
        <input
          value={buscar}
          placeholder="Busca por nombre, código, aplicación o N.° de parte (ej. SAIL)"
          aria-label="Buscar un producto equivalente"
          onChange={(e) => {
            setBuscar(e.target.value);
            setResaltado(0);
          }}
          onKeyDown={tecla}
        />
        {buscar.trim().length >= 2 && (
          <div className="eqv-resultados" role="listbox" aria-label="Productos encontrados">
            {encontrados.length === 0 ? (
              <p className="eqv-vacio">No se encontró ningún producto.</p>
            ) : (
              encontrados.map((p, i) => (
                <button
                  key={p.id}
                  type="button"
                  role="option"
                  aria-selected={i === resaltado}
                  className={`eqv-resultado${i === resaltado ? ' eqv-resultado-activo' : ''}`}
                  onMouseEnter={() => setResaltado(i)}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => agregar(p)}
                >
                  <span className="eqv-info">
                    <span className="eqv-nombre">
                      <span className="eqv-codigo">{p.codigo}</span>
                      {p.nombre}
                    </span>
                    {detalle(p) && <span className="eqv-detalle">{detalle(p)}</span>}
                  </span>
                  <span className="eqv-agregar">+ Agregar</span>
                </button>
              ))
            )}
          </div>
        )}
      </div>

      {lista.length === 0 ? (
        <p className="eqv-sin">Todavía no tiene equivalentes.</p>
      ) : (
        <ul className="eqv-lista">
          {lista.map((p) => (
            <li key={p.id} className="eqv-item">
              <span className="eqv-info">
                <span className="eqv-nombre">
                  <span className="eqv-codigo">{p.codigo}</span>
                  {p.nombre}
                </span>
                {detalle(p) && <span className="eqv-detalle">{detalle(p)}</span>}
              </span>
              {stock(p) != null && (
                <span className={`eqv-stock${stock(p) > 0 ? '' : ' eqv-stock-sin'}`}>
                  {stock(p) > 0 ? `Stock ${formatoCantidad(stock(p))}` : 'Sin stock'}
                </span>
              )}
              <button type="button" className="eqv-quitar" aria-label={`Quitar ${p.nombre} de los equivalentes`} onClick={() => onQuitar(p.id)}>
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
