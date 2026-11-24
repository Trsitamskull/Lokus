use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

pub const MOTIVOS_LABELS: [&str; 8] = [
    "Juez",
    "Fiscalía",
    "Usuario",
    "INPEC",
    "Víctima",
    "ICBF",
    "Defensor Confianza",
    "Defensor Público",
];

pub const TIPOS_AUDIENCIA: [&str; 27] = [
    "Alegatos de conclusión",
    "Aplicación principio de oportunidad",
    "Audiencia concentrada",
    "Audiencia de acusación",
    "Audiencia de celebración de preacuerdo",
    "Audiencia de conciliación",
    "Audiencia de continuación de juicio oral",
    "Audiencia de control de legalidad",
    "Audiencia de formulación de acusación",
    "Audiencia de individualización de pena",
    "Audiencia de imputación",
    "Audiencia de incidente de reparación integral",
    "Audiencia de juicio oral",
    "Audiencia de lectura de decisión",
    "Audiencia de libertad por vencimiento de términos",
    "Audiencia de medidas de aseguramiento",
    "Audiencia de nulidad",
    "Audiencia de preclusión",
    "Audiencia de prórroga",
    "Audiencia de revisión de medida",
    "Audiencia de sentido de fallo",
    "Audiencia de verificación de allanamiento a cargos",
    "Audiencia de verificación de cumplimiento",
    "Audiencia preliminar",
    "Audiencia preparatoria",
    "Sentencia",
    "Otra",
];

pub const HORAS_RAPIDAS: [(&str, &str); 6] = [
    ("07:30", "7:30 AM"),
    ("08:00", "8:00 AM"),
    ("09:00", "9:00 AM"),
    ("10:00", "10:00 AM"),
    ("14:00", "2:00 PM"),
    ("15:00", "3:00 PM"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudienciaInput {
    pub radicado: String,
    pub tipo: String,
    pub fecha: String,
    pub hora: String,
    pub juzgado: String,
    pub realizada: String, // "SI" | "NO"
    pub motivo: String,    // Nombre del motivo si realizada == "NO"
    pub observaciones: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistroAudiencia {
    pub fila_excel: u32,
    pub numero: u32,
    pub radicado: String,
    pub tipo: String,
    pub fecha: String,
    pub hora: String,
    pub juzgado: String,
    pub realizada: String,
    pub motivo: String,
    pub motivos_flags: [bool; 8],
    pub observaciones: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TotalesExcel {
    pub num_registros: u32,
    pub total_si: u32,
    pub total_no: u32,
    pub totales_motivos: [u32; 8],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicadoDetectado {
    pub numero: u32,
    pub fila_excel: u32,
    pub radicado: String,
    pub tipo: String,
    pub fecha: String,
    pub hora: String,
    pub juzgado: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MesDisponible {
    pub anio: i32,
    pub mes: u32,
    pub etiqueta: String,
    pub cantidad: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumenDefensor {
    pub defensor: String,
    pub archivos: Vec<String>,
    pub total: u32,
    pub total_si: u32,
    pub total_no: u32,
    pub efectividad_pct: f64,
    pub motivo_frecuente: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanillaResumen {
    #[serde(default)]
    pub id: String,
    pub archivo: String,
    pub defensor: String,
    pub num_registros: u32,
    #[serde(default)]
    pub archivada: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResumenMesPlanilla {
    pub anio: i32, // 0 cuando el registro no tiene fecha
    pub mes: u32,  // 0 cuando el registro no tiene fecha
    pub total: u32,
    pub total_si: u32,
    pub total_no: u32,
    pub totales_motivos: [u32; 8],
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResumenPlanillaArchivada {
    pub id: String,
    pub archivo: String,
    pub defensor: String,
    pub archivado_en: String,
    pub num_registros: u32,
    pub total_si: u32,
    pub total_no: u32,
    pub totales_motivos: [u32; 8],
    pub por_mes: Vec<ResumenMesPlanilla>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EstadisticasGlobales {
    pub total_audiencias: u32,
    pub total_si: u32,
    pub total_no: u32,
    pub efectividad_pct: f64,
    pub totales_motivos: [u32; 8],
    pub total_planillas: u32,
    pub total_planillas_con_datos: u32,
    pub total_archivadas: u32,
    pub meses_disponibles: Vec<MesDisponible>,
    pub planillas: Vec<PlanillaResumen>,
    pub defensores_disponibles: Vec<String>,
    pub por_defensor: Vec<ResumenDefensor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct ProcesoHabitual {
    pub radicado_completo: String,
    pub juzgado_habitual: String,
    pub tipo_habitual: String,
    #[serde(default)]
    pub defensor_habitual: String,
    pub frecuencia: u32,
}

/// Mapa ligero local de asociaciones (grafo ponderado por frecuencias de co-ocurrencia)
/// entre Defensor, Juzgados, Siglas cortas (`j4`, `j10`), Prefijo DANE del radicado y Tipo de audiencia.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct GrafoAsociaciones {
    /// defensor_clave -> (juzgado -> frecuencia)
    pub defensor_a_juzgados: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
    /// defensor_clave -> (tipo_audiencia -> frecuencia)
    pub defensor_a_tipos: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
    /// defensor_clave -> (sigla_corta ej. "j4" -> (juzgado -> frecuencia))
    pub defensor_sigla_a_juzgado: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
    >,
    /// prefijo DANE 5 dígitos del radicado (ej. "20001") -> (juzgado -> frecuencia)
    pub prefijo_radicado_a_juzgados:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
    /// tipo_audiencia normalizado -> (juzgado -> frecuencia)
    pub tipo_a_juzgados: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
}

impl GrafoAsociaciones {
    pub fn total_conexiones(&self) -> usize {
        let e1: usize = self.defensor_a_juzgados.values().map(|m| m.len()).sum();
        let e2: usize = self.defensor_a_tipos.values().map(|m| m.len()).sum();
        let e3: usize = self
            .defensor_sigla_a_juzgado
            .values()
            .flat_map(|s| s.values())
            .map(|m| m.len())
            .sum();
        let e4: usize = self
            .prefijo_radicado_a_juzgados
            .values()
            .map(|m| m.len())
            .sum();
        let e5: usize = self.tipo_a_juzgados.values().map(|m| m.len()).sum();
        e1 + e2 + e3 + e4 + e5
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IndicePriorsExcel {
    pub formato: u32,
    pub actualizada: String,
    pub total_registros: u32,
    pub juzgados_freq: std::collections::BTreeMap<String, u32>,
    pub tipos_freq: std::collections::BTreeMap<String, u32>,
    pub horas_freq: std::collections::BTreeMap<u32, u32>,
    pub realizada_si: u32,
    pub realizada_no: u32,
    pub motivos_freq: [u32; 8],
    pub procesos_por_numero: std::collections::BTreeMap<String, ProcesoHabitual>,
    #[serde(default)]
    pub grafo: GrafoAsociaciones,
}

/// Mapea cualquier variante del nombre de un motivo a su índice exacto de columna (0..8 -> Columnas I..P)
pub fn mapear_motivo_a_indice(motivo: &str) -> Option<usize> {
    let m = motivo.trim().to_lowercase();
    if m.is_empty() {
        return None;
    }
    if m.contains("juez") {
        Some(0)
    } else if m.contains("fiscal") {
        Some(1)
    } else if m.contains("usuario") || m.contains("procesado") {
        Some(2)
    } else if m.contains("inpec") {
        Some(3)
    } else if m.contains("víctima") || m.contains("victima") {
        Some(4)
    } else if m.contains("icbf") {
        Some(5)
    } else if m.contains("confianza") || m.contains("contractual") || m.contains("privado") {
        Some(6)
    } else if m.contains("público") || m.contains("publico") {
        Some(7)
    } else {
        None
    }
}

pub fn normalizar_texto_sin_tildes(texto: &str) -> String {
    let mut out = String::with_capacity(texto.len());
    let mut prev_space = true;
    for ch in texto.trim().to_lowercase().chars() {
        if ('\u{0300}'..='\u{036f}').contains(&ch) {
            continue;
        }
        let mapped = match ch {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if mapped == ' ' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(mapped);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Normaliza nombres de catálogos (`tipo`, `juzgado`) unificando mayúsculas, tildes (NFC/NFD)
/// y espacios adicionales sin colapsar variantes jurídicas reales ni convertir ordinales ("Segundo" != "2").
pub fn normalizar_clave_catalogo(texto: &str) -> String {
    normalizar_texto_sin_tildes(texto)
}

/// Normaliza el nombre de un Defensor Público para usarlo como clave de perfil estable
/// (insensible a tildes, mayúsculas, puntuación superficial y prefijos honoríficos como Dr./Dra./Defensor).
/// Devuelve cadena vacía si no hay defensor.
pub fn normalizar_clave_defensor(defensor: &str) -> String {
    const PREFIJOS_HONORIFICOS: &[&str] = &[
        "defensor publico",
        "defensora publica",
        "doctor",
        "doctora",
        "defensor",
        "defensora",
        "abogado",
        "abogada",
        "licenciado",
        "licenciada",
        "dra",
        "dr",
        "abg",
        "lic",
    ];
    let mut actual = normalizar_texto_sin_tildes(defensor);
    loop {
        if PREFIJOS_HONORIFICOS.contains(&actual.as_str()) {
            break;
        }
        let mut cambiado = false;
        for &pref in PREFIJOS_HONORIFICOS {
            if let Some(resto) = actual.strip_prefix(pref) {
                if let Some(sin_espacio) = resto.strip_prefix(' ') {
                    let siguiente = sin_espacio.trim();
                    if !siguiente.is_empty() {
                        actual = siguiente.to_string();
                        cambiado = true;
                        break;
                    }
                }
            }
        }
        if !cambiado {
            break;
        }
    }
    actual
}

/// Indica si un alcance de perfil representa el ámbito general/transversal (`""` o `"*"`).
pub fn es_alcance_general(alcance: &str) -> bool {
    let t = alcance.trim();
    t.is_empty() || t == "*"
}

/// Comprueba si un alcance específico de defensor coincide con `def_clave`.
/// Devuelve `false` si `alcance` es general (`""`/`"*"`) o si `def_clave` es vacío.
pub fn coincide_alcance_defensor(alcance: &str, def_clave: &str) -> bool {
    if es_alcance_general(alcance) {
        return false;
    }
    let da = normalizar_clave_defensor(alcance);
    let db = normalizar_clave_defensor(def_clave);
    !da.is_empty() && !db.is_empty() && da == db
}

/// Extrae el prefijo DANE municipal de 5 dígitos de un radicado judicial colombiano (SPOA/CUI),
/// p. ej. `"200016001075202255547"` -> `Some("20001")`, `"20001-60-01086-2022-00419"` -> `Some("20001")`,
/// `"20001 60 0107520130283900"` -> `Some("20001")`.
pub fn extraer_prefijo_dane_radicado(texto: &str) -> Option<String> {
    let numeros = extraer_numeros_radicado(texto);
    // 1. Radicados completos continuos o con guiones (18..=23 dígitos)
    for num in &numeros {
        if num.len() >= 18 && num.is_ascii() {
            return Some(num[..5].to_string());
        }
    }
    // 2. Radicados con espacios internos como "20001 60 0107520130283900" o "20001 60 01075 2022 00123"
    //    Evita falsos positivos con fechas ("12345 15/08/2025"), horas ("12345 12:00") o prosa ("54321 60 minutos").
    let es_grupo_radicado = |tok: &str| -> Option<String> {
        let trimmed = tok.trim_matches(|c: char| !c.is_ascii_digit());
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-')
        {
            return None;
        }
        // Descartar fechas con guiones tipo 15-08-2025 o 2025-08-15
        let partes: Vec<&str> = trimmed.split('-').collect();
        if partes.len() == 3
            && ((partes[0].len() <= 2 && partes[1].len() <= 2 && partes[2].len() == 4)
                || (partes[0].len() == 4 && partes[1].len() <= 2 && partes[2].len() <= 2))
        {
            return None;
        }
        let digitos: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
        if digitos.is_empty() {
            None
        } else {
            Some(digitos)
        }
    };
    let tokens: Vec<&str> = texto.split_whitespace().collect();
    for i in 0..tokens.len() {
        let Some(a) = es_grupo_radicado(tokens[i]) else {
            continue;
        };
        if a.len() != 5 {
            continue;
        }
        let Some(b) = tokens.get(i + 1).and_then(|t| es_grupo_radicado(t)) else {
            continue;
        };
        let mut total_digitos = a.len() + b.len();
        let mut j = i + 2;
        while let Some(nxt) = tokens.get(j).and_then(|t| es_grupo_radicado(t)) {
            total_digitos += nxt.len();
            j += 1;
            if total_digitos >= 23 {
                break;
            }
        }
        if ((b == "60" || b == "61") && total_digitos >= 12)
            || (b.len() >= 6 && total_digitos >= 16)
        {
            return Some(a);
        }
    }
    // 3. Respaldo para radicados truncados de 11..17 dígitos sin espacios
    for num in &numeros {
        if num.len() >= 11 && num.is_ascii() {
            return Some(num[..5].to_string());
        }
    }
    None
}

fn parsear_numero_despacho(token: &str) -> Option<u32> {
    let clean = token.trim_end_matches(['º', 'ª', '°']);
    if let Ok(n) = clean.parse::<u32>() {
        if (1..=99).contains(&n) {
            return Some(n);
        }
        return None;
    }
    match clean {
        "primero" | "primera" | "1o" | "1ro" | "1ra" | "1a" => Some(1),
        "segundo" | "segunda" | "2o" | "2do" | "2da" | "2a" => Some(2),
        "tercero" | "tercera" | "3o" | "3ro" | "3ra" | "3a" => Some(3),
        "cuarto" | "cuarta" | "4o" | "4to" | "4ta" | "4a" => Some(4),
        "quinto" | "quinta" | "5o" | "5to" | "5ta" | "5a" => Some(5),
        "sexto" | "sexta" | "6o" | "6to" | "6ta" | "6a" => Some(6),
        "septimo" | "septima" | "7o" | "7mo" | "7ma" | "7a" => Some(7),
        "octavo" | "octava" | "8o" | "8vo" | "8va" | "8a" => Some(8),
        "noveno" | "novena" | "9o" | "9no" | "9na" | "9a" => Some(9),
        "decimo" | "decima" | "10o" | "10mo" | "10ma" | "10a" => Some(10),
        "undecimo" | "undecima" | "once" => Some(11),
        "duodecimo" | "duodecima" | "doce" => Some(12),
        "trece" => Some(13),
        "catorce" => Some(14),
        "quince" => Some(15),
        "dieciseis" => Some(16),
        "diecisiete" => Some(17),
        "dieciocho" => Some(18),
        "diecinueve" => Some(19),
        "veinte" => Some(20),
        _ => None,
    }
}

/// Extrae siglas cortas canónicas de despacho (`"j4"`, `"j10"`, etc.) tanto desde notas telegráficas
/// (`"J04 CONTROL"`, `"J10 F25"`, `"Jdo 8"`) como desde nombres completos (`"Juzgado 4 Penal..."`,
/// `"Juzgado Segundo Penal..."`). Nunca confunde `"F25"` (Fiscalía) ni `"Juez"`.
pub fn extraer_siglas_cortas_juzgado(texto: &str) -> Vec<String> {
    let norm = normalizar_texto_sin_tildes(texto);
    let tokens: Vec<&str> = norm.split_whitespace().collect();
    let mut siglas: Vec<String> = Vec::new();
    let mut push_sigla = |n: u32| {
        let s = format!("j{}", n);
        if !siglas.contains(&s) {
            siglas.push(s);
        }
    };

    for (idx, &tok) in tokens.iter().enumerate() {
        // Forma compacta: j4, j04, j4º, j10, jdo4, jzg2
        if let Some(rest) = tok
            .strip_prefix("jdo")
            .or_else(|| tok.strip_prefix("jzg"))
            .or_else(|| tok.strip_prefix('j'))
        {
            let rest_clean = rest.trim_end_matches(['º', 'ª', '°']);
            if !rest_clean.is_empty() && rest_clean.chars().all(|c| c.is_ascii_digit()) {
                if let Some(n) = parsear_numero_despacho(rest_clean) {
                    push_sigla(n);
                    continue;
                }
            }
        }
        // Forma separada: "juzgado 4", "juzgado 04", "juzgado 4º", "juzgado segundo", "jdo 8", "j 10"
        if matches!(tok, "juzgado" | "jdo" | "jzg" | "j") {
            if let Some(&sig) = tokens.get(idx + 1) {
                if let Some(n) = parsear_numero_despacho(sig) {
                    push_sigla(n);
                }
            }
        }
    }

    siglas
}

/// Devuelve `Some("j#")` únicamente si `texto` consiste EXCLUSIVAMENTE en una sigla corta desnuda
/// de despacho (`"J4"`, `"J04"`, `"Jdo. 4"`, `"Jzg 2"`, `"Juzgado 4"`), sin calificadores
/// adicionales (`"J4 CTO"`, `"J4 MPAL"`, `"J4 Circuito"`, `"J4 Penal"` devuelven `None`).
pub fn es_solo_sigla_corta_juzgado(texto: &str) -> Option<String> {
    let norm = normalizar_texto_sin_tildes(texto);
    let tokens: Vec<&str> = norm.split_whitespace().collect();
    match tokens.as_slice() {
        [unico] => {
            let rest = unico
                .strip_prefix("jdo")
                .or_else(|| unico.strip_prefix("jzg"))
                .or_else(|| unico.strip_prefix('j'))?;
            let clean = rest.trim_end_matches(['º', 'ª', '°']);
            if !clean.is_empty() && clean.chars().all(|c| c.is_ascii_digit()) {
                parsear_numero_despacho(clean).map(|n| format!("j{}", n))
            } else {
                None
            }
        }
        [pref, num] if matches!(*pref, "juzgado" | "jdo" | "jzg" | "j") => {
            parsear_numero_despacho(num).map(|n| format!("j{}", n))
        }
        _ => None,
    }
}

fn es_negacion_previa_instalacion(palabras: &[&str], pos: usize) -> bool {
    let inicio = pos.saturating_sub(3);
    for &w in palabras[inicio..pos].iter().rev() {
        if matches!(w, "pero" | "aunque" | "empero" | "obstante") {
            break;
        }
        if matches!(
            w,
            "no" | "sin" | "nunca" | "jamas" | "tampoco" | "imposible" | "impidio" | "evito"
        ) {
            return true;
        }
    }
    false
}

/// Detecta si el texto (normalizado sin tildes en minúsculas) afirma que la audiencia
/// efectivamente se instaló o dio inicio, descartando cualquier negación cercana
/// (`"no se instaló"`, `"no pudo ser instalada"`, `"nunca se instaló"`, `"no quedó instalada"`, etc.).
pub(crate) fn texto_indica_instalacion_efectiva(texto_norm: &str) -> bool {
    for clausula in texto_norm.split(['.', ';', '\n']) {
        let palabras: Vec<&str> = clausula
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        for (i, &w) in palabras.iter().enumerate() {
            let es_se_instalo = w == "se"
                && palabras
                    .get(i + 1)
                    .is_some_and(|&s| matches!(s, "instalo" | "instlo" | "instal" | "inicio"));
            let es_se_dio_inicio = w == "se"
                && palabras.get(i + 1) == Some(&"dio")
                && palabras.get(i + 2) == Some(&"inicio");
            let es_instalada = w == "instalada";
            if (es_se_instalo || es_se_dio_inicio || es_instalada)
                && !es_negacion_previa_instalacion(&palabras, i)
            {
                return true;
            }
        }
    }
    false
}

/// Detecta una negación explícita de instalación o inicio de la diligencia
/// (`"no se instaló"`, `"no pudo ser instalada"`, `"no pudo instalarse"`, `"sin instalarse"`, `"no se dio inicio"`).
pub(crate) fn texto_indica_no_instalacion(texto_norm: &str) -> bool {
    for clausula in texto_norm.split(['.', ';', '\n']) {
        let palabras: Vec<&str> = clausula
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        for (i, &w) in palabras.iter().enumerate() {
            let es_verbo_inst = matches!(
                w,
                "instalo" | "instlo" | "instal" | "instalada" | "instalar" | "instalarse" | "inicio"
            );
            let es_se_verbo = w == "se"
                && palabras.get(i + 1).is_some_and(|&s| {
                    matches!(s, "instalo" | "instlo" | "instal" | "inicio")
                        || (s == "dio" && palabras.get(i + 2) == Some(&"inicio"))
                });
            if es_se_verbo && es_negacion_previa_instalacion(&palabras, i) {
                return true;
            }
            if es_verbo_inst
                && !(i > 0 && palabras[i - 1] == "se")
                && !(i > 1 && palabras[i - 2] == "se" && palabras[i - 1] == "dio")
                && es_negacion_previa_instalacion(&palabras, i)
            {
                // Evitar falsos positivos de "sin inicio" en frases irrelevantes; "inicio" solo con "se dio inicio"/"dar inicio"/"se inicio"
                if w == "inicio"
                    && !palabras[i.saturating_sub(2)..i]
                        .iter()
                        .any(|&p| matches!(p, "dar" | "dio" | "se"))
                {
                    continue;
                }
                return true;
            }
        }
    }
    false
}

/// Extrae secuencias numéricas de >= 6 dígitos (ignorando guiones/puntos/barras internos como 11001-60-00000-2025-00001-00).
pub fn extraer_numeros_radicado(texto: &str) -> Vec<String> {
    let mut numeros = Vec::new();
    let mut actual = String::new();

    for ch in texto.chars() {
        if ch.is_ascii_digit() {
            actual.push(ch);
        } else if matches!(ch, '-' | '.' | '/') && !actual.is_empty() {
            // Mantener dígitos unidos cuando están separados por guion/punto/barra
            continue;
        } else {
            if actual.len() >= 6 && !numeros.contains(&actual) {
                numeros.push(actual.clone());
            }
            actual.clear();
        }
    }
    if actual.len() >= 6 && !numeros.contains(&actual) {
        numeros.push(actual);
    }

    numeros
}

fn similitud_levenshtein(a: &str, b: &str) -> f64 {
    let va: Vec<char> = a.chars().collect();
    let vb: Vec<char> = b.chars().collect();
    let la = va.len();
    let lb = vb.len();
    if la == 0 && lb == 0 {
        return 1.0;
    }
    let max_len = la.max(lb);
    if max_len == 0 {
        return 1.0;
    }

    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut curr = vec![0usize; lb + 1];

    for i in 1..=la {
        curr[0] = i;
        for j in 1..=lb {
            let cost = if va[i - 1] == vb[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    1.0 - (prev[lb] as f64 / max_len as f64)
}

/// Determina si dos radicados/procesados corresponden al mismo proceso aunque existan errores tipográficos, tildes o espacios.
pub fn son_radicados_equivalentes(a: &str, b: &str) -> bool {
    let nums_a = extraer_numeros_radicado(a);
    let nums_b = extraer_numeros_radicado(b);

    // Si ambos tienen número de radicado (>= 6 dígitos), el número es la clave principal
    if !nums_a.is_empty() && !nums_b.is_empty() {
        return nums_a.iter().any(|na| nums_b.contains(na));
    }

    // Quitar números largos antes de comparar nombres para tolerar un registro con número y otro solo con nombre
    let limpiar_para_nombre = |s: &str| -> String {
        let sin_largos: String = s
            .split_whitespace()
            .filter(|tok| {
                let digitos: usize = tok.chars().filter(|c| c.is_ascii_digit()).count();
                digitos < 6
            })
            .collect::<Vec<_>>()
            .join(" ");
        normalizar_texto_sin_tildes(&sin_largos)
    };

    let na = limpiar_para_nombre(a);
    let nb = limpiar_para_nombre(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na == nb {
        return true;
    }

    if na.chars().count() < 5 || nb.chars().count() < 5 {
        return false;
    }

    let sim_total = similitud_levenshtein(&na, &nb);
    if sim_total < 0.82 {
        return false;
    }

    // Si ambos tienen 2+ palabras, evitar falsos positivos con distinto apellido ("carlos perez" vs "carlos gomez")
    let words_a: Vec<&str> = na.split_whitespace().collect();
    let words_b: Vec<&str> = nb.split_whitespace().collect();
    if words_a.len() >= 2 && words_b.len() >= 2 {
        let last_a = words_a[words_a.len() - 1];
        let last_b = words_b[words_b.len() - 1];
        if similitud_levenshtein(last_a, last_b) < 0.70 {
            return false;
        }
    }

    true
}

pub fn buscar_posible_duplicado(
    registros: &[RegistroAudiencia],
    datos: &AudienciaInput,
    excluir_fila: Option<u32>,
) -> Option<DuplicadoDetectado> {
    let fecha_objetivo = datos.fecha.trim();
    if fecha_objetivo.is_empty() {
        return None;
    }

    for r in registros {
        if excluir_fila == Some(r.fila_excel) {
            continue;
        }
        if r.fecha.trim() == fecha_objetivo && son_radicados_equivalentes(&r.radicado, &datos.radicado)
        {
            return Some(DuplicadoDetectado {
                numero: r.numero,
                fila_excel: r.fila_excel,
                radicado: r.radicado.clone(),
                tipo: r.tipo.clone(),
                fecha: r.fecha.clone(),
                hora: r.hora.clone(),
                juzgado: r.juzgado.clone(),
            });
        }
    }

    None
}

pub fn validar_audiencia(datos: &AudienciaInput, _permitir_hora_vacia: bool) -> Result<(), String> {
    if datos.radicado.trim().is_empty() {
        return Err("El campo 'Radicado del proceso' es obligatorio".to_string());
    }
    let realizada = datos.realizada.trim().to_uppercase();
    if realizada != "SI" && realizada != "NO" {
        return Err("Debe seleccionar si la audiencia se realizó o no (SI/NO)".to_string());
    }

    // Validar fecha DD/MM/AAAA entre 2018 y 2050 cuando se ingresa (opcional si se deja vacía)
    let fecha_str = datos.fecha.trim();
    if !fecha_str.is_empty() {
        let fecha = NaiveDate::parse_from_str(fecha_str, "%d/%m/%Y")
            .map_err(|_| format!("Formato de fecha inválido: {}. Use DD/MM/AAAA", fecha_str))?;
        let year = fecha.format("%Y").to_string().parse::<i32>().unwrap_or(0);
        if !(2018..=2050).contains(&year) {
            return Err(format!(
                "La fecha debe estar entre los años 2018 y 2050: {}",
                fecha_str
            ));
        }
    }

    // Validar hora HH:MM cuando se ingresa (opcional si se deja vacía)
    let hora_str = datos.hora.trim();
    if !hora_str.is_empty() {
        NaiveTime::parse_from_str(hora_str, "%H:%M")
            .map_err(|_| format!("Formato de hora inválido: {}. Use HH:MM (00:00 - 23:59)", hora_str))?;
    }

    // Validar motivo si NO se realizó
    if realizada == "NO" && mapear_motivo_a_indice(&datos.motivo).is_none() {
        return Err("Debe seleccionar un motivo si la audiencia NO se realizó".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapeo_motivos_columnas_exactas() {
        assert_eq!(mapear_motivo_a_indice("Juez"), Some(0));
        assert_eq!(mapear_motivo_a_indice("Fiscalía"), Some(1));
        assert_eq!(mapear_motivo_a_indice("Usuario"), Some(2));
        assert_eq!(mapear_motivo_a_indice("INPEC"), Some(3));
        assert_eq!(mapear_motivo_a_indice("Víctima"), Some(4));
        assert_eq!(mapear_motivo_a_indice("ICBF"), Some(5));
        assert_eq!(mapear_motivo_a_indice("Defensor Confianza"), Some(6));
        assert_eq!(mapear_motivo_a_indice("Defensor Público"), Some(7));
    }

    #[test]
    fn test_validacion_audiencia() {
        let mut input = AudienciaInput {
            radicado: "11001-60-00000-2025-00001-00".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "31/07/2025".into(),
            hora: "14:30".into(),
            juzgado: "Juzgado 1 Penal".into(),
            realizada: "NO".into(),
            motivo: "Fiscalía".into(),
            observaciones: "Sin novedad".into(),
        };
        assert!(validar_audiencia(&input, false).is_ok());

        input.hora = "".into();
        assert!(validar_audiencia(&input, false).is_ok());

        input.fecha = "".into();
        assert!(validar_audiencia(&input, false).is_ok());

        // Si se escribe una fecha u hora con formato inválido, sigue rechazándola
        input.fecha = "32/13/2025".into();
        assert!(validar_audiencia(&input, false).is_err());
        input.fecha = "".into();

        // Tipo de audiencia y juzgado son opcionales cuando el defensor no los menciona
        input.tipo = "".into();
        input.juzgado = "".into();
        assert!(validar_audiencia(&input, false).is_ok());
    }

    #[test]
    fn test_deteccion_duplicados_con_typos_y_radicado_numerico() {
        // Casos reales reportados por el usuario
        assert!(son_radicados_equivalentes(
            "JUAN ramírez - 1000231553000",
            "JuaN RamIres -   1000231553000   "
        ));
        assert!(son_radicados_equivalentes(
            "JUAN RAmirez - 1212312312312312345231121",
            "JuiAN RAmíres - 1212312312312312345231121"
        ));
        // Radicado con guiones vs compacto
        assert!(son_radicados_equivalentes(
            "11001-60-00000-2025-00001-00",
            "11001600000020250000100"
        ));
        // Solo nombres con typo sin número
        assert!(son_radicados_equivalentes("JUAN RAmirez", "JuiAN RAmíres"));

        // Mismo nombre pero números de radicado distintos -> NO son duplicados
        assert!(!son_radicados_equivalentes(
            "JUAN RAMIREZ - 110016000001",
            "JUAN RAMIREZ - 110016000002"
        ));
        // Nombres distintos con mismo primer nombre -> NO son duplicados
        assert!(!son_radicados_equivalentes("CARLOS PEREZ", "CARLOS GOMEZ"));

        // Solo alerta cuando coincide también la fecha
        let existente = vec![RegistroAudiencia {
            fila_excel: 11,
            numero: 1,
            radicado: "JUAN RAmirez - 1212312312312312345231121".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "15/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 1".into(),
            realizada: "SI".into(),
            motivo: "".into(),
            motivos_flags: [false; 8],
            observaciones: "".into(),
        }];

        let mismo_dia = AudienciaInput {
            radicado: "JuiAN RAmíres - 1212312312312312345231121".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "15/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 1".into(),
            realizada: "SI".into(),
            motivo: "".into(),
            observaciones: "".into(),
        };
        assert!(buscar_posible_duplicado(&existente, &mismo_dia, None).is_some());
        assert!(buscar_posible_duplicado(&existente, &mismo_dia, Some(11)).is_none());

        let otro_dia = AudienciaInput {
            fecha: "20/08/2025".into(),
            ..mismo_dia
        };
        assert!(buscar_posible_duplicado(&existente, &otro_dia, None).is_none());
    }

    #[test]
    fn test_claves_defensor_prefijo_dane_siglas_y_grafo() {
        assert_eq!(normalizar_clave_defensor("  Dra. María José  "), "maria jose");
        assert_eq!(normalizar_clave_defensor("dra maria jose"), "maria jose");
        assert_eq!(normalizar_clave_defensor("Dr. José Daniel"), "jose daniel");
        assert_eq!(normalizar_clave_defensor("JOSE DANIEL"), "jose daniel");
        assert_eq!(
            normalizar_clave_defensor("Defensor Público José Daniel"),
            "jose daniel"
        );
        assert_eq!(
            normalizar_clave_defensor("Defensora Pública: Dra. Ana Pérez"),
            "ana perez"
        );
        assert_eq!(
            normalizar_clave_defensor("Abg. Lic. Carlos Mora"),
            "carlos mora"
        );
        // Guardia: si tras quitar prefijo el resto queda vacío, conserva el token
        assert_eq!(normalizar_clave_defensor("Dr."), "dr");
        assert_eq!(normalizar_clave_defensor("Defensor Público"), "defensor publico");
        assert_eq!(normalizar_clave_defensor("   "), "");

        assert!(es_alcance_general(""));
        assert!(es_alcance_general("  *  "));
        assert!(!es_alcance_general("ana perez"));
        assert!(!coincide_alcance_defensor("*", "ana perez"));
        assert!(!coincide_alcance_defensor("", "ana perez"));
        assert!(!coincide_alcance_defensor("Dra. Ana Pérez", ""));
        assert!(coincide_alcance_defensor("Dra. Ana Pérez", "ana perez"));
        assert!(coincide_alcance_defensor(
            "Defensor Público José Daniel",
            "JOSE DANIEL"
        ));
        assert!(!coincide_alcance_defensor("Dra. Ana Pérez", "Dr. Luis Mora"));

        assert_eq!(
            extraer_prefijo_dane_radicado("PROCESADO - 200016001075202255547"),
            Some("20001".to_string())
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("47001-60-01086-2022-00419"),
            Some("47001".to_string())
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Radicado 20001 60 0107520130283900"),
            Some("20001".to_string())
        );
        // Un número celular de 10 dígitos, fechas o prosa con "60" no deben confundirse con prefijo DANE
        assert_eq!(
            extraer_prefijo_dane_radicado("Contactar al celular 3001234567 sin radicado"),
            None
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Oficio 12345 15/08/2025 audiencia reprogramada"),
            None
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Proceso 54321 60 minutos de espera sin conexión"),
            None
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Expediente 12345 12:00-14:00 sala virtual"),
            None
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Oficio 12345 3001234567 llamar"),
            None
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Radicado 20001 60 01075 2022 00123"),
            Some("20001".to_string())
        );
        assert_eq!(
            extraer_prefijo_dane_radicado("Radicado 20001 6001075202255547"),
            Some("20001".to_string())
        );

        assert_eq!(
            extraer_siglas_cortas_juzgado("J04 CONTROL F25"),
            vec!["j4".to_string()]
        );
        assert_eq!(
            extraer_siglas_cortas_juzgado("J10 Penal y Jdo 8"),
            vec!["j10".to_string(), "j8".to_string()]
        );
        assert_eq!(
            extraer_siglas_cortas_juzgado("Juzgado Segundo Penal del Circuito de Valledupar"),
            vec!["j2".to_string()]
        );
        assert_eq!(
            extraer_siglas_cortas_juzgado("Juzgado 4º Penal Municipal y Juzgado 1ra"),
            vec!["j4".to_string(), "j1".to_string()]
        );
        assert!(extraer_siglas_cortas_juzgado("El juez aplaza por F25").is_empty());

        // Solo siglas desnudas se expanden como bare code; nunca calificadores CTO/MPAL/Circuito
        assert_eq!(es_solo_sigla_corta_juzgado("J04"), Some("j4".to_string()));
        assert_eq!(es_solo_sigla_corta_juzgado("Jdo. 4"), Some("j4".to_string()));
        assert_eq!(es_solo_sigla_corta_juzgado("Juzgado 10"), Some("j10".to_string()));
        assert_eq!(es_solo_sigla_corta_juzgado("J4 CTO"), None);
        assert_eq!(es_solo_sigla_corta_juzgado("J4 Circuito"), None);
        assert_eq!(es_solo_sigla_corta_juzgado("J4 Penal"), None);

        // Detección de instalación efectiva vs negaciones en español judicial
        assert!(texto_indica_instalacion_efectiva(
            "se instalo la audiencia pero se suspendio"
        ));
        assert!(texto_indica_instalacion_efectiva(
            "la fiscal no vino pero la audiencia se instalo y se reprogramo"
        ));
        assert!(!texto_indica_instalacion_efectiva(
            "no se instalo la diligencia y se reprogramo"
        ));
        assert!(!texto_indica_instalacion_efectiva(
            "la audiencia no pudo ser instalada por ausencia del juez"
        ));
        assert!(!texto_indica_instalacion_efectiva(
            "nunca se instalo y no quedo instalada por falla"
        ));
        assert!(!texto_indica_instalacion_efectiva(
            "no ha sido instalada la audiencia"
        ));
        assert!(texto_indica_no_instalacion(
            "no se instalo la diligencia y se fija para el 10/09/2025"
        ));
        assert!(texto_indica_no_instalacion(
            "no pudo ser instalada por inasistencia del despacho"
        ));
        assert!(texto_indica_no_instalacion(
            "sin instalarse por fallas tecnicas"
        ));
        assert!(!texto_indica_no_instalacion(
            "se instalo la audiencia y se suspendio"
        ));

        let mut grafo = GrafoAsociaciones::default();
        grafo
            .defensor_a_juzgados
            .entry("dra ana".into())
            .or_default()
            .insert("Juzgado 4 Penal".into(), 2);
        grafo
            .defensor_a_tipos
            .entry("dra ana".into())
            .or_default()
            .insert("Audiencia preparatoria".into(), 2);
        grafo
            .defensor_sigla_a_juzgado
            .entry("dra ana".into())
            .or_default()
            .entry("j4".into())
            .or_default()
            .insert("Juzgado 4 Penal".into(), 2);
        grafo
            .prefijo_radicado_a_juzgados
            .entry("20001".into())
            .or_default()
            .insert("Juzgado 4 Penal".into(), 2);
        grafo
            .tipo_a_juzgados
            .entry("audiencia preparatoria".into())
            .or_default()
            .insert("Juzgado 4 Penal".into(), 2);
        assert_eq!(grafo.total_conexiones(), 5);
    }
}
