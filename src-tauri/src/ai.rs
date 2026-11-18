use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::aprendizaje::{obtener_contexto_prompt, AnalisisCorreccionIa, CorreccionIa};
use crate::config::cargar_settings;
use crate::models::{mapear_motivo_a_indice, MOTIVOS_LABELS, TIPOS_AUDIENCIA};

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

fn detectar_primera_persona_defensor_publico(texto_lower: &str) -> bool {
    let patrones = [
        "solicité",
        "pedí",
        "solicito",
        "pido",
        "soliciré",
        "no pude",
        "no he tenido",
        "no logré",
        "me fue imposible",
        "no he logrado",
        "no había podido",
        "realicé conexión",
    ];
    patrones.iter().any(|p| texto_lower.contains(p))
}

fn inferir_motivos(texto_original: &str) -> (String, Vec<String>) {
    let texto_lower = texto_original.to_lowercase();
    let es_confianza = detectar_defensor_confianza(&texto_lower);
    let es_primera_persona = detectar_primera_persona_defensor_publico(&texto_lower);

    let frases_motivos: &[(&str, &str)] = &[
        // Juez (mayor prioridad en problemas del despacho)
        ("el juez entró en periodo de vacaciones", "Juez"),
        ("vacaciones", "Juez"),
        ("no habían nombrado titular de despacho", "Juez"),
        ("titular de despacho", "Juez"),
        ("juez no compareció", "Juez"),
        ("ausencia del juez", "Juez"),
        ("juzgado cerrado", "Juez"),
        ("juez en comisión", "Juez"),
        ("despacho en otra audiencia", "Juez"),
        ("depacho en otra audiencia", "Juez"),
        ("estar el despacho en audiencia", "Juez"),
        ("fracsa por estar el despacho", "Juez"),
        ("fracasa por estar el despacho", "Juez"),
        ("ambos despachos en audiencia", "Juez"),
        ("error en la agenda", "Juez"),
        // Fiscalía
        ("la fiscal no se conectó", "Fiscalía"),
        ("fiscal no se conectó", "Fiscalía"),
        ("fiscal fue reemplazad", "Fiscalía"),
        ("fiscal fue reemplazdao", "Fiscalía"),
        ("no conocer la carpeta", "Fiscalía"),
        ("por solicitud de la fiscalía", "Fiscalía"),
        ("solicitud de fiscalía", "Fiscalía"),
        ("solicitud de la fiscalía", "Fiscalía"),
        ("fiscalía no compareció", "Fiscalía"),
        ("la fiscalía solicitó reprogramación", "Fiscalía"),
        ("fiscal solicitó reprogramación", "Fiscalía"),
        ("fiscalía solicitó reprogramación", "Fiscalía"),
        ("la fiscal no vino", "Fiscalía"),
        ("fiscal no vino", "Fiscalía"),
        ("la fiscal no asistió", "Fiscalía"),
        ("fiscal no asistió", "Fiscalía"),
        ("ausencia de la fiscal", "Fiscalía"),
        ("ausencia del fiscal", "Fiscalía"),
        ("posible preacuerdo", "Fiscalía"),
        // Defensor Confianza (abogado contractual / privado)
        ("defensor de confianza no compareció", "Defensor Confianza"),
        ("abogado no compareció", "Defensor Confianza"),
        ("abogado contractual no compareció", "Defensor Confianza"),
        ("abogado contractual no se presentó", "Defensor Confianza"),
        ("abogado contractual ausente", "Defensor Confianza"),
        ("defensor de confianza ausente", "Defensor Confianza"),
        ("inasistencia del defensor de confianza", "Defensor Confianza"),
        ("inasistencia del abogado contractual", "Defensor Confianza"),
        ("inasistencia del abogado", "Defensor Confianza"),
        ("defensor de confianza solicitó", "Defensor Confianza"),
        ("defensor de confianza solicita", "Defensor Confianza"),
        ("abogado contractual solicitó", "Defensor Confianza"),
        ("abogado contractual pidió", "Defensor Confianza"),
        ("abogado solicitó reprogramación", "Defensor Confianza"),
        ("el abogado privado solicitó", "Defensor Confianza"),
        ("el abogado no se presentó", "Defensor Confianza"),
        ("el abogado del procesado no compareció", "Defensor Confianza"),
        ("falta del defensor de confianza", "Defensor Confianza"),
        ("falta del abogado contractual", "Defensor Confianza"),
        ("ausencia de defensor de confianza", "Defensor Confianza"),
        ("ausencia de abogado contractual", "Defensor Confianza"),
        ("defensor privado no compareció", "Defensor Confianza"),
        ("abogado privado no compareció", "Defensor Confianza"),
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
        ("defensor público no compareció", "Defensor Público"),
        ("inasistencia del defensor público", "Defensor Público"),
        ("defensor público solicitó", "Defensor Público"),
        ("solicité reprogram", "Defensor Público"),
        ("soliciré reprogram", "Defensor Público"),
        ("solicito reprogram", "Defensor Público"),
        ("solicito la reprogram", "Defensor Público"),
        ("pedí reprogram", "Defensor Público"),
        ("pedí la reprogram", "Defensor Público"),
        ("pido reprogram", "Defensor Público"),
        ("pido la reprogram", "Defensor Público"),
        ("pedí aplazamiento", "Defensor Público"),
        ("no pude contactar", "Defensor Público"),
        ("no he tenido contacto", "Defensor Público"),
        ("no había tenido contacto con el procesado", "Defensor Público"),
        ("no he logrado contactar", "Defensor Público"),
        ("me fue imposible", "Defensor Público"),
        ("no logré comunicarme", "Defensor Público"),
        ("realicé conexión virtual", "Defensor Público"),
        // Usuario / Procesado
        ("no notificaron al procesado", "Usuario"),
        ("procesado no compareció", "Usuario"),
        ("usuario no se presentó", "Usuario"),
        ("procesado ausente", "Usuario"),
        ("problemas técnicos", "Usuario"),
        ("fallas de conectividad", "Usuario"),
        // INPEC
        ("inpec no trasladó", "INPEC"),
        ("problemas de traslado", "INPEC"),
        ("inpec reportó", "INPEC"),
        ("inpec", "INPEC"),
        // Víctima
        ("víctima no compareció", "Víctima"),
        ("ausencia de la víctima", "Víctima"),
        // ICBF
        ("icbf no compareció", "ICBF"),
        ("representante icbf ausente", "ICBF"),
        ("icbf", "ICBF"),
    ];

    let mut encontrados: Vec<String> = Vec::new();
    for (frase, motivo) in frases_motivos {
        if texto_lower.contains(frase) && !encontrados.contains(&motivo.to_string()) {
            encontrados.push(motivo.to_string());
        }
    }

    // Patrones genéricos de reprogramación que se reasignan según el contexto (de legacy-python)
    let frases_genericas = [
        "solicita reprogramación",
        "solicitó la reprogramación",
        "reprogramación solicitada",
        "solicitó reprogramar",
    ];
    if encontrados.is_empty() && frases_genericas.iter().any(|f| texto_lower.contains(f)) {
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

fn mapear_tipo_audiencia(tipo_ia: &str) -> (String, String) {
    let limpio = tipo_ia.trim();
    if limpio.is_empty() {
        return (String::new(), String::new());
    }
    let lower = limpio.to_lowercase();

    let mapeo_directo = [
        ("alegatos de conclusión", "Alegatos de conclusión"),
        ("control de legalidad", "Audiencia de control de legalidad"),
        ("concentrada", "Audiencia concentrada"),
        ("juicio oral", "Audiencia de juicio oral"),
        ("inicia juicio", "Audiencia de juicio oral"),
        ("continua juicio", "Audiencia de juicio oral"),
        ("preliminar", "Audiencia preliminar"),
        ("preparatoria", "Audiencia preparatoria"),
        ("acusación", "Audiencia de acusación"),
        ("acusacion", "Audiencia de acusación"),
        ("conciliación", "Audiencia de conciliación"),
        ("preclusión", "Audiencia de preclusión"),
        ("preclusion", "Audiencia de preclusión"),
        (
            "individualización de pena",
            "Audiencia de individualización de pena",
        ),
        ("imputación", "Audiencia de imputación"),
        (
            "reparación integral",
            "Audiencia de incidente de reparación integral",
        ),
        (
            "medidas de aseguramiento",
            "Audiencia de medidas de aseguramiento",
        ),
        ("nulidad", "Audiencia de nulidad"),
        ("prórroga", "Audiencia de prórroga"),
        ("revisión de medida", "Audiencia de revisión de medida"),
        (
            "verificación de cumplimiento",
            "Audiencia de verificación de cumplimiento",
        ),
    ];

    for (clave, oficial) in mapeo_directo {
        if lower.contains(clave) {
            return (oficial.to_string(), String::new());
        }
    }

    for &oficial in &TIPOS_AUDIENCIA {
        if oficial != "Otra" && oficial.eq_ignore_ascii_case(limpio) {
            return (oficial.to_string(), String::new());
        }
    }

    ("Otra".to_string(), limpio.to_string())
}

fn extraer_fechas_reprogramacion(texto: &str) -> String {
    let patrones = [
        r"(?i)reprogramada?\s+para\s+el?\s+d[íi]a\s+(\d{1,2}\s+de\s+[a-záéíóú]+(?:\s+de\s+\d{4})?)",
        r"(?i)reprogramada?\s+para\s+el?\s+(\d{1,2}\s+de\s+[a-záéíóú]+)",
        r"(?i)nueva\s+fecha[:\s]+(\d{1,2}/\d{1,2}/\d{2,4})",
        r"(?i)se\s+fij[óo]\s+(?:nueva\s+fecha\s+)?para\s+el?\s+(\d{1,2}\s+de\s+[a-záéíóú]+)",
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

fn construir_prompt(
    texto_preprocesado: &str,
    current_year: i32,
    contexto_aprendizaje: &str,
) -> String {
    let seccion_aprendizaje = if contexto_aprendizaje.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", contexto_aprendizaje.trim())
    };

    format!(
        r#"Eres un asistente jurídico experto en audiencias judiciales en Colombia, optimizado con patrones reales de defensores públicos.
Analiza el texto (que puede venir en estilo ESTRUCTURADO descriptivo o en estilo COMPACTO telegráfico) y devuelve ÚNICAMENTE un objeto JSON válido con esta estructura exacta:

{{
  "radicado": "NOMBRE_PROCESADO - NÚMERO_RADICADO (si hay ambos) o el identificador/radicado encontrado",
  "tipo_audiencia": "Tipo de audiencia (ej: Audiencia concentrada, Audiencia de juicio oral, Audiencia preparatoria, Audiencia de acusación, Audiencia de control de legalidad, Alegatos de conclusión, etc.)",
  "fecha": "DD/MM/YYYY",
  "hora": "HH (formato 24 horas de 2 dígitos, ej: 08, 14, o vacío si no se menciona hora)",
  "minuto": "MM (2 dígitos, ej: 00, 30, o vacío si no se menciona hora)",
  "juzgado": "Nombre del juzgado o despacho judicial",
  "se_realizo": "SI o NO",
  "motivo": "Si se_realizo es NO, elegir SOLO UNO entre: Juez, Fiscalía, Usuario, INPEC, Víctima, ICBF, Defensor Confianza, Defensor Público. Si es SI, dejar vacío",
  "observaciones": "Resumen breve y profesional de detalles relevantes, fechas de reprogramación o incidencias"
}}

REGLAS CRÍTICAS DE EXTRACCIÓN:
1. RADICADO Y PROCESADO:
   - Busca números largos (19 a 23 dígitos, ej: "200016001075202255547") o formatos cortos ("2024-00236", "2019-00542").
   - Si hay varios procesados, inclúyelos (ej: "FERNANDO JULIO CASUIL Y SHARLES DE JESUS LOPEZ JIMENEZ - 20001...").
2. FECHA Y HORA MILITAR (24h):
   - Convierte cualquier fecha ("24 de junio", "JUNIO 24") a DD/MM/YYYY. Si no menciona el año, usa {current_year}.
   - Convierte horas PM a formato 24h ("09:30 am" -> hora: "09", minuto: "30"; "2:30 PM" -> hora: "14", minuto: "30"). Si no hay hora, deja "hora": "" y "minuto": "".
3. TIPOS DE AUDIENCIA (ESTILO ESTRUCTURADO Y COMPACTO):
   - "INICIA JUICIO" o "CONTINUA JUICIO" -> "Audiencia de juicio oral"
   - "ACUSACION" -> "Audiencia de acusación"
   - "PREPARATORIA" -> "Audiencia preparatoria"
4. REALIZACIÓN Y JERARQUÍA DE MOTIVOS:
   - FRACASO (SIEMPRE "se_realizo": "NO"): "se reprogramó", "será reprogramada", "se aplaza", "solicita aplazamiento", "se suspende", "nueva fecha", "se fija", "FRACSA", "FRACASA".
   - ÉXITO ("se_realizo": "SI", "motivo": ""): "se realizó con normalidad", "sin novedad", "se efectuó", "se emitió sentido de fallo", "se dio lectura", o si dice "¿Se realizó?: Sí" y la audiencia continuó pese a la ausencia de un tercero.
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
5. JUZGADOS Y ABREVIACIONES AMBIGUAS:
   - Para abreviaciones ambiguas como "CTO" / "Cto" (que puede significar Circuito o Conocimiento), MPAL o PCTO, MANTÉN la abreviación original tal cual ("Juzgado 3 Cto Penal" -> "Juzgado 3 Cto Penal"); no adivines su expansión.
6. OBSERVACIONES:
   - Incluye fechas de reprogramación, causas secundarias o incidencias técnicas. Si no hay detalles adicionales, escribe "Sin observaciones adicionales".
{seccion_aprendizaje}
TEXTO A ANALIZAR:
"""
{texto_preprocesado}
""""#
    )
}

async fn llamar_openai(api_key: &str, model: &str, prompt: &str) -> Result<Value, String> {
    let mut body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "Eres un extractor experto de datos de audiencias judiciales colombianas. Responde siempre en JSON válido."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "response_format": { "type": "json_object" }
    });

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

    if !status.is_success() {
        let msg = json_resp["error"]["message"]
            .as_str()
            .unwrap_or("Error desconocido en la API de OpenAI");
        return Err(format!("OpenAI ({}): {}", status, msg));
    }

    let content = json_resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "OpenAI no devolvió contenido en la respuesta".to_string())?;

    serde_json::from_str(content)
        .map_err(|e| format!("No se pudo interpretar el JSON de OpenAI: {}", e))
}

async fn llamar_gemini(api_key: &str, model: &str, prompt: &str) -> Result<Value, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model.trim()
    );

    let body = json!({
        "contents": [
            {
                "parts": [
                    { "text": prompt }
                ]
            }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "temperature": 0.1
        }
    });

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

    if !status.is_success() {
        let msg = json_resp["error"]["message"]
            .as_str()
            .unwrap_or("Error desconocido en la API de Gemini");
        return Err(format!("Gemini ({}): {}", status, msg));
    }

    let content = json_resp["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| "Gemini no devolvió texto en la respuesta".to_string())?;

    serde_json::from_str(content)
        .map_err(|e| format!("No se pudo interpretar el JSON de Gemini: {}", e))
}

async fn consultar_json_proveedor(prompt: &str) -> Result<Value, String> {
    let settings = cargar_settings();
    if settings.provider == "gemini" {
        if settings.gemini_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de Google Gemini.".to_string());
        }
        llamar_gemini(&settings.gemini_api_key, &settings.gemini_model, prompt).await
    } else {
        if settings.openai_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de OpenAI.".to_string());
        }
        llamar_openai(&settings.openai_api_key, &settings.openai_model, prompt).await
    }
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

/// Segunda llamada a la IA (solo cuando existen diferencias entre la predicción y lo guardado)
/// para deducir qué falló y convertirlo en una regla reutilizable.
pub async fn analizar_correccion_ia(
    correccion: &CorreccionIa,
) -> Result<AnalisisCorreccionIa, String> {
    let prompt = construir_prompt_analisis_correccion(correccion);
    let raw = consultar_json_proveedor(&prompt).await?;

    let es_error_de_ia = raw
        .get("es_error_de_ia")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
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
        .unwrap_or(0.85)
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

pub async fn extraer_datos_audiencia_ia(texto: &str) -> Result<AiAudienciaOutput, String> {
    if texto.trim().is_empty() {
        return Err("Ingresa el texto de la audiencia para procesar.".to_string());
    }

    let current_year = Local::now().year();
    let texto_pre = expandir_abreviaciones_y_compacto(texto, current_year);
    let contexto_aprendizaje = obtener_contexto_prompt(texto);
    let prompt = construir_prompt(&texto_pre, current_year, &contexto_aprendizaje);
    let raw_json = consultar_json_proveedor(&prompt).await?;

    let get_str = |key: &str| -> String {
        raw_json
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };

    let radicado = get_str("radicado");
    let raw_tipo = get_str("tipo_audiencia");
    let (tipo_audiencia, tipo_otra) = mapear_tipo_audiencia(&raw_tipo);
    let fecha = normalizar_fecha_ia(&get_str("fecha"));
    let (hora, minuto) = normalizar_hora_minuto_ia(&get_str("hora"), &get_str("minuto"));

    let juzgado = get_str("juzgado");
    let mut se_realizo = get_str("se_realizo").to_uppercase();
    if se_realizo == "SÍ" {
        se_realizo = "SI".to_string();
    }

    let texto_lower = texto.to_lowercase();
    let dice_si_realizada = [
        "¿se realizó?: sí",
        "¿se realizó? sí",
        "se realizó: sí",
        "se realizo: si",
    ]
    .iter()
    .any(|p| texto_lower.contains(p));
    let dice_ningun_motivo = texto_lower.contains("motivo (si no se realizó): ninguno")
        || texto_lower.replace("ninguna", "ninguno").contains("ninguno");
    let tiene_motivo_explicito_fracaso = [
        "la fiscal no vino",
        "fiscal no compareció",
        "fiscal no se conectó",
        "defensor no compareció",
        "juez ausente",
        "inasistente",
        "solicité reprogramación",
    ]
    .iter()
    .any(|p| texto_lower.contains(p));

    let patrones_fracaso = [
        "se reprogramó",
        "será reprogramada",
        "se reprograma",
        "reprogramación",
        "solicitó reprogramación",
        "pide reprogramación",
        "solicita aplazamiento",
        "se aplaza",
        "será aplazada",
        "se suspende",
        "suspendida",
        "nueva fecha",
        "se fija",
        "se fijó nueva fecha",
        "fracsa",
        "fracasa",
        "no se realizó",
        "no se hizo",
    ];

    if dice_si_realizada && (!tiene_motivo_explicito_fracaso || dice_ningun_motivo) {
        se_realizo = "SI".to_string();
    } else if dice_si_realizada && tiene_motivo_explicito_fracaso {
        // Caso contradictorio de legacy-python: dice "Sí" pero narra que fracasó
        se_realizo = "NO".to_string();
    } else if patrones_fracaso.iter().any(|p| texto_lower.contains(p)) {
        se_realizo = "NO".to_string();
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
        }

        // Corrección contextual para Defensor Confianza vs Defensor Público
        if motivo_final == "Defensor Público" && detectar_defensor_confianza(&texto_lower) {
            motivo_final = "Defensor Confianza".to_string();
        }
    }

    let mut observaciones = get_str("observaciones");
    if !motivos_secundarios.is_empty() {
        let extra = format!("Causas secundarias: {}", motivos_secundarios.join(", "));
        if observaciones.is_empty() {
            observaciones = extra;
        } else if !observaciones.contains("Causas secundarias") {
            observaciones = format!("{}; {}", observaciones, extra);
        }
    }
    if se_realizo == "NO" {
        let fechas_reprog = extraer_fechas_reprogramacion(texto);
        if !fechas_reprog.is_empty()
            && !observaciones.to_lowercase().contains(&fechas_reprog.to_lowercase())
        {
            let extra = format!("Reprogramada para: {}", fechas_reprog);
            if observaciones.is_empty() {
                observaciones = extra;
            } else {
                observaciones = format!("{}; {}", observaciones, extra);
            }
        }
    }
    if observaciones.is_empty() {
        observaciones = "Sin observaciones adicionales".to_string();
    }

    Ok(AiAudienciaOutput {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
