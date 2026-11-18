<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { FileText, List, PanelLeft, BarChart3, Share2 } from "@lucide/svelte";
  import {
    TIPOS_AUDIENCIA,
    MOTIVOS_LABELS,
    JUZGADOS_BASE,
  } from "./lib/constants.js";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import AudienciaForm from "./lib/components/AudienciaForm.svelte";
  import RegistrosTable from "./lib/components/RegistrosTable.svelte";
  import EstadisticasView from "./lib/components/EstadisticasView.svelte";
  import Modals from "./lib/components/Modals.svelte";

  // Navegación y archivos
  let vistaActiva = $state("formulario"); // "formulario" | "listado" | "estadisticas"
  let sidebarMobileOpen = $state(false);
  let archivoActual = $state(null);
  let archivosDisponibles = $state([]);
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

  // Aprendizaje de la IA: foto de lo que propuso el autocompletado para compararlo al guardar
  let iaSnapshot = $state(null); // { texto, prediccion } | null
  let aprendizaje = $state({
    aciertos: 0,
    total_correcciones: 0,
    total_reglas: 0,
    pendientes_exportar: 0,
    aclaracion_pendiente: null,
  });
  let exportandoAprendizaje = $state(false);

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
    await seleccionarArchivo(nombreArchivo);
    vistaActiva = "listado";
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
        if (snapshotIa) registrarResultadoIa(snapshotIa, datos);
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
    if (limpio === "Otra") {
      tipoSeleccionado = "Otra";
      tipoOtra = (otraExplicita || "").trim();
      return;
    }
    const lower = limpio.toLowerCase();
    const matchGuardado = tiposGuardados.find((t) => t.toLowerCase() === lower);
    if (matchGuardado) {
      tipoSeleccionado = matchGuardado;
      tipoOtra = "";
      return;
    }
    const matchBase = TIPOS_AUDIENCIA.find(
      (t) => t !== "Otra" && t.toLowerCase() === lower
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
    const lower = limpio.toLowerCase();
    const matchGuardado = juzgadosGuardados.find(
      (j) => j.toLowerCase() === lower
    );
    if (matchGuardado) {
      juzgadoSeleccionado = matchGuardado;
      juzgadoOtra = "";
      return;
    }
    const matchBase = JUZGADOS_BASE.find((j) => j.toLowerCase() === lower);
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
      const cfg = await invoke("cmd_agregar_juzgado", { nombre });
      juzgadosGuardados = cfg.juzgados || [];
      juzgadoOtra = "";
      mostrarToast(`'${nombre}' agregado a la lista de juzgados`, "success", 2400);
    } catch (e) {
      mostrarToast(String(e), "error");
    }
  }

  async function eliminarJuzgadoUsuario(nombre) {
    try {
      const cfg = await invoke("cmd_eliminar_juzgado", { nombre });
      juzgadosGuardados = cfg.juzgados || [];
      mostrarToast(`'${nombre}' eliminado de la lista`, "info", 2200);
    } catch (e) {
      mostrarToast(String(e), "error");
    }
  }

  async function agregarTipoAudienciaUsuario(nombre) {
    try {
      const cfg = await invoke("cmd_agregar_tipo_audiencia", { nombre });
      tiposGuardados = cfg.tipos_audiencia || [];
      tipoOtra = "";
      mostrarToast(`'${nombre}' agregado a los tipos de audiencia`, "success", 2400);
    } catch (e) {
      mostrarToast(String(e), "error");
    }
  }

  async function eliminarTipoAudienciaUsuario(nombre) {
    try {
      const cfg = await invoke("cmd_eliminar_tipo_audiencia", { nombre });
      tiposGuardados = cfg.tipos_audiencia || [];
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
        aplicarTipoAlFormulario(
          datos.tipo_audiencia === "Otra"
            ? datos.tipo_otra || "Otra"
            : datos.tipo_audiencia,
          datos.tipo_otra
        );
      }
      if (datos.fecha) fecha = datos.fecha;
      hora = datos.hora || "";
      minuto = datos.minuto || "";
      if (datos.juzgado) aplicarJuzgadoAlFormulario(datos.juzgado);
      if (datos.se_realizo === "SI" || datos.se_realizo === "NO") {
        realizada = datos.se_realizo;
        motivoSeleccionado = datos.se_realizo === "NO" ? datos.motivo || "" : "";
      }
      if (datos.observaciones) observaciones = datos.observaciones;

      // Foto de lo que quedó en el formulario tras la IA, para aprender de las correcciones
      iaSnapshot = { texto: textoIa, prediccion: construirPayloadFormulario() };

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
      aprendizaje = { ...aprendizaje, pendientes_exportar: 0 };
      mostrarToast(`Aprendizaje de la IA exportado en: ${ruta}`, "success", 5000);
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
    }
  }

  onMount(async () => {
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

<div class="desktop-frame">
  <Sidebar
    {archivosDisponibles}
    {archivoActual}
    {totales}
    {aiStatus}
    theme={settings.theme}
    mobileOpen={sidebarMobileOpen}
    onCloseMobile={() => (sidebarMobileOpen = false)}
    onCrearArchivo={abrirCrearArchivoModal}
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
          class="mini-icon-btn mobile-menu-btn"
          onclick={() => (sidebarMobileOpen = true)}
          title="Mostrar lista de planillas"
          aria-label="Mostrar panel de planillas"
        >
          <PanelLeft size={16} />
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
          onclick={exportarAprendizajeIa}
          disabled={exportandoAprendizaje}
          title={aprendizaje.total_correcciones > 0
            ? `Compartir aprendizaje de la IA (${aprendizaje.total_correcciones} correcciones, ${aprendizaje.pendientes_exportar} nuevas)`
            : "Compartir aprendizaje de la IA (aún sin correcciones)"}
          aria-label="Compartir aprendizaje de la IA"
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
