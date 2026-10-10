// Importar productos desde el archivo de otro sistema: reconocer sus
// columnas (sin que importen mayúsculas, tildes ni espacios), leer números
// escritos de cualquier forma y armar las filas que se mandan al servidor.
import { GRUPOS_UNIDADES } from './unidades';

/** Campos del producto que se pueden traer de un archivo. */
export const CAMPOS = [
  { valor: 'codigo', label: 'Código', obligatorio: true },
  { valor: 'nombre', label: 'Nombre del producto', obligatorio: true },
  { valor: 'precio', label: 'Precio de venta', obligatorio: true },
  { valor: 'stock', label: 'Stock' },
  { valor: 'categoria', label: 'Categoría' },
  { valor: 'unidad', label: 'Unidad' },
  { valor: 'precio_compra', label: 'Precio de compra' },
  { valor: 'stock_minimo', label: 'Stock mínimo' },
  { valor: 'descripcion', label: 'Descripción' },
];

/** "Categoría:", " CATEGORIA " y "categoria" -> "categoria" */
export function normalizar(texto) {
  return String(texto ?? '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]/g, '');
}

// Nombres con los que otros sistemas llaman a cada campo (ya normalizados).
const NOMBRES = {
  codigo: ['codigo', 'cod', 'codigobarras', 'codigodebarras', 'codbarras', 'codbarra', 'barras', 'barcode', 'sku', 'ean', 'codigoproducto', 'codproducto', 'codigointerno', 'codigodelproducto', 'referencia', 'ref', 'clave',
    // Odoo: "Referencia interna" (en inglés "Internal Reference" / default_code).
    'referenciainterna', 'internalreference', 'defaultcode'],
  nombre: ['nombre', 'producto', 'articulo', 'nombreproducto', 'nombredelproducto', 'nombrearticulo', 'denominacion', 'productos'],
  precio: ['precio', 'precioventa', 'preciodeventa', 'pventa', 'pvp', 'pv', 'preciounitario', 'punitario', 'preciopublico', 'precioalpublico', 'preciofinal', 'precio1', 'venta', 'valor', 'importe', 'precios'],
  precio_compra: ['preciocompra', 'preciodecompra', 'pcompra', 'costo', 'costounitario', 'preciocosto', 'preciodecosto', 'coste', 'compra', 'pc'],
  stock: ['stock', 'cantidad', 'cant', 'existencia', 'existencias', 'saldo', 'inventario', 'stockactual', 'disponible', 'qty', 'unidadesdisponibles'],
  categoria: ['categoria', 'categorias', 'familia', 'linea', 'rubro', 'grupo', 'clase', 'seccion', 'departamento', 'tipo'],
  unidad: ['unidad', 'unidadmedida', 'unidaddemedida', 'um', 'umedida', 'und', 'medida', 'unidadventa'],
  stock_minimo: ['stockminimo', 'minimo', 'stockmin', 'stockalerta', 'alerta'],
};
// Sirven de nombre si no hay otra columna de nombre; si la hay, son la descripción.
const NOMBRE_O_DESCRIPCION = ['descripcion', 'detalle', 'descripciondelproducto', 'descripcionproducto', 'glosa', 'concepto'];
const SOLO_DESCRIPCION = ['observacion', 'observaciones', 'nota', 'notas', 'comentario'];

// Si el nombre no es exacto ("precioventasoles", "stock tienda 1"), por lo que contiene.
function campoParecido(clave) {
  if (!clave) return null;
  if (clave.includes('minim')) return 'stock_minimo';
  if (clave.includes('compra') || clave.includes('costo')) return 'precio_compra';
  if (clave.includes('precio') || clave.includes('pventa')) return 'precio';
  if (clave.includes('sunat')) return null; // el código de producto de SUNAT no es el código de venta
  if (clave.includes('barra') || clave.startsWith('codigo') || clave.startsWith('cod')) return 'codigo';
  if (clave.includes('stock') || clave.includes('cantidad') || clave.includes('existencia')) return 'stock';
  if (clave.includes('categoria') || clave.includes('familia')) return 'categoria';
  if (clave.includes('unidad')) return 'unidad';
  if (clave.includes('nombre') || clave.includes('producto') || clave.includes('articulo')) return 'nombre';
  return null;
}

/**
 * Qué campo es cada columna, según su título. Devuelve una lista del mismo
 * largo: el campo, o '' si no se reconoció. Ningún campo se repite.
 */
export function sugerirCampos(titulos) {
  const claves = titulos.map(normalizar);
  const mapa = claves.map(() => '');
  const usados = new Set();
  const asignar = (i, campo) => {
    if (!campo || mapa[i] || usados.has(campo)) return;
    mapa[i] = campo;
    usados.add(campo);
  };
  // Primero los nombres exactos, en orden de preferencia ("Precio" gana a
  // "Importe" aunque "Importe" esté antes en el archivo).
  for (const [campo, nombres] of Object.entries(NOMBRES)) {
    for (const nombre of nombres) {
      const i = claves.findIndex((c, indice) => c === nombre && !mapa[indice]);
      if (i >= 0) {
        asignar(i, campo);
        break;
      }
    }
  }
  claves.forEach((c, i) => asignar(i, campoParecido(c)));
  // "Descripción": es el nombre si nadie más lo es; si no, la descripción.
  claves.forEach((c, i) => {
    if (NOMBRE_O_DESCRIPCION.includes(c)) asignar(i, usados.has('nombre') ? 'descripcion' : 'nombre');
  });
  claves.forEach((c, i) => {
    if (SOLO_DESCRIPCION.includes(c) || (c.includes('descripcion') && !mapa[i])) asignar(i, usados.has('nombre') ? 'descripcion' : 'nombre');
  });
  return mapa;
}

/**
 * En qué fila están los títulos (algunos archivos traen el nombre del
 * negocio o una fecha arriba). Devuelve su índice, o -1 si el archivo no
 * parece tener títulos (empieza directo con productos).
 */
export function filaDeTitulos(filas) {
  let mejor = -1;
  let puntos = 0;
  filas.slice(0, 15).forEach((fila, i) => {
    const reconocidas = sugerirCampos(fila || []).filter(Boolean).length;
    if (reconocidas > puntos) {
      puntos = reconocidas;
      mejor = i;
    }
  });
  return puntos >= 1 ? mejor : -1;
}

/** "S/ 1,250.50", "35,90", "1.250,50" o 35.9 -> número (null si no hay). */
export function leerNumero(valor) {
  if (typeof valor === 'number') return Number.isFinite(valor) ? valor : null;
  let t = String(valor ?? '').replace(/s\/\.?|pen|soles|\$|\s/gi, '');
  if (!t) return null;
  const negativo = /^-|^\(.*\)$/.test(t);
  t = t.replace(/[^0-9.,]/g, '');
  if (!t) return null;
  const coma = t.lastIndexOf(',');
  const punto = t.lastIndexOf('.');
  if (coma >= 0 && punto >= 0) {
    // El último es el de los decimales.
    t = coma > punto ? t.replace(/\./g, '').replace(',', '.') : t.replace(/,/g, '');
  } else if (coma >= 0) {
    // Solo comas: miles ("1,250") o decimales ("35,90").
    t = /^\d{1,3}(,\d{3})+$/.test(t) ? t.replace(/,/g, '') : t.replace(',', '.');
  }
  const n = Number(t);
  if (!Number.isFinite(n)) return null;
  return negativo ? -n : n;
}

// Cómo llaman a cada unidad los archivos: su código, su nombre, su abreviatura...
const UNIDADES = new Map();
for (const grupo of GRUPOS_UNIDADES) {
  for (const u of grupo.unidades) {
    for (const nombre of [u.valor, u.label, u.abrev]) UNIDADES.set(normalizar(nombre), u.valor);
    UNIDADES.set(`${normalizar(u.label)}s`, u.valor);
    UNIDADES.set(`${normalizar(u.label)}es`, u.valor);
  }
}
Object.entries({
  UNIDAD: ['u', 'un', 'uni', 'unid', 'unids', 'und', 'unds', 'unidades', 'nu', 'niu', 'pz', 'ud', 'uds'],
  KG: ['kilo', 'kilos', 'kgs', 'kgr', 'kilogramo', 'kilogramos'],
  GRAMO: ['gr', 'grs', 'gramos'],
  LITRO: ['l', 'lt', 'lts', 'litros'],
  ML: ['mililitro', 'mililitros', 'cc'],
  METRO: ['mt', 'mts', 'metros'],
  DOCENA: ['dz', 'docenas'],
  CAJA: ['cj', 'cja', 'cajas'],
  PAQUETE: ['pqt', 'pqte', 'pack', 'paquetes'],
  BOLSA: ['bol', 'bls', 'bolsas'],
  GALON: ['gl', 'gln', 'galones'],
  PAR: ['pares'],
  BOTELLA: ['botellas'],
}).forEach(([codigo, nombres]) => nombres.forEach((n) => UNIDADES.set(n, codigo)));

/** "und", "UND." o "Unidades" -> "UNIDAD" (null si no se reconoce). */
export function unidadDe(texto) {
  return UNIDADES.get(normalizar(texto)) || null;
}

const titulosDeColumna = (n) => {
  let letras = '';
  for (let i = n + 1; i > 0; i = Math.floor((i - 1) / 26)) letras = String.fromCharCode(65 + ((i - 1) % 26)) + letras;
  return `Columna ${letras}`;
};

/**
 * Columnas del archivo: título, ejemplo (primer valor que no esté vacío) e
 * índice. Sin fila de títulos se llaman "Columna A", "Columna B"...
 */
export function columnasDe(filas, filaTitulos) {
  const datos = filas.slice(filaTitulos + 1);
  const ancho = filas.reduce((m, f) => Math.max(m, f.length), 0);
  const columnas = [];
  for (let i = 0; i < ancho; i++) {
    const titulo = filaTitulos >= 0 ? String(filas[filaTitulos][i] ?? '').trim() : '';
    const ejemplo = datos.find((f) => String(f[i] ?? '').trim() !== '')?.[i] ?? '';
    if (!titulo && ejemplo === '') continue; // columna vacía
    columnas.push({ indice: i, titulo: titulo || titulosDeColumna(i), ejemplo: String(ejemplo) });
  }
  return columnas;
}

/**
 * Las filas que se mandan al servidor. mapa: { indiceDeColumna: campo }.
 * Devuelve también cuántas unidades no se reconocieron (entran con la unidad
 * por defecto).
 */
export function armarFilas(filas, filaTitulos, mapa) {
  const columna = {};
  for (const [indice, campo] of Object.entries(mapa)) if (campo) columna[campo] = Number(indice);
  const texto = (fila, campo) => (columna[campo] === undefined ? '' : String(fila[columna[campo]] ?? '').trim());
  const numero = (fila, campo) => (columna[campo] === undefined ? null : leerNumero(fila[columna[campo]]));

  const salida = [];
  let unidadesRaras = 0;
  filas.forEach((fila, i) => {
    if (i <= filaTitulos || !fila || fila.every((c) => String(c ?? '').trim() === '')) return;
    const unidadTexto = texto(fila, 'unidad');
    const unidad = unidadTexto ? unidadDe(unidadTexto) : null;
    if (unidadTexto && !unidad) unidadesRaras += 1;
    salida.push({
      fila: i + 1,
      codigo: texto(fila, 'codigo'),
      nombre: texto(fila, 'nombre'),
      precio: numero(fila, 'precio'),
      stock: numero(fila, 'stock'),
      categoria: texto(fila, 'categoria') || null,
      unidad,
      precio_compra: numero(fila, 'precio_compra'),
      stock_minimo: numero(fila, 'stock_minimo'),
      descripcion: texto(fila, 'descripcion') || null,
    });
  });
  return { filas: salida, unidadesRaras };
}
