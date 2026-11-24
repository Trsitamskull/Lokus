import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { before, describe, it } from "node:test";
import {
  crearCorreccion,
  crearPaqueteV4,
  crearPaqueteV5,
  crearRegla,
  fixturePath,
  loadFixture,
  writeTempFixture,
} from "./fixtures.mjs";
import {
  assertAllNumbersFinite,
  assertBaseInvariantPreserved,
  DOCS_PLAN_PATH,
  DOCS_REPORTE_REAL_PATH,
  ensureIngestarBuilt,
  extractLedgerSources,
  extractTotalAudienciasFromLedger,
  findReportFile,
  getRealExportOrSlicePath,
  loadFrontendConstants,
  normalizarTextoSinTildes,
  parseModelsRsCatalogs,
  readJson,
  REAL_BASE_PATH,
  REAL_CATALOGOS_PATH,
  REAL_LEDGER_PATH,
  REPO_ROOT,
  runIngestar,
  scanNoSecretsInDir,
  sha256File,
  createIsolatedWorkspace,
} from "./helpers.mjs";

before(() => {
  ensureIngestarBuilt();
});

describe("Tier 1 — F1: Proposal & Architecture Doc (docs/PLAN_INGESTA_AUTOAPRENDIZAJE.md)", () => {
  it("T1-F1-01: exists and is non-empty (> 1000 chars)", () => {
    assert.ok(fs.existsSync(DOCS_PLAN_PATH), `Missing ${DOCS_PLAN_PATH}`);
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.ok(content.trim().length > 1000, "PLAN_INGESTA_AUTOAPRENDIZAJE.md is too short");
  });

  it("T1-F1-02: documents ingestion architecture (PaqueteExportacion, aprendizaje_base.json, catalogs)", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /PaqueteExportacion/i);
    assert.match(content, /aprendizaje_base\.json/i);
    assert.match(content, /uso_catalogos|juzgados_base\.json/i);
  });

  it("T1-F1-03: explains how to distinguish Month 1 vs Month 3 cumulative exports of the same user", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /mes\s*1/i);
    assert.match(content, /mes\s*3/i);
    assert.match(content, /instancia_id|snapshot|sembrado_en/i);
  });

  it("T1-F1-04: explains multi-user ingestion and .lokus fuentes_importadas deduplication", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /m[uú]ltiples?\s+usuarios?|multi[- ]?usuario/i);
    assert.match(content, /fuentes_importadas|\.lokus/i);
  });

  it("T1-F1-05: defines retention, compaction, and pruning policy without losing signal", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /compactaci[oó]n|compactar/i);
    assert.match(content, /poda|podar|retenci[oó]n/i);
    assert.match(content, /cuarentena|descarte/i);
  });
});

describe("Tier 1 — F2: Export Format Evolution & formato: 4 Compatibility", () => {
  it("T1-F2-01: lokus_ingestar accepts formato: 4 package without v5 fields", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "json",
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(res.status, 0, `Expected exit 0, got ${res.status}: ${res.stderr}`);
    assert.ok(res.json, `Expected valid stdout JSON, error: ${res.jsonError}`);
    assert.ok(fs.existsSync(ws.ledgerPath), "Ledger must be created");
  });

  it("T1-F2-02: lokus_ingestar accepts formato: 5 package with all new additive fields", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "json",
      fixturePath("v5_usuario_a_seq1.json"),
    ]);
    assert.equal(res.status, 0, `Expected exit 0, got ${res.status}: ${res.stderr}`);
    assert.ok(res.json, "Expected valid JSON stdout");
    const updatedBase = readJson(ws.basePath);
    const baseText = JSON.stringify(updatedBase);
    assert.match(baseText, /regla-u-a-shared|regla-multi-confirmada/);
  });

  it("T1-F2-03: --formato-salida json emits valid structured JSON with finite numbers", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "json",
      fixturePath("v5_usuario_c_independiente.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    assert.equal(res.jsonError, null, `Invalid JSON output: ${res.stdout}`);
    assert.ok(res.json && typeof res.json === "object" && !Array.isArray(res.json));
    assertAllNumbersFinite(res.json, "stdout.json");
  });

  it("T1-F2-04: --dry-run computes report & stdout without mutating --base or --ledger", (t) => {
    const ws = createIsolatedWorkspace(t);
    const baseHashBefore = sha256File(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      "--dry-run",
      "--formato-salida",
      "json",
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    assert.ok(res.json, "Dry run must still return JSON summary");
    assert.equal(sha256File(ws.basePath), baseHashBefore, "--base was mutated during --dry-run");
    assert.equal(fs.existsSync(ws.ledgerPath), false, "--ledger must not be created during --dry-run");
  });

  it("T1-F2-05: --fuente assigns explicit source identifier to formato: 4 package in --ledger", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--fuente",
      "alias-manual-cesar",
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const ledgerRaw = fs.readFileSync(ws.ledgerPath, "utf8");
    assert.match(ledgerRaw, /alias-manual-cesar/, "Ledger should record explicit --fuente alias");
  });
});

describe("Tier 1 — F3: Anonymizer Quarantine & Noise Filtering", () => {
  it("T1-F3-01: quarantines entries containing [PROCESADO] placeholder (San [PROCESADO], armas/pena/campo)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    const nonInvariantRules = (updatedBase.reglas || []).filter(
      (r) => r.origen !== "base_invariante" && !String(r.id || "").startsWith("base-")
    );
    for (const r of nonInvariantRules) {
      assert.ok(
        !String(r.regla || "").includes("[PROCESADO]"),
        `Active/ingested rule ${r.id} leaked [PROCESADO]: ${r.regla}`
      );
      assert.ok(
        !(r.triggers || []).some((tr) => String(tr).includes("[PROCESADO]")),
        `Rule ${r.id} leaked [PROCESADO] in triggers`
      );
    }
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Expected curation report file");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /cuarentena|descartad|\[PROCESADO\]/i);
  });

  it("T1-F3-02: quarantines [RADICADO], [TELEFONO], [NUMERO] placeholders in aliases and rules", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-quarantine-placeholders",
      alias_validados: [
        {
          campo: "juzgado",
          entrada_norm: "juzgado [RADICADO]",
          canonico: "Juzgado [NUMERO] Penal de [TELEFONO]",
          defensor_alcance: "*",
          confianza_q10: 0.9,
        },
        {
          campo: "juzgado",
          entrada_norm: "j9pm",
          canonico: "Juzgado 9 Penal Municipal",
          defensor_alcance: "*",
          confianza_q10: 0.85,
        },
      ],
      tabla_alias_raw: [
        {
          campo: "juzgado",
          entrada_norm: "juzgado [RADICADO]",
          canonico: "Juzgado [NUMERO] Penal de [TELEFONO]",
          alpha: 8.0,
          beta: 1.0,
          defensor_alcance: "*",
          defensores_confirmantes: ["david felipe ariza perez"],
          ultima_actualizacion: "2026-11-01T10:00:00-05:00",
        },
      ],
      reglas: [
        crearRegla({
          id: "regla-leak-radicado",
          regla: "Usar el radicado [RADICADO] y teléfono [TELEFONO] en observaciones.",
          triggers: ["[RADICADO]", "[TELEFONO]", "[NUMERO]"],
        }),
      ],
    });
    const pkgFile = writeTempFixture(ws.tmpDir, "pkg_placeholders.json", pkg);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      pkgFile,
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    const leaked = (updatedBase.reglas || []).find((r) => r.id === "regla-leak-radicado");
    assert.ok(!leaked || leaked.estado === "cuarentena", "Placeholder rule must be quarantined");
    for (const a of updatedBase.tabla_alias || []) {
      assert.ok(!String(a.canonico || "").includes("[NUMERO]"));
      assert.ok(!String(a.canonico || "").includes("[TELEFONO]"));
      assert.ok(!String(a.entrada_norm || "").includes("[RADICADO]"));
    }
  });

  it("T1-F3-03: strips noisy numeric/year triggers (23, 2026, 2) when incorporating rules", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    const ingestedRules = (updatedBase.reglas || []).filter(
      (r) => r.origen !== "base_invariante" && !String(r.id || "").startsWith("base-")
    );
    for (const r of ingestedRules) {
      const triggers = r.triggers || [];
      assert.ok(!triggers.includes("23"), `Noisy trigger '23' survived in ${r.id}`);
      assert.ok(!triggers.includes("2026"), `Noisy trigger '2026' survived in ${r.id}`);
      assert.ok(!triggers.includes("2"), `Noisy trigger '2' survived in ${r.id}`);
    }
  });

  it("T1-F3-04: quarantines low-frequency noise and typos (fdsafeasdfe, Juzgado 10 Penal Maunicipal)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const catalogAfter = readJson(ws.catalogosBasePath);
    assert.ok(!catalogAfter.includes("fdsafeasdfe"), "Noise entry leaked into catalogos-base");
    assert.ok(
      !catalogAfter.includes("Juzgado 10 Penal Maunicipal"),
      "Typo entry leaked into catalogos-base"
    );
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Expected report file");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /fdsafeasdfe|Maunicipal/i);
  });

  it("T1-F3-05: ignores/quarantines retired rules and degenerate partial aliases (1 municipal)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    const retired = (updatedBase.reglas || []).find((r) => r.id === "regla-20261119115511987-30");
    assert.ok(!retired || retired.estado !== "activa", "Retired rule must never be promoted active");
    const badAlias = (updatedBase.tabla_alias || []).find((a) => a.entrada_norm === "1 municipal");
    assert.ok(!badAlias, "Degenerate alias '1 municipal' must not be promoted into tabla_alias");
  });
});

describe("Tier 1 — F4: Maintainer Ingestion Engine & CLI", () => {
  it("T1-F4-01: CLI --help via cargo run documents all 7 required flags", () => {
    const res = runIngestar(["--help"], { useCargoRun: true });
    assert.equal(res.status, 0, `--help failed: ${res.stderr}`);
    const helpText = `${res.stdout}\n${res.stderr}`;
    for (const flag of [
      "--base",
      "--ledger",
      "--catalogos-base",
      "--reporte",
      "--fuente",
      "--dry-run",
      "--formato-salida",
    ]) {
      assert.ok(helpText.includes(flag), `CLI --help missing required flag ${flag}`);
    }
  });

  it("T1-F4-02: preserves all 3 base_invariante rules intact after ingesting multiple packages", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_real_slice.json"),
      fixturePath("v5_usuario_a_seq1.json"),
      fixturePath("v5_usuario_c_independiente.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    assertBaseInvariantPreserved(originalBase, updatedBase);
  });

  it("T1-F4-03: generates --reporte with uso_catalogos novelties (juzgados, tipos, non-quick hours)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Report file was not generated");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /Juzgado 9 Penal Municipal/);
    assert.match(reportText, /Audiencia de formulacion de acusacion/i);
    assert.match(reportText, /07:30/);
  });

  it("T1-F4-04: reports unused default options (predeterminados_sin_uso, horas_rapidas_sin_uso)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Report file was not generated");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /16:00/);
    assert.match(reportText, /sin uso|nunca usados|0 usos/i);
  });

  it("T1-F4-05: compacts and incorporates high-confidence active rules and corrections into --base", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq2.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    assert.ok(
      updatedBase.reglas.length > originalBase.reglas.length,
      "Expected high-confidence rules to be incorporated into --base"
    );
    assert.ok(updatedBase.reglas.length <= 120, "Compacted rules must remain bounded");
    assertAllNumbersFinite(updatedBase, "aprendizaje_base");
  });
});

describe("Tier 1 — F5: Multi-Date & Multi-User Idempotent Ledger", () => {
  it("T1-F5-01: re-ingesting the exact same JSON is idempotent (no duplicate IDs or inflated counts)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const args = [
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
    ];
    const r1 = runIngestar(args);
    assert.equal(r1.status, 0, r1.stderr);
    const baseAfter1 = readJson(ws.basePath);
    const totalAud1 = extractTotalAudienciasFromLedger(readJson(ws.ledgerPath));

    const r2 = runIngestar(args);
    assert.equal(r2.status, 0, r2.stderr);
    const baseAfter2 = readJson(ws.basePath);
    const totalAud2 = extractTotalAudienciasFromLedger(readJson(ws.ledgerPath));

    assert.equal(baseAfter2.reglas.length, baseAfter1.reglas.length, "Duplicate rules added");
    assert.equal(
      baseAfter2.correcciones.length,
      baseAfter1.correcciones.length,
      "Duplicate corrections added"
    );
    assert.equal(totalAud2, totalAud1, "Ledger audiencias inflated on identical re-run");
  });

  it("T1-F5-02: Month 1 -> Month 3 cumulative formato: 4 replaces snapshot without double counting (55, not 75)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const r1 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(r1.status, 0, r1.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 20);

    const r2 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes3_usuario_a_superset.json"),
    ]);
    assert.equal(r2.status, 0, r2.stderr);
    const totalAfterM3 = extractTotalAudienciasFromLedger(readJson(ws.ledgerPath));
    assert.equal(
      totalAfterM3,
      55,
      `Expected Month 3 snapshot replacement total=55 (not 20+55=75), got ${totalAfterM3}`
    );
  });

  it("T1-F5-03: ID union preserves Month 1 items evicted in Month 3 superset", (t) => {
    const ws = createIsolatedWorkspace(t);
    const r1 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(r1.status, 0, r1.stderr);
    const r2 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes3_usuario_a_superset.json"),
    ]);
    assert.equal(r2.status, 0, r2.stderr);

    const ledgerText = fs.readFileSync(ws.ledgerPath, "utf8");
    const baseText = fs.readFileSync(ws.basePath, "utf8");
    assert.ok(
      ledgerText.includes("regla-u-a-m1-evicted") || baseText.includes("regla-u-a-m1-evicted"),
      "Evicted Month 1 rule ID must be preserved in union"
    );
    assert.ok(
      ledgerText.includes("corr-u-a-m1-evicted") || baseText.includes("corr-u-a-m1-evicted"),
      "Evicted Month 1 correction ID must be preserved in union"
    );
  });

  it("T1-F5-04: sums independent packages from User A (30) + User C (25) = 55 without loss", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
      fixturePath("v5_usuario_c_independiente.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const ledger = readJson(ws.ledgerPath);
    const sources = extractLedgerSources(ledger);
    assert.ok(sources.length >= 2, `Expected >=2 independent sources in ledger, got ${sources.length}`);
    assert.equal(extractTotalAudienciasFromLedger(ledger), 55);
  });

  it("T1-F5-05: deduplicates .lokus cross-user imports when fuentes_importadas contains User A", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
      fixturePath("v5_usuario_b_con_paquete_a.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const ledger = readJson(ws.ledgerPath);
    const total = extractTotalAudienciasFromLedger(ledger);
    assert.ok(
      total < 60,
      `Expected deduplicated total < 60 (avoiding full double count of A inside B), got ${total}`
    );
    const baseText = fs.readFileSync(ws.basePath, "utf8");
    const matches = baseText.match(/"regla-u-a-shared"/g) || [];
    assert.equal(matches.length, 1, "Shared rule imported via .lokus must not duplicate");
  });
});

describe("Tier 1 — F6: Native App Domain & Catalog Sync", () => {
  it("T1-F6-01: juzgados_base.json and src/lib/constants.js (JUZGADOS_BASE) have exact parity", async () => {
    const juzgadosJson = readJson(REAL_CATALOGOS_PATH);
    const jsConstants = await loadFrontendConstants();
    assert.deepEqual(
      juzgadosJson,
      jsConstants.JUZGADOS_BASE,
      "juzgados_base.json and JUZGADOS_BASE in src/lib/constants.js must match exactly"
    );
  });

  it("T1-F6-02: models.rs and src/lib/constants.js have exact parity in TIPOS_AUDIENCIA, MOTIVOS_LABELS, HORAS_RAPIDAS", async () => {
    const rust = parseModelsRsCatalogs();
    const js = await loadFrontendConstants();
    assert.deepEqual(rust.MOTIVOS_LABELS, js.MOTIVOS_LABELS, "MOTIVOS_LABELS mismatch");
    assert.deepEqual(rust.TIPOS_AUDIENCIA, js.TIPOS_AUDIENCIA, "TIPOS_AUDIENCIA mismatch");
    const jsHoras = js.HORAS_RAPIDAS.map((h) => ({ h24: `${h.h24}:${h.m}`, label: h.label }));
    assert.deepEqual(rust.HORAS_RAPIDAS, jsHoras, "HORAS_RAPIDAS mismatch");
  });

  it("T1-F6-03: native courts incorporate recurring valid Cesar courts without typos or placeholders", async () => {
    const juzgadosJson = readJson(REAL_CATALOGOS_PATH);
    const js = await loadFrontendConstants();
    for (const expectedCourt of [
      "Juzgado 9 Penal Municipal",
      "Juzgado 11 Penal Municipal",
    ]) {
      assert.ok(
        juzgadosJson.includes(expectedCourt),
        `Missing recurring Cesar court in juzgados_base.json: ${expectedCourt}`
      );
      assert.ok(
        js.JUZGADOS_BASE.includes(expectedCourt),
        `Missing recurring Cesar court in JUZGADOS_BASE: ${expectedCourt}`
      );
    }
    const serialized = JSON.stringify(juzgadosJson);
    assert.ok(!serialized.includes("Maunicipal"), "Typo 'Maunicipal' found in juzgados_base.json");
    assert.ok(!serialized.includes("fdsafeasdfe"), "Noise 'fdsafeasdfe' found in juzgados_base.json");
    assert.ok(!serialized.includes("[PROCESADO]"), "Placeholder '[PROCESADO]' found in juzgados_base.json");
  });

  it("T1-F6-04: TIPOS_AUDIENCIA includes frequent real-JSON hearing types without normalized duplicates", async () => {
    const rust = parseModelsRsCatalogs();
    const normalized = rust.TIPOS_AUDIENCIA.map((t) => normalizarTextoSinTildes(t));
    assert.equal(new Set(normalized).size, normalized.length, "Duplicate normalized hearing types");
    assert.ok(
      normalized.some((t) => t.includes("formulacion de acusacion") || t.includes("acusacion")),
      "Expected acusacion hearing type"
    );
    assert.ok(
      normalized.some((t) => t.includes("principio de oportunidad")),
      "Expected 'Aplicación principio de oportunidad' from real JSON evidence"
    );
  });

  it("T1-F6-05: repository source and config catalogs contain zero leaked API keys or secrets", () => {
    const srcViolations = scanNoSecretsInDir(path.join(REPO_ROOT, "src"));
    const rustViolations = scanNoSecretsInDir(path.join(REPO_ROOT, "src-tauri"));
    assert.deepEqual([...srcViolations, ...rustViolations], [], "Leaked API key pattern detected");
  });
});

describe("Tier 1 — F7: Real JSON Execution & Native Integration", () => {
  it("T1-F7-01: repo aprendizaje_base.json is valid JSON and preserves 3 base_invariante rules", () => {
    const baseObj = readJson(REAL_BASE_PATH);
    assert.ok(typeof baseObj.version === "number" && baseObj.version >= 2);
    const invariants = (baseObj.reglas || []).filter(
      (r) => r.origen === "base_invariante" || String(r.id || "").startsWith("base-")
    );
    assert.equal(invariants.length, 3, "Repo aprendizaje_base.json must keep all 3 base_invariante rules");
  });

  it("T1-F7-02: repo aprendizaje_base.json has zero [PROCESADO] leaks in newly ingested rules/corrections and no junk", () => {
    const baseObj = readJson(REAL_BASE_PATH);
    const nonBaseRules = (baseObj.reglas || []).filter(
      (r) => r.origen !== "base_invariante" && !String(r.id || "").startsWith("base-")
    );
    for (const r of nonBaseRules) {
      assert.ok(!String(r.regla || "").includes("[PROCESADO]"), `Rule ${r.id} has [PROCESADO]`);
    }
    const raw = JSON.stringify(baseObj);
    assert.ok(!raw.includes("fdsafeasdfe"), "Noise 'fdsafeasdfe' leaked into repo aprendizaje_base.json");
    assert.ok(!raw.includes("Maunicipal"), "Typo 'Maunicipal' leaked into repo aprendizaje_base.json");
  });

  it("T1-F7-03: repo ingesta_ledger.json exists post-M4 and records the real JSON run cleanly", () => {
    assert.ok(fs.existsSync(REAL_LEDGER_PATH), `Missing ${REAL_LEDGER_PATH}`);
    const ledger = readJson(REAL_LEDGER_PATH);
    assertAllNumbersFinite(ledger, "ingesta_ledger.json");
    const sources = extractLedgerSources(ledger);
    assert.ok(sources.length >= 1, "ingesta_ledger.json must have at least 1 recorded source");
    const total = extractTotalAudienciasFromLedger(ledger);
    assert.ok(typeof total === "number" && total >= 141, `Expected >=141 audiencias, got ${total}`);
  });

  it("T1-F7-04: dry-run re-ingestion of real JSON against cloned repo base+ledger is idempotent (0 new additions)", (t) => {
    assert.ok(fs.existsSync(REAL_LEDGER_PATH), `Missing ${REAL_LEDGER_PATH}`);
    const ws = createIsolatedWorkspace(t);
    fs.copyFileSync(REAL_LEDGER_PATH, ws.ledgerPath);
    const realInputPath = getRealExportOrSlicePath();
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--dry-run",
      "--formato-salida",
      "json",
      realInputPath,
    ]);
    assert.equal(res.status, 0, res.stderr);
    assert.ok(res.json, "Expected valid stdout JSON");
    const baseBefore = readJson(REAL_BASE_PATH);
    const totalBefore = extractTotalAudienciasFromLedger(readJson(REAL_LEDGER_PATH));
    assert.ok(totalBefore >= 141);
    assert.ok(baseBefore.reglas.length >= 3);
    const jsonStr = JSON.stringify(res.json);
    assert.match(jsonStr, /0|idempotente|omitid|duplicad|sin_cambios/i);
  });

  it("T1-F7-05: curation report in docs/ exists and documents real JSON findings", () => {
    assert.ok(fs.existsSync(DOCS_REPORTE_REAL_PATH), `Missing ${DOCS_REPORTE_REAL_PATH}`);
    const reportText = fs.readFileSync(DOCS_REPORTE_REAL_PATH, "utf8");
    assert.match(reportText, /141/);
    assert.match(reportText, /cuarentena|\[PROCESADO\]/i);
    assert.match(reportText, /Juzgado 9 Penal Municipal/);
  });
});
