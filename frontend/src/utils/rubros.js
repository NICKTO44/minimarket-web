// Rubros y módulos del negocio. El sistema tiene un núcleo universal
// (ventas, caja, productos, clientes, reportes...) y módulos que se
// encienden por negocio. El RUBRO es una plantilla: define qué módulos
// arrancan encendidos, qué unidades se sugieren y cómo se llaman algunas
// pantallas ("Carta" en vez de "Productos" en un restaurante).
// Debe coincidir con backend/src/handlers/rubros.rs.
import { Axe, CakeSlice, Hammer, Pill, Shirt, ShoppingBasket, Store, UtensilsCrossed, WashingMachine, Wrench } from 'lucide-react';
import { UNIDADES_CLASICAS } from './unidades';
import { TEXTOS_ENCARGOS_LAVANDERIA } from './textosEncargos';

export const MODULO_MESAS = 'MESAS';
export const MODULO_SERVICIOS = 'SERVICIOS';
export const MODULO_MEDIDAS = 'MEDIDAS';
export const MODULO_DETRACCION = 'DETRACCION';
export const MODULO_COTIZACIONES = 'COTIZACIONES';
export const MODULO_CREDITO = 'CREDITO';
export const MODULO_GUIAS = 'GUIAS';
export const MODULO_VARIANTES = 'VARIANTES';
export const MODULO_CAMBIOS = 'CAMBIOS';
// No pertenece a ningún rubro: viene apagado y lo enciende quien lo quiera.
export const MODULO_GANANCIAS = 'GANANCIAS';
export const MODULO_PRECIO_VENTA = 'PRECIO_VENTA';
// Ferretería: el mismo producto en rollo, caja o ciento, y precio por mayor.
export const MODULO_PRESENTACIONES = 'PRESENTACIONES';
export const MODULO_PRECIO_MAYOR = 'PRECIO_MAYOR';
// Farmacia / Botica: datos del medicamento (principio activo, receta...).
export const MODULO_FICHA_MEDICAMENTO = 'FICHA_MEDICAMENTO';
// Farmacia: receta obligatoria al cobrar y libro de recetas. Necesita la ficha.
export const MODULO_RECETAS = 'RECETAS';
// Farmacia: productos con el mismo principio activo, concentración y forma.
export const MODULO_ALTERNATIVAS = 'ALTERNATIVAS';
// Lubricentro / Repuestos: marca, n.° de parte, códigos alternos (y
// equivalencias), ubicación y garantía.
export const MODULO_FICHA_REPUESTO = 'FICHA_REPUESTO';
// Lubricentro / Repuestos: vehículos compatibles y búsqueda por vehículo.
export const MODULO_COMPATIBILIDAD = 'COMPATIBILIDAD';
// Lubricentro: vehículos de los clientes, placa y kilometraje en la venta.
export const MODULO_VEHICULOS = 'VEHICULOS';
// Lubricentro: órdenes de servicio, mecánicos y comisiones. Necesita Vehículos.
export const MODULO_ORDENES_SERVICIO = 'ORDENES_SERVICIO';
// Paquetes de productos a un precio ("Cambio de aceite completo").
export const MODULO_PAQUETES = 'PAQUETES';
// Pastelería: pedidos por encargo con fecha de entrega y adelanto.
// Lavandería: el mismo módulo son las órdenes de lavado (utils/textosEncargos.js).
export const MODULO_ENCARGOS = 'ENCARGOS';
// Producción del día y mermas; recetas de producción con insumos y costo.
export const MODULO_PRODUCCION = 'PRODUCCION';
export const MODULO_INSUMOS = 'INSUMOS';
// Dark kitchen: pedidos por WhatsApp / delivery (necesita la atención en mesas).
export const MODULO_DELIVERY = 'DELIVERY';
// Módulo -> el que necesita encendido (igual que DEPENDENCIAS en rubros.rs).
export const DEPENDENCIAS = {
  [MODULO_RECETAS]: MODULO_FICHA_MEDICAMENTO,
  [MODULO_ALTERNATIVAS]: MODULO_FICHA_MEDICAMENTO,
  [MODULO_ORDENES_SERVICIO]: MODULO_VEHICULOS,
  [MODULO_INSUMOS]: MODULO_PRODUCCION,
  [MODULO_DELIVERY]: MODULO_MESAS,
};

/**
 * Módulos al encender o apagar uno, respetando las dependencias: encender
 * Recetas enciende la Ficha; apagar la Ficha apaga Recetas.
 */
export function alternarConDependencias(modulos, valor) {
  if (modulos.includes(valor)) {
    const apagados = new Set([valor]);
    for (const [modulo, necesita] of Object.entries(DEPENDENCIAS)) if (apagados.has(necesita)) apagados.add(modulo);
    return modulos.filter((m) => !apagados.has(m));
  }
  const nuevos = [...modulos, valor];
  const necesita = DEPENDENCIAS[valor];
  return necesita && !nuevos.includes(necesita) ? [...nuevos, necesita] : nuevos;
}
// Módulos que no dependen del rubro: al cambiar de rubro se conservan.
export const MODULOS_SIN_RUBRO = [MODULO_GANANCIAS, MODULO_PRECIO_VENTA];

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
  {
    valor: MODULO_GANANCIAS,
    label: 'Reporte de ganancias',
    descripcion:
      'Cuánto ganaste cada mes: lo que vendiste contra lo que te costó, con comparativa entre meses. Al activarlo, el precio de compra es obligatorio en cada producto.',
  },
  {
    valor: MODULO_PRECIO_VENTA,
    label: 'Cambiar precio al vender',
    descripcion:
      'En el punto de venta puedes cambiar el precio de un producto solo para esa venta (una rebaja, un precio por mayor). La boleta o factura sale con ese precio y el producto conserva el suyo.',
  },
  {
    valor: MODULO_PRESENTACIONES,
    label: 'Presentaciones',
    descripcion:
      'Vende el mismo producto de varias formas, cada una con su precio: cable por metro o por rollo de 100 m, tornillos por unidad, ciento o millar. El stock sigue en una sola unidad.',
  },
  {
    valor: MODULO_PRECIO_MAYOR,
    label: 'Precio por mayor',
    descripcion:
      'Un precio menor desde cierta cantidad (desde 50 unidades, S/ 0.80 c/u). Al vender se aplica solo cuando el cliente lleva esa cantidad.',
  },
  {
    valor: MODULO_FICHA_MEDICAMENTO,
    label: 'Ficha del medicamento',
    descripcion:
      'Principio activo, concentración, forma farmacéutica, laboratorio, registro sanitario y condición de venta de cada medicamento. Se busca por principio activo y el punto de venta avisa cuál pide receta.',
  },
  {
    valor: MODULO_RECETAS,
    label: 'Control de recetas',
    descripcion:
      'No se cobra un medicamento "con receta" sin confirmar que el cliente la presentó, ni uno de receta retenida sin médico, CMP, paciente y N.° de receta. Todo queda en el libro de recetas. Enciende también la ficha del medicamento.',
  },
  {
    valor: MODULO_ALTERNATIVAS,
    label: 'Alternativas de medicamentos',
    descripcion:
      'En el punto de venta, si un medicamento no tiene stock (o el cliente busca algo más barato), muestra los que tienen el mismo principio activo, concentración y forma, con su precio y stock. Enciende también la ficha del medicamento.',
  },
  {
    valor: MODULO_FICHA_REPUESTO,
    label: 'Ficha del repuesto',
    descripcion:
      'Marca, n.° de parte, códigos alternos (el del fabricante, el de Toyota, el de Wix...), ubicación en el almacén y garantía. El punto de venta encuentra el repuesto por cualquiera de sus códigos y muestra los equivalentes cuando no hay stock.',
  },
  {
    valor: MODULO_COMPATIBILIDAD,
    label: 'Compatibilidad con vehículos',
    descripcion:
      'Con qué vehículos va cada repuesto (marca, modelo, años y motor). En el punto de venta eliges el vehículo y ves solo lo que le sirve.',
  },
  {
    valor: MODULO_VEHICULOS,
    label: 'Vehículos y kilometraje',
    descripcion:
      'Vehículos de tus clientes por placa. En la venta anotas la placa, el kilometraje y el próximo cambio (salen en el ticket); el historial muestra qué se le puso cada vez, y los recordatorios avisan por WhatsApp cuando le toca.',
  },
  {
    valor: MODULO_ORDENES_SERVICIO,
    label: 'Órdenes de servicio',
    descripcion:
      'El vehículo entra al taller: abres una orden, vas agregando repuestos y mano de obra, asignas al mecánico y la cobras al terminar. Con comisión por mecánico. Enciende también Vehículos y kilometraje.',
  },
  {
    valor: MODULO_PAQUETES,
    label: 'Paquetes de productos',
    descripcion:
      'Varios productos a un solo precio ("Cambio de aceite completo": aceite + filtro + mano de obra). En la venta entra cada producto con su descuento y se descuenta el stock de cada uno.',
  },
  {
    valor: MODULO_ENCARGOS,
    label: 'Pedidos por encargo',
    descripcion:
      'Tortas y pedidos para una fecha: detalle (sabor, relleno, dedicatoria), foto del diseño y adelantos sin emitir comprobante. Al entregar se cobra el saldo en el punto de venta y sale la boleta completa.',
  },
  {
    valor: MODULO_PRODUCCION,
    label: 'Producción y mermas',
    descripcion:
      'Lo que se produce cada día entra al stock sin hacerlo como compra (con su lote si vence). Las mermas (lo que no se vendió o se malogró) salen con su costo. Resumen diario: producido, vendido y merma.',
  },
  {
    valor: MODULO_INSUMOS,
    label: 'Recetas e insumos',
    descripcion:
      'La receta de producción de cada producto (cuánto rinde y qué insumos lleva): cuánto cuesta hacerlo y cuánto ganas. Al registrar la producción se descuentan los insumos. Enciende también Producción y mermas.',
  },
  {
    valor: MODULO_DELIVERY,
    label: 'Pedidos delivery (dark kitchen)',
    descripcion:
      'Para cocinas sin local que atienden por WhatsApp: el tablero de pedidos reemplaza a las mesas, con cliente, dirección y costo de envío; pago adelantado (no sale sin cobrar) o contra entrega con vuelto; repartidor, en camino y entregado; y "Por rendir": cuánto debe entregar cada repartidor. Enciende también la atención en mesas.',
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
    descripcion: 'Materiales, herramientas y pinturas: por unidad, metro, kilo o galón, en rollo, caja o ciento, con precio por mayor, proformas y crédito.',
    Icono: Hammer,
    modulos: [MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_GUIAS, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR],
    unidades: [
      'UNIDAD', 'KG', 'LITRO', 'PAQUETE', 'CAJA', 'DOCENA', 'PAR', 'METRO', 'GALON', 'BOLSA', 'ROLLO', 'MILLAR', 'JUEGO',
      'PIEZA', 'PLANCHA', 'CIENTO', 'VARILLA', 'BALDE', 'LATA', 'PLIEGO', 'TUBO',
    ],
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
    valor: 'FARMACIA',
    label: 'Farmacia / Botica',
    descripcion: 'Medicamentos por principio activo, con lotes y vencimientos, venta por tableta, blíster o caja y control de recetas.',
    Icono: Pill,
    modulos: [MODULO_FICHA_MEDICAMENTO, MODULO_RECETAS, MODULO_ALTERNATIVAS, MODULO_PRESENTACIONES],
    unidades: [
      'UNIDAD', 'TABLETA', 'CAPSULA', 'BLISTER', 'CAJA', 'FRASCO', 'AMPOLLA', 'VIAL', 'SOBRE', 'TUBO',
      'SUPOSITORIO', 'OVULO', 'PARCHE', 'ML', 'GRAMO', 'PAQUETE',
    ],
    etiquetas: {},
  },
  {
    valor: 'LUBRICENTRO',
    label: 'Lubricentro / Repuestos',
    descripcion: 'Aceites, filtros y repuestos por marca, código y vehículo compatible; placa y kilometraje, órdenes de servicio y paquetes.',
    Icono: Wrench,
    modulos: [
      MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR,
      MODULO_FICHA_REPUESTO, MODULO_COMPATIBILIDAD, MODULO_VEHICULOS, MODULO_ORDENES_SERVICIO, MODULO_PAQUETES,
    ],
    unidades: [
      'UNIDAD', 'LITRO', 'GALON', 'ML', 'KG', 'PAR', 'JUEGO', 'CAJA', 'PAQUETE', 'BALDE', 'LATA', 'TUBO', 'BOTELLA',
      'METRO', 'ROLLO', 'PIEZA',
    ],
    etiquetas: {},
  },
  {
    valor: 'PASTELERIA',
    label: 'Pastelería / Panadería',
    descripcion:
      'Tortas, postres, pan y bocaditos: venta por unidad, porción o ciento, pedidos por encargo con adelanto, producción del día, mermas y costo por receta.',
    Icono: CakeSlice,
    modulos: [
      MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_COTIZACIONES, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR, MODULO_ENCARGOS,
      MODULO_PRODUCCION, MODULO_INSUMOS,
    ],
    unidades: [
      'UNIDAD', 'PORCION', 'ENTERO', 'MEDIO', 'CUARTO', 'DOCENA', 'CIENTO', 'KG', 'GRAMO', 'PAQUETE', 'CAJA', 'BOLSA',
      'TAZA', 'VASO', 'BOTELLA',
    ],
    etiquetas: {},
  },
  {
    valor: 'LAVANDERIA',
    label: 'Lavandería',
    descripcion:
      'Órdenes de lavado por kilo o por prenda (tipo de prenda, tela y precio al recibirla), con adelanto opcional, recojo o delivery, aviso por WhatsApp cuando está lista, clientes a cuenta con cierre de mes y boleta o nota de venta al entregar.',
    Icono: WashingMachine,
    modulos: [MODULO_SERVICIOS, MODULO_MEDIDAS, MODULO_ENCARGOS, MODULO_CREDITO],
    unidades: ['UNIDAD', 'KG', 'PRENDA', 'PAR', 'JUEGO', 'PIEZA'],
    etiquetas: {
      PRODUCTOS: 'Servicios',
      ENCARGOS: 'Órdenes de lavado',
      tituloProductos: 'Servicios y precios',
      nuevoProducto: '+ Nuevo servicio',
      sinStock: 'Es un servicio (lavado, planchado, al seco): se vende sin controlar stock',
      iconos: { ENCARGOS: WashingMachine },
      textosEncargos: TEXTOS_ENCARGOS_LAVANDERIA,
      // Créditos: pestaña "Por cliente" y estado de cuenta del mes (solo aquí;
      // los demás rubros ven Créditos como siempre).
      cierreMes: true,
    },
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
