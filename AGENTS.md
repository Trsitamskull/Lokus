# AGENTS.md — Manual Operativo, Estándares de Ingeniería y Reglas de Desarrollo Agéntico (Lokus)

Este documento constituye la memoria persistente, la especificación operativa y el contrato de comportamiento para cualquier agente de programación con inteligencia artificial que trabaje sobre el repositorio **Lokus (Gestor de Audiencias Judiciales)**. Todas las instrucciones definidas aquí tienen prioridad sobre decisiones circunstanciales tomadas en el historial de una conversación.

> **Principio rector:** El agente posee autonomía para leer archivos, editar múltiples ficheros, ejecutar comandos, correr pruebas e iterar sobre tareas hasta completarlas, pero el desarrollador humano sigue siendo el eje de control indispensable desde la ideación hasta el despliegue. El objetivo no es únicamente que la aplicación "parezca funcionar", sino producir código limpio, bien documentado, seguro, fácil de mantener y fácil de extender.

---

## 1. Estándar Multiplataforma y Jerarquía de Configuración

1. **Archivo canónico compartido:** Este `AGENTS.md` es la fuente única de verdad del repositorio.
2. **Orden de precedencia:** Si existen reglas adicionales en subdirectorios o habilidades en `.agents/skills/`, aplica la regla de proximidad: el archivo más específico al módulo editado prevalece.
3. **Mantenimiento continuo:** Cualquier cambio en la arquitectura, dependencias fijadas, esquema de la plantilla Excel o proveedores de IA debe reflejarse inmediatamente aquí.

---

## 2. Ficha del Proyecto y Especificaciones Técnicas (Lokus)

### 2.1 Identidad, pila tecnológica y comandos operativos

| Categoría | Especificación del proyecto |
| :--- | :--- |
| **Nombre de la aplicación** | **Lokus — Gestor de Audiencias Judiciales** (`v4.3` en rama `rust-tauri` / `v2.0` en rama `legacy-python`) |
| **Pila tecnológica activa (`rust-tauri`)** | **Backend:** Rust 2021 + Tauri v2 (`umya-spreadsheet`, `reqwest` con `rustls-tls`, `tokio`, `serde`, `chrono`, `regex`, `base64`, `flate2`)<br>**Frontend:** Svelte 5 (Runes: `$state`, `$derived`) + Vite 6 + CSS Variables nativas (puerto dev `5173`) |
| **Pila tecnológica legado (`legacy-python`)** | Python 3 + `flet==1.1.2` (`flet-desktop==1.1.2`, `flet-cli==1.1.2`) + `openpyxl==3.1.2` + `openai==1.98.0` |
| **Proveedores de IA soportados** | **OpenAI** (`gpt-5-mini`, `gpt-4o-mini`, `gpt-4o`) y **Google Gemini** (`gemini-2.5-flash`, `gemini-2.0-flash`) para autocompletado de formulario a partir de texto libre |
| **Comando de instalación** | `npm install` (frontend) y resolución automática de crates con `cargo` en `src-tauri/` |
| **Comando de inicio / desarrollo** | `npm run tauri dev` (en rama `rust-tauri`) / `./.lokusvenv/bin/python3 main.py` (en rama `legacy-python`) |
| **Comando de pruebas** | `cd src-tauri && cargo test` (pruebas unitarias y de caracterización en Rust) |
| **Gestión de configuración y secretos** | `config/settings.json` (rama Rust) y `config/config.py` (rama Python). **Ambos ignorados en `.gitignore`; prohibido subir claves a Git** |

### 2.2 Estructura de directorios y responsabilidades

1. **Frontend y vistas (`src/` e `index.html`):**
   - `src/App.svelte`: Orquestador principal de estado, barra superior, comandos IPC y atajos globales.
   - `src/lib/constants.js`: Catálogos oficiales del frontend (`TIPOS_AUDIENCIA`, `MOTIVOS_LABELS`, `HORAS_RAPIDAS`, `JUZGADOS_BASE`, `normalizarClaveCatalogo`).
   - `src/lib/components/`: Componentes modulares Svelte 5 (`Sidebar.svelte`, `AudienciaForm.svelte` incluyendo `.ia-clarify-panel` adaptativo con pills de cambio y chips rápidos, `RegistrosTable.svelte`, `EstadisticasView.svelte`, `Modals.svelte` con controles de consolidación y perfiles sintéticos en `config_ia`, `CustomSelect.svelte`, `AutocompleteInput.svelte` con edición/borrado en línea de defensores guardados, `DatePickerInput.svelte`).
   - `src/app.css`: Sistema de temas (`data-theme="dark"` Warm Obsidian y `data-theme="light"` Azul Niebla Sereno), barras de scroll integradas y CSS `@container` responsivo.
2. **Backend y lógica de dominio en Rust (`src-tauri/src/`):**
   - `models.rs`: Estructuras `AudienciaInput`, `RegistroAudiencia`, `TotalesExcel`, `IndicePriorsExcel` (incluyendo `GrafoAsociaciones`), catálogos oficiales (`TIPOS_AUDIENCIA`, `MOTIVOS_LABELS`, `HORAS_RAPIDAS`), `normalizar_clave_catalogo`, `normalizar_clave_defensor` (identidad unificada sin honoríficos `Dr./Dra./Defensor Público`), `es_alcance_general`, `coincide_alcance_defensor`, `extraer_prefijo_dane_radicado`, `extraer_siglas_cortas_juzgado`, `texto_indica_instalacion_efectiva`, `texto_indica_no_instalacion`, mapeo de columnas `mapear_motivo_a_indice` y validadores.
   - `excel.rs`: Motor de lectura/escritura sobre `umya-spreadsheet`, plantilla embebida con `include_bytes!`, ordenamiento cronológico descendente, carpetas/subcarpetas, caché en memoria, índice de identidad estable (`.lokus_estadisticas.json`) e índice incremental de priors **`v3`** (`.lokus_priors.json`, reconstrucción automática desde `v1`/`v2` con claves de defensor canonizadas sin honoríficos) con Mapa de Conexiones (`GrafoAsociaciones`) por defensor, siglas `J#`, prefijo DANE y tipo, `actualizar_priors_incremental_con_defensor_en` e invalidación automática al cambiar el defensor de una planilla.
   - `paquete.rs`: Exportación e importación portable de copias completas comprimidas (`.lokus`, sin API keys, incluyendo uso histórico de catálogos) y paquetes de solo estadísticas (`.lokus.json`), con inspección previa, deduplicación por ID e invalidación de priors.
   - `ai.rs`: Preprocesador de jerga/abreviaciones judiciales colombianas (incluyendo typos reales y puntuación irregular de horas), cliente HTTP asíncrono dual (OpenAI + Gemini con esquema JSON estricto y respaldo automático), inyección dual de resúmenes consolidados (`general <= 320` chars + `defensor <= 380` chars cuando `usar_resumen_consolidado` está activo), `consolidar_memoria_ia`, `prediccion_local` en `AiAudienciaOutput`, arbitraje híbrido por defensor (invariantes Defensoría + estadística local + protección ante negaciones de instalación + preferencias adaptables como `instalada_reprogramada_si`), inducción agrupada de clústeres y postprocesador de motivos, tipos, juzgados y observaciones.
   - `aprendizaje.rs`: **Memoria `v4`** de autoaprendizaje (`ia_aprendizaje.json`, migración automática `v1 -> v2 -> v3 -> v4` con fusión de colisiones de claves por defensor) en Observación Dual (Cuaderno del Defensor + Cuaderno General transversal `"*"` con promoción automática desde `>= 2` defensores), identidad unificada sin honoríficos con migración al renombrar (`migrar_defensor_en_memoria`), cierre completo de `aprendizaje_base.json` (`base + local` con deduplicación, herencia de preferencias y resúmenes con precedencia local), **Consolidador en Reposo estadístico** (presión por sorpresa de Shannon + disparo probabilístico con histéresis, concurrencia RAII `ConsolidacionGuard`, `watermark_ids` y operaciones delta `fusionar`/`conflicto`/`sintetizar` respetando `base_invariante`), **aclaración adaptativa** (`decidir_aclaracion_local` `<1 ms` / `0` tokens con `VoI` modulado por `EWMA`, auto-descarte y descarte `"Dato externo"` a `0` tokens), pista suave desde la 1.ª corrección pendiente en texto casi idéntico, activación en 2 repeticiones, gobernanza bayesiana (`cuantil_beta`, Wilson, decaimiento híbrido por evento), evaluación en sombra aislada por defensor, tokenizador judicial + BM25 + MMR, atribución causal de crédito sobre búfer balanceado, clústeres diferidos, detección de deriva Page-Hinkley y exportación unificada (`ia + uso_catalogos`) a Descargas **conservando nombres reales y claves unificadas de defensores** (`defensores_directorio`, métricas, perfiles sintéticos y consolidaciones) con **anonimización estricta en pasada lineal** (`Anonimizador` con `IndiceRadicados`, `TrieFrases`, `EscudosDefensores` y caché temporal `CacheCatalogoAnon`) de procesados (`[PROCESADO]`), radicados (`[RADICADO]`), teléfonos (`[TELEFONO]`) y cédulas/números (`[NUMERO]`).
   - `uso_catalogos.rs`: Conteo histórico persistente (`config/uso_catalogos.json`) de uso en `Tipo de audiencia`, `Hora` y `Juzgado` (incluyendo opciones predeterminadas y de usuario con `0` usos, entradas libres `"Otra"` y franjas horarias), siembra única inicial desde Excel, ajuste por edición/borrado de filas e integración en paquetes `.lokus` y exportación JSON unificada.
   - `estadistica_ia.rs`: Motor estadístico local en Rust puro (Regresión Logística Online L2 + AdaGrad, comparativa CNB, BM25 k-NN, resolutor de entidades/ordinales con tabla de alias Beta `Beta(1.0, 1.2)` activable en 2 correcciones con `q10 >= 0.30` y siglas cortas `J#` por defensor, cruce con `GrafoAsociaciones` y `resources/juzgados_base.json`, y desambiguación AM/PM por jornada laboral).
   - `config.rs`: Lectura/escritura de `config/settings.json` (incluyendo `consolidacion_ia_activa`, `usar_resumen_consolidado` y deduplicación de defensores sin honoríficos) y enmascaramiento de API Keys para la interfaz.
   - `lib.rs` y `main.rs`: Registro de comandos IPC (`#[tauri::command]`), exigencia de Defensor Público obligatorio al crear nuevas planillas (`cmd_crear_archivo`), paso de `defensor` y `prediccion_local` a comandos IA, `cmd_responder_aclaracion_ia` (`es_dato_externo`), `cmd_consolidar_memoria_ia`, migración de memoria al renombrar defensores (`cmd_renombrar_defensor`) y arranque de Tauri v2.
3. **Plantilla oficial y datos de usuario:**
   - `templates/plantilla_audiencias.xlsx`: Plantilla oficial de la Defensoría del Pueblo (**INMUTABLE**).
   - `archivos_creados/`: Carpeta local de trabajo en desarrollo (en instaladores de producción se ubica en `Documentos/Lokus/archivos_creados/`). Ignorada en Git; incluye `.respaldos/` con hasta 3 versiones previas por planilla.
4. **Habilidades del proyecto (`.agents/skills/`) y Automatización de Releases:**
   - `.agents/skills/lokus-judicial-domain/SKILL.md`: Especificación detallada del formato Excel de la Defensoría del Pueblo y reglas jurídicas de extracción con IA.
   - `CHANGELOG.md` y `scripts/extract-release-notes.mjs`: Fuente de notas por versión usadas automáticamente por `.github/workflows/release-windows-linux.yml` para publicar las novedades reales en cada GitHub Release.

### 2.3 Convenciones de estilo, diseño y zonas protegidas

1. **Fuente única de diseño (`DESIGN.md`):** Leer y mantener actualizado `DESIGN.md` en la raíz del repositorio para todos los colores, tipografía, espaciados, radios, elevaciones y componentes reutilizables en Svelte 5. Nunca dejar decisiones visuales o de arquitectura únicamente en el historial del chat.
2. **Convenciones de UI (Svelte 5 + CSS + Stack fijo):**
   - Usar exclusivamente **Svelte 5 Runes** (`$state`, `$derived`, `$effect`, `$props`, `$bindable`), **CSS Variables nativas** (`var(--...)` en `src/app.css`), **CSS Container Queries** (`@container`), iconos vectoriales **Lucide** (`@lucide/svelte`) y **Pointer Events** para Drag & Drop. Prohibido introducir Tailwind CSS, React, frameworks CSS externos o sintaxis heredada de Svelte 3/4 (`export let`, `$:`).
   - Todos los colores deben referenciar tokens semánticos de `src/app.css` (`var(--accent-primary)`, `var(--bg-elevated)`, `var(--text-primary)`, etc.) para garantizar paridad total entre Tema Oscuro y Tema Claro (`Ctrl+T`).
   - **Higiene anti-slop y seguridad en UI:** Prohibido usar emojis Unicode como iconos de interfaz, prohibido mezclar bordes y sombras arbitrarios fuera de las excepciones documentadas en `DESIGN.md`, mantener commits pequeños y acotados por vista, y nunca registrar ni incrustar API Keys de OpenAI/Gemini o secretos `.env` en código frontend.
3. **Estándar de código en Rust (Funciones Modelo):**
   - Todo comando Tauri en `src-tauri/src/lib.rs` debe devolver `Result<T, String>` con mensajes de error claros en español listos para mostrarse al usuario.
   - Prohibido usar `.unwrap()` en rutas de entrada de usuario o lectura de archivos Excel/red; propagar errores con `?` y `.map_err(...)`.
4. **Zonas restringidas e inmutables:**
   - Prohibido modificar `templates/plantilla_audiencias.xlsx`.
   - En el motor de Excel (`src-tauri/src/excel.rs`), la celda `B8` (`B8:Q8`) almacena `"DEFENSOR PÚBLICO: <NOMBRE>"` sin alterar su estilo (obligatorio al crear nuevas planillas; las planillas antiguas sin defensor siguen abriéndose en modo compatible usando el Cuaderno General de IA); las filas `11..=110` son exclusivamente para registros, la fila `111` para totales (`G111` = Total SI, `I111..P111` = Totales por motivo, `Q111` = Total NO) y las columnas `I..P` (`9..=16`) corresponden estrictamente en orden a: `[Juez, Fiscalía, Usuario, INPEC, Víctima, ICBF, Defensor Confianza, Defensor Público]`.
5. **Archivos protegidos por pruebas de caracterización:**
   - Ejecutar siempre `cd src-tauri && cargo test` después de cualquier cambio en `src-tauri/src/models.rs`, `src-tauri/src/excel.rs` o `src-tauri/src/ai.rs`.

---

## 3. Modelo de Colaboración Humano-Agente y Control mediante Diffs

1. **Transparencia total en modificaciones:** Todo cambio aplicado en el repositorio debe mantenerse limpio y auditable mediante *diffs* claros, evitando alteraciones masivas no solicitadas.
2. **Trabajo iterativo:** Las mejoras de interfaz o reglas de extracción judicial se refinan progresivamente con el feedback del usuario.
3. **Detención ante incertidumbre de negocio:** Cuando falten definiciones sobre reglas jurídicas o estructura de columnas del reporte de la Defensoría, el agente tiene prohibido inventar políticas en silencio; debe preguntar antes de consolidar esa lógica en código o en pruebas.

---

## 4. Calidad de Código, Prompting Estructurado y Contexto Local

### 4.1 Regla anti-duplicación y código DRY (*Do Not Repeat Yourself*)

1. **Inspección previa obligatoria:** Leer los módulos existentes antes de escribir código nuevo para reutilizar funciones de validación, formateo y acceso a celdas.
2. **Reutilización frente a copia y pega:** Unificar modales similares (como Seleccionar, Eliminar y Descargar archivo) y centralizar la escritura/limpieza de filas de Excel en una única función (`escribir_registro_en_fila`).
3. **Calidad interna por encima del resultado superficial:** Que la interfaz visual funcione no valida una implementación si introduce deuda técnica o rompe el tipado en Rust.

### 4.2 Marco Tarea – Contexto – Salida

| Dimensión | Qué debe verificar el agente | Objetivo operativo |
| :--- | :--- | :--- |
| **Tarea** | Qué acción concreta se solicita construir, corregir o refactorizar | Acotar el alcance exacto del cambio sin añadir funcionalidades no pedidas |
| **Contexto** | Qué archivos de `src-tauri/src/` y `src/` deben inspeccionarse y reutilizarse | Evitar soluciones aisladas que ignoren la arquitectura del proyecto |
| **Salida** | Código DRY, manejo de errores seguro en Rust y preservación de temas Claro/Oscuro | Garantizar calidad de ingeniería y no solo apariencia funcional |

### 4.3 Convención `TODO (AI)` y Funciones Modelo

Cuando el desarrollador deje comentarios `// TODO (AI): ...` o marque una función como `// Model function`, el agente debe replicar la estructura de manejo de errores, tipos y estilo del módulo correspondiente.

---

## 5. Protocolo de Planificación Previa (*Plan Mode*)

1. **Omitir *Plan Mode*** en cambios puntuales de un solo archivo o ajustes visuales directos.
2. **Activar *Plan Mode*** ante cambios arquitectónicos, nuevos módulos o flujos con múltiples piezas móviles.
3. **Auditoría del plan antes de implementar:**
   - Exponer decisiones técnicas asumidas.
   - Listar archivos a crear/modificar evitando sobreingeniería.
   - Cubrir casos límite explícitamente (ej.: archivo Excel abierto en otro programa, hora vacía confirmada por el usuario, límite de 100 registros en filas `11..110`, clave API inválida o sin conexión).

---

## 6. Uso de Habilidades del Proyecto (*Skills*)

- **Dominio judicial y Excel / IA:** Antes de modificar la lógica de Excel (`excel.rs`), las validaciones (`models.rs`) o el extractor de IA (`ai.rs`), consultar:
  - `.agents/skills/lokus-judicial-domain/SKILL.md`
- **Interfaz y Sistema de Diseño (`lokus-ui-craft`):** Antes de auditar, reestilizar o construir cualquier vista, modal o componente en `src/`, activar y seguir:
  - `.agents/skills/lokus-ui-craft/SKILL.md` (incluyendo `scripts/audit_ui.py` y `references/audit-checklist.md`) junto con `DESIGN.md`.

---

## 7. Estrategia Integral de Pruebas (*Testing*): Caracterización y TDD

1. **Pruebas de caracterización obligatorias:** Las reglas de mapeo de motivos (`I..P`), validación de fechas (`2018..=2050`) y comportamiento con hora vacía están protegidas en `src-tauri/src/models.rs`. Todo cambio en `src-tauri/src/` exige ejecutar `cargo test`.
2. **Prohibición de "corregir su propia tarea" (*Marking Own Homework*):** No inventar expectativas de negocio en los tests para hacer pasar código dudoso.
3. **Flujo TDD para nuevas reglas de negocio:** Definir primero el caso de prueba con el humano (fase roja), auditar los casos límite y luego implementar la lógica en Rust hasta pasar en verde.

---

## 8. Higiene en Control de Versiones (Git)

1. **Ramas del repositorio:**
   - `rust-tauri`: Desarrollo activo de Lokus v3.0 (Rust + Tauri v2 + Svelte 5).
   - `legacy-python` (y tag `v2.0-python-flet`): Respaldo congelado de la versión en Python + Flet 1.1.2.
2. **Un commit por unidad coherente:** Mensajes descriptivos en español (`feat:`, `fix:`, `refactor:`, `docs:`) sin mezclar cambios inconexos.

---

## 9. Seguridad Absoluta y Gestión de Secretos

1. **Nunca escribir ni incrustar API Keys reales en el código fuente ni en comentarios** (`sk-...` de OpenAI o `AIza...` de Gemini).
2. **Nunca imprimir claves completas en logs o consola:** Usar siempre las funciones de enmascaramiento (`mask_key` en `src-tauri/src/config.rs`).
3. **Verificar `.gitignore` antes de cada commit:** Confirmar que `config/settings.json`, `config/config.py` y cualquier `.env*` nunca entren al área de *staging* (`git status` / `git diff`).

---

## 10. Lista de Verificación Rápida Antes de Entregar Cualquier Tarea

1. ¿Se respetaron las coordenadas exactas de `plantilla_audiencias.xlsx` (filas `11..110`, totales en `111`, motivos en `I..P`)?
2. ¿Se mantuvo el código DRY tanto en Rust (`src-tauri/src/`) como en Svelte 5 (`src/`)?
3. ¿Pasan todas las pruebas de Rust (`cd src-tauri && cargo test`) sin advertencias críticas?
4. ¿Están libres de API Keys todos los archivos rastreados por Git?
