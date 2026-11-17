pub mod ai;
pub mod config;
pub mod excel;
pub mod models;

use ai::{extraer_datos_audiencia_ia, AiAudienciaOutput};
use config::{cargar_settings, guardar_settings, obtener_estado_ia, AiStatusResponse, AppSettings};
use excel::{
    actualizar_audiencia_existente, crear_copia_plantilla, descargar_archivo, eliminar_archivo,
    eliminar_registro_audiencia, guardar_nueva_audiencia, leer_archivo_completo, listar_archivos,
};
use models::{validar_audiencia, AudienciaInput, RegistroAudiencia, TotalesExcel};
use serde::Serialize;

#[derive(Serialize)]
pub struct DatosArchivoResponse {
    pub registros: Vec<RegistroAudiencia>,
    pub totales: TotalesExcel,
}

#[derive(Serialize)]
pub struct ConfigCompletaResponse {
    pub settings: AppSettings,
    pub status: AiStatusResponse,
}

#[tauri::command]
fn cmd_listar_archivos() -> Result<Vec<String>, String> {
    listar_archivos()
}

#[tauri::command]
fn cmd_crear_archivo(nombre: String) -> Result<String, String> {
    if nombre.trim().is_empty() {
        return Err("El nombre del archivo es obligatorio".to_string());
    }
    crear_copia_plantilla(nombre.trim())
}

#[tauri::command]
fn cmd_eliminar_archivo(nombre: String) -> Result<(), String> {
    eliminar_archivo(&nombre)
}

#[tauri::command]
fn cmd_descargar_archivo(nombre: String) -> Result<String, String> {
    descargar_archivo(&nombre)
}

#[tauri::command]
fn cmd_leer_archivo(nombre: String) -> Result<DatosArchivoResponse, String> {
    let (registros, totales) = leer_archivo_completo(&nombre)?;
    Ok(DatosArchivoResponse { registros, totales })
}

#[tauri::command]
fn cmd_guardar_audiencia(
    nombre_archivo: String,
    datos: AudienciaInput,
    permitir_hora_vacia: bool,
) -> Result<TotalesExcel, String> {
    validar_audiencia(&datos, permitir_hora_vacia)?;
    guardar_nueva_audiencia(&nombre_archivo, &datos)
}

#[tauri::command]
fn cmd_actualizar_audiencia(
    nombre_archivo: String,
    fila_excel: u32,
    datos: AudienciaInput,
    permitir_hora_vacia: bool,
) -> Result<TotalesExcel, String> {
    validar_audiencia(&datos, permitir_hora_vacia)?;
    actualizar_audiencia_existente(&nombre_archivo, fila_excel, &datos)
}

#[tauri::command]
fn cmd_eliminar_registro(nombre_archivo: String, fila_excel: u32) -> Result<TotalesExcel, String> {
    eliminar_registro_audiencia(&nombre_archivo, fila_excel)
}

#[tauri::command]
fn cmd_obtener_config() -> Result<ConfigCompletaResponse, String> {
    let settings = cargar_settings();
    let status = obtener_estado_ia();
    Ok(ConfigCompletaResponse { settings, status })
}

#[tauri::command]
fn cmd_guardar_config(settings: AppSettings) -> Result<AiStatusResponse, String> {
    guardar_settings(&settings)?;
    Ok(obtener_estado_ia())
}

#[tauri::command]
async fn cmd_autocompletar_ia(texto: String) -> Result<AiAudienciaOutput, String> {
    extraer_datos_audiencia_ia(&texto).await
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
            cmd_guardar_audiencia,
            cmd_actualizar_audiencia,
            cmd_eliminar_registro,
            cmd_obtener_config,
            cmd_guardar_config,
            cmd_autocompletar_ia,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}
