---
name: lokus-judicial-domain
description: >-
  Especificación propietaria del dominio judicial colombiano y estructura exacta de la plantilla
  Excel de la Defensoría del Pueblo para Lokus (Gestor de Audiencias). Activar siempre que se
  modifique la lectura/escritura de Excel (excel.rs), validaciones (models.rs) o extracción por
  IA (ai.rs).
---

# Dominio Judicial y Especificación de Plantilla Excel — Lokus

Esta habilidad codifica las reglas propietarias e inmutables del **Gestor de Audiencias Judiciales (Lokus)** que ningún agente debe adivinar ni alterar sin autorización explícita.

## 1. Coordenadas Exactas de `templates/plantilla_audiencias.xlsx`

La plantilla oficial de la Defensoría del Pueblo (`Hoja1`) tiene formato, bordes y encabezados predefinidos:

- **Encabezado editable `B8` (`B8:Q8`):** Contiene `"DEFENSOR PÚBLICO: <NOMBRE>"` (texto alineado a la izquierda, Calibri 11). Lokus puede leer y actualizar `B8` como texto al crear o editar la asignación de la planilla sin alterar `B5`, `B7`, las filas `9..10` ni el logo embebido.
- **Filas de datos de audiencias:** Filas `11` a `110` inclusive (máximo **100 registros** por archivo).
- **Fila de totales:** Fila `111`.
- **Fila de firma:** Fila `112` (**prohibido sobrescribir**).
- **Respaldos automáticos:** Antes de modificar un `.xlsx`, Lokus conserva hasta **3 copias históricas** en `archivos_creados/.respaldos/<archivo>/`.

### Mapa de Columnas (1-indexed en `umya-spreadsheet`)

| Columna Excel | Índice numérico | Campo | Regla de escritura |
| :--- | :--- | :--- | :--- |
| **A** | `1` | `N°` | Consecutivo numérico (`1..=100`) recalculado tras ordenar |
| **B** | `2` | `RADICADO DEL PROCESO` | Texto obligatorio (determina si la fila está ocupada) |
| **C** | `3` | `TIPO DE AUDIENCIA` | Uno de los 17 tipos oficiales o el texto libre si se eligió `"Otra"` |
| **D** | `4` | `FECHA DE LA AUDIENCIA` | Formato estricto `DD/MM/YYYY` (rango válido: `2018..=2050`) |
| **E** | `5` | `HORA` | Formato 24h `HH:MM` (o vacío `""` si el usuario confirmó guardar sin hora) |
| **F** | `6` | `JUZGADO` | Texto obligatorio |
| **G** | `7` | `¿Se Realizó? -> SI` | `"SI"` cuando se realizó; vacío `""` cuando no se realizó. En fila `111`: suma de `"SI"` |
| **H** | `8` | `¿Se Realizó? -> NO` | `"NO"` cuando no se realizó; vacío `""` cuando sí se realizó. En fila `111`: etiqueta `"TOTAL:"` |
| **I** | `9` | Motivo 0: `Juez` | `"Juez"` si aplica. En fila `111`: total atribuible a Juez |
| **J** | `10` | Motivo 1: `Fiscalía` | `"Fiscalía"` si aplica. En fila `111`: total atribuible a Fiscalía |
| **K** | `11` | Motivo 2: `Usuario` | `"Usuario"` si aplica. En fila `111`: total atribuible a Usuario |
| **L** | `12` | Motivo 3: `INPEC` | `"INPEC"` si aplica. En fila `111`: total atribuible a INPEC |
| **M** | `13` | Motivo 4: `Víctima` | `"Víctima"` si aplica. En fila `111`: total atribuible a Víctima |
| **N** | `14` | Motivo 5: `ICBF` | `"ICBF"` si aplica. En fila `111`: total atribuible a ICBF |
| **O** | `15` | Motivo 6: `Defensor Confianza` | `"Defensor Confianza"` si aplica. En fila `111`: total Defensor Confianza |
| **P** | `16` | Motivo 7: `Defensor Público` | `"Defensor Público"` si aplica. En fila `111`: total Defensor Público |
| **Q** | `17` | `OBSERVACIONES` | Texto de observaciones. **En fila `111` (`Q111`): número total de audiencias `NO` realizadas** |

## 2. Regla de Ordenamiento

Cada vez que se inserta, actualiza o elimina un registro:
1. Se leen todos los registros activos entre las filas `11` y `110`.
2. Se ordenan por **Fecha descendente** y luego **Hora descendente** (las audiencias más recientes primero; si la hora está vacía se trata como `00:00` únicamente para la clave de ordenamiento, conservando vacía su celda).
3. Se limpia el rango `A11:Q110` y se reescriben los registros renumerando `A` de `1` a `N`.
4. Se recalculan y escriben los totales en la fila `111`.

## 3. Reglas Jurídicas para el Extractor de IA (`src-tauri/src/ai.rs`)

1. **Diferenciación Defensor Público vs. Defensor de Confianza:**
   - Expresiones en **primera persona** (*"solicité reprogramar"*, *"pedí aplazamiento"*, *"no pude contactar al procesado"*, *"me fue imposible asistir"*) corresponden a **Defensor Público**.
   - Términos como *"abogado contractual"*, *"abogado privado"*, *"def. confianza"* o *"defensor de confianza"* corresponden siempre a **Defensor Confianza**.
2. **Causas Principales vs. Reacciones:**
   - Si en un mismo texto concurre una causa principal (`Juez`, `Fiscalía`, `Usuario`, `INPEC`, `Víctima`, `ICBF`) junto con una reacción del defensor (*"la fiscal no se conectó y por eso solicité reprogramar"*), el motivo principal debe ser la **causa principal** (`Fiscalía`), y la reacción se registra en `observaciones` como causa secundaria.
3. **Verificación:**
   - Ejecutar `cd src-tauri && cargo test` tras cualquier cambio en `models.rs`, `excel.rs` o `ai.rs`.
