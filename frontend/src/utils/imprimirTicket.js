// Imprime SOLO el ticket (components/Recibo.jsx), sin el resto del sistema.
//
// Antes se imprimía la página entera con todo invisible menos el ticket: el
// ticket quedaba donde estuviera el POS en la página y, si algo del sistema
// (invisible pero presente) era más ancho que el papel, Chrome achicaba todo
// para que entrara. Así salió una boleta de VERANE al ~70 %, corrida al
// centro y cortada. Con la clase `imprimiendo-ticket`, Recibo.css saca de la
// impresión todo el sistema (#root) y deja solo el ticket, arriba a la
// izquierda del papel.
export function imprimirTicket() {
  const raiz = document.documentElement;
  raiz.classList.add('imprimiendo-ticket');
  let quitado = false;
  const quitar = () => {
    if (quitado) return;
    quitado = true;
    raiz.classList.remove('imprimiendo-ticket');
  };
  window.addEventListener('afterprint', quitar, { once: true });
  // Respaldo por si el navegador no avisa al terminar (algunos celulares):
  // la clase no debe quedar puesta para otras impresiones (A4, PDF).
  setTimeout(quitar, 120000);
  window.print();
}
