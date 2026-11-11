import { useEffect, useMemo, useRef, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { api } from '../../api/api';
import { textoPlano } from '../../utils/medicamentos';
import { etiquetaUnidad, opcionesUnidad } from '../../utils/unidades';
import { armarIngreso, cantidadYCosto, faltaEnFila, redondear, soles } from '../../utils/ingresoRapido';
import './IngresoRapido.css';

// Ingreso rápido de productos: varios productos nuevos o que ya existen
// entran al stock en un paso, sin elegir proveedor (POST /compras/rapida).
// Por dentro es una compra ya recibida al proveedor "COMPRA RÁPIDA": sale en
// Compras y en los reportes. La foto, si es perecible, el stock mínimo, etc.
// se completan después en el producto.
//
// "Viene por caja": 2 cajas de 9 botellas a S/ 13.50 la caja = 18 botellas
// al stock a S/ 1.50 cada una. Con el módulo Presentaciones, un producto
// nuevo puede crear además su "Caja x9" para venderla entera.

let siguienteClave = 1;
const filaVacia = (anterior) => ({
  clave: siguienteClave++,
  busqueda: '',
  producto: null, // uno que ya existe (reponer); null = nuevo
  codigo: '',
  // Unidad y categoría: las de la fila anterior (suelen repetirse).
  unidad: anterior?.unidad || 'UNIDAD',
  categoria: anterior?.categoria || '',
  precioVenta: '',
  porCaja: false,
  cantidad: '',
  precioCompra: '',
  unidadesCaja: '',
  cajas: '',
  precioCaja: '',
  venderCaja: false,
  precioVentaCaja: '',
  vencimiento: '',
});

function Fila({ fila, n, productos, categorias, unidadesActivas, conPresentaciones, cambiar, quitar, puedeQuitar }) {
  const [abierta, setAbierta] = useState(false);
  const f = fila;
  const sugerencias = useMemo(() => {
    const q = textoPlano(f.busqueda.trim());
    if (f.producto || q.length < 2) return [];
    const palabras = q.split(/\s+/);
    return productos
      .filter((p) => {
        const texto = textoPlano(`${p.nombre} ${p.codigo || ''}`);
        return palabras.every((w) => texto.includes(w));
      })
      .slice(0, 6);
  }, [f.busqueda, f.producto, productos]);
  const { cantidad, costo } = cantidadYCosto(f);
  const subtotal = cantidad * costo;
  const unidad = f.producto ? f.producto.unidad_medida : f.unidad;

  const elegir = (p) => {
    cambiar({ producto: p, busqueda: p.nombre, venderCaja: false });
    setAbierta(false);
  };

  return (
    <div className="ir-fila">
      <div className="ir-fila-cabecera">
        <span className="ir-numero">{n}</span>
        <div className="ir-buscar">
          {f.producto ? (
            <div className="ir-elegido">
              <div>
                <strong>{f.producto.nombre}</strong>
                <small>
                  Reponer · stock actual {f.producto.stock} {etiquetaUnidad(f.producto.unidad_medida).toLowerCase()}
                  {f.producto.codigo ? ` · ${f.producto.codigo}` : ''}
                </small>
              </div>
              <button type="button" onClick={() => cambiar({ producto: null, busqueda: '', vencimiento: '' })}>
                Cambiar
              </button>
            </div>
          ) : (
            <>
              <input
                value={f.busqueda}
                placeholder="Nombre del producto nuevo, o busca uno que ya tienes para reponer"
                onChange={(e) => {
                  cambiar({ busqueda: e.target.value });
                  setAbierta(true);
                }}
                onFocus={() => setAbierta(true)}
                onBlur={() => setTimeout(() => setAbierta(false), 150)}
                onKeyDown={(e) => {
                  // Pistola: el código exacto de un producto lo elige.
                  if (e.key === 'Enter') {
                    const codigo = f.busqueda.trim().toLowerCase();
                    const exacto = productos.find((p) => (p.codigo || '').toLowerCase() === codigo);
                    if (exacto) {
                      e.preventDefault();
                      elegir(exacto);
                    }
                  }
                }}
              />
              {abierta && sugerencias.length > 0 && (
                <ul className="ir-sugerencias" role="listbox">
                  <li className="ir-sugerencias-titulo">Ya lo tienes: reponer</li>
                  {sugerencias.map((p) => (
                    <li key={p.id}>
                      <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={() => elegir(p)}>
                        <span>{p.nombre}</span>
                        <small>
                          {p.codigo} · stock {p.stock}
                        </small>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {f.busqueda.trim().length >= 2 && <span className="ir-etiqueta-nuevo">Nuevo</span>}
            </>
          )}
        </div>
        {puedeQuitar && (
          <button type="button" className="ir-quitar" onClick={quitar} aria-label={`Quitar fila ${n}`}>
            <Trash2 size={16} />
          </button>
        )}
      </div>

      {!f.producto && (
        <div className="ir-grid">
          <div className="inv-campo">
            <label>Categoría</label>
            <select value={f.categoria} onChange={(e) => cambiar({ categoria: e.target.value })}>
              <option value="">Elige...</option>
              {categorias.map((c) => (
                <option key={c.id} value={String(c.id)}>
                  {c.nombre}
                </option>
              ))}
            </select>
          </div>
          <div className="inv-campo">
            <label>Unidad</label>
            <select value={f.unidad} onChange={(e) => cambiar({ unidad: e.target.value })}>
              {opcionesUnidad(unidadesActivas, f.unidad).map((u) => (
                <option key={u.valor} value={u.valor}>
                  {u.label}
                </option>
              ))}
            </select>
          </div>
          <div className="inv-campo">
            <label>Precio de venta (S/ por {etiquetaUnidad(f.unidad).toLowerCase()})</label>
            <input inputMode="decimal" value={f.precioVenta} placeholder="0.00" onChange={(e) => cambiar({ precioVenta: e.target.value })} />
          </div>
          <div className="inv-campo">
            <label>Código (opcional)</label>
            <input value={f.codigo} placeholder="Se genera solo" onChange={(e) => cambiar({ codigo: e.target.value })} />
          </div>
        </div>
      )}

      <label className="ir-check">
        <input type="checkbox" checked={f.porCaja} onChange={(e) => cambiar({ porCaja: e.target.checked })} /> Viene por caja o paquete
      </label>

      {f.porCaja ? (
        <div className="ir-grid">
          <div className="inv-campo">
            <label>Unidades por caja</label>
            <input inputMode="numeric" value={f.unidadesCaja} placeholder="9" onChange={(e) => cambiar({ unidadesCaja: e.target.value })} />
          </div>
          <div className="inv-campo">
            <label>Cajas que entran</label>
            <input inputMode="decimal" value={f.cajas} placeholder="2" onChange={(e) => cambiar({ cajas: e.target.value })} />
          </div>
          <div className="inv-campo">
            <label>Precio de compra de la caja (S/)</label>
            <input inputMode="decimal" value={f.precioCaja} placeholder="0.00" onChange={(e) => cambiar({ precioCaja: e.target.value })} />
          </div>
          {cantidad > 0 && Number.isFinite(costo) && (
            <p className="ir-calculo">
              Entran <strong>{redondear(cantidad, 3)} {etiquetaUnidad(unidad).toLowerCase()}</strong> a {soles(costo)} cada una.
            </p>
          )}
          {!f.producto && conPresentaciones && (
            <>
              <label className="ir-check ir-check-full">
                <input type="checkbox" checked={f.venderCaja} onChange={(e) => cambiar({ venderCaja: e.target.checked })} /> También vender la caja entera
              </label>
              {f.venderCaja && (
                <div className="inv-campo">
                  <label>Precio de venta de la caja (S/)</label>
                  <input inputMode="decimal" value={f.precioVentaCaja} placeholder="0.00" onChange={(e) => cambiar({ precioVentaCaja: e.target.value })} />
                </div>
              )}
            </>
          )}
        </div>
      ) : (
        <div className="ir-grid">
          <div className="inv-campo">
            <label>Cantidad que entra ({etiquetaUnidad(unidad).toLowerCase()})</label>
            <input inputMode="decimal" value={f.cantidad} placeholder="0" onChange={(e) => cambiar({ cantidad: e.target.value })} />
          </div>
          <div className="inv-campo">
            <label>Precio de compra (S/ c/u)</label>
            <input inputMode="decimal" value={f.precioCompra} placeholder="0.00" onChange={(e) => cambiar({ precioCompra: e.target.value })} />
          </div>
        </div>
      )}

      {f.producto?.lleva_vencimiento && (
        <div className="ir-grid">
          <div className="inv-campo">
            <label>Vence el (perecible)</label>
            <input type="date" value={f.vencimiento} onChange={(e) => cambiar({ vencimiento: e.target.value })} />
          </div>
        </div>
      )}

      {subtotal > 0 && <p className="ir-subtotal">Subtotal {soles(subtotal)}</p>}
    </div>
  );
}

export default function IngresoRapido({ productos, categorias, unidadesActivas, presentaciones = false, onCerrar, onTerminado }) {
  const activos = useMemo(() => productos.filter((p) => p.activo !== false), [productos]);
  const [filas, setFilas] = useState(() => [filaVacia({ categoria: categorias[0] ? String(categorias[0].id) : '' })]);
  const [tipoPago, setTipoPago] = useState('EFECTIVO');
  const [caja, setCaja] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState(null);
  const fin = useRef(null);

  // Con caja abierta, por defecto se paga con su efectivo (como en Compras).
  useEffect(() => {
    api
      .cajaAbierta()
      .then((c) => {
        setCaja(c || null);
        if (c) setTipoPago('EFECTIVO_CAJA');
      })
      .catch(() => {});
  }, []);

  const cambiar = (clave, cambios) => {
    setError(null);
    setFilas((fs) => fs.map((f) => (f.clave === clave ? { ...f, ...cambios } : f)));
  };
  const agregar = () => {
    setFilas((fs) => [...fs, filaVacia(fs[fs.length - 1])]);
    setTimeout(() => fin.current?.scrollIntoView({ behavior: 'smooth', block: 'end' }), 50);
  };

  const total = filas.reduce((s, f) => {
    const { cantidad, costo } = cantidadYCosto(f);
    return s + (cantidad > 0 && costo >= 0 ? cantidad * costo : 0);
  }, 0);
  const efectivoCaja = caja
    ? (caja.monto_inicial || 0) + (caja.ventas_efectivo || 0) + (caja.ingresos_total || 0) - (caja.retiros_total || 0) - (caja.gastos_total || 0)
    : 0;
  const cajaNoAlcanza = tipoPago === 'EFECTIVO_CAJA' && total > efectivoCaja + 0.005;

  const guardar = async () => {
    for (const [i, f] of filas.entries()) {
      const falta = faltaEnFila(f, presentaciones);
      if (falta) {
        setError(`Fila ${i + 1}: ${falta}`);
        return;
      }
    }
    setGuardando(true);
    setError(null);
    try {
      const r = await api.compraRapida(armarIngreso(filas, tipoPago, presentaciones));
      onTerminado(r);
    } catch (e) {
      setError(e.message);
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="inv-modal-overlay">
      <div className="inv-modal ir-modal" role="dialog" aria-label="Ingreso rápido de productos">
        <h2>Ingreso rápido</h2>
        <p className="ir-nota">
          Productos nuevos o para reponer, sin elegir proveedor. Queda como una compra en Proveedores → Compras. La foto y lo demás se completan
          después en cada producto.
        </p>

        {categorias.length === 0 && (
          <p className="inv-mensaje inv-mensaje-error">Primero crea una categoría (botón "Categorías e IGV") para los productos nuevos.</p>
        )}

        {filas.map((f, i) => (
          <Fila
            key={f.clave}
            fila={f}
            n={i + 1}
            productos={activos}
            categorias={categorias}
            unidadesActivas={unidadesActivas}
            conPresentaciones={presentaciones}
            cambiar={(c) => cambiar(f.clave, c)}
            quitar={() => setFilas((fs) => fs.filter((x) => x.clave !== f.clave))}
            puedeQuitar={filas.length > 1}
          />
        ))}
        <button type="button" className="ir-agregar" onClick={agregar} ref={fin}>
          <Plus size={16} /> Agregar otro producto
        </button>

        <div className="ir-pie">
          <div className="inv-campo">
            <label>Forma de pago</label>
            <select value={tipoPago} onChange={(e) => setTipoPago(e.target.value)}>
              <option value="EFECTIVO_CAJA" disabled={!caja}>
                {caja ? 'Efectivo de la caja abierta' : 'Efectivo de la caja (no hay caja abierta)'}
              </option>
              <option value="EFECTIVO">Efectivo (no de la caja)</option>
              <option value="TRANSFERENCIA">Transferencia</option>
            </select>
            {tipoPago === 'EFECTIVO_CAJA' && caja && (
              <small className={cajaNoAlcanza ? 'ir-falta' : undefined}>
                {cajaNoAlcanza
                  ? `En la caja hay ${soles(Math.max(0, efectivoCaja))}: no alcanza.`
                  : `Se descuenta del efectivo de la caja (hay ${soles(efectivoCaja)}).`}
              </small>
            )}
          </div>
          <p className="ir-total">
            Total <strong>{soles(total)}</strong>
          </p>
        </div>

        {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}

        <div className="inv-modal-acciones">
          <button className="inv-boton-cancelar" onClick={onCerrar} disabled={guardando}>
            Cancelar
          </button>
          <button className="inv-boton-guardar" onClick={guardar} disabled={guardando || cajaNoAlcanza || (categorias.length === 0 && filas.some((f) => !f.producto))}>
            {guardando ? 'Guardando...' : `Guardar ingreso (${filas.length})`}
          </button>
        </div>
      </div>
    </div>
  );
}
