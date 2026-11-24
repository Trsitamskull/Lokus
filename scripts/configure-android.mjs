#!/usr/bin/env node
/**
 * Configura de forma idempotente el proyecto Android generado por `tauri android init`
 * (`src-tauri/gen/android/`) para producción:
 * - MainActivity.kt: edge-to-edge con insets reales (barras del sistema + recorte + teclado IME),
 *   botón Atrás inteligente conectado a Svelte y puente nativo `window.LokusAndroid`
 *   (Hoja de Compartir directa vía FileProvider sin duplicar en Descargas + respaldo MediaStore opcional + sincronización de tema).
 * - proguard-lokus.pro: preserva `@JavascriptInterface` bajo optimización R8 en builds release.
 * - AndroidManifest.xml: `windowSoftInputMode="adjustResize"`.
 * - res/xml/file_paths.xml: rutas `cache-path` y `files-path` para FileProvider.
 * - app/build.gradle.kts: firma release automática desde variables de entorno cuando estén presentes.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..");
const ANDROID_DIR = path.join(ROOT_DIR, "src-tauri", "gen", "android");
const APP_DIR = path.join(ANDROID_DIR, "app");

const PACKAGE_NAME = "com.josebustamante.gestoraudiencias";
const PACKAGE_REL_PATH = PACKAGE_NAME.replaceAll(".", path.sep);

const MAIN_ACTIVITY_PATH = path.join(
  APP_DIR,
  "src",
  "main",
  "java",
  PACKAGE_REL_PATH,
  "MainActivity.kt"
);
const MANIFEST_PATH = path.join(APP_DIR, "src", "main", "AndroidManifest.xml");
const FILE_PATHS_XML = path.join(APP_DIR, "src", "main", "res", "xml", "file_paths.xml");
const PROGUARD_LOKUS_PATH = path.join(APP_DIR, "proguard-lokus.pro");
const APP_GRADLE_PATH = path.join(APP_DIR, "build.gradle.kts");

const MAIN_ACTIVITY_KT = `package ${PACKAGE_NAME}

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.content.FileProvider
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class MainActivity : TauriActivity() {
  private var activeWebView: WebView? = null
  private var isLightTheme: Boolean = false

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT)
    )
    super.onCreate(savedInstanceState)

    val rootContent = findViewById<View>(android.R.id.content)
    applySystemBarAppearance(isLightTheme)
    ViewCompat.setOnApplyWindowInsetsListener(rootContent) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(rootContent)

    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val wv = activeWebView
        if (wv == null) {
          moveTaskToBack(true)
          return
        }
        wv.evaluateJavascript(
          "(function(){try{return typeof window.__lokusHandleAndroidBack==='function'?window.__lokusHandleAndroidBack():'exit';}catch(e){return 'exit';}})()"
        ) { raw ->
          if (raw != "\\"handled\\"") {
            runOnUiThread { moveTaskToBack(true) }
          }
        }
      }
    })
  }

  @SuppressLint("JavascriptInterface")
  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    activeWebView = webView
    webView.setBackgroundColor(themeBackgroundColor(isLightTheme))
    webView.addJavascriptInterface(LokusAndroidBridge(), "LokusAndroid")
  }

  override fun onDestroy() {
    activeWebView = null
    super.onDestroy()
  }

  private fun themeBackgroundColor(isLight: Boolean): Int {
    return if (isLight) Color.parseColor("#EEF4FA") else Color.parseColor("#151B26")
  }

  private fun applySystemBarAppearance(isLight: Boolean) {
    isLightTheme = isLight
    val bgColor = themeBackgroundColor(isLight)
    findViewById<View>(android.R.id.content)?.setBackgroundColor(bgColor)
    window.decorView.setBackgroundColor(bgColor)
    activeWebView?.setBackgroundColor(bgColor)
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    controller.isAppearanceLightStatusBars = isLight
    controller.isAppearanceLightNavigationBars = isLight
  }

  private fun sanitizeExportFileName(rawName: String?): String {
    val base = File(rawName ?: "").name
      .trim()
      .replace(Regex("[\\\\\\\\/:*?\\"<>|\\\\p{Cntrl}]"), "_")
      .trim('.')
    return if (base.isEmpty()) "archivo_lokus" else base
  }

  private fun saveBytesToDownloads(bytes: ByteArray, fileName: String, mimeType: String): String {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      val values = ContentValues().apply {
        put(MediaStore.Downloads.DISPLAY_NAME, fileName)
        put(MediaStore.Downloads.MIME_TYPE, mimeType)
        put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Lokus")
        put(MediaStore.Downloads.IS_PENDING, 1)
      }
      val resolver = contentResolver
      val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        ?: throw IllegalStateException("No se pudo crear el archivo en Descargas/Lokus")
      try {
        resolver.openOutputStream(uri)?.use { out ->
          out.write(bytes)
          out.flush()
        } ?: throw IllegalStateException("No se pudo abrir el flujo de escritura en Descargas")
        values.clear()
        values.put(MediaStore.Downloads.IS_PENDING, 0)
        resolver.update(uri, values, null, null)
        var actualName = fileName
        try {
          resolver.query(uri, arrayOf(MediaStore.Downloads.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
              val queried = cursor.getString(0)?.trim()
              if (!queried.isNullOrEmpty()) actualName = queried
            }
          }
        } catch (_: Exception) {
        }
        return "Descargas/Lokus/$actualName"
      } catch (e: Exception) {
        try {
          resolver.delete(uri, null, null)
        } catch (_: Exception) {
        }
        throw e
      }
    }

    val baseDir = getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: filesDir
    val targetDir = File(baseDir, "Lokus").apply { mkdirs() }
    val targetFile = File(targetDir, fileName)
    FileOutputStream(targetFile).use { out ->
      out.write(bytes)
      out.flush()
    }
    return targetFile.absolutePath
  }

  private fun prepareShareFile(bytes: ByteArray, fileName: String): File {
    val exportsDir = File(cacheDir, "exports").apply { mkdirs() }
    val cutoff = System.currentTimeMillis() - 24L * 60L * 60L * 1000L
    exportsDir.listFiles()?.forEach { old ->
      if (old.isFile && old.lastModified() < cutoff) {
        try {
          old.delete()
        } catch (_: Exception) {
        }
      }
    }
    val shareFile = File(exportsDir, fileName)
    FileOutputStream(shareFile).use { out ->
      out.write(bytes)
      out.flush()
    }
    return shareFile
  }

  inner class LokusAndroidBridge {
    @JavascriptInterface
    fun isAvailable(): Boolean = true

    @JavascriptInterface
    fun setTheme(isLight: Boolean) {
      runOnUiThread { applySystemBarAppearance(isLight) }
    }

    @JavascriptInterface
    fun copyText(text: String): Boolean {
      if (isFinishing || isDestroyed) return false
      val ok = AtomicBoolean(false)
      val latch = CountDownLatch(1)
      runOnUiThread {
        try {
          val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
          if (clipboard != null) {
            clipboard.setPrimaryClip(ClipData.newPlainText("Lokus", text))
            ok.set(true)
          }
        } catch (_: Exception) {
        } finally {
          latch.countDown()
        }
      }
      return try {
        latch.await(1500, TimeUnit.MILLISECONDS) && ok.get()
      } catch (_: InterruptedException) {
        Thread.currentThread().interrupt()
        false
      }
    }

    @JavascriptInterface
    fun shareFile(
      base64Data: String,
      fileName: String,
      mimeType: String
    ): String {
      val response = JSONObject()
      return try {
        val safeName = sanitizeExportFileName(fileName)
        val safeMime = mimeType.trim().ifEmpty { "application/octet-stream" }
        val bytes = Base64.decode(base64Data, Base64.DEFAULT)

        val shareFile = prepareShareFile(bytes, safeName)
        val contentUri = FileProvider.getUriForFile(
          this@MainActivity,
          "\${packageName}.fileprovider",
          shareFile
        )
        runOnUiThread {
          if (isFinishing || isDestroyed) return@runOnUiThread
          try {
            val sendIntent = Intent(Intent.ACTION_SEND).apply {
              type = safeMime
              putExtra(Intent.EXTRA_STREAM, contentUri)
              putExtra(Intent.EXTRA_SUBJECT, safeName)
              clipData = ClipData.newRawUri(safeName, contentUri)
              addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            val chooser = Intent.createChooser(sendIntent, "Compartir $safeName").apply {
              addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            startActivity(chooser)
          } catch (_: Exception) {
          }
        }

        response.put("ok", true)
        response.put("shared", true)
        response.toString()
      } catch (e: Exception) {
        response.put("ok", false)
        response.put("error", e.message ?: "Error al compartir archivo en Android")
        response.toString()
      }
    }

    @JavascriptInterface
    fun saveToDownloadsAndShare(
      base64Data: String,
      fileName: String,
      mimeType: String,
      openShare: Boolean
    ): String {
      if (openShare) {
        return shareFile(base64Data, fileName, mimeType)
      }
      val response = JSONObject()
      return try {
        val safeName = sanitizeExportFileName(fileName)
        val safeMime = mimeType.trim().ifEmpty { "application/octet-stream" }
        val bytes = Base64.decode(base64Data, Base64.DEFAULT)
        val savedLocation = saveBytesToDownloads(bytes, safeName, safeMime)

        response.put("ok", true)
        response.put("savedLocation", savedLocation)
        response.put("shared", false)
        response.toString()
      } catch (e: Exception) {
        response.put("ok", false)
        response.put("error", e.message ?: "Error al exportar archivo en Android")
        response.toString()
      }
    }
  }
}
`;

const PROGUARD_LOKUS = `# Preservar el puente JavaScript nativo de Lokus y anotaciones @JavascriptInterface bajo R8
-keepattributes RuntimeVisibleAnnotations,AnnotationDefault
-keepclassmembers class ${PACKAGE_NAME}.MainActivity$LokusAndroidBridge {
    @android.webkit.JavascriptInterface <methods>;
}
-keep class ${PACKAGE_NAME}.MainActivity$LokusAndroidBridge { *; }
`;

const FILE_PATHS_CONTENT = `<?xml version="1.0" encoding="utf-8"?>
<paths xmlns:android="http://schemas.android.com/apk/res/android">
  <external-path name="my_images" path="." />
  <cache-path name="my_cache_images" path="." />
  <cache-path name="shared_exports" path="exports/" />
  <files-path name="app_files" path="." />
</paths>
`;

function ensureAndroidProjectExists() {
  if (!fs.existsSync(APP_GRADLE_PATH)) {
    throw new Error(
      `No se encontró ${APP_GRADLE_PATH}. Ejecuta primero \`npx tauri android init --ci\`.`
    );
  }
}

function readCompileSdk() {
  ensureAndroidProjectExists();
  const content = fs.readFileSync(APP_GRADLE_PATH, "utf8");
  const match = content.match(/compileSdk\s*=\s*(\d+)/);
  return match ? match[1] : "35";
}

function patchAndroidManifest() {
  if (!fs.existsSync(MANIFEST_PATH)) {
    throw new Error(`No se encontró ${MANIFEST_PATH}`);
  }
  let xml = fs.readFileSync(MANIFEST_PATH, "utf8");
  if (!xml.includes("android:windowSoftInputMode=")) {
    xml = xml.replace(
      /(<activity\b[^>]*android:name="\.MainActivity")/,
      '$1\n            android:windowSoftInputMode="adjustResize"'
    );
  } else {
    xml = xml.replace(
      /android:windowSoftInputMode="[^"]*"/,
      'android:windowSoftInputMode="adjustResize"'
    );
  }
  fs.writeFileSync(MANIFEST_PATH, xml, "utf8");
}

function patchBuildGradleSigning() {
  let gradle = fs.readFileSync(APP_GRADLE_PATH, "utf8");

  // Eliminar bloques previos si existieran para garantizar idempotencia
  gradle = gradle.replace(
    /[ \t]*\/\/ LOKUS_SIGNING_CONFIG_START[\s\S]*?\/\/ LOKUS_SIGNING_CONFIG_END\r?\n?/g,
    ""
  );
  gradle = gradle.replace(
    /[ \t]*\/\/ LOKUS_SIGNING_ASSIGN_START[\s\S]*?\/\/ LOKUS_SIGNING_ASSIGN_END\r?\n?/g,
    ""
  );

  const signingConfigBlock =
    `    // LOKUS_SIGNING_CONFIG_START\n` +
    `    val lokusKeystorePath = System.getenv("LOKUS_ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }\n` +
    `    val lokusKeystoreFile = lokusKeystorePath?.let { file(it) }?.takeIf { it.exists() }\n` +
    `    if (lokusKeystoreFile != null) {\n` +
    `        signingConfigs {\n` +
    `            create("release") {\n` +
    `                storeFile = lokusKeystoreFile\n` +
    `                storePassword = System.getenv("LOKUS_ANDROID_KEYSTORE_PASSWORD") ?: ""\n` +
    `                keyAlias = System.getenv("LOKUS_ANDROID_KEY_ALIAS") ?: ""\n` +
    `                keyPassword = System.getenv("LOKUS_ANDROID_KEY_PASSWORD") ?: ""\n` +
    `            }\n` +
    `        }\n` +
    `    }\n` +
    `    // LOKUS_SIGNING_CONFIG_END\n`;

  if (!gradle.includes("    buildTypes {")) {
    throw new Error("No se encontró el bloque `buildTypes {` en app/build.gradle.kts");
  }
  gradle = gradle.replace("    buildTypes {", `${signingConfigBlock}    buildTypes {`);

  const signingAssignBlock =
    `\n            // LOKUS_SIGNING_ASSIGN_START\n` +
    `            if (lokusKeystoreFile != null) {\n` +
    `                signingConfig = signingConfigs.getByName("release")\n` +
    `            }\n` +
    `            // LOKUS_SIGNING_ASSIGN_END`;

  if (!gradle.includes('        getByName("release") {')) {
    throw new Error('No se encontró `getByName("release") {` en app/build.gradle.kts');
  }
  gradle = gradle.replace(
    '        getByName("release") {',
    `        getByName("release") {${signingAssignBlock}`
  );

  fs.writeFileSync(APP_GRADLE_PATH, gradle, "utf8");
}

function configureAndroid() {
  ensureAndroidProjectExists();

  fs.mkdirSync(path.dirname(MAIN_ACTIVITY_PATH), { recursive: true });
  fs.writeFileSync(MAIN_ACTIVITY_PATH, MAIN_ACTIVITY_KT, "utf8");

  fs.mkdirSync(path.dirname(FILE_PATHS_XML), { recursive: true });
  fs.writeFileSync(FILE_PATHS_XML, FILE_PATHS_CONTENT, "utf8");

  fs.writeFileSync(PROGUARD_LOKUS_PATH, PROGUARD_LOKUS, "utf8");

  patchAndroidManifest();
  patchBuildGradleSigning();

  const compileSdk = readCompileSdk();
  console.log(
    `[configure-android] Proyecto Android configurado correctamente (compileSdk=${compileSdk}).`
  );
}

const args = process.argv.slice(2);
if (args.includes("--print-compile-sdk")) {
  process.stdout.write(readCompileSdk());
} else {
  configureAndroid();
}
