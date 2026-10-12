// Enlace público de un comprobante (el que se manda por WhatsApp):
//   https://<sitio>/boleta/<identificador del negocio>/<id>?k=<firma>
//
// La firma la da el servidor (`clave_publica` al emitir y en la lista de
// Comprobantes). Sin ella la página pública no muestra nada: así nadie puede
// ver otros comprobantes cambiando el número del enlace.
//
// Si el servidor todavía no manda la firma (versión anterior del backend),
// se arma el enlace de siempre, que ese servidor sí acepta.
export function enlaceComprobantePublico(identificadorNegocio, comprobanteId, clavePublica) {
  const base = `${window.location.origin}/boleta/${identificadorNegocio}/${comprobanteId}`;
  return clavePublica ? `${base}?k=${encodeURIComponent(clavePublica)}` : base;
}

/** La firma que trae la dirección de la página pública (?k=...). */
export function claveDeLaDireccion() {
  try {
    return new URLSearchParams(window.location.search).get('k') || '';
  } catch {
    return '';
  }
}
