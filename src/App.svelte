<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Sparkles, UserCheck } from "@lucide/svelte";
  import { TIPOS_AUDIENCIA } from "./lib/constants.js";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import AudienciaForm from "./lib/components/AudienciaForm.svelte";
  import RegistrosTable from "./lib/components/RegistrosTable.svelte";
  import Modals from "./lib/components/Modals.svelte";

  // Navegación y archivos
  let vistaActiva = $state("formulario"); // "formulario" | "listado"
  let archivoActual = $state(null);
  let archivosDisponibles = $state([]);
  let registros = $state([]);
  let defensorArchivo = $state("");
  let defensores = $state([]);
  let totales = $state({
    num_registros: 0,
    total_si: 0,
    total_no: 0,
    totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
  });

  // Formulario
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
  let inputRadicadoEl = $state(null);

  // Edición
  let modoEdicion = $state(false);
  let filaEditando = $state(null);
  let numeroEditando = $state(null);
  let radicadoEditando = $state("");

  // Modales y confirmaciones
  let modalActivo = $state(null);
  let nombreNuevoArchivo = $state("");
  let defensorNuevoArchivo = $state("");
  let errorModalCrear = $state("");
  let confirmacionPendiente = $state(null);
  let ejecutandoConfirmacion = $state(false);
  let duplicadoDetectado = $state(null);
  let ultimoPermitirHoraVacia = $state(false);

  // Filtros tabla
  let filtroBusqueda = $state("");
  let filtroEstado = $state("TODOS");
  let filtroMotivo = $state("");

  // Defensores y respaldos
  let filtroDefensores = $state("");
  let nuevoDefensorDirectorio = $state("");
  let defensorEditando = $state(null);
  let defensorEditandoValor = $state("");
  let defensorPlanillaDraft = $state("");
  let errorDefensores = $state("");
  let respaldosArchivo = $state([]);
  let respaldosTarget = $state("");
  let cargandoRespaldos = $state(false);

  // IA y configuración
  let textoIa = $state("");
  let procesandoIa = $state(false);
  let errorIa = $state("");
  let nuevaOpenAiKey = $state("");
  let nuevaGeminiKey = $state("");
  let mostrarOpenAiKey = $state(false);
  let mostrarGeminiKey = $state(false);

  let settings = $state({
    provider: "openai",
    openai_model: "gpt-5-mini",
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
      const res = await invoke("cmd_guardar_tema", { tema: nuevo });
      aiStatus = res.status;
    } catch (_) {}
  }

  async function cargarConfigInicial() {
    try {
      const res = await invoke("cmd_obtener_config");
      settings = {
        provider: res.provider || "openai",
        openai_model: res.openai_model || "gpt-5-mini",
        gemini_model: res.gemini_model || "gemini-2.5-flash",
        theme: res.theme || "dark",
      };
      defensores = res.defensores || [];
      aiStatus = res.status;
      aplicarTema(settings.theme);
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
      defensorArchivo = res.defensor || "";
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
    radicadoEditando = "";
    limpiarFormulario();
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

  async function guardarOActualizar(permitirHoraVacia = false, permitirDuplicado = false) {
    if (!archivoActual) {
      mostrarToast("Selecciona o crea un archivo en el panel izquierdo", "error");
      return;
    }

    ultimoPermitirHoraVacia = permitirHoraVacia;
    const datos = construirPayloadFormulario();

    try {
      if (modoEdicion && filaEditando) {
        totales = await invoke("cmd_actualizar_audiencia", {
          nombreArchivo: archivoActual,
          filaExcel: filaEditando,
          radicadoEsperado: radicadoEditando,
          datos,
          permitirHoraVacia,
          permitirDuplicado,
        });
        await recargarArchivoActual();
        modoEdicion = false;
        filaEditando = null;
        numeroEditando = null;
        radicadoEditando = "";
        duplicadoDetectado = null;
        limpiarFormulario();
        modalActivo = null;
        mostrarToast("Registro actualizado en Excel", "success");
      } else {
        totales = await invoke("cmd_guardar_audiencia", {
          nombreArchivo: archivoActual,
          datos,
          permitirHoraVacia,
          permitirDuplicado,
        });
        await recargarArchivoActual();
        duplicadoDetectado = null;
        limpiarFormulario();
        modalActivo = null;
        mostrarToast("Audiencia registrada correctamente", "success");
      }
    } catch (err) {
      const msg = String(err);
      if (msg.includes("HORA_VACIA_REQUIERE_CONFIRMACION")) {
        modalActivo = "confirmar_sin_hora";
      } else if (msg.startsWith("POSIBLE_DUPLICADO:")) {
        try {
          duplicadoDetectado = JSON.parse(msg.slice("POSIBLE_DUPLICADO:".length));
          modalActivo = "confirmar_duplicado";
        } catch (_) {
          mostrarToast("Posible registro duplicado detectado en la misma fecha.", "error", 4000);
        }
      } else {
        mostrarToast(msg, "error", 4000);
      }
    }
  }

  function abrirCrearArchivoModal() {
    nombreNuevoArchivo = "";
    defensorNuevoArchivo = "";
    errorModalCrear = "";
    modalActivo = "crear";
  }

  async function crearNuevoArchivo() {
    if (!nombreNuevoArchivo.trim()) {
      errorModalCrear = "Ingresa un nombre para el archivo";
      return;
    }
    const defensorTrim = defensorNuevoArchivo.trim();
    try {
      const creado = await invoke("cmd_crear_archivo", {
        nombre: nombreNuevoArchivo.trim(),
        defensor: defensorTrim || null,
      });
      if (defensorTrim) {
        try {
          const cfg = await invoke("cmd_agregar_defensor", { nombre: defensorTrim });
          defensores = cfg.defensores || [];
        } catch (_) {}
      }
      await refrescarListaArchivos();
      archivoActual = creado;
      await recargarArchivoActual();
      modalActivo = null;
      nombreNuevoArchivo = "";
      defensorNuevoArchivo = "";
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

  function abrirConfirmacion(titulo, mensaje, textoConfirmar, peligroso, onConfirm) {
    confirmacionPendiente = { titulo, mensaje, textoConfirmar, peligroso, onConfirm };
    ejecutandoConfirmacion = false;
    modalActivo = "confirmar";
  }

  async function ejecutarConfirmacionModal() {
    if (!confirmacionPendiente || ejecutandoConfirmacion) return;
    ejecutandoConfirmacion = true;
    try {
      await confirmacionPendiente.onConfirm();
      modalActivo = null;
      confirmacionPendiente = null;
    } finally {
      ejecutandoConfirmacion = false;
    }
  }

  function eliminarArchivo(nombre) {
    abrirConfirmacion(
      "Eliminar planilla Excel",
      `¿Eliminar permanentemente '${nombre}'? Esta acción no se puede deshacer.`,
      "Eliminar archivo",
      true,
      async () => {
        try {
          await invoke("cmd_eliminar_archivo", { nombre });
          await refrescarListaArchivos();
          if (archivoActual === nombre) {
            archivoActual = archivosDisponibles[0] || null;
            if (archivoActual) {
              await recargarArchivoActual();
            } else {
              registros = [];
              defensorArchivo = "";
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
    );
  }

  async function descargarArchivo(nombre) {
    try {
      const ruta = await invoke("cmd_descargar_archivo", { nombre });
      mostrarToast(`Copia exportada en: ${ruta}`, "success", 4200);
    } catch (e) {
      mostrarToast(`Error al exportar: ${e}`, "error");
    }
  }

  function abrirModalDefensores() {
    defensorPlanillaDraft = defensorArchivo;
    filtroDefensores = "";
    nuevoDefensorDirectorio = "";
    defensorEditando = null;
    defensorEditandoValor = "";
    errorDefensores = "";
    modalActivo = "defensores";
  }

  async function asignarDefensorAPlanilla(nombreDefensor) {
    if (!archivoActual) return;
    const limpio = nombreDefensor.trim();
    errorDefensores = "";
    try {
      const guardado = await invoke("cmd_actualizar_defensor_archivo", {
        nombreArchivo: archivoActual,
        defensor: limpio,
      });
      defensorArchivo = guardado;
      defensorPlanillaDraft = guardado;
      if (limpio) {
        const cfg = await invoke("cmd_agregar_defensor", { nombre: limpio });
        defensores = cfg.defensores || [];
      }
      mostrarToast(
        guardado
          ? `Planilla asignada a ${guardado}`
          : "Se quitó la asignación de defensor en la planilla",
        "success"
      );
    } catch (e) {
      errorDefensores = String(e);
    }
  }

  async function agregarDefensorADirectorio() {
    errorDefensores = "";
    try {
      const cfg = await invoke("cmd_agregar_defensor", {
        nombre: nuevoDefensorDirectorio,
      });
      defensores = cfg.defensores || [];
      nuevoDefensorDirectorio = "";
    } catch (e) {
      errorDefensores = String(e);
    }
  }

  async function guardarRenombreDefensor() {
    if (!defensorEditando) return;
    errorDefensores = "";
    try {
      const cfg = await invoke("cmd_renombrar_defensor", {
        actual: defensorEditando,
        nuevo: defensorEditandoValor,
      });
      defensores = cfg.defensores || [];
      defensorEditando = null;
      defensorEditandoValor = "";
    } catch (e) {
      errorDefensores = String(e);
    }
  }

  function eliminarDefensorDeDirectorio(nombre) {
    abrirConfirmacion(
      "Quitar defensor del directorio",
      `¿Quitar a '${nombre}' de la lista guardada? Las planillas Excel ya creadas no se modificarán.`,
      "Quitar de la lista",
      true,
      async () => {
        try {
          const cfg = await invoke("cmd_eliminar_defensor", { nombre });
          defensores = cfg.defensores || [];
          mostrarToast(`'${nombre}' eliminado del directorio`, "info");
        } catch (e) {
          mostrarToast(`Error: ${e}`, "error");
        }
      }
    );
  }

  async function abrirRespaldos(nombre) {
    respaldosTarget = nombre;
    respaldosArchivo = [];
    cargandoRespaldos = true;
    modalActivo = "respaldos";
    try {
      respaldosArchivo = await invoke("cmd_listar_respaldos", {
        nombreArchivo: nombre,
      });
    } catch (e) {
      mostrarToast(`Error cargando respaldos: ${e}`, "error");
    } finally {
      cargandoRespaldos = false;
    }
  }

  function solicitarRestaurarRespaldo(respaldo) {
    const objetivo = respaldosTarget;
    abrirConfirmacion(
      "Restaurar versión anterior",
      `¿Restaurar '${objetivo}' a la copia del ${respaldo.fecha_legible}? Se guardará antes un respaldo del estado actual.`,
      "Restaurar copia",
      false,
      async () => {
        try {
          await invoke("cmd_restaurar_respaldo", {
            nombreArchivo: objetivo,
            archivoRespaldo: respaldo.archivo,
          });
          if (archivoActual === objetivo) {
            await recargarArchivoActual();
          }
          mostrarToast(`Planilla restaurada a ${respaldo.fecha_legible}`, "success", 3800);
        } catch (e) {
          mostrarToast(`Error al restaurar: ${e}`, "error", 4000);
        }
      }
    );
  }

  function aplicarTipoAlFormulario(tipo) {
    if (TIPOS_AUDIENCIA.includes(tipo) && tipo !== "Otra") {
      tipoSeleccionado = tipo;
      tipoOtra = "";
    } else if (tipo) {
      tipoSeleccionado = "Otra";
      tipoOtra = tipo;
    }
  }

  function cargarRegistroParaEdicion(reg) {
    limpiarFormulario();
    modoEdicion = true;
    filaEditando = reg.fila_excel;
    numeroEditando = reg.numero;
    radicadoEditando = reg.radicado;
    radicado = reg.radicado;
    juzgado = reg.juzgado;
    aplicarTipoAlFormulario(reg.tipo);
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
    aplicarTipoAlFormulario(reg.tipo);
    vistaActiva = "formulario";
    mostrarToast("Radicado y despacho copiados para nueva sesión", "info", 2200);
  }

  function eliminarRegistroFila(reg) {
    abrirConfirmacion(
      "Eliminar registro de audiencia",
      `¿Eliminar el registro #${reg.numero} (${reg.radicado}) de la planilla?`,
      "Eliminar registro",
      true,
      async () => {
        try {
          totales = await invoke("cmd_eliminar_registro", {
            nombreArchivo: archivoActual,
            filaExcel: reg.fila_excel,
            radicadoEsperado: reg.radicado,
          });
          await recargarArchivoActual();
          if (modoEdicion && filaEditando === reg.fila_excel) {
            cancelarEdicion();
          }
          mostrarToast(`Registro #${reg.numero} eliminado`, "info");
        } catch (e) {
          mostrarToast(`Error eliminando registro: ${e}`, "error");
        }
      }
    );
  }

  function abrirConfiguracionIa() {
    nuevaOpenAiKey = "";
    nuevaGeminiKey = "";
    mostrarOpenAiKey = false;
    mostrarGeminiKey = false;
    modalActivo = "config_ia";
  }

  function abrirAutocompletarIa() {
    if (!aiStatus.configured) {
      abrirConfiguracionIa();
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
      const res = await invoke("cmd_guardar_config", {
        config: {
          provider: settings.provider,
          openai_api_key: nuevaOpenAiKey.trim() || null,
          openai_model: settings.openai_model,
          gemini_api_key: nuevaGeminiKey.trim() || null,
          gemini_model: settings.gemini_model,
          theme: settings.theme,
        },
      });
      aiStatus = res.status;
      nuevaOpenAiKey = "";
      nuevaGeminiKey = "";
      mostrarOpenAiKey = false;
      mostrarGeminiKey = false;
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
      abrirCrearArchivoModal();
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
  <Sidebar
    {archivosDisponibles}
    {archivoActual}
    {totales}
    {aiStatus}
    theme={settings.theme}
    onCrearArchivo={abrirCrearArchivoModal}
    onSeleccionarArchivo={seleccionarArchivo}
    onAbrirRespaldos={abrirRespaldos}
    onDescargarArchivo={descargarArchivo}
    onEliminarArchivo={eliminarArchivo}
    onAbrirConfigIa={abrirConfiguracionIa}
    onAbrirAtajos={() => (modalActivo = "atajos")}
    onToggleTema={toggleTema}
  />

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

        <button
          type="button"
          class="defensor-pill"
          onclick={abrirModalDefensores}
          title="Gestionar directorio de defensores y asignación en celda B8"
        >
          <UserCheck size={13} strokeWidth={2} />
          {#if archivoActual}
            <span>Defensor: <strong>{defensorArchivo || "Sin asignar"}</strong></span>
          {:else}
            <span>Directorio ({defensores.length})</span>
          {/if}
        </button>
      </div>

      <div class="header-right">
        <button
          type="button"
          class="btn btn-ai-warm"
          onclick={abrirAutocompletarIa}
          title={aiStatus.configured ? "Autocompletar a partir de texto libre (Ctrl+I)" : "Configurar clave de IA"}
        >
          <Sparkles size={14} strokeWidth={2} />
          <span>{aiStatus.configured ? "Autocompletar IA · Ctrl+I" : "Configurar IA"}</span>
        </button>
      </div>
    </header>

    <main class="workspace-scroll">
      {#if vistaActiva === "formulario"}
        <AudienciaForm
          {archivoActual}
          {registros}
          {totales}
          aiConfigured={aiStatus.configured}
          {modoEdicion}
          {filaEditando}
          {numeroEditando}
          bind:radicado
          bind:juzgado
          bind:tipoSeleccionado
          bind:tipoOtra
          bind:realizada
          bind:fecha
          bind:hora
          bind:minuto
          bind:motivoSeleccionado
          bind:observaciones
          bind:inputRadicadoEl
          onSubmit={() => guardarOActualizar(false)}
          onCancelarEdicion={cancelarEdicion}
          onLimpiar={limpiarFormulario}
          onAbrirIa={abrirAutocompletarIa}
          onVerTodos={() => (vistaActiva = "listado")}
          onEditarRegistro={cargarRegistroParaEdicion}
        />
      {:else}
        <RegistrosTable
          {registros}
          {totales}
          bind:filtroBusqueda
          bind:filtroEstado
          bind:filtroMotivo
          onClonar={clonarBaseParaNuevaSesion}
          onEditar={cargarRegistroParaEdicion}
          onEliminar={eliminarRegistroFila}
        />
      {/if}
    </main>
  </div>
</div>

<Modals
  bind:modalActivo
  {archivoActual}
  {defensorArchivo}
  {defensores}
  bind:settings
  {aiStatus}
  bind:nombreNuevoArchivo
  bind:defensorNuevoArchivo
  {errorModalCrear}
  bind:textoIa
  {procesandoIa}
  {errorIa}
  bind:nuevaOpenAiKey
  bind:nuevaGeminiKey
  bind:mostrarOpenAiKey
  bind:mostrarGeminiKey
  bind:confirmacionPendiente
  {ejecutandoConfirmacion}
  bind:duplicadoDetectado
  bind:filtroDefensores
  bind:nuevoDefensorDirectorio
  bind:defensorEditando
  bind:defensorEditandoValor
  bind:defensorPlanillaDraft
  {errorDefensores}
  {respaldosArchivo}
  {respaldosTarget}
  {cargandoRespaldos}
  onCrearNuevoArchivo={crearNuevoArchivo}
  onAbrirConfigIa={abrirConfiguracionIa}
  onProcesarConIa={procesarConIa}
  onGuardarConfigIa={guardarConfiguracionIa}
  onConfirmarSinHora={() => guardarOActualizar(true)}
  onConfirmarDuplicado={() => guardarOActualizar(ultimoPermitirHoraVacia, true)}
  onEjecutarConfirmacion={ejecutarConfirmacionModal}
  onAsignarDefensor={asignarDefensorAPlanilla}
  onAgregarDefensor={agregarDefensorADirectorio}
  onGuardarRenombreDefensor={guardarRenombreDefensor}
  onEliminarDefensor={eliminarDefensorDeDirectorio}
  onRestaurarRespaldo={solicitarRestaurarRespaldo}
/>

{#if toast}
  <div class="toast" role="status" aria-live="polite">
    <span>{toast.mensaje}</span>
  </div>
{/if}
