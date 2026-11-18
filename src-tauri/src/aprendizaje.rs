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
use std::sync::{Mutex, OnceLock};

use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::{cargar_settings, estado_desde_settings, ruta_en_carpeta_config};
use crate::excel::carpeta_descargas;
use crate::models::AudienciaInput;

pub const NOMBRE_ARCHIVO_MEMORIA: &str = "ia_aprendizaje.json";
/// Máximo de correcciones conservadas localmente (se descartan primero las más antiguas).
pub const MAX_CORRECCIONES: usize = 300;
/// Máximo de reglas aprendidas activas en memoria local.
pub const MAX_REGLAS: usize = 60;
/// Máximo de reglas inyectadas en el prompt de extracción.
pub const MAX_REGLAS_PROMPT: usize = 5;
/// Máximo de ejemplos similares inyectados en el prompt de extracción.
pub const MAX_EJEMPLOS_PROMPT: usize = 3;

/// Por debajo de esta similitud se considera que el usuario reescribió las observaciones.
const UMBRAL_SIMILITUD_OBSERVACIONES: f64 = 0.85;
/// Similitud mínima para considerar que dos reglas aprendidas son la misma y reforzarla.
const UMBRAL_REGLA_SIMILAR: f64 = 0.78;
/// Confianza por debajo de la cual, si la IA pide aclaración, se consulta opcionalmente al usuario.
const UMBRAL_CONFIANZA_ACLARACION: f64 = 0.78;
const VERSION_FORMATO: u32 = 1;
const PALABRAS_VACIAS_NOMBRE: [&str; 7] = ["de", "del", "la", "las", "los", "y", "san"];

const APRENDIZAJE_BASE_JSON: &str = include_str!("../resources/aprendizaje_base.json");

/// Serializa lecturas/escrituras del archivo de memoria entre comandos concurrentes.
static BLOQUEO_MEMORIA: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CampoCorregido {
    pub campo: String,
    pub valor_ia: String,
    pub valor_usuario: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReglaAprendida {
    pub id: String,
    pub tipo_error: String,
    pub regla: String,
    pub refuerzos: u32,
    pub actualizada: String,
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
    pub pregunta_ia: String,
    #[serde(default)]
    pub nota_usuario: String,
    #[serde(default)]
    pub exportada: bool,
}

fn estado_pendiente() -> String {
    "pendiente".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoriaAprendizaje {
    pub version: u32,
    pub aciertos: u32,
    pub reglas: Vec<ReglaAprendida>,
    pub correcciones: Vec<CorreccionIa>,
}

impl Default for MemoriaAprendizaje {
    fn default() -> Self {
        Self {
            version: VERSION_FORMATO,
            aciertos: 0,
            reglas: Vec::new(),
            correcciones: Vec::new(),
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

#[derive(Debug, Serialize)]
struct PaqueteExportacion {
    app: &'static str,
    version_app: &'static str,
    formato: u32,
    exportado: String,
    aciertos: u32,
    total_reglas: usize,
    total_correcciones: usize,
    correcciones_por_campo: BTreeMap<String, usize>,
    juzgados_agregados_usuario: Vec<String>,
    tipos_audiencia_agregados_usuario: Vec<String>,
    reglas: Vec<ReglaAprendida>,
    correcciones: Vec<CorreccionIa>,
}

fn memoria_base_embebida() -> &'static MemoriaAprendizaje {
    static BASE: OnceLock<MemoriaAprendizaje> = OnceLock::new();
    BASE.get_or_init(|| {
        serde_json::from_str::<MemoriaAprendizaje>(APRENDIZAJE_BASE_JSON).unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// Comparación de campos y similitud de textos
// ---------------------------------------------------------------------------

fn colapsar_espacios(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn quitar_tildes(s: &str) -> String {
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
    colapsar_espacios(a).to_lowercase() == colapsar_espacios(b).to_lowercase()
}

/// Coeficiente de Dice sobre bigramas de caracteres (0.0 = distintos, 1.0 = iguales).
/// Ignora mayúsculas y espacios repetidos.
pub fn similitud_textos(a: &str, b: &str) -> f64 {
    let na: Vec<char> = colapsar_espacios(a).to_lowercase().chars().collect();
    let nb: Vec<char> = colapsar_espacios(b).to_lowercase().chars().collect();
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

fn tokens_significativos(texto: &str) -> BTreeSet<String> {
    quitar_tildes(&texto.to_lowercase())
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !w.chars().all(|d| d.is_ascii_digit()))
        .map(String::from)
        .collect()
}

/// Similitud combinada (vocabulario compartido + bigramas) para encontrar los casos pasados
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
            prediccion.realizada.trim() == final_usuario.realizada.trim(),
        ),
        (
            "motivo",
            &prediccion.motivo,
            &final_usuario.motivo,
            prediccion.motivo.trim() == final_usuario.motivo.trim(),
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

pub fn agregar_o_reforzar_regla(
    memoria: &mut MemoriaAprendizaje,
    tipo_error: &str,
    regla_raw: &str,
) {
    let regla = colapsar_espacios(regla_raw);
    if regla.len() < 6 {
        return;
    }
    let tipo = if tipo_error.trim().is_empty() {
        "general".to_string()
    } else {
        tipo_error.trim().to_lowercase()
    };
    let ahora = Local::now().to_rfc3339();

    if let Some(existente) = memoria
        .reglas
        .iter_mut()
        .find(|r| es_misma_regla(&r.regla, &regla))
    {
        existente.refuerzos = existente.refuerzos.saturating_add(1);
        existente.actualizada = ahora;
        if tipo != "general" {
            existente.tipo_error = tipo;
        }
        if regla.len() > existente.regla.len() && regla.len() <= 240 {
            existente.regla = regla;
        }
        return;
    }

    memoria.reglas.push(ReglaAprendida {
        id: Local::now().format("regla-%Y%m%d%H%M%S%3f").to_string(),
        tipo_error: tipo,
        regla,
        refuerzos: 1,
        actualizada: ahora,
    });

    if memoria.reglas.len() > MAX_REGLAS {
        // Eliminar la regla con menos refuerzos (y más antigua en caso de empate)
        if let Some((idx_min, _)) = memoria
            .reglas
            .iter()
            .enumerate()
            .min_by_key(|(idx, r)| (r.refuerzos, *idx))
        {
            memoria.reglas.remove(idx_min);
        }
    }
}

/// Selecciona hasta `MAX_REGLAS_PROMPT` (5) reglas y hasta `MAX_EJEMPLOS_PROMPT` (3) correcciones
/// similares al `texto_nuevo`, combinando una memoria base y la memoria local del usuario.
pub fn seleccionar_contexto_para_prompt<'a>(
    texto_nuevo: &str,
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> (Vec<&'a ReglaAprendida>, Vec<&'a CorreccionIa>) {
    let mut candidatas_reglas: Vec<(usize, &'a ReglaAprendida)> = base
        .reglas
        .iter()
        .chain(local.reglas.iter())
        .enumerate()
        .collect();

    candidatas_reglas.sort_by(|(idx_a, a), (idx_b, b)| {
        let score_a =
            (a.refuerzos as f64) * 2.0 + similitud_consulta(texto_nuevo, &a.regla) * 6.0;
        let score_b =
            (b.refuerzos as f64) * 2.0 + similitud_consulta(texto_nuevo, &b.regla) * 6.0;
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| idx_b.cmp(idx_a))
    });

    let reglas_elegidas: Vec<&'a ReglaAprendida> = candidatas_reglas
        .into_iter()
        .map(|(_, r)| r)
        .take(MAX_REGLAS_PROMPT)
        .collect();

    let mut candidatas_correcciones: Vec<(usize, f64, &'a CorreccionIa)> = base
        .correcciones
        .iter()
        .chain(local.correcciones.iter())
        .filter(|c| c.estado != "descartada" && !c.campos.is_empty())
        .enumerate()
        .map(|(idx, c)| (idx, similitud_consulta(texto_nuevo, &c.texto_original), c))
        .filter(|(_, sim, _)| *sim > 0.02)
        .collect();

    candidatas_correcciones.sort_by(|(idx_a, sim_a, _), (idx_b, sim_b, _)| {
        sim_b
            .partial_cmp(sim_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| idx_b.cmp(idx_a))
    });

    let ejemplos_elegidos: Vec<&'a CorreccionIa> = candidatas_correcciones
        .into_iter()
        .map(|(_, _, c)| c)
        .take(MAX_EJEMPLOS_PROMPT)
        .collect();

    (reglas_elegidas, ejemplos_elegidos)
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

/// Construye el bloque de texto que se inyecta en `construir_prompt` con máximo 5 reglas y 3 ejemplos.
pub fn formatear_contexto_prompt(
    texto_nuevo: &str,
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
) -> String {
    let (reglas, ejemplos) = seleccionar_contexto_para_prompt(texto_nuevo, base, local);
    if reglas.is_empty() && ejemplos.is_empty() {
        return String::new();
    }

    let mut lineas = Vec::new();
    lineas.push(
        "APRENDIZAJE DE CORRECCIONES PREVIAS (aplica estas reglas y ejemplos con prioridad):"
            .to_string(),
    );

    if !reglas.is_empty() {
        lineas.push("Reglas aprendidas del usuario:".to_string());
        for r in reglas {
            lineas.push(format!("- [{}] {}", r.tipo_error, r.regla));
        }
    }

    if !ejemplos.is_empty() {
        lineas.push("Ejemplos corregidos en textos similares:".to_string());
        for ej in ejemplos {
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
                format!(" (Aclaración del usuario: {})", recortar_texto(&ej.nota_usuario, 100))
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
    }

    lineas.join("\n")
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

/// Reemplaza secuencias numéricas de 7 o más dígitos (radicados, cédulas, teléfonos) por
/// `[NUMERO]`, conservando fechas y horas, que son útiles para aprender formatos.
fn enmascarar_numeros_largos(texto: &str) -> String {
    let Some(re) = re_numeros_largos() else {
        return texto.to_string();
    };
    re.replace_all(texto, |caps: &regex::Captures| {
        let m = &caps[0];
        let digitos = m.chars().filter(|c| c.is_ascii_digit()).count();
        let es_fecha = re_fecha_con_guiones().is_some_and(|rf| rf.is_match(m));
        if digitos >= 7 && !es_fecha {
            "[NUMERO]".to_string()
        } else {
            m.to_string()
        }
    })
    .into_owned()
}

/// Reemplaza el nombre del procesado y el número de radicado (conocidos por el campo Radicado,
/// con formato habitual `NOMBRE - NÚMERO`) por marcadores genéricos.
struct Anonimizador {
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
                    if let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(&parte))) {
                        completos.push((re, "[RADICADO]"));
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
pub fn anonimizar_correccion(c: &CorreccionIa) -> CorreccionIa {
    let anon =
        Anonimizador::desde_radicados(&[&c.prediccion.radicado, &c.final_usuario.radicado]);
    CorreccionIa {
        texto_original: anon.aplicar(&c.texto_original),
        prediccion: anon.aplicar_audiencia(&c.prediccion),
        final_usuario: anon.aplicar_audiencia(&c.final_usuario),
        campos: c
            .campos
            .iter()
            .map(|campo| CampoCorregido {
                campo: campo.campo.clone(),
                valor_ia: anon.aplicar(&campo.valor_ia),
                valor_usuario: anon.aplicar(&campo.valor_usuario),
            })
            .collect(),
        regla_generada: anon.aplicar(&c.regla_generada),
        nota_usuario: anon.aplicar(&c.nota_usuario),
        ..c.clone()
    }
}

// ---------------------------------------------------------------------------
// Persistencia y ciclo de vida del análisis
// ---------------------------------------------------------------------------

pub fn ruta_memoria() -> PathBuf {
    ruta_en_carpeta_config(NOMBRE_ARCHIVO_MEMORIA)
}

/// Carga la memoria; si el archivo está dañado lo conserva como `.bak` y empieza de cero.
pub fn cargar_memoria_en(ruta: &Path) -> MemoriaAprendizaje {
    let Ok(contenido) = fs::read_to_string(ruta) else {
        return MemoriaAprendizaje::default();
    };
    match serde_json::from_str::<MemoriaAprendizaje>(&contenido) {
        Ok(memoria) => memoria,
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
        total_reglas: memoria.reglas.len(),
        pendientes_exportar: memoria.correcciones.iter().filter(|c| !c.exportada).count(),
        hubo_correccion,
        aclaracion_pendiente: ultima_aclaracion_pendiente(memoria),
    }
}

pub fn registrar_resultado_en(
    ruta: &Path,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    proveedor: &str,
    modelo: &str,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    if texto.trim().is_empty() {
        return Err("No hay texto de IA asociado a este registro.".to_string());
    }

    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let campos = detectar_correcciones(prediccion, final_usuario);
    let hubo_correccion = !campos.is_empty();
    let mut memoria = cargar_memoria_en(ruta);
    let mut nueva_correccion = None;

    if hubo_correccion {
        let ahora = Local::now();
        let corr = CorreccionIa {
            id: ahora.format("%Y%m%d%H%M%S%3f").to_string(),
            registrada: ahora.to_rfc3339(),
            proveedor: proveedor.to_string(),
            modelo: modelo.to_string(),
            texto_original: texto.trim().to_string(),
            prediccion: prediccion.clone(),
            final_usuario: final_usuario.clone(),
            campos,
            estado: estado_pendiente(),
            regla_generada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
        };
        nueva_correccion = Some(corr.clone());
        memoria.correcciones.push(corr);
        if memoria.correcciones.len() > MAX_CORRECCIONES {
            let exceso = memoria.correcciones.len() - MAX_CORRECCIONES;
            memoria.correcciones.drain(0..exceso);
        }
    } else {
        memoria.aciertos = memoria.aciertos.saturating_add(1);
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok((resumen_de(&memoria, hubo_correccion), nueva_correccion))
}

/// Aplica el resultado de la 2ª llamada a la IA sobre una corrección guardada.
pub fn aplicar_analisis_en(
    ruta: &Path,
    correccion_id: &str,
    analisis: &AnalisisCorreccionIa,
) -> Result<ResumenAprendizaje, String> {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut memoria = cargar_memoria_en(ruta);

    let mut regla_a_registrar: Option<(String, String)> = None;
    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        if !analisis.es_error_de_ia {
            corr.estado = "descartada".to_string();
            corr.pregunta_ia.clear();
        } else if analisis.necesita_aclaracion
            && analisis.confianza < UMBRAL_CONFIANZA_ACLARACION
            && !analisis.pregunta.trim().is_empty()
            && corr.nota_usuario.trim().is_empty()
        {
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
                regla_a_registrar = Some((analisis.tipo_error.clone(), limpia));
            }
        }
    }

    if let Some((tipo, regla)) = regla_a_registrar {
        agregar_o_reforzar_regla(&mut memoria, &tipo, &regla);
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, true))
}

/// Registra la nota explicativa del usuario para una corrección que pedía aclaración.
/// Devuelve la corrección actualizada para que pueda re-analizarse con la IA si hay conexión,
/// y además registra de inmediato la nota como regla para no perderla aunque esté sin internet.
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
    let mut corr_actualizada = None;
    let mut campo_principal = "general".to_string();

    if let Some(corr) = memoria
        .correcciones
        .iter_mut()
        .find(|c| c.id == correccion_id)
    {
        if let Some(primero) = corr.campos.first() {
            campo_principal = primero.campo.clone();
        }
        corr.nota_usuario = nota_limpia.clone();
        corr.pregunta_ia.clear();
        corr.estado = "analizada".to_string();
        if corr.regla_generada.trim().is_empty() {
            corr.regla_generada = nota_limpia.clone();
        }
        corr_actualizada = Some(corr.clone());
    }

    agregar_o_reforzar_regla(&mut memoria, &campo_principal, &nota_limpia);
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
        agregar_o_reforzar_regla(&mut memoria, "general", &regla);
    }

    guardar_memoria_en(ruta, &memoria)?;
    Ok(resumen_de(&memoria, false))
}

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

    let paquete = PaqueteExportacion {
        app: "Lokus",
        version_app: env!("CARGO_PKG_VERSION"),
        formato: VERSION_FORMATO,
        exportado: Local::now().to_rfc3339(),
        aciertos: memoria.aciertos,
        total_reglas: memoria.reglas.len(),
        total_correcciones: memoria.correcciones.len(),
        correcciones_por_campo,
        juzgados_agregados_usuario: juzgados_usuario.to_vec(),
        tipos_audiencia_agregados_usuario: tipos_usuario.to_vec(),
        reglas: memoria.reglas.clone(),
        correcciones: memoria.correcciones.iter().map(anonimizar_correccion).collect(),
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

pub fn obtener_contexto_prompt(texto_nuevo: &str) -> String {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let local = cargar_memoria_en(&ruta_memoria());
    formatear_contexto_prompt(texto_nuevo, memoria_base_embebida(), &local)
}

pub fn registrar_resultado_ia(
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
) -> Result<(ResumenAprendizaje, Option<CorreccionIa>), String> {
    let estado = estado_desde_settings(&cargar_settings());
    registrar_resultado_en(
        &ruta_memoria(),
        texto,
        prediccion,
        final_usuario,
        &estado.provider,
        &estado.model,
    )
}

pub fn obtener_resumen_aprendizaje() -> ResumenAprendizaje {
    let _guard = BLOQUEO_MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    resumen_de(&cargar_memoria_en(&ruta_memoria()), false)
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
}
