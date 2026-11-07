import { useEffect, useRef } from 'react';
import { hoyLima } from './formato';

/**
 * Llama a `alCambiar` cuando empieza un día nuevo en Perú (12:00 a. m.)
 * mientras la pantalla sigue abierta, o al volver a ella después de
 * medianoche. Lo usan las pantallas de venta del restaurante: la carta del
 * día de ayer ya no se vende y hay que volver a leer los productos.
 */
export function useAlCambiarDeDia(alCambiar) {
  const dia = useRef(hoyLima());
  const funcion = useRef(alCambiar);
  useEffect(() => {
    funcion.current = alCambiar;
  });

  useEffect(() => {
    const revisar = () => {
      const hoy = hoyLima();
      if (hoy !== dia.current) {
        dia.current = hoy;
        funcion.current();
      }
    };
    const intervalo = setInterval(revisar, 60000);
    const alVolver = () => document.visibilityState === 'visible' && revisar();
    document.addEventListener('visibilitychange', alVolver);
    window.addEventListener('focus', revisar);
    return () => {
      clearInterval(intervalo);
      document.removeEventListener('visibilitychange', alVolver);
      window.removeEventListener('focus', revisar);
    };
  }, []);
}
