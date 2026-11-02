// Librería: módulo "Ficha del libro" (autor, editorial, serie, año y
// edición). Las reglas deben coincidir con backend/src/handlers/libros.rs.
// El ISBN va como código del producto (se escanea como cualquier código).
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/api';
import { mapaPorProducto } from './repuestos';

export const ANIO_MINIMO = 1450;
const anioMaximo = () => new Date().getFullYear() + 1;

export const FICHA_LIBRO_VACIA = { autor: '', editorial: '', serie: '', anio: '', edicion: '' };

/** "Mario Vargas Llosa · Alfaguara · Serie Harry Potter · 2010" */
export function resumenLibro(ficha) {
  if (!ficha) return '';
  return [ficha.autor, ficha.editorial, ficha.serie ? `Serie ${ficha.serie}` : null, ficha.anio, ficha.edicion]
    .filter(Boolean)
    .join(' · ');
}

/** Todo lo buscable de la ficha, en un texto (para la búsqueda por palabras). */
export function textoLibro(ficha) {
  if (!ficha) return '';
  return [ficha.autor, ficha.editorial, ficha.serie, ficha.anio, ficha.edicion].filter(Boolean).join(' ');
}

/** Los valores distintos de un campo ("editorial", "autor"), ordenados, para filtros y sugerencias. */
export function valoresDe(mapa, campo) {
  const vistos = new Map();
  for (const f of mapa.values()) {
    const v = String(f?.[campo] || '').trim();
    if (v && !vistos.has(v.toLowerCase())) vistos.set(v.toLowerCase(), v);
  }
  return [...vistos.values()].sort((a, b) => a.localeCompare(b, 'es'));
}

/** ¿La ficha es de esa editorial? (sin importar mayúsculas ni espacios) */
export const deEditorial = (ficha, editorial) =>
  !!ficha?.editorial && ficha.editorial.trim().toLowerCase() === String(editorial).trim().toLowerCase();

/** Ficha del servidor (o undefined) -> campos del formulario. */
export function libroAFormulario(ficha) {
  if (!ficha) return { ...FICHA_LIBRO_VACIA };
  return {
    autor: ficha.autor || '',
    editorial: ficha.editorial || '',
    serie: ficha.serie || '',
    anio: ficha.anio ? String(ficha.anio) : '',
    edicion: ficha.edicion || '',
  };
}

const limpio = (t) => String(t || '').trim().replace(/\s+/g, ' ');

/**
 * Campos del formulario -> { ficha } para guardar, { quitar: true } si quedó
 * vacía, o { error } con un mensaje para el usuario.
 */
export function formularioALibro(campos) {
  const anioTexto = limpio(campos.anio);
  const anio = anioTexto === '' ? null : Number(anioTexto);
  if (anio !== null && (!Number.isInteger(anio) || anio < ANIO_MINIMO || anio > anioMaximo())) {
    return { error: `Año: escribe un año entre ${ANIO_MINIMO} y ${anioMaximo()} (o déjalo vacío).` };
  }
  const ficha = {
    autor: limpio(campos.autor) || null,
    editorial: limpio(campos.editorial) || null,
    serie: limpio(campos.serie) || null,
    anio,
    edicion: limpio(campos.edicion) || null,
  };
  const largos = [
    ['autor', 150, 'Autor'],
    ['editorial', 80, 'Editorial'],
    ['serie', 80, 'Serie o colección'],
    ['edicion', 40, 'Edición'],
  ];
  for (const [campo, max, nombre] of largos) {
    if (ficha[campo] && ficha[campo].length > max) return { error: `${nombre}: máximo ${max} letras.` };
  }
  const vacia = !ficha.autor && !ficha.editorial && !ficha.serie && !ficha.anio && !ficha.edicion;
  return vacia ? { quitar: true } : { ficha };
}

/** Las fichas del negocio (Map producto_id -> ficha), solo con el módulo encendido. */
export function useFichasLibro(activo) {
  const [mapa, setMapa] = useState(() => new Map());
  // Sube para volver a leerlas (después de importar desde Excel).
  const [vuelta, setVuelta] = useState(0);
  useEffect(() => {
    if (!activo) return undefined;
    let vivo = true;
    api
      .librosFichas()
      .then((lista) => vivo && setMapa(mapaPorProducto(lista)))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [activo, vuelta]);
  const recargar = useCallback(() => setVuelta((v) => v + 1), []);
  return [mapa, setMapa, recargar];
}

/**
 * Formulario de producto: la ficha del libro, cómo estaba al abrirlo (para
 * guardar solo si cambió) y el guardado después del producto.
 */
export function useFormLibro(activo) {
  const [mapa, setMapa, recargar] = useFichasLibro(activo);
  const [campos, setCampos] = useState(FICHA_LIBRO_VACIA);
  const [original, setOriginal] = useState(JSON.stringify(FICHA_LIBRO_VACIA));

  /** Al abrir el formulario: un producto (id) o uno nuevo (null). */
  const abrir = useCallback(
    (id) => {
      const valores = libroAFormulario(id != null ? mapa.get(id) : null);
      setCampos(valores);
      setOriginal(JSON.stringify(valores));
    },
    [mapa]
  );

  /** null = nada que guardar; { error }, { quitar } o { ficha }. */
  const paraGuardar = () => {
    if (!activo || JSON.stringify(campos) === original) return null;
    return formularioALibro(campos);
  };

  /** Guarda después de crear o actualizar el producto. */
  const guardar = async (id, resultado) => {
    if (resultado.quitar) {
      if (mapa.has(id)) await api.libroQuitar(id);
      setMapa((prev) => {
        const nuevo = new Map(prev);
        nuevo.delete(id);
        return nuevo;
      });
    } else {
      const guardada = await api.libroGuardar(id, resultado.ficha);
      setMapa((prev) => new Map(prev).set(id, { ...guardada, producto_id: id }));
    }
    setOriginal(JSON.stringify(campos));
  };

  return { activo, mapa, campos, setCampos, abrir, paraGuardar, guardar, recargar };
}
