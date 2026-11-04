import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import { opcionesUnidad } from '../../utils/unidades';
import '../../components/PantallaModulo.css';
import './FueraInventario.css';

// Módulo "Vender algo que no está en inventario": el cliente se lleva algo
// que el negocio consiguió afuera. Entra al carrito como una línea más (con
// la marca `externo`) y solo sale en el comprobante: el servidor no lo
// guarda en la venta, así que no entra a caja, reportes, ganancias, stock
// ni créditos (ver backend/src/handlers/fuera_inventario.rs).

let siguiente = 1;

/** Línea del carrito a partir de lo escrito en el formulario. */
const lineaDelCarrito = ({ nombre, cantidad, precio, unidad, codigo }) => {
  const clave = `externo-${Date.now()}-${siguiente++}`;
  return {
    clave,
    id: clave,
    externo: true,
    controla_stock: false,
    nombre,
    cantidad,
    precio,
    descuentoMonto: 0,
    unidad_medida: unidad,
    codigo: codigo || '',
  };
};

/** "12,5" o "" -> número (NaN si no es número). */
const leer = (texto) => Number(String(texto ?? '').trim().replace(',', '.'));

/** Botón que abre el formulario. */
export function BotonFueraInventario({ onClick, disabled, compacto = false }) {
  return (
    <button type="button" className={`fi-boton${compacto ? ' fi-boton-compacto' : ''}`} onClick={onClick} disabled={disabled}>
      ➕ Vender algo que no está en inventario
    </button>
  );
}

/** Etiqueta para la línea en el carrito o en la tabla. */
export function EtiquetaFueraInventario() {
  return <span className="fi-etiqueta">Fuera de inventario</span>;
}

/** Formulario: nombre, cantidad, precio con IGV, unidad y código. */
export function FormularioFueraInventario({ onAgregar, onCerrar }) {
  const [datos, setDatos] = useState({ nombre: '', cantidad: '1', precio: '', unidad: 'UNIDAD', codigo: '' });
  const [error, setError] = useState(null);
  const [unidades, setUnidades] = useState(null);

  useEffect(() => {
    api
      .unidades()
      .then((u) => setUnidades(u.activas))
      .catch(() => {});
  }, []);

  const cambiar = (campo) => (e) => {
    setDatos({ ...datos, [campo]: e.target.value });
    setError(null);
  };

  const agregar = (e) => {
    e.preventDefault();
    const nombre = datos.nombre.trim();
    const cantidad = leer(datos.cantidad);
    const precio = leer(datos.precio);
    if (!nombre) return setError('Escribe el nombre de lo que vendes.');
    if (!(cantidad > 0)) return setError('La cantidad debe ser mayor a 0.');
    if (!(precio > 0)) return setError('Escribe el precio con IGV.');
    onAgregar(
      lineaDelCarrito({
        nombre: nombre.slice(0, 200),
        cantidad: Math.round(cantidad * 1000) / 1000,
        precio: Math.round(precio * 100) / 100,
        unidad: datos.unidad,
        codigo: datos.codigo.trim().slice(0, 40),
      })
    );
  };

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form
        className="pm-modal pm-modal-chico"
        role="dialog"
        aria-modal="true"
        aria-label="Vender algo que no está en inventario"
        onSubmit={agregar}
      >
        <div className="pm-modal-cabecera">
          <h2>Vender algo que no está en inventario</h2>
          <p>Sale en la boleta, factura o nota de venta. No descuenta stock ni entra a caja, reportes ni créditos.</p>
        </div>
        <div className="pm-modal-cuerpo">
          <div className="pm-campos">
            <label className="pm-campo pm-campo-ancho">
              <span>Nombre *</span>
              <input autoFocus maxLength={200} placeholder="Bomba de agua Toyota Hilux" value={datos.nombre} onChange={cambiar('nombre')} />
            </label>
            <label className="pm-campo">
              <span>Cantidad</span>
              <input inputMode="decimal" value={datos.cantidad} onChange={cambiar('cantidad')} onFocus={(e) => e.target.select()} />
            </label>
            <label className="pm-campo">
              <span>Precio con IGV (S/) *</span>
              <input inputMode="decimal" placeholder="0.00" value={datos.precio} onChange={cambiar('precio')} />
            </label>
            <label className="pm-campo">
              <span>Unidad</span>
              <select value={datos.unidad} onChange={cambiar('unidad')}>
                {opcionesUnidad(unidades, datos.unidad).map((u) => (
                  <option key={u.valor} value={u.valor}>
                    {u.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="pm-campo">
              <span>Código (opcional)</span>
              <input maxLength={40} value={datos.codigo} onChange={cambiar('codigo')} />
            </label>
          </div>
          {error && (
            <p className="fi-error" role="alert">
              {error}
            </p>
          )}
        </div>
        <div className="pm-modal-pie">
          <button type="button" className="pm-boton-secundario" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="pm-boton">
            Agregar a la venta
          </button>
        </div>
      </form>
    </div>
  );
}
