//! Registro histórico persistente de frecuencias de uso para `Tipo de audiencia`, `Hora` y `Juzgado`.
//!
//! Se almacena junto a `settings.json` (`uso_catalogos.json`), realiza un único escaneo inicial
//! sobre las planillas Excel existentes del usuario al actualizar y se actualiza de forma incremental
//! al registrar, editar o borrar filas individuales, conservando intacto el histórico acumulado si
//! el usuario elimina un archivo Excel o una carpeta completa.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::config::{cargar_settings, ruta_en_carpeta_config, AppSettings};
use crate::estadistica_ia::juzgados_base;
use crate::excel::{leer_archivo_en, listar_archivos_en, obtener_carpeta_archivos};
use crate::models::{
    normalizar_clave_catalogo, AudienciaInput, HORAS_RAPIDAS, TIPOS_AUDIENCIA,
};

pub const NOMBRE_ARCHIVO_USO: &str = "uso_catalogos.json";
const VERSION_USO: u32 = 1;

static BLOQUEO_USO: Mutex<()> = Mutex::new(());
static CONTADOR_INSTANCIA: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct EntradaUsoCatalogo {
    pub nombre_display: String,
    pub conteo: u32,
    /// `"predeterminado"` | `"lista_usuario"` | `"otra"`
    pub origen_historico: String,
    pub primera_vez: String,
    pub ultima_vez: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct UsoCatalogos {
    pub version: u32,
    pub instancia_id: String,
    pub sembrado_inicial: bool,
    pub sembrado_en: String,
    pub ultima_actualizacion: String,
    pub total_audiencias_contabilizadas: u32,
    pub pendientes_compartir: u32,
    pub tipos_sin_especificar: u32,
    pub juzgados_sin_especificar: u32,
    pub horas_sin_especificar: u32,
    pub tipos_audiencia: BTreeMap<String, EntradaUsoCatalogo>,
    pub juzgados: BTreeMap<String, EntradaUsoCatalogo>,
    pub horas_exactas: BTreeMap<String, u32>,
    pub fuentes_importadas: BTreeSet<String>,
    #[serde(default)]
    pub paquetes_importados: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ItemUsoCatalogo {
    pub nombre: String,
    /// `"predeterminado"` | `"agregado_usuario"` | `"otra"` | `"eliminado_de_lista"`
    pub origen: String,
    pub conteo: u32,
    pub porcentaje: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ReporteUsoCampoCatalogo {
    pub total_audiencias: u32,
    pub total_con_valor: u32,
    pub total_sin_especificar: u32,
    pub porcentaje_sin_especificar: f64,
    pub total_opciones_predeterminadas: usize,
    pub total_opciones_lista_usuario: usize,
    pub total_opciones_otra: usize,
    pub total_opciones_eliminadas_tras_uso: usize,
    pub nuevos_no_predeterminados: Vec<ItemUsoCatalogo>,
    pub predeterminados_sin_uso: Vec<String>,
    pub agregados_usuario_sin_uso: Vec<String>,
    pub detalle: Vec<ItemUsoCatalogo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ItemUsoHora {
    pub hora_24: String,
    pub etiqueta_12h: String,
    pub es_hora_rapida: bool,
    pub conteo: u32,
    pub porcentaje: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ItemUsoFranja {
    pub franja: String,
    pub conteo: u32,
    pub porcentaje: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ReporteUsoHoras {
    pub total_audiencias: u32,
    pub total_con_hora: u32,
    pub total_sin_hora: u32,
    pub porcentaje_sin_hora: f64,
    pub horas_rapidas_predeterminadas: Vec<ItemUsoHora>,
    pub horas_rapidas_sin_uso: Vec<String>,
    pub horas_mas_frecuentes_no_rapidas: Vec<ItemUsoHora>,
    pub detalle_por_hora_exacta: Vec<ItemUsoHora>,
    pub resumen_por_franja_horaria: Vec<ItemUsoFranja>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ReporteUsoCatalogos {
    pub total_audiencias_contabilizadas: u32,
    pub sembrado_inicial_completado: bool,
    pub sembrado_en: String,
    pub ultima_actualizacion: String,
    pub tipos_audiencia: ReporteUsoCampoCatalogo,
    pub juzgados: ReporteUsoCampoCatalogo,
    pub horas: ReporteUsoHoras,
}

fn colapsar_espacios(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn redondear_2_decimales(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn calcular_porcentaje(conteo: u32, total: u32) -> f64 {
    if total == 0 {
        0.0
    } else {
        redondear_2_decimales((conteo as f64 / total as f64) * 100.0)
    }
}

fn generar_instancia_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = CONTADOR_INSTANCIA.fetch_add(1, Ordering::Relaxed);
    format!("inst-{:x}-{:04x}", nanos, seq % 0xffff)
}

pub fn ruta_uso_catalogos() -> PathBuf {
    ruta_en_carpeta_config(NOMBRE_ARCHIVO_USO)
}

pub fn normalizar_hora_hh_mm(raw: &str) -> Option<String> {
    let limpio = raw.trim();
    if limpio.is_empty() {
        return None;
    }
    let mut partes = limpio.split(':');
    let h_str = partes.next()?.trim();
    let m_str = partes.next()?.trim();
    let h = h_str.parse::<u32>().ok()?;
    let m = m_str.parse::<u32>().ok()?;
    if h <= 23 && m <= 59 {
        Some(format!("{:02}:{:02}", h, m))
    } else {
        None
    }
}

pub fn formatear_hora_12h(hh_mm: &str) -> String {
    let mut partes = hh_mm.split(':');
    let h = partes
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let m = partes
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let (h12, per) = if h == 0 {
        (12, "AM")
    } else if h < 12 {
        (h, "AM")
    } else if h == 12 {
        (12, "PM")
    } else {
        (h - 12, "PM")
    };
    format!("{}:{:02} {}", h12, m, per)
}

fn es_hora_rapida(hh_mm: &str) -> bool {
    HORAS_RAPIDAS.iter().any(|(h24, _)| *h24 == hh_mm)
}

const FRANJAS_ORDEN: [&str; 9] = [
    "Madrugada (00:00-05:59)",
    "Mañana temprana (06:00-07:59)",
    "Mañana primera (08:00-09:59)",
    "Mañana media (10:00-11:59)",
    "Mediodía (12:00-13:59)",
    "Tarde primera (14:00-15:59)",
    "Tarde cierre (16:00-17:59)",
    "Noche (18:00-23:59)",
    "Sin hora",
];

fn franja_de_hora(hh_mm: &str) -> &'static str {
    let h = hh_mm
        .split(':')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    match h {
        0..=5 => FRANJAS_ORDEN[0],
        6..=7 => FRANJAS_ORDEN[1],
        8..=9 => FRANJAS_ORDEN[2],
        10..=11 => FRANJAS_ORDEN[3],
        12..=13 => FRANJAS_ORDEN[4],
        14..=15 => FRANJAS_ORDEN[5],
        16..=17 => FRANJAS_ORDEN[6],
        _ => FRANJAS_ORDEN[7],
    }
}

pub fn cargar_uso_con_lock_en(ruta: &Path) -> UsoCatalogos {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    cargar_uso_sin_lock_en(ruta)
}

pub fn cargar_uso_sin_lock_en(ruta: &Path) -> UsoCatalogos {
    let Ok(contenido) = fs::read_to_string(ruta) else {
        return UsoCatalogos {
            version: VERSION_USO,
            instancia_id: generar_instancia_id(),
            ..Default::default()
        };
    };
    match serde_json::from_str::<UsoCatalogos>(&contenido) {
        Ok(mut uso) => {
            if uso.version == 0 {
                uso.version = VERSION_USO;
            }
            if uso.instancia_id.trim().is_empty() {
                uso.instancia_id = generar_instancia_id();
            }
            uso
        }
        Err(_) => {
            let _ = fs::rename(ruta, ruta.with_extension("json.bak"));
            UsoCatalogos {
                version: VERSION_USO,
                instancia_id: generar_instancia_id(),
                ..Default::default()
            }
        }
    }
}

fn guardar_uso_sin_lock_en(ruta: &Path, uso: &UsoCatalogos) -> Result<(), String> {
    if let Some(parent) = ruta.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo preparar carpeta para uso de catálogos: {}", e))?;
    }
    let json = serde_json::to_string_pretty(uso)
        .map_err(|e| format!("Error serializando uso de catálogos: {}", e))?;
    let seq = CONTADOR_INSTANCIA.fetch_add(1, Ordering::Relaxed);
    let tmp = ruta.with_extension(format!("{}.{}.tmp", std::process::id(), seq));
    fs::write(&tmp, json).map_err(|e| format!("Error escribiendo uso de catálogos: {}", e))?;
    if let Err(e) = fs::rename(&tmp, ruta) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Error guardando uso de catálogos: {}", e));
    }
    Ok(())
}

fn resolver_display_y_origen_tipo(limpio: &str, settings: &AppSettings) -> (String, String) {
    let clave = normalizar_clave_catalogo(limpio);
    if let Some(&oficial) = TIPOS_AUDIENCIA
        .iter()
        .find(|&&t| t != "Otra" && normalizar_clave_catalogo(t) == clave)
    {
        return (oficial.to_string(), "predeterminado".to_string());
    }
    if let Some(usr) = settings
        .tipos_audiencia
        .iter()
        .find(|t| normalizar_clave_catalogo(t) == clave)
    {
        return (usr.clone(), "lista_usuario".to_string());
    }
    (limpio.to_string(), "otra".to_string())
}

fn resolver_display_y_origen_juzgado(limpio: &str, settings: &AppSettings) -> (String, String) {
    let clave = normalizar_clave_catalogo(limpio);
    if let Some(oficial) = juzgados_base()
        .iter()
        .find(|j| normalizar_clave_catalogo(j) == clave)
    {
        return (oficial.clone(), "predeterminado".to_string());
    }
    if let Some(usr) = settings
        .juzgados
        .iter()
        .find(|j| normalizar_clave_catalogo(j) == clave)
    {
        return (usr.clone(), "lista_usuario".to_string());
    }
    (limpio.to_string(), "otra".to_string())
}

fn puntaje_calidad_display(s: &str) -> (usize, usize) {
    let acentos = s
        .chars()
        .filter(|c| matches!(c, 'á' | 'é' | 'í' | 'ó' | 'ú' | 'ü' | 'ñ' | 'Á' | 'É' | 'Í' | 'Ó' | 'Ú' | 'Ü' | 'Ñ'))
        .count();
    let mayus = s.chars().filter(|c| c.is_uppercase()).count();
    (acentos, mayus)
}

fn acumular_campo_catalogo(
    mapa: &mut BTreeMap<String, EntradaUsoCatalogo>,
    sin_especificar: &mut u32,
    raw: &str,
    resolver: impl FnOnce(&str) -> (String, String),
    ahora: &str,
) {
    let limpio = colapsar_espacios(raw);
    let clave = normalizar_clave_catalogo(&limpio);
    if limpio.is_empty() || clave.is_empty() {
        *sin_especificar = sin_especificar.saturating_add(1);
        return;
    }

    let (display, origen) = resolver(&limpio);
    let entry = mapa.entry(clave).or_insert_with(|| EntradaUsoCatalogo {
        nombre_display: display.clone(),
        conteo: 0,
        origen_historico: origen.clone(),
        primera_vez: ahora.to_string(),
        ultima_vez: ahora.to_string(),
    });
    entry.conteo = entry.conteo.saturating_add(1);
    entry.ultima_vez = ahora.to_string();
    if entry.primera_vez.is_empty() {
        entry.primera_vez = ahora.to_string();
    }

    if origen == "predeterminado" {
        entry.origen_historico = "predeterminado".to_string();
        entry.nombre_display = display;
    } else if origen == "lista_usuario" && entry.origen_historico != "predeterminado" {
        entry.origen_historico = "lista_usuario".to_string();
        entry.nombre_display = display;
    } else if entry.origen_historico.is_empty() {
        entry.origen_historico = origen;
        entry.nombre_display = display;
    } else if entry.origen_historico == "otra"
        && puntaje_calidad_display(&display) > puntaje_calidad_display(&entry.nombre_display)
    {
        entry.nombre_display = display;
    }
}

fn restar_campo_catalogo(
    mapa: &mut BTreeMap<String, EntradaUsoCatalogo>,
    sin_especificar: &mut u32,
    raw: &str,
) {
    let limpio = colapsar_espacios(raw);
    let clave = normalizar_clave_catalogo(&limpio);
    if limpio.is_empty() || clave.is_empty() {
        *sin_especificar = sin_especificar.saturating_sub(1);
        return;
    }

    let mut eliminar = false;
    if let Some(entry) = mapa.get_mut(&clave) {
        entry.conteo = entry.conteo.saturating_sub(1);
        if entry.conteo == 0 && entry.origen_historico == "otra" {
            eliminar = true;
        }
    }
    if eliminar {
        mapa.remove(&clave);
    }
}

fn acumular_valores_en_uso(
    uso: &mut UsoCatalogos,
    tipo: &str,
    hora: &str,
    juzgado: &str,
    settings: &AppSettings,
    ahora: &str,
) {
    uso.total_audiencias_contabilizadas = uso.total_audiencias_contabilizadas.saturating_add(1);
    acumular_campo_catalogo(
        &mut uso.tipos_audiencia,
        &mut uso.tipos_sin_especificar,
        tipo,
        |s| resolver_display_y_origen_tipo(s, settings),
        ahora,
    );
    acumular_campo_catalogo(
        &mut uso.juzgados,
        &mut uso.juzgados_sin_especificar,
        juzgado,
        |s| resolver_display_y_origen_juzgado(s, settings),
        ahora,
    );
    if let Some(hh_mm) = normalizar_hora_hh_mm(hora) {
        *uso.horas_exactas.entry(hh_mm).or_insert(0) += 1;
    } else {
        uso.horas_sin_especificar = uso.horas_sin_especificar.saturating_add(1);
    }
    uso.ultima_actualizacion = ahora.to_string();
}

fn restar_audiencia_de_uso(uso: &mut UsoCatalogos, reg: &AudienciaInput, ahora: &str) {
    uso.total_audiencias_contabilizadas = uso.total_audiencias_contabilizadas.saturating_sub(1);
    restar_campo_catalogo(&mut uso.tipos_audiencia, &mut uso.tipos_sin_especificar, &reg.tipo);
    restar_campo_catalogo(
        &mut uso.juzgados,
        &mut uso.juzgados_sin_especificar,
        &reg.juzgado,
    );
    if let Some(hh_mm) = normalizar_hora_hh_mm(&reg.hora) {
        if let Some(c) = uso.horas_exactas.get_mut(&hh_mm) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                uso.horas_exactas.remove(&hh_mm);
            }
        }
    } else {
        uso.horas_sin_especificar = uso.horas_sin_especificar.saturating_sub(1);
    }
    uso.ultima_actualizacion = ahora.to_string();
}

fn sincronizar_listas_usuario_en_uso(uso: &mut UsoCatalogos, settings: &AppSettings, ahora: &str) {
    for t in &settings.tipos_audiencia {
        let clave = normalizar_clave_catalogo(t);
        if clave.is_empty() || clave == "otra" {
            continue;
        }
        let es_default = TIPOS_AUDIENCIA
            .iter()
            .any(|&d| d != "Otra" && normalizar_clave_catalogo(d) == clave);
        if es_default {
            continue;
        }
        let entry = uso
            .tipos_audiencia
            .entry(clave)
            .or_insert_with(|| EntradaUsoCatalogo {
                nombre_display: t.clone(),
                conteo: 0,
                origen_historico: "lista_usuario".to_string(),
                primera_vez: ahora.to_string(),
                ultima_vez: ahora.to_string(),
            });
        if entry.origen_historico != "predeterminado" {
            entry.origen_historico = "lista_usuario".to_string();
            entry.nombre_display = t.clone();
        }
    }

    for j in &settings.juzgados {
        let clave = normalizar_clave_catalogo(j);
        if clave.is_empty() || clave == "otra" {
            continue;
        }
        let es_default = juzgados_base()
            .iter()
            .any(|d| normalizar_clave_catalogo(d) == clave);
        if es_default {
            continue;
        }
        let entry = uso
            .juzgados
            .entry(clave)
            .or_insert_with(|| EntradaUsoCatalogo {
                nombre_display: j.clone(),
                conteo: 0,
                origen_historico: "lista_usuario".to_string(),
                primera_vez: ahora.to_string(),
                ultima_vez: ahora.to_string(),
            });
        if entry.origen_historico != "predeterminado" {
            entry.origen_historico = "lista_usuario".to_string();
            entry.nombre_display = j.clone();
        }
    }
}

fn inicializar_uso_si_falta(
    uso: &mut UsoCatalogos,
    carpeta_archivos: &Path,
    settings: &AppSettings,
    ahora: &str,
) -> bool {
    if uso.sembrado_inicial {
        return false;
    }

    let nombres = listar_archivos_en(carpeta_archivos).unwrap_or_default();
    for nombre in nombres {
        let ruta_excel = carpeta_archivos.join(&nombre);
        if let Ok((registros, _, _)) = leer_archivo_en(&ruta_excel) {
            for r in &registros {
                acumular_valores_en_uso(uso, &r.tipo, &r.hora, &r.juzgado, settings, ahora);
            }
        }
    }
    sincronizar_listas_usuario_en_uso(uso, settings, ahora);
    uso.sembrado_inicial = true;
    uso.sembrado_en = ahora.to_string();
    if uso.ultima_actualizacion.is_empty() {
        uso.ultima_actualizacion = ahora.to_string();
    }
    if uso.total_audiencias_contabilizadas > 0
        || !settings.juzgados.is_empty()
        || !settings.tipos_audiencia.is_empty()
    {
        let extra_listas = (settings.juzgados.len() + settings.tipos_audiencia.len()) as u32;
        uso.pendientes_compartir = uso
            .pendientes_compartir
            .max(uso.total_audiencias_contabilizadas.saturating_add(extra_listas));
    }
    true
}

/// Garantiza que el escaneo único inicial sobre las planillas existentes ya haya corrido.
pub fn asegurar_uso_inicializado_en(
    ruta_uso: &Path,
    carpeta_archivos: &Path,
    settings: &AppSettings,
) -> UsoCatalogos {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let mut uso = cargar_uso_sin_lock_en(ruta_uso);
    let ahora = Local::now().to_rfc3339();
    let cambio = inicializar_uso_si_falta(&mut uso, carpeta_archivos, settings, &ahora);
    if cambio || !ruta_uso.exists() {
        let _ = guardar_uso_sin_lock_en(ruta_uso, &uso);
    }
    uso
}

pub fn asegurar_uso_inicializado_actual() -> UsoCatalogos {
    let settings = cargar_settings();
    asegurar_uso_inicializado_en(&ruta_uso_catalogos(), &obtener_carpeta_archivos(), &settings)
}

pub fn hay_datos_uso_exportables(uso: &UsoCatalogos, settings: &AppSettings) -> bool {
    uso.total_audiencias_contabilizadas > 0
        || !settings.juzgados.is_empty()
        || !settings.tipos_audiencia.is_empty()
}

fn podar_ceros_no_activos(
    mapa: &mut BTreeMap<String, EntradaUsoCatalogo>,
    lista_usuario: &[String],
) {
    let claves_usr: BTreeSet<String> = lista_usuario
        .iter()
        .map(|s| normalizar_clave_catalogo(s))
        .filter(|c| !c.is_empty())
        .collect();
    mapa.retain(|clave, entry| {
        entry.conteo > 0
            || entry.origen_historico == "predeterminado"
            || claves_usr.contains(clave)
    });
}

/// Actualiza contadores de forma incremental al crear (`None -> Some`), editar (`Some -> Some`)
/// o eliminar una fila (`Some -> None`).
pub fn registrar_delta_audiencia_en(
    ruta_uso: &Path,
    carpeta_archivos: &Path,
    settings: &AppSettings,
    anterior: Option<&AudienciaInput>,
    nuevo: Option<&AudienciaInput>,
) -> Result<UsoCatalogos, String> {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let mut uso = cargar_uso_sin_lock_en(ruta_uso);
    let ahora = Local::now().to_rfc3339();
    inicializar_uso_si_falta(&mut uso, carpeta_archivos, settings, &ahora);

    if let Some(ant) = anterior {
        restar_audiencia_de_uso(&mut uso, ant, &ahora);
    }
    if let Some(n) = nuevo {
        acumular_valores_en_uso(&mut uso, &n.tipo, &n.hora, &n.juzgado, settings, &ahora);
    }
    podar_ceros_no_activos(&mut uso.tipos_audiencia, &settings.tipos_audiencia);
    podar_ceros_no_activos(&mut uso.juzgados, &settings.juzgados);
    if !hay_datos_uso_exportables(&uso, settings) {
        uso.pendientes_compartir = 0;
    } else if anterior.is_some() || nuevo.is_some() {
        uso.pendientes_compartir = uso.pendientes_compartir.saturating_add(1);
    }

    guardar_uso_sin_lock_en(ruta_uso, &uso)?;
    Ok(uso)
}

pub fn registrar_delta_audiencia_actual(
    anterior: Option<&AudienciaInput>,
    nuevo: Option<&AudienciaInput>,
) -> Result<UsoCatalogos, String> {
    let settings = cargar_settings();
    registrar_delta_audiencia_en(
        &ruta_uso_catalogos(),
        &obtener_carpeta_archivos(),
        &settings,
        anterior,
        nuevo,
    )
}

pub fn registrar_cambio_lista_usuario_en(
    ruta_uso: &Path,
    carpeta_archivos: &Path,
    settings: &AppSettings,
    es_juzgado: bool,
    agregado: Option<&str>,
    eliminado: Option<&str>,
) -> Result<UsoCatalogos, String> {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let mut uso = cargar_uso_sin_lock_en(ruta_uso);
    let ahora = Local::now().to_rfc3339();
    inicializar_uso_si_falta(&mut uso, carpeta_archivos, settings, &ahora);

    let mut hubo_cambio = false;
    let mapa = if es_juzgado {
        &mut uso.juzgados
    } else {
        &mut uso.tipos_audiencia
    };

    if let Some(nombre) = agregado {
        let limpio = colapsar_espacios(nombre);
        let clave = normalizar_clave_catalogo(&limpio);
        let es_default = if es_juzgado {
            juzgados_base()
                .iter()
                .any(|d| normalizar_clave_catalogo(d) == clave)
        } else {
            TIPOS_AUDIENCIA
                .iter()
                .any(|&d| d != "Otra" && normalizar_clave_catalogo(d) == clave)
        };
        if !limpio.is_empty() && !clave.is_empty() && clave != "otra" && !es_default {
            let entry = mapa.entry(clave).or_insert_with(|| EntradaUsoCatalogo {
                nombre_display: limpio.clone(),
                conteo: 0,
                origen_historico: "lista_usuario".to_string(),
                primera_vez: ahora.clone(),
                ultima_vez: ahora.clone(),
            });
            if entry.origen_historico != "predeterminado" {
                entry.origen_historico = "lista_usuario".to_string();
                entry.nombre_display = limpio;
            }
            hubo_cambio = true;
        }
    }

    if let Some(nombre) = eliminado {
        let clave = normalizar_clave_catalogo(nombre);
        if !clave.is_empty() {
            if let Some(entry) = mapa.get(&clave) {
                hubo_cambio = true;
                if entry.conteo == 0 && entry.origen_historico != "predeterminado" {
                    mapa.remove(&clave);
                }
            }
        }
    }

    if !hay_datos_uso_exportables(&uso, settings) {
        uso.pendientes_compartir = 0;
        if hubo_cambio {
            uso.ultima_actualizacion = ahora;
        }
    } else if hubo_cambio {
        uso.pendientes_compartir = uso.pendientes_compartir.saturating_add(1);
        uso.ultima_actualizacion = ahora;
    }
    guardar_uso_sin_lock_en(ruta_uso, &uso)?;
    Ok(uso)
}

pub fn registrar_cambio_lista_usuario_actual(
    settings: &AppSettings,
    es_juzgado: bool,
    agregado: Option<&str>,
    eliminado: Option<&str>,
) {
    let _ = registrar_cambio_lista_usuario_en(
        &ruta_uso_catalogos(),
        &obtener_carpeta_archivos(),
        settings,
        es_juzgado,
        agregado,
        eliminado,
    );
}

pub fn confirmar_uso_exportado_en(
    ruta_uso: &Path,
    instancia_id_exportada: &str,
) -> Result<(), String> {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let falta_archivo = !ruta_uso.exists();
    let mut uso = cargar_uso_sin_lock_en(ruta_uso);
    let id_limpio = instancia_id_exportada.trim();
    let necesita_persistir_id = !id_limpio.is_empty() && (falta_archivo || uso.instancia_id != id_limpio);
    if necesita_persistir_id {
        uso.instancia_id = id_limpio.to_string();
    }
    let habia_pendientes = uso.pendientes_compartir > 0;
    uso.pendientes_compartir = 0;
    if habia_pendientes || necesita_persistir_id {
        guardar_uso_sin_lock_en(ruta_uso, &uso)?;
    }
    Ok(())
}

pub fn marcar_uso_compartido_en(ruta_uso: &Path) -> Result<(), String> {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let mut uso = cargar_uso_sin_lock_en(ruta_uso);
    if uso.pendientes_compartir > 0 {
        uso.pendientes_compartir = 0;
        guardar_uso_sin_lock_en(ruta_uso, &uso)?;
    }
    Ok(())
}

pub fn marcar_uso_compartido_actual() -> Result<(), String> {
    marcar_uso_compartido_en(&ruta_uso_catalogos())
}

fn sumar_entrada_catalogo(
    dest: &mut BTreeMap<String, EntradaUsoCatalogo>,
    clave: String,
    src: &EntradaUsoCatalogo,
) {
    let entry = dest.entry(clave).or_insert_with(|| EntradaUsoCatalogo {
        nombre_display: src.nombre_display.clone(),
        conteo: 0,
        origen_historico: src.origen_historico.clone(),
        primera_vez: src.primera_vez.clone(),
        ultima_vez: src.ultima_vez.clone(),
    });
    entry.conteo = entry.conteo.saturating_add(src.conteo);
    if entry.origen_historico != "predeterminado" && src.origen_historico == "predeterminado" {
        entry.origen_historico = "predeterminado".to_string();
        entry.nombre_display = src.nombre_display.clone();
    } else if entry.origen_historico == "otra" && src.origen_historico == "lista_usuario" {
        entry.origen_historico = "lista_usuario".to_string();
        entry.nombre_display = src.nombre_display.clone();
    }
    if entry.primera_vez.is_empty()
        || (!src.primera_vez.is_empty() && src.primera_vez < entry.primera_vez)
    {
        entry.primera_vez = src.primera_vez.clone();
    }
    if src.ultima_vez > entry.ultima_vez {
        entry.ultima_vez = src.ultima_vez.clone();
    }
}

fn max_entrada_catalogo(
    dest: &mut BTreeMap<String, EntradaUsoCatalogo>,
    clave: String,
    src: &EntradaUsoCatalogo,
) {
    let entry = dest.entry(clave).or_insert_with(|| src.clone());
    if src.conteo > entry.conteo {
        entry.conteo = src.conteo;
    }
    if entry.origen_historico != "predeterminado" && src.origen_historico == "predeterminado" {
        entry.origen_historico = "predeterminado".to_string();
        entry.nombre_display = src.nombre_display.clone();
    } else if entry.origen_historico == "otra" && src.origen_historico == "lista_usuario" {
        entry.origen_historico = "lista_usuario".to_string();
        entry.nombre_display = src.nombre_display.clone();
    }
}

/// Fusiona o restaura los contadores de uso incluidos en una copia portable `.lokus`.
/// Si el paquete proviene de una versión anterior sin `uso_catalogos`, escanea únicamente
/// las planillas nuevas importadas para no dejar al usuario en cero.
pub fn fusionar_uso_importado_en(
    ruta_uso: &Path,
    carpeta_archivos: &Path,
    settings: &AppSettings,
    uso_paquete_opt: Option<&UsoCatalogos>,
    planillas_nuevas_rel: &[String],
    modo_conflicto: &str,
) -> Result<UsoCatalogos, String> {
    let _guard = BLOQUEO_USO.lock().unwrap_or_else(|e| e.into_inner());
    let mut local = cargar_uso_sin_lock_en(ruta_uso);
    let ahora = Local::now().to_rfc3339();
    inicializar_uso_si_falta(&mut local, carpeta_archivos, settings, &ahora);

    if let Some(paq) = uso_paquete_opt {
        let es_misma_fuente = !paq.instancia_id.trim().is_empty()
            && (paq.instancia_id == local.instancia_id
                || local.fuentes_importadas.contains(&paq.instancia_id));
        let firma_paquete = format!(
            "{}|{}|{}",
            paq.instancia_id.trim(),
            paq.ultima_actualizacion.trim(),
            paq.total_audiencias_contabilizadas
        );
        let ya_importado_paquete = !paq.instancia_id.trim().is_empty()
            && local.paquetes_importados.contains(&firma_paquete);

        if local.total_audiencias_contabilizadas == 0
            && local.tipos_audiencia.is_empty()
            && local.juzgados.is_empty()
        {
            let id_local = local.instancia_id.clone();
            local = paq.clone();
            if !paq.instancia_id.is_empty() {
                local.fuentes_importadas.insert(paq.instancia_id.clone());
                local.paquetes_importados.insert(firma_paquete);
            }
            if !id_local.is_empty() {
                local.instancia_id = id_local;
            }
            local.sembrado_inicial = true;
            if local.sembrado_en.is_empty() {
                local.sembrado_en = ahora.clone();
            }
        } else if es_misma_fuente {
            if modo_conflicto == "reemplazar" {
                local.total_audiencias_contabilizadas = local
                    .total_audiencias_contabilizadas
                    .max(paq.total_audiencias_contabilizadas);
                local.tipos_sin_especificar =
                    local.tipos_sin_especificar.max(paq.tipos_sin_especificar);
                local.juzgados_sin_especificar = local
                    .juzgados_sin_especificar
                    .max(paq.juzgados_sin_especificar);
                local.horas_sin_especificar =
                    local.horas_sin_especificar.max(paq.horas_sin_especificar);
                for (k, v) in &paq.tipos_audiencia {
                    max_entrada_catalogo(&mut local.tipos_audiencia, k.clone(), v);
                }
                for (k, v) in &paq.juzgados {
                    max_entrada_catalogo(&mut local.juzgados, k.clone(), v);
                }
                for (k, &v) in &paq.horas_exactas {
                    let e = local.horas_exactas.entry(k.clone()).or_insert(0);
                    *e = (*e).max(v);
                }
                local.paquetes_importados.insert(firma_paquete);
            } else if (!ya_importado_paquete || modo_conflicto == "conservar_ambas")
                && !planillas_nuevas_rel.is_empty()
            {
                let mut agregadas = 0u32;
                for rel in planillas_nuevas_rel {
                    let abs = carpeta_archivos.join(rel);
                    if let Ok((registros, _, _)) = leer_archivo_en(&abs) {
                        for r in &registros {
                            acumular_valores_en_uso(
                                &mut local,
                                &r.tipo,
                                &r.hora,
                                &r.juzgado,
                                settings,
                                &ahora,
                            );
                            agregadas = agregadas.saturating_add(1);
                        }
                    }
                }
                if agregadas > 0 {
                    local.pendientes_compartir = local.pendientes_compartir.saturating_add(agregadas);
                }
                local.paquetes_importados.insert(firma_paquete);
            }
        } else {
            local.total_audiencias_contabilizadas = local
                .total_audiencias_contabilizadas
                .saturating_add(paq.total_audiencias_contabilizadas);
            local.tipos_sin_especificar = local
                .tipos_sin_especificar
                .saturating_add(paq.tipos_sin_especificar);
            local.juzgados_sin_especificar = local
                .juzgados_sin_especificar
                .saturating_add(paq.juzgados_sin_especificar);
            local.horas_sin_especificar = local
                .horas_sin_especificar
                .saturating_add(paq.horas_sin_especificar);
            local.pendientes_compartir = local
                .pendientes_compartir
                .saturating_add(paq.pendientes_compartir);
            for (k, v) in &paq.tipos_audiencia {
                sumar_entrada_catalogo(&mut local.tipos_audiencia, k.clone(), v);
            }
            for (k, v) in &paq.juzgados {
                sumar_entrada_catalogo(&mut local.juzgados, k.clone(), v);
            }
            for (k, &v) in &paq.horas_exactas {
                *local.horas_exactas.entry(k.clone()).or_insert(0) += v;
            }
            if !paq.instancia_id.trim().is_empty() {
                local.fuentes_importadas.insert(paq.instancia_id.clone());
                local.paquetes_importados.insert(firma_paquete);
            }
            for f in &paq.fuentes_importadas {
                let f_trim = f.trim();
                if !f_trim.is_empty() && f_trim != local.instancia_id {
                    local.fuentes_importadas.insert(f_trim.to_string());
                }
            }
        }
        sincronizar_listas_usuario_en_uso(&mut local, settings, &ahora);
        local.ultima_actualizacion = ahora;
    } else if !planillas_nuevas_rel.is_empty() {
        let mut agregadas = 0u32;
        for rel in planillas_nuevas_rel {
            let abs = carpeta_archivos.join(rel);
            if let Ok((registros, _, _)) = leer_archivo_en(&abs) {
                for r in &registros {
                    acumular_valores_en_uso(&mut local, &r.tipo, &r.hora, &r.juzgado, settings, &ahora);
                    agregadas = agregadas.saturating_add(1);
                }
            }
        }
        if agregadas > 0 {
            local.pendientes_compartir = local.pendientes_compartir.saturating_add(agregadas);
        }
        sincronizar_listas_usuario_en_uso(&mut local, settings, &ahora);
    }

    guardar_uso_sin_lock_en(ruta_uso, &local)?;
    Ok(local)
}

fn construir_reporte_campo(
    total_audiencias: u32,
    sin_especificar: u32,
    predeterminados: &[String],
    lista_usuario: &[String],
    almacenados: &BTreeMap<String, EntradaUsoCatalogo>,
    anonimizar_no_predeterminado: &dyn Fn(&str) -> String,
) -> ReporteUsoCampoCatalogo {
    let mut por_clave: BTreeMap<String, ItemUsoCatalogo> = BTreeMap::new();

    // 1. Todas las opciones predeterminadas oficiales (sin anonimizar ni alterar nunca, con 0 si no se han usado)
    for oficial in predeterminados {
        let clave = normalizar_clave_catalogo(oficial);
        if clave.is_empty() {
            continue;
        }
        let conteo = almacenados.get(&clave).map(|e| e.conteo).unwrap_or(0);
        por_clave.insert(
            clave,
            ItemUsoCatalogo {
                nombre: oficial.clone(),
                origen: "predeterminado".to_string(),
                conteo,
                porcentaje: calcular_porcentaje(conteo, total_audiencias),
            },
        );
    }

    // 2. Todas las opciones agregadas por el usuario en settings (anonimizadas, con 0 si no se han usado)
    for usr in lista_usuario {
        let clave = normalizar_clave_catalogo(usr);
        if clave.is_empty() || clave == "otra" || por_clave.contains_key(&clave) {
            continue;
        }
        let conteo = almacenados.get(&clave).map(|e| e.conteo).unwrap_or(0);
        por_clave.insert(
            clave,
            ItemUsoCatalogo {
                nombre: anonimizar_no_predeterminado(usr),
                origen: "agregado_usuario".to_string(),
                conteo,
                porcentaje: calcular_porcentaje(conteo, total_audiencias),
            },
        );
    }

    // 3. Valores escritos libremente vía "Otra" o eliminados de lista tras haberse usado (conteo > 0)
    for (clave, entry) in almacenados {
        if por_clave.contains_key(clave) || entry.conteo == 0 {
            continue;
        }
        let origen = if entry.origen_historico == "lista_usuario" {
            "eliminado_de_lista"
        } else {
            "otra"
        };
        por_clave.insert(
            clave.clone(),
            ItemUsoCatalogo {
                nombre: anonimizar_no_predeterminado(&entry.nombre_display),
                origen: origen.to_string(),
                conteo: entry.conteo,
                porcentaje: calcular_porcentaje(entry.conteo, total_audiencias),
            },
        );
    }

    let mut detalle: Vec<ItemUsoCatalogo> = por_clave.into_values().collect();
    detalle.sort_by(|a, b| {
        b.conteo
            .cmp(&a.conteo)
            .then_with(|| a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase()))
    });

    let mut predeterminados_sin_uso: Vec<String> = detalle
        .iter()
        .filter(|i| i.origen == "predeterminado" && i.conteo == 0)
        .map(|i| i.nombre.clone())
        .collect();
    predeterminados_sin_uso.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    let mut agregados_usuario_sin_uso: Vec<String> = detalle
        .iter()
        .filter(|i| i.origen == "agregado_usuario" && i.conteo == 0)
        .map(|i| i.nombre.clone())
        .collect();
    agregados_usuario_sin_uso.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    let nuevos_no_predeterminados: Vec<ItemUsoCatalogo> = detalle
        .iter()
        .filter(|i| i.origen != "predeterminado")
        .cloned()
        .collect();

    let total_opciones_predeterminadas = detalle
        .iter()
        .filter(|i| i.origen == "predeterminado")
        .count();
    let total_opciones_lista_usuario = detalle
        .iter()
        .filter(|i| i.origen == "agregado_usuario")
        .count();
    let total_opciones_otra = detalle.iter().filter(|i| i.origen == "otra").count();
    let total_opciones_eliminadas_tras_uso = detalle
        .iter()
        .filter(|i| i.origen == "eliminado_de_lista")
        .count();

    ReporteUsoCampoCatalogo {
        total_audiencias,
        total_con_valor: total_audiencias.saturating_sub(sin_especificar),
        total_sin_especificar: sin_especificar,
        porcentaje_sin_especificar: calcular_porcentaje(sin_especificar, total_audiencias),
        total_opciones_predeterminadas,
        total_opciones_lista_usuario,
        total_opciones_otra,
        total_opciones_eliminadas_tras_uso,
        nuevos_no_predeterminados,
        predeterminados_sin_uso,
        agregados_usuario_sin_uso,
        detalle,
    }
}

fn construir_reporte_horas(uso: &UsoCatalogos) -> ReporteUsoHoras {
    let total = uso.total_audiencias_contabilizadas;
    let total_sin_hora = uso.horas_sin_especificar;
    let total_con_hora = total.saturating_sub(total_sin_hora);

    let horas_rapidas_predeterminadas: Vec<ItemUsoHora> = HORAS_RAPIDAS
        .iter()
        .map(|(h24, label)| {
            let conteo = uso.horas_exactas.get(*h24).copied().unwrap_or(0);
            ItemUsoHora {
                hora_24: (*h24).to_string(),
                etiqueta_12h: (*label).to_string(),
                es_hora_rapida: true,
                conteo,
                porcentaje: calcular_porcentaje(conteo, total),
            }
        })
        .collect();

    let horas_rapidas_sin_uso: Vec<String> = horas_rapidas_predeterminadas
        .iter()
        .filter(|h| h.conteo == 0)
        .map(|h| h.hora_24.clone())
        .collect();

    let mut mapa_exactas: BTreeMap<String, ItemUsoHora> = BTreeMap::new();
    for item in &horas_rapidas_predeterminadas {
        mapa_exactas.insert(item.hora_24.clone(), item.clone());
    }
    for (hh_mm, &conteo) in &uso.horas_exactas {
        if conteo == 0 && !es_hora_rapida(hh_mm) {
            continue;
        }
        mapa_exactas.entry(hh_mm.clone()).or_insert_with(|| ItemUsoHora {
            hora_24: hh_mm.clone(),
            etiqueta_12h: formatear_hora_12h(hh_mm),
            es_hora_rapida: es_hora_rapida(hh_mm),
            conteo,
            porcentaje: calcular_porcentaje(conteo, total),
        });
    }

    let mut detalle_por_hora_exacta: Vec<ItemUsoHora> = mapa_exactas.into_values().collect();
    detalle_por_hora_exacta.sort_by(|a, b| {
        b.conteo
            .cmp(&a.conteo)
            .then_with(|| a.hora_24.cmp(&b.hora_24))
    });

    let horas_mas_frecuentes_no_rapidas: Vec<ItemUsoHora> = detalle_por_hora_exacta
        .iter()
        .filter(|h| !h.es_hora_rapida && h.conteo > 0)
        .cloned()
        .collect();

    let mut franjas_conteo: BTreeMap<&str, u32> = BTreeMap::new();
    for (hh_mm, &conteo) in &uso.horas_exactas {
        *franjas_conteo.entry(franja_de_hora(hh_mm)).or_insert(0) += conteo;
    }
    franjas_conteo.insert("Sin hora", total_sin_hora);

    let resumen_por_franja_horaria: Vec<ItemUsoFranja> = FRANJAS_ORDEN
        .iter()
        .map(|&franja| {
            let conteo = franjas_conteo.get(franja).copied().unwrap_or(0);
            ItemUsoFranja {
                franja: franja.to_string(),
                conteo,
                porcentaje: calcular_porcentaje(conteo, total),
            }
        })
        .collect();

    ReporteUsoHoras {
        total_audiencias: total,
        total_con_hora,
        total_sin_hora,
        porcentaje_sin_hora: calcular_porcentaje(total_sin_hora, total),
        horas_rapidas_predeterminadas,
        horas_rapidas_sin_uso,
        horas_mas_frecuentes_no_rapidas,
        detalle_por_hora_exacta,
        resumen_por_franja_horaria,
    }
}

pub fn construir_reporte_uso_catalogos(
    uso: &UsoCatalogos,
    juzgados_usuario: &[String],
    tipos_usuario: &[String],
    anonimizar_no_predeterminado: &dyn Fn(&str) -> String,
) -> ReporteUsoCatalogos {
    let tipos_default: Vec<String> = TIPOS_AUDIENCIA
        .iter()
        .filter(|&&t| t != "Otra")
        .map(|&t| t.to_string())
        .collect();

    let reporte_tipos = construir_reporte_campo(
        uso.total_audiencias_contabilizadas,
        uso.tipos_sin_especificar,
        &tipos_default,
        tipos_usuario,
        &uso.tipos_audiencia,
        anonimizar_no_predeterminado,
    );

    let reporte_juzgados = construir_reporte_campo(
        uso.total_audiencias_contabilizadas,
        uso.juzgados_sin_especificar,
        juzgados_base(),
        juzgados_usuario,
        &uso.juzgados,
        anonimizar_no_predeterminado,
    );

    let reporte_horas = construir_reporte_horas(uso);

    ReporteUsoCatalogos {
        total_audiencias_contabilizadas: uso.total_audiencias_contabilizadas,
        sembrado_inicial_completado: uso.sembrado_inicial,
        sembrado_en: uso.sembrado_en.clone(),
        ultima_actualizacion: uso.ultima_actualizacion.clone(),
        tipos_audiencia: reporte_tipos,
        juzgados: reporte_juzgados,
        horas: reporte_horas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excel::{eliminar_archivo_en, eliminar_carpeta_con_opciones_en};
    use umya_spreadsheet::{reader, writer};

    fn preparar_dir(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lokus_uso_test_{}_{}",
            nombre,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn crear_excel_con_filas(ruta: &Path, filas: &[(&str, &str, &str, &str)]) {
        if let Some(p) = ruta.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let plantilla = include_bytes!("../../templates/plantilla_audiencias.xlsx");
        fs::write(ruta, plantilla).unwrap();
        let mut book = reader::xlsx::read(ruta).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();
        for (idx, (rad, tipo, hora, juzgado)) in filas.iter().enumerate() {
            let row = 11 + idx as u32;
            ws.get_cell_mut((1, row)).set_value_number((idx + 1) as i32);
            ws.get_cell_mut((2, row)).set_value_string(*rad);
            if !tipo.is_empty() {
                ws.get_cell_mut((3, row)).set_value_string(*tipo);
            }
            ws.get_cell_mut((4, row)).set_value_string("15/08/2025");
            if !hora.is_empty() {
                ws.get_cell_mut((5, row)).set_value_string(*hora);
            }
            if !juzgado.is_empty() {
                ws.get_cell_mut((6, row)).set_value_string(*juzgado);
            }
            ws.get_cell_mut((7, row)).set_value_string("SI");
        }
        writer::xlsx::write(&book, ruta).unwrap();
    }

    #[test]
    fn test_siembra_unica_inicial_y_conservacion_tras_borrar_archivos() {
        let dir = preparar_dir("siembra_y_borrado");
        let carpeta_excel = dir.join("archivos");
        let ruta_uso = dir.join("config").join(NOMBRE_ARCHIVO_USO);
        fs::create_dir_all(&carpeta_excel).unwrap();

        crear_excel_con_filas(
            &carpeta_excel.join("2026/enero.xlsx"),
            &[
                (
                    "JUAN PEREZ - 200016001075202500001",
                    "Audiencia de acusacion", // sin tilde
                    "09:00",
                    "Juzgado 2 Penal",
                ),
                (
                    "MARIA LOPEZ - 200016001075202500002",
                    "Audiencia concentrada",
                    "",
                    "Juzgado 2 Penal con Funciones de Conocimiento",
                ),
                (
                    "CARLOS RUIZ - 200016001075202500003",
                    "",
                    "14:30",
                    "Juzgado Segundo Penal",
                ),
            ],
        );

        let mut settings = AppSettings::default();
        settings.juzgados = vec!["Juzgado Propio 0 usos".into()];
        settings.tipos_audiencia = vec!["Audiencia especial 0 usos".into()];

        // Primer escaneo único siembra las 3 audiencias
        let uso1 = asegurar_uso_inicializado_en(&ruta_uso, &carpeta_excel, &settings);
        assert!(uso1.sembrado_inicial);
        assert_eq!(uso1.total_audiencias_contabilizadas, 3);
        assert_eq!(uso1.tipos_sin_especificar, 1);
        assert_eq!(uso1.horas_sin_especificar, 1);
        assert_eq!(uso1.pendientes_compartir, 5); // 3 audiencias + 2 listas

        // Eliminar el archivo Excel y la carpeta completa NO borra el histórico acumulado
        eliminar_archivo_en(&carpeta_excel, "2026/enero.xlsx", false).unwrap();
        eliminar_carpeta_con_opciones_en(&carpeta_excel, "2026", true, false).unwrap();

        let uso2 = asegurar_uso_inicializado_en(&ruta_uso, &carpeta_excel, &settings);
        assert_eq!(uso2.total_audiencias_contabilizadas, 3);

        // Construir reporte verificando:
        // - Todos los 17 tipos oficiales + 97 juzgados base + 6 horas rápidas presentes con 0 si no usados
        // - Sin tilde unificó "Audiencia de acusacion" -> "Audiencia de acusación" (conteo 1)
        // - "Juzgado 2 Penal", "Juzgado 2 Penal con Funciones de Conocimiento" y "Juzgado Segundo Penal" siguen SEPARADOS!
        let rep = construir_reporte_uso_catalogos(
            &uso2,
            &settings.juzgados,
            &settings.tipos_audiencia,
            &|s| s.to_string(),
        );
        assert_eq!(
            rep.tipos_audiencia.total_opciones_predeterminadas,
            TIPOS_AUDIENCIA.len() - 1
        );
        assert_eq!(rep.juzgados.total_opciones_predeterminadas, juzgados_base().len());
        assert_eq!(rep.horas.horas_rapidas_predeterminadas.len(), 6);

        let acus = rep
            .tipos_audiencia
            .detalle
            .iter()
            .find(|i| i.nombre == "Audiencia de acusación")
            .unwrap();
        assert_eq!(acus.conteo, 1);
        assert_eq!(acus.origen, "predeterminado");

        let esp_0 = rep
            .tipos_audiencia
            .detalle
            .iter()
            .find(|i| i.nombre == "Audiencia especial 0 usos")
            .unwrap();
        assert_eq!(esp_0.conteo, 0);
        assert_eq!(esp_0.origen, "agregado_usuario");

        let juz_penal = rep
            .juzgados
            .detalle
            .iter()
            .find(|i| i.nombre == "Juzgado 2 Penal")
            .unwrap();
        let juz_conoc = rep
            .juzgados
            .detalle
            .iter()
            .find(|i| i.nombre == "Juzgado 2 Penal con Funciones de Conocimiento")
            .unwrap();
        let juz_seg = rep
            .juzgados
            .detalle
            .iter()
            .find(|i| i.nombre == "Juzgado Segundo Penal")
            .unwrap();
        assert_eq!(juz_penal.conteo, 1);
        assert_eq!(juz_conoc.conteo, 1);
        assert_eq!(juz_seg.conteo, 1);
        assert_eq!(juz_penal.origen, "otra");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_edicion_eliminacion_fila_y_eliminado_de_lista_tras_uso() {
        let dir = preparar_dir("edicion_eliminacion");
        let carpeta_excel = dir.join("archivos");
        let ruta_uso = dir.join("config").join(NOMBRE_ARCHIVO_USO);
        fs::create_dir_all(&carpeta_excel).unwrap();

        let mut settings = AppSettings::default();
        settings.juzgados = vec!["Juzgado Custom Alpha".into()];

        let a1 = AudienciaInput {
            radicado: "RAD-1".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "10/08/2025".into(),
            hora: "08:00".into(),
            juzgado: "Juzgado Custom Alpha".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };

        let u1 = registrar_delta_audiencia_en(&ruta_uso, &carpeta_excel, &settings, None, Some(&a1))
            .unwrap();
        assert_eq!(u1.total_audiencias_contabilizadas, 1);

        // Usuario quita "Juzgado Custom Alpha" de su lista después de haberlo usado -> debe aparecer como "eliminado_de_lista"
        settings.juzgados.clear();
        registrar_cambio_lista_usuario_en(
            &ruta_uso,
            &carpeta_excel,
            &settings,
            true,
            None,
            Some("Juzgado Custom Alpha"),
        )
        .unwrap();

        let uso_post_del_lista = cargar_uso_sin_lock_en(&ruta_uso);
        let rep = construir_reporte_uso_catalogos(
            &uso_post_del_lista,
            &settings.juzgados,
            &settings.tipos_audiencia,
            &|s| s.to_string(),
        );
        let custom_item = rep
            .juzgados
            .detalle
            .iter()
            .find(|i| i.nombre == "Juzgado Custom Alpha")
            .unwrap();
        assert_eq!(custom_item.conteo, 1);
        assert_eq!(custom_item.origen, "eliminado_de_lista");

        // Editar fila: cambia a 15:00, Audiencia de nulidad, sin juzgado
        let mut a2 = a1.clone();
        a2.tipo = "Audiencia de nulidad".into();
        a2.hora = "15:00".into();
        a2.juzgado = String::new();
        let u2 = registrar_delta_audiencia_en(
            &ruta_uso,
            &carpeta_excel,
            &settings,
            Some(&a1),
            Some(&a2),
        )
        .unwrap();
        assert_eq!(u2.total_audiencias_contabilizadas, 1);
        assert_eq!(u2.juzgados_sin_especificar, 1);
        assert_eq!(u2.horas_exactas.get("08:00").copied(), None);
        assert_eq!(u2.horas_exactas.get("15:00").copied(), Some(1));

        // Eliminar fila individual: vuelve a 0 sin bajar de 0 y resetea pendientes al quedar sin datos
        let u3 = registrar_delta_audiencia_en(&ruta_uso, &carpeta_excel, &settings, Some(&a2), None)
            .unwrap();
        assert_eq!(u3.total_audiencias_contabilizadas, 0);
        assert_eq!(u3.juzgados_sin_especificar, 0);
        assert!(u3.horas_exactas.is_empty());
        assert_eq!(u3.pendientes_compartir, 0);
        assert!(!u3.juzgados.contains_key("juzgado custom alpha"));

        // Restar cuando ya está en 0 nunca desborda ni baja de 0
        let u4 = registrar_delta_audiencia_en(&ruta_uso, &carpeta_excel, &settings, Some(&a2), None)
            .unwrap();
        assert_eq!(u4.total_audiencias_contabilizadas, 0);
        assert_eq!(u4.pendientes_compartir, 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
