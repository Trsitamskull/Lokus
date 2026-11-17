use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub provider: String, // "openai" | "gemini"
    pub openai_api_key: String,
    pub openai_model: String,
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub theme: String, // "dark" | "light"
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

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() <= 8 {
        if trimmed.is_empty() {
            String::new()
        } else {
            "••••••••".to_string()
        }
    } else {
        format!("{}••••{}", &trimmed[..5], &trimmed[trimmed.len() - 4..])
    }
}

fn obtener_ruta_config() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let parent_cfg = cwd.parent().map(|p| p.join("config"));
        if let Some(ref p) = parent_cfg {
            if p.exists() {
                return p.join("settings.json");
            }
        }
        let local_cfg = cwd.join("config");
        if local_cfg.exists() || fs::create_dir_all(&local_cfg).is_ok() {
            return local_cfg.join("settings.json");
        }
    }

    let fallback = dirs::config_dir()
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
        if let Ok(settings) = serde_json::from_str::<AppSettings>(&contenido) {
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

pub fn obtener_estado_ia() -> AiStatusResponse {
    let s = cargar_settings();
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
        provider: s.provider,
        model,
        has_openai_key: has_openai,
        has_gemini_key: has_gemini,
        masked_openai_key: mask_key(&s.openai_api_key),
        masked_gemini_key: mask_key(&s.gemini_api_key),
    }
}
