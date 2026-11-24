<script>
  import {
    SlidersHorizontal,
    X,
    KeyRound,
    Brain,
    Eye,
    EyeOff,
    RefreshCw,
    AlertTriangle,
    ChevronDown,
    Sparkles,
    Trash2,
    BookOpen,
    Lightbulb,
    Gauge,
  } from "@lucide/svelte";
  import { MODELOS_GEMINI, MODELOS_OPENAI } from "../constants.js";

  let {
    settings = $bindable(),
    aiStatus,
    nuevaOpenAiKey = $bindable(""),
    nuevaGeminiKey = $bindable(""),
    mostrarOpenAiKey = $bindable(false),
    mostrarGeminiKey = $bindable(false),
    diagnosticoIa = null,
    cargandoDiagnosticoIa = false,
    defensorArchivo = "",
    consolidandoIa = false,
    onRefrescarDiagnosticoIa = null,
    onConsolidarMemoriaIa = null,
    onEliminarHechoIa = null,
    onEliminarItemPlaybookIa = null,
    onGuardarConfigIa,
    onCerrar,
  } = $props();

  let tabActiva = $state("conexion");
  let consolidandoAmbito = $state("");
  let eliminandoId = $state("");
  let tabConexionEl = $state(null);
  let tabAprendizajeEl = $state(null);

  const SECCIONES_PLAYBOOK = [
    { clave: "siglas_y_despachos", titulo: "Siglas y despachos" },
    { clave: "criterios_realizada_motivo", titulo: "Criterios SI/NO y motivos" },
    { clave: "estilo_observaciones", titulo: "Estilo de observaciones" },
    { clave: "trampas_frecuentes", titulo: "Trampas frecuentes" },
  ];

  $effect(() => {
    if (!consolidandoIa) {
      consolidandoAmbito = "";
    }
  });

  const ETIQUETAS_CAMPO = {
    radicado: "Radicado / Procesado",
    tipo: "Tipo de audiencia",
    fecha: "Fecha",
    hora: "Hora",
    juzgado: "Juzgado",
    realizada: "Realizada (SI/NO)",
    motivo: "Motivo de aplazamiento",
    observaciones: "Observaciones",
    general: "General",
  };

  function etiquetaCampoDiagnostico(campo) {
    const clave = String(campo || "").trim().toLowerCase();
    return ETIQUETAS_CAMPO[clave] || campo;
  }

  function obtenerEstadoCampo(m) {
    const evals = Number(m?.evaluaciones ?? 0);
    const ewma = Number(m?.precision_ewma ?? 0);
    const q10 = Number(m?.confianza_q10 ?? 0);

    if (evals === 0) {
      return {
        texto: "Sin datos",
        clase: "is-neutral",
        detalle: "Aún no se han evaluado extracciones en este campo.",
      };
    }
    if (m?.drift_activo) {
      return {
        texto: "En revisión",
        clase: "is-err",
        detalle: "Se detectó una caída reciente en la precisión (alerta de deriva).",
      };
    }
    if (evals < 3) {
      return {
        texto: "Aprendiendo",
        clase: "is-info",
        detalle: "Aún hay pocas evaluaciones para confirmar su fiabilidad.",
      };
    }
    if (ewma < 0.65 || q10 < 0.4) {
      return {
        texto: "Baja",
        clase: "is-err",
        detalle: "La IA suele requerir correcciones en este campo.",
      };
    }
    if (ewma < 0.85 || q10 < 0.65) {
      return {
        texto: "Mejorable",
        clase: "is-warn",
        detalle: "Precisión intermedia; seguirá mejorando con tus correcciones.",
      };
    }
    return {
      texto: "Confiable",
      clase: "is-ok",
      detalle: "Alta precisión sostenida en las últimas extracciones.",
    };
  }

  function textoEstadoMemoria(presion) {
    const p = Number(presion ?? 0);
    if (p >= 1.2) return "Listo para resumir";
    if (p >= 0.5) return "Acumulando cambios";
    return "Memoria al día";
  }

  function formatoReglaEstado(estado) {
    switch (String(estado || "").toLowerCase()) {
      case "activa":
        return { texto: "Activa", clase: "is-ok" };
      case "prueba":
        return { texto: "En prueba", clase: "is-warn" };
      case "cuarentena":
        return { texto: "En pausa", clase: "is-err" };
      default:
        return { texto: "Pendiente", clase: "is-neutral" };
    }
  }

  let reglasActivasTotal = $derived(
    (diagnosticoIa?.reglas_por_estado?.activa ?? 0) +
      (diagnosticoIa?.reglas_por_estado?.prueba ?? 0)
  );

  let totalEvalGlobal = $derived(
    (diagnosticoIa?.aciertos_totales ?? 0) +
      (diagnosticoIa?.correcciones_totales ?? 0)
  );

  let precisionGlobalTexto = $derived(
    totalEvalGlobal > 0
      ? `${Math.round(((diagnosticoIa?.aciertos_totales ?? 0) / totalEvalGlobal) * 100)}%`
      : "Sin datos"
  );

  let totalAbrevAprendidas = $derived(
    (diagnosticoIa?.alias_validados?.length ?? 0) +
      (diagnosticoIa?.patrones_abreviacion?.length ?? 0)
  );

  let preferenciasLista = $derived.by(() => {
    const p = diagnosticoIa?.preferencias_estilo;
    if (!p) return [];
    return [
      {
        label: "Incluir causas secundarias en observaciones",
        pct: Math.round((p.incluir_causas_secundarias_prob ?? 0) * 100),
      },
      {
        label: "Mencionar reprogramación en observaciones",
        pct: Math.round((p.incluir_reprogramacion_prob ?? 0) * 100),
      },
      {
        label: 'Tratar "se fija nueva fecha" como NO realizada',
        pct: Math.round((p.se_fija_aplazamiento_prob ?? 0) * 100),
      },
      {
        label: "Marcar audiencia instalada y reprogramada como SI",
        pct: Math.round((p.instalada_reprogramada_si_prob ?? 0) * 100),
      },
    ];
  });

  function etiquetaAmbitoMemoria(ambito) {
    const a = String(ambito || "").trim();
    if (!a || a === "*") return "General";
    const encontrado = (diagnosticoIa?.defensores_resumen ?? []).find(
      (d) => d.clave === a || d.nombre === a
    );
    return encontrado?.nombre || a;
  }

  let playbookItemsActivos = $derived.by(() =>
    (diagnosticoIa?.playbook_items ?? []).filter((it) => it && it.activo !== false)
  );

  let playbookPorSeccion = $derived.by(() => {
    const items = playbookItemsActivos;
    return SECCIONES_PLAYBOOK.map((sec) => ({
      ...sec,
      items: items.filter((it) => (it.seccion || "trampas_frecuentes") === sec.clave),
    })).filter((g) => g.items.length > 0);
  });

  let hechosActivos = $derived.by(() =>
    (diagnosticoIa?.hechos_semanticos ?? []).filter((h) => h && h.activo !== false)
  );

  let eficienciaIa = $derived.by(() => {
    const t = diagnosticoIa?.telemetria_eficiencia ?? {};
    const extracciones = Number(t.extracciones_con_telemetria ?? 0);
    const tokensEntrada = Number(t.tokens_entrada_total ?? 0);
    const tokensCacheados = Number(t.tokens_cacheados_total ?? 0);
    const autoverifDisparadas = Number(t.autoverificaciones_disparadas ?? 0);
    const autoverifCambios = Number(t.autoverificaciones_con_cambio ?? 0);
    const pctCache =
      tokensEntrada > 0 ? Math.round((tokensCacheados / tokensEntrada) * 100) : null;
    return {
      extracciones,
      tokensEntrada,
      tokensCacheados,
      autoverifDisparadas,
      autoverifCambios,
      pctCache,
    };
  });

  async function manejarEliminarItemPlaybook(itemId) {
    if (!itemId || !onEliminarItemPlaybookIa || eliminandoId) return;
    eliminandoId = itemId;
    try {
      await onEliminarItemPlaybookIa(itemId);
    } finally {
      eliminandoId = "";
    }
  }

  async function manejarEliminarHecho(hechoId) {
    if (!hechoId || !onEliminarHechoIa || eliminandoId) return;
    eliminandoId = hechoId;
    try {
      await onEliminarHechoIa(hechoId);
    } finally {
      eliminandoId = "";
    }
  }

  function manejarKeydownTab(e) {
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      if (tabActiva === "conexion") {
        tabActiva = "aprendizaje";
        tabAprendizajeEl?.focus();
      } else {
        tabActiva = "conexion";
        tabConexionEl?.focus();
      }
    } else if (e.key === "Home") {
      e.preventDefault();
      tabActiva = "conexion";
      tabConexionEl?.focus();
    } else if (e.key === "End") {
      e.preventDefault();
      tabActiva = "aprendizaje";
      tabAprendizajeEl?.focus();
    }
  }
</script>

<div class="modal-header">
  <h2 id="dialog-heading" class="modal-title">
    <SlidersHorizontal size={15} />
    <span>Inteligencia Artificial y Aprendizaje</span>
  </h2>
  <button
    type="button"
    class="mini-icon-btn"
    aria-label="Cerrar"
    onclick={onCerrar}
  >
    <X size={15} />
  </button>
</div>

<div class="modal-body scroller ia-modal-body">
  <div class="ia-modal-tabs" role="tablist" aria-label="Secciones de Inteligencia Artificial">
    <button
      bind:this={tabConexionEl}
      id="ia-tab-conexion"
      type="button"
      role="tab"
      aria-selected={tabActiva === "conexion"}
      aria-controls="ia-panel-conexion"
      tabindex={tabActiva === "conexion" ? 0 : -1}
      class="ia-modal-tab {tabActiva === 'conexion' ? 'active' : ''}"
      onclick={() => (tabActiva = "conexion")}
      onkeydown={manejarKeydownTab}
    >
      <KeyRound size={14} />
      <span>Conexión y ajustes</span>
    </button>

    <button
      bind:this={tabAprendizajeEl}
      id="ia-tab-aprendizaje"
      type="button"
      role="tab"
      aria-selected={tabActiva === "aprendizaje"}
      aria-controls="ia-panel-aprendizaje"
      tabindex={tabActiva === "aprendizaje" ? 0 : -1}
      class="ia-modal-tab {tabActiva === 'aprendizaje' ? 'active' : ''}"
      onclick={() => (tabActiva = "aprendizaje")}
      onkeydown={manejarKeydownTab}
    >
      <Brain size={14} />
      <span>Aprendizaje y memoria</span>
      {#if diagnosticoIa?.drift_campos_activos?.length > 0}
        <span class="ia-diag-pill is-warn">Revisar</span>
      {:else if reglasActivasTotal > 0}
        <span class="ia-tab-badge">{reglasActivasTotal}</span>
      {/if}
    </button>
  </div>

  {#if tabActiva === "conexion"}
    <div
      id="ia-panel-conexion"
      role="tabpanel"
      aria-labelledby="ia-tab-conexion"
      class="ia-tab-panel"
    >
      <section class="ia-config-section">
        <h3 class="ia-section-title">Proveedor y clave de acceso</h3>

        <div class="ia-status-banner">
          {#if aiStatus.configured}
            <span class="ia-diag-pill is-ok">Conectado</span>
            <span class="ia-status-text">
              Activo:
              <strong>{aiStatus.provider === "gemini" ? "Google Gemini" : "OpenAI"}</strong>
              · {aiStatus.model}
            </span>
          {:else}
            <span class="ia-diag-pill is-warn">Sin clave API</span>
            <span class="ia-status-text">
              Configura una clave de OpenAI o Google Gemini para habilitar el autocompletado.
            </span>
          {/if}
        </div>

        <div class="field-group">
          <span class="field-label">Proveedor</span>
          <div class="segmented-control" role="radiogroup" aria-label="Proveedor de IA">
            <button
              type="button"
              role="radio"
              aria-checked={settings.provider === "openai"}
              class="segmented-option {settings.provider === 'openai' ? 'is-si' : ''}"
              onclick={() => (settings.provider = "openai")}
            >
              <span>OpenAI</span>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={settings.provider === "gemini"}
              class="segmented-option {settings.provider === 'gemini' ? 'is-si' : ''}"
              onclick={() => (settings.provider = "gemini")}
            >
              <span>Google Gemini</span>
            </button>
          </div>
        </div>

        {#if settings.provider === "openai"}
          <div class="field-group">
            <div class="field-header">
              <label class="field-label" for="openai-model">Modelo de OpenAI</label>
              <span class="shortcut-hint">Selecciona uno o escríbelo abajo</span>
            </div>
            <div class="model-presets-wrap">
              {#each MODELOS_OPENAI as m}
                <button
                  type="button"
                  class="preset-btn {settings.openai_model === m ? 'active' : ''}"
                  onclick={() => (settings.openai_model = m)}
                >
                  {m}
                </button>
              {/each}
            </div>
            <input
              id="openai-model"
              type="text"
              class="input"
              bind:value={settings.openai_model}
              autocorrect="off"
              autocapitalize="none"
              spellcheck="false"
            />
          </div>

          <div class="field-group">
            <label class="field-label" for="openai-key">
              Clave API de OpenAI {aiStatus.has_openai_key ? `(${aiStatus.masked_openai_key} — guardada)` : ""}
            </label>
            <div class="input-with-icon">
              <input
                id="openai-key"
                type={mostrarOpenAiKey ? "text" : "password"}
                class="input has-trailing"
                bind:value={nuevaOpenAiKey}
                autocomplete="new-password"
                autocorrect="off"
                autocapitalize="none"
                spellcheck="false"
                placeholder={aiStatus.has_openai_key ? "Deja vacío para conservar la clave actual" : "sk-..."}
              />
              <button
                type="button"
                class="password-reveal-btn"
                aria-pressed={mostrarOpenAiKey}
                aria-label="Mostrar u ocultar clave"
                onclick={() => (mostrarOpenAiKey = !mostrarOpenAiKey)}
              >
                {#if mostrarOpenAiKey}
                  <EyeOff size={14} />
                {:else}
                  <Eye size={14} />
                {/if}
              </button>
            </div>
            <span class="shortcut-hint">
              Tu clave se guarda únicamente en este equipo y nunca se incluye en respaldos.
            </span>
          </div>
        {:else}
          <div class="field-group">
            <div class="field-header">
              <label class="field-label" for="gemini-model">Modelo de Google Gemini</label>
              <span class="shortcut-hint">Selecciona uno o escríbelo abajo</span>
            </div>
            <div class="model-presets-wrap">
              {#each MODELOS_GEMINI as m}
                <button
                  type="button"
                  class="preset-btn {settings.gemini_model === m ? 'active' : ''}"
                  onclick={() => (settings.gemini_model = m)}
                >
                  {m}
                </button>
              {/each}
            </div>
            <input
              id="gemini-model"
              type="text"
              class="input"
              bind:value={settings.gemini_model}
              autocorrect="off"
              autocapitalize="none"
              spellcheck="false"
            />
          </div>

          <div class="field-group">
            <label class="field-label" for="gemini-key">
              Clave API de Gemini {aiStatus.has_gemini_key ? `(${aiStatus.masked_gemini_key} — guardada)` : ""}
            </label>
            <div class="input-with-icon">
              <input
                id="gemini-key"
                type={mostrarGeminiKey ? "text" : "password"}
                class="input has-trailing"
                bind:value={nuevaGeminiKey}
                autocomplete="new-password"
                autocorrect="off"
                autocapitalize="none"
                spellcheck="false"
                placeholder={aiStatus.has_gemini_key ? "Deja vacío para conservar la clave actual" : "AIza..."}
              />
              <button
                type="button"
                class="password-reveal-btn"
                aria-pressed={mostrarGeminiKey}
                aria-label="Mostrar u ocultar clave"
                onclick={() => (mostrarGeminiKey = !mostrarGeminiKey)}
              >
                {#if mostrarGeminiKey}
                  <EyeOff size={14} />
                {:else}
                  <Eye size={14} />
                {/if}
              </button>
            </div>
            <span class="shortcut-hint">
              Tu clave se guarda únicamente en este equipo y nunca se incluye en respaldos.
            </span>
          </div>
        {/if}
      </section>

      <section class="ia-config-section">
        <h3 class="ia-section-title">Cómo aprende Lokus de tus correcciones</h3>

        <div class="ia-config-toggles">
          <label class="delete-archive-check">
            <input type="checkbox" bind:checked={settings.consolidacion_ia_activa} />
            <div>
              <strong>Organizar memoria automáticamente</strong>
              <div class="shortcut-hint">
                Cuando acumulas varias correcciones, Lokus pide a la IA resumir lo aprendido y eliminar reglas repetidas en segundo plano.
              </div>
            </div>
          </label>

          <label class="delete-archive-check">
            <input type="checkbox" bind:checked={settings.usar_resumen_consolidado} />
            <div>
              <strong>Usar resúmenes aprendidos al autocompletar</strong>
              <div class="shortcut-hint">
                Incluye el resumen del defensor activo y las reglas generales al extraer cada audiencia. Desactívalo si prefieres extraer sin historial previo.
              </div>
            </div>
          </label>
        </div>
      </section>

      <div class="ia-tab-shortcut-row">
        <button
          type="button"
          class="inline-action-link"
          onclick={() => (tabActiva = "aprendizaje")}
        >
          Ver precisión y memoria aprendida
        </button>
      </div>
    </div>
  {:else}
    <div
      id="ia-panel-aprendizaje"
      role="tabpanel"
      aria-labelledby="ia-tab-aprendizaje"
      class="ia-tab-panel"
    >
      <div class="ia-learn-header">
        <p class="shortcut-hint">
          Resumen de lo que la IA ha aprendido con tus audiencias y correcciones.
        </p>
        <button
          type="button"
          class="btn btn-subtle ia-diag-refresh-btn"
          disabled={cargandoDiagnosticoIa || consolidandoIa}
          onclick={() => onRefrescarDiagnosticoIa?.()}
          title="Vuelve a leer las estadísticas guardadas en este equipo. No consume IA ni internet."
        >
          <RefreshCw size={12} />
          <span>{cargandoDiagnosticoIa ? "Cargando..." : "Recargar datos"}</span>
        </button>
      </div>

      {#if !diagnosticoIa}
        <p class="ia-diag-empty">
          {cargandoDiagnosticoIa
            ? "Cargando métricas de aprendizaje..."
            : "Aún no hay telemetría registrada en esta sesión."}
        </p>
      {:else}
        <div class="ia-kpi-strip">
          <div class="ia-kpi-card">
            <span class="ia-kpi-label">Precisión global</span>
            <span class="ia-kpi-value">{precisionGlobalTexto}</span>
            <span class="ia-kpi-sub">
              {diagnosticoIa.aciertos_totales} aciertos · {diagnosticoIa.correcciones_totales} correcciones
            </span>
          </div>
          <div class="ia-kpi-card">
            <span class="ia-kpi-label">Reglas activas</span>
            <span class="ia-kpi-value">{reglasActivasTotal}</span>
            <span class="ia-kpi-sub">
              {totalAbrevAprendidas} abreviaciones aprendidas
            </span>
          </div>
          <div class="ia-kpi-card">
            <span class="ia-kpi-label">Perfiles de defensores</span>
            <span class="ia-kpi-value">{diagnosticoIa.perfiles_defensores ?? 0}</span>
            <span class="ia-kpi-sub">
              {diagnosticoIa.conexiones_grafo ?? 0} conexiones de juzgados
            </span>
          </div>
          <div class="ia-kpi-card">
            <span class="ia-kpi-label">Resúmenes creados</span>
            <span class="ia-kpi-value">{diagnosticoIa.consolidaciones_totales ?? 0}</span>
            <span class="ia-kpi-sub">
              {textoEstadoMemoria(diagnosticoIa.presion_consolidacion_global ?? 0)}
            </span>
          </div>
        </div>

        {#if diagnosticoIa.drift_campos_activos?.length > 0}
          <div class="ia-diag-drift-banner" role="status">
            <AlertTriangle size={14} />
            <span>
              Cambios recientes detectados en:
              <strong>
                {diagnosticoIa.drift_campos_activos.map(etiquetaCampoDiagnostico).join(", ")}
              </strong>
            </span>
          </div>
        {/if}

        <section class="ia-consolidate-card">
          <h3 class="ia-diag-subtitle">
            <Sparkles size={13} />
            <span>Resumir y ordenar memoria</span>
          </h3>
          <p class="shortcut-hint">
            Usa la IA para compactar tus correcciones acumuladas en un resumen breve, eliminar duplicados y mejorar futuras extracciones.
          </p>

          <div class="ia-consolidate-actions">
            <div class="ia-consolidate-option">
              <button
                type="button"
                class="btn btn-subtle ia-consolidate-btn"
                disabled={consolidandoIa || !aiStatus.configured || !defensorArchivo.trim()}
                onclick={() => {
                  consolidandoAmbito = "defensor";
                  onConsolidarMemoriaIa?.(defensorArchivo.trim());
                }}
              >
                <span>
                  {consolidandoIa && consolidandoAmbito === "defensor"
                    ? "Consolidando..."
                    : "Consolidar perfil del defensor"}
                </span>
              </button>
              <span class="shortcut-hint">
                {#if defensorArchivo.trim()}
                  Solo para {defensorArchivo.trim()} (su estilo y siglas habituales).
                {:else}
                  Abre una planilla con defensor asignado para habilitar esta opción.
                {/if}
              </span>
            </div>

            <div class="ia-consolidate-option">
              <button
                type="button"
                class="btn btn-subtle ia-consolidate-btn"
                disabled={consolidandoIa || !aiStatus.configured}
                onclick={() => {
                  consolidandoAmbito = "general";
                  onConsolidarMemoriaIa?.("*");
                }}
              >
                <span>
                  {consolidandoIa && consolidandoAmbito === "general"
                    ? "Consolidando..."
                    : "Consolidar reglas generales"}
                </span>
              </button>
              <span class="shortcut-hint">
                Reglas compartidas para todos los defensores (criterios comunes).
              </span>
            </div>
          </div>

          {#if !aiStatus.configured}
            <p class="shortcut-hint ia-consolidate-note">
              Requiere una clave API configurada en Conexión y ajustes.
            </p>
          {/if}
        </section>

        {#if diagnosticoIa.metricas_por_campo?.length > 0}
          <section class="ia-diag-section">
            <h3 class="ia-diag-subtitle">Precisión por campo</h3>
            <p class="ia-diag-help-note">
              <strong>Acierto reciente</strong> muestra qué tan bien acertó en las últimas audiencias.
              <strong>Fiabilidad mínima</strong> es una estimación prudente que sube conforme acumula más evaluaciones.
            </p>
            <div class="ia-diag-table-wrap">
              <table class="ia-diag-table">
                <thead>
                  <tr>
                    <th>Campo</th>
                    <th class="num">Revisadas</th>
                    <th class="num">Acierto reciente</th>
                    <th class="num">Fiabilidad mínima</th>
                    <th>Estado</th>
                  </tr>
                </thead>
                <tbody>
                  {#each diagnosticoIa.metricas_por_campo as m}
                    {@const estado = obtenerEstadoCampo(m)}
                    <tr>
                      <td>{etiquetaCampoDiagnostico(m.campo)}</td>
                      <td class="num" title="{m.aciertos} aciertos · {m.correcciones} correcciones">
                        {m.evaluaciones}
                      </td>
                      <td class="num {m.evaluaciones === 0 ? 'is-muted' : ''}">
                        {m.evaluaciones === 0 ? "—" : `${Math.round((m.precision_ewma ?? 0) * 100)}%`}
                      </td>
                      <td class="num {m.evaluaciones === 0 ? 'is-muted' : ''}">
                        {m.evaluaciones === 0 ? "—" : `${Math.round((m.confianza_q10 ?? 0) * 100)}%`}
                      </td>
                      <td>
                        <span class="ia-diag-pill {estado.clase}" title={estado.detalle}>
                          {estado.texto}
                        </span>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </section>
        {/if}

        {#if preferenciasLista.length > 0}
          <section class="ia-diag-section">
            <h3 class="ia-diag-subtitle">Criterios aprendidos</h3>
            <p class="ia-diag-help-note">
              Tendencia aprendida según tus correcciones previas (0% = evita aplicarlo, 100% = lo aplica siempre).
            </p>
            <div class="ia-prefs-list">
              {#each preferenciasLista as pref}
                <div class="ia-pref-item">
                  <div class="ia-pref-head">
                    <span class="ia-pref-label">{pref.label}</span>
                    <span class="ia-pref-pct">{pref.pct}%</span>
                  </div>
                  <div class="ia-pref-bar" aria-hidden="true">
                    <div class="ia-pref-fill" style="width: {pref.pct}%"></div>
                  </div>
                </div>
              {/each}
            </div>
          </section>
        {/if}

        <section class="ia-diag-section ia-efficiency-section">
          <h3 class="ia-diag-subtitle ia-subtitle-with-icon">
            <Gauge size={13} />
            <span>Eficiencia de caché y verificación</span>
          </h3>
          <div class="ia-diag-metrics-row">
            <div
              class="ia-diag-metric-box"
              title="Tokens reutilizados desde caché: {eficienciaIa.tokensCacheados} de {eficienciaIa.tokensEntrada} tokens de entrada"
            >
              <span class="ia-diag-metric-label">Tokens cacheados</span>
              <span class="ia-diag-metric-value">
                {eficienciaIa.pctCache !== null ? `${eficienciaIa.pctCache}%` : "Sin datos"}
              </span>
            </div>
            <div
              class="ia-diag-metric-box"
              title="Autoverificaciones disparadas en {eficienciaIa.extracciones} extracciones medidas"
            >
              <span class="ia-diag-metric-label">Autoverificaciones</span>
              <span class="ia-diag-metric-value">{eficienciaIa.autoverifDisparadas}</span>
            </div>
            <div
              class="ia-diag-metric-box"
              title="Veces en que la autoverificación ajustó al menos un campo antes de mostrar el resultado"
            >
              <span class="ia-diag-metric-label">Corregidas por revisión</span>
              <span class="ia-diag-metric-value">{eficienciaIa.autoverifCambios}</span>
            </div>
          </div>
        </section>

        <section class="ia-diag-section ia-playbook-section">
          <h3 class="ia-diag-subtitle ia-subtitle-with-icon">
            <BookOpen size={13} />
            <span>Playbook estructurado ({playbookItemsActivos.length})</span>
          </h3>
          {#if playbookPorSeccion.length > 0}
            <div class="ia-playbook-groups">
              {#each playbookPorSeccion as grupo}
                <div class="ia-playbook-group">
                  <h4 class="ia-playbook-group-title">
                    {grupo.titulo} ({grupo.items.length})
                  </h4>
                  <ul class="ia-diag-rules-list">
                    {#each grupo.items as it (it.id)}
                      <li class="ia-diag-rule-item">
                        <div class="ia-diag-rule-head">
                          <div class="ia-diag-chips">
                            <span class="ia-diag-pill {it.ambito === '*' ? 'is-ok' : 'is-info'}">
                              {etiquetaAmbitoMemoria(it.ambito)}
                            </span>
                            <span class="ia-diag-pill is-neutral">
                              {it.util ?? 0} útil · {it.danino ?? 0} revisar
                            </span>
                          </div>
                          {#if onEliminarItemPlaybookIa}
                            <button
                              type="button"
                              class="mini-icon-btn ia-item-remove-btn"
                              aria-label="Retirar ítem del Playbook"
                              title="Retirar este ítem del Playbook"
                              disabled={eliminandoId === it.id || consolidandoIa}
                              onclick={() => manejarEliminarItemPlaybook(it.id)}
                            >
                              <Trash2 size={12} />
                            </button>
                          {/if}
                        </div>
                        <p class="ia-diag-rule-text">{it.texto}</p>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/each}
            </div>
          {:else}
            <p class="ia-diag-empty">
              Aún no hay ítems en el Playbook estructurado. Al consolidar correcciones se organizan por sección aquí.
            </p>
          {/if}
        </section>

        <section class="ia-diag-section ia-facts-section">
          <h3 class="ia-diag-subtitle ia-subtitle-with-icon">
            <Lightbulb size={13} />
            <span>Hechos aprendidos ({hechosActivos.length})</span>
          </h3>
          {#if hechosActivos.length > 0}
            <ul class="ia-diag-rules-list">
              {#each hechosActivos as hecho (hecho.id)}
                <li class="ia-diag-rule-item">
                  <div class="ia-diag-rule-head">
                    <div class="ia-diag-chips">
                      <span class="ia-diag-pill {hecho.ambito === '*' ? 'is-ok' : 'is-info'}">
                        {etiquetaAmbitoMemoria(hecho.ambito)}
                      </span>
                      {#if hecho.categoria}
                        <span class="ia-diag-pill is-neutral">
                          {etiquetaCampoDiagnostico(hecho.categoria)}
                        </span>
                      {/if}
                    </div>
                    {#if onEliminarHechoIa}
                      <button
                        type="button"
                        class="mini-icon-btn ia-item-remove-btn"
                        aria-label="Olvidar hecho aprendido"
                        title="Olvidar este hecho"
                        disabled={eliminandoId === hecho.id || consolidandoIa}
                        onclick={() => manejarEliminarHecho(hecho.id)}
                      >
                        <Trash2 size={12} />
                      </button>
                    {/if}
                  </div>
                  <p class="ia-diag-rule-text">{hecho.contenido}</p>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="ia-diag-empty">
              Aún no hay hechos aprendidos. Responde aclaraciones o consolida la memoria para guardar criterios puntuales.
            </p>
          {/if}
        </section>

        <section class="ia-diag-section">
          <h3 class="ia-diag-subtitle">Perfiles resumidos</h3>
          {#if diagnosticoIa.perfiles_sinteticos?.length > 0}
            <ul class="ia-diag-rules-list">
              {#each diagnosticoIa.perfiles_sinteticos as p}
                <li class="ia-diag-rule-item">
                  <div class="ia-diag-rule-head">
                    <span class="ia-diag-pill {p.ambito === '*' ? 'is-ok' : 'is-info'}">
                      {p.ambito === "*" ? "General (todos los defensores)" : (p.nombre_mostrar || p.ambito)}
                    </span>
                    <span class="ia-diag-rule-meta">
                      v{p.version ?? 1}{p.ultima_actualizacion ? ` · ${p.ultima_actualizacion.slice(0, 10)}` : ""}
                    </span>
                  </div>
                  <p class="ia-diag-rule-text">{p.resumen}</p>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="ia-diag-empty">
              Aún no se han generado perfiles resumidos. Usa los botones de consolidación arriba cuando acumules correcciones.
            </p>
          {/if}
        </section>

        {#if diagnosticoIa.reglas_destacadas?.length > 0 || totalAbrevAprendidas > 0}
          <section class="ia-diag-section">
            <h3 class="ia-diag-subtitle">
              Reglas y abreviaciones en memoria ({diagnosticoIa.reglas_destacadas?.length ?? 0})
            </h3>
            {#if diagnosticoIa.reglas_destacadas?.length > 0}
              <ul class="ia-diag-rules-list">
                {#each diagnosticoIa.reglas_destacadas as r}
                  {@const estRegla = formatoReglaEstado(r.estado)}
                  <li class="ia-diag-rule-item">
                    <div class="ia-diag-rule-head">
                      <span class="ia-diag-pill {estRegla.clase}">
                        {estRegla.texto}
                      </span>
                      <span class="ia-diag-rule-meta">
                        {etiquetaCampoDiagnostico(r.campo_objetivo)}
                        {#if r.defensor_alcance && r.defensor_alcance !== "*"}
                          · {r.defensor_alcance}
                        {/if}
                        · Fiabilidad {Math.round((r.confianza_q10 ?? 0) * 100)}%
                        · {r.aciertos_directos} de {r.impresiones} aciertos
                      </span>
                    </div>
                    <p class="ia-diag-rule-text">{r.regla}</p>
                  </li>
                {/each}
              </ul>
            {/if}

            {#if totalAbrevAprendidas > 0}
              <div class="ia-diag-chips">
                {#each diagnosticoIa.alias_validados ?? [] as a}
                  <span class="ia-diag-pill">
                    {a.entrada_norm} &rarr; {a.canonico}
                  </span>
                {/each}
                {#each diagnosticoIa.patrones_abreviacion ?? [] as pat}
                  <span class="ia-diag-pill is-info">
                    {pat.patron_entrada} &rarr; {pat.patron_canonico}
                  </span>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

        {#if diagnosticoIa.metricas_por_modelo?.length > 0}
          <section class="ia-diag-section">
            <h3 class="ia-diag-subtitle">Modelos evaluados</h3>
            <div class="ia-diag-chips">
              {#each diagnosticoIa.metricas_por_modelo as mod}
                <span class="ia-diag-pill">
                  {mod.clave} · {Math.round((mod.precision_media ?? 0) * 100)}% acierto ({mod.aciertos}/{mod.evaluaciones})
                </span>
              {/each}
            </div>
          </section>
        {/if}

        <details class="ia-tech-details">
          <summary class="ia-tech-summary">
            <span>Detalles técnicos avanzados</span>
            <ChevronDown size={13} class="ia-diag-chevron" />
          </summary>
          <div class="ia-diag-body">
            <div class="ia-diag-metrics-row">
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Ejemplos de acierto</span>
                <span class="ia-diag-metric-value">{diagnosticoIa.ejemplos_positivos_totales ?? 0}</span>
              </div>
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Inducciones IA</span>
                <span class="ia-diag-metric-value">{diagnosticoIa.llamadas_induccion_llm_total ?? 0}</span>
              </div>
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Patrones en cola</span>
                <span class="ia-diag-metric-value">
                  {diagnosticoIa.clusters_pendientes ?? 0}/{diagnosticoIa.clusters_listos ?? 0}
                </span>
              </div>
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Presión global</span>
                <span class="ia-diag-metric-value">
                  {(diagnosticoIa.presion_consolidacion_global ?? 0).toFixed(2)}
                </span>
              </div>
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Resp. a preguntas</span>
                <span class="ia-diag-metric-value">
                  {Math.round((diagnosticoIa.tasa_respuesta_aclaraciones ?? 1) * 100)}%
                </span>
              </div>
              <div class="ia-diag-metric-box">
                <span class="ia-diag-metric-label">Alias / Patrones</span>
                <span class="ia-diag-metric-value">
                  {diagnosticoIa.alias_validados?.length ?? 0} / {diagnosticoIa.patrones_abreviacion?.length ?? 0}
                </span>
              </div>
            </div>
          </div>
        </details>
      {/if}
    </div>
  {/if}
</div>

<div class="modal-footer">
  <button type="button" class="btn btn-subtle" onclick={onCerrar}>
    Cancelar
  </button>
  <button type="button" class="btn btn-solid" onclick={onGuardarConfigIa}>
    Guardar cambios
  </button>
</div>
