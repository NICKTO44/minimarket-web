/**
 * Pone `nuevo` en un campo solo si está vacío o todavía tiene la sugerencia
 * anterior: si el usuario escribió algo suyo, se respeta.
 *
 * `anotadas` es un useRef({ clave: texto }) con la última sugerencia de cada
 * campo. Se anota FUERA de la función que se pasa a `setter`, porque React
 * puede llamar esa función dos veces (modo estricto en desarrollo) y tiene
 * que dar el mismo resultado las dos.
 */
export function sugerirEn(setter, anotadas, clave, nuevo) {
  if (!nuevo) return;
  const previa = (anotadas.current[clave] || '').trim();
  anotadas.current[clave] = nuevo;
  setter((actual) => {
    const escrito = (actual || '').trim();
    return escrito && escrito !== previa ? actual : nuevo;
  });
}
