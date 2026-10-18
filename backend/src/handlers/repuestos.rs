//! Módulo FICHA_REPUESTO (migración 0037): marca, n.° de parte, códigos
//! alternos, ubicación en el almacén y garantía de un producto que es
//! repuesto o lubricante. Pensado para el rubro Lubricentro / Repuestos;
//! sirve a cualquier negocio que lo encienda.
//!
//! La ficha vive en sus propias tablas: un producto sin ficha es un producto
//! común. Nada de esto cambia cómo se vende, cobra o descuenta stock. Las
//! EQUIVALENCIAS (un filtro que reemplaza a otro) se calculan en el
//! navegador: dos productos son equivalentes si comparten un código (su n.°
//! de parte o uno de sus códigos alternos).
//!
//! Rutas (con sesión, y solo con el módulo encendido):
//!   GET    /repuestos/fichas          -> todas, para el punto de venta y Productos
//!   PUT    /productos/:id/repuesto    -> crea o reemplaza la de un producto
//!   DELETE /productos/:id/repuesto    -> el producto deja de tener ficha

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::rubros::{exigir_modulo, MODULO_FICHA_REPUESTO};
use crate::logica::tiempo;
use crate::tenants::TenantDb;

type Fallo = (StatusCode, String);

const NOMBRE_MODULO: &str = "Ficha del repuesto";
/// Códigos alternos por producto, como máximo.
pub const MAX_CODIGOS: usize = 30;
/// Garantía máxima (10 años).
pub const MAX_GARANTIA_DIAS: i64 = 3650;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodigoAlterno {
    pub codigo: String,
    #[serde(default)]
    pub fabricante: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FichaRepuesto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producto_id: Option<i64>,
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub numero_parte: Option<String>,
    #[serde(default)]
    pub ubicacion: Option<String>,
    #[serde(default)]
    pub garantia_dias: Option<i64>,
    #[serde(default)]
    pub codigos: Vec<CodigoAlterno>,
}

fn malo(texto: impl Into<String>) -> Fallo {
    (StatusCode::BAD_REQUEST, texto.into())
}
fn e500<E: std::fmt::Display>(e: E) -> Fallo {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Espacios de más fuera; vacío = None; máximo `max` letras.
fn texto(valor: Option<&str>, max: usize, campo: &str) -> Result<Option<String>, String> {
    let limpio = valor.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty());
    match limpio {
        Some(v) if v.chars().count() > max => Err(format!("{}: máximo {} letras.", campo, max)),
        otro => Ok(otro),
    }
}

/// "90915-YZZE1", "w 712/75", "PH-3614." -> "90915YZZE1", "W71275", "PH3614":
/// solo letras y números, en mayúsculas. Así se encuentra el repuesto
/// aunque lo escriban con o sin guiones.
pub fn clave_codigo(codigo: &str) -> String {
    codigo
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .map(|c| match c {
            'Á' => 'A',
            'É' => 'E',
            'Í' => 'I',
            'Ó' => 'O',
            'Ú' | 'Ü' => 'U',
            'Ñ' => 'N',
            otro => otro,
        })
        .collect()
}

/// Separa una lista de códigos escrita en una sola celda o campo:
/// "PH3614, W712/75; 90915-YZZE1" (comas, punto y coma, barras verticales o
/// saltos de línea). Un código puede llevar el fabricante entre paréntesis:
/// "W712/75 (Mann)".
pub fn codigos_desde_texto(texto: &str) -> Vec<CodigoAlterno> {
    texto
        .split([',', ';', '|', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| match (t.find('('), t.ends_with(')')) {
            (Some(i), true) if i > 0 => CodigoAlterno {
                codigo: t[..i].trim().to_string(),
                fabricante: Some(t[i + 1..t.len() - 1].trim().to_string()).filter(|f| !f.is_empty()),
            },
            _ => CodigoAlterno { codigo: t.to_string(), fabricante: None },
        })
        .collect()
}

/// Revisa y ordena la ficha: textos limpios, códigos sin repetir (ni
/// repetidos del n.° de parte) y garantía en días.
pub fn validar(f: FichaRepuesto) -> Result<FichaRepuesto, String> {
    let marca = texto(f.marca.as_deref(), 60, "Marca")?;
    let numero_parte = texto(f.numero_parte.as_deref(), 40, "N.° de parte")?.map(|p| p.to_uppercase());
    let ubicacion = texto(f.ubicacion.as_deref(), 60, "Ubicación")?;
    let garantia_dias = match f.garantia_dias {
        None | Some(0) => None,
        Some(d) if (1..=MAX_GARANTIA_DIAS).contains(&d) => Some(d),
        Some(_) => return Err(format!("Garantía: escribe los días, de 1 a {} (o déjala vacía).", MAX_GARANTIA_DIAS)),
    };
    if let Some(p) = &numero_parte {
        if clave_codigo(p).len() < 2 {
            return Err("El n.° de parte debe tener al menos 2 letras o números.".into());
        }
    }
    let clave_parte = numero_parte.as_deref().map(clave_codigo);
    let mut vistos: Vec<String> = Vec::new();
    let mut codigos = Vec::new();
    for c in f.codigos {
        let Some(codigo) = texto(Some(&c.codigo), 40, "Código alterno")?.map(|c| c.to_uppercase()) else { continue };
        let clave = clave_codigo(&codigo);
        if clave.len() < 2 {
            return Err(format!("El código alterno \"{}\" debe tener al menos 2 letras o números.", codigo));
        }
        if clave_parte.as_deref() == Some(clave.as_str()) || vistos.contains(&clave) {
            continue;
        }
        vistos.push(clave);
        codigos.push(CodigoAlterno { codigo, fabricante: texto(c.fabricante.as_deref(), 40, "Fabricante del código")? });
    }
    if codigos.len() > MAX_CODIGOS {
        return Err(format!("Máximo {} códigos alternos por producto.", MAX_CODIGOS));
    }
    Ok(FichaRepuesto { producto_id: None, marca, numero_parte, ubicacion, garantia_dias, codigos })
}

/// true si la ficha no dice nada (equivale a no tener ficha).
pub fn vacia(f: &FichaRepuesto) -> bool {
    f.marca.is_none() && f.numero_parte.is_none() && f.ubicacion.is_none() && f.garantia_dias.is_none() && f.codigos.is_empty()
}

/// La ficha tal como viene de un archivo (importación desde Excel).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepuestoImportado {
    #[serde(default)]
    pub marca: Option<String>,
    #[serde(default)]
    pub numero_parte: Option<String>,
    /// "PH3614, W712/75 (Mann)".
    #[serde(default)]
    pub codigos_alternos: Option<String>,
    #[serde(default)]
    pub ubicacion: Option<String>,
    /// "30", "30 días", "6 meses", "1 año".
    #[serde(default)]
    pub garantia: Option<String>,
}

/// "30", "30 días", "6 meses", "1 año", "sin garantía" -> días. None si va
/// vacío o dice que no tiene; Err si no se entiende.
pub fn garantia_desde_texto(texto: &str) -> Result<Option<i64>, String> {
    let t = texto.trim().to_lowercase();
    if t.is_empty() || t.starts_with("sin") || t == "no" || t == "-" || t == "0" {
        return Ok(None);
    }
    let numero: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let resto = t[numero.len()..].trim();
    let n: i64 = numero.parse().map_err(|_| format!("Garantía no reconocida: \"{}\" (escribe los días, p. ej. 30)", texto.trim()))?;
    let dias = if resto.is_empty() || resto.starts_with('d') {
        n
    } else if resto.starts_with("se") {
        n * 7
    } else if resto.starts_with('m') {
        n * 30
    } else if resto.starts_with('a') {
        n * 365
    } else {
        return Err(format!("Garantía no reconocida: \"{}\" (escribe los días, p. ej. 30)", texto.trim()));
    };
    if dias == 0 {
        return Ok(None);
    }
    if dias > MAX_GARANTIA_DIAS {
        return Err(format!("Garantía muy larga: \"{}\"", texto.trim()));
    }
    Ok(Some(dias))
}

/// Ficha de una fila importada. Ok(None) si la fila no trae ningún dato del
/// repuesto (es un producto común).
pub fn ficha_desde_importada(f: &RepuestoImportado) -> Result<Option<FichaRepuesto>, String> {
    let garantia_dias = garantia_desde_texto(f.garantia.as_deref().unwrap_or(""))?;
    let ficha = validar(FichaRepuesto {
        producto_id: None,
        marca: f.marca.clone(),
        numero_parte: f.numero_parte.clone(),
        ubicacion: f.ubicacion.clone(),
        garantia_dias,
        codigos: codigos_desde_texto(f.codigos_alternos.as_deref().unwrap_or("")),
    })?;
    Ok(if vacia(&ficha) { None } else { Some(ficha) })
}

/// Guarda (o reemplaza) fichas dentro de la transacción que reciba. Cada
/// ítem: id del producto y la ficha ya validada. Una ficha vacía borra la
/// que hubiera.
pub async fn guardar_en(conn: &libsql::Connection, fichas: &[(i64, FichaRepuesto)]) -> Result<(), String> {
    let ahora = tiempo::ahora_lima();
    for tanda in fichas.chunks(80) {
        let ids: String = tanda.iter().map(|(id, _)| id.to_string()).collect::<Vec<_>>().join(",");
        conn.execute(&format!("DELETE FROM producto_codigos_alternos WHERE producto_id IN ({})", ids), ())
            .await
            .map_err(|e| format!("no se pudieron guardar los códigos alternos: {}", e))?;
        let vacias: Vec<String> = tanda.iter().filter(|(_, f)| vacia(f)).map(|(id, _)| id.to_string()).collect();
        if !vacias.is_empty() {
            conn.execute(&format!("DELETE FROM producto_repuesto WHERE producto_id IN ({})", vacias.join(",")), ())
                .await
                .map_err(|e| format!("no se pudo quitar la ficha del repuesto: {}", e))?;
        }
        let llenas: Vec<&(i64, FichaRepuesto)> = tanda.iter().filter(|(_, f)| !vacia(f)).collect();
        if llenas.is_empty() {
            continue;
        }
        const COLUMNAS: usize = 7;
        let mut marcas = Vec::with_capacity(llenas.len());
        let mut valores: Vec<libsql::Value> = Vec::with_capacity(llenas.len() * COLUMNAS);
        let txt = |v: &Option<String>| v.clone().map(libsql::Value::Text).unwrap_or(libsql::Value::Null);
        for (i, (id, f)) in llenas.iter().enumerate() {
            let b = i * COLUMNAS;
            marcas.push(format!("({})", (1..=COLUMNAS).map(|n| format!("?{}", b + n)).collect::<Vec<_>>().join(", ")));
            valores.extend([
                libsql::Value::Integer(*id),
                txt(&f.marca),
                txt(&f.numero_parte),
                txt(&f.numero_parte.as_deref().map(clave_codigo)),
                txt(&f.ubicacion),
                f.garantia_dias.map(libsql::Value::Integer).unwrap_or(libsql::Value::Null),
                libsql::Value::Text(ahora.clone()),
            ]);
        }
        conn.execute(
            &format!(
                "INSERT INTO producto_repuesto (producto_id, marca, numero_parte, numero_parte_clave, ubicacion, garantia_dias, fecha_actualizacion)
                 VALUES {}
                 ON CONFLICT(producto_id) DO UPDATE SET
                    marca = excluded.marca, numero_parte = excluded.numero_parte, numero_parte_clave = excluded.numero_parte_clave,
                    ubicacion = excluded.ubicacion, garantia_dias = excluded.garantia_dias,
                    fecha_actualizacion = excluded.fecha_actualizacion",
                marcas.join(", ")
            ),
            libsql::params_from_iter(valores),
        )
        .await
        .map_err(|e| format!("no se pudo guardar la ficha del repuesto: {}", e))?;

        let codigos: Vec<(i64, &CodigoAlterno)> = llenas.iter().flat_map(|(id, f)| f.codigos.iter().map(move |c| (*id, c))).collect();
        for parte in codigos.chunks(100) {
            const COL: usize = 4;
            let mut marcas = Vec::with_capacity(parte.len());
            let mut valores: Vec<libsql::Value> = Vec::with_capacity(parte.len() * COL);
            for (i, (id, c)) in parte.iter().enumerate() {
                let b = i * COL;
                marcas.push(format!("(?{}, ?{}, ?{}, ?{})", b + 1, b + 2, b + 3, b + 4));
                valores.extend([
                    libsql::Value::Integer(*id),
                    libsql::Value::Text(c.codigo.clone()),
                    libsql::Value::Text(clave_codigo(&c.codigo)),
                    txt(&c.fabricante),
                ]);
            }
            conn.execute(
                &format!(
                    "INSERT OR IGNORE INTO producto_codigos_alternos (producto_id, codigo, codigo_clave, fabricante) VALUES {}",
                    marcas.join(", ")
                ),
                libsql::params_from_iter(valores),
            )
            .await
            .map_err(|e| format!("no se pudieron guardar los códigos alternos: {}", e))?;
        }
    }
    Ok(())
}

/// Las fichas guardadas, con sus códigos. Sin la migración 0037: vacío.
pub async fn leer_todas(conn: &libsql::Connection) -> Result<Vec<FichaRepuesto>, Fallo> {
    let mut fichas: Vec<FichaRepuesto> = Vec::new();
    let Ok(mut filas) = conn
        .query(
            "SELECT r.producto_id, r.marca, r.numero_parte, r.ubicacion, r.garantia_dias
             FROM producto_repuesto r JOIN productos p ON p.id = r.producto_id
             WHERE p.activo = 1 ORDER BY r.producto_id",
            (),
        )
        .await
    else {
        return Ok(fichas);
    };
    let mut posicion: HashMap<i64, usize> = HashMap::new();
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        posicion.insert(id, fichas.len());
        fichas.push(FichaRepuesto {
            producto_id: Some(id),
            marca: f.get(1).ok(),
            numero_parte: f.get(2).ok(),
            ubicacion: f.get(3).ok(),
            garantia_dias: f.get::<i64>(4).ok().filter(|d| *d > 0),
            codigos: Vec::new(),
        });
    }
    drop(filas);
    let mut filas = conn
        .query(
            "SELECT c.producto_id, c.codigo, c.fabricante FROM producto_codigos_alternos c
             JOIN producto_repuesto r ON r.producto_id = c.producto_id ORDER BY c.producto_id, c.id",
            (),
        )
        .await
        .map_err(e500)?;
    while let Some(f) = filas.next().await.map_err(e500)? {
        let id: i64 = f.get(0).unwrap_or_default();
        if let Some(i) = posicion.get(&id) {
            fichas[*i].codigos.push(CodigoAlterno { codigo: f.get(1).unwrap_or_default(), fabricante: f.get(2).ok() });
        }
    }
    Ok(fichas)
}

async fn exigir(conn: &libsql::Connection) -> Result<(), Fallo> {
    exigir_modulo(conn, MODULO_FICHA_REPUESTO, NOMBRE_MODULO).await
}

fn sin_migracion(e: String) -> Fallo {
    if e.contains("no such table") {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "La ficha del repuesto aún no está lista en este negocio. Intenta en unos minutos.".to_string(),
        )
    } else {
        e500(e)
    }
}

/// GET /repuestos/fichas — las fichas de los productos activos.
pub async fn todas(Extension(tenant): Extension<Arc<TenantDb>>) -> Result<Json<Vec<FichaRepuesto>>, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    Ok(Json(leer_todas(&conn).await?))
}

/// PUT /productos/:id/repuesto — crea o reemplaza la ficha del producto.
pub async fn guardar(
    Extension(tenant): Extension<Arc<TenantDb>>,
    Path(id): Path<i64>,
    Json(ficha): Json<FichaRepuesto>,
) -> Result<Json<FichaRepuesto>, Fallo> {
    let ficha = validar(ficha).map_err(malo)?;
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let mut filas = conn.query("SELECT 1 FROM productos WHERE id = ?1", libsql::params![id]).await.map_err(e500)?;
    if filas.next().await.map_err(e500)?.is_none() {
        return Err((StatusCode::NOT_FOUND, "El producto no existe.".into()));
    }
    drop(filas);
    let tx = conn.transaction().await.map_err(e500)?;
    guardar_en(&tx, &[(id, ficha.clone())]).await.map_err(sin_migracion)?;
    tx.commit().await.map_err(e500)?;
    Ok(Json(FichaRepuesto { producto_id: Some(id), ..ficha }))
}

/// DELETE /productos/:id/repuesto — el producto deja de tener ficha.
pub async fn quitar(Extension(tenant): Extension<Arc<TenantDb>>, Path(id): Path<i64>) -> Result<StatusCode, Fallo> {
    let conn = tenant.0.connect().map_err(e500)?;
    exigir(&conn).await?;
    let tx = conn.transaction().await.map_err(e500)?;
    let vacia = FichaRepuesto { producto_id: None, marca: None, numero_parte: None, ubicacion: None, garantia_dias: None, codigos: vec![] };
    guardar_en(&tx, &[(id, vacia)]).await.map_err(sin_migracion)?;
    tx.commit().await.map_err(e500)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn claves_de_codigo() {
        assert_eq!(clave_codigo("90915-YZZE1"), "90915YZZE1");
        assert_eq!(clave_codigo(" w 712/75 "), "W71275");
        assert_eq!(clave_codigo("PH-3614."), "PH3614");
    }

    #[test]
    fn lista_de_codigos_en_una_celda() {
        let c = codigos_desde_texto("PH3614, W712/75 (Mann); 90915-YZZE1 |  ");
        assert_eq!(c.len(), 3);
        assert_eq!(c[1], CodigoAlterno { codigo: "W712/75".into(), fabricante: Some("Mann".into()) });
        assert_eq!(c[2].codigo, "90915-YZZE1");
        assert!(codigos_desde_texto("").is_empty());
    }

    #[test]
    fn valida_y_limpia() {
        let f = validar(FichaRepuesto {
            producto_id: None,
            marca: Some("  Bosch ".into()),
            numero_parte: Some("0 986 af1 042".into()),
            ubicacion: Some("Estante  3".into()),
            garantia_dias: Some(0),
            codigos: vec![
                CodigoAlterno { codigo: "ph3614".into(), fabricante: Some("Fram".into()) },
                CodigoAlterno { codigo: "PH-3614".into(), fabricante: None },
                CodigoAlterno { codigo: "0986AF1042".into(), fabricante: None },
                CodigoAlterno { codigo: "  ".into(), fabricante: None },
            ],
        })
        .unwrap();
        assert_eq!(f.marca.as_deref(), Some("Bosch"));
        assert_eq!(f.numero_parte.as_deref(), Some("0 986 AF1 042"));
        assert_eq!(f.ubicacion.as_deref(), Some("Estante 3"));
        assert_eq!(f.garantia_dias, None);
        // El repetido y el igual al n.° de parte no se guardan.
        assert_eq!(f.codigos, vec![CodigoAlterno { codigo: "PH3614".into(), fabricante: Some("Fram".into()) }]);
        let mala = FichaRepuesto { garantia_dias: Some(5000), ..f.clone() };
        assert!(validar(mala).is_err());
        let corta = FichaRepuesto { numero_parte: Some("-".into()), ..f };
        assert!(validar(corta).is_err());
    }

    #[test]
    fn garantia_escrita() {
        assert_eq!(garantia_desde_texto(""), Ok(None));
        assert_eq!(garantia_desde_texto("Sin garantía"), Ok(None));
        assert_eq!(garantia_desde_texto("30"), Ok(Some(30)));
        assert_eq!(garantia_desde_texto("30 días"), Ok(Some(30)));
        assert_eq!(garantia_desde_texto("2 semanas"), Ok(Some(14)));
        assert_eq!(garantia_desde_texto("6 meses"), Ok(Some(180)));
        assert_eq!(garantia_desde_texto("1 año"), Ok(Some(365)));
        assert!(garantia_desde_texto("siempre").is_err());
        assert!(garantia_desde_texto("30 lunas").is_err());
    }

    #[test]
    fn fila_importada() {
        assert_eq!(ficha_desde_importada(&RepuestoImportado::default()), Ok(None));
        let f = ficha_desde_importada(&RepuestoImportado {
            marca: Some("Wix".into()),
            codigos_alternos: Some("51348, PH3614".into()),
            garantia: Some("3 meses".into()),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert_eq!(f.codigos.len(), 2);
        assert_eq!(f.garantia_dias, Some(90));
    }
}
