# DESIGN.md — Lokus Judicial Design System

> Documento canónico de diseño para **Lokus — Gestor de Audiencias Judiciales** (`v3.2`, Tauri v2 + Svelte 5 + Vite 6 + CSS nativo). Cualquier nuevo token, variante de botón, elevación o componente reutilizable en `src/` debe reflejarse aquí y verificarse con `.agents/skills/lokus-ui-craft/scripts/audit_ui.py`.

## 1. Product & Visual Direction

- **Application**: Lokus Judicial (aplicación de escritorio nativa con Tauri v2 para gestión de audiencias judiciales colombianas y automatización de planillas `.xlsx` de la Defensoría del Pueblo).
- **Visual Personality**: Mesa de trabajo jurídico-técnica sobria y de alta legibilidad. Superficies planas y tranquilas, jerarquía tipográfica clara, acentos institucionales contenidos y alta densidad de datos sin ruido visual ni ornamentación genérica.
- **Theme Support**: Dual completo (**Tema Claro: "Azul Niebla Sereno"** y **Tema Oscuro: "Slate Nocturno Sereno"**) controlado mediante `data-theme="light" | "dark"` en `<html>`, `<meta name="color-scheme">` en `index.html` y variables CSS en `src/app.css` (`Ctrl+T` alterna el tema).

---

## 2. Color Tokens (CSS Custom Properties)

Todos los colores en componentes `.svelte` y reglas de `src/app.css` deben referenciar tokens semánticos (`var(--...)`) definidos en `:root[data-theme="light"]` y `:root[data-theme="dark"]` dentro de `src/app.css`:

| Role | CSS Variable | Light ("Azul Niebla Sereno") | Dark ("Slate Nocturno Sereno") | Usage |
| :--- | :--- | :--- | :--- | :--- |
| Sidebar Background | `--bg-sidebar` | `#e3ecf5` | `#10151e` | Barra lateral, `.active-doc-bar`, cabeceras secundarias |
| App / Workspace | `--bg-workspace` | `#eef4fa` | `#151b26` | Lienzo principal de trabajo y barra superior |
| Surface Elevated | `--bg-elevated` | `#f8fbfe` | `#1c2432` | Tarjetas de formulario, KPI, modales y popovers |
| Input Surface | `--bg-input` | `#fbfdff` | `#131923` | Campos de texto, selects, swatches y botones sutiles |
| Surface Subtle | `--bg-subtle` | `#dce7f3` | `#222c3d` | Cabeceras de tabla, badges neutros, contenedores auxiliares |
| Surface Hover | `--bg-hover` | `#cfe0f0` | `#293549` | Hover de filas, botones sutiles y opciones de menú |
| Hairline Border | `--border-hairline` | `#c9d9ea` | `#273244` | Divisores de paneles, guías de árbol, bordes de tarjetas |
| Strong Border | `--border-strong` | `#87a0bc` | `#3c4d66` | Controles, inputs, botones sutiles, popovers |
| Focus Border | `--border-focus` | `#235a8c` | `#7db3f5` | Anillos `:focus-visible` y bordes enfocados |
| Text Primary | `--text-primary` | `#162336` | `#eef2f7` | Títulos, datos de celdas, texto principal |
| Text Secondary | `--text-secondary` | `#435975` | `#aab7ca` | Etiquetas, metadatos, botones secundarios |
| Text Muted | `--text-muted` | `#526986` | `#7d8da4` | Placeholders, textos de ayuda, conteos |
| Accent Primary | `--accent-primary` | `#235a8c` | `#7db3f5` | CTA principal (`.btn-solid`), pestaña activa, foco |
| Accent Hover | `--accent-primary-hover` | `#1a4770` | `#97c4fa` | Hover sobre botón sólido |
| Accent Foreground | `--accent-primary-fg` | `#f8fbfe` | `#0c1726` | Texto/ícono sobre `--accent-primary` |
| Accent Soft | `--accent-soft` | `#dcebfa` | `rgba(125, 179, 245, 0.14)` | Fila activa en sidebar, selección activa |
| AI Surface & Text | `--ia-bg`, `--ia-border`, `--ia-accent`, `--ia-badge-bg`, `--ia-text` | `#e7f0fb` / `#acc8e8` / `#235a8c` / `#d7e7fa` / `#1b4770` | `#172334` / `#2d4466` / `#73aee8` / `#203149` / `#c2ddfa` | Panel de autocompletado con IA y badge de aprendizaje |
| Warning / Pending IA | `--accent-warm`, `--accent-warm-soft`, `--accent-warm-border` | `#b9380b` / `#fff1e6` / `#e69b63` | `#eab364` / `rgba(234, 179, 100, 0.14)` / `rgba(234, 179, 100, 0.45)` | Alertas, advertencias, IA sin configurar |
| Status Success (`SI`) | `--status-si`, `--status-si-bg`, `--status-si-border` | `#166534` / `#dcfce7` / `#22c55e` | `#5eead4` / `rgba(94, 234, 212, 0.12)` / `rgba(94, 234, 212, 0.45)` | Audiencias realizadas, validaciones exitosas |
| Status Danger (`NO`) | `--status-no`, `--status-no-bg`, `--status-no-border` | `#9f1239` / `#ffe4e6` / `#e11d48` | `#fca5a5` / `rgba(252, 165, 165, 0.13)` / `rgba(252, 165, 165, 0.45)` | Audiencias no realizadas, errores, acciones destructivas |
| Excel Icon | `--excel-icon` | `#15803d` | `#4ade80` | Iconos `FileSpreadsheet` en sidebar, `.active-doc-bar` y exportación |
| Folder Palette (8) | `--folder-azul`, `--folder-verde`, `--folder-ambar`, `--folder-coral`, `--folder-rosa`, `--folder-violeta`, `--folder-turquesa` | `#2563eb` / `#15803d` / `#b45309` / `#c2410c` / `#be185d` / `#6d28d9` / `#0f766e` | `#60a5fa` / `#4ade80` / `#fbbf24` / `#fb923c` / `#f472b6` / `#a78bfa` / `#2dd4bf` | Iconos de carpetas personalizables (`COLORES_CARPETA` en `constants.js`) |
| Modal / Drawer Scrim | `--backdrop-scrim` | `rgba(15, 23, 42, 0.44)` | `rgba(6, 10, 18, 0.62)` | Fondo atenuado detrás de `dialog::backdrop` y `.sidebar-backdrop` |

---

## 3. Typography Scale

- **UI Font Stack**: `-apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", Roboto, sans-serif` (definido en `body` dentro de `src/app.css`).
- **Numeric Rule**: Aplicar `font-variant-numeric: tabular-nums` en fechas, horas, conteos de registros (`0/100`), KPIs, badges y fila 111.

| Element | Size | Weight | Line Height | Heading / Element Rule |
| :--- | :--- | :--- | :--- | :--- |
| Brand / Modal Title | `14.5px` | `600`–`700` | `1.3` | `.brand-id`, `.modal-title` (`<h2>` en `<dialog>`) |
| KPI Value | `clamp(1.35rem, 1.5vw + 0.5rem, 1.85rem)` | `700` | `1.15` | `.stats-kpi-value` con `tabular-nums` |
| Body / Controls | `13.5px` (`12.5px`–`13px` en botones/tablas) | `400`–`600` | `1.45` | `body`, `.input`, `.btn`, `.records-table td` |
| Section Header / Label | `11px`–`12px` uppercase | `700` | `1.3` | `.sidebar-section-header`, `.stats-kpi-label`, `th` |
| Helper / Caption | `11.5px`–`12px` | `400`–`500` | `1.35` | `.shortcut-hint`, `.active-doc-bar` |

---

## 4. Spacing, Radii & Elevation Policy

- **Border Radius Scale** (definida en `:root` de `src/app.css`):
  - `--radius-card`: `12px` (tarjetas principales `.dossier-form-card`, `.stats-kpi-card`, `.stats-panel`, popovers grandes)
  - `--radius-ctrl`: `8px` (botones `.btn`, inputs, `.sidebar-folder-row`, `.sidebar-file-row`, swatches)
  - `--radius-chip`: `6px` (`.mini-icon-btn`, `.folder-context-item`, `.causal-filter-chip`, `.active-doc-defensor-btn`)
  - `--radius-pill`: `999px` (`.tab-count`, `.share-badge`, `.tag-si`, `.tag-no`, `.quota-bar`)
- **Elevation vs. Border Rule**:
  - **Superficies planas / ancladas**: usar `border: 1px solid var(--border-hairline)` y `box-shadow: none` (ej. `.sidebar-file-row`, `.sidebar-folder-row`, `.active-doc-bar`, `.extract-panel`).
  - **Superficies elevadas / flotantes**: usan `--shadow-card` (tarjetas de contenido) o `--shadow-popover` (modales `<dialog>`, popovers de calendario/select, `.folder-context-menu`, `.tree-drag-ghost`, `.toast`).

---

## 5. Component & Control Matrix

### Buttons
- **Primitives**:
  - `.btn` (`padding: 7px 13px`, `font-size: 12.5px`, `font-weight: 600`, `border-radius: var(--radius-ctrl)`, `gap: 7px`).
  - Variantes: `.btn-solid` (primaria), `.btn-subtle` (secundaria con `--border-strong`), `.btn-ai-ready` / `.btn-ia-action` / `.btn-ai-warm`, `.btn-danger`.
  - `.mini-icon-btn`: botón cuadrado `26px × 26px` (`32px × 32px` en `.sidebar-toggle-btn`, `22px × 22px` en `.sidebar-collapse-btn`), `border-radius: var(--radius-chip)`.
  - `.active-doc-defensor-btn`: botón discreto en `.active-doc-bar` para editar/asignar el defensor de la planilla activa en 1 clic.
- **Icons**: Lucide SVG importado desde `@lucide/svelte` (`12px`–`16px`). Prohibido usar emojis Unicode como iconos de interfaz.

### Barra Lateral Plegable y Menú Contextual Unificado
- `.desktop-frame.sidebar-collapsed`: colapsa la columna del sidebar a `0px` con transición suave de `grid-template-columns` (`0.18s`, respetando `prefers-reduced-motion`) y se alterna mediante `.sidebar-toggle-btn` (siempre visible) o `Ctrl+B`.
- Tanto carpetas (`[data-context-folder]`) como archivos `.xlsx` (`[data-context-file]`) usan `.folder-context-menu` (`popover="manual"`) con acciones de clic derecho y bloqueo del menú nativo en `.sidebar-files`.

### Controles de Fecha y Hora (`DatePickerInput.svelte` y `AudienciaForm.svelte`)
- **Calendario (`.calendar-popover`)**: alterna vistas inline entre días (`.calendar-grid`), rejilla `4×3` de meses (`.calendar-months-matrix`) y rejilla `4×3` paginada de años `2018–2050` (`.calendar-years-matrix`) mediante `.calendar-cell-btn`, con navegación por rueda del ratón sobre mes, año y cuadrícula.
- **Hora 12h (`.time-inline-box`)**: captura en formato `12h` (`01..12`) + selector `.time-meridiem-group` (`AM`/`PM`) con navegación por bordes mediante `ArrowLeft`/`ArrowRight` y conversión automática a `24h` (`HH:MM`) al persistir en Excel.
- **Diagnóstico Técnico de Autoaprendizaje (`.ia-diag-details`)**: bloque colapsable `<details>` dentro del modal `config_ia` (`Modals.svelte`) sobre superficie plana (`--bg-input`, `--border-hairline`, `box-shadow: none`), métricas numéricas con `tabular-nums`, tabla de fiabilidad por campo, alertas de drift Page-Hinkley y ciclo de vida de reglas sin sobrecargar la vista diaria.

### Container Queries
- Contenedores declarados en `src/app.css`:
  - `.desktop-frame`: `container-type: inline-size; container-name: app;` (colapsa el sidebar a cajón deslizable debajo de `@container app (width < 960px)`).
  - `.workspace-pane`: `container-type: inline-size; container-name: workspace;` (adapta formulario, tabla y estadísticas al ancho útil del panel).

### Pointer Events Drag & Drop
- Implementado en `src/lib/components/Sidebar.svelte` con `onpointerdown`, `window` `onpointermove` / `onpointerup` / `onpointercancel`, `document.elementFromPoint`, atributos `[data-drop-folder]` y umbral de `6px`:
  - Elemento arrastrado: `.is-dragging-source` (`opacity: 0.45`, `body.is-dragging-tree` con `cursor: grabbing`) + fantasma `.tree-drag-ghost`.
  - Destino válido interior: `.is-drop-target` (`outline: 1.5px dashed var(--accent-primary)` sobre fondo suave `--accent-primary` al 16%).
  - Reordenamiento de carpetas: indicadores superior/inferior `.is-drop-before::before` y `.is-drop-after::after` (`2px` `--accent-primary`).

---

## 6. Model Components (Imitate These)

1. `src/lib/components/RegistrosTable.svelte` — Usa Svelte 5 Runes (`$props`, `$derived.by`), clases semánticas en `src/app.css`, cero colores hardcodeados, `tabular-nums` en columnas numéricas/fechas y transformación responsiva por `@container`.
2. `src/lib/components/CustomSelect.svelte` — Selector accesible con teclado (`ArrowUp/Down`, `Enter`, `Escape`), Lucide icons y variables de tema compartidas.
3. `src/lib/components/Sidebar.svelte` — Patrón de referencia para árboles jerárquicos, `untrack` dentro de `$effect`, iconos `@lucide/svelte` y Drag & Drop con Pointer Events sin dependencias externas.

---

## 7. Explicit Exceptions

1. **Borde + Sombra en Superficies Oscuras**: En tema oscuro (`#10151e` / `#151b26` / `#1c2432`), el contraste entre paneles apilados requiere un borde sutil (`1px solid var(--border-hairline)` o `var(--border-strong)`) combinado con `--shadow-card` / `--shadow-popover` en `.dossier-form-card`, `.stats-kpi-card`, `.stats-panel`, `.calendar-popover`, `.custom-select-popover`, `.folder-context-menu`, `dialog:modal`, `.tree-drag-ghost` y `.toast` para mantener separación nítida de capas sin depender solo de sombras oscuras.
2. **Swatches del Selector de Carpetas**: `.swatch-dot` usa `forced-color-adjust: none` estrictamente en el punto de color (nunca en el texto ni el botón completo) para preservar el color identificador en modo de alto contraste, acompañado siempre de etiqueta de texto y `<Check>` como indicadores no cromáticos.
3. **Clases semánticas `*-grid`**: Nombres como `.motivos-grid`, `.calendar-grid`, `.stats-kpi-grid`, `.stats-split-grid`, `.folder-color-grid` y `.shortcut-grid` son clases CSS Grid propias definidas en `src/app.css` (no utilidades Tailwind).
