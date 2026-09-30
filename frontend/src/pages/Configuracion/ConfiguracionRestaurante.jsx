import { useEffect, useMemo, useState } from 'react';
import { Pencil, Plus, Trash2, X } from 'lucide-react';
import { api } from '../../api/api';
import { confirmar } from '../../utils/confirmar';
import './ConfiguracionRestaurante.css';

const MESA_VACIA = { id: null, nombre: '', zona: 'Salón', capacidad: 4 };
const GRUPO_VACIO = { id: null, nombre: '', obligatorio: false, multiple: false, opciones: [], producto_ids: [] };

// Plantillas para que una cafetería empiece en un clic (se pueden editar).
const PLANTILLAS = [
  {
    nombre: 'Tamaño',
    obligatorio: true,
    multiple: false,
    opciones: [
      { nombre: 'Chico', precio_extra: 0 },
      { nombre: 'Mediano', precio_extra: 1 },
      { nombre: 'Grande', precio_extra: 2 },
    ],
  },
  {
    nombre: 'Tipo de leche',
    obligatorio: false,
    multiple: false,
    opciones: [
      { nombre: 'Leche entera', precio_extra: 0 },
      { nombre: 'Deslactosada', precio_extra: 0 },
      { nombre: 'Leche de almendras', precio_extra: 2 },
    ],
  },
  {
    nombre: 'Extras',
    obligatorio: false,
    multiple: true,
    opciones: [
      { nombre: 'Shot extra de café', precio_extra: 2 },
      { nombre: 'Crema batida', precio_extra: 1.5 },
    ],
  },
];

/**
 * Configuración del módulo Cafetería / Restaurante (solo administrador):
 *  - Mesas: nombre, zona y capacidad.
 *  - Opciones de productos: grupos como "Tamaño" o "Extras", con precio
 *    extra por opción, y a qué productos se aplican.
 */
export default function ConfiguracionRestaurante() {
  const [mesas, setMesas] = useState([]);
  const [grupos, setGrupos] = useState([]);
  const [productos, setProductos] = useState([]);
  const [mensaje, setMensaje] = useState(null);
  const [mesaEditando, setMesaEditando] = useState(null);
  const [grupoEditando, setGrupoEditando] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const [busquedaProducto, setBusquedaProducto] = useState('');

  const cargar = () =>
    Promise.all([api.mesas(), api.modificadores(), api.productos()])
      .then(([m, g, p]) => {
        setMesas(m);
        setGrupos(g);
        setProductos(p);
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }));

  useEffect(() => {
    cargar();
  }, []);

  const nombreProducto = useMemo(() => new Map(productos.map((p) => [p.id, p.nombre])), [productos]);

  const guardar = async (accion, textoOk) => {
    setGuardando(true);
    setMensaje(null);
    try {
      await accion();
      setMensaje({ tipo: 'exito', texto: textoOk });
      await cargar();
      return true;
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
      return false;
    } finally {
      setGuardando(false);
    }
  };

  // ---------- Mesas ----------
  const guardarMesa = async () => {
    const m = mesaEditando;
    const datos = { nombre: m.nombre, zona: m.zona, capacidad: parseInt(m.capacidad, 10) || 4 };
    const ok = await guardar(
      () => (m.id ? api.mesaActualizar(m.id, datos) : api.mesaCrear(datos)),
      m.id ? 'Mesa actualizada.' : 'Mesa agregada.'
    );
    if (ok) setMesaEditando(null);
  };

  const quitarMesa = async (m) => {
    const si = await confirmar({
      titulo: `¿Quitar ${m.nombre}?`,
      mensaje: 'Deja de aparecer en el mapa. Los pedidos que ya se cobraron en ella se conservan.',
      textoConfirmar: 'Quitar',
      icono: 'eliminar',
    });
    if (si) guardar(() => api.mesaQuitar(m.id), 'Mesa quitada.');
  };

  // ---------- Opciones ----------
  const abrirGrupo = (g) => {
    setBusquedaProducto('');
    setGrupoEditando({
      ...g,
      opciones: g.opciones.map((o) => ({ ...o, precio_extra: String(o.precio_extra) })),
    });
  };

  const usarPlantilla = (plantilla) => {
    setBusquedaProducto('');
    setGrupoEditando({
      ...GRUPO_VACIO,
      ...plantilla,
      opciones: plantilla.opciones.map((o) => ({ id: null, nombre: o.nombre, precio_extra: String(o.precio_extra) })),
    });
  };

  const cambiarOpcion = (indice, campo, valor) =>
    setGrupoEditando((g) => ({
      ...g,
      opciones: g.opciones.map((o, i) => (i === indice ? { ...o, [campo]: valor } : o)),
    }));

  const alternarProducto = (id) =>
    setGrupoEditando((g) => ({
      ...g,
      producto_ids: g.producto_ids.includes(id) ? g.producto_ids.filter((p) => p !== id) : [...g.producto_ids, id],
    }));

  const guardarGrupo = async () => {
    const g = grupoEditando;
    const datos = {
      nombre: g.nombre,
      obligatorio: g.obligatorio,
      multiple: g.multiple,
      opciones: g.opciones
        .filter((o) => o.nombre.trim())
        .map((o) => ({ id: o.id, nombre: o.nombre, precio_extra: parseFloat(o.precio_extra) || 0 })),
      producto_ids: g.producto_ids,
    };
    const ok = await guardar(
      () => (g.id ? api.modificadorActualizar(g.id, datos) : api.modificadorCrear(datos)),
      'Opciones guardadas.'
    );
    if (ok) setGrupoEditando(null);
  };

  const quitarGrupo = async (g) => {
    const si = await confirmar({
      titulo: `¿Eliminar "${g.nombre}"?`,
      mensaje: 'Los productos dejarán de pedir estas opciones. Los pedidos anteriores no cambian.',
      textoConfirmar: 'Eliminar',
      icono: 'eliminar',
    });
    if (si) guardar(() => api.modificadorQuitar(g.id), 'Grupo eliminado.');
  };

  const zonas = [...new Set(mesas.map((m) => m.zona))];
  const productosFiltrados = productos.filter((p) =>
    p.nombre.toLowerCase().includes(busquedaProducto.trim().toLowerCase())
  );

  return (
    <div className="cfgr">
      {mensaje && <p className={`cfg-mensaje cfg-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {/* ---------- Mesas ---------- */}
      <div className="cfg-card cfgr-card">
        <div className="cfgr-titulo">
          <div>
            <h3 className="cfg-subtitulo-seccion">Mesas</h3>
            <p className="cfg-nota-moneda">
              {mesas.length} mesa(s){zonas.length > 1 ? ` en ${zonas.length} zonas` : ''}. Usa zonas como Salón,
              Terraza o Barra para ordenarlas en el mapa.
            </p>
          </div>
          <button type="button" className="cfgr-boton" onClick={() => setMesaEditando({ ...MESA_VACIA })}>
            <Plus size={15} /> Mesa
          </button>
        </div>
        <div className="cfgr-lista">
          {mesas.map((m) => (
            <div key={m.id} className="cfgr-fila">
              <div className="cfgr-fila-texto">
                <strong>{m.nombre}</strong>
                <span>
                  {m.zona} · {m.capacidad} personas{m.pedido ? ' · con pedido abierto' : ''}
                </span>
              </div>
              <button type="button" className="cfgr-icono" onClick={() => setMesaEditando({ ...m })} aria-label="Editar">
                <Pencil size={15} />
              </button>
              <button
                type="button"
                className="cfgr-icono peligro"
                onClick={() => quitarMesa(m)}
                disabled={!!m.pedido}
                aria-label="Quitar"
                title={m.pedido ? 'Tiene un pedido abierto' : 'Quitar mesa'}
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* ---------- Opciones de productos ---------- */}
      <div className="cfg-card cfgr-card">
        <div className="cfgr-titulo">
          <div>
            <h3 className="cfg-subtitulo-seccion">Opciones de productos</h3>
            <p className="cfg-nota-moneda">
              Grupos como Tamaño, Tipo de leche o Extras. Al pedir un producto que los tiene, el mesero elige la
              opción y el precio extra se suma solo.
            </p>
          </div>
          <button type="button" className="cfgr-boton" onClick={() => abrirGrupo({ ...GRUPO_VACIO })}>
            <Plus size={15} /> Grupo
          </button>
        </div>

        {grupos.length === 0 && (
          <div className="cfgr-plantillas">
            <span>Empieza rápido con un ejemplo:</span>
            {PLANTILLAS.map((p) => (
              <button key={p.nombre} type="button" onClick={() => usarPlantilla(p)}>
                + {p.nombre}
              </button>
            ))}
          </div>
        )}

        <div className="cfgr-lista">
          {grupos.map((g) => (
            <div key={g.id} className="cfgr-fila">
              <div className="cfgr-fila-texto">
                <strong>
                  {g.nombre}
                  <em>{g.obligatorio ? 'obligatorio' : 'opcional'}{g.multiple ? ' · varias' : ''}</em>
                </strong>
                <span>
                  {g.opciones
                    .map((o) => (o.precio_extra > 0 ? `${o.nombre} (+S/ ${o.precio_extra.toFixed(2)})` : o.nombre))
                    .join(' · ')}
                </span>
                <span className="cfgr-productos">
                  {g.producto_ids.length === 0
                    ? 'Aún no se aplica a ningún producto'
                    : `En: ${g.producto_ids
                        .map((id) => nombreProducto.get(id))
                        .filter(Boolean)
                        .join(', ')}`}
                </span>
              </div>
              <button type="button" className="cfgr-icono" onClick={() => abrirGrupo(g)} aria-label="Editar">
                <Pencil size={15} />
              </button>
              <button type="button" className="cfgr-icono peligro" onClick={() => quitarGrupo(g)} aria-label="Eliminar">
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* ---------- Ventana: mesa ---------- */}
      {mesaEditando && (
        <div className="cfg-modal-overlay" onClick={() => setMesaEditando(null)}>
          <div className="cfg-modal" onClick={(e) => e.stopPropagation()}>
            <h2>{mesaEditando.id ? 'Editar mesa' : 'Nueva mesa'}</h2>
            <div className="cfg-campo">
              <label>Nombre</label>
              <input
                value={mesaEditando.nombre}
                onChange={(e) => setMesaEditando({ ...mesaEditando, nombre: e.target.value })}
                placeholder="Ej. Mesa 7"
                autoFocus
              />
            </div>
            <div className="cfg-campo-fila">
              <div className="cfg-campo">
                <label>Zona</label>
                <input
                  value={mesaEditando.zona}
                  onChange={(e) => setMesaEditando({ ...mesaEditando, zona: e.target.value })}
                  placeholder="Salón"
                  list="cfgr-zonas"
                />
                <datalist id="cfgr-zonas">
                  {zonas.map((z) => (
                    <option key={z} value={z} />
                  ))}
                </datalist>
              </div>
              <div className="cfg-campo">
                <label>Capacidad</label>
                <input
                  type="number"
                  min="1"
                  value={mesaEditando.capacidad}
                  onChange={(e) => setMesaEditando({ ...mesaEditando, capacidad: e.target.value })}
                />
              </div>
            </div>
            <div className="cfg-modal-acciones">
              <button className="cfg-boton-cancelar" onClick={() => setMesaEditando(null)}>
                Cancelar
              </button>
              <button className="cfg-boton-guardar-modal" onClick={guardarMesa} disabled={guardando}>
                {guardando ? 'Guardando...' : 'Guardar'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ---------- Ventana: grupo de opciones ---------- */}
      {grupoEditando && (
        <div className="cfg-modal-overlay" onClick={() => setGrupoEditando(null)}>
          <div className="cfg-modal cfgr-modal-grupo" onClick={(e) => e.stopPropagation()}>
            <h2>{grupoEditando.id ? 'Editar opciones' : 'Nuevo grupo de opciones'}</h2>
            <div className="cfg-campo">
              <label>Nombre del grupo</label>
              <input
                value={grupoEditando.nombre}
                onChange={(e) => setGrupoEditando({ ...grupoEditando, nombre: e.target.value })}
                placeholder="Ej. Tamaño"
              />
            </div>
            <label className="cfgr-check">
              <input
                type="checkbox"
                checked={grupoEditando.obligatorio}
                onChange={(e) => setGrupoEditando({ ...grupoEditando, obligatorio: e.target.checked })}
              />
              Obligatorio: siempre hay que elegir una opción
            </label>
            <label className="cfgr-check">
              <input
                type="checkbox"
                checked={grupoEditando.multiple}
                onChange={(e) => setGrupoEditando({ ...grupoEditando, multiple: e.target.checked })}
              />
              Se pueden elegir varias (por ejemplo, extras)
            </label>

            <span className="cfgr-subtitulo">Opciones</span>
            {grupoEditando.opciones.map((o, i) => (
              <div key={o.id ?? `nueva-${i}`} className="cfgr-opcion">
                <input
                  value={o.nombre}
                  onChange={(e) => cambiarOpcion(i, 'nombre', e.target.value)}
                  placeholder="Ej. Grande"
                />
                <div className="cfgr-precio">
                  <span>+ S/</span>
                  <input
                    type="number"
                    min="0"
                    step="0.10"
                    value={o.precio_extra}
                    onChange={(e) => cambiarOpcion(i, 'precio_extra', e.target.value)}
                  />
                </div>
                <button
                  type="button"
                  className="cfgr-icono peligro"
                  onClick={() =>
                    setGrupoEditando((g) => ({ ...g, opciones: g.opciones.filter((_, j) => j !== i) }))
                  }
                  aria-label="Quitar opción"
                >
                  <X size={15} />
                </button>
              </div>
            ))}
            <button
              type="button"
              className="cfgr-agregar-opcion"
              onClick={() =>
                setGrupoEditando((g) => ({ ...g, opciones: [...g.opciones, { id: null, nombre: '', precio_extra: '0' }] }))
              }
            >
              <Plus size={14} /> Agregar opción
            </button>

            <span className="cfgr-subtitulo">
              ¿En qué productos? <small>({grupoEditando.producto_ids.length} elegidos)</small>
            </span>
            <input
              className="cfgr-buscar"
              value={busquedaProducto}
              onChange={(e) => setBusquedaProducto(e.target.value)}
              placeholder="Buscar producto..."
            />
            <div className="cfgr-productos-lista">
              {productosFiltrados.map((p) => (
                <label key={p.id} className="cfgr-check">
                  <input
                    type="checkbox"
                    checked={grupoEditando.producto_ids.includes(p.id)}
                    onChange={() => alternarProducto(p.id)}
                  />
                  {p.nombre}
                </label>
              ))}
              {productosFiltrados.length === 0 && <span className="cfg-nota-moneda">Sin resultados.</span>}
            </div>

            <div className="cfg-modal-acciones">
              <button className="cfg-boton-cancelar" onClick={() => setGrupoEditando(null)}>
                Cancelar
              </button>
              <button className="cfg-boton-guardar-modal" onClick={guardarGrupo} disabled={guardando}>
                {guardando ? 'Guardando...' : 'Guardar'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
