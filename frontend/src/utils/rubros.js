// Rubros y módulos del negocio. El sistema tiene un núcleo universal
// (ventas, caja, productos, clientes, reportes...) y módulos que se
// encienden por negocio. El RUBRO es una plantilla: define qué módulos
// arrancan encendidos, qué unidades se sugieren y cómo se llaman algunas
// pantallas ("Carta" en vez de "Productos" en un restaurante).
// Debe coincidir con backend/src/handlers/rubros.rs.
import { Axe, Hammer, Shirt, ShoppingBasket, Store, UtensilsCrossed } from 'lucide-react';
import { UNIDADES_CLASICAS } from './unidades';

export const MODULO_MESAS = 'MESAS';
export const MODULO_SERVICIOS = 'SERVICIOS';
export const MODULO_MEDIDAS = 'MEDIDAS';
export const MODULO_DETRACCION = 'DETRACCION';
export const MODULO_COTIZACIONES = 'COTIZACIONES';
export const MODULO_CREDITO = 'CREDITO';
export const MODULO_GUIAS = 'GUIAS';
export const MODULO_VARIANTES = 'VARIANTES';
export const MODULO_CAMBIOS = 'CAMBIOS';

// Módulos que ya existen (los nuevos se agregan aquí cuando estén listos).
export const MODULOS = [
  {
    valor: MODULO_MESAS,
    label: 'Atención en mesas',
    descripcion: 'Mesas, pedidos abiertos, comandas a barra o cocina, carta de hoy y opciones por producto.',
  },
  {
    valor: MODULO_SERVICIOS,
    label: 'Servicios y venta sin stock',
    descripcion: 'Vende lo que no descuenta stock: un corte, una instalación, un delivery o algo preparado al momento.',
  },
  {
    valor: MODULO_MEDIDAS,
    label: 'Venta por medidas',
    descripcion: 'Cantidades con decimales en el punto de venta (0.5 kg, 37.5 pies) y calculadora de pie tablar para madera.',
  },
  {
    valor: MODULO_DETRACCION,
    label: 'Detracción en facturas',
    descripcion: 'Facturas sujetas al SPOT (madera, arena y piedra, servicios): porcentaje, código y cuenta del Banco de la Nación configurables.',
  },
  {
    valor: MODULO_COTIZACIONES,
    label: 'Cotizaciones',
    descripcion: 'Guarda una proforma desde el punto de venta, imprímela y cárgala después para venderla al precio ofrecido.',
  },
  {
    valor: MODULO_CREDITO,
    label: 'Ventas al crédito',
    descripcion: 'Vende al fiado con o sin adelanto, registra los abonos y mira cuánto te debe cada cliente.',
  },
  {
    valor: MODULO_GUIAS,
    label: 'Guías de remisión',
    descripcion: 'Emite la guía de remisión electrónica para trasladar lo vendido (requiere FacturaLibre configurado).',
  },
  {
    valor: MODULO_VARIANTES,
    label: 'Tallas y colores',
    descripcion: 'Un modelo con sus tallas y colores: cada una con su propio código de barras, precio y stock, y se elige al vender.',
  },
  {
    valor: MODULO_CAMBIOS,
    label: 'Cambio de prenda',
    descripcion: 'El cliente devuelve una prenda y se lleva otra en la misma operación; solo se cobra o se devuelve la diferencia.',
  },
];

export const RUBRO_GENERAL = 'GENERAL';

// etiquetas: nombres que cambian en ese rubro. La clave es el id de la
// pantalla del menú (utils/menu.js) o un texto de la pantalla de productos.
export const RUBROS = [
  {
    valor: 'BODEGA',
    label: 'Bodega / Minimarket',
    descripcion: 'Abarrotes, licorería, panadería, bazar: venta directa en el punto de venta.',
    Icono: ShoppingBasket,
    modulos: [],
    unidades: null,
    etiquetas: {},
  },
  {
    valor: 'RESTAURANTE',
    label: 'Restaurante / Cafetería',
    descripcion: 'Mesas, pedidos, comandas a cocina, carta fija y carta del día.',
    Icono: UtensilsCrossed,
    modulos: [MODULO_MESAS, MODULO_SERVICIOS],
    unidades: ['UNIDAD', 'PLATO', 'PORCION', 'ENTERO', 'MEDIO', 'CUARTO', 'VASO', 'TAZA', 'JARRA', 'COPA', 'BOTELLA'],
    etiquetas: {
      PRODUCTOS: 'Carta',
      tituloProductos: 'Carta',
      nuevoProducto: '+ Nuevo plato o producto',
      sinStock: 'Preparado al momento (café, jugo, plato): se vende sin controlar stock',
    },
  },
  {
    valor: 'FERRETERIA',
    label: 'Ferretería',
    descripcion: 'Materiales, herramientas y pinturas; venta por unidad, metro, kilo o galón.',
    Icono: Hammer,
    modulos: [MODULO_SERVICIOS],
    unidades: ['UNIDAD', 'KG', 'LITRO', 'PAQUETE', 'CAJA', 'DOCENA', 'PAR', 'METRO', 'GALON', 'BOLSA', 'ROLLO', 'MILLAR', 'JUEGO', 'PIEZA', 'PLANCHA'],
    etiquetas: {},
  },
  {
    valor: 'MADERERA',
    label: 'Maderera',
    descripcion: 'Madera por pie tablar, pieza o plancha, más servicios de corte y cepillado.',
    Icono: Axe,
    modulos: [MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_DETRACCION, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_GUIAS],
    unidades: ['UNIDAD', 'PIE_TABLAR', 'PIEZA', 'PLANCHA', 'METRO', 'M2', 'M3', 'KG', 'GALON', 'CAJA', 'MILLAR'],
    etiquetas: {},
  },
  {
    valor: 'ROPA',
    label: 'Ropa y calzado',
    descripcion: 'Prendas y zapatos por talla y color, cada una con su precio y su código; cambios de prenda y separados.',
    Icono: Shirt,
    modulos: [MODULO_CREDITO, MODULO_VARIANTES, MODULO_CAMBIOS],
    unidades: ['UNIDAD', 'PAR', 'DOCENA', 'PAQUETE', 'CAJA', 'JUEGO'],
    etiquetas: {},
  },
  {
    valor: RUBRO_GENERAL,
    label: 'Otro rubro',
    descripcion: 'Cualquier otro negocio: empieza con lo universal y enciende módulos cuando los necesites.',
    Icono: Store,
    modulos: [],
    unidades: null,
    etiquetas: {},
  },
];

const POR_VALOR = new Map(RUBROS.map((r) => [r.valor, r]));

/** Datos del rubro (si no se conoce, "Otro rubro"). */
export function rubroDe(valor) {
  return POR_VALOR.get(valor) || POR_VALOR.get(RUBRO_GENERAL);
}

/** Unidades con las que conviene empezar en ese rubro. */
export function unidadesRecomendadas(rubro) {
  return rubroDe(rubro).unidades || UNIDADES_CLASICAS;
}

/**
 * Rubro y módulos del negocio a partir de lo que mandó el servidor
 * (configuración o login). Si el servidor aún no los manda, se deducen de
 * modo_negocio, igual que funcionaba antes.
 */
export function datosNegocio(fuente) {
  const mesas = fuente?.modo_negocio === 'RESTAURANTE';
  const modulos = Array.isArray(fuente?.modulos) ? fuente.modulos : mesas ? [MODULO_MESAS, MODULO_SERVICIOS] : [];
  const rubro = fuente?.rubro || (mesas ? 'RESTAURANTE' : RUBRO_GENERAL);
  return { rubro, modulos, modo_negocio: mesas ? 'RESTAURANTE' : 'TIENDA' };
}
