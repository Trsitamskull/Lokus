<script>
  import {
    Scale,
    Plus,
    X,
    FileSpreadsheet,
    UserCheck,
    History,
    Download,
    Trash2,
    Keyboard,
    Sun,
    Moon,
  } from "@lucide/svelte";

  let {
    archivosDisponibles = [],
    archivoActual = null,
    totales,
    aiStatus,
    theme = "dark",
    mobileOpen = false,
    onCloseMobile,
    onCrearArchivo,
    onSeleccionarArchivo,
    onAbrirDefensorArchivo,
    onAbrirRespaldos,
    onDescargarArchivo,
    onEliminarArchivo,
    onAbrirConfigIa,
    onAbrirAtajos,
    onToggleTema,
  } = $props();
</script>

{#if mobileOpen}
  <button
    type="button"
    class="sidebar-backdrop mobile-open"
    aria-label="Cerrar panel de planillas"
    onclick={onCloseMobile}
  ></button>
{/if}

<aside class="sidebar {mobileOpen ? 'mobile-open' : ''}">
  <div class="sidebar-brand">
    <div class="brand-id">
      <span class="brand-mark">
        <Scale size={14} strokeWidth={2.2} />
      </span>
      <span>Lokus Judicial</span>
    </div>
    <div class="sidebar-brand-actions">
      <button
        type="button"
        class="mini-icon-btn"
        onclick={onCrearArchivo}
        title="Crear nuevo archivo Excel (Ctrl+N)"
        aria-label="Crear nuevo archivo Excel"
      >
        <Plus size={16} strokeWidth={2.2} />
      </button>
      {#if mobileOpen}
        <button
          type="button"
          class="mini-icon-btn sidebar-close-btn"
          onclick={onCloseMobile}
          title="Ocultar panel"
          aria-label="Ocultar panel"
        >
          <X size={15} />
        </button>
      {/if}
    </div>
  </div>

  <div class="sidebar-section-header">
    <span>Planillas (.xlsx)</span>
    <span>{archivosDisponibles.length}</span>
  </div>

  <div class="sidebar-files scroller">
    {#if archivosDisponibles.length === 0}
      <div class="sidebar-empty">
        No hay planillas creadas.<br />
        Usa el botón <strong>+</strong> para crear la primera desde la plantilla de la Defensoría.
      </div>
    {:else}
      {#each archivosDisponibles as arch}
        {@const isActive = archivoActual === arch}
        <div class="sidebar-file-row {isActive ? 'active' : ''}">
          <button
            type="button"
            class="sidebar-file-btn"
            onclick={() => {
              onSeleccionarArchivo(arch);
              onCloseMobile?.();
            }}
          >
            <FileSpreadsheet size={15} strokeWidth={1.9} />
            <span class="file-name-truncate">{arch.replace(/\.xlsx$/i, "")}</span>
          </button>
          <div class="file-row-actions">
            <button
              type="button"
              class="mini-icon-btn"
              onclick={() => onAbrirDefensorArchivo(arch)}
              title="Asignar o editar defensor público"
              aria-label="Asignar o editar defensor público"
            >
              <UserCheck size={13} />
            </button>
            <button
              type="button"
              class="mini-icon-btn"
              onclick={() => onAbrirRespaldos(arch)}
              title="Historial de respaldos (últimas 3 versiones)"
              aria-label="Historial de respaldos"
            >
              <History size={13} />
            </button>
            <button
              type="button"
              class="mini-icon-btn"
              onclick={() => onDescargarArchivo(arch)}
              title="Exportar copia a Descargas"
              aria-label="Exportar copia a Descargas"
            >
              <Download size={13} />
            </button>
            <button
              type="button"
              class="mini-icon-btn danger"
              onclick={() => onEliminarArchivo(arch)}
              title="Eliminar archivo"
              aria-label="Eliminar archivo"
            >
              <Trash2 size={13} />
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <div class="sidebar-footer">
    {#if archivoActual}
      {@const pct =
        totales.num_registros > 0
          ? Math.round((totales.total_si / totales.num_registros) * 100)
          : 0}
      <div class="quota-box">
        <div class="quota-header">
          <span>Registros ({pct}% efectividad)</span>
          <strong>{totales.num_registros} / 100</strong>
        </div>
        <div class="quota-bar">
          <div
            class="quota-fill"
            style="width: {Math.min(100, totales.num_registros)}%"
          ></div>
        </div>
        <div class="quota-stats">
          <span><span class="stat-dot-si">●</span> {totales.total_si} realizadas</span>
          <span><span class="stat-dot-no">●</span> {totales.total_no} aplazadas</span>
        </div>
      </div>
    {/if}

    <div class="sidebar-bottom-bar">
      <button
        type="button"
        class="ai-config-trigger {aiStatus.configured ? '' : 'is-unconfigured'}"
        onclick={onAbrirConfigIa}
        title={aiStatus.configured ? "Configurar proveedor de IA" : "Falta configurar tu clave API de IA"}
      >
        <span class="led {aiStatus.configured ? 'on' : 'off'}"></span>
        <span class="ai-trigger-label">
          {#if aiStatus.configured}
            {aiStatus.provider === "gemini" ? "Gemini" : "OpenAI"} · {aiStatus.model}
          {:else}
            Configurar IA
          {/if}
        </span>
      </button>

      <div style="display: flex; gap: 3px; flex-shrink: 0">
        <button
          type="button"
          class="mini-icon-btn"
          onclick={onAbrirAtajos}
          title="Atajos de teclado (F1)"
          aria-label="Atajos de teclado"
        >
          <Keyboard size={14} />
        </button>
        <button
          type="button"
          class="mini-icon-btn"
          onclick={onToggleTema}
          title="Cambiar tema Azul Niebla / Oscuro (Ctrl+T)"
          aria-label="Cambiar tema"
        >
          {#if theme === "dark"}
            <Sun size={14} />
          {:else}
            <Moon size={14} />
          {/if}
        </button>
      </div>
    </div>
  </div>
</aside>
