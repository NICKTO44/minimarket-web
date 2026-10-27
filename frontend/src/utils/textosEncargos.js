// Textos de la pantalla de encargos (módulo ENCARGOS) según el rubro. Los
// de pastelería son los de siempre; la lavandería usa el mismo módulo como
// "órdenes de lavado" con sus propios nombres. El rubro los trae en
// etiquetas.textosEncargos (utils/rubros.js); si no trae, van los de pastelería.
import { CakeSlice, ChefHat, ClipboardList, WashingMachine } from 'lucide-react';

export const TEXTOS_ENCARGOS_PASTELERIA = {
  titulo: 'Pedidos por encargo',
  subtitulo: 'Tortas y pedidos para una fecha. El adelanto no emite comprobante: al entregar se cobra el saldo y sale la boleta completa.',
  Icono: CakeSlice,
  nuevo: 'Nuevo pedido',
  buscarListado: 'Buscar pedidos',
  sinCoincidencias: 'Ningún pedido coincide con la búsqueda.',
  sinPendientes: 'No hay pedidos por entregar. Toca “Nuevo pedido” cuando un cliente encargue algo.',
  sinNada: 'No hay pedidos.',
  // Formulario
  tituloNuevo: 'Nuevo pedido por encargo',
  tituloEditar: (n) => `Editar pedido N° ${n}`,
  ariaNuevo: 'Nuevo pedido',
  ariaEditar: 'Editar pedido',
  ayudaPrecio: 'El precio de cada producto es el que acordaste con el cliente; al entregar se cobra ese.',
  errorSinItems: 'Agrega lo que lleva el pedido.',
  seccionItems: 'Qué lleva',
  vacioItems: 'Busca abajo la torta o los productos del pedido.',
  etiquetaPrecio: 'Precio acordado (c/u)',
  etiquetaDetalle: 'Detalle: sabor, relleno, cobertura, tamaño',
  ejemploDetalle: 'Chocolate, relleno de manjar, cobertura de fondant, 20 porciones',
  buscarProducto: 'Agregar: busca la torta o el producto por nombre o código',
  // Botones con los servicios del negocio para agregarlos de un toque.
  serviciosRapidos: false,
  totalPedido: 'Total del pedido',
  seccionNotas: 'Para el pastelero',
  dedicatoria: true,
  botonRegistrar: 'Registrar pedido',
  // Detalle
  Pedido: 'Pedido',
  estados: { EN_PREPARACION: 'En preparación', LISTO: 'Listo' },
  botonPreparacion: 'En preparación',
  volverPreparacion: 'Volver a preparación',
  avisoNoListo: (n) => `El pedido N° ${n} aún no está marcado como listo`,
  cancelar: 'Cancelar pedido',
  altFoto: 'Foto de referencia del diseño',
  sinFoto: 'Sin foto del diseño',
  fotoEnSistema: 'Ver foto del diseño en el sistema.',
  ordenTrabajo: 'Orden de producción',
  IconoOrden: ChefHat,
  // WhatsApp
  negocio: 'la pastelería',
  tuPedido: (n) => `tu pedido N° ${n}`,
  yaListo: 'ya está listo',
  // Tickets
  ticketCliente: 'PEDIDO POR ENCARGO',
  ticketTrabajo: 'ORDEN DE PRODUCCIÓN',
  pieTicket: 'Este documento no es un comprobante de pago. La boleta o factura se entrega con el pedido.',
};

export const TEXTOS_ENCARGOS_LAVANDERIA = {
  ...TEXTOS_ENCARGOS_PASTELERIA,
  titulo: 'Órdenes de lavado',
  subtitulo:
    'Registra la ropa al recibirla: tipo de prenda, tela y precio, por kilo o por prenda. El adelanto es opcional y no emite comprobante: al entregar se cobra el saldo y sale la boleta.',
  Icono: WashingMachine,
  nuevo: 'Nueva orden',
  buscarListado: 'Buscar órdenes',
  sinCoincidencias: 'Ninguna orden coincide con la búsqueda.',
  sinPendientes: 'No hay ropa por entregar. Toca “Nueva orden” cuando un cliente deje su ropa.',
  sinNada: 'No hay órdenes.',
  tituloNuevo: 'Nueva orden de lavado',
  tituloEditar: (n) => `Editar orden N° ${n}`,
  ariaNuevo: 'Nueva orden',
  ariaEditar: 'Editar orden',
  ayudaPrecio: 'Escribe el precio de cada prenda o de los kilos al recibir la ropa; al entregar se cobra ese.',
  errorSinItems: 'Agrega las prendas o los kilos de la orden.',
  seccionItems: 'Prendas',
  vacioItems: 'Toca “Prenda nueva” y escribe la prenda con su precio, o elige un servicio de arriba.',
  etiquetaPrecio: 'Precio (c/u o por kg)',
  etiquetaDetalle: 'Prenda, tela, color y observaciones',
  ejemploDetalle: 'Terno 2 piezas, lana, azul marino, mancha en la manga',
  buscarProducto: 'Buscar otro servicio por nombre o código',
  serviciosRapidos: true,
  // Prendas escritas en la misma orden (sin crear un servicio por cada una).
  prendasEscritas: true,
  botonPrendaNueva: 'Prenda nueva',
  etiquetaPrenda: 'Prenda o servicio',
  ejemploPrenda: 'Lavado de terno negro',
  etiquetaDetalleEscrita: 'Tela, color y observaciones',
  ejemploDetalleEscrita: 'Lana, 2 piezas, mancha en la manga',
  errorPrendaSinNombre: 'Escribe qué prenda o servicio es en cada línea nueva.',
  totalPedido: 'Total de la orden',
  seccionNotas: 'Observaciones',
  dedicatoria: false,
  botonRegistrar: 'Registrar orden',
  Pedido: 'Orden',
  estados: { EN_PREPARACION: 'En lavado', LISTO: 'Listo para recoger' },
  botonPreparacion: 'En lavado',
  volverPreparacion: 'Volver a lavado',
  avisoNoListo: (n) => `La orden N° ${n} aún no está marcada como lista`,
  cancelar: 'Cancelar orden',
  altFoto: 'Foto de las prendas al recibirlas',
  sinFoto: 'Sin foto de las prendas',
  fotoEnSistema: 'Ver la foto de las prendas en el sistema.',
  ordenTrabajo: 'Orden de trabajo',
  IconoOrden: ClipboardList,
  negocio: 'la lavandería',
  tuPedido: (n) => `tu ropa (orden N° ${n})`,
  yaListo: 'ya está lista',
  ticketCliente: 'ORDEN DE LAVADO',
  ticketTrabajo: 'ORDEN DE TRABAJO',
  pieTicket: 'Presente este ticket para recoger su ropa. No es un comprobante de pago: la boleta se entrega al recoger.',
  // WhatsApp con las prendas, el total y el saldo (Encargos.jsx, mensajeLavanderia).
  whatsappDetallado: true,
};

/**
 * Servicios de respaldo de las prendas escritas en una orden de lavado
 * (backend: handlers/encargos.rs, RESPALDOS). No se muestran para elegir:
 * el nombre y el precio se ponen en cada orden.
 */
export const esRespaldoLavanderia = (p) => /^_LAV_/.test(String(p?.codigo || ''));

/** Los textos del rubro, completando con los de pastelería lo que falte. */
export function textosEncargos(etiquetas) {
  return { ...TEXTOS_ENCARGOS_PASTELERIA, ...(etiquetas?.textosEncargos || {}) };
}
