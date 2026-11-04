// Módulo "Venta en tabla" (Lubricentro): pedido a proveedor por Excel.
// 1) Exportar: los productos con su código interno (y, en el completo,
//    stock, costo y P1/P2/P3) y una columna "Cantidad a pedir".
// 2) Cargar: ese mismo Excel con las cantidades llena la "Nueva compra".
// No guarda nada por sí solo: la compra se registra y se recibe como
// siempre (Proveedores -> Nueva compra -> Recibir).
import { leerNumero, normalizar } from './importacion';
import { claveCodigo } from './repuestos';
import { seVendeFraccionado } from './medidas';
import { GRUPOS_UNIDADES, etiquetaUnidad } from './unidades';

export const FILTROS_PEDIDO = [
  { valor: 'BAJO_MINIMO', label: 'Solo los que están en el stock mínimo o debajo' },
  { valor: 'TODOS', label: 'Todos los productos' },
  { valor: 'CATEGORIA', label: 'Una categoría' },
];

const controlaStock = (p) => p.controla_stock !== false;
export const estaBajoMinimo = (p) => controlaStock(p) && Number(p.stock_minimo) > 0 && Number(p.stock) <= Number(p.stock_minimo);

/** Cuánto pedir de un producto bajo el mínimo: lo que falta para llegar a él (al menos 1). */
export function cantidadSugerida(p) {
  const falta = Number(p.stock_minimo || 0) - Number(p.stock || 0);
  const n = seVendeFraccionado(p.unidad_medida) ? Math.round(falta * 1000) / 1000 : Math.ceil(falta);
  return Math.max(n, 1);
}

/**
 * Los productos que entran al pedido, ordenados por categoría y nombre. Los
 * servicios (no controlan stock: mano de obra) no se le compran a nadie.
 */
export function productosDelPedido(productos, { filtro, categoriaId }) {
  return productos
    .filter(controlaStock)
    .filter((p) => (filtro === 'BAJO_MINIMO' ? estaBajoMinimo(p) : filtro === 'CATEGORIA' ? String(p.categoria_id) === String(categoriaId) : true))
    .sort(
      (a, b) =>
        String(a.categoria_nombre || '').localeCompare(String(b.categoria_nombre || ''), 'es') ||
        String(a.nombre).localeCompare(String(b.nombre), 'es')
    );
}

/** Fila guía al final del Excel del proveedor (no es un producto). */
export const TITULO_NUEVOS = 'PRODUCTOS NUEVOS: agrega debajo los que no están en la lista (deja el Código vacío)';

const COLUMNAS_PROVEEDOR = ['Código', 'N.° de parte', 'Marca', 'Producto', 'Aplicación', 'Unidad', 'Cantidad a pedir', 'Precio unitario'];
const COLUMNAS_COMPLETO = [
  'Código', 'N.° de parte', 'Marca', 'Producto', 'Aplicación', 'Categoría', 'Unidad',
  'Stock', 'Stock mínimo', 'Costo actual', 'P1 Público', 'P2 Tienda', 'P3 Mayor', 'Cantidad a pedir', 'Costo nuevo',
];

/**
 * Las hojas del Excel (para escribirLibro). completo = con stock, costos y
 * P1/P2/P3 (para el negocio); si no, solo lo que necesita el proveedor.
 * fichas: Map producto_id -> ficha del repuesto; precios: Map -> { precio_2, precio_3 }.
 */
export function hojaPedido(lista, { completo, fichas, precios, sugerir, conStock = false }) {
  const numero = (n) => (Number(n) > 0 ? Number(n) : '');
  const filas = lista.map((p) => {
    const ficha = fichas?.get(p.id);
    const cantidad = sugerir && estaBajoMinimo(p) ? cantidadSugerida(p) : '';
    const comunes = [String(p.codigo || ''), ficha?.numero_parte || '', ficha?.marca || '', p.nombre, p.descripcion || ''];
    // conStock: columna "Stock actual" (para el negocio; sin ella, para el proveedor).
    if (!completo) {
      return [...comunes, etiquetaUnidad(p.unidad_medida), ...(conStock ? [controlaStock(p) ? Number(p.stock) : ''] : []), cantidad, ''];
    }
    const extra = precios?.get(p.id);
    return [
      ...comunes,
      p.categoria_nombre || '',
      etiquetaUnidad(p.unidad_medida),
      controlaStock(p) ? Number(p.stock) : '',
      controlaStock(p) ? Number(p.stock_minimo) : '',
      numero(p.precio_compra),
      numero(p.precio),
      numero(extra?.precio_2),
      numero(extra?.precio_3),
      cantidad,
      '',
    ];
  });
  const columnas = conStock ? [...COLUMNAS_PROVEEDOR.slice(0, 6), 'Stock actual', ...COLUMNAS_PROVEEDOR.slice(6)] : COLUMNAS_PROVEEDOR;
  if (!completo) {
    // Al final, un espacio para que el proveedor agregue lo que el negocio
    // aún no tiene (sin código: al cargarlo salen como "Productos nuevos").
    filas.push(columnas.map(() => ''));
    filas.push(columnas.map((_, i) => (i === 3 ? TITULO_NUEVOS : '')));
  }
  return completo
    ? { nombre: 'Pedido', filas: [COLUMNAS_COMPLETO, ...filas], anchos: [14, 15, 14, 42, 38, 18, 10, 8, 12, 12, 11, 11, 11, 16, 13], soles: [9, 10, 11, 12, 14] }
    : conStock
      ? { nombre: 'Pedido', filas: [columnas, ...filas], anchos: [14, 15, 14, 42, 38, 10, 13, 16, 15], soles: [8] }
      : { nombre: 'Pedido', filas: [columnas, ...filas], anchos: [14, 15, 14, 42, 38, 10, 16, 15], soles: [7] };
}

// Títulos que se reconocen al cargar (ya normalizados).
const TITULOS = {
  codigo: ['codigo', 'codigointerno', 'cod', 'sku', 'referencia'],
  parte: ['ndeparte', 'nodeparte', 'numerodeparte', 'nparte', 'partnumber', 'proveedor'],
  cantidad: ['cantidadapedir', 'cantidadpedida', 'cantidad', 'cant', 'pedido', 'apedir'],
  costoNuevo: ['costonuevo', 'preciounitario', 'preciodecompra', 'preciocompra', 'costo', 'costounitario'],
  costoActual: ['costoactual'],
  // Para los productos nuevos que agrega el proveedor.
  producto: ['producto', 'descripcion', 'nombre', 'articulo'],
  marca: ['marca'],
  aplicacion: ['aplicacion'],
  unidad: ['unidad', 'unidaddemedida', 'um'],
  // Precios de venta (Excel "Completo"): si se cambian, se actualizan al registrar la compra.
  p1: ['p1publico', 'p1', 'precio1', 'preciopublico', 'preciodeventa'],
  p2: ['p2tienda', 'p2', 'precio2', 'preciotienda'],
  p3: ['p3mayor', 'p3', 'precio3', 'preciomayor', 'precioxmayor', 'preciopormayor'],
};

function columnas(titulos) {
  const claves = titulos.map(normalizar);
  const col = {};
  for (const [campo, nombres] of Object.entries(TITULOS)) {
    for (const nombre of nombres) {
      const i = claves.findIndex((c, idx) => c === nombre && !Object.values(col).includes(idx));
      if (i >= 0) {
        col[campo] = i;
        break;
      }
    }
  }
  return col;
}

const UNIDADES = GRUPOS_UNIDADES.flatMap((g) => g.unidades);

/** "Unidad", "Galón", "GLN", "litros"… -> valor de unidad; si no se reconoce, Unidad. */
export function unidadDesdeTexto(texto) {
  const t = normalizar(texto);
  if (!t) return 'UNIDAD';
  const u = UNIDADES.find((x) => [x.valor, x.label, x.abrev].some((v) => v && normalizar(v) === t));
  return u ? u.valor : 'UNIDAD';
}

/**
 * Códigos internos para los productos nuevos: REP-0001, REP-0002… siguiendo
 * al mayor REP- que ya exista (y sin repetir los ya usados en esta compra).
 */
export function codigosNuevos(productos, cantidad, usados = []) {
  const ocupados = new Set([...productos.map((p) => String(p.codigo || '').toUpperCase()), ...usados.map((c) => String(c).toUpperCase())]);
  let n = 0;
  for (const c of ocupados) {
    const m = /^REP-(\d+)$/.exec(c);
    if (m) n = Math.max(n, parseInt(m[1], 10));
  }
  const lista = [];
  while (lista.length < cantidad) {
    n += 1;
    const codigo = `REP-${String(n).padStart(4, '0')}`;
    if (!ocupados.has(codigo)) lista.push(codigo);
  }
  return lista;
}

const r2 = (n) => Math.round(Number(n) * 100) / 100;
const distinto = (a, b) => Math.abs(Number(a || 0) - Number(b || 0)) >= 0.005;

/**
 * Precios de venta escritos en la fila que difieren de los actuales. Una
 * celda vacía no cambia nada (no borra el Precio 2 o 3). null = sin cambios;
 * { error } si un precio no es válido.
 */
function preciosDeFila(texto, col, producto, actuales) {
  const leer = (c) => {
    const t = texto(c);
    if (!t) return undefined;
    const n = leerNumero(t);
    return n != null && n > 0 && n < 10_000_000 ? r2(n) : NaN;
  };
  const p1 = leer(col.p1);
  const p2 = leer(col.p2);
  const p3 = leer(col.p3);
  if ([p1, p2, p3].some((n) => Number.isNaN(n))) return { error: 'un precio de venta no es válido (debe ser mayor a 0)' };
  const antes = { p1: r2(producto.precio), p2: actuales?.precio_2 ? r2(actuales.precio_2) : null, p3: actuales?.precio_3 ? r2(actuales.precio_3) : null };
  const despues = {
    p1: p1 !== undefined ? p1 : antes.p1,
    p2: p2 !== undefined ? p2 : antes.p2,
    p3: p3 !== undefined ? p3 : antes.p3,
  };
  if (!distinto(antes.p1, despues.p1) && !distinto(antes.p2, despues.p2) && !distinto(antes.p3, despues.p3)) return null;
  return { antes, despues };
}

/**
 * Lee el Excel del pedido (filas de leerHoja) y arma los productos de la
 * compra. Devuelve { items, precios, sinReconocer: [{ fila, texto }],
 * malas: [{ fila, texto }], sinCosto } o { error }. precios: los productos
 * cuyos P1 / P2 / P3 del Excel difieren de los actuales
 * ([{ producto_id, codigo, nombre, costo, antes, despues }]).
 */
export function leerPedido(filas, productos, fichas, preciosActuales) {
  const inicio = filas.slice(0, 15).findIndex((f) => {
    const c = columnas(f || []);
    return c.cantidad !== undefined && (c.codigo !== undefined || c.parte !== undefined);
  });
  if (inicio < 0) {
    return { error: 'No se encontraron las columnas "Código" y "Cantidad a pedir". Usa el Excel de "Exportar pedido".' };
  }
  const col = columnas(filas[inicio]);
  const porCodigo = new Map(productos.map((p) => [String(p.codigo || '').trim().toLowerCase(), p]));
  const porParte = new Map();
  for (const p of productos) {
    const clave = claveCodigo(fichas?.get(p.id)?.numero_parte || '');
    if (clave.length < 2) continue;
    porParte.set(clave, porParte.has(clave) ? null : p); // repetido = no se usa
  }

  const items = new Map();
  const cambiosPrecio = new Map();
  const nuevos = new Map();
  const ambiguos = [];
  const sinReconocer = [];
  const malas = [];
  let sinCosto = 0;
  filas.slice(inicio + 1).forEach((fila, i) => {
    const numeroFila = inicio + i + 2;
    const texto = (c) => (c === undefined ? '' : String(fila?.[c] ?? '').trim());
    const cantidadTexto = texto(col.cantidad);
    const codigo = texto(col.codigo);
    const parte = texto(col.parte);
    const producto = codigo || parte ? porCodigo.get(codigo.toLowerCase()) || (parte ? porParte.get(claveCodigo(parte)) : null) : null;
    // Precios de venta cambiados en el Excel (aunque esa fila no se compre).
    if (producto) {
      const cambio = preciosDeFila(texto, col, producto, preciosActuales?.get(producto.id));
      if (cambio?.error) {
        malas.push({ fila: numeroFila, texto: `${producto.codigo}: ${cambio.error}` });
        return;
      }
      if (cambio) cambiosPrecio.set(producto.id, { producto_id: producto.id, codigo: producto.codigo, nombre: producto.nombre, ...cambio });
    }
    if (!cantidadTexto) return; // sin cantidad: no se pide
    if (!producto) {
      const clave = claveCodigo(parte);
      if (!codigo && clave.length >= 2 && porParte.has(clave)) {
        // El N.° de parte está en dos productos: no se adivina cuál es.
        ambiguos.push({ fila: numeroFila, texto: parte });
        return;
      }
      const nombre = texto(col.producto);
      if (codigo || !nombre) {
        // Un código que no existe suele ser un error de tipeo: no se crea nada.
        sinReconocer.push({ fila: numeroFila, texto: codigo || parte || '(sin código ni nombre)' });
        return;
      }
      // Producto que el negocio aún no tiene: se crea al registrar la compra.
      const cantidadNueva = leerNumero(cantidadTexto);
      if (cantidadNueva === 0) return;
      const unidad = unidadDesdeTexto(texto(col.unidad));
      if (cantidadNueva == null || !(cantidadNueva > 0) || cantidadNueva > 1_000_000 || (!seVendeFraccionado(unidad) && !Number.isInteger(cantidadNueva))) {
        malas.push({ fila: numeroFila, texto: `${nombre}: la cantidad "${cantidadTexto}" no es válida` });
        return;
      }
      const costoNuevo = leerNumero(texto(col.costoNuevo));
      if (costoNuevo != null && costoNuevo < 0) {
        malas.push({ fila: numeroFila, texto: `${nombre}: el costo no puede ser negativo` });
        return;
      }
      const claveNuevo = clave.length >= 2 ? `P:${clave}` : `N:${normalizar(nombre)}`;
      const previoNuevo = nuevos.get(claveNuevo);
      if (previoNuevo) {
        previoNuevo.cantidad = Math.round((previoNuevo.cantidad + cantidadNueva) * 1000) / 1000;
        if (costoNuevo > 0) previoNuevo.precio_compra = Math.round(costoNuevo * 10000) / 10000;
        return;
      }
      nuevos.set(claveNuevo, {
        fila: numeroFila,
        nombre,
        marca: texto(col.marca),
        numero_parte: parte,
        aplicacion: texto(col.aplicacion),
        unidad,
        cantidad: cantidadNueva,
        precio_compra: costoNuevo > 0 ? Math.round(costoNuevo * 10000) / 10000 : 0,
      });
      return;
    }
    const cantidad = leerNumero(cantidadTexto);
    if (cantidad === 0) return;
    const entera = !seVendeFraccionado(producto.unidad_medida);
    if (cantidad == null || !(cantidad > 0) || cantidad > 1_000_000 || (entera && !Number.isInteger(cantidad))) {
      malas.push({ fila: numeroFila, texto: `${producto.codigo}: la cantidad "${cantidadTexto}" no es válida` });
      return;
    }
    const nuevo = leerNumero(texto(col.costoNuevo));
    const actual = leerNumero(texto(col.costoActual));
    if (nuevo != null && nuevo < 0) {
      malas.push({ fila: numeroFila, texto: `${producto.codigo}: el costo no puede ser negativo` });
      return;
    }
    const costo = nuevo > 0 ? nuevo : actual > 0 ? actual : Number(producto.precio_compra) > 0 ? Number(producto.precio_compra) : 0;
    const previo = items.get(producto.id);
    if (previo) {
      // El mismo producto en dos filas: se suman (con el último costo escrito).
      previo.cantidad = Math.round((previo.cantidad + cantidad) * 1000) / 1000;
      if (nuevo > 0) previo.precio_compra = Math.round(nuevo * 10000) / 10000;
      return;
    }
    if (!(costo > 0)) sinCosto += 1;
    items.set(producto.id, {
      producto_id: producto.id,
      codigo: producto.codigo,
      nombre: producto.nombre,
      cantidad,
      precio_compra: Math.round(costo * 10000) / 10000,
    });
  });
  // Productos cuyo costo sube con esta compra: el negocio revisa sus precios de venta.
  const porId = new Map(productos.map((p) => [p.id, p]));
  const subidas = [...items.values()]
    .filter((it) => {
      const antes = Number(porId.get(it.producto_id)?.precio_compra || 0);
      return antes > 0 && it.precio_compra - antes >= 0.005;
    })
    .map((it) => {
      const p = porId.get(it.producto_id);
      const extra = preciosActuales?.get(p.id);
      const antes = { p1: r2(p.precio), p2: extra?.precio_2 ? r2(extra.precio_2) : null, p3: extra?.precio_3 ? r2(extra.precio_3) : null };
      return { producto_id: p.id, codigo: p.codigo, nombre: p.nombre, costoAntes: Number(p.precio_compra), costoNuevo: it.precio_compra, antes, despues: { ...antes } };
    });
  return {
    items: [...items.values()],
    precios: [...cambiosPrecio.values()],
    subidas,
    nuevos: [...nuevos.values()],
    ambiguos,
    sinReconocer,
    malas,
    sinCosto,
  };
}

/** Las subidas de costo en las que el negocio cambió algún precio de venta. */
export function subidasConCambio(subidas) {
  return subidas.filter((s) => distinto(s.antes.p1, s.despues.p1) || distinto(s.antes.p2, s.despues.p2) || distinto(s.antes.p3, s.despues.p3));
}

/**
 * Qué le falta a cada producto nuevo antes de registrar la compra.
 * Devuelve [] si está todo, o la lista de textos ("Categoría", "Precio 1"…).
 */
export function faltaEnNuevo(n, codigosExistentes) {
  const falta = [];
  if (!String(n.nombre || '').trim()) falta.push('Nombre');
  const codigo = String(n.codigo || '').trim();
  if (!codigo) falta.push('Código');
  else if (codigosExistentes.has(codigo.toUpperCase())) falta.push('Código (ya existe)');
  if (!n.categoria_id) falta.push('Categoría');
  if (!(Number(n.precio) > 0)) falta.push('Precio 1');
  if (!(Number(n.precio_compra) > 0)) falta.push('Costo');
  return falta;
}

/**
 * Las filas para "Importar productos" con "Actualizar" que cambian solo los
 * precios de venta: sin stock ni precio de compra (no se tocan), con el
 * nombre, la categoría y la unidad que ya tiene (tampoco cambian). El Precio
 * 2 y 3 van solo si alguno cambió, y siempre los dos (para no borrar el otro).
 */
export function filasDePrecios(cambios, productos) {
  const porId = new Map(productos.map((p) => [p.id, p]));
  return cambios
    .filter((c) => porId.has(c.producto_id))
    .map((c, i) => {
      const p = porId.get(c.producto_id);
      const extra = distinto(c.antes.p2, c.despues.p2) || distinto(c.antes.p3, c.despues.p3);
      return {
        fila: i + 1,
        codigo: String(p.codigo),
        nombre: p.nombre,
        precio: c.despues.p1,
        stock: null,
        categoria: p.categoria_nombre || null,
        unidad: p.unidad_medida || null,
        precio_compra: null,
        stock_minimo: null,
        descripcion: null,
        ...(extra ? { precios: { precio_2: c.despues.p2 || null, precio_3: c.despues.p3 || null } } : {}),
      };
    });
}
