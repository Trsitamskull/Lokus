use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use umya_spreadsheet::{
    reader, writer, OrientationValues, PageSetup, SheetViewValues, Spreadsheet, Worksheet,
};

use crate::models::{
    buscar_posible_duplicado, extraer_numeros_radicado, extraer_prefijo_dane_radicado,
    extraer_siglas_cortas_juzgado, mapear_motivo_a_indice, normalizar_clave_catalogo,
    normalizar_clave_defensor, AudienciaInput, EstadisticasGlobales, IndicePriorsExcel,
    MesDisponible, PlanillaResumen, ProcesoHabitual, RegistroAudiencia, ResumenDefensor,
    ResumenMesPlanilla, ResumenPlanillaArchivada, TotalesExcel, MOTIVOS_LABELS,
};

const PLANTILLA_BYTES: &[u8] = include_bytes!("../../templates/plantilla_audiencias.xlsx");
pub const NOMBRE_INDICE_ESTADISTICAS: &str = ".lokus_estadisticas.json";
pub const NOMBRE_INDICE_PRIORS: &str = ".lokus_priors.json";
const FORMATO_PRIORS_ACTUAL: u32 = 3;

pub const FILA_DEFENSOR: u32 = 8;
pub const COL_DEFENSOR: u32 = 2; // B8
pub const FILA_INICIO_DATOS: u32 = 11;
pub const FILA_MAXIMA_DATOS: u32 = 110;
pub const FILA_TOTALES: u32 = 111;
pub const MAX_RESPALDOS: usize = 3;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespaldoInfo {
    pub archivo: String,
    pub fecha_legible: String,
    pub tamano_bytes: u64,
}

pub fn obtener_carpeta_archivos() -> PathBuf {
    if let Some(dir) = crate::rutas::ruta_archivos_prioritaria() {
        return dir;
    }

    #[cfg(not(target_os = "android"))]
    if cfg!(debug_assertions) {
        if let Ok(cwd) = std::env::current_dir() {
            // Durante `cargo tauri dev` o `cargo test` dentro de src-tauri/, usar ../archivos_creados
            if cwd.file_name().and_then(|n| n.to_str()) == Some("src-tauri") {
                if let Some(parent) = cwd.parent() {
                    let parent_archivos = parent.join("archivos_creados");
                    if parent_archivos.exists() || fs::create_dir_all(&parent_archivos).is_ok() {
                        return parent_archivos;
                    }
                }
            }
            let local_archivos = cwd.join("archivos_creados");
            if local_archivos.exists() || fs::create_dir_all(&local_archivos).is_ok() {
                return local_archivos;
            }
        }
    }

    let docs = dirs::document_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::home_dir)
        .map(|base| base.join("Lokus").join("archivos_creados"))
        .or_else(crate::rutas::ruta_archivos_respaldo)
        .unwrap_or_else(|| PathBuf::from(".").join("Lokus").join("archivos_creados"));
    let _ = fs::create_dir_all(&docs);
    docs
}

/// Limpia nombres de archivo nuevos para que sean seguros en Windows y Linux.
pub fn sanitizar_nombre_archivo(nombre: &str) -> Result<String, String> {
    let reemplazado: String = nombre
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
            {
                '-'
            } else {
                c
            }
        })
        .collect();

    let mut limpio = reemplazado.trim().trim_end_matches('.').trim().to_string();
    if limpio.to_lowercase().ends_with(".xlsx") {
        let len_sin_ext = limpio.len() - 5;
        limpio = limpio[..len_sin_ext]
            .trim()
            .trim_end_matches('.')
            .trim()
            .to_string();
    }

    if limpio.is_empty() {
        return Err("El nombre del archivo no es válido o está vacío.".to_string());
    }

    let upper = limpio.to_uppercase();
    let es_reservado_windows = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if es_reservado_windows {
        return Err("Ese nombre está reservado por el sistema operativo.".to_string());
    }

    Ok(format!("{}.xlsx", limpio))
}

/// Valida una ruta relativa dentro de `archivos_creados` (separador `/`, sin `..`, sin rutas absolutas)
/// y devuelve sus segmentos ya limpios.
pub(crate) fn segmentos_ruta_relativa(ruta: &str) -> Result<Vec<String>, String> {
    let trimmed = ruta.trim();
    if trimmed.is_empty() || trimmed.contains('\\') || trimmed.starts_with('/') {
        return Err("Ruta inválida.".to_string());
    }
    let mut segmentos = Vec::new();
    for seg in trimmed.split('/') {
        let seg = seg.trim();
        if seg.is_empty() || seg == "." || seg == ".." || seg.contains("..") || seg.starts_with('.')
        {
            return Err("Ruta inválida.".to_string());
        }
        segmentos.push(seg.to_string());
    }
    Ok(segmentos)
}

pub fn ruta_archivo(nombre_archivo: &str) -> Result<PathBuf, String> {
    let segmentos = segmentos_ruta_relativa(nombre_archivo)
        .map_err(|_| "Nombre de archivo inválido.".to_string())?;
    let mut ruta = obtener_carpeta_archivos();
    let ultimo = segmentos.len() - 1;
    for (i, seg) in segmentos.iter().enumerate() {
        if i == ultimo && !seg.to_lowercase().ends_with(".xlsx") {
            ruta.push(format!("{}.xlsx", seg));
        } else {
            ruta.push(seg);
        }
    }
    Ok(ruta)
}

/// Ruta de una carpeta (o subcarpeta) de planillas dentro de `archivos_creados`.
pub fn ruta_carpeta(carpeta_relativa: &str) -> Result<PathBuf, String> {
    let segmentos = segmentos_ruta_relativa(carpeta_relativa)
        .map_err(|_| "Nombre de carpeta inválido.".to_string())?;
    let mut ruta = obtener_carpeta_archivos();
    for seg in segmentos {
        ruta.push(seg);
    }
    Ok(ruta)
}

/// Limpia cada tramo de una ruta de carpeta escrita por el usuario (`2026/Enero`).
pub fn sanitizar_ruta_carpeta(carpeta: &str) -> Result<String, String> {
    let mut limpios = Vec::new();
    for seg in carpeta.split(['/', '\\']) {
        if seg.trim().is_empty() {
            continue;
        }
        let limpio = sanitizar_nombre_archivo(seg)?;
        let nombre = limpio.trim_end_matches(".xlsx").trim_start_matches('.').to_string();
        if nombre.is_empty() {
            return Err("El nombre de la carpeta no es válido.".to_string());
        }
        limpios.push(nombre);
    }
    if limpios.is_empty() {
        return Err("Ingresa el nombre de la carpeta.".to_string());
    }
    Ok(limpios.join("/"))
}

fn es_oculto(nombre: &str) -> bool {
    nombre.starts_with('.') || nombre.starts_with("~$")
}

fn recorrer_carpeta(
    base: &Path,
    actual: &Path,
    archivos: &mut Vec<String>,
    carpetas: &mut Vec<String>,
) -> Result<(), String> {
    let entradas = fs::read_dir(actual).map_err(|e| format!("Error leyendo carpeta: {}", e))?;
    for entrada in entradas.flatten() {
        let Ok(nombre) = entrada.file_name().into_string() else {
            continue;
        };
        if es_oculto(&nombre) {
            continue;
        }
        let ruta = entrada.path();
        let Ok(rel) = ruta.strip_prefix(base) else {
            continue;
        };
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        if ruta.is_dir() {
            carpetas.push(rel_str);
            recorrer_carpeta(base, &ruta, archivos, carpetas)?;
        } else if nombre.to_lowercase().ends_with(".xlsx") {
            archivos.push(rel_str);
        }
    }
    Ok(())
}

pub(crate) fn explorar_carpeta_en(carpeta: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    fs::create_dir_all(carpeta).map_err(|e| format!("Error creando carpeta: {}", e))?;
    let mut archivos = Vec::new();
    let mut carpetas = Vec::new();
    recorrer_carpeta(carpeta, carpeta, &mut archivos, &mut carpetas)?;
    archivos.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    carpetas.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok((archivos, carpetas))
}

/// Lista recursivamente las planillas (`sub/carpeta/archivo.xlsx`, separador `/`).
pub fn listar_archivos_en(carpeta: &Path) -> Result<Vec<String>, String> {
    Ok(explorar_carpeta_en(carpeta)?.0)
}

pub fn listar_archivos() -> Result<Vec<String>, String> {
    listar_archivos_en(&obtener_carpeta_archivos())
}

/// Lista recursivamente todas las carpetas y subcarpetas (incluidas las vacías).
pub fn listar_carpetas() -> Result<Vec<String>, String> {
    Ok(explorar_carpeta_en(&obtener_carpeta_archivos())?.1)
}

pub fn validar_color_carpeta(color: &str) -> Result<String, String> {
    let c = color.trim().to_lowercase();
    if c.is_empty() || c == "default" || c == "predeterminado" {
        return Ok(String::new());
    }
    match c.as_str() {
        "azul" | "verde" | "ambar" | "coral" | "rosa" | "violeta" | "turquesa" => Ok(c),
        _ => Err(format!(
            "Color de carpeta no válido: '{}'. Usa uno de la paleta permitida.",
            color.trim()
        )),
    }
}

pub fn crear_carpeta_con_color_en(
    base: &Path,
    carpeta_relativa: &str,
    color: Option<&str>,
) -> Result<String, String> {
    let limpia = sanitizar_ruta_carpeta(carpeta_relativa)?;
    let color_limpio = match color {
        Some(c) => validar_color_carpeta(c)?,
        None => String::new(),
    };
    let mut ruta = base.to_path_buf();
    for seg in limpia.split('/') {
        ruta.push(seg);
    }
    if ruta.exists() {
        return Err("Ya existe una carpeta con ese nombre.".to_string());
    }
    fs::create_dir_all(&ruta).map_err(|e| format!("No se pudo crear la carpeta: {}", e))?;
    if !color_limpio.is_empty() {
        guardar_color_carpeta_en(base, &limpia, &color_limpio)?;
    }
    Ok(limpia)
}

pub fn crear_carpeta_con_color(
    carpeta_relativa: &str,
    color: Option<&str>,
) -> Result<String, String> {
    crear_carpeta_con_color_en(&obtener_carpeta_archivos(), carpeta_relativa, color)
}

pub fn crear_carpeta(carpeta_relativa: &str) -> Result<String, String> {
    crear_carpeta_con_color(carpeta_relativa, None)
}

fn carpeta_esta_vacia_de_planillas(ruta: &Path) -> bool {
    let mut archivos = Vec::new();
    let mut carpetas = Vec::new();
    recorrer_carpeta(ruta, ruta, &mut archivos, &mut carpetas).is_ok()
        && archivos.is_empty()
        && carpetas.is_empty()
}

pub fn eliminar_carpeta_con_opciones_en(
    base: &Path,
    carpeta_relativa: &str,
    eliminar_contenido: bool,
    conservar_estadisticas: bool,
) -> Result<(), String> {
    let segmentos = segmentos_ruta_relativa(carpeta_relativa)
        .map_err(|_| "Nombre de carpeta inválido.".to_string())?;
    let mut ruta = base.to_path_buf();
    for seg in &segmentos {
        ruta.push(seg);
    }
    let rel_carpeta = segmentos.join("/");

    if !ruta.is_dir() {
        return Err("La carpeta no existe.".to_string());
    }
    if !eliminar_contenido && !carpeta_esta_vacia_de_planillas(&ruta) {
        return Err(
            "La carpeta no está vacía. Mueve o confirma la eliminación de su contenido."
                .to_string(),
        );
    }

    let archivos_internos = listar_archivos_en(&ruta)?;
    let mut reg = cargar_registro_estadisticas_en(base);
    let mut cambio_indice = false;

    for rel_interno in archivos_internos {
        let rel_completa = format!("{}/{}", rel_carpeta, rel_interno);
        let (id, _) = asegurar_id_ruta(&mut reg, &rel_completa);
        let archivo_abs = ruta.join(&rel_interno);
        if conservar_estadisticas {
            if let Ok((registros, _totales, defensor)) = leer_archivo_en(&archivo_abs) {
                if !registros.is_empty() {
                    let resumen = construir_resumen_desde_lectura(
                        id,
                        rel_completa.clone(),
                        &defensor,
                        &registros,
                    );
                    upsert_resumen_archivado(&mut reg, resumen);
                    cambio_indice = true;
                }
            }
        }
    if reg.ids_por_ruta.remove(&rel_completa).is_some() {
            cambio_indice = true;
        }
        invalidar_cache(&archivo_abs);
    }

    let prefijo_carpeta = format!("{}/", rel_carpeta);
    let len_orden_antes = reg.orden_carpetas.len();
    reg.orden_carpetas
        .retain(|c| c != &rel_carpeta && !c.starts_with(&prefijo_carpeta));
    if reg.orden_carpetas.len() != len_orden_antes {
        cambio_indice = true;
    }
    let len_colores_antes = reg.colores_carpetas.len();
    reg.colores_carpetas
        .retain(|k, _| k != &rel_carpeta && !k.starts_with(&prefijo_carpeta));
    if reg.colores_carpetas.len() != len_colores_antes {
        cambio_indice = true;
    }

    if cambio_indice {
        guardar_registro_estadisticas_en(base, &reg)?;
    }
    if let Ok(mut c) = cache_planillas().lock() {
        c.clear();
    }
    fs::remove_dir_all(&ruta).map_err(|e| format!("No se pudo eliminar la carpeta: {}", e))?;
    invalidar_priors_en(base);
    Ok(())
}

pub fn eliminar_carpeta_con_opciones(
    carpeta_relativa: &str,
    eliminar_contenido: bool,
    conservar_estadisticas: bool,
) -> Result<(), String> {
    if eliminar_contenido {
        let _ = crate::uso_catalogos::asegurar_uso_inicializado_actual();
    }
    eliminar_carpeta_con_opciones_en(
        &obtener_carpeta_archivos(),
        carpeta_relativa,
        eliminar_contenido,
        conservar_estadisticas,
    )
}

pub fn eliminar_carpeta(carpeta_relativa: &str) -> Result<(), String> {
    eliminar_carpeta_con_opciones(carpeta_relativa, false, false)
}

/// Renombra o mueve una carpeta completa (con sus planillas y respaldos) a otra ruta relativa.
pub fn renombrar_carpeta(actual: &str, nueva: &str) -> Result<String, String> {
    let origen = ruta_carpeta(actual)?;
    if !origen.is_dir() {
        return Err("La carpeta no existe.".to_string());
    }
    let nueva_limpia = sanitizar_ruta_carpeta(nueva)?;
    let destino = ruta_carpeta(&nueva_limpia)?;
    if destino.exists() {
        return Err("Ya existe una carpeta con ese nombre.".to_string());
    }
    if destino.starts_with(&origen) {
        return Err("No puedes mover una carpeta dentro de sí misma.".to_string());
    }
    if let Some(parent) = destino.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("No se pudo preparar el destino: {}", e))?;
    }
    fs::rename(&origen, &destino).map_err(|e| {
        format!(
            "No se pudo renombrar la carpeta (verifique que no haya planillas abiertas en Excel): {}",
            e
        )
    })?;
    if let Ok(mut c) = cache_planillas().lock() {
        c.clear();
    }
    let base = obtener_carpeta_archivos();
    let limpia_actual = sanitizar_ruta_carpeta(actual).unwrap_or_else(|_| actual.trim().to_string());
    let _ = actualizar_prefijo_carpeta_indice(&base, &limpia_actual, &nueva_limpia);
    Ok(nueva_limpia)
}

fn mover_respaldos_si_existen(origen: &Path, destino: &Path) {
    let respaldos_origen = carpeta_respaldos_de(origen);
    if !respaldos_origen.is_dir() {
        return;
    }
    let respaldos_destino = carpeta_respaldos_de(destino);
    if respaldos_origen == respaldos_destino {
        return;
    }
    if let Some(parent) = respaldos_destino.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let solo_capitalizacion = respaldos_origen.to_string_lossy().to_lowercase()
        == respaldos_destino.to_string_lossy().to_lowercase();
    if solo_capitalizacion {
        if let Some(parent) = respaldos_destino.parent() {
            let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let tmp_dir = parent.join(format!(".bak_tmp_{}_{}", std::process::id(), seq));
            if fs::rename(&respaldos_origen, &tmp_dir).is_ok() {
                if respaldos_destino.exists() || fs::rename(&tmp_dir, &respaldos_destino).is_err() {
                    let _ = fs::rename(&tmp_dir, &respaldos_origen);
                }
            }
        }
    } else if !respaldos_destino.exists() {
        let _ = fs::rename(&respaldos_origen, &respaldos_destino);
    }
}

fn reubicar_archivo_en(
    base: &Path,
    actual: &str,
    nuevo_nombre: Option<&str>,
    carpeta_destino: Option<&str>,
) -> Result<String, String> {
    let segmentos =
        segmentos_ruta_relativa(actual).map_err(|_| "Nombre de archivo inválido.".to_string())?;
    let mut origen = base.to_path_buf();
    let mut norm_partes = Vec::with_capacity(segmentos.len());
    let ultimo = segmentos.len() - 1;
    for (i, seg) in segmentos.into_iter().enumerate() {
        let final_seg = if i == ultimo && !seg.to_lowercase().ends_with(".xlsx") {
            format!("{}.xlsx", seg)
        } else {
            seg
        };
        origen.push(&final_seg);
        norm_partes.push(final_seg);
    }
    if !origen.is_file() {
        return Err("El archivo no existe.".to_string());
    }

    let rel_orig = norm_partes.join("/");
    let nombre_actual = norm_partes
        .last()
        .cloned()
        .ok_or_else(|| "Nombre de archivo inválido.".to_string())?;
    let carpeta_actual = if norm_partes.len() > 1 {
        norm_partes[..norm_partes.len() - 1].join("/")
    } else {
        String::new()
    };

    let nombre_destino = match nuevo_nombre {
        Some(n) => sanitizar_nombre_archivo(n)?,
        None => nombre_actual,
    };

    let carpeta_limpia = match carpeta_destino {
        None => carpeta_actual,
        Some(c) if c.trim().is_empty() => String::new(),
        Some(c) => sanitizar_ruta_carpeta(c)?,
    };

    let (dir_destino, rel_dest) = if carpeta_limpia.is_empty() {
        (base.to_path_buf(), nombre_destino.clone())
    } else {
        let mut dir = base.to_path_buf();
        for seg in carpeta_limpia.split('/') {
            dir.push(seg);
        }
        (dir, format!("{}/{}", carpeta_limpia, nombre_destino))
    };
    let destino = dir_destino.join(&nombre_destino);

    if rel_orig == rel_dest {
        return Ok(rel_dest);
    }

    let solo_capitalizacion = rel_orig.to_lowercase() == rel_dest.to_lowercase();
    if !solo_capitalizacion && destino.exists() {
        return Err(
            "Ya existe una planilla con ese nombre en la carpeta de destino.".to_string(),
        );
    }

    fs::create_dir_all(&dir_destino)
        .map_err(|e| format!("No se pudo preparar la carpeta de destino: {}", e))?;

    if solo_capitalizacion {
        let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_ruta = dir_destino.join(format!(
            ".rename_{}_{}_{}",
            std::process::id(),
            seq,
            nombre_destino
        ));
        fs::rename(&origen, &tmp_ruta).map_err(|e| {
            format!(
                "No se pudo renombrar el archivo (verifique que no esté abierto en Excel): {}",
                e
            )
        })?;
        if destino.exists() {
            let _ = fs::rename(&tmp_ruta, &origen);
            return Err(
                "Ya existe una planilla con ese nombre en la carpeta de destino.".to_string(),
            );
        }
        if let Err(e) = fs::rename(&tmp_ruta, &destino) {
            let _ = fs::rename(&tmp_ruta, &origen);
            return Err(format!(
                "No se pudo renombrar el archivo (verifique que no esté abierto en Excel): {}",
                e
            ));
        }
    } else {
        fs::rename(&origen, &destino).map_err(|e| {
            format!(
                "No se pudo mover o renombrar el archivo (verifique que no esté abierto en Excel): {}",
                e
            )
        })?;
    }

    invalidar_cache(&origen);
    invalidar_cache(&destino);
    let _ = actualizar_ruta_en_indice(base, &rel_orig, &rel_dest);
    mover_respaldos_si_existen(&origen, &destino);
    Ok(rel_dest)
}

/// Renombra y/o reubica una planilla Excel (y su historial de respaldos e índice interno).
pub fn renombrar_archivo_en(
    base: &Path,
    actual: &str,
    nuevo_nombre: &str,
    carpeta_destino: Option<&str>,
) -> Result<String, String> {
    reubicar_archivo_en(base, actual, Some(nuevo_nombre), carpeta_destino)
}

pub fn renombrar_archivo(
    actual: &str,
    nuevo_nombre: &str,
    carpeta_destino: Option<&str>,
) -> Result<String, String> {
    renombrar_archivo_en(&obtener_carpeta_archivos(), actual, nuevo_nombre, carpeta_destino)
}

/// Mueve una planilla (y su historial de respaldos) a otra carpeta. `carpeta_destino` vacío = raíz.
pub fn mover_archivo(nombre_archivo: &str, carpeta_destino: &str) -> Result<String, String> {
    reubicar_archivo_en(
        &obtener_carpeta_archivos(),
        nombre_archivo,
        None,
        Some(carpeta_destino),
    )
}

fn leer_defensor_de_hoja(ws: &Worksheet) -> String {
    let raw = leer_celda(ws, COL_DEFENSOR, FILA_DEFENSOR);
    let lower = raw.to_lowercase();
    for prefijo in ["defensor público:", "defensor publico:"] {
        if let Some(pos) = lower.find(prefijo) {
            let byte_offset = pos + prefijo.len();
            if raw.is_char_boundary(byte_offset) {
                return raw[byte_offset..].trim().to_string();
            }
        }
    }
    String::new()
}

fn escribir_defensor_en_hoja(ws: &mut Worksheet, defensor: &str) {
    let limpio = defensor.split_whitespace().collect::<Vec<_>>().join(" ");
    let valor = if limpio.is_empty() {
        "DEFENSOR PÚBLICO: ".to_string()
    } else {
        format!("DEFENSOR PÚBLICO: {}", limpio)
    };
    ws.get_cell_mut((COL_DEFENSOR, FILA_DEFENSOR))
        .set_value_string(valor);
}

fn crear_copia_plantilla_en(destino: &Path, defensor: Option<&str>) -> Result<(), String> {
    if destino.exists() {
        return Err("Ya existe un archivo con ese nombre.".to_string());
    }
    if let Some(parent) = destino.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo preparar la carpeta destino: {}", e))?;
    }
    fs::write(destino, PLANTILLA_BYTES)
        .map_err(|e| format!("No se pudo crear el archivo desde la plantilla: {}", e))?;

    let mut book = reader::xlsx::read(destino)
        .map_err(|e| format!("Error al preparar plantilla de Excel: {}", e))?;
    if let Some(def) = defensor {
        let limpio = def.trim();
        if !limpio.is_empty() {
            let ws = book
                .get_sheet_mut(&0)
                .ok_or_else(|| "No se pudo acceder a la hoja principal".to_string())?;
            escribir_defensor_en_hoja(ws, limpio);
        }
    }
    guardar_libro(&mut book, destino)?;

    Ok(())
}

/// Crea una planilla desde la plantilla oficial. `carpeta` (opcional) es una ruta relativa como `2026/Enero`.
/// Devuelve la ruta relativa de la planilla (`2026/Enero/informe.xlsx`).
pub fn crear_copia_plantilla(
    nombre_archivo: &str,
    defensor: Option<&str>,
    carpeta: Option<&str>,
) -> Result<String, String> {
    let nombre_sanitizado = sanitizar_nombre_archivo(nombre_archivo)?;
    let relativa = match carpeta.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => format!("{}/{}", sanitizar_ruta_carpeta(c)?, nombre_sanitizado),
        None => nombre_sanitizado,
    };
    let destino = ruta_archivo(&relativa)?;
    crear_copia_plantilla_en(&destino, defensor)?;
    let base = obtener_carpeta_archivos();
    let mut reg = cargar_registro_estadisticas_en(&base);
    let (_, cambio) = asegurar_id_ruta(&mut reg, &relativa);
    if cambio {
        let _ = guardar_registro_estadisticas_en(&base, &reg);
    }
    Ok(relativa)
}

pub fn eliminar_archivo_en(
    carpeta: &Path,
    nombre_archivo: &str,
    conservar_estadisticas: bool,
) -> Result<(), String> {
    let segmentos = segmentos_ruta_relativa(nombre_archivo)
        .map_err(|_| "Nombre de archivo inválido.".to_string())?;
    let mut ruta = carpeta.to_path_buf();
    let mut norm_partes = Vec::with_capacity(segmentos.len());
    let ultimo = segmentos.len() - 1;
    for (i, seg) in segmentos.into_iter().enumerate() {
        let final_seg = if i == ultimo && !seg.to_lowercase().ends_with(".xlsx") {
            format!("{}.xlsx", seg)
        } else {
            seg
        };
        ruta.push(&final_seg);
        norm_partes.push(final_seg);
    }
    let rel_norm = norm_partes.join("/");

    if !ruta.exists() {
        return Err("El archivo no existe.".to_string());
    }

    let mut reg = cargar_registro_estadisticas_en(carpeta);
    let (id, _) = asegurar_id_ruta(&mut reg, &rel_norm);

    if conservar_estadisticas {
        let (registros, _totales, defensor) = leer_archivo_en(&ruta)?;
        if !registros.is_empty() {
            let resumen =
                construir_resumen_desde_lectura(id.clone(), rel_norm.clone(), &defensor, &registros);
            upsert_resumen_archivado(&mut reg, resumen);
        }
    }

    reg.ids_por_ruta.remove(&rel_norm);
    guardar_registro_estadisticas_en(carpeta, &reg)?;

    fs::remove_file(&ruta).map_err(|e| format!("No se pudo eliminar el archivo: {}", e))?;
    invalidar_cache(&ruta);
    invalidar_priors_en(carpeta);
    Ok(())
}

pub fn eliminar_archivo_con_opcion(
    nombre_archivo: &str,
    conservar_estadisticas: bool,
) -> Result<(), String> {
    let _ = crate::uso_catalogos::asegurar_uso_inicializado_actual();
    eliminar_archivo_en(&obtener_carpeta_archivos(), nombre_archivo, conservar_estadisticas)
}

pub fn eliminar_archivo(nombre_archivo: &str) -> Result<(), String> {
    eliminar_archivo_con_opcion(nombre_archivo, false)
}

/// Carpeta de Descargas del usuario (o carpeta de exportaciones en Android) para exportaciones.
pub fn carpeta_descargas() -> Result<PathBuf, String> {
    if let Some(dir) = crate::rutas::ruta_descargas_prioritaria() {
        return Ok(dir);
    }

    dirs::download_dir()
        .or_else(dirs::home_dir)
        .or_else(crate::rutas::ruta_descargas_respaldo)
        .ok_or_else(|| "No se pudo localizar la carpeta de Descargas".to_string())
}

#[cfg(test)]
fn contiene_printer_settings_zip(ruta: &Path) -> bool {
    let Ok(bytes) = fs::read(ruta) else {
        return false;
    };
    const AGUJA: &[u8] = b"xl/printerSettings/";
    bytes.windows(AGUJA.len()).any(|w| w == AGUJA)
}

/// Reescribe un `.xlsx` existente pasándolo por `guardar_libro` para eliminar configuraciones
/// dependientes de impresora/red (`printerSettings*.bin`, DPI, `paperSize`, `PageLayout`).
pub(crate) fn sanear_archivo_excel_en(ruta: &Path) {
    if !ruta.is_file() {
        return;
    }
    if let Ok(mut book) = reader::xlsx::read(ruta) {
        let _ = guardar_libro(&mut book, ruta);
    }
}

pub(crate) fn descargar_archivo_en(
    origen: &Path,
    carpeta_descargas: &Path,
) -> Result<String, String> {
    if !origen.exists() {
        return Err("Archivo no encontrado.".to_string());
    }
    fs::create_dir_all(carpeta_descargas)
        .map_err(|e| format!("No se pudo preparar la carpeta de Descargas: {}", e))?;

    let nombre_limpio = origen
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audiencias.xlsx");
    let stem = Path::new(nombre_limpio)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audiencias");
    let ext = Path::new(nombre_limpio)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("xlsx");

    let mut destino = carpeta_descargas.join(nombre_limpio);
    let mut contador = 1;
    while destino.exists() {
        destino = carpeta_descargas.join(format!("{}_{}.{}", stem, contador, ext));
        contador += 1;
    }

    // Reempaquetar siempre al exportar para eliminar cualquier metadato de impresora
    // (printerSettings*.bin, DPI, paperSize o vista PageLayout) que bloquee a Excel en Windows.
    if let Ok(mut book) = reader::xlsx::read(origen) {
        if guardar_libro(&mut book, &destino).is_ok() {
            return Ok(destino.to_string_lossy().to_string());
        }
    }

    fs::copy(origen, &destino)
        .map_err(|e| format!("No se pudo copiar el archivo a Descargas: {}", e))?;

    Ok(destino.to_string_lossy().to_string())
}

pub fn descargar_archivo(nombre_archivo: &str) -> Result<String, String> {
    let origen = ruta_archivo(nombre_archivo)?;
    let carpeta_descargas = carpeta_descargas()?;
    descargar_archivo_en(&origen, &carpeta_descargas)
}

fn carpeta_respaldos_de(ruta_excel: &Path) -> PathBuf {
    let parent = ruta_excel.parent().unwrap_or_else(|| Path::new("."));
    let stem = ruta_excel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("planilla");
    parent.join(".respaldos").join(stem)
}

pub(crate) fn crear_respaldo_en(ruta_excel: &Path) -> Result<(), String> {
    if !ruta_excel.exists() {
        return Ok(());
    }
    let dir = carpeta_respaldos_de(ruta_excel);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("No se pudo crear carpeta de respaldos: {}", e))?;

    let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S_%3f").to_string();
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let destino = dir.join(format!("{}_{:03}.xlsx", stamp, seq % 1000));
    fs::copy(ruta_excel, &destino)
        .map_err(|e| format!("No se pudo copiar respaldo automático: {}", e))?;

    let mut archivos: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("Error leyendo respaldos: {}", e))?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("xlsx"))
                .unwrap_or(false)
        })
        .collect();

    archivos.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    for viejo in archivos.into_iter().skip(MAX_RESPALDOS) {
        let _ = fs::remove_file(viejo);
    }

    Ok(())
}

fn formatear_nombre_respaldo(nombre: &str) -> String {
    let stem = nombre.trim_end_matches(".xlsx");
    // Formato esperado: YYYY-MM-DD_HH-MM-SS_mmm_sss
    let partes: Vec<&str> = stem.split('_').collect();
    if partes.len() >= 2 {
        let base = format!("{} {}", partes[0], partes[1].replace('-', ":"));
        if let Ok(dt) = NaiveDateTime::parse_from_str(&base, "%Y-%m-%d %H:%M:%S") {
            return dt.format("%d/%m/%Y %H:%M:%S").to_string();
        }
    }
    stem.to_string()
}

fn listar_respaldos_en(ruta_excel: &Path) -> Result<Vec<RespaldoInfo>, String> {
    let dir = carpeta_respaldos_de(ruta_excel);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut items: Vec<RespaldoInfo> = fs::read_dir(&dir)
        .map_err(|e| format!("Error leyendo respaldos: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let name = entry.file_name().into_string().ok()?;
            if !name.to_lowercase().ends_with(".xlsx") {
                return None;
            }
            let meta = fs::metadata(&path).ok()?;
            Some(RespaldoInfo {
                fecha_legible: formatear_nombre_respaldo(&name),
                archivo: name,
                tamano_bytes: meta.len(),
            })
        })
        .collect();

    items.sort_by(|a, b| b.archivo.cmp(&a.archivo));
    items.truncate(MAX_RESPALDOS);
    Ok(items)
}

pub fn listar_respaldos(nombre_archivo: &str) -> Result<Vec<RespaldoInfo>, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    listar_respaldos_en(&ruta)
}

fn restaurar_respaldo_en(ruta_excel: &Path, archivo_respaldo: &str) -> Result<(), String> {
    let clean_name = archivo_respaldo.trim();
    if clean_name.is_empty()
        || clean_name.contains('/')
        || clean_name.contains('\\')
        || clean_name.contains("..")
    {
        return Err("Nombre de respaldo inválido.".to_string());
    }

    let dir = carpeta_respaldos_de(ruta_excel);
    let origen = dir.join(clean_name);
    if !origen.exists() {
        return Err("La versión de respaldo seleccionada ya no existe.".to_string());
    }

    // Validar que el respaldo sea un Excel legible y reescribirlo saneado en temporal
    // antes de rotar respaldos (para no eliminar el propio respaldo elegido si era el más antiguo).
    let mut book = reader::xlsx::read(&origen)
        .map_err(|e| format!("El respaldo seleccionado está dañado: {}", e))?;

    let parent = ruta_excel.parent().unwrap_or_else(|| Path::new("."));
    let file_name = ruta_excel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("planilla.xlsx");
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_ruta = parent.join(format!(".{}.{}.{}.tmp", file_name, std::process::id(), seq));

    guardar_libro(&mut book, &tmp_ruta)
        .map_err(|e| format!("No se pudo preparar la restauración: {}", e))?;

    let _ = crear_respaldo_en(ruta_excel);
    invalidar_cache(ruta_excel);

    if let Err(e) = fs::rename(&tmp_ruta, ruta_excel) {
        let _ = fs::remove_file(&tmp_ruta);
        return Err(format!(
            "No se pudo restaurar el archivo (verifique que no esté abierto en Excel): {}",
            e
        ));
    }

    Ok(())
}

pub fn restaurar_respaldo(nombre_archivo: &str, archivo_respaldo: &str) -> Result<(), String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    restaurar_respaldo_en(&ruta, archivo_respaldo)?;
    invalidar_priors_en(&obtener_carpeta_archivos());
    Ok(())
}

/// Elimina binarios `printerSettings*.bin` (DEVMODE Windows), atributos de página dependientes
/// de impresora (`horizontalDpi`, `verticalDpi`, `paperSize`, `fitToWidth`/`fitToHeight`) y
/// vistas de diseño de página (`PageLayout`/`PageBreakPreview`), conservando orientación y escala
/// para evitar consultas al spooler de impresora al abrir en Windows.
fn limpiar_printer_settings_libro(book: &mut Spreadsheet) {
    for ws in book.get_sheet_collection_mut() {
        let actual = ws.get_page_setup();
        let mut limpio = PageSetup::default();
        match actual.get_orientation() {
            OrientationValues::Landscape => {
                limpio.set_orientation(OrientationValues::Landscape);
            }
            OrientationValues::Portrait => {
                limpio.set_orientation(OrientationValues::Portrait);
            }
            OrientationValues::Default => {}
        }
        if *actual.get_scale() > 0 {
            limpio.set_scale(*actual.get_scale());
        }
        ws.set_page_setup(limpio);

        let mut views = ws.get_sheets_views().clone();
        let mut cambio_view = false;
        for sv in views.get_sheet_view_list_mut() {
            if matches!(
                sv.get_view(),
                SheetViewValues::PageLayout | SheetViewValues::PageBreakPreview
            ) {
                sv.set_view(SheetViewValues::Normal);
                cambio_view = true;
            }
        }
        if cambio_view {
            ws.set_sheets_views(views);
        }
    }
}

/// Escribe el libro Excel de forma atómica usando un archivo temporal en la misma carpeta.
fn guardar_libro(book: &mut Spreadsheet, ruta: &Path) -> Result<(), String> {
    limpiar_printer_settings_libro(book);
    invalidar_cache(ruta);
    let parent = ruta.parent().unwrap_or_else(|| Path::new("."));
    let file_name = ruta
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("planilla.xlsx");
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_ruta = parent.join(format!(".{}.{}.{}.tmp", file_name, std::process::id(), seq));

    if let Err(e) = writer::xlsx::write(book, &tmp_ruta) {
        let _ = fs::remove_file(&tmp_ruta);
        return Err(format!(
            "No se pudo escribir el archivo temporal de Excel: {}",
            e
        ));
    }

    if let Err(e) = fs::rename(&tmp_ruta, ruta) {
        let _ = fs::remove_file(&tmp_ruta);
        return Err(format!(
            "No se pudo guardar el archivo Excel (verifique que no esté abierto en Excel o bloqueado por otro programa): {}",
            e
        ));
    }

    Ok(())
}

fn leer_celda(ws: &Worksheet, col: u32, row: u32) -> String {
    ws.get_cell((col, row))
        .map(|c| c.get_value().trim().to_string())
        .unwrap_or_default()
}

pub fn serial_excel_a_fecha(serial: f64) -> Option<String> {
    let days = serial.floor() as i64;
    if !(30000..=100000).contains(&days) {
        return None;
    }
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?;
    let fecha = epoch.checked_add_signed(Duration::days(days))?;
    Some(fecha.format("%d/%m/%Y").to_string())
}

pub fn serial_excel_a_hora(serial: f64) -> Option<String> {
    if !serial.is_finite() || serial < 0.0 {
        return None;
    }
    let frac = serial.rem_euclid(1.0);
    let total_minutos = ((frac * 1440.0).round() as u32) % 1440;
    Some(format!("{:02}:{:02}", total_minutos / 60, total_minutos % 60))
}

fn leer_celda_fecha(ws: &Worksheet, col: u32, row: u32) -> String {
    let Some(cell) = ws.get_cell((col, row)) else {
        return String::new();
    };
    if let Some(serial) = cell.get_value_number() {
        if let Some(f) = serial_excel_a_fecha(serial) {
            return f;
        }
    }
    let raw = cell.get_value().trim().to_string();
    if raw.is_empty() {
        return raw;
    }
    if let Ok(serial) = raw.parse::<f64>() {
        if let Some(f) = serial_excel_a_fecha(serial) {
            return f;
        }
    }
    for fmt in ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(&raw, fmt) {
            return d.format("%d/%m/%Y").to_string();
        }
    }
    raw
}

fn leer_celda_hora(ws: &Worksheet, col: u32, row: u32) -> String {
    let Some(cell) = ws.get_cell((col, row)) else {
        return String::new();
    };
    if let Some(serial) = cell.get_value_number() {
        if let Some(h) = serial_excel_a_hora(serial) {
            return h;
        }
    }
    let raw = cell.get_value().trim().to_string();
    if raw.is_empty() {
        return raw;
    }
    if raw.contains(':') {
        let partes: Vec<&str> = raw.split(':').collect();
        if partes.len() >= 2 {
            if let (Ok(h), Ok(m)) = (
                partes[0].trim().parse::<u32>(),
                partes[1].trim().parse::<u32>(),
            ) {
                if h <= 23 && m <= 59 {
                    return format!("{:02}:{:02}", h, m);
                }
            }
        }
    }
    if let Ok(serial) = raw.parse::<f64>() {
        if let Some(h) = serial_excel_a_hora(serial) {
            return h;
        }
    }
    raw
}

fn set_celda_texto(ws: &mut Worksheet, col: u32, row: u32, valor: &str) {
    let cell = ws.get_cell_mut((col, row));
    if valor.is_empty() {
        cell.set_blank();
    } else {
        cell.set_value_string(valor);
    }
}

fn limpiar_fila(ws: &mut Worksheet, row: u32) {
    for col in 1..=17 {
        ws.get_cell_mut((col, row)).set_blank();
    }
}

fn escribir_registro_en_fila(ws: &mut Worksheet, row: u32, consecutivo: u32, datos: &AudienciaInput) {
    limpiar_fila(ws, row);

    ws.get_cell_mut((1, row)).set_value_number(consecutivo);
    set_celda_texto(ws, 2, row, datos.radicado.trim());
    set_celda_texto(ws, 3, row, datos.tipo.trim());
    set_celda_texto(ws, 4, row, datos.fecha.trim());
    set_celda_texto(ws, 5, row, datos.hora.trim());
    set_celda_texto(ws, 6, row, datos.juzgado.trim());

    let realizada = datos.realizada.trim().to_uppercase();
    if realizada == "SI" {
        set_celda_texto(ws, 7, row, "SI");
    } else if realizada == "NO" {
        set_celda_texto(ws, 8, row, "NO");

        if let Some(idx) = mapear_motivo_a_indice(&datos.motivo) {
            let label = MOTIVOS_LABELS[idx];
            set_celda_texto(ws, 9 + idx as u32, row, label);
        }
    }

    set_celda_texto(ws, 17, row, datos.observaciones.trim());
}

fn extraer_registros_de_hoja(ws: &Worksheet) -> Vec<RegistroAudiencia> {
    let mut registros = Vec::new();

    for row in FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS {
        let radicado = leer_celda(ws, 2, row);
        if radicado.is_empty() {
            continue;
        }

        let numero = leer_celda(ws, 1, row)
            .parse::<u32>()
            .unwrap_or(row - FILA_INICIO_DATOS + 1);
        let tipo = leer_celda(ws, 3, row);
        let fecha = leer_celda_fecha(ws, 4, row);
        let hora = leer_celda_hora(ws, 5, row);
        let juzgado = leer_celda(ws, 6, row);
        let col_si = leer_celda(ws, 7, row).to_uppercase();
        let col_no = leer_celda(ws, 8, row).to_uppercase();
        let realizada = if col_si == "SI" {
            "SI".to_string()
        } else if col_no == "NO" {
            "NO".to_string()
        } else {
            "".to_string()
        };

        let mut motivos_flags = [false; 8];
        let mut motivo_principal = String::new();
        for i in 0..8 {
            let val = leer_celda(ws, 9 + i as u32, row);
            if !val.is_empty() {
                motivos_flags[i] = true;
                if motivo_principal.is_empty() {
                    motivo_principal = MOTIVOS_LABELS[i].to_string();
                }
            }
        }

        let observaciones = leer_celda(ws, 17, row);

        registros.push(RegistroAudiencia {
            fila_excel: row,
            numero,
            radicado,
            tipo,
            fecha,
            hora,
            juzgado,
            realizada,
            motivo: motivo_principal,
            motivos_flags,
            observaciones,
        });
    }

    registros
}

fn calcular_y_escribir_totales(ws: &mut Worksheet) -> TotalesExcel {
    let mut totales = TotalesExcel::default();

    for row in FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS {
        let radicado = leer_celda(ws, 2, row);
        if radicado.is_empty() {
            continue;
        }
        totales.num_registros += 1;
        if leer_celda(ws, 7, row).eq_ignore_ascii_case("SI") {
            totales.total_si += 1;
        }
        if leer_celda(ws, 8, row).eq_ignore_ascii_case("NO") {
            totales.total_no += 1;
        }
        for i in 0..8 {
            if !leer_celda(ws, 9 + i as u32, row).is_empty() {
                totales.totales_motivos[i] += 1;
            }
        }
    }

    ws.get_cell_mut((7, FILA_TOTALES))
        .set_value_number(totales.total_si);
    for i in 0..8 {
        ws.get_cell_mut((9 + i as u32, FILA_TOTALES))
            .set_value_number(totales.totales_motivos[i]);
    }
    ws.get_cell_mut((17, FILA_TOTALES))
        .set_value_number(totales.total_no);

    totales
}

fn reordenar_y_recalcular(ws: &mut Worksheet) -> TotalesExcel {
    let registros = extraer_registros_de_hoja(ws);

    let mut con_clave: Vec<(NaiveDate, (u32, u32), AudienciaInput)> = registros
        .into_iter()
        .map(|r| {
            let fecha_ord = NaiveDate::parse_from_str(r.fecha.trim(), "%d/%m/%Y")
                .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());

            let hora_ord = if r.hora.trim().is_empty() {
                (0, 0)
            } else {
                chrono::NaiveTime::parse_from_str(r.hora.trim(), "%H:%M")
                    .map(|t| (t.hour(), t.minute()))
                    .unwrap_or((0, 0))
            };

            let input = AudienciaInput {
                radicado: r.radicado,
                tipo: r.tipo,
                fecha: r.fecha,
                hora: r.hora,
                juzgado: r.juzgado,
                realizada: r.realizada,
                motivo: r.motivo,
                observaciones: r.observaciones,
            };
            (fecha_ord, hora_ord, input)
        })
        .collect();

    // Ordenar por fecha DESC y hora DESC (más recientes primero)
    con_clave.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    // Limpiar el área de datos (filas 11 a 110)
    for row in FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS {
        limpiar_fila(ws, row);
    }

    // Escribir los registros ordenados con numeración consecutiva
    for (idx, (_, _, input)) in con_clave.iter().enumerate() {
        let fila_destino = FILA_INICIO_DATOS + idx as u32;
        if fila_destino <= FILA_MAXIMA_DATOS {
            escribir_registro_en_fila(ws, fila_destino, (idx + 1) as u32, input);
        }
    }

    calcular_y_escribir_totales(ws)
}

type LecturaPlanilla = (Vec<RegistroAudiencia>, TotalesExcel, String);

struct EntradaCache {
    modificado: Option<SystemTime>,
    tamano: u64,
    lectura: LecturaPlanilla,
}

/// Caché en memoria de planillas ya leídas: con cientos de archivos, las estadísticas solo
/// vuelven a abrir los Excel que cambiaron (según fecha de modificación y tamaño).
fn cache_planillas() -> &'static Mutex<HashMap<PathBuf, EntradaCache>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, EntradaCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn invalidar_cache(ruta: &Path) {
    if let Ok(mut c) = cache_planillas().lock() {
        c.remove(ruta);
    }
}

pub(crate) fn leer_archivo_en(ruta: &Path) -> Result<LecturaPlanilla, String> {
    let meta = fs::metadata(ruta).ok();
    let modificado = meta.as_ref().and_then(|m| m.modified().ok());
    let tamano = meta.as_ref().map(|m| m.len()).unwrap_or(0);

    if modificado.is_some() {
        if let Ok(c) = cache_planillas().lock() {
            if let Some(e) = c.get(ruta) {
                if e.modificado == modificado && e.tamano == tamano {
                    return Ok(e.lectura.clone());
                }
            }
        }
    }

    let lectura = leer_archivo_sin_cache(ruta)?;
    if modificado.is_some() {
        if let Ok(mut c) = cache_planillas().lock() {
            c.insert(
                ruta.to_path_buf(),
                EntradaCache {
                    modificado,
                    tamano,
                    lectura: lectura.clone(),
                },
            );
        }
    }
    Ok(lectura)
}

fn leer_archivo_sin_cache(ruta: &Path) -> Result<LecturaPlanilla, String> {
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja principal del Excel".to_string())?;

    let defensor = leer_defensor_de_hoja(ws);
    let registros = extraer_registros_de_hoja(ws);
    let mut totales = TotalesExcel::default();
    totales.num_registros = registros.len() as u32;
    for r in &registros {
        if r.realizada == "SI" {
            totales.total_si += 1;
        } else if r.realizada == "NO" {
            totales.total_no += 1;
        }
        for i in 0..8 {
            if r.motivos_flags[i] {
                totales.totales_motivos[i] += 1;
            }
        }
    }

    Ok((registros, totales, defensor))
}

pub fn leer_archivo_completo(
    nombre_archivo: &str,
) -> Result<(Vec<RegistroAudiencia>, TotalesExcel, String), String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    leer_archivo_en(&ruta)
}

fn actualizar_defensor_en(ruta: &Path, defensor: &str) -> Result<String, String> {
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }
    let _ = crear_respaldo_en(ruta);

    let mut book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;

    escribir_defensor_en_hoja(ws, defensor);
    let resultado = leer_defensor_de_hoja(ws);
    guardar_libro(&mut book, ruta)?;
    Ok(resultado)
}

pub fn actualizar_defensor_archivo(nombre_archivo: &str, defensor: &str) -> Result<String, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    let res = actualizar_defensor_en(&ruta, defensor)?;
    invalidar_priors_en(&obtener_carpeta_archivos());
    Ok(res)
}

fn guardar_nueva_audiencia_en_con_defensor(
    ruta: &Path,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<(TotalesExcel, String), String> {
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let mut book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;
    let defensor = leer_defensor_de_hoja(ws);

    if !permitir_duplicado {
        let existentes = extraer_registros_de_hoja(ws);
        if let Some(dup) = buscar_posible_duplicado(&existentes, datos, None) {
            let json = serde_json::to_string(&dup)
                .map_err(|e| format!("Error serializando duplicado: {}", e))?;
            return Err(format!("POSIBLE_DUPLICADO:{}", json));
        }
    }

    let mut fila_destino = None;
    for row in FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS {
        if leer_celda(ws, 2, row).is_empty() {
            fila_destino = Some(row);
            break;
        }
    }

    let fila = fila_destino.ok_or_else(|| {
        "El archivo ya alcanzó el límite máximo de 100 audiencias (filas 11 a 110). Cree un nuevo archivo.".to_string()
    })?;

    let _ = crear_respaldo_en(ruta);

    let consecutivo = fila - FILA_INICIO_DATOS + 1;
    escribir_registro_en_fila(ws, fila, consecutivo, datos);

    let totales = reordenar_y_recalcular(ws);
    guardar_libro(&mut book, ruta)?;
    Ok((totales, defensor))
}

#[cfg(test)]
fn guardar_nueva_audiencia_en(
    ruta: &Path,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    guardar_nueva_audiencia_en_con_defensor(ruta, datos, permitir_duplicado).map(|(t, _)| t)
}

pub fn guardar_nueva_audiencia(
    nombre_archivo: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    let _ = crate::uso_catalogos::asegurar_uso_inicializado_actual();
    let ruta = ruta_archivo(nombre_archivo)?;
    let (totales, defensor) = guardar_nueva_audiencia_en_con_defensor(&ruta, datos, permitir_duplicado)?;
    let base = obtener_carpeta_archivos();
    let _ = actualizar_priors_incremental_con_defensor_en(&base, &defensor, None, Some(datos));
    let _ = crate::uso_catalogos::registrar_delta_audiencia_actual(None, Some(datos));
    Ok(totales)
}

fn validar_fila_y_radicado(ws: &Worksheet, fila_excel: u32, radicado_esperado: &str) -> Result<(), String> {
    if !(FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS).contains(&fila_excel) {
        return Err(format!("Número de fila inválido: {}", fila_excel));
    }
    let radicado_en_fila = leer_celda(ws, 2, fila_excel);
    if radicado_en_fila != radicado_esperado.trim() {
        return Err(
            "El archivo cambió fuera de Lokus. Recargue la planilla (F5) e intente de nuevo."
                .to_string(),
        );
    }
    Ok(())
}

fn leer_registro_en_fila_para_priors(ws: &Worksheet, row: u32) -> Option<AudienciaInput> {
    let radicado = leer_celda(ws, 2, row);
    if radicado.is_empty() {
        return None;
    }
    let tipo = leer_celda(ws, 3, row);
    let fecha = leer_celda_fecha(ws, 4, row);
    let hora = leer_celda_hora(ws, 5, row);
    let juzgado = leer_celda(ws, 6, row);
    let col_si = leer_celda(ws, 7, row).to_uppercase();
    let col_no = leer_celda(ws, 8, row).to_uppercase();
    let realizada = if col_si == "SI" {
        "SI".to_string()
    } else if col_no == "NO" {
        "NO".to_string()
    } else {
        String::new()
    };
    let mut motivo = String::new();
    for i in 0..8 {
        if !leer_celda(ws, 9 + i as u32, row).is_empty() {
            motivo = MOTIVOS_LABELS[i].to_string();
            break;
        }
    }
    let observaciones = leer_celda(ws, 17, row);
    Some(AudienciaInput {
        radicado,
        tipo,
        fecha,
        hora,
        juzgado,
        realizada,
        motivo,
        observaciones,
    })
}

fn actualizar_audiencia_en_con_anterior(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<(TotalesExcel, Option<AudienciaInput>, String), String> {
    if !(FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS).contains(&fila_excel) {
        return Err(format!("Número de fila inválido: {}", fila_excel));
    }
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let mut book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;
    let defensor = leer_defensor_de_hoja(ws);

    validar_fila_y_radicado(ws, fila_excel, radicado_esperado)?;
    let anterior = leer_registro_en_fila_para_priors(ws, fila_excel);

    if !permitir_duplicado {
        let existentes = extraer_registros_de_hoja(ws);
        if let Some(dup) = buscar_posible_duplicado(&existentes, datos, Some(fila_excel)) {
            let json = serde_json::to_string(&dup)
                .map_err(|e| format!("Error serializando duplicado: {}", e))?;
            return Err(format!("POSIBLE_DUPLICADO:{}", json));
        }
    }

    let _ = crear_respaldo_en(ruta);

    let consecutivo = fila_excel - FILA_INICIO_DATOS + 1;
    escribir_registro_en_fila(ws, fila_excel, consecutivo, datos);

    let totales = reordenar_y_recalcular(ws);
    guardar_libro(&mut book, ruta)?;
    Ok((totales, anterior, defensor))
}

#[cfg(test)]
fn actualizar_audiencia_en(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    actualizar_audiencia_en_con_anterior(
        ruta,
        fila_excel,
        radicado_esperado,
        datos,
        permitir_duplicado,
    )
    .map(|(t, _, _)| t)
}

pub fn actualizar_audiencia_existente(
    nombre_archivo: &str,
    fila_excel: u32,
    radicado_esperado: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    let _ = crate::uso_catalogos::asegurar_uso_inicializado_actual();
    let ruta = ruta_archivo(nombre_archivo)?;
    let (totales, anterior, defensor) = actualizar_audiencia_en_con_anterior(
        &ruta,
        fila_excel,
        radicado_esperado,
        datos,
        permitir_duplicado,
    )?;
    let base = obtener_carpeta_archivos();
    let _ = actualizar_priors_incremental_con_defensor_en(
        &base,
        &defensor,
        anterior.as_ref(),
        Some(datos),
    );
    let _ = crate::uso_catalogos::registrar_delta_audiencia_actual(anterior.as_ref(), Some(datos));
    Ok(totales)
}

fn eliminar_registro_en_con_anterior(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
) -> Result<(TotalesExcel, Option<AudienciaInput>, String), String> {
    if !(FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS).contains(&fila_excel) {
        return Err(format!("Número de fila inválido: {}", fila_excel));
    }
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let mut book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;
    let defensor = leer_defensor_de_hoja(ws);

    validar_fila_y_radicado(ws, fila_excel, radicado_esperado)?;
    let anterior = leer_registro_en_fila_para_priors(ws, fila_excel);

    let _ = crear_respaldo_en(ruta);

    limpiar_fila(ws, fila_excel);

    let totales = reordenar_y_recalcular(ws);
    guardar_libro(&mut book, ruta)?;
    Ok((totales, anterior, defensor))
}

#[cfg(test)]
fn eliminar_registro_en(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
) -> Result<TotalesExcel, String> {
    eliminar_registro_en_con_anterior(ruta, fila_excel, radicado_esperado).map(|(t, _, _)| t)
}

pub fn eliminar_registro_audiencia(
    nombre_archivo: &str,
    fila_excel: u32,
    radicado_esperado: &str,
) -> Result<TotalesExcel, String> {
    let _ = crate::uso_catalogos::asegurar_uso_inicializado_actual();
    let ruta = ruta_archivo(nombre_archivo)?;
    let (totales, anterior, defensor) =
        eliminar_registro_en_con_anterior(&ruta, fila_excel, radicado_esperado)?;
    let base = obtener_carpeta_archivos();
    let _ = actualizar_priors_incremental_con_defensor_en(&base, &defensor, anterior.as_ref(), None);
    let _ = crate::uso_catalogos::registrar_delta_audiencia_actual(anterior.as_ref(), None);
    Ok(totales)
}

pub(crate) fn recolectar_radicados_en(carpeta: &Path) -> Vec<String> {
    let Ok(nombres) = listar_archivos_en(carpeta) else {
        return Vec::new();
    };
    let mut vistos = HashSet::new();
    let mut salida = Vec::new();
    for nombre in nombres {
        let ruta = carpeta.join(nombre);
        if let Ok((registros, _, _)) = leer_archivo_en(&ruta) {
            for r in registros {
                let rad = r.radicado.trim();
                if !rad.is_empty() && vistos.insert(rad.to_string()) {
                    salida.push(rad.to_string());
                }
            }
        }
    }
    salida
}

fn nombre_mes_es(mes: u32) -> &'static str {
    match mes {
        1 => "Enero",
        2 => "Febrero",
        3 => "Marzo",
        4 => "Abril",
        5 => "Mayo",
        6 => "Junio",
        7 => "Julio",
        8 => "Agosto",
        9 => "Septiembre",
        10 => "Octubre",
        11 => "Noviembre",
        12 => "Diciembre",
        _ => "Mes",
    }
}

fn redondear_1_decimal(val: f64) -> f64 {
    (val * 10.0).round() / 10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistroEstadisticasLocal {
    pub formato: u32,
    pub ids_por_ruta: BTreeMap<String, String>,
    pub archivadas: Vec<ResumenPlanillaArchivada>,
    pub orden_carpetas: Vec<String>,
    pub colores_carpetas: BTreeMap<String, String>,
}

impl Default for RegistroEstadisticasLocal {
    fn default() -> Self {
        Self {
            formato: 1,
            ids_por_ruta: BTreeMap::new(),
            archivadas: Vec::new(),
            orden_carpetas: Vec::new(),
            colores_carpetas: BTreeMap::new(),
        }
    }
}

pub fn obtener_orden_carpetas_en(carpeta: &Path) -> Vec<String> {
    cargar_registro_estadisticas_en(carpeta).orden_carpetas
}

pub fn obtener_orden_carpetas() -> Vec<String> {
    obtener_orden_carpetas_en(&obtener_carpeta_archivos())
}

pub fn guardar_orden_carpetas_en(carpeta: &Path, orden: Vec<String>) -> Result<Vec<String>, String> {
    let mut reg = cargar_registro_estadisticas_en(carpeta);
    let mut vistos = HashSet::new();
    let mut limpio = Vec::new();
    for item in orden {
        if let Ok(ruta) = sanitizar_ruta_carpeta(&item) {
            if vistos.insert(ruta.clone()) {
                limpio.push(ruta);
            }
        }
    }
    reg.orden_carpetas = limpio.clone();
    guardar_registro_estadisticas_en(carpeta, &reg)?;
    Ok(limpio)
}

pub fn guardar_orden_carpetas(orden: Vec<String>) -> Result<Vec<String>, String> {
    guardar_orden_carpetas_en(&obtener_carpeta_archivos(), orden)
}

pub fn obtener_colores_carpetas_en(carpeta: &Path) -> BTreeMap<String, String> {
    cargar_registro_estadisticas_en(carpeta).colores_carpetas
}

pub fn obtener_colores_carpetas() -> BTreeMap<String, String> {
    obtener_colores_carpetas_en(&obtener_carpeta_archivos())
}

pub fn guardar_color_carpeta_en(
    carpeta: &Path,
    carpeta_relativa: &str,
    color: &str,
) -> Result<BTreeMap<String, String>, String> {
    let limpia = sanitizar_ruta_carpeta(carpeta_relativa)?;
    let mut ruta_dir = carpeta.to_path_buf();
    for seg in limpia.split('/') {
        ruta_dir.push(seg);
    }
    if !ruta_dir.is_dir() {
        return Err("La carpeta seleccionada no existe.".to_string());
    }
    let color_limpio = validar_color_carpeta(color)?;
    let mut reg = cargar_registro_estadisticas_en(carpeta);
    if color_limpio.is_empty() {
        reg.colores_carpetas.remove(&limpia);
    } else {
        reg.colores_carpetas.insert(limpia, color_limpio);
    }
    guardar_registro_estadisticas_en(carpeta, &reg)?;
    Ok(reg.colores_carpetas)
}

pub fn guardar_color_carpeta(
    carpeta_relativa: &str,
    color: &str,
) -> Result<BTreeMap<String, String>, String> {
    guardar_color_carpeta_en(&obtener_carpeta_archivos(), carpeta_relativa, color)
}

pub fn generar_id_planilla() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("pl-{:x}-{:04x}", nanos, seq % 0xffff)
}

pub fn cargar_registro_estadisticas_en(carpeta: &Path) -> RegistroEstadisticasLocal {
    let ruta = carpeta.join(NOMBRE_INDICE_ESTADISTICAS);
    if let Ok(contenido) = fs::read_to_string(&ruta) {
        if let Ok(mut reg) = serde_json::from_str::<RegistroEstadisticasLocal>(&contenido) {
            if reg.formato == 0 {
                reg.formato = 1;
            }
            return reg;
        }
    }
    RegistroEstadisticasLocal::default()
}

pub fn guardar_registro_estadisticas_en(
    carpeta: &Path,
    reg: &RegistroEstadisticasLocal,
) -> Result<(), String> {
    fs::create_dir_all(carpeta)
        .map_err(|e| format!("No se pudo preparar carpeta para índice estadístico: {}", e))?;
    let ruta = carpeta.join(NOMBRE_INDICE_ESTADISTICAS);
    let json = serde_json::to_string_pretty(reg)
        .map_err(|e| format!("Error serializando índice de estadísticas: {}", e))?;
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = carpeta.join(format!(".{}.{}.{}.tmp", NOMBRE_INDICE_ESTADISTICAS, std::process::id(), seq));
    fs::write(&tmp, json)
        .map_err(|e| format!("Error escribiendo índice de estadísticas: {}", e))?;
    if let Err(e) = fs::rename(&tmp, &ruta) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Error guardando índice de estadísticas: {}", e));
    }
    Ok(())
}

pub fn asegurar_id_ruta(reg: &mut RegistroEstadisticasLocal, ruta_relativa: &str) -> (String, bool) {
    if let Some(id) = reg.ids_por_ruta.get(ruta_relativa) {
        if !id.trim().is_empty() {
            return (id.clone(), false);
        }
    }
    let nuevo = generar_id_planilla();
    reg.ids_por_ruta.insert(ruta_relativa.to_string(), nuevo.clone());
    (nuevo, true)
}

fn actualizar_ruta_en_indice(carpeta: &Path, ruta_anterior: &str, ruta_nueva: &str) -> Result<(), String> {
    let mut reg = cargar_registro_estadisticas_en(carpeta);
    let id = reg
        .ids_por_ruta
        .remove(ruta_anterior)
        .unwrap_or_else(generar_id_planilla);
    reg.ids_por_ruta.insert(ruta_nueva.to_string(), id);
    guardar_registro_estadisticas_en(carpeta, &reg)
}

fn actualizar_prefijo_carpeta_indice(
    carpeta: &Path,
    prefijo_anterior: &str,
    prefijo_nuevo: &str,
) -> Result<(), String> {
    let mut reg = cargar_registro_estadisticas_en(carpeta);
    let prefijo_slash = format!("{}/", prefijo_anterior);
    let mut nuevas: BTreeMap<String, String> = BTreeMap::new();
    let mut cambio = false;
    for (k, v) in std::mem::take(&mut reg.ids_por_ruta) {
        if let Some(resto) = k.strip_prefix(&prefijo_slash) {
            nuevas.insert(format!("{}/{}", prefijo_nuevo, resto), v);
            cambio = true;
        } else {
            nuevas.insert(k, v);
        }
    }
    reg.ids_por_ruta = nuevas;

    for item in &mut reg.orden_carpetas {
        if *item == prefijo_anterior {
            *item = prefijo_nuevo.to_string();
            cambio = true;
        } else if let Some(resto) = item.strip_prefix(&prefijo_slash) {
            *item = format!("{}/{}", prefijo_nuevo, resto);
            cambio = true;
        }
    }

    let mut nuevos_colores: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in std::mem::take(&mut reg.colores_carpetas) {
        if k == prefijo_anterior {
            nuevos_colores.insert(prefijo_nuevo.to_string(), v);
            cambio = true;
        } else if let Some(resto) = k.strip_prefix(&prefijo_slash) {
            nuevos_colores.insert(format!("{}/{}", prefijo_nuevo, resto), v);
            cambio = true;
        } else {
            nuevos_colores.insert(k, v);
        }
    }
    reg.colores_carpetas = nuevos_colores;

    if cambio {
        guardar_registro_estadisticas_en(carpeta, &reg)?;
    }
    Ok(())
}

pub fn upsert_resumen_archivado(reg: &mut RegistroEstadisticasLocal, resumen: ResumenPlanillaArchivada) {
    reg.archivadas.retain(|r| r.id != resumen.id);
    reg.archivadas.push(resumen);
}

fn agrupar_registros_por_mes(registros: &[RegistroAudiencia]) -> Vec<ResumenMesPlanilla> {
    let mut mapa: BTreeMap<(i32, u32), ResumenMesPlanilla> = BTreeMap::new();
    for r in registros {
        let clave = NaiveDate::parse_from_str(r.fecha.trim(), "%d/%m/%Y")
            .map(|d| (d.year(), d.month()))
            .unwrap_or((0, 0));
        let entry = mapa.entry(clave).or_insert_with(|| ResumenMesPlanilla {
            anio: clave.0,
            mes: clave.1,
            ..Default::default()
        });
        entry.total += 1;
        if r.realizada == "SI" {
            entry.total_si += 1;
        } else if r.realizada == "NO" {
            entry.total_no += 1;
        }
        for i in 0..8 {
            if r.motivos_flags[i] {
                entry.totales_motivos[i] += 1;
            }
        }
    }
    mapa.into_values().collect()
}

pub fn construir_resumen_desde_lectura(
    id: String,
    archivo: String,
    defensor: &str,
    registros: &[RegistroAudiencia],
) -> ResumenPlanillaArchivada {
    let por_mes = agrupar_registros_por_mes(registros);
    let mut resumen = ResumenPlanillaArchivada {
        id,
        archivo,
        defensor: defensor.split_whitespace().collect::<Vec<_>>().join(" "),
        archivado_en: Local::now().to_rfc3339(),
        num_registros: registros.len() as u32,
        por_mes,
        ..Default::default()
    };
    for m in &resumen.por_mes {
        resumen.total_si += m.total_si;
        resumen.total_no += m.total_no;
        for i in 0..8 {
            resumen.totales_motivos[i] += m.totales_motivos[i];
        }
    }
    resumen
}

pub fn eliminar_estadistica_archivada_en(carpeta: &Path, id: &str) -> Result<(), String> {
    let mut reg = cargar_registro_estadisticas_en(carpeta);
    let antes = reg.archivadas.len();
    reg.archivadas.retain(|a| a.id != id);
    if reg.archivadas.len() == antes {
        return Err("No se encontró ese resumen archivado.".to_string());
    }
    guardar_registro_estadisticas_en(carpeta, &reg)
}

pub fn eliminar_estadistica_archivada(id: &str) -> Result<(), String> {
    eliminar_estadistica_archivada_en(&obtener_carpeta_archivos(), id)
}

#[derive(Default)]
struct AcumuladorDefensor {
    display: String,
    archivos: Vec<String>,
    total: u32,
    si: u32,
    no: u32,
    motivos: [u32; 8],
}

fn acumular_planilla_en_stats(
    nombre_display: &str,
    defensor_norm: &str,
    por_mes: &[ResumenMesPlanilla],
    anio: Option<i32>,
    mes: Option<u32>,
    filtro_archivo: Option<&str>,
    filtro_defensor: Option<&str>,
    meses_map: &mut BTreeMap<(i32, u32), u32>,
    defensores_map: &mut HashMap<String, AcumuladorDefensor>,
    stats: &mut EstadisticasGlobales,
) {
    if let Some(filtro_arch) = filtro_archivo {
        if nombre_display != filtro_arch {
            return;
        }
    }

    if let Some(filtro_def) = filtro_defensor {
        if filtro_def == "__SIN_ASIGNAR__" {
            if !defensor_norm.is_empty() {
                return;
            }
        } else if defensor_norm.to_lowercase() != filtro_def.to_lowercase() {
            return;
        }
    }

    let mut tuvo_datos_en_rango = false;

    for m in por_mes {
        if m.anio > 0 && m.mes > 0 {
            *meses_map.entry((m.anio, m.mes)).or_insert(0) += m.total;
        }

        let cumple_periodo = match (anio, mes) {
            (Some(y), Some(mes_num)) => m.anio == y && m.mes == mes_num,
            (Some(y), None) => m.anio == y,
            (None, Some(mes_num)) => m.mes == mes_num,
            (None, None) => true,
        };

        if !cumple_periodo {
            continue;
        }

        tuvo_datos_en_rango = true;
        stats.total_audiencias += m.total;
        stats.total_si += m.total_si;
        stats.total_no += m.total_no;
        for i in 0..8 {
            stats.totales_motivos[i] += m.totales_motivos[i];
        }

        let (clave_def, display_def) = if defensor_norm.is_empty() {
            (
                format!("__sin_asignar__:{}", nombre_display.to_lowercase()),
                format!("Sin asignar ({})", nombre_display.trim_end_matches(".xlsx")),
            )
        } else {
            (defensor_norm.to_lowercase(), defensor_norm.to_string())
        };

        let acc = defensores_map
            .entry(clave_def)
            .or_insert_with(|| AcumuladorDefensor {
                display: display_def,
                ..Default::default()
            });
        let archivo_str = nombre_display.to_string();
        if !acc.archivos.contains(&archivo_str) {
            acc.archivos.push(archivo_str);
        }
        acc.total += m.total;
        acc.si += m.total_si;
        acc.no += m.total_no;
        for i in 0..8 {
            acc.motivos[i] += m.totales_motivos[i];
        }
    }

    if tuvo_datos_en_rango {
        stats.total_planillas_con_datos += 1;
    }
}

/// Consolida estadísticas globales o filtradas por año/mes/planilla/defensor sobre los Excel activos
/// y los resúmenes archivados (sin duplicar nunca una planilla que siga en disco con el mismo ID).
pub fn calcular_estadisticas_en(
    carpeta: &Path,
    anio: Option<i32>,
    mes: Option<u32>,
    archivo_filtro: Option<&str>,
    defensor_filtro: Option<&str>,
) -> Result<EstadisticasGlobales, String> {
    let nombres = listar_archivos_en(carpeta)?;
    let mut reg_est = cargar_registro_estadisticas_en(carpeta);
    let mut cambio_indice = false;

    let filtro_archivo_limpio = archivo_filtro
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let filtro_defensor_limpio = defensor_filtro
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty());

    let mut stats = EstadisticasGlobales {
        total_planillas: nombres.len() as u32,
        ..Default::default()
    };

    let mut ids_activos: HashSet<String> = HashSet::new();
    let mut meses_map: BTreeMap<(i32, u32), u32> = BTreeMap::new();
    let mut defensores_map: HashMap<String, AcumuladorDefensor> = HashMap::new();
    let mut defensores_disponibles_map: HashMap<String, String> = HashMap::new();
    let mut priors_acum = IndicePriorsExcel {
        formato: FORMATO_PRIORS_ACTUAL,
        actualizada: Local::now().to_rfc3339(),
        ..Default::default()
    };

    for nombre in &nombres {
        let (id, creado) = asegurar_id_ruta(&mut reg_est, nombre);
        if creado {
            cambio_indice = true;
        }
        ids_activos.insert(id.clone());

        let ruta = carpeta.join(nombre);
        let (registros, _totales, defensor) = leer_archivo_en(&ruta)
            .map_err(|e| format!("Error leyendo '{}': {}", nombre, e))?;
        for r in &registros {
            acumular_registro_en_priors_con_defensor(
                &mut priors_acum,
                &defensor,
                &r.radicado,
                &r.tipo,
                &r.hora,
                &r.juzgado,
                &r.realizada,
                &r.motivo,
            );
        }

        let defensor_norm = defensor.split_whitespace().collect::<Vec<_>>().join(" ");
        if !defensor_norm.is_empty() {
            defensores_disponibles_map
                .entry(defensor_norm.to_lowercase())
                .or_insert_with(|| defensor_norm.clone());
        }

        stats.planillas.push(PlanillaResumen {
            id,
            archivo: nombre.clone(),
            defensor: defensor_norm.clone(),
            num_registros: registros.len() as u32,
            archivada: false,
        });

        let por_mes = agrupar_registros_por_mes(&registros);
        acumular_planilla_en_stats(
            nombre,
            &defensor_norm,
            &por_mes,
            anio,
            mes,
            filtro_archivo_limpio,
            filtro_defensor_limpio.as_deref(),
            &mut meses_map,
            &mut defensores_map,
            &mut stats,
        );
    }

    if cambio_indice {
        let _ = guardar_registro_estadisticas_en(carpeta, &reg_est);
    }
    let _ = guardar_priors_en(carpeta, &priors_acum);

    for arch in &reg_est.archivadas {
        if ids_activos.contains(&arch.id) {
            continue;
        }
        stats.total_planillas += 1;
        stats.total_archivadas += 1;

        let defensor_norm = arch.defensor.split_whitespace().collect::<Vec<_>>().join(" ");
        if !defensor_norm.is_empty() {
            defensores_disponibles_map
                .entry(defensor_norm.to_lowercase())
                .or_insert_with(|| defensor_norm.clone());
        }

        stats.planillas.push(PlanillaResumen {
            id: arch.id.clone(),
            archivo: arch.archivo.clone(),
            defensor: defensor_norm.clone(),
            num_registros: arch.num_registros,
            archivada: true,
        });

        acumular_planilla_en_stats(
            &arch.archivo,
            &defensor_norm,
            &arch.por_mes,
            anio,
            mes,
            filtro_archivo_limpio,
            filtro_defensor_limpio.as_deref(),
            &mut meses_map,
            &mut defensores_map,
            &mut stats,
        );
    }

    if stats.total_audiencias > 0 {
        stats.efectividad_pct =
            redondear_1_decimal((stats.total_si as f64 / stats.total_audiencias as f64) * 100.0);
    }

    stats.meses_disponibles = meses_map
        .into_iter()
        .rev()
        .map(|((y, m), cantidad)| MesDisponible {
            anio: y,
            mes: m,
            etiqueta: format!("{} {}", nombre_mes_es(m), y),
            cantidad,
        })
        .collect();

    let mut por_defensor: Vec<ResumenDefensor> = defensores_map
        .into_values()
        .map(|mut acc| {
            acc.archivos.sort();
            let efect = if acc.total > 0 {
                redondear_1_decimal((acc.si as f64 / acc.total as f64) * 100.0)
            } else {
                0.0
            };
            let mut best_idx = None;
            let mut best_val = 0u32;
            for i in 0..8 {
                if acc.motivos[i] > best_val {
                    best_val = acc.motivos[i];
                    best_idx = Some(i);
                }
            }
            let motivo_frecuente = best_idx
                .map(|i| format!("{} ({})", MOTIVOS_LABELS[i], best_val))
                .unwrap_or_else(|| "—".to_string());

            ResumenDefensor {
                defensor: acc.display,
                archivos: acc.archivos,
                total: acc.total,
                total_si: acc.si,
                total_no: acc.no,
                efectividad_pct: efect,
                motivo_frecuente,
            }
        })
        .collect();

    por_defensor.sort_by(|a, b| {
        b.total
            .cmp(&a.total)
            .then_with(|| a.defensor.to_lowercase().cmp(&b.defensor.to_lowercase()))
    });
    stats.por_defensor = por_defensor;

    let mut defs_disp: Vec<String> = defensores_disponibles_map.into_values().collect();
    defs_disp.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    stats.defensores_disponibles = defs_disp;

    Ok(stats)
}

pub fn obtener_estadisticas_globales(
    anio: Option<i32>,
    mes: Option<u32>,
    archivo: Option<String>,
    defensor: Option<String>,
) -> Result<EstadisticasGlobales, String> {
    calcular_estadisticas_en(
        &obtener_carpeta_archivos(),
        anio,
        mes,
        archivo.as_deref(),
        defensor.as_deref(),
    )
}

fn extraer_hora_num(hora: &str) -> Option<u32> {
    let h_str = hora.trim().split(':').next()?.trim();
    let h = h_str.parse::<u32>().ok()?;
    if h <= 23 {
        Some(h)
    } else {
        None
    }
}

fn incrementar_arista(
    mapa: &mut BTreeMap<String, BTreeMap<String, u32>>,
    origen: &str,
    destino: &str,
) {
    if origen.is_empty() || destino.is_empty() {
        return;
    }
    *mapa
        .entry(origen.to_string())
        .or_default()
        .entry(destino.to_string())
        .or_insert(0) += 1;
}

fn decrementar_arista(
    mapa: &mut BTreeMap<String, BTreeMap<String, u32>>,
    origen: &str,
    destino: &str,
) {
    if origen.is_empty() || destino.is_empty() {
        return;
    }
    let mut quitar_origen = false;
    if let Some(dests) = mapa.get_mut(origen) {
        if let Some(c) = dests.get_mut(destino) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                dests.remove(destino);
            }
        }
        quitar_origen = dests.is_empty();
    }
    if quitar_origen {
        mapa.remove(origen);
    }
}

pub fn acumular_registro_en_priors_con_defensor(
    priors: &mut IndicePriorsExcel,
    defensor: &str,
    radicado: &str,
    tipo: &str,
    hora: &str,
    juzgado: &str,
    realizada: &str,
    motivo: &str,
) {
    priors.total_registros = priors.total_registros.saturating_add(1);
    let juz_limpio = juzgado.split_whitespace().collect::<Vec<_>>().join(" ");
    if !juz_limpio.is_empty() {
        *priors.juzgados_freq.entry(juz_limpio.clone()).or_insert(0) += 1;
    }
    let tipo_limpio = tipo.split_whitespace().collect::<Vec<_>>().join(" ");
    if !tipo_limpio.is_empty() {
        *priors.tipos_freq.entry(tipo_limpio.clone()).or_insert(0) += 1;
    }
    if let Some(h) = extraer_hora_num(hora) {
        *priors.horas_freq.entry(h).or_insert(0) += 1;
    }
    if realizada.trim().eq_ignore_ascii_case("SI") {
        priors.realizada_si = priors.realizada_si.saturating_add(1);
    } else if realizada.trim().eq_ignore_ascii_case("NO") {
        priors.realizada_no = priors.realizada_no.saturating_add(1);
        if let Some(idx) = mapear_motivo_a_indice(motivo) {
            priors.motivos_freq[idx] = priors.motivos_freq[idx].saturating_add(1);
        }
    }
    let def_limpio = defensor.split_whitespace().collect::<Vec<_>>().join(" ");
    let def_clave = normalizar_clave_defensor(&def_limpio);
    let rad_limpio = radicado.split_whitespace().collect::<Vec<_>>().join(" ");

    for num in extraer_numeros_radicado(&rad_limpio) {
        let entry = priors
            .procesos_por_numero
            .entry(num)
            .or_insert_with(ProcesoHabitual::default);
        entry.frecuencia = entry.frecuencia.saturating_add(1);
        if !rad_limpio.is_empty() && rad_limpio.len() >= entry.radicado_completo.len() {
            entry.radicado_completo = rad_limpio.clone();
        }
        if !juz_limpio.is_empty() {
            entry.juzgado_habitual = juz_limpio.clone();
        }
        if !tipo_limpio.is_empty() {
            entry.tipo_habitual = tipo_limpio.clone();
        }
        if !def_limpio.is_empty() {
            entry.defensor_habitual = def_limpio.clone();
        }
    }

    // Aristas del Mapa de Conexiones ligero (GrafoAsociaciones)
    if !def_clave.is_empty() {
        incrementar_arista(&mut priors.grafo.defensor_a_juzgados, &def_clave, &juz_limpio);
        incrementar_arista(&mut priors.grafo.defensor_a_tipos, &def_clave, &tipo_limpio);
        if !juz_limpio.is_empty() {
            for sigla in extraer_siglas_cortas_juzgado(&juz_limpio) {
                *priors
                    .grafo
                    .defensor_sigla_a_juzgado
                    .entry(def_clave.clone())
                    .or_default()
                    .entry(sigla)
                    .or_default()
                    .entry(juz_limpio.clone())
                    .or_insert(0) += 1;
            }
        }
    }
    if let Some(prefijo) = extraer_prefijo_dane_radicado(&rad_limpio) {
        incrementar_arista(
            &mut priors.grafo.prefijo_radicado_a_juzgados,
            &prefijo,
            &juz_limpio,
        );
    }
    let tipo_clave = normalizar_clave_catalogo(&tipo_limpio);
    if !tipo_clave.is_empty() && tipo_clave != "otra" {
        incrementar_arista(&mut priors.grafo.tipo_a_juzgados, &tipo_clave, &juz_limpio);
    }
}

pub fn acumular_registro_en_priors(
    priors: &mut IndicePriorsExcel,
    radicado: &str,
    tipo: &str,
    hora: &str,
    juzgado: &str,
    realizada: &str,
    motivo: &str,
) {
    acumular_registro_en_priors_con_defensor(
        priors, "", radicado, tipo, hora, juzgado, realizada, motivo,
    );
}

fn restar_registro_de_priors_con_defensor(
    priors: &mut IndicePriorsExcel,
    defensor: &str,
    reg: &AudienciaInput,
) {
    priors.total_registros = priors.total_registros.saturating_sub(1);
    let juz_limpio = reg.juzgado.split_whitespace().collect::<Vec<_>>().join(" ");
    if !juz_limpio.is_empty() {
        if let Some(c) = priors.juzgados_freq.get_mut(&juz_limpio) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                priors.juzgados_freq.remove(&juz_limpio);
            }
        }
    }
    let tipo_limpio = reg.tipo.split_whitespace().collect::<Vec<_>>().join(" ");
    if !tipo_limpio.is_empty() {
        if let Some(c) = priors.tipos_freq.get_mut(&tipo_limpio) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                priors.tipos_freq.remove(&tipo_limpio);
            }
        }
    }
    if let Some(h) = extraer_hora_num(&reg.hora) {
        if let Some(c) = priors.horas_freq.get_mut(&h) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                priors.horas_freq.remove(&h);
            }
        }
    }
    if reg.realizada.trim().eq_ignore_ascii_case("SI") {
        priors.realizada_si = priors.realizada_si.saturating_sub(1);
    } else if reg.realizada.trim().eq_ignore_ascii_case("NO") {
        priors.realizada_no = priors.realizada_no.saturating_sub(1);
        if let Some(idx) = mapear_motivo_a_indice(&reg.motivo) {
            priors.motivos_freq[idx] = priors.motivos_freq[idx].saturating_sub(1);
        }
    }
    for num in extraer_numeros_radicado(&reg.radicado) {
        if let Some(p) = priors.procesos_por_numero.get_mut(&num) {
            p.frecuencia = p.frecuencia.saturating_sub(1);
            if p.frecuencia == 0 {
                priors.procesos_por_numero.remove(&num);
            }
        }
    }

    let def_clave = normalizar_clave_defensor(defensor);
    if !def_clave.is_empty() {
        decrementar_arista(&mut priors.grafo.defensor_a_juzgados, &def_clave, &juz_limpio);
        decrementar_arista(&mut priors.grafo.defensor_a_tipos, &def_clave, &tipo_limpio);
        if !juz_limpio.is_empty() {
            for sigla in extraer_siglas_cortas_juzgado(&juz_limpio) {
                if let Some(por_sigla) = priors.grafo.defensor_sigla_a_juzgado.get_mut(&def_clave) {
                    decrementar_arista(por_sigla, &sigla, &juz_limpio);
                    if por_sigla.is_empty() {
                        priors.grafo.defensor_sigla_a_juzgado.remove(&def_clave);
                    }
                }
            }
        }
    }
    if let Some(prefijo) = extraer_prefijo_dane_radicado(&reg.radicado) {
        decrementar_arista(
            &mut priors.grafo.prefijo_radicado_a_juzgados,
            &prefijo,
            &juz_limpio,
        );
    }
    let tipo_clave = normalizar_clave_catalogo(&tipo_limpio);
    if !tipo_clave.is_empty() && tipo_clave != "otra" {
        decrementar_arista(&mut priors.grafo.tipo_a_juzgados, &tipo_clave, &juz_limpio);
    }
}

pub fn cargar_priors_en(carpeta: &Path) -> Option<IndicePriorsExcel> {
    let ruta = carpeta.join(NOMBRE_INDICE_PRIORS);
    let contenido = fs::read_to_string(&ruta).ok()?;
    serde_json::from_str::<IndicePriorsExcel>(&contenido).ok()
}

pub fn guardar_priors_en(carpeta: &Path, priors: &IndicePriorsExcel) -> Result<(), String> {
    fs::create_dir_all(carpeta)
        .map_err(|e| format!("No se pudo preparar carpeta para índice de priors: {}", e))?;
    let ruta = carpeta.join(NOMBRE_INDICE_PRIORS);
    let json = serde_json::to_string_pretty(priors)
        .map_err(|e| format!("Error serializando índice de priors: {}", e))?;
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = carpeta.join(format!(
        ".{}.{}.{}.tmp",
        NOMBRE_INDICE_PRIORS,
        std::process::id(),
        seq
    ));
    fs::write(&tmp, json)
        .map_err(|e| format!("Error escribiendo índice de priors: {}", e))?;
    if let Err(e) = fs::rename(&tmp, &ruta) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Error guardando índice de priors: {}", e));
    }
    Ok(())
}

pub fn reconstruir_priors_en(carpeta: &Path) -> Result<IndicePriorsExcel, String> {
    let nombres = listar_archivos_en(carpeta)?;
    let mut priors = IndicePriorsExcel {
        formato: FORMATO_PRIORS_ACTUAL,
        actualizada: Local::now().to_rfc3339(),
        ..Default::default()
    };
    for nombre in &nombres {
        let ruta = carpeta.join(nombre);
        if let Ok((registros, _, defensor)) = leer_archivo_en(&ruta) {
            for r in &registros {
                acumular_registro_en_priors_con_defensor(
                    &mut priors,
                    &defensor,
                    &r.radicado,
                    &r.tipo,
                    &r.hora,
                    &r.juzgado,
                    &r.realizada,
                    &r.motivo,
                );
            }
        }
    }
    guardar_priors_en(carpeta, &priors)?;
    Ok(priors)
}

pub fn cargar_o_inicializar_priors_en(carpeta: &Path) -> IndicePriorsExcel {
    if let Some(priors) = cargar_priors_en(carpeta) {
        if priors.formato >= FORMATO_PRIORS_ACTUAL {
            return priors;
        }
    }
    reconstruir_priors_en(carpeta).unwrap_or_default()
}

pub fn actualizar_priors_incremental_con_defensor_en(
    carpeta: &Path,
    defensor: &str,
    anterior: Option<&AudienciaInput>,
    nuevo: Option<&AudienciaInput>,
) -> Result<IndicePriorsExcel, String> {
    let Some(mut priors) = cargar_priors_en(carpeta) else {
        return reconstruir_priors_en(carpeta);
    };
    if priors.formato < FORMATO_PRIORS_ACTUAL {
        return reconstruir_priors_en(carpeta);
    }
    if let Some(ant) = anterior {
        restar_registro_de_priors_con_defensor(&mut priors, defensor, ant);
    }
    if let Some(n) = nuevo {
        acumular_registro_en_priors_con_defensor(
            &mut priors,
            defensor,
            &n.radicado,
            &n.tipo,
            &n.hora,
            &n.juzgado,
            &n.realizada,
            &n.motivo,
        );
    }
    priors.actualizada = Local::now().to_rfc3339();
    guardar_priors_en(carpeta, &priors)?;
    Ok(priors)
}

pub fn actualizar_priors_incremental_en(
    carpeta: &Path,
    anterior: Option<&AudienciaInput>,
    nuevo: Option<&AudienciaInput>,
) -> Result<IndicePriorsExcel, String> {
    actualizar_priors_incremental_con_defensor_en(carpeta, "", anterior, nuevo)
}

pub(crate) fn invalidar_priors_en(carpeta: &Path) {
    let _ = fs::remove_file(carpeta.join(NOMBRE_INDICE_PRIORS));
}

pub fn cargar_priors_actuales() -> IndicePriorsExcel {
    cargar_o_inicializar_priors_en(&obtener_carpeta_archivos())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preparar_temp_dir(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lokus_test_{}_{}", nombre, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ejemplo_audiencia(radicado: &str, fecha: &str, hora: &str, realizada: &str, motivo: &str) -> AudienciaInput {
        AudienciaInput {
            radicado: radicado.into(),
            tipo: "Audiencia concentrada".into(),
            fecha: fecha.into(),
            hora: hora.into(),
            juzgado: "Juzgado 1 Penal".into(),
            realizada: realizada.into(),
            motivo: motivo.into(),
            observaciones: "Obs".into(),
        }
    }

    #[test]
    fn test_radicado_23_digitos_sobrevive_disco_sin_notacion_cientifica() {
        let dir = preparar_temp_dir("radicado_23");
        let ruta = dir.join("radicado.xlsx");
        crear_copia_plantilla_en(&ruta, Some("María Fernanda López")).unwrap();

        let radicado_largo = "11001600000020250000100";
        let input = ejemplo_audiencia(radicado_largo, "15/08/2025", "14:30", "SI", "");
        guardar_nueva_audiencia_en(&ruta, &input, false).unwrap();

        let (registros, _, defensor) = leer_archivo_en(&ruta).unwrap();
        assert_eq!(defensor, "María Fernanda López");
        assert_eq!(registros.len(), 1);
        assert_eq!(registros[0].radicado, radicado_largo);

        let book = reader::xlsx::read(&ruta).unwrap();
        let cell = book.get_sheet(&0).unwrap().get_cell((2, 11)).unwrap();
        assert_eq!(cell.get_data_type(), "s");
        assert_eq!(cell.get_value(), radicado_largo);
        assert_eq!(
            book.get_sheet(&0).unwrap().get_cell((2, 8)).unwrap().get_value(),
            "DEFENSOR PÚBLICO: María Fernanda López"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_plantilla_embebida_ordenamiento_y_totales_columnas() {
        let dir = preparar_temp_dir("totales_orden");
        let ruta = dir.join("test_audiencias.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();

        let a1 = ejemplo_audiencia("11001-2025-001", "10/08/2025", "09:00", "NO", "Defensor Público");
        let a2 = ejemplo_audiencia("11001-2025-002", "15/08/2025", "14:00", "SI", "");

        guardar_nueva_audiencia_en(&ruta, &a1, false).unwrap();
        let totales = guardar_nueva_audiencia_en(&ruta, &a2, false).unwrap();

        assert_eq!(totales.num_registros, 2);
        assert_eq!(totales.total_si, 1);
        assert_eq!(totales.total_no, 1);
        assert_eq!(totales.totales_motivos[7], 1);
        assert_eq!(totales.totales_motivos[0], 0);

        let (registros, _, defensor) = leer_archivo_en(&ruta).unwrap();
        assert_eq!(defensor, "");
        assert_eq!(registros[0].fila_excel, 11);
        assert_eq!(registros[0].radicado, "11001-2025-002");
        assert_eq!(registros[1].fila_excel, 12);
        assert_eq!(registros[1].radicado, "11001-2025-001");
        assert_eq!(registros[1].motivo, "Defensor Público");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_duplicados_y_rotacion_3_respaldos_en_disco() {
        let dir = preparar_temp_dir("backups_dups");
        let ruta = dir.join("agosto.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();

        let a1 = ejemplo_audiencia(
            "JUAN RAmirez - 1212312312312312345231121",
            "10/08/2025",
            "09:00",
            "SI",
            "",
        );
        guardar_nueva_audiencia_en(&ruta, &a1, false).unwrap();

        // Duplicado con error ortográfico en el nombre y mismo radicado/fecha -> rechazado sin override
        let a_dup = ejemplo_audiencia(
            "JuiAN RAmíres - 1212312312312312345231121",
            "10/08/2025",
            "09:00",
            "SI",
            "",
        );
        let err = guardar_nueva_audiencia_en(&ruta, &a_dup, false).unwrap_err();
        assert!(err.starts_with("POSIBLE_DUPLICADO:"));

        // Con permitir_duplicado=true sí guarda, y con 4 escrituras totales solo quedan 3 respaldos
        guardar_nueva_audiencia_en(&ruta, &a_dup, true).unwrap();
        guardar_nueva_audiencia_en(
            &ruta,
            &ejemplo_audiencia("OTRO-003", "12/08/2025", "10:00", "SI", ""),
            false,
        )
        .unwrap();
        guardar_nueva_audiencia_en(
            &ruta,
            &ejemplo_audiencia("OTRO-004", "13/08/2025", "11:00", "SI", ""),
            false,
        )
        .unwrap();

        let respaldos = listar_respaldos_en(&ruta).unwrap();
        assert_eq!(respaldos.len(), 3);

        // El respaldo más antiguo de los 3 (index 2) corresponde al estado con menos de 4 registros
        let respaldo_viejo = respaldos[2].archivo.clone();
        restaurar_respaldo_en(&ruta, &respaldo_viejo).unwrap();
        let (registros_restaurados, _, _) = leer_archivo_en(&ruta).unwrap();
        assert!(registros_restaurados.len() < 4);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_proteccion_fila_rechaza_radicado_modificado() {
        let dir = preparar_temp_dir("row_guard");
        let ruta = dir.join("guard.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();

        let a1 = ejemplo_audiencia("RAD-ORIGINAL", "10/08/2025", "09:00", "SI", "");
        guardar_nueva_audiencia_en(&ruta, &a1, false).unwrap();

        let editado = ejemplo_audiencia("RAD-EDITADO", "10/08/2025", "10:00", "SI", "");
        let err_upd = actualizar_audiencia_en(&ruta, 11, "OTRO-RADICADO", &editado, false).unwrap_err();
        assert!(err_upd.contains("cambió fuera de Lokus"));

        let err_del = eliminar_registro_en(&ruta, 11, "OTRO-RADICADO").unwrap_err();
        assert!(err_del.contains("cambió fuera de Lokus"));

        actualizar_audiencia_en(&ruta, 11, "RAD-ORIGINAL", &editado, false).unwrap();
        let tot = eliminar_registro_en(&ruta, 11, "RAD-EDITADO").unwrap();
        assert_eq!(tot.num_registros, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_seriales_excel_fecha_y_hora() {
        assert_eq!(serial_excel_a_fecha(45678.0).as_deref(), Some("21/01/2025"));
        assert_eq!(serial_excel_a_fecha(45884.75).as_deref(), Some("15/08/2025"));
        assert_eq!(serial_excel_a_hora(0.375).as_deref(), Some("09:00"));
        assert_eq!(serial_excel_a_hora(45678.6041667).as_deref(), Some("14:30"));
    }

    #[test]
    fn test_limite_100_registros() {
        let dir = preparar_temp_dir("limite_100");
        let ruta = dir.join("lleno.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();

        let mut book = reader::xlsx::read(&ruta).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();
        for idx in 0..100u32 {
            let rad = format!("RAD-{:03}", idx + 1);
            let input = ejemplo_audiencia(&rad, "10/08/2025", "09:00", "SI", "");
            escribir_registro_en_fila(ws, FILA_INICIO_DATOS + idx, idx + 1, &input);
        }
        guardar_libro(&mut book, &ruta).unwrap();

        let extra = ejemplo_audiencia("RAD-101", "11/08/2025", "09:00", "SI", "");
        let err = guardar_nueva_audiencia_en(&ruta, &extra, false).unwrap_err();
        assert!(err.contains("límite máximo de 100"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sanitizar_nombre_archivo() {
        assert_eq!(
            sanitizar_nombre_archivo("informe: 08/2025*").unwrap(),
            "informe- 08-2025-.xlsx"
        );
        assert_eq!(
            sanitizar_nombre_archivo("agosto.xlsx").unwrap(),
            "agosto.xlsx"
        );
        assert!(sanitizar_nombre_archivo("   ").is_err());
        assert!(sanitizar_nombre_archivo("CON").is_err());
        assert!(ruta_archivo("../secreto.xlsx").is_err());
    }

    #[test]
    fn test_rutas_con_subcarpetas_seguras() {
        let ok = ruta_archivo("2026/Enero/agosto").unwrap();
        assert!(ok.ends_with("2026/Enero/agosto.xlsx"));
        assert!(ruta_archivo("/etc/pasaporte.xlsx").is_err());
        assert!(ruta_archivo("2026/../../x.xlsx").is_err());
        assert!(ruta_archivo("2026\\Enero\\x.xlsx").is_err());
        assert!(ruta_archivo(".respaldos/x.xlsx").is_err());
        assert!(ruta_carpeta("2026//Enero").is_err());

        assert_eq!(sanitizar_ruta_carpeta("2026 / Enero").unwrap(), "2026/Enero");
        assert_eq!(sanitizar_ruta_carpeta("Año: 2026*").unwrap(), "Año- 2026-");
        assert!(sanitizar_ruta_carpeta("  ").is_err());
    }

    #[test]
    fn test_listado_recursivo_ignora_ocultos_y_respaldos() {
        let dir = preparar_temp_dir("listado_recursivo");
        fs::create_dir_all(dir.join("2026/Enero")).unwrap();
        fs::create_dir_all(dir.join("Vacia")).unwrap();
        fs::create_dir_all(dir.join(".respaldos/x")).unwrap();
        crear_copia_plantilla_en(&dir.join("raiz.xlsx"), None).unwrap();
        crear_copia_plantilla_en(&dir.join("2026/Enero/a.xlsx"), None).unwrap();
        fs::write(dir.join("2026/~$a.xlsx"), b"tmp").unwrap();
        fs::write(dir.join(".respaldos/x/old.xlsx"), b"bak").unwrap();

        let (archivos, carpetas) = explorar_carpeta_en(&dir).unwrap();
        assert_eq!(archivos, vec!["2026/Enero/a.xlsx", "raiz.xlsx"]);
        assert_eq!(carpetas, vec!["2026", "2026/Enero", "Vacia"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_se_invalida_al_guardar() {
        let dir = preparar_temp_dir("cache_planilla");
        let ruta = dir.join("cache.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();
        assert_eq!(leer_archivo_en(&ruta).unwrap().0.len(), 0);
        assert_eq!(leer_archivo_en(&ruta).unwrap().0.len(), 0);

        let input = ejemplo_audiencia("PEDRO LOPEZ - 123456789", "15/08/2025", "09:00", "SI", "");
        guardar_nueva_audiencia_en(&ruta, &input, false).unwrap();
        assert_eq!(leer_archivo_en(&ruta).unwrap().0.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_estadisticas_globales_por_mes_y_defensor() {
        let dir = preparar_temp_dir("stats_globales");
        let p1 = dir.join("paula.xlsx");
        let p2 = dir.join("carlos.xlsx");
        let p3 = dir.join("libre.xlsx");

        crear_copia_plantilla_en(&p1, Some("Dra. Paula Ríos")).unwrap();
        crear_copia_plantilla_en(&p2, Some("Dr. Carlos Mendoza")).unwrap();
        crear_copia_plantilla_en(&p3, None).unwrap();

        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-1", "10/08/2025", "09:00", "SI", ""), false).unwrap();
        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-2", "11/08/2025", "10:00", "NO", "Fiscalía"), false).unwrap();
        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-3", "20/07/2025", "11:00", "SI", ""), false).unwrap();

        guardar_nueva_audiencia_en(&p2, &ejemplo_audiencia("R-4", "15/08/2025", "14:00", "NO", "INPEC"), false).unwrap();
        guardar_nueva_audiencia_en(&p3, &ejemplo_audiencia("R-5", "18/08/2025", "15:00", "SI", ""), false).unwrap();

        // Todo el historial sin filtros
        let todo = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(todo.total_planillas, 3);
        assert_eq!(todo.total_planillas_con_datos, 3);
        assert_eq!(todo.total_audiencias, 5);
        assert_eq!(todo.total_si, 3);
        assert_eq!(todo.total_no, 2);
        assert_eq!(todo.efectividad_pct, 60.0);
        assert_eq!(todo.totales_motivos[1], 1); // Fiscalía
        assert_eq!(todo.totales_motivos[3], 1); // INPEC
        assert_eq!(todo.meses_disponibles.len(), 2);
        assert_eq!(todo.meses_disponibles[0].etiqueta, "Agosto 2025");
        assert_eq!(todo.meses_disponibles[0].cantidad, 4);
        assert_eq!(todo.meses_disponibles[1].etiqueta, "Julio 2025");
        assert_eq!(todo.meses_disponibles[1].cantidad, 1);
        assert_eq!(todo.defensores_disponibles, vec!["Dr. Carlos Mendoza", "Dra. Paula Ríos"]);
        assert_eq!(todo.por_defensor[0].defensor, "Dra. Paula Ríos");
        assert_eq!(todo.por_defensor[0].total, 3);
        assert_eq!(todo.por_defensor[0].motivo_frecuente, "Fiscalía (1)");

        // Filtro por Agosto 2025
        let ago = calcular_estadisticas_en(&dir, Some(2025), Some(8), None, None).unwrap();
        assert_eq!(ago.total_audiencias, 4);
        assert_eq!(ago.total_si, 2);
        assert_eq!(ago.total_no, 2);
        assert_eq!(ago.por_defensor.len(), 3);

        // Filtro por archivo específico (conservando lista completa de planillas y defensores)
        let solo_paula = calcular_estadisticas_en(&dir, Some(2025), Some(8), Some("paula.xlsx"), None).unwrap();
        assert_eq!(solo_paula.total_audiencias, 2);
        assert_eq!(solo_paula.planillas.len(), 3);
        assert_eq!(solo_paula.por_defensor.len(), 1);
        assert_eq!(solo_paula.por_defensor[0].defensor, "Dra. Paula Ríos");

        // Filtro por defensor directo (todos los meses) y por planillas sin asignar
        let por_carlos = calcular_estadisticas_en(&dir, None, None, None, Some("dr. carlos mendoza")).unwrap();
        assert_eq!(por_carlos.total_audiencias, 1);
        assert_eq!(por_carlos.total_no, 1);
        assert_eq!(por_carlos.por_defensor[0].defensor, "Dr. Carlos Mendoza");
        assert_eq!(por_carlos.meses_disponibles.len(), 1);

        let sin_asignar = calcular_estadisticas_en(&dir, None, None, None, Some("__SIN_ASIGNAR__")).unwrap();
        assert_eq!(sin_asignar.total_audiencias, 1);
        assert_eq!(sin_asignar.total_si, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_archivar_estadisticas_y_evitar_duplicados() {
        let dir = preparar_temp_dir("stats_archivadas");
        let p1 = dir.join("2026/enero.xlsx");
        let p2 = dir.join("febrero.xlsx");

        crear_copia_plantilla_en(&p1, Some("Dra. Paula Ríos")).unwrap();
        crear_copia_plantilla_en(&p2, Some("Dr. Carlos Mendoza")).unwrap();

        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-1", "10/01/2026", "09:00", "SI", ""), false).unwrap();
        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-2", "11/01/2026", "10:00", "NO", "Juez"), false).unwrap();
        // También una audiencia sin fecha (opcional)
        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-3", "", "", "SI", ""), false).unwrap();
        guardar_nueva_audiencia_en(&p2, &ejemplo_audiencia("R-4", "15/02/2026", "11:00", "NO", "Fiscalía"), false).unwrap();

        // Primer cálculo asigna IDs estables a p1 y p2
        let antes = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(antes.total_audiencias, 4);
        assert_eq!(antes.total_archivadas, 0);
        let id_p1 = antes
            .planillas
            .iter()
            .find(|p| p.archivo == "2026/enero.xlsx")
            .unwrap()
            .id
            .clone();

        // Eliminar 2026/enero.xlsx conservando sus estadísticas
        eliminar_archivo_en(&dir, "2026/enero.xlsx", true).unwrap();
        assert!(!p1.exists());

        let despues = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(despues.total_audiencias, 4);
        assert_eq!(despues.total_si, 2);
        assert_eq!(despues.total_no, 2);
        assert_eq!(despues.total_archivadas, 1);
        assert_eq!(despues.totales_motivos[0], 1); // Juez conservado
        assert_eq!(despues.totales_motivos[1], 1); // Fiscalía conservado

        // Filtro por Enero 2026 sigue encontrando las 2 audiencias con fecha de enero (sin incluir la sin fecha)
        let enero = calcular_estadisticas_en(&dir, Some(2026), Some(1), None, None).unwrap();
        assert_eq!(enero.total_audiencias, 2);
        assert_eq!(enero.total_si, 1);
        assert_eq!(enero.total_no, 1);

        // Si el archivo vuelve a estar en disco con el MISMO ID, nunca se duplica el conteo
        crear_copia_plantilla_en(&p1, Some("Dra. Paula Ríos")).unwrap();
        guardar_nueva_audiencia_en(&p1, &ejemplo_audiencia("R-1", "10/01/2026", "09:00", "SI", ""), false).unwrap();
        let mut reg = cargar_registro_estadisticas_en(&dir);
        reg.ids_por_ruta.insert("2026/enero.xlsx".to_string(), id_p1.clone());
        guardar_registro_estadisticas_en(&dir, &reg).unwrap();

        let sin_duplicar = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(sin_duplicar.total_audiencias, 2); // 1 de p1 vivo + 1 de p2 vivo (ignora el resumen archivado del mismo ID)
        assert_eq!(sin_duplicar.total_archivadas, 0);

        // Borrar el archivo sin conservar y quitar resumen archivado
        eliminar_archivo_en(&dir, "2026/enero.xlsx", false).unwrap();
        let con_archivado = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(con_archivado.total_archivadas, 1);
        eliminar_estadistica_archivada_en(&dir, &id_p1).unwrap();
        let limpio = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(limpio.total_archivadas, 0);
        assert_eq!(limpio.total_audiencias, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_eliminar_carpeta_llena_con_y_sin_conservar_estadisticas() {
        let dir = preparar_temp_dir("eliminar_carpeta_llena");
        let sub = dir.join("2026/Enero");
        fs::create_dir_all(&sub).unwrap();
        let p = sub.join("audiencias.xlsx");
        crear_copia_plantilla_en(&p, Some("Dra. Laura Gómez")).unwrap();
        guardar_nueva_audiencia_en(&p, &ejemplo_audiencia("R-1", "10/01/2026", "09:00", "SI", ""), false).unwrap();
        guardar_nueva_audiencia_en(&p, &ejemplo_audiencia("R-2", "11/01/2026", "10:00", "NO", "INPEC"), false).unwrap();

        // Sin confirmar eliminar_contenido -> debe fallar
        assert!(eliminar_carpeta_con_opciones_en(&dir, "2026", false, false).is_err());
        assert!(p.exists());

        // Con confirmar + conservar_estadisticas -> borra carpeta pero conserva estadísticas
        eliminar_carpeta_con_opciones_en(&dir, "2026", true, true).unwrap();
        assert!(!dir.join("2026").exists());

        let stats = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        assert_eq!(stats.total_audiencias, 2);
        assert_eq!(stats.total_si, 1);
        assert_eq!(stats.total_no, 1);
        assert_eq!(stats.total_archivadas, 1);
        assert_eq!(stats.totales_motivos[3], 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_colores_carpetas_crud_y_limpieza() {
        let dir = preparar_temp_dir("colores_carpetas");
        crear_carpeta_con_color_en(&dir, "2026", Some("azul")).unwrap();
        crear_carpeta_con_color_en(&dir, "2026/Enero", Some("verde")).unwrap();
        assert!(guardar_color_carpeta_en(&dir, "2026", "neon-invalido").is_err());

        let colores = obtener_colores_carpetas_en(&dir);
        assert_eq!(colores.get("2026").map(String::as_str), Some("azul"));
        assert_eq!(colores.get("2026/Enero").map(String::as_str), Some("verde"));

        fs::rename(dir.join("2026"), dir.join("2027")).unwrap();
        actualizar_prefijo_carpeta_indice(&dir, "2026", "2027").unwrap();
        let renombrados = obtener_colores_carpetas_en(&dir);
        assert!(!renombrados.contains_key("2026"));
        assert_eq!(renombrados.get("2027").map(String::as_str), Some("azul"));
        assert_eq!(
            renombrados.get("2027/Enero").map(String::as_str),
            Some("verde")
        );

        assert!(guardar_color_carpeta_en(&dir, "2026/Enero", "").is_err());
        let tras_limpiar = guardar_color_carpeta_en(&dir, "2027/Enero", "").unwrap();
        assert!(!tras_limpiar.contains_key("2027/Enero"));
        assert_eq!(tras_limpiar.get("2027").map(String::as_str), Some("azul"));

        eliminar_carpeta_con_opciones_en(&dir, "2027", true, false).unwrap();
        assert!(obtener_colores_carpetas_en(&dir).is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_priors_excel_reconstruccion_y_actualizacion_incremental() {
        let dir = preparar_temp_dir("priors_incremental");
        let sub = dir.join("2025/Agosto");
        fs::create_dir_all(&sub).unwrap();
        let ruta = sub.join("agosto.xlsx");
        crear_copia_plantilla_en(&ruta, None).unwrap();

        let mut reg1 = ejemplo_audiencia(
            "JUAN PEREZ - 200016001075202500123",
            "15/08/2025",
            "14:30",
            "NO",
            "Fiscalía",
        );
        reg1.juzgado = "Juzgado 3 Cto Penal".into();
        reg1.tipo = "Audiencia concentrada".into();
        guardar_nueva_audiencia_en(&ruta, &reg1, false).unwrap();

        let priors_init = cargar_o_inicializar_priors_en(&dir);
        assert_eq!(priors_init.total_registros, 1);
        assert_eq!(priors_init.realizada_no, 1);
        assert_eq!(priors_init.motivos_freq[1], 1);
        assert_eq!(priors_init.horas_freq.get(&14).copied(), Some(1));
        assert_eq!(
            priors_init
                .juzgados_freq
                .get("Juzgado 3 Cto Penal")
                .copied(),
            Some(1)
        );
        assert!(priors_init
            .procesos_por_numero
            .contains_key("200016001075202500123"));
        assert!(dir.join(NOMBRE_INDICE_PRIORS).exists());

        // Actualización incremental: cambia a SI / 09:00 / otro juzgado
        let mut reg2 = reg1.clone();
        reg2.hora = "09:00".into();
        reg2.juzgado = "Juzgado 1 Penal".into();
        reg2.realizada = "SI".into();
        reg2.motivo = String::new();
        let priors_upd =
            actualizar_priors_incremental_en(&dir, Some(&reg1), Some(&reg2)).unwrap();
        assert_eq!(priors_upd.total_registros, 1);
        assert_eq!(priors_upd.realizada_no, 0);
        assert_eq!(priors_upd.realizada_si, 1);
        assert_eq!(priors_upd.motivos_freq[1], 0);
        assert!(!priors_upd.horas_freq.contains_key(&14));
        assert_eq!(priors_upd.horas_freq.get(&9).copied(), Some(1));
        assert_eq!(
            priors_upd.juzgados_freq.get("Juzgado 1 Penal").copied(),
            Some(1)
        );
        assert!(!priors_upd.juzgados_freq.contains_key("Juzgado 3 Cto Penal"));

        // Eliminación incremental limpia contadores y proceso
        let priors_del = actualizar_priors_incremental_en(&dir, Some(&reg2), None).unwrap();
        assert_eq!(priors_del.total_registros, 0);
        assert_eq!(priors_del.realizada_si, 0);
        assert!(priors_del.procesos_por_numero.is_empty());

        // Al eliminar el archivo Excel completo, .lokus_priors.json se invalida y se reconstruye limpio
        let ruta2 = dir.join("otro.xlsx");
        crear_copia_plantilla_en(&ruta2, None).unwrap();
        let mut reg_otro = reg1.clone();
        reg_otro.radicado = "MARIA LOPEZ - 200016001075202500999".into();
        guardar_nueva_audiencia_en(&ruta2, &reg_otro, false).unwrap();
        let p_con_otro = actualizar_priors_incremental_en(&dir, None, Some(&reg_otro)).unwrap();
        assert_eq!(p_con_otro.total_registros, 1);
        assert!(p_con_otro
            .procesos_por_numero
            .contains_key("200016001075202500999"));
        assert!(dir.join(NOMBRE_INDICE_PRIORS).exists());
        eliminar_archivo_en(&dir, "otro.xlsx", false).unwrap();
        assert!(!dir.join(NOMBRE_INDICE_PRIORS).exists());
        let p_reconstruido = cargar_o_inicializar_priors_en(&dir);
        assert!(!p_reconstruido
            .procesos_por_numero
            .contains_key("200016001075202500999"));
        assert_eq!(p_reconstruido.total_registros, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_priors_v2_grafo_asociaciones_migracion_y_actualizacion_incremental_por_defensor() {
        let dir = preparar_temp_dir("priors_v2_grafo");
        let ruta_a = dir.join("planilla_ana.xlsx");
        let ruta_b = dir.join("planilla_luis.xlsx");
        crear_copia_plantilla_en(&ruta_a, Some("Dra. Ana Pérez")).unwrap();
        crear_copia_plantilla_en(&ruta_b, Some("Dr. Luis Mora")).unwrap();

        let reg_a = AudienciaInput {
            radicado: "JUAN DIAZ - 200016001075202500111".into(),
            tipo: "Audiencia de control de legalidad".into(),
            fecha: "10/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 4 Penal Municipal de Valledupar".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        let reg_b = AudienciaInput {
            radicado: "MARIA SOL - 470016001075202500222".into(),
            tipo: "Audiencia de juicio oral".into(),
            fecha: "11/08/2025".into(),
            hora: "14:00".into(),
            juzgado: "Juzgado 4 Penal del Circuito de Santa Marta".into(),
            realizada: "NO".into(),
            motivo: "Fiscalía".into(),
            observaciones: String::new(),
        };
        guardar_nueva_audiencia_en(&ruta_a, &reg_a, false).unwrap();
        guardar_nueva_audiencia_en(&ruta_b, &reg_b, false).unwrap();

        // Simular archivo .lokus_priors.json en formato v1 antiguo -> debe reconstruir automáticamente a v2 con grafo
        let priors_v1 = IndicePriorsExcel {
            formato: 1,
            actualizada: "2025-08-01T10:00:00-05:00".into(),
            total_registros: 1,
            ..Default::default()
        };
        guardar_priors_en(&dir, &priors_v1).unwrap();

        let p = cargar_o_inicializar_priors_en(&dir);
        assert_eq!(p.formato, FORMATO_PRIORS_ACTUAL);
        assert_eq!(p.total_registros, 2);
        assert_eq!(
            p.procesos_por_numero
                .get("200016001075202500111")
                .map(|pr| pr.defensor_habitual.as_str()),
            Some("Dra. Ana Pérez")
        );
        assert_eq!(
            p.grafo
                .defensor_sigla_a_juzgado
                .get("ana perez")
                .and_then(|m| m.get("j4"))
                .and_then(|d| d.get("Juzgado 4 Penal Municipal de Valledupar"))
                .copied(),
            Some(1)
        );
        assert_eq!(
            p.grafo
                .defensor_sigla_a_juzgado
                .get("luis mora")
                .and_then(|m| m.get("j4"))
                .and_then(|d| d.get("Juzgado 4 Penal del Circuito de Santa Marta"))
                .copied(),
            Some(1)
        );
        assert_eq!(
            p.grafo
                .prefijo_radicado_a_juzgados
                .get("20001")
                .and_then(|m| m.get("Juzgado 4 Penal Municipal de Valledupar"))
                .copied(),
            Some(1)
        );
        assert_eq!(
            p.grafo
                .prefijo_radicado_a_juzgados
                .get("47001")
                .and_then(|m| m.get("Juzgado 4 Penal del Circuito de Santa Marta"))
                .copied(),
            Some(1)
        );
        assert!(p.grafo.total_conexiones() >= 10);

        // Actualización incremental: editar el juzgado de Ana debe decrementar el viejo y agregar el nuevo
        let mut reg_a_edit = reg_a.clone();
        reg_a_edit.juzgado = "Juzgado 10 Penal Municipal de Valledupar".into();
        let p_edit = actualizar_priors_incremental_con_defensor_en(
            &dir,
            "Dra. Ana Pérez",
            Some(&reg_a),
            Some(&reg_a_edit),
        )
        .unwrap();
        assert!(p_edit
            .grafo
            .defensor_sigla_a_juzgado
            .get("ana perez")
            .and_then(|m| m.get("j4"))
            .is_none());
        assert_eq!(
            p_edit
                .grafo
                .defensor_sigla_a_juzgado
                .get("ana perez")
                .and_then(|m| m.get("j10"))
                .and_then(|d| d.get("Juzgado 10 Penal Municipal de Valledupar"))
                .copied(),
            Some(1)
        );

        // Simular archivo .lokus_priors.json en formato v2 (con prefijos honoríficos) -> debe reconstruir automáticamente a v3
        let priors_v2 = IndicePriorsExcel {
            formato: 2,
            actualizada: "2025-08-01T10:00:00-05:00".into(),
            total_registros: 1,
            ..Default::default()
        };
        guardar_priors_en(&dir, &priors_v2).unwrap();
        let p_mig_v3 = cargar_o_inicializar_priors_en(&dir);
        assert_eq!(p_mig_v3.formato, FORMATO_PRIORS_ACTUAL);
        assert!(p_mig_v3.grafo.defensor_a_juzgados.contains_key("ana perez"));

        // Cambiar el defensor en la planilla + invalidar priors debe reflejarse al reconstruir
        actualizar_defensor_en(&ruta_a, "Dra. Sofía Ruiz").unwrap();
        invalidar_priors_en(&dir);
        let p_nuevo_def = cargar_o_inicializar_priors_en(&dir);
        assert!(p_nuevo_def
            .grafo
            .defensor_a_juzgados
            .contains_key("sofia ruiz"));
        assert!(!p_nuevo_def
            .grafo
            .defensor_a_juzgados
            .contains_key("ana perez"));

        // Calcular estadísticas globales no debe degradar .lokus_priors.json ni borrar el grafo por defensor
        calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        let p_post_stats = cargar_priors_en(&dir).unwrap();
        assert_eq!(p_post_stats.formato, FORMATO_PRIORS_ACTUAL);
        assert!(p_post_stats.grafo.defensor_a_juzgados.contains_key("sofia ruiz"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_nueva_plantilla_sin_printer_settings_ni_abspath_conserva_logo_y_totales() {
        let dir = preparar_temp_dir("nueva_plantilla_limpia");
        let ruta = dir.join("Laura_Gomez_Noviembre_2026.xlsx");
        crear_copia_plantilla_en(&ruta, Some("Laura Gómez")).unwrap();

        let a1 = ejemplo_audiencia("20001-2026-001", "20/11/2026", "09:00", "SI", "");
        let a2 = ejemplo_audiencia("20001-2026-002", "21/11/2026", "14:30", "NO", "Fiscalía");
        guardar_nueva_audiencia_en(&ruta, &a1, false).unwrap();
        let tot = guardar_nueva_audiencia_en(&ruta, &a2, false).unwrap();
        assert_eq!(tot.num_registros, 2);
        assert_eq!(tot.total_si, 1);
        assert_eq!(tot.total_no, 1);

        let bytes = fs::read(&ruta).unwrap();
        assert!(!contiene_printer_settings_zip(&ruta), "No debe contener xl/printerSettings/");
        assert!(
            !bytes.windows(b"x15ac:absPath".len()).any(|w| w == b"x15ac:absPath"),
            "No debe contener x15ac:absPath"
        );
        assert!(
            bytes.windows(b"xl/drawings/drawing1.xml".len()).any(|w| w == b"xl/drawings/drawing1.xml"),
            "Debe conservar drawing1.xml"
        );
        assert!(
            bytes.windows(b"xl/media/image1.png".len()).any(|w| w == b"xl/media/image1.png"),
            "Debe conservar image1.png"
        );

        let book = reader::xlsx::read(&ruta).unwrap();
        let ws = book.get_sheet(&0).unwrap();
        let ps = ws.get_page_setup();
        assert!(ps.get_object_data().is_none());
        assert_eq!(*ps.get_scale(), 33);
        assert!(matches!(ps.get_orientation(), OrientationValues::Portrait));
        assert_eq!(*ps.get_horizontal_dpi(), 0);
        assert_eq!(*ps.get_vertical_dpi(), 0);
        assert_eq!(*ps.get_paper_size(), 0);
        assert_eq!(*ps.get_fit_to_width(), 0);
        assert_eq!(*ps.get_fit_to_height(), 0);
        assert!(ws
            .get_sheets_views()
            .get_sheet_view_list()
            .iter()
            .all(|v| matches!(v.get_view(), SheetViewValues::Normal)));
        assert_eq!(ws.get_cell((2, FILA_DEFENSOR)).unwrap().get_value(), "DEFENSOR PÚBLICO: Laura Gómez");
        assert_eq!(ws.get_cell((7, FILA_TOTALES)).unwrap().get_value(), "1");
        assert_eq!(ws.get_cell((10, FILA_TOTALES)).unwrap().get_value(), "1");
        assert_eq!(ws.get_cell((17, FILA_TOTALES)).unwrap().get_value(), "1");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_descargar_archivo_sanea_printer_settings_de_planilla_antigua() {
        let dir = preparar_temp_dir("descargar_saneado");
        let descargas = dir.join("Descargas");
        let ruta = dir.join("planilla_vieja.xlsx");

        // Simular una planilla antigua guardada antes de la limpieza de printerSettings
        // Simular una planilla antigua guardada antes de la limpieza de printerSettings
        let mut book =
            reader::xlsx::read_reader(std::io::Cursor::new(PLANTILLA_BYTES), true).unwrap();
        let mut ps = PageSetup::default();
        ps.set_scale(33);
        ps.set_orientation(OrientationValues::Portrait);
        ps.set_object_data(vec![1u8, 2, 3, 4, 5]);
        {
            let ws = book.get_sheet_mut(&0).unwrap();
            ws.set_page_setup(ps);
            escribir_defensor_en_hoja(ws, "Carlos Mendoza");
            escribir_registro_en_fila(
                ws,
                FILA_INICIO_DATOS,
                1,
                &ejemplo_audiencia("20001-2026-010", "22/11/2026", "10:00", "SI", ""),
            );
        }
        writer::xlsx::write(&book, &ruta).unwrap();
        assert!(contiene_printer_settings_zip(&ruta));

        let exportado = descargar_archivo_en(&ruta, &descargas).unwrap();
        let ruta_exp = Path::new(&exportado);
        assert!(ruta_exp.exists());
        assert!(
            !contiene_printer_settings_zip(ruta_exp),
            "La copia exportada debe quedar libre de xl/printerSettings/"
        );

        let (regs, _, def) = leer_archivo_en(ruta_exp).unwrap();
        assert_eq!(def, "Carlos Mendoza");
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].radicado, "20001-2026-010");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_renombrar_archivo_misma_carpeta_moviendo_y_conservando_respaldos_e_indice() {
        let dir = preparar_temp_dir("renombrar_archivo");
        let sub = dir.join("2026/Enero");
        fs::create_dir_all(&sub).unwrap();
        let orig = sub.join("orig.xlsx");
        let otro = sub.join("ocupado.xlsx");
        crear_copia_plantilla_en(&orig, Some("Laura Gómez")).unwrap();
        crear_copia_plantilla_en(&otro, Some("Laura Gómez")).unwrap();
        guardar_nueva_audiencia_en(
            &orig,
            &ejemplo_audiencia("RAD-REN-1", "10/11/2026", "09:00", "SI", ""),
            false,
        )
        .unwrap();
        assert_eq!(listar_respaldos_en(&orig).unwrap().len(), 1);

        let stats_ini = calcular_estadisticas_en(&dir, None, None, None, None).unwrap();
        let id_orig = stats_ini
            .planillas
            .iter()
            .find(|p| p.archivo == "2026/Enero/orig.xlsx")
            .unwrap()
            .id
            .clone();

        // Colisión y nombres inválidos deben rechazarse
        assert!(renombrar_archivo_en(&dir, "2026/Enero/orig.xlsx", "ocupado", None).is_err());
        assert!(renombrar_archivo_en(&dir, "2026/Enero/orig.xlsx", "CON", None).is_err());
        assert!(renombrar_archivo_en(&dir, "2026/Enero/orig.xlsx", "   ", None).is_err());

        // Renombrar en la misma carpeta
        let r1 =
            renombrar_archivo_en(&dir, "2026/Enero/orig.xlsx", "Laura_Noviembre_2026", None)
                .unwrap();
        assert_eq!(r1, "2026/Enero/Laura_Noviembre_2026.xlsx");
        assert!(!orig.exists());
        let p1 = dir.join(&r1);
        assert!(p1.is_file());
        assert_eq!(listar_respaldos_en(&p1).unwrap().len(), 1);

        let reg_mid = cargar_registro_estadisticas_en(&dir);
        assert_eq!(reg_mid.ids_por_ruta.get(&r1), Some(&id_orig));
        assert!(!reg_mid.ids_por_ruta.contains_key("2026/Enero/orig.xlsx"));

        // Cambio solo de mayúsculas/minúsculas
        let r_case = renombrar_archivo_en(&dir, &r1, "laura_noviembre_2026", None).unwrap();
        assert_eq!(r_case, "2026/Enero/laura_noviembre_2026.xlsx");
        assert_eq!(listar_respaldos_en(&dir.join(&r_case)).unwrap().len(), 1);

        // Renombrar + mover a otra subcarpeta
        let r2 = renombrar_archivo_en(
            &dir,
            &r_case,
            "Planilla_Final",
            Some("2026/Febrero"),
        )
        .unwrap();
        assert_eq!(r2, "2026/Febrero/Planilla_Final.xlsx");
        assert!(dir.join(&r2).is_file());
        assert_eq!(listar_respaldos_en(&dir.join(&r2)).unwrap().len(), 1);

        // Renombrar + mover a la raíz
        let r3 = renombrar_archivo_en(&dir, &r2, "En_Raiz", Some("")).unwrap();
        assert_eq!(r3, "En_Raiz.xlsx");
        let p3 = dir.join(&r3);
        assert!(p3.is_file());
        assert_eq!(listar_respaldos_en(&p3).unwrap().len(), 1);

        let (regs, _, def) = leer_archivo_en(&p3).unwrap();
        assert_eq!(def, "Laura Gómez");
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].radicado, "RAD-REN-1");

        let reg_fin = cargar_registro_estadisticas_en(&dir);
        assert_eq!(reg_fin.ids_por_ruta.get(&r3), Some(&id_orig));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_descargar_y_sanear_elimina_dpi_papersize_y_page_layout_sin_printer_bin() {
        let dir = preparar_temp_dir("sanear_dpi_papersize");
        let descargas = dir.join("Descargas");
        let ruta = dir.join("planilla_sin_bin_con_dpi.xlsx");

        // Simular planilla sin xl/printerSettings/*.bin pero con atributos de impresora
        // heredados de ONLYOFFICE/Excel (horizontalDpi, paperSize, fitToWidth, PageLayout).
        let mut book =
            reader::xlsx::read_reader(std::io::Cursor::new(PLANTILLA_BYTES), true).unwrap();
        let mut ps = PageSetup::default();
        ps.set_paper_size(9);
        ps.set_scale(33);
        ps.set_orientation(OrientationValues::Portrait);
        ps.set_fit_to_width(1);
        ps.set_fit_to_height(1);
        ps.set_horizontal_dpi(600);
        ps.set_vertical_dpi(600);
        {
            let ws = book.get_sheet_mut(&0).unwrap();
            ws.set_page_setup(ps);
            let mut views = ws.get_sheets_views().clone();
            for sv in views.get_sheet_view_list_mut() {
                sv.set_view(SheetViewValues::PageLayout);
            }
            ws.set_sheets_views(views);
            escribir_defensor_en_hoja(ws, "Carlos Mendoza");
        }
        writer::xlsx::write(&book, &ruta).unwrap();
        assert!(!contiene_printer_settings_zip(&ruta));

        let exportado = descargar_archivo_en(&ruta, &descargas).unwrap();
        let ruta_exp = Path::new(&exportado);
        let book_exp = reader::xlsx::read(ruta_exp).unwrap();
        let ws_exp = book_exp.get_sheet(&0).unwrap();
        let ps_exp = ws_exp.get_page_setup();
        assert_eq!(*ps_exp.get_horizontal_dpi(), 0);
        assert_eq!(*ps_exp.get_vertical_dpi(), 0);
        assert_eq!(*ps_exp.get_paper_size(), 0);
        assert_eq!(*ps_exp.get_fit_to_width(), 0);
        assert_eq!(*ps_exp.get_fit_to_height(), 0);
        assert_eq!(*ps_exp.get_scale(), 33);
        assert!(matches!(ps_exp.get_orientation(), OrientationValues::Portrait));
        assert!(ws_exp
            .get_sheets_views()
            .get_sheet_view_list()
            .iter()
            .all(|v| matches!(v.get_view(), SheetViewValues::Normal)));

        // También sanear in-place (ruta usada al importar paquetes .lokus)
        sanear_archivo_excel_en(&ruta);
        let book_in = reader::xlsx::read(&ruta).unwrap();
        let ps_in = book_in.get_sheet(&0).unwrap().get_page_setup();
        assert_eq!(*ps_in.get_horizontal_dpi(), 0);
        assert_eq!(*ps_in.get_paper_size(), 0);
        assert!(book_in
            .get_sheet(&0)
            .unwrap()
            .get_sheets_views()
            .get_sheet_view_list()
            .iter()
            .all(|v| matches!(v.get_view(), SheetViewValues::Normal)));

        let _ = fs::remove_dir_all(&dir);
    }
}
