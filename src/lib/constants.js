export const TIPOS_AUDIENCIA = [
  "Alegatos de conclusión",
  "Aplicación principio de oportunidad",
  "Audiencia concentrada",
  "Audiencia de acusación",
  "Audiencia de celebración de preacuerdo",
  "Audiencia de conciliación",
  "Audiencia de continuación de juicio oral",
  "Audiencia de control de legalidad",
  "Audiencia de formulación de acusación",
  "Audiencia de individualización de pena",
  "Audiencia de imputación",
  "Audiencia de incidente de reparación integral",
  "Audiencia de juicio oral",
  "Audiencia de lectura de decisión",
  "Audiencia de libertad por vencimiento de términos",
  "Audiencia de medidas de aseguramiento",
  "Audiencia de nulidad",
  "Audiencia de preclusión",
  "Audiencia de prórroga",
  "Audiencia de revisión de medida",
  "Audiencia de sentido de fallo",
  "Audiencia de verificación de allanamiento a cargos",
  "Audiencia de verificación de cumplimiento",
  "Audiencia preliminar",
  "Audiencia preparatoria",
  "Sentencia",
  "Otra",
];

export const MOTIVOS_LABELS = [
  "Juez",
  "Fiscalía",
  "Usuario",
  "INPEC",
  "Víctima",
  "ICBF",
  "Defensor Confianza",
  "Defensor Público",
];

export const HORAS_RAPIDAS = [
  { h24: "07", h12: "07", m: "30", periodo: "AM", label: "7:30 AM" },
  { h24: "08", h12: "08", m: "00", periodo: "AM", label: "8:00 AM" },
  { h24: "09", h12: "09", m: "00", periodo: "AM", label: "9:00 AM" },
  { h24: "10", h12: "10", m: "00", periodo: "AM", label: "10:00 AM" },
  { h24: "14", h12: "02", m: "00", periodo: "PM", label: "2:00 PM" },
  { h24: "15", h12: "03", m: "00", periodo: "PM", label: "3:00 PM" },
];

export function normalizarClaveCatalogo(texto = "") {
  return String(texto || "")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase();
}

export const MESES_ES = [
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

export function generarNombrePlanilla(defensor = "", fechaRef = new Date()) {
  const limpio = String(defensor || "")
    .replace(/[<>:"/\\|?*.\u0000-\u001f]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!limpio) return "";

  const soloLetras = limpio.replace(/[^\p{L}]/gu, "");
  const esTodoMayus =
    soloLetras.length > 0 && soloLetras === soloLetras.toLocaleUpperCase("es");

  const baseDefensor = limpio.replace(/\s+/g, "_").replace(/^_+|_+$/g, "");
  if (!baseDefensor) return "";

  const mesBase = MESES_ES[fechaRef.getMonth()] || "Mes";
  const mes = esTodoMayus ? mesBase.toLocaleUpperCase("es") : mesBase;
  const anio = fechaRef.getFullYear();
  return `${baseDefensor}_${mes}_${anio}`;
}

export const COLORES_CARPETA = [
  { id: "", label: "Predeterminado", cssVar: "var(--text-secondary)" },
  { id: "azul", label: "Azul", cssVar: "var(--folder-azul)" },
  { id: "verde", label: "Verde", cssVar: "var(--folder-verde)" },
  { id: "ambar", label: "Ámbar", cssVar: "var(--folder-ambar)" },
  { id: "coral", label: "Coral", cssVar: "var(--folder-coral)" },
  { id: "rosa", label: "Rosa", cssVar: "var(--folder-rosa)" },
  { id: "violeta", label: "Violeta", cssVar: "var(--folder-violeta)" },
  { id: "turquesa", label: "Turquesa", cssVar: "var(--folder-turquesa)" },
];

export const MODELOS_GEMINI = [
  "gemini-2.5-flash",
  "gemini-2.5-pro",
  "gemini-2.5-flash-lite",
  "gemini-3-flash-preview",
  "gemini-3.1-pro-preview",
  "gemini-3.1-flash-lite-preview",
  "gemini-flash-latest",
  "gemini-pro-latest",
];

export const MODELOS_OPENAI = [
  "gpt-6-luna",
  "gpt-5-mini",
  "gpt-5",
  "gpt-5-nano",
  "gpt-4.1-mini",
  "gpt-4.1",
  "gpt-4.1-nano",
  "o4-mini",
  "gpt-4o-mini",
  "gpt-4o",
];

// Catálogo base de despachos judiciales penales (Cesar, Magdalena, La Guajira y denominaciones generales)
export const JUZGADOS_BASE = [
  // Cesar — Valledupar y municipios
  "Juzgado 1 Penal Municipal con Función de Control de Garantías de Valledupar",
  "Juzgado 2 Penal Municipal con Función de Control de Garantías de Valledupar",
  "Juzgado 3 Penal Municipal con Función de Control de Garantías de Valledupar",
  "Juzgado 4 Penal Municipal con Función de Control de Garantías de Valledupar",
  "Juzgado 5 Penal Municipal con Función de Control de Garantías de Valledupar",
  "Juzgado 1 Penal Municipal con Funciones de Conocimiento de Valledupar",
  "Juzgado 2 Penal Municipal con Funciones de Conocimiento de Valledupar",
  "Juzgado 3 Penal Municipal con Funciones de Conocimiento de Valledupar",
  "Juzgado 1 Penal del Circuito con Funciones de Conocimiento de Valledupar",
  "Juzgado 2 Penal del Circuito con Funciones de Conocimiento de Valledupar",
  "Juzgado 3 Penal del Circuito con Funciones de Conocimiento de Valledupar",
  "Juzgado 4 Penal del Circuito con Funciones de Conocimiento de Valledupar",
  "Juzgado 5 Penal del Circuito con Funciones de Conocimiento de Valledupar",
  "Juzgado 8 Penal del Circuito con Función de Conocimiento de Valledupar",
  "Juzgado 9 Penal del Circuito con Función de Conocimiento de Valledupar",
  "Juzgado 10 Penal del Circuito con Función de Conocimiento de Valledupar",
  "Juzgado 1 Penal del Circuito Especializado de Valledupar",
  "Juzgado 2 Penal del Circuito Especializado de Valledupar",
  "Juzgado 3 Penal del Circuito Especializado de Valledupar",
  "Juzgado 4 Penal del Circuito Especializado de Valledupar",
  "Juzgado 1 de Ejecución de Penas y Medidas de Seguridad de Valledupar",
  "Juzgado 2 de Ejecución de Penas y Medidas de Seguridad de Valledupar",
  "Juzgado Penal del Circuito para Adolescentes de Valledupar",
  "Juzgado 1 Promiscuo Municipal de Aguachica",
  "Juzgado 2 Promiscuo Municipal de Aguachica",
  "Juzgado 3 Promiscuo Municipal de Aguachica",
  "Juzgado Penal del Circuito de Aguachica",
  "Juzgado 1 Promiscuo del Circuito de Aguachica",
  "Juzgado 2 Promiscuo del Circuito de Aguachica",
  "Juzgado 3 Penal del Circuito con Función de Conocimiento Aguachica",
  "Juzgado 1 Promiscuo Municipal de Agustín Codazzi",
  "Juzgado 2 Promiscuo Municipal de Agustín Codazzi",
  "Juzgado Promiscuo Municipal de Bosconia",
  "Juzgado Promiscuo del Circuito de Chiriguaná",
  "Juzgado Promiscuo Municipal de Chiriguaná",
  "Juzgado Promiscuo Primero Municipal de Chiriguaná",
  "Juzgado Promiscuo Municipal de Chimichagua",
  "Juzgado Promiscuo Municipal de Curumaní",
  "Juzgado Promiscuo Municipal de La Jagua de Ibirico",
  "Juzgado Promiscuo Municipal de El Copey",
  "Juzgado Promiscuo Municipal del Paso",
  "Juzgado Promiscuo Municipal de San Alberto",
  "Juzgado 1 Promiscuo Municipal de San Alberto",
  "Tribunal Superior del Distrito Judicial de Valledupar - Sala Penal",

  // Magdalena — Santa Marta y municipios
  "Juzgado 1 Penal Municipal con Función de Control de Garantías de Santa Marta",
  "Juzgado 2 Penal Municipal con Función de Control de Garantías de Santa Marta",
  "Juzgado 3 Penal Municipal con Función de Control de Garantías de Santa Marta",
  "Juzgado 4 Penal Municipal con Función de Control de Garantías de Santa Marta",
  "Juzgado 5 Penal Municipal con Función de Control de Garantías de Santa Marta",
  "Juzgado 1 Penal Municipal con Funciones de Conocimiento de Santa Marta",
  "Juzgado 2 Penal Municipal con Funciones de Conocimiento de Santa Marta",
  "Juzgado 1 Penal del Circuito con Funciones de Conocimiento de Santa Marta",
  "Juzgado 2 Penal del Circuito con Funciones de Conocimiento de Santa Marta",
  "Juzgado 3 Penal del Circuito con Funciones de Conocimiento de Santa Marta",
  "Juzgado 4 Penal del Circuito con Funciones de Conocimiento de Santa Marta",
  "Juzgado 5 Penal del Circuito con Funciones de Conocimiento de Santa Marta",
  "Juzgado Penal del Circuito Especializado de Santa Marta",
  "Juzgado 1 de Ejecución de Penas y Medidas de Seguridad de Santa Marta",
  "Juzgado 2 de Ejecución de Penas y Medidas de Seguridad de Santa Marta",
  "Juzgado 1 Promiscuo Municipal de Ciénaga",
  "Juzgado 2 Promiscuo Municipal de Ciénaga",
  "Juzgado 3 Promiscuo Municipal de Ciénaga",
  "Juzgado Penal del Circuito de Ciénaga",
  "Juzgado Promiscuo Municipal de Fundación",
  "Juzgado Penal del Circuito de Fundación",
  "Juzgado Promiscuo Municipal de El Banco",
  "Juzgado Penal del Circuito de El Banco",
  "Juzgado Promiscuo Municipal de Plato",
  "Juzgado Penal del Circuito de Plato",
  "Juzgado Promiscuo Municipal de Pivijay",
  "Juzgado Promiscuo Municipal de Aracataca",
  "Tribunal Superior del Distrito Judicial de Santa Marta - Sala Penal",

  // La Guajira — Riohacha, Maicao y municipios
  "Juzgado 1 Penal Municipal con Función de Control de Garantías de Riohacha",
  "Juzgado 2 Penal Municipal con Función de Control de Garantías de Riohacha",
  "Juzgado 3 Penal Municipal con Función de Control de Garantías de Riohacha",
  "Juzgado 1 Penal Municipal con Funciones de Conocimiento de Riohacha",
  "Juzgado 2 Penal Municipal con Funciones de Conocimiento de Riohacha",
  "Juzgado 1 Penal del Circuito de Riohacha",
  "Juzgado 2 Penal del Circuito de Riohacha",
  "Juzgado 3 Penal del Circuito de Riohacha",
  "Juzgado Penal del Circuito Especializado de Riohacha",
  "Juzgado de Ejecución de Penas y Medidas de Seguridad de Riohacha",
  "Juzgado 1 Promiscuo Municipal de Maicao",
  "Juzgado 2 Promiscuo Municipal de Maicao",
  "Juzgado 3 Promiscuo Municipal de Maicao",
  "Juzgado Penal del Circuito de Maicao",
  "Juzgado 1 Promiscuo Municipal de San Juan del Cesar",
  "Juzgado 2 Promiscuo Municipal de San Juan del Cesar",
  "Juzgado Promiscuo del Circuito de San Juan del Cesar",
  "Juzgado Promiscuo Municipal de Fonseca",
  "Juzgado Promiscuo Municipal de Villanueva",
  "Juzgado Promiscuo del Circuito de Villanueva",
  "Juzgado Promiscuo Municipal de Uribia",
  "Juzgado Promiscuo Municipal de Barrancas",
  "Tribunal Superior del Distrito Judicial de Riohacha - Sala Penal",

  // Denominaciones abreviadas / generales frecuentes
  "Juzgado 1 Penal del Circuito",
  "Juzgado 2 Penal del Circuito",
  "Juzgado 3 Penal del Circuito",
  "Juzgado 4 Penal del Circuito",
  "Juzgado 5 Penal del Circuito",
  "Juzgado 1 Penal Municipal",
  "Juzgado 2 Penal Municipal",
  "Juzgado 3 Penal Municipal",
  "Juzgado 4 Penal Municipal",
  "Juzgado 5 Penal Municipal",
  "Juzgado 6 Penal Municipal",
  "Juzgado 7 Penal Municipal",
  "Juzgado 8 Penal Municipal",
  "Juzgado 9 Penal Municipal",
  "Juzgado 10 Penal Municipal",
  "Juzgado 11 Penal Municipal",
  "Juzgado 13 Penal Municipal",
  "Juzgado Promiscuo Municipal",
  "Juzgado Promiscuo del Circuito",
  "Juzgado Penal del Circuito Especializado",
];
