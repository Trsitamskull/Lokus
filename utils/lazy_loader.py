"""
Lazy Loading Utilities para optimización de startup de la aplicación.

Este módulo proporciona herramientas para implementar carga diferida (lazy loading)
de componentes GUI pesados para mejorar significativamente el tiempo de inicio.

Autor: Jose David Bustamante Sánchez  
Año: 2025
"""

import asyncio
from typing import Callable, Any, Dict, Optional
from functools import wraps
import time
import threading


class LazyComponentLoader:
    """
    Gestor de carga diferida de componentes para optimizar el startup.
    
    Esta clase permite cargar componentes GUI de forma diferida cuando
    realmente se necesitan, mejorando dramáticamente el tiempo de inicio.
    """
    
    def __init__(self):
        self._loaded_components: Dict[str, Any] = {}
        self._loading_tasks: Dict[str, threading.Thread] = {}
        self._component_factories: Dict[str, Callable] = {}
        self._startup_time = time.time()
        
    def register_component(self, name: str, factory: Callable, priority: str = "normal"):
        """
        Registra un componente para carga diferida.
        
        Args:
            name: Identificador único del componente
            factory: Función que crea el componente
            priority: "critical", "high", "normal", "low"
        """
        self._component_factories[name] = {
            "factory": factory,
            "priority": priority,
            "loaded": False
        }
    
    def get_component(self, name: str, timeout: float = 5.0) -> Any:
        """
        Obtiene un componente, cargándolo si es necesario.
        
        Args:
            name: Nombre del componente
            timeout: Tiempo máximo de espera en segundos
            
        Returns:
            El componente solicitado
        """
        # Si ya está cargado, devolverlo inmediatamente
        if name in self._loaded_components:
            return self._loaded_components[name]
        
        # Si no está registrado, error
        if name not in self._component_factories:
            raise ValueError(f"Componente '{name}' no registrado")
        
        # Cargar de forma síncrona si no está en progreso
        if name not in self._loading_tasks:
            component_info = self._component_factories[name]
            print(f"[LAZY] Cargando componente: {name}")
            start_time = time.time()
            
            try:
                component = component_info["factory"]()
                self._loaded_components[name] = component
                component_info["loaded"] = True
                
                load_time = (time.time() - start_time) * 1000
                print(f"[LAZY] {name} cargado en {load_time:.1f}ms")
                
                return component
                
            except Exception as e:
                print(f"[LAZY] Error cargando {name}: {e}")
                raise
        
        # Si está en progreso, esperar
        thread = self._loading_tasks.get(name)
        if thread and thread.is_alive():
            thread.join(timeout=timeout)
            
        return self._loaded_components.get(name)
    
    def preload_component_async(self, name: str):
        """
        Inicia la carga de un componente en background.
        
        Args:
            name: Nombre del componente a precargar
        """
        if name in self._loaded_components or name in self._loading_tasks:
            return  # Ya cargado o en progreso
            
        if name not in self._component_factories:
            return  # No registrado
        
        def load_worker():
            try:
                component_info = self._component_factories[name]
                print(f"[LAZY] Precargando componente: {name}")
                start_time = time.time()
                
                component = component_info["factory"]()
                self._loaded_components[name] = component
                component_info["loaded"] = True
                
                load_time = (time.time() - start_time) * 1000
                print(f"[LAZY] {name} precargado en {load_time:.1f}ms")
                
            except Exception as e:
                print(f"[LAZY] Error precargando {name}: {e}")
            finally:
                # Limpiar el thread de la lista
                if name in self._loading_tasks:
                    del self._loading_tasks[name]
        
        # Iniciar en thread separado
        thread = threading.Thread(target=load_worker, daemon=True)
        self._loading_tasks[name] = thread
        thread.start()
    
    def preload_by_priority(self, max_priority: str = "high"):
        """
        Precarga componentes basado en prioridad.
        
        Args:
            max_priority: Máxima prioridad a precargar
        """
        priority_levels = {"critical": 4, "high": 3, "normal": 2, "low": 1}
        max_level = priority_levels.get(max_priority, 2)
        
        for name, info in self._component_factories.items():
            priority = info.get("priority", "normal")
            level = priority_levels.get(priority, 2)
            
            if level >= max_level and not info["loaded"]:
                self.preload_component_async(name)
    
    def get_load_statistics(self) -> Dict[str, Any]:
        """
        Obtiene estadísticas de carga de componentes.
        
        Returns:
            Diccionario con estadísticas de rendimiento
        """
        total_components = len(self._component_factories)
        loaded_components = len(self._loaded_components)
        loading_components = len(self._loading_tasks)
        startup_time = (time.time() - self._startup_time) * 1000
        
        return {
            "total_components": total_components,
            "loaded_components": loaded_components,
            "loading_components": loading_components,
            "load_percentage": (loaded_components / total_components * 100) if total_components > 0 else 0,
            "startup_time_ms": startup_time,
            "components_status": {
                name: {
                    "loaded": info["loaded"],
                    "priority": info["priority"]
                }
                for name, info in self._component_factories.items()
            }
        }


# Instancia global del lazy loader
lazy_loader = LazyComponentLoader()


def lazy_component(name: str, priority: str = "normal"):
    """
    Decorador para marcar una función como componente de carga diferida.
    
    Args:
        name: Nombre único del componente
        priority: Prioridad de carga
    """
    def decorator(factory_func: Callable) -> Callable:
        # Registrar el componente
        lazy_loader.register_component(name, factory_func, priority)
        
        @wraps(factory_func)
        def wrapper(*args, **kwargs):
            return lazy_loader.get_component(name)
        
        return wrapper
    
    return decorator


def preload_critical_components():
    """
    Precarga componentes críticos en background.
    Esta función debe llamarse después del startup inicial.
    """
    print("[LAZY] Iniciando precarga de componentes críticos...")
    lazy_loader.preload_by_priority("critical")


def preload_high_priority_components():
    """
    Precarga componentes de alta prioridad.
    Llamar cuando la interfaz básica esté lista.
    """
    print("[LAZY] Iniciando precarga de componentes de alta prioridad...")
    lazy_loader.preload_by_priority("high")


def get_lazy_stats() -> Dict[str, Any]:
    """
    Obtiene estadísticas de rendimiento del lazy loading.
    
    Returns:
        Estadísticas detalladas de carga
    """
    return lazy_loader.get_load_statistics()


# Funciones de utilidad para logging de rendimiento
class PerformanceTimer:
    """Context manager para medir tiempo de ejecución."""
    
    def __init__(self, operation_name: str):
        self.operation_name = operation_name
        self.start_time = None
    
    def __enter__(self):
        self.start_time = time.time()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        elapsed = (time.time() - self.start_time) * 1000
        print(f"[PERF] {self.operation_name}: {elapsed:.1f}ms")