import { useEffect, useRef, useState } from 'react';
import { Scale } from 'lucide-react';
import { formatoCantidad, leerCantidad, subtotalLinea } from '../utils/medidas';
import { abreviaturaUnidad, etiquetaUnidad } from '../utils/unidades';
import './CalculadoraPieTablar.css';

// Atajos según la unidad: en kilos, litros o metros se vende por fracción
// (¼ kg); en gramos o mililitros, por cantidades redondas.
const ATAJOS = {
  GRAMO: ['100', '250', '500', '1000'],
  ML: ['100', '250', '500', '1000'],
  ONZA: ['1', '4', '8', '16'],
};
const ATAJOS_POR_DEFECTO = ['1/4', '1/2', '3/4', '1', '2'];

/**
 * Pregunta cuánto se lleva de un producto que se vende por peso, volumen o
 * largo (kilos, litros, metros...) antes de agregarlo al carrito. Se puede
 * escribir la cantidad ("0.75", "1/2", "1 1/2") o el monto en soles que
 * pide el cliente ("S/ 5 de azúcar"), y se calcula la cantidad.
 */
export default function CantidadFraccionada({ producto, yaEnCarrito = 0, onAgregar, onCerrar }) {
  const [modo, setModo] = useState('CANTIDAD');
  const [texto, setTexto] = useState('');
  const campo = useRef(null);
  const unidad = abreviaturaUnidad(producto.unidad_medida);

  useEffect(() => {
    campo.current?.focus();
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const cambiarModo = (nuevo) => {
    setModo(nuevo);
    setTexto('');
    campo.current?.focus();
  };

  const monto = modo === 'MONTO' ? leerCantidad(texto) : null;
  const cantidad =
    modo === 'CANTIDAD'
      ? leerCantidad(texto)
      : monto && producto.precio > 0
        ? Math.round((monto / producto.precio) * 1000) / 1000 || null
        : null;
  // Lo que se cobrará en el carrito (precio × cantidad, al céntimo). Por
  // monto puede diferir en un céntimo de lo escrito por el redondeo.
  const importe = cantidad ? subtotalLinea(producto.precio, cantidad) : 0;
  const controlaStock = producto.controla_stock !== false;
  const disponible = Math.max(0, producto.stock - yaEnCarrito);
  const sinStock = controlaStock && cantidad !== null && cantidad > disponible + 1e-6;

  const agregar = (e) => {
    e.preventDefault();
    if (!cantidad || sinStock) return;
    onAgregar(cantidad);
  };

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="pt-modal" onSubmit={agregar} role="dialog" aria-modal="true" aria-label={`Cantidad de ${producto.nombre}`}>
        <header className="pt-cabecera">
          <span className="pt-icono">
            <Scale size={18} />
          </span>
          <div>
            <h2>{producto.nombre}</h2>
            <p>
              S/ {producto.precio.toFixed(2)} por {etiquetaUnidad(producto.unidad_medida).toLowerCase()}
              {controlaStock && ` · quedan ${formatoCantidad(disponible)} ${unidad}`}
            </p>
          </div>
        </header>

        <div className="pt-modos" role="tablist">
          <button type="button" role="tab" aria-selected={modo === 'CANTIDAD'} className={modo === 'CANTIDAD' ? 'activo' : ''} onClick={() => cambiarModo('CANTIDAD')}>
            Por cantidad
          </button>
          <button type="button" role="tab" aria-selected={modo === 'MONTO'} className={modo === 'MONTO' ? 'activo' : ''} onClick={() => cambiarModo('MONTO')}>
            Por monto (S/)
          </button>
        </div>

        <label className="pt-campo pt-campo-directo">
          <span>{modo === 'CANTIDAD' ? `Cantidad (${unidad})` : 'Monto en soles'}</span>
          <input
            ref={campo}
            inputMode="decimal"
            placeholder={modo === 'CANTIDAD' ? '0.75 o 1/2' : '5.00'}
            value={texto}
            onChange={(e) => setTexto(e.target.value)}
          />
        </label>

        {modo === 'CANTIDAD' && (
          <div className="pt-largo-en" role="group" aria-label="Cantidades rápidas">
            {(ATAJOS[producto.unidad_medida] || ATAJOS_POR_DEFECTO).map((a) => (
              <button type="button" key={a} className={texto === a ? 'activo' : ''} onClick={() => setTexto(a)}>
                {a} {unidad}
              </button>
            ))}
          </div>
        )}

        <div className={`pt-resultado${sinStock ? ' pt-resultado-error' : ''}`}>
          <span>{cantidad ? `${formatoCantidad(cantidad)} ${unidad}` : modo === 'CANTIDAD' ? 'Escribe la cantidad' : 'Escribe el monto'}</span>
          <strong>S/ {importe.toFixed(2)}</strong>
        </div>
        {sinStock && (
          <p className="pt-aviso">
            No alcanza el stock: quedan {formatoCantidad(disponible)} {unidad}.
          </p>
        )}

        <div className="pt-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="pt-agregar" disabled={!cantidad || sinStock}>
            Agregar al carrito
          </button>
        </div>
      </form>
    </div>
  );
}
