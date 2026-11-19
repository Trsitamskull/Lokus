<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    FileText,
    List,
    PanelLeftClose,
    PanelLeftOpen,
    BarChart3,
    Share2,
    Package,
    FileSpreadsheet,
    UserCheck,
  } from "@lucide/svelte";
  import {
    TIPOS_AUDIENCIA,
    MOTIVOS_LABELS,
    JUZGADOS_BASE,
    normalizarClaveCatalogo,
  } from "./lib/constants.js";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import AudienciaForm from "./lib/components/AudienciaForm.svelte";
  import RegistrosTable from "./lib/components/RegistrosTable.svelte";
  import EstadisticasView from "./lib/components/EstadisticasView.svelte";
  import Modals from "./lib/components/Modals.svelte";

  // Navegación y archivos
  let vistaActiva = $state("formulario"); // "formulario" | "listado" | "estadisticas"
  let sidebarMobileOpen = $state(false);
  let sidebarColapsada = $state(false);
  let archivoActual = $state(null);
  let archivosDisponibles = $state([]);
  let carpetasDisponibles = $state([]);
  let ordenCarpetas = $state([]);
  let coloresCarpetas = $state({});
  let registros = $state([]);
  let defensorArchivo = $state("");
  let defensores = $state([]);
  let juzgadosGuardados = $state([]);
  let tiposGuardados = $state([]);
  let totales = $state({
    num_registros: 0,
    total_si: 0,
    total_no: 0,
    totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
  });

  // Estadísticas globales / mensuales
  let statsMesSeleccionado = $state(""); // "" = todos los meses, o "YYYY-MM"
  let statsArchivoFiltro = $state(""); // "" = todas las planillas
  let statsDefensorFiltro = $state(""); // "" = todos los defensores
  let estadisticas = $state({
    total_audiencias: 0,
    total_si: 0,
    total_no: 0,
    efectividad_pct: 0,
    totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
    total_planillas: 0,
    total_planillas_con_datos: 0,
    meses_disponibles: [],
    planillas: [],
    defensores_disponibles: [],
    por_defensor: [],
  });
  let cargandoEstadisticas = $state(false);

  // Formulario
  let radicado = $state("");
  let juzgadoSeleccionado = $state("");
  let juzgadoOtra = $state("");
  let tipoSeleccionado = $state("");
  let tipoOtra = $state("");
  let realizada = $state(""); // "SI" | "NO" | ""
  let fecha = $state("");
  let hora = $state("");
  let minuto = $state("");
  let motivoSeleccionado = $state("");
  let observaciones = $state("");
  let inputRadicadoEl = $state(null);
  let formResetTick = $state(0);

  // Aprendizaje de la IA: foto de lo que propuso el autocompletado para compararlo al guardar
  let iaSnapshot = $state(null); // { texto, prediccion } | null
  let aprendizaje = $state({
    aciertos: 0,
    total_correcciones: 0,
    total_reglas: 0,
    pendientes_exportar: 0,
    pendientes_ia: 0,
    pendientes_uso: 0,
    total_audiencias_contabilizadas: 0,
    puede_exportar: false,
    aclaracion_pendiente: null,
  });
  let exportandoAprendizaje = $state(false);
  let diagnosticoIa = $state(null);
  let cargandoDiagnosticoIa = $state(false);

  // Edición
  let modoEdicion = $state(false);
  let filaEditando = $state(null);
  let numeroEditando = $state(null);
  let radicadoEditando = $state("");

  // Modales y confirmaciones
  let modalActivo = $state(null);
  let nombreNuevoArchivo = $state("");
  let defensorNuevoArchivo = $state("");
  let carpetaNuevoArchivo = $state("");
  let errorModalCrear = $state("");
  let carpetaPadreNueva = $state("");
  let nombreNuevaCarpeta = $state("");
  let colorNuevaCarpeta = $state("");
  let carpetaRenombrarActual = $state("");
  let carpetaRenombrarNueva = $state("");
  let colorCarpetaRenombrar = $state("");
  let colorCarpetaTarget = $state("");
  let colorCarpetaEditando = $state("");
  let errorModalCarpeta = $state("");
  let archivoMoverTarget = $state("");
  let carpetaDestinoMover = $state("");
  let carpetaDestinoNueva = $state("");
  let errorModalMover = $state("");
  let eliminarTargetNombre = $state("");
  let eliminarTargetResumen = $state(null);
  let eliminarCarpetaTarget = $state(null);
  let conservarEstadisticasAlEliminar = $state(true);
  let portabilidadTab = $state("exportar");
  let tipoExportacion = $state("completo");
  let exportandoPaquete = $state(false);
  let archivoImportNombre = $state("");
  let archivoImportB64 = $state("");
  let inspeccionImportacion = $state(null);
  let errorImportacion = $state("");
  let modoConflictoImport = $state("omitir");
  let importarConfigPaquete = $state(true);
  let importandoPaquete = $state(false);
  let confirmacionPendiente = $state(null);
  let ejecutandoConfirmacion = $state(false);
  let duplicadoDetectado = $state(null);
  let ultimoPermitirHoraVacia = $state(false);

  // Filtros tabla
  let filtroBusqueda = $state("");
  let filtroEstado = $state("TODOS");
  let filtroMotivo = $state("");

  // Defensores y respaldos
  let defensorTargetArchivo = $state(null);
  let defensorTargetNombre = $state("");
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
      juzgadosGuardados = res.juzgados || [];
      tiposGuardados = res.tipos_audiencia || [];
      aiStatus = res.status;
      aplicarTema(settings.theme);
    } catch (_) {
      aplicarTema("dark");
    }
  }

  async function refrescarListaArchivos() {
    try {
      const [archivos, carpetas, orden, colores] = await Promise.all([
        invoke("cmd_listar_archivos"),
        invoke("cmd_listar_carpetas"),
        invoke("cmd_obtener_orden_carpetas"),
        invoke("cmd_obtener_colores_carpetas"),
      ]);
      archivosDisponibles = archivos;
      carpetasDisponibles = carpetas;
      ordenCarpetas = orden;
      coloresCarpetas = colores || {};
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

  async function cargarEstadisticas() {
    cargandoEstadisticas = true;
    try {
      let anio = null;
      let mes = null;
      if (statsMesSeleccionado && statsMesSeleccionado.includes("-")) {
        const [yStr, mStr] = statsMesSeleccionado.split("-");
        const yNum = Number.parseInt(yStr, 10);
        const mNum = Number.parseInt(mStr, 10);
        if (Number.isFinite(yNum) && Number.isFinite(mNum)) {
          anio = yNum;
          mes = mNum;
        }
      }
      estadisticas = await invoke("cmd_obtener_estadisticas", {
        anio,
        mes,
        archivo: statsArchivoFiltro || null,
        defensor: statsDefensorFiltro || null,
      });
    } catch (e) {
      mostrarToast(`Error calculando estadísticas: ${e}`, "error");
    } finally {
      cargandoEstadisticas = false;
    }
  }

  function cambiarVista(destino) {
    vistaActiva = destino;
    if (destino === "estadisticas") {
      cargarEstadisticas();
    }
  }

  async function abrirPlanillaDesdeStats(nombreArchivo) {
    if (!archivosDisponibles.includes(nombreArchivo)) {
      mostrarToast(
        "Planilla archivada: solo se conservan sus estadísticas en el historial (el archivo Excel fue eliminado).",
        "info",
        3800
      );
      return;
    }
    await seleccionarArchivo(nombreArchivo);
    vistaActiva = "listado";
  }

  function solicitarEliminarArchivada(arch) {
    abrirConfirmacion(
      "Eliminar estadística archivada",
      `¿Eliminar del historial el resumen archivado de '${arch.archivo}' (${arch.num_registros} audiencias)?`,
      "Eliminar del historial",
      true,
      async () => {
        try {
          await invoke("cmd_eliminar_estadistica_archivada", { id: arch.id });
          if (statsArchivoFiltro === arch.archivo) {
            statsArchivoFiltro = "";
          }
          await cargarEstadisticas();
          mostrarToast("Resumen archivado eliminado del historial", "info");
        } catch (e) {
          mostrarToast(String(e), "error");
        }
      }
    );
  }

  async function copiarResumenEstadisticas() {
    if (!estadisticas || estadisticas.total_audiencias === 0) return;
    const mesObj = estadisticas.meses_disponibles.find(
      (m) => `${m.anio}-${String(m.mes).padStart(2, "0")}` === statsMesSeleccionado
    );
    const periodo = mesObj ? mesObj.etiqueta : "Todos los meses";
    const defEtiqueta =
      statsDefensorFiltro === "__SIN_ASIGNAR__"
        ? "Sin defensor asignado"
        : statsDefensorFiltro;
    const alcance = statsArchivoFiltro
      ? `Planilla ${statsArchivoFiltro}`
      : defEtiqueta
        ? `Defensor: ${defEtiqueta}`
        : `Todas las planillas (${estadisticas.total_planillas})`;
    const causalesTexto = MOTIVOS_LABELS.map(
      (m, i) => `  - ${m}: ${estadisticas.totales_motivos[i] ?? 0}`
    ).join("\n");

    const resumen = [
      `RESUMEN DE AUDIENCIAS — LOKUS JUDICIAL`,
      `Periodo: ${periodo}`,
      `Alcance: ${alcance}`,
      `Total audiencias: ${estadisticas.total_audiencias}`,
      `Realizadas (SI): ${estadisticas.total_si} (${estadisticas.efectividad_pct.toFixed(1)}%)`,
      `Aplazadas (NO): ${estadisticas.total_no}`,
      `Causales de aplazamiento:`,
      causalesTexto,
    ].join("\n");

    try {
      await navigator.clipboard.writeText(resumen);
      mostrarToast("Resumen mensual copiado al portapapeles", "success");
    } catch (_) {
      mostrarToast("No se pudo copiar al portapapeles", "error");
    }
  }

  function limpiarFormulario() {
    radicado = "";
    juzgadoSeleccionado = "";
    juzgadoOtra = "";
    tipoSeleccionado = "";
    tipoOtra = "";
    realizada = "";
    fecha = "";
    hora = "";
    minuto = "";
    motivoSeleccionado = "";
    observaciones = "";
    iaSnapshot = null;
    formResetTick += 1;
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
    const juzgadoFinal =
      juzgadoSeleccionado === "Otra"
        ? juzgadoOtra.trim()
        : juzgadoSeleccionado.trim();
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
      juzgado: juzgadoFinal,
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
        cargarResumenAprendizaje();
      } else {
        const snapshotIa = iaSnapshot;
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
        if (snapshotIa) {
          registrarResultadoIa(snapshotIa, datos);
        } else {
          cargarResumenAprendizaje();
        }
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

  function abrirCrearArchivoModal(carpetaInicial = "") {
    nombreNuevoArchivo = "";
    defensorNuevoArchivo = "";
    carpetaNuevoArchivo = typeof carpetaInicial === "string" ? carpetaInicial : "";
    errorModalCrear = "";
    modalActivo = "crear";
  }

  async function crearNuevoArchivo() {
    if (!nombreNuevoArchivo.trim()) {
      errorModalCrear = "Ingresa un nombre para el archivo";
      return;
    }
    const defensorTrim = defensorNuevoArchivo.trim();
    const carpetaTrim = carpetaNuevoArchivo.trim();
    try {
      const creado = await invoke("cmd_crear_archivo", {
        nombre: nombreNuevoArchivo.trim(),
        defensor: defensorTrim || null,
        carpeta: carpetaTrim || null,
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
      carpetaNuevoArchivo = "";
      errorModalCrear = "";
      mostrarToast(`Archivo '${creado}' creado`, "success");
    } catch (e) {
      errorModalCrear = String(e);
    }
  }

  function abrirCrearCarpetaModal(carpetaPadre = "") {
    carpetaPadreNueva = typeof carpetaPadre === "string" ? carpetaPadre : "";
    nombreNuevaCarpeta = "";
    colorNuevaCarpeta = "";
    errorModalCarpeta = "";
    modalActivo = "crear_carpeta";
  }

  async function crearNuevaCarpeta() {
    const limpio = nombreNuevaCarpeta.trim().replace(/^\/+|\/+$/g, "");
    if (!limpio) {
      errorModalCarpeta = "Ingresa el nombre de la carpeta";
      return;
    }
    const rutaCompleta = carpetaPadreNueva ? `${carpetaPadreNueva}/${limpio}` : limpio;
    try {
      const creada = await invoke("cmd_crear_carpeta", {
        ruta: rutaCompleta,
        color: colorNuevaCarpeta || null,
      });
      await refrescarListaArchivos();
      modalActivo = null;
      nombreNuevaCarpeta = "";
      colorNuevaCarpeta = "";
      errorModalCarpeta = "";
      mostrarToast(`Carpeta '${creada}' creada`, "success");
    } catch (e) {
      errorModalCarpeta = String(e);
    }
  }

  function abrirRenombrarCarpetaModal(carpeta) {
    carpetaRenombrarActual = carpeta;
    carpetaRenombrarNueva = carpeta;
    colorCarpetaRenombrar = coloresCarpetas[carpeta] || "";
    errorModalCarpeta = "";
    modalActivo = "renombrar_carpeta";
  }

  async function renombrarCarpetaExistente() {
    const nueva = carpetaRenombrarNueva.trim();
    if (!nueva) {
      errorModalCarpeta = "Ingresa el nuevo nombre de la carpeta";
      return;
    }
    const colorActual = coloresCarpetas[carpetaRenombrarActual] || "";
    const colorNuevo = colorCarpetaRenombrar || "";
    if (nueva === carpetaRenombrarActual) {
      if (colorNuevo === colorActual) {
        modalActivo = null;
        return;
      }
      try {
        coloresCarpetas = await invoke("cmd_guardar_color_carpeta", {
          ruta: carpetaRenombrarActual,
          color: colorNuevo,
        });
        modalActivo = null;
        errorModalCarpeta = "";
        mostrarToast("Color de carpeta actualizado", "success");
      } catch (e) {
        errorModalCarpeta = String(e);
      }
      return;
    }
    try {
      const renombrada = await aplicarMovimientoCarpeta(
        carpetaRenombrarActual,
        nueva,
        colorNuevo,
      );
      modalActivo = null;
      errorModalCarpeta = "";
      mostrarToast(`Carpeta movida/renombrada a '${renombrada}'`, "success");
    } catch (e) {
      errorModalCarpeta = String(e);
    }
  }

  function abrirColorCarpetaModal(carpeta) {
    colorCarpetaTarget = carpeta;
    colorCarpetaEditando = coloresCarpetas[carpeta] || "";
    modalActivo = "color_carpeta";
  }

  async function guardarColorCarpetaModal() {
    if (!colorCarpetaTarget) return;
    try {
      coloresCarpetas = await invoke("cmd_guardar_color_carpeta", {
        ruta: colorCarpetaTarget,
        color: colorCarpetaEditando || "",
      });
      modalActivo = null;
      mostrarToast("Color de carpeta actualizado", "success");
    } catch (e) {
      mostrarToast(`No se pudo guardar el color: ${e}`, "error");
    }
  }

  async function aplicarMovimientoCarpeta(actual, nueva, color = null) {
    const renombrada = await invoke("cmd_renombrar_carpeta", {
      actual,
      nueva,
      color,
    });
    const prefijo = `${actual}/`;
    if (archivoActual && archivoActual.startsWith(prefijo)) {
      archivoActual = `${renombrada}/${archivoActual.slice(prefijo.length)}`;
    }
    if (statsArchivoFiltro && statsArchivoFiltro.startsWith(prefijo)) {
      statsArchivoFiltro = `${renombrada}/${statsArchivoFiltro.slice(prefijo.length)}`;
    }
    await refrescarListaArchivos();
    return renombrada;
  }

  async function aplicarMovimientoArchivo(nombre, destino) {
    const nuevaRuta = await invoke("cmd_mover_archivo", {
      nombre,
      carpetaDestino: destino,
    });
    if (archivoActual === nombre) {
      archivoActual = nuevaRuta;
    }
    if (statsArchivoFiltro === nombre) {
      statsArchivoFiltro = nuevaRuta;
    }
    await refrescarListaArchivos();
    return nuevaRuta;
  }

  async function finalizarEliminacionCarpeta(
    carpeta,
    eliminarContenido,
    conservarEstadisticas
  ) {
    try {
      await invoke("cmd_eliminar_carpeta", {
        ruta: carpeta,
        eliminarContenido,
        conservarEstadisticas,
      });
      const prefijo = `${carpeta}/`;
      await refrescarListaArchivos();
      if (archivoActual && archivoActual.startsWith(prefijo)) {
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
      if (statsArchivoFiltro && statsArchivoFiltro.startsWith(prefijo)) {
        statsArchivoFiltro = "";
      }
      if (vistaActiva === "estadisticas") {
        cargarEstadisticas();
      }
      cargarResumenAprendizaje();
      mostrarToast(
        eliminarContenido && conservarEstadisticas
          ? `Carpeta '${carpeta}' eliminada (estadísticas conservadas)`
          : `Carpeta '${carpeta}' eliminada`,
        "info"
      );
    } catch (e) {
      mostrarToast(String(e), "error", 4200);
    }
  }

  function eliminarCarpetaSolicitada(carpeta) {
    const prefijo = `${carpeta}/`;
    const totalArchivos = archivosDisponibles.filter((a) =>
      a.startsWith(prefijo)
    ).length;
    const totalSubcarpetas = carpetasDisponibles.filter((c) =>
      c.startsWith(prefijo)
    ).length;

    if (totalArchivos === 0 && totalSubcarpetas === 0) {
      abrirConfirmacion(
        "Eliminar carpeta",
        `¿Eliminar la carpeta vacía '${carpeta}'?`,
        "Eliminar carpeta",
        true,
        () => finalizarEliminacionCarpeta(carpeta, false, false)
      );
      return;
    }

    eliminarCarpetaTarget = { carpeta, totalArchivos, totalSubcarpetas };
    conservarEstadisticasAlEliminar = true;
    modalActivo = "eliminar_carpeta";
  }

  async function confirmarEliminacionCarpeta() {
    if (!eliminarCarpetaTarget) return;
    const { carpeta, totalArchivos } = eliminarCarpetaTarget;
    const conservar = totalArchivos > 0 ? conservarEstadisticasAlEliminar : false;
    modalActivo = null;
    eliminarCarpetaTarget = null;
    await finalizarEliminacionCarpeta(carpeta, true, conservar);
  }

  function abrirModalPortabilidad(tabInicial = "exportar") {
    portabilidadTab = tabInicial;
    archivoImportNombre = "";
    archivoImportB64 = "";
    inspeccionImportacion = null;
    errorImportacion = "";
    modoConflictoImport = "omitir";
    importarConfigPaquete = true;
    modalActivo = "portabilidad";
  }

  async function exportarPaqueteLokus() {
    if (exportandoPaquete) return;
    exportandoPaquete = true;
    try {
      const ruta = await invoke("cmd_exportar_paquete", {
        tipoPaquete: tipoExportacion,
      });
      modalActivo = null;
      mostrarToast(
        tipoExportacion === "completo"
          ? `Respaldo completo guardado en: ${ruta}`
          : `Estadísticas exportadas en: ${ruta}`,
        "success",
        5000
      );
    } catch (e) {
      mostrarToast(String(e), "error", 4500);
    } finally {
      exportandoPaquete = false;
    }
  }

  async function seleccionarArchivoImportar(nombre, b64) {
    archivoImportNombre = nombre;
    archivoImportB64 = b64;
    inspeccionImportacion = null;
    errorImportacion = "";
    try {
      inspeccionImportacion = await invoke("cmd_inspeccionar_importacion", {
        contenidoB64: b64,
      });
    } catch (e) {
      archivoImportB64 = "";
      errorImportacion = String(e);
    }
  }

  async function ejecutarImportacionPaquete() {
    if (!archivoImportB64 || importandoPaquete) return;
    importandoPaquete = true;
    errorImportacion = "";
    try {
      const res = await invoke("cmd_importar_paquete", {
        contenidoB64: archivoImportB64,
        modoConflicto: modoConflictoImport,
        importarConfig: Boolean(importarConfigPaquete),
      });
      await refrescarListaArchivos();
      await cargarConfigInicial();
      await cargarResumenAprendizaje();
      if (!archivoActual && archivosDisponibles.length > 0) {
        archivoActual = archivosDisponibles[0];
        await recargarArchivoActual();
      } else if (archivoActual) {
        await recargarArchivoActual();
      }
      if (vistaActiva === "estadisticas") {
        await cargarEstadisticas();
      }
      modalActivo = null;

      const partes = [];
      const planillasTocadas = res.planillas_importadas + res.planillas_reemplazadas;
      const resumenesTocados = res.resumenes_importados + res.resumenes_reemplazados;
      if (planillasTocadas > 0) {
        partes.push(`${planillasTocadas} planilla(s)`);
      }
      if (resumenesTocados > 0) {
        partes.push(`${resumenesTocados} resumen(es) estadístico(s)`);
      }
      if (res.planillas_omitidas + res.resumenes_omitidos > 0) {
        partes.push(`${res.planillas_omitidas + res.resumenes_omitidos} repetido(s) omitido(s)`);
      }
      mostrarToast(
        partes.length > 0
          ? `Importación lista: ${partes.join(" · ")}`
          : "Importación completada sin cambios (todo ya existía)",
        "success",
        4800
      );
    } catch (e) {
      errorImportacion = String(e);
    } finally {
      importandoPaquete = false;
    }
  }

  function abrirMoverArchivoModal(arch) {
    archivoMoverTarget = arch;
    const idx = arch.lastIndexOf("/");
    carpetaDestinoMover = idx >= 0 ? arch.slice(0, idx) : "";
    carpetaDestinoNueva = "";
    errorModalMover = "";
    modalActivo = "mover_archivo";
  }

  async function moverArchivoACarpeta() {
    const destino =
      carpetaDestinoMover === "__NUEVA__"
        ? carpetaDestinoNueva.trim()
        : carpetaDestinoMover.trim();
    if (carpetaDestinoMover === "__NUEVA__" && !destino) {
      errorModalMover = "Escribe el nombre de la nueva carpeta";
      return;
    }
    try {
      const nuevaRuta = await aplicarMovimientoArchivo(archivoMoverTarget, destino);
      modalActivo = null;
      errorModalMover = "";
      mostrarToast(`Planilla movida a '${nuevaRuta}'`, "success");
    } catch (e) {
      errorModalMover = String(e);
    }
  }

  async function moverPorArrastre(elemento, destinoCarpeta) {
    try {
      if (elemento.tipo === "archivo") {
        const nuevaRuta = await aplicarMovimientoArchivo(
          elemento.ruta,
          destinoCarpeta
        );
        mostrarToast(`Planilla movida a '${nuevaRuta}'`, "success", 2600);
        return nuevaRuta;
      } else if (elemento.tipo === "carpeta") {
        const partes = elemento.ruta.split("/");
        const nombreBase = partes[partes.length - 1];
        const nuevaRuta = destinoCarpeta
          ? `${destinoCarpeta}/${nombreBase}`
          : nombreBase;
        if (nuevaRuta === elemento.ruta) return elemento.ruta;
        const movida = await aplicarMovimientoCarpeta(elemento.ruta, nuevaRuta);
        mostrarToast(`Carpeta movida a '${movida}'`, "success", 2600);
        return movida;
      }
    } catch (e) {
      mostrarToast(String(e), "error", 4000);
    }
    return null;
  }

  async function guardarOrdenCarpetas(nuevoOrden) {
    ordenCarpetas = nuevoOrden;
    try {
      ordenCarpetas = await invoke("cmd_guardar_orden_carpetas", {
        orden: nuevoOrden,
      });
    } catch (_) {}
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

  async function finalizarEliminacionArchivo(nombre, conservarEstadisticas) {
    try {
      await invoke("cmd_eliminar_archivo", {
        nombre,
        conservarEstadisticas,
      });
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
      if (vistaActiva === "estadisticas") {
        cargarEstadisticas();
      }
      cargarResumenAprendizaje();
      mostrarToast(
        conservarEstadisticas
          ? `Archivo '${nombre}' eliminado (estadísticas conservadas)`
          : `Archivo '${nombre}' eliminado`,
        "info"
      );
    } catch (e) {
      mostrarToast(`Error al eliminar: ${e}`, "error");
    }
  }

  async function eliminarArchivo(nombre) {
    let resumen = null;
    if (nombre === archivoActual) {
      resumen = totales;
    } else {
      try {
        const res = await invoke("cmd_leer_archivo", { nombre });
        resumen = res.totales;
      } catch (_) {}
    }

    if (resumen && resumen.num_registros > 0) {
      eliminarTargetNombre = nombre;
      eliminarTargetResumen = resumen;
      conservarEstadisticasAlEliminar = true;
      modalActivo = "eliminar_archivo";
      return;
    }

    abrirConfirmacion(
      "Eliminar planilla Excel",
      `¿Eliminar permanentemente '${nombre}'? Esta acción no se puede deshacer.`,
      "Eliminar archivo",
      true,
      () => finalizarEliminacionArchivo(nombre, false)
    );
  }

  async function confirmarEliminacionConOpciones() {
    if (!eliminarTargetNombre) return;
    const nombre = eliminarTargetNombre;
    const conservar = conservarEstadisticasAlEliminar;
    modalActivo = null;
    eliminarTargetNombre = "";
    eliminarTargetResumen = null;
    await finalizarEliminacionArchivo(nombre, conservar);
  }

  async function descargarArchivo(nombre) {
    try {
      const ruta = await invoke("cmd_descargar_archivo", { nombre });
      mostrarToast(`Copia exportada en: ${ruta}`, "success", 4200);
    } catch (e) {
      mostrarToast(`Error al exportar: ${e}`, "error");
    }
  }

  async function abrirModalDefensoresArchivo(nombreArchivo) {
    defensorTargetArchivo = nombreArchivo;
    let actualDef = "";
    if (nombreArchivo && nombreArchivo === archivoActual) {
      actualDef = defensorArchivo || "";
    } else if (nombreArchivo) {
      try {
        const res = await invoke("cmd_leer_archivo", { nombre: nombreArchivo });
        actualDef = res.defensor || "";
      } catch (_) {}
    }
    defensorTargetNombre = actualDef;
    defensorPlanillaDraft = actualDef;
    filtroDefensores = "";
    nuevoDefensorDirectorio = "";
    defensorEditando = null;
    defensorEditandoValor = "";
    errorDefensores = "";
    sidebarMobileOpen = false;
    modalActivo = "defensores";
  }

  async function asignarDefensorAPlanilla(nombreDefensor) {
    const target = defensorTargetArchivo || archivoActual;
    if (!target) return;
    const limpio = nombreDefensor.trim();
    errorDefensores = "";
    try {
      const guardado = await invoke("cmd_actualizar_defensor_archivo", {
        nombreArchivo: target,
        defensor: limpio,
      });
      defensorTargetNombre = guardado;
      defensorPlanillaDraft = guardado;
      if (target === archivoActual) {
        defensorArchivo = guardado;
      }
      if (limpio) {
        const cfg = await invoke("cmd_agregar_defensor", { nombre: limpio });
        defensores = cfg.defensores || [];
      }
      if (vistaActiva === "estadisticas") {
        cargarEstadisticas();
      }
      mostrarToast(
        guardado
          ? `'${target}' asignada a ${guardado}`
          : `Se quitó la asignación de defensor en '${target}'`,
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

  function aplicarTipoAlFormulario(tipo, otraExplicita = "") {
    const limpio = (tipo || "").trim();
    if (!limpio) {
      tipoSeleccionado = "";
      tipoOtra = "";
      return;
    }
    const key = normalizarClaveCatalogo(limpio);
    if (key === "otra") {
      tipoSeleccionado = "Otra";
      tipoOtra = (otraExplicita || "").trim();
      return;
    }
    const matchGuardado = tiposGuardados.find(
      (t) => normalizarClaveCatalogo(t) === key
    );
    if (matchGuardado) {
      tipoSeleccionado = matchGuardado;
      tipoOtra = "";
      return;
    }
    const matchBase = TIPOS_AUDIENCIA.find(
      (t) => t !== "Otra" && normalizarClaveCatalogo(t) === key
    );
    if (matchBase) {
      tipoSeleccionado = matchBase;
      tipoOtra = "";
      return;
    }
    tipoSeleccionado = "Otra";
    tipoOtra = limpio;
  }

  function aplicarJuzgadoAlFormulario(juzgadoValor) {
    const limpio = (juzgadoValor || "").trim();
    if (!limpio) {
      juzgadoSeleccionado = "";
      juzgadoOtra = "";
      return;
    }
    const key = normalizarClaveCatalogo(limpio);
    if (key === "otra") {
      juzgadoSeleccionado = "Otra";
      juzgadoOtra = "";
      return;
    }
    const matchGuardado = juzgadosGuardados.find(
      (j) => normalizarClaveCatalogo(j) === key
    );
    if (matchGuardado) {
      juzgadoSeleccionado = matchGuardado;
      juzgadoOtra = "";
      return;
    }
    const matchBase = JUZGADOS_BASE.find(
      (j) => normalizarClaveCatalogo(j) === key
    );
    if (matchBase) {
      juzgadoSeleccionado = matchBase;
      juzgadoOtra = "";
      return;
    }
    juzgadoSeleccionado = "Otra";
    juzgadoOtra = limpio;
  }

  async function agregarJuzgadoUsuario(nombre) {
    try {
      const key = normalizarClaveCatalogo(nombre);
      const prevKeys = new Set(juzgadosGuardados.map((j) => normalizarClaveCatalogo(j)));
      const cfg = await invoke("cmd_agregar_juzgado", { nombre });
      const nuevaLista = cfg.juzgados || [];
      const enUsuario = nuevaLista.find((j) => normalizarClaveCatalogo(j) === key);
      const enBase = JUZGADOS_BASE.find((j) => normalizarClaveCatalogo(j) === key);
      const canonical = enUsuario || enBase || nombre.trim().replace(/\s+/g, " ");
      const yaExistia = Boolean(enBase) || prevKeys.has(key);
      juzgadosGuardados = nuevaLista;
      juzgadoOtra = "";
      cargarResumenAprendizaje();
      mostrarToast(
        yaExistia
          ? `'${canonical}' ya existe en la lista de juzgados`
          : `'${canonical}' agregado a la lista de juzgados`,
        yaExistia ? "info" : "success",
        2400
      );
      return canonical;
    } catch (e) {
      mostrarToast(String(e), "error");
      return null;
    }
  }

  async function eliminarJuzgadoUsuario(nombre) {
    try {
      const cfg = await invoke("cmd_eliminar_juzgado", { nombre });
      juzgadosGuardados = cfg.juzgados || [];
      cargarResumenAprendizaje();
      mostrarToast(`'${nombre}' eliminado de la lista`, "info", 2200);
    } catch (e) {
      mostrarToast(String(e), "error");
    }
  }

  async function agregarTipoAudienciaUsuario(nombre) {
    try {
      const key = normalizarClaveCatalogo(nombre);
      const prevKeys = new Set(tiposGuardados.map((t) => normalizarClaveCatalogo(t)));
      const cfg = await invoke("cmd_agregar_tipo_audiencia", { nombre });
      const nuevaLista = cfg.tipos_audiencia || [];
      const enUsuario = nuevaLista.find((t) => normalizarClaveCatalogo(t) === key);
      const enBase = TIPOS_AUDIENCIA.find(
        (t) => t !== "Otra" && normalizarClaveCatalogo(t) === key
      );
      const canonical = enUsuario || enBase || nombre.trim().replace(/\s+/g, " ");
      const yaExistia = Boolean(enBase) || prevKeys.has(key);
      tiposGuardados = nuevaLista;
      tipoOtra = "";
      cargarResumenAprendizaje();
      mostrarToast(
        yaExistia
          ? `'${canonical}' ya existe en los tipos de audiencia`
          : `'${canonical}' agregado a los tipos de audiencia`,
        yaExistia ? "info" : "success",
        2400
      );
      return canonical;
    } catch (e) {
      mostrarToast(String(e), "error");
      return null;
    }
  }

  async function eliminarTipoAudienciaUsuario(nombre) {
    try {
      const cfg = await invoke("cmd_eliminar_tipo_audiencia", { nombre });
      tiposGuardados = cfg.tipos_audiencia || [];
      cargarResumenAprendizaje();
      mostrarToast(`'${nombre}' eliminado de la lista`, "info", 2200);
    } catch (e) {
      mostrarToast(String(e), "error");
    }
  }

  function cargarRegistroParaEdicion(reg) {
    limpiarFormulario();
    modoEdicion = true;
    filaEditando = reg.fila_excel;
    numeroEditando = reg.numero;
    radicadoEditando = reg.radicado;
    radicado = reg.radicado;
    aplicarJuzgadoAlFormulario(reg.juzgado);
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
    aplicarJuzgadoAlFormulario(reg.juzgado);
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
          cargarResumenAprendizaje();
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

  async function cargarDiagnosticoIa() {
    cargandoDiagnosticoIa = true;
    try {
      diagnosticoIa = await invoke("cmd_diagnostico_aprendizaje_ia");
    } catch {
      diagnosticoIa = null;
    } finally {
      cargandoDiagnosticoIa = false;
    }
  }

  function abrirConfiguracionIa() {
    nuevaOpenAiKey = "";
    nuevaGeminiKey = "";
    mostrarOpenAiKey = false;
    mostrarGeminiKey = false;
    modalActivo = "config_ia";
    cargarDiagnosticoIa();
  }

  function abrirAutocompletarIa() {
    if (modoEdicion) {
      mostrarToast("Finaliza o cancela la edición actual antes de usar la IA", "info", 2800);
      return;
    }
    if (!aiStatus.configured) {
      abrirConfiguracionIa();
      return;
    }
    vistaActiva = "formulario";
    if (textoIa.trim()) {
      procesarConIa();
    } else {
      errorIa = "";
      setTimeout(() => {
        document.getElementById("ia-inline-textarea")?.focus();
      }, 15);
    }
  }

  async function procesarConIa() {
    if (modoEdicion) {
      return;
    }
    if (!textoIa.trim()) {
      errorIa = "Pega el texto de la audiencia";
      return;
    }
    errorIa = "";
    procesandoIa = true;
    try {
      const datos = await invoke("cmd_autocompletar_ia", { texto: textoIa });
      radicado = datos.radicado || "";
      aplicarTipoAlFormulario(
        datos.tipo_audiencia === "Otra"
          ? datos.tipo_otra || "Otra"
          : datos.tipo_audiencia || "",
        datos.tipo_otra || ""
      );
      fecha = datos.fecha || "";
      hora = datos.hora || "";
      minuto = datos.minuto || "";
      aplicarJuzgadoAlFormulario(datos.juzgado || "");
      if (datos.se_realizo === "SI" || datos.se_realizo === "NO") {
        realizada = datos.se_realizo;
        motivoSeleccionado = datos.se_realizo === "NO" ? datos.motivo || "" : "";
      } else {
        realizada = "";
        motivoSeleccionado = "";
      }
      observaciones = datos.observaciones || "";
      formResetTick += 1;

      // Foto de lo que quedó en el formulario tras la IA, para aprender de las correcciones
      iaSnapshot = {
        texto: textoIa,
        prediccion: construirPayloadFormulario(),
        reglas_inyectadas: datos.reglas_inyectadas || [],
        ejemplos_inyectados: datos.ejemplos_inyectados || [],
        prediccion_local: datos.prediccion_local || null,
      };

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

  // Aprendizaje silencioso: compara lo propuesto por la IA con lo guardado (no bloquea al usuario)
  async function registrarResultadoIa(snapshot, datosFinales) {
    try {
      aprendizaje = await invoke("cmd_registrar_resultado_ia", {
        texto: snapshot.texto,
        prediccion: snapshot.prediccion,
        finalUsuario: datosFinales,
        reglasInyectadas: snapshot.reglas_inyectadas || [],
      });
    } catch (e) {
      console.warn("No se pudo registrar el aprendizaje de la IA:", e);
    }
  }

  async function cargarResumenAprendizaje() {
    try {
      aprendizaje = await invoke("cmd_resumen_aprendizaje_ia");
    } catch (_) {}
  }

  async function responderAclaracionIa(nota) {
    const id = aprendizaje.aclaracion_pendiente?.correccion_id;
    if (!id) return;
    try {
      aprendizaje = await invoke("cmd_responder_aclaracion_ia", {
        correccionId: id,
        nota,
      });
      mostrarToast("Regla guardada en el aprendizaje de la IA", "success", 3200);
    } catch (e) {
      mostrarToast(`No se pudo guardar la aclaración: ${e}`, "error");
    }
  }

  async function omitirAclaracionIa() {
    const id = aprendizaje.aclaracion_pendiente?.correccion_id;
    if (!id) return;
    try {
      aprendizaje = await invoke("cmd_descartar_aclaracion_ia", {
        correccionId: id,
      });
    } catch (_) {
      aprendizaje = { ...aprendizaje, aclaracion_pendiente: null };
    }
  }

  async function exportarAprendizajeIa() {
    if (exportandoAprendizaje) return;
    exportandoAprendizaje = true;
    try {
      const ruta = await invoke("cmd_exportar_aprendizaje_ia");
      aprendizaje = {
        ...aprendizaje,
        pendientes_exportar: 0,
        pendientes_ia: 0,
        pendientes_uso: 0,
      };
      mostrarToast(
        `Uso de catálogos y aprendizaje exportados en: ${ruta}`,
        "success",
        5000
      );
    } catch (e) {
      mostrarToast(String(e), "info", 4200);
    } finally {
      exportandoAprendizaje = false;
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
      } else if (vistaActiva === "estadisticas") {
        cargarEstadisticas();
        mostrarToast("Estadísticas actualizadas", "info", 1400);
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
    } else if (key === "1") {
      e.preventDefault();
      cambiarVista("formulario");
    } else if (key === "2") {
      e.preventDefault();
      cambiarVista("listado");
    } else if (key === "3") {
      e.preventDefault();
      cambiarVista("estadisticas");
    } else if (key === "f" || key === "e") {
      e.preventDefault();
      cambiarVista(vistaActiva === "listado" ? "formulario" : "listado");
    } else if (key === "i") {
      e.preventDefault();
      abrirAutocompletarIa();
    } else if (key === "l") {
      e.preventDefault();
      limpiarFormulario();
    } else if (key === "t") {
      e.preventDefault();
      toggleTema();
    } else if (key === "b") {
      e.preventDefault();
      toggleSidebar();
    }
  }

  function toggleSidebar() {
    const esPantallaEstrecha =
      typeof window !== "undefined" && window.matchMedia("(max-width: 960px)").matches;
    if (esPantallaEstrecha) {
      sidebarMobileOpen = !sidebarMobileOpen;
      return;
    }
    sidebarColapsada = !sidebarColapsada;
    try {
      localStorage.setItem("sidebar-collapsed", String(sidebarColapsada));
    } catch (_) {}
  }

  onMount(async () => {
    try {
      sidebarColapsada = localStorage.getItem("sidebar-collapsed") === "true";
    } catch (_) {}
    await cargarConfigInicial();
    cargarResumenAprendizaje();
    await refrescarListaArchivos();
    if (archivosDisponibles.length > 0 && !archivoActual) {
      archivoActual = archivosDisponibles[0];
      await recargarArchivoActual();
    }
  });
</script>

<svelte:window onkeydown={manejarTecladoGlobal} />

<div class="desktop-frame {sidebarColapsada ? 'sidebar-collapsed' : ''}">
  <Sidebar
    {archivosDisponibles}
    {carpetasDisponibles}
    {ordenCarpetas}
    {coloresCarpetas}
    {archivoActual}
    {totales}
    {aiStatus}
    theme={settings.theme}
    mobileOpen={sidebarMobileOpen}
    onCloseMobile={() => (sidebarMobileOpen = false)}
    onCrearArchivo={abrirCrearArchivoModal}
    onCrearCarpeta={abrirCrearCarpetaModal}
    onRenombrarCarpeta={abrirRenombrarCarpetaModal}
    onCambiarColorCarpeta={abrirColorCarpetaModal}
    onEliminarCarpeta={eliminarCarpetaSolicitada}
    onMoverArchivo={abrirMoverArchivoModal}
    onMoverPorArrastre={moverPorArrastre}
    onReordenarCarpetas={guardarOrdenCarpetas}
    onSeleccionarArchivo={seleccionarArchivo}
    onAbrirDefensorArchivo={abrirModalDefensoresArchivo}
    onAbrirRespaldos={abrirRespaldos}
    onDescargarArchivo={descargarArchivo}
    onEliminarArchivo={eliminarArchivo}
    onAbrirConfigIa={abrirConfiguracionIa}
    onAbrirAtajos={() => (modalActivo = "atajos")}
    onToggleTema={toggleTema}
  />

  <div class="workspace-pane">
    <header class="workspace-header">
      <div class="header-nav-wrap">
        <button
          type="button"
          class="mini-icon-btn sidebar-toggle-btn"
          onclick={toggleSidebar}
          aria-expanded={!sidebarColapsada}
          title={sidebarColapsada
            ? "Mostrar panel de planillas (Ctrl+B)"
            : "Ocultar panel de planillas (Ctrl+B)"}
          aria-label={sidebarColapsada
            ? "Mostrar panel de planillas"
            : "Ocultar panel de planillas"}
        >
          {#if sidebarColapsada}
            <PanelLeftOpen size={16} />
          {:else}
            <PanelLeftClose size={16} />
          {/if}
        </button>

        <div class="topbar-nav" role="tablist" aria-label="Secciones principales">
          <button
            type="button"
            role="tab"
            aria-selected={vistaActiva === "formulario"}
            class="topbar-tab {vistaActiva === 'formulario' ? 'active' : ''}"
            onclick={() => cambiarVista("formulario")}
          >
            <FileText size={14} strokeWidth={2} />
            <span>Nueva Audiencia</span>
          </button>

          <span class="topbar-sep" aria-hidden="true"></span>

          <button
            type="button"
            role="tab"
            aria-selected={vistaActiva === "listado"}
            class="topbar-tab {vistaActiva === 'listado' ? 'active' : ''}"
            onclick={() => cambiarVista("listado")}
          >
            <List size={14} strokeWidth={2} />
            <span>Registros</span>
            <span class="tab-count">{totales.num_registros}</span>
          </button>

          <span class="topbar-sep" aria-hidden="true"></span>

          <button
            type="button"
            role="tab"
            aria-selected={vistaActiva === "estadisticas"}
            class="topbar-tab {vistaActiva === 'estadisticas' ? 'active' : ''}"
            onclick={() => cambiarVista("estadisticas")}
          >
            <BarChart3 size={14} strokeWidth={2} />
            <span>Estadísticas</span>
          </button>
        </div>
      </div>

      <div class="header-right">
        <button
          type="button"
          class="mini-icon-btn share-learning-btn"
          onclick={() => abrirModalPortabilidad("exportar")}
          title="Exportar o importar respaldos y estadísticas"
          aria-label="Exportar o importar datos"
        >
          <Package size={15} />
        </button>

        <button
          type="button"
          class="mini-icon-btn share-learning-btn"
          onclick={exportarAprendizajeIa}
          disabled={exportandoAprendizaje}
          title={aprendizaje.puede_exportar || aprendizaje.total_correcciones > 0 || aprendizaje.total_audiencias_contabilizadas > 0
            ? `Compartir uso de catálogos y aprendizaje IA (${aprendizaje.total_audiencias_contabilizadas ?? 0} audiencias, ${aprendizaje.total_correcciones} correcciones, ${aprendizaje.pendientes_exportar} novedades)`
            : "Compartir uso de catálogos y aprendizaje IA (aún sin registros ni correcciones)"}
          aria-label="Compartir uso de catálogos y aprendizaje IA"
        >
          <Share2 size={15} />
          {#if aprendizaje.pendientes_exportar > 0}
            <span class="share-badge" aria-hidden="true">
              {aprendizaje.pendientes_exportar > 99 ? "99+" : aprendizaje.pendientes_exportar}
            </span>
          {/if}
        </button>
      </div>
    </header>

    {#if vistaActiva !== "estadisticas"}
      <div class="active-doc-bar">
        <div class="active-doc-left">
          <FileSpreadsheet size={14} strokeWidth={2} class="active-doc-icon" />
          <span class="active-doc-prefix">Trabajando en:</span>
          {#if archivoActual}
            {@const idxSlash = archivoActual.lastIndexOf("/")}
            {@const carpetaPart = idxSlash >= 0 ? archivoActual.slice(0, idxSlash) : ""}
            {@const nombrePart = idxSlash >= 0 ? archivoActual.slice(idxSlash + 1) : archivoActual}
            {#if carpetaPart}
              <span class="active-doc-folder">{carpetaPart.replaceAll("/", " / ")} /</span>
            {/if}
            <strong class="active-doc-name" title={archivoActual}>
              {nombrePart}
            </strong>
          {:else}
            <span class="active-doc-empty">Ninguna planilla abierta</span>
          {/if}
        </div>

        <div class="active-doc-right">
          {#if archivoActual}
            <button
              type="button"
              class="active-doc-defensor-btn {defensorArchivo ? '' : 'is-empty'}"
              onclick={() => abrirModalDefensoresArchivo(archivoActual)}
              title={defensorArchivo
                ? `Defensor asignado: ${defensorArchivo} (clic para cambiar)`
                : "Asignar Defensor Público a esta planilla"}
            >
              <UserCheck size={12} />
              <span>{defensorArchivo || "Asignar defensor"}</span>
            </button>
            <span class="active-doc-sep" aria-hidden="true">·</span>
            <span class="active-doc-meta">{totales.num_registros}/100 reg.</span>
          {/if}
          <button
            type="button"
            class="inline-action-link active-doc-switch"
            onclick={() => (sidebarMobileOpen = true)}
          >
            Cambiar
          </button>
        </div>
      </div>
    {/if}

    <main class="workspace-scroll scroller">
      {#if vistaActiva === "formulario"}
        <AudienciaForm
          {archivoActual}
          {registros}
          {juzgadosGuardados}
          {tiposGuardados}
          {totales}
          {aiStatus}
          bind:textoIa
          {procesandoIa}
          {errorIa}
          {modoEdicion}
          {filaEditando}
          {numeroEditando}
          bind:radicado
          bind:juzgadoSeleccionado
          bind:juzgadoOtra
          bind:tipoSeleccionado
          bind:tipoOtra
          bind:realizada
          bind:fecha
          bind:hora
          bind:minuto
          bind:motivoSeleccionado
          bind:observaciones
          bind:inputRadicadoEl
          {formResetTick}
          onSubmit={() => guardarOActualizar(false)}
          onCancelarEdicion={cancelarEdicion}
          onLimpiar={limpiarFormulario}
          onProcesarConIa={procesarConIa}
          onAbrirConfigIa={abrirConfiguracionIa}
          onVerTodos={() => cambiarVista("listado")}
          onEditarRegistro={cargarRegistroParaEdicion}
          onAgregarJuzgado={agregarJuzgadoUsuario}
          onEliminarJuzgado={eliminarJuzgadoUsuario}
          onAgregarTipo={agregarTipoAudienciaUsuario}
          onEliminarTipo={eliminarTipoAudienciaUsuario}
          aclaracionPendiente={aprendizaje.aclaracion_pendiente}
          onResponderAclaracion={responderAclaracionIa}
          onOmitirAclaracion={omitirAclaracionIa}
        />
      {:else if vistaActiva === "listado"}
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
      {:else}
        <EstadisticasView
          stats={estadisticas}
          cargando={cargandoEstadisticas}
          bind:mesSeleccionado={statsMesSeleccionado}
          bind:archivoFiltro={statsArchivoFiltro}
          bind:defensorFiltro={statsDefensorFiltro}
          onFiltrarChange={cargarEstadisticas}
          onAbrirPlanilla={abrirPlanillaDesdeStats}
          onCopiarResumen={copiarResumenEstadisticas}
          onEliminarArchivada={solicitarEliminarArchivada}
        />
      {/if}
    </main>
  </div>
</div>

<Modals
  bind:modalActivo
  {defensorTargetArchivo}
  {defensorTargetNombre}
  {defensores}
  {carpetasDisponibles}
  bind:settings
  {aiStatus}
  bind:nombreNuevoArchivo
  bind:defensorNuevoArchivo
  bind:carpetaNuevoArchivo
  {errorModalCrear}
  bind:carpetaPadreNueva
  bind:nombreNuevaCarpeta
  bind:colorNuevaCarpeta
  {carpetaRenombrarActual}
  bind:carpetaRenombrarNueva
  bind:colorCarpetaRenombrar
  {colorCarpetaTarget}
  bind:colorCarpetaEditando
  {errorModalCarpeta}
  {archivoMoverTarget}
  bind:carpetaDestinoMover
  bind:carpetaDestinoNueva
  {errorModalMover}
  {eliminarTargetNombre}
  {eliminarTargetResumen}
  {eliminarCarpetaTarget}
  bind:conservarEstadisticasAlEliminar
  bind:portabilidadTab
  bind:tipoExportacion
  {exportandoPaquete}
  {archivoImportNombre}
  {inspeccionImportacion}
  {errorImportacion}
  bind:modoConflictoImport
  bind:importarConfigPaquete
  {importandoPaquete}
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
  {diagnosticoIa}
  {cargandoDiagnosticoIa}
  onRefrescarDiagnosticoIa={cargarDiagnosticoIa}
  onCrearNuevoArchivo={crearNuevoArchivo}
  onCrearCarpeta={crearNuevaCarpeta}
  onRenombrarCarpeta={renombrarCarpetaExistente}
  onGuardarColorCarpeta={guardarColorCarpetaModal}
  onMoverArchivo={moverArchivoACarpeta}
  onConfirmarEliminarArchivo={confirmarEliminacionConOpciones}
  onConfirmarEliminarCarpeta={confirmarEliminacionCarpeta}
  onExportarPaquete={exportarPaqueteLokus}
  onSeleccionarArchivoImportar={seleccionarArchivoImportar}
  onEjecutarImportacion={ejecutarImportacionPaquete}
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
