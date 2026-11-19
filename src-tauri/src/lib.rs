pub mod ai;
pub mod aprendizaje;
pub mod config;
pub mod estadistica_ia;
pub mod excel;
pub mod models;
pub mod paquete;
pub mod uso_catalogos;

use ai::{
    analizar_cluster_correcciones_ia, analizar_correccion_ia, extraer_datos_audiencia_ia,
    AiAudienciaOutput,
};
use aprendizaje::{
    aplicar_analisis_cluster_ia, aplicar_analisis_en, combinar_resumen_con_uso,
    descartar_aclaracion_en, exportar_aprendizaje_ia, guardar_nota_aclaracion_en,
    intentar_reservar_cluster_induccion, liberar_cluster_induccion,
    obtener_cluster_listo_para_inducir_ia, obtener_diagnostico_aprendizaje,
    obtener_resumen_aprendizaje, registrar_resultado_ia_con_traza, ruta_memoria,
    DiagnosticoAprendizaje, ResumenAprendizaje,
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
    guardar_color_carpeta, guardar_nueva_audiencia, guardar_orden_carpetas,
    leer_archivo_completo, listar_archivos, listar_carpetas, listar_respaldos, mover_archivo,
    obtener_colores_carpetas, obtener_estadisticas_globales, obtener_orden_carpetas,
    renombrar_carpeta, restaurar_respaldo, validar_color_carpeta, RespaldoInfo,
};
use std::collections::BTreeMap;
use models::{
    validar_audiencia, AudienciaInput, EstadisticasGlobales, RegistroAudiencia, TotalesExcel,
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

#[tauri::command]
async fn cmd_crear_archivo(
    nombre: String,
    defensor: Option<String>,
    carpeta: Option<String>,
) -> Result<String, String> {
    if nombre.trim().is_empty() {
        return Err("El nombre del archivo es obligatorio".to_string());
    }
    ejecutar_en_hilo_io(move || {
        crear_copia_plantilla(nombre.trim(), defensor.as_deref(), carpeta.as_deref())
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
    renombrar_defensor(&actual, &nuevo)
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

#[tauri::command]
async fn cmd_autocompletar_ia(texto: String) -> Result<AiAudienciaOutput, String> {
    let resultado = extraer_datos_audiencia_ia(&texto).await?;
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
) -> Result<ResumenAprendizaje, String> {
    let reglas_ids = reglas_inyectadas.unwrap_or_default();
    let (mut resumen, _) = ejecutar_en_hilo_io(move || {
        registrar_resultado_ia_con_traza(&texto, &prediccion, &final_usuario, &reglas_ids)
    })
    .await?;

    // Inducción diferida por clústeres: solo llama al LLM si hay >= 2 correcciones coherentes
    if let Some((cluster_id, corrs)) =
        ejecutar_en_hilo_io(|| Ok(obtener_cluster_listo_para_inducir_ia())).await?
    {
        if intentar_reservar_cluster_induccion(&cluster_id) {
            let _guard = ClusterInduccionGuard(cluster_id.clone());
            let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
            if let Ok(analisis) = analizar_cluster_correcciones_ia(&corrs).await {
                let cid_move = cluster_id.clone();
                if let Ok(actualizado) = ejecutar_en_hilo_io(move || {
                    aplicar_analisis_cluster_ia(&cid_move, &ids, &analisis)
                })
                .await
                {
                    resumen = actualizado;
                }
            }
        }
    }

    ejecutar_en_hilo_io(move || Ok(combinar_con_uso_actual(resumen))).await
}

#[tauri::command]
async fn cmd_responder_aclaracion_ia(
    correccion_id: String,
    nota: String,
) -> Result<ResumenAprendizaje, String> {
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
            cmd_exportar_aprendizaje_ia,
            cmd_exportar_paquete,
            cmd_inspeccionar_importacion,
            cmd_importar_paquete,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}
