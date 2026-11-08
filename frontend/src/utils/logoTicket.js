// Módulo "Logo en el ticket" (LOGO_TICKET): el logo de Configuración sale
// arriba del ticket de 80/58 mm (components/Recibo.jsx y su PDF,
// utils/pdfTicket.js). App.jsx lo enciende según los módulos del negocio;
// apagado (lo de siempre) el ticket sale sin logo.
let activo = false;

export function usarLogoTicket(valor) {
  activo = !!valor;
}

export const logoEnTicket = () => activo;
