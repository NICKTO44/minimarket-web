import { useEffect } from 'react';
import { ArrowLeftRight } from 'lucide-react';
import { formatoCantidad, formatoPrecio } from '../utils/medidas';
import { abreviaturaUnidad } from '../utils/unidades';
import { resumenRepuesto } from '../utils/repuestos';
import './CalculadoraPieTablar.css';
import './AlternativasMedicamento.css';

/**
 * Equivalentes de un repuesto (módulo FICHA_REPUESTO): los productos que
 * comparten con él un código (n.° de parte o código alterno). Se abre cuando
 * el elegido no tiene stock o desde su línea del carrito para cambiarlo.
 * `motivo`: 'SIN_STOCK' | 'CAMBIAR'. Usa los estilos de las alternativas de
 * farmacia.
 */
export default function EquivalentesRepuesto({ producto, ficha, equivalentes, motivo, onElegir, onCerrar }) {
  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const masBarato = equivalentes.find(
    (a) => a.conStock && a.producto.unidad_medida === producto.unidad_medida && a.producto.precio < producto.precio
  );

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pt-modal am-modal" role="dialog" aria-modal="true" aria-label={`Equivalentes de ${producto.nombre}`}>
        <header className="pt-cabecera">
          <span className="pt-icono">
            <ArrowLeftRight size={18} />
          </span>
          <div>
            <h2>
              {motivo !== 'SIN_STOCK'
                ? `Cambiar ${producto.nombre}`
                : producto.stock > 0
                  ? `Ya no queda más: ${producto.nombre}`
                  : `Sin stock: ${producto.nombre}`}
            </h2>
            <p>Equivalentes por código{ficha ? `: ${resumenRepuesto(ficha)}` : ''}</p>
          </div>
        </header>

        {equivalentes.length === 0 ? (
          <p className="am-vacio">No hay otro producto que comparta un código con este.</p>
        ) : (
          <div className="am-lista">
            {equivalentes.map(({ producto: p, ficha: f, conStock }) => (
              <button
                type="button"
                key={p.id}
                className="am-opcion"
                disabled={!conStock}
                onClick={() => onElegir(p)}
                title={conStock ? undefined : p.stock > 0 ? 'Todo lo que hay ya está en el carrito' : 'Sin stock'}
              >
                <span className="am-nombre">
                  {p.nombre}
                  <small>{resumenRepuesto(f) || 'Sin marca'}</small>
                </span>
                <span className="am-derecha">
                  <span className="am-precio">
                    S/ {formatoPrecio(p.precio)}
                    <small>/{abreviaturaUnidad(p.unidad_medida)}</small>
                  </span>
                  <span className={`am-stock${conStock ? '' : ' am-sin-stock'}`}>
                    {conStock
                      ? p.controla_stock === false
                        ? 'Disponible'
                        : `${formatoCantidad(p.stock)} ${abreviaturaUnidad(p.unidad_medida)}`
                      : p.stock > 0
                        ? 'Ya en el carrito'
                        : 'Sin stock'}
                  </span>
                  {masBarato?.producto.id === p.id && <span className="am-barata">Más barato</span>}
                </span>
              </button>
            ))}
          </div>
        )}

        <p className="am-nota">Confirma con el cliente que le sirve para su vehículo antes de cambiar.</p>

        <div className="pt-acciones am-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            {motivo === 'SIN_STOCK' ? 'Cerrar' : 'Mantener el que eligió'}
          </button>
        </div>
      </div>
    </div>
  );
}
