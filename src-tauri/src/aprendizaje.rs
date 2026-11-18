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

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::{cargar_settings, estado_desde_settings, ruta_en_carpeta_config};
use crate::excel::carpeta_descargas;
use crate::models::{normalizar_texto_sin_tildes, AudienciaInput};

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
/// Decaimiento por evento aplicable y tope de tamaño efectivo de muestra (ESS).
pub const DECAY_EVENTO_LAMBDA: f64 = 0.96;
pub const MAX_ESS_N: f64 = 40.0;
/// Ventana libre de decaimiento calendario para evitar olvido durante la vacancia judicial.
pub const DIAS_GRACIA_VACANCIA: f64 = 180.0;

/// Por debajo de esta similitud se considera que el usuario reescribió las observaciones.
const UMBRAL_SIMILITUD_OBSERVACIONES: f64 = 0.85;
/// Similitud mínima para considerar que dos reglas aprendidas son la misma y reforzarla.
const UMBRAL_REGLA_SIMILAR: f64 = 0.78;
/// Confianza por debajo de la cual, si la IA pide aclaración, se consulta opcionalmente al usuario.
const UMBRAL_CONFIANZA_ACLARACION: f64 = 0.78;
/// Tamaño mínimo de un clúster coherente de errores antes de disparar inducción con LLM.
pub const MIN_TAMANO_CLUSTER_INDUCCION: usize = 2;
/// Tope de clústeres mantenidos en cola local.
const MAX_CLUSTERS_COLA: usize = 80;
/// Interacciones de enfriamiento tras descartar o responder una solicitud de aclaración.
const COOLDOWN_ACLARACION_INTERACCIONES: u32 = 4;
const VERSION_FORMATO: u32 = 2;
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
static CONTADOR_IDS: AtomicU64 = AtomicU64::new(1);

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PreferenciasEstilo {
    pub incluir_causas_secundarias_alpha: f64,
    pub incluir_causas_secundarias_beta: f64,
    pub incluir_reprogramacion_alpha: f64,
    pub incluir_reprogramacion_beta: f64,
    pub se_fija_aplazamiento_alpha: f64,
    pub se_fija_aplazamiento_beta: f64,
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
    pub ultima_actualizacion: String,
}

impl Default for AliasEstadistico {
    fn default() -> Self {
        Self {
            campo: String::new(),
            entrada_norm: String::new(),
            canonico: String::new(),
            alpha: PRIOR_BETA_ALPHA,
            beta: PRIOR_BETA_BETA,
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
    pub preferencias_estilo: PreferenciasEstilo,
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
            preferencias_estilo: PreferenciasEstilo::default(),
            tabla_alias: Vec::new(),
            cola_induccion: Vec::new(),
            page_hinkley_campos: BTreeMap::new(),
            page_hinkley_modelos: BTreeMap::new(),
            enfriamiento_aclaracion: 0,
            llamadas_induccion_llm_total: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumenAprendizaje {
    pub aciertos: u32,
    pub total_correcciones: usize,
    pub total_reglas: usize,
    pub pendientes_exportar: usize,
    pub hubo_correccion: bool,
    pub aclaracion_pendiente: Option<AclaracionPendiente>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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
pub struct ModeloDiagnostico {
    pub clave: String,
    pub evaluaciones: u32,
    pub aciertos: u32,
    pub correcciones: u32,
    pub precision_media: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ReglaDiagnostico {
    pub id: String,
    pub estado: String,
    pub campo_objetivo: String,
    pub origen: String,
    pub regla: String,
    pub impresiones: u32,
    pub aciertos_directos: u32,
    pub fallos_directos: u32,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AliasDiagnostico {
    pub campo: String,
    pub entrada_norm: String,
    pub canonico: String,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PreferenciasEstiloResumen {
    pub incluir_causas_secundarias_prob: f64,
    pub incluir_reprogramacion_prob: f64,
    pub se_fija_aplazamiento_prob: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DiagnosticoAprendizaje {
    pub aciertos_totales: u32,
    pub correcciones_totales: usize,
    pub ejemplos_positivos_totales: usize,
    pub llamadas_induccion_llm_total: u32,
    pub clusters_pendientes: usize,
    pub clusters_listos: usize,
    pub drift_campos_activos: Vec<String>,
    pub enfriamiento_aclaracion: u32,
    pub reglas_por_estado: BTreeMap<String, usize>,
    pub metricas_por_campo: Vec<CampoDiagnostico>,
    pub metricas_por_modelo: Vec<ModeloDiagnostico>,
    pub preferencias_estilo: PreferenciasEstiloResumen,
    pub alias_validados: Vec<AliasDiagnostico>,
    pub reglas_destacadas: Vec<ReglaDiagnostico>,
}

#[derive(Debug, Serialize)]
struct PaqueteExportacion {
    app: &'static str,
    version_app: &'static str,
    formato: u32,
    exportado: String,
    aciertos: u32,
    total_reglas: usize,
    total_correcciones: usize,
    total_ejemplos_aciertos: usize,
    llamadas_induccion_llm_total: u32,
    correcciones_por_campo: BTreeMap<String, usize>,
    metricas_por_campo: Vec<CampoDiagnostico>,
    metricas_por_modelo: Vec<ModeloDiagnostico>,
    preferencias_estilo: PreferenciasEstiloResumen,
    alias_validados: Vec<AliasDiagnostico>,
    juzgados_agregados_usuario: Vec<String>,
    tipos_audiencia_agregados_usuario: Vec<String>,
    reglas: Vec<ReglaAprendida>,
    correcciones: Vec<CorreccionIa>,
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

fn inferir_campo_objetivo_desde_tipo(tipo_error: &str) -> String {
    match tipo_error.trim().to_lowercase().as_str() {
        "abreviacion" | "juzgado" => "juzgado".to_string(),
        "realizacion" | "realizada" => "realizada".to_string(),
        "motivo" => "motivo".to_string(),
        "fecha" => "fecha".to_string(),
        "hora" => "hora".to_string(),
        "tipo" => "tipo".to_string(),
        "radicado" => "radicado".to_string(),
        "estilo_observaciones" | "observaciones" => "observaciones".to_string(),
        _ => "general".to_string(),
    }
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
        ultima_actualizacion: actualizada,
        confianza_llm,
        aciertos_sombra: 0,
        confirmaciones_sombra: 0,
        origen_correccion_id,
    }
}

pub fn migrar_memoria_v1_a_v2(memoria: &mut MemoriaAprendizaje) {
    let era_v1 = memoria.version < 2;
    if memoria.version < VERSION_FORMATO {
        memoria.version = VERSION_FORMATO;
    }

    for r in &mut memoria.reglas {
        if r.campo_objetivo.trim().is_empty() || r.campo_objetivo == "general" {
            let inf = inferir_campo_objetivo_desde_tipo(&r.tipo_error);
            if inf != "general" {
                r.campo_objetivo = inf;
            }
        }
        if r.triggers.is_empty() {
            r.triggers = tokenizar_judicial(&r.regla)
                .into_iter()
                .take(8)
                .collect();
        }
        if r.ultima_actualizacion.is_empty() {
            r.ultima_actualizacion = r.actualizada.clone();
        }
        if r.id.starts_with("base-") {
            r.origen = "base_invariante".to_string();
            r.estado = "activa".to_string();
            if r.alpha <= PRIOR_BETA_ALPHA && r.beta >= PRIOR_BETA_BETA {
                r.alpha = 6.0;
                r.beta = 1.0;
            }
        } else if era_v1 || r.estado.trim().is_empty() {
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

pub fn aplicar_decaimiento_beta_en(
    alpha: &mut f64,
    beta: &mut f64,
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
                *alpha = PRIOR_BETA_ALPHA + (*alpha - PRIOR_BETA_ALPHA).max(0.0) * factor_cal;
                *beta = PRIOR_BETA_BETA + (*beta - PRIOR_BETA_BETA).max(0.0) * factor_cal;
            }
        }
    }

    *alpha = PRIOR_BETA_ALPHA + DECAY_EVENTO_LAMBDA * (*alpha - PRIOR_BETA_ALPHA).max(0.0);
    *beta = PRIOR_BETA_BETA + DECAY_EVENTO_LAMBDA * (*beta - PRIOR_BETA_BETA).max(0.0);
    normalizar_ess(alpha, beta);
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

fn es_misma_regla(a: &str, b: &str) -> bool {
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

fn extraer_triggers_texto(texto: &str, max_triggers: usize) -> Vec<String> {
    let mut vistos = BTreeSet::new();
    let mut out = Vec::new();
    for tok in tokenizar_judicial(texto) {
        if tok == "_rad_" {
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

    let mut triggers = extraer_triggers_texto(&regla, 8);
    for t in triggers_extra {
        let limpio = quitar_tildes(&t.trim().to_lowercase());
        if !limpio.is_empty() && limpio != "_rad_" && !triggers.contains(&limpio) && triggers.len() < 12
        {
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
        if existente.estado == "retirada"
            || (existente.estado == "cuarentena" && estado_inicial != "cuarentena")
        {
            existente.estado = estado_inicial.to_string();
            existente.alpha = PRIOR_BETA_ALPHA + 1.0;
            existente.beta = PRIOR_BETA_BETA;
            existente.impresiones = 0;
            existente.aciertos_directos = 0;
            existente.fallos_directos = 0;
            existente.aciertos_sombra = 0;
            existente.confirmaciones_sombra = 0;
        } else if existente.estado == "candidata" && existente.refuerzos >= 2 {
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
    memoria.reglas.push(construir_regla_v2(
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
    ));
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

pub fn seleccionar_contexto_detallado_para_modelo<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
) -> ContextoPromptDetalle<'a> {
    let query_tokens = tokenizar_judicial(texto_nuevo);
    let query_ngrams: BTreeSet<String> = ngramas_caracteres(texto_nuevo, 3).into_iter().collect();

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

    let todas_reglas: Vec<&'a ReglaAprendida> = base
        .reglas
        .iter()
        .chain(local.reglas.iter())
        .collect();

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
            let score = thompson * (0.5 + bm25_prueba[pos] + trig) * factor_modelo(r);
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
                let relevancia =
                    (bm25_norm * 0.60 + trig * 0.25 + fiabilidad * 0.15) * factor_modelo(r);
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
    let corrs: Vec<&'a CorreccionIa> = base
        .correcciones
        .iter()
        .chain(local.correcciones.iter())
        .filter(|c| c.estado == "analizada" && !c.campos.is_empty())
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
            let rel = 0.7 * rel_bm25 + 0.3 * sim_dice;
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

    let aciertos_pool: Vec<&'a EjemploPositivo> = base
        .ejemplos_aciertos
        .iter()
        .chain(local.ejemplos_aciertos.iter())
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
            let rel = 0.7 * rel_bm25 + 0.3 * sim_dice;
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

    let cupo_aciertos = if cand_aciertos.is_empty() {
        0
    } else if cand_corr.len() >= 2 {
        1
    } else {
        (MAX_EJEMPLOS_PROMPT - cand_corr.len()).min(cand_aciertos.len())
    };
    let cupo_corr = (MAX_EJEMPLOS_PROMPT - cupo_aciertos).min(cand_corr.len());

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
    ejemplos_inyectados_ids.extend(ejemplos_aciertos.iter().map(|e| e.id.clone()));

    ContextoPromptDetalle {
        reglas: reglas_elegidas,
        ejemplos_correcciones,
        ejemplos_aciertos,
        reglas_inyectadas_ids,
        ejemplos_inyectados_ids,
    }
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

pub fn formatear_contexto_prompt_con_detalle_para_modelo<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
    modelo_actual: Option<&str>,
) -> (String, Vec<String>, Vec<String>) {
    let det = seleccionar_contexto_detallado_para_modelo(texto_nuevo, base, local, modelo_actual);
    if det.reglas.is_empty()
        && det.ejemplos_correcciones.is_empty()
        && det.ejemplos_aciertos.is_empty()
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
            lineas.push(format!("- [{}] {}", r.tipo_error, r.regla));
        }
    }

    if !det.ejemplos_correcciones.is_empty() || !det.ejemplos_aciertos.is_empty() {
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

/// Patrón regex de una palabra que ignora tildes (se combina con la bandera `(?i)`).
fn patron_sin_tildes(palabra: &str) -> String {
    palabra
        .chars()
        .map(|c| match c.to_lowercase().next().unwrap_or(c) {
            'a' | 'á' => "[aá]".to_string(),
            'e' | 'é' => "[eé]".to_string(),
            'i' | 'í' => "[ií]".to_string(),
            'o' | 'ó' => "[oó]".to_string(),
            'u' | 'ú' | 'ü' => "[uúü]".to_string(),
            'n' | 'ñ' => "[nñ]".to_string(),
            otro => regex::escape(&otro.to_string()),
        })
        .collect()
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

/// Reemplaza secuencias numéricas de 6 o más dígitos (radicados, cédulas, teléfonos) por
/// `[NUMERO]`, conservando fechas y horas, que son útiles para aprender formatos.
fn enmascarar_numeros_largos(texto: &str) -> String {
    let Some(re) = re_numeros_largos() else {
        return texto.to_string();
    };
    re.replace_all(texto, |caps: &regex::Captures| {
        let m = &caps[0];
        let digitos = m.chars().filter(|c| c.is_ascii_digit()).count();
        let es_fecha = re_fecha_con_guiones().is_some_and(|rf| rf.is_match(m));
        if digitos >= 6 && !es_fecha {
            "[NUMERO]".to_string()
        } else {
            m.to_string()
        }
    })
    .into_owned()
}

/// Reemplaza el nombre del procesado y el número de radicado (conocidos por el campo Radicado,
/// con formato habitual `NOMBRE - NÚMERO`) por marcadores genéricos.
pub(crate) struct Anonimizador {
    completos: Vec<(Regex, &'static str)>,
    palabras: Vec<(Regex, &'static str)>,
}

impl Anonimizador {
    fn desde_radicados(radicados: &[&str]) -> Self {
        let mut completos = Vec::new();
        let mut palabras = Vec::new();

        for radicado in radicados {
            let partes: Vec<String> = match re_separador_radicado() {
                Some(re) => re.split(radicado).map(colapsar_espacios).collect(),
                None => vec![colapsar_espacios(radicado)],
            };

            for parte in partes.into_iter().filter(|p| !p.is_empty()) {
                let digitos = parte.chars().filter(|c| c.is_ascii_digit()).count();
                if digitos >= 5 {
                    let mut solo_alfa = String::new();
                    let mut num_runs: Vec<String> = Vec::new();
                    let mut curr_num = String::new();
                    for ch in parte.chars() {
                        if ch.is_ascii_digit() || (matches!(ch, '-' | '.' | '/') && !curr_num.is_empty()) {
                            curr_num.push(ch);
                        } else {
                            if curr_num.chars().filter(|c| c.is_ascii_digit()).count() >= 5 {
                                let trimmed = curr_num.trim_end_matches(|c| matches!(c, '-' | '.' | '/')).to_string();
                                if !trimmed.is_empty() {
                                    num_runs.push(trimmed);
                                }
                            }
                            curr_num.clear();
                            if ch.is_alphabetic() || ch.is_whitespace() {
                                solo_alfa.push(ch);
                            } else {
                                solo_alfa.push(' ');
                            }
                        }
                    }
                    if curr_num.chars().filter(|c| c.is_ascii_digit()).count() >= 5 {
                        let trimmed = curr_num.trim_end_matches(|c| matches!(c, '-' | '.' | '/')).to_string();
                        if !trimmed.is_empty() {
                            num_runs.push(trimmed);
                        }
                    }

                    let alfa_limpio = colapsar_espacios(&solo_alfa);
                    if alfa_limpio.is_empty() {
                        if let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(&parte))) {
                            completos.push((re, "[RADICADO]"));
                        }
                    } else {
                        for nr in num_runs {
                            if let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(&nr))) {
                                completos.push((re, "[RADICADO]"));
                            }
                        }
                        let tokens_alfa: Vec<&str> = alfa_limpio
                            .split_whitespace()
                            .filter(|t| {
                                let lw = t.to_lowercase();
                                !matches!(lw.as_str(), "rad" | "radicado" | "nro" | "num" | "proc" | "procesado")
                            })
                            .collect();
                        if !tokens_alfa.is_empty() {
                            let pat_full = tokens_alfa
                                .iter()
                                .map(|t| patron_sin_tildes(t))
                                .collect::<Vec<_>>()
                                .join(r"\s+");
                            if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", pat_full)) {
                                completos.push((re, "[PROCESADO]"));
                            }
                            for token in tokens_alfa {
                                let lower = token.to_lowercase();
                                if token.chars().count() < 3
                                    || PALABRAS_VACIAS_NOMBRE.contains(&lower.as_str())
                                {
                                    continue;
                                }
                                if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", patron_sin_tildes(token))) {
                                    palabras.push((re, "[PROCESADO]"));
                                }
                            }
                        }
                    }
                    continue;
                }

                let tokens: Vec<&str> = parte.split_whitespace().collect();
                let patron_completo = tokens
                    .iter()
                    .map(|t| patron_sin_tildes(t))
                    .collect::<Vec<_>>()
                    .join(r"\s+");
                if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", patron_completo)) {
                    completos.push((re, "[PROCESADO]"));
                }

                for token in tokens {
                    let lower = token.to_lowercase();
                    if token.chars().count() < 3 || PALABRAS_VACIAS_NOMBRE.contains(&lower.as_str())
                    {
                        continue;
                    }
                    if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", patron_sin_tildes(token))) {
                        palabras.push((re, "[PROCESADO]"));
                    }
                }
            }
        }

        Self {
            completos,
            palabras,
        }
    }

    fn aplicar(&self, texto: &str) -> String {
        let mut out = texto.to_string();
        for (re, marca) in self.completos.iter().chain(self.palabras.iter()) {
            out = re.replace_all(&out, *marca).into_owned();
        }
        enmascarar_numeros_largos(&out)
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
    let aplicar_ambos = |s: &str| -> String {
        let paso1 = anon_local.aplicar(s);
        match anon_global {
            Some(g) => g.aplicar(&paso1),
            None => paso1,
        }
    };
    let aplicar_aud = |a: &AudienciaInput| -> AudienciaInput {
        let paso1 = anon_local.aplicar_audiencia(a);
        match anon_global {
            Some(g) => g.aplicar_audiencia(&paso1),
            None => paso1,
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

/// Carga la memoria; si es `v1` la migra automáticamente a `v2` sin perder datos,
/// y si el archivo está dañado lo conserva como `.bak` y empieza de cero.
pub fn cargar_memoria_en(ruta: &Path) -> MemoriaAprendizaje {
    let Ok(contenido) = fs::read_to_string(ruta) else {
        return MemoriaAprendizaje::default();
    };
    match serde_json::from_str::<MemoriaAprendizaje>(&contenido) {
        Ok(mut memoria) => {
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
    ResumenAprendizaje {
        aciertos: memoria.aciertos,
        total_correcciones: memoria.correcciones.len(),
        total_reglas: memoria
            .reglas
            .iter()
            .filter(|r| r.estado != "retirada")
            .count(),
        pendientes_exportar: memoria.correcciones.iter().filter(|c| !c.exportada).count(),
        hubo_correccion,
        aclaracion_pendiente: ultima_aclaracion_pendiente(memoria),
    }
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

fn actualizar_metricas_y_credito(
    memoria: &mut MemoriaAprendizaje,
    texto: &str,
    campos_corregidos: &[CampoCorregido],
    reglas_inyectadas: &[String],
    proveedor: &str,
    modelo: &str,
    ahora: &str,
) {
    let mut campos_err: BTreeSet<&str> = BTreeSet::new();
    for c in campos_corregidos {
        campos_err.insert(c.campo.as_str());
    }

    // 1. Métricas por campo, por (proveedor, modelo) y detector Page-Hinkley de drift
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
            let actualizar_mc = |mc: &mut MetricasCampo| {
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
            };

            let mc_global = memoria
                .metricas_campos
                .entry(campo.to_string())
                .or_insert_with(|| MetricasCampo {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            actualizar_mc(mc_global);

            let mc_mod = mod_entry
                .por_campo
                .entry(campo.to_string())
                .or_insert_with(|| MetricasCampo {
                    campo: campo.to_string(),
                    ..Default::default()
                });
            actualizar_mc(mc_mod);

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
        if regla.estado == "candidata" || regla.estado == "cuarentena" {
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
}

fn calcular_firma_error(corr: &CorreccionIa) -> (String, String) {
    let Some(primero) = corr.campos.first() else {
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

fn encolar_correccion_en_cluster(
    memoria: &mut MemoriaAprendizaje,
    corr: &CorreccionIa,
    ahora: &str,
) {
    let (campo_principal, firma) = calcular_firma_error(corr);
    let clave_modelo = format!("{}:{}", corr.proveedor.trim(), corr.modelo.trim());
    let en_drift = memoria
        .page_hinkley_campos
        .get(&campo_principal)
        .is_some_and(|p| p.drift_detectado)
        || memoria
            .page_hinkley_modelos
            .get(&clave_modelo)
            .and_then(|m| m.get(&campo_principal))
            .is_some_and(|p| p.drift_detectado);

    // Si ya existe un clúster inducido reciente para esta firma con regla vigente ("prueba"/"activa")
    // y no hay drift activo, evita re-llamar al LLM por el mismo patrón repetitivo.
    if !en_drift {
        let ya_cubierto = memoria.cola_induccion.iter().any(|c| {
            c.estado == "inducido"
                && c.campo_principal == campo_principal
                && c.firma_error == firma
                && !c.regla_inducida_id.is_empty()
                && memoria.reglas.iter().any(|r| {
                    r.id == c.regla_inducida_id
                        && matches!(r.estado.as_str(), "prueba" | "activa")
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
        if cluster.firma_error == firma {
            indice_objetivo = Some(idx);
            break;
        }
        let bm25_match = cluster.correccion_ids.iter().rev().take(2).any(|cid| {
            memoria
                .correcciones
                .iter()
                .find(|c| &c.id == cid)
                .is_some_and(|anchor| {
                    similitud_bm25_normalizada(&corr.texto_original, &anchor.texto_original) >= 0.55
                })
        });
        if bm25_match {
            indice_objetivo = Some(idx);
            break;
        }
    }

    if let Some(idx) = indice_objetivo {
        let cluster = &mut memoria.cola_induccion[idx];
        if !cluster.correccion_ids.contains(&corr.id) {
            cluster.correccion_ids.push(corr.id.clone());
        }
        cluster.ultima_vez = ahora.to_string();
        if cluster.correccion_ids.len() >= MIN_TAMANO_CLUSTER_INDUCCION {
            cluster.estado = "listo".to_string();
        }
    } else {
        memoria.cola_induccion.push(ClusterCorreccion {
            id: generar_id_unico("clu"),
            campo_principal,
            firma_error: firma,
            correccion_ids: vec![corr.id.clone()],
            primera_vez: ahora.to_string(),
            ultima_vez: ahora.to_string(),
            estado: "acumulando".to_string(),
            regla_inducida_id: String::new(),
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

fn es_cambio_ambiguo_o_contradictorio(corr: &CorreccionIa) -> bool {
    if corr.campos.len() >= 2 {
        return true;
    }
    let Some(c) = corr.campos.first() else {
        return false;
    };
    if matches!(c.campo.as_str(), "realizada" | "motivo" | "tipo" | "juzgado") {
        return true;
    }
    let tiene_sigla = c
        .valor_usuario
        .split_whitespace()
        .any(|w| w.len() >= 2 && w.chars().filter(|ch| ch.is_alphabetic()).count() >= 2 && w.chars().all(|ch| !ch.is_lowercase()));
    tiene_sigla
}

fn puede_solicitar_aclaracion(
    memoria: &MemoriaAprendizaje,
    corr: &CorreccionIa,
    analisis: &AnalisisCorreccionIa,
) -> bool {
    if !analisis.necesita_aclaracion
        || analisis.confianza >= UMBRAL_CONFIANZA_ACLARACION
        || analisis.pregunta.trim().is_empty()
        || !corr.nota_usuario.trim().is_empty()
    {
        return false;
    }
    if memoria.enfriamiento_aclaracion > 0 {
        return false;
    }
    let ya_esperando = memoria
        .correcciones
        .iter()
        .any(|c| c.id != corr.id && c.estado == "esperando_aclaracion");
    if ya_esperando {
        return false;
    }
    es_cambio_ambiguo_o_contradictorio(corr)
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
    if texto.trim().is_empty() {
        return Err("No hay texto de IA asociado a este registro.".to_string());
    }

    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let campos = detectar_correcciones(prediccion, final_usuario);
    let hubo_correccion = !campos.is_empty();
    let mut memoria = cargar_memoria_en(ruta);
    let ahora = Local::now().to_rfc3339();
    let mut nueva_correccion = None;

    memoria.enfriamiento_aclaracion = memoria.enfriamiento_aclaracion.saturating_sub(1);

    actualizar_metricas_y_credito(
        &mut memoria,
        texto,
        &campos,
        reglas_inyectadas,
        proveedor,
        modelo,
        &ahora,
    );
    crate::estadistica_ia::actualizar_alias_y_preferencias_en_memoria(
        &mut memoria,
        texto,
        prediccion,
        final_usuario,
        &campos,
        &ahora,
    );

    if hubo_correccion {
        let corr = CorreccionIa {
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
        };
        encolar_correccion_en_cluster(&mut memoria, &corr, &ahora);
        nueva_correccion = Some(corr.clone());
        memoria.correcciones.push(corr);
        if memoria.correcciones.len() > MAX_CORRECCIONES {
            let exceso = memoria.correcciones.len() - MAX_CORRECCIONES;
            memoria.correcciones.drain(0..exceso);
        }
    } else {
        memoria.aciertos = memoria.aciertos.saturating_add(1);
        let anon = Anonimizador::desde_radicados(&[&final_usuario.radicado]);
        let ejemplo = EjemploPositivo {
            id: generar_id_unico("pos"),
            registrada: ahora,
            proveedor: proveedor.to_string(),
            modelo: modelo.to_string(),
            texto_anonimizado: anon.aplicar(texto.trim()),
            salida_anonimizada: anon.aplicar_audiencia(final_usuario),
            reglas_inyectadas: reglas_inyectadas.to_vec(),
        };
        memoria.ejemplos_aciertos.push(ejemplo);
        if memoria.ejemplos_aciertos.len() > MAX_ACIERTOS_EJEMPLOS {
            let exceso = memoria.ejemplos_aciertos.len() - MAX_ACIERTOS_EJEMPLOS;
            memoria.ejemplos_aciertos.drain(0..exceso);
        }
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok((resumen_de(&memoria, hubo_correccion), nueva_correccion))
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
    memoria.llamadas_induccion_llm_total =
        memoria.llamadas_induccion_llm_total.saturating_add(1);

    let mut regla_a_registrar: Option<(String, String, String, String, Vec<String>, f64)> = None;
    let mut modelo_alcance = "*".to_string();
    if let Some(pos) = memoria.correcciones.iter().position(|c| c.id == correccion_id) {
        let permitir_aclaracion =
            puede_solicitar_aclaracion(&memoria, &memoria.correcciones[pos], analisis);
        let corr = &mut memoria.correcciones[pos];
        modelo_alcance = format!("{}:{}", corr.proveedor, corr.modelo);
        if !analisis.es_error_de_ia {
            corr.estado = "descartada".to_string();
            corr.pregunta_ia.clear();
        } else if permitir_aclaracion {
            corr.estado = "esperando_aclaracion".to_string();
            corr.pregunta_ia = colapsar_espacios(&analisis.pregunta);
            if !analisis.regla.trim().is_empty() {
                corr.regla_generada = colapsar_espacios(&analisis.regla);
            }
        } else {
            corr.estado = "analizada".to_string();
            corr.pregunta_ia.clear();
            if !analisis.regla.trim().is_empty() {
                let limpia = colapsar_espacios(&analisis.regla);
                corr.regla_generada = limpia.clone();
                let campo_obj = corr
                    .campos
                    .first()
                    .map(|c| c.campo.clone())
                    .unwrap_or_else(|| inferir_campo_objetivo_desde_tipo(&analisis.tipo_error));
                let triggers = extraer_triggers_texto(&corr.texto_original, 8);
                regla_a_registrar = Some((
                    analisis.tipo_error.clone(),
                    limpia,
                    corr.regla_id_asociada.clone(),
                    campo_obj,
                    triggers,
                    analisis.confianza,
                ));
            }
        }
    }

    if let Some((tipo, regla, id_asociado, campo_obj, triggers, confianza)) = regla_a_registrar {
        let mut actualizada_en_sitio = false;
        if !id_asociado.is_empty() {
            if let Some(existente) = memoria.reglas.iter_mut().find(|r| r.id == id_asociado) {
                if !tipo.trim().is_empty() && tipo.trim() != "general" {
                    existente.tipo_error = tipo.trim().to_lowercase();
                }
                if regla.len() >= 6 && regla.len() <= 240 {
                    existente.regla = regla.clone();
                }
                existente.actualizada = Local::now().to_rfc3339();
                existente.ultima_actualizacion = existente.actualizada.clone();
                existente.confianza_llm = confianza;
                actualizada_en_sitio = true;
            }
        }
        if !actualizada_en_sitio {
            if let Some(nuevo_id) = agregar_regla_interna(
                &mut memoria,
                &tipo,
                &regla,
                "candidata",
                Some(&campo_obj),
                &triggers,
                "mitigacion_modelo",
                &modelo_alcance,
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
/// Devuelve la corrección actualizada para que pueda re-analizarse con la IA si hay conexión,
/// y además registra de inmediato la nota como regla provisional vinculada (evitando duplicados
/// si la IA la refina posteriormente).
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
    let mut campo_principal = "general".to_string();
    let mut regla_id_previa = String::new();

    if let Some(corr) = memoria
        .correcciones
        .iter()
        .find(|c| c.id == correccion_id)
    {
        if let Some(primero) = corr.campos.first() {
            campo_principal = primero.campo.clone();
        }
        regla_id_previa = corr.regla_id_asociada.clone();
    }

    let id_regla = if !regla_id_previa.is_empty()
        && memoria.reglas.iter().any(|r| r.id == regla_id_previa)
    {
        if let Some(r) = memoria.reglas.iter_mut().find(|r| r.id == regla_id_previa) {
            r.regla = nota_limpia.clone();
            r.actualizada = Local::now().to_rfc3339();
        }
        Some(regla_id_previa)
    } else {
        agregar_o_reforzar_regla_con_id(&mut memoria, &campo_principal, &nota_limpia)
    };

    let mut corr_actualizada = None;
    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        corr.nota_usuario = nota_limpia.clone();
        corr.pregunta_ia.clear();
        corr.estado = "analizada".to_string();
        if corr.regla_generada.trim().is_empty() {
            corr.regla_generada = nota_limpia.clone();
        }
        if let Some(ref rid) = id_regla {
            corr.regla_id_asociada = rid.clone();
        }
        corr_actualizada = Some(corr.clone());
    }

    memoria.enfriamiento_aclaracion = COOLDOWN_ACLARACION_INTERACCIONES;
    guardar_memoria_en(ruta, &memoria)?;
    Ok((resumen_de(&memoria, false), corr_actualizada))
}

/// Permite omitir una solicitud de aclaración sin borrar la corrección subyacente.
pub fn descartar_aclaracion_en(
    ruta: &Path,
    correccion_id: &str,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    let mut regla_previa: Option<String> = None;

    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        corr.pregunta_ia.clear();
        corr.estado = "analizada".to_string();
        if !corr.regla_generada.trim().is_empty() {
            regla_previa = Some(corr.regla_generada.clone());
        }
    }

    if let Some(regla) = regla_previa {
        let _ = agregar_regla_interna(
            &mut memoria,
            "general",
            &regla,
            "candidata",
            None,
            &[],
            "mitigacion_modelo",
            "*",
            0.5,
            correccion_id,
        );
    }

    memoria.enfriamiento_aclaracion = COOLDOWN_ACLARACION_INTERACCIONES;
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
                    .find(|c| &c.id == cid && c.estado == "pendiente")
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
    memoria.llamadas_induccion_llm_total =
        memoria.llamadas_induccion_llm_total.saturating_add(1);

    let mut ids_set: BTreeSet<String> = correccion_ids.iter().cloned().collect();
    if let Some(clu) = memoria.cola_induccion.iter().find(|c| c.id == cluster_id) {
        for cid in &clu.correccion_ids {
            ids_set.insert(cid.clone());
        }
    }
    let ultima_corr = memoria
        .correcciones
        .iter()
        .rev()
        .find(|c| ids_set.contains(&c.id))
        .cloned();

    let permitir_aclaracion = ultima_corr
        .as_ref()
        .is_some_and(|c| puede_solicitar_aclaracion(&memoria, c, analisis));

    let mut regla_inducida_id = String::new();
    if !analisis.es_error_de_ia {
        for c in &mut memoria.correcciones {
            if ids_set.contains(&c.id) {
                c.estado = "descartada".to_string();
                c.pregunta_ia.clear();
            }
        }
    } else if permitir_aclaracion {
        let rep_id = ultima_corr.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        for c in &mut memoria.correcciones {
            if ids_set.contains(&c.id) {
                if c.id == rep_id {
                    c.estado = "esperando_aclaracion".to_string();
                    c.pregunta_ia = colapsar_espacios(&analisis.pregunta);
                } else {
                    c.estado = "analizada".to_string();
                    c.pregunta_ia.clear();
                }
                if !analisis.regla.trim().is_empty() {
                    c.regla_generada = colapsar_espacios(&analisis.regla);
                }
            }
        }
    } else {
        let mut triggers_unificados: Vec<String> = Vec::new();
        let mut campo_obj = String::new();
        let mut modelo_alcance = "*".to_string();
        let mut rep_id = String::new();

        for c in &memoria.correcciones {
            if !ids_set.contains(&c.id) {
                continue;
            }
            if rep_id.is_empty() {
                rep_id = c.id.clone();
            }
            if campo_obj.is_empty() {
                campo_obj = c
                    .campos
                    .first()
                    .map(|cf| cf.campo.clone())
                    .unwrap_or_else(|| inferir_campo_objetivo_desde_tipo(&analisis.tipo_error));
            }
            modelo_alcance = format!("{}:{}", c.proveedor, c.modelo);
            for t in extraer_triggers_texto(&c.texto_original, 6) {
                if !triggers_unificados.contains(&t) && triggers_unificados.len() < 10 {
                    triggers_unificados.push(t);
                }
            }
        }

        let regla_limpia = colapsar_espacios(&analisis.regla);
        let estado_inicial = if ids_set.len() >= MIN_TAMANO_CLUSTER_INDUCCION
            && analisis.confianza >= 0.70
        {
            "prueba"
        } else {
            "candidata"
        };

        let nuevo_id = if !regla_limpia.is_empty() {
            agregar_regla_interna(
                &mut memoria,
                &analisis.tipo_error,
                &regla_limpia,
                estado_inicial,
                Some(&campo_obj),
                &triggers_unificados,
                "mitigacion_modelo",
                &modelo_alcance,
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
    }
}

fn construir_metricas_diagnostico(
    memoria: &MemoriaAprendizaje,
    anon: &Anonimizador,
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
            Some(AliasDiagnostico {
                campo: a.campo.clone(),
                entrada_norm: anon.aplicar(&a.entrada_norm),
                canonico: anon.aplicar(&a.canonico),
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

pub fn obtener_diagnostico_aprendizaje_en(ruta: &Path) -> DiagnosticoAprendizaje {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let memoria = cargar_memoria_en(ruta);
    let mut todos_radicados: Vec<&str> = Vec::new();
    for c in &memoria.correcciones {
        if !c.prediccion.radicado.trim().is_empty() {
            todos_radicados.push(&c.prediccion.radicado);
        }
        if !c.final_usuario.radicado.trim().is_empty() {
            todos_radicados.push(&c.final_usuario.radicado);
        }
    }
    let anon = Anonimizador::desde_radicados(&todos_radicados);

    let (metricas_por_campo, metricas_por_modelo, alias_validados, drift_campos_activos) =
        construir_metricas_diagnostico(&memoria, &anon);

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

    let prioridad_estado = |s: &str| match s {
        "activa" => 4u8,
        "prueba" => 3u8,
        "candidata" => 2u8,
        "cuarentena" => 1u8,
        _ => 0u8,
    };

    let mut reglas_destacadas: Vec<ReglaDiagnostico> = memoria
        .reglas
        .iter()
        .filter(|r| r.estado != "retirada")
        .map(|r| ReglaDiagnostico {
            id: r.id.clone(),
            estado: r.estado.clone(),
            campo_objetivo: r.campo_objetivo.clone(),
            origen: r.origen.clone(),
            regla: anon.aplicar(&r.regla),
            impresiones: r.impresiones,
            aciertos_directos: r.aciertos_directos,
            fallos_directos: r.fallos_directos,
            confianza_q10: limite_inferior_bayesiano(r.alpha, r.beta),
        })
        .collect();
    reglas_destacadas.sort_by(|a, b| {
        prioridad_estado(&b.estado)
            .cmp(&prioridad_estado(&a.estado))
            .then_with(|| {
                b.confianza_q10
                    .partial_cmp(&a.confianza_q10)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    reglas_destacadas.truncate(15);

    DiagnosticoAprendizaje {
        aciertos_totales: memoria.aciertos,
        correcciones_totales: memoria.correcciones.len(),
        ejemplos_positivos_totales: memoria.ejemplos_aciertos.len(),
        llamadas_induccion_llm_total: memoria.llamadas_induccion_llm_total,
        clusters_pendientes,
        clusters_listos,
        drift_campos_activos,
        enfriamiento_aclaracion: memoria.enfriamiento_aclaracion,
        reglas_por_estado,
        metricas_por_campo,
        metricas_por_modelo,
        preferencias_estilo: construir_preferencias_resumen(&memoria.preferencias_estilo),
        alias_validados: alias_validados.into_iter().take(12).collect(),
        reglas_destacadas,
    }
}

pub fn exportar_aprendizaje_con_catalogos_en(
    ruta: &Path,
    carpeta_destino: &Path,
    juzgados_usuario: &[String],
    tipos_usuario: &[String],
) -> Result<String, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);
    if memoria.correcciones.is_empty()
        && memoria.reglas.is_empty()
        && juzgados_usuario.is_empty()
        && tipos_usuario.is_empty()
    {
        return Err(
            "Aún no hay correcciones de la IA ni elementos agregados para compartir."
                .to_string(),
        );
    }

    let mut correcciones_por_campo: BTreeMap<String, usize> = BTreeMap::new();
    for c in &memoria.correcciones {
        for campo in &c.campos {
            *correcciones_por_campo.entry(campo.campo.clone()).or_insert(0) += 1;
        }
    }

    let mut todos_radicados: Vec<&str> = Vec::new();
    for c in &memoria.correcciones {
        if !c.prediccion.radicado.trim().is_empty() {
            todos_radicados.push(&c.prediccion.radicado);
        }
        if !c.final_usuario.radicado.trim().is_empty() {
            todos_radicados.push(&c.final_usuario.radicado);
        }
    }
    let anon_global = Anonimizador::desde_radicados(&todos_radicados);

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
                        && limpio != "[NUMERO]"
                })
                .collect();
            ReglaAprendida {
                regla: anon_global.aplicar(&r.regla),
                triggers: triggers_limpios,
                ..r.clone()
            }
        })
        .collect();

    let correcciones_anonimizadas: Vec<CorreccionIa> = memoria
        .correcciones
        .iter()
        .map(|c| anonimizar_correccion_con_global(c, Some(&anon_global)))
        .collect();

    let juzgados_anon: Vec<String> = juzgados_usuario
        .iter()
        .map(|j| anon_global.aplicar(j))
        .collect();
    let tipos_anon: Vec<String> = tipos_usuario
        .iter()
        .map(|t| anon_global.aplicar(t))
        .collect();

    let (metricas_por_campo, metricas_por_modelo, alias_validados, _) =
        construir_metricas_diagnostico(&memoria, &anon_global);
    let preferencias_estilo = construir_preferencias_resumen(&memoria.preferencias_estilo);

    let paquete = PaqueteExportacion {
        app: "Lokus",
        version_app: env!("CARGO_PKG_VERSION"),
        formato: VERSION_FORMATO,
        exportado: Local::now().to_rfc3339(),
        aciertos: memoria.aciertos,
        total_reglas: reglas_anonimizadas.len(),
        total_correcciones: correcciones_anonimizadas.len(),
        total_ejemplos_aciertos: memoria.ejemplos_aciertos.len(),
        llamadas_induccion_llm_total: memoria.llamadas_induccion_llm_total,
        correcciones_por_campo,
        metricas_por_campo,
        metricas_por_modelo,
        preferencias_estilo,
        alias_validados,
        juzgados_agregados_usuario: juzgados_anon,
        tipos_audiencia_agregados_usuario: tipos_anon,
        reglas: reglas_anonimizadas,
        correcciones: correcciones_anonimizadas,
    };
    let json = serde_json::to_string_pretty(&paquete)
        .map_err(|e| format!("Error preparando la exportación del aprendizaje: {}", e))?;

    fs::create_dir_all(carpeta_destino)
        .map_err(|e| format!("No se pudo acceder a la carpeta de destino: {}", e))?;
    let base = format!("lokus_aprendizaje_ia_{}", Local::now().format("%Y%m%d_%H%M%S"));
    let mut destino = carpeta_destino.join(format!("{}.json", base));
    let mut contador = 1;
    while destino.exists() {
        destino = carpeta_destino.join(format!("{}_{}.json", base, contador));
        contador += 1;
    }
    fs::write(&destino, json)
        .map_err(|e| format!("No se pudo guardar el archivo de aprendizaje: {}", e))?;

    for c in memoria.correcciones.iter_mut() {
        c.exportada = true;
    }
    guardar_memoria_en(ruta, &memoria)?;

    Ok(destino.to_string_lossy().to_string())
}

pub fn exportar_aprendizaje_en(ruta: &Path, carpeta_destino: &Path) -> Result<String, String> {
    exportar_aprendizaje_con_catalogos_en(ruta, carpeta_destino, &[], &[])
}

// ---------------------------------------------------------------------------
// API pública usada por `ai.rs` y `lib.rs`
// ---------------------------------------------------------------------------

pub fn obtener_contexto_prompt_con_traza(texto_nuevo: &str) -> (String, Vec<String>, Vec<String>) {
    let estado = estado_desde_settings(&cargar_settings());
    let modelo_actual = format!("{}:{}", estado.provider.trim(), estado.model.trim());
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let local = cargar_memoria_en(&ruta_memoria());
    formatear_contexto_prompt_con_detalle_para_modelo(
        texto_nuevo,
        memoria_base_embebida(),
        &local,
        Some(&modelo_actual),
    )
}

pub fn obtener_contexto_prompt(texto_nuevo: &str) -> String {
    obtener_contexto_prompt_con_traza(texto_nuevo).0
}

pub fn registrar_resultado_ia_con_traza(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    reglas_inyectadas: &[String],
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    let estado = estado_desde_settings(&cargar_settings());
    registrar_resultado_con_traza_en(
        &ruta_memoria(),
        texto,
        prediccion,
        final_usuario,
        &estado.provider,
        &estado.model,
        reglas_inyectadas,
    )
}

pub fn registrar_resultado_ia(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    registrar_resultado_ia_con_traza(texto, prediccion, final_usuario, &[])
}

pub fn obtener_resumen_aprendizaje() -> ResumenAprendizaje {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    resumen_de(&cargar_memoria_en(&ruta_memoria()), false)
}

pub fn obtener_diagnostico_aprendizaje() -> DiagnosticoAprendizaje {
    obtener_diagnostico_aprendizaje_en(&ruta_memoria())
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
    exportar_aprendizaje_con_catalogos_en(
        &ruta_memoria(),
        &carpeta_descargas()?,
        &settings.juzgados,
        &settings.tipos_audiencia,
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

        let (_, c1) = registrar_resultado_en(
            &ruta,
            "Audiencia en Juzgado 12 CTO 3",
            &pred,
            &fin,
            "openai",
            "m",
        )
        .expect("registrar c1");
        let id1 = c1.expect("c1").id;

        // 1) Análisis que solicita aclaración al usuario
        let res_acl = aplicar_analisis_en(
            &ruta,
            &id1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar CTO abreviado en juzgados".into(),
                confianza: 0.4,
                necesita_aclaracion: true,
                pregunta: "¿Por qué cambiaste Circuito por CTO?".into(),
            },
        )
        .expect("aplicar aclaración");
        assert!(res_acl.aclaracion_pendiente.is_some());
        assert_eq!(res_acl.total_reglas, 0);

        // El usuario responde la aclaración -> se guarda la nota y se crea la regla
        let (res_nota, _) = guardar_nota_aclaracion_en(
            &ruta,
            &id1,
            "Dejar CTO abreviado siempre en el nombre del juzgado",
        )
        .expect("guardar nota");
        assert!(res_nota.aclaracion_pendiente.is_none());
        assert_eq!(res_nota.total_reglas, 1);

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
        let (_, corr) = registrar_resultado_en(
            &ruta,
            "Audiencia en Juzgado 3 MPAL",
            &pred,
            &fin,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let cid = corr.unwrap().id;
        aplicar_analisis_en(
            &ruta,
            &cid,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar MPAL".into(),
                confianza: 0.4,
                necesita_aclaracion: true,
                pregunta: "¿Qué significa MPAL?".into(),
            },
        )
        .unwrap();
        let (res_nota, _) =
            guardar_nota_aclaracion_en(&ruta, &cid, "Dejar MPAL sin expandir").unwrap();
        assert_eq!(res_nota.total_reglas, 1);

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
        assert!(mem_despues.reglas[0].regla.contains("MPAL"));
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
        assert_eq!(mem.version, 2);
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

        // 4) Política mínima de aclaración: bloqueo por cooldown y cambios no contradictorios
        let mut fin_fecha = audiencia_base();
        fin_fecha.fecha = "16/08/2025".into();
        let (_, c_fecha) = registrar_resultado_en(
            &ruta_ph,
            "Audiencia el 16/08/2025",
            &audiencia_base(),
            &fin_fecha,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let res_no_acl = aplicar_analisis_en(
            &ruta_ph,
            &c_fecha.unwrap().id,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "fecha".into(),
                regla: "Ajustar fecha literal".into(),
                confianza: 0.4,
                necesita_aclaracion: true,
                pregunta: "¿Seguro la fecha?".into(),
            },
        )
        .unwrap();
        assert!(
            res_no_acl.aclaracion_pendiente.is_none(),
            "No debe pedir aclaración en cambios simples de fecha"
        );

        // En cambio semántico (juzgado CTO), pide aclaración -> al descartar activa cooldown de 4 interacciones
        let (_, c_juz1) = registrar_resultado_en(
            &ruta_ph,
            "Audiencia en Juzgado 3 CTO",
            &audiencia_base(),
            &fin_cto,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let id_j1 = c_juz1.unwrap().id;
        let res_si_acl = aplicar_analisis_en(
            &ruta_ph,
            &id_j1,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar CTO".into(),
                confianza: 0.4,
                necesita_aclaracion: true,
                pregunta: "¿Qué significa CTO en este juzgado?".into(),
            },
        )
        .unwrap();
        assert!(res_si_acl.aclaracion_pendiente.is_some());
        descartar_aclaracion_en(&ruta_ph, &id_j1).unwrap();

        let (_, c_juz2) = registrar_resultado_en(
            &ruta_ph,
            "Otra en Juzgado 5 CTO",
            &audiencia_base(),
            &fin_cto,
            "openai",
            "gpt-5-mini",
        )
        .unwrap();
        let res_bloqueada = aplicar_analisis_en(
            &ruta_ph,
            &c_juz2.unwrap().id,
            &AnalisisCorreccionIa {
                es_error_de_ia: true,
                tipo_error: "abreviacion".into(),
                regla: "Conservar CTO".into(),
                confianza: 0.4,
                necesita_aclaracion: true,
                pregunta: "¿Seguro?".into(),
            },
        )
        .unwrap();
        assert!(
            res_bloqueada.aclaracion_pendiente.is_none(),
            "Cooldown debe bloquear nueva aclaración inmediatamente después de Omitir"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir_sim);
        let _ = fs::remove_dir_all(&dir_ph);
    }
}
