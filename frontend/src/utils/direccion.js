// Dirección propia de cada negocio en el navegador:
//   /verane          -> login con el logo y el color de VERANE
//   /verane/caja     -> la pantalla Caja de VERANE (con sesión)
// /panel y /boleta/... son del sistema y no se tocan aquí.

const RUTAS_DEL_SISTEMA = ['panel', 'boleta'];

/** Pantallas que pueden ir en la dirección (las de App.jsx). */
const PANTALLAS = [
  'RESUMEN', 'POS', 'CAJA', 'HISTORIAL_CAJA', 'PRODUCTOS', 'STOCK', 'PROVEEDORES', 'DEVOLUCIONES',
  'CLIENTES', 'COMPROBANTES', 'REPORTES', 'CONFIGURACION', 'SUSCRIPCION', 'COTIZACIONES', 'CREDITOS',
  'GUIAS', 'GASTOS', 'MESAS', 'PREPARACION', 'CARTA', 'RECETAS', 'VEHICULOS', 'ORDENES', 'PAQUETES',
];

/** 'HISTORIAL_CAJA' -> 'historial-caja' */
export function pantallaEnDireccion(pantalla) {
  return String(pantalla || '').toLowerCase().replace(/_/g, '-');
}

/** 'historial-caja' -> 'HISTORIAL_CAJA'; null si no es una pantalla. */
export function pantallaDeDireccion(texto) {
  const p = String(texto || '').toUpperCase().replace(/-/g, '_');
  return PANTALLAS.includes(p) ? p : null;
}

/** Lo que dice la barra de direcciones: { slug, pantalla } (null si no hay). */
export function leerDireccion() {
  const partes = window.location.pathname.split('/').filter(Boolean);
  const slug = partes[0] ? decodeURIComponent(partes[0]).toLowerCase() : null;
  if (!slug || RUTAS_DEL_SISTEMA.includes(slug)) return { slug: null, pantalla: null, delSistema: !!slug };
  return { slug, pantalla: pantallaDeDireccion(partes[1]), delSistema: false };
}

/** Cambia la dirección sin recargar. `nueva` = true agrega un paso al historial (botón atrás). */
export function irADireccion(ruta, nueva = false) {
  if (window.location.pathname === ruta) return;
  try {
    if (nueva) window.history.pushState(null, '', ruta);
    else window.history.replaceState(null, '', ruta);
  } catch {
    // Algunos navegadores limitan los cambios seguidos; la app sigue igual.
  }
}
