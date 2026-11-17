use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveTime};
use umya_spreadsheet::{reader, writer, Worksheet};

use crate::models::{
    mapear_motivo_a_indice, AudienciaInput, RegistroAudiencia, TotalesExcel, MOTIVOS_LABELS,
};

const PLANTILLA_BYTES: &[u8] = include_bytes!("../../templates/plantilla_audiencias.xlsx");

pub const FILA_INICIO_DATOS: u32 = 11;
pub const FILA_MAXIMA_DATOS: u32 = 110;
pub const FILA_TOTALES: u32 = 111;

pub fn obtener_carpeta_archivos() -> PathBuf {
    // Priorizar carpeta archivos_creados en el directorio de trabajo / ejecutable (modo portable)
    if let Ok(cwd) = std::env::current_dir() {
        // Si estamos dentro de src-tauri durante `cargo tauri dev`, subir un nivel si existe ../archivos_creados
        let parent_archivos = cwd.parent().map(|p| p.join("archivos_creados"));
        if let Some(ref p) = parent_archivos {
            if p.exists() {
                return p.clone();
            }
        }
        let local_archivos = cwd.join("archivos_creados");
        if local_archivos.exists() || fs::create_dir_all(&local_archivos).is_ok() {
            return local_archivos;
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_archivos = exe_dir.join("archivos_creados");
            if fs::create_dir_all(&exe_archivos).is_ok() {
                return exe_archivos;
            }
        }
    }

    let fallback = dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("GestorAudiencias")
        .join("archivos_creados");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

pub fn ruta_archivo(nombre_archivo: &str) -> Result<PathBuf, String> {
    let limpio = Path::new(nombre_archivo)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Nombre de archivo inválido".to_string())?;
    let nombre_final = if limpio.to_lowercase().ends_with(".xlsx") {
        limpio.to_string()
    } else {
        format!("{}.xlsx", limpio)
    };
    Ok(obtener_carpeta_archivos().join(nombre_final))
}

pub fn listar_archivos() -> Result<Vec<String>, String> {
    let carpeta = obtener_carpeta_archivos();
    fs::create_dir_all(&carpeta).map_err(|e| format!("Error creando carpeta: {}", e))?;

    let mut archivos: Vec<String> = fs::read_dir(&carpeta)
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

pub fn crear_copia_plantilla(nombre_archivo: &str) -> Result<String, String> {
    let destino = ruta_archivo(nombre_archivo)?;
    if destino.exists() {
        return Err("Ya existe un archivo con ese nombre.".to_string());
    }
    let nombre_final = destino
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(nombre_archivo)
        .to_string();

    fs::write(&destino, PLANTILLA_BYTES)
        .map_err(|e| format!("No se pudo crear el archivo desde la plantilla: {}", e))?;

    Ok(nombre_final)
}

pub fn eliminar_archivo(nombre_archivo: &str) -> Result<(), String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    if !ruta.exists() {
        return Err("El archivo no existe.".to_string());
    }
    fs::remove_file(&ruta).map_err(|e| format!("No se pudo eliminar el archivo: {}", e))
}

pub fn descargar_archivo(nombre_archivo: &str) -> Result<String, String> {
    let origen = ruta_archivo(nombre_archivo)?;
    if !origen.exists() {
        return Err("Archivo no encontrado.".to_string());
    }

    let carpeta_descargas = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "No se pudo localizar la carpeta de Descargas".to_string())?;

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

fn leer_celda(ws: &Worksheet, col: u32, row: u32) -> String {
    ws.get_cell((col, row))
        .map(|c| c.get_value().trim().to_string())
        .unwrap_or_default()
}

fn limpiar_fila(ws: &mut Worksheet, row: u32) {
    for col in 1..=17 {
        ws.get_cell_mut((col, row)).set_value("");
    }
}

fn escribir_registro_en_fila(ws: &mut Worksheet, row: u32, consecutivo: u32, datos: &AudienciaInput) {
    limpiar_fila(ws, row);

    ws.get_cell_mut((1, row)).set_value_number(consecutivo);
    ws.get_cell_mut((2, row)).set_value(datos.radicado.trim());
    ws.get_cell_mut((3, row)).set_value(datos.tipo.trim());
    ws.get_cell_mut((4, row)).set_value(datos.fecha.trim());
    ws.get_cell_mut((5, row)).set_value(datos.hora.trim());
    ws.get_cell_mut((6, row)).set_value(datos.juzgado.trim());

    let realizada = datos.realizada.trim().to_uppercase();
    if realizada == "SI" {
        ws.get_cell_mut((7, row)).set_value("SI");
        ws.get_cell_mut((8, row)).set_value("");
    } else if realizada == "NO" {
        ws.get_cell_mut((7, row)).set_value("");
        ws.get_cell_mut((8, row)).set_value("NO");

        if let Some(idx) = mapear_motivo_a_indice(&datos.motivo) {
            let label = MOTIVOS_LABELS[idx];
            ws.get_cell_mut((9 + idx as u32, row)).set_value(label);
        }
    }

    ws.get_cell_mut((17, row))
        .set_value(datos.observaciones.trim());
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
        let fecha = leer_celda(ws, 4, row);
        let hora = leer_celda(ws, 5, row);
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

    let mut con_clave: Vec<(NaiveDate, NaiveTime, AudienciaInput)> = registros
        .into_iter()
        .map(|r| {
            let fecha_ord = NaiveDate::parse_from_str(r.fecha.trim(), "%d/%m/%Y")
                .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
            let hora_ord = if r.hora.trim().is_empty() {
                NaiveTime::from_hms_opt(0, 0, 0).unwrap()
            } else {
                NaiveTime::parse_from_str(r.hora.trim(), "%H:%M")
                    .unwrap_or_else(|_| NaiveTime::from_hms_opt(0, 0, 0).unwrap())
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

pub fn leer_archivo_completo(
    nombre_archivo: &str,
) -> Result<(Vec<RegistroAudiencia>, TotalesExcel), String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let book = reader::xlsx::read(&ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja principal del Excel".to_string())?;

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

    Ok((registros, totales))
}

pub fn guardar_nueva_audiencia(
    nombre_archivo: &str,
    datos: &AudienciaInput,
) -> Result<TotalesExcel, String> {
    let ruta = ruta_archivo(nombre_archivo)?;
    if !ruta.exists() {
        return Err("El archivo seleccionado no existe.".to_string());
    }

    let mut book = reader::xlsx::read(&ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;

    // Buscar primera fila disponible entre 11 y 110
    let mut fila_destino = None;
    for row in FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS {
        if leer_celda(ws, 2, row).is_empty() {
            fila_destino = Some(row);
            break;
        }
    }

    let fila = fila_destino
        .ok_or_else(|| "El archivo ya alcanzó el límite máximo de 100 audiencias (filas 11 a 110). Cree un nuevo archivo.".to_string())?;

    let consecutivo = fila - FILA_INICIO_DATOS + 1;
    escribir_registro_en_fila(ws, fila, consecutivo, datos);

    let totales = reordenar_y_recalcular(ws);

    writer::xlsx::write(&book, &ruta).map_err(|e| {
        format!(
            "No se pudo guardar el archivo Excel (verifique que no esté abierto en otro programa): {}",
            e
        )
    })?;

    Ok(totales)
}

pub fn actualizar_audiencia_existente(
    nombre_archivo: &str,
    fila_excel: u32,
    datos: &AudienciaInput,
) -> Result<TotalesExcel, String> {
    if !(FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS).contains(&fila_excel) {
        return Err(format!("Número de fila inválido: {}", fila_excel));
    }

    let ruta = ruta_archivo(nombre_archivo)?;
    let mut book = reader::xlsx::read(&ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;

    let consecutivo = fila_excel - FILA_INICIO_DATOS + 1;
    escribir_registro_en_fila(ws, fila_excel, consecutivo, datos);

    let totales = reordenar_y_recalcular(ws);

    writer::xlsx::write(&book, &ruta)
        .map_err(|e| format!("Error al guardar cambios en Excel: {}", e))?;

    Ok(totales)
}

pub fn eliminar_registro_audiencia(
    nombre_archivo: &str,
    fila_excel: u32,
) -> Result<TotalesExcel, String> {
    if !(FILA_INICIO_DATOS..=FILA_MAXIMA_DATOS).contains(&fila_excel) {
        return Err(format!("Número de fila inválido: {}", fila_excel));
    }

    let ruta = ruta_archivo(nombre_archivo)?;
    let mut book = reader::xlsx::read(&ruta)
        .map_err(|e| format!("Error al abrir el archivo Excel: {}", e))?;
    let ws = book
        .get_sheet_mut(&0)
        .ok_or_else(|| "No se pudo acceder a la hoja de trabajo".to_string())?;

    limpiar_fila(ws, fila_excel);

    let totales = reordenar_y_recalcular(ws);

    writer::xlsx::write(&book, &ruta)
        .map_err(|e| format!("Error al guardar cambios en Excel: {}", e))?;

    Ok(totales)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plantilla_embebida_y_totales_columnas() {
        let temp_dir = std::env::temp_dir().join("lokus_test_excel");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let test_xlsx = temp_dir.join("test_audiencias.xlsx");
        fs::write(&test_xlsx, PLANTILLA_BYTES).unwrap();

        let mut book = reader::xlsx::read(&test_xlsx).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();

        let a1 = AudienciaInput {
            radicado: "11001-2025-001".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "10/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 1".into(),
            realizada: "NO".into(),
            motivo: "Defensor Público".into(),
            observaciones: "Obs 1".into(),
        };
        let a2 = AudienciaInput {
            radicado: "11001-2025-002".into(),
            tipo: "Audiencia de juicio oral".into(),
            fecha: "15/08/2025".into(),
            hora: "14:00".into(),
            juzgado: "Juzgado 2".into(),
            realizada: "SI".into(),
            motivo: "".into(),
            observaciones: "Obs 2".into(),
        };

        escribir_registro_en_fila(ws, 11, 1, &a1);
        escribir_registro_en_fila(ws, 12, 2, &a2);

        let totales = reordenar_y_recalcular(ws);
        assert_eq!(totales.num_registros, 2);
        assert_eq!(totales.total_si, 1);
        assert_eq!(totales.total_no, 1);
        // Defensor Público es índice 7 (columna 16 = P)
        assert_eq!(totales.totales_motivos[7], 1);
        assert_eq!(totales.totales_motivos[0], 0);

        // El más reciente (15/08/2025) debe quedar en la fila 11 tras reordenar
        assert_eq!(leer_celda(ws, 2, 11), "11001-2025-002");
        assert_eq!(leer_celda(ws, 2, 12), "11001-2025-001");
        assert_eq!(leer_celda(ws, 16, 12), "Defensor Público");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

