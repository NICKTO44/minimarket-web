import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/api';

// Módulo "3 precios de venta" (rubro Lubricentro / Repuestos). El Precio 1
// es el precio de venta del producto; el 2 y el 3 viven aparte
// (GET /precios-venta). Sin el módulo, nada de esto se carga.

export const PRECIOS_VACIOS = { precio_2: '', precio_3: '' };

/** [{ producto_id, precio_2, precio_3 }] -> Map(producto_id -> { precio_2, precio_3 }) */
export function mapaPrecios(lista) {
  const mapa = new Map();
  for (const p of lista || []) {
    if (p.precio_2 || p.precio_3) mapa.set(p.producto_id, { precio_2: p.precio_2 || null, precio_3: p.precio_3 || null });
  }
  return mapa;
}

/** Los precios de una línea para elegir: [{ numero, precio }], solo los que existen. */
export function opcionesDePrecio(precio1, extra) {
  if (!extra) return [];
  return [
    { numero: 1, precio: precio1 },
    { numero: 2, precio: extra.precio_2 },
    { numero: 3, precio: extra.precio_3 },
  ].filter((o) => Number(o.precio) > 0);
}

/** Texto del formulario -> número (vacío = sin ese precio). */
function leer(texto, nombre) {
  const t = String(texto ?? '').trim().replace(',', '.');
  if (!t) return { valor: null };
  const n = Number(t);
  if (!Number.isFinite(n) || n < 0) return { error: `El ${nombre} no es válido.` };
  return { valor: n === 0 ? null : Math.round(n * 100) / 100 };
}

/** Campos del formulario -> lo que se manda al servidor, o { error }. */
export function formularioAPrecios(campos) {
  const dos = leer(campos.precio_2, 'Precio 2');
  if (dos.error) return dos;
  const tres = leer(campos.precio_3, 'Precio 3');
  if (tres.error) return tres;
  return { precio_2: dos.valor, precio_3: tres.valor };
}

/**
 * Precios 2 y 3 de todos los productos, cargados una vez si el módulo está
 * encendido. `fijar(id, precios)` actualiza uno tras guardarlo.
 */
export function usePreciosVenta(activo) {
  const [mapa, setMapa] = useState(() => new Map());
  useEffect(() => {
    if (!activo) return undefined;
    let vivo = true;
    api
      .preciosVenta()
      .then((lista) => vivo && setMapa(mapaPrecios(lista)))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [activo]);
  const fijar = useCallback((id, precios) => {
    setMapa((prev) => {
      const nuevo = new Map(prev);
      if (precios.precio_2 || precios.precio_3) nuevo.set(id, { precio_2: precios.precio_2 || null, precio_3: precios.precio_3 || null });
      else nuevo.delete(id);
      return nuevo;
    });
  }, []);
  return { mapa, fijar };
}

/**
 * Formulario de producto: los campos Precio 2 y 3, cómo estaban al abrirlo
 * (para guardar solo si cambiaron) y el guardado después del producto.
 */
export function useFormPreciosVenta(activo) {
  const { mapa, fijar } = usePreciosVenta(activo);
  const [campos, setCampos] = useState(PRECIOS_VACIOS);
  const [original, setOriginal] = useState(JSON.stringify(PRECIOS_VACIOS));

  /** Al abrir el formulario: un producto (id) o uno nuevo (null). */
  const abrir = useCallback(
    (id) => {
      const p = id != null ? mapa.get(id) : null;
      const valores = p
        ? { precio_2: p.precio_2 != null ? String(p.precio_2) : '', precio_3: p.precio_3 != null ? String(p.precio_3) : '' }
        : PRECIOS_VACIOS;
      setCampos(valores);
      setOriginal(JSON.stringify(valores));
    },
    [mapa]
  );

  /** null = no hay nada que guardar; { error } o los precios a guardar. */
  const paraGuardar = () => {
    if (!activo || JSON.stringify(campos) === original) return null;
    return formularioAPrecios(campos);
  };

  /** Guarda después de crear o actualizar el producto. */
  const guardar = async (id, precios) => {
    await api.preciosVentaGuardar(id, precios);
    fijar(id, precios);
    setOriginal(JSON.stringify(campos));
  };

  return { activo, mapa, campos, setCampos, abrir, paraGuardar, guardar };
}
