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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn validar_audiencia(datos: &AudienciaInput, permitir_hora_vacia: bool) -> Result<(), String> {
    if datos.radicado.trim().is_empty() {
        return Err("El campo 'Radicado del proceso' es obligatorio".to_string());
    }
    if datos.tipo.trim().is_empty() || datos.tipo.trim() == "-- Seleccione tipo --" {
        return Err("El campo 'Tipo de audiencia' es obligatorio".to_string());
    }
    if datos.juzgado.trim().is_empty() {
        return Err("El campo 'Juzgado' es obligatorio".to_string());
    }
    let realizada = datos.realizada.trim().to_uppercase();
    if realizada != "SI" && realizada != "NO" {
        return Err("Debe seleccionar si la audiencia se realizó o no (SI/NO)".to_string());
    }

    // Validar fecha DD/MM/AAAA entre 2018 y 2050
    let fecha_str = datos.fecha.trim();
    let fecha = NaiveDate::parse_from_str(fecha_str, "%d/%m/%Y")
        .map_err(|_| format!("Formato de fecha inválido: {}. Use DD/MM/AAAA", fecha_str))?;
    let year = fecha.format("%Y").to_string().parse::<i32>().unwrap_or(0);
    if !(2018..=2050).contains(&year) {
        return Err(format!(
            "La fecha debe estar entre los años 2018 y 2050: {}",
            fecha_str
        ));
    }

    // Validar hora HH:MM
    let hora_str = datos.hora.trim();
    if hora_str.is_empty() {
        if !permitir_hora_vacia {
            return Err("HORA_VACIA_REQUIERE_CONFIRMACION".to_string());
        }
    } else {
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
        assert_eq!(
            validar_audiencia(&input, false),
            Err("HORA_VACIA_REQUIERE_CONFIRMACION".into())
        );
        assert!(validar_audiencia(&input, true).is_ok());
    }
}
