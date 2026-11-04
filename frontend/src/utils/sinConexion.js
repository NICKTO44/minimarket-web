// Módulo VENTA_SIN_CONEXION: vender sin internet.
//
// Solo funciona en un negocio con el módulo encendido (App.jsx llama a
// activarSinConexion). Sin el módulo nada de esto se usa: no se guarda nada
// en el navegador y api.js hace lo de siempre.
//
// Con el módulo:
// - api.js guarda en este equipo la última respuesta de lo que el punto de
//   venta necesita para vender (productos, clientes, caja abierta,
//   configuración...). Sin internet responde con esa copia.
// - Lo que se hace sin internet (una venta, abrir la caja, un gasto de caja)
//   queda en una cola en este equipo (IndexedDB), cada operación con su
//   código único y la hora en que se hizo.
// - Al volver la conexión, se sube en orden a POST /sin-conexion/operaciones
//   (backend/src/handlers/sin_conexion.rs). El código único hace que nada
//   entre dos veces aunque se mande de nuevo.
//
// Lo que no se puede subir (por ejemplo, ya no alcanza el stock porque otro
// equipo vendió lo mismo) queda "por revisar" con el motivo, para corregirlo
// y reintentar. Nunca se borra solo.

const CLAVE_ACTIVO = 'monspeet_sin_conexion';
const NOMBRE_BASE = 'monspeet-sin-conexion';
const VERSION_BASE = 1;
// Lo ya subido se guarda unos días en la lista (para enviar por WhatsApp).
const DIAS_GUARDAR_HECHAS = 7;
// SUNAT da unos 3 días para enviar: se avisa desde las 48 horas.
export const HORAS_AVISO = 48;

// Lo que el punto de venta, la caja y los gastos leen del servidor. Solo
// esto se guarda en el equipo (no reportes ni historiales).
const RUTAS_GUARDADAS = new Set([
  '/productos',
  '/productos/opciones-venta',
  '/categorias',
  '/clientes/todos',
  '/cajas/abierta',
  '/cajas/movimientos',
  '/configuracion',
  '/negocio',
  '/detraccion',
  '/cambios/config',
  '/unidades',
  '/gastos/categorias',
  '/medicamentos/fichas',
  '/repuestos/fichas',
  '/repuestos/compatibilidades',
  '/precios-venta',
  '/equivalentes',
  '/libros/fichas',
  '/paquetes',
  '/suscripcion/estado',
]);

// Se piden apenas se enciende, para que haya copia aunque el cajero no
// haya abierto esas pantallas.
const RUTAS_PRECARGA = ['/productos', '/categorias', '/clientes/todos', '/configuracion', '/cajas/abierta', '/gastos/categorias', '/unidades'];

function leerActivo() {
  try {
    return localStorage.getItem(CLAVE_ACTIVO) || null;
  } catch {
    return null;
  }
}

let tiendaActiva = leerActivo();

// ---------------------------------------------------------------------
// Estado visible (EstadoSinConexion.jsx lo muestra)
// ---------------------------------------------------------------------

const estado = {
  enLinea: typeof navigator === 'undefined' ? true : navigator.onLine !== false,
  subiendo: false,
  operaciones: [],
  sinAlmacen: false,
};
const oyentes = new Set();

function avisar() {
  const copia = { ...estado, operaciones: [...estado.operaciones] };
  for (const f of oyentes) {
    try {
      f(copia);
    } catch {
      // un oyente con error no frena a los demás
    }
  }
}

export function suscribirSinConexion(f) {
  oyentes.add(f);
  f({ ...estado, operaciones: [...estado.operaciones] });
  return () => oyentes.delete(f);
}

export function modoSinConexionActivo() {
  return !!tiendaActiva;
}

/** api.js avisa cuándo una petición no llegó y cuándo sí. */
export function marcarSinConexion() {
  if (!estado.enLinea) return;
  estado.enLinea = false;
  avisar();
}

export function marcarConConexion() {
  if (estado.enLinea) return;
  estado.enLinea = true;
  avisar();
  // Volvió: se sube lo pendiente.
  programarSincronizacion(500);
}

export function hayConexion() {
  return estado.enLinea;
}

// ---------------------------------------------------------------------
// IndexedDB (con un respaldo en memoria si el navegador no lo permite)
// ---------------------------------------------------------------------

let basePromesa = null;
const memoria = { cache: new Map(), cola: new Map() };

function abrirBase() {
  if (basePromesa) return basePromesa;
  basePromesa = new Promise((resolver) => {
    try {
      if (typeof indexedDB === 'undefined') throw new Error('sin IndexedDB');
      const pedido = indexedDB.open(NOMBRE_BASE, VERSION_BASE);
      pedido.onupgradeneeded = () => {
        const db = pedido.result;
        if (!db.objectStoreNames.contains('cache')) db.createObjectStore('cache');
        if (!db.objectStoreNames.contains('cola')) db.createObjectStore('cola', { keyPath: 'uuid' });
      };
      pedido.onsuccess = () => resolver(pedido.result);
      pedido.onerror = () => {
        estado.sinAlmacen = true;
        resolver(null);
      };
    } catch {
      estado.sinAlmacen = true;
      resolver(null);
    }
  });
  return basePromesa;
}

async function guardar(almacen, valor, clave) {
  const db = await abrirBase();
  if (!db) {
    memoria[almacen].set(clave ?? valor.uuid, valor);
    return;
  }
  await new Promise((resolver, rechazar) => {
    const tx = db.transaction(almacen, 'readwrite');
    if (clave === undefined) tx.objectStore(almacen).put(valor);
    else tx.objectStore(almacen).put(valor, clave);
    tx.oncomplete = () => resolver();
    tx.onerror = () => rechazar(tx.error);
    tx.onabort = () => rechazar(tx.error);
  });
}

async function leer(almacen, clave) {
  const db = await abrirBase();
  if (!db) return memoria[almacen].get(clave);
  return new Promise((resolver) => {
    const pedido = db.transaction(almacen, 'readonly').objectStore(almacen).get(clave);
    pedido.onsuccess = () => resolver(pedido.result);
    pedido.onerror = () => resolver(undefined);
  });
}

async function leerTodo(almacen) {
  const db = await abrirBase();
  if (!db) return [...memoria[almacen].values()];
  return new Promise((resolver) => {
    const pedido = db.transaction(almacen, 'readonly').objectStore(almacen).getAll();
    pedido.onsuccess = () => resolver(pedido.result || []);
    pedido.onerror = () => resolver([]);
  });
}

async function borrar(almacen, clave) {
  const db = await abrirBase();
  if (!db) {
    memoria[almacen].delete(clave);
    return;
  }
  await new Promise((resolver) => {
    const tx = db.transaction(almacen, 'readwrite');
    tx.objectStore(almacen).delete(clave);
    tx.oncomplete = () => resolver();
    tx.onerror = () => resolver();
  });
}

// ---------------------------------------------------------------------
// Copia de lo que lee el punto de venta
// ---------------------------------------------------------------------

function claveCache(ruta) {
  return `${tiendaActiva}|${ruta}`;
}

/** ¿Esta lectura se guarda en el equipo? */
export function rutaGuardable(ruta) {
  return RUTAS_GUARDADAS.has(ruta) || ruta.startsWith('/vehiculos/placa/');
}

export async function guardarCopia(ruta, datos) {
  if (!tiendaActiva || !rutaGuardable(ruta)) return;
  try {
    await guardar('cache', { datos, guardado: Date.now() }, claveCache(ruta));
  } catch {
    // Sin espacio o sin permiso: se sigue sin copia.
  }
}

/**
 * La copia guardada de una lectura, para cuando no hay internet.
 * undefined = no hay copia.
 */
export async function copiaSinConexion(ruta) {
  if (!tiendaActiva) return undefined;
  // Buscar clientes: se busca en la lista guardada.
  if (ruta.startsWith('/clientes?q=')) {
    const texto = decodeURIComponent(ruta.slice('/clientes?q='.length)).trim().toLowerCase();
    const todos = (await leer('cache', claveCache('/clientes/todos')))?.datos;
    if (!Array.isArray(todos)) return [];
    return todos
      .filter(
        (c) =>
          c.activo !== false &&
          (String(c.numero_documento || '').toLowerCase().includes(texto) ||
            String(c.nombre_razon_social || '').toLowerCase().includes(texto))
      )
      .slice(0, 20);
  }
  if (!rutaGuardable(ruta)) return undefined;
  const guardada = await leer('cache', claveCache(ruta));
  if (guardada) return guardada.datos;
  // Una placa que nunca se buscó: no hay datos del vehículo.
  if (ruta.startsWith('/vehiculos/placa/')) return null;
  return undefined;
}

/** Cambia la copia guardada de una lectura (p. ej. el stock tras vender). */
async function cambiarCopia(ruta, cambio) {
  const guardada = await leer('cache', claveCache(ruta));
  const nuevos = cambio(guardada ? guardada.datos : undefined);
  if (nuevos === undefined) return;
  await guardar('cache', { datos: nuevos, guardado: guardada?.guardado || Date.now() }, claveCache(ruta));
}

// ---------------------------------------------------------------------
// Encender / apagar (lo llama App.jsx según los módulos del negocio)
// ---------------------------------------------------------------------

let precargaHecha = null;
let pedirLecturaGuardada = null;

/** Vuelve a pedir al servidor lo que el punto de venta necesita sin internet. */
function refrescarCopias() {
  if (!pedirLecturaGuardada || !tiendaActiva) return;
  for (const ruta of RUTAS_PRECARGA) pedirLecturaGuardada(ruta).catch(() => {});
}

export function activarSinConexion(tienda, pedirLectura) {
  if (!tienda) return;
  const cambio = tiendaActiva !== tienda;
  tiendaActiva = tienda;
  try {
    localStorage.setItem(CLAVE_ACTIVO, tienda);
  } catch {
    // sin localStorage: queda encendido hasta recargar
  }
  // Que el navegador no borre lo guardado cuando le falte espacio.
  try {
    navigator.storage?.persist?.();
  } catch {
    // no todos los navegadores lo permiten
  }
  guardarAppEnEquipo();
  if (pedirLectura) pedirLecturaGuardada = pedirLectura;
  if (cambio || precargaHecha !== tienda) {
    precargaHecha = tienda;
    refrescarCopias();
  }
  recargarOperaciones().then(() => programarSincronizacion(1000));
}

/** Módulo apagado: deja de guardar copias. Lo que falte subir se conserva. */
export function desactivarSinConexion() {
  if (!tiendaActiva) return;
  tiendaActiva = null;
  try {
    localStorage.removeItem(CLAVE_ACTIVO);
  } catch {
    // nada que hacer
  }
  avisar();
}

/** Pide al service worker (public/sw.js) que guarde la app para abrirla sin internet. */
function guardarAppEnEquipo() {
  try {
    if (!('serviceWorker' in navigator)) return;
    const archivos = new Set(['/', '/index.html', '/manifest.webmanifest', '/favicon.svg', '/icono-192.png', '/icono-512.png']);
    for (const e of document.querySelectorAll('script[src], link[rel="stylesheet"][href], link[rel="modulepreload"][href]')) {
      const url = e.getAttribute('src') || e.getAttribute('href');
      if (url && url.startsWith('/')) archivos.add(url);
    }
    navigator.serviceWorker.ready
      .then((r) => r.active?.postMessage({ tipo: 'GUARDAR_APP', archivos: [...archivos] }))
      .catch(() => {});
  } catch {
    // sin service worker la app no abre sin internet, pero vende si ya está abierta
  }
}

// ---------------------------------------------------------------------
// Cola de operaciones
// ---------------------------------------------------------------------

export function nuevoCodigo() {
  try {
    if (crypto?.randomUUID) return crypto.randomUUID();
  } catch {
    // navegadores viejos
  }
  const azar = () => Math.random().toString(16).slice(2, 10);
  return `${Date.now().toString(16)}-${azar()}-${azar()}-${azar()}`;
}

/** "AAAA-MM-DD HH:MM:SS" en hora de Perú. */
export function ahoraLima() {
  const partes = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'America/Lima',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(new Date());
  const p = Object.fromEntries(partes.map((x) => [x.type, x.value]));
  return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}:${p.second}`;
}

async function recargarOperaciones() {
  const todas = await leerTodo('cola');
  const limite = Date.now() - DIAS_GUARDAR_HECHAS * 24 * 3600 * 1000;
  const vigentes = [];
  for (const op of todas) {
    if (op.estado === 'HECHA' && (op.terminada || op.creada) < limite) {
      await borrar('cola', op.uuid);
      continue;
    }
    vigentes.push(op);
  }
  vigentes.sort((a, b) => (a.creada === b.creada ? (a.orden || 0) - (b.orden || 0) : a.creada - b.creada));
  estado.operaciones = vigentes;
  avisar();
  return vigentes;
}

let contadorOrden = 0;

/**
 * Guarda una operación hecha sin internet.
 * @param {'VENTA'|'ABRIR_CAJA'|'GASTO'} tipo
 * @param {object} datos lo que se mandará al servidor
 * @param {object} resumen lo que se muestra en la lista (total, cliente...)
 * @param {string} [uuid] código ya puesto (una venta lleva el suyo en uuid_local)
 */
export async function encolar(tipo, datos, resumen = {}, uuid = nuevoCodigo()) {
  if (!tiendaActiva) throw new Error('Este negocio no tiene encendida la venta sin conexión.');
  const op = {
    uuid,
    tienda: tiendaActiva,
    tipo,
    fecha_local: ahoraLima(),
    datos,
    resumen,
    estado: 'PENDIENTE',
    intentos: 0,
    error: null,
    resultado: null,
    creada: Date.now(),
    orden: ++contadorOrden,
  };
  await guardar('cola', op);
  await recargarOperaciones();
  return op;
}

async function actualizar(op, cambios) {
  const nueva = { ...op, ...cambios };
  await guardar('cola', nueva);
  return nueva;
}

/** Folio provisional que se imprime en el ticket ("SC-1104-0003"). */
export function folioProvisional() {
  const hoy = ahoraLima();
  const prefijo = `SC-${hoy.slice(5, 7)}${hoy.slice(8, 10)}-`;
  const delDia = estado.operaciones.filter((o) => o.tipo === 'VENTA' && o.resumen?.folio_provisional?.startsWith(prefijo)).length;
  return `${prefijo}${String(delDia + 1).padStart(4, '0')}`;
}

/** Después de vender sin internet: baja el stock en la copia de productos. */
export async function descontarStockGuardado(lineas) {
  if (!tiendaActiva) return;
  const porProducto = new Map();
  for (const l of lineas) porProducto.set(l.id, (porProducto.get(l.id) || 0) + Number(l.cantidad || 0));
  await cambiarCopia('/productos', (lista) =>
    Array.isArray(lista)
      ? lista.map((p) =>
          porProducto.has(p.id) && p.controla_stock !== false && typeof p.stock === 'number'
            ? { ...p, stock: Math.round((p.stock - porProducto.get(p.id)) * 1000) / 1000 }
            : p
        )
      : undefined
  );
}

/** Caja abierta sin internet: la copia muestra una caja provisional. */
export async function cajaProvisional(caja) {
  await cambiarCopia('/cajas/abierta', () => caja);
  await cambiarCopia('/cajas/movimientos', () => []);
}

/** Gasto de caja sin internet: se suma a la copia de la caja abierta. */
export async function sumarGastoGuardado(monto) {
  await cambiarCopia('/cajas/abierta', (c) => (c ? { ...c, gastos_total: (c.gastos_total || 0) + monto } : undefined));
}

/** Venta sin internet: se suma a la copia de la caja abierta. */
export async function sumarVentaGuardada(total, efectivo) {
  await cambiarCopia('/cajas/abierta', (c) =>
    c
      ? {
          ...c,
          total_ventas: (c.total_ventas || 0) + total,
          ventas_efectivo: (c.ventas_efectivo || 0) + efectivo,
          numero_transacciones: (c.numero_transacciones || 0) + 1,
        }
      : undefined
  );
}

// ---------------------------------------------------------------------
// Subir lo pendiente
// ---------------------------------------------------------------------

let subirOperacion = null;
let temporizador = null;

/** api.js registra cómo se sube una operación (así no hay dependencia circular). */
export function registrarSubida(f) {
  subirOperacion = f;
}

export function programarSincronizacion(ms = 0) {
  if (temporizador) clearTimeout(temporizador);
  temporizador = setTimeout(() => {
    temporizador = null;
    sincronizar().catch(() => {});
  }, ms);
}

/**
 * Sube en orden lo que está pendiente. Si se corta el internet, para y
 * espera. Lo que el servidor no acepta queda "por revisar" (no se reintenta
 * solo, salvo una boleta o factura que no se pudo emitir).
 * @param {{ incluirPorRevisar?: boolean, solo?: string }} opciones
 */
export async function sincronizar({ incluirPorRevisar = false, solo = null } = {}) {
  if (estado.subiendo || !subirOperacion) return;
  const tienda = tiendaActiva;
  const lista = (await recargarOperaciones()).filter(
    (o) =>
      o.tienda === tienda &&
      (solo ? o.uuid === solo : true) &&
      (o.estado === 'PENDIENTE' || o.estado === 'SIN_COMPROBANTE' || (incluirPorRevisar && o.estado === 'POR_REVISAR'))
  );
  if (lista.length === 0) return;
  estado.subiendo = true;
  avisar();
  let algunaSubio = false;
  try {
    for (const op of lista) {
      let actual = op;
      try {
        const r = await subirOperacion({ uuid: op.uuid, tipo: op.tipo, fecha_local: op.fecha_local, datos: op.datos });
        actual = await actualizar(op, {
          estado: r.estado === 'VENTA_HECHA' ? 'SIN_COMPROBANTE' : 'HECHA',
          resultado: r,
          error: r.comprobante_error || null,
          intentos: op.intentos + 1,
          terminada: Date.now(),
        });
        algunaSubio = true;
        marcarConConexion();
      } catch (e) {
        if (e?.sinConexion) {
          // Se cortó otra vez: se sigue después.
          marcarSinConexion();
          break;
        }
        // Sesión vencida (se vuelve a entrar y se sigue) o un problema del
        // servidor que pasa solo: queda pendiente, sin marcarla por revisar.
        if (e?.status === 401 || e?.status >= 500) {
          const conAviso = await actualizar(op, { error: e?.message || 'El servidor no respondió.', intentos: op.intentos + 1 });
          estado.operaciones = estado.operaciones.map((o) => (o.uuid === conAviso.uuid ? conAviso : o));
          break;
        }
        actual = await actualizar(op, { estado: 'POR_REVISAR', error: e?.message || 'No se pudo subir.', intentos: op.intentos + 1 });
      }
      estado.operaciones = estado.operaciones.map((o) => (o.uuid === actual.uuid ? actual : o));
      avisar();
    }
  } finally {
    estado.subiendo = false;
    await recargarOperaciones();
    // Lo subido ya está en el servidor: la copia (stock, caja) queda al día.
    // (Solo si no quedó nada por subir: si no, el stock guardado perdería lo
    // que se vendió y todavía no llega.)
    if (algunaSubio && !pendientesDe(estado.operaciones).some((o) => o.estado === 'PENDIENTE')) refrescarCopias();
  }
}

/** Quitar de la lista algo que no se va a subir (solo lo que está por revisar). */
export async function descartarOperacion(uuid) {
  const op = estado.operaciones.find((o) => o.uuid === uuid);
  if (!op || op.estado !== 'POR_REVISAR') return;
  await borrar('cola', uuid);
  await recargarOperaciones();
}

/** Lo de este negocio (la cola puede tener lo de otra sesión de este equipo). */
export function deEsteNegocio(operaciones, tienda = tiendaActiva) {
  return operaciones.filter((o) => o.tienda === tienda);
}

/** Lo que falta subir de este negocio. */
export function pendientesDe(operaciones, tienda = tiendaActiva) {
  return operaciones.filter((o) => o.tienda === tienda && o.estado !== 'HECHA');
}

/** ¿Hay algo hecho hace más de 48 horas que todavía no llega a SUNAT? */
export function hayAtrasadas(operaciones, tienda = tiendaActiva) {
  const limite = Date.now() - HORAS_AVISO * 3600 * 1000;
  return pendientesDe(operaciones, tienda).some((o) => o.tipo === 'VENTA' && o.creada < limite);
}

/** Ventas ya subidas, con su boleta o factura y el WhatsApp del cliente, que falta enviarle. */
export function listasParaEnviar(operaciones, tienda = tiendaActiva) {
  return deEsteNegocio(operaciones, tienda).filter(
    (o) => o.tipo === 'VENTA' && o.estado === 'HECHA' && o.resultado?.comprobante?.comprobante_id && o.resumen?.telefono && !o.resumen?.enviado
  );
}

/** Ya se abrió WhatsApp para mandarle su comprobante. */
export async function marcarEnviado(uuid) {
  const op = (await leer('cola', uuid)) || null;
  if (!op) return;
  await actualizar(op, { resumen: { ...op.resumen, enviado: true } });
  await recargarOperaciones();
}

/** Guarda el teléfono del cliente con su venta (para enviarle la boleta después). */
export async function guardarTelefono(uuid, telefono) {
  const op = (await leer('cola', uuid)) || null;
  if (!op) return;
  await actualizar(op, { resumen: { ...op.resumen, telefono } });
  await recargarOperaciones();
}

// ---------------------------------------------------------------------
// Respaldo en archivo
// ---------------------------------------------------------------------

/** Descarga un archivo con todo lo que falta subir. */
export function descargarRespaldo(operaciones, nombreNegocio = 'negocio') {
  const pendientes = pendientesDe(operaciones);
  const contenido = JSON.stringify(
    { tipo: 'monspeet-respaldo-sin-conexion', version: 1, tienda: tiendaActiva, creado: ahoraLima(), operaciones: pendientes },
    null,
    2
  );
  const blob = new Blob([contenido], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  const limpio = String(nombreNegocio).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'negocio';
  a.href = url;
  a.download = `respaldo-${limpio}-${ahoraLima().replace(/[: ]/g, '-')}.json`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
  return pendientes.length;
}

/**
 * Carga un respaldo (de este equipo o de otro). Lo que ya está en la lista
 * no se duplica; el servidor tampoco deja entrar dos veces el mismo código.
 * @returns {Promise<number>} cuántas operaciones nuevas se agregaron
 */
export async function cargarRespaldo(archivo) {
  const texto = await archivo.text();
  let respaldo;
  try {
    respaldo = JSON.parse(texto);
  } catch {
    throw new Error('El archivo no es un respaldo de Monspeet.');
  }
  if (respaldo?.tipo !== 'monspeet-respaldo-sin-conexion' || !Array.isArray(respaldo.operaciones)) {
    throw new Error('El archivo no es un respaldo de Monspeet.');
  }
  if (respaldo.tienda && respaldo.tienda !== tiendaActiva) {
    throw new Error('Ese respaldo es de otro negocio.');
  }
  const existentes = new Set((await leerTodo('cola')).map((o) => o.uuid));
  let nuevas = 0;
  for (const op of respaldo.operaciones) {
    if (!op?.uuid || existentes.has(op.uuid) || !['VENTA', 'ABRIR_CAJA', 'GASTO'].includes(op.tipo)) continue;
    await guardar('cola', { ...op, tienda: tiendaActiva, estado: 'PENDIENTE', error: null });
    nuevas += 1;
  }
  await recargarOperaciones();
  programarSincronizacion(300);
  return nuevas;
}

// Eventos del navegador.
if (typeof window !== 'undefined') {
  window.addEventListener('online', () => {
    if (tiendaActiva) programarSincronizacion(1500);
  });
  window.addEventListener('offline', () => {
    if (tiendaActiva) marcarSinConexion();
  });
  // Cada minuto, si hay algo pendiente, se intenta subir.
  setInterval(() => {
    if (tiendaActiva && !estado.subiendo && pendientesDe(estado.operaciones).some((o) => o.estado !== 'POR_REVISAR')) {
      sincronizar().catch(() => {});
    }
  }, 60 * 1000);
}
