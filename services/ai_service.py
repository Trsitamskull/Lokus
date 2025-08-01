"""
Servicio de IA para autocompletar formularios de audiencias.
Utiliza OpenAI GPT para extraer información de texto libre directamente.
Sistema optimizado para máxima precisión en extracción.
"""

import json
import re
from typing import Dict, Optional, Any
from datetime import datetime

try:
    import openai
    from openai import OpenAI
    OPENAI_AVAILABLE = True
except ImportError:
    OPENAI_AVAILABLE = False
    print("❌ Error: OpenAI no está instalado. Ejecuta: pip install openai")

try:
    from config.config import OPENAI_API_KEY, OPENAI_MODEL
    HAS_CONFIG = True
except ImportError:
    print("❌ Error: config.py no encontrado. Crea el archivo config/config.py con tu API key.")
    OPENAI_API_KEY = None
    OPENAI_MODEL = "gpt-4o-mini"
    HAS_CONFIG = False


class AIFormFiller:
    """Servicio para autocompletar formularios usando OpenAI exclusivamente."""
    
    def __init__(self, api_key: Optional[str] = None):
        self.api_key = api_key or OPENAI_API_KEY
        
        if not OPENAI_AVAILABLE:
            raise Exception("OpenAI no está disponible. Instala con: pip install openai")
        
        if not self.api_key:
            raise Exception("API Key de OpenAI requerida. Configura config.py")
        
        # Inicializar cliente OpenAI
        self.client = OpenAI(api_key=self.api_key)
        self.model = OPENAI_MODEL
        print(f"🤖 OpenAI configurado correctamente - Modelo: {self.model}")
    
    def extract_audiencia_info(self, texto: str) -> Dict[str, Any]:
        """
        Extrae información de audiencias usando OpenAI GPT directamente.
        
        Args:
            texto (str): Texto libre con información de la audiencia
            
        Returns:
            Dict con los campos extraídos para el formulario
        """
        
        try:
            # Procesar directamente con OpenAI
            resultado = self._extract_with_openai(texto)
            
            print("🤖 Procesado directamente con OpenAI")
            return resultado
            
        except Exception as e:
            print(f"❌ Error OpenAI: {e}")
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
                "demandante": "",
                "demandado": ""
            }
    
    def _extract_with_openai(self, texto: str) -> Dict[str, Any]:
        """
        Extracción usando OpenAI GPT - Modo de alta precisión.
        """
        
        prompt = f"""
Eres un experto en extraer información de documentos judiciales colombianos.

TEXTO A ANALIZAR:
"{texto}"

Extrae ÚNICAMENTE la información que encuentres en el texto y responde con JSON válido:

{{
    "radicado": "NOMBRE_PERSONA - NÚMERO_RADICADO (si hay ambos) o solo lo que encuentres",
    "tipo_audiencia": "tipo exacto de audiencia encontrado",
    "fecha": "DD/MM/YYYY",
    "hora": "HH",
    "minuto": "MM",
    "juzgado": "nombre del juzgado encontrado",
    "se_realizo": "SI o NO según el texto",
    "motivos": "motivo principal si NO se realizó",
    "observaciones": "información adicional relevante",
    "demandante": "",
    "demandado": ""
}}

REGLAS CRÍTICAS:
1. RADICADO: 
   - Busca números largos (10+ dígitos) juntos o con guiones: "2024-00123-45" o "200016001075201602652"
   - Busca nombres de personas ANYWHERE en el texto: "GARCÍA LÓPEZ MIGUEL ÁNGEL", "LÓPEZ TORRES, CARLOS EDUARDO"
   - Si hay nombre Y radicado: "NOMBRE_COMPLETO - RADICADO"
   - Si solo hay uno: pon solo ese
   - IMPORTANTE: Nombres pueden estar con etiquetas como "(procesado)", "(imputado)", "identificado con cédula"

2. FECHAS EN TEXTO:
   - "veintidós de mayo de 2025" → "22/05/2025"
   - "28 de febrero de 2025" → "28/02/2025"
   - "miércoles 15 de abril" → busca el año y convierte

3. HORAS:
   - "tres y media de la tarde" → hora:"15", minuto:"30"
   - "11:45" → hora:"11", minuto:"45"
   - "2:30 PM" → hora:"14", minuto:"30"
   - "8:30 AM" → hora:"08", minuto:"30"

4. SE_REALIZO:
   - "se realizó", "se llevó a cabo", "exitosamente" → "SI"
   - "NO se llevó a cabo", "suspendida", "no compareció" → "NO"
   - Si hay contradicción, usar el resultado FINAL

5. MOTIVOS (SOLO si NO se realizó):
   - "inasistencia del defensor público" → "defensor público"
   - "falta del Juez", "ausencia del juez" → "juez"
   - "fiscalía no compareció" → "fiscalía"
   - "problemas técnicos del Usuario" → "usuario"
   - "INPEC reportó fuga" → "inpec"
   - IMPORTANTE: Los motivos NO son observaciones. Solo pon códigos simples.

6. OBSERVACIONES:
   - Información adicional importante: reprogramaciones, cambios, detalles técnicos
   - Si hay cambio de tipo de audiencia, mencionalo: "Originalmente era X, ahora es Y"
   - Si hay traslados, confirmaciones del INPEC, nuevas fechas, etc.
   - NO pongas aquí los motivos de suspensión (van en "motivos")

7. Si no encuentras algo, déjalo vacío ""

EJEMPLOS DE EXTRACCIÓN:
- "GARCÍA LÓPEZ MIGUEL ÁNGEL (procesado). Radicado: 11001-123" → radicado: "GARCÍA LÓPEZ MIGUEL ÁNGEL - 11001-123"
- "proceso contra MARTÍNEZ SILVA, ANA - número 2024-456" → radicado: "MARTÍNEZ SILVA, ANA - 2024-456"
- "28 de febrero de 2025 a las tres y media de la tarde" → fecha: "28/02/2025", hora: "15", minuto: "30"

RESPONDE SOLO EL JSON, NADA MÁS.
"""
        
        try:
            response = self.client.chat.completions.create(
                model=self.model,
                messages=[
                    {"role": "system", "content": "Eres un experto en extraer datos de documentos judiciales. Responde SOLO con JSON válido, sin explicaciones."},
                    {"role": "user", "content": prompt}
                ],
                max_tokens=1000,  # Más tokens para casos complejos
                temperature=0  # Máxima precisión
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
            campos_requeridos = ["radicado", "tipo_audiencia", "fecha", "hora", "minuto", 
                               "juzgado", "se_realizo", "motivos", "observaciones", 
                               "demandante", "demandado"]
            
            for campo in campos_requeridos:
                if campo not in resultado:
                    resultado[campo] = ""
            
            return resultado
            
        except json.JSONDecodeError as e:
            print(f"❌ Error parsing JSON de OpenAI: {e}")
            raise Exception(f"Error en formato JSON: {e}")
        except Exception as e:
            print(f"❌ Error OpenAI: {e}")
            raise Exception(f"Error de OpenAI: {e}")


# Instancia global del servicio
ai_service = AIFormFiller()
