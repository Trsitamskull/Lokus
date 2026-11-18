use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use umya_spreadsheet::{reader, writer, Spreadsheet, Worksheet};

use crate::models::{
    buscar_posible_duplicado, mapear_motivo_a_indice, AudienciaInput, EstadisticasGlobales,
    MesDisponible, PlanillaResumen, RegistroAudiencia, ResumenDefensor, TotalesExcel,
    MOTIVOS_LABELS,
};

const PLANTILLA_BYTES: &[u8] = include_bytes!("../../templates/plantilla_audiencias.xlsx");

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
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Lokus")
        .join("archivos_creados");
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

pub fn ruta_archivo(nombre_archivo: &str) -> Result<PathBuf, String> {
    let trimmed = nombre_archivo.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err("Nombre de archivo inválido.".to_string());
    }
    let nombre_final = if trimmed.to_lowercase().ends_with(".xlsx") {
        trimmed.to_string()
    } else {
        format!("{}.xlsx", trimmed)
    };
    Ok(obtener_carpeta_archivos().join(nombre_final))
}

pub fn listar_archivos_en(carpeta: &Path) -> Result<Vec<String>, String> {
    fs::create_dir_all(carpeta).map_err(|e| format!("Error creando carpeta: {}", e))?;

    let mut archivos: Vec<String> = fs::read_dir(carpeta)
        .map_err(|e| format!("Error leyendo carpeta: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            if name.to_lowercase().ends_with(".xlsx") && !name.starts_with("~$") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    archivos.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok(archivos)
}

pub fn listar_archivos() -> Result<Vec<String>, String> {
    listar_archivos_en(&obtener_carpeta_archivos())
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

    if let Some(def) = defensor {
        let limpio = def.trim();
        if !limpio.is_empty() {
            let mut book = reader::xlsx::read(destino)
                .map_err(|e| format!("Error al preparar defensor en plantilla: {}", e))?;
            let ws = book
                .get_sheet_mut(&0)
                .ok_or_else(|| "No se pudo acceder a la hoja principal".to_string())?;
            escribir_defensor_en_hoja(ws, limpio);
            guardar_libro(&book, destino)?;
        }
    }

    Ok(())
}

pub fn crear_copia_plantilla(nombre_archivo: &str, defensor: Option<&str>) -> Result<String, String> {
    let nombre_sanitizado = sanitizar_nombre_archivo(nombre_archivo)?;
    let destino = ruta_archivo(&nombre_sanitizado)?;
    crear_copia_plantilla_en(&destino, defensor)?;
    Ok(nombre_sanitizado)
}

pub fn eliminar_archivo(nombre_archivo: &str) -> Result<(), String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    if !ruta.exists() {
        return Err("El archivo no existe.".to_string());
    }
    fs::remove_file(&ruta).map_err(|e| format!("No se pudo eliminar el archivo: {}", e))
}

/// Carpeta de Descargas del usuario (o su carpeta personal si no existe) para exportaciones.
pub fn carpeta_descargas() -> Result<PathBuf, String> {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "No se pudo localizar la carpeta de Descargas".to_string())
}

pub fn descargar_archivo(nombre_archivo: &str) -> Result<String, String> {
    let origen = ruta_archivo(nombre_archivo)?;
    if !origen.exists() {
        return Err("Archivo no encontrado.".to_string());
    }

    let carpeta_descargas = carpeta_descargas()?;

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

    fs::copy(&origen, &destino)
        .map_err(|e| format!("No se pudo copiar el archivo a Descargas: {}", e))?;

    Ok(destino.to_string_lossy().to_string())
}

fn carpeta_respaldos_de(ruta_excel: &Path) -> PathBuf {
    let parent = ruta_excel.parent().unwrap_or_else(|| Path::new("."));
    let stem = ruta_excel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("planilla");
    parent.join(".respaldos").join(stem)
}

fn crear_respaldo_en(ruta_excel: &Path) -> Result<(), String> {
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

    // Validar que el respaldo sea un Excel legible y copiarlo primero a un archivo temporal
    // antes de rotar respaldos (para no eliminar el propio respaldo elegido si era el más antiguo).
    let _ = reader::xlsx::read(&origen)
        .map_err(|e| format!("El respaldo seleccionado está dañado: {}", e))?;

    let parent = ruta_excel.parent().unwrap_or_else(|| Path::new("."));
    let file_name = ruta_excel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("planilla.xlsx");
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_ruta = parent.join(format!(".{}.{}.{}.tmp", file_name, std::process::id(), seq));

    fs::copy(&origen, &tmp_ruta)
        .map_err(|e| format!("No se pudo preparar la restauración: {}", e))?;

    let _ = crear_respaldo_en(ruta_excel);

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
    restaurar_respaldo_en(&ruta, archivo_respaldo)
}

/// Escribe el libro Excel de forma atómica usando un archivo temporal en la misma carpeta.
fn guardar_libro(book: &Spreadsheet, ruta: &Path) -> Result<(), String> {
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

fn leer_archivo_en(ruta: &Path) -> Result<(Vec<RegistroAudiencia>, TotalesExcel, String), String> {
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
    guardar_libro(&book, ruta)?;
    Ok(resultado)
}

pub fn actualizar_defensor_archivo(nombre_archivo: &str, defensor: &str) -> Result<String, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    actualizar_defensor_en(&ruta, defensor)
}

fn guardar_nueva_audiencia_en(
    ruta: &Path,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let mut book = reader::xlsx::read(ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;

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
    guardar_libro(&book, ruta)?;
    Ok(totales)
}

pub fn guardar_nueva_audiencia(
    nombre_archivo: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    guardar_nueva_audiencia_en(&ruta, datos, permitir_duplicado)
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

fn actualizar_audiencia_en(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
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

    validar_fila_y_radicado(ws, fila_excel, radicado_esperado)?;

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
    guardar_libro(&book, ruta)?;
    Ok(totales)
}

pub fn actualizar_audiencia_existente(
    nombre_archivo: &str,
    fila_excel: u32,
    radicado_esperado: &str,
    datos: &AudienciaInput,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    actualizar_audiencia_en(&ruta, fila_excel, radicado_esperado, datos, permitir_duplicado)
}

fn eliminar_registro_en(
    ruta: &Path,
    fila_excel: u32,
    radicado_esperado: &str,
) -> Result<TotalesExcel, String> {
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

    validar_fila_y_radicado(ws, fila_excel, radicado_esperado)?;

    let _ = crear_respaldo_en(ruta);

    limpiar_fila(ws, fila_excel);

    let totales = reordenar_y_recalcular(ws);
    guardar_libro(&book, ruta)?;
    Ok(totales)
}

pub fn eliminar_registro_audiencia(
    nombre_archivo: &str,
    fila_excel: u32,
    radicado_esperado: &str,
) -> Result<TotalesExcel, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    eliminar_registro_en(&ruta, fila_excel, radicado_esperado)
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

#[derive(Default)]
struct AcumuladorDefensor {
    display: String,
    archivos: Vec<String>,
    total: u32,
    si: u32,
    no: u32,
    motivos: [u32; 8],
}

/// Consolida estadísticas globales o filtradas por año/mes/planilla/defensor sobre los Excel creados.
pub fn calcular_estadisticas_en(
    carpeta: &Path,
    anio: Option<i32>,
    mes: Option<u32>,
    archivo_filtro: Option<&str>,
    defensor_filtro: Option<&str>,
) -> Result<EstadisticasGlobales, String> {
    let nombres = listar_archivos_en(carpeta)?;
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

    let mut meses_map: BTreeMap<(i32, u32), u32> = BTreeMap::new();
    let mut defensores_map: HashMap<String, AcumuladorDefensor> = HashMap::new();
    let mut defensores_disponibles_map: HashMap<String, String> = HashMap::new();

    for nombre in &nombres {
        let ruta = carpeta.join(nombre);
        let (registros, _totales, defensor) = leer_archivo_en(&ruta)
            .map_err(|e| format!("Error leyendo '{}': {}", nombre, e))?;

        let defensor_norm = defensor.split_whitespace().collect::<Vec<_>>().join(" ");
        if !defensor_norm.is_empty() {
            defensores_disponibles_map
                .entry(defensor_norm.to_lowercase())
                .or_insert_with(|| defensor_norm.clone());
        }

        stats.planillas.push(PlanillaResumen {
            archivo: nombre.clone(),
            defensor: defensor_norm.clone(),
            num_registros: registros.len() as u32,
        });

        if let Some(filtro_arch) = filtro_archivo_limpio {
            if nombre != filtro_arch {
                continue;
            }
        }

        if let Some(ref filtro_def) = filtro_defensor_limpio {
            if filtro_def == "__SIN_ASIGNAR__" {
                if !defensor_norm.is_empty() {
                    continue;
                }
            } else if defensor_norm.to_lowercase() != filtro_def.to_lowercase() {
                continue;
            }
        }

        let mut tuvo_datos_en_rango = false;

        for r in &registros {
            let fecha_parsed = NaiveDate::parse_from_str(r.fecha.trim(), "%d/%m/%Y").ok();

            if let Some(d) = fecha_parsed {
                *meses_map.entry((d.year(), d.month())).or_insert(0) += 1;
            }

            let cumple_periodo = match (anio, mes) {
                (Some(y), Some(m)) => fecha_parsed
                    .map(|d| d.year() == y && d.month() == m)
                    .unwrap_or(false),
                (Some(y), None) => fecha_parsed.map(|d| d.year() == y).unwrap_or(false),
                (None, Some(m)) => fecha_parsed.map(|d| d.month() == m).unwrap_or(false),
                (None, None) => true,
            };

            if !cumple_periodo {
                continue;
            }

            tuvo_datos_en_rango = true;
            stats.total_audiencias += 1;
            if r.realizada == "SI" {
                stats.total_si += 1;
            } else if r.realizada == "NO" {
                stats.total_no += 1;
            }
            for i in 0..8 {
                if r.motivos_flags[i] {
                    stats.totales_motivos[i] += 1;
                }
            }

            let (clave_def, display_def) = if defensor.trim().is_empty() {
                (
                    format!("__sin_asignar__:{}", nombre.to_lowercase()),
                    format!("Sin asignar ({})", nombre.trim_end_matches(".xlsx")),
                )
            } else {
                let norm = defensor.split_whitespace().collect::<Vec<_>>().join(" ");
                (norm.to_lowercase(), norm)
            };

            let acc = defensores_map
                .entry(clave_def)
                .or_insert_with(|| AcumuladorDefensor {
                    display: display_def,
                    ..Default::default()
                });
            if !acc.archivos.contains(nombre) {
                acc.archivos.push(nombre.clone());
            }
            acc.total += 1;
            if r.realizada == "SI" {
                acc.si += 1;
            } else if r.realizada == "NO" {
                acc.no += 1;
            }
            for i in 0..8 {
                if r.motivos_flags[i] {
                    acc.motivos[i] += 1;
                }
            }
        }

        if tuvo_datos_en_rango {
            stats.total_planillas_con_datos += 1;
        }
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
        guardar_libro(&book, &ruta).unwrap();

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
}
