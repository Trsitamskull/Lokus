use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub provider: String, // "openai" | "gemini"
    pub openai_api_key: String,
    pub openai_model: String,
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub theme: String, // "dark" | "light"
    pub defensores: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            openai_api_key: String::new(),
            openai_model: "gpt-5-mini".to_string(),
            gemini_api_key: String::new(),
            gemini_model: "gemini-2.5-flash".to_string(),
            theme: "dark".to_string(),
            defensores: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatusResponse {
    pub configured: bool,
    pub provider: String,
    pub model: String,
    pub has_openai_key: bool,
    pub has_gemini_key: bool,
    pub masked_openai_key: String,
    pub masked_gemini_key: String,
}

/// Configuración segura enviada al frontend (nunca incluye API keys en texto claro).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPublica {
    pub provider: String,
    pub openai_model: String,
    pub gemini_model: String,
    pub theme: String,
    pub defensores: Vec<String>,
    pub status: AiStatusResponse,
}

/// Payload recibido desde el frontend para actualizar configuración sin exponer claves existentes.
#[derive(Debug, Clone, Deserialize)]
pub struct ActualizarConfig {
    pub provider: String,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub theme: String,
    #[serde(default)]
    pub clear_openai_key: bool,
    #[serde(default)]
    pub clear_gemini_key: bool,
}

pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    let len = chars.len();
    if len == 0 {
        String::new()
    } else if len <= 8 {
        "••••••••".to_string()
    } else {
        let prefix: String = chars[..5].iter().collect();
        let suffix: String = chars[len - 4..].iter().collect();
        format!("{}••••{}", prefix, suffix)
    }
}

pub fn normalizar_lista_defensores(lista: Vec<String>) -> Vec<String> {
    let mut vistos = HashSet::new();
    let mut salida = Vec::new();
    for item in lista {
        let limpio = item.split_whitespace().collect::<Vec<_>>().join(" ");
        if limpio.is_empty() {
            continue;
        }
        let clave = limpio.to_lowercase();
        if vistos.insert(clave) {
            salida.push(limpio);
        }
    }
    salida.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    salida
}

fn obtener_ruta_config() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Ok(cwd) = std::env::current_dir() {
            // Durante `cargo tauri dev` o `cargo test` desde src-tauri/, usar ../config
            if cwd.file_name().and_then(|n| n.to_str()) == Some("src-tauri") {
                if let Some(parent) = cwd.parent() {
                    let parent_cfg = parent.join("config");
                    if parent_cfg.exists() || fs::create_dir_all(&parent_cfg).is_ok() {
                        return parent_cfg.join("settings.json");
                    }
                }
            }
            let local_cfg = cwd.join("config");
            if local_cfg.exists() || fs::create_dir_all(&local_cfg).is_ok() {
                return local_cfg.join("settings.json");
            }
        }
    }

    let fallback = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lokus-audiencias");
    let _ = fs::create_dir_all(&fallback);
    fallback.join("settings.json")
}

fn intentar_migrar_config_python() -> Option<String> {
    let posibles = [
        PathBuf::from("config/config.py"),
        PathBuf::from("../config/config.py"),
    ];
    let re = Regex::new(r#"OPENAI_API_KEY\s*=\s*["'](sk-[^"']+)["']"#).ok()?;
    for ruta in &posibles {
        if let Ok(contenido) = fs::read_to_string(ruta) {
            if let Some(caps) = re.captures(&contenido) {
                if let Some(m) = caps.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
    }
    None
}

pub fn cargar_settings() -> AppSettings {
    let ruta = obtener_ruta_config();
    if let Ok(contenido) = fs::read_to_string(&ruta) {
        if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&contenido) {
            settings.defensores = normalizar_lista_defensores(settings.defensores);
            return settings;
        }
    }

    let mut default_settings = AppSettings::default();
    if let Some(legacy_key) = intentar_migrar_config_python() {
        default_settings.openai_api_key = legacy_key;
        let _ = guardar_settings(&default_settings);
    }
    default_settings
}

pub fn guardar_settings(settings: &AppSettings) -> Result<(), String> {
    let ruta = obtener_ruta_config();
    if let Some(parent) = ruta.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Error serializando configuración: {}", e))?;
    fs::write(&ruta, json).map_err(|e| format!("Error guardando configuración: {}", e))
}

pub fn estado_desde_settings(s: &AppSettings) -> AiStatusResponse {
    let has_openai = !s.openai_api_key.trim().is_empty();
    let has_gemini = !s.gemini_api_key.trim().is_empty();
    let configured = if s.provider == "gemini" {
        has_gemini
    } else {
        has_openai
    };
    let model = if s.provider == "gemini" {
        s.gemini_model.clone()
    } else {
        s.openai_model.clone()
    };

    AiStatusResponse {
        configured,
        provider: s.provider.clone(),
        model,
        has_openai_key: has_openai,
        has_gemini_key: has_gemini,
        masked_openai_key: mask_key(&s.openai_api_key),
        masked_gemini_key: mask_key(&s.gemini_api_key),
    }
}

pub fn config_publica_desde_settings(s: &AppSettings) -> ConfigPublica {
    ConfigPublica {
        provider: s.provider.clone(),
        openai_model: s.openai_model.clone(),
        gemini_model: s.gemini_model.clone(),
        theme: s.theme.clone(),
        defensores: s.defensores.clone(),
        status: estado_desde_settings(s),
    }
}

pub fn cargar_config_publica() -> ConfigPublica {
    let s = cargar_settings();
    config_publica_desde_settings(&s)
}

fn aplicar_actualizacion_en_memoria(mut actual: AppSettings, input: ActualizarConfig) -> AppSettings {
    actual.provider = if input.provider.trim() == "gemini" {
        "gemini".to_string()
    } else {
        "openai".to_string()
    };

    let openai_model = input.openai_model.trim();
    if !openai_model.is_empty() {
        actual.openai_model = openai_model.to_string();
    }

    let gemini_model = input.gemini_model.trim();
    if !gemini_model.is_empty() {
        actual.gemini_model = gemini_model.to_string();
    }

    actual.theme = if input.theme.trim() == "light" {
        "light".to_string()
    } else {
        "dark".to_string()
    };

    if input.clear_openai_key {
        actual.openai_api_key.clear();
    } else if let Some(ref key) = input.openai_api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            actual.openai_api_key = trimmed.to_string();
        }
    }

    if input.clear_gemini_key {
        actual.gemini_api_key.clear();
    } else if let Some(ref key) = input.gemini_api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            actual.gemini_api_key = trimmed.to_string();
        }
    }

    actual
}

pub fn actualizar_config(input: ActualizarConfig) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let nuevo = aplicar_actualizacion_en_memoria(actual, input);
    guardar_settings(&nuevo)?;
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn actualizar_tema(tema: &str) -> Result<ConfigPublica, String> {
    let mut actual = cargar_settings();
    actual.theme = if tema.trim() == "light" {
        "light".to_string()
    } else {
        "dark".to_string()
    };
    guardar_settings(&actual)?;
    Ok(config_publica_desde_settings(&actual))
}

fn agregar_defensor_en_memoria(mut actual: AppSettings, nombre: &str) -> Result<AppSettings, String> {
    let limpio = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpio.is_empty() {
        return Err("Ingresa el nombre del defensor público.".to_string());
    }
    actual.defensores.push(limpio);
    actual.defensores = normalizar_lista_defensores(actual.defensores);
    Ok(actual)
}

fn renombrar_defensor_en_memoria(
    mut actual: AppSettings,
    actual_nombre: &str,
    nuevo_nombre: &str,
) -> Result<AppSettings, String> {
    let target = actual_nombre.trim().to_lowercase();
    let nuevo = nuevo_nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if nuevo.is_empty() {
        return Err("El nuevo nombre del defensor no puede estar vacío.".to_string());
    }
    let nuevo_lower = nuevo.to_lowercase();
    let existe_otro = actual
        .defensores
        .iter()
        .any(|d| d.to_lowercase() == nuevo_lower && d.to_lowercase() != target);
    if existe_otro {
        return Err(format!("Ya existe otro defensor llamado '{}'.", nuevo));
    }

    let mut encontrado = false;
    for d in &mut actual.defensores {
        if d.to_lowercase() == target {
            *d = nuevo.clone();
            encontrado = true;
            break;
        }
    }
    if !encontrado {
        actual.defensores.push(nuevo);
    }
    actual.defensores = normalizar_lista_defensores(actual.defensores);
    Ok(actual)
}

fn eliminar_defensor_en_memoria(mut actual: AppSettings, nombre: &str) -> AppSettings {
    let target = nombre.trim().to_lowercase();
    actual
        .defensores
        .retain(|d| d.to_lowercase() != target);
    actual
}

pub fn agregar_defensor(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let nuevo = agregar_defensor_en_memoria(actual, nombre)?;
    guardar_settings(&nuevo)?;
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn renombrar_defensor(actual_nombre: &str, nuevo_nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let nuevo = renombrar_defensor_en_memoria(actual, actual_nombre, nuevo_nombre)?;
    guardar_settings(&nuevo)?;
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn eliminar_defensor(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let nuevo = eliminar_defensor_en_memoria(actual, nombre);
    guardar_settings(&nuevo)?;
    Ok(config_publica_desde_settings(&nuevo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_key_unicode_y_bordes() {
        assert_eq!(mask_key(""), "");
        assert_eq!(mask_key("   "), "");
        assert_eq!(mask_key("abc"), "••••••••");
        assert_eq!(mask_key("áéíóú123"), "••••••••");
        assert_eq!(mask_key("sk-test1234567890abcd"), "sk-te••••abcd");
        assert_eq!(mask_key("áéíóú67890wxyz"), "áéíóú••••wxyz");
    }

    #[test]
    fn test_config_publica_conserva_claves_y_no_las_expone() {
        let inicial = AppSettings {
            provider: "openai".into(),
            openai_api_key: "sk-secret-key-9999".into(),
            openai_model: "gpt-5-mini".into(),
            gemini_api_key: "AIzaSecretGemini1234".into(),
            gemini_model: "gemini-2.5-flash".into(),
            theme: "dark".into(),
            defensores: vec!["Ana Pérez".into()],
        };

        let actualizado = aplicar_actualizacion_en_memoria(
            inicial,
            ActualizarConfig {
                provider: "gemini".into(),
                openai_api_key: Some("".into()),
                openai_model: "gpt-4o-mini".into(),
                gemini_api_key: None,
                gemini_model: "gemini-2.5-flash".into(),
                theme: "light".into(),
                clear_openai_key: false,
                clear_gemini_key: false,
            },
        );

        assert_eq!(actualizado.openai_api_key, "sk-secret-key-9999");
        assert_eq!(actualizado.gemini_api_key, "AIzaSecretGemini1234");
        assert_eq!(actualizado.defensores, vec!["Ana Pérez".to_string()]);

        let publica = config_publica_desde_settings(&actualizado);
        let json_str = serde_json::to_string(&publica).unwrap();
        assert!(!json_str.contains("sk-secret-key-9999"));
        assert!(!json_str.contains("AIzaSecretGemini1234"));
    }

    #[test]
    fn test_crud_directorio_defensores() {
        let mut s = AppSettings::default();
        s = agregar_defensor_en_memoria(s, "  Carlos   Mendoza ").unwrap();
        s = agregar_defensor_en_memoria(s, "Ana Gómez").unwrap();
        s = agregar_defensor_en_memoria(s, "ana gómez").unwrap(); // sin duplicar
        assert_eq!(
            s.defensores,
            vec!["Ana Gómez".to_string(), "Carlos Mendoza".to_string()]
        );

        s = renombrar_defensor_en_memoria(s, "Ana Gómez", "Ana María Gómez").unwrap();
        assert_eq!(
            s.defensores,
            vec!["Ana María Gómez".to_string(), "Carlos Mendoza".to_string()]
        );

        // No debe permitir renombrar al mismo nombre de otro defensor
        assert!(renombrar_defensor_en_memoria(s.clone(), "Ana María Gómez", "carlos mendoza").is_err());

        s = eliminar_defensor_en_memoria(s, "Carlos Mendoza");
        assert_eq!(s.defensores, vec!["Ana María Gómez".to_string()]);
    }
}
