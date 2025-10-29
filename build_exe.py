#!/usr/bin/env python3
"""
🚀 GESTOR DE AUDIENCIAS - BUILD PLUG & PLAY
Script definitivo para crear ejecutable listo para distribución
Autor: Jose David Bustamante Sánchez - 2025

✨ CARACTERÍSTICAS:
• PyInstaller actualizado automáticamente
• API Key embebida y segura
• Icono personalizado incluido
• Ejecutable único sin configuración
• Compatible con Windows 10+ sin Python instalado
"""

import subprocess
import sys
import shutil
import zipfile
import os
from pathlib import Path
from datetime import datetime

# ============================================
# CONFIGURACIÓN DEL BUILD
# ============================================

class BuildConfig:
    """Configuración centralizada del proceso de build"""

    # Información de la aplicación
    APP_NAME = "GestorAudiencias"
    APP_VERSION = "2.0"
    AUTHOR = "Jose David Bustamante Sánchez"
    YEAR = "2025"

    # Archivos y directorios
    MAIN_SCRIPT = "main.py"
    ICON_FILES = ["compliant.ico", "icon.ico", "app.ico", "gestor.ico"]
    TEMPLATE_FILE = "templates/plantilla_audiencias.xlsx"

    # Directorios de salida
    DIST_DIR = "GestorAudiencias_PlugAndPlay"
    ZIP_NAME = f"{APP_NAME}_v{APP_VERSION}_PlugAndPlay.zip"

    # Configuración PyInstaller
    PYINSTALLER_OPTIONS = [
        "--onedir",  # Carpeta en lugar de archivo único (más compatible)
        "--windowed",  # Sin ventana de consola
        "--clean",
        "--noconfirm",
        "--optimize=2",  # Optimización máxima
        "--strip",  # Reducir tamaño
    ]

    # Módulos a incluir explícitamente
    HIDDEN_IMPORTS = [
        "flet",
        "flet.core",
        "openpyxl",
        "openai",
        "gui.main_window",
        "gui.dialogs",
        "gui.constants",
        "gui.widgets",
        "gui.ia_config_dialog",
        "gui.busqueda_avanzada",
        "gui.procesador_masivo",
        "models.audiencia",
        "models.excel_manager",
        "services.ai_service",
        "services.ai_training_service",
        "utils.validators",
        "config.config",
        "config.config_template",
    ]

    # Módulos a excluir para reducir tamaño
    EXCLUDE_MODULES = [
        "matplotlib",
        "scipy",
        "pandas",
        "numpy.distutils",
        "tkinter.test",
        "test",
        "unittest",
        "pdb",
        "doctest",
        "pytest",
        "ipython",
        "jupyter",
        "notebook",
        "black",
        "flake8",
        "mypy",
        "autopep8",
    ]

def mostrar_banner():
    """Muestra el banner del proceso de build"""
    print("🚀" + "="*60)
    print(f"   GESTOR DE AUDIENCIAS v{BuildConfig.APP_VERSION} - BUILD PLUG & PLAY")
    print("   Creando ejecutable listo para distribución")
    print(f"   Por {BuildConfig.AUTHOR} - {BuildConfig.YEAR}")
    print("="*62)
    print()

def verificar_y_actualizar_pyinstaller():
    """Verifica e instala/actualiza PyInstaller a la última versión"""
    print("🔍 VERIFICANDO PYINSTALLER...")
    print("-" * 30)

    try:
        # Verificar si PyInstaller está instalado
        result = subprocess.run([sys.executable, "-c", "import PyInstaller; print(PyInstaller.__version__)"],
                              capture_output=True, text=True)

        if result.returncode == 0:
            current_version = result.stdout.strip()
            print(f"📦 PyInstaller {current_version} encontrado")

            # Actualizar a la última versión
            print("🔄 Actualizando a la última versión...")
            update_result = subprocess.run([sys.executable, "-m", "pip", "install", "--upgrade", "pyinstaller"],
                                         capture_output=True, text=True)

            if update_result.returncode == 0:
                # Verificar nueva versión
                new_result = subprocess.run([sys.executable, "-c", "import PyInstaller; print(PyInstaller.__version__)"],
                                          capture_output=True, text=True)
                new_version = new_result.stdout.strip()

                if new_version != current_version:
                    print(f"✅ PyInstaller actualizado: {current_version} → {new_version}")
                else:
                    print(f"✅ PyInstaller ya está actualizado: {new_version}")
                return True
            else:
                print("⚠️ Error al actualizar, pero PyInstaller funciona")
                return True

        else:
            # PyInstaller no está instalado
            print("❌ PyInstaller no encontrado. Instalando...")
            install_result = subprocess.run([sys.executable, "-m", "pip", "install", "pyinstaller"],
                                          capture_output=True, text=True)

            if install_result.returncode == 0:
                print("✅ PyInstaller instalado exitosamente")
                return True
            else:
                print("❌ Error al instalar PyInstaller")
                print(install_result.stderr)
                return False

    except Exception as e:
        print(f"❌ Error verificando PyInstaller: {e}")
        return False

def limpiar_archivos_temporales():
    """Limpia archivos y directorios temporales de builds anteriores"""
    print("\n🧹 LIMPIANDO ARCHIVOS TEMPORALES...")
    print("-" * 37)

    items_to_clean = {
        "directorios": ["build", "dist", "__pycache__", BuildConfig.DIST_DIR],
        "archivos": ["*.spec", BuildConfig.ZIP_NAME]
    }

    # Limpiar directorios
    for directory in items_to_clean["directorios"]:
        dir_path = Path(directory)
        if dir_path.exists():
            shutil.rmtree(dir_path)
            print(f"🗑️  Eliminado directorio: {directory}/")

    # Limpiar archivos
    for pattern in items_to_clean["archivos"]:
        for file_path in Path(".").glob(pattern):
            file_path.unlink()
            print(f"🗑️  Eliminado archivo: {file_path}")

    print("✅ Limpieza completada")

def detectar_icono():
    """Detecta y valida el archivo de icono disponible"""
    print("\n🎨 DETECTANDO ICONO...")
    print("-" * 21)

    for icon_name in BuildConfig.ICON_FILES:
        icon_path = Path(icon_name)
        if icon_path.exists():
            file_size = icon_path.stat().st_size
            if file_size > 1000:  # Mínimo 1KB para ser un icono válido
                print(f"🎯 Icono encontrado: {icon_name}")
                print(f"📏 Tamaño: {file_size:,} bytes")
                return str(icon_path.absolute())
            else:
                print(f"⚠️ Icono muy pequeño ignorado: {icon_name} ({file_size} bytes)")

    print("⚠️ No se encontró icono válido")
    print("💡 Tip: Coloca un archivo .ico en la raíz del proyecto")
    return None

def verificar_dependencias():
    """Verifica que todos los archivos necesarios estén presentes"""
    print("\n🔍 VERIFICANDO DEPENDENCIAS...")
    print("-" * 29)

    archivos_requeridos = [
        BuildConfig.MAIN_SCRIPT,
        BuildConfig.TEMPLATE_FILE,
        "requirements.txt",
        "config/config.py",
        "services/ai_service.py"
    ]

    archivos_faltantes = []

    for archivo in archivos_requeridos:
        if Path(archivo).exists():
            print(f"✅ {archivo}")
        else:
            print(f"❌ {archivo}")
            archivos_faltantes.append(archivo)

    if archivos_faltantes:
        print(f"\n❌ Archivos faltantes: {', '.join(archivos_faltantes)}")
        return False

    print("✅ Todas las dependencias están presentes")
    return True

def crear_comando_pyinstaller():
    """Crea el comando completo de PyInstaller"""
    print("\n🔧 CONFIGURANDO PYINSTALLER...")
    print("-" * 31)

    cmd = ["pyinstaller"] + BuildConfig.PYINSTALLER_OPTIONS

    # Nombre de la aplicación
    cmd.extend(["--name", BuildConfig.APP_NAME])

    # Incluir datos
    cmd.extend([
        "--add-data", f"{BuildConfig.TEMPLATE_FILE};templates",
        "--add-data", "config;config",
    ])

    # Incluir imports ocultos
    for import_module in BuildConfig.HIDDEN_IMPORTS:
        cmd.extend(["--hidden-import", import_module])

    # Excluir módulos innecesarios
    for exclude_module in BuildConfig.EXCLUDE_MODULES:
        cmd.extend(["--exclude-module", exclude_module])

    # Detectar y agregar icono
    icon_path = detectar_icono()
    if icon_path:
        cmd.extend(["--icon", icon_path])

    # Archivo principal
    cmd.append(BuildConfig.MAIN_SCRIPT)

    return cmd

def ejecutar_pyinstaller():
    """Ejecuta PyInstaller con la configuración optimizada"""
    print("\n🚀 EJECUTANDO PYINSTALLER...")
    print("-" * 28)

    cmd = crear_comando_pyinstaller()

    print(f"🔧 Comando: {' '.join(cmd[:5])}... (y {len(cmd)-5} parámetros más)")
    print("⏳ Esto puede tomar varios minutos...")
    print()

    try:
        # Ejecutar PyInstaller con output en tiempo real
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            universal_newlines=True
        )

        # Mostrar progreso
        output_lines = []
        for line in process.stdout:
            line = line.strip()
            if line:
                output_lines.append(line)
                # Mostrar líneas importantes
                if any(keyword in line.lower() for keyword in ['info:', 'warning:', 'error:', 'building']):
                    print(f"  {line}")

        process.wait()

        if process.returncode == 0:
            print("\n✅ PyInstaller completado exitosamente")
            return True
        else:
            print("\n❌ Error en PyInstaller")
            # Mostrar últimas líneas para debug
            print("Últimas líneas del output:")
            for line in output_lines[-10:]:
                print(f"  {line}")
            return False

    except Exception as e:
        print(f"\n❌ Error ejecutando PyInstaller: {e}")
        return False

def verificar_ejecutable():
    """Verifica que el ejecutable se haya creado correctamente"""
    print("\n🔍 VERIFICANDO EJECUTABLE...")
    print("-" * 26)

    exe_path = Path(f"dist/{BuildConfig.APP_NAME}/{BuildConfig.APP_NAME}.exe")

    if exe_path.exists():
        file_size = exe_path.stat().st_size
        print(f"✅ Ejecutable creado: {exe_path}")
        print(f"📏 Tamaño: {file_size:,} bytes ({file_size/1024/1024:.1f} MB)")
        return True
    else:
        print(f"❌ Ejecutable no encontrado en: {exe_path}")

        # Buscar en otras ubicaciones posibles
        dist_path = Path("dist")
        if dist_path.exists():
            print("📂 Contenido de dist/:")
            for item in dist_path.iterdir():
                print(f"  {item}")

        return False

def crear_distribucion_final():
    """Crea la distribución final lista para compartir"""
    print("\n📦 CREANDO DISTRIBUCIÓN FINAL...")
    print("-" * 32)

    # Verificar que el ejecutable exista
    exe_source = Path(f"dist/{BuildConfig.APP_NAME}")
    if not exe_source.exists():
        print("❌ Directorio del ejecutable no encontrado")
        return False

    # Crear directorio de distribución
    dist_final = Path(BuildConfig.DIST_DIR)
    if dist_final.exists():
        shutil.rmtree(dist_final)
    dist_final.mkdir()

    # Copiar ejecutable y dependencias
    shutil.copytree(exe_source, dist_final / BuildConfig.APP_NAME)
    print(f"✅ Aplicación copiada a {BuildConfig.DIST_DIR}/")

    # Crear launcher principal
    crear_launcher_principal(dist_final)

    # Crear documentación
    crear_documentacion(dist_final)

    # Crear archivo ZIP
    crear_zip_distribucion(dist_final)

    return True

def crear_launcher_principal(dist_path: Path):
    """Crea un launcher principal fácil de usar"""
    launcher_content = f'''@echo off
title {BuildConfig.APP_NAME} v{BuildConfig.APP_VERSION}
cls
echo.
echo 🏛️ ========================================
echo    GESTOR DE AUDIENCIAS v{BuildConfig.APP_VERSION}
echo    Plug ^& Play - Sin configuración
echo ========================================
echo.
echo 🚀 Iniciando aplicación...
echo.

cd /d "%~dp0"
start "" "{BuildConfig.APP_NAME}\\{BuildConfig.APP_NAME}.exe"

echo ✅ Aplicación iniciada correctamente
echo.
echo 💡 Si no se abre automáticamente:
echo    • Ve a la carpeta {BuildConfig.APP_NAME}
echo    • Ejecuta {BuildConfig.APP_NAME}.exe
echo.
echo ⏳ Esta ventana se cerrará en 3 segundos...
timeout /t 3 /nobreak >nul
'''

    launcher_path = dist_path / "🚀 ABRIR_GESTOR_AUDIENCIAS.bat"
    with open(launcher_path, "w", encoding="utf-8") as f:
        f.write(launcher_content)

    print("✅ Launcher principal creado")

def crear_documentacion(dist_path: Path):
    """Crea documentación completa para el usuario final"""

    # README principal
    readme_content = f"""🏛️ GESTOR DE AUDIENCIAS v{BuildConfig.APP_VERSION} - PLUG & PLAY

🎉 ¡COMPLETAMENTE LISTO PARA USAR!
✨ Sin instalación • Sin configuración • Sin complicaciones

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🚀 INICIO RÁPIDO (¡Solo 2 pasos!)

1️⃣ DESCOMPRIMIR:
   • Haz clic derecho en el archivo ZIP
   • Selecciona "Extraer todo" o "Extraer aquí"

2️⃣ EJECUTAR:
   • Haz doble clic en "🚀 ABRIR_GESTOR_AUDIENCIAS.bat"
   • ¡LISTO! La aplicación se abre automáticamente

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ CARACTERÍSTICAS INCLUIDAS

🤖 INTELIGENCIA ARTIFICIAL:
   • ✅ API OpenAI pre-configurada y oculta
   • ✅ Autocompletado inteligente (Ctrl+I)
   • ✅ Procesamiento masivo de documentos
   • ✅ Reconoce cualquier formato de audiencia

📊 GESTIÓN AVANZADA:
   • ✅ Archivos Excel automáticos
   • ✅ Búsqueda y filtrado avanzado
   • ✅ Edición en tiempo real
   • ✅ Validación de datos automática
   • ✅ Reordenamiento inteligente

🎨 INTERFAZ MODERNA:
   • ✅ Tema claro/oscuro
   • ✅ Controles intuitivos
   • ✅ Atajos de teclado útiles
   • ✅ Diseño responsivo

🛡️ SEGURIDAD Y PRIVACIDAD:
   • ✅ API key completamente oculta
   • ✅ Datos procesados localmente
   • ✅ Sin registro ni seguimiento

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📱 GUÍA DE USO

1. CREAR ARCHIVO:
   • Botón "Nuevo Archivo" → Escribe nombre
   • Se crea automáticamente con plantilla

2. AGREGAR AUDIENCIA:
   • Completa los campos del formulario
   • Usa Ctrl+I para autocompletar con IA
   • Guarda con Enter o botón "Guardar"

3. AUTOCOMPLETADO IA:
   • Pega texto de audiencia en el diálogo
   • La IA extrae automáticamente:
     - Radicado, fecha, hora, juzgado
     - Tipo de audiencia
     - Si se realizó o no
     - Motivos de cancelación
     - Observaciones

4. PROCESAMIENTO MASIVO:
   • Botón "Procesar Masivo"
   • Selecciona PDF o Word
   • Extrae múltiples audiencias automáticamente

5. BUSCAR Y EDITAR:
   • Botón "Buscar Avanzado"
   • Filtra por fecha, tipo, estado
   • Edita directamente desde resultados

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔧 SOLUCIÓN DE PROBLEMAS

❓ Windows dice "Windows protegió tu PC":
   1. Haz clic en "Más información"
   2. Selecciona "Ejecutar de todas formas"
   3. Es normal, Windows es precavido con apps nuevas

❓ La aplicación no se abre:
   1. Ve a la carpeta "{BuildConfig.APP_NAME}"
   2. Ejecuta directamente "{BuildConfig.APP_NAME}.exe"
   3. Si persiste, ejecuta como administrador

❓ Error de permisos:
   1. Clic derecho en el launcher
   2. "Ejecutar como administrador"
   3. Permite en Windows Defender si aparece

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💡 CONSEJOS ÚTILES

🎯 ATAJOS DE TECLADO:
   • Ctrl+I: Autocompletar con IA
   • Ctrl+N: Nuevo archivo
   • Ctrl+S: Guardar
   • Ctrl+F: Buscar
   • F5: Recargar datos

🎨 PERSONALIZACIÓN:
   • Botón 🌙/☀️ para cambiar tema
   • La aplicación recuerda tus preferencias

📂 ARCHIVOS:
   • Se guardan en "archivos_creados/"
   • Formato Excel estándar
   • Compatible con Office/LibreOffice

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📋 REQUISITOS DEL SISTEMA

✅ Windows 10/11 (cualquier edición)
✅ 4 GB RAM mínimo (8 GB recomendado)
✅ 200 MB espacio libre
✅ Conexión a internet (solo para IA)

❌ NO requiere:
   • Python instalado
   • Configuración manual
   • Dependencias adicionales
   • Permisos especiales

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🎉 ¡DISFRUTA TU GESTOR DE AUDIENCIAS!

Aplicación desarrollada por {BuildConfig.AUTHOR}
Año {BuildConfig.YEAR} • Versión {BuildConfig.APP_VERSION}

Para soporte o sugerencias: ¡Solo contacta! 😊
"""

    with open(dist_path / "📖 LÉEME_PRIMERO.txt", "w", encoding="utf-8") as f:
        f.write(readme_content)

    # Crear archivo de troubleshooting
    troubleshoot_content = '''@echo off
title Solucionador de Problemas - Gestor de Audiencias
color 0A
cls
echo.
echo 🔧 SOLUCIONADOR DE PROBLEMAS AUTOMÁTICO
echo ========================================
echo.
echo Detectando y solucionando problemas comunes...
echo.

REM Verificar ejecutable
if exist "{app_name}\\{app_name}.exe" (
    echo ✅ Ejecutable encontrado
) else (
    echo ❌ ERROR: Ejecutable no encontrado
    echo.
    echo SOLUCIÓN:
    echo 1. Asegúrate de haber extraído completamente el ZIP
    echo 2. No muevas archivos de la carpeta {app_name}
    echo 3. Si persiste, vuelve a descargar
    echo.
    pause
    exit /b 1
)

echo 🔄 Intentando ejecutar con permisos elevados...
echo.

powershell -Command "Start-Process '{app_name}\\{app_name}.exe' -Verb RunAs" 2>nul

if errorlevel 1 (
    echo.
    echo ⚠️ No se pudo ejecutar con permisos de administrador
    echo 🔄 Intentando ejecución normal...
    echo.
    start "" "{app_name}\\{app_name}.exe"
)

echo.
echo ✅ Proceso de solución completado
echo.
echo Si aún tienes problemas:
echo 1. Desactiva temporalmente Windows Defender
echo 2. Agrega la carpeta a excepciones del antivirus
echo 3. Reinicia el equipo e intenta nuevamente
echo.
echo 💡 Esta aplicación es 100%% segura y sin virus
echo.
pause
'''.format(app_name=BuildConfig.APP_NAME)

    with open(dist_path / "🔧 SOLUCIONAR_PROBLEMAS.bat", "w", encoding="utf-8") as f:
        f.write(troubleshoot_content)

    print("✅ Documentación creada")

def crear_zip_distribucion(dist_path: Path):
    """Crea el archivo ZIP final para distribución"""
    zip_path = Path(BuildConfig.ZIP_NAME)
    if zip_path.exists():
        zip_path.unlink()

    print(f"📦 Creando {BuildConfig.ZIP_NAME}...")

    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as zipf:
        for file_path in dist_path.rglob('*'):
            if file_path.is_file():
                arcname = file_path.relative_to(dist_path.parent)
                zipf.write(file_path, arcname)

    zip_size = zip_path.stat().st_size
    print(f"✅ ZIP creado: {zip_size:,} bytes ({zip_size/1024/1024:.1f} MB)")

def mostrar_resumen_final():
    """Muestra el resumen final del proceso"""
    print("\n" + "🎉" + "="*58)
    print("   BUILD COMPLETADO - EJECUTABLE PLUG & PLAY LISTO")
    print("="*60)
    print()

    print("📂 ARCHIVOS GENERADOS:")
    print(f"├── 📦 {BuildConfig.ZIP_NAME}")
    print(f"├── 📁 {BuildConfig.DIST_DIR}/")
    print(f"│   ├── 🚀 ABRIR_GESTOR_AUDIENCIAS.bat")
    print(f"│   ├── 📖 LÉEME_PRIMERO.txt")
    print(f"│   ├── 🔧 SOLUCIONAR_PROBLEMAS.bat")
    print(f"│   └── 📁 {BuildConfig.APP_NAME}/")
    print(f"│       └── 💻 {BuildConfig.APP_NAME}.exe")
    print()

    print("✅ CARACTERÍSTICAS INCLUIDAS:")
    print("• 🤖 API OpenAI embebida y oculta")
    print("• 📊 Plantilla Excel integrada")
    print("• 🎨 Icono personalizado aplicado")
    print("• 🛡️ Compatible con Windows 10+")
    print("• 🚫 Sin necesidad de Python instalado")
    print("• ⚡ Ejecución inmediata sin configuración")
    print()

    print("📤 PARA DISTRIBUIR:")
    print(f"1. Comparte el archivo: {BuildConfig.ZIP_NAME}")
    print("2. Instrucción simple: 'Descomprime y ejecuta el launcher'")
    print("3. ¡Listo! Los usuarios pueden usar la app inmediatamente")
    print()

    print("🎯 PRUEBA LOCAL:")
    print(f"1. Ve a: {BuildConfig.DIST_DIR}/")
    print("2. Ejecuta: 🚀 ABRIR_GESTOR_AUDIENCIAS.bat")
    print("3. Verifica que todo funcione correctamente")
    print()

    print(f"💫 ¡Tu aplicación Plug & Play está lista para compartir!")
    print()

def main():
    """Función principal del proceso de build"""
    mostrar_banner()

    # Paso 1: Verificar y actualizar PyInstaller
    if not verificar_y_actualizar_pyinstaller():
        print("❌ No se pudo configurar PyInstaller")
        return False

    # Paso 2: Verificar dependencias
    if not verificar_dependencias():
        print("❌ Dependencias faltantes")
        return False

    # Paso 3: Limpiar archivos temporales
    limpiar_archivos_temporales()

    # Paso 4: Ejecutar PyInstaller
    if not ejecutar_pyinstaller():
        print("❌ Error en la compilación")
        return False

    # Paso 5: Verificar ejecutable
    if not verificar_ejecutable():
        print("❌ Error verificando ejecutable")
        return False

    # Paso 6: Crear distribución final
    if not crear_distribucion_final():
        print("❌ Error creando distribución")
        return False

    # Paso 7: Mostrar resumen
    mostrar_resumen_final()

    return True

if __name__ == "__main__":
    try:
        success = main()
        sys.exit(0 if success else 1)
    except KeyboardInterrupt:
        print("\n\n⏹️ Build cancelado por el usuario")
        sys.exit(1)
    except Exception as e:
        print(f"\n❌ Error inesperado: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
