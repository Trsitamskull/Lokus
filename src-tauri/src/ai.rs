use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
        (r"(?i)\bFRACSA\b", "Fracasó"),
        (r"(?i)\bFRACASA\b", "Fracasó"),
        (r"(?i)\bFRCASA\b", "Fracasó"),
        (r"(?i)\bDEPACHO\b", "Despacho"),
        (r"(?i)\bEZADO\b", "Especializado"),
        (r"(?i)\bEZADA\b", "Especializada"),
        (r"(?i)\bGRAVADO\b", "Agravado"),
        (r"(?i)\bAGRVADO\b", "Agravado"),
        (r"(?i)\bREEMPLAZDAO\b", "Reemplazado"),
        (r"(?i)\bGENDA\b", "Agenda"),
        (r"(?i)\bCONTINUCION\b", "Continuación"),
        (r"(?i)\bNO CONEXION\b", "No se pudo establecer conexión"),
        (
            r"(?i)\bAMBOS DESPACHOS EN AUDIENCIA\b",
            "Ambos despachos estaban ocupados en otras audiencias",
        ),
        (
            r"(?i)\bERROR EN LA GENDA\b",
            "Error en la agenda del despacho",
        ),
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
        // Juez
        ("vacaciones", "Juez"),
        ("titular de despacho", "Juez"),
        ("juez no compareció", "Juez"),
        ("ausencia del juez", "Juez"),
        ("juzgado cerrado", "Juez"),
        ("juez en comisión", "Juez"),
        ("despacho en otra audiencia", "Juez"),
        ("estar el despacho en audiencia", "Juez"),
        ("ambos despachos en audiencia", "Juez"),
        ("error en la agenda", "Juez"),
        // Fiscalía
        ("fiscal no se conectó", "Fiscalía"),
        ("fiscal fue reemplazad", "Fiscalía"),
        ("no conocer la carpeta", "Fiscalía"),
        ("solicitud de fiscalía", "Fiscalía"),
        ("solicitud de la fiscalía", "Fiscalía"),
        ("fiscalía no compareció", "Fiscalía"),
        ("fiscal solicitó reprogramación", "Fiscalía"),
        ("fiscalía solicitó reprogramación", "Fiscalía"),
        ("fiscal no vino", "Fiscalía"),
        ("fiscal no asistió", "Fiscalía"),
        ("ausencia de la fiscal", "Fiscalía"),
        ("ausencia del fiscal", "Fiscalía"),
        ("posible preacuerdo", "Fiscalía"),
        // Defensor Confianza
        ("defensor de confianza", "Defensor Confianza"),
        ("abogado contractual", "Defensor Confianza"),
        ("abogado privado", "Defensor Confianza"),
        ("def. confianza", "Defensor Confianza"),
        ("def confianza", "Defensor Confianza"),
        ("inasistencia del abogado", "Defensor Confianza"),
        // Defensor Público
        ("defensor público no compareció", "Defensor Público"),
        ("inasistencia del defensor público", "Defensor Público"),
        ("defensor público solicitó", "Defensor Público"),
        ("solicité reprogram", "Defensor Público"),
        ("pedí reprogram", "Defensor Público"),
        ("pedí aplazamiento", "Defensor Público"),
        ("no pude contactar", "Defensor Público"),
        ("no había tenido contacto con el procesado", "Defensor Público"),
        ("me fue imposible", "Defensor Público"),
        // Usuario
        ("no notificaron al procesado", "Usuario"),
        ("procesado no compareció", "Usuario"),
        ("usuario no se presentó", "Usuario"),
        ("procesado ausente", "Usuario"),
        // INPEC
        ("inpec no trasladó", "INPEC"),
        ("problemas de traslado", "INPEC"),
        ("inpec reportó", "INPEC"),
        ("inpec", "INPEC"),
        // Víctima
        ("víctima no compareció", "Víctima"),
        ("ausencia de la víctima", "Víctima"),
        // ICBF
        ("icbf", "ICBF"),
    ];

    let mut encontrados: Vec<String> = Vec::new();
    for (frase, motivo) in frases_motivos {
        if texto_lower.contains(frase) && !encontrados.contains(&motivo.to_string()) {
            encontrados.push(motivo.to_string());
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

fn construir_prompt(texto_preprocesado: &str, current_year: i32) -> String {
    format!(
        r#"Eres un asistente jurídico experto en audiencias judiciales en Colombia.
Analiza el siguiente texto y extrae los datos de la audiencia devolviendo ÚNICAMENTE un objeto JSON válido con esta estructura exacta:

{{
  "radicado": "NOMBRE_PROCESADO - NÚMERO_RADICADO (si hay ambos) o el identificador/radicado encontrado",
  "tipo_audiencia": "Tipo de audiencia (ej: Audiencia concentrada, Audiencia de juicio oral, Audiencia preparatoria, Audiencia de acusación, Audiencia de control de legalidad, Alegatos de conclusión, etc.)",
  "fecha": "DD/MM/YYYY",
  "hora": "HH (formato 24 horas de 2 dígitos, ej: 08, 14, o vacío si no se menciona hora)",
  "minuto": "MM (2 dígitos, ej: 00, 30, o vacío si no se menciona hora)",
  "juzgado": "Nombre completo del juzgado o despacho judicial",
  "se_realizo": "SI o NO",
  "motivo": "Si se_realizo es NO, elegir SOLO UNO entre: Juez, Fiscalía, Usuario, INPEC, Víctima, ICBF, Defensor Confianza, Defensor Público. Si es SI, dejar vacío",
  "observaciones": "Resumen breve y profesional de detalles relevantes, fechas de reprogramación o incidencias"
}}

REGLAS CRÍTICAS:
1. FECHA: Convierte cualquier fecha al formato DD/MM/YYYY. Si el texto no menciona el año, usa {current_year}.
2. HORA MILITAR (24h): Convierte horas PM a formato 24h (ej: 2:30 PM -> hora: "14", minuto: "30"). Si el texto no menciona ninguna hora, deja "hora": "" y "minuto": "".
3. REALIZACIÓN Y MOTIVOS:
   - Si el texto menciona que la audiencia fracasó, se aplazó, se suspendió o se reprogramó, entonces "se_realizo": "NO".
   - Diferencia bien entre "Defensor Confianza" (abogado contractual/privado) y "Defensor Público" (defensoría o primera persona como "solicité reprogramar", "no pude contactar al procesado").

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

pub async fn extraer_datos_audiencia_ia(texto: &str) -> Result<AiAudienciaOutput, String> {
    if texto.trim().is_empty() {
        return Err("Ingresa el texto de la audiencia para procesar.".to_string());
    }

    let settings = cargar_settings();
    let current_year = Local::now().year();
    let texto_pre = expandir_abreviaciones_y_compacto(texto, current_year);
    let prompt = construir_prompt(&texto_pre, current_year);

    let raw_json = if settings.provider == "gemini" {
        if settings.gemini_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de Google Gemini.".to_string());
        }
        llamar_gemini(&settings.gemini_api_key, &settings.gemini_model, &prompt).await?
    } else {
        if settings.openai_api_key.trim().is_empty() {
            return Err("No has configurado tu API Key de OpenAI.".to_string());
        }
        llamar_openai(&settings.openai_api_key, &settings.openai_model, &prompt).await?
    };

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
    let patrones_fracaso = [
        "se reprogramó",
        "será reprogramada",
        "se reprograma",
        "reprogramación",
        "solicitó reprogramación",
        "solicita aplazamiento",
        "se aplaza",
        "se suspende",
        "fracsa",
        "fracasa",
        "no se realizó",
    ];
    if patrones_fracaso.iter().any(|p| texto_lower.contains(p))
        && !texto_lower.contains("se realizó: sí")
    {
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
}
