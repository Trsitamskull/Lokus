<script>
  import { Sparkles, CheckCircle2, XCircle, Edit3 } from "@lucide/svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import {
    TIPOS_AUDIENCIA,
    MOTIVOS_LABELS,
    HORAS_RAPIDAS,
  } from "../constants.js";

  let {
    archivoActual = null,
    registros = [],
    totales,
    aiConfigured = false,
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
    onAbrirIa,
    onVerTodos,
    onEditarRegistro,
  } = $props();

  let juzgadosFrecuentes = $derived.by(() => {
    const mapa = new Map();
    for (const r of registros) {
      const j = r.juzgado?.trim();
      if (j) mapa.set(j, (mapa.get(j) || 0) + 1);
    }
    return [...mapa.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 4)
      .map(([nombre]) => nombre);
  });

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
</script>

<form
  class="dossier-sheet"
  onsubmit={(e) => {
    e.preventDefault();
    onSubmit();
  }}
>
  {#if modoEdicion}
    <div class="dossier-banner-edit">
      <span>Editando registro #{numeroEditando} (Fila {filaEditando} de {archivoActual})</span>
      <button type="button" class="btn btn-subtle" onclick={onCancelarEdicion}>
        Cancelar edición (Esc)
      </button>
    </div>
  {:else}
    <button
      type="button"
      class="ia-quick-strip {aiConfigured ? 'ready' : ''}"
      onclick={onAbrirIa}
    >
      <span class="ia-quick-strip-left">
        <Sparkles size={14} strokeWidth={2} />
        {#if aiConfigured}
          <span>¿Tienes el acta o nota de la audiencia? Pegar aquí para autocompletar</span>
        {:else}
          <span>Configura tu clave de OpenAI o Gemini para autocompletar audiencias desde texto</span>
        {/if}
      </span>
      <span class="kbd">{aiConfigured ? "Ctrl+I" : "Configurar"}</span>
    </button>
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
      <input
        id="juzgado"
        name="juzgado"
        type="text"
        list="juzgados-sugeridos"
        class="input"
        bind:value={juzgado}
        placeholder="Nombre del juzgado o despacho"
      />
      <datalist id="juzgados-sugeridos">
        {#each juzgadosFrecuentes as j}
          <option value={j}></option>
        {/each}
      </datalist>
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
        <span class="shortcut-hint">Columnas I a P de la plantilla</span>
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

  <!-- ACCIONES FINALES -->
  <div class="dossier-actions">
    <button type="button" class="btn btn-subtle" onclick={onLimpiar}>
      Limpiar campos (Ctrl+L)
    </button>

    <div style="display: flex; gap: 10px; align-items: center">
      <span class="shortcut-hint">Ctrl + S</span>
      <button type="submit" class="btn btn-solid" style="padding: 8px 22px">
        <span>{modoEdicion ? "Guardar cambios" : "Registrar audiencia"}</span>
      </button>
    </div>
  </div>

  <!-- ACTIVIDAD RECIENTE EN VIVO -->
  {#if registros.length > 0 && !modoEdicion}
    <div class="recent-records-strip">
      <div class="field-header">
        <span class="shortcut-hint">Últimos registros en {archivoActual}</span>
        <button type="button" class="inline-action-link" onclick={onVerTodos}>
          Ver todos ({registros.length})
        </button>
      </div>
      {#each registros.slice(0, 3) as r}
        <div class="recent-strip-row">
          <div style="display: flex; align-items: center; gap: 10px; min-width: 0">
            <span style="color: var(--text-muted); font-variant-numeric: tabular-nums">
              #{r.numero}
            </span>
            <strong style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
              {r.radicado}
            </strong>
            <span style="color: var(--text-secondary)">· {r.tipo}</span>
          </div>
          <div style="display: flex; align-items: center; gap: 10px; flex-shrink: 0">
            <span style="font-variant-numeric: tabular-nums; color: var(--text-secondary)">
              {r.fecha} {r.hora}
            </span>
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
  {/if}
</form>
