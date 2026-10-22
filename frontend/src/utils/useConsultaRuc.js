import { useEffect, useRef, useState } from 'react';
import { api } from '../api/api';
import { errorFormatoRuc } from './validacionRegistro';

/**
 * Consulta en SUNAT (vía Factiliza) un RUC con formato válido.
 * Devuelve { estado: 'inactivo' | 'consultando' | 'listo', resultado, mensaje, datos }.
 * `activo = false` no consulta (por ejemplo, cuando el RUC es el ya guardado).
 * `alEncontrar(datos)` se llama cuando el RUC existe, para autocompletar.
 */
export function useConsultaRuc(ruc, activo = true, alEncontrar) {
  const [respuesta, setRespuesta] = useState(null);
  const actual = useRef(0);
  const callback = useRef(alEncontrar);
  useEffect(() => {
    callback.current = alEncontrar;
  });
  const conFormato = activo && ruc.length === 11 && !errorFormatoRuc(ruc);

  useEffect(() => {
    const id = ++actual.current;
    if (!conFormato) return;
    const t = setTimeout(async () => {
      const r = await api.consultarRucRegistro(ruc);
      if (actual.current !== id) return;
      setRespuesta({ ruc, ...r });
      if (r.resultado === 'ENCONTRADO' && r.datos) callback.current?.(r.datos);
    }, 300);
    return () => clearTimeout(t);
  }, [ruc, conFormato]);

  if (!conFormato) return { estado: 'inactivo' };
  return respuesta?.ruc === ruc ? { estado: 'listo', ...respuesta } : { estado: 'consultando' };
}
