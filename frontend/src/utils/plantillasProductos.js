// Plantilla de Excel de productos según el negocio (ver utils/plantillaExcel.js).
// Los títulos son los que reconoce "Importar productos" (utils/importacion.js):
// el " *" de los obligatorios no estorba (se ignora al leer).
import { CONDICIONES_VENTA, FORMAS_FARMACEUTICAS, etiquetaForma } from './medicamentos';
import { normalizar } from './importacion';
import { etiquetaUnidad, opcionesUnidad } from './unidades';

const BASE = [
  { titulo: 'Código *', grupo: 'OBLIGATORIO', tipo: 'texto', ancho: 16, ayuda: 'El código interno o el de barras. No se repite entre productos (en varias filas solo si son lotes del mismo producto).' },
  { titulo: 'Nombre *', grupo: 'OBLIGATORIO', tipo: 'texto', ancho: 38, ayuda: 'Como quieres verlo al vender y en la boleta.' },
  { titulo: 'Precio de venta *', grupo: 'OBLIGATORIO', tipo: 'soles', ancho: 15, ayuda: 'En soles, con IGV. Solo el número: 12.50' },
  { titulo: 'Descripción', grupo: 'PRODUCTO', tipo: 'texto', ancho: 28, ayuda: 'Opcional.' },
  { titulo: 'Categoría', grupo: 'PRODUCTO', tipo: 'lista', lista: 'Categorías', ancho: 20, ayuda: 'Elige de la lista. Si escribes una nueva, se crea al importar.' },
  { titulo: 'Unidad', grupo: 'PRODUCTO', tipo: 'lista', lista: 'Unidades', ancho: 14, ayuda: 'Cómo se vende: Unidad, Caja, Tableta… Si la dejas vacía, queda Unidad.' },
  { titulo: 'Precio de compra', grupo: 'PRODUCTO', tipo: 'soles', ancho: 15, ayuda: 'Lo que te cuesta cada unidad, en soles. Sirve para tus ganancias.' },
  { titulo: 'Stock', grupo: 'PRODUCTO', tipo: 'numero', ancho: 10, ayuda: 'Cuántos tienes hoy. Con lote: la cantidad de ese lote.' },
  { titulo: 'Stock mínimo', grupo: 'PRODUCTO', tipo: 'numero', ancho: 12, ayuda: 'Te avisa cuando quedan esta cantidad o menos.' },
];

const LOTES = [
  { titulo: 'Lote', grupo: 'LOTE', tipo: 'texto', ancho: 13, ayuda: 'N.° de lote del empaque (opcional). Un producto con varios lotes: una fila por lote, repitiendo el código.' },
  { titulo: 'Vencimiento', grupo: 'LOTE', tipo: 'fecha', ancho: 14, ayuda: 'Fecha de vencimiento: 31/12/2027. Si el empaque solo trae mes y año, escribe 12/2027.' },
];

const FICHA_MEDICAMENTO = [
  { titulo: 'Principio activo', grupo: 'FICHA', tipo: 'texto', ancho: 24, ayuda: 'El genérico (DCI): Paracetamol, Amoxicilina… Sirve para buscar alternativas.' },
  { titulo: 'Concentración', grupo: 'FICHA', tipo: 'texto', ancho: 14, ayuda: 'Por ejemplo: 500 mg, 250 mg/5 ml.' },
  { titulo: 'Forma farmacéutica', grupo: 'FICHA', tipo: 'lista', lista: 'Formas', ancho: 20, ayuda: 'Tableta, cápsula, jarabe… Elige de la lista.' },
  { titulo: 'Laboratorio', grupo: 'FICHA', tipo: 'texto', ancho: 20, ayuda: 'Quién lo fabrica.' },
  { titulo: 'Registro sanitario', grupo: 'FICHA', tipo: 'texto', ancho: 18, ayuda: 'N.° de registro DIGEMID (opcional).' },
  { titulo: 'Condición de venta', grupo: 'FICHA', tipo: 'lista', lista: 'Condición', ancho: 18, ayuda: 'Sin receta, con receta o receta retenida. Con receta, el punto de venta la pide al cobrar.' },
  { titulo: 'Genérico', grupo: 'FICHA', tipo: 'lista', lista: 'Sí/No', ancho: 10, ayuda: 'Sí si es genérico, No si es de marca.' },
];

const LIBRO = [
  { titulo: 'Autor', grupo: 'EXTRA', tipo: 'texto', ancho: 26, ayuda: 'El ISBN va en la columna Código.' },
  { titulo: 'Editorial', grupo: 'EXTRA', tipo: 'texto', ancho: 20 },
  { titulo: 'Serie', grupo: 'EXTRA', tipo: 'texto', ancho: 18, ayuda: 'Serie o colección (opcional).' },
  { titulo: 'Año', grupo: 'EXTRA', tipo: 'entero', ancho: 8 },
  { titulo: 'Edición', grupo: 'EXTRA', tipo: 'texto', ancho: 14 },
];

// Módulo Presentaciones: blíster y caja en la misma fila del producto.
// Al importar se guardan como presentaciones (cuántas unidades lleva y su precio).
export const PRESENTACIONES_PLANTILLA = [
  { nombre: 'Blíster', clave: 'blister' },
  { nombre: 'Caja', clave: 'caja' },
];
const COLUMNAS_PRESENTACION = PRESENTACIONES_PLANTILLA.flatMap((p) => [
  { titulo: `${p.nombre}: unidades`, grupo: 'PRESENTACION', tipo: 'numero', ancho: 12, ayuda: `Cuántas unidades del producto trae un ${p.nombre.toLowerCase()} (por ejemplo 10). Vacío si no se vende por ${p.nombre.toLowerCase()}.` },
  { titulo: `${p.nombre}: precio`, grupo: 'PRESENTACION', tipo: 'soles', ancho: 12, ayuda: `Precio del ${p.nombre.toLowerCase()} entero, en soles.` },
]);

/** { indiceDeColumna: { nombre, campo: 'factor'|'precio' } } de las columnas de blíster y caja. */
export function columnasPresentacionDe(titulos) {
  const salida = {};
  (titulos || []).forEach((t, i) => {
    const k = normalizar(t);
    for (const p of PRESENTACIONES_PLANTILLA) {
      if (k === `${p.clave}unidades`) salida[i] = { nombre: p.nombre, campo: 'factor' };
      if (k === `${p.clave}precio`) salida[i] = { nombre: p.nombre, campo: 'precio' };
    }
  });
  return salida;
}

/**
 * Las presentaciones que trae la hoja (para api.presentacionesImportar):
 * una por producto y nombre, aunque el producto venga en varias filas (lotes).
 */
export function presentacionesDeHoja(hoja, filaTitulos, columnaCodigo, columnas) {
  const vistas = new Map();
  const numero = (v) => {
    const n = parseFloat(String(v ?? '').replace(/[^\d.,-]/g, '').replace(',', '.'));
    return Number.isFinite(n) ? n : null;
  };
  hoja.slice(filaTitulos + 1).forEach((fila, i) => {
    const codigo = String(fila?.[columnaCodigo] ?? '').trim();
    if (!codigo) return;
    const porNombre = {};
    for (const [indice, c] of Object.entries(columnas)) {
      porNombre[c.nombre] = porNombre[c.nombre] || {};
      porNombre[c.nombre][c.campo] = numero(fila?.[indice]);
    }
    for (const [nombre, d] of Object.entries(porNombre)) {
      if (!(d.factor > 0) || !(d.precio > 0)) continue;
      const clave = `${codigo.toLowerCase()}|${nombre}`;
      if (!vistas.has(clave)) vistas.set(clave, { fila: filaTitulos + i + 2, codigo_producto: codigo, nombre, factor: d.factor, precio: d.precio });
    }
  });
  return [...vistas.values()];
}

const CONDICIONES_CORTAS = { SIN_RECETA: 'Sin receta', CON_RECETA: 'Con receta', RECETA_RETENIDA: 'Receta retenida' };

/** Nombre del archivo según el negocio. */
export function nombrePlantilla({ fichaMedicamento, fichaLibro }) {
  if (fichaMedicamento) return 'Plantilla-productos-farmacia.xlsx';
  if (fichaLibro) return 'Plantilla-productos-libreria.xlsx';
  return 'Plantilla-productos.xlsx';
}

/**
 * Datos para libroPlantilla según los módulos del negocio:
 * { fichaMedicamento, fichaLibro, categorias: [{ nombre }], unidadesActivas: [valor] }.
 */
export function plantillaProductos({ fichaMedicamento = false, fichaLibro = false, presentaciones = false, categorias = [], unidadesActivas = [] }) {
  const columnas = [
    ...BASE,
    ...LOTES,
    ...(presentaciones ? COLUMNAS_PRESENTACION : []),
    ...(fichaMedicamento ? FICHA_MEDICAMENTO : []),
    ...(fichaLibro ? LIBRO : []),
  ];
  const listas = {
    Categorías: categorias.map((c) => c.nombre).filter(Boolean),
    Unidades: opcionesUnidad(unidadesActivas).map((u) => u.label),
    ...(fichaMedicamento
      ? {
          Formas: FORMAS_FARMACEUTICAS.map((f) => f.label),
          Condición: CONDICIONES_VENTA.map((c) => CONDICIONES_CORTAS[c.valor] || c.label),
          'Sí/No': ['Sí', 'No'],
        }
      : {}),
  };

  const negocio = fichaMedicamento ? 'Farmacia / Botica' : fichaLibro ? 'Librería' : 'tu negocio';
  const grupos = [
    'OBLIGATORIO',
    'PRODUCTO',
    'LOTE',
    ...(presentaciones ? ['PRESENTACION'] : []),
    ...(fichaMedicamento ? ['FICHA'] : []),
    ...(fichaLibro ? ['EXTRA'] : []),
  ];
  const instrucciones = [
    { tipo: 'titulo', texto: `Plantilla de productos — ${negocio}` },
    {
      tipo: 'parrafo',
      texto:
        'Llena la hoja "Productos": una fila por producto. Luego, en el sistema: Productos → Importar → sube este archivo. Nada se guarda hasta que revises y confirmes.',
      alto: 32,
    },
    {
      tipo: 'parrafo',
      texto:
        'Al pararte en una celda verás una ayuda. Categoría, Unidad y las columnas con flecha tienen una lista para elegir. Los precios se escriben como número (12.50) y salen en soles. No cambies los títulos de la fila 1.',
      alto: 32,
    },
    { tipo: 'subtitulo', texto: 'Qué significa cada color' },
    {
      tipo: 'grupos',
      grupos,
      textos: {
        OBLIGATORIO: 'Sin estos tres datos el producto no se puede importar.',
        PRODUCTO: 'Opcionales, pero recomendados: categoría, unidad, costo y stock.',
        LOTE: fichaMedicamento
          ? 'Para medicamentos con vencimiento. Un producto con 2 lotes = 2 filas con el mismo código, cada una con su lote, vencimiento y stock.'
          : 'Solo para productos que vencen. Si no vencen, déjalas vacías.',
        FICHA: 'Datos del medicamento: para buscar por principio activo, ver alternativas y pedir receta al vender.',
        EXTRA: 'Datos del libro: para buscar por autor o editorial.',
        PRESENTACION: fichaMedicamento
          ? 'Si también vendes por blíster o por caja: cuántas tabletas trae y su precio. El producto se vende por unidad (tableta) y el punto de venta te deja elegir blíster o caja.'
          : 'Si también lo vendes en otra presentación: cuántas unidades trae y su precio.',
      },
    },
    { tipo: 'subtitulo', texto: 'Columna por columna' },
    { tipo: 'columnas', columnas },
  ];
  if (fichaMedicamento && !presentaciones) {
    instrucciones.push(
      { tipo: 'subtitulo', texto: 'Precios por blíster o caja' },
      {
        tipo: 'parrafo',
        texto:
          'Importa primero los productos por unidad (tableta, cápsula). Luego, en Productos → Presentaciones, agrega el blíster y la caja con cuántas unidades lleva cada una y su precio.',
        alto: 32,
      }
    );
  }

  const ejemplos = fichaMedicamento
    ? [
        ['7750001001234', 'Paracetamol 500 mg x 100 tab', 0.5, '', 'Analgésico', 'Tableta', 0.2, 300, 50, 'L2401', '2027-12-31', 'Paracetamol', '500 mg', 'Tableta', 'Laboratorio Ejemplo', 'EN-01234', 'Sin receta', 'Sí'],
        ['7750001001234', 'Paracetamol 500 mg x 100 tab', 0.5, '', 'Analgésico', 'Tableta', 0.2, 200, 50, 'L2409', '2028-06-30', 'Paracetamol', '500 mg', 'Tableta', 'Laboratorio Ejemplo', 'EN-01234', 'Sin receta', 'Sí'],
        ['AMOX-250', 'Amoxicilina 250 mg/5 ml susp. 60 ml', 9.9, 'Agitar antes de usar', 'Antibióticos', 'Frasco', 5.5, 12, 4, 'A3321', '2027-03-31', 'Amoxicilina', '250 mg/5 ml', 'Suspensión', 'Laboratorio Ejemplo', 'EE-05678', 'Con receta', 'Sí'],
      ]
    : fichaLibro
      ? [['9786120000001', 'Cien años de soledad (ed. bolsillo)', 45, '', 'Novelas', 'Unidad', 30, 5, 2, '', '', 'Gabriel García Márquez', 'Editorial Ejemplo', '', 2017, '1.ª edición']]
      : [['7751234000011', 'Gaseosa 500 ml', 2.5, '', 'Bebidas', 'Unidad', 1.6, 48, 12, '', '']];

  // Ejemplos: blíster y caja van después de lote y vencimiento.
  const conPresentacion = (fila, valores) => (presentaciones ? [...fila.slice(0, 11), ...valores, ...fila.slice(11)] : fila);
  const ejemplosFinales = ejemplos.map((fila, i) =>
    conPresentacion(fila, fichaMedicamento && i < 2 ? [10, 4.5, 100, 40] : ['', '', '', ''])
  );

  return { columnas, listas, instrucciones, ejemplos: ejemplosFinales };
}

const SI_NO = (v) => (v === true ? 'Sí' : v === false ? 'No' : '');

/**
 * Filas para exportar con el mismo formato de la plantilla. Un producto con
 * lotes sale en una fila por lote (con su stock); los demás, en una fila.
 * fichas: Map producto_id -> ficha; lotes: [{ producto_id, numero_lote,
 * fecha_vencimiento, cantidad }]; opciones: { presentaciones: [...] }.
 */
export function filasParaExportar(productos, { fichaMedicamento, fichaLibro, presentaciones, fichas, libros, lotes = [], opciones }) {
  const lotesDe = new Map();
  for (const l of lotes) {
    if (!lotesDe.has(l.producto_id)) lotesDe.set(l.producto_id, []);
    lotesDe.get(l.producto_id).push(l);
  }
  const presDe = new Map();
  for (const p of opciones?.presentaciones || []) {
    const k = normalizar(p.nombre);
    const plantilla = PRESENTACIONES_PLANTILLA.find((x) => k === x.clave || k.startsWith(x.clave));
    if (!plantilla) continue;
    if (!presDe.has(p.producto_id)) presDe.set(p.producto_id, {});
    const actual = presDe.get(p.producto_id);
    if (!actual[plantilla.nombre]) actual[plantilla.nombre] = p;
  }
  const numero = (n) => (Number(n) > 0 ? Number(n) : '');
  const filas = [];
  for (const p of productos) {
    const base = [
      String(p.codigo ?? ''),
      p.nombre,
      Number(p.precio) || 0,
      p.descripcion || '',
      p.categoria_nombre || '',
      etiquetaUnidad(p.unidad_medida),
      numero(p.precio_compra),
    ];
    const stockMinimo = Number(p.stock_minimo) || 0;
    const pres = presDe.get(p.id) || {};
    const columnasPres = presentaciones
      ? PRESENTACIONES_PLANTILLA.flatMap((x) => (pres[x.nombre] ? [Number(pres[x.nombre].factor), Number(pres[x.nombre].precio)] : ['', '']))
      : [];
    const f = fichas?.get(p.id);
    const columnasFicha = fichaMedicamento
      ? [
          f?.principio_activo || '',
          f?.concentracion || '',
          f?.forma_farmaceutica ? etiquetaForma(f.forma_farmaceutica) : '',
          f?.laboratorio || '',
          f?.registro_sanitario || '',
          f?.condicion_venta ? CONDICIONES_CORTAS[f.condicion_venta] || '' : '',
          f ? SI_NO(f.es_generico) : '',
        ]
      : [];
    const lb = libros?.get(p.id);
    const columnasLibro = fichaLibro ? [lb?.autor || '', lb?.editorial || '', lb?.serie || '', lb?.anio || '', lb?.edicion || ''] : [];
    const sinStock = p.controla_stock === false;
    const suyos = lotesDe.get(p.id) || [];
    if (p.lleva_vencimiento && suyos.length) {
      for (const l of suyos) {
        filas.push([...base, Number(l.cantidad), stockMinimo, l.numero_lote || '', String(l.fecha_vencimiento || '').slice(0, 10), ...columnasPres, ...columnasFicha, ...columnasLibro]);
      }
    } else {
      filas.push([...base, sinStock ? '' : Number(p.stock) || 0, stockMinimo, '', '', ...columnasPres, ...columnasFicha, ...columnasLibro]);
    }
  }
  return filas;
}
