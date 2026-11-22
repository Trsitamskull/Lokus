use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::estadistica_ia::juzgados_base;
use crate::models::{normalizar_clave_catalogo, normalizar_clave_defensor, TIPOS_AUDIENCIA};

fn default_true() -> bool {
    true
}

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
    pub juzgados: Vec<String>,
    pub tipos_audiencia: Vec<String>,
    #[serde(default = "default_true")]
    pub consolidacion_ia_activa: bool,
    #[serde(default = "default_true")]
    pub usar_resumen_consolidado: bool,
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
            juzgados: Vec::new(),
            tipos_audiencia: Vec::new(),
            consolidacion_ia_activa: true,
            usar_resumen_consolidado: true,
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
    pub juzgados: Vec<String>,
    pub tipos_audiencia: Vec<String>,
    #[serde(default = "default_true")]
    pub consolidacion_ia_activa: bool,
    #[serde(default = "default_true")]
    pub usar_resumen_consolidado: bool,
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
    #[serde(default)]
    pub consolidacion_ia_activa: Option<bool>,
    #[serde(default)]
    pub usar_resumen_consolidado: Option<bool>,
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

pub(crate) fn es_nombre_defensor_mas_informativo(candidato: &str, actual: &str) -> bool {
    let puntaje = |s: &str| -> (bool, usize, bool, bool) {
        let mixed = s.chars().any(|c| c.is_uppercase()) && s.chars().any(|c| c.is_lowercase());
        let len = s.chars().count();
        let tiene_tilde = s
            .chars()
            .any(|c| "áéíóúüñÁÉÍÓÚÜÑ".contains(c));
        let tiene_mayus = s.chars().any(|c| c.is_uppercase());
        (mixed, len, tiene_tilde, tiene_mayus)
    };
    puntaje(candidato) > puntaje(actual)
}

pub fn normalizar_lista_defensores(lista: Vec<String>) -> Vec<String> {
    let mut mapa: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for item in lista {
        let limpio = item.split_whitespace().collect::<Vec<_>>().join(" ");
        if limpio.is_empty() {
            continue;
        }
        let clave_norm = normalizar_clave_defensor(&limpio);
        let clave = if clave_norm.is_empty() {
            limpio.to_lowercase()
        } else {
            clave_norm
        };
        match mapa.get_mut(&clave) {
            Some(existente) => {
                if es_nombre_defensor_mas_informativo(&limpio, existente) {
                    *existente = limpio;
                }
            }
            None => {
                mapa.insert(clave, limpio);
            }
        }
    }
    let mut salida: Vec<String> = mapa.into_values().collect();
    salida.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    salida
}

pub fn normalizar_lista_catalogo_usuario(lista: Vec<String>, defaults: &[String]) -> Vec<String> {
    let mut claves_default: HashSet<String> = defaults
        .iter()
        .map(|d| normalizar_clave_catalogo(d))
        .filter(|c| !c.is_empty())
        .collect();
    claves_default.insert("otra".to_string());

    let mut vistos = HashSet::new();
    let mut salida = Vec::new();
    for item in lista {
        let limpio = item.split_whitespace().collect::<Vec<_>>().join(" ");
        if limpio.is_empty() {
            continue;
        }
        let clave = normalizar_clave_catalogo(&limpio);
        if clave.is_empty() || claves_default.contains(&clave) {
            continue;
        }
        if vistos.insert(clave) {
            salida.push(limpio);
        }
    }
    salida.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    salida
}

fn tipos_audiencia_default_strings() -> Vec<String> {
    TIPOS_AUDIENCIA
        .iter()
        .filter(|&&t| t != "Otra")
        .map(|&t| t.to_string())
        .collect()
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

/// Ruta de un archivo auxiliar ubicado en la misma carpeta de configuración que `settings.json`
/// (`config/` en desarrollo, carpeta de configuración del sistema en producción).
pub fn ruta_en_carpeta_config(nombre_archivo: &str) -> PathBuf {
    obtener_ruta_config().with_file_name(nombre_archivo)
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

pub fn normalizar_modelo_gemini(modelo: &str) -> String {
    let limpio = modelo.trim().trim_start_matches("models/");
    match limpio {
        "" | "gemini-2.0-flash" | "gemini-2.0-flash-exp" | "gemini-2.0-flash-lite"
        | "gemini-1.5-flash" | "gemini-1.5-pro" => "gemini-2.5-flash".to_string(),
        otro => otro.to_string(),
    }
}

pub fn normalizar_modelo_openai(modelo: &str) -> String {
    let limpio = modelo.trim();
    if limpio.is_empty() {
        "gpt-5-mini".to_string()
    } else {
        limpio.to_string()
    }
}

pub fn cargar_settings() -> AppSettings {
    let ruta = obtener_ruta_config();
    if let Ok(contenido) = fs::read_to_string(&ruta) {
        if let Ok(mut settings) = serde_json::from_str::<AppSettings>(&contenido) {
            let defensores_antes = settings.defensores.clone();
            settings.defensores = normalizar_lista_defensores(settings.defensores);
            let tipos_default = tipos_audiencia_default_strings();
            let juzgados_antes = settings.juzgados.clone();
            let tipos_antes = settings.tipos_audiencia.clone();
            settings.juzgados = normalizar_lista_catalogo_usuario(
                std::mem::take(&mut settings.juzgados),
                juzgados_base(),
            );
            settings.tipos_audiencia = normalizar_lista_catalogo_usuario(
                std::mem::take(&mut settings.tipos_audiencia),
                &tipos_default,
            );
            let cambio_catalogos = settings.defensores != defensores_antes
                || settings.juzgados != juzgados_antes
                || settings.tipos_audiencia != tipos_antes;
            let gemini_norm = normalizar_modelo_gemini(&settings.gemini_model);
            let openai_norm = normalizar_modelo_openai(&settings.openai_model);
            let cambio_modelo =
                gemini_norm != settings.gemini_model || openai_norm != settings.openai_model;
            settings.gemini_model = gemini_norm;
            settings.openai_model = openai_norm;
            if cambio_modelo || cambio_catalogos {
                let _ = guardar_settings(&settings);
            }
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
        juzgados: s.juzgados.clone(),
        tipos_audiencia: s.tipos_audiencia.clone(),
        consolidacion_ia_activa: s.consolidacion_ia_activa,
        usar_resumen_consolidado: s.usar_resumen_consolidado,
        status: estado_desde_settings(s),
    }
}

fn agregar_juzgado_en_memoria(
    mut actual: AppSettings,
    nombre: &str,
) -> Result<(AppSettings, Option<String>), String> {
    let limpio = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpio.is_empty() {
        return Err("Ingresa el nombre del juzgado o despacho.".to_string());
    }
    let clave = normalizar_clave_catalogo(&limpio);
    if clave.is_empty() || clave == "otra" {
        return Err("Ese nombre no es válido para la lista de juzgados.".to_string());
    }
    let ya_en_default = juzgados_base()
        .iter()
        .any(|d| normalizar_clave_catalogo(d) == clave);
    let ya_en_usuario = actual
        .juzgados
        .iter()
        .any(|u| normalizar_clave_catalogo(u) == clave);
    if ya_en_default || ya_en_usuario {
        actual.juzgados = normalizar_lista_catalogo_usuario(actual.juzgados, juzgados_base());
        return Ok((actual, None));
    }
    actual.juzgados.push(limpio.clone());
    actual.juzgados = normalizar_lista_catalogo_usuario(actual.juzgados, juzgados_base());
    Ok((actual, Some(limpio)))
}

fn eliminar_juzgado_en_memoria(
    mut actual: AppSettings,
    nombre: &str,
) -> (AppSettings, Option<String>) {
    let target = normalizar_clave_catalogo(nombre);
    let mut eliminado = None;
    actual.juzgados.retain(|j| {
        if normalizar_clave_catalogo(j) == target {
            if eliminado.is_none() {
                eliminado = Some(j.clone());
            }
            false
        } else {
            true
        }
    });
    (actual, eliminado)
}

fn agregar_tipo_audiencia_en_memoria(
    mut actual: AppSettings,
    nombre: &str,
) -> Result<(AppSettings, Option<String>), String> {
    let limpio = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpio.is_empty() {
        return Err("Ingresa el nombre del tipo de audiencia.".to_string());
    }
    let clave = normalizar_clave_catalogo(&limpio);
    if clave.is_empty() || clave == "otra" {
        return Err("Ese nombre no es válido para la lista de tipos de audiencia.".to_string());
    }
    let defaults = tipos_audiencia_default_strings();
    let ya_en_default = defaults
        .iter()
        .any(|d| normalizar_clave_catalogo(d) == clave);
    let ya_en_usuario = actual
        .tipos_audiencia
        .iter()
        .any(|u| normalizar_clave_catalogo(u) == clave);
    if ya_en_default || ya_en_usuario {
        actual.tipos_audiencia =
            normalizar_lista_catalogo_usuario(actual.tipos_audiencia, &defaults);
        return Ok((actual, None));
    }
    actual.tipos_audiencia.push(limpio.clone());
    actual.tipos_audiencia = normalizar_lista_catalogo_usuario(actual.tipos_audiencia, &defaults);
    Ok((actual, Some(limpio)))
}

fn eliminar_tipo_audiencia_en_memoria(
    mut actual: AppSettings,
    nombre: &str,
) -> (AppSettings, Option<String>) {
    let target = normalizar_clave_catalogo(nombre);
    let mut eliminado = None;
    actual.tipos_audiencia.retain(|t| {
        if normalizar_clave_catalogo(t) == target {
            if eliminado.is_none() {
                eliminado = Some(t.clone());
            }
            false
        } else {
            true
        }
    });
    (actual, eliminado)
}

pub fn agregar_juzgado(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let (nuevo, agregado) = agregar_juzgado_en_memoria(actual, nombre)?;
    if agregado.is_some() {
        guardar_settings(&nuevo)?;
        crate::uso_catalogos::registrar_cambio_lista_usuario_actual(
            &nuevo,
            true,
            agregado.as_deref(),
            None,
        );
    }
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn eliminar_juzgado(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let (nuevo, eliminado) = eliminar_juzgado_en_memoria(actual, nombre);
    if eliminado.is_some() {
        guardar_settings(&nuevo)?;
        crate::uso_catalogos::registrar_cambio_lista_usuario_actual(
            &nuevo,
            true,
            None,
            eliminado.as_deref(),
        );
    }
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn agregar_tipo_audiencia(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let (nuevo, agregado) = agregar_tipo_audiencia_en_memoria(actual, nombre)?;
    if agregado.is_some() {
        guardar_settings(&nuevo)?;
        crate::uso_catalogos::registrar_cambio_lista_usuario_actual(
            &nuevo,
            false,
            agregado.as_deref(),
            None,
        );
    }
    Ok(config_publica_desde_settings(&nuevo))
}

pub fn eliminar_tipo_audiencia(nombre: &str) -> Result<ConfigPublica, String> {
    let actual = cargar_settings();
    let (nuevo, eliminado) = eliminar_tipo_audiencia_en_memoria(actual, nombre);
    if eliminado.is_some() {
        guardar_settings(&nuevo)?;
        crate::uso_catalogos::registrar_cambio_lista_usuario_actual(
            &nuevo,
            false,
            None,
            eliminado.as_deref(),
        );
    }
    Ok(config_publica_desde_settings(&nuevo))
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
        actual.openai_model = normalizar_modelo_openai(openai_model);
    }

    let gemini_model = input.gemini_model.trim();
    if !gemini_model.is_empty() {
        actual.gemini_model = normalizar_modelo_gemini(gemini_model);
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

    if let Some(val) = input.consolidacion_ia_activa {
        actual.consolidacion_ia_activa = val;
    }
    if let Some(val) = input.usar_resumen_consolidado {
        actual.usar_resumen_consolidado = val;
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
    let target_lower = actual_nombre.trim().to_lowercase();
    let target_clave = normalizar_clave_defensor(actual_nombre);
    let nuevo = nuevo_nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if nuevo.is_empty() {
        return Err("El nuevo nombre del defensor no puede estar vacío.".to_string());
    }
    let nuevo_lower = nuevo.to_lowercase();
    let nuevo_clave = normalizar_clave_defensor(&nuevo);

    let idx_target = actual
        .defensores
        .iter()
        .position(|d| d.to_lowercase() == target_lower)
        .or_else(|| {
            if target_clave.is_empty() {
                None
            } else {
                actual
                    .defensores
                    .iter()
                    .position(|d| normalizar_clave_defensor(d) == target_clave)
            }
        });

    let existe_otro = actual.defensores.iter().enumerate().any(|(idx, d)| {
        if Some(idx) == idx_target {
            return false;
        }
        d.to_lowercase() == nuevo_lower
            || (!nuevo_clave.is_empty() && normalizar_clave_defensor(d) == nuevo_clave)
    });
    if existe_otro {
        return Err(format!("Ya existe otro defensor llamado '{}'.", nuevo));
    }

    if let Some(idx) = idx_target {
        actual.defensores[idx] = nuevo;
    } else {
        actual.defensores.push(nuevo);
    }
    actual.defensores = normalizar_lista_defensores(actual.defensores);
    Ok(actual)
}

fn eliminar_defensor_en_memoria(mut actual: AppSettings, nombre: &str) -> AppSettings {
    let target_lower = nombre.trim().to_lowercase();
    let target_clave = normalizar_clave_defensor(nombre);
    actual.defensores.retain(|d| {
        d.to_lowercase() != target_lower
            && (target_clave.is_empty() || normalizar_clave_defensor(d) != target_clave)
    });
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
            juzgados: vec!["Juzgado 1 Penal".into()],
            tipos_audiencia: vec!["Audiencia especial".into()],
            consolidacion_ia_activa: true,
            usar_resumen_consolidado: true,
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
                consolidacion_ia_activa: Some(false),
                usar_resumen_consolidado: None,
            },
        );

        assert_eq!(actualizado.openai_api_key, "sk-secret-key-9999");
        assert_eq!(actualizado.gemini_api_key, "AIzaSecretGemini1234");
        assert_eq!(actualizado.defensores, vec!["Ana Pérez".to_string()]);
        assert!(!actualizado.consolidacion_ia_activa);
        assert!(actualizado.usar_resumen_consolidado);

        let publica = config_publica_desde_settings(&actualizado);
        assert!(!publica.consolidacion_ia_activa);
        assert!(publica.usar_resumen_consolidado);
        let json_str = serde_json::to_string(&publica).unwrap();
        assert!(!json_str.contains("sk-secret-key-9999"));
        assert!(!json_str.contains("AIzaSecretGemini1234"));

        // Deserialización de settings legacy sin los nuevos flags debe defaults = true
        let legacy: AppSettings = serde_json::from_str(r#"{"provider":"openai"}"#).unwrap();
        assert!(legacy.consolidacion_ia_activa);
        assert!(legacy.usar_resumen_consolidado);
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

        // Deduplicación por clave sin honoríficos conservando el nombre más informativo
        let dedup = normalizar_lista_defensores(vec![
            "JOSE DANIEL".into(),
            "Dr. José Daniel".into(),
            "Defensor Público José Daniel".into(),
        ]);
        assert_eq!(dedup.len(), 1);
        assert!(dedup[0].contains("José Daniel"));

        // Renombrar dentro de la misma clave canonizada y con nuevo apellido
        let mut s_jd = AppSettings::default();
        s_jd = agregar_defensor_en_memoria(s_jd, "Dr. José Daniel").unwrap();
        s_jd = renombrar_defensor_en_memoria(s_jd, "Dr. José Daniel", "José Daniel").unwrap();
        assert_eq!(s_jd.defensores, vec!["José Daniel".to_string()]);
        s_jd = renombrar_defensor_en_memoria(s_jd, "José Daniel", "José Daniel Martínez").unwrap();
        assert_eq!(s_jd.defensores, vec!["José Daniel Martínez".to_string()]);
        // Conflicto entre "Dr. Luis Mora" y "luis mora" contra otro defensor existente
        s_jd = agregar_defensor_en_memoria(s_jd, "Dra. Ana Pérez").unwrap();
        assert!(renombrar_defensor_en_memoria(s_jd.clone(), "José Daniel Martínez", "Ana Pérez").is_err());
    }

    #[test]
    fn test_catalogos_usuario_no_duplican_predeterminados_con_tildes_o_mayusculas() {
        let mut s = AppSettings::default();
        // Variante sin tilde de un tipo oficial -> no debe duplicarse en tipos_audiencia
        let (s1, agr1) =
            agregar_tipo_audiencia_en_memoria(s, "audiencia de acusacion").unwrap();
        assert!(agr1.is_none());
        assert!(s1.tipos_audiencia.is_empty());

        // Variante sin tilde de juzgado base -> no debe duplicarse en juzgados
        let (s2, agr2) = agregar_juzgado_en_memoria(
            s1,
            "Juzgado 1 Penal Municipal con Funcion de Control de Garantias de Valledupar",
        )
        .unwrap();
        assert!(agr2.is_none());
        assert!(s2.juzgados.is_empty());

        // Variante jurídica distinta ("Juzgado 2 Penal" vs "Juzgado 2 Penal con Funciones de Conocimiento") SÍ es aceptada y separada
        let (s3, agr3) = agregar_juzgado_en_memoria(s2, "Juzgado 2 Penal").unwrap();
        assert_eq!(agr3.as_deref(), Some("Juzgado 2 Penal"));
        let (s4, agr4) = agregar_juzgado_en_memoria(
            s3,
            "Juzgado 2 Penal con Funciones de Conocimiento",
        )
        .unwrap();
        assert_eq!(
            agr4.as_deref(),
            Some("Juzgado 2 Penal con Funciones de Conocimiento")
        );
        // Mismo nombre solo cambiando tildes/mayúsculas NO duplica
        let (s5, agr5) = agregar_juzgado_en_memoria(s4, "  juzgado 2 penal  ").unwrap();
        assert!(agr5.is_none());
        assert_eq!(s5.juzgados.len(), 2);

        // "Segundo" y "2" se mantienen separados
        let (s6, agr6) = agregar_juzgado_en_memoria(s5, "Juzgado Segundo Penal").unwrap();
        assert_eq!(agr6.as_deref(), Some("Juzgado Segundo Penal"));
        assert_eq!(s6.juzgados.len(), 3);

        // Eliminar sin tildes funciona
        let (s7, elim) = eliminar_juzgado_en_memoria(s6, "juzgado segundo penal");
        assert_eq!(elim.as_deref(), Some("Juzgado Segundo Penal"));
        assert_eq!(s7.juzgados.len(), 2);

        s = s7;
        assert!(agregar_tipo_audiencia_en_memoria(s, "Otra").is_err());
    }
}
