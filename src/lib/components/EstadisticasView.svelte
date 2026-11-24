<script>
  import {
    ChevronLeft,
    ChevronRight,
    Copy,
    Search,
    RefreshCw,
    Archive,
    Trash2,
  } from "@lucide/svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import { MOTIVOS_LABELS } from "../constants.js";

  let {
    stats,
    cargando = false,
    mesSeleccionado = $bindable(""), // "" = todos, o "YYYY-MM"
    archivoFiltro = $bindable(""), // "" = todas las planillas
    defensorFiltro = $bindable(""), // "" = todos los defensores
    onFiltrarChange,
    onAbrirPlanilla,
    onCopiarResumen,
    onEliminarArchivada = undefined,
  } = $props();

  let busquedaDefensor = $state("");
  let verArchivadas = $state(false);

  let planillasArchivadas = $derived(
    (stats?.planillas || []).filter((p) => p.archivada)
  );

  let causalPrincipal = $derived.by(() => {
    const motivos = stats?.totales_motivos || [];
    let maxIdx = -1;
    let maxVal = 0;
    for (let i = 0; i < motivos.length; i++) {
      if (motivos[i] > maxVal) {
        maxVal = motivos[i];
        maxIdx = i;
      }
    }
    if (maxIdx < 0) return { nombre: "Sin aplazamientos", total: 0, pct: 0 };
    const totalNo = stats?.total_no ?? 0;
    const pct = totalNo > 0 ? ((maxVal / totalNo) * 100).toFixed(1) : "0.0";
    return { nombre: MOTIVOS_LABELS[maxIdx], total: maxVal, pct };
  });

  let causalesOrdenadas = $derived.by(() => {
    const motivos = stats?.totales_motivos || [0, 0, 0, 0, 0, 0, 0, 0];
    const totalNo = stats?.total_no ?? 0;
    const items = MOTIVOS_LABELS.map((label, idx) => ({
      idx,
      label,
      total: motivos[idx] || 0,
      pct: totalNo > 0 ? (motivos[idx] / totalNo) * 100 : 0,
    }));
    items.sort((a, b) => b.total - a.total || a.idx - b.idx);
    return items;
  });

  let maxCausal = $derived(
    Math.max(1, ...causalesOrdenadas.map((c) => c.total))
  );

  let defensoresFiltrados = $derived.by(() => {
    const lista = stats?.por_defensor || [];
    const q = busquedaDefensor.trim().toLowerCase();
    if (!q) return lista;
    return lista.filter(
      (d) =>
        d.defensor.toLowerCase().includes(q) ||
        d.archivos.some((a) => a.toLowerCase().includes(q)) ||
        d.motivo_frecuente.toLowerCase().includes(q)
    );
  });

  let mesesDisponibles = $derived(stats?.meses_disponibles || []);

  let opcionesMes = $derived([
    { value: "", label: "Todos los meses" },
    ...mesesDisponibles.map((m) => ({
      value: `${m.anio}-${String(m.mes).padStart(2, "0")}`,
      label: `${m.etiqueta} (${m.cantidad})`,
    })),
  ]);

  let opcionesPlanilla = $derived.by(() => {
    const lista = (stats?.planillas || []).filter((p) => {
      if (!defensorFiltro) return true;
      if (defensorFiltro === "__SIN_ASIGNAR__") return !p.defensor;
      return p.defensor.toLowerCase() === defensorFiltro.toLowerCase();
    });
    return [
      {
        value: "",
        label: `Todas las planillas (${lista.length})`,
      },
      ...lista.map((p) => ({
        value: p.archivo,
        label: `${p.archivo.replace(/\.xlsx$/i, "")}${p.defensor ? ` · ${p.defensor}` : ""}${p.archivada ? " (archivada)" : ""}`,
      })),
    ];
  });

  let opcionesDefensor = $derived.by(() => {
    const lista = stats?.defensores_disponibles || [];
    const haySinAsignar = (stats?.planillas || []).some((p) => !p.defensor);
    const opts = [
      {
        value: "",
        label: `Todos los defensores (${lista.length})`,
      },
      ...lista.map((d) => ({ value: d, label: d })),
    ];
    if (haySinAsignar) {
      opts.push({ value: "__SIN_ASIGNAR__", label: "Sin defensor asignado" });
    }
    return opts;
  });

  function manejarCambioDefensor(nuevoDef) {
    if (archivoFiltro) {
      const p = (stats?.planillas || []).find((x) => x.archivo === archivoFiltro);
      const coincide =
        !nuevoDef ||
        (nuevoDef === "__SIN_ASIGNAR__"
          ? p && !p.defensor
          : p && p.defensor.toLowerCase() === nuevoDef.toLowerCase());
      if (!coincide) archivoFiltro = "";
    }
    onFiltrarChange();
  }

  function manejarCambioPlanilla(nuevaPlanilla) {
    if (nuevaPlanilla && defensorFiltro) {
      const p = (stats?.planillas || []).find((x) => x.archivo === nuevaPlanilla);
      const coincide =
        defensorFiltro === "__SIN_ASIGNAR__"
          ? p && !p.defensor
          : p && p.defensor.toLowerCase() === defensorFiltro.toLowerCase();
      if (!coincide) defensorFiltro = "";
    }
    onFiltrarChange();
  }

  let indiceMesActual = $derived.by(() => {
    if (!mesSeleccionado) return -1;
    return mesesDisponibles.findIndex(
      (m) => `${m.anio}-${String(m.mes).padStart(2, "0")}` === mesSeleccionado
    );
  });

  function moverMes(delta) {
    if (mesesDisponibles.length === 0) return;
    const nextIdx = indiceMesActual === -1 ? 0 : indiceMesActual + delta;
    if (nextIdx < 0 || nextIdx >= mesesDisponibles.length) return;
    const target = mesesDisponibles[nextIdx];
    mesSeleccionado = `${target.anio}-${String(target.mes).padStart(2, "0")}`;
    onFiltrarChange();
  }
</script>

<div class="stats-view">
  <div class="stats-topbar">
    <div>
      <h1 class="dossier-title">Estadísticas Mensuales</h1>
      <p class="dossier-subtitle">
        Consolidado independiente de tus planillas Excel y defensores públicos
      </p>
    </div>

    <div class="stats-filters">
      <div class="month-stepper">
        <button
          type="button"
          class="mini-icon-btn"
          title="Mes anterior (más antiguo)"
          aria-label="Mes más antiguo"
          disabled={mesesDisponibles.length === 0 || indiceMesActual >= mesesDisponibles.length - 1}
          onclick={() => moverMes(1)}
        >
          <ChevronLeft size={14} />
        </button>

        <div class="stats-select-month">
          <CustomSelect
            id="stats-mes-select"
            compact
            searchable={mesesDisponibles.length > 5}
            searchPlaceholder="Buscar mes..."
            bind:value={mesSeleccionado}
            options={opcionesMes}
            placeholder="Todos los meses"
            onchange={onFiltrarChange}
          />
        </div>

        <button
          type="button"
          class="mini-icon-btn"
          title="Mes más reciente"
          aria-label="Mes más reciente"
          disabled={mesesDisponibles.length === 0 || indiceMesActual <= 0}
          onclick={() => moverMes(-1)}
        >
          <ChevronRight size={14} />
        </button>
      </div>

      <div class="stats-select-defensor">
        <CustomSelect
          id="stats-defensor-select"
          compact
          searchPlaceholder="Filtrar defensor..."
          bind:value={defensorFiltro}
          options={opcionesDefensor}
          placeholder="Todos los defensores"
          onchange={manejarCambioDefensor}
        />
      </div>

      <div class="stats-select-planilla">
        <CustomSelect
          id="stats-planilla-select"
          compact
          searchPlaceholder="Filtrar planilla..."
          bind:value={archivoFiltro}
          options={opcionesPlanilla}
          placeholder="Todas las planillas"
          onchange={manejarCambioPlanilla}
        />
      </div>

      <button
        type="button"
        class="btn btn-subtle"
        onclick={onFiltrarChange}
        disabled={cargando}
        title="Recalcular estadísticas desde el disco"
      >
        <RefreshCw size={13} />
        <span>{cargando ? "Calculando..." : "Actualizar"}</span>
      </button>

      <button
        type="button"
        class="btn btn-solid"
        onclick={onCopiarResumen}
        disabled={!stats || stats.total_audiencias === 0}
      >
        <Copy size={13} />
        <span>Copiar resumen</span>
      </button>
    </div>
  </div>

  {#if (stats?.total_archivadas ?? 0) > 0}
    <div class="stats-archive-banner">
      <div class="stats-archive-summary">
        <Archive size={14} />
        <span>
          Incluye <strong>{stats.total_archivadas}</strong>
          {stats.total_archivadas === 1 ? "planilla archivada" : "planillas archivadas"} sin archivo Excel en disco.
        </span>
      </div>
      <button
        type="button"
        class="inline-action-link"
        onclick={() => (verArchivadas = !verArchivadas)}
      >
        {verArchivadas ? "Ocultar lista" : "Gestionar archivadas"}
      </button>
    </div>

    {#if verArchivadas && planillasArchivadas.length > 0}
      <div class="stats-archive-list">
        {#each planillasArchivadas as arch (arch.id)}
          <div class="stats-archive-row">
            <div>
              <strong>{arch.archivo.replace(/\.xlsx$/i, "")}</strong>
              <span class="shortcut-hint">
                · {arch.num_registros} audiencias{arch.defensor ? ` · ${arch.defensor}` : ""}
              </span>
            </div>
            {#if onEliminarArchivada}
              <button
                type="button"
                class="mini-icon-btn danger"
                title="Eliminar resumen archivado del historial"
                aria-label="Eliminar resumen archivado {arch.archivo}"
                onclick={() => onEliminarArchivada(arch)}
              >
                <Trash2 size={13} />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}

  <!-- 4 TARJETAS KPI -->
  <div class="stats-kpi-grid">
    <div class="stats-kpi-card">
      <span class="stats-kpi-label">Total Audiencias</span>
      <span class="stats-kpi-value">{stats?.total_audiencias ?? 0}</span>
      <span class="stats-kpi-sub">
        {stats?.total_planillas_con_datos ?? 0} de {stats?.total_planillas ?? 0} planillas con datos
      </span>
    </div>

    <div class="stats-kpi-card">
      <span class="stats-kpi-label">Realizadas (SI)</span>
      <span class="stats-kpi-value is-si">{stats?.total_si ?? 0}</span>
      <span class="stats-kpi-sub">
        Efectividad: {(stats?.efectividad_pct ?? 0).toFixed(1)}%
      </span>
    </div>

    <div class="stats-kpi-card">
      <span class="stats-kpi-label">Aplazadas (NO)</span>
      <span class="stats-kpi-value is-no">{stats?.total_no ?? 0}</span>
      <span class="stats-kpi-sub">
        {stats?.total_audiencias > 0
          ? `${((stats.total_no / stats.total_audiencias) * 100).toFixed(1)}% del total`
          : "0.0% del total"}
      </span>
    </div>

    <div class="stats-kpi-card">
      <span class="stats-kpi-label">Causal Principal</span>
      <span class="stats-kpi-value" style="font-size: clamp(1.1rem, 1.2vw + 0.45rem, 1.4rem)">
        {causalPrincipal.nombre}
      </span>
      <span class="stats-kpi-sub">
        {#if causalPrincipal.total > 0}
          {causalPrincipal.total} casos ({causalPrincipal.pct}% de aplazadas)
        {:else}
          Sin aplazamientos en el periodo
        {/if}
      </span>
    </div>
  </div>

  <!-- DESGLOSE POR CAUSAL + TABLA DE 60+ DEFENSORES -->
  <div class="stats-split-grid">
    <div class="stats-panel">
      <div class="stats-panel-header">
        <h2 class="stats-panel-title">Atribución de Aplazamientos</h2>
        <span class="shortcut-hint">{stats?.total_no ?? 0} casos</span>
      </div>

      <div class="causal-bar-list">
        {#each causalesOrdenadas as item}
          {@const ancho = item.total > 0 ? Math.max(4, Math.round((item.total / maxCausal) * 100)) : 0}
          <div class="causal-bar-item">
            <div class="causal-bar-meta">
              <span style="font-weight: {item.total > 0 ? 600 : 400}; color: {item.total > 0 ? 'var(--text-primary)' : 'var(--text-muted)'}">
                {item.label}
              </span>
              <span style="color: var(--text-secondary)">
                {item.total} ({item.pct.toFixed(1)}%)
              </span>
            </div>
            <div class="causal-bar-track">
              <div class="causal-bar-fill" style="width: {ancho}%"></div>
            </div>
          </div>
        {/each}
      </div>
    </div>

    <div class="stats-panel">
      <div class="stats-panel-header">
        <div>
          <h2 class="stats-panel-title">Consolidado por Defensor Público</h2>
          <p class="shortcut-hint">
            Haz clic sobre un defensor para abrir su planilla
          </p>
        </div>
        <div class="search-box-wrap" style="max-width: 240px">
          <span class="search-box-icon">
            <Search size={13} />
          </span>
          <input
            type="search"
            class="input search-input"
            style="padding-top: 6px; padding-bottom: 6px; font-size: 12.5px"
            bind:value={busquedaDefensor}
            placeholder="Filtrar defensor o archivo..."
          />
        </div>
      </div>

      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <section class="stats-table-wrap scroller" tabindex="0" aria-label="Consolidado por defensor público">
        <table class="records-table">
          <thead>
            <tr>
              <th>Defensor / Planilla</th>
              <th>Total</th>
              <th>SI / NO</th>
              <th>Efect.</th>
              <th>Causal frecuente</th>
            </tr>
          </thead>
          <tbody>
            {#if defensoresFiltrados.length === 0}
              <tr>
                <td colspan="5" style="text-align: center; padding: 28px; color: var(--text-muted)">
                  {stats?.total_audiencias === 0
                    ? "No hay registros de audiencias en el periodo seleccionado."
                    : "Ningún defensor coincide con el filtro."}
                </td>
              </tr>
            {:else}
              {#each defensoresFiltrados as row}
                <tr
                  class="stats-defensor-row"
                  onclick={() => {
                    if (row.archivos[0]) onAbrirPlanilla(row.archivos[0]);
                  }}
                  title="Abrir {row.archivos.join(', ')}"
                >
                  <td>
                    <button
                      type="button"
                      class="inline-action-link"
                      style="text-align: left; text-decoration: none; color: var(--text-primary); display: block"
                      onclick={(e) => {
                        e.stopPropagation();
                        if (row.archivos[0]) onAbrirPlanilla(row.archivos[0]);
                      }}
                    >
                      {row.defensor}
                    </button>
                    <div class="shortcut-hint" style="font-size: 11px">
                      {row.archivos.map((a) => a.replace(/\.xlsx$/i, "")).join(", ")}
                    </div>
                  </td>
                  <td style="font-variant-numeric: tabular-nums; font-weight: 600">{row.total}</td>
                  <td style="font-variant-numeric: tabular-nums">
                    <span style="color: var(--status-si); font-weight: 600">{row.total_si}</span>
                    /
                    <span style="color: var(--status-no); font-weight: 600">{row.total_no}</span>
                  </td>
                  <td style="font-variant-numeric: tabular-nums; font-weight: 600">
                    {row.efectividad_pct.toFixed(1)}%
                  </td>
                  <td>{row.motivo_frecuente}</td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </section>
    </div>
  </div>
</div>
