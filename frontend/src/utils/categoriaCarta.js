// Categoría interna de los platos de la "Carta de hoy" (la crea el backend,
// ver handlers/carta.rs). No se elige para productos de la carta fija ni para
// mercadería: se oculta de las listas de categorías.
export const CATEGORIA_CARTA = 'Carta del día';

/**
 * Las categorías sin la de la carta del día. Si algún producto fijo ya quedó
 * en ella (de antes de este cambio), se sigue mostrando para poder editarlo.
 */
export function sinCategoriaCarta(categorias, productos = []) {
  if (!Array.isArray(categorias)) return [];
  return categorias.filter(
    (c) => c.nombre !== CATEGORIA_CARTA || productos.some((p) => !p.carta_dia && String(p.categoria_id) === String(c.id))
  );
}
