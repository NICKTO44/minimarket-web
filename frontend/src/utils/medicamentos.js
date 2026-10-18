// Módulo "Ficha del medicamento" (rubro Farmacia / Botica). Las formas y
// condiciones de venta deben coincidir con backend/src/handlers/medicamentos.rs.

export const FORMAS_FARMACEUTICAS = [
  { valor: 'TABLETA', label: 'Tableta' },
  { valor: 'TABLETA_RECUBIERTA', label: 'Tableta recubierta' },
  { valor: 'CAPSULA', label: 'Cápsula' },
  { valor: 'JARABE', label: 'Jarabe' },
  { valor: 'SUSPENSION', label: 'Suspensión' },
  { valor: 'SOLUCION', label: 'Solución' },
  { valor: 'GOTAS', label: 'Gotas' },
  { valor: 'INYECTABLE', label: 'Inyectable' },
  { valor: 'CREMA', label: 'Crema' },
  { valor: 'UNGUENTO', label: 'Ungüento' },
  { valor: 'GEL', label: 'Gel' },
  { valor: 'OVULO', label: 'Óvulo' },
  { valor: 'SUPOSITORIO', label: 'Supositorio' },
  { valor: 'INHALADOR', label: 'Inhalador' },
  { valor: 'POLVO', label: 'Polvo' },
  { valor: 'PARCHE', label: 'Parche' },
  { valor: 'COLIRIO', label: 'Colirio' },
  { valor: 'SPRAY', label: 'Spray' },
  { valor: 'OTRA', label: 'Otra' },
];

export const CONDICIONES_VENTA = [
  { valor: 'SIN_RECETA', label: 'Sin receta (venta libre)', corto: null },
  { valor: 'CON_RECETA', label: 'Con receta médica', corto: 'Receta' },
  {
    valor: 'RECETA_RETENIDA',
    label: 'Receta médica retenida (psicotrópicos y estupefacientes)',
    corto: 'Receta retenida',
  },
];

const FORMA_POR_VALOR = new Map(FORMAS_FARMACEUTICAS.map((f) => [f.valor, f.label]));
const CONDICION_POR_VALOR = new Map(CONDICIONES_VENTA.map((c) => [c.valor, c]));

export function etiquetaForma(valor) {
  return FORMA_POR_VALOR.get(valor) || '';
}

/** "Receta" / "Receta retenida" para la etiqueta corta (null si es venta libre). */
export function etiquetaCondicion(valor) {
  return CONDICION_POR_VALOR.get(valor)?.corto || null;
}

/** "Paracetamol 500 mg · Tableta" */
export function resumenFicha(ficha) {
  if (!ficha) return '';
  const principal = [ficha.principio_activo, ficha.concentracion].filter(Boolean).join(' ');
  return [principal, etiquetaForma(ficha.forma_farmaceutica)].filter(Boolean).join(' · ');
}

/** Minúsculas y sin tildes, para buscar "cafeina" y encontrar "Cafeína". */
export function textoPlano(texto) {
  return String(texto || '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '');
}

/** ¿La búsqueda (ya en textoPlano) aparece en el principio activo o el laboratorio? */
export function fichaCoincide(ficha, consultaPlana) {
  if (!ficha || !consultaPlana) return false;
  return textoPlano(ficha.principio_activo).includes(consultaPlana) || textoPlano(ficha.laboratorio).includes(consultaPlana);
}

/** Lista del servidor -> Map producto_id -> ficha. */
export function mapaFichas(lista) {
  return new Map((lista || []).map((f) => [f.producto_id, f]));
}

export const FICHA_VACIA = {
  es_medicamento: false,
  principio_activo: '',
  concentracion: '',
  forma_farmaceutica: '',
  laboratorio: '',
  registro_sanitario: '',
  condicion_venta: 'SIN_RECETA',
  es_generico: false,
};

/** Ficha del servidor (o undefined) -> campos del formulario. */
export function fichaAFormulario(ficha) {
  if (!ficha) return { ...FICHA_VACIA };
  return {
    es_medicamento: true,
    principio_activo: ficha.principio_activo || '',
    concentracion: ficha.concentracion || '',
    forma_farmaceutica: ficha.forma_farmaceutica || '',
    laboratorio: ficha.laboratorio || '',
    registro_sanitario: ficha.registro_sanitario || '',
    condicion_venta: ficha.condicion_venta || 'SIN_RECETA',
    es_generico: !!ficha.es_generico,
  };
}

/**
 * Campos del formulario -> { ficha } para guardar, { quitar: true } si dejó
 * de ser medicamento, o { error } con un mensaje para el usuario.
 */
export function formularioAFicha(campos) {
  if (!campos.es_medicamento) return { quitar: true };
  const principio = campos.principio_activo.trim();
  if (principio.length < 3) {
    return { error: 'Escribe el principio activo del medicamento (por ejemplo "Paracetamol").' };
  }
  return {
    ficha: {
      principio_activo: principio,
      concentracion: campos.concentracion.trim() || null,
      forma_farmaceutica: campos.forma_farmaceutica || null,
      laboratorio: campos.laboratorio.trim() || null,
      registro_sanitario: campos.registro_sanitario.trim().toUpperCase() || null,
      condicion_venta: campos.condicion_venta,
      es_generico: !!campos.es_generico,
    },
  };
}

/**
 * Alternativas de un medicamento (módulo ALTERNATIVAS): los productos con el
 * MISMO principio activo, concentración y forma farmacéutica. Si a la ficha
 * le falta la concentración o la forma, no se ofrece ninguna: no se compara
 * a medias tratándose de medicamentos. Primero las que tienen stock, de la
 * más barata a la más cara.
 */
export function alternativasDe(productoId, fichas, productos, disponible = (p) => (p.controla_stock === false ? Infinity : p.stock)) {
  const base = fichas.get(productoId);
  if (!base?.principio_clave || !base.concentracion_clave || !base.forma_farmaceutica) return [];
  return productos
    .filter((p) => {
      if (p.id === productoId || p.agotado) return false;
      const f = fichas.get(p.id);
      return (
        f &&
        f.principio_clave === base.principio_clave &&
        f.concentracion_clave === base.concentracion_clave &&
        f.forma_farmaceutica === base.forma_farmaceutica
      );
    })
    // conStock: lo que queda para vender (en el punto de venta, descontando lo que ya está en el carrito).
    .map((p) => ({ producto: p, ficha: fichas.get(p.id), conStock: disponible(p) > 0 }))
    .sort((a, b) => Number(b.conStock) - Number(a.conStock) || a.producto.precio - b.producto.precio);
}
