import { api } from '../api/api';
import { etiquetaUnidad } from './unidades';
import { lineasFuera } from './fueraInventario';

// Módulo "Venta en tabla": el código interno de cada línea de una venta, en
// el mismo orden que las líneas de su comprobante (las dos van por el orden
// en que se guardaron). Es solo para la hoja impresa: si algo falla, la hoja
// sale igual, sin códigos.
export async function codigosDeVenta(folio) {
  const lineas = await lineasDeVenta(folio);
  return lineas ? lineas.map((l) => l.codigo) : null;
}

/** Tasa de IGV con la que se calcula el "precio sin IGV" impreso. */
export const TASA_IGV_IMPRESA = 0.18;

/** Precio sin IGV de una línea (exonerados e inafectos no llevan IGV). */
export function precioSinIgv(precio, afectacion, tasa = TASA_IGV_IMPRESA) {
  const p = Number(precio) || 0;
  return !afectacion || afectacion === 'GRAVADO' ? p / (1 + tasa) : p;
}

/**
 * Código interno, afectación de IGV y unidad de cada línea de una venta, en
 * el orden de sus líneas: [{ codigo, afectacion, unidad }]. null si algo falla.
 */
export async function lineasDeVenta(folio) {
  if (!folio) return null;
  const [venta, productos] = await Promise.all([
    api.ventaParaDevolucion(folio).catch(() => null),
    api.productos().catch(() => null),
  ]);
  if (!Array.isArray(venta?.productos) || !Array.isArray(productos)) return null;
  const producto = new Map(productos.map((p) => [p.id, p]));
  // Lo de afuera va al final del comprobante, con su propio código (gravado).
  return [
    ...venta.productos.map((l) => ({
      codigo: producto.get(l.producto_id)?.codigo || '',
      afectacion: producto.get(l.producto_id)?.afectacion_igv || null,
      unidad: producto.get(l.producto_id) ? etiquetaUnidad(producto.get(l.producto_id).unidad_medida) : 'Unidad',
    })),
    ...lineasFuera(venta).map((l) => ({ codigo: l.codigo, afectacion: 'GRAVADO', unidad: etiquetaUnidad(l.unidad_medida) })),
  ];
}
