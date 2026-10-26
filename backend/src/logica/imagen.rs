//! Procesar imágenes que suben los usuarios (foto de producto, logo, foto de
//! encargo) sin poner en riesgo el servidor.
//!
//! Por qué existe (auditoría de seguridad, oct 2026): una imagen comprimida
//! puede pesar poco como archivo y ser enorme al abrirla. Un PNG de 1,7 MB de
//! 12 000 × 10 000 píxeles ocupaba ~474 MB de RAM y 5 s de CPU con
//! `image::load_from_memory`, y eso corría en el mismo hilo que atiende las
//! ventas. Dos o tres a la vez dejaban sin memoria al servidor de 1 GB.
//!
//! Lo que hace ahora, en este orden:
//!  1. Lee solo el encabezado: formato y medidas, sin abrir la imagen.
//!  2. Acepta solo JPG, PNG, WebP y GIF (lo que sube un celular o una PC);
//!     el resto de decodificadores (TIFF, BMP, ICO...) no se toca.
//!  3. Rechaza lo que pase de `MAX_MEGAPIXELES` o `MAX_LADO`: una foto de
//!     celular de 50 MP entra; una imagen trampa, no.
//!  4. Abre la imagen con tope de memoria (`MAX_MEMORIA`).
//!  5. Todo el trabajo pesado va en un hilo aparte (`spawn_blocking`), así
//!     las ventas siguen respondiendo mientras tanto.
//!  6. Una sola imagen a la vez en todo el servidor (`COLA`): si llegan
//!     varias juntas esperan su turno en vez de sumar memoria. Si la espera
//!     pasa de `ESPERA_MAXIMA`, se pide intentar de nuevo.

use std::io::Cursor;
use std::sync::OnceLock;
use std::time::Duration;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use tokio::sync::Semaphore;

/// Una foto de 50 MP (8160 × 6120) entra; más que eso no la saca ningún
/// celular común y solo sirve para atacar.
pub const MAX_MEGAPIXELES: u64 = 52_000_000;
/// Lado más largo permitido (panorámicas incluidas).
pub const MAX_LADO: u32 = 12_000;
/// Memoria máxima que puede pedir el decodificador para una imagen. Una
/// foto de 50 MP en RGB son ~150 MB; con canal alfa, ~200 MB.
pub const MAX_MEMORIA: u64 = 256 * 1024 * 1024;
/// Cuánto puede esperar una subida su turno antes de pedir reintentar.
const ESPERA_MAXIMA: Duration = Duration::from_secs(30);

/// Imágenes procesándose a la vez en todo el servidor.
fn cola() -> &'static Semaphore {
    static COLA: OnceLock<Semaphore> = OnceLock::new();
    COLA.get_or_init(|| Semaphore::new(1))
}

/// Cómo se guarda el resultado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Salida {
    /// JPEG con esa calidad (fotos de producto y de encargo).
    Jpeg(u8),
    /// PNG, conserva la transparencia (logo del negocio).
    Png,
}

/// Por qué no se aceptó una imagen. El texto es para mostrarlo tal cual.
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorImagen {
    /// Lo que mandó el usuario no sirve: 400.
    Invalida(String),
    /// El servidor está ocupado con otras imágenes: 503, que reintente.
    Ocupado,
    /// Falla nuestra (no pudo codificar, se cayó el hilo...): 500.
    Interna(String),
}

impl ErrorImagen {
    pub fn mensaje(&self) -> String {
        match self {
            ErrorImagen::Invalida(m) | ErrorImagen::Interna(m) => m.clone(),
            ErrorImagen::Ocupado => {
                "El sistema está procesando otras imágenes en este momento. Espera unos segundos e inténtalo de nuevo.".to_string()
            }
        }
    }

    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            ErrorImagen::Invalida(_) => StatusCode::BAD_REQUEST,
            ErrorImagen::Ocupado => StatusCode::SERVICE_UNAVAILABLE,
            ErrorImagen::Interna(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

const FORMATO_NO_SOPORTADO: &str = "Ese archivo no es una imagen que el sistema pueda usar. Sube una foto JPG, PNG o WebP.";

/// Revisa formato y medidas leyendo solo el encabezado (no abre la imagen).
/// Devuelve el formato y las medidas si se puede abrir con seguridad.
pub fn revisar(bytes: &[u8]) -> Result<(ImageFormat, u32, u32), ErrorImagen> {
    let lector = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ErrorImagen::Invalida(FORMATO_NO_SOPORTADO.to_string()))?;
    let formato = match lector.format() {
        Some(f @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif)) => f,
        _ => return Err(ErrorImagen::Invalida(FORMATO_NO_SOPORTADO.to_string())),
    };
    let (ancho, alto) = lector
        .into_dimensions()
        .map_err(|_| ErrorImagen::Invalida("La imagen está dañada o incompleta. Prueba con otra foto.".to_string()))?;
    if ancho == 0 || alto == 0 {
        return Err(ErrorImagen::Invalida("La imagen está vacía. Prueba con otra foto.".to_string()));
    }
    if ancho > MAX_LADO || alto > MAX_LADO || u64::from(ancho) * u64::from(alto) > MAX_MEGAPIXELES {
        return Err(ErrorImagen::Invalida(format!(
            "La imagen es demasiado grande ({} × {} píxeles). Usa una foto de hasta 50 megapíxeles, o toma una captura de pantalla de la foto y sube esa.",
            ancho, alto
        )));
    }
    Ok((formato, ancho, alto))
}

/// Abre la imagen (ya revisada) con tope de memoria.
fn abrir(bytes: &[u8], formato: ImageFormat) -> Result<DynamicImage, ErrorImagen> {
    let mut lector = ImageReader::with_format(Cursor::new(bytes), formato);
    let mut limites = Limits::default();
    limites.max_image_width = Some(MAX_LADO);
    limites.max_image_height = Some(MAX_LADO);
    limites.max_alloc = Some(MAX_MEMORIA);
    lector.limits(limites);
    lector
        .decode()
        .map_err(|_| ErrorImagen::Invalida("No se pudo leer la imagen. Prueba con otra foto (JPG, PNG o WebP).".to_string()))
}

/// Achica (sin agrandar) a `lado_maximo` y la codifica. Síncrono: llamar
/// solo desde un hilo aparte (lo hace `procesar`).
pub fn procesar_bloqueante(bytes: &[u8], lado_maximo: u32, salida: Salida) -> Result<Vec<u8>, ErrorImagen> {
    let (formato, ancho, alto) = revisar(bytes)?;
    let img = abrir(bytes, formato)?;
    // Triangle (bilineal) en vez de Lanczos3: al achicar se ve igual en una
    // foto de producto y tarda varias veces menos.
    let img = if ancho > lado_maximo || alto > lado_maximo {
        img.resize(lado_maximo, lado_maximo, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut resultado = Vec::new();
    match salida {
        Salida::Jpeg(calidad) => {
            // JPEG no tiene transparencia: lo transparente queda blanco (no negro).
            let rgb = sobre_blanco(&img);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut resultado, calidad)
                .encode_image(&rgb)
                .map_err(|e| ErrorImagen::Interna(format!("No se pudo comprimir la imagen: {}", e)))?;
        }
        Salida::Png => {
            img.write_to(&mut Cursor::new(&mut resultado), ImageFormat::Png)
                .map_err(|e| ErrorImagen::Interna(format!("No se pudo guardar la imagen: {}", e)))?;
        }
    }
    Ok(resultado)
}

/// Pone la imagen sobre fondo blanco (para JPEG).
fn sobre_blanco(img: &DynamicImage) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = u16::from(p[3]);
        let mezcla = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
        image::Rgb([mezcla(p[0]), mezcla(p[1]), mezcla(p[2])])
    })
}

/// Revisa, achica y codifica una imagen subida, en un hilo aparte y de a
/// una por vez en todo el servidor.
pub async fn procesar(bytes: Vec<u8>, lado_maximo: u32, salida: Salida) -> Result<Vec<u8>, ErrorImagen> {
    // Revisión rápida antes de hacer cola: lo inválido se rechaza al toque
    // y no le quita el turno a nadie.
    revisar(&bytes)?;
    let _turno = tokio::time::timeout(ESPERA_MAXIMA, cola().acquire())
        .await
        .map_err(|_| ErrorImagen::Ocupado)?
        .map_err(|_| ErrorImagen::Ocupado)?;
    tokio::task::spawn_blocking(move || procesar_bloqueante(&bytes, lado_maximo, salida))
        .await
        .map_err(|_| ErrorImagen::Interna("Se interrumpió el procesamiento de la imagen. Inténtalo de nuevo.".to_string()))?
}

/// Abre una imagen ya guardada por el sistema (el logo, para el ícono de la
/// app) con los mismos topes. Síncrono.
pub fn abrir_guardada(ruta: &std::path::Path) -> Result<DynamicImage, ErrorImagen> {
    let bytes = std::fs::read(ruta).map_err(|_| ErrorImagen::Invalida("sin imagen".to_string()))?;
    let (formato, _, _) = revisar(&bytes)?;
    abrir(&bytes, formato)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn png(ancho: u32, alto: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        image::RgbImage::new(ancho, alto).write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        buf
    }

    /// Un PNG que en el encabezado dice medir `ancho` × `alto` (lo único que
    /// se lee antes de decidir). Armarlo así evita crear 360 MB en la prueba.
    fn png_que_dice_medir(ancho: u32, alto: u32) -> Vec<u8> {
        fn crc32(datos: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for b in datos {
                crc ^= u32::from(*b);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        }
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend_from_slice(&ancho.to_be_bytes());
        ihdr.extend_from_slice(&alto.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, RGB
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(&ihdr);
        png.extend_from_slice(&crc32(&ihdr).to_be_bytes());
        // Comienzo de los datos (IDAT), como si la imagen siguiera comprimida.
        png.extend_from_slice(&1024u32.to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&[0u8; 1024]);
        png
    }

    fn jpeg(ancho: u32, alto: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(ancho, alto, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 120]));
        let mut buf = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&img).unwrap();
        buf
    }

    fn medidas(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn la_imagen_trampa_se_rechaza_sin_abrirla() {
        // El caso de la auditoría: archivo chico, imagen gigante.
        let bomba = png_que_dice_medir(12_000, 10_000);
        assert_eq!(revisar(&png_que_dice_medir(640, 480)).unwrap(), (ImageFormat::Png, 640, 480), "el PNG armado es válido");
        let inicio = std::time::Instant::now();
        let error = procesar_bloqueante(&bomba, 800, Salida::Jpeg(80)).unwrap_err();
        assert!(matches!(error, ErrorImagen::Invalida(ref m) if m.contains("demasiado grande")), "{:?}", error);
        assert!(inicio.elapsed() < Duration::from_millis(200), "se rechaza leyendo solo el encabezado");
    }

    #[test]
    fn muchos_megapixeles_aunque_cada_lado_entre() {
        assert!(revisar(&png_que_dice_medir(9_000, 9_000)).is_err(), "81 MP no entra");
        assert!(revisar(&png_que_dice_medir(12_001, 100)).is_err(), "lado de más de 12 000");
        assert!(revisar(&png_que_dice_medir(12_000, 4_000)).is_ok(), "panorámica de 48 MP entra");
    }

    #[test]
    fn una_foto_de_celular_de_50_mp_entra_y_se_achica() {
        // 8160 × 6120 = 49,9 MP (cámara de 50 MP).
        let foto = jpeg(8_160, 6_120);
        let listo = procesar_bloqueante(&foto, 800, Salida::Jpeg(80)).unwrap();
        assert_eq!(medidas(&listo), (800, 600));
    }

    #[test]
    fn una_foto_comun_queda_igual_que_antes() {
        let listo = procesar_bloqueante(&jpeg(4_000, 3_000), 800, Salida::Jpeg(80)).unwrap();
        assert_eq!(medidas(&listo), (800, 600));
        let vertical = procesar_bloqueante(&jpeg(3_000, 4_000), 1200, Salida::Jpeg(82)).unwrap();
        assert_eq!(medidas(&vertical), (900, 1200));
    }

    #[test]
    fn una_imagen_chica_no_se_agranda() {
        let listo = procesar_bloqueante(&jpeg(300, 200), 800, Salida::Jpeg(80)).unwrap();
        assert_eq!(medidas(&listo), (300, 200));
    }

    #[test]
    fn el_logo_conserva_la_transparencia() {
        let logo = image::RgbaImage::from_fn(1_000, 500, |x, _| image::Rgba([10, 120, 60, if x < 500 { 0 } else { 255 }]));
        let mut buf = Vec::new();
        logo.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        let listo = procesar_bloqueante(&buf, 400, Salida::Png).unwrap();
        let img = image::load_from_memory(&listo).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (400, 200));
        assert_eq!(img.get_pixel(10, 100).0[3], 0, "lo transparente sigue transparente");
        assert_eq!(img.get_pixel(390, 100).0[3], 255);
    }

    #[test]
    fn lo_transparente_queda_blanco_en_jpeg() {
        let img = image::RgbaImage::from_pixel(50, 50, image::Rgba([0, 0, 0, 0]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        let listo = procesar_bloqueante(&buf, 800, Salida::Jpeg(90)).unwrap();
        let p = image::load_from_memory(&listo).unwrap().to_rgb8().get_pixel(25, 25).0;
        assert!(p.iter().all(|c| *c > 245), "blanco, no negro: {:?}", p);
    }

    #[test]
    fn formatos_que_no_son_fotos_se_rechazan() {
        let mut tiff = Vec::new();
        image::RgbImage::new(10, 10).write_to(&mut Cursor::new(&mut tiff), ImageFormat::Tiff).unwrap();
        assert!(matches!(revisar(&tiff), Err(ErrorImagen::Invalida(_))));
        assert!(matches!(revisar(b"no soy una imagen"), Err(ErrorImagen::Invalida(_))));
        assert!(matches!(revisar(b"%PDF-1.7 ..."), Err(ErrorImagen::Invalida(_))));
        assert!(matches!(revisar(&[]), Err(ErrorImagen::Invalida(_))));
    }

    #[test]
    fn una_imagen_cortada_a_la_mitad_no_tumba_nada() {
        let foto = jpeg(2_000, 1_500);
        let cortada = &foto[..foto.len() / 2];
        // Puede abrirse parcial o rechazarse, pero nunca entrar en pánico.
        let _ = procesar_bloqueante(cortada, 800, Salida::Jpeg(80));
        let png_cortado = &png(500, 500)[..60];
        assert!(procesar_bloqueante(png_cortado, 800, Salida::Jpeg(80)).is_err());
    }

    #[tokio::test]
    async fn de_a_una_por_vez_y_sin_bloquear() {
        // Varias subidas a la vez terminan todas bien (esperan su turno).
        let tareas: Vec<_> = (0..4).map(|_| tokio::spawn(procesar(jpeg(3_000, 2_000), 800, Salida::Jpeg(80)))).collect();
        for t in tareas {
            let listo = t.await.unwrap().unwrap();
            assert_eq!(medidas(&listo), (800, 533));
        }
        // La trampa se rechaza antes de hacer cola.
        assert!(matches!(procesar(png_que_dice_medir(12_000, 10_000), 800, Salida::Jpeg(80)).await, Err(ErrorImagen::Invalida(_))));
    }
}
