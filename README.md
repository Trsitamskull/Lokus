# Lokus — Gestor de Audiencias Judiciales (v4.1)

Aplicación nativa multiplataforma (**Windows**, **Linux** y build móvil **Android**) construida con **Rust + Tauri v2 + Svelte 5** para la gestión, control estadístico y autocompletado inteligente de las planillas oficiales de audiencias judiciales de la Defensoría del Pueblo (Colombia).

---

## Descarga Directa (Sin instalar dependencias)
Los artefactos compilados para **Windows**, **Linux** y **Android** se publican en [`https://github.com/Trsitamskull/Lokus/releases`](https://github.com/Trsitamskull/Lokus/releases):

- **Windows:**
  - `Lokus_3.0.0_x64-setup.exe`: Instalador recomendado de un solo archivo (incluye auto-instalación silenciosa de `WebView2` en caso de que el equipo no lo tenga).
  - `Lokus.exe`: Ejecutable portable directo (para Windows 10/11 con `WebView2`).
- **Linux (Fedora / Ubuntu / Debian):**
  - `.AppImage`: Ejecutable portable universal.
  - `.rpm` / `.deb`: Paquetes nativos de instalación.
- **Android (ARM64):**
  - `Lokus_<version>_android_arm64-unsigned.apk`: Instalación manual para pruebas en dispositivo.
  - `Lokus_<version>_android_arm64.aab`: Bundle para publicación en Play Console.

---

## Características Principales

- **Motor Excel fiel a la plantilla oficial (`umya-spreadsheet`):**
  - Plantilla oficial de la Defensoría del Pueblo incrustada en el binario (`include_bytes!`).
  - Escritura y lectura exacta en filas `11..=110` (hasta 100 registros por planilla), cálculo automático de totales en la fila `111` (`G111` realizadas, `I111..P111` causales, `Q111` no realizadas) y asignación de defensor público en `B8`.
  - Ordenamiento cronológico descendente automático y **hasta 3 respaldos históricos** automáticos por planilla en `.respaldos/`.
- **Autocompletado con IA (OpenAI y Google Gemini) + Autoaprendizaje continuo:**
  - Extracción estructurada desde notas de WhatsApp, actas o reportes en estilo descriptivo o telegráfico/compacto.
  - **Base jurídica integrada:** más de 65 patrones de causales (`Juez`, `Fiscalía`, `Defensor Público`, `Defensor Confianza`, `Usuario`, `INPEC`, `Víctima`, `ICBF`), jerarquía entre causas principales y reacciones en primera persona, preservación de abreviaciones ambiguas (`CTO`, `MPAL`, `PCTO`) y detección de fechas de reprogramación.
  - **Autoaprendizaje por correcciones:** cuando el usuario corrige algún campo tras usar el autocompletado, Lokus compara la predicción con lo guardado, analiza la diferencia en segundo plano con una 2ª llamada a la IA y genera/refuerza reglas locales que se inyectan dinámicamente en futuras extracciones (hasta 5 reglas y los 3 casos más similares).
  - **Exportación anonimizada (`Share2` en la barra superior):** exporta a `Descargas` un paquete `.json` reemplazando nombres y radicados por `[PROCESADO]`, `[RADICADO]` y `[NUMERO]` para alimentar `src-tauri/resources/aprendizaje_base.json` en nuevas versiones.
- **Estadísticas Mensuales y por Defensor:**
  - Vista dedicada de estadísticas con filtros combinables por **Mes**, **Defensor Público** y **Planilla**, indicadores KPI, distribución porcentual de causales y consolidado copiable al portapapeles.
- **Interfaz Adaptativa y Atajos de Teclado:**
  - Temas **Warm Obsidian** (oscuro) y **Azul Niebla Sereno** (claro) con cambio instantáneo (`Ctrl + T`).
  - Diseño responsivo con CSS `@container` queries, selectores buscables (`CustomSelect`) y sugerencias integradas (`AutocompleteInput`).

---

## Requisitos y Dependencias (Requirements)

> **Nota sobre versiones:** A partir de `v3.0`, Lokus migró de Python a **Rust + Tauri v2 + Svelte 5**, por lo que las dependencias se declaran en [`src-tauri/Cargo.toml`](src-tauri/Cargo.toml) y [`package.json`](package.json). El antiguo `requirements.txt` de la versión `v2.0` (Python + Flet) se conserva en la rama [`legacy-python`](https://github.com/Trsitamskull/Lokus/tree/legacy-python).

### 1. Requisitos para usuarios finales (Ejecutable compilado)

| Sistema Operativo | Requisitos |
| :--- | :--- |
| **Windows 10 / 11 (64-bit)** | Ninguno adicional usando `Lokus_3.0.0_x64-setup.exe` (instala `Microsoft Edge WebView2` automáticamente si falta). |
| **Linux (64-bit)** | `webkit2gtk-4.1` y `libappindicator` (incluidos o resueltos automáticamente al instalar el paquete `.rpm` / `.deb`). |

### 2. Requisitos para desarrollo (Compilar desde código fuente)

- **Rust:** `>= 1.77` (`rustup`, `cargo`)
- **Node.js:** `>= 20 LTS` (`npm`)
- **Librerías del sistema (solo en Linux para compilar Tauri):**
  - **Fedora:**
    ```bash
    sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel
    ```
  - **Ubuntu / Debian:**
    ```bash
    sudo apt-get update
    sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
    ```

### 3. Dependencias del Backend Rust (`src-tauri/Cargo.toml`)

| Crate | Versión | Propósito |
| :--- | :--- | :--- |
| `tauri` / `tauri-build` | `2` | Framework nativo de escritorio e IPC seguro |
| `umya-spreadsheet` | `2` | Lectura y escritura nativa de archivos Excel (`.xlsx`) |
| `reqwest` | `0.12` (`json`, `rustls-tls`) | Cliente HTTP asíncrono con TLS en Rust puro para OpenAI y Gemini |
| `tokio` | `1` (`full`) | Runtime asíncrono para llamadas de red e I/O en segundo plano |
| `serde` / `serde_json` | `1` | Serialización tipada de modelos, configuración y memoria de IA |
| `chrono` | `0.4` (`serde`) | Validación y cálculo de fechas (`DD/MM/YYYY`) y marcas de tiempo |
| `regex` | `1` | Preprocesador de jerga judicial, fechas compactas y anonimización |
| `dirs` | `5` | Rutas multiplataforma (`AppData`/`~/.config`, `Documentos`, `Descargas`) |
| `thiserror` | `2` | Manejo estructurado de errores |

### 4. Dependencias del Frontend (`package.json`)

| Paquete | Versión | Propósito |
| :--- | :--- | :--- |
| `svelte` | `^5.16.0` | Interfaz reactiva basada en Svelte 5 Runes (`$state`, `$derived`, `$effect`) |
| `vite` / `@sveltejs/vite-plugin-svelte` | `^6.0.0` / `^5.0.0` | Empaquetador y servidor de desarrollo ultrarrápido |
| `@tauri-apps/api` / `@tauri-apps/cli` | `^2.2.0` | Puente IPC con el backend en Rust y CLI de empaquetado |
| `@lucide/svelte` | `^1.74.0` | Iconografía vectorial ligera |

---

## Estructura del Proyecto

```text
Lokus/
├── src/                               # Frontend Svelte 5 + Vite 6
│   ├── lib/
│   │   ├── constants.js               # Catálogos oficiales de audiencias, causales y horas rápidas
│   │   └── components/
│   │       ├── AudienciaForm.svelte     # Formulario principal, barra inferior de acciones y cajón de IA
│   │       ├── AutocompleteInput.svelte # Autocompletado de texto integrado al sistema de temas
│   │       ├── CustomSelect.svelte      # Selector buscable para tipos de audiencia, meses y filtros
│   │       ├── DatePickerInput.svelte   # Campo de fecha DD/MM/AAAA con calendario desplegable e indicadores
│   │       ├── EstadisticasView.svelte  # Vista de estadísticas mensuales y por defensor
│   │       ├── Modals.svelte            # Diálogos (<dialog>): crear planilla, IA, defensores y respaldos
│   │       ├── RegistrosTable.svelte    # Tabla responsiva y resumen de totales (Fila 111)
│   │       └── Sidebar.svelte           # Navegador de planillas (.xlsx), defensor por archivo y tema
│   ├── App.svelte                     # Orquestador de estado, barra superior y atajos globales
│   ├── app.css                        # Variables CSS, temas Oscuro/Claro y @container queries
│   └── main.js                        # Punto de entrada del frontend
├── src-tauri/                         # Backend Rust + Tauri v2
│   ├── capabilities/                  # Permisos de ventana Tauri
│   ├── icons/                         # Iconos de aplicación (PNG / ICO)
│   ├── resources/
│   │   └── aprendizaje_base.json      # Base de reglas y casos reales embebida en el ejecutable
│   ├── src/
│   │   ├── ai.rs                      # Extractor dual (OpenAI + Gemini), preprocesador y análisis de errores
│   │   ├── aprendizaje.rs             # Memoria adaptativa de correcciones, similitud y exportación anonimizada
│   │   ├── config.rs                  # Persistencia de settings.json y enmascaramiento de API Keys
│   │   ├── excel.rs                   # Motor Excel atómico, ordenamiento cronológico y 3 respaldos
│   │   ├── lib.rs                     # Registro de comandos IPC (#[tauri::command])
│   │   ├── main.rs                    # Punto de entrada del binario Lokus
│   │   └── models.rs                  # Modelos de dominio, validadores y detección de duplicados
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── templates/
│   └── plantilla_audiencias.xlsx      # Plantilla oficial inmutable de la Defensoría del Pueblo
├── .github/workflows/                 # CI/CD: compilación y publicación en GitHub Releases
├── .agents/                           # Especificación del dominio judicial colombiano (SKILL.md)
├── AGENTS.md                          # Manual operativo y estándares de ingeniería
├── index.html
├── package.json
├── svelte.config.js
└── vite.config.js
```

> **Ubicación de archivos en uso:** En desarrollo, `archivos_creados/` y `config/` (`settings.json` e `ia_aprendizaje.json`) se guardan en la raíz del proyecto (ignorados en `.gitignore`). En instalaciones de producción, las planillas se guardan en `Documentos/Lokus/archivos_creados/` y la configuración/aprendizaje en la carpeta de datos del usuario del sistema (`AppData\Roaming\lokus-audiencias` en Windows o `~/.config/lokus-audiencias` en Linux).

---

## Comandos Principales

### Desarrollo local
```bash
npm install
npm run tauri dev
```

### Pruebas y verificación
```bash
npm run build
cargo test --offline --manifest-path src-tauri/Cargo.toml
```

### Compilación de instaladores locales
```bash
npm run tauri build
```

### Compilación Android (APK + AAB)
> Requiere tener instalado Android SDK/NDK y definir `ANDROID_HOME` y `NDK_HOME`.

```bash
npm run tauri android init --ci
npm run tauri android build --target aarch64 --apk --aab --ci
```

Los artefactos se generan en:

- `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk`
- `src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`

---

## Atajos de Teclado

| Atajo | Acción |
| :--- | :--- |
| `Ctrl + S` | Registrar audiencia / Guardar cambios |
| `Ctrl + L` | Limpiar campos del formulario |
| `Ctrl + I` | Abrir autocompletado con IA |
| `Ctrl + Enter` | Extraer datos al formulario desde el cuadro de IA |
| `Ctrl + N` | Crear nueva planilla Excel |
| `Ctrl + 1 / 2 / 3` | Cambiar entre *Nueva Audiencia*, *Registros* y *Estadísticas* |
| `Ctrl + Shift + S` | Exportar copia de la planilla actual a `Descargas` |
| `Ctrl + T` | Alternar entre tema *Warm Obsidian* (oscuro) y *Azul Niebla Sereno* (claro) |
| `F5` / `Esc` | Recargar planilla actual / Cerrar modal o cancelar edición |
