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

use crate::aprendizaje::{
    fusionar_memoria_ia_desde_paquete_en, inspeccionar_fusion_memoria_ia_en,
    memoria_desde_paquete_exportacion, memoria_tiene_actividad,
    preparar_snapshot_memoria_para_paquete_en, ruta_memoria, MemoriaAprendizaje,
    PaqueteExportacion,
};
use crate::config::{
    cargar_settings, guardar_settings, normalizar_lista_catalogo_usuario,
    normalizar_lista_defensores, AppSettings,
};
use crate::estadistica_ia::juzgados_base;
use crate::excel::{
    asegurar_id_ruta, cargar_registro_estadisticas_en, carpeta_descargas,
    construir_resumen_desde_lectura, crear_respaldo_en, explorar_carpeta_en,
    generar_id_planilla, guardar_registro_estadisticas_en, invalidar_cache, leer_archivo_en,
    obtener_carpeta_archivos, sanitizar_nombre_archivo, sanitizar_ruta_carpeta,
    segmentos_ruta_relativa, upsert_resumen_archivado, validar_color_carpeta,
    RegistroEstadisticasLocal,
};
use crate::models::{ResumenPlanillaArchivada, TIPOS_AUDIENCIA};
use crate::uso_catalogos::{
    asegurar_uso_inicializado_en, fusionar_uso_importado_en, ruta_uso_catalogos, UsoCatalogos,
};

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
    pub tipo_paquete: String, // "completo" | "estadisticas" | "aprendizaje_ia"
    pub exportado: String,
    pub carpetas: Vec<String>,
    pub orden_carpetas: Vec<String>,
    pub colores_carpetas: BTreeMap<String, String>,
    pub planillas: Vec<PlanillaExportada>,
    pub resumenes_estadisticas: Vec<ResumenPlanillaArchivada>,
    pub configuracion: Option<ConfigExportada>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uso_catalogos: Option<UsoCatalogos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aprendizaje_ia: Option<MemoriaAprendizaje>,
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
            uso_catalogos: None,
            aprendizaje_ia: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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
    pub trae_aprendizaje_ia: bool,
    pub ia_reglas_en_paquete: usize,
    pub ia_reglas_nuevas: usize,
    pub ia_reglas_actualizables: usize,
    pub ia_correcciones_en_paquete: usize,
    pub ia_correcciones_nuevas: usize,
    pub ia_alias_en_paquete: usize,
    pub ia_alias_nuevos: usize,
    pub ia_perfiles_en_paquete: usize,
    pub ia_perfiles_actualizables: usize,
    pub ia_ya_sincronizada: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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
    pub ia_fusionada: bool,
    pub ia_reglas_nuevas: usize,
    pub ia_reglas_actualizadas: usize,
    pub ia_correcciones_nuevas: usize,
    pub ia_ejemplos_nuevos: usize,
    pub ia_alias_nuevos: usize,
    pub ia_perfiles_actualizados: usize,
    pub ia_consolidaciones_nuevas: usize,
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

pub fn exportar_paquete_completo_con_ia_en(
    carpeta_archivos: &Path,
    carpeta_destino: &Path,
    settings: &AppSettings,
    tipo_paquete: &str,
    ruta_uso: Option<&Path>,
    ruta_memoria_ia: Option<&Path>,
) -> Result<String, String> {
    let tipo = match tipo_paquete.trim() {
        "completo" => "completo",
        "estadisticas" => "estadisticas",
        "aprendizaje_ia" => "aprendizaje_ia",
        otro => return Err(format!("Tipo de exportación no soportado: {}", otro)),
    };

    let (carpetas, orden_carpetas, colores_carpetas, planillas_exp, resumenes_exp) =
        if tipo == "aprendizaje_ia" {
            (
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
                Vec::new(),
                Vec::new(),
            )
        } else {
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
                    "Aún no hay audiencias registradas para exportar estadísticas.".to_string(),
                );
            }

            (
                carpetas,
                reg.orden_carpetas.clone(),
                reg.colores_carpetas.clone(),
                planillas_exp,
                resumenes_exp,
            )
        };

    let uso_exportado = if tipo == "completo" {
        ruta_uso.map(|r| asegurar_uso_inicializado_en(r, carpeta_archivos, settings))
    } else {
        None
    };

    let aprendizaje_exportado = match (tipo, ruta_memoria_ia) {
        ("aprendizaje_ia", Some(r_mem)) => {
            let mem = preparar_snapshot_memoria_para_paquete_en(r_mem)?;
            if !memoria_tiene_actividad(&mem) {
                return Err(
                    "Aún no hay aprendizaje de IA acumulado para exportar en este dispositivo."
                        .to_string(),
                );
            }
            Some(mem)
        }
        ("aprendizaje_ia", None) => {
            return Err("No se encontró la ruta de memoria de aprendizaje de IA.".to_string());
        }
        ("completo", Some(r_mem)) => {
            let mem = preparar_snapshot_memoria_para_paquete_en(r_mem)?;
            if memoria_tiene_actividad(&mem) {
                Some(mem)
            } else {
                None
            }
        }
        _ => None,
    };

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
            orden_carpetas
        } else {
            Vec::new()
        },
        colores_carpetas: if tipo == "completo" {
            colores_carpetas
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
        uso_catalogos: uso_exportado,
        aprendizaje_ia: aprendizaje_exportado,
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
    } else if tipo == "aprendizaje_ia" {
        let json = serde_json::to_string_pretty(&paquete)
            .map_err(|e| format!("Error preparando el aprendizaje de IA: {}", e))?;
        let ruta = ruta_unica_descargas(
            carpeta_destino,
            &format!("lokus_ia_{}", sello),
            "lokus.json",
        );
        fs::write(&ruta, json)
            .map_err(|e| format!("No se pudo guardar el aprendizaje de IA: {}", e))?;
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

pub fn exportar_paquete_con_uso_en(
    carpeta_archivos: &Path,
    carpeta_destino: &Path,
    settings: &AppSettings,
    tipo_paquete: &str,
    ruta_uso: Option<&Path>,
) -> Result<String, String> {
    exportar_paquete_completo_con_ia_en(
        carpeta_archivos,
        carpeta_destino,
        settings,
        tipo_paquete,
        ruta_uso,
        None,
    )
}

pub fn exportar_paquete_en(
    carpeta_archivos: &Path,
    carpeta_destino: &Path,
    settings: &AppSettings,
    tipo_paquete: &str,
) -> Result<String, String> {
    exportar_paquete_con_uso_en(carpeta_archivos, carpeta_destino, settings, tipo_paquete, None)
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

    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&json_bytes) {
        if val.get("tipo_paquete").is_none()
            && val.get("formato").is_some()
            && val.get("reglas").is_some()
            && val.get("correcciones").is_some()
        {
            if let Ok(paq_exp) = serde_json::from_slice::<PaqueteExportacion>(&json_bytes) {
                if paq_exp.app == "Lokus" && paq_exp.formato >= 2 {
                    return Ok(PaqueteLokus {
                        app: "Lokus".to_string(),
                        version_app: paq_exp.version_app.clone(),
                        formato: FORMATO_PAQUETE,
                        tipo_paquete: "aprendizaje_ia".to_string(),
                        exportado: paq_exp.exportado.clone(),
                        aprendizaje_ia: Some(memoria_desde_paquete_exportacion(&paq_exp)),
                        ..Default::default()
                    });
                }
            }
            return Err("Este archivo es el reporte técnico anónimo de una versión muy antigua. Para sincronizar tu IA entre equipos usa Portabilidad → Exportar → Solo aprendizaje de IA (.lokus.json) o un Respaldo completo (.lokus).".to_string());
        }
    }

    let paquete: PaqueteLokus = serde_json::from_slice(&json_bytes).map_err(|_| {
        "El archivo seleccionado no es un respaldo o paquete válido de Lokus.".to_string()
    })?;

    if paquete.app != "Lokus"
        || !matches!(
            paquete.tipo_paquete.as_str(),
            "completo" | "estadisticas" | "aprendizaje_ia"
        )
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

pub fn inspeccionar_importacion_con_ia_en(
    carpeta_archivos: &Path,
    ruta_memoria_ia: Option<&Path>,
    contenido_b64: &str,
) -> Result<ResumenInspeccionImportacion, String> {
    let paquete = decodificar_paquete_b64(contenido_b64)?;
    let (
        planillas_nuevas,
        planillas_conflicto_id,
        planillas_conflicto_ruta,
        resumenes_nuevos,
        resumenes_ya_en_disco,
        resumenes_ya_archivados,
    ) = if paquete.tipo_paquete == "aprendizaje_ia" {
        (0, 0, 0, 0, 0, 0)
    } else {
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

        (
            planillas_nuevas,
            planillas_conflicto_id,
            planillas_conflicto_ruta,
            resumenes_nuevos,
            resumenes_ya_en_disco,
            resumenes_ya_archivados,
        )
    };

    let (trae_configuracion, defs, juzs, tipos) = match &paquete.configuracion {
        Some(c) => (true, c.defensores.len(), c.juzgados.len(), c.tipos_audiencia.len()),
        None => (false, 0, 0, 0),
    };

    let (
        trae_aprendizaje_ia,
        ia_reglas_en_paquete,
        ia_reglas_nuevas,
        ia_reglas_actualizables,
        ia_correcciones_en_paquete,
        ia_correcciones_nuevas,
        ia_alias_en_paquete,
        ia_alias_nuevos,
        ia_perfiles_en_paquete,
        ia_perfiles_actualizables,
        ia_ya_sincronizada,
    ) = if let Some(mem_in) = &paquete.aprendizaje_ia {
        let insp_ia = ruta_memoria_ia
            .map(|r| inspeccionar_fusion_memoria_ia_en(r, mem_in))
            .unwrap_or_default();
        (
            true,
            mem_in.reglas.len(),
            insp_ia.reglas_nuevas,
            insp_ia.reglas_actualizables,
            mem_in.correcciones.len(),
            insp_ia.correcciones_nuevas,
            mem_in.tabla_alias.len(),
            insp_ia.alias_nuevos,
            mem_in.perfiles_sinteticos.len(),
            insp_ia.perfiles_nuevos_o_actualizados,
            ruta_memoria_ia.is_some() && insp_ia.ya_sincronizado,
        )
    } else {
        (false, 0, 0, 0, 0, 0, 0, 0, 0, 0, false)
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
        trae_aprendizaje_ia,
        ia_reglas_en_paquete,
        ia_reglas_nuevas,
        ia_reglas_actualizables,
        ia_correcciones_en_paquete,
        ia_correcciones_nuevas,
        ia_alias_en_paquete,
        ia_alias_nuevos,
        ia_perfiles_en_paquete,
        ia_perfiles_actualizables,
        ia_ya_sincronizada,
    })
}

pub fn inspeccionar_importacion_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
) -> Result<ResumenInspeccionImportacion, String> {
    inspeccionar_importacion_con_ia_en(carpeta_archivos, None, contenido_b64)
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
    crate::excel::sanear_archivo_excel_en(destino);
    invalidar_cache(destino);
    Ok(())
}

pub fn importar_paquete_completo_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
    mut settings_actual: Option<&mut AppSettings>,
    ruta_uso: Option<&Path>,
    ruta_memoria_ia: Option<&Path>,
) -> Result<ResultadoImportacion, String> {
    let modo = match modo_conflicto.trim() {
        "omitir" => "omitir",
        "reemplazar" => "reemplazar",
        "conservar_ambas" => "conservar_ambas",
        otro => return Err(format!("Modo de importación inválido: {}", otro)),
    };

    let paquete = decodificar_paquete_b64(contenido_b64)?;
    if paquete.tipo_paquete == "completo" {
        if let Some(r_uso) = ruta_uso {
            let s_ref = settings_actual.as_deref();
            let defecto = AppSettings::default();
            let _ = asegurar_uso_inicializado_en(
                r_uso,
                carpeta_archivos,
                s_ref.unwrap_or(&defecto),
            );
        }
    }

    let mut res = ResultadoImportacion {
        tipo_paquete: paquete.tipo_paquete.clone(),
        ..Default::default()
    };
    let mut planillas_nuevas_rel: Vec<String> = Vec::new();

    if paquete.tipo_paquete != "aprendizaje_ia" {
        let (archivos_locales, _) = explorar_carpeta_en(carpeta_archivos)?;
        let mut reg = cargar_registro_estadisticas_en(carpeta_archivos);
        let (mut ruta_por_id, mut rutas_activas, _) =
            construir_mapa_local(&archivos_locales, &mut reg);

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
                    rutas_activas.insert(nueva_rel.clone());
                    planillas_nuevas_rel.push(nueva_rel);
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
            rutas_activas.insert(ruta_limpia.clone());
            planillas_nuevas_rel.push(ruta_limpia);
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
    }

    // 4. Fusionar listas de usuario (sin borrar las actuales ni sobrescribir API keys)
    if importar_config {
        if let (Some(cfg), Some(s)) = (&paquete.configuracion, settings_actual.as_deref_mut()) {
            let mut defs = std::mem::take(&mut s.defensores);
            defs.extend(cfg.defensores.iter().cloned());
            s.defensores = normalizar_lista_defensores(defs);

            let mut juzs = std::mem::take(&mut s.juzgados);
            juzs.extend(cfg.juzgados.iter().cloned());
            s.juzgados = normalizar_lista_catalogo_usuario(juzs, juzgados_base());

            let tipos_default: Vec<String> = TIPOS_AUDIENCIA
                .iter()
                .filter(|&&t| t != "Otra")
                .map(|&t| t.to_string())
                .collect();
            let mut tipos = std::mem::take(&mut s.tipos_audiencia);
            tipos.extend(cfg.tipos_audiencia.iter().cloned());
            s.tipos_audiencia = normalizar_lista_catalogo_usuario(tipos, &tipos_default);

            res.config_fusionada = true;
        }
    }

    // 5. Fusionar o restaurar historial de uso de catálogos en copias completas
    if paquete.tipo_paquete == "completo" {
        if let Some(r_uso) = ruta_uso {
            let defecto = AppSettings::default();
            let s_ref = settings_actual.as_deref().unwrap_or(&defecto);
            fusionar_uso_importado_en(
                r_uso,
                carpeta_archivos,
                s_ref,
                paquete.uso_catalogos.as_ref(),
                &planillas_nuevas_rel,
                modo,
            )?;
        }
    }

    // 6. Fusionar memoria de autoaprendizaje IA de manera estrictamente idempotente
    if let (Some(mem_in), Some(r_mem)) = (&paquete.aprendizaje_ia, ruta_memoria_ia) {
        let res_ia = fusionar_memoria_ia_desde_paquete_en(r_mem, mem_in)?;
        res.ia_fusionada = res_ia.cambios_totales > 0;
        res.ia_reglas_nuevas = res_ia.reglas_nuevas;
        res.ia_reglas_actualizadas = res_ia.reglas_actualizadas;
        res.ia_correcciones_nuevas = res_ia.correcciones_nuevas;
        res.ia_ejemplos_nuevos = res_ia.ejemplos_nuevos;
        res.ia_alias_nuevos = res_ia.alias_nuevos + res_ia.alias_actualizados;
        res.ia_perfiles_actualizados = res_ia.perfiles_actualizados;
        res.ia_consolidaciones_nuevas = res_ia.consolidaciones_nuevas;
    }

    Ok(res)
}

pub fn importar_paquete_con_uso_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
    settings_actual: Option<&mut AppSettings>,
    ruta_uso: Option<&Path>,
) -> Result<ResultadoImportacion, String> {
    importar_paquete_completo_en(
        carpeta_archivos,
        contenido_b64,
        modo_conflicto,
        importar_config,
        settings_actual,
        ruta_uso,
        None,
    )
}

pub fn importar_paquete_en(
    carpeta_archivos: &Path,
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
    settings_actual: Option<&mut AppSettings>,
) -> Result<ResultadoImportacion, String> {
    importar_paquete_completo_en(
        carpeta_archivos,
        contenido_b64,
        modo_conflicto,
        importar_config,
        settings_actual,
        None,
        None,
    )
}

pub fn exportar_paquete(tipo_paquete: &str) -> Result<String, String> {
    let settings = cargar_settings();
    let ruta_uso = ruta_uso_catalogos();
    let ruta_mem = ruta_memoria();
    exportar_paquete_completo_con_ia_en(
        &obtener_carpeta_archivos(),
        &carpeta_descargas()?,
        &settings,
        tipo_paquete,
        Some(&ruta_uso),
        Some(&ruta_mem),
    )
}

pub fn inspeccionar_importacion(contenido_b64: &str) -> Result<ResumenInspeccionImportacion, String> {
    inspeccionar_importacion_con_ia_en(&obtener_carpeta_archivos(), Some(&ruta_memoria()), contenido_b64)
}

pub fn importar_paquete(
    contenido_b64: &str,
    modo_conflicto: &str,
    importar_config: bool,
) -> Result<ResultadoImportacion, String> {
    let mut settings = cargar_settings();
    let ruta_uso = ruta_uso_catalogos();
    let ruta_mem = ruta_memoria();
    let res = importar_paquete_completo_en(
        &obtener_carpeta_archivos(),
        contenido_b64,
        modo_conflicto,
        importar_config,
        Some(&mut settings),
        Some(&ruta_uso),
        Some(&ruta_mem),
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

    #[test]
    fn test_paquete_completo_transporta_uso_catalogos_y_estadisticas_no_lo_incluye() {
        use crate::models::AudienciaInput;
        use crate::uso_catalogos::{
            cargar_uso_sin_lock_en, registrar_delta_audiencia_en, NOMBRE_ARCHIVO_USO,
        };

        let orig = preparar_dir("uso_orig");
        let desc = preparar_dir("uso_desc");
        let dest = preparar_dir("uso_dest");
        let uso_orig = orig.join("config").join(NOMBRE_ARCHIVO_USO);
        let uso_dest = dest.join("config").join(NOMBRE_ARCHIVO_USO);

        plantilla_con_datos(
            &orig.join("2026/marzo.xlsx"),
            "Dra. Ana López",
            "10/03/2026",
            "RAD-300",
        );

        let mut settings_orig = AppSettings::default();
        settings_orig.juzgados = vec!["Juzgado Propio Portable".into()];

        // Además de la audiencia del Excel (sembrada: 1), agregamos otra histórica que ya no está en Excel
        let aud_hist = AudienciaInput {
            radicado: "RAD-HIST".into(),
            tipo: "Audiencia de nulidad".into(),
            fecha: "05/03/2026".into(),
            hora: "15:00".into(),
            juzgado: "Juzgado Propio Portable".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        registrar_delta_audiencia_en(&uso_orig, &orig, &settings_orig, None, Some(&aud_hist))
            .unwrap();

        // 1) Paquete de solo estadísticas NO debe incluir uso_catalogos en su JSON
        let ruta_est =
            exportar_paquete_con_uso_en(&orig, &desc, &settings_orig, "estadisticas", Some(&uso_orig))
                .unwrap();
        let json_est = fs::read_to_string(&ruta_est).unwrap();
        assert!(!json_est.contains("uso_catalogos"));

        // 2) Paquete completo .lokus SÍ transporta uso_catalogos (2 audiencias totales) a PC destino
        let ruta_full =
            exportar_paquete_con_uso_en(&orig, &desc, &settings_orig, "completo", Some(&uso_orig))
                .unwrap();
        let b64_full = BASE64.encode(fs::read(&ruta_full).unwrap());
        let mut settings_dest = AppSettings::default();
        importar_paquete_con_uso_en(
            &dest,
            &b64_full,
            "omitir",
            true,
            Some(&mut settings_dest),
            Some(&uso_dest),
        )
        .unwrap();

        let uso_imp = cargar_uso_sin_lock_en(&uso_dest);
        assert_eq!(uso_imp.total_audiencias_contabilizadas, 2);
        assert_eq!(uso_imp.horas_exactas.get("15:00").copied(), Some(1));
        assert_eq!(uso_imp.horas_exactas.get("09:00").copied(), Some(1));

        // Reimportar el mismo paquete en modo omitir NO duplica frecuencias
        importar_paquete_con_uso_en(
            &dest,
            &b64_full,
            "omitir",
            true,
            Some(&mut settings_dest),
            Some(&uso_dest),
        )
        .unwrap();
        let uso_imp2 = cargar_uso_sin_lock_en(&uso_dest);
        assert_eq!(uso_imp2.total_audiencias_contabilizadas, 2);

        // 3) Si el equipo origen añade una nueva planilla y genera un segundo respaldo, importarlo en modo "omitir"
        //    SÍ debe contabilizar las audiencias nuevas de la planilla nueva sin duplicar las anteriores.
        plantilla_con_datos(
            &orig.join("2026/abril.xlsx"),
            "Dra. Ana López",
            "12/04/2026",
            "RAD-301",
        );
        // Forzar actualización incremental en origen (simulando nueva audiencia guardada allá)
        let aud_abril = AudienciaInput {
            radicado: "RAD-301".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "12/04/2026".into(),
            hora: "10:00".into(),
            juzgado: String::new(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        registrar_delta_audiencia_en(&uso_orig, &orig, &settings_orig, None, Some(&aud_abril))
            .unwrap();
        let ruta_full2 =
            exportar_paquete_con_uso_en(&orig, &desc, &settings_orig, "completo", Some(&uso_orig))
                .unwrap();
        let b64_full2 = BASE64.encode(fs::read(&ruta_full2).unwrap());
        importar_paquete_con_uso_en(
            &dest,
            &b64_full2,
            "omitir",
            true,
            Some(&mut settings_dest),
            Some(&uso_dest),
        )
        .unwrap();
        let uso_imp3 = cargar_uso_sin_lock_en(&uso_dest);
        assert_eq!(uso_imp3.total_audiencias_contabilizadas, 3);
        assert_eq!(uso_imp3.horas_exactas.get("09:00").copied(), Some(2));

        let _ = fs::remove_dir_all(&orig);
        let _ = fs::remove_dir_all(&desc);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn test_paquete_aprendizaje_ia_y_completo_bidireccional_sin_duplicados() {
        use crate::aprendizaje::{
            cargar_memoria_en, registrar_resultado_completo_en,
        };
        use crate::models::AudienciaInput;

        let dir_pc = preparar_dir("ia_sync_pc");
        let dir_cel = preparar_dir("ia_sync_cel");
        let desc = preparar_dir("ia_sync_desc");
        let mem_pc = dir_pc.join("ia_aprendizaje.json");
        let mem_cel = dir_cel.join("ia_aprendizaje.json");

        let mut base = AudienciaInput {
            radicado: "2026-0001".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "10/03/2026".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 1 Penal Municipal".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        registrar_resultado_completo_en(
            &mem_pc,
            "Concentrada J1PM 9am",
            &base,
            &base,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana López"),
            None,
        )
        .unwrap();

        base.radicado = "2026-0002".into();
        registrar_resultado_completo_en(
            &mem_cel,
            "Imputación J2PM 11am",
            &base,
            &base,
            "gemini",
            "gemini-2.5-flash",
            &[],
            Some("Dr. Carlos Ruiz"),
            None,
        )
        .unwrap();

        // 1) PC exporta "aprendizaje_ia" sin requerir Excel previos -> lokus_ia_*.lokus.json
        let settings = AppSettings::default();
        let ruta_ia_pc = exportar_paquete_completo_con_ia_en(
            &dir_pc,
            &desc,
            &settings,
            "aprendizaje_ia",
            None,
            Some(&mem_pc),
        )
        .unwrap();
        let nombre_ia = Path::new(&ruta_ia_pc)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(nombre_ia.starts_with("lokus_ia_"), "nombre={}", nombre_ia);
        assert!(nombre_ia.ends_with(".lokus.json"), "nombre={}", nombre_ia);

        // 2) Celular inspecciona e importa aprendizaje_ia
        let b64_pc = BASE64.encode(fs::read(&ruta_ia_pc).unwrap());
        let insp = inspeccionar_importacion_con_ia_en(&dir_cel, Some(&mem_cel), &b64_pc).unwrap();
        assert_eq!(insp.tipo_paquete, "aprendizaje_ia");
        assert!(insp.trae_aprendizaje_ia);
        assert!(!insp.ia_ya_sincronizada);

        let res_imp = importar_paquete_completo_en(
            &dir_cel,
            &b64_pc,
            "omitir",
            false,
            None,
            None,
            Some(&mem_cel),
        )
        .unwrap();
        assert!(res_imp.ia_fusionada);
        assert_eq!(cargar_memoria_en(&mem_cel).aciertos, 2);

        // 3) Reimportar el mismo archivo reporta ya sincronizado (0 cambios)
        let insp_rep = inspeccionar_importacion_con_ia_en(&dir_cel, Some(&mem_cel), &b64_pc).unwrap();
        assert!(insp_rep.ia_ya_sincronizada);
        let res_rep = importar_paquete_completo_en(
            &dir_cel,
            &b64_pc,
            "omitir",
            false,
            None,
            None,
            Some(&mem_cel),
        )
        .unwrap();
        assert!(!res_rep.ia_fusionada);
        assert_eq!(cargar_memoria_en(&mem_cel).aciertos, 2);

        // 4) Celular exporta de vuelta a PC ("ping-pong"): PC queda en 2 aciertos sin inflar
        let ruta_ia_cel = exportar_paquete_completo_con_ia_en(
            &dir_cel,
            &desc,
            &settings,
            "aprendizaje_ia",
            None,
            Some(&mem_cel),
        )
        .unwrap();
        let b64_cel = BASE64.encode(fs::read(&ruta_ia_cel).unwrap());
        importar_paquete_completo_en(
            &dir_pc,
            &b64_cel,
            "omitir",
            false,
            None,
            None,
            Some(&mem_pc),
        )
        .unwrap();
        assert_eq!(cargar_memoria_en(&mem_pc).aciertos, 2);

        // 5) Respaldo "completo" también embebe aprendizaje_ia sin error
        plantilla_con_datos(&dir_pc.join("2026/enero.xlsx"), "Dra. Ana López", "10/03/2026", "RAD-1");
        let ruta_completo = exportar_paquete_completo_con_ia_en(
            &dir_pc,
            &desc,
            &settings,
            "completo",
            None,
            Some(&mem_pc),
        )
        .unwrap();
        let paq_completo = decodificar_paquete_bytes(&fs::read(&ruta_completo).unwrap()).unwrap();
        assert!(paq_completo.aprendizaje_ia.is_some());
        assert_eq!(paq_completo.aprendizaje_ia.unwrap().aciertos, 2);

        // 6) Importar archivo legado (lokus_aprendizaje_ia_*.json) generado antes de la actualización
        //    y verificar que al volver del celular al PC no se duplica ningún contador ni regla.
        let dir_legacy_pc = preparar_dir("ia_legacy_pc");
        let dir_legacy_cel = preparar_dir("ia_legacy_cel");
        let uso_legacy_pc = dir_legacy_pc.join("uso_catalogos.json");
        let mem_legacy_pc = dir_legacy_pc.join("ia_aprendizaje.json");
        let mem_legacy_cel = dir_legacy_cel.join("ia_aprendizaje.json");

        // Simular PC pre-actualización: uso_catalogos.json tiene instancia_id propio,
        // mientras ia_aprendizaje.json todavía no tenía `instancia_id`.
        let uso_pc_obj = UsoCatalogos {
            instancia_id: "inst-pc-legacy".to_string(),
            ..Default::default()
        };
        fs::write(&uso_legacy_pc, serde_json::to_string_pretty(&uso_pc_obj).unwrap()).unwrap();
        let mem_pc_pre = crate::aprendizaje::MemoriaAprendizaje {
            version: 5,
            aciertos: 4,
            llamadas_extraccion_total: 4,
            ..Default::default()
        };
        fs::write(&mem_legacy_pc, serde_json::to_string_pretty(&mem_pc_pre).unwrap()).unwrap();

        let ruta_telemetria = crate::aprendizaje::exportar_aprendizaje_completo_en(
            &mem_legacy_pc,
            Some(&uso_legacy_pc),
            None,
            &desc,
            &[],
            &[],
        )
        .unwrap();
        let b64_tel = BASE64.encode(fs::read(&ruta_telemetria).unwrap());
        let insp_tel = inspeccionar_importacion_con_ia_en(&dir_legacy_cel, Some(&mem_legacy_cel), &b64_tel)
            .unwrap();
        assert_eq!(insp_tel.tipo_paquete, "aprendizaje_ia");
        assert!(insp_tel.trae_aprendizaje_ia);

        // Celular ya tenía 2 aciertos propios antes de importar el JSON legado del PC
        for _ in 0..2 {
            registrar_resultado_completo_en(
                &mem_legacy_cel,
                "Concentrada 9am",
                &base,
                &base,
                "gemini",
                "gemini-2.5-flash",
                &[],
                Some("Dr. Carlos Ruiz"),
                None,
            )
            .unwrap();
        }
        importar_paquete_completo_en(
            &dir_legacy_cel,
            &b64_tel,
            "omitir",
            false,
            None,
            None,
            Some(&mem_legacy_cel),
        )
        .unwrap();
        assert_eq!(cargar_memoria_en(&mem_legacy_cel).aciertos, 6); // 4 del PC legado + 2 del celular

        // Celular actualizado exporta de vuelta al PC actualizado -> PC adopta "inst-pc-legacy"
        // desde uso_catalogos.json y queda en 6 exactos (sin sumar dos veces sus propios 4)
        let ruta_vuelta_cel = exportar_paquete_completo_con_ia_en(
            &dir_legacy_cel,
            &desc,
            &settings,
            "aprendizaje_ia",
            None,
            Some(&mem_legacy_cel),
        )
        .unwrap();
        let b64_vuelta_cel = BASE64.encode(fs::read(&ruta_vuelta_cel).unwrap());
        importar_paquete_completo_en(
            &dir_legacy_pc,
            &b64_vuelta_cel,
            "omitir",
            false,
            None,
            None,
            Some(&mem_legacy_pc),
        )
        .unwrap();
        let mem_pc_post = cargar_memoria_en(&mem_legacy_pc);
        assert_eq!(mem_pc_post.instancia_id, "inst-pc-legacy");
        assert_eq!(mem_pc_post.aciertos, 6);

        let _ = fs::remove_dir_all(&dir_legacy_pc);
        let _ = fs::remove_dir_all(&dir_legacy_cel);
        let _ = fs::remove_dir_all(&dir_pc);
        let _ = fs::remove_dir_all(&dir_cel);
        let _ = fs::remove_dir_all(&desc);
    }
}
