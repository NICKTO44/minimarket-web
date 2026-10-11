import { useEffect, useRef } from 'react';
import { irADireccion, leerDireccion, pantallaEnDireccion } from '../utils/direccion';

/**
 * Con sesión abierta, la barra de direcciones dice el negocio y la pantalla
 * (/verane/caja). Cada cambio de pantalla es un paso del historial, así el
 * botón "atrás" del celular o del navegador vuelve a la pantalla anterior.
 * No dibuja nada.
 */
export default function DireccionNegocio({ slug, pantalla, onCambiarPantalla }) {
  const primera = useRef(true);

  useEffect(() => {
    if (!slug) return;
    // La primera vez solo se corrige la dirección; después, cada pantalla
    // nueva se agrega al historial.
    irADireccion(`/${slug}/${pantallaEnDireccion(pantalla)}`, !primera.current);
    primera.current = false;
  }, [slug, pantalla]);

  useEffect(() => {
    const alVolver = () => {
      const { pantalla: anterior } = leerDireccion();
      if (anterior) onCambiarPantalla(anterior);
    };
    window.addEventListener('popstate', alVolver);
    return () => window.removeEventListener('popstate', alVolver);
  }, [onCambiarPantalla]);

  return null;
}
