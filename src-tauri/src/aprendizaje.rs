//! Aprendizaje continuo a partir de las correcciones del usuario sobre el autocompletado con IA.
//!
//! Flujo completo (Fases 1, 2 y 3):
//! 1. El frontend guarda una "foto" de lo que la IA dejó en el formulario (`prediccion`).
//! 2. Al registrar la audiencia envía esa foto junto con lo que el usuario guardó (`final_usuario`).
//! 3. Si no hay diferencias se cuenta un acierto; si las hay, la corrección se almacena en
//!    `ia_aprendizaje.json` (junto a `settings.json`) y se analiza en segundo plano con una 2ª llamada
//!    a la IA para extraer o reforzar una regla reutilizable.
//! 4. Si la IA no logra explicar el cambio con confianza, deja una pregunta breve opcional para el usuario.
//! 5. En cada nueva extracción se inyectan hasta 5 reglas y hasta 3 ejemplos similares (combinando la
//!    base embebida `resources/aprendizaje_base.json` con la memoria local del usuario).
//! 6. El usuario puede exportar todo el aprendizaje (anonimizado) a su carpeta de Descargas.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::{
    cargar_settings, es_nombre_defensor_mas_informativo, estado_desde_settings,
    ruta_en_carpeta_config, AppSettings,
};
use crate::excel::{
    cargar_o_inicializar_priors_en, cargar_priors_actuales, carpeta_descargas,
    obtener_carpeta_archivos, recolectar_radicados_en,
};
use crate::models::{
    coincide_alcance_defensor, es_alcance_general, es_solo_sigla_corta_juzgado,
    extraer_siglas_cortas_juzgado, normalizar_clave_catalogo, normalizar_clave_defensor,
    normalizar_texto_sin_tildes, AudienciaInput, GrafoAsociaciones, IndicePriorsExcel,
    TIPOS_AUDIENCIA,
};
use crate::uso_catalogos::{
    asegurar_uso_inicializado_actual, asegurar_uso_inicializado_en, cargar_uso_con_lock_en,
    confirmar_uso_exportado_en, construir_reporte_uso_catalogos, ruta_uso_catalogos,
    ReporteUsoCatalogos, UsoCatalogos,
};

pub const NOMBRE_ARCHIVO_MEMORIA: &str = "ia_aprendizaje.json";
/// Máximo de correcciones conservadas localmente (se descartan primero las más antiguas).
pub const MAX_CORRECCIONES: usize = 300;
/// Máximo de aciertos ancla conservados localmente en búfer circular anonimizado.
pub const MAX_ACIERTOS_EJEMPLOS: usize = 200;
/// Máximo de reglas aprendidas activas en memoria local.
pub const MAX_REGLAS: usize = 60;
/// Máximo de reglas inyectadas en el prompt de extracción.
pub const MAX_REGLAS_PROMPT: usize = 5;
/// Máximo de ejemplos similares inyectados en el prompt de extracción.
pub const MAX_EJEMPLOS_PROMPT: usize = 3;

/// Prior bayesiano escéptico Beta(1, 3): 25% de confianza media inicial antes de evidencia.
pub const PRIOR_BETA_ALPHA: f64 = 1.0;
pub const PRIOR_BETA_BETA: f64 = 3.0;
/// Prior calibrado para alias de juzgado/tipo: 1 corrección da q10≈0.25 (solo candidato en prompt),
/// 2 correcciones consistentes sin contradicción dan q10≈0.38 (>= 0.30, activación en la 2.ª vez).
pub const PRIOR_ALIAS_ALPHA: f64 = 1.0;
pub const PRIOR_ALIAS_BETA: f64 = 1.2;
/// Similitud mínima (texto casi idéntico) para mostrar 1 pista suave desde la 1.ª corrección pendiente del mismo defensor.
pub const UMBRAL_SIMILITUD_PISTA_PENDIENTE: f64 = 0.72;
/// Decaimiento por evento aplicable y tope de tamaño efectivo de muestra (ESS).
pub const DECAY_EVENTO_LAMBDA: f64 = 0.96;
pub const MAX_ESS_N: f64 = 40.0;
/// Ventana libre de decaimiento calendario para evitar olvido durante la vacancia judicial.
pub const DIAS_GRACIA_VACANCIA: f64 = 180.0;

/// Por debajo de esta similitud se considera que el usuario reescribió las observaciones.
const UMBRAL_SIMILITUD_OBSERVACIONES: f64 = 0.85;
/// Similitud mínima para considerar que dos reglas aprendidas son la misma y reforzarla.
const UMBRAL_REGLA_SIMILAR: f64 = 0.78;
/// Tamaño mínimo de un clúster coherente de errores antes de disparar inducción con LLM.
pub const MIN_TAMANO_CLUSTER_INDUCCION: usize = 2;
/// Tope de clústeres mantenidos en cola local.
const MAX_CLUSTERS_COLA: usize = 80;
/// Tope máximo de consolidaciones almacenadas en historial.
const MAX_CONSOLIDACIONES_HISTORIAL: usize = 12;
/// Tope aproximado de caracteres para el lote enviado al Consolidador LLM.
const MAX_CHARS_LOTE_CONSOLIDACION: usize = 8000;
const MAX_CHARS_RESUMEN_GENERAL: usize = 320;
const MAX_CHARS_RESUMEN_DEFENSOR: usize = 380;
const DEFAULT_SEMILLA_RNG: u64 = 0x5EED_CAFE_BABE_0001;
pub const VERSION_FORMATO: u32 = 5;
const PALABRAS_VACIAS_NOMBRE: [&str; 7] = ["de", "del", "la", "las", "los", "y", "san"];

pub const CAMPOS_AUDIENCIA: [&str; 8] = [
    "radicado",
    "tipo",
    "fecha",
    "hora",
    "juzgado",
    "realizada",
    "motivo",
    "observaciones",
];

const APRENDIZAJE_BASE_JSON: &str = include_str!("../resources/aprendizaje_base.json");

/// Serializa lecturas/escrituras del archivo de memoria entre comandos concurrentes.
static BLOQUEO_MEMORIA: Mutex<()> = Mutex::new(());
static CLUSTERS_EN_INDUCCION: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static CONSOLIDACION_EN_CURSO: AtomicBool = AtomicBool::new(false);
static CONTADOR_IDS: AtomicU64 = AtomicU64::new(1);

pub fn intentar_reservar_consolidacion() -> bool {
    CONSOLIDACION_EN_CURSO
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub fn liberar_consolidacion() {
    CONSOLIDACION_EN_CURSO.store(false, Ordering::Release);
}

pub(crate) fn intentar_reservar_cluster_induccion(cluster_id: &str) -> bool {
    let mut set = CLUSTERS_EN_INDUCCION
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    set.insert(cluster_id.to_string())
}

pub(crate) fn liberar_cluster_induccion(cluster_id: &str) {
    let mut set = CLUSTERS_EN_INDUCCION
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    set.remove(cluster_id);
}

fn generar_instancia_id_ia() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = CONTADOR_IDS.fetch_add(1, Ordering::Relaxed);
    format!("inst-ia-{:x}-{:04x}", nanos, seq % 0xffff)
}

fn recalcular_totales_desde_contadores(memoria: &mut MemoriaAprendizaje) {
    if memoria.contadores_por_instancia.is_empty() {
        return;
    }
    let (mut ac, mut ext, mut ind, mut con) = (0u32, 0u32, 0u32, 0u32);
    for c in memoria.contadores_por_instancia.values() {
        ac = ac.saturating_add(c.aciertos);
        ext = ext.saturating_add(c.llamadas_extraccion);
        ind = ind.saturating_add(c.llamadas_induccion);
        con = con.saturating_add(c.llamadas_consolidacion);
    }
    memoria.aciertos = ac;
    memoria.llamadas_extraccion_total = ext;
    memoria.llamadas_induccion_llm_total = ind;
    memoria.llamadas_consolidacion_total = con;
}

/// Garantiza `instancia_id` único para esta máquina/memoria y sincroniza el G-Counter
/// (`contadores_por_instancia`) con los totales escalares heredados o modificados.
pub fn asegurar_instancia_y_contadores(memoria: &mut MemoriaAprendizaje) -> String {
    if memoria.instancia_id.trim().is_empty() {
        memoria.instancia_id = generar_instancia_id_ia();
    } else {
        memoria.instancia_id = memoria.instancia_id.trim().to_string();
    }
    let inst_local = memoria.instancia_id.clone();

    let mut vist = BTreeSet::new();
    memoria.instancias_sincronizadas.retain(|s| {
        let t = s.trim();
        !t.is_empty() && t != inst_local && vist.insert(t.to_string())
    });

    let (mut sum_ac, mut sum_ext, mut sum_ind, mut sum_con) = (0u32, 0u32, 0u32, 0u32);
    for c in memoria.contadores_por_instancia.values() {
        sum_ac = sum_ac.saturating_add(c.aciertos);
        sum_ext = sum_ext.saturating_add(c.llamadas_extraccion);
        sum_ind = sum_ind.saturating_add(c.llamadas_induccion);
        sum_con = sum_con.saturating_add(c.llamadas_consolidacion);
    }

    if memoria.aciertos > sum_ac
        || memoria.llamadas_extraccion_total > sum_ext
        || memoria.llamadas_induccion_llm_total > sum_ind
        || memoria.llamadas_consolidacion_total > sum_con
    {
        let entry = memoria
            .contadores_por_instancia
            .entry(inst_local.clone())
            .or_default();
        if memoria.aciertos > sum_ac {
            entry.aciertos = entry.aciertos.saturating_add(memoria.aciertos - sum_ac);
        }
        if memoria.llamadas_extraccion_total > sum_ext {
            entry.llamadas_extraccion = entry
                .llamadas_extraccion
                .saturating_add(memoria.llamadas_extraccion_total - sum_ext);
        }
        if memoria.llamadas_induccion_llm_total > sum_ind {
            entry.llamadas_induccion = entry
                .llamadas_induccion
                .saturating_add(memoria.llamadas_induccion_llm_total - sum_ind);
        }
        if memoria.llamadas_consolidacion_total > sum_con {
            entry.llamadas_consolidacion = entry
                .llamadas_consolidacion
                .saturating_add(memoria.llamadas_consolidacion_total - sum_con);
        }
    }

    if !memoria.contadores_por_instancia.is_empty() {
        recalcular_totales_desde_contadores(memoria);
    }

    inst_local
}

fn incrementar_aciertos(memoria: &mut MemoriaAprendizaje, delta: u32) {
    let inst = asegurar_instancia_y_contadores(memoria);
    let entry = memoria.contadores_por_instancia.entry(inst).or_default();
    entry.aciertos = entry.aciertos.saturating_add(delta);
    recalcular_totales_desde_contadores(memoria);
}

fn incrementar_llamadas_extraccion(memoria: &mut MemoriaAprendizaje, delta: u32) {
    let inst = asegurar_instancia_y_contadores(memoria);
    let entry = memoria.contadores_por_instancia.entry(inst).or_default();
    entry.llamadas_extraccion = entry.llamadas_extraccion.saturating_add(delta);
    recalcular_totales_desde_contadores(memoria);
}

fn incrementar_llamadas_induccion(memoria: &mut MemoriaAprendizaje, delta: u32) {
    let inst = asegurar_instancia_y_contadores(memoria);
    let entry = memoria.contadores_por_instancia.entry(inst).or_default();
    entry.llamadas_induccion = entry.llamadas_induccion.saturating_add(delta);
    recalcular_totales_desde_contadores(memoria);
}

fn incrementar_llamadas_consolidacion(memoria: &mut MemoriaAprendizaje, delta: u32) {
    let inst = asegurar_instancia_y_contadores(memoria);
    let entry = memoria.contadores_por_instancia.entry(inst).or_default();
    entry.llamadas_consolidacion = entry.llamadas_consolidacion.saturating_add(delta);
    recalcular_totales_desde_contadores(memoria);
}

fn generar_id_unico(prefijo: &str) -> String {
    let seq = CONTADOR_IDS.fetch_add(1, Ordering::Relaxed);
    let stamp = Local::now().format("%Y%m%d%H%M%S%3f");
    if prefijo.is_empty() {
        format!("{}-{}", stamp, seq)
    } else {
        format!("{}-{}-{}", prefijo, stamp, seq)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CampoCorregido {
    pub campo: String,
    pub valor_ia: String,
    pub valor_usuario: String,
}

fn estado_regla_default_v1() -> String {
    String::new()
}
fn campo_general() -> String {
    "general".to_string()
}
fn origen_mitigacion() -> String {
    "mitigacion_modelo".to_string()
}
fn alcance_todos() -> String {
    "*".to_string()
}
fn prior_alpha_default() -> f64 {
    PRIOR_BETA_ALPHA
}
fn prior_beta_default() -> f64 {
    PRIOR_BETA_BETA
}
fn uno_u32() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReglaAprendida {
    pub id: String,
    pub tipo_error: String,
    pub regla: String,
    #[serde(default = "uno_u32")]
    pub refuerzos: u32,
    #[serde(default)]
    pub actualizada: String,
    /// `candidata` | `prueba` | `activa` | `cuarentena` | `retirada`
    #[serde(default = "estado_regla_default_v1")]
    pub estado: String,
    #[serde(default = "campo_general")]
    pub campo_objetivo: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default = "prior_alpha_default")]
    pub alpha: f64,
    #[serde(default = "prior_beta_default")]
    pub beta: f64,
    #[serde(default)]
    pub impresiones: u32,
    #[serde(default)]
    pub aciertos_directos: u32,
    #[serde(default)]
    pub fallos_directos: u32,
    /// `preferencia_usuario` | `mitigacion_modelo` | `base_invariante`
    #[serde(default = "origen_mitigacion")]
    pub origen: String,
    #[serde(default = "alcance_todos")]
    pub modelo_alcance: String,
    /// `"*"` = Cuaderno General compartido; o clave normalizada del defensor (`normalizar_clave_defensor`).
    #[serde(default = "alcance_todos")]
    pub defensor_alcance: String,
    #[serde(default)]
    pub defensores_confirmantes: Vec<String>,
    #[serde(default)]
    pub ultima_actualizacion: String,
    #[serde(default)]
    pub confianza_llm: f64,
    #[serde(default)]
    pub aciertos_sombra: u32,
    #[serde(default)]
    pub confirmaciones_sombra: u32,
    #[serde(default)]
    pub origen_correccion_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EjemploPositivo {
    pub id: String,
    pub registrada: String,
    pub proveedor: String,
    pub modelo: String,
    pub texto_anonimizado: String,
    pub salida_anonimizada: AudienciaInput,
    #[serde(default)]
    pub reglas_inyectadas: Vec<String>,
    #[serde(default)]
    pub defensor_clave: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MetricasCampo {
    pub campo: String,
    pub evaluaciones: u32,
    pub aciertos: u32,
    pub correcciones: u32,
    pub alpha: f64,
    pub beta: f64,
    pub precision_ewma: f64,
    pub ultima_actualizacion: String,
}

impl Default for MetricasCampo {
    fn default() -> Self {
        Self {
            campo: String::new(),
            evaluaciones: 0,
            aciertos: 0,
            correcciones: 0,
            alpha: PRIOR_BETA_ALPHA,
            beta: PRIOR_BETA_BETA,
            precision_ewma: 0.5,
            ultima_actualizacion: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct MetricasModelo {
    pub clave: String,
    pub proveedor: String,
    pub modelo: String,
    pub evaluaciones: u32,
    pub aciertos: u32,
    pub correcciones: u32,
    pub por_campo: BTreeMap<String, MetricasCampo>,
}

fn instalada_si_alpha_default() -> f64 {
    1.0
}
fn instalada_si_beta_default() -> f64 {
    2.5
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PreferenciasEstilo {
    pub incluir_causas_secundarias_alpha: f64,
    pub incluir_causas_secundarias_beta: f64,
    pub incluir_reprogramacion_alpha: f64,
    pub incluir_reprogramacion_beta: f64,
    pub se_fija_aplazamiento_alpha: f64,
    pub se_fija_aplazamiento_beta: f64,
    /// Probabilidad de que un defensor marque `SI` cuando la diligencia se instaló pero luego se suspendió/reprogramó.
    #[serde(default = "instalada_si_alpha_default")]
    pub instalada_reprogramada_si_alpha: f64,
    #[serde(default = "instalada_si_beta_default")]
    pub instalada_reprogramada_si_beta: f64,
}

impl Default for PreferenciasEstilo {
    fn default() -> Self {
        Self {
            incluir_causas_secundarias_alpha: 3.0,
            incluir_causas_secundarias_beta: 1.0,
            incluir_reprogramacion_alpha: 3.0,
            incluir_reprogramacion_beta: 1.0,
            se_fija_aplazamiento_alpha: 2.0,
            se_fija_aplazamiento_beta: 2.0,
            instalada_reprogramada_si_alpha: instalada_si_alpha_default(),
            instalada_reprogramada_si_beta: instalada_si_beta_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AliasEstadistico {
    pub campo: String,        // "juzgado" | "tipo"
    pub entrada_norm: String, // clave normalizada con ordinales unificados
    pub canonico: String,
    pub alpha: f64,
    pub beta: f64,
    #[serde(default = "alcance_todos")]
    pub defensor_alcance: String,
    #[serde(default)]
    pub defensores_confirmantes: Vec<String>,
    pub ultima_actualizacion: String,
}

impl Default for AliasEstadistico {
    fn default() -> Self {
        Self {
            campo: String::new(),
            entrada_norm: String::new(),
            canonico: String::new(),
            alpha: PRIOR_ALIAS_ALPHA,
            beta: PRIOR_ALIAS_BETA,
            defensor_alcance: "*".to_string(),
            defensores_confirmantes: Vec::new(),
            ultima_actualizacion: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PrediccionLocalResumen {
    pub realizada: Option<String>,
    pub realizada_confianza: f64,
    pub motivo: Option<String>,
    pub motivo_confianza: f64,
    pub juzgado_sugerido: Option<String>,
    pub tipo_sugerido: Option<String>,
    pub radicado_sugerido: Option<String>,
    pub muestras_entrenamiento: usize,
    pub realizadas_clases_vistas: usize,
    pub realizada_soporte_clase: usize,
    pub motivos_clases_vistas: usize,
    pub motivo_soporte_clase: usize,
    pub knn_realizada_acuerdo: bool,
    pub knn_motivo_acuerdo: bool,
    pub drift_realizada: bool,
    pub drift_motivo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AnalisisCorreccionIa {
    #[serde(default)]
    pub es_error_de_ia: bool,
    #[serde(default)]
    pub tipo_error: String,
    #[serde(default)]
    pub regla: String,
    #[serde(default)]
    pub confianza: f64,
    #[serde(default)]
    pub necesita_aclaracion: bool,
    #[serde(default)]
    pub pregunta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AclaracionPendiente {
    pub correccion_id: String,
    pub pregunta: String,
    pub campos: Vec<CampoCorregido>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorreccionIa {
    pub id: String,
    pub registrada: String,
    pub proveedor: String,
    pub modelo: String,
    pub texto_original: String,
    pub prediccion: AudienciaInput,
    pub final_usuario: AudienciaInput,
    pub campos: Vec<CampoCorregido>,
    /// `pendiente` | `analizada` | `descartada` | `esperando_aclaracion`
    #[serde(default = "estado_pendiente")]
    pub estado: String,
    #[serde(default)]
    pub regla_generada: String,
    #[serde(default)]
    pub regla_id_asociada: String,
    #[serde(default)]
    pub pregunta_ia: String,
    #[serde(default)]
    pub nota_usuario: String,
    #[serde(default)]
    pub exportada: bool,
    #[serde(default)]
    pub defensor_clave: String,
}

fn estado_pendiente() -> String {
    "pendiente".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct EstadoPageHinkley {
    pub campo: String,
    pub n_obs: u32,
    pub media_error: f64,
    pub suma_desviacion: f64,
    pub min_suma_desviacion: f64,
    pub drift_detectado: bool,
    pub ultimo_drift: String,
    pub eventos_desde_drift: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ClusterCorreccion {
    pub id: String,
    pub campo_principal: String,
    pub firma_error: String,
    pub correccion_ids: Vec<String>,
    pub primera_vez: String,
    pub ultima_vez: String,
    /// `acumulando` | `listo` | `inducido`
    pub estado: String,
    pub regla_inducida_id: String,
    #[serde(default)]
    pub defensor_clave: String,
    #[serde(default)]
    pub defensores_presentes: Vec<String>,
}

fn default_alpha_resp() -> f64 {
    2.0
}
fn default_beta_omit() -> f64 {
    1.0
}
fn default_semilla_rng() -> u64 {
    DEFAULT_SEMILLA_RNG
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct MetricasDefensor {
    pub nombre_mostrar: String,
    pub aciertos: u32,
    pub correcciones: u32,
    pub por_campo: BTreeMap<String, MetricasCampo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PerfilSinteticoDefensor {
    pub ambito: String,
    pub nombre_mostrar: String,
    pub resumen: String,
    pub version: u32,
    pub ultima_actualizacion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct AccionReglaConsolidacion {
    pub tipo_accion: String,
    pub ids_origen: Vec<String>,
    pub tipo_error: String,
    pub campo_objetivo: String,
    pub regla: String,
    pub triggers: Vec<String>,
    pub motivo: String,
    pub confianza: f64,
    pub aplicada: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct SugerenciaBuild {
    pub categoria: String,
    pub ambito: String,
    pub detalle: String,
    pub evidencia: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct TelemetriaDisparoConsolidacion {
    pub ambito: String,
    pub presion: f64,
    pub umbral_adaptativo: f64,
    pub probabilidad_disparo: f64,
    pub muestra_rng: f64,
    pub manual: bool,
    pub razones: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct RegistroConsolidacionIa {
    pub id: String,
    pub fecha: String,
    pub ambito: String,
    pub nombre_ambito: String,
    pub modelo: String,
    pub acciones_reglas: Vec<AccionReglaConsolidacion>,
    pub resumen_actualizado: String,
    pub sugerencias_build: Vec<SugerenciaBuild>,
    pub telemetria_disparo: TelemetriaDisparoConsolidacion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct EstadoConsolidacion {
    pub presion: f64,
    pub correcciones_sin_consolidar: u32,
    pub notas_sin_consolidar: u32,
    pub extracciones_desde_ultima: u32,
    pub ultima_consolidacion: String,
    pub ids_consolidadas_recientes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EstadoAclaracionAdaptativa {
    #[serde(default = "default_alpha_resp")]
    pub alpha_resp: f64,
    #[serde(default = "default_beta_omit")]
    pub beta_omit: f64,
    pub racha_omisiones: u32,
    pub turnos_sin_responder: u32,
    pub firmas_explicadas_recientes: Vec<(String, u64)>,
}

impl Default for EstadoAclaracionAdaptativa {
    fn default() -> Self {
        Self {
            alpha_resp: default_alpha_resp(),
            beta_omit: default_beta_omit(),
            racha_omisiones: 0,
            turnos_sin_responder: 0,
            firmas_explicadas_recientes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PatronTransversalExport {
    pub firma: String,
    pub campo: String,
    pub ocurrencias: usize,
    pub defensores_distintos: usize,
    pub ejemplo_anonimizado: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ResumenPatronesGenerales {
    pub total_defensores_activos: usize,
    pub top_juzgados_globales: Vec<(String, u32)>,
    pub top_tipos_globales: Vec<(String, u32)>,
    pub top_horas_globales: Vec<(u32, u32)>,
    pub reglas_transversales_promovidas: usize,
    pub errores_recurrentes_transversales: Vec<PatronTransversalExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ResumenDefensorExport {
    pub clave: String,
    pub nombre: String,
    pub aciertos: u32,
    pub correcciones: u32,
    pub tasa_acierto: f64,
    pub por_campo: Vec<CampoDiagnostico>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PresionDefensorDiagnostico {
    pub clave: String,
    pub nombre: String,
    pub presion: f64,
    pub correcciones_sin_consolidar: u32,
    pub notas_sin_consolidar: u32,
    pub extracciones_desde_ultima: u32,
    pub ultima_consolidacion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct AccionConsolidacionInput {
    pub tipo_accion: String,
    pub ids_origen: Vec<String>,
    pub tipo_error: String,
    pub campo_objetivo: String,
    pub regla: String,
    pub triggers: Vec<String>,
    pub motivo: String,
    pub confianza: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct SugerenciaBuildInput {
    pub categoria: String,
    pub detalle: String,
    pub evidencia: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ResultadoConsolidacionIa {
    pub acciones_reglas: Vec<AccionConsolidacionInput>,
    pub resumen_actualizado: String,
    pub sugerencias_build: Vec<SugerenciaBuildInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ReglaLoteConsolidacion {
    pub id: String,
    pub estado: String,
    pub origen: String,
    pub campo_objetivo: String,
    pub tipo_error: String,
    pub defensor_alcance: String,
    pub regla: String,
    pub triggers: Vec<String>,
    pub confianza_q10: f64,
    pub impresiones: u32,
    pub aciertos_directos: u32,
    pub fallos_directos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct CorreccionLoteConsolidacion {
    pub id: String,
    pub fecha: String,
    pub defensor_clave: String,
    pub campo_principal: String,
    pub texto_anonimizado: String,
    pub cambios: Vec<CampoCorregido>,
    pub nota_usuario: String,
    pub estado: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct LoteConsolidacion {
    pub ambito: String,
    pub nombre_ambito: String,
    pub resumen_actual: String,
    pub version_resumen: u32,
    pub reglas_ambito: Vec<ReglaLoteConsolidacion>,
    pub correcciones_recientes: Vec<CorreccionLoteConsolidacion>,
    pub metricas_resumen: Vec<CampoDiagnostico>,
    pub patrones_transversales: Vec<PatronTransversalExport>,
    pub watermark_ids: Vec<String>,
    pub snapshot_extracciones: u32,
    pub telemetria_disparo: TelemetriaDisparoConsolidacion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ContadoresInstanciaIa {
    pub aciertos: u32,
    pub llamadas_extraccion: u32,
    pub llamadas_induccion: u32,
    pub llamadas_consolidacion: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct InspeccionFusionIa {
    pub reglas_nuevas: usize,
    pub reglas_actualizables: usize,
    pub correcciones_nuevas: usize,
    pub ejemplos_nuevos: usize,
    pub alias_nuevos: usize,
    pub perfiles_nuevos_o_actualizados: usize,
    pub consolidaciones_nuevas: usize,
    pub defensores_nuevos: usize,
    pub ya_sincronizado: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ResultadoFusionIa {
    pub reglas_nuevas: usize,
    pub reglas_actualizadas: usize,
    pub correcciones_nuevas: usize,
    pub ejemplos_nuevos: usize,
    pub alias_nuevos: usize,
    pub alias_actualizados: usize,
    pub perfiles_actualizados: usize,
    pub consolidaciones_nuevas: usize,
    pub defensores_nuevos: usize,
    pub cambios_totales: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoriaAprendizaje {
    pub version: u32,
    pub aciertos: u32,
    pub reglas: Vec<ReglaAprendida>,
    pub correcciones: Vec<CorreccionIa>,
    #[serde(default)]
    pub ejemplos_aciertos: Vec<EjemploPositivo>,
    #[serde(default)]
    pub metricas_campos: BTreeMap<String, MetricasCampo>,
    #[serde(default)]
    pub metricas_modelos: BTreeMap<String, MetricasModelo>,
    #[serde(default)]
    pub metricas_por_defensor: BTreeMap<String, MetricasDefensor>,
    #[serde(default)]
    pub nombres_defensores: BTreeMap<String, String>,
    #[serde(default)]
    pub preferencias_estilo: PreferenciasEstilo,
    #[serde(default)]
    pub preferencias_por_defensor: BTreeMap<String, PreferenciasEstilo>,
    #[serde(default)]
    pub perfiles_sinteticos: BTreeMap<String, PerfilSinteticoDefensor>,
    #[serde(default)]
    pub estados_consolidacion: BTreeMap<String, EstadoConsolidacion>,
    #[serde(default)]
    pub consolidaciones_ia: Vec<RegistroConsolidacionIa>,
    #[serde(default)]
    pub estado_aclaracion: EstadoAclaracionAdaptativa,
    #[serde(default)]
    pub tabla_alias: Vec<AliasEstadistico>,
    #[serde(default)]
    pub cola_induccion: Vec<ClusterCorreccion>,
    #[serde(default)]
    pub page_hinkley_campos: BTreeMap<String, EstadoPageHinkley>,
    #[serde(default)]
    pub page_hinkley_modelos: BTreeMap<String, BTreeMap<String, EstadoPageHinkley>>,
    #[serde(default)]
    pub enfriamiento_aclaracion: u32,
    #[serde(default)]
    pub llamadas_induccion_llm_total: u32,
    #[serde(default)]
    pub llamadas_extraccion_total: u32,
    #[serde(default)]
    pub llamadas_consolidacion_total: u32,
    #[serde(default = "default_semilla_rng")]
    pub semilla_rng: u64,
    #[serde(default)]
    pub exportacion_seq: u64,
    #[serde(default)]
    pub ultima_exportacion_previa: String,
    #[serde(default)]
    pub instancia_id: String,
    #[serde(default)]
    pub contadores_por_instancia: BTreeMap<String, ContadoresInstanciaIa>,
    #[serde(default)]
    pub instancias_sincronizadas: Vec<String>,
}

impl Default for MemoriaAprendizaje {
    fn default() -> Self {
        Self {
            version: VERSION_FORMATO,
            aciertos: 0,
            reglas: Vec::new(),
            correcciones: Vec::new(),
            ejemplos_aciertos: Vec::new(),
            metricas_campos: BTreeMap::new(),
            metricas_modelos: BTreeMap::new(),
            metricas_por_defensor: BTreeMap::new(),
            nombres_defensores: BTreeMap::new(),
            preferencias_estilo: PreferenciasEstilo::default(),
            preferencias_por_defensor: BTreeMap::new(),
            perfiles_sinteticos: BTreeMap::new(),
            estados_consolidacion: BTreeMap::new(),
            consolidaciones_ia: Vec::new(),
            estado_aclaracion: EstadoAclaracionAdaptativa::default(),
            tabla_alias: Vec::new(),
            cola_induccion: Vec::new(),
            page_hinkley_campos: BTreeMap::new(),
            page_hinkley_modelos: BTreeMap::new(),
            enfriamiento_aclaracion: 0,
            llamadas_induccion_llm_total: 0,
            llamadas_extraccion_total: 0,
            llamadas_consolidacion_total: 0,
            semilla_rng: DEFAULT_SEMILLA_RNG,
            exportacion_seq: 0,
            ultima_exportacion_previa: String::new(),
            instancia_id: String::new(),
            contadores_por_instancia: BTreeMap::new(),
            instancias_sincronizadas: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumenAprendizaje {
    pub aciertos: u32,
    pub total_correcciones: usize,
    pub total_reglas: usize,
    pub pendientes_exportar: usize,
    #[serde(default)]
    pub pendientes_ia: usize,
    #[serde(default)]
    pub pendientes_uso: usize,
    #[serde(default)]
    pub total_audiencias_contabilizadas: u32,
    #[serde(default)]
    pub puede_exportar: bool,
    pub hubo_correccion: bool,
    pub aclaracion_pendiente: Option<AclaracionPendiente>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct CampoDiagnostico {
    pub campo: String,
    pub evaluaciones: u32,
    pub aciertos: u32,
    pub correcciones: u32,
    pub precision_ewma: f64,
    pub confianza_q10: f64,
    pub drift_activo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ModeloDiagnostico {
    pub clave: String,
    pub evaluaciones: u32,
    pub aciertos: u32,
    pub correcciones: u32,
    pub precision_media: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ReglaDiagnostico {
    pub id: String,
    pub estado: String,
    pub campo_objetivo: String,
    pub origen: String,
    #[serde(default = "alcance_todos")]
    pub defensor_alcance: String,
    pub regla: String,
    pub impresiones: u32,
    pub aciertos_directos: u32,
    pub fallos_directos: u32,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct AliasDiagnostico {
    pub campo: String,
    pub entrada_norm: String,
    pub canonico: String,
    #[serde(default = "alcance_todos")]
    pub defensor_alcance: String,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PatronAbreviacionDiagnostico {
    pub campo: String,
    pub tipo_slot: String,
    pub patron_entrada: String,
    pub patron_canonico: String,
    #[serde(default = "alcance_todos")]
    pub defensor_alcance: String,
    pub ejemplos_distintos: usize,
    pub total_instancias: usize,
    pub consistencia: f64,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PreferenciasEstiloResumen {
    pub incluir_causas_secundarias_prob: f64,
    pub incluir_reprogramacion_prob: f64,
    pub se_fija_aplazamiento_prob: f64,
    #[serde(default)]
    pub instalada_reprogramada_si_prob: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DiagnosticoAprendizaje {
    pub aciertos_totales: u32,
    pub correcciones_totales: usize,
    pub ejemplos_positivos_totales: usize,
    pub llamadas_induccion_llm_total: u32,
    pub clusters_pendientes: usize,
    pub clusters_listos: usize,
    #[serde(default)]
    pub perfiles_defensores: usize,
    #[serde(default)]
    pub conexiones_grafo: usize,
    pub drift_campos_activos: Vec<String>,
    pub enfriamiento_aclaracion: u32,
    pub reglas_por_estado: BTreeMap<String, usize>,
    pub metricas_por_campo: Vec<CampoDiagnostico>,
    pub metricas_por_modelo: Vec<ModeloDiagnostico>,
    pub preferencias_estilo: PreferenciasEstiloResumen,
    pub alias_validados: Vec<AliasDiagnostico>,
    #[serde(default)]
    pub patrones_abreviacion: Vec<PatronAbreviacionDiagnostico>,
    pub reglas_destacadas: Vec<ReglaDiagnostico>,
    #[serde(default)]
    pub consolidaciones_totales: u32,
    #[serde(default)]
    pub presion_consolidacion_global: f64,
    #[serde(default)]
    pub presiones_por_defensor: Vec<PresionDefensorDiagnostico>,
    #[serde(default)]
    pub ultima_consolidacion: Option<RegistroConsolidacionIa>,
    #[serde(default)]
    pub perfiles_sinteticos: Vec<PerfilSinteticoDefensor>,
    #[serde(default)]
    pub tasa_respuesta_aclaraciones: f64,
    #[serde(default)]
    pub defensores_resumen: Vec<ResumenDefensorExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaqueteExportacion {
    pub app: String,
    pub version_app: String,
    pub formato: u32,
    pub exportado: String,
    #[serde(default)]
    pub instancia_id: String,
    #[serde(default)]
    pub exportacion_seq: u64,
    #[serde(default)]
    pub exportacion_id: String,
    #[serde(default)]
    pub ultima_exportacion_previa: String,
    #[serde(default)]
    pub fuentes_importadas: Vec<String>,
    pub aciertos: u32,
    pub total_reglas: usize,
    pub total_correcciones: usize,
    pub total_ejemplos_aciertos: usize,
    pub perfiles_defensores: usize,
    pub llamadas_induccion_llm_total: u32,
    pub llamadas_extraccion_total: u32,
    pub llamadas_consolidacion_total: u32,
    pub correcciones_por_campo: BTreeMap<String, usize>,
    pub metricas_por_campo: Vec<CampoDiagnostico>,
    pub metricas_por_modelo: Vec<ModeloDiagnostico>,
    pub preferencias_estilo: PreferenciasEstiloResumen,
    pub preferencias_por_defensor: BTreeMap<String, PreferenciasEstiloResumen>,
    #[serde(default)]
    pub preferencias_raw: Option<PreferenciasEstilo>,
    #[serde(default)]
    pub preferencias_por_defensor_raw: BTreeMap<String, PreferenciasEstilo>,
    pub defensores_directorio: Vec<ResumenDefensorExport>,
    pub resumen_patrones_generales: ResumenPatronesGenerales,
    pub grafo_asociaciones: GrafoAsociaciones,
    pub perfiles_sinteticos: Vec<PerfilSinteticoDefensor>,
    pub consolidaciones_ia: Vec<RegistroConsolidacionIa>,
    pub alias_validados: Vec<AliasDiagnostico>,
    #[serde(default)]
    pub tabla_alias_raw: Vec<AliasEstadistico>,
    pub patrones_abreviacion: Vec<PatronAbreviacionDiagnostico>,
    pub juzgados_agregados_usuario: Vec<String>,
    pub tipos_audiencia_agregados_usuario: Vec<String>,
    pub uso_catalogos: ReporteUsoCatalogos,
    pub reglas: Vec<ReglaAprendida>,
    pub correcciones: Vec<CorreccionIa>,
    #[serde(default)]
    pub ejemplos_aciertos: Vec<EjemploPositivo>,
}

pub(crate) fn memoria_base_embebida() -> &'static MemoriaAprendizaje {
    static BASE: OnceLock<MemoriaAprendizaje> = OnceLock::new();
    BASE.get_or_init(|| {
        let mut m = serde_json::from_str::<MemoriaAprendizaje>(APRENDIZAJE_BASE_JSON)
            .unwrap_or_default();
        migrar_memoria_v1_a_v2(&mut m);
        m
    })
}

pub(crate) fn cargar_memoria_local_snapshot() -> MemoriaAprendizaje {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    cargar_memoria_en(&ruta_memoria())
}

pub(crate) fn inferir_campo_objetivo_desde_tipo(tipo_error: &str) -> String {
    match tipo_error.trim().to_lowercase().as_str() {
        "abreviacion" | "juzgado" | "despacho" => "juzgado".to_string(),
        "realizacion" | "realizada" => "realizada".to_string(),
        "motivo" => "motivo".to_string(),
        "fecha" => "fecha".to_string(),
        "hora" => "hora".to_string(),
        "tipo" | "tipo_audiencia" => "tipo".to_string(),
        "radicado" => "radicado".to_string(),
        "estilo_observaciones" | "observaciones" | "observacion" => "observaciones".to_string(),
        _ => "general".to_string(),
    }
}

fn recortar_sin_elipsis(s: &str, max_chars: usize) -> String {
    let limpio = colapsar_espacios(s);
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = limpio.chars().collect();
    if chars.len() <= max_chars {
        limpio
    } else if max_chars > 3 {
        let prefijo: String = chars[..(max_chars - 3)].iter().collect();
        format!("{}...", prefijo.trim_end())
    } else {
        chars[..max_chars].iter().collect()
    }
}

fn title_case_basico(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                None => String::new(),
                Some(first) => {
                    let mut out: String = first.to_uppercase().collect();
                    out.push_str(cs.as_str());
                    out
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn registrar_nombre_defensor(nombres: &mut BTreeMap<String, String>, raw: &str) {
    let limpio = colapsar_espacios(raw);
    if limpio.is_empty() || es_alcance_general(&limpio) {
        return;
    }
    let clave = normalizar_clave_defensor(&limpio);
    if clave.is_empty() {
        return;
    }
    let candidato = if limpio == normalizar_texto_sin_tildes(&limpio) {
        title_case_basico(&clave)
    } else {
        limpio
    };
    match nombres.get_mut(&clave) {
        Some(actual) => {
            if es_nombre_defensor_mas_informativo(&candidato, actual) {
                *actual = candidato;
            }
        }
        None => {
            nombres.insert(clave, candidato);
        }
    }
}

pub(crate) fn alcance_canonico(alcance: &str) -> String {
    if es_alcance_general(alcance) {
        "*".to_string()
    } else {
        let norm = normalizar_clave_defensor(alcance);
        if norm.is_empty() {
            "*".to_string()
        } else {
            norm
        }
    }
}

/// Devuelve las preferencias efectivas para un defensor dado combinando `base` y `local`
/// (heredando del Cuaderno General cuando el defensor aún no tiene perfil específico).
pub(crate) fn preferencias_efectivas_con_base_para_defensor(
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    defensor: Option<&str>,
) -> PreferenciasEstilo {
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let pref_base_gen = &base.preferencias_estilo;
    let pref_local_gen = &local.preferencias_estilo;
    let def_estilo = PreferenciasEstilo::default();

    let elegir_par = |la: f64, lb: f64, ba: f64, bb: f64, da: f64, db: f64| -> (f64, f64) {
        if (la - da).abs() > 1e-6 || (lb - db).abs() > 1e-6 {
            (la, lb)
        } else if (ba - da).abs() > 1e-6 || (bb - db).abs() > 1e-6 {
            (ba, bb)
        } else {
            (da, db)
        }
    };

    let mezclar_sobre = |superior: &PreferenciasEstilo, base_actual: &PreferenciasEstilo| -> PreferenciasEstilo {
        let mut out = PreferenciasEstilo::default();
        (
            out.incluir_causas_secundarias_alpha,
            out.incluir_causas_secundarias_beta,
        ) = elegir_par(
            superior.incluir_causas_secundarias_alpha,
            superior.incluir_causas_secundarias_beta,
            base_actual.incluir_causas_secundarias_alpha,
            base_actual.incluir_causas_secundarias_beta,
            def_estilo.incluir_causas_secundarias_alpha,
            def_estilo.incluir_causas_secundarias_beta,
        );
        (
            out.incluir_reprogramacion_alpha,
            out.incluir_reprogramacion_beta,
        ) = elegir_par(
            superior.incluir_reprogramacion_alpha,
            superior.incluir_reprogramacion_beta,
            base_actual.incluir_reprogramacion_alpha,
            base_actual.incluir_reprogramacion_beta,
            def_estilo.incluir_reprogramacion_alpha,
            def_estilo.incluir_reprogramacion_beta,
        );
        (
            out.se_fija_aplazamiento_alpha,
            out.se_fija_aplazamiento_beta,
        ) = elegir_par(
            superior.se_fija_aplazamiento_alpha,
            superior.se_fija_aplazamiento_beta,
            base_actual.se_fija_aplazamiento_alpha,
            base_actual.se_fija_aplazamiento_beta,
            def_estilo.se_fija_aplazamiento_alpha,
            def_estilo.se_fija_aplazamiento_beta,
        );
        (
            out.instalada_reprogramada_si_alpha,
            out.instalada_reprogramada_si_beta,
        ) = elegir_par(
            superior.instalada_reprogramada_si_alpha,
            superior.instalada_reprogramada_si_beta,
            base_actual.instalada_reprogramada_si_alpha,
            base_actual.instalada_reprogramada_si_beta,
            def_estilo.instalada_reprogramada_si_alpha,
            def_estilo.instalada_reprogramada_si_beta,
        );
        out
    };

    let mut combinada = mezclar_sobre(pref_local_gen, pref_base_gen);

    if !def_clave.is_empty() {
        let local_def = local
            .preferencias_por_defensor
            .iter()
            .find(|(k, _)| coincide_alcance_defensor(k, &def_clave))
            .map(|(_, v)| v);
        let base_def = base
            .preferencias_por_defensor
            .iter()
            .find(|(k, _)| coincide_alcance_defensor(k, &def_clave))
            .map(|(_, v)| v);

        if let Some(p_bas) = base_def {
            combinada = mezclar_sobre(p_bas, &combinada);
        }
        if let Some(p_loc) = local_def {
            combinada = mezclar_sobre(p_loc, &combinada);
        }
    }
    combinada
}

/// Devuelve las preferencias efectivas para un defensor dado (heredando del Cuaderno General si no tiene perfil propio aún).
#[allow(dead_code)]
pub(crate) fn preferencias_efectivas_para_defensor(
    memoria: &MemoriaAprendizaje,
    defensor: Option<&str>,
) -> PreferenciasEstilo {
    preferencias_efectivas_con_base_para_defensor(
        &MemoriaAprendizaje::default(),
        memoria,
        defensor,
    )
}

/// Devuelve `(resumen_general_transversal <= 320 chars, resumen_especifico_defensor <= 380 chars)`
/// con precedencia de `local` sobre `base`.
pub(crate) fn obtener_resumenes_consolidados_prompt(
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    defensor: Option<&str>,
) -> (Option<String>, Option<String>) {
    let buscar_resumen = |mem: &MemoriaAprendizaje, clave: &str| -> Option<String> {
        mem.perfiles_sinteticos
            .get(clave)
            .or_else(|| {
                if clave == "*" {
                    None
                } else {
                    mem.perfiles_sinteticos
                        .iter()
                        .find(|(k, _)| coincide_alcance_defensor(k, clave))
                        .map(|(_, v)| v)
                }
            })
            .map(|p| colapsar_espacios(&p.resumen))
            .filter(|s| !s.is_empty())
    };

    let general = buscar_resumen(local, "*")
        .or_else(|| buscar_resumen(base, "*"))
        .map(|s| recortar_sin_elipsis(&s, MAX_CHARS_RESUMEN_GENERAL));

    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let especifico = if def_clave.is_empty() {
        None
    } else {
        buscar_resumen(local, &def_clave)
            .or_else(|| buscar_resumen(base, &def_clave))
            .map(|s| recortar_sin_elipsis(&s, MAX_CHARS_RESUMEN_DEFENSOR))
    };

    (general, especifico)
}

pub(crate) fn construir_regla_v2(
    id: String,
    tipo_error: String,
    regla: String,
    refuerzos: u32,
    actualizada: String,
    estado: &str,
    campo_objetivo: String,
    triggers: Vec<String>,
    origen: &str,
    modelo_alcance: String,
    confianza_llm: f64,
    origen_correccion_id: String,
) -> ReglaAprendida {
    let alpha = PRIOR_BETA_ALPHA + (refuerzos.max(1) as f64);
    let beta = PRIOR_BETA_BETA;
    ReglaAprendida {
        id,
        tipo_error,
        regla,
        refuerzos,
        actualizada: actualizada.clone(),
        estado: estado.to_string(),
        campo_objetivo,
        triggers,
        alpha,
        beta,
        impresiones: 0,
        aciertos_directos: 0,
        fallos_directos: 0,
        origen: origen.to_string(),
        modelo_alcance,
        defensor_alcance: "*".to_string(),
        defensores_confirmantes: Vec::new(),
        ultima_actualizacion: actualizada,
        confianza_llm,
        aciertos_sombra: 0,
        confirmaciones_sombra: 0,
        origen_correccion_id,
    }
}

fn prioridad_estado_regla(s: &str) -> u8 {
    match s {
        "activa" => 4,
        "prueba" => 3,
        "candidata" => 2,
        "cuarentena" => 1,
        _ => 0,
    }
}

fn fusionar_regla_en(dest: &mut ReglaAprendida, otra: ReglaAprendida) {
    dest.refuerzos = dest.refuerzos.saturating_add(otra.refuerzos);
    dest.alpha = (dest.alpha + (otra.alpha - PRIOR_BETA_ALPHA).max(0.0)).max(0.5);
    dest.beta = (dest.beta + (otra.beta - PRIOR_BETA_BETA).max(0.0)).max(0.5);
    normalizar_ess(&mut dest.alpha, &mut dest.beta);
    dest.impresiones = dest.impresiones.saturating_add(otra.impresiones);
    dest.aciertos_directos = dest.aciertos_directos.saturating_add(otra.aciertos_directos);
    dest.fallos_directos = dest.fallos_directos.saturating_add(otra.fallos_directos);
    dest.aciertos_sombra = dest.aciertos_sombra.saturating_add(otra.aciertos_sombra);
    dest.confirmaciones_sombra = dest
        .confirmaciones_sombra
        .saturating_add(otra.confirmaciones_sombra);
    if otra.regla.len() > dest.regla.len() && otra.regla.len() <= 240 {
        dest.regla = otra.regla;
    }
    if prioridad_estado_regla(&otra.estado) > prioridad_estado_regla(&dest.estado) {
        dest.estado = otra.estado;
    }
    for t in otra.triggers {
        if !dest.triggers.contains(&t) && dest.triggers.len() < 12 {
            dest.triggers.push(t);
        }
    }
    for d in otra.defensores_confirmantes {
        let dk = normalizar_clave_defensor(&d);
        if !dk.is_empty() && !dest.defensores_confirmantes.contains(&dk) {
            dest.defensores_confirmantes.push(dk);
        }
    }
    if otra.ultima_actualizacion > dest.ultima_actualizacion {
        dest.ultima_actualizacion = otra.ultima_actualizacion;
    }
    if otra.actualizada > dest.actualizada {
        dest.actualizada = otra.actualizada;
    }
    if otra.confianza_llm > dest.confianza_llm {
        dest.confianza_llm = otra.confianza_llm;
    }
}

fn fusionar_preferencias_estilo(destino: &mut PreferenciasEstilo, otra: &PreferenciasEstilo) {
    const MAX_ESS_PREF: f64 = 12.0;
    let def = PreferenciasEstilo::default();
    let sum_par = |da: &mut f64, db: &mut f64, oa: f64, ob: f64, pa: f64, pb: f64| {
        *da = (*da + (oa - pa).max(0.0)).max(0.5);
        *db = (*db + (ob - pb).max(0.0)).max(0.5);
        normalizar_ess_par(da, db, MAX_ESS_PREF);
    };
    sum_par(
        &mut destino.incluir_causas_secundarias_alpha,
        &mut destino.incluir_causas_secundarias_beta,
        otra.incluir_causas_secundarias_alpha,
        otra.incluir_causas_secundarias_beta,
        def.incluir_causas_secundarias_alpha,
        def.incluir_causas_secundarias_beta,
    );
    sum_par(
        &mut destino.incluir_reprogramacion_alpha,
        &mut destino.incluir_reprogramacion_beta,
        otra.incluir_reprogramacion_alpha,
        otra.incluir_reprogramacion_beta,
        def.incluir_reprogramacion_alpha,
        def.incluir_reprogramacion_beta,
    );
    sum_par(
        &mut destino.se_fija_aplazamiento_alpha,
        &mut destino.se_fija_aplazamiento_beta,
        otra.se_fija_aplazamiento_alpha,
        otra.se_fija_aplazamiento_beta,
        def.se_fija_aplazamiento_alpha,
        def.se_fija_aplazamiento_beta,
    );
    sum_par(
        &mut destino.instalada_reprogramada_si_alpha,
        &mut destino.instalada_reprogramada_si_beta,
        otra.instalada_reprogramada_si_alpha,
        otra.instalada_reprogramada_si_beta,
        def.instalada_reprogramada_si_alpha,
        def.instalada_reprogramada_si_beta,
    );
}

fn fusionar_metricas_defensor(destino: &mut MetricasDefensor, otra: MetricasDefensor) {
    if !otra.nombre_mostrar.trim().is_empty()
        && (destino.nombre_mostrar.trim().is_empty()
            || es_nombre_defensor_mas_informativo(&otra.nombre_mostrar, &destino.nombre_mostrar))
    {
        destino.nombre_mostrar = otra.nombre_mostrar;
    }
    destino.aciertos = destino.aciertos.saturating_add(otra.aciertos);
    destino.correcciones = destino.correcciones.saturating_add(otra.correcciones);
    for (campo, mc2) in otra.por_campo {
        match destino.por_campo.get_mut(&campo) {
            Some(mc1) => {
                let eval_tot = mc1.evaluaciones.saturating_add(mc2.evaluaciones);
                if eval_tot > 0 {
                    mc1.precision_ewma = (mc1.precision_ewma * mc1.evaluaciones as f64
                        + mc2.precision_ewma * mc2.evaluaciones as f64)
                        / eval_tot as f64;
                }
                mc1.evaluaciones = eval_tot;
                mc1.aciertos = mc1.aciertos.saturating_add(mc2.aciertos);
                mc1.correcciones = mc1.correcciones.saturating_add(mc2.correcciones);
                mc1.alpha = (mc1.alpha + (mc2.alpha - PRIOR_BETA_ALPHA).max(0.0)).max(0.5);
                mc1.beta = (mc1.beta + (mc2.beta - PRIOR_BETA_BETA).max(0.0)).max(0.5);
                normalizar_ess(&mut mc1.alpha, &mut mc1.beta);
                if mc2.ultima_actualizacion > mc1.ultima_actualizacion {
                    mc1.ultima_actualizacion = mc2.ultima_actualizacion;
                }
            }
            None => {
                destino.por_campo.insert(campo, mc2);
            }
        }
    }
}

fn fusionar_estado_consolidacion(destino: &mut EstadoConsolidacion, otra: EstadoConsolidacion) {
    destino.presion += otra.presion;
    destino.correcciones_sin_consolidar = destino
        .correcciones_sin_consolidar
        .saturating_add(otra.correcciones_sin_consolidar);
    destino.notas_sin_consolidar = destino
        .notas_sin_consolidar
        .saturating_add(otra.notas_sin_consolidar);
    destino.extracciones_desde_ultima = destino
        .extracciones_desde_ultima
        .saturating_add(otra.extracciones_desde_ultima);
    if otra.ultima_consolidacion > destino.ultima_consolidacion {
        destino.ultima_consolidacion = otra.ultima_consolidacion;
    }
    for id in otra.ids_consolidadas_recientes {
        if !destino.ids_consolidadas_recientes.contains(&id) {
            destino.ids_consolidadas_recientes.push(id);
        }
    }
    if destino.ids_consolidadas_recientes.len() > 200 {
        let ex = destino.ids_consolidadas_recientes.len() - 200;
        destino.ids_consolidadas_recientes.drain(0..ex);
    }
}

fn normalizar_y_fusionar_defensores(memoria: &mut MemoriaAprendizaje, fusionar_colecciones: bool) {
    let nombres_prev = std::mem::take(&mut memoria.nombres_defensores);
    for (k, v) in nombres_prev {
        registrar_nombre_defensor(&mut memoria.nombres_defensores, &v);
        registrar_nombre_defensor(&mut memoria.nombres_defensores, &k);
    }

    for r in &mut memoria.reglas {
        if !es_alcance_general(&r.defensor_alcance) {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &r.defensor_alcance);
            r.defensor_alcance = alcance_canonico(&r.defensor_alcance);
        } else {
            r.defensor_alcance = "*".to_string();
        }
        let mut conf_norm = Vec::new();
        for d in std::mem::take(&mut r.defensores_confirmantes) {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &d);
            let dk = normalizar_clave_defensor(&d);
            if !dk.is_empty() && !conf_norm.contains(&dk) {
                conf_norm.push(dk);
            }
        }
        r.defensores_confirmantes = conf_norm;
    }

    for a in &mut memoria.tabla_alias {
        if !es_alcance_general(&a.defensor_alcance) {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &a.defensor_alcance);
            a.defensor_alcance = alcance_canonico(&a.defensor_alcance);
        } else {
            a.defensor_alcance = "*".to_string();
        }
        let mut conf_norm = Vec::new();
        for d in std::mem::take(&mut a.defensores_confirmantes) {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &d);
            let dk = normalizar_clave_defensor(&d);
            if !dk.is_empty() && !conf_norm.contains(&dk) {
                conf_norm.push(dk);
            }
        }
        a.defensores_confirmantes = conf_norm;
    }

    for c in &mut memoria.correcciones {
        if !c.defensor_clave.trim().is_empty() {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &c.defensor_clave);
            c.defensor_clave = normalizar_clave_defensor(&c.defensor_clave);
        } else {
            c.defensor_clave.clear();
        }
    }
    for e in &mut memoria.ejemplos_aciertos {
        if !e.defensor_clave.trim().is_empty() {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &e.defensor_clave);
            e.defensor_clave = normalizar_clave_defensor(&e.defensor_clave);
        } else {
            e.defensor_clave.clear();
        }
    }
    for clu in &mut memoria.cola_induccion {
        if !clu.defensor_clave.trim().is_empty() {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &clu.defensor_clave);
            clu.defensor_clave = normalizar_clave_defensor(&clu.defensor_clave);
        }
        let mut pres_norm = Vec::new();
        for d in std::mem::take(&mut clu.defensores_presentes) {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &d);
            let dk = normalizar_clave_defensor(&d);
            if !dk.is_empty() && !pres_norm.contains(&dk) {
                pres_norm.push(dk);
            }
        }
        clu.defensores_presentes = pres_norm;
    }

    let prefs_prev = std::mem::take(&mut memoria.preferencias_por_defensor);
    for (k, v) in prefs_prev {
        registrar_nombre_defensor(&mut memoria.nombres_defensores, &k);
        let dk = normalizar_clave_defensor(&k);
        if dk.is_empty() {
            continue;
        }
        match memoria.preferencias_por_defensor.get_mut(&dk) {
            Some(dest) => fusionar_preferencias_estilo(dest, &v),
            None => {
                memoria.preferencias_por_defensor.insert(dk, v);
            }
        }
    }

    let met_prev = std::mem::take(&mut memoria.metricas_por_defensor);
    for (k, mut v) in met_prev {
        registrar_nombre_defensor(&mut memoria.nombres_defensores, &v.nombre_mostrar);
        registrar_nombre_defensor(&mut memoria.nombres_defensores, &k);
        let dk = normalizar_clave_defensor(&k);
        if dk.is_empty() {
            continue;
        }
        if let Some(mejor_nom) = memoria.nombres_defensores.get(&dk) {
            if v.nombre_mostrar.trim().is_empty()
                || es_nombre_defensor_mas_informativo(mejor_nom, &v.nombre_mostrar)
            {
                v.nombre_mostrar = mejor_nom.clone();
            }
        }
        match memoria.metricas_por_defensor.get_mut(&dk) {
            Some(dest) => fusionar_metricas_defensor(dest, v),
            None => {
                memoria.metricas_por_defensor.insert(dk, v);
            }
        }
    }

    let perf_prev = std::mem::take(&mut memoria.perfiles_sinteticos);
    for (k, mut v) in perf_prev {
        let amb = alcance_canonico(&k);
        if amb != "*" {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &v.nombre_mostrar);
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &k);
            if let Some(nom) = memoria.nombres_defensores.get(&amb) {
                v.nombre_mostrar = nom.clone();
            }
        }
        v.ambito = amb.clone();
        match memoria.perfiles_sinteticos.get(&amb) {
            Some(actual)
                if actual.version > v.version
                    || (actual.version == v.version
                        && actual.ultima_actualizacion >= v.ultima_actualizacion) => {}
            _ => {
                memoria.perfiles_sinteticos.insert(amb, v);
            }
        }
    }

    let est_prev = std::mem::take(&mut memoria.estados_consolidacion);
    for (k, v) in est_prev {
        let amb = alcance_canonico(&k);
        if amb != "*" {
            registrar_nombre_defensor(&mut memoria.nombres_defensores, &k);
        }
        match memoria.estados_consolidacion.get_mut(&amb) {
            Some(dest) => fusionar_estado_consolidacion(dest, v),
            None => {
                memoria.estados_consolidacion.insert(amb, v);
            }
        }
    }

    if fusionar_colecciones {
        let mut reglas_fusionadas: Vec<ReglaAprendida> = Vec::with_capacity(memoria.reglas.len());
        let mut mapeo_ids_reglas: BTreeMap<String, String> = BTreeMap::new();
        for r in std::mem::take(&mut memoria.reglas) {
            if !r.id.starts_with("base-") {
                if let Some(dest) = reglas_fusionadas.iter_mut().find(|x| {
                    !x.id.starts_with("base-")
                        && x.defensor_alcance == r.defensor_alcance
                        && x.campo_objetivo == r.campo_objetivo
                        && es_misma_regla(&x.regla, &r.regla)
                }) {
                    if !r.id.is_empty() && !dest.id.is_empty() && r.id != dest.id {
                        mapeo_ids_reglas.insert(r.id.clone(), dest.id.clone());
                    }
                    fusionar_regla_en(dest, r);
                    continue;
                }
            }
            reglas_fusionadas.push(r);
        }
        memoria.reglas = reglas_fusionadas;
        if !mapeo_ids_reglas.is_empty() {
            for c in &mut memoria.correcciones {
                if let Some(nuevo_id) = mapeo_ids_reglas.get(&c.regla_id_asociada) {
                    c.regla_id_asociada = nuevo_id.clone();
                }
            }
            for clu in &mut memoria.cola_induccion {
                if let Some(nuevo_id) = mapeo_ids_reglas.get(&clu.regla_inducida_id) {
                    clu.regla_inducida_id = nuevo_id.clone();
                }
            }
        }

        let mut alias_fusionados: Vec<AliasEstadistico> =
            Vec::with_capacity(memoria.tabla_alias.len());
        for a in std::mem::take(&mut memoria.tabla_alias) {
            let ent_norm = crate::estadistica_ia::canonizar_entrada_alias(&a.campo, &a.entrada_norm);
            let can_norm = normalizar_texto_sin_tildes(&a.canonico);
            if let Some(dest) = alias_fusionados.iter_mut().find(|x| {
                x.campo == a.campo
                    && crate::estadistica_ia::canonizar_entrada_alias(&x.campo, &x.entrada_norm)
                        == ent_norm
                    && normalizar_texto_sin_tildes(&x.canonico) == can_norm
                    && x.defensor_alcance == a.defensor_alcance
            }) {
                dest.alpha = (dest.alpha + (a.alpha - PRIOR_ALIAS_ALPHA).max(0.0)).max(0.5);
                dest.beta = (dest.beta + (a.beta - PRIOR_ALIAS_BETA).max(0.0)).max(0.5);
                normalizar_ess_par(&mut dest.alpha, &mut dest.beta, MAX_ESS_N);
                for d in a.defensores_confirmantes {
                    if !dest.defensores_confirmantes.contains(&d) {
                        dest.defensores_confirmantes.push(d);
                    }
                }
                if a.ultima_actualizacion > dest.ultima_actualizacion {
                    dest.ultima_actualizacion = a.ultima_actualizacion;
                }
                continue;
            }
            alias_fusionados.push(a);
        }
        memoria.tabla_alias = alias_fusionados;
    }
}

pub fn migrar_memoria_v1_a_v2(memoria: &mut MemoriaAprendizaje) {
    let era_v1 = memoria.version < 2;
    let era_anterior_v4 = memoria.version < 4;
    if memoria.version < VERSION_FORMATO {
        memoria.version = VERSION_FORMATO;
    }
    if memoria.semilla_rng == 0 {
        memoria.semilla_rng = DEFAULT_SEMILLA_RNG;
    }

    for r in &mut memoria.reglas {
        if r.campo_objetivo.trim().is_empty() || r.campo_objetivo == "general" {
            let inf = inferir_campo_objetivo_desde_tipo(&r.tipo_error);
            if inf != "general" {
                r.campo_objetivo = inf;
            }
        }
        if r.triggers.is_empty() {
            r.triggers = extraer_triggers_texto(&r.regla, 8);
        }
        if r.ultima_actualizacion.is_empty() {
            r.ultima_actualizacion = r.actualizada.clone();
        }
        if r.id.starts_with("base-") {
            // Brecha A: no sobreescribir defensor_alcance ni origen si ya vienen definidos;
            // normalizar defensor_alcance con normalizar_clave_defensor salvo "*".
            if es_alcance_general(&r.defensor_alcance) {
                r.defensor_alcance = "*".to_string();
            } else {
                registrar_nombre_defensor(&mut memoria.nombres_defensores, &r.defensor_alcance);
                r.defensor_alcance = alcance_canonico(&r.defensor_alcance);
            }
            if r.origen.trim().is_empty() || r.origen == "mitigacion_modelo" {
                r.origen = if r.defensor_alcance == "*" {
                    "base_invariante".to_string()
                } else {
                    "base_perfil".to_string()
                };
            }
            if r.estado.trim().is_empty() {
                r.estado = "activa".to_string();
            }
            if r.alpha <= PRIOR_BETA_ALPHA && r.beta >= PRIOR_BETA_BETA {
                r.alpha = 6.0;
                r.beta = 1.0;
            }
        } else {
            if r.defensor_alcance.trim().is_empty() {
                r.defensor_alcance = "*".to_string();
            }
            if era_v1 || r.estado.trim().is_empty() {
                if r.alpha <= PRIOR_BETA_ALPHA && r.refuerzos > 0 {
                    r.alpha = PRIOR_BETA_ALPHA + (r.refuerzos as f64).min(20.0);
                    r.beta = PRIOR_BETA_BETA;
                }
                if r.estado.trim().is_empty() {
                    r.estado = if r.refuerzos >= 2 {
                        "activa".to_string()
                    } else {
                        "candidata".to_string()
                    };
                }
            }
        }
    }

    for a in &mut memoria.tabla_alias {
        if a.defensor_alcance.trim().is_empty() {
            a.defensor_alcance = "*".to_string();
        }
    }

    if era_v1 && memoria.metricas_campos.is_empty() && !memoria.correcciones.is_empty() {
        for campo in CAMPOS_AUDIENCIA {
            let corr_campo = memoria
                .correcciones
                .iter()
                .filter(|c| c.campos.iter().any(|fc| fc.campo == campo))
                .count() as u32;
            let total_eval = (memoria.aciertos + memoria.correcciones.len() as u32).max(corr_campo);
            let aciertos_campo = total_eval.saturating_sub(corr_campo);
            if total_eval > 0 {
                let mut mc = MetricasCampo {
                    campo: campo.to_string(),
                    evaluaciones: total_eval,
                    aciertos: aciertos_campo,
                    correcciones: corr_campo,
                    alpha: PRIOR_BETA_ALPHA + (aciertos_campo as f64).min(30.0),
                    beta: PRIOR_BETA_BETA + (corr_campo as f64).min(30.0),
                    precision_ewma: aciertos_campo as f64 / total_eval as f64,
                    ultima_actualizacion: Local::now().to_rfc3339(),
                };
                normalizar_ess(&mut mc.alpha, &mut mc.beta);
                memoria.metricas_campos.insert(campo.to_string(), mc);
            }
        }
    }

    normalizar_y_fusionar_defensores(memoria, era_anterior_v4);
    asegurar_instancia_y_contadores(memoria);
}

pub(crate) fn renombrar_defensor_en_memoria_struct(
    memoria: &mut MemoriaAprendizaje,
    actual: &str,
    nuevo: &str,
) -> bool {
    let clave_vieja = normalizar_clave_defensor(actual);
    let clave_nueva = normalizar_clave_defensor(nuevo);
    let nuevo_display = colapsar_espacios(nuevo);
    if clave_vieja.is_empty() || clave_nueva.is_empty() || nuevo_display.is_empty() {
        return false;
    }

    let mut tocado = false;
    if clave_vieja == clave_nueva {
        let existe_en_memoria = memoria.nombres_defensores.contains_key(&clave_nueva)
            || memoria.metricas_por_defensor.contains_key(&clave_nueva)
            || memoria.perfiles_sinteticos.contains_key(&clave_nueva)
            || memoria.preferencias_por_defensor.contains_key(&clave_nueva)
            || memoria.estados_consolidacion.contains_key(&clave_nueva)
            || memoria
                .reglas
                .iter()
                .any(|r| coincide_alcance_defensor(&r.defensor_alcance, &clave_nueva))
            || memoria
                .tabla_alias
                .iter()
                .any(|a| coincide_alcance_defensor(&a.defensor_alcance, &clave_nueva))
            || memoria
                .correcciones
                .iter()
                .any(|c| coincide_alcance_defensor(&c.defensor_clave, &clave_nueva))
            || memoria
                .ejemplos_aciertos
                .iter()
                .any(|e| coincide_alcance_defensor(&e.defensor_clave, &clave_nueva))
            || memoria
                .consolidaciones_ia
                .iter()
                .any(|co| coincide_alcance_defensor(&co.ambito, &clave_nueva));
        if existe_en_memoria {
            memoria
                .nombres_defensores
                .insert(clave_nueva.clone(), nuevo_display.clone());
            if let Some(m) = memoria.metricas_por_defensor.get_mut(&clave_nueva) {
                m.nombre_mostrar = nuevo_display.clone();
            }
            if let Some(p) = memoria.perfiles_sinteticos.get_mut(&clave_nueva) {
                p.nombre_mostrar = nuevo_display.clone();
            }
            for co in &mut memoria.consolidaciones_ia {
                if coincide_alcance_defensor(&co.ambito, &clave_nueva) {
                    co.nombre_ambito = nuevo_display.clone();
                }
            }
            tocado = true;
        }
        return tocado;
    }

    if memoria.nombres_defensores.remove(&clave_vieja).is_some() {
        tocado = true;
    }
    for r in &mut memoria.reglas {
        if r.defensor_alcance == clave_vieja {
            r.defensor_alcance = clave_nueva.clone();
            tocado = true;
        }
        for d in &mut r.defensores_confirmantes {
            if *d == clave_vieja {
                *d = clave_nueva.clone();
                tocado = true;
            }
        }
    }
    for a in &mut memoria.tabla_alias {
        if a.defensor_alcance == clave_vieja {
            a.defensor_alcance = clave_nueva.clone();
            tocado = true;
        }
        for d in &mut a.defensores_confirmantes {
            if *d == clave_vieja {
                *d = clave_nueva.clone();
                tocado = true;
            }
        }
    }
    for c in &mut memoria.correcciones {
        if c.defensor_clave == clave_vieja {
            c.defensor_clave = clave_nueva.clone();
            tocado = true;
        }
    }
    for e in &mut memoria.ejemplos_aciertos {
        if e.defensor_clave == clave_vieja {
            e.defensor_clave = clave_nueva.clone();
            tocado = true;
        }
    }
    for clu in &mut memoria.cola_induccion {
        if clu.defensor_clave == clave_vieja {
            clu.defensor_clave = clave_nueva.clone();
            tocado = true;
        }
        for d in &mut clu.defensores_presentes {
            if *d == clave_vieja {
                *d = clave_nueva.clone();
                tocado = true;
            }
        }
    }
    if let Some(pref_v) = memoria.preferencias_por_defensor.remove(&clave_vieja) {
        match memoria.preferencias_por_defensor.get_mut(&clave_nueva) {
            Some(dest) => fusionar_preferencias_estilo(dest, &pref_v),
            None => {
                memoria
                    .preferencias_por_defensor
                    .insert(clave_nueva.clone(), pref_v);
            }
        }
        tocado = true;
    }
    if let Some(mut met_v) = memoria.metricas_por_defensor.remove(&clave_vieja) {
        met_v.nombre_mostrar = nuevo_display.clone();
        match memoria.metricas_por_defensor.get_mut(&clave_nueva) {
            Some(dest) => fusionar_metricas_defensor(dest, met_v),
            None => {
                memoria
                    .metricas_por_defensor
                    .insert(clave_nueva.clone(), met_v);
            }
        }
        tocado = true;
    }
    if let Some(mut perf_v) = memoria.perfiles_sinteticos.remove(&clave_vieja) {
        perf_v.ambito = clave_nueva.clone();
        perf_v.nombre_mostrar = nuevo_display.clone();
        match memoria.perfiles_sinteticos.get(&clave_nueva) {
            Some(actual_p) if actual_p.version > perf_v.version => {}
            _ => {
                memoria
                    .perfiles_sinteticos
                    .insert(clave_nueva.clone(), perf_v);
            }
        }
        tocado = true;
    }
    if let Some(est_v) = memoria.estados_consolidacion.remove(&clave_vieja) {
        match memoria.estados_consolidacion.get_mut(&clave_nueva) {
            Some(dest) => fusionar_estado_consolidacion(dest, est_v),
            None => {
                memoria
                    .estados_consolidacion
                    .insert(clave_nueva.clone(), est_v);
            }
        }
        tocado = true;
    }
    for co in &mut memoria.consolidaciones_ia {
        if coincide_alcance_defensor(&co.ambito, &clave_vieja) {
            co.ambito = clave_nueva.clone();
            co.nombre_ambito = nuevo_display.clone();
            co.telemetria_disparo.ambito = clave_nueva.clone();
            for s in &mut co.sugerencias_build {
                if coincide_alcance_defensor(&s.ambito, &clave_vieja) {
                    s.ambito = clave_nueva.clone();
                }
            }
            tocado = true;
        }
    }

    if tocado {
        memoria
            .nombres_defensores
            .insert(clave_nueva, nuevo_display);
        normalizar_y_fusionar_defensores(memoria, true);
    }
    tocado
}

pub fn migrar_defensor_en_memoria_en(ruta: &Path, actual: &str, nuevo: &str) -> Result<(), String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let existe = ruta.exists();
    let mut memoria = cargar_memoria_en(ruta);
    let tocado = renombrar_defensor_en_memoria_struct(&mut memoria, actual, nuevo);
    if tocado && (existe || !memoria.correcciones.is_empty() || !memoria.reglas.is_empty()) {
        guardar_memoria_en(ruta, &memoria)?;
    }
    Ok(())
}

pub fn migrar_defensor_en_memoria(actual: &str, nuevo: &str) -> Result<(), String> {
    migrar_defensor_en_memoria_en(&ruta_memoria(), actual, nuevo)
}

pub(crate) fn normalizar_ess_par(alpha: &mut f64, beta: &mut f64, max_n: f64) {
    let total = *alpha + *beta;
    if total > max_n && total > 0.0 {
        let escala = max_n / total;
        *alpha = (*alpha * escala).max(0.5);
        *beta = (*beta * escala).max(0.5);
    }
}

fn normalizar_ess(alpha: &mut f64, beta: &mut f64) {
    normalizar_ess_par(alpha, beta, MAX_ESS_N);
}

pub fn ln_gamma(z: f64) -> f64 {
    const COEFFS: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    if z < 0.5 {
        let pi = std::f64::consts::PI;
        return pi.ln() - (pi * z).sin().abs().ln() - ln_gamma(1.0 - z);
    }
    let z = z - 1.0;
    let mut x = COEFFS[0];
    for (i, &c) in COEFFS.iter().enumerate().skip(1) {
        x += c / (z + i as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
}

fn betacf(x: f64, a: f64, b: f64) -> f64 {
    const MAX_IT: usize = 200;
    const EPS: f64 = 3.0e-12;
    const FPMIN: f64 = 1.0e-30;

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_IT {
        let m_f = m as f64;
        let m2 = 2.0 * m_f;
        let aa = m_f * (b - m_f) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m_f) * (qab + m_f) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// Función de distribución acumulada de Beta(a, b): I_x(a, b).
pub fn beta_inc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let a = a.max(1e-6);
    let b = b.max(1e-6);
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        (bt * betacf(x, a, b) / a).clamp(0.0, 1.0)
    } else {
        (1.0 - bt * betacf(1.0 - x, b, a) / b).clamp(0.0, 1.0)
    }
}

/// Cuantil exacto de la distribución Beta(alpha, beta) por bisección sobre I_x(alpha, beta).
pub fn cuantil_beta(alpha: f64, beta: f64, q: f64) -> f64 {
    let target = q.clamp(0.0001, 0.9999);
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if beta_inc(mid, alpha, beta) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Límite inferior creíble bayesiano (cuantil q_0.10 de Beta(alpha, beta)).
pub fn limite_inferior_bayesiano(alpha: f64, beta: f64) -> f64 {
    cuantil_beta(alpha.max(0.1), beta.max(0.1), 0.10)
}

/// Cota inferior exacta del intervalo de confianza de Wilson.
pub fn limite_inferior_wilson(aciertos: f64, total: f64, z: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    let p = (aciertos / total).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / total;
    let centro = p + z2 / (2.0 * total);
    let margen = z * ((p * (1.0 - p) + z2 / (4.0 * total)) / total).sqrt();
    ((centro - margen) / denom).clamp(0.0, 1.0)
}

/// Generador pseudoaleatorio determinístico ligero (SplitMix64) para muestreo Thompson.
fn next_u01(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    let val = (z >> 11) as f64 / ((1u64 << 53) as f64);
    val.clamp(1e-12, 1.0 - 1e-12)
}

fn next_normal(state: &mut u64) -> f64 {
    let u1 = next_u01(state);
    let u2 = next_u01(state);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn muestrear_gamma(shape: f64, state: &mut u64) -> f64 {
    let a = shape.max(1e-3);
    if a < 1.0 {
        let u = next_u01(state);
        return muestrear_gamma(a + 1.0, state) * u.powf(1.0 / a);
    }
    let d = a - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    for _ in 0..64 {
        let x = next_normal(state);
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = next_u01(state);
        if u < 1.0 - 0.0331 * x * x * x * x {
            return d * v3;
        }
        if u.ln() < 0.5 * x * x + d * (1.0 - v3 + v3.ln()) {
            return d * v3;
        }
    }
    d.max(1e-6)
}

pub fn muestrear_beta(alpha: f64, beta: f64, seed: u64) -> f64 {
    let mut state = seed ^ 0x517cc1b727220a95;
    let g1 = muestrear_gamma(alpha.max(0.1), &mut state);
    let g2 = muestrear_gamma(beta.max(0.1), &mut state);
    let denom = g1 + g2;
    if denom <= 0.0 {
        0.5
    } else {
        (g1 / denom).clamp(0.0, 1.0)
    }
}

/// Decaimiento híbrido por evento aplicable (lambda = 0.96, ESS <= 40) y decaimiento
/// calendario muy lento solo más allá de 180 días (protege contra la vacancia judicial).
pub fn aplicar_decaimiento_beta(alpha: &mut f64, beta: &mut f64, ultima_actualizacion: &str) {
    aplicar_decaimiento_beta_en(alpha, beta, ultima_actualizacion, None);
}

pub fn aplicar_decaimiento_beta_con_prior_en(
    alpha: &mut f64,
    beta: &mut f64,
    prior_alpha: f64,
    prior_beta: f64,
    ultima_actualizacion: &str,
    referencia_ahora: Option<&str>,
) {
    let ref_dt = referencia_ahora
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_else(Local::now);

    if !ultima_actualizacion.trim().is_empty() {
        if let Ok(prev) = chrono::DateTime::parse_from_rfc3339(ultima_actualizacion.trim()) {
            let dias = (ref_dt.signed_duration_since(prev.with_timezone(&Local))).num_days() as f64;
            if dias > DIAS_GRACIA_VACANCIA {
                let factor_cal = (-((dias - DIAS_GRACIA_VACANCIA) / 365.0)).exp().clamp(0.5, 1.0);
                *alpha = prior_alpha + (*alpha - prior_alpha).max(0.0) * factor_cal;
                *beta = prior_beta + (*beta - prior_beta).max(0.0) * factor_cal;
            }
        }
    }

    *alpha = prior_alpha + DECAY_EVENTO_LAMBDA * (*alpha - prior_alpha).max(0.0);
    *beta = prior_beta + DECAY_EVENTO_LAMBDA * (*beta - prior_beta).max(0.0);
    normalizar_ess(alpha, beta);
}

pub fn aplicar_decaimiento_beta_en(
    alpha: &mut f64,
    beta: &mut f64,
    ultima_actualizacion: &str,
    referencia_ahora: Option<&str>,
) {
    aplicar_decaimiento_beta_con_prior_en(
        alpha,
        beta,
        PRIOR_BETA_ALPHA,
        PRIOR_BETA_BETA,
        ultima_actualizacion,
        referencia_ahora,
    );
}

// ---------------------------------------------------------------------------
// Comparación de campos, tokenizador judicial, BM25 y MMR
// ---------------------------------------------------------------------------

pub(crate) fn colapsar_espacios(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn quitar_tildes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' | 'ü' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            otro => otro,
        })
        .collect()
}

fn equivalentes_texto(a: &str, b: &str) -> bool {
    normalizar_texto_sin_tildes(a) == normalizar_texto_sin_tildes(b)
}

/// Coeficiente de Dice sobre bigramas de caracteres (0.0 = distintos, 1.0 = iguales).
/// Ignora mayúsculas, tildes y espacios repetidos.
pub fn similitud_textos(a: &str, b: &str) -> f64 {
    let na: Vec<char> = quitar_tildes(&colapsar_espacios(a).to_lowercase())
        .chars()
        .collect();
    let nb: Vec<char> = quitar_tildes(&colapsar_espacios(b).to_lowercase())
        .chars()
        .collect();
    if na == nb {
        return 1.0;
    }
    if na.len() < 2 || nb.len() < 2 {
        return 0.0;
    }

    let bigramas = |v: &[char]| -> BTreeMap<(char, char), usize> {
        let mut mapa = BTreeMap::new();
        for par in v.windows(2) {
            *mapa.entry((par[0], par[1])).or_insert(0) += 1;
        }
        mapa
    };
    let ba = bigramas(&na);
    let bb = bigramas(&nb);

    let interseccion: usize = ba
        .iter()
        .filter_map(|(k, ca)| bb.get(k).map(|cb| (*ca).min(*cb)))
        .sum();
    let total = (na.len() - 1) + (nb.len() - 1);
    2.0 * interseccion as f64 / total as f64
}

fn normalizar_ordinal_token(tok: &str) -> &str {
    match tok {
        "primero" | "primera" | "1ro" | "1ra" | "1o" | "1a" | "01" => "1",
        "segundo" | "segunda" | "2do" | "2da" | "2o" | "2a" | "02" => "2",
        "tercero" | "tercera" | "3ro" | "3ra" | "3o" | "3a" | "03" => "3",
        "cuarto" | "cuarta" | "4to" | "4ta" | "4o" | "4a" | "04" => "4",
        "quinto" | "quinta" | "5to" | "5ta" | "5o" | "5a" | "05" => "5",
        "sexto" | "sexta" | "6to" | "6ta" | "6o" | "6a" | "06" => "6",
        "septimo" | "septima" | "7mo" | "7ma" | "7o" | "7a" | "07" => "7",
        "octavo" | "octava" | "8vo" | "8va" | "8o" | "8a" | "08" => "8",
        "noveno" | "novena" | "9no" | "9na" | "9o" | "9a" | "09" => "9",
        "decimo" | "decima" | "10mo" | "10ma" | "10o" | "10a" => "10",
        "undecimo" | "undecima" | "11mo" | "11o" | "11a" => "11",
        "duodecimo" | "duodecima" | "12mo" | "12o" | "12a" => "12",
        "decimotercero" | "13o" => "13",
        "decimocuarto" | "14o" => "14",
        "decimoquinto" | "15o" => "15",
        _ => tok,
    }
}

/// Tokenizador judicial especializado: preserva tokens cortos críticos (`no`, `si`, `am`, `pm`,
/// `me`, `su`), siglas (`cto`, `mpal`, `pcto`, `uri`, `jdo`), números pequeños (p. ej. juzgado `3`)
/// y unifica ordinales (`tercero` -> `3`).
pub(crate) fn tokenizar_judicial(texto: &str) -> Vec<String> {
    const STOPWORDS_NEUTRAS: &[&str] = &[
        "de", "del", "la", "las", "los", "el", "en", "y", "a", "o", "u", "un", "una", "unos",
        "unas", "por", "para", "con", "que", "es", "al", "lo", "le", "les",
    ];
    let mut tokens = Vec::new();
    for raw in quitar_tildes(&texto.to_lowercase()).split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let norm = normalizar_ordinal_token(raw);
        if STOPWORDS_NEUTRAS.contains(&norm) {
            continue;
        }
        let solo_digitos = norm.chars().all(|c| c.is_ascii_digit());
        if solo_digitos {
            if norm.len() >= 6 {
                tokens.push("_rad_".to_string());
            } else {
                tokens.push(norm.to_string());
            }
            continue;
        }
        if norm.len() >= 2 {
            tokens.push(norm.to_string());
        }
    }
    tokens
}

pub(crate) fn ngramas_caracteres(texto: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for tok in tokenizar_judicial(texto) {
        if tok == "_rad_" || tok.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let chars: Vec<char> = tok.chars().collect();
        if chars.len() >= n {
            for win in chars.windows(n) {
                out.push(win.iter().collect());
            }
        }
    }
    out
}

fn tokens_significativos(texto: &str) -> BTreeSet<String> {
    tokenizar_judicial(texto).into_iter().collect()
}

/// Puntajes Okapi BM25 sobre una colección de documentos tokenizados.
pub(crate) fn calcular_bm25(query_tokens: &[String], docs: &[Vec<String>]) -> Vec<f64> {
    if docs.is_empty() || query_tokens.is_empty() {
        return vec![0.0; docs.len()];
    }
    let k1 = 1.5;
    let b = 0.75;
    let n_docs = docs.len() as f64;
    let avg_dl = (docs.iter().map(|d| d.len()).sum::<usize>() as f64 / n_docs).max(1.0);

    let query_uniq: BTreeSet<&str> = query_tokens.iter().map(String::as_str).collect();
    let mut df_map: BTreeMap<&str, usize> = BTreeMap::new();
    for &qt in &query_uniq {
        let df = docs.iter().filter(|d| d.iter().any(|t| t == qt)).count();
        df_map.insert(qt, df);
    }

    docs.iter()
        .map(|doc| {
            if doc.is_empty() {
                return 0.0;
            }
            let dl = doc.len() as f64;
            let mut score = 0.0;
            for &qt in &query_uniq {
                let tf = doc.iter().filter(|t| t.as_str() == qt).count() as f64;
                if tf <= 0.0 {
                    continue;
                }
                let df = *df_map.get(qt).unwrap_or(&0) as f64;
                let idf = (((n_docs - df + 0.5) / (df + 0.5)) + 1.0).ln().max(0.05);
                let num = tf * (k1 + 1.0);
                let den = tf + k1 * (1.0 - b + b * (dl / avg_dl));
                score += idf * (num / den);
            }
            score
        })
        .collect()
}

fn jaccard_tokens(a: &[String], b: &[String]) -> f64 {
    let sa: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let sb: BTreeSet<&str> = b.iter().map(String::as_str).collect();
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Maximal Marginal Relevance (MMR, lambda = 0.7) para equilibrar relevancia y diversidad.
pub(crate) fn seleccionar_mmr(
    candidatos: &[(usize, f64)],
    doc_tokens: &[Vec<String>],
    k: usize,
    lambda: f64,
) -> Vec<usize> {
    if candidatos.is_empty() || k == 0 {
        return Vec::new();
    }
    let max_rel = candidatos
        .iter()
        .map(|(_, s)| *s)
        .fold(0.0_f64, f64::max)
        .max(1e-9);

    let mut restantes: Vec<(usize, f64)> = candidatos
        .iter()
        .map(|(idx, rel)| (*idx, (*rel / max_rel).clamp(0.0, 1.0)))
        .collect();
    let mut elegidos: Vec<usize> = Vec::with_capacity(k.min(restantes.len()));

    while elegidos.len() < k && !restantes.is_empty() {
        let mut mejor_pos = 0usize;
        let mut mejor_mmr = f64::NEG_INFINITY;

        for (pos, &(idx, rel_norm)) in restantes.iter().enumerate() {
            let max_sim = elegidos
                .iter()
                .map(|&sel_idx| jaccard_tokens(&doc_tokens[idx], &doc_tokens[sel_idx]))
                .fold(0.0_f64, f64::max);
            let mmr = lambda * rel_norm - (1.0 - lambda) * max_sim;
            if mmr > mejor_mmr + 1e-12 {
                mejor_mmr = mmr;
                mejor_pos = pos;
            }
        }

        let (idx_ganador, _) = restantes.remove(mejor_pos);
        elegidos.push(idx_ganador);
    }

    elegidos
}

/// Similitud combinada (vocabulario judicial compartido + bigramas) para encontrar los casos pasados
/// más parecidos a un texto nuevo sin llamadas a red.
pub fn similitud_consulta(a: &str, b: &str) -> f64 {
    let ta = tokens_significativos(a);
    let tb = tokens_significativos(b);
    let dice_palabras = if ta.is_empty() || tb.is_empty() {
        0.0
    } else {
        let comunes = ta.intersection(&tb).count();
        (2.0 * comunes as f64) / ((ta.len() + tb.len()) as f64)
    };
    let big = similitud_textos(a, b);
    0.7 * dice_palabras + 0.3 * big
}

/// Devuelve los campos en los que lo guardado por el usuario difiere de lo que propuso la IA.
/// - Radicado, tipo y juzgado: no cuentan cambios de mayúsculas ni de espacios.
/// - Observaciones: solo cuentan reescrituras (similitud menor al umbral), no retoques.
pub fn detectar_correcciones(
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
) -> Vec<CampoCorregido> {
    let comparaciones: [(&str, &str, &str, bool); 8] = [
        (
            "radicado",
            &prediccion.radicado,
            &final_usuario.radicado,
            equivalentes_texto(&prediccion.radicado, &final_usuario.radicado),
        ),
        (
            "tipo",
            &prediccion.tipo,
            &final_usuario.tipo,
            equivalentes_texto(&prediccion.tipo, &final_usuario.tipo),
        ),
        (
            "fecha",
            &prediccion.fecha,
            &final_usuario.fecha,
            prediccion.fecha.trim() == final_usuario.fecha.trim(),
        ),
        (
            "hora",
            &prediccion.hora,
            &final_usuario.hora,
            prediccion.hora.trim() == final_usuario.hora.trim(),
        ),
        (
            "juzgado",
            &prediccion.juzgado,
            &final_usuario.juzgado,
            equivalentes_texto(&prediccion.juzgado, &final_usuario.juzgado),
        ),
        (
            "realizada",
            &prediccion.realizada,
            &final_usuario.realizada,
            equivalentes_texto(&prediccion.realizada, &final_usuario.realizada),
        ),
        (
            "motivo",
            &prediccion.motivo,
            &final_usuario.motivo,
            equivalentes_texto(&prediccion.motivo, &final_usuario.motivo),
        ),
        (
            "observaciones",
            &prediccion.observaciones,
            &final_usuario.observaciones,
            similitud_textos(&prediccion.observaciones, &final_usuario.observaciones)
                >= UMBRAL_SIMILITUD_OBSERVACIONES,
        ),
    ];

    comparaciones
        .iter()
        .filter(|(_, _, _, iguales)| !iguales)
        .map(|(campo, ia, usuario, _)| CampoCorregido {
            campo: (*campo).to_string(),
            valor_ia: ia.trim().to_string(),
            valor_usuario: usuario.trim().to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reglas aprendidas y selección de contexto para el prompt (Fase 2 y 3)
// ---------------------------------------------------------------------------

pub(crate) fn es_misma_regla(a: &str, b: &str) -> bool {
    if equivalentes_texto(a, b) {
        return true;
    }
    let sim = similitud_textos(a, b);
    if sim < UMBRAL_REGLA_SIMILAR {
        return false;
    }
    let tok = |s: &str| -> BTreeSet<String> {
        quitar_tildes(&s.to_lowercase())
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect()
    };
    let ta = tok(a);
    let tb = tok(b);
    ta == tb || (sim >= 0.85 && (ta.is_subset(&tb) || tb.is_subset(&ta)))
}

pub(crate) fn es_trigger_valido(tok_norm: &str) -> bool {
    if tok_norm.is_empty()
        || tok_norm == "_rad_"
        || tok_norm.contains('[')
        || tok_norm.contains(']')
        || tok_norm.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    !matches!(
        tok_norm,
        "enero"
            | "febrero"
            | "marzo"
            | "abril"
            | "mayo"
            | "junio"
            | "julio"
            | "agosto"
            | "septiembre"
            | "setiembre"
            | "octubre"
            | "noviembre"
            | "diciembre"
            | "lunes"
            | "martes"
            | "miercoles"
            | "jueves"
            | "viernes"
            | "sabado"
            | "domingo"
            | "dia"
            | "dias"
            | "numero"
            | "telefono"
            | "proceso"
            | "bajo"
            | "rad"
            | "radicado"
            | "nro"
            | "seguido"
            | "seguida"
            | "adelantado"
            | "adelantada"
            | "contra"
            | "dentro"
            | "cabo"
            | "llevar"
            | "fin"
            | "aras"
            | "vez"
            | "como"
            | "cuando"
            | "donde"
            | "porque"
            | "sin"
            | "entre"
            | "sobre"
            | "ante"
            | "desde"
            | "hasta"
            | "hacia"
            | "se"
            | "su"
            | "sus"
            | "ha"
            | "han"
            | "hay"
            | "ser"
            | "estar"
            | "esta"
            | "este"
            | "eso"
            | "esa"
            | "ese"
            | "parte"
            | "partes"
            | "una"
            | "uno"
            | "dos"
            | "tres"
            | "mil"
            | "ano"
            | "anos"
            | "presente"
            | "dicho"
            | "dicha"
            | "mismo"
            | "misma"
            | "cual"
            | "quien"
            | "solo"
            | "solamente"
            | "unicamente"
    )
}

pub(crate) fn extraer_triggers_texto(texto: &str, max_triggers: usize) -> Vec<String> {
    let mut texto_limpio = texto.to_lowercase();
    for marcador in ["[procesado]", "[radicado]", "[numero]", "[telefono]"] {
        if texto_limpio.contains(marcador) {
            texto_limpio = texto_limpio.replace(marcador, " ");
        }
    }
    let mut vistos = BTreeSet::new();
    let mut out = Vec::new();
    for tok in tokenizar_judicial(&texto_limpio) {
        if !es_trigger_valido(&tok) {
            continue;
        }
        if vistos.insert(tok.clone()) {
            out.push(tok);
            if out.len() >= max_triggers {
                break;
            }
        }
    }
    out
}

fn prioridad_desalojo_regla(r: &ReglaAprendida, idx: usize) -> (u8, u32, usize) {
    if r.origen == "base_invariante" || r.id.starts_with("base-") {
        return (255, u32::MAX, idx);
    }
    let tier = match r.estado.as_str() {
        "retirada" => 0,
        "cuarentena" => 1,
        "candidata" => 2,
        "prueba" => 3,
        _ => 4, // activa
    };
    let lb = (limite_inferior_bayesiano(r.alpha, r.beta) * 10_000.0).round() as u32;
    (tier, lb, idx)
}

pub(crate) fn agregar_regla_interna(
    memoria: &mut MemoriaAprendizaje,
    tipo_error: &str,
    regla_raw: &str,
    estado_inicial: &str,
    campo_objetivo_opt: Option<&str>,
    triggers_extra: &[String],
    origen: &str,
    modelo_alcance: &str,
    confianza_llm: f64,
    origen_correccion_id: &str,
) -> Option<String> {
    agregar_regla_interna_con_defensor(
        memoria,
        tipo_error,
        regla_raw,
        estado_inicial,
        campo_objetivo_opt,
        triggers_extra,
        origen,
        modelo_alcance,
        "*",
        &[],
        confianza_llm,
        origen_correccion_id,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn agregar_regla_interna_con_defensor(
    memoria: &mut MemoriaAprendizaje,
    tipo_error: &str,
    regla_raw: &str,
    estado_inicial: &str,
    campo_objetivo_opt: Option<&str>,
    triggers_extra: &[String],
    origen: &str,
    modelo_alcance: &str,
    defensor_alcance: &str,
    defensores_extra: &[String],
    confianza_llm: f64,
    origen_correccion_id: &str,
) -> Option<String> {
    let regla = colapsar_espacios(regla_raw);
    if regla.len() < 6 {
        return None;
    }
    let tipo = if tipo_error.trim().is_empty() {
        "general".to_string()
    } else {
        tipo_error.trim().to_lowercase()
    };
    let campo_objetivo = campo_objetivo_opt
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| inferir_campo_objetivo_desde_tipo(&tipo));

    let def_norm = if defensor_alcance.trim().is_empty() || defensor_alcance.trim() == "*" {
        "*".to_string()
    } else {
        normalizar_clave_defensor(defensor_alcance)
    };
    let def_alcance_efectivo = if def_norm.is_empty() {
        "*".to_string()
    } else {
        def_norm
    };

    let mut triggers = extraer_triggers_texto(&regla, 8);
    for t in triggers_extra {
        let limpio = quitar_tildes(&t.trim().to_lowercase());
        if es_trigger_valido(&limpio) && !triggers.contains(&limpio) && triggers.len() < 12 {
            triggers.push(limpio);
        }
    }

    let ahora = Local::now().to_rfc3339();

    if let Some(existente) = memoria
        .reglas
        .iter_mut()
        .find(|r| es_misma_regla(&r.regla, &regla))
    {
        existente.refuerzos = existente.refuerzos.saturating_add(1);
        existente.actualizada = ahora.clone();
        existente.ultima_actualizacion = ahora;
        existente.alpha = (existente.alpha + 1.0).min(MAX_ESS_N);
        normalizar_ess(&mut existente.alpha, &mut existente.beta);
        if tipo != "general" {
            existente.tipo_error = tipo;
        }
        if existente.campo_objetivo == "general" && campo_objetivo != "general" {
            existente.campo_objetivo = campo_objetivo;
        }
        for t in triggers {
            if !existente.triggers.contains(&t) && existente.triggers.len() < 12 {
                existente.triggers.push(t);
            }
        }
        if regla.len() > existente.regla.len() && regla.len() <= 240 {
            existente.regla = regla;
        }
        if confianza_llm > 0.0 {
            existente.confianza_llm = if existente.confianza_llm > 0.0 {
                0.5 * existente.confianza_llm + 0.5 * confianza_llm
            } else {
                confianza_llm
            };
        }

        // Promoción automática al Cuaderno General cuando 2+ defensores distintos confirman la misma regla.
        // Si la regla estaba retirada/en cuarentena para otro defensor, se reactiva solo para el defensor actual
        // sin contar al defensor previo como confirmante activo.
        let reactivada = existente.estado == "retirada"
            || (existente.estado == "cuarentena" && estado_inicial != "cuarentena");
        if reactivada {
            existente.estado = estado_inicial.to_string();
            existente.alpha = PRIOR_BETA_ALPHA + 1.0;
            existente.beta = PRIOR_BETA_BETA;
            existente.impresiones = 0;
            existente.aciertos_directos = 0;
            existente.fallos_directos = 0;
            existente.aciertos_sombra = 0;
            existente.confirmaciones_sombra = 0;
            if existente.defensor_alcance != "*" && existente.defensor_alcance != def_alcance_efectivo
            {
                existente.defensores_confirmantes.clear();
                existente.defensor_alcance = def_alcance_efectivo.clone();
            }
        }

        if def_alcance_efectivo == "*" {
            existente.defensor_alcance = "*".to_string();
        } else {
            if !existente.defensor_alcance.is_empty()
                && existente.defensor_alcance != "*"
                && !existente.defensores_confirmantes.contains(&existente.defensor_alcance)
            {
                existente
                    .defensores_confirmantes
                    .push(existente.defensor_alcance.clone());
            }
            if !existente.defensores_confirmantes.contains(&def_alcance_efectivo) {
                existente
                    .defensores_confirmantes
                    .push(def_alcance_efectivo.clone());
            }
            for d in defensores_extra {
                let dk = normalizar_clave_defensor(d);
                if !dk.is_empty() && !existente.defensores_confirmantes.contains(&dk) {
                    existente.defensores_confirmantes.push(dk);
                }
            }
            if existente.defensores_confirmantes.len() >= 2 {
                existente.defensor_alcance = "*".to_string();
            }
        }

        if !reactivada && existente.estado == "candidata" && existente.refuerzos >= 2 {
            let lb = limite_inferior_bayesiano(existente.alpha, existente.beta);
            existente.estado = if lb >= 0.42 {
                "activa".to_string()
            } else {
                "prueba".to_string()
            };
        }
        return Some(existente.id.clone());
    }

    // Desalojar primero una regla existente antes de insertar (priorizando retirada -> cuarentena
    // -> candidata -> prueba -> activa con menor límite inferior creíble).
    while memoria.reglas.len() >= MAX_REGLAS {
        if let Some((idx_min, _)) = memoria
            .reglas
            .iter()
            .enumerate()
            .min_by_key(|(idx, r)| prioridad_desalojo_regla(r, *idx))
        {
            memoria.reglas.remove(idx_min);
        } else {
            break;
        }
    }

    let id = generar_id_unico("regla");
    let alcance = if modelo_alcance.trim().is_empty() {
        "*".to_string()
    } else {
        modelo_alcance.trim().to_string()
    };
    let mut confirmantes: Vec<String> = Vec::new();
    if def_alcance_efectivo != "*" {
        confirmantes.push(def_alcance_efectivo.clone());
    }
    for d in defensores_extra {
        let dk = normalizar_clave_defensor(d);
        if !dk.is_empty() && !confirmantes.contains(&dk) {
            confirmantes.push(dk);
        }
    }
    let alcance_final_def = if confirmantes.len() >= 2 {
        "*".to_string()
    } else {
        def_alcance_efectivo
    };

    let mut nueva = construir_regla_v2(
        id.clone(),
        tipo,
        regla,
        1,
        ahora,
        estado_inicial,
        campo_objetivo,
        triggers,
        origen,
        alcance,
        confianza_llm,
        origen_correccion_id.to_string(),
    );
    nueva.defensor_alcance = alcance_final_def;
    nueva.defensores_confirmantes = confirmantes;
    memoria.reglas.push(nueva);
    Some(id)
}

pub fn agregar_o_reforzar_regla_con_id(
    memoria: &mut MemoriaAprendizaje,
    tipo_error: &str,
    regla_raw: &str,
) -> Option<String> {
    agregar_regla_interna(
        memoria,
        tipo_error,
        regla_raw,
        "activa",
        None,
        &[],
        "preferencia_usuario",
        "*",
        0.9,
        "",
    )
}

pub fn agregar_o_reforzar_regla(
    memoria: &mut MemoriaAprendizaje,
    tipo_error: &str,
    regla_raw: &str,
) {
    let _ = agregar_o_reforzar_regla_con_id(memoria, tipo_error, regla_raw);
}

#[derive(Debug, Clone)]
pub struct ContextoPromptDetalle<'a> {
    pub reglas: Vec<&'a ReglaAprendida>,
    pub ejemplos_correcciones: Vec<&'a CorreccionIa>,
    pub ejemplos_aciertos: Vec<&'a EjemploPositivo>,
    pub pista_pendiente: Option<&'a CorreccionIa>,
    pub reglas_inyectadas_ids: Vec<String>,
    pub ejemplos_inyectados_ids: Vec<String>,
}

fn coincidencia_triggers(query_tokens: &[String], query_ngrams: &BTreeSet<String>, regla: &ReglaAprendida) -> f64 {
    if regla.triggers.is_empty() {
        return 0.0;
    }
    let q_set: BTreeSet<&str> = query_tokens.iter().map(String::as_str).collect();
    let mut hits = 0.0;
    for tr in &regla.triggers {
        let tr_norm = quitar_tildes(&tr.to_lowercase());
        if q_set.contains(tr_norm.as_str())
            || query_tokens.iter().any(|qt| qt.contains(&tr_norm) || tr_norm.contains(qt.as_str()))
        {
            hits += 1.0;
        } else if tr_norm.len() >= 4 {
            let tri: BTreeSet<String> = ngramas_caracteres(&tr_norm, 3).into_iter().collect();
            if !tri.is_empty() && tri.intersection(query_ngrams).count() * 2 >= tri.len() {
                hits += 0.5;
            }
        }
    }
    (hits / (regla.triggers.len().min(4) as f64)).clamp(0.0, 1.0)
}

pub fn seleccionar_contexto_detallado_para_defensor<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
    defensor_actual: Option<&str>,
) -> ContextoPromptDetalle<'a> {
    let query_tokens = tokenizar_judicial(texto_nuevo);
    let query_ngrams: BTreeSet<String> = ngramas_caracteres(texto_nuevo, 3).into_iter().collect();
    let def_clave = defensor_actual
        .map(normalizar_clave_defensor)
        .unwrap_or_default();

    let regla_aplica_a_defensor = |r: &ReglaAprendida| -> bool {
        es_alcance_general(&r.defensor_alcance)
            || coincide_alcance_defensor(&r.defensor_alcance, &def_clave)
    };
    let factor_defensor_regla = |r: &ReglaAprendida| -> f64 {
        if coincide_alcance_defensor(&r.defensor_alcance, &def_clave) {
            1.35
        } else {
            1.0
        }
    };
    let factor_modelo = |r: &ReglaAprendida| -> f64 {
        let Some(actual) = modelo_actual else {
            return 1.0;
        };
        if r.modelo_alcance.trim().is_empty()
            || r.modelo_alcance == "*"
            || r.modelo_alcance == actual
        {
            1.0
        } else {
            0.85
        }
    };

    // Brecha D: deduplicar base.reglas + local.reglas por es_misma_regla y alcance, priorizando local.
    let reglas_local_ap: Vec<&'a ReglaAprendida> = local
        .reglas
        .iter()
        .filter(|r| regla_aplica_a_defensor(r))
        .collect();
    let mut todas_reglas: Vec<&'a ReglaAprendida> =
        Vec::with_capacity(base.reglas.len() + reglas_local_ap.len());
    todas_reglas.extend(reglas_local_ap.iter().copied());
    for r_base in base.reglas.iter().filter(|r| regla_aplica_a_defensor(r)) {
        let alc_base = alcance_canonico(&r_base.defensor_alcance);
        let es_invariante = r_base.origen == "base_invariante" || r_base.id.starts_with("base-");
        let duplicada_en_local = reglas_local_ap.iter().any(|r_loc| {
            if es_invariante && !matches!(r_loc.estado.as_str(), "activa" | "") {
                return false;
            }
            r_loc.id == r_base.id
                || (es_misma_regla(&r_loc.regla, &r_base.regla)
                    && alcance_canonico(&r_loc.defensor_alcance) == alc_base)
        });
        if !duplicada_en_local {
            todas_reglas.push(r_base);
        }
    }

    // 1) Exploración controlada (máx. 1 ranura) entre reglas en estado "prueba" aplicables vía Thompson Sampling
    let mut regla_exploracion: Option<(usize, &'a ReglaAprendida)> = None;
    let seed_base = CONTADOR_IDS.fetch_add(1, Ordering::Relaxed)
        ^ (Local::now().timestamp_nanos_opt().unwrap_or(1) as u64);
    let prueba_indices: Vec<usize> = todas_reglas
        .iter()
        .enumerate()
        .filter(|(_, r)| r.estado == "prueba")
        .map(|(i, _)| i)
        .collect();

    if !prueba_indices.is_empty() {
        let prueba_docs: Vec<Vec<String>> = prueba_indices
            .iter()
            .map(|&i| {
                let mut d = tokenizar_judicial(&todas_reglas[i].regla);
                d.extend(todas_reglas[i].triggers.iter().cloned());
                d
            })
            .collect();
        let bm25_prueba = calcular_bm25(&query_tokens, &prueba_docs);
        let mut mejor_score = 0.0_f64;
        for (pos, &orig_idx) in prueba_indices.iter().enumerate() {
            let r = todas_reglas[orig_idx];
            let trig = coincidencia_triggers(&query_tokens, &query_ngrams, r);
            if bm25_prueba[pos] <= 0.0 && trig <= 0.0 {
                continue;
            }
            let thompson = muestrear_beta(r.alpha, r.beta, seed_base ^ (orig_idx as u64).wrapping_mul(0x9e37));
            let boost_def = if coincide_alcance_defensor(&r.defensor_alcance, &def_clave) {
                1.5
            } else {
                1.0
            };
            let score = thompson * (0.5 + bm25_prueba[pos] + trig) * factor_modelo(r) * boost_def;
            if score > mejor_score {
                mejor_score = score;
                regla_exploracion = Some((orig_idx, r));
            }
        }
    }

    // 2) Reglas "activa" seleccionadas por BM25 + triggers + límite inferior bayesiano + MMR (lambda = 0.7)
    let activas_indices: Vec<usize> = todas_reglas
        .iter()
        .enumerate()
        .filter(|(_, r)| r.estado == "activa" || r.estado.is_empty())
        .map(|(i, _)| i)
        .collect();

    let mut reglas_elegidas: Vec<&'a ReglaAprendida> = Vec::new();
    let k_activas = if regla_exploracion.is_some() {
        MAX_REGLAS_PROMPT.saturating_sub(1)
    } else {
        MAX_REGLAS_PROMPT
    };

    if !activas_indices.is_empty() && k_activas > 0 {
        let doc_tokens: Vec<Vec<String>> = activas_indices
            .iter()
            .map(|&i| {
                let mut t = tokenizar_judicial(&todas_reglas[i].regla);
                t.extend(todas_reglas[i].triggers.iter().cloned());
                t
            })
            .collect();
        let bm25 = calcular_bm25(&query_tokens, &doc_tokens);
        let max_bm25 = bm25.iter().copied().fold(0.0_f64, f64::max);

        let mut candidatos: Vec<(usize, f64)> = activas_indices
            .iter()
            .enumerate()
            .map(|(pos, &orig_idx)| {
                let r = todas_reglas[orig_idx];
                let bm25_norm = if max_bm25 > 0.0 {
                    bm25[pos] / max_bm25
                } else {
                    0.0
                };
                let trig = coincidencia_triggers(&query_tokens, &query_ngrams, r);
                let fiabilidad = limite_inferior_bayesiano(r.alpha, r.beta);
                let relevancia = (bm25_norm * 0.60 + trig * 0.25 + fiabilidad * 0.15)
                    * factor_modelo(r)
                    * factor_defensor_regla(r);
                (pos, relevancia)
            })
            .collect();

        candidatos.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| activas_indices[b.0].cmp(&activas_indices[a.0]))
        });

        let elegidas_pos = seleccionar_mmr(&candidatos, &doc_tokens, k_activas, 0.7);
        for pos in elegidas_pos {
            reglas_elegidas.push(todas_reglas[activas_indices[pos]]);
        }
    }

    if let Some((_, exp_regla)) = regla_exploracion {
        if !reglas_elegidas.iter().any(|r| r.id == exp_regla.id) {
            reglas_elegidas.push(exp_regla);
        }
    }

    // 3) Ejemplos balanceados vía BM25 + MMR (hasta 2 correcciones verificadas + 1 acierto ancla; tope 3)
    //    Se priorizan ejemplos del defensor activo y se hereda del Cuaderno General (defensor_clave == "").
    let local_corr_ids: BTreeSet<&str> = local.correcciones.iter().map(|c| c.id.as_str()).collect();
    let corrs: Vec<&'a CorreccionIa> = local
        .correcciones
        .iter()
        .chain(
            base.correcciones
                .iter()
                .filter(|c| !local_corr_ids.contains(c.id.as_str())),
        )
        .filter(|c| {
            c.estado == "analizada"
                && !c.campos.is_empty()
                && (es_alcance_general(&c.defensor_clave)
                    || coincide_alcance_defensor(&c.defensor_clave, &def_clave))
        })
        .collect();
    let corr_docs: Vec<Vec<String>> = corrs
        .iter()
        .map(|c| tokenizar_judicial(&c.texto_original))
        .collect();
    let corr_bm25 = calcular_bm25(&query_tokens, &corr_docs);
    let max_corr_bm25 = corr_bm25.iter().copied().fold(0.0_f64, f64::max);

    let mut cand_corr: Vec<(usize, f64)> = corrs
        .iter()
        .enumerate()
        .filter_map(|(pos, c)| {
            let sim_dice = similitud_consulta(texto_nuevo, &c.texto_original);
            let rel_bm25 = if max_corr_bm25 > 0.0 {
                corr_bm25[pos] / max_corr_bm25
            } else {
                0.0
            };
            let boost_def = if coincide_alcance_defensor(&c.defensor_clave, &def_clave) {
                1.35
            } else {
                1.0
            };
            let rel = (0.7 * rel_bm25 + 0.3 * sim_dice) * boost_def;
            if sim_dice > 0.02 || corr_bm25[pos] > 0.05 {
                Some((pos, rel))
            } else {
                None
            }
        })
        .collect();
    cand_corr.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.0.cmp(&a.0))
    });

    // 3b) Pista suave desde la 1.ª corrección pendiente del mismo defensor SOLO si el texto es casi idéntico (>= 0.72)
    let mut pista_pendiente: Option<&'a CorreccionIa> = None;
    let ya_hay_verificada_identica = corrs
        .iter()
        .any(|c| similitud_consulta(texto_nuevo, &c.texto_original) >= UMBRAL_SIMILITUD_PISTA_PENDIENTE);
    if !ya_hay_verificada_identica {
        let mut mejor_pista: Option<(f64, &'a CorreccionIa)> = None;
        for c in local.correcciones.iter().rev() {
            if !matches!(c.estado.as_str(), "pendiente" | "esperando_aclaracion")
                || c.campos.is_empty()
            {
                continue;
            }
            let mismo_ambito = if !def_clave.is_empty() {
                coincide_alcance_defensor(&c.defensor_clave, &def_clave)
            } else {
                c.defensor_clave.trim().is_empty()
            };
            if !mismo_ambito {
                continue;
            }
            let sim_cons = similitud_consulta(texto_nuevo, &c.texto_original);
            let sim_bm25 = similitud_bm25_normalizada(texto_nuevo, &c.texto_original);
            let sim = if sim_cons >= UMBRAL_SIMILITUD_PISTA_PENDIENTE {
                sim_cons.max(sim_bm25)
            } else if sim_bm25 >= UMBRAL_SIMILITUD_PISTA_PENDIENTE && sim_cons >= 0.55 {
                sim_bm25
            } else {
                0.0
            };
            if sim >= UMBRAL_SIMILITUD_PISTA_PENDIENTE
                && mejor_pista.is_none_or(|(best_sim, _)| sim > best_sim)
            {
                mejor_pista = Some((sim, c));
            }
        }
        pista_pendiente = mejor_pista.map(|(_, c)| c);
    }

    let local_ac_ids: BTreeSet<&str> = local
        .ejemplos_aciertos
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    let aciertos_pool: Vec<&'a EjemploPositivo> = local
        .ejemplos_aciertos
        .iter()
        .chain(
            base.ejemplos_aciertos
                .iter()
                .filter(|e| !local_ac_ids.contains(e.id.as_str())),
        )
        .filter(|e| {
            es_alcance_general(&e.defensor_clave)
                || coincide_alcance_defensor(&e.defensor_clave, &def_clave)
        })
        .collect();
    let aciertos_docs: Vec<Vec<String>> = aciertos_pool
        .iter()
        .map(|e| tokenizar_judicial(&e.texto_anonimizado))
        .collect();
    let aciertos_bm25 = calcular_bm25(&query_tokens, &aciertos_docs);
    let max_aciertos_bm25 = aciertos_bm25.iter().copied().fold(0.0_f64, f64::max);

    let mut cand_aciertos: Vec<(usize, f64)> = aciertos_pool
        .iter()
        .enumerate()
        .filter_map(|(pos, e)| {
            let sim_dice = similitud_consulta(texto_nuevo, &e.texto_anonimizado);
            let rel_bm25 = if max_aciertos_bm25 > 0.0 {
                aciertos_bm25[pos] / max_aciertos_bm25
            } else {
                0.0
            };
            let boost_def = if !def_clave.is_empty() && e.defensor_clave == def_clave {
                1.25
            } else {
                1.0
            };
            let rel = (0.7 * rel_bm25 + 0.3 * sim_dice) * boost_def;
            if sim_dice > 0.02 || aciertos_bm25[pos] > 0.05 {
                Some((pos, rel))
            } else {
                None
            }
        })
        .collect();
    cand_aciertos.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.0.cmp(&a.0))
    });

    let cupo_disponible = if pista_pendiente.is_some() {
        MAX_EJEMPLOS_PROMPT.saturating_sub(1)
    } else {
        MAX_EJEMPLOS_PROMPT
    };
    let cupo_aciertos = if cand_aciertos.is_empty() || cupo_disponible == 0 {
        0
    } else if cand_corr.len() >= cupo_disponible {
        if cupo_disponible >= 2 { 1 } else { 0 }
    } else {
        (cupo_disponible - cand_corr.len()).min(cand_aciertos.len())
    };
    let cupo_corr = (cupo_disponible - cupo_aciertos).min(cand_corr.len());

    let corr_sel_pos = seleccionar_mmr(&cand_corr, &corr_docs, cupo_corr, 0.7);
    let aciertos_sel_pos = seleccionar_mmr(&cand_aciertos, &aciertos_docs, cupo_aciertos, 0.7);

    let ejemplos_correcciones: Vec<&'a CorreccionIa> =
        corr_sel_pos.into_iter().map(|pos| corrs[pos]).collect();
    let ejemplos_aciertos: Vec<&'a EjemploPositivo> = aciertos_sel_pos
        .into_iter()
        .map(|pos| aciertos_pool[pos])
        .collect();

    let reglas_inyectadas_ids = reglas_elegidas.iter().map(|r| r.id.clone()).collect();
    let mut ejemplos_inyectados_ids: Vec<String> =
        ejemplos_correcciones.iter().map(|c| c.id.clone()).collect();
    if let Some(pista) = pista_pendiente {
        ejemplos_inyectados_ids.push(pista.id.clone());
    }
    ejemplos_inyectados_ids.extend(ejemplos_aciertos.iter().map(|e| e.id.clone()));

    ContextoPromptDetalle {
        reglas: reglas_elegidas,
        ejemplos_correcciones,
        ejemplos_aciertos,
        pista_pendiente,
        reglas_inyectadas_ids,
        ejemplos_inyectados_ids,
    }
}

pub fn seleccionar_contexto_detallado_para_modelo<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
) -> ContextoPromptDetalle<'a> {
    seleccionar_contexto_detallado_para_defensor(texto_nuevo, base, local, modelo_actual, None)
}

pub fn seleccionar_contexto_detallado<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> ContextoPromptDetalle<'a> {
    seleccionar_contexto_detallado_para_modelo(texto_nuevo, base, local, None)
}

/// Selecciona hasta `MAX_REGLAS_PROMPT` (5) reglas y hasta `MAX_EJEMPLOS_PROMPT` (3) correcciones
/// similares al `texto_nuevo`, combinando una memoria base y la memoria local del usuario.
pub fn seleccionar_contexto_para_prompt<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> (Vec<&'a ReglaAprendida>, Vec<&'a CorreccionIa>) {
    let det = seleccionar_contexto_detallado(texto_nuevo, base, local);
    (det.reglas, det.ejemplos_correcciones)
}

fn recortar_texto(s: &str, max_chars: usize) -> String {
    let limpio = colapsar_espacios(s);
    let chars: Vec<char> = limpio.chars().collect();
    if chars.len() <= max_chars {
        limpio
    } else {
        let prefijo: String = chars[..max_chars].iter().collect();
        format!("{}...", prefijo)
    }
}

pub fn formatear_contexto_prompt_con_detalle_para_defensor<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
    defensor_actual: Option<&str>,
) -> (String, Vec<String>, Vec<String>) {
    let det = seleccionar_contexto_detallado_para_defensor(
        texto_nuevo,
        base,
        local,
        modelo_actual,
        defensor_actual,
    );
    if det.reglas.is_empty()
        && det.ejemplos_correcciones.is_empty()
        && det.ejemplos_aciertos.is_empty()
        && det.pista_pendiente.is_none()
    {
        return (String::new(), Vec::new(), Vec::new());
    }

    let mut lineas = Vec::new();
    lineas.push(
        "APRENDIZAJE DE CORRECCIONES PREVIAS (aplica estas reglas y ejemplos con prioridad):"
            .to_string(),
    );

    if !det.reglas.is_empty() {
        lineas.push("Reglas aprendidas del usuario:".to_string());
        for r in &det.reglas {
            let etiqueta_alcance = if !r.defensor_alcance.is_empty() && r.defensor_alcance != "*" {
                "perfil defensor"
            } else {
                r.tipo_error.as_str()
            };
            lineas.push(format!("- [{}] {}", etiqueta_alcance, r.regla));
        }
    }

    if !det.ejemplos_correcciones.is_empty()
        || !det.ejemplos_aciertos.is_empty()
        || det.pista_pendiente.is_some()
    {
        lineas.push("Ejemplos corregidos en textos similares:".to_string());
        for ej in &det.ejemplos_correcciones {
            let extracto = recortar_texto(&ej.texto_original, 150);
            let cambios: Vec<String> = ej
                .campos
                .iter()
                .map(|c| {
                    format!(
                        "{}: '{}' -> '{}'",
                        c.campo,
                        recortar_texto(&c.valor_ia, 50),
                        recortar_texto(&c.valor_usuario, 50)
                    )
                })
                .collect();
            let nota = if !ej.nota_usuario.trim().is_empty() {
                format!(
                    " (Aclaración del usuario: {})",
                    recortar_texto(&ej.nota_usuario, 100)
                )
            } else if !ej.regla_generada.trim().is_empty() {
                format!(" (Regla: {})", recortar_texto(&ej.regla_generada, 100))
            } else {
                String::new()
            };
            lineas.push(format!(
                "- Texto similar: \"{}\" => Corrección oficial [{}]{}",
                extracto,
                cambios.join("; "),
                nota
            ));
        }
        if let Some(pista) = det.pista_pendiente {
            let extracto = recortar_texto(&pista.texto_original, 150);
            let cambios: Vec<String> = pista
                .campos
                .iter()
                .map(|c| {
                    format!(
                        "{}: '{}' -> '{}'",
                        c.campo,
                        recortar_texto(&c.valor_ia, 50),
                        recortar_texto(&c.valor_usuario, 50)
                    )
                })
                .collect();
            lineas.push(format!(
                "- Pista reciente del defensor en texto casi idéntico (1.ª corrección): \"{}\" => El defensor ajustó [{}] (aplícala si coincide el mismo patrón)",
                extracto,
                cambios.join("; ")
            ));
        }
        for ac in &det.ejemplos_aciertos {
            let extracto = recortar_texto(&ac.texto_anonimizado, 150);
            lineas.push(format!(
                "- Texto similar verificado: \"{}\" => Extracción confirmada [tipo='{}', juzgado='{}', realizada='{}', motivo='{}']",
                extracto,
                recortar_texto(&ac.salida_anonimizada.tipo, 45),
                recortar_texto(&ac.salida_anonimizada.juzgado, 55),
                ac.salida_anonimizada.realizada,
                ac.salida_anonimizada.motivo
            ));
        }
    }

    (
        lineas.join("\n"),
        det.reglas_inyectadas_ids,
        det.ejemplos_inyectados_ids,
    )
}

pub fn formatear_contexto_prompt_con_detalle_para_modelo<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
) -> (String, Vec<String>, Vec<String>) {
    formatear_contexto_prompt_con_detalle_para_defensor(
        texto_nuevo,
        base,
        local,
        modelo_actual,
        None,
    )
}

/// Construye el bloque de texto que se inyecta en `construir_prompt` con máximo 5 reglas y 3 ejemplos.
pub fn formatear_contexto_prompt_con_detalle<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> (String, Vec<String>, Vec<String>) {
    formatear_contexto_prompt_con_detalle_para_modelo(texto_nuevo, base, local, None)
}

pub fn formatear_contexto_prompt(
    texto_nuevo: &str,
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
) -> String {
    formatear_contexto_prompt_con_detalle(texto_nuevo, base, local).0
}

// ---------------------------------------------------------------------------
// Anonimización para exportar
// ---------------------------------------------------------------------------

#[inline]
fn es_marca_combinante(c: char) -> bool {
    ('\u{0300}'..='\u{036f}').contains(&c)
}

#[inline]
fn es_inicio_palabra_anon(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[inline]
fn es_char_palabra_anon(c: char) -> bool {
    es_inicio_palabra_anon(c) || es_marca_combinante(c)
}

#[inline]
fn contar_digitos_ascii_hasta(texto: &str, limite: usize) -> usize {
    let mut n = 0usize;
    for &b in texto.as_bytes() {
        if b.is_ascii_digit() {
            n += 1;
            if n >= limite {
                return n;
            }
        }
    }
    n
}

fn plegar_char_anon(c: char, out: &mut String) {
    if es_marca_combinante(c) {
        return;
    }
    for lc in c.to_lowercase() {
        if es_marca_combinante(lc) {
            continue;
        }
        let mapped = match lc {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        };
        out.push(mapped);
    }
}

#[derive(Clone, Copy)]
struct TokenSpan {
    orig_start: usize,
    orig_end: usize,
    fold_start: usize,
    fold_end: usize,
}

fn tokenizar_con_spans(texto: &str) -> (Vec<TokenSpan>, String) {
    let mut tokens = Vec::new();
    let mut fold_buf = String::with_capacity(texto.len());
    let mut iter = texto.char_indices().peekable();

    while let Some((start_idx, ch)) = iter.next() {
        if !es_inicio_palabra_anon(ch) {
            continue;
        }
        let fold_start = fold_buf.len();
        plegar_char_anon(ch, &mut fold_buf);
        let mut orig_end = start_idx + ch.len_utf8();

        while let Some(&(next_idx, next_ch)) = iter.peek() {
            if es_char_palabra_anon(next_ch) {
                iter.next();
                plegar_char_anon(next_ch, &mut fold_buf);
                orig_end = next_idx + next_ch.len_utf8();
            } else {
                break;
            }
        }
        let fold_end = fold_buf.len();
        if fold_end > fold_start {
            tokens.push(TokenSpan {
                orig_start: start_idx,
                orig_end,
                fold_start,
                fold_end,
            });
        }
    }

    (tokens, fold_buf)
}

fn re_numeros_largos() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d[\d.\-]*\d").ok()).as_ref()
}

fn re_fecha_con_guiones() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,2}[.\-]\d{1,2}[.\-]\d{2,4}$").ok())
        .as_ref()
}

fn re_separador_radicado() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+[-–—]\s+").ok()).as_ref()
}

fn re_telefono_espaciado() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:3\d{2}|60\d)\s+\d{3}\s+\d{4}\b").ok())
        .as_ref()
}

fn re_radicado_espaciado() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b\d{5}\s+(?:60|61)\s+\d{5}(?:\s+\d{4}\s+\d{5}|\s+\d{6,16})\b").ok()
    })
    .as_ref()
}

/// Reemplaza secuencias numéricas de 6 o más dígitos (radicados, teléfonos y cédulas) por
/// `[RADICADO]`, `[TELEFONO]` o `[NUMERO]`, conservando fechas y horas.
fn enmascarar_numeros_largos<'a>(texto: &'a str) -> Cow<'a, str> {
    let digitos = contar_digitos_ascii_hasta(texto, 18);
    if digitos < 6 {
        return Cow::Borrowed(texto);
    }
    let mut paso: Cow<'a, str> = Cow::Borrowed(texto);
    if digitos >= 18 {
        if let Some(re_rad) = re_radicado_espaciado() {
            if let Cow::Owned(s) = re_rad.replace_all(&paso, "[RADICADO]") {
                paso = Cow::Owned(s);
            }
        }
    }
    if digitos >= 10 {
        if let Some(re_tel) = re_telefono_espaciado() {
            if let Cow::Owned(s) = re_tel.replace_all(&paso, "[TELEFONO]") {
                paso = Cow::Owned(s);
            }
        }
    }
    let Some(re) = re_numeros_largos() else {
        return paso;
    };
    let mut hubo_reemplazo = false;
    let res = re.replace_all(&paso, |caps: &regex::Captures| {
        let m = &caps[0];
        let mut n_dig = 0usize;
        let mut pref = [0u8; 3];
        for &b in m.as_bytes() {
            if b.is_ascii_digit() {
                if n_dig < 3 {
                    pref[n_dig] = b;
                }
                n_dig += 1;
            }
        }
        let es_fecha = re_fecha_con_guiones().is_some_and(|rf| rf.is_match(m));
        if es_fecha || n_dig < 6 {
            m.to_string()
        } else if (n_dig == 10 && (pref[0] == b'3' || (pref[0] == b'6' && pref[1] == b'0')))
            || (n_dig == 12 && pref == *b"573")
        {
            hubo_reemplazo = true;
            "[TELEFONO]".to_string()
        } else if n_dig >= 15 {
            hubo_reemplazo = true;
            "[RADICADO]".to_string()
        } else {
            hubo_reemplazo = true;
            "[NUMERO]".to_string()
        }
    });
    if hubo_reemplazo {
        Cow::Owned(res.into_owned())
    } else {
        paso
    }
}

fn es_termino_estructural_judicial(tok_fold: &str) -> bool {
    matches!(
        tok_fold,
        "juzgado"
            | "juzgados"
            | "juez"
            | "audiencia"
            | "audiencias"
            | "penal"
            | "circuito"
            | "municipal"
            | "promiscuo"
            | "conocimiento"
            | "garantias"
            | "especializado"
            | "especializada"
            | "mixto"
            | "mixta"
            | "despacho"
            | "despachos"
            | "tribunal"
            | "tribunales"
            | "superior"
            | "sala"
            | "ejecucion"
            | "pena"
            | "penas"
            | "medidas"
            | "seguridad"
            | "adolescentes"
            | "distrito"
            | "judicial"
            | "funcion"
            | "funciones"
            | "control"
            | "fiscalia"
            | "defensor"
            | "publico"
            | "imputacion"
            | "acusacion"
            | "preparatoria"
            | "concentrada"
            | "preliminar"
            | "preclusion"
            | "nulidad"
            | "prorroga"
            | "revision"
            | "verificacion"
            | "cumplimiento"
            | "incidente"
            | "reparacion"
            | "integral"
            | "conciliacion"
            | "alegatos"
            | "conclusion"
            | "sustitucion"
            | "revocatoria"
            | "libertad"
            | "terminos"
            | "vencimiento"
            | "preacuerdo"
            | "allanamiento"
            | "apelacion"
            | "captura"
            | "legalizacion"
            | "rebaja"
            | "acumulacion"
            | "lectura"
            | "fallo"
            | "sentencia"
            | "victima"
            | "usuario"
            | "usuaria"
            | "procesado"
            | "procesada"
            | "radicado"
            | "numero"
            | "telefono"
            | "inpec"
            | "icbf"
            | "arma"
            | "armas"
            | "campo"
            | "campos"
            | "fuente"
            | "fuentes"
            | "sigla"
            | "siglas"
            | "acta"
            | "actas"
            | "delito"
            | "delitos"
            | "proceso"
            | "procesos"
            | "planilla"
            | "planillas"
            | "regla"
            | "reglas"
            | "correccion"
            | "correcciones"
            | "abreviatura"
            | "abreviaturas"
            | "motivo"
            | "motivos"
            | "observacion"
            | "observaciones"
            | "nro"
            | "cui"
            | "prision"
            | "fuego"
            | "municion"
            | "municiones"
    )
}

fn es_token_anonimizacion_valido(tok_fold: &str) -> bool {
    tok_fold.chars().count() >= 3
        && tok_fold.chars().any(char::is_alphabetic)
        && !PALABRAS_VACIAS_NOMBRE.contains(&tok_fold)
        && !es_termino_estructural_judicial(tok_fold)
}

const FLAG_TRIE_FINAL: u8 = 1;
const FLAG_TRIE_PERMITE_COMA: u8 = 2;
const FLAG_TRIE_FINAL_PUNT: u8 = 4;

#[derive(Default)]
struct TrieFrases {
    transiciones: HashMap<(u32, u32), u32>,
    flags: Vec<u8>,
}

impl TrieFrases {
    fn new() -> Self {
        Self {
            transiciones: HashMap::new(),
            flags: vec![0],
        }
    }

    fn esta_vacio(&self) -> bool {
        self.transiciones.is_empty()
    }

    fn insertar_con_flags(&mut self, ids: &[u32], flags_extra: u8) {
        if ids.is_empty() {
            return;
        }
        if self.flags.is_empty() {
            self.flags.push(0);
        }
        let mut nodo = 0u32;
        for &wid in ids {
            if let Some(&sig) = self.transiciones.get(&(nodo, wid)) {
                nodo = sig;
            } else {
                let nuevo = self.flags.len() as u32;
                self.flags.push(0);
                self.transiciones.insert((nodo, wid), nuevo);
                nodo = nuevo;
            }
        }
        self.flags[nodo as usize] |= flags_extra;
    }

    fn insertar(&mut self, ids: &[u32], permite_coma: bool) {
        let mut fl = FLAG_TRIE_FINAL;
        if permite_coma {
            fl |= FLAG_TRIE_PERMITE_COMA;
        }
        self.insertar_con_flags(ids, fl);
    }

    #[inline]
    fn siguiente(&self, nodo: u32, wid: u32) -> Option<u32> {
        self.transiciones.get(&(nodo, wid)).copied()
    }

    #[inline]
    fn flags_nodo(&self, nodo: u32) -> u8 {
        self.flags.get(nodo as usize).copied().unwrap_or(0)
    }
}

/// Índice de búsqueda lineal para radicados conocidos (exactos o pegados a texto sin espacio),
/// evitando compilar miles de expresiones regulares individuales.
struct IndiceRadicados {
    patrones: HashSet<Box<[u8]>>,
    primer_byte: [bool; 256],
    longitudes_por_prefijo5: HashMap<[u8; 5], Vec<usize>>,
}

impl Default for IndiceRadicados {
    fn default() -> Self {
        Self {
            patrones: HashSet::new(),
            primer_byte: [false; 256],
            longitudes_por_prefijo5: HashMap::new(),
        }
    }
}

impl IndiceRadicados {
    fn insertar(&mut self, patron: &str) {
        let bytes = patron.as_bytes();
        if bytes.len() < 5 {
            return;
        }
        if !self.patrones.insert(bytes.to_vec().into_boxed_slice()) {
            return;
        }
        self.primer_byte[bytes[0] as usize] = true;
        let pref5: [u8; 5] = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]];
        let longs = self.longitudes_por_prefijo5.entry(pref5).or_default();
        if !longs.contains(&bytes.len()) {
            longs.push(bytes.len());
            longs.sort_unstable_by(|a, b| b.cmp(a));
        }
    }

    fn esta_vacio(&self) -> bool {
        self.patrones.is_empty()
    }

    fn reemplazar<'a>(&self, texto: &'a str) -> Cow<'a, str> {
        if self.esta_vacio() || texto.len() < 5 || contar_digitos_ascii_hasta(texto, 5) < 5 {
            return Cow::Borrowed(texto);
        }
        let bytes = texto.as_bytes();
        let len = bytes.len();
        let mut out: Option<String> = None;
        let mut ultimo = 0usize;
        let mut i = 0usize;

        while i + 5 <= len {
            if self.primer_byte[bytes[i] as usize] {
                let pref5: [u8; 5] = [
                    bytes[i],
                    bytes[i + 1],
                    bytes[i + 2],
                    bytes[i + 3],
                    bytes[i + 4],
                ];
                if let Some(longs) = self.longitudes_por_prefijo5.get(&pref5) {
                    let mut match_len = 0usize;
                    for &l in longs {
                        if i + l <= len && self.patrones.contains(&bytes[i..i + l]) {
                            // Evitar cortar por la mitad una secuencia numérica continua más larga
                            // (`[0-9.-]+`, p. ej. un teléfono `3001234567`/`300-123-4567`, una fecha
                            // `15-08-2025` o un radicado más largo) porque `enmascarar_numeros_largos`
                            // la cubrirá completa; si el patrón contiene `/` o espacios, se reemplaza
                            // directamente para no dejar colas.
                            let es_num_continuo = bytes[i..i + l]
                                .iter()
                                .all(|b| b.is_ascii_digit() || *b == b'-' || *b == b'.');
                            let prev_continuacion = bytes[i].is_ascii_digit()
                                && ((i > 0 && bytes[i - 1].is_ascii_digit())
                                    || (i > 1
                                        && matches!(bytes[i - 1], b'-' | b'.')
                                        && bytes[i - 2].is_ascii_digit()));
                            let next_continuacion = bytes[i + l - 1].is_ascii_digit()
                                && ((i + l < len && bytes[i + l].is_ascii_digit())
                                    || (i + l + 1 < len
                                        && matches!(bytes[i + l], b'-' | b'.')
                                        && bytes[i + l + 1].is_ascii_digit()));
                            if !(es_num_continuo && (prev_continuacion || next_continuacion)) {
                                match_len = l;
                                break;
                            }
                        }
                    }
                    if match_len > 0 {
                        let buf = out.get_or_insert_with(|| String::with_capacity(len));
                        buf.push_str(&texto[ultimo..i]);
                        buf.push_str("[RADICADO]");
                        i += match_len;
                        ultimo = i;
                        continue;
                    }
                }
            }
            i += 1;
        }

        match out {
            Some(mut buf) => {
                buf.push_str(&texto[ultimo..]);
                Cow::Owned(buf)
            }
            None => Cow::Borrowed(texto),
        }
    }
}

#[inline]
fn es_sep_solo_espacios(sep: &str) -> bool {
    !sep.is_empty() && sep.chars().all(char::is_whitespace)
}

#[inline]
fn es_sep_post_honorifico(sep: &str) -> bool {
    if let Some(resto) = sep.strip_prefix('.') {
        es_sep_solo_espacios(resto)
    } else {
        es_sep_solo_espacios(sep)
    }
}

#[inline]
fn es_sep_inicial_con_punto(sep: &str, tok_prev_fold: &str) -> bool {
    sep.strip_prefix('.').is_some_and(|resto| {
        (resto.is_empty() || resto.chars().all(char::is_whitespace))
            && tok_prev_fold.chars().count() <= 2
            && !PALABRAS_VACIAS_NOMBRE.contains(&tok_prev_fold)
    })
}

#[inline]
fn es_sep_escudo_defensor(sep: &str, tok_prev_fold: &str) -> bool {
    es_sep_solo_espacios(sep)
        || matches!(sep, "-" | "–" | "—" | "'" | "’")
        || es_sep_inicial_con_punto(sep, tok_prev_fold)
}

fn detectar_prefijo_honorifico(
    texto: &str,
    tokens: &[TokenSpan],
    fold_buf: &str,
    consumido: Option<&[bool]>,
    i: usize,
) -> Option<usize> {
    let n = tokens.len();
    let esta_consumido = |idx: usize| consumido.is_some_and(|c| c[idx]);
    if i + 1 >= n || esta_consumido(i) {
        return None;
    }
    let t0 = &fold_buf[tokens[i].fold_start..tokens[i].fold_end];

    // Forma de 2 tokens: `defensor(a)? \s+ public[oa] \.? \s+`
    if matches!(t0, "defensor" | "defensora") && i + 2 < n && !esta_consumido(i + 1) {
        let sep01 = &texto[tokens[i].orig_end..tokens[i + 1].orig_start];
        let t1 = &fold_buf[tokens[i + 1].fold_start..tokens[i + 1].fold_end];
        if es_sep_solo_espacios(sep01) && matches!(t1, "publico" | "publica") {
            let sep12 = &texto[tokens[i + 1].orig_end..tokens[i + 2].orig_start];
            if es_sep_post_honorifico(sep12) {
                return Some(i + 2);
            }
        }
    }

    // Forma de 1 token: `(dr|dra|doctor|doctora|abg|abogado|abogada|defensor|defensora|lic|licenciado|licenciada) \.? \s+`
    if matches!(
        t0,
        "dr" | "dra"
            | "doctor"
            | "doctora"
            | "abg"
            | "abogado"
            | "abogada"
            | "defensor"
            | "defensora"
            | "lic"
            | "licenciado"
            | "licenciada"
    ) {
        let sep01 = &texto[tokens[i].orig_end..tokens[i + 1].orig_start];
        if es_sep_post_honorifico(sep01) {
            return Some(i + 1);
        }
    }
    None
}

#[derive(Default)]
struct EscudosDefensores {
    vocab: HashMap<String, u32>,
    sin_honorifico: TrieFrases,
    con_honorifico: TrieFrases,
}

impl EscudosDefensores {
    fn esta_vacio(&self) -> bool {
        self.sin_honorifico.esta_vacio() && self.con_honorifico.esta_vacio()
    }

    fn internar(&mut self, tok: &str) -> u32 {
        if let Some(&id) = self.vocab.get(tok) {
            return id;
        }
        let id = self.vocab.len() as u32;
        self.vocab.insert(tok.to_string(), id);
        id
    }

    fn insertar_con_hon(&mut self, tokens: &[&str]) {
        let ids: Vec<u32> = tokens.iter().map(|t| self.internar(t)).collect();
        self.con_honorifico.insertar(&ids, false);
    }

    fn insertar_sin_hon(&mut self, tokens: &[&str]) {
        let ids: Vec<u32> = tokens.iter().map(|t| self.internar(t)).collect();
        self.sin_honorifico.insertar(&ids, false);
    }

    fn agregar_claves_y_nombres(&mut self, claves_y_nombres: &[String]) {
        let mut claves_vistas: HashSet<String> = HashSet::new();
        for raw in claves_y_nombres {
            let dk = normalizar_clave_defensor(raw);
            if dk.is_empty() || !claves_vistas.insert(dk.clone()) {
                continue;
            }
            let tokens: Vec<&str> = dk.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            if tokens.len() >= 2 && tokens.iter().any(|t| es_token_anonimizacion_valido(t)) {
                self.insertar_sin_hon(&tokens);
                self.insertar_con_hon(&tokens);
                if tokens.len() >= 3 {
                    for win_len in (2..tokens.len()).rev() {
                        for win in tokens.windows(win_len) {
                            if win.iter().any(|t| es_token_anonimizacion_valido(t)) {
                                self.insertar_con_hon(win);
                            }
                        }
                    }
                }
                for tok in &tokens {
                    if es_token_anonimizacion_valido(tok) {
                        self.insertar_con_hon(&[*tok]);
                    }
                }
            } else if es_token_anonimizacion_valido(tokens[0]) {
                self.insertar_con_hon(&[tokens[0]]);
            }
        }
    }

    fn buscar_largo_en_trie(
        &self,
        trie: &TrieFrases,
        texto: &str,
        tokens: &[TokenSpan],
        fold_buf: &str,
        consumido: Option<&[bool]>,
        inicio: usize,
    ) -> Option<usize> {
        let esta_consumido = |idx: usize| consumido.is_some_and(|c| c[idx]);
        if inicio >= tokens.len() || esta_consumido(inicio) {
            return None;
        }
        let mut nodo = 0u32;
        let mut mejor_fin: Option<usize> = None;
        for k in inicio..tokens.len() {
            if esta_consumido(k) {
                break;
            }
            if k > inicio {
                let sep = &texto[tokens[k - 1].orig_end..tokens[k].orig_start];
                let prev_fold = &fold_buf[tokens[k - 1].fold_start..tokens[k - 1].fold_end];
                if !es_sep_escudo_defensor(sep, prev_fold) {
                    break;
                }
            }
            let tok_fold = &fold_buf[tokens[k].fold_start..tokens[k].fold_end];
            let Some(&wid) = self.vocab.get(tok_fold) else {
                break;
            };
            let Some(sig) = trie.siguiente(nodo, wid) else {
                break;
            };
            nodo = sig;
            if (trie.flags_nodo(nodo) & FLAG_TRIE_FINAL) != 0 {
                mejor_fin = Some(k);
            }
        }
        mejor_fin
    }

    fn detectar_rangos(
        &self,
        texto: &str,
        tokens: &[TokenSpan],
        fold_buf: &str,
    ) -> Vec<(usize, usize)> {
        let mut rangos = Vec::new();
        if self.esta_vacio() {
            return rangos;
        }
        for i in 0..tokens.len() {
            if let Some(inicio_nombre) =
                detectar_prefijo_honorifico(texto, tokens, fold_buf, None, i)
            {
                if let Some(fin) = self.buscar_largo_en_trie(
                    &self.con_honorifico,
                    texto,
                    tokens,
                    fold_buf,
                    None,
                    inicio_nombre,
                ) {
                    rangos.push((i, fin));
                }
            }
            if let Some(fin) = self.buscar_largo_en_trie(
                &self.sin_honorifico,
                texto,
                tokens,
                fold_buf,
                None,
                i,
            ) {
                rangos.push((i, fin));
            }
        }
        rangos
    }

    fn marcar_protegidos(
        &self,
        texto: &str,
        tokens: &[TokenSpan],
        fold_buf: &str,
        consumido: &[bool],
        protegido: &mut [bool],
    ) {
        if self.esta_vacio() {
            return;
        }
        let n = tokens.len();
        for i in 0..n {
            if consumido[i] {
                continue;
            }
            if let Some(inicio_nombre) =
                detectar_prefijo_honorifico(texto, tokens, fold_buf, Some(consumido), i)
            {
                if let Some(fin) = self.buscar_largo_en_trie(
                    &self.con_honorifico,
                    texto,
                    tokens,
                    fold_buf,
                    Some(consumido),
                    inicio_nombre,
                ) {
                    for p in &mut protegido[i..=fin] {
                        *p = true;
                    }
                }
            }
            if let Some(fin) = self.buscar_largo_en_trie(
                &self.sin_honorifico,
                texto,
                tokens,
                fold_buf,
                Some(consumido),
                i,
            ) {
                for p in &mut protegido[i..=fin] {
                    *p = true;
                }
            }
        }
    }
}

enum SepProcesado {
    Invalido,
    Espacio,
    Puntuacion,
    ConComa,
}

fn clasificar_sep_nombre_procesado(sep: &str, tok_prev_fold: &str) -> SepProcesado {
    if sep.is_empty() {
        return SepProcesado::Invalido;
    }
    if sep.chars().all(char::is_whitespace) {
        return SepProcesado::Espacio;
    }
    if matches!(sep, "-" | "–" | "—" | "'" | "’") {
        return SepProcesado::Puntuacion;
    }
    if es_sep_inicial_con_punto(sep, tok_prev_fold) {
        return SepProcesado::Puntuacion;
    }
    if let Some(resto) = sep.strip_prefix(',') {
        if resto.is_empty() || resto.chars().all(char::is_whitespace) {
            return SepProcesado::ConComa;
        }
    }
    SepProcesado::Invalido
}

#[inline]
fn es_etiqueta_radicado_fold(tok_fold: &str) -> bool {
    matches!(
        tok_fold,
        "rad" | "radicado" | "nro" | "num" | "proc" | "procesado"
    )
}

fn tiene_coma_entre_tokens(texto: &str, spans: &[TokenSpan]) -> bool {
    spans
        .windows(2)
        .any(|w| texto[w[0].orig_end..w[1].orig_start].contains(','))
}

fn extraer_frases_procesado(
    texto: &str,
    filtrar_etiquetas: bool,
) -> (Vec<String>, Vec<Vec<String>>, bool) {
    let (spans, fold) = tokenizar_con_spans(texto);
    let spans_filtrados: Vec<TokenSpan> = if filtrar_etiquetas {
        spans
            .into_iter()
            .filter(|sp| !es_etiqueta_radicado_fold(&fold[sp.fold_start..sp.fold_end]))
            .collect()
    } else {
        spans
    };
    let tokens_fold: Vec<String> = spans_filtrados
        .iter()
        .map(|sp| fold[sp.fold_start..sp.fold_end].to_string())
        .collect();
    let permite_coma = tiene_coma_entre_tokens(texto, &spans_filtrados);

    let mut subfrases: Vec<Vec<String>> = Vec::new();
    for bloque in texto.split_whitespace() {
        let (sub_spans, sub_fold) = tokenizar_con_spans(bloque);
        let sub: Vec<String> = sub_spans
            .iter()
            .map(|sp| sub_fold[sp.fold_start..sp.fold_end].to_string())
            .filter(|t| !filtrar_etiquetas || !es_etiqueta_radicado_fold(t))
            .collect();
        if sub.len() >= 2 {
            subfrases.push(sub);
        }
    }

    (tokens_fold, subfrases, permite_coma)
}

fn toponimos_judiciales_colombia() -> &'static HashSet<String> {
    static TOPONIMOS: OnceLock<HashSet<String>> = OnceLock::new();
    TOPONIMOS.get_or_init(|| {
        let mut set: HashSet<String> = [
            "san alberto",
            "san juan del cesar",
            "san juan cesar",
            "san martin",
            "san diego",
            "el paso",
            "del paso",
            "san marcos",
            "san jacinto",
            "san onofre",
            "san sebastian",
            "san andres",
            "san gil",
            "san vicente",
            "san jose",
            "san pedro",
            "san pablo",
            "san luis",
            "san carlos",
            "san antonio",
            "san francisco",
            "santa marta",
            "santa rosa",
            "santa ana",
            "santa barbara",
            "cesar",
            "magdalena",
            "guajira",
            "la guajira",
            "santander",
            "norte de santander",
            "bucaramanga",
            "barrancabermeja",
            "ocana",
            "cucuta",
            "bogota",
            "medellin",
            "cali",
            "barranquilla",
            "cartagena",
            "valledupar",
            "aguachica",
            "agustin codazzi",
            "bosconia",
            "chiriguana",
            "chimichagua",
            "curumani",
            "la jagua de ibirico",
            "jagua ibirico",
            "el copey",
            "cienaga",
            "fundacion",
            "el banco",
            "plato",
            "pivijay",
            "aracataca",
            "riohacha",
            "maicao",
            "fonseca",
            "villanueva",
            "uribia",
            "barrancas",
            "pailitas",
            "pelaya",
            "astrea",
            "becerril",
            "gamarra",
            "la paz",
            "manaure",
            "pueblo bello",
            "rio de oro",
            "tamalameque",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        for j in crate::estadistica_ia::juzgados_base() {
            let base = j.split(" - ").next().unwrap_or(j);
            let norm = normalizar_clave_catalogo(base);
            for sep in [
                "municipal de ",
                "circuito de ",
                "especializado de ",
                "adolescentes de ",
                "seguridad de ",
                "judicial de ",
                "garantias de ",
                "conocimiento de ",
            ] {
                if let Some(pos) = norm.rfind(sep) {
                    let suf = norm[pos + sep.len()..].trim();
                    let n_toks = suf.split_whitespace().count();
                    if (1..=4).contains(&n_toks)
                        && suf
                            .split_whitespace()
                            .any(|t| !es_termino_estructural_judicial(t))
                    {
                        set.insert(suf.to_string());
                        let sin_sw: Vec<&str> = suf
                            .split_whitespace()
                            .filter(|t| !matches!(*t, "de" | "del" | "la" | "las" | "los"))
                            .collect();
                        if !sin_sw.is_empty() && sin_sw.len() <= 4 {
                            set.insert(sin_sw.join(" "));
                        }
                    }
                }
            }
        }
        set
    })
}

fn marcar_toponimos_en_juzgados(
    texto: &str,
    tokens: &[TokenSpan],
    fold_buf: &str,
    consumido: &[bool],
    protegido: &mut [bool],
) {
    let n = tokens.len();
    if n < 2 {
        return;
    }
    let toponimos = toponimos_judiciales_colombia();
    let mut probe = String::with_capacity(48);
    let mut i = 0usize;
    while i < n {
        let inicio_fold = &fold_buf[tokens[i].fold_start..tokens[i].fold_end];
        if !matches!(
            inicio_fold,
            "juzgado"
                | "juzgados"
                | "jdo"
                | "jdos"
                | "tribunal"
                | "tribunales"
                | "despacho"
                | "despachos"
        ) {
            i += 1;
            continue;
        }
        let max_j = (i + 15).min(n);
        let mut j_end = i + 1;
        while j_end < max_j {
            let sep = &texto[tokens[j_end - 1].orig_end..tokens[j_end].orig_start];
            if sep.contains(';') || sep.contains('\n') || sep.contains('\r') || sep.contains('\t') {
                break;
            }
            let prev_fold = &fold_buf[tokens[j_end - 1].fold_start..tokens[j_end - 1].fold_end];
            if sep.contains('.')
                && !matches!(
                    prev_fold,
                    "jdo"
                        | "jdos"
                        | "mpal"
                        | "ppal"
                        | "cto"
                        | "pcto"
                        | "prom"
                        | "mun"
                        | "esp"
                        | "nro"
                        | "no"
                        | "num"
                        | "ejec"
                        | "gtia"
                        | "gtias"
                        | "con"
                        | "j"
                        | "p"
                )
                && !prev_fold.chars().all(|c| c.is_ascii_digit())
            {
                break;
            }
            let tok_fold = &fold_buf[tokens[j_end].fold_start..tokens[j_end].fold_end];
            if matches!(
                tok_fold,
                "contra"
                    | "seguido"
                    | "seguida"
                    | "adelantado"
                    | "adelantada"
                    | "procesado"
                    | "procesada"
                    | "radicado"
                    | "rad"
                    | "delito"
                    | "delitos"
                    | "donde"
                    | "fracasa"
                    | "fracsa"
                    | "reprograma"
                    | "reprogramo"
                    | "reprogramada"
                    | "solicito"
                    | "comparecio"
                    | "asistio"
                    | "informa"
            ) {
                break;
            }
            j_end += 1;
        }

        let mut k = i + 1;
        while k < j_end {
            let mut matched_len = 0usize;
            for len in (1..=4).rev() {
                if k + len > j_end {
                    continue;
                }
                if (k..k + len).any(|idx| consumido[idx]) {
                    continue;
                }
                probe.clear();
                for idx in k..k + len {
                    if idx > k {
                        probe.push(' ');
                    }
                    probe.push_str(&fold_buf[tokens[idx].fold_start..tokens[idx].fold_end]);
                }
                if toponimos.contains(&probe) {
                    matched_len = len;
                    break;
                }
            }
            if matched_len > 0 {
                for p in &mut protegido[k..k + matched_len] {
                    *p = true;
                }
                k += matched_len;
            } else {
                k += 1;
            }
        }
        i = j_end;
    }
}

/// Reemplaza únicamente el nombre del procesado, números de radicado, cédulas y teléfonos
/// por marcadores (`[PROCESADO]`, `[RADICADO]`, `[TELEFONO]`, `[NUMERO]`), preservando los
/// nombres reales de defensores públicos incluso si comparten un apellido con algún procesado.
pub(crate) struct Anonimizador {
    escudos_defensores: EscudosDefensores,
    radicados: IndiceRadicados,
    vocab: HashMap<String, u32>,
    frases_procesado: TrieFrases,
    subfrases_procesado: TrieFrases,
    es_palabra_procesado: Vec<bool>,
    tiene_palabras_procesado: bool,
}

impl Anonimizador {
    fn internar(&mut self, tok_fold: &str) -> u32 {
        if let Some(&id) = self.vocab.get(tok_fold) {
            return id;
        }
        let id = self.vocab.len() as u32;
        self.vocab.insert(tok_fold.to_string(), id);
        self.es_palabra_procesado.push(false);
        id
    }

    fn registrar_frase_y_palabras(
        &mut self,
        tokens_fold: &[String],
        subfrases: &[Vec<String>],
        permite_coma: bool,
    ) {
        if !tokens_fold.iter().any(|t| es_token_anonimizacion_valido(t)) {
            return;
        }
        if tokens_fold.len() >= 2 {
            let ids: Vec<u32> = tokens_fold.iter().map(|t| self.internar(t)).collect();
            self.frases_procesado.insertar(&ids, permite_coma);
        }
        for sub in subfrases {
            if sub.len() >= 2 && sub.iter().any(|t| es_token_anonimizacion_valido(t)) {
                let ids: Vec<u32> = sub.iter().map(|t| self.internar(t)).collect();
                self.subfrases_procesado
                    .insertar_con_flags(&ids, FLAG_TRIE_FINAL_PUNT);
            }
        }
        for tok in tokens_fold {
            if !es_token_anonimizacion_valido(tok) {
                continue;
            }
            let id = self.internar(tok);
            self.es_palabra_procesado[id as usize] = true;
            self.tiene_palabras_procesado = true;
        }
    }

    fn desde_radicados(radicados: &[&str]) -> Self {
        let mut anon = Self {
            escudos_defensores: EscudosDefensores::default(),
            radicados: IndiceRadicados::default(),
            vocab: HashMap::new(),
            frases_procesado: TrieFrases::new(),
            subfrases_procesado: TrieFrases::new(),
            es_palabra_procesado: Vec::new(),
            tiene_palabras_procesado: false,
        };

        let mut radicados_vistos: HashSet<&str> = HashSet::with_capacity(radicados.len());
        let mut partes_vistas: HashSet<String> = HashSet::new();

        for &radicado in radicados {
            let rad_trim = radicado.trim();
            if rad_trim.is_empty() || !radicados_vistos.insert(rad_trim) {
                continue;
            }
            let partes: Vec<String> = match re_separador_radicado() {
                Some(re) => re.split(rad_trim).map(colapsar_espacios).collect(),
                None => vec![colapsar_espacios(rad_trim)],
            };

            for parte in partes.into_iter().filter(|p| !p.is_empty()) {
                if !partes_vistas.insert(parte.clone()) {
                    continue;
                }
                let digitos = parte.bytes().filter(u8::is_ascii_digit).count();
                if digitos >= 5 {
                    let chars: Vec<char> = parte.chars().collect();
                    let len_c = chars.len();
                    let mut num_runs: Vec<String> = Vec::new();
                    let mut texto_no_num = String::with_capacity(parte.len());
                    let mut idx = 0usize;
                    while idx < len_c {
                        if chars[idx].is_ascii_digit() {
                            let start = idx;
                            let mut total_dig = 0usize;
                            let mut end_valido = idx;
                            let mut j = idx;
                            while j < len_c {
                                let c = chars[j];
                                if c.is_ascii_digit() {
                                    total_dig += 1;
                                    j += 1;
                                    end_valido = j;
                                } else if matches!(c, '-' | '.' | '/')
                                    && j + 1 < len_c
                                    && chars[j + 1].is_ascii_digit()
                                {
                                    j += 1;
                                } else if c == ' '
                                    && total_dig >= 2
                                    && j + 1 < len_c
                                    && chars[j + 1].is_ascii_digit()
                                {
                                    // Mantener unido un radicado espaciado (`20001 60 01075 2025 00777`)
                                    // para no aislar el prefijo DANE (`20001`) como patrón suelto.
                                    j += 1;
                                } else {
                                    break;
                                }
                            }
                            if total_dig >= 5 {
                                let run: String = chars[start..end_valido].iter().collect();
                                num_runs.push(run);
                            }
                            texto_no_num.push(' ');
                            idx = end_valido;
                        } else {
                            texto_no_num.push(chars[idx]);
                            idx += 1;
                        }
                    }

                    let tiene_alfa = texto_no_num.chars().any(char::is_alphabetic);
                    if !tiene_alfa {
                        anon.radicados.insertar(&parte);
                        if num_runs.len() == 1
                            && num_runs[0] != parte
                            && num_runs[0].bytes().filter(u8::is_ascii_digit).count() == digitos
                        {
                            anon.radicados.insertar(&num_runs[0]);
                        }
                    } else {
                        for nr in &num_runs {
                            anon.radicados.insertar(nr);
                        }
                        let (tokens_alfa, subfrases, permite_coma) =
                            extraer_frases_procesado(&texto_no_num, true);
                        anon.registrar_frase_y_palabras(&tokens_alfa, &subfrases, permite_coma);
                    }
                    continue;
                }

                let (tokens_fold, subfrases, permite_coma) =
                    extraer_frases_procesado(&parte, false);
                anon.registrar_frase_y_palabras(&tokens_fold, &subfrases, permite_coma);
            }
        }

        anon
    }

    fn con_escudos_defensores(mut self, claves_y_nombres: &[String]) -> Self {
        self.escudos_defensores
            .agregar_claves_y_nombres(claves_y_nombres);
        self
    }

    fn aplicar_nombres_y_escudos<'a>(
        &self,
        texto: &'a str,
        escudos: &EscudosDefensores,
    ) -> Cow<'a, str> {
        let (tokens, fold_buf) = tokenizar_con_spans(texto);
        let n = tokens.len();
        if n == 0 {
            return Cow::Borrowed(texto);
        }

        const SIN_ID: u32 = u32::MAX;
        let mut word_ids: Vec<u32> = Vec::with_capacity(n);
        let mut algun_conocido = false;
        for tok in &tokens {
            let slice = &fold_buf[tok.fold_start..tok.fold_end];
            if let Some(&id) = self.vocab.get(slice) {
                word_ids.push(id);
                algun_conocido = true;
            } else {
                word_ids.push(SIN_ID);
            }
        }

        if !algun_conocido {
            return Cow::Borrowed(texto);
        }

        // 0) Detectar rangos de defensores (escudos) para impedir que una frase con coma
        //    (`López, Fernando`) o una frase parcial estrictamente interna (`Juan Carlos` dentro
        //    de `Dr. Juan Carlos López`) rompa el escudo del defensor.
        let rangos_escudos = if !escudos.esta_vacio() {
            escudos.detectar_rangos(texto, &tokens, &fold_buf)
        } else {
            Vec::new()
        };

        let mut reemplazo_fin: Vec<Option<usize>> = vec![None; n];
        let mut consumido: Vec<bool> = vec![false; n];
        let mut hubo_reemplazo = false;

        // 1) Frases completas de procesados (coincidencia más larga de izquierda a derecha)
        if !self.frases_procesado.esta_vacio() {
            let mut i = 0usize;
            while i < n {
                let wid0 = word_ids[i];
                if wid0 == SIN_ID {
                    i += 1;
                    continue;
                }
                let Some(mut nodo) = self.frases_procesado.siguiente(0, wid0) else {
                    i += 1;
                    continue;
                };
                let mut uso_coma = false;
                let mut mejor_fin: Option<usize> = None;
                for k in (i + 1)..n {
                    let sep = &texto[tokens[k - 1].orig_end..tokens[k].orig_start];
                    let prev_fold = &fold_buf[tokens[k - 1].fold_start..tokens[k - 1].fold_end];
                    match clasificar_sep_nombre_procesado(sep, prev_fold) {
                        SepProcesado::Invalido => break,
                        SepProcesado::Espacio | SepProcesado::Puntuacion => {}
                        SepProcesado::ConComa => {
                            uso_coma = true;
                        }
                    }
                    let wid = word_ids[k];
                    if wid == SIN_ID {
                        break;
                    }
                    let Some(sig) = self.frases_procesado.siguiente(nodo, wid) else {
                        break;
                    };
                    nodo = sig;
                    let fl = self.frases_procesado.flags_nodo(nodo);
                    let acepta_normal = (fl & FLAG_TRIE_FINAL) != 0
                        && (!uso_coma || (fl & FLAG_TRIE_PERMITE_COMA) != 0);
                    if acepta_normal {
                        let bloqueado_por_escudo = rangos_escudos.iter().any(|&(ds, de)| {
                            (uso_coma && ((ds < i && i <= de) || (ds <= k && k < de)))
                                || (ds < i && k < de)
                        });
                        if !bloqueado_por_escudo {
                            mejor_fin = Some(k);
                        }
                    }
                }
                if let Some(fin) = mejor_fin {
                    reemplazo_fin[i] = Some(fin);
                    for c in &mut consumido[i..=fin] {
                        *c = true;
                    }
                    hubo_reemplazo = true;
                    i = fin + 1;
                } else {
                    i += 1;
                }
            }
        }

        // 2) Escudos de defensores y topónimos judiciales sobre tokens no consumidos
        let mut protegido: Vec<bool> = vec![false; n];
        if (self.tiene_palabras_procesado || !self.subfrases_procesado.esta_vacio())
            && !rangos_escudos.is_empty()
        {
            if hubo_reemplazo {
                escudos.marcar_protegidos(texto, &tokens, &fold_buf, &consumido, &mut protegido);
            } else {
                for &(ds, de) in &rangos_escudos {
                    for p in &mut protegido[ds..=de] {
                        *p = true;
                    }
                }
            }
        }
        if self.tiene_palabras_procesado || !self.subfrases_procesado.esta_vacio() {
            marcar_toponimos_en_juzgados(texto, &tokens, &fold_buf, &consumido, &mut protegido);
        }

        // 3) Sub-frases compuestas con puntuación (`Juan-Carlos`, `O'Connor`) y palabras individuales
        //    fuera de frases consumidas y escudos de defensores
        if !self.subfrases_procesado.esta_vacio() {
            let mut i = 0usize;
            while i < n {
                if consumido[i] || protegido[i] {
                    i += 1;
                    continue;
                }
                let wid0 = word_ids[i];
                if wid0 == SIN_ID {
                    i += 1;
                    continue;
                }
                let Some(mut nodo) = self.subfrases_procesado.siguiente(0, wid0) else {
                    i += 1;
                    continue;
                };
                let mut uso_punt = false;
                let mut mejor_fin: Option<usize> = None;
                for k in (i + 1)..n {
                    if consumido[k] || protegido[k] {
                        break;
                    }
                    let sep = &texto[tokens[k - 1].orig_end..tokens[k].orig_start];
                    let prev_fold = &fold_buf[tokens[k - 1].fold_start..tokens[k - 1].fold_end];
                    match clasificar_sep_nombre_procesado(sep, prev_fold) {
                        SepProcesado::Puntuacion => {
                            uso_punt = true;
                        }
                        SepProcesado::Espacio => {}
                        SepProcesado::ConComa | SepProcesado::Invalido => break,
                    }
                    let wid = word_ids[k];
                    if wid == SIN_ID {
                        break;
                    }
                    let Some(sig) = self.subfrases_procesado.siguiente(nodo, wid) else {
                        break;
                    };
                    nodo = sig;
                    if (self.subfrases_procesado.flags_nodo(nodo) & FLAG_TRIE_FINAL_PUNT) != 0
                        && uso_punt
                    {
                        mejor_fin = Some(k);
                    }
                }
                if let Some(fin) = mejor_fin {
                    reemplazo_fin[i] = Some(fin);
                    for c in &mut consumido[i..=fin] {
                        *c = true;
                    }
                    hubo_reemplazo = true;
                    i = fin + 1;
                } else {
                    i += 1;
                }
            }
        }

        if self.tiene_palabras_procesado {
            let mut i = 0usize;
            while i < n {
                if consumido[i] || protegido[i] {
                    i += 1;
                    continue;
                }
                let wid = word_ids[i];
                if wid != SIN_ID && self.es_palabra_procesado[wid as usize] {
                    let mut fin = i;
                    while fin + 1 < n
                        && !consumido[fin + 1]
                        && !protegido[fin + 1]
                        && reemplazo_fin[fin + 1].is_none()
                    {
                        let wid_sig = word_ids[fin + 1];
                        if wid_sig == SIN_ID || !self.es_palabra_procesado[wid_sig as usize] {
                            break;
                        }
                        let sep = &texto[tokens[fin].orig_end..tokens[fin + 1].orig_start];
                        let prev_fold = &fold_buf[tokens[fin].fold_start..tokens[fin].fold_end];
                        if !matches!(
                            clasificar_sep_nombre_procesado(sep, prev_fold),
                            SepProcesado::Espacio
                        ) {
                            break;
                        }
                        fin += 1;
                    }
                    reemplazo_fin[i] = Some(fin);
                    hubo_reemplazo = true;
                    i = fin + 1;
                } else {
                    i += 1;
                }
            }
        }

        if !hubo_reemplazo {
            return Cow::Borrowed(texto);
        }

        let mut out = String::with_capacity(texto.len());
        let mut ultimo_byte = 0usize;
        let mut i = 0usize;
        while i < n {
            if let Some(fin) = reemplazo_fin[i] {
                out.push_str(&texto[ultimo_byte..tokens[i].orig_start]);
                out.push_str("[PROCESADO]");
                ultimo_byte = tokens[fin].orig_end;
                i = fin + 1;
            } else {
                i += 1;
            }
        }
        out.push_str(&texto[ultimo_byte..]);
        Cow::Owned(out)
    }

    fn aplicar_sin_numeros_largos_con_escudos<'a>(
        &self,
        texto: &'a str,
        escudos: &EscudosDefensores,
    ) -> Cow<'a, str> {
        if texto.is_empty() {
            return Cow::Borrowed(texto);
        }
        let tras_rad = self.radicados.reemplazar(texto);
        if self.frases_procesado.esta_vacio()
            && self.subfrases_procesado.esta_vacio()
            && !self.tiene_palabras_procesado
        {
            return tras_rad;
        }
        match self.aplicar_nombres_y_escudos(&tras_rad, escudos) {
            Cow::Borrowed(_) => tras_rad,
            Cow::Owned(s) => Cow::Owned(s),
        }
    }

    fn aplicar_con_escudos(&self, texto: &str, escudos: &EscudosDefensores) -> String {
        if texto.is_empty() {
            return String::new();
        }
        let tras_nombres = self.aplicar_sin_numeros_largos_con_escudos(texto, escudos);
        enmascarar_numeros_largos(&tras_nombres).into_owned()
    }

    fn aplicar(&self, texto: &str) -> String {
        self.aplicar_con_escudos(texto, &self.escudos_defensores)
    }

    fn aplicar_audiencia(&self, a: &AudienciaInput) -> AudienciaInput {
        AudienciaInput {
            radicado: self.aplicar(&a.radicado),
            tipo: self.aplicar(&a.tipo),
            fecha: a.fecha.clone(),
            hora: a.hora.clone(),
            juzgado: self.aplicar(&a.juzgado),
            realizada: a.realizada.clone(),
            motivo: a.motivo.clone(),
            observaciones: self.aplicar(&a.observaciones),
        }
    }
}

/// Copia anonimizada de una corrección, lista para exportar.
pub(crate) fn anonimizar_correccion_con_global(
    c: &CorreccionIa,
    anon_global: Option<&Anonimizador>,
) -> CorreccionIa {
    let anon_local =
        Anonimizador::desde_radicados(&[&c.prediccion.radicado, &c.final_usuario.radicado]);
    let escudos_vacios = EscudosDefensores::default();
    let escudos = anon_global
        .map(|g| &g.escudos_defensores)
        .unwrap_or(&escudos_vacios);
    let aplicar_ambos = |s: &str| -> String {
        match anon_global {
            Some(g) => {
                let paso1 = anon_local.aplicar_sin_numeros_largos_con_escudos(s, escudos);
                g.aplicar(&paso1)
            }
            None => anon_local.aplicar_con_escudos(s, escudos),
        }
    };
    let aplicar_aud = |a: &AudienciaInput| -> AudienciaInput {
        AudienciaInput {
            radicado: aplicar_ambos(&a.radicado),
            tipo: aplicar_ambos(&a.tipo),
            fecha: a.fecha.clone(),
            hora: a.hora.clone(),
            juzgado: aplicar_ambos(&a.juzgado),
            realizada: a.realizada.clone(),
            motivo: a.motivo.clone(),
            observaciones: aplicar_ambos(&a.observaciones),
        }
    };

    CorreccionIa {
        texto_original: aplicar_ambos(&c.texto_original),
        prediccion: aplicar_aud(&c.prediccion),
        final_usuario: aplicar_aud(&c.final_usuario),
        campos: c
            .campos
            .iter()
            .map(|campo| CampoCorregido {
                campo: campo.campo.clone(),
                valor_ia: aplicar_ambos(&campo.valor_ia),
                valor_usuario: aplicar_ambos(&campo.valor_usuario),
            })
            .collect(),
        regla_generada: aplicar_ambos(&c.regla_generada),
        pregunta_ia: aplicar_ambos(&c.pregunta_ia),
        nota_usuario: aplicar_ambos(&c.nota_usuario),
        ..c.clone()
    }
}

pub fn anonimizar_correccion(c: &CorreccionIa) -> CorreccionIa {
    anonimizar_correccion_con_global(c, None)
}

// ---------------------------------------------------------------------------
// Persistencia y ciclo de vida del análisis
// ---------------------------------------------------------------------------

pub fn ruta_memoria() -> PathBuf {
    ruta_en_carpeta_config(NOMBRE_ARCHIVO_MEMORIA)
}

fn leer_instancia_id_uso_hermano(ruta_memoria: &Path) -> Option<String> {
    let ruta_uso = ruta_memoria.with_file_name(crate::uso_catalogos::NOMBRE_ARCHIVO_USO);
    let raw = fs::read_to_string(ruta_uso).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get("instancia_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Carga la memoria; si es `v1` la migra automáticamente a `v2` sin perder datos,
/// y si el archivo está dañado lo conserva como `.bak` y empieza de cero.
pub fn cargar_memoria_en(ruta: &Path) -> MemoriaAprendizaje {
    let Ok(contenido) = fs::read_to_string(ruta) else {
        return MemoriaAprendizaje::default();
    };
    match serde_json::from_str::<MemoriaAprendizaje>(&contenido) {
        Ok(mut memoria) => {
            if memoria.instancia_id.trim().is_empty() {
                if let Some(id_uso) = leer_instancia_id_uso_hermano(ruta) {
                    memoria.instancia_id = id_uso;
                }
            }
            migrar_memoria_v1_a_v2(&mut memoria);
            memoria
        }
        Err(_) => {
            let _ = fs::rename(ruta, ruta.with_extension("json.bak"));
            MemoriaAprendizaje::default()
        }
    }
}

fn guardar_memoria_en(ruta: &Path, memoria: &MemoriaAprendizaje) -> Result<(), String> {
    if let Some(parent) = ruta.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear la carpeta de aprendizaje de IA: {}", e))?;
    }
    let json = serde_json::to_string_pretty(memoria)
        .map_err(|e| format!("Error serializando el aprendizaje de IA: {}", e))?;
    // Escritura atómica: archivo temporal + renombrado, para no dejar el JSON a medias.
    let temporal = ruta.with_extension("json.tmp");
    fs::write(&temporal, json)
        .map_err(|e| format!("Error guardando el aprendizaje de IA: {}", e))?;
    fs::rename(&temporal, ruta).map_err(|e| format!("Error guardando el aprendizaje de IA: {}", e))
}

fn ultima_aclaracion_pendiente(memoria: &MemoriaAprendizaje) -> Option<AclaracionPendiente> {
    memoria
        .correcciones
        .iter()
        .rev()
        .find(|c| c.estado == "esperando_aclaracion" && !c.pregunta_ia.trim().is_empty())
        .map(|c| AclaracionPendiente {
            correccion_id: c.id.clone(),
            pregunta: c.pregunta_ia.clone(),
            campos: c.campos.clone(),
        })
}

fn resumen_de(memoria: &MemoriaAprendizaje, hubo_correccion: bool) -> ResumenAprendizaje {
    let pendientes_ia = memoria.correcciones.iter().filter(|c| !c.exportada).count();
    let puede_exportar = !memoria.correcciones.is_empty()
        || !memoria.reglas.is_empty()
        || memoria.aciertos > 0
        || !memoria.ejemplos_aciertos.is_empty();
    ResumenAprendizaje {
        aciertos: memoria.aciertos,
        total_correcciones: memoria.correcciones.len(),
        total_reglas: memoria
            .reglas
            .iter()
            .filter(|r| r.estado != "retirada")
            .count(),
        pendientes_exportar: pendientes_ia,
        pendientes_ia,
        pendientes_uso: 0,
        total_audiencias_contabilizadas: 0,
        puede_exportar,
        hubo_correccion,
        aclaracion_pendiente: ultima_aclaracion_pendiente(memoria),
    }
}

pub fn combinar_resumen_con_uso(
    mut base: ResumenAprendizaje,
    uso: &UsoCatalogos,
    settings: &AppSettings,
) -> ResumenAprendizaje {
    let hay_uso = crate::uso_catalogos::hay_datos_uso_exportables(uso, settings);
    base.pendientes_uso = if hay_uso {
        uso.pendientes_compartir as usize
    } else {
        0
    };
    base.total_audiencias_contabilizadas = uso.total_audiencias_contabilizadas;
    base.puede_exportar = base.puede_exportar || hay_uso;
    base.pendientes_exportar = if base.puede_exportar {
        base.pendientes_ia.saturating_add(base.pendientes_uso)
    } else {
        0
    };
    base
}

fn actualizar_estado_page_hinkley(
    est: &mut EstadoPageHinkley,
    campo: &str,
    hubo_fallo: bool,
    ahora: &str,
) -> bool {
    const DELTA_PH: f64 = 0.08;
    const LAMBDA_PH: f64 = 3.2;
    const MIN_OBS_PH: u32 = 8;

    est.campo = campo.to_string();
    est.n_obs = est.n_obs.saturating_add(1);
    let x = if hubo_fallo { 1.0 } else { 0.0 };
    est.media_error += (x - est.media_error) / (est.n_obs as f64);
    est.suma_desviacion += x - est.media_error - DELTA_PH;
    if est.suma_desviacion < est.min_suma_desviacion {
        est.min_suma_desviacion = est.suma_desviacion;
    }

    let estadistico = est.suma_desviacion - est.min_suma_desviacion;
    if est.n_obs >= MIN_OBS_PH && estadistico > LAMBDA_PH {
        est.drift_detectado = true;
        est.ultimo_drift = ahora.to_string();
        est.eventos_desde_drift = 0;
        est.suma_desviacion = 0.0;
        est.min_suma_desviacion = 0.0;
        est.media_error = 0.5 * est.media_error + 0.5 * x;
        true
    } else {
        if est.drift_detectado {
            est.eventos_desde_drift = est.eventos_desde_drift.saturating_add(1);
            if est.eventos_desde_drift >= 10 {
                est.drift_detectado = false;
            }
        }
        false
    }
}

fn actualizar_mc_campo(mc: &mut MetricasCampo, campo: &str, hubo_fallo: bool, ahora: &str) {
    mc.campo = campo.to_string();
    aplicar_decaimiento_beta_en(
        &mut mc.alpha,
        &mut mc.beta,
        &mc.ultima_actualizacion,
        Some(ahora),
    );
    mc.evaluaciones = mc.evaluaciones.saturating_add(1);
    if hubo_fallo {
        mc.correcciones = mc.correcciones.saturating_add(1);
        mc.beta += 1.0;
        mc.precision_ewma = 0.85 * mc.precision_ewma;
    } else {
        mc.aciertos = mc.aciertos.saturating_add(1);
        mc.alpha += 1.0;
        mc.precision_ewma = 0.85 * mc.precision_ewma + 0.15;
    }
    normalizar_ess(&mut mc.alpha, &mut mc.beta);
    mc.ultima_actualizacion = ahora.to_string();
}

fn actualizar_metricas_y_credito(
    memoria: &mut MemoriaAprendizaje,
    texto: &str,
    campos_corregidos: &[CampoCorregido],
    reglas_inyectadas: &[String],
    proveedor: &str,
    modelo: &str,
    ahora: &str,
    def_clave: &str,
) -> BTreeSet<String> {
    let mut campos_err: BTreeSet<&str> = BTreeSet::new();
    for c in campos_corregidos {
        campos_err.insert(c.campo.as_str());
    }

    // 1. Métricas por campo, por (proveedor, modelo), por defensor y detector Page-Hinkley de drift
    let clave_modelo = format!("{}:{}", proveedor.trim(), modelo.trim());
    let mut campos_con_drift: BTreeSet<String> = BTreeSet::new();
    {
        let mod_entry = memoria
            .metricas_modelos
            .entry(clave_modelo.clone())
            .or_insert_with(|| MetricasModelo {
                clave: clave_modelo.clone(),
                proveedor: proveedor.trim().to_string(),
                modelo: modelo.trim().to_string(),
                ..Default::default()
            });
        mod_entry.evaluaciones = mod_entry.evaluaciones.saturating_add(1);
        if campos_err.is_empty() {
            mod_entry.aciertos = mod_entry.aciertos.saturating_add(1);
        } else {
            mod_entry.correcciones = mod_entry.correcciones.saturating_add(1);
        }

        let ph_mod_map = memoria
            .page_hinkley_modelos
            .entry(clave_modelo.clone())
            .or_default();

        for campo in CAMPOS_AUDIENCIA {
            let hubo_fallo = campos_err.contains(campo);

            let mc_global = memoria
                .metricas_campos
                .entry(campo.to_string())
                .or_insert_with(|| MetricasCampo {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            actualizar_mc_campo(mc_global, campo, hubo_fallo, ahora);

            let mc_mod = mod_entry
                .por_campo
                .entry(campo.to_string())
                .or_insert_with(|| MetricasCampo {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            actualizar_mc_campo(mc_mod, campo, hubo_fallo, ahora);

            let ph_global = memoria
                .page_hinkley_campos
                .entry(campo.to_string())
                .or_insert_with(|| EstadoPageHinkley {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            if actualizar_estado_page_hinkley(ph_global, campo, hubo_fallo, ahora) {
                campos_con_drift.insert(campo.to_string());
            }

            let ph_mod = ph_mod_map
                .entry(campo.to_string())
                .or_insert_with(|| EstadoPageHinkley {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            if actualizar_estado_page_hinkley(ph_mod, campo, hubo_fallo, ahora) {
                campos_con_drift.insert(campo.to_string());
            }
        }
    }

    if !def_clave.is_empty() {
        let nom_mostrar = memoria
            .nombres_defensores
            .get(def_clave)
            .cloned()
            .unwrap_or_else(|| title_case_basico(def_clave));
        let md = memoria
            .metricas_por_defensor
            .entry(def_clave.to_string())
            .or_insert_with(|| MetricasDefensor {
                nombre_mostrar: nom_mostrar.clone(),
                ..Default::default()
            });
        if md.nombre_mostrar.trim().is_empty()
            || es_nombre_defensor_mas_informativo(&nom_mostrar, &md.nombre_mostrar)
        {
            md.nombre_mostrar = nom_mostrar;
        }
        if campos_err.is_empty() {
            md.aciertos = md.aciertos.saturating_add(1);
        } else {
            md.correcciones = md.correcciones.saturating_add(1);
        }
        for campo in CAMPOS_AUDIENCIA {
            let hubo_fallo = campos_err.contains(campo);
            let mc_def = md
                .por_campo
                .entry(campo.to_string())
                .or_insert_with(|| MetricasCampo {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            actualizar_mc_campo(mc_def, campo, hubo_fallo, ahora);
        }
    }

    if !campos_con_drift.is_empty() {
        for r in &mut memoria.reglas {
            if r.origen != "base_invariante"
                && (campos_con_drift.contains(&r.campo_objetivo) || r.campo_objetivo == "general")
            {
                r.alpha = PRIOR_BETA_ALPHA + (r.alpha - PRIOR_BETA_ALPHA).max(0.0) * 0.70;
                r.beta = PRIOR_BETA_BETA + (r.beta - PRIOR_BETA_BETA).max(0.0) * 0.70;
            }
        }
    }

    // 2. Atribución causal condicional sobre reglas inyectadas
    let query_tokens = tokenizar_judicial(texto);
    let query_ngrams: BTreeSet<String> = ngramas_caracteres(texto, 3).into_iter().collect();
    let inyectadas_set: BTreeSet<&str> = reglas_inyectadas.iter().map(String::as_str).collect();

    for regla in &mut memoria.reglas {
        if inyectadas_set.contains(regla.id.as_str()) {
            let fallo_objetivo = if regla.campo_objetivo == "general" {
                !campos_err.is_empty()
            } else if regla.campo_objetivo == "motivo" {
                campos_err.contains("motivo") || campos_err.contains("realizada")
            } else {
                campos_err.contains(regla.campo_objetivo.as_str())
            };

            aplicar_decaimiento_beta(&mut regla.alpha, &mut regla.beta, &regla.ultima_actualizacion);
            regla.impresiones = regla.impresiones.saturating_add(1);
            if fallo_objetivo {
                regla.fallos_directos = regla.fallos_directos.saturating_add(1);
                regla.beta += 1.0;
            } else {
                regla.aciertos_directos = regla.aciertos_directos.saturating_add(1);
                regla.alpha += 1.0;
            }
            normalizar_ess(&mut regla.alpha, &mut regla.beta);
            regla.ultima_actualizacion = ahora.to_string();

            if regla.origen != "base_invariante" {
                let lb = limite_inferior_bayesiano(regla.alpha, regla.beta);
                if regla.estado == "prueba" && regla.impresiones >= 2 && lb >= 0.42 {
                    regla.estado = "activa".to_string();
                } else if matches!(regla.estado.as_str(), "activa" | "prueba")
                    && regla.impresiones >= 5
                {
                    let err_regla = regla.fallos_directos as f64 / regla.impresiones as f64;
                    let err_base = memoria
                        .metricas_campos
                        .get(&regla.campo_objetivo)
                        .and_then(|m| {
                            let evals_sin_regla = m.evaluaciones.saturating_sub(regla.impresiones);
                            let corrs_sin_regla =
                                m.correcciones.saturating_sub(regla.fallos_directos);
                            if evals_sin_regla >= 3 {
                                Some(corrs_sin_regla as f64 / evals_sin_regla as f64)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0.25);
                    let delta_err = err_regla - err_base;
                    if (delta_err > 0.05 && lb < 0.35) || lb < 0.25 {
                        regla.estado = "cuarentena".to_string();
                        regla.aciertos_sombra = 0;
                        regla.confirmaciones_sombra = 0;
                    }
                }
            }
            continue;
        }

        // 3. Evaluación en sombra (shadow evaluation) de reglas en estado "candidata" o "cuarentena"
        //    aislada por alcance de defensor para que un defensor no retire ni promueva reglas privadas de otro.
        if regla.estado == "candidata" || regla.estado == "cuarentena" {
            let aplica_sombra_def = es_alcance_general(&regla.defensor_alcance)
                || coincide_alcance_defensor(&regla.defensor_alcance, def_clave);
            if !aplica_sombra_def {
                continue;
            }
            let trig = coincidencia_triggers(&query_tokens, &query_ngrams, regla);
            let sim_tokens = {
                let rt: BTreeSet<String> = tokenizar_judicial(&regla.regla).into_iter().collect();
                let qt: BTreeSet<String> = query_tokens.iter().cloned().collect();
                if rt.is_empty() || qt.is_empty() {
                    0.0
                } else {
                    rt.intersection(&qt).count() as f64 / rt.len().min(5) as f64
                }
            };
            if trig >= 0.25 || sim_tokens >= 0.30 {
                let fallo_objetivo = if regla.campo_objetivo == "general" {
                    !campos_err.is_empty()
                } else if regla.campo_objetivo == "motivo" {
                    campos_err.contains("motivo") || campos_err.contains("realizada")
                } else {
                    campos_err.contains(regla.campo_objetivo.as_str())
                };
                if regla.estado == "candidata" {
                    if fallo_objetivo {
                        regla.confirmaciones_sombra = regla.confirmaciones_sombra.saturating_add(1);
                        regla.refuerzos = regla.refuerzos.saturating_add(1);
                        regla.alpha += 1.0;
                        regla.ultima_actualizacion = ahora.to_string();
                        regla.actualizada = ahora.to_string();
                        normalizar_ess(&mut regla.alpha, &mut regla.beta);
                        if regla.confirmaciones_sombra >= 2 {
                            regla.estado = "activa".to_string();
                        } else {
                            regla.estado = "prueba".to_string();
                        }
                    } else {
                        regla.aciertos_sombra = regla.aciertos_sombra.saturating_add(1);
                        regla.beta += 0.5;
                        regla.ultima_actualizacion = ahora.to_string();
                        normalizar_ess(&mut regla.alpha, &mut regla.beta);
                        if regla.aciertos_sombra >= 3 && regla.confirmaciones_sombra == 0 {
                            regla.estado = "retirada".to_string();
                        }
                    }
                } else {
                    // Histéresis de salida para "cuarentena":
                    // si el LLM falla repetidamente en este patrón sin la regla (>= 3), rehabilita a "prueba";
                    // si el LLM acierta repetidamente sin ella (>= 3 sin fallos), la retira definitivamente.
                    if fallo_objetivo {
                        regla.confirmaciones_sombra = regla.confirmaciones_sombra.saturating_add(1);
                        regla.ultima_actualizacion = ahora.to_string();
                        if regla.confirmaciones_sombra >= 3 {
                            regla.estado = "prueba".to_string();
                            regla.alpha = PRIOR_BETA_ALPHA + 1.0;
                            regla.beta = PRIOR_BETA_BETA;
                            regla.impresiones = 0;
                            regla.aciertos_directos = 0;
                            regla.fallos_directos = 0;
                            regla.confirmaciones_sombra = 0;
                            regla.aciertos_sombra = 0;
                        }
                    } else {
                        regla.aciertos_sombra = regla.aciertos_sombra.saturating_add(1);
                        regla.ultima_actualizacion = ahora.to_string();
                        if regla.aciertos_sombra >= 3 && regla.confirmaciones_sombra == 0 {
                            regla.estado = "retirada".to_string();
                        }
                    }
                }
            }
        }
    }

    campos_con_drift
}

fn prioridad_campo_cluster(campo: &str) -> u8 {
    match campo {
        "realizada" => 0,
        "motivo" => 1,
        "tipo" => 2,
        "juzgado" => 3,
        "hora" => 4,
        "fecha" => 5,
        "radicado" => 6,
        "observaciones" => 7,
        _ => 8,
    }
}

fn calcular_firma_error(corr: &CorreccionIa) -> (String, String) {
    let Some(primero) = corr
        .campos
        .iter()
        .min_by_key(|c| prioridad_campo_cluster(&c.campo))
    else {
        return ("general".to_string(), "general:general".to_string());
    };
    let campo = primero.campo.clone();
    let firma = match campo.as_str() {
        "realizada" | "motivo" | "tipo" => {
            let v_ia = normalizar_texto_sin_tildes(&primero.valor_ia);
            let v_usr = normalizar_texto_sin_tildes(&primero.valor_usuario);
            format!("{}:{}->{}", campo, v_ia, v_usr)
        }
        "juzgado" => {
            let usr_up = primero.valor_usuario.to_uppercase();
            if usr_up.contains("CTO") {
                "juzgado:sigla_cto".to_string()
            } else if usr_up.contains("MPAL") {
                "juzgado:sigla_mpal".to_string()
            } else if usr_up.contains("PCTO") {
                "juzgado:sigla_pcto".to_string()
            } else {
                "juzgado:normalizacion".to_string()
            }
        }
        "hora" => {
            if primero.valor_ia.trim().is_empty() && !primero.valor_usuario.trim().is_empty() {
                "hora:faltante".to_string()
            } else {
                "hora:ajuste_ampm_o_min".to_string()
            }
        }
        "fecha" => "fecha:normalizacion".to_string(),
        "radicado" => "radicado:extraccion".to_string(),
        "observaciones" => "observaciones:estilo".to_string(),
        _ => format!("{}:otro", campo),
    };
    (campo, firma)
}

fn similitud_bm25_normalizada(a: &str, b: &str) -> f64 {
    let ta = tokenizar_judicial(a);
    let tb = tokenizar_judicial(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let self_a = calcular_bm25(&ta, std::slice::from_ref(&ta))[0].max(1e-9);
    let self_b = calcular_bm25(&tb, std::slice::from_ref(&tb))[0].max(1e-9);
    let s_ab = calcular_bm25(&ta, std::slice::from_ref(&tb))[0] / self_a;
    let s_ba = calcular_bm25(&tb, std::slice::from_ref(&ta))[0] / self_b;
    0.5 * (s_ab.clamp(0.0, 1.0) + s_ba.clamp(0.0, 1.0))
}

fn es_firma_especifica_compartible(firma: &str) -> bool {
    !matches!(
        firma,
        "juzgado:normalizacion"
            | "observaciones:estilo"
            | "hora:ajuste_ampm_o_min"
            | "fecha:normalizacion"
            | "radicado:extraccion"
            | "general:general"
    )
}

fn encolar_correccion_en_cluster(
    memoria: &mut MemoriaAprendizaje,
    corr: &CorreccionIa,
    ahora: &str,
) {
    let (campo_principal, firma) = calcular_firma_error(corr);
    let clave_modelo = format!("{}:{}", corr.proveedor.trim(), corr.modelo.trim());
    let def_clave = corr.defensor_clave.trim();
    let en_drift = memoria
        .page_hinkley_campos
        .get(&campo_principal)
        .is_some_and(|p| p.drift_detectado)
        || memoria
            .page_hinkley_modelos
            .get(&clave_modelo)
            .and_then(|m| m.get(&campo_principal))
            .is_some_and(|p| p.drift_detectado);

    let compartible = es_firma_especifica_compartible(&firma);

    // Si ya existe un clúster inducido reciente para este patrón con regla vigente aplicable a este defensor
    // y no hay drift activo, evita re-llamar al LLM por el mismo patrón repetitivo.
    // Para firmas genéricas (juzgado:normalizacion, observaciones:estilo, etc.) exige similitud textual BM25
    // para no bloquear patrones distintos del mismo campo.
    if !en_drift {
        let ya_cubierto = memoria.cola_induccion.iter().any(|c| {
            let texto_similar = c.correccion_ids.iter().rev().take(3).any(|cid| {
                memoria
                    .correcciones
                    .iter()
                    .find(|x| x.id == *cid)
                    .is_some_and(|x| {
                        similitud_bm25_normalizada(&x.texto_original, &corr.texto_original) >= 0.55
                    })
            });
            let misma_firma_o_texto = if compartible {
                c.firma_error == firma || texto_similar
            } else {
                texto_similar
            };
            c.estado == "inducido"
                && c.campo_principal == campo_principal
                && misma_firma_o_texto
                && !c.regla_inducida_id.is_empty()
                && memoria.reglas.iter().any(|r| {
                    let aplica_def = r.defensor_alcance.trim().is_empty()
                        || r.defensor_alcance == "*"
                        || (!def_clave.is_empty() && r.defensor_alcance == def_clave);
                    r.id == c.regla_inducida_id
                        && matches!(r.estado.as_str(), "prueba" | "activa")
                        && aplica_def
                })
        });
        if ya_cubierto {
            return;
        }
    }

    let mut indice_objetivo: Option<usize> = None;

    for (idx, cluster) in memoria.cola_induccion.iter().enumerate() {
        if cluster.estado == "inducido" || cluster.campo_principal != campo_principal {
            continue;
        }
        let mismo_defensor = cluster.defensor_clave == def_clave;
        let sim_cluster = |umbral: f64| -> bool {
            cluster.correccion_ids.iter().rev().take(2).any(|cid| {
                memoria
                    .correcciones
                    .iter()
                    .find(|c| &c.id == cid)
                    .is_some_and(|anchor| {
                        similitud_bm25_normalizada(&corr.texto_original, &anchor.texto_original)
                            >= umbral
                    })
            })
        };
        if cluster.firma_error == firma {
            if mismo_defensor {
                indice_objetivo = Some(idx);
                break;
            }
            // Entre defensores distintos solo fusiona firmas específicas; para transiciones amplias
            // de `realizada` exige además similitud textual (>= 0.35) para no mezclar criterios distintos.
            let puede_compartir = compartible
                && (campo_principal != "realizada" || sim_cluster(0.35));
            if puede_compartir {
                indice_objetivo = Some(idx);
                break;
            }
        }
        if !mismo_defensor {
            continue;
        }
        if sim_cluster(0.55) {
            indice_objetivo = Some(idx);
            break;
        }
    }

    if let Some(idx) = indice_objetivo {
        let resuelto_por_alias_local = !en_drift
            && corr.campos.len() == 1
            && matches!(corr.campos[0].campo.as_str(), "juzgado" | "tipo")
            && !corr.campos[0].valor_ia.trim().is_empty()
            && !corr.campos[0].valor_usuario.trim().is_empty()
            && crate::estadistica_ia::resolver_alias_exacto_validado_para_defensor(
                memoria,
                &corr.campos[0].campo,
                &corr.campos[0].valor_ia,
                0.30,
                if def_clave.is_empty() {
                    None
                } else {
                    Some(def_clave)
                },
            )
            .is_some_and(|res| {
                normalizar_texto_sin_tildes(&res)
                    == normalizar_texto_sin_tildes(&corr.campos[0].valor_usuario)
            });
        // Si un alias exacto de juzgado/tipo acaba de validarse localmente en la 2.ª corrección,
        // solo escala a inducción LLM si el fallo persiste una 3.ª vez pese al alias local.
        let umbral_listo = if resuelto_por_alias_local {
            MIN_TAMANO_CLUSTER_INDUCCION + 1
        } else {
            MIN_TAMANO_CLUSTER_INDUCCION
        };
        let cluster = &mut memoria.cola_induccion[idx];
        if !cluster.correccion_ids.contains(&corr.id) {
            cluster.correccion_ids.push(corr.id.clone());
        }
        if !def_clave.is_empty() && !cluster.defensores_presentes.iter().any(|d| d == def_clave) {
            cluster.defensores_presentes.push(def_clave.to_string());
        }
        if cluster.defensores_presentes.len() >= 2 {
            cluster.defensor_clave.clear();
        }
        cluster.ultima_vez = ahora.to_string();
        if cluster.correccion_ids.len() >= umbral_listo {
            cluster.estado = "listo".to_string();
        }
    } else {
        let mut defensores_presentes = Vec::new();
        if !def_clave.is_empty() {
            defensores_presentes.push(def_clave.to_string());
        }
        memoria.cola_induccion.push(ClusterCorreccion {
            id: generar_id_unico("clu"),
            campo_principal,
            firma_error: firma,
            correccion_ids: vec![corr.id.clone()],
            primera_vez: ahora.to_string(),
            ultima_vez: ahora.to_string(),
            estado: "acumulando".to_string(),
            regla_inducida_id: String::new(),
            defensor_clave: def_clave.to_string(),
            defensores_presentes,
        });
    }

    if memoria.cola_induccion.len() > MAX_CLUSTERS_COLA {
        if let Some(pos_ind) = memoria
            .cola_induccion
            .iter()
            .position(|c| c.estado == "inducido")
        {
            memoria.cola_induccion.remove(pos_ind);
        } else {
            memoria.cola_induccion.remove(0);
        }
    }
}

fn calcular_madurez_sistema(memoria: &MemoriaAprendizaje) -> f64 {
    let total_obs = (memoria.llamadas_extraccion_total as usize)
        .max(memoria.aciertos as usize + memoria.correcciones.len()) as f64;
    (total_obs / 80.0).clamp(0.0, 1.0)
}

fn clave_firma_aclaracion(corr: &CorreccionIa) -> (String, String) {
    let (campo, firma) = calcular_firma_error(corr);
    let combinado = format!("{}|{}", campo, firma);
    (campo, combinado)
}

fn registrar_firma_explicada(memoria: &mut MemoriaAprendizaje, clave_firma: String) {
    let turno = memoria.llamadas_extraccion_total as u64;
    memoria
        .estado_aclaracion
        .firmas_explicadas_recientes
        .retain(|(f, t)| f != &clave_firma && turno.saturating_sub(*t) <= 30);
    memoria
        .estado_aclaracion
        .firmas_explicadas_recientes
        .push((clave_firma, turno));
    if memoria.estado_aclaracion.firmas_explicadas_recientes.len() > 40 {
        let ex = memoria.estado_aclaracion.firmas_explicadas_recientes.len() - 40;
        memoria
            .estado_aclaracion
            .firmas_explicadas_recientes
            .drain(0..ex);
    }
}

fn construir_pregunta_aclaracion_local(corr: &CorreccionIa, campo_principal: &str) -> String {
    let valor_usr = corr
        .campos
        .iter()
        .find(|c| c.campo == campo_principal)
        .map(|c| c.valor_usuario.trim())
        .unwrap_or("");
    let valor_ia = corr
        .campos
        .iter()
        .find(|c| c.campo == campo_principal)
        .map(|c| c.valor_ia.trim())
        .unwrap_or("");

    let tiene_realizada = corr.campos.iter().any(|c| c.campo == "realizada");
    let tiene_motivo = corr.campos.iter().any(|c| c.campo == "motivo");

    if tiene_realizada && tiene_motivo {
        let mot = corr
            .campos
            .iter()
            .find(|c| c.campo == "motivo")
            .map(|c| c.valor_usuario.trim())
            .unwrap_or("");
        if mot.is_empty() {
            format!(
                "¿Por qué marcaste la audiencia como '{}'?",
                corr.final_usuario.realizada.trim()
            )
        } else {
            format!(
                "¿Por qué se marcó '{}' en realización con motivo '{}'?",
                corr.final_usuario.realizada.trim(),
                mot
            )
        }
    } else if campo_principal == "juzgado" && !valor_usr.is_empty() {
        if valor_ia.is_empty() {
            format!("¿Por qué correspondió al juzgado '{}'?", valor_usr)
        } else {
            format!(
                "¿Por qué se ajustó el juzgado '{}' a '{}'?",
                recortar_sin_elipsis(valor_ia, 40),
                recortar_sin_elipsis(valor_usr, 45)
            )
        }
    } else if campo_principal == "motivo" && !valor_usr.is_empty() {
        format!("¿Por qué correspondió el motivo '{}'?", valor_usr)
    } else if campo_principal == "realizada" {
        format!(
            "¿Por qué se marcó '{}' en realización?",
            corr.final_usuario.realizada.trim()
        )
    } else if campo_principal == "tipo" && !valor_usr.is_empty() {
        format!("¿Por qué correspondió al tipo '{}'?", valor_usr)
    } else {
        format!("¿Por qué corregiste el campo '{}'?", campo_principal)
    }
}

/// Evalúa localmente (en <1 ms y a 0 tokens LLM) si conviene mostrar el cuadro ocasional
/// "¿Por qué la corrección?" para esta corrección.
pub(crate) fn decidir_aclaracion_local(
    memoria: &MemoriaAprendizaje,
    corr: &CorreccionIa,
    prediccion_local: Option<&PrediccionLocalResumen>,
    def_clave: &str,
) -> Option<String> {
    if memoria.enfriamiento_aclaracion > 0
        || !corr.nota_usuario.trim().is_empty()
        || corr.campos.is_empty()
    {
        return None;
    }
    if memoria
        .correcciones
        .iter()
        .any(|c| c.id != corr.id && c.estado == "esperando_aclaracion")
    {
        return None;
    }

    // Excluir fecha, hora, radicado y ajustes menores de observaciones
    let tiene_campo_semantico = corr
        .campos
        .iter()
        .any(|c| matches!(c.campo.as_str(), "realizada" | "motivo" | "tipo" | "juzgado"));
    if !tiene_campo_semantico {
        let solo_obs = corr.campos.len() == 1 && corr.campos[0].campo == "observaciones";
        if !solo_obs {
            return None;
        }
        let obs = &corr.campos[0];
        let sim_obs = similitud_textos(&obs.valor_ia, &obs.valor_usuario);
        let retiro_sufijo = (obs.valor_ia.contains("Causas secundarias:")
            && !obs.valor_usuario.contains("Causas secundarias:"))
            || (obs.valor_ia.contains("Reprogramada para:")
                && !obs.valor_usuario.contains("Reprogramada para:"));
        if obs.valor_usuario.trim().is_empty() || sim_obs >= 0.55 || retiro_sufijo {
            return None;
        }
    }

    let (campo_principal, clave_firma) = clave_firma_aclaracion(corr);
    let turno_actual = memoria.llamadas_extraccion_total as u64;
    if memoria
        .estado_aclaracion
        .firmas_explicadas_recientes
        .iter()
        .any(|(f, t)| f == &clave_firma && turno_actual.saturating_sub(*t) <= 30)
    {
        return None;
    }

    // Precisión EWMA por defensor si existe; respaldo en métrica global del campo
    let precision_ewma = if !def_clave.is_empty() {
        memoria
            .metricas_por_defensor
            .get(def_clave)
            .and_then(|md| md.por_campo.get(&campo_principal))
            .filter(|m| m.evaluaciones > 0)
            .map(|m| m.precision_ewma)
    } else {
        None
    }
    .or_else(|| {
        memoria
            .metricas_campos
            .get(&campo_principal)
            .filter(|m| m.evaluaciones > 0)
            .map(|m| m.precision_ewma)
    })
    .unwrap_or(0.5);

    let incert = (1.0 - precision_ewma).clamp(0.10, 0.90);
    let impacto_campo: f64 = if corr.campos.len() >= 2 {
        0.35
    } else if matches!(
        campo_principal.as_str(),
        "realizada" | "motivo" | "juzgado" | "tipo"
    ) {
        0.25
    } else {
        0.12
    };

    let campo_obj = corr
        .campos
        .iter()
        .find(|c| c.campo == campo_principal)
        .unwrap_or(&corr.campos[0]);
    let usr_up = campo_obj.valor_usuario.to_uppercase();
    let tiene_sigla_juz = campo_principal == "juzgado"
        && (usr_up.contains("CTO")
            || usr_up.contains("MPAL")
            || usr_up.contains("PCTO")
            || es_solo_sigla_corta_juzgado(&campo_obj.valor_ia).is_some()
            || crate::patrones_abreviacion::es_abreviacion_corta_juzgado(&campo_obj.valor_ia)
            || !extraer_siglas_cortas_juzgado(&corr.texto_original).is_empty());

    if tokenizar_judicial(&corr.texto_original).len() < 3 && !tiene_sigla_juz {
        return None;
    }

    let bonus_ambiguedad = if tiene_sigla_juz {
        0.25
    } else if campo_principal == "realizada"
        || (campo_principal == "motivo"
            && !campo_obj.valor_ia.trim().is_empty()
            && !campo_obj.valor_usuario.trim().is_empty())
    {
        0.25
    } else if !campo_obj.valor_usuario.trim().is_empty() {
        0.15
    } else {
        0.0
    };

    let n_prev_firma = memoria
        .correcciones
        .iter()
        .filter(|c| c.id != corr.id && c.estado != "descartada" && clave_firma_aclaracion(c).1 == clave_firma)
        .count();
    let bonus_novedad = match n_prev_firma {
        0 => 0.20,
        1 => 0.10,
        _ => 0.0,
    };

    let bonus_desacuerdo_local = if let Some(pl) = prediccion_local {
        let desac_real = campo_principal == "realizada"
            && pl.realizada_confianza >= 0.70
            && pl
                .realizada
                .as_deref()
                .is_some_and(|r| !r.eq_ignore_ascii_case(&corr.final_usuario.realizada));
        let desac_mot = campo_principal == "motivo"
            && pl.motivo_confianza >= 0.65
            && pl.motivo.as_deref().is_some_and(|m| {
                normalizar_texto_sin_tildes(m)
                    != normalizar_texto_sin_tildes(&corr.final_usuario.motivo)
            });
        if pl.drift_realizada || pl.drift_motivo {
            0.15
        } else if desac_real || desac_mot {
            0.10
        } else {
            0.0
        }
    } else {
        0.0
    };

    let voi_bruto: f64 =
        (impacto_campo + bonus_ambiguedad + bonus_novedad + bonus_desacuerdo_local).clamp(0.0_f64, 1.2_f64);
    let voi = voi_bruto * (0.50 + 0.65 * incert);

    let madurez = calcular_madurez_sistema(memoria);
    let denom_resp = (memoria.estado_aclaracion.alpha_resp + memoria.estado_aclaracion.beta_omit)
        .max(1e-6);
    let r_resp = (memoria.estado_aclaracion.alpha_resp / denom_resp).clamp(0.0, 1.0);
    let umbral = 0.45 + 0.35 * madurez + 0.15 * (1.0 - r_resp);

    if voi >= umbral {
        Some(construir_pregunta_aclaracion_local(corr, &campo_principal))
    } else {
        None
    }
}

fn obtener_precision_ewma_antes(memoria: &MemoriaAprendizaje, def_clave: &str, campo: &str) -> f64 {
    if !def_clave.is_empty() {
        if let Some(p) = memoria
            .metricas_por_defensor
            .get(def_clave)
            .and_then(|md| md.por_campo.get(campo))
            .filter(|m| m.evaluaciones > 0)
            .map(|m| m.precision_ewma)
        {
            return p;
        }
    }
    memoria
        .metricas_campos
        .get(campo)
        .filter(|m| m.evaluaciones > 0)
        .map(|m| m.precision_ewma)
        .unwrap_or(0.5)
}

fn retiro_o_eliminacion_regla_asociada(memoria: &mut MemoriaAprendizaje, regla_id: &str, corr_id: &str) {
    if regla_id.trim().is_empty() {
        return;
    }
    let usada_por_otra = memoria.correcciones.iter().any(|c| {
        c.id != corr_id && c.estado != "descartada" && c.regla_id_asociada == regla_id
    });
    if usada_por_otra {
        return;
    }
    if let Some(pos) = memoria.reglas.iter().position(|r| r.id == regla_id) {
        let es_unica_de_esta_corr =
            memoria.reglas[pos].refuerzos <= 1 && memoria.reglas[pos].origen_correccion_id == corr_id;
        if es_unica_de_esta_corr && !memoria.reglas[pos].id.starts_with("base-") {
            memoria.reglas.remove(pos);
        } else if !memoria.reglas[pos].id.starts_with("base-") {
            memoria.reglas[pos].estado = "retirada".to_string();
        }
    }
}

fn regenerar_triggers_correccion(
    texto_original: &str,
    regla: &str,
    nota_usuario: &str,
) -> Vec<String> {
    let mut triggers = extraer_triggers_texto(regla, 8);
    for t in extraer_triggers_texto(texto_original, 6)
        .into_iter()
        .chain(extraer_triggers_texto(nota_usuario, 4))
    {
        if !triggers.contains(&t) && triggers.len() < 12 {
            triggers.push(t);
        }
    }
    triggers
}

/// Evalúa si el sistema debe disparar una consolidación periódica con el LLM en el ámbito
/// del defensor activo o en el ámbito transversal `"*"`.
pub fn evaluar_candidato_consolidacion(
    memoria: &mut MemoriaAprendizaje,
    defensor: Option<&str>,
    rng_override: Option<f64>,
) -> Option<TelemetriaDisparoConsolidacion> {
    if memoria.llamadas_extraccion_total < 8 {
        return None;
    }
    let tasa =
        memoria.llamadas_consolidacion_total as f64 / memoria.llamadas_extraccion_total.max(1) as f64;
    if tasa > 0.06 {
        return None;
    }

    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let mut ambitos: Vec<String> = Vec::new();
    if !def_clave.is_empty() {
        ambitos.push(def_clave);
    }
    ambitos.push("*".to_string());

    let madurez = calcular_madurez_sistema(memoria);
    let tiene_drift = memoria
        .page_hinkley_campos
        .values()
        .any(|p| p.drift_detectado);
    let drift_bonus = if tiene_drift { 1.2 } else { 0.0 };

    let mut mejor_candidato: Option<(f64, TelemetriaDisparoConsolidacion)> = None;

    for ambito in ambitos {
        let Some(est) = memoria.estados_consolidacion.get(&ambito) else {
            continue;
        };
        if est.extracciones_desde_ultima < 6 {
            continue;
        }
        let tiene_nota = est.notas_sin_consolidar > 0;
        let cumple_min_corr = est.correcciones_sin_consolidar >= 3
            || (est.correcciones_sin_consolidar >= 2 && (tiene_drift || tiene_nota));
        if !cumple_min_corr {
            continue;
        }

        let tau = (4.2 + 1.2 * madurez - drift_bonus).max(1.5);
        let prob = 1.0 / (1.0 + (-0.85 * (est.presion - tau)).exp());
        let margen = est.presion - tau;

        let mut razones = Vec::new();
        razones.push(format!(
            "presion={:.2}, umbral={:.2}, p={:.2}",
            est.presion, tau, prob
        ));
        razones.push(format!(
            "{}_correcciones_sin_consolidar",
            est.correcciones_sin_consolidar
        ));
        if tiene_nota {
            razones.push(format!("{}_notas_usuario", est.notas_sin_consolidar));
        }
        if tiene_drift {
            razones.push("drift_activo".to_string());
        }
        razones.push(if ambito == "*" {
            "ambito_general_transversal".to_string()
        } else {
            "ambito_perfil_defensor".to_string()
        });

        let tel = TelemetriaDisparoConsolidacion {
            ambito,
            presion: est.presion,
            umbral_adaptativo: tau,
            probabilidad_disparo: prob,
            muestra_rng: 0.0,
            manual: false,
            razones,
        };
        if mejor_candidato
            .as_ref()
            .is_none_or(|(best_m, _)| margen > *best_m)
        {
            mejor_candidato = Some((margen, tel));
        }
    }

    let (_, mut tel) = mejor_candidato?;
    let u = rng_override.unwrap_or_else(|| next_u01(&mut memoria.semilla_rng));
    tel.muestra_rng = u;
    if u < tel.probabilidad_disparo {
        Some(tel)
    } else {
        None
    }
}

pub fn preparar_lote_desde_memoria(
    memoria: &MemoriaAprendizaje,
    ambito: &str,
    telemetria: TelemetriaDisparoConsolidacion,
    forzar_manual: bool,
    radicados_extra: &[String],
) -> Option<LoteConsolidacion> {
    let amb_norm = alcance_canonico(ambito);
    let nombre_ambito = if amb_norm == "*" {
        "General (transversal)".to_string()
    } else {
        memoria
            .nombres_defensores
            .get(&amb_norm)
            .cloned()
            .unwrap_or_else(|| title_case_basico(&amb_norm))
    };

    let (todos_radicados_str, escudos) =
        recoger_radicados_y_escudos(memoria, radicados_extra, &[]);
    let todos_radicados: Vec<&str> = todos_radicados_str.iter().map(String::as_str).collect();
    let anon = Anonimizador::desde_radicados(&todos_radicados).con_escudos_defensores(&escudos);

    let (resumen_actual, version_resumen) = memoria
        .perfiles_sinteticos
        .get(&amb_norm)
        .map(|p| (anon.aplicar(&p.resumen), p.version))
        .unwrap_or_else(|| (String::new(), 0));

    let max_chars_resumen = if amb_norm == "*" {
        MAX_CHARS_RESUMEN_GENERAL
    } else {
        MAX_CHARS_RESUMEN_DEFENSOR
    };
    let resumen_actual = recortar_sin_elipsis(&resumen_actual, max_chars_resumen);

    let mut chars_usados = resumen_actual.chars().count();

    let mut reglas_ambito: Vec<ReglaLoteConsolidacion> = Vec::new();
    for r in memoria
        .reglas
        .iter()
        .filter(|r| r.estado != "retirada")
        .filter(|r| {
            if amb_norm == "*" {
                es_alcance_general(&r.defensor_alcance) || r.defensores_confirmantes.len() >= 2
            } else {
                coincide_alcance_defensor(&r.defensor_alcance, &amb_norm)
                    || es_alcance_general(&r.defensor_alcance)
            }
        })
    {
        let regla_anon = anon.aplicar(&r.regla);
        let triggers_anon: Vec<String> = r
            .triggers
            .iter()
            .map(|t| anon.aplicar(t))
            .filter(|t| {
                let l = t.trim();
                !l.is_empty() && l != "[PROCESADO]" && l != "[RADICADO]" && l != "[TELEFONO]" && l != "[NUMERO]"
            })
            .collect();
        let costo = regla_anon.chars().count() + triggers_anon.iter().map(|t| t.len() + 2).sum::<usize>() + 48;
        if chars_usados + costo > MAX_CHARS_LOTE_CONSOLIDACION / 2 && !reglas_ambito.is_empty() {
            continue;
        }
        chars_usados += costo;
        reglas_ambito.push(ReglaLoteConsolidacion {
            id: r.id.clone(),
            estado: r.estado.clone(),
            origen: r.origen.clone(),
            campo_objetivo: r.campo_objetivo.clone(),
            tipo_error: r.tipo_error.clone(),
            defensor_alcance: alcance_canonico(&r.defensor_alcance),
            regla: regla_anon,
            triggers: triggers_anon,
            confianza_q10: limite_inferior_bayesiano(r.alpha, r.beta),
            impresiones: r.impresiones,
            aciertos_directos: r.aciertos_directos,
            fallos_directos: r.fallos_directos,
        });
    }

    let ids_ya_cons: BTreeSet<&str> = memoria
        .estados_consolidacion
        .get(&amb_norm)
        .map(|e| e.ids_consolidadas_recientes.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let mut correcciones_recientes: Vec<CorreccionLoteConsolidacion> = Vec::new();
    let mut watermark_ids: Vec<String> = Vec::new();

    for c in memoria.correcciones.iter().rev() {
        if c.estado == "descartada" {
            continue;
        }
        if amb_norm != "*" && !coincide_alcance_defensor(&c.defensor_clave, &amb_norm) {
            continue;
        }
        if !forzar_manual && ids_ya_cons.contains(c.id.as_str()) {
            continue;
        }
        let ca = anonimizar_correccion_con_global(c, Some(&anon));
        let (campo_p, _) = calcular_firma_error(&ca);
        let texto_rec = recortar_sin_elipsis(&ca.texto_original, 220);
        let nota_rec = recortar_sin_elipsis(&ca.nota_usuario, 160);
        let costo = texto_rec.chars().count()
            + nota_rec.chars().count()
            + ca.campos
                .iter()
                .map(|fc| fc.campo.len() + fc.valor_ia.chars().count() + fc.valor_usuario.chars().count() + 10)
                .sum::<usize>()
            + 40;
        if chars_usados + costo > MAX_CHARS_LOTE_CONSOLIDACION && !correcciones_recientes.is_empty()
        {
            break;
        }
        chars_usados += costo;
        watermark_ids.push(c.id.clone());
        correcciones_recientes.push(CorreccionLoteConsolidacion {
            id: c.id.clone(),
            fecha: c.registrada.clone(),
            defensor_clave: if c.defensor_clave.trim().is_empty() {
                String::new()
            } else {
                normalizar_clave_defensor(&c.defensor_clave)
            },
            campo_principal: campo_p,
            texto_anonimizado: texto_rec,
            cambios: ca.campos,
            nota_usuario: nota_rec,
            estado: c.estado.clone(),
        });
    }

    if reglas_ambito.is_empty() && correcciones_recientes.is_empty() {
        return None;
    }

    let metricas_resumen: Vec<CampoDiagnostico> = CAMPOS_AUDIENCIA
        .iter()
        .filter_map(|&campo| {
            let mc = if amb_norm != "*" {
                memoria
                    .metricas_por_defensor
                    .get(&amb_norm)
                    .and_then(|md| md.por_campo.get(campo))
                    .or_else(|| memoria.metricas_campos.get(campo))
            } else {
                memoria.metricas_campos.get(campo)
            }?;
            if mc.evaluaciones == 0 {
                return None;
            }
            Some(CampoDiagnostico {
                campo: campo.to_string(),
                evaluaciones: mc.evaluaciones,
                aciertos: mc.aciertos,
                correcciones: mc.correcciones,
                precision_ewma: mc.precision_ewma,
                confianza_q10: limite_inferior_bayesiano(mc.alpha, mc.beta),
                drift_activo: memoria
                    .page_hinkley_campos
                    .get(campo)
                    .is_some_and(|p| p.drift_detectado),
            })
        })
        .collect();

    let patrones_transversales = calcular_patrones_transversales(memoria, &anon);
    let snapshot_extracciones = memoria
        .estados_consolidacion
        .get(&amb_norm)
        .map(|e| e.extracciones_desde_ultima)
        .unwrap_or(0);

    Some(LoteConsolidacion {
        ambito: amb_norm,
        nombre_ambito,
        resumen_actual,
        version_resumen,
        reglas_ambito,
        correcciones_recientes,
        metricas_resumen,
        patrones_transversales,
        watermark_ids,
        snapshot_extracciones,
        telemetria_disparo: telemetria,
    })
}

pub fn preparar_lote_consolidacion_en(
    ruta: &Path,
    ambito: &str,
    telemetria: TelemetriaDisparoConsolidacion,
    forzar_manual: bool,
) -> Option<LoteConsolidacion> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let memoria = cargar_memoria_en(ruta);
    preparar_lote_desde_memoria(&memoria, ambito, telemetria, forzar_manual, &[])
}

pub fn evaluar_y_preparar_consolidacion_en(
    ruta: &Path,
    defensor: Option<&str>,
) -> Option<LoteConsolidacion> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let semilla_antes = memoria.semilla_rng;
    let res = evaluar_candidato_consolidacion(&mut memoria, defensor, None);
    if memoria.semilla_rng != semilla_antes {
        let _ = guardar_memoria_en(ruta, &memoria);
    }
    let telemetria = res?;
    preparar_lote_desde_memoria(&memoria, &telemetria.ambito.clone(), telemetria, false, &[])
}

pub fn evaluar_y_preparar_consolidacion_ia(defensor: Option<&str>) -> Option<LoteConsolidacion> {
    evaluar_y_preparar_consolidacion_en(&ruta_memoria(), defensor)
}

pub fn aplicar_consolidacion_ia_en(
    ruta: &Path,
    lote: &LoteConsolidacion,
    resultado: &ResultadoConsolidacionIa,
    modelo: &str,
) -> Result<DiagnosticoAprendizaje, String> {
    {
        let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
        let mut memoria = cargar_memoria_en(ruta);
        let ahora = Local::now().to_rfc3339();
        let amb_norm = alcance_canonico(&lote.ambito);
        let mut acciones_reg = Vec::new();
        let mut sintesis_creadas = 0usize;

        for acc in &resultado.acciones_reglas {
            let tipo_acc = acc.tipo_accion.trim().to_lowercase();
            let mut aplicada = false;

            match tipo_acc.as_str() {
                "fusionar" => {
                    let regla_limpia = recortar_sin_elipsis(&acc.regla, 240);
                    let mut indices_validos: Vec<usize> = memoria
                        .reglas
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| {
                            acc.ids_origen.contains(&r.id)
                                && r.origen != "base_invariante"
                                && !r.id.starts_with("base-")
                                && r.estado != "retirada"
                                && (amb_norm == "*"
                                    || coincide_alcance_defensor(&r.defensor_alcance, &amb_norm))
                        })
                        .map(|(i, _)| i)
                        .collect();

                    if !indices_validos.is_empty() && regla_limpia.len() >= 6 {
                        indices_validos.sort_by(|&a, &b| {
                            prioridad_estado_regla(&memoria.reglas[b].estado)
                                .cmp(&prioridad_estado_regla(&memoria.reglas[a].estado))
                        });
                        let idx_principal = indices_validos[0];
                        let id_principal = memoria.reglas[idx_principal].id.clone();

                        for &idx_sec in indices_validos.iter().skip(1) {
                            let sec = memoria.reglas[idx_sec].clone();
                            memoria.reglas[idx_sec].estado = "retirada".to_string();
                            fusionar_regla_en(&mut memoria.reglas[idx_principal], sec);
                            let sec_id = memoria.reglas[idx_sec].id.clone();
                            for c in &mut memoria.correcciones {
                                if c.regla_id_asociada == sec_id {
                                    c.regla_id_asociada = id_principal.clone();
                                }
                            }
                        }

                        let dest = &mut memoria.reglas[idx_principal];
                        dest.regla = regla_limpia.clone();
                        if !acc.tipo_error.trim().is_empty() {
                            dest.tipo_error = acc.tipo_error.trim().to_lowercase();
                        }
                        if !acc.campo_objetivo.trim().is_empty() {
                            dest.campo_objetivo = acc.campo_objetivo.trim().to_lowercase();
                        }
                        for t in &acc.triggers {
                            let tn = quitar_tildes(&t.trim().to_lowercase());
                            if es_trigger_valido(&tn)
                                && !dest.triggers.contains(&tn)
                                && dest.triggers.len() < 12
                            {
                                dest.triggers.push(tn);
                            }
                        }
                        dest.actualizada = ahora.clone();
                        dest.ultima_actualizacion = ahora.clone();
                        aplicada = true;
                    }
                }
                "conflicto" => {
                    if acc.confianza >= 0.75 {
                        let mut toco_algo = false;
                        for r in &mut memoria.reglas {
                            if !acc.ids_origen.contains(&r.id) {
                                continue;
                            }
                            if r.origen == "base_invariante" || r.id.starts_with("base-") {
                                continue;
                            }
                            if amb_norm != "*" && !coincide_alcance_defensor(&r.defensor_alcance, &amb_norm) {
                                continue;
                            }
                            r.estado = if matches!(r.estado.as_str(), "candidata" | "cuarentena") {
                                "retirada".to_string()
                            } else {
                                "cuarentena".to_string()
                            };
                            r.aciertos_sombra = 0;
                            r.confirmaciones_sombra = 0;
                            r.ultima_actualizacion = ahora.clone();
                            toco_algo = true;
                        }
                        aplicada = toco_algo;
                    }
                }
                "sintetizar" => {
                    let regla_limpia = recortar_sin_elipsis(&acc.regla, 240);
                    if sintesis_creadas < 2 && regla_limpia.len() >= 6 {
                        let ya_en_base = memoria_base_embebida().reglas.iter().any(|rb| {
                            es_misma_regla(&rb.regla, &regla_limpia)
                                && (es_alcance_general(&rb.defensor_alcance)
                                    || amb_norm == "*"
                                    || coincide_alcance_defensor(&rb.defensor_alcance, &amb_norm))
                        });
                        let ya_en_local = memoria.reglas.iter().any(|rl| {
                            rl.estado != "retirada"
                                && es_misma_regla(&rl.regla, &regla_limpia)
                                && (alcance_canonico(&rl.defensor_alcance) == amb_norm
                                    || (amb_norm != "*" && es_alcance_general(&rl.defensor_alcance)))
                        });

                        if !ya_en_base && !ya_en_local {
                            let campo_obj = if acc.campo_objetivo.trim().is_empty() {
                                None
                            } else {
                                Some(acc.campo_objetivo.as_str())
                            };
                            let tipo_err = if acc.tipo_error.trim().is_empty() {
                                "general"
                            } else {
                                acc.tipo_error.as_str()
                            };
                            if agregar_regla_interna_con_defensor(
                                &mut memoria,
                                tipo_err,
                                &regla_limpia,
                                "prueba",
                                campo_obj,
                                &acc.triggers,
                                "consolidacion_ia",
                                "*",
                                &amb_norm,
                                &[],
                                acc.confianza.clamp(0.4, 0.98),
                                acc.ids_origen.first().map(String::as_str).unwrap_or(""),
                            )
                            .is_some()
                            {
                                sintesis_creadas += 1;
                                aplicada = true;
                            }
                        }
                    }
                }
                _ => {}
            }

            acciones_reg.push(AccionReglaConsolidacion {
                tipo_accion: tipo_acc,
                ids_origen: acc.ids_origen.clone(),
                tipo_error: acc.tipo_error.clone(),
                campo_objetivo: acc.campo_objetivo.clone(),
                regla: colapsar_espacios(&acc.regla),
                triggers: acc.triggers.clone(),
                motivo: colapsar_espacios(&acc.motivo),
                confianza: acc.confianza,
                aplicada,
            });
        }

        let max_res = if amb_norm == "*" {
            MAX_CHARS_RESUMEN_GENERAL
        } else {
            MAX_CHARS_RESUMEN_DEFENSOR
        };
        let resumen_recortado = recortar_sin_elipsis(&resultado.resumen_actualizado, max_res);
        if !resumen_recortado.is_empty() {
            let nombre_mostrar = if amb_norm == "*" {
                "General (transversal)".to_string()
            } else {
                memoria
                    .nombres_defensores
                    .get(&amb_norm)
                    .cloned()
                    .unwrap_or_else(|| lote.nombre_ambito.clone())
            };
            let entry = memoria
                .perfiles_sinteticos
                .entry(amb_norm.clone())
                .or_insert_with(|| PerfilSinteticoDefensor {
                    ambito: amb_norm.clone(),
                    nombre_mostrar: nombre_mostrar.clone(),
                    resumen: String::new(),
                    version: 0,
                    ultima_actualizacion: String::new(),
                });
            entry.nombre_mostrar = nombre_mostrar;
            entry.resumen = resumen_recortado.clone();
            entry.version = entry.version.saturating_add(1);
            entry.ultima_actualizacion = ahora.clone();
        }

        let sugerencias: Vec<SugerenciaBuild> = resultado
            .sugerencias_build
            .iter()
            .filter(|s| !s.detalle.trim().is_empty())
            .map(|s| SugerenciaBuild {
                categoria: colapsar_espacios(&s.categoria),
                ambito: amb_norm.clone(),
                detalle: colapsar_espacios(&s.detalle),
                evidencia: colapsar_espacios(&s.evidencia),
            })
            .collect();

        memoria.consolidaciones_ia.push(RegistroConsolidacionIa {
            id: generar_id_unico("cons"),
            fecha: ahora.clone(),
            ambito: amb_norm.clone(),
            nombre_ambito: lote.nombre_ambito.clone(),
            modelo: modelo.to_string(),
            acciones_reglas: acciones_reg,
            resumen_actualizado: resumen_recortado,
            sugerencias_build: sugerencias,
            telemetria_disparo: lote.telemetria_disparo.clone(),
        });
        if memoria.consolidaciones_ia.len() > MAX_CONSOLIDACIONES_HISTORIAL {
            let ex = memoria.consolidaciones_ia.len() - MAX_CONSOLIDACIONES_HISTORIAL;
            memoria.consolidaciones_ia.drain(0..ex);
        }

        incrementar_llamadas_consolidacion(&mut memoria, 1);

        // Descuento atómico únicamente de watermark_ids y reducción de presión (presion *= 0.2)
        let notas_por_id: BTreeMap<String, bool> = memoria
            .correcciones
            .iter()
            .map(|c| (c.id.clone(), !c.nota_usuario.trim().is_empty()))
            .collect();
        let est = memoria
            .estados_consolidacion
            .entry(amb_norm)
            .or_default();
        est.presion *= 0.2;
        est.ultima_consolidacion = ahora;
        est.extracciones_desde_ultima = est
            .extracciones_desde_ultima
            .saturating_sub(lote.snapshot_extracciones);
        for wid in &lote.watermark_ids {
            if !est.ids_consolidadas_recientes.contains(wid) {
                est.ids_consolidadas_recientes.push(wid.clone());
                est.correcciones_sin_consolidar =
                    est.correcciones_sin_consolidar.saturating_sub(1);
                if notas_por_id.get(wid).copied().unwrap_or(false) {
                    est.notas_sin_consolidar = est.notas_sin_consolidar.saturating_sub(1);
                }
            }
        }
        if est.ids_consolidadas_recientes.len() > 200 {
            let ex = est.ids_consolidadas_recientes.len() - 200;
            est.ids_consolidadas_recientes.drain(0..ex);
        }

        guardar_memoria_en(ruta, &memoria)?;
    }

    Ok(obtener_diagnostico_aprendizaje_en(ruta))
}

pub fn aplicar_consolidacion_ia(
    lote: &LoteConsolidacion,
    resultado: &ResultadoConsolidacionIa,
    modelo: &str,
) -> Result<DiagnosticoAprendizaje, String> {
    aplicar_consolidacion_ia_en(&ruta_memoria(), lote, resultado, modelo)
}

#[allow(clippy::too_many_arguments)]
fn observacion_contiene_reprogramacion_futura(obs: &str) -> bool {
    let s = quitar_tildes(&obs.to_lowercase());
    if s.trim().is_empty() {
        return false;
    }
    let niega_fijacion = s.contains("no fijan fecha")
        || s.contains("no se fijo nueva fecha")
        || s.contains("sin nueva fecha")
        || s.contains("sin fijar fecha");
    if s.contains("proxima fecha")
        || s.contains("proxima audiencia")
        || s.contains("reprogramada para")
        || s.contains("reprogramo para el")
        || s.contains("reprogramo para la")
        || s.contains("se senalo nuevamente")
        || s.contains("senalo nuevamente")
        || s.contains("senala nuevamente")
    {
        return true;
    }
    if !niega_fijacion && s.contains("nueva fecha") {
        return true;
    }
    if !niega_fijacion
        && (s.contains("se fijo")
            || s.contains("fijo la")
            || s.contains("fijo audiencia")
            || s.contains("fijo su")
            || s.contains("fije nueva"))
        && (s.contains("continuacion")
            || s.contains("audiencia")
            || s.contains("fecha")
            || s.contains("para el "))
    {
        return true;
    }
    false
}

fn actualizar_preferencia_reprogramacion_en_memoria(
    memoria: &mut MemoriaAprendizaje,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    def_clave: &str,
) {
    const MAX_ESS_PREF: f64 = 12.0;
    let pred_lower = prediccion.observaciones.to_lowercase();
    let fin_lower = final_usuario.observaciones.to_lowercase();
    // Evitar doble conteo cuando estadistica_ia ya procesó el marcador literal "reprogramada para:"
    if pred_lower.contains("reprogramada para:") || fin_lower.contains("reprogramada para:") {
        return;
    }
    let pred_tiene = observacion_contiene_reprogramacion_futura(&prediccion.observaciones);
    let fin_tiene = observacion_contiene_reprogramacion_futura(&final_usuario.observaciones);
    if !pred_tiene && !fin_tiene {
        return;
    }
    let es_positiva = fin_tiene;
    let aplicar = |pref: &mut PreferenciasEstilo, peso: f64| {
        if es_positiva {
            pref.incluir_reprogramacion_alpha += peso;
        } else {
            pref.incluir_reprogramacion_beta += peso;
        }
        normalizar_ess_par(
            &mut pref.incluir_reprogramacion_alpha,
            &mut pref.incluir_reprogramacion_beta,
            MAX_ESS_PREF,
        );
    };
    if !def_clave.is_empty() {
        let pref_def = memoria
            .preferencias_por_defensor
            .entry(def_clave.to_string())
            .or_insert_with(|| memoria.preferencias_estilo.clone());
        aplicar(pref_def, 1.0);
        aplicar(&mut memoria.preferencias_estilo, 0.35);
    } else {
        aplicar(&mut memoria.preferencias_estilo, 1.0);
    }
}

pub fn registrar_resultado_completo_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    proveedor: &str,
    modelo: &str,
    reglas_inyectadas: &[String],
    defensor: Option<&str>,
    prediccion_local: Option<&PrediccionLocalResumen>,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    if texto.trim().is_empty() {
        return Err("No hay texto de IA asociado a este registro.".to_string());
    }

    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let campos = detectar_correcciones(prediccion, final_usuario);
    let hubo_correccion = !campos.is_empty();
    let mut memoria = cargar_memoria_en(ruta);
    let ahora = Local::now().to_rfc3339();
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    if let Some(def_raw) = defensor {
        registrar_nombre_defensor(&mut memoria.nombres_defensores, def_raw);
    }

    memoria.enfriamiento_aclaracion = memoria.enfriamiento_aclaracion.saturating_sub(1);
    incrementar_llamadas_extraccion(&mut memoria, 1);

    // Auto-descartar aclaración previa pendiente si cambió el defensor o pasaron >= 2 turnos sin respuesta
    let mut auto_descartar_por_omision = false;
    if let Some(idx_esp) = memoria
        .correcciones
        .iter()
        .rposition(|c| c.estado == "esperando_aclaracion")
    {
        let prev_def = normalizar_clave_defensor(&memoria.correcciones[idx_esp].defensor_clave);
        let cambio_def = prev_def != def_clave;
        if cambio_def {
            for c_prev in memoria
                .correcciones
                .iter_mut()
                .filter(|c| c.estado == "esperando_aclaracion")
            {
                c_prev.pregunta_ia.clear();
                c_prev.estado = if c_prev.regla_generada.trim().is_empty() {
                    "pendiente"
                } else {
                    "analizada"
                }
                .to_string();
            }
            memoria.estado_aclaracion.turnos_sin_responder = 0;
        } else {
            memoria.estado_aclaracion.turnos_sin_responder =
                memoria.estado_aclaracion.turnos_sin_responder.saturating_add(1);
            if memoria.estado_aclaracion.turnos_sin_responder >= 2 {
                for c_prev in memoria
                    .correcciones
                    .iter_mut()
                    .filter(|c| c.estado == "esperando_aclaracion")
                {
                    c_prev.pregunta_ia.clear();
                    c_prev.estado = if c_prev.regla_generada.trim().is_empty() {
                        "pendiente"
                    } else {
                        "analizada"
                    }
                    .to_string();
                }
                auto_descartar_por_omision = true;
            }
        }
    }
    if auto_descartar_por_omision {
        memoria.estado_aclaracion.turnos_sin_responder = 0;
        memoria.estado_aclaracion.beta_omit += 1.0;
        memoria.estado_aclaracion.racha_omisiones =
            memoria.estado_aclaracion.racha_omisiones.saturating_add(1);
        let madurez = calcular_madurez_sistema(&memoria);
        let base_cd = (3.0 + 11.0 * madurez).round() as u32;
        let mult = 1u32 << memoria.estado_aclaracion.racha_omisiones.min(3);
        memoria.enfriamiento_aclaracion = memoria
            .enfriamiento_aclaracion
            .max(base_cd.saturating_mul(mult));
    }

    // Leer precision_ewma previa para calcular sorpresa de Shannon antes de actualizar este turno
    let sorpresa_campos: f64 = campos
        .iter()
        .map(|fc| {
            let p_hat = obtener_precision_ewma_antes(&memoria, &def_clave, &fc.campo);
            -(1.0 - p_hat).max(0.05).ln()
        })
        .sum();

    let campos_con_drift = actualizar_metricas_y_credito(
        &mut memoria,
        texto,
        &campos,
        reglas_inyectadas,
        proveedor,
        modelo,
        &ahora,
        &def_clave,
    );
    crate::estadistica_ia::actualizar_alias_y_preferencias_en_memoria_para_defensor(
        &mut memoria,
        texto,
        prediccion,
        final_usuario,
        &campos,
        &ahora,
        if def_clave.is_empty() {
            None
        } else {
            Some(def_clave.as_str())
        },
    );
    actualizar_preferencia_reprogramacion_en_memoria(
        &mut memoria,
        prediccion,
        final_usuario,
        &def_clave,
    );

    let mut nueva_correccion = None;
    if hubo_correccion {
        let mut corr = CorreccionIa {
            id: generar_id_unico("corr"),
            registrada: ahora.clone(),
            proveedor: proveedor.to_string(),
            modelo: modelo.to_string(),
            texto_original: texto.trim().to_string(),
            prediccion: prediccion.clone(),
            final_usuario: final_usuario.clone(),
            campos,
            estado: estado_pendiente(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: def_clave.clone(),
        };
        encolar_correccion_en_cluster(&mut memoria, &corr, &ahora);

        // Decidir aclaración localmente (<1 ms, 0 tokens LLM)
        if let Some(pregunta) = decidir_aclaracion_local(&memoria, &corr, prediccion_local, &def_clave) {
            corr.estado = "esperando_aclaracion".to_string();
            corr.pregunta_ia = pregunta;
            memoria.estado_aclaracion.turnos_sin_responder = 0;
        }

        // Impulsos de presión de consolidación (defensor + general "*")
        let hay_drift_corr = !campos_con_drift.is_empty()
            || corr.campos.iter().any(|fc| {
                memoria
                    .page_hinkley_campos
                    .get(&fc.campo)
                    .is_some_and(|p| p.drift_detectado)
            });
        let impulso_drift = if hay_drift_corr { 1.5 } else { 0.0 };
        let es_huerfana_n1 = memoria.cola_induccion.iter().any(|clu| {
            clu.estado == "acumulando"
                && clu.correccion_ids.len() == 1
                && clu.correccion_ids.contains(&corr.id)
        });
        let impulso_huerfana = if es_huerfana_n1 { 0.55 } else { 0.0 };
        let reglas_activas_total = memoria
            .reglas
            .iter()
            .filter(|r| r.estado != "retirada")
            .count();
        let hay_cuarentena = memoria.reglas.iter().any(|r| r.estado == "cuarentena");
        let impulso_reglas = if reglas_activas_total > 42 || hay_cuarentena {
            0.6
        } else {
            0.0
        };
        let sorpresa_total = sorpresa_campos + impulso_drift + impulso_huerfana + impulso_reglas;

        let (_, firma_corr) = calcular_firma_error(&corr);
        nueva_correccion = Some(corr.clone());
        memoria.correcciones.push(corr);
        if memoria.correcciones.len() > MAX_CORRECCIONES {
            let exceso = memoria.correcciones.len() - MAX_CORRECCIONES;
            memoria.correcciones.drain(0..exceso);
        }

        if !def_clave.is_empty() {
            let est_def = memoria
                .estados_consolidacion
                .entry(def_clave)
                .or_default();
            est_def.extracciones_desde_ultima = est_def.extracciones_desde_ultima.saturating_add(1);
            est_def.correcciones_sin_consolidar =
                est_def.correcciones_sin_consolidar.saturating_add(1);
            est_def.presion += sorpresa_total;
        }

        let defensores_firma: BTreeSet<String> = memoria
            .correcciones
            .iter()
            .filter(|c| c.estado != "descartada" && calcular_firma_error(c).1 == firma_corr)
            .map(|c| normalizar_clave_defensor(&c.defensor_clave))
            .filter(|k| !k.is_empty())
            .collect();
        let est_gen = memoria
            .estados_consolidacion
            .entry("*".to_string())
            .or_default();
        est_gen.extracciones_desde_ultima = est_gen.extracciones_desde_ultima.saturating_add(1);
        est_gen.correcciones_sin_consolidar =
            est_gen.correcciones_sin_consolidar.saturating_add(1);
        if defensores_firma.len() >= 2 {
            est_gen.presion += 1.0 * sorpresa_total + 1.0;
        } else {
            est_gen.presion += 0.45 * sorpresa_total;
        }
    } else {
        incrementar_aciertos(&mut memoria, 1);
        let anon = Anonimizador::desde_radicados(&[&final_usuario.radicado]);
        let ejemplo = EjemploPositivo {
            id: generar_id_unico("pos"),
            registrada: ahora,
            proveedor: proveedor.to_string(),
            modelo: modelo.to_string(),
            texto_anonimizado: anon.aplicar(texto.trim()),
            salida_anonimizada: anon.aplicar_audiencia(final_usuario),
            reglas_inyectadas: reglas_inyectadas.to_vec(),
            defensor_clave: def_clave.clone(),
        };
        memoria.ejemplos_aciertos.push(ejemplo);
        if memoria.ejemplos_aciertos.len() > MAX_ACIERTOS_EJEMPLOS {
            let exceso = memoria.ejemplos_aciertos.len() - MAX_ACIERTOS_EJEMPLOS;
            memoria.ejemplos_aciertos.drain(0..exceso);
        }

        if !def_clave.is_empty() {
            let est_def = memoria
                .estados_consolidacion
                .entry(def_clave)
                .or_default();
            est_def.presion *= 0.93;
            est_def.extracciones_desde_ultima = est_def.extracciones_desde_ultima.saturating_add(1);
        }
        let est_gen = memoria
            .estados_consolidacion
            .entry("*".to_string())
            .or_default();
        est_gen.presion *= 0.93;
        est_gen.extracciones_desde_ultima = est_gen.extracciones_desde_ultima.saturating_add(1);
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok((resumen_de(&memoria, hubo_correccion), nueva_correccion))
}

#[allow(clippy::too_many_arguments)]
pub fn registrar_resultado_con_traza_y_defensor_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    proveedor: &str,
    modelo: &str,
    reglas_inyectadas: &[String],
    defensor: Option<&str>,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_completo_en(
        ruta,
        texto,
        prediccion,
        final_usuario,
        proveedor,
        modelo,
        reglas_inyectadas,
        defensor,
        None,
    )
}

pub fn registrar_resultado_con_traza_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    proveedor: &str,
    modelo: &str,
    reglas_inyectadas: &[String],
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_con_traza_y_defensor_en(
        ruta,
        texto,
        prediccion,
        final_usuario,
        proveedor,
        modelo,
        reglas_inyectadas,
        None,
    )
}

pub fn registrar_resultado_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    proveedor: &str,
    modelo: &str,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_con_traza_en(
        ruta,
        texto,
        prediccion,
        final_usuario,
        proveedor,
        modelo,
        &[],
    )
}

/// Aplica el resultado de la 2ª llamada a la IA sobre una corrección guardada.
pub fn aplicar_analisis_en(
    ruta: &Path,
    correccion_id: &str,
    analisis: &AnalisisCorreccionIa,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    incrementar_llamadas_induccion(&mut memoria, 1);

    let mut regla_a_registrar: Option<(String, String, String, String, Vec<String>, f64, bool)> =
        None;
    let mut regla_id_a_retirar: Option<String> = None;
    let mut modelo_alcance = "*".to_string();
    let mut defensor_alcance = "*".to_string();
    if let Some(pos) = memoria.correcciones.iter().position(|c| c.id == correccion_id) {
        let corr = &mut memoria.correcciones[pos];
        modelo_alcance = format!("{}:{}", corr.proveedor, corr.modelo);
        if !corr.defensor_clave.trim().is_empty() {
            defensor_alcance = corr.defensor_clave.clone();
        }
        if !analisis.es_error_de_ia {
            corr.estado = "descartada".to_string();
            corr.pregunta_ia.clear();
            if !corr.regla_id_asociada.is_empty() {
                regla_id_a_retirar = Some(std::mem::take(&mut corr.regla_id_asociada));
            }
        } else {
            corr.estado = "analizada".to_string();
            corr.pregunta_ia.clear();
            let tiene_nota = !corr.nota_usuario.trim().is_empty();
            if !analisis.regla.trim().is_empty() {
                let limpia = colapsar_espacios(&analisis.regla);
                corr.regla_generada = limpia.clone();
                let campo_inf = inferir_campo_objetivo_desde_tipo(&analisis.tipo_error);
                let campo_obj = if campo_inf != "general" {
                    campo_inf
                } else {
                    corr.campos
                        .first()
                        .map(|c| c.campo.clone())
                        .unwrap_or(campo_inf)
                };
                let triggers =
                    regenerar_triggers_correccion(&corr.texto_original, &limpia, &corr.nota_usuario);
                regla_a_registrar = Some((
                    analisis.tipo_error.clone(),
                    limpia,
                    corr.regla_id_asociada.clone(),
                    campo_obj,
                    triggers,
                    analisis.confianza,
                    tiene_nota,
                ));
            }
        }
    }

    if let Some(rid_ret) = regla_id_a_retirar {
        retiro_o_eliminacion_regla_asociada(&mut memoria, &rid_ret, correccion_id);
    }

    if let Some((tipo, regla, id_asociado, campo_obj, triggers, confianza, tiene_nota)) =
        regla_a_registrar
    {
        let estado_obj = if tiene_nota { "prueba" } else { "candidata" };
        let mut actualizada_en_sitio = false;
        if !id_asociado.is_empty() {
            if let Some(existente) = memoria.reglas.iter_mut().find(|r| r.id == id_asociado) {
                if !tipo.trim().is_empty() && tipo.trim() != "general" {
                    existente.tipo_error = tipo.trim().to_lowercase();
                }
                if existente.origen != "base_invariante"
                    && !existente.id.starts_with("base-")
                    && campo_obj != "general"
                {
                    existente.campo_objetivo = campo_obj.clone();
                }
                if regla.len() >= 6 && regla.len() <= 240 {
                    existente.regla = regla.clone();
                }
                if !triggers.is_empty() {
                    existente.triggers = triggers.clone();
                }
                if tiene_nota
                    && matches!(
                        existente.estado.as_str(),
                        "candidata" | "cuarentena" | "retirada"
                    )
                {
                    existente.estado = "prueba".to_string();
                }
                existente.actualizada = Local::now().to_rfc3339();
                existente.ultima_actualizacion = existente.actualizada.clone();
                existente.confianza_llm = confianza;
                actualizada_en_sitio = true;
            }
        }
        if !actualizada_en_sitio {
            if let Some(nuevo_id) = agregar_regla_interna_con_defensor(
                &mut memoria,
                &tipo,
                &regla,
                estado_obj,
                Some(&campo_obj),
                &triggers,
                "mitigacion_modelo",
                &modelo_alcance,
                &defensor_alcance,
                &[],
                confianza,
                correccion_id,
            ) {
                if let Some(corr) = memoria
                    .correcciones
                    .iter_mut()
                    .find(|c| c.id == correccion_id)
                {
                    corr.regla_id_asociada = nuevo_id;
                }
            }
        }
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, true))
}

/// Registra la nota explicativa del usuario para una corrección que pedía aclaración.
/// No inserta una regla cruda con triggers vacíos: guarda `nota_usuario`, deja `estado = "pendiente"`
/// para refinamiento/análisis o consolidación, actualiza `alpha_resp` y suma presión de consolidación.
pub fn guardar_nota_aclaracion_en(
    ruta: &Path,
    correccion_id: &str,
    nota: &str,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    let nota_limpia = colapsar_espacios(nota);
    if nota_limpia.is_empty() {
        return descartar_aclaracion_en(ruta, correccion_id).map(|r| (r, None));
    }

    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let mut defensor_corr = String::new();
    let mut clave_firma = None;
    let mut corr_actualizada = None;

    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        corr.nota_usuario = nota_limpia.clone();
        corr.pregunta_ia.clear();
        corr.estado = "pendiente".to_string();
        if !corr.defensor_clave.trim().is_empty() {
            defensor_corr = normalizar_clave_defensor(&corr.defensor_clave);
        }
        clave_firma = Some(clave_firma_aclaracion(corr).1);
        corr_actualizada = Some(corr.clone());
    }

    if corr_actualizada.is_none() {
        return Ok((resumen_de(&memoria, false), None));
    }

    memoria.estado_aclaracion.alpha_resp += 1.0;
    memoria.estado_aclaracion.racha_omisiones = 0;
    memoria.estado_aclaracion.turnos_sin_responder = 0;
    if let Some(kf) = clave_firma {
        registrar_firma_explicada(&mut memoria, kf);
    }

    let madurez = calcular_madurez_sistema(&memoria);
    memoria.enfriamiento_aclaracion = (3.0 + 11.0 * madurez).round() as u32;

    let mut reabrir_en_ambito = |ambito_est: &str, impulso_presion: f64| {
        let est = memoria
            .estados_consolidacion
            .entry(ambito_est.to_string())
            .or_default();
        est.presion += impulso_presion;
        est.notas_sin_consolidar = est.notas_sin_consolidar.saturating_add(1);
        let antes = est.ids_consolidadas_recientes.len();
        est.ids_consolidadas_recientes
            .retain(|id| id != correccion_id);
        if est.ids_consolidadas_recientes.len() < antes {
            est.correcciones_sin_consolidar = est.correcciones_sin_consolidar.saturating_add(1);
        }
    };
    if !defensor_corr.is_empty() {
        reabrir_en_ambito(&defensor_corr, 1.4);
    }
    reabrir_en_ambito("*", 0.6);

    guardar_memoria_en(ruta, &memoria)?;
    Ok((resumen_de(&memoria, false), corr_actualizada))
}

/// Marca la corrección como "Dato externo (no estaba en el texto)" a 0 tokens LLM,
/// descarta la corrección y limpia cualquier regla asociada sin dejar huérfanas.
pub fn marcar_dato_externo_en(
    ruta: &Path,
    correccion_id: &str,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let mut regla_id_prev = String::new();
    let mut clave_firma = None;

    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        corr.pregunta_ia.clear();
        corr.estado = "descartada".to_string();
        corr.nota_usuario = "Dato externo (no estaba en el texto)".to_string();
        regla_id_prev = std::mem::take(&mut corr.regla_id_asociada);
        clave_firma = Some(clave_firma_aclaracion(corr).1);
    }

    if !regla_id_prev.is_empty() {
        retiro_o_eliminacion_regla_asociada(&mut memoria, &regla_id_prev, correccion_id);
    }

    memoria.estado_aclaracion.alpha_resp += 1.0;
    memoria.estado_aclaracion.racha_omisiones = 0;
    memoria.estado_aclaracion.turnos_sin_responder = 0;
    if let Some(kf) = clave_firma {
        registrar_firma_explicada(&mut memoria, kf);
    }
    let madurez = calcular_madurez_sistema(&memoria);
    memoria.enfriamiento_aclaracion = (3.0 + 11.0 * madurez).round() as u32;

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, false))
}

/// Permite omitir una solicitud de aclaración sin borrar la corrección subyacente,
/// incrementando `beta_omit` y aplicando enfriamiento exponencial.
pub fn descartar_aclaracion_en(
    ruta: &Path,
    correccion_id: &str,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let mut regla_previa: Option<(String, String, String)> = None;
    let mut defensor_corr = "*".to_string();

    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        corr.pregunta_ia.clear();
        corr.estado = if corr.regla_generada.trim().is_empty() {
            "pendiente".to_string()
        } else {
            "analizada".to_string()
        };
        if !corr.defensor_clave.trim().is_empty() {
            defensor_corr = normalizar_clave_defensor(&corr.defensor_clave);
        }
        if !corr.regla_generada.trim().is_empty() && corr.regla_id_asociada.trim().is_empty() {
            regla_previa = Some((
                corr.regla_generada.clone(),
                corr.texto_original.clone(),
                corr.nota_usuario.clone(),
            ));
        }
    }

    if let Some((regla, texto_orig, nota_usr)) = regla_previa {
        let tr = regenerar_triggers_correccion(&texto_orig, &regla, &nota_usr);
        let _ = agregar_regla_interna_con_defensor(
            &mut memoria,
            "general",
            &regla,
            "candidata",
            None,
            &tr,
            "mitigacion_modelo",
            "*",
            &defensor_corr,
            &[],
            0.5,
            correccion_id,
        );
    }

    memoria.estado_aclaracion.beta_omit += 1.0;
    memoria.estado_aclaracion.racha_omisiones =
        memoria.estado_aclaracion.racha_omisiones.saturating_add(1);
    memoria.estado_aclaracion.turnos_sin_responder = 0;

    let madurez = calcular_madurez_sistema(&memoria);
    let base_cd = (3.0 + 11.0 * madurez).round() as u32;
    let mult = 1u32 << memoria.estado_aclaracion.racha_omisiones.min(3);
    memoria.enfriamiento_aclaracion = base_cd.saturating_mul(mult);

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, false))
}

/// Devuelve el primer clúster coherente listo para inducción agrupada (`N >= 2`).
/// Nunca devuelve casos aislados `N = 1`, reduciendo llamadas secundarias al LLM.
pub fn obtener_cluster_listo_para_inducir_en(ruta: &Path) -> Option<(String, Vec<CorreccionIa>)> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);

    let mut cluster_listo: Option<(String, Vec<CorreccionIa>)> = None;
    let mut cambio_cola = false;

    for cluster in &mut memoria.cola_induccion {
        if cluster.estado != "listo" {
            continue;
        }
        let miembros: Vec<CorreccionIa> = cluster
            .correccion_ids
            .iter()
            .filter_map(|cid| {
                memoria
                    .correcciones
                    .iter()
                    .find(|c| {
                        &c.id == cid
                            && matches!(c.estado.as_str(), "pendiente" | "esperando_aclaracion")
                    })
                    .cloned()
            })
            .take(4)
            .collect();

        if miembros.len() >= MIN_TAMANO_CLUSTER_INDUCCION {
            cluster_listo = Some((cluster.id.clone(), miembros));
            break;
        } else {
            // Ya fueron procesadas/descartadas por otra vía
            cluster.estado = "inducido".to_string();
            cambio_cola = true;
        }
    }

    if cambio_cola {
        let _ = guardar_memoria_en(ruta, &memoria);
    }
    cluster_listo
}

/// Aplica el análisis agrupado de 1 sola llamada LLM sobre todas las correcciones del clúster.
pub fn aplicar_analisis_cluster_en(
    ruta: &Path,
    cluster_id: &str,
    correccion_ids: &[String],
    analisis: &AnalisisCorreccionIa,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    incrementar_llamadas_induccion(&mut memoria, 1);

    let mut ids_set: BTreeSet<String> = correccion_ids.iter().cloned().collect();
    if let Some(clu) = memoria.cola_induccion.iter().find(|c| c.id == cluster_id) {
        for cid in &clu.correccion_ids {
            ids_set.insert(cid.clone());
        }
    }

    let mut regla_inducida_id = String::new();
    if !analisis.es_error_de_ia {
        let mut a_retirar = Vec::new();
        for c in &mut memoria.correcciones {
            if ids_set.contains(&c.id) {
                c.estado = "descartada".to_string();
                c.pregunta_ia.clear();
                if !c.regla_id_asociada.is_empty() {
                    a_retirar.push((std::mem::take(&mut c.regla_id_asociada), c.id.clone()));
                }
            }
        }
        for (rid, cid) in a_retirar {
            retiro_o_eliminacion_regla_asociada(&mut memoria, &rid, &cid);
        }
    } else {
        let mut triggers_unificados: Vec<String> = Vec::new();
        let campo_inf = inferir_campo_objetivo_desde_tipo(&analisis.tipo_error);
        let mut campo_obj = if campo_inf != "general" {
            campo_inf
        } else {
            String::new()
        };
        let mut modelo_alcance = "*".to_string();
        let mut rep_id = String::new();
        let mut defensores_cluster: Vec<String> = Vec::new();

        for c in &memoria.correcciones {
            if !ids_set.contains(&c.id) {
                continue;
            }
            if rep_id.is_empty() {
                rep_id = c.id.clone();
            }
            if campo_obj.is_empty() {
                if let Some(cf) = c.campos.first() {
                    campo_obj = cf.campo.clone();
                }
            }
            modelo_alcance = format!("{}:{}", c.proveedor, c.modelo);
            let dk = normalizar_clave_defensor(&c.defensor_clave);
            if !dk.is_empty() && !defensores_cluster.contains(&dk) {
                defensores_cluster.push(dk);
            }
            for t in extraer_triggers_texto(&c.texto_original, 6) {
                if !triggers_unificados.contains(&t) && triggers_unificados.len() < 10 {
                    triggers_unificados.push(t);
                }
            }
        }
        if campo_obj.is_empty() {
            campo_obj = "general".to_string();
        }

        let defensor_alcance_regla = if defensores_cluster.len() == 1 {
            defensores_cluster[0].clone()
        } else {
            "*".to_string()
        };

        let regla_limpia = colapsar_espacios(&analisis.regla);
        // En clúster coherente (N >= 2), entrar directamente a "prueba" desde confianza >= 0.55 para aprender en 2 veces
        let estado_inicial = if ids_set.len() >= MIN_TAMANO_CLUSTER_INDUCCION
            && analisis.confianza >= 0.55
        {
            "prueba"
        } else {
            "candidata"
        };

        let nuevo_id = if !regla_limpia.is_empty() {
            agregar_regla_interna_con_defensor(
                &mut memoria,
                &analisis.tipo_error,
                &regla_limpia,
                estado_inicial,
                Some(&campo_obj),
                &triggers_unificados,
                "mitigacion_modelo",
                &modelo_alcance,
                &defensor_alcance_regla,
                &defensores_cluster,
                analisis.confianza,
                &rep_id,
            )
        } else {
            None
        };

        if let Some(ref rid) = nuevo_id {
            regla_inducida_id = rid.clone();
        }
        for c in &mut memoria.correcciones {
            if ids_set.contains(&c.id) {
                c.estado = "analizada".to_string();
                c.pregunta_ia.clear();
                if !regla_limpia.is_empty() {
                    c.regla_generada = regla_limpia.clone();
                }
                if let Some(ref rid) = nuevo_id {
                    c.regla_id_asociada = rid.clone();
                }
            }
        }
    }

    if let Some(clu) = memoria.cola_induccion.iter_mut().find(|c| c.id == cluster_id) {
        clu.estado = "inducido".to_string();
        if !regla_inducida_id.is_empty() {
            clu.regla_inducida_id = regla_inducida_id;
        }
    }
    if !memoria
        .correcciones
        .iter()
        .any(|c| c.estado == "esperando_aclaracion")
    {
        memoria.estado_aclaracion.turnos_sin_responder = 0;
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, false))
}

#[allow(dead_code)]
pub fn obtener_pendientes_analisis_en(ruta: &Path, limite: usize) -> Vec<CorreccionIa> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let memoria = cargar_memoria_en(ruta);
    memoria
        .correcciones
        .into_iter()
        .rev()
        .filter(|c| c.estado == "pendiente")
        .take(limite)
        .collect()
}

fn construir_preferencias_resumen(pref: &PreferenciasEstilo) -> PreferenciasEstiloResumen {
    let ratio = |a: f64, b: f64| {
        let d = (a + b).max(1e-6);
        (a / d).clamp(0.0, 1.0)
    };
    PreferenciasEstiloResumen {
        incluir_causas_secundarias_prob: ratio(
            pref.incluir_causas_secundarias_alpha,
            pref.incluir_causas_secundarias_beta,
        ),
        incluir_reprogramacion_prob: ratio(
            pref.incluir_reprogramacion_alpha,
            pref.incluir_reprogramacion_beta,
        ),
        se_fija_aplazamiento_prob: ratio(
            pref.se_fija_aplazamiento_alpha,
            pref.se_fija_aplazamiento_beta,
        ),
        instalada_reprogramada_si_prob: ratio(
            pref.instalada_reprogramada_si_alpha,
            pref.instalada_reprogramada_si_beta,
        ),
    }
}

fn clave_plantilla_juzgado(valor: &str) -> Option<String> {
    let sin_slot = valor.replace("{N}", " ").replace("{n}", " ");
    let norm = normalizar_clave_catalogo(&sin_slot);
    if !norm.starts_with("juzgado ") && !norm.starts_with("tribunal ") {
        return None;
    }
    let es_num_u_ordinal = |t: &str| -> bool {
        t.chars().all(|c| c.is_ascii_digit())
            || matches!(
                t,
                "1ro"
                    | "2do"
                    | "3ro"
                    | "4to"
                    | "5to"
                    | "6to"
                    | "7mo"
                    | "8vo"
                    | "9no"
                    | "10mo"
                    | "primero"
                    | "segundo"
                    | "tercero"
                    | "cuarto"
                    | "quinto"
                    | "sexto"
                    | "septimo"
                    | "octavo"
                    | "noveno"
                    | "decimo"
            )
    };
    let toks: Vec<&str> = norm
        .split_whitespace()
        .filter(|t| !es_num_u_ordinal(t))
        .collect();
    if toks.len() < 3 {
        return None;
    }
    Some(toks.join(" "))
}

fn plantillas_juzgados_oficiales() -> &'static HashSet<String> {
    static PLANTILLAS: OnceLock<HashSet<String>> = OnceLock::new();
    PLANTILLAS.get_or_init(|| {
        let mut set = HashSet::new();
        for j in crate::estadistica_ia::juzgados_base() {
            if let Some(plantilla) = clave_plantilla_juzgado(j) {
                set.insert(plantilla);
            }
        }
        set
    })
}

fn claves_catalogo_oficiales() -> &'static HashSet<String> {
    static OFICIALES: OnceLock<HashSet<String>> = OnceLock::new();
    OFICIALES.get_or_init(|| {
        let mut set = HashSet::new();
        for t in TIPOS_AUDIENCIA {
            let norm = normalizar_clave_catalogo(t);
            if !norm.is_empty() {
                set.insert(norm);
            }
        }
        for j in crate::estadistica_ia::juzgados_base() {
            let norm = normalizar_clave_catalogo(j);
            if !norm.is_empty() {
                set.insert(norm);
            }
        }
        set
    })
}

fn es_juzgado_o_tipo_oficial(valor: &str) -> bool {
    let norm = normalizar_clave_catalogo(valor);
    if norm.is_empty() {
        return false;
    }
    if claves_catalogo_oficiales().contains(&norm) {
        return true;
    }
    clave_plantilla_juzgado(valor)
        .is_some_and(|plantilla| plantillas_juzgados_oficiales().contains(&plantilla))
}

fn anonimizar_valor_catalogo(valor: &str, anon: &Anonimizador) -> String {
    if es_juzgado_o_tipo_oficial(valor) {
        valor.to_string()
    } else {
        anon.aplicar(valor)
    }
}

/// Caché local para evitar re-anonimizar los mismos nombres de juzgados/tipos repetidos
/// a través del grafo de asociaciones, frecuencias, alias y reporte de uso durante una exportación.
struct CacheCatalogoAnon<'a> {
    anon: &'a Anonimizador,
    cache: RefCell<HashMap<String, String>>,
}

impl<'a> CacheCatalogoAnon<'a> {
    fn new(anon: &'a Anonimizador) -> Self {
        Self {
            anon,
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn anonimizar(&self, valor: &str) -> String {
        if let Some(cached) = self.cache.borrow().get(valor) {
            return cached.clone();
        }
        let res = anonimizar_valor_catalogo(valor, self.anon);
        self.cache.borrow_mut().insert(valor.to_string(), res.clone());
        res
    }
}

fn recoger_radicados_y_escudos(
    memoria: &MemoriaAprendizaje,
    radicados_excel: &[String],
    nombres_defensores_extra: &[String],
) -> (Vec<String>, Vec<String>) {
    fn agregar_rad<'a>(s: &'a str, vistos: &mut HashSet<&'a str>, out: &mut Vec<String>) {
        let trim = s.trim();
        if !trim.is_empty() && vistos.insert(trim) {
            out.push(trim.to_string());
        }
    }
    let mut radicados_vistos: HashSet<&str> = HashSet::with_capacity(
        memoria.correcciones.len() * 2 + memoria.ejemplos_aciertos.len() + radicados_excel.len(),
    );
    let mut radicados: Vec<String> = Vec::with_capacity(radicados_excel.len());
    for c in &memoria.correcciones {
        agregar_rad(&c.prediccion.radicado, &mut radicados_vistos, &mut radicados);
        agregar_rad(&c.final_usuario.radicado, &mut radicados_vistos, &mut radicados);
    }
    for e in &memoria.ejemplos_aciertos {
        agregar_rad(&e.salida_anonimizada.radicado, &mut radicados_vistos, &mut radicados);
    }
    for rad in radicados_excel {
        agregar_rad(rad, &mut radicados_vistos, &mut radicados);
    }

    let mut escudos_vistos: HashSet<String> = HashSet::new();
    let mut escudos: Vec<String> = Vec::new();
    let mut push_escudo = |s: &str| {
        let limpio = colapsar_espacios(s);
        if !limpio.is_empty()
            && !es_alcance_general(&limpio)
            && escudos_vistos.insert(limpio.clone())
        {
            escudos.push(limpio);
        }
    };
    for (k, v) in &memoria.nombres_defensores {
        push_escudo(k);
        push_escudo(v);
    }
    for (k, m) in &memoria.metricas_por_defensor {
        push_escudo(k);
        push_escudo(&m.nombre_mostrar);
    }
    for (k, p) in &memoria.perfiles_sinteticos {
        if !es_alcance_general(k) {
            push_escudo(k);
            push_escudo(&p.nombre_mostrar);
        }
    }
    for r in &memoria.reglas {
        push_escudo(&r.defensor_alcance);
        for d in &r.defensores_confirmantes {
            push_escudo(d);
        }
    }
    for c in &memoria.correcciones {
        push_escudo(&c.defensor_clave);
    }
    for e in &memoria.ejemplos_aciertos {
        push_escudo(&e.defensor_clave);
    }
    for n in nombres_defensores_extra {
        push_escudo(n);
    }
    (radicados, escudos)
}

fn calcular_patrones_transversales(
    memoria: &MemoriaAprendizaje,
    anon: &Anonimizador,
) -> Vec<PatronTransversalExport> {
    #[derive(Default)]
    struct Acum {
        campo: String,
        ocurrencias: usize,
        defensores: BTreeSet<String>,
        ejemplo: String,
    }
    let mut mapa: BTreeMap<String, Acum> = BTreeMap::new();
    for c in &memoria.correcciones {
        if c.estado == "descartada" {
            continue;
        }
        let (campo, firma) = calcular_firma_error(c);
        if firma.is_empty() {
            continue;
        }
        let entry = mapa.entry(firma).or_default();
        if entry.campo.is_empty() {
            entry.campo = campo;
        }
        entry.ocurrencias += 1;
        let dk = normalizar_clave_defensor(&c.defensor_clave);
        if !dk.is_empty() {
            entry.defensores.insert(dk);
        }
        let ej_txt = recortar_sin_elipsis(&anon.aplicar(&c.texto_original), 140);
        if !ej_txt.is_empty() {
            entry.ejemplo = ej_txt;
        }
    }

    let mut out: Vec<PatronTransversalExport> = mapa
        .into_iter()
        .filter(|(_, ac)| ac.defensores.len() >= 2 || ac.ocurrencias >= 2)
        .map(|(firma, ac)| PatronTransversalExport {
            firma,
            campo: ac.campo,
            ocurrencias: ac.ocurrencias,
            defensores_distintos: ac.defensores.len(),
            ejemplo_anonimizado: ac.ejemplo,
        })
        .collect();

    out.sort_by(|a, b| {
        b.defensores_distintos
            .cmp(&a.defensores_distintos)
            .then_with(|| b.ocurrencias.cmp(&a.ocurrencias))
            .then_with(|| a.firma.cmp(&b.firma))
    });
    out.truncate(12);
    out
}

fn construir_directorio_defensores(
    memoria: &MemoriaAprendizaje,
    priors: Option<&IndicePriorsExcel>,
    nombres_settings: &[String],
) -> Vec<ResumenDefensorExport> {
    let mut nombres: BTreeMap<String, String> = memoria.nombres_defensores.clone();
    for raw in nombres_settings {
        registrar_nombre_defensor(&mut nombres, raw);
    }

    let mut claves: BTreeSet<String> = BTreeSet::new();
    let mut insertar = |raw: &str| {
        if !es_alcance_general(raw) {
            let dk = normalizar_clave_defensor(raw);
            if !dk.is_empty() {
                claves.insert(dk);
            }
        }
    };

    for (k, m) in &memoria.metricas_por_defensor {
        insertar(k);
        registrar_nombre_defensor(&mut nombres, k);
        registrar_nombre_defensor(&mut nombres, &m.nombre_mostrar);
    }
    for k in memoria.nombres_defensores.keys() {
        insertar(k);
    }
    for k in memoria.preferencias_por_defensor.keys() {
        insertar(k);
        registrar_nombre_defensor(&mut nombres, k);
    }
    for (k, p) in &memoria.perfiles_sinteticos {
        if !es_alcance_general(k) {
            insertar(k);
            registrar_nombre_defensor(&mut nombres, k);
            registrar_nombre_defensor(&mut nombres, &p.nombre_mostrar);
        }
    }
    for k in memoria.estados_consolidacion.keys() {
        insertar(k);
    }
    for r in &memoria.reglas {
        insertar(&r.defensor_alcance);
        for d in &r.defensores_confirmantes {
            insertar(d);
            registrar_nombre_defensor(&mut nombres, d);
        }
    }
    for a in &memoria.tabla_alias {
        insertar(&a.defensor_alcance);
        for d in &a.defensores_confirmantes {
            insertar(d);
            registrar_nombre_defensor(&mut nombres, d);
        }
    }
    for c in &memoria.correcciones {
        insertar(&c.defensor_clave);
    }
    for e in &memoria.ejemplos_aciertos {
        insertar(&e.defensor_clave);
    }
    for clu in &memoria.cola_induccion {
        insertar(&clu.defensor_clave);
        for d in &clu.defensores_presentes {
            insertar(d);
        }
    }
    if let Some(pr) = priors {
        for dk in pr
            .grafo
            .defensor_a_juzgados
            .keys()
            .chain(pr.grafo.defensor_a_tipos.keys())
            .chain(pr.grafo.defensor_sigla_a_juzgado.keys())
        {
            insertar(dk);
            registrar_nombre_defensor(&mut nombres, dk);
        }
    }

    claves
        .into_iter()
        .map(|clave| {
            let md = memoria.metricas_por_defensor.get(&clave);
            let aciertos = md.map(|m| m.aciertos).unwrap_or(0);
            let correcciones = md.map(|m| m.correcciones).unwrap_or(0);
            let total = aciertos.saturating_add(correcciones);
            let tasa_acierto = if total > 0 {
                aciertos as f64 / total as f64
            } else {
                0.0
            };
            let nombre = nombres
                .get(&clave)
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .or_else(|| {
                    md.map(|m| m.nombre_mostrar.clone())
                        .filter(|s| !s.trim().is_empty())
                })
                .unwrap_or_else(|| title_case_basico(&clave));

            let por_campo: Vec<CampoDiagnostico> = md
                .map(|m| {
                    CAMPOS_AUDIENCIA
                        .iter()
                        .filter_map(|&campo| {
                            m.por_campo.get(campo).map(|mc| CampoDiagnostico {
                                campo: campo.to_string(),
                                evaluaciones: mc.evaluaciones,
                                aciertos: mc.aciertos,
                                correcciones: mc.correcciones,
                                precision_ewma: mc.precision_ewma,
                                confianza_q10: limite_inferior_bayesiano(mc.alpha, mc.beta),
                                drift_activo: memoria
                                    .page_hinkley_campos
                                    .get(campo)
                                    .is_some_and(|ph| ph.drift_detectado),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            ResumenDefensorExport {
                clave,
                nombre,
                aciertos,
                correcciones,
                tasa_acierto,
                por_campo,
            }
        })
        .collect()
}

fn construir_resumen_defensores(
    memoria: &MemoriaAprendizaje,
    priors: Option<&IndicePriorsExcel>,
    nombres_settings: &[String],
) -> Vec<ResumenDefensorExport> {
    construir_directorio_defensores(memoria, priors, nombres_settings)
}

fn construir_resumen_patrones_generales(
    memoria: &MemoriaAprendizaje,
    priors: Option<&IndicePriorsExcel>,
    directorio: &[ResumenDefensorExport],
    cache_cat: &CacheCatalogoAnon<'_>,
) -> ResumenPatronesGenerales {
    let mut top_juzgados: Vec<(String, u32)> = priors
        .map(|p| {
            p.juzgados_freq
                .iter()
                .map(|(nombre, &conteo)| (cache_cat.anonimizar(nombre), conteo))
                .collect()
        })
        .unwrap_or_default();
    top_juzgados.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_juzgados.truncate(10);

    let mut top_tipos: Vec<(String, u32)> = priors
        .map(|p| {
            p.tipos_freq
                .iter()
                .map(|(nombre, &conteo)| (cache_cat.anonimizar(nombre), conteo))
                .collect()
        })
        .unwrap_or_default();
    top_tipos.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_tipos.truncate(10);

    let mut top_horas: Vec<(u32, u32)> = priors
        .map(|p| p.horas_freq.iter().map(|(&h, &c)| (h, c)).collect())
        .unwrap_or_default();
    top_horas.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_horas.truncate(10);

    let reglas_transversales_promovidas = memoria
        .reglas
        .iter()
        .filter(|r| {
            r.estado != "retirada"
                && !r.id.starts_with("base-")
                && es_alcance_general(&r.defensor_alcance)
                && r.defensores_confirmantes.len() >= 2
        })
        .count();

    let errores_recurrentes_transversales = calcular_patrones_transversales(memoria, cache_cat.anon);

    ResumenPatronesGenerales {
        total_defensores_activos: directorio.len(),
        top_juzgados_globales: top_juzgados,
        top_tipos_globales: top_tipos,
        top_horas_globales: top_horas,
        reglas_transversales_promovidas,
        errores_recurrentes_transversales,
    }
}

fn anonimizar_registro_consolidacion(
    reg: &RegistroConsolidacionIa,
    anon: &Anonimizador,
) -> RegistroConsolidacionIa {
    let ambito = alcance_canonico(&reg.ambito);
    let limpiar_trig = |t: &str| {
        let a = anon.aplicar(t);
        let l = a.trim();
        if l.is_empty()
            || l == "[PROCESADO]"
            || l == "[RADICADO]"
            || l == "[TELEFONO]"
            || l == "[NUMERO]"
        {
            None
        } else {
            Some(a)
        }
    };
    RegistroConsolidacionIa {
        id: reg.id.clone(),
        fecha: reg.fecha.clone(),
        ambito: ambito.clone(),
        nombre_ambito: reg.nombre_ambito.clone(),
        modelo: reg.modelo.clone(),
        acciones_reglas: reg
            .acciones_reglas
            .iter()
            .map(|a| AccionReglaConsolidacion {
                tipo_accion: a.tipo_accion.clone(),
                ids_origen: a.ids_origen.clone(),
                tipo_error: a.tipo_error.clone(),
                campo_objetivo: a.campo_objetivo.clone(),
                regla: anon.aplicar(&a.regla),
                triggers: a.triggers.iter().filter_map(|t| limpiar_trig(t)).collect(),
                motivo: anon.aplicar(&a.motivo),
                confianza: a.confianza,
                aplicada: a.aplicada,
            })
            .collect(),
        resumen_actualizado: anon.aplicar(&reg.resumen_actualizado),
        sugerencias_build: reg
            .sugerencias_build
            .iter()
            .map(|s| SugerenciaBuild {
                categoria: s.categoria.clone(),
                ambito: alcance_canonico(&s.ambito),
                detalle: anon.aplicar(&s.detalle),
                evidencia: anon.aplicar(&s.evidencia),
            })
            .collect(),
        telemetria_disparo: TelemetriaDisparoConsolidacion {
            ambito,
            presion: reg.telemetria_disparo.presion,
            umbral_adaptativo: reg.telemetria_disparo.umbral_adaptativo,
            probabilidad_disparo: reg.telemetria_disparo.probabilidad_disparo,
            muestra_rng: reg.telemetria_disparo.muestra_rng,
            manual: reg.telemetria_disparo.manual,
            razones: reg
                .telemetria_disparo
                .razones
                .iter()
                .map(|r| anon.aplicar(r))
                .collect(),
        },
    }
}

fn construir_metricas_diagnostico(
    memoria: &MemoriaAprendizaje,
    cache_cat: &CacheCatalogoAnon<'_>,
) -> (
    Vec<CampoDiagnostico>,
    Vec<ModeloDiagnostico>,
    Vec<AliasDiagnostico>,
    Vec<String>,
) {
    let mut campos_diag = Vec::with_capacity(CAMPOS_AUDIENCIA.len());
    let mut drift_campos_activos = Vec::new();

    for campo in CAMPOS_AUDIENCIA {
        let mc = memoria.metricas_campos.get(campo);
        let drift_activo = memoria
            .page_hinkley_campos
            .get(campo)
            .is_some_and(|ph| ph.drift_detectado);
        if drift_activo {
            drift_campos_activos.push(campo.to_string());
        }
        let (evaluaciones, aciertos, correcciones, ewma, q10) = if let Some(m) = mc {
            (
                m.evaluaciones,
                m.aciertos,
                m.correcciones,
                m.precision_ewma,
                limite_inferior_bayesiano(m.alpha, m.beta),
            )
        } else {
            (0, 0, 0, 0.5, limite_inferior_bayesiano(PRIOR_BETA_ALPHA, PRIOR_BETA_BETA))
        };
        campos_diag.push(CampoDiagnostico {
            campo: campo.to_string(),
            evaluaciones,
            aciertos,
            correcciones,
            precision_ewma: ewma,
            confianza_q10: q10,
            drift_activo,
        });
    }

    let modelos_diag: Vec<ModeloDiagnostico> = memoria
        .metricas_modelos
        .values()
        .map(|mm| {
            let prec = if mm.evaluaciones > 0 {
                mm.aciertos as f64 / mm.evaluaciones as f64
            } else {
                0.0
            };
            ModeloDiagnostico {
                clave: mm.clave.clone(),
                evaluaciones: mm.evaluaciones,
                aciertos: mm.aciertos,
                correcciones: mm.correcciones,
                precision_media: prec,
            }
        })
        .collect();

    let mut alias_diag: Vec<AliasDiagnostico> = memoria
        .tabla_alias
        .iter()
        .filter_map(|a| {
            let q10 = limite_inferior_bayesiano(a.alpha, a.beta);
            if q10 < 0.30 {
                return None;
            }
            let (entrada_norm, canonico) = if a.campo == "juzgado" || a.campo == "tipo" {
                (
                    cache_cat.anonimizar(&a.entrada_norm),
                    cache_cat.anonimizar(&a.canonico),
                )
            } else {
                (
                    cache_cat.anon.aplicar(&a.entrada_norm),
                    cache_cat.anon.aplicar(&a.canonico),
                )
            };
            Some(AliasDiagnostico {
                campo: a.campo.clone(),
                entrada_norm,
                canonico,
                defensor_alcance: alcance_canonico(&a.defensor_alcance),
                confianza_q10: q10,
            })
        })
        .collect();
    alias_diag.sort_by(|a, b| {
        b.confianza_q10
            .partial_cmp(&a.confianza_q10)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    (campos_diag, modelos_diag, alias_diag, drift_campos_activos)
}

fn construir_patrones_diagnostico(
    memoria: &MemoriaAprendizaje,
    cache_cat: &CacheCatalogoAnon<'_>,
) -> Vec<PatronAbreviacionDiagnostico> {
    crate::patrones_abreviacion::inducir_patrones_todos_ambitos(memoria)
        .into_iter()
        .map(|p| PatronAbreviacionDiagnostico {
            campo: p.campo,
            tipo_slot: p.tipo_slot.as_str().to_string(),
            patron_entrada: cache_cat.anonimizar(&p.patron_entrada),
            patron_canonico: cache_cat.anonimizar(&p.patron_canonico),
            defensor_alcance: alcance_canonico(&p.defensor_alcance),
            ejemplos_distintos: p.ejemplos_distintos,
            total_instancias: p.total_instancias,
            consistencia: p.consistencia,
            confianza_q10: p.confianza_q10,
        })
        .collect()
}

fn construir_diagnostico_desde_memoria(
    memoria: &MemoriaAprendizaje,
    priors: Option<&IndicePriorsExcel>,
    nombres_settings: &[String],
) -> DiagnosticoAprendizaje {
    let (radicados_str, escudos) = recoger_radicados_y_escudos(memoria, &[], nombres_settings);
    let radicados_ref: Vec<&str> = radicados_str.iter().map(String::as_str).collect();
    let anon = Anonimizador::desde_radicados(&radicados_ref).con_escudos_defensores(&escudos);
    let cache_cat = CacheCatalogoAnon::new(&anon);

    let (metricas_por_campo, metricas_por_modelo, alias_validados, drift_campos_activos) =
        construir_metricas_diagnostico(memoria, &cache_cat);
    let patrones_abreviacion = construir_patrones_diagnostico(memoria, &cache_cat);

    let mut reglas_por_estado: BTreeMap<String, usize> = BTreeMap::new();
    for r in &memoria.reglas {
        *reglas_por_estado.entry(r.estado.clone()).or_insert(0) += 1;
    }

    let clusters_pendientes = memoria
        .cola_induccion
        .iter()
        .filter(|c| c.estado == "acumulando")
        .count();
    let clusters_listos = memoria
        .cola_induccion
        .iter()
        .filter(|c| c.estado == "listo")
        .count();

    let mut reglas_destacadas: Vec<ReglaDiagnostico> = memoria
        .reglas
        .iter()
        .filter(|r| r.estado != "retirada")
        .map(|r| ReglaDiagnostico {
            id: r.id.clone(),
            estado: r.estado.clone(),
            campo_objetivo: r.campo_objetivo.clone(),
            origen: r.origen.clone(),
            defensor_alcance: alcance_canonico(&r.defensor_alcance),
            regla: anon.aplicar(&r.regla),
            impresiones: r.impresiones,
            aciertos_directos: r.aciertos_directos,
            fallos_directos: r.fallos_directos,
            confianza_q10: limite_inferior_bayesiano(r.alpha, r.beta),
        })
        .collect();
    reglas_destacadas.sort_by(|a, b| {
        prioridad_estado_regla(&b.estado)
            .cmp(&prioridad_estado_regla(&a.estado))
            .then_with(|| {
                b.confianza_q10
                    .partial_cmp(&a.confianza_q10)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    reglas_destacadas.truncate(15);

    let defensores_resumen = construir_resumen_defensores(memoria, priors, nombres_settings);
    let mapa_nombres: BTreeMap<String, String> = defensores_resumen
        .iter()
        .map(|d| (d.clave.clone(), d.nombre.clone()))
        .collect();

    let presion_consolidacion_global = memoria
        .estados_consolidacion
        .get("*")
        .map(|e| e.presion)
        .unwrap_or(0.0);

    let mut presiones_por_defensor: Vec<PresionDefensorDiagnostico> = Vec::new();
    let mut claves_presion: BTreeSet<String> = memoria.estados_consolidacion.keys().cloned().collect();
    for d in &defensores_resumen {
        claves_presion.insert(d.clave.clone());
    }
    for k in claves_presion {
        let amb = alcance_canonico(&k);
        let est = memoria
            .estados_consolidacion
            .get(&amb)
            .cloned()
            .unwrap_or_default();
        let nombre = if amb == "*" {
            "General (transversal)".to_string()
        } else {
            mapa_nombres
                .get(&amb)
                .cloned()
                .unwrap_or_else(|| title_case_basico(&amb))
        };
        presiones_por_defensor.push(PresionDefensorDiagnostico {
            clave: amb,
            nombre,
            presion: est.presion,
            correcciones_sin_consolidar: est.correcciones_sin_consolidar,
            notas_sin_consolidar: est.notas_sin_consolidar,
            extracciones_desde_ultima: est.extracciones_desde_ultima,
            ultima_consolidacion: est.ultima_consolidacion,
        });
    }
    presiones_por_defensor.sort_by(|a, b| {
        (a.clave != "*")
            .cmp(&(b.clave != "*"))
            .then_with(|| {
                b.presion
                    .partial_cmp(&a.presion)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.clave.cmp(&b.clave))
    });

    let mut perfiles_sinteticos: Vec<PerfilSinteticoDefensor> = memoria
        .perfiles_sinteticos
        .values()
        .filter(|p| !p.resumen.trim().is_empty())
        .map(|p| {
            let amb = alcance_canonico(&p.ambito);
            let nombre_mostrar = if amb == "*" {
                "General (transversal)".to_string()
            } else {
                mapa_nombres
                    .get(&amb)
                    .cloned()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        if !p.nombre_mostrar.trim().is_empty() {
                            Some(p.nombre_mostrar.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| title_case_basico(&amb))
            };
            PerfilSinteticoDefensor {
                ambito: amb,
                nombre_mostrar,
                resumen: anon.aplicar(&p.resumen),
                version: p.version,
                ultima_actualizacion: p.ultima_actualizacion.clone(),
            }
        })
        .collect();
    perfiles_sinteticos.sort_by(|a, b| {
        (a.ambito != "*")
            .cmp(&(b.ambito != "*"))
            .then_with(|| a.ambito.cmp(&b.ambito))
    });

    let ultima_consolidacion = memoria
        .consolidaciones_ia
        .last()
        .map(|r| anonimizar_registro_consolidacion(r, &anon));

    let denom_aclar =
        (memoria.estado_aclaracion.alpha_resp + memoria.estado_aclaracion.beta_omit).max(1e-6);
    let tasa_respuesta_aclaraciones = (memoria.estado_aclaracion.alpha_resp / denom_aclar).clamp(0.0, 1.0);

    DiagnosticoAprendizaje {
        aciertos_totales: memoria.aciertos,
        correcciones_totales: memoria.correcciones.len(),
        ejemplos_positivos_totales: memoria.ejemplos_aciertos.len(),
        llamadas_induccion_llm_total: memoria.llamadas_induccion_llm_total,
        clusters_pendientes,
        clusters_listos,
        perfiles_defensores: defensores_resumen.len(),
        conexiones_grafo: priors.map(|p| p.grafo.total_conexiones()).unwrap_or(0),
        drift_campos_activos,
        enfriamiento_aclaracion: memoria.enfriamiento_aclaracion,
        reglas_por_estado,
        metricas_por_campo,
        metricas_por_modelo,
        preferencias_estilo: construir_preferencias_resumen(&memoria.preferencias_estilo),
        alias_validados: alias_validados.into_iter().take(12).collect(),
        patrones_abreviacion: patrones_abreviacion.into_iter().take(12).collect(),
        reglas_destacadas,
        consolidaciones_totales: memoria.llamadas_consolidacion_total,
        presion_consolidacion_global,
        presiones_por_defensor,
        ultima_consolidacion,
        perfiles_sinteticos,
        tasa_respuesta_aclaraciones,
        defensores_resumen,
    }
}

pub fn obtener_diagnostico_aprendizaje_en(ruta: &Path) -> DiagnosticoAprendizaje {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let memoria = cargar_memoria_en(ruta);
    construir_diagnostico_desde_memoria(&memoria, None, &[])
}

fn anonimizar_ejemplo_positivo_con_global(
    e: &EjemploPositivo,
    anon_global: &Anonimizador,
    cache_cat: &CacheCatalogoAnon<'_>,
) -> EjemploPositivo {
    EjemploPositivo {
        id: e.id.clone(),
        registrada: e.registrada.clone(),
        proveedor: e.proveedor.clone(),
        modelo: e.modelo.clone(),
        texto_anonimizado: anon_global.aplicar(&e.texto_anonimizado),
        salida_anonimizada: AudienciaInput {
            radicado: anon_global.aplicar(&e.salida_anonimizada.radicado),
            tipo: cache_cat.anonimizar(&e.salida_anonimizada.tipo),
            fecha: e.salida_anonimizada.fecha.clone(),
            hora: e.salida_anonimizada.hora.clone(),
            juzgado: cache_cat.anonimizar(&e.salida_anonimizada.juzgado),
            realizada: e.salida_anonimizada.realizada.clone(),
            motivo: e.salida_anonimizada.motivo.clone(),
            observaciones: anon_global.aplicar(&e.salida_anonimizada.observaciones),
        },
        reglas_inyectadas: e.reglas_inyectadas.clone(),
        defensor_clave: if e.defensor_clave.trim().is_empty() {
            String::new()
        } else {
            normalizar_clave_defensor(&e.defensor_clave)
        },
    }
}

pub fn exportar_aprendizaje_completo_con_priors_en(
    ruta: &Path,
    ruta_uso: Option<&Path>,
    carpeta_archivos: Option<&Path>,
    priors_explicitos: Option<&IndicePriorsExcel>,
    carpeta_destino: &Path,
    juzgados_usuario: &[String],
    tipos_usuario: &[String],
    nombres_defensores_settings: &[String],
) -> Result<String, String> {
    let uso = match (ruta_uso, carpeta_archivos) {
        (Some(r_uso), Some(carp)) => {
            let mut s_tmp = AppSettings::default();
            s_tmp.juzgados = juzgados_usuario.to_vec();
            s_tmp.tipos_audiencia = tipos_usuario.to_vec();
            asegurar_uso_inicializado_en(r_uso, carp, &s_tmp)
        }
        (Some(r_uso), None) => cargar_uso_con_lock_en(r_uso),
        _ => UsoCatalogos::default(),
    };
    let instancia_id_uso = uso.instancia_id.clone();

    let radicados_excel = carpeta_archivos
        .map(recolectar_radicados_en)
        .unwrap_or_default();

    let priors_propios: Option<IndicePriorsExcel> = if priors_explicitos.is_some() {
        None
    } else {
        carpeta_archivos.map(cargar_o_inicializar_priors_en)
    };
    let priors_ref = priors_explicitos.or(priors_propios.as_ref());

    let (destino_str, instancia_id_confirmar) = {
        let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
        let mut memoria = cargar_memoria_en(ruta);
        asegurar_instancia_y_contadores(&mut memoria);
        let instancia_id = if !instancia_id_uso.trim().is_empty() {
            instancia_id_uso.clone()
        } else {
            memoria.instancia_id.clone()
        };
        let mut fuentes_set: BTreeSet<String> = uso.fuentes_importadas.iter().cloned().collect();
        for inst in &memoria.instancias_sincronizadas {
            if !inst.trim().is_empty() && inst != &instancia_id {
                fuentes_set.insert(inst.clone());
            }
        }
        for inst in memoria.contadores_por_instancia.keys() {
            if !inst.trim().is_empty() && inst != &instancia_id && inst != &memoria.instancia_id {
                fuentes_set.insert(inst.clone());
            }
        }
        let fuentes_importadas: Vec<String> = fuentes_set.into_iter().collect();

        if memoria.correcciones.is_empty()
            && memoria.reglas.is_empty()
            && memoria.aciertos == 0
            && memoria.ejemplos_aciertos.is_empty()
            && memoria.consolidaciones_ia.is_empty()
            && memoria.tabla_alias.is_empty()
            && juzgados_usuario.is_empty()
            && tipos_usuario.is_empty()
            && uso.total_audiencias_contabilizadas == 0
        {
            return Err(
                "Aún no hay audiencias registradas, actividad de la IA ni elementos agregados para compartir."
                    .to_string(),
            );
        }

        let ahora_dt = Local::now();
        let exportado_ahora = ahora_dt.to_rfc3339();
        let exportacion_seq = memoria.exportacion_seq.saturating_add(1);
        let exportacion_id = format!(
            "exp-{}-{}",
            ahora_dt.format("%Y%m%d%H%M%S%3f"),
            exportacion_seq
        );
        let ultima_exportacion_previa = memoria.ultima_exportacion_previa.clone();

        let mut correcciones_por_campo: BTreeMap<String, usize> = BTreeMap::new();
        for c in &memoria.correcciones {
            for campo in &c.campos {
                *correcciones_por_campo.entry(campo.campo.clone()).or_insert(0) += 1;
            }
        }

        let defensores_directorio =
            construir_directorio_defensores(&memoria, priors_ref, nombres_defensores_settings);
        let mut nombres_extra: Vec<String> = nombres_defensores_settings.to_vec();
        for d in &defensores_directorio {
            nombres_extra.push(d.nombre.clone());
            nombres_extra.push(d.clave.clone());
        }
        let (todos_radicados, escudos) =
            recoger_radicados_y_escudos(&memoria, &radicados_excel, &nombres_extra);
        let radicados_ref: Vec<&str> = todos_radicados.iter().map(String::as_str).collect();
        let anon_global =
            Anonimizador::desde_radicados(&radicados_ref).con_escudos_defensores(&escudos);
        let cache_cat = CacheCatalogoAnon::new(&anon_global);

        let reglas_anonimizadas: Vec<ReglaAprendida> = memoria
            .reglas
            .iter()
            .map(|r| {
                let triggers_limpios: Vec<String> = r
                    .triggers
                    .iter()
                    .map(|t| anon_global.aplicar(t))
                    .filter(|t| {
                        let limpio = t.trim();
                        !limpio.is_empty()
                            && limpio != "[PROCESADO]"
                            && limpio != "[RADICADO]"
                            && limpio != "[TELEFONO]"
                            && limpio != "[NUMERO]"
                    })
                    .collect();
                let confirmantes_norm: Vec<String> = r
                    .defensores_confirmantes
                    .iter()
                    .map(|d| normalizar_clave_defensor(d))
                    .filter(|d| !d.is_empty())
                    .collect();
                ReglaAprendida {
                    regla: anon_global.aplicar(&r.regla),
                    triggers: triggers_limpios,
                    defensor_alcance: alcance_canonico(&r.defensor_alcance),
                    defensores_confirmantes: confirmantes_norm,
                    ..r.clone()
                }
            })
            .collect();

        let correcciones_anonimizadas: Vec<CorreccionIa> = memoria
            .correcciones
            .iter()
            .map(|c| {
                let mut ca = anonimizar_correccion_con_global(c, Some(&anon_global));
                ca.defensor_clave = if c.defensor_clave.trim().is_empty() {
                    String::new()
                } else {
                    normalizar_clave_defensor(&c.defensor_clave)
                };
                ca
            })
            .collect();

        let ejemplos_aciertos_anon: Vec<EjemploPositivo> = memoria
            .ejemplos_aciertos
            .iter()
            .map(|e| anonimizar_ejemplo_positivo_con_global(e, &anon_global, &cache_cat))
            .collect();

        let juzgados_anon: Vec<String> = juzgados_usuario
            .iter()
            .map(|j| cache_cat.anonimizar(j))
            .collect();
        let tipos_anon: Vec<String> = tipos_usuario
            .iter()
            .map(|t| cache_cat.anonimizar(t))
            .collect();

        let reporte_uso = construir_reporte_uso_catalogos(
            &uso,
            juzgados_usuario,
            tipos_usuario,
            &|s| cache_cat.anonimizar(s),
        );

        let (metricas_por_campo, metricas_por_modelo, alias_validados, _) =
            construir_metricas_diagnostico(&memoria, &cache_cat);

        let mut tabla_alias_raw: Vec<AliasEstadistico> = memoria
            .tabla_alias
            .iter()
            .map(|a| {
                let (entrada_norm, canonico) = if a.campo == "juzgado" || a.campo == "tipo" {
                    (
                        cache_cat.anonimizar(&a.entrada_norm),
                        cache_cat.anonimizar(&a.canonico),
                    )
                } else {
                    (
                        anon_global.aplicar(&a.entrada_norm),
                        anon_global.aplicar(&a.canonico),
                    )
                };
                let mut confirmantes: Vec<String> = Vec::new();
                for d in &a.defensores_confirmantes {
                    let dk = normalizar_clave_defensor(d);
                    if !dk.is_empty() && !confirmantes.contains(&dk) {
                        confirmantes.push(dk);
                    }
                }
                confirmantes.sort();
                AliasEstadistico {
                    campo: a.campo.clone(),
                    entrada_norm,
                    canonico,
                    alpha: a.alpha,
                    beta: a.beta,
                    defensor_alcance: alcance_canonico(&a.defensor_alcance),
                    defensores_confirmantes: confirmantes,
                    ultima_actualizacion: a.ultima_actualizacion.clone(),
                }
            })
            .collect();
        tabla_alias_raw.sort_by(|a, b| {
            let qa = limite_inferior_bayesiano(a.alpha, a.beta);
            let qb = limite_inferior_bayesiano(b.alpha, b.beta);
            qb.partial_cmp(&qa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.campo.cmp(&b.campo))
                .then_with(|| a.defensor_alcance.cmp(&b.defensor_alcance))
                .then_with(|| a.entrada_norm.cmp(&b.entrada_norm))
                .then_with(|| a.canonico.cmp(&b.canonico))
        });

        let patrones_abreviacion = construir_patrones_diagnostico(&memoria, &cache_cat);
        let preferencias_estilo = construir_preferencias_resumen(&memoria.preferencias_estilo);
        let preferencias_raw = Some(memoria.preferencias_estilo.clone());
        let mut preferencias_por_defensor_raw: BTreeMap<String, PreferenciasEstilo> =
            BTreeMap::new();
        for (k, p) in &memoria.preferencias_por_defensor {
            let dk = normalizar_clave_defensor(k);
            if dk.is_empty() {
                continue;
            }
            match preferencias_por_defensor_raw.get_mut(&dk) {
                Some(dest) => fusionar_preferencias_estilo(dest, p),
                None => {
                    preferencias_por_defensor_raw.insert(dk, p.clone());
                }
            }
        }
        let preferencias_por_defensor: BTreeMap<String, PreferenciasEstiloResumen> =
            preferencias_por_defensor_raw
                .iter()
                .map(|(dk, p)| (dk.clone(), construir_preferencias_resumen(p)))
                .collect();

        let resumen_patrones_generales = construir_resumen_patrones_generales(
            &memoria,
            priors_ref,
            &defensores_directorio,
            &cache_cat,
        );

        let grafo_asociaciones: GrafoAsociaciones = priors_ref
            .map(|p| {
                let mut g = GrafoAsociaciones::default();
                for (dk, m) in &p.grafo.defensor_a_juzgados {
                    let k_norm = normalizar_clave_defensor(dk);
                    if k_norm.is_empty() {
                        continue;
                    }
                    let dest = g.defensor_a_juzgados.entry(k_norm).or_default();
                    for (juz, &cnt) in m {
                        let juz_limpio = cache_cat.anonimizar(juz);
                        *dest.entry(juz_limpio).or_insert(0) += cnt;
                    }
                }
                for (dk, m) in &p.grafo.defensor_a_tipos {
                    let k_norm = normalizar_clave_defensor(dk);
                    if k_norm.is_empty() {
                        continue;
                    }
                    let dest = g.defensor_a_tipos.entry(k_norm).or_default();
                    for (tp, &cnt) in m {
                        let tp_limpio = cache_cat.anonimizar(tp);
                        *dest.entry(tp_limpio).or_insert(0) += cnt;
                    }
                }
                for (dk, siglas) in &p.grafo.defensor_sigla_a_juzgado {
                    let k_norm = normalizar_clave_defensor(dk);
                    if k_norm.is_empty() {
                        continue;
                    }
                    let dest_dk = g.defensor_sigla_a_juzgado.entry(k_norm).or_default();
                    for (sig, m) in siglas {
                        let dest_sig = dest_dk.entry(sig.clone()).or_default();
                        for (juz, &cnt) in m {
                            let juz_limpio = cache_cat.anonimizar(juz);
                            *dest_sig.entry(juz_limpio).or_insert(0) += cnt;
                        }
                    }
                }
                for (pref, m) in &p.grafo.prefijo_radicado_a_juzgados {
                    let dest = g.prefijo_radicado_a_juzgados.entry(pref.clone()).or_default();
                    for (juz, &cnt) in m {
                        let juz_limpio = cache_cat.anonimizar(juz);
                        *dest.entry(juz_limpio).or_insert(0) += cnt;
                    }
                }
                for (tp, m) in &p.grafo.tipo_a_juzgados {
                    let tp_limpio = cache_cat.anonimizar(tp);
                    let dest = g.tipo_a_juzgados.entry(tp_limpio).or_default();
                    for (juz, &cnt) in m {
                        let juz_limpio = cache_cat.anonimizar(juz);
                        *dest.entry(juz_limpio).or_insert(0) += cnt;
                    }
                }
                g
            })
            .unwrap_or_default();

        let mapa_nombres: BTreeMap<String, String> = defensores_directorio
            .iter()
            .map(|d| (d.clave.clone(), d.nombre.clone()))
            .collect();

        let mut perfiles_sinteticos: Vec<PerfilSinteticoDefensor> = memoria
            .perfiles_sinteticos
            .values()
            .map(|p| {
                let amb = alcance_canonico(&p.ambito);
                let nombre_mostrar = if amb == "*" {
                    "General (transversal)".to_string()
                } else {
                    mapa_nombres
                        .get(&amb)
                        .cloned()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| {
                            if !p.nombre_mostrar.trim().is_empty() {
                                Some(p.nombre_mostrar.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| title_case_basico(&amb))
                };
                PerfilSinteticoDefensor {
                    ambito: amb,
                    nombre_mostrar,
                    resumen: anon_global.aplicar(&p.resumen),
                    version: p.version,
                    ultima_actualizacion: p.ultima_actualizacion.clone(),
                }
            })
            .collect();
        perfiles_sinteticos.sort_by(|a, b| {
            (a.ambito != "*")
                .cmp(&(b.ambito != "*"))
                .then_with(|| a.ambito.cmp(&b.ambito))
        });

        let consolidaciones_ia: Vec<RegistroConsolidacionIa> = memoria
            .consolidaciones_ia
            .iter()
            .map(|c| anonimizar_registro_consolidacion(c, &anon_global))
            .collect();

        let paquete = PaqueteExportacion {
            app: "Lokus".to_string(),
            version_app: env!("CARGO_PKG_VERSION").to_string(),
            formato: VERSION_FORMATO,
            exportado: exportado_ahora.clone(),
            instancia_id: instancia_id.clone(),
            exportacion_seq,
            exportacion_id,
            ultima_exportacion_previa,
            fuentes_importadas,
            aciertos: memoria.aciertos,
            total_reglas: reglas_anonimizadas.len(),
            total_correcciones: correcciones_anonimizadas.len(),
            total_ejemplos_aciertos: ejemplos_aciertos_anon.len(),
            perfiles_defensores: defensores_directorio.len(),
            llamadas_induccion_llm_total: memoria.llamadas_induccion_llm_total,
            llamadas_extraccion_total: memoria.llamadas_extraccion_total,
            llamadas_consolidacion_total: memoria.llamadas_consolidacion_total,
            correcciones_por_campo,
            metricas_por_campo,
            metricas_por_modelo,
            preferencias_estilo,
            preferencias_por_defensor,
            preferencias_raw,
            preferencias_por_defensor_raw,
            defensores_directorio,
            resumen_patrones_generales,
            grafo_asociaciones,
            perfiles_sinteticos,
            consolidaciones_ia,
            alias_validados,
            tabla_alias_raw,
            patrones_abreviacion,
            juzgados_agregados_usuario: juzgados_anon,
            tipos_audiencia_agregados_usuario: tipos_anon,
            uso_catalogos: reporte_uso,
            reglas: reglas_anonimizadas,
            correcciones: correcciones_anonimizadas,
            ejemplos_aciertos: ejemplos_aciertos_anon,
        };
        let json = serde_json::to_string_pretty(&paquete)
            .map_err(|e| format!("Error preparando la exportación del aprendizaje: {}", e))?;

        fs::create_dir_all(carpeta_destino)
            .map_err(|e| format!("No se pudo acceder a la carpeta de destino: {}", e))?;
        let base = format!("lokus_aprendizaje_ia_{}", ahora_dt.format("%Y%m%d_%H%M%S"));
        let mut destino = carpeta_destino.join(format!("{}.json", base));
        let mut contador = 1;
        while destino.exists() {
            destino = carpeta_destino.join(format!("{}_{}.json", base, contador));
            contador += 1;
        }
        fs::write(&destino, json)
            .map_err(|e| format!("No se pudo guardar el archivo de aprendizaje: {}", e))?;

        let tiene_actividad_ia = ruta.exists()
            || !memoria.correcciones.is_empty()
            || !memoria.reglas.is_empty()
            || memoria.aciertos > 0
            || !memoria.ejemplos_aciertos.is_empty()
            || !memoria.consolidaciones_ia.is_empty()
            || !memoria.tabla_alias.is_empty();
        memoria.exportacion_seq = exportacion_seq;
        memoria.ultima_exportacion_previa = exportado_ahora;
        for c in memoria.correcciones.iter_mut() {
            if !c.exportada {
                c.exportada = true;
            }
        }
        if tiene_actividad_ia {
            guardar_memoria_en(ruta, &memoria)?;
        }

        (destino.to_string_lossy().to_string(), instancia_id_uso)
    };

    if let Some(r_uso) = ruta_uso {
        confirmar_uso_exportado_en(r_uso, &instancia_id_confirmar)?;
    }

    Ok(destino_str)
}

pub fn exportar_aprendizaje_completo_en(
    ruta: &Path,
    ruta_uso: Option<&Path>,
    carpeta_archivos: Option<&Path>,
    carpeta_destino: &Path,
    juzgados_usuario: &[String],
    tipos_usuario: &[String],
) -> Result<String, String> {
    exportar_aprendizaje_completo_con_priors_en(
        ruta,
        ruta_uso,
        carpeta_archivos,
        None,
        carpeta_destino,
        juzgados_usuario,
        tipos_usuario,
        &[],
    )
}

pub fn exportar_aprendizaje_con_catalogos_en(
    ruta: &Path,
    carpeta_destino: &Path,
    juzgados_usuario: &[String],
    tipos_usuario: &[String],
) -> Result<String, String> {
    exportar_aprendizaje_completo_en(
        ruta,
        None,
        None,
        carpeta_destino,
        juzgados_usuario,
        tipos_usuario,
    )
}

pub fn exportar_aprendizaje_en(ruta: &Path, carpeta_destino: &Path) -> Result<String, String> {
    exportar_aprendizaje_con_catalogos_en(ruta, carpeta_destino, &[], &[])
}

// ---------------------------------------------------------------------------
// Sincronización bidireccional e idempotente entre dispositivos ("anti ping-pong")
// ---------------------------------------------------------------------------

pub fn memoria_tiene_actividad(m: &MemoriaAprendizaje) -> bool {
    !m.reglas.is_empty()
        || !m.correcciones.is_empty()
        || !m.ejemplos_aciertos.is_empty()
        || !m.tabla_alias.is_empty()
        || !m.perfiles_sinteticos.is_empty()
        || !m.consolidaciones_ia.is_empty()
        || m.aciertos > 0
        || m.llamadas_extraccion_total > 0
        || m.llamadas_induccion_llm_total > 0
        || m.llamadas_consolidacion_total > 0
        || !m.contadores_por_instancia.is_empty()
        || !m.metricas_por_defensor.is_empty()
        || !m.metricas_campos.is_empty()
        || !m.metricas_modelos.is_empty()
        || !m.preferencias_por_defensor.is_empty()
        || m.preferencias_estilo != PreferenciasEstilo::default()
}

fn elegir_par_beta_idempotente(
    da: f64,
    db: f64,
    oa: f64,
    ob: f64,
    pa: f64,
    pb: f64,
) -> (f64, f64) {
    let dist_d = (da - pa).abs() + (db - pb).abs();
    let dist_o = (oa - pa).abs() + (ob - pb).abs();
    let ord_d = (dist_d, da, db);
    let ord_o = (dist_o, oa, ob);
    if ord_o > ord_d {
        (oa, ob)
    } else {
        (da, db)
    }
}

fn fusionar_metricas_campo_idempotente(dest: &mut MetricasCampo, otra: &MetricasCampo) -> bool {
    let antes = dest.clone();
    if dest.campo.trim().is_empty() && !otra.campo.trim().is_empty() {
        dest.campo = otra.campo.clone();
    }
    let (nueva_ewma, nueva_ts) = if otra.ultima_actualizacion > dest.ultima_actualizacion
        || (otra.ultima_actualizacion == dest.ultima_actualizacion
            && otra.evaluaciones > dest.evaluaciones)
    {
        (otra.precision_ewma, otra.ultima_actualizacion.clone())
    } else {
        (dest.precision_ewma, dest.ultima_actualizacion.clone())
    };
    dest.evaluaciones = dest.evaluaciones.max(otra.evaluaciones);
    dest.aciertos = dest.aciertos.max(otra.aciertos);
    dest.correcciones = dest.correcciones.max(otra.correcciones);
    let (na, nb) = elegir_par_beta_idempotente(
        dest.alpha,
        dest.beta,
        otra.alpha,
        otra.beta,
        PRIOR_BETA_ALPHA,
        PRIOR_BETA_BETA,
    );
    dest.alpha = na;
    dest.beta = nb;
    dest.precision_ewma = nueva_ewma;
    dest.ultima_actualizacion = nueva_ts;
    *dest != antes
}

fn fusionar_preferencias_idempotente(dest: &mut PreferenciasEstilo, otra: &PreferenciasEstilo) -> bool {
    let antes = dest.clone();
    let def = PreferenciasEstilo::default();
    let (a, b) = elegir_par_beta_idempotente(
        dest.incluir_causas_secundarias_alpha,
        dest.incluir_causas_secundarias_beta,
        otra.incluir_causas_secundarias_alpha,
        otra.incluir_causas_secundarias_beta,
        def.incluir_causas_secundarias_alpha,
        def.incluir_causas_secundarias_beta,
    );
    dest.incluir_causas_secundarias_alpha = a;
    dest.incluir_causas_secundarias_beta = b;

    let (a, b) = elegir_par_beta_idempotente(
        dest.incluir_reprogramacion_alpha,
        dest.incluir_reprogramacion_beta,
        otra.incluir_reprogramacion_alpha,
        otra.incluir_reprogramacion_beta,
        def.incluir_reprogramacion_alpha,
        def.incluir_reprogramacion_beta,
    );
    dest.incluir_reprogramacion_alpha = a;
    dest.incluir_reprogramacion_beta = b;

    let (a, b) = elegir_par_beta_idempotente(
        dest.se_fija_aplazamiento_alpha,
        dest.se_fija_aplazamiento_beta,
        otra.se_fija_aplazamiento_alpha,
        otra.se_fija_aplazamiento_beta,
        def.se_fija_aplazamiento_alpha,
        def.se_fija_aplazamiento_beta,
    );
    dest.se_fija_aplazamiento_alpha = a;
    dest.se_fija_aplazamiento_beta = b;

    let (a, b) = elegir_par_beta_idempotente(
        dest.instalada_reprogramada_si_alpha,
        dest.instalada_reprogramada_si_beta,
        otra.instalada_reprogramada_si_alpha,
        otra.instalada_reprogramada_si_beta,
        def.instalada_reprogramada_si_alpha,
        def.instalada_reprogramada_si_beta,
    );
    dest.instalada_reprogramada_si_alpha = a;
    dest.instalada_reprogramada_si_beta = b;
    *dest != antes
}

fn fusionar_regla_idempotente(dest: &mut ReglaAprendida, otra: &ReglaAprendida) -> bool {
    let antes = dest.clone();
    dest.refuerzos = dest.refuerzos.max(otra.refuerzos);
    let (na, nb) = elegir_par_beta_idempotente(
        dest.alpha,
        dest.beta,
        otra.alpha,
        otra.beta,
        PRIOR_BETA_ALPHA,
        PRIOR_BETA_BETA,
    );
    dest.alpha = na;
    dest.beta = nb;
    dest.impresiones = dest.impresiones.max(otra.impresiones);
    dest.aciertos_directos = dest.aciertos_directos.max(otra.aciertos_directos);
    dest.fallos_directos = dest.fallos_directos.max(otra.fallos_directos);
    dest.aciertos_sombra = dest.aciertos_sombra.max(otra.aciertos_sombra);
    dest.confirmaciones_sombra = dest.confirmaciones_sombra.max(otra.confirmaciones_sombra);

    let ts_dest = if !dest.ultima_actualizacion.is_empty() {
        dest.ultima_actualizacion.as_str()
    } else {
        dest.actualizada.as_str()
    };
    let ts_otra = if !otra.ultima_actualizacion.is_empty() {
        otra.ultima_actualizacion.as_str()
    } else {
        otra.actualizada.as_str()
    };

    let prio_d = prioridad_estado_regla(&dest.estado);
    let prio_o = prioridad_estado_regla(&otra.estado);
    if (ts_otra, prio_o) > (ts_dest, prio_d) && !otra.estado.trim().is_empty() {
        dest.estado = otra.estado.clone();
    }
    if ((ts_otra > ts_dest && !otra.regla.trim().is_empty())
        || (ts_otra == ts_dest && otra.regla.len() > dest.regla.len()))
        && otra.regla.len() <= 240
    {
        dest.regla = otra.regla.clone();
    }

    let mut triggers_set: BTreeSet<String> = dest.triggers.iter().cloned().collect();
    for t in &otra.triggers {
        if triggers_set.len() >= 12 {
            break;
        }
        if !t.trim().is_empty() && triggers_set.insert(t.clone()) {
            dest.triggers.push(t.clone());
        }
    }
    let mut conf_set: BTreeSet<String> = dest
        .defensores_confirmantes
        .iter()
        .map(|d| normalizar_clave_defensor(d))
        .filter(|d| !d.is_empty())
        .collect();
    for d in &otra.defensores_confirmantes {
        let dk = normalizar_clave_defensor(d);
        if !dk.is_empty() && conf_set.insert(dk.clone()) {
            dest.defensores_confirmantes.push(dk);
        }
    }
    if otra.ultima_actualizacion > dest.ultima_actualizacion {
        dest.ultima_actualizacion = otra.ultima_actualizacion.clone();
    }
    if otra.actualizada > dest.actualizada {
        dest.actualizada = otra.actualizada.clone();
    }
    dest.confianza_llm = dest.confianza_llm.max(otra.confianza_llm);
    if dest.origen_correccion_id.is_empty() && !otra.origen_correccion_id.is_empty() {
        dest.origen_correccion_id = otra.origen_correccion_id.clone();
    }
    *dest != antes
}

fn rango_estado_correccion(estado: &str) -> u8 {
    match estado {
        "analizada" | "descartada" => 3,
        "esperando_aclaracion" => 2,
        "pendiente" => 1,
        _ => 0,
    }
}

fn fusionar_correccion_idempotente(dest: &mut CorreccionIa, otra: &CorreccionIa) -> bool {
    let mut cambio = false;
    let clave_dest = (
        rango_estado_correccion(&dest.estado),
        !dest.nota_usuario.trim().is_empty(),
        !dest.regla_generada.trim().is_empty(),
        !dest.regla_id_asociada.is_empty(),
    );
    let clave_otra = (
        rango_estado_correccion(&otra.estado),
        !otra.nota_usuario.trim().is_empty(),
        !otra.regla_generada.trim().is_empty(),
        !otra.regla_id_asociada.is_empty(),
    );
    if clave_otra > clave_dest {
        dest.estado = otra.estado.clone();
        dest.pregunta_ia = otra.pregunta_ia.clone();
        cambio = true;
    }
    if dest.nota_usuario.trim().is_empty() && !otra.nota_usuario.trim().is_empty() {
        dest.nota_usuario = otra.nota_usuario.clone();
        cambio = true;
    }
    if dest.regla_generada.trim().is_empty() && !otra.regla_generada.trim().is_empty() {
        dest.regla_generada = otra.regla_generada.clone();
        cambio = true;
    }
    if dest.regla_id_asociada.is_empty() && !otra.regla_id_asociada.is_empty() {
        dest.regla_id_asociada = otra.regla_id_asociada.clone();
        cambio = true;
    }
    if dest.defensor_clave.trim().is_empty() && !otra.defensor_clave.trim().is_empty() {
        dest.defensor_clave = otra.defensor_clave.clone();
        cambio = true;
    }
    cambio
}

fn fusionar_estado_consolidacion_idempotente(
    dest: &mut EstadoConsolidacion,
    otra: &EstadoConsolidacion,
) -> bool {
    let antes = dest.clone();
    if otra.ultima_consolidacion > dest.ultima_consolidacion {
        dest.presion = otra.presion;
        dest.correcciones_sin_consolidar = otra.correcciones_sin_consolidar;
        dest.notas_sin_consolidar = otra.notas_sin_consolidar;
        dest.extracciones_desde_ultima = otra.extracciones_desde_ultima;
        dest.ultima_consolidacion = otra.ultima_consolidacion.clone();
    } else if otra.ultima_consolidacion == dest.ultima_consolidacion {
        dest.presion = dest.presion.max(otra.presion);
        dest.correcciones_sin_consolidar = dest
            .correcciones_sin_consolidar
            .max(otra.correcciones_sin_consolidar);
        dest.notas_sin_consolidar = dest.notas_sin_consolidar.max(otra.notas_sin_consolidar);
        dest.extracciones_desde_ultima = dest
            .extracciones_desde_ultima
            .max(otra.extracciones_desde_ultima);
    }
    for id in &otra.ids_consolidadas_recientes {
        if !dest.ids_consolidadas_recientes.contains(id) {
            dest.ids_consolidadas_recientes.push(id.clone());
        }
    }
    dest.ids_consolidadas_recientes.sort();
    dest.ids_consolidadas_recientes.dedup();
    if dest.ids_consolidadas_recientes.len() > 200 {
        let ex = dest.ids_consolidadas_recientes.len() - 200;
        dest.ids_consolidadas_recientes.drain(0..ex);
    }
    *dest != antes
}

fn clave_cluster_sync(c: &ClusterCorreccion) -> (String, String, String) {
    (
        c.defensor_clave.clone(),
        c.campo_principal.clone(),
        c.firma_error.clone(),
    )
}

fn rango_estado_cluster(s: &str) -> u8 {
    match s {
        "inducido" => 3,
        "listo" => 2,
        "acumulando" => 1,
        _ => 0,
    }
}

/// Fusión estrictamente idempotente y conmutativa entre la memoria local y una memoria remota
/// (`merge(A, A) == A` y `merge(merge(A, B), A) == merge(A, B)`).
pub fn fusionar_memoria_ia_structs(
    dest: &mut MemoriaAprendizaje,
    incoming: &MemoriaAprendizaje,
) -> ResultadoFusionIa {
    migrar_memoria_v1_a_v2(dest);
    let mut remota = incoming.clone();
    migrar_memoria_v1_a_v2(&mut remota);

    let mut res = ResultadoFusionIa::default();
    let mut estado_interno_cambio = false;

    // 1. Identidad de instancia y G-Counters por instancia (anti ping-pong)
    let id_local = dest.instancia_id.clone();
    if memoria_tiene_actividad(&remota) {
        let mut vistas: BTreeSet<String> = dest.instancias_sincronizadas.iter().cloned().collect();
        if !remota.instancia_id.is_empty()
            && remota.instancia_id != id_local
            && vistas.insert(remota.instancia_id.clone())
        {
            dest.instancias_sincronizadas.push(remota.instancia_id.clone());
        }
        for s in &remota.instancias_sincronizadas {
            if !s.is_empty() && s != &id_local && vistas.insert(s.clone()) {
                dest.instancias_sincronizadas.push(s.clone());
            }
        }
        dest.instancias_sincronizadas.sort();
        dest.instancias_sincronizadas.dedup();
    }

    for (inst_id, cnt_in) in &remota.contadores_por_instancia {
        if inst_id.trim().is_empty() {
            continue;
        }
        let entry = dest
            .contadores_por_instancia
            .entry(inst_id.clone())
            .or_default();
        let antes = entry.clone();
        entry.aciertos = entry.aciertos.max(cnt_in.aciertos);
        entry.llamadas_extraccion = entry.llamadas_extraccion.max(cnt_in.llamadas_extraccion);
        entry.llamadas_induccion = entry.llamadas_induccion.max(cnt_in.llamadas_induccion);
        entry.llamadas_consolidacion = entry
            .llamadas_consolidacion
            .max(cnt_in.llamadas_consolidacion);
        if *entry != antes {
            estado_interno_cambio = true;
        }
    }
    recalcular_totales_desde_contadores(dest);

    // 2. Directorio de defensores (unión conservando el nombre más completo)
    for (_, nombre) in &remota.nombres_defensores {
        let clave = normalizar_clave_defensor(nombre);
        if clave.is_empty() {
            continue;
        }
        let existia = dest.nombres_defensores.contains_key(&clave);
        let antes = dest.nombres_defensores.get(&clave).cloned();
        registrar_nombre_defensor(&mut dest.nombres_defensores, nombre);
        if !existia {
            res.defensores_nuevos += 1;
        } else if dest.nombres_defensores.get(&clave) != antes.as_ref() {
            estado_interno_cambio = true;
        }
    }

    // 3. Reglas aprendidas
    for r in &remota.reglas {
        let mut pos = if !r.id.trim().is_empty() {
            dest.reglas.iter().position(|x| x.id == r.id)
        } else {
            None
        };
        if pos.is_none() && !r.id.starts_with("base-") {
            pos = dest.reglas.iter().position(|x| {
                !x.id.starts_with("base-")
                    && x.defensor_alcance == r.defensor_alcance
                    && x.campo_objetivo == r.campo_objetivo
                    && x.tipo_error == r.tipo_error
                    && es_misma_regla(&x.regla, &r.regla)
            });
        }
        if let Some(idx) = pos {
            if fusionar_regla_idempotente(&mut dest.reglas[idx], r) {
                res.reglas_actualizadas += 1;
            }
        } else {
            dest.reglas.push(r.clone());
            res.reglas_nuevas += 1;
        }
    }
    if dest.reglas.len() > MAX_REGLAS {
        dest.reglas.sort_by(|a, b| {
            let a_base = a.id.starts_with("base-");
            let b_base = b.id.starts_with("base-");
            b_base
                .cmp(&a_base)
                .then_with(|| {
                    prioridad_estado_regla(&b.estado).cmp(&prioridad_estado_regla(&a.estado))
                })
                .then_with(|| {
                    (b.alpha - b.beta)
                        .partial_cmp(&(a.alpha - a.beta))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.ultima_actualizacion.cmp(&a.ultima_actualizacion))
                .then_with(|| a.id.cmp(&b.id))
        });
        dest.reglas.truncate(MAX_REGLAS);
    }

    // 4. Correcciones de IA
    for c in &remota.correcciones {
        if let Some(idx) = dest
            .correcciones
            .iter()
            .position(|x| !c.id.is_empty() && x.id == c.id)
        {
            if fusionar_correccion_idempotente(&mut dest.correcciones[idx], c) {
                estado_interno_cambio = true;
            }
        } else {
            dest.correcciones.push(c.clone());
            res.correcciones_nuevas += 1;
        }
    }
    dest.correcciones
        .sort_by(|a, b| a.registrada.cmp(&b.registrada).then_with(|| a.id.cmp(&b.id)));
    if dest.correcciones.len() > MAX_CORRECCIONES {
        let ex = dest.correcciones.len() - MAX_CORRECCIONES;
        dest.correcciones.drain(0..ex);
    }

    // 5. Ejemplos positivos ancla
    for e in &remota.ejemplos_aciertos {
        let ya_esta = !e.id.is_empty() && dest.ejemplos_aciertos.iter().any(|x| x.id == e.id);
        if !ya_esta {
            dest.ejemplos_aciertos.push(e.clone());
            res.ejemplos_nuevos += 1;
        }
    }
    dest.ejemplos_aciertos
        .sort_by(|a, b| a.registrada.cmp(&b.registrada).then_with(|| a.id.cmp(&b.id)));
    if dest.ejemplos_aciertos.len() > MAX_ACIERTOS_EJEMPLOS {
        let ex = dest.ejemplos_aciertos.len() - MAX_ACIERTOS_EJEMPLOS;
        dest.ejemplos_aciertos.drain(0..ex);
    }

    // 6. Tabla de alias estadísticos
    for a in &remota.tabla_alias {
        let ent_norm = crate::estadistica_ia::canonizar_entrada_alias(&a.campo, &a.entrada_norm);
        let can_norm = normalizar_texto_sin_tildes(&a.canonico);
        if let Some(exist) = dest.tabla_alias.iter_mut().find(|x| {
            x.campo == a.campo
                && crate::estadistica_ia::canonizar_entrada_alias(&x.campo, &x.entrada_norm)
                    == ent_norm
                && normalizar_texto_sin_tildes(&x.canonico) == can_norm
                && x.defensor_alcance == a.defensor_alcance
        }) {
            let antes = exist.clone();
            let (na, nb) = elegir_par_beta_idempotente(
                exist.alpha,
                exist.beta,
                a.alpha,
                a.beta,
                PRIOR_ALIAS_ALPHA,
                PRIOR_ALIAS_BETA,
            );
            exist.alpha = na;
            exist.beta = nb;
            for d in &a.defensores_confirmantes {
                let dk = normalizar_clave_defensor(d);
                if !dk.is_empty() && !exist.defensores_confirmantes.contains(&dk) {
                    exist.defensores_confirmantes.push(dk);
                }
            }
            if a.ultima_actualizacion > exist.ultima_actualizacion {
                exist.ultima_actualizacion = a.ultima_actualizacion.clone();
            }
            if *exist != antes {
                res.alias_actualizados += 1;
            }
        } else {
            dest.tabla_alias.push(a.clone());
            res.alias_nuevos += 1;
        }
    }

    // 7. Perfiles sintéticos (mayor versión -> fecha más reciente -> resumen más completo)
    for (k, p) in &remota.perfiles_sinteticos {
        let amb = alcance_canonico(k);
        if p.resumen.trim().is_empty() {
            continue;
        }
        match dest.perfiles_sinteticos.get(&amb) {
            Some(actual)
                if (actual.version, &actual.ultima_actualizacion, actual.resumen.len())
                    >= (p.version, &p.ultima_actualizacion, p.resumen.len()) => {}
            _ => {
                let mut nuevo = p.clone();
                nuevo.ambito = amb.clone();
                dest.perfiles_sinteticos.insert(amb, nuevo);
                res.perfiles_actualizados += 1;
            }
        }
    }

    // 8. Historial de consolidaciones IA
    for co in &remota.consolidaciones_ia {
        if !co.id.is_empty() && !dest.consolidaciones_ia.iter().any(|x| x.id == co.id) {
            dest.consolidaciones_ia.push(co.clone());
            res.consolidaciones_nuevas += 1;
        }
    }
    dest.consolidaciones_ia
        .sort_by(|a, b| a.fecha.cmp(&b.fecha).then_with(|| a.id.cmp(&b.id)));
    if dest.consolidaciones_ia.len() > MAX_CONSOLIDACIONES_HISTORIAL {
        let ex = dest.consolidaciones_ia.len() - MAX_CONSOLIDACIONES_HISTORIAL;
        dest.consolidaciones_ia.drain(0..ex);
    }

    // 9. Métricas (por campo, modelo y defensor) sin inflación
    for (campo, mc_in) in &remota.metricas_campos {
        match dest.metricas_campos.get_mut(campo) {
            Some(mc) => {
                if fusionar_metricas_campo_idempotente(mc, mc_in) {
                    estado_interno_cambio = true;
                }
            }
            None => {
                dest.metricas_campos.insert(campo.clone(), mc_in.clone());
                estado_interno_cambio = true;
            }
        }
    }
    for (clave, mm_in) in &remota.metricas_modelos {
        match dest.metricas_modelos.get_mut(clave) {
            Some(mm) => {
                let antes = mm.clone();
                mm.evaluaciones = mm.evaluaciones.max(mm_in.evaluaciones);
                mm.aciertos = mm.aciertos.max(mm_in.aciertos);
                mm.correcciones = mm.correcciones.max(mm_in.correcciones);
                for (campo, mc_in) in &mm_in.por_campo {
                    match mm.por_campo.get_mut(campo) {
                        Some(mc) => {
                            fusionar_metricas_campo_idempotente(mc, mc_in);
                        }
                        None => {
                            mm.por_campo.insert(campo.clone(), mc_in.clone());
                        }
                    }
                }
                if *mm != antes {
                    estado_interno_cambio = true;
                }
            }
            None => {
                dest.metricas_modelos.insert(clave.clone(), mm_in.clone());
                estado_interno_cambio = true;
            }
        }
    }
    for (clave, md_in) in &remota.metricas_por_defensor {
        let dk = normalizar_clave_defensor(clave);
        if dk.is_empty() {
            continue;
        }
        match dest.metricas_por_defensor.get_mut(&dk) {
            Some(md) => {
                let antes = md.clone();
                if !md_in.nombre_mostrar.trim().is_empty()
                    && (md.nombre_mostrar.trim().is_empty()
                        || es_nombre_defensor_mas_informativo(
                            &md_in.nombre_mostrar,
                            &md.nombre_mostrar,
                        ))
                {
                    md.nombre_mostrar = md_in.nombre_mostrar.clone();
                }
                md.aciertos = md.aciertos.max(md_in.aciertos);
                md.correcciones = md.correcciones.max(md_in.correcciones);
                for (campo, mc_in) in &md_in.por_campo {
                    match md.por_campo.get_mut(campo) {
                        Some(mc) => {
                            fusionar_metricas_campo_idempotente(mc, mc_in);
                        }
                        None => {
                            md.por_campo.insert(campo.clone(), mc_in.clone());
                        }
                    }
                }
                if *md != antes {
                    estado_interno_cambio = true;
                }
            }
            None => {
                dest.metricas_por_defensor.insert(dk, md_in.clone());
                estado_interno_cambio = true;
            }
        }
    }

    // 10. Preferencias de estilo y estados de consolidación / cola / Page-Hinkley
    if fusionar_preferencias_idempotente(&mut dest.preferencias_estilo, &remota.preferencias_estilo)
    {
        estado_interno_cambio = true;
    }
    for (k, pref_in) in &remota.preferencias_por_defensor {
        let dk = normalizar_clave_defensor(k);
        if dk.is_empty() {
            continue;
        }
        match dest.preferencias_por_defensor.get_mut(&dk) {
            Some(p) => {
                if fusionar_preferencias_idempotente(p, pref_in) {
                    estado_interno_cambio = true;
                }
            }
            None => {
                dest.preferencias_por_defensor.insert(dk, pref_in.clone());
                estado_interno_cambio = true;
            }
        }
    }

    for (k, est_in) in &remota.estados_consolidacion {
        let amb = alcance_canonico(k);
        match dest.estados_consolidacion.get_mut(&amb) {
            Some(est) => {
                if fusionar_estado_consolidacion_idempotente(est, est_in) {
                    estado_interno_cambio = true;
                }
            }
            None => {
                dest.estados_consolidacion.insert(amb, est_in.clone());
                estado_interno_cambio = true;
            }
        }
    }

    for clu in &remota.cola_induccion {
        let pos = dest
            .cola_induccion
            .iter()
            .position(|x| x.id == clu.id || clave_cluster_sync(x) == clave_cluster_sync(clu));
        if let Some(idx) = pos {
            let d = &mut dest.cola_induccion[idx];
            let antes = d.clone();
            for cid in &clu.correccion_ids {
                if !d.correccion_ids.contains(cid) {
                    d.correccion_ids.push(cid.clone());
                }
            }
            d.correccion_ids.sort();
            d.correccion_ids.dedup();
            if d.primera_vez.is_empty()
                || (!clu.primera_vez.is_empty() && clu.primera_vez < d.primera_vez)
            {
                d.primera_vez = clu.primera_vez.clone();
            }
            if clu.ultima_vez > d.ultima_vez {
                d.ultima_vez = clu.ultima_vez.clone();
            }
            if rango_estado_cluster(&clu.estado) > rango_estado_cluster(&d.estado) {
                d.estado = clu.estado.clone();
            }
            if d.regla_inducida_id.is_empty() && !clu.regla_inducida_id.is_empty() {
                d.regla_inducida_id = clu.regla_inducida_id.clone();
            }
            for def_p in &clu.defensores_presentes {
                let dk = normalizar_clave_defensor(def_p);
                if !dk.is_empty() && !d.defensores_presentes.contains(&dk) {
                    d.defensores_presentes.push(dk);
                }
            }
            if *d != antes {
                estado_interno_cambio = true;
            }
        } else {
            dest.cola_induccion.push(clu.clone());
            estado_interno_cambio = true;
        }
    }
    if dest.cola_induccion.len() > MAX_CLUSTERS_COLA {
        dest.cola_induccion.sort_by(|a, b| {
            rango_estado_cluster(&a.estado)
                .cmp(&rango_estado_cluster(&b.estado))
                .then_with(|| b.ultima_vez.cmp(&a.ultima_vez))
        });
        dest.cola_induccion.truncate(MAX_CLUSTERS_COLA);
    }

    if (
        remota.estado_aclaracion.alpha_resp,
        remota.estado_aclaracion.beta_omit,
    ) > (
        dest.estado_aclaracion.alpha_resp,
        dest.estado_aclaracion.beta_omit,
    ) {
        dest.estado_aclaracion.alpha_resp = remota.estado_aclaracion.alpha_resp;
        dest.estado_aclaracion.beta_omit = remota.estado_aclaracion.beta_omit;
        estado_interno_cambio = true;
    }
    for (firma, turno) in &remota.estado_aclaracion.firmas_explicadas_recientes {
        if let Some((_, t_d)) = dest
            .estado_aclaracion
            .firmas_explicadas_recientes
            .iter_mut()
            .find(|(f, _)| f == firma)
        {
            if *turno > *t_d {
                *t_d = *turno;
                estado_interno_cambio = true;
            }
        } else {
            dest.estado_aclaracion
                .firmas_explicadas_recientes
                .push((firma.clone(), *turno));
            estado_interno_cambio = true;
        }
    }

    for (k, ph_in) in &remota.page_hinkley_campos {
        match dest.page_hinkley_campos.get(k) {
            Some(act) if (act.n_obs, &act.ultimo_drift) >= (ph_in.n_obs, &ph_in.ultimo_drift) => {
            }
            _ => {
                dest.page_hinkley_campos.insert(k.clone(), ph_in.clone());
                estado_interno_cambio = true;
            }
        }
    }
    for (m_k, map_in) in &remota.page_hinkley_modelos {
        let dest_map = dest.page_hinkley_modelos.entry(m_k.clone()).or_default();
        for (c_k, ph_in) in map_in {
            match dest_map.get(c_k) {
                Some(act)
                    if (act.n_obs, &act.ultimo_drift) >= (ph_in.n_obs, &ph_in.ultimo_drift) =>
                {
                }
                _ => {
                    dest_map.insert(c_k.clone(), ph_in.clone());
                    estado_interno_cambio = true;
                }
            }
        }
    }

    let cambios_visibles = res.reglas_nuevas
        + res.reglas_actualizadas
        + res.correcciones_nuevas
        + res.ejemplos_nuevos
        + res.alias_nuevos
        + res.alias_actualizados
        + res.perfiles_actualizados
        + res.consolidaciones_nuevas
        + res.defensores_nuevos;
    res.cambios_totales = cambios_visibles + usize::from(estado_interno_cambio && cambios_visibles == 0);
    res
}

/// Prepara un snapshot normalizado y con `instancia_id` estable para exportar en `PaqueteLokus`.
pub fn preparar_snapshot_memoria_para_paquete_en(
    ruta: &Path,
) -> Result<MemoriaAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let existia_en_disco = ruta.exists();
    let raw_sin_inst = if existia_en_disco {
        fs::read_to_string(ruta)
            .ok()
            .and_then(|s| serde_json::from_str::<MemoriaAprendizaje>(&s).ok())
            .is_some_and(|m| m.instancia_id.trim().is_empty())
    } else {
        false
    };
    let mut memoria = cargar_memoria_en(ruta);
    asegurar_instancia_y_contadores(&mut memoria);
    if raw_sin_inst || (!existia_en_disco && memoria_tiene_actividad(&memoria)) {
        guardar_memoria_en(ruta, &memoria)?;
    }
    Ok(memoria)
}

pub fn inspeccionar_fusion_memoria_ia_en(
    ruta: &Path,
    incoming: &MemoriaAprendizaje,
) -> InspeccionFusionIa {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut copia = cargar_memoria_en(ruta);
    let res = fusionar_memoria_ia_structs(&mut copia, incoming);
    InspeccionFusionIa {
        reglas_nuevas: res.reglas_nuevas,
        reglas_actualizables: res.reglas_actualizadas,
        correcciones_nuevas: res.correcciones_nuevas,
        ejemplos_nuevos: res.ejemplos_nuevos,
        alias_nuevos: res.alias_nuevos + res.alias_actualizados,
        perfiles_nuevos_o_actualizados: res.perfiles_actualizados,
        consolidaciones_nuevas: res.consolidaciones_nuevas,
        defensores_nuevos: res.defensores_nuevos,
        ya_sincronizado: res.cambios_totales == 0,
    }
}

pub fn fusionar_memoria_ia_desde_paquete_en(
    ruta: &Path,
    incoming: &MemoriaAprendizaje,
) -> Result<ResultadoFusionIa, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let res = fusionar_memoria_ia_structs(&mut memoria, incoming);
    if res.cambios_totales > 0 || (!ruta.exists() && memoria_tiene_actividad(&memoria)) {
        guardar_memoria_en(ruta, &memoria)?;
    }
    Ok(res)
}

/// Convierte un archivo técnico legado (`lokus_aprendizaje_ia_*.json` exportado antes de la sincronización bidireccional)
/// en una `MemoriaAprendizaje` compatible con la fusión idempotente entre equipos.
pub fn memoria_desde_paquete_exportacion(paq: &PaqueteExportacion) -> MemoriaAprendizaje {
    let mut mem = MemoriaAprendizaje::default();
    let inst_id = if !paq.instancia_id.trim().is_empty() {
        paq.instancia_id.trim().to_string()
    } else if !paq.exportacion_id.trim().is_empty() {
        format!("legacy-export-{}", paq.exportacion_id.trim())
    } else {
        "legacy-export".to_string()
    };

    mem.instancia_id = inst_id.clone();
    mem.instancias_sincronizadas = paq
        .fuentes_importadas
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != inst_id)
        .map(str::to_string)
        .collect();
    mem.contadores_por_instancia.insert(
        inst_id,
        ContadoresInstanciaIa {
            aciertos: paq.aciertos,
            llamadas_extraccion: paq.llamadas_extraccion_total,
            llamadas_induccion: paq.llamadas_induccion_llm_total,
            llamadas_consolidacion: paq.llamadas_consolidacion_total,
        },
    );
    mem.aciertos = paq.aciertos;
    mem.llamadas_extraccion_total = paq.llamadas_extraccion_total;
    mem.llamadas_induccion_llm_total = paq.llamadas_induccion_llm_total;
    mem.llamadas_consolidacion_total = paq.llamadas_consolidacion_total;

    mem.reglas = paq.reglas.clone();
    mem.correcciones = paq.correcciones.clone();
    mem.ejemplos_aciertos = paq.ejemplos_aciertos.clone();
    mem.consolidaciones_ia = paq.consolidaciones_ia.clone();

    mem.tabla_alias = if !paq.tabla_alias_raw.is_empty() {
        paq.tabla_alias_raw.clone()
    } else {
        paq.alias_validados
            .iter()
            .map(|a| AliasEstadistico {
                campo: a.campo.clone(),
                entrada_norm: a.entrada_norm.clone(),
                canonico: a.canonico.clone(),
                alpha: 3.0,
                beta: PRIOR_ALIAS_BETA,
                defensor_alcance: alcance_canonico(&a.defensor_alcance),
                defensores_confirmantes: Vec::new(),
                ultima_actualizacion: paq.exportado.clone(),
            })
            .collect()
    };

    if let Some(pref) = &paq.preferencias_raw {
        mem.preferencias_estilo = pref.clone();
    }
    mem.preferencias_por_defensor = paq.preferencias_por_defensor_raw.clone();

    for p in &paq.perfiles_sinteticos {
        let amb = alcance_canonico(&p.ambito);
        if !p.resumen.trim().is_empty() {
            let mut copy = p.clone();
            copy.ambito = amb.clone();
            mem.perfiles_sinteticos.insert(amb, copy);
        }
    }

    let campo_diag_a_metrica = |c: &CampoDiagnostico| -> MetricasCampo {
        let mut alpha = PRIOR_BETA_ALPHA + (c.aciertos as f64).min(30.0);
        let mut beta = PRIOR_BETA_BETA + (c.correcciones as f64).min(30.0);
        normalizar_ess(&mut alpha, &mut beta);
        MetricasCampo {
            campo: c.campo.clone(),
            evaluaciones: c.evaluaciones,
            aciertos: c.aciertos,
            correcciones: c.correcciones,
            alpha,
            beta,
            precision_ewma: c.precision_ewma,
            ultima_actualizacion: paq.exportado.clone(),
        }
    };

    for c in &paq.metricas_por_campo {
        mem.metricas_campos
            .insert(c.campo.clone(), campo_diag_a_metrica(c));
    }

    for mm in &paq.metricas_por_modelo {
        let (prov, mod_nom) = mm
            .clave
            .split_once(':')
            .map(|(p, m)| (p.to_string(), m.to_string()))
            .unwrap_or_else(|| (String::new(), mm.clave.clone()));
        mem.metricas_modelos.insert(
            mm.clave.clone(),
            MetricasModelo {
                clave: mm.clave.clone(),
                proveedor: prov,
                modelo: mod_nom,
                evaluaciones: mm.evaluaciones,
                aciertos: mm.aciertos,
                correcciones: mm.correcciones,
                por_campo: BTreeMap::new(),
            },
        );
    }

    for d in &paq.defensores_directorio {
        let dk = normalizar_clave_defensor(&d.nombre);
        if dk.is_empty() {
            continue;
        }
        registrar_nombre_defensor(&mut mem.nombres_defensores, &d.nombre);
        let por_campo = d
            .por_campo
            .iter()
            .map(|c| (c.campo.clone(), campo_diag_a_metrica(c)))
            .collect();
        mem.metricas_por_defensor.insert(
            dk,
            MetricasDefensor {
                nombre_mostrar: d.nombre.clone(),
                aciertos: d.aciertos,
                correcciones: d.correcciones,
                por_campo,
            },
        );
    }

    migrar_memoria_v1_a_v2(&mut mem);
    mem
}

// ---------------------------------------------------------------------------
// API pública usada por `ai.rs` y `lib.rs`
// ---------------------------------------------------------------------------

pub fn obtener_contexto_prompt_con_traza_y_defensor(
    texto_nuevo: &str,
    defensor: Option<&str>,
) -> (String, Vec<String>, Vec<String>) {
    let estado = estado_desde_settings(&cargar_settings());
    let modelo_actual = format!("{}:{}", estado.provider.trim(), estado.model.trim());
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let local = cargar_memoria_en(&ruta_memoria());
    formatear_contexto_prompt_con_detalle_para_defensor(
        texto_nuevo,
        memoria_base_embebida(),
        &local,
        Some(&modelo_actual),
        defensor,
    )
}

pub fn obtener_contexto_prompt_con_traza(texto_nuevo: &str) -> (String, Vec<String>, Vec<String>) {
    obtener_contexto_prompt_con_traza_y_defensor(texto_nuevo, None)
}

pub fn obtener_contexto_prompt(texto_nuevo: &str) -> String {
    obtener_contexto_prompt_con_traza(texto_nuevo).0
}

#[allow(clippy::too_many_arguments)]
pub fn registrar_resultado_ia_completo_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    reglas_inyectadas: &[String],
    defensor: Option<&str>,
    prediccion_local: Option<&PrediccionLocalResumen>,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    let estado = estado_desde_settings(&cargar_settings());
    registrar_resultado_completo_en(
        ruta,
        texto,
        prediccion,
        final_usuario,
        &estado.provider,
        &estado.model,
        reglas_inyectadas,
        defensor,
        prediccion_local,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn registrar_resultado_ia_completo(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    reglas_inyectadas: &[String],
    defensor: Option<&str>,
    prediccion_local: Option<&PrediccionLocalResumen>,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_ia_completo_en(
        &ruta_memoria(),
        texto,
        prediccion,
        final_usuario,
        reglas_inyectadas,
        defensor,
        prediccion_local,
    )
}

pub fn registrar_resultado_ia_con_traza_y_defensor(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    reglas_inyectadas: &[String],
    defensor: Option<&str>,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_ia_completo(
        texto,
        prediccion,
        final_usuario,
        reglas_inyectadas,
        defensor,
        None,
    )
}

pub fn registrar_resultado_ia_con_traza(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    reglas_inyectadas: &[String],
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_ia_con_traza_y_defensor(texto, prediccion, final_usuario, reglas_inyectadas, None)
}

pub fn registrar_resultado_ia(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_ia_con_traza(texto, prediccion, final_usuario, &[])
}

pub fn obtener_resumen_aprendizaje() -> ResumenAprendizaje {
    let settings = cargar_settings();
    let uso = asegurar_uso_inicializado_actual();
    let base = {
        let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
        resumen_de(&cargar_memoria_en(&ruta_memoria()), false)
    };
    combinar_resumen_con_uso(base, &uso, &settings)
}

pub fn obtener_diagnostico_aprendizaje() -> DiagnosticoAprendizaje {
    let settings = cargar_settings();
    let priors = cargar_priors_actuales();
    let mut diag = {
        let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
        let memoria = cargar_memoria_en(&ruta_memoria());
        construir_diagnostico_desde_memoria(&memoria, Some(&priors), &settings.defensores)
    };
    diag.conexiones_grafo = priors.grafo.total_conexiones();
    diag
}

pub fn obtener_cluster_listo_para_inducir_ia() -> Option<(String, Vec<CorreccionIa>)> {
    obtener_cluster_listo_para_inducir_en(&ruta_memoria())
}

pub fn aplicar_analisis_cluster_ia(
    cluster_id: &str,
    correccion_ids: &[String],
    analisis: &AnalisisCorreccionIa,
) -> Result<ResumenAprendizaje, String> {
    aplicar_analisis_cluster_en(&ruta_memoria(), cluster_id, correccion_ids, analisis)
}

pub fn exportar_aprendizaje_ia() -> Result<String, String> {
    let settings = cargar_settings();
    let ruta_uso = ruta_uso_catalogos();
    let carpeta = obtener_carpeta_archivos();
    let priors = cargar_priors_actuales();
    exportar_aprendizaje_completo_con_priors_en(
        &ruta_memoria(),
        Some(&ruta_uso),
        Some(&carpeta),
        Some(&priors),
        &carpeta_descargas()?,
        &settings.juzgados,
        &settings.tipos_audiencia,
        &settings.defensores,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audiencia_base() -> AudienciaInput {
        AudienciaInput {
            radicado: "JUAN PÉREZ GÓMEZ - 200016001075202400123".to_string(),
            tipo: "Audiencia concentrada".to_string(),
            fecha: "15/08/2025".to_string(),
            hora: "14:30".to_string(),
            juzgado: "Juzgado 2 Penal del Circuito".to_string(),
            realizada: "SI".to_string(),
            motivo: String::new(),
            observaciones: "Se realizó sin novedad".to_string(),
        }
    }

    fn preparar_dir(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lokus_aprendizaje_{}_{}",
            nombre,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crear carpeta temporal de prueba");
        dir
    }

    #[test]
    fn test_sin_cambios_no_genera_correccion() {
        let a = audiencia_base();
        assert!(detectar_correcciones(&a, &a.clone()).is_empty());
    }

    #[test]
    fn test_cambio_realizada_y_motivo_detecta_ambos_campos() {
        let pred = audiencia_base();
        let mut fin = audiencia_base();
        fin.realizada = "NO".to_string();
        fin.motivo = "Fiscalía".to_string();

        let campos = detectar_correcciones(&pred, &fin);
        let nombres: Vec<&str> = campos.iter().map(|c| c.campo.as_str()).collect();
        assert_eq!(nombres, vec!["realizada", "motivo"]);
        assert_eq!(campos[0].valor_ia, "SI");
        assert_eq!(campos[0].valor_usuario, "NO");
        assert_eq!(campos[1].valor_usuario, "Fiscalía");
    }

    #[test]
    fn test_juzgado_solo_mayusculas_o_espacios_no_cuenta() {
        let pred = audiencia_base();
        let mut fin = audiencia_base();
        fin.juzgado = "  JUZGADO 2   PENAL DEL CIRCUITO ".to_string();
        assert!(detectar_correcciones(&pred, &fin).is_empty());

        fin.juzgado = "Juzgado 2 Penal del CTO".to_string();
        let campos = detectar_correcciones(&pred, &fin);
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].campo, "juzgado");
    }

    #[test]
    fn test_observaciones_retoque_no_cuenta_y_reescritura_si() {
        let mut pred = audiencia_base();
        pred.observaciones =
            "La fiscal no se conectó a la audiencia virtual y se reprogramó para septiembre".into();

        let mut retoque = pred.clone();
        retoque.observaciones =
            "La fiscal no se conectó a la audiencia virtual, y se reprogramó para septiembre.".into();
        assert!(detectar_correcciones(&pred, &retoque).is_empty());

        let mut reescrita = pred.clone();
        reescrita.observaciones = "Fiscal ausente. Nueva fecha: 10/09/2025".into();
        let campos = detectar_correcciones(&pred, &reescrita);
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].campo, "observaciones");
    }

    #[test]
    fn test_anonimiza_nombre_y_radicado_conservando_fechas() {
        let mut pred = audiencia_base();
        pred.observaciones = "Juan Perez no compareció".to_string();
        let correccion = CorreccionIa {
            id: "1".into(),
            registrada: "2025-08-15T10:00:00-05:00".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            defensor_clave: String::new(),
            texto_original: "Audiencia de JUAN PÉREZ GÓMEZ rad 200016001075202400123 del 15-08-2025 a las 14:30. Pérez no asistió. Cel 3001234567".into(),
            prediccion: pred.clone(),
            final_usuario: audiencia_base(),
            campos: vec![CampoCorregido {
                campo: "radicado".into(),
                valor_ia: "JUAN PÉREZ GÓMEZ - 200016001075202400123".into(),
                valor_usuario: "JUAN PÉREZ GÓMEZ - 200016001075202400123".into(),
            }],
            estado: estado_pendiente(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
        };

        let anon = anonimizar_correccion(&correccion);
        let texto = &anon.texto_original;
        assert!(!texto.to_lowercase().contains("juan"), "{}", texto);
        assert!(!texto.to_lowercase().contains("pérez"), "{}", texto);
        assert!(!texto.contains("200016001075202400123"), "{}", texto);
        assert!(!texto.contains("3001234567"), "{}", texto);
        assert!(texto.contains("15-08-2025"), "{}", texto);
        assert!(texto.contains("14:30"), "{}", texto);
        assert!(texto.contains("[PROCESADO]"), "{}", texto);
        assert_eq!(anon.prediccion.radicado, "[PROCESADO] - [RADICADO]");
        assert!(!anon.prediccion.observaciones.to_lowercase().contains("perez"));
        assert_eq!(anon.campos[0].valor_usuario, "[PROCESADO] - [RADICADO]");
        assert_eq!(anon.final_usuario.fecha, "15/08/2025");
    }

    #[test]
    fn test_registrar_acierto_correccion_y_exportar() {
        let dir = preparar_dir("registro_exportacion");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let destino = dir.join("descargas");
        let pred = audiencia_base();

        // Sin correcciones aún: exportar debe fallar con mensaje claro
        assert!(exportar_aprendizaje_en(&ruta, &destino).is_err());

        let (r1, c1) = registrar_resultado_en(&ruta, "texto", &pred, &pred.clone(), "openai", "m")
            .expect("registrar acierto");
        assert_eq!(r1.aciertos, 1);
        assert_eq!(r1.total_correcciones, 0);
        assert!(!r1.hubo_correccion);
        assert!(c1.is_none());

        let mut fin = pred.clone();
        fin.realizada = "NO".into();
        fin.motivo = "Juez".into();
        let (r2, c2) = registrar_resultado_en(&ruta, "texto 2", &pred, &fin, "openai", "m")
            .expect("registrar corrección");
        assert!(r2.hubo_correccion);
        assert_eq!(r2.total_correcciones, 1);
        assert_eq!(r2.pendientes_exportar, 1);
        assert!(c2.is_some());

        let memoria = cargar_memoria_en(&ruta);
        assert_eq!(memoria.correcciones[0].estado, "pendiente");
        assert_eq!(memoria.correcciones[0].campos.len(), 2);

        let archivo = exportar_aprendizaje_en(&ruta, &destino).expect("exportar");
        let contenido = fs::read_to_string(&archivo).expect("leer exportación");
        assert!(!contenido.contains("200016001075202400123"));
        assert!(contenido.contains("\"correcciones_por_campo\""));

        let tras_exportar = cargar_memoria_en(&ruta);
        assert_eq!(resumen_de(&tras_exportar, false).pendientes_exportar, 0);
        assert_eq!(tras_exportar.correcciones.len(), 1, "exportar no borra lo aprendido");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_tope_de_correcciones_descarta_las_mas_antiguas() {
        let dir = preparar_dir("tope");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();

        let mut memoria = MemoriaAprendizaje::default();
        for i in 0..MAX_CORRECCIONES {
            memoria.correcciones.push(CorreccionIa {
                id: format!("viejo-{}", i),
                registrada: String::new(),
                proveedor: String::new(),
                modelo: String::new(),
                defensor_clave: String::new(),
                texto_original: "t".into(),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: Vec::new(),
                estado: estado_pendiente(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: true,
            });
        }
        guardar_memoria_en(&ruta, &memoria).expect("guardar memoria inicial");

        let mut fin = pred.clone();
        fin.fecha = "16/08/2025".into();
        let (r, _) = registrar_resultado_en(&ruta, "nuevo", &pred, &fin, "gemini", "m")
            .expect("registrar");
        assert_eq!(r.total_correcciones, MAX_CORRECCIONES);

        let final_mem = cargar_memoria_en(&ruta);
        assert_eq!(final_mem.correcciones[0].id, "viejo-1");
        assert_eq!(
            final_mem.correcciones.last().map(|c| c.texto_original.as_str()),
            Some("nuevo")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_archivo_danado_se_respalda_y_empieza_de_cero() {
        let dir = preparar_dir("danado");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        fs::write(&ruta, "{ esto no es json").expect("escribir archivo dañado");

        let memoria = cargar_memoria_en(&ruta);
        assert_eq!(memoria.correcciones.len(), 0);
        assert!(dir.join("ia_aprendizaje.json.bak").exists());

        let pred = audiencia_base();
        assert!(registrar_resultado_en(&ruta, "t", &pred, &pred.clone(), "openai", "m").is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_seleccion_devuelve_las_3_correcciones_mas_parecidas() {
        let base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();
        let pred = audiencia_base();

        let textos = [
            ("c1", "Audiencia preparatoria en Juzgado 12 CTO 3 por hurto calificado"),
            ("c2", "Audiencia de acusación ante Juzgado 4 Penal Municipal"),
            ("c3", "Juzgado 12 CTO 3 aplaza audiencia preparatoria por inasistencia fiscal"),
            ("c4", "Lectura de fallo en proceso de alimentos ante juzgado de familia"),
            ("c5", "En el Juzgado 12 CTO 3 se instaló audiencia preparatoria sin novedad"),
            ("c6", "Control de garantías por captura en flagrancia en URI"),
            ("c7", "Incidente de reparación integral ante juez promiscuo municipal"),
            ("c8", "Sustitución de medida de aseguramiento solicitada por defensa"),
            ("c9", "Preclusión por atipicidad en proceso contravencional"),
            ("c10", "Verificación de allanamiento a cargos ante tribunal superior"),
        ];

        for (id, t) in textos {
            local.correcciones.push(CorreccionIa {
                id: id.to_string(),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_original: t.to_string(),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: vec![CampoCorregido {
                    campo: "juzgado".into(),
                    valor_ia: "Juzgado 12 Circuito 3".into(),
                    valor_usuario: "Juzgado 12 CTO 3".into(),
                }],
                estado: "analizada".into(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: false,
            });
        }

        let consulta = "Citacion a audiencia preparatoria en el Juzgado 12 CTO 3";
        let (_, elegidas) = seleccionar_contexto_para_prompt(consulta, &base, &local);
        assert_eq!(elegidas.len(), 3);
        let ids: Vec<&str> = elegidas.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"c1"), "ids={:?}", ids);
        assert!(ids.contains(&"c3"), "ids={:?}", ids);
        assert!(ids.contains(&"c5"), "ids={:?}", ids);
    }

    #[test]
    fn test_prompt_respeta_limite_maximo_5_reglas_y_3_ejemplos() {
        let base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();
        let pred = audiencia_base();

        for i in 1..=8 {
            agregar_o_reforzar_regla(
                &mut local,
                "abreviacion",
                &format!("Regla diferenciada número {} sobre siglas judiciales especificas {}", i, i * 17),
            );
        }
        assert_eq!(local.reglas.len(), 8);

        for i in 1..=6 {
            local.correcciones.push(CorreccionIa {
                id: format!("ej-{}", i),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_original: format!("Audiencia concentrada fiscalia ausente caso {}", i),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: vec![CampoCorregido {
                    campo: "motivo".into(),
                    valor_ia: "Juez".into(),
                    valor_usuario: "Fiscalía".into(),
                }],
                estado: "analizada".into(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: false,
            });
        }

        let (reglas, ejemplos) = seleccionar_contexto_para_prompt(
            "Audiencia concentrada fiscalia ausente",
            &base,
            &local,
        );
        assert_eq!(reglas.len(), MAX_REGLAS_PROMPT);
        assert_eq!(ejemplos.len(), MAX_EJEMPLOS_PROMPT);

        let bloque = formatear_contexto_prompt(
            "Audiencia concentrada fiscalia ausente",
            &base,
            &local,
        );
        assert!(bloque.contains("APRENDIZAJE DE CORRECCIONES PREVIAS"));
        assert_eq!(bloque.matches("- [abreviacion]").count(), 5);
        assert_eq!(bloque.matches("- Texto similar:").count(), 3);
    }

    #[test]
    fn test_aplicar_analisis_refuerza_regla_aclaracion_y_descarta_no_error() {
        let dir = preparar_dir("analisis_ciclo");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();
        let mut fin = pred.clone();
        fin.juzgado = "Juzgado 12 CTO 3".into();

        let (res_r1, c1) = registrar_resultado_en(
            &ruta,
            "Audiencia en Juzgado 12 CTO 3",
            &pred,
            &fin,
            "openai",
            "m",
        )
        .expect("registrar c1");
        let id1 = c1.expect("c1").id;

        // 1) Motor local decide aclaración inmediatamente en c1 sin consumir tokens LLM
        assert!(res_r1.aclaracion_pendiente.is_some());
        assert_eq!(res_r1.total_reglas, 0);

        // El usuario responde la aclaración -> NO inserta regla cruda activa; la deja en "pendiente"
        // con nota_usuario para análisis LLM posterior (0 huérfanas con triggers vacíos)
        let (res_nota, _) = guardar_nota_aclaracion_en(
            &ruta,
            &id1,
            "Dejar CTO abreviado siempre en el nombre del juzgado",
        )
        .expect("guardar nota");
        assert!(res_nota.aclaracion_pendiente.is_none());
        assert_eq!(res_nota.total_reglas, 0);

        // Luego el análisis LLM con la nota crea la regla en estado "prueba" y con triggers poblados
        let res_con_nota = aplicar_analisis_en(
            &ruta,
            &id1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Dejar CTO abreviado siempre en el nombre del juzgado".into(),
                confianza: 0.92,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .expect("aplicar analisis tras nota");
        assert_eq!(res_con_nota.total_reglas, 1);
        let mem_reg1 = cargar_memoria_en(&ruta);
        assert_eq!(mem_reg1.reglas[0].estado, "prueba");
        assert!(!mem_reg1.reglas[0].triggers.is_empty());

        // 2) Segunda corrección con la misma regla -> refuerza la regla existente en lugar de duplicarla
        let (_, c2) = registrar_resultado_en(
            &ruta,
            "Otra audiencia en Juzgado 5 CTO 1",
            &pred,
            &fin,
            "openai",
            "m",
        )
        .expect("registrar c2");
        let id2 = c2.expect("c2").id;
        let res_ref = aplicar_analisis_en(
            &ruta,
            &id2,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Dejar CTO abreviado siempre en el nombre del juzgado".into(),
                confianza: 0.95,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .expect("reforzar regla");
        assert_eq!(res_ref.total_reglas, 1);
        let mem = cargar_memoria_en(&ruta);
        assert_eq!(mem.reglas[0].refuerzos, 2);

        // 3) Cambio que no fue error de la IA -> se marca descartada y no agrega regla
        let (_, c3) = registrar_resultado_en(&ruta, "Texto 3", &pred, &fin, "openai", "m")
            .expect("registrar c3");
        let id3 = c3.expect("c3").id;
        aplicar_analisis_en(
            &ruta,
            &id3,
            &AnalisisCorreccionIa {
                es_error_de_ia: false,
                tipo_error: "otro".into(),
                regla: "No debería guardarse".into(),
                confianza: 0.9,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .expect("descartar no error");
        let mem_final = cargar_memoria_en(&ruta);
        assert_eq!(mem_final.reglas.len(), 1);
        assert_eq!(
            mem_final
                .correcciones
                .iter()
                .find(|c| c.id == id3)
                .map(|c| c.estado.as_str()),
            Some("descartada")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase0_saneamiento_tildes_y_estados_prompt() {
        let mut pred = audiencia_base();
        let mut fin = audiencia_base();
        fin.radicado = "JUAN PEREZ GOMEZ - 200016001075202400123".into();
        fin.tipo = "Audiencia de acusacion".into();
        pred.tipo = "Audiencia de acusación".into();
        fin.juzgado = "Juzgado 2 Penal del Circuito".into();
        fin.motivo = "Victima".into();
        pred.motivo = "Víctima".into();
        assert!(detectar_correcciones(&pred, &fin).is_empty());

        let base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();
        for (id, estado) in [
            ("p1", "pendiente"),
            ("e1", "esperando_aclaracion"),
            ("d1", "descartada"),
            ("a1", "analizada"),
        ] {
            local.correcciones.push(CorreccionIa {
                id: id.into(),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_original: "Audiencia preparatoria en Juzgado 12 CTO 3".into(),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: vec![CampoCorregido {
                    campo: "juzgado".into(),
                    valor_ia: "Juzgado 12 Circuito 3".into(),
                    valor_usuario: "Juzgado 12 CTO 3".into(),
                }],
                estado: estado.into(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: false,
            });
        }
        let (_, ejemplos) = seleccionar_contexto_para_prompt(
            "Audiencia preparatoria en Juzgado 12 CTO 3",
            &base,
            &local,
        );
        assert_eq!(ejemplos.len(), 1);
        assert_eq!(ejemplos[0].id, "a1");
    }

    #[test]
    fn test_fase0_max_reglas_no_elimina_regla_nueva_y_no_duplica_aclaracion() {
        let mut mem = MemoriaAprendizaje::default();
        for i in 0..MAX_REGLAS {
            mem.reglas.push(construir_regla_v2(
                format!("r-{}", i),
                "motivo".into(),
                format!("Regla consolidada numero {} para pruebas de desalojo", i),
                5,
                "2025-01-01T00:00:00-05:00".into(),
                "activa",
                "motivo".into(),
                vec!["consolidada".into()],
                "mitigacion_modelo",
                "*".into(),
                0.9,
                String::new(),
            ));
        }
        let nueva_id = agregar_o_reforzar_regla_con_id(
            &mut mem,
            "abreviacion",
            "Conservar sigla MPAL intacta en el juzgado",
        )
        .expect("debe insertar nueva regla");
        assert_eq!(mem.reglas.len(), MAX_REGLAS);
        assert!(mem.reglas.iter().any(|r| r.id == nueva_id));

        // Verificar que nota de aclaración + re-análisis posterior no duplica regla
        let dir = preparar_dir("no_doble_regla");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();
        let mut fin = pred.clone();
        fin.juzgado = "Juzgado 3 MPAL".into();
        let (res_acl, corr) = registrar_resultado_en(
            &ruta,
            "Audiencia en Juzgado 3 MPAL",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        assert!(res_acl.aclaracion_pendiente.is_some());
        let cid = corr.unwrap().id;
        let (res_nota, _) =
            guardar_nota_aclaracion_en(&ruta, &cid, "Dejar MPAL sin expandir").unwrap();
        assert!(res_nota.aclaracion_pendiente.is_none());
        assert_eq!(res_nota.total_reglas, 0);

        let res_primero = aplicar_analisis_en(
            &ruta,
            &cid,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar MPAL en el juzgado".into(),
                confianza: 0.85,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        assert_eq!(res_primero.total_reglas, 1);

        let res_refinado = aplicar_analisis_en(
            &ruta,
            &cid,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Mantener MPAL abreviado en el nombre del despacho".into(),
                confianza: 0.92,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        assert_eq!(res_refinado.total_reglas, 1);
        let mem_despues = cargar_memoria_en(&ruta);
        assert_eq!(mem_despues.reglas.len(), 1);
        assert_eq!(mem_despues.reglas[0].refuerzos, 1);
        assert_eq!(mem_despues.reglas[0].estado, "prueba");
        assert!(mem_despues.reglas[0].regla.contains("MPAL"));
        assert!(!mem_despues.reglas[0].triggers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase0_anonimiza_reglas_y_numeros_de_6_digitos() {
        let dir = preparar_dir("anon_reglas_6dig");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let destino = dir.join("descargas");
        let mut pred = audiencia_base();
        pred.radicado = "CARLOS MENDOZA - 123456".into();
        let mut fin = pred.clone();
        fin.juzgado = "Juzgado 1 Penal".into();

        registrar_resultado_en(
            &ruta,
            "Audiencia de Carlos Mendoza radicado 123456 del 15-08-2025",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();

        let mut mem = cargar_memoria_en(&ruta);
        agregar_o_reforzar_regla(
            &mut mem,
            "radicado",
            "En procesos de Carlos Mendoza con número 123456 y radicado 654321 conservar formato",
        );
        guardar_memoria_en(&ruta, &mem).unwrap();

        let archivo = exportar_aprendizaje_en(&ruta, &destino).expect("exportar");
        let contenido = fs::read_to_string(&archivo).expect("leer exportacion");
        assert!(!contenido.to_lowercase().contains("carlos"), "{}", contenido);
        assert!(!contenido.to_lowercase().contains("mendoza"), "{}", contenido);
        assert!(!contenido.contains("123456"), "{}", contenido);
        assert!(!contenido.contains("654321"), "{}", contenido);
        assert!(contenido.contains("15-08-2025"), "{}", contenido);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase0_harness_prequential_linea_base() {
        // Banco sintético realista anonimizado para evaluación offline prequential (test-then-train)
        let dir = preparar_dir("harness_prequential_f0");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let casos = [
            ("JUNIO 24.- JUZGADO 3 CTO PENAL [PROCESADO] FRACSA POR PROBLEMAS DE CONEXION", "NO", "Usuario"),
            ("Audiencia preparatoria [PROCESADO] [RADICADO]. La fiscal no se conectó y por ello solicité reprogramar.", "NO", "Fiscalía"),
            ("Control de legalidad [PROCESADO]. Abogado contractual no compareció.", "NO", "Defensor Confianza"),
            ("Audiencia concentrada [PROCESADO]. No pude contactar al procesado y pedí aplazamiento.", "NO", "Defensor Público"),
            ("Juicio oral [PROCESADO]. INPEC no trasladó al detenido.", "NO", "INPEC"),
            ("Acusación [PROCESADO]. Se realizó sin novedad y se fija fecha para preparatoria.", "SI", ""),
            ("Lectura de fallo [PROCESADO]. Se efectuó correctamente.", "SI", ""),
            ("Preparatoria [PROCESADO]. El juez entró en periodo de vacaciones.", "NO", "Juez"),
            ("Incidente [PROCESADO]. Víctima no compareció a la diligencia.", "NO", "Víctima"),
            ("Audiencia preliminar [PROCESADO]. Representante ICBF ausente.", "NO", "ICBF"),
        ];

        let mut total_llamadas_esperadas_sin_cluster = 0usize;
        for ronda in 0..2 {
            for (idx, (texto, real_esp, mot_esp)) in casos.iter().enumerate() {
                let mem_actual = cargar_memoria_en(&ruta);
                let det = seleccionar_contexto_detallado(texto, memoria_base_embebida(), &mem_actual);
                assert!(det.reglas.len() <= MAX_REGLAS_PROMPT);

                let mut gt = audiencia_base();
                gt.realizada = (*real_esp).into();
                gt.motivo = (*mot_esp).into();
                let mut pred = gt.clone();
                if ronda == 0 || idx % 2 == 0 {
                    // Simula fallo inicial en realizada/motivo para ejercitar clustering y métricas
                    pred.realizada = if *real_esp == "SI" { "NO".into() } else { "SI".into() };
                    pred.motivo = String::new();
                    total_llamadas_esperadas_sin_cluster += 1;
                }

                registrar_resultado_con_traza_en(
                    &ruta,
                    texto,
                    &pred,
                    &gt,
                    "openai",
                    "gpt-5-mini",
                    &det.reglas_inyectadas_ids,
                )
                .unwrap();

                if let Some((cid, corrs)) = obtener_cluster_listo_para_inducir_en(&ruta) {
                    let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
                    aplicar_analisis_cluster_en(
                        &ruta,
                        &cid,
                        &ids,
                        &AnalisisCorreccionIa {
                            es_error_de_ia: true,
                            tipo_error: "realizacion".into(),
                            regla: format!("Patrón judicial prequential {}", cid),
                            confianza: 0.90,
                            necesita_aclaracion: false,
                            pregunta: String::new(),
                        },
                    )
                    .unwrap();
                }
            }
        }

        let mem_fin = cargar_memoria_en(&ruta);
        assert_eq!(mem_fin.metricas_campos.len(), CAMPOS_AUDIENCIA.len());
        assert!(mem_fin.ejemplos_aciertos.len() >= 4);
        assert!(
            mem_fin.llamadas_induccion_llm_total as f64
                <= (total_llamadas_esperadas_sin_cluster as f64) * 0.60,
            "llamadas={} sin_cluster={}",
            mem_fin.llamadas_induccion_llm_total,
            total_llamadas_esperadas_sin_cluster
        );

        // Verificar también anonimización sin espacios alrededor de '-' y salida de cuarentena por histéresis
        let anon_sin_espacio = Anonimizador::desde_radicados(&["CARLOS MENDOZA-200016001075"]);
        let txt_anon = anon_sin_espacio.aplicar("Audiencia de Carlos Mendoza radicado 200016001075");
        assert!(!txt_anon.to_lowercase().contains("carlos"), "{}", txt_anon);
        assert!(!txt_anon.to_lowercase().contains("mendoza"), "{}", txt_anon);
        assert!(!txt_anon.contains("200016001075"), "{}", txt_anon);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cuarentena_histeresis_y_reactivacion_regla_retirada() {
        let dir = preparar_dir("cuarentena_histeresis");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let mut mem = MemoriaAprendizaje::default();
        mem.reglas.push(construir_regla_v2(
            "r-cuar".into(),
            "motivo".into(),
            "Regla sobre fiscalia ausente en diligencia".into(),
            2,
            Local::now().to_rfc3339(),
            "cuarentena",
            "motivo".into(),
            vec!["fiscalia".into(), "ausente".into()],
            "mitigacion_modelo",
            "*".into(),
            0.85,
            String::new(),
        ));
        guardar_memoria_en(&ruta, &mem).unwrap();

        let mut pred = audiencia_base();
        pred.realizada = "NO".into();
        pred.motivo = "Usuario".into();
        let mut fin = pred.clone();
        fin.motivo = "Fiscalía".into();

        // Si el LLM falla 3 veces en el patrón mientras la regla está en cuarentena -> rehabilita a "prueba"
        for _ in 0..3 {
            registrar_resultado_en(
                &ruta,
                "Fiscalia ausente en sala virtual",
                &pred,
                &fin,
                "openai",
                "gpt-5-mini",
            )
            .unwrap();
        }
        let mem2 = cargar_memoria_en(&ruta);
        assert_eq!(mem2.reglas[0].estado, "prueba");

        // Si una regla está "retirada" pero se vuelve a inducir por es_misma_regla -> se reactiva
        let mut mem3 = mem2;
        mem3.reglas[0].estado = "retirada".into();
        let rid = agregar_regla_interna(
            &mut mem3,
            "motivo",
            "Regla sobre fiscalia ausente en diligencia",
            "prueba",
            Some("motivo"),
            &["fiscalia".into()],
            "mitigacion_modelo",
            "*",
            0.9,
            "",
        )
        .unwrap();
        assert_eq!(rid, "r-cuar");
        assert_eq!(mem3.reglas[0].estado, "prueba");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase1_migracion_v1_a_v2_sin_perdida() {
        let dir = preparar_dir("migracion_v1_v2");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let json_v1 = r#"{
          "version": 1,
          "aciertos": 4,
          "reglas": [
            {
              "id": "r-v1-1",
              "tipo_error": "abreviacion",
              "regla": "No expandir MPAL en el nombre del juzgado",
              "refuerzos": 3,
              "actualizada": "2025-05-01T10:00:00-05:00"
            },
            {
              "id": "r-v1-n1",
              "tipo_error": "motivo",
              "regla": "Regla aislada de una sola vez",
              "refuerzos": 1,
              "actualizada": "2025-05-02T10:00:00-05:00"
            }
          ],
          "correcciones": []
        }"#;
        fs::write(&ruta, json_v1).unwrap();

        let mem = cargar_memoria_en(&ruta);
        assert_eq!(mem.version, VERSION_FORMATO);
        assert_eq!(mem.aciertos, 4);
        assert_eq!(mem.reglas.len(), 2);
        assert_eq!(mem.reglas[0].estado, "activa");
        assert_eq!(mem.reglas[0].campo_objetivo, "juzgado");
        assert!(mem.reglas[0].alpha >= 4.0);
        assert!(!mem.reglas[0].triggers.is_empty());
        assert_eq!(mem.reglas[1].estado, "candidata");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase1_bayes_wilson_decaimiento_y_n1_candidata_con_sombra() {
        // 1) Beta cuantil q0.10 y Wilson
        let q_prior = limite_inferior_bayesiano(1.0, 3.0);
        let q_fuerte = limite_inferior_bayesiano(15.0, 3.0);
        assert!(q_prior < 0.10, "prior q10={}", q_prior);
        assert!(q_fuerte > 0.70, "fuerte q10={}", q_fuerte);
        assert!(limite_inferior_wilson(9.0, 10.0, 1.645) > 0.65);

        // 2) Vacancia judicial (60 días < 180 días) NO borra aprendizaje
        let mut a = 10.0;
        let mut b = 3.0;
        let hace_60d = (Local::now() - chrono::Duration::days(60)).to_rfc3339();
        aplicar_decaimiento_beta(&mut a, &mut b, &hace_60d);
        assert!(a > 9.0, "no debe olvidarse tras 60 días de vacancia: a={}", a);

        // 3) N=1 corrección aislada crea 'candidata' y NO entra al prompt activo
        let dir = preparar_dir("sombra_credito");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();
        let mut fin = pred.clone();
        fin.juzgado = "Juzgado 4 PCTO".into();

        let (_, corr1) = registrar_resultado_en(
            &ruta,
            "Audiencia en Juzgado 4 PCTO de Valledupar",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let id1 = corr1.unwrap().id;
        aplicar_analisis_en(
            &ruta,
            &id1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar abreviacion PCTO tal cual en juzgado".into(),
                confianza: 0.9,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();

        let mem1 = cargar_memoria_en(&ruta);
        assert_eq!(mem1.reglas[0].estado, "candidata");
        let base_vacia = MemoriaAprendizaje::default();
        let (sel1, _) = seleccionar_contexto_para_prompt(
            "Audiencia en Juzgado 4 PCTO",
            &base_vacia,
            &mem1,
        );
        assert!(sel1.is_empty(), "regla N=1 candidata NO debe inyectarse como activa");

        // 4) Evaluación en sombra: si el LLM vuelve a fallar en PCTO sin la regla, promueve candidata -> prueba!
        let _ = registrar_resultado_en(
            &ruta,
            "Otra audiencia en Juzgado 2 PCTO",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let mem2 = cargar_memoria_en(&ruta);
        assert_eq!(mem2.reglas[0].estado, "prueba");
        assert_eq!(mem2.reglas[0].confirmaciones_sombra, 1);

        // 5) Atribución causal + acierto positivo ancla (escéptico Beta(1,3) exige >= 3 aciertos directos para superar q0.10 >= 0.42)
        let rid = mem2.reglas[0].id.clone();
        for i in 0..3 {
            let _ = registrar_resultado_con_traza_en(
                &ruta,
                &format!("Audiencia exitosa en Juzgado 4 PCTO caso {}", i),
                &fin,
                &fin,
                "openai",
                "gpt-5-mini",
                std::slice::from_ref(&rid),
            )
            .unwrap();
        }
        let mem3 = cargar_memoria_en(&ruta);
        assert_eq!(mem3.reglas[0].estado, "activa");
        assert_eq!(mem3.reglas[0].aciertos_directos, 3);
        assert_eq!(mem3.ejemplos_aciertos.len(), 3);
        assert!(!mem3.ejemplos_aciertos[0].texto_anonimizado.contains("JUAN"));

        // 6) Cuarentena cuando la regla falla repetidamente tras >= 5 impresiones
        for i in 0..6 {
            let _ = registrar_resultado_con_traza_en(
                &ruta,
                &format!("Fallo con regla PCTO {}", i),
                &pred,
                &fin,
                "openai",
                "gpt-5-mini",
                std::slice::from_ref(&rid),
            )
            .unwrap();
        }
        let mem4 = cargar_memoria_en(&ruta);
        assert_eq!(mem4.reglas[0].estado, "cuarentena");

        // 7) Regla candidata cuyos triggers coinciden pero el LLM acierta >= 3 veces sin ella -> retirada
        let (_, c_ok) = registrar_resultado_en(
            &ruta,
            "Audiencia preliminar URI captura",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        aplicar_analisis_en(
            &ruta,
            &c_ok.unwrap().id,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "juzgado".into(),
                regla: "Regla tentativa URI captura".into(),
                confianza: 0.85,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        for _ in 0..3 {
            let _ = registrar_resultado_en(
                &ruta,
                "Audiencia preliminar URI captura",
                &fin,
                &fin,
                "openai",
                "gpt-5-mini",
            )
            .unwrap();
        }
        let mem5 = cargar_memoria_en(&ruta);
        let r_uri = mem5.reglas.iter().find(|r| r.regla.contains("URI")).unwrap();
        assert_eq!(r_uri.estado, "retirada");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fase1_tokenizador_judicial_y_mmr_balanceado() {
        let toks = tokenizar_judicial("Juzgado Tercero Penal no se realizó a las 2:30 PM CTO");
        assert!(toks.contains(&"3".to_string()));
        assert!(toks.contains(&"no".to_string()));
        assert!(toks.contains(&"pm".to_string()));
        assert!(toks.contains(&"cto".to_string()));

        // Verificar selección balanceada (2 correcciones + 1 acierto ancla)
        let base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();
        let pred = audiencia_base();
        for i in 0..3 {
            local.correcciones.push(CorreccionIa {
                id: format!("c-{}", i),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_original: format!("Juzgado 3 CTO Penal audiencia preparatoria caso {}", i),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: vec![CampoCorregido {
                    campo: "juzgado".into(),
                    valor_ia: "Juzgado 3 Circuito Penal".into(),
                    valor_usuario: "Juzgado 3 CTO Penal".into(),
                }],
                estado: "analizada".into(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: false,
            });
        }
        local.ejemplos_aciertos.push(EjemploPositivo {
            id: "pos-1".into(),
            registrada: String::new(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            defensor_clave: String::new(),
            texto_anonimizado: "Juzgado 3 CTO Penal audiencia preparatoria sin novedad".into(),
            salida_anonimizada: pred,
            reglas_inyectadas: Vec::new(),
        });

        let det = seleccionar_contexto_detallado(
            "Audiencia preparatoria en Juzgado 3 CTO Penal",
            &base,
            &local,
        );
        assert_eq!(det.ejemplos_correcciones.len(), 2);
        assert_eq!(det.ejemplos_aciertos.len(), 1);
    }

    #[test]
    fn test_fase3_clustering_page_hinkley_cooldown_y_reduccion_llm() {
        let dir = preparar_dir("fase3_clustering_ph");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);

        // 1) Caso aislado N=1 NO dispara inducción LLM
        let pred = audiencia_base();
        let mut fin_cto = pred.clone();
        fin_cto.juzgado = "Juzgado 3 CTO Penal".into();

        registrar_resultado_en(
            &ruta,
            "Audiencia concentrada en Juzgado 3 CTO Penal",
            &pred,
            &fin_cto,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        assert!(
            obtener_cluster_listo_para_inducir_en(&ruta).is_none(),
            "N=1 no debe disparar llamada secundaria al LLM"
        );

        // 2) Simulación de 20 correcciones repartidas en 4 clústeres recurrentes (5 c/u):
        //    antes = 20 llamadas LLM; con clúster diferido <= 4 llamadas (>= 80% reducción >= 70%).
        let dir_sim = preparar_dir("fase3_simulacion_llamadas");
        let ruta_sim = dir_sim.join(NOMBRE_ARCHIVO_MEMORIA);
        let mut patrones: Vec<(&str, &str, &str, &str, &str)> = vec![
            ("motivo", "Defensor Público", "Defensor Confianza", "Abogado contractual no compareció caso ", "Conservar Defensor Confianza ante abogado contractual"),
            ("realizada", "SI", "NO", "Audiencia fue aplazada por inasistencia fiscal caso ", "Si se aplaza por inasistencia fiscal marcar NO"),
            ("juzgado", "Juzgado 3 Circuito", "Juzgado 3 CTO", "Audiencia en Juzgado 3 CTO Penal del circuito caso ", "Mantener CTO abreviado en despachos locales"),
            ("tipo", "Otra", "Audiencia de control de legalidad", "Diligencia de control posterior garantías caso ", "Mapear control posterior a Audiencia de control de legalidad"),
        ];

        for ciclo in 0..5 {
            for (idx_p, (campo, val_ia, val_usr, prefijo_txt, regla_txt)) in
                patrones.iter_mut().enumerate()
            {
                let mut p = audiencia_base();
                let mut f = audiencia_base();
                match *campo {
                    "motivo" => {
                        p.realizada = "NO".into();
                        f.realizada = "NO".into();
                        p.motivo = (*val_ia).into();
                        f.motivo = (*val_usr).into();
                    }
                    "realizada" => {
                        p.realizada = (*val_ia).into();
                        f.realizada = (*val_usr).into();
                        f.motivo = "Fiscalía".into();
                    }
                    "juzgado" => {
                        p.juzgado = (*val_ia).into();
                        f.juzgado = (*val_usr).into();
                    }
                    _ => {
                        p.tipo = (*val_ia).into();
                        f.tipo = (*val_usr).into();
                    }
                }
                registrar_resultado_en(
                    &ruta_sim,
                    &format!("{} {} clu {}", prefijo_txt, ciclo, idx_p),
                    &p,
                    &f,
                    "openai",
                    "gpt-5-mini",
                )
                .unwrap();

                if let Some((cid, corrs)) = obtener_cluster_listo_para_inducir_en(&ruta_sim) {
                    let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
                    aplicar_analisis_cluster_en(
                        &ruta_sim,
                        &cid,
                        &ids,
                        &AnalisisCorreccionIa {
                            es_error_de_ia: true,
                            tipo_error: (*campo).into(),
                            regla: (*regla_txt).into(),
                            confianza: 0.9,
                            necesita_aclaracion: false,
                            pregunta: String::new(),
                        },
                    )
                    .unwrap();
                }
            }
        }

        let mem_sim = cargar_memoria_en(&ruta_sim);
        let llamadas_realizadas = mem_sim.llamadas_induccion_llm_total;
        assert!(
            llamadas_realizadas <= 4,
            "Se esperaban <= 4 llamadas LLM para 20 correcciones agrupadas, hubo {}",
            llamadas_realizadas
        );
        let reduccion = 1.0 - (llamadas_realizadas as f64 / 20.0);
        assert!(
            reduccion >= 0.70,
            "Reducción esperada >= 70%, obtenida={:.1}%",
            reduccion * 100.0
        );
        assert_eq!(mem_sim.reglas.len(), 4);
        assert!(mem_sim.reglas.iter().all(|r| r.estado == "prueba"));

        // 3) Detector Page-Hinkley de drift y decaimiento acelerado en reglas de ese campo
        let dir_ph = preparar_dir("fase3_page_hinkley");
        let ruta_ph = dir_ph.join(NOMBRE_ARCHIVO_MEMORIA);
        let ok = audiencia_base();
        for i in 0..10 {
            registrar_resultado_en(
                &ruta_ph,
                &format!("Audiencia correcta base {}", i),
                &ok,
                &ok,
                "openai",
                "gpt-5-mini",
            )
            .unwrap();
        }
        // Inyectamos regla existente de motivo para comprobar decaimiento selectivo ante drift
        let mut mem_ph = cargar_memoria_en(&ruta_ph);
        let mut r_motivo = construir_regla_v2(
            "r-drift-motivo".into(),
            "motivo".into(),
            "Heuristica antigua de motivo".into(),
            10,
            Local::now().to_rfc3339(),
            "activa",
            "motivo".into(),
            vec!["motivo".into()],
            "mitigacion_modelo",
            "*".into(),
            0.9,
            String::new(),
        );
        r_motivo.alpha = 15.0;
        let alpha_antes = r_motivo.alpha;
        mem_ph.reglas.push(r_motivo);
        guardar_memoria_en(&ruta_ph, &mem_ph).unwrap();

        let mut p_err = audiencia_base();
        p_err.realizada = "NO".into();
        p_err.motivo = "Usuario".into();
        let mut f_err = p_err.clone();
        f_err.motivo = "Fiscalía".into();
        for i in 0..8 {
            registrar_resultado_en(
                &ruta_ph,
                &format!("Cambio repentino formato fiscalia {}", i),
                &p_err,
                &f_err,
                "openai",
                "gpt-5-mini",
            )
            .unwrap();
        }
        let mem_ph_despues = cargar_memoria_en(&ruta_ph);
        assert!(
            mem_ph_despues
                .page_hinkley_campos
                .get("motivo")
                .is_some_and(|p| p.drift_detectado),
            "Page-Hinkley debe detectar drift en 'motivo' tras racha de fallos"
        );
        assert!(
            mem_ph_despues.reglas[0].alpha < alpha_antes - 1.0,
            "El drift debe acelerar el decaimiento de reglas afectadas"
        );

        // 4) Política local de aclaración: exclusión de fecha/hora/radicado y cooldown exponencial tras Omitir
        let dir_acl = preparar_dir("fase3_aclaracion_local");
        let ruta_acl = dir_acl.join(NOMBRE_ARCHIVO_MEMORIA);
        let mut fin_fecha = audiencia_base();
        fin_fecha.fecha = "16/08/2025".into();
        let (res_no_acl, _) = registrar_resultado_en(
            &ruta_acl,
            "Audiencia programada el 16/08/2025 en despacho",
            &audiencia_base(),
            &fin_fecha,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        assert!(
            res_no_acl.aclaracion_pendiente.is_none(),
            "No debe pedir aclaración en cambios simples de fecha"
        );

        // En cambio semántico (juzgado CTO), pide aclaración localmente -> al descartar activa cooldown exponencial
        let (res_si_acl, c_juz1) = registrar_resultado_en(
            &ruta_acl,
            "Audiencia en Juzgado 3 CTO",
            &audiencia_base(),
            &fin_cto,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let id_j1 = c_juz1.unwrap().id;
        assert!(res_si_acl.aclaracion_pendiente.is_some());
        descartar_aclaracion_en(&ruta_acl, &id_j1).unwrap();

        let (res_bloqueada, _) = registrar_resultado_en(
            &ruta_acl,
            "Otra audiencia en Juzgado 5 CTO",
            &audiencia_base(),
            &fin_cto,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        assert!(
            res_bloqueada.aclaracion_pendiente.is_none(),
            "Cooldown debe bloquear nueva aclaración inmediatamente después de Omitir"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir_sim);
        let _ = fs::remove_dir_all(&dir_ph);
        let _ = fs::remove_dir_all(&dir_acl);
    }

    #[test]
    fn test_exportacion_unificada_sin_ia_anonimiza_personalizados_y_protege_predeterminados() {
        use crate::uso_catalogos::{
            cargar_uso_sin_lock_en, registrar_delta_audiencia_en, NOMBRE_ARCHIVO_USO,
        };
        use umya_spreadsheet::{reader, writer};

        let dir = preparar_dir("export_unificada_sin_ia");
        let ruta_mem = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let ruta_uso = dir.join(NOMBRE_ARCHIVO_USO);
        let carpeta_excel = dir.join("archivos");
        let descargas = dir.join("descargas");
        fs::create_dir_all(&carpeta_excel).unwrap();

        // Excel con un procesado llamado "JUAN CARLOS CESAR" y radicado largo (sin haber usado nunca la IA)
        let ruta_xlsx = carpeta_excel.join("planilla.xlsx");
        let plantilla = include_bytes!("../../templates/plantilla_audiencias.xlsx");
        fs::write(&ruta_xlsx, plantilla).unwrap();
        let mut book = reader::xlsx::read(&ruta_xlsx).unwrap();
        let ws = book.get_sheet_mut(&0).unwrap();
        ws.get_cell_mut((1, 11)).set_value_number(1);
        ws.get_cell_mut((2, 11))
            .set_value_string("JUAN CARLOS CESAR - 200016001075202599887");
        ws.get_cell_mut((3, 11))
            .set_value_string("Audiencia de acusación");
        ws.get_cell_mut((4, 11)).set_value_string("15/08/2025");
        ws.get_cell_mut((5, 11)).set_value_string("09:00");
        ws.get_cell_mut((6, 11))
            .set_value_string("Juzgado 1 Promiscuo Municipal de San Juan del Cesar");
        ws.get_cell_mut((7, 11)).set_value_string("SI");
        writer::xlsx::write(&book, &ruta_xlsx).unwrap();

        // También un registro con texto libre "Otra" que accidentalmente menciona al procesado y un número largo
        let mut settings = AppSettings::default();
        settings.juzgados = vec!["Juzgado Especial JUAN CARLOS 200016001075202599887".into()];
        let extra = AudienciaInput {
            radicado: "JUAN CARLOS CESAR - 200016001075202599887".into(),
            tipo: "Audiencia de rebaja de pena".into(),
            fecha: "16/08/2025".into(),
            hora: "08:30".into(),
            juzgado: "Juzgado Libre CARLOS 123456".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        registrar_delta_audiencia_en(&ruta_uso, &carpeta_excel, &settings, None, Some(&extra))
            .unwrap();

        let archivo = exportar_aprendizaje_completo_en(
            &ruta_mem,
            Some(&ruta_uso),
            Some(&carpeta_excel),
            &descargas,
            &settings.juzgados,
            &settings.tipos_audiencia,
        )
        .expect("debe permitir exportar aunque nunca se haya usado la IA");

        let json_str = fs::read_to_string(&archivo).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Sin correcciones de IA previas, no debe crear innecesariamente ia_aprendizaje.json en disco
        assert!(!ruta_mem.exists());

        // El uso pendiente se reseteó a 0 sin borrar los conteos
        let uso_post = cargar_uso_sin_lock_en(&ruta_uso);
        assert_eq!(uso_post.pendientes_compartir, 0);
        assert_eq!(uso_post.total_audiencias_contabilizadas, 2);

        // Si se queda sin datos exportables (ni IA ni audiencias ni listas), el resumen nunca anuncia novedades fantasma
        let mut uso_fantasma = UsoCatalogos::default();
        uso_fantasma.pendientes_compartir = 4;
        let resumen_vacio = combinar_resumen_con_uso(
            resumen_de(&MemoriaAprendizaje::default(), false),
            &uso_fantasma,
            &AppSettings::default(),
        );
        assert!(!resumen_vacio.puede_exportar);
        assert_eq!(resumen_vacio.pendientes_exportar, 0);

        // El catálogo predeterminado oficial ("San Juan del Cesar") NO se alteró aunque el procesado se llamara Juan Cesar
        let juzgados_det = parsed["uso_catalogos"]["juzgados"]["detalle"]
            .as_array()
            .unwrap();
        let oficial = juzgados_det
            .iter()
            .find(|i| {
                i["nombre"] == "Juzgado 1 Promiscuo Municipal de San Juan del Cesar"
            })
            .expect("debe conservar intacto el catálogo oficial");
        assert_eq!(oficial["origen"], "predeterminado");
        assert_eq!(oficial["conteo"], 1);

        // En cambio, los nombres no predeterminados SÍ fueron anonimizados
        let no_pred = parsed["uso_catalogos"]["juzgados"]["nuevos_no_predeterminados"]
            .as_array()
            .unwrap();
        assert_eq!(no_pred.len(), 2);
        let texto_no_pred = serde_json::to_string(no_pred).unwrap();
        assert!(!texto_no_pred.contains("CARLOS"), "{}", texto_no_pred);
        assert!(!texto_no_pred.contains("200016001075202599887"));
        assert!(!texto_no_pred.contains("123456"));

        // Horas: 08:30 aparece en horas_mas_frecuentes_no_rapidas y 09:00 en horas_rapidas_predeterminadas
        let horas_rap = parsed["uso_catalogos"]["horas"]["horas_rapidas_predeterminadas"]
            .as_array()
            .unwrap();
        assert_eq!(horas_rap.len(), 6);
        assert!(parsed["uso_catalogos"]["horas"]["horas_mas_frecuentes_no_rapidas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["hora_24"] == "08:30" && h["conteo"] == 1));
        assert!(parsed["uso_catalogos"]["horas"]["resumen_por_franja_horaria"].is_array());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_v3_migracion_v2_pista_1ra_vez_dos_cuadernos_sombra_y_anonimizacion() {
        // 1) Migración limpia v2 -> v3 sin pérdida (reglas y alias heredan alcance "*")
        let dir_mig = preparar_dir("migracion_v2_v3");
        let ruta_mig = dir_mig.join(NOMBRE_ARCHIVO_MEMORIA);
        let json_v2 = r#"{
          "version": 2,
          "aciertos": 9,
          "reglas": [
            {
              "id": "r-v2-1",
              "tipo_error": "motivo",
              "regla": "Si la fiscal no asiste marcar Fiscalía",
              "refuerzos": 3,
              "actualizada": "2025-07-01T10:00:00-05:00",
              "estado": "activa",
              "campo_objetivo": "motivo",
              "alpha": 5.0,
              "beta": 1.0
            }
          ],
          "tabla_alias": [
            {
              "campo": "juzgado",
              "entrada_norm": "juzgado 2 circuito",
              "canonico": "Juzgado 2 CTO",
              "alpha": 4.0,
              "beta": 1.2,
              "ultima_actualizacion": "2025-07-01T10:00:00-05:00"
            }
          ],
          "correcciones": []
        }"#;
        fs::write(&ruta_mig, json_v2).unwrap();
        let mem_mig = cargar_memoria_en(&ruta_mig);
        assert_eq!(mem_mig.version, VERSION_FORMATO);
        assert_eq!(mem_mig.aciertos, 9);
        assert_eq!(mem_mig.reglas[0].defensor_alcance, "*");
        assert_eq!(mem_mig.tabla_alias[0].defensor_alcance, "*");
        let _ = fs::remove_dir_all(&dir_mig);

        // 2) Pista suave desde la 1.ª corrección pendiente en texto casi idéntico (sin contaminar otros textos ni defensores)
        let dir = preparar_dir("v3_dos_cuadernos");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();
        let mut fin_si = pred.clone();
        fin_si.realizada = "NO".into();
        fin_si.motivo = "Fiscalía".into();

        let texto_base =
            "J04 CONTROL AUDIENCIA PREPARATORIA DEL PROCESADO EN VALLEDUPAR SE REPROGRAMA POR AUSENCIA DE LA FISCALIA DELEGADA";
        registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            texto_base,
            &pred,
            &fin_si,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();

        let local_1 = cargar_memoria_en(&ruta);
        let base_vacia = MemoriaAprendizaje::default();
        let texto_casi_igual =
            "J04 CONTROL AUDIENCIA PREPARATORIA DEL PROCESADO EN VALLEDUPAR SE REPROGRAMA POR AUSENCIA DE LA FISCALIA DELEGADA NUEVA FECHA";
        let det_igual = seleccionar_contexto_detallado_para_defensor(
            texto_casi_igual,
            &base_vacia,
            &local_1,
            Some("openai:gpt-5-mini"),
            Some("Dra. Ana Pérez"),
        );
        assert!(
            det_igual.pista_pendiente.is_some(),
            "Debe emitir pista suave en texto casi idéntico del mismo defensor"
        );
        let (bloque_pista, _, _) = formatear_contexto_prompt_con_detalle_para_defensor(
            texto_casi_igual,
            &base_vacia,
            &local_1,
            Some("openai:gpt-5-mini"),
            Some("Dra. Ana Pérez"),
        );
        assert!(bloque_pista.contains("Pista reciente del defensor en texto casi idéntico"));

        // Texto distinto, fragmento diminuto u otro defensor NO reciben la pista
        let det_distinto = seleccionar_contexto_detallado_para_defensor(
            "Lectura de fallo en proceso de alimentos ante juzgado de familia en Riohacha",
            &base_vacia,
            &local_1,
            Some("openai:gpt-5-mini"),
            Some("Dra. Ana Pérez"),
        );
        assert!(det_distinto.pista_pendiente.is_none());
        let det_fragmento = seleccionar_contexto_detallado_para_defensor(
            "Audiencia preparatoria",
            &base_vacia,
            &local_1,
            Some("openai:gpt-5-mini"),
            Some("Dra. Ana Pérez"),
        );
        assert!(det_fragmento.pista_pendiente.is_none());
        let det_otro_def = seleccionar_contexto_detallado_para_defensor(
            texto_casi_igual,
            &base_vacia,
            &local_1,
            Some("openai:gpt-5-mini"),
            Some("Dr. Luis Mora"),
        );
        assert!(det_otro_def.pista_pendiente.is_none());

        // 3) Activación en 2 repeticiones con confianza = 0.60 (>= 0.55) pasa directo a "prueba" en el cuaderno de Ana
        registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            "J04 CONTROL AUDIENCIA PREPARATORIA EN VALLEDUPAR SE REPROGRAMA POR AUSENCIA FISCALIA DELEGADA CASO DOS",
            &pred,
            &fin_si,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        let (cid, corrs) = obtener_cluster_listo_para_inducir_en(&ruta)
            .expect("2 correcciones coherentes deben dejar el clúster listo");
        let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
        aplicar_analisis_cluster_en(
            &ruta,
            &cid,
            &ids,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "realizacion".into(),
                regla: "Cuando se reprograma por ausencia de fiscalia delegada marcar NO y Fiscalía"
                    .into(),
                confianza: 0.60,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();

        let mem_2 = cargar_memoria_en(&ruta);
        let regla_ana = mem_2
            .reglas
            .iter()
            .find(|r| r.regla.contains("ausencia de fiscalia delegada"))
            .expect("debe crearse la regla");
        assert_eq!(regla_ana.estado, "prueba");
        assert_eq!(regla_ana.defensor_alcance, "ana perez");

        // Aislamiento: Luis no recibe la regla privada de Ana
        let det_luis = seleccionar_contexto_detallado_para_defensor(
            "Se reprograma por ausencia de fiscalia delegada en Valledupar",
            &base_vacia,
            &mem_2,
            Some("openai:gpt-5-mini"),
            Some("Dr. Luis Mora"),
        );
        assert!(det_luis.reglas.iter().all(|r| r.id != regla_ana.id));

        // 4) Evaluación en sombra aislada: aciertos de Luis NO retiran una regla "candidata" privada de Ana
        let mut mem_sombra = mem_2.clone();
        let rid_cand = agregar_regla_interna_con_defensor(
            &mut mem_sombra,
            "observaciones",
            "Incluir frase exacta expediente digital verificado en observaciones",
            "candidata",
            Some("observaciones"),
            &["expediente".into(), "digital".into(), "verificado".into()],
            "mitigacion_modelo",
            "*",
            "Dra. Ana Pérez",
            &[],
            0.5,
            "",
        )
        .unwrap();
        guardar_memoria_en(&ruta, &mem_sombra).unwrap();
        for i in 0..4 {
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!("Audiencia con expediente digital verificado caso {}", i),
                &pred,
                &pred,
                "openai",
                "gpt-5-mini",
                &[],
                Some("Dr. Luis Mora"),
            )
            .unwrap();
        }
        let mem_tras_luis = cargar_memoria_en(&ruta);
        let cand_ana = mem_tras_luis
            .reglas
            .iter()
            .find(|r| r.id == rid_cand)
            .unwrap();
        assert_eq!(
            cand_ana.estado, "candidata",
            "Aciertos de otro defensor no deben retirar regla candidata de Ana"
        );

        // 5) Promoción automática al Cuaderno General ("*") cuando Luis confirma la misma regla que Ana
        let mut mem_promo = mem_tras_luis;
        agregar_regla_interna_con_defensor(
            &mut mem_promo,
            "realizacion",
            "Cuando se reprograma por ausencia de fiscalia delegada marcar NO y Fiscalía",
            "prueba",
            Some("realizada"),
            &["fiscalia".into(), "delegada".into()],
            "mitigacion_modelo",
            "*",
            "Dr. Luis Mora",
            &[],
            0.85,
            "",
        );
        let regla_promo = mem_promo
            .reglas
            .iter()
            .find(|r| r.id == regla_ana.id)
            .unwrap();
        assert_eq!(regla_promo.defensor_alcance, "*");
        assert_eq!(regla_promo.defensores_confirmantes.len(), 2);
        guardar_memoria_en(&ruta, &mem_promo).unwrap();

        // 6) Diagnóstico y exportación conservan nombres reales + claves unificadas de defensores
        //    (SIN DEFENSOR_1..2), manteniendo 100% anonimizados procesados, radicados y teléfonos.
        let mut pred_texto_def = pred.clone();
        pred_texto_def.radicado = "CARLOS MENDOZA - 200016001075202499999".into();
        pred_texto_def.observaciones = "Nota con Dra. Ana Pérez, Dr. Luis Mora y procesado Carlos Mendoza tel 3009876543".into();
        let mut fin_texto_def = pred_texto_def.clone();
        fin_texto_def.observaciones =
            "Defensora Ana Pérez confirmó coordinación con Defensor Mora en audiencia de Carlos Mendoza".into();
        registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            "La Dra. Ana Pérez informó que el despacho reprogramó proceso 200016001075202499999 de Carlos Mendoza cel 3009876543",
            &pred_texto_def,
            &fin_texto_def,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();

        let diag = obtener_diagnostico_aprendizaje_en(&ruta);
        assert_eq!(diag.perfiles_defensores, 2);
        let diag_json = serde_json::to_string(&diag).unwrap();
        let diag_lower = diag_json.to_lowercase();
        assert!(!diag_json.contains("DEFENSOR_1"), "{}", diag_json);
        assert!(diag_lower.contains("ana perez"), "{}", diag_json);
        assert!(diag_lower.contains("luis mora"), "{}", diag_json);
        assert!(!diag_lower.contains("carlos mendoza"), "{}", diag_json);
        assert!(!diag_json.contains("200016001075202499999"), "{}", diag_json);

        let descargas = dir.join("descargas");
        let ruta_uso = dir.join("uso_catalogos.json");
        let archivo_exp = exportar_aprendizaje_completo_en(
            &ruta,
            Some(&ruta_uso),
            None,
            &descargas,
            &[],
            &[],
        )
        .unwrap();
        let exp_txt = fs::read_to_string(&archivo_exp).unwrap();
        assert!(!exp_txt.contains("DEFENSOR_1"), "{}", exp_txt);
        assert!(!exp_txt.contains("DEFENSOR_2"), "{}", exp_txt);
        let exp_lower = exp_txt.to_lowercase();
        assert!(exp_lower.contains("ana perez"), "{}", exp_txt);
        assert!(exp_lower.contains("luis mora"), "{}", exp_txt);
        assert!(exp_txt.contains("Dra. Ana Pérez"), "{}", exp_txt);
        assert!(exp_lower.contains("defensor mora"), "{}", exp_txt);
        assert!(!exp_lower.contains("carlos mendoza"), "fuga procesado en export: {}", exp_txt);
        assert!(!exp_txt.contains("200016001075202499999"), "fuga radicado en export: {}", exp_txt);
        assert!(!exp_txt.contains("3009876543"), "fuga telefono en export: {}", exp_txt);
        assert!(exp_txt.contains("[PROCESADO]"), "{}", exp_txt);
        assert!(exp_txt.contains("[RADICADO]"), "{}", exp_txt);
        assert!(exp_txt.contains("[TELEFONO]"), "{}", exp_txt);

        // 7) Dos correcciones NO->SI no relacionadas de defensores distintos NO deben fusionarse en un clúster
        //    ni promover falsamente al Cuaderno General; y reactivar una regla retirada de otro defensor no la promueve a "*".
        let dir_iso = preparar_dir("v3_cluster_aislado");
        let ruta_iso = dir_iso.join(NOMBRE_ARCHIVO_MEMORIA);
        let mut pred_no = audiencia_base();
        pred_no.realizada = "NO".into();
        pred_no.motivo = "Juez".into();
        let mut fin_si_only = audiencia_base();
        fin_si_only.realizada = "SI".into();
        fin_si_only.motivo = String::new();
        registrar_resultado_con_traza_y_defensor_en(
            &ruta_iso,
            "Se instaló la audiencia preparatoria pero luego se suspendió por agenda del despacho",
            &pred_no,
            &fin_si_only,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        registrar_resultado_con_traza_y_defensor_en(
            &ruta_iso,
            "Lectura de fallo condenatorio efectuada normalmente con presencia de todas las partes",
            &pred_no,
            &fin_si_only,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dr. Luis Mora"),
        )
        .unwrap();
        assert!(
            obtener_cluster_listo_para_inducir_en(&ruta_iso).is_none(),
            "Dos textos totalmente distintos NO->SI de defensores distintos no deben fusionarse en un mismo clúster"
        );

        let mut mem_rev = MemoriaAprendizaje::default();
        let rid_old = agregar_regla_interna_con_defensor(
            &mut mem_rev,
            "observaciones",
            "Conservar frase exacta despacho virtual verificado en observaciones",
            "retirada",
            Some("observaciones"),
            &[],
            "mitigacion_modelo",
            "*",
            "Dra. Ana Pérez",
            &[],
            0.5,
            "",
        )
        .unwrap();
        agregar_regla_interna_con_defensor(
            &mut mem_rev,
            "observaciones",
            "Conservar frase exacta despacho virtual verificado en observaciones",
            "prueba",
            Some("observaciones"),
            &[],
            "mitigacion_modelo",
            "*",
            "Dr. Luis Mora",
            &[],
            0.8,
            "",
        );
        let r_rev = mem_rev.reglas.iter().find(|r| r.id == rid_old).unwrap();
        assert_eq!(
            r_rev.defensor_alcance, "luis mora",
            "Reactivar una regla retirada de Ana por Luis no debe promocionarla a '*'"
        );

        let _ = fs::remove_dir_all(&dir_iso);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pilar1_migracion_v3_v4_fusion_colisiones_y_brechas_a_d_e_f() {
        let dir = preparar_dir("pilar1_v3_v4_brechas");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let json_v3 = r#"{
          "version": 3,
          "aciertos": 12,
          "reglas": [
            {
              "id": "base-perfil-luis",
              "tipo_error": "observaciones",
              "regla": "Luis incluye hora de reprogramación al final",
              "refuerzos": 3,
              "actualizada": "2025-07-01T10:00:00-05:00",
              "estado": "activa",
              "campo_objetivo": "observaciones",
              "triggers": ["reprogramacion"],
              "alpha": 5.0,
              "beta": 1.0,
              "origen": "base_perfil",
              "defensor_alcance": "Dr. Luis Mora"
            },
            {
              "id": "r-jose-1",
              "tipo_error": "juzgado",
              "regla": "J5 significa Juzgado 5 Penal del Circuito",
              "refuerzos": 2,
              "actualizada": "2025-07-01T10:00:00-05:00",
              "estado": "prueba",
              "campo_objetivo": "juzgado",
              "triggers": ["j5"],
              "alpha": 3.0,
              "beta": 3.0,
              "origen": "mitigacion_modelo",
              "defensor_alcance": "Dr. José Daniel"
            },
            {
              "id": "r-jose-2",
              "tipo_error": "juzgado",
              "regla": "J5 significa Juzgado 5 Penal del Circuito",
              "refuerzos": 2,
              "actualizada": "2025-07-02T10:00:00-05:00",
              "estado": "activa",
              "campo_objetivo": "juzgado",
              "triggers": ["juzgado", "5"],
              "alpha": 4.0,
              "beta": 3.0,
              "origen": "mitigacion_modelo",
              "defensor_alcance": "JOSE DANIEL"
            }
          ],
          "tabla_alias": [
            {
              "campo": "juzgado",
              "entrada_norm": "j5",
              "canonico": "Juzgado 5 Penal del Circuito",
              "alpha": 3.0,
              "beta": 1.2,
              "defensor_alcance": "Dr. José Daniel",
              "defensores_confirmantes": ["Dr. José Daniel"],
              "ultima_actualizacion": "2025-07-01T10:00:00-05:00"
            },
            {
              "campo": "juzgado",
              "entrada_norm": "j5",
              "canonico": "Juzgado 5 Penal del Circuito",
              "alpha": 3.0,
              "beta": 1.2,
              "defensor_alcance": "Defensor Público José Daniel",
              "defensores_confirmantes": ["JOSE DANIEL"],
              "ultima_actualizacion": "2025-07-02T10:00:00-05:00"
            }
          ],
          "preferencias_por_defensor": {
            "Dr. José Daniel": { "instalada_reprogramada_si_alpha": 4.0, "instalada_reprogramada_si_beta": 2.0 },
            "JOSE DANIEL": { "instalada_reprogramada_si_alpha": 4.0, "instalada_reprogramada_si_beta": 2.0 }
          },
          "correcciones": [],
          "ejemplos_aciertos": []
        }"#;
        fs::write(&ruta, json_v3).unwrap();

        let mem = cargar_memoria_en(&ruta);
        assert_eq!(mem.version, VERSION_FORMATO);

        // Brecha A: base-perfil-luis conserva alcance normalizado y origen
        let r_base_luis = mem.reglas.iter().find(|r| r.id == "base-perfil-luis").unwrap();
        assert_eq!(r_base_luis.defensor_alcance, "luis mora");
        assert_eq!(r_base_luis.origen, "base_perfil");

        // Fusión de colisiones v3 -> v4 sobre "jose daniel" preservando nombre más informativo y evidencia Beta aditiva
        let reglas_jose: Vec<&ReglaAprendida> = mem
            .reglas
            .iter()
            .filter(|r| r.defensor_alcance == "jose daniel")
            .collect();
        assert_eq!(reglas_jose.len(), 1);
        assert_eq!(reglas_jose[0].refuerzos, 4);
        assert!(reglas_jose[0].alpha > 4.5, "alpha={}", reglas_jose[0].alpha);

        let aliases_jose: Vec<&AliasEstadistico> = mem
            .tabla_alias
            .iter()
            .filter(|a| a.defensor_alcance == "jose daniel")
            .collect();
        assert_eq!(aliases_jose.len(), 1);
        assert!(aliases_jose[0].alpha >= 4.9, "alpha alias={}", aliases_jose[0].alpha);
        assert_eq!(aliases_jose[0].defensores_confirmantes, vec!["jose daniel".to_string()]);
        assert!(mem
            .nombres_defensores
            .get("jose daniel")
            .is_some_and(|n| n.contains("José Daniel")));

        // Brecha D: deduplicación base.reglas + local.reglas priorizando local
        let mut base = MemoriaAprendizaje::default();
        base.reglas.push(construir_regla_v2(
            "b-reg".into(),
            "motivo".into(),
            "Si fiscal no asiste marcar Fiscalía siempre".into(),
            3,
            "2025-07-01T10:00:00-05:00".into(),
            "activa",
            "motivo".into(),
            vec!["fiscal".into()],
            "base_invariante",
            "*".into(),
            0.9,
            String::new(),
        ));
        let mut local = MemoriaAprendizaje::default();
        let mut loc_r = construir_regla_v2(
            "l-reg".into(),
            "motivo".into(),
            "Si fiscal no asiste marcar Fiscalía siempre".into(),
            4,
            "2025-08-01T10:00:00-05:00".into(),
            "activa",
            "motivo".into(),
            vec!["fiscal".into()],
            "mitigacion_modelo",
            "*".into(),
            0.95,
            String::new(),
        );
        loc_r.alpha = 8.0;
        local.reglas.push(loc_r);
        let det = seleccionar_contexto_detallado_para_defensor(
            "La fiscal no asiste a la audiencia",
            &base,
            &local,
            None,
            Some("Dr. José Daniel"),
        );
        assert_eq!(det.reglas.len(), 1);
        assert_eq!(det.reglas[0].id, "l-reg");

        // Brecha E: preferencias_efectivas_con_base_para_defensor hereda de base cuando local no tiene al defensor
        let mut pref_luis_base = PreferenciasEstilo::default();
        pref_luis_base.instalada_reprogramada_si_alpha = 7.0;
        pref_luis_base.instalada_reprogramada_si_beta = 1.0;
        base.preferencias_por_defensor
            .insert("Dr. Luis Mora".into(), pref_luis_base);
        let eff_luis = preferencias_efectivas_con_base_para_defensor(&base, &local, Some("luis mora"));
        assert!((eff_luis.instalada_reprogramada_si_alpha - 7.0).abs() < 1e-6);

        // Brecha F: obtener_resumenes_consolidados_prompt aplica precedencia local > base y recorte 320/380
        base.perfiles_sinteticos.insert(
            "*".into(),
            PerfilSinteticoDefensor {
                ambito: "*".into(),
                nombre_mostrar: "General".into(),
                resumen: "Criterio base general".into(),
                version: 1,
                ultima_actualizacion: String::new(),
            },
        );
        local.perfiles_sinteticos.insert(
            "*".into(),
            PerfilSinteticoDefensor {
                ambito: "*".into(),
                nombre_mostrar: "General".into(),
                resumen: "Criterio local ".repeat(40),
                version: 2,
                ultima_actualizacion: String::new(),
            },
        );
        local.perfiles_sinteticos.insert(
            "Dra. Ana Pérez".into(),
            PerfilSinteticoDefensor {
                ambito: "ana perez".into(),
                nombre_mostrar: "Dra. Ana Pérez".into(),
                resumen: "Perfil específico Ana ".repeat(35),
                version: 1,
                ultima_actualizacion: String::new(),
            },
        );
        let (rg, rd) = obtener_resumenes_consolidados_prompt(&base, &local, Some("Dra. Ana Pérez"));
        let rg_txt = rg.expect("resumen general");
        let rd_txt = rd.expect("resumen defensor");
        assert!(rg_txt.starts_with("Criterio local"));
        assert!(rg_txt.chars().count() <= 320);
        assert!(rd_txt.starts_with("Perfil específico Ana"));
        assert!(rd_txt.chars().count() <= 380);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pilar1_renombrado_y_export_con_priors_sin_fuga_pii() {
        let dir = preparar_dir("pilar1_renombrado_export");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let descargas = dir.join("descargas");

        let mut pred = audiencia_base();
        pred.radicado = "JUAN CESAR - 200016001075202588888".into();
        pred.juzgado = "Juzgado 1 Promiscuo Municipal de San Juan del Cesar".into();
        let mut fin = pred.clone();
        fin.motivo = "Fiscalía".into();
        fin.realizada = "NO".into();

        for def in ["Dr. José Daniel", "Dra. Ana Pérez"] {
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                "Audiencia de JUAN CESAR rad 200016001075202588888 cel 3101234567 fiscal no vino",
                &pred,
                &fin,
                "openai",
                "gpt-5-mini",
                &[],
                Some(def),
            )
            .unwrap();
        }

        // Añadir perfil sintético y registro de consolidación con PII accidental para verificar sanitización en export
        {
            let mut m = cargar_memoria_en(&ruta);
            m.perfiles_sinteticos.insert(
                "jose daniel".into(),
                PerfilSinteticoDefensor {
                    ambito: "jose daniel".into(),
                    nombre_mostrar: "Dr. José Daniel".into(),
                    resumen: "Dr. José Daniel prioriza radicado 200016001075202588888 de JUAN CESAR cel 3101234567".into(),
                    version: 1,
                    ultima_actualizacion: Local::now().to_rfc3339(),
                },
            );
            m.consolidaciones_ia.push(RegistroConsolidacionIa {
                id: "cons-test".into(),
                fecha: Local::now().to_rfc3339(),
                ambito: "*".into(),
                nombre_ambito: "General".into(),
                modelo: "openai:gpt-5-mini".into(),
                acciones_reglas: Vec::new(),
                resumen_actualizado: "Resumen sobre JUAN CESAR 200016001075202588888".into(),
                sugerencias_build: vec![SugerenciaBuild {
                    categoria: "jerga".into(),
                    ambito: "*".into(),
                    detalle: "Detalle de JUAN CESAR cel 3101234567".into(),
                    evidencia: "ev".into(),
                }],
                telemetria_disparo: TelemetriaDisparoConsolidacion::default(),
            });
            guardar_memoria_en(&ruta, &m).unwrap();
        }

        let mut priors = IndicePriorsExcel::default();
        priors
            .juzgados_freq
            .insert("Juzgado 1 Promiscuo Municipal de San Juan del Cesar".into(), 5);
        priors.tipos_freq.insert("Audiencia concentrada".into(), 5);
        priors.horas_freq.insert(9, 4);
        priors
            .grafo
            .defensor_a_juzgados
            .entry("jose daniel".into())
            .or_default()
            .insert("Juzgado 1 Promiscuo Municipal de San Juan del Cesar".into(), 3);

        let arch = exportar_aprendizaje_completo_con_priors_en(
            &ruta,
            None,
            None,
            Some(&priors),
            &descargas,
            &[],
            &[],
            &["Dr. José Daniel".into(), "Dra. Ana Pérez".into()],
        )
        .unwrap();
        let json_str = fs::read_to_string(&arch).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.get("defensores_directorio").is_some());
        assert!(parsed.get("resumen_patrones_generales").is_some());
        assert!(parsed.get("preferencias_por_defensor").is_some());
        assert!(parsed.get("grafo_asociaciones").is_some());
        assert!(parsed.get("perfiles_sinteticos").is_some());
        assert!(parsed.get("consolidaciones_ia").is_some());

        assert!(!json_str.contains("DEFENSOR_1"));
        assert!(json_str.contains("jose daniel"));
        assert!(json_str.contains("Dr. José Daniel"));
        assert!(json_str.contains("San Juan del Cesar"));
        assert!(!json_str.to_lowercase().contains("juan cesar"), "{}", json_str);
        assert!(!json_str.contains("200016001075202588888"), "{}", json_str);
        assert!(!json_str.contains("3101234567"), "{}", json_str);

        // Renombrar defensor migra todas sus claves en memoria
        migrar_defensor_en_memoria_en(&ruta, "Dr. José Daniel", "José Daniel Martínez").unwrap();
        let post = cargar_memoria_en(&ruta);
        assert_eq!(
            post.nombres_defensores.get("jose daniel martinez").map(String::as_str),
            Some("José Daniel Martínez")
        );
        assert!(post.perfiles_sinteticos.contains_key("jose daniel martinez"));
        assert!(!post.perfiles_sinteticos.contains_key("jose daniel"));
        assert!(post
            .correcciones
            .iter()
            .any(|c| c.defensor_clave == "jose daniel martinez"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pilar2_consolidacion_dual_shannon_watermark_y_concurrencia() {
        // 1) Guard atómico CONSOLIDACION_EN_CURSO
        assert!(intentar_reservar_consolidacion());
        assert!(!intentar_reservar_consolidacion());
        liberar_consolidacion();
        assert!(intentar_reservar_consolidacion());
        liberar_consolidacion();

        let dir = preparar_dir("pilar2_consolidacion");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let ok = audiencia_base();
        let mut p_err = ok.clone();
        p_err.radicado = "MARIO GOMEZ - 200016001075202577777".into();
        let mut f_err = p_err.clone();
        f_err.realizada = "NO".into();
        f_err.motivo = "Fiscalía".into();

        // Calentar ewma con 5 aciertos: q10 subiría pero precision_ewma refleja certeza actual
        for i in 0..5 {
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!("Audiencia preparatoria sin novedad caso {} en despacho", i),
                &ok,
                &ok,
                "openai",
                "gpt-5-mini",
                &[],
                Some("Dra. Ana Pérez"),
            )
            .unwrap();
        }
        let mem_frio = cargar_memoria_en(&ruta);
        let p_tras_ok = mem_frio
            .estados_consolidacion
            .get("*")
            .map(|e| e.presion)
            .unwrap_or(0.0);
        assert!(p_tras_ok <= 1e-6);

        // Guardas bloquean con pocas correcciones (< 3) aunque RNG sea 0.0
        let mut mem_guard = cargar_memoria_en(&ruta);
        assert!(evaluar_candidato_consolidacion(&mut mem_guard, Some("Dra. Ana Pérez"), Some(0.0)).is_none());

        // Provocar 5 errores transversales en Ana y Luis sobre motivo Fiscalía
        let mut ids_corr = Vec::new();
        for i in 0..4 {
            let def = if i < 2 { "Dra. Ana Pérez" } else { "Dr. Luis Mora" };
            let (_, c) = registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!(
                    "Audiencia de MARIO GOMEZ rad 200016001075202577777 cel 3001112233 fiscalia ausente caso {}",
                    i
                ),
                &p_err,
                &f_err,
                "openai",
                "gpt-5-mini",
                &[],
                Some(def),
            )
            .unwrap();
            ids_corr.push(c.unwrap().id);
        }

        // Añadir 2 reglas redundantes + 1 regla base_invariante en memoria
        {
            let mut m = cargar_memoria_en(&ruta);
            m.reglas.push(construir_regla_v2(
                "base-intocable".into(),
                "motivo".into(),
                "Base invariante nunca se pone en cuarentena".into(),
                5,
                Local::now().to_rfc3339(),
                "activa",
                "motivo".into(),
                vec!["fiscal".into()],
                "base_invariante",
                "*".into(),
                0.95,
                String::new(),
            ));
            m.reglas.push(construir_regla_v2(
                "loc-dup-1".into(),
                "motivo".into(),
                "Marcar Fiscalía si el fiscal no asiste".into(),
                2,
                Local::now().to_rfc3339(),
                "prueba",
                "motivo".into(),
                vec!["fiscal".into()],
                "mitigacion_modelo",
                "*".into(),
                0.8,
                String::new(),
            ));
            m.reglas.push(construir_regla_v2(
                "loc-dup-2".into(),
                "motivo".into(),
                "Cuando la fiscal no comparece usar Fiscalía".into(),
                1,
                Local::now().to_rfc3339(),
                "candidata",
                "motivo".into(),
                vec!["comparece".into()],
                "mitigacion_modelo",
                "*".into(),
                0.7,
                String::new(),
            ));
            m.reglas.push(construir_regla_v2(
                "loc-conf".into(),
                "motivo".into(),
                "Regla obsoleta contradictoria de prueba".into(),
                1,
                Local::now().to_rfc3339(),
                "activa",
                "motivo".into(),
                vec!["obsoleta".into()],
                "mitigacion_modelo",
                "*".into(),
                0.6,
                String::new(),
            ));
            // Asegurar presión elevada tras errores
            m.estados_consolidacion.entry("*".into()).or_default().presion = 8.0;
            guardar_memoria_en(&ruta, &m).unwrap();
        }

        let mut mem_eval = cargar_memoria_en(&ruta);
        // Con rng=0.9999 no dispara (modelo probabilístico no determinista)
        assert!(evaluar_candidato_consolidacion(&mut mem_eval, Some("Dra. Ana Pérez"), Some(0.9999)).is_none());
        let tel = evaluar_candidato_consolidacion(&mut mem_eval, Some("Dra. Ana Pérez"), Some(0.001))
            .expect("debe disparar consolidación con presión alta y rng bajo");
        assert_eq!(tel.ambito, "*");

        let lote = preparar_lote_consolidacion_en(&ruta, &tel.ambito, tel.clone(), false)
            .expect("debe preparar lote");
        let lote_json = serde_json::to_string(&lote).unwrap();
        assert!(!lote_json.to_lowercase().contains("mario gomez"), "{}", lote_json);
        assert!(!lote_json.contains("200016001075202577777"), "{}", lote_json);
        assert!(!lote_json.contains("3001112233"), "{}", lote_json);
        assert!(!lote.watermark_ids.is_empty());

        // Simular llegada concurrente de una nueva corrección DESPUÉS de tomar el snapshot (no está en watermark_ids)
        registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            "Audiencia adicional concurrente con fiscalia ausente en sala",
            &p_err,
            &f_err,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        let sin_cons_antes = cargar_memoria_en(&ruta).estados_consolidacion["*"].correcciones_sin_consolidar;

        let res_cons = ResultadoConsolidacionIa {
            acciones_reglas: vec![
                AccionConsolidacionInput {
                    tipo_accion: "fusionar".into(),
                    ids_origen: vec!["loc-dup-1".into(), "loc-dup-2".into()],
                    tipo_error: "motivo".into(),
                    campo_objetivo: "motivo".into(),
                    regla: "Si el fiscal no se conecta o no comparece usar Fiscalía".into(),
                    triggers: vec!["fiscal".into(), "comparece".into()],
                    motivo: "Unificación".into(),
                    confianza: 0.9,
                },
                AccionConsolidacionInput {
                    tipo_accion: "conflicto".into(),
                    ids_origen: vec!["base-intocable".into(), "loc-conf".into()],
                    tipo_error: "motivo".into(),
                    campo_objetivo: "motivo".into(),
                    regla: String::new(),
                    triggers: Vec::new(),
                    motivo: "Obsoleta".into(),
                    confianza: 0.95,
                },
                AccionConsolidacionInput {
                    tipo_accion: "sintetizar".into(),
                    ids_origen: Vec::new(),
                    tipo_error: "motivo".into(),
                    campo_objetivo: "motivo".into(),
                    regla: "Nueva síntesis 1 sobre Fiscalía".into(),
                    triggers: vec!["sintesis1".into()],
                    motivo: "S1".into(),
                    confianza: 0.85,
                },
                AccionConsolidacionInput {
                    tipo_accion: "sintetizar".into(),
                    ids_origen: Vec::new(),
                    tipo_error: "motivo".into(),
                    campo_objetivo: "motivo".into(),
                    regla: "Nueva síntesis 2 sobre preacuerdos".into(),
                    triggers: vec!["preacuerdo".into()],
                    motivo: "S2".into(),
                    confianza: 0.85,
                },
                AccionConsolidacionInput {
                    tipo_accion: "sintetizar".into(),
                    ids_origen: Vec::new(),
                    tipo_error: "motivo".into(),
                    campo_objetivo: "motivo".into(),
                    regla: "Tercera síntesis debe respetarse tope de 2".into(),
                    triggers: vec!["exceso".into()],
                    motivo: "S3".into(),
                    confianza: 0.85,
                },
            ],
            resumen_actualizado: "Resumen transversal consolidado ".repeat(20),
            sugerencias_build: vec![SugerenciaBuildInput {
                categoria: "jerga".into(),
                detalle: "Agregar abreviación".into(),
                evidencia: "4 casos".into(),
            }],
        };

        let diag = aplicar_consolidacion_ia_en(&ruta, &lote, &res_cons, "openai:gpt-5-mini").unwrap();
        assert_eq!(diag.consolidaciones_totales, 1);
        let mem_fin = cargar_memoria_en(&ruta);
        assert_eq!(
            mem_fin.reglas.iter().find(|r| r.id == "base-intocable").unwrap().estado,
            "activa",
            "base_invariante nunca debe tocarse"
        );
        assert_eq!(
            mem_fin.reglas.iter().find(|r| r.id == "loc-conf").unwrap().estado,
            "cuarentena"
        );
        assert_eq!(
            mem_fin.reglas.iter().find(|r| r.id == "loc-dup-2").unwrap().estado,
            "retirada"
        );
        assert_eq!(
            mem_fin
                .reglas
                .iter()
                .filter(|r| r.origen == "consolidacion_ia")
                .count(),
            2
        );
        assert!(mem_fin.reglas.iter().filter(|r| r.origen == "consolidacion_ia").all(|r| r.estado == "prueba"));
        assert!(mem_fin.perfiles_sinteticos["*"].resumen.chars().count() <= MAX_CHARS_RESUMEN_GENERAL);

        // Solo watermark_ids se restan: queda >= 1 corrección concurrente pendiente
        let sin_cons_despues = mem_fin.estados_consolidacion["*"].correcciones_sin_consolidar;
        assert_eq!(
            sin_cons_despues,
            sin_cons_antes.saturating_sub(lote.watermark_ids.len() as u32)
        );
        assert!(sin_cons_despues >= 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pilar2_simulacion_prequential_presion_consolidacion_y_decaimiento() {
        let dir = preparar_dir("pilar2_sim_prequential");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let ok = audiencia_base();
        let mut p_err = ok.clone();
        p_err.realizada = "NO".into();
        p_err.motivo = "Usuario".into();
        let mut f_err = p_err.clone();
        f_err.motivo = "Fiscalía".into();

        // Tramo 1: 15 aciertos limpios -> presión ~0, cero disparos
        for i in 0..15 {
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!("Audiencia sin novedad limpia caso {}", i),
                &ok,
                &ok,
                "openai",
                "gpt-5-mini",
                &[],
                Some("Dra. Ana Pérez"),
            )
            .unwrap();
        }
        let m1 = cargar_memoria_en(&ruta);
        assert!(m1.estados_consolidacion["*"].presion < 0.05);

        // Tramo 2: racha de errores transversales de 2 defensores -> presión crece por sorpresa de Shannon y dispara
        for i in 0..10 {
            let def = if i % 2 == 0 { "Dra. Ana Pérez" } else { "Dr. Luis Mora" };
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!("Diligencia aplazada por ausencia de fiscalia caso {}", i),
                &p_err,
                &f_err,
                "openai",
                "gpt-5-mini",
                &[],
                Some(def),
            )
            .unwrap();
        }
        let mut m2 = cargar_memoria_en(&ruta);
        let presion_pico = m2.estados_consolidacion["*"].presion;
        assert!(presion_pico > 4.5, "presion_pico={}", presion_pico);
        let tel = evaluar_candidato_consolidacion(&mut m2, Some("Dra. Ana Pérez"), Some(0.01))
            .expect("debe disparar tras drift transversal");
        let amb_tel = tel.ambito.clone();
        let lote = preparar_lote_consolidacion_en(&ruta, &amb_tel, tel, false).unwrap();
        aplicar_consolidacion_ia_en(
            &ruta,
            &lote,
            &ResultadoConsolidacionIa {
                acciones_reglas: Vec::new(),
                resumen_actualizado: "Regla transversal fiscalía ausente".into(),
                sugerencias_build: Vec::new(),
            },
            "openai:gpt-5-mini",
        )
        .unwrap();

        // Tramo 3: 10 aciertos siguientes reducen presión residual exponencialmente (0.93^10)
        for i in 0..10 {
            registrar_resultado_con_traza_y_defensor_en(
                &ruta,
                &format!("Audiencia post consolidacion correcta {}", i),
                &ok,
                &ok,
                "openai",
                "gpt-5-mini",
                &[],
                Some("Dra. Ana Pérez"),
            )
            .unwrap();
        }
        let m3 = cargar_memoria_en(&ruta);
        assert!(m3.estados_consolidacion["*"].presion < presion_pico * 0.15);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pilar3_aclaracion_adaptativa_dato_externo_y_cero_huerfanas() {
        let dir = preparar_dir("pilar3_aclaracion");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();
        let mut fin_juz = pred.clone();
        fin_juz.juzgado = "Juzgado 3 CTO".into();

        // 1) Modulación por precision_ewma del defensor + firma ya explicada
        let mut mem_fria = MemoriaAprendizaje::default();
        let corr_test = CorreccionIa {
            id: "c-mod".into(),
            registrada: String::new(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "Audiencia en Juzgado 3 CTO en Valledupar".into(),
            prediccion: pred.clone(),
            final_usuario: fin_juz.clone(),
            campos: vec![CampoCorregido {
                campo: "juzgado".into(),
                valor_ia: pred.juzgado.clone(),
                valor_usuario: fin_juz.juzgado.clone(),
            }],
            estado: "pendiente".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: "ana perez".into(),
        };
        assert!(decidir_aclaracion_local(&mem_fria, &corr_test, None, "ana perez").is_some());

        // Con defensor ya maduro y precision_ewma=0.95 en juzgado, deja de molestar
        mem_fria.llamadas_extraccion_total = 65;
        let mut md = MetricasDefensor::default();
        md.por_campo.insert(
            "juzgado".into(),
            MetricasCampo {
                campo: "juzgado".into(),
                evaluaciones: 30,
                aciertos: 29,
                correcciones: 1,
                alpha: 25.0,
                beta: 2.0,
                precision_ewma: 0.95,
                ultima_actualizacion: String::new(),
            },
        );
        mem_fria.metricas_por_defensor.insert("ana perez".into(), md);
        assert!(decidir_aclaracion_local(&mem_fria, &corr_test, None, "ana perez").is_none());

        // 2) "Dato externo (no estaba en el texto)" a 0 tokens + limpieza de regla asociada (0 huérfanas)
        let (r1, c1) = registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            "Audiencia preparatoria en Juzgado 3 CTO Valledupar",
            &pred,
            &fin_juz,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        assert!(r1.aclaracion_pendiente.is_some());
        let id1 = c1.unwrap().id;
        // Simular que existía regla candidata asociada previamente
        {
            let mut m = cargar_memoria_en(&ruta);
            let rid = agregar_regla_interna_con_defensor(
                &mut m,
                "juzgado",
                "Regla asociada temporal candidata",
                "candidata",
                Some("juzgado"),
                &[],
                "mitigacion_modelo",
                "*",
                "ana perez",
                &[],
                0.5,
                &id1,
            )
            .unwrap();
            m.correcciones.iter_mut().find(|c| c.id == id1).unwrap().regla_id_asociada = rid;
            guardar_memoria_en(&ruta, &m).unwrap();
        }
        let res_ext = marcar_dato_externo_en(&ruta, &id1).unwrap();
        assert!(res_ext.aclaracion_pendiente.is_none());
        let mem_ext = cargar_memoria_en(&ruta);
        let corr_ext = mem_ext.correcciones.iter().find(|c| c.id == id1).unwrap();
        assert_eq!(corr_ext.estado, "descartada");
        assert_eq!(corr_ext.nota_usuario, "Dato externo (no estaba en el texto)");
        assert!(mem_ext.reglas.is_empty(), "regla huérfana debió eliminarse");

        // Firma ya explicada no vuelve a preguntar en las siguientes 30 interacciones
        let (r_rep, _) = registrar_resultado_con_traza_y_defensor_en(
            &ruta,
            "Audiencia preparatoria en Juzgado 8 CTO Valledupar",
            &pred,
            &fin_juz,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        assert!(r_rep.aclaracion_pendiente.is_none());

        // 3) Auto-descarte tras >= 2 turnos sin responder y al cambiar de defensor
        let dir2 = preparar_dir("pilar3_autodescarte");
        let ruta2 = dir2.join(NOMBRE_ARCHIVO_MEMORIA);
        let mut fin_tipo = pred.clone();
        fin_tipo.tipo = "Audiencia preliminar".into();
        let (r_ask, c_ask) = registrar_resultado_con_traza_y_defensor_en(
            &ruta2,
            "Diligencia en Juzgado 3 CTO sobre control preliminar",
            &pred,
            &fin_juz,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        assert!(r_ask.aclaracion_pendiente.is_some());
        let ask_id = c_ask.unwrap().id;
        // 2 turnos del mismo defensor sin responder auto-descartan aclaración e incrementan beta_omit
        registrar_resultado_con_traza_y_defensor_en(
            &ruta2,
            "Turno 1 sin responder",
            &pred,
            &pred,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        let (r_t2, _) = registrar_resultado_con_traza_y_defensor_en(
            &ruta2,
            "Turno 2 sin responder",
            &pred,
            &pred,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        assert!(r_t2.aclaracion_pendiente.is_none());
        let mem_auto = cargar_memoria_en(&ruta2);
        assert!(mem_auto.estado_aclaracion.beta_omit >= 2.0);
        assert_eq!(mem_auto.estado_aclaracion.racha_omisiones, 1);
        assert!(mem_auto.enfriamiento_aclaracion >= 6);
        assert_eq!(
            mem_auto.correcciones.iter().find(|c| c.id == ask_id).unwrap().estado,
            "pendiente"
        );

        // Cambio de defensor limpia esperando_aclaracion de inmediato
        mem_auto_check_cambio_def(&ruta2, &pred, &fin_tipo);

        // 4) aplicar_analisis_en con !es_error_de_ia retira/elimina regla_id_asociada (0 huérfanas)
        let mut pred_h = pred.clone();
        pred_h.juzgado = "Juzgado 2 Penal del Circuito".into();
        let mut fin_h = pred.clone();
        fin_h.realizada = "NO".into();
        fin_h.motivo = "Fiscalía".into();
        let dir3 = preparar_dir("pilar3_huerfana_no_error");
        let ruta3 = dir3.join(NOMBRE_ARCHIVO_MEMORIA);
        let (_, c_h) = registrar_resultado_con_traza_y_defensor_en(
            &ruta3,
            "Audiencia en Juzgado 2 Penal reprogramada por fiscalia",
            &pred_h,
            &fin_h,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        let cid_h = c_h.unwrap().id;
        {
            let mut m = cargar_memoria_en(&ruta3);
            let rid = agregar_regla_interna_con_defensor(
                &mut m,
                "motivo",
                "Regla preliminar asociada a corrección",
                "candidata",
                Some("motivo"),
                &["fiscalia".into()],
                "mitigacion_modelo",
                "*",
                "ana perez",
                &[],
                0.5,
                &cid_h,
            )
            .unwrap();
            m.correcciones.iter_mut().find(|c| c.id == cid_h).unwrap().regla_id_asociada = rid;
            guardar_memoria_en(&ruta3, &m).unwrap();
        }
        aplicar_analisis_en(
            &ruta3,
            &cid_h,
            &AnalisisCorreccionIa {
                es_error_de_ia: false,
                tipo_error: "otro".into(),
                regla: String::new(),
                confianza: 0.9,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        let mem_h = cargar_memoria_en(&ruta3);
        assert!(mem_h.reglas.iter().all(|r| r.estado == "retirada") || mem_h.reglas.is_empty());
        assert_eq!(
            mem_h.correcciones.iter().find(|c| c.id == cid_h).unwrap().regla_id_asociada,
            ""
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
        let _ = fs::remove_dir_all(&dir3);
    }

    fn mem_auto_check_cambio_def(ruta: &Path, pred: &AudienciaInput, fin_tipo: &AudienciaInput) {
        let mut m = cargar_memoria_en(ruta);
        m.enfriamiento_aclaracion = 0;
        guardar_memoria_en(ruta, &m).unwrap();

        // Tras omisión previa (beta_omit >= 2.0 -> r_resp menor), una variación leve de `tipo` ya NO molesta
        let (r_leve, _) = registrar_resultado_con_traza_y_defensor_en(
            ruta,
            "Audiencia preliminar urgente en despacho penal ordinario",
            pred,
            fin_tipo,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dr. Luis Mora"),
        )
        .unwrap();
        assert!(r_leve.aclaracion_pendiente.is_none());

        // En cambio, una corrección de alto VoI (realizada + motivo) sí solicita aclaración para Luis
        // y se auto-descarta inmediatamente al cambiar a Ana
        let mut fin_amb = pred.clone();
        fin_amb.realizada = "NO".into();
        fin_amb.motivo = "Juez".into();
        let (r_b, _) = registrar_resultado_con_traza_y_defensor_en(
            ruta,
            "Diligencia se instaló pero se suspendió y se reprogramó por despacho en sala",
            pred,
            &fin_amb,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dr. Luis Mora"),
        )
        .unwrap();
        assert!(r_b.aclaracion_pendiente.is_some());
        let (r_otro, _) = registrar_resultado_con_traza_y_defensor_en(
            ruta,
            "Audiencia normal de control",
            pred,
            pred,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
        )
        .unwrap();
        assert!(r_otro.aclaracion_pendiente.is_none());
    }

    #[test]
    fn test_regresiones_rng_notas_consolidadas_renombrado_y_anonimizador() {
        let dir = preparar_dir("regresiones_v4");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let pred = audiencia_base();

        // 1) evaluar_y_preparar_consolidacion_en persiste semilla_rng aunque u >= prob rechace el disparo
        {
            let mut m = MemoriaAprendizaje::default();
            m.llamadas_extraccion_total = 12;
            m.semilla_rng = DEFAULT_SEMILLA_RNG;
            let mut est = EstadoConsolidacion::default();
            est.extracciones_desde_ultima = 8;
            est.correcciones_sin_consolidar = 4;
            est.presion = 0.5; // prob muy baja -> primer u casi seguro >= prob
            m.estados_consolidacion.insert("*".into(), est);
            m.correcciones.push(CorreccionIa {
                id: "c-rng".into(),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                texto_original: "Audiencia de prueba rng".into(),
                prediccion: pred.clone(),
                final_usuario: pred.clone(),
                campos: vec![CampoCorregido {
                    campo: "motivo".into(),
                    valor_ia: "Juez".into(),
                    valor_usuario: "Fiscalía".into(),
                }],
                estado: "pendiente".into(),
                regla_generada: String::new(),
                regla_id_asociada: String::new(),
                pregunta_ia: String::new(),
                nota_usuario: String::new(),
                exportada: false,
                defensor_clave: String::new(),
            });
            guardar_memoria_en(&ruta, &m).unwrap();
        }
        let lote_rech = evaluar_y_preparar_consolidacion_en(&ruta, None);
        assert!(lote_rech.is_none());
        let mem_post_rng = cargar_memoria_en(&ruta);
        assert_ne!(
            mem_post_rng.semilla_rng, DEFAULT_SEMILLA_RNG,
            "semilla_rng debe avanzar en disco aunque el roll no dispare"
        );

        // 2) guardar_nota_aclaracion_en reabre una corrección ya consolidada y no cuenta IDs inexistentes
        {
            let mut m = cargar_memoria_en(&ruta);
            let est = m.estados_consolidacion.get_mut("*").unwrap();
            est.ids_consolidadas_recientes.push("c-rng".into());
            est.correcciones_sin_consolidar = 0;
            est.notas_sin_consolidar = 0;
            guardar_memoria_en(&ruta, &m).unwrap();
        }
        let (_, fantasma) = guardar_nota_aclaracion_en(&ruta, "id-inexistente", "Nota fantasma").unwrap();
        assert!(fantasma.is_none());
        assert_eq!(cargar_memoria_en(&ruta).estados_consolidacion["*"].notas_sin_consolidar, 0);

        let (_, real_nota) = guardar_nota_aclaracion_en(&ruta, "c-rng", "Nota nueva tras consolidar").unwrap();
        assert!(real_nota.is_some());
        let m_reabierta = cargar_memoria_en(&ruta);
        assert!(!m_reabierta.estados_consolidacion["*"].ids_consolidadas_recientes.contains(&"c-rng".to_string()));
        assert_eq!(m_reabierta.estados_consolidacion["*"].correcciones_sin_consolidar, 1);
        assert_eq!(m_reabierta.estados_consolidacion["*"].notas_sin_consolidar, 1);

        // 3) descartar_aclaracion_en no refuerza regla ya asociada; retiro respeta otra corrección activa
        {
            let mut m = MemoriaAprendizaje::default();
            let rid = agregar_regla_interna_con_defensor(
                &mut m,
                "motivo",
                "Regla compartida de cluster dos casos",
                "prueba",
                Some("motivo"),
                &["fiscal".into()],
                "mitigacion_modelo",
                "*",
                "ana perez",
                &[],
                0.85,
                "c1",
            )
            .unwrap();
            for (cid, est_c) in [("c1", "esperando_aclaracion"), ("c2", "analizada")] {
                m.correcciones.push(CorreccionIa {
                    id: cid.into(),
                    registrada: String::new(),
                    proveedor: "openai".into(),
                    modelo: "gpt-5-mini".into(),
                    texto_original: "Fiscal ausente".into(),
                    prediccion: pred.clone(),
                    final_usuario: pred.clone(),
                    campos: vec![CampoCorregido {
                        campo: "motivo".into(),
                        valor_ia: "Usuario".into(),
                        valor_usuario: "Fiscalía".into(),
                    }],
                    estado: est_c.into(),
                    regla_generada: "Regla compartida de cluster dos casos".into(),
                    regla_id_asociada: rid.clone(),
                    pregunta_ia: if cid == "c1" { "¿Por qué?".into() } else { String::new() },
                    nota_usuario: String::new(),
                    exportada: false,
                    defensor_clave: "ana perez".into(),
                });
            }
            guardar_memoria_en(&ruta, &m).unwrap();
        }
        descartar_aclaracion_en(&ruta, "c1").unwrap();
        assert_eq!(cargar_memoria_en(&ruta).reglas[0].refuerzos, 1);
        marcar_dato_externo_en(&ruta, "c1").unwrap();
        assert_eq!(
            cargar_memoria_en(&ruta).reglas[0].estado,
            "prueba",
            "No debe eliminar regla mientras c2 siga respaldándola"
        );

        // 4) Anonimizador no censura palabra legal 'procesado' y protege sub-frases honoríficas de 3 tokens
        let anon = Anonimizador::desde_radicados(&["PROCESADO - 1234", "PEDRO DANIEL - 200016001075202500111"])
            .con_escudos_defensores(&["Dr. José Daniel Martínez".into()]);
        let out_regla = anon.aplicar("No pude contactar al procesado Pedro Daniel ni a Daniel; confirmó el Dr. José Daniel.");
        assert!(out_regla.contains("al procesado"), "{}", out_regla);
        assert!(!out_regla.to_lowercase().contains("pedro"), "{}", out_regla);
        assert_eq!(out_regla.matches("[PROCESADO]").count(), 2, "{}", out_regla);
        assert!(out_regla.contains("Dr. José Daniel"), "{}", out_regla);

        // 5) Renombrar defensor actualiza consolidaciones_ia y repoints regla_id_asociada al colapsar reglas
        let mut mem_ren = MemoriaAprendizaje::default();
        mem_ren.consolidaciones_ia.push(RegistroConsolidacionIa {
            id: "cons-1".into(),
            fecha: String::new(),
            ambito: "jose daniel".into(),
            nombre_ambito: "Dr. José Daniel".into(),
            modelo: "openai:gpt-5-mini".into(),
            acciones_reglas: Vec::new(),
            resumen_actualizado: "Ok".into(),
            sugerencias_build: vec![SugerenciaBuild {
                categoria: "jerga".into(),
                ambito: "jose daniel".into(),
                detalle: "d".into(),
                evidencia: "e".into(),
            }],
            telemetria_disparo: TelemetriaDisparoConsolidacion {
                ambito: "jose daniel".into(),
                ..Default::default()
            },
        });
        let r_a = agregar_regla_interna_con_defensor(
            &mut mem_ren,
            "juzgado",
            "Mantener abreviacion CTO en juzgado",
            "activa",
            Some("juzgado"),
            &["cto".into()],
            "mitigacion_modelo",
            "*",
            "Dra. Ana Pérez",
            &[],
            0.9,
            "ca",
        )
        .unwrap();
        let r_b = agregar_regla_interna_con_defensor(
            &mut mem_ren,
            "juzgado",
            "Mantener abreviacion CTO en juzgado",
            "prueba",
            Some("juzgado"),
            &["cto".into()],
            "mitigacion_modelo",
            "*",
            "Dr. José Daniel",
            &[],
            0.8,
            "cb",
        )
        .unwrap();
        // Separar reglas manualmente para probar colapso en renombrado y repoint de regla_id_asociada
        assert_eq!(r_a, r_b);
        mem_ren.reglas.clear();
        mem_ren.reglas.push(construir_regla_v2(
            "r-ana-dup".into(),
            "juzgado".into(),
            "Conservar sigla MPAL intacta".into(),
            2,
            String::new(),
            "activa",
            "juzgado".into(),
            vec!["mpal".into()],
            "mitigacion_modelo",
            "*".into(),
            0.9,
            "ca".into(),
        ));
        mem_ren.reglas[0].defensor_alcance = "ana perez".into();
        let mut r_jose = mem_ren.reglas[0].clone();
        r_jose.id = "r-jose-dup".into();
        r_jose.defensor_alcance = "jose daniel".into();
        mem_ren.reglas.push(r_jose);
        mem_ren.correcciones.push(CorreccionIa {
            id: "c-col".into(),
            registrada: String::new(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "Juzgado 3 MPAL".into(),
            prediccion: pred.clone(),
            final_usuario: pred,
            campos: Vec::new(),
            estado: "analizada".into(),
            regla_generada: "Conservar sigla MPAL intacta".into(),
            regla_id_asociada: "r-jose-dup".into(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: "jose daniel".into(),
        });

        assert!(renombrar_defensor_en_memoria_struct(&mut mem_ren, "Dr. José Daniel", "José Daniel Martínez"));
        assert_eq!(mem_ren.consolidaciones_ia[0].ambito, "jose daniel martinez");
        assert_eq!(mem_ren.consolidaciones_ia[0].nombre_ambito, "José Daniel Martínez");
        assert_eq!(mem_ren.consolidaciones_ia[0].sugerencias_build[0].ambito, "jose daniel martinez");

        // Fusionar José Daniel Martínez -> Ana Pérez colapsa reglas duplicadas y repunta regla_id_asociada
        assert!(renombrar_defensor_en_memoria_struct(&mut mem_ren, "José Daniel Martínez", "Dra. Ana Pérez"));
        assert_eq!(mem_ren.reglas.len(), 1);
        assert_eq!(mem_ren.correcciones[0].regla_id_asociada, "r-ana-dup");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_anonimizador_radicados_pegados_cortos_largos_solapados_y_telefonos() {
        let anon = Anonimizador::desde_radicados(&[
            "CARLOS MENDOZA - 12345",
            "CARLOS MENDOZA - 123456789",
            "ANA TORRES - 200016001075",
            "PEDRO RUIZ - 200016001075202500111",
            "LUIS GOMEZ - (200016009999)",
            "SOFIA VARGAS - 2025-00123",
            "DIEGO SILVA - 20001/00123",
            "20001 60 01075 2025 00777",
        ]);

        // Radicados cortos/largos pegados a texto sin espacios
        assert_eq!(anon.aplicar("rad12345"), "rad[RADICADO]");
        assert_eq!(anon.aplicar("proc12345fin"), "proc[RADICADO]fin");
        assert_eq!(anon.aplicar("rad200016001075"), "rad[RADICADO]");
        assert_eq!(
            anon.aplicar("CARLOS MENDOZA-200016001075"),
            "[PROCESADO]-[RADICADO]"
        );
        assert_eq!(
            anon.aplicar("CARLOS MENDOZA200016001075"),
            "[PROCESADO][RADICADO]"
        );
        assert_eq!(
            anon.aplicar("200016001075CARLOS MENDOZA"),
            "[RADICADO][PROCESADO]"
        );
        assert_eq!(anon.aplicar("rad2025-00123"), "rad[RADICADO]");
        assert_eq!(anon.aplicar("rad20001/00123"), "rad[RADICADO]");
        assert_eq!(anon.aplicar("120001/00123"), "1[RADICADO]");

        // Prefijos solapados: debe ganar la coincidencia más larga sin dejar colas numéricas
        assert_eq!(anon.aplicar("caso 123456789"), "caso [RADICADO]");
        assert_eq!(
            anon.aplicar("rad 200016001075202500111"),
            "rad [RADICADO]"
        );

        // Un radicado corto de 5 dígitos NO debe partir un teléfono ni un radicado no registrado más largo
        assert_eq!(anon.aplicar("cel 3001234567"), "cel [TELEFONO]");
        assert_eq!(anon.aplicar("cel 300 123 4567"), "cel [TELEFONO]");
        assert_eq!(anon.aplicar("fijo 6012345678"), "fijo [TELEFONO]");
        assert_eq!(anon.aplicar("fijo 601 234 5678"), "fijo [TELEFONO]");
        assert_eq!(anon.aplicar("wa 573101234567"), "wa [TELEFONO]");
        assert_eq!(anon.aplicar("cc 91234567"), "cc [NUMERO]");
        assert_eq!(
            anon.aplicar("otro 2000160010759999"),
            "otro [RADICADO]"
        );

        // Radicado envuelto en paréntesis cubre tanto la forma con paréntesis como la limpia
        assert_eq!(anon.aplicar("ver (200016009999) y 200016009999"), "ver [RADICADO] y [RADICADO]");

        // Radicado espaciado se reemplaza completo sin convertir su prefijo DANE 20001 en patrón aislado
        assert_eq!(
            anon.aplicar("rad 20001 60 01075 2025 00777 en codigo 20001"),
            "rad [RADICADO] en codigo 20001"
        );

        // Fechas, horas y textos vacíos se preservan intactos
        assert_eq!(
            anon.aplicar("Audiencia el 15-08-2025, 15/08/2025 y 15.08.2025 a las 08:30 y 14:45"),
            "Audiencia el 15-08-2025, 15/08/2025 y 15.08.2025 a las 08:30 y 14:45"
        );
        assert_eq!(anon.aplicar(""), "");
        assert_eq!(anon.aplicar("   "), "   ");

        // Anonimizador vacío sigue enmascarando teléfonos y números >= 6 dígitos
        let anon_vacio = Anonimizador::desde_radicados(&[]);
        assert_eq!(
            anon_vacio.aplicar("Tel 3109876543 cc 1065888999 fecha 15-08-2025"),
            "Tel [TELEFONO] cc [NUMERO] fecha 15-08-2025"
        );
    }

    #[test]
    fn test_anonimizador_tildes_nfd_limites_palabra_y_puntuacion_nombres() {
        let anon = Anonimizador::desde_radicados(&[
            "JOSÉ PÉREZ PEÑA AGÜERO - 200016001075",
            "JUAN-CARLOS MENDOZA - 200016001076",
            "MARÍA O'CONNOR - 200016001077",
            "J. PÉREZ - 200016001078",
            "M.A. GÓMEZ - 200016001079",
            "LOPEZ, FERNANDO - 200016001080",
            "MARÍA DE LOS ÁNGELES ORTEGA - 200016001081",
            "JOSE\u{0301} RAMI\u{0301}REZ-200016001082",
        ]);

        // Tildes NFC, sin tildes y Unicode NFD (combining marks) sin dejar acentos huérfanos
        assert_eq!(
            anon.aplicar("Audiencia de Jose Perez Pena Aguero"),
            "Audiencia de [PROCESADO]"
        );
        let nfd = "Audiencia de Jose\u{0301} Pe\u{0301}rez Pen\u{0303}a Agu\u{0308}ero y Jose\u{0301} Rami\u{0301}rez";
        assert_eq!(
            anon.aplicar(nfd),
            "Audiencia de [PROCESADO] y [PROCESADO]"
        );

        // Emoji y UTF-8 multibyte alrededor de nombres/radicados sin errores de frontera
        assert_eq!(
            anon.aplicar("🏛️ Audiencia «Pérez» (200016001075) — OK"),
            "🏛️ Audiencia «[PROCESADO]» ([RADICADO]) — OK"
        );

        // Límites de palabra estrictos (\b): no recorta subcadenas de otras palabras
        assert_eq!(
            anon.aplicar("El perezoso aperez juanito perez_1 y Pérez/Gómez"),
            "El perezoso aperez juanito perez_1 y [PROCESADO]/[PROCESADO]"
        );

        // Frontera de oración y coma no registrada no se tragan puntuación
        assert_eq!(
            anon.aplicar("Se citó a José. Pérez no asistió"),
            "Se citó a [PROCESADO]. [PROCESADO] no asistió"
        );
        assert_eq!(
            anon.aplicar("Citaron a José, Pérez no llegó"),
            "Citaron a [PROCESADO], [PROCESADO] no llegó"
        );

        // Nombres con guiones, apóstrofes, iniciales con punto, comas registradas y preposiciones
        assert_eq!(
            anon.aplicar("Asistió Juan-Carlos Mendoza, luego Juan Carlos Mendoza y Juan-Carlos"),
            "Asistió [PROCESADO], luego [PROCESADO] y [PROCESADO]"
        );
        assert_eq!(
            anon.aplicar("Asistió María O'Connor, luego O'Connor; uno o dos"),
            "Asistió [PROCESADO], luego [PROCESADO]; uno o dos"
        );
        assert_eq!(
            anon.aplicar("Asistió J. Pérez, J.Pérez y M.A. Gómez en J. 2 Penal"),
            "Asistió [PROCESADO], [PROCESADO] y [PROCESADO] en J. 2 Penal"
        );
        assert_eq!(
            anon.aplicar("Caso de López, Fernando; López no compareció"),
            "Caso de [PROCESADO]; [PROCESADO] no compareció"
        );
        assert_eq!(
            anon.aplicar("Audiencia de María de los Ángeles Ortega acerca de los hechos"),
            "Audiencia de [PROCESADO] acerca de los hechos"
        );
    }

    #[test]
    fn test_anonimizador_escudos_defensores_honorificos_coma_homonimo_y_cache_catalogos() {
        // Deduplicación masiva + variedad de casos de interacción entre procesados y defensores
        let mut rads = vec![
            "JUAN-CARLOS MENDOZA - 200016001075",
            "PEDRO DANIEL - 200016001076",
            "CARLOS PEREZ - 200016001077",
            "RODRIGUEZ - 200016001078",
            "ANA MORA - 200016001079",
        ];
        for _ in 0..200 {
            rads.push("CARLOS PEREZ - 200016001077");
        }
        let anon = Anonimizador::desde_radicados(&rads)
            .con_escudos_defensores(&[
                "Dra. Ana Pérez".into(),
                "Dr. José Daniel Martínez".into(),
                "Dr. Juan Carlos López".into(),
            ])
            .con_escudos_defensores(&[
                "Dr. Luis Rodríguez".into(),
                "Dra. Ana Mora".into(),
            ]);

        // 1) Todos los honoríficos y nombre completo sin honorífico quedan protegidos
        let txt1 = "La Dra. Ana Pérez, Defensora Pública Ana Pérez, Defensor Público Martínez, Abg. José Daniel, Lic. José-Daniel y José Daniel Martínez asistieron; Carlos Pérez y Pérez no.";
        let out1 = anon.aplicar(txt1);
        assert!(out1.contains("Dra. Ana Pérez"), "{}", out1);
        assert!(out1.contains("Defensora Pública Ana Pérez"), "{}", out1);
        assert!(out1.contains("Defensor Público Martínez"), "{}", out1);
        assert!(out1.contains("Abg. José Daniel"), "{}", out1);
        assert!(out1.contains("Lic. José-Daniel"), "{}", out1);
        assert!(out1.contains("José Daniel Martínez"), "{}", out1);
        assert!(!out1.contains("Carlos Pérez"), "{}", out1);
        assert!(out1.ends_with("[PROCESADO] y [PROCESADO] no."), "{}", out1);

        // 2) Sub-frase con guion (`JUAN-CARLOS`) NO pisa al defensor `Dr. Juan Carlos López`
        let out2 = anon.aplicar("Confirmó el Dr. Juan Carlos López que Juan-Carlos Mendoza faltó.");
        assert!(out2.contains("Dr. Juan Carlos López"), "{}", out2);
        assert!(!out2.contains("Juan-Carlos"), "{}", out2);
        assert!(!out2.contains("Mendoza"), "{}", out2);

        // 3) Procesado de 1 sola palabra (`RODRIGUEZ`) respeta escudo `Dr. Luis Rodríguez` pero redacta `Rodríguez` suelto
        let out3 = anon.aplicar("El Dr. Luis Rodríguez asistió por Rodríguez.");
        assert_eq!(out3, "El Dr. Luis Rodríguez asistió por [PROCESADO].");

        // 4) La coma después del honorífico bloquea el escudo y evita fuga del procesado `Daniel`
        let out4 = anon.aplicar("Confirmó el Dr. José, Daniel no asistió.");
        assert_eq!(out4, "Confirmó el Dr. José, [PROCESADO] no asistió.");

        // 5) Sub-frase sin honorífico (`José Daniel`) SÍ redacta el token compartido `Daniel`
        let out5 = anon.aplicar("José Daniel no compareció.");
        assert_eq!(out5, "José [PROCESADO] no compareció.");

        // 6) Homónimo exacto (`ANA MORA`) se redacta siempre para no filtrar PII de procesado
        let out6 = anon.aplicar("Audiencia del procesado Ana Mora.");
        assert_eq!(out6, "Audiencia del procesado [PROCESADO].");

        // 7) CacheCatalogoAnon conserva catálogos oficiales (incluso sin tildes/mayúsculas) y cachea personalizados
        let cache = CacheCatalogoAnon::new(&anon);
        assert_eq!(
            cache.anonimizar("Juzgado 1 Promiscuo Municipal de San Juan del Cesar"),
            "Juzgado 1 Promiscuo Municipal de San Juan del Cesar"
        );
        assert_eq!(
            cache.anonimizar("AUDIENCIA DE ACUSACION"),
            "AUDIENCIA DE ACUSACION"
        );
        let cust1 = cache.anonimizar("Juzgado Libre Carlos Pérez 200016001077");
        let cust2 = cache.anonimizar("Juzgado Libre Carlos Pérez 200016001077");
        assert_eq!(cust1, "Juzgado Libre [PROCESADO] [RADICADO]");
        assert_eq!(cust1, cust2);
        assert_eq!(cache.cache.borrow().len(), 3);
    }

    #[test]
    fn test_anonimizador_bordes_escudos_puntuacion_radicados_y_propiedades_fuzz() {
        let anon = Anonimizador::desde_radicados(&[
            "CARLOS O'CONNOR - 200016001075",
            "JUAN-CARLOS MENDOZA - 200016001076",
            "CARLOS PÉREZ - 200016001077",
            "PEDRO GÓMEZ - 200016001078",
            "LÓPEZ, FERNANDO - 200016001079",
            "ANA RUIZ - 123-4567",
            "SOFIA VARGAS - 2025-00123",
            "DIEGO - 08-2025",
            "20001 60 1075 2025 1234",
            "RAD 20001 60 01075 2025 00777",
            "SOFÍA O'BRIEN 200016001081",
            "PEREZ, CAMILO 200016001082",
            "LUIS-MIGUEL TORRES-200016001083",
            "JUAN - 2025 - 00123",
            "JUAN CARLOS - 200016001084",
            "SERGIO SALAZAR, 200016001085",
        ])
        .con_escudos_defensores(&[
            "Dra. María O'Connor".into(),
            "Dr. Juan-Carlos López".into(),
            "Dr. J. Pérez".into(),
            "Dra. M.A. Gómez".into(),
            "Licenciado Juan Pérez".into(),
            "Dr. Luis López".into(),
            "Dr. Fernando Gómez".into(),
            "Dr. Juan Carlos López".into(),
        ]);

        // 1) Sub-frases compuestas (`O'Connor`, `Juan-Carlos`) NO rompen escudos de defensores
        assert_eq!(
            anon.aplicar("La Dra. María O'Connor asistió por Carlos O'Connor y O'Connor."),
            "La Dra. María O'Connor asistió por [PROCESADO] y [PROCESADO]."
        );
        assert_eq!(
            anon.aplicar("El Dr. Juan-Carlos López asistió por Juan-Carlos Mendoza y Juan-Carlos."),
            "El Dr. Juan-Carlos López asistió por [PROCESADO] y [PROCESADO]."
        );

        // 2) Escudos con iniciales con punto (`Dr. J. Pérez`, `J. Pérez`, `Dra. M.A. Gómez`) y `Licenciado`
        assert_eq!(
            anon.aplicar("El Dr. J. Pérez y J. Pérez confirmaron el caso de Carlos Pérez."),
            "El Dr. J. Pérez y J. Pérez confirmaron el caso de [PROCESADO]."
        );
        assert_eq!(
            anon.aplicar("La Dra. M.A. Gómez y el Licenciado Pérez atendieron a Pedro Gómez."),
            "La Dra. M.A. Gómez y el Licenciado Pérez atendieron a [PROCESADO]."
        );
        // Un punto final de oración tras palabra normal NO filtra escudo hacia la siguiente oración
        assert_eq!(
            anon.aplicar("Asistió el Dr. Luis. Pérez no llegó."),
            "Asistió el Dr. Luis. [PROCESADO] no llegó."
        );

        // 3) Procesado con coma (`LÓPEZ, FERNANDO`) NO rompe escudos a izquierda ni a derecha de la coma
        assert_eq!(
            anon.aplicar("Confirmó el Dr. Luis López, Fernando no asistió."),
            "Confirmó el Dr. Luis López, [PROCESADO] no asistió."
        );
        assert_eq!(
            anon.aplicar("Faltó López, Fernando Gómez confirmó."),
            "Faltó [PROCESADO], Fernando Gómez confirmó."
        );
        // Frase parcial atrapada dentro de escudo más largo (`Dr. Juan Carlos López`) no rompe el escudo
        assert_eq!(
            anon.aplicar("El Dr. Juan Carlos López atendió a Juan Carlos."),
            "El Dr. Juan Carlos López atendió a [PROCESADO]."
        );

        // 4) Radicados con guion NO parten teléfonos, radicados largos, colas `-00` ni fechas
        assert_eq!(anon.aplicar("Tel 300-123-4567 para confirmar."), "Tel [TELEFONO] para confirmar.");
        assert_eq!(
            anon.aplicar("rad 11001-60-00015-2025-00123 pendiente."),
            "rad [RADICADO] pendiente."
        );
        assert_eq!(anon.aplicar("rad 2025-00123-00 pendiente."), "rad [NUMERO] pendiente.");
        assert_eq!(
            anon.aplicar("Fecha 15-08-2025 reprogramada para Juan."),
            "Fecha 15-08-2025 reprogramada para [PROCESADO]."
        );

        // 5) Radicados espaciados (con o sin etiqueta `RAD`) no convierten DANE `20001` en patrón suelto
        assert_eq!(
            anon.aplicar("Caso RAD 20001 60 01075 2025 00777 y 20001 60 1075 2025 1234; DANE 20001."),
            "Caso RAD [RADICADO] y [RADICADO]; DANE 20001."
        );

        // 6) Nombre + radicado sin ` - ` conserva subfrases/comas internas, sin habilitar coma final suelta
        assert_eq!(
            anon.aplicar("Audiencia de O'Brien, Perez, Camilo y Luis-Miguel; Sergio, Salazar."),
            "Audiencia de [PROCESADO], [PROCESADO] y [PROCESADO]; [PROCESADO], [PROCESADO]."
        );

        // 7) `anonimizar_correccion_con_global` clasifica radicados globales de 6-14 dígitos como `[RADICADO]`
        let corr = CorreccionIa {
            id: "c-glob".into(),
            registrada: "2026-01-01".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "El Dr. J. Pérez confundió 00123 con 200016001075 de Carlos O'Connor."
                .into(),
            prediccion: AudienciaInput {
                radicado: "JUAN - 00123".into(),
                tipo: "Acusación".into(),
                fecha: "15-08-2025".into(),
                hora: "08:00".into(),
                juzgado: "Juzgado 1".into(),
                realizada: "SI".into(),
                motivo: String::new(),
                observaciones: String::new(),
            },
            final_usuario: AudienciaInput {
                radicado: "JUAN - 00123".into(),
                tipo: "Acusación".into(),
                fecha: "15-08-2025".into(),
                hora: "08:00".into(),
                juzgado: "Juzgado 1".into(),
                realizada: "NO".into(),
                motivo: "Fiscalía".into(),
                observaciones: "Confirmó Dra. María O'Connor sobre 200016001075".into(),
            },
            campos: vec![],
            estado: "pendiente".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: "maria o connor".into(),
        };
        let ca = anonimizar_correccion_con_global(&corr, Some(&anon));
        assert_eq!(
            ca.texto_original,
            "El Dr. J. Pérez confundió [RADICADO] con [RADICADO] de [PROCESADO]."
        );
        assert_eq!(
            ca.final_usuario.observaciones,
            "Confirmó Dra. María O'Connor sobre [RADICADO]"
        );

        // 8) Propiedades de fuzz determinista (250 notas con tildes NFC/NFD, emojis, puntuación, teléfonos y fechas):
        //    sin pánicos, sin fugas PII, defensores/fechas intactos e idempotencia estricta
        let mut seed: u64 = 0x5EED_CAFE_1234_9876;
        let mut next_usize = |bound: usize| -> usize {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % bound
        };
        let defs_txt = [
            "Dra. María O'Connor",
            "Dr. Juan-Carlos López",
            "Dr. J. Pérez",
            "Dra. M.A. Gómez",
            "Licenciado Pérez",
        ];
        let procs_txt = [
            ("Carlos O'Connor", "oconnor"),
            ("Juan-Carlos Mendoza", "mendoza"),
            ("Lo\u{0301}pez, Fernando", "fernando"),
            ("Luis-Miguel Torres", "torres"),
            ("Sofi\u{0301}a O'Brien", "obrien"),
            ("Sergio Salazar", "salazar"),
        ];
        let rads_txt = [
            "200016001075",
            "200016001076",
            "2025-00123",
            "20001 60 01075 2025 00777",
            "00123",
        ];
        let tels_txt = ["3105559876", "300-123-4567", "300 456 7890", "573112223344"];
        let fechas_txt = ["15-08-2025", "01.12.2026", "20/05/2025"];
        let seps = [" ", " — ", "; ", " | ", " 🏛️ ", "\n", ", "];

        for _ in 0..250 {
            let def = defs_txt[next_usize(defs_txt.len())];
            let (proc_nom, _) = procs_txt[next_usize(procs_txt.len())];
            let rad = rads_txt[next_usize(rads_txt.len())];
            let tel = tels_txt[next_usize(tels_txt.len())];
            let fecha = fechas_txt[next_usize(fechas_txt.len())];
            let s1 = seps[next_usize(seps.len())];
            let s2 = seps[next_usize(seps.len())];
            let s3 = seps[next_usize(seps.len())];
            let nota = format!(
                "Audiencia el {}{}atendida por {}{}contra {} rad{}cel {} (DANE 20001).",
                fecha, s1, def, s2, proc_nom, rad, tel
            ) + s3;

            let out = anon.aplicar(&nota);
            assert!(out.contains(fecha), "Fecha perdida en: {} -> {}", nota, out);
            assert!(out.contains(def), "Defensor perdido en: {} -> {}", nota, out);
            assert!(out.contains("20001"), "DANE 20001 perdido en: {} -> {}", nota, out);
            assert!(!out.contains(rad), "Radicado filtrado en: {} -> {}", nota, out);
            assert!(!out.contains(tel), "Teléfono filtrado en: {} -> {}", nota, out);
            assert!(
                !out.to_lowercase().contains("mendoza")
                    && !out.to_lowercase().contains("salazar")
                    && !out.to_lowercase().contains("torres"),
                "Procesado filtrado en: {} -> {}",
                nota,
                out
            );
            // Idempotencia estricta
            assert_eq!(anon.aplicar(&out), out, "No idempotente: {}", out);
        }
    }

    #[test]
    fn test_diagnostico_y_exportacion_patrones_abreviacion_anonimizados_y_renombrado() {
        let dir = std::env::temp_dir().join(format!(
            "lokus_test_patrones_exp_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("ia_aprendizaje.json");
        let destino = dir.join("export");
        fs::create_dir_all(&destino).unwrap();

        let mut mem = MemoriaAprendizaje::default();
        // Registrar procesado real para comprobar que se anonimiza si apareciera en un alias
        mem.correcciones.push(CorreccionIa {
            id: "c-pii".into(),
            registrada: "2025-09-01T10:00:00-05:00".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "JPM1 audiencia de CARLOS MENDOZA".into(),
            prediccion: AudienciaInput {
                radicado: "CARLOS MENDOZA - 200016001075202500999".into(),
                tipo: "Audiencia preliminar".into(),
                fecha: "01/09/2025".into(),
                hora: "08:00".into(),
                juzgado: "JPM1".into(),
                realizada: "SI".into(),
                motivo: String::new(),
                observaciones: String::new(),
            },
            final_usuario: AudienciaInput {
                radicado: "CARLOS MENDOZA - 200016001075202500999".into(),
                tipo: "Audiencia preliminar".into(),
                fecha: "01/09/2025".into(),
                hora: "08:00".into(),
                juzgado: "Juzgado 1 de Carlos Mendoza".into(),
                realizada: "SI".into(),
                motivo: String::new(),
                observaciones: String::new(),
            },
            campos: vec![CampoCorregido {
                campo: "juzgado".into(),
                valor_ia: "JPM1".into(),
                valor_usuario: "Juzgado 1 de Carlos Mendoza".into(),
            }],
            estado: "analizada".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: "jose daniel".into(),
        });
        for n in [1u32, 2, 3] {
            mem.tabla_alias.push(AliasEstadistico {
                campo: "juzgado".into(),
                entrada_norm: format!("jpm{}", n),
                canonico: format!("Juzgado {} de Carlos Mendoza", n),
                alpha: 3.0,
                beta: 1.2,
                defensor_alcance: "jose daniel".into(),
                defensores_confirmantes: vec!["jose daniel".into()],
                ultima_actualizacion: "2025-09-01T10:00:00-05:00".into(),
            });
        }
        guardar_memoria_en(&ruta, &mem).unwrap();

        // 1) Diagnóstico incluye patrones inferidos anonimizados conservando `{N}`
        let diag = obtener_diagnostico_aprendizaje_en(&ruta);
        assert_eq!(diag.patrones_abreviacion.len(), 1, "{:?}", diag.patrones_abreviacion);
        let pat_diag = &diag.patrones_abreviacion[0];
        assert_eq!(pat_diag.patron_entrada, "jpm {N}");
        assert_eq!(pat_diag.defensor_alcance, "jose daniel");
        assert!(pat_diag.patron_canonico.contains("{N}"));
        assert!(
            !pat_diag.patron_canonico.to_lowercase().contains("mendoza"),
            "No debe fugarse PII en patrones del diagnóstico: {}",
            pat_diag.patron_canonico
        );

        // 2) Exportación JSON incluye `patrones_abreviacion` anonimizados
        let archivo = exportar_aprendizaje_completo_con_priors_en(
            &ruta,
            None,
            None,
            None,
            &destino,
            &[],
            &[],
            &["Dr. José Daniel".to_string()],
        )
        .unwrap();
        let contenido = fs::read_to_string(&archivo).unwrap();
        assert!(contenido.contains("\"patrones_abreviacion\""));
        assert!(contenido.contains("jpm {N}"));
        assert!(
            !contenido.to_lowercase().contains("mendoza"),
            "No debe fugarse PII en la exportación"
        );

        // 3) Renombrar defensor migra sus patrones inferidos al nuevo identificador
        migrar_defensor_en_memoria_en(&ruta, "Dr. José Daniel", "José Daniel Martínez").unwrap();
        let diag_ren = obtener_diagnostico_aprendizaje_en(&ruta);
        assert!(
            diag_ren
                .patrones_abreviacion
                .iter()
                .any(|p| p.defensor_alcance == "jose daniel martinez" && p.patron_entrada == "jpm {N}"),
            "El patrón debe migrar al renombrar defensor: {:?}",
            diag_ren.patrones_abreviacion
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_f2_exportacion_formato_v5_secuencia_instancia_raw_y_ejemplos_anonimizados() {
        use crate::uso_catalogos::{cargar_uso_sin_lock_en, NOMBRE_ARCHIVO_USO};

        let dir = preparar_dir("f2_export_v5");
        let ruta_mem = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let ruta_uso = dir.join(NOMBRE_ARCHIVO_USO);
        let descargas = dir.join("descargas");

        let mut uso = UsoCatalogos::default();
        uso.version = 1;
        uso.instancia_id = "inst-f2-test".into();
        uso.sembrado_inicial = true;
        uso.total_audiencias_contabilizadas = 3;
        uso.pendientes_compartir = 2;
        uso.fuentes_importadas.insert("inst-peer-b".into());
        uso.fuentes_importadas.insert("inst-peer-a".into());
        fs::write(&ruta_uso, serde_json::to_string_pretty(&uso).unwrap()).unwrap();

        let mut mem = MemoriaAprendizaje::default();
        mem.aciertos = 1;
        mem.preferencias_estilo.incluir_reprogramacion_beta = 4.2;
        let mut pref_def = PreferenciasEstilo::default();
        pref_def.instalada_reprogramada_si_alpha = 6.5;
        mem.preferencias_por_defensor
            .insert("Dr. José Daniel".into(), pref_def);
        mem.tabla_alias = vec![
            AliasEstadistico {
                campo: "juzgado".into(),
                entrada_norm: "juzgado libre carlos mendoza".into(),
                canonico: "Juzgado Libre Carlos Mendoza".into(),
                alpha: 1.5, // q10 < 0.30 (solo candidato, omitido en alias_validados pero conservado en tabla_alias_raw)
                beta: 1.2,
                defensor_alcance: "Dr. José Daniel".into(),
                defensores_confirmantes: vec!["Dr. José Daniel".into()],
                ultima_actualizacion: "2026-11-23T10:00:00-05:00".into(),
            },
            AliasEstadistico {
                campo: "juzgado".into(),
                entrada_norm: "jpm1 san alberto".into(),
                canonico: "Juzgado 1 Promiscuo Municipal de San Alberto".into(),
                alpha: 4.0,
                beta: 1.2,
                defensor_alcance: "*".into(),
                defensores_confirmantes: vec!["jose daniel".into()],
                ultima_actualizacion: "2026-11-23T10:05:00-05:00".into(),
            },
        ];
        mem.ejemplos_aciertos.push(EjemploPositivo {
            id: "pos-f2".into(),
            registrada: "2026-11-23T10:00:00-05:00".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_anonimizado: "Audiencia de CARLOS MENDOZA rad 200016001075202500999 cel 3105559988".into(),
            salida_anonimizada: AudienciaInput {
                radicado: "CARLOS MENDOZA - 200016001075202500999".into(),
                tipo: "Audiencia de acusación".into(),
                fecha: "23/11/2026".into(),
                hora: "09:00".into(),
                juzgado: "Juzgado 1 Promiscuo Municipal de San Alberto".into(),
                realizada: "SI".into(),
                motivo: String::new(),
                observaciones: "Asistió CARLOS MENDOZA cel 3105559988".into(),
            },
            reglas_inyectadas: vec!["base-1".into()],
            defensor_clave: "Dr. José Daniel".into(),
        });
        mem.correcciones.push(CorreccionIa {
            id: "corr-f2".into(),
            registrada: "2026-11-23T10:10:00-05:00".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "Caso CARLOS MENDOZA 200016001075202500999".into(),
            prediccion: AudienciaInput {
                radicado: "CARLOS MENDOZA - 200016001075202500999".into(),
                ..audiencia_base()
            },
            final_usuario: AudienciaInput {
                radicado: "CARLOS MENDOZA - 200016001075202500999".into(),
                ..audiencia_base()
            },
            campos: vec![],
            estado: "pendiente".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: "jose daniel".into(),
        });
        guardar_memoria_en(&ruta_mem, &mem).unwrap();

        let arch1 = exportar_aprendizaje_completo_con_priors_en(
            &ruta_mem,
            Some(&ruta_uso),
            None,
            None,
            &descargas,
            &[],
            &[],
            &["Dr. José Daniel".into()],
        )
        .unwrap();
        let p1: PaqueteExportacion =
            serde_json::from_str(&fs::read_to_string(&arch1).unwrap()).unwrap();
        assert_eq!(p1.formato, 5);
        assert_eq!(p1.instancia_id, "inst-f2-test");
        assert_eq!(p1.exportacion_seq, 1);
        assert!(p1.exportacion_id.starts_with("exp-"));
        assert_eq!(p1.ultima_exportacion_previa, "");
        assert_eq!(
            p1.fuentes_importadas,
            vec!["inst-peer-a".to_string(), "inst-peer-b".to_string()]
        );
        assert_eq!(p1.alias_validados.len(), 1);
        assert_eq!(p1.tabla_alias_raw.len(), 2);
        let raw_bajo = p1
            .tabla_alias_raw
            .iter()
            .find(|a| a.defensor_alcance == "jose daniel")
            .expect("alias de bajo q10 debe estar en tabla_alias_raw");
        assert!((raw_bajo.alpha - 1.5).abs() < 1e-9);
        assert!((raw_bajo.beta - 1.2).abs() < 1e-9);
        assert!(!raw_bajo.canonico.to_lowercase().contains("mendoza"));
        assert!(p1
            .tabla_alias_raw
            .iter()
            .any(|a| a.canonico == "Juzgado 1 Promiscuo Municipal de San Alberto"));
        assert!((p1.preferencias_raw.as_ref().unwrap().incluir_reprogramacion_beta - 4.2).abs() < 1e-9);
        assert!(
            (p1.preferencias_por_defensor_raw["jose daniel"].instalada_reprogramada_si_alpha - 6.5)
                .abs()
                < 1e-9
        );
        assert_eq!(p1.total_ejemplos_aciertos, 1);
        assert_eq!(p1.ejemplos_aciertos.len(), 1);
        let ej_str = serde_json::to_string(&p1.ejemplos_aciertos).unwrap();
        assert!(!ej_str.to_lowercase().contains("mendoza"), "{}", ej_str);
        assert!(!ej_str.contains("200016001075202500999"), "{}", ej_str);
        assert!(!ej_str.contains("3105559988"), "{}", ej_str);
        assert!(ej_str.contains("Juzgado 1 Promiscuo Municipal de San Alberto"));
        assert_eq!(cargar_uso_sin_lock_en(&ruta_uso).pendientes_compartir, 0);

        let arch2 = exportar_aprendizaje_completo_con_priors_en(
            &ruta_mem,
            Some(&ruta_uso),
            None,
            None,
            &descargas,
            &[],
            &[],
            &["Dr. José Daniel".into()],
        )
        .unwrap();
        let p2: PaqueteExportacion =
            serde_json::from_str(&fs::read_to_string(&arch2).unwrap()).unwrap();
        assert_eq!(p2.exportacion_seq, 2);
        assert_eq!(p2.ultima_exportacion_previa, p1.exportado);
        assert_eq!(p2.instancia_id, "inst-f2-test");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_f2_exportacion_error_escritura_no_corrompe_secuencia_ni_pendientes() {
        use crate::uso_catalogos::{cargar_uso_sin_lock_en, NOMBRE_ARCHIVO_USO};

        let dir = preparar_dir("f2_export_err");
        let ruta_mem = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let ruta_uso = dir.join(NOMBRE_ARCHIVO_USO);
        let archivo_como_carpeta = dir.join("no_es_directorio");
        fs::write(&archivo_como_carpeta, b"bloqueo").unwrap();

        let mut uso = UsoCatalogos::default();
        uso.version = 1;
        uso.instancia_id = "inst-err".into();
        uso.total_audiencias_contabilizadas = 1;
        uso.pendientes_compartir = 2;
        fs::write(&ruta_uso, serde_json::to_string_pretty(&uso).unwrap()).unwrap();

        let mut mem = MemoriaAprendizaje::default();
        mem.aciertos = 1;
        mem.correcciones.push(CorreccionIa {
            id: "corr-err".into(),
            registrada: "2026-11-23T10:00:00-05:00".into(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "Texto".into(),
            prediccion: audiencia_base(),
            final_usuario: audiencia_base(),
            campos: vec![],
            estado: "pendiente".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: String::new(),
        });
        guardar_memoria_en(&ruta_mem, &mem).unwrap();

        let res = exportar_aprendizaje_completo_con_priors_en(
            &ruta_mem,
            Some(&ruta_uso),
            None,
            None,
            &archivo_como_carpeta,
            &[],
            &[],
            &[],
        );
        assert!(res.is_err());

        let mem_post = cargar_memoria_en(&ruta_mem);
        assert_eq!(mem_post.exportacion_seq, 0);
        assert_eq!(mem_post.ultima_exportacion_previa, "");
        assert!(!mem_post.correcciones[0].exportada);
        assert_eq!(cargar_uso_sin_lock_en(&ruta_uso).pendientes_compartir, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_f2_deserializacion_retrocompatible_formato_4_y_migracion_memoria() {
        let json_v4 = r#"{
          "app": "Lokus",
          "version_app": "5.1.0",
          "formato": 4,
          "exportado": "2026-11-23T10:29:16.716664800-05:00",
          "aciertos": 71,
          "total_reglas": 1,
          "total_correcciones": 1,
          "total_ejemplos_aciertos": 71,
          "perfiles_defensores": 1,
          "llamadas_induccion_llm_total": 11,
          "llamadas_extraccion_total": 99,
          "llamadas_consolidacion_total": 6,
          "correcciones_por_campo": { "motivo": 1 },
          "metricas_por_campo": [{
            "campo": "motivo",
            "evaluaciones": 10,
            "aciertos": 9,
            "correcciones": 1,
            "precision_ewma": 0.9,
            "confianza_q10": 0.7,
            "drift_activo": false
          }],
          "metricas_por_modelo": [{
            "clave": "openai:gpt-5-mini",
            "evaluaciones": 10,
            "aciertos": 9,
            "correcciones": 1,
            "precision_media": 0.9
          }],
          "preferencias_estilo": {
            "incluir_causas_secundarias_prob": 0.5,
            "incluir_reprogramacion_prob": 0.75,
            "se_fija_aplazamiento_prob": 0.75,
            "instalada_reprogramada_si_prob": 0.5
          },
          "preferencias_por_defensor": {},
          "defensores_directorio": [{
            "clave": "jose daniel",
            "nombre": "Dr. José Daniel",
            "audiencias_excel": 10,
            "aciertos_ia": 5,
            "correcciones_ia": 1,
            "reglas_propias": 0
          }],
          "resumen_patrones_generales": {
            "total_defensores_activos": 1,
            "top_juzgados_globales": [],
            "top_tipos_globales": [],
            "top_horas_globales": [],
            "reglas_transversales_promovidas": 0,
            "errores_recurrentes_transversales": []
          },
          "grafo_asociaciones": {},
          "perfiles_sinteticos": [],
          "consolidaciones_ia": [],
          "alias_validados": [{
            "campo": "juzgado",
            "entrada_norm": "j1pm san alberto",
            "canonico": "Juzgado 1 Promiscuo Municipal de San Alberto",
            "defensor_alcance": "*",
            "confianza_q10": 0.42
          }],
          "patrones_abreviacion": [],
          "juzgados_agregados_usuario": [],
          "tipos_audiencia_agregados_usuario": [],
          "uso_catalogos": {
            "total_audiencias_contabilizadas": 12,
            "sembrado_inicial_completado": true,
            "sembrado_en": "2026-11-01T10:00:00-05:00",
            "ultima_actualizacion": "2026-11-23T10:00:00-05:00",
            "tipos_audiencia": {
              "total_audiencias": 12,
              "total_con_valor": 12,
              "total_sin_especificar": 0,
              "porcentaje_sin_especificar": 0.0,
              "total_opciones_predeterminadas": 16,
              "total_opciones_lista_usuario": 0,
              "total_opciones_otra": 0,
              "total_opciones_eliminadas_tras_uso": 0,
              "nuevos_no_predeterminados": [],
              "predeterminados_sin_uso": [],
              "agregados_usuario_sin_uso": [],
              "detalle": []
            },
            "juzgados": {
              "total_audiencias": 12,
              "total_con_valor": 12,
              "total_sin_especificar": 0,
              "porcentaje_sin_especificar": 0.0,
              "total_opciones_predeterminadas": 97,
              "total_opciones_lista_usuario": 0,
              "total_opciones_otra": 0,
              "total_opciones_eliminadas_tras_uso": 0,
              "nuevos_no_predeterminados": [],
              "predeterminados_sin_uso": [],
              "agregados_usuario_sin_uso": [],
              "detalle": []
            },
            "horas": {
              "total_audiencias": 12,
              "total_con_hora": 12,
              "total_sin_hora": 0,
              "porcentaje_sin_hora": 0.0,
              "horas_rapidas_predeterminadas": [],
              "horas_rapidas_sin_uso": [],
              "horas_mas_frecuentes_no_rapidas": [],
              "detalle_por_hora_exacta": [],
              "resumen_por_franja_horaria": []
            }
          },
          "reglas": [{
            "id": "r-v4",
            "tipo_error": "motivo",
            "regla": "Si no notificaron al procesado marcar Juez",
            "refuerzos": 2,
            "actualizada": "2026-11-23T10:00:00-05:00",
            "estado": "activa",
            "campo_objetivo": "motivo",
            "triggers": ["notificaron", "procesado"],
            "alpha": 3.0,
            "beta": 3.0,
            "impresiones": 1,
            "aciertos_directos": 1,
            "fallos_directos": 0,
            "origen": "mitigacion_modelo",
            "modelo_alcance": "*",
            "defensor_alcance": "*",
            "defensores_confirmantes": [],
            "ultima_actualizacion": "2026-11-23T10:00:00-05:00",
            "confianza_llm": 0.9,
            "aciertos_sombra": 0,
            "confirmaciones_sombra": 0,
            "origen_correccion_id": ""
          }],
          "correcciones": []
        }"#;

        let paq: PaqueteExportacion = serde_json::from_str(json_v4).unwrap();
        assert_eq!(paq.version_app, "5.1.0");
        assert_eq!(paq.formato, 4);
        assert_eq!(paq.aciertos, 71);
        assert_eq!(paq.instancia_id, "");
        assert_eq!(paq.exportacion_seq, 0);
        assert_eq!(paq.exportacion_id, "");
        assert_eq!(paq.ultima_exportacion_previa, "");
        assert!(paq.fuentes_importadas.is_empty());
        assert!(paq.tabla_alias_raw.is_empty());
        assert!(paq.preferencias_raw.is_none());
        assert!(paq.preferencias_por_defensor_raw.is_empty());
        assert!(paq.ejemplos_aciertos.is_empty());

        let dir = preparar_dir("f2_migracion_v4_v5");
        let ruta_mem = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        fs::write(&ruta_mem, r#"{"version": 4, "aciertos": 9}"#).unwrap();
        let mem = cargar_memoria_en(&ruta_mem);
        assert_eq!(mem.version, VERSION_FORMATO);
        assert_eq!(mem.aciertos, 9);
        assert_eq!(mem.exportacion_seq, 0);
        assert_eq!(mem.ultima_exportacion_previa, "");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_f3a_anonimizador_protege_municipios_juzgados_y_terminos_judiciales_sin_fugas() {
        let anon = Anonimizador::desde_radicados(&[
            "ALBERTO CESAR PEÑA ARMAS - 200016001075202500111",
            "FUENTES CAMPO - 200016001075202500222",
            "CARLOS MENDOZA - 200016001075202500333",
        ]);
        let cache = CacheCatalogoAnon::new(&anon);

        assert_eq!(
            cache.anonimizar("Juzgado 1 Promiscuo Municipal de San Alberto"),
            "Juzgado 1 Promiscuo Municipal de San Alberto"
        );
        assert_eq!(
            cache.anonimizar("Juzgado 2 Penal del Circuito de San Juan del Cesar"),
            "Juzgado 2 Penal del Circuito de San Juan del Cesar"
        );
        assert_eq!(
            anon.aplicar("Despacho Judicial: Juzgado Promiscuo Municipal de San Alberto - Cesar, con funciones de conocimiento."),
            "Despacho Judicial: Juzgado Promiscuo Municipal de San Alberto - Cesar, con funciones de conocimiento."
        );
        assert_eq!(
            anon.aplicar("juzgado 1 promiscuo municipal san alberto"),
            "juzgado 1 promiscuo municipal san alberto"
        );
        assert_eq!(
            anon.aplicar("una pena de 55 meses de prisión por PORTE DE ARMAS DE FUEGO; no completes ese campo en las fuentes"),
            "una pena de 55 meses de prisión por PORTE DE ARMAS DE FUEGO; no completes ese campo en las fuentes"
        );

        assert_eq!(
            anon.aplicar("Audiencia contra Alberto Cesar; Carlos Mendoza no asistió."),
            "Audiencia contra [PROCESADO]; [PROCESADO] no asistió."
        );
        assert_eq!(
            cache.anonimizar("Juzgado 1 de Carlos Mendoza"),
            "Juzgado 1 de [PROCESADO]"
        );
        assert_eq!(
            cache.anonimizar("Juzgado Libre CARLOS 123456"),
            "Juzgado Libre [PROCESADO] [NUMERO]"
        );
    }

    #[test]
    fn test_f3b_campo_objetivo_desde_tipo_error_y_triggers_sin_ruido() {
        let dir = preparar_dir("f3b_campo_y_triggers");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);

        let pred = AudienciaInput {
            radicado: "CARLOS - 200016001075202600001".into(),
            tipo: "Audiencia de imputación".into(),
            fecha: "23/09/2026".into(),
            hora: "09:00".into(),
            juzgado: "J2PCA".into(),
            realizada: "NO".into(),
            motivo: "Usuario".into(),
            observaciones: "No compareció".into(),
        };
        let fin = AudienciaInput {
            radicado: "CARLOS - 200016001075202600001".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "23/09/2026".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 2 Penal del Circuito de Valledupar".into(),
            realizada: "NO".into(),
            motivo: "Juez".into(),
            observaciones: "Reprogramada porque el despacho no notificó al procesado.".into(),
        };

        let (_, c1) = registrar_resultado_en(
            &ruta,
            "1. Proceso bajo radicado 20001-60-01075-2026-00001 del 23 de septiembre de 2026 en J2PCA mpal: no notificaron al procesado [PROCESADO]",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let id1 = c1.unwrap().id;

        aplicar_analisis_en(
            &ruta,
            &id1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "motivo".into(),
                regla: "Si no notificaron al procesado marcar Juez".into(),
                confianza: 0.9,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        let mem1 = cargar_memoria_en(&ruta);
        assert_eq!(mem1.reglas[0].campo_objetivo, "motivo");
        let tr = &mem1.reglas[0].triggers;
        for ruido in ["20001", "2026", "23", "septiembre", "proceso", "bajo", "radicado", "[procesado]"] {
            assert!(!tr.contains(&ruido.to_string()), "trigger ruidoso {}: {:?}", ruido, tr);
        }
        assert!(tr.contains(&"j2pca".to_string()), "{:?}", tr);
        assert!(tr.contains(&"mpal".to_string()), "{:?}", tr);
        assert!(tr.contains(&"notificaron".to_string()), "{:?}", tr);
        assert!(tr.contains(&"procesado".to_string()), "{:?}", tr);

        // Actualización en sitio de regla no-base actualiza campo_objetivo desde tipo_error
        aplicar_analisis_en(
            &ruta,
            &id1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "estilo_observaciones".into(),
                regla: "Indicar cuando el despacho no notificó al procesado".into(),
                confianza: 0.92,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        let mem2 = cargar_memoria_en(&ruta);
        assert_eq!(mem2.reglas[0].campo_objetivo, "observaciones");

        // Fallback a campos.first() únicamente cuando tipo_error es "otro"/"general"
        let (_, c2) = registrar_resultado_en(
            &ruta,
            "Otro caso distinto en J2PCA",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let id2 = c2.unwrap().id;
        aplicar_analisis_en(
            &ruta,
            &id2,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "otro".into(),
                regla: "Regla general distinta para fallback de campo".into(),
                confianza: 0.85,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        let mem3 = cargar_memoria_en(&ruta);
        assert_eq!(mem3.reglas[1].campo_objetivo, "tipo");

        // Clúster multi-campo con tipo_error = "motivo" asigna campo_objetivo = "motivo"
        let (_, c3) = registrar_resultado_en(&ruta, "Cluster texto 1", &pred, &fin, "openai", "gpt-5-mini").unwrap();
        let (_, c4) = registrar_resultado_en(&ruta, "Cluster texto 2", &pred, &fin, "openai", "gpt-5-mini").unwrap();
        aplicar_analisis_cluster_en(
            &ruta,
            "clu-test",
            &[c3.unwrap().id, c4.unwrap().id],
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "motivo".into(),
                regla: "Regla de clúster sobre motivo de no realización".into(),
                confianza: 0.9,
                necesita_aclaracion: false,
                pregunta: String::new(),
            },
        )
        .unwrap();
        let mem4 = cargar_memoria_en(&ruta);
        assert_eq!(mem4.reglas.last().unwrap().campo_objetivo, "motivo");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_f3c_preferencia_reprogramacion_se_adapta_sin_doble_conteo() {
        let dir = preparar_dir("f3c_reprogramacion");
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);

        let frases_ia = [
            "No compareció la Fiscalía. Se fijó nueva fecha para el 20/11/2026.",
            "Juez sin conexión. Próxima fecha: 22 de enero de 2027.",
            "Usuario no asistió. Se señaló nuevamente para el 05/12/2026.",
            "Aplazada por Fiscalía. Se fijó su continuación para el 10/12/2026.",
        ];
        for (i, obs_ia) in frases_ia.iter().enumerate() {
            let mut pred = audiencia_base();
            pred.realizada = "NO".into();
            pred.motivo = "Fiscalía".into();
            pred.observaciones = (*obs_ia).to_string();

            let mut fin = pred.clone();
            fin.observaciones = "La audiencia se reprogramó por Fiscalía.".into();

            registrar_resultado_completo_en(
                &ruta,
                &format!("Caso {} fiscalía fallida", i),
                &pred,
                &fin,
                "openai",
                "gpt-5-mini",
                &[],
                Some("Dra. Ana Pérez"),
                None,
            )
            .unwrap();
        }

        let mem = cargar_memoria_en(&ruta);
        let p_def = &mem.preferencias_por_defensor["ana perez"];
        let prob_def = p_def.incluir_reprogramacion_alpha
            / (p_def.incluir_reprogramacion_alpha + p_def.incluir_reprogramacion_beta);
        assert!(p_def.incluir_reprogramacion_beta > p_def.incluir_reprogramacion_alpha);
        assert!(prob_def < 0.45, "prob_def={}", prob_def);
        assert!(mem.preferencias_estilo.incluir_reprogramacion_beta > 1.0);

        // Explicación del resultado actual (sin fecha futura) conservada NO infla alpha
        let alpha_antes = p_def.incluir_reprogramacion_alpha;
        let mut pred_res = audiencia_base();
        pred_res.realizada = "NO".into();
        pred_res.motivo = "Fiscalía".into();
        pred_res.observaciones = "La audiencia se reprogramó por Fiscalía.".into();
        registrar_resultado_completo_en(
            &ruta,
            "Caso resultado sin fecha futura",
            &pred_res,
            &pred_res,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
            None,
        )
        .unwrap();
        let mem2 = cargar_memoria_en(&ruta);
        assert!(
            (mem2.preferencias_por_defensor["ana perez"].incluir_reprogramacion_alpha - alpha_antes)
                .abs()
                < 1e-9
        );

        // Con el marcador literal "Reprogramada para:", solo estadistica_ia actualiza (sin doble conteo)
        let beta_antes = mem2.preferencias_por_defensor["ana perez"].incluir_reprogramacion_beta;
        let mut pred_lit = audiencia_base();
        pred_lit.realizada = "NO".into();
        pred_lit.motivo = "Fiscalía".into();
        pred_lit.observaciones = "Reprogramada para: 15/12/2026".into();
        let mut fin_lit = pred_lit.clone();
        fin_lit.observaciones = "Se solicitó reprogramación.".into();
        registrar_resultado_completo_en(
            &ruta,
            "Caso literal reprogramada para",
            &pred_lit,
            &fin_lit,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
            None,
        )
        .unwrap();
        let mem3 = cargar_memoria_en(&ruta);
        let beta_despues = mem3.preferencias_por_defensor["ana perez"].incluir_reprogramacion_beta;
        assert!(
            (beta_despues - (beta_antes + 1.0)).abs() < 1e-9,
            "antes={}, despues={}",
            beta_antes,
            beta_despues
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sincronizacion_ia_bidireccional_anti_ping_pong_pc_celular_y_tres_equipos() {
        let dir = preparar_dir("sync_ping_pong");
        let ruta_pc = dir.join("pc_ia.json");
        let ruta_cel = dir.join("cel_ia.json");
        let ruta_pc2 = dir.join("pc2_ia.json");

        // 1) PC acumula 2 aciertos + 1 corrección + alias + perfil sintético
        let base_pc = audiencia_base();
        registrar_resultado_completo_en(
            &ruta_pc,
            "Audiencia concentrada 09:00 J01",
            &base_pc,
            &base_pc,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
            None,
        )
        .unwrap();
        registrar_resultado_completo_en(
            &ruta_pc,
            "Imputación J02 10:00",
            &base_pc,
            &base_pc,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
            None,
        )
        .unwrap();
        let mut mem_pc = preparar_snapshot_memoria_para_paquete_en(&ruta_pc).unwrap();
        mem_pc.reglas.push(ReglaAprendida {
            id: "reg-pc-1".into(),
            tipo_error: "otro".into(),
            regla: "Conservar siglas J01 PMG en observaciones".into(),
            refuerzos: 3,
            actualizada: "2026-11-20T10:00:00-05:00".into(),
            estado: "activa".into(),
            campo_objetivo: "juzgado".into(),
            triggers: vec!["j01".into()],
            alpha: 5.0,
            beta: 1.0,
            impresiones: 4,
            aciertos_directos: 4,
            fallos_directos: 0,
            origen: "induccion_llm".into(),
            modelo_alcance: "*".into(),
            defensor_alcance: "ana perez".into(),
            defensores_confirmantes: vec!["ana perez".into()],
            ultima_actualizacion: "2026-11-20T10:00:00-05:00".into(),
            confianza_llm: 0.9,
            aciertos_sombra: 0,
            confirmaciones_sombra: 0,
            origen_correccion_id: String::new(),
        });
        mem_pc.tabla_alias.push(AliasEstadistico {
            campo: "juzgado".into(),
            entrada_norm: "j1 pmg".into(),
            canonico: "Juzgado 1 Penal Municipal".into(),
            alpha: 4.0,
            beta: 1.2,
            defensor_alcance: "ana perez".into(),
            defensores_confirmantes: vec!["ana perez".into()],
            ultima_actualizacion: "2026-11-20T10:00:00-05:00".into(),
        });
        mem_pc.perfiles_sinteticos.insert(
            "ana perez".into(),
            PerfilSinteticoDefensor {
                ambito: "ana perez".into(),
                nombre_mostrar: "Dra. Ana Pérez".into(),
                resumen: "Usa siglas cortas J01 PMG".into(),
                version: 2,
                ultima_actualizacion: "2026-11-20T10:00:00-05:00".into(),
            },
        );
        guardar_memoria_en(&ruta_pc, &mem_pc).unwrap();

        // 2) Celular ya tiene 1 acierto propio + 1 regla propia
        registrar_resultado_completo_en(
            &ruta_cel,
            "Control garantías 14:00",
            &base_pc,
            &base_pc,
            "gemini",
            "gemini-2.5-flash",
            &[],
            Some("Dr. Luis Mora"),
            None,
        )
        .unwrap();
        let mut mem_cel = preparar_snapshot_memoria_para_paquete_en(&ruta_cel).unwrap();
        mem_cel.reglas.push(ReglaAprendida {
            id: "reg-cel-1".into(),
            tipo_error: "motivo".into(),
            regla: "Si no hay fiscalía presencial marcar motivo Fiscalía".into(),
            refuerzos: 2,
            actualizada: "2026-11-21T09:00:00-05:00".into(),
            estado: "prueba".into(),
            campo_objetivo: "motivo".into(),
            triggers: vec!["fiscal".into()],
            alpha: 3.0,
            beta: 2.0,
            impresiones: 2,
            aciertos_directos: 2,
            fallos_directos: 0,
            origen: "induccion_llm".into(),
            modelo_alcance: "*".into(),
            defensor_alcance: "*".into(),
            defensores_confirmantes: vec!["luis mora".into()],
            ultima_actualizacion: "2026-11-21T09:00:00-05:00".into(),
            confianza_llm: 0.85,
            aciertos_sombra: 0,
            confirmaciones_sombra: 0,
            origen_correccion_id: String::new(),
        });
        guardar_memoria_en(&ruta_cel, &mem_cel).unwrap();

        // Ronda 1: PC -> Celular
        let snap_pc_1 = preparar_snapshot_memoria_para_paquete_en(&ruta_pc).unwrap();
        let res_1 = fusionar_memoria_ia_desde_paquete_en(&ruta_cel, &snap_pc_1).unwrap();
        assert_eq!(res_1.reglas_nuevas, 1);
        assert!(res_1.alias_nuevos >= 1);
        assert_eq!(res_1.perfiles_actualizados, 1);

        let cel_tras_1 = cargar_memoria_en(&ruta_cel);
        assert_eq!(cel_tras_1.aciertos, 3); // 2 de PC + 1 de Celular
        assert_eq!(cel_tras_1.llamadas_extraccion_total, 3);
        assert_eq!(cel_tras_1.reglas.len(), 2);
        let cnt_alias_j1 = cel_tras_1
            .tabla_alias
            .iter()
            .filter(|a| a.entrada_norm == "j1 pmg")
            .count();
        assert_eq!(cnt_alias_j1, 1);

        // Ronda 2 (ping-pong): Celular -> PC
        let snap_cel_1 = preparar_snapshot_memoria_para_paquete_en(&ruta_cel).unwrap();
        let res_2 = fusionar_memoria_ia_desde_paquete_en(&ruta_pc, &snap_cel_1).unwrap();
        assert_eq!(res_2.reglas_nuevas, 1);

        let pc_tras_2 = cargar_memoria_en(&ruta_pc);
        assert_eq!(pc_tras_2.aciertos, 3);
        assert_eq!(pc_tras_2.llamadas_extraccion_total, 3);
        assert_eq!(pc_tras_2.reglas.len(), 2);

        // Ronda 3 (ping-pong sin cambios nuevos): PC -> Celular y Celular -> PC otra vez
        let snap_pc_2 = preparar_snapshot_memoria_para_paquete_en(&ruta_pc).unwrap();
        let insp_rep = inspeccionar_fusion_memoria_ia_en(&ruta_cel, &snap_pc_2);
        assert!(insp_rep.ya_sincronizado);
        let res_3 = fusionar_memoria_ia_desde_paquete_en(&ruta_cel, &snap_pc_2).unwrap();
        assert_eq!(res_3.cambios_totales, 0);

        let snap_cel_2 = preparar_snapshot_memoria_para_paquete_en(&ruta_cel).unwrap();
        let res_4 = fusionar_memoria_ia_desde_paquete_en(&ruta_pc, &snap_cel_2).unwrap();
        assert_eq!(res_4.cambios_totales, 0);

        let pc_estable = cargar_memoria_en(&ruta_pc);
        let cel_estable = cargar_memoria_en(&ruta_cel);
        assert_eq!(pc_estable.aciertos, 3);
        assert_eq!(cel_estable.aciertos, 3);
        assert_eq!(pc_estable.reglas.len(), 2);
        assert_eq!(cel_estable.reglas.len(), 2);
        let reg_pc_1_a = pc_estable.reglas.iter().find(|r| r.id == "reg-pc-1").unwrap();
        let reg_pc_1_b = cel_estable.reglas.iter().find(|r| r.id == "reg-pc-1").unwrap();
        assert_eq!(reg_pc_1_a.refuerzos, 3);
        assert_eq!(reg_pc_1_b.refuerzos, 3);
        assert!((reg_pc_1_a.alpha - 5.0).abs() < 1e-9);
        assert!((reg_pc_1_b.alpha - 5.0).abs() < 1e-9);

        // Ronda 4: PC suma +1 acierto nuevo -> Celular recibe exactamente +1 (total 4, no doble conteo)
        registrar_resultado_completo_en(
            &ruta_pc,
            "Acusación 15:00",
            &base_pc,
            &base_pc,
            "openai",
            "gpt-5-mini",
            &[],
            Some("Dra. Ana Pérez"),
            None,
        )
        .unwrap();
        let snap_pc_3 = preparar_snapshot_memoria_para_paquete_en(&ruta_pc).unwrap();
        fusionar_memoria_ia_desde_paquete_en(&ruta_cel, &snap_pc_3).unwrap();
        let cel_final = cargar_memoria_en(&ruta_cel);
        assert_eq!(cel_final.aciertos, 4);
        assert_eq!(cel_final.llamadas_extraccion_total, 4);

        // Tercer equipo (PC <-> PC2) converge igual e incluye instancias_sincronizadas en telemetría
        let mut mem_pc2 = MemoriaAprendizaje::default();
        incrementar_aciertos(&mut mem_pc2, 2);
        incrementar_llamadas_extraccion(&mut mem_pc2, 2);
        guardar_memoria_en(&ruta_pc2, &mem_pc2).unwrap();
        let snap_pc2 = preparar_snapshot_memoria_para_paquete_en(&ruta_pc2).unwrap();
        fusionar_memoria_ia_desde_paquete_en(&ruta_cel, &snap_pc2).unwrap();
        let cel_con_3_equipos = cargar_memoria_en(&ruta_cel);
        assert_eq!(cel_con_3_equipos.aciertos, 6); // 3 PC + 1 Celular + 2 PC2
        assert_eq!(cel_con_3_equipos.instancias_sincronizadas.len(), 2);

        let ruta_export_dev = exportar_aprendizaje_en(&ruta_cel, &dir).unwrap();
        let dev_json: PaqueteExportacion =
            serde_json::from_str(&fs::read_to_string(ruta_export_dev).unwrap()).unwrap();
        assert_eq!(dev_json.aciertos, 6);
        assert!(dev_json.fuentes_importadas.len() >= 2);

        let _ = fs::remove_dir_all(&dir);
    }
}
