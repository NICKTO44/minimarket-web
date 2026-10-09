// Impresión de las notas de crédito de la emisión directa: se arman con el
// documento que se envió a SUNAT (igual que el A4 de facturas y boletas).
import { api, API_URL } from '../api/api';
import { datosA4DeDocumento, imprimirComprobanteA4, imprimirComprobanteTicket } from './facturaA4';

/** Datos del negocio que el documento no trae (logo, color, contacto). */
export function extraDelNegocio(cfg, telefono) {
  return {
    telefono: cfg?.telefono || telefono,
    email: cfg?.email,
    logo: cfg?.logo_path ? `${API_URL}${cfg.logo_path}` : null,
    color: cfg?.color_acento,
  };
}

/**
 * Imprime una nota de crédito. formato: 'ticket' (80 mm) o 'a4'.
 * nota = { id, hash }; cfg = configuración del negocio (opcional).
 */
export async function imprimirNotaCredito(nota, formato, cfg, telefono) {
  const documento = await api.notaCreditoDocumento(nota.id);
  const negocio = cfg === undefined ? await api.configuracionObtener().catch(() => null) : cfg;
  const datos = await datosA4DeDocumento(documento, { ...extraDelNegocio(negocio, telefono), hash: nota.hash });
  if (formato === 'a4') imprimirComprobanteA4(datos);
  else imprimirComprobanteTicket(datos);
}
