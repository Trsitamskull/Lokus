---
name: lokus-ui-craft
description: >
  Audit, refactor, and design UI views and components for Lokus Judicial (Tauri v2,
  Svelte 5 Runes, Vite 6, native CSS Variables, Container Queries, Lucide icons).
  Use this skill whenever the user asks to improve, polish, clean up, audit, restyle,
  or fix visual inconsistencies in the interface, unify buttons, spacing, borders, or
  typography, eliminate generic AI-looking UI ("AI slop"), create or update DESIGN.md,
  or build a new screen, panel, modal, or hearing-table workflow—even if they don't
  explicitly mention "design system" or "UI audit." Do not use for pure Rust backend,
  Excel parsing (umya-spreadsheet), or AI prompt logic changes that do not touch UI.
compatibility: Requires Python 3.8+ for scripts/audit_ui.py. Built for Tauri v2 + Svelte 5 + Vite 6 + native CSS Variables.
---

# Lokus UI Craft

Eliminate generic AI aesthetics ("AI slop") and enforce visual, structural, and code consistency across **Lokus Judicial**—a native desktop application built with **Tauri v2 (Rust + Tokio)** and **Svelte 5 (Runes) + Vite 6 + native CSS Variables + Container Queries + Lucide icons + Pointer Events Drag & Drop**.

## Non-Negotiable Stack Defaults

Follow these defaults on every UI task unless the user explicitly overrides them:

- **Styling**: Native CSS with CSS Custom Properties (`var(--...)`) and CSS Container Queries (`@container`). Do **not** introduce Tailwind CSS, UnoCSS, Bootstrap, React, or ShadCN.
- **Reactivity**: Svelte 5 Runes (`$state`, `$derived`, `$effect`, `$props`). Do **not** rewrite Runes back to legacy Svelte 4 `$:` reactive statements or `export let`.
- **Icons**: Vector SVG icons via the project's **Lucide** setup (`lucide-svelte`). Never use Unicode emojis as UI icons, status badges, or button glyphs.
- **Drag & Drop**: Preserve native **Pointer Events** (`pointerdown`, `pointermove`, `pointerup`, `pointercancel`, `setPointerCapture`) and visual drop-zone feedback. Never replace them with HTML5 `draggable` or external DnD libraries unless requested.
- **Domain Ergonomics**: Design for dense Colombian judicial hearing workflows (`Defensoría del Pueblo` `.xlsx` templates, schedule tables, dual OpenAI/Gemini AI status feedback, and `.lokus` backup indicators)—prioritize scannability, keyboard navigation, and `tabular-nums` alignment over decorative SaaS marketing layouts.

## Available Bundled Resources

- **`scripts/audit_ui.py`** — Deterministic static analyzer for `.svelte`, `.css`, and `.html` files. Outputs structured JSON to `stdout` and diagnostics to `stderr`. Run `python3 scripts/audit_ui.py --help` for flags.
- **`references/audit-checklist.md`** — Load this file when performing **Flow A (Audit & Refactor)** or before final sign-off on any UI change.
- **`assets/design-system-template.md`** — Load this file when `DESIGN.md` does not exist yet or needs restructuring.
- **`assets/agents-md-snippet.md`** — Short rules block to merge into the repository's root `AGENTS.md`.

## First-Time Repo Hydration (Run Once After Installing This Skill)

If `DESIGN.md` does not exist in the project root or the `Project-Specific Context` section below still contains placeholders:

1. Inspect `package.json`, the frontend source tree (`src/`), existing global CSS/token stylesheets, and 2–3 well-written reference `.svelte` components.
2. Check `AGENTS.md` (and any `CLAUDE.md` or IDE rule files) for existing style conventions and merge conflicts with the user if needed.
3. Read `assets/design-system-template.md` and create `DESIGN.md` at the repository root populated with the **actual** CSS variable names, palette values, fonts, radii, elevations, and model components found in the repo.
4. Update the `Project-Specific Context` section at the bottom of this `SKILL.md` with the real file paths, token prefixes, and model components so future sessions skip discovery.

## Gotchas (Mistakes to Actively Avoid)

1. **Preserving Unrequested Colors**: If the user asks to unify layout, spacing, or components without mentioning colors, preserve the existing color tokens (`var(--...)`) rather than inventing a new palette.
2. **Border + Shadow Clash**: Never put a visible `border` and a `box-shadow` on the same card/panel surface unless `DESIGN.md` explicitly defines an exception. Pick one elevation strategy per surface tier.
3. **Hardcoded Tokens**: Never write raw hex/rgb colors, arbitrary `border-radius` values (e.g., `7px` in one card, `12px` in another), or magic padding numbers outside CSS variable definitions.
4. **Skipped Heading Levels**: Never jump heading levels (e.g., `<h1>` directly to `<h3>`). Maintain strict `h1 -> h2 -> h3` hierarchy per view.
5. **Button & Control Drift**: Never create one-off button sizes, heights, or hover treatments per component. Reuse shared button primitives/classes defined in `DESIGN.md`.
6. **Breaking Runes or Pointer Events During Restyling**: When refactoring markup or wrapping elements for layout/CSS, verify `$props()`, `$bindable()`, `bind:this`, and `onpointer*` listeners stay attached to the intended DOM nodes.
7. **Chat History Amnesia**: Never keep design decisions only in conversation—persist every token, rule, or new component variant in `DESIGN.md`.
8. **Secret & PII Leakage**: Never log, display, or comment API keys (OpenAI/Gemini), `.env` values, or sensitive judicial case data inside frontend components or `console.log` statements.
9. **Over-Running Broad Tests**: During iterative UI styling, run scoped UI audits (`scripts/audit_ui.py`) and targeted checks; run full build/test suites (`npm run check` / `cargo test` / Playwright if configured) before finalizing.

---

## Workflow Selection

- **Use Flow A (Audit & Refactor)** by default when improving, polishing, unifying, or fixing existing screens/components.
- **Use Flow B (New View / Feature)** when building a new screen, modal, drawer, or major workflow from scratch.

---

## Flow A: Audit & Refactor Existing UI (Default)

Progress:
- [ ] Step 1: Establish Design Truth (`DESIGN.md`)
- [ ] Step 2: Run Static UI Audit (`scripts/audit_ui.py`)
- [ ] Step 3: Load Checklist & Inspect Target Views
- [ ] Step 4: Refactor Incrementally (Small, Coherent Edits)
- [ ] Step 5: Validation Loop (Static + Visual + Type Check)

### Step 1: Establish Design Truth
Check if `DESIGN.md` exists at the repository root.
- If it exists, read it first and treat its tokens and component specs as authoritative.
- If it is missing, inspect existing stylesheets and components, then create `DESIGN.md` using `assets/design-system-template.md` before changing any UI code. Ask the user brief clarifying questions **only** if conflicting visual styles exist in the repo and it is unclear which view is the canonical style.

### Step 2: Run Static UI Audit
Execute the bundled deterministic audit script against the frontend source directory (do not pass interactive prompts):

```bash
python3 scripts/audit_ui.py --root src --format json
```

If the output is large, filter by path (`--root src/lib/components`) or increase `--limit`. Inspect every reported violation (`emoji-as-ui`, `hardcoded-color`, `border-with-shadow`, `arbitrary-radius`, `heading-skip`, `tailwind-smell`, `secret-log`).

### Step 3: Load Checklist & Inspect Target Views
Read `references/audit-checklist.md` and evaluate the target `.svelte` and `.css` files against it. Identify 1–2 **model components** in the codebase whose structure and CSS quality match `DESIGN.md`, and imitate their exact pattern rather than inventing a new style.

### Step 4: Refactor Incrementally
1. Consolidate shared tokens (colors, spacing, typography, radii, elevations) into the root CSS variables file and document them in `DESIGN.md`.
2. Extract repeated UI structures into clean, DRY Svelte 5 components with explicit `$props()`.
3. Replace emojis with Lucide SVG icons, unify button variants, fix heading levels, and replace media queries inside components with `@container` queries where layout adapts to panel width.
4. Keep changes scoped to the requested views—one coherent unit per commit/pass.

### Step 5: Validation Loop
1. Re-run `python3 scripts/audit_ui.py --root <target_path> --format json`.
2. If any issues remain, fix them and re-run until clean (or only explicitly documented exceptions in `DESIGN.md` remain).
3. Run the project's frontend check (e.g., `npm run check` or `npx svelte-check` / `npm run build`) to confirm zero Svelte 5 or TypeScript/Vite regressions.
4. If **Playwright** (`@playwright/test` via `npm run test:ui` or Playwright MCP) **or** Antigravity's native **`browser` subagent** (Chrome DevTools) is available, launch/open the app preview (`http://localhost:5173`), inspect the rendered layout, test interactive states (hover, focus, right-click context menus, drag-and-drop, dense Excel rows), and confirm zero console errors or visual overflow before reporting completion.

---

## Flow B: Build a New View or Component

Progress:
- [ ] Step 1: Batched Discovery Questions (Human Leads, AI Executes)
- [ ] Step 2: Align with `DESIGN.md` & Model Components
- [ ] Step 3: Build Reference Slice First
- [ ] Step 4: Complete Implementation & Container Responsiveness
- [ ] Step 5: Validation Loop

### Step 1: Batched Discovery (Skip Only If Scope Is Already Unambiguous)
Before writing UI code for a non-trivial new screen or workflow, ask **1–2 concise batches** of questions covering:
- **Primary user goal & data density**: What must the judicial operator see/act on at a glance (e.g., Excel hearing rows, AI confidence/warnings, `.lokus` backup status)?
- **Entry point & container context**: Is this a full route, resizable panel, modal, or drawer?
- **Edge cases**: Empty states, long Spanish judicial strings, failed OpenAI/Gemini calls, corrupted `.xlsx` rows, and keyboard shortcuts.

Do **not** invent product decisions silently when requirements are ambiguous.

### Step 2: Align with `DESIGN.md` & Model Components
Read `DESIGN.md` (or create it from `assets/design-system-template.md` if missing). Identify existing model components in the repo to match CSS naming, Svelte 5 Runes patterns, and Lucide icon sizing.

### Step 3: Build a Reference Slice First
Build the primary shell/layout or first interactive view using existing CSS variables and components, then confirm visual alignment with the user (or via Playwright MCP screenshot/inspection) before expanding into secondary states or sub-views.

### Step 4: Complete Implementation
- Implement full Svelte 5 state handling (loading, empty, error, success, AI dual-provider states).
- Use Container Queries (`container-type: inline-size`) for adaptive panels inside the Tauri desktop window.
- Document any new reusable component or token in `DESIGN.md`.

### Step 5: Validation Loop
Run the exact same validation loop as Flow A Step 5 (`scripts/audit_ui.py` -> fix -> `svelte-check`/build -> Playwright MCP inspection if available).

---

## Project-Specific Context (Hydrate From Repo)

- **Frontend entry & token stylesheet**: `index.html` (theme anti-FOUC boot script), `src/main.js`, `src/App.svelte` (main orchestrator), and `src/app.css` (single global stylesheet defining `:root[data-theme="light"]` *"Azul Niebla Sereno"* and `:root[data-theme="dark"]` *"Slate Nocturno Sereno"* tokens, radii, elevations, and semantic classes).
- **Component directory**: `src/lib/components/` (`AudienciaForm.svelte`, `AutocompleteInput.svelte`, `CustomSelect.svelte`, `DatePickerInput.svelte`, `EstadisticasView.svelte`, `Modals.svelte`, `RegistrosTable.svelte`, `Sidebar.svelte`), plus shared catalogs in `src/lib/constants.js`.
- **Model components to imitate**:
  - `src/lib/components/RegistrosTable.svelte` — clean Svelte 5 `$props()` / `$derived.by()` filtering, zero raw color literals, `tabular-nums` alignment, and responsive `@container` table-to-card reflow.
  - `src/lib/components/CustomSelect.svelte` — accessible custom dropdown with keyboard navigation, Lucide icons, and theme variables.
  - `src/lib/components/Sidebar.svelte` — Svelte 5 Runes + `untrack()`, Pointer Events Drag & Drop tree, and dual-theme folder/Excel color tokens.
- **CSS naming convention**: Semantic global classes in `src/app.css` paired with semantic tokens (`--bg-*`, `--border-*`, `--text-*`, `--accent-*`, `--ia-*`, `--status-*`, `--excel-icon`, `--folder-*`, `--radius-*`, `--shadow-*`) and CSS Container Queries (`container-name: app` on `.desktop-frame`, `container-name: workspace` on `.workspace-pane`). Note: semantic layout classes ending in `-grid` (e.g. `.motivos-grid`, `.stats-kpi-grid`) are custom CSS Grid classes, not Tailwind utilities.
- **Lucide import convention**: `import { Calendar, FileSpreadsheet } from "@lucide/svelte";` (uses `@lucide/svelte` `^1.74.0` from `package.json`).
- **Verification commands**: `python3 .agents/skills/lokus-ui-craft/scripts/audit_ui.py --root src --format json && npm run build` (plus `npm run test:ui` with Playwright or live inspection via Antigravity's native `browser` subagent at `http://localhost:5173`, and `cargo test --offline --manifest-path src-tauri/Cargo.toml` when IPC/backend contracts are touched).
- **Known legacy files not to restyle without asking**: `templates/plantilla_audiencias.xlsx` (immutable official Defensoría template), `src-tauri/src/` backend business rules, the `<meta name="color-scheme">` boot script in `index.html`, and the Pointer Events Drag & Drop handlers/attributes (`[data-drop-folder]`, `iniciarPosibleArrastre`, `manejarPointerMoveGlobal`, `manejarPointerUpGlobal`) in `src/lib/components/Sidebar.svelte`.
