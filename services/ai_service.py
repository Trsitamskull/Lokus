"""
Servicio de IA para autocompletar formularios de audiencias.
Utiliza OpenAI GPT para extraer información de texto libre directamente.
Sistema optimizado para máxima precisión en extracción con auto-entrenamiento.
"""

import json
import re
import time
from typing import Dict, Optional, Any, List
from openai import OpenAI
from datetime import datetime

try:
    from openai import OpenAI

    OPENAI_AVAILABLE = True
except ImportError:
    OPENAI_AVAILABLE = False
    print("ERROR: OpenAI no está instalado. Ejecuta: pip install openai")

# CONFIGURACIÓN EMBEBIDA - API KEY OCULTA EN EL EJECUTABLE
OPENAI_API_KEY_EMBEBIDA = ""  # API Key removida por seguridad - se configura en build

try:
    from config.config import OPENAI_API_KEY, OPENAI_MODEL

    HAS_CONFIG = True
    print("Usando configuracion externa (config.py)")
except ImportError:
    # Usar configuración embebida en el ejecutable
    OPENAI_API_KEY = OPENAI_API_KEY_EMBEBIDA
    OPENAI_MODEL = "gpt-4o-mini"
    HAS_CONFIG = False
    print("Usando configuracion embebida (API key oculta)")


class AIFormFiller:
    """Servicio para autocompletar formularios usando OpenAI con auto-entrenamiento."""

    # Singleton para cliente OpenAI (optimización de memoria)
    _client_instance = None
    _current_api_key = None

    def __init__(self, api_key: Optional[str] = None):
        self.api_key = api_key or OPENAI_API_KEY

        if not OPENAI_AVAILABLE:
            raise Exception(
                "OpenAI no está disponible. Instala con: pip install openai"
            )

        if not self.api_key:
            raise Exception("API Key de OpenAI requerida. Configura config.py")

        # Optimización: Reutilizar cliente OpenAI si la API key es la misma
        if (
            AIFormFiller._client_instance is None
            or AIFormFiller._current_api_key != self.api_key
        ):
            AIFormFiller._client_instance = OpenAI(api_key=self.api_key)
            AIFormFiller._current_api_key = self.api_key

        self.client = AIFormFiller._client_instance
        # MEJORA DE RENDIMIENTO: Usar modelo más rápido por defecto
        self.model = "gpt-4o-mini"  # Más rápido que gpt-4o
        self.model_rapido = "gpt-3.5-turbo"  # Para operaciones simples

        # MEJORAS BASADAS EN DATOS REALES
        self.diccionario_abreviaciones = {
            "F": "Fiscal",
            # 'CTO': 'Circuito',  # REMOVIDO: Ambiguo (puede ser Circuito o Conocimiento) - mantener original
            "EZADO": "Especializado",
            "EZADA": "Especializada",
            "DTO": "Documento",
            "GRAVADO": "Agravado",
            "AGRVADO": "Agravado",
            "FRACSA": "Fracasa",
            "REPROGRAMARA": "Reprogramará",
            "REEMPLAZDAO": "Reemplazado",
            "DEPACHO": "Despacho",
            "AUDIEN": "Audiencia",
            "CONTINUA": "Continuación",
            "INICIA": "Inicio",
            "PREPARATORIA": "Audiencia Preparatoria",
            "ACUSACION": "Audiencia de Acusación",
        }

        # MOTIVOS VÁLIDOS SEGÚN TABLA EXCEL
        self.motivos_validos = {
            "juez": "juez",
            "fiscalía": "fiscalía",
            "usuario": "usuario",
            "inpec": "inpec",
            "víctima": "víctima",
            "icbf": "icbf",
            "defensor confianza": "defensor confianza",
            "defensor público": "defensor público",
        }

        self.motivos_implícitos = {
            # JUEZ
            "el juez entró en periodo de vacaciones": "juez",
            "no habían nombrado titular de despacho": "juez",
            "juez no compareció": "juez",
            "ausencia del juez": "juez",
            "juzgado cerrado": "juez",
            "juez en comisión": "juez",
            # FISCALÍA
            "la fiscal no se conectó": "fiscalía",
            "fiscal no se conectó": "fiscalía",
            "fiscal fue reemplazado": "fiscalía",
            "fiscal fue reemplazdao": "fiscalía",
            "no conocer la carpeta": "fiscalía",
            "solicitud de fiscalía": "fiscalía",
            "solicitud de la fiscalía": "fiscalía",
            "por solicitud de la fiscalía": "fiscalía",
            "fiscalía no compareció": "fiscalía",
            "fiscal solicitó reprogramación": "fiscalía",
            "la fiscalía solicitó reprogramación": "fiscalía",
            "fiscalía solicitó reprogramación": "fiscalía",
            "la fiscal no vino": "fiscalía",
            "fiscal no vino": "fiscalía",
            "la fiscal no asistió": "fiscalía",
            "fiscal no asistió": "fiscalía",
            "ausencia de la fiscal": "fiscalía",
            "ausencia del fiscal": "fiscalía",
            # DEFENSOR PÚBLICO
            "defensor público no compareció": "defensor público",
            "inasistencia del defensor público": "defensor público",
            "defensor público solicitó reprogramación": "defensor público",
            "solicité reprogramarla": "defensor público",
            "solicité reprogramación": "defensor público",
            "solicité la reprogramación": "defensor público",
            "solicité la reprogramación de la audiencia": "defensor público",
            "pedí reprogramación": "defensor público",
            "pedí la reprogramación": "defensor público",
            "pedí aplazamiento": "defensor público",
            "pedí aplazamiento por no contactar al procesado": "defensor público",
            "soliciré reprogramar": "defensor público",
            "solicito reprogramar": "defensor público",
            "solicito la reprogramación": "defensor público",
            "pido reprogramación": "defensor público",
            "pido la reprogramación": "defensor público",
            "no pude contactar": "defensor público",
            "no pude contactar al acusado": "defensor público",
            "no he tenido contacto": "defensor público",
            "no había tenido contacto con el procesado": "defensor público",
            "no he logrado contactar": "defensor público",
            "me fue imposible contactar": "defensor público",
            "me fue imposible asistir": "defensor público",
            "me fue imposible asistir a la audiencia": "defensor público",
            "no logré comunicarme": "defensor público",
            "realicé conexión virtual": "defensor público",
            "realicé conexión virtual pero solicité reprogramar": "defensor público",
            # DEFENSOR DE CONFIANZA
            "defensor de confianza no compareció": "defensor confianza",
            "abogado no compareció": "defensor confianza",
            "abogado contractual no compareció": "defensor confianza",
            "abogado contractual no se presentó": "defensor confianza",
            "abogado contractual ausente": "defensor confianza",
            "defensor de confianza ausente": "defensor confianza",
            "inasistencia del defensor de confianza": "defensor confianza",
            "inasistencia del abogado": "defensor confianza",
            "inasistencia del abogado contractual": "defensor confianza",
            "defensor de confianza solicitó reprogramación": "defensor confianza",
            "defensor de confianza solicita aplazamiento": "defensor confianza",
            "abogado contractual solicitó reprogramación": "defensor confianza",
            "abogado contractual pidió nueva fecha": "defensor confianza",
            "abogado solicitó reprogramación": "defensor confianza",
            "el abogado solicitó reprogramación": "defensor confianza",
            "el abogado privado solicitó reprogramación": "defensor confianza",
            "el abogado no se presentó": "defensor confianza",
            "el defensor de confianza no se presentó": "defensor confianza",
            "el abogado del procesado no compareció": "defensor confianza",
            "falta del defensor de confianza": "defensor confianza",
            "falta del abogado contractual": "defensor confianza",
            "ausencia de defensor de confianza": "defensor confianza",
            "ausencia de abogado contractual": "defensor confianza",
            "defensor privado no compareció": "defensor confianza",
            "abogado privado no compareció": "defensor confianza",
            "solicitud del abogado contractual": "defensor confianza",
            "defensor de confianza inasistente": "defensor confianza",
            "abogado contractual inasistente": "defensor confianza",
            "defensor confianza inasistente": "defensor confianza",
            "inasistente defensor de confianza": "defensor confianza",
            "inasistente abogado contractual": "defensor confianza",
            "defensor de confianza": "defensor confianza",
            "abogado contractual": "defensor confianza",
            "def confianza": "defensor confianza",
            "def. confianza": "defensor confianza",
            "def de confianza": "defensor confianza",
            "defensor confianza": "defensor confianza",
            # USUARIO/PROCESADO
            "no notificaron al procesado": "usuario",
            "procesado no compareció": "usuario",
            "usuario no se presentó": "usuario",
            "procesado ausente": "usuario",
            # INPEC
            "inpec no trasladó": "inpec",
            "problemas de traslado": "inpec",
            "inpec reportó": "inpec",
            # VÍCTIMA
            "víctima no compareció": "víctima",
            "ausencia de la víctima": "víctima",
            # ICBF
            "icbf no compareció": "icbf",
            "representante icbf ausente": "icbf",
            # MOTIVOS ADMINISTRATIVOS/TÉCNICOS
            "despacho en otra audiencia": "juez",
            "estar el despacho en audiencia": "juez",
            "fracsa por estar el despacho": "juez",
            "fracasa por estar el despacho": "juez",
            "depacho en otra audiencia": "juez",
            "ambos despachos en audiencia": "juez",
            "problemas técnicos": "usuario",
            "fallas de conectividad": "usuario",
            "posible preacuerdo": "fiscalía",
            # PATRONES GENÉRICOS
            "solicita reprogramación": "usuario",
            "solicitó la reprogramación": "usuario",
            "reprogramación solicitada": "usuario",
            "solicitó reprogramar": "usuario",
        }

        # Configuración del servicio (sin auto-entrenamiento)
        self.training_service = None
        self.training_enabled = False
        print(f"OpenAI configurado - Modelo: {self.model}")

        # Caché para optimización de rendimiento
        self._cache_abreviaciones = {}
        self._cache_resultados = {}

        # NUEVO: Caché persistente para optimización extrema
        try:
            from services.cache_service import get_cache_instance

            self.persistent_cache = get_cache_instance()
            print("Cache persistente activado - Optimizacion extrema habilitada")
        except ImportError as e:
            self.persistent_cache = None
            print(f"Cache persistente no disponible: {e}")

    def _expandir_abreviaciones(self, texto: str) -> str:
        """Expandir abreviaciones judiciales encontradas en datos reales (con caché)"""
        # Optimización: usar caché para textos ya procesados
        if texto in self._cache_abreviaciones:
            return self._cache_abreviaciones[texto]

        texto_expandido = texto

        for abrev, expansion in self.diccionario_abreviaciones.items():
            # Reemplazar abreviación completa manteniendo contexto
            patron = r"\b" + re.escape(abrev) + r"\b"
            texto_expandido = re.sub(
                patron, expansion, texto_expandido, flags=re.IGNORECASE
            )

        # Guardar en caché
        self._cache_abreviaciones[texto] = texto_expandido

        return texto_expandido

    def _inferir_motivos_implícitos(self, texto: str) -> str:
        """
        Inferir motivos basado en frases implícitas del texto.
        SIMPLIFICADO: Análisis contextual puro, sin jerarquías predefinidas.
        """
        texto_lower = texto.lower()

        # Buscar motivos específicos en el texto
        for frase, motivo in self.motivos_implícitos.items():
            if frase in texto_lower:
                return motivo

        # Si no se encontró ningún motivo específico, devolver vacío
        return ""

    def _detectar_defensor_confianza(self, texto_lower: str) -> bool:
        """
        Detecta específicamente si el texto se refiere a un defensor de confianza.
        """
        indicadores_confianza = [
            "defensor de confianza",
            "abogado contractual",
            "abogado privado",
            "defensor privado",
            "el abogado",
            "su abogado",
            "abogado del procesado",
            "abogado defensor",
            "defensor de confianza inasistente",
            "abogado contractual inasistente",
            "inasistente defensor de confianza",
            "inasistente abogado contractual",
            "def confianza",
            "def. confianza",
            "def de confianza",
            "defensor confianza",
        ]

        return any(indicador in texto_lower for indicador in indicadores_confianza)

    def _mejorar_fecha_compacta(self, texto: str) -> str:
        """Mejorar detección de fechas en formato compacto como 'JUNIO 24.-'"""
        meses_map = {
            "ENERO": "enero",
            "FEBRERO": "febrero",
            "MARZO": "marzo",
            "ABRIL": "abril",
            "MAYO": "mayo",
            "JUNIO": "junio",
            "JULIO": "julio",
            "AGOSTO": "agosto",
            "SEPTIEMBRE": "septiembre",
            "OCTUBRE": "octubre",
            "NOVIEMBRE": "noviembre",
            "DICIEMBRE": "diciembre",
        }

        texto_mejorado = texto

        # Convertir formato "JUNIO 24.-" a "24 de junio de 2025"
        patron_fecha_compacta = r"(ENERO|FEBRERO|MARZO|ABRIL|MAYO|JUNIO|JULIO|AGOSTO|SEPTIEMBRE|OCTUBRE|NOVIEMBRE|DICIEMBRE)\s+(\d{1,2})\.-"

        def reemplazar_fecha(match):
            mes, dia = match.groups()
            mes_lower = meses_map.get(mes, mes.lower())
            return f"{dia} de {mes_lower} de 2025"

        texto_mejorado = re.sub(patron_fecha_compacta, reemplazar_fecha, texto_mejorado)

        return texto_mejorado

    def _preprocesar_texto(self, texto: str) -> str:
        """Preprocesar texto aplicando todas las mejoras identificadas"""
        # 1. Expandir abreviaciones
        texto_procesado = self._expandir_abreviaciones(texto)

        # 2. Mejorar fechas compactas
        texto_procesado = self._mejorar_fecha_compacta(texto_procesado)

        # 3. Detectar y optimizar estilo compacto
        if self._detectar_estilo_compacto(texto):
            texto_procesado = self._optimizar_estilo_compacto(texto_procesado)

        return texto_procesado

    def _detectar_estilo_compacto(self, texto: str) -> bool:
        """Detectar si el texto corresponde al estilo compacto del Defensor 2"""
        indicadores_compactos = [
            "FRACSA",
            "FRACASA",
            "FRCASA",
            "DEPACHO",
            "EZADO",
            "EZADA",
            "CTO",
            "CFTO",
            "GENDA",
            "CONTINUA JUICIO",
            "INICIA JUICIO",
            "PREPARATORIA",
            "ACUSACION",
        ]

        texto_upper = texto.upper()
        indicadores_encontrados = sum(
            1 for indicador in indicadores_compactos if indicador in texto_upper
        )

        # Es estilo compacto si tiene 2 o más indicadores
        return indicadores_encontrados >= 2

    def _optimizar_estilo_compacto(self, texto: str) -> str:
        """Aplicar optimizaciones específicas para el estilo compacto"""
        texto_optimizado = texto

        # Diccionario específico para Defensor 2
        optimizaciones_d2 = {
            "FRACSA": "Fracasó",
            "FRACASA": "Fracasó",
            "FRCASA": "Fracasó",
            "DEPACHO": "Despacho",
            "EZADO": "Juzgado",
            "EZADA": "Juzgada",
            "GENDA": "Agenda",
            "BMEJA": "Barranquilla",
            "GRAVADO": "Agravado",
            "AGRVADO": "Agravado",
            "HUBICARSE": "Ubicarse",
            "TESTI": "Testigos",
            "RDRIGUEZ": "Rodriguez",
            "CONTINUCION": "Continuación",
            "PREPARATOR": "Preparatoria",
        }

        # Aplicar optimizaciones
        for original, corregido in optimizaciones_d2.items():
            patron = r"\b" + re.escape(original) + r"\b"
            texto_optimizado = re.sub(
                patron, corregido, texto_optimizado, flags=re.IGNORECASE
            )

        # Mejorar estructura de frases compactas
        texto_optimizado = self._expandir_frases_compactas(texto_optimizado)

        return texto_optimizado

    def _expandir_frases_compactas(self, texto: str) -> str:
        """Expandir frases típicas del estilo compacto"""
        expansiones = {
            "NO CONEXION": "No se pudo establecer conexión",
            "AMBOS DESPACHOS EN AUDIENCIA": "Ambos despachos estaban ocupados en otras audiencias",
            "PROBLEMAS DE CONEXION": "Se presentaron problemas técnicos de conexión",
            "ERROR EN LA GENDA": "Error en la agenda del despacho",
            "SE REPROGRAMA": "Se reprogramó la audiencia",
            "SERA REPROGRAMADA": "La audiencia será reprogramada",
            "SE FIJO": "Se fijó nueva fecha",
            "SIN NOVEDAD": "Se realizó sin novedad",
            "SE EFECTUO": "Se efectuó correctamente",
        }

        texto_expandido = texto
        for compacta, expandida in expansiones.items():
            texto_expandido = re.sub(
                re.escape(compacta), expandida, texto_expandido, flags=re.IGNORECASE
            )

        return texto_expandido

    def extract_audiencia_info(self, texto: str) -> Dict[str, Any]:
        """
        Extrae información de audiencias usando OpenAI GPT con mejora continua.
        MEJORADO: Incluye preprocesamiento basado en datos reales.

        Args:
            texto (str): Texto libre con información de la audiencia

        Returns:
            Dict con los campos extraídos para el formulario
        """

        try:
            # NUEVO: Preprocesar texto con mejoras identificadas
            texto_preprocesado = self._preprocesar_texto(texto)

            # Procesar con OpenAI mejorado
            resultado = self._extract_with_openai_enhanced(texto_preprocesado)

            # NUEVO: Post-procesar resultado para mejoras adicionales
            resultado = self._postprocesar_resultado(resultado, texto)

            print("AI: Procesado con OpenAI mejorado")
            return resultado

        except Exception as e:
            print(f"ERROR OpenAI: {e}")
            # Devolver estructura vacía en caso de error
            return {
                "radicado": "",
                "tipo_audiencia": "",
                "fecha": "",
                "hora": "",
                "minuto": "",
                "juzgado": "",
                "se_realizo": "",
                "motivos": "",
                "observaciones": f"Error al procesar: {str(e)}",
            }

    def _formatear_nombres_inteligente(self, texto: str) -> str:
        """
        Formatea nombres propios con lógica inteligente:
        - Minúsculas → Capitalizar (jose bustamante → Jose Bustamante)
        - Mayúsculas → Mantener (JOSE BUSTAMANTE → JOSE BUSTAMANTE)
        - Mixto con mayúsculas → Convertir a mayúsculas (JOSE bustamante → JOSE BUSTAMANTE)
        """
        if not texto or not isinstance(texto, str):
            return texto

        texto = texto.strip()
        if not texto:
            return texto

        # Dividir en palabras para análisis
        palabras = texto.split()
        if not palabras:
            return texto

        # Contar palabras en diferentes formatos
        palabras_minusculas = sum(1 for p in palabras if p.islower() and p.isalpha())
        palabras_mayusculas = sum(1 for p in palabras if p.isupper() and p.isalpha())
        palabras_mixtas = sum(
            1 for p in palabras if not p.islower() and not p.isupper() and p.isalpha()
        )
        palabras_alfabeticas = sum(1 for p in palabras if p.isalpha())

        # Si no hay palabras alfabéticas, devolver original
        if palabras_alfabeticas == 0:
            return texto

        # LÓGICA DE FORMATEO:

        # 1. Si tiene palabras en mayúsculas (completas), convertir todo a mayúsculas
        if palabras_mayusculas > 0:
            return " ".join(p.upper() if p.isalpha() else p for p in palabras)

        # 2. Si todo está en minúsculas, capitalizar cada palabra
        elif palabras_minusculas == palabras_alfabeticas:
            return " ".join(p.capitalize() if p.isalpha() else p for p in palabras)

        # 3. Si hay mezcla (incluye mixtas), convertir todo a mayúsculas
        elif palabras_mixtas > 0:
            return " ".join(p.upper() if p.isalpha() else p for p in palabras)

        # 4. Fallback: capitalizar
        else:
            return " ".join(p.capitalize() if p.isalpha() else p for p in palabras)

    def _aplicar_formateo_nombres_en_resultado(
        self, resultado: Dict[str, Any]
    ) -> Dict[str, Any]:
        """
        Aplica formateo inteligente de nombres a los campos relevantes del resultado.
        """
        resultado_formateado = resultado.copy()

        # Campos que pueden contener nombres propios
        # Formatear radicado (puede contener nombres)
        if resultado_formateado.get("radicado"):
            radicado = resultado_formateado["radicado"]
            # Si el radicado contiene un guión, formatear solo la parte del nombre
            if " - " in radicado:
                partes = radicado.split(" - ", 1)
                nombre_formateado = self._formatear_nombres_inteligente(partes[0])
                resultado_formateado["radicado"] = f"{nombre_formateado} - {partes[1]}"
            else:
                # Si no hay guión pero parece contener un nombre, formatearlo
                if any(char.isalpha() for char in radicado):
                    resultado_formateado["radicado"] = (
                        self._formatear_nombres_inteligente(radicado)
                    )

        return resultado_formateado

    def _postprocesar_resultado(
        self, resultado: Dict[str, Any], texto_original: str
    ) -> Dict[str, Any]:
        """
        Post-procesar resultado aplicando solo mejoras básicas de formato.
        SIMPLIFICADO: Solo análisis contextual puro, sin lógica especial.
        """
        resultado_mejorado = resultado.copy()

        # 1. Aplicar formateo inteligente de nombres
        resultado_mejorado = self._aplicar_formateo_nombres_en_resultado(
            resultado_mejorado
        )

        # 2. Inferir motivos implícitos solo si no hay motivos y no se realizó
        if resultado_mejorado.get("se_realizo") == "NO" and not resultado_mejorado.get(
            "motivos"
        ):
            motivos_inferidos = self._inferir_motivos_implícitos(texto_original)
            if motivos_inferidos:
                resultado_mejorado["motivos"] = motivos_inferidos

        # 3. Mejorar observaciones con información relevante
        resultado_mejorado = self._mejorar_observaciones(
            resultado_mejorado, texto_original
        )

        # 4. Validar y corregir fechas a 2025
        fecha = resultado_mejorado.get("fecha", "")
        if fecha and "/" in fecha:
            partes = fecha.split("/")
            if len(partes) == 3 and partes[2] in ["2022", "2023", "2024"]:
                resultado_mejorado["fecha"] = f"{partes[0]}/{partes[1]}/2025"

        return resultado_mejorado

    def _mejorar_observaciones(
        self, resultado: Dict[str, Any], texto_original: str
    ) -> Dict[str, Any]:
        """
        Las observaciones ya están optimizadas por el prompt AI mejorado.
        Esta función ahora solo retorna el resultado sin procesamiento adicional.
        """
        # Usar únicamente las observaciones del resultado AI (sin procesamiento adicional)
        # Las observaciones ya están mejoradas por el prompt AI actualizado
        return resultado

    def _extraer_fechas_reprogramacion(self, texto: str) -> str:
        """
        Extrae fechas de reprogramación del texto.
        MEJORADO: Maneja múltiples formatos de fechas.
        """
        import re

        # Patrones de fechas de reprogramación (mejorados)
        patrones_fecha = [
            # Formato completo: "reprogramada para el día 26 de septiembre de 2025"
            r"reprogramada?\s+para\s+el?\s+d[íi]a\s+(\d{1,2}\s+de\s+\w+(?:\s+de\s+\d{4})?)",
            # Formato: "reprogramada para el 31 de julio"
            r"reprogramada?\s+para\s+el?\s+(\d{1,2}\s+de\s+\w+)",
            # Formato: "nueva fecha: 15/08/2025"
            r"nueva\s+fecha[:\s]+(\d{1,2}/\d{1,2}/\d{4})",
            # Formato: "se fijó para el 5 de septiembre"
            r"se\s+fij[óo]\s+.*?para.*?(\d{1,2}\s+de\s+\w+)",
            # Formato: "el 19 de agosto de 2025 a las 02:30 pm"
            r"(\d{1,2}\s+de\s+\w+\s+de\s+\d{4})\s+a\s+las\s+(\d{1,2}:\d{2})",
            # Formato: "el 5 SEPT" o "5 de septiembre"
            r"el\s+(\d{1,2}\s+(?:de\s+)?\w+)",
            # Formato: "26 de septiembre"
            r"(\d{1,2}\s+de\s+\w+)",
            # Formato: "31 de julio"
            r"(\d{1,2}\s+de\s+julio|agosto|septiembre|octubre|noviembre|diciembre|enero|febrero|marzo|abril|mayo|junio)",
            # Formato compacto: "EL 5 SEPT"
            r"el\s+(\d{1,2}\s+sept\w*)",
            # Formato abreviado: "26 septiembre"
            r"(\d{1,2}\s+(?:enero|febrero|marzo|abril|mayo|junio|julio|agosto|sept\w*|octubre|noviembre|diciembre))",
        ]

        fechas_encontradas = []
        texto_lower = texto.lower()

        for patron in patrones_fecha:
            matches = re.findall(patron, texto_lower, re.IGNORECASE)
            for match in matches:
                if isinstance(match, tuple):
                    # Si el patrón tiene grupos múltiples, tomar todos los no vacíos
                    fechas_encontradas.extend([m.strip() for m in match if m.strip()])
                else:
                    fechas_encontradas.append(match.strip())

        if fechas_encontradas:
            # Limpiar duplicados y formatear
            fechas_unicas = []
            for fecha in fechas_encontradas:
                if fecha and fecha not in fechas_unicas:
                    fechas_unicas.append(fecha)

            return ", ".join(fechas_unicas)

        return ""

    def guardar_correccion(
        self,
        texto_original: str,
        datos_ia: Dict,
        datos_corregidos: Dict,
        es_correcto: bool,
    ) -> bool:
        """
        Guarda una corrección del usuario para mejorar el sistema.

        Args:
            texto_original: Texto original procesado
            datos_ia: Datos extraídos por la IA
            datos_corregidos: Datos corregidos por el usuario
            es_correcto: Si la corrección es válida

        Returns:
            bool: True si se guardó correctamente
        """
        print("ADVERTENCIA: Sistema de entrenamiento no disponible")
        return False

    def obtener_reporte_precision(self) -> Dict:
        """Obtiene un reporte de precisión del sistema."""
        return {"error": "Sistema de entrenamiento no disponible"}

    def _extract_with_openai_enhanced(self, texto: str) -> Dict[str, Any]:
        """
        Extracción usando OpenAI GPT con ejemplos aprendidos.
        """

        prompt = f"""
Eres un experto en extraer información de audiencias judiciales colombianas.
Analiza el texto y extrae ÚNICAMENTE la información presente usando análisis contextual puro.

TEXTO A ANALIZAR: "{texto}"

EJEMPLOS DE REFERENCIA (casos reales con resultados EXACTOS esperados):

Ejemplo 1: "JUZGADO TERCERO PENAL DEL CIRCUITO AGUACHICA, 20011600119320190181400 ALEXIS ROJAS PAYARES Y OTRA. CONTINUACION JUICIO 9:30 A.M. MIERCOLES 30 de julio, fiscalía desiste de las pruebas y termina con la etapa probatoria, solcito aplazamiento para que sean escuchados los testigos de la defensa que fueron decretados en la audiencia preparatoria. -, se fija nueva fecha el día 26 DE NOVIEMBRE 11 AM Y 03 DICIEMBRE 11 AM."
Resultado EXACTO: radicado="20011600119320190181400 - ALEXIS ROJAS PAYARES", tipo_audiencia="Continuación de Juicio Oral", fecha="30/07/2025", hora="09:30", juzgado="", se_realizo="SI", motivos="", observaciones="Fiscalía desiste de las pruebas y termina la etapa probatoria. Se solicita aplazamiento para escuchar testigos de la defensa. Nueva fecha programada: 26 de noviembre a las 11:00 AM y 3 de diciembre a las 11:00 AM"

Ejemplo 2: "Tercero penal del circuito, 20228600119920190026800 DOSTIN RODRIGUEZ DONADO – JUICIO, 9:00 A.M. JUEVES 31 julio, fracasa por solicitud de la fiscalía, no fijan fecha, no se abre audio."
Resultado EXACTO: radicado="20228600119920190026800 - DOSTIN RODRIGUEZ DONADO", tipo_audiencia="Juicio Oral", fecha="31/07/2025", hora="09:00", juzgado="Juzgado Tercero Penal del Circuito", se_realizo="NO", motivos="Fiscalía", observaciones="La audiencia fracasa por solicitud de la fiscalía. No se programa nueva fecha y no se establece conexión de audio"

Ejemplo 3: "PROCESO: 20011600123220241115600 AUDIENCIA DESPACHO: Juzgado 002 Promiscuo Municipal de Aguachica 200114089002 AGUACHICA, VERIFICACION DE ALLANAMIENTO VS ROBINSON JAIME TARAZONA Y RIKELVER GONZALEZ VENEZO EXTORSIO, 19 de agosto 10:30 am, se realiza conexión, se acepta la aceptación de cargos del señor Robinson jaime Tarazona y fija fecha para lectura de sentencia el dia 05 de septiembre 2:30 para la lectura de la sentencia. en cuanto al usurio Rikelver se realizó audiencia preparatoria y se pidio como testigo al señor robinson, son decretadas por el juzgado todas las solicitudes."
Resultado EXACTO: radicado="20011600123220241115600 - ROBINSON JAIME TARAZONA Y RIKELVER GONZALEZ VENEZO EXTORSIO", tipo_audiencia="Verificación de Allanamiento", fecha="19/08/2025", hora="10:30", juzgado="Juzgado 002 Promiscuo Municipal de Aguachica", se_realizo="SI", motivos="", observaciones="Se realiza conexión exitosa. Se acepta la aceptación de cargos de Robinson Jaime Tarazona y se programa lectura de sentencia para el 5 de septiembre a las 2:30 PM. Para Rikelver se realizó audiencia preparatoria, se solicita a Robinson como testigo y el juzgado decreta todas las solicitudes"

Ejemplo 4: "31 DE JULIO DE 2025 A LAS 8:00 AM en el proceso penal que se sigue en contra de DEIBER FRANCO ALVERNIA, identificado con C.C. 1.092.670.947; por el delito de TRÁFICO, FABRICACIÓN O PORTE DE ESTUPEFACIENTES; con radicado No. 200116001232202200293-00, tercero penal del circuito, fiscalía termina con la etapa probatorio y dan paso a la defensa y solicito suspensión para ubicar los testigos, fijaron fecha para nueva audiencia 28 de octubre 3:30."
Resultado EXACTO: radicado="DEIBER FRANCO ALVERNIA - 200116001232202200293-00", tipo_audiencia="", fecha="31/07/2025", hora="08:00", juzgado="Juzgado tercero penal del circuito", se_realizo="SI", motivos="", observaciones="Fiscalía termina la etapa probatoria y se da paso a la defensa. Se solicita suspensión para ubicar testigos. Nueva audiencia programada para el 28 de octubre a las 3:30 PM"

Ejemplo 5: "CARLOS ANDRÉS CARVAJAL ESPITIA / NO ASISTIÓ No. DOCUMENTO 1.098.696.511,hurto agravado, rad. 20 011 600 1087 2022 00218 00, san alberto JPM, juiciooral, se instala la audiencia, se presentan alegatos de apertura y se inicia etapa probatoria, la fiscalia presenta al testigo carlos andres ciro hoya,policia que practico la captura en flagrancia al usuario, se interrogo y se contrainterrogatorio.La fiscalia solicita suspensión del proceso para traer los testigos que faltany que fueron decretados en la audiencia la audiencia concentrada y fecha nuevafecha para el 16 de octubre 4:30 de la tarde para continuación."
Resultado EXACTO: radicado="CARLOS ANDRÉS CARVAJAL ESPITIA - 20011600108720220021800", tipo_audiencia="Juicio Oral", fecha="", hora="", juzgado="Juzgado san alberto JPM", se_realizo="SI", motivos="", observaciones="El imputado no asistió. Se instala la audiencia, se presentan alegatos de apertura y se inicia etapa probatoria. Fiscalía presenta testigo Carlos Andrés Ciro Hoya (policía que practicó la captura en flagrancia), se realiza interrogatorio y contrainterrogatorio. Fiscalía solicita suspensión para traer testigos faltantes decretados en audiencia concentrada. Nueva fecha: 16 de octubre a las 4:30 PM"

Ejemplo 6: "05 de agosto 2025 a las 4:30 de la tarde, NO SE LLEVARÁ A CABO teniendo en cuenta que el titular de la Fiscalía 10 Local de San Alberto - Cesar, solicitó elaplazamiento de esta, JPM SA, CUI. 200116001087201900283, Giovanny Amaya Galvis, aplicación PO"
Resultado EXACTO: radicado="200116001087201900283 - Giovanny Amaya Galvis", tipo_audiencia="Aplicación de Principio de oportunidad", fecha="05/08/2025", hora="16:30", juzgado="JPM SA", se_realizo="NO", motivos="Fiscalía", observaciones="La audiencia no se lleva a cabo debido a que la Fiscalía 10 Local de San Alberto - Cesar solicitó el aplazamiento"

Ejemplo 7: "J3PCA. 20011600119220210005800 JOSE ANTONIO GUTIERREZ MORENO - PPL - CONTINUACION JUICIO-. 10:00 A.M. MIERCOLES 6, fracasa por conexión del procesado"
Resultado EXACTO: radicado="20011600119220210005800 - JOSE ANTONIO GUTIERREZ MORENO", tipo_audiencia="Continuación de Juicio Oral", fecha="06/08/2025", hora="10:00", juzgado="J3PCA", se_realizo="NO", motivos="Usuario", observaciones="La audiencia fracasa por problemas de conexión del procesado"

Ejemplo 8: "2PCA, AUDIENCIA DE INSTALACIÓN DE JUICIO ORAL para el día 06 DE AGOSTO DE 2025 A LAS 03:00 PM en el proceso penal que se sigue en contra de FABIAN CHINCHILLA CONTRERAS ,identificado con C.C. 1.065.893.074; por el delito de FEMINICIDIO AGRAVADO TENTADO; con radicado No. 200116001232202200297-00, solicito se aplace la audiencia a fin celebrar preacuerdo, fijan fecha para el 01 de octubre 2:00 PM."
Resultado EXACTO: radicado="200116001232202200297-00 - FABIAN CHINCHILLA CONTRERAS", tipo_audiencia="Instalación de Juicio Oral", fecha="06/08/2025", hora="15:00", juzgado="2PCA", se_realizo="NO", motivos="Defensor público", observaciones="Se solicita aplazamiento para celebrar preacuerdo. Nueva fecha programada: 1 de octubre a las 2:00 PM"

Ejemplo 9: "El dia 19 de agosto a las 8:00 de la mañana, 20011600123220160216300 AUDIENCIA DESPACHO: Juzgado 002 Promiscuo Municipal de Aguachica , AGUACHICA JHON CARLOS SANCHEZ ORTEGA, por el delito de INASISTENCIA ALIMENTARIA juicio oral, fracasa por conexión del juzgado."
Resultado EXACTO: radicado="20011600123220160216300 - JHON CARLOS SANCHEZ ORTEGA", tipo_audiencia="Juicio Oral", fecha="19/08/2025", hora="08:00", juzgado="Juzgado 002 Promiscuo Municipal de Aguachica", se_realizo="NO", motivos="Juez", observaciones="La audiencia fracasa por problemas de conexión del juzgado"

Ejemplo 10: "J1PMA, proceso 20011600123220241115600, AUDIENCIA DE VERIFICACION DE ALLANAMIENTO, 19 de agosto 10:30 am, ROBINSON JAIME TARAZONA, se realiza conexión exitosa"
Resultado EXACTO: radicado="20011600123220241115600 - ROBINSON JAIME TARAZONA", tipo_audiencia="Verificación de Allanamiento", fecha="19/08/2025", hora="10:30", juzgado="J1PMA", se_realizo="SI", motivos="", observaciones="Se realiza conexión exitosa"

REGLAS CRÍTICAS PARA 100% PRECISIÓN:

1. RADICADO - FORMATO ESTRICTO:
   - SIEMPRE formato "NÚMERO - NOMBRES" (nunca "NOMBRES - NÚMERO")
   - "20011600119320190181400 ALEXIS ROJAS PAYARES Y OTRA" → "20011600119320190181400 - ALEXIS ROJAS PAYARES"
   - "DOSTIN RODRIGUEZ DONADO - 20228600119920190026800" → "20228600119920190026800 - DOSTIN RODRIGUEZ DONADO"
   - "Giovanny Amaya Galvis - 200116001087201900283" → "200116001087201900283 - Giovanny Amaya Galvis"
   - Mantener EXACTAMENTE la capitalización original de los nombres
   - "DOSTIN RODRIGUEZ DONADO" → "DOSTIN RODRIGUEZ DONADO" (mantener mayúsculas)
   - "Giovanny Amaya Galvis" → "Giovanny Amaya Galvis" (mantener formato mixto)

   MÚLTIPLES NOMBRES - REGLAS ESPECÍFICAS:
   - SIEMPRE usar EXACTAMENTE los nombres del texto SIN sufijos como "Y OTRA", "Y OTROS"
   - Si hay múltiples nombres completos, incluir todos separados por "Y"
   - "ROBINSON JAIME TARAZONA Y RIKELVER GONZALEZ VENEZO EXTORSIO" → incluir todos los nombres completos
   - "ALEXIS ROJAS PAYARES Y OTRA" → usar solo "ALEXIS ROJAS PAYARES" (eliminar "Y OTRA")
   - "PEDRO LOPEZ Y MARIA GONZALEZ Y JOSE MARTINEZ" → incluir todos los nombres completos
   - Para nombres largos o múltiples, priorizar la completitud sobre la brevedad
   - Si un nombre parece incompleto o corrupto, usar solo la parte clara y completa

2. JUZGADO ESPECÍFICO:
   - Si el texto dice "JUZGADO TERCERO PENAL" al inicio → dejar VACÍO ""
   - Si dice "Tercero penal del circuito" en medio → "Juzgado Tercero Penal del Circuito"
   - Si menciona "Juzgado 002 Promiscuo" → mantener completo
   - "JPM SA" → mantener tal como está
   - "san alberto JPM" → "Juzgado san alberto JPM"
   - Si NO se menciona o está al inicio → dejar VACÍO ""

   ABREVIACIONES DE JUZGADOS:
   - J3PCA, J2PCA, J1PCA → mantener tal como está (ej: "J3PCA") o expandir (ej: "Juzgado 3PCA")
   - J1PMA, J2PMA, J3PMA → mantener tal como está (ej: "J1PMA") o expandir (ej: "Juzgado 1PMA")
   - La "J" inicial significa "Juzgado", puede mantenerse abreviado o expandirse
   - Ejemplos: "J3PCA" es válido como "J3PCA" o como "Juzgado 3PCA"
   - Ejemplos: "J1PMA" es válido como "J1PMA" o como "Juzgado 1PMA"

2.5. TIPO_AUDIENCIA - DETECCIÓN INTELIGENTE:
   - Buscar palabras clave específicas en el texto para identificar el tipo
   - "CONTINUACION JUICIO" → "Continuación de Juicio Oral"
   - "JUICIO ORAL" → "Juicio Oral"
   - "INSTALACIÓN DE JUICIO" → "Instalación de Juicio Oral"
   - "VERIFICACION DE ALLANAMIENTO" → "Verificación de Allanamiento"
   - "APLICACION PO" → "Aplicación de Principio de oportunidad"
   - "PREPARATORIA" → "Audiencia Preparatoria"
   - "ACUSACION" → "Audiencia de Acusación"
   - "LECTURA DE SENTENCIA" → "Lectura de Sentencia"
   - "IMPOSICION DE PENA" → "Imposición de Pena"
   - "ALEGATOS" → "Alegatos de Conclusión"
   - Si NO hay indicadores claros → dejar VACÍO ""
   - Priorizar la exactitud sobre la inferencia
   - Si el texto es ambiguo, es mejor dejarlo vacío que adivinar

3. SE REALIZÓ - REGLAS ESTRICTAS:
   - "fiscalía desiste" + "termina etapa probatoria" → SIEMPRE "SI" (la audiencia se desarrolló, aunque hubo aplazamiento)
   - "se instala la audiencia" → SIEMPRE "SI"
   - "se realiza conexión" → SIEMPRE "SI"
   - "fiscalía termina etapa probatoria" → SIEMPRE "SI"
   - "termina con la etapa probatoria" → SIEMPRE "SI"
   - "NO SE LLEVARÁ A CABO" → SIEMPRE "NO"
   - "fracasa por solicitud" → SIEMPRE "NO"
   - "fracasa por conexión" → SIEMPRE "NO" (no hubo actuación judicial)
   - "solicito se aplace" SIN instalación previa → "NO"
   - "solicito aplazamiento" SIN instalación previa → "NO"
   - "solicito suspensión" CON desarrollo previo → "SI"

4. MOTIVOS - REGLAS ESTRICTAS:
   - SI se_realizo="SI" → motivos="" (SIEMPRE VACÍO)
   - SI se_realizo="NO" → motivos debe ser UNO de estos valores EXACTOS:
     * "Juez" (cuando falla por juzgado, conexión del juzgado)
     * "Fiscalía" (cuando falla por fiscalía, solicitud de fiscalía)
     * "Usuario" (cuando falla por procesado, imputado, conexión del procesado)
     * "Defensor público" (cuando falla por defensor, solicitud de aplazamiento sin especificar)
     * "Defensor Confianza"
     * "Víctima"
     * "Inpec"
     * "ICBF"

4. OBSERVACIONES MEJORADAS:
   - Redactar de manera profesional y clara basándose en el contenido
   - Sintetizar información relevante sin copiar textualmente
   - Incluir fechas de reprogramación de manera organizada
   - Corregir errores ortográficos y mejorar gramática
   - NO agregar información que no esté en el texto original
   - Ejemplo: "solcito aplazamiento" → "Se solicita aplazamiento"

CRÍTICO: Seguir EXACTAMENTE los ejemplos. NO inventar ni modificar información.

Responde ÚNICAMENTE con JSON válido:
{{
    "radicado": "NÚMERO - NOMBRES",
    "tipo_audiencia": "Tipo específico o vacío",
    "fecha": "DD/MM/YYYY o vacío",
    "hora": "HH:MM o vacío",
    "juzgado": "Juzgado completo o vacío",
    "se_realizo": "SI o NO",
    "motivos": "Motivo si NO se realizó o vacío",
    "observaciones": "Información relevante del texto"
}}

RESPONDE SOLO EL JSON, NADA MÁS.
"""

        # OPTIMIZACIÓN EXTREMA: Verificar caché persistente primero
        if self.persistent_cache:
            cached_result = self.persistent_cache.get(
                "extract_audiencia", texto, self.model
            )
            if cached_result:
                print("Cache hit persistente - Respuesta instantanea")
                return cached_result

        # Fallback: caché en memoria (para compatibilidad)
        texto_hash = hash(texto)
        if texto_hash in self._cache_resultados:
            print("CACHE: Usando resultado en cache memoria")
            return self._cache_resultados[texto_hash]

        try:
            response = self.client.chat.completions.create(
                model=self.model,
                messages=[
                    {
                        "role": "system",
                        "content": "Eres un experto en extraer datos de documentos judiciales. Responde SOLO con JSON válido, sin explicaciones.",
                    },
                    {"role": "user", "content": prompt},
                ],
                max_tokens=1000,  # Más tokens para casos complejos
                temperature=0,  # Máxima precisión
            )

            resultado_texto = response.choices[0].message.content
            if resultado_texto:
                resultado_texto = resultado_texto.strip()
            else:
                raise Exception("Respuesta vacía de OpenAI")

            # Limpiar posibles caracteres markdown
            if resultado_texto.startswith("```json"):
                resultado_texto = resultado_texto[7:]
            if resultado_texto.endswith("```"):
                resultado_texto = resultado_texto[:-3]

            resultado = json.loads(resultado_texto)

            # Validar y normalizar resultado
            campos_requeridos = [
                "radicado",
                "tipo_audiencia",
                "fecha",
                "hora",
                "minuto",
                "juzgado",
                "se_realizo",
                "motivos",
                "observaciones",
            ]

            for campo in campos_requeridos:
                if campo not in resultado:
                    resultado[campo] = ""

            # OPTIMIZACIÓN EXTREMA: Guardar en caché persistente
            if self.persistent_cache:
                self.persistent_cache.set(
                    "extract_audiencia", texto, resultado, self.model
                )

            # Guardar en caché memoria (para compatibilidad)
            self._cache_resultados[texto_hash] = resultado

            return resultado

        except json.JSONDecodeError as e:
            print(f"ERROR parsing JSON de OpenAI: {e}")
            raise Exception(f"Error en formato JSON: {e}")
        except Exception as e:
            print(f"ERROR OpenAI: {e}")
            raise Exception(f"Error de OpenAI: {e}")

    def dividir_audiencias_masivo(self, texto: str) -> List[str]:
        """
        Divide un texto largo en secciones individuales de audiencias.
        Para procesamiento masivo de documentos.
        VERSION MEJORADA: Con caché persistente y parser robusto.
        """
        # OPTIMIZACIÓN EXTREMA: Verificar caché persistente primero
        if self.persistent_cache:
            cached_result = self.persistent_cache.get(
                "divide_audiencias", texto, self.model
            )
            if cached_result:
                print("Cache hit persistente para division - Respuesta instantanea")
                return cached_result.get("secciones", [])

        # MEJORA 1: Sanitizar texto de entrada
        texto_sanitizado = self._sanitizar_texto_para_json(texto)

        # MEJORA 2: Límite optimizado para GPT-4 (2025)
        if len(texto_sanitizado) > 16000:  # Límite aumentado para aprovechar GPT-4
            print(
                f"ADVERTENCIA: Texto muy largo ({len(texto_sanitizado)} chars), dividiendo en chunks..."
            )
            return self._dividir_por_chunks(texto_sanitizado)

        prompt = f"""
Divide este texto en secciones separadas, donde cada sección corresponde a UNA audiencia diferente.

TEXTO COMPLETO:
{texto_sanitizado}

Identifica los límites entre audiencias buscando:
- Nuevos números de radicado
- Nuevas fechas de audiencia
- Cambios de juzgado
- Separadores como líneas, numeración, etc.

Responde SOLO con JSON array válido, sin markdown ni explicaciones:

["audiencia 1 texto completo", "audiencia 2 texto completo", "audiencia 3 texto completo"]

IMPORTANTE:
- Incluye TODA la información relevante de cada audiencia
- No resumas ni acortes el texto
- Si no puedes dividir claramente, devuelve el texto completo en un solo elemento
"""

        try:
            response = self.client.chat.completions.create(
                model=self.model,
                messages=[
                    {
                        "role": "system",
                        "content": "Eres experto en dividir documentos judiciales. Responde SOLO con JSON array válido, sin explicaciones.",
                    },
                    {"role": "user", "content": prompt},
                ],
                max_tokens=2000,
                temperature=0,
            )

            resultado_texto = response.choices[0].message.content
            if not resultado_texto:
                return [texto]  # Fallback si no hay respuesta

            # MEJORA 3: Parser JSON robusto con múltiples fallbacks
            secciones = self._parsear_json_robusto(resultado_texto, texto)

            if isinstance(secciones, list) and len(secciones) > 0:
                # OPTIMIZACIÓN EXTREMA: Guardar división exitosa en caché persistente
                if self.persistent_cache:
                    cache_data = {"secciones": secciones, "total": len(secciones)}
                    self.persistent_cache.set(
                        "divide_audiencias", texto, cache_data, self.model
                    )
                    print(f"Division cacheada: {len(secciones)} secciones")

                return secciones
            else:
                print("ADVERTENCIA: No se pudo dividir, devolviendo texto original")
                return [texto]  # Fallback si no es array válido

        except Exception as e:
            print(f"ERROR dividiendo audiencias: {e}")
            return [texto]  # Fallback a texto original

    def _sanitizar_texto_para_json(self, texto: str) -> str:
        """Sanitiza texto para evitar problemas JSON."""
        import re

        # Remover caracteres de control
        texto = re.sub(r"[\x00-\x1f\x7f-\x9f]", " ", texto)

        # Escapar comillas dobles problemáticas
        texto = texto.replace('"', '\\"')
        texto = texto.replace("'", "\\'")

        # Normalizar espacios en blanco
        texto = re.sub(r"\s+", " ", texto)
        texto = texto.strip()

        return texto

    def _dividir_por_chunks(self, texto: str) -> List[str]:
        """Divide texto muy largo en chunks manejables."""
        chunks = []
        chunk_size = 4000  # Tamaño seguro por chunk

        # Buscar puntos naturales de división
        separadores = ["═══", "AUDIENCIA", "RADICADO:", "───", "\n\n"]

        inicio = 0
        while inicio < len(texto):
            fin = inicio + chunk_size

            if fin >= len(texto):
                # Último chunk
                chunks.append(texto[inicio:])
                break

            # Buscar punto de división natural cerca del límite
            punto_division = fin
            for sep in separadores:
                pos = texto.find(sep, fin - 500, fin + 500)
                if pos != -1:
                    punto_division = pos
                    break

            chunks.append(texto[inicio:punto_division])
            inicio = punto_division

        # Procesar cada chunk individualmente
        todas_secciones = []
        for i, chunk in enumerate(chunks):
            print(f"  📄 Procesando chunk {i+1}/{len(chunks)}...")
            try:
                secciones_chunk = self.dividir_audiencias_masivo(chunk)
                todas_secciones.extend(secciones_chunk)
            except Exception as e:
                print(f"  ADVERTENCIA: Error en chunk {i+1}: {e}")
                todas_secciones.append(chunk)  # Agregar chunk original si falla

        return todas_secciones

    def _parsear_json_robusto(self, respuesta: str, texto_original: str) -> List[str]:
        """Parser JSON robusto con múltiples estrategias de fallback."""
        import re

        # Estrategia 1: JSON directo
        try:
            respuesta_limpia = respuesta.strip()

            # Remover markdown si existe
            if respuesta_limpia.startswith("```json"):
                respuesta_limpia = respuesta_limpia[7:]
            if respuesta_limpia.endswith("```"):
                respuesta_limpia = respuesta_limpia[:-3]

            respuesta_limpia = respuesta_limpia.strip()

            secciones = json.loads(respuesta_limpia)
            if isinstance(secciones, list):
                return secciones

        except json.JSONDecodeError as e:
            print(f"  ADVERTENCIA: JSON directo falló: {e}")

        # Estrategia 2: Buscar array JSON en el texto
        try:
            patron_json = r"\[.*?\]"
            matches = re.findall(patron_json, respuesta, re.DOTALL)

            for match in matches:
                try:
                    secciones = json.loads(match)
                    if isinstance(secciones, list) and len(secciones) > 0:
                        return secciones
                except:
                    continue

        except Exception as e:
            print(f"  ADVERTENCIA: Búsqueda de array falló: {e}")

        # Estrategia 3: Parsing manual de strings entre comillas
        try:
            patron_strings = r'"([^"]*)"'
            strings = re.findall(patron_strings, respuesta)

            if len(strings) > 0:
                # Filtrar strings que parezcan audiencias (tienen contenido mínimo)
                audiencias = [s for s in strings if len(s.strip()) > 50]
                if len(audiencias) > 0:
                    return audiencias

        except Exception as e:
            print(f"  ADVERTENCIA: Parsing manual falló: {e}")

        # Estrategia 4: División heurística básica
        try:
            return self._division_heuristica_fallback(texto_original)
        except Exception as e:
            print(f"  ADVERTENCIA: División heurística falló: {e}")

        # Último recurso: texto original
        return [texto_original]

    def _division_heuristica_fallback(self, texto: str) -> List[str]:
        """División heurística como último recurso."""
        import re

        # Buscar patrones comunes de separación
        patrones_separacion = [
            r"═{10,}.*?═{10,}",  # Líneas de igual
            r"AUDIENCIA\s+#?\d+",  # "AUDIENCIA #1", "AUDIENCIA 2", etc.
            r"RADICADO\s*:?\s*\d{4}-\d+",  # Patrones de radicado
            r"─{10,}",  # Líneas de guión
        ]

        secciones = []

        for patron in patrones_separacion:
            matches = list(re.finditer(patron, texto, re.IGNORECASE))

            if len(matches) >= 2:  # Al menos 2 matches para dividir
                inicio = 0

                for match in matches:
                    if match.start() > inicio:
                        seccion = texto[inicio : match.start()].strip()
                        if (
                            len(seccion) > 100
                        ):  # Solo secciones con contenido sustancial
                            secciones.append(seccion)
                    inicio = match.start()

                # Agregar última sección
                seccion_final = texto[inicio:].strip()
                if len(seccion_final) > 100:
                    secciones.append(seccion_final)

                if len(secciones) >= 2:
                    return secciones

        # Si no encontró divisiones claras, intentar por párrafos largos
        parrafos = texto.split("\n\n")
        parrafos_largos = [p.strip() for p in parrafos if len(p.strip()) > 200]

        if len(parrafos_largos) >= 2:
            return parrafos_largos

        # Si todo falla, devolver texto original
        return [texto]

    def procesar_audiencias_paralelo(
        self, secciones: List[str], progreso_callback=None
    ) -> List[Dict[str, Any]]:
        """
        MEJORA DE RENDIMIENTO: Procesa múltiples audiencias en paralelo.

        Args:
            secciones: Lista de textos de audiencias
            progreso_callback: Función para reportar progreso

        Returns:
            Lista de diccionarios con datos extraídos
        """
        import threading
        import queue
        import time

        if len(secciones) <= 1:
            # Para una sola sección, procesamiento normal
            if len(secciones) == 1:
                return [self.extract_audiencia_info(secciones[0])]
            return []

        print(f"PROCESAMIENTO: Paralelo de {len(secciones)} audiencias...")

        # Configurar threading
        resultados: List[Dict[str, Any]] = [{}] * len(secciones)
        errores = queue.Queue()

        def procesar_seccion(indice, texto_seccion):
            """Procesar una sección individual."""
            try:
                resultado = self.extract_audiencia_info(texto_seccion)
                resultados[indice] = resultado

                if progreso_callback:
                    progreso_callback(indice + 1, len(secciones))

            except Exception as e:
                errores.put((indice, str(e)))
                resultados[indice] = {
                    "radicado": "",
                    "tipo_audiencia": "",
                    "fecha": "",
                    "hora": "",
                    "minuto": "",
                    "juzgado": "",
                    "se_realizo": "",
                    "motivos": "",
                    "observaciones": f"Error en procesamiento paralelo: {str(e)}",
                }

        # Crear y lanzar threads
        threads = []
        max_threads = min(3, len(secciones))  # Máximo 3 threads paralelos

        inicio = time.time()

        for i in range(0, len(secciones), max_threads):
            batch = secciones[i : i + max_threads]
            batch_threads = []

            for j, seccion in enumerate(batch):
                thread = threading.Thread(
                    target=procesar_seccion, args=(i + j, seccion)
                )
                thread.start()
                batch_threads.append(thread)
                threads.append(thread)

            # Esperar a que termine este batch antes del siguiente
            for thread in batch_threads:
                thread.join()

            print(f"  OK: Batch {i//max_threads + 1} completado")

        fin = time.time()
        tiempo_total = fin - inicio

        # Reportar errores si los hay
        if not errores.empty():
            print("ADVERTENCIA: Errores en procesamiento paralelo:")
            while not errores.empty():
                indice, error = errores.get()
                print(f"  Sección {indice + 1}: {error}")

        print(f"🏁 Procesamiento paralelo completado en {tiempo_total:.2f}s")

        # Filtrar resultados vacíos
        resultados_validos = [r for r in resultados if r and r != {}]

        return resultados_validos

    def extract_audiencia_info_rapido(self, texto: str) -> Dict[str, Any]:
        """
        MEJORA DE RENDIMIENTO: Versión rápida con modelo más liviano.

        Args:
            texto (str): Texto de la audiencia

        Returns:
            Dict con los campos extraídos
        """
        if len(texto) < 200:
            # Para textos muy cortos, usar modelo rápido
            modelo_usar = self.model_rapido
        else:
            # Para textos normales, usar modelo estándar
            modelo_usar = self.model

        try:
            prompt = f"""
Extrae información EXACTA de esta audiencia judicial. Responde SOLO JSON válido:

TEXTO:
{texto[:2000]}  # Limitar texto para mayor velocidad

Responde con este formato JSON:
{{
    "radicado": "número exacto del radicado con nombres formateados",
    "tipo_audiencia": "tipo específico de audiencia",
    "fecha": "DD/MM/AAAA",
    "hora": "HH",
    "minuto": "MM",
    "juzgado": "nombre completo del juzgado",
    "se_realizo": "Si" o "No",
    "motivos": "motivos específicos de la audiencia",
    "observaciones": "observaciones relevantes",
}}

FORMATEO DE NOMBRES CRÍTICO:
- Si está en minúsculas: "jose bustamante" → "Jose Bustamante"
- Si está en MAYÚSCULAS: "JOSE BUSTAMANTE" → "JOSE BUSTAMANTE"
- Si es mixto: "JOSE bustamante" → "JOSE BUSTAMANTE"
"""

            response = self.client.chat.completions.create(
                model=modelo_usar,
                messages=[
                    {
                        "role": "system",
                        "content": "Extrae información judicial. Responde SOLO JSON válido, sin explicaciones.",
                    },
                    {"role": "user", "content": prompt},
                ],
                max_tokens=800,  # Reducido para mayor velocidad
                temperature=0,
            )

            resultado_texto = response.choices[0].message.content
            if not resultado_texto:
                return self._estructura_vacia()

            # Parser robusto también aquí
            resultado_limpio = resultado_texto.strip()

            if resultado_limpio.startswith("```json"):
                resultado_limpio = resultado_limpio[7:-3]

            datos = json.loads(resultado_limpio)

            # Validar estructura
            campos_requeridos = [
                "radicado",
                "tipo_audiencia",
                "fecha",
                "hora",
                "minuto",
                "juzgado",
                "se_realizo",
                "motivos",
                "observaciones",
            ]

            for campo in campos_requeridos:
                if campo not in datos:
                    datos[campo] = ""

            # Aplicar formateo inteligente de nombres
            datos = self._aplicar_formateo_nombres_en_resultado(datos)

            return datos

        except Exception as e:
            print(f"ERROR en extraccion rapida: {e}")
            return self._estructura_vacia()

    def _estructura_vacia(self) -> Dict[str, Any]:
        """Estructura vacía de datos de audiencia."""
        return {
            "radicado": "",
            "tipo_audiencia": "",
            "fecha": "",
            "hora": "",
            "juzgado": "",
            "se_realizo": "",
            "motivos": "",
            "observaciones": "",
        }

    def procesar_documento_completo_optimizado(
        self, texto: str, usar_paralelo=True, progreso_callback=None
    ) -> List[Dict[str, Any]]:
        """
        MEJORA COMPLETA: Procesa documento completo con todas las optimizaciones.

        Args:
            texto: Texto completo del documento
            usar_paralelo: Si usar procesamiento paralelo
            progreso_callback: Función para reportar progreso

        Returns:
            Lista de audiencias procesadas
        """
        print("PROCESAMIENTO OPTIMIZADO INICIADO")
        print("=" * 50)

        inicio_total = time.time()

        # Paso 1: División optimizada
        print(" Paso 1: Dividiendo documento...")
        inicio_division = time.time()

        secciones = self.dividir_audiencias_masivo(texto)

        fin_division = time.time()
        tiempo_division = fin_division - inicio_division

        print(
            f"OK: División completada en {tiempo_division:.2f}s - {len(secciones)} secciones"
        )

        if len(secciones) == 0:
            print("ERROR: No se encontraron secciones")
            return []

        # Paso 2: Procesamiento optimizado
        print(f"\n PASO 2: Procesando {len(secciones)} audiencias...")
        inicio_procesamiento = time.time()

        if usar_paralelo and len(secciones) > 1:
            resultados = self.procesar_audiencias_paralelo(secciones, progreso_callback)
        else:
            resultados = []
            for i, seccion in enumerate(secciones):
                resultado = self.extract_audiencia_info_rapido(seccion)
                resultados.append(resultado)

                if progreso_callback:
                    progreso_callback(i + 1, len(secciones))

                print(f"  OK: Audiencia {i+1}/{len(secciones)} procesada")

        fin_procesamiento = time.time()
        tiempo_procesamiento = fin_procesamiento - inicio_procesamiento

        print(f"OK: Procesamiento completado en {tiempo_procesamiento:.2f}s")

        # Paso 3: Validación y limpieza
        print(f"\n PASO 3: Validando resultados...")
        resultados_validos = []

        for i, resultado in enumerate(resultados):
            if resultado and isinstance(resultado, dict):
                # Validar que tenga datos mínimos
                tiene_radicado = resultado.get("radicado", "").strip() != ""
                tiene_fecha = resultado.get("fecha", "").strip() != ""

                if tiene_radicado or tiene_fecha:
                    resultados_validos.append(resultado)
                    print(f"  OK: Audiencia {i+1}: Válida")
                else:
                    print(f"  ADVERTENCIA: Audiencia {i+1}: Datos insuficientes")
            else:
                print(f"  ERROR: Audiencia {i+1}: Resultado inválido")

        fin_total = time.time()
        tiempo_total = fin_total - inicio_total

        # Resumen final
        print(f"\n PROCESAMIENTO COMPLETADO")
        print(f"TIEMPO: Tiempo total: {tiempo_total:.2f} segundos")
        print(
            f"ESTADISTICAS: Audiencias válidas: {len(resultados_validos)}/{len(secciones)}"
        )
        print(
            f"VELOCIDAD: Velocidad promedio: {tiempo_total/len(secciones):.2f}s por audiencia"
        )

        return resultados_validos

    def get_cache_stats(self) -> Dict[str, Any]:
        """
        NUEVA FUNCIONALIDAD: Obtiene estadísticas del caché persistente

        Returns:
            Diccionario con estadísticas de rendimiento del caché
        """
        if self.persistent_cache:
            stats = self.persistent_cache.get_stats()
            stats["cache_enabled"] = True
            return stats
        else:
            return {
                "cache_enabled": False,
                "message": "Caché persistente no disponible",
            }

    def clear_cache(self):
        """
        NUEVA FUNCIONALIDAD: Limpia todo el caché persistente
        """
        if self.persistent_cache:
            self.persistent_cache.clear_cache()
            print("CACHE: Caché persistente limpiado completamente")
        else:
            print("ADVERTENCIA: Caché persistente no disponible")

        # Limpiar también caché en memoria
        self._cache_resultados.clear()
        self._cache_abreviaciones.clear()
        print(" Caché en memoria limpiado")


# Instancia global del servicio
ai_service = AIFormFiller()
