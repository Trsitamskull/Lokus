pub mod ai;
pub mod aprendizaje;
pub mod config;
pub mod excel;
pub mod models;

use ai::{analizar_correccion_ia, extraer_datos_audiencia_ia, AiAudienciaOutput};
use aprendizaje::{
    aplicar_analisis_en, descartar_aclaracion_en, exportar_aprendizaje_ia,
    guardar_nota_aclaracion_en, obtener_pendientes_analisis_en, obtener_resumen_aprendizaje,
    registrar_resultado_ia, ruta_memoria, ResumenAprendizaje,
};
use config::{
    actualizar_config, actualizar_tema, agregar_defensor, agregar_juzgado, agregar_tipo_audiencia,
    cargar_config_publica, eliminar_defensor, eliminar_juzgado, eliminar_tipo_audiencia,
    renombrar_defensor, ActualizarConfig, ConfigPublica,
};
use excel::{
    actualizar_audiencia_existente, actualizar_defensor_archivo, crear_copia_plantilla,
    descargar_archivo, eliminar_archivo, eliminar_registro_audiencia, guardar_nueva_audiencia,
    leer_archivo_completo, listar_archivos, listar_respaldos, obtener_estadisticas_globales,
    restaurar_respaldo, RespaldoInfo,
};
use models::{
    validar_audiencia, AudienciaInput, EstadisticasGlobales, RegistroAudiencia, TotalesExcel,
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
async fn cmd_crear_archivo(nombre: String, defensor: Option<String>) -> Result<String, String> {
    if nombre.trim().is_empty() {
        return Err("El nombre del archivo es obligatorio".to_string());
    }
    ejecutar_en_hilo_io(move || crear_copia_plantilla(nombre.trim(), defensor.as_deref())).await
}

#[tauri::command]
async fn cmd_eliminar_archivo(nombre: String) -> Result<(), String> {
    ejecutar_en_hilo_io(move || eliminar_archivo(&nombre)).await
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

#[tauri::command]
async fn cmd_autocompletar_ia(texto: String) -> Result<AiAudienciaOutput, String> {
    let resultado = extraer_datos_audiencia_ia(&texto).await?;
    // Si quedó alguna corrección pendiente por falta de conexión previa, intentarla en segundo plano
    tauri::async_runtime::spawn(async move {
        let pendientes = obtener_pendientes_analisis_en(&ruta_memoria(), 1);
        for corr in pendientes {
            if let Ok(analisis) = analizar_correccion_ia(&corr).await {
                let _ = aplicar_analisis_en(&ruta_memoria(), &corr.id, &analisis);
            }
        }
    });
    Ok(resultado)
}

/// Compara lo que propuso la IA con lo que el usuario guardó, almacena la corrección si la hay
/// y ejecuta la 2ª llamada de análisis únicamente cuando existen diferencias.
#[tauri::command]
async fn cmd_registrar_resultado_ia(
    texto: String,
    prediccion: AudienciaInput,
    final_usuario: AudienciaInput,
) -> Result<ResumenAprendizaje, String> {
    let (mut resumen, nueva_correccion) =
        ejecutar_en_hilo_io(move || registrar_resultado_ia(&texto, &prediccion, &final_usuario))
            .await?;

    if let Some(corr) = nueva_correccion {
        if let Ok(analisis) = analizar_correccion_ia(&corr).await {
            let id = corr.id;
            if let Ok(actualizado) =
                ejecutar_en_hilo_io(move || aplicar_analisis_en(&ruta_memoria(), &id, &analisis))
                    .await
            {
                resumen = actualizado;
            }
        }
    }

    Ok(resumen)
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

    Ok(resumen)
}

#[tauri::command]
async fn cmd_descartar_aclaracion_ia(correccion_id: String) -> Result<ResumenAprendizaje, String> {
    ejecutar_en_hilo_io(move || descartar_aclaracion_en(&ruta_memoria(), &correccion_id)).await
}

#[tauri::command]
async fn cmd_resumen_aprendizaje_ia() -> Result<ResumenAprendizaje, String> {
    ejecutar_en_hilo_io(|| Ok(obtener_resumen_aprendizaje())).await
}

#[tauri::command]
async fn cmd_exportar_aprendizaje_ia() -> Result<String, String> {
    ejecutar_en_hilo_io(exportar_aprendizaje_ia).await
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
            cmd_crear_archivo,
            cmd_eliminar_archivo,
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
            cmd_exportar_aprendizaje_ia,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}
