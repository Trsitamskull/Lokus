import sys
import os
sys.path.append(os.getcwd())

from services.ai_service import AIFormFiller

def test_ia():
    ai = AIFormFiller()
    
    # Texto de ejemplo
    texto = """Radicado: 20228600119920190026800
Acusado: DOSTIN RODRIGUEZ DONADO
Tipo de audiencia: JUICIO ORAL
Fecha: 17 de agosto de 2022
Hora: 09:00 am
Juzgado: Juzgado Tercero Penal del Circuito de Especializado de Valledupar
Estado: NO se realizó
Motivo: FISCALIA
Observaciones: Audiencia programada pero no realizada por problemas de la Fiscalía"""
    
    # Probar la IA
    print("Probando IA...")
    resultado = ai.extract_audiencia_info(texto)
    
    print("\nResultado:")
    for key, value in resultado.items():
        print(f"{key}: {value}")

if __name__ == "__main__":
    test_ia()