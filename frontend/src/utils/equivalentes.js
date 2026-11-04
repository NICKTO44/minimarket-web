// Módulo "Equivalentes" (repuestos): el dueño elige a mano qué productos son
// equivalentes. Forman un grupo: todos los del grupo se ven entre sí, en los
// dos sentidos (backend/src/handlers/equivalentes.rs).
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/api';
import { coincidenTodas, fichaRepuestoCoincide, palabrasDeBusqueda, resumenRepuesto } from './repuestos';
import { textoPlano } from './medicamentos';

/** [{ producto_id, grupo }] -> Map producto_id -> [ids de sus equivalentes]. */
export function mapaEquivalentes(lista) {
  const grupos = new Map();
  for (const { producto_id: id, grupo } of lista || []) {
    if (!grupos.has(grupo)) grupos.set(grupo, []);
    grupos.get(grupo).push(id);
  }
  const mapa = new Map();
  for (const ids of grupos.values()) {
    if (ids.length < 2) continue;
    for (const id of ids) mapa.set(id, ids.filter((x) => x !== id));
  }
  return mapa;
}

/**
 * Los equivalentes de un producto con lo que queda para vender: primero los
 * que tienen stock, del más barato al más caro. Mismo formato que los
 * equivalentes por código (EquivalentesRepuesto).
 */
export function equivalentesElegidos(id, mapa, productos, disponible = (p) => (p.controla_stock === false ? Infinity : p.stock)) {
  const ids = new Set(mapa.get(id) || []);
  if (ids.size === 0) return [];
  return productos
    .filter((p) => ids.has(p.id) && !p.agotado)
    .map((p) => ({ producto: p, conStock: disponible(p) > 0 }))
    .sort((a, b) => Number(b.conStock) - Number(a.conStock) || a.producto.precio - b.producto.precio);
}

/**
 * Busca productos para agregarlos como equivalentes: por nombre, código,
 * aplicación, marca o n.° de parte, en cualquier orden ("sail", "trapecio
 * aveo", "716CV01B200"). `excluir`: ids que no se muestran.
 */
export function buscarParaEquivalentes(productos, consulta, repuestos, excluir = new Set(), maximo = 8) {
  const q = String(consulta || '').trim().toLowerCase();
  if (q.length < 2) return [];
  const plano = textoPlano(q);
  const palabras = palabrasDeBusqueda(q);
  const encontrados = [];
  for (const p of productos) {
    if (excluir.has(p.id)) continue;
    const ficha = repuestos?.get(p.id);
    if (
      p.nombre.toLowerCase().includes(q) ||
      String(p.codigo || '').toLowerCase().includes(q) ||
      fichaRepuestoCoincide(ficha, plano) ||
      coincidenTodas(palabras, p.nombre, p.codigo, p.descripcion, resumenRepuesto(ficha))
    ) {
      encontrados.push(p);
      if (encontrados.length >= maximo) break;
    }
  }
  return encontrados;
}

/**
 * Después de importar productos: la columna "Equivalentes" (códigos separados
 * por coma). SOLO AGREGA: lo que el producto ya tenía se mantiene; para quitar
 * uno se edita el producto. Devuelve { actualizados, noEncontrados, errores }
 * o null si el archivo no traía equivalentes.
 */
export async function aplicarEquivalentesImportados(filas) {
  const conTexto = (filas || []).filter((f) => f.equivalentes && f.codigo);
  if (conTexto.length === 0) return null;
  const [productos, lista] = await Promise.all([api.productos(), api.equivalentes()]);
  const porCodigo = new Map(productos.map((p) => [String(p.codigo || '').trim().toLowerCase(), p.id]));
  let mapa = mapaEquivalentes(lista);
  let actualizados = 0;
  let errores = 0;
  const noEncontrados = new Set();
  for (const f of conTexto) {
    const id = porCodigo.get(String(f.codigo).trim().toLowerCase());
    if (!id) continue;
    const ids = String(f.equivalentes)
      .split(/[,;|\n]/)
      .map((c) => c.trim())
      .filter(Boolean)
      .map((c) => {
        const x = porCodigo.get(c.toLowerCase());
        if (!x) noEncontrados.add(c);
        return x;
      })
      .filter((x) => x && x !== id);
    const actuales = mapa.get(id) || [];
    const nuevos = [...new Set(ids)].filter((x) => !actuales.includes(x));
    if (nuevos.length === 0) continue;
    try {
      mapa = mapaEquivalentes(await api.equivalentesGuardar(id, [...actuales, ...nuevos]));
      actualizados += 1;
    } catch {
      errores += 1;
    }
  }
  return { actualizados, noEncontrados: [...noEncontrados], errores };
}

/** Los grupos de equivalentes del negocio (solo con el módulo). */
export function useEquivalentes(activo) {
  const [mapa, setMapa] = useState(() => new Map());
  const [vuelta, setVuelta] = useState(0);
  useEffect(() => {
    if (!activo) return undefined;
    let vivo = true;
    api
      .equivalentes()
      .then((lista) => vivo && setMapa(mapaEquivalentes(lista)))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [activo, vuelta]);
  const recargar = useCallback(() => setVuelta((v) => v + 1), []);
  return [mapa, setMapa, recargar];
}

/**
 * Formulario de producto: los equivalentes elegidos (ids), cómo estaban al
 * abrirlo (para guardar solo si cambiaron) y el guardado después del producto.
 */
export function useFormEquivalentes(activo) {
  const [mapa, setMapa, recargar] = useEquivalentes(activo);
  const [elegidos, setElegidos] = useState([]);
  const [original, setOriginal] = useState('[]');

  /** Al abrir el formulario: un producto (id) o uno nuevo (null). */
  const abrir = useCallback(
    (id) => {
      const ids = id != null ? [...(mapa.get(id) || [])] : [];
      setElegidos(ids);
      setOriginal(JSON.stringify([...ids].sort((a, b) => a - b)));
    },
    [mapa]
  );

  /** Agrega un producto y, si ya tenía equivalentes, también los suyos (son el mismo grupo). */
  const agregar = (id, propio) =>
    setElegidos((prev) => {
      const nuevos = [id, ...(mapa.get(id) || [])].filter((x) => x !== propio && !prev.includes(x));
      return [...prev, ...nuevos];
    });
  const quitar = (id) => setElegidos((prev) => prev.filter((x) => x !== id));

  const cambiaron = () => activo && JSON.stringify([...elegidos].sort((a, b) => a - b)) !== original;

  /** Guarda después de crear o actualizar el producto. */
  const guardar = async (id) => {
    const lista = await api.equivalentesGuardar(id, elegidos.filter((x) => x !== id));
    setMapa(mapaEquivalentes(lista));
    setOriginal(JSON.stringify([...elegidos].sort((a, b) => a - b)));
  };

  return { activo, mapa, elegidos, abrir, agregar, quitar, cambiaron, guardar, recargar };
}
