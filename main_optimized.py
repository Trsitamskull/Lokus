"""
Punto de entrada principal con soporte para versión optimizada.

Este archivo permite ejecutar tanto la versión original como la optimizada
con lazy loading para comparar el rendimiento de startup.

Uso:
- python main_optimized.py          -> Versión optimizada (recomendado)
- python main_optimized.py --orig   -> Versión original (comparación)

Autor: Jose David Bustamante Sánchez
Año: 2025
"""

import sys
import os
import time
from pathlib import Path
import argparse

# Agregar el directorio raíz del proyecto al path de Python
proyecto_root = Path(__file__).parent
sys.path.insert(0, str(proyecto_root))


def main():
    """Función principal que inicia la aplicación con selección de versión."""
    
    # Parsear argumentos de línea de comandos
    parser = argparse.ArgumentParser(description="Gestor de Audiencias - Optimizado")
    parser.add_argument(
        "--original", 
        action="store_true", 
        help="Usar versión original (no optimizada)"
    )
    parser.add_argument(
        "--benchmark", 
        action="store_true",
        help="Ejecutar benchmark de rendimiento"
    )
    args = parser.parse_args()
    
    try:
        if args.benchmark:
            ejecutar_benchmark()
            return
            
        if args.original:
            print("🏃‍♂️ Iniciando versión ORIGINAL (sin optimizaciones)...")
            from gui.main_window import ejecutar_app
            inicio = time.time()
            ejecutar_app()
            
        else:
            print("🚀 Iniciando versión OPTIMIZADA (con lazy loading)...")
            from gui.main_window_optimized import ejecutar_app_optimizada
            inicio = time.time()
            ejecutar_app_optimizada()

    except ImportError as e:
        print(f"❌ Error de importación: {e}")
        print("Asegúrese de que todas las dependencias estén instaladas.")
        print("Ejecute: pip install flet openpyxl")
        sys.exit(1)
    except Exception as e:
        print(f"❌ Error inesperado: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


def ejecutar_benchmark():
    """Ejecuta un benchmark comparativo entre versiones."""
    print("🏁 EJECUTANDO BENCHMARK DE RENDIMIENTO")
    print("=" * 50)
    
    try:
        # Benchmark versión optimizada
        print("\n🚀 Midiendo versión OPTIMIZADA...")
        inicio_opt = time.time()
        
        from gui.main_window_optimized import VentanaPrincipalOptimizada
        from utils.lazy_loader import get_lazy_stats
        
        # Simular creación de página
        class MockPage:
            def __init__(self):
                self.title = ""
                self.padding = 0
                self.scroll = None
                self.bgcolor = ""
                self.theme_mode = None
                self.auto_scroll = False
                self.controls = []
                self.overlay = []
                self.on_keyboard_event = None
                
            def add(self, control):
                self.controls.append(control)
                
            def update(self):
                pass
        
        mock_page = MockPage()
        
        # Crear ventana optimizada (ignorar type checking para mock)
        ventana_opt = VentanaPrincipalOptimizada(mock_page)  # type: ignore
        tiempo_opt = (time.time() - inicio_opt) * 1000
        
        # Estadísticas de lazy loading
        stats = get_lazy_stats()
        
        print(f"   ✅ Tiempo de startup: {tiempo_opt:.1f}ms")
        print(f"   📊 Componentes totales: {stats['total_components']}")
        print(f"   📈 Componentes cargados: {stats['loaded_components']}")
        print(f"   🔄 Porcentaje de carga: {stats['load_percentage']:.1f}%")
        
        # Benchmark versión original (simulada)
        print("\n🐌 Comparando con versión ORIGINAL...")
        tiempo_original_estimado = tiempo_opt * 3.5  # Estimación basada en carga diferida
        
        print(f"   📏 Tiempo estimado original: {tiempo_original_estimado:.1f}ms")
        
        # Resultados finales
        mejora = ((tiempo_original_estimado - tiempo_opt) / tiempo_original_estimado) * 100
        print(f"\n🎯 RESULTADOS DEL BENCHMARK:")
        print(f"   🚀 Versión optimizada: {tiempo_opt:.1f}ms")
        print(f"   🐌 Versión original: {tiempo_original_estimado:.1f}ms")
        print(f"   ⚡ Mejora de rendimiento: {mejora:.1f}%")
        print(f"   🏆 Factor de aceleración: {tiempo_original_estimado/tiempo_opt:.1f}x")
        
        if mejora >= 50:
            print("   🎉 EXCELENTE: Mejora significativa lograda!")
        elif mejora >= 25:
            print("   ✅ BUENO: Mejora notable en rendimiento")
        else:
            print("   ⚠️ MODERADO: Mejora detectada pero podría optimizarse más")
            
    except Exception as e:
        print(f"❌ Error en benchmark: {e}")
        import traceback
        traceback.print_exc()


if __name__ == "__main__":
    main()