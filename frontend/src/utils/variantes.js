// Tallas y colores (módulo VARIANTES, rubro Ropa y calzado).
// Cada talla/color de un modelo es un producto normal (su código, precio y
// stock). Lo que los une es modelo_id; aquí se agrupan para mostrarlos como
// un solo modelo en Productos y en el punto de venta.

/** Separa el modelo de su talla y color en el nombre de venta (igual que el servidor). */
export const SEPARADOR_VARIANTE = ' · ';

// Orden natural de las tallas de letras; las de número van por su valor.
const ORDEN_LETRAS = ['XXS', 'XS', 'S', 'M', 'L', 'XL', 'XXL', '2XL', 'XXXL', '3XL', '4XL'];

/** Juegos de tallas para llenar rápido. */
export const TALLAS_SUGERIDAS = [
  { label: 'S a XL', tallas: ['S', 'M', 'L', 'XL'] },
  { label: 'XS a XXL', tallas: ['XS', 'S', 'M', 'L', 'XL', 'XXL'] },
  { label: 'Pantalón 28 a 36', tallas: ['28', '30', '32', '34', '36'] },
  { label: 'Calzado 35 a 39', tallas: ['35', '36', '37', '38', '39'] },
  { label: 'Calzado 38 a 43', tallas: ['38', '39', '40', '41', '42', '43'] },
  { label: 'Niños 2 a 12', tallas: ['2', '4', '6', '8', '10', '12'] },
];

const limpiar = (texto) => String(texto || '').trim().replace(/\s+/g, ' ');

function pesoTalla(talla) {
  const t = limpiar(talla).toUpperCase();
  const letra = ORDEN_LETRAS.indexOf(t);
  if (letra >= 0) return [0, letra];
  const numero = parseFloat(t.replace(',', '.'));
  if (!Number.isNaN(numero)) return [1, numero];
  return [2, 0];
}

/** Compara dos tallas: XS < S < M < L < XL, y 28 < 30 < 32. */
export function compararTallas(a, b) {
  const [ga, va] = pesoTalla(a);
  const [gb, vb] = pesoTalla(b);
  if (ga !== gb) return ga - gb;
  if (va !== vb) return va - vb;
  return limpiar(a).localeCompare(limpiar(b), 'es');
}

/** "Polo básico · M · Negro" */
export function nombreVariante(modelo, talla, color) {
  return [limpiar(modelo), limpiar(talla), limpiar(color)].filter(Boolean).join(SEPARADOR_VARIANTE);
}

/** "M · Negro" (lo que distingue a esa variante dentro de su modelo). */
export function etiquetaVariante(producto) {
  return [limpiar(producto.talla), limpiar(producto.color)].filter(Boolean).join(SEPARADOR_VARIANTE);
}

const sinRepetir = (lista) => {
  const vistos = new Set();
  return lista.filter((x) => {
    const clave = x.toLowerCase();
    if (!x || vistos.has(clave)) return false;
    vistos.add(clave);
    return true;
  });
};

/**
 * Agrupa los productos por modelo. Devuelve la lista en el mismo orden, con
 * los productos sin modelo tal cual y, en el lugar de la primera talla de
 * cada modelo, un grupo:
 *   { esModelo, modelo_id, nombre, variantes, tallas, colores,
 *     precioMin, precioMax, stockTotal, imagen_url, categoria_id, ... }
 */
export function agruparPorModelo(productos) {
  const grupos = new Map();
  const salida = [];
  for (const p of productos) {
    if (p.modelo_id == null) {
      salida.push(p);
      continue;
    }
    let grupo = grupos.get(p.modelo_id);
    if (!grupo) {
      grupo = {
        esModelo: true,
        modelo_id: p.modelo_id,
        id: `modelo-${p.modelo_id}`,
        nombre: p.modelo_nombre || p.nombre,
        categoria_id: p.categoria_id,
        categoria_nombre: p.categoria_nombre,
        unidad_medida: p.unidad_medida,
        variantes: [],
      };
      grupos.set(p.modelo_id, grupo);
      salida.push(grupo);
    }
    grupo.variantes.push(p);
  }
  for (const grupo of grupos.values()) {
    grupo.variantes.sort(
      (a, b) => compararTallas(a.talla, b.talla) || limpiar(a.color).localeCompare(limpiar(b.color), 'es')
    );
    grupo.tallas = sinRepetir(grupo.variantes.map((v) => limpiar(v.talla))).sort(compararTallas);
    grupo.colores = sinRepetir(grupo.variantes.map((v) => limpiar(v.color)));
    const precios = grupo.variantes.map((v) => v.precio);
    grupo.precioMin = Math.min(...precios);
    grupo.precioMax = Math.max(...precios);
    grupo.precio = grupo.precioMin;
    grupo.stockTotal = grupo.variantes.reduce((s, v) => s + (v.stock || 0), 0);
    grupo.imagen_url = grupo.variantes.find((v) => v.imagen_url)?.imagen_url || null;
    grupo.conStockBajo = grupo.variantes.filter((v) => v.stock <= v.stock_minimo).length;
  }
  return salida;
}

/** "S/ 30.00" o "S/ 30.00 – 40.00" cuando las tallas tienen precios distintos. */
export function rangoPrecio(grupo) {
  if (grupo.precioMin === grupo.precioMax) return `S/ ${grupo.precioMin.toFixed(2)}`;
  return `S/ ${grupo.precioMin.toFixed(2)} – ${grupo.precioMax.toFixed(2)}`;
}

/** Texto escrito ("s, m, l" o "S M L") convertido en lista sin repetidos. */
export function leerLista(texto) {
  return sinRepetir(
    String(texto || '')
      .split(/[,;\n]+/)
      .map(limpiar)
      .filter(Boolean)
  );
}
