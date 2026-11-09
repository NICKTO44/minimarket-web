// Sesión por pestaña: cada pestaña trabaja con UN negocio, aunque en el
// mismo navegador haya otras pestañas con otro negocio (un dueño con dos
// sistemas: negocio A en una pestaña y negocio B en otra).
//
// - La sesión y el negocio de ESTA pestaña viven en sessionStorage (es de
//   cada pestaña y sobrevive a la recarga). api.js manda siempre el token de
//   esta pestaña, así que lo que se hace en la pestaña A se graba en A.
// - localStorage guarda el "llavero" (las sesiones abiertas, por negocio) y
//   el último negocio usado, para que una pestaña nueva abra ese, como
//   siempre. Una pestaña nueva con la dirección de un negocio del llavero
//   (/negocio-b) entra directo a ese negocio, sin volver a iniciar sesión.
//
// Quien tiene un solo negocio no nota nada: todo funciona igual que antes.

export const CLAVE_SESION = 'minimarket_sesion';
export const CLAVE_TIENDA = 'minimarket_tienda';
const CLAVE_LLAVERO = 'monspeet_sesiones';
// Marca de que esta pestaña ya tomó su sesión (no vuelve a tomar otra).
const CLAVE_PESTANA = 'monspeet_pestana';
// Último negocio usado: el que abre una pestaña nueva.
const CLAVE_ULTIMO = 'monspeet_ultimo';

function leerJson(texto) {
  try {
    return JSON.parse(texto || 'null');
  } catch {
    return null;
  }
}

function deSesion(clave) {
  try {
    return sessionStorage.getItem(clave);
  } catch {
    return null;
  }
}

function deLocal(clave) {
  try {
    return localStorage.getItem(clave);
  } catch {
    return null;
  }
}

function aSesion(clave, valor) {
  try {
    if (valor == null) sessionStorage.removeItem(clave);
    else sessionStorage.setItem(clave, valor);
  } catch {
    // sin sessionStorage: se usa solo localStorage (como antes)
  }
}

function aLocal(clave, valor) {
  try {
    if (valor == null) localStorage.removeItem(clave);
    else localStorage.setItem(clave, valor);
  } catch {
    // sin localStorage: la sesión sigue en esta pestaña hasta cerrarla
  }
}

function sessionStorageDisponible() {
  try {
    sessionStorage.setItem('__monspeet_prueba', '1');
    sessionStorage.removeItem('__monspeet_prueba');
    return true;
  } catch {
    return false;
  }
}
const porPestana = sessionStorageDisponible();

function leerLlavero() {
  const llavero = leerJson(deLocal(CLAVE_LLAVERO));
  return llavero && typeof llavero === 'object' && !Array.isArray(llavero) ? llavero : {};
}

function guardarLlavero(llavero) {
  aLocal(CLAVE_LLAVERO, Object.keys(llavero).length ? JSON.stringify(llavero) : null);
}

/** Negocio del llavero que corresponde a la dirección (/slug), o null. */
function delLlaveroPorDireccion() {
  const primera = window.location.pathname.split('/').filter(Boolean)[0];
  if (!primera) return null;
  const slug = decodeURIComponent(primera).toLowerCase();
  const llavero = leerLlavero();
  for (const entrada of Object.values(llavero)) {
    const tienda = leerJson(entrada?.tienda);
    if (!tienda || !entrada?.sesion) continue;
    if (String(tienda.slug || '').toLowerCase() === slug || String(tienda.identificador || '').toLowerCase() === slug) {
      return entrada;
    }
  }
  return null;
}

function guardarUltimo(identificador, entrada) {
  aLocal(CLAVE_ULTIMO, identificador);
  // También en las claves de siempre (versiones anteriores del sistema).
  aLocal(CLAVE_SESION, entrada?.sesion ?? null);
  aLocal(CLAVE_TIENDA, entrada?.tienda ?? null);
}

// Al abrir una pestaña nueva (una sola vez por pestaña) toma, en este orden:
// el negocio de la dirección (/negocio-b) si está en el llavero; el último
// usado; o lo que dejó guardado la versión anterior del sistema.
(function tomarSesionDeLaPestana() {
  if (!porPestana || deSesion(CLAVE_PESTANA)) return;
  const llavero = leerLlavero();
  const entrada = delLlaveroPorDireccion() || llavero[deLocal(CLAVE_ULTIMO)] || null;
  if (entrada?.sesion && entrada?.tienda) {
    aSesion(CLAVE_SESION, entrada.sesion);
    aSesion(CLAVE_TIENDA, entrada.tienda);
  } else {
    aSesion(CLAVE_SESION, deLocal(CLAVE_SESION));
    aSesion(CLAVE_TIENDA, deLocal(CLAVE_TIENDA));
    // Sesión de antes de este cambio: pasa al llavero.
    if (deSesion(CLAVE_SESION)) recordarEnLlavero();
  }
  aSesion(CLAVE_PESTANA, '1');
})();

/** Lo guardado de esta pestaña (texto JSON o null). */
export function leerGuardado(clave) {
  return porPestana ? deSesion(clave) : deLocal(clave);
}

/**
 * Guarda en esta pestaña. Para las demás pestañas y las nuevas cuenta
 * recién al llamar a recordarEnLlavero(), cuando sesión y negocio ya son
 * del mismo negocio (así nunca se cruza la sesión de uno con el otro).
 */
export function guardarEnPestana(clave, valor) {
  if (porPestana) aSesion(clave, valor);
  else aLocal(clave, valor);
}

/**
 * Negocio identificado en el login (todavía sin sesión). Se recuerda para
 * las pestañas nuevas solo si en este navegador no hay ninguna sesión.
 */
export function guardarNegocioDelLogin(valor) {
  guardarEnPestana(CLAVE_TIENDA, valor);
  if (porPestana && !deLocal(CLAVE_SESION)) aLocal(CLAVE_TIENDA, valor);
}

/** Quita de esta pestaña (cerrar sesión, olvidar negocio en el login). */
export function quitarDePestana(clave) {
  if (!porPestana) {
    aLocal(clave, null);
    return;
  }
  aSesion(clave, null);
  // Olvidar el negocio en el login: también para las pestañas nuevas, si no
  // hay sesión de nadie.
  if (clave === CLAVE_TIENDA && !deLocal(CLAVE_SESION)) aLocal(CLAVE_TIENDA, null);
}

/**
 * Quita solo de esta pestaña (para pasarla a otro negocio): las otras
 * pestañas y las nuevas siguen con el negocio de antes.
 */
export function quitarSoloDeLaPestana(clave) {
  if (porPestana) aSesion(clave, null);
  else aLocal(clave, null);
}

/** Negocio (objeto) de esta pestaña, o null. */
export function tiendaDeLaPestana() {
  return leerJson(leerGuardado(CLAVE_TIENDA));
}

/**
 * Anota en el llavero la sesión de esta pestaña con su negocio. Se llama
 * cuando los dos ya son del mismo negocio (después del login, al renovar el
 * token, etc.).
 */
export function recordarEnLlavero() {
  const sesion = leerGuardado(CLAVE_SESION);
  const tienda = tiendaDeLaPestana();
  if (!sesion || !tienda?.identificador) return;
  const llavero = leerLlavero();
  const entrada = { sesion, tienda: JSON.stringify(tienda) };
  llavero[tienda.identificador] = entrada;
  guardarLlavero(llavero);
  guardarUltimo(tienda.identificador, entrada);
}

/** Saca del llavero el negocio de esta pestaña (al cerrar sesión). */
export function olvidarDelLlavero() {
  const tienda = tiendaDeLaPestana();
  if (!tienda?.identificador) return;
  const llavero = leerLlavero();
  if (!llavero[tienda.identificador]) return;
  delete llavero[tienda.identificador];
  guardarLlavero(llavero);
  // Si era el último usado, las pestañas nuevas abren otro que siga abierto.
  if (deLocal(CLAVE_ULTIMO) === tienda.identificador) {
    const [otro] = Object.keys(llavero);
    if (otro) guardarUltimo(otro, llavero[otro]);
    else {
      aLocal(CLAVE_ULTIMO, null);
      aLocal(CLAVE_SESION, null);
    }
  }
}

/** ¿Hay una sesión abierta de ese negocio en este navegador? */
export function haySesionDe(identificador) {
  return !!(identificador && leerLlavero()[identificador]?.sesion);
}

/**
 * Pasa esta pestaña al negocio del llavero (sin volver a iniciar sesión).
 * Devuelve false si ya no hay sesión guardada de ese negocio.
 */
export function usarNegocioEnPestana(identificador) {
  const entrada = leerLlavero()[identificador];
  if (!entrada?.sesion || !entrada?.tienda) return false;
  guardarEnPestana(CLAVE_SESION, entrada.sesion);
  guardarEnPestana(CLAVE_TIENDA, entrada.tienda);
  guardarUltimo(identificador, entrada);
  return true;
}

/** El servidor dijo que la sesión de esta pestaña ya no vale (401). */
export function cerrarSesionVencida() {
  olvidarDelLlavero();
  quitarDePestana(CLAVE_SESION);
}
