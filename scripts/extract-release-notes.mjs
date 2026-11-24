import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

function parseArgs(argv) {
  const opts = {
    version: "",
    changelog: "CHANGELOG.md",
    tauriConf: "src-tauri/tauri.conf.json",
    outputFile: "",
    githubOutput: "",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--version" && argv[i + 1]) {
      opts.version = argv[++i];
    } else if (arg === "--changelog" && argv[i + 1]) {
      opts.changelog = argv[++i];
    } else if (arg === "--tauri-conf" && argv[i + 1]) {
      opts.tauriConf = argv[++i];
    } else if (arg === "--output-file" && argv[i + 1]) {
      opts.outputFile = argv[++i];
    } else if (arg === "--github-output" && argv[i + 1]) {
      opts.githubOutput = argv[++i];
    }
  }
  return opts;
}

function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function extraerSeccionChangelog(contenido, versionLimpia) {
  const lineas = contenido.split(/\r?\n/);
  const headerRegex = new RegExp(
    `^##\\s+\\[?v?${escapeRegExp(versionLimpia)}\\]?(?:\\s|$|-)`,
    "i"
  );
  let start = -1;
  for (let i = 0; i < lineas.length; i += 1) {
    if (headerRegex.test(lineas[i].trim())) {
      start = i + 1;
      break;
    }
  }
  if (start === -1) return "";
  const cuerpo = [];
  for (let i = start; i < lineas.length; i += 1) {
    if (/^##\s+/.test(lineas[i].trim())) break;
    cuerpo.push(lineas[i]);
  }
  return cuerpo.join("\n").trim();
}

function generarFallbackDesdeGit(versionLimpia) {
  try {
    const tagsRaw = execSync("git tag --sort=-v:refname", { encoding: "utf8" }).trim();
    const tags = tagsRaw ? tagsRaw.split(/\r?\n/).filter(Boolean) : [];
    const currentTag = `v${versionLimpia}`;
    const prevTag = tags.find((t) => t !== currentTag);
    const range = prevTag ? `${prevTag}..HEAD` : "-n 12";
    const commits = execSync(`git log ${range} --pretty=format:"- %s"`, {
      encoding: "utf8",
    }).trim();
    if (commits) {
      return `### Cambios en v${versionLimpia}\n\n${commits}`;
    }
  } catch (_) {}
  return `### Cambios en v${versionLimpia}\n\n- Mejoras de estabilidad, rendimiento y experiencia de uso en Lokus.`;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  let version = opts.version.trim().replace(/^v/i, "");
  if (!version) {
    const tauriPath = path.resolve(opts.tauriConf);
    const tauriJson = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
    version = String(tauriJson.version || "").trim().replace(/^v/i, "");
  }
  if (!version) {
    throw new Error("No se pudo determinar la versión para generar las notas de release.");
  }

  let notas = "";
  const changelogPath = path.resolve(opts.changelog);
  if (fs.existsSync(changelogPath)) {
    const contenido = fs.readFileSync(changelogPath, "utf8");
    notas = extraerSeccionChangelog(contenido, version);
  }
  if (!notas) {
    notas = generarFallbackDesdeGit(version);
  }

  const descargas = [
    "### Descargas",
    "- **Windows:** `Lokus.exe` (ejecutable portable directo) o `Lokus-setup.exe` (instalador con WebView2 incluido).",
    "- **Linux:** `.AppImage`, `.deb` y `.rpm`.",
    `- **Android:** \`Lokus_${version}_android_arm64.apk\` (instalable en teléfonos ARM64) y \`.aab\`.`,
  ].join("\n");

  const cuerpoFinal = `${notas}\n\n${descargas}\n`;

  if (opts.outputFile) {
    fs.writeFileSync(path.resolve(opts.outputFile), cuerpoFinal, "utf8");
  }
  if (opts.githubOutput) {
    const delim = `EOF_LOKUS_NOTES_${Date.now()}`;
    fs.appendFileSync(
      path.resolve(opts.githubOutput),
      `body<<${delim}\n${cuerpoFinal}\n${delim}\n`,
      "utf8"
    );
  }
  if (!opts.outputFile && !opts.githubOutput) {
    process.stdout.write(cuerpoFinal);
  }
}

main();
