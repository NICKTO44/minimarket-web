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

// Los endpoints públicos (login/registro/verificar-usuario) devuelven el
// error a veces como JSON ({ message }) y a veces como texto plano, según
// el tipo de StatusCode que use el handler en el backend. Esta función lee
// cualquiera de los dos formatos sin romperse.
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
  registro: async ({ nombre_negocio, nombre_completo, usuario, password, ruc }) => {
    const res = await fetch(`${API_URL}/registro`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ nombre_negocio, nombre_completo, usuario, password, ruc: ruc || null }),
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
  ventaCrear: (venta) => request('/ventas', { method: 'POST', body: JSON.stringify(venta) }),
  cajaAbrir: (data) => request('/cajas/abrir', { method: 'POST', body: JSON.stringify(data) }),
  cajaCerrar: (data) => request('/cajas/cerrar', { method: 'POST', body: JSON.stringify(data) }),
  comprobanteEmitir: (data) => request('/comprobantes', { method: 'POST', body: JSON.stringify(data) }),
  cajaAbierta: () => request('/cajas/abierta'),
  cajasListar: (inicio, fin) => request(`/cajas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  categorias: () => request('/categorias'),
  productosStockBajo: () => request('/productos/stock-bajo'),
  productoCrear: (data) => request('/productos', { method: 'POST', body: JSON.stringify(data) }),
  productoActualizar: (id, data) => request(`/productos/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  productoEliminar: (id) => request(`/productos/${id}`, { method: 'DELETE' }),
  productoDesactivar: (id) => request(`/productos/${id}/desactivar`, { method: 'POST' }),
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
  usuariosListar: () => request('/usuarios'),
  usuarioCrear: (data) => request('/usuarios', { method: 'POST', body: JSON.stringify(data) }),
  usuarioDesactivar: (id) => request(`/usuarios/${id}/desactivar`, { method: 'POST' }),
  reportesVentas: (inicio, fin) => request(`/reportes/ventas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  reportesProductosVendidos: (inicio, fin) => request(`/reportes/productos-vendidos?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  reportesEstadisticas: (inicio, fin) => request(`/reportes/estadisticas?fecha_inicio=${inicio}&fecha_fin=${fin}`),
  comprobantesListar: (filtros = {}) => {
    const params = new URLSearchParams(filtros).toString();
    return request(`/comprobantes${params ? `?${params}` : ''}`);
  },
  productoSubirImagen: async (id, archivo) => {
    const token = obtenerToken();
    const formData = new FormData();
    formData.append('imagen', archivo);

    const res = await fetch(`${API_URL}/productos/${id}/imagen`, {
      method: 'POST',
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      // OJO: no pongas 'Content-Type' aquí — el navegador lo arma solo
      // con el "boundary" correcto para FormData; si lo fuerzas a JSON
      // rompe la subida del archivo.
      body: formData,
    });

    const data = await res.json().catch(() => null);
    if (!res.ok) throw new Error(data?.message || 'Error al subir la imagen');
    return data;
  },
};