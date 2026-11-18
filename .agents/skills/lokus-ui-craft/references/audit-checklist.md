# Lokus Judicial UI Audit Checklist

Load this checklist during **Flow A (Step 3)** and before final sign-off on any UI change. Every item reflects deterministic anti-slop rules and desktop ergonomics for **Lokus Judicial**.

---

## 1. Anti-Slop Visual Hygiene

- [ ] **No Border + Shadow Clash**: Surfaces use either a hairline border (`1px solid var(--border-*)`) OR a subtle elevation shadow (`var(--shadow-*)`), never both unless explicitly justified in `DESIGN.md` (e.g., floating popover over a similarly colored panel).
- [ ] **Consistent Border Radii**: All cards, inputs, modals, badges, and buttons draw from the `DESIGN.md` radius scale (`--radius-sm`, `--radius-md`, `--radius-lg`). Zero arbitrary one-off values (`3px`, `7px`, `11px`, `14px`).
- [ ] **Uniform Button Geometry**: Buttons sharing a tier (`primary`, `secondary`, `ghost`, `danger`, `icon`) share exact height, horizontal padding, font-weight, radius, and icon gap.
- [ ] **Vector Icons Only (Lucide)**: Zero Unicode emojis (`📄`, `⚖️`, `✅`, `⚠️`, `⚙️`) used as UI icons, navigation markers, or action triggers. All icons use consistent Lucide stroke width and size tokens (`14px` inline, `16px` controls, `20px` headers).
- [ ] **No Generic SaaS / AI Ornamentation**: Remove gratuitous neon gradients, oversized marketing hero padding, fake glassmorphism blurs that hurt text legibility, or generic decorative SVG blobs.

---

## 2. CSS Architecture & Svelte 5 Discipline

- [ ] **CSS Variables Only**: No hardcoded hex, `rgb()`, `rgba()`, or `hsl()` values inside `.svelte` component styles—only `var(--...)` semantic tokens (colors, spacing, fonts, radii, z-index, transitions).
- [ ] **Container Queries Over Viewport Hacks**: Resizable desktop panels, cards, and hearing detail drawers adapt using `@container` rules (`container-type: inline-size`) rather than fragile global `@media` queries alone.
- [ ] **DRY Component Extraction**: Repeated table cells, status pills, AI provider badges (OpenAI / Gemini), or toolbar groups are extracted into reusable Svelte 5 components using `$props()` rather than copy-pasted markup.
- [ ] **Preserved Reactivity & DnD**: Svelte 5 Runes (`$state`, `$derived`, `$effect`, `$props`) and Pointer Events Drag & Drop handlers (`onpointerdown`, `onpointermove`, `onpointerup`, `setPointerCapture`) remain intact with clear visual drag-state feedback (`cursor`, drop target highlight).

---

## 3. Typography & Hierarchy

- [ ] **Strict Heading Sequence**: Each view/dialog follows `h1 -> h2 -> h3` without skipping levels. Visual scale is controlled via CSS classes/tokens, not by picking heading tags for font size.
- [ ] **Tabular Data Alignment**: Hearing dates, times, court file numbers (*radicados*), row counts, and confidence scores use `font-variant-numeric: tabular-nums` so columns align cleanly in dense tables.
- [ ] **Long Spanish Judicial Text Resilience**: Long names of Colombian *juzgados*, *despachos*, *defensores*, and *audiencias* handle overflow intentionally (`text-overflow: ellipsis` with accessible `title`/tooltip or clean wrapping) without breaking grid tracks or pushing action buttons off-screen.

---

## 4. Desktop Ergonomics & Domain States (Tauri v2)

- [ ] **Complete Interactive States**: Every interactive element defines distinct `hover`, `focus-visible` (accessible keyboard ring), `active`, and `disabled` states.
- [ ] **Explicit Async & Error States**: Views touching Rust/Tokio commands, `.xlsx` (`umya-spreadsheet`) read/write, `.lokus` compressed backups, or dual AI (OpenAI / Gemini) calls handle:
  - Loading / processing state (non-blocking indicator, clear progress)
  - Empty state (concise guidance, no dead blank box)
  - Error / fallback state (actionable message without exposing raw stack traces or API keys)
- [ ] **Tauri Window & Scroll Discipline**: Scroll containers live on the data region (e.g., hearing table body), keeping primary toolbars, file-save actions, and status bars anchored without double scrollbars.
- [ ] **Zero Credential Exposure**: No API keys, `.env` secrets, or sensitive user tokens appear in UI logs, tooltips, comments, or DOM attributes.
