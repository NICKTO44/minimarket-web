use std::env;

use libsql::Builder;
use minimarket_backend::{
    crypto,
    licencias_logica::{calcular_nueva_fecha, generar_codigo, hoy},
    tenants::RegistroTiendas,
};

async fn listar(registro: &RegistroTiendas) {
    let tiendas = match registro.listar_todas().await {
        Ok(t) => t,
        Err(e) => {
            println!("Error listando negocios: {}", e);
            return;
        }
    };

    let hoy_str = hoy().format("%Y-%m-%d").to_string();

    println!(
        "{:<26} {:<22} {:<12} {:<12} {}",
        "IDENTIFICADOR", "NEGOCIO", "ESTADO", "VENCE", "SITUACIÓN"
    );
    for t in tiendas {
        let situacion = if t.estado == "SUSPENDIDO" {
            "bloqueado".to_string()
        } else if t.estado == "RESTRINGIDO" {
            "modo lectura".to_string()
        } else {
            match &t.fecha_vencimiento {
                Some(f) if f.as_str() < hoy_str.as_str() => "modo lectura (venció)".to_string(),
                Some(_) => "activo".to_string(),
                None => "activo (sin vencimiento)".to_string(),
            }
        };

        println!(
            "{:<26} {:<22} {:<12} {:<12} {}",
            t.identificador,
            t.nombre_negocio,
            t.estado,
            t.fecha_vencimiento.as_deref().unwrap_or("—"),
            situacion
        );
    }
}

async fn buscar(conn: &libsql::Connection, texto: &str) {
    let patron = format!("%{}%", texto);

    let mut rows = match conn
        .query(
            "SELECT t.nombre_negocio, t.identificador, t.ruc, t.estado, t.fecha_vencimiento, u.usuario
             FROM tiendas t
             LEFT JOIN usuarios_indice u ON u.tienda_id = t.id
             WHERE t.nombre_negocio LIKE ?1
                OR t.identificador LIKE ?1
                OR t.ruc LIKE ?1
                OR u.usuario LIKE ?1",
            libsql::params![patron],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Error buscando: {}", e);
            return;
        }
    };

    let mut encontrados = 0;
    while let Ok(Some(row)) = rows.next().await {
        encontrados += 1;
        let nombre: String = row.get(0).unwrap_or_default();
        let identificador: String = row.get(1).unwrap_or_default();
        let ruc: Option<String> = row.get(2).unwrap_or(None);
        let estado: String = row.get(3).unwrap_or_default();
        let vencimiento: Option<String> = row.get(4).unwrap_or(None);
        let usuario: Option<String> = row.get(5).unwrap_or(None);

        println!("Negocio:       {}", nombre);
        println!("Identificador: {}", identificador);
        println!("RUC:           {}", ruc.as_deref().unwrap_or("—"));
        println!("Usuario:       {}", usuario.as_deref().unwrap_or("—"));
        println!("Estado:        {}", estado);
        println!("Vence:         {}", vencimiento.as_deref().unwrap_or("sin vencimiento"));
        println!("---");
    }

    if encontrados == 0 {
        println!("No se encontró ningún negocio que coincida con '{}'.", texto);
    }
}

async fn activar(conn: &libsql::Connection, identificador: &str, cantidad: i64, unidad: &str) {
    let mut rows = match conn
        .query(
            "SELECT fecha_vencimiento FROM tiendas WHERE identificador = ?1",
            libsql::params![identificador],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Error consultando: {}", e);
            return;
        }
    };

    let actual: Option<String> = match rows.next().await {
        Ok(Some(row)) => row.get(0).unwrap_or(None),
        Ok(None) => {
            println!("No se encontró ningún negocio con identificador '{}'.", identificador);
            return;
        }
        Err(e) => {
            println!("Error leyendo resultado: {}", e);
            return;
        }
    };

    let nueva_fecha = match calcular_nueva_fecha(actual.as_deref(), cantidad, unidad) {
        Ok(f) => f,
        Err(e) => {
            println!("Error: {}", e);
            return;
        }
    };

    if let Err(e) = conn
        .execute(
            "UPDATE tiendas SET estado = 'ACTIVO', fecha_vencimiento = ?1 WHERE identificador = ?2",
            libsql::params![nueva_fecha.clone(), identificador],
        )
        .await
    {
        println!("Error actualizando: {}", e);
        return;
    }

    println!("✅ {} activado hasta {}", identificador, nueva_fecha);
}

async fn cambiar_estado(conn: &libsql::Connection, identificador: &str, estado: &str) {
    let filas_afectadas = match conn
        .execute(
            "UPDATE tiendas SET estado = ?1 WHERE identificador = ?2",
            libsql::params![estado, identificador],
        )
        .await
    {
        Ok(n) => n,
        Err(e) => {
            println!("Error actualizando: {}", e);
            return;
        }
    };

    if filas_afectadas == 0 {
        println!("No se encontró ningún negocio con identificador '{}'.", identificador);
    } else {
        println!("✅ {} -> {}", identificador, estado);
    }
}

fn normalizar_unidad(unidad: &str) -> Option<&'static str> {
    match unidad {
        "dia" | "dias" | "día" | "días" => Some("DIA"),
        "mes" | "meses" => Some("MES"),
        "anio" | "anios" | "año" | "años" => Some("ANIO"),
        _ => None,
    }
}

async fn generar_codigo_cmd(conn: &libsql::Connection, cantidad: i64, unidad: &str, dias_para_expirar: i64) {
    let unidad_normalizada = match normalizar_unidad(unidad) {
        Some(u) => u,
        None => {
            println!("Unidad no reconocida: '{}' (usa dias, meses o anios)", unidad);
            return;
        }
    };

    let codigo = generar_codigo();
    let fecha_expira = (hoy() + chrono::Duration::days(dias_para_expirar))
        .format("%Y-%m-%d")
        .to_string();

    if let Err(e) = conn
        .execute(
            "INSERT INTO codigos_activacion (codigo, duracion_cantidad, duracion_unidad, fecha_expira_si_no_se_usa)
             VALUES (?1, ?2, ?3, ?4)",
            libsql::params![codigo.clone(), cantidad, unidad_normalizada, fecha_expira.clone()],
        )
        .await
    {
        println!("Error generando el código: {}", e);
        return;
    }

    println!("✅ Código generado: {}", codigo);
    println!("   Activa por: {} {}", cantidad, unidad_normalizada.to_lowercase());
    println!("   Caduca si nadie lo usa antes del: {}", fecha_expira);
    println!("   Mándaselo al cliente por WhatsApp — es de un solo uso, para un solo negocio.");
}

fn imprimir_ayuda() {
    println!("Uso:");
    println!("  licencias generar-codigo <cantidad> <dias|meses|anios> [dias_para_caducar_sin_usar=30]");
    println!("  licencias buscar <nombre, RUC o usuario>");
    println!("  licencias listar");
    println!("  licencias activar <identificador> <cantidad> <dias|meses|anios>");
    println!("  licencias restringir <identificador>");
    println!("  licencias suspender <identificador>");
    println!("  licencias reactivar <identificador>");
    println!();
    println!("Ejemplos:");
    println!("  licencias generar-codigo 1 mes");
    println!("  licencias generar-codigo 1 anio 15");
    println!("  licencias buscar \"La Esquina\"");
    println!("  licencias activar bodega-juan-83af 1 mes");
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let comando = args.get(1).map(|s| s.as_str());

    let central_db_url = std::env::var("CENTRAL_DATABASE_URL").expect("Falta CENTRAL_DATABASE_URL en .env");
    let central_db_token = std::env::var("CENTRAL_AUTH_TOKEN").expect("Falta CENTRAL_AUTH_TOKEN en .env");
    let clave_cifrado = crypto::cargar_clave_desde_env();

    let central_db = Builder::new_remote(central_db_url, central_db_token)
        .build()
        .await
        .expect("No se pudo conectar a la base central");

    let registro = RegistroTiendas::nuevo(central_db, clave_cifrado);
    let conn = registro
        .conexion_central()
        .expect("No se pudo abrir conexión a la base central");

    match comando {
        Some("generar-codigo") => {
            let cantidad: i64 = match args.get(2).and_then(|s| s.parse().ok()) {
                Some(v) => v,
                None => return imprimir_ayuda(),
            };
            let unidad = match args.get(3) {
                Some(v) => v,
                None => return imprimir_ayuda(),
            };
            let dias_para_expirar: i64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);
            generar_codigo_cmd(&conn, cantidad, unidad, dias_para_expirar).await;
        }
        Some("buscar") => match args.get(2) {
            Some(texto) => buscar(&conn, texto).await,
            None => imprimir_ayuda(),
        },
        Some("listar") => listar(&registro).await,
        Some("activar") => {
            let identificador = match args.get(2) {
                Some(v) => v,
                None => return imprimir_ayuda(),
            };
            let cantidad: i64 = match args.get(3).and_then(|s| s.parse().ok()) {
                Some(v) => v,
                None => return imprimir_ayuda(),
            };
            let unidad = match args.get(4) {
                Some(v) => v,
                None => return imprimir_ayuda(),
            };
            activar(&conn, identificador, cantidad, unidad).await;
        }
        Some("restringir") => match args.get(2) {
            Some(identificador) => cambiar_estado(&conn, identificador, "RESTRINGIDO").await,
            None => imprimir_ayuda(),
        },
        Some("suspender") => match args.get(2) {
            Some(identificador) => cambiar_estado(&conn, identificador, "SUSPENDIDO").await,
            None => imprimir_ayuda(),
        },
        Some("reactivar") => match args.get(2) {
            Some(identificador) => cambiar_estado(&conn, identificador, "ACTIVO").await,
            None => imprimir_ayuda(),
        },
        _ => imprimir_ayuda(),
    }
}