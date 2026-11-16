// Ajuste de inventario (módulo Ajustes y Kárdex): comparar lo contado con lo
// que dice el sistema. Sirve tanto para el ajuste a mano como para el Excel
// que sale de Productos → Exportar (con la columna Stock cambiada por lo
// contado, o con una columna "Stock contado").
import { leerNumero, normalizar } from './importacion';

export const MOTIVOS_AJUSTE = ['Conteo físico', 'Pérdida', 'Rotura o daño', 'Vencido', 'Error de registro', 'Consumo interno', 'Otro'];

const sinCeros = (n) => Math.round(n * 1000) / 1000;

/** producto_id -> lotes vigentes ({ lote_id, numero_lote, fecha_vencimiento, cantidad }). */
export function lotesPorProducto(lotes) {
  const mapa = new Map();
  for (const l of lotes || []) {
    if (!mapa.has(l.producto_id)) mapa.set(l.producto_id, []);
    mapa.get(l.producto_id).push(l);
  }
  return mapa;
}

/** "31/12/2027", "2027-12-31", "12/2027" o un número de fecha de Excel -> "2027-12-31" (o el mes "2027-12"). */
function fechaIso(valor) {
  const t = String(valor ?? '').trim();
  if (!t) return '';
  if (/^\d{4}-\d{2}-\d{2}/.test(t)) return t.slice(0, 10);
  let m = t.match(/^(\d{1,2})[/-](\d{1,2})[/-](\d{4})$/);
  if (m) return `${m[3]}-${m[2].padStart(2, '0')}-${m[1].padStart(2, '0')}`;
  m = t.match(/^(\d{1,2})[/-](\d{4})$/);
  if (m) return `${m[2]}-${m[1].padStart(2, '0')}`;
  if (/^\d{5}(\.\d+)?$/.test(t)) {
    const d = new Date(Date.UTC(1899, 11, 30) + Math.round(Number(t)) * 86400000);
    return d.toISOString().slice(0, 10);
  }
  return '';
}

/** Busca en qué columna está cada dato. Prefiere "Stock contado" a "Stock". */
export function columnasDeConteo(titulos) {
  const n = (titulos || []).map((t) => normalizar(t));
  const buscar = (prueba) => n.findIndex(prueba);
  const contado = buscar((t) => ['stockcontado', 'contado', 'conteo', 'stockfisico', 'cantidadcontada', 'fisico'].includes(t));
  return {
    codigo: buscar((t) => t === 'codigo' || t === 'cod' || t === 'codigodebarras' || t === 'sku'),
    stock: contado >= 0 ? contado : buscar((t) => t === 'stock' || t === 'stockactual' || t === 'cantidad'),
    lote: buscar((t) => t === 'lote' || t === 'numerodelote' || t === 'nlote' || t === 'nodelote'),
    vencimiento: buscar((t) => t.startsWith('vencimiento') || t === 'fechadevencimiento'),
    usaContado: contado >= 0,
  };
}

/**
 * Lee las filas del Excel y arma las líneas de ajuste. Devuelve
 * { lineas, sinCambio, desconocidos, sinLote, error }.
 *  - lineas: solo las que cambian ({ producto, lote, sistema, contado }).
 *  - desconocidos: códigos que no están en el inventario.
 *  - sinLote: filas de productos con vencimiento cuyo lote no se encontró.
 */
export function leerConteo(filas, productos, lotes) {
  const iTitulos = (filas || []).findIndex((f) => {
    const c = columnasDeConteo(f);
    return c.codigo >= 0 && c.stock >= 0;
  });
  if (iTitulos < 0) {
    return { error: 'No se encontraron las columnas "Código" y "Stock". Usa el Excel que sale de Productos → ⬇ Exportar.' };
  }
  const col = columnasDeConteo(filas[iTitulos]);
  const porCodigo = new Map(productos.map((p) => [normalizar(p.codigo), p]).filter(([c]) => c));
  const porProductoLotes = lotesPorProducto(lotes);

  const lineas = [];
  const desconocidos = [];
  const sinLote = [];
  let sinCambio = 0;
  const vistos = new Set();
  for (const fila of filas.slice(iTitulos + 1)) {
    const codigo = String(fila[col.codigo] ?? '').trim();
    const contado = leerNumero(fila[col.stock]);
    if (!codigo || contado == null) continue;
    const p = porCodigo.get(normalizar(codigo));
    if (!p) {
      desconocidos.push(codigo);
      continue;
    }
    if (p.controla_stock === false) continue;
    const valor = Math.max(0, sinCeros(contado));
    if (p.lleva_vencimiento) {
      const lotesP = porProductoLotes.get(p.id) || [];
      const numero = col.lote >= 0 ? normalizar(fila[col.lote]) : '';
      const venc = col.vencimiento >= 0 ? fechaIso(fila[col.vencimiento]) : '';
      const lote =
        (numero && lotesP.find((l) => normalizar(l.numero_lote) === numero)) ||
        (!numero && venc && lotesP.find((l) => String(l.fecha_vencimiento).startsWith(venc))) ||
        (!numero && !venc && lotesP.length === 1 ? lotesP[0] : null);
      if (!lote) {
        sinLote.push(`${p.nombre}${numero ? ` (lote ${fila[col.lote]})` : ''}`);
        continue;
      }
      if (vistos.has(`L${lote.lote_id}`)) continue;
      vistos.add(`L${lote.lote_id}`);
      if (Math.abs(valor - lote.cantidad) < 1e-6) sinCambio += 1;
      else lineas.push({ producto: p, lote, sistema: lote.cantidad, contado: valor });
    } else {
      if (vistos.has(`P${p.id}`)) continue;
      vistos.add(`P${p.id}`);
      const sistema = Number(p.stock) || 0;
      if (Math.abs(valor - sistema) < 1e-6) sinCambio += 1;
      else lineas.push({ producto: p, lote: null, sistema, contado: valor });
    }
  }
  return { lineas, sinCambio, desconocidos, sinLote, usaContado: col.usaContado };
}

/** Lo que se manda al servidor. */
export function cuerpoAjuste(lineas, motivo) {
  return {
    motivo,
    lineas: lineas.map((l) => ({
      producto_id: l.producto.id,
      lote_id: l.lote ? l.lote.lote_id : null,
      stock_contado: l.contado,
      motivo: l.motivo || null,
    })),
  };
}

/** Texto de una diferencia: "+3" / "−2.5". */
export const textoDiferencia = (d) => (d > 0 ? `+${sinCeros(d)}` : `−${sinCeros(-d)}`);
