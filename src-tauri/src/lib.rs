pub mod ai;
pub mod aprendizaje;
pub mod config;
pub mod estadistica_ia;
pub mod excel;
pub mod models;
pub mod paquete;
pub mod patrones_abreviacion;
pub mod uso_catalogos;

use ai::{
    analizar_cluster_correcciones_ia, analizar_correccion_ia, consolidar_memoria_ia,
    extraer_datos_audiencia_ia_con_defensor, AiAudienciaOutput,
};
use aprendizaje::{
    aplicar_analisis_cluster_ia, aplicar_analisis_en, aplicar_consolidacion_ia,
    combinar_resumen_con_uso, descartar_aclaracion_en, evaluar_y_preparar_consolidacion_ia,
    exportar_aprendizaje_ia, guardar_nota_aclaracion_en, intentar_reservar_cluster_induccion,
    intentar_reservar_consolidacion, liberar_cluster_induccion, liberar_consolidacion,
    marcar_dato_externo_en, migrar_defensor_en_memoria, obtener_cluster_listo_para_inducir_ia,
    obtener_diagnostico_aprendizaje, obtener_resumen_aprendizaje, preparar_lote_consolidacion_en,
    registrar_resultado_ia_completo, ruta_memoria, DiagnosticoAprendizaje,
    PrediccionLocalResumen, ResumenAprendizaje, TelemetriaDisparoConsolidacion,
};
use config::{
    actualizar_config, actualizar_tema, agregar_defensor, agregar_juzgado, agregar_tipo_audiencia,
    cargar_config_publica, eliminar_defensor, eliminar_juzgado, eliminar_tipo_audiencia,
    renombrar_defensor, ActualizarConfig, ConfigPublica,
};
use excel::{
    actualizar_audiencia_existente, actualizar_defensor_archivo, crear_carpeta_con_color,
    crear_copia_plantilla, descargar_archivo, eliminar_archivo_con_opcion,
    eliminar_carpeta_con_opciones, eliminar_estadistica_archivada, eliminar_registro_audiencia,
    guardar_color_carpeta, guardar_nueva_audiencia, guardar_orden_carpetas, invalidar_priors_en,
    leer_archivo_completo, listar_archivos, listar_carpetas, listar_respaldos, mover_archivo,
    obtener_carpeta_archivos, obtener_colores_carpetas, obtener_estadisticas_globales,
    obtener_orden_carpetas, renombrar_carpeta, restaurar_respaldo, validar_color_carpeta,
    RespaldoInfo,
};
use std::collections::BTreeMap;
use models::{
    es_alcance_general, normalizar_clave_defensor, validar_audiencia, AudienciaInput,
    EstadisticasGlobales, RegistroAudiencia, TotalesExcel,
};
use paquete::{
    exportar_paquete, importar_paquete, inspeccionar_importacion, ResultadoImportacion,
    ResumenInspeccionImportacion,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct DatosArchivoResponse {
    pub registros: Vec<RegistroAudiencia>,
    pub totales: TotalesExcel,
    pub defensor: String,
}

async fn ejecutar_en_hilo_io<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("Error interno al ejecutar operación de archivo: {}", e))?
}

#[tauri::command]
async fn cmd_listar_archivos() -> Result<Vec<String>, String> {
    ejecutar_en_hilo_io(listar_archivos).await
}

#[tauri::command]
async fn cmd_listar_carpetas() -> Result<Vec<String>, String> {
    ejecutar_en_hilo_io(listar_carpetas).await
}

#[tauri::command]
async fn cmd_obtener_orden_carpetas() -> Result<Vec<String>, String> {
    ejecutar_en_hilo_io(|| Ok(obtener_orden_carpetas())).await
}

#[tauri::command]
async fn cmd_guardar_orden_carpetas(orden: Vec<String>) -> Result<Vec<String>, String> {
    ejecutar_en_hilo_io(move || guardar_orden_carpetas(orden)).await
}

#[tauri::command]
async fn cmd_obtener_colores_carpetas() -> Result<BTreeMap<String, String>, String> {
    ejecutar_en_hilo_io(|| Ok(obtener_colores_carpetas())).await
}

#[tauri::command]
async fn cmd_guardar_color_carpeta(
    ruta: String,
    color: String,
) -> Result<BTreeMap<String, String>, String> {
    ejecutar_en_hilo_io(move || guardar_color_carpeta(&ruta, &color)).await
}

#[tauri::command]
async fn cmd_crear_carpeta(ruta: String, color: Option<String>) -> Result<String, String> {
    ejecutar_en_hilo_io(move || crear_carpeta_con_color(&ruta, color.as_deref())).await
}

#[tauri::command]
async fn cmd_renombrar_carpeta(
    actual: String,
    nueva: String,
    color: Option<String>,
) -> Result<String, String> {
    ejecutar_en_hilo_io(move || {
        if let Some(ref c) = color {
            validar_color_carpeta(c)?;
        }
        let nueva_ruta = renombrar_carpeta(&actual, &nueva)?;
        if let Some(c) = color {
            guardar_color_carpeta(&nueva_ruta, &c)?;
        }
        Ok(nueva_ruta)
    })
    .await
}

#[tauri::command]
async fn cmd_eliminar_carpeta(
    ruta: String,
    eliminar_contenido: Option<bool>,
    conservar_estadisticas: Option<bool>,
) -> Result<(), String> {
    let eliminar = eliminar_contenido.unwrap_or(false);
    let conservar = conservar_estadisticas.unwrap_or(false);
    ejecutar_en_hilo_io(move || eliminar_carpeta_con_opciones(&ruta, eliminar, conservar)).await
}

#[tauri::command]
async fn cmd_mover_archivo(nombre: String, carpeta_destino: String) -> Result<String, String> {
    ejecutar_en_hilo_io(move || mover_archivo(&nombre, &carpeta_destino)).await
}

fn validar_datos_creacion_archivo(
    nombre: &str,
    defensor: Option<&str>,
) -> Result<(String, String), String> {
    let nombre_limpio = nombre.trim();
    if nombre_limpio.is_empty() {
        return Err("El nombre del archivo es obligatorio".to_string());
    }
    let defensor_limpio = defensor.map(str::trim).unwrap_or("");
    if defensor_limpio.is_empty() {
        return Err("Debes asignar un Defensor Público para crear la planilla.".to_string());
    }
    Ok((nombre_limpio.to_string(), defensor_limpio.to_string()))
}

#[tauri::command]
async fn cmd_crear_archivo(
    nombre: String,
    defensor: Option<String>,
    carpeta: Option<String>,
) -> Result<String, String> {
    let (nombre_limpio, defensor_limpio) =
        validar_datos_creacion_archivo(&nombre, defensor.as_deref())?;
    ejecutar_en_hilo_io(move || {
        crear_copia_plantilla(&nombre_limpio, Some(&defensor_limpio), carpeta.as_deref())
    })
    .await
}

#[tauri::command]
async fn cmd_eliminar_archivo(
    nombre: String,
    conservar_estadisticas: Option<bool>,
) -> Result<(), String> {
    let conservar = conservar_estadisticas.unwrap_or(false);
    ejecutar_en_hilo_io(move || eliminar_archivo_con_opcion(&nombre, conservar)).await
}

#[tauri::command]
async fn cmd_eliminar_estadistica_archivada(id: String) -> Result<(), String> {
    ejecutar_en_hilo_io(move || eliminar_estadistica_archivada(&id)).await
}

#[tauri::command]
async fn cmd_descargar_archivo(nombre: String) -> Result<String, String> {
    ejecutar_en_hilo_io(move || descargar_archivo(&nombre)).await
}

#[tauri::command]
async fn cmd_leer_archivo(nombre: String) -> Result<DatosArchivoResponse, String> {
    ejecutar_en_hilo_io(move || {
        let (registros, totales, defensor) = leer_archivo_completo(&nombre)?;
        Ok(DatosArchivoResponse {
            registros,
            totales,
            defensor,
        })
    })
    .await
}

#[tauri::command]
async fn cmd_actualizar_defensor_archivo(
    nombre_archivo: String,
    defensor: String,
) -> Result<String, String> {
    ejecutar_en_hilo_io(move || actualizar_defensor_archivo(&nombre_archivo, &defensor)).await
}

#[tauri::command]
async fn cmd_guardar_audiencia(
    nombre_archivo: String,
    datos: AudienciaInput,
    permitir_hora_vacia: bool,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    validar_audiencia(&datos, permitir_hora_vacia)?;
    ejecutar_en_hilo_io(move || guardar_nueva_audiencia(&nombre_archivo, &datos, permitir_duplicado))
        .await
}

#[tauri::command]
async fn cmd_actualizar_audiencia(
    nombre_archivo: String,
    fila_excel: u32,
    radicado_esperado: String,
    datos: AudienciaInput,
    permitir_hora_vacia: bool,
    permitir_duplicado: bool,
) -> Result<TotalesExcel, String> {
    validar_audiencia(&datos, permitir_hora_vacia)?;
    ejecutar_en_hilo_io(move || {
        actualizar_audiencia_existente(
            &nombre_archivo,
            fila_excel,
            &radicado_esperado,
            &datos,
            permitir_duplicado,
        )
    })
    .await
}

#[tauri::command]
async fn cmd_eliminar_registro(
    nombre_archivo: String,
    fila_excel: u32,
    radicado_esperado: String,
) -> Result<TotalesExcel, String> {
    ejecutar_en_hilo_io(move || {
        eliminar_registro_audiencia(&nombre_archivo, fila_excel, &radicado_esperado)
    })
    .await
}

#[tauri::command]
async fn cmd_listar_respaldos(nombre_archivo: String) -> Result<Vec<RespaldoInfo>, String> {
    ejecutar_en_hilo_io(move || listar_respaldos(&nombre_archivo)).await
}

#[tauri::command]
async fn cmd_restaurar_respaldo(
    nombre_archivo: String,
    archivo_respaldo: String,
) -> Result<(), String> {
    ejecutar_en_hilo_io(move || restaurar_respaldo(&nombre_archivo, &archivo_respaldo)).await
}

#[tauri::command]
fn cmd_obtener_config() -> Result<ConfigPublica, String> {
    Ok(cargar_config_publica())
}

#[tauri::command]
fn cmd_guardar_config(config: ActualizarConfig) -> Result<ConfigPublica, String> {
    actualizar_config(config)
}

#[tauri::command]
fn cmd_guardar_tema(tema: String) -> Result<ConfigPublica, String> {
    actualizar_tema(&tema)
}

#[tauri::command]
fn cmd_agregar_defensor(nombre: String) -> Result<ConfigPublica, String> {
    agregar_defensor(&nombre)
}

#[tauri::command]
fn cmd_renombrar_defensor(actual: String, nuevo: String) -> Result<ConfigPublica, String> {
    let cfg = renombrar_defensor(&actual, &nuevo)?;
    let _ = migrar_defensor_en_memoria(&actual, &nuevo);
    invalidar_priors_en(&obtener_carpeta_archivos());
    Ok(cfg)
}

#[tauri::command]
fn cmd_eliminar_defensor(nombre: String) -> Result<ConfigPublica, String> {
    eliminar_defensor(&nombre)
}

#[tauri::command]
fn cmd_agregar_juzgado(nombre: String) -> Result<ConfigPublica, String> {
    agregar_juzgado(&nombre)
}

#[tauri::command]
fn cmd_eliminar_juzgado(nombre: String) -> Result<ConfigPublica, String> {
    eliminar_juzgado(&nombre)
}

#[tauri::command]
fn cmd_agregar_tipo_audiencia(nombre: String) -> Result<ConfigPublica, String> {
    agregar_tipo_audiencia(&nombre)
}

#[tauri::command]
fn cmd_eliminar_tipo_audiencia(nombre: String) -> Result<ConfigPublica, String> {
    eliminar_tipo_audiencia(&nombre)
}

struct ClusterInduccionGuard(String);

impl Drop for ClusterInduccionGuard {
    fn drop(&mut self) {
        liberar_cluster_induccion(&self.0);
    }
}

struct ConsolidacionGuard;

impl Drop for ConsolidacionGuard {
    fn drop(&mut self) {
        liberar_consolidacion();
    }
}

#[tauri::command]
async fn cmd_autocompletar_ia(
    texto: String,
    defensor: Option<String>,
) -> Result<AiAudienciaOutput, String> {
    let resultado = extraer_datos_audiencia_ia_con_defensor(&texto, defensor.as_deref()).await?;
    // Si quedó algún clúster coherente listo (N >= 2) pendiente por falta de conexión previa,
    // se induce en segundo plano con 1 sola llamada agrupada (nunca con casos aislados N = 1).
    tauri::async_runtime::spawn(async move {
        if let Some((cluster_id, corrs)) = obtener_cluster_listo_para_inducir_ia() {
            if !intentar_reservar_cluster_induccion(&cluster_id) {
                return;
            }
            let _guard = ClusterInduccionGuard(cluster_id.clone());
            let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
            if let Ok(analisis) = analizar_cluster_correcciones_ia(&corrs).await {
                let _ = aplicar_analisis_cluster_ia(&cluster_id, &ids, &analisis);
            }
        }
    });
    Ok(resultado)
}

/// Compara lo que propuso la IA con lo que el usuario guardó, almacena la corrección en cola
/// de clústeres y ejecuta inducción agrupada con LLM únicamente cuando un patrón alcanza `N >= 2`.
fn combinar_con_uso_actual(resumen: ResumenAprendizaje) -> ResumenAprendizaje {
    let settings = config::cargar_settings();
    let uso = uso_catalogos::asegurar_uso_inicializado_actual();
    combinar_resumen_con_uso(resumen, &uso, &settings)
}

#[tauri::command]
async fn cmd_registrar_resultado_ia(
    texto: String,
    prediccion: AudienciaInput,
    final_usuario: AudienciaInput,
    reglas_inyectadas: Option<Vec<String>>,
    defensor: Option<String>,
    prediccion_local: Option<PrediccionLocalResumen>,
) -> Result<ResumenAprendizaje, String> {
    let reglas_ids = reglas_inyectadas.unwrap_or_default();
    let def_para_bg = defensor.clone();
    let resumen = ejecutar_en_hilo_io(move || {
        let (res, _) = registrar_resultado_ia_completo(
            &texto,
            &prediccion,
            &final_usuario,
            &reglas_ids,
            defensor.as_deref(),
            prediccion_local.as_ref(),
        )?;
        Ok(combinar_con_uso_actual(res))
    })
    .await?;

    // Inducción diferida por clústeres + consolidación periódica no bloqueantes en segundo plano
    tauri::async_runtime::spawn(async move {
        if let Some((cluster_id, corrs)) =
            tauri::async_runtime::spawn_blocking(obtener_cluster_listo_para_inducir_ia)
                .await
                .ok()
                .flatten()
        {
            if intentar_reservar_cluster_induccion(&cluster_id) {
                let _guard = ClusterInduccionGuard(cluster_id.clone());
                let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
                if let Ok(analisis) = analizar_cluster_correcciones_ia(&corrs).await {
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        aplicar_analisis_cluster_ia(&cluster_id, &ids, &analisis)
                    })
                    .await;
                }
            }
        }

        let consolidacion_activa = tauri::async_runtime::spawn_blocking(|| {
            config::cargar_settings().consolidacion_ia_activa
        })
        .await
        .unwrap_or(false);
        if consolidacion_activa && intentar_reservar_consolidacion() {
            let _cons_guard = ConsolidacionGuard;
            let lote_opt = tauri::async_runtime::spawn_blocking(move || {
                evaluar_y_preparar_consolidacion_ia(def_para_bg.as_deref())
            })
            .await
            .ok()
            .flatten();
            if let Some(lote) = lote_opt {
                if let Ok((res_cons, modelo_usado)) = consolidar_memoria_ia(&lote).await {
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        aplicar_consolidacion_ia(&lote, &res_cons, &modelo_usado)
                    })
                    .await;
                }
            }
        }
    });

    Ok(resumen)
}

#[tauri::command]
async fn cmd_responder_aclaracion_ia(
    correccion_id: String,
    nota: String,
    es_dato_externo: Option<bool>,
) -> Result<ResumenAprendizaje, String> {
    if es_dato_externo.unwrap_or(false) {
        return ejecutar_en_hilo_io(move || {
            let res = marcar_dato_externo_en(&ruta_memoria(), &correccion_id)?;
            Ok(combinar_con_uso_actual(res))
        })
        .await;
    }

    let id_clon = correccion_id.clone();
    let nota_clon = nota.clone();
    let (mut resumen, corr_opt) = ejecutar_en_hilo_io(move || {
        guardar_nota_aclaracion_en(&ruta_memoria(), &id_clon, &nota_clon)
    })
    .await?;

    if let Some(corr) = corr_opt {
        if let Ok(mut analisis) = analizar_correccion_ia(&corr).await {
            analisis.necesita_aclaracion = false;
            let id_ref = corr.id;
            if let Ok(actualizado) =
                ejecutar_en_hilo_io(move || aplicar_analisis_en(&ruta_memoria(), &id_ref, &analisis))
                    .await
            {
                resumen = actualizado;
            }
        }
    }

    ejecutar_en_hilo_io(move || Ok(combinar_con_uso_actual(resumen))).await
}

fn normalizar_ambito_consolidacion(ambito: Option<&str>) -> String {
    match ambito {
        None => "*".to_string(),
        Some(s) if es_alcance_general(s) => "*".to_string(),
        Some(s) => {
            let dk = normalizar_clave_defensor(s);
            if dk.is_empty() {
                "*".to_string()
            } else {
                dk
            }
        }
    }
}

#[tauri::command]
async fn cmd_consolidar_memoria_ia(
    ambito: Option<String>,
) -> Result<DiagnosticoAprendizaje, String> {
    if !intentar_reservar_consolidacion() {
        return Err(
            "Ya hay una consolidación de memoria de IA en curso. Intente nuevamente en unos segundos."
                .to_string(),
        );
    }
    let _cons_guard = ConsolidacionGuard;

    let amb = normalizar_ambito_consolidacion(ambito.as_deref());
    let telemetria = TelemetriaDisparoConsolidacion {
        ambito: amb.clone(),
        manual: true,
        razones: vec!["consolidacion_manual".to_string()],
        ..Default::default()
    };

    let lote = ejecutar_en_hilo_io(move || {
        preparar_lote_consolidacion_en(&ruta_memoria(), &amb, telemetria, true).ok_or_else(|| {
            "Aún no hay reglas ni correcciones registradas en este ámbito para consolidar."
                .to_string()
        })
    })
    .await?;

    let (resultado, modelo_usado) = consolidar_memoria_ia(&lote).await?;
    ejecutar_en_hilo_io(move || {
        let _ = aplicar_consolidacion_ia(&lote, &resultado, &modelo_usado)?;
        Ok(obtener_diagnostico_aprendizaje())
    })
    .await
}

#[tauri::command]
async fn cmd_descartar_aclaracion_ia(correccion_id: String) -> Result<ResumenAprendizaje, String> {
    ejecutar_en_hilo_io(move || {
        let resumen = descartar_aclaracion_en(&ruta_memoria(), &correccion_id)?;
        Ok(combinar_con_uso_actual(resumen))
    })
    .await
}

#[tauri::command]
async fn cmd_resumen_aprendizaje_ia() -> Result<ResumenAprendizaje, String> {
    ejecutar_en_hilo_io(|| Ok(obtener_resumen_aprendizaje())).await
}

#[tauri::command]
async fn cmd_diagnostico_aprendizaje_ia() -> Result<DiagnosticoAprendizaje, String> {
    ejecutar_en_hilo_io(|| Ok(obtener_diagnostico_aprendizaje())).await
}

#[tauri::command]
async fn cmd_exportar_aprendizaje_ia() -> Result<String, String> {
    ejecutar_en_hilo_io(exportar_aprendizaje_ia).await
}

#[tauri::command]
async fn cmd_exportar_paquete(tipo_paquete: String) -> Result<String, String> {
    ejecutar_en_hilo_io(move || exportar_paquete(&tipo_paquete)).await
}

#[tauri::command]
async fn cmd_inspeccionar_importacion(
    contenido_b64: String,
) -> Result<ResumenInspeccionImportacion, String> {
    ejecutar_en_hilo_io(move || inspeccionar_importacion(&contenido_b64)).await
}

#[tauri::command]
async fn cmd_importar_paquete(
    contenido_b64: String,
    modo_conflicto: String,
    importar_config: bool,
) -> Result<ResultadoImportacion, String> {
    ejecutar_en_hilo_io(move || importar_paquete(&contenido_b64, &modo_conflicto, importar_config))
        .await
}

#[tauri::command]
async fn cmd_obtener_estadisticas(
    anio: Option<i32>,
    mes: Option<u32>,
    archivo: Option<String>,
    defensor: Option<String>,
) -> Result<EstadisticasGlobales, String> {
    ejecutar_en_hilo_io(move || obtener_estadisticas_globales(anio, mes, archivo, defensor)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            cmd_listar_archivos,
            cmd_listar_carpetas,
            cmd_obtener_orden_carpetas,
            cmd_guardar_orden_carpetas,
            cmd_obtener_colores_carpetas,
            cmd_guardar_color_carpeta,
            cmd_crear_carpeta,
            cmd_renombrar_carpeta,
            cmd_eliminar_carpeta,
            cmd_mover_archivo,
            cmd_crear_archivo,
            cmd_eliminar_archivo,
            cmd_eliminar_estadistica_archivada,
            cmd_descargar_archivo,
            cmd_leer_archivo,
            cmd_actualizar_defensor_archivo,
            cmd_guardar_audiencia,
            cmd_actualizar_audiencia,
            cmd_eliminar_registro,
            cmd_listar_respaldos,
            cmd_restaurar_respaldo,
            cmd_obtener_estadisticas,
            cmd_obtener_config,
            cmd_guardar_config,
            cmd_guardar_tema,
            cmd_agregar_defensor,
            cmd_renombrar_defensor,
            cmd_eliminar_defensor,
            cmd_agregar_juzgado,
            cmd_eliminar_juzgado,
            cmd_agregar_tipo_audiencia,
            cmd_eliminar_tipo_audiencia,
            cmd_autocompletar_ia,
            cmd_registrar_resultado_ia,
            cmd_responder_aclaracion_ia,
            cmd_descartar_aclaracion_ia,
            cmd_resumen_aprendizaje_ia,
            cmd_diagnostico_aprendizaje_ia,
            cmd_consolidar_memoria_ia,
            cmd_exportar_aprendizaje_ia,
            cmd_exportar_paquete,
            cmd_inspeccionar_importacion,
            cmd_importar_paquete,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}

#[cfg(test)]
mod tests {
    use super::validar_datos_creacion_archivo;

    #[test]
    fn test_validar_datos_creacion_archivo_exige_defensor_y_nombre() {
        assert!(validar_datos_creacion_archivo("", Some("Dra. Ana Pérez")).is_err());
        assert!(validar_datos_creacion_archivo("   ", Some("Dra. Ana Pérez")).is_err());

        let err_none = validar_datos_creacion_archivo("enero_2026", None).unwrap_err();
        assert!(err_none.contains("Defensor Público"), "{}", err_none);
        assert!(validar_datos_creacion_archivo("enero_2026", Some("")).is_err());
        assert!(validar_datos_creacion_archivo("enero_2026", Some("   \t  ")).is_err());

        let (nombre, defensor) =
            validar_datos_creacion_archivo("  enero_2026  ", Some("  Dra. Ana Pérez  "))
                .expect("debe aceptar nombre y defensor válidos");
        assert_eq!(nombre, "enero_2026");
        assert_eq!(defensor, "Dra. Ana Pérez");
    }

    #[test]
    fn test_normalizar_ambito_consolidacion_y_renombrado() {
        use super::normalizar_ambito_consolidacion;
        assert_eq!(normalizar_ambito_consolidacion(None), "*");
        assert_eq!(normalizar_ambito_consolidacion(Some("  ")), "*");
        assert_eq!(normalizar_ambito_consolidacion(Some(" * ")), "*");
        assert_eq!(
            normalizar_ambito_consolidacion(Some("Dr. José Daniel")),
            "jose daniel"
        );
        assert_eq!(
            normalizar_ambito_consolidacion(Some("Defensor Público")),
            "defensor publico"
        );
    }
}
