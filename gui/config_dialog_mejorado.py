"""
Diálogo de configuración avanzado de la aplicación.
Implementa las mejores prácticas de Material Design para diálogos de configuración.
"""

import flet as ft
import os
import sqlite3
from typing import Optional, Callable
from gui.constants import get_theme_colors


class ConfigDialog:
    """Diálogo de configuración profesional con navegación por pestañas."""

    def __init__(self, page: ft.Page, callback: Optional[Callable] = None):
        self.page = page
        self.callback = callback
        self.dialog = None
        self.tab_index = 0
        self._crear_dialogo()

    def _crear_dialogo(self):
        """Crea el diálogo de configuración con pestañas."""
        colors = get_theme_colors()

        # Pestañas de navegación
        tabs = ft.Tabs(
            selected_index=0,
            animation_duration=200,
            tabs=[
                ft.Tab(
                    text="Cache",
                    icon=ft.Icons.STORAGE,
                    content=self._crear_tab_cache(),
                ),
                ft.Tab(
                    text="IA",
                    icon=ft.Icons.SMART_TOY,
                    content=self._crear_tab_ia(),
                ),
                ft.Tab(
                    text="Interfaz",
                    icon=ft.Icons.PALETTE,
                    content=self._crear_tab_interfaz(),
                ),
                ft.Tab(
                    text="Avanzado",
                    icon=ft.Icons.SETTINGS_APPLICATIONS,
                    content=self._crear_tab_avanzado(),
                ),
            ],
            expand=1,
        )

        # Botones de acción
        actions_row = ft.Row(
            controls=[
                ft.TextButton(
                    text="Restaurar por defecto",
                    icon=ft.Icons.RESTORE,
                    on_click=self._on_restaurar_defecto,
                    style=ft.ButtonStyle(
                        color=colors["text_secondary"],
                    ),
                ),
                ft.Container(expand=1),  # Spacer
                ft.TextButton(
                    text="Cancelar",
                    on_click=self._on_cancelar,
                    style=ft.ButtonStyle(
                        color=colors["text_secondary"],
                    ),
                ),
                ft.ElevatedButton(
                    text="Aplicar",
                    icon=ft.Icons.CHECK,
                    on_click=self._on_aplicar,
                    style=ft.ButtonStyle(
                        bgcolor=colors["primary"],
                        color=colors["text_on_primary"],
                        elevation=2,
                        shape=ft.RoundedRectangleBorder(radius=8),
                    ),
                ),
            ],
            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
        )

        # Contenido principal
        main_content = ft.Container(
            content=ft.Column(
                controls=[
                    # Encabezado
                    ft.Container(
                        content=ft.Row(
                            controls=[
                                ft.Icon(
                                    ft.Icons.SETTINGS,
                                    size=32,
                                    color=colors["primary"],
                                ),
                                ft.Column(
                                    controls=[
                                        ft.Text(
                                            "Configuración",
                                            size=24,
                                            weight=ft.FontWeight.BOLD,
                                            color=colors["text_primary"],
                                        ),
                                        ft.Text(
                                            "Personaliza tu experiencia en el Gestor de Audiencias",
                                            size=14,
                                            color=colors["text_secondary"],
                                        ),
                                    ],
                                    spacing=4,
                                    expand=1,
                                ),
                            ],
                            spacing=16,
                        ),
                        padding=ft.Padding(0, 0, 0, 20),
                    ),
                    # Pestañas de contenido
                    ft.Container(
                        content=tabs,
                        expand=1,
                    ),
                    # Separador
                    ft.Divider(color=colors["surface_border"]),
                    # Botones de acción
                    actions_row,
                ],
                spacing=0,
            ),
            width=700,
            height=550,
            padding=ft.Padding(24, 24, 24, 24),
        )

        # Crear el diálogo
        self.dialog = ft.AlertDialog(
            modal=True,
            title=None,
            content=main_content,
            actions=[],
            actions_alignment=ft.MainAxisAlignment.CENTER,
            content_padding=ft.Padding(0, 0, 0, 0),
            shape=ft.RoundedRectangleBorder(radius=16),
        )

    def _crear_tab_cache(self):
        """Crea la pestaña de gestión de cache."""
        colors = get_theme_colors()
        cache_info = self._obtener_info_cache()

        # Información del cache
        cache_status = self._crear_info_card(
            titulo="Estado del Cache",
            contenido=[
                ft.Text(cache_info["texto"], size=14, color=colors["text_primary"]),
                ft.Row(
                    controls=[
                        ft.Icon(
                            ft.Icons.CIRCLE,
                            size=12,
                            color=(
                                colors["success"]
                                if cache_info["existe"]
                                else colors["warning"]
                            ),
                        ),
                        ft.Text(
                            "Activo" if cache_info["existe"] else "Inactivo",
                            size=12,
                            weight=ft.FontWeight.W_500,
                            color=(
                                colors["success"]
                                if cache_info["existe"]
                                else colors["warning"]
                            ),
                        ),
                    ],
                    spacing=8,
                ),
            ],
            icono=ft.Icons.INFO_OUTLINE,
        )

        # Botones de acción
        botones_cache = ft.Container(
            content=ft.Column(
                controls=[
                    ft.ElevatedButton(
                        text="Limpiar Cache",
                        icon=ft.Icons.CLEANING_SERVICES,
                        width=200,
                        on_click=self._on_limpiar_cache,
                        style=ft.ButtonStyle(
                            bgcolor=colors["warning"],
                            color=colors["text_on_primary"],
                            elevation=1,
                            shape=ft.RoundedRectangleBorder(radius=8),
                        ),
                    ),
                    ft.Text(
                        "⚠️ Esto forzará a la IA a procesar todos los casos con las reglas más recientes",
                        size=12,
                        color=colors["text_secondary"],
                        italic=True,
                    ),
                ],
                horizontal_alignment=ft.CrossAxisAlignment.START,
                spacing=8,
            ),
            padding=ft.Padding(0, 16, 0, 0),
        )

        return ft.Container(
            content=ft.Column(
                controls=[
                    cache_status,
                    ft.Container(height=16),  # Spacer
                    botones_cache,
                ],
                scroll=ft.ScrollMode.AUTO,
            ),
            padding=ft.Padding(16, 16, 16, 16),
        )

    def _crear_tab_ia(self):
        """Crea la pestaña de configuración de IA."""
        colors = get_theme_colors()

        config_ia = self._crear_info_card(
            titulo="Configuración Actual",
            contenido=[
                self._crear_config_item("Modelo", "GPT-4o-mini", "🤖"),
                self._crear_config_item("Cache persistente", "Habilitado", "💾"),
                self._crear_config_item("Auto-entrenamiento", "9 casos reales", "📚"),
                self._crear_config_item("Precisión", "Optimizada", "🎯"),
            ],
            icono=ft.Icons.PSYCHOLOGY,
        )

        estadisticas = self._crear_info_card(
            titulo="Estadísticas de Uso",
            contenido=[
                self._crear_config_item(
                    "Consultas en cache", f"{self._contar_cache()} respuestas", "📊"
                ),
                self._crear_config_item("Última actualización", "Hoy", "🕒"),
                self._crear_config_item(
                    "Tipos de audiencia", "9 tipos soportados", "⚖️"
                ),
            ],
            icono=ft.Icons.ANALYTICS,
        )

        return ft.Container(
            content=ft.Column(
                controls=[
                    config_ia,
                    ft.Container(height=16),
                    estadisticas,
                ],
                scroll=ft.ScrollMode.AUTO,
            ),
            padding=ft.Padding(16, 16, 16, 16),
        )

    def _crear_tab_interfaz(self):
        """Crea la pestaña de configuración de interfaz."""
        colors = get_theme_colors()

        tema_config = self._crear_info_card(
            titulo="Apariencia",
            contenido=[
                self._crear_config_item("Tema", "Automático (Claro/Oscuro)", "🎨"),
                self._crear_config_item("Animaciones", "Habilitadas", "✨"),
                self._crear_config_item("Efectos visuales", "Activados", "🌟"),
            ],
            icono=ft.Icons.PALETTE,
        )

        atajos_config = self._crear_info_card(
            titulo="Navegación",
            contenido=[
                self._crear_config_item("Atajos de teclado", "F1 para ayuda", "⌨️"),
                self._crear_config_item("Auto-guardado", "Habilitado", "💾"),
                self._crear_config_item("Confirmaciones", "Activadas", "✅"),
            ],
            icono=ft.Icons.KEYBOARD,
        )

        return ft.Container(
            content=ft.Column(
                controls=[
                    tema_config,
                    ft.Container(height=16),
                    atajos_config,
                ],
                scroll=ft.ScrollMode.AUTO,
            ),
            padding=ft.Padding(16, 16, 16, 16),
        )

    def _crear_tab_avanzado(self):
        """Crea la pestaña de configuración avanzada."""
        colors = get_theme_colors()

        sistema_config = self._crear_info_card(
            titulo="Sistema",
            contenido=[
                self._crear_config_item("Versión", "2.0 (Optimizada)", "🔧"),
                self._crear_config_item("Rendimiento", "Alto", "⚡"),
                self._crear_config_item("Seguridad", "Máxima", "🔒"),
            ],
            icono=ft.Icons.SETTINGS_APPLICATIONS,
        )

        datos_config = self._crear_info_card(
            titulo="Gestión de Datos",
            contenido=[
                self._crear_config_item("Anonimización", "Automática", "🛡️"),
                self._crear_config_item("Validación", "Completa", "✔️"),
                self._crear_config_item("Exportación", "Excel optimizado", "📄"),
            ],
            icono=ft.Icons.DATA_OBJECT,
        )

        return ft.Container(
            content=ft.Column(
                controls=[
                    sistema_config,
                    ft.Container(height=16),
                    datos_config,
                ],
                scroll=ft.ScrollMode.AUTO,
            ),
            padding=ft.Padding(16, 16, 16, 16),
        )

    def _crear_info_card(self, titulo: str, contenido: list, icono: str):
        """Crea una tarjeta de información estilizada."""
        colors = get_theme_colors()

        return ft.Container(
            content=ft.Column(
                controls=[
                    ft.Row(
                        controls=[
                            ft.Icon(icono, size=20, color=colors["primary"]),
                            ft.Text(
                                titulo,
                                size=16,
                                weight=ft.FontWeight.W_600,
                                color=colors["text_primary"],
                            ),
                        ],
                        spacing=12,
                    ),
                    ft.Container(height=12),
                    ft.Column(
                        controls=contenido,
                        spacing=8,
                    ),
                ],
                spacing=0,
            ),
            padding=ft.Padding(20, 20, 20, 20),
            bgcolor=colors["surface_primary"],
            border_radius=12,
            border=ft.border.all(1, colors["surface_border"]),
        )

    def _crear_config_item(self, etiqueta: str, valor: str, emoji: str = ""):
        """Crea un elemento de configuración."""
        colors = get_theme_colors()

        return ft.Row(
            controls=[
                ft.Text(emoji, size=16) if emoji else ft.Container(width=0),
                ft.Text(
                    etiqueta,
                    size=14,
                    color=colors["text_secondary"],
                    expand=1,
                ),
                ft.Text(
                    valor,
                    size=14,
                    weight=ft.FontWeight.W_500,
                    color=colors["text_primary"],
                ),
            ],
            spacing=8,
            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
        )

    def _obtener_info_cache(self):
        """Obtiene información detallada sobre el estado del cache."""
        cache_path = "ai_cache.db"

        if os.path.exists(cache_path):
            try:
                size_bytes = os.path.getsize(cache_path)
                size_kb = size_bytes / 1024

                conn = sqlite3.connect(cache_path)
                cursor = conn.cursor()
                cursor.execute("SELECT COUNT(*) FROM cache")
                count = cursor.fetchone()[0]
                conn.close()

                return {
                    "existe": True,
                    "texto": f"{count} respuestas guardadas ({size_kb:.1f} KB)",
                    "count": count,
                    "size": size_kb,
                }

            except Exception:
                return {
                    "existe": False,
                    "texto": "Cache presente pero no se puede leer",
                    "count": 0,
                    "size": 0,
                }
        else:
            return {
                "existe": False,
                "texto": "Cache vacío - todas las consultas serán nuevas",
                "count": 0,
                "size": 0,
            }

    def _contar_cache(self):
        """Cuenta los elementos en cache."""
        info = self._obtener_info_cache()
        return info["count"]

    def _on_limpiar_cache(self, e):
        """Maneja la limpieza del cache con confirmación mejorada."""
        colors = get_theme_colors()

        def confirmar_limpieza(e):
            self._limpiar_cache()
            self.page.close(dlg_confirmacion)

        def cancelar_limpieza(e):
            self.page.close(dlg_confirmacion)

        dlg_confirmacion = ft.AlertDialog(
            modal=True,
            title=ft.Row(
                controls=[
                    ft.Icon(ft.Icons.WARNING_AMBER, color=colors["warning"], size=24),
                    ft.Text(
                        "Confirmar Limpieza", color=colors["text_primary"], size=18
                    ),
                ],
                spacing=12,
            ),
            content=ft.Container(
                content=ft.Column(
                    controls=[
                        ft.Text(
                            "¿Estás seguro de que deseas limpiar el cache de IA?",
                            size=16,
                            weight=ft.FontWeight.W_500,
                            color=colors["text_primary"],
                        ),
                        ft.Container(height=8),
                        ft.Text(
                            "• La próxima consulta tomará más tiempo\n"
                            "• Se garantiza el uso de las reglas más recientes\n"
                            "• Esta acción no se puede deshacer",
                            size=14,
                            color=colors["text_secondary"],
                        ),
                    ],
                    spacing=0,
                    tight=True,
                ),
                width=400,
            ),
            actions=[
                ft.TextButton(
                    text="Cancelar",
                    on_click=cancelar_limpieza,
                    style=ft.ButtonStyle(color=colors["text_secondary"]),
                ),
                ft.ElevatedButton(
                    text="Sí, limpiar cache",
                    on_click=confirmar_limpieza,
                    style=ft.ButtonStyle(
                        bgcolor=colors["warning"],
                        color=colors["text_on_primary"],
                    ),
                ),
            ],
            actions_alignment=ft.MainAxisAlignment.END,
            shape=ft.RoundedRectangleBorder(radius=12),
        )

        self.page.open(dlg_confirmacion)

    def _limpiar_cache(self):
        """Limpia el cache de IA."""
        cache_files = ["ai_cache.db", "ai_cache.db-journal", "ai_cache.db-wal"]

        limpiados = 0
        for cache_file in cache_files:
            if os.path.exists(cache_file):
                try:
                    os.remove(cache_file)
                    limpiados += 1
                except Exception as e:
                    print(f"Error limpiando {cache_file}: {e}")

        if limpiados > 0:
            self._mostrar_notificacion(
                "✅ Cache limpiado exitosamente", colors=get_theme_colors()["success"]
            )
        else:
            self._mostrar_notificacion(
                "ℹ️ No había cache que limpiar", colors=get_theme_colors()["primary"]
            )

    def _on_restaurar_defecto(self, e):
        """Restaura configuraciones por defecto."""
        self._mostrar_notificacion(
            "🔄 Configuración restaurada por defecto",
            colors=get_theme_colors()["primary"],
        )

    def _on_cancelar(self, e):
        """Cancela y cierra el diálogo."""
        self.page.close(self.dialog)
        if self.callback:
            self.callback()

    def _on_aplicar(self, e):
        """Aplica cambios y cierra el diálogo."""
        self._mostrar_notificacion(
            "✅ Configuración aplicada", colors=get_theme_colors()["success"]
        )
        self.page.close(self.dialog)
        if self.callback:
            self.callback()

    def _mostrar_notificacion(self, mensaje: str, colors: str):
        """Muestra una notificación temporal."""
        snack = ft.SnackBar(
            content=ft.Text(mensaje, color="white"),
            bgcolor=colors,
            duration=3000,
            behavior=ft.SnackBarBehavior.FLOATING,
            margin=ft.Margin(20, 20, 20, 20),
            shape=ft.RoundedRectangleBorder(radius=8),
        )
        self.page.overlay.append(snack)
        snack.open = True
        self.page.update()

    def mostrar(self):
        """Muestra el diálogo de configuración."""
        self.page.open(self.dialog)


def mostrar_dialogo_configuracion(page: ft.Page, callback: Optional[Callable] = None):
    """Función de utilidad para mostrar el diálogo de configuración."""
    config_dialog = ConfigDialog(page, callback)
    config_dialog.mostrar()
