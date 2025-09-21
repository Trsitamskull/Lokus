from services.ai_service import AIFormFiller
import json

def probar_ia_mejorada():
    ai_service = AIFormFiller()
    
    # Caso de prueba 2
    texto = 'Tercero penal del circuito, 20228600119920190026800 DOSTIN RODRIGUEZ DONADO – JUICIO, 9:00 A.M. JUEVES 31 julio, fracasa por solicitud de la fiscalía, no fijan fecha, no se abre audio.'
    
    print('PROBANDO IA MEJORADA - CASO 2:')
    print('Texto:', texto)
    print()
    
    resultado = ai_service.extract_audiencia_info(texto)
    
    print('RESULTADO:')
    print(json.dumps(resultado, indent=2, ensure_ascii=False))
    print()
    
    # Verificar campos específicos
    print('VERIFICACION:')
    print('Radicado esperado: 20228600119920190026800 - DOSTIN RODRIGUEZ DONADO')
    print('Radicado obtenido:', resultado.get('radicado'))
    print()
    print('Tipo esperado: Juicio Oral')
    print('Tipo obtenido:', resultado.get('tipo_audiencia'))
    print()
    print('Hora esperada: 09:00')
    print('Hora obtenida:', resultado.get('hora'))
    print()
    print('Juzgado esperado: Juzgado Tercero Penal del Circuito')
    print('Juzgado obtenido:', resultado.get('juzgado'))

if __name__ == "__main__":
    probar_ia_mejorada()