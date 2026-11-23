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
    FolderPlus,
    FolderInput,
    Pencil,
    Package,
    Download,
    Upload,
    Archive,
    FileSpreadsheet,
    Palette,
    Check,
    Activity,
    ChevronDown,
    RefreshCw,
  } from "@lucide/svelte";
  import AutocompleteInput from "./AutocompleteInput.svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import {
    COLORES_CARPETA,
    MODELOS_GEMINI,
    MODELOS_OPENAI,
    generarNombrePlanilla,
  } from "../constants.js";

  let {
    modalActivo = $bindable(null),
    defensorTargetArchivo = null,
    defensorTargetNombre = "",
    defensores = [],
    carpetasDisponibles = [],
    settings = $bindable(),
    aiStatus,
    nombreNuevoArchivo = $bindable(""),
    defensorNuevoArchivo = $bindable(""),
    carpetaNuevoArchivo = $bindable(""),
    errorModalCrear = "",
    errorModalDefensor = "",
    archivoEditarTarget = "",
    archivoEditarNombre = $bindable(""),
    archivoEditarDefensor = $bindable(""),
    archivoEditarCarpeta = $bindable(""),
    errorModalEditarArchivo = "",
    guardandoEdicionArchivo = false,
    carpetaPadreNueva = $bindable(""),
    nombreNuevaCarpeta = $bindable(""),
    colorNuevaCarpeta = $bindable(""),
    carpetaRenombrarActual = "",
    carpetaRenombrarNueva = $bindable(""),
    colorCarpetaRenombrar = $bindable(""),
    colorCarpetaTarget = "",
    colorCarpetaEditando = $bindable(""),
    errorModalCarpeta = "",
    archivoMoverTarget = "",
    carpetaDestinoMover = $bindable(""),
    carpetaDestinoNueva = $bindable(""),
    errorModalMover = "",
    eliminarTargetNombre = "",
    eliminarTargetResumen = null,
    eliminarCarpetaTarget = null,
    conservarEstadisticasAlEliminar = $bindable(true),
    portabilidadTab = $bindable("exportar"),
    tipoExportacion = $bindable("completo"),
    exportandoPaquete = false,
    archivoImportNombre = "",
    inspeccionImportacion = null,
    errorImportacion = "",
    modoConflictoImport = $bindable("omitir"),
    importarConfigPaquete = $bindable(true),
    importandoPaquete = false,
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
    diagnosticoIa = null,
    cargandoDiagnosticoIa = false,
    defensorArchivo = "",
    consolidandoIa = false,
    toast = null,
    onRefrescarDiagnosticoIa = null,
    onConsolidarMemoriaIa = null,
    onCrearNuevoArchivo,
    onDefensorNuevoChange = null,
    onNombreNuevoInput = null,
    onUsarFormatoAutomaticoCrear = null,
    onGuardarEdicionArchivo = null,
    onUsarFormatoAutomaticoEditar = null,
    onCrearCarpeta,
    onRenombrarCarpeta,
    onGuardarColorCarpeta,
    onMoverArchivo,
    onConfirmarEliminarArchivo,
    onConfirmarEliminarCarpeta,
    onExportarPaquete,
    onSeleccionarArchivoImportar,
    onEjecutarImportacion,
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
    onRenombrarDefensorDirecto = null,
    onEliminarDefensorDirecto = null,
    onRestaurarRespaldo,
  } = $props();

  let sugerenciaNombreCrear = $derived(generarNombrePlanilla(defensorNuevoArchivo));
  let sugerenciaNombreEditar = $derived(generarNombrePlanilla(archivoEditarDefensor));

  let dialogEl = $state(null);
  let consolidandoAmbito = $state("");

  $effect(() => {
    if (modalActivo && dialogEl && !dialogEl.open) {
      dialogEl.showModal();
    }
  });

  $effect(() => {
    if (!consolidandoIa) {
      consolidandoAmbito = "";
    }
  });

  let defensoresFiltrados = $derived.by(() => {
    const q = filtroDefensores.trim().toLowerCase();
    if (!q) return defensores;
    return defensores.filter((d) => d.toLowerCase().includes(q));
  });

  let opcionesCarpetas = $derived([
    { value: "", label: "Carpeta principal (raíz)" },
    ...carpetasDisponibles.map((c) => ({ value: c, label: c })),
  ]);

  let opcionesCarpetasMover = $derived([
    ...opcionesCarpetas,
    { value: "__NUEVA__", label: "+ Escribir otra carpeta nueva..." },
  ]);

  function uint8ABase64(bytes) {
    let bin = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
    }
    return btoa(bin);
  }

  async function manejarArchivoImportInput(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = "";
    if (!file) return;
    const buffer = await file.arrayBuffer();
    const b64 = uint8ABase64(new Uint8Array(buffer));
    await onSeleccionarArchivoImportar?.(file.name, b64);
  }

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

{#snippet selectorColorCarpeta(valor, onSelect)}
  <div class="field-group">
    <span class="field-label">Color de la carpeta</span>
    <div class="folder-color-grid" role="radiogroup" aria-label="Color de carpeta">
      {#each COLORES_CARPETA as c (c.id)}
        {@const seleccionado = (valor || "") === c.id}
        <button
          type="button"
          role="radio"
          aria-checked={seleccionado}
          class="folder-color-swatch {seleccionado ? 'active' : ''}"
          onclick={() => onSelect(c.id)}
        >
          <span class="swatch-dot" style="--swatch-color: {c.cssVar}">
            {#if seleccionado}
              <Check size={11} strokeWidth={3} />
            {/if}
          </span>
          <span class="swatch-label">{c.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/snippet}

{#if modalActivo}
  <dialog
    bind:this={dialogEl}
    class={modalActivo === "defensores" || modalActivo === "portabilidad"
      ? "modal-wide"
      : ["crear", "crear_carpeta", "mover_archivo", "editar_archivo"].includes(modalActivo)
        ? "modal-overflow-visible"
        : ""}
    aria-labelledby="dialog-heading"
    oncancel={(e) => {
      const until = Number(dialogEl?.dataset.blockCancelUntil || 0);
      if (Date.now() <= until) {
        e.preventDefault();
      }
    }}
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
          <div class="field-header">
            <label class="field-label" for="nuevo-archivo-defensor">
              Defensor Público asignado
            </label>
          </div>
          <span id="nuevo-archivo-defensor-hint" class="shortcut-hint">
            {#if defensores.length > 0}
              Obligatorio: elige un defensor de tu lista o escribe un nombre nuevo. La IA aprenderá su estilo propio.
            {:else}
              Obligatorio: escribe el nombre del defensor y se guardará en tu directorio para personalizar la IA.
            {/if}
          </span>
          <AutocompleteInput
            id="nuevo-archivo-defensor"
            bind:value={defensorNuevoArchivo}
            suggestions={defensores}
            invalid={Boolean(errorModalDefensor)}
            showDropdownButton={defensores.length > 0}
            headerText="Defensores guardados en tu directorio"
            allowEmptyOption={false}
            describedBy={errorModalDefensor
              ? "nuevo-archivo-defensor-hint nuevo-archivo-defensor-error"
              : "nuevo-archivo-defensor-hint"}
            maxSuggestions={100}
            autocorrect="off"
            spellcheck="false"
            placeholder={defensores.length > 0
              ? "Elige de tu lista o escribe un nombre nuevo"
              : "Escribe el nombre del Defensor Público"}
            onSelect={(val) => onDefensorNuevoChange?.(val)}
            onInput={(val) => onDefensorNuevoChange?.(val)}
            onEditOption={onRenombrarDefensorDirecto}
            onDeleteOption={onEliminarDefensorDirecto}
            onEnter={onCrearNuevoArchivo}
          />
          {#if errorModalDefensor}
            <p
              id="nuevo-archivo-defensor-error"
              role="alert"
              style="color: var(--status-no); font-size: 12px"
            >
              {errorModalDefensor}
            </p>
          {/if}
          <span class="shortcut-hint">
            Recomendación: escribe el nombre completo sin títulos (Dr./Dra.) para que la IA reconozca siempre al mismo defensor.
          </span>
        </div>

        <div class="field-group">
          <div class="field-header">
            <label class="field-label" for="nuevo-archivo-input">
              Nombre de la planilla Excel
            </label>
            {#if sugerenciaNombreCrear && nombreNuevoArchivo.trim() !== sugerenciaNombreCrear}
              <button
                type="button"
                class="inline-action-link"
                onclick={() => onUsarFormatoAutomaticoCrear?.()}
              >
                Usar formato automático
              </button>
            {/if}
          </div>
          <input
            id="nuevo-archivo-input"
            type="text"
            class="input {errorModalCrear ? 'is-invalid' : ''}"
            bind:value={nombreNuevoArchivo}
            autocorrect="off"
            spellcheck="false"
            placeholder="Ej: Laura_Gomez_Noviembre_2026"
            oninput={(e) => onNombreNuevoInput?.(e.currentTarget.value)}
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                onCrearNuevoArchivo();
              }
            }}
          />
          {#if errorModalCrear}
            <p style="color: var(--status-no); font-size: 12px">{errorModalCrear}</p>
          {:else}
            <span class="shortcut-hint">
              Al elegir un defensor se completa automáticamente como <code>[DEFENSOR]_[MES]_[AÑO]</code> (editable).
            </span>
          {/if}
        </div>

        {#if carpetasDisponibles.length > 0 || carpetaNuevoArchivo}
          <div class="field-group">
            <label class="field-label" for="nuevo-archivo-carpeta">
              Carpeta de ubicación (opcional)
            </label>
            <CustomSelect
              id="nuevo-archivo-carpeta"
              bind:value={carpetaNuevoArchivo}
              options={opcionesCarpetas}
              placeholder="Carpeta principal (raíz)"
              searchPlaceholder="Filtrar carpeta..."
            />
          </div>
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onCrearNuevoArchivo}>
          Crear archivo
        </button>
      </div>

    {:else if modalActivo === "crear_carpeta"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <FolderPlus size={15} />
          <span>Nueva carpeta</span>
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
        {#if carpetasDisponibles.length > 0 || carpetaPadreNueva}
          <div class="field-group">
            <label class="field-label" for="carpeta-padre-select">
              Crear dentro de
            </label>
            <CustomSelect
              id="carpeta-padre-select"
              bind:value={carpetaPadreNueva}
              options={opcionesCarpetas}
              placeholder="Carpeta principal (raíz)"
              searchPlaceholder="Filtrar carpeta..."
            />
          </div>
        {/if}
        <div class="field-group">
          <label class="field-label" for="nueva-carpeta-input">
            Nombre de la carpeta o subruta
          </label>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            id="nueva-carpeta-input"
            type="text"
            class="input {errorModalCarpeta ? 'is-invalid' : ''}"
            bind:value={nombreNuevaCarpeta}
            autofocus
            autocorrect="off"
            spellcheck="false"
            placeholder="Ej: 2026 o 2026/Enero"
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                onCrearCarpeta();
              }
            }}
          />
          {#if errorModalCarpeta}
            <p style="color: var(--status-no); font-size: 12px">{errorModalCarpeta}</p>
          {:else}
            <span class="shortcut-hint">
              Puedes crear varios niveles a la vez separando con <code>/</code> (ej: <code>2026/Febrero</code>).
            </span>
          {/if}
        </div>
        {@render selectorColorCarpeta(
          colorNuevaCarpeta,
          (id) => (colorNuevaCarpeta = id),
        )}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onCrearCarpeta}>
          Crear carpeta
        </button>
      </div>

    {:else if modalActivo === "renombrar_carpeta"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <Pencil size={15} />
          <span>Renombrar o mover carpeta</span>
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
          Carpeta actual: <strong>{carpetaRenombrarActual}</strong>
        </p>
        <div class="field-group">
          <label class="field-label" for="renombrar-carpeta-input">
            Nuevo nombre o ruta de carpeta
          </label>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            id="renombrar-carpeta-input"
            type="text"
            class="input {errorModalCarpeta ? 'is-invalid' : ''}"
            bind:value={carpetaRenombrarNueva}
            autofocus
            autocorrect="off"
            spellcheck="false"
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                onRenombrarCarpeta();
              }
            }}
          />
          {#if errorModalCarpeta}
            <p style="color: var(--status-no); font-size: 12px">{errorModalCarpeta}</p>
          {/if}
        </div>
        {@render selectorColorCarpeta(
          colorCarpetaRenombrar,
          (id) => (colorCarpetaRenombrar = id),
        )}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onRenombrarCarpeta}>
          Guardar cambios
        </button>
      </div>

    {:else if modalActivo === "color_carpeta" && colorCarpetaTarget}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <Palette size={15} />
          <span>Color de carpeta</span>
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
          Carpeta: <strong>{colorCarpetaTarget}</strong>
        </p>
        {@render selectorColorCarpeta(
          colorCarpetaEditando,
          (id) => (colorCarpetaEditando = id),
        )}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onGuardarColorCarpeta}>
          Guardar color
        </button>
      </div>

    {:else if modalActivo === "mover_archivo"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <FolderInput size={15} />
          <span>Mover planilla de carpeta</span>
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
          Planilla: <strong>{archivoMoverTarget}</strong>
        </p>
        <div class="field-group">
          <label class="field-label" for="mover-carpeta-select">
            Carpeta de destino
          </label>
          <CustomSelect
            id="mover-carpeta-select"
            bind:value={carpetaDestinoMover}
            options={opcionesCarpetasMover}
            placeholder="Carpeta principal (raíz)"
            searchPlaceholder="Filtrar carpeta..."
          />
        </div>
        {#if carpetaDestinoMover === "__NUEVA__"}
          <div class="field-group">
            <label class="field-label" for="mover-carpeta-nueva-input">
              Nombre de la carpeta nueva
            </label>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              id="mover-carpeta-nueva-input"
              type="text"
              class="input {errorModalMover ? 'is-invalid' : ''}"
              bind:value={carpetaDestinoNueva}
              autofocus
              autocorrect="off"
              spellcheck="false"
              placeholder="Ej: 2026/Marzo"
              onkeydown={(e) => {
                if (!e.isComposing && e.key === "Enter") {
                  e.preventDefault();
                  onMoverArchivo();
                }
              }}
            />
          </div>
        {/if}
        {#if errorModalMover}
          <p style="color: var(--status-no); font-size: 12px">{errorModalMover}</p>
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={onMoverArchivo}>
          Mover aquí
        </button>
      </div>

    {:else if modalActivo === "editar_archivo" && archivoEditarTarget}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <Pencil size={15} />
          <span>Editar planilla Excel</span>
        </h2>
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Cerrar"
          disabled={guardandoEdicionArchivo}
          onclick={() => (modalActivo = null)}
        >
          <X size={15} />
        </button>
      </div>
      <div class="modal-body scroller">
        <p class="shortcut-hint">
          Planilla actual: <strong>{archivoEditarTarget}</strong>
        </p>
        <div class="field-group">
          <div class="field-header">
            <label class="field-label" for="editar-archivo-nombre">
              Nombre de la planilla Excel
            </label>
            {#if sugerenciaNombreEditar && archivoEditarNombre.trim() !== sugerenciaNombreEditar}
              <button
                type="button"
                class="inline-action-link"
                onclick={() => onUsarFormatoAutomaticoEditar?.()}
              >
                Usar formato automático
              </button>
            {/if}
          </div>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            id="editar-archivo-nombre"
            type="text"
            class="input {errorModalEditarArchivo ? 'is-invalid' : ''}"
            bind:value={archivoEditarNombre}
            autofocus
            autocorrect="off"
            spellcheck="false"
            placeholder="Ej: Laura_Gomez_Noviembre_2026"
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                onGuardarEdicionArchivo?.();
              }
            }}
          />
        </div>

        <div class="field-group">
          <div class="field-header">
            <label class="field-label" for="editar-archivo-defensor">
              Defensor Público asignado
            </label>
          </div>
          <AutocompleteInput
            id="editar-archivo-defensor"
            bind:value={archivoEditarDefensor}
            suggestions={defensores}
            showDropdownButton={defensores.length > 0}
            headerText="Defensores guardados en tu directorio"
            allowEmptyOption={false}
            maxSuggestions={100}
            autocorrect="off"
            spellcheck="false"
            placeholder={defensores.length > 0
              ? "Elige de tu lista o escribe un nombre nuevo"
              : "Escribe el nombre del Defensor Público"}
            onEditOption={onRenombrarDefensorDirecto}
            onDeleteOption={onEliminarDefensorDirecto}
            onEnter={onGuardarEdicionArchivo}
          />
          <span class="shortcut-hint">
            Recomendación: escribe el nombre completo sin títulos (Dr./Dra.) para que la IA reconozca siempre al mismo defensor.
          </span>
        </div>

        {#if carpetasDisponibles.length > 0 || archivoEditarCarpeta}
          <div class="field-group">
            <label class="field-label" for="editar-archivo-carpeta">
              Carpeta de ubicación
            </label>
            <CustomSelect
              id="editar-archivo-carpeta"
              bind:value={archivoEditarCarpeta}
              options={opcionesCarpetas}
              placeholder="Carpeta principal (raíz)"
              searchPlaceholder="Filtrar carpeta..."
            />
          </div>
        {/if}

        {#if errorModalEditarArchivo}
          <p role="alert" style="color: var(--status-no); font-size: 12px">
            {errorModalEditarArchivo}
          </p>
        {/if}
      </div>
      <div class="modal-footer">
        <button
          type="button"
          class="btn btn-subtle"
          disabled={guardandoEdicionArchivo}
          onclick={() => (modalActivo = null)}
        >
          Cancelar
        </button>
        <button
          type="button"
          class="btn btn-solid"
          disabled={guardandoEdicionArchivo}
          onclick={() => onGuardarEdicionArchivo?.()}
        >
          {guardandoEdicionArchivo ? "Guardando..." : "Guardar cambios"}
        </button>
      </div>

    {:else if modalActivo === "eliminar_archivo" && eliminarTargetNombre}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">Eliminar planilla Excel</h2>
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
          ¿Eliminar permanentemente <strong>{eliminarTargetNombre}</strong>? Esta acción borrará el archivo Excel y no se puede deshacer.
        </p>
        {#if eliminarTargetResumen}
          <div class="delete-stats-summary">
            <span>{eliminarTargetResumen.num_registros} audiencias registradas</span>
            <span>· {eliminarTargetResumen.total_si} realizadas · {eliminarTargetResumen.total_no} aplazadas</span>
          </div>
        {/if}
        <label class="delete-archive-check">
          <input type="checkbox" bind:checked={conservarEstadisticasAlEliminar} />
          <div>
            <strong>Conservar sus estadísticas en el historial global</strong>
            <div class="shortcut-hint">
              Sus totales por mes, defensor y causal seguirán apareciendo en Estadísticas aunque elimines el Excel.
            </div>
          </div>
        </label>
      </div>
      <div class="modal-footer">
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
        >
          Cancelar
        </button>
        <button
          type="button"
          class="btn btn-danger"
          onclick={onConfirmarEliminarArchivo}
        >
          Eliminar planilla
        </button>
      </div>

    {:else if modalActivo === "eliminar_carpeta" && eliminarCarpetaTarget}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <AlertTriangle size={15} style="color: var(--status-no)" />
          <span>Eliminar carpeta con contenido</span>
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
        <div class="folder-delete-warning">
          <strong>¡Advertencia!</strong> La carpeta
          <strong>{eliminarCarpetaTarget.carpeta}</strong> no está vacía:
          {#if eliminarCarpetaTarget.totalArchivos > 0}
            contiene <strong>{eliminarCarpetaTarget.totalArchivos}</strong>
            {eliminarCarpetaTarget.totalArchivos === 1 ? "planilla Excel" : "planillas Excel"}
          {/if}
          {#if eliminarCarpetaTarget.totalSubcarpetas > 0}
            {eliminarCarpetaTarget.totalArchivos > 0 ? " y " : "contiene "}
            <strong>{eliminarCarpetaTarget.totalSubcarpetas}</strong>
            {eliminarCarpetaTarget.totalSubcarpetas === 1 ? "subcarpeta" : "subcarpetas"}
          {/if}.
          Al confirmar se borrarán permanentemente todos esos archivos y sus respaldos.
        </div>
        {#if eliminarCarpetaTarget.totalArchivos > 0}
          <label class="delete-archive-check">
            <input type="checkbox" bind:checked={conservarEstadisticasAlEliminar} />
            <div>
              <strong>Conservar las estadísticas de estas planillas en el historial global</strong>
              <div class="shortcut-hint">
                Sus totales por mes, defensor y causales seguirán contando en Estadísticas aunque se borren los Excel.
              </div>
            </div>
          </label>
        {/if}
      </div>
      <div class="modal-footer">
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
        >
          Cancelar
        </button>
        <button
          type="button"
          class="btn btn-danger"
          onclick={onConfirmarEliminarCarpeta}
        >
          Eliminar carpeta y contenido
        </button>
      </div>

    {:else if modalActivo === "portabilidad"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <Package size={15} />
          <span>Exportar e Importar datos</span>
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
        <div class="segmented-control" role="tablist" aria-label="Exportar o Importar">
          <button
            type="button"
            role="tab"
            aria-selected={portabilidadTab === "exportar"}
            class="segmented-option {portabilidadTab === 'exportar' ? 'is-si' : ''}"
            onclick={() => (portabilidadTab = "exportar")}
          >
            <Download size={14} />
            <span>Exportar</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={portabilidadTab === "importar"}
            class="segmented-option {portabilidadTab === 'importar' ? 'is-si' : ''}"
            onclick={() => (portabilidadTab = "importar")}
          >
            <Upload size={14} />
            <span>Importar</span>
          </button>
        </div>

        {#if portabilidadTab === "exportar"}
          <p class="shortcut-hint">
            Elige qué deseas guardar en tu carpeta de Descargas. Cada planilla conserva su identificador único para no contar duplicados al reimportar.
          </p>

          <div class="portabilidad-options">
            <label class="portabilidad-card-option {tipoExportacion === 'completo' ? 'selected' : ''}">
              <input type="radio" name="tipo-exp" value="completo" bind:group={tipoExportacion} />
              <div>
                <div class="portabilidad-card-title">
                  <FileSpreadsheet size={14} class="excel-icon" />
                  <strong>Copia completa de Lokus (.lokus)</strong>
                </div>
                <p class="shortcut-hint">
                  Incluye todas tus carpetas, planillas Excel, estadísticas archivadas, tus listas (defensores, juzgados y tipos de audiencia) y el historial de uso de catálogos. Nunca incluye tus claves API.
                </p>
              </div>
            </label>

            <label class="portabilidad-card-option {tipoExportacion === 'estadisticas' ? 'selected' : ''}">
              <input type="radio" name="tipo-exp" value="estadisticas" bind:group={tipoExportacion} />
              <div>
                <div class="portabilidad-card-title">
                  <Archive size={14} />
                  <strong>Solo estadísticas (.lokus.json)</strong>
                </div>
                <p class="shortcut-hint">
                  Archivo liviano con los totales por mes, defensor y causal sin incluir los archivos Excel. Ideal para consolidar o compartir indicadores sin duplicar registros.
                </p>
              </div>
            </label>
          </div>
        {:else}
          <div class="field-group">
            <label class="import-drop-btn">
              <Upload size={15} />
              <span>
                {archivoImportNombre
                  ? `Archivo elegido: ${archivoImportNombre}`
                  : "Seleccionar respaldo (.lokus) o estadísticas (.lokus.json)..."}
              </span>
              <input
                type="file"
                accept=".lokus,.json,application/json,application/gzip"
                hidden
                onchange={manejarArchivoImportInput}
              />
            </label>
          </div>

          {#if errorImportacion}
            <p style="color: var(--status-no); font-size: 12.5px">{errorImportacion}</p>
          {/if}

          {#if inspeccionImportacion}
            <div class="import-preview-box">
              <div class="import-preview-head">
                <strong>
                  {inspeccionImportacion.tipo_paquete === "completo"
                    ? "Paquete detectado: Copia completa"
                    : "Paquete detectado: Solo estadísticas"}
                </strong>
                <span class="shortcut-hint">v{inspeccionImportacion.version_app}</span>
              </div>

              <div class="import-preview-stats">
                {#if inspeccionImportacion.tipo_paquete === "completo"}
                  <span>• Carpetas en el respaldo: <strong>{inspeccionImportacion.total_carpetas}</strong></span>
                  <span>
                    • Planillas Excel: <strong>{inspeccionImportacion.planillas_nuevas}</strong> nuevas
                    {#if inspeccionImportacion.planillas_conflicto_id + inspeccionImportacion.planillas_conflicto_ruta > 0}
                      · <strong>{inspeccionImportacion.planillas_conflicto_id + inspeccionImportacion.planillas_conflicto_ruta}</strong> ya existen aquí
                    {/if}
                  </span>
                {/if}

                {#if inspeccionImportacion.total_resumenes > 0}
                  <span>
                    • Resúmenes estadísticos: <strong>{inspeccionImportacion.resumenes_nuevos}</strong> nuevos
                    {#if inspeccionImportacion.resumenes_ya_en_disco > 0}
                      · <strong>{inspeccionImportacion.resumenes_ya_en_disco}</strong> ya están activos en tus Excel (no se duplicarán)
                    {/if}
                    {#if inspeccionImportacion.resumenes_ya_archivados > 0}
                      · <strong>{inspeccionImportacion.resumenes_ya_archivados}</strong> ya archivados
                    {/if}
                  </span>
                {/if}
              </div>

              {#if inspeccionImportacion.planillas_conflicto_id + inspeccionImportacion.planillas_conflicto_ruta + inspeccionImportacion.resumenes_ya_archivados > 0}
                <div class="field-group" style="margin-top: 6px">
                  <span class="field-label">Si algún archivo o resumen ya existe aquí:</span>
                  <div class="import-conflict-modes">
                    <label class="import-mode-row">
                      <input type="radio" name="modo-conflicto" value="omitir" bind:group={modoConflictoImport} />
                      <span><strong>Omitir repetidos (Recomendado)</strong> — agrega solo lo nuevo y evita duplicados</span>
                    </label>
                    <label class="import-mode-row">
                      <input type="radio" name="modo-conflicto" value="reemplazar" bind:group={modoConflictoImport} />
                      <span><strong>Reemplazar existentes</strong> — actualiza los repetidos guardando respaldo previo</span>
                    </label>
                    <label class="import-mode-row">
                      <input type="radio" name="modo-conflicto" value="conservar_ambas" bind:group={modoConflictoImport} />
                      <span><strong>Conservar ambas copias</strong> — guarda los repetidos como <code>_importado</code></span>
                    </label>
                  </div>
                </div>
              {/if}

              {#if inspeccionImportacion.trae_configuracion}
                <label class="delete-archive-check" style="margin-top: 4px">
                  <input type="checkbox" bind:checked={importarConfigPaquete} />
                  <div>
                    <strong>Fusionar listas guardadas</strong>
                    <div class="shortcut-hint">
                      Agrega sin borrar tus actuales: {inspeccionImportacion.defensores_en_paquete} defensores, {inspeccionImportacion.juzgados_en_paquete} juzgados y {inspeccionImportacion.tipos_en_paquete} tipos de audiencia.
                    </div>
                  </div>
                </label>
              {/if}
            </div>
          {/if}
        {/if}
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cerrar
        </button>
        {#if portabilidadTab === "exportar"}
          <button
            type="button"
            class="btn btn-solid"
            disabled={exportandoPaquete}
            onclick={onExportarPaquete}
          >
            <Download size={14} />
            <span>{exportandoPaquete ? "Exportando..." : "Exportar a Descargas"}</span>
          </button>
        {:else}
          <button
            type="button"
            class="btn btn-solid"
            disabled={!inspeccionImportacion || importandoPaquete}
            onclick={onEjecutarImportacion}
          >
            <Upload size={14} />
            <span>{importandoPaquete ? "Importando..." : "Confirmar importación"}</span>
          </button>
        {/if}
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

        <div class="ia-config-toggles">
          <label class="delete-archive-check">
            <input type="checkbox" bind:checked={settings.consolidacion_ia_activa} />
            <div>
              <strong>Consolidación periódica automática con IA</strong>
              <div class="shortcut-hint">
                Sintetiza en segundo plano patrones acumulados y reglas redundantes cuando el sistema detecta presión estadística.
              </div>
            </div>
          </label>

          <label class="delete-archive-check">
            <input type="checkbox" bind:checked={settings.usar_resumen_consolidado} />
            <div>
              <strong>Inyectar resumen consolidado al extraer</strong>
              <div class="shortcut-hint">
                Incluye el perfil sintético general y del defensor activo en el contexto enviado al modelo de IA.
              </div>
            </div>
          </label>
        </div>

        <details class="ia-diag-details">
          <summary class="ia-diag-summary">
            <span class="ia-diag-summary-left">
              <Activity size={13} />
              <span>Diagnóstico técnico de autoaprendizaje</span>
            </span>
            <span class="ia-diag-summary-badges">
              {#if diagnosticoIa}
                <span class="ia-diag-pill">
                  {(diagnosticoIa.reglas_por_estado?.activa ?? 0) +
                    (diagnosticoIa.reglas_por_estado?.prueba ?? 0)} activas
                </span>
                {#if diagnosticoIa.drift_campos_activos?.length > 0}
                  <span class="ia-diag-pill is-warn">
                    Drift ({diagnosticoIa.drift_campos_activos.length})
                  </span>
                {/if}
              {/if}
              <ChevronDown size={13} class="ia-diag-chevron" />
            </span>
          </summary>

          <div class="ia-diag-body">
            <div class="ia-diag-toolbar">
              <span class="shortcut-hint">Telemetría local anonimizada para inspección técnica</span>
              <div class="ia-diag-actions">
                {#if defensorArchivo.trim()}
                  <button
                    type="button"
                    class="btn btn-subtle ia-diag-refresh-btn"
                    disabled={consolidandoIa || !aiStatus.configured}
                    onclick={() => {
                      consolidandoAmbito = "defensor";
                      onConsolidarMemoriaIa?.(defensorArchivo.trim());
                    }}
                    title={!aiStatus.configured
                      ? "Configura una clave API de IA para consolidar"
                      : `Consolidar memoria de IA para ${defensorArchivo.trim()}`}
                  >
                    <span>
                      {consolidandoIa && consolidandoAmbito === "defensor"
                        ? "Consolidando..."
                        : "Consolidar defensor"}
                    </span>
                  </button>
                {/if}
                <button
                  type="button"
                  class="btn btn-subtle ia-diag-refresh-btn"
                  disabled={consolidandoIa || !aiStatus.configured}
                  onclick={() => {
                    consolidandoAmbito = "general";
                    onConsolidarMemoriaIa?.("*");
                  }}
                  title={!aiStatus.configured
                    ? "Configura una clave API de IA para consolidar"
                    : "Consolidar memoria general transversal (*)"}
                >
                  <span>
                    {consolidandoIa && consolidandoAmbito === "general"
                      ? "Consolidando..."
                      : "Consolidar general (*)"}
                  </span>
                </button>
                <button
                  type="button"
                  class="btn btn-subtle ia-diag-refresh-btn"
                  disabled={cargandoDiagnosticoIa || consolidandoIa}
                  onclick={() => onRefrescarDiagnosticoIa?.()}
                >
                  <RefreshCw size={12} />
                  <span>{cargandoDiagnosticoIa ? "Cargando..." : "Actualizar"}</span>
                </button>
              </div>
            </div>

            {#if !diagnosticoIa}
              <p class="ia-diag-empty">
                {cargandoDiagnosticoIa
                  ? "Cargando métricas de aprendizaje..."
                  : "Aún no hay telemetría registrada en esta sesión."}
              </p>
            {:else}
              <div class="ia-diag-metrics-row">
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Aciertos</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.aciertos_totales}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Correcciones</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.correcciones_totales}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Anclas pos.</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.ejemplos_positivos_totales}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Inducciones LLM</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.llamadas_induccion_llm_total}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Cola clúster</span>
                  <span class="ia-diag-metric-value">
                    {diagnosticoIa.clusters_pendientes}/{diagnosticoIa.clusters_listos}
                  </span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Perfiles def.</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.perfiles_defensores ?? 0}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Mapa conex.</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.conexiones_grafo ?? 0}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Consolidaciones</span>
                  <span class="ia-diag-metric-value">{diagnosticoIa.consolidaciones_totales ?? 0}</span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Presión global</span>
                  <span class="ia-diag-metric-value">
                    {(diagnosticoIa.presion_consolidacion_global ?? 0).toFixed(2)}
                  </span>
                </div>
                <div class="ia-diag-metric-box">
                  <span class="ia-diag-metric-label">Resp. aclarac.</span>
                  <span class="ia-diag-metric-value">
                    {Math.round((diagnosticoIa.tasa_respuesta_aclaraciones ?? 1) * 100)}%
                  </span>
                </div>
              </div>

              {#if diagnosticoIa.drift_campos_activos?.length > 0}
                <div class="ia-diag-drift-banner" role="status">
                  <AlertTriangle size={13} />
                  <span>
                    Deriva detectada (Page-Hinkley) en:
                    <strong>{diagnosticoIa.drift_campos_activos.join(", ")}</strong>
                  </span>
                </div>
              {/if}

              {#if diagnosticoIa.metricas_por_campo?.length > 0}
                <div class="ia-diag-section">
                  <h3 class="ia-diag-subtitle">Fiabilidad por campo</h3>
                  <div class="ia-diag-table-wrap">
                    <table class="ia-diag-table">
                      <thead>
                        <tr>
                          <th>Campo</th>
                          <th>Eval.</th>
                          <th>Aciertos</th>
                          <th>EWMA</th>
                          <th>Conf. q10</th>
                          <th>Estado</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each diagnosticoIa.metricas_por_campo as m}
                          <tr>
                            <td>{m.campo}</td>
                            <td class="num">{m.evaluaciones}</td>
                            <td class="num">{m.aciertos}</td>
                            <td class="num">{Math.round((m.precision_ewma ?? 0) * 100)}%</td>
                            <td class="num">{Math.round((m.confianza_q10 ?? 0) * 100)}%</td>
                            <td>
                              {#if m.drift_activo}
                                <span class="ia-diag-pill is-warn">Drift</span>
                              {:else}
                                <span class="ia-diag-pill is-ok">Estable</span>
                              {/if}
                            </td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                </div>
              {/if}

              {#if diagnosticoIa.metricas_por_modelo?.length > 0}
                <div class="ia-diag-section">
                  <h3 class="ia-diag-subtitle">Modelos evaluados</h3>
                  <div class="ia-diag-chips">
                    {#each diagnosticoIa.metricas_por_modelo as mod}
                      <span class="ia-diag-pill">
                        {mod.clave}: {Math.round((mod.precision_media ?? 0) * 100)}% ({mod.aciertos}/{mod.evaluaciones})
                      </span>
                    {/each}
                  </div>
                </div>
              {/if}

              {#if diagnosticoIa.preferencias_estilo}
                <div class="ia-diag-section">
                  <h3 class="ia-diag-subtitle">Heurísticas adaptables y alias</h3>
                  <div class="ia-diag-chips">
                    <span class="ia-diag-pill">
                      Causas sec.: {Math.round((diagnosticoIa.preferencias_estilo.incluir_causas_secundarias_prob ?? 0) * 100)}%
                    </span>
                    <span class="ia-diag-pill">
                      Reprogramada: {Math.round((diagnosticoIa.preferencias_estilo.incluir_reprogramacion_prob ?? 0) * 100)}%
                    </span>
                    <span class="ia-diag-pill">
                      "Se fija" &rarr; NO: {Math.round((diagnosticoIa.preferencias_estilo.se_fija_aplazamiento_prob ?? 0) * 100)}%
                    </span>
                    <span class="ia-diag-pill">
                      Instalada &rarr; SI: {Math.round((diagnosticoIa.preferencias_estilo.instalada_reprogramada_si_prob ?? 0) * 100)}%
                    </span>
                    <span class="ia-diag-pill">
                      Alias validados: {diagnosticoIa.alias_validados?.length ?? 0}
                    </span>
                    <span class="ia-diag-pill">
                      Patrones inferidos: {diagnosticoIa.patrones_abreviacion?.length ?? 0}
                    </span>
                  </div>
                </div>
              {/if}

              {#if diagnosticoIa.perfiles_sinteticos?.length > 0}
                <div class="ia-diag-section">
                  <h3 class="ia-diag-subtitle">Perfiles sintéticos consolidados</h3>
                  <ul class="ia-diag-rules-list">
                    {#each diagnosticoIa.perfiles_sinteticos as p}
                      <li class="ia-diag-rule-item">
                        <div class="ia-diag-rule-head">
                          <span class="ia-diag-pill {p.ambito === '*' ? 'is-ok' : ''}">
                            {p.ambito === "*" ? "General (transversal)" : (p.nombre_mostrar || p.ambito)}
                          </span>
                          <span class="ia-diag-rule-meta">
                            v{p.version ?? 1}{p.ultima_actualizacion ? ` · ${p.ultima_actualizacion.slice(0, 10)}` : ""}
                          </span>
                        </div>
                        <p class="ia-diag-rule-text">{p.resumen}</p>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}

              {#if diagnosticoIa.reglas_destacadas?.length > 0}
                <div class="ia-diag-section">
                  <h3 class="ia-diag-subtitle">Reglas en memoria ({diagnosticoIa.reglas_destacadas.length})</h3>
                  <ul class="ia-diag-rules-list">
                    {#each diagnosticoIa.reglas_destacadas as r}
                      <li class="ia-diag-rule-item">
                        <div class="ia-diag-rule-head">
                          <span
                            class="ia-diag-pill {r.estado === 'activa'
                              ? 'is-ok'
                              : r.estado === 'cuarentena'
                                ? 'is-err'
                                : r.estado === 'prueba'
                                  ? 'is-warn'
                                  : ''}"
                          >
                            {r.estado}
                          </span>
                          <span class="ia-diag-rule-meta">
                            {r.campo_objetivo} · q10={Math.round((r.confianza_q10 ?? 0) * 100)}% · {r.aciertos_directos}/{r.impresiones}
                          </span>
                        </div>
                        <p class="ia-diag-rule-text">{r.regla}</p>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
            {/if}
          </div>
        </details>
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
                  showDropdownButton={defensores.length > 0}
                  maxSuggestions={100}
                  headerText="Defensores guardados en tu directorio"
                  placeholder="Escribe, corrige o elige el defensor de esta planilla"
                  onEditOption={onRenombrarDefensorDirecto}
                  onDeleteOption={onEliminarDefensorDirecto}
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
            <span class="shortcut-hint">
              Recomendación: escribe el nombre completo sin títulos (Dr./Dra.) para que la IA reconozca siempre al mismo defensor.
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
          {#if !defensorTargetArchivo}
            <span class="shortcut-hint">
              Recomendación: escribe el nombre completo sin títulos (Dr./Dra.) para que la IA reconozca siempre al mismo defensor.
            </span>
          {/if}
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
          <div class="shortcut-row"><span>Mostrar / ocultar panel</span><span class="kbd">Ctrl+B</span></div>
          <div class="shortcut-row"><span>Refrescar / Cerrar</span><span class="kbd">F5 / Esc</span></div>
        </div>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-solid" onclick={() => (modalActivo = null)}>
          Cerrar
        </button>
      </div>
    {/if}
    {#if toast}
      <div class="toast" role="status" aria-live="polite">
        <span>{toast.mensaje}</span>
      </div>
    {/if}
  </dialog>
{/if}
