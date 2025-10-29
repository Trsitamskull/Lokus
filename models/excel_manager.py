from openpyxl import load_workbook
from datetime import datetime
from typing import List, Tuple, Optional, Dict
from .audiencia import Audiencia


class ExcelManager:
    """Gestiona las operaciones con archivos Excel."""

    FILA_INICIO_DATOS = 11
    FILA_MAXIMA_DATOS_PARA_LIMPIAR = 110
    FILA_TOTALES = 111

    def __init__(self, archivo_path: str):
        self.archivo_path = archivo_path

    def guardar_audiencia(self, audiencia: Audiencia) -> bool:
        """Guarda una nueva audiencia en el archivo Excel."""
        try:
            wb = load_workbook(self.archivo_path)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            # Encontrar la primera fila vacía
            fila_destino = self.FILA_INICIO_DATOS
            while ws.cell(row=fila_destino, column=2).value is not None:
                fila_destino += 1

            # Mapeo de motivos a índices de columna (I=0, J=1, etc.)
            MOTIVOS_MAP = {
                "juez": 0,
                "fiscalía": 1,
                "usuario": 2,
                "inpec": 3,
                "víctima": 4,
                "icbf": 5,
                "defensor confianza": 6,
                "defensor público": 7,
            }

            # Escribir los datos con mapeo correcto de motivos
            self._escribir_fila_audiencia_con_mapeo_motivos(
                ws, fila_destino, audiencia, MOTIVOS_MAP
            )

            # Calcular y escribir totales (incluyendo total de NO en columna Q)
            num_registros, total_si, total_no, totales_motivos = self._calcular_totales(
                ws
            )
            self._escribir_totales(ws, total_si, total_no, totales_motivos)

            wb.save(self.archivo_path)
            return True

        except PermissionError:
            raise Exception(
                "No se pudo guardar. Cierre el archivo de Excel si está abierto."
            )
        except Exception as e:
            raise Exception(f"Error al guardar: {e}")

    def actualizar_audiencia(self, fila: int, audiencia: Audiencia) -> bool:
        """Actualiza una audiencia existente."""
        try:
            wb = load_workbook(self.archivo_path)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            # Mapeo de motivos a índices de columna (I=0, J=1, etc.)
            MOTIVOS_MAP = {
                "juez": 0,
                "fiscalía": 1,
                "usuario": 2,
                "inpec": 3,
                "víctima": 4,
                "icbf": 5,
                "defensor confianza": 6,
                "defensor público": 7,
            }

            self._escribir_fila_audiencia_con_mapeo_motivos(
                ws, fila, audiencia, MOTIVOS_MAP
            )

            # Calcular y escribir totales (incluyendo total de NO en columna Q)
            num_registros, total_si, total_no, totales_motivos = self._calcular_totales(
                ws
            )
            self._escribir_totales(ws, total_si, total_no, totales_motivos)

            wb.save(self.archivo_path)
            return True

        except Exception as e:
            raise Exception(f"Error al actualizar: {e}")

    def guardar_audiencias_masivo(
        self, audiencias: List[Audiencia]
    ) -> Tuple[int, int, List[str]]:
        """Guarda múltiples audiencias de forma masiva y actualiza totales automáticamente."""
        try:
            wb = load_workbook(self.archivo_path)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            exitosas = 0
            errores = []

            # Mapeo de motivos a índices de columna (I=0, J=1, etc.)
            MOTIVOS_MAP = {
                "juez": 0,
                "fiscalía": 1,
                "usuario": 2,
                "inpec": 3,
                "víctima": 4,
                "icbf": 5,
                "defensor confianza": 6,
                "defensor público": 7,
            }

            for i, audiencia in enumerate(audiencias):
                try:
                    # Encontrar la primera fila disponible
                    fila_destino = None
                    for row_num in range(
                        self.FILA_INICIO_DATOS, self.FILA_MAXIMA_DATOS_PARA_LIMPIAR + 1
                    ):
                        if not ws.cell(
                            row=row_num, column=2
                        ).value:  # Columna B (radicado)
                            fila_destino = row_num
                            break

                    if fila_destino is None:
                        errores.append(
                            f"Audiencia {i+1}: No hay espacio disponible en Excel"
                        )
                        continue

                    # Escribir datos básicos usando el método existente
                    self._escribir_fila_audiencia_con_mapeo_motivos(
                        ws, fila_destino, audiencia, MOTIVOS_MAP
                    )
                    exitosas += 1

                except Exception as ex:
                    errores.append(f"Audiencia {i+1}: {str(ex)}")
                    continue

            # ACTUALIZAR TOTALES AUTOMÁTICAMENTE
            num_registros, total_si, total_no, totales_motivos = self._calcular_totales(
                ws
            )
            self._escribir_totales(ws, total_si, total_no, totales_motivos)

            wb.save(self.archivo_path)
            return exitosas, len(errores), errores

        except PermissionError:
            raise Exception(
                "No se pudo guardar. Cierre el archivo de Excel si está abierto."
            )
        except Exception as e:
            raise Exception(f"Error en procesamiento masivo: {e}")

    def _escribir_fila_audiencia_con_mapeo_motivos(
        self, ws, fila: int, audiencia: Audiencia, motivos_map: Dict
    ):
        """Escribe una audiencia con mapeo correcto de motivos a columnas específicas."""

        # LIMPIEZA FORZADA Y EXPLÍCITA - Solución para bug de persistencia
        # Limpiar cada celda individualmente con diferentes métodos
        for col in range(1, 18):
            try:
                ws.cell(row=fila, column=col, value="")
                ws.cell(row=fila, column=col, value=None)
                ws.cell(row=fila, column=col, value="")
            except Exception:
                continue

        # Escribir datos básicos
        ws.cell(
            row=fila, column=1, value=fila - self.FILA_INICIO_DATOS + 1
        )  # Número consecutivo
        ws.cell(row=fila, column=2, value=audiencia.radicado)
        # Convertir "(Vacío)" a celda vacía real
        tipo_valor = None if audiencia.tipo == "(Vacío)" else audiencia.tipo
        ws.cell(row=fila, column=3, value=tipo_valor)

        # Escribir FECHA en columna D - Si está vacía, dejar celda vacía
        fecha_valor = (
            audiencia.fecha if audiencia.fecha and audiencia.fecha.strip() else None
        )
        ws.cell(row=fila, column=4, value=fecha_valor)

        # Escribir HORA en columna E - Si está vacía, dejar celda vacía
        hora_valor = None
        if audiencia.hora and audiencia.hora.strip() and audiencia.hora != ":":
            hora_valor = audiencia.hora
        ws.cell(row=fila, column=5, value=hora_valor)

        # Juzgado
        ws.cell(row=fila, column=6, value=audiencia.juzgado)

        # LIMPIEZA EXPLÍCITA Y FORZADA DE SI/NO - Solución para persistencia
        # Limpiar múltiples veces con diferentes métodos
        for intento in range(3):  # 3 intentos de limpieza
            try:
                ws.cell(row=fila, column=7, value="")  # String vacío primero
                ws.cell(row=fila, column=8, value="")  # String vacío primero
                ws.cell(row=fila, column=7, value=None)  # Luego None
                ws.cell(row=fila, column=8, value=None)  # Luego None
            except Exception:
                continue

        # Escribir SOLO el valor necesario para SI/NO
        if audiencia.realizada_si == "SI":
            ws.cell(row=fila, column=7, value="SI")  # Solo escribir SI
            # Asegurar que NO esté vacío
            ws.cell(row=fila, column=8, value=None)
            ws.cell(row=fila, column=8, value="")
            ws.cell(row=fila, column=8, value=None)
        elif audiencia.realizada_no == "NO":
            ws.cell(row=fila, column=8, value="NO")  # Solo escribir NO
            # Asegurar que SI esté vacío
            ws.cell(row=fila, column=7, value=None)
            ws.cell(row=fila, column=7, value="")
            ws.cell(row=fila, column=7, value=None)

        # LIMPIEZA EXPLÍCITA DE MOTIVOS - Limpiar todas las columnas de motivos primero
        for i in range(8):  # Limpiar columnas 9-16 (motivos)
            for intento in range(3):  # 3 intentos de limpieza por columna
                try:
                    ws.cell(row=fila, column=9 + i, value="")
                    ws.cell(row=fila, column=9 + i, value=None)
                except Exception:
                    continue

        # MAPEO CORRECTO DE MOTIVOS A COLUMNAS ESPECÍFICAS
        for motivo in audiencia.motivos:
            if motivo and motivo.strip():
                motivo_lower = motivo.lower().strip()

                # Buscar el motivo en el mapeo
                columna_idx = None
                for motivo_key, idx in motivos_map.items():
                    if motivo_key in motivo_lower or motivo_lower in motivo_key:
                        columna_idx = idx
                        break

                # Si se encontró el motivo, escribirlo en la columna correcta
                if columna_idx is not None:
                    ws.cell(row=fila, column=9 + columna_idx, value=motivo.strip())

        # Escribir observaciones en columna Q (17)
        ws.cell(row=fila, column=17, value=audiencia.observaciones)

    def _escribir_fila_audiencia(self, ws, fila: int, audiencia: Audiencia):
        """Escribe los datos de una audiencia en una fila específica."""
        # LIMPIEZA FORZADA Y EXPLÍCITA - Solución para bug de persistencia
        # Limpiar cada celda individualmente con diferentes métodos
        for col in range(1, 18):  # Columnas A-Q
            try:
                # Método 1: Asignar None
                ws.cell(row=fila, column=col, value=None)
                # Método 2: Asignar string vacío para forzar limpieza
                ws.cell(row=fila, column=col, value="")
                # Método 3: Volver a None
                ws.cell(row=fila, column=col, value=None)
            except Exception:
                # Si hay problema con la celda, intentar con string vacío solamente
                try:
                    ws.cell(row=fila, column=col, value="")
                    ws.cell(row=fila, column=col, value=None)
                except Exception:
                    pass  # Si no se puede limpiar, continuar

        # Escribir datos básicos
        ws.cell(
            row=fila, column=1, value=fila - self.FILA_INICIO_DATOS + 1
        )  # Número consecutivo
        ws.cell(row=fila, column=2, value=audiencia.radicado)
        # Convertir "(Vacío)" a celda vacía real
        tipo_valor = None if audiencia.tipo == "(Vacío)" else audiencia.tipo
        ws.cell(row=fila, column=3, value=tipo_valor)

        # Escribir FECHA en columna D - Si está vacía, dejar celda vacía
        fecha_valor = (
            audiencia.fecha if audiencia.fecha and audiencia.fecha.strip() else None
        )
        ws.cell(row=fila, column=4, value=fecha_valor)

        # Escribir HORA en columna E - Si está vacía, dejar celda vacía
        hora_valor = None
        if audiencia.hora and audiencia.hora.strip() and audiencia.hora != ":":
            try:
                if ":" in str(audiencia.hora):
                    # Ya está en formato correcto
                    hora_valor = str(audiencia.hora)
                else:
                    # Convertir número a formato hora
                    hora_num = int(audiencia.hora)
                    hora_valor = f"{hora_num:02d}:00"
            except (ValueError, TypeError):
                hora_valor = None
        ws.cell(row=fila, column=5, value=hora_valor)

        # Escribir JUZGADO en columna F
        ws.cell(row=fila, column=6, value=audiencia.juzgado)

        # LIMPIEZA EXPLÍCITA Y FORZADA DE SI/NO - Solución para persistencia
        # Limpiar múltiples veces con diferentes métodos
        for intento in range(3):  # 3 intentos de limpieza
            try:
                ws.cell(row=fila, column=7, value="")  # String vacío primero
                ws.cell(row=fila, column=8, value="")  # String vacío primero
                ws.cell(row=fila, column=7, value=None)  # Luego None
                ws.cell(row=fila, column=8, value=None)  # Luego None
            except Exception:
                continue

        # Escribir SOLO el valor necesario para SI/NO
        if audiencia.realizada_si == "SI":
            ws.cell(row=fila, column=7, value="SI")  # Solo escribir SI
            # Asegurar que NO esté vacío
            ws.cell(row=fila, column=8, value=None)
            ws.cell(row=fila, column=8, value="")
            ws.cell(row=fila, column=8, value=None)
        elif audiencia.realizada_no == "NO":
            ws.cell(row=fila, column=8, value="NO")  # Solo escribir NO
            # Asegurar que SI esté vacío
            ws.cell(row=fila, column=7, value=None)
            ws.cell(row=fila, column=7, value="")
            ws.cell(row=fila, column=7, value=None)

        # LIMPIEZA EXPLÍCITA DE MOTIVOS - Limpiar todas las columnas de motivos primero
        for i in range(8):  # Limpiar columnas 9-16 (motivos)
            for intento in range(3):  # 3 intentos de limpieza por columna
                try:
                    ws.cell(row=fila, column=9 + i, value="")
                    ws.cell(row=fila, column=9 + i, value=None)
                except Exception:
                    continue

        # Escribir solo los motivos que tienen contenido real
        for i, motivo in enumerate(audiencia.motivos):
            if (
                i < 8 and motivo and motivo.strip()
            ):  # Solo escribir si hay contenido real
                ws.cell(row=fila, column=9 + i, value=motivo)

        # Escribir observaciones en columna Q (17)
        ws.cell(row=fila, column=17, value=audiencia.observaciones)

    def leer_registros(self) -> List[Tuple[int, List]]:
        """Lee todos los registros del archivo."""
        try:
            wb = load_workbook(self.archivo_path, read_only=True)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            registros = []
            for fila_num in range(self.FILA_INICIO_DATOS, ws.max_row + 1):
                datos_fila = [cell.value for cell in ws[fila_num]]
                if datos_fila[1]:  # Si hay radicado
                    registros.append((fila_num, datos_fila))

            return registros

        except Exception as e:
            raise Exception(f"Error al leer registros: {e}")

    def reordenar_y_guardar(self) -> Tuple[int, int, List[int]]:
        """Reordena todos los registros por fecha/hora y calcula totales."""
        try:
            wb = load_workbook(self.archivo_path)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            # Leer y ordenar datos - SOLO leer datos válidos
            datos_tabla = []
            for row_num in range(self.FILA_INICIO_DATOS, ws.max_row + 1):
                # Leer fila completa
                fila_datos = []
                for col in range(1, 18):  # Columnas A-Q
                    valor = ws.cell(row=row_num, column=col).value
                    fila_datos.append(valor if valor is not None else "")

                # Solo procesar si hay radicado (columna B)
                if not fila_datos[1]:
                    continue

                try:
                    # Parsear fecha (columna D)
                    fecha_str = str(fila_datos[3]) if fila_datos[3] else ""
                    if not fecha_str:
                        continue
                    fecha_obj = datetime.strptime(fecha_str, "%d/%m/%Y")

                    # Parsear hora (columna E) - puede ser texto "HH:MM" o número
                    hora_str = str(fila_datos[4]) if fila_datos[4] else "00:00"

                    # Limpiar formato de hora para manejar "HH:MM:SS" o "HH:MM"
                    if ":" in hora_str:
                        # Si tiene formato HH:MM:SS, tomar solo HH:MM
                        partes_hora = hora_str.split(":")
                        hora_limpia = f"{partes_hora[0]}:{partes_hora[1]}"
                        hora_obj = datetime.strptime(hora_limpia, "%H:%M")
                    else:
                        # Si es un número, convertir a formato hora
                        try:
                            hora_num = int(float(hora_str))
                            hora_obj = datetime.strptime(f"{hora_num:02d}:00", "%H:%M")
                        except:
                            hora_obj = datetime.strptime("00:00", "%H:%M")

                    datos_tabla.append((fecha_obj, hora_obj, fila_datos))

                except (ValueError, TypeError) as e:
                    print(f"Error parseando fila {row_num}: {e}")
                    continue

            # Ordenar por fecha DESC y hora DESC (más recientes primero)
            datos_tabla.sort(key=lambda x: (x[0], x[1]), reverse=True)

            # Limpiar área de datos COMPLETAMENTE
            self._limpiar_area_datos(ws)

            # Escribir datos ordenados usando el método correcto
            for idx, (_, _, fila_datos) in enumerate(datos_tabla, start=1):
                fila_destino = self.FILA_INICIO_DATOS + idx - 1

                # Limpiar fila destino primero
                for col in range(1, 18):
                    ws.cell(row=fila_destino, column=col, value=None)

                # Escribir datos en posiciones correctas
                ws.cell(row=fila_destino, column=1, value=idx)  # Número consecutivo
                ws.cell(row=fila_destino, column=2, value=fila_datos[1])  # Radicado
                ws.cell(row=fila_destino, column=3, value=fila_datos[2])  # Tipo
                ws.cell(row=fila_destino, column=4, value=fila_datos[3])  # Fecha
                ws.cell(row=fila_destino, column=5, value=fila_datos[4])  # Hora
                ws.cell(row=fila_destino, column=6, value=fila_datos[5])  # Juzgado
                ws.cell(row=fila_destino, column=7, value=fila_datos[6])  # SI
                ws.cell(row=fila_destino, column=8, value=fila_datos[7])  # NO

                # Escribir motivos (columnas 9-16)
                for i in range(8):
                    motivo_idx = 8 + i
                    if motivo_idx < len(fila_datos):
                        ws.cell(
                            row=fila_destino, column=9 + i, value=fila_datos[motivo_idx]
                        )

                # Escribir observaciones (columna 17)
                if len(fila_datos) > 16:
                    ws.cell(row=fila_destino, column=17, value=fila_datos[16])

            # Calcular y escribir totales
            num_registros, total_si, total_no, totales_motivos = self._calcular_totales(
                ws
            )
            self._escribir_totales(ws, total_si, total_no, totales_motivos)

            wb.save(self.archivo_path)
            return num_registros, total_si, totales_motivos

        except Exception as e:
            raise Exception(f"Error al reordenar: {e}")

    def _limpiar_area_datos(self, ws):
        """Limpia el área de datos en la hoja."""
        for i in range(self.FILA_INICIO_DATOS, self.FILA_MAXIMA_DATOS_PARA_LIMPIAR + 1):
            for j in range(1, 18):  # Columnas A-Q (1-17)
                ws.cell(row=i, column=j, value=None)

    def _calcular_totales(self, ws) -> Tuple[int, int, int, List[int]]:
        """Calcula los totales de la hoja."""
        totales_motivos = [0] * 8
        total_si = 0
        total_no = 0
        num_registros = 0

        for fila in range(
            self.FILA_INICIO_DATOS, self.FILA_MAXIMA_DATOS_PARA_LIMPIAR + 1
        ):
            if ws.cell(row=fila, column=2).value:  # Si hay radicado
                num_registros += 1
                if ws.cell(row=fila, column=7).value == "SI":  # Columna G=7 = SI
                    total_si += 1
                if ws.cell(row=fila, column=8).value == "NO":  # Columna H=8 = NO
                    total_no += 1
                # Contar motivos en columnas I-P (9-16)
                for i in range(8):
                    if ws.cell(row=fila, column=9 + i).value:
                        totales_motivos[i] += 1

        return num_registros, total_si, total_no, totales_motivos

    def _escribir_totales(
        self, ws, total_si: int, total_no: int, totales_motivos: List[int]
    ):
        """Escribe los totales en la hoja."""
        ws.cell(row=self.FILA_TOTALES, column=7, value=total_si)  # Columna G=7 = SI
        ws.cell(
            row=self.FILA_TOTALES, column=17, value=total_no
        )  # Columna Q=17 = NO (total)
        for i, total in enumerate(totales_motivos):
            ws.cell(
                row=self.FILA_TOTALES, column=9 + i, value=total
            )  # Columnas I-P (9-16) = motivos

    def contar_registros(self) -> int:
        """Cuenta el número de registros en el archivo."""
        try:
            wb = load_workbook(self.archivo_path, read_only=True)
            ws = wb.active
            num_registros, _, _, _ = self._calcular_totales(ws)
            return num_registros
        except Exception:
            return 0

    def leer_datos(self) -> List[List]:
        """Lee todos los datos del archivo incluyendo encabezados para búsqueda avanzada."""
        try:
            wb = load_workbook(self.archivo_path, read_only=True)
            ws = wb.active

            if ws is None:
                raise Exception("No se pudo acceder a la hoja de trabajo")

            # Leer todas las filas desde el inicio de datos
            datos = []

            # Agregar fila de encabezados (opcional para compatibilidad)
            encabezados = [
                "#",
                "Radicado",
                "Tipo",
                "Fecha",
                "Hora",
                "Juzgado",
                "SI",
                "NO",
                "Juez",
                "Fiscalía",
                "Usuario",
                "Inpec",
                "Víctima",
                "ICBF",
                "Def.Confianza",
                "Def.Público",
                "Observaciones",
            ]
            datos.append(encabezados)

            # Leer datos reales
            for fila_num in range(self.FILA_INICIO_DATOS, ws.max_row + 1):
                fila = []
                for col in range(1, 18):  # Columnas A-Q
                    valor = ws.cell(row=fila_num, column=col).value
                    fila.append(valor if valor is not None else "")

                # Solo incluir filas con radicado
                if fila[1]:  # Si hay radicado en columna B
                    datos.append(fila)

            return datos

        except Exception as e:
            raise Exception(f"Error al leer datos: {e}")
