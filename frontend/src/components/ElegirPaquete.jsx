import { useEffect } from 'react';
import { Package } from 'lucide-react';
import { formatoCantidad } from '../utils/medidas';
import { abreviaturaUnidad } from '../utils/unidades';
import './CalculadoraPieTablar.css';
import './AlternativasMedicamento.css';

/**
 * Elegir un paquete en el punto de venta (módulo "Paquetes de productos").
 * `faltante(paquete)`: texto de lo que no alcanza (o null si se puede vender).
 * Usa los estilos de las alternativas de farmacia.
 */
export default function ElegirPaquete({ paquetes, faltante, onElegir, onCerrar }) {
  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pt-modal am-modal" role="dialog" aria-modal="true" aria-label="Paquetes">
        <header className="pt-cabecera">
          <span className="pt-icono">
            <Package size={18} />
          </span>
          <div>
            <h2>Paquetes</h2>
            <p>Entran sus productos, cada uno con su descuento, hasta el precio del paquete.</p>
          </div>
        </header>

        {paquetes.length === 0 ? (
          <p className="am-vacio">No hay paquetes. El administrador los arma en Inventario → Paquetes.</p>
        ) : (
          <div className="am-lista">
            {paquetes.map((p) => {
              const falta = faltante(p);
              const ahorro = Math.max(0, p.precio_normal - p.precio);
              return (
                <button
                  type="button"
                  key={p.id}
                  className="am-opcion"
                  disabled={!!falta}
                  onClick={() => onElegir(p)}
                  title={falta || undefined}
                >
                  <span className="am-nombre">
                    {p.nombre}
                    <small>
                      {p.items
                        .map((i) => `${formatoCantidad(i.cantidad)} ${abreviaturaUnidad(i.unidad_medida)} ${i.nombre}`)
                        .join(' + ')}
                    </small>
                    {falta && <small className="am-sin-stock">{falta}</small>}
                  </span>
                  <span className="am-derecha">
                    <span className="am-precio">S/ {p.precio.toFixed(2)}</span>
                    {ahorro > 0.004 && <span className="am-barata">Ahorra S/ {ahorro.toFixed(2)}</span>}
                  </span>
                </button>
              );
            })}
          </div>
        )}

        <div className="pt-acciones am-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
