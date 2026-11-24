# Plan Técnico: Recepción, Ingesta Idempotente, Compactación y Aplicación del Autoaprendizaje de IA y Uso de Catálogos en Lokus

**Versión del documento:** 1.1  
**Fecha:** 2026-11-23  
**Alcance:** Requisitos `R1`–`R5` (Hito `M1 / F1` y especificación contractual para `M2`–`M4`)  
**Repositorio objetivo:** `Lokus — Gestor de Audiencias Judiciales` (`v5.2.0`, rama `rust-tauri`)  
**Nota de referencia de líneas:** Las citas de línea en `aprendizaje.rs` y `uso_catalogos.rs` toman como línea base `HEAD` previo a las ediciones evolutivas de `M2`; cuando difieren en el árbol de trabajo (`WT`) de `M2`, se indica explícitamente (`HEAD` / `WT`).

---

## Resumen Ejecutivo

Lokus ya cuenta con un sistema avanzado de exportación diagnóstica (`PaqueteExportacion`, `formato: 4`, implementado en `src-tauri/src/aprendizaje.rs`) que combina el autoaprendizaje de IA (`ia_aprendizaje.json`), las asociaciones estadísticas locales de Excel (`GrafoAsociaciones`) y la telemetría de uso de catálogos (`uso_catalogos.json`). Sin embargo, hoy el ciclo es unidireccional: la aplicación **exporta** paquetes JSON hacia Descargas, pero **no existe un flujo automatizado e idempotente para que el mantenedor ingiera múltiples entregas** (longitudinales del mismo usuario o provenientes de distintas instalaciones), filtre contaminación o ruido, compacte el conocimiento y actualice de forma segura `aprendizaje_base.json`, los catálogos estáticos (`juzgados_base.json`, `TIPOS_AUDIENCIA`, `HORAS_RAPIDAS`) y las reglas de dominio de `ai.rs`.

Este documento establece el plan técnico completo en cinco partes:
1. **Análisis del esquema vigente (`formato: 4`) y de la carga/fusión en tiempo de ejecución.**
2. **Evaluación de tres opciones de arquitectura y justificación de la Opción A** (módulo librería `src-tauri/src/ingesta.rs` + binario CLI `lokus_ingestar`).
3. **Mecanismo exacto de deduplicación e idempotencia** (Mes 1 → Mes 3, entregas fuera de orden Mes 3 → Mes 1, multiusuario, polinización cruzada de respaldos `.lokus` y evolución compatible `formato: 4` → `formato: 5`).
4. **Política campo por campo** de retención, agregación, compactación, cuarentena y poda (preservando estrictamente la inmutabilidad de `base_invariante`).
5. **Auditoría cuantitativa del JSON real aportado** (`exportado: "2026-11-23T10:29:16.716664800-05:00"`, 5.203 líneas, `formato: 4`) y **hoja de ruta `R5`** con mejoras nativas verificadas línea a línea en el código fuente.

---

## 1. Análisis del esquema actual de exportación (`PaqueteExportacion`, `formato: 4`) y carga/fusión en tiempo de ejecución

### 1.1 Desglose de `PaqueteExportacion` (`formato: 4`) y tipos de soporte

En `src-tauri/src/aprendizaje.rs` (`VERSION_FORMATO: u32 = 4`, línea `88`), la función `exportar_aprendizaje_completo_con_priors_en` (líneas `7786–8114`) construye la estructura `PaqueteExportacion` (líneas `885–916`, actualmente privada y con `#[derive(Debug, Serialize)]` únicamente). Sus **29 campos** se agrupan en metadatos de exportación, contadores acumulados, métricas diagnósticas, conocimiento aprendido, grafo de priors y reporte de catálogos:

| # | Campo en `PaqueteExportacion` | Tipo Rust (`aprendizaje.rs:885-916`) | Origen / Construcción en `exportar_aprendizaje_completo_con_priors_en` |
|---:|---|---|---|
| 1 | `app` | `&'static str` | Literal `"Lokus"` (`8050`). |
| 2 | `version_app` | `&'static str` | `env!("CARGO_PKG_VERSION")` (`"5.2.0"` en `Cargo.toml:3`; `"5.1.0"` en la muestra real). |
| 3 | `formato` | `u32` | `VERSION_FORMATO` (`4` en `88` y `8052`). |
| 4 | `exportado` | `String` | Marca temporal RFC3339 (`Local::now().to_rfc3339()`, `8053`). |
| 5 | `aciertos` | `u32` | `memoria.aciertos` acumulado de extracciones sin corrección (`8054`). |
| 6 | `total_reglas` | `usize` | `reglas_anonimizadas.len()` (solo reglas locales, no incluye `aprendizaje_base.json`, `8055`). |
| 7 | `total_correcciones` | `usize` | `correcciones_anonimizadas.len()` (`8056`). |
| 8 | `total_ejemplos_aciertos` | `usize` | `memoria.ejemplos_aciertos.len()` — **solo el conteo**, sin los payloads `EjemploPositivo` (`8057`). |
| 9 | `perfiles_defensores` | `usize` | `defensores_directorio.len()` (`8058`). |
| 10 | `llamadas_induccion_llm_total` | `u32` | Conteo acumulado de llamadas LLM de inducción (`8059`). |
| 11 | `llamadas_extraccion_total` | `u32` | Conteo acumulado de llamadas LLM de extracción (`8060`). |
| 12 | `llamadas_consolidacion_total` | `u32` | Conteo acumulado de llamadas LLM de consolidación (`8061`). |
| 13 | `correcciones_por_campo` | `BTreeMap<String, usize>` | Conteo de entradas `c.campos` por `campo` (`7836–7841`). |
| 14 | `metricas_por_campo` | `Vec<CampoDiagnostico>` | `evaluaciones`, `aciertos`, `correcciones`, `precision_ewma`, `confianza_q10`, `drift_activo` (`779–788`, `7485–7517`). |
| 15 | `metricas_por_modelo` | `Vec<ModeloDiagnostico>` | `clave` (`proveedor:modelo`), `evaluaciones`, `aciertos`, `correcciones`, `precision_media` (`790–797`, `7519–7536`). |
| 16 | `preferencias_estilo` | `PreferenciasEstiloResumen` | Proyección probabilística `a / (a + b)` de `memoria.preferencias_estilo` (`838–845`, `7002–7019`). |
| 17 | `preferencias_por_defensor` | `BTreeMap<String, PreferenciasEstiloResumen>` | Mapa por clave normalizada de defensor hacia `PreferenciasEstiloResumen` (`7924–7935`). |
| 18 | `defensores_directorio` | `Vec<ResumenDefensorExport>` | Conserva nombre real y clave unificada del defensor, `aciertos`, `correcciones`, `tasa_acierto`, `por_campo` (`583–592`, `7206–7341`). |
| 19 | `resumen_patrones_generales` | `ResumenPatronesGenerales` | Vista derivada Top-10/Top-12 de juzgados/tipos/horas, conteo de reglas transversales y errores recurrentes (`572–581`, `7351–7406`). |
| 20 | `grafo_asociaciones` | `GrafoAsociaciones` | Relaciones `defensor_a_juzgados`, `defensor_a_tipos`, `defensor_sigla_a_juzgado`, `prefijo_radicado_a_juzgados`, `tipo_a_juzgados` (`models.rs:173–210`), anonimizadas vía `CacheCatalogoAnon` (`7944–8000`). |
| 21 | `perfiles_sinteticos` | `Vec<PerfilSinteticoDefensor>` | `ambito` (`"*"` o clave de defensor), `nombre_mostrar`, `resumen`, `version`, `ultima_actualizacion` (`468–476`, `8007–8041`). |
| 22 | `consolidaciones_ia` | `Vec<RegistroConsolidacionIa>` | Historial de consolidaciones (`id`, `fecha`, `ambito`, `modelo`, `acciones_reglas`, `resumen_actualizado`, `sugerencias_build`, `telemetria_disparo`), anonimizado (`513–525`, `7408–7474`). |
| 23 | `alias_validados` | `Vec<AliasDiagnostico>` | Proyección de `tabla_alias` filtrada estrictamente a `q10 >= 0.30` y anonimizada (`814–822`, `7538–7570`). |
| 24 | `patrones_abreviacion` | `Vec<PatronAbreviacionDiagnostico>` | Patrones `{N}`/`{X}`/siglas inducidos en vivo con `inducir_patrones_todos_ambitos` y anonimizados (`824–836`, `7575–7593`). |
| 25 | `juzgados_agregados_usuario` | `Vec<String>` | Lista de `settings.juzgados` anonimizada vía `CacheCatalogoAnon` (`7904–7907`). |
| 26 | `tipos_audiencia_agregados_usuario` | `Vec<String>` | Lista de `settings.tipos_audiencia` anonimizada vía `CacheCatalogoAnon` (`7908–7911`). |
| 27 | `uso_catalogos` | `ReporteUsoCatalogos` | Reporte estructurado generado por `construir_reporte_uso_catalogos` (`uso_catalogos.rs`, `HEAD:118–126, 1131–1172` / `WT:124–132, 1162–1203`). |
| 28 | `reglas` | `Vec<ReglaAprendida>` | Reglas locales completas anonimizadas (`173–219`, `7857–7888`). |
| 29 | `correcciones` | `Vec<CorreccionIa>` | Correcciones locales completas anonimizadas (`387–412`, `7890–7902`). |

#### Estructuras internas origen que alimentan el paquete
- **`MemoriaAprendizaje`** (`aprendizaje.rs:681–728`): contiene además `ejemplos_aciertos: Vec<EjemploPositivo>` (`221–233`), `tabla_alias: Vec<AliasEstadistico>` (`313–341`, con `alpha`, `beta`, `defensores_confirmantes` y `ultima_actualizacion`), `preferencias_estilo: PreferenciasEstilo` y `preferencias_por_defensor` con pares Beta `(alpha, beta)` (`282–311`), `estados_consolidacion`, `estado_aclaracion`, `cola_induccion`, `page_hinkley_*` y `semilla_rng`. No contiene `grafo_asociaciones` ni `uso_catalogos`.
- **`UsoCatalogos`** (`uso_catalogos.rs:42–61`, `VERSION_USO = 1`, persistido en `config/uso_catalogos.json`): almacena `instancia_id: String` (`"inst-{nanos:x}-{seq:04x}"`, `144–151`), `sembrado_inicial`, `sembrado_en`, `ultima_actualizacion`, `total_audiencias_contabilizadas`, `pendientes_compartir`, `*_sin_especificar`, mapas `tipos_audiencia`, `juzgados` y `horas_exactas`, y los conjuntos clave de linaje **`fuentes_importadas: BTreeSet<String>`** y **`paquetes_importados: BTreeSet<String>`**.

### 1.2 Por qué `PaqueteExportacion` (`formato: 4`) es una proyección diagnóstica con pérdida

Aunque `formato: 4` es rico para auditoría humana, presenta **cinco pérdidas estructurales** respecto al estado real en `MemoriaAprendizaje` y `UsoCatalogos`:

1. **Omite `UsoCatalogos.instancia_id`**: el identificador persistente de la instalación ya existe en `config/uso_catalogos.json`, pero `construir_reporte_uso_catalogos` (`uso_catalogos.rs`, `HEAD:1131–1172` / `WT:1162–1203`) no lo incluye en `ReporteUsoCatalogos`. Sin él, el ingester no tiene una clave primaria explícita de fuente en `formato: 4`.
2. **Omite `UsoCatalogos.fuentes_importadas`**: si el Usuario A restauró un respaldo `.lokus` del Usuario B, el conteo local del Usuario A absorbió los conteos del Usuario B. Al exportar `formato: 4`, esa relación no viaja en el JSON, creando riesgo de doble conteo si se ingieren ambos usuarios.
3. **Omite los parámetros Beta crudos `(alpha, beta)` en alias y preferencias**:
   - `AliasDiagnostico` (`aprendizaje.rs:814–822`) solo expone `confianza_q10` y excluye cualquier `AliasEstadistico` con `q10 < 0.30` (como alias observados `1` vez, cuyo `q10 ≈ 0.25` con `PRIOR_ALIAS_BETA = 1.2`), además de descartar `defensores_confirmantes` y `ultima_actualizacion`.
   - `PreferenciasEstiloResumen` (`aprendizaje.rs:838–845`) solo expone la media `a / (a + b)`, perdiendo el tamaño efectivo de muestra (ESS). Por ejemplo, el valor por defecto `3.0 / (3.0 + 1.0) = 0.75` es indistinguible en `formato: 4` de un par actualizado `9.0 / (9.0 + 3.0) = 0.75`.
4. **Omite los cuerpos de `ejemplos_aciertos`**: `total_ejemplos_aciertos` exporta únicamente el entero `len()`, impidiendo aprovechar anclas positivas (`EjemploPositivo`) para entrenar a `MotorEstadisticoLocal` en `aprendizaje_base.json`.
5. **No incluye `exportacion_seq` ni `ultima_exportacion_previa`**: no hay secuencia monotónica declarada para detectar entregas fuera de orden.

### 1.3 Carga y fusión en tiempo de ejecución en la aplicación nativa

#### A. Base embebida de IA (`src-tauri/resources/aprendizaje_base.json`)
- **Carga única:** `memoria_base_embebida()` (`aprendizaje.rs:918–926`) deserializa en un `OnceLock<MemoriaAprendizaje>` la constante compilada `APRENDIZAJE_BASE_JSON` (`include_str!("../resources/aprendizaje_base.json")`, línea `102`) y ejecuta `migrar_memoria_v1_a_v2(&mut m)` (`1570–1672`).
- **Contenido actual (`141` líneas):** `version: 2`, `aciertos: 0`, **3 reglas** `origen: "base_invariante"` (`base-abreviacion-cto`, `base-jerarquia-causas-reacciones`, `base-defensor-confianza-contractual`, todas `alpha: 6.0, beta: 1.0`) y **2 correcciones** (`base-caso-dostin-fiscalia`, `base-caso-cto-juzgado`).
- **Campos aún sin poblar en `aprendizaje_base.json`:** `preferencias_estilo`, `preferencias_por_defensor`, `perfiles_sinteticos`, `tabla_alias` y `ejemplos_aciertos` quedan vacíos/por defecto vía `#[serde(default)]` (`aprendizaje.rs:682`).
- **Precedencia estricta `local > base` en cada fusión:**
  - *Reglas* (`seleccionar_contexto_detallado_para_defensor`, `HEAD:2812–2831` / `WT:2942–2960`): prioriza `local.reglas` e incorpora `base.reglas` solo si no hay colisión de `id` ni duplicado semántico en el mismo ámbito (`es_misma_regla`, `2436–2454`). **Brecha de sombreado en runtime detectada (`M2`/`M4`):** actualmente `reglas_local_ap` no excluye reglas locales en estado `"retirada"` o `"cuarentena"` antes de evaluar `duplicada_en_local`, por lo que una regla local retirada/en cuarentena que coincida con `es_misma_regla` o `id` puede sombrear una regla `base_invariante` e impedir su entrada al prompt. Debe eximirse explícitamente a toda regla `base_invariante` (`origen == "base_invariante"` o `id.starts_with("base-")`) de ser sombreada por reglas locales cuyo `estado` no sea `"activa"` (y en particular nunca por `"retirada"` ni `"cuarentena"`).
  - *Correcciones y ejemplos positivos* (`2941–2956`, `3032–3049`; `estadistica_ia.rs:1436–1522`): concatena `local` antes de `base` deduplicando por `id`. **Punto crítico de rendimiento:** `MotorEstadisticoLocal::entrenar_desde_memorias_para_defensor` (`estadistica_ia.rs:1420–1542`) se ejecuta **en cada extracción de IA** (`ai.rs:2385`) entrenando Regresión Logística L2 + BM25 k-NN desde cero sobre la unión `base + local`. Por ello, `aprendizaje_base.json` debe mantenerse compacto y altamente curado.
  - *Alias y patrones de abreviación:* `combinar_alias_local_sobre_base` (`estadistica_ia.rs:1710–1726`) y `combinar_alias_para_patrones` (`patrones_abreviacion.rs:1462–1491`) sobreponen `local.tabla_alias` sobre `base.tabla_alias` por `(campo, entrada, canonico, alcance)` y re-inducen dinámicamente `{N}`, `{X}` y siglas (`inducir_patrones_abreviacion_para_defensor`, `1496–1575`).
  - *Preferencias y perfiles sintéticos:* en la implementación vigente de `preferencias_efectivas_con_base_para_defensor` (`aprendizaje.rs`, `HEAD:1021–1107` / `WT:1051–1137`), el Cuaderno General (`combinada_gen`) sí mezcla par por par contra `Default`; sin embargo, cuando existe **cualquier** entrada en `local.preferencias_por_defensor`, `if let Some(p_loc) = local_def { return p_loc.clone(); }` (`HEAD:1099–1101` / `WT:1129–1131`) devuelve la estructura completa del defensor de inmediato, **sombreando** `base.preferencias_estilo` y `base.preferencias_por_defensor` en campos aún intactos/por defecto (como `incluir_reprogramacion` en `3.0 / 1.0`). En `R5-C` (`M2`/`M4`) se exige corregir esto mezclando cada uno de los 4 pares `(alpha, beta)` individualmente con precedencia `local_def > base_def > local_gen > base_gen > Default` para que las mejoras de `aprendizaje_base.json` alcancen a defensores existentes. Por su parte, `obtener_resumenes_consolidados_prompt` (`1124–1160`) toma el resumen `"*"` (`<= 320` chars) y el del defensor (`<= 380` chars) dando precedencia a `local` sobre `base`.

#### B. Catálogos estáticos, `UsoCatalogos` y duplicación Frontend/Backend
- **Backend (`src-tauri/resources/` y `models.rs`):**
  - `src-tauri/resources/juzgados_base.json`: **96 juzgados** (Cesar, Magdalena, La Guajira y genéricos nacionales), cargado en `OnceLock` vía `include_str!` en `estadistica_ia.rs:39–45`.
  - `src-tauri/src/models.rs:4–43`: `MOTIVOS_LABELS` (8 valores fijos: `Juez`, `Fiscalía`, `Usuario`, `INPEC`, `Víctima`, `ICBF`, `Defensor Confianza`, `Defensor Público`), `TIPOS_AUDIENCIA` (18 valores: 17 tipos + `"Otra"`) y `HORAS_RAPIDAS` (6 horas: `08:00`, `09:00`, `10:00`, `14:00`, `15:00`, `16:00`).
  - `UsoCatalogos` (`config/uso_catalogos.json`): en la primera ejecución (`inicializar_uso_si_falta`, `uso_catalogos.rs:497–532`) escanea todas las planillas `.xlsx` existentes, siembra los contadores, sincroniza las listas del usuario en `settings.json` y fija `sembrado_inicial = true` + `sembrado_en`. Luego ajusta conteos incrementalmente al crear/editar/eliminar audiencias.
- **Frontend (`src/lib/constants.js`):** duplica textualmente `TIPOS_AUDIENCIA` (`1–20`), `MOTIVOS_LABELS` (`22–31`), `HORAS_RAPIDAS` (`33–40`) y `JUZGADOS_BASE` (`122–226`, 96 entradas), porque `ConfigPublica` (`config.rs:64–77`) únicamente expone al frontend las listas añadidas por el usuario (`tipos_audiencia`, `juzgados`). **Implicación operativa:** toda actualización de catálogos base en `M4` debe sincronizar `juzgados_base.json`, `models.rs` y `src/lib/constants.js` de forma atómica.

#### C. `IndicePriorsExcel` / `GrafoAsociaciones`
- Definido en `models.rs:173–227` y gestionado en `excel.rs` (`FORMATO_PRIORS_ACTUAL = 3`, línea `23`; `NOMBRE_INDICE_PRIORS = ".lokus_priors.json"`, línea `22`).
- Se construye **exclusivamente** a partir de las planillas `.xlsx` locales (`reconstruir_priors_en` / `cargar_o_inicializar_priors_en`, `excel.rs:2544–2579`, y `actualizar_priors_incremental_con_defensor_en`, `2581–2611`). No existe hoy ningún grafo base embebido para arranque en frío (*cold start*).

#### D. `PaqueteLokus` y precedente de linaje en `uso_catalogos.rs`
- `src-tauri/src/paquete.rs:32–94` define `PaqueteLokus` (`FORMATO_PAQUETE = 1`):
  - `.lokus` (`tipo_paquete == "completo"`, JSON comprimido con `flate2`/gzip, líneas `261–278`): exporta planillas, estadísticas, configuración sin API keys y **`uso_catalogos: Option<UsoCatalogos>`** completo.
  - `.lokus.json` (`tipo_paquete == "estadisticas"`, `279–290`): omite `configuracion` y `uso_catalogos`.
- Al importar un `.lokus`, `fusionar_uso_importado_en` (`uso_catalogos.rs`, `HEAD:777–919` / `WT:808–940`, llamado desde `paquete.rs:679–692`) ya implementa el precedente del repositorio para identificar fuentes:
  - Verifica `es_misma_fuente` comparando `paq.instancia_id == local.instancia_id || local.fuentes_importadas.contains(&paq.instancia_id)` (`HEAD:791–793` / `WT:822–824`) y usa `firma_paquete = "{instancia_id}|{ultima_actualizacion}|{total_audiencias_contabilizadas}"` (`HEAD:794–799` / `WT:825–830`).
  - Si `local` es una instalación distinta con historial previo (`HEAD:868–897` / `WT:900–928`), **suma** los conteos de `paq` dentro de `local` e inserta `paq.instancia_id` en `local.fuentes_importadas` (`HEAD:894` / `WT:925`) — aunque en `HEAD` no propaga transitivamente `paq.fuentes_importadas` ni registra un desglose por fuente en `PaqueteExportacion`, lo cual se corrige en `M2`/`M3` (§3.2.4).
  - **Caso borde crítico (`HEAD:803–819` / `WT:834–850`):** si la instalación receptora tiene `0` audiencias y mapas vacíos, clona `paq` completo (incluyendo **`paq.sembrado_en`** si no está vacío), conservando su propio `instancia_id` e insertando `paq.instancia_id` en `fuentes_importadas` (y en `paquete.rs:59–75` copia además las planillas `.xlsx`, pero **no** copia `ia_aprendizaje.json`).

---

## 2. Comparación de opciones de arquitectura y justificación de la Opción A

Para que el mantenedor reciba uno o varios JSON exportados, los deduplice contra entregas previas, compacte la señal útil, actualice `aprendizaje_base.json` y emita un reporte accionable de actualización de catálogos y reglas de dominio, se evaluaron tres alternativas:

| Criterio de evaluación | Opción A: Módulo librería Rust (`ingesta.rs`) + Binario CLI (`lokus_ingestar`) | Opción B: Flujo dentro de la UI de escritorio Tauri (`ConfigIaModal.svelte`) | Opción C: Script externo independiente (Python / Node.js) |
|---|---|---|---|
| **Reutilización de lógica de dominio** | **100% directa** vía `lokus_audiencias_lib` (`rlib` ya declarada en `Cargo.toml:8–10`). Cero reimplementación. | **100% directa**, pero mezcla responsabilidades de usuario final y mantenedor. | **0%**: obliga a duplicar algoritmos complejos de Rust en otro lenguaje con alto riesgo de deriva. |
| **Capacidad de actualizar la base del programa** | **Completa**: opera directamente en el árbol fuente (`src-tauri/resources/aprendizaje_base.json`, `ingesta_ledger.json`, reporte para `juzgados_base.json`/`models.rs`/`constants.js`). | **Bloqueada en compilados release**: `APRENDIZAJE_BASE_JSON` (`aprendizaje.rs:102`) y `JUZGADOS_BASE_JSON` (`estadistica_ia.rs:39`) se incrustan en tiempo de compilación con `include_str!`; el binario instalado no puede reescribir sus propios recursos embebidos. | **Parcial**: puede editar JSON, pero no puede usar `MotorEstadisticoLocal`, `es_misma_regla` ni `cuantil_beta` sin reescribirlos. |
| **Dependencias nuevas** | **Cero** (`serde`, `serde_json`, `chrono`, `regex` ya existen en `Cargo.toml:21–27`). | N/A (añadiría comandos IPC y componentes UI innecesarios para el usuario final). | Requeriría entorno externo (Python/Node) y librerías de terceros o reimplementaciones manuales. |
| **Verificación automatizada e idempotencia** | **`cargo test` nativo** directamente sobre `ingesta::tests` + ejecución determinista en CI/E2E con `--formato-salida json`. | Requiere pruebas manuales o WebDriver sobre Tauri UI. | Desacoplado de `cargo test`; no detecta rupturas cuando cambian structs de `aprendizaje.rs`. |
| **Seguridad y separación de roles** | **Total**: el usuario final mantiene una UI limpia (`ConfigIaModal.svelte` de 2 pestañas); el mantenedor ejecuta un CLI auditable con `--dry-run`. | Riesgo de exponer herramientas de curación interna a defensores públicos. | Separado de la UI, pero frágil ante evolución de tipos Rust (`formato: 5`). |

### Decisión arquitectónica: Opción A

Se adopta la **Opción A** por cinco razones técnicas determinantes:

1. **Cero duplicación de funciones críticas de dominio en Rust:** el proceso de deduplicación, compactación y promoción estadística depende exactamente de las mismas funciones que usa Lokus en tiempo de ejecución, cuya semántica ya está fijada en el crate `lokus_audiencias_lib` (`src-tauri/Cargo.toml:8–15`, `src-tauri/src/lib.rs:1–9`):
   - `normalizar_clave_catalogo` y `normalizar_texto_sin_tildes` (`models.rs:256–290`).
   - `normalizar_clave_defensor` (`models.rs:295–335`, eliminación iterativa de 14 prefijos honoríficos españoles).
   - `es_misma_regla` (`aprendizaje.rs:2436–2454`, similitud Dice `>= 0.78` o igualdad/subconjunto de tokens `>= 0.85`).
   - `cuantil_beta` y `limite_inferior_bayesiano` (`aprendizaje.rs:1882–1992`, `ln_gamma`, `betacf`, `beta_inc` y bisección de 48 pasos).
   - `tokenizar_judicial` (`aprendizaje.rs:2197–2225`, colapso ordinal, stopwords judiciales y tokenización de radicados).
   - `canonizar_entrada_alias` (`estadistica_ia.rs:454–456`, wrapper que delega a `patrones_abreviacion::canonizar_entrada_alias_general`, importado en `estadistica_ia.rs:33`) e `inducir_patrones_todos_ambitos` (`patrones_abreviacion.rs:1607–1659`).
2. **Inmutabilidad de `include_str!` en binarios compilados:** dado que la aplicación de escritorio embebe `aprendizaje_base.json` y `juzgados_base.json` en compilación, la ingesta pertenece al ciclo de desarrollo/release del repositorio, no al binario distribuido al usuario final.
3. **Cero dependencias adicionales en `Cargo.toml`:** `serde`, `serde_json`, `chrono` y `regex` ya están disponibles; solo se agrega `pub mod ingesta;` en `src-tauri/src/lib.rs` y la sección `[[bin]] name = "lokus_ingestar"` en `src-tauri/Cargo.toml` sin alterar `[[bin]] name = "Lokus"`.
4. **Contrato CLI explícito, auditable y apto para E2E:**
   ```bash
   cargo run --manifest-path src-tauri/Cargo.toml --bin lokus_ingestar -- [OPCIONES] <INPUT_JSON...>
   ```
   Flags contractuales (alineadas con `PROJECT.md`):
   - `--base <PATH>`: ruta a `aprendizaje_base.json` (defecto: `src-tauri/resources/aprendizaje_base.json`).
   - `--ledger <PATH>`: ruta al ledger persistente del mantenedor (defecto: `src-tauri/resources/ingesta_ledger.json`).
   - `--catalogos-base <PATH>`: ruta a `juzgados_base.json` (defecto: `src-tauri/resources/juzgados_base.json`).
   - `--reporte <PATH>`: ruta de salida para el reporte de curación en Markdown (opcional; si se omite, imprime resumen en `stdout`).
   - `--fuente <ALIAS>`: anulación explícita de identificador de fuente para archivos `formato: 4` sin `instancia_id`.
   - `--dry-run`: calcula estadísticas, cuarentenas y propuestas sin modificar `--base` ni `--ledger`.
   - `--formato-salida <text|json>`: formato del resumen en `stdout` (`text` por defecto, `json` para automatización y pruebas E2E).
   - **Códigos de salida:** `0` en éxito (incluyendo *no-op* idempotente ante duplicados o snapshots obsoletos, claramente reportado); distinto de cero (`1` / `2`) ante argumentos inválidos, JSON ilegible o malformado, `formato > 5` futuro no soportado, campos obligatorios faltantes o violación de invariantes.
5. **Validación directa con `cargo test`:** todas las propiedades de idempotencia, orden inverso, multiusuario y protección `base_invariante` quedan cubiertas dentro de la misma suite de pruebas de Rust (`cd src-tauri && cargo test`).

---

## 3. Mecanismo exacto de deduplicación, idempotencia y evolución de formato (`formato: 4` → `formato: 5`)

### 3.1 Por qué fallan tanto el *append* ingenuo como el reemplazo total ingenuo

Un `PaqueteExportacion` combina **dos naturalezas de datos incompatibles** con cualquier estrategia simple:

1. **Contadores y mapas acumulados de por vida de la instalación** (no se recortan):
   - `aciertos`, `llamadas_induccion_llm_total`, `llamadas_extraccion_total`, `llamadas_consolidacion_total`.
   - `metricas_por_campo`, `metricas_por_modelo`, `defensores_directorio`.
   - `uso_catalogos` (`total_audiencias_contabilizadas`, `tipos_audiencia`, `juzgados`, `horas`).
   - `grafo_asociaciones`.
   - *Fallo del append ingenuo:* si el usuario exporta en el Mes 1 (`141` audiencias, `71` aciertos) y vuelve a exportar en el Mes 3 (`260` audiencias, `140` aciertos), sumar ambos JSON produciría `401` audiencias y `211` aciertos — inflando artificialmente las frecuencias históricas.

2. **Colecciones acotadas con expulsión por ventana deslizante o prioridad** (`aprendizaje.rs:48–82` y `estadistica_ia.rs:40`):
   - `correcciones`: tope `MAX_CORRECCIONES = 300` (elimina las más antiguas `0..exceso`, `aprendizaje.rs:6368–6371`).
   - `reglas`: tope `MAX_REGLAS = 60` (expulsa la de menor `prioridad_desalojo_regla`: `retirada=0 < cuarentena=1 < candidata=2 < prueba=3 < activa=4`, desempate por `q10`, con `base-* = 255`; `2473–2486, 2660–2671`).
   - `consolidaciones_ia`: tope `MAX_CONSOLIDACIONES_HISTORIAL = 12` (elimina las más antiguas `0..exceso`, `6137–6140`).
   - `tabla_alias`: tope `MAX_ALIAS = 180` (expulsa menor `(q10, idx)`, `estadistica_ia.rs:627–641`).
   - `ejemplos_aciertos`: tope `MAX_ACIERTOS_EJEMPLOS = 200` (elimina los más antiguos `0..exceso`, `aprendizaje.rs:6417–6420`).
   - *Fallo del reemplazo ingenuo:* si el Mes 3 reemplaza por completo el registro del Mes 1, cualquier corrección, regla o `sugerencia_build` del Mes 1 que haya sido expulsada por los topes `300 / 60 / 12` entre el Mes 1 y el Mes 3 **desaparecería del histórico del mantenedor**, perdiendo trazabilidad y señal de aprendizaje ya depurada.
   - *Nota adicional sobre `exportada`:* actualmente `exportar_aprendizaje_completo_con_priors_en` (`aprendizaje.rs:8049–8111`) exporta **todas** las correcciones en el búfer (tanto `exportada: false` como `exportada: true`) y marca `c.exportada = true` después de `fs::write`. Por lo tanto, el campo `exportada` en el JSON es informativo (indica qué era nuevo *respecto a exportaciones previas del cliente*), no un filtro de exportación; la deduplicación del lado del mantenedor no debe depender únicamente de `exportada == false`.

### 3.2 Modelo adoptado: Reemplazo de Snapshot por Fuente + Ledger Global de Unión por ID (`ingesta_ledger.json`)

El ledger persistente del mantenedor (`ingesta_ledger.json`) separa estrictamente ambos tipos de datos por `source_id`:

```
                  ┌──────────────────────────────────────┐
                  │ Entrada JSON (formato 4 ó formato 5) │
                  └──────────────────┬───────────────────┘
                                     ▼
               1. Verificar SHA-256 & huella (source_id, seq, exportado)
                  ├─ Coincidencia exacta ──► No-op idempotente (exit 0)
                  └─ Nuevo o distinto
                                     ▼
               2. Resolver source_id (v5 instancia_id | v4 fallback | --fuente)
                                     ▼
          ┌──────────────────────────┴──────────────────────────┐
          ▼                                                     ▼
  [A. Contadores / Snapshots por fuente]             [B. Entidades con ID global]
  (uso_catalogos, grafo, aciertos,                   (reglas, correcciones,
   llamadas_*, metricas_*, perfiles)                  consolidaciones, ejemplos)
          │                                                     │
          ▼                                                     ▼
  Comparar marca de agua monotónica                  SIEMPRE unir por ID global:
  (exportacion_seq, exportado)                       - Nuevo ID -> insertar en unión
  - Si entrante >= último: REEMPLAZAR                - ID ya visto -> conservar/actualizar
    snapshot de ese source_id                          solo si snapshot entrante es más
  - Si entrante < último (fuera de orden):             reciente (o mayor estado/actualización)
    IGNORAR contadores obsoletos!                    - ¡Backfill garantizado en Mes 3->Mes 1!
          │                                                     │
          └──────────────────────────┬──────────────────────────┘
                                     ▼
               3. Agregación entre fuentes distintas (excluyendo solapamientos
                  por `.lokus` vía `fuentes_importadas`), Cuarentena, Compactación
                  y Escritura de `aprendizaje_base.json` (preservando `base_invariante`)
```

#### Comportamiento en los 4 escenarios críticos (`R3`)

1. **Mismo usuario a través del tiempo (`Mes 1` → `Mes 3`):**
   - `Mes 1` crea `sources["inst-A"]` con `snapshot = { audiencias: 141, aciertos: 71, uso_catalogos, grafo, metricas }` y registra los IDs de `reglas` (`13`), `correcciones` (`37`) y `consolidaciones_ia` (`6`) en las colecciones indexadas por `id`.
   - `Mes 3` llega con `inst-A` más reciente (`exportacion_seq` mayor o `exportado` posterior). El snapshot acumulado de `sources["inst-A"]` **se reemplaza** por el del Mes 3 (no se suma `141 + 260`), mientras las colecciones por `id` hacen **unión**: los IDs compartidos se actualizan in-place sin duplicarse, los IDs nuevos del Mes 3 se agregan, y los IDs del Mes 1 expulsados por los topes `300/60/12` en el Mes 3 **permanecen intactos en el ledger**.

2. **Entregas fuera de orden (`Mes 3` → `Mes 1`):**
   - Primero se ingiere `Mes 3` (`exportado: 2026-12-20`, `260` audiencias). La marca de agua de `sources["inst-A"]` queda fijada en `(seq_3, "2026-12-20...")`.
   - Después llega `Mes 1` (`exportado: 2026-11-23`, `141` audiencias). Al detectar `entrante < marca_de_agua`, el ingester **ignora** todos los contadores acumulados del Mes 1 (evitando retroceder `260 -> 141` o sumar `260 + 141`), pero **procesa en modo *backfill* las entidades con ID** (`reglas`, `correcciones`, `consolidaciones_ia`, `ejemplos_aciertos`): cualquier ID histórico del Mes 1 que ya hubiera sido expulsado en el Mes 3 y no exista aún en el ledger se incorpora sin alterar los conteos acumulados.

3. **Re-ingesta idempotente del mismo JSON:**
   - Si el hash `SHA-256` del archivo completo (o la tupla `(source_id, exportacion_seq, exportado)`) ya consta en `ledger.ingested_files`, el ingester concluye sin mutar `ledger` ni `aprendizaje_base.json`, reporta estado `"idempotent_noop"` y devuelve código de salida `0`.

4. **Múltiples usuarios / instalaciones distintas (`Usuario A` + `Usuario B`) y polinización `.lokus`:**
   - Dos fuentes distintas (`sources["inst-A"]` y `sources["inst-B"]`) **suman** sus snapshots en los totales globales del ledger (`uso_catalogos`, `grafo_asociaciones`, `metricas_*`, `aciertos`), ponderan métricas (`precision_media = sum(aciertos) / sum(evaluaciones)`) y habilitan la **promoción por consenso transversal** (p. ej., una regla, alias o adición de catálogo observada en `>= 2` fuentes distintas sin conflicto puede promoverse a `"*"` / catálogo base).
   - **Prevención segura de doble conteo por respaldos `.lokus` (sin resta ingenua):** cuando `Usuario A` importa el respaldo `.lokus` de `Usuario B`, `UsoCatalogos.fuentes_importadas` (`uso_catalogos.rs:58`, `HEAD:810, 894` / `WT:841, 925`) registra `inst-B`. En `M2`, la rama de fusión no vacía de `fusionar_uso_importado_en` (`HEAD:868–897` / `WT:900–928`) debe además **propagar transitivamente `paq.fuentes_importadas` dentro de `local.fuentes_importadas`** (cerrando la cadena `inst-B -> inst-A -> inst-C`), y `PaqueteExportacion` `formato: 5` expone ese conjunto en `fuentes_importadas: Vec<String>` (manteniendo intacto el contrato de campos de `PaqueteExportacion` en `PROJECT.md`). Durante la agregación entre fuentes en `ingesta.rs` (`M3`):
     - **Prohibición de la resta ingenua (`inst-A + inst-B - inst-B`) y del `max` simple:** si `inst-B` fue absorbida por `inst-A` en $t_1$ (con `100` audiencias) y `inst-B` continuó trabajando hasta $t_2 > t_{\text{import}}$ (alcanzando `140` audiencias), restar el snapshot actual `inst-B(t_2)` destruiría las `40` audiencias nuevas de `inst-B`; tomar `max(inst-A, inst-B)` perdería igualmente el crecimiento posterior o las audiencias propias de `inst-A`.
     - **Deduplicación acotada por línea base o delta posterior:** si el ledger dispone de un snapshot histórico previo de `inst-B` con `ultima_actualizacion <= inst-A.ultima_actualizacion` (o una cota absorbida conocida `inst-B_absorbed_cap` registrada en el ledger/baseline al momento de la transferencia), la deducción se acota estrictamente a `min(inst-B_absorbed_cap, inst-B_snapshot)`, conservando íntegro el delta posterior `inst-B(t_2) - inst-B_at_import`. Si `inst-B.ultima_actualizacion > inst-A.ultima_actualizacion` o no existe snapshot/cota histórica previa de `inst-B` anterior a la importación, **nunca** se resta `inst-B(t_2)` a ciegas: se preserva el crecimiento de `inst-B` (restando solo si existe cota histórica segura, o evitando la resta ciega) y se emite obligatoriamente la bandera estructurada **`"overlap_con_crecimiento_posterior"`** en el reporte de curación (`ingesta_reporte.md` y salida JSON) para `uso_catalogos`.
     - **Solapamiento en `grafo_asociaciones`:** como `.lokus` (`paquete.rs:261–278`) también copia las planillas `.xlsx` de `inst-B` dentro de `inst-A`, `excel.rs` reconstruye `inst-A.grafo_asociaciones` incluyendo esas audiencias. Cuando `inst-A.fuentes_importadas` contiene `inst-B` y ambos están en el ledger, `ingesta.rs` debe aplicar la misma detección de solapamiento sobre `grafo_asociaciones` y emitir **`"overlap_con_crecimiento_posterior"`** / advertencia de grafo compartido, evitando sumar a ciegas las aristas solapadas de `inst-A` e `inst-B`.
     - **En `formato: 4`** (donde `fuentes_importadas` aún no viaja), la restauración sobre instalación nueva (`uso_catalogos.rs`, `HEAD:803–819` / `WT:834–850`) se gestiona mediante las reglas de huella y detección de clones `.lokus` de §3.3.A y el desambiguador manual `--fuente <ALIAS>`.

### 3.3 Resolución de fuente en `formato: 4` vs evolución a `formato: 5`

#### A. Algoritmo de huella para `formato: 4` (sin `instancia_id`)
Cuando un JSON entrante tiene `formato == 4` (como la muestra real aportada):
1. Si el mantenedor pasó `--fuente <ALIAS>`, usar `format!("manual:{}", normalizar_clave_catalogo(alias))`.
2. En caso contrario, inspeccionar `ledger.sources` buscando coincidencia determinista:
   - **Ancla 1 (primaria):** `uso_catalogos.sembrado_en` no vacío (en la muestra real: `"2026-11-19T11:11:25.345266200-05:00"`), que es inmutable tras la siembra inicial de la instalación.
   - **Ancla 2 (confirmación / desambiguación exhaustiva de clon `.lokus`):** calcular el solapamiento de IDs no-`base-*` (`regla-*`, `corr-*`, `cons-*`) y de `defensores_directorio` contra las fuentes con el mismo `sembrado_en` en el ledger, cubriendo explícitamente **todos los subcasos**:
     1. **Mismo `sembrado_en` + `>= 1` ID no-`base-*` compartido:** se confirma la **misma fuente** (`v4:sembrado:<hash>`).
     2. **Mismo `sembrado_en` + IDs no-`base-*` disjuntos + `defensores_directorio` disjunto:** **clon `.lokus` confirmado** → dividir con advertencia asignando `v4:clon:<sembrado_hash>:<ids_o_def_hash>` (o requerir `--fuente`).
     3. **Mismo `sembrado_en` + IDs no-`base-*` disjuntos + `defensores_directorio` con solapamiento:** corresponde al escenario real de restauración `.lokus` en un PC nuevo antes de copiar memoria de IA (`.lokus` copia `.xlsx` y `uso_catalogos` —conservando `sembrado_en` y defensores— pero **omite** `ia_aprendizaje.json`, de modo que las correcciones posteriores generan IDs `corr-*`/`regla-*` disjuntos). Se clasifica explícitamente como **sospecha de clon `.lokus`**, se **divide** con advertencia usando `v4:clon:<sembrado_hash>:<ids_o_def_hash>` y se exige `--fuente <ALIAS>` para fusionar/confirmar si realmente corresponde a la misma instalación.
     4. **Sin IDs de IA todavía (`reglas` y `correcciones` locales vacías en alguno o ambos paquetes) + mismo `sembrado_en`:** **NO** sobrescribir a ciegas el snapshot previo sin `--fuente` o verificación estricta de compatibilidad monotónica (mismos defensores y conteos de `uso_catalogos` monotónicamente no decrecientes respecto al snapshot anterior); si no se puede confirmar monotonía compatible, emitir advertencia de posible clon `.lokus` y dividir (`v4:clon:<sembrado_hash>:<ids_o_def_hash>`) o requerir `--fuente`.
   - **Ancla 3 (fallback sin `sembrado_en`):** si `sembrado_en` está vacío (instalación sin planillas), usar el conjunto de IDs no-`base-*` o la firma de `defensores_directorio`; si todo está vacío, requerir `--fuente` o generar `v4:anon:<sha256_prefijo>`.

#### B. Contrato evolutivo `formato: 5` (`src-tauri/src/aprendizaje.rs` y `uso_catalogos.rs`)
Para eliminar heurísticas en futuras entregas sin romper compatibilidad con `formato: 4`:
- `VERSION_FORMATO: u32 = 5` en `aprendizaje.rs`.
- `PaqueteExportacion` deriva `#[derive(Debug, Clone, Serialize, Deserialize)]` con `#[serde(default)]` en todos los campos nuevos y opcionales, de modo que **deserialice nativamente tanto `formato: 4` como `formato: 5`**.
- **Campos aditivos en `PaqueteExportacion` (`formato: 5`, todos `#[serde(default)]`):**
  - `instancia_id: String`: copiado desde `UsoCatalogos.instancia_id` (`uso_catalogos.rs:46`).
  - `exportacion_seq: u64`: contador monotónico persistido en `MemoriaAprendizaje` (`exportacion_seq`, incrementado en cada exportación exitosa).
  - `exportacion_id: String`: identificador único de entrega (`format!("exp-{}-{}", timestamp_compacto, exportacion_seq)`).
  - `ultima_exportacion_previa: String`: marca RFC3339 de la exportación inmediatamente anterior (cadena vacía en la primera).
  - `fuentes_importadas: Vec<String>`: copiado desde `UsoCatalogos.fuentes_importadas` (`uso_catalogos.rs:58`, propagando transitivamente `paq.fuentes_importadas` en `M2`) para bloquear doble conteo tras restauraciones `.lokus`.
  - `tabla_alias_raw: Vec<AliasEstadistico>`: alias completos de `memoria.tabla_alias` (anonimizados con `CacheCatalogoAnon`), conservando `alpha`, `beta`, `defensores_confirmantes` y `ultima_actualizacion`, incluyendo candidatos con `q10 < 0.30`.
  - `preferencias_raw: Option<PreferenciasEstilo>` y `preferencias_por_defensor_raw: BTreeMap<String, PreferenciasEstilo>`: pares Beta `(alpha, beta)` reales para distinguir preferencias intactas de evidencia acumulada.
  - `ejemplos_aciertos: Vec<EjemploPositivo>`: anclas positivas de `memoria.ejemplos_aciertos` re-anonimizadas con el `Anonimizador` global.
- **Corrección del orden de persistencia post-escritura (`exportar_aprendizaje_completo_con_priors_en`, `aprendizaje.rs:8049–8111`):**
  - Calcular `nueva_seq = memoria.exportacion_seq + 1` y `nuevo_exportado = Local::now().to_rfc3339()`, poblar `paquete` y serializar `serde_json::to_string_pretty(&paquete)` **en memoria** antes de mutar `memoria`.
  - Escribir el archivo destino (`fs::write(&destino, json)`).
  - **Únicamente si `fs::write` tiene éxito**, actualizar `memoria.exportacion_seq = nueva_seq`, `memoria.ultima_exportacion_previa = nuevo_exportado`, marcar `c.exportada = true` y persistir `guardar_memoria_en(ruta, &memoria)?`, seguido de `marcar_uso_compartido_en`. Así, un fallo de disco nunca quema números de secuencia ni marca correcciones como exportadas.

---

## 4. Política campo por campo de retención, agregación, compactación, cuarentena y poda

La regla rectora de retención distingue entre:
- **El Ledger del Mantenedor (`ingesta_ledger.json`)**, que conserva el registro auditable completo por fuente e IDs históricos (incluyendo IDs retirados/descartados para no re-aceptarlos ni perder trazabilidad).
- **La Base Embebida de la App (`aprendizaje_base.json`)**, que se carga en memoria en cada arranque y **entrena a `MotorEstadisticoLocal` en cada extracción de IA (`ai.rs:2385`)**, por lo que debe mantenerse **compacta, diversa, libre de PII/artefactos `[PROCESADO]` y con reglas `base_invariante` intocables**.

### 4.1 Contrato estricto de protección `base_invariante`

Cualquier regla ya residente en `aprendizaje_base.json` que cumpla:
```rust
r.origen == "base_invariante" || r.id.starts_with("base-")
```
**NO PUEDE BAJO NINGUNA CIRCUNSTANCIA** ser eliminada, degradada a `"retirada"` o `"cuarentena"`, fusionada con otra regla, recortada en sus `triggers` ni modificada en su texto o parámetros `(alpha, beta)`. Adicionalmente, para cerrar la vía de *spoofing* desde archivos externos no confiables y el sombreado en runtime:
1. **Rechazo/cuarentena de entrada externa en `ingesta.rs` (`M3`):** toda regla entrante proveniente de un JSON exportado por usuario que declare `r.id.starts_with("base-")` o `r.origen == "base_invariante"` debe ser **rechazada y enviada a cuarentena de inmediato** (nunca incorporada a `aprendizaje_base.json` ni al conjunto de IDs confiables de usuario en el ledger, evitando colisiones de ID o suplantación de inmunidad `base_invariante`).
2. **Chequeo post-compactación con rollback:** `ingesta.rs` debe verificar al final de la compactación que todas las reglas `base_invariante` previas de `--base` permanecen idénticas (fallando con código distinto de cero y sin escribir `--base` si alguna desapareció o cambió).
3. **Exención de sombreado en tiempo de ejecución (`M2`/`M4`):** en `seleccionar_contexto_detallado_para_defensor` (`aprendizaje.rs`, `HEAD:2812–2831` / `WT:2942–2960`), ninguna regla local no `"activa"` (en especial `"retirada"` o `"cuarentena"`) puede sombrear una regla `base_invariante` de `aprendizaje_base.json`.

### 4.2 Matriz completa campo por campo (`formato: 4` y `formato: 5`)

| # | Campo (`PaqueteExportacion`) | Destino | Política | Reglas concretas, umbrales y criterios de cuarentena / poda |
|---:|---|---|---|---|
| 1–4 | `app`, `version_app`, `formato`, `exportado` | `ingesta_ledger.json` | **Conservar** en bitácora | Validar `app == "Lokus"` y `formato <= 5` (rechazar `formato > 5` o `exportado` inválido). Registrar `(source_id, sha256, exportado, version_app)`. |
| — | `instancia_id`, `exportacion_seq`, `exportacion_id`, `ultima_exportacion_previa`, `fuentes_importadas` (`v5`) | `ingesta_ledger.json` | **Conservar** como claves de fuente | Llave primaria de deduplicación, orden monotónico y grafo de exclusión `.lokus`. |
| 5–15, 18 | `aciertos`, `total_*`, `llamadas_*`, `correcciones_por_campo`, `metricas_por_campo`, `metricas_por_modelo`, `defensores_directorio` | `ingesta_ledger.json` | **Agregar por snapshot** (último por fuente, suma entre fuentes) | **No escribir en `aprendizaje_base.json`** (no los usa en tiempo de ejecución; `base.aciertos` permanece `0`). Recalcular `precision_media` ponderada desde $\sum \text{aciertos} / \sum \text{evaluaciones}$. |
| 16–17 | `preferencias_estilo`, `preferencias_por_defensor` (+ `*_raw` en `v5`) | `ingesta_ledger.json` + `aprendizaje_base.json` (solo `"*"`) | **Compactar / Proteger contra defaults** | **No** copiar preferencias personales de un defensor al perfil global `"*"` de `base`. En `v4`, ignorar valores exactamente iguales al default intacto (`0.75 / 0.75 / 0.5 / 0.2857142857142857`). En `v5`, agregar pares crudos por consenso entre `>= 2` fuentes distintas acotando ESS con `MAX_ESS_PREF = 12.0` (`estadistica_ia.rs:667`). |
| 19 | `resumen_patrones_generales` | Ninguno | **Descartar** | Vista derivada Top-10/12 (`aprendizaje.rs:7351–7406`), 100% reconstruible desde `uso_catalogos` + `grafo_asociaciones` + `correcciones`. |
| 20 | `grafo_asociaciones` | `ingesta_ledger.json` (y futuro `grafo_base` / reporte) | **Agregar por fuente, Cuarentena y Poda** | Reemplazar por `source_id`, sumar entre fuentes; si `inst-A.fuentes_importadas` contiene `inst-B`, aplicar detección de solapamiento sin resta ciega y emitir `"overlap_con_crecimiento_posterior"` (§3.2.4). **Cuarentena:** cualquier arista con `[PROCESADO]`, `[RADICADO]`, `[TELEFONO]`, `[NUMERO]` o typos conocidos (`Maunicipal`, `Promiscuo Segundo Promiscuo`). **Poda:** descartar aristas de peso `1` no confirmadas por `>= 2` fuentes para cualquier eventual prior en frío. |
| 21 | `perfiles_sinteticos` | `ingesta_ledger.json` + `aprendizaje_base.json` (`"*"` curado) | **Compactar y Cuarentena** | Conservar la última `(version, ultima_actualizacion)` por `(source_id, ambito)`. **Cuarentena:** rechazar resúmenes con `[PROCESADO]`/`[RADICADO]` (ej. `"no completes ese [PROCESADO]"`). Promover a `base` solo resumen `"*"` validado (`<= 320` chars). |
| 22 | `consolidaciones_ia` | `ingesta_ledger.json` + `ingesta_reporte.md` | **Unión de `sugerencias_build`; Descartar `telemetria_disparo`** | Unir por `id` (`cons-*`) para retener historial expulsado tras `MAX_CONSOLIDACIONES_HISTORIAL = 12`. Extraer y deduplicar `sugerencias_build` (`categoria`, `detalle`, `evidencia`) enviando a cuarentena las contaminadas con `[PROCESADO]`. **Descartar** `telemetria_disparo` (`presion`, `muestra_rng`, etc.). |
| 23 | `alias_validados` (+ `tabla_alias_raw` en `v5`) | `aprendizaje_base.json` (`tabla_alias`) + `ingesta_ledger.json` | **Compactar, Cuarentena y Poda** | **Poda:** eliminar auto-identidades (`normalizar_clave_entidad(entrada) == normalizar_clave_entidad(canonico)`), fragmentos genéricos sin entidad (`"1 municipal"`, L1386) y `q10 < 0.30`. **Cuarentena:** alias con `[PROCESADO]` (ej. `San [PROCESADO]`) o typos de catálogo. En `v4`, reconstruir `(alpha, beta)` conservador fijando `beta = PRIOR_ALIAS_BETA = 1.2` y resolviendo `alpha` para `confianza_q10` (con tope `MAX_ESS_N = 40.0`). Promover a `"*"` en `base` solo si `>= 2` fuentes o `>= 2` defensores confirman el mismo canónico sin conflicto; si es específico de un solo defensor/región sin conflicto, conservar con su `defensor_alcance` o como evidencia de patrón. |
| 24 | `patrones_abreviacion` | `ingesta_reporte.md` | **Conservar como evidencia en reporte** | En tiempo de ejecución se re-inducen dinámicamente desde `tabla_alias` (`patrones_abreviacion.rs:1607`). Conservar los de alta consistencia (`consistencia >= 0.90`, `ejemplos_distintos >= 2`) en el reporte de curación para nutrir reglas de `ai.rs` y validar `tabla_alias`. |
| 25–27 | `juzgados_agregados_usuario`, `tipos_audiencia_agregados_usuario`, `uso_catalogos` | `ingesta_ledger.json` + `ingesta_reporte.md` (+ propuesta a `juzgados_base.json`/`models.rs`/`constants.js`) | **Agregar por fuente, Cuarentena y Poda** | Reemplazar snapshot por `source_id`, sumar entre fuentes distintas aplicando deducción acotada al baseline absorbido (o preservando el delta posterior + bandera `"overlap_con_crecimiento_posterior"` si no hay cota segura, §3.2.4). **Regla de 0 usos:** calcular `predeterminados_sin_uso` globales **únicamente** donde la suma entre **TODAS** las fuentes sea `0` (nunca podar catálogos regionales desde una sola instalación). **Cuarentena:** entradas con `[PROCESADO]`/`[RADICADO]`/`[TELEFONO]`/`[NUMERO]`, basura (`"fdsafeasdfe"`), typos (`"Maunicipal"`, `"Promiscuo Segundo Promiscuo"`) o variantes sin tildes/preposiciones. **Poda:** entradas `"otra"` con `conteo <= 1` salvo aparición en `>= 2` fuentes. |
| 28 | `reglas` | `aprendizaje_base.json` + `ingesta_ledger.json` | **Preservar `base_invariante`; Compactar y Poda** | **Rechazar / enviar a cuarentena** cualquier regla entrante de usuario con `id.starts_with("base-")` o `origen == "base_invariante"` (*anti-spoofing*). Unión por `id` del resto. **Cuarentena:** reglas cuyo texto contenga `[PROCESADO]` o contradigan una `base_invariante`. **Poda:** descartar de `base` reglas con `estado == "retirada"` o `q10 < 0.30` (conservando su `id` en `ledger.seen_rule_ids`). **Compactación:** deduplicar semánticamente con `es_misma_regla` (`aprendizaje.rs:2436`), acotar ESS a `MAX_ESS_N = 40.0` (`69`), limpiar `triggers` ruidosos (fechas, años, códigos DANE, números de 1–2 dígitos, muletillas) y corregir `campo_objetivo` si es inconsistente con `tipo_error`. |
| 29 | `correcciones` | `aprendizaje_base.json` (subconjunto representativo) + `ingesta_ledger.json` | **Compactar (BM25 + MMR), Cuarentena y Poda** | Unión por `id` en ledger. **Poda:** descartar de `base` las `estado == "descartada"` (el `id` queda en ledger). **Cuarentena:** separar de `base` si `final_usuario.juzgado` o `final_usuario.tipo` contiene `[PROCESADO]` (ej. `San [PROCESADO]`) o basura. **Compactación:** seleccionar solo `estado == "analizada"` representativas y diversas usando BM25 + MMR (`1–2` por `(campo, firma_error)`, tope `~30–60` en `aprendizaje_base.json`) para mantener veloz el reentrenamiento de `MotorEstadisticoLocal` en `ai.rs:2385`. |
| — | `ejemplos_aciertos` (`v5`) | `aprendizaje_base.json` + `ingesta_ledger.json` | **Compactar (BM25 + MMR) y Cuarentena** | Unión por `id`, cuarentena si `salida_anonimizada.juzgado`/`tipo` tiene `[PROCESADO]`, selección diversa BM25 + MMR con tope `<= 30` anclas positivas en `base`. |

---

## 5. Hallazgos concretos y hoja de ruta `R5` basados en el JSON real (`media_1795447850839_b6e8f4e8.json`, `formato: 4`)

### 5.1 Resumen cuantitativo auditado del JSON real

La inspección línea por línea de las **5.203 líneas** del archivo real (`app: "Lokus"`, `version_app: "5.1.0"`, `formato: 4`, `exportado: "2026-11-23T10:29:16.716664800-05:00"`) arroja las siguientes cifras exactas:

- **Volumen operativo:** `141` audiencias contabilizadas en `uso_catalogos` (`sembrado_en: "2026-11-19T11:11:25.345266200-05:00"`, `ultima_actualizacion: "2026-11-23T10:20:19.938821500-05:00"`), `108` evaluaciones de IA (`71` `aciertos` + `37` `correcciones`, precisión global `65.74%` con modelo `openai:gpt-6-luna`), `99` llamadas de extracción, `11` de inducción y `6` de consolidación.
- **Desglose de `correcciones_por_campo` (`58` ediciones de campo sobre `37` correcciones):** `observaciones: 24`, `juzgado: 16`, `motivo: 12`, `radicado: 2`, `tipo: 2`, `fecha: 1`, `realizada: 1`, `hora: 0`.
- **Desglose exacto de las `37` `correcciones` (líneas `3579–5202`):**
  - Por marca `exportada`: **`22` con `exportada: true`** (9 `corr-20261119*` sin `defensor_clave` + 13 de `ivan cordoba`) y **`15` con `exportada: false`** (3 de `ivan cordoba` + 12 de `david felipe ariza perez`).
  - Por `estado`: **`19` `analizada`** (`11` `exportada: true`, `8` `exportada: false`), **`16` `pendiente`** (`9` `exportada: true`, `7` `exportada: false`) y **`2` `descartada`** (`corr-20261122215318750-16` en L4179, `corr-20261122215516506-19` en L4220).
- **Desglose de `reglas` (`13`, líneas `3084–3578`):** `10` `mitigacion_modelo` + `3` `consolidacion_ia`; por estado: `9` `activa`, `3` `prueba`, `1` `retirada`. (`base_invariante` no aparece aquí porque se fusiona en runtime desde `aprendizaje_base.json`).
- **Otros artefactos:** `33` `alias_validados` (`21` de `juzgado`, `12` de `tipo`), `2` `patrones_abreviacion`, `6` `consolidaciones_ia` (todas con `sugerencias_build`), `3` `perfiles_sinteticos` (`"*"`, `david felipe ariza perez`, `ivan cordoba`), y `4` defensores en `defensores_directorio` (`camilo pino`: 0 evals IA / 32 audiencias en grafo; `david felipe ariza perez`: 41/12 = `77.36%`; `ivan cordoba`: 16/16 = `50.0%`; `julio cesar rojas daza`: 14/0 = `100.0%`; los `9` restantes de `108` son las correcciones tempranas con `defensor_clave: ""`).

### 5.2 Hoja de ruta `R5` (Sub-hitos `R5-A` .. `R5-E`) con evidencia exacta de código

#### `R5-A` — Endurecimiento del `Anonimizador` (`src-tauri/src/aprendizaje.rs`)
- **Evidencia en JSON:**
  - `"Juzgado 1 Promiscuo Municipal de San [PROCESADO]"` aparece en `juzgados_agregados_usuario` (L1418), `uso_catalogos` (L1723), `grafo_asociaciones` (L577, L781 bajo DANE `20710` = San Alberto, Cesar) y `alias_validados` (L1198); también `"San [PROCESADO] - [PROCESADO]"` en L4224 (donde incluso `"Cesar"` resultó enmascarado tras `"San Alberto - Cesar"`).
  - Falsos positivos en palabras comunes del dominio judicial y de meta-texto: `"PORTE DE [PROCESADO] DE FUEGO"` (L3672, L3999, L4183, L4608, L4654), `"una [PROCESADO] de 55 meses"` (L4183), `"ese [PROCESADO]"` (`"campo"`, L909, L935, L1089) y `"las [PROCESADO]"` (`"fuentes"`/`"siglas"`, L1089).
- **Causa raíz en código:**
  - `Anonimizador::registrar_frase_y_palabras` (`aprendizaje.rs:4134–4162`) marca todo token individual `>= 3` chars que pase `es_token_anonimizacion_valido` (`3582–3587`) dentro de `es_palabra_procesado[id] = true` (`4159`), y el reemplazo de palabra suelta (`4451–4462`) sustituye cualquier ocurrencia aislada.
  - `es_termino_estructural_judicial` (`3504–3580`) incluye `"penas"` (L3525), pero omite `"pena"`, `"arma"`, `"armas"`, `"fuego"`, `"campo"`, `"campos"`, `"fuente"`, `"fuentes"`, `"sigla"`, `"siglas"`.
  - `CacheCatalogoAnon::anonimizar` / `anonimizar_valor_catalogo` (`7042–7053`) solo exime **coincidencias exactas** contra `juzgados_base.json` y `TIPOS_AUDIENCIA`. Como `juzgados_base.json:33` dice `"Juzgado Promiscuo Municipal de San Alberto"` (sin `"1"`), `"Juzgado 1 Promiscuo Municipal de San Alberto"` no queda exento y el token `"alberto"` (registrado por algún procesado homónimo) se reemplaza por `[PROCESADO]`.
- **Acción `R5-A`:**
  1. Ampliar `es_termino_estructural_judicial` (`3504–3580`) con los términos judiciales/técnicos faltantes (`"pena"`, `"arma"`, `"armas"`, `"fuego"`, `"campo"`, `"campos"`, `"fuente"`, `"fuentes"`, `"sigla"`, `"siglas"`, etc.).
  2. Proteger topónimos municipales en contexto judicial (`de San <X>`, `de Santa <X>`, `de La <X>` como `La Jagua de Ibirico`, `del <X>` como `del Paso` / `San Juan del Cesar`, `de El <X>` como `El Banco` / `El Copey`, municipios de `juzgados_base.json` / DANE) y nombres completos de despachos cuando la frase coincida estructuralmente como juzgado (p. ej. prefijo `"juzgado ... de <municipio>"`) en lugar de reemplazar un solo token de pila (`Alberto`, `Cesar`, `Paso`, `Banco`, `Jagua`) dentro de un nombre de despacho o meta-texto en minúscula de `perfiles_sinteticos`/`sugerencias_build`.

#### `R5-B` — Corrección de `campo_objetivo` y limpieza de `triggers` (`src-tauri/src/aprendizaje.rs`)
- **Evidencia en JSON:**
  - `regla-20261119114551117-16` (L3086): `tipo_error: "estilo_observaciones"`, pero `campo_objetivo: "juzgado"` y `triggers: ["23","10","2026","2",...]`.
  - `regla-20261122214447804-8` (L3158): `tipo_error: "motivo"`, pero `campo_objetivo: "tipo"` y `triggers: ["2","j2pca","continuacion",...]`.
  - 10 de las 13 reglas contienen en `triggers` años (`"2026"`), códigos DANE (`"20001"`), números cortos (`"1"`, `"2"`, `"8"`, `"13"`, `"18"`, `"21"`, `"22"`, `"23"`, `"25"`), meses (`"septiembre"`) o muletillas (`"proceso"`, `"bajo"`, `"radicado"`).
- **Causa raíz en código:**
  - En `aplicar_analisis_en` (`6541–6545`) y `aplicar_analisis_cluster_ia` (`6896–6900`), `campo_obj` antepone `corr.campos.first()` a `inferir_campo_objetivo_desde_tipo(&analisis.tipo_error)` (`933–945`). Cuando una corrección editó primero `juzgado` o `tipo`, la regla inducida para `observaciones` o `motivo` hereda un `campo_objetivo` erróneo, distorsionando la atribución de crédito (`4886–4963`).
  - `regenerar_triggers_correccion` (`5576–5591`) y `extraer_triggers_texto` (`2456–2471`) consumen `tokenizar_judicial` (`2197–2225`), que solo colapsa dígitos de `len >= 6` a `"_rad_"` (`2213`), dejando pasar años, DANE de 5 dígitos y números sueltos de 1–2 dígitos.
- **Acción `R5-B`:**
  1. Dar prioridad a `inferir_campo_objetivo_desde_tipo(&analisis.tipo_error)` (usando `corr.campos` solo si coincide o cuando el tipo sea `"general"`).
  2. Filtrar en `extraer_triggers_texto` / `tokenizar_judicial` tokens puramente numéricos de 1–5 dígitos no ordinales de despacho, años `2018..=2050`, nombres de meses y muletillas genéricas (`"proceso"`, `"bajo"`, `"radicado"`, `"dia"`).

#### `R5-C` — Desbloqueo del prior muerto `incluir_reprogramacion` y herencia de preferencias (`src-tauri/src/estadistica_ia.rs` y `aprendizaje.rs`)
- **Evidencia en JSON:**
  - En `preferencias_estilo` y `preferencias_por_defensor` (L106–131), `incluir_reprogramacion_prob` está congelado en **`0.75`** (`3.0 / (3.0 + 1.0)`) en todos los defensores, pese a existir **10 correcciones reales** donde la IA incluyó fechas de reprogramación en lenguaje natural (`"Se fijó nueva fecha..."`, `"Próxima fecha: ..."`, `"Se señaló nuevamente..."`, `"Se está a la espera de que se fije nueva fecha"` — p. ej. `corr-20261119113814711-6`..`215757254-21`) y el usuario las eliminó o limpió.
- **Causa raíz en código:**
  - `actualizar_preferencias_en_bloque` (`estadistica_ia.rs:691–709`) únicamente actualiza `incluir_reprogramacion_alpha/beta` cuando `pred_obs.contains("reprogramada para:")` (y análogo literal `"causas secundarias:"` en `671–689`). Como `ai.rs:2323–2337` no añade ese prefijo si la fecha ya venía redactada por el LLM, `incluir_reprogramacion_beta` **nunca** se incrementa, y `ai.rs:937` (Regla 7) sigue obligando a incluir reprogramaciones.
  - Adicionalmente, en `preferencias_efectivas_con_base_para_defensor` (`aprendizaje.rs`, `HEAD:1099–1101` / `WT:1129–1131`), `if let Some(p_loc) = local_def { return p_loc.clone(); }` retorna la estructura completa del defensor apenas existe una entrada en `local.preferencias_por_defensor`, bloqueando la herencia desde `base.preferencias_estilo` y `base.preferencias_por_defensor` en los campos que el defensor local aún conserva en su valor `Default` (p. ej. `incluir_reprogramacion`).
- **Acción `R5-C`:**
  1. Detectar contenido semántico de reprogramación/nueva fecha en `pred_obs` y `fin_obs` (presencia de `reprogramada para:`, `proxima fecha`, `se fijo nueva fecha`, `se reprogramo para`, `fecha para continuar` o fecha extraída por patrón) para actualizar correctamente `incluir_reprogramacion_alpha/beta` y adaptar el prompt/postprocesador por defensor.
  2. Corregir `preferencias_efectivas_con_base_para_defensor` (`aprendizaje.rs`) para que mezcle **cada uno de los 4 pares `(alpha, beta)` individualmente** contra `Default` con precedencia `local_def > base_def > local_gen > base_gen > Default`, garantizando que las actualizaciones en `aprendizaje_base.json` alcancen efectivamente a defensores existentes con perfiles parciales.

#### `R5-D` — Correcciones de extracción y normalización en `src-tauri/src/ai.rs`
- **Evidencia en JSON y causa raíz verificada en `ai.rs`:**
  1. **`"no notificaron al procesado"` → `"Juez"` (en lugar de `"Usuario"`):** en `ai.rs:345` y `929`, `"no notificaron al procesado"` está asignado a `"Usuario"`. En el JSON real, **4 correcciones** (`corr-115232583-21`, `115356626-24`, `115444578-26`, `115507586-29`), `errores_recurrentes_transversales` (`motivo:usuario->juez`, 4×), `regla-20261119115511987-30` (L3123) y `consolidaciones_ia[0]` (L929) confirman que la falta de notificación al procesado es atribuible al despacho (`"Juez"`). *Restricción de prueba:* el test `estadistica_ia.rs:1938` (`"Procesado no compareció y no notificaron al usuario"`) debe seguir clasificando `"Usuario"` por `"procesado no comparecio"` (`ai.rs:346`).
  2. **`"MPAL"` vs `"CTO"`:** `ai.rs:935` prohíbe expandir `"CTO"`, `"MPAL"` y `"PCTO"`. La evidencia real confirma que `"MPAL"` es unívocamente `"Municipal"` (`regla-20261123003455351-6` con `49/51` aciertos; `patrones_abreviacion[0]` `"juzgado {N} penal mpal"` → `"Juzgado {N} Penal Municipal"` con `9/9`, consistencia `1.0`; `consolidaciones_ia[4],[5]`). En `M4` debe retirarse `"MPAL"` de la prohibición en `ai.rs:935` (manteniendo `"CTO"` y `"PCTO"`, preservando `base-abreviacion-cto` como `base_invariante` y respetando el override prequential de `"Juzgado 4 MPAL"` probado en `ai.rs:3009–3052`).
  3. **Expansión `"EMP"`:** agregar `(r"(?i)\bEMP\b", "elementos materiales probatorios")` en `expandir_abreviaciones_y_compacto` (`ai.rs:123–175`), respaldado por `corr-20261123081203019-61` (L5038) y `consolidaciones_ia[5].sugerencias_build` (L1140–1141).
  4. **Ajuste de `detectar_defensor_confianza` (`ai.rs:204–225`):** las subcadenas genéricas `"el abogado"` (`210`), `"su abogado"` (`211`) y `"abogado defensor"` (`213`) disparan falsos positivos de `"Defensor Confianza"` cuando el texto habla del propio defensor público o de un abogado sin connotación contractual (`corr-20261122214306987-6`, `regla-20261122214447804-8`, `regla-20261122223823639-85`). Se deben retirar las tres (`"el abogado"`, `"su abogado"` y `"abogado defensor"`), manteniendo los indicadores específicos de representación privada/contractual (`"abogado del procesado"`, `"abogado contractual"`, `"abogado privado"`, `"defensor de confianza"`, `"def. confianza"`, etc.), para cumplir exactamente con `base-defensor-confianza-contractual` y el test `ai.rs:2478`.
  5. **Ceros a la izquierda en juzgados (`models.rs:256–290`, `ai.rs:500–522`):** entradas como `"JUZGADO 03 PENAL MPAL"`, `"Juzgado 06 Penal Municipal"` (`corr-20261123082640745-104`, L5162–5193), `"JUZGADO 08 PENAL MPAL"` y `"JUZGADO 09 PENAL MPAL"` no coinciden con `"Juzgado 6 Penal Municipal"` porque `normalizar_clave_catalogo` no elimina ceros a la izquierda en numerales de despacho. Normalizar enteros iniciales de despacho (`06` → `6`) al comparar/emparejar juzgados.
  6. **Prioridad de frases específicas en `mapear_tipo_audiencia_con_usuario` (`ai.rs:430–498`):** `mapeo_directo` colapsa `"continua juicio"` → `"Audiencia de juicio oral"` (`459`) y `"acusacion"` → `"Audiencia de acusación"` (`462`) mediante `norm.contains(clave)` antes de distinguir `"continuacion de juicio oral"` (6 usos) o `"formulacion de acusacion"` (10 usos, `corr-20261122222125823-72`). Tras incorporar esos tipos al catálogo oficial, se deben evaluar las frases específicas primero y no colapsarlas al tipo genérico.

#### `R5-E` — Sincronización de catálogos, `HORAS_RAPIDAS` y enriquecimiento de `aprendizaje_base.json`
1. **Juzgados (`src-tauri/resources/juzgados_base.json` + `src/lib/constants.js`):**
   - **Incorporar 19 juzgados validados** de uso real (ordenados por frecuencia en la muestra):
     - `Juzgado 9 Penal Municipal` (`13`), `Juzgado 11 Penal Municipal` (`12`), `Juzgado 2 Promiscuo del Circuito de Aguachica` (`9`), `Juzgado 10 Penal Municipal` (`7`), `Juzgado 13 Penal Municipal` (`7`), `Juzgado 3 Penal del Circuito con Función de Conocimiento Aguachica` (`7`), `Juzgado 1 Promiscuo Municipal de San Alberto` (`7`, desanonimizado desde `San [PROCESADO]` vía DANE `20710`), `Juzgado 10 Penal del Circuito con Función de Conocimiento de Valledupar` (`5`), `Juzgado 8 Penal Municipal` (`5`), `Juzgado 1 Promiscuo del Circuito de Aguachica` (`4`), `Juzgado 5 Penal Municipal` (`4`), `Juzgado Promiscuo Municipal del Paso` (`4`), `Juzgado 6 Penal Municipal` (`3`), `Juzgado 8 Penal del Circuito con Función de Conocimiento de Valledupar` (`3`), `Juzgado 9 Penal del Circuito con Función de Conocimiento de Valledupar` (`3`), `Juzgado Promiscuo Primero Municipal de Chiriguaná` (`2`), `Juzgado 3 Penal del Circuito Especializado de Valledupar` (`1`), `Juzgado 4 Penal del Circuito Especializado de Valledupar` (`1`), `Juzgado 7 Penal Municipal` (`1`), además de evaluar la inclusión/estandarización de `Tribunal Penal de Valledupar` y resolver la colisión singular/plural **`"Función"` vs `"Funciones"`**: en `media_1795447850839_b6e8f4e8.json:558–559` coexisten `"Juzgado 5 Penal del Circuito con Funciones de Conocimiento de Valledupar"` (`juzgados_base.json:14`, 1 uso) y `"Juzgado 5 Penal del Circuito con Función de Conocimiento de Valledupar"` (1 uso), mientras que los juzgados 8–10 en el JSON real usan `"Función"` (`juzgados_base.json:10–14` usa `"Funciones"`). Como `normalizar_clave_catalogo` no pliega `"funcion"`/`"funciones"`, `juzgados_base.json` / `normalizar_juzgado_catalogo` (o `normalizar_clave_catalogo`) y la curación de `ingesta.rs` deben canonizar `"Función"`/`"Funciones"` o emitir advertencia explícita de casi-duplicado para evitar entradas redundantes.
   - **Rechazar / poner en cuarentena (nunca agregar al catálogo base):** `"fdsafeasdfe"` (`1`), `"Juzgado 10 Penal Maunicipal"` (`1`), `"Juzgado Promiscuo Segundo Promiscuo de Chiriguana"` (`1`), `"Juzgado 7 Penal de Circuito con Funcion de Conocimiento"` (`3`, falta `"del"`, `"de Valledupar"` y tilde) y cualquier variante con `[PROCESADO]`.
   - **Proteger los `84` juzgados predeterminados con `0` usos** (Magdalena y La Guajira), ya que corresponden a otra subregión ausente en esta muestra del Cesar.
2. **Tipos de audiencia (`TIPOS_AUDIENCIA` en `models.rs:15–34` y `src/lib/constants.js:1–20`):**
   - Incorporar (con ortografía y tildes normalizadas) los **9 tipos validados** por uso real: `"Aplicación principio de oportunidad"` (`7`), `"Audiencia de celebración de preacuerdo"` (`2`), `"Audiencia de continuación de juicio oral"` (`6`), `"Audiencia de formulación de acusación"` (`10`), `"Audiencia de lectura de decisión"` (`1`), `"Audiencia de libertad por vencimiento de términos"` (`1`), `"Audiencia de sentido de fallo"` (`4`), `"Audiencia de verificación de allanamiento a cargos"` (`3`) y `"Sentencia"` (`1`), manteniendo `"Otra"` al final y preservando los 9 predeterminados con `0` usos.
3. **`HORAS_RAPIDAS` (`models.rs:36–43` y `src/lib/constants.js:33–40`):**
   - `16:00` (`4:00 PM`) registró **`0` usos** (`horas_rapidas_sin_uso: ["16:00"]`, L2713–2722), mientras **`07:30` (`7:30 AM`) empató en el 1.º puesto global con `8` usos** (`5.67%`, L2725–2730), seguido de `08:30` (`6`), `14:30` (`6`), `11:00` (`5`) y `10:30` (`4`).
   - Reemplazar `"16:00"` por `"07:30"` (`"7:30 AM"`) en `HORAS_RAPIDAS` (o ampliar la lista rápida incluyendo `07:30`, `08:30`, `14:30` según diseño UI, manteniendo paridad exacta entre `models.rs` y `constants.js`). Revisar además en `ai.rs` el tratamiento de `"12: 00 m."` / mediodía frente a las anomalías `00:00` (`2`) y `02:00` (`1`).
4. **Enriquecimiento de `src-tauri/resources/aprendizaje_base.json` (ejecutando `lokus_ingestar` en `M4`):**
   - Conservar intactas las 3 reglas `base_invariante` y las 2 correcciones base originales.
   - Incorporar las reglas curadas limpias (sin `[PROCESADO]`, con `campo_objetivo` y `triggers` depurados), alias validados limpios (`j2pca`, `j3pca`, `j1pca`, `j2pma`, `j1pma`, etc., excluyendo `"1 municipal"` y `San [PROCESADO]`) y correcciones representativas compactadas vía BM25 + MMR para reducir la tasa de error inicial en nuevos usuarios desde el primer día.
