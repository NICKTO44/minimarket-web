// ============================================================
// Tema por negocio
// ------------------------------------------------------------
// El color de acento de cada negocio vive en SU base de datos
// (configuracion_tienda.color_acento), así que cada cuenta solo
// ve el suyo. Aquí se calculan todas las variantes (hover, fondo
// suave, borde, sidebar...) como hex/rgb simples y se escriben
// como variables CSS en <html>. No se usa color-mix() para que
// funcione igual en celulares Android con navegadores antiguos.
//
// Si el negocio no eligió color, se usa el verde Monspeet, que es
// el mismo que trae src/styles/tema.css por defecto.
// ============================================================

export const COLOR_MONSPEET = '#16a34a';

// Índigo que la migración 0003 dejó como DEFAULT en todos los
// negocios existentes. Nadie lo eligió a propósito (era el valor
// que venía puesto), así que se trata como "sin color elegido".
const COLOR_LEGADO_POR_DEFECTO = '#4338ca';

const VARIABLES_TEMA = [
  '--color-primario',
  '--color-primario-rgb',
  '--color-primario-hover',
  '--color-primario-texto',
  '--color-primario-suave',
  '--color-primario-borde',
  '--color-sobre-primario',
  '--sb-bg',
  '--sb-panel',
  '--sb-texto',
  '--sb-texto-item',
  '--sb-texto-tenue',
  '--sb-menta',
  '--sb-menta-texto',
];

function hexARgb(hex) {
  let h = hex.replace('#', '');
  if (h.length === 3) h = h.split('').map((c) => c + c).join('');
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
}

function rgbAHex([r, g, b]) {
  return '#' + [r, g, b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
}

// Mezcla `peso` (0..1) de colorA con el resto de colorB.
function mezclar(colorA, colorB, peso) {
  const a = hexARgb(colorA);
  const b = hexARgb(colorB);
  return rgbAHex(a.map((v, i) => v * peso + b[i] * (1 - peso)));
}

function luminancia(hex) {
  const [r, g, b] = hexARgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Devuelve un color de acento válido en formato #rrggbb.
 * Vacío, inválido o el índigo heredado -> verde Monspeet.
 */
export function normalizarAcento(color) {
  if (typeof color !== 'string') return COLOR_MONSPEET;
  const limpio = color.trim().toLowerCase();
  if (!/^#([0-9a-f]{3}|[0-9a-f]{6})$/.test(limpio)) return COLOR_MONSPEET;
  const seisDigitos = rgbAHex(hexARgb(limpio));
  if (seisDigitos === COLOR_LEGADO_POR_DEFECTO) return COLOR_MONSPEET;
  return seisDigitos;
}

/**
 * Calcula todas las variables del tema a partir de un color de acento.
 */
export function calcularTema(colorElegido) {
  const marca = normalizarAcento(colorElegido);

  // Los botones llevan texto blanco. Si el negocio elige un color muy
  // claro (amarillo, celeste...), se oscurece solo lo necesario para
  // que el texto siga leyéndose; el tono se mantiene.
  let primario = marca;
  for (let i = 0; i < 12 && luminancia(primario) > 0.3; i++) {
    primario = mezclar(primario, '#000000', 0.92);
  }

  const casiNegro = '#0b0f0d';

  return {
    '--color-primario': primario,
    '--color-primario-rgb': hexARgb(primario).join(', '),
    '--color-primario-hover': mezclar(primario, '#000000', 0.85),
    '--color-primario-texto': mezclar(primario, '#000000', 0.78),
    '--color-primario-suave': mezclar(primario, '#ffffff', 0.1),
    '--color-primario-borde': mezclar(primario, '#ffffff', 0.3),
    '--color-sobre-primario': '#ffffff',

    // Sidebar: oscuro, teñido con el color del negocio
    '--sb-bg': mezclar(marca, casiNegro, 0.12),
    '--sb-panel': mezclar(marca, '#16201b', 0.2),
    '--sb-texto': mezclar(marca, '#ffffff', 0.08),
    '--sb-texto-item': mezclar(marca, '#d4dcd8', 0.15),
    '--sb-texto-tenue': mezclar(marca, '#8a948f', 0.3),
    '--sb-menta': mezclar(marca, '#ffffff', 0.55),
    '--sb-menta-texto': mezclar(marca, '#050806', 0.25),
  };
}

/** Aplica el tema del negocio a toda la app. */
export function aplicarTema(colorElegido) {
  const raiz = document.documentElement;
  const tema = calcularTema(colorElegido);
  Object.entries(tema).forEach(([variable, valor]) => raiz.style.setProperty(variable, valor));
}

/** Vuelve al verde Monspeet (al cerrar sesión). */
export function limpiarTema() {
  const raiz = document.documentElement;
  VARIABLES_TEMA.forEach((variable) => raiz.style.removeProperty(variable));
}
