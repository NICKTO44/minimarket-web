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
  // Perecibles: con vencimiento, el Stock de la fila es la cantidad de ese
  // lote y el mismo código en varias filas son varios lotes.
  { valor: 'lote', label: 'N.° de lote' },
  { valor: 'vencimiento', label: 'Fecha de vencimiento' },
];

/** Datos del medicamento (solo con el módulo "Ficha del medicamento"). */
export const CAMPOS_FICHA = [
  { valor: 'principio_activo', label: 'Principio activo (DCI)', ficha: true },
  { valor: 'concentracion', label: 'Concentración', ficha: true },
  { valor: 'forma_farmaceutica', label: 'Forma farmacéutica', ficha: true },
  { valor: 'laboratorio', label: 'Laboratorio', ficha: true },
  { valor: 'registro_sanitario', label: 'Registro sanitario', ficha: true },
  { valor: 'condicion_venta', label: 'Condición de venta (receta)', ficha: true },
  { valor: 'es_generico', label: 'Genérico (Sí/No)', ficha: true },
];

/** Lubricentro: ficha del repuesto (módulo "Ficha del repuesto"). */
export const CAMPOS_REPUESTO = [
  { valor: 'marca_repuesto', label: 'Marca del repuesto', repuesto: true },
  { valor: 'numero_parte', label: 'N.° de parte / código del fabricante', repuesto: true },
  { valor: 'codigos_alternos', label: 'Códigos alternos (equivalencias)', repuesto: true },
  { valor: 'ubicacion', label: 'Ubicación en el almacén', repuesto: true },
  { valor: 'garantia', label: 'Garantía (días)', repuesto: true },
];

/** Repuestos: códigos de los productos equivalentes (módulo "Equivalentes"). Al importar solo se agregan. */
export const CAMPOS_EQUIVALENTES = [{ valor: 'equivalentes', label: 'Equivalentes (códigos)', equivalentes: true }];

/** Lubricentro: vehículos compatibles (módulo "Compatibilidad con vehículos"). */
export const CAMPOS_COMPATIBILIDAD = [{ valor: 'compatible', label: 'Compatible con (vehículos)', compatibilidad: true }];

/** Módulo "3 precios de venta": Precio 2 y 3 (el Precio 1 es el precio de venta). */
export const CAMPOS_TRES_PRECIOS = [
  { valor: 'precio_2', label: 'Precio 2', tresPrecios: true },
  { valor: 'precio_3', label: 'Precio 3', tresPrecios: true },
];

/** Librería: ficha del libro (módulo "Ficha del libro"). El ISBN va como Código. */
export const CAMPOS_LIBRO = [
  { valor: 'libro_autor', label: 'Autor', libro: true },
  { valor: 'libro_editorial', label: 'Editorial', libro: true },
  { valor: 'libro_serie', label: 'Serie o colección', libro: true },
  { valor: 'libro_anio', label: 'Año', libro: true },
  { valor: 'libro_edicion', label: 'Edición', libro: true },
];

/** Los campos que se pueden elegir en este negocio. */
export function camposImportacion({ ficha = false, repuesto = false, compatibilidad = false, tresPrecios = false, libro = false, equivalentes = false } = {}) {
  return [
    ...CAMPOS,
    ...(equivalentes ? CAMPOS_EQUIVALENTES : []),
    ...(ficha ? CAMPOS_FICHA : []),
    ...(repuesto ? CAMPOS_REPUESTO : []),
    ...(compatibilidad ? CAMPOS_COMPATIBILIDAD : []),
    ...(tresPrecios ? CAMPOS_TRES_PRECIOS : []),
    ...(libro ? CAMPOS_LIBRO : []),
  ];
}

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
  lote: ['lote', 'numerolote', 'numerodelote', 'nrolote', 'nrodelote', 'nlote', 'numlote', 'lotes', 'lot', 'batch'],
  vencimiento: ['vencimiento', 'fechavencimiento', 'fechadevencimiento', 'fvencimiento', 'fvenc', 'fecvenc', 'fechavenc', 'vence', 'venc',
    'vto', 'vcto', 'fvto', 'fechavcto', 'fechadevcto', 'caducidad', 'fechacaducidad', 'fechadecaducidad', 'expiracion', 'fechaexpiracion', 'expira', 'fv'],
  // Lubricentro (solo si el negocio tiene la ficha del repuesto o la
  // compatibilidad). Van antes que farmacia: "Marca" es la del repuesto.
  marca_repuesto: ['marca', 'marcarepuesto', 'marcadelrepuesto', 'brand', 'fabricante'],
  numero_parte: ['numerodeparte', 'numeroparte', 'ndeparte', 'nodeparte', 'nparte', 'nrodeparte', 'nroparte', 'partnumber', 'pn', 'codigofabricante',
    'codigooem', 'oem', 'codigooriginal', 'codfabricante', 'npieza', 'numerodepieza'],
  // Antes que los códigos alternos: con el módulo "Equivalentes", la columna es suya.
  equivalentes: ['equivalentes', 'equivalente', 'equivalencias', 'productosequivalentes', 'codigosequivalentes'],
  codigos_alternos: ['codigosalternos', 'codigoalterno', 'alternos', 'alterno', 'equivalencias', 'equivalentes', 'equivalencia',
    'referenciascruzadas', 'crossreference', 'reemplazos', 'reemplaza', 'otroscodigos'],
  ubicacion: ['ubicacion', 'ubic', 'estante', 'anaquel', 'percha', 'casillero', 'posicion', 'ubicacionalmacen'],
  garantia: ['garantia', 'garantiadias', 'diasgarantia', 'garantiaendias', 'diasdegarantia'],
  compatible: ['compatiblecon', 'compatible', 'compatibilidad', 'aplicacion', 'aplicaciones', 'vehiculos', 'vehiculo',
    'modelos', 'paravehiculo', 'usoen', 'vehiculoscompatibles'],
  // Módulo "3 precios de venta" (solo si el negocio lo tiene).
  precio_2: ['precio2', 'precio02', 'preciodos', 'p2', 'precioventa2', 'pventa2'],
  precio_3: ['precio3', 'precio03', 'preciotres', 'p3', 'precioventa3', 'pventa3'],
  // Librería (solo si el negocio tiene la ficha del libro).
  libro_autor: ['autor', 'autores', 'autora', 'escritor', 'autordellibro'],
  libro_editorial: ['editorial', 'sello', 'selloeditorial', 'casaeditorial'],
  libro_serie: ['serie', 'saga', 'coleccion'],
  libro_anio: ['ano', 'anio', 'anodepublicacion', 'anopublicacion', 'anodeedicion', 'publicacion'],
  libro_edicion: ['edicion'],
  // Farmacia (solo se usan si el negocio tiene la ficha del medicamento).
  principio_activo: ['principioactivo', 'principio', 'dci', 'nombregenerico', 'denominacioncomuninternacional', 'sustancia', 'compuesto'],
  concentracion: ['concentracion', 'conc', 'dosis', 'potencia', 'mg'],
  forma_farmaceutica: ['formafarmaceutica', 'forma', 'formafarm', 'presentacionfarmaceutica', 'ff'],
  laboratorio: ['laboratorio', 'lab', 'fabricante', 'marca', 'titular'],
  registro_sanitario: ['registrosanitario', 'registro', 'rs', 'regsanitario', 'nregistro', 'registrodigemid'],
  condicion_venta: ['condicionventa', 'condiciondeventa', 'condicion', 'receta', 'conreceta', 'tiporeceta'],
  es_generico: ['generico', 'esgenerico', 'genericoono', 'genericosino', 'genericomarca', 'marcaogenerico', 'tipogenerico'],
};
// Sirven de nombre si no hay otra columna de nombre; si la hay, son la descripción.
const NOMBRE_O_DESCRIPCION = ['descripcion', 'detalle', 'descripciondelproducto', 'descripcionproducto', 'glosa', 'concepto'];
const SOLO_DESCRIPCION = ['observacion', 'observaciones', 'nota', 'notas', 'comentario'];

// Si el nombre no es exacto ("precioventasoles", "stock tienda 1"), por lo que contiene.
function campoParecido(clave, permitidos = new Set()) {
  if (!clave) return null;
  // Lubricentro: solo si el negocio tiene esos campos (así, en otro rubro,
  // una columna "Código alterno" sigue leyéndose como antes).
  const lubricentro = (campo) => (permitidos.has(campo) ? campo : null);
  if (clave.includes('venc') || clave.includes('vcto') || clave.includes('caduc') || clave.includes('expir')) return 'vencimiento';
  // Librería: el ISBN es el código y el título es el nombre.
  if (permitidos.has('libro_autor')) {
    if (clave.includes('isbn')) return 'codigo';
    if (clave === 'titulo' || clave === 'titulodellibro' || clave === 'obra') return 'nombre';
    if (clave.includes('autor')) return 'libro_autor';
    if (clave.includes('editorial')) return 'libro_editorial';
  }
  if (clave.includes('lote') && !clave.includes('cant') && !clave.includes('stock')) return 'lote';
  if (clave.includes('equivalen') && permitidos.has('equivalentes')) return 'equivalentes';
  const alterno = clave.includes('alterno') || clave.includes('equivalen') || clave.includes('reemplaz');
  if (alterno && lubricentro('codigos_alternos')) return 'codigos_alternos';
  if ((clave.includes('compatib') || clave.includes('aplicacion') || clave.includes('vehicul')) && lubricentro('compatible')) return 'compatible';
  if ((clave.includes('departe') || clave.includes('partnumber') || clave.includes('oem')) && lubricentro('numero_parte')) return 'numero_parte';
  if (clave.includes('garantia') && lubricentro('garantia')) return 'garantia';
  if ((clave.includes('ubicacion') || clave.includes('estante')) && lubricentro('ubicacion')) return 'ubicacion';
  if (clave.includes('principio') || clave.includes('dci')) return 'principio_activo';
  if (clave.includes('concentracion')) return 'concentracion';
  if (clave.includes('farmaceutica')) return 'forma_farmaceutica';
  if (clave.includes('laboratorio')) return 'laboratorio';
  if (clave.includes('registrosanitario') || clave.includes('digemid')) return 'registro_sanitario';
  if (clave.includes('receta') || clave.includes('condicion')) return 'condicion_venta';
  if (clave.includes('generic') && !clave.includes('nombre')) return 'es_generico';
  if (clave.includes('minim')) return 'stock_minimo';
  if (clave.includes('compra') || clave.includes('costo')) return 'precio_compra';
  if (/precio0?2|preciodos/.test(clave) && lubricentro('precio_2')) return 'precio_2';
  if (/precio0?3|preciotres/.test(clave) && lubricentro('precio_3')) return 'precio_3';
  if (clave.includes('precio') || clave.includes('pventa')) return 'precio';
  if (clave.includes('sunat')) return null; // el código de producto de SUNAT no es el código de venta
  if (clave.includes('barra') || clave.startsWith('codigo') || clave.startsWith('cod')) return 'codigo';
  if (clave.includes('stock') || clave.includes('cantidad') || clave.includes('existencia')) return 'stock';
  if (clave.includes('categoria') || clave.includes('familia')) return 'categoria';
  if (clave.includes('unidad')) return 'unidad';
  if (clave.includes('nombre') || clave.includes('producto') || clave.includes('articulo')) return 'nombre';
  return null;
}

// Lubricentro: ¿la mayoría de los valores de la columna cumple la prueba?
function mayoria(valores, prueba) {
  const llenos = (valores || []).map((v) => String(v ?? '').trim()).filter(Boolean);
  return llenos.length > 0 && llenos.filter(prueba).length >= Math.ceil(llenos.length * 0.8);
}
// "716CV01B200", "9968407": sin espacios y con algún número.
const pareceCodigo = (t) => t.length >= 4 && t.length <= 25 && !/\s/.test(t) && /\d/.test(t);
// "Toyota Hilux 2005-2015" sí se puede leer como vehículos; "AVEO 05/UP SAIL 11/15" no.
const traeAnios = (t) => /(^|\D)(19|20)\d{2}(\D|$)/.test(t);

/**
 * Lubricentro: columnas de las listas de repuestos que el título solo no
 * resuelve. Usa los valores de la columna (`muestras`) cuando hacen falta.
 *  - "Aplicación" en texto libre ("CHEVROLET AVEO 05/UP SAIL 11/15") va a la
 *    descripción, donde se guarda tal cual y el buscador la encuentra; solo
 *    va a "Compatible con" si trae años de 4 cifras que se puedan leer.
 *  - "Precio público" es el Precio 1, "Precio tienda" el 2 y "Precio x mayor"
 *    el 3 (con el módulo "3 precios de venta").
 *  - "Proveedor" con códigos (716CV01B200) es el n.° de parte.
 */
function camposDeRepuestos(claves, muestras, asignar, permitidos) {
  claves.forEach((c, i) => {
    const valores = muestras[i];
    if (c.includes('aplicacion') && permitidos.has('compatible') && valores?.length) {
      asignar(i, mayoria(valores, traeAnios) ? 'compatible' : 'descripcion');
    }
    if (c.includes('proveedor') && permitidos.has('numero_parte') && mayoria(valores, pareceCodigo)) asignar(i, 'numero_parte');
    if (!permitidos.has('precio_2') || !c.includes('precio') || c.includes('compra') || c.includes('costo')) return;
    if (c.includes('publico')) asignar(i, 'precio');
    else if (c.includes('tienda')) asignar(i, 'precio_2');
    else if (c.includes('mayor')) asignar(i, 'precio_3');
  });
}

/**
 * Qué campo es cada columna, según su título (y, en Lubricentro, sus
 * valores: `muestras[i]` son algunos valores de la columna i). Devuelve una
 * lista del mismo largo: el campo, o '' si no se reconoció. Ningún campo se repite.
 */
export function sugerirCampos(titulos, campos = CAMPOS, muestras = []) {
  const permitidos = new Set(campos.map((c) => c.valor));
  const claves = titulos.map(normalizar);
  const mapa = claves.map(() => '');
  const usados = new Set();
  const asignar = (i, campo) => {
    if (!campo || !permitidos.has(campo) || mapa[i] || usados.has(campo)) return;
    mapa[i] = campo;
    usados.add(campo);
  };
  camposDeRepuestos(claves, muestras, asignar, permitidos);
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
  claves.forEach((c, i) => asignar(i, campoParecido(c, permitidos)));
  // "Descripción": es el nombre si nadie más lo es; si no, la descripción.
  claves.forEach((c, i) => {
    if (NOMBRE_O_DESCRIPCION.includes(c)) asignar(i, usados.has('nombre') ? 'descripcion' : 'nombre');
  });
  claves.forEach((c, i) => {
    if (SOLO_DESCRIPCION.includes(c) || (c.includes('descripcion') && !mapa[i])) asignar(i, usados.has('nombre') ? 'descripcion' : 'nombre');
  });
  // "3 precios de venta": otras columnas de precio que sobren son el Precio 2 y el 3.
  claves.forEach((c, i) => {
    if (mapa[i] || !c.includes('precio') || c.includes('compra') || c.includes('costo')) return;
    asignar(i, usados.has('precio_2') ? 'precio_3' : 'precio_2');
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

// Ficha del medicamento de una fila: solo si el archivo trae alguna de sus
// columnas y la fila tiene algo escrito en ellas. Los textos van tal cual
// ("Tableta", "con receta"): el servidor los reconoce.
function fichaDeFila(fila, texto, columna) {
  if (!CAMPOS_FICHA.some((c) => columna[c.valor] !== undefined)) return {};
  const ficha = Object.fromEntries(CAMPOS_FICHA.map((c) => [c.valor, texto(fila, c.valor) || null]));
  return Object.values(ficha).some(Boolean) ? { ficha } : {};
}

// Lubricentro: ficha del repuesto y compatibilidad de una fila. Solo si el
// archivo trae esas columnas (si no, el producto queda como estaba).
function repuestoDeFila(fila, texto, columna) {
  const salida = {};
  if (CAMPOS_REPUESTO.some((c) => columna[c.valor] !== undefined)) {
    salida.repuesto = {
      marca: texto(fila, 'marca_repuesto') || null,
      numero_parte: texto(fila, 'numero_parte') || null,
      codigos_alternos: texto(fila, 'codigos_alternos') || null,
      ubicacion: texto(fila, 'ubicacion') || null,
      garantia: texto(fila, 'garantia') || null,
    };
  }
  if (columna.compatible !== undefined) salida.compatible = texto(fila, 'compatible') || '';
  return salida;
}

// Librería: ficha del libro de una fila, solo si el archivo trae alguna de sus columnas.
function libroDeFila(fila, texto, columna) {
  if (!CAMPOS_LIBRO.some((c) => columna[c.valor] !== undefined)) return {};
  return {
    libro: {
      autor: texto(fila, 'libro_autor') || null,
      editorial: texto(fila, 'libro_editorial') || null,
      serie: texto(fila, 'libro_serie') || null,
      anio: texto(fila, 'libro_anio') || null,
      edicion: texto(fila, 'libro_edicion') || null,
    },
  };
}

// "3 precios de venta": Precio 2 y 3 de una fila, solo si el archivo trae esas columnas.
function preciosDeFila(fila, numero, columna) {
  if (columna.precio_2 === undefined && columna.precio_3 === undefined) return {};
  return { precios: { precio_2: numero(fila, 'precio_2'), precio_3: numero(fila, 'precio_3') } };
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
      lote: texto(fila, 'lote') || null,
      // Tal como viene ("31/12/2027", "12/2027"): el servidor la lee.
      vencimiento: texto(fila, 'vencimiento') || null,
      ...fichaDeFila(fila, texto, columna),
      ...repuestoDeFila(fila, texto, columna),
      ...preciosDeFila(fila, numero, columna),
      ...libroDeFila(fila, texto, columna),
      // Módulo "Equivalentes": se aplican después de importar (no van al servidor de productos).
      ...(columna.equivalentes !== undefined ? { equivalentes: texto(fila, 'equivalentes') || null } : {}),
    });
  });
  return { filas: salida, unidadesRaras };
}
