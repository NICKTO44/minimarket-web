// Service worker de Monspeet POS: solo para que el sistema se pueda instalar
// como app (ver src/utils/appInstalable.js).
//
// A propósito NO guarda nada en caché: cada vez se pide todo al servidor, así
// la app instalada siempre abre la última versión publicada y nunca muestra
// ventas, stock o caja viejos. Si no hay internet, en vez de la pantalla de
// error del navegador sale un aviso para reintentar.

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

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (evento) => evento.waitUntil(self.clients.claim()));

self.addEventListener('fetch', (evento) => {
  // Solo la apertura de páginas; lo demás (archivos, API) va directo al servidor.
  if (evento.request.mode !== 'navigate') return;
  evento.respondWith(
    fetch(evento.request).catch(
      () => new Response(SIN_CONEXION, { headers: { 'Content-Type': 'text/html; charset=utf-8' } })
    )
  );
});
