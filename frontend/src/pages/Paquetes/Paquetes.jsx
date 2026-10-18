import { useEffect, useMemo, useState } from 'react';
import { Package, Plus } from 'lucide-react';
import { api } from '../../api/api';
import { confirmar } from '../../utils/confirmar';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import '../../components/PantallaModulo.css';

/**
 * Paquetes (módulo "Paquetes de productos"): varios productos a un precio,
 * como "Cambio de aceite completo" (aceite + filtro + mano de obra). En el
 * punto de venta entra cada producto con su descuento hasta llegar al
 * precio del paquete, y el stock baja de cada uno. Solo el administrador.
 */
export default function Paquetes() {
  const [lista, setLista] = useState([]);
  const [productos, setProductos] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  // Formulario abierto: { id (null = nuevo), nombre, precio, items: [{ producto_id, cantidad }] }.
  const [form, setForm] = useState(null);
  const [buscar, setBuscar] = useState('');
  const [guardando, setGuardando] = useState(false);

  const cargar = () => {
    setCargando(true);
    api
      .paquetes()
      .then((d) => {
        setLista(d);
        setError(null);
      })
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  };
  useEffect(() => {
    api
      .paquetes()
      .then(setLista)
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
    api.productos().then(setProductos).catch(() => {});
  }, []);

  const porId = useMemo(() => new Map(productos.map((p) => [p.id, p])), [productos]);
  const encontrados = useMemo(() => {
    const q = buscar.trim().toLowerCase();
    if (q.length < 2) return [];
    return productos.filter((p) => p.nombre.toLowerCase().includes(q) || String(p.codigo).toLowerCase().includes(q)).slice(0, 8);
  }, [productos, buscar]);

  const suma = form
    ? Math.round(form.items.reduce((s, i) => s + (porId.get(i.producto_id)?.precio || 0) * (Number(i.cantidad) || 0), 0) * 100) / 100
    : 0;
  const precio = Number(String(form?.precio || '').replace(',', '.')) || 0;

  const agregar = (p) => {
    setBuscar('');
    setForm((f) =>
      f.items.some((i) => i.producto_id === p.id)
        ? { ...f, items: f.items.map((i) => (i.producto_id === p.id ? { ...i, cantidad: String((Number(i.cantidad) || 0) + 1) } : i)) }
        : { ...f, items: [...f.items, { producto_id: p.id, cantidad: '1' }] }
    );
  };

  const guardar = async () => {
    setGuardando(true);
    setError(null);
    const datos = {
      nombre: form.nombre,
      precio,
      items: form.items.map((i) => ({ producto_id: i.producto_id, cantidad: Number(String(i.cantidad).replace(',', '.')) })),
    };
    try {
      if (form.id) await api.paqueteActualizar(form.id, datos);
      else await api.paqueteCrear(datos);
      setForm(null);
      cargar();
    } catch (e) {
      setError(e.message);
    } finally {
      setGuardando(false);
    }
  };

  const quitar = async () => {
    const ok = await confirmar({
      titulo: `¿Dar de baja "${form.nombre}"?`,
      mensaje: 'Ya no saldrá en el punto de venta. Las ventas que ya se hicieron no cambian.',
      textoConfirmar: 'Dar de baja',
      icono: 'aviso',
    });
    if (!ok) return;
    try {
      await api.paqueteQuitar(form.id);
      setForm(null);
      cargar();
    } catch (e) {
      setError(e.message);
    }
  };

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Package size={22} /> Paquetes
          </h1>
          <p className="pm-subtitulo">Varios productos a un precio. Al venderlo entra cada producto con su descuento y baja su stock.</p>
        </div>
        <div className="pm-acciones">
          <button className="pm-boton" onClick={() => setForm({ id: null, nombre: '', precio: '', items: [] })}>
            <Plus size={15} /> Nuevo paquete
          </button>
        </div>
      </header>

      {error && !form && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      <div className="pm-tarjeta">
        {cargando ? (
          <p className="pm-vacio">Cargando...</p>
        ) : lista.length === 0 ? (
          <p className="pm-vacio">Aún no hay paquetes. Arma uno, por ejemplo “Cambio de aceite completo”.</p>
        ) : (
          lista.map((p) => (
            <button
              key={p.id}
              className="pm-fila"
              onClick={() =>
                setForm({
                  id: p.id,
                  nombre: p.nombre,
                  precio: String(p.precio),
                  items: p.items.filter((i) => i.activo).map((i) => ({ producto_id: i.producto_id, cantidad: String(i.cantidad) })),
                })
              }
            >
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">
                  {p.nombre} {p.items.some((i) => !i.activo) && <span className="pm-chip pm-chip-mal">Tiene un producto desactivado</span>}
                </span>
                <span className="pm-fila-detalle" style={{ display: 'block' }}>
                  {p.items.map((i) => `${formatoCantidad(i.cantidad)} ${abreviaturaUnidad(i.unidad_medida)} ${i.nombre}`).join(' + ')}
                </span>
              </span>
              <span className="pm-fila-monto">
                <strong>S/ {p.precio.toFixed(2)}</strong>
                {p.precio_normal > p.precio && <small>normal S/ {p.precio_normal.toFixed(2)}</small>}
              </span>
            </button>
          ))
        )}
      </div>

      {form && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setForm(null)}>
          <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Paquete">
            <div className="pm-modal-cabecera">
              <h2>{form.id ? 'Editar paquete' : 'Nuevo paquete'}</h2>
              <p>El precio del paquete no puede pasar de lo que suman sus productos.</p>
            </div>
            <div className="pm-modal-cuerpo">
              {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Nombre</span>
                  <input
                    autoFocus
                    value={form.nombre}
                    maxLength={80}
                    placeholder="Cambio de aceite completo"
                    onChange={(e) => setForm((f) => ({ ...f, nombre: e.target.value }))}
                  />
                </label>
                <label className="pm-campo">
                  <span>Precio del paquete (S/)</span>
                  <input inputMode="decimal" value={form.precio} onChange={(e) => setForm((f) => ({ ...f, precio: e.target.value }))} />
                  <small>
                    Suman S/ {suma.toFixed(2)}
                    {precio > 0 && precio < suma && ` · el cliente ahorra S/ ${(suma - precio).toFixed(2)}`}
                    {precio > suma + 0.001 && ' · el precio pasa de lo que suman'}
                  </small>
                </label>
              </div>

              <p className="pm-seccion">Productos</p>
              {form.items.length > 0 && (
                <table className="pm-lineas">
                  <tbody>
                    {form.items.map((i) => {
                      const p = porId.get(i.producto_id);
                      return (
                        <tr key={i.producto_id}>
                          <td>
                            {p?.nombre || 'Producto desactivado'}
                            {p && <small>S/ {p.precio.toFixed(2)} c/u</small>}
                          </td>
                          <td className="pm-num">
                            <input
                              inputMode="decimal"
                              value={i.cantidad}
                              style={{ width: 70, textAlign: 'right' }}
                              aria-label={`Cantidad de ${p?.nombre || 'producto'}`}
                              onChange={(e) =>
                                setForm((f) => ({
                                  ...f,
                                  items: f.items.map((x) => (x.producto_id === i.producto_id ? { ...x, cantidad: e.target.value } : x)),
                                }))
                              }
                            />{' '}
                            {p && abreviaturaUnidad(p.unidad_medida)}
                          </td>
                          <td className="pm-num">
                            <button
                              className="pm-boton-secundario"
                              aria-label="Quitar"
                              onClick={() => setForm((f) => ({ ...f, items: f.items.filter((x) => x.producto_id !== i.producto_id) }))}
                            >
                              ✕
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
              <div className="pm-campo" style={{ marginTop: 10 }}>
                <input value={buscar} placeholder="Agregar producto o mano de obra (nombre o código)" onChange={(e) => setBuscar(e.target.value)} />
              </div>
              {encontrados.map((p) => (
                <button key={p.id} className="pm-fila" onClick={() => agregar(p)}>
                  <span className="pm-fila-principal">
                    <span className="pm-fila-titulo">{p.nombre}</span>
                    <span className="pm-fila-detalle" style={{ display: 'block' }}>{p.codigo}</span>
                  </span>
                  <span className="pm-fila-monto">
                    <strong>S/ {p.precio.toFixed(2)}</strong>
                  </span>
                </button>
              ))}
            </div>
            <div className="pm-modal-pie">
              {form.id && (
                <button className="pm-boton-peligro" onClick={quitar}>
                  Dar de baja
                </button>
              )}
              <button className="pm-boton-secundario" onClick={() => setForm(null)}>
                Cancelar
              </button>
              <button className="pm-boton" onClick={guardar} disabled={guardando}>
                {guardando ? 'Guardando...' : 'Guardar'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
