import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const SUITE_DIR = __dirname;
export const REPO_ROOT = path.resolve(__dirname, "..", "..");
export const CARGO_TOML_PATH = path.join(REPO_ROOT, "src-tauri", "Cargo.toml");
export const REAL_BASE_PATH = path.join(REPO_ROOT, "src-tauri", "resources", "aprendizaje_base.json");
export const REAL_CATALOGOS_PATH = path.join(REPO_ROOT, "src-tauri", "resources", "juzgados_base.json");
export const REAL_LEDGER_PATH = path.join(REPO_ROOT, "src-tauri", "resources", "ingesta_ledger.json");
export const DOCS_PLAN_PATH = path.join(REPO_ROOT, "docs", "PLAN_INGESTA_AUTOAPRENDIZAJE.md");
export const DOCS_REPORTE_REAL_PATH = path.join(REPO_ROOT, "docs", "INGESTA_REPORTE_REAL.md");
export const CONSTANTS_JS_PATH = path.join(REPO_ROOT, "src", "lib", "constants.js");
export const MODELS_RS_PATH = path.join(REPO_ROOT, "src-tauri", "src", "models.rs");
export const AI_RS_PATH = path.join(REPO_ROOT, "src-tauri", "src", "ai.rs");
export const APRENDIZAJE_RS_PATH = path.join(REPO_ROOT, "src-tauri", "src", "aprendizaje.rs");
export const TEMPLATE_XLSX_PATH = path.join(REPO_ROOT, "templates", "plantilla_audiencias.xlsx");
export const TEMPLATES_DIR = path.join(REPO_ROOT, "templates");

let cachedBinPath = null;

export function resolveExpectedBinPath() {
  if (process.env.LOKUS_INGESTAR_BIN) {
    return path.resolve(process.env.LOKUS_INGESTAR_BIN);
  }
  const exe = process.platform === "win32" ? "lokus_ingestar.exe" : "lokus_ingestar";
  return path.join(REPO_ROOT, "src-tauri", "target", "debug", exe);
}

export function ensureIngestarBuilt() {
  if (cachedBinPath && fs.existsSync(cachedBinPath)) {
    return cachedBinPath;
  }

  if (process.env.LOKUS_INGESTAR_BIN) {
    const custom = path.resolve(process.env.LOKUS_INGESTAR_BIN);
    if (!fs.existsSync(custom)) {
      throw new Error(`LOKUS_INGESTAR_BIN points to missing binary: ${custom}`);
    }
    cachedBinPath = custom;
    return cachedBinPath;
  }

  const res = spawnSync(
    "cargo",
    ["build", "--manifest-path", CARGO_TOML_PATH, "--bin", "lokus_ingestar"],
    {
      cwd: REPO_ROOT,
      encoding: "utf8",
      timeout: 180_000,
    }
  );

  if (res.error) {
    throw new Error(`Failed to invoke cargo build for lokus_ingestar: ${res.error.message}`);
  }
  if (res.status !== 0) {
    throw new Error(
      `cargo build --bin lokus_ingestar failed (exit ${res.status}):\n${res.stderr || res.stdout}`
    );
  }

  const binPath = resolveExpectedBinPath();
  if (!fs.existsSync(binPath)) {
    throw new Error(`lokus_ingestar binary missing after cargo build: ${binPath}`);
  }
  cachedBinPath = binPath;
  return cachedBinPath;
}

export function runIngestar(args, opts = {}) {
  const { useCargoRun = false, cwd = REPO_ROOT, timeoutMs = 120_000 } = opts;

  let cmd;
  let fullArgs;
  if (useCargoRun) {
    cmd = "cargo";
    fullArgs = [
      "run",
      "--quiet",
      "--manifest-path",
      CARGO_TOML_PATH,
      "--bin",
      "lokus_ingestar",
      "--",
      ...args,
    ];
  } else {
    cmd = ensureIngestarBuilt();
    fullArgs = args;
  }

  const res = spawnSync(cmd, fullArgs, {
    cwd,
    encoding: "utf8",
    timeout: timeoutMs,
  });

  if (res.error) {
    throw new Error(`Failed to execute ${cmd}: ${res.error.message}`);
  }

  const stdout = res.stdout || "";
  const stderr = res.stderr || "";
  let json = null;
  let jsonError = null;
  const expectsJson =
    args.includes("--formato-salida") &&
    args[args.indexOf("--formato-salida") + 1] === "json";

  if (expectsJson && res.status === 0) {
    try {
      json = JSON.parse(stdout);
    } catch (err) {
      jsonError = err;
    }
  }

  return {
    status: res.status ?? -1,
    stdout,
    stderr,
    json,
    jsonError,
  };
}

export function createIsolatedWorkspace(t, opts = {}) {
  const baseRoot = process.env.LOKUS_E2E_TMPDIR || os.tmpdir();
  const tmpDir = fs.mkdtempSync(path.join(baseRoot, "lokus-e2e-"));
  const basePath = path.join(tmpDir, "aprendizaje_base.json");
  const catalogosBasePath = path.join(tmpDir, "juzgados_base.json");
  const ledgerPath = path.join(tmpDir, "ingesta_ledger.json");
  const reportePath = path.join(tmpDir, "ingesta_reporte.md");

  if (!opts.skipBaseCopy) {
    fs.copyFileSync(REAL_BASE_PATH, basePath);
  }
  if (!opts.skipCatalogosCopy) {
    fs.copyFileSync(REAL_CATALOGOS_PATH, catalogosBasePath);
  }

  if (t && typeof t.after === "function") {
    t.after(() => {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    });
  }

  return {
    tmpDir,
    basePath,
    catalogosBasePath,
    ledgerPath,
    reportePath,
    cleanup: () => fs.rmSync(tmpDir, { recursive: true, force: true }),
  };
}

export function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

export function sha256File(filePath) {
  if (!fs.existsSync(filePath)) return null;
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

export function extractBaseInvariantRules(baseObj) {
  const rules = Array.isArray(baseObj?.reglas) ? baseObj.reglas : [];
  return rules.filter(
    (r) => r.origen === "base_invariante" || (typeof r.id === "string" && r.id.startsWith("base-"))
  );
}

export function assertBaseInvariantPreserved(originalBase, updatedBase) {
  const origInvariants = extractBaseInvariantRules(originalBase);
  const newInvariants = extractBaseInvariantRules(updatedBase);
  assert.ok(origInvariants.length >= 3, "Original base must contain at least 3 base_invariante rules");
  assert.equal(
    newInvariants.length,
    origInvariants.length,
    "base_invariante rule count must not change"
  );

  const byId = new Map(newInvariants.map((r) => [r.id, r]));
  for (const orig of origInvariants) {
    const actual = byId.get(orig.id);
    assert.ok(actual, `Missing base_invariante rule ${orig.id}`);
    assert.equal(actual.estado, "activa", `${orig.id} must stay activa`);
    assert.equal(actual.origen, orig.origen, `${orig.id} origen altered`);
    assert.equal(actual.regla, orig.regla, `${orig.id} regla text altered`);
    assert.deepEqual(actual.triggers, orig.triggers, `${orig.id} triggers altered`);
    assert.equal(actual.alpha, orig.alpha, `${orig.id} alpha altered`);
    assert.equal(actual.beta, orig.beta, `${orig.id} beta altered`);
    assert.equal(actual.campo_objetivo, orig.campo_objetivo, `${orig.id} campo_objetivo altered`);
  }
}

export function assertAllNumbersFinite(value, trail = "root") {
  if (typeof value === "number") {
    assert.ok(Number.isFinite(value), `Non-finite number at ${trail}: ${value}`);
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, idx) => assertAllNumbersFinite(item, `${trail}[${idx}]`));
    return;
  }
  if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) {
      assertAllNumbersFinite(v, `${trail}.${k}`);
    }
  }
}

export function normalizarTextoSinTildes(texto = "") {
  return String(texto || "")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase();
}

export async function loadFrontendConstants() {
  const url = pathToFileURL(CONSTANTS_JS_PATH).href;
  return import(url);
}

export function parseModelsRsCatalogs() {
  const src = fs.readFileSync(MODELS_RS_PATH, "utf8");

  const extractStringArray = (constName) => {
    const re = new RegExp(`pub\\s+const\\s+${constName}[^=]*=\\s*\\[([\\s\\S]*?)\\];`);
    const match = src.match(re);
    assert.ok(match, `Could not find ${constName} in models.rs`);
    const items = [];
    const strRe = /"((?:[^"\\]|\\.)*)"/g;
    let m;
    while ((m = strRe.exec(match[1])) !== null) {
      items.push(m[1]);
    }
    return items;
  };

  const extractHorasRapidas = () => {
    const re = /pub\s+const\s+HORAS_RAPIDAS[^=]*=\s*\[([\s\S]*?)\];/;
    const match = src.match(re);
    assert.ok(match, "Could not find HORAS_RAPIDAS in models.rs");
    const pairs = [];
    const pairRe = /\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)/g;
    let m;
    while ((m = pairRe.exec(match[1])) !== null) {
      pairs.push({ h24: m[1], label: m[2] });
    }
    return pairs;
  };

  return {
    MOTIVOS_LABELS: extractStringArray("MOTIVOS_LABELS"),
    TIPOS_AUDIENCIA: extractStringArray("TIPOS_AUDIENCIA"),
    HORAS_RAPIDAS: extractHorasRapidas(),
  };
}

export function getRealExportOrSlicePath() {
  const envPath = process.env.LOKUS_REAL_EXPORT_JSON;
  if (envPath && fs.existsSync(envPath)) {
    return envPath;
  }
  return path.join(SUITE_DIR, "fixtures", "v4_real_slice.json");
}

export function findReportFile(preferredPath, tmpDir) {
  if (fs.existsSync(preferredPath)) return preferredPath;
  const entries = fs.readdirSync(tmpDir);
  const candidate = entries.find((e) => /reporte/i.test(e) && (e.endsWith(".md") || e.endsWith(".json")));
  if (candidate) return path.join(tmpDir, candidate);
  return null;
}

export function extractLedgerSources(ledgerObj) {
  if (!ledgerObj || typeof ledgerObj !== "object") return [];
  const raw = ledgerObj.fuentes || ledgerObj.sources || ledgerObj.snapshots || {};
  if (Array.isArray(raw)) return raw;
  return Object.entries(raw).map(([id, val]) => ({ id, ...(val || {}) }));
}

export function extractTotalAudienciasFromLedger(ledgerObj) {
  if (!ledgerObj || typeof ledgerObj !== "object") return null;
  if (typeof ledgerObj.total_audiencias_acumuladas === "number") {
    return ledgerObj.total_audiencias_acumuladas;
  }
  if (typeof ledgerObj.total_audiencias_contabilizadas === "number") {
    return ledgerObj.total_audiencias_contabilizadas;
  }
  if (typeof ledgerObj.total_audiencias === "number") {
    return ledgerObj.total_audiencias;
  }
  if (ledgerObj.uso_agregado && typeof ledgerObj.uso_agregado.total_audiencias === "number") {
    return ledgerObj.uso_agregado.total_audiencias;
  }
  if (
    ledgerObj.uso_catalogos_agregado &&
    typeof ledgerObj.uso_catalogos_agregado.total_audiencias_contabilizadas === "number"
  ) {
    return ledgerObj.uso_catalogos_agregado.total_audiencias_contabilizadas;
  }
  const sources = extractLedgerSources(ledgerObj);
  if (sources.length > 0) {
    let sum = 0;
    let foundAny = false;
    for (const s of sources) {
      if (s.absorbido_por || s.deduplicado_por || s.omitido) continue;
      const val =
        s.audiencias_netas ??
        s.total_audiencias_netas ??
        s.total_audiencias_contabilizadas ??
        s.total_audiencias ??
        s.audiencias;
      if (typeof val === "number") {
        sum += val;
        foundAny = true;
      }
    }
    if (foundAny) return sum;
  }
  return null;
}

export function scanNoSecretsInDir(dirPath) {
  const secretPatterns = [
    /\bsk-(?!(?:SUPER-SECRETA|test|dummy|fake|ejemplo))[A-Za-z0-9_-]{20,}\b/i,
    /\bAIza(?!(?:Dummy|Test|Fake))[0-9A-Za-z_-]{30,}\b/,
  ];
  const violations = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === "target" || entry.name === "node_modules" || entry.name === ".git") continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (/\.(rs|js|mjs|svelte|json|toml|md)$/i.test(entry.name)) {
        const content = fs.readFileSync(full, "utf8");
        for (const pat of secretPatterns) {
          if (pat.test(content)) {
            violations.push(full);
          }
        }
      }
    }
  };
  walk(dirPath);
  return violations;
}
