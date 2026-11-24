import assert from "node:assert/strict";
import fs from "node:fs";
import { before, describe, it } from "node:test";
import {
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
  createIsolatedWorkspace,
  DOCS_PLAN_PATH,
  DOCS_REPORTE_REAL_PATH,
  ensureIngestarBuilt,
  extractLedgerSources,
  extractTotalAudienciasFromLedger,
  findReportFile,
  getRealExportOrSlicePath,
  loadFrontendConstants,
  parseModelsRsCatalogs,
  readJson,
  REAL_BASE_PATH,
  REAL_CATALOGOS_PATH,
  REAL_LEDGER_PATH,
  runIngestar,
  sha256File,
} from "./helpers.mjs";

before(() => {
  ensureIngestarBuilt();
});

describe("Tier 4 — Realistic End-to-End Application Scenarios", () => {
  it("T4-01: full maintainer ingestion lifecycle on real export (or v4_real_slice fallback): dry-run -> real -> quarantine audit -> idempotent re-run", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const realInputPath = getRealExportOrSlicePath();

    // Step 1: Dry-run review
    const dryRes = runIngestar([
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
    assert.equal(dryRes.status, 0, dryRes.stderr);
    assert.ok(dryRes.json, "Dry run must return valid JSON");
    assert.equal(fs.existsSync(ws.ledgerPath), false, "Dry run must not create ledger");

    // Step 2: Real ingestion with report
    const realRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      "--formato-salida",
      "json",
      realInputPath,
    ]);
    assert.equal(realRes.status, 0, realRes.stderr);
    assert.ok(realRes.json, "Real run must return valid JSON");

    const updatedBase = readJson(ws.basePath);
    assertBaseInvariantPreserved(originalBase, updatedBase);
    assertAllNumbersFinite(updatedBase, "T4-01.base");

    // Quarantine verification on base
    const baseText = JSON.stringify(
      (updatedBase.reglas || []).filter((r) => r.origen !== "base_invariante")
    );
    assert.ok(!baseText.includes("[PROCESADO]"), "No [PROCESADO] leak in active/ingested rules");
    assert.ok(!JSON.stringify(updatedBase).includes("fdsafeasdfe"), "No junk in base");

    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Curation report must be generated");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /141/);
    assert.match(reportText, /cuarentena|\[PROCESADO\]|fdsafeasdfe|Maunicipal/i);

    // Step 3: Idempotent re-run
    const baseHashAfterFirst = sha256File(ws.basePath);
    const totalAfterFirst = extractTotalAudienciasFromLedger(readJson(ws.ledgerPath));
    assert.equal(totalAfterFirst, 141);

    const rerunRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "json",
      realInputPath,
    ]);
    assert.equal(rerunRes.status, 0, rerunRes.stderr);
    assert.equal(sha256File(ws.basePath), baseHashAfterFirst, "Idempotent re-run must not mutate --base");
    assert.equal(
      extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)),
      141,
      "Idempotent re-run must keep 141 audiencias"
    );
  });

  it("T4-02: 3-month single-defender longitudinal lifecycle (M1 v4 -> M2 v4 eviction -> M3 v5 migration + stale M1 rejection)", (t) => {
    const ws = createIsolatedWorkspace(t);

    // Month 1 (v4, 20 audiencias, includes evicted item)
    const m1Res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--fuente",
      "inst-user-a-001",
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(m1Res.status, 0, m1Res.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 20);

    // Month 2 (v4 superset, 55 audiencias, evicts M1 item)
    const m2Res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--fuente",
      "inst-user-a-001",
      fixturePath("v4_mes3_usuario_a_superset.json"),
    ]);
    assert.equal(m2Res.status, 0, m2Res.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 55);

    // Month 3 (v5 migration seq=2, 65 audiencias)
    const m3Res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq2.json"),
    ]);
    assert.equal(m3Res.status, 0, m3Res.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 65);

    // Historic M1 evicted signal still tracked
    const ledgerText = fs.readFileSync(ws.ledgerPath, "utf8");
    const baseText = fs.readFileSync(ws.basePath, "utf8");
    assert.ok(
      ledgerText.includes("corr-u-a-m1-evicted") || baseText.includes("corr-u-a-m1-evicted"),
      "Evicted Month 1 correction ID must remain tracked across longitudinal snapshots"
    );

    // Accidental stale Month 1 re-submission must NOT regress count from 65 to 20
    const staleRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--fuente",
      "inst-user-a-001",
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.equal(staleRes.status, 0, staleRes.stderr);
    assert.equal(
      extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)),
      65,
      "Stale M1 submission must not regress M3 count of 65"
    );
  });

  it("T4-03: multi-user consolidation with .lokus package exchange (A + B(imported A) + C)", (t) => {
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
      fixturePath("v5_usuario_a_seq1.json"),
      fixturePath("v5_usuario_b_con_paquete_a.json"),
      fixturePath("v5_usuario_c_independiente.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);

    const ledger = readJson(ws.ledgerPath);
    const total = extractTotalAudienciasFromLedger(ledger);
    // A(30) + B(30, overlaps A) + C(25) -> strict upper bound < 85 (deduped A inside B) and >= 55
    assert.ok(total >= 55 && total < 85, `Expected deduped multi-user total in [55, 85), got ${total}`);

    const updatedBase = readJson(ws.basePath);
    const shared = (updatedBase.reglas || []).filter((r) => r.id === "regla-multi-confirmada");
    assert.equal(shared.length, 1, "Cross-user confirmed rule must merge into 1 entry");
    assert.ok(
      (shared[0].defensores_confirmantes || []).length >= 2,
      "Cross-user confirmed rule should reflect >=2 confirming defenders"
    );
  });

  it("T4-04: incremental catalog curation distinguishes recurring valid candidates (>=3) vs singletons/junk vs unused defaults", (t) => {
    const ws = createIsolatedWorkspace(t);
    const p1 = crearPaqueteV5({
      instancia_id: "inst-cur-1",
      juzgados_agregados_usuario: ["Juzgado 14 Penal Municipal", "Juzgado 99 Penal Maunicipal"],
      tipos_audiencia_agregados_usuario: ["Audiencia de control posterior"],
      uso_catalogos_overrides: {
        total_audiencias_contabilizadas: 20,
        juzgados_nuevos: [
          { nombre: "Juzgado 14 Penal Municipal", origen: "agregado_usuario", conteo: 4, porcentaje: 20.0 },
          { nombre: "Juzgado 99 Penal Maunicipal", origen: "agregado_usuario", conteo: 1, porcentaje: 5.0 },
          { nombre: "asdf1234", origen: "otra", conteo: 1, porcentaje: 5.0 },
        ],
        tipos_nuevos: [
          { nombre: "Audiencia de control posterior", origen: "agregado_usuario", conteo: 4, porcentaje: 20.0 },
        ],
      },
    });
    const p2 = crearPaqueteV5({
      instancia_id: "inst-cur-2",
      juzgados_agregados_usuario: ["Juzgado 14 Penal Municipal"],
      tipos_audiencia_agregados_usuario: ["Audiencia de control posterior"],
      uso_catalogos_overrides: {
        total_audiencias_contabilizadas: 15,
        juzgados_nuevos: [
          { nombre: "Juzgado 14 Penal Municipal", origen: "agregado_usuario", conteo: 3, porcentaje: 20.0 },
        ],
        tipos_nuevos: [
          { nombre: "Audiencia de control posterior", origen: "agregado_usuario", conteo: 3, porcentaje: 20.0 },
        ],
      },
    });

    const f1 = writeTempFixture(ws.tmpDir, "cur1.json", p1);
    const f2 = writeTempFixture(ws.tmpDir, "cur2.json", p2);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      f1,
      f2,
    ]);
    assert.equal(res.status, 0, res.stderr);
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    assert.ok(reportPath, "Curation report missing");
    const reportText = fs.readFileSync(reportPath, "utf8");
    assert.match(reportText, /Juzgado 14 Penal Municipal/);
    assert.match(reportText, /Audiencia de control posterior/);
    assert.match(reportText, /16:00/);
    assert.match(reportText, /asdf1234|Maunicipal/i);
  });

  it("T4-05: operational error recovery preserves prior valid state on failed batch and succeeds on clean retry", (t) => {
    const ws = createIsolatedWorkspace(t);

    // Seed valid state with User C
    const seedRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_c_independiente.json"),
    ]);
    assert.equal(seedRes.status, 0, seedRes.stderr);
    const baseHashSeeded = sha256File(ws.basePath);
    const ledgerHashSeeded = sha256File(ws.ledgerPath);

    // Attempt batch containing a malformed file
    const badBatchRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
      fixturePath("invalid_malformed.json"),
    ]);
    assert.notEqual(badBatchRes.status, 0, "Batch with malformed file must fail");
    assert.equal(sha256File(ws.basePath), baseHashSeeded, "Seeded --base corrupted on failed batch");
    assert.equal(sha256File(ws.ledgerPath), ledgerHashSeeded, "Seeded --ledger corrupted on failed batch");

    // Clean retry without malformed file
    const retryRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
    ]);
    assert.equal(retryRes.status, 0, retryRes.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 55);
  });

  it("T4-06: end-to-end post-M4 native integration audit across docs, base, catalogs, ledger, and dry-run", async (t) => {
    assert.ok(fs.existsSync(DOCS_PLAN_PATH), "Missing PLAN_INGESTA_AUTOAPRENDIZAJE.md");
    assert.ok(fs.existsSync(DOCS_REPORTE_REAL_PATH), "Missing INGESTA_REPORTE_REAL.md");
    assert.ok(fs.existsSync(REAL_LEDGER_PATH), "Missing repo ingesta_ledger.json");

    const baseObj = readJson(REAL_BASE_PATH);
    const catalogos = readJson(REAL_CATALOGOS_PATH);
    const ledger = readJson(REAL_LEDGER_PATH);
    const js = await loadFrontendConstants();
    const rust = parseModelsRsCatalogs();

    assert.deepEqual(catalogos, js.JUZGADOS_BASE, "juzgados_base.json vs constants.js out of sync");
    assert.deepEqual(rust.TIPOS_AUDIENCIA, js.TIPOS_AUDIENCIA, "TIPOS_AUDIENCIA out of sync");
    assert.deepEqual(rust.MOTIVOS_LABELS, js.MOTIVOS_LABELS, "MOTIVOS_LABELS out of sync");
    assertAllNumbersFinite(baseObj, "repo.aprendizaje_base");
    assertAllNumbersFinite(ledger, "repo.ingesta_ledger");

    const ws = createIsolatedWorkspace(t);
    fs.copyFileSync(REAL_LEDGER_PATH, ws.ledgerPath);
    const dryRes = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--dry-run",
      "--formato-salida",
      "json",
      getRealExportOrSlicePath(),
    ]);
    assert.equal(dryRes.status, 0, dryRes.stderr);
    assert.ok(dryRes.json, "Dry-run against integrated repo state must emit valid JSON");
    assertBaseInvariantPreserved(baseObj, readJson(ws.basePath));
  });
});
