<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import CustomSelect from "./CustomSelect.svelte";
  import {
    Scale,
    Plus,
    Search,
    Sparkles,
    Trash2,
    Download,
    CheckCircle2,
    XCircle,
    SlidersHorizontal,
    Keyboard,
    Sun,
    Moon,
    Edit3,
    Copy,
    Eye,
    EyeOff,
    X,
    FileSpreadsheet,
  } from "@lucide/svelte";

  const TIPOS_AUDIENCIA = [
    "Alegatos de conclusión",
    "Audiencia concentrada",
    "Audiencia de acusación",
    "Audiencia de conciliación",
    "Audiencia de control de legalidad",
    "Audiencia de individualización de pena",
    "Audiencia de imputación",
    "Audiencia de incidente de reparación integral",
    "Audiencia de juicio oral",
    "Audiencia de medidas de aseguramiento",
    "Audiencia de nulidad",
    "Audiencia de preclusión",
    "Audiencia de prórroga",
    "Audiencia de revisión de medida",
    "Audiencia de verificación de cumplimiento",
    "Audiencia preliminar",
    "Audiencia preparatoria",
    "Otra",
  ];

  const MOTIVOS_LABELS = [
    "Juez",
    "Fiscalía",
    "Usuario",
    "INPEC",
    "Víctima",
    "ICBF",
    "Defensor Confianza",
    "Defensor Público",
  ];

  const HORAS_RAPIDAS = [
    ["08", "00"],
    ["10", "00"],
    ["14", "00"],
    ["16", "00"],
  ];

  // Estado de navegación y archivos
  let vistaActiva = $state("formulario"); // "formulario" | "listado"
  let archivoActual = $state(null);
  let archivosDisponibles = $state([]);
  let registros = $state([]);
  let totales = $state({
    num_registros: 0,
    total_si: 0,
    total_no: 0,
    totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
  });

  // Estado del formulario
  let radicado = $state("");
  let juzgado = $state("");
  let tipoSeleccionado = $state("");
  let tipoOtra = $state("");
  let realizada = $state(""); // "SI" | "NO" | ""
  let fecha = $state("");
  let hora = $state("");
  let minuto = $state("");
  let motivoSeleccionado = $state("");
  let observaciones = $state("");

  // Estado de edición
  let modoEdicion = $state(false);
  let filaEditando = $state(null);
  let numeroEditando = $state(null);

  // Estado de modales (<dialog>)
  let modalActivo = $state(null); // "crear" | "ia" | "config_ia" | "atajos" | "confirmar_sin_hora"
  let dialogEl = $state(null);
  let nombreNuevoArchivo = $state("");
  let errorModalCrear = $state("");
  let filtroBusqueda = $state("");
  let filtroEstado = $state("TODOS"); // "TODOS" | "SI" | "NO"
  let filtroMotivo = $state(""); // "" o uno de MOTIVOS_LABELS

  // Estado IA y Configuración
  let textoIa = $state("");
  let procesandoIa = $state(false);
  let errorIa = $state("");
  let mostrarOpenAiKey = $state(false);
  let mostrarGeminiKey = $state(false);

  let settings = $state({
    provider: "openai",
    openai_api_key: "",
    openai_model: "gpt-5-mini",
    gemini_api_key: "",
    gemini_model: "gemini-2.5-flash",
    theme: "dark",
  });

  let aiStatus = $state({
    configured: false,
    provider: "openai",
    model: "gpt-5-mini",
    has_openai_key: false,
    has_gemini_key: false,
    masked_openai_key: "",
    masked_gemini_key: "",
  });

  let toast = $state(null);
  let toastTimer = null;
  let inputRadicadoEl = $state(null);

  $effect(() => {
    if (modalActivo && dialogEl && !dialogEl.open) {
      dialogEl.showModal();
    }
  });

  // Despachos frecuentes extraídos automáticamente de la planilla activa
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

  function mostrarToast(mensaje, tipo = "info", duracion = 3000) {
    if (toastTimer) clearTimeout(toastTimer);
    toast = { mensaje, tipo };
    toastTimer = setTimeout(() => {
      toast = null;
    }, duracion);
  }

  function aplicarTema(tema) {
    document.documentElement.setAttribute("data-theme", tema);
    const meta = document.querySelector('meta[name="color-scheme"]');
    if (meta) meta.content = tema;
    try {
      localStorage.setItem("color-scheme", tema);
    } catch (_) {}
    settings.theme = tema;
  }

  async function toggleTema() {
    const nuevo = settings.theme === "dark" ? "light" : "dark";
    aplicarTema(nuevo);
    try {
      aiStatus = await invoke("cmd_guardar_config", { settings });
    } catch (_) {}
  }

  async function cargarConfigInicial() {
    try {
      const res = await invoke("cmd_obtener_config");
      settings = res.settings;
      aiStatus = res.status;
      aplicarTema(settings.theme || "dark");
    } catch (_) {
      aplicarTema("dark");
    }
  }

  async function refrescarListaArchivos() {
    try {
      archivosDisponibles = await invoke("cmd_listar_archivos");
    } catch (_) {}
  }

  async function recargarArchivoActual() {
    if (!archivoActual) return;
    try {
      const res = await invoke("cmd_leer_archivo", { nombre: archivoActual });
      registros = res.registros;
      totales = res.totales;
    } catch (e) {
      mostrarToast(`Error leyendo archivo: ${e}`, "error");
    }
  }

  function limpiarFormulario() {
    radicado = "";
    juzgado = "";
    tipoSeleccionado = "";
    tipoOtra = "";
    realizada = "";
    fecha = "";
    hora = "";
    minuto = "";
    motivoSeleccionado = "";
    observaciones = "";
    inputRadicadoEl?.focus();
  }

  function cancelarEdicion() {
    modoEdicion = false;
    filaEditando = null;
    numeroEditando = null;
    limpiarFormulario();
  }

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

  function construirPayloadFormulario() {
    const tipoFinal =
      tipoSeleccionado === "Otra" ? tipoOtra.trim() : tipoSeleccionado.trim();

    const h = hora.trim();
    const m = minuto.trim();
    const horaFinal =
      !h && !m
        ? ""
        : `${(h || "00").padStart(2, "0")}:${(m || "00").padStart(2, "0")}`;

    return {
      radicado: radicado.trim(),
      tipo: tipoFinal,
      fecha: fecha.trim(),
      hora: horaFinal,
      juzgado: juzgado.trim(),
      realizada,
      motivo: realizada === "NO" ? motivoSeleccionado : "",
      observaciones: observaciones.trim(),
    };
  }

  async function guardarOActualizar(permitirHoraVacia = false) {
    if (!archivoActual) {
      mostrarToast("Selecciona o crea un archivo en el panel izquierdo", "error");
      return;
    }

    const datos = construirPayloadFormulario();

    try {
      if (modoEdicion && filaEditando) {
        totales = await invoke("cmd_actualizar_audiencia", {
          nombreArchivo: archivoActual,
          filaExcel: filaEditando,
          datos,
          permitirHoraVacia,
        });
        await recargarArchivoActual();
        modoEdicion = false;
        filaEditando = null;
        numeroEditando = null;
        limpiarFormulario();
        modalActivo = null;
        mostrarToast("Registro actualizado en Excel", "success");
      } else {
        totales = await invoke("cmd_guardar_audiencia", {
          nombreArchivo: archivoActual,
          datos,
          permitirHoraVacia,
        });
        await recargarArchivoActual();
        limpiarFormulario();
        modalActivo = null;
        mostrarToast("Audiencia registrada correctamente", "success");
      }
    } catch (err) {
      if (String(err).includes("HORA_VACIA_REQUIERE_CONFIRMACION")) {
        modalActivo = "confirmar_sin_hora";
      } else {
        mostrarToast(String(err), "error", 4000);
      }
    }
  }

  async function crearNuevoArchivo() {
    if (!nombreNuevoArchivo.trim()) {
      errorModalCrear = "Ingresa un nombre para el archivo";
      return;
    }
    try {
      const creado = await invoke("cmd_crear_archivo", {
        nombre: nombreNuevoArchivo.trim(),
      });
      await refrescarListaArchivos();
      archivoActual = creado;
      await recargarArchivoActual();
      modalActivo = null;
      nombreNuevoArchivo = "";
      errorModalCrear = "";
      mostrarToast(`Archivo '${creado}' creado`, "success");
    } catch (e) {
      errorModalCrear = String(e);
    }
  }

  async function seleccionarArchivo(nombre) {
    archivoActual = nombre;
    await recargarArchivoActual();
  }

  async function eliminarArchivo(nombre) {
    if (!confirm(`¿Eliminar permanentemente '${nombre}'?`)) return;
    try {
      await invoke("cmd_eliminar_archivo", { nombre });
      await refrescarListaArchivos();
      if (archivoActual === nombre) {
        archivoActual = archivosDisponibles[0] || null;
        if (archivoActual) {
          await recargarArchivoActual();
        } else {
          registros = [];
          totales = {
            num_registros: 0,
            total_si: 0,
            total_no: 0,
            totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
          };
        }
      }
      mostrarToast(`Archivo '${nombre}' eliminado`, "info");
    } catch (e) {
      mostrarToast(`Error al eliminar: ${e}`, "error");
    }
  }

  async function descargarArchivo(nombre) {
    try {
      const ruta = await invoke("cmd_descargar_archivo", { nombre });
      mostrarToast(`Copia exportada en: ${ruta}`, "success", 4200);
    } catch (e) {
      mostrarToast(`Error al exportar: ${e}`, "error");
    }
  }

  function cargarRegistroParaEdicion(reg) {
    limpiarFormulario();
    modoEdicion = true;
    filaEditando = reg.fila_excel;
    numeroEditando = reg.numero;

    radicado = reg.radicado;
    juzgado = reg.juzgado;

    if (TIPOS_AUDIENCIA.includes(reg.tipo) && reg.tipo !== "Otra") {
      tipoSeleccionado = reg.tipo;
      tipoOtra = "";
    } else if (reg.tipo) {
      tipoSeleccionado = "Otra";
      tipoOtra = reg.tipo;
    }

    fecha = reg.fecha;
    if (reg.hora && reg.hora.includes(":")) {
      const [h, m] = reg.hora.split(":");
      hora = h.padStart(2, "0");
      minuto = m.padStart(2, "0");
    } else {
      hora = "";
      minuto = "";
    }

    realizada = reg.realizada || "";
    motivoSeleccionado = reg.realizada === "NO" ? reg.motivo : "";
    observaciones = reg.observaciones || "";
    vistaActiva = "formulario";
  }

  function clonarBaseParaNuevaSesion(reg) {
    cancelarEdicion();
    radicado = reg.radicado;
    juzgado = reg.juzgado;
    if (TIPOS_AUDIENCIA.includes(reg.tipo) && reg.tipo !== "Otra") {
      tipoSeleccionado = reg.tipo;
      tipoOtra = "";
    } else if (reg.tipo) {
      tipoSeleccionado = "Otra";
      tipoOtra = reg.tipo;
    }
    vistaActiva = "formulario";
    mostrarToast("Radicado y despacho copiados para nueva sesión", "info", 2200);
  }

  async function eliminarRegistroFila(reg) {
    if (!confirm(`¿Eliminar el registro #${reg.numero} (${reg.radicado})?`))
      return;
    try {
      totales = await invoke("cmd_eliminar_registro", {
        nombreArchivo: archivoActual,
        filaExcel: reg.fila_excel,
      });
      await recargarArchivoActual();
      mostrarToast(`Registro #${reg.numero} eliminado`, "info");
    } catch (e) {
      mostrarToast(`Error eliminando registro: ${e}`, "error");
    }
  }

  function abrirAutocompletarIa() {
    if (!aiStatus.configured) {
      modalActivo = "config_ia";
      return;
    }
    errorIa = "";
    modalActivo = "ia";
  }

  async function procesarConIa() {
    if (!textoIa.trim()) {
      errorIa = "Pega el texto de la audiencia";
      return;
    }
    errorIa = "";
    procesandoIa = true;
    try {
      const datos = await invoke("cmd_autocompletar_ia", { texto: textoIa });
      if (datos.radicado) radicado = datos.radicado;
      if (datos.tipo_audiencia) {
        tipoSeleccionado = datos.tipo_audiencia;
        tipoOtra = datos.tipo_otra || "";
      }
      if (datos.fecha) fecha = datos.fecha;
      hora = datos.hora || "";
      minuto = datos.minuto || "";
      if (datos.juzgado) juzgado = datos.juzgado;
      if (datos.se_realizo === "SI" || datos.se_realizo === "NO") {
        realizada = datos.se_realizo;
        motivoSeleccionado = datos.se_realizo === "NO" ? datos.motivo || "" : "";
      }
      if (datos.observaciones) observaciones = datos.observaciones;

      modalActivo = null;
      vistaActiva = "formulario";
      textoIa = "";
      mostrarToast("Datos extraídos al formulario", "success");
    } catch (e) {
      errorIa = String(e);
    } finally {
      procesandoIa = false;
    }
  }

  async function guardarConfiguracionIa() {
    try {
      aiStatus = await invoke("cmd_guardar_config", { settings });
      modalActivo = null;
      mostrarToast("Configuración guardada", "success");
    } catch (e) {
      mostrarToast(`Error: ${e}`, "error");
    }
  }

  function manejarTecladoGlobal(e) {
    if (e.isComposing) return;
    const key = e.key.toLowerCase();

    if (modalActivo === "ia" && e.ctrlKey && e.key === "Enter") {
      e.preventDefault();
      procesarConIa();
      return;
    }

    if (e.key === "Escape") {
      if (modalActivo) {
        modalActivo = null;
        return;
      }
      if (modoEdicion) {
        cancelarEdicion();
        return;
      }
    }

    if (e.key === "F1") {
      e.preventDefault();
      modalActivo = modalActivo === "atajos" ? null : "atajos";
      return;
    }

    if (e.key === "F5") {
      e.preventDefault();
      if (modoEdicion) {
        guardarOActualizar(false);
      } else {
        recargarArchivoActual();
        mostrarToast("Sincronizado con Excel", "info", 1400);
      }
      return;
    }

    if (!e.ctrlKey) return;

    if (e.shiftKey && key === "s") {
      e.preventDefault();
      if (archivoActual) descargarArchivo(archivoActual);
    } else if (key === "s") {
      e.preventDefault();
      guardarOActualizar(false);
    } else if (key === "n") {
      e.preventDefault();
      nombreNuevoArchivo = "";
      errorModalCrear = "";
      modalActivo = "crear";
    } else if (key === "f" || key === "e") {
      e.preventDefault();
      vistaActiva = vistaActiva === "listado" ? "formulario" : "listado";
    } else if (key === "i") {
      e.preventDefault();
      abrirAutocompletarIa();
    } else if (key === "l") {
      e.preventDefault();
      limpiarFormulario();
    } else if (key === "t") {
      e.preventDefault();
      toggleTema();
    }
  }

  onMount(async () => {
    await cargarConfigInicial();
    await refrescarListaArchivos();
    if (archivosDisponibles.length > 0 && !archivoActual) {
      archivoActual = archivosDisponibles[0];
      await recargarArchivoActual();
    }
  });
</script>

<svelte:window onkeydown={manejarTecladoGlobal} />

<div class="desktop-frame">
  <!-- PANEL LATERAL DE ARCHIVOS -->
  <aside class="sidebar">
    <div class="sidebar-brand">
      <div class="brand-id">
        <span class="brand-mark">
          <Scale size={14} strokeWidth={2.2} />
        </span>
        <span>Lokus Judicial</span>
      </div>
      <button
        type="button"
        class="mini-icon-btn"
        onclick={() => {
          nombreNuevoArchivo = "";
          errorModalCrear = "";
          modalActivo = "crear";
        }}
        title="Crear nuevo archivo Excel (Ctrl+N)"
        aria-label="Crear nuevo archivo Excel"
      >
        <Plus size={16} strokeWidth={2.2} />
      </button>
    </div>

    <div class="sidebar-section-header">
      <span>Planillas (.xlsx)</span>
      <span>{archivosDisponibles.length}</span>
    </div>

    <div class="sidebar-files">
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
              onclick={() => seleccionarArchivo(arch)}
            >
              <FileSpreadsheet size={15} strokeWidth={1.9} />
              <span class="file-name-truncate">{arch.replace(/\.xlsx$/i, "")}</span>
            </button>
            <div class="file-row-actions">
              <button
                type="button"
                class="mini-icon-btn"
                onclick={() => descargarArchivo(arch)}
                title="Exportar copia a Descargas"
                aria-label="Exportar copia a Descargas"
              >
                <Download size={13} />
              </button>
              <button
                type="button"
                class="mini-icon-btn danger"
                onclick={() => eliminarArchivo(arch)}
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
          class="ai-config-trigger"
          onclick={() => (modalActivo = "config_ia")}
          title="Configurar proveedor de IA"
        >
          <span class="led {aiStatus.configured ? 'on' : 'off'}"></span>
          <span>{aiStatus.provider === "gemini" ? "Gemini" : "OpenAI"} · {aiStatus.model}</span>
        </button>

        <div style="display: flex; gap: 3px">
          <button
            type="button"
            class="mini-icon-btn"
            onclick={() => (modalActivo = "atajos")}
            title="Atajos de teclado (F1)"
            aria-label="Atajos de teclado"
          >
            <Keyboard size={14} />
          </button>
          <button
            type="button"
            class="mini-icon-btn"
            onclick={toggleTema}
            title="Cambiar tema claro/oscuro (Ctrl+T)"
            aria-label="Cambiar tema"
          >
            {#if settings.theme === "dark"}
              <Sun size={14} />
            {:else}
              <Moon size={14} />
            {/if}
          </button>
        </div>
      </div>
    </div>
  </aside>

  <!-- ÁREA DE TRABAJO PRINCIPAL -->
  <div class="workspace-pane">
    <header class="workspace-header">
      <div class="header-left">
        <div class="view-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={vistaActiva === "formulario"}
            class="view-tab {vistaActiva === 'formulario' ? 'active' : ''}"
            onclick={() => (vistaActiva = "formulario")}
          >
            <span>Nueva Audiencia</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={vistaActiva === "listado"}
            class="view-tab {vistaActiva === 'listado' ? 'active' : ''}"
            onclick={() => (vistaActiva = "listado")}
          >
            <span>Registros en Planilla</span>
            <span class="tab-count">{totales.num_registros}</span>
          </button>
        </div>
      </div>

      <div class="header-right">
        <button
          type="button"
          class="btn btn-ai-warm"
          onclick={abrirAutocompletarIa}
          title="Autocompletar a partir de texto libre (Ctrl+I)"
        >
          <Sparkles size={14} strokeWidth={2} />
          <span>Autocompletar con texto</span>
        </button>
      </div>
    </header>

    <main class="workspace-scroll">
      {#if vistaActiva === "formulario"}
        <form
          class="dossier-sheet"
          onsubmit={(e) => {
            e.preventDefault();
            guardarOActualizar(false);
          }}
        >
          {#if modoEdicion}
            <div class="dossier-banner-edit">
              <span>Editando registro #{numeroEditando} (Fila {filaEditando} de {archivoActual})</span>
              <button type="button" class="btn btn-subtle" onclick={cancelarEdicion}>
                Cancelar edición (Esc)
              </button>
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

          <!-- FILA 4: CAUSAL (Solo aparece cuando se elige NO realizada) -->
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
            <button
              type="button"
              class="btn btn-subtle"
              onclick={limpiarFormulario}
            >
              Limpiar campos (Ctrl+L)
            </button>

            <div style="display: flex; gap: 10px; align-items: center">
              <span class="shortcut-hint">Ctrl + S</span>
              <button type="submit" class="btn btn-solid" style="padding: 8px 22px">
                <span>{modoEdicion ? "Guardar cambios" : "Registrar audiencia"}</span>
              </button>
            </div>
          </div>

          <!-- ACTIVIDAD RECIENTE EN VIVO (Últimos 3 registros de la planilla) -->
          {#if registros.length > 0 && !modoEdicion}
            <div class="recent-records-strip">
              <div class="field-header">
                <span class="shortcut-hint">Últimos registros en {archivoActual}</span>
                <button
                  type="button"
                  class="inline-action-link"
                  onclick={() => (vistaActiva = "listado")}
                >
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
                      onclick={() => cargarRegistroParaEdicion(r)}
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
      {:else}
        <!-- VISTA DE TABLA INTEGRADA + DESGLOSE INTERACTIVO DE FILA 111 -->
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

          <!-- FILTROS RÁPIDOS POR CAUSAL (Totales Fila 111 del Excel) -->
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

          <div class="records-sheet">
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
                      <td style="color: var(--text-muted)">{reg.numero}</td>
                      <td style="font-weight: 600">{reg.radicado}</td>
                      <td>{reg.tipo}</td>
                      <td style="font-variant-numeric: tabular-nums">{reg.fecha}</td>
                      <td style="font-variant-numeric: tabular-nums">{reg.hora || "—"}</td>
                      <td>{reg.juzgado}</td>
                      <td>
                        {#if reg.realizada === "SI"}
                          <span class="tag-si">SI</span>
                        {:else}
                          <span class="tag-no">NO</span>
                        {/if}
                      </td>
                      <td>{reg.motivo || "—"}</td>
                      <td style="text-align: right">
                        <div style="display: inline-flex; gap: 4px">
                          <button
                            type="button"
                            class="mini-icon-btn"
                            onclick={() => clonarBaseParaNuevaSesion(reg)}
                            title="Usar este radicado y despacho para nueva audiencia"
                            aria-label="Clonar proceso para nueva audiencia"
                          >
                            <Copy size={13} />
                          </button>
                          <button
                            type="button"
                            class="mini-icon-btn"
                            onclick={() => cargarRegistroParaEdicion(reg)}
                            title="Editar este registro"
                            aria-label="Editar este registro"
                          >
                            <Edit3 size={14} />
                          </button>
                          <button
                            type="button"
                            class="mini-icon-btn danger"
                            onclick={() => eliminarRegistroFila(reg)}
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
          </div>
        </div>
      {/if}
    </main>
  </div>
</div>

<!-- MODALES NATIVOS (<dialog>) -->
{#if modalActivo}
  <dialog
    bind:this={dialogEl}
    aria-labelledby="dialog-heading"
    onclose={() => (modalActivo = null)}
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
      <div class="modal-body">
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
                crearNuevoArchivo();
              }
            }}
          />
          {#if errorModalCrear}
            <p style="color: var(--status-no); font-size: 12px">{errorModalCrear}</p>
          {/if}
        </div>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Cancelar
        </button>
        <button type="button" class="btn btn-solid" onclick={crearNuevoArchivo}>
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
      <div class="modal-body">
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
          onclick={() => (modalActivo = "config_ia")}
          disabled={procesandoIa}
        >
          Ajustes de IA
        </button>
        <button
          type="button"
          class="btn btn-solid"
          onclick={procesarConIa}
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
      <div class="modal-body">
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
              <label class="field-label" for="openai-model">Modelo</label>
              <div style="display: flex; gap: 4px">
                {#each ["gpt-5-mini", "gpt-4o-mini", "gpt-4o"] as m}
                  <button
                    type="button"
                    class="preset-btn"
                    onclick={() => (settings.openai_model = m)}
                  >
                    {m}
                  </button>
                {/each}
              </div>
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
              Clave API de OpenAI {aiStatus.has_openai_key ? `(${aiStatus.masked_openai_key})` : ""}
            </label>
            <div class="input-with-icon">
              <input
                id="openai-key"
                type={mostrarOpenAiKey ? "text" : "password"}
                class="input has-trailing"
                bind:value={settings.openai_api_key}
                autocorrect="off"
                autocapitalize="none"
                spellcheck="false"
                placeholder="sk-..."
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
              <label class="field-label" for="gemini-model">Modelo</label>
              <div style="display: flex; gap: 4px">
                {#each ["gemini-2.5-flash", "gemini-2.0-flash"] as m}
                  <button
                    type="button"
                    class="preset-btn"
                    onclick={() => (settings.gemini_model = m)}
                  >
                    {m}
                  </button>
                {/each}
              </div>
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
              Clave API de Gemini {aiStatus.has_gemini_key ? `(${aiStatus.masked_gemini_key})` : ""}
            </label>
            <div class="input-with-icon">
              <input
                id="gemini-key"
                type={mostrarGeminiKey ? "text" : "password"}
                class="input has-trailing"
                bind:value={settings.gemini_api_key}
                autocorrect="off"
                autocapitalize="none"
                spellcheck="false"
                placeholder="AIza..."
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
        <button type="button" class="btn btn-solid" onclick={guardarConfiguracionIa}>
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
      <div class="modal-body">
        <p style="color: var(--text-secondary)">
          No especificaste una hora. ¿Deseas registrar la audiencia dejando vacía la columna de hora en el Excel?
        </p>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn btn-subtle" onclick={() => (modalActivo = null)}>
          Volver
        </button>
        <button
          type="button"
          class="btn btn-solid"
          onclick={() => guardarOActualizar(true)}
        >
          Confirmar y guardar
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
      <div class="modal-body">
        <div class="shortcut-grid">
          <div class="shortcut-row"><span>Guardar audiencia</span><span class="kbd">Ctrl+S</span></div>
          <div class="shortcut-row"><span>Autocompletar IA</span><span class="kbd">Ctrl+I</span></div>
          <div class="shortcut-row"><span>Nuevo archivo</span><span class="kbd">Ctrl+N</span></div>
          <div class="shortcut-row"><span>Alternar vista tabla</span><span class="kbd">Ctrl+F</span></div>
          <div class="shortcut-row"><span>Limpiar campos</span><span class="kbd">Ctrl+L</span></div>
          <div class="shortcut-row"><span>Exportar copia</span><span class="kbd">Ctrl+Shift+S</span></div>
          <div class="shortcut-row"><span>Cambiar tema</span><span class="kbd">Ctrl+T</span></div>
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

{#if toast}
  <div class="toast" role="status" aria-live="polite">
    <span>{toast.mensaje}</span>
  </div>
{/if}
