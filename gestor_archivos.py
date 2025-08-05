#!/usr/bin/env python3
"""
Gestor de Archivos para Gestor de Audiencias
Maneja la creación, selección y gestión de archivos Excel.
Consolidado y optimizado - Versión única.
"""

import os
import sys
import shutil

def obtener_directorio_real():
    """
    Devuelve la carpeta donde realmente se encuentra el .exe o .py
    Corregido para PyInstaller con recursos embebidos
    """
    if getattr(sys, 'frozen', False):
        # Ejecutable PyInstaller - usar directorio del ejecutable para archivos de usuario
        return os.path.dirname(sys.executable)
    else:
        # Desarrollo - usar directorio del script
        return os.path.dirname(os.path.abspath(__file__))

def obtener_directorio_recursos():
    """
    Obtiene el directorio donde están los recursos embebidos (plantillas, etc.)
    """
    if hasattr(sys, '_MEIPASS'):
        # PyInstaller - recursos embebidos en directorio temporal
        return sys._MEIPASS
    else:
        # Desarrollo - usar directorio del script
        return os.path.dirname(os.path.abspath(__file__))

# Directorio base para archivos de usuario (donde está el .exe o desarrollo)
BASE_DIR = obtener_directorio_real()

# Directorio para recursos embebidos (plantillas, etc.)
RECURSOS_DIR = obtener_directorio_recursos()

# Rutas para archivos de usuario (escribibles) - siempre donde está el .exe
CARPETA_ARCHIVOS = os.path.join(BASE_DIR, "archivos_creados") 
CONFIG_DIR = os.path.join(BASE_DIR, "config")
DOCS_DIR = os.path.join(BASE_DIR, "docs")

# Rutas para recursos embebidos (solo lectura) - desde PyInstaller o desarrollo
TEMPLATES_DIR = os.path.join(RECURSOS_DIR, "templates")
plantilla_excel = os.path.join(TEMPLATES_DIR, "plantilla_audiencias.xlsx")

# Crear directorios de usuario si no existen (solo los escribibles)
os.makedirs(CARPETA_ARCHIVOS, exist_ok=True)

# Solo crear directorios de desarrollo si no estamos en ejecutable
if not getattr(sys, 'frozen', False):
    os.makedirs(TEMPLATES_DIR, exist_ok=True)
    os.makedirs(CONFIG_DIR, exist_ok=True)
    os.makedirs(DOCS_DIR, exist_ok=True)

def crear_carpeta_si_no_existe():
    """Crea la carpeta de archivos creados si no existe."""
    if not os.path.exists(CARPETA_ARCHIVOS):
        os.makedirs(CARPETA_ARCHIVOS)

def crear_copia_plantilla(nombre_archivo):
    """
    Crea una copia de la plantilla con el nombre especificado.
    Funciona tanto en desarrollo como en ejecutable con recursos embebidos.
    """
    crear_carpeta_si_no_existe()
    destino = os.path.join(CARPETA_ARCHIVOS, nombre_archivo)
    
    if os.path.exists(destino):
        raise FileExistsError("Ya existe un archivo con ese nombre.")
    
    # Verificar que la plantilla embebida existe
    if not os.path.exists(plantilla_excel):
        raise FileNotFoundError(f"No se pudo encontrar la plantilla Excel en: {plantilla_excel}")
    
    # Copiar la plantilla embebida al destino
    shutil.copy2(plantilla_excel, destino)
    
    print(f"✅ Plantilla copiada desde: {plantilla_excel}")
    print(f"✅ Archivo creado en: {destino}")
    
    return destino

def listar_archivos_creados():
    """Retorna una lista de archivos Excel en la carpeta de archivos creados."""
    crear_carpeta_si_no_existe()
    return [f for f in os.listdir(CARPETA_ARCHIVOS) if f.endswith(".xlsx")]

def seleccionar_archivo(nombre_archivo):
    """Retorna la ruta completa del archivo seleccionado."""
    ruta = os.path.join(CARPETA_ARCHIVOS, nombre_archivo)
    if not os.path.exists(ruta):
        raise FileNotFoundError("Archivo no encontrado.")
    return ruta

def eliminar_archivo(nombre_archivo):
    """Elimina un archivo de la carpeta de archivos creados."""
    ruta = os.path.join(CARPETA_ARCHIVOS, nombre_archivo)
    if os.path.exists(ruta):
        os.remove(ruta)
        return True
    return False

def descargar_archivo(nombre_archivo):
    """Copia el archivo a la carpeta de Descargas del usuario."""
    ruta = os.path.join(CARPETA_ARCHIVOS, nombre_archivo)
    if not os.path.exists(ruta):
        raise FileNotFoundError("Archivo no encontrado.")
    
    # Usar la carpeta de Descargas del usuario
    carpeta_descargas = os.path.join(os.path.expanduser("~"), "Downloads")
    destino = os.path.join(carpeta_descargas, nombre_archivo)
    
    # Si ya existe, añadir un número
    contador = 1
    base_nombre, extension = os.path.splitext(nombre_archivo)
    while os.path.exists(destino):
        nuevo_nombre = f"{base_nombre}_{contador}{extension}"
        destino = os.path.join(carpeta_descargas, nuevo_nombre)
        contador += 1
    
    shutil.copy2(ruta, destino)
    return destino

def obtener_ruta_plantilla():
    """Retorna la ruta de la plantilla Excel."""
    return plantilla_excel

def obtener_ruta_config():
    """Retorna la ruta del directorio de configuración."""
    return CONFIG_DIR

def obtener_ruta_docs():
    """Retorna la ruta del directorio de documentación."""
    return DOCS_DIR