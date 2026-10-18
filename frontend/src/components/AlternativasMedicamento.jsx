import { useEffect } from 'react';
import { ArrowLeftRight } from 'lucide-react';
import { formatoCantidad, formatoPrecio } from '../utils/medidas';
import { abreviaturaUnidad } from '../utils/unidades';
import { etiquetaCondicion, resumenFicha } from '../utils/medicamentos';
import './CalculadoraPieTablar.css';
import './AlternativasMedicamento.css';

/**
 * Alternativas de un medicamento (módulo ALTERNATIVAS): mismo principio
 * activo, concentración y forma. Se abre cuando el elegido no tiene stock o
 * desde su línea del carrito para cambiarlo por otro.
 * `motivo`: 'SIN_STOCK' | 'CAMBIAR'.
 */
export default function AlternativasMedicamento({ producto, ficha, alternativas, motivo, onElegir, onCerrar }) {
  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const masBarata = alternativas.find(
    (a) => a.conStock && a.producto.unidad_medida === producto.unidad_medida && a.producto.precio < producto.precio
  );

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pt-modal am-modal" role="dialog" aria-modal="true" aria-label={`Alternativas de ${producto.nombre}`}>
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
            <p>Mismo principio activo, concentración y forma: {resumenFicha(ficha)}</p>
          </div>
        </header>

        {alternativas.length === 0 ? (
          <p className="am-vacio">No hay otro producto con el mismo principio activo, concentración y forma.</p>
        ) : (
          <div className="am-lista">
            {alternativas.map(({ producto: p, ficha: f, conStock }) => (
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
                  <small>
                    {[f.laboratorio, f.es_generico ? 'Genérico' : null].filter(Boolean).join(' · ') || 'Sin laboratorio'}
                    {etiquetaCondicion(f.condicion_venta) && ` · ${etiquetaCondicion(f.condicion_venta)}`}
                  </small>
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
                  {masBarata?.producto.id === p.id && <span className="am-barata">Más barata</span>}
                </span>
              </button>
            ))}
          </div>
        )}

        <p className="am-nota">Confirma con el cliente antes de cambiar. Si pide receta, se pedirá al cobrar.</p>

        <div className="pt-acciones am-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            {motivo === 'SIN_STOCK' ? 'Cerrar' : 'Mantener el que eligió'}
          </button>
        </div>
      </div>
    </div>
  );
}
