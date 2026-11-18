<script>
  import {
    Sparkles,
    SlidersHorizontal,
    Eye,
    EyeOff,
    X,
    UserCheck,
    History,
    RotateCcw,
    AlertTriangle,
    Edit3,
    Trash2,
  } from "@lucide/svelte";
  import AutocompleteInput from "./AutocompleteInput.svelte";
  import { MODELOS_GEMINI, MODELOS_OPENAI } from "../constants.js";

  let {
    modalActivo = $bindable(null),
    defensorTargetArchivo = null,
    defensorTargetNombre = "",
    defensores = [],
    settings = $bindable(),
    aiStatus,
    nombreNuevoArchivo = $bindable(""),
    defensorNuevoArchivo = $bindable(""),
    errorModalCrear = "",
    textoIa = $bindable(""),
    procesandoIa = false,
    errorIa = "",
    nuevaOpenAiKey = $bindable(""),
    nuevaGeminiKey = $bindable(""),
    mostrarOpenAiKey = $bindable(false),
    mostrarGeminiKey = $bindable(false),
    confirmacionPendiente = $bindable(null),
    ejecutandoConfirmacion = false,
    duplicadoDetectado = $bindable(null),
    filtroDefensores = $bindable(""),
    nuevoDefensorDirectorio = $bindable(""),
    defensorEditando = $bindable(null),
    defensorEditandoValor = $bindable(""),
    defensorPlanillaDraft = $bindable(""),
    errorDefensores = "",
    respaldosArchivo = [],
    respaldosTarget = "",
    cargandoRespaldos = false,
    onCrearNuevoArchivo,
    onAbrirConfigIa,
    onProcesarConIa,
    onGuardarConfigIa,
    onConfirmarSinHora,
    onConfirmarDuplicado,
    onEjecutarConfirmacion,
    onAsignarDefensor,
    onAgregarDefensor,
    onGuardarRenombreDefensor,
    onEliminarDefensor,
    onRestaurarRespaldo,
  } = $props();

  let dialogEl = $state(null);

  $effect(() => {
    if (modalActivo && dialogEl && !dialogEl.open) {
      dialogEl.showModal();
    }
  });

  let defensoresFiltrados = $derived.by(() => {
    const q = filtroDefensores.trim().toLowerCase();
    if (!q) return defensores;
    return defensores.filter((d) => d.toLowerCase().includes(q));
  });

  async function cerrarDefensoresYGuardarPendiente() {
    if (
      defensorTargetArchivo &&
      defensorPlanillaDraft.trim() !== defensorTargetNombre.trim()
    ) {
      await onAsignarDefensor(defensorPlanillaDraft);
    }
    modalActivo = null;
  }
</script>

{#if modalActivo}
  <dialog
    bind:this={dialogEl}
    class={modalActivo === "defensores"
      ? "modal-wide"
      : modalActivo === "crear"
        ? "modal-overflow-visible"
        : ""}
    aria-labelledby="dialog-heading"
    onclose={() => {
      modalActivo = null;
      confirmacionPendiente = null;
      duplicadoDetectado = null;
      nuevaOpenAiKey = "";
      nuevaGeminiKey = "";
      mostrarOpenAiKey = false;
      mostrarGeminiKey = false;
    }}
  >
    {#if modalActivo === "crear"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">Nuevo archivo de audiencias</h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <div class="field-group">
          <label class="field-label" for="nuevo-archivo-input">
            Nombre de la planilla Excel
          </label>
          <input
            id="nuevo-archivo-input"
            type="text"
            class="input {errorModalCrear ? 'is-invalid' : ''}"
            bind:value={nombreNuevoArchivo}
            autocorrect="off"
            spellcheck="false"
            placeholder="Ej: informe_agosto_2025"
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                onCrearNuevoArchivo();
              }
            }}
          />
          {#if errorModalCrear}
            <p style="color: var(--status-no); font-size: 12px">{errorModalCrear}</p>
          {/if}
        </div>

        <div class="field-group">
          <div class="field-header">
            <label class="field-label" for="nuevo-archivo-defensor">
              Defensor Público asignado (opcional)
            </label>
          </div>
          <AutocompleteInput
            id="nuevo-archivo-defensor"
            bind:value={defensorNuevoArchivo}
            suggestions={defensores}
            placeholder={defensores.length > 0
              ? "Selecciona de tu directorio o escribe un nombre nuevo"
              : "Escribe el nombre y se guardará para próximas planillas"}
            onEnter={onCrearNuevoArchivo}
          />
          <span class="shortcut-hint">
            Si ingresas un nombre nuevo se guardará en tu directorio de defensores.
          </span>
        </div>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onCrearNuevoArchivo}>
          Crear archivo
        </button>
      </div>

    {:else if modalActivo === "ia"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <Sparkles size={15} />
          <span>Autocompletar desde texto ({aiStatus.model})</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <div class="field-group">
          <label class="field-label" for="ia-textarea">
            Pega el reporte, acta o nota de la audiencia
          </label>
          <textarea
            id="ia-textarea"
            class="textarea {errorIa ? 'is-invalid' : ''}"
            style="min-height: 130px"
            bind:value={textoIa}
            placeholder="Ej: Audiencia concentrada proceso 200016001075202400123 contra JUAN PEREZ el 15 de agosto a las 2:30 PM en el Juzgado 2 Penal del Circuito. No se realizó porque la fiscal no se conectó..."
          ></textarea>
        </div>
        <span class="shortcut-hint">Presiona Ctrl + Enter para extraer datos</span>
        {#if errorIa}
          <p style="color: var(--status-no); font-size: 12px">{errorIa}</p>
        {/if}
      </div>
      <div class="modal-footer">
        <button
          type="button"
          class="btn btn-subtle"
          onclick={onAbrirConfigIa}
          disabled={procesandoIa}
        >
          Ajustes de IA
        </button>
        <button
          type="button"
          class="btn btn-solid"
          onclick={onProcesarConIa}
          disabled={procesandoIa}
        >
          {procesandoIa ? "Extrayendo..." : "Extraer al formulario"}
        </button>
      </div>

    {:else if modalActivo === "config_ia"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <SlidersHorizontal size={15} />
          <span>Proveedor de Inteligencia Artificial</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
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
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onGuardarConfigIa}>
          Guardar
        </button>
      </div>

    {:else if modalActivo === "confirmar_sin_hora"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">Guardar audiencia sin hora</h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <p style="color: var(--text-secondary)">
          No especificaste una hora. ¿Deseas registrar la audiencia dejando vacía la columna de hora en el Excel?
        </p>
      </div>
      <div class="modal-footer">
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
        >
          Volver
        </button>
        <button type="button" class="btn btn-solid" onclick={onConfirmarSinHora}>
          Confirmar y guardar
        </button>
      </div>

    {:else if modalActivo === "confirmar_duplicado" && duplicadoDetectado}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <AlertTriangle size={15} style="color: var(--accent-warm)" />
          <span>Posible audiencia duplicada</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <p style="color: var(--text-secondary)">
          Ya existe un registro en esta planilla con el mismo proceso o nombre muy similar en la misma fecha:
        </p>
        <div class="duplicado-card">
          <strong>#{duplicadoDetectado.numero} · {duplicadoDetectado.radicado}</strong>
          <div class="duplicado-meta">
            <span>{duplicadoDetectado.fecha} {duplicadoDetectado.hora || "sin hora"}</span>
            <span>{duplicadoDetectado.tipo}</span>
            <span>{duplicadoDetectado.juzgado}</span>
          </div>
        </div>
      </div>
      <div class="modal-footer">
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
        >
          Cancelar y revisar
        </button>
        <button type="button" class="btn btn-solid" onclick={onConfirmarDuplicado}>
          Guardar de todas formas
        </button>
      </div>

    {:else if modalActivo === "defensores"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <UserCheck size={15} />
          <span>Defensor Público de la planilla</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        {#if defensorTargetArchivo}
          <div class="field-group">
            <label class="field-label" for="defensor-planilla-input">
              Defensor asignado en {defensorTargetArchivo}
            </label>
            <div style="display: flex; gap: 8px; align-items: stretch">
              <div style="flex: 1; min-width: 0">
                <AutocompleteInput
                  id="defensor-planilla-input"
                  bind:value={defensorPlanillaDraft}
                  suggestions={defensores}
                  placeholder="Escribe, corrige o elige el defensor de esta planilla"
                  onEnter={() => onAsignarDefensor(defensorPlanillaDraft)}
                />
              </div>
              <button
                type="button"
                class="btn btn-solid"
                onclick={() => onAsignarDefensor(defensorPlanillaDraft)}
              >
                Guardar
              </button>
              {#if defensorTargetNombre}
                <button
                  type="button"
                  class="btn btn-subtle"
                  onclick={() => onAsignarDefensor("")}
                  title="Quitar el nombre de defensor asignado en este Excel"
                >
                  Quitar
                </button>
              {/if}
            </div>
            <span class="shortcut-hint">
              Si te equivocaste al crear el archivo, corrige el nombre aquí y se actualizará directamente en el Excel.
            </span>
          </div>
          <div class="dossier-divider"></div>
        {/if}

        <div class="field-group">
          <div class="field-header">
            <span class="field-label">Directorio guardado ({defensores.length})</span>
            <span class="shortcut-hint">Selecciona, corrige o elimina</span>
          </div>
          <div style="display: flex; gap: 8px">
            <input
              type="text"
              class="input"
              bind:value={nuevoDefensorDirectorio}
              placeholder="Agregar nuevo defensor al directorio..."
              onkeydown={(e) => {
                if (!e.isComposing && e.key === "Enter") {
                  e.preventDefault();
                  onAgregarDefensor();
                }
              }}
            />
            <button type="button" class="btn btn-subtle" onclick={onAgregarDefensor}>
              Agregar
            </button>
          </div>
          {#if defensores.length > 6}
            <input
              type="search"
              class="input"
              bind:value={filtroDefensores}
              placeholder="Filtrar entre los {defensores.length} defensores..."
            />
          {/if}
          {#if errorDefensores}
            <p style="color: var(--status-no); font-size: 12px">{errorDefensores}</p>
          {/if}
        </div>

        {#if defensores.length === 0}
          <div class="sidebar-empty" style="margin-top: 0">
            Tu directorio está vacío. Agrega los nombres que necesites para reutilizarlos con un clic.
          </div>
        {:else if defensoresFiltrados.length === 0}
          <p class="shortcut-hint">No hay defensores que coincidan con "{filtroDefensores}".</p>
        {:else}
          <div class="defensores-list scroller">
            {#each defensoresFiltrados as defName}
              <div class="defensor-row">
                {#if defensorEditando === defName}
                  <input
                    type="text"
                    class="input"
                    style="height: 30px; padding: 4px 8px"
                    bind:value={defensorEditandoValor}
                    onkeydown={(e) => {
                      if (!e.isComposing && e.key === "Enter") {
                        e.preventDefault();
                        onGuardarRenombreDefensor();
                      } else if (e.key === "Escape") {
                        e.preventDefault();
                        e.stopPropagation();
                        defensorEditando = null;
                      }
                    }}
                  />
                  <div style="display: flex; gap: 4px">
                    <button
                      type="button"
                      class="btn btn-solid"
                      style="padding: 4px 10px"
                      onclick={onGuardarRenombreDefensor}
                    >
                      Guardar
                    </button>
                    <button
                      type="button"
                      class="btn btn-subtle"
                      style="padding: 4px 8px"
                      onclick={() => (defensorEditando = null)}
                    >
                      Cancelar
                    </button>
                  </div>
                {:else}
                  <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                    {defName}
                  </span>
                  <div style="display: inline-flex; gap: 4px; flex-shrink: 0">
                    {#if defensorTargetArchivo}
                      <button
                        type="button"
                        class="btn btn-subtle"
                        style="padding: 3px 9px; font-size: 11.5px"
                        onclick={() => onAsignarDefensor(defName)}
                      >
                        Usar en planilla
                      </button>
                    {/if}
                    <button
                      type="button"
                      class="mini-icon-btn"
                      title="Corregir nombre"
                      aria-label="Corregir nombre"
                      onclick={() => {
                        defensorEditando = defName;
                        defensorEditandoValor = defName;
                      }}
                    >
                      <Edit3 size={13} />
                    </button>
                    <button
                      type="button"
                      class="mini-icon-btn danger"
                      title="Eliminar del directorio"
                      aria-label="Eliminar del directorio"
                      onclick={() => onEliminarDefensor(defName)}
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-solid" onclick={cerrarDefensoresYGuardarPendiente}>
          Listo
        </button>
      </div>

    {:else if modalActivo === "respaldos"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <History size={15} />
          <span>Respaldos automáticos de {respaldosTarget}</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <p class="shortcut-hint">
          Lokus guarda automáticamente hasta 3 versiones previas antes de cada cambio en la planilla.
        </p>
        {#if cargandoRespaldos}
          <p class="shortcut-hint">Cargando versiones...</p>
        {:else if respaldosArchivo.length === 0}
          <div class="sidebar-empty" style="margin-top: 0">
            Aún no hay respaldos guardados para esta planilla. Se crearán automáticamente al registrar o editar audiencias.
          </div>
        {:else}
          <div style="display: flex; flex-direction: column; gap: 8px">
            {#each respaldosArchivo as resp, idx}
              <div class="respaldo-row">
                <div>
                  <div style="font-weight: 600">{resp.fecha_legible}</div>
                  <div class="shortcut-hint">
                    {idx === 0 ? "Versión más reciente" : `Copia previa #${idx + 1}`} · {(resp.tamano_bytes / 1024).toFixed(1)} KB
                  </div>
                </div>
                <button
                  type="button"
                  class="btn btn-subtle"
                  onclick={() => onRestaurarRespaldo(resp)}
                >
                  <RotateCcw size={13} />
                  <span>Restaurar</span>
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-solid" onclick={() => (modalActivo = null)}>
          Cerrar
        </button>
      </div>

    {:else if modalActivo === "confirmar" && confirmacionPendiente}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">{confirmacionPendiente.titulo}</h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
          disabled={ejecutandoConfirmacion}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <p style="color: var(--text-secondary)">
          {confirmacionPendiente.mensaje}
        </p>
      </div>
      <div class="modal-footer">
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
          disabled={ejecutandoConfirmacion}
        >
          Cancelar
        </button>
        <button
          type="button"
          class="btn {confirmacionPendiente.peligroso ? 'btn-danger' : 'btn-solid'}"
          onclick={onEjecutarConfirmacion}
          disabled={ejecutandoConfirmacion}
        >
          {ejecutandoConfirmacion ? "Procesando..." : confirmacionPendiente.textoConfirmar}
        </button>
      </div>

    {:else if modalActivo === "atajos"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">Atajos de teclado</h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <div class="shortcut-grid">
          <div class="shortcut-row"><span>Guardar audiencia</span><span class="kbd">Ctrl+S</span></div>
          <div class="shortcut-row"><span>Autocompletar IA</span><span class="kbd">Ctrl+I</span></div>
          <div class="shortcut-row"><span>Nuevo archivo</span><span class="kbd">Ctrl+N</span></div>
          <div class="shortcut-row"><span>Cambiar de sección</span><span class="kbd">Ctrl+1 / 2 / 3</span></div>
          <div class="shortcut-row"><span>Limpiar campos</span><span class="kbd">Ctrl+L</span></div>
          <div class="shortcut-row"><span>Exportar copia</span><span class="kbd">Ctrl+Shift+S</span></div>
          <div class="shortcut-row"><span>Tema Azul / Oscuro</span><span class="kbd">Ctrl+T</span></div>
          <div class="shortcut-row"><span>Refrescar / Cerrar</span><span class="kbd">F5 / Esc</span></div>
        </div>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-solid" onclick={() => (modalActivo = null)}>
          Cerrar
        </button>
      </div>
    {/if}
  </dialog>
{/if}
