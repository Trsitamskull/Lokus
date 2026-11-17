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
    UserCheck,
    History,
    RotateCcw,
    AlertTriangle,
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
  let defensorArchivo = $state("");
  let defensores = $state([]);
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
  let radicadoEditando = $state("");

  // Estado de modales (<dialog>)
  let modalActivo = $state(null); // "crear" | "ia" | "config_ia" | "atajos" | "confirmar_sin_hora" | "confirmar" | "confirmar_duplicado" | "defensores" | "respaldos"
  let dialogEl = $state(null);
  let nombreNuevoArchivo = $state("");
  let defensorNuevoArchivo = $state("");
  let errorModalCrear = $state("");
  let confirmacionPendiente = $state(null);
  let ejecutandoConfirmacion = $state(false);
  let duplicadoDetectado = $state(null);
  let ultimoPermitirHoraVacia = $state(false);

  // Gestión de defensores y respaldos
  let filtroDefensores = $state("");
  let nuevoDefensorDirectorio = $state("");
  let defensorEditando = $state(null);
  let defensorEditandoValor = $state("");
  let defensorPlanillaDraft = $state("");
  let errorDefensores = $state("");
  let respaldosArchivo = $state([]);
  let respaldosTarget = $state("");
  let cargandoRespaldos = $state(false);

  let filtroBusqueda = $state("");
  let filtroEstado = $state("TODOS"); // "TODOS" | "SI" | "NO"
  let filtroMotivo = $state(""); // "" o uno de MOTIVOS_LABELS

  // Estado IA y Configuración (sin claves en claro en el frontend)
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
  let inputRadicadoEl = $state(null);

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
    radicadoEditando = reg.radicado;

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
      nombreNuevoArchivo = "";
      defensorNuevoArchivo = "";
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
          defensorNuevoArchivo = "";
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
                onclick={() => abrirRespaldos(arch)}
                title="Historial de respaldos (últimas 3 versiones)"
                aria-label="Historial de respaldos"
              >
                <History size={13} />
              </button>
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
          onclick={abrirConfiguracionIa}
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
          {:else}
            <button
              type="button"
              class="ia-quick-strip {aiStatus.configured ? 'ready' : ''}"
              onclick={abrirAutocompletarIa}
            >
              <span class="ia-quick-strip-left">
                <Sparkles size={14} strokeWidth={2} />
                {#if aiStatus.configured}
                  <span>¿Tienes el acta o nota de la audiencia? Pegar aquí para autocompletar</span>
                {:else}
                  <span>Configura tu clave de OpenAI o Gemini para autocompletar audiencias desde texto</span>
                {/if}
              </span>
              <span class="kbd">{aiStatus.configured ? "Ctrl+I" : "Configurar"}</span>
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
<datalist id="defensores-datalist">
  {#each defensores as d}
    <option value={d}></option>
  {/each}
</datalist>

{#if modalActivo}
  <dialog
    bind:this={dialogEl}
    class={modalActivo === "defensores" ? "modal-wide" : ""}
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

        <div class="field-group">
          <div class="field-header">
            <label class="field-label" for="nuevo-archivo-defensor">
              Defensor Público asignado (celda B8, opcional)
            </label>
          </div>
          <input
            id="nuevo-archivo-defensor"
            type="text"
            list="defensores-datalist"
            class="input"
            bind:value={defensorNuevoArchivo}
            placeholder={defensores.length > 0
              ? "Selecciona de tu lista o escribe un nombre nuevo"
              : "Escribe el nombre y se guardará para próximas planillas"}
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
          onclick={abrirConfiguracionIa}
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
        <!-- svelte-ignore a11y_autofocus -->
        <button
          type="button"
          class="btn btn-subtle"
          autofocus
          onclick={() => (modalActivo = null)}
        >
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
      <div class="modal-body">
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
          onclick={ejecutarConfirmacionModal}
          disabled={ejecutandoConfirmacion}
        >
          {ejecutandoConfirmacion ? "Procesando..." : confirmacionPendiente.textoConfirmar}
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
      <div class="modal-body">
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
        <button
          type="button"
          class="btn btn-solid"
          onclick={() => guardarOActualizar(ultimoPermitirHoraVacia, true)}
        >
          Guardar de todas formas
        </button>
      </div>

    {:else if modalActivo === "defensores"}
      <div class="modal-header">
        <h2 id="dialog-heading" class="modal-title">
          <UserCheck size={15} />
          <span>Defensores Públicos y celda B8</span>
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
        {#if archivoActual}
          <div class="field-group">
            <label class="field-label" for="defensor-planilla-input">
              Asignado en {archivoActual} (celda B8)
            </label>
            <div style="display: flex; gap: 8px">
              <input
                id="defensor-planilla-input"
                type="text"
                list="defensores-datalist"
                class="input"
                bind:value={defensorPlanillaDraft}
                placeholder="Nombre del defensor en este Excel"
              />
              <button
                type="button"
                class="btn btn-solid"
                onclick={() => asignarDefensorAPlanilla(defensorPlanillaDraft)}
              >
                Asignar
              </button>
              {#if defensorArchivo}
                <button
                  type="button"
                  class="btn btn-subtle"
                  onclick={() => asignarDefensorAPlanilla("")}
                  title="Dejar celda B8 sin nombre"
                >
                  Quitar
                </button>
              {/if}
            </div>
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
                  agregarDefensorADirectorio();
                }
              }}
            />
            <button type="button" class="btn btn-subtle" onclick={agregarDefensorADirectorio}>
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
          <div class="defensores-list">
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
                        guardarRenombreDefensor();
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
                      onclick={guardarRenombreDefensor}
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
                    {#if archivoActual}
                      <button
                        type="button"
                        class="btn btn-subtle"
                        style="padding: 3px 9px; font-size: 11.5px"
                        onclick={() => asignarDefensorAPlanilla(defName)}
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
                      onclick={() => eliminarDefensorDeDirectorio(defName)}
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
        <button type="button" class="btn btn-solid" onclick={() => (modalActivo = null)}>
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
      <div class="modal-body">
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
                  onclick={() => solicitarRestaurarRespaldo(resp)}
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
