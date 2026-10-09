export const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000';
const STORAGE_KEY = 'minimarket_sesion';

function obtenerToken() {
  try {
    const sesion = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    return sesion?.token || null;
  } catch {
    return null;
  }
}

async function leerRespuesta(res) {
  const isJson = res.headers.get('content-type')?.includes('application/json');
  return isJson ? res.json().catch(() => null) : res.text();
}

async function request(path, options = {}) {
  const token = obtenerToken();
  const headers = {
    'Content-Type': 'application/json',
    ...(options.headers || {}),
  };
  if (token) headers['Authorization'] = `Bearer ${token}`;

  const res = await fetch(`${API_URL}${path}`, { ...options, headers });

  if (res.status === 401) {
    localStorage.removeItem(STORAGE_KEY);
    window.location.reload();
    throw new Error('Tu sesión expiró. Vuelve a iniciar sesión.');
  }

  const data = await leerRespuesta(res);

  if (!res.ok) {
    const mensaje = typeof data === 'string' ? data : data?.message || 'Error en la solicitud';
    throw new Error(mensaje);
  }
  return data;
}

export const api = {
  login: async (usuario, password, tienda) => {
    const res = await fetch(`${API_URL}/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ usuario, password, tienda: tienda || null }),
    });
    const data = await leerRespuesta(res);
    if (!res.ok) {
      const mensaje = typeof data === 'string' ? data : data?.message;
      throw new Error(mensaje || 'Usuario o contraseña incorrectos');
    }
    return data;
  },
  identificarUsuario: async (usuario) => {
    const res = await fetch(`${API_URL}/login/identificar`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ usuario }),
    });
    const data = await leerRespuesta(res);
    if (!res.ok) {
      const mensaje = typeof data === 'string' ? data : data?.message;
      throw new Error(mensaje || 'Usuario o contraseña incorrectos');
    }
    return data;
  },
  registro: async ({ nombre_negocio, nombre_completo, usuario, password, ruc, modo_negocio, rubro }) => {
    const res = await fetch(`${API_URL}/registro`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        nombre_negocio,
        nombre_completo,
        usuario,
        password,
        ruc: ruc || null,
        modo_negocio: modo_negocio || 'TIENDA',
        rubro: rubro || null,
      }),
    });
    const data = await leerRespuesta(res);
    if (!res.ok) {
      const mensaje = typeof data === 'string' ? data : data?.message;
      throw new Error(mensaje || 'No se pudo registrar el negocio');
    }
    return data;
  },
  verificarUsuario: async (usuario) => {
    const res = await fetch(`${API_URL}/registro/verificar-usuario?usuario=${encodeURIComponent(usuario)}`);
    const data = await leerRespuesta(res);
    if (!res.ok || typeof data !== 'object' || data === null) {
      return { disponible: null };
    }
    return data;
  },
  productos: () => request('/productos'),
  clientesBuscar: (q) => request(`/clientes?q=${encodeURIComponent(q)}`),
  clienteCrear: (cliente) => request('/clientes', { method: 'POST', body: JSON.stringify(cliente) }),
  clientesTodos: () => request('/clientes/todos'),
  clienteActualizar: (id, data) => request(`/clientes/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  clienteDesactivar: (id) => request(`/clientes/${id}/desactivar`, { method: 'POST' }),
  clientesDesactivados: () => request('/clientes/desactivados'),
  clienteReactivar: (id) => request(`/clientes/${id}/reactivar`, { method: 'POST' }),
  documentoConsultar: (tipo, numero) => request(`/documentos/consultar?tipo=${tipo}&numero=${encodeURIComponent(numero)}`),
  ventaCrear: (venta) => request('/ventas', { method: 'POST', body: JSON.stringify(venta) }),
  cajaAbrir: (data) => request('/cajas/abrir', { method: 'POST', body: JSON.stringify(data) }),
  cajaCerrar: (data) => request('/cajas/cerrar', { method: 'POST', body: JSON.stringify(data) }),
  comprobanteEmitir: (data) => request('/comprobantes', { method: 'POST', body: JSON.stringify(data) }),
  // Emisión directa: el documento tal como se envió a SUNAT (para el A4).
  comprobanteDocumento: (id) => request(`/comprobantes/${id}/documento`),
  // Emisión directa: reenviar un comprobante PENDIENTE y avisos de plazos.
  comprobanteReenviar: (id) => request(`/comprobantes/${id}/reenviar`, { method: 'POST' }),
  sunatAvisos: () => request('/sunat/avisos'),
  cajaAbierta: () => request('/cajas/abierta'),
  cajasListar: (inicio, fin) => request(`/cajas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  // Retiro o ingreso de efectivo en la caja abierta (los gastos van por /gastos).
  cajaMovimiento: (data) => request('/cajas/movimiento', { method: 'POST', body: JSON.stringify(data) }),
  cajaMovimientos: () => request('/cajas/movimientos'),
  // Módulo Gastos.
  gastos: (filtro = {}) => {
    const q = new URLSearchParams(Object.entries(filtro).filter(([, v]) => v !== '' && v != null && v !== false));
    return request(`/gastos${q.toString() ? `?${q}` : ''}`);
  },
  gastoRegistrar: (data) => request('/gastos', { method: 'POST', body: JSON.stringify(data) }),
  gastoAnular: (id, motivo) => request(`/gastos/${id}/anular`, { method: 'POST', body: JSON.stringify({ motivo }) }),
  gastoCategorias: () => request('/gastos/categorias'),
  gastoCategoriaCrear: (nombre) => request('/gastos/categorias', { method: 'POST', body: JSON.stringify({ nombre }) }),
  gastoCategoriaActualizar: (id, cambios) =>
    request(`/gastos/categorias/${id}`, { method: 'PUT', body: JSON.stringify(cambios) }),
  categorias: () => request('/categorias'),
  categoriaCrear: (data) => request('/categorias', { method: 'POST', body: JSON.stringify(data) }),
  productosStockBajo: () => request('/productos/stock-bajo'),
  productoCrear: (data) => request('/productos', { method: 'POST', body: JSON.stringify(data) }),
  productoActualizar: (id, data) => request(`/productos/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  // Importar desde Excel/CSV (solo administrador). solo_revisar: true = vista previa.
  productosImportar: (data) => request('/productos/importar', { method: 'POST', body: JSON.stringify(data) }),
  productoEliminar: (id) => request(`/productos/${id}`, { method: 'DELETE' }),
  productoDesactivar: (id) => request(`/productos/${id}/desactivar`, { method: 'POST' }),
  productosDesactivados: () => request('/productos/desactivados'),
  productoReactivar: (id) => request(`/productos/${id}/reactivar`, { method: 'POST' }),
  lotesDeProducto: (id) => request(`/productos/${id}/lotes`),
  loteCrear: (data) => request('/lotes', { method: 'POST', body: JSON.stringify(data) }),
  lotesPorVencer: (dias) => request(`/lotes/por-vencer?dias=${dias}`),
  loteDescartar: (id) => request(`/lotes/${id}/descartar`, { method: 'POST' }),
  proveedores: () => request('/proveedores'),
  proveedorCrear: (data) => request('/proveedores', { method: 'POST', body: JSON.stringify(data) }),
  compras: () => request('/compras'),
  compraDetalle: (id) => request(`/compras/${id}`),
  compraCrear: (data) => request('/compras', { method: 'POST', body: JSON.stringify(data) }),
  compraRecibir: (data) => request('/compras/recibir', { method: 'POST', body: JSON.stringify(data) }),
  devolucionProveedorRegistrar: (data) => request('/devoluciones-proveedor', { method: 'POST', body: JSON.stringify(data) }),
  devolucionesProveedorListar: () => request('/devoluciones-proveedor'),
  devolucionProveedorResolver: (id, data) => request(`/devoluciones-proveedor/${id}/resolver`, { method: 'POST', body: JSON.stringify(data) }),
  ventaParaDevolucion: (identificador) => request(`/ventas/${encodeURIComponent(identificador)}`),
  devolucionCrear: (data) => request('/devoluciones', { method: 'POST', body: JSON.stringify(data) }),
  configuracionObtener: () => request('/configuracion'),
  configuracionActualizar: (data) => request('/configuracion', { method: 'PUT', body: JSON.stringify(data) }),
  configuracionSubirLogo: async (archivo) => {
    const token = obtenerToken();
    const formData = new FormData();
    formData.append('logo', archivo);

    const res = await fetch(`${API_URL}/configuracion/logo`, {
      method: 'POST',
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      body: formData,
    });

    const data = await res.json().catch(() => null);
    if (!res.ok) throw new Error(data?.message || 'Error al subir el logo');
    return data;
  },
  usuariosListar: () => request('/usuarios'),
  usuarioCrear: (data) => request('/usuarios', { method: 'POST', body: JSON.stringify(data) }),
  usuarioDesactivar: (id) => request(`/usuarios/${id}/desactivar`, { method: 'POST' }),
  usuarioReactivar: (id) => request(`/usuarios/${id}/reactivar`, { method: 'POST' }),
  roles: () => request('/roles'),
  modoNegocioCambiar: (modo_negocio) =>
    request('/configuracion/modo-negocio', { method: 'PUT', body: JSON.stringify({ modo_negocio }) }),
  // --- Cafetería / Restaurante (atención en mesas) ---
  mesas: () => request('/mesas'),
  mesaCrear: (data) => request('/mesas', { method: 'POST', body: JSON.stringify(data) }),
  mesaActualizar: (id, data) => request(`/mesas/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  mesaQuitar: (id) => request(`/mesas/${id}/desactivar`, { method: 'POST' }),
  pedidosAbiertos: () => request('/pedidos/abiertos'),
  pedidoAbrir: (data) => request('/pedidos', { method: 'POST', body: JSON.stringify(data) }),
  pedido: (id) => request(`/pedidos/${id}`),
  pedidoAgregarItems: (id, items) => request(`/pedidos/${id}/items`, { method: 'POST', body: JSON.stringify({ items }) }),
  // nota: undefined = no tocarla; '' = borrarla.
  pedidoCambiarCantidad: (id, itemId, cantidad, nota) =>
    request(`/pedidos/${id}/items/${itemId}`, {
      method: 'PUT',
      body: JSON.stringify(nota === undefined ? { cantidad } : { cantidad, nota }),
    }),
  pedidoQuitarItem: (id, itemId, motivo) =>
    request(`/pedidos/${id}/items/${itemId}/quitar`, { method: 'POST', body: JSON.stringify({ motivo: motivo || null }) }),
  pedidoEnviar: (id) => request(`/pedidos/${id}/enviar`, { method: 'POST' }),
  pedidoMover: (id, mesa_id) => request(`/pedidos/${id}/mover`, { method: 'POST', body: JSON.stringify({ mesa_id }) }),
  pedidoAnular: (id, motivo) => request(`/pedidos/${id}/anular`, { method: 'POST', body: JSON.stringify({ motivo: motivo || null }) }),
  // Barra / cocina: lo que está en preparación o listo sin entregar.
  preparacion: () => request('/preparacion'),
  preparacionListo: (item_ids, listo = true) =>
    request('/preparacion/listo', { method: 'POST', body: JSON.stringify({ item_ids, listo }) }),
  preparacionEntregado: (item_ids) =>
    request('/preparacion/entregado', { method: 'POST', body: JSON.stringify({ item_ids }) }),
  modificadores: () => request('/modificadores'),
  negocioGuardar: (rubro, modulos) =>
    request('/configuracion/negocio', { method: 'PUT', body: JSON.stringify({ rubro, modulos }) }),
  categoriaIgv: (id, afectacion_igv) =>
    request(`/categorias/${id}/igv`, { method: 'PUT', body: JSON.stringify({ afectacion_igv }) }),
  productoIgv: (id, afectacion_igv) =>
    request(`/productos/${id}/igv`, { method: 'PUT', body: JSON.stringify({ afectacion_igv }) }),
  igvVenta: (ventaId) => request(`/igv/venta/${ventaId}`),
  detraccion: () => request('/detraccion'),
  // Cotizaciones (módulo COTIZACIONES)
  cotizaciones: (estado) => request(`/cotizaciones${estado ? `?estado=${estado}` : ''}`),
  cotizacion: (id) => request(`/cotizaciones/${id}`),
  cotizacionCrear: (datos) => request('/cotizaciones', { method: 'POST', body: JSON.stringify(datos) }),
  cotizacionAnular: (id) => request(`/cotizaciones/${id}/anular`, { method: 'POST' }),
  // Ventas al crédito (módulo CREDITO)
  creditos: (estado) => request(`/creditos${estado ? `?estado=${estado}` : ''}`),
  credito: (id) => request(`/creditos/${id}`),
  creditoAbonar: (id, datos) => request(`/creditos/${id}/abonos`, { method: 'POST', body: JSON.stringify(datos) }),
  creditoDeudaCliente: (clienteId) => request(`/creditos/cliente/${clienteId}`),
  // Guías de remisión (módulo GUIAS)
  guias: () => request('/guias'),
  guiasConfig: () => request('/guias/config'),
  guiasConfigGuardar: (datos) => request('/configuracion/guias', { method: 'PUT', body: JSON.stringify(datos) }),
  guia: (id) => request(`/guias/${id}`),
  guiaVenta: (folio) => request(`/guias/venta/${encodeURIComponent(folio)}`),
  guiaCrear: (datos) => request('/guias', { method: 'POST', body: JSON.stringify(datos) }),
  guiaConsultar: (id) => request(`/guias/${id}/consultar`, { method: 'POST' }),
  // Guía emitida directo a SUNAT: XML firmado ('xml') o constancia ('cdr').
  guiaArchivo: (id, cual) => descargarArchivoSunat(`/guias/${id}/${cual}`),
  // Ropa y calzado: tallas y colores (módulo VARIANTES) y cambio de prenda (módulo CAMBIOS)
  modeloCrear: (datos) => request('/modelos', { method: 'POST', body: JSON.stringify(datos) }),
  modeloActualizar: (id, datos) => request(`/modelos/${id}`, { method: 'PUT', body: JSON.stringify(datos) }),
  modeloCompartirImagen: (id, productoId) => request(`/modelos/${id}/imagen/${productoId}`, { method: 'POST' }),
  cambiosConfig: () => request('/cambios/config'),
  cambiosConfigGuardar: (dias) => request('/configuracion/cambios', { method: 'PUT', body: JSON.stringify({ dias }) }),
  cambioVenta: (identificador) => request(`/cambios/venta/${encodeURIComponent(identificador)}`),
  detraccionGuardar: (datos) => request('/configuracion/detraccion', { method: 'PUT', body: JSON.stringify(datos) }),
  unidades: () => request('/unidades'),
  unidadesGuardar: (activas) => request('/configuracion/unidades', { method: 'PUT', body: JSON.stringify({ activas }) }),
  cartaDia: () => request('/carta-dia'),
  cartaAgregar: (nombre, precio) => request('/carta-dia', { method: 'POST', body: JSON.stringify({ nombre, precio }) }),
  cartaActualizar: (id, cambios) => request(`/carta-dia/${id}`, { method: 'PUT', body: JSON.stringify(cambios) }),
  cartaQuitar: (id) => request(`/carta-dia/${id}/quitar`, { method: 'POST' }),
  modificadorCrear: (data) => request('/modificadores', { method: 'POST', body: JSON.stringify(data) }),
  modificadorActualizar: (id, data) => request(`/modificadores/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  modificadorQuitar: (id) => request(`/modificadores/${id}/desactivar`, { method: 'POST' }),
  canjearCodigo: (codigo) => request('/suscripcion/canjear-codigo', { method: 'POST', body: JSON.stringify({ codigo }) }),
  suscripcionEstado: () => request('/suscripcion/estado'),
  comprobantePublico: async (identificador, id) => {
    const res = await fetch(`${API_URL}/publico/comprobante/${encodeURIComponent(identificador)}/${id}`);
    const data = await leerRespuesta(res);
    if (!res.ok) {
      const mensaje = typeof data === 'string' ? data : data?.message;
      throw new Error(mensaje || 'No se pudo cargar el comprobante');
    }
    return data;
  },
  reportesVentas: (inicio, fin) => request(`/reportes/ventas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  reportesProductosVendidos: (inicio, fin) => request(`/reportes/productos-vendidos?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  reportesEstadisticas: (inicio, fin) => request(`/reportes/estadisticas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  // Módulo "Reporte de ganancias" (solo administrador). mes: "2026-10".
  reportesGanancias: (mes) => request(`/reportes/ganancias${mes ? `?mes=${mes}` : ''}`),
  // Costo promedio de cada producto: [{ producto_id, costo_promedio }]
  gananciasCostos: () => request('/ganancias/costos'),
  comprobantesListar: (filtros = {}) => {
    const params = new URLSearchParams(filtros).toString();
    return request(`/comprobantes${params ? `?${params}` : ''}`);
  },
  comprobantePdfUrl: (id) => {
    const token = obtenerToken();
    return `${API_URL}/comprobantes/${id}/pdf${token ? `?token=${encodeURIComponent(token)}` : ''}`;
  },
  // XML firmado ('xml') o constancia de SUNAT ('cdr') de un comprobante.
  // Devuelve el archivo y su extensión: el CDR puede llegar como XML o ZIP.
  comprobanteArchivo: (id, cual) => descargarArchivoSunat(`/comprobantes/${id}/${cual}`),
  // Notas de crédito (emisión directa).
  notaCreditoPreparar: (comprobanteId) => request(`/comprobantes/${comprobanteId}/nota-credito`),
  notaCreditoEmitir: (comprobanteId, data) =>
    request(`/comprobantes/${comprobanteId}/nota-credito`, { method: 'POST', body: JSON.stringify(data) }),
  notaCreditoReenviar: (id) => request(`/notas-credito/${id}/reenviar`, { method: 'POST' }),
  notaCreditoDocumento: (id) => request(`/notas-credito/${id}/documento`),
  notaCreditoArchivo: (id, cual) => descargarArchivoSunat(`/notas-credito/${id}/${cual}`),
  // Anulación (baja de facturas, resumen diario de boletas).
  comprobanteAnulacion: (id) => request(`/comprobantes/${id}/anulacion`),
  comprobanteAnular: (id, data) => request(`/comprobantes/${id}/anular`, { method: 'POST', body: JSON.stringify(data) }),
  comprobanteAnulacionConsultar: (id) => request(`/comprobantes/${id}/anulacion/consultar`, { method: 'POST' }),
  productoSubirImagen: async (id, archivo) => {
    const token = obtenerToken();
    const formData = new FormData();
    formData.append('imagen', archivo);

    const res = await fetch(`${API_URL}/productos/${id}/imagen`, {
      method: 'POST',
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      body: formData,
    });

    const data = await res.json().catch(() => null);
    if (!res.ok) throw new Error(data?.message || 'Error al subir la imagen');
    return data;
  },
};

// XML firmado ('xml') o constancia de SUNAT ('cdr') de un comprobante o de
// una nota de crédito. Devuelve el archivo y su extensión: el CDR puede
// llegar como XML o ZIP.
async function descargarArchivoSunat(ruta) {
  const token = obtenerToken();
  const res = await fetch(`${API_URL}${ruta}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (res.status === 401) {
    localStorage.removeItem(STORAGE_KEY);
    window.location.reload();
    throw new Error('Tu sesión expiró. Vuelve a iniciar sesión.');
  }
  if (!res.ok) {
    const texto = await res.text().catch(() => '');
    // Un servidor que aún no tiene esta descarga responde 404 sin texto.
    throw new Error(texto && texto.length < 200 ? texto : 'No se pudo descargar el archivo. Intenta de nuevo en un momento.');
  }
  const tipo = res.headers.get('content-type') || '';
  return { blob: await res.blob(), extension: tipo.includes('zip') ? 'zip' : 'xml' };
}
