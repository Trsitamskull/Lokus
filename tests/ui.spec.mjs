import { test, expect } from "@playwright/test";

function mockTauriInit(opts = {}) {
  const configured = Boolean(opts?.configured);
  const sinDefensorInicial = Boolean(opts?.sinDefensor);
  const carpetasIniciales = [
    "2026",
    "2026/Enero",
    ...(Array.isArray(opts?.extraCarpetas) ? opts.extraCarpetas : []),
  ];
  let carpetas = [...carpetasIniciales];
  let ordenCarpetas = [...carpetasIniciales];
  const extraArchivos = Array.isArray(opts?.extraArchivos) ? opts.extraArchivos : [];
  let archivos = ["2026/Enero/audiencias_enero.xlsx", ...extraArchivos];
  const defensoresPorArchivo = {
    "2026/Enero/audiencias_enero.xlsx": sinDefensorInicial ? "" : "Dra. Laura Gómez",
  };
  for (const extra of extraArchivos) {
    defensoresPorArchivo[extra] = opts?.defensoresPorArchivo?.[extra] ?? "Dra. Laura Gómez";
  }
  const colores = { "2026": "azul", "2026/Enero": "verde" };
  let defensores = [
    "Dra. Laura Gómez",
    "Dr. Carlos Mendoza",
    ...(Array.isArray(opts?.extraDefensores) ? opts.extraDefensores : []),
  ];
  let providerActual = "openai";
  let openaiModelActual = "gpt-5-mini";
  let geminiModelActual = "gemini-2.5-flash";
  let temaActual = "dark";
  let consolidacionIaActiva = opts?.consolidacion_ia_activa ?? true;
  let usarResumenConsolidado = opts?.usar_resumen_consolidado ?? true;
  let juzgadosUsuario = [];
  let tiposUsuario = [];

  let aciertos = 12;
  let totalCorrecciones = 2;
  let totalReglas = 3;
  let pendientesExportar = 3;
  let pendientesIa = 1;
  let pendientesUso = 2;
  let totalAudienciasContabilizadas = 5;
  let aclaracionPendiente = null;
  let seqCorr = 0;
  let consolidacionesTotales = 0;
  let presionGlobal = 1.45;
  let ultimaConsolidacion = null;
  let perfilesSinteticos = Array.isArray(opts?.perfilesIniciales)
    ? [...opts.perfilesIniciales]
    : [];

  function normalizarCmp(v) {
    return String(v || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/\s+/g, " ")
      .trim()
      .toLowerCase();
  }

  function normalizarClaveDef(v) {
    return normalizarCmp(v).replace(/^(?:defensora?\s+public[ao]|doctor[a]?|dra?|abg|lic)\.?\s+/i, "");
  }

  function construirResumen(huboCorreccion = false) {
    return {
      aciertos,
      total_correcciones: totalCorrecciones,
      total_reglas: totalReglas,
      pendientes_exportar: pendientesExportar,
      pendientes_ia: pendientesIa,
      pendientes_uso: pendientesUso,
      total_audiencias_contabilizadas: totalAudienciasContabilizadas,
      puede_exportar: true,
      hubo_correccion: huboCorreccion,
      aclaracion_pendiente: aclaracionPendiente,
    };
  }

  function construirDiagnostico() {
    return {
      aciertos_totales: aciertos,
      correcciones_totales: totalCorrecciones,
      ejemplos_positivos_totales: 10,
      llamadas_induccion_llm_total: 1,
      clusters_pendientes: 1,
      clusters_listos: 0,
      perfiles_defensores: 2,
      conexiones_grafo: 14,
      drift_campos_activos: [],
      enfriamiento_aclaracion: 0,
      reglas_por_estado: { activa: 2, prueba: 1 },
      metricas_por_campo: [
        {
          campo: "realizada",
          evaluaciones: 14,
          aciertos: 13,
          correcciones: 1,
          precision_ewma: 0.93,
          confianza_q10: 0.72,
          drift_activo: false,
        },
        {
          campo: "motivo",
          evaluaciones: 14,
          aciertos: 12,
          correcciones: 2,
          precision_ewma: 0.86,
          confianza_q10: 0.61,
          drift_activo: false,
        },
        {
          campo: "hora",
          evaluaciones: 0,
          aciertos: 0,
          correcciones: 0,
          precision_ewma: 0.5,
          confianza_q10: 0.24,
          drift_activo: false,
        },
      ],
      metricas_por_modelo: [
        {
          clave: `${providerActual}:${providerActual === "gemini" ? geminiModelActual : openaiModelActual}`,
          evaluaciones: 14,
          aciertos: 12,
          correcciones: 2,
          precision_media: 0.86,
        },
      ],
      preferencias_estilo: {
        incluir_causas_secundarias_prob: 0.75,
        incluir_reprogramacion_prob: 0.8,
        se_fija_aplazamiento_prob: 0.5,
        instalada_reprogramada_si_prob: 0.4,
      },
      alias_validados: [],
      reglas_destacadas: [
        {
          id: "base-1",
          estado: "activa",
          campo_objetivo: "juzgado",
          origen: "base_invariante",
          defensor_alcance: "*",
          regla: "Conservar abreviaciones CTO, MPAL y PCTO tal como aparecen",
          impresiones: 6,
          aciertos_directos: 6,
          fallos_directos: 0,
          confianza_q10: 0.81,
        },
      ],
      consolidaciones_totales: consolidacionesTotales,
      presion_consolidacion_global: presionGlobal,
      presiones_por_defensor: [
        {
          clave: "*",
          nombre: "General (transversal)",
          presion: presionGlobal,
          correcciones_sin_consolidar: 2,
          notas_sin_consolidar: 0,
          extracciones_desde_ultima: 8,
          ultima_consolidacion: ultimaConsolidacion?.fecha || "",
        },
        {
          clave: "laura gomez",
          nombre: "Dra. Laura Gómez",
          presion: 2.1,
          correcciones_sin_consolidar: 2,
          notas_sin_consolidar: 1,
          extracciones_desde_ultima: 6,
          ultima_consolidacion: "",
        },
      ],
      ultima_consolidacion: ultimaConsolidacion,
      perfiles_sinteticos: perfilesSinteticos,
      tasa_respuesta_aclaraciones: 0.8,
      defensores_resumen: [
        {
          clave: "laura gomez",
          nombre: "Dra. Laura Gómez",
          aciertos: 10,
          correcciones: 2,
          tasa_acierto: 0.833,
          por_campo: [],
        },
        {
          clave: "carlos mendoza",
          nombre: "Dr. Carlos Mendoza",
          aciertos: 2,
          correcciones: 0,
          tasa_acierto: 1.0,
          por_campo: [],
        },
      ],
    };
  }

  function construirStatus() {
    return {
      configured,
      provider: providerActual,
      model: providerActual === "gemini" ? geminiModelActual : openaiModelActual,
      has_openai_key: configured,
      has_gemini_key: false,
      masked_openai_key: configured ? "sk-***1234" : "",
      masked_gemini_key: "",
    };
  }

  window.__LOKUS_TEST_CALLS__ = {
    crearArchivo: [],
    moverArchivo: [],
    renombrarArchivo: [],
    guardarOrdenCarpetas: [],
    renombrarCarpeta: [],
    autocompletarIa: [],
    registrarResultadoIa: [],
    actualizarDefensor: [],
    renombrarDefensor: [],
    eliminarDefensor: [],
    guardarConfig: [],
    responderAclaracionIa: [],
    descartarAclaracionIa: [],
    consolidarMemoriaIa: [],
    exportarPaquete: [],
    importarPaquete: [],
  };

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args = {}) => {
      switch (cmd) {
        case "cmd_crear_archivo": {
          window.__LOKUS_TEST_CALLS__.crearArchivo.push(args);
          const defTrim = (args.defensor || "").trim();
          if (!defTrim) {
            throw "Debes asignar un Defensor Público para crear la planilla.";
          }
          const nombreBase = (args.nombre || "").trim().replace(/\.xlsx$/i, "");
          const rel = args.carpeta ? `${args.carpeta}/${nombreBase}.xlsx` : `${nombreBase}.xlsx`;
          if (!archivos.includes(rel)) {
            archivos = [...archivos, rel];
          }
          defensoresPorArchivo[rel] = defTrim;
          return rel;
        }
        case "cmd_agregar_defensor": {
          const limpio = (args.nombre || "").trim();
          if (limpio && !defensores.some((d) => d.toLowerCase() === limpio.toLowerCase())) {
            defensores = [...defensores, limpio];
          }
          return { defensores };
        }
        case "cmd_renombrar_defensor": {
          window.__LOKUS_TEST_CALLS__.renombrarDefensor.push(args);
          const actual = (args.actual || "").trim();
          const nuevo = (args.nuevo || "").trim().replace(/\s+/g, " ");
          if (!nuevo) {
            throw "El nuevo nombre del defensor no puede estar vacío.";
          }
          const idx = defensores.findIndex((d) => d.toLowerCase() === actual.toLowerCase());
          const nuevaClave = normalizarClaveDef(nuevo);
          const colision = defensores.some(
            (d, i) => i !== idx && (d.toLowerCase() === nuevo.toLowerCase() || normalizarClaveDef(d) === nuevaClave)
          );
          if (colision) {
            throw `Ya existe otro defensor llamado '${nuevo}'.`;
          }
          if (idx >= 0) {
            defensores = defensores.map((d, i) => (i === idx ? nuevo : d));
          } else {
            defensores = [...defensores, nuevo];
          }
          return { defensores };
        }
        case "cmd_eliminar_defensor": {
          window.__LOKUS_TEST_CALLS__.eliminarDefensor.push(args);
          const targetLower = (args.nombre || "").trim().toLowerCase();
          const targetClave = normalizarClaveDef(args.nombre || "");
          defensores = defensores.filter(
            (d) => d.toLowerCase() !== targetLower && normalizarClaveDef(d) !== targetClave
          );
          return { defensores };
        }
        case "cmd_actualizar_defensor_archivo": {
          window.__LOKUS_TEST_CALLS__.actualizarDefensor.push(args);
          const limpio = (args.defensor || "").trim();
          const target = args.nombreArchivo || args.nombre;
          if (target) {
            defensoresPorArchivo[target] = limpio;
          }
          return limpio;
        }
        case "cmd_obtener_config":
          return {
            provider: providerActual,
            openai_model: openaiModelActual,
            gemini_model: geminiModelActual,
            theme: temaActual,
            defensores,
            juzgados: juzgadosUsuario,
            tipos_audiencia: tiposUsuario,
            consolidacion_ia_activa: consolidacionIaActiva,
            usar_resumen_consolidado: usarResumenConsolidado,
            status: construirStatus(),
          };
        case "cmd_guardar_config": {
          window.__LOKUS_TEST_CALLS__.guardarConfig.push(args);
          const cfg = args.config || {};
          if (cfg.provider) providerActual = cfg.provider;
          if (cfg.openai_model) openaiModelActual = cfg.openai_model;
          if (cfg.gemini_model) geminiModelActual = cfg.gemini_model;
          if (cfg.theme) temaActual = cfg.theme;
          if (typeof cfg.consolidacion_ia_activa === "boolean") {
            consolidacionIaActiva = cfg.consolidacion_ia_activa;
          }
          if (typeof cfg.usar_resumen_consolidado === "boolean") {
            usarResumenConsolidado = cfg.usar_resumen_consolidado;
          }
          return {
            provider: providerActual,
            openai_model: openaiModelActual,
            gemini_model: geminiModelActual,
            theme: temaActual,
            defensores,
            juzgados: juzgadosUsuario,
            tipos_audiencia: tiposUsuario,
            consolidacion_ia_activa: consolidacionIaActiva,
            usar_resumen_consolidado: usarResumenConsolidado,
            status: construirStatus(),
          };
        }
        case "cmd_agregar_juzgado": {
          const limpio = (args.nombre || "").trim().replace(/\s+/g, " ");
          const norm = limpio
            .normalize("NFD")
            .replace(/[\u0300-\u036f]/g, "")
            .replace(/[^\p{L}\p{N}]+/gu, " ")
            .trim()
            .toLowerCase();
          const yaEnBase = norm === "tribunal superior del distrito judicial de valledupar sala penal";
          if (norm && !yaEnBase && !juzgadosUsuario.some((j) => j.toLowerCase() === limpio.toLowerCase())) {
            juzgadosUsuario = [...juzgadosUsuario, limpio];
          }
          return { juzgados: juzgadosUsuario };
        }
        case "cmd_eliminar_juzgado": {
          const target = (args.nombre || "").trim().toLowerCase();
          juzgadosUsuario = juzgadosUsuario.filter((j) => j.toLowerCase() !== target);
          return { juzgados: juzgadosUsuario };
        }
        case "cmd_agregar_tipo_audiencia": {
          const limpio = (args.nombre || "").trim().replace(/\s+/g, " ");
          const norm = limpio
            .normalize("NFD")
            .replace(/[\u0300-\u036f]/g, "")
            .replace(/[^\p{L}\p{N}]+/gu, " ")
            .trim()
            .toLowerCase();
          if (norm && norm !== "audiencia de imputacion") {
            tiposUsuario = [...tiposUsuario, limpio];
          }
          return { tipos_audiencia: tiposUsuario };
        }
        case "cmd_eliminar_tipo_audiencia": {
          const target = (args.nombre || "").trim().toLowerCase();
          tiposUsuario = tiposUsuario.filter((t) => t.toLowerCase() !== target);
          return { tipos_audiencia: tiposUsuario };
        }
        case "cmd_guardar_tema":
          temaActual = args.tema || "dark";
          return {
            status: {
              configured,
              provider: "openai",
              model: "gpt-5-mini",
              has_openai_key: configured,
              has_gemini_key: false,
              masked_openai_key: configured ? "sk-***1234" : "",
              masked_gemini_key: "",
            },
          };
        case "cmd_mover_archivo": {
          window.__LOKUS_TEST_CALLS__.moverArchivo.push(args);
          const actual = String(args.nombre || "");
          const dest = String(args.carpetaDestino || "").trim();
          const idx = actual.lastIndexOf("/");
          const base = idx >= 0 ? actual.slice(idx + 1) : actual;
          const nuevaRuta = dest ? `${dest}/${base}` : base;
          if (nuevaRuta !== actual && archivos.includes(nuevaRuta)) {
            throw "Ya existe una planilla con ese nombre en la carpeta de destino.";
          }
          archivos = archivos.map((a) => (a === actual ? nuevaRuta : a));
          if (actual !== nuevaRuta) {
            defensoresPorArchivo[nuevaRuta] = defensoresPorArchivo[actual] ?? "";
            delete defensoresPorArchivo[actual];
          }
          return nuevaRuta;
        }
        case "cmd_renombrar_archivo": {
          window.__LOKUS_TEST_CALLS__.renombrarArchivo.push(args);
          const actual = String(args.actual || "");
          const idx = actual.lastIndexOf("/");
          const carpetaActual = idx >= 0 ? actual.slice(0, idx) : "";
          const baseLimpia = String(args.nuevoNombre || "")
            .trim()
            .replace(/\.xlsx$/i, "");
          if (!baseLimpia) {
            throw "El nombre del archivo no es válido o está vacío.";
          }
          const nombreFinal = `${baseLimpia}.xlsx`;
          const carpetaDest =
            args.carpetaDestino !== undefined && args.carpetaDestino !== null
              ? String(args.carpetaDestino).trim()
              : carpetaActual;
          const nuevaRuta = carpetaDest ? `${carpetaDest}/${nombreFinal}` : nombreFinal;
          if (nuevaRuta !== actual && archivos.includes(nuevaRuta)) {
            throw "Ya existe una planilla con ese nombre en la carpeta de destino.";
          }
          archivos = archivos.map((a) => (a === actual ? nuevaRuta : a));
          if (actual !== nuevaRuta) {
            defensoresPorArchivo[nuevaRuta] = defensoresPorArchivo[actual] ?? "";
            delete defensoresPorArchivo[actual];
          }
          return nuevaRuta;
        }
        case "cmd_renombrar_carpeta": {
          window.__LOKUS_TEST_CALLS__.renombrarCarpeta.push(args);
          const actual = String(args.actual || "");
          const nueva = String(args.nueva || "").trim();
          const prefijo = `${actual}/`;
          carpetas = carpetas.map((c) =>
            c === actual ? nueva : c.startsWith(prefijo) ? `${nueva}/${c.slice(prefijo.length)}` : c
          );
          ordenCarpetas = ordenCarpetas.map((c) =>
            c === actual ? nueva : c.startsWith(prefijo) ? `${nueva}/${c.slice(prefijo.length)}` : c
          );
          archivos = archivos.map((a) =>
            a.startsWith(prefijo) ? `${nueva}/${a.slice(prefijo.length)}` : a
          );
          return nueva;
        }
        case "cmd_listar_archivos":
          return archivos;
        case "cmd_listar_carpetas":
          return carpetas;
        case "cmd_obtener_orden_carpetas":
          return ordenCarpetas;
        case "cmd_guardar_orden_carpetas":
          window.__LOKUS_TEST_CALLS__.guardarOrdenCarpetas.push(args);
          ordenCarpetas = Array.isArray(args.orden) ? [...args.orden] : ordenCarpetas;
          return ordenCarpetas;
        case "cmd_obtener_colores_carpetas":
          return colores;
        case "cmd_resumen_aprendizaje_ia":
          return construirResumen(false);
        case "cmd_guardar_audiencia":
          return {
            num_registros: 2,
            total_si: 2,
            total_no: 0,
            totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
          };
        case "cmd_registrar_resultado_ia": {
          window.__LOKUS_TEST_CALLS__.registrarResultadoIa.push(args);
          const pred = args.prediccion || {};
          const fin = args.finalUsuario || {};
          const camposDef = [
            "radicado",
            "tipo",
            "fecha",
            "hora",
            "juzgado",
            "realizada",
            "motivo",
            "observaciones",
          ];
          const campos = [];
          for (const campo of camposDef) {
            const vIa = String(pred[campo] || "").trim();
            const vUsr = String(fin[campo] || "").trim();
            if (normalizarCmp(vIa) !== normalizarCmp(vUsr)) {
              campos.push({ campo, valor_ia: vIa, valor_usuario: vUsr });
            }
          }
          if (campos.length > 0) {
            totalCorrecciones += 1;
            pendientesIa += 1;
            pendientesExportar += 1;
            seqCorr += 1;
            aclaracionPendiente = {
              correccion_id: `corr-${seqCorr}`,
              pregunta: `¿Por qué se marcó '${fin.realizada || campos[0].valor_usuario}' en ${campos[0].campo}?`,
              campos,
            };
            return construirResumen(true);
          }
          aciertos += 1;
          aclaracionPendiente = null;
          return construirResumen(false);
        }
        case "cmd_responder_aclaracion_ia": {
          if (window.__LOKUS_FAIL_NEXT_RESPONDER__) {
            window.__LOKUS_FAIL_NEXT_RESPONDER__ = false;
            throw "Error temporal de red";
          }
          window.__LOKUS_TEST_CALLS__.responderAclaracionIa.push(args);
          aclaracionPendiente = null;
          if (!args.esDatoExterno) {
            totalReglas += 1;
          }
          return construirResumen(false);
        }
        case "cmd_descartar_aclaracion_ia": {
          window.__LOKUS_TEST_CALLS__.descartarAclaracionIa.push(args);
          aclaracionPendiente = null;
          return construirResumen(false);
        }
        case "cmd_exportar_aprendizaje_ia":
          pendientesExportar = 0;
          pendientesIa = 0;
          pendientesUso = 0;
          return "/home/test/Descargas/lokus_uso_y_aprendizaje_ia_test.json";
        case "cmd_descargar_archivo": {
          const base = String(args.nombre || "audiencias.xlsx").split("/").pop() || "audiencias.xlsx";
          return `/data/user/0/com.josebustamante.gestoraudiencias/files/exportaciones/${base}`;
        }
        case "cmd_leer_archivo_exportado": {
          const ruta = String(args.ruta || "");
          const nombre = ruta.split("/").pop() || "audiencias.xlsx";
          const mime = nombre.endsWith(".xlsx")
            ? "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            : nombre.endsWith(".json")
              ? "application/json"
              : "application/octet-stream";
          return {
            nombre,
            mime,
            contenido_b64: "UEsDBAoAAAAAA",
          };
        }
        case "cmd_limpiar_archivo_exportado": {
          window.__LOKUS_CLEANED_EXPORTS__ = window.__LOKUS_CLEANED_EXPORTS__ || [];
          window.__LOKUS_CLEANED_EXPORTS__.push(String(args.ruta || ""));
          return null;
        }
        case "cmd_autocompletar_ia": {
          window.__LOKUS_TEST_CALLS__.autocompletarIa.push(args);
          const baseResp = {
            radicado: "200016001075202500999",
            tipo_audiencia: "Audiencia de imputacion",
            tipo_otra: "",
            fecha: "18/01/2026",
            hora: "15",
            minuto: "00",
            juzgado: "juzgado 1 promiscuo municipal de agustin codazzi",
            se_realizo: "SI",
            motivo: "",
            observaciones: "Se realizó audiencia preliminar.",
            reglas_inyectadas: [],
            ejemplos_inyectados: [],
            prediccion_local: {
              realizada: "SI",
              realizada_confianza: 0.72,
              motivo: null,
              motivo_confianza: 0.2,
              juzgado_sugerido: null,
              tipo_sugerido: null,
              radicado_sugerido: null,
              muestras_entrenamiento: 12,
              realizadas_clases_vistas: 2,
              realizada_soporte_clase: 6,
              motivos_clases_vistas: 2,
              motivo_soporte_clase: 2,
              knn_realizada_acuerdo: true,
              knn_motivo_acuerdo: false,
              drift_realizada: false,
              drift_motivo: false,
            },
          };
          return opts?.iaCustomResponse ? { ...baseResp, ...opts.iaCustomResponse } : baseResp;
        }
        case "cmd_diagnostico_aprendizaje_ia":
          return construirDiagnostico();
        case "cmd_consolidar_memoria_ia": {
          window.__LOKUS_TEST_CALLS__.consolidarMemoriaIa.push(args);
          consolidacionesTotales += 1;
          const ambRaw = String(args.ambito || "").trim();
          const esGeneral = !ambRaw || ambRaw === "*";
          const ambito = esGeneral ? "*" : normalizarClaveDef(ambRaw) || "laura gomez";
          const nombreMostrar = esGeneral ? "General (transversal)" : ambRaw;
          const prev = perfilesSinteticos.find((p) => p.ambito === ambito);
          const version = (prev?.version ?? 0) + 1;
          const resumen = esGeneral
            ? "Criterio transversal consolidado: priorizar causa principal de aplazamiento y conservar siglas CTO/MPAL."
            : `Perfil de ${nombreMostrar}: consolida criterios propios en audiencias reprogramadas y siglas J01PMGC_VALLEDUPAR_2026_COMPLETO_SIN_ESPACIOS.`;
          const actualizado = {
            ambito,
            nombre_mostrar: nombreMostrar,
            resumen,
            version,
            ultima_actualizacion: "2026-11-20T16:30:00-05:00",
          };
          perfilesSinteticos = [
            ...perfilesSinteticos.filter((p) => p.ambito !== ambito),
            actualizado,
          ].sort((a, b) => (a.ambito === "*" ? -1 : b.ambito === "*" ? 1 : a.ambito.localeCompare(b.ambito)));
          if (esGeneral) {
            presionGlobal = 0.29;
          }
          ultimaConsolidacion = {
            id: `cons-${consolidacionesTotales}`,
            fecha: "2026-11-20T16:30:00-05:00",
            ambito,
            nombre_ambito: nombreMostrar,
            modelo: `${providerActual}:${openaiModelActual}`,
            acciones_reglas: [],
            resumen_actualizado: resumen,
            sugerencias_build: [],
            telemetria_disparo: {
              ambito,
              presion: presionGlobal,
              umbral_adaptativo: 4.5,
              probabilidad_disparo: 1,
              muestra_rng: 0,
              manual: true,
              razones: ["consolidacion_manual"],
            },
          };
          return construirDiagnostico();
        }
        case "cmd_obtener_estadisticas":
          return {
            total_audiencias: 1,
            total_si: 1,
            total_no: 0,
            efectividad_pct: 100,
            totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
            total_planillas: archivos.length,
            total_planillas_con_datos: 1,
            total_archivadas: 0,
            meses_disponibles: [
              { anio: 2026, mes: 1, etiqueta: "Enero 2026", cantidad: 1 },
            ],
            planillas: archivos.map((archivo) => ({
              archivo,
              defensor: defensoresPorArchivo[archivo] || "",
              total_audiencias: 1,
              total_si: 1,
              total_no: 0,
              efectividad_pct: 100,
              archivada: false,
            })),
            defensores_disponibles: ["Dra. Laura Gómez"],
            por_defensor: [
              {
                defensor: "Dra. Laura Gómez",
                total_audiencias: 1,
                total_si: 1,
                total_no: 0,
                efectividad_pct: 100,
                motivo_frecuente: "Sin aplazamientos",
                archivos: ["2026/Enero/audiencias_enero.xlsx"],
              },
            ],
          };
        case "cmd_leer_archivo":
          return {
            registros: [
              {
                numero: 1,
                fila_excel: 11,
                radicado: "200016001075202500123",
                tipo: "Audiencia de acusación",
                fecha: "15/01/2026",
                hora: "09:00",
                juzgado: "Juzgado 1 Penal del Circuito de Valledupar",
                realizada: "SI",
                motivo: "",
                observaciones: "Instalada sin novedad.",
              },
            ],
            totales: {
              num_registros: 1,
              total_si: 1,
              total_no: 0,
              totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
            },
            defensor: defensoresPorArchivo[args.nombre] ?? "",
          };
        case "cmd_exportar_paquete": {
          window.__LOKUS_TEST_CALLS__.exportarPaquete.push(args);
          if (args.tipoPaquete === "aprendizaje_ia") {
            return "/home/test/Descargas/lokus_ia_20261124_065000.lokus.json";
          }
          if (args.tipoPaquete === "estadisticas") {
            return "/home/test/Descargas/lokus_estadisticas_20261124_065000.lokus.json";
          }
          return "/home/test/Descargas/lokus_respaldo_20261124_065000.lokus";
        }
        case "cmd_inspeccionar_importacion": {
          let payload = null;
          try {
            const jsonText = decodeURIComponent(escape(atob(args.contenidoB64 || "")));
            payload = JSON.parse(jsonText);
          } catch {
            payload = null;
          }
          if (payload?.tipo_paquete === "aprendizaje_ia") {
            const yaSync = Boolean(opts?.iaYaSincronizada);
            return {
              tipo_paquete: "aprendizaje_ia",
              exportado: "2026-11-24T06:50:00-05:00",
              version_app: "5.3.0",
              total_carpetas: 0,
              total_planillas: 0,
              planillas_nuevas: 0,
              planillas_conflicto_id: 0,
              planillas_conflicto_ruta: 0,
              total_resumenes: 0,
              resumenes_nuevos: 0,
              resumenes_ya_en_disco: 0,
              resumenes_ya_archivados: 0,
              trae_configuracion: false,
              defensores_en_paquete: 0,
              juzgados_en_paquete: 0,
              tipos_en_paquete: 0,
              trae_aprendizaje_ia: true,
              ia_reglas_en_paquete: 5,
              ia_reglas_nuevas: yaSync ? 0 : 2,
              ia_reglas_actualizables: yaSync ? 0 : 1,
              ia_correcciones_en_paquete: 4,
              ia_correcciones_nuevas: yaSync ? 0 : 2,
              ia_alias_en_paquete: 3,
              ia_alias_nuevos: yaSync ? 0 : 1,
              ia_perfiles_en_paquete: 2,
              ia_perfiles_actualizables: yaSync ? 0 : 1,
              ia_ya_sincronizada: yaSync,
            };
          }
          return {
            tipo_paquete: "completo",
            exportado: "2026-11-24T06:50:00-05:00",
            version_app: "5.3.0",
            total_carpetas: 1,
            total_planillas: 1,
            planillas_nuevas: 1,
            planillas_conflicto_id: 0,
            planillas_conflicto_ruta: 0,
            total_resumenes: 0,
            resumenes_nuevos: 0,
            resumenes_ya_en_disco: 0,
            resumenes_ya_archivados: 0,
            trae_configuracion: true,
            defensores_en_paquete: 2,
            juzgados_en_paquete: 1,
            tipos_en_paquete: 1,
            trae_aprendizaje_ia: false,
            ia_reglas_en_paquete: 0,
            ia_reglas_nuevas: 0,
            ia_reglas_actualizables: 0,
            ia_correcciones_en_paquete: 0,
            ia_correcciones_nuevas: 0,
            ia_alias_en_paquete: 0,
            ia_alias_nuevos: 0,
            ia_perfiles_en_paquete: 0,
            ia_perfiles_actualizables: 0,
            ia_ya_sincronizada: false,
          };
        }
        case "cmd_importar_paquete": {
          window.__LOKUS_TEST_CALLS__.importarPaquete.push(args);
          const yaSync = Boolean(opts?.iaYaSincronizada);
          return {
            tipo_paquete: "aprendizaje_ia",
            carpetas_creadas: 0,
            planillas_importadas: 0,
            planillas_reemplazadas: 0,
            planillas_omitidas: 0,
            resumenes_importados: 0,
            resumenes_reemplazados: 0,
            resumenes_omitidos: 0,
            config_fusionada: false,
            ia_fusionada: !yaSync,
            ia_reglas_nuevas: yaSync ? 0 : 2,
            ia_reglas_actualizadas: yaSync ? 0 : 1,
            ia_correcciones_nuevas: yaSync ? 0 : 2,
            ia_ejemplos_nuevos: yaSync ? 0 : 1,
            ia_alias_nuevos: yaSync ? 0 : 1,
            ia_perfiles_actualizados: yaSync ? 0 : 1,
            ia_consolidaciones_nuevas: 0,
          };
        }
        default:
          return null;
      }
    },
    transformCallback: () => 0,
  };
}

test.describe("Lokus UI smoke checks", () => {
  test("menú contextual en carpetas, plegado activo, selector de defensores y temas", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit);
    await page.goto("/");

    const carpeta2026Row = page.locator('[data-context-folder="2026"]');
    await expect(carpeta2026Row).toBeVisible();

    // Sin barra flotante de botones en filas de carpeta
    await expect(carpeta2026Row.locator(".file-row-actions")).toHaveCount(0);

    // Desactiva el menú contextual nativo dentro de .sidebar-files
    const defaultPrevented = await page.evaluate(() => {
      const target = document.querySelector(".sidebar-files");
      const ev = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: 40,
        clientY: 200,
      });
      target.dispatchEvent(ev);
      return ev.defaultPrevented;
    });
    expect(defaultPrevented).toBe(true);

    // Clic derecho estilo Linux/WebKitGTK (pointerdown -> contextmenu -> pointerup)
    // no debe cerrarse al soltar el botón derecho
    const box = await carpeta2026Row.boundingBox();
    await page.mouse.move(box.x + 24, box.y + box.height / 2);
    await page.mouse.down({ button: "right" });
    await page.evaluate(
      ({ x, y }) => {
        const btn = document.querySelector('[data-context-folder="2026"] .sidebar-folder-btn');
        btn.dispatchEvent(
          new MouseEvent("contextmenu", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: x,
            clientY: y,
          })
        );
      },
      { x: box.x + 24, y: box.y + box.height / 2 }
    );
    await page.mouse.up({ button: "right" });

    const ctxMenu = page.locator(".folder-context-menu");
    await expect(ctxMenu).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Nueva planilla aquí" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Nueva subcarpeta" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Cambiar color" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Renombrar o mover" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Eliminar carpeta" })).toBeVisible();

    // Escape cierra el menú contextual
    await page.keyboard.press("Escape");
    await expect(ctxMenu).toBeHidden();

    // Abrir "Nueva planilla aquí" desde el menú contextual y validar lista de defensores
    await carpeta2026Row.click({ button: "right" });
    await ctxMenu.getByRole("button", { name: "Nueva planilla aquí" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();
    await page.getByRole("button", { name: "Mostrar opciones guardadas" }).click();
    await expect(page.getByRole("option", { name: "Dra. Laura Gómez" })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Cancelar" }).click();

    // Archivo Excel tampoco muestra barra flotante; abre menú contextual con clic derecho
    const archivoRow = page.locator('[data-context-file="2026/Enero/audiencias_enero.xlsx"]');
    await expect(archivoRow).toBeVisible();
    await expect(archivoRow.locator(".file-row-actions")).toHaveCount(0);
    await archivoRow.click({ button: "right" });
    await expect(ctxMenu).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Editar o renombrar planilla" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Mover a otra carpeta" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Asignar o editar defensor" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Historial de respaldos" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Exportar copia a Descargas" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Eliminar planilla" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(ctxMenu).toBeHidden();

    // Plegar una carpeta aunque contenga la planilla activa
    const carpetaBtn = carpeta2026Row.locator(".sidebar-folder-btn");
    await expect(carpetaBtn).toHaveAttribute("aria-expanded", "true");
    await carpetaBtn.click();
    await expect(carpetaBtn).toHaveAttribute("aria-expanded", "false");
    await expect(carpeta2026Row.locator(".folder-active-dot")).toBeVisible();

    // Colapsar y expandir barra lateral en escritorio con botón visible y Ctrl+B
    const toggleSidebarBtn = page.locator(".sidebar-toggle-btn");
    await expect(toggleSidebarBtn).toHaveAttribute("aria-expanded", "true");
    await toggleSidebarBtn.click();
    await expect(page.locator(".desktop-frame")).toHaveClass(/sidebar-collapsed/);
    await expect(toggleSidebarBtn).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("Control+b");
    await expect(page.locator(".desktop-frame")).not.toHaveClass(/sidebar-collapsed/);

    // Acceso directo al defensor desde .active-doc-bar
    await page.locator(".active-doc-defensor-btn").click();
    await expect(page.getByRole("heading", { name: "Defensor Público de la planilla" })).toBeVisible();
    await page.keyboard.press("Escape");

    // Validación 12h, AM/PM, flechas ←/→ y presets en Hora
    const horaInput = page.locator("#hora");
    const minutoInput = page.locator("#minuto");
    await horaInput.fill("15");
    await expect(page.locator("#hora-error-msg")).toHaveCount(0);
    await minutoInput.blur();
    await page.locator("#radicado").focus();
    await expect(page.locator("#hora-error-msg")).toContainText("01 y 12");
    await horaInput.fill("02");
    await minutoInput.fill("30");
    await expect(page.locator("#hora-error-msg")).toHaveCount(0);

    // Navegación con flechas ←/→ entre HH, MM y AM/PM
    await horaInput.focus();
    await page.evaluate(() => {
      const h = document.getElementById("hora");
      h.setSelectionRange(h.value.length, h.value.length);
    });
    await page.keyboard.press("ArrowRight");
    await expect(minutoInput).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    await expect(horaInput).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    const amBtn = page.locator(".time-meridiem-btn", { hasText: "AM" });
    const pmBtn = page.locator(".time-meridiem-btn", { hasText: "PM" });
    await expect(amBtn).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(pmBtn).toBeFocused();
    await pmBtn.click();
    await expect(pmBtn).toHaveAttribute("aria-pressed", "true");
    // 6 accesos rápidos de hora en fila única debajo de HH:MM AM/PM
    const presetBtns = page.locator(".time-presets .preset-btn");
    await expect(presetBtns).toHaveCount(6);
    await expect(presetBtns).toHaveText([
      "7:30 AM",
      "8:00 AM",
      "9:00 AM",
      "10:00 AM",
      "2:00 PM",
      "3:00 PM",
    ]);
    const presetLayout = await page.evaluate(() => {
      const container = document.querySelector(".time-presets");
      const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
      const tops = btns.map((b) => Math.round(b.getBoundingClientRect().top));
      const anyClipped = btns.some((b) => b.scrollWidth > b.clientWidth + 1);
      const minEl = document.getElementById("minuto");
      const merEl = document.querySelector(".time-meridiem-group");
      const meridiemGap =
        merEl.getBoundingClientRect().left - minEl.getBoundingClientRect().right;
      return {
        rowSpread: Math.max(...tops) - Math.min(...tops),
        containerOverflow: container.scrollWidth > container.clientWidth + 1,
        anyClipped,
        meridiemGap,
      };
    });
    expect(presetLayout.rowSpread).toBeLessThan(2);
    expect(presetLayout.containerOverflow).toBe(false);
    expect(presetLayout.anyClipped).toBe(false);
    expect(presetLayout.meridiemGap).toBeGreaterThanOrEqual(0);
    expect(presetLayout.meridiemGap).toBeLessThanOrEqual(16);

    await page.locator(".time-presets .preset-btn", { hasText: "9:00 AM" }).click();
    await expect(horaInput).toHaveValue("09");
    await expect(minutoInput).toHaveValue("00");
    await expect(amBtn).toHaveAttribute("aria-pressed", "true");

    await page.locator(".time-presets .preset-btn", { hasText: "3:00 PM" }).click();
    await expect(horaInput).toHaveValue("03");
    await expect(minutoInput).toHaveValue("00");
    await expect(pmBtn).toHaveAttribute("aria-pressed", "true");

    await page.locator(".time-presets .preset-btn", { hasText: "8:00 AM" }).click();
    await expect(horaInput).toHaveValue("08");
    await expect(minutoInput).toHaveValue("00");
    await expect(amBtn).toHaveAttribute("aria-pressed", "true");

    // Botón compartir unificado (badge de novedades y limpieza tras exportar)
    const shareBtn = page.getByRole("button", {
      name: "Compartir uso de catálogos y aprendizaje IA",
    });
    await expect(shareBtn).toHaveAttribute("title", /5 audiencias/);
    await expect(shareBtn.locator(".share-badge")).toHaveText("3");
    await shareBtn.click();
    await expect(shareBtn.locator(".share-badge")).toHaveCount(0);
    await expect(page.locator(".toast")).toContainText("Uso de catálogos y aprendizaje exportados");

    // Navegación por rueda del ratón y rejillas 4x3 de mes y año en DatePickerInput
    const fechaInput = page.locator("#fecha");
    await fechaInput.fill("18/01/2026");
    const calPopover = page.locator(".calendar-popover");
    await expect(calPopover).toBeVisible();
    const mesBtn = calPopover.locator(".calendar-select-trigger").first();
    const anioBtn = calPopover.locator(".calendar-select-trigger.is-year");
    await expect(mesBtn).toContainText("Enero");
    await expect(anioBtn).toContainText("2026");
    await mesBtn.hover();
    await page.mouse.wheel(0, 100);
    await expect(mesBtn).toContainText("Febrero");
    await mesBtn.hover();
    await page.mouse.wheel(0, -100);
    await expect(mesBtn).toContainText("Enero");
    await anioBtn.hover();
    await page.mouse.wheel(0, 100);
    await expect(anioBtn).toContainText("2027");

    // Rejilla 4x3 de meses
    await mesBtn.click();
    const monthsMatrix = calPopover.locator(".calendar-months-matrix");
    await expect(monthsMatrix).toBeVisible();
    await expect(monthsMatrix.locator(".calendar-cell-btn")).toHaveCount(12);
    await monthsMatrix.getByRole("button", { name: "Marzo" }).click();
    await expect(mesBtn).toContainText("Marzo");
    await expect(calPopover.locator(".calendar-grid")).toBeVisible();

    // Rejilla 4x3 paginada de años (2018-2050)
    await anioBtn.click();
    const yearsMatrix = calPopover.locator(".calendar-years-matrix");
    await expect(yearsMatrix).toBeVisible();
    await expect(calPopover.locator(".calendar-subview-label")).toContainText("2018 – 2029");
    await calPopover.getByRole("button", { name: "Años siguientes" }).click();
    await expect(calPopover.locator(".calendar-subview-label")).toContainText("2030 – 2041");
    await yearsMatrix.getByRole("button", { name: "2030" }).click();
    await expect(anioBtn).toContainText("2030");
    await expect(calPopover.locator(".calendar-grid")).toBeVisible();

    await fechaInput.focus();
    await page.keyboard.press("Escape");
    await expect(calPopover).toBeHidden();

    // Verificar encabezado <h1> en vista Registros
    await page.getByRole("tab", { name: /Registros/ }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Registros de la Planilla" })).toBeVisible();

    // Verificar modal IA y pestaña Aprendizaje y memoria
    await page.getByRole("button", { name: /Configurar IA/i }).click();
    await expect(
      page.getByRole("heading", { level: 2, name: "Inteligencia Artificial y Aprendizaje" })
    ).toBeVisible();
    await expect(page.locator(".ia-status-banner")).toContainText("Sin clave API");
    await page.getByRole("tab", { name: /Aprendizaje y memoria/i }).click();
    await expect(page.getByRole("heading", { level: 3, name: "Precisión por campo" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "Criterios aprendidos" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "Modelos evaluados" })).toBeVisible();
    await expect(page.locator(".ia-kpi-strip")).toContainText("86%");
    await expect(page.locator(".ia-kpi-strip")).toContainText("Perfiles de defensores");
    await expect(page.locator(".ia-kpi-strip")).toContainText("14 conexiones de juzgados");
    await expect(page.locator(".ia-prefs-list")).toContainText("instalada y reprogramada");
    await expect(page.locator(".ia-prefs-list")).toContainText("40%");
    const tablaCampos = page.locator(".ia-diag-table");
    await expect(tablaCampos).toContainText("Realizada (SI/NO)");
    await expect(tablaCampos.locator(".ia-diag-pill.is-ok")).toContainText("Confiable");
    await expect(tablaCampos.locator(".ia-diag-pill.is-warn")).toContainText("Mejorable");
    await expect(tablaCampos).toContainText("Hora");
    await expect(tablaCampos.locator(".ia-diag-pill.is-neutral")).toContainText("Sin datos");
    await expect(tablaCampos).toContainText("—");
    await expect(page.locator(".ia-diag-rules-list")).toContainText("CTO");
    await expect(page.getByRole("button", { name: "Consolidar perfil del defensor" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Consolidar reglas generales" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Recargar datos" })).toBeEnabled();
    await page.getByRole("button", { name: "Cancelar" }).click();

    // Cambio de tema y verificación de no desbordamiento horizontal en tema claro
    await toggleSidebarBtn.click();
    await page.keyboard.press("Control+b");
    await page.getByRole("button", { name: "Cambiar tema" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.getByRole("tab", { name: /Nueva Audiencia/ }).click();
    const lightCheck = await page.evaluate(() => {
      const docOverflow =
        document.documentElement.scrollWidth > document.documentElement.clientWidth;
      const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
      const tops = btns.map((b) => Math.round(b.getBoundingClientRect().top));
      return {
        docOverflow,
        rowSpread: Math.max(...tops) - Math.min(...tops),
      };
    });
    expect(lightCheck.docOverflow).toBe(false);
    expect(lightCheck.rowSpread).toBeLessThan(2);
    expect(errors).toEqual([]);
  });

  test("autocompletado IA empata sin tildes ni mayúsculas con el catálogo y resalta 3:00 PM", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, { configured: true });
    await page.goto("/");

    const iaTextarea = page.locator("#ia-inline-textarea");
    await expect(iaTextarea).toBeVisible();
    await iaTextarea.fill(
      "Audiencia de imputacion ante juzgado 1 promiscuo municipal de agustin codazzi a las 3 pm"
    );
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();

    // Verifica que el autocompletado con IA recibe el defensor activo de la planilla
    const iaCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.autocompletarIa);
    expect(iaCalls.length).toBe(1);
    expect(iaCalls[0].defensor).toBe("Dra. Laura Gómez");

    await expect(page.locator("#tipo-audiencia")).toContainText("Audiencia de imputación");
    await expect(page.locator("#tipo-otra")).toHaveCount(0);
    await expect(page.locator("#juzgado")).toContainText(
      "Juzgado 1 Promiscuo Municipal de Agustín Codazzi"
    );
    await expect(page.locator("#juzgado-otra")).toHaveCount(0);
    await expect(
      page.locator(".time-presets .preset-btn", { hasText: "3:00 PM" })
    ).toHaveAttribute("aria-pressed", "true");

    // Verificar diseño en ventana estrecha (3x2 en accesos rápidos sin desbordamiento ni recortes en todos los anchos)
    for (const width of [1280, 1024, 980, 960, 720, 670, 580, 480]) {
      await page.setViewportSize({ width, height: 820 });
      const layout = await page.evaluate(() => {
        const docOverflow =
          document.documentElement.scrollWidth > document.documentElement.clientWidth;
        const container = document.querySelector(".time-presets");
        const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
        const anyClipped = btns.some((b) => b.scrollWidth > b.clientWidth + 1);
        const containerOverflow = container.scrollWidth > container.clientWidth + 1;
        return { docOverflow, anyClipped, containerOverflow };
      });
      expect(layout.docOverflow, `docOverflow at ${width}px`).toBe(false);
      expect(layout.anyClipped, `anyClipped at ${width}px`).toBe(false);
      expect(layout.containerOverflow, `containerOverflow at ${width}px`).toBe(false);
    }

    // Probar alta unificada desde "Otra" + botón + (sin pie "Agregar nuevo") y variante sin tilde
    await page.setViewportSize({ width: 1280, height: 820 });
    await page.locator("#tipo-audiencia").click();
    await expect(page.locator(".custom-select-add-trigger")).toHaveCount(0);
    await page.getByRole("option", { name: /Otra/i }).click();
    await page.locator("#tipo-otra").fill("Audiencia de imputacion");
    await page.getByRole("button", { name: "Guardar tipo de audiencia en la lista" }).click();
    await expect(page.locator("#tipo-audiencia")).toContainText("Audiencia de imputación");
    await expect(page.locator(".toast")).toContainText("ya existe en los tipos de audiencia");

    await page.locator("#juzgado").click();
    await expect(page.locator(".custom-select-add-trigger")).toHaveCount(0);
    await page.getByRole("option", { name: /Otra/i }).click();
    await page
      .locator("#juzgado-otra")
      .fill("Tribunal Superior del Distrito Judicial de Valledupar Sala Penal");
    await page.getByRole("button", { name: "Guardar juzgado en la lista" }).click();
    await expect(page.locator("#juzgado")).toContainText(
      "Tribunal Superior del Distrito Judicial de Valledupar - Sala Penal"
    );
    await expect(page.locator(".toast")).toContainText("ya existe en la lista de juzgados");

    // Agregar juzgado nuevo real desde "Otra" y luego eliminarlo de la lista
    await page.locator("#juzgado").click();
    await page.getByRole("option", { name: /Otra/i }).click();
    await page.locator("#juzgado-otra").fill("Juzgado 2 Penal");
    await page.getByRole("button", { name: "Guardar juzgado en la lista" }).click();
    await expect(page.locator("#juzgado")).toContainText("Juzgado 2 Penal");
    await expect(page.locator(".toast")).toContainText("agregado a la lista de juzgados");

    await page.locator("#juzgado").click();
    await page.getByRole("button", { name: "Eliminar Juzgado 2 Penal" }).click();
    await expect(page.locator("#juzgado")).toContainText("Sin especificar");

    // El desplegable de juzgados sigue abierto tras eliminar: seleccionar un juzgado y guardar para
    // verificar que cmd_registrar_resultado_ia recibe el defensor activo de la planilla
    await page
      .getByRole("option", { name: "Juzgado 1 Promiscuo Municipal de Agustín Codazzi" })
      .click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();
    await expect
      .poll(async () =>
        page.evaluate(() => window.__LOKUS_TEST_CALLS__.registrarResultadoIa.length)
      )
      .toBe(1);
    const regCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.registrarResultadoIa);
    expect(regCalls[0].defensor).toBe("Dra. Laura Gómez");

    expect(errors).toEqual([]);
  });

  test("defensor obligatorio al crear planilla y aviso discreto en planilla antigua sin defensor", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, { sinDefensor: true, configured: true });
    await page.goto("/");

    // 1) Planilla antigua sin defensor muestra aviso discreto en .active-doc-bar en ambos temas sin desbordar
    const emptyDefBtn = page.locator(".active-doc-defensor-btn.is-empty");
    await expect(emptyDefBtn).toBeVisible();
    await expect(emptyDefBtn).toContainText("Sin defensor (perfil general)");

    // Autocompletar con IA en planilla antigua sin defensor envía defensor: null (usa Cuaderno General)
    await page.locator("#ia-inline-textarea").fill("Audiencia preliminar en J04");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    const iaLegacy = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.autocompletarIa);
    expect(iaLegacy.length).toBe(1);
    expect(iaLegacy[0].defensor).toBeNull();

    for (const theme of ["dark", "light"]) {
      if (theme === "light") {
        await page.getByRole("button", { name: "Cambiar tema" }).click();
        await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
      }
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth
      );
      expect(overflow, `overflow en tema ${theme}`).toBe(false);
    }

    // 2) Clic en el aviso abre el modal y permite asignar un defensor en 1 clic, retirando el aviso
    await emptyDefBtn.click();
    await expect(page.getByRole("heading", { name: "Defensor Público de la planilla" })).toBeVisible();
    await page
      .locator(".defensor-row", { hasText: "Dr. Carlos Mendoza" })
      .getByRole("button", { name: "Usar en planilla" })
      .click();
    await page.keyboard.press("Escape");
    await expect(page.locator(".active-doc-defensor-btn")).toContainText("Dr. Carlos Mendoza");
    await expect(page.locator(".active-doc-defensor-btn.is-empty")).toHaveCount(0);

    // 3) Modal "Nuevo archivo de audiencias" exige defensor obligatorio y no ofrece opción vacía
    const carpeta2026Row = page.locator('[data-context-folder="2026"]');
    await carpeta2026Row.click({ button: "right" });
    await page.locator(".folder-context-menu").getByRole("button", { name: "Nueva planilla aquí" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();

    await page.getByRole("button", { name: "Mostrar opciones guardadas" }).click();
    await expect(page.getByRole("option", { name: "Dra. Laura Gómez" })).toBeVisible();
    await expect(page.getByRole("option", { name: /Dejar sin defensor/i })).toHaveCount(0);
    await page.keyboard.press("Escape");

    // Intentar crear solo con nombre y sin defensor -> bloquea y muestra alerta
    await page.locator("#nuevo-archivo-input").fill("audiencias_febrero");
    await page.getByRole("button", { name: "Crear archivo" }).click();
    await expect(page.locator("#nuevo-archivo-defensor-error")).toContainText(
      "Debes asignar un Defensor Público"
    );
    const llamadasAntes = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.crearArchivo.length);
    expect(llamadasAntes).toBe(0);

    // Completar el defensor y crear -> invoca cmd_crear_archivo y muestra el defensor en la barra activa
    await page.locator("#nuevo-archivo-defensor").fill("Dra. Laura Gómez");
    await page.getByRole("button", { name: "Crear archivo" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeHidden();
    const llamadasDespues = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.crearArchivo);
    expect(llamadasDespues.length).toBe(1);
    expect(llamadasDespues[0].nombre).toBe("audiencias_febrero");
    expect(llamadasDespues[0].defensor).toBe("Dra. Laura Gómez");
    await expect(page.locator(".active-doc-defensor-btn")).toContainText("Dra. Laura Gómez");
    await expect(page.locator(".active-doc-defensor-btn.is-empty")).toHaveCount(0);

    expect(errors).toEqual([]);
  });

  test("cuadro adaptativo de aclaración, recomendación de defensores y consolidación en config_ia", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, {
      configured: true,
      extraArchivos: ["2026/Enero/audiencias_febrero.xlsx"],
      extraDefensores: ["Dr. Mario Núñez"],
    });
    await page.goto("/");

    // 1) Recomendación sin títulos (Dr./Dra.) y edición/borrado inline en desplegable de defensores
    await page.locator('[data-context-folder="2026"]').click({ button: "right" });
    await page.locator(".folder-context-menu").getByRole("button", { name: "Nueva planilla aquí" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();
    await expect(
      page.getByText(/Recomendación: escribe el nombre completo sin títulos \(Dr\.\/Dra\.\)/i)
    ).toBeVisible();

    // Desplegable de #nuevo-archivo-defensor: editar inline y eliminar defensor guardado sin cerrar el modal
    await page.getByRole("button", { name: "Mostrar opciones guardadas" }).click();
    await page.getByRole("button", { name: "Editar Dr. Mario Núñez" }).click();
    const inlineDefInput = page.getByLabel("Nuevo nombre para Dr. Mario Núñez");
    await expect(inlineDefInput).toBeVisible();
    await inlineDefInput.fill("Mario Núñez Díaz");
    await page.locator(".autocomplete-edit-row").getByRole("button", { name: "Guardar" }).click();
    await expect(page.getByRole("option", { name: "Mario Núñez Díaz" })).toBeVisible();
    await expect(page.locator(".toast")).toContainText("Defensor renombrado a 'Mario Núñez Díaz'");
    const renCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.renombrarDefensor);
    expect(renCalls.length).toBe(1);
    expect(renCalls[0].actual).toBe("Dr. Mario Núñez");
    expect(renCalls[0].nuevo).toBe("Mario Núñez Díaz");

    // Verificar Escape en edición inline no cierra el modal y eliminar defensor lo retira de la lista
    await page.getByRole("button", { name: "Editar Mario Núñez Díaz" }).click();
    await expect(page.getByLabel("Nuevo nombre para Mario Núñez Díaz")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();
    await page.getByRole("button", { name: "Eliminar Mario Núñez Díaz" }).click();
    await expect(page.getByRole("option", { name: "Mario Núñez Díaz" })).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();
    await expect(page.locator(".toast")).toContainText("eliminado de los defensores guardados");
    const delCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.eliminarDefensor);
    expect(delCalls.length).toBe(1);
    expect(delCalls[0].nombre).toBe("Mario Núñez Díaz");

    await page.getByRole("button", { name: "Cancelar" }).click();

    await page.locator(".active-doc-defensor-btn").click();
    await expect(page.getByRole("heading", { name: "Defensor Público de la planilla" })).toBeVisible();
    await expect(
      page.getByText(/Recomendación: escribe el nombre completo sin títulos \(Dr\.\/Dra\.\)/i)
    ).toBeVisible();
    await page.keyboard.press("Escape");

    // 2) Autocompletar con IA -> corregir campo -> aparece .ia-clarify-panel con pills IA -> Usuario
    await page.locator("#ia-inline-textarea").fill(
      "Audiencia de imputacion ante juzgado 1 promiscuo municipal de agustin codazzi a las 3 pm"
    );
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    await page.getByRole("radio", { name: /No realizada \/ Aplazada \(NO\)/i }).click();
    await page.getByRole("radio", { name: /Fiscalía/i }).click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();

    const clarifyPanel = page.locator(".ia-clarify-panel");
    await expect(clarifyPanel).toBeVisible();
    await expect(clarifyPanel.locator("svg.ia-clarify-icon")).toBeVisible();
    await expect(clarifyPanel.locator(".ia-clarify-diff-strip")).toContainText("Realizada:");
    await expect(clarifyPanel.locator(".ia-clarify-diff-strip")).toContainText("SI → NO");
    await expect(clarifyPanel.locator(".ia-clarify-diff-strip")).toContainText("Motivo:");
    await expect(clarifyPanel.locator(".ia-clarify-diff-strip")).toContainText("vacío → Fiscalía");

    // Verificar cascada CSS: pills y chips deben aplicar estilos cálidos/elevados y no quedar sobrescritos por .ia-diag-pill / .preset-btn
    const clarifyStyles = await page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const pill = document.querySelector(".ia-clarify-diff-pill");
      const chip = document.querySelector(".ia-clarify-chip");
      const bgProbe = document.createElement("div");
      bgProbe.style.backgroundColor = root.getPropertyValue("--bg-elevated").trim();
      document.body.appendChild(bgProbe);
      const expectedElevated = getComputedStyle(bgProbe).backgroundColor;
      bgProbe.remove();
      return {
        pillBg: getComputedStyle(pill).backgroundColor,
        chipBg: getComputedStyle(chip).backgroundColor,
        chipFontSize: getComputedStyle(chip).fontSize,
        expectedElevated,
      };
    });
    expect(clarifyStyles.pillBg).toBe(clarifyStyles.expectedElevated);
    expect(clarifyStyles.chipBg).toBe(clarifyStyles.expectedElevated);
    expect(clarifyStyles.chipFontSize).toBe("11.5px");

    // Verificar forwarding de prediccionLocal
    const regCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.registrarResultadoIa);
    expect(regCalls.length).toBe(1);
    expect(regCalls[0].prediccionLocal?.realizada).toBe("SI");

    // Verificar tema oscuro y claro sin desbordamiento horizontal con .ia-clarify-panel visible
    for (const theme of ["dark", "light"]) {
      if (theme === "light") {
        await page.getByRole("button", { name: "Cambiar tema" }).click();
        await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
      }
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth
      );
      expect(overflow, `overflow con clarify panel en tema ${theme}`).toBe(false);
    }
    await page.getByRole("button", { name: "Cambiar tema" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    // Chips rápidos prellenan y "Dato externo" descarta con esDatoExterno: true
    await clarifyPanel.getByRole("button", { name: "Criterio de este defensor" }).click();
    const clarifyInput = clarifyPanel.locator("input[type='text']");
    await expect(clarifyInput).toHaveValue("Criterio habitual de este defensor: ");
    await expect(clarifyInput).toBeFocused();

    await clarifyPanel.getByRole("button", { name: "Sigla / abreviación habitual" }).click();
    await expect(clarifyInput).toHaveValue("Abreviación habitual: ");
    await expect(clarifyInput).toBeFocused();

    await clarifyPanel.getByRole("button", { name: "Dato externo (no estaba en el texto)" }).click();
    await expect(clarifyPanel).toBeHidden();
    await expect(page.locator(".toast")).toContainText("Marcado como dato externo — no se creará regla");
    const respCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.responderAclaracionIa);
    expect(respCalls.length).toBe(1);
    expect(respCalls[0].esDatoExterno).toBe(true);

    // 3) Reaparece con nuevo correccion_id, resetea input, se oculta al cambiar de planilla y no resucita al refrescar resumen
    await page.locator("#ia-inline-textarea").fill("Audiencia de imputacion J1 Valledupar 3 pm");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    await page.getByRole("radio", { name: /No realizada \/ Aplazada \(NO\)/i }).click();
    await page.getByRole("radio", { name: /Juez/i }).click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();

    await expect(clarifyPanel).toBeVisible();
    await expect(clarifyInput).toHaveValue("");
    await page.locator('[data-context-file="2026/Enero/audiencias_febrero.xlsx"]').click();
    await expect(clarifyPanel).toBeHidden();

    // Agregar juzgado dispara cargarResumenAprendizaje(): el panel oculto no debe reaparecer
    await page.locator("#juzgado").click();
    await page.getByRole("option", { name: /Otra/i }).click();
    await page.locator("#juzgado-otra").fill("Juzgado Prueba Zombie");
    await page.getByRole("button", { name: "Guardar juzgado en la lista" }).click();
    await expect(clarifyPanel).toBeHidden();

    // 4) También se oculta si cambia el defensor de la planilla activa
    await page.locator("#ia-inline-textarea").fill("Audiencia de imputacion J2 Valledupar 3 pm");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    await page.getByRole("radio", { name: /No realizada \/ Aplazada \(NO\)/i }).click();
    await page.getByRole("radio", { name: /Usuario/i }).click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();
    await expect(clarifyPanel).toBeVisible();

    await page.locator(".active-doc-defensor-btn").click();
    await page
      .locator(".defensor-row", { hasText: "Dr. Carlos Mendoza" })
      .getByRole("button", { name: "Usar en planilla" })
      .click();
    await page.keyboard.press("Escape");
    await expect(clarifyPanel).toBeHidden();

    // 4b) Si falla cmd_responder_aclaracion_ia conserva el texto; luego "Guardar regla" envía esDatoExterno: false
    await page.locator("#ia-inline-textarea").fill("Audiencia de imputacion J3 Valledupar 3 pm");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    await page.getByRole("radio", { name: /No realizada \/ Aplazada \(NO\)/i }).click();
    await page.getByRole("radio", { name: /INPEC/i }).click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();
    await expect(clarifyPanel).toBeVisible();
    await clarifyPanel.getByRole("button", { name: "Criterio de este defensor" }).click();
    await clarifyInput.fill("Criterio habitual de este defensor: INPEC no trasladó");
    await page.evaluate(() => {
      window.__LOKUS_FAIL_NEXT_RESPONDER__ = true;
    });
    await clarifyPanel.getByRole("button", { name: "Guardar regla" }).click();
    await expect(clarifyPanel).toBeVisible();
    await expect(clarifyInput).toHaveValue("Criterio habitual de este defensor: INPEC no trasladó");
    await expect(page.locator(".toast")).toContainText("No se pudo guardar la aclaración");

    await clarifyPanel.getByRole("button", { name: "Guardar regla" }).click();
    await expect(clarifyPanel).toBeHidden();
    await expect(page.locator(".toast")).toContainText("Aclaración guardada en el aprendizaje de la IA");
    const respCallsManual = await page.evaluate(
      () => window.__LOKUS_TEST_CALLS__.responderAclaracionIa
    );
    expect(respCallsManual.length).toBe(2);
    expect(respCallsManual[1].esDatoExterno).toBe(false);

    // 4c) Enter en campo vacío no descarta; clic en "Omitir" invoca cmd_descartar_aclaracion_ia
    await page.locator("#ia-inline-textarea").fill("Audiencia de imputacion J4 Valledupar 3 pm");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();
    await page.getByRole("radio", { name: /No realizada \/ Aplazada \(NO\)/i }).click();
    await page.getByRole("radio", { name: /ICBF/i }).click();
    await page.getByRole("button", { name: "Registrar audiencia" }).click();
    await expect(clarifyPanel).toBeVisible();
    await clarifyInput.focus();
    await page.keyboard.press("Enter");
    await expect(clarifyPanel).toBeVisible();
    await clarifyPanel.getByRole("button", { name: "Omitir" }).click();
    await expect(clarifyPanel).toBeHidden();
    const discardCalls = await page.evaluate(
      () => window.__LOKUS_TEST_CALLS__.descartarAclaracionIa
    );
    expect(discardCalls.length).toBe(1);

    // 5) En config_ia: 2 interruptores claros (con rollback al cancelar), toast en top-layer, KPIs, consolidación y perfiles resumidos
    await page.locator(".ai-config-trigger").click();
    await expect(page.locator(".ia-status-banner")).toContainText("Conectado");
    const chkConsolidacion = page.getByRole("checkbox", {
      name: /Organizar memoria automáticamente/i,
    });
    const chkResumen = page.getByRole("checkbox", {
      name: /Usar resúmenes aprendidos al autocompletar/i,
    });
    await expect(chkConsolidacion).toBeVisible();
    await expect(chkConsolidacion).toBeChecked();
    await expect(chkResumen).toBeVisible();
    await expect(chkResumen).toBeChecked();

    // Cancelar sin guardar no debe conservar cambios en el estado del formulario
    await chkConsolidacion.uncheck();
    await page.getByRole("button", { name: "Cancelar" }).click();
    await page.locator(".ai-config-trigger").click();
    await expect(chkConsolidacion).toBeChecked();

    await chkResumen.uncheck();
    await page.getByRole("button", { name: "Guardar cambios" }).click();
    const savedCfg = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.guardarConfig);
    expect(savedCfg.length).toBe(1);
    expect(savedCfg[0].config.consolidacion_ia_activa).toBe(true);
    expect(savedCfg[0].config.usar_resumen_consolidado).toBe(false);

    await page.locator(".ai-config-trigger").click();
    await expect(chkResumen).not.toBeChecked();

    await page.getByRole("tab", { name: /Aprendizaje y memoria/i }).click();
    const kpiStrip = page.locator(".ia-kpi-strip");
    await expect(kpiStrip).toContainText("Resúmenes creados");
    await expect(kpiStrip).toContainText("Listo para resumir");

    await page.locator("summary.ia-tech-summary").click();
    const metricsRow = page.locator(".ia-diag-metrics-row");
    await expect(metricsRow).toContainText("Presión global");
    await expect(metricsRow).toContainText("1.45");
    await expect(metricsRow).toContainText("Resp. a preguntas");

    const btnConsDef = page.getByRole("button", { name: "Consolidar perfil del defensor" });
    const btnConsGen = page.getByRole("button", { name: "Consolidar reglas generales" });
    await expect(btnConsDef).toBeEnabled();
    await expect(btnConsGen).toBeEnabled();

    await btnConsGen.click();
    const panelAprendizaje = page.locator("#ia-panel-aprendizaje");
    await expect(
      page.getByRole("heading", { level: 3, name: "Perfiles resumidos" })
    ).toBeVisible();
    await expect(panelAprendizaje).toContainText("General (todos los defensores)");
    await expect(panelAprendizaje).toContainText("Criterio transversal consolidado");
    await expect(metricsRow).toContainText("0.29");
    await expect(kpiStrip).toContainText("Memoria al día");

    // El toast debe renderizarse por encima de <dialog> abierto (Top Layer)
    const toastLocator = page.locator(".toast");
    await expect(toastLocator).toBeVisible();
    await expect(toastLocator).toContainText("Memoria general de IA consolidada");
    const toastOnTop = await page.evaluate(() => {
      const el = document.querySelector(".toast");
      if (!el) return false;
      const r = el.getBoundingClientRect();
      const topEl = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return Boolean(topEl && el.contains(topEl));
    });
    expect(toastOnTop).toBe(true);

    await btnConsDef.click();
    await expect(panelAprendizaje).toContainText("Dr. Carlos Mendoza");

    const consCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.consolidarMemoriaIa);
    expect(consCalls.length).toBe(2);
    expect(consCalls[0].ambito).toBe("*");
    expect(consCalls[1].ambito).toBe("Dr. Carlos Mendoza");

    // Verificar cero desbordamiento en diálogo config_ia en tema oscuro y claro (incluyendo token largo sin espacios)
    for (const theme of ["dark", "light"]) {
      if (theme === "light") {
        await page.getByRole("button", { name: "Cancelar" }).click();
        await page.getByRole("button", { name: "Cambiar tema" }).click();
        await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
        await page.locator(".ai-config-trigger").click();
        await page.getByRole("tab", { name: /Aprendizaje y memoria/i }).click();
        await page.locator("summary.ia-tech-summary").click();
      }
      const dlgOverflow = await page.evaluate(() => {
        const docOver = document.documentElement.scrollWidth > document.documentElement.clientWidth;
        const dlgBody = document.querySelector("dialog[open] .modal-body");
        const bodyOver = dlgBody ? dlgBody.scrollWidth > dlgBody.clientWidth + 1 : false;
        return { docOver, bodyOver };
      });
      expect(dlgOverflow.docOver, `docOverflow en config_ia (${theme})`).toBe(false);
      expect(dlgOverflow.bodyOver, `modal body overflow en config_ia (${theme})`).toBe(false);
    }

    expect(errors).toEqual([]);
  });

  test("drag & drop en árbol de planillas (carpeta expandida, raíz y reordenamiento)", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, {
      configured: true,
      extraCarpetas: ["2026/Febrero"],
      extraArchivos: ["suelta.xlsx"],
    });
    await page.goto("/");

    const sueltaBtn = page.locator('[data-context-file="suelta.xlsx"] .sidebar-file-btn');
    const eneroHijoArchivo = page.locator(
      '[data-context-file="2026/Enero/audiencias_enero.xlsx"]'
    );
    await expect(sueltaBtn).toBeVisible();
    await expect(eneroHijoArchivo).toBeVisible();

    // Salvaguardas WebView2: draggable="false", touch-action: none, user-select: none y dragstart prevenido
    const dndGuard = await page.evaluate(() => {
      const fileBtn = document.querySelector('[data-context-file="suelta.xlsx"] .sidebar-file-btn');
      const style = getComputedStyle(fileBtn);
      const ev = new Event("dragstart", { bubbles: true, cancelable: true });
      fileBtn.dispatchEvent(ev);
      return {
        draggableAttr: fileBtn.getAttribute("draggable"),
        touchAction: style.touchAction,
        userSelect: style.userSelect || style.webkitUserSelect,
        dragStartPrevented: ev.defaultPrevented,
      };
    });
    expect(dndGuard.draggableAttr).toBe("false");
    expect(dndGuard.touchAction).toBe("none");
    expect(dndGuard.userSelect).toBe("none");
    expect(dndGuard.dragStartPrevented).toBe(true);

    // 1) Arrastrar archivo raíz directamente sobre un archivo hijo dentro de carpeta expandida "2026/Enero"
    const origBox = await sueltaBtn.boundingBox();
    const hijoBox = await eneroHijoArchivo.boundingBox();
    await page.mouse.move(origBox.x + origBox.width / 2, origBox.y + origBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      origBox.x + origBox.width / 2 + 12,
      origBox.y + origBox.height / 2,
      { steps: 4 }
    );
    await expect(page.locator(".tree-drag-ghost")).toBeVisible();
    await page.mouse.move(hijoBox.x + hijoBox.width / 2, hijoBox.y + hijoBox.height / 2, {
      steps: 8,
    });
    await expect(page.locator('[data-drop-folder="2026/Enero"].is-drop-target')).toBeVisible();
    await page.mouse.up();

    await expect
      .poll(async () => page.evaluate(() => window.__LOKUS_TEST_CALLS__.moverArchivo.length))
      .toBe(1);
    const mov1 = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.moverArchivo[0]);
    expect(mov1.nombre).toBe("suelta.xlsx");
    expect(mov1.carpetaDestino).toBe("2026/Enero");
    await expect(page.locator('[data-context-file="2026/Enero/suelta.xlsx"]')).toBeVisible();

    // 2) Arrastrar desde carpeta hacia la zona superior ".sidebar-root-dropzone" (raíz "")
    const movidaBtn = page.locator(
      '[data-context-file="2026/Enero/suelta.xlsx"] .sidebar-file-btn'
    );
    const movidaBox = await movidaBtn.boundingBox();
    await page.mouse.move(movidaBox.x + movidaBox.width / 2, movidaBox.y + movidaBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      movidaBox.x + movidaBox.width / 2 + 12,
      movidaBox.y + movidaBox.height / 2,
      { steps: 4 }
    );
    const rootZone = page.locator(".sidebar-root-dropzone");
    await expect(rootZone).toBeVisible();
    const rootBox = await rootZone.boundingBox();
    await page.mouse.move(rootBox.x + rootBox.width / 2, rootBox.y + rootBox.height / 2, {
      steps: 6,
    });
    await page.mouse.up();

    await expect
      .poll(async () => page.evaluate(() => window.__LOKUS_TEST_CALLS__.moverArchivo.length))
      .toBe(2);
    const mov2 = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.moverArchivo[1]);
    expect(mov2.nombre).toBe("2026/Enero/suelta.xlsx");
    expect(mov2.carpetaDestino).toBe("");
    await expect(page.locator('[data-context-file="suelta.xlsx"]')).toBeVisible();

    // 3) Reordenar subcarpeta "2026/Febrero" antes de "2026/Enero"
    const eneroRow = page.locator('[data-context-folder="2026/Enero"]');
    const febreroRow = page.locator('[data-context-folder="2026/Febrero"]');
    const febBox = await febreroRow.boundingBox();
    const eneBox = await eneroRow.boundingBox();
    await page.mouse.move(febBox.x + 24, febBox.y + febBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(eneBox.x + 24, eneBox.y + Math.max(2, eneBox.height * 0.14), {
      steps: 10,
    });
    await page.mouse.up();

    await expect
      .poll(async () => page.evaluate(() => window.__LOKUS_TEST_CALLS__.guardarOrdenCarpetas.length))
      .toBe(1);
    const nuevoOrden = await page.evaluate(
      () => window.__LOKUS_TEST_CALLS__.guardarOrdenCarpetas[0].orden
    );
    expect(nuevoOrden.indexOf("2026/Febrero")).toBeLessThan(nuevoOrden.indexOf("2026/Enero"));
    expect(errors).toEqual([]);
  });

  test("editar y renombrar planilla Excel y autocompletado [DEFENSOR]_[MES]_[AÑO]", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, { configured: true });
    await page.goto("/");

    // 1) Editar y renombrar planilla desde menú contextual del sidebar
    await page.locator('[data-context-file="2026/Enero/audiencias_enero.xlsx"]').click({
      button: "right",
    });
    await page
      .locator(".folder-context-menu")
      .getByRole("button", { name: "Editar o renombrar planilla" })
      .click();
    await expect(page.getByRole("heading", { name: "Editar planilla Excel" })).toBeVisible();

    await page.locator("#editar-archivo-nombre").fill("Laura_Renombrada");
    await page.locator("#editar-archivo-defensor").fill("Dr. Carlos Mendoza");
    await page.getByRole("button", { name: "Guardar cambios" }).click();

    await expect(page.getByRole("heading", { name: "Editar planilla Excel" })).toBeHidden();
    await expect(page.locator(".active-doc-name")).toHaveText("Laura_Renombrada.xlsx");
    await expect(page.locator(".active-doc-defensor-btn")).toContainText("Dr. Carlos Mendoza");
    await expect(
      page.locator('[data-context-file="2026/Enero/Laura_Renombrada.xlsx"]')
    ).toBeVisible();

    const renArchivoCalls = await page.evaluate(
      () => window.__LOKUS_TEST_CALLS__.renombrarArchivo
    );
    expect(renArchivoCalls.length).toBe(1);
    expect(renArchivoCalls[0]).toEqual({
      actual: "2026/Enero/audiencias_enero.xlsx",
      nuevoNombre: "Laura_Renombrada",
      carpetaDestino: "2026/Enero",
    });

    // 2) También puede editarse con el botón rápido en .active-doc-bar y reubicar a la raíz
    await page.locator(".active-doc-edit-btn").click();
    await expect(page.getByRole("heading", { name: "Editar planilla Excel" })).toBeVisible();
    await page.locator("#editar-archivo-carpeta").click();
    await page.getByRole("option", { name: "Carpeta principal (raíz)" }).click();
    await page.getByRole("button", { name: "Guardar cambios" }).click();
    await expect(page.locator('[data-context-file="Laura_Renombrada.xlsx"]')).toBeVisible();

    // 3) Autocompletado [NOMBRE_DEFENSOR]_[MES_ACTUAL]_[AÑO_ACTUAL] al crear planilla nueva
    const meses = [
      "Enero",
      "Febrero",
      "Marzo",
      "Abril",
      "Mayo",
      "Junio",
      "Julio",
      "Agosto",
      "Septiembre",
      "Octubre",
      "Noviembre",
      "Diciembre",
    ];
    const ahora = new Date();
    const mesEsperado = meses[ahora.getMonth()];
    const anioEsperado = ahora.getFullYear();

    await page.locator('[data-context-folder="2026"]').click({ button: "right" });
    await page
      .locator(".folder-context-menu")
      .getByRole("button", { name: "Nueva planilla aquí" })
      .click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();

    // Elegir defensor guardado rellena automáticamente [DEFENSOR]_[MES]_[AÑO]
    await page.getByRole("button", { name: "Mostrar opciones guardadas" }).click();
    await page.getByRole("option", { name: "Dra. Laura Gómez" }).click();
    const nombreInput = page.locator("#nuevo-archivo-input");
    await expect(nombreInput).toHaveValue(`Dra_Laura_Gómez_${mesEsperado}_${anioEsperado}`);

    // Si el usuario escribe manualmente un nombre propio, cambiar de defensor NO lo sobrescribe
    await nombreInput.fill("Planilla_Manual_Especial");
    await page.locator("#nuevo-archivo-defensor").fill("Dr. Carlos Mendoza");
    await expect(nombreInput).toHaveValue("Planilla_Manual_Especial");

    // Botón "Usar formato automático" vuelve a aplicar [DEFENSOR]_[MES]_[AÑO]
    await page.getByRole("button", { name: "Usar formato automático" }).click();
    const esperadoCarlos = `Dr_Carlos_Mendoza_${mesEsperado}_${anioEsperado}`;
    await expect(nombreInput).toHaveValue(esperadoCarlos);

    await page.getByRole("button", { name: "Crear archivo" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeHidden();
    const crearCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.crearArchivo);
    expect(crearCalls.length).toBe(1);
    expect(crearCalls[0].nombre).toBe(esperadoCarlos);
    expect(crearCalls[0].defensor).toBe("Dr. Carlos Mendoza");
    expect(errors).toEqual([]);
  });

  test("IA en 'Otra', plantilla rápida desde catálogo base/usuario y alta unificada con +", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, {
      configured: true,
      iaCustomResponse: {
        juzgado: "Juzgado Promiscuo Municipal de La Paz",
        tipo_audiencia: "Otra",
        tipo_otra: "Audiencia de revocatoria de medida",
      },
    });
    await page.goto("/");

    await page
      .locator("#ia-inline-textarea")
      .fill("Audiencia de revocatoria de medida ante Juzgado Promiscuo de La Paz");
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();

    await expect(page.locator("#juzgado")).toContainText("Otra");
    await expect(page.locator("#juzgado-otra")).toHaveValue(
      "Juzgado Promiscuo Municipal de La Paz"
    );
    await expect(page.locator("#tipo-audiencia")).toContainText("Otra");
    await expect(page.locator("#tipo-otra")).toHaveValue("Audiencia de revocatoria de medida");
    await expect(page.locator(".otra-inline-block").first()).toContainText(
      "Solo para esta audiencia, o pulsa + para guardarlo en tu lista."
    );

    // Ya no existe pie duplicado "Agregar nuevo..." dentro del selector
    await page.locator("#juzgado").click();
    await expect(page.locator(".custom-select-add-trigger")).toHaveCount(0);
    await expect(page.locator(".custom-select-template-hint")).toBeVisible();
    await page.locator("#juzgado").click();

    // Cero desbordamiento horizontal en tema oscuro y claro con .otra-inline-block visible
    for (const theme of ["dark", "light"]) {
      if (theme === "light") {
        await page.getByRole("button", { name: "Cambiar tema" }).click();
        await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
      }
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth
      );
      expect(overflow, `overflow con .otra-inline-block en tema ${theme}`).toBe(false);
    }

    // Botón + junto a #juzgado-otra agrega a la lista y lo selecciona inmediatamente
    await page.getByRole("button", { name: "Guardar juzgado en la lista" }).click();
    await expect(page.locator("#juzgado")).toContainText(
      "Juzgado Promiscuo Municipal de La Paz"
    );
    await expect(page.locator("#juzgado-otra")).toHaveCount(0);
    await expect(page.locator(".toast")).toContainText("agregado a la lista de juzgados");

    // Botón + junto a #tipo-otra agrega el nuevo tipo y lo selecciona inmediatamente
    await page.getByRole("button", { name: "Guardar tipo de audiencia en la lista" }).click();
    await expect(page.locator("#tipo-audiencia")).toContainText(
      "Audiencia de revocatoria de medida"
    );
    await expect(page.locator("#tipo-otra")).toHaveCount(0);
    await expect(page.locator(".toast")).toContainText("agregado a los tipos de audiencia");

    // Usar un juzgado base como plantilla copia su texto en "Otra" y preselecciona el número
    await page.locator("#juzgado").click();
    await page
      .getByRole("button", {
        name: "Usar Juzgado 1 Promiscuo Municipal de Agustín Codazzi como plantilla",
      })
      .click();
    const juzgadoOtraInput = page.locator("#juzgado-otra");
    await expect(juzgadoOtraInput).toHaveValue("Juzgado 1 Promiscuo Municipal de Agustín Codazzi");
    await expect(juzgadoOtraInput).toBeFocused();
    await page.keyboard.type("10");
    await expect(juzgadoOtraInput).toHaveValue(
      "Juzgado 10 Promiscuo Municipal de Agustín Codazzi"
    );
    await page.getByRole("button", { name: "Guardar juzgado en la lista" }).click();
    await expect(page.locator("#juzgado")).toContainText(
      "Juzgado 10 Promiscuo Municipal de Agustín Codazzi"
    );

    // También funciona usando un juzgado guardado por el usuario como plantilla
    await page.locator("#juzgado").click();
    await page
      .getByRole("button", {
        name: "Usar Juzgado 10 Promiscuo Municipal de Agustín Codazzi como plantilla",
      })
      .click();
    await expect(juzgadoOtraInput).toHaveValue(
      "Juzgado 10 Promiscuo Municipal de Agustín Codazzi"
    );
    await expect(juzgadoOtraInput).toBeFocused();
    await page.keyboard.type("11");
    await expect(juzgadoOtraInput).toHaveValue(
      "Juzgado 11 Promiscuo Municipal de Agustín Codazzi"
    );

    // Usar un tipo de audiencia base como plantilla copia en #tipo-otra
    await page.locator("#tipo-audiencia").click();
    await page
      .getByRole("button", { name: "Usar Audiencia de imputación como plantilla" })
      .click();
    await expect(page.locator("#tipo-audiencia")).toContainText("Otra");
    await expect(page.locator("#tipo-otra")).toHaveValue("Audiencia de imputación");

    // Buscar un juzgado inexistente permite pasarlo directo a "Otra"
    await page.locator("#juzgado").click();
    await page.locator(".custom-select-search-input").fill("Juzgado Especializado de Prueba");
    await page
      .getByRole("button", { name: 'Usar "Juzgado Especializado de Prueba" en Otra' })
      .click();
    await expect(juzgadoOtraInput).toHaveValue("Juzgado Especializado de Prueba");

    expect(errors).toEqual([]);
  });

  test("Android móvil: menú de opciones táctil, puente nativo de exportación/tema y botón Atrás", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });

    await page.setViewportSize({ width: 390, height: 844 });
    await page.addInitScript(() => {
      window.__ANDROID_BRIDGE_CALLS__ = { themes: [], exports: [] };
      window.LokusAndroid = {
        isAvailable: () => true,
        setTheme: (isLight) => {
          window.__ANDROID_BRIDGE_CALLS__.themes.push(Boolean(isLight));
        },
        shareFile: (b64, fileName, mime) => {
          window.__ANDROID_BRIDGE_CALLS__.exports.push({ b64, fileName, mime, mode: "share" });
          return JSON.stringify({
            ok: true,
            shared: true,
          });
        },
        saveToDownloadsAndShare: (b64, fileName, mime, openShare) => {
          window.__ANDROID_BRIDGE_CALLS__.exports.push({ b64, fileName, mime, openShare });
          return JSON.stringify({
            ok: true,
            savedLocation: openShare ? undefined : `Descargas/Lokus/${fileName}`,
            shared: Boolean(openShare),
          });
        },
      };
    });
    await page.addInitScript(mockTauriInit, { configured: true });

    await page.goto("/");
    await expect(page.locator(".active-doc-name")).toHaveText("audiencias_enero.xlsx");

    // 1. Cero desbordamiento horizontal y cabecera móvil en 2 bandas limpias (sin botón huérfano)
    const headerLayout = await page.evaluate(() => {
      const sb = document.querySelector(".sidebar-toggle-btn")?.getBoundingClientRect();
      const brand = document.querySelector(".mobile-brand-title")?.getBoundingClientRect();
      const right = document.querySelector(".header-right")?.getBoundingClientRect();
      const nav = document.querySelector(".topbar-nav")?.getBoundingClientRect();
      const verTodos = document.querySelector(".dossier-recent-card .inline-action-link")?.getBoundingClientRect();
      return {
        overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
        sbTop: sb?.top ?? 0,
        brandTop: brand?.top ?? 0,
        brandWidth: brand?.width ?? 0,
        rightTop: right?.top ?? 0,
        navTop: nav?.top ?? 0,
        navWidth: nav?.width ?? 0,
        verTodosHeight: verTodos?.height ?? 0,
      };
    });
    expect(headerLayout.overflow).toBe(false);
    expect(headerLayout.brandWidth).toBeGreaterThan(20);
    expect(Math.abs(headerLayout.sbTop - headerLayout.rightTop)).toBeLessThan(12);
    expect(headerLayout.navTop).toBeGreaterThan(headerLayout.sbTop + 25);
    expect(headerLayout.navWidth).toBeGreaterThan(340);
    expect(headerLayout.verTodosHeight).toBeLessThanOrEqual(36);

    // 2. Abrir cajón lateral móvil y usar botón de opciones (⋮) sin clic derecho
    await page.locator(".active-doc-switch").click();
    await expect(page.locator(".sidebar.mobile-open")).toBeVisible();

    const btnOpciones = page.getByRole("button", { name: "Opciones de audiencias_enero" });
    await expect(btnOpciones).toBeVisible();
    await btnOpciones.click();

    // Exportar planilla Excel debe invocar el puente Android directo (Share Sheet sin duplicado en Descargas)
    await page.getByRole("button", { name: "Exportar copia a Descargas" }).click();
    await expect(page.locator(".toast")).toContainText(
      "Listo para compartir o guardar: audiencias_enero.xlsx"
    );

    const bridgeCalls = await page.evaluate(() => window.__ANDROID_BRIDGE_CALLS__);
    expect(bridgeCalls.exports.length).toBe(1);
    expect(bridgeCalls.exports[0].fileName).toBe("audiencias_enero.xlsx");
    expect(bridgeCalls.exports[0].mime).toBe(
      "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    expect(bridgeCalls.exports[0].mode).toBe("share");
    const cleanedExports = await page.evaluate(() => window.__LOKUS_CLEANED_EXPORTS__ || []);
    expect(cleanedExports).toContain(
      "/data/user/0/com.josebustamante.gestoraudiencias/files/exportaciones/audiencias_enero.xlsx"
    );

    // 3. Cambiar tema sincroniza con la barra de estado de Android
    await page.locator(".active-doc-switch").click();
    await page.getByRole("button", { name: "Cambiar tema" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    const themeCalls = await page.evaluate(() => window.__ANDROID_BRIDGE_CALLS__.themes);
    expect(themeCalls[themeCalls.length - 1]).toBe(true);

    // 4. Botón Atrás con menú contextual (⋮) abierto cierra primero el popover sin dejarlo huérfano
    await page.getByRole("button", { name: "Opciones de audiencias_enero" }).click();
    await expect(page.getByRole("button", { name: "Exportar copia a Descargas" })).toBeVisible();
    const backPopover = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backPopover).toBe("handled");
    await expect(page.getByRole("button", { name: "Exportar copia a Descargas" })).toHaveCount(0);
    await expect(page.locator(".sidebar.mobile-open")).toBeVisible();

    // 5. Botón '+' del sidebar cierra el cajón móvil al abrir modal y AutocompleteInput no secuestra pointerdown
    await page.getByRole("button", { name: "Crear nuevo archivo Excel" }).click();
    await expect(page.locator("dialog[open]")).toBeVisible();
    await expect(page.locator(".sidebar.mobile-open")).toHaveCount(0);
    const toggleDef = page.locator("dialog[open]").getByRole("button", { name: "Mostrar opciones guardadas" });
    await toggleDef.click();
    const primeraOpc = page.locator("dialog[open] .custom-select-option").first();
    await expect(primeraOpc).toBeVisible();
    // pointerdown táctil (inicio de scroll) NO debe seleccionar ni cerrar el desplegable
    await primeraOpc.dispatchEvent("pointerdown", { pointerType: "touch", bubbles: true });
    await expect(page.locator("dialog[open] .autocomplete-popover")).toBeVisible();
    // click normal SÍ selecciona y cierra
    await primeraOpc.click();
    await expect(page.locator("dialog[open] .autocomplete-popover")).toHaveCount(0);
    const backCrearModal = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backCrearModal).toBe("handled");
    await expect(page.locator("dialog[open]")).toHaveCount(0);

    // 6. Botón Atrás de Android: cierra sidebar -> vuelve de Registros -> import accept="*/*" -> retorna 'exit' en raíz
    await page.locator(".active-doc-switch").click();
    await expect(page.locator(".sidebar.mobile-open")).toBeVisible();
    const backSidebar = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backSidebar).toBe("handled");
    await expect(page.locator(".sidebar.mobile-open")).toHaveCount(0);

    await page.getByRole("tab", { name: /Registros/i }).click();
    await page.locator(".search-input").fill("zzznomatch");
    const emptyStateCheck = await page.evaluate(() => {
      const cell = document.querySelector(".records-empty-cell");
      const row = document.querySelector(".records-empty-row");
      return {
        overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
        textAlign: cell ? getComputedStyle(cell).textAlign : "",
        rowBorder: row ? getComputedStyle(row).borderTopStyle : "",
      };
    });
    expect(emptyStateCheck.overflow).toBe(false);
    expect(emptyStateCheck.textAlign).toBe("center");
    expect(emptyStateCheck.rowBorder).toBe("none");
    await page.locator(".search-input").fill("");

    await page.getByRole("tab", { name: /Estadísticas/i }).click();
    const statsOverflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth
    );
    expect(statsOverflow).toBe(false);

    const backTab = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backTab).toBe("handled");
    await expect(page.locator("#radicado")).toBeVisible();

    await page.getByRole("button", { name: "Exportar o importar datos" }).click();
    await expect(page.locator("dialog[open]")).toBeVisible();
    await page.locator("dialog[open]").getByRole("tab", { name: /Importar/i }).click();
    await expect(page.locator('dialog[open] input[type="file"]')).toHaveAttribute("accept", "*/*");
    const backModal = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backModal).toBe("handled");
    await expect(page.locator("dialog[open]")).toHaveCount(0);

    const backRoot = await page.evaluate(() => window.__lokusHandleAndroidBack());
    expect(backRoot).toBe("exit");

    expect(errors).toEqual([]);
  });

  test("portabilidad permite exportar e importar solo aprendizaje de IA entre equipos sin duplicados", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, { configured: true });
    await page.goto("/");

    await page.getByRole("button", { name: "Exportar o importar datos" }).click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog).toBeVisible();

    await expect(dialog.getByText("Copia completa de Lokus (.lokus)")).toBeVisible();
    await expect(dialog.getByText("Solo aprendizaje de IA (.lokus.json)")).toBeVisible();
    await expect(dialog.getByText("Solo estadísticas (.lokus.json)")).toBeVisible();

    await dialog.getByText("Solo aprendizaje de IA (.lokus.json)").click();
    await dialog.getByRole("button", { name: "Exportar a Descargas" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByText(/Aprendizaje de IA exportado en:.*lokus_ia_/)).toBeVisible();

    const exportCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.exportarPaquete);
    expect(exportCalls).toHaveLength(1);
    expect(exportCalls[0].tipoPaquete).toBe("aprendizaje_ia");

    await page.getByRole("button", { name: "Exportar o importar datos" }).click();
    await expect(dialog).toBeVisible();
    await dialog.getByRole("tab", { name: /Importar/i }).click();

    await dialog.locator('input[type="file"]').setInputFiles({
      name: "lokus_ia_celular.lokus.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify({ app: "Lokus", tipo_paquete: "aprendizaje_ia" })),
    });

    await expect(dialog.getByText("Paquete detectado: Aprendizaje de IA")).toBeVisible();
    await expect(dialog.getByText(/Aprendizaje IA:/)).toBeVisible();
    await expect(dialog.getByText(/2\s*regla\(s\) nueva\(s\)/)).toBeVisible();

    await dialog.getByRole("button", { name: "Confirmar importación" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByText(/IA sincronizada \(7 novedad\(es\)\)/)).toBeVisible();

    const importCalls = await page.evaluate(() => window.__LOKUS_TEST_CALLS__.importarPaquete);
    expect(importCalls).toHaveLength(1);

    expect(errors).toEqual([]);
  });
});
