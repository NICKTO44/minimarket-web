import { useEffect, useRef, useState } from 'react';
import { Ruler } from 'lucide-react';
import { detalleMedidas, formatoCantidad, leerCantidad, piesTablares, subtotalLinea } from '../utils/medidas';
import './CalculadoraPieTablar.css';

const CLAVE_LARGO = 'medidas_largo_en';

function largoRecordado() {
  try {
    return localStorage.getItem(CLAVE_LARGO) === 'METROS' ? 'METROS' : 'PIES';
  } catch {
    return 'PIES';
  }
}

/**
 * Calculadora de pie tablar (módulo "Venta por medidas"). Se abre al tocar
 * en el punto de venta un producto cuya unidad es Pie tablar.
 *
 * Pies tablares = espesor (pulg) × ancho (pulg) × largo (pies) ÷ 12 × piezas.
 * También se pueden escribir los pies directamente. Cada cálculo entra al
 * carrito como una línea propia, con sus medidas, y así salen en el
 * comprobante.
 */
export default function CalculadoraPieTablar({ producto, yaEnCarrito = 0, onAgregar, onCerrar }) {
  const [modo, setModo] = useState('MEDIDAS');
  const [piezas, setPiezas] = useState('1');
  const [espesor, setEspesor] = useState('');
  const [ancho, setAncho] = useState('');
  const [largo, setLargo] = useState('');
  const [largoEn, setLargoEn] = useState(largoRecordado);
  const [directo, setDirecto] = useState('');
  const primerCampo = useRef(null);

  useEffect(() => {
    primerCampo.current?.focus();
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const cambiarLargoEn = (valor) => {
    setLargoEn(valor);
    try {
      localStorage.setItem(CLAVE_LARGO, valor);
    } catch {
      // Sin almacenamiento local: solo no se recuerda la preferencia.
    }
  };

  const medidas = { espesor, ancho, largo, largoEn, piezas };
  const pies = modo === 'MEDIDAS' ? piesTablares(medidas) : leerCantidad(directo);
  const importe = pies ? subtotalLinea(producto.precio, pies) : 0;
  const controlaStock = producto.controla_stock !== false;
  const disponible = Math.max(0, producto.stock - yaEnCarrito);
  const sinStock = controlaStock && pies !== null && pies > disponible + 1e-6;

  const agregar = (e) => {
    e.preventDefault();
    if (!pies || sinStock) return;
    onAgregar({ cantidad: pies, detalle: modo === 'MEDIDAS' ? detalleMedidas(medidas) : null });
  };

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="pt-modal" onSubmit={agregar} role="dialog" aria-modal="true" aria-label="Calculadora de pie tablar">
        <header className="pt-cabecera">
          <span className="pt-icono">
            <Ruler size={18} />
          </span>
          <div>
            <h2>{producto.nombre}</h2>
            <p>
              S/ {producto.precio.toFixed(2)} por pie tablar
              {controlaStock && ` · quedan ${formatoCantidad(disponible)} pt`}
            </p>
          </div>
        </header>

        <div className="pt-modos" role="tablist">
          <button type="button" role="tab" aria-selected={modo === 'MEDIDAS'} className={modo === 'MEDIDAS' ? 'activo' : ''} onClick={() => setModo('MEDIDAS')}>
            Por medidas
          </button>
          <button type="button" role="tab" aria-selected={modo === 'DIRECTO'} className={modo === 'DIRECTO' ? 'activo' : ''} onClick={() => setModo('DIRECTO')}>
            Pies directos
          </button>
        </div>

        {modo === 'MEDIDAS' ? (
          <>
            <div className="pt-campos">
              <label className="pt-campo">
                <span>Piezas</span>
                <input ref={primerCampo} inputMode="numeric" value={piezas} onChange={(e) => setPiezas(e.target.value)} onFocus={(e) => e.target.select()} />
              </label>
              <label className="pt-campo">
                <span>Espesor (pulg)</span>
                <input inputMode="decimal" placeholder='2 o 1 1/2' value={espesor} onChange={(e) => setEspesor(e.target.value)} />
              </label>
              <label className="pt-campo">
                <span>Ancho (pulg)</span>
                <input inputMode="decimal" placeholder="4" value={ancho} onChange={(e) => setAncho(e.target.value)} />
              </label>
              <label className="pt-campo">
                <span>Largo</span>
                <input inputMode="decimal" placeholder={largoEn === 'METROS' ? '3' : '10'} value={largo} onChange={(e) => setLargo(e.target.value)} />
              </label>
            </div>
            <div className="pt-largo-en" role="radiogroup" aria-label="Unidad del largo">
              <span>El largo está en</span>
              <button type="button" role="radio" aria-checked={largoEn === 'PIES'} className={largoEn === 'PIES' ? 'activo' : ''} onClick={() => cambiarLargoEn('PIES')}>
                pies
              </button>
              <button type="button" role="radio" aria-checked={largoEn === 'METROS'} className={largoEn === 'METROS' ? 'activo' : ''} onClick={() => cambiarLargoEn('METROS')}>
                metros
              </button>
            </div>
            <p className="pt-formula">espesor × ancho × largo en pies ÷ 12 × piezas</p>
          </>
        ) : (
          <label className="pt-campo pt-campo-directo">
            <span>Pies tablares</span>
            <input ref={primerCampo} inputMode="decimal" placeholder="37.5" value={directo} onChange={(e) => setDirecto(e.target.value)} />
          </label>
        )}

        <div className={`pt-resultado${sinStock ? ' pt-resultado-error' : ''}`}>
          <span>{pies ? `${formatoCantidad(pies)} pies tablares` : 'Escribe las medidas'}</span>
          <strong>S/ {importe.toFixed(2)}</strong>
        </div>
        {sinStock && <p className="pt-aviso">No alcanza el stock: quedan {formatoCantidad(disponible)} pt.</p>}

        <div className="pt-acciones">
          <button type="button" className="pt-cancelar" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="pt-agregar" disabled={!pies || sinStock}>
            Agregar al carrito
          </button>
        </div>
      </form>
    </div>
  );
}
