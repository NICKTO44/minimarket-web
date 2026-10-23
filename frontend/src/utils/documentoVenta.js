// Cómo se arma el documento de una venta (ticket propio o si tiene hoja A4).
// Lo comparten Comprobantes (donde además se anula, se emiten notas de
// crédito y se reenvía) y Reportes (solo lectura). Antes vivía dentro de
// Comprobantes.jsx: se movió sin cambios para que los dos lugares armen
// exactamente el mismo documento (y el mismo QR).
import { api } from '../api/api';
import { fechaHoraLima } from './formato';
import { pagoParaImprimir, textoPago } from './metodoPago';

// Comprobante con hoja A4 (además del ticket de 80 mm):
// - la factura de FacturaLibre (su PDF oficial);
// - la factura o la boleta emitida directo a SUNAT y aceptada (nuestro
//   diseño, utils/facturaA4.js, armado con el documento que se envió).
export const a4Propio = (comp) => comp.proveedor === 'SUNAT_DIRECTO' && comp.estado === 'ACEPTADO' && !comp.enlace_pdf;
export const facturaConA4 = (comp) =>
  !!comp.id && ((comp.tipo === 'FACTURA' && (!!comp.enlace_pdf || a4Propio(comp))) || (comp.tipo === 'BOLETA' && a4Propio(comp)));

// Datos del ticket propio de una venta (nota simple, boleta, o comprobante
// sin PDF oficial), con los datos reales que ya trae la fila (hash, RUC
// emisor, fecha) para que el QR salga correcto. Lo usan Imprimir y PDF.
export const datosDelTicket = async (comp) => {
  const detalle = await api.ventaParaDevolucion(comp.folio_venta);
  // Desglose de IGV como se cobró (tasa, gravado, exonerado). Si el
  // servidor aún no lo ofrece, se imprime como siempre (todo al 18 %).
  const igvVenta = await api.igvVenta(comp.venta_id).catch(() => null);

  // Sin documento: la emisión directa envía "-" a SUNAT (catálogo 06,
  // ventas menores); el QR debe llevar lo mismo que el XML.
  const tipoDocCliente =
    comp.tipo === 'FACTURA'
      ? '6'
      : comp.cliente_documento
        ? comp.cliente_documento.length === 11
          ? '6'
          : '1'
        : comp.proveedor === 'SUNAT_DIRECTO'
          ? '-'
          : '0';

  // El cliente sale de la fila (la venta no lo detalla). En una factura
  // se busca además su dirección fiscal entre los clientes guardados; si
  // no aparece, el ticket sale sin dirección.
  let cliente = detalle.cliente || null;
  if (!cliente && comp.cliente_nombre) {
    cliente = { nombre_razon_social: comp.cliente_nombre, numero_documento: comp.cliente_documento || null };
    if (comp.tipo === 'FACTURA' && comp.cliente_documento) {
      const guardados = await api.clientesBuscar(comp.cliente_documento).catch(() => []);
      const guardado = Array.isArray(guardados)
        ? guardados.find((c) => c.numero_documento === comp.cliente_documento)
        : null;
      if (guardado?.direccion) cliente.direccion = guardado.direccion;
    }
  }

  return {
    // Fecha y hora de emisión (no la del momento en que se reimprime).
    fecha: fechaHoraLima(comp.fecha_emision),
    // metodoPago: para la "Forma de pago" de la factura (contado o crédito).
    venta: {
      folio: detalle.folio,
      total: detalle.total,
      montoRecibido: null,
      cambio: null,
      metodoPago: detalle.metodo_pago,
      // Con qué pagó el cliente (un medio o las dos partes de un mixto).
      pagoOtro: detalle.pago_otro,
      pagoOtroMetodo: detalle.pago_otro_metodo,
      pagoSegundo: detalle.pago_efectivo,
      pagoSegundoMetodo: detalle.pago_efectivo_metodo,
    },
    items: detalle.productos.map((p) => {
      // Descuento de la línea (un paquete): lo cobrado es `subtotal`. Sin
      // descuento la diferencia es 0 y el ticket sale como siempre.
      const descuento = Math.round((p.cantidad * p.precio_unitario - (p.subtotal ?? p.cantidad * p.precio_unitario)) * 100) / 100;
      return { nombre: p.nombre, cantidad: p.cantidad, precio: p.precio_unitario, ...(descuento > 0.004 ? { descuento } : {}) };
    }),
    comprobante: comp.id
      ? {
          tipo: comp.tipo,
          serie: comp.serie,
          numero: comp.numero,
          estado: comp.estado,
          hash: comp.hash,
          ruc_emisor: comp.ruc_emisor,
          fecha_emision: comp.fecha_emision_corta,
          igv: igvVenta?.igv ?? comp.monto - comp.monto / 1.18,
          total_venta: comp.monto,
          igv_tasa: igvVenta?.igv_tasa,
          op_gravadas: igvVenta?.op_gravadas,
          op_exoneradas: igvVenta?.op_exoneradas,
          op_inafectas: igvVenta?.op_inafectas,
          detraccion_porcentaje: igvVenta?.detraccion_porcentaje,
          detraccion_monto: igvVenta?.detraccion_monto,
          detraccion_cuenta: igvVenta?.detraccion_cuenta,
          cliente_tipo_documento_codigo: tipoDocCliente,
          cliente_numero_documento: comp.cliente_documento || '-',
        }
      : null,
    cliente,
  };
};

// Con qué pagó el cliente, en texto para la hoja A4 ("Yape", "Pago mixto:
// Yape S/ 50.00 + Plin S/ 85.00"). Si no se puede leer la venta, la hoja
// sale igual, sin esa línea.
export const pagoDeVenta = async (folio) => {
  if (!folio) return null;
  const detalle = await api.ventaParaDevolucion(folio).catch(() => null);
  return textoPago(pagoParaImprimir(detalle)) || null;
};
