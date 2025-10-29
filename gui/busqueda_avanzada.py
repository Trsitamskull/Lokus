"""
Sistema de búsqueda avanzada con edición integrada
Permite filtrar y editar audiencias directamente
"""

import flet as ft
from typing import List, Dict, Callable, Optional, Tuple
from datetime import datetime, date
from gui.constants import get_theme_colors
import re
import threading
import time


class VentanaBusquedaAvanzada:
    """Ventana de búsqueda avanzada con edición integrada."""
    
    def __init__(self, page: ft.Page, gestor_excel, callback_editar: Callable[[int, List], None]):
        self.page = page
        self.gestor_excel = gestor_excel
        self.callback_editar = callback_editar
        self.dialog = None
        self.resultados_mostrados = []
        
        # Campos de filtro
        self.filtro_radicado = None
        self.filtro_fecha_desde = None
        self.filtro_fecha_hasta = None
        self.filtro_se_realizo = None
        self.filtro_motivos = None
        self.filtro_tipo_audiencia = None
        
        # Contenedor de resultados
        self.contenedor_resultados = None
        self.texto_sin_resultados = None
        
        # Indicadores de carga
        self.indicador_carga_inicial = None
        self.indicador_carga_filtro = None
        
        self._crear_dialogo()
    
    def _crear_dialogo(self):
        """Crea la ventana de búsqueda avanzada."""
        colors = get_theme_colors()
        
        # Filtros de búsqueda - VERSIÓN COMPACTA
        self.filtro_radicado = ft.TextField(
            label="Radicado/Nombre",
            hint_text="GARCÍA LÓPEZ o 2024-00123",
            width=200,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
        )
        
        # Filtro de fecha desde
        self.filtro_fecha_desde = ft.TextField(
            label="Desde",
            hint_text="DD/MM/YYYY",
            width=100,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
        )
        
        # Filtro de fecha hasta
        self.filtro_fecha_hasta = ft.TextField(
            label="Hasta",
            hint_text="DD/MM/YYYY",
            width=100,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
        )
        
        # Filtro de se realizó
        self.filtro_se_realizo = ft.Dropdown(
            label="Estado",
            width=90,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
            options=[
                ft.dropdown.Option("", "Todos"),
                ft.dropdown.Option("SI", "✅ SI"),
                ft.dropdown.Option("NO", "❌ NO"),
            ],
            value="",
            on_change=self._on_se_realizo_change,  # 🆕 Evento para habilitar/deshabilitar motivos
        )
        
        # Filtro de motivos de fracaso - 🆕 MEJORADO: 8 motivos válidos del Excel
        self.filtro_motivos = ft.Dropdown(
            label="Motivo",
            width=140,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
            options=[
                ft.dropdown.Option("", "Todos"),
                ft.dropdown.Option("juez", "👨‍⚖️ Juez"),
                ft.dropdown.Option("fiscalía", "🏛️ Fiscalía"),
                ft.dropdown.Option("usuario", "� Usuario"),
                ft.dropdown.Option("inpec", "🏢 INPEC"),
                ft.dropdown.Option("víctima", "� Víctima"),
                ft.dropdown.Option("icbf", "🏢 ICBF"),
                ft.dropdown.Option("defensor confianza", "⚖️ Defensor Confianza"),
                ft.dropdown.Option("defensor público", "�️ Defensor Público"),
            ],
            value="",
            disabled=False,  # Se habilitará/deshabilitará según "¿Se realizó?"
        )
        
        # Filtro de tipo de audiencia - 🆕 MEJORADO: Dropdown con tipos exactos + opción "Otra"
        self.filtro_tipo_audiencia = ft.Dropdown(
            label="Tipo",
            width=200,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
            options=[
                ft.dropdown.Option("", "Todas"),
                ft.dropdown.Option("Alegatos de conclusión", "Alegatos de conclusión"),
                ft.dropdown.Option("Audiencia concentrada", "Audiencia concentrada"),
                ft.dropdown.Option("Audiencia de acusación", "Audiencia de acusación"),
                ft.dropdown.Option("Audiencia de conciliación", "Audiencia de conciliación"),
                ft.dropdown.Option("Audiencia de control de legalidad", "Audiencia de control de legalidad"),
                ft.dropdown.Option("Audiencia de individualización de pena", "Audiencia de individualización de pena"),
                ft.dropdown.Option("Audiencia de imputación", "Audiencia de imputación"),
                ft.dropdown.Option("Audiencia de incidente de reparación integral", "Audiencia de incidente de reparación integral"),
                ft.dropdown.Option("Audiencia de juicio oral", "Audiencia de juicio oral"),
                ft.dropdown.Option("Audiencia de medidas de aseguramiento", "Audiencia de medidas de aseguramiento"),
                ft.dropdown.Option("Audiencia de nulidad", "Audiencia de nulidad"),
                ft.dropdown.Option("Audiencia de preclusión", "Audiencia de preclusión"),
                ft.dropdown.Option("Audiencia de prórroga", "Audiencia de prórroga"),
                ft.dropdown.Option("Audiencia de revisión de medida", "Audiencia de revisión de medida"),
                ft.dropdown.Option("Audiencia de verificación de cumplimiento", "Audiencia de verificación de cumplimiento"),
                ft.dropdown.Option("Audiencia preliminar", "Audiencia preliminar"),
                ft.dropdown.Option("Audiencia preparatoria", "Audiencia preparatoria"),
                ft.dropdown.Option("Otra", "Otra"),
            ],
            value="",
            on_change=self._on_tipo_audiencia_change,  # 🆕 Para manejar opción "Otra"
        )
        
        # Campo de texto libre para "Otra" (inicialmente oculto)
        self.filtro_tipo_audiencia_otra = ft.TextField(
            label="Especificar otro tipo",
            hint_text="Escriba el tipo de audiencia",
            width=200,
            border_radius=6,
            filled=True,
            bgcolor=colors["surface_primary"],
            border_color=colors["surface_border"],
            text_style=ft.TextStyle(size=12, color=colors["text_primary"]),
            label_style=ft.TextStyle(size=11, color=colors["text_secondary"]),
            content_padding=ft.Padding(8, 4, 8, 4),
            visible=False,  # Oculto inicialmente
        )
        
        # Botones de acción - más compactos
        btn_buscar = ft.ElevatedButton(
            text="🔍 Filtrar",
            on_click=self._on_buscar,
            style=ft.ButtonStyle(
                bgcolor=colors["primary"],
                color=colors["text_on_primary"],
                elevation=2,
                shape=ft.RoundedRectangleBorder(radius=6),
                padding=ft.Padding(12, 6, 12, 6),
                text_style=ft.TextStyle(size=12, weight=ft.FontWeight.W_600),
            ),
        )
        
        btn_limpiar = ft.TextButton(
            text="🧹 Limpiar",
            on_click=self._on_limpiar_filtros,
            style=ft.ButtonStyle(
                color=colors["text_secondary"],
                padding=ft.Padding(10, 6, 10, 6),
                text_style=ft.TextStyle(size=12),
            ),
        )
        
        btn_cerrar = ft.TextButton(
            text="Cerrar",
            on_click=self._on_cerrar,
            style=ft.ButtonStyle(
                color=colors["text_secondary"],
                padding=ft.Padding(20, 10, 20, 10),
            ),
        )
        
        # Contenedor de resultados (inicialmente vacío) - 🆕 CON ALTURA ESPECÍFICA Y SCROLL
        self.contenedor_resultados = ft.Column(
            controls=[],
            spacing=5,
            scroll=ft.ScrollMode.AUTO,
            height=500,  # 🆕 ALTURA ESPECÍFICA para el scroll interno
            expand=False,  # 🆕 No expandir para mantener altura fija
        )
        
        # Indicador de carga inicial
        self.indicador_carga_inicial = ft.Container(
            content=ft.Column(
                controls=[
                    ft.ProgressRing(
                        width=48,
                        height=48,
                        stroke_width=4,
                        color=colors["primary"],
                    ),
                    ft.Container(height=15),
                    ft.Text(
                        "🔍 Cargando audiencias...",
                        size=16,
                        color=colors["text_primary"],
                        text_align=ft.TextAlign.CENTER,
                        weight=ft.FontWeight.W_500,
                    ),
                    ft.Text(
                        "Por favor espere mientras se cargan los datos",
                        size=12,
                        color=colors["text_secondary"],
                        text_align=ft.TextAlign.CENTER,
                        italic=True,
                    ),
                ],
                horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                spacing=5,
            ),
            padding=ft.Padding(20, 60, 20, 60),
            alignment=ft.alignment.center,
            visible=False,
        )
        
        # Indicador de carga para filtros
        self.indicador_carga_filtro = ft.Container(
            content=ft.Column(
                controls=[
                    ft.ProgressRing(
                        width=36,
                        height=36,
                        stroke_width=3,
                        color=colors["primary"],
                    ),
                    ft.Container(height=10),
                    ft.Text(
                        "🎯 Aplicando filtros...",
                        size=14,
                        color=colors["text_primary"],
                        text_align=ft.TextAlign.CENTER,
                        weight=ft.FontWeight.W_500,
                    ),
                    ft.Text(
                        "Buscando audiencias que coincidan",
                        size=11,
                        color=colors["text_secondary"],
                        text_align=ft.TextAlign.CENTER,
                        italic=True,
                    ),
                ],
                horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                spacing=5,
            ),
            padding=ft.Padding(20, 40, 20, 40),
            alignment=ft.alignment.center,
            visible=False,
        )
        
        # Mensaje sin resultados
        self.texto_sin_resultados = ft.Container(
            content=ft.Column(
                controls=[
                    ft.Icon(ft.Icons.SEARCH_OFF, size=48, color=colors["text_secondary"]),
                    ft.Text(
                        "📋 Todas las audiencias se muestran por defecto. Use los filtros para buscar específicamente.",
                        size=16,
                        color=colors["text_secondary"],
                        text_align=ft.TextAlign.CENTER,
                    ),
                    ft.Text(
                        "Los resultados aparecerán aquí con opción de editar",
                        size=12,
                        color=colors["text_secondary"],
                        text_align=ft.TextAlign.CENTER,
                        italic=True,
                    ),
                ],
                horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                spacing=10,
            ),
            padding=ft.Padding(20, 40, 20, 40),
            alignment=ft.alignment.center,
        )
        
        # Contenedor principal de resultados - 🆕 SIN ALTURA FIJA para evitar truncamiento
        contenedor_resultados_principal = ft.Container(
            content=ft.Column(
                controls=[
                    self.indicador_carga_inicial,
                    self.indicador_carga_filtro,
                    self.texto_sin_resultados,
                    self.contenedor_resultados,
                ],
                spacing=5,
            ),
            # 🆕 REMOVIDA: height=600 - Causa truncamiento de resultados
            border=ft.border.all(1, colors["surface_border"]),
            border_radius=6,
            bgcolor=colors["surface_secondary"],
            padding=ft.Padding(10, 10, 10, 10),
            expand=True,  # 🆕 EXPANDIR para usar todo el espacio disponible
        )
        
        # Contenido del diálogo
        self.dialog = ft.AlertDialog(
            modal=True,
            title=ft.Text(
                "🔍 Búsqueda Avanzada y Edición",
                size=20,
                weight=ft.FontWeight.BOLD,
                color=colors["text_primary"]
            ),
            bgcolor=colors["surface_secondary"],
            content=ft.Container(
                content=ft.Column(
                    controls=[
                        # Sección de filtros
                        ft.Container(
                            content=ft.Column(
                                controls=[
                                    ft.Text(
                                        "🎯 Filtros:",
                                        size=14,
                                        weight=ft.FontWeight.W_600,
                                        color=colors["text_primary"],
                                    ),
                                    ft.Container(height=3),
                                    # Fila compacta de filtros - TODOS EN UNA FILA
                                    ft.Row(
                                        controls=[
                                            self.filtro_radicado,
                                            self.filtro_fecha_desde, 
                                            self.filtro_fecha_hasta,
                                            self.filtro_se_realizo,
                                            self.filtro_motivos,
                                        ],
                                        spacing=8,
                                        wrap=True,
                                    ),
                                    # Segunda fila más compacta
                                    ft.Row(
                                        controls=[
                                            self.filtro_tipo_audiencia,
                                            self.filtro_tipo_audiencia_otra,  # 🆕 Campo "Otra" (oculto inicialmente)
                                            btn_buscar,
                                            btn_limpiar,
                                        ],
                                        spacing=8,
                                        wrap=True,
                                    ),
                                ],
                                spacing=8,
                            ),
                            bgcolor=colors["surface_primary"],
                            border_radius=8,
                            padding=ft.Padding(15, 15, 15, 15),
                            margin=ft.Margin(0, 0, 0, 15),
                        ),
                        
                        # Sección de resultados
                        ft.Text(
                            "📋 Resultados:",
                            size=16,
                            weight=ft.FontWeight.W_600,
                            color=colors["text_primary"],
                        ),
                        contenedor_resultados_principal,
                    ],
                    tight=True,
                    spacing=6,
                    scroll=ft.ScrollMode.AUTO,  # Añadir scroll
                ),
                width=850,  # 🆕 AUMENTADO: de 750 a 850 para más espacio horizontal
                height=700,  # 🆕 AUMENTADO: de 650 a 700 para más espacio vertical
            ),
            actions=[btn_cerrar],
            actions_alignment=ft.MainAxisAlignment.END,
        )
        
        # Mostrar el diálogo
        self.page.overlay.append(self.dialog)
        self.dialog.open = True
        self.page.update()
        
        # NUEVO: Cargar y mostrar todos los datos automáticamente al abrir
        self._cargar_datos_iniciales()
    
    def _mostrar_carga_inicial(self, mostrar: bool = True):
        """Muestra u oculta el indicador de carga inicial."""
        self.indicador_carga_inicial.visible = mostrar
        self.texto_sin_resultados.visible = not mostrar
        self.contenedor_resultados.visible = not mostrar
        self.indicador_carga_filtro.visible = False
        try:
            self.page.update()
        except:
            pass
    
    def _mostrar_carga_filtro(self, mostrar: bool = True):
        """Muestra u oculta el indicador de carga de filtros."""
        self.indicador_carga_filtro.visible = mostrar
        self.texto_sin_resultados.visible = not mostrar
        self.contenedor_resultados.visible = not mostrar
        self.indicador_carga_inicial.visible = False
        try:
            self.page.update()
        except:
            pass
    
    def _cargar_datos_iniciales(self):
        """Carga y muestra todos los datos automáticamente al abrir la búsqueda."""
        # Mostrar indicador de carga inicial
        self._mostrar_carga_inicial(True)
        
        # Ejecutar carga en un hilo separado
        def cargar_en_hilo():
            try:
                time.sleep(0.5)  # Pequeña pausa para mostrar la carga
                
                print("=== DEBUG: Cargando datos iniciales ===")
                
                # Obtener todos los registros del Excel con sus filas reales
                registros = self.gestor_excel.leer_registros()
                print(f"=== DEBUG: Registros iniciales obtenidos: {len(registros) if registros else 0} registros ===")
                
                if not registros:
                    self._mostrar_carga_inicial(False)
                    self._mostrar_sin_resultados("No hay audiencias registradas en este archivo")
                    return
                
                # Los registros ya vienen con (fila_real, datos)
                print(f"=== DEBUG: Resultados iniciales preparados: {len(registros)} ===")
                
                # Ocultar indicador de carga y mostrar resultados
                self._mostrar_carga_inicial(False)
                
                if registros:
                    self._mostrar_resultados(registros)
                else:
                    self._mostrar_sin_resultados("No hay audiencias válidas en este archivo")
                    
            except Exception as ex:
                print(f"=== DEBUG: Error cargando datos iniciales: {str(ex)} ===")
                self._mostrar_carga_inicial(False)
                self._mostrar_sin_resultados(f"Error cargando datos: {str(ex)}")
        
        # Ejecutar en hilo separado
        thread = threading.Thread(target=cargar_en_hilo, daemon=True)
        thread.start()
    
    def _on_buscar(self, e):
        """Ejecuta la búsqueda con los filtros aplicados."""
        # Mostrar indicador de carga para filtros
        self._mostrar_carga_filtro(True)
        
        # Ejecutar búsqueda en un hilo separado
        def buscar_en_hilo():
            try:
                time.sleep(0.3)  # Pequeña pausa para mostrar la carga
                
                print("=== DEBUG: Iniciando búsqueda ===")
                
                # Obtener todos los registros del Excel con sus filas reales
                registros = self.gestor_excel.leer_registros()
                print(f"=== DEBUG: Registros obtenidos: {len(registros) if registros else 0} registros ===")
                
                if not registros:
                    self._mostrar_carga_filtro(False)
                    self._mostrar_sin_resultados("No hay datos en el archivo")
                    return
                
                # Mostrar primeras filas para debug
                if len(registros) > 0:
                    print(f"=== DEBUG: Primer registro (fila={registros[0][0]}): {registros[0][1][:5]}... ===")
                if len(registros) > 1:
                    print(f"=== DEBUG: Segundo registro (fila={registros[1][0]}): {registros[1][1][:5]}... ===")
                
                # Aplicar filtros - los registros ya vienen con (fila_real, datos)
                resultados_filtrados = self._filtrar_registros(registros)
                print(f"=== DEBUG: Registros filtrados: {len(resultados_filtrados)} ===")
                
                # Ocultar indicador de carga
                self._mostrar_carga_filtro(False)
                
                if not resultados_filtrados:
                    self._mostrar_sin_resultados("No se encontraron audiencias con estos filtros")
                    return
                
                # Mostrar resultados
                self._mostrar_resultados(resultados_filtrados)
                
            except Exception as ex:
                print(f"=== DEBUG: Error en búsqueda: {str(ex)} ===")
                self._mostrar_carga_filtro(False)
                self._mostrar_sin_resultados(f"Error en la búsqueda: {str(ex)}")
        
        # Ejecutar en hilo separado
        thread = threading.Thread(target=buscar_en_hilo, daemon=True)
        thread.start()
    
    def _filtrar_registros(self, registros: List[Tuple[int, List]]) -> List[Tuple[int, List]]:
        """Filtra los registros basándose en los criterios seleccionados."""
        resultados = []
        
        for fila_real, fila_datos in registros:
            if len(fila_datos) < 11:  # Verificar que tenga suficientes columnas
                continue
            
            # Aplicar filtros uno por uno
            if not self._cumple_filtros(fila_datos):
                continue
            
            resultados.append((fila_real, fila_datos))  # Usar fila_real del Excel
        
        return resultados
    
    def _cumple_filtros(self, fila: List) -> bool:
        """Verifica si una fila cumple todos los filtros."""
        
        # Filtro de radicado/nombre
        if self.filtro_radicado and self.filtro_radicado.value and self.filtro_radicado.value.strip():
            radicado = str(fila[1]).lower() if len(fila) > 1 else ""
            filtro_texto = self.filtro_radicado.value.strip().lower()
            if filtro_texto not in radicado:
                return False
        
        # Filtro de tipo de audiencia - 🆕 MEJORADO: Maneja opción "Otra" y "Todas"
        if self.filtro_tipo_audiencia and self.filtro_tipo_audiencia.value and self.filtro_tipo_audiencia.value.strip():
            tipo = str(fila[2]).lower() if len(fila) > 2 else ""
            filtro_tipo_seleccionado = self.filtro_tipo_audiencia.value.strip()
            
            # 🆕 Si se seleccionó "Todas" o cadena vacía, no aplicar filtro (mostrar todas)
            if filtro_tipo_seleccionado == "" or filtro_tipo_seleccionado.lower() in ["todas", "todos"]:
                pass  # No aplicar filtro, mostrar todas las audiencias
            # Si se seleccionó "Otra", usar el texto libre
            elif filtro_tipo_seleccionado == "Otra":
                if self.filtro_tipo_audiencia_otra and self.filtro_tipo_audiencia_otra.value and self.filtro_tipo_audiencia_otra.value.strip():
                    filtro_tipo = self.filtro_tipo_audiencia_otra.value.strip().lower()
                    if filtro_tipo not in tipo:
                        return False
                else:
                    return False  # "Otra" seleccionada pero sin texto específico
            else:
                # Filtro específico seleccionado
                filtro_tipo = filtro_tipo_seleccionado.lower()
                if filtro_tipo not in tipo:
                    return False
        
        # Filtro de se realizó - CORREGIDO según nueva estructura
        if self.filtro_se_realizo and self.filtro_se_realizo.value and self.filtro_se_realizo.value.strip():
            # Pos 6=SI, Pos 7=NO según nueva estructura (array base 0)
            realizada_si = str(fila[6]).upper() if len(fila) > 6 and fila[6] else ""
            realizada_no = str(fila[7]).upper() if len(fila) > 7 and fila[7] else ""
            
            if self.filtro_se_realizo.value == "SI":
                # Si busca "SI", debe tener "SI" en pos 6
                if realizada_si != "SI":
                    return False
            elif self.filtro_se_realizo.value == "NO":
                # Si busca "NO", debe tener "NO" en pos 7
                if realizada_no != "NO":
                    return False
        
        # Filtro de motivos - CORREGIDO según nueva estructura (pos 8-15 + observaciones pos 16)
        if self.filtro_motivos and self.filtro_motivos.value and self.filtro_motivos.value.strip():
            filtro_motivo = self.filtro_motivos.value.strip().lower()
            motivos_encontrados = False
            
            # Buscar en columnas de motivos (pos 8-15) + observaciones (pos 16)
            motivos_columnas = list(range(8, 16)) + [16]
            
            for col in motivos_columnas:
                if len(fila) > col and fila[col]:
                    motivo = str(fila[col]).lower()
                    if filtro_motivo in motivo:
                        motivos_encontrados = True
                        break
            
            if not motivos_encontrados:
                return False
        
        # Filtro de fecha desde
        if self.filtro_fecha_desde and self.filtro_fecha_desde.value and self.filtro_fecha_desde.value.strip():
            if not self._cumple_filtro_fecha(fila, "desde"):
                return False
        
        # Filtro de fecha hasta
        if self.filtro_fecha_hasta and self.filtro_fecha_hasta.value and self.filtro_fecha_hasta.value.strip():
            if not self._cumple_filtro_fecha(fila, "hasta"):
                return False
        
        return True
    
    def _cumple_filtro_fecha(self, fila: List, tipo_filtro: str) -> bool:
        """Verifica el filtro de fecha."""
        try:
            fecha_audiencia = str(fila[3]) if len(fila) > 3 else ""
            if not fecha_audiencia or fecha_audiencia == "":
                return True  # Si no hay fecha, no filtrar
            
            # Parsear fecha de la audiencia (formato DD/MM/YYYY)
            partes_fecha = fecha_audiencia.split("/")
            if len(partes_fecha) != 3:
                return True  # Si formato incorrecto, no filtrar
            
            fecha_aud = date(int(partes_fecha[2]), int(partes_fecha[1]), int(partes_fecha[0]))
            
            # Obtener fecha del filtro
            if tipo_filtro == "desde":
                fecha_filtro_str = self.filtro_fecha_desde.value.strip()
            else:  # hasta
                fecha_filtro_str = self.filtro_fecha_hasta.value.strip()
            
            partes_filtro = fecha_filtro_str.split("/")
            if len(partes_filtro) != 3:
                return True  # Si formato incorrecto, no filtrar
            
            fecha_filtro = date(int(partes_filtro[2]), int(partes_filtro[1]), int(partes_filtro[0]))
            
            # Aplicar filtro
            if tipo_filtro == "desde":
                return fecha_aud >= fecha_filtro
            else:  # hasta
                return fecha_aud <= fecha_filtro
                
        except (ValueError, IndexError):
            return True  # En caso de error, no filtrar
    
    def _mostrar_resultados(self, resultados: List[Tuple[int, List]]):
        """Muestra los resultados de la búsqueda."""
        try:
            colors = get_theme_colors()
            
            # Ocultar indicadores de carga
            self.indicador_carga_inicial.visible = False
            self.indicador_carga_filtro.visible = False
            self.texto_sin_resultados.visible = False
            self.contenedor_resultados.visible = True
            
            # Limpiar contenedor de manera segura
            if self.contenedor_resultados and hasattr(self.contenedor_resultados, 'controls'):
                self.contenedor_resultados.controls.clear()
            
            # Agregar header con contador mejorado - 🆕 CONTADOR DETALLADO
            header = ft.Container(
                content=ft.Column(
                    controls=[
                        ft.Text(
                            f"🎯 {len(resultados)} audiencias encontradas",
                            size=16,
                            weight=ft.FontWeight.W_600,
                            color=colors["primary"],
                        ),
                        ft.Text(
                            f"📋 Mostrando {len(resultados)} audiencias completas",
                            size=12,
                            color=colors["text_secondary"],
                            italic=True,
                        ),
                    ],
                    spacing=2,
                ),
                margin=ft.Margin(0, 0, 0, 10),
                padding=ft.Padding(8, 8, 8, 8),
                bgcolor=colors["surface_primary"],
                border_radius=6,
            )
            if self.contenedor_resultados and hasattr(self.contenedor_resultados, 'controls'):
                self.contenedor_resultados.controls.append(header)
            
            # Agregar cada resultado
            for fila_num, fila in resultados:
                card_resultado = self._crear_card_resultado(fila_num, fila, colors)
                if self.contenedor_resultados and hasattr(self.contenedor_resultados, 'controls'):
                    self.contenedor_resultados.controls.append(card_resultado)
            
            self.resultados_mostrados = resultados
            
            # Actualizar página de manera segura
            try:
                self.page.update()
            except:
                pass  # Ignorar errores de Event loop closed
                
        except Exception as e:
            print(f"=== DEBUG: Error mostrando resultados: {str(e)} ===")
    
    def _crear_card_resultado(self, fila_num: int, fila: List, colors: Dict) -> ft.Container:
        """Crea una tarjeta para mostrar un resultado."""
        
        # Extraer información principal con posiciones corregidas
        radicado = str(fila[1])[:50] + "..." if len(str(fila[1])) > 50 else str(fila[1])
        tipo = str(fila[2]) if len(fila) > 2 else "N/A"
        fecha = str(fila[3]) if len(fila) > 3 else "N/A"
        hora = str(fila[4]) if len(fila) > 4 and fila[4] else "N/A"
        juzgado = str(fila[5])[:30] + "..." if len(str(fila[5])) > 30 else str(fila[5]) if len(fila) > 5 else "N/A"
        
        # Determinar si se realizó (posiciones 6=SI, 7=NO en array base 0)
        realizada_si = str(fila[6]) if len(fila) > 6 and fila[6] else ""
        realizada_no = str(fila[7]) if len(fila) > 7 and fila[7] else ""
        
        if realizada_si == "SI":
            se_realizo = "SI"
        elif realizada_no == "NO":
            se_realizo = "NO"
        else:
            se_realizo = "N/A"
        
        # Iconos y colores según estado
        if se_realizo == "SI":
            icono_estado = "✅"
            color_estado = colors["success"]
        elif se_realizo == "NO":
            icono_estado = "❌"
            color_estado = colors["error"]
        else:
            icono_estado = "❓"
            color_estado = colors["text_secondary"]
        
        # Información de motivos si no se realizó (buscar en posiciones 8-15)
        info_adicional = ""
        if se_realizo == "NO":
            motivos_encontrados = []
            for i in range(8, 16):  # Posiciones 8-15 para motivos (array base 0)
                if len(fila) > i and fila[i]:
                    motivos_encontrados.append(str(fila[i]))
            if motivos_encontrados:
                info_adicional = f" (Motivo: {', '.join(motivos_encontrados)})"
        
        return ft.Container(
            content=ft.Row(
                controls=[
                    # Información principal
                    ft.Container(
                        content=ft.Column(
                            controls=[
                                ft.Row(
                                    controls=[
                                        ft.Text(
                                            f"#{fila[0] if fila[0] else fila_num}",
                                            size=12,
                                            weight=ft.FontWeight.W_600,
                                            color=colors["primary"],
                                        ),
                                        ft.Text(
                                            radicado,
                                            size=13,
                                            weight=ft.FontWeight.W_500,
                                            color=colors["text_primary"],
                                            expand=True,
                                        ),
                                    ],
                                    spacing=8,
                                ),
                                ft.Row(
                                    controls=[
                                        ft.Icon(ft.Icons.GAVEL, size=14, color=colors["text_secondary"]),
                                        ft.Text(tipo, size=11, color=colors["text_secondary"]),
                                        ft.Icon(ft.Icons.CALENDAR_TODAY, size=14, color=colors["text_secondary"]),
                                        ft.Text(fecha, size=11, color=colors["text_secondary"]),
                                        ft.Icon(ft.Icons.ACCESS_TIME, size=14, color=colors["text_secondary"]),
                                        ft.Text(hora, size=11, color=colors["text_secondary"]),
                                    ],
                                    spacing=5,
                                ),
                                ft.Row(
                                    controls=[
                                        ft.Icon(ft.Icons.ACCOUNT_BALANCE, size=14, color=colors["text_secondary"]),
                                        ft.Text(juzgado, size=11, color=colors["text_secondary"], expand=True),
                                        ft.Text(
                                            f"{icono_estado} {se_realizo}{info_adicional}",
                                            size=11,
                                            color=color_estado,
                                            weight=ft.FontWeight.W_500,
                                        ),
                                    ],
                                    spacing=5,
                                ),
                            ],
                            spacing=3,
                            tight=True,
                        ),
                        expand=True,
                        padding=ft.Padding(10, 8, 10, 8),
                    ),
                    
                    # Botón de edición mejorado
                    ft.Container(
                        content=ft.IconButton(
                            icon=ft.Icons.EDIT,
                            icon_size=16,  # Más pequeño
                            icon_color="#FFFFFF",  # Blanco para contraste
                            bgcolor="#FFA726",  # Amarillo/Naranja atractivo
                            tooltip="✏️ Editar esta audiencia",
                            width=32,  # Botón más pequeño
                            height=32,
                            style=ft.ButtonStyle(
                                shape=ft.CircleBorder(),
                                overlay_color={
                                    ft.ControlState.HOVERED: "#FFB74D",  # Amarillo más claro al hover
                                    ft.ControlState.PRESSED: "#FF9800",  # Amarillo más oscuro al presionar
                                },
                                elevation=2,
                            ),
                            on_click=lambda e, fn=fila_num, f=fila: self._on_editar_resultado(fn, f),
                        ),
                        padding=ft.Padding(3, 3, 3, 3),  # Padding reducido
                    ),
                ],
                spacing=5,
                vertical_alignment=ft.CrossAxisAlignment.CENTER,
            ),
            bgcolor=colors["surface_primary"],
            border=ft.border.all(1, colors["surface_border"]),
            border_radius=8,
            margin=ft.Margin(0, 2, 0, 2),
            ink=True,
        )
    
    def _mostrar_sin_resultados(self, mensaje: str):
        """Muestra mensaje cuando no hay resultados."""
        colors = get_theme_colors()
        
        # Ocultar indicadores de carga
        self.indicador_carga_inicial.visible = False
        self.indicador_carga_filtro.visible = False
        self.contenedor_resultados.visible = False
        self.texto_sin_resultados.visible = True
        
        self.contenedor_resultados.controls.clear()
        self.texto_sin_resultados.content.controls[1].value = mensaje
        self.page.update()
    
    def _on_editar_resultado(self, fila_num: int, fila: List):
        """Maneja el click en editar un resultado."""
        try:
            print(f"=== DEBUG: Editando audiencia - Fila: {fila_num}, Datos: {fila[:3]}... ===")
            
            # Cerrar ventana de búsqueda
            if self.dialog:
                self.dialog.open = False
                self.page.update()
                
                # Pequeña pausa para que se cierre la ventana
                time.sleep(0.1)
            
            # Llamar callback de edición con los datos
            if self.callback_editar:
                print(f"=== DEBUG: Llamando callback de edición ===")
                self.callback_editar(fila_num, fila)
            else:
                print("=== DEBUG ERROR: No hay callback de edición definido ===")
                
        except Exception as ex:
            print(f"=== DEBUG ERROR: Error en _on_editar_resultado: {str(ex)} ===")
            # Mostrar mensaje de error al usuario
            self._mostrar_mensaje_error(f"Error al editar: {str(ex)}")
    
    def _on_limpiar_filtros(self, e):
        """Limpia todos los filtros."""
        self.filtro_radicado.value = ""
        self.filtro_fecha_desde.value = ""
        self.filtro_fecha_hasta.value = ""
        self.filtro_se_realizo.value = ""
        self.filtro_motivos.value = ""
        self.filtro_motivos.disabled = False  # 🆕 Habilitar motivos al limpiar
        self.filtro_tipo_audiencia.value = ""
        self.filtro_tipo_audiencia_otra.value = ""  # 🆕 Limpiar campo "Otra"
        self.filtro_tipo_audiencia_otra.visible = False  # 🆕 Ocultar campo "Otra"
        
        # Limpiar resultados
        self.contenedor_resultados.controls.clear()
        self.texto_sin_resultados.content.controls[1].value = "Use los filtros de arriba para buscar audiencias"
        self.texto_sin_resultados.visible = True
        
        self.page.update()
    
    def _mostrar_mensaje_error(self, mensaje: str):
        """Muestra un mensaje de error usando AlertDialog."""
        colors = get_theme_colors()
        
        def cerrar_mensaje(e):
            dlg_error.open = False
            self.page.update()
        
        dlg_error = ft.AlertDialog(
            modal=True,
            title=ft.Text("Error", size=18, weight=ft.FontWeight.BOLD, color=colors["error"]),
            content=ft.Text(mensaje, size=14, color=colors["text_primary"]),
            actions=[
                ft.ElevatedButton(
                    "OK",
                    on_click=cerrar_mensaje,
                    style=ft.ButtonStyle(
                        bgcolor=colors["error"],
                        color=colors["text_on_primary"],
                        elevation=2,
                        shape=ft.RoundedRectangleBorder(radius=8),
                        padding=ft.Padding(20, 10, 20, 10),
                    ),
                ),
            ],
            actions_alignment=ft.MainAxisAlignment.END,
            bgcolor=colors["surface_card"],
        )
        
        self.page.overlay.append(dlg_error)
        dlg_error.open = True
        self.page.update()
    
    def _on_cerrar(self, e):
        """Cierra la ventana de búsqueda."""
        self.dialog.open = False
        self.page.update()
    
    # 🆕 NUEVOS MÉTODOS DE EVENTO
    
    def _on_se_realizo_change(self, e):
        """Habilita/deshabilita el campo de motivos según el estado de "¿Se realizó?"."""
        try:
            estado_seleccionado = e.control.value
            
            # Si se selecciona "SI", deshabilitar motivos (no aplican)
            if estado_seleccionado == "SI":
                self.filtro_motivos.disabled = True
                self.filtro_motivos.value = ""  # Limpiar selección
                
            # Si se selecciona "NO" o "Todos", habilitar motivos
            else:
                self.filtro_motivos.disabled = False
                
            self.page.update()
            
        except Exception as ex:
            print(f"Error en _on_se_realizo_change: {ex}")
    
    def _on_tipo_audiencia_change(self, e):
        """Muestra/oculta el campo de texto libre cuando se selecciona "Otra"."""
        try:
            tipo_seleccionado = e.control.value
            
            # Si se selecciona "Otra", mostrar campo de texto libre
            if tipo_seleccionado == "Otra":
                self.filtro_tipo_audiencia_otra.visible = True
            else:
                self.filtro_tipo_audiencia_otra.visible = False
                self.filtro_tipo_audiencia_otra.value = ""  # Limpiar campo
            
            self.page.update()
            
        except Exception as ex:
            print(f"Error en _on_tipo_audiencia_change: {ex}")
