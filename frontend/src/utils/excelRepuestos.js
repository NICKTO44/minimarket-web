// Repuestos (módulo "Ficha del repuesto"): las columnas del Excel de
// productos. Las usan la plantilla de importar y "Exportar" de Productos, así
// el Excel exportado se edita y se vuelve a importar tal cual. Los títulos
// son los que el importador reconoce solo (utils/importacion.js).
import { etiquetaUnidad } from './unidades';

/** Títulos de las columnas, en orden. */
export function titulosRepuestos({ tresPrecios = false, equivalentes = false, extras = {} } = {}) {
  return [
    'Código',
    'Nombre',
    'Aplicación',
    'Marca',
    'N.° de parte',
    'Categoría',
    'Unidad',
    'Precio de compra',
    ...(tresPrecios ? ['Precio 1 (público)', 'Precio 2 (tienda)', 'Precio 3 (por mayor)'] : ['Precio de venta']),
    'Stock',
    'Stock mínimo',
    // Solo si algún producto tiene ese dato (ver columnasExtra).
    ...(extras.ubicacion ? ['Ubicación'] : []),
    ...(extras.garantia ? ['Garantía (días)'] : []),
    ...(extras.alternos ? ['Códigos alternos'] : []),
    // Módulo "Equivalentes": sus códigos separados por coma ("ATK-31002L, GEN-31002L").
    ...(equivalentes ? ['Equivalentes'] : []),
  ];
}

/** Anchos de columna (en caracteres) para los mismos títulos. */
export function anchosRepuestos({ tresPrecios = false, equivalentes = false, extras = {} } = {}) {
  return [
    14, 42, 38, 16, 16, 22, 10, 16, ...(tresPrecios ? [18, 17, 20] : [15]), 9, 13,
    ...(extras.ubicacion ? [16] : []),
    ...(extras.garantia ? [15] : []),
    ...(extras.alternos ? [30] : []),
    ...(equivalentes ? [34] : []),
  ];
}

/**
 * Columnas opcionales de la ficha: salen en el Excel solo si al menos un
 * producto tiene ese dato (así no aparecen columnas vacías que nadie usa).
 */
export function columnasExtra(fichas) {
  const lista = [...(fichas?.values() || [])];
  return {
    ubicacion: lista.some((f) => f?.ubicacion),
    garantia: lista.some((f) => f?.garantia_dias),
    alternos: lista.some((f) => f?.codigos?.length > 0),
  };
}

// "W712/75 (Mann), PH3614": como lo lee el importador.
const textoAlternos = (codigos) =>
  (codigos || []).map((c) => (c.fabricante ? `${c.codigo} (${c.fabricante})` : c.codigo)).join(', ');

const numero = (n) => (Number(n) > 0 ? Number(n) : '');

/**
 * Una fila del Excel para un producto. `ficha`: su ficha del repuesto;
 * `extra`: su Precio 2 / 3; `equivalentes`: códigos de sus equivalentes
 * (null = el negocio no tiene el módulo).
 */
export function filaRepuesto(p, { ficha, extra, tresPrecios = false, equivalentes = null, extras = {} } = {}) {
  return [
    String(p.codigo || ''),
    p.nombre,
    p.descripcion || '',
    ficha?.marca || '',
    ficha?.numero_parte || '',
    p.categoria_nombre || '',
    etiquetaUnidad(p.unidad_medida),
    numero(p.precio_compra),
    p.precio,
    ...(tresPrecios ? [numero(extra?.precio_2), numero(extra?.precio_3)] : []),
    p.controla_stock === false ? '' : p.stock,
    p.controla_stock === false ? '' : p.stock_minimo,
    ...(extras.ubicacion ? [ficha?.ubicacion || ''] : []),
    ...(extras.garantia ? [ficha?.garantia_dias || ''] : []),
    ...(extras.alternos ? [textoAlternos(ficha?.codigos)] : []),
    ...(equivalentes ? [equivalentes.join(', ')] : []),
  ];
}
