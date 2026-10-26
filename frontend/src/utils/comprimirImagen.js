// Achica una foto en el navegador antes de subirla. Una foto de celular
// pesa 3-8 MB y el servidor igual la achica (ver logica/imagen.rs del
// backend): así se sube en un segundo con datos móviles y el servidor casi
// no trabaja, por pesada que sea la foto original.
// Si el navegador no puede leerla (p. ej. HEIC en algunos navegadores), se
// manda la original y el servidor decide.
//
// opciones:
//   ladoMaximo: lado más largo del resultado (1200 por defecto).
//   png: true para conservar la transparencia (logo del negocio); si no,
//        JPEG sobre fondo blanco.
const LADO_MAXIMO = 1200;
const CALIDAD = 0.85;

export async function comprimirImagen(archivo, { ladoMaximo = LADO_MAXIMO, png = false } = {}) {
  if (!archivo) return archivo;
  try {
    const bitmap = await createImageBitmap(archivo);
    const escala = Math.min(1, ladoMaximo / Math.max(bitmap.width, bitmap.height));
    const ancho = Math.max(1, Math.round(bitmap.width * escala));
    const alto = Math.max(1, Math.round(bitmap.height * escala));
    const lienzo = document.createElement('canvas');
    lienzo.width = ancho;
    lienzo.height = alto;
    const ctx = lienzo.getContext('2d');
    if (!png) {
      // Fondo blanco: un PNG con transparencia no queda negro al pasar a JPEG.
      ctx.fillStyle = '#fff';
      ctx.fillRect(0, 0, ancho, alto);
    }
    ctx.drawImage(bitmap, 0, 0, ancho, alto);
    bitmap.close?.();
    const tipo = png ? 'image/png' : 'image/jpeg';
    const blob = await new Promise((ok) => lienzo.toBlob(ok, tipo, png ? undefined : CALIDAD));
    // Ya era chica y el resultado no pesa menos: se manda la original.
    if (!blob || (escala === 1 && blob.size >= archivo.size)) return archivo;
    return new File([blob], png ? 'imagen.png' : 'imagen.jpg', { type: tipo });
  } catch {
    return archivo;
  }
}
