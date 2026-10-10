//! Alta de un negocio para emitir directo a SUNAT (sin FacturaLibre).
//!
//!   sunat ver <identificador>
//!   sunat dato <identificador> <campo> <valor>
//!   sunat alta <identificador> <certificado.p12|.pem> <usuario_sol_secundario> [produccion|beta]
//!   sunat modo <identificador> <directo|facturalibre>
//!
//! `alta` registra al negocio en Lycet (su RUC, su certificado y su usuario
//! secundario SOL) y lo pasa a emisión directa. La clave SOL se pide por
//! teclado (o se toma de SUNAT_SOL_CLAVE) para que no quede en el historial.
//!
//! El certificado puede ser el .p12/.pfx que entrega SUNAT (su clave se pide
//! por teclado o se toma de SUNAT_CERT_CLAVE) o un .pem ya convertido: se
//! convierte solo. Lo mismo se hace con botones desde el panel (/panel).

use std::env;
use std::io::{self, Write};

use libsql::Builder;
use minimarket_backend::{
    crypto,
    handlers::facturacion_directa::leer_emisor,
    logica::{alta_sunat, certificado},
    tenants::RegistroTiendas,
};

use alta_sunat::CAMPOS;

fn imprimir_ayuda() {
    println!("Uso:");
    println!("  sunat ver <identificador>");
    println!("  sunat dato <identificador> <campo> <valor>");
    println!("  sunat alta <identificador> <certificado.p12|.pem> <usuario_sol_secundario> [produccion|beta]");
    println!("  sunat modo <identificador> <directo|facturalibre>");
    println!();
    println!("Campos de `dato`:");
    for (campo, desc) in CAMPOS {
        println!("  {:<15} {}", campo, desc);
    }
    println!();
    println!("Ejemplo para un negocio nuevo:");
    println!("  sunat dato verane-1a2b razon_social \"CHAVARRIA HUAMAN JORGE\"");
    println!("  sunat dato verane-1a2b ubigeo 150101");
    println!("  sunat dato verane-1a2b serie_boleta BM01");
    println!("  sunat ver verane-1a2b");
    println!("  sunat alta verane-1a2b certificado.p12 MONSPEET produccion");
}

async fn conexion(registro: &RegistroTiendas, identificador: &str) -> Result<libsql::Connection, String> {
    let tienda = registro.buscar_por_identificador(identificador).await?;
    let db = registro.conectar(&tienda).await?;
    db.connect().map_err(|e| e.to_string())
}

async fn texto_config(conn: &libsql::Connection, columna: &str) -> Option<String> {
    let mut filas = conn.query(&format!("SELECT {} FROM configuracion_tienda LIMIT 1", columna), ()).await.ok()?;
    filas.next().await.ok()??.get::<String>(0).ok()
}

async fn ver(conn: &libsql::Connection) {
    let modo = texto_config(conn, "facturacion_proveedor").await.unwrap_or_else(|| "(sin definir)".into());
    println!("Modo de emisión: {}", if modo == "SUNAT_DIRECTO" { "DIRECTO a SUNAT".to_string() } else { format!("{} (FacturaLibre)", modo) });
    for (campo, desc) in CAMPOS {
        let valor = texto_config(conn, campo).await.unwrap_or_default();
        println!("  {:<15} {:<45} ({})", campo, if valor.is_empty() { "—".to_string() } else { valor }, desc);
    }
    match leer_emisor(conn).await {
        Ok(emisor) => match emisor.dato_faltante() {
            None => println!("\n✅ Datos del emisor completos."),
            Some(falta) => println!("\n⚠️  Falta {} para poder emitir.", falta),
        },
        Err((_, e)) => println!("\n⚠️  {}", e),
    }
}

async fn dato(conn: &libsql::Connection, campo: &str, valor: &str) {
    let valor = match alta_sunat::validar_dato(campo, valor) {
        Ok(v) => v,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };
    match conn
        .execute(&format!("UPDATE configuracion_tienda SET {} = ?1", campo), libsql::params![valor.clone()])
        .await
    {
        Ok(_) => println!("✅ {} = {}", campo, valor),
        Err(e) => println!("Error guardando {}: {}", campo, e),
    }
}

async fn modo(conn: &libsql::Connection, modo: &str) {
    let valor = match modo {
        "directo" => "SUNAT_DIRECTO",
        "facturalibre" => "FACTURALIBRE",
        _ => return imprimir_ayuda(),
    };
    match conn.execute("UPDATE configuracion_tienda SET facturacion_proveedor = ?1", libsql::params![valor]).await {
        Ok(_) => println!("✅ Modo de emisión: {}", valor),
        Err(e) => println!("Error: {}", e),
    }
}

/// Lee un secreto de una variable de entorno o, si no está, del teclado.
fn leer_secreto(variable: &str, pregunta: &str) -> String {
    if let Ok(valor) = env::var(variable) {
        if !valor.trim().is_empty() {
            return valor.trim().to_string();
        }
    }
    print!("{} (no se verá al escribir): ", pregunta);
    io::stdout().flush().ok();
    // En una terminal, lo que se escribe no se muestra (stty -echo), igual
    // que al pedir una contraseña. Se restaura siempre, aunque falle la lectura.
    let terminal = io::IsTerminal::is_terminal(&io::stdin());
    let eco = |activo: bool| {
        if terminal {
            let _ = std::process::Command::new("stty")
                .arg(if activo { "echo" } else { "-echo" })
                .stdin(std::process::Stdio::inherit())
                .status();
        }
    };
    eco(false);
    let mut valor = String::new();
    io::stdin().read_line(&mut valor).ok();
    eco(true);
    if terminal {
        println!();
    }
    valor.trim().to_string()
}

async fn alta(conn: &libsql::Connection, archivo: &str, usuario_sol: &str, ambiente: &str) {
    if alta_sunat::normalizar_ambiente(ambiente).is_none() {
        return imprimir_ayuda();
    }
    let bytes = match std::fs::read(archivo) {
        Ok(b) => b,
        Err(e) => {
            println!("No se pudo leer {}: {}", archivo, e);
            return;
        }
    };
    let es_pem = String::from_utf8_lossy(&bytes).contains("-----BEGIN");
    let clave_cert = if es_pem { String::new() } else { leer_secreto("SUNAT_CERT_CLAVE", "Clave del certificado (.p12)") };
    let cert = match certificado::leer(&bytes, &clave_cert) {
        Ok(c) => c,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };
    println!("Certificado de \"{}\", vigente hasta {}.", cert.descripcion.titular, cert.descripcion.vence);

    let clave_sol = leer_secreto("SUNAT_SOL_CLAVE", "Clave SOL del usuario secundario");
    // Credenciales API de SUNAT para guías (opcionales, solo por variables).
    let gre_id = env::var("SUNAT_GRE_CLIENT_ID").unwrap_or_default();
    let gre_secreto = env::var("SUNAT_GRE_CLIENT_SECRET").unwrap_or_default();
    let guias = (!gre_id.trim().is_empty() && !gre_secreto.trim().is_empty()).then_some((gre_id.as_str(), gre_secreto.as_str()));
    match alta_sunat::registrar(conn, &cert, usuario_sol, &clave_sol, ambiente, guias).await {
        Ok(hecha) => {
            println!("✅ {} ({}) registrado en Lycet para {}.", hecha.razon_social, hecha.ruc, hecha.ambiente.to_lowercase());
            println!("✅ Modo de emisión: SUNAT_DIRECTO");
            if let Some(aviso) = hecha.aviso {
                println!("⚠️  {}", aviso);
            }
            if hecha.ambiente == "BETA" {
                println!("Siguiente: emite una boleta de prueba; en beta SUNAT responde pero no tiene valor legal.");
            } else {
                println!("Siguiente: emite una boleta de monto bajo y verifícala en SUNAT.");
            }
        }
        Err(e) => println!("{}", e),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args: Vec<String> = env::args().collect();
    let comando = args.get(1).map(|s| s.as_str());
    let identificador = match (comando, args.get(2)) {
        (Some(_), Some(id)) => id.clone(),
        _ => return imprimir_ayuda(),
    };

    let central_db_url = env::var("CENTRAL_DATABASE_URL").expect("Falta CENTRAL_DATABASE_URL en .env");
    let central_db_token = env::var("CENTRAL_AUTH_TOKEN").expect("Falta CENTRAL_AUTH_TOKEN en .env");
    let central_db = Builder::new_remote(central_db_url, central_db_token)
        .build()
        .await
        .expect("No se pudo conectar a la base central");
    let registro = RegistroTiendas::nuevo(central_db, crypto::cargar_clave_desde_env());

    let conn = match conexion(&registro, &identificador).await {
        Ok(c) => c,
        Err(e) => {
            println!("No se pudo abrir el negocio \"{}\": {}", identificador, e);
            return;
        }
    };

    match comando {
        Some("ver") => ver(&conn).await,
        Some("dato") => match (args.get(3), args.get(4)) {
            (Some(campo), Some(valor)) => dato(&conn, campo, valor).await,
            _ => imprimir_ayuda(),
        },
        Some("alta") => match (args.get(3), args.get(4)) {
            (Some(archivo), Some(usuario)) => {
                let ambiente = args.get(5).map(|s| s.as_str()).unwrap_or("produccion");
                alta(&conn, archivo, usuario, ambiente).await
            }
            _ => imprimir_ayuda(),
        },
        Some("modo") => match args.get(3) {
            Some(m) => modo(&conn, m).await,
            None => imprimir_ayuda(),
        },
        _ => imprimir_ayuda(),
    }
}
