//! Motor general de abreviaciones y generalización de patrones parametrizados para Lokus.
//!
//! No depende de reglas estáticas ni de un único prefijo (`J#`): aprende e infiere
//! dinámicamente cualquier convención de abreviación utilizada por cada defensor
//! (o promovida al Cuaderno General `"*"`), cubriendo:
//!
//! 1. **Abreviaciones con slot numérico `{N}`** (en cualquier posición: prefijo, sufijo,
//!    infijo o multipalabra, con arábigos u ordinales españoles en el canónico):
//!    ej. `J10`/`J04`/`J-4`, `JPM10`/`JPM 4`, `J10M`/`J4 M`, `J10PM`, `EPMS 2`,
//!    `10 MPAL`, `J1 CG`, `P4`, `C2`, `Juzgado 3 Circuito Penal -> Juzgado 3 Cto Penal`,
//!    o `J1 -> Juzgado Primero Penal Municipal`.
//! 2. **Abreviaciones con slot léxico `{X}`** (ej. municipio o tipo de audiencia):
//!    ej. `JPM Bosconia` / `JPM Curumaní` -> `JPM Chimichagua` o `JPM La Jagua de Ibirico`;
//!    `AUD ACUSACION` / `AUD IMPUTACION` -> `AUD NULIDAD` / `AUD PRORROGA`.
//! 3. **Abreviaciones fijas / exactas** de 2+ caracteres (ej. `JO`, `CG`, `PREP`, `TSV`, `JPM`).
//!
//! **Candados de seguridad invariantes:**
//! - Aislamiento estricto por defensor (`defensor_alcance`), con respaldo al Cuaderno General `"*"`
//!   solo cuando el defensor no contradice ese patrón.
//! - Consistencia alta obligatoria (`>= 0.85` en `{N}` con `>= 2` instancias distintas, cero
//!   conflictos en muestras pequeñas `2..=5`; `>= 0.90` en `{X}`; `q10 >= 0.30`).
//! - Cualquier alias/corrección específica para un valor concreto (incluso tras 1 sola corrección
//!   o una retracción explícita) **siempre** tiene prioridad y bloquea al patrón general.
//! - Nunca sobrescribe ni pisa calificadores explícitos de despacho (`CTO`, `Circuito`,
//!   `Conocimiento`, `MPAL`, `PCTO`, `Especializado`, `Promiscuo`, `M`, `C`, etc.), ni confunde
//!   fechas, horas (`4pm`, `10am`, `14:30`), años o radicados.
//! - Los patrones inferidos se entregan a la IA únicamente como **sugerencia suave** en el prompt
//!   (nunca se imponen a ciegas en arbitraje post-LLM).

use std::collections::{BTreeMap, BTreeSet};

use crate::aprendizaje::{
    colapsar_espacios, limite_inferior_bayesiano, quitar_tildes, AliasEstadistico,
    MemoriaAprendizaje, PRIOR_ALIAS_ALPHA, PRIOR_ALIAS_BETA,
};
use crate::config::AppSettings;
use crate::estadistica_ia::{juzgados_base, normalizar_clave_entidad};
use crate::models::{
    coincide_alcance_defensor, es_alcance_general, normalizar_clave_catalogo,
    normalizar_clave_defensor, IndicePriorsExcel, TIPOS_AUDIENCIA,
};

const MAX_SLOT_NUM: u32 = 200;
const UMBRAL_CONSISTENCIA_NUM: f64 = 0.85;
const UMBRAL_CONSISTENCIA_TEXTO: f64 = 0.90;
const UMBRAL_Q10_PATRON: f64 = 0.30;
const UMBRAL_Q10_ALIAS_ACTIVO: f64 = 0.22;
const UMBRAL_Q10_ALIAS_EXACTO: f64 = 0.30;

const STOPWORDS_ALIAS: &[&str] = &[
    "de", "del", "la", "las", "los", "el", "en", "y", "a", "o", "u", "un", "una", "unos", "unas",
    "por", "para", "con", "que", "es", "al", "lo", "le", "les",
];

const PALABRAS_CORTAS_COMUNES_2: &[&str] = &[
    "no", "si", "de", "la", "el", "en", "al", "lo", "le", "se", "su", "me", "mi", "te", "ti",
    "tu", "un", "es", "ya", "ni", "ha", "he", "va", "ve", "ir", "vi", "di", "da", "do", "fe",
    "so", "yo", "os", "am", "pm", "dr", "sr", "na", "ok",
];

const SUFIJOS_TIEMPO: &[&str] = &["am", "pm", "h", "hr", "hrs", "min", "mins"];

/// Calificadores terminales de tipo/categoría de despacho (distinguen el nivel o función del
/// juzgado: Circuito, Municipal, Promiscuo, Especializado, Garantías, Conocimiento, EPMS, etc.).
pub(crate) const CALIFICADORES_TERMINALES_JUZGADO: &[&str] = &[
    "cto",
    "circuito",
    "conocimiento",
    "conoc",
    "con",
    "mpal",
    "municipal",
    "mun",
    "pcto",
    "promiscuo",
    "prom",
    "especializado",
    "especializada",
    "ezado",
    "ezada",
    "esp",
    "cg",
    "garantias",
    "control",
    "epms",
    "jepms",
    "ejecucion",
    "penas",
    "adolescentes",
    "pm",
    "pc",
    "m",
    "c",
    "p",
    "e",
    "g",
];

/// Todos los calificadores de despacho (incluye especialidad/rama como `penal`, `civil`, `familia`,
/// `laboral`, `mixto`, `tribunal`, `sala` además de los calificadores terminales).
pub(crate) const CALIFICADORES_JUZGADO: &[&str] = &[
    "cto",
    "circuito",
    "conocimiento",
    "conoc",
    "con",
    "mpal",
    "municipal",
    "mun",
    "pcto",
    "promiscuo",
    "prom",
    "especializado",
    "especializada",
    "ezado",
    "ezada",
    "esp",
    "cg",
    "garantias",
    "control",
    "epms",
    "jepms",
    "ejecucion",
    "penas",
    "adolescentes",
    "familia",
    "civil",
    "laboral",
    "penal",
    "mixto",
    "tribunal",
    "sala",
    "pm",
    "pc",
    "m",
    "c",
    "p",
    "e",
    "g",
];

const PREFIJOS_NO_JUZGADO_NUMERICOS: &[&str] = &[
    "f",
    "fis",
    "fisc",
    "fiscal",
    "fiscalia",
    "art",
    "articulo",
    "ley",
    "inc",
    "inciso",
    "cap",
    "capitulo",
    "num",
    "nro",
    "rad",
    "radicado",
    "exp",
    "expediente",
    "cui",
    "spoa",
    "tel",
    "cel",
    "cc",
    "nit",
    "id",
    "res",
    "resolucion",
    "of",
    "oficio",
    "fol",
    "folio",
    "pag",
    "pagina",
];

const RAICES_DESPACHO_SEPARADO: &[&str] = &[
    "j", "jdo", "jzg", "juz", "juzgado", "jpm", "jpc", "jcg", "jcon", "epms", "jepms", "jm",
    "jc", "jp", "je", "jg", "sala", "desp", "trib",
];

const MESES_ES: &[&str] = &[
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "setiembre",
    "octubre",
    "noviembre",
    "diciembre",
    "ene",
    "feb",
    "mar",
    "abr",
    "may",
    "jun",
    "jul",
    "ago",
    "sep",
    "sept",
    "oct",
    "nov",
    "dic",
];

const BLOQUEO_SLOT_TEXTO_JUZGADO: &[&str] = &[
    "audiencia",
    "aud",
    "preliminar",
    "preparatoria",
    "juicio",
    "oral",
    "acusacion",
    "imputacion",
    "concentrada",
    "preclusion",
    "nulidad",
    "prorroga",
    "conciliacion",
    "reparacion",
    "incidente",
    "medida",
    "medidas",
    "aseguramiento",
    "alegatos",
    "conclusion",
    "individualizacion",
    "verificacion",
    "cumplimiento",
    "revision",
    "lectura",
    "fallo",
    "sentencia",
    "diligencia",
    "realizo",
    "realizada",
    "efectuo",
    "instalo",
    "instalada",
    "inicio",
    "fracaso",
    "fracsa",
    "reprogramo",
    "reprogramada",
    "reprograma",
    "reprogramacion",
    "aplazo",
    "aplaza",
    "aplazamiento",
    "suspendio",
    "suspende",
    "fija",
    "fijo",
    "nueva",
    "fecha",
    "hora",
    "horas",
    "radicado",
    "proceso",
    "procesado",
    "imputado",
    "acusado",
    "usuario",
    "fiscal",
    "fiscalia",
    "juez",
    "defensor",
    "abogado",
    "contractual",
    "privado",
    "publico",
    "confianza",
    "inpec",
    "victima",
    "icbf",
    "testigo",
    "testigos",
    "novedad",
    "motivo",
    "observaciones",
    "solicite",
    "solicito",
    "solicita",
    "pedi",
    "pido",
    "comparecio",
    "inasistente",
    "ausente",
    "conecto",
    "conexion",
    "virtual",
    "presencial",
    "vacaciones",
    "agenda",
    "despacho",
    "titular",
    "carpeta",
    "traslado",
    "detenido",
    "preacuerdo",
    "estuvo",
    "ocupado",
    "ocupada",
    "remitio",
    "manana",
    "tarde",
    "noche",
    "hoy",
    "ayer",
    "lunes",
    "martes",
    "miercoles",
    "jueves",
    "viernes",
    "sabado",
    "domingo",
    "contra",
    "sobre",
    "entre",
    "hasta",
    "desde",
    "durante",
    "segun",
    "porque",
    "pero",
    "aunque",
    "tampoco",
    "nunca",
    "jamas",
    "imposible",
];

const BLOQUEO_SLOT_TEXTO_TIPO: &[&str] = &[
    "realizo",
    "realizada",
    "efectuo",
    "instalo",
    "instalada",
    "inicio",
    "fracaso",
    "fracsa",
    "reprogramo",
    "reprogramada",
    "reprograma",
    "aplazo",
    "aplaza",
    "suspendio",
    "suspende",
    "fija",
    "fijo",
    "nueva",
    "fecha",
    "hora",
    "horas",
    "radicado",
    "proceso",
    "procesado",
    "usuario",
    "fiscal",
    "fiscalia",
    "juez",
    "juzgado",
    "defensor",
    "abogado",
    "inpec",
    "victima",
    "icbf",
    "novedad",
    "motivo",
    "observaciones",
    "solicite",
    "solicito",
    "solicita",
    "pedi",
    "comparecio",
    "ausente",
    "conecto",
    "virtual",
    "manana",
    "tarde",
    "noche",
    "hoy",
    "ayer",
    "lunes",
    "martes",
    "miercoles",
    "jueves",
    "viernes",
    "sabado",
    "domingo",
    "contra",
    "despacho",
    "sala",
    "estuvo",
    "ocupado",
];

const BOILERPLATE_JUZGADO_EXTRACCION: &[&str] = &[
    "juzgado",
    "jdo",
    "jzg",
    "juz",
    "jpm",
    "jpc",
    "jcg",
    "jepms",
    "epms",
    "penal",
    "municipal",
    "mpal",
    "circuito",
    "cto",
    "pcto",
    "promiscuo",
    "prom",
    "conocimiento",
    "control",
    "garantias",
    "funcion",
    "funciones",
    "especializado",
    "especializada",
    "ejecucion",
    "penas",
    "medidas",
    "seguridad",
    "adolescentes",
    "tribunal",
    "superior",
    "distrito",
    "judicial",
    "sala",
    "despacho",
    "civil",
    "familia",
    "laboral",
    "mixto",
];

const BOILERPLATE_TIPO_EXTRACCION: &[&str] = &["audiencia", "aud", "diligencia", "otra"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoSlotPatron {
    Numerico,
    Texto,
}

impl TipoSlotPatron {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Numerico => "numerico",
            Self::Texto => "texto",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatronAbreviacionInferido {
    pub campo: String,
    pub tipo_slot: TipoSlotPatron,
    pub patron_entrada: String,
    pub patron_canonico: String,
    pub defensor_alcance: String,
    pub ejemplos_distintos: usize,
    pub total_instancias: usize,
    pub consistencia: f64,
    pub confianza_q10: f64,
    pub origen: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolucionPatronInferido {
    pub campo: String,
    pub abreviacion_detectada: String,
    pub expansion_sugerida: String,
    pub patron_entrada: String,
    pub ejemplos_distintos: usize,
    pub total_instancias: usize,
    pub consistencia: f64,
    pub confianza_q10: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolucionAbreviacionesTexto {
    pub exactas: Vec<(String, String, String)>,
    pub inferidas: Vec<ResolucionPatronInferido>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SegmentoPatron {
    Alpha(String),
    Num(u32),
    Otro(String),
}

// ---------------------------------------------------------------------------
// Normalización general de siglas, ordinales y segmentación estructural
// ---------------------------------------------------------------------------

/// Colapsa siglas de letras individuales separadas por puntos (`"J.O."` -> `"JO"`,
/// `"J.P.M. 4"` -> `"JPM 4"`, `"E.P.M.S."` -> `"EPMS"`) sin alterar abreviaturas
/// multipalabra (`"Jdo. 4"`, `"Dr. Ana"`) ni expresiones separadas por espacio (`"p. m."`).
pub(crate) fn colapsar_siglas_con_puntos(texto: &str) -> String {
    let chars: Vec<char> = texto.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(texto.len());
    let mut i = 0;

    while i < n {
        let inicio_palabra = i == 0 || !chars[i - 1].is_alphanumeric();
        if inicio_palabra && chars[i].is_ascii_alphabetic() {
            let mut letras = String::new();
            let mut j = i;
            let mut puntos = 0usize;
            while j < n && chars[j].is_ascii_alphabetic() {
                if j + 1 < n && chars[j + 1] == '.' {
                    letras.push(chars[j]);
                    puntos += 1;
                    j += 2;
                } else if puntos >= 1 && (j + 1 >= n || !chars[j + 1].is_alphanumeric()) {
                    letras.push(chars[j]);
                    j += 1;
                    break;
                } else {
                    break;
                }
            }
            if letras.len() >= 2 && puntos >= 1 && (j >= n || !chars[j].is_ascii_alphabetic()) {
                out.push_str(&letras);
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn normalizar_ordinal_token_alias(tok: &str) -> Option<&'static str> {
    match tok {
        "primero" | "primera" | "1ro" | "1ra" | "1o" | "1a" => Some("1"),
        "segundo" | "segunda" | "2do" | "2da" | "2o" | "2a" => Some("2"),
        "tercero" | "tercera" | "3ro" | "3ra" | "3o" | "3a" => Some("3"),
        "cuarto" | "cuarta" | "4to" | "4ta" | "4o" | "4a" => Some("4"),
        "quinto" | "quinta" | "5to" | "5ta" | "5o" | "5a" => Some("5"),
        "sexto" | "sexta" | "6to" | "6ta" | "6o" | "6a" => Some("6"),
        "septimo" | "septima" | "7mo" | "7ma" | "7o" | "7a" => Some("7"),
        "octavo" | "octava" | "8vo" | "8va" | "8o" | "8a" => Some("8"),
        "noveno" | "novena" | "9no" | "9na" | "9o" | "9a" => Some("9"),
        "decimo" | "decima" | "10mo" | "10ma" | "10o" | "10a" => Some("10"),
        "undecimo" | "undecima" | "11mo" | "11o" | "11a" => Some("11"),
        "duodecimo" | "duodecima" | "12mo" | "12o" | "12a" => Some("12"),
        "decimotercero" | "decimotercera" | "13o" | "13a" => Some("13"),
        "decimocuarto" | "decimocuarta" | "14o" | "14a" => Some("14"),
        "decimoquinto" | "decimoquinta" | "15o" | "15a" => Some("15"),
        "decimosexto" | "decimosexta" | "16o" | "16a" => Some("16"),
        "decimoseptimo" | "decimoseptima" | "17o" | "17a" => Some("17"),
        "decimoctavo" | "decimoctava" | "18o" | "18a" => Some("18"),
        "decimonoveno" | "decimonovena" | "19o" | "19a" => Some("19"),
        "vigesimo" | "vigesima" | "20o" | "20a" => Some("20"),
        _ => None,
    }
}

/// Quita ceros a la izquierda en corridas numéricas pequeñas (`1..=99999`) dentro de tokens
/// alfanuméricos (`"j04"` -> `"j4"`, `"jpm04"` -> `"jpm4"`, `"j04m"` -> `"j4m"`, `"04mpal"` -> `"4mpal"`).
fn normalizar_digitos_internos(tok: &str) -> String {
    let mut out = String::with_capacity(tok.len());
    let chars: Vec<char> = tok.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            if run.len() < 6 {
                let sin_ceros = run.trim_start_matches('0');
                if sin_ceros.is_empty() {
                    out.push('0');
                } else {
                    out.push_str(sin_ceros);
                }
            } else {
                out.push_str(&run);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn es_simbolo_ordinal(ch: char) -> bool {
    matches!(ch, 'º' | 'ª' | '°')
}

fn es_char_palabra(ch: char) -> bool {
    ch.is_alphanumeric() && !es_simbolo_ordinal(ch)
}

/// Tokeniza una entrada o texto de alias preservando letras aisladas significativas
/// (`"J 4"` -> `["j", "4"]`, `"J4 M"` -> `["j4", "m"]`, `"J4 C"` -> `["j4", "c"]`),
/// colapsando siglas con puntos (`"J.O."` -> `["jo"]`) y unificando ceros a la izquierda
/// (`"JPM04"` -> `["jpm4"]`), símbolos ordinales (`"J4º"` -> `["j4"]`) y ordinales (`"Tercero"` -> `["3"]`).
pub(crate) fn tokenizar_para_alias(texto: &str) -> Vec<String> {
    let colapsado = colapsar_siglas_con_puntos(texto);
    let lower = quitar_tildes(&colapsado.to_lowercase());
    let mut tokens = Vec::new();
    for raw in lower.split(|c: char| !es_char_palabra(c)) {
        if raw.is_empty() {
            continue;
        }
        if let Some(ord) = normalizar_ordinal_token_alias(raw) {
            tokens.push(ord.to_string());
            continue;
        }
        if STOPWORDS_ALIAS.contains(&raw) {
            continue;
        }
        if raw.chars().all(|c| c.is_ascii_digit()) {
            if raw.len() >= 6 {
                continue;
            }
            let sin_ceros = raw.trim_start_matches('0');
            tokens.push(if sin_ceros.is_empty() {
                "0".to_string()
            } else {
                sin_ceros.to_string()
            });
            continue;
        }
        tokens.push(normalizar_digitos_internos(raw));
    }
    tokens
}

pub(crate) fn canonizar_entrada_alias_general(entrada_raw: &str) -> String {
    tokenizar_para_alias(entrada_raw).join(" ")
}

/// Descompone una secuencia de tokens normalizados en segmentos atómicos `Alpha`/`Num`/`Otro`,
/// partiendo únicamente en transiciones letra<->dígito para unificar `"JPM4"`, `"JPM 4"`,
/// `"JPM-04"` y `"J.P.M. 4"` bajo `[Alpha("jpm"), Num(4)]`, sin fusionar palabras alfabéticas
/// separadas.
pub(crate) fn segmentar_tokens_alias(tokens: &[String]) -> Vec<SegmentoPatron> {
    let mut segs = Vec::new();
    for tok in tokens {
        let chars: Vec<char> = tok.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_ascii_digit() {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let run: String = chars[start..i].iter().collect();
                if let Ok(n) = run.parse::<u32>() {
                    if (1..=MAX_SLOT_NUM).contains(&n) {
                        segs.push(SegmentoPatron::Num(n));
                    } else {
                        segs.push(SegmentoPatron::Otro(run));
                    }
                } else {
                    segs.push(SegmentoPatron::Otro(run));
                }
            } else if chars[i].is_alphabetic() {
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let run: String = chars[start..i].iter().collect();
                segs.push(SegmentoPatron::Alpha(run));
            } else {
                i += 1;
            }
        }
    }
    segs
}

pub(crate) fn clave_estructural_alias(texto: &str) -> Vec<SegmentoPatron> {
    let toks = tokenizar_para_alias(texto);
    segmentar_tokens_alias(&toks)
}

fn es_patron_hora_ambigua(segs: &[SegmentoPatron]) -> bool {
    matches!(
        segs,
        [SegmentoPatron::Num(_), SegmentoPatron::Alpha(suf)]
            if SUFIJOS_TIEMPO.contains(&suf.as_str())
    )
}

/// Valida si una clave de alias normalizada es segura para almacenarse (acepta siglas de 2
/// caracteres legítimas como `j4`, `m4`, `c2`, `jo`, `cg`, `ap`, bloqueando palabras vacías,
/// números desnudos y expresiones horarias como `4pm` / `10am`).
pub(crate) fn es_clave_alias_valida(entrada_norm: &str) -> bool {
    let trimmed = entrada_norm.trim();
    if trimmed.len() < 2 {
        return false;
    }
    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let segs = clave_estructural_alias(trimmed);
    if segs.is_empty() || es_patron_hora_ambigua(&segs) {
        return false;
    }
    if trimmed.len() == 2 {
        if PALABRAS_CORTAS_COMUNES_2.contains(&trimmed) {
            return false;
        }
        return trimmed.chars().all(|c| c.is_ascii_alphanumeric());
    }
    true
}

/// Detecta si `texto` es una abreviación corta de juzgado (para priorizar aclaraciones
/// adaptativas sin limitarse a `J#`: ej. `J4`, `JPM4`, `J4M`, `EPMS 2`, `JPC3`, `JPM`).
pub(crate) fn es_abreviacion_corta_juzgado(texto: &str) -> bool {
    let norm = canonizar_entrada_alias_general(texto);
    if !es_clave_alias_valida(&norm) || norm.len() > 14 {
        return false;
    }
    let toks: Vec<&str> = norm.split_whitespace().collect();
    if toks.is_empty() || toks.len() > 3 {
        return false;
    }
    // Si tiene palabra completa "juzgado" junto con calificadores largos no es una sigla corta
    if toks.iter().any(|t| *t == "juzgado") && toks.len() >= 3 {
        return false;
    }
    let segs = segmentar_tokens_alias(&toks.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
    let tiene_num = segs.iter().any(|s| matches!(s, SegmentoPatron::Num(_)));
    let solo_cortos = toks.iter().all(|t| t.len() <= 6);
    (tiene_num && solo_cortos) || (toks.len() == 1 && toks[0].len() <= 5)
}

fn es_identidad_alias(entrada: &str, canonico: &str) -> bool {
    let e = normalizar_clave_entidad(entrada);
    let c = normalizar_clave_entidad(canonico);
    if !e.is_empty() && e == c {
        return true;
    }
    canonizar_entrada_alias_general(entrada) == canonizar_entrada_alias_general(canonico)
}

// ---------------------------------------------------------------------------
// Ordinales españoles (1..=20) para plantillas canónicas
// ---------------------------------------------------------------------------

const ORDINALES_MASC: [&str; 20] = [
    "Primero",
    "Segundo",
    "Tercero",
    "Cuarto",
    "Quinto",
    "Sexto",
    "Séptimo",
    "Octavo",
    "Noveno",
    "Décimo",
    "Undécimo",
    "Duodécimo",
    "Decimotercero",
    "Decimocuarto",
    "Decimoquinto",
    "Decimosexto",
    "Decimoséptimo",
    "Decimoctavo",
    "Decimonoveno",
    "Vigésimo",
];

const ORDINALES_FEM: [&str; 20] = [
    "Primera",
    "Segunda",
    "Tercera",
    "Cuarta",
    "Quinta",
    "Sexta",
    "Séptima",
    "Octava",
    "Novena",
    "Décima",
    "Undécima",
    "Duodécima",
    "Decimotercera",
    "Decimocuarta",
    "Decimoquinta",
    "Decimosexta",
    "Decimoséptima",
    "Decimoctava",
    "Decimonovena",
    "Vigésima",
];

const ORDINALES_ABREV_M: [&str; 20] = [
    "1ro", "2do", "3ro", "4to", "5to", "6to", "7mo", "8vo", "9no", "10mo", "11mo", "12mo", "13o",
    "14o", "15o", "16o", "17o", "18o", "19o", "20o",
];

const ORDINALES_ABREV_F: [&str; 20] = [
    "1ra", "2da", "3ra", "4ta", "5ta", "6ta", "7ma", "8va", "9na", "10ma", "11a", "12a", "13a",
    "14a", "15a", "16a", "17a", "18a", "19a", "20a",
];

fn detectar_estilo_caja_palabra(raw: &str) -> u8 {
    // 0 = lower, 1 = Title, 2 = UPPER
    let letras: Vec<char> = raw.chars().filter(|c| c.is_alphabetic()).collect();
    if letras.is_empty() {
        return 1;
    }
    if letras.iter().all(|c| c.is_uppercase()) {
        2
    } else if letras[0].is_uppercase() {
        1
    } else {
        0
    }
}

fn parsear_ordinal_con_estilo(raw: &str) -> Option<(u32, &'static str)> {
    let norm = quitar_tildes(&raw.to_lowercase());
    let caja = detectar_estilo_caja_palabra(raw);
    for (idx, &ord) in ORDINALES_MASC.iter().enumerate() {
        if quitar_tildes(&ord.to_lowercase()) == norm {
            let ph = match caja {
                2 => "{ORD_M_UP}",
                0 => "{ord_m}",
                _ => "{ORD_M}",
            };
            return Some(((idx + 1) as u32, ph));
        }
    }
    for (idx, &ord) in ORDINALES_FEM.iter().enumerate() {
        if quitar_tildes(&ord.to_lowercase()) == norm {
            let ph = match caja {
                2 => "{ORD_F_UP}",
                0 => "{ord_f}",
                _ => "{ORD_F}",
            };
            return Some(((idx + 1) as u32, ph));
        }
    }
    for (idx, &ord) in ORDINALES_ABREV_M.iter().enumerate() {
        if ord == norm {
            return Some(((idx + 1) as u32, "{ORD_ABREV_M}"));
        }
    }
    for (idx, &ord) in ORDINALES_ABREV_F.iter().enumerate() {
        if ord == norm {
            return Some(((idx + 1) as u32, "{ORD_ABREV_F}"));
        }
    }
    None
}

fn formatear_slot_numerico(n: u32, placeholder: &str) -> Option<String> {
    if placeholder == "{N}" {
        return Some(n.to_string());
    }
    let idx = n.checked_sub(1)? as usize;
    if idx >= 20 {
        return None;
    }
    match placeholder {
        "{ORD_M}" => Some(ORDINALES_MASC[idx].to_string()),
        "{ord_m}" => Some(ORDINALES_MASC[idx].to_lowercase()),
        "{ORD_M_UP}" => Some(ORDINALES_MASC[idx].to_uppercase()),
        "{ORD_F}" => Some(ORDINALES_FEM[idx].to_string()),
        "{ord_f}" => Some(ORDINALES_FEM[idx].to_lowercase()),
        "{ORD_F_UP}" => Some(ORDINALES_FEM[idx].to_uppercase()),
        "{ORD_ABREV_M}" => Some(ORDINALES_ABREV_M[idx].to_string()),
        "{ORD_ABREV_F}" => Some(ORDINALES_ABREV_F[idx].to_string()),
        _ => None,
    }
}

fn aplicar_estilo_slot_texto(slot_raw: &str, placeholder: &str) -> String {
    match placeholder {
        "{x}" => slot_raw.to_lowercase(),
        "{X_UP}" => slot_raw.to_uppercase(),
        _ => {
            // {X_TITLE}: Title Case preservando preposiciones/artículos internos en minúscula
            const CONECTORES_INTERNOS: &[&str] = &["de", "del", "la", "las", "los", "el", "y"];
            slot_raw
                .split_whitespace()
                .enumerate()
                .map(|(idx, w)| {
                    let lower = w.to_lowercase();
                    if idx > 0 && CONECTORES_INTERNOS.contains(&lower.as_str()) {
                        lower
                    } else {
                        let mut cs = lower.chars();
                        match cs.next() {
                            None => String::new(),
                            Some(first) => {
                                let mut s: String = first.to_uppercase().collect();
                                s.push_str(cs.as_str());
                                s
                            }
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

// ---------------------------------------------------------------------------
// Extracción de plantillas desde alias individuales
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SpanPalabra<'a> {
    start: usize,
    end: usize,
    raw: &'a str,
    norm: String,
}

fn extraer_spans_palabras(texto: &str) -> Vec<SpanPalabra<'_>> {
    let mut out = Vec::new();
    let mut inicio: Option<usize> = None;
    for (idx, ch) in texto.char_indices() {
        if es_char_palabra(ch) {
            if inicio.is_none() {
                inicio = Some(idx);
            }
        } else if let Some(st) = inicio.take() {
            let raw = &texto[st..idx];
            out.push(SpanPalabra {
                start: st,
                end: idx,
                raw,
                norm: quitar_tildes(&raw.to_lowercase()),
            });
        }
    }
    if let Some(st) = inicio {
        let raw = &texto[st..];
        out.push(SpanPalabra {
            start: st,
            end: texto.len(),
            raw,
            norm: quitar_tildes(&raw.to_lowercase()),
        });
    }
    out
}

#[derive(Debug, Clone)]
struct PlantillaExtraida {
    tipo_slot: TipoSlotPatron,
    patron_entrada: String,
    patron_canonico: String,
    slot_valor: String,
}

fn extraer_plantilla_numerica(entrada_norm: &str, canonico: &str) -> Option<PlantillaExtraida> {
    let segs = clave_estructural_alias(entrada_norm);
    if segs.len() < 2 || es_patron_hora_ambigua(&segs) {
        return None;
    }
    let mut num_pos: Option<(usize, u32)> = None;
    let mut tiene_alpha = false;
    for (idx, s) in segs.iter().enumerate() {
        match s {
            SegmentoPatron::Num(n) => {
                if num_pos.is_some() {
                    return None; // Más de un número en entrada: ambiguo
                }
                num_pos = Some((idx, *n));
            }
            SegmentoPatron::Alpha(_) => tiene_alpha = true,
            SegmentoPatron::Otro(_) => return None,
        }
    }
    let (_, n_objetivo) = num_pos?;
    if !tiene_alpha {
        return None;
    }

    let spans = extraer_spans_palabras(canonico);
    let mut coincidencias: Vec<(usize, usize, &'static str)> = Vec::new();
    for sp in &spans {
        if sp.raw.chars().all(|c| c.is_ascii_digit()) {
            if sp.raw.len() < 6 {
                if let Ok(v) = sp.raw.parse::<u32>() {
                    if v == n_objetivo {
                        coincidencias.push((sp.start, sp.end, "{N}"));
                    }
                }
            }
        } else if let Some((ord_n, ph)) = parsear_ordinal_con_estilo(sp.raw) {
            if ord_n == n_objetivo {
                coincidencias.push((sp.start, sp.end, ph));
            }
        }
    }
    if coincidencias.len() != 1 {
        return None;
    }

    let (st, en, ph) = coincidencias[0];
    let patron_canonico = format!("{}{}{}", &canonico[..st], ph, &canonico[en..]);
    let patron_entrada = segs
        .iter()
        .map(|s| match s {
            SegmentoPatron::Alpha(a) => a.as_str(),
            SegmentoPatron::Num(_) => "{N}",
            SegmentoPatron::Otro(o) => o.as_str(),
        })
        .collect::<Vec<_>>()
        .join(" ");

    Some(PlantillaExtraida {
        tipo_slot: TipoSlotPatron::Numerico,
        patron_entrada,
        patron_canonico,
        slot_valor: n_objetivo.to_string(),
    })
}

fn es_boilerplate_extraccion(campo: &str, tok: &str) -> bool {
    if STOPWORDS_ALIAS.contains(&tok) {
        return true;
    }
    if campo == "tipo" {
        BOILERPLATE_TIPO_EXTRACCION.contains(&tok) || BLOQUEO_SLOT_TEXTO_TIPO.contains(&tok)
    } else {
        BOILERPLATE_JUZGADO_EXTRACCION.contains(&tok)
            || BLOQUEO_SLOT_TEXTO_JUZGADO.contains(&tok)
            || MESES_ES.contains(&tok)
    }
}

fn extraer_plantilla_texto(
    campo: &str,
    entrada_norm: &str,
    canonico: &str,
) -> Option<PlantillaExtraida> {
    let toks_e = tokenizar_para_alias(entrada_norm);
    if toks_e.len() < 2 {
        return None;
    }
    // No inducir patrón de texto si la entrada lleva dígitos (eso pertenece a patrones numéricos)
    if toks_e
        .iter()
        .any(|t| t.chars().any(|c| c.is_ascii_digit()))
    {
        return None;
    }

    let spans_c = extraer_spans_palabras(canonico);
    if spans_c.len() < 2 {
        return None;
    }

    // Buscar el tramo compartido más largo entre `toks_e` y `spans_c` (permitiendo conectores internos
    // como "de"/"la"/"del" dentro de nombres compuestos en el canónico, p. ej. "La Jagua de Ibirico").
    let mut mejor: Option<(usize, usize, usize, usize)> = None; // (e_start, e_end, c_start, c_end)

    for e_start in 0..toks_e.len() {
        let w_first = &toks_e[e_start];
        if w_first.len() < 4
            || !w_first.chars().all(|c| c.is_alphabetic())
            || es_boilerplate_extraccion(campo, w_first)
        {
            continue;
        }
        // Encontrar ocurrencias de `w_first` en `spans_c`
        let pos_c: Vec<usize> = spans_c
            .iter()
            .enumerate()
            .filter(|(_, sp)| sp.norm == *w_first)
            .map(|(i, _)| i)
            .collect();
        if pos_c.len() != 1 {
            continue;
        }
        let c_start_base = pos_c[0];
        let mut e_end = e_start + 1;
        let mut c_end = c_start_base + 1;

        while e_end < toks_e.len() && c_end < spans_c.len() {
            // Saltar hasta 2 stopwords internos en `spans_c` (ej. "jagua" [de] "ibirico")
            let mut c_probe = c_end;
            let mut saltos = 0;
            while c_probe < spans_c.len()
                && saltos < 2
                && STOPWORDS_ALIAS.contains(&spans_c[c_probe].norm.as_str())
            {
                c_probe += 1;
                saltos += 1;
            }
            if c_probe < spans_c.len()
                && spans_c[c_probe].norm == toks_e[e_end]
                && toks_e[e_end].chars().all(|c| c.is_alphabetic())
                && !es_boilerplate_extraccion(campo, &toks_e[e_end])
            {
                e_end += 1;
                c_end = c_probe + 1;
            } else {
                break;
            }
        }

        // Debe quedar al menos 1 token ancla fuera del tramo en `entrada`
        if e_start == 0 && e_end == toks_e.len() {
            continue;
        }
        // Si en `canonico` hay un artículo propio del nombre compuesto justo antes de `c_start_base`
        // (ej. "de La Jagua de Ibirico"), solo incluirlo si es "la"/"el"/"las"/"los" precedido de "de"
        let c_start = if c_start_base >= 2
            && matches!(spans_c[c_start_base - 1].norm.as_str(), "la" | "el" | "las" | "los")
            && matches!(spans_c[c_start_base - 2].norm.as_str(), "de" | "del")
            && spans_c[c_start_base - 1]
                .raw
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase())
        {
            c_start_base - 1
        } else {
            c_start_base
        };
        if c_start == 0 && c_end == spans_c.len() {
            continue;
        }

        let len_tramo = e_end - e_start;
        let mejora = match mejor {
            None => true,
            Some((ms, me, _, _)) => len_tramo > (me - ms),
        };
        if mejora {
            mejor = Some((e_start, e_end, c_start, c_end));
        }
    }

    let (e_start, e_end, c_start, c_end) = mejor?;

    // Verificar que exista al menos una palabra alfabética ancla fuera del slot
    let tiene_ancla = toks_e[..e_start]
        .iter()
        .chain(toks_e[e_end..].iter())
        .any(|t| t.chars().any(|c| c.is_alphabetic()));
    if !tiene_ancla {
        return None;
    }

    let byte_st = spans_c[c_start].start;
    let byte_en = spans_c[c_end - 1].end;
    let raw_slot = &canonico[byte_st..byte_en];
    let caja = detectar_estilo_caja_palabra(raw_slot);
    let ph = match caja {
        2 => "{X_UP}",
        0 => "{x}",
        _ => "{X_TITLE}",
    };

    let mut partes_entrada: Vec<&str> = Vec::new();
    for t in &toks_e[..e_start] {
        partes_entrada.push(t.as_str());
    }
    partes_entrada.push("{X}");
    for t in &toks_e[e_end..] {
        partes_entrada.push(t.as_str());
    }

    let patron_entrada = partes_entrada.join(" ");
    let patron_canonico = format!("{}{}{}", &canonico[..byte_st], ph, &canonico[byte_en..]);

    // Exigir expansión real (los restos fuera del slot deben diferir)
    let resto_e = normalizar_clave_entidad(&patron_entrada.replace("{X}", ""));
    let resto_c = normalizar_clave_entidad(&patron_canonico.replace(ph, ""));
    if resto_e == resto_c {
        return None;
    }

    let slot_valor = toks_e[e_start..e_end].join(" ");
    Some(PlantillaExtraida {
        tipo_slot: TipoSlotPatron::Texto,
        patron_entrada,
        patron_canonico,
        slot_valor,
    })
}

fn extraer_plantilla_de_alias(
    campo: &str,
    entrada_norm: &str,
    canonico: &str,
) -> Option<PlantillaExtraida> {
    if !matches!(campo, "juzgado" | "tipo") || canonico.trim().len() < 3 {
        return None;
    }
    if es_identidad_alias(entrada_norm, canonico) {
        return None;
    }
    extraer_plantilla_numerica(entrada_norm, canonico)
        .or_else(|| extraer_plantilla_texto(campo, entrada_norm, canonico))
}

// ---------------------------------------------------------------------------
// Inducción estadística de patrones consistentes por ámbito (defensor / "*")
// ---------------------------------------------------------------------------

struct InstanciaAgrupada {
    canon_norm: String,
    canon_display: String,
    evidencia_pos: f64,
    q10: f64,
    ultima_actualizacion: String,
}

#[derive(Default)]
struct GrupoPatron {
    tipo_slot: Option<TipoSlotPatron>,
    /// slot_valor -> lista de entradas activas (alpha > beta && q10 >= 0.22)
    activas_por_slot: BTreeMap<String, Vec<InstanciaAgrupada>>,
    /// (slot_valor, canon_norm) donde el usuario retractó explícitamente (beta > alpha)
    retractadas: BTreeSet<(String, String)>,
}

struct ResultadoAlcance {
    patrones: Vec<PatronAbreviacionInferido>,
    /// (campo, patron_entrada) -> conjunto de `canon_norm` activos observados en este alcance
    canonicos_observados: BTreeMap<(String, String), BTreeSet<String>>,
}

fn inducir_en_alcance(aliases: &[&AliasEstadistico], alcance_etiqueta: &str) -> ResultadoAlcance {
    let mut grupos: BTreeMap<(String, String), GrupoPatron> = BTreeMap::new();

    for a in aliases {
        let Some(pl) = extraer_plantilla_de_alias(&a.campo, &a.entrada_norm, &a.canonico) else {
            continue;
        };
        let canon_norm = normalizar_clave_entidad(&pl.patron_canonico);
        if canon_norm.is_empty() {
            continue;
        }
        let grupo = grupos
            .entry((a.campo.clone(), pl.patron_entrada.clone()))
            .or_default();
        if grupo.tipo_slot.is_none() {
            grupo.tipo_slot = Some(pl.tipo_slot);
        }

        let q10 = limite_inferior_bayesiano(a.alpha, a.beta);
        if a.alpha > a.beta && q10 >= UMBRAL_Q10_ALIAS_ACTIVO {
            grupo
                .activas_por_slot
                .entry(pl.slot_valor)
                .or_default()
                .push(InstanciaAgrupada {
                    canon_norm,
                    canon_display: pl.patron_canonico,
                    evidencia_pos: (a.alpha - PRIOR_ALIAS_ALPHA).clamp(0.5, 2.5),
                    q10,
                    ultima_actualizacion: a.ultima_actualizacion.clone(),
                });
        } else if a.beta > a.alpha {
            grupo.retractadas.insert((pl.slot_valor, canon_norm));
        }
    }

    let mut patrones = Vec::new();
    let mut canonicos_observados: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for ((campo, patron_entrada), grupo) in grupos {
        let Some(tipo_slot) = grupo.tipo_slot else {
            continue;
        };
        if grupo.activas_por_slot.is_empty() {
            continue;
        }

        let mut total_instancias = 0usize;
        // canon_norm -> (ejemplos_distintos, suma_evidencia_pos, mejor_display, mejor_fecha, mejor_q10)
        let mut votos: BTreeMap<String, (usize, f64, String, String, f64)> = BTreeMap::new();
        let obs_set = canonicos_observados
            .entry((campo.clone(), patron_entrada.clone()))
            .or_default();

        for (_slot_val, mut cands) in grupo.activas_por_slot {
            // Consolidar posibles duplicados del mismo canon_norm dentro del mismo slot
            let mut por_canon: BTreeMap<String, InstanciaAgrupada> = BTreeMap::new();
            for c in cands.drain(..) {
                obs_set.insert(c.canon_norm.clone());
                match por_canon.get_mut(&c.canon_norm) {
                    Some(prev) => {
                        if c.q10 > prev.q10 {
                            *prev = c;
                        }
                    }
                    None => {
                        por_canon.insert(c.canon_norm.clone(), c);
                    }
                }
            }
            total_instancias += 1;
            if por_canon.len() > 1 {
                // Este slot tiene dos canónicos activos contradictorios: cuenta como conflicto
                continue;
            }
            if let Some((_, inst)) = por_canon.into_iter().next() {
                let entry = votos.entry(inst.canon_norm).or_insert_with(|| {
                    (
                        0,
                        0.0,
                        inst.canon_display.clone(),
                        inst.ultima_actualizacion.clone(),
                        inst.q10,
                    )
                });
                entry.0 += 1;
                entry.1 += inst.evidencia_pos;
                if inst.q10 > entry.4
                    || ((inst.q10 - entry.4).abs() <= 1e-6
                        && inst.ultima_actualizacion > entry.3)
                {
                    entry.2 = inst.canon_display;
                    entry.3 = inst.ultima_actualizacion;
                    entry.4 = inst.q10;
                }
            }
        }

        let Some((top_canon_norm, (ejemplos_distintos, suma_pos, display, _, _))) =
            votos.into_iter().max_by(|a, b| {
                a.1 .0
                    .cmp(&b.1 .0)
                    .then_with(|| a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal))
            })
        else {
            continue;
        };

        if ejemplos_distintos < 2 || total_instancias < 2 {
            continue;
        }
        let consistencia = ejemplos_distintos as f64 / total_instancias as f64;
        let umbral_cons = match tipo_slot {
            TipoSlotPatron::Numerico => UMBRAL_CONSISTENCIA_NUM,
            TipoSlotPatron::Texto => UMBRAL_CONSISTENCIA_TEXTO,
        };
        if consistencia + 1e-9 < umbral_cons {
            continue;
        }

        let conflictos = total_instancias.saturating_sub(ejemplos_distintos) as f64;
        let retracciones_top = grupo
            .retractadas
            .iter()
            .filter(|(_, cn)| cn == &top_canon_norm)
            .count() as f64;

        let alpha_pat = PRIOR_ALIAS_ALPHA + suma_pos;
        let beta_pat = PRIOR_ALIAS_BETA + conflictos * 1.5 + retracciones_top;
        let confianza_q10 = limite_inferior_bayesiano(alpha_pat, beta_pat);
        if confianza_q10 < UMBRAL_Q10_PATRON {
            continue;
        }

        patrones.push(PatronAbreviacionInferido {
            campo,
            tipo_slot,
            patron_entrada,
            patron_canonico: display,
            defensor_alcance: alcance_etiqueta.to_string(),
            ejemplos_distintos,
            total_instancias,
            consistencia,
            confianza_q10,
            origen: "alias".to_string(),
        });
    }

    ResultadoAlcance {
        patrones,
        canonicos_observados,
    }
}

/// Respaldo en arranque en frío desde `priors.grafo.defensor_sigla_a_juzgado` únicamente cuando
/// el defensor no tiene evidencia en `tabla_alias` para `j {N}` y el 100% de sus despachos
/// numerados en Excel (`>= 3` números distintos, 0 conflictos) siguen una sola plantilla.
fn inducir_patron_respaldo_grafo(
    priors: &IndicePriorsExcel,
    def_clave: &str,
) -> Option<PatronAbreviacionInferido> {
    if def_clave.is_empty() {
        return None;
    }
    let siglas_map = priors.grafo.defensor_sigla_a_juzgado.get(def_clave)?;
    let mut plantillas_distintas: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut total_siglas = 0usize;

    for (sigla, destinos) in siglas_map {
        let activos: Vec<(&String, u32)> = destinos
            .iter()
            .filter(|(_, &f)| f > 0)
            .map(|(j, &f)| (j, f))
            .collect();
        if activos.is_empty() {
            continue;
        }
        total_siglas += 1;
        if activos.len() > 1 {
            // Un mismo número mapea a más de un juzgado distinto en Excel: no generalizar
            return None;
        }
        let juz = activos[0].0;
        let pl = extraer_plantilla_numerica(sigla, juz)?;
        if pl.patron_entrada != "j {N}" {
            return None;
        }
        let cn = normalizar_clave_entidad(&pl.patron_canonico);
        let e = plantillas_distintas
            .entry(cn)
            .or_insert_with(|| (0, pl.patron_canonico.clone()));
        e.0 += 1;
    }

    if total_siglas < 3 || plantillas_distintas.len() != 1 {
        return None;
    }
    let (_, (ejemplos, display)) = plantillas_distintas.into_iter().next()?;
    let q10 = limite_inferior_bayesiano(
        PRIOR_ALIAS_ALPHA + (ejemplos.min(8) as f64),
        PRIOR_ALIAS_BETA,
    );
    Some(PatronAbreviacionInferido {
        campo: "juzgado".to_string(),
        tipo_slot: TipoSlotPatron::Numerico,
        patron_entrada: "j {N}".to_string(),
        patron_canonico: display,
        defensor_alcance: def_clave.to_string(),
        ejemplos_distintos: ejemplos,
        total_instancias: total_siglas,
        consistencia: 1.0,
        confianza_q10: q10,
        origen: "grafo".to_string(),
    })
}

fn combinar_alias_para_patrones<'a>(
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> Vec<&'a AliasEstadistico> {
    let mut vistos: BTreeSet<(String, Vec<SegmentoPatron>, String, String)> = BTreeSet::new();
    let mut out = Vec::with_capacity(local.tabla_alias.len() + base.tabla_alias.len());
    let clave_dedup = |a: &AliasEstadistico| {
        let alc = if es_alcance_general(&a.defensor_alcance) {
            "*".to_string()
        } else {
            normalizar_clave_defensor(&a.defensor_alcance)
        };
        (
            a.campo.clone(),
            clave_estructural_alias(&a.entrada_norm),
            normalizar_clave_entidad(&a.canonico),
            alc,
        )
    };
    for a in &local.tabla_alias {
        vistos.insert(clave_dedup(a));
        out.push(a);
    }
    for a in &base.tabla_alias {
        if vistos.insert(clave_dedup(a)) {
            out.push(a);
        }
    }
    out
}

/// Induce todos los patrones consistentes aplicables al defensor activo (priorizando su cuaderno
/// privado y heredando del Cuaderno General `"*"` únicamente cuando el defensor no contradice
/// ese patrón).
pub fn inducir_patrones_abreviacion_para_defensor(
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    priors: Option<&IndicePriorsExcel>,
    defensor: Option<&str>,
) -> Vec<PatronAbreviacionInferido> {
    let combinados = combinar_alias_para_patrones(base, local);
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();

    let mut aliases_def: Vec<&AliasEstadistico> = Vec::new();
    let mut aliases_gen: Vec<&AliasEstadistico> = Vec::new();

    for a in &combinados {
        let es_gen = es_alcance_general(&a.defensor_alcance);
        if es_gen {
            aliases_gen.push(*a);
            if !def_clave.is_empty()
                && a.defensores_confirmantes
                    .iter()
                    .any(|d| normalizar_clave_defensor(d) == def_clave)
            {
                aliases_def.push(*a);
            }
        } else if !def_clave.is_empty()
            && coincide_alcance_defensor(&a.defensor_alcance, &def_clave)
        {
            aliases_def.push(*a);
        }
    }

    let res_def = if def_clave.is_empty() {
        ResultadoAlcance {
            patrones: Vec::new(),
            canonicos_observados: BTreeMap::new(),
        }
    } else {
        inducir_en_alcance(&aliases_def, &def_clave)
    };
    let res_gen = inducir_en_alcance(&aliases_gen, "*");

    let mut finales: Vec<PatronAbreviacionInferido> = Vec::new();
    let mut claves_presentes: BTreeSet<(String, String)> = BTreeSet::new();

    for p in res_def.patrones {
        claves_presentes.insert((p.campo.clone(), p.patron_entrada.clone()));
        finales.push(p);
    }

    for p in res_gen.patrones {
        let key = (p.campo.clone(), p.patron_entrada.clone());
        if claves_presentes.contains(&key) {
            continue;
        }
        // Si el defensor activo ya registró ejemplos contradictorios para este patrón, bloquear "*"
        if let Some(obs_def) = res_def.canonicos_observados.get(&key) {
            let gen_cn = normalizar_clave_entidad(&p.patron_canonico);
            if obs_def.iter().any(|c| c != &gen_cn) {
                continue;
            }
        }
        claves_presentes.insert(key);
        finales.push(p);
    }

    // Respaldo frío en Excel para `j {N}` si no existe ninguna evidencia en tabla_alias
    let key_j = ("juzgado".to_string(), "j {N}".to_string());
    if !def_clave.is_empty()
        && !claves_presentes.contains(&key_j)
        && !res_def.canonicos_observados.contains_key(&key_j)
    {
        if let Some(pr) = priors {
            if let Some(pat_grafo) = inducir_patron_respaldo_grafo(pr, &def_clave) {
                finales.push(pat_grafo);
            }
        }
    }

    ordenar_patrones(&mut finales, &def_clave);
    finales
}

fn ordenar_patrones(patrones: &mut [PatronAbreviacionInferido], def_clave: &str) {
    patrones.sort_by(|a, b| {
        let a_def = !def_clave.is_empty() && a.defensor_alcance == def_clave;
        let b_def = !def_clave.is_empty() && b.defensor_alcance == def_clave;
        let a_segs = a.patron_entrada.split_whitespace().count();
        let b_segs = b.patron_entrada.split_whitespace().count();
        let a_num = matches!(a.tipo_slot, TipoSlotPatron::Numerico);
        let b_num = matches!(b.tipo_slot, TipoSlotPatron::Numerico);

        b_def
            .cmp(&a_def)
            .then_with(|| b_segs.cmp(&a_segs))
            .then_with(|| b_num.cmp(&a_num))
            .then_with(|| {
                b.consistencia
                    .partial_cmp(&a.consistencia)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b.confianza_q10
                    .partial_cmp(&a.confianza_q10)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| b.ejemplos_distintos.cmp(&a.ejemplos_distintos))
            .then_with(|| a.patron_entrada.cmp(&b.patron_entrada))
    });
}

/// Induce los patrones consistentes de todos los ámbitos presentes en `memoria` (Cuaderno General
/// `"*"` y cada defensor registrado en `tabla_alias`) para diagnóstico y exportación anonimizada.
pub fn inducir_patrones_todos_ambitos(memoria: &MemoriaAprendizaje) -> Vec<PatronAbreviacionInferido> {
    let mut ambitos: BTreeSet<String> = BTreeSet::new();
    ambitos.insert("*".to_string());
    for a in &memoria.tabla_alias {
        if !es_alcance_general(&a.defensor_alcance) {
            let dk = normalizar_clave_defensor(&a.defensor_alcance);
            if !dk.is_empty() {
                ambitos.insert(dk);
            }
        }
        for d in &a.defensores_confirmantes {
            let dk = normalizar_clave_defensor(d);
            if !dk.is_empty() {
                ambitos.insert(dk);
            }
        }
    }

    let mut out: Vec<PatronAbreviacionInferido> = Vec::new();
    for amb in ambitos {
        let aliases: Vec<&AliasEstadistico> = if amb == "*" {
            memoria
                .tabla_alias
                .iter()
                .filter(|a| es_alcance_general(&a.defensor_alcance))
                .collect()
        } else {
            memoria
                .tabla_alias
                .iter()
                .filter(|a| {
                    coincide_alcance_defensor(&a.defensor_alcance, &amb)
                        || (es_alcance_general(&a.defensor_alcance)
                            && a.defensores_confirmantes
                                .iter()
                                .any(|d| normalizar_clave_defensor(d) == amb))
                })
                .collect()
        };
        let res = inducir_en_alcance(&aliases, &amb);
        out.extend(res.patrones);
    }

    out.sort_by(|a, b| {
        b.confianza_q10
            .partial_cmp(&a.confianza_q10)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.ejemplos_distintos.cmp(&a.ejemplos_distintos))
            .then_with(|| a.defensor_alcance.cmp(&b.defensor_alcance))
            .then_with(|| a.patron_entrada.cmp(&b.patron_entrada))
    });
    out
}

// ---------------------------------------------------------------------------
// Escáner seguro de texto libre (cláusulas, barreras fecha/hora/radicado)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TokenTexto {
    raw: String,
    norm: String,
    clause_id: usize,
    es_barrera: bool,
    es_stopword: bool,
    es_mayus_inicial: bool,
    segmentos: Vec<SegmentoPatron>,
}

fn es_token_fecha_o_hora_compuesto(raw: &str) -> bool {
    // 15/08/2025, 5/8/25, 15-08-2025, 09:30, 2:30pm, 14.30
    let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric());
    if trimmed.contains(':') {
        let partes: Vec<&str> = trimmed.split(':').collect();
        if partes.len() >= 2
            && !partes[0].is_empty()
            && partes[0].chars().all(|c| c.is_ascii_digit())
            && partes[1].chars().take(2).all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    if trimmed.contains('/') {
        let partes: Vec<&str> = trimmed.split('/').collect();
        if partes.len() >= 2
            && partes
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    }
    if trimmed.contains('-') {
        let partes: Vec<&str> = trimmed.split('-').collect();
        if partes.len() == 3
            && partes
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    }
    if trimmed.contains('.') {
        let partes: Vec<&str> = trimmed.split('.').collect();
        if partes.len() == 2
            && (1..=2).contains(&partes[0].len())
            && partes[1].len() == 2
            && partes[0].chars().all(|c| c.is_ascii_digit())
            && partes[1].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

fn tokenizar_texto_con_contexto(texto: &str) -> (Vec<TokenTexto>, bool) {
    let colapsado = colapsar_siglas_con_puntos(texto);
    let tiene_mayusculas_mixtas =
        colapsado.chars().any(|c| c.is_lowercase()) && colapsado.chars().any(|c| c.is_uppercase());

    let chars: Vec<char> = colapsado.chars().collect();
    let n = chars.len();
    let mut tokens: Vec<TokenTexto> = Vec::new();
    let mut clause_id = 0usize;
    let mut i = 0usize;

    while i < n {
        let ch = chars[i];
        if ch.is_whitespace() {
            if ch == '\n' || ch == '\r' {
                clause_id += 1;
            }
            i += 1;
            continue;
        }
        if matches!(ch, ',' | ';' | ':' | '(' | ')' | '[' | ']' | '—' | '–') {
            clause_id += 1;
            i += 1;
            continue;
        }
        if ch == '.' {
            // Punto seguido de espacio o fin de texto cierra cláusula salvo abreviatura corta tipo "Jdo."
            let prev_corto = tokens
                .last()
                .is_some_and(|t| t.clause_id == clause_id && t.norm.len() <= 3 && t.norm.chars().all(|c| c.is_alphabetic()));
            if !prev_corto {
                clause_id += 1;
            }
            i += 1;
            continue;
        }
        if ch == '-' {
            // " - " con espacios es separador de cláusula; "J-4" o "JPM-04" es interno
            let sep_espacio = (i > 0 && chars[i - 1].is_whitespace())
                || (i + 1 < n && chars[i + 1].is_whitespace());
            if sep_espacio {
                clause_id += 1;
            }
            i += 1;
            continue;
        }
        if !es_char_palabra(ch) {
            i += 1;
            continue;
        }

        // Capturar token compuesto para detectar fechas/horas/radicados (ej. 15/08/2025, 09:30, 20001-60-...)
        let start = i;
        while i < n
            && (es_char_palabra(chars[i])
                || (matches!(chars[i], '/' | ':' | '.' | '-')
                    && i + 1 < n
                    && chars[i + 1].is_ascii_digit()
                    && chars[start].is_ascii_digit()))
        {
            i += 1;
        }
        let raw_comp: String = chars[start..i].iter().collect();
        // Omitir símbolo ordinal pegado `º`/`ª`/`°` sin romper cláusula
        while i < n && es_simbolo_ordinal(chars[i]) {
            i += 1;
        }

        let digitos_total = raw_comp.chars().filter(|c| c.is_ascii_digit()).count();
        let es_fecha_hora = es_token_fecha_o_hora_compuesto(&raw_comp);
        let es_radicado = digitos_total >= 6 && raw_comp.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '.' | '/'));

        if es_fecha_hora || es_radicado {
            tokens.push(TokenTexto {
                raw: raw_comp,
                norm: String::new(),
                clause_id,
                es_barrera: true,
                es_stopword: false,
                es_mayus_inicial: false,
                segmentos: Vec::new(),
            });
            continue;
        }

        let norm_lower = quitar_tildes(&raw_comp.to_lowercase());
        let norm_ord = normalizar_ordinal_token_alias(&norm_lower)
            .map(str::to_string)
            .unwrap_or_else(|| normalizar_digitos_internos(&norm_lower));
        let es_stopword = STOPWORDS_ALIAS.contains(&norm_ord.as_str());
        let es_mayus_inicial = raw_comp.chars().next().is_some_and(|c| c.is_uppercase());
        let segs = if es_stopword {
            Vec::new()
        } else {
            segmentar_tokens_alias(std::slice::from_ref(&norm_ord))
        };
        let es_barrera = es_patron_hora_ambigua(&segs)
            || (norm_ord.chars().all(|c| c.is_ascii_digit())
                && norm_ord
                    .parse::<u32>()
                    .is_ok_and(|v| v == 0 || v > MAX_SLOT_NUM));

        tokens.push(TokenTexto {
            raw: raw_comp,
            norm: norm_ord,
            clause_id,
            es_barrera,
            es_stopword,
            es_mayus_inicial,
            segmentos: segs,
        });
    }

    // Marcar números que pertenecen a fechas en texto ("15 de agosto", "agosto 15") u horas ("4 pm", "a las 4")
    for idx in 0..tokens.len() {
        if tokens[idx].es_barrera {
            continue;
        }
        let solo_num = matches!(tokens[idx].segmentos.as_slice(), [SegmentoPatron::Num(_)]);
        if !solo_num {
            continue;
        }
        let cid = tokens[idx].clause_id;
        let val = match tokens[idx].segmentos[0] {
            SegmentoPatron::Num(v) => v,
            _ => 0,
        };

        // "4 pm" / "4 am" / "4 hrs" / "4 min"
        if let Some(nxt) = tokens.get(idx + 1) {
            if nxt.clause_id == cid && SUFIJOS_TIEMPO.contains(&nxt.norm.as_str()) {
                tokens[idx].es_barrera = true;
                tokens[idx + 1].es_barrera = true;
                continue;
            }
        }
        // "15 de agosto" / "15 agosto"
        if val <= 31 {
            let sig_es_mes = tokens.get(idx + 1).is_some_and(|t| {
                t.clause_id == cid && MESES_ES.contains(&t.norm.as_str())
            }) || (tokens
                .get(idx + 1)
                .is_some_and(|t| t.clause_id == cid && t.norm == "de")
                && tokens
                    .get(idx + 2)
                    .is_some_and(|t| t.clause_id == cid && MESES_ES.contains(&t.norm.as_str())));
            let prev_es_mes = idx > 0
                && tokens[idx - 1].clause_id == cid
                && MESES_ES.contains(&tokens[idx - 1].norm.as_str());
            if sig_es_mes || prev_es_mes {
                tokens[idx].es_barrera = true;
                continue;
            }
        }
        // "a las 4" / "hora 4"
        if idx > 0
            && tokens[idx - 1].clause_id == cid
            && matches!(tokens[idx - 1].norm.as_str(), "las" | "hora" | "horas")
        {
            tokens[idx].es_barrera = true;
        }
    }

    (tokens, tiene_mayusculas_mixtas)
}

pub(crate) fn es_calificador_juzgado(tok: &str) -> bool {
    CALIFICADORES_JUZGADO.contains(&tok)
}

pub(crate) fn es_calificador_terminal_juzgado(tok: &str) -> bool {
    CALIFICADORES_TERMINALES_JUZGADO.contains(&tok)
}

fn termina_en_calificador_terminal(segs: &[SegmentoPatron]) -> bool {
    matches!(segs.last(), Some(SegmentoPatron::Alpha(a)) if es_calificador_terminal_juzgado(a))
}

/// Avanza opcionalmente sobre un conector `"de"` / `"del"` en la misma cláusula y devuelve
/// el índice del siguiente token si es un calificador de despacho (p. ej. `"J4 CTO"`,
/// `"J4 del Circuito"`, `"J4 de Garantías"`, `"J4 Penal del Circuito"`).
fn indice_siguiente_calificador(
    tokens: &[TokenTexto],
    end_excl: usize,
    clause_id: usize,
) -> Option<usize> {
    let mut probe = end_excl;
    if let Some(t) = tokens.get(probe) {
        if t.clause_id == clause_id && matches!(t.norm.as_str(), "de" | "del") {
            probe += 1;
        }
    }
    let nxt = tokens.get(probe)?;
    if nxt.clause_id != clause_id || nxt.es_barrera || nxt.es_stopword {
        return None;
    }
    if es_calificador_juzgado(&nxt.norm)
        || matches!(nxt.segmentos.first(), Some(SegmentoPatron::Alpha(a)) if es_calificador_juzgado(a))
    {
        Some(probe)
    } else {
        None
    }
}

fn siguiente_token_es_calificador(tokens: &[TokenTexto], end_excl: usize, clause_id: usize) -> bool {
    indice_siguiente_calificador(tokens, end_excl, clause_id).is_some()
}

/// Intenta hacer coincidir una secuencia de segmentos `objetivo` empezando exactamente en `start_idx`
/// dentro de la misma cláusula (saltando stopwords neutros entre tokens, sin cruzar barreras).
/// Devuelve `Some(end_excl)` si la coincidencia consume tokens completos al inicio y al final.
fn coincidir_segmentos_en_texto(
    tokens: &[TokenTexto],
    start_idx: usize,
    objetivo: &[SegmentoPatron],
) -> Option<usize> {
    if objetivo.is_empty() || start_idx >= tokens.len() {
        return None;
    }
    let cid = tokens[start_idx].clause_id;
    if tokens[start_idx].es_barrera || tokens[start_idx].es_stopword {
        return None;
    }

    let mut ti = start_idx;
    let mut oi = 0usize;
    while oi < objetivo.len() {
        while ti < tokens.len() && tokens[ti].clause_id == cid && tokens[ti].es_stopword && ti > start_idx {
            ti += 1;
        }
        if ti >= tokens.len() || tokens[ti].clause_id != cid || tokens[ti].es_barrera {
            return None;
        }
        let tsegs = &tokens[ti].segmentos;
        if tsegs.is_empty() || oi + tsegs.len() > objetivo.len() {
            return None;
        }
        if &objetivo[oi..oi + tsegs.len()] != tsegs.as_slice() {
            return None;
        }
        oi += tsegs.len();
        ti += 1;
    }
    Some(ti)
}

#[derive(Debug, Clone)]
enum PartePatronNum {
    Alpha(String),
    SlotNum,
}

fn parsear_patron_numerico(patron_entrada: &str) -> Option<Vec<PartePatronNum>> {
    let mut out = Vec::new();
    let mut slots = 0usize;
    for tok in patron_entrada.split_whitespace() {
        if tok == "{N}" {
            slots += 1;
            out.push(PartePatronNum::SlotNum);
        } else {
            out.push(PartePatronNum::Alpha(tok.to_string()));
        }
    }
    if slots == 1 && !out.is_empty() {
        Some(out)
    } else {
        None
    }
}

fn coincidir_patron_numerico_en_texto(
    tokens: &[TokenTexto],
    start_idx: usize,
    partes: &[PartePatronNum],
) -> Option<(usize, u32)> {
    if partes.is_empty() || start_idx >= tokens.len() {
        return None;
    }
    let cid = tokens[start_idx].clause_id;
    if tokens[start_idx].es_barrera || tokens[start_idx].es_stopword {
        return None;
    }

    let mut ti = start_idx;
    let mut pi = 0usize;
    let mut valor_n: Option<u32> = None;

    while pi < partes.len() {
        while ti < tokens.len() && tokens[ti].clause_id == cid && tokens[ti].es_stopword && ti > start_idx {
            ti += 1;
        }
        if ti >= tokens.len() || tokens[ti].clause_id != cid || tokens[ti].es_barrera {
            return None;
        }
        let tsegs = &tokens[ti].segmentos;
        if tsegs.is_empty() || pi + tsegs.len() > partes.len() {
            return None;
        }
        for (offset, seg) in tsegs.iter().enumerate() {
            match (&partes[pi + offset], seg) {
                (PartePatronNum::Alpha(a), SegmentoPatron::Alpha(b)) if a == b => {}
                (PartePatronNum::SlotNum, SegmentoPatron::Num(n)) => {
                    valor_n = Some(*n);
                }
                _ => return None,
            }
        }
        pi += tsegs.len();
        ti += 1;
    }

    valor_n.map(|n| (ti, n))
}

fn instanciar_patron_numerico(patron_canonico: &str, n: u32) -> Option<String> {
    for ph in [
        "{N}",
        "{ORD_M}",
        "{ord_m}",
        "{ORD_M_UP}",
        "{ORD_F}",
        "{ord_f}",
        "{ORD_F_UP}",
        "{ORD_ABREV_M}",
        "{ORD_ABREV_F}",
    ] {
        if patron_canonico.contains(ph) {
            let reemplazo = formatear_slot_numerico(n, ph)?;
            return Some(patron_canonico.replacen(ph, &reemplazo, 1));
        }
    }
    None
}

fn instanciar_patron_texto(patron_canonico: &str, slot_raw: &str) -> Option<String> {
    for ph in ["{X_TITLE}", "{x}", "{X_UP}"] {
        if patron_canonico.contains(ph) {
            let reemplazo = aplicar_estilo_slot_texto(slot_raw, ph);
            return Some(patron_canonico.replacen(ph, &reemplazo, 1));
        }
    }
    None
}

fn existe_bloqueo_concreto_en_alias(
    combinados: &[&AliasEstadistico],
    campo: &str,
    entrada_concreta: &str,
    expansion_sugerida: &str,
    def_clave: &str,
) -> bool {
    let segs_obj = clave_estructural_alias(entrada_concreta);
    if segs_obj.is_empty() {
        return false;
    }
    let exp_norm = normalizar_clave_entidad(expansion_sugerida);

    for a in combinados {
        if a.campo != campo {
            continue;
        }
        let es_gen = es_alcance_general(&a.defensor_alcance);
        let es_def = !def_clave.is_empty() && coincide_alcance_defensor(&a.defensor_alcance, def_clave);
        if !es_gen && !es_def {
            continue;
        }
        if clave_estructural_alias(&a.entrada_norm) != segs_obj {
            continue;
        }
        let q10 = limite_inferior_bayesiano(a.alpha, a.beta);
        // 1) Cualquier alias específico activo (incluso tras 1 sola corrección, q10 >= 0.22)
        //    bloquea que el patrón general proponga otra cosa para este valor concreto.
        if a.alpha > a.beta && q10 >= UMBRAL_Q10_ALIAS_ACTIVO {
            return true;
        }
        // 2) Si el defensor retractó explícitamente esta expansión exacta para este valor concreto,
        //    no volver a sugerirla por patrón.
        if a.beta > a.alpha && normalizar_clave_entidad(&a.canonico) == exp_norm {
            return true;
        }
    }
    false
}

fn construir_mapa_catalogo_juzgados(
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
) -> BTreeMap<String, String> {
    let mut mapa = BTreeMap::new();
    for j in juzgados_base() {
        let k = normalizar_clave_catalogo(j);
        if !k.is_empty() {
            mapa.entry(k).or_insert_with(|| j.clone());
        }
    }
    for j in priors.juzgados_freq.keys() {
        let limpio = colapsar_espacios(j);
        let k = normalizar_clave_catalogo(&limpio);
        if !k.is_empty() {
            mapa.insert(k, limpio);
        }
    }
    for j in &settings.juzgados {
        let limpio = colapsar_espacios(j);
        let k = normalizar_clave_catalogo(&limpio);
        if !k.is_empty() {
            mapa.insert(k, limpio);
        }
    }
    mapa
}

fn construir_mapa_catalogo_tipos(
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
) -> BTreeMap<String, String> {
    let mut mapa = BTreeMap::new();
    for &t in &TIPOS_AUDIENCIA {
        if t == "Otra" {
            continue;
        }
        let k = normalizar_clave_catalogo(t);
        if !k.is_empty() {
            mapa.insert(k, t.to_string());
        }
    }
    for t in priors.tipos_freq.keys() {
        let limpio = colapsar_espacios(t);
        let k = normalizar_clave_catalogo(&limpio);
        if !k.is_empty() && k != "otra" {
            mapa.entry(k).or_insert(limpio);
        }
    }
    for t in &settings.tipos_audiencia {
        let limpio = colapsar_espacios(t);
        let k = normalizar_clave_catalogo(&limpio);
        if !k.is_empty() && k != "otra" {
            mapa.insert(k, limpio);
        }
    }
    mapa
}

fn es_palabra_bloqueada_slot_texto(campo: &str, norm: &str) -> bool {
    if STOPWORDS_ALIAS.contains(&norm) || MESES_ES.contains(&norm) {
        return true;
    }
    if campo == "tipo" {
        BLOQUEO_SLOT_TEXTO_TIPO.contains(&norm) || BOILERPLATE_TIPO_EXTRACCION.contains(&norm)
    } else {
        BLOQUEO_SLOT_TEXTO_JUZGADO.contains(&norm) || CALIFICADORES_JUZGADO.contains(&norm)
    }
}

#[derive(Debug, Clone)]
enum CandidatoSpan {
    Exacto {
        campo: String,
        start: usize,
        end: usize,
        segs_len: usize,
        es_defensor: bool,
        q10: f64,
        abrev_norm: String,
        canonico: String,
    },
    Inferido {
        campo: String,
        start: usize,
        end: usize,
        segs_len: usize,
        es_defensor: bool,
        q10: f64,
        res: ResolucionPatronInferido,
    },
}

impl CandidatoSpan {
    fn campo(&self) -> &str {
        match self {
            Self::Exacto { campo, .. } | Self::Inferido { campo, .. } => campo,
        }
    }
    fn span(&self) -> (usize, usize) {
        match self {
            Self::Exacto { start, end, .. } | Self::Inferido { start, end, .. } => (*start, *end),
        }
    }
    fn clave_orden(&self) -> (usize, usize, u8, u8, u32) {
        match self {
            Self::Exacto {
                start,
                end,
                segs_len,
                es_defensor,
                q10,
                ..
            } => (
                end.saturating_sub(*start),
                *segs_len,
                1, // Exacto siempre gana sobre inferido en igual tramo
                if *es_defensor { 1 } else { 0 },
                (*q10 * 10_000.0).round() as u32,
            ),
            Self::Inferido {
                start,
                end,
                segs_len,
                es_defensor,
                q10,
                ..
            } => (
                end.saturating_sub(*start),
                *segs_len,
                0,
                if *es_defensor { 1 } else { 0 },
                (*q10 * 10_000.0).round() as u32,
            ),
        }
    }
}

/// Resuelve tanto abreviaciones exactas validadas (`q10 >= 0.30`) como patrones parametrizados
/// inferidos en `texto` (sin limitarse a `J#`), garantizando que:
/// 1. El tramo más largo no solapado gana (`"JPM 4"` vence a `"JPM"`, `"J4 C"` vence a `"J4"`).
/// 2. En igual tramo, un alias exacto siempre vence a un patrón inferido.
/// 3. Calificadores explícitos (`CTO`, `Circuito`, `Conocimiento`, `MPAL`, etc.) nunca son pisados.
pub fn resolver_abreviaciones_y_patrones_en_texto(
    texto: &str,
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
    defensor: Option<&str>,
) -> ResolucionAbreviacionesTexto {
    let (tokens, tiene_mayus_mixtas) = tokenizar_texto_con_contexto(texto);
    if tokens.is_empty() {
        return ResolucionAbreviacionesTexto::default();
    }

    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let combinados = combinar_alias_para_patrones(base, local);
    let patrones = inducir_patrones_abreviacion_para_defensor(base, local, Some(priors), defensor);

    let cat_juz = construir_mapa_catalogo_juzgados(settings, priors);
    let cat_tipo = construir_mapa_catalogo_tipos(settings, priors);

    let mut candidatos: Vec<CandidatoSpan> = Vec::new();

    // 1. Coincidencias de alias exactos validados (q10 >= 0.30, no-identidad)
    for a in &combinados {
        if !matches!(a.campo.as_str(), "juzgado" | "tipo") {
            continue;
        }
        if es_identidad_alias(&a.entrada_norm, &a.canonico) {
            continue;
        }
        let es_gen = es_alcance_general(&a.defensor_alcance);
        let es_def = !def_clave.is_empty() && coincide_alcance_defensor(&a.defensor_alcance, &def_clave);
        if !es_gen && !es_def {
            continue;
        }
        let q10 = limite_inferior_bayesiano(a.alpha, a.beta);
        if q10 < UMBRAL_Q10_ALIAS_EXACTO {
            continue;
        }
        let segs_alias = clave_estructural_alias(&a.entrada_norm);
        if segs_alias.is_empty() || es_patron_hora_ambigua(&segs_alias) {
            continue;
        }
        let abrev_norm = canonizar_entrada_alias_general(&a.entrada_norm);
        let ya_calificado = a.campo == "juzgado" && termina_en_calificador_terminal(&segs_alias);

        for start in 0..tokens.len() {
            if let Some(end) = coincidir_segmentos_en_texto(&tokens, start, &segs_alias) {
                let cid = tokens[start].clause_id;
                if a.campo == "juzgado"
                    && !ya_calificado
                    && siguiente_token_es_calificador(&tokens, end, cid)
                {
                    continue;
                }
                candidatos.push(CandidatoSpan::Exacto {
                    campo: a.campo.clone(),
                    start,
                    end,
                    segs_len: segs_alias.len(),
                    es_defensor: es_def,
                    q10,
                    abrev_norm: abrev_norm.clone(),
                    canonico: a.canonico.clone(),
                });
            }
        }
    }

    // 2. Coincidencias de patrones parametrizados inferidos ({N} y {X})
    for pat in &patrones {
        let es_def = !def_clave.is_empty() && pat.defensor_alcance == def_clave;
        match pat.tipo_slot {
            TipoSlotPatron::Numerico => {
                let Some(partes) = parsear_patron_numerico(&pat.patron_entrada) else {
                    continue;
                };
                let ya_calificado = pat.campo == "juzgado"
                    && matches!(partes.last(), Some(PartePatronNum::Alpha(a)) if es_calificador_terminal_juzgado(a));
                let empieza_con_num = matches!(partes.first(), Some(PartePatronNum::SlotNum));

                for start in 0..tokens.len() {
                    let Some((end, n_val)) =
                        coincidir_patron_numerico_en_texto(&tokens, start, &partes)
                    else {
                        continue;
                    };
                    let cid = tokens[start].clause_id;
                    if pat.campo == "juzgado"
                        && !ya_calificado
                        && siguiente_token_es_calificador(&tokens, end, cid)
                    {
                        continue;
                    }
                    if empieza_con_num && start > 0 && tokens[start - 1].clause_id == cid {
                        let prev = tokens[start - 1].norm.as_str();
                        if PREFIJOS_NO_JUZGADO_NUMERICOS.contains(&prev)
                            || matches!(prev, "dia" | "dias" | "mes" | "hora" | "horas")
                        {
                            continue;
                        }
                    }

                    let Some(expansion) = instanciar_patron_numerico(&pat.patron_canonico, n_val)
                    else {
                        continue;
                    };
                    let raw_span = tokens[start..end]
                        .iter()
                        .map(|t| t.raw.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let concrete_key = canonizar_entrada_alias_general(&raw_span);
                    if existe_bloqueo_concreto_en_alias(
                        &combinados,
                        &pat.campo,
                        &concrete_key,
                        &expansion,
                        &def_clave,
                    ) {
                        continue;
                    }

                    // Normalizar hacia la forma exacta del catálogo si existe coincidencia sin tildes
                    let expansion_final = if pat.campo == "juzgado" {
                        cat_juz
                            .get(&normalizar_clave_catalogo(&expansion))
                            .cloned()
                            .unwrap_or(expansion)
                    } else {
                        cat_tipo
                            .get(&normalizar_clave_catalogo(&expansion))
                            .cloned()
                            .unwrap_or(expansion)
                    };

                    candidatos.push(CandidatoSpan::Inferido {
                        campo: pat.campo.clone(),
                        start,
                        end,
                        segs_len: partes.len(),
                        es_defensor: es_def,
                        q10: pat.confianza_q10,
                        res: ResolucionPatronInferido {
                            campo: pat.campo.clone(),
                            abreviacion_detectada: raw_span,
                            expansion_sugerida: expansion_final,
                            patron_entrada: pat.patron_entrada.clone(),
                            ejemplos_distintos: pat.ejemplos_distintos,
                            total_instancias: pat.total_instancias,
                            consistencia: pat.consistencia,
                            confianza_q10: pat.confianza_q10,
                        },
                    });
                }
            }
            TipoSlotPatron::Texto => {
                let toks_pat: Vec<&str> = pat.patron_entrada.split_whitespace().collect();
                let Some(pos_x) = toks_pat.iter().position(|&t| t == "{X}") else {
                    continue;
                };
                let pref_toks: Vec<String> = toks_pat[..pos_x]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect();
                let suf_toks: Vec<String> = toks_pat[pos_x + 1..]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect();
                let pref_segs = segmentar_tokens_alias(&pref_toks);
                let suf_segs = segmentar_tokens_alias(&suf_toks);
                if pref_segs.is_empty() && suf_segs.is_empty() {
                    continue;
                }

                for start in 0..tokens.len() {
                    let cid = tokens[start].clause_id;
                    let slot_start_base = if pref_segs.is_empty() {
                        start
                    } else {
                        match coincidir_segmentos_en_texto(&tokens, start, &pref_segs) {
                            Some(e) => e,
                            None => continue,
                        }
                    };
                    // Permitir 1 conector opcional "de"/"del" entre prefijo y slot (ej. "JPM de Chimichagua", "AUD DE NULIDAD")
                    let mut slot_start = slot_start_base;
                    if !pref_segs.is_empty()
                        && slot_start < tokens.len()
                        && tokens[slot_start].clause_id == cid
                        && matches!(tokens[slot_start].norm.as_str(), "de" | "del")
                    {
                        slot_start += 1;
                    }
                    if slot_start >= tokens.len()
                        || tokens[slot_start].clause_id != cid
                        || tokens[slot_start].es_barrera
                    {
                        continue;
                    }

                    // Probar de 4 palabras hacia 1 (prioriza nombres compuestos exactos del catálogo)
                    let mut encontrado: Option<(usize, String, String)> = None;
                    for span_len in (1..=4).rev() {
                        let slot_end = slot_start + span_len;
                        if slot_end > tokens.len() {
                            continue;
                        }
                        if tokens[slot_start..slot_end]
                            .iter()
                            .any(|t| t.clause_id != cid || t.es_barrera)
                        {
                            continue;
                        }
                        // Validar sufijo si existe
                        let match_end = if suf_segs.is_empty() {
                            slot_end
                        } else {
                            match coincidir_segmentos_en_texto(&tokens, slot_end, &suf_segs) {
                                Some(e) => e,
                                None => continue,
                            }
                        };

                        let primero = &tokens[slot_start];
                        let ultimo = &tokens[slot_end - 1];
                        if tokens[slot_start..slot_end]
                            .iter()
                            .any(|t| t.raw.chars().any(|c| c.is_ascii_digit()))
                        {
                            continue;
                        }
                        if ultimo.es_stopword
                            || es_palabra_bloqueada_slot_texto(&pat.campo, &ultimo.norm)
                        {
                            continue;
                        }
                        if span_len == 1
                            && (primero.es_stopword
                                || es_palabra_bloqueada_slot_texto(&pat.campo, &primero.norm))
                        {
                            continue;
                        }
                        if span_len > 1
                            && !matches!(primero.norm.as_str(), "la" | "el" | "las" | "los" | "san" | "santa" | "puerto" | "villa")
                            && (primero.es_stopword
                                || es_palabra_bloqueada_slot_texto(&pat.campo, &primero.norm))
                        {
                            continue;
                        }

                        let slot_raw = tokens[slot_start..slot_end]
                            .iter()
                            .map(|t| t.raw.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        let Some(expansion) =
                            instanciar_patron_texto(&pat.patron_canonico, &slot_raw)
                        else {
                            continue;
                        };

                        let clave_cat = normalizar_clave_catalogo(&expansion);
                        let cat_hit = if pat.campo == "juzgado" {
                            cat_juz.get(&clave_cat).cloned()
                        } else {
                            cat_tipo.get(&clave_cat).cloned()
                        };

                        if let Some(exacto_cat) = cat_hit {
                            encontrado = Some((match_end, slot_raw, exacto_cat));
                            break;
                        }

                        // Vocabulario abierto (ej. municipio de otro departamento no listado en juzgados_base.json)
                        if pat.campo == "juzgado" {
                            let valido_abierto = if span_len == 1 {
                                primero.norm.chars().count() >= 4
                                    && primero.raw.chars().all(|c| c.is_alphabetic())
                                    && (!tiene_mayus_mixtas || primero.es_mayus_inicial)
                            } else if span_len == 2 {
                                let pref_toponimo = matches!(
                                    primero.norm.as_str(),
                                    "san" | "santa" | "el" | "la" | "puerto" | "villa"
                                );
                                pref_toponimo
                                    && ultimo.norm.chars().count() >= 4
                                    && ultimo.raw.chars().all(|c| c.is_alphabetic())
                                    && (!tiene_mayus_mixtas
                                        || (primero.es_mayus_inicial && ultimo.es_mayus_inicial))
                            } else {
                                false
                            };
                            if valido_abierto {
                                encontrado = Some((match_end, slot_raw, expansion));
                                break;
                            }
                        } else if span_len == 1
                            && primero.norm.chars().count() >= 4
                            && primero.raw.chars().all(|c| c.is_alphabetic())
                        {
                            encontrado = Some((match_end, slot_raw, expansion));
                            break;
                        }
                    }

                    let Some((end, _slot_raw, expansion_final)) = encontrado else {
                        continue;
                    };
                    let raw_span = tokens[start..end]
                        .iter()
                        .map(|t| t.raw.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let concrete_key = canonizar_entrada_alias_general(&raw_span);
                    if existe_bloqueo_concreto_en_alias(
                        &combinados,
                        &pat.campo,
                        &concrete_key,
                        &expansion_final,
                        &def_clave,
                    ) {
                        continue;
                    }

                    candidatos.push(CandidatoSpan::Inferido {
                        campo: pat.campo.clone(),
                        start,
                        end,
                        segs_len: pref_segs.len() + suf_segs.len() + 1,
                        es_defensor: es_def,
                        q10: pat.confianza_q10,
                        res: ResolucionPatronInferido {
                            campo: pat.campo.clone(),
                            abreviacion_detectada: raw_span,
                            expansion_sugerida: expansion_final,
                            patron_entrada: pat.patron_entrada.clone(),
                            ejemplos_distintos: pat.ejemplos_distintos,
                            total_instancias: pat.total_instancias,
                            consistencia: pat.consistencia,
                            confianza_q10: pat.confianza_q10,
                        },
                    });
                }
            }
        }
    }

    // 3. Selección sin solapamiento por campo: tramo más largo gana; en empate, exacto > inferido
    candidatos.sort_by(|a, b| b.clave_orden().cmp(&a.clave_orden()));

    let mut ocupados_juz: Vec<(usize, usize)> = Vec::new();
    let mut ocupados_tipo: Vec<(usize, usize)> = Vec::new();
    let mut vistas_exactas: BTreeSet<(String, String)> = BTreeSet::new();
    let mut vistas_inferidas: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out = ResolucionAbreviacionesTexto::default();

    for cand in candidatos {
        let (st, en) = cand.span();
        let ocupados = if cand.campo() == "tipo" {
            &mut ocupados_tipo
        } else {
            &mut ocupados_juz
        };
        let solapa = ocupados.iter().any(|&(a, b)| st < b && a < en);
        if solapa {
            continue;
        }
        ocupados.push((st, en));

        match cand {
            CandidatoSpan::Exacto {
                campo,
                abrev_norm,
                canonico,
                ..
            } => {
                if vistas_exactas.insert((campo.clone(), abrev_norm.clone())) {
                    out.exactas.push((campo, abrev_norm, canonico));
                }
            }
            CandidatoSpan::Inferido { res, .. } => {
                let k = (
                    res.campo.clone(),
                    canonizar_entrada_alias_general(&res.abreviacion_detectada),
                );
                if vistas_inferidas.insert(k) {
                    out.inferidas.push(res);
                }
            }
        }
    }

    out
}

/// Indica si una sigla corta tipo `"j4"` (incluyendo sus variantes `"J04"`, `"J 4"`, `"Jdo 4"`,
/// `"Jzg 4"`, `"Juzgado 4"`) aparece en `texto` sin un calificador explícito de despacho
/// inmediatamente siguiente en la misma cláusula (`CTO`, `Circuito`, `Conocimiento`, `MPAL`, etc.).
pub(crate) fn sigla_desnuda_sin_calificador_en_texto(texto: &str, sigla: &str) -> bool {
    let Some(num_str) = sigla.strip_prefix('j') else {
        return false;
    };
    let Ok(n_obj) = num_str.parse::<u32>() else {
        return false;
    };
    let (tokens, _) = tokenizar_texto_con_contexto(texto);
    let variantes: [&[SegmentoPatron]; 4] = [
        &[SegmentoPatron::Alpha("j".into()), SegmentoPatron::Num(n_obj)],
        &[SegmentoPatron::Alpha("jdo".into()), SegmentoPatron::Num(n_obj)],
        &[SegmentoPatron::Alpha("jzg".into()), SegmentoPatron::Num(n_obj)],
        &[SegmentoPatron::Alpha("juzgado".into()), SegmentoPatron::Num(n_obj)],
    ];
    for start in 0..tokens.len() {
        let cid = tokens[start].clause_id;
        for var in &variantes {
            if let Some(end) = coincidir_segmentos_en_texto(&tokens, start, var) {
                if !siguiente_token_es_calificador(&tokens, end, cid) {
                    return true;
                }
            }
        }
    }
    false
}

/// Si el LLM devolvió `juzgado` o `tipo` vacío, busca si `texto` contiene una única abreviación
/// exacta validada (`q10 >= 0.30`) para ese campo (nunca aplica patrones inferidos a ciegas).
pub(crate) fn resolver_abreviacion_exacta_unica_en_texto(
    campo: &str,
    texto: &str,
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    defensor: Option<&str>,
) -> Option<String> {
    let res = resolver_abreviaciones_y_patrones_en_texto(
        texto,
        base,
        local,
        &AppSettings::default(),
        &IndicePriorsExcel::default(),
        defensor,
    );
    let exactas_campo: Vec<String> = res
        .exactas
        .into_iter()
        .filter(|(c, _, _)| c == campo)
        .map(|(_, _, canon)| canon)
        .collect();
    let unicas: BTreeSet<&str> = exactas_campo.iter().map(String::as_str).collect();
    if unicas.len() == 1 {
        exactas_campo.into_iter().next()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Cosecha general de abreviaciones desde el texto al guardar feedback
// ---------------------------------------------------------------------------

fn extraer_numero_unico_juzgado(juzgado: &str) -> Option<u32> {
    let spans = extraer_spans_palabras(juzgado);
    let mut encontrados: BTreeSet<u32> = BTreeSet::new();
    for sp in spans {
        if sp.raw.chars().all(|c| c.is_ascii_digit()) {
            if sp.raw.len() < 6 {
                if let Ok(v) = sp.raw.parse::<u32>() {
                    if (1..=MAX_SLOT_NUM).contains(&v) {
                        encontrados.insert(v);
                    }
                }
            }
        } else if let Some((ord_n, _)) = parsear_ordinal_con_estilo(sp.raw) {
            encontrados.insert(ord_n);
        }
    }
    if encontrados.len() == 1 {
        encontrados.into_iter().next()
    } else {
        None
    }
}

fn es_prefijo_separado_juzgado_valido(tok: &TokenTexto, juzgado_lower: &str) -> bool {
    if tok.es_barrera || tok.es_stopword || tok.norm.is_empty() || tok.norm.len() > 7 {
        return false;
    }
    if !tok.norm.chars().all(|c| c.is_alphabetic()) {
        return false;
    }
    if PREFIJOS_NO_JUZGADO_NUMERICOS.contains(&tok.norm.as_str())
        && !juzgado_lower.contains(&tok.norm)
    {
        return false;
    }
    if MESES_ES.contains(&tok.norm.as_str())
        || BLOQUEO_SLOT_TEXTO_JUZGADO.contains(&tok.norm.as_str())
        || PALABRAS_CORTAS_COMUNES_2.contains(&tok.norm.as_str())
    {
        return false;
    }
    if RAICES_DESPACHO_SEPARADO.contains(&tok.norm.as_str()) {
        return true;
    }
    // Cualquier abreviación en mayúsculas (ej. "JPM 10", "JPC 3", "P 4", "M 10", "C 2")
    let todo_mayus = tok.raw.chars().all(|c| c.is_uppercase());
    todo_mayus && tok.norm.len() <= 5
}

/// Extrae una abreviación numérica inequívoca de `texto` cuyo número coincida con el número único
/// de `juzgado_final` (funciona para cualquier convención: `J10`, `JPM10`, `J10M`, `EPMS 2`,
/// `J 10 PM`, `J10 CTO`, `10 MPAL`, `P4`, `C2`, etc., preservando calificadores adjuntos).
pub(crate) fn extraer_abreviacion_numerica_unica_para_juzgado(
    texto: &str,
    juzgado_final: &str,
) -> Option<String> {
    let n_obj = extraer_numero_unico_juzgado(juzgado_final)?;
    let juz_lower = quitar_tildes(&juzgado_final.to_lowercase());
    let (tokens, _) = tokenizar_texto_con_contexto(texto);
    if tokens.is_empty() {
        return None;
    }

    let mut candidatos: BTreeMap<Vec<SegmentoPatron>, String> = BTreeMap::new();

    for (idx, tok) in tokens.iter().enumerate() {
        if tok.es_barrera || tok.es_stopword || tok.segmentos.is_empty() {
            continue;
        }
        let cid = tok.clause_id;

        // Caso 1: token compacto alfanumérico con `n_obj` (ej. J10, JPM10, J10M, EPMS2, 10MPAL, P4)
        let tiene_alpha = tok
            .segmentos
            .iter()
            .any(|s| matches!(s, SegmentoPatron::Alpha(_)));
        let nums_tok: Vec<u32> = tok
            .segmentos
            .iter()
            .filter_map(|s| match s {
                SegmentoPatron::Num(v) => Some(*v),
                _ => None,
            })
            .collect();

        if tiene_alpha && nums_tok == [n_obj] && tok.norm.len() <= 12 {
            // Descartar códigos no-juzgado (F10, Art10, Ley10, Rad10, etc.) salvo que el juzgado lo mencione
            let prefijo_no_juz = matches!(
                tok.segmentos.first(),
                Some(SegmentoPatron::Alpha(a))
                    if PREFIJOS_NO_JUZGADO_NUMERICOS.contains(&a.as_str())
                        && !juz_lower.contains(a.as_str())
            );
            if prefijo_no_juz || es_patron_hora_ambigua(&tok.segmentos) {
                continue;
            }
            let ya_calificado = termina_en_calificador_terminal(&tok.segmentos);
            let mut frase = tok.norm.clone();
            if !ya_calificado {
                let mut cursor = idx + 1;
                for _ in 0..2 {
                    if let Some(q_idx) = indice_siguiente_calificador(&tokens, cursor, cid) {
                        let q_norm = &tokens[q_idx].norm;
                        frase = format!("{} {}", frase, q_norm);
                        cursor = q_idx + 1;
                        if es_calificador_terminal_juzgado(q_norm) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            if !es_identidad_alias(&frase, juzgado_final) && es_clave_alias_valida(&frase) {
                let clave = clave_estructural_alias(&frase);
                candidatos.entry(clave).or_insert(frase);
            }
            continue;
        }

        // Caso 2: número separado `n_obj` precedido por abreviación (ej. "JPM 10", "EPMS 2", "J 10 PM")
        // o seguido de calificador de despacho (ej. "10 MPAL", "3 CTO", "4 PC")
        if tok.segmentos.as_slice() == [SegmentoPatron::Num(n_obj)] {
            if idx > 0
                && tokens[idx - 1].clause_id == cid
                && es_prefijo_separado_juzgado_valido(&tokens[idx - 1], &juz_lower)
            {
                let pref = &tokens[idx - 1].norm;
                // Si el texto ya trae el nombre completo ("Juzgado 4 Penal Municipal..."),
                // no cosechar un prefijo truncado como si fuera una abreviación.
                if pref == "juzgado" && siguiente_token_es_calificador(&tokens, idx + 1, cid) {
                    continue;
                }
                let mut frase = format!("{} {}", pref, n_obj);
                let mut cursor = idx + 1;
                for _ in 0..2 {
                    if let Some(q_idx) = indice_siguiente_calificador(&tokens, cursor, cid) {
                        let q_norm = &tokens[q_idx].norm;
                        frase = format!("{} {}", frase, q_norm);
                        cursor = q_idx + 1;
                        if es_calificador_terminal_juzgado(q_norm) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                if !es_identidad_alias(&frase, juzgado_final) && es_clave_alias_valida(&frase) {
                    let clave = clave_estructural_alias(&frase);
                    candidatos.entry(clave).or_insert(frase);
                }
            } else if let Some(q_idx) = indice_siguiente_calificador(&tokens, idx + 1, cid) {
                // Solo aceptar si el calificador está inmediatamente adyacente (no tras "de"/"del")
                if q_idx == idx + 1 {
                    let qual = &tokens[q_idx].norm;
                    // Evitar "10 pm" (hora) o 1 sola letra ambigua sin prefijo
                    if !SUFIJOS_TIEMPO.contains(&qual.as_str()) && qual.len() >= 2 {
                        let frase = format!("{} {}", n_obj, qual);
                        if !es_identidad_alias(&frase, juzgado_final)
                            && es_clave_alias_valida(&frase)
                        {
                            let clave = clave_estructural_alias(&frase);
                            candidatos.entry(clave).or_insert(frase);
                        }
                    }
                }
            }
        }
    }

    if candidatos.len() == 1 {
        candidatos.into_values().next()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aprendizaje::{AliasEstadistico, MemoriaAprendizaje};
    use crate::models::GrafoAsociaciones;

    fn alias(
        campo: &str,
        entrada: &str,
        canonico: &str,
        alpha: f64,
        beta: f64,
        defensor: &str,
    ) -> AliasEstadistico {
        let mut confs = Vec::new();
        if !es_alcance_general(defensor) {
            confs.push(normalizar_clave_defensor(defensor));
        }
        AliasEstadistico {
            campo: campo.into(),
            entrada_norm: canonizar_entrada_alias_general(entrada),
            canonico: canonico.into(),
            alpha,
            beta,
            defensor_alcance: if es_alcance_general(defensor) {
                "*".into()
            } else {
                normalizar_clave_defensor(defensor)
            },
            defensores_confirmantes: confs,
            ultima_actualizacion: "2026-01-10T10:00:00-05:00".into(),
        }
    }

    #[test]
    fn test_generalizacion_cualquier_abreviacion_numerica_y_ordinal() {
        let mut mem = MemoriaAprendizaje::default();
        let def = "Dra. Ana Pérez";

        // 1) J10, J9, J3 -> Juzgado N Penal Municipal (incluso con 1 confirmación cada uno)
        mem.tabla_alias.push(alias("juzgado", "J10", "Juzgado 10 Penal Municipal", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "J9", "Juzgado 9 Penal Municipal", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "J3", "Juzgado 3 Penal Municipal", 2.0, 1.2, def));

        // 2) JPM10, JPM 03 -> Juzgado N Promiscuo Municipal de Aguachica (prefijo largo + espacio/cero)
        mem.tabla_alias.push(alias("juzgado", "JPM10", "Juzgado 10 Promiscuo Municipal de Aguachica", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "JPM 03", "Juzgado 3 Promiscuo Municipal de Aguachica", 2.0, 1.2, def));

        // 3) J1M, J2M -> Juzgado N Penal Municipal de Valledupar (infijo: prefijo + N + sufijo)
        mem.tabla_alias.push(alias("juzgado", "J1M", "Juzgado 1 Penal Municipal de Valledupar", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "J2 M", "Juzgado 2 Penal Municipal de Valledupar", 2.0, 1.2, def));

        // 4) EPMS 1, EPMS-2 -> Juzgado N de Ejecución de Penas y Medidas de Seguridad de Valledupar
        mem.tabla_alias.push(alias("juzgado", "EPMS 1", "Juzgado 1 de Ejecución de Penas y Medidas de Seguridad de Valledupar", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "EPMS-2", "Juzgado 2 de Ejecución de Penas y Medidas de Seguridad de Valledupar", 2.0, 1.2, def));

        // 5) P1, P2 -> Juzgado Primero/Segundo Penal del Circuito (ordinal español en canónico)
        mem.tabla_alias.push(alias("juzgado", "P1", "Juzgado Primero Penal del Circuito", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "P2", "Juzgado Segundo Penal del Circuito", 2.0, 1.2, def));

        // 6) 1 MPAL, 2MPAL -> Juzgado N Penal Municipal de Santa Marta (sufijo)
        mem.tabla_alias.push(alias("juzgado", "1 MPAL", "Juzgado 1 Penal Municipal de Santa Marta", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "2MPAL", "Juzgado 2 Penal Municipal de Santa Marta", 2.0, 1.2, def));

        // 7) Transformación de estilo: Juzgado N Circuito Penal -> Juzgado N Cto Penal
        mem.tabla_alias.push(alias("juzgado", "Juzgado 1 Circuito Penal", "Juzgado 1 Cto Penal", 2.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "Juzgado 3 Circuito Penal", "Juzgado 3 Cto Penal", 2.0, 1.2, def));

        // Auto-alias de identidad NO deben contaminar ni crear patrones redundantes
        mem.tabla_alias.push(alias("juzgado", "Juzgado 10 Penal Municipal", "Juzgado 10 Penal Municipal", 4.0, 1.2, def));
        mem.tabla_alias.push(alias("juzgado", "Juzgado 9 Penal Municipal", "Juzgado 9 Penal Municipal", 4.0, 1.2, def));

        let base = MemoriaAprendizaje::default();
        let settings = AppSettings::default();
        let priors = IndicePriorsExcel::default();

        // J4 nunca visto -> Juzgado 4 Penal Municipal (incluyendo variantes J04, J-4, J 4)
        for texto in [
            "J4 audiencia preparatoria",
            "J04 audiencia preparatoria",
            "J-4 audiencia preparatoria",
            "J 4 audiencia preparatoria",
        ] {
            let res = resolver_abreviaciones_y_patrones_en_texto(
                texto, &base, &mem, &settings, &priors, Some(def),
            );
            assert!(
                res.inferidas
                    .iter()
                    .any(|p| p.expansion_sugerida == "Juzgado 4 Penal Municipal"),
                "Falló '{}': {:?}",
                texto,
                res.inferidas
            );
        }

        // JPM4 / J.P.M. 04 nunca visto
        let res_jpm = resolver_abreviaciones_y_patrones_en_texto(
            "J.P.M. 04 audiencia preliminar",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            res_jpm
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4 Promiscuo Municipal de Aguachica"),
            "{:?}",
            res_jpm.inferidas
        );

        // J4M / J4 M (más específico que J4: debe elegir Valledupar, NO bare J4)
        let res_j4m = resolver_abreviaciones_y_patrones_en_texto(
            "Audiencia en J4 M realizada sin novedad",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert_eq!(res_j4m.inferidas.len(), 1, "{:?}", res_j4m.inferidas);
        assert_eq!(
            res_j4m.inferidas[0].expansion_sugerida,
            "Juzgado 4 Penal Municipal de Valledupar"
        );

        // EPMS 3 nunca visto
        let res_epms = resolver_abreviaciones_y_patrones_en_texto(
            "EPMS3 audiencia de libertad condicional",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            res_epms.inferidas.iter().any(|p| {
                p.expansion_sugerida
                    == "Juzgado 3 de Ejecución de Penas y Medidas de Seguridad de Valledupar"
            }),
            "{:?}",
            res_epms.inferidas
        );

        // P4 -> Juzgado Cuarto Penal del Circuito (ordinal español)
        let res_ord = resolver_abreviaciones_y_patrones_en_texto(
            "P4 audiencia de acusación",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            res_ord
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado Cuarto Penal del Circuito"),
            "{:?}",
            res_ord.inferidas
        );
        // P25 (> 20 fuera de tabla ordinal segura) no inventa texto raro
        let res_ord25 = resolver_abreviaciones_y_patrones_en_texto(
            "P25 audiencia de acusación",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(res_ord25.inferidas.is_empty(), "{:?}", res_ord25.inferidas);

        // 4 MPAL -> Juzgado 4 Penal Municipal de Santa Marta
        let res_suf = resolver_abreviaciones_y_patrones_en_texto(
            "Audiencia en 4 MPAL realizada",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            res_suf
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4 Penal Municipal de Santa Marta"),
            "{:?}",
            res_suf.inferidas
        );

        // Juzgado 5 Circuito Penal -> Juzgado 5 Cto Penal
        let res_cto = resolver_abreviaciones_y_patrones_en_texto(
            "Audiencia en Juzgado 5 Circuito Penal sin novedad",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            res_cto
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 5 Cto Penal"),
            "{:?}",
            res_cto.inferidas
        );
    }

    #[test]
    fn test_generalizacion_slot_texto_juzgado_y_tipo_con_bloqueo_falsos_positivos() {
        let mut mem = MemoriaAprendizaje::default();
        let def = "Dra. Ana Pérez";

        // Juzgados con slot de municipio: JPM {X} -> Juzgado Promiscuo Municipal de {X}
        mem.tabla_alias.push(alias(
            "juzgado",
            "JPM Bosconia",
            "Juzgado Promiscuo Municipal de Bosconia",
            2.0,
            1.2,
            def,
        ));
        mem.tabla_alias.push(alias(
            "juzgado",
            "JPM Curumaní",
            "Juzgado Promiscuo Municipal de Curumaní",
            2.0,
            1.2,
            def,
        ));

        // Tipos de audiencia con slot léxico: AUD {X} -> Audiencia de {x}
        mem.tabla_alias.push(alias(
            "tipo",
            "AUD ACUSACION",
            "Audiencia de acusación",
            2.0,
            1.2,
            def,
        ));
        mem.tabla_alias.push(alias(
            "tipo",
            "AUD IMPUTACION",
            "Audiencia de imputación",
            2.0,
            1.2,
            def,
        ));

        let base = MemoriaAprendizaje::default();
        let settings = AppSettings::default();
        let priors = IndicePriorsExcel::default();

        // Municipio listado en juzgados_base.json (1 palabra y multipalabra)
        let r1 = resolver_abreviaciones_y_patrones_en_texto(
            "JPM Chimichagua AUD NULIDAD, se realizó sin novedad",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            r1.inferidas
                .iter()
                .any(|p| p.campo == "juzgado"
                    && p.expansion_sugerida == "Juzgado Promiscuo Municipal de Chimichagua"),
            "{:?}",
            r1.inferidas
        );
        assert!(
            r1.inferidas
                .iter()
                .any(|p| p.campo == "tipo" && p.expansion_sugerida == "Audiencia de nulidad"),
            "{:?}",
            r1.inferidas
        );

        let r_multi = resolver_abreviaciones_y_patrones_en_texto(
            "JPM La Jagua de Ibirico AUD DE PRORROGA",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            r_multi.inferidas.iter().any(|p| p.expansion_sugerida
                == "Juzgado Promiscuo Municipal de La Jagua de Ibirico"),
            "{:?}",
            r_multi.inferidas
        );
        assert!(
            r_multi
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Audiencia de prórroga"),
            "Debe restaurar tilde oficial del catálogo: {:?}",
            r_multi.inferidas
        );

        // Municipio de otro departamento no listado en juzgados_base.json
        let r_otro = resolver_abreviaciones_y_patrones_en_texto(
            "JPM Turbaco audiencia preliminar",
            &base,
            &mem,
            &settings,
            &priors,
            Some(def),
        );
        assert!(
            r_otro
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado Promiscuo Municipal de Turbaco"),
            "{:?}",
            r_otro.inferidas
        );

        // Falsos positivos que JAMÁS deben disparar `jpm {X}` o `aud {X}`
        for trampa in [
            "JPM audiencia preparatoria, se realizó sin novedad",
            "JPM no se conectó el juez",
            "JPM se reprogramó para el 15/08/2025",
            "JPM estuvo ocupado en otra diligencia",
            "AUD en J10 se realizó sin novedad",
            "AUD mañana a las 4pm",
        ] {
            let r_bad = resolver_abreviaciones_y_patrones_en_texto(
                trampa, &base, &mem, &settings, &priors, Some(def),
            );
            assert!(
                r_bad.inferidas.is_empty(),
                "Falso positivo en '{}': {:?}",
                trampa,
                r_bad.inferidas
            );
        }
    }

    #[test]
    fn test_candados_seguridad_inconsistencia_calificadores_override_y_aislamiento() {
        let base = MemoriaAprendizaje::default();
        let settings = AppSettings::default();
        let priors = IndicePriorsExcel::default();
        let ana = "Dra. Ana Pérez";
        let luis = "Dr. Luis Mora";

        // 1) Inconsistencia: J10=Mpal, J9=Mpal, J3=Circuito (2/3 = 66.7% < 85%) -> NO generalizar
        let mut mem_mix = MemoriaAprendizaje::default();
        mem_mix.tabla_alias.push(alias("juzgado", "J10", "Juzgado 10 Penal Municipal", 3.0, 1.2, ana));
        mem_mix.tabla_alias.push(alias("juzgado", "J9", "Juzgado 9 Penal Municipal", 3.0, 1.2, ana));
        mem_mix.tabla_alias.push(alias("juzgado", "J3", "Juzgado 3 Penal del Circuito", 3.0, 1.2, ana));
        let r_mix = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &mem_mix,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(r_mix.inferidas.is_empty(), "No debe generalizar mezcla contradictoria: {:?}", r_mix.inferidas);

        // 2) Calificadores explícitos (CTO, Circuito, Conocimiento, Especializado, Promiscuo) nunca se pisan
        let mut mem_ok = MemoriaAprendizaje::default();
        mem_ok.tabla_alias.push(alias("juzgado", "J10", "Juzgado 10 Penal Municipal", 3.0, 1.2, ana));
        mem_ok.tabla_alias.push(alias("juzgado", "J9", "Juzgado 9 Penal Municipal", 3.0, 1.2, ana));
        mem_ok.tabla_alias.push(alias("juzgado", "J3", "Juzgado 3 Penal Municipal", 3.0, 1.2, ana));
        for calificado in [
            "Audiencia en J4 CTO realizada",
            "Audiencia en J4 Circuito realizada",
            "Audiencia en J4 Conocimiento realizada",
            "Audiencia en J4 Especializado realizada",
            "Audiencia en J4 Promiscuo realizada",
            "Audiencia en J4 Penal del Circuito realizada",
            "Audiencia en J4 M realizada",
        ] {
            let r_cal = resolver_abreviaciones_y_patrones_en_texto(
                calificado, &base, &mem_ok, &settings, &priors, Some(ana),
            );
            assert!(
                r_cal.inferidas.is_empty(),
                "Calificador '{}' jamás debe ser pisado por j {{N}}: {:?}",
                calificado,
                r_cal.inferidas
            );
        }

        // 3) Aislamiento por defensor: Luis y planilla sin defensor (None) NO ven el patrón privado de Ana
        let r_luis = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &mem_ok,
            &settings,
            &priors,
            Some(luis),
        );
        assert!(r_luis.inferidas.is_empty());
        let r_none = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &mem_ok,
            &settings,
            &priors,
            None,
        );
        assert!(r_none.inferidas.is_empty());

        // 4) Corrección específica (incluso 1 sola o 2+) SIEMPRE gana y bloquea al patrón general
        let mut mem_ov = MemoriaAprendizaje::default();
        for n in [1, 2, 3, 5, 6, 9] {
            mem_ov.tabla_alias.push(alias(
                "juzgado",
                &format!("J{}", n),
                &format!("Juzgado {} Penal Municipal", n),
                3.0,
                1.2,
                ana,
            ));
        }
        // J4 corregido 1 sola vez a Circuito (alpha=2.0, beta=1.2)
        mem_ov.tabla_alias.push(alias("juzgado", "J4", "Juzgado 4 Penal del Circuito", 2.0, 1.2, ana));
        let r_j4 = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &mem_ov,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(
            r_j4.inferidas.is_empty(),
            "J4 tiene corrección específica a Circuito; patrón Municipal debe bloquearse en J4: {:?}",
            r_j4.inferidas
        );
        // Pero J7 (no visto) sí puede usar el patrón 6/7 (85.7% >= 85%)
        let r_j7 = resolver_abreviaciones_y_patrones_en_texto(
            "J7 audiencia preparatoria",
            &base,
            &mem_ov,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(
            r_j7.inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 7 Penal Municipal"),
            "{:?}",
            r_j7.inferidas
        );

        // 5) Retracción explícita de J8 -> Municipal impide volver a sugerir J8 -> Municipal
        mem_ov.tabla_alias.push(alias("juzgado", "J8", "Juzgado 8 Penal Municipal", 2.0, 3.2, ana));
        let r_j8 = resolver_abreviaciones_y_patrones_en_texto(
            "J8 audiencia preparatoria",
            &base,
            &mem_ov,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(r_j8.inferidas.is_empty(), "J8 retractado no debe re-sugerirse: {:?}", r_j8.inferidas);

        // 6) Respaldo frío desde grafo Excel requiere >= 3 distintos y 100% consistencia
        let mut pr_frio = IndicePriorsExcel {
            grafo: GrafoAsociaciones::default(),
            ..Default::default()
        };
        for n in [1, 2, 3] {
            pr_frio
                .grafo
                .defensor_sigla_a_juzgado
                .entry("ana perez".into())
                .or_default()
                .entry(format!("j{}", n))
                .or_default()
                .insert(format!("Juzgado {} Penal Municipal de Valledupar", n), 2);
        }
        let r_frio = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &MemoriaAprendizaje::default(),
            &settings,
            &pr_frio,
            Some(ana),
        );
        assert!(
            r_frio
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4 Penal Municipal de Valledupar"),
            "{:?}",
            r_frio.inferidas
        );
        // Al añadir 1 despacho contradictorio en grafo Excel, se desactiva el respaldo frío
        pr_frio
            .grafo
            .defensor_sigla_a_juzgado
            .entry("ana perez".into())
            .or_default()
            .entry("j5".into())
            .or_default()
            .insert("Juzgado 5 Penal del Circuito de Valledupar".into(), 1);
        let r_frio_mix = resolver_abreviaciones_y_patrones_en_texto(
            "J4 audiencia preparatoria",
            &base,
            &MemoriaAprendizaje::default(),
            &settings,
            &pr_frio,
            Some(ana),
        );
        assert!(r_frio_mix.inferidas.is_empty(), "{:?}", r_frio_mix.inferidas);
    }

    #[test]
    fn test_cosecha_abreviaciones_numericas_y_barreras_fecha_hora_fiscalia() {
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "JPM10 audiencia preparatoria, rad 200016001075202500010, F10 no vino",
                "Juzgado 10 Penal Municipal"
            )
            .as_deref(),
            Some("jpm10")
        );
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "J10M audiencia el 10/08/2025 a las 10:00 am",
                "Juzgado 10 Penal Municipal"
            )
            .as_deref(),
            Some("j10m")
        );
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia en EPMS 2 a las 2pm del 2 de mayo",
                "Juzgado 2 de Ejecución de Penas y Medidas de Seguridad"
            )
            .as_deref(),
            Some("epms 2")
        );
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia en J10 CTO realizada sin novedad",
                "Juzgado 10 Penal del Circuito"
            )
            .as_deref(),
            Some("j10 cto")
        );
        // Hora "4pm" o fecha "4 de marzo" jamás se confunden con juzgado 4
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia el 4 de marzo a las 4pm sin novedad",
                "Juzgado 4 Penal Municipal"
            ),
            None
        );
        // Validación de claves cortas: acepta JO, CG, M4, J4; rechaza 4pm, 10, no, si
        assert!(es_clave_alias_valida("jo"));
        assert!(es_clave_alias_valida("cg"));
        assert!(es_clave_alias_valida("m4"));
        assert!(es_clave_alias_valida("j4"));
        assert!(!es_clave_alias_valida("4pm"));
        assert!(!es_clave_alias_valida("10am"));
        assert!(!es_clave_alias_valida("10"));
        assert!(!es_clave_alias_valida("no"));
        assert!(!es_clave_alias_valida("si"));
    }

    #[test]
    fn test_red_team_bordes_unicode_ordinales_del_circuito_penal_y_promocion_general() {
        let base = MemoriaAprendizaje::default();
        let settings = AppSettings::default();
        let priors = IndicePriorsExcel::default();
        let ana = "Dra. Ana Pérez";

        // 1) Símbolos ordinales Unicode (`º`, `ª`, `°`) y ordinales abreviados (`1ro`/`2do`/`3ro` -> `4to`)
        let mut mem_ord = MemoriaAprendizaje::default();
        mem_ord.tabla_alias.push(alias("juzgado", "J1º", "Juzgado 1º Penal Municipal", 2.0, 1.2, ana));
        mem_ord.tabla_alias.push(alias("juzgado", "J2°", "Juzgado 2º Penal Municipal", 2.0, 1.2, ana));
        mem_ord.tabla_alias.push(alias("juzgado", "C1", "Juzgado 1ro Penal del Circuito", 2.0, 1.2, ana));
        mem_ord.tabla_alias.push(alias("juzgado", "C2", "Juzgado 2do Penal del Circuito", 2.0, 1.2, ana));

        let r_simb = resolver_abreviaciones_y_patrones_en_texto(
            "Audiencia en J4º y C4 realizada",
            &base,
            &mem_ord,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(
            r_simb
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4º Penal Municipal"),
            "Debe manejar º/° Unicode limpiamente: {:?}",
            r_simb.inferidas
        );
        assert!(
            r_simb
                .inferidas
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4to Penal del Circuito"),
            "Debe conjugar 1ro/2do -> 4to: {:?}",
            r_simb.inferidas
        );

        // 2) Calificadores con conector "de"/"del" ("J4 del Circuito", "J4 de Garantías", "J4 Penal del Circuito")
        let mut mem_pen = MemoriaAprendizaje::default();
        mem_pen.tabla_alias.push(alias("juzgado", "J1", "Juzgado 1 Penal Municipal", 3.0, 1.2, ana));
        mem_pen.tabla_alias.push(alias("juzgado", "J2", "Juzgado 2 Penal Municipal", 3.0, 1.2, ana));
        mem_pen.tabla_alias.push(alias("juzgado", "J1 Penal", "Juzgado 1 Penal Municipal", 3.0, 1.2, ana));
        mem_pen.tabla_alias.push(alias("juzgado", "J2 Penal", "Juzgado 2 Penal Municipal", 3.0, 1.2, ana));
        for calif in [
            "Audiencia en J4 del Circuito",
            "Audiencia en J4 de Garantías",
            "Audiencia en J4 de Conocimiento",
            "Audiencia en J4 de Ejecución",
            "Audiencia en J4 Penal del Circuito",
            "Audiencia en J4 Penal Especializado",
        ] {
            let r = resolver_abreviaciones_y_patrones_en_texto(
                calif, &base, &mem_pen, &settings, &priors, Some(ana),
            );
            assert!(
                r.inferidas.is_empty(),
                "Calificador con conector '{}' no debe ser pisado: {:?}",
                calif,
                r.inferidas
            );
        }

        // 3) Un nombre completo ya escrito ("Juzgado 4 Penal Municipal de Valledupar") NO se cosecha
        //    como abreviación truncada; sí se cosecha "Juzgado 4" desnudo o "J4 Penal del Circuito"
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia en Juzgado 4 Penal Municipal de Valledupar realizada",
                "Juzgado 4 Penal Municipal de Valledupar"
            ),
            None
        );
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia en Juzgado 4, realizada sin novedad",
                "Juzgado 4 Penal Municipal de Valledupar"
            )
            .as_deref(),
            Some("juzgado 4")
        );
        assert_eq!(
            extraer_abreviacion_numerica_unica_para_juzgado(
                "Audiencia en J4 Penal del Circuito realizada",
                "Juzgado 4 Penal del Circuito"
            )
            .as_deref(),
            Some("j4 penal circuito")
        );

        // 4) Coexistencia de alias exacto ("JPM"), patrón numérico ("JPM {N}") y patrón léxico ("JPM {X}")
        let mut mem_coex = MemoriaAprendizaje::default();
        mem_coex.tabla_alias.push(alias("juzgado", "JPM", "Juzgado Promiscuo Municipal de Bosconia", 4.0, 1.2, ana));
        mem_coex.tabla_alias.push(alias("juzgado", "JPM 1", "Juzgado 1 Promiscuo Municipal de Aguachica", 2.0, 1.2, ana));
        mem_coex.tabla_alias.push(alias("juzgado", "JPM 2", "Juzgado 2 Promiscuo Municipal de Aguachica", 2.0, 1.2, ana));
        mem_coex.tabla_alias.push(alias("juzgado", "JPM Bosconia", "Juzgado Promiscuo Municipal de Bosconia", 2.0, 1.2, ana));
        mem_coex.tabla_alias.push(alias("juzgado", "JPM Curumaní", "Juzgado Promiscuo Municipal de Curumaní", 2.0, 1.2, ana));

        let r_num = resolver_abreviaciones_y_patrones_en_texto(
            "JPM 3 audiencia preliminar",
            &base,
            &mem_coex,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(r_num.exactas.is_empty(), "Tramo más largo JPM 3 debe ganar sobre JPM: {:?}", r_num);
        assert_eq!(
            r_num.inferidas.first().map(|p| p.expansion_sugerida.as_str()),
            Some("Juzgado 3 Promiscuo Municipal de Aguachica")
        );

        let r_txt = resolver_abreviaciones_y_patrones_en_texto(
            "JPM Chimichagua audiencia preliminar",
            &base,
            &mem_coex,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(r_txt.exactas.is_empty(), "Tramo más largo JPM Chimichagua debe ganar sobre JPM: {:?}", r_txt);
        assert_eq!(
            r_txt.inferidas.first().map(|p| p.expansion_sugerida.as_str()),
            Some("Juzgado Promiscuo Municipal de Chimichagua")
        );

        let r_solo = resolver_abreviaciones_y_patrones_en_texto(
            "JPM audiencia preliminar",
            &base,
            &mem_coex,
            &settings,
            &priors,
            Some(ana),
        );
        assert!(r_solo.inferidas.is_empty());
        assert_eq!(
            r_solo.exactas.first().map(|(_, _, c)| c.as_str()),
            Some("Juzgado Promiscuo Municipal de Bosconia")
        );

        // 5) Herencia desde Cuaderno General ("*") + bloqueo cuando un defensor contradice "*" + override propio
        let mut mem_gen = MemoriaAprendizaje::default();
        let mut a10 = alias("juzgado", "JPM10", "Juzgado 10 Penal Municipal de Valledupar", 3.0, 1.2, "*");
        a10.defensores_confirmantes = vec!["ana perez".into(), "luis mora".into()];
        let mut a9 = alias("juzgado", "JPM9", "Juzgado 9 Penal Municipal de Valledupar", 3.0, 1.2, "*");
        a9.defensores_confirmantes = vec!["ana perez".into(), "luis mora".into()];
        mem_gen.tabla_alias.push(a10);
        mem_gen.tabla_alias.push(a9);
        // Carlos tiene 1 alias privado contradictorio (Santa Marta)
        mem_gen.tabla_alias.push(alias(
            "juzgado",
            "JPM1",
            "Juzgado 1 Penal Municipal de Santa Marta",
            2.0,
            1.2,
            "Dr. Carlos Conflicto",
        ));
        // Diego tiene su propio patrón consistente (Aguachica)
        mem_gen.tabla_alias.push(alias(
            "juzgado",
            "JPM1",
            "Juzgado 1 Promiscuo Municipal de Aguachica",
            2.0,
            1.2,
            "Dr. Diego Propio",
        ));
        mem_gen.tabla_alias.push(alias(
            "juzgado",
            "JPM2",
            "Juzgado 2 Promiscuo Municipal de Aguachica",
            2.0,
            1.2,
            "Dr. Diego Propio",
        ));

        let r_sofia = resolver_abreviaciones_y_patrones_en_texto(
            "JPM4 audiencia preparatoria",
            &base,
            &mem_gen,
            &settings,
            &priors,
            Some("Dra. Sofía Nueva"),
        );
        assert_eq!(
            r_sofia.inferidas.first().map(|p| p.expansion_sugerida.as_str()),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );

        let r_carlos = resolver_abreviaciones_y_patrones_en_texto(
            "JPM4 audiencia preparatoria",
            &base,
            &mem_gen,
            &settings,
            &priors,
            Some("Dr. Carlos Conflicto"),
        );
        assert!(
            r_carlos.inferidas.is_empty(),
            "Carlos contradice '*' con Santa Marta; '*' no debe imponerse: {:?}",
            r_carlos.inferidas
        );

        let r_diego = resolver_abreviaciones_y_patrones_en_texto(
            "JPM4 audiencia preparatoria",
            &base,
            &mem_gen,
            &settings,
            &priors,
            Some("Dr. Diego Propio"),
        );
        assert_eq!(
            r_diego.inferidas.first().map(|p| p.expansion_sugerida.as_str()),
            Some("Juzgado 4 Promiscuo Municipal de Aguachica")
        );

        // 6) Entradas vacías / solo signos / solo fechas y radicados nunca fallan ni generan ruido
        for vacio in ["", "   ", "--- , ; : ()", "15/08/2025 14:30 200016001075202500004 4pm"] {
            let r_v = resolver_abreviaciones_y_patrones_en_texto(
                vacio, &base, &mem_gen, &settings, &priors, Some("Dra. Sofía Nueva"),
            );
            assert!(r_v.exactas.is_empty() && r_v.inferidas.is_empty(), "{}: {:?}", vacio, r_v);
        }
    }
}
