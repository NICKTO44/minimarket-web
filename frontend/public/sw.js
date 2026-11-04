// Service worker de Monspeet POS: para que el sistema se pueda instalar
// como app (ver src/utils/appInstalable.js).
//
// A propósito NO guarda nada en caché: cada vez se pide todo al servidor, así
// la app instalada siempre abre la última versión publicada y nunca muestra
// ventas, stock o caja viejos. Si no hay internet, en vez de la pantalla de
// error del navegador sale un aviso para reintentar.
//
// Única excepción: un negocio con el módulo "Venta sin conexión" le pide
// (mensaje GUARDAR_APP, ver src/utils/sinConexion.js) que guarde los archivos
// de la app en este equipo, para poder abrirla sin internet. Igual se pide
// primero al servidor (siempre la última versión); la copia solo se usa si
// no hay conexión. Sin ese pedido no se guarda nada y todo sigue como arriba.
// Los datos (productos, ventas...) no pasan por aquí: van a otro servidor.

const SIN_CONEXION = `<!doctype html>
<html lang="es"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sin conexión · Monspeet POS</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; font-family: system-ui, sans-serif; background: #f6f7f6; color: #1c2b22; }
  main { max-width: 340px; padding: 24px; text-align: center; }
  h1 { font-size: 20px; margin: 0 0 8px; }
  p { margin: 0 0 20px; color: #4b5b52; line-height: 1.5; }
  button { font: inherit; font-weight: 600; padding: 12px 22px; border: 0; border-radius: 10px; background: #16a34a; color: #fff; cursor: pointer; }
</style></head>
<body><main>
  <h1>Sin conexión a internet</h1>
  <p>El sistema necesita internet para vender y ver tu caja. Revisa la conexión y vuelve a intentar.</p>
  <button onclick="location.reload()">Reintentar</button>
</main></body></html>`;

const CACHE_APP = 'monspeet-app-sin-conexion-v1';
// ¿Algún negocio de este equipo pidió guardar la app? Se averigua al abrir.
let appGuardada = false;
const revisarAppGuardada = () =>
  caches
    .has(CACHE_APP)
    .then((hay) => (appGuardada = hay))
    .catch(() => false);
revisarAppGuardada();

// Cloudflare Pages redirige /index.html → / (308). Una respuesta "redirigida"
// no sirve para abrir una página (Chrome corta con ERR_FAILED), así que se
// vuelve a armar como respuesta normal, al guardarla y al usarla.
async function copiaLimpia(respuesta) {
  if (!respuesta || !respuesta.redirected) return respuesta;
  return new Response(await respuesta.blob(), {
    status: respuesta.status,
    statusText: respuesta.statusText,
    headers: respuesta.headers,
  });
}

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (evento) => evento.waitUntil(self.clients.claim()));

// Módulo "Venta sin conexión": guardar los archivos de la versión actual (y
// quitar los de versiones anteriores).
self.addEventListener('message', (evento) => {
  const datos = evento.data;
  if (datos?.tipo !== 'GUARDAR_APP' || !Array.isArray(datos.archivos)) return;
  const archivos = datos.archivos.filter((a) => typeof a === 'string' && a.startsWith('/'));
  evento.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE_APP);
      appGuardada = true;
      await Promise.all(
        archivos.map((url) =>
          fetch(url, { cache: 'no-cache' })
            .then(async (r) => (r.ok ? cache.put(url, await copiaLimpia(r)) : null))
            .catch(() => null)
        )
      );
      // Archivos de versiones anteriores (/assets/... con otro nombre).
      const vigentes = new Set(archivos.map((a) => new URL(a, self.location.origin).pathname));
      for (const pedido of await cache.keys()) {
        const ruta = new URL(pedido.url).pathname;
        if (ruta.startsWith('/assets/') && !vigentes.has(ruta)) await cache.delete(pedido);
      }
    })()
  );
});

function sinConexion() {
  return new Response(SIN_CONEXION, { headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}

self.addEventListener('fetch', (evento) => {
  const pedido = evento.request;
  if (pedido.mode === 'navigate') {
    evento.respondWith(
      (async () => {
        await revisarAppGuardada();
        try {
          const respuesta = await fetch(pedido);
          // Con la app guardada, la página queda al día para abrirla sin internet.
          if (appGuardada && respuesta.ok) {
            const copia = respuesta.clone();
            copiaLimpia(copia)
              .then((limpia) => caches.open(CACHE_APP).then((c) => c.put('/index.html', limpia)))
              .catch(() => {});
          }
          return respuesta;
        } catch {
          if (appGuardada) {
            const cache = await caches.open(CACHE_APP);
            const guardada = (await cache.match('/')) || (await cache.match('/index.html'));
            if (guardada) return copiaLimpia(guardada);
          }
          return sinConexion();
        }
      })()
    );
    return;
  }
  // Lo demás va directo al servidor, salvo los archivos de la app cuando un
  // negocio con "Venta sin conexión" la guardó: primero el servidor y, si no
  // hay conexión, la copia.
  if (!appGuardada || pedido.method !== 'GET') return;
  const url = new URL(pedido.url);
  if (url.origin !== self.location.origin) return;
  evento.respondWith(
    (async () => {
      try {
        const respuesta = await fetch(pedido);
        if (respuesta.ok && url.pathname.startsWith('/assets/')) {
          const copia = respuesta.clone();
          caches.open(CACHE_APP).then((c) => c.put(url.pathname, copia)).catch(() => {});
        }
        return respuesta;
      } catch (error) {
        const guardada = await caches.match(url.pathname, { cacheName: CACHE_APP });
        if (guardada) return copiaLimpia(guardada);
        throw error;
      }
    })()
  );
});
