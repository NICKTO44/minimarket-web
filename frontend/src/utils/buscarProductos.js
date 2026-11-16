// Búsqueda de productos para los buscadores (compras, etc.).

// Sin tildes ni mayúsculas, separado en palabras: "Amoxicilina 500mg" -> ["amoxicilina", "500mg"].
const limpiar = (t) =>
  String(t ?? '')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '');
const palabras = (t) => limpiar(t).split(/[^a-z0-9.]+/).filter(Boolean);

// Cuántos se muestran a la vez (con más, se pide escribir más).
export const MAXIMO_VISIBLES = 60;

/**
 * Lista de productos ya ordenada para lo que se escribió: primero el código
 * exacto, luego los que empiezan con lo escrito, luego los que tienen todas
 * las palabras (en cualquier orden y en nombre, código o marca).
 */
export function buscarProductos(productos, texto) {
  const ps = palabras(texto);
  if (ps.length === 0) return productos;
  const junto = limpiar(texto).trim();
  const encontrados = [];
  for (const p of productos) {
    const nombre = limpiar(p.nombre);
    const codigo = limpiar(p.codigo);
    const todo = `${nombre} ${codigo} ${limpiar(p.marca)} ${limpiar(p.principio_activo)}`;
    if (!ps.every((x) => todo.includes(x))) continue;
    const puntos = codigo && codigo === junto ? 0 : nombre.startsWith(junto) || (codigo && codigo.startsWith(junto)) ? 1 : 2;
    encontrados.push({ p, puntos });
  }
  return encontrados
    .sort((a, b) => a.puntos - b.puntos || String(a.p.nombre).localeCompare(String(b.p.nombre), 'es'))
    .map((x) => x.p);
}
