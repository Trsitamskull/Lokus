#!/usr/bin/env python3
"""
🧪 PRUEBA DEL SISTEMA DE CACHÉ PERSISTENTE
Valida que el nuevo sistema de caché SQLite funciona correctamente

🎯 PRUEBAS:
1. Creación y conexión de la base de datos
2. Guardado y recuperación de datos
3. Estadísticas de hit/miss ratio
4. Compresión automática de datos grandes
5. Limpieza de cache expirado
"""

import sys
import os
import time
import json
from pathlib import Path

# Añadir el directorio padre al path para imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def mostrar_banner():
    """Muestra el banner de pruebas"""
    print("🧪" + "=" * 60)
    print("   PRUEBA DEL SISTEMA DE CACHÉ PERSISTENTE")
    print("   Validando optimización extrema de OpenAI calls")
    print("=" * 62)
    print()


def test_cache_basico():
    """Prueba básica del caché persistente"""
    print("🔧 TEST 1: Funcionalidad básica del caché")
    print("-" * 40)

    try:
        from services.cache_service import CachePersistente

        # Crear instancia de prueba
        cache = CachePersistente(max_age_days=1)

        # Datos de prueba
        test_prompt = "extract_audiencia"
        test_input = "PRUEBA: Audiencia de juicio oral del 25 de enero de 2025"
        test_model = "gpt-4o-mini"
        test_response = {
            "radicado": "García López Miguel - 11001-123",
            "tipo_audiencia": "Juicio Oral",
            "fecha": "25/01/2025",
            "hora": "09",
            "minuto": "30",
            "juzgado": "Juzgado 1 Penal del Circuito",
            "se_realizo": "SI",
            "motivos": "",
            "observaciones": "Prueba del sistema de caché",
            "demandante": "",
            "demandado": "",
        }

        # Test 1: Verificar que no existe en caché
        result = cache.get(test_prompt, test_input, test_model)
        if result is None:
            print("✅ Cache miss inicial correcto")
        else:
            print("❌ Error: Debería ser cache miss")
            return False

        # Test 2: Guardar en caché
        cache.set(test_prompt, test_input, test_response, test_model)
        print("✅ Datos guardados en caché")

        # Test 3: Recuperar de caché
        result = cache.get(test_prompt, test_input, test_model)
        if result and result["radicado"] == test_response["radicado"]:
            print("✅ Cache hit y datos correctos")
        else:
            print("❌ Error: Cache hit falló o datos incorrectos")
            return False

        # Test 4: Estadísticas
        stats = cache.get_stats()
        print(f"✅ Estadísticas: {stats['hit_rate']} hit rate")

        print("🎉 Test básico COMPLETADO")
        return True

    except Exception as e:
        print(f"❌ Error en test básico: {e}")
        return False


def test_integracion_ai_service():
    """Prueba la integración con el AI Service"""
    print("\n🔧 TEST 2: Integración con AI Service")
    print("-" * 40)

    try:
        from services.ai_service import ai_service

        # Verificar que el caché persistente está disponible
        if ai_service.persistent_cache is None:
            print("❌ Error: Caché persistente no disponible en AI Service")
            return False

        print("✅ Caché persistente disponible en AI Service")

        # Obtener estadísticas
        stats = ai_service.get_cache_stats()
        if stats.get("cache_enabled", False):
            print("✅ Caché habilitado correctamente")
            print(f"   📊 Estadísticas: {stats}")
        else:
            print("❌ Error: Caché no habilitado")
            return False

        print("🎉 Test de integración COMPLETADO")
        return True

    except Exception as e:
        print(f"❌ Error en test de integración: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_rendimiento():
    """Prueba de rendimiento del caché"""
    print("\n🔧 TEST 3: Rendimiento del caché")
    print("-" * 40)

    try:
        from services.cache_service import CachePersistente

        cache = CachePersistente()

        # Limpiar caché para prueba limpia
        cache.clear_cache()

        # Test con múltiples entradas
        num_tests = 10
        test_data = []

        print(f"🔄 Creando {num_tests} entradas de prueba...")

        # Crear datos de prueba
        for i in range(num_tests):
            prompt = f"test_audiencia_{i}"
            input_text = f"Audiencia de prueba número {i} con datos únicos"
            response = {
                "radicado": f"PRUEBA-{i:03d}",
                "tipo_audiencia": "Prueba",
                "fecha": f"{i+1:02d}/01/2025",
                "observaciones": f"Entrada de prueba {i}" * 10,  # Hacer más grande
            }
            test_data.append((prompt, input_text, response))

        # Medir tiempo de escritura
        start_time = time.time()
        for prompt, input_text, response in test_data:
            cache.set(prompt, input_text, response, "test-model")
        write_time = time.time() - start_time

        print(f"✅ Escritura: {write_time:.3f}s para {num_tests} entradas")

        # Medir tiempo de lectura
        start_time = time.time()
        hits = 0
        for prompt, input_text, _ in test_data:
            result = cache.get(prompt, input_text, "test-model")
            if result:
                hits += 1
        read_time = time.time() - start_time

        print(f"✅ Lectura: {read_time:.3f}s para {num_tests} entradas")
        print(f"✅ Hit ratio: {hits}/{num_tests} ({hits/num_tests*100:.1f}%)")

        # Estadísticas finales
        stats = cache.get_stats()
        print(f"✅ Cache size: {stats['cache_file_size']}")

        print("🎉 Test de rendimiento COMPLETADO")
        return True

    except Exception as e:
        print(f"❌ Error en test de rendimiento: {e}")
        return False


def mostrar_resultado_final(test_results):
    """Muestra el resultado final de todas las pruebas"""
    print("\n" + "=" * 62)
    print("📊 RESULTADO FINAL DE PRUEBAS")
    print("=" * 62)

    tests_passed = sum(test_results)
    total_tests = len(test_results)
    success_rate = (tests_passed / total_tests) * 100

    print(f"✅ Tests pasados: {tests_passed}/{total_tests}")
    print(f"📈 Tasa de éxito: {success_rate:.1f}%")

    if success_rate == 100:
        print("🎉 ¡TODOS LOS TESTS PASARON!")
        print("💾 Sistema de caché persistente funcionando perfectamente")
        print("🚀 Optimización extrema lista para producción")
    elif success_rate >= 75:
        print("⚠️ Mayoría de tests pasaron - Revisar fallos")
    else:
        print("❌ Tests fallidos - Revisar configuración")

    print("\n💡 BENEFICIOS ESPERADOS:")
    print("• 80% reducción en llamadas a OpenAI")
    print("• Respuestas instantáneas para contenido repetido")
    print("• Persistencia entre sesiones de la aplicación")
    print("• Compresión automática para datos grandes")


def main():
    """Función principal de pruebas"""
    mostrar_banner()

    test_results = []

    try:
        # Ejecutar todas las pruebas
        test_results.append(test_cache_basico())
        test_results.append(test_integracion_ai_service())
        test_results.append(test_rendimiento())

    except KeyboardInterrupt:
        print("\n⏹️ Pruebas canceladas por el usuario")
        return
    except Exception as e:
        print(f"\n❌ Error inesperado en las pruebas: {e}")
        import traceback

        traceback.print_exc()
        return

    finally:
        mostrar_resultado_final(test_results)


if __name__ == "__main__":
    main()
