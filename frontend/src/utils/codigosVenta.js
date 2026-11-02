import { api } from '../api/api';

// Módulo "Venta en tabla": el código interno de cada línea de una venta, en
// el mismo orden que las líneas de su comprobante (las dos van por el orden
// en que se guardaron). Es solo para la hoja impresa: si algo falla, la hoja
// sale igual, sin códigos.
export async function codigosDeVenta(folio) {
  if (!folio) return null;
  const [venta, productos] = await Promise.all([
    api.ventaParaDevolucion(folio).catch(() => null),
    api.productos().catch(() => null),
  ]);
  if (!Array.isArray(venta?.productos) || !Array.isArray(productos)) return null;
  const codigoDe = new Map(productos.map((p) => [p.id, p.codigo]));
  return venta.productos.map((l) => codigoDe.get(l.producto_id) || '');
}
