use axum::Json;
use std::net::TcpStream;
use std::io::Write;
use std::time::Duration;

use crate::models::impresora::{DatosImpresion, ImpresionResponse};

fn centrar(texto: &str, ancho: usize) -> String {
    if texto.len() >= ancho { return texto.to_string(); }
    let espacios = (ancho - texto.len()) / 2;
    format!("{}{}", " ".repeat(espacios), texto)
}

fn alinear_derecha(izquierda: &str, derecha: &str, ancho: usize) -> String {
    let total = izquierda.len() + derecha.len();
    if total >= ancho { return format!("{}{}", izquierda, derecha); }
    format!("{}{}{}", izquierda, ".".repeat(ancho - total), derecha)
}

pub async fn imprimir_boleta(Json(datos): Json<DatosImpresion>) -> Json<ImpresionResponse> {
    let puerto = datos.impresora_puerto.unwrap_or(9100);
    let direccion = format!("{}:{}", datos.impresora_ip, puerto);
    let ancho = 42usize; // estándar en térmicas de 80mm

    let resultado = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut stream = TcpStream::connect(&direccion)
            .map_err(|e| format!("No se pudo conectar a la impresora {}: {}", direccion, e))?;
        stream.set_write_timeout(Some(Duration::from_secs(8))).ok();

        let mut texto = String::new();
        texto.push_str(&"=".repeat(ancho));
        texto.push('\n');
        texto.push_str(&centrar(&datos.nombre_tienda, ancho));
        texto.push('\n');
        if let Some(dir) = &datos.direccion { if !dir.is_empty() {
            texto.push_str(&centrar(dir, ancho)); texto.push('\n');
        }}
        if let Some(tel) = &datos.telefono { if !tel.is_empty() {
            texto.push_str(&centrar(&format!("Tel: {}", tel), ancho)); texto.push('\n');
        }}
        texto.push_str(&"=".repeat(ancho));
        texto.push('\n');
        if let Some(num) = &datos.numero_boleta { texto.push_str(&format!("Boleta N: {}\n", num)); }
        if let Some(cajero) = &datos.cajero { texto.push_str(&format!("Cajero: {}\n", cajero)); }
        let ahora = chrono::Local::now();
        texto.push_str(&format!("Fecha: {}\n", ahora.format("%d/%m/%Y %H:%M")));
        texto.push_str(&"-".repeat(ancho));
        texto.push('\n');

        for item in &datos.items {
            let nombre = if item.nombre.len() > ancho { item.nombre[..ancho].to_string() } else { item.nombre.clone() };
            texto.push_str(&format!("{}\n", nombre));
            let izq = format!("  {:.0} x S/.{:.2}", item.cantidad, item.precio_unitario);
            let der = format!("S/.{:.2}", item.subtotal);
            texto.push_str(&alinear_derecha(&izq, &der, ancho));
            texto.push('\n');
        }

        texto.push_str(&"=".repeat(ancho));
        texto.push('\n');
        texto.push_str(&alinear_derecha("TOTAL:", &format!("S/.{:.2}", datos.total), ancho));
        texto.push('\n');
        if let Some(efectivo) = datos.efectivo {
            texto.push_str(&alinear_derecha("Efectivo:", &format!("S/.{:.2}", efectivo), ancho)); texto.push('\n');
        }
        if let Some(cambio) = datos.cambio {
            texto.push_str(&alinear_derecha("Cambio:", &format!("S/.{:.2}", cambio), ancho)); texto.push('\n');
        }
        texto.push_str(&"=".repeat(ancho));
        texto.push('\n');
        texto.push_str(&centrar("Gracias por su compra!", ancho));
        texto.push_str("\n\n\n\n");

        // Secuencia estándar ESC/POS para térmicas modernas
        stream.write_all(&[0x1B, 0x40]).map_err(|e| e.to_string())?; // init
        stream.write_all(&[0x1B, 0x74, 0x10]).map_err(|e| e.to_string())?; // charset
        stream.write_all(texto.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(&[0x1D, 0x56, 0x41, 0x00]).ok(); // corte de papel
        stream.flush().ok();
        Ok(())
    }).await;

    match resultado {
        Ok(Ok(())) => Json(ImpresionResponse { success: true, message: "Boleta impresa correctamente".into() }),
        Ok(Err(e)) => Json(ImpresionResponse { success: false, message: e }),
        Err(e) => Json(ImpresionResponse { success: false, message: format!("Error interno: {}", e) }),
    }
}