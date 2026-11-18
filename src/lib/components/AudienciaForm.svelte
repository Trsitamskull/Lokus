<script>
  import {
    Sparkles,
    CheckCircle2,
    XCircle,
    Edit3,
    ChevronDown,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import AutocompleteInput from "./AutocompleteInput.svelte";
  import {
    TIPOS_AUDIENCIA,
    MOTIVOS_LABELS,
    HORAS_RAPIDAS,
  } from "../constants.js";

  let {
    archivoActual = null,
    registros = [],
    totales,
    aiStatus,
    textoIa = $bindable(""),
    procesandoIa = false,
    errorIa = "",
    modoEdicion = false,
    filaEditando = null,
    numeroEditando = null,
    radicado = $bindable(""),
    juzgado = $bindable(""),
    tipoSeleccionado = $bindable(""),
    tipoOtra = $bindable(""),
    realizada = $bindable(""),
    fecha = $bindable(""),
    hora = $bindable(""),
    minuto = $bindable(""),
    motivoSeleccionado = $bindable(""),
    observaciones = $bindable(""),
    inputRadicadoEl = $bindable(null),
    onSubmit,
    onCancelarEdicion,
    onLimpiar,
    onProcesarConIa,
    onAbrirConfigIa,
    onVerTodos,
    onEditarRegistro,
    aclaracionPendiente = null,
    onResponderAclaracion,
    onOmitirAclaracion,
  } = $props();

  let iaPanelAbierto = $state(false);
  let iaTextareaEl = $state(null);
  let notaAclaracion = $state("");

  let juzgadosOrdenados = $derived.by(() => {
    const mapa = new Map();
    for (const r of registros) {
      const j = r.juzgado?.trim();
      if (j) mapa.set(j, (mapa.get(j) || 0) + 1);
    }
    return [...mapa.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([nombre]) => nombre);
  });

  let juzgadosFrecuentes = $derived(juzgadosOrdenados.slice(0, 4));

  function formatearFechaInput(e) {
    const digitos = e.target.value.replace(/\D/g, "").slice(0, 8);
    if (digitos.length <= 2) {
      fecha = digitos;
    } else if (digitos.length <= 4) {
      fecha = `${digitos.slice(0, 2)}/${digitos.slice(2)}`;
    } else {
      fecha = `${digitos.slice(0, 2)}/${digitos.slice(2, 4)}/${digitos.slice(4)}`;
    }
  }

  function ponerFechaHoy() {
    const hoy = new Date();
    const d = String(hoy.getDate()).padStart(2, "0");
    const m = String(hoy.getMonth() + 1).padStart(2, "0");
    const y = hoy.getFullYear();
    fecha = `${d}/${m}/${y}`;
  }

  function toggleIaPanel() {
    iaPanelAbierto = !iaPanelAbierto;
    if (iaPanelAbierto) {
      setTimeout(() => {
        iaTextareaEl?.focus();
      }, 20);
    }
  }

  async function manejarExtraer() {
    await onProcesarConIa();
    if (!errorIa && !textoIa.trim()) {
      iaPanelAbierto = false;
    }
  }
</script>

<div class="dossier-stack">
  <!-- FORMULARIO PRINCIPAL AMPLIO -->
  <form
    class="dossier-form-card"
    onsubmit={(e) => {
      e.preventDefault();
      onSubmit();
    }}
  >
    <div class="dossier-card-head">
      <div>
        <h1 class="dossier-title">
          {modoEdicion ? `Editando registro #${numeroEditando}` : "Nueva Audiencia Judicial"}
        </h1>
        <p class="dossier-subtitle">
          {#if archivoActual}
            {archivoActual} · orden cronológico automático al guardar
          {:else}
            Selecciona o crea una planilla en el panel izquierdo para comenzar
          {/if}
        </p>
      </div>
    </div>

    {#if modoEdicion}
      <div class="dossier-banner-edit">
        <span>Editando registro #{numeroEditando} en {archivoActual}</span>
        <div class="action-with-shortcut">
          <span class="shortcut-hint">Esc</span>
          <button type="button" class="btn btn-subtle" onclick={onCancelarEdicion}>
            Cancelar edición
          </button>
        </div>
      </div>
    {:else if !aiStatus.configured}
      <!-- AVISO SUPERIOR SOLO CUANDO FALTA CONFIGURAR LA API DE IA -->
      <div class="ia-top-strip pending">
        <div class="ia-top-row">
          <div class="ia-top-left">
            <span class="ia-card-dot"></span>
            <Sparkles size={14} strokeWidth={2.2} />
            <span class="ia-top-pending-msg">
              Configura tu clave de OpenAI o Gemini para habilitar el autocompletado con IA
            </span>
          </div>
          <button type="button" class="btn btn-ai-warm" onclick={onAbrirConfigIa}>
            <span>Configurar API Key</span>
          </button>
        </div>
      </div>
    {/if}

    {#if !modoEdicion && aclaracionPendiente}
      <div class="ia-top-strip ready">
        <div class="ia-top-row">
          <div class="ia-top-left">
            <span class="ia-card-dot"></span>
            <Sparkles size={14} strokeWidth={2.2} />
            <span class="ia-top-hint" style="color: var(--text-primary); font-weight: 600">
              {aclaracionPendiente.pregunta}
            </span>
          </div>
          <button
            type="button"
            class="btn btn-subtle"
            onclick={() => {
              notaAclaracion = "";
              onOmitirAclaracion?.();
            }}
          >
            Omitir
          </button>
        </div>
        <div class="ia-clarify-row">
          <input
            type="text"
            class="input"
            bind:value={notaAclaracion}
            placeholder="Nota breve opcional para que la IA lo recuerde la próxima vez..."
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                const texto = notaAclaracion.trim();
                notaAclaracion = "";
                if (texto) onResponderAclaracion?.(texto);
                else onOmitirAclaracion?.();
              }
            }}
          />
          <button
            type="button"
            class="btn btn-solid"
            disabled={!notaAclaracion.trim()}
            onclick={() => {
              const texto = notaAclaracion.trim();
              notaAclaracion = "";
              onResponderAclaracion?.(texto);
            }}
          >
            <span>Guardar regla</span>
          </button>
        </div>
      </div>
    {/if}

    <!-- FILA 1: RADICADO Y DESPACHO -->
    <div class="dossier-row-2col">
      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="radicado">Radicado o Procesado</label>
        </div>
        <input
          id="radicado"
          name="radicado"
          type="text"
          class="input"
          bind:this={inputRadicadoEl}
          bind:value={radicado}
          autocorrect="off"
          autocapitalize="none"
          spellcheck="false"
          placeholder="Ej: JUAN PEREZ - 11001600000020250000100"
        />
      </div>

      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="juzgado">Juzgado o Despacho Judicial</label>
        </div>
        <AutocompleteInput
          id="juzgado"
          name="juzgado"
          bind:value={juzgado}
          suggestions={juzgadosOrdenados}
          placeholder="Nombre del juzgado o despacho"
        />
        {#if juzgadosFrecuentes.length > 0}
          <div class="recent-chips-row">
            {#each juzgadosFrecuentes as j}
              <button
                type="button"
                class="recent-chip"
                onclick={() => (juzgado = j)}
                title="Usar '{j}'"
              >
                {j}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <!-- FILA 2: TIPO, FECHA Y HORA -->
    <div class="dossier-row-3col">
      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="tipo-audiencia">Tipo de Audiencia</label>
        </div>
        <CustomSelect
          id="tipo-audiencia"
          bind:value={tipoSeleccionado}
          options={TIPOS_AUDIENCIA}
          placeholder="Seleccionar o filtrar tipo..."
          searchPlaceholder="Filtrar tipo de audiencia..."
        />
      </div>

      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="fecha">Fecha</label>
          <button type="button" class="inline-action-link" onclick={ponerFechaHoy}>
            Hoy
          </button>
        </div>
        <input
          id="fecha"
          name="fecha"
          type="text"
          inputmode="numeric"
          maxlength="10"
          class="input"
          value={fecha}
          oninput={formatearFechaInput}
          placeholder="DD/MM/AAAA"
        />
      </div>

      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="hora">Hora (24h)</label>
        </div>
        <div class="time-inline-box">
          <input
            id="hora"
            type="text"
            inputmode="numeric"
            maxlength="2"
            class="time-part-input"
            bind:value={hora}
            onclick={(e) => e.currentTarget.select()}
            placeholder="HH"
            aria-label="Hora"
          />
          <span class="time-sep">:</span>
          <input
            id="minuto"
            type="text"
            inputmode="numeric"
            maxlength="2"
            class="time-part-input"
            bind:value={minuto}
            onclick={(e) => e.currentTarget.select()}
            placeholder="MM"
            aria-label="Minutos"
          />
          <div class="time-presets">
            {#each HORAS_RAPIDAS as [h, m]}
              <button
                type="button"
                class="preset-btn"
                onclick={() => {
                  hora = h;
                  minuto = m;
                }}
              >
                {h}:{m}
              </button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    {#if tipoSeleccionado === "Otra"}
      <div class="field-group">
        <label class="field-label" for="tipo-otra">Especificar otro tipo de audiencia</label>
        <input
          id="tipo-otra"
          type="text"
          class="input"
          bind:value={tipoOtra}
          placeholder="Escriba el tipo de audiencia..."
        />
      </div>
    {/if}

    <div class="dossier-divider"></div>

    <!-- FILA 3: ESTADO DE REALIZACIÓN -->
    <div class="field-group">
      <span class="field-label">Resultado de la diligencia</span>
      <div class="segmented-control" role="radiogroup" aria-label="¿Se realizó la audiencia?">
        <button
          type="button"
          role="radio"
          aria-checked={realizada === "SI"}
          class="segmented-option {realizada === 'SI' ? 'is-si' : ''}"
          onclick={() => {
            realizada = "SI";
            motivoSeleccionado = "";
          }}
        >
          <CheckCircle2 size={15} strokeWidth={2} />
          <span>Realizada (SI)</span>
        </button>

        <button
          type="button"
          role="radio"
          aria-checked={realizada === "NO"}
          class="segmented-option {realizada === 'NO' ? 'is-no' : ''}"
          onclick={() => {
            realizada = "NO";
          }}
        >
          <XCircle size={15} strokeWidth={2} />
          <span>No realizada / Aplazada (NO)</span>
        </button>
      </div>
    </div>

    <!-- FILA 4: CAUSAL -->
    {#if realizada === "NO"}
      <div class="motivos-drawer">
        <div class="field-header">
          <span class="field-label">Causal atribuible a (seleccione una)</span>
          <span class="shortcut-hint">Causales de aplazamiento</span>
        </div>
        <div class="motivos-grid" role="radiogroup" aria-label="Causal de no realización">
          {#each MOTIVOS_LABELS as m, idx}
            {@const isSelected = motivoSeleccionado === m}
            <button
              type="button"
              role="radio"
              aria-checked={isSelected}
              class="motivo-item {isSelected ? 'selected' : ''}"
              onclick={() => {
                motivoSeleccionado = motivoSeleccionado === m ? "" : m;
              }}
            >
              <span>{m}</span>
              <span class="motivo-badge">{totales.totales_motivos[idx] ?? 0}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- FILA 5: OBSERVACIONES -->
    <div class="field-group">
      <div class="field-header">
        <label class="field-label" for="observaciones">Observaciones</label>
        <span class="shortcut-hint">Opcional</span>
      </div>
      <textarea
        id="observaciones"
        class="textarea"
        rows="3"
        bind:value={observaciones}
        placeholder="Constancias, causas secundarias o fecha de reprogramación..."
      ></textarea>
    </div>

    <div class="dossier-divider"></div>

    {#if !modoEdicion && aiStatus.configured && iaPanelAbierto}
      <div class="ia-top-strip ready">
        <div class="ia-top-row">
          <div class="ia-top-left">
            <span class="ia-card-dot"></span>
            <Sparkles size={14} strokeWidth={2.2} />
            <span class="ia-top-hint" style="color: var(--text-primary); font-weight: 600">
              Autocompletar con IA
            </span>
            <span class="ia-top-hint">
              Pega una nota de WhatsApp o acta
            </span>
          </div>

          <button
            type="button"
            class="mini-icon-btn"
            onclick={onAbrirConfigIa}
            title="Ajustar proveedor de IA ({aiStatus.model})"
            aria-label="Ajustar configuración de IA"
          >
            <SlidersHorizontal size={14} />
          </button>
        </div>

        <div class="ia-top-body">
          <textarea
            bind:this={iaTextareaEl}
            class="textarea"
            rows="3"
            bind:value={textoIa}
            placeholder="Pega aquí el texto o acta de la audiencia (Ctrl+Enter para extraer)..."
            onkeydown={(e) => {
              if (e.ctrlKey && e.key === "Enter") {
                e.preventDefault();
                manejarExtraer();
              }
            }}
          ></textarea>
          {#if errorIa}
            <p style="color: var(--status-no); font-size: 12px">{errorIa}</p>
          {/if}
          <div class="ia-card-actions">
            <button
              type="button"
              class="btn btn-subtle"
              onclick={() => (iaPanelAbierto = false)}
              disabled={procesandoIa}
            >
              Cerrar
            </button>
            <div class="action-with-shortcut">
              <span class="shortcut-hint">Ctrl + Enter</span>
              <button
                type="button"
                class="btn btn-solid"
                onclick={manejarExtraer}
                disabled={procesandoIa || !textoIa.trim()}
              >
                <span>{procesandoIa ? "Extrayendo..." : "Extraer al formulario →"}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <!-- ACCIONES FINALES: AUTOCOMPLETAR (CON API) A LA IZQUIERDA · LIMPIAR Y REGISTRAR A LA DERECHA -->
    <div class="dossier-actions">
      {#if !modoEdicion && aiStatus.configured}
        <div class="dossier-actions-left">
          <div class="action-with-shortcut">
            <button
              type="button"
              class="btn btn-ia-action"
              aria-expanded={iaPanelAbierto}
              onclick={toggleIaPanel}
              title="Autocompletar con IA ({aiStatus.model})"
            >
              <Sparkles size={14} strokeWidth={2.2} />
              <span>Autocompletar con IA</span>
              <ChevronDown
                size={13}
                style="transform: {iaPanelAbierto ? 'rotate(0deg)' : 'rotate(180deg)'}; transition: transform 0.14s ease"
              />
            </button>
            <span class="shortcut-hint">Ctrl + I</span>
          </div>
        </div>
      {/if}

      <div class="dossier-actions-right">
        {#if !modoEdicion}
          <div class="action-with-shortcut">
            <span class="shortcut-hint">Ctrl + L</span>
            <button type="button" class="btn btn-subtle" onclick={onLimpiar}>
              Limpiar
            </button>
          </div>
        {/if}
        <div class="action-with-shortcut">
          <span class="shortcut-hint">Ctrl + S</span>
          <button type="submit" class="btn btn-solid" style="padding: 8px 22px">
            <span>{modoEdicion ? "Guardar cambios" : "Registrar audiencia"}</span>
          </button>
        </div>
      </div>
    </div>
  </form>

  <!-- ACTIVIDAD RECIENTE ABAJO -->
  {#if registros.length > 0}
    <section class="dossier-recent-card">
      <div class="field-header">
        <span class="field-label">Últimos registros en {archivoActual?.replace(/\.xlsx$/i, "")}</span>
        <button type="button" class="inline-action-link" onclick={onVerTodos}>
          Ver todos ({registros.length}) →
        </button>
      </div>

      <div class="recent-strip-list">
        {#each registros.slice(0, 3) as r}
          <div class="recent-strip-row">
            <div class="recent-strip-meta">
              <strong style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                #{r.numero} · {r.radicado}
              </strong>
              <span class="shortcut-hint" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                {r.fecha} {r.hora || "sin hora"} · {r.tipo}
              </span>
            </div>
            <div class="recent-strip-right">
              <span class={r.realizada === "SI" ? "tag-si" : "tag-no"}>
                {r.realizada}
              </span>
              <button
                type="button"
                class="mini-icon-btn"
                onclick={() => onEditarRegistro(r)}
                title="Editar este registro"
                aria-label="Editar este registro"
              >
                <Edit3 size={13} />
              </button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}
</div>
