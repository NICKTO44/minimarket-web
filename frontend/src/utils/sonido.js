// Sonido corto de aviso generado con el navegador (sin archivos de audio).
// Los navegadores solo dejan sonar después de que la persona tocó algo en
// la página; como el sistema siempre se usa tocando, en la práctica suena.

let contexto = null;

function obtenerContexto() {
  if (typeof window === 'undefined') return null;
  const Ctx = window.AudioContext || window.webkitAudioContext;
  if (!Ctx) return null;
  if (!contexto) contexto = new Ctx();
  if (contexto.state === 'suspended') contexto.resume().catch(() => {});
  return contexto;
}

function tono(ctx, frecuencia, inicio, duracion) {
  const osc = ctx.createOscillator();
  const vol = ctx.createGain();
  osc.type = 'sine';
  osc.frequency.value = frecuencia;
  vol.gain.setValueAtTime(0.0001, ctx.currentTime + inicio);
  vol.gain.exponentialRampToValueAtTime(0.35, ctx.currentTime + inicio + 0.02);
  vol.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + inicio + duracion);
  osc.connect(vol).connect(ctx.destination);
  osc.start(ctx.currentTime + inicio);
  osc.stop(ctx.currentTime + inicio + duracion + 0.05);
}

/** "Ding-dong": pedido listo para entregar. */
export function sonarListo() {
  const ctx = obtenerContexto();
  if (!ctx) return;
  tono(ctx, 880, 0, 0.25);
  tono(ctx, 1175, 0.18, 0.35);
}

/** Tres tonos cortos: llegó un pedido nuevo a barra/cocina. */
export function sonarNuevoPedido() {
  const ctx = obtenerContexto();
  if (!ctx) return;
  tono(ctx, 660, 0, 0.12);
  tono(ctx, 660, 0.16, 0.12);
  tono(ctx, 990, 0.32, 0.25);
}

/** Vibra el celular (si el equipo lo permite). */
export function vibrar() {
  try {
    navigator.vibrate?.([220, 90, 220]);
  } catch {
    // no disponible (computadora, iPhone): no pasa nada
  }
}

/** Llamar en un toque del usuario para "desbloquear" el audio del navegador. */
export function prepararSonido() {
  obtenerContexto();
}
