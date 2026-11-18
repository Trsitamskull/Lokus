<script>
  import { Search, Copy, Edit3, Trash2 } from "@lucide/svelte";
  import { MOTIVOS_LABELS } from "../constants.js";

  let {
    registros = [],
    totales,
    filtroBusqueda = $bindable(""),
    filtroEstado = $bindable("TODOS"),
    filtroMotivo = $bindable(""),
    onClonar,
    onEditar,
    onEliminar,
  } = $props();

  let registrosFiltrados = $derived.by(() => {
    const q = filtroBusqueda.trim().toLowerCase();
    return registros.filter((r) => {
      if (filtroEstado !== "TODOS" && r.realizada !== filtroEstado) return false;
      if (filtroMotivo && r.motivo !== filtroMotivo) return false;
      if (!q) return true;
      return (
        r.radicado.toLowerCase().includes(q) ||
        r.juzgado.toLowerCase().includes(q) ||
        r.tipo.toLowerCase().includes(q) ||
        r.fecha.toLowerCase().includes(q) ||
        r.motivo.toLowerCase().includes(q) ||
        r.observaciones.toLowerCase().includes(q)
      );
    });
  });
</script>

<div class="table-view-container">
  <search class="table-Command-bar">
    <div class="search-box-wrap">
      <span class="search-box-icon">
        <Search size={14} />
      </span>
      <input
        type="search"
        class="input search-input"
        bind:value={filtroBusqueda}
        autocorrect="off"
        spellcheck="false"
        placeholder="Buscar por radicado, procesado, juzgado, fecha o causal..."
      />
    </div>

    <div class="filter-pills">
      <button
        type="button"
        class="btn {filtroEstado === 'TODOS' && !filtroMotivo ? 'btn-solid' : 'btn-subtle'}"
        onclick={() => {
          filtroEstado = "TODOS";
          filtroMotivo = "";
        }}
      >
        Todos ({registros.length})
      </button>
      <button
        type="button"
        class="btn {filtroEstado === 'SI' ? 'btn-solid' : 'btn-subtle'}"
        onclick={() => {
          filtroEstado = "SI";
          filtroMotivo = "";
        }}
      >
        Realizadas ({totales.total_si})
      </button>
      <button
        type="button"
        class="btn {filtroEstado === 'NO' ? 'btn-solid' : 'btn-subtle'}"
        onclick={() => (filtroEstado = "NO")}
      >
        No realizadas ({totales.total_no})
      </button>
    </div>
  </search>

  {#if totales.total_no > 0}
    <div class="motivos-summary-strip">
      <span class="shortcut-hint" style="margin-right: 4px">Filtrar por causal:</span>
      {#each MOTIVOS_LABELS as label, idx}
        {@const count = totales.totales_motivos[idx] ?? 0}
        {#if count > 0}
          <button
            type="button"
            class="causal-filter-chip {filtroMotivo === label ? 'active' : ''}"
            onclick={() => {
              if (filtroMotivo === label) {
                filtroMotivo = "";
              } else {
                filtroEstado = "NO";
                filtroMotivo = label;
              }
            }}
          >
            <span>{label}</span>
            <strong>{count}</strong>
          </button>
        {/if}
      {/each}
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <section class="records-sheet scroller" tabindex="0" aria-label="Registros en la planilla">
    <table class="records-table">
      <thead>
        <tr>
          <th>N°</th>
          <th>Radicado / Procesado</th>
          <th>Tipo de Audiencia</th>
          <th>Fecha</th>
          <th>Hora</th>
          <th>Despacho</th>
          <th>Estado</th>
          <th>Causal</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#if registrosFiltrados.length === 0}
          <tr>
            <td colspan="9" style="text-align: center; padding: 36px; color: var(--text-muted)">
              No hay audiencias que coincidan con el filtro actual.
            </td>
          </tr>
        {:else}
          {#each registrosFiltrados as reg}
            <tr>
              <td data-label="N°" style="color: var(--text-muted); font-variant-numeric: tabular-nums">{reg.numero}</td>
              <td data-label="Radicado" style="font-weight: 600">{reg.radicado}</td>
              <td data-label="Audiencia">{reg.tipo}</td>
              <td data-label="Fecha" style="font-variant-numeric: tabular-nums">{reg.fecha}</td>
              <td data-label="Hora" style="font-variant-numeric: tabular-nums">{reg.hora || "—"}</td>
              <td data-label="Despacho">{reg.juzgado}</td>
              <td data-label="Estado">
                {#if reg.realizada === "SI"}
                  <span class="tag-si">SI</span>
                {:else}
                  <span class="tag-no">NO</span>
                {/if}
              </td>
              <td data-label="Causal">{reg.motivo || "—"}</td>
              <td data-label="Acciones" style="text-align: right">
                <div style="display: inline-flex; gap: 4px">
                  <button
                    type="button"
                    class="mini-icon-btn"
                    onclick={() => onClonar(reg)}
                    title="Usar este radicado y despacho para nueva audiencia"
                    aria-label="Clonar proceso para nueva audiencia"
                  >
                    <Copy size={13} />
                  </button>
                  <button
                    type="button"
                    class="mini-icon-btn"
                    onclick={() => onEditar(reg)}
                    title="Editar este registro"
                    aria-label="Editar este registro"
                  >
                    <Edit3 size={14} />
                  </button>
                  <button
                    type="button"
                    class="mini-icon-btn danger"
                    onclick={() => onEliminar(reg)}
                    title="Eliminar fila"
                    aria-label="Eliminar fila"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </section>

  <div class="fila111-footer">
    <span class="fila111-title">Totales de la planilla</span>
    <div class="fila111-metrics">
      <span style="color: var(--status-si)">Realizadas: {totales.total_si}</span>
      <span style="color: var(--status-no)">Aplazadas: {totales.total_no}</span>
      <span style="color: var(--text-secondary)">
        {MOTIVOS_LABELS.map((m, idx) => `${m}: ${totales.totales_motivos[idx] ?? 0}`).join(" · ")}
      </span>
    </div>
  </div>
</div>
