use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::aprendizaje::{
    cargar_memoria_local_snapshot, memoria_base_embebida,
    obtener_contexto_prompt_con_traza_y_defensor, obtener_resumenes_consolidados_prompt,
    preferencias_efectivas_con_base_para_defensor, quitar_tildes, AnalisisCorreccionIa,
    CorreccionIa, LoteConsolidacion, MemoriaAprendizaje, PrediccionLocalResumen,
    PreferenciasEstilo, ResultadoConsolidacionIa,
};
use crate::config::{cargar_settings, guardar_settings};
use crate::estadistica_ia::{
    juzgados_base, resolver_alias_exacto_validado_con_base_para_defensor, resolver_hora_con_prior,
    seleccionar_candidatos_prompt_con_base_para_defensor, CandidatosPrompt,
    MotorEstadisticoLocal,
};
use crate::excel::cargar_priors_actuales;
use crate::models::{
    es_solo_sigla_corta_juzgado, extraer_numeros_radicado, extraer_siglas_cortas_juzgado,
    mapear_motivo_a_indice, normalizar_clave_catalogo, normalizar_texto_sin_tildes,
    texto_indica_instalacion_efectiva, texto_indica_no_instalacion, IndicePriorsExcel,
    MOTIVOS_LABELS, TIPOS_AUDIENCIA,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiAudienciaOutput {
    pub radicado: String,
    pub tipo_audiencia: String,
    pub tipo_otra: String,
    pub fecha: String,
    pub hora: String,
    pub minuto: String,
    pub juzgado: String,
    pub se_realizo: String,
    pub motivo: String,
    pub observaciones: String,
    #[serde(default)]
    pub reglas_inyectadas: Vec<String>,
    #[serde(default)]
    pub ejemplos_inyectados: Vec<String>,
    #[serde(default)]
    pub prediccion_local: Option<PrediccionLocalResumen>,
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn formatear_error_red(proveedor: &str, err: reqwest::Error) -> String {
    let sin_url = err.without_url();
    if sin_url.is_timeout() {
        format!(
            "Tiempo de espera agotado al consultar {} (45s). Intente nuevamente.",
            proveedor
        )
    } else if sin_url.is_connect() {
        format!(
            "No se pudo conectar con {}. Verifique su conexión a internet.",
            proveedor
        )
    } else {
        format!("Error de conexión con {}: {}", proveedor, sin_url)
    }
}

pub fn normalizar_fecha_ia(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }

    // %d y %m en chrono también aceptan 1 dígito ("5/8/2025")
    for fmt in ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%Y/%m/%d", "%d/%m/%y", "%d-%m-%y"] {
        if let Ok(fecha) = NaiveDate::parse_from_str(s, fmt) {
            return fecha.format("%d/%m/%Y").to_string();
        }
    }

    s.to_string()
}

fn normalizar_hora_minuto_ia(raw_hora: &str, raw_minuto: &str) -> (String, String) {
    let h_str = raw_hora.trim();
    let m_str = raw_minuto.trim();
    if h_str.is_empty() && m_str.is_empty() {
        return (String::new(), String::new());
    }

    let Ok(h) = h_str.parse::<u32>() else {
        return (String::new(), String::new());
    };
    if h > 23 {
        return (String::new(), String::new());
    }

    let m = if m_str.is_empty() {
        0
    } else {
        match m_str.parse::<u32>() {
            Ok(val) if val <= 59 => val,
            _ => return (String::new(), String::new()),
        }
    };

    (format!("{:02}", h), format!("{:02}", m))
}

fn expandir_abreviaciones_y_compacto(texto: &str, current_year: i32) -> String {
    let mut out = texto.to_string();

    let reemplazos = [
        // Nota de legacy-python: 'CTO' NO se expande porque puede ser Circuito o Conocimiento
        (r"(?i)\bFRACSA\b", "Fracasó"),
        (r"(?i)\bFRACASA\b", "Fracasó"),
        (r"(?i)\bFRCASA\b", "Fracasó"),
        (r"(?i)\bDEPACHO\b", "Despacho"),
        (r"(?i)\bDESPCHO\b", "Despacho"),
        (r"(?i)\bCONOCIEMIENTO\b", "Conocimiento"),
        (r"(?i)\bVALLEDUPARA\b", "Valledupar"),
        (r"(?i)\bSOLCITO\b", "Solicitó"),
        (r"(?i)\bREPRGRAMACION\b", "Reprogramación"),
        (r"(?i)\bAPALZAMIENTO\b", "Aplazamiento"),
        (r"(?i)\bDILIGNCIA\b", "Diligencia"),
        (r"(?i)\bDOILIGENCA\b", "Diligencia"),
        (r"(?i)\bEZADO\b", "Especializado"),
        (r"(?i)\bEZADA\b", "Especializada"),
        (r"(?i)\bGRAVADO\b", "Agravado"),
        (r"(?i)\bAGRVADO\b", "Agravado"),
        (r"(?i)\bREEMPLAZDAO\b", "Reemplazado"),
        (r"(?i)\bREPROGRAMARA\b", "Reprogramará"),
        (r"(?i)\bAUDIEN\b", "Audiencia"),
        (r"(?i)\bHUBICARSE\b", "Ubicarse"),
        (r"(?i)\bTESTI\b", "Testigos"),
        (r"(?i)\bPREPARATOR\b", "Preparatoria"),
        (r"(?i)\bPREPARATORI\b", "Preparatoria"),
        (r"(?i)\bINSTLO\b", "Instaló"),
        (r"(?i)\bGENDA\b", "Agenda"),
        (r"(?i)\bCONTINUCION\b", "Continuación"),
        (r"(?i)\bNO CONEXION\b", "No se pudo establecer conexión"),
        (
            r"(?i)\bPROBLEMAS DE CONEXION\b",
            "Se presentaron problemas técnicos de conexión",
        ),
        (
            r"(?i)\bAMBOS DESPACHOS EN AUDIENCIA\b",
            "Ambos despachos estaban ocupados en otras audiencias",
        ),
        (
            r"(?i)\bERROR EN LA GENDA\b",
            "Error en la agenda del despacho",
        ),
        (r"(?i)\bSE REPROGRAMA\b", "Se reprogramó la audiencia"),
        (
            r"(?i)\bSERA REPROGRAMADA\b",
            "La audiencia será reprogramada",
        ),
        (r"(?i)\bSE FIJO\b", "Se fijó nueva fecha"),
        (r"(?i)\bSIN NOVEDAD\b", "Se realizó sin novedad"),
        (r"(?i)\bSE EFECTUO\b", "Se efectuó correctamente"),
    ];

    for (pat, rep) in reemplazos {
        if let Ok(re) = Regex::new(pat) {
            out = re.replace_all(&out, rep).to_string();
        }
    }

    // Formatos de hora con puntuación irregular ("HORA. 12:00", "HORA; 8:00", "HORA: 2.30")
    if let Ok(re_hora) = Regex::new(r"(?i)\bHORA\s*[.;:]\s*(\d{1,2})\s*[.:]\s*(\d{2})\b") {
        out = re_hora.replace_all(&out, "HORA: $1:$2").to_string();
    }

    // Fechas compactas tipo "JUNIO 24.-"
    if let Ok(re_fecha) = Regex::new(
        r"(?i)\b(ENERO|FEBRERO|MARZO|ABRIL|MAYO|JUNIO|JULIO|AGOSTO|SEPTIEMBRE|OCTUBRE|NOVIEMBRE|DICIEMBRE)\s+(\d{1,2})\.-",
    ) {
        out = re_fecha
            .replace_all(&out, |caps: &regex::Captures| {
                let mes = caps[1].to_lowercase();
                let dia = &caps[2];
                format!("{} de {} de {}", dia, mes, current_year)
            })
            .to_string();
    }

    out
}

fn detectar_defensor_confianza(texto_lower: &str) -> bool {
    let indicadores = [
        "defensor de confianza",
        "abogado contractual",
        "abogado privado",
        "defensor privado",
        "el abogado",
        "su abogado",
        "abogado del procesado",
        "abogado defensor",
        "defensor de confianza inasistente",
        "abogado contractual inasistente",
        "defensor confianza inasistente",
        "inasistente defensor de confianza",
        "inasistente abogado contractual",
        "def confianza",
        "def. confianza",
        "def de confianza",
        "defensor confianza",
    ];
    indicadores.iter().any(|ind| texto_lower.contains(ind))
}

fn detectar_primera_persona_defensor_publico(texto_norm: &str) -> bool {
    let patrones = [
        "solicite",
        "pedi",
        "solicito",
        "pido",
        "solicire",
        "no pude",
        "no he tenido",
        "no logre",
        "me fue imposible",
        "no he logrado",
        "no habia podido",
        "realice conexion",
    ];
    patrones.iter().any(|p| texto_norm.contains(p))
}

fn inferir_motivos(texto_original: &str) -> (String, Vec<String>) {
    let texto_norm = quitar_tildes(&texto_original.to_lowercase());
    let es_confianza = detectar_defensor_confianza(&texto_norm);
    let es_primera_persona = detectar_primera_persona_defensor_publico(&texto_norm);

    let frases_motivos: &[(&str, &str)] = &[
        // Juez (mayor prioridad en problemas del despacho)
        ("el juez entro en periodo de vacaciones", "Juez"),
        ("vacaciones", "Juez"),
        ("no habian nombrado titular de despacho", "Juez"),
        ("titular de despacho", "Juez"),
        ("juez no comparecio", "Juez"),
        ("ausencia del juez", "Juez"),
        ("juzgado cerrado", "Juez"),
        ("juez en comision", "Juez"),
        ("despacho en otra audiencia", "Juez"),
        ("depacho en otra audiencia", "Juez"),
        ("estar el despacho en audiencia", "Juez"),
        ("fracsa por estar el despacho", "Juez"),
        ("fracasa por estar el despacho", "Juez"),
        ("ambos despachos en audiencia", "Juez"),
        ("error en la agenda", "Juez"),
        // Fiscalía
        ("la fiscal no se conecto", "Fiscalía"),
        ("fiscal no se conecto", "Fiscalía"),
        ("fiscal fue reemplazad", "Fiscalía"),
        ("fiscal fue reemplazdao", "Fiscalía"),
        ("no conocer la carpeta", "Fiscalía"),
        ("por solicitud de la fiscalia", "Fiscalía"),
        ("solicitud de fiscalia", "Fiscalía"),
        ("solicitud de la fiscalia", "Fiscalía"),
        ("fiscalia no comparecio", "Fiscalía"),
        ("la fiscalia solicito reprogramacion", "Fiscalía"),
        ("fiscal solicito reprogramacion", "Fiscalía"),
        ("fiscalia solicito reprogramacion", "Fiscalía"),
        ("la fiscal no vino", "Fiscalía"),
        ("fiscal no vino", "Fiscalía"),
        ("la fiscal no asistio", "Fiscalía"),
        ("fiscal no asistio", "Fiscalía"),
        ("ausencia de la fiscal", "Fiscalía"),
        ("ausencia del fiscal", "Fiscalía"),
        ("posible preacuerdo", "Fiscalía"),
        // Defensor Confianza (abogado contractual / privado)
        ("defensor de confianza no comparecio", "Defensor Confianza"),
        ("abogado no comparecio", "Defensor Confianza"),
        ("abogado contractual no comparecio", "Defensor Confianza"),
        ("abogado contractual no se presento", "Defensor Confianza"),
        ("abogado contractual ausente", "Defensor Confianza"),
        ("defensor de confianza ausente", "Defensor Confianza"),
        ("inasistencia del defensor de confianza", "Defensor Confianza"),
        ("inasistencia del abogado contractual", "Defensor Confianza"),
        ("inasistencia del abogado", "Defensor Confianza"),
        ("defensor de confianza solicito", "Defensor Confianza"),
        ("defensor de confianza solicita", "Defensor Confianza"),
        ("abogado contractual solicito", "Defensor Confianza"),
        ("abogado contractual pidio", "Defensor Confianza"),
        ("abogado solicito reprogramacion", "Defensor Confianza"),
        ("el abogado privado solicito", "Defensor Confianza"),
        ("el abogado no se presento", "Defensor Confianza"),
        ("el abogado del procesado no comparecio", "Defensor Confianza"),
        ("falta del defensor de confianza", "Defensor Confianza"),
        ("falta del abogado contractual", "Defensor Confianza"),
        ("ausencia de defensor de confianza", "Defensor Confianza"),
        ("ausencia de abogado contractual", "Defensor Confianza"),
        ("defensor privado no comparecio", "Defensor Confianza"),
        ("abogado privado no comparecio", "Defensor Confianza"),
        ("solicitud del abogado contractual", "Defensor Confianza"),
        ("defensor de confianza inasistente", "Defensor Confianza"),
        ("abogado contractual inasistente", "Defensor Confianza"),
        ("defensor confianza inasistente", "Defensor Confianza"),
        ("inasistente defensor de confianza", "Defensor Confianza"),
        ("inasistente abogado contractual", "Defensor Confianza"),
        ("defensor de confianza", "Defensor Confianza"),
        ("abogado contractual", "Defensor Confianza"),
        ("abogado privado", "Defensor Confianza"),
        ("def. confianza", "Defensor Confianza"),
        ("def confianza", "Defensor Confianza"),
        ("def de confianza", "Defensor Confianza"),
        ("defensor confianza", "Defensor Confianza"),
        // Defensor Público (incluye primera persona del defensor)
        ("defensor publico no comparecio", "Defensor Público"),
        ("inasistencia del defensor publico", "Defensor Público"),
        ("defensor publico solicito", "Defensor Público"),
        ("solicite reprogram", "Defensor Público"),
        ("solicire reprogram", "Defensor Público"),
        ("solicito reprogram", "Defensor Público"),
        ("solicito la reprogram", "Defensor Público"),
        ("pedi reprogram", "Defensor Público"),
        ("pedi la reprogram", "Defensor Público"),
        ("pido reprogram", "Defensor Público"),
        ("pido la reprogram", "Defensor Público"),
        ("pedi aplazamiento", "Defensor Público"),
        ("no pude contactar", "Defensor Público"),
        ("no he tenido contacto", "Defensor Público"),
        ("no habia tenido contacto con el procesado", "Defensor Público"),
        ("no he logrado contactar", "Defensor Público"),
        ("me fue imposible", "Defensor Público"),
        ("no logre comunicarme", "Defensor Público"),
        ("realice conexion virtual", "Defensor Público"),
        // Usuario / Procesado
        ("no notificaron al procesado", "Usuario"),
        ("procesado no comparecio", "Usuario"),
        ("usuario no se presento", "Usuario"),
        ("procesado ausente", "Usuario"),
        ("problemas tecnicos", "Usuario"),
        ("fallas de conectividad", "Usuario"),
        // INPEC
        ("inpec no traslado", "INPEC"),
        ("problemas de traslado", "INPEC"),
        ("inpec reporto", "INPEC"),
        ("inpec", "INPEC"),
        // Víctima
        ("victima no comparecio", "Víctima"),
        ("ausencia de la victima", "Víctima"),
        // ICBF
        ("icbf no comparecio", "ICBF"),
        ("representante icbf ausente", "ICBF"),
        ("icbf", "ICBF"),
    ];

    let mut encontrados: Vec<String> = Vec::new();
    for (frase, motivo) in frases_motivos {
        if texto_norm.contains(frase) && !encontrados.contains(&motivo.to_string()) {
            encontrados.push(motivo.to_string());
        }
    }

    // Patrones genéricos de reprogramación que se reasignan según el contexto (de legacy-python)
    let frases_genericas = [
        "solicita reprogramacion",
        "solicito la reprogramacion",
        "reprogramacion solicitada",
        "solicito reprogramar",
    ];
    if encontrados.is_empty() && frases_genericas.iter().any(|f| texto_norm.contains(f)) {
        if es_confianza {
            encontrados.push("Defensor Confianza".to_string());
        } else if es_primera_persona {
            encontrados.push("Defensor Público".to_string());
        } else {
            encontrados.push("Usuario".to_string());
        }
    }

    if encontrados.is_empty() {
        if es_confianza {
            encontrados.push("Defensor Confianza".to_string());
        } else if es_primera_persona {
            encontrados.push("Defensor Público".to_string());
        }
    }

    if encontrados.is_empty() {
        return (String::new(), Vec::new());
    }

    // Priorizar causas principales sobre reacciones si ambas están presentes
    let causas_principales: Vec<String> = encontrados
        .iter()
        .filter(|m| {
            matches!(
                m.as_str(),
                "Juez" | "Fiscalía" | "Usuario" | "INPEC" | "Víctima" | "ICBF"
            )
        })
        .cloned()
        .collect();
    let reacciones: Vec<String> = encontrados
        .iter()
        .filter(|m| matches!(m.as_str(), "Defensor Público" | "Defensor Confianza"))
        .cloned()
        .collect();

    if !causas_principales.is_empty() && !reacciones.is_empty() {
        let principal = causas_principales[0].clone();
        let mut secundarios: Vec<String> = causas_principales[1..].to_vec();
        secundarios.extend(reacciones);
        return (principal, secundarios);
    }

    let principal = encontrados[0].clone();
    let secundarios = encontrados[1..].to_vec();
    (principal, secundarios)
}

fn mapear_tipo_audiencia_con_usuario(tipo_ia: &str, tipos_usuario: &[String]) -> (String, String) {
    let limpio_raw = limpiar_valor_opcional_vacio(tipo_ia);
    let limpio = limpio_raw.trim();
    if limpio.is_empty() {
        return (String::new(), String::new());
    }
    let norm = normalizar_clave_catalogo(limpio);
    if norm.is_empty() || norm == "otra" {
        return (String::new(), String::new());
    }

    for &oficial in &TIPOS_AUDIENCIA {
        if oficial != "Otra" && normalizar_clave_catalogo(oficial) == norm {
            return (oficial.to_string(), String::new());
        }
    }

    for usr in tipos_usuario {
        if normalizar_clave_catalogo(usr) == norm {
            return (usr.clone(), String::new());
        }
    }

    let mapeo_directo = [
        ("alegatos de conclusion", "Alegatos de conclusión"),
        ("control de legalidad", "Audiencia de control de legalidad"),
        ("concentrada", "Audiencia concentrada"),
        ("juicio oral", "Audiencia de juicio oral"),
        ("inicia juicio", "Audiencia de juicio oral"),
        ("continua juicio", "Audiencia de juicio oral"),
        ("preliminar", "Audiencia preliminar"),
        ("preparatoria", "Audiencia preparatoria"),
        ("acusacion", "Audiencia de acusación"),
        ("conciliacion", "Audiencia de conciliación"),
        ("preclusion", "Audiencia de preclusión"),
        (
            "individualizacion de pena",
            "Audiencia de individualización de pena",
        ),
        ("imputacion", "Audiencia de imputación"),
        (
            "reparacion integral",
            "Audiencia de incidente de reparación integral",
        ),
        (
            "medidas de aseguramiento",
            "Audiencia de medidas de aseguramiento",
        ),
        (
            "medida de aseguramiento",
            "Audiencia de medidas de aseguramiento",
        ),
        ("nulidad", "Audiencia de nulidad"),
        ("prorroga", "Audiencia de prórroga"),
        ("revision de medida", "Audiencia de revisión de medida"),
        (
            "verificacion de cumplimiento",
            "Audiencia de verificación de cumplimiento",
        ),
    ];

    for (clave, oficial) in mapeo_directo {
        if norm.contains(clave) {
            return (oficial.to_string(), String::new());
        }
    }

    ("Otra".to_string(), limpio.to_string())
}

fn normalizar_juzgado_catalogo(juzgado_ia: &str, juzgados_usuario: &[String]) -> String {
    let limpio = limpiar_valor_opcional_vacio(juzgado_ia);
    if limpio.is_empty() {
        return String::new();
    }
    let norm = normalizar_clave_catalogo(&limpio);
    if norm.is_empty() || norm == "otra" {
        return String::new();
    }
    if let Some(usr) = juzgados_usuario
        .iter()
        .find(|j| normalizar_clave_catalogo(j) == norm)
    {
        return usr.clone();
    }
    if let Some(oficial) = juzgados_base()
        .iter()
        .find(|j| normalizar_clave_catalogo(j) == norm)
    {
        return oficial.clone();
    }
    limpio
}

fn extraer_fechas_reprogramacion(texto: &str) -> String {
    let patrones = [
        r"(?i)reprogram(?:ad[oa]?|ar)\s+para\s+el?\s+d[íi]a\s+(\d{1,2}\s+de\s+[a-záéíóú]+(?:\s+de\s+\d{4})?)",
        r"(?i)reprogram(?:ad[oa]?|ar)\s+para\s+el?\s+(\d{1,2}\s+de\s+[a-záéíóú]+(?:\s+de\s+\d{4})?)",
        r"(?i)nueva\s+fecha[:\s]+(\d{1,2}/\d{1,2}/\d{2,4})",
        r"(?i)se\s+fij[óo]\s+(?:nueva\s+fecha\s+)?para\s+el?\s+(\d{1,2}\s+de\s+[a-záéíóú]+(?:\s+de\s+\d{4})?)",
    ];
    let mut halladas: Vec<String> = Vec::new();
    for pat in patrones {
        if let Ok(re) = Regex::new(pat) {
            for caps in re.captures_iter(texto) {
                if let Some(m) = caps.get(1) {
                    let f = m.as_str().trim().to_string();
                    if !f.is_empty() && !halladas.contains(&f) {
                        halladas.push(f);
                    }
                }
            }
        }
    }
    halladas.join(", ")
}

fn re_ningun_motivo_contextual() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:motivo(?:\s*\([^)]*\))?\s*[:\-]\s*(?:ningun[oa]|n/?a|no\s+aplica)\b|\bsin\s+motivo\s+de\s+(?:aplazamiento|suspensi[oó]n)\b)",
        )
        .ok()
    })
    .as_ref()
}

fn detectar_dice_ningun_motivo(texto_lower: &str) -> bool {
    re_ningun_motivo_contextual().is_some_and(|re| re.is_match(texto_lower))
}

#[cfg(test)]
fn detectar_patron_fracaso_contextual(texto_lower: &str) -> bool {
    detectar_patron_fracaso_con_preferencias(texto_lower, &PreferenciasEstilo::default())
}

fn detectar_patron_fracaso_con_preferencias(
    texto_lower: &str,
    pref: &PreferenciasEstilo,
) -> bool {
    let texto_norm = quitar_tildes(texto_lower);

    // Negación explícita de instalación ("no se instaló", "no pudo ser instalada", "sin instalarse")
    // indica inequívocamente que la audiencia NO se realizó.
    if texto_indica_no_instalacion(&texto_norm) {
        return true;
    }

    // Si la audiencia efectivamente se instaló/inició (sin negación como "no se instaló")
    // y luego se reprogramó/suspendió, respetamos el criterio aprendido del defensor si prefiere SI.
    let hubo_instalacion = texto_indica_instalacion_efectiva(&texto_norm);
    if hubo_instalacion {
        let denom_inst =
            (pref.instalada_reprogramada_si_alpha + pref.instalada_reprogramada_si_beta).max(1.0);
        let p_instalada_si = pref.instalada_reprogramada_si_alpha / denom_inst;
        if p_instalada_si >= 0.50 {
            return false;
        }
    }

    let patrones_fracaso = [
        "se reprogramo",
        "sera reprogramada",
        "se reprograma",
        "reprogramacion",
        "reprgramacion",
        "solicito reprogramacion",
        "pide reprogramacion",
        "solicita aplazamiento",
        "apalzamiento",
        "se aplaza",
        "sera aplazada",
        "se suspende",
        "suspendida",
        "nueva fecha",
        "se fijo nueva fecha",
        "se fija nueva fecha",
        "se fija fecha de reprogramacion",
        "fracsa",
        "fracasa",
        "no se realizo",
        "no se hizo",
    ];

    if patrones_fracaso.iter().any(|p| texto_norm.contains(p))
        || (texto_norm.contains("fracaso")
            && !texto_norm.contains("no fracaso")
            && !texto_norm.contains("sin fracaso"))
    {
        return true;
    }

    // "se fija" ambiguo es adaptable estadísticamente según PreferenciasEstilo:
    // si el defensor suele marcarlo como realización/continuación, su probabilidad baja de 0.45.
    if texto_norm.contains("se fija") || texto_norm.contains("se fijo") {
        let senales_realizacion = [
            "se realizo",
            "se efectuo",
            "se llevo a cabo",
            "se emitio sentido de fallo",
            "se dio lectura",
            "se fija lectura",
            "se fijo lectura",
            "para lectura",
            "para continuar",
            "sin novedad",
        ];
        if !hubo_instalacion && !senales_realizacion.iter().any(|s| texto_norm.contains(s)) {
            let denom =
                (pref.se_fija_aplazamiento_alpha + pref.se_fija_aplazamiento_beta).max(1.0);
            let p_aplaza = pref.se_fija_aplazamiento_alpha / denom;
            if p_aplaza >= 0.45 {
                return true;
            }
        }
    }

    false
}

pub fn limpiar_valor_opcional_vacio(raw: &str) -> String {
    let limpio = raw.trim();
    if limpio.is_empty() {
        return String::new();
    }
    if limpio
        .chars()
        .all(|c| matches!(c, '-' | '–' | '—' | '.' | '_' | '/'))
    {
        return String::new();
    }
    let norm = normalizar_texto_sin_tildes(limpio);
    if norm.is_empty()
        || matches!(
            norm.as_str(),
            "no especificado"
                | "no especificada"
                | "no aplica"
                | "n a"
                | "na"
                | "ninguno"
                | "ninguna"
                | "ningun"
                | "sin especificar"
                | "sin informacion"
                | "sin datos"
                | "sin dato"
                | "desconocido"
                | "desconocida"
                | "no mencionado"
                | "no mencionada"
                | "no indica"
                | "no se indica"
                | "no se menciona"
                | "no se especifica"
                | "no disponible"
                | "no consta"
                | "por definir"
                | "no registra"
                | "sin registro"
                | "sin hora"
                | "sin fecha"
                | "sin juzgado"
                | "sin tipo"
                | "sin motivo"
                | "sin radicado"
                | "sin observaciones"
                | "sin observaciones adicionales"
                | "ninguna observacion"
                | "s n"
                | "s i"
                | "n r"
                | "vacio"
                | "vacia"
                | "null"
                | "none"
                | "empty"
        )
    {
        String::new()
    } else {
        limpio.to_string()
    }
}

fn limpiar_observacion_vacia(obs: String) -> String {
    limpiar_valor_opcional_vacio(&obs)
}

fn formatear_candidatos_prompt(cands: Option<&CandidatosPrompt>) -> String {
    let Some(c) = cands else {
        return String::new();
    };
    let mut bloques: Vec<String> = Vec::new();
    if let Some(ref proc_hab) = c.radicado_vinculado {
        bloques.push(format!(
            "- Proceso conocido por historial del radicado (frecuencia={}): radicado=\"{}\", juzgado_habitual=\"{}\", tipo_habitual=\"{}\". Úsalos como referencia de normalización salvo que el texto indique un cambio explícito.",
            proc_hab.frecuencia,
            proc_hab.radicado_completo,
            proc_hab.juzgado_habitual,
            proc_hab.tipo_habitual
        ));
    }
    if !c.siglas_resueltas.is_empty() {
        let pares: Vec<String> = c
            .siglas_resueltas
            .iter()
            .map(|(sigla, juzgado)| format!("{} => {}", sigla.to_uppercase(), juzgado))
            .collect();
        bloques.push(format!(
            "- Siglas habituales de este defensor (Mapa de Conexiones): {}",
            pares.join(" | ")
        ));
    }
    if !c.juzgados_top.is_empty() {
        bloques.push(format!(
            "- Juzgados candidatos más probables (normaliza hacia la forma exacta si coincide, respetando siglas como CTO/MPAL/PCTO): {}",
            c.juzgados_top.join(" | ")
        ));
    }
    if !c.tipos_top.is_empty() {
        bloques.push(format!(
            "- Tipos de audiencia candidatos del catálogo: {}",
            c.tipos_top.join(" | ")
        ));
    }
    if bloques.is_empty() {
        String::new()
    } else {
        format!(
            "\nCANDIDATOS ESTADÍSTICOS LOCALES (PRIOR DE PLANILLAS):\n{}\n",
            bloques.join("\n")
        )
    }
}

fn construir_catalogo_tipos_para_prompt(tipos_usuario: &[String]) -> String {
    let mut vistos: Vec<String> = Vec::new();
    let mut claves: std::collections::HashSet<String> = std::collections::HashSet::new();
    for &t in &TIPOS_AUDIENCIA {
        if t == "Otra" {
            continue;
        }
        let k = normalizar_clave_catalogo(t);
        if claves.insert(k) {
            vistos.push(format!("- {}", t));
        }
    }
    let mut personalizados: Vec<String> = Vec::new();
    for t in tipos_usuario {
        let limpio = t.split_whitespace().collect::<Vec<_>>().join(" ");
        let k = normalizar_clave_catalogo(&limpio);
        if !k.is_empty() && k != "otra" && claves.insert(k) {
            personalizados.push(format!("- {}", limpio));
        }
    }
    if !personalizados.is_empty() {
        vistos.push("Tipos personalizados guardados por el usuario:".to_string());
        vistos.extend(personalizados);
    }
    vistos.join("\n")
}

#[allow(dead_code)]
fn construir_prompt(
    texto_preprocesado: &str,
    current_year: i32,
    contexto_aprendizaje: &str,
) -> String {
    construir_prompt_con_candidatos(texto_preprocesado, current_year, contexto_aprendizaje, None)
}

#[allow(dead_code)]
fn construir_prompt_con_candidatos(
    texto_preprocesado: &str,
    current_year: i32,
    contexto_aprendizaje: &str,
    candidatos: Option<&CandidatosPrompt>,
) -> String {
    construir_prompt_con_candidatos_y_catalogo(
        texto_preprocesado,
        current_year,
        contexto_aprendizaje,
        candidatos,
        &[],
    )
}

fn construir_prompt_con_candidatos_y_catalogo(
    texto_preprocesado: &str,
    current_year: i32,
    contexto_aprendizaje: &str,
    candidatos: Option<&CandidatosPrompt>,
    tipos_usuario: &[String],
) -> String {
    construir_prompt_con_candidatos_y_resumenes(
        texto_preprocesado,
        current_year,
        contexto_aprendizaje,
        candidatos,
        tipos_usuario,
        None,
        None,
    )
}

fn construir_prompt_con_candidatos_y_resumenes(
    texto_preprocesado: &str,
    current_year: i32,
    contexto_aprendizaje: &str,
    candidatos: Option<&CandidatosPrompt>,
    tipos_usuario: &[String],
    resumen_general: Option<&str>,
    resumen_defensor: Option<&str>,
) -> String {
    let seccion_candidatos = formatear_candidatos_prompt(candidatos);
    let catalogo_tipos = construir_catalogo_tipos_para_prompt(tipos_usuario);
    let seccion_aprendizaje = if contexto_aprendizaje.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", contexto_aprendizaje.trim())
    };
    let mut bloque_resumenes = String::new();
    if let Some(rg) = resumen_general.map(str::trim).filter(|s| !s.is_empty()) {
        bloque_resumenes.push_str(&format!(
            "\nCRITERIOS GENERALES CONSOLIDADOS (TRANSVERSALES):\n{}\n",
            rg
        ));
    }
    if let Some(rd) = resumen_defensor.map(str::trim).filter(|s| !s.is_empty()) {
        bloque_resumenes.push_str(&format!(
            "\nPERFIL SINTÉTICO DEL DEFENSOR (ESPECÍFICO):\n{}\n",
            rd
        ));
    }

    format!(
        r#"Eres un asistente jurídico experto en audiencias judiciales en Colombia, optimizado con patrones reales de defensores públicos.
Analiza el texto (que puede venir en estilo ESTRUCTURADO descriptivo o en estilo COMPACTO telegráfico) y devuelve ÚNICAMENTE un objeto JSON válido con esta estructura exacta:

{{
  "radicado": "NOMBRE_PROCESADO - NÚMERO_RADICADO (si hay ambos) o el identificador/radicado encontrado (o \"\" si no existe)",
  "tipo_audiencia": "Nombre exacto del catálogo de tipos de audiencia si corresponde, o el nombre real si es distinto (o \"\" si no se menciona)",
  "fecha": "DD/MM/YYYY (o \"\" si no se menciona)",
  "hora": "HH (formato 24 horas de 2 dígitos, ej: 08, 14, o \"\" si no se menciona hora)",
  "minuto": "MM (2 dígitos, ej: 00, 30, o \"\" si no se menciona hora)",
  "juzgado": "Nombre del juzgado o despacho judicial (o \"\" si no se menciona)",
  "se_realizo": "SI o NO (o \"\" si el texto no permite determinarlo)",
  "motivo": "Si se_realizo es NO, elegir SOLO UNO entre: Juez, Fiscalía, Usuario, INPEC, Víctima, ICBF, Defensor Confianza, Defensor Público. Si es SI, dejar \"\"",
  "observaciones": "Resumen breve y profesional de detalles relevantes, fechas de reprogramación o incidencias (o \"\" si no hay detalles adicionales)"
}}

CATÁLOGO COMPLETO DE TIPOS DE AUDIENCIA DEL FORMULARIO (elige el nombre EXACTO cuando coincida, o "" si el texto no indica tipo):
{catalogo_tipos}

REGLAS CRÍTICAS DE EXTRACCIÓN:
1. CAMPOS OPCIONALES AUSENTES:
   - Si un dato opcional (`tipo_audiencia`, `juzgado`, `fecha`, `hora`, `minuto`, `observaciones`) NO aparece en el texto, devuelve SIEMPRE cadena vacía `""`. Nunca escribas textos de relleno como "No especificado", "N/A", "Ninguno" ni "Sin datos".
2. RADICADO Y PROCESADO:
   - Busca números largos (19 a 23 dígitos, ej: "200016001075202255547") o formatos cortos ("2024-00236", "2019-00542").
   - Si hay varios procesados, inclúyelos (ej: "FERNANDO JULIO CASUIL Y SHARLES DE JESUS LOPEZ JIMENEZ - 20001...").
3. FECHA Y HORA MILITAR (24h):
   - Convierte cualquier fecha ("24 de junio", "JUNIO 24") a DD/MM/YYYY. Si no menciona el año, usa {current_year}.
   - Convierte horas PM a formato 24h ("09:30 am" -> hora: "09", minuto: "30"; "2:30 PM" -> hora: "14", minuto: "30"). Si no hay hora, deja "hora": "" y "minuto": "".
4. TIPOS DE AUDIENCIA (ESTILO ESTRUCTURADO Y COMPACTO):
   - "INICIA JUICIO" o "CONTINUA JUICIO" -> "Audiencia de juicio oral"
   - "ACUSACION" -> "Audiencia de acusación"
   - "PREPARATORIA" -> "Audiencia preparatoria"
   - Si corresponde a uno del CATÁLOGO COMPLETO anterior, devuélvelo con su nombre exacto (incluyendo tildes). Si es un tipo distinto no listado arriba, escribe su nombre real (nunca escribas "Otra").
5. REALIZACIÓN Y JERARQUÍA DE MOTIVOS:
   - FRACASO (SIEMPRE "se_realizo": "NO"): "se reprogramó", "será reprogramada", "se aplaza", "solicita aplazamiento", "se suspende", "se fija nueva fecha", "FRACSA", "FRACASA".
   - ÉXITO ("se_realizo": "SI", "motivo": ""): "se realizó con normalidad", "sin novedad", "se efectuó", "se emitió sentido de fallo", "se dio lectura", o si dice "¿Se realizó?: Sí" y la audiencia continuó pese a la ausencia de un tercero (incluso si se fija fecha posterior para continuar o leer fallo).
   - Si "se_realizo" es "NO", usa EXCLUSIVAMENTE uno de estos 8 valores en "motivo" (nunca pongas frases descriptivas en "motivo"):
     1) "Juez": despacho en otra audiencia, ambos despachos ocupados, juez en vacaciones/comisión, sin titular, error en la agenda.
     2) "Fiscalía": fiscal no se conectó / no compareció / no vino, fiscal reemplazado, no conoce la carpeta, solicitud de fiscalía, posible preacuerdo.
     3) "Defensor Público": primera persona ("solicité reprogramar", "pedí aplazamiento", "no pude contactar al procesado", "me fue imposible asistir") cuando NO exista una causa principal anterior (Juez/Fiscalía/INPEC/Usuario) que haya originado esa solicitud.
     4) "Defensor Confianza": abogado contractual, abogado privado, "def. confianza", "el abogado del procesado no compareció".
     5) "Usuario": procesado no compareció, no notificaron al procesado, usuario ausente, problemas técnicos del usuario.
     6) "INPEC": INPEC no trasladó al detenido, problemas de traslado.
     7) "Víctima": víctima no compareció.
     8) "ICBF": representante del ICBF no compareció.
   - Si concurren una causa principal (ej. Fiscalía no se conectó) y una reacción del defensor ("por eso solicité reprogramar"), el "motivo" principal es la causa ("Fiscalía") y la reacción va a "observaciones".
6. JUZGADOS Y ABREVIACIONES AMBIGUAS:
   - Para abreviaciones ambiguas como "CTO" / "Cto" (que puede significar Circuito o Conocimiento), MPAL o PCTO, MANTÉN la abreviación original tal cual ("Juzgado 3 Cto Penal" -> "Juzgado 3 Cto Penal"); no adivines su expansión.
7. OBSERVACIONES:
   - Incluye fechas de reprogramación, causas secundarias o incidencias técnicas. Si no hay detalles adicionales, deja el campo vacío "".
{bloque_resumenes}{seccion_candidatos}{seccion_aprendizaje}
TEXTO A ANALIZAR:
"""
{texto_preprocesado}
""""#
    )
}

pub(crate) fn esquema_extraccion_audiencia_openai() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "audiencia_judicial",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "radicado": { "type": "string" },
                    "tipo_audiencia": { "type": "string" },
                    "fecha": { "type": "string" },
                    "hora": { "type": "string" },
                    "minuto": { "type": "string" },
                    "juzgado": { "type": "string" },
                    "se_realizo": { "type": "string", "enum": ["SI", "NO", ""] },
                    "motivo": {
                        "type": "string",
                        "enum": [
                            "",
                            "Juez",
                            "Fiscalía",
                            "Usuario",
                            "INPEC",
                            "Víctima",
                            "ICBF",
                            "Defensor Confianza",
                            "Defensor Público"
                        ]
                    },
                    "observaciones": { "type": "string" }
                },
                "required": [
                    "radicado",
                    "tipo_audiencia",
                    "fecha",
                    "hora",
                    "minuto",
                    "juzgado",
                    "se_realizo",
                    "motivo",
                    "observaciones"
                ],
                "additionalProperties": false
            }
        }
    })
}

pub(crate) fn esquema_extraccion_audiencia_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "radicado": { "type": "STRING" },
            "tipo_audiencia": { "type": "STRING" },
            "fecha": { "type": "STRING" },
            "hora": { "type": "STRING" },
            "minuto": { "type": "STRING" },
            "juzgado": { "type": "STRING" },
            "se_realizo": { "type": "STRING", "enum": ["SI", "NO", ""] },
            "motivo": {
                "type": "STRING",
                "enum": [
                    "",
                    "Juez",
                    "Fiscalía",
                    "Usuario",
                    "INPEC",
                    "Víctima",
                    "ICBF",
                    "Defensor Confianza",
                    "Defensor Público"
                ]
            },
            "observaciones": { "type": "STRING" }
        },
        "required": [
            "radicado",
            "tipo_audiencia",
            "fecha",
            "hora",
            "minuto",
            "juzgado",
            "se_realizo",
            "motivo",
            "observaciones"
        ]
    })
}

fn esquema_analisis_openai() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "analisis_correccion_ia",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "es_error_de_ia": { "type": "boolean" },
                    "tipo_error": {
                        "type": "string",
                        "enum": [
                            "abreviacion",
                            "fecha",
                            "hora",
                            "realizacion",
                            "motivo",
                            "tipo",
                            "juzgado",
                            "estilo_observaciones",
                            "otro"
                        ]
                    },
                    "regla": { "type": "string" },
                    "confianza": { "type": "number" },
                    "necesita_aclaracion": { "type": "boolean" },
                    "pregunta": { "type": "string" }
                },
                "required": [
                    "es_error_de_ia",
                    "tipo_error",
                    "regla",
                    "confianza",
                    "necesita_aclaracion",
                    "pregunta"
                ],
                "additionalProperties": false
            }
        }
    })
}

fn esquema_analisis_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "es_error_de_ia": { "type": "BOOLEAN" },
            "tipo_error": {
                "type": "STRING",
                "enum": [
                    "abreviacion",
                    "fecha",
                    "hora",
                    "realizacion",
                    "motivo",
                    "tipo",
                    "juzgado",
                    "estilo_observaciones",
                    "otro"
                ]
            },
            "regla": { "type": "STRING" },
            "confianza": { "type": "NUMBER" },
            "necesita_aclaracion": { "type": "BOOLEAN" },
            "pregunta": { "type": "STRING" }
        },
        "required": [
            "es_error_de_ia",
            "tipo_error",
            "regla",
            "confianza",
            "necesita_aclaracion",
            "pregunta"
        ]
    })
}

pub(crate) fn es_error_esquema_no_soportado(status: reqwest::StatusCode, msg: &str) -> bool {
    if !matches!(status.as_u16(), 400 | 422) {
        return false;
    }
    let m = msg.to_lowercase();
    m.contains("json_schema")
        || m.contains("response_format")
        || m.contains("responseschema")
        || m.contains("response_schema")
        || m.contains("schema")
        || m.contains("structured")
        || m.contains("additionalproperties")
        || m.contains("generation_config")
        || m.contains("generationconfig")
}

fn parsear_json_limpio(content: &str) -> Result<Value, serde_json::Error> {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let sin_lang = rest
            .trim_start_matches("json")
            .trim_start_matches("JSON")
            .trim();
        let limpio = sin_lang.strip_suffix("```").unwrap_or(sin_lang).trim();
        if let Ok(v) = serde_json::from_str(limpio) {
            return Ok(v);
        }
    }
    if let Ok(v) = serde_json::from_str(trimmed) {
        return Ok(v);
    }
    if let (Some(ini), Some(fin)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if fin > ini {
            return serde_json::from_str(&trimmed[ini..=fin]);
        }
    }
    serde_json::from_str(trimmed)
}

const SYSTEM_PROMPT_EXTRACCION: &str =
    "Eres un extractor experto de datos de audiencias judiciales colombianas. Responde siempre en JSON válido.";
const SYSTEM_PROMPT_CONSOLIDACION: &str = "Eres un curador senior de memoria de aprendizaje para un asistente de audiencias judiciales en Colombia. Analiza únicamente la evidencia anonimizada provista (patrones transversales generales '*' y/o específicos de un defensor), nunca inventes datos ni nombres propios ni números sensibles, y devuelve exclusivamente JSON válido ajustado al esquema.";

pub(crate) fn esquema_consolidacion_openai() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "consolidacion_memoria_ia",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "acciones_reglas": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tipo_accion": {
                                    "type": "string",
                                    "enum": ["fusionar", "conflicto", "sintetizar"]
                                },
                                "ids_origen": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "tipo_error": {
                                    "type": "string",
                                    "enum": [
                                        "abreviacion",
                                        "fecha",
                                        "hora",
                                        "realizacion",
                                        "motivo",
                                        "tipo",
                                        "juzgado",
                                        "estilo_observaciones",
                                        "general",
                                        "otro"
                                    ]
                                },
                                "campo_objetivo": {
                                    "type": "string",
                                    "enum": [
                                        "radicado",
                                        "tipo",
                                        "fecha",
                                        "hora",
                                        "juzgado",
                                        "realizada",
                                        "motivo",
                                        "observaciones",
                                        "general"
                                    ]
                                },
                                "regla": { "type": "string" },
                                "triggers": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "motivo": { "type": "string" },
                                "confianza": { "type": "number" }
                            },
                            "required": [
                                "tipo_accion",
                                "ids_origen",
                                "tipo_error",
                                "campo_objetivo",
                                "regla",
                                "triggers",
                                "motivo",
                                "confianza"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "resumen_actualizado": { "type": "string" },
                    "sugerencias_build": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "categoria": { "type": "string" },
                                "detalle": { "type": "string" },
                                "evidencia": { "type": "string" }
                            },
                            "required": ["categoria", "detalle", "evidencia"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["acciones_reglas", "resumen_actualizado", "sugerencias_build"],
                "additionalProperties": false
            }
        }
    })
}

pub(crate) fn esquema_consolidacion_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "acciones_reglas": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "tipo_accion": {
                            "type": "STRING",
                            "enum": ["fusionar", "conflicto", "sintetizar"]
                        },
                        "ids_origen": {
                            "type": "ARRAY",
                            "items": { "type": "STRING" }
                        },
                        "tipo_error": {
                            "type": "STRING",
                            "enum": [
                                "abreviacion",
                                "fecha",
                                "hora",
                                "realizacion",
                                "motivo",
                                "tipo",
                                "juzgado",
                                "estilo_observaciones",
                                "general",
                                "otro"
                            ]
                        },
                        "campo_objetivo": {
                            "type": "STRING",
                            "enum": [
                                "radicado",
                                "tipo",
                                "fecha",
                                "hora",
                                "juzgado",
                                "realizada",
                                "motivo",
                                "observaciones",
                                "general"
                            ]
                        },
                        "regla": { "type": "STRING" },
                        "triggers": {
                            "type": "ARRAY",
                            "items": { "type": "STRING" }
                        },
                        "motivo": { "type": "STRING" },
                        "confianza": { "type": "NUMBER" }
                    },
                    "required": [
                        "tipo_accion",
                        "ids_origen",
                        "tipo_error",
                        "campo_objetivo",
                        "regla",
                        "triggers",
                        "motivo",
                        "confianza"
                    ]
                }
            },
            "resumen_actualizado": { "type": "STRING" },
            "sugerencias_build": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "categoria": { "type": "STRING" },
                        "detalle": { "type": "STRING" },
                        "evidencia": { "type": "STRING" }
                    },
                    "required": ["categoria", "detalle", "evidencia"]
                }
            }
        },
        "required": ["acciones_reglas", "resumen_actualizado", "sugerencias_build"]
    })
}

async fn enviar_peticion_openai(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    prompt: &str,
    max_tokens: Option<u32>,
    response_format: Value,
) -> Result<(reqwest::StatusCode, Value), String> {
    let sys = if system_prompt.trim().is_empty() {
        SYSTEM_PROMPT_EXTRACCION
    } else {
        system_prompt
    };
    let mut body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": sys
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "response_format": response_format
    });

    if let Some(mt) = max_tokens {
        body["max_completion_tokens"] = json!(mt);
    }

    // Modelos clásicos soportan temperature=0.1; modelos de razonamiento (gpt-5*, o1, o3, o4) usan su temperatura por defecto
    if model.starts_with("gpt-4") || model.starts_with("gpt-3.5") {
        body["temperature"] = json!(0.1);
    }

    let resp = http_client()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| formatear_error_red("OpenAI", e))?;

    let status = resp.status();
    let json_resp: Value = resp
        .json()
        .await
        .map_err(|e| format!("Respuesta inválida de OpenAI: {}", e.without_url()))?;
    Ok((status, json_resp))
}

async fn llamar_openai(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    prompt: &str,
    max_tokens: Option<u32>,
    response_schema: Option<Value>,
) -> Result<Value, String> {
    let formato_inicial = response_schema
        .clone()
        .unwrap_or_else(|| json!({ "type": "json_object" }));
    let (mut status, mut json_resp) = enviar_peticion_openai(
        api_key,
        model,
        system_prompt,
        prompt,
        max_tokens,
        formato_inicial,
    )
    .await?;

    if !status.is_success() && response_schema.is_some() {
        let msg = json_resp["error"]["message"].as_str().unwrap_or("");
        if es_error_esquema_no_soportado(status, msg) {
            (status, json_resp) = enviar_peticion_openai(
                api_key,
                model,
                system_prompt,
                prompt,
                max_tokens,
                json!({ "type": "json_object" }),
            )
            .await?;
        }
    }

    if !status.is_success() {
        let msg = json_resp["error"]["message"]
            .as_str()
            .unwrap_or("Error desconocido en la API de OpenAI");
        return Err(format!("OpenAI ({}): {}", status, msg));
    }

    let content = json_resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "OpenAI no devolvió contenido en la respuesta".to_string())?;

    parsear_json_limpio(content)
        .map_err(|e| format!("No se pudo interpretar el JSON de OpenAI: {}", e))
}

async fn enviar_peticion_gemini(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    prompt: &str,
    max_tokens: Option<u32>,
    response_schema: Option<&Value>,
) -> Result<(reqwest::StatusCode, Value), String> {
    let modelo_limpio = model.trim().trim_start_matches("models/");
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        modelo_limpio
    );

    let mut generation_config = json!({
        "responseMimeType": "application/json",
        "temperature": 0.1
    });
    if let Some(mt) = max_tokens {
        generation_config["maxOutputTokens"] = json!(mt);
    }
    if let Some(schema) = response_schema {
        generation_config["responseSchema"] = schema.clone();
    }

    let mut body = json!({
        "contents": [
            {
                "parts": [
                    { "text": prompt }
                ]
            }
        ],
        "generationConfig": generation_config
    });
    if !system_prompt.trim().is_empty() {
        body["systemInstruction"] = json!({
            "parts": [
                { "text": system_prompt }
            ]
        });
    }

    let resp = http_client()
        .post(&url)
        .header("x-goog-api-key", api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| formatear_error_red("Gemini", e))?;

    let status = resp.status();
    let json_resp: Value = resp
        .json()
        .await
        .map_err(|e| format!("Respuesta inválida de Gemini: {}", e.without_url()))?;
    Ok((status, json_resp))
}

fn extraer_modelo_sugerido_gemini(msg: &str) -> Option<String> {
    let re = Regex::new(r"use models/([a-zA-Z0-9._-]+)").ok()?;
    re.captures(msg)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

async fn llamar_gemini(
    api_key: &str,
    model: &str,
    system_prompt: &str,
    prompt: &str,
    max_tokens: Option<u32>,
    response_schema: Option<Value>,
) -> Result<(Value, String), String> {
    let mut modelo_activo = model.trim().trim_start_matches("models/").to_string();
    let (mut status, mut json_resp) = enviar_peticion_gemini(
        api_key,
        &modelo_activo,
        system_prompt,
        prompt,
        max_tokens,
        response_schema.as_ref(),
    )
    .await?;

    // Si Google retiró el modelo para cuentas nuevas (404), usar automáticamente el modelo sugerido
    // por el propio mensaje de error de Google (o "gemini-flash-latest") y actualizar settings.json.
    if status.as_u16() == 404 {
        let msg_404 = json_resp["error"]["message"].as_str().unwrap_or("");
        let reemplazo = extraer_modelo_sugerido_gemini(msg_404)
            .unwrap_or_else(|| "gemini-flash-latest".to_string());
        if reemplazo != modelo_activo {
            if let Ok((st2, jr2)) = enviar_peticion_gemini(
                api_key,
                &reemplazo,
                system_prompt,
                prompt,
                max_tokens,
                response_schema.as_ref(),
            )
            .await
            {
                if st2.is_success()
                    || es_error_esquema_no_soportado(
                        st2,
                        jr2["error"]["message"].as_str().unwrap_or(""),
                    )
                {
                    modelo_activo = reemplazo.clone();
                    if st2.is_success() {
                        let mut cfg = cargar_settings();
                        cfg.gemini_model = reemplazo;
                        let _ = guardar_settings(&cfg);
                    }
                    status = st2;
                    json_resp = jr2;
                }
            }
        }
    }

    if !status.is_success() && response_schema.is_some() {
        let msg = json_resp["error"]["message"].as_str().unwrap_or("");
        if es_error_esquema_no_soportado(status, msg) {
            (status, json_resp) = enviar_peticion_gemini(
                api_key,
                &modelo_activo,
                system_prompt,
                prompt,
                max_tokens,
                None,
            )
            .await?;
            if status.is_success() && modelo_activo != model.trim().trim_start_matches("models/") {
                let mut cfg = cargar_settings();
                cfg.gemini_model = modelo_activo.clone();
                let _ = guardar_settings(&cfg);
            }
        }
    }

    if !status.is_success() {
        let msg = json_resp["error"]["message"]
            .as_str()
            .unwrap_or("Error desconocido en la API de Gemini");
        return Err(format!("Gemini ({}): {}", status, msg));
    }

    let content = json_resp["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemini no devolvió texto en la respuesta".to_string())?;

    let val = parsear_json_limpio(content)
        .map_err(|e| format!("No se pudo interpretar el JSON de Gemini: {}", e))?;
    Ok((val, modelo_activo))
}

async fn consultar_json_proveedor_con_opciones(
    prompt: &str,
    system_prompt: &str,
    max_tokens: Option<u32>,
    openai_schema: Option<Value>,
    gemini_schema: Option<Value>,
) -> Result<(Value, String), String> {
    let settings = cargar_settings();
    if settings.provider == "gemini" {
        if settings.gemini_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de Google Gemini.".to_string());
        }
        let (val, modelo_usado) = llamar_gemini(
            &settings.gemini_api_key,
            &settings.gemini_model,
            system_prompt,
            prompt,
            max_tokens,
            gemini_schema,
        )
        .await?;
        Ok((val, format!("gemini:{}", modelo_usado)))
    } else {
        if settings.openai_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de OpenAI.".to_string());
        }
        let val = llamar_openai(
            &settings.openai_api_key,
            &settings.openai_model,
            system_prompt,
            prompt,
            max_tokens,
            openai_schema,
        )
        .await?;
        Ok((val, format!("openai:{}", settings.openai_model.trim())))
    }
}

async fn consultar_json_proveedor_con_esquema(
    prompt: &str,
    openai_schema: Option<Value>,
    gemini_schema: Option<Value>,
) -> Result<Value, String> {
    let (val, _) = consultar_json_proveedor_con_opciones(
        prompt,
        SYSTEM_PROMPT_EXTRACCION,
        None,
        openai_schema,
        gemini_schema,
    )
    .await?;
    Ok(val)
}

async fn consultar_json_proveedor(prompt: &str) -> Result<Value, String> {
    consultar_json_proveedor_con_esquema(
        prompt,
        Some(esquema_analisis_openai()),
        Some(esquema_analisis_gemini()),
    )
    .await
}

fn construir_prompt_analisis_correccion(correccion: &CorreccionIa) -> String {
    let cambios: Vec<String> = correccion
        .campos
        .iter()
        .map(|c| {
            format!(
                "- Campo '{}': IA propuso {:?} -> Usuario guardó {:?}",
                c.campo, c.valor_ia, c.valor_usuario
            )
        })
        .collect();
    let nota = if correccion.nota_usuario.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nACLARACIÓN DEL USUARIO:\n\"{}\"\n",
            correccion.nota_usuario.trim()
        )
    };

    format!(
        r#"Analiza por qué el usuario corrigió los datos que extrajo la IA a partir de un texto de audiencia judicial en Colombia.
Devuelve ÚNICAMENTE un objeto JSON válido con esta estructura:

{{
  "es_error_de_ia": true,
  "tipo_error": "abreviacion | fecha | hora | realizacion | motivo | tipo | juzgado | estilo_observaciones | otro",
  "regla": "Instrucción breve, concreta y reutilizable (máximo 1 oración) para no repetir el fallo en textos similares",
  "confianza": 0.9,
  "necesita_aclaracion": false,
  "pregunta": "Solo si necesita_aclaracion es true, pregunta corta y directa para el usuario"
}}

CRITERIOS:
1. Si el usuario puso un dato que NO aparecía ni se podía deducir del texto original (por ejemplo, agregó información externa posterior), responde "es_error_de_ia": false y "necesita_aclaracion": false.
2. Si el usuario dejó una abreviación tal cual (ej: CTO, MPAL) o corrigió el motivo de no realización, formula una "regla" clara para futuros textos parecidos.
3. Si el usuario reescribió "observaciones", sintetiza en "regla" su estilo preferido de redacción con "tipo_error": "estilo_observaciones".
4. Usa "necesita_aclaracion": true (y "confianza" menor a 0.75) ÚNICAMENTE cuando el cambio parezca contradecir el texto original o el significado de una sigla/motivo no pueda deducirse sin preguntar al usuario. Si ya hay ACLARACIÓN DEL USUARIO, responde "necesita_aclaracion": false.

TEXTO ORIGINAL:
"""
{}
"""

DIFERENCIAS DETECTADAS:
{}
{}"#,
        correccion.texto_original,
        cambios.join("\n"),
        nota
    )
}

fn construir_prompt_analisis_cluster(correcciones: &[CorreccionIa]) -> String {
    if correcciones.len() == 1 {
        return construir_prompt_analisis_correccion(&correcciones[0]);
    }
    let casos: Vec<String> = correcciones
        .iter()
        .enumerate()
        .take(4)
        .map(|(idx, corr)| {
            let cambios: Vec<String> = corr
                .campos
                .iter()
                .map(|c| format!("{} ({:?} -> {:?})", c.campo, c.valor_ia, c.valor_usuario))
                .collect();
            let nota = if corr.nota_usuario.trim().is_empty() {
                String::new()
            } else {
                format!(" | Aclaración: {:?}", corr.nota_usuario.trim())
            };
            format!(
                "Caso {}:\n  Texto: \"{}\"\n  Correcciones: {}{}",
                idx + 1,
                corr.texto_original,
                cambios.join("; "),
                nota
            )
        })
        .collect();

    format!(
        r#"Analiza este clúster coherente de {} correcciones repetidas hechas por el usuario a las extracciones de IA en audiencias judiciales colombianas.
Induce UNA sola regla general y reutilizable que capture el patrón común sin sobreajustar a nombres propios ni números de radicado concretos.
Devuelve ÚNICAMENTE un objeto JSON válido con esta estructura:

{{
  "es_error_de_ia": "true si el patrón corresponde a un error o preferencia repetida de la IA, o false si es dato externo",
  "tipo_error": "abreviacion | fecha | hora | realizacion | motivo | tipo | juzgado | estilo_observaciones | otro",
  "regla": "Instrucción general, breve y reutilizable (máximo 1 oración) aplicable a futuros textos similares",
  "confianza": "Número entre 0.0 y 1.0 (usa >= 0.75 si el patrón es claro o cuenta con aclaración; < 0.75 si sigue ambiguo)",
  "necesita_aclaracion": "true si persiste una ambigüedad o contradicción que requiera preguntar al usuario; false si el patrón ya es claro o tiene aclaración",
  "pregunta": "Solo si necesita_aclaracion es true, pregunta corta y directa; de lo contrario \"\""
}}

CRITERIOS:
1. Si todos los cambios agregan datos que no estaban ni podían deducirse del texto original, responde "es_error_de_ia": false y "necesita_aclaracion": false.
2. Si algún caso incluye Aclaración del usuario que ya explica el criterio, responde "necesita_aclaracion": false.
3. Si el patrón sigue siendo ambiguo o aparentemente contradictorio con el texto y no hay aclaración, usa "necesita_aclaracion": true y "confianza" < 0.75.

CASOS AGRUPADOS:
{}"#,
        correcciones.len(),
        casos.join("\n\n")
    )
}

pub(crate) fn construir_prompt_consolidacion(lote: &LoteConsolidacion) -> String {
    let max_chars = if lote.ambito == "*" { 320 } else { 380 };
    let reglas_txt = if lote.reglas_ambito.is_empty() {
        "(Sin reglas en este ámbito)".to_string()
    } else {
        lote.reglas_ambito
            .iter()
            .map(|r| {
                format!(
                    "- id={} | estado={} | origen={} | alcance={} | campo={} | q10={:.2} | imp={} ok={} fail={} | regla={:?} | triggers={:?}",
                    r.id,
                    r.estado,
                    r.origen,
                    r.defensor_alcance,
                    r.campo_objetivo,
                    r.confianza_q10,
                    r.impresiones,
                    r.aciertos_directos,
                    r.fallos_directos,
                    r.regla,
                    r.triggers
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let corrs_txt = if lote.correcciones_recientes.is_empty() {
        "(Sin correcciones recientes pendientes de consolidar)".to_string()
    } else {
        lote.correcciones_recientes
            .iter()
            .map(|c| {
                let cambios: Vec<String> = c
                    .cambios
                    .iter()
                    .map(|fc| format!("{}: {:?}->{:?}", fc.campo, fc.valor_ia, fc.valor_usuario))
                    .collect();
                format!(
                    "- id={} | def={} | campo={} | estado={} | cambios=[{}] | nota={:?} | texto={:?}",
                    c.id,
                    c.defensor_clave,
                    c.campo_principal,
                    c.estado,
                    cambios.join("; "),
                    c.nota_usuario,
                    c.texto_anonimizado
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let met_txt = if lote.metricas_resumen.is_empty() {
        "(Sin métricas registradas)".to_string()
    } else {
        lote.metricas_resumen
            .iter()
            .map(|m| {
                format!(
                    "- {}: eval={} ok={} corr={} ewma={:.2} drift={}",
                    m.campo, m.evaluaciones, m.aciertos, m.correcciones, m.precision_ewma, m.drift_activo
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let pat_txt = if lote.patrones_transversales.is_empty() {
        "(Sin patrones transversales repetidos)".to_string()
    } else {
        lote.patrones_transversales
            .iter()
            .map(|p| {
                format!(
                    "- {} (campo={}, ocurrencias={}, defensores={}): {:?}",
                    p.firma, p.campo, p.ocurrencias, p.defensores_distintos, p.ejemplo_anonimizado
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"Consolida de forma incremental la memoria de aprendizaje para el ámbito `{}` ({}).
Resumen sintético previo (v{}): {:?}

MÉTRICAS DEL ÁMBITO:
{}

PATRONES TRANSVERSALES OBSERVADOS:
{}

REGLAS ACTUALES DEL ÁMBITO:
{}

CORRECCIONES ANONIMIZADAS RECIENTES:
{}

INSTRUCCIONES:
1. Operaciones delta permitidas en `acciones_reglas`:
   - `"fusionar"`: agrupa `ids_origen` de reglas locales redundantes en una redacción más clara (nunca toques reglas `base_invariante` / `base-*`).
   - `"conflicto"`: marca reglas locales obsoletas o contradictorias en `ids_origen` (requiere `confianza >= 0.75`; prohibido sobre `base_invariante`).
   - `"sintetizar"`: propone como máximo 2 reglas nuevas concisas (que no dupliquen reglas ya presentes) con `triggers` cortos útiles.
2. `resumen_actualizado`: redacta un perfil sintético consolidado de máximo {} caracteres con los criterios duraderos más útiles de este ámbito para inyectar al prompt de extracción. No incluyas nombres propios ni números de radicado.
3. `sugerencias_build`: si detectas jerga recurrente o entidades faltantes útiles para futuras versiones del software, inclúyelas de forma anónima; si no hay, devuelve `[]`."#,
        lote.ambito,
        lote.nombre_ambito,
        lote.version_resumen,
        lote.resumen_actual,
        met_txt,
        pat_txt,
        reglas_txt,
        corrs_txt,
        max_chars
    )
}

pub async fn consolidar_memoria_ia(
    lote: &LoteConsolidacion,
) -> Result<(ResultadoConsolidacionIa, String), String> {
    let prompt = construir_prompt_consolidacion(lote);
    let (raw, modelo) = consultar_json_proveedor_con_opciones(
        &prompt,
        SYSTEM_PROMPT_CONSOLIDACION,
        Some(1400),
        Some(esquema_consolidacion_openai()),
        Some(esquema_consolidacion_gemini()),
    )
    .await?;
    let resultado: ResultadoConsolidacionIa = serde_json::from_value(raw)
        .map_err(|e| format!("No se pudo interpretar la consolidación de IA: {}", e))?;
    Ok((resultado, modelo))
}

/// Inducción diferida por clústeres (`N >= 2` ejemplos coherentes): deduce una regla general
/// en una sola llamada al LLM en lugar de llamar por cada corrección aislada.
pub async fn analizar_cluster_correcciones_ia(
    correcciones: &[CorreccionIa],
) -> Result<AnalisisCorreccionIa, String> {
    if correcciones.is_empty() {
        return Err("No hay correcciones en el clúster para analizar.".to_string());
    }
    let prompt = construir_prompt_analisis_cluster(correcciones);
    let raw = consultar_json_proveedor(&prompt).await?;

    let es_error_de_ia = raw
        .get("es_error_de_ia")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tipo_error = raw
        .get("tipo_error")
        .and_then(|v| v.as_str())
        .unwrap_or("general")
        .trim()
        .to_string();
    let regla = raw
        .get("regla")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let confianza = raw
        .get("confianza")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let necesita_aclaracion = raw
        .get("necesita_aclaracion")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let pregunta = raw
        .get("pregunta")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(AnalisisCorreccionIa {
        es_error_de_ia,
        tipo_error,
        regla,
        confianza,
        necesita_aclaracion,
        pregunta,
    })
}

/// Segunda llamada a la IA para una corrección puntual (p. ej. tras aclaración manual del usuario).
pub async fn analizar_correccion_ia(
    correccion: &CorreccionIa,
) -> Result<AnalisisCorreccionIa, String> {
    analizar_cluster_correcciones_ia(std::slice::from_ref(correccion)).await
}

#[cfg(test)]
pub(crate) fn arbitrar_salida_hibrida(
    texto: &str,
    raw_json: &Value,
    memoria_local: &MemoriaAprendizaje,
    priors: &IndicePriorsExcel,
    reglas_inyectadas: Vec<String>,
    ejemplos_inyectados: Vec<String>,
    motor: &MotorEstadisticoLocal,
) -> AiAudienciaOutput {
    arbitrar_salida_hibrida_con_catalogos(
        texto,
        raw_json,
        memoria_local,
        priors,
        reglas_inyectadas,
        ejemplos_inyectados,
        motor,
        &[],
        &[],
    )
}

#[cfg(test)]
pub(crate) fn arbitrar_salida_hibrida_con_catalogos(
    texto: &str,
    raw_json: &Value,
    memoria_local: &MemoriaAprendizaje,
    priors: &IndicePriorsExcel,
    reglas_inyectadas: Vec<String>,
    ejemplos_inyectados: Vec<String>,
    motor: &MotorEstadisticoLocal,
    tipos_usuario: &[String],
    juzgados_usuario: &[String],
) -> AiAudienciaOutput {
    arbitrar_salida_hibrida_con_catalogos_y_defensor(
        texto,
        raw_json,
        memoria_local,
        priors,
        reglas_inyectadas,
        ejemplos_inyectados,
        motor,
        tipos_usuario,
        juzgados_usuario,
        None,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn arbitrar_salida_hibrida_con_catalogos_y_defensor(
    texto: &str,
    raw_json: &Value,
    memoria_local: &MemoriaAprendizaje,
    priors: &IndicePriorsExcel,
    reglas_inyectadas: Vec<String>,
    ejemplos_inyectados: Vec<String>,
    motor: &MotorEstadisticoLocal,
    tipos_usuario: &[String],
    juzgados_usuario: &[String],
    defensor: Option<&str>,
) -> AiAudienciaOutput {
    let base_vacia = MemoriaAprendizaje::default();
    arbitrar_salida_hibrida_con_base_catalogos_y_defensor(
        texto,
        raw_json,
        &base_vacia,
        memoria_local,
        priors,
        reglas_inyectadas,
        ejemplos_inyectados,
        motor,
        tipos_usuario,
        juzgados_usuario,
        defensor,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn arbitrar_salida_hibrida_con_base_catalogos_y_defensor(
    texto: &str,
    raw_json: &Value,
    memoria_base: &MemoriaAprendizaje,
    memoria_local: &MemoriaAprendizaje,
    priors: &IndicePriorsExcel,
    reglas_inyectadas: Vec<String>,
    ejemplos_inyectados: Vec<String>,
    motor: &MotorEstadisticoLocal,
    tipos_usuario: &[String],
    juzgados_usuario: &[String],
    defensor: Option<&str>,
) -> AiAudienciaOutput {
    let get_str = |key: &str| -> String {
        raw_json
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };

    let motor_pred =
        motor.predecir_con_base_para_defensor(texto, memoria_base, memoria_local, priors, defensor);
    let pref_efectiva =
        preferencias_efectivas_con_base_para_defensor(memoria_base, memoria_local, defensor);

    let mut radicado = limpiar_valor_opcional_vacio(&get_str("radicado"));
    let mut proc_vinculado = None;
    for num in extraer_numeros_radicado(if radicado.is_empty() { texto } else { &radicado }) {
        if let Some(p) = priors.procesos_por_numero.get(&num) {
            proc_vinculado = Some(p.clone());
            break;
        }
    }
    if let Some(ref rad_sug) = motor_pred.radicado_sugerido {
        let nums_raw = extraer_numeros_radicado(&radicado);
        let nums_sug = extraer_numeros_radicado(rad_sug);
        if radicado.is_empty()
            || (!radicado.contains(" - ")
                && rad_sug.contains(" - ")
                && nums_raw.iter().any(|n| nums_sug.contains(n) || rad_sug.contains(n)))
        {
            radicado = rad_sug.clone();
        }
    }

    let raw_tipo = limpiar_valor_opcional_vacio(&get_str("tipo_audiencia"));
    let tipo_resuelto = resolver_alias_exacto_validado_con_base_para_defensor(
        memoria_base,
        memoria_local,
        "tipo",
        &raw_tipo,
        0.30,
        defensor,
    )
    .unwrap_or_else(|| {
        if raw_tipo.is_empty() {
            proc_vinculado
                .as_ref()
                .map(|p| p.tipo_habitual.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| motor_pred.tipo_sugerido.clone())
                .unwrap_or_default()
        } else {
            raw_tipo.clone()
        }
    });
    let (tipo_audiencia, tipo_otra) =
        mapear_tipo_audiencia_con_usuario(&tipo_resuelto, tipos_usuario);

    let fecha = normalizar_fecha_ia(&limpiar_valor_opcional_vacio(&get_str("fecha")));
    let raw_hora = limpiar_valor_opcional_vacio(&get_str("hora"));
    let raw_minuto = limpiar_valor_opcional_vacio(&get_str("minuto"));
    let (hora_norm, minuto_norm) = normalizar_hora_minuto_ia(&raw_hora, &raw_minuto);
    let (hora, minuto, _) = resolver_hora_con_prior(&hora_norm, &minuto_norm, texto, priors);

    let raw_juzgado = limpiar_valor_opcional_vacio(&get_str("juzgado"));
    // Preserva abreviaciones CTO/MPAL/PCTO por defecto; sustituye solo si existe un alias validado
    // (q10 >= 0.30, activable a partir de la 2.ª corrección consistente) o coincidencia exacta del catálogo.
    let juzgado_bruto = if raw_juzgado.is_empty() {
        let siglas_texto = extraer_siglas_cortas_juzgado(texto);
        let por_sigla = if siglas_texto.len() == 1 {
            resolver_alias_exacto_validado_con_base_para_defensor(
                memoria_base,
                memoria_local,
                "juzgado",
                &siglas_texto[0],
                0.30,
                defensor,
            )
        } else {
            None
        };
        por_sigla
            .or_else(|| {
                proc_vinculado
                    .as_ref()
                    .map(|p| p.juzgado_habitual.clone())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| motor_pred.juzgado_sugerido.clone())
            .unwrap_or_default()
    } else {
        resolver_alias_exacto_validado_con_base_para_defensor(
            memoria_base,
            memoria_local,
            "juzgado",
            &raw_juzgado,
            0.30,
            defensor,
        )
        .or_else(|| {
            // Solo expande siglas cortas desnudas ("J4", "J04", "Jdo 4", "Juzgado 4");
            // nunca sobreescribe calificadores explícitos como "J4 CTO", "J4 MPAL" o "J4 Circuito".
            es_solo_sigla_corta_juzgado(&raw_juzgado).and_then(|sigla| {
                resolver_alias_exacto_validado_con_base_para_defensor(
                    memoria_base,
                    memoria_local,
                    "juzgado",
                    &sigla,
                    0.30,
                    defensor,
                )
            })
        })
        .unwrap_or(raw_juzgado)
    };
    let juzgado = normalizar_juzgado_catalogo(&juzgado_bruto, juzgados_usuario);

    let mut se_realizo = get_str("se_realizo").to_uppercase();
    if se_realizo == "SÍ" {
        se_realizo = "SI".to_string();
    }

    let texto_norm = quitar_tildes(&texto.to_lowercase());
    let dice_si_realizada = [
        "¿se realizo?: si",
        "¿se realizo? si",
        "se realizo?: si",
        "se realizo: si",
    ]
    .iter()
    .any(|p| texto_norm.contains(p));
    let dice_ningun_motivo = detectar_dice_ningun_motivo(&texto_norm);
    let tiene_motivo_explicito_fracaso = [
        "la fiscal no vino",
        "fiscal no comparecio",
        "fiscal no se conecto",
        "defensor no comparecio",
        "juez ausente",
        "inasistente",
        "solicite reprogramacion",
    ]
    .iter()
    .any(|p| texto_norm.contains(p));

    let mut regla_realizacion_fijada = false;
    if dice_si_realizada && (!tiene_motivo_explicito_fracaso || dice_ningun_motivo) {
        se_realizo = "SI".to_string();
        regla_realizacion_fijada = true;
    } else if dice_si_realizada && tiene_motivo_explicito_fracaso {
        // Caso contradictorio de legacy-python: dice "Sí" pero narra que fracasó
        se_realizo = "NO".to_string();
        regla_realizacion_fijada = true;
    } else if detectar_patron_fracaso_con_preferencias(&texto_norm, &pref_efectiva) {
        se_realizo = "NO".to_string();
        regla_realizacion_fijada = true;
    }

    if !regla_realizacion_fijada {
        let valido_llm = se_realizo == "SI" || se_realizo == "NO";
        if let Some(ref pred_r) = motor_pred.realizada {
            let override_seguro = !motor_pred.drift_realizada
                && motor_pred.muestras_entrenamiento >= 20
                && motor_pred.realizadas_clases_vistas >= 2
                && motor_pred.realizada_soporte_clase >= 5
                && motor_pred.realizada_confianza >= 0.90
                && motor_pred.knn_realizada_acuerdo;
            if (!valido_llm && motor_pred.realizada_confianza >= 0.55) || override_seguro {
                se_realizo = pred_r.clone();
            }
        }
    }

    let raw_motivo = if !get_str("motivo").is_empty() {
        get_str("motivo")
    } else {
        get_str("motivos")
    };

    let (motivo_inferido, motivos_secundarios) = inferir_motivos(texto);
    let mut motivo_final = String::new();

    if se_realizo == "NO" {
        if let Some(idx) = mapear_motivo_a_indice(&raw_motivo) {
            motivo_final = MOTIVOS_LABELS[idx].to_string();
        } else if let Some(idx) = mapear_motivo_a_indice(&motivo_inferido) {
            motivo_final = MOTIVOS_LABELS[idx].to_string();
        } else if motor_pred.motivo_confianza >= 0.60 {
            if let Some(ref pred_m) = motor_pred.motivo {
                if let Some(idx) = mapear_motivo_a_indice(pred_m) {
                    motivo_final = MOTIVOS_LABELS[idx].to_string();
                }
            }
        }

        // Arbitraje local sobre un motivo ya válido del LLM: exige consenso LogReg + k-NN,
        // soporte balanceado en varias clases y ausencia de drift en el campo.
        let override_motivo_seguro = !motor_pred.drift_motivo
            && motor_pred.motivos_clases_vistas >= 2
            && motor_pred.motivo_soporte_clase >= 3
            && motor_pred.motivo_confianza >= 0.90
            && motor_pred.knn_motivo_acuerdo;
        if override_motivo_seguro {
            if let Some(ref pred_m) = motor_pred.motivo {
                if let Some(idx) = mapear_motivo_a_indice(pred_m) {
                    let candidato = MOTIVOS_LABELS[idx];
                    let es_reaccion =
                        matches!(candidato, "Defensor Público" | "Defensor Confianza");
                    let hay_causa_principal_expl = matches!(
                        motivo_inferido.as_str(),
                        "Juez" | "Fiscalía" | "Usuario" | "INPEC" | "Víctima" | "ICBF"
                    );
                    if !(es_reaccion && hay_causa_principal_expl) {
                        motivo_final = candidato.to_string();
                    }
                }
            }
        }

        // Invariante Defensoría 1: abogado contractual/privado/def. confianza -> Defensor Confianza
        if motivo_final == "Defensor Público" && detectar_defensor_confianza(&texto_norm) {
            motivo_final = "Defensor Confianza".to_string();
        }

        // Invariante Defensoría 2: causa principal prevalece sobre reacción del defensor
        if matches!(
            motivo_final.as_str(),
            "Defensor Público" | "Defensor Confianza"
        ) && matches!(
            motivo_inferido.as_str(),
            "Juez" | "Fiscalía" | "Usuario" | "INPEC" | "Víctima" | "ICBF"
        ) {
            motivo_final = motivo_inferido.clone();
        }
    }

    let pref = &pref_efectiva;
    let p_causas_sec = pref.incluir_causas_secundarias_alpha
        / (pref.incluir_causas_secundarias_alpha + pref.incluir_causas_secundarias_beta).max(1.0);
    let p_reprog = pref.incluir_reprogramacion_alpha
        / (pref.incluir_reprogramacion_alpha + pref.incluir_reprogramacion_beta).max(1.0);

    let mut observaciones = limpiar_observacion_vacia(get_str("observaciones"));
    if p_causas_sec >= 0.45 && !motivos_secundarios.is_empty() {
        let extra = format!("Causas secundarias: {}", motivos_secundarios.join(", "));
        if observaciones.is_empty() {
            observaciones = extra;
        } else if !observaciones.contains("Causas secundarias") {
            observaciones = format!("{}; {}", observaciones, extra);
        }
    }
    if se_realizo == "NO" && p_reprog >= 0.45 {
        let fechas_reprog = extraer_fechas_reprogramacion(texto);
        if !fechas_reprog.is_empty()
            && !observaciones
                .to_lowercase()
                .contains(&fechas_reprog.to_lowercase())
        {
            let extra = format!("Reprogramada para: {}", fechas_reprog);
            if observaciones.is_empty() {
                observaciones = extra;
            } else {
                observaciones = format!("{}; {}", observaciones, extra);
            }
        }
    }

    AiAudienciaOutput {
        radicado,
        tipo_audiencia,
        tipo_otra,
        fecha,
        hora,
        minuto,
        juzgado,
        se_realizo,
        motivo: motivo_final,
        observaciones,
        reglas_inyectadas,
        ejemplos_inyectados,
        prediccion_local: Some(motor_pred),
    }
}

pub async fn extraer_datos_audiencia_ia(texto: &str) -> Result<AiAudienciaOutput, String> {
    extraer_datos_audiencia_ia_con_defensor(texto, None).await
}

pub async fn extraer_datos_audiencia_ia_con_defensor(
    texto: &str,
    defensor: Option<&str>,
) -> Result<AiAudienciaOutput, String> {
    if texto.trim().is_empty() {
        return Err("Ingresa el texto de la audiencia para procesar.".to_string());
    }

    let current_year = Local::now().year();
    let texto_pre = expandir_abreviaciones_y_compacto(texto, current_year);
    let (contexto_aprendizaje, reglas_inyectadas, ejemplos_inyectados) =
        obtener_contexto_prompt_con_traza_y_defensor(texto, defensor);

    let memoria_local = cargar_memoria_local_snapshot();
    let memoria_base = memoria_base_embebida();
    let settings = cargar_settings();
    let priors = cargar_priors_actuales();
    let candidatos = seleccionar_candidatos_prompt_con_base_para_defensor(
        &texto_pre,
        memoria_base,
        &memoria_local,
        &settings,
        &priors,
        defensor,
    );
    let motor = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
        memoria_base,
        &memoria_local,
        defensor,
    );
    let (resumen_general, resumen_defensor) = if settings.usar_resumen_consolidado {
        obtener_resumenes_consolidados_prompt(memoria_base, &memoria_local, defensor)
    } else {
        (None, None)
    };

    let prompt = construir_prompt_con_candidatos_y_resumenes(
        &texto_pre,
        current_year,
        &contexto_aprendizaje,
        Some(&candidatos),
        &settings.tipos_audiencia,
        resumen_general.as_deref(),
        resumen_defensor.as_deref(),
    );
    let raw_json = consultar_json_proveedor_con_esquema(
        &prompt,
        Some(esquema_extraccion_audiencia_openai()),
        Some(esquema_extraccion_audiencia_gemini()),
    )
    .await?;

    Ok(arbitrar_salida_hibrida_con_base_catalogos_y_defensor(
        texto,
        &raw_json,
        memoria_base,
        &memoria_local,
        &priors,
        reglas_inyectadas,
        ejemplos_inyectados,
        &motor,
        &settings.tipos_audiencia,
        &settings.juzgados,
        defensor,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aprendizaje::obtener_contexto_prompt;

    #[test]
    fn test_inferencia_motivos_primera_persona_y_jerarquia() {
        let (m1, _) = inferir_motivos("No pude contactar al procesado y solicité reprogramar");
        assert_eq!(m1, "Defensor Público");

        let (m2, _) = inferir_motivos("El abogado contractual no se presentó a la audiencia");
        assert_eq!(m2, "Defensor Confianza");

        // Causa principal (Fiscalía) debe prevalecer sobre reacción (Defensor Público)
        let (m3, sec3) =
            inferir_motivos("La fiscal no se conectó y por ello solicité reprogramar la audiencia");
        assert_eq!(m3, "Fiscalía");
        assert!(sec3.contains(&"Defensor Público".to_string()));
    }

    #[test]
    fn test_normalizar_fecha_ia() {
        assert_eq!(normalizar_fecha_ia("5/8/2025"), "05/08/2025");
        assert_eq!(normalizar_fecha_ia("2025-08-05"), "05/08/2025");
        assert_eq!(normalizar_fecha_ia("5-8-2025"), "05/08/2025");
        assert_eq!(normalizar_fecha_ia("15/08/2025"), "15/08/2025");
        assert_eq!(normalizar_fecha_ia(""), "");
    }

    #[test]
    fn test_normalizar_hora_minuto_ia() {
        assert_eq!(normalizar_hora_minuto_ia("8", "5"), ("08".into(), "05".into()));
        assert_eq!(normalizar_hora_minuto_ia("14", ""), ("14".into(), "00".into()));
        assert_eq!(normalizar_hora_minuto_ia("", ""), ("".into(), "".into()));
        assert_eq!(normalizar_hora_minuto_ia("25", "10"), ("".into(), "".into()));
        assert_eq!(normalizar_hora_minuto_ia("10", "75"), ("".into(), "".into()));
    }

    #[test]
    fn test_base_legacy_python_integrada() {
        // 1) Abreviaciones del estilo compacto conservando CTO intacto
        let exp = expandir_abreviaciones_y_compacto(
            "JUNIO 24.- JUZGADO 3 CTO PENAL FRACSA POR PROBLEMAS DE CONEXION",
            2026,
        );
        assert!(exp.contains("24 de junio de 2026"));
        assert!(exp.contains("CTO"));
        assert!(exp.contains("Fracasó"));
        assert!(exp.contains("Se presentaron problemas técnicos de conexión"));

        // 2) Reasignación contextual de frases genéricas
        let (mc, _) = inferir_motivos("El abogado del procesado solicita reprogramación");
        assert_eq!(mc, "Defensor Confianza");

        let (mu, _) = inferir_motivos("Se solicita reprogramación de la vista pública");
        assert_eq!(mu, "Usuario");

        // 3) Extracción de fecha de reprogramación para observaciones
        let f = extraer_fechas_reprogramacion(
            "La audiencia fue reprogramada para el día 26 de septiembre de 2025",
        );
        assert_eq!(f, "26 de septiembre de 2025");

        // 4) La base embebida (aprendizaje_base.json) ya inyecta las reglas de legacy-python
        let ctx = obtener_contexto_prompt("Audiencia en Juzgado 3 Cto Penal fracasa fiscalía");
        assert!(ctx.contains("CTO"));
        assert!(ctx.contains("Fiscalía"));

        // 5) Extracción de modelo sugerido ante 404 de Gemini y migración de modelos retirados
        let msg = "This model models/gemini-2.0-flash is no longer available to new users. Please update your code to use models/gemini-2.5-flash for the latest features and improvements.";
        assert_eq!(
            extraer_modelo_sugerido_gemini(msg),
            Some("gemini-2.5-flash".to_string())
        );
        assert_eq!(
            crate::config::normalizar_modelo_gemini("models/gemini-2.0-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(
            crate::config::normalizar_modelo_gemini("models/gemini-3-flash-preview"),
            "gemini-3-flash-preview"
        );
    }

    #[test]
    fn test_fase0_saneamiento_ninguna_se_fija_y_observaciones() {
        // 1) "sin ninguna excusa" NO debe activar dice_ningun_motivo
        assert!(!detectar_dice_ningun_motivo(
            "la fiscal no compareció sin ninguna excusa"
        ));
        assert!(detectar_dice_ningun_motivo(
            "¿se realizó?: sí. motivo (si no se realizó): ninguno"
        ));

        // 2) "se fija" contextual: cuando se realizó y se fija fecha para continuar/lectura, NO fuerza NO
        assert!(!detectar_patron_fracaso_contextual(
            "se realizó audiencia preparatoria y se fija fecha para juicio oral"
        ));
        assert!(!detectar_patron_fracaso_contextual(
            "se efectuó acusación y se fija lectura de fallo"
        ));
        assert!(detectar_patron_fracaso_contextual(
            "se fija nueva fecha para el día 20 de agosto"
        ));
        assert!(detectar_patron_fracaso_contextual(
            "fracsa por solicitud de la fiscalía"
        ));

        // 3) No forzar "Sin observaciones adicionales"
        assert_eq!(limpiar_observacion_vacia("Sin observaciones adicionales".into()), "");
        assert_eq!(limpiar_observacion_vacia("Ninguna".into()), "");
        assert_eq!(
            limpiar_observacion_vacia("Reprogramada para mayo".into()),
            "Reprogramada para mayo"
        );
    }

    #[test]
    fn test_fase2_invariantes_defensoria_vs_heuristicas_adaptables() {
        use crate::aprendizaje::{AliasEstadistico, EjemploPositivo};
        use crate::models::{AudienciaInput, ProcesoHabitual};

        let mut mem = MemoriaAprendizaje::default();
        // Llenar masa mínima para activar el motor local
        for i in 0..6 {
            mem.ejemplos_aciertos.push(EjemploPositivo {
                id: format!("si-{}", i),
                registrada: "2025-08-10T10:00:00-05:00".into(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                texto_anonimizado: "Se realizó audiencia preparatoria sin novedad".into(),
                salida_anonimizada: AudienciaInput {
                    radicado: "PROC - 202500001".into(),
                    tipo: "Audiencia preparatoria".into(),
                    fecha: "10/08/2025".into(),
                    hora: "09:00".into(),
                    juzgado: "Juzgado 1 Penal".into(),
                    realizada: "SI".into(),
                    motivo: String::new(),
                    observaciones: String::new(),
                },
                reglas_inyectadas: Vec::new(),
                defensor_clave: String::new(),
            });
            mem.ejemplos_aciertos.push(EjemploPositivo {
                id: format!("no-{}", i),
                registrada: "2025-08-10T10:00:00-05:00".into(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                texto_anonimizado: "Fracasó audiencia porque el fiscal no se conectó".into(),
                salida_anonimizada: AudienciaInput {
                    radicado: "PROC - 202500002".into(),
                    tipo: "Audiencia concentrada".into(),
                    fecha: "10/08/2025".into(),
                    hora: "14:00".into(),
                    juzgado: "Juzgado 3 Cto Penal".into(),
                    realizada: "NO".into(),
                    motivo: "Fiscalía".into(),
                    observaciones: String::new(),
                },
                reglas_inyectadas: Vec::new(),
                defensor_clave: String::new(),
            });
        }

        let mut priors = IndicePriorsExcel::default();
        priors.horas_freq.insert(15, 20);
        priors.horas_freq.insert(3, 0);
        priors.procesos_por_numero.insert(
            "200016001075202599999".into(),
            ProcesoHabitual {
                radicado_completo: "JUAN PEREZ - 200016001075202599999".into(),
                juzgado_habitual: "Juzgado 3 Cto Penal".into(),
                tipo_habitual: "Audiencia de juicio oral".into(),
                frecuencia: 4,
                defensor_habitual: String::new(),
            },
        );

        let motor = MotorEstadisticoLocal::entrenar_desde_memorias(&MemoriaAprendizaje::default(), &mem);

        // 1) Invariantes Defensoría: causa principal (Fiscalía) vence a reacción (Defensor Público),
        //    CTO se preserva y hora 03 sin AM se resuelve a 15 con prior laboral + radicado enriquecido
        let raw_llm = json!({
            "radicado": "200016001075202599999",
            "tipo_audiencia": "Audiencia de juicio oral",
            "fecha": "20/08/2025",
            "hora": "03",
            "minuto": "30",
            "juzgado": "Juzgado 3 Cto Penal",
            "se_realizo": "NO",
            "motivo": "Defensor Público",
            "observaciones": ""
        });
        let texto = "Audiencia 200016001075202599999 a las 3:30 en Juzgado 3 Cto Penal. La fiscal no se conectó y por ello solicité reprogramar para el día 26 de septiembre de 2025";
        let out = arbitrar_salida_hibrida(
            texto,
            &raw_llm,
            &mem,
            &priors,
            vec!["r1".into()],
            Vec::new(),
            &motor,
        );
        assert_eq!(out.motivo, "Fiscalía");
        assert_eq!(out.juzgado, "Juzgado 3 Cto Penal");
        assert_eq!(out.hora, "15");
        assert_eq!(out.radicado, "JUAN PEREZ - 200016001075202599999");
        assert!(out.observaciones.contains("Causas secundarias: Defensor Público"));
        assert!(out.observaciones.contains("Reprogramada para: 26 de septiembre de 2025"));
        assert!(out.prediccion_local.is_some());

        // 2) Heurísticas estadísticamente adaptables: si el usuario rechaza sufijos y "se fija"->SI,
        //    el sistema deja de imponerlos y aplica alias validado
        mem.preferencias_estilo.incluir_causas_secundarias_alpha = 1.0;
        mem.preferencias_estilo.incluir_causas_secundarias_beta = 8.0;
        mem.preferencias_estilo.incluir_reprogramacion_alpha = 1.0;
        mem.preferencias_estilo.incluir_reprogramacion_beta = 8.0;
        mem.preferencias_estilo.se_fija_aplazamiento_alpha = 1.0;
        mem.preferencias_estilo.se_fija_aplazamiento_beta = 6.0;
        mem.tabla_alias.push(AliasEstadistico {
            campo: "juzgado".into(),
            entrada_norm: crate::estadistica_ia::normalizar_clave_entidad(
                "Juzgado 3 Penal del Circuito",
            ),
            canonico: "Juzgado 3 Penal del Circuito Valledupar".into(),
            alpha: 7.0,
            beta: 1.0,
            ultima_actualizacion: "2025-08-18T10:00:00-05:00".into(),
            defensor_alcance: "*".into(),
            defensores_confirmantes: Vec::new(),
        });

        let raw_adapt = json!({
            "radicado": "RAD-55",
            "tipo_audiencia": "Audiencia concentrada",
            "fecha": "21/08/2025",
            "hora": "09",
            "minuto": "00",
            "juzgado": "Juzgado Tercero Penal del Circuito",
            "se_realizo": "SI",
            "motivo": "",
            "observaciones": ""
        });
        let out_adapt = arbitrar_salida_hibrida(
            "En Juzgado 3 Penal del Circuito se fija fecha próxima audiencia",
            &raw_adapt,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
        );
        assert_eq!(out_adapt.juzgado, "Juzgado 3 Penal del Circuito Valledupar");
        assert_eq!(out_adapt.se_realizo, "SI");
        assert_eq!(out_adapt.observaciones, "");

        // Mismo texto anterior con sufijos desactivados ya no agrega "Causas secundarias" ni "Reprogramada"
        let out_sin_sufijos = arbitrar_salida_hibrida(
            texto,
            &raw_llm,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
        );
        assert!(!out_sin_sufijos.observaciones.contains("Causas secundarias:"));
        assert!(!out_sin_sufijos.observaciones.contains("Reprogramada para:"));

        // 3) Prompt enriquecido incluye top-5 juzgados/tipos y proceso conocido
        let cands = CandidatosPrompt {
            juzgados_top: vec!["Juzgado 3 Cto Penal".into()],
            tipos_top: vec!["Audiencia de juicio oral".into()],
            radicado_vinculado: priors.procesos_por_numero.get("200016001075202599999").cloned(),
            siglas_resueltas: Vec::new(),
        };
        let prompt = construir_prompt_con_candidatos(texto, 2025, "", Some(&cands));
        assert!(prompt.contains("CANDIDATOS ESTADÍSTICOS LOCALES"));
        assert!(prompt.contains("JUAN PEREZ - 200016001075202599999"));
        assert!(prompt.contains("Juzgado 3 Cto Penal"));
    }

    #[test]
    fn test_fase4_diagnostico_export_enriquecido_y_harness_prequential() {
        use crate::aprendizaje::{
            aplicar_analisis_cluster_en, cargar_memoria_en, exportar_aprendizaje_con_catalogos_en,
            obtener_cluster_listo_para_inducir_en, obtener_diagnostico_aprendizaje_en,
            registrar_resultado_con_traza_en, AnalisisCorreccionIa, NOMBRE_ARCHIVO_MEMORIA,
        };
        use crate::models::AudienciaInput;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("lokus_fase4_e2e_{}", stamp));
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        let descargas = dir.join("descargas");
        fs::create_dir_all(&dir).unwrap();

        // 1) Evaluación offline prequential (arranque en frío vs pos-aprendizaje)
        //    Simulamos un LLM ruidoso que al inicio devuelve "" en realizada/motivo para casos locales,
        //    midiendo mejora del árbitro híbrido tras aprender de las primeras iteraciones.
        let mut casos_preq = Vec::new();
        for i in 0..8 {
            casos_preq.push((
                format!("Audiencia concentrada proceso {} en Juzgado 3 CTO Penal se efectuó correctamente", i),
                "SI",
                "",
            ));
            casos_preq.push((
                format!("Juicio oral proceso {} en Juzgado 2 Penal se reprogramó porque el INPEC no trasladó", i),
                "NO",
                "INPEC",
            ));
        }

        let mut f1_frio_aciertos = 0usize;
        let mut f1_post_aciertos = 0usize;
        let priors = IndicePriorsExcel::default();

        for (idx, (texto, real_esperada, mot_esperado)) in casos_preq.iter().enumerate() {
            let mem_actual = cargar_memoria_en(&ruta);
            let motor =
                MotorEstadisticoLocal::entrenar_desde_memorias(memoria_base_embebida(), &mem_actual);
            // LLM parcial/indeciso para medir aporte estadístico local
            let raw_parcial = json!({
                "radicado": format!("CARLOS MENDOZA - 20001600107520250088{}", idx % 10),
                "tipo_audiencia": "Audiencia concentrada",
                "fecha": "20/08/2025",
                "hora": "09",
                "minuto": "00",
                "juzgado": "Juzgado 3 CTO Penal",
                "se_realizo": if idx < 4 { "SI" } else { "" },
                "motivo": "",
                "observaciones": ""
            });
            let pred_out = arbitrar_salida_hibrida(
                texto,
                &raw_parcial,
                &mem_actual,
                &priors,
                Vec::new(),
                Vec::new(),
                &motor,
            );
            let correcto = pred_out.se_realizo == *real_esperada
                && (*real_esperada == "SI" || pred_out.motivo == *mot_esperado);
            if idx < 4 {
                if correcto {
                    f1_frio_aciertos += 1;
                }
            } else if idx >= 10 && correcto {
                f1_post_aciertos += 1;
            }

            let pred_in = AudienciaInput {
                radicado: pred_out.radicado,
                tipo: pred_out.tipo_audiencia,
                fecha: pred_out.fecha,
                hora: "09:00".into(),
                juzgado: pred_out.juzgado,
                realizada: pred_out.se_realizo,
                motivo: pred_out.motivo,
                observaciones: pred_out.observaciones,
            };
            let mut gt = pred_in.clone();
            gt.radicado = "CARLOS MENDOZA - 200016001075202500888".into();
            gt.realizada = (*real_esperada).into();
            gt.motivo = (*mot_esperado).into();

            registrar_resultado_con_traza_en(
                &ruta,
                texto,
                &pred_in,
                &gt,
                "openai",
                "gpt-5-mini",
                &[],
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
                        regla: "Si INPEC no trasladó marcar NO con motivo INPEC".into(),
                        confianza: 0.92,
                        necesita_aclaracion: false,
                        pregunta: String::new(),
                    },
                )
                .unwrap();
            }
        }

        // Tramo final (últimos 6 casos) debe superar ampliamente la línea base inicial
        assert!(
            f1_post_aciertos >= 5,
            "Tramo post-aprendizaje esperados >=5/6 aciertos, hubo {} (frío={})",
            f1_post_aciertos,
            f1_frio_aciertos
        );

        // 2) Diagnóstico completo para desarrollador y exportación enriquecida 100% anonimizada
        let diag = obtener_diagnostico_aprendizaje_en(&ruta);
        assert!(diag.metricas_por_campo.len() == 8);
        assert!(!diag.metricas_por_modelo.is_empty());
        assert!(diag.aciertos_totales > 0);
        assert!(diag.llamadas_induccion_llm_total <= 3);

        let archivo_exp = exportar_aprendizaje_con_catalogos_en(
            &ruta,
            &descargas,
            &["Juzgado 3 CTO Penal CARLOS MENDOZA".into()],
            &[],
        )
        .unwrap();
        let json_txt = fs::read_to_string(&archivo_exp).unwrap();
        let parsed: Value = serde_json::from_str(&json_txt).unwrap();
        assert!(parsed.get("metricas_por_campo").is_some());
        assert!(parsed.get("metricas_por_modelo").is_some());
        assert!(parsed.get("preferencias_estilo").is_some());
        assert!(parsed.get("total_ejemplos_aciertos").is_some());
        assert!(parsed.get("llamadas_induccion_llm_total").is_some());
        assert!(!json_txt.contains("CARLOS MENDOZA"), "PII filtrada en export");
        assert!(!json_txt.contains("200016001075202500888"), "Radicado filtrado en export");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_arbitraje_no_corrompe_llm_valido_con_historial_sesgado_y_tolera_sin_tildes() {
        use crate::aprendizaje::EjemploPositivo;
        use crate::models::{AudienciaInput, ProcesoHabitual};

        // Historial fuertemente desbalanceado: 10 SI y 6 NO todos "Fiscalía"
        let mut mem = MemoriaAprendizaje::default();
        for i in 0..10 {
            mem.ejemplos_aciertos.push(EjemploPositivo {
                id: format!("si-{}", i),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_anonimizado: "Audiencia de juicio oral instalada normalmente en juzgado penal".into(),
                salida_anonimizada: AudienciaInput {
                    radicado: "PROC - 202500001".into(),
                    tipo: "Audiencia de juicio oral".into(),
                    fecha: "10/08/2025".into(),
                    hora: "09:00".into(),
                    juzgado: "Juzgado 1 Penal".into(),
                    realizada: "SI".into(),
                    motivo: String::new(),
                    observaciones: String::new(),
                },
                reglas_inyectadas: Vec::new(),
            });
        }
        for i in 0..6 {
            mem.ejemplos_aciertos.push(EjemploPositivo {
                id: format!("fisc-{}", i),
                registrada: String::new(),
                proveedor: "openai".into(),
                modelo: "gpt-5-mini".into(),
                defensor_clave: String::new(),
                texto_anonimizado: "Audiencia aplazada porque la fiscal no asistio a sala".into(),
                salida_anonimizada: AudienciaInput {
                    radicado: "PROC - 202500002".into(),
                    tipo: "Audiencia de juicio oral".into(),
                    fecha: "10/08/2025".into(),
                    hora: "14:00".into(),
                    juzgado: "Juzgado 1 Penal".into(),
                    realizada: "NO".into(),
                    motivo: "Fiscalía".into(),
                    observaciones: String::new(),
                },
                reglas_inyectadas: Vec::new(),
            });
        }

        let mut priors = IndicePriorsExcel::default();
        priors.procesos_por_numero.insert(
            "200016001075202577777".into(),
            ProcesoHabitual {
                radicado_completo: "MARIA TORRES - 20001-60-01075-2025-77777".into(),
                juzgado_habitual: "Juzgado 2 Cto Penal".into(),
                tipo_habitual: "Audiencia preparatoria".into(),
                defensor_habitual: String::new(),
                frecuencia: 2,
            },
        );

        let motor = MotorEstadisticoLocal::entrenar_desde_memorias(&MemoriaAprendizaje::default(), &mem);

        // 1) Caso nuevo donde el LLM devuelve válidamente NO + Juez con redacción no listada en inferir_motivos:
        //    el clasificador sesgado hacia SI / Fiscalía NO debe sobrescribir la salida válida del LLM.
        let raw_llm = json!({
            "radicado": "20001-60-01075-2025-77777",
            "tipo_audiencia": "",
            "fecha": "22/08/2025",
            "hora": "03",
            "minuto": "15",
            "juzgado": "",
            "se_realizo": "NO",
            "motivo": "Juez",
            "observaciones": ""
        });
        let out = arbitrar_salida_hibrida(
            "Audiencia de juicio oral en juzgado penal 20001-60-01075-2025-77777. El titular tuvo cita medica impostergable.",
            &raw_llm,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
        );
        assert_eq!(out.se_realizo, "NO", "No debe sobrescribir NO válido con SI");
        assert_eq!(out.motivo, "Juez", "No debe sobrescribir Juez válido con Fiscalía");
        assert_eq!(out.radicado, "MARIA TORRES - 20001-60-01075-2025-77777");
        assert_eq!(out.juzgado, "Juzgado 2 Cto Penal");
        assert_eq!(out.tipo_audiencia, "Audiencia preparatoria");

        // 2) Texto sin tildes debe respetar jerarquía Defensoría (Fiscalía > Defensor Público)
        let (mot_sin_tilde, sec_sin_tilde) =
            inferir_motivos("La fiscal no se conecto y por eso solicite reprogramar la diligencia");
        assert_eq!(mot_sin_tilde, "Fiscalía");
        assert!(sec_sin_tilde.contains(&"Defensor Público".to_string()));
    }

    #[test]
    fn test_harness_prequential_multiclase_macro_f1_entidades_y_reduccion_llm() {
        use crate::aprendizaje::{
            aplicar_analisis_cluster_en, cargar_memoria_en,
            obtener_cluster_listo_para_inducir_en, registrar_resultado_con_traza_en,
            AnalisisCorreccionIa, NOMBRE_ARCHIVO_MEMORIA,
        };
        use crate::models::AudienciaInput;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("lokus_preq_multi_{}", stamp));
        let ruta = dir.join(NOMBRE_ARCHIVO_MEMORIA);
        fs::create_dir_all(&dir).unwrap();

        // Banco sintético anonimizado multi-clase (SI + 4 causales distintas + normalización juzgado/tipo)
        let plantilla_casos: Vec<(&str, &str, &str, &str, &str)> = vec![
            (
                "Audiencia concentrada en Juzgado 3 CTO Penal [PROCESADO] efectuada con normalidad",
                "SI",
                "",
                "Juzgado 3 Cto Penal",
                "Audiencia concentrada",
            ),
            (
                "Juicio oral en Juzgado 2 Penal [PROCESADO] aplazado porque INPEC no realizo traslado",
                "NO",
                "INPEC",
                "Juzgado 2 Penal",
                "Audiencia de juicio oral",
            ),
            (
                "Preparatoria en Juzgado 1 Penal [PROCESADO] suspendida porque fiscalia solicito preacuerdo",
                "NO",
                "Fiscalía",
                "Juzgado 1 Penal",
                "Audiencia preparatoria",
            ),
            (
                "Control de legalidad en Juzgado 4 MPAL [PROCESADO] no asistio abogado contractual",
                "NO",
                "Defensor Confianza",
                "Juzgado 4 MPAL",
                "Audiencia de control de legalidad",
            ),
            (
                "Acusacion en Juzgado 5 Penal [PROCESADO] fracasa porque titular esta en comision",
                "NO",
                "Juez",
                "Juzgado 5 Penal",
                "Audiencia de acusación",
            ),
            (
                "Verificacion en Juzgado 4 MPAL [PROCESADO] efectuada correctamente",
                "SI",
                "",
                "Juzgado 4 MPAL",
                "Audiencia de verificación de cumplimiento",
            ),
        ];

        let mut preds_frio: Vec<(String, String)> = Vec::new();
        let mut reals_frio: Vec<(String, String)> = Vec::new();
        let mut preds_post: Vec<(String, String)> = Vec::new();
        let mut reals_post: Vec<(String, String)> = Vec::new();
        let mut juz_exactos_post = 0usize;
        let mut total_post = 0usize;
        let mut total_correcciones_registradas = 0usize;
        let priors = IndicePriorsExcel::default();

        // 6 rondas prequential (36 casos) simulando un LLM que al inicio expande mal CTO/MPAL y omite realizada/motivo
        for ronda in 0..6 {
            for (idx_c, (texto, real_gt, mot_gt, juz_gt, tipo_gt)) in
                plantilla_casos.iter().enumerate()
            {
                let mem_actual = cargar_memoria_en(&ruta);
                let motor = MotorEstadisticoLocal::entrenar_desde_memorias(
                    memoria_base_embebida(),
                    &mem_actual,
                );
                let raw_juz = match *juz_gt {
                    "Juzgado 3 Cto Penal" => "Juzgado 3 Circuito Penal",
                    "Juzgado 4 MPAL" => "Juzgado 4 Municipal",
                    other => other,
                };
                let raw_llm = json!({
                    "radicado": "[PROCESADO] - [RADICADO]",
                    "tipo_audiencia": *tipo_gt,
                    "fecha": "18/08/2025",
                    "hora": "09",
                    "minuto": "00",
                    "juzgado": raw_juz,
                    "se_realizo": "",
                    "motivo": "",
                    "observaciones": ""
                });
                let out = arbitrar_salida_hibrida(
                    &format!("{} caso r{} c{}", texto, ronda, idx_c),
                    &raw_llm,
                    &mem_actual,
                    &priors,
                    Vec::new(),
                    Vec::new(),
                    &motor,
                );

                if ronda == 0 {
                    preds_frio.push((out.se_realizo.clone(), out.motivo.clone()));
                    reals_frio.push(((*real_gt).into(), (*mot_gt).into()));
                } else if ronda == 5 {
                    preds_post.push((out.se_realizo.clone(), out.motivo.clone()));
                    reals_post.push(((*real_gt).into(), (*mot_gt).into()));
                    total_post += 1;
                    if out.juzgado == *juz_gt {
                        juz_exactos_post += 1;
                    }
                }

                let pred_in = AudienciaInput {
                    radicado: out.radicado,
                    tipo: out.tipo_audiencia,
                    fecha: out.fecha,
                    hora: "09:00".into(),
                    juzgado: out.juzgado,
                    realizada: out.se_realizo,
                    motivo: out.motivo,
                    observaciones: out.observaciones,
                };
                let gt_in = AudienciaInput {
                    radicado: "[PROCESADO] - [RADICADO]".into(),
                    tipo: (*tipo_gt).into(),
                    fecha: "18/08/2025".into(),
                    hora: "09:00".into(),
                    juzgado: (*juz_gt).into(),
                    realizada: (*real_gt).into(),
                    motivo: (*mot_gt).into(),
                    observaciones: String::new(),
                };

                let (res_reg, _) = registrar_resultado_con_traza_en(
                    &ruta,
                    &format!("{} caso r{} c{}", texto, ronda, idx_c),
                    &pred_in,
                    &gt_in,
                    "openai",
                    "gpt-5-mini",
                    &[],
                )
                .unwrap();
                if res_reg.hubo_correccion {
                    total_correcciones_registradas += 1;
                }

                if let Some((cid, corrs)) = obtener_cluster_listo_para_inducir_en(&ruta) {
                    let ids: Vec<String> = corrs.iter().map(|c| c.id.clone()).collect();
                    aplicar_analisis_cluster_en(
                        &ruta,
                        &cid,
                        &ids,
                        &AnalisisCorreccionIa {
                            es_error_de_ia: true,
                            tipo_error: "general".into(),
                            regla: format!("Regla inducida para clúster {}", cid),
                            confianza: 0.9,
                            necesita_aclaracion: false,
                            pregunta: String::new(),
                        },
                    )
                    .unwrap();
                }
            }
        }

        let macro_f1_etiqueta = |preds: &[(String, String)], reals: &[(String, String)]| -> f64 {
            let clases = ["SI", "INPEC", "Fiscalía", "Defensor Confianza", "Juez"];
            let label = |p: &(String, String)| -> String {
                if p.0 == "SI" {
                    "SI".into()
                } else {
                    p.1.clone()
                }
            };
            let mut sum = 0.0;
            for cl in clases {
                let tp = preds
                    .iter()
                    .zip(reals)
                    .filter(|(p, r)| label(p) == cl && label(r) == cl)
                    .count() as f64;
                let fp = preds
                    .iter()
                    .zip(reals)
                    .filter(|(p, r)| label(p) == cl && label(r) != cl)
                    .count() as f64;
                let fn_ = preds
                    .iter()
                    .zip(reals)
                    .filter(|(p, r)| label(p) != cl && label(r) == cl)
                    .count() as f64;
                let denom = 2.0 * tp + fp + fn_;
                if denom > 0.0 {
                    sum += (2.0 * tp) / denom;
                }
            }
            sum / clases.len() as f64
        };

        let f1_frio = macro_f1_etiqueta(&preds_frio, &reals_frio);
        let f1_post = macro_f1_etiqueta(&preds_post, &reals_post);
        assert!(
            f1_post > f1_frio && f1_post >= 0.80,
            "Macro-F1 debe mejorar tras aprendizaje prequential: frio={} post={}",
            f1_frio,
            f1_post
        );
        assert_eq!(juz_exactos_post, total_post, "Alias CTO aprendido debe resolver 100% en ronda final");

        let mem_fin = cargar_memoria_en(&ruta);
        let reduccion = 1.0
            - (mem_fin.llamadas_induccion_llm_total as f64
                / total_correcciones_registradas.max(1) as f64);
        assert!(
            reduccion >= 0.70,
            "Reducción LLM esperada >= 70%, obtenida {:.1}% (llamadas={}, correcciones={})",
            reduccion * 100.0,
            mem_fin.llamadas_induccion_llm_total,
            total_correcciones_registradas
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_contrato_api_formulario_esquema_catalogo_tildes_y_vacios() {
        // 1) Esquemas estructurados nativos para OpenAI (strict) y Gemini (sin additionalProperties)
        let oa = esquema_extraccion_audiencia_openai();
        assert_eq!(oa["type"], "json_schema");
        assert_eq!(oa["json_schema"]["strict"], true);
        assert_eq!(oa["json_schema"]["schema"]["additionalProperties"], false);
        assert_eq!(
            oa["json_schema"]["schema"]["properties"]["se_realizo"]["enum"],
            json!(["SI", "NO", ""])
        );
        assert!(oa["json_schema"]["schema"]["properties"]["motivo"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("Defensor Confianza")));

        let gm = esquema_extraccion_audiencia_gemini();
        assert_eq!(gm["type"], "OBJECT");
        assert!(gm.get("additionalProperties").is_none());
        assert_eq!(gm["properties"]["se_realizo"]["enum"], json!(["SI", "NO", ""]));

        // Detección de error 400 por esquema no soportado para respaldo automático
        assert!(es_error_esquema_no_soportado(
            reqwest::StatusCode::BAD_REQUEST,
            "Invalid parameter: response_format json_schema is not supported on this model"
        ));
        assert!(!es_error_esquema_no_soportado(
            reqwest::StatusCode::UNAUTHORIZED,
            "Invalid API key"
        ));

        // 2) Prompt incluye los 17 tipos oficiales + tipos personalizados del usuario
        let prompt = construir_prompt_con_candidatos_y_catalogo(
            "Audiencia verificacion de cumplimiento",
            2026,
            "",
            None,
            &["Audiencia de sustitución de medida".into()],
        );
        assert!(prompt.contains("Audiencia de verificación de cumplimiento"));
        assert!(prompt.contains("Audiencia de incidente de reparación integral"));
        assert!(prompt.contains("- Audiencia de sustitución de medida"));
        assert!(!prompt.contains("(personalizado del usuario)"));
        assert!(!prompt.contains("Alegatos de conclusión, etc."));

        // Parseo tolerante cuando el modelo de respaldo envuelve JSON en texto breve
        let json_envuelto = parsear_json_limpio("Resultado:\n```json\n{\"se_realizo\":\"SI\"}\n```").unwrap();
        assert_eq!(json_envuelto["se_realizo"], "SI");

        // 3) Empate insensible a tildes en tipos y juzgados + limpieza de placeholders opcionales
        let mem = MemoriaAprendizaje::default();
        let priors = IndicePriorsExcel::default();
        let motor = MotorEstadisticoLocal::entrenar_desde_memorias(
            &MemoriaAprendizaje::default(),
            &mem,
        );
        let raw_sin_tildes = json!({
            "radicado": "CARLOS GOMEZ - 200016001075202600010",
            "tipo_audiencia": "Audiencia de verificacion de cumplimiento",
            "fecha": "15/08/2026",
            "hora": "No especificado",
            "minuto": "N/A",
            "juzgado": "Juzgado 1 Penal Municipal con Funcion de Control de Garantias de Valledupar",
            "se_realizo": "SI",
            "motivo": "Ninguno",
            "observaciones": "No especificado"
        });
        let out = arbitrar_salida_hibrida_con_catalogos(
            "Audiencia de verificacion de cumplimiento sin novedad",
            &raw_sin_tildes,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &["Audiencia de sustitución de medida".into()],
            &[],
        );
        assert_eq!(out.tipo_audiencia, "Audiencia de verificación de cumplimiento");
        assert_eq!(out.tipo_otra, "");
        assert_eq!(
            out.juzgado,
            "Juzgado 1 Penal Municipal con Función de Control de Garantías de Valledupar"
        );
        assert_eq!(out.hora, "");
        assert_eq!(out.minuto, "");
        assert_eq!(out.motivo, "");
        assert_eq!(out.observaciones, "");

        // Tipo personalizado del usuario sin tilde también resuelve exacto y "Otra" literal limpia a ""
        assert_eq!(
            mapear_tipo_audiencia_con_usuario(
                "audiencia de sustitucion de medida",
                &["Audiencia de sustitución de medida".into()]
            ),
            ("Audiencia de sustitución de medida".into(), "".into())
        );
        assert_eq!(
            mapear_tipo_audiencia_con_usuario("Otra", &[]),
            ("".into(), "".into())
        );

        // No debe fusionar variantes jurídicas reales en juzgado
        assert_eq!(
            normalizar_juzgado_catalogo("Juzgado 2 Penal", &[]),
            "Juzgado 2 Penal"
        );
        assert_eq!(
            normalizar_juzgado_catalogo("Juzgado Segundo Penal", &[]),
            "Juzgado Segundo Penal"
        );
    }

    #[test]
    fn test_typos_reales_hora_irregular_instalada_adaptable_y_sigla_por_defensor() {
        // 1) Typos frecuentes y puntuación irregular de horas (inspirado en muestras reales)
        let pre = expandir_abreviaciones_y_compacto(
            "DESPCHO DE CONOCIEMIENTO VALLEDUPARA. SE INSTLO PREPARATORI PERO SOLCITO REPRGRAMACION DE DILIGNCIA. HORA. 12:00 / HORA; 8:00 / HORA: 2.30",
            2025,
        );
        assert!(pre.contains("Despacho"), "{}", pre);
        assert!(pre.contains("Conocimiento"), "{}", pre);
        assert!(pre.contains("Valledupar"), "{}", pre);
        assert!(pre.contains("Instaló"), "{}", pre);
        assert!(pre.contains("Preparatoria"), "{}", pre);
        assert!(pre.contains("Solicitó"), "{}", pre);
        assert!(pre.contains("Reprogramación"), "{}", pre);
        assert!(pre.contains("Diligencia"), "{}", pre);
        assert!(pre.contains("HORA: 12:00"), "{}", pre);
        assert!(pre.contains("HORA: 8:00"), "{}", pre);
        assert!(pre.contains("HORA: 2:30"), "{}", pre);

        // 2) "Se instaló pero se suspendió/reprogramó": adaptable por defensor en 2 correcciones,
        //    aislado de otros defensores, y NUNCA confunde "No se instaló"
        let mut mem = MemoriaAprendizaje::default();
        let priors = IndicePriorsExcel::default();
        let motor = MotorEstadisticoLocal::entrenar_desde_memorias(&MemoriaAprendizaje::default(), &mem);

        let texto_instalada =
            "Audiencia preparatoria en J10: se instaló la diligencia pero se suspendió y se reprogramó para el 20/08/2025";
        let raw_si = json!({
            "radicado": "PROCESADO - 200016001075202500555",
            "tipo_audiencia": "Audiencia preparatoria",
            "fecha": "18/08/2025",
            "hora": "09",
            "minuto": "00",
            "juzgado": "J10",
            "se_realizo": "SI",
            "motivo": "",
            "observaciones": "Se instaló y se reprogramó continuación."
        });

        // Por defecto (antes de aprender): fuerza NO
        let out_default = arbitrar_salida_hibrida_con_catalogos_y_defensor(
            texto_instalada,
            &raw_si,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &[],
            &[],
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(out_default.se_realizo, "NO");

        // Entrenar 2 correcciones de Ana marcando SI + asociando J10 -> Juzgado 10 Penal Municipal
        let pred_no = crate::models::AudienciaInput {
            radicado: "PROCESADO - 200016001075202500555".into(),
            tipo: "Audiencia preparatoria".into(),
            fecha: "18/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "J10".into(),
            realizada: "NO".into(),
            motivo: "Juez".into(),
            observaciones: String::new(),
        };
        let fin_si = crate::models::AudienciaInput {
            juzgado: "Juzgado 10 Penal Municipal de Valledupar".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            ..pred_no.clone()
        };
        let campos_corr = crate::aprendizaje::detectar_correcciones(&pred_no, &fin_si);
        for _ in 0..2 {
            crate::estadistica_ia::actualizar_alias_y_preferencias_en_memoria_para_defensor(
                &mut mem,
                texto_instalada,
                &pred_no,
                &fin_si,
                &campos_corr,
                "2025-08-18T10:00:00-05:00",
                Some("Dra. Ana Pérez"),
            );
        }

        // Ahora para Ana: respeta SI y resuelve J10 -> Juzgado 10 Penal Municipal de Valledupar
        let out_ana = arbitrar_salida_hibrida_con_catalogos_y_defensor(
            texto_instalada,
            &raw_si,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &[],
            &[],
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(out_ana.se_realizo, "SI");
        assert_eq!(out_ana.juzgado, "Juzgado 10 Penal Municipal de Valledupar");

        // Para otro defensor (Luis): sigue marcando NO y NO recibe el alias privado J10 de Ana
        let out_luis = arbitrar_salida_hibrida_con_catalogos_y_defensor(
            texto_instalada,
            &raw_si,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &[],
            &[],
            Some("Dr. Luis Mora"),
        );
        assert_eq!(out_luis.se_realizo, "NO");
        assert_eq!(out_luis.juzgado, "J10");

        // Caso límite de negación: "No se instaló...", "no pudo ser instalada", "nunca se instaló"
        // y "no se instaló y se fija..." SIEMPRE deben marcar NO incluso para Ana
        for texto_neg in [
            "Audiencia preparatoria en J10: no se instaló la diligencia y se reprogramó para el 25/08/2025",
            "Audiencia preparatoria en J10: no pudo ser instalada por ausencia del juez y se reprogramó",
            "Audiencia preparatoria en J10: nunca se instaló y no quedó instalada, se fija fecha para mayo",
            "No se instaló la diligencia y se fija para el 10/09/2025",
            "SE SOLCITO REPRGRAMACION POR AGENDA DEL DESPCHO",
        ] {
            let out_neg = arbitrar_salida_hibrida_con_catalogos_y_defensor(
                texto_neg,
                &raw_si,
                &mem,
                &priors,
                Vec::new(),
                Vec::new(),
                &motor,
                &[],
                &[],
                Some("Dra. Ana Pérez"),
            );
            assert_eq!(
                out_neg.se_realizo, "NO",
                "caso '{}' jamás debe tratarse como SI",
                texto_neg
            );
        }

        // Invariante CTO/MPAL/Circuito: si el LLM devolvió "J10 CTO" o "J10 Circuito",
        // NO se sobrescribe con el alias de sigla desnuda J10 -> Municipal
        let raw_cto = json!({
            "radicado": "PROCESADO - 200016001075202500555",
            "tipo_audiencia": "Audiencia preparatoria",
            "fecha": "18/08/2025",
            "hora": "09",
            "minuto": "00",
            "juzgado": "J10 CTO",
            "se_realizo": "SI",
            "motivo": "",
            "observaciones": ""
        });
        let out_cto = arbitrar_salida_hibrida_con_catalogos_y_defensor(
            "Audiencia en J10 CTO realizada sin novedad",
            &raw_cto,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &[],
            &[],
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(out_cto.juzgado, "J10 CTO");

        // Aislamiento de alto q10 vía MotorEstadisticoLocal: si LLM devuelve juzgado vacío para Luis,
        // el alias de alta confianza (>= 0.55) de Ana NO debe filtrarse a Luis
        for _ in 0..6 {
            crate::estadistica_ia::actualizar_alias_y_preferencias_en_memoria_para_defensor(
                &mut mem,
                "Audiencia en J10 preparatoria",
                &pred_no,
                &fin_si,
                &campos_corr,
                "2025-08-18T10:00:00-05:00",
                Some("Dra. Ana Pérez"),
            );
        }
        let raw_juz_vacio = json!({
            "radicado": "OTRO - 470016001075202500888",
            "tipo_audiencia": "Audiencia preparatoria",
            "fecha": "19/08/2025",
            "hora": "10",
            "minuto": "00",
            "juzgado": "",
            "se_realizo": "SI",
            "motivo": "",
            "observaciones": ""
        });
        let motor_luis = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
            &MemoriaAprendizaje::default(),
            &mem,
            Some("Dr. Luis Mora"),
        );
        let out_luis_vacio = arbitrar_salida_hibrida_con_catalogos_y_defensor(
            "Audiencia en J10 preparatoria",
            &raw_juz_vacio,
            &mem,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor_luis,
            &[],
            &[],
            Some("Dr. Luis Mora"),
        );
        assert_eq!(
            out_luis_vacio.juzgado, "",
            "Alias privado de alta confianza de Ana no debe filtrarse a Luis cuando juzgado viene vacío"
        );

        // 3) Prompt incluye bloque de siglas resueltas del Mapa de Conexiones
        let cands = CandidatosPrompt {
            juzgados_top: vec!["Juzgado 10 Penal Municipal de Valledupar".into()],
            tipos_top: vec!["Audiencia preparatoria".into()],
            radicado_vinculado: None,
            siglas_resueltas: vec![(
                "j10".into(),
                "Juzgado 10 Penal Municipal de Valledupar".into(),
            )],
        };
        let prompt = construir_prompt_con_candidatos(texto_instalada, 2025, "", Some(&cands));
        assert!(prompt.contains("Siglas habituales de este defensor (Mapa de Conexiones)"));
        assert!(prompt.contains("J10 => Juzgado 10 Penal Municipal de Valledupar"));
    }

    #[test]
    fn test_pilar2_esquemas_openai_gemini_consolidacion_resumenes_y_cluster_sin_sesgo() {
        use crate::aprendizaje::{
            CampoCorregido, CorreccionLoteConsolidacion, LoteConsolidacion,
            PatronTransversalExport, PerfilSinteticoDefensor, TelemetriaDisparoConsolidacion,
        };
        use crate::models::AudienciaInput;

        // 1) Validación recursiva de esquema OpenAI strict=true y esquema Gemini sin additionalProperties
        fn validar_openai_strict(v: &Value) {
            if let Some(obj) = v.as_object() {
                if obj.get("type").and_then(|t| t.as_str()) == Some("object") {
                    assert_eq!(
                        obj.get("additionalProperties"),
                        Some(&json!(false)),
                        "Falta additionalProperties: false en {:?}",
                        obj
                    );
                    if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
                        let req = obj
                            .get("required")
                            .and_then(|r| r.as_array())
                            .expect("objeto debe definir required");
                        for k in props.keys() {
                            assert!(req.contains(&json!(k)), "Falta '{}' en required", k);
                        }
                    }
                }
                for val in obj.values() {
                    validar_openai_strict(val);
                }
            } else if let Some(arr) = v.as_array() {
                for val in arr {
                    validar_openai_strict(val);
                }
            }
        }
        fn validar_gemini_sin_additional(v: &Value) {
            if let Some(obj) = v.as_object() {
                assert!(
                    !obj.contains_key("additionalProperties"),
                    "Gemini no soporta additionalProperties: {:?}",
                    obj
                );
                for val in obj.values() {
                    validar_gemini_sin_additional(val);
                }
            } else if let Some(arr) = v.as_array() {
                for val in arr {
                    validar_gemini_sin_additional(val);
                }
            }
        }

        let oa = esquema_consolidacion_openai();
        assert_eq!(oa["type"], "json_schema");
        assert_eq!(oa["json_schema"]["strict"], true);
        validar_openai_strict(&oa["json_schema"]["schema"]);

        let gm = esquema_consolidacion_gemini();
        assert_eq!(gm["type"], "OBJECT");
        validar_gemini_sin_additional(&gm);

        // 2) Prompt de consolidación con ámbito real de defensor y patrones transversales
        let lote = LoteConsolidacion {
            ambito: "ana perez".into(),
            nombre_ambito: "Dra. Ana Pérez".into(),
            resumen_actual: "Ana prefiere conservar CTO".into(),
            version_resumen: 1,
            reglas_ambito: Vec::new(),
            correcciones_recientes: vec![CorreccionLoteConsolidacion {
                id: "c1".into(),
                fecha: "2025-08-20".into(),
                defensor_clave: "ana perez".into(),
                campo_principal: "juzgado".into(),
                texto_anonimizado: "Audiencia de [PROCESADO] rad [RADICADO] en Juzgado 3 CTO".into(),
                cambios: Vec::new(),
                nota_usuario: "Mantener CTO".into(),
                estado: "pendiente".into(),
            }],
            metricas_resumen: Vec::new(),
            patrones_transversales: vec![PatronTransversalExport {
                firma: "motivo:Fiscalía".into(),
                campo: "motivo".into(),
                ocurrencias: 3,
                defensores_distintos: 2,
                ejemplo_anonimizado: "[PROCESADO] fiscal no vino".into(),
            }],
            watermark_ids: vec!["c1".into()],
            snapshot_extracciones: 6,
            telemetria_disparo: TelemetriaDisparoConsolidacion::default(),
        };
        let prompt_cons = construir_prompt_consolidacion(&lote);
        assert!(prompt_cons.contains("ana perez"));
        assert!(prompt_cons.contains("Dra. Ana Pérez"));
        assert!(prompt_cons.contains("[PROCESADO]"));
        assert!(prompt_cons.contains("PATRONES TRANSVERSALES"));

        // 3) Prompt de extracción inyecta resúmenes consolidados general + defensor
        let prompt_ext = construir_prompt_con_candidatos_y_resumenes(
            "Audiencia preparatoria en J10",
            2025,
            "",
            None,
            &[],
            Some("Criterio general consolidado"),
            Some("Perfil sintético de Ana"),
        );
        assert!(prompt_ext.contains("CRITERIOS GENERALES CONSOLIDADOS (TRANSVERSALES)"));
        assert!(prompt_ext.contains("Criterio general consolidado"));
        assert!(prompt_ext.contains("PERFIL SINTÉTICO DEL DEFENSOR (ESPECÍFICO)"));
        assert!(prompt_ext.contains("Perfil sintético de Ana"));

        // 4) Prompt de clúster no pre-sesga literal `"necesita_aclaracion": false`
        let base_aud = AudienciaInput {
            radicado: "P - 200016001075202500001".into(),
            tipo: "Audiencia preparatoria".into(),
            fecha: "15/08/2025".into(),
            hora: "09:00".into(),
            juzgado: "Juzgado 1 Penal".into(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };
        let corr_ex = CorreccionIa {
            id: "c1".into(),
            registrada: String::new(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_original: "Texto 1".into(),
            prediccion: base_aud.clone(),
            final_usuario: base_aud.clone(),
            campos: vec![CampoCorregido {
                campo: "juzgado".into(),
                valor_ia: "J1".into(),
                valor_usuario: "J2".into(),
            }],
            estado: "pendiente".into(),
            regla_generada: String::new(),
            regla_id_asociada: String::new(),
            pregunta_ia: String::new(),
            nota_usuario: String::new(),
            exportada: false,
            defensor_clave: String::new(),
        };
        let prompt_cluster = construir_prompt_analisis_cluster(&[corr_ex.clone(), corr_ex]);
        assert!(!prompt_cluster.contains("\"necesita_aclaracion\": false,"));

        // 5) Arbitraje con `memoria_base` (Brechas B y H) aplica preferencia sintética heredada desde `base`
        let mut base = MemoriaAprendizaje::default();
        let mut pref_ana = PreferenciasEstilo::default();
        pref_ana.instalada_reprogramada_si_alpha = 6.0;
        pref_ana.instalada_reprogramada_si_beta = 1.0;
        base.preferencias_por_defensor.insert("Dra. Ana Pérez".into(), pref_ana);
        base.perfiles_sinteticos.insert(
            "ana perez".into(),
            PerfilSinteticoDefensor {
                ambito: "ana perez".into(),
                nombre_mostrar: "Dra. Ana Pérez".into(),
                resumen: "Instalada luego reprogramada cuenta como SI".into(),
                version: 1,
                ultima_actualizacion: String::new(),
            },
        );
        let local = MemoriaAprendizaje::default();
        let priors = IndicePriorsExcel::default();
        let motor = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
            &base,
            &local,
            Some("Dra. Ana Pérez"),
        );
        let raw_si = json!({
            "radicado": "PROC - 200016001075202500555",
            "tipo_audiencia": "Audiencia preparatoria",
            "fecha": "18/08/2025",
            "hora": "09",
            "minuto": "00",
            "juzgado": "J10",
            "se_realizo": "SI",
            "motivo": "",
            "observaciones": ""
        });
        let out_b = arbitrar_salida_hibrida_con_base_catalogos_y_defensor(
            "Se instaló la audiencia preparatoria pero luego se reprogramó",
            &raw_si,
            &base,
            &local,
            &priors,
            Vec::new(),
            Vec::new(),
            &motor,
            &[],
            &[],
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(out_b.se_realizo, "SI");
    }
}
