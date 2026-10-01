// Achica una foto en el navegador antes de subirla. Una foto de celular
// pesa 3-8 MB y el servidor igual la deja en 800 px (ver imagenes.rs):
// así se sube en un segundo con datos móviles. Si el navegador no puede
// leerla, se manda la original y el servidor decide.
const LADO_MAXIMO = 1200;
const CALIDAD = 0.85;

export async function comprimirImagen(archivo) {
  try {
    const bitmap = await createImageBitmap(archivo);
    const escala = Math.min(1, LADO_MAXIMO / Math.max(bitmap.width, bitmap.height));
    const ancho = Math.round(bitmap.width * escala);
    const alto = Math.round(bitmap.height * escala);
    const lienzo = document.createElement('canvas');
    lienzo.width = ancho;
    lienzo.height = alto;
    const ctx = lienzo.getContext('2d');
    // Fondo blanco: un PNG con transparencia no queda negro al pasar a JPEG.
    ctx.fillStyle = '#fff';
    ctx.fillRect(0, 0, ancho, alto);
    ctx.drawImage(bitmap, 0, 0, ancho, alto);
    bitmap.close?.();
    const blob = await new Promise((ok) => lienzo.toBlob(ok, 'image/jpeg', CALIDAD));
    if (!blob || blob.size >= archivo.size) return archivo;
    return new File([blob], 'imagen.jpg', { type: 'image/jpeg' });
  } catch {
    return archivo;
  }
}
