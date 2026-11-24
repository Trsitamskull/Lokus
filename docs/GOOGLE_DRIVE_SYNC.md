# Sincronización con Google Drive (`drive.appdata`) en Lokus

Lokus permite sincronizar automáticamente tus planillas Excel, carpetas, estadísticas archivadas, listas personalizadas (defensores, juzgados y tipos de audiencia), historial de uso de catálogos y memoria de autoaprendizaje de IA entre tu computador (Windows / Linux / macOS) y tu celular (Android) usando tu propia cuenta de Google Drive **de forma 100 % gratuita y privada**.

---

## 1. Privacidad, seguridad y manejo de conflictos

- **Carpeta oculta aislada (`appDataFolder`):** Lokus utiliza únicamente el permiso `https://www.googleapis.com/auth/drive.appdata`. Solo puede leer y escribir los archivos que Lokus crea dentro de su carpeta oculta de aplicación; **no tiene acceso** a tus documentos, fotos ni carpetas personales de Google Drive.
- **Cero secretos en la nube:** Lokus **nunca sube** a Google Drive tus claves API de OpenAI o Gemini (`openai_api_key`, `gemini_api_key`) ni tus tokens OAuth (`access_token`, `refresh_token`). Esos datos permanecen únicamente en tu dispositivo (`config/settings.json` y `config/drive_sync.json`).
- **Cero pérdida de datos ante edición simultánea:** Si editas la misma planilla en dos dispositivos sin haber sincronizado antes, Lokus crea un respaldo automático en `.respaldos/` y guarda la versión remota como copia paralela `*_nube_conflicto.xlsx` para que puedas revisar ambas versiones sin perder ninguna audiencia.
- **Anti ping-pong e idempotencia:** La memoria de IA (`ia_memoria_<instancia>.json`) y el historial de uso (`uso_<instancia>.json`) se publican por instancia y se fusionan de forma idempotente; si vuelves a sincronizar sin cambios locales ni remotos, Lokus no realiza subidas redundantes.

---

## 2. Crear el proyecto gratuito en Google Cloud Console

No requiere tarjeta de crédito ni facturación activa.

1. Ingresa a [Google Cloud Console](https://console.cloud.google.com/) con tu cuenta de Google.
2. En el selector superior de proyectos, pulsa **Proyecto nuevo**.
   - **Nombre:** `Lokus Audiencias` (o el nombre que prefieras).
   - Pulsa **Crear** y selecciona el proyecto recién creado.
3. En el menú lateral ve a **APIs y servicios → Biblioteca** (*Library*).
4. Busca **Google Drive API**, ábrela y pulsa **Habilitar** (*Enable*).

---

## 3. Configurar la pantalla de consentimiento OAuth (`drive.appdata`)

1. En el menú lateral ve a **Google Auth Platform → Público objetivo / Pantalla de consentimiento OAuth** (*OAuth consent screen*).
2. Completa los datos básicos:
   - **Nombre de la aplicación:** `Lokus — Gestor de Audiencias`
   - **Correo de asistencia:** tu correo de mantenedor
   - **Tipo de público:** **Externo** (*External*) para que cualquier cuenta `@gmail.com` o Google Workspace pueda conectar su Drive.
3. Ve a **Acceso a datos / Alcances (*Scopes*)** y agrega únicamente:
   - `https://www.googleapis.com/auth/drive.appdata`
   - `openid`
   - `email`
   - `profile`
4. **Modo Prueba vs. Producción:**
   - **Modo Prueba (*Testing*):** funciona de inmediato con hasta **100 usuarios de prueba** (agrega los correos en **Público objetivo → Usuarios de prueba**). Ten en cuenta que en modo prueba Google caduca los *refresh tokens* a los 7 días.
   - **Publicar para público general (*In production*):** `drive.appdata` es un alcance **no sensible** (*non-sensitive*), por lo que **no requiere auditoría de seguridad paga (CASA)**. Solo requiere la verificación estándar de marca (nombre de la app, logotipo y enlace a política de privacidad) para eliminar la pantalla de advertencia *"Google no verificó esta app"* y permitir sesiones duraderas.

---

## 4. Crear las credenciales OAuth 2.0 (Escritorio y Android)

Lokus usa **OAuth 2.0 con PKCE (`S256`)**. Necesitarás un **Client ID** para escritorio y otro para Android (Google exige tipos de cliente distintos por plataforma).

### 4.1 Escritorio (Windows, Linux y macOS)

1. Ve a **Google Auth Platform → Clientes** (o **APIs y servicios → Credenciales**) y pulsa **Crear cliente**.
2. Selecciona **Tipo de aplicación: Aplicación de escritorio** (*Desktop app*).
   - Nombre: `Lokus Desktop`.
3. Pulsa **Crear**. Google te mostrará:
   - **ID de cliente** (`xxxx.apps.googleusercontent.com`) → úsalo en `LOKUS_GOOGLE_CLIENT_ID`.
   - **Secreto de cliente** (opcional) → Google emite un *client secret* para clientes tipo *Desktop app*; si Google te exige enviarlo al intercambiar el código, configúralo en `LOKUS_GOOGLE_CLIENT_SECRET` (como secreto de entorno o GitHub Secret, **nunca** en el código fuente).
4. Lokus abre automáticamente un servidor local efímero en `http://127.0.0.1:<puerto>/oauth2callback`, que Google admite nativamente para clientes *Desktop app* sin registrar puertos fijos.

### 4.2 Android

1. Pulsa nuevamente **Crear cliente** y selecciona **Tipo de aplicación: Android**.
   - **Nombre:** `Lokus Android`
   - **Nombre del paquete (*Package name*):** `com.josebustamante.gestoraudiencias`
   - **Huella digital del certificado SHA-1:** obtén el SHA-1 de tu keystore de firma release (o debug en desarrollo):
     ```bash
     keytool -list -v -keystore ruta/a/tu-release.jks -alias tu_alias
     ```
2. En la configuración avanzada del cliente Android, asegúrate de permitir el esquema de redirección personalizado (*Custom URI scheme*):
   - **Redirect URI:** `com.josebustamante.gestoraudiencias:/oauth2redirect`
3. Copia el **ID de cliente Android** generado (`yyyy.apps.googleusercontent.com`). Los clientes Android con PKCE **no utilizan** `client_secret`.
4. Al preparar el proyecto Android con `node scripts/configure-android.mjs`, Lokus configura automáticamente:
   - El `<intent-filter>` del esquema `com.josebustamante.gestoraudiencias` y `android:launchMode="singleTask"` en `AndroidManifest.xml`.
   - El método `window.LokusAndroid.openExternalAuthUrl(url)` para abrir el navegador del sistema y el callback `window.__lokusOnGoogleAuthCallback({ code, state, error, errorDescription })` al regresar a la app.

---

## 5. Configurar `LOKUS_GOOGLE_CLIENT_ID` al compilar o ejecutar

Lokus resuelve el Client ID en el siguiente orden de prioridad:
1. Override pasado al comando de conexión.
2. Valor guardado por el usuario en **Portabilidad → Nube (Drive) → Configuración avanzada (Client ID de Google)** (`config/drive_sync.json`).
3. Variable de compilación `LOKUS_GOOGLE_CLIENT_ID` embebida en el binario (`option_env!`).
4. Variable de entorno en tiempo de ejecución `LOKUS_GOOGLE_CLIENT_ID`.

> **Regla de seguridad:** Nunca escribas ni subas a Git ningún `Client ID` privado ni `Client Secret` dentro de archivos rastreados del repositorio.

### 5.1 Desarrollo local

**Linux / macOS:**
```bash
export LOKUS_GOOGLE_CLIENT_ID="1234567890-xxxxxxxx.apps.googleusercontent.com"
# Opcional solo si tu cliente Desktop en Google Cloud lo requiere:
export LOKUS_GOOGLE_CLIENT_SECRET="GOCSPX-..."
npm run tauri dev
```

**Windows (PowerShell):**
```powershell
$env:LOKUS_GOOGLE_CLIENT_ID = "1234567890-xxxxxxxx.apps.googleusercontent.com"
# Opcional:
$env:LOKUS_GOOGLE_CLIENT_SECRET = "GOCSPX-..."
npm run tauri dev
```

También puedes dejar estas variables sin definir en desarrollo y pegar tu Client ID directamente en la interfaz: **Exportar / Importar → Nube (Drive) → Configuración avanzada (Client ID de Google)**.

### 5.2 GitHub Actions (Releases automáticos)

1. En tu repositorio de GitHub ve a **Settings → Secrets and variables → Actions**.
2. Crea los siguientes secretos (*Repository secrets*):
   - `LOKUS_GOOGLE_CLIENT_ID_DESKTOP` (ID de cliente tipo *Desktop app*)
   - `LOKUS_GOOGLE_CLIENT_SECRET_DESKTOP` (opcional, si el cliente Desktop lo requiere)
   - `LOKUS_GOOGLE_CLIENT_ID_ANDROID` (ID de cliente tipo *Android*)
3. En tus workflows de compilación:
   - En `.github/workflows/release-windows-linux.yml`, expón en el paso de build:
     ```yaml
     env:
       LOKUS_GOOGLE_CLIENT_ID: ${{ secrets.LOKUS_GOOGLE_CLIENT_ID_DESKTOP }}
       LOKUS_GOOGLE_CLIENT_SECRET: ${{ secrets.LOKUS_GOOGLE_CLIENT_SECRET_DESKTOP }}
     ```
   - En `.github/workflows/release-android.yml`, expón en el paso de build:
     ```yaml
     env:
       LOKUS_GOOGLE_CLIENT_ID: ${{ secrets.LOKUS_GOOGLE_CLIENT_ID_ANDROID }}
     ```

Con eso, los instaladores generados por CI ya incluirán el Client ID correspondiente y los usuarios finales solo tendrán que pulsar **Conectar cuenta de Google** sin pasos técnicos adicionales.
