//! Motor de sincronización automática en la nube con Google Drive (`appDataFolder`).
//!
//! Sincroniza entre equipos (escritorio y Android) usando la carpeta oculta y privada
//! `appDataFolder` del propio Google Drive del usuario:
//! 1. Memoria de autoaprendizaje IA (`ia_memoria_<instancia_id>.json`) con fusión idempotente.
//! 2. Historial de uso de catálogos (`uso_<instancia_id>.json`) con fusión `max` por instancia.
//! 3. Planillas Excel (`planilla_<id>.xlsx`), estructura/colores/orden de carpetas, resúmenes
//!    estadísticos archivados y listas de usuario (defensores, juzgados y tipos de audiencia)
//!    mediante `lokus_sync_manifest.json`.
//!
//! Seguridad: NUNCA sube `openai_api_key`, `gemini_api_key`, `access_token` ni `refresh_token`
//! a Google Drive. Las credenciales OAuth locales viven exclusivamente en `config/drive_sync.json`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::aprendizaje::{
    fusionar_memoria_ia_desde_paquete_en, memoria_tiene_actividad,
    preparar_snapshot_memoria_para_paquete_en, ruta_memoria, MemoriaAprendizaje,
};
use crate::config::{
    cargar_settings, guardar_settings, normalizar_lista_catalogo_usuario,
    normalizar_lista_defensores, ruta_en_carpeta_config, AppSettings,
};
use crate::estadistica_ia::juzgados_base;
use crate::excel::{
    asegurar_id_ruta, cargar_registro_estadisticas_en, crear_respaldo_en, explorar_carpeta_en,
    generar_id_planilla, guardar_registro_estadisticas_en, invalidar_cache, invalidar_priors_en,
    leer_archivo_en, obtener_carpeta_archivos, sanear_archivo_excel_en, sanitizar_nombre_archivo,
    sanitizar_ruta_carpeta, segmentos_ruta_relativa, upsert_resumen_archivado,
    validar_color_carpeta,
};
use crate::ingesta::sha256_hex;
use crate::models::{ResumenPlanillaArchivada, TIPOS_AUDIENCIA};
use crate::uso_catalogos::{
    asegurar_uso_inicializado_en, cargar_uso_con_lock_en, fusionar_snapshot_instancia_en,
    ruta_uso_catalogos, UsoCatalogosInstancia,
};

pub const NOMBRE_ARCHIVO_DRIVE_SYNC: &str = "drive_sync.json";
pub const GOOGLE_DRIVE_SCOPE: &str =
    "https://www.googleapis.com/auth/drive.appdata openid email profile";
pub const ANDROID_REDIRECT_URI: &str = "com.josebustamante.gestoraudiencias:/oauth2redirect";

const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const GOOGLE_DRIVE_FILES_API: &str = "https://www.googleapis.com/drive/v3/files";
const GOOGLE_DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3/files";
const NOMBRE_MANIFIESTO_NUBE: &str = "lokus_sync_manifest.json";
const FORMATO_MANIFIESTO_NUBE: u32 = 1;
const MIME_JSON: &str = "application/json";
const MIME_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const TIMEOUT_OAUTH_SECS: u64 = 180;
const TTL_OAUTH_ANDROID_SECS: i64 = 900;
const MAX_ARCHIVO_REMOTO_BYTES: usize = 64 * 1024 * 1024;

static BLOQUEO_DRIVE_CONFIG: Mutex<()> = Mutex::new(());
static CONTADOR_SYNC: AtomicU64 = AtomicU64::new(1);
static SYNC_EN_CURSO: AtomicBool = AtomicBool::new(false);
static PENDIENTE_OAUTH_ANDROID: Mutex<Option<EstadoOauthPendiente>> = Mutex::new(None);

#[derive(Debug, Clone)]
struct EstadoOauthPendiente {
    code_verifier: String,
    state: String,
    client_id: String,
    redirect_uri: String,
    creado_epoch: i64,
}

struct SyncLockGuard;

impl SyncLockGuard {
    fn adquirir() -> Result<Self, String> {
        if SYNC_EN_CURSO
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Ya hay una sincronización con Google Drive en curso.".to_string());
        }
        Ok(Self)
    }
}

impl Drop for SyncLockGuard {
    fn drop(&mut self) {
        SYNC_EN_CURSO.store(false, Ordering::SeqCst);
    }
}

/// Estado local persistido en `config/drive_sync.json` (nunca se sube a Google Drive).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ConfigDriveSyncLocal {
    pub client_id_custom: String,
    pub auto_sync: bool,
    pub email: String,
    pub nombre: String,
    pub foto_url: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_epoch: i64,
    pub ultima_sincronizacion: String,
    pub ultimo_error: String,
    /// Mapa `clave -> "<hash_local>|<hash_remoto>"` (o `<hash>` único) de la última sincronización
    /// exitosa para detectar cambios concurrentes sin falsos conflictos ni ping-pong.
    pub hashes_ult_sync: BTreeMap<String, String>,
}

/// Estado seguro enviado al frontend (nunca expone tokens OAuth).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EstadoDriveSync {
    pub conectado: bool,
    pub email: String,
    pub nombre: String,
    pub foto_url: String,
    pub auto_sync: bool,
    pub client_id_configurado: bool,
    pub tiene_client_id_embebido: bool,
    pub client_id_custom: String,
    pub ultima_sincronizacion: String,
    pub ultimo_error: String,
}

/// Resumen estructurado del resultado de una sincronización incremental con Google Drive.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ResultadoDriveSync {
    pub sincronizado_en: String,
    pub planillas_subidas: usize,
    pub planillas_descargadas: usize,
    pub conflictos_respaldados: usize,
    pub carpetas_creadas: usize,
    pub resumenes_fusionados: usize,
    pub ia_subida: bool,
    pub ia_cambios_fusionados: usize,
    pub uso_subido: bool,
    pub uso_instancias_fusionadas: usize,
    pub config_fusionada: bool,
    pub sin_cambios: bool,
    pub resumen_mensaje: String,
}

/// Datos necesarios para abrir el flujo OAuth2 PKCE desde Android y validar el deep link de retorno.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InicioAuthAndroid {
    pub auth_url: String,
    pub state: String,
    pub redirect_uri: String,
}

/// Metadatos mínimos de un archivo remoto en `appDataFolder`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ArchivoRemoto {
    pub id: String,
    pub name: String,
    pub modified_time: String,
    pub size: u64,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstracción de transporte remoto para desacoplar la lógica de sincronización de Google Drive
/// y permitir pruebas deterministas en memoria sin red.
pub trait SyncTransport: Send + Sync {
    fn listar_archivos<'a>(&'a self) -> BoxFuture<'a, Result<Vec<ArchivoRemoto>, String>>;

    fn descargar_archivo<'a>(&'a self, file_id: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>>;

    fn subir_o_actualizar<'a>(
        &'a self,
        nombre: &'a str,
        mime_type: &'a str,
        bytes: &'a [u8],
        file_id_existente: Option<&'a str>,
    ) -> BoxFuture<'a, Result<ArchivoRemoto, String>>;

    fn eliminar_archivo<'a>(&'a self, file_id: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
struct EntradaPlanillaNube {
    id: String,
    ruta_relativa: String,
    sha256: String,
    drive_file_name: String,
    actualizada_en: String,
    defensor: String,
    num_registros: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
struct ListasSincronizadas {
    defensores: Vec<String>,
    juzgados: Vec<String>,
    tipos_audiencia: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ManifiestoSyncNube {
    app: String,
    formato: u32,
    actualizada_en: String,
    carpetas: Vec<String>,
    orden_carpetas: Vec<String>,
    colores_carpetas: BTreeMap<String, String>,
    planillas: BTreeMap<String, EntradaPlanillaNube>,
    resumenes_archivados: Vec<ResumenPlanillaArchivada>,
    listas: ListasSincronizadas,
}

impl Default for ManifiestoSyncNube {
    fn default() -> Self {
        Self {
            app: "Lokus".to_string(),
            formato: FORMATO_MANIFIESTO_NUBE,
            actualizada_en: String::new(),
            carpetas: Vec::new(),
            orden_carpetas: Vec::new(),
            colores_carpetas: BTreeMap::new(),
            planillas: BTreeMap::new(),
            resumenes_archivados: Vec::new(),
            listas: ListasSincronizadas::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers criptográficos, PKCE, codificación URL y utilidades
// ---------------------------------------------------------------------------

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn hex_a_bytes_32(hex: &str) -> Option<[u8; 32]> {
    let raw = hex.as_bytes();
    if raw.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_nibble(raw[i * 2])?;
        let lo = hex_nibble(raw[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Calcula el `code_challenge` PKCE `S256` conforme a RFC 7636:
/// `BASE64URL-ENCODE(SHA256(ASCII(code_verifier)))` sin relleno `=`.
pub fn pkce_challenge_s256(verifier: &str) -> String {
    let hex = sha256_hex(verifier.as_bytes());
    let bytes = hex_a_bytes_32(&hex).unwrap_or([0u8; 32]);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn recolectar_entropia(etiqueta: &str, ronda: u64) -> [u8; 32] {
    let mut semilla = Vec::with_capacity(128);
    semilla.extend_from_slice(etiqueta.as_bytes());
    semilla.extend_from_slice(&ronda.to_le_bytes());

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    semilla.extend_from_slice(&nanos.to_le_bytes());
    semilla.extend_from_slice(&std::process::id().to_le_bytes());
    let seq = CONTADOR_SYNC.fetch_add(1, Ordering::SeqCst);
    semilla.extend_from_slice(&seq.to_le_bytes());

    let ptr_addr = (&semilla as *const Vec<u8>) as usize;
    semilla.extend_from_slice(&ptr_addr.to_le_bytes());

    #[cfg(unix)]
    {
        if let Ok(mut f) = fs::File::open("/dev/urandom") {
            let mut buf = [0u8; 32];
            if f.read_exact(&mut buf).is_ok() {
                semilla.extend_from_slice(&buf);
            }
        }
    }

    let hex = sha256_hex(&semilla);
    hex_a_bytes_32(&hex).unwrap_or([0u8; 32])
}

/// Genera un token aleatorio URL-safe sin relleno de 64 caracteres (válido para `code_verifier`
/// RFC 7636 de 43..=128 caracteres y para `state` OAuth2).
fn generar_token_pkce(etiqueta: &str) -> String {
    let a = recolectar_entropia(etiqueta, 1);
    let b = recolectar_entropia(etiqueta, 2);
    let mut bloque = [0u8; 48];
    bloque[..32].copy_from_slice(&a);
    bloque[32..].copy_from_slice(&b[..16]);
    URL_SAFE_NO_PAD.encode(bloque)
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2])) {
                    out.push((hi << 4) | lo);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn encode_form_urlencoded(pares: &[(&str, &str)]) -> String {
    pares
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn parsear_query_params(query: &str) -> HashMap<String, String> {
    let mut mapa = HashMap::new();
    for par in query.split('&') {
        if par.is_empty() {
            continue;
        }
        let mut it = par.splitn(2, '=');
        let k = percent_decode(it.next().unwrap_or(""));
        let v = percent_decode(it.next().unwrap_or(""));
        if !k.is_empty() {
            mapa.insert(k, v);
        }
    }
    mapa
}

fn sanitizar_segmento_id(raw: &str) -> String {
    let limpio: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if limpio.is_empty() {
        "local".to_string()
    } else {
        limpio
    }
}

fn parsear_par_hash(raw: &str) -> (&str, &str) {
    if let Some((loc, rem)) = raw.split_once('|') {
        (loc, rem)
    } else {
        (raw, raw)
    }
}

fn formatear_par_hash(hash_local: &str, hash_remoto: &str) -> String {
    if hash_local == hash_remoto {
        hash_local.to_string()
    } else {
        format!("{}|{}", hash_local, hash_remoto)
    }
}

// ---------------------------------------------------------------------------
// Persistencia local (`config/drive_sync.json`) y resolución de Client ID
// ---------------------------------------------------------------------------

pub fn ruta_config_drive_sync() -> PathBuf {
    ruta_en_carpeta_config(NOMBRE_ARCHIVO_DRIVE_SYNC)
}

fn cargar_config_drive_sin_lock_en(ruta: &Path) -> ConfigDriveSyncLocal {
    let Ok(contenido) = fs::read_to_string(ruta) else {
        return ConfigDriveSyncLocal::default();
    };
    serde_json::from_str::<ConfigDriveSyncLocal>(&contenido).unwrap_or_default()
}

pub fn cargar_config_drive_en(ruta: &Path) -> ConfigDriveSyncLocal {
    let _guard = BLOQUEO_DRIVE_CONFIG
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cargar_config_drive_sin_lock_en(ruta)
}

pub fn guardar_config_drive_en(ruta: &Path, cfg: &ConfigDriveSyncLocal) -> Result<(), String> {
    let _guard = BLOQUEO_DRIVE_CONFIG
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = ruta.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo preparar carpeta para Drive: {}", e))?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Error serializando configuración de Google Drive: {}", e))?;
    let seq = CONTADOR_SYNC.fetch_add(1, Ordering::Relaxed);
    let tmp = ruta.with_extension(format!("{}.{}.tmp", std::process::id(), seq));
    fs::write(&tmp, json)
        .map_err(|e| format!("Error escribiendo configuración de Google Drive: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    if let Err(e) = fs::rename(&tmp, ruta) {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "Error guardando configuración de Google Drive: {}",
            e
        ));
    }
    Ok(())
}

fn client_id_embebido() -> Option<String> {
    if let Some(v) = option_env!("LOKUS_GOOGLE_CLIENT_ID") {
        let limpio = v.trim();
        if !limpio.is_empty() {
            return Some(limpio.to_string());
        }
    }
    if let Ok(v) = std::env::var("LOKUS_GOOGLE_CLIENT_ID") {
        let limpio = v.trim();
        if !limpio.is_empty() {
            return Some(limpio.to_string());
        }
    }
    None
}

fn resolver_client_id(
    client_id_override: Option<&str>,
    cfg: &ConfigDriveSyncLocal,
) -> Result<String, String> {
    if let Some(ov) = client_id_override {
        let limpio = ov.trim();
        if !limpio.is_empty() {
            return Ok(limpio.to_string());
        }
    }
    if !cfg.client_id_custom.trim().is_empty() {
        return Ok(cfg.client_id_custom.trim().to_string());
    }
    if let Some(emb) = client_id_embebido() {
        return Ok(emb);
    }
    Err(
        "Falta configurar el Client ID de Google OAuth para conectar Drive. Abre 'Configuración avanzada (Client ID)' en la pestaña Nube (Drive) o revisa docs/GOOGLE_DRIVE_SYNC.md."
            .to_string(),
    )
}

fn resolver_client_secret() -> Option<String> {
    if let Some(v) = option_env!("LOKUS_GOOGLE_CLIENT_SECRET") {
        let limpio = v.trim();
        if !limpio.is_empty() {
            return Some(limpio.to_string());
        }
    }
    if let Ok(v) = std::env::var("LOKUS_GOOGLE_CLIENT_SECRET") {
        let limpio = v.trim();
        if !limpio.is_empty() {
            return Some(limpio.to_string());
        }
    }
    None
}

fn estado_desde_config(cfg: &ConfigDriveSyncLocal) -> EstadoDriveSync {
    let tiene_embebido = client_id_embebido().is_some();
    let client_id_configurado = tiene_embebido || !cfg.client_id_custom.trim().is_empty();
    let conectado = !cfg.refresh_token.trim().is_empty() || !cfg.access_token.trim().is_empty();
    EstadoDriveSync {
        conectado,
        email: cfg.email.clone(),
        nombre: cfg.nombre.clone(),
        foto_url: cfg.foto_url.clone(),
        auto_sync: cfg.auto_sync,
        client_id_configurado,
        tiene_client_id_embebido: tiene_embebido,
        client_id_custom: cfg.client_id_custom.clone(),
        ultima_sincronizacion: cfg.ultima_sincronizacion.clone(),
        ultimo_error: cfg.ultimo_error.clone(),
    }
}

pub fn obtener_estado_drive_sync() -> EstadoDriveSync {
    let cfg = cargar_config_drive_en(&ruta_config_drive_sync());
    estado_desde_config(&cfg)
}

pub fn guardar_config_drive_sync(
    auto_sync: bool,
    client_id_custom: Option<String>,
) -> Result<EstadoDriveSync, String> {
    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    cfg.auto_sync = auto_sync;
    if let Some(cid) = client_id_custom {
        cfg.client_id_custom = cid.trim().to_string();
    }
    guardar_config_drive_en(&ruta, &cfg)?;
    Ok(estado_desde_config(&cfg))
}

pub fn desconectar_google_drive() -> Result<EstadoDriveSync, String> {
    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    cfg.email.clear();
    cfg.nombre.clear();
    cfg.foto_url.clear();
    cfg.access_token.clear();
    cfg.refresh_token.clear();
    cfg.expires_at_epoch = 0;
    cfg.ultimo_error.clear();
    cfg.hashes_ult_sync.clear();
    guardar_config_drive_en(&ruta, &cfg)?;
    if let Ok(mut guard) = PENDIENTE_OAUTH_ANDROID.lock() {
        *guard = None;
    }
    Ok(estado_desde_config(&cfg))
}

// ---------------------------------------------------------------------------
// Cliente HTTP y transporte Google Drive API v3 (`spaces=appDataFolder`)
// ---------------------------------------------------------------------------

fn construir_cliente_http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("No se pudo inicializar el cliente HTTP para Google Drive: {}", e))
}

#[derive(Debug, Deserialize)]
struct DriveListResponse {
    #[serde(default)]
    files: Vec<DriveFileEntry>,
    #[serde(rename = "nextPageToken", default)]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveFileEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "modifiedTime", default)]
    modified_time: String,
    #[serde(default)]
    size: Option<String>,
}

impl From<DriveFileEntry> for ArchivoRemoto {
    fn from(f: DriveFileEntry) -> Self {
        Self {
            id: f.id,
            name: f.name,
            modified_time: f.modified_time,
            size: f
                .size
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
        }
    }
}

fn describir_error_drive(status: reqwest::StatusCode, cuerpo: &str) -> String {
    let detalle = serde_json::from_str::<serde_json::Value>(cuerpo)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();

    match status.as_u16() {
        401 => "La sesión de Google Drive expiró o fue revocada. Vuelve a conectar tu cuenta de Google.".to_string(),
        403 => {
            if detalle.is_empty() {
                "Google Drive rechazó el acceso (403). Verifica que la API de Google Drive esté habilitada y los permisos concedidos.".to_string()
            } else {
                format!("Google Drive rechazó el acceso (403): {}", detalle)
            }
        }
        404 => "El archivo remoto ya no existe en Google Drive (404).".to_string(),
        429 => "Google Drive limitó temporalmente las solicitudes (429). Intenta de nuevo en unos momentos.".to_string(),
        code => {
            if detalle.is_empty() {
                format!("Google Drive respondió con error HTTP {}.", code)
            } else {
                format!("Error de Google Drive (HTTP {}): {}", code, detalle)
            }
        }
    }
}

fn construir_cuerpo_multipart_drive(
    boundary: &str,
    nombre: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let metadata = serde_json::json!({
        "name": nombre,
        "parents": ["appDataFolder"],
    });
    let metadata_str = serde_json::to_string(&metadata)
        .map_err(|e| format!("Error serializando metadatos para Drive: {}", e))?;

    let mut body = Vec::with_capacity(bytes.len() + metadata_str.len() + 256);
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(metadata_str.as_bytes());
    body.extend_from_slice(format!("\r\n--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime_type).as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    Ok(body)
}

/// Transporte real sobre Google Drive REST API v3 operando exclusivamente en `spaces=appDataFolder`.
pub struct GoogleDriveTransport {
    client: reqwest::Client,
    access_token: String,
}

impl GoogleDriveTransport {
    pub fn nuevo(access_token: String) -> Result<Self, String> {
        Ok(Self {
            client: construir_cliente_http()?,
            access_token,
        })
    }
}

impl SyncTransport for GoogleDriveTransport {
    fn listar_archivos<'a>(&'a self) -> BoxFuture<'a, Result<Vec<ArchivoRemoto>, String>> {
        Box::pin(async move {
            let mut salida = Vec::new();
            let mut page_token: Option<String> = None;

            loop {
                let mut url = format!(
                    "{}?spaces=appDataFolder&fields=nextPageToken,files(id,name,modifiedTime,size)&pageSize=1000",
                    GOOGLE_DRIVE_FILES_API
                );
                if let Some(ref tok) = page_token {
                    url.push_str("&pageToken=");
                    url.push_str(&percent_encode(tok));
                }

                let resp = self
                    .client
                    .get(&url)
                    .bearer_auth(&self.access_token)
                    .send()
                    .await
                    .map_err(|e| {
                        format!(
                            "No se pudo conectar con Google Drive para listar archivos: {}",
                            e
                        )
                    })?;

                let status = resp.status();
                let texto = resp
                    .text()
                    .await
                    .map_err(|e| format!("Error leyendo respuesta de Google Drive: {}", e))?;
                if !status.is_success() {
                    return Err(describir_error_drive(status, &texto));
                }

                let parsed: DriveListResponse = serde_json::from_str(&texto).map_err(|e| {
                    format!("Respuesta inesperada de Google Drive al listar archivos: {}", e)
                })?;
                salida.extend(parsed.files.into_iter().map(ArchivoRemoto::from));

                match parsed.next_page_token {
                    Some(tok) if !tok.trim().is_empty() => page_token = Some(tok),
                    _ => break,
                }
            }

            Ok(salida)
        })
    }

    fn descargar_archivo<'a>(&'a self, file_id: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            let url = format!(
                "{}/{}?alt=media",
                GOOGLE_DRIVE_FILES_API,
                percent_encode(file_id)
            );
            let resp = self
                .client
                .get(&url)
                .bearer_auth(&self.access_token)
                .send()
                .await
                .map_err(|e| format!("Error descargando archivo desde Google Drive: {}", e))?;

            let status = resp.status();
            if !status.is_success() {
                let cuerpo = resp.text().await.unwrap_or_default();
                return Err(describir_error_drive(status, &cuerpo));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| format!("Error leyendo contenido descargado de Google Drive: {}", e))?;
            if bytes.len() > MAX_ARCHIVO_REMOTO_BYTES {
                return Err(
                    "Un archivo en Google Drive excede el límite seguro de descarga (64 MB)."
                        .to_string(),
                );
            }
            Ok(bytes.to_vec())
        })
    }

    fn subir_o_actualizar<'a>(
        &'a self,
        nombre: &'a str,
        mime_type: &'a str,
        bytes: &'a [u8],
        file_id_existente: Option<&'a str>,
    ) -> BoxFuture<'a, Result<ArchivoRemoto, String>> {
        Box::pin(async move {
            let resp = if let Some(file_id) = file_id_existente {
                let url = format!(
                    "{}/{}?uploadType=media&fields=id,name,modifiedTime,size",
                    GOOGLE_DRIVE_UPLOAD_API,
                    percent_encode(file_id)
                );
                self.client
                    .patch(&url)
                    .bearer_auth(&self.access_token)
                    .header("Content-Type", mime_type)
                    .body(bytes.to_vec())
                    .send()
                    .await
                    .map_err(|e| {
                        format!("Error actualizando '{}' en Google Drive: {}", nombre, e)
                    })?
            } else {
                let seq = CONTADOR_SYNC.fetch_add(1, Ordering::Relaxed);
                let boundary = format!("lokus_drive_boundary_{}_{}", std::process::id(), seq);
                let cuerpo = construir_cuerpo_multipart_drive(&boundary, nombre, mime_type, bytes)?;
                let url = format!(
                    "{}?uploadType=multipart&fields=id,name,modifiedTime,size",
                    GOOGLE_DRIVE_UPLOAD_API
                );
                self.client
                    .post(&url)
                    .bearer_auth(&self.access_token)
                    .header(
                        "Content-Type",
                        format!("multipart/related; boundary={}", boundary),
                    )
                    .body(cuerpo)
                    .send()
                    .await
                    .map_err(|e| format!("Error subiendo '{}' a Google Drive: {}", nombre, e))?
            };

            let status = resp.status();
            let texto = resp
                .text()
                .await
                .map_err(|e| format!("Error leyendo respuesta al subir '{}': {}", nombre, e))?;
            if !status.is_success() {
                return Err(describir_error_drive(status, &texto));
            }
            let entry: DriveFileEntry = serde_json::from_str(&texto).map_err(|e| {
                format!(
                    "Respuesta inesperada de Google Drive al subir '{}': {}",
                    nombre, e
                )
            })?;
            Ok(ArchivoRemoto::from(entry))
        })
    }

    fn eliminar_archivo<'a>(&'a self, file_id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let url = format!("{}/{}", GOOGLE_DRIVE_FILES_API, percent_encode(file_id));
            let resp = self
                .client
                .delete(&url)
                .bearer_auth(&self.access_token)
                .send()
                .await
                .map_err(|e| format!("Error eliminando archivo en Google Drive: {}", e))?;
            let status = resp.status();
            if status.is_success() || status.as_u16() == 404 {
                Ok(())
            } else {
                let cuerpo = resp.text().await.unwrap_or_default();
                Err(describir_error_drive(status, &cuerpo))
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Flujo OAuth2 PKCE (Desktop + Android + Refresh automático)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    picture: String,
}

#[derive(Debug, Clone, Default)]
struct PerfilGoogle {
    email: String,
    nombre: String,
    foto_url: String,
}

fn construir_url_autorizacion(
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let query = encode_form_urlencoded(&[
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", GOOGLE_DRIVE_SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ]);
    format!("{}?{}", GOOGLE_AUTH_ENDPOINT, query)
}

fn abrir_navegador_sistema(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let url_ps = url.replace('\'', "''");
        let script = format!("Start-Process '{}'", url_ps);
        return Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .or_else(|_| {
                Command::new("rundll32")
                    .args(["url.dll,FileProtocolHandler", url])
                    .spawn()
            })
            .map(|_| ())
            .map_err(|e| {
                format!(
                    "No se pudo abrir el navegador para iniciar sesión con Google: {}",
                    e
                )
            });
    }

    #[cfg(target_os = "macos")]
    let resultado = Command::new("open").arg(url).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let resultado = Command::new("xdg-open").arg(url).spawn();

    #[cfg(not(target_os = "windows"))]
    resultado
        .map(|_| ())
        .map_err(|e| format!("No se pudo abrir el navegador para iniciar sesión con Google: {}", e))
}

fn responder_html_oauth(stream: &mut std::net::TcpStream, status: &str, titulo: &str, detalle: &str) {
    let html = format!(
        "<!DOCTYPE html><html lang=\"es\"><head><meta charset=\"utf-8\"/><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"/><title>{titulo}</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#151b26;color:#eef2f7;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;padding:24px}}main{{max-width:440px;background:#1c2432;border:1px solid #273244;border-radius:12px;padding:24px;text-align:center}}h1{{font-size:18px;margin:0 0 10px}}p{{font-size:14px;color:#aab7ca;line-height:1.5;margin:0}}</style></head><body><main><h1>{titulo}</h1><p>{detalle}</p></main></body></html>"
    );
    let respuesta = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(respuesta.as_bytes());
    let _ = stream.flush();
}

fn esperar_callback_oauth_local(
    listener: TcpListener,
    expected_state: String,
    timeout: Duration,
) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Error configurando servidor local OAuth: {}", e))?;

    let inicio = Instant::now();
    loop {
        if inicio.elapsed() >= timeout {
            return Err(
                "Tiempo de espera agotado (3 minutos) esperando la autorización de Google."
                    .to_string(),
            );
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_nonblocking(false);
                let mut buf = [0u8; 8192];
                let n = match stream.read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    _ => continue,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let primera_linea = req.lines().next().unwrap_or("");
                let target = primera_linea.split_whitespace().nth(1).unwrap_or("");

                if !target.starts_with("/oauth2callback") {
                    responder_html_oauth(
                        &mut stream,
                        "404 Not Found",
                        "Ruta no válida",
                        "Vuelve a Lokus para completar la conexión.",
                    );
                    continue;
                }

                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
                let params = parsear_query_params(query);

                if let Some(err) = params.get("error") {
                    responder_html_oauth(
                        &mut stream,
                        "200 OK",
                        "Autorización cancelada",
                        "No se conectó tu cuenta de Google. Puedes cerrar esta pestaña y volver a Lokus.",
                    );
                    let desc = params
                        .get("error_description")
                        .cloned()
                        .unwrap_or_else(|| err.clone());
                    return Err(format!("Google canceló o rechazó la autorización: {}", desc));
                }

                let state_recibido = params.get("state").map(String::as_str).unwrap_or("");
                if state_recibido != expected_state {
                    responder_html_oauth(
                        &mut stream,
                        "400 Bad Request",
                        "Verificación de seguridad inválida",
                        "El parámetro de estado no coincide. Intenta conectar de nuevo desde Lokus.",
                    );
                    return Err(
                        "Error de verificación de seguridad OAuth (state inválido). Intenta conectar de nuevo."
                            .to_string(),
                    );
                }

                let Some(code) = params.get("code").filter(|c| !c.trim().is_empty()) else {
                    responder_html_oauth(
                        &mut stream,
                        "400 Bad Request",
                        "Código no recibido",
                        "Google no devolvió el código de autorización. Intenta de nuevo desde Lokus.",
                    );
                    return Err("Google no devolvió el código de autorización OAuth.".to_string());
                };

                responder_html_oauth(
                    &mut stream,
                    "200 OK",
                    "Conectado a Lokus",
                    "Tu cuenta de Google Drive quedó vinculada correctamente. Ya puedes cerrar esta pestaña y volver a Lokus.",
                );
                return Ok(code.clone());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(format!("Error recibiendo respuesta OAuth local: {}", e));
            }
        }
    }
}

async fn intercambiar_codigo_oauth(
    client: &reqwest::Client,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<GoogleTokenResponse, String> {
    let secret = resolver_client_secret();
    let mut pares: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];
    if let Some(ref sec) = secret {
        pares.push(("client_secret", sec.as_str()));
    }
    let body = encode_form_urlencoded(&pares);

    let resp = client
        .post(GOOGLE_TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("No se pudo contactar a Google para completar el inicio de sesión: {}", e))?;

    let status = resp.status();
    let texto = resp
        .text()
        .await
        .map_err(|e| format!("Error leyendo respuesta de token de Google: {}", e))?;
    if !status.is_success() {
        let detalle = serde_json::from_str::<serde_json::Value>(&texto)
            .ok()
            .and_then(|v| {
                v.get("error_description")
                    .or_else(|| v.get("error"))
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(format!(
            "Google rechazó el intercambio del código OAuth: {}",
            detalle
        ));
    }

    let parsed: GoogleTokenResponse = serde_json::from_str(&texto)
        .map_err(|e| format!("Respuesta de token de Google inválida: {}", e))?;
    if parsed.access_token.trim().is_empty() {
        return Err("Google no devolvió un token de acceso válido.".to_string());
    }
    Ok(parsed)
}

async fn refrescar_access_token(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, String> {
    let secret = resolver_client_secret();
    let mut pares: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    if let Some(ref sec) = secret {
        pares.push(("client_secret", sec.as_str()));
    }
    let body = encode_form_urlencoded(&pares);

    let resp = client
        .post(GOOGLE_TOKEN_ENDPOINT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("No se pudo renovar la sesión de Google Drive: {}", e))?;

    let status = resp.status();
    let texto = resp
        .text()
        .await
        .map_err(|e| format!("Error leyendo respuesta de renovación de Google: {}", e))?;
    if !status.is_success() {
        if texto.contains("invalid_grant") {
            return Err(
                "Tu autorización con Google Drive caducó o fue revocada. Vuelve a conectar tu cuenta de Google."
                    .to_string(),
            );
        }
        return Err(format!(
            "No se pudo renovar el acceso a Google Drive (HTTP {}). Vuelve a conectar tu cuenta.",
            status.as_u16()
        ));
    }

    let parsed: GoogleTokenResponse = serde_json::from_str(&texto)
        .map_err(|e| format!("Respuesta de renovación de Google inválida: {}", e))?;
    if parsed.access_token.trim().is_empty() {
        return Err("Google no devolvió un nuevo token de acceso.".to_string());
    }
    Ok(parsed)
}

async fn consultar_perfil_google(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<PerfilGoogle, String> {
    let resp = client
        .get(GOOGLE_USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("No se pudo consultar el perfil de la cuenta de Google: {}", e))?;

    let status = resp.status();
    let texto = resp
        .text()
        .await
        .map_err(|e| format!("Error leyendo perfil de Google: {}", e))?;
    if !status.is_success() {
        return Err(describir_error_drive(status, &texto));
    }
    let info: GoogleUserInfo = serde_json::from_str(&texto)
        .map_err(|e| format!("Respuesta de perfil de Google inválida: {}", e))?;
    Ok(PerfilGoogle {
        email: info.email.trim().to_string(),
        nombre: info.name.trim().to_string(),
        foto_url: info.picture.trim().to_string(),
    })
}

fn aplicar_sesion_google_en_config(
    cfg: &mut ConfigDriveSyncLocal,
    perfil: PerfilGoogle,
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
) {
    cfg.email = perfil.email;
    cfg.nombre = perfil.nombre;
    cfg.foto_url = perfil.foto_url;
    cfg.access_token = access_token;
    if let Some(rt) = refresh_token.filter(|r| !r.trim().is_empty()) {
        cfg.refresh_token = rt.trim().to_string();
    }
    cfg.expires_at_epoch = now_epoch() + expires_in.max(300);
    if cfg.ultima_sincronizacion.trim().is_empty() {
        cfg.auto_sync = true;
    }
    cfg.ultimo_error.clear();
}

async fn asegurar_access_token_vigente(
    cfg: &mut ConfigDriveSyncLocal,
    ruta_cfg: &Path,
) -> Result<String, String> {
    let ahora = now_epoch();
    if !cfg.access_token.trim().is_empty() && ahora < cfg.expires_at_epoch - 60 {
        return Ok(cfg.access_token.clone());
    }

    if cfg.refresh_token.trim().is_empty() {
        return Err(
            "La sesión de Google Drive expiró. Conecta de nuevo tu cuenta de Google.".to_string(),
        );
    }

    let client_id = resolver_client_id(None, cfg)?;
    let client = construir_cliente_http()?;
    let tok = match refrescar_access_token(&client, &client_id, &cfg.refresh_token).await {
        Ok(t) => t,
        Err(e) => {
            cfg.ultimo_error = e.clone();
            let _ = guardar_config_drive_en(ruta_cfg, cfg);
            return Err(e);
        }
    };

    cfg.access_token = tok.access_token;
    if let Some(rt) = tok.refresh_token.filter(|r| !r.trim().is_empty()) {
        cfg.refresh_token = rt;
    }
    cfg.expires_at_epoch = now_epoch() + tok.expires_in.max(300);
    cfg.ultimo_error.clear();
    guardar_config_drive_en(ruta_cfg, cfg)?;
    Ok(cfg.access_token.clone())
}

/// Inicia y completa el flujo OAuth2 PKCE en escritorio (loopback `127.0.0.1:<puerto>`).
pub async fn conectar_google_drive_desktop(
    client_id_override: Option<String>,
) -> Result<EstadoDriveSync, String> {
    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    if let Some(ref ov) = client_id_override {
        let limpio = ov.trim();
        if !limpio.is_empty() {
            cfg.client_id_custom = limpio.to_string();
        }
    }
    let client_id = resolver_client_id(client_id_override.as_deref(), &cfg)?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("No se pudo abrir un puerto local para conectar con Google: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("No se pudo obtener el puerto local OAuth: {}", e))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{}/oauth2callback", port);

    let code_verifier = generar_token_pkce("pkce-desktop-verifier");
    let code_challenge = pkce_challenge_s256(&code_verifier);
    let state = generar_token_pkce("pkce-desktop-state");
    let auth_url = construir_url_autorizacion(&client_id, &redirect_uri, &code_challenge, &state);

    abrir_navegador_sistema(&auth_url)?;

    let state_esperado = state.clone();
    let code = tokio::task::spawn_blocking(move || {
        esperar_callback_oauth_local(
            listener,
            state_esperado,
            Duration::from_secs(TIMEOUT_OAUTH_SECS),
        )
    })
    .await
    .map_err(|e| format!("Error esperando respuesta del navegador: {}", e))??;

    let client = construir_cliente_http()?;
    let tokens =
        intercambiar_codigo_oauth(&client, &client_id, &redirect_uri, &code, &code_verifier)
            .await?;
    let perfil = consultar_perfil_google(&client, &tokens.access_token)
        .await
        .unwrap_or_default();

    aplicar_sesion_google_en_config(
        &mut cfg,
        perfil,
        tokens.access_token,
        tokens.refresh_token,
        tokens.expires_in,
    );
    guardar_config_drive_en(&ruta, &cfg)?;

    Ok(estado_desde_config(&cfg))
}

/// Prepara el URL de autorización OAuth2 PKCE para Android y conserva `code_verifier`/`state`
/// en memoria hasta recibir el deep link `com.josebustamante.gestoraudiencias:/oauth2redirect`.
pub fn preparar_auth_google_drive_android(
    client_id_override: Option<String>,
) -> Result<InicioAuthAndroid, String> {
    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    if let Some(ref ov) = client_id_override {
        let limpio = ov.trim();
        if !limpio.is_empty() {
            cfg.client_id_custom = limpio.to_string();
            let _ = guardar_config_drive_en(&ruta, &cfg);
        }
    }
    let client_id = resolver_client_id(client_id_override.as_deref(), &cfg)?;

    let code_verifier = generar_token_pkce("pkce-android-verifier");
    let code_challenge = pkce_challenge_s256(&code_verifier);
    let state = generar_token_pkce("pkce-android-state");
    let redirect_uri = ANDROID_REDIRECT_URI.to_string();
    let auth_url = construir_url_autorizacion(&client_id, &redirect_uri, &code_challenge, &state);

    let mut guard = PENDIENTE_OAUTH_ANDROID
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *guard = Some(EstadoOauthPendiente {
        code_verifier,
        state: state.clone(),
        client_id,
        redirect_uri: redirect_uri.clone(),
        creado_epoch: now_epoch(),
    });

    Ok(InicioAuthAndroid {
        auth_url,
        state,
        redirect_uri,
    })
}

/// Completa el intercambio OAuth2 PKCE en Android al regresar el deep link con `code` y `state`.
pub async fn completar_auth_google_drive_android(
    code: String,
    state: String,
) -> Result<EstadoDriveSync, String> {
    let code_trim = code.trim().to_string();
    let state_trim = state.trim().to_string();
    if code_trim.is_empty() {
        return Err("No se recibió el código de autorización de Google.".to_string());
    }

    let pendiente = {
        let mut guard = PENDIENTE_OAUTH_ANDROID
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.take()
    }
    .ok_or_else(|| {
        "No hay una solicitud de inicio de sesión con Google pendiente. Intenta conectar de nuevo."
            .to_string()
    })?;

    if now_epoch() - pendiente.creado_epoch > TTL_OAUTH_ANDROID_SECS {
        return Err(
            "La solicitud de conexión con Google expiró. Intenta conectar de nuevo.".to_string(),
        );
    }

    if state_trim != pendiente.state {
        return Err(
            "Error de seguridad OAuth (state no coincide). Intenta conectar de nuevo.".to_string(),
        );
    }

    let client = construir_cliente_http()?;
    let tokens = intercambiar_codigo_oauth(
        &client,
        &pendiente.client_id,
        &pendiente.redirect_uri,
        &code_trim,
        &pendiente.code_verifier,
    )
    .await?;
    let perfil = consultar_perfil_google(&client, &tokens.access_token)
        .await
        .unwrap_or_default();

    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    aplicar_sesion_google_en_config(
        &mut cfg,
        perfil,
        tokens.access_token,
        tokens.refresh_token,
        tokens.expires_in,
    );
    guardar_config_drive_en(&ruta, &cfg)?;

    Ok(estado_desde_config(&cfg))
}

/// Vincula la sesión local a partir de un token OAuth ya obtenido externamente (p. ej. puente nativo).
pub async fn conectar_google_drive_desde_token(
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
) -> Result<EstadoDriveSync, String> {
    let at = access_token.trim().to_string();
    if at.is_empty() {
        return Err("El token de acceso de Google está vacío.".to_string());
    }

    let client = construir_cliente_http()?;
    let perfil = consultar_perfil_google(&client, &at).await?;

    let ruta = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta);
    aplicar_sesion_google_en_config(
        &mut cfg,
        perfil,
        at,
        refresh_token,
        expires_in.unwrap_or(3600),
    );
    guardar_config_drive_en(&ruta, &cfg)?;

    Ok(estado_desde_config(&cfg))
}

// ---------------------------------------------------------------------------
// Motor de sincronización incremental sin conflictos
// ---------------------------------------------------------------------------

struct InfoPlanillaLocal {
    id: String,
    ruta_relativa: String,
    sha256: String,
    bytes: Vec<u8>,
}

fn normalizar_ruta_planilla_sync(ruta_relativa: &str) -> Result<String, String> {
    let segmentos = segmentos_ruta_relativa(ruta_relativa)?;
    let mut limpios = Vec::with_capacity(segmentos.len());
    let ultimo = segmentos.len().saturating_sub(1);
    for (i, seg) in segmentos.into_iter().enumerate() {
        if i == ultimo {
            limpios.push(sanitizar_nombre_archivo(&seg)?);
        } else {
            limpios.push(sanitizar_ruta_carpeta(&seg)?);
        }
    }
    Ok(limpios.join("/"))
}

fn ruta_alternativa_conflicto(carpeta_base: &Path, ruta_rel: &str, sufijo: &str) -> String {
    let path = Path::new(ruta_rel);
    let parent = path
        .parent()
        .and_then(|p| p.to_str())
        .filter(|p| !p.is_empty());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("planilla");

    for idx in 1..=9999 {
        let candidato_nombre = if idx == 1 {
            format!("{}_{}.xlsx", stem, sufijo)
        } else {
            format!("{}_{}_{}.xlsx", stem, sufijo, idx)
        };
        let candidato_rel = match parent {
            Some(dir) => format!("{}/{}", dir, candidato_nombre),
            None => candidato_nombre,
        };
        if !carpeta_base.join(&candidato_rel).exists() {
            return candidato_rel;
        }
    }
    format!("{}_{}_{}.xlsx", stem, sufijo, generar_id_planilla())
}

fn escribir_excel_desde_nube(
    destino: &Path,
    bytes: &[u8],
    hacer_respaldo: bool,
) -> Result<String, String> {
    if !bytes.starts_with(b"PK") {
        return Err(
            "Una planilla descargada desde Google Drive no tiene formato Excel (.xlsx) válido."
                .to_string(),
        );
    }
    if let Some(parent) = destino.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo preparar carpeta al sincronizar: {}", e))?;
    }
    if hacer_respaldo && destino.exists() {
        let _ = crear_respaldo_en(destino);
    }
    fs::write(destino, bytes)
        .map_err(|e| format!("No se pudo escribir '{}': {}", destino.display(), e))?;
    sanear_archivo_excel_en(destino);
    invalidar_cache(destino);
    let bytes_finales = fs::read(destino)
        .map_err(|e| format!("No se pudo verificar '{}' tras sincronizar: {}", destino.display(), e))?;
    Ok(sha256_hex(&bytes_finales))
}

fn leer_meta_planilla(abs: &Path) -> (String, u32) {
    match leer_archivo_en(abs) {
        Ok((regs, _tot, def)) => (def, regs.len() as u32),
        Err(_) => (String::new(), 0),
    }
}

fn construir_resumen_mensaje(r: &ResultadoDriveSync) -> String {
    if r.sin_cambios {
        return "Todo ya estaba al día con Google Drive.".to_string();
    }
    let mut partes = Vec::new();
    if r.planillas_subidas > 0 {
        partes.push(format!("{} planilla(s) subida(s)", r.planillas_subidas));
    }
    if r.planillas_descargadas > 0 {
        partes.push(format!(
            "{} planilla(s) descargada(s)",
            r.planillas_descargadas
        ));
    }
    if r.conflictos_respaldados > 0 {
        partes.push(format!(
            "{} conflicto(s) respaldado(s) sin pérdida",
            r.conflictos_respaldados
        ));
    }
    if r.ia_subida || r.ia_cambios_fusionados > 0 {
        partes.push("aprendizaje IA sincronizado".to_string());
    }
    if r.uso_subido || r.uso_instancias_fusionadas > 0 {
        partes.push("uso de catálogos sincronizado".to_string());
    }
    if r.config_fusionada {
        partes.push("listas fusionadas".to_string());
    }
    if r.resumenes_fusionados > 0 {
        partes.push(format!(
            "{} resumen(es) estadístico(s)",
            r.resumenes_fusionados
        ));
    }
    if partes.is_empty() {
        "Sincronización completada con Google Drive.".to_string()
    } else {
        format!("Sincronización completada: {}.", partes.join(", "))
    }
}

/// Ejecuta la sincronización incremental completa sobre un `SyncTransport` arbitrario
/// (`GoogleDriveTransport` en producción o `MockSyncTransport` en pruebas).
pub async fn sincronizar_con_transporte_en<T: SyncTransport>(
    transport: &T,
    carpeta_archivos: &Path,
    ruta_cfg_drive: &Path,
    ruta_uso: &Path,
    ruta_mem_ia: &Path,
    settings: &mut AppSettings,
) -> Result<ResultadoDriveSync, String> {
    fs::create_dir_all(carpeta_archivos)
        .map_err(|e| format!("No se pudo acceder a la carpeta de planillas: {}", e))?;

    let mut cfg_drive = cargar_config_drive_en(ruta_cfg_drive);
    let ahora_iso = Local::now().to_rfc3339();
    let mut res = ResultadoDriveSync {
        sincronizado_en: ahora_iso.clone(),
        ..Default::default()
    };

    // Listar contenido remoto actual en `appDataFolder`
    let archivos_remotos = transport.listar_archivos().await?;
    let mut remoto_por_nombre: HashMap<String, ArchivoRemoto> = HashMap::new();
    for item in archivos_remotos {
        match remoto_por_nombre.get(&item.name) {
            Some(prev) if prev.modified_time > item.modified_time => {}
            _ => {
                remoto_por_nombre.insert(item.name.clone(), item);
            }
        }
    }

    // -----------------------------------------------------------------------
    // 1. Memoria IA (idempotente anti ping-pong: 1 archivo por instancia)
    // -----------------------------------------------------------------------
    let mem_inicial = preparar_snapshot_memoria_para_paquete_en(ruta_mem_ia)?;
    let mi_instancia_ia = sanitizar_segmento_id(&mem_inicial.instancia_id);
    let mi_archivo_ia = format!("ia_memoria_{}.json", mi_instancia_ia);
    let key_ia = format!("__ia__::{}", mi_archivo_ia);
    let bytes_ia_inicial = serde_json::to_vec(&mem_inicial)
        .map_err(|e| format!("Error serializando memoria de IA para Drive: {}", e))?;
    let hash_ia_inicial = sha256_hex(&bytes_ia_inicial);
    let tiene_actividad_inicial = memoria_tiene_actividad(&mem_inicial);
    let tiene_actividad_propia = mem_inicial
        .contadores_por_instancia
        .contains_key(&mem_inicial.instancia_id)
        || !mem_inicial.reglas.is_empty()
        || !mem_inicial.correcciones.is_empty();
    let cambio_local_ia = tiene_actividad_inicial
        && match cfg_drive.hashes_ult_sync.get(&key_ia).map(String::as_str) {
            Some(prev) => prev != hash_ia_inicial.as_str(),
            None => tiene_actividad_propia || !remoto_por_nombre.contains_key(&mi_archivo_ia),
        };

    let mut nombres_ia_remotos: Vec<String> = remoto_por_nombre
        .keys()
        .filter(|n| n.starts_with("ia_memoria_") && n.ends_with(".json") && *n != &mi_archivo_ia)
        .cloned()
        .collect();
    nombres_ia_remotos.sort();

    for nombre in &nombres_ia_remotos {
        if let Some(meta) = remoto_por_nombre.get(nombre) {
            let key_rem = format!("__rem_ia__::{}", nombre);
            let firma_rem = format!("{}|{}|{}", meta.id, meta.modified_time, meta.size);
            if cfg_drive.hashes_ult_sync.get(&key_rem).map(String::as_str)
                == Some(firma_rem.as_str())
            {
                continue;
            }
            let bytes = transport.descargar_archivo(&meta.id).await?;
            if let Ok(mem_remota) = serde_json::from_slice::<MemoriaAprendizaje>(&bytes) {
                let r_ia = fusionar_memoria_ia_desde_paquete_en(ruta_mem_ia, &mem_remota)?;
                res.ia_cambios_fusionados += r_ia.cambios_totales;
                cfg_drive.hashes_ult_sync.insert(key_rem, firma_rem);
            }
        }
    }

    let mem_actual = preparar_snapshot_memoria_para_paquete_en(ruta_mem_ia)?;
    if memoria_tiene_actividad(&mem_actual) {
        let inst_actual = sanitizar_segmento_id(&mem_actual.instancia_id);
        let archivo_ia_actual = format!("ia_memoria_{}.json", inst_actual);
        let key_ia_actual = format!("__ia__::{}", archivo_ia_actual);
        let bytes_ia = serde_json::to_vec(&mem_actual)
            .map_err(|e| format!("Error serializando memoria de IA para Drive: {}", e))?;
        let hash_ia = sha256_hex(&bytes_ia);
        if cambio_local_ia {
            let existe_remoto = remoto_por_nombre.get(&archivo_ia_actual);
            let subido = transport
                .subir_o_actualizar(
                    &archivo_ia_actual,
                    MIME_JSON,
                    &bytes_ia,
                    existe_remoto.map(|m| m.id.as_str()),
                )
                .await?;
            remoto_por_nombre.insert(archivo_ia_actual, subido);
            res.ia_subida = true;
        }
        cfg_drive.hashes_ult_sync.insert(key_ia_actual, hash_ia);
    }

    // -----------------------------------------------------------------------
    // 2. Uso de catálogos (`uso_catalogos v2`: 1 snapshot por instancia)
    // -----------------------------------------------------------------------
    let uso_inicial = asegurar_uso_inicializado_en(ruta_uso, carpeta_archivos, settings);
    let mi_instancia_uso = sanitizar_segmento_id(&uso_inicial.instancia_id);
    let mi_archivo_uso = format!("uso_{}.json", mi_instancia_uso);
    let config_dir_uso = ruta_uso.parent().unwrap_or_else(|| Path::new("."));

    let mut nombres_uso_remotos: Vec<String> = remoto_por_nombre
        .keys()
        .filter(|n| n.starts_with("uso_") && n.ends_with(".json") && *n != &mi_archivo_uso)
        .cloned()
        .collect();
    nombres_uso_remotos.sort();

    for nombre in &nombres_uso_remotos {
        if let Some(meta) = remoto_por_nombre.get(nombre) {
            let key_rem = format!("__rem_uso__::{}", nombre);
            let firma_rem = format!("{}|{}|{}", meta.id, meta.modified_time, meta.size);
            if cfg_drive.hashes_ult_sync.get(&key_rem).map(String::as_str)
                == Some(firma_rem.as_str())
            {
                continue;
            }
            let bytes = transport.descargar_archivo(&meta.id).await?;
            if let Ok(snap_remoto) = serde_json::from_slice::<UsoCatalogosInstancia>(&bytes) {
                if fusionar_snapshot_instancia_en(config_dir_uso, &snap_remoto)? {
                    res.uso_instancias_fusionadas += 1;
                }
                cfg_drive.hashes_ult_sync.insert(key_rem, firma_rem);
            }
        }
    }

    let uso_actual = cargar_uso_con_lock_en(ruta_uso);
    let mi_snap_uso = uso_actual.snapshot_instancia();
    let snap_tiene_datos = mi_snap_uso.total_audiencias_contabilizadas > 0
        || mi_snap_uso.tipos_sin_especificar > 0
        || mi_snap_uso.juzgados_sin_especificar > 0
        || mi_snap_uso.horas_sin_especificar > 0
        || !mi_snap_uso.tipos_audiencia.is_empty()
        || !mi_snap_uso.juzgados.is_empty()
        || !mi_snap_uso.horas_exactas.is_empty();

    if snap_tiene_datos {
        let bytes_uso = serde_json::to_vec(&mi_snap_uso)
            .map_err(|e| format!("Error serializando uso de catálogos para Drive: {}", e))?;
        let hash_uso = sha256_hex(&bytes_uso);
        let key_uso = format!("__uso__::{}", mi_archivo_uso);
        let existe_remoto = remoto_por_nombre.get(&mi_archivo_uso);
        let cambio_uso = existe_remoto.is_none()
            || cfg_drive.hashes_ult_sync.get(&key_uso).map(String::as_str)
                != Some(hash_uso.as_str());
        if cambio_uso {
            let subido = transport
                .subir_o_actualizar(
                    &mi_archivo_uso,
                    MIME_JSON,
                    &bytes_uso,
                    existe_remoto.map(|m| m.id.as_str()),
                )
                .await?;
            remoto_por_nombre.insert(mi_archivo_uso.clone(), subido);
            cfg_drive.hashes_ult_sync.insert(key_uso, hash_uso);
            res.uso_subido = true;
        }
    }

    // -----------------------------------------------------------------------
    // 3. Planillas Excel, carpetas, estadísticas archivadas y listas
    // -----------------------------------------------------------------------
    let manifest_remoto_meta = remoto_por_nombre.get(NOMBRE_MANIFIESTO_NUBE).cloned();
    let mut manifest = if let Some(ref meta) = manifest_remoto_meta {
        let bytes = transport.descargar_archivo(&meta.id).await?;
        serde_json::from_slice::<ManifiestoSyncNube>(&bytes).unwrap_or_default()
    } else {
        ManifiestoSyncNube::default()
    };
    let mut manifest_modificado = manifest_remoto_meta.is_none();

    let (archivos_locales, carpetas_locales) = explorar_carpeta_en(carpeta_archivos)?;
    let mut reg = cargar_registro_estadisticas_en(carpeta_archivos);
    let mut reg_modificado = false;

    // 3.1 Carpetas bidireccionales
    let mut set_carpetas: BTreeSet<String> = BTreeSet::new();
    for c in &carpetas_locales {
        if let Ok(limpia) = sanitizar_ruta_carpeta(c) {
            set_carpetas.insert(limpia);
        }
    }
    for c in &manifest.carpetas {
        if let Ok(limpia) = sanitizar_ruta_carpeta(c) {
            let dir = carpeta_archivos.join(&limpia);
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|e| {
                    format!("No se pudo crear carpeta sincronizada '{}': {}", limpia, e)
                })?;
                res.carpetas_creadas += 1;
            }
            set_carpetas.insert(limpia);
        }
    }
    let carpetas_unificadas: Vec<String> = set_carpetas.into_iter().collect();
    if manifest.carpetas != carpetas_unificadas {
        manifest.carpetas = carpetas_unificadas;
        manifest_modificado = true;
    }

    // 3.2 Inventario local de planillas
    let mut local_por_id: BTreeMap<String, InfoPlanillaLocal> = BTreeMap::new();
    let mut rutas_activas: HashSet<String> = HashSet::new();
    for rel in &archivos_locales {
        let (id, creado) = asegurar_id_ruta(&mut reg, rel);
        if creado {
            reg_modificado = true;
        }
        let abs = carpeta_archivos.join(rel);
        let bytes = fs::read(&abs)
            .map_err(|e| format!("No se pudo leer planilla local '{}': {}", rel, e))?;
        let sha256 = sha256_hex(&bytes);
        rutas_activas.insert(rel.clone());
        local_por_id.insert(
            id.clone(),
            InfoPlanillaLocal {
                id,
                ruta_relativa: rel.clone(),
                sha256,
                bytes,
            },
        );
    }

    // 3.3 Reconciliación planilla por planilla
    let mut todos_ids: BTreeSet<String> = BTreeSet::new();
    todos_ids.extend(local_por_id.keys().cloned());
    todos_ids.extend(manifest.planillas.keys().cloned());

    let mut ids_activos_finales: HashSet<String> = local_por_id.keys().cloned().collect();

    for id in todos_ids {
        let loc_opt = local_por_id.get(&id);
        let rem_opt = manifest.planillas.get(&id).cloned();
        let prev_pair = cfg_drive
            .hashes_ult_sync
            .get(&id)
            .map(|s| parsear_par_hash(s.as_str()));

        match (loc_opt, rem_opt) {
            // Caso A: Solo existe localmente -> subir a Drive
            (Some(loc), None) => {
                let drive_name = format!("planilla_{}.xlsx", sanitizar_segmento_id(&loc.id));
                let file_id_exist = remoto_por_nombre.get(&drive_name).map(|m| m.id.as_str());
                let subido = transport
                    .subir_o_actualizar(&drive_name, MIME_XLSX, &loc.bytes, file_id_exist)
                    .await?;
                remoto_por_nombre.insert(drive_name.clone(), subido);

                let (defensor, num_registros) =
                    leer_meta_planilla(&carpeta_archivos.join(&loc.ruta_relativa));
                manifest.planillas.insert(
                    loc.id.clone(),
                    EntradaPlanillaNube {
                        id: loc.id.clone(),
                        ruta_relativa: loc.ruta_relativa.clone(),
                        sha256: loc.sha256.clone(),
                        drive_file_name: drive_name,
                        actualizada_en: ahora_iso.clone(),
                        defensor,
                        num_registros,
                    },
                );
                cfg_drive
                    .hashes_ult_sync
                    .insert(loc.id.clone(), formatear_par_hash(&loc.sha256, &loc.sha256));
                manifest_modificado = true;
                res.planillas_subidas += 1;
            }

            // Caso B: Solo existe en la nube -> descargar localmente
            (None, Some(rem)) => {
                let Some(meta) = remoto_por_nombre.get(&rem.drive_file_name).cloned() else {
                    continue;
                };
                let bytes = transport.descargar_archivo(&meta.id).await?;
                let ruta_norm = normalizar_ruta_planilla_sync(&rem.ruta_relativa)?;
                let destino_rel = if rutas_activas.contains(&ruta_norm) {
                    ruta_alternativa_conflicto(carpeta_archivos, &ruta_norm, "nube_conflicto")
                } else {
                    ruta_norm
                };
                let destino_abs = carpeta_archivos.join(&destino_rel);
                let hash_disco = escribir_excel_desde_nube(&destino_abs, &bytes, false)?;

                reg.ids_por_ruta.insert(destino_rel.clone(), id.clone());
                reg.archivadas.retain(|a| a.id != id);
                reg_modificado = true;
                rutas_activas.insert(destino_rel);
                ids_activos_finales.insert(id.clone());

                cfg_drive
                    .hashes_ult_sync
                    .insert(id.clone(), formatear_par_hash(&hash_disco, &rem.sha256));
                res.planillas_descargadas += 1;
            }

            // Caso C: Existe tanto local como en la nube
            (Some(loc), Some(rem)) => {
                // C1: Idénticos por contenido o sin cambios respecto a la última sync registrada
                let sin_cambios_desde_base = prev_pair
                    .is_some_and(|(p_loc, p_rem)| p_loc == loc.sha256 && p_rem == rem.sha256);

                if loc.sha256 == rem.sha256 || sin_cambios_desde_base {
                    // Asegurar que el binario exista en Drive si faltaba
                    if !remoto_por_nombre.contains_key(&rem.drive_file_name) {
                        let subido = transport
                            .subir_o_actualizar(&rem.drive_file_name, MIME_XLSX, &loc.bytes, None)
                            .await?;
                        remoto_por_nombre.insert(rem.drive_file_name.clone(), subido);
                    }
                    if loc.sha256 == rem.sha256 {
                        cfg_drive
                            .hashes_ult_sync
                            .insert(id.clone(), formatear_par_hash(&loc.sha256, &rem.sha256));
                    }
                    continue;
                }

                let local_cambio = prev_pair.map(|(p_loc, _)| p_loc != loc.sha256).unwrap_or(true);
                let remoto_cambio = prev_pair.map(|(_, p_rem)| p_rem != rem.sha256).unwrap_or(true);

                if local_cambio && !remoto_cambio {
                    // C2: Solo cambió localmente -> subir actualización
                    let file_id_exist = remoto_por_nombre
                        .get(&rem.drive_file_name)
                        .map(|m| m.id.as_str());
                    let subido = transport
                        .subir_o_actualizar(
                            &rem.drive_file_name,
                            MIME_XLSX,
                            &loc.bytes,
                            file_id_exist,
                        )
                        .await?;
                    remoto_por_nombre.insert(rem.drive_file_name.clone(), subido);

                    let (defensor, num_registros) =
                        leer_meta_planilla(&carpeta_archivos.join(&loc.ruta_relativa));
                    manifest.planillas.insert(
                        id.clone(),
                        EntradaPlanillaNube {
                            id: id.clone(),
                            ruta_relativa: loc.ruta_relativa.clone(),
                            sha256: loc.sha256.clone(),
                            drive_file_name: rem.drive_file_name.clone(),
                            actualizada_en: ahora_iso.clone(),
                            defensor,
                            num_registros,
                        },
                    );
                    cfg_drive
                        .hashes_ult_sync
                        .insert(id.clone(), formatear_par_hash(&loc.sha256, &loc.sha256));
                    manifest_modificado = true;
                    res.planillas_subidas += 1;
                } else if !local_cambio && remoto_cambio {
                    // C3: Solo cambió remotamente -> descargar actualización respaldando copia previa
                    if let Some(meta) = remoto_por_nombre.get(&rem.drive_file_name).cloned() {
                        let bytes = transport.descargar_archivo(&meta.id).await?;
                        let destino_abs = carpeta_archivos.join(&loc.ruta_relativa);
                        let hash_disco = escribir_excel_desde_nube(&destino_abs, &bytes, true)?;
                        cfg_drive
                            .hashes_ult_sync
                            .insert(id.clone(), formatear_par_hash(&hash_disco, &rem.sha256));
                        res.planillas_descargadas += 1;
                    }
                } else {
                    // C4: Conflicto simultáneo (cambió local Y cambió remoto con distinto hash).
                    // Política cero pérdida de datos:
                    // 1) Respalda la planilla local en `.respaldos/`.
                    // 2) Descarga la versión remota como copia segura `*_nube_conflicto.xlsx` con ID propio.
                    // 3) Sube tanto la versión local (en `id`) como la copia de conflicto para convergencia.
                    let local_abs = carpeta_archivos.join(&loc.ruta_relativa);
                    let _ = crear_respaldo_en(&local_abs);

                    if let Some(meta) = remoto_por_nombre.get(&rem.drive_file_name).cloned() {
                        let bytes_remotos = transport.descargar_archivo(&meta.id).await?;
                        let conflicto_rel = ruta_alternativa_conflicto(
                            carpeta_archivos,
                            &loc.ruta_relativa,
                            "nube_conflicto",
                        );
                        let conflicto_abs = carpeta_archivos.join(&conflicto_rel);
                        let hash_conflicto_disco =
                            escribir_excel_desde_nube(&conflicto_abs, &bytes_remotos, false)?;
                        let bytes_conflicto_final = fs::read(&conflicto_abs).map_err(|e| {
                            format!("No se pudo leer copia de conflicto '{}': {}", conflicto_rel, e)
                        })?;

                        let conflicto_id = generar_id_planilla();
                        let conflicto_drive_name =
                            format!("planilla_{}.xlsx", sanitizar_segmento_id(&conflicto_id));
                        let subido_conf = transport
                            .subir_o_actualizar(
                                &conflicto_drive_name,
                                MIME_XLSX,
                                &bytes_conflicto_final,
                                None,
                            )
                            .await?;
                        remoto_por_nombre.insert(conflicto_drive_name.clone(), subido_conf);

                        let (def_conf, num_conf) = leer_meta_planilla(&conflicto_abs);
                        reg.ids_por_ruta
                            .insert(conflicto_rel.clone(), conflicto_id.clone());
                        reg_modificado = true;
                        rutas_activas.insert(conflicto_rel.clone());
                        ids_activos_finales.insert(conflicto_id.clone());

                        manifest.planillas.insert(
                            conflicto_id.clone(),
                            EntradaPlanillaNube {
                                id: conflicto_id.clone(),
                                ruta_relativa: conflicto_rel,
                                sha256: hash_conflicto_disco.clone(),
                                drive_file_name: conflicto_drive_name,
                                actualizada_en: ahora_iso.clone(),
                                defensor: def_conf,
                                num_registros: num_conf,
                            },
                        );
                        cfg_drive.hashes_ult_sync.insert(
                            conflicto_id,
                            formatear_par_hash(&hash_conflicto_disco, &hash_conflicto_disco),
                        );
                    }

                    // Actualizar la planilla principal `id` con la versión local
                    let file_id_exist = remoto_por_nombre
                        .get(&rem.drive_file_name)
                        .map(|m| m.id.as_str());
                    let subido_local = transport
                        .subir_o_actualizar(
                            &rem.drive_file_name,
                            MIME_XLSX,
                            &loc.bytes,
                            file_id_exist,
                        )
                        .await?;
                    remoto_por_nombre.insert(rem.drive_file_name.clone(), subido_local);

                    let (defensor, num_registros) = leer_meta_planilla(&local_abs);
                    manifest.planillas.insert(
                        id.clone(),
                        EntradaPlanillaNube {
                            id: id.clone(),
                            ruta_relativa: loc.ruta_relativa.clone(),
                            sha256: loc.sha256.clone(),
                            drive_file_name: rem.drive_file_name.clone(),
                            actualizada_en: ahora_iso.clone(),
                            defensor,
                            num_registros,
                        },
                    );
                    cfg_drive
                        .hashes_ult_sync
                        .insert(id.clone(), formatear_par_hash(&loc.sha256, &loc.sha256));
                    manifest_modificado = true;
                    res.conflictos_respaldados += 1;
                }
            }

            (None, None) => {}
        }
    }

    // 3.4 Fusión de orden y colores de carpetas
    for item in &manifest.orden_carpetas {
        if let Ok(limpia) = sanitizar_ruta_carpeta(item) {
            if !reg.orden_carpetas.contains(&limpia) {
                reg.orden_carpetas.push(limpia);
                reg_modificado = true;
            }
        }
    }
    for item in &reg.orden_carpetas {
        if let Ok(limpia) = sanitizar_ruta_carpeta(item) {
            if !manifest.orden_carpetas.contains(&limpia) {
                manifest.orden_carpetas.push(limpia);
                manifest_modificado = true;
            }
        }
    }

    for (carpeta, color) in &manifest.colores_carpetas {
        if let (Ok(limpia), Ok(color_ok)) =
            (sanitizar_ruta_carpeta(carpeta), validar_color_carpeta(color))
        {
            if !color_ok.is_empty() && !reg.colores_carpetas.contains_key(&limpia) {
                reg.colores_carpetas.insert(limpia, color_ok);
                reg_modificado = true;
            }
        }
    }
    for (carpeta, color) in &reg.colores_carpetas {
        if let (Ok(limpia), Ok(color_ok)) =
            (sanitizar_ruta_carpeta(carpeta), validar_color_carpeta(color))
        {
            if !color_ok.is_empty() && manifest.colores_carpetas.get(&limpia) != Some(&color_ok) {
                manifest.colores_carpetas.insert(limpia, color_ok);
                manifest_modificado = true;
            }
        }
    }

    // 3.5 Fusión de resúmenes estadísticos archivados por ID (sin duplicar planillas vivas)
    let mut ids_archivados_locales: HashSet<String> =
        reg.archivadas.iter().map(|a| a.id.clone()).collect();
    for arch_rem in &manifest.resumenes_archivados {
        if ids_activos_finales.contains(&arch_rem.id) {
            continue;
        }
        if !ids_archivados_locales.contains(&arch_rem.id) {
            upsert_resumen_archivado(&mut reg, arch_rem.clone());
            ids_archivados_locales.insert(arch_rem.id.clone());
            reg_modificado = true;
            res.resumenes_fusionados += 1;
        }
    }

    let mut ids_archivados_remotos: HashSet<String> = manifest
        .resumenes_archivados
        .iter()
        .map(|a| a.id.clone())
        .collect();
    let len_rem_antes = manifest.resumenes_archivados.len();
    manifest
        .resumenes_archivados
        .retain(|a| !ids_activos_finales.contains(&a.id));
    if manifest.resumenes_archivados.len() != len_rem_antes {
        manifest_modificado = true;
    }
    for arch_loc in &reg.archivadas {
        if ids_activos_finales.contains(&arch_loc.id) {
            continue;
        }
        if !ids_archivados_remotos.contains(&arch_loc.id) {
            manifest.resumenes_archivados.push(arch_loc.clone());
            ids_archivados_remotos.insert(arch_loc.id.clone());
            manifest_modificado = true;
        }
    }

    if reg_modificado {
        guardar_registro_estadisticas_en(carpeta_archivos, &reg)?;
    }
    if res.planillas_descargadas > 0 || res.conflictos_respaldados > 0 {
        invalidar_priors_en(carpeta_archivos);
    }

    // 3.6 Fusión bidireccional de listas de usuario (NUNCA API keys ni tokens)
    let tipos_default: Vec<String> = TIPOS_AUDIENCIA
        .iter()
        .filter(|&&t| t != "Otra")
        .map(|&t| t.to_string())
        .collect();

    let defs_antes = settings.defensores.clone();
    let juzs_antes = settings.juzgados.clone();
    let tipos_antes = settings.tipos_audiencia.clone();

    let mut defs_union = settings.defensores.clone();
    defs_union.extend(manifest.listas.defensores.iter().cloned());
    let defs_final = normalizar_lista_defensores(defs_union);

    let mut juzs_union = settings.juzgados.clone();
    juzs_union.extend(manifest.listas.juzgados.iter().cloned());
    let juzs_final = normalizar_lista_catalogo_usuario(juzs_union, juzgados_base());

    let mut tipos_union = settings.tipos_audiencia.clone();
    tipos_union.extend(manifest.listas.tipos_audiencia.iter().cloned());
    let tipos_final = normalizar_lista_catalogo_usuario(tipos_union, &tipos_default);

    if defs_final != defs_antes || juzs_final != juzs_antes || tipos_final != tipos_antes {
        settings.defensores = defs_final.clone();
        settings.juzgados = juzs_final.clone();
        settings.tipos_audiencia = tipos_final.clone();
        res.config_fusionada = true;
    }

    let nuevas_listas_remotas = ListasSincronizadas {
        defensores: defs_final,
        juzgados: juzs_final,
        tipos_audiencia: tipos_final,
    };
    if manifest.listas != nuevas_listas_remotas {
        manifest.listas = nuevas_listas_remotas;
        manifest_modificado = true;
    }

    // 3.7 Subir manifiesto actualizado únicamente si hubo cambios
    if manifest_modificado {
        manifest.app = "Lokus".to_string();
        manifest.formato = FORMATO_MANIFIESTO_NUBE;
        manifest.actualizada_en = ahora_iso.clone();
        let bytes_manifiesto = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("Error serializando manifiesto de Drive: {}", e))?;
        transport
            .subir_o_actualizar(
                NOMBRE_MANIFIESTO_NUBE,
                MIME_JSON,
                &bytes_manifiesto,
                manifest_remoto_meta.as_ref().map(|m| m.id.as_str()),
            )
            .await?;
    }

    // 3.8 Persistir metadatos locales de última sincronización
    cfg_drive.ultima_sincronizacion = ahora_iso;
    cfg_drive.ultimo_error.clear();
    guardar_config_drive_en(ruta_cfg_drive, &cfg_drive)?;

    res.sin_cambios = res.planillas_subidas == 0
        && res.planillas_descargadas == 0
        && res.conflictos_respaldados == 0
        && res.carpetas_creadas == 0
        && res.resumenes_fusionados == 0
        && !res.ia_subida
        && res.ia_cambios_fusionados == 0
        && !res.uso_subido
        && res.uso_instancias_fusionadas == 0
        && !res.config_fusionada;
    res.resumen_mensaje = construir_resumen_mensaje(&res);

    Ok(res)
}

/// Ejecuta una sincronización real contra Google Drive (`appDataFolder`) usando las rutas activas
/// de Lokus, renovando el `access_token` automáticamente si está vencido.
pub async fn sincronizar_google_drive() -> Result<ResultadoDriveSync, String> {
    let _lock = SyncLockGuard::adquirir()?;
    let ruta_cfg = ruta_config_drive_sync();
    let mut cfg = cargar_config_drive_en(&ruta_cfg);

    if cfg.refresh_token.trim().is_empty() && cfg.access_token.trim().is_empty() {
        return Err("Primero conecta tu cuenta de Google Drive para sincronizar.".to_string());
    }

    let access_token = asegurar_access_token_vigente(&mut cfg, &ruta_cfg).await?;
    let transport = GoogleDriveTransport::nuevo(access_token)?;

    let carpeta_archivos = obtener_carpeta_archivos();
    let ruta_uso = ruta_uso_catalogos();
    let ruta_mem = ruta_memoria();
    let mut settings = cargar_settings();

    let resultado = sincronizar_con_transporte_en(
        &transport,
        &carpeta_archivos,
        &ruta_cfg,
        &ruta_uso,
        &ruta_mem,
        &mut settings,
    )
    .await;

    match resultado {
        Ok(res) => {
            if res.config_fusionada {
                guardar_settings(&settings)?;
            }
            Ok(res)
        }
        Err(err) => {
            let mut cfg_err = cargar_config_drive_en(&ruta_cfg);
            cfg_err.ultimo_error = err.clone();
            let _ = guardar_config_drive_en(&ruta_cfg, &cfg_err);
            Err(err)
        }
    }
}

// ---------------------------------------------------------------------------
// Pruebas unitarias (PKCE + sincronización PC <-> Celular + conflictos)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aprendizaje::ReglaAprendida;
    use crate::models::AudienciaInput;
    use crate::uso_catalogos::{registrar_delta_audiencia_en, NOMBRE_ARCHIVO_USO};
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct MockSyncTransport {
        archivos: Arc<Mutex<BTreeMap<String, (ArchivoRemoto, Vec<u8>)>>>,
        subidas_totales: Arc<AtomicU64>,
        seq: Arc<AtomicU64>,
    }

    impl MockSyncTransport {
        fn conteo_subidas(&self) -> u64 {
            self.subidas_totales.load(Ordering::SeqCst)
        }

        fn volcado_textos_remotos(&self) -> String {
            let guard = self.archivos.lock().unwrap();
            guard
                .values()
                .map(|(_, bytes)| String::from_utf8_lossy(bytes).to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    impl SyncTransport for MockSyncTransport {
        fn listar_archivos<'a>(&'a self) -> BoxFuture<'a, Result<Vec<ArchivoRemoto>, String>> {
            Box::pin(async move {
                let guard = self.archivos.lock().unwrap();
                Ok(guard.values().map(|(meta, _)| meta.clone()).collect())
            })
        }

        fn descargar_archivo<'a>(
            &'a self,
            file_id: &'a str,
        ) -> BoxFuture<'a, Result<Vec<u8>, String>> {
            Box::pin(async move {
                let guard = self.archivos.lock().unwrap();
                guard
                    .get(file_id)
                    .map(|(_, bytes)| bytes.clone())
                    .ok_or_else(|| format!("Archivo remoto '{}' no encontrado", file_id))
            })
        }

        fn subir_o_actualizar<'a>(
            &'a self,
            nombre: &'a str,
            _mime_type: &'a str,
            bytes: &'a [u8],
            file_id_existente: Option<&'a str>,
        ) -> BoxFuture<'a, Result<ArchivoRemoto, String>> {
            Box::pin(async move {
                let mut guard = self.archivos.lock().unwrap();
                self.subidas_totales.fetch_add(1, Ordering::SeqCst);
                let n = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
                let id = file_id_existente
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("drive-id-{}", n));
                let meta = ArchivoRemoto {
                    id: id.clone(),
                    name: nombre.to_string(),
                    modified_time: format!(
                        "2026-11-24T12:{:02}:{:02}Z",
                        (n / 60) % 60,
                        n % 60
                    ),
                    size: bytes.len() as u64,
                };
                guard.insert(id, (meta.clone(), bytes.to_vec()));
                Ok(meta)
            })
        }

        fn eliminar_archivo<'a>(&'a self, file_id: &'a str) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                let mut guard = self.archivos.lock().unwrap();
                guard.remove(file_id);
                Ok(())
            })
        }
    }

    fn preparar_entorno(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lokus_sync_drive_{}_{}_{}",
            nombre,
            std::process::id(),
            CONTADOR_SYNC.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("archivos")).unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        dir
    }

    fn crear_planilla_prueba(ruta: &Path, defensor: &str, radicado: &str, fecha: &str) {
        if let Some(p) = ruta.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let plantilla = include_bytes!("../../templates/plantilla_audiencias.xlsx");
        fs::write(ruta, plantilla).unwrap();
        let mut book = umya_spreadsheet::reader::xlsx::read(ruta).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();
        ws.get_cell_mut((2, 8))
            .set_value_string(format!("DEFENSOR PÚBLICO: {}", defensor));
        ws.get_cell_mut((1, 11)).set_value_number(1);
        ws.get_cell_mut((2, 11)).set_value_string(radicado);
        ws.get_cell_mut((3, 11))
            .set_value_string("Audiencia concentrada");
        ws.get_cell_mut((4, 11)).set_value_string(fecha);
        ws.get_cell_mut((5, 11)).set_value_string("09:00");
        ws.get_cell_mut((7, 11)).set_value_string("SI");
        umya_spreadsheet::writer::xlsx::write(&book, ruta).unwrap();
    }

    fn agregar_segunda_audiencia_prueba(ruta: &Path, radicado: &str, fecha: &str) {
        let mut book = umya_spreadsheet::reader::xlsx::read(ruta).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();
        ws.get_cell_mut((1, 12)).set_value_number(2);
        ws.get_cell_mut((2, 12)).set_value_string(radicado);
        ws.get_cell_mut((3, 12))
            .set_value_string("Audiencia de acusación");
        ws.get_cell_mut((4, 12)).set_value_string(fecha);
        ws.get_cell_mut((5, 12)).set_value_string("14:00");
        ws.get_cell_mut((7, 12)).set_value_string("SI");
        umya_spreadsheet::writer::xlsx::write(&book, ruta).unwrap();
        invalidar_cache(ruta);
    }

    fn sembrar_regla_ia(ruta_mem: &Path, instancia_id: &str, regla_id: &str, texto_regla: &str) {
        let regla: ReglaAprendida = serde_json::from_value(serde_json::json!({
            "id": regla_id,
            "tipo_error": "juzgado",
            "regla": texto_regla,
            "estado": "activa",
            "campo_objetivo": "juzgado",
            "defensor_alcance": "carlos mendoza",
            "refuerzos": 3,
            "alpha": 3.5,
            "beta": 1.0,
        }))
        .unwrap();
        let mut mem = MemoriaAprendizaje::default();
        mem.instancia_id = instancia_id.to_string();
        mem.reglas.push(regla);
        fusionar_memoria_ia_desde_paquete_en(ruta_mem, &mem).unwrap();
    }

    #[test]
    fn test_pkce_s256_vector_rfc7636() {
        // Vector oficial RFC 7636 Apéndice B
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        let challenge = pkce_challenge_s256(verifier);
        assert_eq!(challenge, expected);
        assert!(!challenge.contains('='));
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));

        let generado = generar_token_pkce("prueba");
        assert!((43..=128).contains(&generado.len()));
        assert!(!generado.contains('='));
    }

    #[tokio::test]
    async fn test_sync_pc_celular_completo_idempotencia_y_sin_api_keys() {
        let pc = preparar_entorno("pc");
        let cel = preparar_entorno("cel");
        let cloud = MockSyncTransport::default();

        let pc_arch = pc.join("archivos");
        let pc_cfg = pc.join("config/drive_sync.json");
        let pc_uso = pc.join("config").join(NOMBRE_ARCHIVO_USO);
        let pc_mem = pc.join("config/ia_aprendizaje.json");

        let cel_arch = cel.join("archivos");
        let cel_cfg = cel.join("config/drive_sync.json");
        let cel_uso = cel.join("config").join(NOMBRE_ARCHIVO_USO);
        let cel_mem = cel.join("config/ia_aprendizaje.json");

        // Preparar datos en PC (incluyendo API keys secretas que NUNCA deben subir)
        crear_planilla_prueba(
            &pc_arch.join("2026/Carlos_Noviembre_2026.xlsx"),
            "Carlos Mendoza",
            "RAD-PC-001",
            "10/11/2026",
        );
        let mut settings_pc = AppSettings {
            openai_api_key: "sk-secreto-pc-no-subir-12345".to_string(),
            gemini_api_key: "AIzaSecretoPcNoSubir98765".to_string(),
            defensores: vec!["Carlos Mendoza".to_string()],
            juzgados: vec!["Juzgado Especializado de Prueba".to_string()],
            tipos_audiencia: vec!["Audiencia Especial PC".to_string()],
            ..Default::default()
        };
        sembrar_regla_ia(&pc_mem, "inst-pc-1", "regla-pc-1", "J1P es Juzgado 1 Penal");
        let aud_pc = AudienciaInput {
            radicado: "RAD-PC-001".into(),
            tipo: "Audiencia Especial PC".into(),
            fecha: "10/11/2026".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado Especializado de Prueba".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        registrar_delta_audiencia_en(&pc_uso, &pc_arch, &settings_pc, None, Some(&aud_pc))
            .unwrap();

        // 1. PC sincroniza -> sube Excel, IA, uso_catalogos y listas (SIN API keys)
        let r1 = sincronizar_con_transporte_en(
            &cloud,
            &pc_arch,
            &pc_cfg,
            &pc_uso,
            &pc_mem,
            &mut settings_pc,
        )
        .await
        .unwrap();
        assert_eq!(r1.planillas_subidas, 1);
        assert!(r1.ia_subida);
        assert!(r1.uso_subido);
        assert!(!r1.sin_cambios);

        // Verificar que NINGUNA API key se filtró a Google Drive
        let dump_nube = cloud.volcado_textos_remotos();
        assert!(!dump_nube.contains("sk-secreto-pc-no-subir-12345"));
        assert!(!dump_nube.contains("AIzaSecretoPcNoSubir98765"));

        // 2. Celular (limpio, con sus propias API keys locales) sincroniza -> descarga todo
        let mut settings_cel = AppSettings {
            openai_api_key: "sk-clave-propia-celular".to_string(),
            ..Default::default()
        };
        let r2 = sincronizar_con_transporte_en(
            &cloud,
            &cel_arch,
            &cel_cfg,
            &cel_uso,
            &cel_mem,
            &mut settings_cel,
        )
        .await
        .unwrap();
        assert_eq!(r2.planillas_descargadas, 1);
        assert!(r2.ia_cambios_fusionados > 0);
        assert_eq!(r2.uso_instancias_fusionadas, 1);
        assert!(r2.config_fusionada);

        // Verificar integridad en Celular
        let ruta_cel_excel = cel_arch.join("2026/Carlos_Noviembre_2026.xlsx");
        assert!(ruta_cel_excel.exists());
        let (regs_cel, _, def_cel) = leer_archivo_en(&ruta_cel_excel).unwrap();
        assert_eq!(regs_cel.len(), 1);
        assert_eq!(regs_cel[0].radicado, "RAD-PC-001");
        assert_eq!(def_cel, "Carlos Mendoza");
        assert!(settings_cel.defensores.contains(&"Carlos Mendoza".to_string()));
        assert!(settings_cel
            .juzgados
            .contains(&"Juzgado Especializado de Prueba".to_string()));
        assert!(settings_cel
            .tipos_audiencia
            .contains(&"Audiencia Especial PC".to_string()));
        // Sus propias API keys locales siguen intactas
        assert_eq!(settings_cel.openai_api_key, "sk-clave-propia-celular");

        // 3. Idempotencia en 2.ª vuelta consecutiva (tanto en Celular como en PC)
        let subidas_antes_cel = cloud.conteo_subidas();
        let r3 = sincronizar_con_transporte_en(
            &cloud,
            &cel_arch,
            &cel_cfg,
            &cel_uso,
            &cel_mem,
            &mut settings_cel,
        )
        .await
        .unwrap();
        assert!(r3.sin_cambios, "r3={:?}", r3);
        assert_eq!(r3.planillas_subidas, 0);
        assert_eq!(r3.planillas_descargadas, 0);
        assert_eq!(cloud.conteo_subidas(), subidas_antes_cel);

        let subidas_antes_pc = cloud.conteo_subidas();
        let r4 = sincronizar_con_transporte_en(
            &cloud,
            &pc_arch,
            &pc_cfg,
            &pc_uso,
            &pc_mem,
            &mut settings_pc,
        )
        .await
        .unwrap();
        assert!(r4.sin_cambios);
        assert_eq!(cloud.conteo_subidas(), subidas_antes_pc);

        let _ = fs::remove_dir_all(&pc);
        let _ = fs::remove_dir_all(&cel);
    }

    #[tokio::test]
    async fn test_sync_conflicto_simultaneo_preserva_ambas_versiones_sin_perdida() {
        let pc = preparar_entorno("conf_pc");
        let cel = preparar_entorno("conf_cel");
        let cloud = MockSyncTransport::default();

        let pc_arch = pc.join("archivos");
        let pc_cfg = pc.join("config/drive_sync.json");
        let pc_uso = pc.join("config").join(NOMBRE_ARCHIVO_USO);
        let pc_mem = pc.join("config/ia_aprendizaje.json");

        let cel_arch = cel.join("archivos");
        let cel_cfg = cel.join("config/drive_sync.json");
        let cel_uso = cel.join("config").join(NOMBRE_ARCHIVO_USO);
        let cel_mem = cel.join("config/ia_aprendizaje.json");

        let mut settings_pc = AppSettings::default();
        let mut settings_cel = AppSettings::default();

        let rel = "Audiencias_Compartidas.xlsx";
        crear_planilla_prueba(&pc_arch.join(rel), "Ana Pérez", "RAD-BASE-1", "01/11/2026");

        // Sync inicial: PC -> nube -> Celular
        sincronizar_con_transporte_en(
            &cloud,
            &pc_arch,
            &pc_cfg,
            &pc_uso,
            &pc_mem,
            &mut settings_pc,
        )
        .await
        .unwrap();
        sincronizar_con_transporte_en(
            &cloud,
            &cel_arch,
            &cel_cfg,
            &cel_uso,
            &cel_mem,
            &mut settings_cel,
        )
        .await
        .unwrap();

        // Edición concurrente SIN sincronizar entre ambos:
        // PC agrega RAD-EDIT-PC y sincroniza primero
        agregar_segunda_audiencia_prueba(&pc_arch.join(rel), "RAD-EDIT-PC", "12/11/2026");
        let r_pc = sincronizar_con_transporte_en(
            &cloud,
            &pc_arch,
            &pc_cfg,
            &pc_uso,
            &pc_mem,
            &mut settings_pc,
        )
        .await
        .unwrap();
        assert_eq!(r_pc.planillas_subidas, 1);

        // Celular agrega RAD-EDIT-CEL sobre la misma planilla y ahora sincroniza -> CONFLICTO
        agregar_segunda_audiencia_prueba(&cel_arch.join(rel), "RAD-EDIT-CEL", "13/11/2026");
        let r_cel = sincronizar_con_transporte_en(
            &cloud,
            &cel_arch,
            &cel_cfg,
            &cel_uso,
            &cel_mem,
            &mut settings_cel,
        )
        .await
        .unwrap();
        assert_eq!(r_cel.conflictos_respaldados, 1);

        // Verificar que en Celular NO se perdió NINGUNA audiencia:
        // - La planilla original conserva RAD-EDIT-CEL
        // - La copia `*_nube_conflicto.xlsx` conserva RAD-EDIT-PC
        let (regs_local, _, _) = leer_archivo_en(&cel_arch.join(rel)).unwrap();
        assert!(regs_local.iter().any(|r| r.radicado == "RAD-EDIT-CEL"));

        let copia_conflicto = cel_arch.join("Audiencias_Compartidas_nube_conflicto.xlsx");
        assert!(copia_conflicto.exists());
        let (regs_remoto, _, _) = leer_archivo_en(&copia_conflicto).unwrap();
        assert!(regs_remoto.iter().any(|r| r.radicado == "RAD-EDIT-PC"));

        // Una 2.ª vuelta inmediata en Celular debe ser idempotente (cero conflictos extra)
        let r_cel_2 = sincronizar_con_transporte_en(
            &cloud,
            &cel_arch,
            &cel_cfg,
            &cel_uso,
            &cel_mem,
            &mut settings_cel,
        )
        .await
        .unwrap();
        assert!(r_cel_2.sin_cambios);
        assert_eq!(r_cel_2.conflictos_respaldados, 0);

        let _ = fs::remove_dir_all(&pc);
        let _ = fs::remove_dir_all(&cel);
    }
}
