## UI & Design System Rules (Lokus Judicial)

- **Design Source of Truth**: Read and maintain `DESIGN.md` at the repository root for all colors, typography, spacing, border radii, elevations, and reusable Svelte 5 UI primitives. Never leave visual or architectural decisions solely in chat history.
- **Fixed Frontend Stack**: Use **Svelte 5 Runes** (`$state`, `$derived`, `$effect`, `$props`), **native CSS Variables** (`var(--...)`), **CSS Container Queries** (`@container`), **Lucide icons**, and **Pointer Events** for Drag & Drop. Do not introduce Tailwind CSS, React, or external CSS frameworks.
- **UI Skill Enforcement**: Before auditing, restyling, or building any UI view, modal, or component, activate and follow the `lokus-ui-craft` skill (including `scripts/audit_ui.py` and `references/audit-checklist.md`).
- **Quality & Safety**: Never use emojis as UI icons, never mix arbitrary borders and shadows on the same surface, keep commits small and scoped per view, and never log or hardcode OpenAI/Gemini API keys or `.env` secrets in frontend code.
