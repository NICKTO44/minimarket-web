// Rubros y módulos del negocio. El sistema tiene un núcleo universal
// (ventas, caja, productos, clientes, reportes...) y módulos que se
// encienden por negocio. El RUBRO es una plantilla: define qué módulos
// arrancan encendidos, qué unidades se sugieren y cómo se llaman algunas
// pantallas ("Carta" en vez de "Productos" en un restaurante).
// Debe coincidir con backend/src/handlers/rubros.rs.
import { Axe, BookOpen, CakeSlice, Hammer, Pill, Shirt, ShoppingBasket, Smartphone, Store, UtensilsCrossed, WashingMachine, Wrench } from 'lucide-react';
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
// Lubricentro / Repuestos: Precio 2 y Precio 3 por producto; el vendedor elige cuál cobrar.
export const MODULO_TRES_PRECIOS = 'TRES_PRECIOS';
// Lubricentro: punto de venta en tabla (buscador con lista y una fila por producto).
export const MODULO_VENTA_TABLA = 'VENTA_TABLA';
// Librería: autor, editorial, serie, año y edición de los libros; búsqueda y filtro por editorial.
export const MODULO_FICHA_LIBRO = 'FICHA_LIBRO';
export const MODULO_EQUIVALENTES = 'EQUIVALENTES';
export const MODULO_MENU_ARRIBA = 'MENU_ARRIBA';
// Vender algo que no está en inventario: solo sale en el comprobante.
export const MODULO_VENTA_FUERA_INVENTARIO = 'VENTA_FUERA_INVENTARIO';
// Vender sin internet: lo vendido se guarda en el equipo y se sube al volver la conexión.
export const MODULO_VENTA_SIN_CONEXION = 'VENTA_SIN_CONEXION';
// Comprobante impreso en formato clásico: datos del cliente en recuadro, sin hora y pago solo por su medio.
export const MODULO_COMPROBANTE_CLASICO = 'COMPROBANTE_CLASICO';
// Logo del negocio arriba del ticket de 80/58 mm (boleta, factura y nota de venta).
export const MODULO_LOGO_TICKET = 'LOGO_TICKET';
// Al cobrar se imprime solo: boleta y nota de venta en ticket, factura en A4.
export const MODULO_IMPRESION_DIRECTA = 'IMPRESION_DIRECTA';
// Catálogo: los productos en tarjetas con foto, solo para mirar (vendedores).
export const MODULO_CATALOGO = 'CATALOGO';
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
// Restaurante: margen por plato (costo por receta, costo fijo o %; alertas).
export const MODULO_COSTEO = 'COSTEO';
// Tecnología: órdenes de reparación (equipo, diagnóstico, presupuesto, garantía).
export const MODULO_REPARACIONES = 'REPARACIONES';
// Varias cajas abiertas a la vez: cada vendedor abre la suya (una por computadora).
export const MODULO_VARIAS_CAJAS = 'VARIAS_CAJAS';
// Ingreso rápido de productos (nuevos o para reponer) sin elegir proveedor.
export const MODULO_INGRESO_RAPIDO = 'INGRESO_RAPIDO';
// Compras por Excel: exportar la lista, llenar cantidad y costo y cargarla en Nueva compra.
export const MODULO_COMPRA_EXCEL = 'COMPRA_EXCEL';
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
export const MODULOS_SIN_RUBRO = [MODULO_GANANCIAS, MODULO_PRECIO_VENTA, MODULO_VENTA_SIN_CONEXION, MODULO_COMPROBANTE_CLASICO, MODULO_LOGO_TICKET, MODULO_IMPRESION_DIRECTA, MODULO_CATALOGO, MODULO_VARIAS_CAJAS, MODULO_INGRESO_RAPIDO, MODULO_COMPRA_EXCEL];

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
    descripcion: 'Emite la guía de remisión electrónica para trasladar lo vendido (requiere la facturación electrónica activa).',
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
    valor: MODULO_TRES_PRECIOS,
    label: '3 precios de venta',
    descripcion:
      'Además del precio de venta (Precio 1), cada producto puede tener un Precio 2 y un Precio 3 (por ejemplo para talleres o por mayor). Al vender, el vendedor ve los tres y elige cuál cobrar con un toque; también se cargan desde Excel.',
    rubros: ['LUBRICENTRO'],
  },
  {
    valor: MODULO_VENTA_TABLA,
    label: 'Venta en tabla',
    descripcion:
      'El punto de venta como tabla, igual que un sistema de facturación: buscas el producto, entra como una fila con marca, categoría, unidad, cantidad, costo, precio sin IGV, precio y descuento por producto, y abajo el subtotal, el descuento y el total.',
    rubros: ['LUBRICENTRO'],
  },
  {
    valor: MODULO_EQUIVALENTES,
    label: 'Equivalentes',
    descripcion:
      'Eliges a mano qué productos son equivalentes (resuelven lo mismo). Al buscar en Productos y en el punto de venta, debajo de cada producto salen sus equivalentes con su stock y precio, para ofrecer otro si no hay o queda poco.',
  },
  {
    valor: MODULO_MENU_ARRIBA,
    label: 'Menú arriba',
    descripcion:
      'En computadoras y laptops, el menú va en una barra arriba (en vez de la columna de la izquierda) y la pantalla gana todo el ancho. Lo que no entra queda en "Más". En tablets y celulares no cambia nada.',
  },
  {
    valor: MODULO_VENTA_FUERA_INVENTARIO,
    label: 'Vender algo que no está en inventario',
    descripcion:
      'En el punto de venta, un botón para vender algo que consigues afuera (nombre, cantidad, precio con IGV, unidad y código). Sale en la boleta, factura o nota de venta con el total completo, pero no entra a caja, reportes, ganancias, stock ni créditos: para el sistema solo cuenta lo de tu inventario.',
  },
  {
    valor: MODULO_VENTA_SIN_CONEXION,
    label: 'Venta sin conexión',
    descripcion:
      'Para cuando se va el internet: sigues vendiendo (nota, boleta o factura), abres la caja y registras gastos de caja. El cliente se lleva un comprobante provisional y, al volver la conexión, todo se sube solo y la boleta o factura se emite con la fecha real de la venta. Guarda una copia de tus productos y clientes en este equipo. Instala Monspeet como app en la computadora de la caja y vende desde ese mismo equipo mientras no haya internet.',
  },
  {
    valor: MODULO_COMPROBANTE_CLASICO,
    label: 'Comprobante formato clásico',
    descripcion:
      'Cambia cómo se imprimen tus boletas, facturas y notas (hoja A4, A5 y ticket): los datos del cliente van en un recuadro (R.U.C. o DNI, señores, vendedor, dirección y teléfono), sin la hora, y el pago solo con su medio ("Efectivo", "Yape / Plin"), sin montos. Lo que se envía a SUNAT no cambia.',
  },
  {
    valor: MODULO_IMPRESION_DIRECTA,
    label: 'Impresión directa',
    descripcion:
      'Al cobrar, el comprobante se imprime solo, sin preguntar el formato: la boleta y la nota de venta en ticket de 80 mm (o 58 mm) y la factura en hoja A4. Si SUNAT todavía no responde, la factura sale en ticket para no hacer esperar al cliente. No aplica si tu negocio imprime también en hoja A5 (Venta en tabla): ahí siempre se pregunta el formato.',
  },
  {
    valor: MODULO_LOGO_TICKET,
    label: 'Logo en el ticket',
    descripcion:
      'Pone el logo de tu negocio (el de Configuración) arriba del ticket de 80 o 58 mm: boleta, factura y nota de venta, al imprimir, al reimprimir y en el PDF que mandas por WhatsApp. Sale en blanco y negro: usa un logo de fondo blanco o transparente. Lo que se envía a SUNAT no cambia.',
  },
  {
    valor: MODULO_CATALOGO,
    label: 'Catálogo de productos',
    descripcion:
      'Una pantalla con tus productos en tarjetas con foto: marca, nombre, N.° de parte, aplicación, precios, stock, código y unidad. Es solo para mirar (no vende ni edita) y sirve para que tus vendedores revisen rápido. El stock se actualiza solo según tu inventario. Las fotos se suben desde Productos.',
  },
  {
    valor: MODULO_FICHA_LIBRO,
    label: 'Ficha del libro',
    descripcion:
      'Autor, editorial, serie o colección, año y edición de cada libro. En el punto de venta lo encuentras por título, autor, editorial o ISBN (escaneado) y filtras por editorial; también ves las ventas por editorial. Se cargan desde Excel.',
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
  {
    valor: MODULO_COSTEO,
    label: 'Margen por plato',
    descripcion:
      'Cuánto te cuesta y cuánto ganas en cada plato: costo por receta (insumos en gramos), un costo fijo o un % aproximado. Margen sin IGV (18 %, 10.5 % o Nuevo RUS), alerta de los platos que ganan menos de lo que quieres, precio sugerido, ganancia del mes y descarga en Excel.',
    // Solo se ofrece en estos rubros (el servidor tampoco lo deja en otros).
    rubros: ['RESTAURANTE'],
  },
  {
    valor: MODULO_REPARACIONES,
    label: 'Órdenes de reparación',
    descripcion:
      'El cliente deja su celular, laptop o impresora: tipo, marca, modelo, N° de serie o IMEI, lo que trae, cómo llega, la falla y su clave. Diagnóstico, presupuesto con repuestos del stock y mano de obra, aprobación por WhatsApp, técnico, adelanto sin comprobante, aviso de listo, garantía por IMEI y cobro del saldo al entregar.',
    rubros: ['TECNOLOGIA'],
  },
  {
    valor: MODULO_VARIAS_CAJAS,
    label: 'Varias cajas',
    descripcion:
      'Para vender en dos o más computadoras a la vez: cada vendedor abre su propia caja sin cerrar la de los demás, vende solo en la suya y la cuadra al cerrar. El administrador ve todas las cajas abiertas en la pantalla Caja y puede cerrar cualquiera. Para vender, el administrador también abre la suya.',
  },
  {
    valor: MODULO_INGRESO_RAPIDO,
    label: 'Ingreso rápido de productos',
    descripcion:
      'Botón "Ingreso rápido" en Productos: varios productos nuevos o para reponer en un paso, sin elegir proveedor. El código se genera solo y, si viene por caja (9 botellas), calcula las unidades y el costo de cada una. Queda como una compra en Compras y en los reportes.',
  },
  {
    valor: MODULO_COMPRA_EXCEL,
    label: 'Compras por Excel',
    descripcion:
      'En Compras, "Exportar pedido" descarga tus productos (todos, una categoría o los que están en su stock mínimo) en Excel. Llenas la cantidad y el costo (o se lo mandas a tu proveedor) y en Nueva compra lo cargas con "Cargar desde Excel": entran todos los productos de un golpe. Los que no tienes se crean ahí mismo.',
  },
];

/** ¿El módulo se puede encender en ese rubro? */
export const moduloDisponible = (modulo, rubro) => !modulo.rubros || modulo.rubros.includes(rubro);

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
    modulos: [MODULO_FICHA_MEDICAMENTO, MODULO_RECETAS, MODULO_ALTERNATIVAS, MODULO_PRESENTACIONES, MODULO_COMPRA_EXCEL],
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
      MODULO_TRES_PRECIOS,
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
    valor: 'TECNOLOGIA',
    label: 'Tecnología / Reparaciones',
    descripcion:
      'Accesorios de celulares, laptops e impresoras (cargadores, cables, tóner, tintas) y servicio técnico: órdenes de reparación con el equipo, diagnóstico, presupuesto, garantía y cobro al entregar. Con proformas, crédito y precio por mayor.',
    Icono: Smartphone,
    modulos: [MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRECIO_MAYOR, MODULO_REPARACIONES],
    unidades: ['UNIDAD', 'PAQUETE', 'CAJA', 'PAR', 'JUEGO', 'METRO', 'ROLLO', 'PIEZA'],
    etiquetas: {
      sinStock: 'Es un servicio (mano de obra, instalación, revisión): se vende sin controlar stock',
    },
  },
  {
    valor: 'LIBRERIA',
    label: 'Librería',
    descripcion:
      'Libros, útiles escolares y de oficina: búsqueda por título, autor, editorial o ISBN, venta por docena o ciento, listas escolares cotizadas, fotocopias e impresiones, y precio por mayor.',
    Icono: BookOpen,
    modulos: [MODULO_SERVICIOS, MODULO_COTIZACIONES, MODULO_CREDITO, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR, MODULO_FICHA_LIBRO],
    unidades: ['UNIDAD', 'PAQUETE', 'CAJA', 'DOCENA', 'CIENTO', 'MILLAR', 'PLIEGO', 'JUEGO', 'ROLLO', 'BOLSA', 'SOBRE', 'PIEZA', 'METRO'],
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
