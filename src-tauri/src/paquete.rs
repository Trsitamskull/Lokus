use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Local;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::config::{
    cargar_settings, guardar_settings, normalizar_lista_defensores, AppSettings,
};
use crate::excel::{
    asegurar_id_ruta, cargar_registro_estadisticas_en, carpeta_descargas,
    construir_resumen_desde_lectura, crear_respaldo_en, explorar_carpeta_en,
    generar_id_planilla, guardar_registro_estadisticas_en, invalidar_cache, leer_archivo_en,
    obtener_carpeta_archivos, sanitizar_nombre_archivo, sanitizar_ruta_carpeta,
    segmentos_ruta_relativa, upsert_resumen_archivado, validar_color_carpeta,
    RegistroEstadisticasLocal,
};
use crate::models::ResumenPlanillaArchivada;

pub const FORMATO_PAQUETE: u32 = 1;
const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanillaExportada {
    pub id: String,
    pub ruta_relativa: String,
    #[serde(default)]
    pub defensor: String,
    #[serde(default)]
    pub num_registros: u32,
    pub archivo_b64: String,
}

/// Copia segura de listas y preferencias visuales (nunca incluye API keys).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ConfigExportada {
    pub provider: String,
    pub openai_model: String,
    pub gemini_model: String,
    pub theme: String,
    pub defensores: Vec<String>,
    pub juzgados: Vec<String>,
    pub tipos_audiencia: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaqueteLokus {
    pub app: String,
    pub version_app: String,
    pub formato: u32,
    pub tipo_paquete: String, // "completo" | "estadisticas"
    pub exportado: String,
    pub carpetas: Vec<String>,
    pub orden_carpetas: Vec<String>,
    pub colores_carpetas: BTreeMap<String, String>,
    pub planillas: Vec<PlanillaExportada>,
    pub resumenes_estadisticas: Vec<ResumenPlanillaArchivada>,
    pub configuracion: Option<ConfigExportada>,
}

impl Default for PaqueteLokus {
    fn default() -> Self {
        Self {
            app: "Lokus".to_string(),
            version_app: env!("CARGO_PKG_VERSION").to_string(),
            formato: FORMATO_PAQUETE,
            tipo_paquete: "completo".to_string(),
            exportado: String::new(),
            carpetas: Vec::new(),
            orden_carpetas: Vec::new(),
            colores_carpetas: BTreeMap::new(),
            planillas: Vec::new(),
            resumenes_estadisticas: Vec::new(),
            configuracion: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumenInspeccionImportacion {
    pub tipo_paquete: String,
    pub exportado: String,
    pub version_app: String,
    pub total_carpetas: usize,
    pub total_planillas: usize,
    pub planillas_nuevas: usize,
    pub planillas_conflicto_id: usize,
    pub planillas_conflicto_ruta: usize,
    pub total_resumenes: usize,
    pub resumenes_nuevos: usize,
    pub resumenes_ya_en_disco: usize,
    pub resumenes_ya_archivados: usize,
    pub trae_configuracion: bool,
    pub defensores_en_paquete: usize,
    pub juzgados_en_paquete: usize,
    pub tipos_en_paquete: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultadoImportacion {
    pub tipo_paquete: String,
    pub carpetas_creadas: usize,
    pub planillas_importadas: usize,
    pub planillas_reemplazadas: usize,
    pub planillas_omitidas: usize,
    pub resumenes_importados: usize,
    pub resumenes_reemplazados: usize,
    pub resumenes_omitidos: usize,
    pub config_fusionada: bool,
}

fn construir_config_exportada(s: &AppSettings) -> ConfigExportada {
    ConfigExportada {
        provider: s.provider.clone(),
        openai_model: s.openai_model.clone(),
        gemini_model: s.gemini_model.clone(),
        theme: s.theme.clone(),
        defensores: s.defensores.clone(),
        juzgados: s.juzgados.clone(),
        tipos_audiencia: s.tipos_audiencia.clone(),
    }
}

fn ruta_unica_descargas(carpeta_destino: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut destino = carpeta_destino.join(format!("{}.{}", stem, ext));
    let mut contador = 1;
    while destino.exists() {
        destino = carpeta_destino.join(format!("{}_{}.{}", stem, contador, ext));
        contador += 1;
    }
    destino
}

pub fn exportar_paquete_en(
    carpeta_archivos: &Path,
    carpeta_destino: &Path,
    settings: &AppSettings,
    tipo_paquete: &str,
) -> Result<String, String> {
    let tipo = match tipo_paquete.trim() {
        "completo" => "completo",
        "estadisticas" => "estadisticas",
        otro => return Err(format!("Tipo de exportación no soportado: {}", otro)),
    };

    let (archivos, carpetas) = explorar_carpeta_en(carpeta_archivos)?;
    let mut reg = cargar_registro_estadisticas_en(carpeta_archivos);
    let mut cambio_indice = false;
    let mut ids_activos: HashSet<String> = HashSet::new();

    let mut planillas_exp = Vec::new();
    let mut resumenes_exp = Vec::new();

    for rel in &archivos {
        let (id, creado) = asegurar_id_ruta(&mut reg, rel);
        if creado {
            cambio_indice = true;
        }
        ids_activos.insert(id.clone());

        let abs = carpeta_archivos.join(rel);
        let (registros, _totales, defensor) =
            leer_archivo_en(&abs).map_err(|e| format!("Error leyendo '{}': {}", rel, e))?;

        if tipo == "completo" {
            let bytes = fs::read(&abs)
                .map_err(|e| format!("No se pudo leer '{}': {}", rel, e))?;
            planillas_exp.push(PlanillaExportada {
                id,
                ruta_relativa: rel.clone(),
                defensor,
                num_registros: registros.len() as u32,
                archivo_b64: BASE64.encode(bytes),
            });
        } else if !registros.is_empty() {
            resumenes_exp.push(construir_resumen_desde_lectura(
                id,
                rel.clone(),
                &defensor,
                &registros,
            ));
        }
    }

    if cambio_indice {
        guardar_registro_estadisticas_en(carpeta_archivos, &reg)?;
    }

    for arch in &reg.archivadas {
        if !ids_activos.contains(&arch.id) {
            resumenes_exp.push(arch.clone());
        }
    }

    if tipo == "estadisticas" && resumenes_exp.is_empty() {
        return Err(
            "Aún no hay audiencias registradas para exportar estadísticas."
                .to_string(),
        );
    }

    let paquete = PaqueteLokus {
        app: "Lokus".to_string(),
        version_app: env!("CARGO_PKG_VERSION").to_string(),
        formato: FORMATO_PAQUETE,
        tipo_paquete: tipo.to_string(),
        exportado: Local::now().to_rfc3339(),
        carpetas: if tipo == "completo" {
            carpetas
        } else {
            Vec::new()
        },
        orden_carpetas: if tipo == "completo" {
            reg.orden_carpetas.clone()
        } else {
            Vec::new()
        },
        colores_carpetas: if tipo == "completo" {
            reg.colores_carpetas.clone()
        } else {
            BTreeMap::new()
        },
        planillas: planillas_exp,
        resumenes_estadisticas: resumenes_exp,
        configuracion: if tipo == "completo" {
            Some(construir_config_exportada(settings))
        } else {
            None
        },
    };

    fs::create_dir_all(carpeta_destino)
        .map_err(|e| format!("No se pudo acceder a la carpeta de Descargas: {}", e))?;

    let sello = Local::now().format("%Y%m%d_%H%M%S");
    let destino = if tipo == "completo" {
        let json_bytes = serde_json::to_vec(&paquete)
            .map_err(|e| format!("Error preparando el respaldo: {}", e))?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&json_bytes)
            .map_err(|e| format!("Error comprimiendo el respaldo: {}", e))?;
        let comprimido = encoder
            .finish()
            .map_err(|e| format!("Error finalizando compresión: {}", e))?;
        let ruta = ruta_unica_descargas(
            carpeta_destino,
            &format!("lokus_respaldo_{}", sello),
            "lokus",
        );
        fs::write(&ruta, comprimido)
            .map_err(|e| format!("No se pudo guardar el respaldo: {}", e))?;
        ruta
    } else {
        let json = serde_json::to_string_pretty(&paquete)
            .map_err(|e| format!("Error preparando las estadísticas: {}", e))?;
        let ruta = ruta_unica_descargas(
            carpeta_destino,
            &format!("lokus_estadisticas_{}", sello),
            "lokus.json",
        );
        fs::write(&ruta, json)
            .map_err(|e| format!("No se pudieron guardar las estadísticas: {}", e))?;
        ruta
    };

    Ok(destino.to_string_lossy().to_string())
}

pub fn decodificar_paquete_bytes(raw: &[u8]) -> Result<PaqueteLokus, String> {
    if raw.len() as u64 > MAX_IMPORT_BYTES {
        return Err("El archivo seleccionado es demasiado grande para importarse.".to_string());
    }

    let json_bytes: Vec<u8> = if raw.starts_with(&[0x1f, 0x8b]) {
        let gz = GzDecoder::new(raw);
        let mut salida = Vec::new();
        gz.take(MAX_IMPORT_BYTES)
            .read_to_end(&mut salida)
            .map_err(|e| format!("El respaldo comprimido está dañado: {}", e))?;
        salida
    } else {
        raw.to_vec()
    };

    let paquete: PaqueteLokus = serde_json::from_slice(&json_bytes).map_err(|_| {
        "El archivo seleccionado no es un respaldo o paquete válido de Lokus.".to_string()
    })?;

    if paquete.app != "Lokus"
        || (paquete.tipo_paquete != "completo" && paquete.tipo_paquete != "estadisticas")
    {
        return Err("El archivo no corresponde a un paquete válido de Lokus.".to_string());
    }

    Ok(paquete)
}

pub fn decodificar_paquete_b64(contenido_b64: &str) -> Result<PaqueteLokus, String> {
    let raw = BASE64
        .decode(contenido_b64.trim())
        .map_err(|_| "No se pudo leer el archivo seleccionado.".to_string())?;
    decodificar_paquete_bytes(&raw)
}

fn normalizar_ruta_planilla_paquete(ruta_relativa: &str) -> Result<String, String> {
    let segmentos = segmentos_ruta_relativa(ruta_relativa)?;
    let mut limpios = Vec::with_capacity(segmentos.len());
    let ultimo = segmentos.len() - 1;
    for (i, seg) in segmentos.into_iter().enumerate() {
        if i == ultimo {
            limpios.push(sanitizar_nombre_archivo(&seg)?);
        } else {
            let limpio_dir = sanitizar_ruta_carpeta(&seg)?;
            limpios.push(limpio_dir);
        }
    }
    Ok(limpios.join("/"))
}

fn construir_mapa_local(
    archivos_locales: &[String],
    reg: &mut RegistroEstadisticasLocal,
) -> (HashMap<String, String>, HashSet<String>, HashSet<String>) {
    let mut ruta_por_id = HashMap::new();
    let mut rutas_activas = HashSet::new();
    for ruta in archivos_locales {
        let (id, _) = asegurar_id_ruta(reg, ruta);
        ruta_por_id.insert(id, ruta.clone());
        rutas_activas.insert(ruta.clone());
    }
    let ids_archivados: HashSet<String> = reg.archivadas.iter().map(|a| a.id.clone()).collect();
    (ruta_por_id, rutas_activas, ids_archivados)
}

pub fn inspeccionar_importacion_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
) -> Result<ResumenInspeccionImportacion, String> {
    let paquete = decodificar_paquete_b64(contenido_b64)?;
    let (archivos_locales, _) = explorar_carpeta_en(carpeta_archivos)?;
    let mut reg = cargar_registro_estadisticas_en(carpeta_archivos);
    let (ruta_por_id, rutas_activas, ids_archivados) =
        construir_mapa_local(&archivos_locales, &mut reg);

    let mut planillas_nuevas = 0;
    let mut planillas_conflicto_id = 0;
    let mut planillas_conflicto_ruta = 0;

    for p in &paquete.planillas {
        let ruta_ok = normalizar_ruta_planilla_paquete(&p.ruta_relativa)
            .unwrap_or_else(|_| p.ruta_relativa.clone());
        if ruta_por_id.contains_key(&p.id) {
            planillas_conflicto_id += 1;
        } else if rutas_activas.contains(&ruta_ok) {
            planillas_conflicto_ruta += 1;
        } else {
            planillas_nuevas += 1;
        }
    }

    let mut ids_despues_planillas: HashSet<String> = ruta_por_id.keys().cloned().collect();
    for p in &paquete.planillas {
        if !p.id.trim().is_empty() {
            ids_despues_planillas.insert(p.id.clone());
        }
    }

    let mut resumenes_nuevos = 0;
    let mut resumenes_ya_en_disco = 0;
    let mut resumenes_ya_archivados = 0;

    for r in &paquete.resumenes_estadisticas {
        if ids_despues_planillas.contains(&r.id) {
            resumenes_ya_en_disco += 1;
        } else if ids_archivados.contains(&r.id) {
            resumenes_ya_archivados += 1;
        } else {
            resumenes_nuevos += 1;
        }
    }

    let (trae_configuracion, defs, juzs, tipos) = match &paquete.configuracion {
        Some(c) => (true, c.defensores.len(), c.juzgados.len(), c.tipos_audiencia.len()),
        None => (false, 0, 0, 0),
    };

    Ok(ResumenInspeccionImportacion {
        tipo_paquete: paquete.tipo_paquete,
        exportado: paquete.exportado,
        version_app: paquete.version_app,
        total_carpetas: paquete.carpetas.len(),
        total_planillas: paquete.planillas.len(),
        planillas_nuevas,
        planillas_conflicto_id,
        planillas_conflicto_ruta,
        total_resumenes: paquete.resumenes_estadisticas.len(),
        resumenes_nuevos,
        resumenes_ya_en_disco,
        resumenes_ya_archivados,
        trae_configuracion,
        defensores_en_paquete: defs,
        juzgados_en_paquete: juzs,
        tipos_en_paquete: tipos,
    })
}

fn ruta_alternativa_sin_colision(carpeta_base: &Path, ruta_rel: &str) -> String {
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
            format!("{}_importado.xlsx", stem)
        } else {
            format!("{}_importado_{}.xlsx", stem, idx)
        };
        let candidato_rel = match parent {
            Some(dir) => format!("{}/{}", dir, candidato_nombre),
            None => candidato_nombre,
        };
        if !carpeta_base.join(&candidato_rel).exists() {
            return candidato_rel;
        }
    }
    format!("{}_{}.xlsx", stem, generar_id_planilla())
}

fn escribir_excel_importado(destino: &Path, bytes: &[u8], hacer_respaldo: bool) -> Result<(), String> {
    if !bytes.starts_with(b"PK") {
        return Err("Una planilla del respaldo no tiene formato Excel (.xlsx) válido.".to_string());
    }
    if let Some(parent) = destino.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo preparar carpeta al importar: {}", e))?;
    }
    if hacer_respaldo && destino.exists() {
        let _ = crear_respaldo_en(destino);
    }
    fs::write(destino, bytes).map_err(|e| format!("No se pudo escribir '{}': {}", destino.display(), e))?;
    invalidar_cache(destino);
    Ok(())
}

pub fn importar_paquete_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
    settings_actual: Option<&mut AppSettings>,
) -> Result<ResultadoImportacion, String> {
    let modo = match modo_conflicto.trim() {
        "omitir" => "omitir",
        "reemplazar" => "reemplazar",
        "conservar_ambas" => "conservar_ambas",
        otro => return Err(format!("Modo de importación inválido: {}", otro)),
    };

    let paquete = decodificar_paquete_b64(contenido_b64)?;
    let (archivos_locales, _) = explorar_carpeta_en(carpeta_archivos)?;
    let mut reg = cargar_registro_estadisticas_en(carpeta_archivos);
    let (mut ruta_por_id, mut rutas_activas, _) =
        construir_mapa_local(&archivos_locales, &mut reg);

    let mut res = ResultadoImportacion {
        tipo_paquete: paquete.tipo_paquete.clone(),
        ..Default::default()
    };

    // 1. Recrear estructura de carpetas (incluyendo carpetas vacías)
    for carpeta_rel in &paquete.carpetas {
        if let Ok(limpia) = sanitizar_ruta_carpeta(carpeta_rel) {
            let dir = carpeta_archivos.join(&limpia);
            if !dir.exists() {
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("No se pudo crear carpeta '{}': {}", limpia, e))?;
                res.carpetas_creadas += 1;
            }
        }
    }

    // 2. Importar planillas Excel (.xlsx) en backups completos
    for p in &paquete.planillas {
        let ruta_limpia = normalizar_ruta_planilla_paquete(&p.ruta_relativa)?;
        let bytes = BASE64
            .decode(p.archivo_b64.trim())
            .map_err(|_| format!("Archivo '{}' corrupto en el respaldo.", ruta_limpia))?;

        let ruta_existente_por_id = ruta_por_id.get(&p.id).cloned();
        let existe_en_ruta = rutas_activas.contains(&ruta_limpia);
        let hay_conflicto = ruta_existente_por_id.is_some() || existe_en_ruta;

        if hay_conflicto {
            match modo {
                "omitir" => {
                    res.planillas_omitidas += 1;
                }
                "reemplazar" => {
                    let destino_rel = ruta_existente_por_id.unwrap_or(ruta_limpia);
                    let destino_abs = carpeta_archivos.join(&destino_rel);
                    escribir_excel_importado(&destino_abs, &bytes, true)?;
                    let final_id = if p.id.trim().is_empty() {
                        generar_id_planilla()
                    } else {
                        p.id.clone()
                    };
                    reg.ids_por_ruta.insert(destino_rel.clone(), final_id.clone());
                    reg.archivadas.retain(|a| a.id != final_id);
                    ruta_por_id.insert(final_id, destino_rel.clone());
                    rutas_activas.insert(destino_rel);
                    res.planillas_reemplazadas += 1;
                }
                "conservar_ambas" => {
                    let nueva_rel = ruta_alternativa_sin_colision(carpeta_archivos, &ruta_limpia);
                    let nuevo_id = generar_id_planilla();
                    let destino_abs = carpeta_archivos.join(&nueva_rel);
                    escribir_excel_importado(&destino_abs, &bytes, false)?;
                    reg.ids_por_ruta.insert(nueva_rel.clone(), nuevo_id.clone());
                    ruta_por_id.insert(nuevo_id, nueva_rel.clone());
                    rutas_activas.insert(nueva_rel);
                    res.planillas_importadas += 1;
                }
                _ => {}
            }
        } else {
            let destino_abs = carpeta_archivos.join(&ruta_limpia);
            escribir_excel_importado(&destino_abs, &bytes, false)?;
            let final_id = if p.id.trim().is_empty() {
                generar_id_planilla()
            } else {
                p.id.clone()
            };
            reg.ids_por_ruta.insert(ruta_limpia.clone(), final_id.clone());
            // Si antes estaba archivada con el mismo ID, el Excel vivo reemplaza al resumen
            reg.archivadas.retain(|a| a.id != final_id);
            ruta_por_id.insert(final_id, ruta_limpia.clone());
            rutas_activas.insert(ruta_limpia);
            res.planillas_importadas += 1;
        }
    }

    // 3. Importar resúmenes estadísticos garantizando que NUNCA dupliquen planillas vivas con el mismo ID
    for r in &paquete.resumenes_estadisticas {
        if ruta_por_id.contains_key(&r.id) {
            // Ya existe la planilla Excel viva en disco con el mismo ID: omitir resumen para no contar doble
            res.resumenes_omitidos += 1;
            continue;
        }

        let ya_archivada = reg.archivadas.iter().any(|a| a.id == r.id);
        if ya_archivada {
            match modo {
                "omitir" => {
                    res.resumenes_omitidos += 1;
                }
                "reemplazar" => {
                    upsert_resumen_archivado(&mut reg, r.clone());
                    res.resumenes_reemplazados += 1;
                }
                "conservar_ambas" => {
                    let mut copia = r.clone();
                    copia.id = generar_id_planilla();
                    reg.archivadas.push(copia);
                    res.resumenes_importados += 1;
                }
                _ => {}
            }
        } else {
            upsert_resumen_archivado(&mut reg, r.clone());
            res.resumenes_importados += 1;
        }
    }

    for item in &paquete.orden_carpetas {
        if let Ok(limpia) = sanitizar_ruta_carpeta(item) {
            if !reg.orden_carpetas.contains(&limpia) {
                reg.orden_carpetas.push(limpia);
            }
        }
    }

    for (carpeta, color) in &paquete.colores_carpetas {
        if let (Ok(limpia), Ok(color_valido)) =
            (sanitizar_ruta_carpeta(carpeta), validar_color_carpeta(color))
        {
            if !color_valido.is_empty() && !reg.colores_carpetas.contains_key(&limpia) {
                reg.colores_carpetas.insert(limpia, color_valido);
            }
        }
    }

    guardar_registro_estadisticas_en(carpeta_archivos, &reg)?;
    if res.planillas_importadas > 0 || res.planillas_reemplazadas > 0 {
        crate::excel::invalidar_priors_en(carpeta_archivos);
    }

    // 4. Fusionar listas de usuario (sin borrar las actuales ni sobrescribir API keys)
    if importar_config {
        if let (Some(cfg), Some(s)) = (&paquete.configuracion, settings_actual) {
            let mut defs = std::mem::take(&mut s.defensores);
            defs.extend(cfg.defensores.iter().cloned());
            s.defensores = normalizar_lista_defensores(defs);

            let mut juzs = std::mem::take(&mut s.juzgados);
            juzs.extend(cfg.juzgados.iter().cloned());
            s.juzgados = normalizar_lista_defensores(juzs);

            let mut tipos = std::mem::take(&mut s.tipos_audiencia);
            tipos.extend(cfg.tipos_audiencia.iter().cloned());
            s.tipos_audiencia = normalizar_lista_defensores(tipos);

            res.config_fusionada = true;
        }
    }

    Ok(res)
}

pub fn exportar_paquete(tipo_paquete: &str) -> Result<String, String> {
    let settings = cargar_settings();
    exportar_paquete_en(
        &obtener_carpeta_archivos(),
        &carpeta_descargas()?,
        &settings,
        tipo_paquete,
    )
}

pub fn inspeccionar_importacion(contenido_b64: &str) -> Result<ResumenInspeccionImportacion, String> {
    inspeccionar_importacion_en(&obtener_carpeta_archivos(), contenido_b64)
}

pub fn importar_paquete(
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
) -> Result<ResultadoImportacion, String> {
    let mut settings = cargar_settings();
    let res = importar_paquete_en(
        &obtener_carpeta_archivos(),
        contenido_b64,
        modo_conflicto,
        importar_config,
        Some(&mut settings),
    )?;
    if res.config_fusionada {
        guardar_settings(&settings)?;
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excel::calcular_estadisticas_en;

    fn preparar_dir(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lokus_paquete_{}_{}",
            nombre,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plantilla_con_datos(ruta: &Path, defensor: &str, fecha: &str, radicado: &str) {
        if let Some(p) = ruta.parent() {
            fs::create_dir_all(p).unwrap();
        }
        // Copiar desde plantilla oficial embebida vía umya / bytes
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

    #[test]
    fn test_exportar_completo_e_importar_conserva_carpetas_sin_api_keys() {
        let orig = preparar_dir("orig_completo");
        let dest_exp = preparar_dir("descargas_completo");
        let destino_imp = preparar_dir("destino_completo");

        fs::create_dir_all(orig.join("2026/Vacia")).unwrap();
        plantilla_con_datos(
            &orig.join("2026/Enero/enero.xlsx"),
            "Dra. Ana López",
            "10/01/2026",
            "RAD-001",
        );

        let mut settings_orig = AppSettings::default();
        settings_orig.openai_api_key = "sk-SUPER-SECRETA-123456".into();
        settings_orig.gemini_api_key = "AIzaSUPERSECRETA".into();
        settings_orig.defensores = vec!["Dra. Ana López".into()];
        settings_orig.juzgados = vec!["Juzgado Propio 99".into()];
        settings_orig.tipos_audiencia = vec!["Audiencia especial local".into()];

        let ruta_lokus = exportar_paquete_en(&orig, &dest_exp, &settings_orig, "completo").unwrap();
        let bytes = fs::read(&ruta_lokus).unwrap();
        let b64 = BASE64.encode(&bytes);

        let paq = decodificar_paquete_b64(&b64).unwrap();
        let json_paq = serde_json::to_string(&paq).unwrap();
        assert!(!json_paq.contains("sk-SUPER-SECRETA-123456"));
        assert!(!json_paq.contains("AIzaSUPERSECRETA"));

        let inspeccion = inspeccionar_importacion_en(&destino_imp, &b64).unwrap();
        assert_eq!(inspeccion.tipo_paquete, "completo");
        assert_eq!(inspeccion.planillas_nuevas, 1);
        assert_eq!(inspeccion.total_carpetas, 3);

        let mut settings_dest = AppSettings::default();
        settings_dest.openai_api_key = "sk-CLAVE-LOCAL-DESTINO".into();
        settings_dest.juzgados = vec!["Juzgado Destino 1".into()];

        let res = importar_paquete_en(
            &destino_imp,
            &b64,
            "omitir",
            true,
            Some(&mut settings_dest),
        )
        .unwrap();
        assert_eq!(res.planillas_importadas, 1);
        assert!(destino_imp.join("2026/Enero/enero.xlsx").exists());
        assert!(destino_imp.join("2026/Vacia").is_dir());
        assert_eq!(settings_dest.openai_api_key, "sk-CLAVE-LOCAL-DESTINO");
        assert!(settings_dest.juzgados.contains(&"Juzgado Propio 99".to_string()));
        assert!(settings_dest.juzgados.contains(&"Juzgado Destino 1".to_string()));

        // Reimportar el mismo paquete en modo omitir NO duplica nada
        let res2 = importar_paquete_en(&destino_imp, &b64, "omitir", false, None).unwrap();
        assert_eq!(res2.planillas_omitidas, 1);
        assert_eq!(res2.planillas_importadas, 0);

        let stats = calcular_estadisticas_en(&destino_imp, None, None, None, None).unwrap();
        assert_eq!(stats.total_audiencias, 1);
        assert_eq!(stats.total_si, 1);

        let _ = fs::remove_dir_all(&orig);
        let _ = fs::remove_dir_all(&dest_exp);
        let _ = fs::remove_dir_all(&destino_imp);
    }

    #[test]
    fn test_exportar_solo_estadisticas_e_importar_completo_no_duplica() {
        let eq_a = preparar_dir("eq_a");
        let desc = preparar_dir("desc_est");
        let eq_b = preparar_dir("eq_b");

        plantilla_con_datos(
            &eq_a.join("2026/febrero.xlsx"),
            "Dr. Luis Torres",
            "15/02/2026",
            "RAD-200",
        );

        let settings = AppSettings::default();
        // 1) Equipo A exporta SOLO estadísticas y Equipo B las importa
        let ruta_stats = exportar_paquete_en(&eq_a, &desc, &settings, "estadisticas").unwrap();
        let b64_stats = BASE64.encode(fs::read(&ruta_stats).unwrap());

        let insp1 = inspeccionar_importacion_en(&eq_b, &b64_stats).unwrap();
        assert_eq!(insp1.tipo_paquete, "estadisticas");
        assert_eq!(insp1.resumenes_nuevos, 1);

        importar_paquete_en(&eq_b, &b64_stats, "omitir", false, None).unwrap();
        let stats_b1 = calcular_estadisticas_en(&eq_b, None, None, None, None).unwrap();
        assert_eq!(stats_b1.total_audiencias, 1);
        assert_eq!(stats_b1.total_archivadas, 1);

        // 2) Más tarde, Equipo A exporta copia COMPLETA con el Excel real y Equipo B lo importa
        let ruta_full = exportar_paquete_en(&eq_a, &desc, &settings, "completo").unwrap();
        let b64_full = BASE64.encode(fs::read(&ruta_full).unwrap());
        importar_paquete_en(&eq_b, &b64_full, "omitir", false, None).unwrap();

        // Debe existir el Excel vivo y NO contar doble con el resumen estadístico previo
        let stats_b2 = calcular_estadisticas_en(&eq_b, None, None, None, None).unwrap();
        assert_eq!(stats_b2.total_audiencias, 1);
        assert_eq!(stats_b2.total_archivadas, 0);

        // 3) Y si después vuelve a importar el paquete de solo estadísticas, debe detectarlo y omitirlo
        let insp2 = inspeccionar_importacion_en(&eq_b, &b64_stats).unwrap();
        assert_eq!(insp2.resumenes_ya_en_disco, 1);
        assert_eq!(insp2.resumenes_nuevos, 0);

        let _ = fs::remove_dir_all(&eq_a);
        let _ = fs::remove_dir_all(&desc);
        let _ = fs::remove_dir_all(&eq_b);
    }

    #[test]
    fn test_rechaza_traversal_y_excel_invalido() {
        let dir = preparar_dir("seguridad_imp");
        let malo = PaqueteLokus {
            tipo_paquete: "completo".into(),
            planillas: vec![PlanillaExportada {
                id: "pl-1".into(),
                ruta_relativa: "../../etc/evil.xlsx".into(),
                defensor: String::new(),
                num_registros: 0,
                archivo_b64: BASE64.encode(b"PK\x03\x04"),
            }],
            ..Default::default()
        };
        let b64 = BASE64.encode(serde_json::to_vec(&malo).unwrap());
        assert!(importar_paquete_en(&dir, &b64, "omitir", false, None).is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
