// Compras por Excel con el archivo del proveedor (cualquier formato):
// 1) qué columna es cada dato (se adivina por el título y se puede cambiar),
// 2) qué producto nuestro es cada fila (lo aprendido de ese proveedor, el
//    código, el N.° de parte o el nombre exacto; si no, sugerencias),
// 3) la hoja "como la del sistema" que lee leerPedido (utils/pedidoProveedor.js).
import { leerNumero, normalizar } from './importacion';
import { claveCodigo } from './repuestos';

export const CAMPOS_COMPRA = [
  { valor: 'codigo', label: 'Código del producto', ayuda: 'opcional' },
  { valor: 'producto', label: 'Producto / descripción', ayuda: 'opcional si hay código' },
  { valor: 'cantidad', label: 'Cantidad', ayuda: 'obligatoria' },
  { valor: 'costo', label: 'Costo unitario', ayuda: 'o el importe total' },
  { valor: 'total', label: 'Importe total de la fila', ayuda: 'si no hay costo unitario' },
  { valor: 'unidad', label: 'Unidad', ayuda: 'opcional' },
];

// Títulos conocidos (normalizados), en orden de preferencia.
const NOMBRES = {
  codigo: ['codigo', 'cod', 'codigoproducto', 'codproducto', 'codigointerno', 'codigodelproducto', 'codigoarticulo', 'codart', 'sku',
    'referencia', 'ref', 'codbarras', 'codigobarras', 'codigodebarras', 'barras', 'ean', 'barcode', 'codigoitem', 'clave'],
  producto: ['producto', 'descripcion', 'nombre', 'articulo', 'detalle', 'denominacion', 'descripciondelproducto', 'descripcionproducto',
    'nombreproducto', 'nombredelproducto', 'concepto', 'productos', 'mercaderia', 'medicamento'],
  cantidad: ['cantidad', 'cant', 'cantidadapedir', 'cantidadpedida', 'cantidades', 'qty', 'unidades', 'pedido', 'apedir', 'canti', 'ctd', 'cdad'],
  costo: ['preciounitario', 'punitario', 'punit', 'pu', 'preciounit', 'precunit', 'valorunitario', 'vunitario', 'vu', 'costounitario', 'costo',
    'preciocompra', 'preciodecompra', 'preciocosto', 'costounit', 'precio', 'pventa', 'valor'],
  total: ['importe', 'total', 'subtotal', 'importetotal', 'valortotal', 'preciototal', 'montototal', 'monto', 'valorventa', 'parcial', 'totalitem'],
  unidad: ['unidad', 'um', 'unidaddemedida', 'unidadmedida', 'und', 'medida', 'umedida', 'unid'],
};

// Si el título no es exacto, por lo que contiene.
function parecido(c) {
  if (!c) return '';
  if (c.includes('cant') || c.includes('qty')) return 'cantidad';
  if ((c.includes('unit') || c.startsWith('pu')) && (c.includes('prec') || c.includes('valor') || c.includes('cost') || c.startsWith('pu'))) return 'costo';
  if (c.includes('costo')) return 'costo';
  if (c.includes('import') || c.includes('total') || c.includes('monto')) return 'total';
  if (c.includes('descrip') || c.includes('producto') || c.includes('articulo')) return 'producto';
  if (c.startsWith('cod') || c.includes('codigo')) return 'codigo';
  if (c.startsWith('unidad') || c === 'u') return 'unidad';
  if (c.includes('precio')) return 'costo';
  return '';
}

/** { campo: índice de columna } a partir de una fila de títulos. */
export function columnasPorTitulo(titulos) {
  const claves = (titulos || []).map(normalizar);
  const mapa = {};
  const usada = new Set();
  const poner = (campo, i) => {
    if (mapa[campo] === undefined && !usada.has(i)) {
      mapa[campo] = i;
      usada.add(i);
    }
  };
  for (const [campo, nombres] of Object.entries(NOMBRES)) {
    for (const nombre of nombres) {
      const i = claves.findIndex((c, idx) => c === nombre && !usada.has(idx));
      if (i >= 0) {
        poner(campo, i);
        break;
      }
    }
  }
  claves.forEach((c, i) => {
    const campo = parecido(c);
    if (campo) poner(campo, i);
  });
  return mapa;
}

/** ¿Alcanza para leer la compra? Cantidad y (código o producto). */
export function columnasCompletas(mapa) {
  return mapa?.cantidad !== undefined && (mapa.codigo !== undefined || mapa.producto !== undefined);
}

/**
 * Fila de títulos (entre las 20 primeras) y sus columnas: la que reconoce
 * más datos. Devuelve { filaTitulos, columnas } (filaTitulos -1 si ninguna).
 */
export function detectarColumnas(filas) {
  let mejor = { filaTitulos: -1, columnas: {}, puntos: 0 };
  (filas || []).slice(0, 20).forEach((fila, i) => {
    const columnas = columnasPorTitulo(fila);
    const puntos = Object.keys(columnas).length + (columnasCompletas(columnas) ? 3 : 0);
    if (puntos > mejor.puntos) mejor = { filaTitulos: i, columnas, puntos };
  });
  return { filaTitulos: mejor.filaTitulos, columnas: mejor.columnas };
}

/** Letra de una columna de Excel (0 -> A, 27 -> AB). */
export function letraColumna(i) {
  let n = i + 1;
  let s = '';
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

/** Las columnas del archivo para elegir: [{ indice, texto: "B · Descripción (ej: Paracetamol…)" }]. */
export function opcionesDeColumnas(filas, filaTitulos) {
  const ancho = (filas || []).reduce((m, f) => Math.max(m, (f || []).length), 0);
  const datos = filas.slice(filaTitulos + 1);
  const lista = [];
  for (let i = 0; i < ancho; i += 1) {
    const titulo = filaTitulos >= 0 ? String(filas[filaTitulos]?.[i] ?? '').trim() : '';
    const ejemplo = String(datos.find((f) => String(f?.[i] ?? '').trim() !== '')?.[i] ?? '').trim();
    if (!titulo && !ejemplo) continue;
    const corto = ejemplo.length > 28 ? `${ejemplo.slice(0, 28)}…` : ejemplo;
    lista.push({ indice: i, texto: `${letraColumna(i)} · ${titulo || '(sin título)'}${corto ? ` (ej: ${corto})` : ''}` });
  }
  return lista;
}

const r4 = (n) => Math.round(Number(n) * 10000) / 10000;

/**
 * Las filas de la compra: [{ fila, codigo, nombre, cantidad, costo, unidad }].
 * Se saltan las vacías y las que no traen cantidad (títulos, "TOTAL", "IGV"…).
 * Si no hay costo unitario pero sí importe, costo = importe / cantidad.
 */
export function filasDeCompra(filas, filaTitulos, columnas) {
  const texto = (f, campo) => (columnas[campo] === undefined ? '' : String(f?.[columnas[campo]] ?? '').trim());
  const salida = [];
  filas.slice(filaTitulos + 1).forEach((f, i) => {
    const codigo = texto(f, 'codigo');
    const nombre = texto(f, 'producto');
    if (!codigo && !nombre) return;
    const cantidad = leerNumero(texto(f, 'cantidad'));
    if (!(cantidad > 0)) return;
    let costo = leerNumero(texto(f, 'costo'));
    if (!(costo > 0)) {
      const total = leerNumero(texto(f, 'total'));
      costo = total > 0 ? total / cantidad : null;
    }
    salida.push({
      fila: filaTitulos + i + 2,
      codigo,
      nombre,
      cantidad,
      costo: costo > 0 ? r4(costo) : null,
      unidad: texto(f, 'unidad'),
    });
  });
  return salida;
}

/** Claves con que se recuerda una fila de ese proveedor. */
export const claveDeCodigo = (codigo) => (normalizar(codigo) ? `C:${normalizar(codigo)}` : '');
export const claveDeNombre = (nombre) => (normalizar(nombre) ? `N:${normalizar(nombre).slice(0, 190)}` : '');

// Palabras de un nombre para comparar ("Paracetamol 500 mg x100" -> paracetamol, 500, mg, x100).
function palabras(texto) {
  return String(texto ?? '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/(\d)\s*(mg|ml|g|gr|kg|l|lt|cc|mcg|ui)\b/g, '$1$2')
    .replace(/\bx\s*(\d)/g, ' $1') // "x100", "x 100" -> 100
    .split(/[^a-z0-9]+/)
    .filter((p) => p.length >= 2 || /\d/.test(p));
}

// Palabras que no distinguen un producto de otro (presentación, unidades…).
const GENERICAS = new Set([
  'tab', 'tabs', 'tableta', 'tabletas', 'comp', 'comprimido', 'comprimidos', 'cap', 'caps', 'capsula', 'capsulas', 'caja', 'cajas', 'cj', 'cja',
  'und', 'unid', 'unidad', 'unidades', 'frasco', 'fco', 'jarabe', 'jbe', 'sobre', 'sobres', 'blister', 'ampolla', 'ampollas', 'amp', 'vial',
  'tubo', 'crema', 'gotas', 'susp', 'suspension', 'solucion', 'sol', 'inyectable', 'iny', 'oral', 'mg', 'ml', 'gr', 'kg', 'lt', 'mcg', 'ui',
  'por', 'con', 'sin', 'de', 'del', 'la', 'el', 'en', 'para', 'pack', 'paquete', 'pqt', 'bolsa', 'botella', 'lata', 'docena', 'par',
]);
const distintiva = (p) => !/\d/.test(p) && p.length >= 3 && !GENERICAS.has(p);

/**
 * Qué tanto se parecen dos nombres (0 a 1). Los números (500mg, x100) pesan
 * más, pero sin una palabra propia en común (el principio activo, la marca…)
 * no se parecen: "Cetirizina 10mg x 100" no es "Loratadina 10mg x 100".
 */
export function parecidoNombres(a, b) {
  const pa = palabras(a);
  const pb = new Set(palabras(b));
  if (pa.length === 0 || pb.size === 0) return 0;
  if (!pa.some((p) => distintiva(p) && pb.has(p))) return 0;
  let comun = 0;
  let total = 0;
  const vistas = new Set();
  for (const p of pa) {
    if (vistas.has(p)) continue;
    vistas.add(p);
    const peso = /\d/.test(p) ? 2 : 1;
    total += peso;
    if (pb.has(p)) comun += peso;
  }
  for (const p of pb) if (!vistas.has(p)) total += /\d/.test(p) ? 2 : 1;
  return total ? comun / total : 0;
}

/** Los 3 productos con nombre más parecido (score >= 0.25). */
export function sugerencias(nombre, productos, cuantas = 3) {
  if (!nombre) return [];
  return productos
    .map((p) => ({ producto: p, score: parecidoNombres(nombre, p.nombre) }))
    .filter((x) => x.score >= 0.25)
    .sort((a, b) => b.score - a.score)
    .slice(0, cuantas);
}

/**
 * Qué producto es cada fila. memoria: Map clave -> producto_id (lo aprendido
 * de ese proveedor); fichas: Map producto_id -> ficha del repuesto (o vacío).
 * Devuelve [{ ...fila, producto, como }] con como = 'MEMORIA' | 'CODIGO' |
 * 'PARTE' | 'NOMBRE' | null (sin reconocer).
 */
export function reconocerFilas(filasCompra, productos, memoria, fichas) {
  const porId = new Map(productos.map((p) => [p.id, p]));
  const porCodigo = new Map(productos.map((p) => [String(p.codigo || '').trim().toLowerCase(), p]));
  const porNombre = new Map();
  for (const p of productos) {
    const k = normalizar(p.nombre);
    if (k) porNombre.set(k, porNombre.has(k) ? null : p); // repetido = no se adivina
  }
  const porParte = new Map();
  for (const p of productos) {
    const k = claveCodigo(fichas?.get(p.id)?.numero_parte || '');
    if (k.length >= 2) porParte.set(k, porParte.has(k) ? null : p);
  }
  return filasCompra.map((f) => {
    const recordado = memoria?.get(claveDeCodigo(f.codigo)) ?? memoria?.get(claveDeNombre(f.nombre));
    if (recordado && porId.has(recordado)) return { ...f, producto: porId.get(recordado), como: 'MEMORIA' };
    const porCod = f.codigo ? porCodigo.get(f.codigo.toLowerCase()) : null;
    if (porCod) return { ...f, producto: porCod, como: 'CODIGO' };
    const porPar = f.codigo ? porParte.get(claveCodigo(f.codigo)) : null;
    if (porPar) return { ...f, producto: porPar, como: 'PARTE' };
    const porNom = f.nombre ? porNombre.get(normalizar(f.nombre)) : null;
    if (porNom) return { ...f, producto: porNom, como: 'NOMBRE' };
    return { ...f, producto: null, como: null };
  });
}

/**
 * La hoja "como la del sistema" para leerPedido: Código (nuestro), Producto,
 * Unidad, Cantidad a pedir, Precio unitario. decisiones: Map fila ->
 * { producto } | { nuevo: true } | { omitir: true } para las que no se reconocieron.
 */
export function hojaParaPedido(reconocidas, decisiones) {
  const filas = [['Código', 'Producto', 'Unidad', 'Cantidad a pedir', 'Precio unitario']];
  for (const f of reconocidas) {
    const d = f.producto ? { producto: f.producto } : decisiones.get(f.fila);
    if (!d || d.omitir) continue;
    const costo = f.costo > 0 ? String(f.costo) : '';
    if (d.producto) filas.push([String(d.producto.codigo || ''), d.producto.nombre, f.unidad, String(f.cantidad), costo]);
    else if (d.nuevo) filas.push(['', f.nombre || f.codigo, f.unidad, String(f.cantidad), costo]);
  }
  return filas;
}

/**
 * Lo que se le enseña al sistema de este proveedor: las filas que se
 * reconocieron por nombre o que el dueño eligió a mano (por su código si lo
 * tiene; si no, por el nombre). Las de código o memoria ya se reconocen solas.
 */
export function enlacesAprendidos(reconocidas, decisiones) {
  const enlaces = new Map();
  for (const f of reconocidas) {
    const elegido = f.producto ? (f.como === 'NOMBRE' || f.como === 'PARTE' ? f.producto : null) : decisiones.get(f.fila)?.producto;
    if (!elegido) continue;
    const clave = claveDeCodigo(f.codigo) || claveDeNombre(f.nombre);
    if (clave) enlaces.set(clave, { clave, producto_id: elegido.id });
  }
  return [...enlaces.values()];
}
