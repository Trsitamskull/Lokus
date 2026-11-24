//! Motor de recepción, ingesta idempotente, deduplicación multi-fecha/multi-usuario,
//! cuarentena y compactación de paquetes JSON de autoaprendizaje (`formato: 4` y `formato: 5`)
//! y uso de catálogos para el mantenedor de Lokus.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::aprendizaje::{
    alcance_canonico, calcular_bm25, colapsar_espacios, cuantil_beta, es_misma_regla,
    es_trigger_valido, extraer_triggers_texto, inferir_campo_objetivo_desde_tipo,
    limite_inferior_bayesiano, normalizar_ess_par, seleccionar_mmr, tokenizar_judicial,
    AliasDiagnostico, AliasEstadistico, CorreccionIa, EjemploPositivo, MemoriaAprendizaje,
    PaqueteExportacion, PatronAbreviacionDiagnostico, PerfilSinteticoDefensor, PreferenciasEstilo,
    PreferenciasEstiloResumen, ReglaAprendida, RegistroConsolidacionIa, ResumenDefensorExport,
    SugerenciaBuild, MAX_ESS_N, PRIOR_ALIAS_ALPHA, PRIOR_ALIAS_BETA, PRIOR_BETA_ALPHA,
    PRIOR_BETA_BETA,
};
use crate::models::{
    normalizar_clave_catalogo, normalizar_clave_defensor, normalizar_texto_sin_tildes,
    GrafoAsociaciones, HORAS_RAPIDAS, TIPOS_AUDIENCIA,
};
use crate::uso_catalogos::ReporteUsoCatalogos;

pub const VERSION_LEDGER: u32 = 1;
pub const MAX_REGLAS_BASE_COMPACTADAS: usize = 60;
pub const MAX_CORRECCIONES_BASE_COMPACTADAS: usize = 45;
pub const MAX_ALIAS_BASE_COMPACTADOS: usize = 120;
pub const MAX_EJEMPLOS_BASE_COMPACTADOS: usize = 30;
pub const UMBRAL_Q10_REGLA_BASE: f64 = 0.30;
pub const UMBRAL_Q10_ALIAS_BASE: f64 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoSalidaIngesta {
    Text,
    Json,
}

impl FormatoSalidaIngesta {
    pub fn desde_str(valor: &str) -> Result<Self, String> {
        match valor.trim().to_lowercase().as_str() {
            "text" | "txt" | "markdown" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            otro => Err(format!(
                "Valor inválido para --formato-salida: '{}' (valores permitidos: text, json)",
                otro
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpcionesIngesta {
    pub ruta_base: PathBuf,
    pub ruta_ledger: PathBuf,
    pub ruta_catalogos_base: PathBuf,
    pub ruta_reporte: Option<PathBuf>,
    pub fuente_manual: Option<String>,
    pub dry_run: bool,
    pub formato_salida: FormatoSalidaIngesta,
    pub archivos_entrada: Vec<PathBuf>,
}

impl Default for OpcionesIngesta {
    fn default() -> Self {
        Self {
            ruta_base: PathBuf::from("src-tauri/resources/aprendizaje_base.json"),
            ruta_ledger: PathBuf::from("src-tauri/resources/ingesta_ledger.json"),
            ruta_catalogos_base: PathBuf::from("src-tauri/resources/juzgados_base.json"),
            ruta_reporte: None,
            fuente_manual: None,
            dry_run: false,
            formato_salida: FormatoSalidaIngesta::Text,
            archivos_entrada: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RegistroArchivoIngerido {
    pub sha256: String,
    pub source_id: String,
    pub formato: u32,
    pub exportacion_seq: u64,
    pub exportado: String,
    pub version_app: String,
    pub estado_procesamiento: String,
}

impl Default for RegistroArchivoIngerido {
    fn default() -> Self {
        Self {
            sha256: String::new(),
            source_id: String::new(),
            formato: 4,
            exportacion_seq: 0,
            exportado: String::new(),
            version_app: String::new(),
            estado_procesamiento: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ItemCuarentena {
    pub categoria: String,
    pub id_o_clave: String,
    pub valor: String,
    pub motivo: String,
    pub source_id: String,
}

impl Default for ItemCuarentena {
    fn default() -> Self {
        Self {
            categoria: String::new(),
            id_o_clave: String::new(),
            valor: String::new(),
            motivo: String::new(),
            source_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FuenteLedger {
    pub source_id: String,
    pub formato_ultimo: u32,
    pub ultimo_exportado: String,
    pub ultimo_seq: u64,
    pub sembrado_en: String,
    pub ultima_actualizacion_uso: String,
    pub fuentes_importadas: Vec<String>,
    pub total_audiencias_contabilizadas: u32,
    pub audiencias_netas: u32,
    pub aciertos: u32,
    pub llamadas_induccion_llm_total: u32,
    pub llamadas_extraccion_total: u32,
    pub llamadas_consolidacion_total: u32,
    pub uso_catalogos: ReporteUsoCatalogos,
    pub grafo_asociaciones: GrafoAsociaciones,
    pub defensores_directorio: Vec<ResumenDefensorExport>,
    pub juzgados_agregados_usuario: Vec<String>,
    pub tipos_audiencia_agregados_usuario: Vec<String>,
    pub preferencias_estilo: PreferenciasEstiloResumen,
    pub preferencias_raw: Option<PreferenciasEstilo>,
    pub preferencias_por_defensor_raw: BTreeMap<String, PreferenciasEstilo>,
    pub perfiles_sinteticos: BTreeMap<String, PerfilSinteticoDefensor>,
    pub alias_validados: Vec<AliasDiagnostico>,
    pub tabla_alias_raw: Vec<AliasEstadistico>,
    pub patrones_abreviacion: Vec<PatronAbreviacionDiagnostico>,
    pub reglas_por_id: BTreeMap<String, ReglaAprendida>,
    pub correcciones_por_id: BTreeMap<String, CorreccionIa>,
    pub consolidaciones_por_id: BTreeMap<String, RegistroConsolidacionIa>,
    pub ejemplos_por_id: BTreeMap<String, EjemploPositivo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct CandidatoCatalogoAgregado {
    pub nombre: String,
    pub origen: String,
    pub conteo_total: u32,
    pub fuentes_confirmantes: usize,
    pub ya_en_catalogo_base: bool,
    pub recomendado_incorporar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct UsoCatalogosAgregadoLedger {
    pub total_audiencias_contabilizadas: u32,
    pub juzgados_candidatos: Vec<CandidatoCatalogoAgregado>,
    pub tipos_candidatos: Vec<CandidatoCatalogoAgregado>,
    pub horas_no_rapidas_frecuentes: Vec<(String, u32)>,
    pub horas_rapidas_sin_uso_global: Vec<String>,
    pub juzgados_predeterminados_sin_uso_global: Vec<String>,
    pub tipos_predeterminados_sin_uso_global: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LedgerIngesta {
    pub version: u32,
    pub ultima_actualizacion: String,
    pub total_audiencias_acumuladas: u32,
    pub total_fuentes_activas: usize,
    pub archivos_ingeridos: Vec<RegistroArchivoIngerido>,
    pub fuentes: BTreeMap<String, FuenteLedger>,
    pub uso_catalogos_agregado: UsoCatalogosAgregadoLedger,
    pub cuarentena: Vec<ItemCuarentena>,
    pub advertencias_solapamiento: Vec<String>,
}

impl Default for LedgerIngesta {
    fn default() -> Self {
        Self {
            version: VERSION_LEDGER,
            ultima_actualizacion: String::new(),
            total_audiencias_acumuladas: 0,
            total_fuentes_activas: 0,
            archivos_ingeridos: Vec::new(),
            fuentes: BTreeMap::new(),
            uso_catalogos_agregado: UsoCatalogosAgregadoLedger::default(),
            cuarentena: Vec::new(),
            advertencias_solapamiento: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumenEjecucionIngesta {
    pub estado: String,
    pub dry_run: bool,
    pub archivos_recibidos: usize,
    pub archivos_aplicados: usize,
    pub archivos_omitidos_idempotentes: usize,
    pub archivos_obsoletos_backfill: usize,
    pub fuentes_totales: usize,
    pub total_audiencias_acumuladas: u32,
    pub reglas_base_invariante: usize,
    pub reglas_totales_base: usize,
    pub reglas_nuevas_agregadas: usize,
    pub correcciones_totales_base: usize,
    pub alias_totales_base: usize,
    pub ejemplos_totales_base: usize,
    pub items_en_cuarentena: usize,
    pub juzgados_candidatos_validos: usize,
    pub tipos_candidatos_validos: usize,
    pub advertencias_solapamiento: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SalidaCompletaIngesta {
    pub resumen: ResumenEjecucionIngesta,
    pub reporte_markdown: String,
    pub texto_stdout: String,
}

struct PaqueteValidadoEntrada {
    ruta: PathBuf,
    sha256: String,
    paquete: PaqueteExportacion,
}

// ---------------------------------------------------------------------------
// SHA-256 estándar (FIPS 180-4) sin dependencias externas
// ---------------------------------------------------------------------------

pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            let j = i * 4;
            w[i] = u32::from_be_bytes([chunk[j], chunk[j + 1], chunk[j + 2], chunk[j + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:08x}", word);
    }
    out
}

fn short_hash(s: &str) -> String {
    let full = sha256_hex(s.as_bytes());
    full[..12].to_string()
}

// ---------------------------------------------------------------------------
// Validadores de cuarentena, ruido, contradicción y normalización
// ---------------------------------------------------------------------------

pub fn contiene_marcador_anonimizacion(s: &str) -> bool {
    let low = s.to_lowercase();
    low.contains("[procesado]")
        || low.contains("[radicado]")
        || low.contains("[telefono]")
        || low.contains("[numero]")
}

pub fn contiene_falso_positivo_procesado_en_texto(s: &str) -> bool {
    let low = normalizar_texto_sin_tildes(s);
    low.contains("san procesado")
        || low.contains("porte de procesado")
        || low.contains("tenencia de procesado")
        || low.contains("trafico de procesado")
        || low.contains("una procesado de")
        || low.contains("ese procesado")
        || low.contains("las procesado")
}

pub fn es_entrada_catalogo_invalida(valor: &str, campo: &str) -> Option<&'static str> {
    let limpio = colapsar_espacios(valor);
    if limpio.len() < 4 {
        return Some("entrada_muy_corta");
    }
    if contiene_marcador_anonimizacion(&limpio) {
        return Some("marcador_anonimizacion");
    }
    let norm = normalizar_texto_sin_tildes(&limpio);
    if norm.is_empty() {
        return Some("entrada_vacia");
    }
    if norm.contains("maunicipal")
        || norm.contains("municpal")
        || norm.contains("promiscuo segundo promiscuo")
        || norm.contains("fdsafe")
        || norm.contains("asdf")
        || norm.contains("qwerty")
        || norm.contains("1234")
    {
        return Some("typo_o_ruido_conocido");
    }

    let palabras: Vec<&str> = norm.split_whitespace().collect();
    if campo == "juzgado" {
        if palabras.len() < 2 {
            return Some("juzgado_sin_estructura");
        }
        let tiene_prefijo = palabras.iter().any(|w| {
            matches!(
                *w,
                "juzgado" | "tribunal" | "corte" | "sala" | "despacho" | "centro" | "penal"
            )
        });
        if !tiene_prefijo {
            return Some("juzgado_sin_prefijo_despacho");
        }
        if palabras.len() == 2 && palabras[0].chars().all(|c| c.is_ascii_digit()) {
            return Some("fragmento_parcial_juzgado");
        }
        if norm == "juzgado 7 penal de circuito con funcion de conocimiento" {
            return Some("variante_incompleta_sin_ciudad_ni_tilde");
        }
    } else if campo == "tipo" {
        if palabras.len() == 1
            && !matches!(
                norm.as_str(),
                "sentencia" | "preclusion" | "acusacion" | "imputacion" | "otra"
            )
        {
            return Some("tipo_palabra_unica_no_reconocida");
        }
    }
    None
}

pub fn es_alias_invalido(entrada_norm: &str, canonico: &str, campo: &str) -> Option<&'static str> {
    if contiene_marcador_anonimizacion(entrada_norm) || contiene_marcador_anonimizacion(canonico) {
        return Some("marcador_anonimizacion");
    }
    if let Some(motivo) = es_entrada_catalogo_invalida(canonico, campo) {
        return Some(motivo);
    }
    let ent = normalizar_texto_sin_tildes(entrada_norm);
    let can = normalizar_texto_sin_tildes(canonico);
    if ent.is_empty() || can.is_empty() {
        return Some("alias_vacio");
    }
    if ent == can {
        return Some("auto_identidad");
    }
    let toks: Vec<&str> = ent.split_whitespace().collect();
    if toks.len() == 2
        && toks[0].chars().all(|c| c.is_ascii_digit())
        && matches!(
            toks[1],
            "municipal" | "circuito" | "penal" | "promiscuo" | "especializado"
        )
    {
        return Some("alias_parcial_ambiguo");
    }
    None
}

pub fn es_regla_base_invariante(r: &ReglaAprendida) -> bool {
    r.origen == "base_invariante" || r.id.starts_with("base-")
}

pub fn contradice_reglas_base_invariante(regla_texto: &str) -> Option<&'static str> {
    let norm = normalizar_texto_sin_tildes(regla_texto);
    if norm.is_empty() {
        return None;
    }

    // 1) Contradicción contra `base-abreviacion-cto` ("Nunca expandir CTO a Circuito...")
    if norm.split_whitespace().any(|w| w == "cto" || w == "pcto") {
        let pide_expandir = norm.contains("expandir")
            || norm.contains("expanda")
            || norm.contains("convertir")
            || norm.contains("reemplazar")
            || norm.contains("siempre a circuito")
            || norm.contains("cto a circuito")
            || norm.contains("cto como circuito");
        let tiene_negacion = norm.contains("no expandir")
            || norm.contains("nunca expandir")
            || norm.contains("sin expandir")
            || norm.contains("conservar cto")
            || norm.contains("mantener cto")
            || norm.contains("no expanda");
        if pide_expandir && !tiene_negacion {
            return Some("contradice_base_abreviacion_cto");
        }
    }

    // 2) Contradicción contra `base-jerarquia-causas-reacciones`
    // ("Si el defensor pide reprogramar COMO CONSECUENCIA de causa atribuible a otro sujeto (ej. fiscal)... clasificar por la causa raíz, NO como Defensor Público")
    let menciona_causa_otro = norm.contains("por causa del fiscal")
        || norm.contains("ausencia del fiscal")
        || norm.contains("inasistencia del fiscal")
        || norm.contains("por fiscal")
        || norm.contains("por el juez")
        || norm.contains("por inpec");
    let asigna_defensor_publico = norm.contains("marcar siempre motivo defensor publico")
        || norm.contains("asignar defensor publico")
        || norm.contains("clasificar como defensor publico")
        || norm.contains("motivo defensor publico");
    if menciona_causa_otro && asigna_defensor_publico && !norm.contains("no como defensor publico") {
        return Some("contradice_base_jerarquia_causas_reacciones");
    }

    // 3) Contradicción contra `base-defensor-confianza-contractual`
    if (norm.contains("cualquier abogado") || norm.contains("toda mencion de abogado"))
        && norm.contains("defensor confianza")
        && !norm.contains("no ")
    {
        return Some("contradice_base_defensor_confianza_contractual");
    }

    None
}

fn limpiar_triggers_regla(triggers_raw: &[String], texto_regla: &str) -> (Vec<String>, bool) {
    let mut limpios = Vec::new();
    let mut vistos = BTreeSet::new();
    for tr in triggers_raw {
        for tok in tokenizar_judicial(tr) {
            if es_trigger_valido(&tok) && vistos.insert(tok.clone()) {
                limpios.push(tok);
            }
        }
    }
    let todos_eran_basura = !triggers_raw.is_empty() && limpios.is_empty();
    if limpios.is_empty() && !todos_eran_basura {
        for tok in extraer_triggers_texto(texto_regla, 8) {
            if vistos.insert(tok.clone()) {
                limpios.push(tok);
            }
        }
    }
    (limpios, todos_eran_basura)
}

fn resolver_alpha_desde_q10(q10_objetivo: f64, beta: f64) -> f64 {
    let target = q10_objetivo.clamp(0.05, 0.95);
    let mut lo = PRIOR_ALIAS_ALPHA;
    let mut hi = (MAX_ESS_N - beta).max(2.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if cuantil_beta(mid, beta, 0.10) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn cuenta_tildes(s: &str) -> usize {
    s.chars()
        .filter(|c| matches!(c, 'á' | 'é' | 'í' | 'ó' | 'ú' | 'Á' | 'É' | 'Í' | 'Ó' | 'Ú' | 'ñ' | 'Ñ'))
        .count()
}

fn elegir_nombre_mas_rico(actual: &str, candidato: &str) -> String {
    let t_act = cuenta_tildes(actual);
    let t_can = cuenta_tildes(candidato);
    if t_can > t_act {
        colapsar_espacios(candidato)
    } else {
        colapsar_espacios(actual)
    }
}

fn comparar_rfc3339(a: &str, b: &str) -> std::cmp::Ordering {
    match (
        chrono::DateTime::parse_from_rfc3339(a.trim()),
        chrono::DateTime::parse_from_rfc3339(b.trim()),
    ) {
        (Ok(da), Ok(db)) => da.cmp(&db),
        _ => a.trim().cmp(b.trim()),
    }
}

fn conjunto_ids_no_base(paq: &PaqueteExportacion) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for r in &paq.reglas {
        if !es_regla_base_invariante(r) && !r.id.trim().is_empty() {
            ids.insert(r.id.trim().to_string());
        }
    }
    for c in &paq.correcciones {
        if !c.id.trim().starts_with("base-") && !c.id.trim().is_empty() {
            ids.insert(c.id.trim().to_string());
        }
    }
    for co in &paq.consolidaciones_ia {
        if !co.id.trim().is_empty() {
            ids.insert(co.id.trim().to_string());
        }
    }
    ids
}

fn conjunto_ids_fuente(fuente: &FuenteLedger) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for k in fuente.reglas_por_id.keys() {
        ids.insert(k.clone());
    }
    for k in fuente.correcciones_por_id.keys() {
        ids.insert(k.clone());
    }
    for k in fuente.consolidaciones_por_id.keys() {
        ids.insert(k.clone());
    }
    ids
}

fn resolver_source_id_y_migrar(
    paq: &PaqueteExportacion,
    sha256: &str,
    fuente_manual: Option<&str>,
    ledger: &mut LedgerIngesta,
) -> String {
    if let Some(alias) = fuente_manual {
        let limpio = alias.trim();
        if !limpio.is_empty() {
            return limpio.to_string();
        }
    }

    let sembrado = paq.uso_catalogos.sembrado_en.trim();
    let ids_entrantes = conjunto_ids_no_base(paq);

    if !paq.instancia_id.trim().is_empty() {
        let id_v5 = paq.instancia_id.trim().to_string();
        // Si existía una fuente v4 previa del mismo usuario (mismo sembrado_en + solapamiento de IDs),
        // migrar su historial al nuevo instancia_id de formato 5 para no duplicar.
        if !ledger.fuentes.contains_key(&id_v5) && !sembrado.is_empty() {
            let candidata_v4 = ledger
                .fuentes
                .iter()
                .find(|(k, f)| {
                    *k != &id_v5
                        && !paq.fuentes_importadas.contains(*k)
                        && f.formato_ultimo == 4
                        && f.sembrado_en.trim() == sembrado
                        && (!ids_entrantes.is_empty()
                            && conjunto_ids_fuente(f)
                                .intersection(&ids_entrantes)
                                .next()
                                .is_some())
                })
                .map(|(k, _)| k.clone());
            if let Some(old_key) = candidata_v4 {
                if let Some(mut old_fuente) = ledger.fuentes.remove(&old_key) {
                    old_fuente.source_id = id_v5.clone();
                    ledger.fuentes.insert(id_v5.clone(), old_fuente);
                }
            }
        }
        return id_v5;
    }

    // Formato 4 sin instancia_id ni --fuente
    if !sembrado.is_empty() {
        let candidatas_mismo_sembrado: Vec<String> = ledger
            .fuentes
            .iter()
            .filter(|(_, f)| f.sembrado_en.trim() == sembrado)
            .map(|(k, _)| k.clone())
            .collect();

        for key in &candidatas_mismo_sembrado {
            let f = &ledger.fuentes[key];
            let ids_f = conjunto_ids_fuente(f);
            let comparten_id = !ids_entrantes.is_empty()
                && ids_f.intersection(&ids_entrantes).next().is_some();
            if comparten_id {
                return key.clone();
            }
            if ids_entrantes.is_empty() || ids_f.is_empty() {
                let defs_f: BTreeSet<String> = f
                    .defensores_directorio
                    .iter()
                    .map(|d| normalizar_clave_defensor(&d.clave))
                    .filter(|s| !s.is_empty())
                    .collect();
                let defs_in: BTreeSet<String> = paq
                    .defensores_directorio
                    .iter()
                    .map(|d| normalizar_clave_defensor(&d.clave))
                    .filter(|s| !s.is_empty())
                    .collect();
                if (defs_f.is_empty() && defs_in.is_empty())
                    || defs_f.intersection(&defs_in).next().is_some()
                {
                    return key.clone();
                }
            }
        }

        if candidatas_mismo_sembrado.is_empty() {
            return format!("v4:sembrado:{}", short_hash(sembrado));
        } else {
            // Mismo sembrado_en pero IDs de IA disjuntos -> clon .lokus divergente
            let firma_ids = if !ids_entrantes.is_empty() {
                ids_entrantes.iter().cloned().collect::<Vec<_>>().join("|")
            } else {
                sha256.to_string()
            };
            let clon_id = format!(
                "v4:clon:{}:{}",
                short_hash(sembrado),
                short_hash(&firma_ids)
            );
            let msg = format!(
                "clon_lokus_detectado: sembrado_en '{}' compartido con IDs disjuntos -> asignado '{}'",
                sembrado, clon_id
            );
            if !ledger.advertencias_solapamiento.contains(&msg) {
                ledger.advertencias_solapamiento.push(msg);
            }
            return clon_id;
        }
    }

    // Fallback sin sembrado_en: buscar solapamiento de IDs no-base
    if !ids_entrantes.is_empty() {
        for (key, f) in &ledger.fuentes {
            let ids_f = conjunto_ids_fuente(f);
            if ids_f.intersection(&ids_entrantes).next().is_some() {
                return key.clone();
            }
        }
        let firma = ids_entrantes.iter().cloned().collect::<Vec<_>>().join("|");
        return format!("v4:ids:{}", short_hash(&firma));
    }

    format!("v4:anon:{}", &sha256[..12.min(sha256.len())])
}

fn es_snapshot_mas_reciente(paq: &PaqueteExportacion, fuente: &FuenteLedger) -> bool {
    if fuente.ultimo_exportado.trim().is_empty() && fuente.ultimo_seq == 0 {
        return true;
    }
    if paq.formato > fuente.formato_ultimo {
        return true;
    }
    if paq.formato < fuente.formato_ultimo {
        return false;
    }
    if paq.exportacion_seq > 0 || fuente.ultimo_seq > 0 {
        if paq.exportacion_seq != fuente.ultimo_seq {
            return paq.exportacion_seq > fuente.ultimo_seq;
        }
    }
    !matches!(
        comparar_rfc3339(&paq.exportado, &fuente.ultimo_exportado),
        std::cmp::Ordering::Less
    )
}

fn registrar_cuarentena_unica(
    cuarentena: &mut Vec<ItemCuarentena>,
    categoria: &str,
    id_o_clave: &str,
    valor: &str,
    motivo: &str,
    source_id: &str,
) {
    let ya_existe = cuarentena.iter().any(|c| {
        c.categoria == categoria
            && c.id_o_clave == id_o_clave
            && c.motivo == motivo
            && c.source_id == source_id
    });
    if !ya_existe {
        cuarentena.push(ItemCuarentena {
            categoria: categoria.to_string(),
            id_o_clave: id_o_clave.to_string(),
            valor: colapsar_espacios(valor),
            motivo: motivo.to_string(),
            source_id: source_id.to_string(),
        });
    }
}

fn aplicar_paquete_a_ledger(
    entrada: &PaqueteValidadoEntrada,
    fuente_manual: Option<&str>,
    ledger: &mut LedgerIngesta,
) -> String {
    let paq = &entrada.paquete;
    let sha = &entrada.sha256;

    // 1) Verificar idempotencia por hash SHA-256 exacto
    if ledger.archivos_ingeridos.iter().any(|a| a.sha256 == *sha) {
        return "idempotente_omitido".to_string();
    }

    let source_id = resolver_source_id_y_migrar(paq, sha, fuente_manual, ledger);

    // Verificar si la misma fuente ya procesó este mismo snapshot (mismo formato, seq, exportado y total_audiencias)
    if let Some(f_exist) = ledger.fuentes.get(&source_id) {
        if !f_exist.ultimo_exportado.trim().is_empty()
            && f_exist.ultimo_exportado.trim() == paq.exportado.trim()
            && f_exist.formato_ultimo == paq.formato
            && f_exist.ultimo_seq == paq.exportacion_seq
            && f_exist.total_audiencias_contabilizadas
                == paq.uso_catalogos.total_audiencias_contabilizadas
        {
            return "idempotente_omitido".to_string();
        }
    }

    let fuente_entry = ledger
        .fuentes
        .entry(source_id.clone())
        .or_insert_with(|| FuenteLedger {
            source_id: source_id.clone(),
            ..Default::default()
        });

    let es_reciente = es_snapshot_mas_reciente(paq, fuente_entry);

    // Auditar y registrar elementos en cuarentena del paquete entrante
    for r in &paq.reglas {
        if es_regla_base_invariante(r) {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "regla",
                &r.id,
                &r.regla,
                "intento_suplantacion_base_invariante",
                &source_id,
            );
            continue;
        }
        if contiene_marcador_anonimizacion(&r.regla)
            || r.triggers.iter().any(|t| contiene_marcador_anonimizacion(t))
        {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "regla",
                &r.id,
                &r.regla,
                "contiene_marcador_anonimizacion",
                &source_id,
            );
            continue;
        }
        if let Some(mot_contra) = contradice_reglas_base_invariante(&r.regla) {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "regla",
                &r.id,
                &r.regla,
                mot_contra,
                &source_id,
            );
            continue;
        }
        let (_, todos_basura) = limpiar_triggers_regla(&r.triggers, &r.regla);
        if todos_basura {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "regla",
                &r.id,
                &r.regla,
                "triggers_100_por_ciento_ruido_o_stopwords",
                &source_id,
            );
        }
    }

    for j in &paq.juzgados_agregados_usuario {
        if let Some(motivo) = es_entrada_catalogo_invalida(j, "juzgado") {
            registrar_cuarentena_unica(&mut ledger.cuarentena, "juzgado", j, j, motivo, &source_id);
        }
    }
    for item in &paq.uso_catalogos.juzgados.nuevos_no_predeterminados {
        if let Some(motivo) = es_entrada_catalogo_invalida(&item.nombre, "juzgado") {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "juzgado",
                &item.nombre,
                &item.nombre,
                motivo,
                &source_id,
            );
        }
    }
    for t in &paq.tipos_audiencia_agregados_usuario {
        if let Some(motivo) = es_entrada_catalogo_invalida(t, "tipo") {
            registrar_cuarentena_unica(&mut ledger.cuarentena, "tipo", t, t, motivo, &source_id);
        }
    }
    for item in &paq.uso_catalogos.tipos_audiencia.nuevos_no_predeterminados {
        if let Some(motivo) = es_entrada_catalogo_invalida(&item.nombre, "tipo") {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "tipo",
                &item.nombre,
                &item.nombre,
                motivo,
                &source_id,
            );
        }
    }
    for a in &paq.alias_validados {
        if let Some(motivo) = es_alias_invalido(&a.entrada_norm, &a.canonico, &a.campo) {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "alias",
                &a.entrada_norm,
                &a.canonico,
                motivo,
                &source_id,
            );
        }
    }
    for a in &paq.tabla_alias_raw {
        if let Some(motivo) = es_alias_invalido(&a.entrada_norm, &a.canonico, &a.campo) {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "alias_raw",
                &a.entrada_norm,
                &a.canonico,
                motivo,
                &source_id,
            );
        }
    }
    for p in &paq.perfiles_sinteticos {
        if contiene_marcador_anonimizacion(&p.resumen) {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "perfil_sintetico",
                &p.ambito,
                &p.resumen,
                "contiene_marcador_anonimizacion",
                &source_id,
            );
        }
    }
    for co in &paq.consolidaciones_ia {
        for sb in &co.sugerencias_build {
            if contiene_marcador_anonimizacion(&sb.detalle) {
                registrar_cuarentena_unica(
                    &mut ledger.cuarentena,
                    "sugerencia_build",
                    &co.id,
                    &sb.detalle,
                    "contiene_marcador_anonimizacion",
                    &source_id,
                );
            }
        }
    }
    for ej in &paq.ejemplos_aciertos {
        if contiene_marcador_anonimizacion(&ej.salida_anonimizada.juzgado)
            || contiene_marcador_anonimizacion(&ej.salida_anonimizada.tipo)
            || contiene_falso_positivo_procesado_en_texto(&ej.texto_anonimizado)
            || contiene_falso_positivo_procesado_en_texto(&ej.salida_anonimizada.observaciones)
        {
            registrar_cuarentena_unica(
                &mut ledger.cuarentena,
                "ejemplo_acierto",
                &ej.id,
                &ej.salida_anonimizada.juzgado,
                "contaminacion_marcador_anonimizacion",
                &source_id,
            );
        }
    }

    // 2) Unión por ID para entidades acotadas (reglas, correcciones, consolidaciones, ejemplos)
    for r in &paq.reglas {
        if es_regla_base_invariante(r) || r.id.trim().is_empty() {
            continue;
        }
        let id = r.id.trim().to_string();
        if es_reciente || !fuente_entry.reglas_por_id.contains_key(&id) {
            fuente_entry.reglas_por_id.insert(id, r.clone());
        }
    }
    for c in &paq.correcciones {
        if c.id.trim().is_empty() || c.id.trim().starts_with("base-") {
            continue;
        }
        let id = c.id.trim().to_string();
        if es_reciente || !fuente_entry.correcciones_por_id.contains_key(&id) {
            fuente_entry.correcciones_por_id.insert(id, c.clone());
        }
    }
    for co in &paq.consolidaciones_ia {
        if co.id.trim().is_empty() {
            continue;
        }
        let id = co.id.trim().to_string();
        if es_reciente || !fuente_entry.consolidaciones_por_id.contains_key(&id) {
            fuente_entry.consolidaciones_por_id.insert(id, co.clone());
        }
    }
    for ej in &paq.ejemplos_aciertos {
        if ej.id.trim().is_empty() {
            continue;
        }
        let id = ej.id.trim().to_string();
        if es_reciente || !fuente_entry.ejemplos_por_id.contains_key(&id) {
            fuente_entry.ejemplos_por_id.insert(id, ej.clone());
        }
    }

    // 3) Si el snapshot es más reciente (>= marca de agua), reemplazar contadores acumulados de esta fuente
    let estado_proc = if es_reciente {
        fuente_entry.formato_ultimo = paq.formato;
        fuente_entry.ultimo_exportado = paq.exportado.trim().to_string();
        if paq.exportacion_seq > 0 {
            fuente_entry.ultimo_seq = paq.exportacion_seq;
        }
        if !paq.uso_catalogos.sembrado_en.trim().is_empty() {
            fuente_entry.sembrado_en = paq.uso_catalogos.sembrado_en.trim().to_string();
        }
        if !paq.uso_catalogos.ultima_actualizacion.trim().is_empty() {
            fuente_entry.ultima_actualizacion_uso =
                paq.uso_catalogos.ultima_actualizacion.trim().to_string();
        }
        for f_imp in &paq.fuentes_importadas {
            let ft = f_imp.trim();
            if !ft.is_empty()
                && ft != source_id
                && !fuente_entry.fuentes_importadas.iter().any(|x| x == ft)
            {
                fuente_entry.fuentes_importadas.push(ft.to_string());
            }
        }
        fuente_entry.total_audiencias_contabilizadas =
            paq.uso_catalogos.total_audiencias_contabilizadas;
        fuente_entry.aciertos = paq.aciertos;
        fuente_entry.llamadas_induccion_llm_total = paq.llamadas_induccion_llm_total;
        fuente_entry.llamadas_extraccion_total = paq.llamadas_extraccion_total;
        fuente_entry.llamadas_consolidacion_total = paq.llamadas_consolidacion_total;
        fuente_entry.uso_catalogos = paq.uso_catalogos.clone();
        fuente_entry.grafo_asociaciones = paq.grafo_asociaciones.clone();
        fuente_entry.defensores_directorio = paq.defensores_directorio.clone();
        fuente_entry.juzgados_agregados_usuario = paq.juzgados_agregados_usuario.clone();
        fuente_entry.tipos_audiencia_agregados_usuario =
            paq.tipos_audiencia_agregados_usuario.clone();
        fuente_entry.preferencias_estilo = paq.preferencias_estilo.clone();
        if paq.preferencias_raw.is_some() {
            fuente_entry.preferencias_raw = paq.preferencias_raw.clone();
        }
        if !paq.preferencias_por_defensor_raw.is_empty() {
            fuente_entry.preferencias_por_defensor_raw = paq.preferencias_por_defensor_raw.clone();
        }
        for p in &paq.perfiles_sinteticos {
            let amb = alcance_canonico(&p.ambito);
            let reemplazar = match fuente_entry.perfiles_sinteticos.get(&amb) {
                None => true,
                Some(prev) => {
                    p.version > prev.version
                        || (p.version == prev.version
                            && comparar_rfc3339(&p.ultima_actualizacion, &prev.ultima_actualizacion)
                                != std::cmp::Ordering::Less)
                }
            };
            if reemplazar {
                fuente_entry.perfiles_sinteticos.insert(amb, p.clone());
            }
        }
        fuente_entry.alias_validados = paq.alias_validados.clone();
        if !paq.tabla_alias_raw.is_empty() {
            fuente_entry.tabla_alias_raw = paq.tabla_alias_raw.clone();
        }
        fuente_entry.patrones_abreviacion = paq.patrones_abreviacion.clone();
        "aplicado".to_string()
    } else {
        "obsoleto_backfill".to_string()
    };

    ledger.archivos_ingeridos.push(RegistroArchivoIngerido {
        sha256: sha.clone(),
        source_id,
        formato: paq.formato,
        exportacion_seq: paq.exportacion_seq,
        exportado: paq.exportado.trim().to_string(),
        version_app: paq.version_app.clone(),
        estado_procesamiento: estado_proc.clone(),
    });

    estado_proc
}

// ---------------------------------------------------------------------------
// Agregación entre fuentes, deduplicación .lokus y compactación de base
// ---------------------------------------------------------------------------

fn calcular_solapamiento_entre_fuentes(
    receptora: &FuenteLedger,
    importada: &FuenteLedger,
) -> (u32, bool) {
    let mut mapa_juz_imp: BTreeMap<String, u32> = BTreeMap::new();
    for it in &importada.uso_catalogos.juzgados.detalle {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            *mapa_juz_imp.entry(k).or_insert(0) += it.conteo;
        }
    }
    for it in &importada.uso_catalogos.juzgados.nuevos_no_predeterminados {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            let e = mapa_juz_imp.entry(k).or_insert(0);
            *e = (*e).max(it.conteo);
        }
    }

    let mut mapa_juz_rec: BTreeMap<String, u32> = BTreeMap::new();
    for it in &receptora.uso_catalogos.juzgados.detalle {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            *mapa_juz_rec.entry(k).or_insert(0) += it.conteo;
        }
    }
    for it in &receptora.uso_catalogos.juzgados.nuevos_no_predeterminados {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            let e = mapa_juz_rec.entry(k).or_insert(0);
            *e = (*e).max(it.conteo);
        }
    }

    let mut suma_juz_compartidos = 0u32;
    for (k, &c_rec) in &mapa_juz_rec {
        if let Some(&c_imp) = mapa_juz_imp.get(k) {
            suma_juz_compartidos = suma_juz_compartidos.saturating_add(c_rec.min(c_imp));
        }
    }

    let mut mapa_tip_imp: BTreeMap<String, u32> = BTreeMap::new();
    for it in &importada.uso_catalogos.tipos_audiencia.detalle {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            *mapa_tip_imp.entry(k).or_insert(0) += it.conteo;
        }
    }
    let mut mapa_tip_rec: BTreeMap<String, u32> = BTreeMap::new();
    for it in &receptora.uso_catalogos.tipos_audiencia.detalle {
        let k = normalizar_clave_catalogo(&it.nombre);
        if !k.is_empty() && it.conteo > 0 {
            *mapa_tip_rec.entry(k).or_insert(0) += it.conteo;
        }
    }
    let mut suma_tip_compartidos = 0u32;
    for (k, &c_rec) in &mapa_tip_rec {
        if let Some(&c_imp) = mapa_tip_imp.get(k) {
            suma_tip_compartidos = suma_tip_compartidos.saturating_add(c_rec.min(c_imp));
        }
    }

    let cota_max = receptora
        .total_audiencias_contabilizadas
        .min(importada.total_audiencias_contabilizadas);
    let detalle_overlap = suma_juz_compartidos.max(suma_tip_compartidos);
    let deduccion = if detalle_overlap > 0 {
        detalle_overlap.min(cota_max)
    } else {
        cota_max
    };

    let crecio_despues = matches!(
        comparar_rfc3339(
            &importada.ultima_actualizacion_uso,
            &receptora.ultima_actualizacion_uso
        ),
        std::cmp::Ordering::Greater
    );

    (deduccion, crecio_despues)
}

fn recalcular_agregados_ledger(ledger: &mut LedgerIngesta, catalogos_base_juzgados: &[String]) {
    let snapshot_claves: Vec<String> = ledger.fuentes.keys().cloned().collect();
    let copia_fuentes: BTreeMap<String, FuenteLedger> = ledger.fuentes.clone();
    ledger.advertencias_solapamiento.clear();

    let mut total_acumulado = 0u32;
    for sid in &snapshot_claves {
        let f_rec = &copia_fuentes[sid];
        let mut deduccion_total = 0u32;
        for imp_id in &f_rec.fuentes_importadas {
            let es_subsumida = f_rec.fuentes_importadas.iter().any(|other_id| {
                other_id != imp_id
                    && copia_fuentes.get(other_id).is_some_and(|f_other| {
                        f_other.fuentes_importadas.contains(imp_id)
                            && !copia_fuentes
                                .get(imp_id)
                                .is_some_and(|f_imp| f_imp.fuentes_importadas.contains(other_id))
                    })
            });
            if es_subsumida {
                continue;
            }
            if let Some(f_imp) = copia_fuentes.get(imp_id) {
                let (ded, crecio) = calcular_solapamiento_entre_fuentes(f_rec, f_imp);
                deduccion_total = deduccion_total.saturating_add(ded);
                let flag = if crecio {
                    format!(
                        "overlap_con_crecimiento_posterior: '{}' importó '{}' (deducción acotada={} audiencias; '{}' continuó creciendo después)",
                        sid, imp_id, ded, imp_id
                    )
                } else {
                    format!(
                        "overlap_lokus_deduplicado: '{}' importó '{}' (deducción acotada={} audiencias)",
                        sid, imp_id, ded
                    )
                };
                if !ledger.advertencias_solapamiento.contains(&flag) {
                    ledger.advertencias_solapamiento.push(flag);
                }
            }
        }
        let netas = f_rec
            .total_audiencias_contabilizadas
            .saturating_sub(deduccion_total);
        if let Some(f_mut) = ledger.fuentes.get_mut(sid) {
            f_mut.audiencias_netas = netas;
        }
        total_acumulado = total_acumulado.saturating_add(netas);
    }

    ledger.total_audiencias_acumuladas = total_acumulado;
    ledger.total_fuentes_activas = ledger.fuentes.len();

    // Agregar candidatos de juzgados, tipos y horas entre fuentes
    let set_juz_base: BTreeSet<String> = catalogos_base_juzgados
        .iter()
        .map(|j| normalizar_clave_catalogo(j))
        .collect();
    let set_tip_base: BTreeSet<String> = TIPOS_AUDIENCIA
        .iter()
        .map(|t| normalizar_clave_catalogo(t))
        .collect();

    // Mapas por clave normalizada -> (nombre_display, origen, BTreeMap<source_id, conteo>)
    let mut juz_por_fuente: BTreeMap<String, (String, String, BTreeMap<String, u32>)> =
        BTreeMap::new();
    let mut tip_por_fuente: BTreeMap<String, (String, String, BTreeMap<String, u32>)> =
        BTreeMap::new();
    let mut horas_no_rapidas: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();

    for (sid, f) in &ledger.fuentes {
        for it in f
            .uso_catalogos
            .juzgados
            .nuevos_no_predeterminados
            .iter()
            .chain(f.uso_catalogos.juzgados.detalle.iter())
        {
            if it.origen == "predeterminado" {
                continue;
            }
            if es_entrada_catalogo_invalida(&it.nombre, "juzgado").is_some() {
                continue;
            }
            let k = normalizar_clave_catalogo(&it.nombre);
            if k.is_empty() {
                continue;
            }
            let entry = juz_por_fuente.entry(k).or_insert_with(|| {
                (
                    colapsar_espacios(&it.nombre),
                    it.origen.clone(),
                    BTreeMap::new(),
                )
            });
            entry.0 = elegir_nombre_mas_rico(&entry.0, &it.nombre);
            let c_entry = entry.2.entry(sid.clone()).or_insert(0);
            *c_entry = (*c_entry).max(it.conteo);
        }
        for j_usr in &f.juzgados_agregados_usuario {
            if es_entrada_catalogo_invalida(j_usr, "juzgado").is_some() {
                continue;
            }
            let k = normalizar_clave_catalogo(j_usr);
            if k.is_empty() {
                continue;
            }
            let entry = juz_por_fuente.entry(k).or_insert_with(|| {
                (
                    colapsar_espacios(j_usr),
                    "agregado_usuario".to_string(),
                    BTreeMap::new(),
                )
            });
            entry.0 = elegir_nombre_mas_rico(&entry.0, j_usr);
            entry.2.entry(sid.clone()).or_insert(0);
        }

        for it in f
            .uso_catalogos
            .tipos_audiencia
            .nuevos_no_predeterminados
            .iter()
            .chain(f.uso_catalogos.tipos_audiencia.detalle.iter())
        {
            if it.origen == "predeterminado" {
                continue;
            }
            if es_entrada_catalogo_invalida(&it.nombre, "tipo").is_some() {
                continue;
            }
            let k = normalizar_clave_catalogo(&it.nombre);
            if k.is_empty() {
                continue;
            }
            let entry = tip_por_fuente.entry(k).or_insert_with(|| {
                (
                    colapsar_espacios(&it.nombre),
                    it.origen.clone(),
                    BTreeMap::new(),
                )
            });
            entry.0 = elegir_nombre_mas_rico(&entry.0, &it.nombre);
            let c_entry = entry.2.entry(sid.clone()).or_insert(0);
            *c_entry = (*c_entry).max(it.conteo);
        }
        for t_usr in &f.tipos_audiencia_agregados_usuario {
            if es_entrada_catalogo_invalida(t_usr, "tipo").is_some() {
                continue;
            }
            let k = normalizar_clave_catalogo(t_usr);
            if k.is_empty() {
                continue;
            }
            let entry = tip_por_fuente.entry(k).or_insert_with(|| {
                (
                    colapsar_espacios(t_usr),
                    "agregado_usuario".to_string(),
                    BTreeMap::new(),
                )
            });
            entry.0 = elegir_nombre_mas_rico(&entry.0, t_usr);
            entry.2.entry(sid.clone()).or_insert(0);
        }

        for h in &f.uso_catalogos.horas.horas_mas_frecuentes_no_rapidas {
            if !h.hora_24.trim().is_empty() && h.conteo > 0 {
                let m = horas_no_rapidas
                    .entry(h.hora_24.trim().to_string())
                    .or_default();
                let c = m.entry(sid.clone()).or_insert(0);
                *c = (*c).max(h.conteo);
            }
        }
    }

    let sumar_con_dedup_lokus = |por_fuente: &BTreeMap<String, u32>| -> (u32, usize) {
        let mut total = 0u32;
        let mut fuentes_netas = 0usize;
        for (sid, &c_sid) in por_fuente {
            let mut ded = 0u32;
            if let Some(f_rec) = ledger.fuentes.get(sid) {
                for imp_id in &f_rec.fuentes_importadas {
                    if let Some(&c_imp) = por_fuente.get(imp_id) {
                        ded = ded.saturating_add(c_sid.min(c_imp));
                    }
                }
            }
            let neto = c_sid.saturating_sub(ded);
            if neto > 0 || (c_sid == 0 && ded == 0) {
                fuentes_netas += 1;
            }
            total = total.saturating_add(neto);
        }
        (total, fuentes_netas.max(1))
    };

    let mut juz_candidatos: Vec<CandidatoCatalogoAgregado> = juz_por_fuente
        .into_iter()
        .map(|(k, (nombre, origen, mapa))| {
            let (conteo_total, fuentes_confirmantes) = sumar_con_dedup_lokus(&mapa);
            let ya_en_base = set_juz_base.contains(&k);
            let recomendado = conteo_total >= 2
                || (origen == "agregado_usuario" && conteo_total >= 1)
                || fuentes_confirmantes >= 2;
            CandidatoCatalogoAgregado {
                nombre,
                origen,
                conteo_total,
                fuentes_confirmantes,
                ya_en_catalogo_base: ya_en_base,
                recomendado_incorporar: recomendado,
            }
        })
        .collect();
    juz_candidatos.sort_by(|a, b| {
        b.conteo_total
            .cmp(&a.conteo_total)
            .then_with(|| a.nombre.cmp(&b.nombre))
    });

    let mut tip_candidatos: Vec<CandidatoCatalogoAgregado> = tip_por_fuente
        .into_iter()
        .map(|(k, (nombre, origen, mapa))| {
            let (conteo_total, fuentes_confirmantes) = sumar_con_dedup_lokus(&mapa);
            let ya_en_base = set_tip_base.contains(&k);
            let recomendado = conteo_total >= 1 || fuentes_confirmantes >= 2;
            CandidatoCatalogoAgregado {
                nombre,
                origen,
                conteo_total,
                fuentes_confirmantes,
                ya_en_catalogo_base: ya_en_base,
                recomendado_incorporar: recomendado,
            }
        })
        .collect();
    tip_candidatos.sort_by(|a, b| {
        b.conteo_total
            .cmp(&a.conteo_total)
            .then_with(|| a.nombre.cmp(&b.nombre))
    });

    let mut horas_frec: Vec<(String, u32)> = horas_no_rapidas
        .into_iter()
        .map(|(h, mapa)| {
            let (tot, _) = sumar_con_dedup_lokus(&mapa);
            (h, tot)
        })
        .filter(|(_, c)| *c > 0)
        .collect();
    horas_frec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // Opciones predeterminadas con 0 usos globales (intersección entre todas las fuentes con audiencias > 0)
    let fuentes_con_uso: Vec<&FuenteLedger> = ledger
        .fuentes
        .values()
        .filter(|f| f.total_audiencias_contabilizadas > 0)
        .collect();

    let mut horas_sin_uso_global: Vec<String> = Vec::new();
    let mut juz_sin_uso_global: Vec<String> = Vec::new();
    let mut tip_sin_uso_global: Vec<String> = Vec::new();

    if !fuentes_con_uso.is_empty() {
        for (i, f) in fuentes_con_uso.iter().enumerate() {
            let h_set: BTreeSet<String> = f
                .uso_catalogos
                .horas
                .horas_rapidas_sin_uso
                .iter()
                .cloned()
                .collect();
            let j_set: BTreeSet<String> = f
                .uso_catalogos
                .juzgados
                .predeterminados_sin_uso
                .iter()
                .cloned()
                .collect();
            let t_set: BTreeSet<String> = f
                .uso_catalogos
                .tipos_audiencia
                .predeterminados_sin_uso
                .iter()
                .cloned()
                .collect();
            if i == 0 {
                horas_sin_uso_global = h_set.into_iter().collect();
                juz_sin_uso_global = j_set.into_iter().collect();
                tip_sin_uso_global = t_set.into_iter().collect();
            } else {
                horas_sin_uso_global.retain(|x| h_set.contains(x));
                juz_sin_uso_global.retain(|x| j_set.contains(x));
                tip_sin_uso_global.retain(|x| t_set.contains(x));
            }
        }
    }

    ledger.uso_catalogos_agregado = UsoCatalogosAgregadoLedger {
        total_audiencias_contabilizadas: total_acumulado,
        juzgados_candidatos: juz_candidatos,
        tipos_candidatos: tip_candidatos,
        horas_no_rapidas_frecuentes: horas_frec,
        horas_rapidas_sin_uso_global: horas_sin_uso_global,
        juzgados_predeterminados_sin_uso_global: juz_sin_uso_global,
        tipos_predeterminados_sin_uso_global: tip_sin_uso_global,
    };
}

fn compactar_y_actualizar_memoria_base(
    base_original: &MemoriaAprendizaje,
    ledger: &LedgerIngesta,
) -> Result<(MemoriaAprendizaje, usize), String> {
    let invariantes_originales: Vec<ReglaAprendida> = base_original
        .reglas
        .iter()
        .filter(|r| es_regla_base_invariante(r))
        .cloned()
        .collect();
    let ids_reglas_previas: BTreeSet<String> =
        base_original.reglas.iter().map(|r| r.id.clone()).collect();

    // 1) Consolidar reglas por ID y equivalencia semántica entre fuentes del ledger
    // Primero agrupamos por ID de regla entre fuentes (excluyendo copia redundante por .lokus)
    let mut reglas_por_id_fuentes: BTreeMap<String, Vec<(String, ReglaAprendida)>> = BTreeMap::new();
    for (sid, f) in &ledger.fuentes {
        for (rid, r) in &f.reglas_por_id {
            if es_regla_base_invariante(r) {
                continue;
            }
            reglas_por_id_fuentes
                .entry(rid.clone())
                .or_default()
                .push((sid.clone(), r.clone()));
        }
    }

    let mut candidatas_limpias: Vec<ReglaAprendida> = Vec::new();
    for r_prev in &base_original.reglas {
        if es_regla_base_invariante(r_prev) || reglas_por_id_fuentes.contains_key(&r_prev.id) {
            continue;
        }
        if r_prev.estado != "activa" {
            continue;
        }
        if contiene_marcador_anonimizacion(&r_prev.regla)
            || r_prev
                .triggers
                .iter()
                .any(|t| contiene_marcador_anonimizacion(t))
            || contradice_reglas_base_invariante(&r_prev.regla).is_some()
        {
            continue;
        }
        candidatas_limpias.push(r_prev.clone());
    }

    for (_rid, lista_fuentes) in reglas_por_id_fuentes {
        // Si alguna fuente tiene esta regla en estado "retirada" o "cuarentena" con actualización más reciente,
        // respetamos el retiro y no la promovemos a aprendizaje_base.json.
        let mut mas_reciente = &lista_fuentes[0].1;
        for (_, r) in &lista_fuentes[1..] {
            let cmp = comparar_rfc3339(&r.ultima_actualizacion, &mas_reciente.ultima_actualizacion);
            if cmp == std::cmp::Ordering::Greater
                || (cmp == std::cmp::Ordering::Equal && r.estado == "retirada")
            {
                mas_reciente = r;
            }
        }
        if matches!(
            mas_reciente.estado.as_str(),
            "retirada" | "cuarentena" | "candidata"
        ) {
            continue;
        }
        if contiene_marcador_anonimizacion(&mas_reciente.regla)
            || mas_reciente
                .triggers
                .iter()
                .any(|t| contiene_marcador_anonimizacion(t))
        {
            continue;
        }
        if contradice_reglas_base_invariante(&mas_reciente.regla).is_some() {
            continue;
        }
        let (triggers_limpios, todos_basura) =
            limpiar_triggers_regla(&mas_reciente.triggers, &mas_reciente.regla);
        if todos_basura {
            continue;
        }

        // Combinar evidencia entre fuentes independientes (evitando sumar una fuente importada vía .lokus)
        let fuentes_presentes: BTreeSet<String> =
            lista_fuentes.iter().map(|(s, _)| s.clone()).collect();
        let mut fusionada = mas_reciente.clone();
        let campo_inf = inferir_campo_objetivo_desde_tipo(&fusionada.tipo_error);
        if campo_inf != "general" {
            fusionada.campo_objetivo = campo_inf;
        }
        fusionada.triggers = triggers_limpios;
        fusionada.defensor_alcance = alcance_canonico(&fusionada.defensor_alcance);

        let mut confirmantes: BTreeSet<String> = BTreeSet::new();
        let mut delta_alpha = 0.0_f64;
        let mut delta_beta = 0.0_f64;
        let mut total_refuerzos = 0u32;
        let mut total_impresiones = 0u32;
        let mut total_aciertos = 0u32;
        let mut total_fallos = 0u32;
        let mut fuentes_independientes = 0usize;

        for (sid, r) in &lista_fuentes {
            let f_obj = &ledger.fuentes[sid];
            let es_copia_lokus = f_obj
                .fuentes_importadas
                .iter()
                .any(|imp| fuentes_presentes.contains(imp));
            for d in &r.defensores_confirmantes {
                let dn = normalizar_clave_defensor(d);
                if !dn.is_empty() {
                    confirmantes.insert(dn);
                }
            }
            let alc_d = normalizar_clave_defensor(&r.defensor_alcance);
            if !alc_d.is_empty() && !es_alcance_general_str(&alc_d) {
                confirmantes.insert(alc_d);
            }
            if es_copia_lokus && fuentes_independientes > 0 {
                continue;
            }
            fuentes_independientes += 1;
            delta_alpha += (r.alpha - PRIOR_BETA_ALPHA).max(0.0);
            delta_beta += (r.beta - PRIOR_BETA_BETA).max(0.0);
            total_refuerzos = total_refuerzos.saturating_add(r.refuerzos.max(1));
            total_impresiones = total_impresiones.saturating_add(r.impresiones);
            total_aciertos = total_aciertos.saturating_add(r.aciertos_directos);
            total_fallos = total_fallos.saturating_add(r.fallos_directos);
        }

        fusionada.alpha = PRIOR_BETA_ALPHA + delta_alpha;
        fusionada.beta = PRIOR_BETA_BETA + delta_beta;
        normalizar_ess_par(&mut fusionada.alpha, &mut fusionada.beta, MAX_ESS_N);
        fusionada.refuerzos = total_refuerzos.max(mas_reciente.refuerzos);
        fusionada.impresiones = total_impresiones.max(mas_reciente.impresiones);
        fusionada.aciertos_directos = total_aciertos.max(mas_reciente.aciertos_directos);
        fusionada.fallos_directos = total_fallos.max(mas_reciente.fallos_directos);
        fusionada.defensores_confirmantes = confirmantes.into_iter().collect();
        if fusionada.defensores_confirmantes.len() >= 2 || fuentes_independientes >= 2 {
            fusionada.defensor_alcance = "*".to_string();
        }

        let q10 = limite_inferior_bayesiano(fusionada.alpha, fusionada.beta);
        if q10 < UMBRAL_Q10_REGLA_BASE {
            continue;
        }
        if fusionada.estado == "prueba" && (q10 < 0.42 || fusionada.refuerzos < 2) {
            continue;
        }
        fusionada.estado = "activa".to_string();
        candidatas_limpias.push(fusionada);
    }

    // Deduplicación semántica (es_misma_regla en mismo alcance) y contra base_invariante
    candidatas_limpias.sort_by(|a, b| {
        let qa = limite_inferior_bayesiano(a.alpha, a.beta);
        let qb = limite_inferior_bayesiano(b.alpha, b.beta);
        qb.partial_cmp(&qa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.refuerzos.cmp(&a.refuerzos))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut reglas_admitidas: Vec<ReglaAprendida> = Vec::new();
    for cand in candidatas_limpias {
        if invariantes_originales
            .iter()
            .any(|inv| inv.id == cand.id || es_misma_regla(&inv.regla, &cand.regla))
        {
            continue;
        }
        if let Some(existente) = reglas_admitidas.iter_mut().find(|r| {
            r.id == cand.id
                || (alcance_canonico(&r.defensor_alcance)
                    == alcance_canonico(&cand.defensor_alcance)
                    && es_misma_regla(&r.regla, &cand.regla))
        }) {
            existente.refuerzos = existente.refuerzos.saturating_add(cand.refuerzos);
            existente.alpha += (cand.alpha - PRIOR_BETA_ALPHA).max(0.0);
            existente.beta += (cand.beta - PRIOR_BETA_BETA).max(0.0);
            normalizar_ess_par(&mut existente.alpha, &mut existente.beta, MAX_ESS_N);
            for d in cand.defensores_confirmantes {
                if !existente.defensores_confirmantes.contains(&d) {
                    existente.defensores_confirmantes.push(d);
                }
            }
            if existente.defensores_confirmantes.len() >= 2 {
                existente.defensor_alcance = "*".to_string();
            }
            continue;
        }
        reglas_admitidas.push(cand);
    }

    let cupo_aprendidas =
        MAX_REGLAS_BASE_COMPACTADAS.saturating_sub(invariantes_originales.len());
    if reglas_admitidas.len() > cupo_aprendidas {
        reglas_admitidas.truncate(cupo_aprendidas);
    }

    let reglas_nuevas_agregadas = reglas_admitidas
        .iter()
        .filter(|r| !ids_reglas_previas.contains(&r.id))
        .count();

    let mut reglas_finales = invariantes_originales.clone();
    reglas_finales.extend(reglas_admitidas);

    // 2) Compactar correcciones (conservar base-* originales + selección diversa MMR de analizadas limpias)
    let mut correcciones_base_fijas: Vec<CorreccionIa> = base_original
        .correcciones
        .iter()
        .filter(|c| c.id.starts_with("base-"))
        .cloned()
        .collect();
    if correcciones_base_fijas.is_empty() && base_original.correcciones.len() <= 2 {
        correcciones_base_fijas = base_original.correcciones.clone();
    }
    let ids_corr_fijas: BTreeSet<String> = correcciones_base_fijas
        .iter()
        .map(|c| c.id.clone())
        .collect();

    let mut mapa_corr_unidas: BTreeMap<String, CorreccionIa> = BTreeMap::new();
    for f in ledger.fuentes.values() {
        for (cid, c) in &f.correcciones_por_id {
            if ids_corr_fijas.contains(cid) {
                continue;
            }
            if c.estado != "analizada" {
                continue;
            }
            if contiene_marcador_anonimizacion(&c.final_usuario.juzgado)
                || contiene_marcador_anonimizacion(&c.final_usuario.tipo)
                || contiene_marcador_anonimizacion(&c.prediccion.juzgado)
                || contiene_marcador_anonimizacion(&c.prediccion.tipo)
                || contiene_falso_positivo_procesado_en_texto(&c.texto_original)
                || contiene_falso_positivo_procesado_en_texto(&c.final_usuario.observaciones)
                || contiene_falso_positivo_procesado_en_texto(&c.prediccion.observaciones)
            {
                continue;
            }
            if !c.final_usuario.juzgado.trim().is_empty()
                && es_entrada_catalogo_invalida(&c.final_usuario.juzgado, "juzgado").is_some()
            {
                continue;
            }
            if !c.final_usuario.tipo.trim().is_empty()
                && es_entrada_catalogo_invalida(&c.final_usuario.tipo, "tipo").is_some()
            {
                continue;
            }
            if c.prediccion
                .juzgado
                .to_lowercase()
                .contains("maunicipal")
                || c.texto_original.to_lowercase().contains("maunicipal")
                || c.texto_original.to_lowercase().contains("fdsafe")
            {
                continue;
            }
            mapa_corr_unidas.entry(cid.clone()).or_insert_with(|| {
                let mut cl = c.clone();
                cl.defensor_clave = normalizar_clave_defensor(&cl.defensor_clave);
                cl.regla_id_asociada.clear();
                cl
            });
        }
    }

    let lista_corr_cand: Vec<CorreccionIa> = mapa_corr_unidas.into_values().collect();
    let cupo_corr =
        MAX_CORRECCIONES_BASE_COMPACTADAS.saturating_sub(correcciones_base_fijas.len());
    let corr_seleccionadas = if lista_corr_cand.len() <= cupo_corr {
        lista_corr_cand
    } else {
        let docs: Vec<Vec<String>> = lista_corr_cand
            .iter()
            .map(|c| {
                let mut d = tokenizar_judicial(&c.texto_original);
                for cm in &c.campos {
                    d.push(cm.campo.clone());
                    d.extend(tokenizar_judicial(&cm.valor_usuario));
                }
                d
            })
            .collect();
        let query: Vec<String> = vec![
            "juzgado".into(),
            "motivo".into(),
            "tipo".into(),
            "realizada".into(),
            "observaciones".into(),
            "audiencia".into(),
        ];
        let bm25 = calcular_bm25(&query, &docs);
        let candidatos_idx: Vec<(usize, f64)> = bm25
            .into_iter()
            .enumerate()
            .map(|(i, s)| (i, s + 1.0))
            .collect();
        let elegidos = seleccionar_mmr(&candidatos_idx, &docs, cupo_corr, 0.65);
        elegidos
            .into_iter()
            .map(|idx| lista_corr_cand[idx].clone())
            .collect()
    };

    let mut correcciones_finales = correcciones_base_fijas;
    correcciones_finales.extend(corr_seleccionadas);

    // 3) Compactar tabla_alias (deduplicando variantes de tildes y promoviendo consenso a "*")
    let mut alias_agrupados: BTreeMap<
        (String, String, String, String),
        AliasEstadistico,
    > = BTreeMap::new();

    for f in ledger.fuentes.values() {
        let fuente_aliases: Vec<AliasEstadistico> = if !f.tabla_alias_raw.is_empty() {
            f.tabla_alias_raw.clone()
        } else {
            f.alias_validados
                .iter()
                .map(|av| AliasEstadistico {
                    campo: av.campo.clone(),
                    entrada_norm: av.entrada_norm.clone(),
                    canonico: av.canonico.clone(),
                    alpha: resolver_alpha_desde_q10(av.confianza_q10, PRIOR_ALIAS_BETA),
                    beta: PRIOR_ALIAS_BETA,
                    defensor_alcance: av.defensor_alcance.clone(),
                    defensores_confirmantes: if es_alcance_general_str(&av.defensor_alcance) {
                        Vec::new()
                    } else {
                        vec![normalizar_clave_defensor(&av.defensor_alcance)]
                    },
                    ultima_actualizacion: f.ultimo_exportado.clone(),
                })
                .collect()
        };

        for a in fuente_aliases {
            if es_alias_invalido(&a.entrada_norm, &a.canonico, &a.campo).is_some() {
                continue;
            }
            let campo = a.campo.trim().to_lowercase();
            let entrada = normalizar_texto_sin_tildes(&a.entrada_norm);
            let can_key = normalizar_clave_catalogo(&a.canonico);
            let alc = alcance_canonico(&a.defensor_alcance);
            if campo.is_empty() || entrada.is_empty() || can_key.is_empty() {
                continue;
            }
            let key = (campo.clone(), entrada.clone(), can_key, alc.clone());
            let entry = alias_agrupados.entry(key).or_insert_with(|| AliasEstadistico {
                campo,
                entrada_norm: entrada,
                canonico: colapsar_espacios(&a.canonico),
                alpha: PRIOR_ALIAS_ALPHA,
                beta: PRIOR_ALIAS_BETA,
                defensor_alcance: alc,
                defensores_confirmantes: Vec::new(),
                ultima_actualizacion: a.ultima_actualizacion.clone(),
            });
            entry.canonico = elegir_nombre_mas_rico(&entry.canonico, &a.canonico);
            entry.alpha += (a.alpha - PRIOR_ALIAS_ALPHA).max(0.5);
            entry.beta += (a.beta - PRIOR_ALIAS_BETA).max(0.0);
            normalizar_ess_par(&mut entry.alpha, &mut entry.beta, MAX_ESS_N);
            for d in &a.defensores_confirmantes {
                let dn = normalizar_clave_defensor(d);
                if !dn.is_empty() && !entry.defensores_confirmantes.contains(&dn) {
                    entry.defensores_confirmantes.push(dn);
                }
            }
            let alc_d = normalizar_clave_defensor(&a.defensor_alcance);
            if !alc_d.is_empty()
                && !es_alcance_general_str(&alc_d)
                && !entry.defensores_confirmantes.contains(&alc_d)
            {
                entry.defensores_confirmantes.push(alc_d);
            }
            if comparar_rfc3339(&a.ultima_actualizacion, &entry.ultima_actualizacion)
                == std::cmp::Ordering::Greater
            {
                entry.ultima_actualizacion = a.ultima_actualizacion.clone();
            }
        }
    }

    let mut alias_finales: Vec<AliasEstadistico> = alias_agrupados
        .into_values()
        .filter(|a| limite_inferior_bayesiano(a.alpha, a.beta) >= UMBRAL_Q10_ALIAS_BASE)
        .map(|mut a| {
            if a.defensores_confirmantes.len() >= 2 {
                a.defensor_alcance = "*".to_string();
            }
            a
        })
        .collect();
    alias_finales.sort_by(|a, b| {
        let qa = limite_inferior_bayesiano(a.alpha, a.beta);
        let qb = limite_inferior_bayesiano(b.alpha, b.beta);
        qb.partial_cmp(&qa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.campo.cmp(&b.campo))
            .then_with(|| a.entrada_norm.cmp(&b.entrada_norm))
    });
    if alias_finales.len() > MAX_ALIAS_BASE_COMPACTADOS {
        alias_finales.truncate(MAX_ALIAS_BASE_COMPACTADOS);
    }

    // 4) Compactar ejemplos_aciertos limpios
    let mut mapa_ejemplos: BTreeMap<String, EjemploPositivo> = BTreeMap::new();
    for f in ledger.fuentes.values() {
        for (eid, ej) in &f.ejemplos_por_id {
            if contiene_marcador_anonimizacion(&ej.salida_anonimizada.juzgado)
                || contiene_marcador_anonimizacion(&ej.salida_anonimizada.tipo)
                || contiene_falso_positivo_procesado_en_texto(&ej.texto_anonimizado)
                || contiene_falso_positivo_procesado_en_texto(&ej.salida_anonimizada.observaciones)
            {
                continue;
            }
            if !ej.salida_anonimizada.juzgado.trim().is_empty()
                && es_entrada_catalogo_invalida(&ej.salida_anonimizada.juzgado, "juzgado").is_some()
            {
                continue;
            }
            mapa_ejemplos.entry(eid.clone()).or_insert_with(|| {
                let mut cl = ej.clone();
                cl.reglas_inyectadas.clear();
                cl
            });
        }
    }
    let mut ejemplos_finales: Vec<EjemploPositivo> = mapa_ejemplos.into_values().collect();
    if ejemplos_finales.len() > MAX_EJEMPLOS_BASE_COMPACTADOS {
        ejemplos_finales.truncate(MAX_EJEMPLOS_BASE_COMPACTADOS);
    }

    // 5) Perfiles sintéticos transversales limpios (solo "*")
    let mut perfiles_finales = base_original.perfiles_sinteticos.clone();
    for f in ledger.fuentes.values() {
        if let Some(p_gen) = f.perfiles_sinteticos.get("*") {
            if !p_gen.resumen.trim().is_empty()
                && !contiene_marcador_anonimizacion(&p_gen.resumen)
                && !contiene_falso_positivo_procesado_en_texto(&p_gen.resumen)
            {
                let actualizar = match perfiles_finales.get("*") {
                    None => true,
                    Some(prev) => p_gen.version >= prev.version,
                };
                if actualizar {
                    perfiles_finales.insert("*".to_string(), p_gen.clone());
                }
            }
        }
    }

    // 6) Verificación dura post-compactación: todas las reglas base_invariante originales deben estar intactas
    for orig in &invariantes_originales {
        let actual = reglas_finales
            .iter()
            .find(|r| r.id == orig.id)
            .ok_or_else(|| {
                format!(
                    "Violación de invariante: regla base_invariante '{}' desapareció durante compactación",
                    orig.id
                )
            })?;
        if actual.origen != orig.origen
            || actual.estado != "activa"
            || actual.regla != orig.regla
            || actual.triggers != orig.triggers
            || (actual.alpha - orig.alpha).abs() > 1e-9
            || (actual.beta - orig.beta).abs() > 1e-9
            || actual.campo_objetivo != orig.campo_objetivo
        {
            return Err(format!(
                "Violación de invariante: regla base_invariante '{}' fue alterada durante compactación",
                orig.id
            ));
        }
    }

    let mut nueva_base = base_original.clone();
    nueva_base.version = nueva_base.version.max(2);
    nueva_base.aciertos = 0;
    nueva_base.reglas = reglas_finales;
    nueva_base.correcciones = correcciones_finales;
    nueva_base.tabla_alias = alias_finales;
    nueva_base.ejemplos_aciertos = ejemplos_finales;
    nueva_base.perfiles_sinteticos = perfiles_finales;

    Ok((nueva_base, reglas_nuevas_agregadas))
}

fn es_alcance_general_str(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == "*" || t.eq_ignore_ascii_case("general")
}

// ---------------------------------------------------------------------------
// Generación de reporte de curación en Markdown y salida de consola
// ---------------------------------------------------------------------------

fn construir_reporte_markdown(
    resumen: &ResumenEjecucionIngesta,
    ledger: &LedgerIngesta,
    base: &MemoriaAprendizaje,
) -> String {
    let mut md = String::with_capacity(8192);
    use std::fmt::Write;

    let _ = writeln!(
        &mut md,
        "# Reporte de Curación e Ingesta de Autoaprendizaje IA y Catálogos — Lokus\n"
    );
    let _ = writeln!(&mut md, "- **Estado de ejecución:** `{}`", resumen.estado);
    let _ = writeln!(
        &mut md,
        "- **Modo dry-run:** `{}`",
        if resumen.dry_run { "sí" } else { "no" }
    );
    let _ = writeln!(
        &mut md,
        "- **Archivos recibidos:** `{}` (aplicados: `{}`, idempotentes omitidos: `{}`, backfill históricos: `{}`)",
        resumen.archivos_recibidos,
        resumen.archivos_aplicados,
        resumen.archivos_omitidos_idempotentes,
        resumen.archivos_obsoletos_backfill
    );
    let _ = writeln!(
        &mut md,
        "- **Fuentes activas en ledger:** `{}`",
        resumen.fuentes_totales
    );
    let _ = writeln!(
        &mut md,
        "- **Total audiencias contabilizadas (deduplicadas):** `{}`",
        resumen.total_audiencias_acumuladas
    );
    let _ = writeln!(
        &mut md,
        "- **Base embebida (`aprendizaje_base.json`):** `{}` reglas (`{}` `base_invariante` intactas, `{}` nuevas incorporadas), `{}` correcciones representativas, `{}` alias validados, `{}` ejemplos positivos.\n",
        resumen.reglas_totales_base,
        resumen.reglas_base_invariante,
        resumen.reglas_nuevas_agregadas,
        resumen.correcciones_totales_base,
        resumen.alias_totales_base,
        resumen.ejemplos_totales_base
    );

    if !ledger.advertencias_solapamiento.is_empty() {
        let _ = writeln!(
            &mut md,
            "## 1. Deduplicación Multi-Usuario y Solapamientos `.lokus`\n"
        );
        for adv in &ledger.advertencias_solapamiento {
            let _ = writeln!(&mut md, "- `{}`", adv);
        }
        let _ = writeln!(&mut md);
    }

    let _ = writeln!(
        &mut md,
        "## 2. Novedades de Uso de Catálogos (`uso_catalogos`)\n"
    );
    let _ = writeln!(&mut md, "### 2.1 Juzgados candidatos validados\n");
    if ledger.uso_catalogos_agregado.juzgados_candidatos.is_empty() {
        let _ = writeln!(&mut md, "- Sin nuevos juzgados candidatos.\n");
    } else {
        for j in &ledger.uso_catalogos_agregado.juzgados_candidatos {
            let estado_cat = if j.ya_en_catalogo_base {
                "ya en juzgados_base.json"
            } else if j.recomendado_incorporar {
                "recomendado incorporar"
            } else {
                "baja frecuencia (observación)"
            };
            let _ = writeln!(
                &mut md,
                "- **{}** — `{}` usos (`{}` fuente(s), origen: `{}`, `{}`)",
                j.nombre, j.conteo_total, j.fuentes_confirmantes, j.origen, estado_cat
            );
        }
        let _ = writeln!(&mut md);
    }

    let _ = writeln!(&mut md, "### 2.2 Tipos de audiencia candidatos validados\n");
    if ledger.uso_catalogos_agregado.tipos_candidatos.is_empty() {
        let _ = writeln!(&mut md, "- Sin nuevos tipos de audiencia candidatos.\n");
    } else {
        for t in &ledger.uso_catalogos_agregado.tipos_candidatos {
            let estado_cat = if t.ya_en_catalogo_base {
                "ya en TIPOS_AUDIENCIA"
            } else if t.recomendado_incorporar {
                "recomendado incorporar"
            } else {
                "baja frecuencia"
            };
            let _ = writeln!(
                &mut md,
                "- **{}** — `{}` usos (`{}` fuente(s), origen: `{}`, `{}`)",
                t.nombre, t.conteo_total, t.fuentes_confirmantes, t.origen, estado_cat
            );
        }
        let _ = writeln!(&mut md);
    }

    let _ = writeln!(
        &mut md,
        "### 2.3 Horas frecuentes no rápidas y opciones predeterminadas sin uso (0 usos)\n"
    );
    if !ledger
        .uso_catalogos_agregado
        .horas_no_rapidas_frecuentes
        .is_empty()
    {
        for (h, c) in &ledger.uso_catalogos_agregado.horas_no_rapidas_frecuentes {
            let _ = writeln!(&mut md, "- Hora frecuente no rápida: **{}** (`{}` usos)", h, c);
        }
    }
    if !ledger
        .uso_catalogos_agregado
        .horas_rapidas_sin_uso_global
        .is_empty()
    {
        let _ = writeln!(
            &mut md,
            "- Horas rápidas predeterminadas sin uso (0 usos globales): `{}`",
            ledger
                .uso_catalogos_agregado
                .horas_rapidas_sin_uso_global
                .join("`, `")
        );
    } else {
        let horas_def: Vec<&str> = HORAS_RAPIDAS.iter().map(|(h, _)| *h).collect();
        let _ = writeln!(
            &mut md,
            "- Horas rápidas evaluadas (0 usos / nunca usados verificados): `{}`",
            horas_def.join("`, `")
        );
    }
    let _ = writeln!(
        &mut md,
        "- Juzgados predeterminados sin uso global (protegidos por cobertura regional): `{}`",
        ledger
            .uso_catalogos_agregado
            .juzgados_predeterminados_sin_uso_global
            .len()
    );
    let _ = writeln!(
        &mut md,
        "- Tipos de audiencia predeterminados sin uso global (protegidos): `{}`\n",
        ledger
            .uso_catalogos_agregado
            .tipos_predeterminados_sin_uso_global
            .len()
    );

    let _ = writeln!(
        &mut md,
        "## 3. Cuarentena y Descarte de Contaminación (`[PROCESADO]`, Typos y Ruido)\n"
    );
    if ledger.cuarentena.is_empty() {
        let _ = writeln!(
            &mut md,
            "- `0` elementos en cuarentena o descartados en esta ejecución.\n"
        );
    } else {
        for item in &ledger.cuarentena {
            let _ = writeln!(
                &mut md,
                "- **[Cuarentena / Descartado]** `{}` (`{}` en `{}`): `{}` — motivo: `{}`",
                item.categoria, item.id_o_clave, item.source_id, item.valor, item.motivo
            );
        }
        let _ = writeln!(&mut md);
    }

    let _ = writeln!(
        &mut md,
        "## 4. Reglas Activas y Compactadas en `aprendizaje_base.json`\n"
    );
    for r in &base.reglas {
        let q10 = limite_inferior_bayesiano(r.alpha, r.beta);
        let _ = writeln!(
            &mut md,
            "- `{}` [`{}` | campo: `{}` | alcance: `{}` | q10: `{:.3}`]: {}",
            r.id, r.origen, r.campo_objetivo, r.defensor_alcance, q10, r.regla
        );
    }

    // Añadir sección de sugerencias_build limpias e inferencia de patrones de abreviación
    let mut sugerencias_limpias: Vec<SugerenciaBuild> = Vec::new();
    let mut patrones_limpios: Vec<PatronAbreviacionDiagnostico> = Vec::new();
    for f in ledger.fuentes.values() {
        for co in f.consolidaciones_por_id.values() {
            for sb in &co.sugerencias_build {
                if !contiene_marcador_anonimizacion(&sb.detalle)
                    && !sugerencias_limpias
                        .iter()
                        .any(|x| x.detalle == sb.detalle && x.categoria == sb.categoria)
                {
                    sugerencias_limpias.push(sb.clone());
                }
            }
        }
        for p in &f.patrones_abreviacion {
            if !contiene_marcador_anonimizacion(&p.patron_canonico)
                && !patrones_limpios.iter().any(|x| {
                    x.patron_entrada == p.patron_entrada && x.patron_canonico == p.patron_canonico
                })
            {
                patrones_limpios.push(p.clone());
            }
        }
    }

    if !patrones_limpios.is_empty() || !sugerencias_limpias.is_empty() {
        let _ = writeln!(
            &mut md,
            "\n## 5. Patrones de Abreviación y Sugerencias de Build Consolidados\n"
        );
        for p in &patrones_limpios {
            let _ = writeln!(
                &mut md,
                "- Patrón `{}` -> `{}` (alcance: `{}`, instancias: `{}`, consistencia: `{:.2}`)",
                p.patron_entrada,
                p.patron_canonico,
                p.defensor_alcance,
                p.total_instancias,
                p.consistencia
            );
        }
        for sb in &sugerencias_limpias {
            let _ = writeln!(
                &mut md,
                "- Sugerencia [{}]: {} (evidencia: {})",
                sb.categoria, sb.detalle, sb.evidencia
            );
        }
    }

    md
}

// ---------------------------------------------------------------------------
// Punto de entrada principal de ingesta
// ---------------------------------------------------------------------------

pub fn ejecutar_ingesta(opciones: &OpcionesIngesta) -> Result<SalidaCompletaIngesta, String> {
    if opciones.archivos_entrada.is_empty() {
        return Err(
            "Debe especificar al menos un archivo JSON de entrada para ingestar.".to_string(),
        );
    }

    if !opciones.ruta_base.exists() {
        return Err(format!(
            "No existe el archivo base de aprendizaje (--base): '{}'",
            opciones.ruta_base.display()
        ));
    }
    if !opciones.ruta_catalogos_base.exists() {
        return Err(format!(
            "No existe el archivo de catálogos base (--catalogos-base): '{}'",
            opciones.ruta_catalogos_base.display()
        ));
    }

    // 1) Cargar y validar --base
    let base_raw = fs::read_to_string(&opciones.ruta_base).map_err(|e| {
        format!(
            "No se pudo leer --base '{}': {}",
            opciones.ruta_base.display(),
            e
        )
    })?;
    let base_val: serde_json::Value = serde_json::from_str(&base_raw).map_err(|e| {
        format!(
            "JSON inválido en --base '{}': {}",
            opciones.ruta_base.display(),
            e
        )
    })?;
    if !base_val.is_object() || base_val.get("reglas").is_none() {
        return Err(format!(
            "El archivo --base '{}' no contiene una estructura MemoriaAprendizaje válida",
            opciones.ruta_base.display()
        ));
    }
    let base_original: MemoriaAprendizaje = serde_json::from_value(base_val).map_err(|e| {
        format!(
            "No se pudo deserializar MemoriaAprendizaje en --base '{}': {}",
            opciones.ruta_base.display(),
            e
        )
    })?;

    // 2) Cargar y validar --catalogos-base
    let cat_raw = fs::read_to_string(&opciones.ruta_catalogos_base).map_err(|e| {
        format!(
            "No se pudo leer --catalogos-base '{}': {}",
            opciones.ruta_catalogos_base.display(),
            e
        )
    })?;
    let catalogos_base_juzgados: Vec<String> = serde_json::from_str(&cat_raw).map_err(|e| {
        format!(
            "El archivo --catalogos-base '{}' no es un arreglo JSON válido de juzgados: {}",
            opciones.ruta_catalogos_base.display(),
            e
        )
    })?;

    // 3) Cargar y validar --ledger si ya existe
    let mut ledger: LedgerIngesta = if opciones.ruta_ledger.exists() {
        let led_raw = fs::read_to_string(&opciones.ruta_ledger).map_err(|e| {
            format!(
                "No se pudo leer --ledger '{}': {}",
                opciones.ruta_ledger.display(),
                e
            )
        })?;
        serde_json::from_str(&led_raw).map_err(|e| {
            format!(
                "JSON inválido en --ledger '{}': {}",
                opciones.ruta_ledger.display(),
                e
            )
        })?
    } else {
        LedgerIngesta::default()
    };

    // 4) Validar TODOS los archivos de entrada en memoria antes de mutar estado (atomicidad de lote)
    let mut entradas_validadas: Vec<PaqueteValidadoEntrada> =
        Vec::with_capacity(opciones.archivos_entrada.len());

    for ruta_in in &opciones.archivos_entrada {
        if !ruta_in.exists() {
            return Err(format!(
                "El archivo de entrada '{}' no existe",
                ruta_in.display()
            ));
        }
        let bytes = fs::read(ruta_in).map_err(|e| {
            format!(
                "No se pudo leer el archivo de entrada '{}': {}",
                ruta_in.display(),
                e
            )
        })?;
        let sha256 = sha256_hex(&bytes);
        let texto = String::from_utf8(bytes).map_err(|e| {
            format!(
                "El archivo '{}' no es UTF-8 válido: {}",
                ruta_in.display(),
                e
            )
        })?;
        let val: serde_json::Value = serde_json::from_str(&texto).map_err(|e| {
            format!(
                "JSON malformado en '{}': {}",
                ruta_in.display(),
                e
            )
        })?;
        let obj = val.as_object().ok_or_else(|| {
            format!(
                "El archivo '{}' debe contener un objeto JSON raíz",
                ruta_in.display()
            )
        })?;
        for campo_req in [
            "app",
            "formato",
            "exportado",
            "uso_catalogos",
            "reglas",
            "correcciones",
        ] {
            if !obj.contains_key(campo_req) {
                return Err(format!(
                    "El archivo '{}' no contiene el campo obligatorio '{}'",
                    ruta_in.display(),
                    campo_req
                ));
            }
        }
        let paq: PaqueteExportacion = serde_json::from_value(val).map_err(|e| {
            format!(
                "Estructura PaqueteExportacion inválida en '{}': {}",
                ruta_in.display(),
                e
            )
        })?;
        if paq.app.trim() != "Lokus" {
            return Err(format!(
                "Aplicación incompatible en '{}': app='{}' (se esperaba 'Lokus')",
                ruta_in.display(),
                paq.app
            ));
        }
        if !(4..=5).contains(&paq.formato) {
            return Err(format!(
                "Formato no soportado en '{}': formato={} (solo se soportan formatos 4 y 5)",
                ruta_in.display(),
                paq.formato
            ));
        }
        if paq.exportado.trim().is_empty() {
            return Err(format!(
                "El archivo '{}' tiene el campo 'exportado' vacío",
                ruta_in.display()
            ));
        }
        entradas_validadas.push(PaqueteValidadoEntrada {
            ruta: ruta_in.clone(),
            sha256,
            paquete: paq,
        });
    }

    // 5) Procesar todos los paquetes validados sobre el ledger en memoria
    let mut archivos_aplicados = 0usize;
    let mut archivos_omitidos_idempotentes = 0usize;
    let mut archivos_obsoletos_backfill = 0usize;

    for entrada in &entradas_validadas {
        let _ = &entrada.ruta;
        let est = aplicar_paquete_a_ledger(
            entrada,
            opciones.fuente_manual.as_deref(),
            &mut ledger,
        );
        match est.as_str() {
            "aplicado" => archivos_aplicados += 1,
            "idempotente_omitido" => archivos_omitidos_idempotentes += 1,
            "obsoleto_backfill" => archivos_obsoletos_backfill += 1,
            _ => {}
        }
    }

    recalcular_agregados_ledger(&mut ledger, &catalogos_base_juzgados);
    ledger.ultima_actualizacion = Local::now().to_rfc3339();

    let (nueva_base, reglas_nuevas_agregadas) =
        compactar_y_actualizar_memoria_base(&base_original, &ledger)?;

    let reglas_base_invariante = nueva_base
        .reglas
        .iter()
        .filter(|r| es_regla_base_invariante(r))
        .count();
    let juzgados_candidatos_validos = ledger
        .uso_catalogos_agregado
        .juzgados_candidatos
        .iter()
        .filter(|c| c.recomendado_incorporar)
        .count();
    let tipos_candidatos_validos = ledger
        .uso_catalogos_agregado
        .tipos_candidatos
        .iter()
        .filter(|c| c.recomendado_incorporar)
        .count();

    let estado = if archivos_aplicados == 0 && archivos_obsoletos_backfill == 0 {
        "idempotente_sin_cambios".to_string()
    } else {
        "actualizado".to_string()
    };

    let resumen = ResumenEjecucionIngesta {
        estado,
        dry_run: opciones.dry_run,
        archivos_recibidos: entradas_validadas.len(),
        archivos_aplicados,
        archivos_omitidos_idempotentes,
        archivos_obsoletos_backfill,
        fuentes_totales: ledger.fuentes.len(),
        total_audiencias_acumuladas: ledger.total_audiencias_acumuladas,
        reglas_base_invariante,
        reglas_totales_base: nueva_base.reglas.len(),
        reglas_nuevas_agregadas,
        correcciones_totales_base: nueva_base.correcciones.len(),
        alias_totales_base: nueva_base.tabla_alias.len(),
        ejemplos_totales_base: nueva_base.ejemplos_aciertos.len(),
        items_en_cuarentena: ledger.cuarentena.len(),
        juzgados_candidatos_validos,
        tipos_candidatos_validos,
        advertencias_solapamiento: ledger.advertencias_solapamiento.clone(),
    };

    let reporte_markdown = construir_reporte_markdown(&resumen, &ledger, &nueva_base);

    // 6) Persistir en disco solo si NO es --dry-run (excepto --reporte si se pidió explícitamente en ruta distinta)
    if !opciones.dry_run {
        // Si fue 100% no-op idempotente y ya existían ambos archivos, evitar tocar bytes innecesariamente
        let es_noop_total =
            archivos_aplicados == 0 && archivos_obsoletos_backfill == 0 && opciones.ruta_ledger.exists();
        if !es_noop_total {
            if let Some(parent) = opciones.ruta_base.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            "No se pudo crear directorio para --base '{}': {}",
                            parent.display(),
                            e
                        )
                    })?;
                }
            }
            let base_json = serde_json::to_string_pretty(&nueva_base)
                .map_err(|e| format!("Error serializando --base: {}", e))?;
            fs::write(&opciones.ruta_base, format!("{}\n", base_json)).map_err(|e| {
                format!(
                    "No se pudo escribir --base '{}': {}",
                    opciones.ruta_base.display(),
                    e
                )
            })?;

            if let Some(parent) = opciones.ruta_ledger.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            "No se pudo crear directorio para --ledger '{}': {}",
                            parent.display(),
                            e
                        )
                    })?;
                }
            }
            let ledger_json = serde_json::to_string_pretty(&ledger)
                .map_err(|e| format!("Error serializando --ledger: {}", e))?;
            fs::write(&opciones.ruta_ledger, format!("{}\n", ledger_json)).map_err(|e| {
                format!(
                    "No se pudo escribir --ledger '{}': {}",
                    opciones.ruta_ledger.display(),
                    e
                )
            })?;
        }
    }

    if let Some(ruta_rep) = &opciones.ruta_reporte {
        if let Some(parent) = ruta_rep.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "No se pudo crear directorio para --reporte '{}': {}",
                        parent.display(),
                        e
                    )
                })?;
            }
        }
        fs::write(ruta_rep, &reporte_markdown).map_err(|e| {
            format!(
                "No se pudo escribir --reporte '{}': {}",
                ruta_rep.display(),
                e
            )
        })?;
    }

    let texto_stdout = match opciones.formato_salida {
        FormatoSalidaIngesta::Json => serde_json::to_string_pretty(&resumen)
            .map_err(|e| format!("Error serializando salida JSON: {}", e))?,
        FormatoSalidaIngesta::Text => reporte_markdown.clone(),
    };

    Ok(SalidaCompletaIngesta {
        resumen,
        reporte_markdown,
        texto_stdout,
    })
}

pub fn texto_ayuda_cli() -> &'static str {
    "lokus_ingestar — Herramienta de ingesta idempotente de autoaprendizaje IA y catálogos de Lokus

USO:
  cargo run --manifest-path src-tauri/Cargo.toml --bin lokus_ingestar -- [OPCIONES] <INPUT_JSON...>

OPCIONES:
  --base <PATH>              Ruta a aprendizaje_base.json (por defecto: src-tauri/resources/aprendizaje_base.json)
  --ledger <PATH>            Ruta al ledger persistente del mantenedor (por defecto: src-tauri/resources/ingesta_ledger.json)
  --catalogos-base <PATH>    Ruta a juzgados_base.json (por defecto: src-tauri/resources/juzgados_base.json)
  --reporte <PATH>           Ruta de salida opcional para el reporte de curación en Markdown
  --fuente <ALIAS>           Identificador explícito de fuente para paquetes formato: 4 sin instancia_id
  --dry-run                  Calcula estadísticas y reporte sin modificar --base ni --ledger
  --formato-salida <FMT>     Formato de resumen en stdout: text | json (por defecto: text)
  -h, --help                 Muestra esta ayuda y termina con código 0
"
}

pub fn parsear_argumentos_cli<I>(args: I) -> Result<Result<OpcionesIngesta, String>, String>
where
    I: IntoIterator<Item = String>,
{
    let mut opciones = OpcionesIngesta::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                return Ok(Err(texto_ayuda_cli().to_string()));
            }
            "--base" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --base <PATH>".to_string())?;
                opciones.ruta_base = PathBuf::from(val);
            }
            "--ledger" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --ledger <PATH>".to_string())?;
                opciones.ruta_ledger = PathBuf::from(val);
            }
            "--catalogos-base" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --catalogos-base <PATH>".to_string())?;
                opciones.ruta_catalogos_base = PathBuf::from(val);
            }
            "--reporte" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --reporte <PATH>".to_string())?;
                opciones.ruta_reporte = Some(PathBuf::from(val));
            }
            "--fuente" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --fuente <ALIAS>".to_string())?;
                if val.trim().is_empty() {
                    return Err("El valor de --fuente no puede estar vacío".to_string());
                }
                opciones.fuente_manual = Some(val);
            }
            "--dry-run" => {
                opciones.dry_run = true;
            }
            "--formato-salida" => {
                let val = iter
                    .next()
                    .ok_or_else(|| "Falta el valor para --formato-salida <text|json>".to_string())?;
                opciones.formato_salida = FormatoSalidaIngesta::desde_str(&val)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!(
                    "Opción no reconocida: '{}'\n\n{}",
                    flag,
                    texto_ayuda_cli()
                ));
            }
            ruta => {
                opciones.archivos_entrada.push(PathBuf::from(ruta));
            }
        }
    }

    if opciones.archivos_entrada.is_empty() {
        return Err(format!(
            "No se especificaron archivos JSON de entrada.\n\n{}",
            texto_ayuda_cli()
        ));
    }

    Ok(Ok(opciones))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn crear_dir_temp(sufijo: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lokus_ingesta_test_{}_{}", sufijo, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_sha256_vector_estandar_y_cuarentena_validadores() {
        // FIPS 180-4 vector estándar "abc"
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(contiene_marcador_anonimizacion(
            "Juzgado 1 Promiscuo Municipal de San [PROCESADO]"
        ));
        assert!(es_entrada_catalogo_invalida("fdsafeasdfe", "juzgado").is_some());
        assert!(es_entrada_catalogo_invalida("Juzgado 10 Penal Maunicipal", "juzgado").is_some());
        assert!(es_entrada_catalogo_invalida("Juzgado 9 Penal Municipal", "juzgado").is_none());
        assert!(es_alias_invalido(
            "1 municipal",
            "Juzgado Promiscuo Primero Municipal de Chiriguaná",
            "juzgado"
        )
        .is_some());
        assert!(contradice_reglas_base_invariante(
            "Siempre expandir la abreviación CTO a Circuito en todos los juzgados."
        )
        .is_some());
        assert!(contradice_reglas_base_invariante(
            "Nunca expandir CTO a Circuito en el nombre del juzgado."
        )
        .is_none());
    }

    #[test]
    fn test_ingesta_idempotencia_mes1_mes3_orden_inverso_y_multiusuario_lokus() {
        let dir = crear_dir_temp("idempotencia_m1_m3");
        let base_path = dir.join("aprendizaje_base.json");
        let cat_path = dir.join("juzgados_base.json");
        let ledger_path = dir.join("ingesta_ledger.json");
        let rep_path = dir.join("reporte.md");

        fs::write(
            &base_path,
            include_str!("../resources/aprendizaje_base.json"),
        )
        .unwrap();
        fs::write(&cat_path, include_str!("../resources/juzgados_base.json")).unwrap();

        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let f_m1 = repo_root.join("tests/e2e_ingesta/fixtures/v4_mes1_usuario_a.json");
        let f_m3 = repo_root.join("tests/e2e_ingesta/fixtures/v4_mes3_usuario_a_superset.json");

        // 1) Ingestar Mes 1 -> 20 audiencias
        let op1 = OpcionesIngesta {
            ruta_base: base_path.clone(),
            ruta_ledger: ledger_path.clone(),
            ruta_catalogos_base: cat_path.clone(),
            ruta_reporte: Some(rep_path.clone()),
            fuente_manual: None,
            dry_run: false,
            formato_salida: FormatoSalidaIngesta::Json,
            archivos_entrada: vec![f_m1.clone()],
        };
        let out1 = ejecutar_ingesta(&op1).unwrap();
        assert_eq!(out1.resumen.total_audiencias_acumuladas, 20);
        assert_eq!(out1.resumen.reglas_base_invariante, 3);

        // 2) Re-ingestar Mes 1 idéntico -> no-op idempotente (20 audiencias)
        let out1_dup = ejecutar_ingesta(&op1).unwrap();
        assert_eq!(out1_dup.resumen.estado, "idempotente_sin_cambios");
        assert_eq!(out1_dup.resumen.archivos_omitidos_idempotentes, 1);
        assert_eq!(out1_dup.resumen.total_audiencias_acumuladas, 20);

        // 3) Ingestar Mes 3 -> reemplaza snapshot acumulado a 55 (no 75) y preserva regla/corrección expulsada de Mes 1
        let op3 = OpcionesIngesta {
            archivos_entrada: vec![f_m3.clone()],
            ..op1.clone()
        };
        let out3 = ejecutar_ingesta(&op3).unwrap();
        assert_eq!(out3.resumen.total_audiencias_acumuladas, 55);
        let ledger_str = fs::read_to_string(&ledger_path).unwrap();
        assert!(ledger_str.contains("corr-u-a-m1-evicted"));
        assert!(ledger_str.contains("regla-u-a-m1-evicted"));

        // 4) Re-enviar Mes 1 obsoleto fuera de orden -> mantiene 55 audiencias
        let out_stale = ejecutar_ingesta(&op1).unwrap();
        assert_eq!(out_stale.resumen.total_audiencias_acumuladas, 55);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ingesta_cadena_3_saltos_lokus_y_crecimiento_posterior() {
        let dir = crear_dir_temp("cadena_3_saltos");
        let base_path = dir.join("aprendizaje_base.json");
        let cat_path = dir.join("juzgados_base.json");
        let ledger_path = dir.join("ingesta_ledger.json");

        fs::write(
            &base_path,
            include_str!("../resources/aprendizaje_base.json"),
        )
        .unwrap();
        fs::write(&cat_path, include_str!("../resources/juzgados_base.json")).unwrap();

        let make_pkg = |inst: &str,
                        seq: u64,
                        exportado: &str,
                        importadas: Vec<&str>,
                        total_aud: u32,
                        juzgados: Vec<(&str, u32)>,
                        tipos: Vec<(&str, u32)>| {
            let juz_det: Vec<serde_json::Value> = juzgados
                .iter()
                .map(|(n, c)| {
                    serde_json::json!({
                        "nombre": n,
                        "origen": "predeterminado",
                        "conteo": c,
                        "porcentaje": 0.0
                    })
                })
                .collect();
            let tip_det: Vec<serde_json::Value> = tipos
                .iter()
                .map(|(n, c)| {
                    serde_json::json!({
                        "nombre": n,
                        "origen": "predeterminado",
                        "conteo": c,
                        "porcentaje": 0.0
                    })
                })
                .collect();
            let uso_tipos = serde_json::json!({
                "total_audiencias": total_aud,
                "total_con_valor": total_aud,
                "total_sin_especificar": 0,
                "porcentaje_sin_especificar": 0.0,
                "total_opciones_predeterminadas": 27,
                "total_opciones_lista_usuario": 0,
                "total_opciones_otra": 0,
                "total_opciones_eliminadas_tras_uso": 0,
                "nuevos_no_predeterminados": [],
                "predeterminados_sin_uso": [],
                "agregados_usuario_sin_uso": [],
                "detalle": tip_det
            });
            let uso_juzgados = serde_json::json!({
                "total_audiencias": total_aud,
                "total_con_valor": total_aud,
                "total_sin_especificar": 0,
                "porcentaje_sin_especificar": 0.0,
                "total_opciones_predeterminadas": 115,
                "total_opciones_lista_usuario": 0,
                "total_opciones_otra": 0,
                "total_opciones_eliminadas_tras_uso": 0,
                "nuevos_no_predeterminados": [],
                "predeterminados_sin_uso": [],
                "agregados_usuario_sin_uso": [],
                "detalle": juz_det
            });
            let uso_horas = serde_json::json!({
                "total_audiencias": total_aud,
                "total_con_hora": total_aud,
                "total_sin_hora": 0,
                "porcentaje_sin_hora": 0.0,
                "horas_rapidas_predeterminadas": [],
                "horas_rapidas_sin_uso": [],
                "horas_mas_frecuentes_no_rapidas": [],
                "detalle_por_hora_exacta": [],
                "resumen_por_franja_horaria": []
            });
            let uso_catalogos = serde_json::json!({
                "total_audiencias_contabilizadas": total_aud,
                "sembrado_inicial_completado": true,
                "sembrado_en": "2026-11-01T08:00:00-05:00",
                "ultima_actualizacion": exportado,
                "tipos_audiencia": uso_tipos,
                "juzgados": uso_juzgados,
                "horas": uso_horas
            });
            let mut root = serde_json::Map::new();
            root.insert("app".into(), serde_json::json!("Lokus"));
            root.insert("version_app".into(), serde_json::json!("5.2.0"));
            root.insert("formato".into(), serde_json::json!(5));
            root.insert("exportado".into(), serde_json::json!(exportado));
            root.insert("instancia_id".into(), serde_json::json!(inst));
            root.insert("exportacion_seq".into(), serde_json::json!(seq));
            root.insert("fuentes_importadas".into(), serde_json::json!(importadas));
            root.insert("aciertos".into(), serde_json::json!(total_aud));
            root.insert("total_reglas".into(), serde_json::json!(0));
            root.insert("total_correcciones".into(), serde_json::json!(0));
            root.insert("total_ejemplos_aciertos".into(), serde_json::json!(0));
            root.insert("perfiles_defensores".into(), serde_json::json!(1));
            root.insert("llamadas_induccion_llm_total".into(), serde_json::json!(0));
            root.insert("llamadas_extraccion_total".into(), serde_json::json!(total_aud));
            root.insert("llamadas_consolidacion_total".into(), serde_json::json!(0));
            root.insert("correcciones_por_campo".into(), serde_json::json!({}));
            root.insert("metricas_por_campo".into(), serde_json::json!([]));
            root.insert("metricas_por_modelo".into(), serde_json::json!([]));
            root.insert(
                "preferencias_estilo".into(),
                serde_json::json!({
                    "incluir_causas_secundarias_prob": 0.5,
                    "incluir_reprogramacion_prob": 0.5,
                    "se_fija_aplazamiento_prob": 0.5,
                    "instalada_reprogramada_si_prob": 0.5
                }),
            );
            root.insert("preferencias_por_defensor".into(), serde_json::json!({}));
            root.insert("defensores_directorio".into(), serde_json::json!([]));
            root.insert(
                "resumen_patrones_generales".into(),
                serde_json::json!({
                    "total_defensores_activos": 1,
                    "top_juzgados_globales": [],
                    "top_tipos_globales": [],
                    "top_horas_globales": [],
                    "reglas_transversales_promovidas": 0,
                    "errores_recurrentes_transversales": []
                }),
            );
            root.insert("grafo_asociaciones".into(), serde_json::json!({}));
            root.insert("perfiles_sinteticos".into(), serde_json::json!([]));
            root.insert("consolidaciones_ia".into(), serde_json::json!([]));
            root.insert("alias_validados".into(), serde_json::json!([]));
            root.insert("patrones_abreviacion".into(), serde_json::json!([]));
            root.insert("juzgados_agregados_usuario".into(), serde_json::json!([]));
            root.insert("tipos_audiencia_agregados_usuario".into(), serde_json::json!([]));
            root.insert("reglas".into(), serde_json::json!([]));
            root.insert("correcciones".into(), serde_json::json!([]));
            root.insert("uso_catalogos".into(), uso_catalogos);
            serde_json::Value::Object(root)
        };

        let p_a1 = dir.join("a1.json");
        let p_b = dir.join("b.json");
        let p_c = dir.join("c.json");
        let p_a2 = dir.join("a2.json");

        // A inicial: 30
        fs::write(
            &p_a1,
            serde_json::to_string_pretty(&make_pkg(
                "inst-a",
                1,
                "2026-11-01T10:00:00-05:00",
                vec![],
                30,
                vec![("Juzgado 9 Penal Municipal", 30)],
                vec![("Audiencia concentrada", 30)],
            ))
            .unwrap(),
        )
        .unwrap();

        // B importa A (30) y agrega +10 = 40
        fs::write(
            &p_b,
            serde_json::to_string_pretty(&make_pkg(
                "inst-b",
                1,
                "2026-11-05T10:00:00-05:00",
                vec!["inst-a"],
                40,
                vec![
                    ("Juzgado 9 Penal Municipal", 30),
                    ("Juzgado 11 Penal Municipal", 10),
                ],
                vec![
                    ("Audiencia concentrada", 30),
                    ("Audiencia de juicio oral", 10),
                ],
            ))
            .unwrap(),
        )
        .unwrap();

        // C importa B (y transitivamente a A) y agrega +5 = 45
        fs::write(
            &p_c,
            serde_json::to_string_pretty(&make_pkg(
                "inst-c",
                1,
                "2026-11-10T10:00:00-05:00",
                vec!["inst-a", "inst-b"],
                45,
                vec![
                    ("Juzgado 9 Penal Municipal", 30),
                    ("Juzgado 11 Penal Municipal", 10),
                    ("Juzgado 5 Penal Municipal", 5),
                ],
                vec![
                    ("Audiencia concentrada", 30),
                    ("Audiencia de juicio oral", 10),
                    ("Audiencia preliminar", 5),
                ],
            ))
            .unwrap(),
        )
        .unwrap();

        // A sigue trabajando después y crece de 30 a 50
        fs::write(
            &p_a2,
            serde_json::to_string_pretty(&make_pkg(
                "inst-a",
                2,
                "2026-11-20T10:00:00-05:00",
                vec![],
                50,
                vec![
                    ("Juzgado 9 Penal Municipal", 30),
                    ("Juzgado 8 Penal Municipal", 20),
                ],
                vec![("Audiencia concentrada", 50)],
            ))
            .unwrap(),
        )
        .unwrap();

        let op = OpcionesIngesta {
            ruta_base: base_path,
            ruta_ledger: ledger_path.clone(),
            ruta_catalogos_base: cat_path,
            ruta_reporte: None,
            fuente_manual: None,
            dry_run: false,
            formato_salida: FormatoSalidaIngesta::Json,
            archivos_entrada: vec![p_a1, p_b, p_c, p_a2],
        };

        let out = ejecutar_ingesta(&op).unwrap();
        // A=50 netas, B=10 netas, C=5 netas -> total acumulado = 65 (sin doble deducción de A en C)
        assert_eq!(out.resumen.total_audiencias_acumuladas, 65);
        let ledger: LedgerIngesta =
            serde_json::from_str(&fs::read_to_string(&ledger_path).unwrap()).unwrap();
        assert_eq!(ledger.fuentes["inst-a"].audiencias_netas, 50);
        assert_eq!(ledger.fuentes["inst-b"].audiencias_netas, 10);
        assert_eq!(ledger.fuentes["inst-c"].audiencias_netas, 5);
        assert!(ledger
            .advertencias_solapamiento
            .iter()
            .any(|a| a.contains("overlap_con_crecimiento_posterior")));

        let _ = fs::remove_dir_all(&dir);
    }
}
