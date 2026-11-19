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

pub const TIPOS_AUDIENCIA: [&str; 18] = [
    "Alegatos de conclusión",
    "Audiencia concentrada",
    "Audiencia de acusación",
    "Audiencia de conciliación",
    "Audiencia de control de legalidad",
    "Audiencia de individualización de pena",
    "Audiencia de imputación",
    "Audiencia de incidente de reparación integral",
    "Audiencia de juicio oral",
    "Audiencia de medidas de aseguramiento",
    "Audiencia de nulidad",
    "Audiencia de preclusión",
    "Audiencia de prórroga",
    "Audiencia de revisión de medida",
    "Audiencia de verificación de cumplimiento",
    "Audiencia preliminar",
    "Audiencia preparatoria",
    "Otra",
];

pub const HORAS_RAPIDAS: [(&str, &str); 6] = [
    ("08:00", "8:00 AM"),
    ("09:00", "9:00 AM"),
    ("10:00", "10:00 AM"),
    ("14:00", "2:00 PM"),
    ("15:00", "3:00 PM"),
    ("16:00", "4:00 PM"),
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
    pub frecuencia: u32,
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
}
