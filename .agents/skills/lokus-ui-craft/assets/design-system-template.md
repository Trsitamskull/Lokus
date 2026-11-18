# DESIGN.md — Lokus Judicial Design System

> **Instructions for the coding agent**: Hydrate every bracketed placeholder from the live codebase (`src/` CSS files and existing model components) before editing UI. Keep this file updated whenever tokens, component variants, or elevation rules change.

## 1. Product & Visual Direction

- **Application**: Lokus Judicial (Tauri v2 native desktop app for Colombian judicial hearing management and `.xlsx` automation using the Defensoría del Pueblo template).
- **Visual Personality**: `[e.g., Crisp, high-legibility legal-tech desktop workbench — calm neutral surfaces, restrained institutional accent, high data density without visual clutter]`
- **Theme Support**: `[Light only / Light + Dark via data-theme attribute and CSS Variables]`

---

## 2. Color Tokens (CSS Custom Properties)

Define and use **semantic tokens** only inside `.svelte` components (never raw hex values outside the token definition file):

| Role | CSS Variable | Current Value | Usage |
| :--- | :--- | :--- | :--- |
| App Background | `--bg-app` | `[#...]` | Root desktop canvas |
| Surface Primary | `--bg-surface` | `[#...]` | Main workspace panels, hearing grid |
| Surface Elevated | `--bg-elevated` | `[#...]` | Modals, popovers, active cards |
| Surface Subtle | `--bg-subtle` | `[#...]` | Table headers, hover rows, sidebars |
| Text Primary | `--text-primary` | `[#...]` | Body text, cell data, headings |
| Text Secondary | `--text-secondary` | `[#...]` | Metadata, labels, timestamps |
| Text Muted | `--text-muted` | `[#...]` | Placeholders, disabled text |
| Border Default | `--border-default` | `[#...]` | Hairline panel dividers, inputs |
| Accent Primary | `--accent-primary` | `[#...]` | Primary CTA, active tab, focus ring |
| Status Success | `--status-success` | `[#...]` | Validated `.xlsx` sync, `.lokus` backup OK |
| Status Warning | `--status-warning` | `[#...]` | Heuristic review needed, partial match |
| Status Danger | `--status-danger` | `[#...]` | Validation errors, destructive actions |
| AI OpenAI / Gemini | `--badge-ai-*` | `[#...]` | Dual AI provider indicators |

---

## 3. Typography Scale

- **UI Font Stack**: `--font-sans: [...]`
- **Monospace / Radicados Stack**: `--font-mono: [...]`
- **Numeric Rule**: Apply `font-variant-numeric: tabular-nums` on hearing times, dates, row counters, and Excel cell indices.

| Element | CSS Token / Size | Weight | Line Height | Heading Tag Rule |
| :--- | :--- | :--- | :--- | :--- |
| View Title | `--text-xl` (`[...]`) | `[600]` | `[1.25]` | `<h1>` (one per view) |
| Section Header | `--text-lg` (`[...]`) | `[600]` | `[1.3]` | `<h2>` |
| Panel / Card Title | `--text-md` (`[...]`) | `[600]` | `[1.35]` | `<h3>` |
| Body / Table Cell | `--text-base` (`[...]`) | `[400]` | `[1.45]` | `<p>`, `<td>` |
| Caption / Badge | `--text-sm` (`[...]`) | `[500]` | `[1.3]` | `<span>`, `<small>` |

---

## 4. Spacing, Radii & Elevation Policy

- **Spacing Scale**: `--space-1` (`4px`), `--space-2` (`8px`), `--space-3` (`12px`), `--space-4` (`16px`), `--space-6` (`24px`), `--space-8` (`32px`).
- **Border Radius Scale** (strictly limited to 3 tiers + pill):
  - `--radius-sm`: `[4px]` (badges, table chips, small inputs)
  - `--radius-md`: `[8px]` (buttons, standard inputs, cards)
  - `--radius-lg`: `[12px]` (modals, main panels)
  - `--radius-full`: `9999px` (pills, status dots)
- **Elevation vs. Border Rule (Anti-Slop)**:
  - **Flat / Docked Panels**: Use `border: 1px solid var(--border-default)` and `box-shadow: none`.
  - **Floating Overlays (Modals, Dropdowns, Drag Ghost)**: Use `box-shadow: var(--shadow-lg)` and `border: none` (or `1px solid var(--border-subtle)` only if contrast requires it—document explicitly here).

---

## 5. Component & Control Matrix

### Buttons
- **Heights**: `--btn-height-sm` (`28px`), `--btn-height-md` (`34px` default), `--btn-height-lg` (`40px`).
- **Variants**: `.btn-primary`, `.btn-secondary`, `.btn-ghost`, `.btn-danger`, `.btn-icon`.
- **Icons**: Lucide SVG (`16px` inside `md` buttons, `14px` inside `sm` buttons, `gap: var(--space-2)`).

### Container Queries
- Resizable sidebars, inspector cards, and hearing preview panes must declare `container-type: inline-size` and adapt at consistent thresholds (`@container (max-width: 420px)` / `@container (min-width: 720px)`).

### Pointer Events Drag & Drop
- Dragged item: `[opacity, cursor: grabbing, shadow token]`
- Valid drop target: `[2px dashed var(--accent-primary), subtle --bg-accent-soft]`

---

## 6. Model Components (Imitate These)

> List the cleanest existing `.svelte` components in the repo so agents imitate their exact Svelte 5 Runes + CSS Variables pattern:

1. `[src/lib/components/ExampleComponent.svelte]` — `[Why it is a model: Runes usage, clean tokens, accessible states]`
2. `[src/lib/components/ExampleTable.svelte]` — `[Why it is a model: tabular-nums, container query, keyboard nav]`

---

## 7. Explicit Exceptions

> Document any intentional deviations from the audit rules here so future audits do not revert them.

- `[None yet]`
