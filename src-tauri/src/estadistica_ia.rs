//! Motor estadístico local en Rust puro para Lokus (Fase 2).
//!
//! Combina:
//! 1. Regresión Logística Online con regularización L2 + AdaGrad para `realizada` (binario)
//!    y `motivo` (8 clases), entrenada incrementalmente sobre el búfer balanceado
//!    (`ejemplos_aciertos + correcciones`).
//! 2. Complement Naive Bayes (para comparativa offline de calibración y Macro-F1).
//! 3. BM25 k-NN ponderado por similitud sobre casos históricos.
//! 4. Resolutor de entidades (`juzgado`, `tipo`, `radicado`) con normalización de ordinales,
//!    tabla estadística de alias Beta(alpha, beta) y priors incrementales de las planillas `.xlsx`.
//! 5. Prior empírico de jornada laboral judicial (07:00–18:00) para desambiguación AM/PM.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

use regex::Regex;

use crate::aprendizaje::{
    aplicar_decaimiento_beta_con_prior_en, calcular_bm25, colapsar_espacios,
    limite_inferior_bayesiano, ngramas_caracteres, normalizar_ess_par, quitar_tildes,
    tokenizar_judicial, AliasEstadistico, CampoCorregido, MemoriaAprendizaje,
    PrediccionLocalResumen, PreferenciasEstilo, MAX_ESS_N, PRIOR_ALIAS_ALPHA, PRIOR_ALIAS_BETA,
};
use crate::config::AppSettings;
use crate::models::{
    coincide_alcance_defensor, es_alcance_general, extraer_numeros_radicado,
    extraer_prefijo_dane_radicado, extraer_siglas_cortas_juzgado, mapear_motivo_a_indice,
    normalizar_clave_catalogo, normalizar_clave_defensor, normalizar_texto_sin_tildes,
    texto_indica_instalacion_efectiva, AudienciaInput, IndicePriorsExcel, ProcesoHabitual,
    MOTIVOS_LABELS, TIPOS_AUDIENCIA,
};
use crate::patrones_abreviacion::{
    canonizar_entrada_alias_general, clave_estructural_alias, es_calificador_juzgado,
    es_calificador_terminal_juzgado, es_clave_alias_valida,
    extraer_abreviacion_numerica_unica_para_juzgado, resolver_abreviaciones_y_patrones_en_texto,
    sigla_desnuda_sin_calificador_en_texto, ResolucionPatronInferido,
};

const JUZGADOS_BASE_JSON: &str = include_str!("../resources/juzgados_base.json");
const MAX_ALIAS: usize = 180;

pub fn juzgados_base() -> &'static Vec<String> {
    static BASE: OnceLock<Vec<String>> = OnceLock::new();
    BASE.get_or_init(|| serde_json::from_str(JUZGADOS_BASE_JSON).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Extracción de features judiciales y normalización de entidades
// ---------------------------------------------------------------------------

pub fn normalizar_clave_entidad(texto: &str) -> String {
    let tokens = tokenizar_judicial(texto);
    tokens
        .into_iter()
        .filter(|t| t != "_rad_")
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn extraer_features_texto(texto: &str) -> Vec<(String, f64)> {
    let lower = quitar_tildes(&texto.to_lowercase());
    let tokens = tokenizar_judicial(texto);
    let mut mapa: BTreeMap<String, f64> = BTreeMap::new();

    for tok in &tokens {
        *mapa.entry(format!("u:{}", tok)).or_insert(0.0) += 1.0;
    }
    for par in tokens.windows(2) {
        *mapa.entry(format!("b:{}_{}", par[0], par[1])).or_insert(0.0) += 1.25;
    }
    for gram in ngramas_caracteres(texto, 4) {
        *mapa.entry(format!("c4:{}", gram)).or_insert(0.0) += 0.45;
    }

    // Indicadores estructurales de dominio judicial colombiano
    let indicadores: [(&str, bool); 9] = [
        (
            "ind:dice_si",
            lower.contains("se realizo: si")
                || lower.contains("se realizo?: si")
                || lower.contains("sin novedad")
                || lower.contains("se efectuo")
                || lower.contains("sentido de fallo")
                || lower.contains("se dio lectura"),
        ),
        (
            "ind:fracaso",
            lower.contains("fracas")
                || lower.contains("fracsa")
                || lower.contains("frcasa")
                || lower.contains("reprogram")
                || lower.contains("aplaz")
                || lower.contains("suspend")
                || lower.contains("no se realizo")
                || lower.contains("no se hizo"),
        ),
        (
            "ind:se_fija_nueva",
            lower.contains("se fija nueva fecha") || lower.contains("se fijo nueva fecha"),
        ),
        (
            "ind:se_fija_continuar",
            lower.contains("para continuar")
                || lower.contains("lectura")
                || lower.contains("se realizo y se fija"),
        ),
        (
            "ind:primera_persona",
            lower.contains("solicite")
                || lower.contains("pedi")
                || lower.contains("no pude")
                || lower.contains("me fue imposible")
                || lower.contains("no logre")
                || lower.contains("no he tenido contacto"),
        ),
        (
            "ind:def_confianza",
            lower.contains("contractual")
                || lower.contains("privado")
                || lower.contains("defianza")
                || lower.contains("def confianza")
                || lower.contains("def. confianza")
                || lower.contains("defensor de confianza")
                || lower.contains("abogado del procesado"),
        ),
        (
            "ind:fiscalia",
            lower.contains("fiscal") || lower.contains("preacuerdo"),
        ),
        (
            "ind:juez",
            lower.contains("juez")
                || lower.contains("despacho")
                || lower.contains("depacho")
                || lower.contains("vacaciones")
                || lower.contains("agenda")
                || lower.contains("titular"),
        ),
        ("ind:inpec", lower.contains("inpec") || lower.contains("traslad")),
    ];

    for (clave, activo) in indicadores {
        if activo {
            *mapa.entry(clave.to_string()).or_insert(0.0) += 1.6;
        }
    }

    let norma = mapa.values().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
    mapa.into_iter()
        .map(|(k, v)| (k, v / norma))
        .collect()
}

// ---------------------------------------------------------------------------
// Clasificador 1: Regresión Logística Online con L2 + AdaGrad
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClasificadorBinarioLogRegL2 {
    pub pesos: HashMap<String, f64>,
    pub bias: f64,
    pub grad_sq: HashMap<String, f64>,
    pub bias_grad_sq: f64,
    pub lr: f64,
    pub l2: f64,
    pub muestras: usize,
}

impl Default for ClasificadorBinarioLogRegL2 {
    fn default() -> Self {
        Self {
            pesos: HashMap::new(),
            bias: 0.0,
            grad_sq: HashMap::new(),
            bias_grad_sq: 1.0,
            lr: 0.45,
            l2: 0.002,
            muestras: 0,
        }
    }
}

fn sigmoide(z: f64) -> f64 {
    let clamped = z.clamp(-25.0, 25.0);
    1.0 / (1.0 + (-clamped).exp())
}

impl ClasificadorBinarioLogRegL2 {
    pub fn predecir_prob(&self, feats: &[(String, f64)]) -> f64 {
        let mut z = self.bias;
        for (k, v) in feats {
            if let Some(&w) = self.pesos.get(k) {
                z += w * v;
            }
        }
        sigmoide(z)
    }

    pub fn actualizar(&mut self, feats: &[(String, f64)], positivo: bool, peso_muestra: f64) {
        let y = if positivo { 1.0 } else { 0.0 };
        let p = self.predecir_prob(feats);
        let err = (p - y) * peso_muestra;

        self.bias_grad_sq += err * err;
        let eta_b = self.lr / (self.bias_grad_sq + 1e-8).sqrt();
        self.bias -= eta_b * err;

        for (k, v) in feats {
            let w_prev = *self.pesos.get(k).unwrap_or(&0.0);
            let grad = err * v + self.l2 * w_prev;
            let gsq = self.grad_sq.entry(k.clone()).or_insert(1.0);
            *gsq += grad * grad;
            let eta = self.lr / (*gsq + 1e-8).sqrt();
            self.pesos.insert(k.clone(), w_prev - eta * grad);
        }
        self.muestras += 1;
    }
}

#[derive(Debug, Clone)]
pub struct ClasificadorMotivoLogRegL2 {
    pub clases: [HashMap<String, f64>; 8],
    pub biases: [f64; 8],
    pub grad_sq: [HashMap<String, f64>; 8],
    pub bias_grad_sq: [f64; 8],
    pub lr: f64,
    pub l2: f64,
    pub muestras: usize,
}

impl Default for ClasificadorMotivoLogRegL2 {
    fn default() -> Self {
        Self {
            clases: std::array::from_fn(|_| HashMap::new()),
            biases: [0.0; 8],
            grad_sq: std::array::from_fn(|_| HashMap::new()),
            bias_grad_sq: [1.0; 8],
            lr: 0.55,
            l2: 0.0015,
            muestras: 0,
        }
    }
}

impl ClasificadorMotivoLogRegL2 {
    pub fn predecir_probs(&self, feats: &[(String, f64)]) -> [f64; 8] {
        let mut logits = self.biases;
        for c in 0..8 {
            for (k, v) in feats {
                if let Some(&w) = self.clases[c].get(k) {
                    logits[c] += w * v;
                }
            }
        }
        let max_l = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut exps = [0.0_f64; 8];
        let mut sum = 0.0_f64;
        for c in 0..8 {
            exps[c] = (logits[c] - max_l).clamp(-30.0, 30.0).exp();
            sum += exps[c];
        }
        if sum <= 0.0 {
            return [0.125; 8];
        }
        for e in &mut exps {
            *e /= sum;
        }
        exps
    }

    pub fn actualizar(&mut self, feats: &[(String, f64)], clase_idx: usize, peso_muestra: f64) {
        if clase_idx >= 8 {
            return;
        }
        let probs = self.predecir_probs(feats);
        for c in 0..8 {
            let target = if c == clase_idx { 1.0 } else { 0.0 };
            let err = (probs[c] - target) * peso_muestra;
            self.bias_grad_sq[c] += err * err;
            let eta_b = self.lr / (self.bias_grad_sq[c] + 1e-8).sqrt();
            self.biases[c] -= eta_b * err;

            for (k, v) in feats {
                let w_prev = *self.clases[c].get(k).unwrap_or(&0.0);
                let grad = err * v + self.l2 * w_prev;
                let gsq = self.grad_sq[c].entry(k.clone()).or_insert(1.0);
                *gsq += grad * grad;
                let eta = self.lr / (*gsq + 1e-8).sqrt();
                self.clases[c].insert(k.clone(), w_prev - eta * grad);
            }
        }
        self.muestras += 1;
    }
}

// ---------------------------------------------------------------------------
// Complement Naive Bayes (para benchmark comparativo offline)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ComplementNaiveBayes {
    pub num_clases: usize,
    pub vocab: BTreeSet<String>,
    pub conteos_clase: Vec<HashMap<String, f64>>,
    pub total_clase: Vec<f64>,
    pub docs_clase: Vec<f64>,
    pub alpha: f64,
}

impl ComplementNaiveBayes {
    pub fn nuevo(num_clases: usize) -> Self {
        Self {
            num_clases,
            vocab: BTreeSet::new(),
            conteos_clase: vec![HashMap::new(); num_clases],
            total_clase: vec![0.0; num_clases],
            docs_clase: vec![0.0; num_clases],
            alpha: 1.0,
        }
    }

    pub fn observar(&mut self, feats: &[(String, f64)], clase_idx: usize) {
        if clase_idx >= self.num_clases {
            return;
        }
        self.docs_clase[clase_idx] += 1.0;
        for (k, v) in feats {
            let val = v.abs();
            self.vocab.insert(k.clone());
            *self.conteos_clase[clase_idx].entry(k.clone()).or_insert(0.0) += val;
            self.total_clase[clase_idx] += val;
        }
    }

    pub fn predecir_probs(&self, feats: &[(String, f64)]) -> Vec<f64> {
        let v_size = (self.vocab.len() as f64).max(1.0);
        let total_global: f64 = self.total_clase.iter().sum();
        let total_docs: f64 = self.docs_clase.iter().sum::<f64>().max(1.0);

        let mut scores = vec![0.0_f64; self.num_clases];
        for c in 0..self.num_clases {
            let comp_total = (total_global - self.total_clase[c]).max(0.0);
            let prior = ((self.docs_clase[c] + 0.5) / (total_docs + 0.5 * self.num_clases as f64)).ln();
            let mut weights_sum = 0.0_f64;
            let mut raw_w: Vec<(f64, f64)> = Vec::with_capacity(feats.len());
            for (k, val) in feats {
                let count_c = *self.conteos_clase[c].get(k).unwrap_or(&0.0);
                let total_k: f64 = self
                    .conteos_clase
                    .iter()
                    .map(|m| *m.get(k).unwrap_or(&0.0))
                    .sum();
                let comp_k = (total_k - count_c).max(0.0);
                let theta = (comp_k + self.alpha) / (comp_total + self.alpha * v_size);
                let w = -theta.ln();
                weights_sum += w.abs();
                raw_w.push((w, *val));
            }
            let norm = weights_sum.max(1.0);
            let dot: f64 = raw_w.iter().map(|(w, v)| (w / norm) * v).sum();
            scores[c] = prior + dot;
        }

        let max_s = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut exps: Vec<f64> = scores
            .into_iter()
            .map(|s| (s - max_s).clamp(-30.0, 30.0).exp())
            .collect();
        let sum: f64 = exps.iter().sum::<f64>().max(1e-12);
        for e in &mut exps {
            *e /= sum;
        }
        exps
    }
}

// ---------------------------------------------------------------------------
// BM25 k-NN ponderado por similitud
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RegistroHistorico {
    pub texto: String,
    pub tokens: Vec<String>,
    pub salida: AudienciaInput,
}

#[derive(Debug, Clone, Default)]
pub struct PrediccionKnn {
    pub realizada: Option<(String, f64)>,
    pub motivo: Option<(String, f64)>,
    pub tipo: Option<(String, f64)>,
    pub juzgado: Option<(String, f64)>,
}

pub fn predecir_knn_bm25(texto: &str, corpus: &[RegistroHistorico], k: usize) -> PrediccionKnn {
    if corpus.is_empty() || texto.trim().is_empty() {
        return PrediccionKnn::default();
    }
    let query_tokens = tokenizar_judicial(texto);
    let docs: Vec<Vec<String>> = corpus.iter().map(|r| r.tokens.clone()).collect();
    let scores = calcular_bm25(&query_tokens, &docs);

    let mut vecinos: Vec<(usize, f64)> = scores
        .into_iter()
        .enumerate()
        .filter(|(_, s)| *s > 0.25)
        .collect();
    vecinos.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    vecinos.truncate(k);

    if vecinos.is_empty() {
        return PrediccionKnn::default();
    }

    let votar = |extractor: fn(&AudienciaInput) -> String| -> Option<(String, f64)> {
        let mut votos: BTreeMap<String, (f64, String)> = BTreeMap::new();
        let mut total_peso = 0.0;
        for &(idx, peso) in &vecinos {
            let val = extractor(&corpus[idx].salida);
            if val.is_empty() {
                continue;
            }
            let clave = normalizar_texto_sin_tildes(&val);
            total_peso += peso;
            let e = votos.entry(clave).or_insert((0.0, val));
            e.0 += peso;
        }
        if total_peso <= 0.0 {
            return None;
        }
        votos
            .into_values()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(peso, val)| (val, (peso / total_peso).clamp(0.0, 1.0)))
    };

    PrediccionKnn {
        realizada: votar(|a| a.realizada.trim().to_uppercase()),
        motivo: votar(|a| a.motivo.trim().to_string()),
        tipo: votar(|a| a.tipo.trim().to_string()),
        juzgado: votar(|a| a.juzgado.trim().to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tabla estadística de alias, selección de candidatos y vinculación de radicado
// ---------------------------------------------------------------------------

/// Normaliza la clave de entrada de un alias para cualquier tipo de abreviación:
/// colapsa siglas con puntos (`"J.O."` -> `"jo"`, `"J.P.M. 04"` -> `"jpm 4"`), unifica ceros a la
/// izquierda (`"J04"` -> `"j4"`, `"JPM04"` -> `"jpm4"`, `"J04M"` -> `"j4m"`) y preserva letras
/// calificadoras aisladas (`"J4 M"` != `"J4 C"` != `"J4"`).
pub(crate) fn canonizar_entrada_alias(_campo: &str, entrada_raw: &str) -> String {
    canonizar_entrada_alias_general(entrada_raw)
}

#[cfg(test)]
fn actualizar_alias(
    memoria: &mut MemoriaAprendizaje,
    campo: &str,
    entrada_raw: &str,
    canonico_raw: &str,
    positivo: bool,
    ahora: &str,
) {
    actualizar_alias_para_defensor(
        memoria,
        campo,
        entrada_raw,
        canonico_raw,
        positivo,
        ahora,
        None,
    );
}

fn actualizar_alias_para_defensor(
    memoria: &mut MemoriaAprendizaje,
    campo: &str,
    entrada_raw: &str,
    canonico_raw: &str,
    positivo: bool,
    ahora: &str,
    defensor_clave: Option<&str>,
) {
    let entrada_norm = canonizar_entrada_alias(campo, entrada_raw);
    let entrada_segs = clave_estructural_alias(&entrada_norm);
    let canonico = colapsar_espacios(canonico_raw);
    let canonico_norm = normalizar_clave_entidad(&canonico);
    if !es_clave_alias_valida(&entrada_norm) || entrada_segs.is_empty() || canonico.len() < 3 {
        return;
    }

    let def_norm = defensor_clave
        .map(normalizar_clave_defensor)
        .unwrap_or_default();
    let alcance_objetivo = if def_norm.is_empty() {
        "*".to_string()
    } else {
        def_norm.clone()
    };

    let aplica_mismo_ambito = |alcance: &str| -> bool {
        if alcance_objetivo == "*" {
            es_alcance_general(alcance)
        } else {
            coincide_alcance_defensor(alcance, &alcance_objetivo)
        }
    };

    let misma_entrada = |existente: &str| -> bool {
        let ex_norm = canonizar_entrada_alias(campo, existente);
        ex_norm == entrada_norm || clave_estructural_alias(&ex_norm) == entrada_segs
    };

    if positivo {
        for otro in &mut memoria.tabla_alias {
            if otro.campo == campo
                && misma_entrada(&otro.entrada_norm)
                && aplica_mismo_ambito(&otro.defensor_alcance)
                && normalizar_clave_entidad(&otro.canonico) != canonico_norm
            {
                aplicar_decaimiento_beta_con_prior_en(
                    &mut otro.alpha,
                    &mut otro.beta,
                    PRIOR_ALIAS_ALPHA,
                    PRIOR_ALIAS_BETA,
                    &otro.ultima_actualizacion,
                    Some(ahora),
                );
                otro.beta += 1.0;
                normalizar_ess_par(&mut otro.alpha, &mut otro.beta, MAX_ESS_N);
                otro.ultima_actualizacion = ahora.to_string();
            }
        }
    }

    // Prioridad de búsqueda para evitar duplicados, fugas negativas y falsas promociones:
    // 1) Coincidencia exacta en el mismo alcance (propio del defensor o "*" si es planilla general).
    // 2) En positivo con defensor activo: coincidencia ya promovida en Cuaderno General ("*").
    // 3) En positivo con defensor activo: coincidencia ACTIVA (no retractada: alpha > beta y q10 >= 0.22)
    //    de otro defensor para promoverla al Cuaderno General ("*"). Un alias retractado de otro defensor
    //    nunca se reutiliza ni contamina el aprendizaje del defensor activo.
    let idx_objetivo = if positivo {
        let mismo_alcance = memoria.tabla_alias.iter().position(|a| {
            a.campo == campo
                && misma_entrada(&a.entrada_norm)
                && normalizar_clave_entidad(&a.canonico) == canonico_norm
                && aplica_mismo_ambito(&a.defensor_alcance)
        });
        mismo_alcance
            .or_else(|| {
                if def_norm.is_empty() {
                    return None;
                }
                memoria.tabla_alias.iter().position(|a| {
                    a.campo == campo
                        && misma_entrada(&a.entrada_norm)
                        && normalizar_clave_entidad(&a.canonico) == canonico_norm
                        && es_alcance_general(&a.defensor_alcance)
                })
            })
            .or_else(|| {
                if def_norm.is_empty() {
                    return None;
                }
                memoria.tabla_alias.iter().position(|a| {
                    a.campo == campo
                        && misma_entrada(&a.entrada_norm)
                        && normalizar_clave_entidad(&a.canonico) == canonico_norm
                        && !es_alcance_general(&a.defensor_alcance)
                        && !coincide_alcance_defensor(&a.defensor_alcance, &def_norm)
                        && a.alpha > a.beta
                        && limite_inferior_bayesiano(a.alpha, a.beta) >= 0.22
                })
            })
    } else {
        memoria.tabla_alias.iter().position(|a| {
            a.campo == campo
                && misma_entrada(&a.entrada_norm)
                && normalizar_clave_entidad(&a.canonico) == canonico_norm
                && aplica_mismo_ambito(&a.defensor_alcance)
        })
    };

    if let Some(idx) = idx_objetivo {
        let alias = &mut memoria.tabla_alias[idx];
        aplicar_decaimiento_beta_con_prior_en(
            &mut alias.alpha,
            &mut alias.beta,
            PRIOR_ALIAS_ALPHA,
            PRIOR_ALIAS_BETA,
            &alias.ultima_actualizacion,
            Some(ahora),
        );
        if positivo {
            alias.alpha += 1.0;
            alias.canonico = canonico;
            // Si 2+ defensores distintos confirman la misma equivalencia, promover al Cuaderno General ("*")
            if !def_norm.is_empty() {
                if !es_alcance_general(&alias.defensor_alcance) {
                    let alc_norm = normalizar_clave_defensor(&alias.defensor_alcance);
                    if !alc_norm.is_empty() && !alias.defensores_confirmantes.contains(&alc_norm) {
                        alias.defensores_confirmantes.push(alc_norm);
                    }
                }
                if !alias.defensores_confirmantes.contains(&def_norm) {
                    alias.defensores_confirmantes.push(def_norm);
                }
                if alias.defensores_confirmantes.len() >= 2 {
                    alias.defensor_alcance = "*".to_string();
                }
            }
        } else {
            alias.beta += 1.0;
        }
        normalizar_ess_par(&mut alias.alpha, &mut alias.beta, MAX_ESS_N);
        alias.ultima_actualizacion = ahora.to_string();
        return;
    }

    if !positivo {
        return;
    }

    if memoria.tabla_alias.len() >= MAX_ALIAS {
        if let Some((idx_min, _)) = memoria
            .tabla_alias
            .iter()
            .enumerate()
            .min_by_key(|(idx, a)| {
                (
                    (limite_inferior_bayesiano(a.alpha, a.beta) * 10_000.0) as u32,
                    *idx,
                )
            })
        {
            memoria.tabla_alias.remove(idx_min);
        }
    }

    let mut confirmantes = Vec::new();
    if !def_norm.is_empty() {
        confirmantes.push(def_norm);
    }

    memoria.tabla_alias.push(AliasEstadistico {
        campo: campo.to_string(),
        entrada_norm,
        canonico,
        alpha: PRIOR_ALIAS_ALPHA + 1.0,
        beta: PRIOR_ALIAS_BETA,
        defensor_alcance: alcance_objetivo,
        defensores_confirmantes: confirmantes,
        ultima_actualizacion: ahora.to_string(),
    });
}

fn actualizar_preferencias_en_bloque(
    pref: &mut PreferenciasEstilo,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    peso: f64,
) {
    const MAX_ESS_PREF: f64 = 12.0;
    let pred_obs = prediccion.observaciones.to_lowercase();
    let fin_obs = final_usuario.observaciones.to_lowercase();

    if pred_obs.contains("causas secundarias:") {
        if fin_obs.contains("causas secundarias:") {
            pref.incluir_causas_secundarias_alpha += peso;
        } else {
            pref.incluir_causas_secundarias_beta += peso;
        }
        normalizar_ess_par(
            &mut pref.incluir_causas_secundarias_alpha,
            &mut pref.incluir_causas_secundarias_beta,
            MAX_ESS_PREF,
        );
    } else if fin_obs.contains("causas secundarias:") {
        pref.incluir_causas_secundarias_alpha += peso;
        normalizar_ess_par(
            &mut pref.incluir_causas_secundarias_alpha,
            &mut pref.incluir_causas_secundarias_beta,
            MAX_ESS_PREF,
        );
    }

    if pred_obs.contains("reprogramada para:") {
        if fin_obs.contains("reprogramada para:") {
            pref.incluir_reprogramacion_alpha += peso;
        } else {
            pref.incluir_reprogramacion_beta += peso;
        }
        normalizar_ess_par(
            &mut pref.incluir_reprogramacion_alpha,
            &mut pref.incluir_reprogramacion_beta,
            MAX_ESS_PREF,
        );
    } else if fin_obs.contains("reprogramada para:") {
        pref.incluir_reprogramacion_alpha += peso;
        normalizar_ess_par(
            &mut pref.incluir_reprogramacion_alpha,
            &mut pref.incluir_reprogramacion_beta,
            MAX_ESS_PREF,
        );
    }

    let texto_lower = quitar_tildes(&texto.to_lowercase());
    if (texto_lower.contains("se fija") || texto_lower.contains("se fijo"))
        && !texto_lower.contains("nueva fecha")
    {
        if final_usuario.realizada.trim().eq_ignore_ascii_case("NO") {
            pref.se_fija_aplazamiento_alpha += peso;
        } else if final_usuario.realizada.trim().eq_ignore_ascii_case("SI") {
            pref.se_fija_aplazamiento_beta += peso;
        }
        normalizar_ess_par(
            &mut pref.se_fija_aplazamiento_alpha,
            &mut pref.se_fija_aplazamiento_beta,
            MAX_ESS_PREF,
        );
    }

    // Preferencia adaptable: diligencia instalada (sin negación) que luego se suspende/declara fallida/reprograma
    let dice_instalada = texto_indica_instalacion_efectiva(&texto_lower);
    let dice_incidencia = texto_lower.contains("fallida")
        || texto_lower.contains("reprogram")
        || texto_lower.contains("reprgram")
        || texto_lower.contains("suspend")
        || texto_lower.contains("aplaz")
        || texto_lower.contains("apalz")
        || texto_lower.contains("nueva fecha");
    if dice_instalada && dice_incidencia {
        if final_usuario.realizada.trim().eq_ignore_ascii_case("SI") {
            pref.instalada_reprogramada_si_alpha += peso;
        } else if final_usuario.realizada.trim().eq_ignore_ascii_case("NO") {
            pref.instalada_reprogramada_si_beta += peso;
        }
        normalizar_ess_par(
            &mut pref.instalada_reprogramada_si_alpha,
            &mut pref.instalada_reprogramada_si_beta,
            MAX_ESS_PREF,
        );
    }
}

pub fn actualizar_alias_y_preferencias_en_memoria_para_defensor(
    memoria: &mut MemoriaAprendizaje,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    campos_corregidos: &[CampoCorregido],
    ahora: &str,
    defensor_clave: Option<&str>,
) {
    let corregido = |campo: &str| campos_corregidos.iter().any(|c| c.campo == campo);
    let def_norm = defensor_clave
        .map(normalizar_clave_defensor)
        .filter(|s| !s.is_empty());
    let def_ref = def_norm.as_deref();

    // Detectar patrones inferidos vigentes ANTES de mutar memoria para poder confirmar o corregir
    // abreviaciones no numéricas (ej. "JPM Chimichagua", "AUD NULIDAD") cuando el usuario guarda.
    let patrones_previos = resolver_abreviaciones_y_patrones_en_texto(
        texto,
        &MemoriaAprendizaje::default(),
        memoria,
        &AppSettings::default(),
        &IndicePriorsExcel::default(),
        def_ref,
    );

    // 1. Tabla de alias para juzgado y tipo (por defensor + promoción automática a General)
    if !final_usuario.juzgado.trim().is_empty() {
        let clave_pred_juz = if corregido("juzgado") && !prediccion.juzgado.trim().is_empty() {
            Some(clave_estructural_alias(&prediccion.juzgado))
        } else {
            None
        };
        if corregido("juzgado") {
            if !prediccion.juzgado.trim().is_empty() {
                actualizar_alias_para_defensor(
                    memoria,
                    "juzgado",
                    &prediccion.juzgado,
                    &prediccion.juzgado,
                    false,
                    ahora,
                    def_ref,
                );
                actualizar_alias_para_defensor(
                    memoria,
                    "juzgado",
                    &prediccion.juzgado,
                    &final_usuario.juzgado,
                    true,
                    ahora,
                    def_ref,
                );
            }
            actualizar_alias_para_defensor(
                memoria,
                "juzgado",
                &final_usuario.juzgado,
                &final_usuario.juzgado,
                true,
                ahora,
                def_ref,
            );
        } else {
            actualizar_alias_para_defensor(
                memoria,
                "juzgado",
                &final_usuario.juzgado,
                &final_usuario.juzgado,
                true,
                ahora,
                def_ref,
            );
        }

        // Cosecha general: si el texto traía una abreviación numérica inequívoca (J10, JPM10, J10M,
        // EPMS 2, 10 MPAL, J10 CTO, P4, etc.) con el mismo número del juzgado guardado, aprender
        // la asociación directa sin duplicar conteo si `prediccion.juzgado` ya la registró.
        let mut clave_cosechada_juz = None;
        if let Some(abrev_num) =
            extraer_abreviacion_numerica_unica_para_juzgado(texto, &final_usuario.juzgado)
        {
            let segs_abrev = clave_estructural_alias(&abrev_num);
            if clave_pred_juz.as_ref() != Some(&segs_abrev) {
                actualizar_alias_para_defensor(
                    memoria,
                    "juzgado",
                    &abrev_num,
                    &final_usuario.juzgado,
                    true,
                    ahora,
                    def_ref,
                );
            }
            clave_cosechada_juz = Some(segs_abrev);
        }

        // Confirmación o corrección de patrones parametrizados (ej. slot léxico "JPM Chimichagua")
        let inf_juz: Vec<_> = patrones_previos
            .inferidas
            .iter()
            .filter(|p| p.campo == "juzgado")
            .collect();
        if inf_juz.len() == 1 {
            let p = inf_juz[0];
            let segs_inf = clave_estructural_alias(&p.abreviacion_detectada);
            if clave_pred_juz.as_ref() != Some(&segs_inf)
                && clave_cosechada_juz.as_ref() != Some(&segs_inf)
                && normalizar_clave_entidad(&p.abreviacion_detectada)
                    != normalizar_clave_entidad(&final_usuario.juzgado)
            {
                actualizar_alias_para_defensor(
                    memoria,
                    "juzgado",
                    &p.abreviacion_detectada,
                    &final_usuario.juzgado,
                    true,
                    ahora,
                    def_ref,
                );
            }
        }
    }

    if !final_usuario.tipo.trim().is_empty() && final_usuario.tipo != "Otra" {
        let clave_pred_tipo = if corregido("tipo") && !prediccion.tipo.trim().is_empty() {
            Some(clave_estructural_alias(&prediccion.tipo))
        } else {
            None
        };
        if corregido("tipo") {
            if !prediccion.tipo.trim().is_empty() {
                actualizar_alias_para_defensor(
                    memoria,
                    "tipo",
                    &prediccion.tipo,
                    &prediccion.tipo,
                    false,
                    ahora,
                    def_ref,
                );
                actualizar_alias_para_defensor(
                    memoria,
                    "tipo",
                    &prediccion.tipo,
                    &final_usuario.tipo,
                    true,
                    ahora,
                    def_ref,
                );
            }
            actualizar_alias_para_defensor(
                memoria,
                "tipo",
                &final_usuario.tipo,
                &final_usuario.tipo,
                true,
                ahora,
                def_ref,
            );
        } else {
            actualizar_alias_para_defensor(
                memoria,
                "tipo",
                &final_usuario.tipo,
                &final_usuario.tipo,
                true,
                ahora,
                def_ref,
            );
        }

        let inf_tipo: Vec<_> = patrones_previos
            .inferidas
            .iter()
            .filter(|p| p.campo == "tipo")
            .collect();
        if inf_tipo.len() == 1 {
            let p = inf_tipo[0];
            let segs_inf = clave_estructural_alias(&p.abreviacion_detectada);
            if clave_pred_tipo.as_ref() != Some(&segs_inf)
                && normalizar_clave_entidad(&p.abreviacion_detectada)
                    != normalizar_clave_entidad(&final_usuario.tipo)
            {
                actualizar_alias_para_defensor(
                    memoria,
                    "tipo",
                    &p.abreviacion_detectada,
                    &final_usuario.tipo,
                    true,
                    ahora,
                    def_ref,
                );
            }
        }
    }

    // 2. Preferencias estilísticas adaptables (cuaderno del defensor + aporte suavizado al Cuaderno General)
    if let Some(ref dk) = def_norm {
        let base_global = memoria.preferencias_estilo.clone();
        let entry = memoria
            .preferencias_por_defensor
            .entry(dk.clone())
            .or_insert(base_global);
        actualizar_preferencias_en_bloque(entry, texto, prediccion, final_usuario, 1.0);
        actualizar_preferencias_en_bloque(
            &mut memoria.preferencias_estilo,
            texto,
            prediccion,
            final_usuario,
            0.35,
        );
    } else {
        actualizar_preferencias_en_bloque(
            &mut memoria.preferencias_estilo,
            texto,
            prediccion,
            final_usuario,
            1.0,
        );
    }
}

pub fn actualizar_alias_y_preferencias_en_memoria(
    memoria: &mut MemoriaAprendizaje,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    campos_corregidos: &[CampoCorregido],
    ahora: &str,
) {
    actualizar_alias_y_preferencias_en_memoria_para_defensor(
        memoria,
        texto,
        prediccion,
        final_usuario,
        campos_corregidos,
        ahora,
        None,
    );
}

#[derive(Debug, Clone, Default)]
pub struct CandidatosPrompt {
    pub juzgados_top: Vec<String>,
    pub tipos_top: Vec<String>,
    pub radicado_vinculado: Option<ProcesoHabitual>,
    pub siglas_resueltas: Vec<(String, String)>,
    pub patrones_inferidos: Vec<ResolucionPatronInferido>,
}

fn puntuar_candidato_entidad(query_tokens: &BTreeSet<String>, cand: &str, freq_prior: u32, bonus: f64) -> f64 {
    let cand_tokens: BTreeSet<String> = tokenizar_judicial(cand).into_iter().collect();
    if cand_tokens.is_empty() {
        return 0.0;
    }
    let comunes = query_tokens.intersection(&cand_tokens).count() as f64;
    if comunes == 0.0 && freq_prior == 0 && bonus <= 0.0 {
        return 0.0;
    }
    let jaccard = comunes / (cand_tokens.len().max(1) as f64);
    let prior_log = (1.0 + freq_prior as f64).ln() * 0.12;
    jaccard * 3.0 + comunes * 0.45 + prior_log + bonus
}

fn detectar_tipos_mencionados_en_texto(texto: &str) -> Vec<String> {
    let norm = normalizar_texto_sin_tildes(texto);
    let mut tipos = Vec::new();
    let pistas: &[(&str, &str)] = &[
        ("control de garantias", "Audiencia de control de legalidad"),
        ("control de legalidad", "Audiencia de control de legalidad"),
        ("juicio oral", "Audiencia de juicio oral"),
        ("preparatoria", "Audiencia preparatoria"),
        ("acusacion", "Audiencia de acusación"),
        ("preclusion", "Audiencia de preclusión"),
        ("imputacion", "Audiencia de imputación"),
        ("concentrada", "Audiencia concentrada"),
        ("preliminar", "Audiencia preliminar"),
        ("medida de aseguramiento", "Audiencia de medidas de aseguramiento"),
        ("medidas de aseguramiento", "Audiencia de medidas de aseguramiento"),
        ("alegatos de conclusion", "Alegatos de conclusión"),
        ("conciliacion", "Audiencia de conciliación"),
        ("individualizacion de pena", "Audiencia de individualización de pena"),
        ("reparacion integral", "Audiencia de incidente de reparación integral"),
        ("nulidad", "Audiencia de nulidad"),
        ("prorroga", "Audiencia de prórroga"),
        ("revision de medida", "Audiencia de revisión de medida"),
        ("verificacion de cumplimiento", "Audiencia de verificación de cumplimiento"),
    ];
    for (clave, oficial) in pistas {
        if norm.contains(clave) {
            let k = normalizar_clave_catalogo(oficial);
            if !tipos.contains(&k) {
                tipos.push(k);
            }
        }
    }
    tipos
}

pub fn seleccionar_candidatos_prompt_con_base_para_defensor(
    texto: &str,
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
    defensor: Option<&str>,
) -> CandidatosPrompt {
    let query_tokens: BTreeSet<String> = tokenizar_judicial(texto).into_iter().collect();
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let siglas_texto = extraer_siglas_cortas_juzgado(texto);
    let prefijo_dane = extraer_prefijo_dane_radicado(texto);
    let tipos_detectados = detectar_tipos_mencionados_en_texto(texto);

    // 1. Candidatos de juzgado combinando: Mapa de Conexiones (GrafoAsociaciones) + alias + settings + priors + JUZGADOS_BASE
    let mut mapa_juz: BTreeMap<String, (String, f64)> = BTreeMap::new();
    let sumar_juz = |mapa: &mut BTreeMap<String, (String, f64)>, nombre: &str, extra_score: f64| {
        let limpio = colapsar_espacios(nombre);
        if limpio.is_empty() {
            return;
        }
        let clave = normalizar_clave_entidad(&limpio);
        let freq = *priors.juzgados_freq.get(&limpio).unwrap_or(&0);
        let base_score = puntuar_candidato_entidad(&query_tokens, &limpio, freq, 0.0);
        match mapa.get_mut(&clave) {
            Some(existente) => {
                existente.1 += extra_score;
                if limpio.len() > existente.0.len() {
                    existente.0 = limpio;
                }
            }
            None => {
                let total = base_score + extra_score;
                if total > 0.05 {
                    mapa.insert(clave, (limpio, total));
                }
            }
        }
    };

    // 1a. Alias validados y candidatos recientes (>= 0.22 ya sugiere en prompt; prioriza defensor activo)
    for a in combinar_alias_local_sobre_base(base, local) {
        if a.campo != "juzgado" {
            continue;
        }
        let es_gen = es_alcance_general(&a.defensor_alcance);
        let es_def = !es_gen && coincide_alcance_defensor(&a.defensor_alcance, &def_clave);
        if !es_gen && !es_def {
            continue;
        }
        let lb = limite_inferior_bayesiano(a.alpha, a.beta);
        if lb >= 0.22 {
            let mult_def = if es_def { 1.35 } else { 1.0 };
            sumar_juz(&mut mapa_juz, &a.canonico, lb * 1.8 * mult_def);
        }
    }

    // 1b. Aristas del Mapa de Conexiones ligero (GrafoAsociaciones)
    if !def_clave.is_empty() {
        if let Some(juz_def) = priors.grafo.defensor_a_juzgados.get(&def_clave) {
            for (j, &freq) in juz_def {
                sumar_juz(&mut mapa_juz, j, (1.0 + freq as f64).ln() * 0.55);
            }
        }
        if let Some(siglas_def) = priors.grafo.defensor_sigla_a_juzgado.get(&def_clave) {
            for sigla in &siglas_texto {
                if let Some(destinos) = siglas_def.get(sigla) {
                    for (j, &freq) in destinos {
                        let peso = 1.6 + (1.0 + freq as f64).ln() * 0.85;
                        sumar_juz(&mut mapa_juz, j, peso);
                    }
                }
            }
        }
    }
    if let Some(ref prefijo) = prefijo_dane {
        if let Some(juz_mun) = priors.grafo.prefijo_radicado_a_juzgados.get(prefijo) {
            for (j, &freq) in juz_mun {
                sumar_juz(&mut mapa_juz, j, 0.90 + (1.0 + freq as f64).ln() * 0.50);
            }
        }
    }
    for tipo_k in &tipos_detectados {
        if let Some(juz_tipo) = priors.grafo.tipo_a_juzgados.get(tipo_k) {
            for (j, &freq) in juz_tipo {
                sumar_juz(&mut mapa_juz, j, 0.55 + (1.0 + freq as f64).ln() * 0.35);
            }
        }
    }

    // 1c. Catálogos de usuario, priors globales y JUZGADOS_BASE
    for j in &settings.juzgados {
        sumar_juz(&mut mapa_juz, j, 0.35);
    }
    for j in priors.juzgados_freq.keys() {
        sumar_juz(&mut mapa_juz, j, 0.25);
    }
    for j in juzgados_base() {
        sumar_juz(&mut mapa_juz, j, 0.0);
    }

    // 1d. Resolución general de abreviaciones exactas validadas (juzgado y tipo, sin limitarse a J#)
    //     y patrones parametrizados inferidos ({N} y {X}) con candados de seguridad.
    let res_abrevs = resolver_abreviaciones_y_patrones_en_texto(
        texto, base, local, settings, priors, defensor,
    );

    let mut siglas_resueltas: Vec<(String, String)> = Vec::new();
    let mut claves_siglas_vistas: BTreeSet<String> = BTreeSet::new();

    for (campo_abrev, abrev_norm, canonico) in &res_abrevs.exactas {
        if campo_abrev == "juzgado" {
            sumar_juz(&mut mapa_juz, canonico, 2.2);
        }
        if claves_siglas_vistas.insert(abrev_norm.clone()) {
            siglas_resueltas.push((abrev_norm.clone(), canonico.clone()));
        }
    }

    // Respaldo del Mapa de Conexiones (GrafoAsociaciones) para siglas `j#` presentes en Excel
    // cuando no fueron cubiertas por un alias exacto y aparecen desnudas (sin CTO/Circuito/MPAL/M).
    for sigla in &siglas_texto {
        if claves_siglas_vistas.contains(sigla)
            || !sigla_desnuda_sin_calificador_en_texto(texto, sigla)
        {
            continue;
        }
        if let Some(res_alias) = resolver_alias_exacto_validado_con_base_para_defensor(
            base, local, "juzgado", sigla, 0.30, defensor,
        ) {
            if claves_siglas_vistas.insert(sigla.clone()) {
                siglas_resueltas.push((sigla.clone(), res_alias));
            }
            continue;
        }
        if !def_clave.is_empty() {
            if let Some(destinos) = priors
                .grafo
                .defensor_sigla_a_juzgado
                .get(&def_clave)
                .and_then(|m| m.get(sigla))
            {
                let mut mejor_grafo: Option<(&String, f64, u32)> = None;
                for (j, &freq) in destinos {
                    if freq == 0 {
                        continue;
                    }
                    let clave = normalizar_clave_entidad(&colapsar_espacios(j));
                    let score_comb = mapa_juz.get(&clave).map(|(_, s)| *s).unwrap_or(0.0);
                    let supera = match mejor_grafo {
                        None => true,
                        Some((_, bs, bf)) => {
                            score_comb > bs + 1e-6 || ((score_comb - bs).abs() <= 1e-6 && freq > bf)
                        }
                    };
                    if supera {
                        mejor_grafo = Some((j, score_comb, freq));
                    }
                }
                if let Some((j_top, _, _)) = mejor_grafo {
                    if claves_siglas_vistas.insert(sigla.clone()) {
                        siglas_resueltas.push((sigla.clone(), j_top.clone()));
                    }
                }
            }
        }
    }

    // Filtrar patrones inferidos cuya abreviación ya fue resuelta de forma exacta (incluyendo grafo)
    let patrones_inferidos: Vec<ResolucionPatronInferido> = res_abrevs
        .inferidas
        .into_iter()
        .filter(|p| {
            let k = canonizar_entrada_alias(&p.campo, &p.abreviacion_detectada);
            !claves_siglas_vistas.contains(&k)
        })
        .collect();

    for p in &patrones_inferidos {
        if p.campo == "juzgado" {
            sumar_juz(&mut mapa_juz, &p.expansion_sugerida, 1.35 + p.confianza_q10 * 0.9);
        }
    }

    let mut juzgados_vec: Vec<(String, f64)> = mapa_juz.into_values().collect();
    juzgados_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let juzgados_top: Vec<String> = juzgados_vec.into_iter().take(5).map(|(n, _)| n).collect();

    // 2. Candidatos de tipo de audiencia (cruzando alias + patrones + defensor_a_tipos del grafo + settings + priors + TIPOS_AUDIENCIA)
    let mut mapa_tipo: BTreeMap<String, (String, f64)> = BTreeMap::new();
    let mut sumar_tipo = |nombre: &str, extra_score: f64| {
        let limpio = colapsar_espacios(nombre);
        if limpio.is_empty() || limpio == "Otra" {
            return;
        }
        let clave = normalizar_clave_entidad(&limpio);
        let freq = *priors.tipos_freq.get(&limpio).unwrap_or(&0);
        let base_score = puntuar_candidato_entidad(&query_tokens, &limpio, freq, 0.0);
        match mapa_tipo.get_mut(&clave) {
            Some(existente) => existente.1 += extra_score,
            None => {
                let total = base_score + extra_score;
                if total > 0.05 {
                    mapa_tipo.insert(clave, (limpio, total));
                }
            }
        }
    };

    for (campo_abrev, _, canonico) in &res_abrevs.exactas {
        if campo_abrev == "tipo" {
            sumar_tipo(canonico, 2.2);
        }
    }
    for p in &patrones_inferidos {
        if p.campo == "tipo" {
            sumar_tipo(&p.expansion_sugerida, 1.35 + p.confianza_q10 * 0.9);
        }
    }
    if !def_clave.is_empty() {
        if let Some(tipos_def) = priors.grafo.defensor_a_tipos.get(&def_clave) {
            for (t, &freq) in tipos_def {
                sumar_tipo(t, (1.0 + freq as f64).ln() * 0.45);
            }
        }
    }
    for t in &settings.tipos_audiencia {
        sumar_tipo(t, 0.35);
    }
    for t in priors.tipos_freq.keys() {
        sumar_tipo(t, 0.25);
    }
    for &t in &TIPOS_AUDIENCIA {
        sumar_tipo(t, 0.0);
    }

    let mut tipos_vec: Vec<(String, f64)> = mapa_tipo.into_values().collect();
    tipos_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let tipos_top: Vec<String> = tipos_vec.into_iter().take(5).map(|(n, _)| n).collect();

    // 3. Vinculación de entidad por número de radicado desde .lokus_priors.json
    let mut radicado_vinculado = None;
    for num in extraer_numeros_radicado(texto) {
        if let Some(proc_hab) = priors.procesos_por_numero.get(&num) {
            radicado_vinculado = Some(proc_hab.clone());
            break;
        }
    }

    CandidatosPrompt {
        juzgados_top,
        tipos_top,
        radicado_vinculado,
        siglas_resueltas,
        patrones_inferidos,
    }
}

pub fn seleccionar_candidatos_prompt_para_defensor(
    texto: &str,
    memoria: &MemoriaAprendizaje,
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
    defensor: Option<&str>,
) -> CandidatosPrompt {
    let base = MemoriaAprendizaje::default();
    seleccionar_candidatos_prompt_con_base_para_defensor(
        texto, &base, memoria, settings, priors, defensor,
    )
}

pub fn seleccionar_candidatos_prompt(
    texto: &str,
    memoria: &MemoriaAprendizaje,
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
) -> CandidatosPrompt {
    seleccionar_candidatos_prompt_para_defensor(texto, memoria, settings, priors, None)
}

fn re_am_explicito() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:(?:\b|\d)\s*am\b|\ba\.\s*m\.?|\bmadrugada\b|\bmanana\b)").ok()
    })
    .as_ref()
}

fn re_pm_explicito() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:(?:\b|\d)\s*pm\b|\bp\.\s*m\.?|\btarde\b|\bnoche\b)").ok()
    })
    .as_ref()
}

/// Resuelve ambigüedades AM/PM usando indicadores explícitos + prior empírico de jornada
/// laboral judicial (07:00–18:00) y frecuencias observadas en `.xlsx`.
pub fn resolver_hora_con_prior(
    hora: &str,
    minuto: &str,
    texto: &str,
    priors: &IndicePriorsExcel,
) -> (String, String, bool) {
    let h_trim = hora.trim();
    let m_trim = minuto.trim();
    if h_trim.is_empty() {
        return (String::new(), String::new(), false);
    }
    let Ok(h) = h_trim.parse::<u32>() else {
        return (h_trim.to_string(), m_trim.to_string(), false);
    };
    if h > 23 {
        return (h_trim.to_string(), m_trim.to_string(), false);
    }

    let lower = quitar_tildes(&texto.to_lowercase());
    let tiene_am_explicito = re_am_explicito().is_some_and(|re| re.is_match(&lower));
    let tiene_pm_explicito = re_pm_explicito().is_some_and(|re| re.is_match(&lower));

    let mut hora_resuelta = h;
    let mut modificada = false;

    if (1..=6).contains(&h) && !tiene_am_explicito {
        let freq_am = *priors.horas_freq.get(&h).unwrap_or(&0);
        let freq_pm = *priors.horas_freq.get(&(h + 12)).unwrap_or(&0);
        // En jornada laboral judicial colombiana (07:00–18:00), 01..06 sin "AM" explícito
        // corresponde a la tarde (13..18), salvo que el histórico muestre lo contrario.
        if tiene_pm_explicito || freq_pm >= freq_am {
            hora_resuelta = h + 12;
            modificada = true;
        }
    } else if (7..=11).contains(&h) && tiene_pm_explicito && !tiene_am_explicito {
        hora_resuelta = h + 12;
        modificada = true;
    } else if (13..=23).contains(&h) && tiene_am_explicito && !tiene_pm_explicito {
        hora_resuelta = h - 12;
        modificada = true;
    }

    let m_out = if m_trim.is_empty() {
        "00".to_string()
    } else {
        m_trim.to_string()
    };
    (format!("{:02}", hora_resuelta), m_out, modificada)
}

// ---------------------------------------------------------------------------
// Motor Estadístico Híbrido Unificado
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MotorEstadisticoLocal {
    pub clf_realizada: ClasificadorBinarioLogRegL2,
    pub clf_motivo: ClasificadorMotivoLogRegL2,
    pub corpus_knn: Vec<RegistroHistorico>,
    pub muestras_entrenamiento: usize,
    pub conteo_realizada: [usize; 2],
    pub conteo_motivo: [usize; 8],
}

impl MotorEstadisticoLocal {
    pub fn entrenar_desde_memorias(
        base: &MemoriaAprendizaje,
        local: &MemoriaAprendizaje,
    ) -> Self {
        Self::entrenar_desde_memorias_para_defensor(base, local, None)
    }

    pub fn entrenar_desde_memorias_para_defensor(
        base: &MemoriaAprendizaje,
        local: &MemoriaAprendizaje,
        defensor: Option<&str>,
    ) -> Self {
        let mut clf_realizada = ClasificadorBinarioLogRegL2::default();
        let mut clf_motivo = ClasificadorMotivoLogRegL2::default();
        let mut corpus_knn: Vec<RegistroHistorico> = Vec::new();
        let mut conteo_realizada = [0usize; 2];
        let mut conteo_motivo = [0usize; 8];
        let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();

        // Reunir observaciones balanceadas aisladas por defensor (base + Cuaderno General + defensor activo)
        let mut muestras_realizada: Vec<(Vec<(String, f64)>, bool, f64)> = Vec::new();
        let mut muestras_motivo: Vec<(Vec<(String, f64)>, usize, f64)> = Vec::new();

        let local_ac_ids: BTreeSet<&str> = local
            .ejemplos_aciertos
            .iter()
            .filter(|e| !e.id.trim().is_empty())
            .map(|e| e.id.as_str())
            .collect();
        for ac in local.ejemplos_aciertos.iter().chain(
            base.ejemplos_aciertos
                .iter()
                .filter(|e| e.id.trim().is_empty() || !local_ac_ids.contains(e.id.as_str())),
        ) {
            if ac.texto_anonimizado.trim().is_empty() {
                continue;
            }
            if !es_alcance_general(&ac.defensor_clave)
                && !coincide_alcance_defensor(&ac.defensor_clave, &def_clave)
            {
                continue;
            }
            let peso_def = if coincide_alcance_defensor(&ac.defensor_clave, &def_clave) {
                1.2
            } else {
                1.0
            };
            let feats = extraer_features_texto(&ac.texto_anonimizado);
            let real_up = ac.salida_anonimizada.realizada.trim().to_uppercase();
            if real_up == "SI" || real_up == "NO" {
                let es_si = real_up == "SI";
                conteo_realizada[if es_si { 0 } else { 1 }] += 1;
                muestras_realizada.push((feats.clone(), es_si, peso_def));
                if !es_si {
                    if let Some(idx) = mapear_motivo_a_indice(&ac.salida_anonimizada.motivo) {
                        conteo_motivo[idx] += 1;
                        muestras_motivo.push((feats, idx, peso_def));
                    }
                }
            }
            corpus_knn.push(RegistroHistorico {
                texto: ac.texto_anonimizado.clone(),
                tokens: tokenizar_judicial(&ac.texto_anonimizado),
                salida: ac.salida_anonimizada.clone(),
            });
        }

        let local_corr_ids: BTreeSet<&str> = local
            .correcciones
            .iter()
            .filter(|c| !c.id.trim().is_empty())
            .map(|c| c.id.as_str())
            .collect();
        for c in local.correcciones.iter().chain(
            base.correcciones
                .iter()
                .filter(|c| c.id.trim().is_empty() || !local_corr_ids.contains(c.id.as_str())),
        ) {
            if c.estado == "descartada" || c.texto_original.trim().is_empty() {
                continue;
            }
            if !es_alcance_general(&c.defensor_clave)
                && !coincide_alcance_defensor(&c.defensor_clave, &def_clave)
            {
                continue;
            }
            let peso_def = if coincide_alcance_defensor(&c.defensor_clave, &def_clave) {
                1.3
            } else {
                1.1
            };
            let feats = extraer_features_texto(&c.texto_original);
            let real_up = c.final_usuario.realizada.trim().to_uppercase();
            if real_up == "SI" || real_up == "NO" {
                let es_si = real_up == "SI";
                conteo_realizada[if es_si { 0 } else { 1 }] += 1;
                muestras_realizada.push((feats.clone(), es_si, peso_def));
                if !es_si {
                    if let Some(idx) = mapear_motivo_a_indice(&c.final_usuario.motivo) {
                        conteo_motivo[idx] += 1;
                        muestras_motivo.push((feats, idx, peso_def));
                    }
                }
            }
            corpus_knn.push(RegistroHistorico {
                texto: c.texto_original.clone(),
                tokens: tokenizar_judicial(&c.texto_original),
                salida: c.final_usuario.clone(),
            });
        }

        // 4 pasadas deterministas sobre el búfer balanceado (< 1.5 ms para <= 500 casos)
        for _ in 0..4 {
            for (feats, es_si, peso) in &muestras_realizada {
                clf_realizada.actualizar(feats, *es_si, *peso);
            }
            for (feats, idx, peso) in &muestras_motivo {
                clf_motivo.actualizar(feats, *idx, *peso);
            }
        }

        Self {
            clf_realizada,
            clf_motivo,
            muestras_entrenamiento: muestras_realizada.len(),
            corpus_knn,
            conteo_realizada,
            conteo_motivo,
        }
    }

    pub fn predecir(
        &self,
        texto: &str,
        memoria: &MemoriaAprendizaje,
        priors: &IndicePriorsExcel,
    ) -> PrediccionLocalResumen {
        self.predecir_para_defensor(texto, memoria, priors, None)
    }

    pub fn predecir_para_defensor(
        &self,
        texto: &str,
        memoria: &MemoriaAprendizaje,
        priors: &IndicePriorsExcel,
        defensor: Option<&str>,
    ) -> PrediccionLocalResumen {
        let base = MemoriaAprendizaje::default();
        self.predecir_con_base_para_defensor(texto, &base, memoria, priors, defensor)
    }

    pub fn predecir_con_base_para_defensor(
        &self,
        texto: &str,
        base: &MemoriaAprendizaje,
        local: &MemoriaAprendizaje,
        priors: &IndicePriorsExcel,
        defensor: Option<&str>,
    ) -> PrediccionLocalResumen {
        let feats = extraer_features_texto(texto);
        let knn = predecir_knn_bm25(texto, &self.corpus_knn, 5);

        let realizadas_clases_vistas = self.conteo_realizada.iter().filter(|&&c| c > 0).count();
        let motivos_clases_vistas = self.conteo_motivo.iter().filter(|&&c| c > 0).count();

        let (realizada, realizada_confianza, knn_realizada_acuerdo) =
            if self.muestras_entrenamiento >= 8 {
                let p_si = self.clf_realizada.predecir_prob(&feats);
                let (lbl_lr, conf_lr) = if p_si >= 0.5 {
                    ("SI".to_string(), p_si)
                } else {
                    ("NO".to_string(), 1.0 - p_si)
                };
                if let Some((lbl_knn, conf_knn)) = &knn.realizada {
                    if *lbl_knn == lbl_lr {
                        (
                            Some(lbl_lr),
                            (0.7 * conf_lr + 0.3 * conf_knn).min(0.99),
                            *conf_knn >= 0.78,
                        )
                    } else if *conf_knn >= 0.85 && conf_lr < 0.70 {
                        (Some(lbl_knn.clone()), *conf_knn * 0.85, false)
                    } else {
                        (Some(lbl_lr), conf_lr, false)
                    }
                } else {
                    (Some(lbl_lr), conf_lr, false)
                }
            } else if let Some((lbl_knn, conf_knn)) = &knn.realizada {
                (Some(lbl_knn.clone()), *conf_knn * 0.75, false)
            } else {
                (None, 0.0, false)
            };

        let realizada_soporte_clase = match realizada.as_deref() {
            Some("SI") => self.conteo_realizada[0],
            Some("NO") => self.conteo_realizada[1],
            _ => 0,
        };

        let (motivo, motivo_confianza, knn_motivo_acuerdo) = if self.clf_motivo.muestras >= 6 {
            let probs = self.clf_motivo.predecir_probs(&feats);
            let mut best_idx = 0usize;
            let mut best_p = probs[0];
            for (i, &p) in probs.iter().enumerate().skip(1) {
                if p > best_p {
                    best_p = p;
                    best_idx = i;
                }
            }
            let lbl_lr = MOTIVOS_LABELS[best_idx].to_string();
            if let Some((lbl_knn, conf_knn)) = &knn.motivo {
                if normalizar_texto_sin_tildes(lbl_knn) == normalizar_texto_sin_tildes(&lbl_lr) {
                    (
                        Some(lbl_lr),
                        (0.7 * best_p + 0.3 * conf_knn).min(0.99),
                        *conf_knn >= 0.78,
                    )
                } else {
                    (Some(lbl_lr), best_p, false)
                }
            } else {
                (Some(lbl_lr), best_p, false)
            }
        } else if let Some((lbl_knn, conf_knn)) = &knn.motivo {
            (Some(lbl_knn.clone()), *conf_knn * 0.75, false)
        } else {
            (None, 0.0, false)
        };

        let motivo_soporte_clase = motivo
            .as_deref()
            .and_then(mapear_motivo_a_indice)
            .map(|i| self.conteo_motivo[i])
            .unwrap_or(0);

        let juzgado_sugerido =
            resolver_alias_validado_con_base_para_defensor(base, local, "juzgado", texto, 0.55, defensor)
                .or_else(|| knn.juzgado.filter(|(_, c)| *c >= 0.70).map(|(j, _)| j));
        let tipo_sugerido =
            resolver_alias_validado_con_base_para_defensor(base, local, "tipo", texto, 0.55, defensor)
                .or_else(|| knn.tipo.filter(|(_, c)| *c >= 0.70).map(|(t, _)| t));

        let mut radicado_sugerido = None;
        for num in extraer_numeros_radicado(texto) {
            if let Some(p) = priors.procesos_por_numero.get(&num) {
                radicado_sugerido = Some(p.radicado_completo.clone());
                break;
            }
        }

        let drift_realizada = local
            .page_hinkley_campos
            .get("realizada")
            .or_else(|| base.page_hinkley_campos.get("realizada"))
            .is_some_and(|p| p.drift_detectado);
        let drift_motivo = local
            .page_hinkley_campos
            .get("motivo")
            .or_else(|| base.page_hinkley_campos.get("motivo"))
            .is_some_and(|p| p.drift_detectado);

        PrediccionLocalResumen {
            realizada,
            realizada_confianza,
            motivo,
            motivo_confianza,
            juzgado_sugerido,
            tipo_sugerido,
            radicado_sugerido,
            muestras_entrenamiento: self.muestras_entrenamiento,
            realizadas_clases_vistas,
            realizada_soporte_clase,
            motivos_clases_vistas,
            motivo_soporte_clase,
            knn_realizada_acuerdo,
            knn_motivo_acuerdo,
            drift_realizada,
            drift_motivo,
        }
    }
}

fn clave_dedup_alias(a: &AliasEstadistico) -> (String, String, String, String) {
    let alc = if es_alcance_general(&a.defensor_alcance) {
        "*".to_string()
    } else {
        normalizar_clave_defensor(&a.defensor_alcance)
    };
    (
        a.campo.clone(),
        canonizar_entrada_alias(&a.campo, &a.entrada_norm),
        normalizar_clave_entidad(&a.canonico),
        alc,
    )
}

fn combinar_alias_local_sobre_base<'a>(
    base: &'a MemoriaAprendizaje,
    local: &'a MemoriaAprendizaje,
) -> Vec<&'a AliasEstadistico> {
    let mut vistos: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut out = Vec::with_capacity(local.tabla_alias.len() + base.tabla_alias.len());
    for a in &local.tabla_alias {
        vistos.insert(clave_dedup_alias(a));
        out.push(a);
    }
    for a in &base.tabla_alias {
        if vistos.insert(clave_dedup_alias(a)) {
            out.push(a);
        }
    }
    out
}

pub fn resolver_alias_exacto_validado_con_base_para_defensor(
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
    defensor: Option<&str>,
) -> Option<String> {
    let clave = canonizar_entrada_alias(campo, entrada);
    let clave_segs = clave_estructural_alias(&clave);
    if clave.is_empty() || clave_segs.is_empty() {
        return None;
    }
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let mut mejor_def: Option<(String, f64)> = None;
    let mut mejor_gen: Option<(String, f64)> = None;
    for a in combinar_alias_local_sobre_base(base, local) {
        if a.campo != campo {
            continue;
        }
        let a_norm = canonizar_entrada_alias(campo, &a.entrada_norm);
        if a_norm != clave && clave_estructural_alias(&a_norm) != clave_segs {
            continue;
        }
        let lb = limite_inferior_bayesiano(a.alpha, a.beta);
        if lb < umbral_q10 {
            continue;
        }
        if es_alcance_general(&a.defensor_alcance) {
            if mejor_gen.as_ref().is_none_or(|(_, s)| lb > *s) {
                mejor_gen = Some((a.canonico.clone(), lb));
            }
        } else if coincide_alcance_defensor(&a.defensor_alcance, &def_clave) {
            if mejor_def.as_ref().is_none_or(|(_, s)| lb > *s) {
                mejor_def = Some((a.canonico.clone(), lb));
            }
        }
    }
    mejor_def.or(mejor_gen).map(|(c, _)| c)
}

pub fn resolver_alias_exacto_validado_para_defensor(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
    defensor: Option<&str>,
) -> Option<String> {
    let base = MemoriaAprendizaje::default();
    resolver_alias_exacto_validado_con_base_para_defensor(
        &base, memoria, campo, entrada, umbral_q10, defensor,
    )
}

pub fn resolver_alias_exacto_validado(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
) -> Option<String> {
    resolver_alias_exacto_validado_para_defensor(memoria, campo, entrada, umbral_q10, None)
}

fn contiene_secuencia_tokens_segura(
    campo: &str,
    texto_tokens: &[String],
    patron_tokens: &[String],
) -> bool {
    if patron_tokens.is_empty() || patron_tokens.len() > texto_tokens.len() {
        return false;
    }
    let patron_calificado = campo == "juzgado"
        && patron_tokens
            .last()
            .is_some_and(|t| es_calificador_terminal_juzgado(t));
    for (idx, w) in texto_tokens.windows(patron_tokens.len()).enumerate() {
        if w != patron_tokens {
            continue;
        }
        if campo == "juzgado" && !patron_calificado {
            if let Some(nxt) = texto_tokens.get(idx + patron_tokens.len()) {
                if es_calificador_juzgado(nxt) {
                    continue;
                }
            }
        }
        return true;
    }
    false
}

pub fn resolver_alias_validado_con_base_para_defensor(
    base: &MemoriaAprendizaje,
    local: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
    defensor: Option<&str>,
) -> Option<String> {
    let clave = canonizar_entrada_alias(campo, entrada);
    if clave.is_empty() {
        return None;
    }
    let def_clave = defensor.map(normalizar_clave_defensor).unwrap_or_default();
    let canon_tok = |t: &str| -> String {
        if campo == "juzgado" {
            canonizar_entrada_alias(campo, t)
        } else {
            t.to_string()
        }
    };
    let clave_tokens: Vec<String> = clave.split_whitespace().map(canon_tok).collect();
    let mut mejor_def: Option<(String, (u8, usize, u32))> = None;
    let mut mejor_gen: Option<(String, (u8, usize, u32))> = None;
    for a in combinar_alias_local_sobre_base(base, local) {
        if a.campo != campo {
            continue;
        }
        let es_gen = es_alcance_general(&a.defensor_alcance);
        let es_def = !es_gen && coincide_alcance_defensor(&a.defensor_alcance, &def_clave);
        if !es_def && !es_gen {
            continue;
        }
        let lb = limite_inferior_bayesiano(a.alpha, a.beta);
        if lb < umbral_q10 {
            continue;
        }
        let patron_norm = canonizar_entrada_alias(campo, &a.entrada_norm);
        let patron_tokens: Vec<String> = patron_norm.split_whitespace().map(canon_tok).collect();
        let es_exacto = patron_norm == clave;
        if es_exacto || contiene_secuencia_tokens_segura(campo, &clave_tokens, &patron_tokens) {
            let clave_orden = (
                if es_exacto { 1u8 } else { 0u8 },
                patron_tokens.len(),
                (lb * 10_000.0).round() as u32,
            );
            let destino = if es_def { &mut mejor_def } else { &mut mejor_gen };
            if destino.as_ref().is_none_or(|(_, sc)| clave_orden > *sc) {
                *destino = Some((a.canonico.clone(), clave_orden));
            }
        }
    }
    mejor_def.or(mejor_gen).map(|(c, _)| c)
}

pub fn resolver_alias_validado_para_defensor(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
    defensor: Option<&str>,
) -> Option<String> {
    let base = MemoriaAprendizaje::default();
    resolver_alias_validado_con_base_para_defensor(
        &base, memoria, campo, entrada, umbral_q10, defensor,
    )
}

pub fn resolver_alias_validado(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
) -> Option<String> {
    resolver_alias_validado_para_defensor(memoria, campo, entrada, umbral_q10, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aprendizaje::{EjemploPositivo, MemoriaAprendizaje};

    fn caso_aud(texto: &str, realizada: &str, motivo: &str) -> EjemploPositivo {
        EjemploPositivo {
            id: "t".into(),
            registrada: String::new(),
            proveedor: "openai".into(),
            modelo: "gpt-5-mini".into(),
            texto_anonimizado: texto.into(),
            salida_anonimizada: AudienciaInput {
                radicado: "[PROCESADO] - [RADICADO]".into(),
                tipo: "Audiencia concentrada".into(),
                fecha: "15/08/2025".into(),
                hora: "09:00".into(),
                juzgado: "Juzgado 1 Penal".into(),
                realizada: realizada.into(),
                motivo: motivo.into(),
                observaciones: String::new(),
            },
            reglas_inyectadas: Vec::new(),
            defensor_clave: String::new(),
        }
    }

    fn dataset_benchmark_judicial() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("Audiencia preparatoria se realizó sin novedad ante juzgado", "SI", ""),
            ("Se efectuó audiencia concentrada y se emitió sentido de fallo", "SI", ""),
            ("Acusación instalada sin novedad se fija fecha lectura", "SI", ""),
            ("Lectura de fallo efectuada correctamente en sala virtual", "SI", ""),
            ("Se realizó audiencia preliminar de control de garantías", "SI", ""),
            ("Audiencia de imputación realizada con normalidad", "SI", ""),
            ("Se llevó a cabo audiencia de verificación sin novedad", "SI", ""),
            ("Se realizó acusación y se fija fecha para continuar", "SI", ""),
            ("La fiscal no se conectó y por eso solicité reprogramar", "NO", "Fiscalía"),
            ("Fiscalía solicitó reprogramación por no conocer la carpeta", "NO", "Fiscalía"),
            ("La fiscal no vino a la vista pública fracasa audiencia", "NO", "Fiscalía"),
            ("El juez entró en periodo de vacaciones se reprogramó", "NO", "Juez"),
            ("Fracsa por estar el despacho en otra audiencia programada", "NO", "Juez"),
            ("Error en la agenda del despacho judicial se aplaza", "NO", "Juez"),
            ("Procesado no compareció y no notificaron al usuario", "NO", "Usuario"),
            ("Usuario no se presentó por fallas de conectividad", "NO", "Usuario"),
            ("INPEC no trasladó al detenido al juzgado se suspende", "NO", "INPEC"),
            ("Problemas de traslado por parte del INPEC se aplaza", "NO", "INPEC"),
            ("Abogado contractual no se presentó a la audiencia", "NO", "Defensor Confianza"),
            ("Defensor de confianza ausente solicita reprogramación", "NO", "Defensor Confianza"),
            ("No pude contactar al procesado y pedí aplazamiento", "NO", "Defensor Público"),
            ("Me fue imposible asistir y solicité reprogramar audiencia", "NO", "Defensor Público"),
            ("Víctima no compareció a la audiencia de reparación", "NO", "Víctima"),
            ("Ausencia de la víctima en incidente se reprograma", "NO", "Víctima"),
            ("Representante ICBF no compareció a la diligencia", "NO", "ICBF"),
            ("Audiencia suspende por inasistencia del delegado ICBF", "NO", "ICBF"),
        ]
    }

    fn macro_f1_binario(preds: &[bool], reals: &[bool]) -> f64 {
        let mut f1_sum = 0.0;
        for target in [true, false] {
            let tp = preds
                .iter()
                .zip(reals)
                .filter(|(&p, &r)| p == target && r == target)
                .count() as f64;
            let fp = preds
                .iter()
                .zip(reals)
                .filter(|(&p, &r)| p == target && r != target)
                .count() as f64;
            let fn_ = preds
                .iter()
                .zip(reals)
                .filter(|(&p, &r)| p != target && r == target)
                .count() as f64;
            let denom = 2.0 * tp + fp + fn_;
            if denom > 0.0 {
                f1_sum += (2.0 * tp) / denom;
            }
        }
        f1_sum / 2.0
    }

    #[test]
    fn test_fase2_benchmark_logreg_l2_vs_complement_naive_bayes() {
        let datos = dataset_benchmark_judicial();
        let base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();
        let mut cnb_real = ComplementNaiveBayes::nuevo(2);

        for (texto, real, mot) in &datos {
            local.ejemplos_aciertos.push(caso_aud(texto, real, mot));
            let feats = extraer_features_texto(texto);
            cnb_real.observar(&feats, if *real == "SI" { 1 } else { 0 });
        }

        let motor = MotorEstadisticoLocal::entrenar_desde_memorias(&base, &local);
        let mut lr_preds = Vec::new();
        let mut cnb_preds = Vec::new();
        let mut reals = Vec::new();
        let mut lr_brier = 0.0;
        let mut cnb_brier = 0.0;

        let evaluacion = [
            ("Se realizó audiencia concentrada sin novedad y se fija lectura", true, ""),
            ("La fiscal no se conectó por ello solicité reprogramar", false, "Fiscalía"),
            ("Juzgado ocupado en otra audiencia se reprogramó", false, "Juez"),
            ("INPEC no trasladó al procesado", false, "INPEC"),
            ("Abogado privado no compareció a juicio", false, "Defensor Confianza"),
            ("No logré contactar al procesado y solicité reprogramar", false, "Defensor Público"),
        ];

        for (texto, es_si, mot) in &evaluacion {
            let feats = extraer_features_texto(texto);
            let p_lr = motor.clf_realizada.predecir_prob(&feats);
            let p_cnb = cnb_real.predecir_probs(&feats)[1];
            lr_preds.push(p_lr >= 0.5);
            cnb_preds.push(p_cnb >= 0.5);
            reals.push(*es_si);
            let y = if *es_si { 1.0 } else { 0.0 };
            lr_brier += (p_lr - y).powi(2);
            cnb_brier += (p_cnb - y).powi(2);

            if !*es_si {
                let probs_m = motor.clf_motivo.predecir_probs(&feats);
                let best_m = probs_m
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                    .unwrap()
                    .0;
                assert_eq!(
                    MOTIVOS_LABELS[best_m], *mot,
                    "motivo predicho para '{}'",
                    texto
                );
            }
        }

        let f1_lr = macro_f1_binario(&lr_preds, &reals);
        let f1_cnb = macro_f1_binario(&cnb_preds, &reals);
        assert!(f1_lr >= 0.90, "f1_lr={} f1_cnb={}", f1_lr, f1_cnb);
        assert!(
            lr_brier <= cnb_brier + 0.15,
            "calibración LogReg={} vs CNB={}",
            lr_brier,
            cnb_brier
        );
    }

    #[test]
    fn test_fase2_entidades_ordinales_alias_radicado_y_hora_laboral() {
        assert_eq!(
            normalizar_clave_entidad("Juzgado Tercero Penal del Circuito"),
            normalizar_clave_entidad("Juzgado 3 Penal del Circuito")
        );

        let mut mem = MemoriaAprendizaje::default();
        let pred = AudienciaInput {
            radicado: "200016001075202500111".into(),
            tipo: "Audiencia concentrada".into(),
            fecha: "10/08/2025".into(),
            hora: "02:30".into(),
            juzgado: "Juzgado 3 Circuito Penal".into(),
            realizada: "NO".into(),
            motivo: "Fiscalía".into(),
            observaciones: "Causas secundarias: Defensor Público".into(),
        };
        let mut fin = pred.clone();
        fin.juzgado = "Juzgado 3 Cto Penal".into();
        fin.observaciones = String::new(); // Usuario borró sufijo automático

        for i in 0..4 {
            actualizar_alias_y_preferencias_en_memoria(
                &mut mem,
                "Se fija fecha audiencia en Juzgado 3 Cto Penal",
                &pred,
                &fin,
                &[
                    CampoCorregido {
                        campo: "juzgado".into(),
                        valor_ia: pred.juzgado.clone(),
                        valor_usuario: fin.juzgado.clone(),
                    },
                    CampoCorregido {
                        campo: "observaciones".into(),
                        valor_ia: pred.observaciones.clone(),
                        valor_usuario: fin.observaciones.clone(),
                    },
                ],
                &format!("2025-08-1{}T10:00:00-05:00", i),
            );
        }

        let resuelto = resolver_alias_validado(&mem, "juzgado", "Juzgado 3 Circuito Penal", 0.35);
        assert_eq!(resuelto.as_deref(), Some("Juzgado 3 Cto Penal"));
        // Preferencia de causas secundarias bajó tras rechazos repetidos
        assert!(
            mem.preferencias_estilo.incluir_causas_secundarias_beta
                > mem.preferencias_estilo.incluir_causas_secundarias_alpha
        );

        // Priors .xlsx + vinculación por radicado + desambiguación AM/PM (2:30 -> 14:30)
        let mut priors = IndicePriorsExcel::default();
        priors.horas_freq.insert(14, 15);
        priors.horas_freq.insert(2, 0);
        priors.procesos_por_numero.insert(
            "200016001075202500111".into(),
            ProcesoHabitual {
                radicado_completo: "[PROCESADO] - 200016001075202500111".into(),
                juzgado_habitual: "Juzgado 3 Cto Penal".into(),
                tipo_habitual: "Audiencia concentrada".into(),
                frecuencia: 3,
                defensor_habitual: String::new(),
            },
        );

        let (h_pm, m_pm, cambio_pm) =
            resolver_hora_con_prior("02", "30", "Audiencia a las 2:30 en el juzgado 3", &priors);
        assert_eq!((h_pm.as_str(), m_pm.as_str(), cambio_pm), ("14", "30", true));

        let (h_am, _, cambio_am) =
            resolver_hora_con_prior("02", "30", "Audiencia a las 2:30 am", &priors);
        assert_eq!((h_am.as_str(), cambio_am), ("02", false));
        let (h_am2, _, _) =
            resolver_hora_con_prior("02", "30", "Audiencia 2:30am.", &priors);
        assert_eq!(h_am2, "02");
        let (h_pm7, _, cambio_pm7) =
            resolver_hora_con_prior("07", "00", "Diligencia a las 7:00 p. m.", &priors);
        assert_eq!((h_pm7.as_str(), cambio_pm7), ("19", true));

        // 1) Word boundary en resolver_alias_validado: "Juzgado 1" NO debe capturar "Juzgado 10"
        let mut mem_lim = MemoriaAprendizaje::default();
        for _ in 0..8 {
            actualizar_alias(
                &mut mem_lim,
                "juzgado",
                "Juzgado 1 Penal",
                "Juzgado 1 Penal Valledupar",
                true,
                "2025-08-18T10:00:00-05:00",
            );
        }
        assert_eq!(
            resolver_alias_validado(&mem_lim, "juzgado", "Audiencia en Juzgado 10 Penal", 0.35),
            None
        );

        // 2) Reversión de preferencias estilísticas sin bloqueo en tope tras >= 25 aceptaciones previas
        let mut mem_pref = MemoriaAprendizaje::default();
        let mut pred_obs = pred.clone();
        pred_obs.observaciones = "Causas secundarias: Defensor Público".into();
        let mut fin_obs_keep = pred_obs.clone();
        fin_obs_keep.observaciones = "Causas secundarias: Defensor Público".into();
        let mut fin_obs_del = pred_obs.clone();
        fin_obs_del.observaciones = String::new();
        for _ in 0..25 {
            actualizar_alias_y_preferencias_en_memoria(
                &mut mem_pref,
                "Audiencia normal",
                &pred_obs,
                &fin_obs_keep,
                &[],
                "2025-08-18T10:00:00-05:00",
            );
        }
        for _ in 0..15 {
            actualizar_alias_y_preferencias_en_memoria(
                &mut mem_pref,
                "Audiencia normal",
                &pred_obs,
                &fin_obs_del,
                &[CampoCorregido {
                    campo: "observaciones".into(),
                    valor_ia: pred_obs.observaciones.clone(),
                    valor_usuario: String::new(),
                }],
                "2025-08-19T10:00:00-05:00",
            );
        }
        let p_sec = mem_pref.preferencias_estilo.incluir_causas_secundarias_alpha
            / (mem_pref.preferencias_estilo.incluir_causas_secundarias_alpha
                + mem_pref.preferencias_estilo.incluir_causas_secundarias_beta);
        assert!(
            p_sec < 0.45,
            "La preferencia debe adaptarse debajo de 0.45 tras rechazos sostenidos: p={}",
            p_sec
        );

        // 3) Reversión de alias penaliza canónico competidor
        for _ in 0..6 {
            actualizar_alias(
                &mut mem_lim,
                "juzgado",
                "Juzgado 3 Circuito Penal",
                "Juzgado 3 Cto Penal",
                true,
                "2025-08-20T10:00:00-05:00",
            );
        }
        for _ in 0..6 {
            actualizar_alias(
                &mut mem_lim,
                "juzgado",
                "Juzgado 3 Circuito Penal",
                "Juzgado 3 Circuito Penal",
                true,
                "2025-08-21T10:00:00-05:00",
            );
        }
        assert_eq!(
            resolver_alias_exacto_validado(&mem_lim, "juzgado", "Juzgado 3 Circuito Penal", 0.35).as_deref(),
            Some("Juzgado 3 Circuito Penal")
        );

        let cands = seleccionar_candidatos_prompt(
            "Audiencia proceso 200016001075202500111 en tercero penal circuito",
            &mem,
            &AppSettings::default(),
            &priors,
        );
        assert!(cands.radicado_vinculado.is_some());
        assert!(!cands.juzgados_top.is_empty());
        assert!(juzgados_base().len() >= 50);
    }

    #[test]
    fn test_alias_converge_en_2_correcciones_aislamiento_defensores_y_promocion_general() {
        let mut mem = MemoriaAprendizaje::default();
        let ahora = "2025-08-20T10:00:00-05:00";

        // 1) 1.ª corrección de Defensor A: NO sobrescribe aún (q10 < 0.30), pero sí entra como candidato en prompt (>= 0.22)
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j4",
            "Juzgado 4 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dra. Ana Pérez"),
            ),
            None,
            "1 sola corrección no debe auto-aplicar alias todavía"
        );
        let cands_1 = seleccionar_candidatos_prompt_para_defensor(
            "J04 audiencia preliminar",
            &mem,
            &AppSettings::default(),
            &IndicePriorsExcel::default(),
            Some("Dra. Ana Pérez"),
        );
        assert!(
            cands_1
                .juzgados_top
                .iter()
                .any(|j| j == "Juzgado 4 Penal Municipal de Valledupar"),
            "Tras 1 corrección ya debe aparecer como candidato en prompt: {:?}",
            cands_1.juzgados_top
        );

        // 2) 2.ª corrección consistente de Defensor A: YA se valida y aplica automáticamente (q10 >= 0.30)
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j4",
            "Juzgado 4 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dra. Ana Pérez"),
            )
            .as_deref(),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );
        // Aislamiento: ni Defensor B ni el Cuaderno General (None) ven el alias privado de Defensor A
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dr. Luis Mora"),
            ),
            None
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(&mem, "juzgado", "j4", 0.30, None),
            None
        );

        // 3) Defensor B entrena significado opuesto para j4 (y una corrección negativa de B no corrompe a A)
        for _ in 0..2 {
            actualizar_alias_para_defensor(
                &mut mem,
                "juzgado",
                "j4",
                "Juzgado 4 Penal del Circuito de Santa Marta",
                true,
                ahora,
                Some("Dr. Luis Mora"),
            );
        }
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j4",
            "Juzgado 4 Penal Municipal de Valledupar",
            false,
            ahora,
            Some("Dr. Luis Mora"),
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dra. Ana Pérez"),
            )
            .as_deref(),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dr. Luis Mora"),
            )
            .as_deref(),
            Some("Juzgado 4 Penal del Circuito de Santa Marta")
        );

        // 4) Si Defensor A contradice su propio alias, baja q10 y deja de sobrescribir
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j4",
            "Juzgado 4 Penal Municipal de Valledupar",
            false,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j4",
                0.30,
                Some("Dra. Ana Pérez"),
            ),
            None
        );

        // 5) Promoción automática al Cuaderno General ("*") cuando 2 defensores distintos confirman la misma equivalencia
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j10",
            "Juzgado 10 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "j10",
            "Juzgado 10 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dr. Luis Mora"),
        );
        let alias_j10 = mem
            .tabla_alias
            .iter()
            .find(|a| a.entrada_norm == "j10")
            .expect("debe existir alias j10");
        assert_eq!(alias_j10.defensor_alcance, "*");
        assert_eq!(alias_j10.defensores_confirmantes.len(), 2);
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "j10",
                0.30,
                Some("Dra. Sofía Nueva"),
            )
            .as_deref(),
            Some("Juzgado 10 Penal Municipal de Valledupar")
        );
    }

    #[test]
    fn test_mapa_conexiones_desambigua_sigla_con_defensor_dane_y_tipo() {
        let mut priors = IndicePriorsExcel::default();
        let juz_val = "Juzgado 4 Penal Municipal de Valledupar".to_string();
        let juz_sm = "Juzgado 4 Penal del Circuito de Santa Marta".to_string();

        priors.juzgados_freq.insert(juz_val.clone(), 4);
        priors.juzgados_freq.insert(juz_sm.clone(), 4);
        priors
            .grafo
            .defensor_a_juzgados
            .entry("ana perez".into())
            .or_default()
            .insert(juz_val.clone(), 4);
        priors
            .grafo
            .defensor_sigla_a_juzgado
            .entry("ana perez".into())
            .or_default()
            .entry("j4".into())
            .or_default()
            .insert(juz_val.clone(), 4);
        priors
            .grafo
            .prefijo_radicado_a_juzgados
            .entry("20001".into())
            .or_default()
            .insert(juz_val.clone(), 4);
        priors
            .grafo
            .tipo_a_juzgados
            .entry("audiencia de control de legalidad".into())
            .or_default()
            .insert(juz_val.clone(), 4);

        priors
            .grafo
            .defensor_a_juzgados
            .entry("luis mora".into())
            .or_default()
            .insert(juz_sm.clone(), 4);
        priors
            .grafo
            .defensor_sigla_a_juzgado
            .entry("luis mora".into())
            .or_default()
            .entry("j4".into())
            .or_default()
            .insert(juz_sm.clone(), 4);
        priors
            .grafo
            .prefijo_radicado_a_juzgados
            .entry("47001".into())
            .or_default()
            .insert(juz_sm.clone(), 4);
        priors
            .grafo
            .tipo_a_juzgados
            .entry("audiencia de juicio oral".into())
            .or_default()
            .insert(juz_sm.clone(), 4);

        let cands_a = seleccionar_candidatos_prompt_para_defensor(
            "J04 rad 200016001075202500111 control de garantias",
            &MemoriaAprendizaje::default(),
            &AppSettings::default(),
            &priors,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(cands_a.juzgados_top.first().map(String::as_str), Some(juz_val.as_str()));
        assert!(cands_a
            .siglas_resueltas
            .iter()
            .any(|(s, j)| s == "j4" && j == &juz_val));

        let cands_b = seleccionar_candidatos_prompt_para_defensor(
            "J04 rad 470016001075202500222 juicio oral",
            &MemoriaAprendizaje::default(),
            &AppSettings::default(),
            &priors,
            Some("Dr. Luis Mora"),
        );
        assert_eq!(cands_b.juzgados_top.first().map(String::as_str), Some(juz_sm.as_str()));
        assert!(cands_b
            .siglas_resueltas
            .iter()
            .any(|(s, j)| s == "j4" && j == &juz_sm));

        // Mismo defensor con dos juzgados para la misma sigla J4 (Valledupar freq=5 > Santa Marta freq=3):
        // al aparecer radicado 47001 + juicio oral, el cruce de aristas DANE+tipo desambigua siglas_resueltas hacia Santa Marta.
        priors
            .grafo
            .defensor_sigla_a_juzgado
            .entry("ana perez".into())
            .or_default()
            .entry("j4".into())
            .or_default()
            .insert(juz_val.clone(), 5);
        priors
            .grafo
            .defensor_sigla_a_juzgado
            .entry("ana perez".into())
            .or_default()
            .entry("j4".into())
            .or_default()
            .insert(juz_sm.clone(), 3);
        let cands_cruce = seleccionar_candidatos_prompt_para_defensor(
            "J04 rad 470016001075202500222 juicio oral",
            &MemoriaAprendizaje::default(),
            &AppSettings::default(),
            &priors,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            cands_cruce.siglas_resueltas.first().map(|(_, j)| j.as_str()),
            Some(juz_sm.as_str()),
            "El cruce DANE+tipo debe resolver siglas_resueltas hacia Santa Marta: {:?}",
            cands_cruce.siglas_resueltas
        );
    }

    #[test]
    fn test_aislamiento_motor_local_alias_retractado_ceros_y_override_privado() {
        let mut mem = MemoriaAprendizaje::default();
        let ahora = "2025-08-22T10:00:00-05:00";

        // 1) "J04" y "J4" deben converger juntos a "j4" en 2 correcciones
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "J04",
            "Juzgado 4 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        actualizar_alias_para_defensor(
            &mut mem,
            "juzgado",
            "J4",
            "Juzgado 4 Penal Municipal de Valledupar",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem,
                "juzgado",
                "J04",
                0.30,
                Some("Dra. Ana Pérez"),
            )
            .as_deref(),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );

        // 2) Alias de alta confianza (q10 >= 0.55) y ejemplos k-NN privados de Ana NO se filtran a Luis vía MotorEstadisticoLocal
        for _ in 0..6 {
            actualizar_alias_para_defensor(
                &mut mem,
                "juzgado",
                "j4",
                "Juzgado 4 Penal Municipal de Valledupar",
                true,
                ahora,
                Some("Dra. Ana Pérez"),
            );
        }
        for i in 0..4 {
            let mut ej = caso_aud("J4 audiencia preparatoria de control", "SI", "");
            ej.id = format!("ana-{}", i);
            ej.defensor_clave = "ana perez".into();
            ej.salida_anonimizada.juzgado = "Juzgado 4 Penal Municipal de Valledupar".into();
            mem.ejemplos_aciertos.push(ej);
        }

        let motor_ana = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
            &MemoriaAprendizaje::default(),
            &mem,
            Some("Dra. Ana Pérez"),
        );
        let pred_ana = motor_ana.predecir_para_defensor(
            "J04 audiencia preparatoria de control",
            &mem,
            &IndicePriorsExcel::default(),
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            pred_ana.juzgado_sugerido.as_deref(),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );

        let motor_luis = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
            &MemoriaAprendizaje::default(),
            &mem,
            Some("Dr. Luis Mora"),
        );
        let pred_luis = motor_luis.predecir_para_defensor(
            "J04 audiencia preparatoria de control",
            &mem,
            &IndicePriorsExcel::default(),
            Some("Dr. Luis Mora"),
        );
        assert_eq!(
            pred_luis.juzgado_sugerido, None,
            "MotorEstadisticoLocal nunca debe filtrar alias ni k-NN privados de Ana a Luis"
        );

        // 3) Alias retractado de Ana no debe envenenar ni promover falsamente a "*" cuando Luis lo entrena
        let mut mem_ret = MemoriaAprendizaje::default();
        actualizar_alias_para_defensor(
            &mut mem_ret,
            "juzgado",
            "j7",
            "Juzgado 7 Penal Municipal",
            true,
            ahora,
            Some("Dra. Ana Pérez"),
        );
        for _ in 0..3 {
            actualizar_alias_para_defensor(
                &mut mem_ret,
                "juzgado",
                "j7",
                "Juzgado 7 Penal Municipal",
                false,
                ahora,
                Some("Dra. Ana Pérez"),
            );
        }
        for _ in 0..2 {
            actualizar_alias_para_defensor(
                &mut mem_ret,
                "juzgado",
                "j7",
                "Juzgado 7 Penal Municipal",
                true,
                ahora,
                Some("Dr. Luis Mora"),
            );
        }
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(
                &mem_ret,
                "juzgado",
                "j7",
                0.30,
                Some("Dr. Luis Mora"),
            )
            .as_deref(),
            Some("Juzgado 7 Penal Municipal"),
            "Luis debe activar j7 en 2 correcciones sin quedar bloqueado por el alias retractado de Ana"
        );
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(&mem_ret, "juzgado", "j7", 0.30, None),
            None,
            "Un alias retractado por Ana no debe contar como confirmación para promover a '*'"
        );

        // 4) Override privado del defensor debe ganar sobre "*" en siglas_resueltas incluso si "*" aparece antes
        let mut mem_ov = MemoriaAprendizaje::default();
        for _ in 0..3 {
            actualizar_alias_para_defensor(
                &mut mem_ov,
                "juzgado",
                "j4",
                "Juzgado 4 Penal Municipal de Valledupar",
                true,
                ahora,
                None,
            );
        }
        for _ in 0..2 {
            actualizar_alias_para_defensor(
                &mut mem_ov,
                "juzgado",
                "j4",
                "Juzgado 4 Penal del Circuito de Santa Marta",
                true,
                ahora,
                Some("Dr. Carlos Override"),
            );
        }
        let cands_ov = seleccionar_candidatos_prompt_para_defensor(
            "J04 audiencia preparatoria",
            &mem_ov,
            &AppSettings::default(),
            &IndicePriorsExcel::default(),
            Some("Dr. Carlos Override"),
        );
        assert_eq!(
            cands_ov.siglas_resueltas,
            vec![(
                "j4".to_string(),
                "Juzgado 4 Penal del Circuito de Santa Marta".to_string()
            )]
        );
    }

    #[test]
    fn test_brechas_b_h_con_base_para_defensor_y_aislamiento_en_base() {
        use crate::aprendizaje::AliasEstadistico;
        let mut base = MemoriaAprendizaje::default();
        let mut local = MemoriaAprendizaje::default();

        // Base tiene 6 ejemplos privados de Luis marcando NO / Juez y 2 ejemplos privados de Ana
        for i in 0..6 {
            let mut ej = caso_aud("Juzgado 9 audiencia suspendida caso base", "NO", "Juez");
            ej.id = format!("base-luis-{}", i);
            ej.defensor_clave = "Dr. Luis Mora".into();
            base.ejemplos_aciertos.push(ej);
        }
        for i in 0..4 {
            let mut ej = caso_aud("Juzgado 9 audiencia realizada normalmente caso ana", "SI", "");
            ej.id = format!("base-ana-{}", i);
            ej.defensor_clave = "Dra. Ana Pérez".into();
            base.ejemplos_aciertos.push(ej);
        }

        // Alias heredado en base y override en local (con precedencia local)
        base.tabla_alias.push(AliasEstadistico {
            campo: "juzgado".into(),
            entrada_norm: "j9".into(),
            canonico: "Juzgado 9 Penal Base".into(),
            alpha: 5.0,
            beta: 1.2,
            defensor_alcance: "Dra. Ana Pérez".into(),
            defensores_confirmantes: vec!["ana perez".into()],
            ultima_actualizacion: "2025-08-01T10:00:00-05:00".into(),
        });
        base.tabla_alias.push(AliasEstadistico {
            campo: "juzgado".into(),
            entrada_norm: "j12".into(),
            canonico: "Juzgado 12 Penal Base".into(),
            alpha: 5.0,
            beta: 1.2,
            defensor_alcance: "*".into(),
            defensores_confirmantes: Vec::new(),
            ultima_actualizacion: "2025-08-01T10:00:00-05:00".into(),
        });
        local.tabla_alias.push(AliasEstadistico {
            campo: "juzgado".into(),
            entrada_norm: "j9".into(),
            canonico: "Juzgado 9 Penal Local Override".into(),
            alpha: 5.0,
            beta: 1.2,
            defensor_alcance: "ana perez".into(),
            defensores_confirmantes: vec!["ana perez".into()],
            ultima_actualizacion: "2025-08-10T10:00:00-05:00".into(),
        });

        let motor_ana = MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor(
            &base,
            &local,
            Some("Dra. Ana Pérez"),
        );
        // Los 6 ejemplos de Luis en `base` no deben entrenar el motor de Ana
        assert_eq!(motor_ana.muestras_entrenamiento, 4);

        let priors = IndicePriorsExcel::default();
        let pred_ana = motor_ana.predecir_con_base_para_defensor(
            "J9 audiencia realizada normalmente",
            &base,
            &local,
            &priors,
            Some("Dra. Ana Pérez"),
        );
        assert_eq!(
            pred_ana.juzgado_sugerido.as_deref(),
            Some("Juzgado 9 Penal Local Override")
        );
        assert_eq!(
            resolver_alias_exacto_validado_con_base_para_defensor(
                &base,
                &local,
                "juzgado",
                "j12",
                0.30,
                Some("Dra. Ana Pérez")
            )
            .as_deref(),
            Some("Juzgado 12 Penal Base")
        );
        let cands = seleccionar_candidatos_prompt_con_base_para_defensor(
            "J12 audiencia concentrada",
            &base,
            &local,
            &AppSettings::default(),
            &priors,
            Some("Dra. Ana Pérez"),
        );
        assert!(cands
            .siglas_resueltas
            .iter()
            .any(|(s, j)| s == "j12" && j == "Juzgado 12 Penal Base"));
    }

    #[test]
    fn test_flujo_completo_feedback_generalizacion_patrones_y_override_especifico() {
        let mut mem = MemoriaAprendizaje::default();
        let def = Some("Dra. Ana Pérez");
        let base_aud = AudienciaInput {
            radicado: "PROCESADO - 200016001075202500111".into(),
            tipo: "Audiencia preparatoria".into(),
            fecha: "15/08/2025".into(),
            hora: "09:00".into(),
            juzgado: String::new(),
            realizada: "SI".into(),
            motivo: String::new(),
            observaciones: String::new(),
        };

        // 1) El LLM NO conservó la abreviación ("JPM10", "JPM9", "JPM3") en `prediccion.juzgado`,
        //    pero el texto sí la traía; la cosecha general debe aprenderla y generalizar hacia "JPM4".
        for (i, n) in [10u32, 9, 3].into_iter().enumerate() {
            let mut pred = base_aud.clone();
            pred.juzgado = format!("Juzgado {} Penal", n);
            let mut fin = base_aud.clone();
            fin.juzgado = format!("Juzgado {} Penal Municipal", n);
            actualizar_alias_y_preferencias_en_memoria_para_defensor(
                &mut mem,
                &format!("JPM{} audiencia preparatoria, se realizó sin novedad", n),
                &pred,
                &fin,
                &[CampoCorregido {
                    campo: "juzgado".into(),
                    valor_ia: pred.juzgado.clone(),
                    valor_usuario: fin.juzgado.clone(),
                }],
                &format!("2025-08-1{}T10:00:00-05:00", i),
                def,
            );
        }

        let cands_jpm4 = seleccionar_candidatos_prompt_para_defensor(
            "JPM4 audiencia preparatoria",
            &mem,
            &AppSettings::default(),
            &IndicePriorsExcel::default(),
            def,
        );
        assert!(
            cands_jpm4
                .patrones_inferidos
                .iter()
                .any(|p| p.expansion_sugerida == "Juzgado 4 Penal Municipal"),
            "Debe inferir JPM4 desde JPM10/JPM9/JPM3: {:?}",
            cands_jpm4.patrones_inferidos
        );
        assert!(
            cands_jpm4
                .juzgados_top
                .iter()
                .any(|j| j == "Juzgado 4 Penal Municipal"),
            "Debe impulsar Juzgado 4 Penal Municipal en juzgados_top: {:?}",
            cands_jpm4.juzgados_top
        );

        // 2) Si el usuario corrige JPM4 a Circuito, se registra el override específico para JPM4
        //    y la consistencia del patrón cae (3/4 = 75% < 85%)
        let mut pred4 = base_aud.clone();
        pred4.juzgado = "Juzgado 4 Penal Municipal".into();
        let mut fin4 = base_aud.clone();
        fin4.juzgado = "Juzgado 4 Penal del Circuito".into();
        actualizar_alias_y_preferencias_en_memoria_para_defensor(
            &mut mem,
            "JPM4 audiencia preparatoria",
            &pred4,
            &fin4,
            &[CampoCorregido {
                campo: "juzgado".into(),
                valor_ia: pred4.juzgado.clone(),
                valor_usuario: fin4.juzgado.clone(),
            }],
            "2025-08-19T10:00:00-05:00",
            def,
        );
        let cands_jpm4_tras_corr = seleccionar_candidatos_prompt_para_defensor(
            "JPM4 audiencia preparatoria",
            &mem,
            &AppSettings::default(),
            &IndicePriorsExcel::default(),
            def,
        );
        assert!(
            cands_jpm4_tras_corr.patrones_inferidos.is_empty(),
            "Tras contradicción en JPM4 el patrón debe pausarse: {:?}",
            cands_jpm4_tras_corr.patrones_inferidos
        );

        // 3) Flujo {X}: JPM Bosconia + JPM Curumaní -> JPM Chimichagua aceptado se promueve a tabla_alias
        let mut mem_x = MemoriaAprendizaje::default();
        for (ab, full) in [
            ("JPM Bosconia", "Juzgado Promiscuo Municipal de Bosconia"),
            ("JPM Curumaní", "Juzgado Promiscuo Municipal de Curumaní"),
        ] {
            let mut pred = base_aud.clone();
            pred.juzgado = ab.into();
            let mut fin = base_aud.clone();
            fin.juzgado = full.into();
            actualizar_alias_y_preferencias_en_memoria_para_defensor(
                &mut mem_x,
                &format!("{} audiencia preliminar", ab),
                &pred,
                &fin,
                &[CampoCorregido {
                    campo: "juzgado".into(),
                    valor_ia: pred.juzgado.clone(),
                    valor_usuario: fin.juzgado.clone(),
                }],
                "2025-08-20T10:00:00-05:00",
                def,
            );
        }
        // Usuario acepta sin editar la sugerencia de JPM Chimichagua
        let mut ok_ch = base_aud.clone();
        ok_ch.juzgado = "Juzgado Promiscuo Municipal de Chimichagua".into();
        actualizar_alias_y_preferencias_en_memoria_para_defensor(
            &mut mem_x,
            "JPM Chimichagua audiencia preliminar",
            &ok_ch,
            &ok_ch,
            &[],
            "2025-08-21T10:00:00-05:00",
            def,
        );
        assert!(
            mem_x
                .tabla_alias
                .iter()
                .any(|a| a.entrada_norm == "jpm chimichagua"
                    && a.canonico == "Juzgado Promiscuo Municipal de Chimichagua"),
            "Confirmar un patrón {{X}} debe consolidar el alias concreto: {:?}",
            mem_x.tabla_alias
        );

        // 4) Abreviación de tipo de 2 letras ("JO" / "J.O.") converge en 2 correcciones y entra a siglas_resueltas
        let mut mem_tipo = MemoriaAprendizaje::default();
        for ab in ["JO", "J.O."] {
            let mut pred = base_aud.clone();
            pred.tipo = ab.into();
            let mut fin = base_aud.clone();
            fin.tipo = "Audiencia de juicio oral".into();
            actualizar_alias_y_preferencias_en_memoria_para_defensor(
                &mut mem_tipo,
                &format!("{} en J4 sin novedad", ab),
                &pred,
                &fin,
                &[CampoCorregido {
                    campo: "tipo".into(),
                    valor_ia: pred.tipo.clone(),
                    valor_usuario: fin.tipo.clone(),
                }],
                "2025-08-22T10:00:00-05:00",
                def,
            );
        }
        assert_eq!(
            resolver_alias_exacto_validado_para_defensor(&mem_tipo, "tipo", "J.O.", 0.30, def)
                .as_deref(),
            Some("Audiencia de juicio oral")
        );
        let cands_jo = seleccionar_candidatos_prompt_para_defensor(
            "JO en Juzgado 1 sin novedad",
            &mem_tipo,
            &AppSettings::default(),
            &IndicePriorsExcel::default(),
            def,
        );
        assert!(
            cands_jo
                .siglas_resueltas
                .iter()
                .any(|(s, t)| s == "jo" && t == "Audiencia de juicio oral"),
            "{:?}",
            cands_jo.siglas_resueltas
        );

        // 5) Guard de calificador en resolver_alias_validado (MotorEstadisticoLocal):
        //    alias de alta confianza `j4 -> Municipal` NO debe capturar "Audiencia en J4 CTO"
        let mut mem_hi = MemoriaAprendizaje::default();
        for _ in 0..6 {
            actualizar_alias_para_defensor(
                &mut mem_hi,
                "juzgado",
                "J4",
                "Juzgado 4 Penal Municipal de Valledupar",
                true,
                "2025-08-22T10:00:00-05:00",
                def,
            );
        }
        assert_eq!(
            resolver_alias_validado_para_defensor(&mem_hi, "juzgado", "Audiencia en J4 CTO", 0.55, def),
            None
        );
        assert_eq!(
            resolver_alias_validado_para_defensor(
                &mem_hi,
                "juzgado",
                "Audiencia en J4 preparatoria",
                0.55,
                def
            )
            .as_deref(),
            Some("Juzgado 4 Penal Municipal de Valledupar")
        );
    }
}
