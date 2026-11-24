import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PAQUETE_ADDITIVE_KEYS_V5, PAQUETE_ROOT_KEYS_V4 } from "./fixtures.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const FIXTURES_DIR = path.join(__dirname, "fixtures");
const CARGO_TOML_PATH = path.join(REPO_ROOT, "src-tauri", "Cargo.toml");

const REQUIRED_MJS = [
  "fixtures.mjs",
  "helpers.mjs",
  "check_suite.mjs",
  "tier1_features.test.mjs",
  "tier2_boundaries.test.mjs",
  "tier3_combinations.test.mjs",
  "tier4_scenarios.test.mjs",
];

const EXPECTED_VALID_FIXTURES = [
  "v4_real_slice.json",
  "v4_mes1_usuario_a.json",
  "v4_mes3_usuario_a_superset.json",
  "v5_usuario_a_seq1.json",
  "v5_usuario_a_seq2.json",
  "v5_usuario_b_con_paquete_a.json",
  "v5_usuario_c_independiente.json",
  "empty_minimal_v4.json",
  "invalid_future_v99.json",
];

let errors = 0;
function fail(msg) {
  console.error(`[FAIL] ${msg}`);
  errors += 1;
}
function ok(msg) {
  console.log(`[OK] ${msg}`);
}

// 1. Check JS files exist and pass `node --check`
for (const file of REQUIRED_MJS) {
  const full = path.join(__dirname, file);
  if (!fs.existsSync(full)) {
    fail(`Missing required module: ${file}`);
    continue;
  }
  const chk = spawnSync(process.execPath, ["--check", full], { encoding: "utf8" });
  if (chk.status !== 0) {
    fail(`Syntax check failed for ${file}: ${chk.stderr}`);
  } else {
    ok(`Syntax valid: ${file}`);
  }
}

// 2. Validate JSON fixtures + 29-key schema
for (const fx of EXPECTED_VALID_FIXTURES) {
  const full = path.join(FIXTURES_DIR, fx);
  if (!fs.existsSync(full)) {
    fail(`Missing fixture: fixtures/${fx}`);
    continue;
  }
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(full, "utf8"));
  } catch (err) {
    fail(`Invalid JSON in fixtures/${fx}: ${err.message}`);
    continue;
  }
  const missingV4 = PAQUETE_ROOT_KEYS_V4.filter((k) => !(k in parsed));
  if (missingV4.length > 0) {
    fail(`Fixture ${fx} missing PaqueteExportacion keys: ${missingV4.join(", ")}`);
    continue;
  }
  if (parsed.formato === 5) {
    const missingV5 = PAQUETE_ADDITIVE_KEYS_V5.filter((k) => !(k in parsed));
    if (missingV5.length > 0) {
      fail(`v5 fixture ${fx} missing additive keys: ${missingV5.join(", ")}`);
      continue;
    }
  }
  ok(`Fixture schema valid (formato=${parsed.formato}, keys=${Object.keys(parsed).length}): ${fx}`);
}

// Confirm invalid_malformed.json exists and is intentionally malformed JSON
const malformedPath = path.join(FIXTURES_DIR, "invalid_malformed.json");
if (!fs.existsSync(malformedPath)) {
  fail("Missing fixture: fixtures/invalid_malformed.json");
} else {
  let threw = false;
  try {
    JSON.parse(fs.readFileSync(malformedPath, "utf8"));
  } catch {
    threw = true;
  }
  if (!threw) {
    fail("fixtures/invalid_malformed.json parsed cleanly — expected intentionally invalid JSON");
  } else {
    ok("Negative fixture invalid_malformed.json confirmed malformed JSON");
  }
}

// 3. Count declared tests across Tiers 1..4 and verify zero .skip calls
const tierThresholds = [
  { file: "tier1_features.test.mjs", min: 35, label: "Tier 1" },
  { file: "tier2_boundaries.test.mjs", min: 35, label: "Tier 2" },
  { file: "tier3_combinations.test.mjs", min: 7, label: "Tier 3" },
  { file: "tier4_scenarios.test.mjs", min: 5, label: "Tier 4" },
];

let totalTests = 0;
for (const tier of tierThresholds) {
  const full = path.join(__dirname, tier.file);
  if (!fs.existsSync(full)) continue;
  const content = fs.readFileSync(full, "utf8");
  if (/\.skip\s*\(/m.test(content) || /skip\s*:\s*true/m.test(content)) {
    fail(`Forbidden test skip detected in ${tier.file}`);
  }
  const matches = content.match(/\bit\s*\(/g) || [];
  totalTests += matches.length;
  if (matches.length < tier.min) {
    fail(`${tier.label} (${tier.file}) has ${matches.length} tests; expected >= ${tier.min}`);
  } else {
    ok(`${tier.label} (${tier.file}): ${matches.length} tests (min ${tier.min}), 0 skips`);
  }
}

if (totalTests < 82) {
  fail(`Total test count ${totalTests} is below required >= 82`);
} else {
  ok(`Total E2E tests declared across 4 tiers: ${totalTests}`);
}

// 4. Probe M3 binary availability honestly
const cargoToml = fs.existsSync(CARGO_TOML_PATH) ? fs.readFileSync(CARGO_TOML_PATH, "utf8") : "";
const binDeclared = /name\s*=\s*"lokus_ingestar"/.test(cargoToml);
const binFile = path.join(REPO_ROOT, "src-tauri", "src", "bin", "lokus_ingestar.rs");
const binBuilt = path.join(
  REPO_ROOT,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "lokus_ingestar.exe" : "lokus_ingestar"
);
const m3State =
  binDeclared && fs.existsSync(binFile)
    ? fs.existsSync(binBuilt)
      ? "READY (declared + compiled)"
      : "READY (declared; run `cargo build --bin lokus_ingestar`)"
    : "PENDING_M3 (binary lokus_ingestar not yet declared in src-tauri/Cargo.toml)";

console.log(`\n[PROBE] M3 CLI status: ${m3State}`);

if (errors > 0) {
  console.error(`\ncheck_suite.mjs FAILED with ${errors} error(s).`);
  process.exit(1);
}
console.log("\ncheck_suite.mjs PASSED.");
