import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const FIXTURES_DIR = path.join(__dirname, "fixtures");

export const PAQUETE_ROOT_KEYS_V4 = [
  "app",
  "version_app",
  "formato",
  "exportado",
  "aciertos",
  "total_reglas",
  "total_correcciones",
  "total_ejemplos_aciertos",
  "perfiles_defensores",
  "llamadas_induccion_llm_total",
  "llamadas_extraccion_total",
  "llamadas_consolidacion_total",
  "correcciones_por_campo",
  "metricas_por_campo",
  "metricas_por_modelo",
  "preferencias_estilo",
  "preferencias_por_defensor",
  "defensores_directorio",
  "resumen_patrones_generales",
  "grafo_asociaciones",
  "perfiles_sinteticos",
  "consolidaciones_ia",
  "alias_validados",
  "patrones_abreviacion",
  "juzgados_agregados_usuario",
  "tipos_audiencia_agregados_usuario",
  "uso_catalogos",
  "reglas",
  "correcciones",
];

export const PAQUETE_ADDITIVE_KEYS_V5 = [
  "instancia_id",
  "exportacion_seq",
  "exportacion_id",
  "ultima_exportacion_previa",
  "fuentes_importadas",
  "tabla_alias_raw",
  "preferencias_raw",
  "preferencias_por_defensor_raw",
  "ejemplos_aciertos",
];

const HORAS_RAPIDAS_DEF = [
  { hora_24: "08:00", etiqueta_12h: "8:00 AM" },
  { hora_24: "09:00", etiqueta_12h: "9:00 AM" },
  { hora_24: "10:00", etiqueta_12h: "10:00 AM" },
  { hora_24: "14:00", etiqueta_12h: "2:00 PM" },
  { hora_24: "15:00", etiqueta_12h: "3:00 PM" },
  { hora_24: "16:00", etiqueta_12h: "4:00 PM" },
];

export function crearAudienciaInput(overrides = {}) {
  return {
    radicado: "[PROCESADO] - [RADICADO]",
    tipo: "Audiencia de juicio oral",
    fecha: "20/10/2026",
    hora: "09:00",
    juzgado: "Juzgado 9 Penal Municipal",
    realizada: "SI",
    motivo: "",
    observaciones: "Se realizó audiencia de manera adecuada.",
    ...overrides,
  };
}

export function crearRegla(overrides = {}) {
  return {
    id: "regla-test-1",
    tipo_error: "juzgado",
    regla: "Mantener el nombre completo del despacho judicial sin inventar municipio.",
    refuerzos: 2,
    actualizada: "2026-10-15T10:00:00-05:00",
    estado: "activa",
    campo_objetivo: "juzgado",
    triggers: ["juzgado", "despacho", "municipal"],
    alpha: 8.0,
    beta: 1.5,
    impresiones: 10,
    aciertos_directos: 9,
    fallos_directos: 1,
    origen: "mitigacion_modelo",
    modelo_alcance: "openai:gpt-5-mini",
    defensor_alcance: "*",
    defensores_confirmantes: ["david felipe ariza perez"],
    ultima_actualizacion: "2026-10-20T12:00:00-05:00",
    confianza_llm: 0.92,
    aciertos_sombra: 0,
    confirmaciones_sombra: 0,
    origen_correccion_id: "corr-test-1",
    ...overrides,
  };
}

export function crearCorreccion(overrides = {}) {
  const prediccion = crearAudienciaInput(overrides.prediccion || {});
  const finalUsuario = crearAudienciaInput({
    juzgado: "Juzgado 9 Penal Municipal",
    ...(overrides.final_usuario || {}),
  });
  return {
    id: "corr-test-1",
    registrada: "2026-10-15T09:30:00-05:00",
    proveedor: "openai",
    modelo: "gpt-5-mini",
    texto_original: "Audiencia de juicio oral en el juzgado 9 penal municipal contra [PROCESADO].",
    prediccion,
    final_usuario: finalUsuario,
    campos: overrides.campos || [
      {
        campo: "observaciones",
        valor_ia: "Se realizó sin novedad.",
        valor_usuario: "Se realizó audiencia de manera adecuada.",
      },
    ],
    estado: "analizada",
    regla_generada: "Usar redacción breve para audiencias realizadas.",
    regla_id_asociada: "regla-test-1",
    pregunta_ia: "",
    nota_usuario: "",
    exportada: true,
    defensor_clave: "david felipe ariza perez",
    ...overrides,
    prediccion,
    final_usuario: finalUsuario,
  };
}

export function crearUsoCatalogos(overrides = {}) {
  const totalAudiencias = overrides.total_audiencias_contabilizadas ?? 20;
  const tiposDetalle = overrides.tipos_detalle || [
    {
      nombre: "Audiencia de juicio oral",
      origen: "predeterminado",
      conteo: totalAudiencias > 0 ? Math.max(1, Math.floor(totalAudiencias * 0.5)) : 0,
      porcentaje: totalAudiencias > 0 ? 50.0 : 0.0,
    },
    {
      nombre: "Audiencia de acusación",
      origen: "predeterminado",
      conteo: 0,
      porcentaje: 0.0,
    },
  ];
  const juzgadosDetalle = overrides.juzgados_detalle || [
    {
      nombre: "Juzgado 1 Penal Municipal con Función de Control de Garantías de Valledupar",
      origen: "predeterminado",
      conteo: totalAudiencias > 0 ? Math.max(1, Math.floor(totalAudiencias * 0.4)) : 0,
      porcentaje: totalAudiencias > 0 ? 40.0 : 0.0,
    },
    {
      nombre: "Juzgado 1 de Ejecución de Penas y Medidas de Seguridad de Santa Marta",
      origen: "predeterminado",
      conteo: 0,
      porcentaje: 0.0,
    },
  ];
  const horasDetalle = overrides.horas_detalle || [
    {
      hora_24: "08:00",
      etiqueta_12h: "8:00 AM",
      es_hora_rapida: true,
      conteo: totalAudiencias > 0 ? Math.max(1, Math.floor(totalAudiencias * 0.3)) : 0,
      porcentaje: totalAudiencias > 0 ? 30.0 : 0.0,
    },
  ];

  const horasRapidasPredeterminadas =
    overrides.horas_rapidas_predeterminadas ||
    HORAS_RAPIDAS_DEF.map((h) => {
      const match = horasDetalle.find((d) => d.hora_24 === h.hora_24);
      const conteo = match ? match.conteo : 0;
      const porcentaje =
        totalAudiencias > 0 ? Math.round((conteo / totalAudiencias) * 10000) / 100 : 0.0;
      return {
        hora_24: h.hora_24,
        etiqueta_12h: h.etiqueta_12h,
        es_hora_rapida: true,
        conteo,
        porcentaje,
      };
    });

  const horasRapidasSinUso =
    overrides.horas_rapidas_sin_uso ||
    horasRapidasPredeterminadas.filter((h) => h.conteo === 0).map((h) => h.hora_24);

  const horasNoRapidas =
    overrides.horas_mas_frecuentes_no_rapidas ||
    horasDetalle.filter((h) => !h.es_hora_rapida && h.conteo > 0);

  return {
    total_audiencias_contabilizadas: totalAudiencias,
    sembrado_inicial_completado: overrides.sembrado_inicial_completado ?? true,
    sembrado_en: overrides.sembrado_en ?? "2026-10-01T09:00:00-05:00",
    ultima_actualizacion: overrides.ultima_actualizacion ?? "2026-10-31T17:30:00-05:00",
    tipos_audiencia: {
      total_audiencias: totalAudiencias,
      total_con_valor: overrides.tipos_con_valor ?? totalAudiencias,
      total_sin_especificar: overrides.tipos_sin_especificar ?? 0,
      porcentaje_sin_especificar: 0.0,
      total_opciones_predeterminadas: 17,
      total_opciones_lista_usuario: (overrides.tipos_nuevos || []).length,
      total_opciones_otra: 0,
      total_opciones_eliminadas_tras_uso: 0,
      nuevos_no_predeterminados: overrides.tipos_nuevos || [],
      predeterminados_sin_uso:
        overrides.tipos_predeterminados_sin_uso ||
        tiposDetalle.filter((t) => t.origen === "predeterminado" && t.conteo === 0).map((t) => t.nombre),
      agregados_usuario_sin_uso: [],
      detalle: tiposDetalle,
    },
    juzgados: {
      total_audiencias: totalAudiencias,
      total_con_valor: overrides.juzgados_con_valor ?? totalAudiencias,
      total_sin_especificar: overrides.juzgados_sin_especificar ?? 0,
      porcentaje_sin_especificar: 0.0,
      total_opciones_predeterminadas: 96,
      total_opciones_lista_usuario: (overrides.juzgados_nuevos || []).filter(
        (j) => j.origen === "agregado_usuario"
      ).length,
      total_opciones_otra: (overrides.juzgados_nuevos || []).filter((j) => j.origen === "otra").length,
      total_opciones_eliminadas_tras_uso: 0,
      nuevos_no_predeterminados: overrides.juzgados_nuevos || [],
      predeterminados_sin_uso:
        overrides.juzgados_predeterminados_sin_uso ||
        juzgadosDetalle
          .filter((j) => j.origen === "predeterminado" && j.conteo === 0)
          .map((j) => j.nombre),
      agregados_usuario_sin_uso: [],
      detalle: juzgadosDetalle,
    },
    horas: {
      total_audiencias: totalAudiencias,
      total_con_hora: overrides.horas_con_valor ?? totalAudiencias,
      total_sin_hora: overrides.horas_sin_especificar ?? 0,
      porcentaje_sin_hora: 0.0,
      horas_rapidas_predeterminadas: horasRapidasPredeterminadas,
      horas_rapidas_sin_uso: horasRapidasSinUso,
      horas_mas_frecuentes_no_rapidas: horasNoRapidas,
      detalle_por_hora_exacta: horasDetalle,
      resumen_por_franja_horaria: overrides.resumen_por_franja_horaria || [
        {
          franja: "Mañana (06:00-11:59)",
          conteo: totalAudiencias,
          porcentaje: totalAudiencias > 0 ? 100.0 : 0.0,
        },
      ],
    },
  };
}

export function crearPaqueteV4(overrides = {}) {
  const reglas = overrides.reglas ?? [];
  const correcciones = overrides.correcciones ?? [];
  const aciertos = overrides.aciertos ?? 10;
  const defensoresDirectorio = overrides.defensores_directorio ?? [
    {
      clave: "david felipe ariza perez",
      nombre: "David Felipe Ariza Perez",
      aciertos,
      correcciones: correcciones.length,
      tasa_acierto:
        aciertos + correcciones.length > 0
          ? aciertos / (aciertos + correcciones.length)
          : 0.0,
      por_campo: [],
    },
  ];
  const usoCatalogos = overrides.uso_catalogos || crearUsoCatalogos(overrides.uso_catalogos_overrides || {});

  const base = {
    app: "Lokus",
    version_app: "5.2.0",
    formato: 4,
    exportado: "2026-10-31T18:00:00-05:00",
    aciertos,
    total_reglas: reglas.length,
    total_correcciones: correcciones.length,
    total_ejemplos_aciertos: aciertos,
    perfiles_defensores: defensoresDirectorio.length,
    llamadas_induccion_llm_total: 2,
    llamadas_extraccion_total: aciertos + correcciones.length,
    llamadas_consolidacion_total: 1,
    correcciones_por_campo: { observaciones: correcciones.length },
    metricas_por_campo: [
      {
        campo: "juzgado",
        evaluaciones: Math.max(1, aciertos + correcciones.length),
        aciertos,
        correcciones: correcciones.length,
        precision_ewma: 0.9,
        confianza_q10: 0.75,
        drift_activo: false,
      },
    ],
    metricas_por_modelo: [
      {
        clave: "openai:gpt-5-mini",
        evaluaciones: aciertos + correcciones.length,
        aciertos,
        correcciones: correcciones.length,
        precision_media:
          aciertos + correcciones.length > 0
            ? aciertos / (aciertos + correcciones.length)
            : 0.0,
      },
    ],
    preferencias_estilo: {
      incluir_causas_secundarias_prob: 0.75,
      incluir_reprogramacion_prob: 0.75,
      se_fija_aplazamiento_prob: 0.5,
      instalada_reprogramada_si_prob: 0.25,
    },
    preferencias_por_defensor: {},
    defensores_directorio: defensoresDirectorio,
    resumen_patrones_generales: {
      total_defensores_activos: defensoresDirectorio.length,
      top_juzgados_globales: [["Juzgado 9 Penal Municipal", 4]],
      top_tipos_globales: [["Audiencia de juicio oral", 6]],
      top_horas_globales: [[8, 6]],
      reglas_transversales_promovidas: 0,
      errores_recurrentes_transversales: [],
    },
    grafo_asociaciones: {
      defensor_a_juzgados: {},
      defensor_a_tipos: {},
      defensor_sigla_a_juzgado: {},
      prefijo_radicado_a_juzgados: {},
      tipo_a_juzgados: {},
    },
    perfiles_sinteticos: [],
    consolidaciones_ia: [],
    alias_validados: [],
    patrones_abreviacion: [],
    juzgados_agregados_usuario: [],
    tipos_audiencia_agregados_usuario: [],
    uso_catalogos: usoCatalogos,
    reglas,
    correcciones,
  };

  const { uso_catalogos_overrides: _unused, ...restOverrides } = overrides;
  return {
    ...base,
    ...restOverrides,
    total_reglas: restOverrides.total_reglas ?? (restOverrides.reglas ?? reglas).length,
    total_correcciones:
      restOverrides.total_correcciones ?? (restOverrides.correcciones ?? correcciones).length,
  };
}

export function crearPaqueteV5(overrides = {}) {
  const seq = overrides.exportacion_seq ?? 1;
  const v4 = crearPaqueteV4({
    ...overrides,
    formato: 5,
  });
  return {
    ...v4,
    formato: 5,
    instancia_id: "instancia_id" in overrides ? overrides.instancia_id : "inst-test-001",
    exportacion_seq: seq,
    exportacion_id:
      "exportacion_id" in overrides ? overrides.exportacion_id : `exp-20261101-${seq}`,
    ultima_exportacion_previa:
      "ultima_exportacion_previa" in overrides ? overrides.ultima_exportacion_previa : "",
    fuentes_importadas:
      "fuentes_importadas" in overrides ? overrides.fuentes_importadas : [],
    tabla_alias_raw: "tabla_alias_raw" in overrides ? overrides.tabla_alias_raw : [],
    preferencias_raw:
      "preferencias_raw" in overrides
        ? overrides.preferencias_raw
        : {
            incluir_causas_secundarias_alpha: 3.0,
            incluir_causas_secundarias_beta: 1.0,
            incluir_reprogramacion_alpha: 3.0,
            incluir_reprogramacion_beta: 1.0,
            se_fija_aplazamiento_alpha: 2.0,
            se_fija_aplazamiento_beta: 2.0,
            instalada_reprogramada_si_alpha: 1.0,
            instalada_reprogramada_si_beta: 3.0,
          },
    preferencias_por_defensor_raw:
      "preferencias_por_defensor_raw" in overrides
        ? overrides.preferencias_por_defensor_raw
        : {},
    ejemplos_aciertos:
      "ejemplos_aciertos" in overrides ? overrides.ejemplos_aciertos : [],
  };
}

export function loadFixture(filename) {
  const fullPath = path.join(FIXTURES_DIR, filename);
  return JSON.parse(fs.readFileSync(fullPath, "utf8"));
}

export function fixturePath(filename) {
  return path.join(FIXTURES_DIR, filename);
}

export function writeTempFixture(dir, filename, data) {
  const target = path.join(dir, filename);
  fs.writeFileSync(target, typeof data === "string" ? data : JSON.stringify(data, null, 2), "utf8");
  return target;
}
