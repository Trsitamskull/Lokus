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
    aplicar_decaimiento_beta_en, calcular_bm25, colapsar_espacios, limite_inferior_bayesiano,
    ngramas_caracteres, normalizar_ess_par, quitar_tildes, tokenizar_judicial, AliasEstadistico,
    CampoCorregido, MemoriaAprendizaje, PrediccionLocalResumen, MAX_ESS_N, PRIOR_BETA_ALPHA,
    PRIOR_BETA_BETA,
};
use crate::config::AppSettings;
use crate::models::{
    extraer_numeros_radicado, mapear_motivo_a_indice, normalizar_texto_sin_tildes, AudienciaInput,
    IndicePriorsExcel, ProcesoHabitual, MOTIVOS_LABELS, TIPOS_AUDIENCIA,
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

fn actualizar_alias(
    memoria: &mut MemoriaAprendizaje,
    campo: &str,
    entrada_raw: &str,
    canonico_raw: &str,
    positivo: bool,
    ahora: &str,
) {
    let entrada_norm = normalizar_clave_entidad(entrada_raw);
    let canonico = colapsar_espacios(canonico_raw);
    let canonico_norm = normalizar_clave_entidad(&canonico);
    if entrada_norm.len() < 3 || canonico.len() < 3 {
        return;
    }

    if positivo {
        for otro in &mut memoria.tabla_alias {
            if otro.campo == campo
                && otro.entrada_norm == entrada_norm
                && normalizar_clave_entidad(&otro.canonico) != canonico_norm
            {
                aplicar_decaimiento_beta_en(
                    &mut otro.alpha,
                    &mut otro.beta,
                    &otro.ultima_actualizacion,
                    Some(ahora),
                );
                otro.beta += 1.0;
                normalizar_ess_par(&mut otro.alpha, &mut otro.beta, MAX_ESS_N);
                otro.ultima_actualizacion = ahora.to_string();
            }
        }
    }

    if let Some(alias) = memoria.tabla_alias.iter_mut().find(|a| {
        a.campo == campo
            && a.entrada_norm == entrada_norm
            && normalizar_clave_entidad(&a.canonico) == canonico_norm
    }) {
        aplicar_decaimiento_beta_en(
            &mut alias.alpha,
            &mut alias.beta,
            &alias.ultima_actualizacion,
            Some(ahora),
        );
        if positivo {
            alias.alpha += 1.0;
            alias.canonico = canonico;
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

    memoria.tabla_alias.push(AliasEstadistico {
        campo: campo.to_string(),
        entrada_norm,
        canonico,
        alpha: PRIOR_BETA_ALPHA + 1.0,
        beta: PRIOR_BETA_BETA,
        ultima_actualizacion: ahora.to_string(),
    });
}

pub fn actualizar_alias_y_preferencias_en_memoria(
    memoria: &mut MemoriaAprendizaje,
    texto: &str,
    prediccion: &AudienciaInput,
    final_usuario: &AudienciaInput,
    campos_corregidos: &[CampoCorregido],
    ahora: &str,
) {
    let corregido = |campo: &str| campos_corregidos.iter().any(|c| c.campo == campo);

    // 1. Tabla de alias para juzgado y tipo
    if !final_usuario.juzgado.trim().is_empty() {
        if corregido("juzgado") {
            if !prediccion.juzgado.trim().is_empty() {
                actualizar_alias(
                    memoria,
                    "juzgado",
                    &prediccion.juzgado,
                    &prediccion.juzgado,
                    false,
                    ahora,
                );
                actualizar_alias(
                    memoria,
                    "juzgado",
                    &prediccion.juzgado,
                    &final_usuario.juzgado,
                    true,
                    ahora,
                );
            }
            actualizar_alias(
                memoria,
                "juzgado",
                &final_usuario.juzgado,
                &final_usuario.juzgado,
                true,
                ahora,
            );
        } else {
            actualizar_alias(
                memoria,
                "juzgado",
                &final_usuario.juzgado,
                &final_usuario.juzgado,
                true,
                ahora,
            );
        }
    }

    if !final_usuario.tipo.trim().is_empty() && final_usuario.tipo != "Otra" {
        if corregido("tipo") {
            if !prediccion.tipo.trim().is_empty() {
                actualizar_alias(memoria, "tipo", &prediccion.tipo, &prediccion.tipo, false, ahora);
                actualizar_alias(
                    memoria,
                    "tipo",
                    &prediccion.tipo,
                    &final_usuario.tipo,
                    true,
                    ahora,
                );
            }
            actualizar_alias(
                memoria,
                "tipo",
                &final_usuario.tipo,
                &final_usuario.tipo,
                true,
                ahora,
            );
        } else {
            actualizar_alias(
                memoria,
                "tipo",
                &final_usuario.tipo,
                &final_usuario.tipo,
                true,
                ahora,
            );
        }
    }

    // 2. Preferencias estilísticas adaptables en observaciones y "se fija"
    //    Se normaliza el tamaño efectivo de muestra (MAX_ESS_PREF = 12.0) para evitar bloqueo en el tope.
    const MAX_ESS_PREF: f64 = 12.0;
    let pred_obs = prediccion.observaciones.to_lowercase();
    let fin_obs = final_usuario.observaciones.to_lowercase();

    if pred_obs.contains("causas secundarias:") {
        if fin_obs.contains("causas secundarias:") {
            memoria.preferencias_estilo.incluir_causas_secundarias_alpha += 1.0;
        } else {
            memoria.preferencias_estilo.incluir_causas_secundarias_beta += 1.0;
        }
        normalizar_ess_par(
            &mut memoria.preferencias_estilo.incluir_causas_secundarias_alpha,
            &mut memoria.preferencias_estilo.incluir_causas_secundarias_beta,
            MAX_ESS_PREF,
        );
    } else if fin_obs.contains("causas secundarias:") {
        memoria.preferencias_estilo.incluir_causas_secundarias_alpha += 1.0;
        normalizar_ess_par(
            &mut memoria.preferencias_estilo.incluir_causas_secundarias_alpha,
            &mut memoria.preferencias_estilo.incluir_causas_secundarias_beta,
            MAX_ESS_PREF,
        );
    }

    if pred_obs.contains("reprogramada para:") {
        if fin_obs.contains("reprogramada para:") {
            memoria.preferencias_estilo.incluir_reprogramacion_alpha += 1.0;
        } else {
            memoria.preferencias_estilo.incluir_reprogramacion_beta += 1.0;
        }
        normalizar_ess_par(
            &mut memoria.preferencias_estilo.incluir_reprogramacion_alpha,
            &mut memoria.preferencias_estilo.incluir_reprogramacion_beta,
            MAX_ESS_PREF,
        );
    } else if fin_obs.contains("reprogramada para:") {
        memoria.preferencias_estilo.incluir_reprogramacion_alpha += 1.0;
        normalizar_ess_par(
            &mut memoria.preferencias_estilo.incluir_reprogramacion_alpha,
            &mut memoria.preferencias_estilo.incluir_reprogramacion_beta,
            MAX_ESS_PREF,
        );
    }

    let texto_lower = quitar_tildes(&texto.to_lowercase());
    if (texto_lower.contains("se fija") || texto_lower.contains("se fijo"))
        && !texto_lower.contains("nueva fecha")
    {
        if final_usuario.realizada.trim().eq_ignore_ascii_case("NO") {
            memoria.preferencias_estilo.se_fija_aplazamiento_alpha += 1.0;
        } else if final_usuario.realizada.trim().eq_ignore_ascii_case("SI") {
            memoria.preferencias_estilo.se_fija_aplazamiento_beta += 1.0;
        }
        normalizar_ess_par(
            &mut memoria.preferencias_estilo.se_fija_aplazamiento_alpha,
            &mut memoria.preferencias_estilo.se_fija_aplazamiento_beta,
            MAX_ESS_PREF,
        );
    }
}

#[derive(Debug, Clone, Default)]
pub struct CandidatosPrompt {
    pub juzgados_top: Vec<String>,
    pub tipos_top: Vec<String>,
    pub radicado_vinculado: Option<ProcesoHabitual>,
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

pub fn seleccionar_candidatos_prompt(
    texto: &str,
    memoria: &MemoriaAprendizaje,
    settings: &AppSettings,
    priors: &IndicePriorsExcel,
) -> CandidatosPrompt {
    let query_tokens: BTreeSet<String> = tokenizar_judicial(texto).into_iter().collect();

    // 1. Candidatos de juzgado (alias validados + settings + priors .xlsx + JUZGADOS_BASE)
    let mut mapa_juz: BTreeMap<String, (String, f64)> = BTreeMap::new();
    let mut insertar_juz = |nombre: &str, freq: u32, bonus: f64| {
        let limpio = colapsar_espacios(nombre);
        if limpio.is_empty() {
            return;
        }
        let clave = normalizar_clave_entidad(&limpio);
        let score = puntuar_candidato_entidad(&query_tokens, &limpio, freq, bonus);
        if score <= 0.05 {
            return;
        }
        match mapa_juz.get_mut(&clave) {
            Some(existente) if score > existente.1 => *existente = (limpio, score),
            None => {
                mapa_juz.insert(clave, (limpio, score));
            }
            _ => {}
        }
    };

    for a in &memoria.tabla_alias {
        if a.campo == "juzgado" {
            let lb = limite_inferior_bayesiano(a.alpha, a.beta);
            if lb >= 0.30 {
                let freq = *priors.juzgados_freq.get(&a.canonico).unwrap_or(&0);
                insertar_juz(&a.canonico, freq, lb * 1.8);
            }
        }
    }
    for j in &settings.juzgados {
        let freq = *priors.juzgados_freq.get(j).unwrap_or(&0);
        insertar_juz(j, freq, 0.35);
    }
    for (j, &freq) in &priors.juzgados_freq {
        insertar_juz(j, freq, 0.25);
    }
    for j in juzgados_base() {
        let freq = *priors.juzgados_freq.get(j).unwrap_or(&0);
        insertar_juz(j, freq, 0.0);
    }

    let mut juzgados_vec: Vec<(String, f64)> = mapa_juz.into_values().collect();
    juzgados_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let juzgados_top: Vec<String> = juzgados_vec.into_iter().take(5).map(|(n, _)| n).collect();

    // 2. Candidatos de tipo de audiencia
    let mut mapa_tipo: BTreeMap<String, (String, f64)> = BTreeMap::new();
    let mut insertar_tipo = |nombre: &str, freq: u32, bonus: f64| {
        let limpio = colapsar_espacios(nombre);
        if limpio.is_empty() || limpio == "Otra" {
            return;
        }
        let clave = normalizar_clave_entidad(&limpio);
        let score = puntuar_candidato_entidad(&query_tokens, &limpio, freq, bonus);
        if score <= 0.05 {
            return;
        }
        match mapa_tipo.get_mut(&clave) {
            Some(existente) if score > existente.1 => *existente = (limpio, score),
            None => {
                mapa_tipo.insert(clave, (limpio, score));
            }
            _ => {}
        }
    };

    for t in &settings.tipos_audiencia {
        let freq = *priors.tipos_freq.get(t).unwrap_or(&0);
        insertar_tipo(t, freq, 0.35);
    }
    for (t, &freq) in &priors.tipos_freq {
        insertar_tipo(t, freq, 0.25);
    }
    for &t in &TIPOS_AUDIENCIA {
        let freq = *priors.tipos_freq.get(t).unwrap_or(&0);
        insertar_tipo(t, freq, 0.0);
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
    }
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
        let mut clf_realizada = ClasificadorBinarioLogRegL2::default();
        let mut clf_motivo = ClasificadorMotivoLogRegL2::default();
        let mut corpus_knn: Vec<RegistroHistorico> = Vec::new();
        let mut conteo_realizada = [0usize; 2];
        let mut conteo_motivo = [0usize; 8];

        // Reunir observaciones balanceadas (aciertos confirmados + correcciones con ground truth final_usuario)
        let mut muestras_realizada: Vec<(Vec<(String, f64)>, bool, f64)> = Vec::new();
        let mut muestras_motivo: Vec<(Vec<(String, f64)>, usize, f64)> = Vec::new();

        for ac in base
            .ejemplos_aciertos
            .iter()
            .chain(local.ejemplos_aciertos.iter())
        {
            if ac.texto_anonimizado.trim().is_empty() {
                continue;
            }
            let feats = extraer_features_texto(&ac.texto_anonimizado);
            let real_up = ac.salida_anonimizada.realizada.trim().to_uppercase();
            if real_up == "SI" || real_up == "NO" {
                let es_si = real_up == "SI";
                conteo_realizada[if es_si { 0 } else { 1 }] += 1;
                muestras_realizada.push((feats.clone(), es_si, 1.0));
                if !es_si {
                    if let Some(idx) = mapear_motivo_a_indice(&ac.salida_anonimizada.motivo) {
                        conteo_motivo[idx] += 1;
                        muestras_motivo.push((feats, idx, 1.0));
                    }
                }
            }
            corpus_knn.push(RegistroHistorico {
                texto: ac.texto_anonimizado.clone(),
                tokens: tokenizar_judicial(&ac.texto_anonimizado),
                salida: ac.salida_anonimizada.clone(),
            });
        }

        for c in base.correcciones.iter().chain(local.correcciones.iter()) {
            if c.estado == "descartada" || c.texto_original.trim().is_empty() {
                continue;
            }
            let feats = extraer_features_texto(&c.texto_original);
            let real_up = c.final_usuario.realizada.trim().to_uppercase();
            if real_up == "SI" || real_up == "NO" {
                let es_si = real_up == "SI";
                conteo_realizada[if es_si { 0 } else { 1 }] += 1;
                muestras_realizada.push((feats.clone(), es_si, 1.1));
                if !es_si {
                    if let Some(idx) = mapear_motivo_a_indice(&c.final_usuario.motivo) {
                        conteo_motivo[idx] += 1;
                        muestras_motivo.push((feats, idx, 1.1));
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

        let juzgado_sugerido = resolver_alias_validado(memoria, "juzgado", texto, 0.55)
            .or_else(|| knn.juzgado.filter(|(_, c)| *c >= 0.70).map(|(j, _)| j));
        let tipo_sugerido = resolver_alias_validado(memoria, "tipo", texto, 0.55)
            .or_else(|| knn.tipo.filter(|(_, c)| *c >= 0.70).map(|(t, _)| t));

        let mut radicado_sugerido = None;
        for num in extraer_numeros_radicado(texto) {
            if let Some(p) = priors.procesos_por_numero.get(&num) {
                radicado_sugerido = Some(p.radicado_completo.clone());
                break;
            }
        }

        let drift_realizada = memoria
            .page_hinkley_campos
            .get("realizada")
            .is_some_and(|p| p.drift_detectado);
        let drift_motivo = memoria
            .page_hinkley_campos
            .get("motivo")
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

pub fn resolver_alias_exacto_validado(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
) -> Option<String> {
    let clave = normalizar_clave_entidad(entrada);
    if clave.is_empty() {
        return None;
    }
    let mut mejor: Option<(String, f64)> = None;
    for a in &memoria.tabla_alias {
        if a.campo != campo || a.entrada_norm != clave {
            continue;
        }
        let lb = limite_inferior_bayesiano(a.alpha, a.beta);
        if lb < umbral_q10 {
            continue;
        }
        if mejor.as_ref().is_none_or(|(_, s)| lb > *s) {
            mejor = Some((a.canonico.clone(), lb));
        }
    }
    mejor.map(|(c, _)| c)
}

fn contiene_secuencia_tokens(texto_tokens: &[&str], patron_tokens: &[&str]) -> bool {
    if patron_tokens.is_empty() || patron_tokens.len() > texto_tokens.len() {
        return false;
    }
    texto_tokens
        .windows(patron_tokens.len())
        .any(|w| w == patron_tokens)
}

pub fn resolver_alias_validado(
    memoria: &MemoriaAprendizaje,
    campo: &str,
    entrada: &str,
    umbral_q10: f64,
) -> Option<String> {
    let clave = normalizar_clave_entidad(entrada);
    if clave.is_empty() {
        return None;
    }
    let clave_tokens: Vec<&str> = clave.split_whitespace().collect();
    let mut mejor: Option<(String, (u8, usize, u32))> = None;
    for a in &memoria.tabla_alias {
        if a.campo != campo {
            continue;
        }
        let lb = limite_inferior_bayesiano(a.alpha, a.beta);
        if lb < umbral_q10 {
            continue;
        }
        let patron_tokens: Vec<&str> = a.entrada_norm.split_whitespace().collect();
        let es_exacto = a.entrada_norm == clave;
        if es_exacto || contiene_secuencia_tokens(&clave_tokens, &patron_tokens) {
            let clave_orden = (
                if es_exacto { 1u8 } else { 0u8 },
                patron_tokens.len(),
                (lb * 10_000.0).round() as u32,
            );
            if mejor.as_ref().is_none_or(|(_, sc)| clave_orden > *sc) {
                mejor = Some((a.canonico.clone(), clave_orden));
            }
        }
    }
    mejor.map(|(c, _)| c)
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
}
