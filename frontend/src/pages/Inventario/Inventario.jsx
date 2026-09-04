import { useState, useEffect, useMemo } from 'react';
import { api, API_URL } from '../../api/api';
import './Inventario.css';
import EscanerCodigoBarras from '../../components/EscanerCodigoBarras';

const FORM_VACIO = {
  codigo: '',
  nombre: '',
  descripcion: '',
  precio: '',
  stock: '',
  stock_minimo: '5',
  unidad_medida: 'UNIDAD',
  categoria_id: '',
  lleva_vencimiento: false,
  precio_compra: '',
};

export default function Inventario() {
  const [productos, setProductos] = useState([]);
  const [categorias, setCategorias] = useState([]);
  const [busqueda, setBusqueda] = useState('');
  const [filtroCategoria, setFiltroCategoria] = useState('');
  const [soloStockBajo, setSoloStockBajo] = useState(false);
  const [cargando, setCargando] = useState(true);

  const [mostrarForm, setMostrarForm] = useState(false);
  const [editandoId, setEditandoId] = useState(null);
  const [form, setForm] = useState(FORM_VACIO);
  const [guardando, setGuardando] = useState(false);
  const [mensaje, setMensaje] = useState(null);

  const [imagenArchivo, setImagenArchivo] = useState(null);
  const [imagenPreview, setImagenPreview] = useState(null);

  const [loteInicialCantidad, setLoteInicialCantidad] = useState('');
  const [loteInicialFecha, setLoteInicialFecha] = useState('');

  const [lotesProducto, setLotesProducto] = useState([]);
  const [cargandoLotes, setCargandoLotes] = useState(false);
  const [nuevoLoteCantidad, setNuevoLoteCantidad] = useState('');
  const [nuevoLoteFecha, setNuevoLoteFecha] = useState('');
  const [agregandoLote, setAgregandoLote] = useState(false);

  // --- Escáner de código de barras (modo una sola lectura) ---
  const [escanerCodigoAbierto, setEscanerCodigoAbierto] = useState(false);

  const cargarTodo = () => {
    setCargando(true);
    Promise.all([api.productos(), api.categorias()])
      .then(([p, c]) => {
        setProductos(p);
        setCategorias(c);
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargarTodo();
  }, []);

  const productosFiltrados = useMemo(() => {
    let lista = productos;
    if (busqueda.trim()) {
      const q = busqueda.toLowerCase();
      lista = lista.filter((p) => p.nombre.toLowerCase().includes(q) || p.codigo.includes(q));
    }
    if (filtroCategoria) {
      lista = lista.filter((p) => String(p.categoria_id) === filtroCategoria);
    }
    if (soloStockBajo) {
      lista = lista.filter((p) => p.stock <= p.stock_minimo);
    }
    return lista;
  }, [productos, busqueda, filtroCategoria, soloStockBajo]);

  const abrirNuevo = () => {
    setEditandoId(null);
    setForm(FORM_VACIO);
    setLoteInicialCantidad('');
    setLoteInicialFecha('');
    setLotesProducto([]);
    setImagenArchivo(null);
    setImagenPreview(null);
    setMensaje(null);
    setMostrarForm(true);
  };

  const abrirEdicion = (p) => {
    setEditandoId(p.id);
    setForm({
      codigo: p.codigo,
      nombre: p.nombre,
      descripcion: p.descripcion || '',
      precio: String(p.precio),
      stock: String(p.stock),
      stock_minimo: String(p.stock_minimo),
      unidad_medida: p.unidad_medida,
      categoria_id: String(p.categoria_id),
      lleva_vencimiento: p.lleva_vencimiento,
      precio_compra: String(p.precio_compra || ''),
    });
    setImagenArchivo(null);
    setImagenPreview(p.imagen_url ? `${API_URL}${p.imagen_url}?t=${Date.now()}` : null);
    setMensaje(null);
    setMostrarForm(true);

    if (p.lleva_vencimiento) {
      cargarLotes(p.id);
    } else {
      setLotesProducto([]);
    }
  };

  const cargarLotes = (productoId) => {
    setCargandoLotes(true);
    api
      .lotesDeProducto(productoId)
      .then(setLotesProducto)
      .catch(() => setLotesProducto([]))
      .finally(() => setCargandoLotes(false));
  };

  const cerrarForm = () => {
    setMostrarForm(false);
    setEditandoId(null);
    setForm(FORM_VACIO);
    setLotesProducto([]);
    setLoteInicialCantidad('');
    setLoteInicialFecha('');
    setImagenArchivo(null);
    setImagenPreview(null);
  };

  const cambiarCampo = (campo, valor) => {
    setForm((f) => ({ ...f, [campo]: valor }));
  };

  const manejarSeleccionImagen = (e) => {
    const archivo = e.target.files?.[0];
    if (!archivo) return;
    if (!archivo.type.startsWith('image/')) {
      setMensaje({ tipo: 'error', texto: 'El archivo debe ser una imagen.' });
      return;
    }
    setImagenArchivo(archivo);
    setImagenPreview(URL.createObjectURL(archivo));
  };

  const agregarLoteAProductoExistente = async () => {
    if (!nuevoLoteCantidad || !nuevoLoteFecha) {
      setMensaje({ tipo: 'error', texto: 'Completa cantidad y fecha de vencimiento del lote.' });
      return;
    }
    setAgregandoLote(true);
    try {
      await api.loteCrear({
        producto_id: editandoId,
        cantidad: parseFloat(nuevoLoteCantidad),
        fecha_vencimiento: nuevoLoteFecha,
      });
      setNuevoLoteCantidad('');
      setNuevoLoteFecha('');
      cargarLotes(editandoId);
      cargarTodo();
      setMensaje({ tipo: 'exito', texto: 'Lote agregado.' });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setAgregandoLote(false);
    }
  };

  const validarYGuardar = async () => {
    setMensaje(null);
    if (!form.codigo.trim() || !form.nombre.trim() || !form.categoria_id) {
      setMensaje({ tipo: 'error', texto: 'Código, nombre y categoría son obligatorios.' });
      return;
    }
    const precio = parseFloat(form.precio);
    if (isNaN(precio) || precio <= 0) {
      setMensaje({ tipo: 'error', texto: 'El precio debe ser mayor a 0.' });
      return;
    }

    if (!form.lleva_vencimiento) {
      const stock = parseFloat(form.stock);
      if (isNaN(stock) || stock < 0) {
        setMensaje({ tipo: 'error', texto: 'El stock no puede ser negativo.' });
        return;
      }
    } else if (!editandoId) {
      if (!loteInicialCantidad || !loteInicialFecha) {
        setMensaje({
          tipo: 'error',
          texto: 'Como es perecible, indica la cantidad y fecha de vencimiento del primer lote.',
        });
        return;
      }
    }

    const payload = {
      codigo: form.codigo.trim(),
      nombre: form.nombre.trim(),
      descripcion: form.descripcion.trim() || null,
      precio,
      stock: form.lleva_vencimiento ? 0 : parseFloat(form.stock),
      stock_minimo: parseFloat(form.stock_minimo) || 0,
      unidad_medida: form.unidad_medida,
      categoria_id: parseInt(form.categoria_id, 10),
      lleva_vencimiento: form.lleva_vencimiento,
      precio_compra: form.precio_compra ? parseFloat(form.precio_compra) : 0,
    };

    setGuardando(true);
    try {
      let idParaImagen = editandoId;

      if (editandoId) {
        await api.productoActualizar(editandoId, payload);
      } else {
        const creado = await api.productoCrear(payload);
        idParaImagen = creado.producto_id;
        if (form.lleva_vencimiento && creado.producto_id) {
          await api.loteCrear({
            producto_id: creado.producto_id,
            cantidad: parseFloat(loteInicialCantidad),
            fecha_vencimiento: loteInicialFecha,
          });
        }
      }

      if (imagenArchivo && idParaImagen) {
        try {
          await api.productoSubirImagen(idParaImagen, imagenArchivo);
        } catch (e) {
          setMensaje({ tipo: 'error', texto: `Producto guardado, pero falló la imagen: ${e.message}` });
          cargarTodo();
          return;
        }
      }

      setMensaje({ tipo: 'exito', texto: editandoId ? 'Producto actualizado.' : 'Producto agregado.' });
      cerrarForm();
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  const eliminarOProducto = async (p) => {
    if (!confirm(`¿Eliminar "${p.nombre}"?`)) return;
    try {
      const resultado = await api.productoEliminar(p.id);
      if (!resultado.success) {
        if (confirm(`${resultado.message}\n\n¿Deseas desactivarlo en su lugar? (dejará de aparecer en el POS)`)) {
          await api.productoDesactivar(p.id);
          setMensaje({ tipo: 'exito', texto: 'Producto desactivado.' });
          cargarTodo();
        }
        return;
      }
      setMensaje({ tipo: 'exito', texto: 'Producto eliminado.' });
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const stockBajoCantidad = productos.filter((p) => p.stock <= p.stock_minimo).length;

  return (
    <div className="inv-layout">
      <div className="inv-header">
        <h1>Inventario</h1>
        <button className="inv-boton-nuevo" onClick={abrirNuevo}>
          + Nuevo producto
        </button>
      </div>

      <div className="inv-filtros">
        <input
          type="text"
          placeholder="Buscar por nombre o código..."
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
        />
        <select value={filtroCategoria} onChange={(e) => setFiltroCategoria(e.target.value)}>
          <option value="">Todas las categorías</option>
          {categorias.map((c) => (
            <option key={c.id} value={c.id}>
              {c.nombre}
            </option>
          ))}
        </select>
        <button
          className={`inv-filtro-stock ${soloStockBajo ? 'activo' : ''}`}
          onClick={() => setSoloStockBajo((v) => !v)}
        >
          ⚠ Stock bajo {stockBajoCantidad > 0 && `(${stockBajoCantidad})`}
        </button>
      </div>

      {mensaje && !mostrarForm && (
        <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>
      )}

      {cargando ? (
        <p className="inv-cargando">Cargando...</p>
      ) : (
        <div className="inv-tabla-wrapper">
          <table className="inv-tabla">
            <thead>
              <tr>
                <th></th>
                <th>Código</th>
                <th>Nombre</th>
                <th>Categoría</th>
                <th>Precio</th>
                <th>Stock</th>
                <th>Unidad</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {productosFiltrados.map((p) => (
                <tr key={p.id} className={p.stock <= p.stock_minimo ? 'inv-fila-alerta' : ''}>
                  <td>
                    {p.imagen_url ? (
                      <img className="inv-miniatura" src={`${API_URL}${p.imagen_url}`} alt={p.nombre} />
                    ) : (
                      <div className="inv-miniatura inv-miniatura-vacia">📦</div>
                    )}
                  </td>
                  <td>{p.codigo}</td>
                  <td>
                    {p.nombre}
                    {p.lleva_vencimiento && <span className="inv-badge-vencimiento">vence</span>}
                  </td>
                  <td>{p.categoria_nombre || '—'}</td>
                  <td>S/ {p.precio.toFixed(2)}</td>
                  <td className={p.stock <= p.stock_minimo ? 'inv-stock-bajo' : ''}>
                    {p.stock} {p.stock <= p.stock_minimo && '⚠'}
                  </td>
                  <td>{p.unidad_medida}</td>
                  <td>
                    <button className="inv-boton-editar" onClick={() => abrirEdicion(p)}>
                      Editar
                    </button>
                    <button className="inv-boton-eliminar" onClick={() => eliminarOProducto(p)}>
                      Eliminar
                    </button>
                  </td>
                </tr>
              ))}
              {productosFiltrados.length === 0 && (
                <tr>
                  <td colSpan={8} className="inv-sin-resultados">
                    No hay productos que coincidan.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {mostrarForm && (
        <div className="inv-modal-overlay" onClick={cerrarForm}>
          <div className="inv-modal" onClick={(e) => e.stopPropagation()}>
            <h2>{editandoId ? 'Editar producto' : 'Nuevo producto'}</h2>

            <div className="inv-campo inv-campo-imagen">
              <label>Foto del producto (opcional)</label>
              <div className="inv-imagen-selector">
                {imagenPreview ? (
                  <img src={imagenPreview} alt="Vista previa" className="inv-imagen-preview" />
                ) : (
                  <div className="inv-imagen-preview inv-imagen-preview-vacia">📦</div>
                )}
                <label className="inv-boton-subir-imagen">
                  {imagenPreview ? 'Cambiar foto' : 'Elegir foto'}
                  <input type="file" accept="image/*" onChange={manejarSeleccionImagen} hidden />
                </label>
              </div>
              <p className="inv-imagen-nota">Se optimiza automáticamente al guardar (máx. 800px, comprimida).</p>
            </div>

            <div className="inv-form-grid">
              <div className="inv-campo">
                <label>Código</label>
                <div className="inv-campo-codigo-fila">
                  <input value={form.codigo} onChange={(e) => cambiarCampo('codigo', e.target.value)} />
                  <button
                    type="button"
                    className="inv-boton-escanear"
                    onClick={() => {
                      setMensaje(null);
                      setEscanerCodigoAbierto(true);
                    }}
                    aria-label="Escanear código de barras"
                  >
                    📷
                  </button>
                </div>
              </div>
              <div className="inv-campo">
                <label>Nombre</label>
                <input value={form.nombre} onChange={(e) => cambiarCampo('nombre', e.target.value)} />
              </div>
              <div className="inv-campo inv-campo-full">
                <label>Descripción (opcional)</label>
                <input value={form.descripcion} onChange={(e) => cambiarCampo('descripcion', e.target.value)} />
              </div>
              <div className="inv-campo">
                <label>Categoría</label>
                <select value={form.categoria_id} onChange={(e) => cambiarCampo('categoria_id', e.target.value)}>
                  <option value="">Selecciona...</option>
                  {categorias.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.nombre}
                    </option>
                  ))}
                </select>
              </div>
              <div className="inv-campo">
                <label>Unidad de medida</label>
                <select value={form.unidad_medida} onChange={(e) => cambiarCampo('unidad_medida', e.target.value)}>
                  <option value="UNIDAD">Unidad</option>
                  <option value="KG">Kilogramo</option>
                  <option value="GRAMO">Gramo</option>
                  <option value="LITRO">Litro</option>
                  <option value="ML">Mililitro</option>
                  <option value="PAQUETE">Paquete</option>
                </select>
              </div>
              <div className="inv-campo">
                <label>Precio de venta (S/)</label>
                <input type="number" value={form.precio} onChange={(e) => cambiarCampo('precio', e.target.value)} />
              </div>
              <div className="inv-campo">
                <label>Precio de compra (S/, opcional)</label>
                <input
                  type="number"
                  value={form.precio_compra}
                  onChange={(e) => cambiarCampo('precio_compra', e.target.value)}
                />
              </div>

              {!form.lleva_vencimiento && (
                <div className="inv-campo">
                  <label>Stock {editandoId ? '' : 'inicial'}</label>
                  <input type="number" value={form.stock} onChange={(e) => cambiarCampo('stock', e.target.value)} />
                </div>
              )}

              <div className="inv-campo">
                <label>Stock mínimo (alerta)</label>
                <input
                  type="number"
                  value={form.stock_minimo}
                  onChange={(e) => cambiarCampo('stock_minimo', e.target.value)}
                />
              </div>

              <div className="inv-campo inv-campo-checkbox">
                <label>
                  <input
                    type="checkbox"
                    checked={form.lleva_vencimiento}
                    onChange={(e) => cambiarCampo('lleva_vencimiento', e.target.checked)}
                  />
                  Es perecible (maneja lotes con fecha de vencimiento)
                </label>
              </div>
            </div>

            {form.lleva_vencimiento && !editandoId && (
              <div className="inv-lote-caja">
                <p className="inv-lote-titulo">Primer lote de este producto</p>
                <p className="inv-lote-nota">
                  El stock de productos perecibles se calcula por lotes, no se escribe a mano.
                </p>
                <div className="inv-lote-form">
                  <div className="inv-campo">
                    <label>Cantidad</label>
                    <input
                      type="number"
                      value={loteInicialCantidad}
                      onChange={(e) => setLoteInicialCantidad(e.target.value)}
                    />
                  </div>
                  <div className="inv-campo">
                    <label>Fecha de vencimiento</label>
                    <input
                      type="date"
                      value={loteInicialFecha}
                      onChange={(e) => setLoteInicialFecha(e.target.value)}
                    />
                  </div>
                </div>
              </div>
            )}

            {form.lleva_vencimiento && editandoId && (
              <div className="inv-lote-caja">
                <p className="inv-lote-titulo">Lotes de este producto</p>
                {cargandoLotes ? (
                  <p className="inv-lote-nota">Cargando lotes...</p>
                ) : (
                  <>
                    {lotesProducto.length === 0 ? (
                      <p className="inv-lote-nota">Este producto aún no tiene lotes registrados.</p>
                    ) : (
                      <div className="inv-lote-lista">
                        {lotesProducto.map((l) => (
                          <div key={l.id} className="inv-lote-item">
                            <span>{l.cantidad} unid.</span>
                            <span>Vence: {l.fecha_vencimiento}</span>
                            {l.numero_lote && <span className="inv-lote-codigo">{l.numero_lote}</span>}
                          </div>
                        ))}
                      </div>
                    )}

                    <p className="inv-lote-subtitulo">Agregar nuevo lote (ej: nueva mercadería recibida)</p>
                    <div className="inv-lote-form">
                      <div className="inv-campo">
                        <label>Cantidad</label>
                        <input
                          type="number"
                          value={nuevoLoteCantidad}
                          onChange={(e) => setNuevoLoteCantidad(e.target.value)}
                        />
                      </div>
                      <div className="inv-campo">
                        <label>Fecha de vencimiento</label>
                        <input
                          type="date"
                          value={nuevoLoteFecha}
                          onChange={(e) => setNuevoLoteFecha(e.target.value)}
                        />
                      </div>
                    </div>
                    <button
                      className="inv-boton-agregar-lote"
                      onClick={agregarLoteAProductoExistente}
                      disabled={agregandoLote}
                    >
                      {agregandoLote ? 'Agregando...' : '+ Agregar lote'}
                    </button>
                  </>
                )}
              </div>
            )}

            {mensaje && <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

            <div className="inv-modal-acciones">
              <button className="inv-boton-cancelar" onClick={cerrarForm}>
                Cancelar
              </button>
              <button className="inv-boton-guardar" onClick={validarYGuardar} disabled={guardando}>
                {guardando ? 'Guardando...' : editandoId ? 'Guardar cambios' : 'Crear producto'}
              </button>
            </div>
          </div>
        </div>
      )}

      {escanerCodigoAbierto && (
        <EscanerCodigoBarras
          cerrarAlDetectar
          onCodigoDetectado={(codigo) => cambiarCampo('codigo', codigo)}
          onCerrar={() => setEscanerCodigoAbierto(false)}
        />
      )}
    </div>
  );
}