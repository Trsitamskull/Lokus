# Lokus — Gestor de Audiencias Judiciales (v3.0)

Aplicación de escritorio nativa desarrollada en **Rust + Tauri v2 + Svelte 5** para la gestión de planillas oficiales de audiencias de la Defensoría del Pueblo.

---

## Estructura del Proyecto

```text
Lokus/
├── src/                               # Frontend Svelte 5 + Vite
│   ├── lib/
│   │   ├── constants.js               # Catálogos de audiencias, causales y horas rápidas
│   │   └── components/
│   │       ├── AudienciaForm.svelte   # Formulario principal + tira de registros recientes
│   │       ├── CustomSelect.svelte    # Selector buscable para tipos de audiencia
│   │       ├── Modals.svelte          # Diálogos nativos (<dialog>): crear, IA, defensores, respaldos
│   │       ├── RegistrosTable.svelte  # Tabla filtrable y chips de Fila 111
│   │       └── Sidebar.svelte         # Navegador de planillas (.xlsx), cupo y estado IA
│   ├── App.svelte                     # Orquestador de estado y comandos Tauri
│   ├── app.css                        # Variables CSS y temas Oscuro / Claro
│   └── main.js                        # Punto de entrada del frontend
├── src-tauri/                         # Backend Rust + Tauri v2
│   ├── capabilities/                  # Permisos de ventana Tauri
│   ├── icons/                         # Iconos de aplicación (PNG / ICO)
│   ├── src/
│   │   ├── ai.rs                      # Cliente OpenAI + Gemini y reglas jurídicas
│   │   ├── config.rs                  # Configuración en JSON y directorio de defensores
│   │   ├── excel.rs                   # Motor Excel atómico, ordenamiento y 3 respaldos
│   │   ├── lib.rs                     # Comandos IPC asíncronos (#[tauri::command])
│   │   ├── main.rs                    # Arranque del binario de escritorio
│   │   └── models.rs                  # Modelos, validadores y detección de duplicados
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── templates/
│   └── plantilla_audiencias.xlsx      # Plantilla oficial inmutable de la Defensoría
├── .github/workflows/                 # Compilación multiplataforma (Windows / Linux)
├── .agents/                           # Reglas y especificación del dominio judicial
├── AGENTS.md                          # Estándares de ingeniería y arquitectura
├── index.html
├── package.json
├── svelte.config.js
└── vite.config.js
```

> Nota: Las carpetas `archivos_creados/` (planillas `.xlsx` y `.respaldos/`) y `config/` (`settings.json` con API keys) permanecen ignoradas por Git. En instalaciones de producción las planillas se almacenan en `Documentos/Lokus/archivos_creados/`.

---

## Comandos Principales

### Desarrollo local
```bash
npm install
cargo tauri dev
```

### Pruebas y verificación
```bash
npm run build
cargo test --offline --manifest-path src-tauri/Cargo.toml
```

### Respaldo legado (Python + Flet)
La versión anterior en Python (`v2.0`) se conserva congelada en la rama `legacy-python` y el tag `v2.0-python-flet`.
