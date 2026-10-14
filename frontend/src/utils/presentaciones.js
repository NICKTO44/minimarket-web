// Presentaciones y precio por mayor (módulos PRESENTACIONES y PRECIO_MAYOR).
// Mismo cálculo que el backend (handlers/presentaciones.rs): si cambia uno,
// hay que cambiar el otro.
//
// Una presentación ("Rollo x 100 m" de un cable que se vende por metro)
// entra al carrito en la unidad del producto: 1 rollo = 100 m a S/ 85 / 100
// = S/ 0.85 el metro (6 decimales, como la media docena). Así el stock, las
// devoluciones, la caja y SUNAT siguen en metros.

/** Precio por unidad del producto de una presentación (6 decimales). */
export const precioUnitarioDe = (precioPresentacion, factor) => Math.round((precioPresentacion / factor) * 1e6) / 1e6;

/** "Rollo x 100 m", 2 -> "2 × Rollo x 100 m" (igual que lo guarda el servidor). */
export const detallePresentacion = (nombre, cantidad) => `${cantidad} × ${nombre}`;

/**
 * Respuesta de GET /productos/opciones-venta -> Map producto_id ->
 * { presentaciones: [...], mayor: [{ desde, precio }] (de menor a mayor) }.
 */
export function agruparOpciones(respuesta) {
  const mapa = new Map();
  const de = (id) => {
    if (!mapa.has(id)) mapa.set(id, { presentaciones: [], mayor: [] });
    return mapa.get(id);
  };
  for (const p of respuesta?.presentaciones || []) de(p.producto_id).presentaciones.push(p);
  for (const m of respuesta?.precios_mayor || []) de(m.producto_id).mayor.push({ desde: m.desde, precio: m.precio });
  for (const v of mapa.values()) v.mayor.sort((a, b) => a.desde - b.desde);
  return mapa;
}

/** De las escalas que alcanza esa cantidad, la de menor precio (o null). */
export function escalaPara(escalas, cantidad) {
  let elegida = null;
  for (const e of escalas || []) {
    if (cantidad + 1e-9 >= e.desde && (!elegida || e.precio < elegida.precio)) elegida = e;
  }
  return elegida;
}

/**
 * Escalas de precio por volumen de un producto vendido suelto:
 * - las de "Precio por mayor" (si ese módulo está encendido);
 * - cada presentación cuyo precio por unidad es menor al normal (si el
 *   módulo de presentaciones está encendido): quien lleva 150 m de un cable
 *   con "Rollo x 100 m a S/ 85" paga el metro a S/ 0.85, como el rollo.
 * El servidor acepta las mismas (precio_por_volumen_valido).
 */
export function escalasDe(opcion, precioNormal, { mayor = true, presentaciones = true } = {}) {
  if (!opcion) return [];
  const escalas = mayor ? opcion.mayor.map((m) => ({ ...m, origen: 'mayor' })) : [];
  if (presentaciones) {
    for (const p of opcion.presentaciones) {
      const precio = precioUnitarioDe(p.precio, p.factor);
      if (precio < precioNormal - 1e-9) escalas.push({ desde: p.factor, precio, origen: p.nombre });
    }
  }
  return escalas.sort((a, b) => a.desde - b.desde || a.precio - b.precio);
}

// ===== Formulario del producto (components/OpcionesDeVenta.jsx) =====

/** Lo que guarda el servidor -> filas del formulario (texto). */
export function opcionesAFormulario(respuesta) {
  return {
    presentaciones: (respuesta?.presentaciones || []).map((p) => ({ nombre: p.nombre, factor: String(p.factor), precio: String(p.precio) })),
    precios_mayor: (respuesta?.precios_mayor || []).map((m) => ({ desde: String(m.desde), precio: String(m.precio) })),
  };
}

export const OPCIONES_VACIAS = { presentaciones: [], precios_mayor: [] };

export const numeroDeTexto = (texto) => {
  const n = parseFloat(String(texto ?? '').trim().replace(',', '.'));
  return Number.isFinite(n) ? n : NaN;
};
const vacia = (fila) => Object.values(fila).every((v) => !String(v ?? '').trim());

/**
 * Filas del formulario -> lo que recibe PUT /productos/:id/presentaciones.
 * Las filas vacías se ignoran; una a medias da { error }.
 */
export function formularioAOpciones(opciones) {
  const presentaciones = [];
  for (const f of opciones.presentaciones.filter((x) => !vacia(x))) {
    const factor = numeroDeTexto(f.factor);
    const precio = numeroDeTexto(f.precio);
    if (!f.nombre.trim() || !(factor > 0) || !(precio > 0)) {
      return { error: 'Completa cada presentación: nombre, cuánto lleva y su precio (o bórrala).' };
    }
    presentaciones.push({ nombre: f.nombre.trim(), factor, precio });
  }
  const precios_mayor = [];
  for (const f of opciones.precios_mayor.filter((x) => !vacia(x))) {
    const desde = numeroDeTexto(f.desde);
    const precio = numeroDeTexto(f.precio);
    if (!(desde > 0) || !(precio > 0)) {
      return { error: 'Completa cada precio por mayor: desde qué cantidad y el precio (o bórralo).' };
    }
    precios_mayor.push({ desde, precio });
  }
  return { presentaciones, precios_mayor };
}

