use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::Serialize;
use tauri::Manager;

const MAX_EXPORT_READ_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RutasApp {
    pub config_dir: PathBuf,
    pub archivos_dir: PathBuf,
    pub descargas_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchivoExportado {
    pub nombre: String,
    pub mime: String,
    pub contenido_b64: String,
}

/// Rutas principales registradas en tiempo de ejecución (obligatorias en Android, donde `dirs` retorna `None`).
static RUTAS_PRIORITARIAS: OnceLock<RutasApp> = OnceLock::new();

/// Rutas de respaldo en escritorio en caso de que el entorno no exponga XDG / carpetas del usuario.
static RUTAS_RESPALDO: OnceLock<RutasApp> = OnceLock::new();

/// Construye el árbol de almacenamiento persistente dentro del directorio privado de la app móvil.
pub fn rutas_desde_base_movil(base: &Path) -> RutasApp {
    RutasApp {
        config_dir: base.join("config"),
        archivos_dir: base.join("archivos_creados"),
        descargas_dir: base.join("exportaciones"),
    }
}

#[cfg(any(target_os = "android", test))]
fn asegurar_directorios(rutas: &RutasApp) {
    let _ = fs::create_dir_all(&rutas.config_dir);
    let _ = fs::create_dir_all(&rutas.archivos_dir);
    let _ = fs::create_dir_all(&rutas.descargas_dir);
}

/// Inicializa las rutas de almacenamiento a partir del `PathResolver` oficial de Tauri v2.
///
/// - En Android (`target_os = "android"`), registra rutas prioritarias dentro de `app_data_dir`,
///   garantizando lectura y escritura sin depender de `dirs::*` ni permisos de almacenamiento legado.
/// - En escritorio (Windows/Linux), mantiene intactas las rutas tradicionales y solo guarda
///   las rutas de Tauri como último respaldo si las variables del sistema no están disponibles.
pub fn inicializar_rutas_tauri(app: &tauri::App) {
    let resolver = app.path();
    let base = resolver
        .app_data_dir()
        .or_else(|_| resolver.app_config_dir())
        .or_else(|_| resolver.app_cache_dir());

    if let Ok(base_dir) = base {
        let rutas = rutas_desde_base_movil(&base_dir);
        #[cfg(target_os = "android")]
        {
            asegurar_directorios(&rutas);
            let _ = RUTAS_PRIORITARIAS.set(rutas);
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = RUTAS_RESPALDO.set(rutas);
        }
    }
}

pub fn ruta_config_prioritaria() -> Option<PathBuf> {
    RUTAS_PRIORITARIAS.get().map(|r| {
        let _ = fs::create_dir_all(&r.config_dir);
        r.config_dir.clone()
    })
}

pub fn ruta_archivos_prioritaria() -> Option<PathBuf> {
    RUTAS_PRIORITARIAS.get().map(|r| {
        let _ = fs::create_dir_all(&r.archivos_dir);
        r.archivos_dir.clone()
    })
}

pub fn ruta_descargas_prioritaria() -> Option<PathBuf> {
    RUTAS_PRIORITARIAS.get().map(|r| {
        let _ = fs::create_dir_all(&r.descargas_dir);
        r.descargas_dir.clone()
    })
}

pub fn ruta_config_respaldo() -> Option<PathBuf> {
    RUTAS_RESPALDO.get().map(|r| {
        let _ = fs::create_dir_all(&r.config_dir);
        r.config_dir.clone()
    })
}

pub fn ruta_archivos_respaldo() -> Option<PathBuf> {
    RUTAS_RESPALDO.get().map(|r| {
        let _ = fs::create_dir_all(&r.archivos_dir);
        r.archivos_dir.clone()
    })
}

pub fn ruta_descargas_respaldo() -> Option<PathBuf> {
    RUTAS_RESPALDO.get().map(|r| {
        let _ = fs::create_dir_all(&r.descargas_dir);
        r.descargas_dir.clone()
    })
}

pub fn inferir_mime_exportado(nombre: &str) -> &'static str {
    let lower = nombre.trim().to_ascii_lowercase();
    if lower.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if lower.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

/// Lee de forma segura un archivo previamente exportado dentro de `carpeta_base` para entregarlo
/// al puente nativo de Android (guardar en Descargas públicas vía MediaStore y abrir Hoja de Compartir).
pub fn leer_archivo_exportado_en(
    carpeta_base: &Path,
    ruta: &Path,
) -> Result<ArchivoExportado, String> {
    let base_canon = fs::canonicalize(carpeta_base)
        .map_err(|e| format!("No se pudo verificar la carpeta de exportaciones: {}", e))?;
    let ruta_canon = fs::canonicalize(ruta)
        .map_err(|e| format!("No se encontró el archivo exportado: {}", e))?;

    if !ruta_canon.starts_with(&base_canon) {
        return Err("Acceso denegado: el archivo está fuera de la carpeta de exportaciones.".to_string());
    }
    if !ruta_canon.is_file() {
        return Err("La ruta exportada no corresponde a un archivo válido.".to_string());
    }

    let meta = fs::metadata(&ruta_canon)
        .map_err(|e| format!("No se pudo inspeccionar el archivo exportado: {}", e))?;
    if meta.len() > MAX_EXPORT_READ_BYTES {
        return Err("El archivo exportado supera el límite permitido para compartir (50 MB).".to_string());
    }

    let nombre = ruta_canon
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "El archivo exportado no tiene un nombre válido.".to_string())?
        .to_string();

    let bytes = fs::read(&ruta_canon)
        .map_err(|e| format!("No se pudo leer el archivo exportado: {}", e))?;

    Ok(ArchivoExportado {
        mime: inferir_mime_exportado(&nombre).to_string(),
        nombre,
        contenido_b64: BASE64_STANDARD.encode(&bytes),
    })
}

pub fn leer_archivo_exportado(ruta: &str) -> Result<ArchivoExportado, String> {
    let limpio = ruta.trim();
    if limpio.is_empty() {
        return Err("La ruta del archivo exportado está vacía.".to_string());
    }
    let carpeta = crate::excel::carpeta_descargas()?;
    leer_archivo_exportado_en(&carpeta, Path::new(limpio))
}

/// Elimina de forma segura un archivo temporal dentro de `carpeta_base` (usado en Android tras
/// entregar la copia al almacenamiento público vía MediaStore, evitando acumulación en `app_data_dir`).
pub fn limpiar_archivo_exportado_en(carpeta_base: &Path, ruta: &Path) -> Result<(), String> {
    let base_canon = fs::canonicalize(carpeta_base)
        .map_err(|e| format!("No se pudo verificar la carpeta de exportaciones: {}", e))?;
    let ruta_canon = match fs::canonicalize(ruta) {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };

    if !ruta_canon.starts_with(&base_canon) {
        return Err(
            "Acceso denegado: no se puede eliminar un archivo fuera de la carpeta de exportaciones."
                .to_string(),
        );
    }
    if !ruta_canon.is_file() {
        return Err("La ruta indicada no corresponde a un archivo válido.".to_string());
    }

    fs::remove_file(&ruta_canon)
        .map_err(|e| format!("No se pudo limpiar el archivo temporal exportado: {}", e))
}

/// Limpia únicamente archivos temporales dentro de la carpeta prioritaria de staging (Android).
/// En escritorio (`ruta_descargas_prioritaria() == None`) no toca la carpeta Descargas del usuario.
pub fn limpiar_archivo_exportado(ruta: &str) -> Result<(), String> {
    let limpio = ruta.trim();
    if limpio.is_empty() {
        return Ok(());
    }
    let Some(carpeta) = ruta_descargas_prioritaria() else {
        return Ok(());
    };
    limpiar_archivo_exportado_en(&carpeta, Path::new(limpio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn dir_temporal(prefijo: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "lokus_rutas_{}_{}_{}",
            prefijo,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("crear dir temporal");
        dir
    }

    #[test]
    fn test_rutas_desde_base_movil_estructura_completa() {
        let base = dir_temporal("movil_base");
        let rutas = rutas_desde_base_movil(&base);
        asegurar_directorios(&rutas);

        assert_eq!(rutas.config_dir, base.join("config"));
        assert_eq!(rutas.archivos_dir, base.join("archivos_creados"));
        assert_eq!(rutas.descargas_dir, base.join("exportaciones"));
        assert!(rutas.config_dir.is_dir());
        assert!(rutas.archivos_dir.is_dir());
        assert!(rutas.descargas_dir.is_dir());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_inferir_mime_exportado_cubre_formatos_lokus() {
        assert_eq!(
            inferir_mime_exportado("enero_2026.xlsx"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(
            inferir_mime_exportado("ENERO_2026.XLSX"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(
            inferir_mime_exportado("lokus_estadisticas_20260101.lokus.json"),
            "application/json"
        );
        assert_eq!(
            inferir_mime_exportado("lokus_aprendizaje_20260101.json"),
            "application/json"
        );
        assert_eq!(
            inferir_mime_exportado("lokus_respaldo_20260101.lokus"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_leer_archivo_exportado_en_valida_limites_y_bloquea_path_traversal() {
        let raiz = dir_temporal("export_sec");
        let carpeta_exp = raiz.join("exportaciones");
        fs::create_dir_all(&carpeta_exp).expect("crear exportaciones");

        let archivo_ok = carpeta_exp.join("planilla_enero.xlsx");
        fs::write(&archivo_ok, b"PK\x03\x04contenido-excel").expect("escribir xlsx");

        let externo = raiz.join("secreto.json");
        fs::write(&externo, br#"{"api_key":"secreta"}"#).expect("escribir secreto");

        let leido = leer_archivo_exportado_en(&carpeta_exp, &archivo_ok)
            .expect("debe leer archivo dentro de exportaciones");
        assert_eq!(leido.nombre, "planilla_enero.xlsx");
        assert_eq!(
            leido.mime,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        let decodificado = BASE64_STANDARD
            .decode(&leido.contenido_b64)
            .expect("base64 válido");
        assert_eq!(decodificado, b"PK\x03\x04contenido-excel");

        // Bloquear intento de traversal con ../
        let intento_traversal = carpeta_exp.join("..").join("secreto.json");
        let err = leer_archivo_exportado_en(&carpeta_exp, &intento_traversal)
            .expect_err("debe bloquear path traversal");
        assert!(err.contains("Acceso denegado"), "{}", err);

        // Bloquear ruta directa externa
        let err_ext = leer_archivo_exportado_en(&carpeta_exp, &externo)
            .expect_err("debe bloquear lectura externa");
        assert!(err_ext.contains("Acceso denegado"), "{}", err_ext);

        // Bloquear directorio en vez de archivo
        let err_dir = leer_archivo_exportado_en(&carpeta_exp, &carpeta_exp)
            .expect_err("debe rechazar directorios");
        assert!(err_dir.contains("archivo válido"), "{}", err_dir);

        // Limpieza segura bloquea traversal y elimina únicamente archivo permitido
        let err_limpiar_ext = limpiar_archivo_exportado_en(&carpeta_exp, &intento_traversal)
            .expect_err("limpieza debe bloquear path traversal");
        assert!(err_limpiar_ext.contains("Acceso denegado"), "{}", err_limpiar_ext);
        assert!(externo.exists());

        limpiar_archivo_exportado_en(&carpeta_exp, &archivo_ok)
            .expect("debe limpiar archivo temporal dentro de exportaciones");
        assert!(!archivo_ok.exists());
        // Llamada idempotente si ya fue borrado
        limpiar_archivo_exportado_en(&carpeta_exp, &archivo_ok)
            .expect("limpieza repetida debe ser idempotente");

        let _ = fs::remove_dir_all(&raiz);
    }
}
