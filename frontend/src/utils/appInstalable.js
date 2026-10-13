// El sistema como app instalable (PWA).
//
// - En la dirección general sale como "Monspeet POS" (public/manifest.webmanifest).
// - En la dirección propia de un negocio (/verane, /verane/caja) el manifiesto
//   lo arma el backend (/publico/app/verane/manifest): al instalar, la app
//   queda con el nombre, el color y el logo de ese negocio.
// - public/sw.js no guarda nada en caché: la app siempre abre la última versión.
import { API_URL } from '../api/api';
import { leerDireccion } from './direccion';

function enlace(rel) {
  let link = document.querySelector(`link[rel="${rel}"]`);
  if (!link) {
    link = document.createElement('link');
    link.rel = rel;
    document.head.appendChild(link);
  }
  return link;
}

/** Usa el manifiesto y el ícono del negocio de esa dirección (o el general sin dirección). */
export function usarAppDelNegocio(slug, { conLogo = true } = {}) {
  const base = `${API_URL}/publico/app/${encodeURIComponent(slug)}`;
  enlace('manifest').href = `${base}/manifest?origen=${encodeURIComponent(window.location.origin)}`;
  // iPhone (Agregar a inicio) toma este ícono, no el del manifiesto.
  enlace('apple-touch-icon').href = conLogo ? `${base}/icono/180` : '/apple-touch-icon.png';
}

export function usarAppGeneral() {
  enlace('manifest').href = '/manifest.webmanifest';
  enlace('apple-touch-icon').href = '/apple-touch-icon.png';
}

/** Al abrir la página: manifiesto según la dirección y registro del service worker. */
export function iniciarAppInstalable() {
  const { slug } = leerDireccion();
  if (slug) usarAppDelNegocio(slug);

  // Solo en producción: en desarrollo (npm run dev) no hace falta.
  if (import.meta.env.PROD && 'serviceWorker' in navigator) {
    window.addEventListener('load', () => {
      navigator.serviceWorker.register('/sw.js').catch(() => {
        // Sin service worker el sistema funciona igual; solo no se ofrece instalarlo.
      });
    });
  }
}
