"""
Procesador masivo de documentos para extraer múltiples audiencias
Soporta archivos PDF y Word
"""

import flet as ft
from typing import List, Dict, Callable, Optional
from gui.constants import get_theme_colors
import threading
import time
import os
import hashlib
from functools import lru_cache
import concurrent.futures
from datetime import datetime


class DialogoProcesadorMasivo:
    """Diálogo para procesar documentos masivamente - VERSIÓN OPTIMIZADA."""

    def __init__(self, page: ft.Page, callback: Callable[[List[Dict]], None]):
        self.page = page
        self.callback = callback
        self.dialog = None
        self.archivo_seleccionado = None
        self.audiencias_extraidas = []

        # 🚀 OPTIMIZACIONES NUEVAS
        self._cache_documentos = {}  # Caché para contenido de documentos
        self._cache_ia_calls = {}  # Caché para llamadas a IA
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=3)
        self._stats_cache = {"hits": 0, "misses": 0}

        # Controles de UI
        self.btn_seleccionar_archivo = None
        self.texto_archivo = None
        self.btn_procesar = None
        self.indicador_progreso = None
        self.contenedor_preview = None
        self.lista_audiencias = None

        self._crear_dialogo()

    def _crear_dialogo(self):
        """Crea el diálogo de procesamiento masivo."""
        colors = get_theme_colors()

        # Botón para seleccionar archivo
        self.btn_seleccionar_archivo = ft.ElevatedButton(
            text="📁 Seleccionar Documento",
            icon=ft.Icons.UPLOAD_FILE,
            on_click=self._on_seleccionar_archivo,
            style=ft.ButtonStyle(
                bgcolor=colors["primary"],
                color=colors["text_on_primary"],
                elevation=2,
                shape=ft.RoundedRectangleBorder(radius=8),
                padding=ft.Padding(20, 12, 20, 12),
            ),
        )

        # Texto del archivo seleccionado
        self.texto_archivo = ft.Text(
            "Ningún archivo seleccionado",
            size=12,
            color=colors["text_secondary"],
            italic=True,
        )

        # Botón procesar (TEXTO MEJORADO)
        self.btn_procesar = ft.ElevatedButton(
            text="🤖 Analizar con IA",
            icon=ft.Icons.AUTO_AWESOME,
            on_click=self._on_procesar_documento,
            disabled=True,
            style=ft.ButtonStyle(
                bgcolor=colors["success"],
                color=colors["text_on_primary"],
                elevation=2,
                shape=ft.RoundedRectangleBorder(radius=8),
                padding=ft.Padding(20, 12, 20, 12),
            ),
        )

        # Botón de ayuda de optimización
        self.btn_ayuda_optimizacion = ft.OutlinedButton(
            text="💡 Optimizar Documento",
            icon=ft.Icons.HELP_OUTLINE,
            on_click=self._on_mostrar_ayuda_optimizacion,
            style=ft.ButtonStyle(
                color=colors["primary"],
                side=ft.BorderSide(1, colors["primary"]),
                shape=ft.RoundedRectangleBorder(radius=8),
                padding=ft.Padding(15, 10, 15, 10),
            ),
            tooltip="Ver prompt para optimizar documentos en otras IAs",
        )

        # Indicador de progreso (CENTRADO HORIZONTAL Y VERTICALMENTE)
        self.indicador_progreso = ft.Container(
            content=ft.Column(
                controls=[
                    ft.ProgressBar(
                        width=400,
                        color=colors["primary"],
                        bgcolor=colors["surface_border"],
                    ),
                    ft.Text(
                        "Procesando documento...",
                        size=14,
                        color=colors["primary"],
                        text_align=ft.TextAlign.CENTER,
                    ),
                ],
                horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                spacing=10,
            ),
            visible=False,
            padding=ft.Padding(20, 15, 20, 15),
            # CENTRADO PERFECTO - horizontal y vertical
            alignment=ft.alignment.center,
        )

        # Lista de audiencias extraídas (preview)
        self.lista_audiencias = ft.Column(
            controls=[],
            spacing=5,
            scroll=ft.ScrollMode.AUTO,
        )

        # Contenedor de preview
        self.contenedor_preview = ft.Container(
            content=ft.Column(
                controls=[
                    ft.Text(
                        "🎯 Audiencias encontradas:",
                        size=16,
                        weight=ft.FontWeight.W_600,
                        color=colors["text_primary"],
                    ),
                    ft.Text(
                        "📅 Ordenadas por fecha (más reciente primero) y hora (más tarde primero)",
                        size=12,
                        color=colors["text_secondary"],
                        italic=True,
                    ),
                    ft.Container(
                        content=self.lista_audiencias,
                        height=200,  # 🆕 REDUCIDO: de 280 a 200 para pantallas pequeñas
                        border=ft.border.all(1, colors["surface_border"]),
                        border_radius=8,
                        bgcolor=colors["surface_secondary"],
                        padding=ft.Padding(10, 10, 10, 10),
                    ),
                ],
                spacing=8,  # 🆕 REDUCIDO: de 10 a 8 para optimizar espacio vertical
            ),
            visible=False,
        )

        # Botones de acción final (TEXTOS MEJORADOS)
        btn_importar_todas = ft.ElevatedButton(
            text="📥 Importar al Excel",
            icon=ft.Icons.DOWNLOAD,
            on_click=self._on_importar_todas,
            style=ft.ButtonStyle(
                bgcolor=colors["success"],
                color=colors["text_on_primary"],
                elevation=2,
                shape=ft.RoundedRectangleBorder(radius=8),
                padding=ft.Padding(20, 10, 20, 10),
            ),
        )

        btn_cancelar = ft.TextButton(
            text="❌ Cancelar",
            on_click=self._on_cancelar,
            style=ft.ButtonStyle(
                color=colors["text_secondary"],
                padding=ft.Padding(20, 10, 20, 10),
            ),
        )

        # Contenido del diálogo
        self.dialog = ft.AlertDialog(
            modal=True,
            title=ft.Text(
                "📄 Procesamiento Masivo de Documentos",
                size=18,
                weight=ft.FontWeight.BOLD,
                color=colors["text_primary"],
            ),
            bgcolor=colors["surface_secondary"],
            content=ft.Container(
                content=ft.Column(
                    controls=[
                        # Información inicial
                        ft.Container(
                            content=ft.Column(
                                controls=[
                                    ft.Row(
                                        controls=[
                                            ft.Icon(
                                                ft.Icons.INFO_OUTLINE,
                                                size=20,
                                                color=colors["primary"],
                                            ),
                                            ft.Text(
                                                "Selecciona un documento PDF o Word con múltiples audiencias",
                                                size=14,
                                                color=colors["text_primary"],
                                                expand=True,
                                            ),
                                        ],
                                        spacing=8,
                                    ),
                                    ft.Text(
                                        "• La IA identificará y extraerá automáticamente todas las audiencias\n• Podrás revisar cada una antes de importar\n• Formatos soportados: .pdf, .docx, .doc, .txt",
                                        size=12,
                                        color=colors["text_secondary"],
                                        italic=True,
                                    ),
                                ],
                                spacing=8,
                            ),
                            bgcolor=colors["primary_light"],
                            border_radius=8,
                            padding=ft.Padding(
                                12, 8, 12, 8
                            ),  # 🆕 REDUCIDO: de 15,12 a 12,8 para ahorrar espacio
                            margin=ft.Margin(0, 0, 0, 10),  # 🆕 REDUCIDO: de 15 a 10
                        ),
                        # Selección de archivo
                        ft.Column(
                            controls=[
                                ft.Row(
                                    controls=[
                                        self.btn_seleccionar_archivo,
                                        self.btn_procesar,
                                    ],
                                    spacing=15,
                                    alignment=ft.MainAxisAlignment.CENTER,
                                ),
                                ft.Row(
                                    controls=[
                                        self.btn_ayuda_optimizacion,
                                    ],
                                    spacing=10,
                                    alignment=ft.MainAxisAlignment.CENTER,
                                ),
                                self.texto_archivo,
                            ],
                            horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                            spacing=8,
                        ),
                        # Indicador de progreso
                        self.indicador_progreso,
                        # Preview de resultados
                        self.contenedor_preview,
                    ],
                    tight=True,
                    spacing=10,  # 🆕 REDUCIDO: de 15 a 10 para optimizar espacio vertical
                ),
                width=750,  # Mantenido: ancho óptimo para contenido horizontal
                height=500,  # 🆕 REDUCIDO: de 620 a 500 para pantallas pequeñas
            ),
            actions=[
                btn_cancelar,
                btn_importar_todas,
            ],
            actions_alignment=ft.MainAxisAlignment.END,
        )

        # Mostrar el diálogo
        self.page.overlay.append(self.dialog)
        self.dialog.open = True
        self.page.update()

    def _on_seleccionar_archivo(self, e):
        """Maneja la selección de archivo."""

        def on_file_picker_result(e: ft.FilePickerResultEvent):
            if e.files:
                archivo = e.files[0]
                self.archivo_seleccionado = archivo.path

                # Verificar extensión
                extension = archivo.name.lower().split(".")[-1]
                if extension not in ["pdf", "docx", "doc", "txt"]:
                    self.texto_archivo.value = (
                        f"❌ Formato no soportado: {archivo.name}"
                    )
                    self.texto_archivo.color = get_theme_colors()["error"]
                    self.btn_procesar.disabled = True
                else:
                    self.texto_archivo.value = f"📄 {archivo.name}"
                    self.texto_archivo.color = get_theme_colors()["success"]
                    self.btn_procesar.disabled = False

                self.page.update()

        # Crear file picker
        file_picker = ft.FilePicker(on_result=on_file_picker_result)
        self.page.overlay.append(file_picker)
        self.page.update()

        # Abrir diálogo de selección
        file_picker.pick_files(
            dialog_title="Seleccionar documento",
            file_type=ft.FilePickerFileType.CUSTOM,
            allowed_extensions=["pdf", "docx", "doc", "txt"],
            allow_multiple=False,
        )

    def _on_procesar_documento(self, e):
        """Procesa el documento seleccionado."""
        if not self.archivo_seleccionado:
            self._mostrar_mensaje(
                "Primero debe seleccionar un documento para procesar.\n\nUse el botón '📁 Seleccionar Documento' para elegir un archivo PDF o Word que contenga las audiencias a procesar."
            )
            return

        # Mostrar indicador de progreso
        self.indicador_progreso.visible = True
        self.btn_procesar.disabled = True
        self.contenedor_preview.visible = False
        self.page.update()

        # Procesar en hilo separado
        threading.Thread(target=self._procesar_en_hilo, daemon=True).start()

    def _procesar_en_hilo(self):
        """Procesa el documento en un hilo separado - VERSIÓN OPTIMIZADA."""
        try:
            # 🚀 OPTIMIZACIÓN: Verificar caché primero
            archivo_hash = self._obtener_hash_archivo(self.archivo_seleccionado)

            if archivo_hash in self._cache_documentos:
                self._stats_cache["hits"] += 1
                print(f"📈 Cache HIT - Reutilizando análisis previo")
                texto_documento = self._cache_documentos[archivo_hash]["texto"]
            else:
                self._stats_cache["misses"] += 1
                print(f"📊 Cache MISS - Analizando documento por primera vez")

                # Extraer texto del documento
                texto_documento = self._extraer_texto_documento(
                    self.archivo_seleccionado
                )

                if not texto_documento:
                    self._mostrar_error("No se pudo extraer texto del documento")
                    return

                # 🚀 GUARDAR EN CACHÉ
                self._cache_documentos[archivo_hash] = {
                    "texto": texto_documento,
                    "timestamp": time.time(),
                    "archivo": os.path.basename(self.archivo_seleccionado),
                }

                # Limpiar caché si crece mucho (mantener últimos 10 documentos)
                if len(self._cache_documentos) > 10:
                    self._limpiar_cache_documentos()

            # Actualizar progreso
            self._actualizar_progreso("🤖 Dividiendo audiencias...")

            # 🚀 PROCESAMIENTO OPTIMIZADO CON CACHÉ DE IA
            secciones_audiencias = self._dividir_audiencias_con_cache(texto_documento)

            if not secciones_audiencias:
                self._mostrar_error("No se encontraron audiencias en el documento")
                return

            # 🚀 PROCESAMIENTO PARALELO
            self._actualizar_progreso("⚡ Procesando audiencias en paralelo...")
            self.audiencias_extraidas = self._procesar_secciones_paralelo(
                secciones_audiencias
            )

            if not self.audiencias_extraidas:
                self._mostrar_error("No se pudieron procesar las audiencias")
                return

            # Mostrar estadísticas de caché
            print(
                f"📊 Cache Stats - Hits: {self._stats_cache['hits']}, Misses: {self._stats_cache['misses']}"
            )

            # Mostrar resultados
            self._mostrar_resultados()

        except Exception as ex:
            self._mostrar_error(f"Error procesando documento: {str(ex)}")
            print(f"Error detallado: {ex}")  # Para debugging

    def _extraer_texto_documento(self, ruta_archivo: str) -> str:
        """Extrae texto de un documento PDF, Word o TXT."""
        try:
            extension = ruta_archivo.lower().split(".")[-1]

            if extension == "pdf":
                return self._extraer_texto_pdf(ruta_archivo)
            elif extension in ["docx", "doc"]:
                return self._extraer_texto_word(ruta_archivo)
            elif extension == "txt":
                return self._extraer_texto_txt(ruta_archivo)
            else:
                raise Exception(f"Formato no soportado: {extension}")

        except Exception as ex:
            print(f"Error extrayendo texto: {ex}")
            return ""

    def _extraer_texto_pdf(self, ruta_archivo: str) -> str:
        """Extrae texto de un archivo PDF."""
        try:
            import PyPDF2

            texto_completo = ""
            with open(ruta_archivo, "rb") as archivo:
                reader = PyPDF2.PdfReader(archivo)
                for pagina in reader.pages:
                    texto_completo += pagina.extract_text() + "\n"

            return texto_completo

        except ImportError:
            raise Exception("PyPDF2 no está instalado. Ejecuta: pip install PyPDF2")
        except Exception as ex:
            raise Exception(f"Error leyendo PDF: {ex}")

    def _extraer_texto_word(self, ruta_archivo: str) -> str:
        """Extrae texto de un archivo Word."""
        try:
            import docx

            doc = docx.Document(ruta_archivo)
            texto_completo = ""

            for parrafo in doc.paragraphs:
                texto_completo += parrafo.text + "\n"

            return texto_completo

        except ImportError:
            raise Exception(
                "python-docx no está instalado. Ejecuta: pip install python-docx"
            )
        except Exception as ex:
            raise Exception(f"Error leyendo Word: {ex}")

    def _extraer_texto_txt(self, ruta_archivo: str) -> str:
        """Extrae texto de un archivo TXT."""
        try:
            # Intentar múltiples encodings para mayor compatibilidad
            encodings = ["utf-8", "latin-1", "cp1252", "iso-8859-1"]

            for encoding in encodings:
                try:
                    with open(ruta_archivo, "r", encoding=encoding) as archivo:
                        texto_completo = archivo.read()
                        print(
                            f"✅ Archivo TXT leído exitosamente con encoding {encoding}"
                        )
                        return texto_completo
                except UnicodeDecodeError:
                    continue

            # Si ningún encoding funciona, intentar con errores ignorados
            with open(ruta_archivo, "r", encoding="utf-8", errors="ignore") as archivo:
                texto_completo = archivo.read()
                print("⚠️ Archivo TXT leído con algunos caracteres ignorados")
                return texto_completo

        except Exception as ex:
            raise Exception(f"Error leyendo TXT: {ex}")

    # 🚀 MÉTODOS DE OPTIMIZACIÓN NUEVOS

    def _obtener_hash_archivo(self, ruta_archivo: str) -> str:
        """Genera hash único para un archivo basado en ruta y timestamp."""
        try:
            stat = os.stat(ruta_archivo)
            contenido_hash = f"{ruta_archivo}_{stat.st_size}_{stat.st_mtime}"
            return hashlib.md5(contenido_hash.encode()).hexdigest()
        except Exception:
            # Fallback: solo usar la ruta
            return hashlib.md5(ruta_archivo.encode()).hexdigest()

    def _limpiar_cache_documentos(self):
        """Limpia el caché manteniendo solo los documentos más recientes."""
        if len(self._cache_documentos) <= 10:
            return

        # Ordenar por timestamp y mantener los 8 más recientes
        items_ordenados = sorted(
            self._cache_documentos.items(),
            key=lambda x: x[1]["timestamp"],
            reverse=True,
        )

        self._cache_documentos.clear()
        for key, value in items_ordenados[:8]:
            self._cache_documentos[key] = value

        print(
            f"🧹 Cache limpiado - Manteniendo {len(self._cache_documentos)} documentos"
        )

    def _dividir_audiencias_con_cache(self, texto_documento: str) -> List[str]:
        """Divide audiencias usando caché inteligente para la IA."""
        # Crear hash del texto para caché de IA
        texto_hash = hashlib.md5(texto_documento.encode()).hexdigest()

        if texto_hash in self._cache_ia_calls:
            print("🚀 Reutilizando división de audiencias desde caché")
            return self._cache_ia_calls[texto_hash]

        # Llamada original a la IA
        from services.ai_service import ai_service

        secciones = ai_service.dividir_audiencias_masivo(texto_documento)

        # Guardar en caché
        self._cache_ia_calls[texto_hash] = secciones

        # Limpiar caché de IA si crece mucho
        if len(self._cache_ia_calls) > 50:
            self._limpiar_cache_ia()

        return secciones

    def _limpiar_cache_ia(self):
        """Limpia el caché de llamadas a IA manteniendo las más recientes."""
        if len(self._cache_ia_calls) <= 50:
            return

        # Mantener solo las primeras 30 (LRU simple)
        items = list(self._cache_ia_calls.items())
        self._cache_ia_calls.clear()

        for key, value in items[-30:]:  # Últimas 30
            self._cache_ia_calls[key] = value

        print(
            f"🧹 Cache IA limpiado - Manteniendo {len(self._cache_ia_calls)} llamadas"
        )

    def _procesar_secciones_paralelo(
        self, secciones_audiencias: List[str]
    ) -> List[Dict]:
        """Procesa múltiples secciones en paralelo para mayor velocidad."""
        if not secciones_audiencias:
            return []

        total_secciones = len(secciones_audiencias)
        audiencias_procesadas = []

        # Si hay pocas secciones, procesar secuencialmente
        if total_secciones <= 2:
            return self._procesar_secciones_secuencial(secciones_audiencias)

        print(f"⚡ Procesando {total_secciones} secciones en paralelo")

        # Procesar en paralelo con ThreadPoolExecutor
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
            # Enviar todas las tareas
            future_to_seccion = {
                executor.submit(self._procesar_seccion_individual, i, seccion): (
                    i,
                    seccion,
                )
                for i, seccion in enumerate(secciones_audiencias, 1)
            }

            # Recopilar resultados conforme van completándose
            for future in concurrent.futures.as_completed(
                future_to_seccion, timeout=120
            ):
                i, seccion = future_to_seccion[future]

                try:
                    resultado = future.result(timeout=30)
                    if resultado:
                        audiencias_procesadas.append(resultado)
                        self._actualizar_progreso(
                            f"✅ Completadas {len(audiencias_procesadas)} de {total_secciones} audiencias"
                        )
                except Exception as ex:
                    print(f"❌ Error procesando sección {i}: {ex}")
                    continue

        # Ordenar por orden original si es posible
        return audiencias_procesadas

    def _procesar_secciones_secuencial(
        self, secciones_audiencias: List[str]
    ) -> List[Dict]:
        """Procesamiento secuencial para pocas secciones."""
        audiencias_procesadas = []
        total_secciones = len(secciones_audiencias)

        for i, seccion in enumerate(secciones_audiencias, 1):
            self._actualizar_progreso(
                f"Procesando audiencia {i} de {total_secciones}..."
            )

            try:
                resultado = self._procesar_seccion_individual(i, seccion)
                if resultado:
                    audiencias_procesadas.append(resultado)
            except Exception as ex:
                print(f"Error procesando sección {i}: {ex}")
                continue

        return audiencias_procesadas

    def _procesar_seccion_individual(self, numero: int, seccion: str) -> Optional[Dict]:
        """Procesa una sección individual con manejo de errores robusto."""
        try:
            from services.ai_service import ai_service

            # Crear hash para caché de esta sección específica
            seccion_hash = hashlib.md5(seccion.encode()).hexdigest()
            cache_key = f"seccion_{seccion_hash}"

            if cache_key in self._cache_ia_calls:
                print(f"🚀 Cache hit para sección {numero}")
                datos_audiencia = self._cache_ia_calls[cache_key]
            else:
                # Procesar con IA
                datos_audiencia = ai_service.extract_audiencia_info(seccion)
                # Guardar en caché
                self._cache_ia_calls[cache_key] = datos_audiencia

            # Agregar metadatos
            datos_audiencia["_texto_original"] = (
                seccion[:200] + "..." if len(seccion) > 200 else seccion
            )
            datos_audiencia["_numero_seccion"] = numero

            return datos_audiencia

        except Exception as ex:
            print(f"Error en sección {numero}: {ex}")
            return None

    def _actualizar_progreso(self, mensaje: str):
        """Actualiza el mensaje de progreso."""
        if self.indicador_progreso.content.controls[1]:
            self.indicador_progreso.content.controls[1].value = mensaje
            self.page.update()

    def _mostrar_error(self, mensaje: str):
        """Muestra un error y oculta el progreso."""
        colors = get_theme_colors()

        self.indicador_progreso.visible = False
        self.btn_procesar.disabled = False

        # Mostrar mensaje de error
        error_container = ft.Container(
            content=ft.Row(
                controls=[
                    ft.Icon(ft.Icons.ERROR, color=colors["error"], size=20),
                    ft.Text(mensaje, color=colors["error"], size=14),
                ],
                spacing=8,
                alignment=ft.MainAxisAlignment.CENTER,
            ),
            bgcolor=colors["error_light"],
            border_radius=8,
            padding=ft.Padding(15, 10, 15, 10),
        )

        # Reemplazar progreso con error temporalmente
        self.indicador_progreso.content = error_container
        self.indicador_progreso.visible = True
        self.page.update()

        # Ocultar error después de 5 segundos
        def ocultar_error():
            time.sleep(5)
            self.indicador_progreso.visible = False
            self.page.update()

        threading.Thread(target=ocultar_error, daemon=True).start()

    def _ordenar_audiencias_por_fecha_hora(self, audiencias: List[Dict]) -> List[Dict]:
        """Ordena las audiencias por fecha (más reciente primero) y hora (más tardía primero)."""

        def extraer_fecha_hora(audiencia: Dict) -> tuple:
            """Extrae fecha y hora como tupla para ordenamiento."""
            try:
                # Obtener fecha de la audiencia
                fecha_str = audiencia.get("fecha", "")
                hora_str = audiencia.get("hora", "")

                # Convertir fecha DD/MM/YYYY a objeto datetime
                fecha_datetime = None
                if fecha_str and fecha_str != "N/A":
                    try:
                        # Intentar diferentes formatos de fecha
                        if "/" in fecha_str:
                            # Formato DD/MM/YYYY
                            partes_fecha = fecha_str.split("/")
                            if len(partes_fecha) == 3:
                                dia, mes, anio = (
                                    int(partes_fecha[0]),
                                    int(partes_fecha[1]),
                                    int(partes_fecha[2]),
                                )
                                fecha_datetime = datetime(anio, mes, dia)
                        elif "-" in fecha_str:
                            # Formato YYYY-MM-DD
                            fecha_datetime = datetime.strptime(fecha_str, "%Y-%m-%d")
                        else:
                            # Intentar formato ISO
                            fecha_datetime = datetime.fromisoformat(fecha_str)
                    except (ValueError, IndexError):
                        # Si falla el parseo, usar fecha muy antigua para que vaya al final
                        fecha_datetime = datetime(1900, 1, 1)
                else:
                    # Sin fecha, usar fecha muy antigua
                    fecha_datetime = datetime(1900, 1, 1)

                # Convertir hora HH:MM a objeto time para ordenamiento
                hora_datetime = None
                if hora_str and hora_str != "N/A":
                    try:
                        # Formato HH:MM
                        if ":" in hora_str:
                            partes_hora = hora_str.split(":")
                            if len(partes_hora) >= 2:
                                hora = int(partes_hora[0])
                                minuto = int(partes_hora[1])
                                # Crear datetime con fecha base y hora específica
                                hora_datetime = datetime(2000, 1, 1, hora, minuto)
                        else:
                            # Solo hora sin minutos
                            hora = int(hora_str)
                            hora_datetime = datetime(2000, 1, 1, hora, 0)
                    except (ValueError, IndexError):
                        # Si falla el parseo, usar hora muy temprana
                        hora_datetime = datetime(2000, 1, 1, 0, 0)
                else:
                    # Sin hora, usar hora muy temprana
                    hora_datetime = datetime(2000, 1, 1, 0, 0)

                # Retornar tupla para ordenamiento: fecha descendente, hora descendente
                # Usar timestamp negativo para orden descendente de fecha
                # Usar hora y minuto negativos para orden descendente de hora
                return (
                    -fecha_datetime.timestamp(),
                    -hora_datetime.hour,
                    -hora_datetime.minute,
                )

            except Exception as ex:
                print(f"⚠️  Error ordenando audiencia: {ex}")
                # En caso de error, devolver valores que pongan esta audiencia al final
                return (0, 0, 0)

        try:
            # Ordenar audiencias usando la función de extracción
            audiencias_ordenadas = sorted(audiencias, key=extraer_fecha_hora)

            print(f"📅 Audiencias ordenadas: {len(audiencias_ordenadas)} elementos")

            # Debug: mostrar primeras fechas para verificar orden
            for i, audiencia in enumerate(audiencias_ordenadas[:3]):
                fecha = audiencia.get("fecha", "N/A")
                hora = audiencia.get("hora", "N/A")
                print(f"  {i+1}. {fecha} {hora}")

            return audiencias_ordenadas

        except Exception as ex:
            print(f"⚠️  Error en ordenamiento general: {ex}")
            # Si falla el ordenamiento, devolver lista original
            return audiencias

    def _mostrar_resultados(self):
        """Muestra los resultados del procesamiento."""
        colors = get_theme_colors()

        # Ocultar progreso
        self.indicador_progreso.visible = False
        self.btn_procesar.disabled = False

        # Limpiar lista anterior
        self.lista_audiencias.controls.clear()

        if not self.audiencias_extraidas:
            self.lista_audiencias.controls.append(
                ft.Container(
                    content=ft.Text(
                        "No se encontraron audiencias válidas en el documento",
                        color=colors["text_secondary"],
                        text_align=ft.TextAlign.CENTER,
                    ),
                    alignment=ft.alignment.center,
                    height=100,
                )
            )
        else:
            # 🆕 ORDENAR AUDIENCIAS POR FECHA Y HORA ANTES DE MOSTRAR
            print("📅 Aplicando ordenamiento por fecha y hora...")
            audiencias_ordenadas = self._ordenar_audiencias_por_fecha_hora(
                self.audiencias_extraidas
            )

            # Agregar cada audiencia como card (usando audiencias ordenadas)
            for i, audiencia in enumerate(audiencias_ordenadas, 1):
                card = self._crear_card_audiencia(i, audiencia, colors)
                self.lista_audiencias.controls.append(card)

            # Actualizar la lista interna con las audiencias ordenadas
            self.audiencias_extraidas = audiencias_ordenadas

        # Mostrar contenedor de preview
        self.contenedor_preview.visible = True
        self.page.update()

    def _crear_card_audiencia(
        self, numero: int, audiencia: Dict, colors: Dict
    ) -> ft.Container:
        """Crea una tarjeta para mostrar una audiencia extraída (INCLUYENDO MOTIVOS)."""

        # Información básica
        radicado = audiencia.get("radicado", "N/A")
        tipo = audiencia.get("tipo_audiencia", "N/A")
        fecha = audiencia.get("fecha", "N/A")
        hora = (
            f"{audiencia.get('hora', '')}:{audiencia.get('minuto', '')}"
            if audiencia.get("hora")
            else "N/A"
        )
        se_realizo = audiencia.get("se_realizo", "N/A")
        motivos = audiencia.get("motivos", "")

        # Checkbox para seleccionar/deseleccionar
        checkbox = ft.Checkbox(
            value=True,  # Por defecto seleccionadas
            data=numero - 1,  # Índice en la lista
        )

        # Crear lista de controles base
        controles_info = [
            ft.Row(
                controls=[
                    ft.Text(
                        f"Audiencia #{numero}",
                        size=13,
                        weight=ft.FontWeight.W_600,
                        color=colors["primary"],
                    ),
                    ft.Text(
                        f"• {tipo}",
                        size=12,
                        color=colors["text_secondary"],
                    ),
                ],
                spacing=5,
            ),
            ft.Text(
                f"📋 {radicado[:50]}{'...' if len(radicado) > 50 else ''}",
                size=11,
                color=colors["text_primary"],
            ),
            ft.Row(
                controls=[
                    ft.Text(f"📅 {fecha}", size=10, color=colors["text_secondary"]),
                    ft.Text(f"🕐 {hora}", size=10, color=colors["text_secondary"]),
                    ft.Text(
                        f"{'✅' if se_realizo == 'SI' else '❌' if se_realizo == 'NO' else '❓'} {se_realizo}",
                        size=10,
                        color=(
                            colors["success"]
                            if se_realizo == "SI"
                            else (
                                colors["error"]
                                if se_realizo == "NO"
                                else colors["text_secondary"]
                            )
                        ),
                    ),
                ],
                spacing=10,
            ),
        ]

        # *** AGREGAR MOTIVOS SI LA RESPUESTA ES "NO" ***
        if se_realizo == "NO" and motivos and str(motivos).strip():
            # Procesar motivos (puede ser string o lista)
            motivos_texto = ""
            if isinstance(motivos, list):
                motivos_texto = ", ".join(
                    [str(m).strip() for m in motivos if str(m).strip()]
                )
            else:
                motivos_texto = str(motivos).strip()

            if motivos_texto:
                controles_info.append(
                    ft.Container(
                        content=ft.Row(
                            controls=[
                                ft.Icon(
                                    ft.Icons.ERROR_OUTLINE,
                                    size=14,
                                    color=colors["warning"],
                                ),
                                ft.Text(
                                    f"Motivo: {motivos_texto}",
                                    size=10,
                                    color=colors["warning"],
                                    weight=ft.FontWeight.W_500,
                                ),
                            ],
                            spacing=4,
                        ),
                        bgcolor=colors["warning_light"],
                        border_radius=4,
                        padding=ft.Padding(6, 4, 6, 4),
                        margin=ft.Margin(0, 2, 0, 0),
                    )
                )

        return ft.Container(
            content=ft.Row(
                controls=[
                    checkbox,
                    ft.Container(
                        content=ft.Column(
                            controls=controles_info,
                            spacing=3,
                            tight=True,
                        ),
                        expand=True,
                    ),
                ],
                spacing=8,
                vertical_alignment=ft.CrossAxisAlignment.START,
            ),
            bgcolor=colors["surface_primary"],
            border=ft.border.all(1, colors["surface_border"]),
            border_radius=6,
            padding=ft.Padding(10, 8, 10, 8),
            margin=ft.Margin(0, 2, 0, 2),
        )

    def _on_importar_todas(self, e):
        """Importa todas las audiencias seleccionadas."""
        if not self.audiencias_extraidas:
            return

        # Filtrar audiencias seleccionadas
        audiencias_seleccionadas = []

        for i, control in enumerate(self.lista_audiencias.controls):
            if isinstance(control, ft.Container):
                checkbox = control.content.controls[0]
                if isinstance(checkbox, ft.Checkbox) and checkbox.value:
                    audiencias_seleccionadas.append(self.audiencias_extraidas[i])

        if not audiencias_seleccionadas:
            # Mostrar error si no hay seleccionadas
            colors = get_theme_colors()
            error_msg = ft.Text(
                "Selecciona al menos una audiencia para importar",
                color=colors["error"],
                size=12,
            )
            # Agregar mensaje temporalmente
            return

        # Cerrar diálogo
        self.dialog.open = False
        self.page.update()

        # Ejecutar callback con audiencias seleccionadas
        self.callback(audiencias_seleccionadas)

    def _on_cancelar(self, e):
        """Cancela el procesamiento masivo y limpia recursos."""
        # 🚀 LIMPIEZA DE RECURSOS
        try:
            if hasattr(self, "_executor"):
                self._executor.shutdown(wait=False)
            print("🧹 Recursos limpiados correctamente")
        except Exception as ex:
            print(f"Warning: Error limpiando recursos: {ex}")

        self.dialog.open = False
        self.page.update()

    def _mostrar_mensaje(self, mensaje: str):
        """Muestra un mensaje de información usando AlertDialog."""
        colors = get_theme_colors()

        def cerrar_mensaje(e):
            dlg.open = False
            self.page.update()

        # Determinar si es un mensaje de error para usar colores apropiados
        es_error = any(
            palabra in mensaje.lower()
            for palabra in ["error", "no se pudo", "falta", "obligatorio", "inválido"]
        )
        es_exito = any(
            palabra in mensaje.lower()
            for palabra in [
                "correctamente",
                "guardado",
                "creado",
                "actualizado",
                "éxito",
            ]
        )

        # Elegir colores según el tipo de mensaje
        if es_error:
            titulo_color = colors["error"]
            mensaje_color = colors["text_primary"]
            boton_color = colors["error"]
        elif es_exito:
            titulo_color = colors["success"]
            mensaje_color = colors["text_primary"]
            boton_color = colors["success"]
        else:
            titulo_color = colors["primary"]
            mensaje_color = colors["text_primary"]
            boton_color = colors["primary"]

        dlg = ft.AlertDialog(
            modal=True,
            title=ft.Text(
                "Información", size=18, weight=ft.FontWeight.BOLD, color=titulo_color
            ),
            content=ft.Text(mensaje, size=14, color=mensaje_color),
            actions=[
                ft.TextButton(
                    "Entendido", on_click=lambda _: self._cerrar_dialogo_info(dlg)
                )
            ],
            actions_alignment=ft.MainAxisAlignment.END,
        )

        self.page.overlay.append(dlg)
        dlg.open = True
        self.page.update()

    def _cerrar_dialogo_info(self, dialogo):
        """Cierra un diálogo de información."""
        dialogo.open = False
        self.page.update()

    def _on_mostrar_ayuda_optimizacion(self, e):
        """Muestra el prompt de optimización para usar en otras IAs."""
        colors = get_theme_colors()

        prompt_texto = """ERES UN EXPERTO EN FORMATEO DE AUDIENCIAS JUDICIALES 🏛️

🎯 MISIÓN PRINCIPAL:
Transformar registros abreviados de audiencias en un formato estructurado, coherente y detallado, compatible con el procesamiento de una IA especializada.

📌 OBJETIVO:
Obtener textos precisos para alimentar los modelos de IA individual y masiva, asegurando estandarización en fechas, actores, motivos y tipos de audiencia.

🧠 INSTRUCCIONES ESTRICTAS:

1️⃣ FECHAS → SIEMPRE expandir completamente:
    • "JUNIO 24.-"  → "El día 24 de junio de 2025"
    • "JULIO 2.-"   → "El día 2 de julio de 2025"
    ✔ Usa el año completo: (año que se encuentra en el texto, por ejemplo, 2025 o 2026)

2️⃣ ABREVIACIONES → Corregir exactamente:
    • "F 17"        → "Fiscal 17"
    • "3 CTO"       → "Juzgado 3 de CTO"
    • "9 CTO"       → "Juzgado 9 de CTO"
    • "DEPACHO"     → "Despacho"
    • "FISCAL"      → "Fiscal"
    • "FRACSA"      → "fracasó"

3️⃣ TIPO DE AUDIENCIA → Normalizar así:
    • "INICIA JUICIO"       → "AUDIENCIA DE JUICIO ORAL"
    • "CONTINUA JUICIO"     → "CONTINUACIÓN DE JUICIO ORAL"
    • "ACUSACION"           → "AUDIENCIA DE FORMULACIÓN DE ACUSACIÓN"
    • "PREPARATORIA"        → "AUDIENCIA PREPARATORIA"
    • "PREPARACIÓN"         → "AUDIENCIA PREPARATORIA"

4️⃣ ESTADO Y MOTIVOS → Traducir cuidadosamente según el actor:

    🔹 Si el defensor habla en primera persona (ej. "solicité", "pedí"):
        • "solicité reprogramación"     → "solicité la reprogramación de la audiencia"
        • "pedí aplazamiento"           → "pedí el aplazamiento de la audiencia"
        • "no pude contactar"           → "no pude contactar al acusado"
        • "me fue imposible"            → "me fue imposible asistir"
        ➕ Estos se asignan como motivo: **Defensor Público**

    🔹 Si no hay referencia al defensor:
        • "FRACSA POR DEPACHO"          → "NO se realizó porque fracasó por el Despacho"
        • "FISCAL PIDE REPROGRAMACION"  → "NO se realizó por solicitud de la Fiscalía"
        • "AUDIENCIA CANCELADA"         → Reescribir con motivo implícito si está claro
        • "SE REALIZÓ" o sin falla      → "se realizó con normalidad"

    ⚠ Importante: si el texto no contiene ningún motivo válido, el campo de motivo debe quedar en blanco, **no se debe inferir ni inventar información**.

5️⃣ SOLO 8 MOTIVOS VÁLIDOS (Excel):
    • Juez
    • Fiscalía
    • Usuario
    • Inpec
    • Víctima
    • ICBF
    • Defensor de Confianza
    • Defensor Público

6️⃣ FORMATO FINAL:
    • Numerar cada registro: "1-", "2-", etc.
    • Incluir siempre la frase:
      🔸 "realicé conexión virtual para llevar a cabo audiencia programada"
    • Mantener nombres propios completos tal como aparecen

⚠ DETECCIÓN AUTOMÁTICA:
El sistema IA reconoce lenguaje en primera persona para asignar correctamente el motivo como “Defensor Público”. Sé claro al escribir estas expresiones.

📌 EJEMPLO FORMATEADO - Motivo externo:
ANTES:
"JUNIO 24.- INICIA JUICIO FERNANDO LOPEZ.- F 17.- FRACSA POR DEPACHO"

DESPUÉS:
"1- El día 24 de junio de 2025, realicé conexión virtual para llevar a cabo audiencia programada (AUDIENCIA DE JUICIO ORAL) del señor FERNANDO LOPEZ, Fiscal 17, NO se realizó porque fracasó por el Despacho."

📌 EJEMPLO FORMATEADO - Motivo defensor público:
ANTES:
"JULIO 2.- PREPARATORIA CARLOS MESA.- solicité reprogramación por no contactar acusado"

DESPUÉS:
"2- El día 2 de julio de 2025, realicé conexión virtual para llevar a cabo audiencia programada (AUDIENCIA PREPARATORIA) del señor CARLOS MESA, solicité la reprogramación de la audiencia porque no pude contactar al acusado."

🔽 TEXTO A CONVERTIR:
[AQUÍ PEGAR TU DOCUMENTO]"""

        # Campo de texto seleccionable para el prompt
        campo_prompt = ft.TextField(
            value=prompt_texto,
            multiline=True,
            min_lines=15,
            max_lines=20,
            read_only=True,
            border_color=colors["primary"],
            text_style=ft.TextStyle(size=12),
        )

        # Diálogo de ayuda
        dlg_ayuda = ft.AlertDialog(
            modal=True,
            title=ft.Text(
                "💡 Optimizar Documento con Otras IAs",
                size=18,
                weight=ft.FontWeight.BOLD,
                color=colors["primary"],
            ),
            content=ft.Container(
                content=ft.Column(
                    controls=[
                        ft.Text(
                            "Para mejorar la precisión, usa este prompt en ChatGPT, DeepSeek, Grok u otra IA:",
                            size=14,
                            color=colors["text_primary"],
                        ),
                        ft.Container(
                            content=campo_prompt,
                            border=ft.border.all(1, colors["surface_border"]),
                            border_radius=8,
                            padding=10,
                        ),
                        ft.Text(
                            "📋 Selecciona todo el texto (Ctrl+A) y cópialo (Ctrl+C) para usar en otra IA",
                            size=12,
                            color=colors["text_secondary"],
                            italic=True,
                            text_align=ft.TextAlign.CENTER,
                        ),
                    ],
                    spacing=15,
                    tight=True,
                ),
                width=600,
                height=500,
            ),
            actions=[
                ft.TextButton(
                    "Cerrar", on_click=lambda _: self._cerrar_dialogo_info(dlg_ayuda)
                ),
            ],
            actions_alignment=ft.MainAxisAlignment.END,
        )

        self.page.overlay.append(dlg_ayuda)
        dlg_ayuda.open = True
        self.page.update()
