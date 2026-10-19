// Ancho del papel de la ticketera: 80 mm (por defecto) o 58 mm.
//
// Se guarda en ESTE equipo (localStorage), no en el negocio: cada celular o
// computadora tiene su propia impresora, y en un mismo local puede haber una
// de 80 mm en caja y una de 58 mm en cocina. Sin elegir nada queda en 80 mm,
// o sea, los negocios que ya trabajan no notan ningún cambio.
//
// Con 58 mm:
//  - <html> lleva la clase `papel-58`: Recibo.css y TicketMesa.css achican
//    el ticket a 48 mm (lo que imprime una ticketera de 58 mm).
//  - Se agrega al final del <body> una regla @page de 58 mm. Va al final
//    para ganarle al @page de 80 mm de Recibo.css (en @page gana la última
//    regla), incluso si alguna pantalla carga su CSS después.

const CLAVE = 'monspeet_papel_ticket';
const ID_ESTILO = 'monspeet-papel-58';

export const PAPEL_80 = '80';
export const PAPEL_58 = '58';

export function papelTicket() {
  try {
    return localStorage.getItem(CLAVE) === PAPEL_58 ? PAPEL_58 : PAPEL_80;
  } catch {
    return PAPEL_80;
  }
}

export function esPapel58() {
  return papelTicket() === PAPEL_58;
}

/** Texto para botones y menús: "Ticket 80 mm" / "Ticket 58 mm". */
export function etiquetaTicket() {
  return `Ticket ${papelTicket()} mm`;
}

export function guardarPapelTicket(valor) {
  try {
    if (valor === PAPEL_58) localStorage.setItem(CLAVE, PAPEL_58);
    else localStorage.removeItem(CLAVE);
  } catch {
    // Sin localStorage (modo privado): se aplica solo mientras siga abierto.
  }
  aplicarPapel(valor === PAPEL_58);
}

function aplicarPapel(es58) {
  document.documentElement.classList.toggle('papel-58', es58);
  let estilo = document.getElementById(ID_ESTILO);
  if (!es58) {
    estilo?.remove();
    return;
  }
  if (!estilo) {
    estilo = document.createElement('style');
    estilo.id = ID_ESTILO;
    estilo.textContent = '@media print { @page { size: 58mm auto; margin: 0; } }';
  }
  // Siempre al final: si se movió algo después, vuelve a quedar último.
  document.body.appendChild(estilo);
}

/** Se llama una vez al abrir el sistema (main.jsx). */
export function iniciarPapelTicket() {
  aplicarPapel(esPapel58());
  // Justo antes de imprimir se vuelve a poner al final, por si una pantalla
  // cargó su CSS (con el @page de 80 mm) después de abrir el sistema.
  window.addEventListener('beforeprint', () => {
    if (esPapel58()) aplicarPapel(true);
  });
}
