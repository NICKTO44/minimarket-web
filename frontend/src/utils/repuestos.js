// Lubricentro / Repuestos: módulos "Ficha del repuesto" y "Compatibilidad
// con vehículos". Las reglas deben coincidir con
// backend/src/handlers/repuestos.rs y backend/src/handlers/compatibilidad.rs.
import { textoPlano } from './medicamentos';

/** "90915-YZZE1" -> "90915YZZE1": solo letras y números, en mayúsculas. */
export function claveCodigo(codigo) {
  return textoPlano(codigo).replace(/[^a-z0-9ñ]/g, '').toUpperCase();
}

/** Lista del servidor -> Map producto_id -> ficha (o compatibilidades). */
export function mapaPorProducto(lista) {
  return new Map((lista || []).map((f) => [f.producto_id, f]));
}

/** Compatibilidades del servidor -> Map producto_id -> [filas]. */
export function mapaCompatibilidades(lista) {
  const mapa = new Map();
  for (const c of lista || []) {
    if (!mapa.has(c.producto_id)) mapa.set(c.producto_id, []);
    mapa.get(c.producto_id).push(c);
  }
  return mapa;
}

/** Todas las claves de código de una ficha: su n.° de parte y sus alternos. */
export function clavesDeFicha(ficha) {
  if (!ficha) return [];
  const claves = (ficha.codigos || []).map((c) => claveCodigo(c.codigo));
  if (ficha.numero_parte) claves.push(claveCodigo(ficha.numero_parte));
  return [...new Set(claves.filter((c) => c.length >= 2))];
}

/** clave de código -> [producto_id] (para buscar al instante por código). */
export function indiceCodigos(fichas) {
  const indice = new Map();
  for (const [id, ficha] of fichas) {
    for (const clave of clavesDeFicha(ficha)) {
      if (!indice.has(clave)) indice.set(clave, []);
      indice.get(clave).push(id);
    }
  }
  return indice;
}

/**
 * ¿La búsqueda coincide con la ficha? `consultaPlana` ya en textoPlano. Por
 * marca o ubicación (contiene) y por código (contiene, sin guiones ni
 * espacios: "w712" encuentra "W 712/75").
 */
export function fichaRepuestoCoincide(ficha, consultaPlana) {
  if (!ficha || !consultaPlana) return false;
  if (textoPlano(ficha.marca).includes(consultaPlana) || textoPlano(ficha.ubicacion).includes(consultaPlana)) return true;
  const clave = claveCodigo(consultaPlana);
  if (clave.length < 2) return false;
  return clavesDeFicha(ficha).some((c) => c.includes(clave));
}

/** "Bosch · N.° 0986AF1042 · Estante 3" */
export function resumenRepuesto(ficha) {
  if (!ficha) return '';
  return [ficha.marca, ficha.numero_parte ? `N.° ${ficha.numero_parte}` : null, ficha.ubicacion ? `📍 ${ficha.ubicacion}` : null]
    .filter(Boolean)
    .join(' · ');
}

/** "30 días", "3 meses", "1 año" */
export function textoGarantia(dias) {
  const d = Number(dias) || 0;
  if (d <= 0) return '';
  if (d % 365 === 0) return d === 365 ? '1 año' : `${d / 365} años`;
  if (d % 30 === 0) return d === 30 ? '1 mes' : `${d / 30} meses`;
  return d === 1 ? '1 día' : `${d} días`;
}

/**
 * Equivalentes de un repuesto: los productos que comparten con él un código
 * (su n.° de parte o uno de sus alternos), también en cadena: si el Fram es
 * equivalente al Toyota y el Toyota al Wix, el Wix también lo es del Fram.
 * Primero los que tienen stock, del más barato al más caro.
 */
export function equivalentesDe(productoId, fichas, productos, disponible = (p) => (p.controla_stock === false ? Infinity : p.stock)) {
  if (clavesDeFicha(fichas.get(productoId)).length === 0) return [];
  const indice = indiceCodigos(fichas);
  const grupo = new Set([productoId]);
  const vistas = new Set();
  const pendientes = [productoId];
  while (pendientes.length > 0) {
    for (const clave of clavesDeFicha(fichas.get(pendientes.pop()))) {
      if (vistas.has(clave)) continue;
      vistas.add(clave);
      for (const id of indice.get(clave) || []) {
        if (!grupo.has(id)) {
          grupo.add(id);
          pendientes.push(id);
        }
      }
    }
  }
  return productos
    .filter((p) => p.id !== productoId && !p.agotado && grupo.has(p.id))
    .map((p) => ({ producto: p, ficha: fichas.get(p.id), conStock: disponible(p) > 0 }))
    .sort((a, b) => Number(b.conStock) - Number(a.conStock) || a.producto.precio - b.producto.precio);
}

export const FICHA_REPUESTO_VACIA = {
  marca: '',
  numero_parte: '',
  ubicacion: '',
  garantia_dias: '',
  codigos: '',
};

/** Ficha del servidor (o undefined) -> campos del formulario. */
export function repuestoAFormulario(ficha) {
  if (!ficha) return { ...FICHA_REPUESTO_VACIA };
  return {
    marca: ficha.marca || '',
    numero_parte: ficha.numero_parte || '',
    ubicacion: ficha.ubicacion || '',
    garantia_dias: ficha.garantia_dias ? String(ficha.garantia_dias) : '',
    codigos: (ficha.codigos || []).map((c) => (c.fabricante ? `${c.codigo} (${c.fabricante})` : c.codigo)).join(', '),
  };
}

/** "PH3614, W712/75 (Mann)" -> [{ codigo, fabricante }] (como el servidor). */
export function codigosDesdeTexto(texto) {
  return String(texto || '')
    .split(/[,;|\n]/)
    .map((t) => t.trim())
    .filter(Boolean)
    .map((t) => {
      const m = t.match(/^(.+?)\s*\(([^)]*)\)$/);
      return m ? { codigo: m[1].trim(), fabricante: m[2].trim() || null } : { codigo: t, fabricante: null };
    });
}

/**
 * Campos del formulario -> { ficha } para guardar, { quitar: true } si quedó
 * vacía, o { error } con un mensaje para el usuario.
 */
export function formularioARepuesto(campos) {
  const garantiaTexto = String(campos.garantia_dias || '').trim();
  const garantia = garantiaTexto === '' ? null : Number(garantiaTexto);
  if (garantia !== null && (!Number.isInteger(garantia) || garantia < 0 || garantia > 3650)) {
    return { error: 'Garantía: escribe los días, de 1 a 3650 (o déjala vacía).' };
  }
  const ficha = {
    marca: campos.marca.trim() || null,
    numero_parte: campos.numero_parte.trim().toUpperCase() || null,
    ubicacion: campos.ubicacion.trim() || null,
    garantia_dias: garantia || null,
    codigos: codigosDesdeTexto(campos.codigos),
  };
  if (ficha.numero_parte && claveCodigo(ficha.numero_parte).length < 2) {
    return { error: 'El n.° de parte debe tener al menos 2 letras o números.' };
  }
  const malo = ficha.codigos.find((c) => claveCodigo(c.codigo).length < 2);
  if (malo) return { error: `El código alterno "${malo.codigo}" debe tener al menos 2 letras o números.` };
  if (ficha.codigos.length > 30) return { error: 'Máximo 30 códigos alternos por producto.' };
  const vacia = !ficha.marca && !ficha.numero_parte && !ficha.ubicacion && !ficha.garantia_dias && ficha.codigos.length === 0;
  return vacia ? { quitar: true } : { ficha };
}

// ---------------------------------------------------------------------
// Compatibilidad con vehículos
// ---------------------------------------------------------------------

/** "Toyota Hilux 2005–2015 · 2.5 D-4D" */
export function textoCompatibilidad(c) {
  let anios = '';
  if (c.anio_desde && c.anio_hasta) anios = c.anio_desde === c.anio_hasta ? `${c.anio_desde}` : `${c.anio_desde}–${c.anio_hasta}`;
  else if (c.anio_desde) anios = `${c.anio_desde} en adelante`;
  const principal = [c.marca, c.modelo, anios].filter(Boolean).join(' ');
  return [principal, c.motor, c.nota].filter(Boolean).join(' · ');
}

/** ¿La fila sirve para ese vehículo? (marca y modelo en textoPlano; año número o null). */
export function compatibleCon(c, { marca, modelo, anio }) {
  if (marca && textoPlano(c.marca) !== marca) return false;
  if (modelo) {
    // Sin modelo en la fila = sirve para toda la marca.
    if (c.modelo && !textoPlano(c.modelo).includes(modelo) && !modelo.includes(textoPlano(c.modelo))) return false;
  }
  if (anio) {
    if (c.anio_desde && anio < c.anio_desde) return false;
    if (c.anio_hasta && anio > c.anio_hasta) return false;
  }
  return true;
}

/** ¿El texto buscado aparece en alguna compatibilidad ("hilux", "frontier 2012")? */
export function compatibilidadCoincide(lista, consultaPlana) {
  if (!lista?.length || !consultaPlana) return false;
  const palabras = consultaPlana.split(/\s+/).filter(Boolean);
  return lista.some((c) => {
    const texto = textoPlano(`${c.marca} ${c.modelo || ''} ${c.motor || ''}`);
    return palabras.every((p) => {
      if (/^\d{4}$/.test(p)) {
        const a = Number(p);
        return (!c.anio_desde || a >= c.anio_desde) && (!c.anio_hasta || a <= c.anio_hasta) && (c.anio_desde || c.anio_hasta);
      }
      return texto.includes(p);
    });
  });
}

/** Marcas y modelos (de lo cargado), para sugerir en el buscador por vehículo. */
export function marcasYModelos(compatibilidades) {
  const marcas = new Map();
  for (const lista of compatibilidades.values()) {
    for (const c of lista) {
      const clave = textoPlano(c.marca);
      if (!marcas.has(clave)) marcas.set(clave, { marca: c.marca, modelos: new Map() });
      if (c.modelo) marcas.get(clave).modelos.set(textoPlano(c.modelo), c.modelo);
    }
  }
  return [...marcas.values()]
    .map((m) => ({ marca: m.marca, modelos: [...m.modelos.values()].sort((a, b) => a.localeCompare(b, 'es')) }))
    .sort((a, b) => a.marca.localeCompare(b.marca, 'es'));
}

/** Fila vacía del editor de compatibilidades. */
export const COMPATIBILIDAD_VACIA = { marca: '', modelo: '', anio_desde: '', anio_hasta: '', motor: '', nota: '' };

/** Del servidor -> filas del editor. */
export function compatibilidadesAFormulario(lista) {
  return (lista || []).map((c) => ({
    marca: c.marca || '',
    modelo: c.modelo || '',
    anio_desde: c.anio_desde ? String(c.anio_desde) : '',
    anio_hasta: c.anio_hasta ? String(c.anio_hasta) : '',
    motor: c.motor || '',
    nota: c.nota || '',
  }));
}

/** Filas del editor -> { lista } para guardar o { error }. Las filas sin nada escrito se ignoran. */
export function formularioACompatibilidades(filas) {
  const lista = [];
  const maximo = new Date().getFullYear() + 2;
  for (const f of filas) {
    const vacia = !f.marca.trim() && !f.modelo.trim() && !f.anio_desde && !f.anio_hasta && !f.motor.trim() && !f.nota.trim();
    if (vacia) continue;
    if (!f.marca.trim()) return { error: 'Cada vehículo compatible necesita su marca (Toyota, Honda...).' };
    const anio = (t) => (String(t).trim() === '' ? null : Number(t));
    const desde = anio(f.anio_desde);
    const hasta = anio(f.anio_hasta);
    for (const a of [desde, hasta]) {
      if (a !== null && (!Number.isInteger(a) || a < 1950 || a > maximo)) {
        return { error: `Año no válido en ${f.marca} ${f.modelo}: escribe años de 1950 a ${maximo}.` };
      }
    }
    lista.push({
      marca: f.marca.trim(),
      modelo: f.modelo.trim() || null,
      anio_desde: desde,
      anio_hasta: hasta,
      motor: f.motor.trim() || null,
      nota: f.nota.trim() || null,
    });
  }
  if (lista.length > 80) return { error: 'Máximo 80 vehículos por producto.' };
  return { lista };
}
