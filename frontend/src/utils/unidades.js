// Catálogo de unidades de venta. Es el mismo que el del backend
// (src/handlers/unidades.rs): si se agrega una unidad allá, también aquí.
// Cada negocio elige cuáles usa en Configuración → Unidades.
export const GRUPOS_UNIDADES = [
  {
    titulo: 'Restaurante y cafetería',
    restaurante: true,
    unidades: [
      { valor: 'PLATO', label: 'Plato', abrev: 'plato' },
      { valor: 'PORCION', label: 'Porción', abrev: 'porc' },
      { valor: 'ENTERO', label: 'Entero', abrev: 'entero' },
      { valor: 'MEDIO', label: 'Medio', abrev: 'medio' },
      { valor: 'CUARTO', label: 'Cuarto', abrev: 'cuarto' },
      { valor: 'VASO', label: 'Vaso', abrev: 'vaso' },
      { valor: 'TAZA', label: 'Taza', abrev: 'taza' },
      { valor: 'JARRA', label: 'Jarra', abrev: 'jarra' },
      { valor: 'COPA', label: 'Copa', abrev: 'copa' },
      { valor: 'BOTELLA', label: 'Botella', abrev: 'bot' },
    ],
  },
  {
    titulo: 'Básicas',
    unidades: [
      { valor: 'UNIDAD', label: 'Unidad', abrev: 'und' },
      { valor: 'PAQUETE', label: 'Paquete', abrev: 'paq' },
      { valor: 'CAJA', label: 'Caja', abrev: 'caja' },
      { valor: 'DOCENA', label: 'Docena', abrev: 'doc' },
      { valor: 'PAR', label: 'Par', abrev: 'par' },
      { valor: 'BOLSA', label: 'Bolsa', abrev: 'bolsa' },
      { valor: 'JUEGO', label: 'Juego', abrev: 'jgo' },
    ],
  },
  {
    titulo: 'Peso',
    unidades: [
      { valor: 'KG', label: 'Kilogramo', abrev: 'kg' },
      { valor: 'GRAMO', label: 'Gramo', abrev: 'g' },
      { valor: 'LIBRA', label: 'Libra', abrev: 'lb' },
      { valor: 'ONZA', label: 'Onza', abrev: 'oz' },
      { valor: 'SACO', label: 'Saco', abrev: 'saco' },
      { valor: 'TONELADA', label: 'Tonelada', abrev: 't' },
    ],
  },
  {
    titulo: 'Líquidos',
    unidades: [
      { valor: 'LITRO', label: 'Litro', abrev: 'L' },
      { valor: 'ML', label: 'Mililitro', abrev: 'ml' },
      { valor: 'GALON', label: 'Galón', abrev: 'gal' },
    ],
  },
  {
    titulo: 'Medida y otros',
    unidades: [
      { valor: 'METRO', label: 'Metro', abrev: 'm' },
      { valor: 'YARDA', label: 'Yarda', abrev: 'yd' },
      { valor: 'ROLLO', label: 'Rollo', abrev: 'rollo' },
      { valor: 'MILLAR', label: 'Millar', abrev: 'mill' },
    ],
  },
];

// La unidad base: siempre activa.
export const UNIDAD_BASE = 'UNIDAD';

const TODAS = GRUPOS_UNIDADES.flatMap((g) => g.unidades.map((u) => ({ ...u, restaurante: !!g.restaurante })));
const POR_VALOR = new Map(TODAS.map((u) => [u.valor, u]));

/** Las 20 de siempre (lo que ve un negocio que nunca configuró sus unidades). */
export const UNIDADES_CLASICAS = TODAS.filter((u) => !u.restaurante).map((u) => u.valor);

/** "Kilogramo" a partir de "KG" (o el mismo código si no se conoce). */
export function etiquetaUnidad(valor) {
  return POR_VALOR.get(valor)?.label || valor || '';
}

/** "kg" a partir de "KG": para etiquetas cortas de stock. */
export function abreviaturaUnidad(valor) {
  return POR_VALOR.get(valor)?.abrev || (valor || 'und').toLowerCase();
}

/** Con qué conviene empezar según el tipo de negocio. */
export function unidadesRecomendadas(restaurante) {
  return restaurante
    ? [UNIDAD_BASE, ...TODAS.filter((u) => u.restaurante).map((u) => u.valor)]
    : UNIDADES_CLASICAS;
}

/**
 * Opciones para el desplegable de un producto: las activas del negocio,
 * con la unidad base primero. `incluir` es la unidad que ya tiene el
 * producto que se edita (se muestra aunque ya no esté activa).
 */
export function opcionesUnidad(activas, incluir) {
  const visibles = new Set(activas?.length ? activas : UNIDADES_CLASICAS);
  visibles.add(UNIDAD_BASE);
  if (incluir) visibles.add(incluir);
  const lista = TODAS.filter((u) => visibles.has(u.valor));
  return [...lista.filter((u) => u.valor === UNIDAD_BASE), ...lista.filter((u) => u.valor !== UNIDAD_BASE)];
}
