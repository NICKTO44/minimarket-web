import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import './Proveedores.css';


const MOTIVOS = [
  { value: 'DAÑADO', label: 'Dañado' },
  { value: 'DEFECTUOSO', label: 'Defectuoso' },
  { value: 'PRODUCTO_INCORRECTO', label: 'Producto incorrecto' },
  { value: 'VENCIDO', label: 'Vencido' },
  { value: 'OTRO', label: 'Otro' },
];

export default function Proveedores() {
  const [vista, setVista] = useState('COMPRAS');
  const [proveedores, setProveedores] = useState([]);
  const [compras, setCompras] = useState([]);
  const [productos, setProductos] = useState([]);
  const [categorias, setCategorias] = useState([]);
  const [devoluciones, setDevoluciones] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);

  const [mostrarNuevoProveedor, setMostrarNuevoProveedor] = useState(false);
  const [formProveedor, setFormProveedor] = useState({ nombre: '', telefono: '', tipo_documento: 'RUC', numero_documento: '' });
  const [guardandoProveedor, setGuardandoProveedor] = useState(false);

  const [mostrarNuevaCompra, setMostrarNuevaCompra] = useState(false);
  const [compraProveedorId, setCompraProveedorId] = useState('');
  const [compraItems, setCompraItems] = useState([]);
  const [compraProductoSel, setCompraProductoSel] = useState('');
  const [compraCantidad, setCompraCantidad] = useState('');
  const [compraPrecio, setCompraPrecio] = useState('');
  const [compraTipoPago, setCompraTipoPago] = useState('EFECTIVO');
  const [guardandoCompra, setGuardandoCompra] = useState(false);

  const [compraSeleccionada, setCompraSeleccionada] = useState(null);
  const [itemsRecepcion, setItemsRecepcion] = useState([]);
  const [recibiendo, setRecibiendo] = useState(false);

  const [mostrarNuevoProductoEnCompra, setMostrarNuevoProductoEnCompra] = useState(false);
  const [nuevoProdCodigo, setNuevoProdCodigo] = useState('');
  const [nuevoProdNombre, setNuevoProdNombre] = useState('');
  const [nuevoProdCategoria, setNuevoProdCategoria] = useState('');
  const [nuevoProdUnidad, setNuevoProdUnidad] = useState('UNIDAD');
  const [nuevoProdPrecioVenta, setNuevoProdPrecioVenta] = useState('');
  const [nuevoProdLleveVencimiento, setNuevoProdLleveVencimiento] = useState(false);
  const [nuevoProdCantidadCompra, setNuevoProdCantidadCompra] = useState('');
  const [nuevoProdPrecioCompra, setNuevoProdPrecioCompra] = useState('');
  const [creandoProdEnCompra, setCreandoProdEnCompra] = useState(false);

  const [compraParaDevolver, setCompraParaDevolver] = useState(null);
  const [itemsDevolucion, setItemsDevolucion] = useState([]);
  const [devMotivo, setDevMotivo] = useState('DAÑADO');
  const [devDetalleMotivo, setDevDetalleMotivo] = useState('');
  const [registrandoDevolucion, setRegistrandoDevolucion] = useState(false);

  const [devolucionAResolver, setDevolucionAResolver] = useState(null);
  const [resolverEstado, setResolverEstado] = useState('ACEPTADA');
  const [resolverTipo, setResolverTipo] = useState('CREDITO');
  const [resolverNotas, setResolverNotas] = useState('');
  const [resolviendo, setResolviendo] = useState(false);
  const [compraCreditoAplicado, setCompraCreditoAplicado] = useState('');

  const cargarTodo = () => {
    setCargando(true);
    Promise.all([api.proveedores(), api.compras(), api.productos(), api.categorias(), api.devolucionesProveedorListar()])
      .then(([p, c, prod, cats, devs]) => {
        setProveedores(p);
        setCompras(c);
        setProductos(prod);
        setCategorias(cats);
        setDevoluciones(devs);
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargarTodo();
  }, []);

  const guardarProveedor = async () => {
    if (!formProveedor.nombre.trim()) {
      setMensaje({ tipo: 'error', texto: 'El nombre del proveedor es obligatorio.' });
      return;
    }
    setGuardandoProveedor(true);
    try {
      await api.proveedorCrear(formProveedor);
      setMostrarNuevoProveedor(false);
      setFormProveedor({ nombre: '', telefono: '', tipo_documento: 'RUC', numero_documento: '' });
      setMensaje({ tipo: 'exito', texto: 'Proveedor registrado.' });
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoProveedor(false);
    }
  };

  const agregarItemCompra = () => {
    if (!compraProductoSel || !compraCantidad || !compraPrecio) {
      setMensaje({ tipo: 'error', texto: 'Selecciona producto, cantidad y precio.' });
      return;
    }
    const producto = productos.find((p) => p.id === parseInt(compraProductoSel, 10));
    setCompraItems((prev) => [
      ...prev,
      { producto_id: producto.id, nombre: producto.nombre, cantidad: parseFloat(compraCantidad), precio_compra: parseFloat(compraPrecio) },
    ]);
    setCompraProductoSel('');
    setCompraCantidad('');
    setCompraPrecio('');
  };

  const limpiarFormNuevoProducto = () => {
    setNuevoProdCodigo('');
    setNuevoProdNombre('');
    setNuevoProdCategoria('');
    setNuevoProdUnidad('UNIDAD');
    setNuevoProdPrecioVenta('');
    setNuevoProdLleveVencimiento(false);
    setNuevoProdCantidadCompra('');
    setNuevoProdPrecioCompra('');
  };

  const crearProductoYAgregarloACompra = async () => {
    if (!nuevoProdCodigo.trim() || !nuevoProdNombre.trim() || !nuevoProdCategoria || !nuevoProdPrecioVenta) {
      setMensaje({ tipo: 'error', texto: 'Completa código, nombre, categoría y precio de venta.' });
      return;
    }
    if (!nuevoProdCantidadCompra || !nuevoProdPrecioCompra) {
      setMensaje({ tipo: 'error', texto: 'Indica cuántas unidades llegan y a qué precio de compra.' });
      return;
    }
    setCreandoProdEnCompra(true);
    try {
      const creado = await api.productoCrear({
        codigo: nuevoProdCodigo.trim(),
        nombre: nuevoProdNombre.trim(),
        precio: parseFloat(nuevoProdPrecioVenta),
        stock: 0,
        stock_minimo: 5,
        unidad_medida: nuevoProdUnidad,
        categoria_id: parseInt(nuevoProdCategoria, 10),
        lleva_vencimiento: nuevoProdLleveVencimiento,
        precio_compra: parseFloat(nuevoProdPrecioCompra),
      });
      setCompraItems((prev) => [
        ...prev,
        { producto_id: creado.producto_id, nombre: nuevoProdNombre.trim(), cantidad: parseFloat(nuevoProdCantidadCompra), precio_compra: parseFloat(nuevoProdPrecioCompra) },
      ]);
      setMostrarNuevoProductoEnCompra(false);
      limpiarFormNuevoProducto();
      setMensaje({ tipo: 'exito', texto: `"${nuevoProdNombre}" creado y agregado a la compra.` });
      api.productos().then(setProductos);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setCreandoProdEnCompra(false);
    }
  };

  const quitarItemCompra = (idx) => setCompraItems((prev) => prev.filter((_, i) => i !== idx));

   const subtotalCompra = compraItems.reduce((sum, i) => sum + i.cantidad * i.precio_compra, 0);
   const totalCompra = Math.max(0, subtotalCompra - (parseFloat(compraCreditoAplicado) || 0));

  const guardarCompra = async () => {
    if (!compraProveedorId || compraItems.length === 0) {
      setMensaje({ tipo: 'error', texto: 'Elige un proveedor y agrega al menos un producto.' });
      return;
    }
    setGuardandoCompra(true);
    try {
       await api.compraCrear({
        proveedor_id: parseInt(compraProveedorId, 10),
        fecha_compra: new Date().toISOString().slice(0, 10),
        items: compraItems.map((i) => ({ producto_id: i.producto_id, cantidad: i.cantidad, precio_compra: i.precio_compra })),
        tipo_pago: compraTipoPago,
        credito_aplicado: parseFloat(compraCreditoAplicado) || 0,
        usuario_id: 1,
      });
      setMostrarNuevaCompra(false);
      setCompraProveedorId('');
      setCompraItems([]);
      setMensaje({ tipo: 'exito', texto: 'Compra registrada. Ahora puedes recibirla cuando llegue la mercadería.' });
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoCompra(false);
    }
  };

  const abrirRecepcion = async (compra) => {
    setMensaje(null);
    try {
      const detalle = await api.compraDetalle(compra.id);
      setCompraSeleccionada(detalle);
      setItemsRecepcion(
        detalle.items.map((it) => ({
          detalle_id: it.id,
          producto_nombre: it.producto_nombre,
          lleva_vencimiento: it.lleva_vencimiento,
          cantidad_pedida: it.cantidad,
          cantidad_recibida: String(it.cantidad),
          cantidad_conforme: String(it.cantidad),
          fecha_vencimiento: '',
        }))
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const cambiarItemRecepcion = (idx, campo, valor) => {
    setItemsRecepcion((prev) => prev.map((it, i) => (i === idx ? { ...it, [campo]: valor } : it)));
  };

  const confirmarRecepcion = async () => {
    for (const it of itemsRecepcion) {
      const conforme = parseFloat(it.cantidad_conforme) || 0;
      if (it.lleva_vencimiento && conforme > 0 && !it.fecha_vencimiento) {
        setMensaje({ tipo: 'error', texto: `Falta la fecha de vencimiento del lote para "${it.producto_nombre}".` });
        return;
      }
    }
    setRecibiendo(true);
    try {
      const resultado = await api.compraRecibir({
        compra_id: compraSeleccionada.id,
        items: itemsRecepcion.map((it) => ({
          detalle_id: it.detalle_id,
          cantidad_recibida: parseFloat(it.cantidad_recibida) || 0,
          cantidad_conforme: parseFloat(it.cantidad_conforme) || 0,
          fecha_vencimiento: it.lleva_vencimiento ? it.fecha_vencimiento : null,
        })),
      });
      setMensaje({ tipo: 'exito', texto: resultado.message });
      setCompraSeleccionada(null);
      setItemsRecepcion([]);
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setRecibiendo(false);
    }
  };

  const cerrarModalCompra = () => {
    setMostrarNuevaCompra(false);
    setMostrarNuevoProductoEnCompra(false);
    limpiarFormNuevoProducto();
    setCompraCreditoAplicado('');
  };
  const abrirDevolucion = async (compra) => {
    setMensaje(null);
    try {
      const detalle = await api.compraDetalle(compra.id);
      setCompraParaDevolver(detalle);
      setDevMotivo('DAÑADO');
      setDevDetalleMotivo('');
      setItemsDevolucion(
        detalle.items
          .map((it) => {
            const danado = it.cantidad_recibida - it.cantidad_conforme;
            const faltante = it.cantidad - it.cantidad_recibida;
            const total = danado + faltante;
            const partes = [];
            if (danado > 0.001) partes.push(`${danado} dañado(s)`);
            if (faltante > 0.001) partes.push(`${faltante} extraviado(s)`);
            return {
              detalle_compra_id: it.id,
              producto_id: it.producto_id,
              producto_nombre: it.producto_nombre,
              precio_compra: it.precio_compra,
              total,
              descripcion: partes.join(' + '),
            };
          })
          .filter((it) => it.total > 0.001)
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

 

  const registrarDevolucion = async () => {
        const items = itemsDevolucion.map((it) => ({
      detalle_compra_id: it.detalle_compra_id,
      producto_id: it.producto_id,
      cantidad_devuelta: it.total,
      precio_compra: it.precio_compra,
      motivo_item: it.descripcion,
    }));

    if (items.length === 0) {
      setMensaje({ tipo: 'error', texto: 'Indica cuántas unidades devuelves de al menos un producto.' });
      return;
    }

    setRegistrandoDevolucion(true);
    try {
      const resultado = await api.devolucionProveedorRegistrar({
        compra_id: compraParaDevolver.id,
        motivo: devMotivo,
        detalle_motivo: devDetalleMotivo || null,
        items,
        notas: null,
        usuario_id: 1,
      });
      setMensaje({ tipo: 'exito', texto: resultado.message });
      setCompraParaDevolver(null);
      setItemsDevolucion([]);
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setRegistrandoDevolucion(false);
    }
  };

  const abrirResolver = (dev) => {
    setDevolucionAResolver(dev);
    setResolverEstado('ACEPTADA');
    setResolverTipo('CREDITO');
    setResolverNotas('');
  };

  const confirmarResolucion = async () => {
    setResolviendo(true);
    try {
      const resultado = await api.devolucionProveedorResolver(devolucionAResolver.id, {
        estado: resolverEstado,
        tipo_resolucion: resolverEstado === 'ACEPTADA' ? resolverTipo : null,
        notas: resolverNotas || null,
      });
      setMensaje({ tipo: 'exito', texto: resultado.message });
      setDevolucionAResolver(null);
      cargarTodo();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setResolviendo(false);
    }
  };

  return (
    <div className="prov-layout">
      <div className="prov-header">
        <h1>Proveedores</h1>
        <div className="prov-tabs">
          <button className={vista === 'COMPRAS' ? 'activo' : ''} onClick={() => setVista('COMPRAS')}>Compras</button>
          <button className={vista === 'PROVEEDORES' ? 'activo' : ''} onClick={() => setVista('PROVEEDORES')}>Proveedores</button>
          <button className={vista === 'DEVOLUCIONES' ? 'activo' : ''} onClick={() => setVista('DEVOLUCIONES')}>Devoluciones</button>
        </div>
        {vista === 'COMPRAS' && (
          <button className="prov-boton-nuevo" onClick={() => setMostrarNuevaCompra(true)}>+ Nueva compra</button>
        )}
        {vista === 'PROVEEDORES' && (
          <button className="prov-boton-nuevo" onClick={() => setMostrarNuevoProveedor(true)}>+ Nuevo proveedor</button>
        )}
      </div>

      {mensaje && <p className={`prov-mensaje prov-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {cargando ? (
        <p className="prov-cargando">Cargando...</p>
      ) : vista === 'COMPRAS' ? (
        <div className="prov-tabla-wrapper">
          <table className="prov-tabla">
            <thead>
              <tr>
                <th>Folio</th><th>Proveedor</th><th>Fecha</th><th>Total</th><th>Estado</th><th>Pago</th><th></th>
              </tr>
            </thead>
            <tbody>
              {compras.map((c) => (
                <tr key={c.id}>
                  <td>{c.folio}</td>
                  <td>{c.proveedor_nombre}</td>
                  <td>{c.fecha_compra}</td>
                  <td>S/ {c.total.toFixed(2)}</td>
                 <td>
                         <span className={`prov-badge prov-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                            {(c.unidades_danadas + c.unidades_faltantes - c.unidades_ya_devueltas) > 0.001 && (
                            <span className="prov-nota-parcial">
                                {(c.unidades_danadas + c.unidades_faltantes - c.unidades_ya_devueltas).toFixed(0)} unidad(es) por reclamar
                            </span>
                            )}
                        </td>
                        <td>{c.estado_pago}</td>
                        <td>
                            {c.estado === 'PENDIENTE' && (
                            <button className="prov-boton-recibir" onClick={() => abrirRecepcion(c)}>Recibir</button>
                            )}
                            {(c.estado === 'RECIBIDA' || c.estado === 'PARCIAL') && ((c.unidades_danadas + c.unidades_faltantes - c.unidades_ya_devueltas) > 0.001) && (
                            <button className="prov-boton-devolver" onClick={() => abrirDevolucion(c)}>
                                Reclamar ({(c.unidades_danadas + c.unidades_faltantes - c.unidades_ya_devueltas).toFixed(0)})
                            </button>
                            )}
                   </td>
                </tr>
              ))}
              {compras.length === 0 && (
                <tr><td colSpan={7} className="prov-sin-resultados">No hay compras registradas.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      ) : vista === 'PROVEEDORES' ? (
        <div className="prov-tabla-wrapper">
          <table className="prov-tabla">
            <thead>
              <tr><th>Nombre</th><th>Teléfono</th><th>Documento</th><th>Crédito disponible</th></tr>
            </thead>
            <tbody>
              {proveedores.map((p) => (
                <tr key={p.id}>
                  <td>{p.nombre}</td>
                  <td>{p.telefono || '—'}</td>
                  <td>{p.tipo_documento} {p.numero_documento || ''}</td>
                  <td>{p.credito_disponible > 0 ? <strong className="prov-credito">S/ {p.credito_disponible.toFixed(2)}</strong> : '—'}</td>
                </tr>
              ))}
              {proveedores.length === 0 && (
                <tr><td colSpan={4} className="prov-sin-resultados">No hay proveedores registrados.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="prov-tabla-wrapper">
          <table className="prov-tabla">
            <thead>
              <tr><th>Folio</th><th>Proveedor</th><th>Fecha</th><th>Motivo</th><th>Monto</th><th>Estado</th><th></th></tr>
            </thead>
            <tbody>
              {devoluciones.map((d) => (
                <tr key={d.id}>
                  <td>{d.folio}</td>
                  <td>{d.proveedor_nombre}</td>
                  <td>{d.fecha}</td>
                  <td>{d.motivo}</td>
                  <td>S/ {d.monto_devolucion.toFixed(2)}</td>
                  <td>
                    <span className={`prov-badge prov-badge-${d.estado.toLowerCase()}`}>{d.estado}</span>
                    {d.tipo_resolucion && <span className="prov-nota-parcial">{d.tipo_resolucion}</span>}
                  </td>
                  <td>
                    {d.estado === 'PENDIENTE' && (
                      <button className="prov-boton-recibir" onClick={() => abrirResolver(d)}>Resolver</button>
                    )}
                  </td>
                </tr>
              ))}
              {devoluciones.length === 0 && (
                <tr><td colSpan={7} className="prov-sin-resultados">No hay devoluciones registradas.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {mostrarNuevoProveedor && (
        <div className="prov-modal-overlay" onClick={() => setMostrarNuevoProveedor(false)}>
          <div className="prov-modal" onClick={(e) => e.stopPropagation()}>
            <h2>Nuevo proveedor</h2>
            <div className="prov-campo">
              <label>Nombre</label>
              <input value={formProveedor.nombre} onChange={(e) => setFormProveedor({ ...formProveedor, nombre: e.target.value })} />
            </div>
            <div className="prov-campo">
              <label>Teléfono</label>
              <input value={formProveedor.telefono} onChange={(e) => setFormProveedor({ ...formProveedor, telefono: e.target.value })} />
            </div>
            <div className="prov-campo-fila">
              <div className="prov-campo">
                <label>Tipo</label>
                <select value={formProveedor.tipo_documento} onChange={(e) => setFormProveedor({ ...formProveedor, tipo_documento: e.target.value })}>
                  <option value="RUC">RUC</option>
                  <option value="DNI">DNI</option>
                </select>
              </div>
              <div className="prov-campo">
                <label>Número</label>
                <input value={formProveedor.numero_documento} onChange={(e) => setFormProveedor({ ...formProveedor, numero_documento: e.target.value })} />
              </div>
            </div>
            <div className="prov-modal-acciones">
              <button className="prov-boton-cancelar" onClick={() => setMostrarNuevoProveedor(false)}>Cancelar</button>
              <button className="prov-boton-guardar" onClick={guardarProveedor} disabled={guardandoProveedor}>
                {guardandoProveedor ? 'Guardando...' : 'Registrar'}
              </button>
            </div>
          </div>
        </div>
      )}

      {mostrarNuevaCompra && (
        <div className="prov-modal-overlay" onClick={cerrarModalCompra}>
          <div className="prov-modal prov-modal-grande" onClick={(e) => e.stopPropagation()}>
            <h2>Nueva compra</h2>
                      <div className="prov-campo">
              <label>Proveedor</label>
              <select value={compraProveedorId} onChange={(e) => { setCompraProveedorId(e.target.value); setCompraCreditoAplicado(''); }}>
                <option value="">Selecciona...</option>
                {proveedores.map((p) => <option key={p.id} value={p.id}>{p.nombre}</option>)}
              </select>
            </div>

            {compraProveedorId && (() => {
              const proveedorSel = proveedores.find((p) => p.id === parseInt(compraProveedorId, 10));
              const creditoDisp = proveedorSel?.credito_disponible || 0;
              if (creditoDisp <= 0) return null;
              return (
                <div className="prov-credito-caja">
                  <span>Crédito disponible con este proveedor: <strong>S/ {creditoDisp.toFixed(2)}</strong></span>
                  <div className="prov-campo">
                    <label>Aplicar crédito a esta compra (S/)</label>
                    <input
                      type="number"
                      max={creditoDisp}
                      value={compraCreditoAplicado}
                      onChange={(e) => {
                        const val = Math.min(parseFloat(e.target.value) || 0, creditoDisp);
                        setCompraCreditoAplicado(String(val));
                      }}
                    />
                  </div>
                </div>
              );
            })()}
            <div className="prov-campo">
              <label>Forma de pago</label>
              <select value={compraTipoPago} onChange={(e) => setCompraTipoPago(e.target.value)}>
                <option value="EFECTIVO">Efectivo</option>
                <option value="TRANSFERENCIA">Transferencia</option>
                <option value="CREDITO">Crédito</option>
              </select>
            </div>
            <div className="prov-agregar-item">
              <select value={compraProductoSel} onChange={(e) => setCompraProductoSel(e.target.value)}>
                <option value="">Producto existente...</option>
                {productos.map((p) => <option key={p.id} value={p.id}>{p.nombre}</option>)}
              </select>
              <input type="number" placeholder="Cant." value={compraCantidad} onChange={(e) => setCompraCantidad(e.target.value)} />
              <input type="number" placeholder="Precio" value={compraPrecio} onChange={(e) => setCompraPrecio(e.target.value)} />
              <button onClick={agregarItemCompra}>+ Agregar</button>
            </div>
            <button className="prov-boton-producto-nuevo" onClick={() => setMostrarNuevoProductoEnCompra((v) => !v)}>
              🆕 ¿No está en la lista? Crear producto nuevo
            </button>
            {mostrarNuevoProductoEnCompra && (
              <div className="prov-nuevo-prod-caja">
                <p className="prov-nuevo-prod-titulo">Crear producto nuevo</p>
                <div className="prov-campo-fila">
                  <div className="prov-campo">
                    <label>Código</label>
                    <input value={nuevoProdCodigo} onChange={(e) => setNuevoProdCodigo(e.target.value)} />
                  </div>
                  <div className="prov-campo">
                    <label>Nombre</label>
                    <input value={nuevoProdNombre} onChange={(e) => setNuevoProdNombre(e.target.value)} />
                  </div>
                </div>
                <div className="prov-campo-fila">
                  <div className="prov-campo">
                    <label>Categoría</label>
                    <select value={nuevoProdCategoria} onChange={(e) => setNuevoProdCategoria(e.target.value)}>
                      <option value="">Selecciona...</option>
                      {categorias.map((c) => <option key={c.id} value={c.id}>{c.nombre}</option>)}
                    </select>
                  </div>
                  <div className="prov-campo">
                    <label>Unidad</label>
                    <select value={nuevoProdUnidad} onChange={(e) => setNuevoProdUnidad(e.target.value)}>
                      <option value="UNIDAD">Unidad</option>
                      <option value="KG">Kilogramo</option>
                      <option value="GRAMO">Gramo</option>
                      <option value="LITRO">Litro</option>
                      <option value="ML">Mililitro</option>
                      <option value="PAQUETE">Paquete</option>
                    </select>
                  </div>
                </div>
                <div className="prov-campo-fila">
                  <div className="prov-campo">
                    <label>Precio de venta (S/)</label>
                    <input type="number" value={nuevoProdPrecioVenta} onChange={(e) => setNuevoProdPrecioVenta(e.target.value)} />
                  </div>
                  <div className="prov-campo prov-campo-checkbox-vert">
                    <label>
                      <input type="checkbox" checked={nuevoProdLleveVencimiento} onChange={(e) => setNuevoProdLleveVencimiento(e.target.checked)} />
                      Es perecible
                    </label>
                  </div>
                </div>
                <p className="prov-nuevo-prod-subtitulo">¿Cuánto llega en esta compra?</p>
                <div className="prov-campo-fila">
                  <div className="prov-campo">
                    <label>Cantidad</label>
                    <input type="number" value={nuevoProdCantidadCompra} onChange={(e) => setNuevoProdCantidadCompra(e.target.value)} />
                  </div>
                  <div className="prov-campo">
                    <label>Precio de compra (S/)</label>
                    <input type="number" value={nuevoProdPrecioCompra} onChange={(e) => setNuevoProdPrecioCompra(e.target.value)} />
                  </div>
                </div>
                <div className="prov-nuevo-prod-acciones">
                  <button onClick={() => setMostrarNuevoProductoEnCompra(false)}>Cancelar</button>
                  <button className="prov-boton-crear-agregar" onClick={crearProductoYAgregarloACompra} disabled={creandoProdEnCompra}>
                    {creandoProdEnCompra ? 'Creando...' : 'Crear y agregar a la compra'}
                  </button>
                </div>
              </div>
            )}
            <div className="prov-items-lista">
              {compraItems.map((it, idx) => (
                <div key={idx} className="prov-item-fila">
                  <span>{it.nombre}</span>
                  <span>{it.cantidad} x S/{it.precio_compra.toFixed(2)}</span>
                  <span>S/ {(it.cantidad * it.precio_compra).toFixed(2)}</span>
                  <button onClick={() => quitarItemCompra(idx)}>🗑</button>
                </div>
              ))}
              {compraItems.length === 0 && <p className="prov-items-vacio">Aún no agregas productos.</p>}
            </div>
                       {parseFloat(compraCreditoAplicado) > 0 && (
              <div className="prov-resumen-linea">
                <span>Subtotal</span>
                <span>S/ {subtotalCompra.toFixed(2)}</span>
                   </div>
            )}
                {parseFloat(compraCreditoAplicado) > 0 && (
                 <div className="prov-resumen-linea prov-resumen-credito">
                <span>Crédito aplicado</span>
                <span>- S/ {parseFloat(compraCreditoAplicado).toFixed(2)}</span>
                  </div>
                  )}
                 <div className="prov-total-compra">
                 <span>Total a pagar</span>
              <strong>S/ {totalCompra.toFixed(2)}</strong>
            </div>
            <div className="prov-modal-acciones">
              <button className="prov-boton-cancelar" onClick={cerrarModalCompra}>Cancelar</button>
              <button className="prov-boton-guardar" onClick={guardarCompra} disabled={guardandoCompra}>
                {guardandoCompra ? 'Guardando...' : 'Registrar compra'}
              </button>
            </div>
          </div>
        </div>
      )}

      {compraSeleccionada && (
        <div className="prov-modal-overlay" onClick={() => setCompraSeleccionada(null)}>
          <div className="prov-modal prov-modal-grande" onClick={(e) => e.stopPropagation()}>
            <h2>Recibir mercadería — {compraSeleccionada.folio}</h2>
            <p className="prov-recepcion-nota">Para productos perecibles, indica la fecha de vencimiento del lote que está llegando.</p>
            {itemsRecepcion.map((it, idx) => (
              <div key={it.detalle_id} className="prov-recepcion-item">
                <div className="prov-recepcion-nombre">
                  {it.producto_nombre}
                  {it.lleva_vencimiento && <span className="prov-badge-vencimiento">perecible</span>}
                </div>
                <div className="prov-recepcion-campos">
                  <div className="prov-campo"><label>Pedido</label><input value={it.cantidad_pedida} disabled /></div>
                  <div className="prov-campo">
                    <label>Recibido</label>
                    <input type="number" value={it.cantidad_recibida} onChange={(e) => cambiarItemRecepcion(idx, 'cantidad_recibida', e.target.value)} />
                  </div>
                  <div className="prov-campo">
                    <label>Conforme</label>
                    <input type="number" value={it.cantidad_conforme} onChange={(e) => cambiarItemRecepcion(idx, 'cantidad_conforme', e.target.value)} />
                  </div>
                  {it.lleva_vencimiento && (
                    <div className="prov-campo">
                      <label>Vencimiento</label>
                      <input type="date" value={it.fecha_vencimiento} onChange={(e) => cambiarItemRecepcion(idx, 'fecha_vencimiento', e.target.value)} />
                    </div>
                  )}
                </div>
              </div>
            ))}
            <div className="prov-modal-acciones">
              <button className="prov-boton-cancelar" onClick={() => setCompraSeleccionada(null)}>Cancelar</button>
              <button className="prov-boton-guardar" onClick={confirmarRecepcion} disabled={recibiendo}>
                {recibiendo ? 'Procesando...' : 'Confirmar recepción'}
              </button>
            </div>
          </div>
        </div>
      )}

      {compraParaDevolver && (
        <div className="prov-modal-overlay" onClick={() => setCompraParaDevolver(null)}>
          <div className="prov-modal prov-modal-grande" onClick={(e) => e.stopPropagation()}>
            <h2>Devolver a proveedor — {compraParaDevolver.folio}</h2>
             <p className="prov-recepcion-nota">
              Incluye lo dañado (que sí tienes en mano) y lo extraviado (que nunca llegó) — en ambos casos ya pagaste por esas unidades, así que el crédito debe cubrir el total.
            </p>

            <div className="prov-campo-fila">
              <div className="prov-campo">
                <label>Motivo</label>
                <select value={devMotivo} onChange={(e) => setDevMotivo(e.target.value)}>
                  {MOTIVOS.map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}
                </select>
              </div>
              <div className="prov-campo">
                <label>Detalle (opcional)</label>
                <input value={devDetalleMotivo} onChange={(e) => setDevDetalleMotivo(e.target.value)} />
              </div>
            </div>

            {itemsDevolucion.map((it) => (
              <div key={it.detalle_compra_id} className="prov-recepcion-item">
                <div className="prov-recepcion-nombre">{it.producto_nombre}</div>
                <div className="prov-danado-fijo">
                  {it.descripcion} — total a reclamar: {it.total} unidad(es)
                </div>
              </div>
            ))}
            {itemsDevolucion.length === 0 && (
              <p className="prov-items-vacio">Esta compra no tiene nada pendiente de reclamar.</p>
            )}
            <div className="prov-modal-acciones">
              <button className="prov-boton-cancelar" onClick={() => setCompraParaDevolver(null)}>Cancelar</button>
              <button className="prov-boton-guardar" onClick={registrarDevolucion} disabled={registrandoDevolucion}>
                {registrandoDevolucion ? 'Registrando...' : 'Registrar devolución'}
              </button>
            </div>
          </div>
        </div>
      )}

      {devolucionAResolver && (
        <div className="prov-modal-overlay" onClick={() => setDevolucionAResolver(null)}>
          <div className="prov-modal" onClick={(e) => e.stopPropagation()}>
            <h2>Resolver {devolucionAResolver.folio}</h2>
            <p className="prov-recepcion-nota">S/ {devolucionAResolver.monto_devolucion.toFixed(2)} — {devolucionAResolver.motivo}</p>

            <div className="prov-campo">
              <label>¿Qué respondió el proveedor?</label>
              <select value={resolverEstado} onChange={(e) => setResolverEstado(e.target.value)}>
                <option value="ACEPTADA">Aceptó la devolución</option>
                <option value="RECHAZADA">Rechazó la devolución</option>
              </select>
            </div>

            {resolverEstado === 'ACEPTADA' && (
              <div className="prov-campo">
                <label>¿Cómo resuelve?</label>
                <select value={resolverTipo} onChange={(e) => setResolverTipo(e.target.value)}>
                  <option value="CREDITO">Crédito para próxima compra</option>
                  <option value="REEMBOLSO">Reembolso de dinero</option>
                  <option value="CAMBIO">Cambio por mercadería nueva</option>
                </select>
              </div>
            )}

            <div className="prov-campo">
              <label>Notas (opcional)</label>
              <input value={resolverNotas} onChange={(e) => setResolverNotas(e.target.value)} />
            </div>

            <div className="prov-modal-acciones">
              <button className="prov-boton-cancelar" onClick={() => setDevolucionAResolver(null)}>Cancelar</button>
              <button className="prov-boton-guardar" onClick={confirmarResolucion} disabled={resolviendo}>
                {resolviendo ? 'Guardando...' : 'Confirmar'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
