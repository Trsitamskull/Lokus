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
  assertBaseInvariantPreserved,
  createIsolatedWorkspace,
  DOCS_PLAN_PATH,
  ensureIngestarBuilt,
  extractTotalAudienciasFromLedger,
  findReportFile,
  readJson,
  runIngestar,
} from "./helpers.mjs";

before(() => {
  ensureIngestarBuilt();
});

describe("Tier 3 — Cross-Feature Pairwise Combinations", () => {
  it("T3-01 (F2 x F5): v4 -> v5 format migration for same user avoids double counting", (t) => {
    const ws = createIsolatedWorkspace(t);
    const r1 = runIngestar([
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
    assert.equal(r1.status, 0, r1.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 20);

    const r2 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
    ]);
    assert.equal(r2.status, 0, r2.stderr);
    const total = extractTotalAudienciasFromLedger(readJson(ws.ledgerPath));
    assert.equal(
      total,
      30,
      `Migrating inst-user-a-001 from v4 (20) to v5 (30) must replace snapshot to 30, got ${total}`
    );
  });

  it("T3-02 (F3 x F4): mixed package quarantines noise/contradiction, admits clean rule, preserves base_invariante", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-mixed-f3-f4",
      reglas: [
        crearRegla({
          id: "regla-clean-valid",
          regla: "Mapear J9PM a Juzgado 9 Penal Municipal cuando no haya ambigüedad.",
          triggers: ["j9pm", "municipal", "penal"],
          alpha: 12.0,
          beta: 1.0,
          impresiones: 14,
          aciertos_directos: 14,
        }),
        crearRegla({
          id: "regla-bad-procesado",
          regla: "En San [PROCESADO] registrar siempre como municipal.",
          triggers: ["san", "[PROCESADO]"],
        }),
        crearRegla({
          id: "regla-contradict-jerarquia",
          tipo_error: "motivo",
          campo_objetivo: "motivo",
          regla: "Cuando el defensor solicite reprogramar por causa del fiscal, marcar siempre motivo Defensor Público.",
          triggers: ["solicite", "reprogram", "fiscal", "defensor"],
        }),
      ],
      uso_catalogos_overrides: {
        total_audiencias_contabilizadas: 12,
        juzgados_nuevos: [
          { nombre: "Juzgado 10 Penal Maunicipal", origen: "agregado_usuario", conteo: 1, porcentaje: 8.33 },
          { nombre: "Juzgado 9 Penal Municipal", origen: "agregado_usuario", conteo: 5, porcentaje: 41.67 },
        ],
      },
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "mixed_f3_f4.json", pkg);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--reporte",
      ws.reportePath,
      pkgPath,
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    assertBaseInvariantPreserved(originalBase, updatedBase);
    const ids = new Set((updatedBase.reglas || []).map((r) => r.id));
    assert.ok(ids.has("regla-clean-valid"), "Clean valid rule should be incorporated");
    const badProc = (updatedBase.reglas || []).find((r) => r.id === "regla-bad-procesado");
    assert.ok(!badProc || badProc.estado !== "activa");
    const badContra = (updatedBase.reglas || []).find((r) => r.id === "regla-contradict-jerarquia");
    assert.ok(!badContra || badContra.estado !== "activa");
  });

  it("T3-03 (F4 x F5): multi-file batch in a single CLI call is order-independent and idempotent", (t) => {
    const ws1 = createIsolatedWorkspace(t);
    const ws2 = createIsolatedWorkspace(t);
    const fileA = fixturePath("v5_usuario_a_seq1.json");
    const fileC = fixturePath("v5_usuario_c_independiente.json");

    const r1 = runIngestar([
      "--base",
      ws1.basePath,
      "--ledger",
      ws1.ledgerPath,
      "--catalogos-base",
      ws1.catalogosBasePath,
      fileA,
      fileC,
    ]);
    assert.equal(r1.status, 0, r1.stderr);

    const r2 = runIngestar([
      "--base",
      ws2.basePath,
      "--ledger",
      ws2.ledgerPath,
      "--catalogos-base",
      ws2.catalogosBasePath,
      fileC,
      fileA,
    ]);
    assert.equal(r2.status, 0, r2.stderr);

    assert.equal(
      extractTotalAudienciasFromLedger(readJson(ws1.ledgerPath)),
      extractTotalAudienciasFromLedger(readJson(ws2.ledgerPath))
    );
    const rules1 = readJson(ws1.basePath).reglas.map((r) => r.id).sort();
    const rules2 = readJson(ws2.basePath).reglas.map((r) => r.id).sort();
    assert.deepEqual(rules1, rules2, "Batch ingestion order must yield identical rule set");
  });

  it("T3-04 (F2 x F3): formato: 5 ejemplos_aciertos and tabla_alias_raw quarantine [PROCESADO] leaks while admitting clean entries", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-v5-quarantine",
      tabla_alias_raw: [
        {
          campo: "juzgado",
          entrada_norm: "san procesado",
          canonico: "Juzgado 1 Promiscuo Municipal de San [PROCESADO]",
          alpha: 8.0,
          beta: 1.0,
          defensor_alcance: "*",
          defensores_confirmantes: ["ivan cordoba"],
          ultima_actualizacion: "2026-11-05T10:00:00-05:00",
        },
        {
          campo: "juzgado",
          entrada_norm: "j9pm",
          canonico: "Juzgado 9 Penal Municipal",
          alpha: 8.0,
          beta: 1.0,
          defensor_alcance: "*",
          defensores_confirmantes: ["david felipe ariza perez"],
          ultima_actualizacion: "2026-11-05T10:00:00-05:00",
        },
      ],
      ejemplos_aciertos: [
        {
          id: "ej-contaminated",
          registrada: "2026-11-05T10:00:00-05:00",
          proveedor: "openai",
          modelo: "gpt-5-mini",
          texto_anonimizado: "Audiencia por tráfico de [PROCESADO] de fuego en San [PROCESADO].",
          salida_anonimizada: {
            radicado: "[PROCESADO] - [RADICADO]",
            tipo: "Audiencia concentrada",
            fecha: "05/11/2026",
            hora: "08:00",
            juzgado: "Juzgado 1 Promiscuo Municipal de San [PROCESADO]",
            realizada: "SI",
            motivo: "",
            observaciones: "Porte de [PROCESADO] de fuego.",
          },
          reglas_inyectadas: [],
          defensor_clave: "ivan cordoba",
        },
        {
          id: "ej-clean",
          registrada: "2026-11-05T11:00:00-05:00",
          proveedor: "openai",
          modelo: "gpt-5-mini",
          texto_anonimizado: "Audiencia de juicio oral en Juzgado 9 Penal Municipal contra [PROCESADO].",
          salida_anonimizada: {
            radicado: "[PROCESADO] - [RADICADO]",
            tipo: "Audiencia de juicio oral",
            fecha: "05/11/2026",
            hora: "09:00",
            juzgado: "Juzgado 9 Penal Municipal",
            realizada: "SI",
            motivo: "",
            observaciones: "Se realizó audiencia de manera adecuada.",
          },
          reglas_inyectadas: [],
          defensor_clave: "david felipe ariza perez",
        },
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "v5_ejemplos_quarantine.json", pkg);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      pkgPath,
    ]);
    assert.equal(res.status, 0, res.stderr);
    const updatedBase = readJson(ws.basePath);
    for (const a of updatedBase.tabla_alias || []) {
      assert.ok(!String(a.canonico || "").includes("[PROCESADO]"), "Contaminated alias leaked");
    }
    for (const e of updatedBase.ejemplos_aciertos || []) {
      assert.ok(
        !String(e.salida_anonimizada?.juzgado || "").includes("[PROCESADO]"),
        "Contaminated positive example leaked into base"
      );
    }
  });

  it("T3-05 (F3 x F5): rule transitioning from prueba in M1 to retirada in M3 is not re-promoted active", (t) => {
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

    const updatedBase = readJson(ws.basePath);
    const pruebaRule = (updatedBase.reglas || []).find((r) => r.id === "regla-u-a-prueba");
    assert.ok(!pruebaRule || pruebaRule.estado !== "activa", "Retired rule in M3 must not be active");
  });

  it("T3-06 (F4 x F7): --dry-run JSON predictions match real ingestion deltas on v4_real_slice.json", (t) => {
    const ws = createIsolatedWorkspace(t);
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
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(dryRes.status, 0, dryRes.stderr);
    assert.ok(dryRes.json, "Expected JSON from dry-run");

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
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(realRes.status, 0, realRes.stderr);
    assert.ok(realRes.json, "Expected JSON from real run");

    for (const key of Object.keys(dryRes.json)) {
      if (typeof dryRes.json[key] === "number" && typeof realRes.json[key] === "number") {
        assert.equal(
          realRes.json[key],
          dryRes.json[key],
          `Mismatch between dry-run and real run numeric metric '${key}'`
        );
      }
    }
  });

  it("T3-07 (F6 x F7): ingesting real slice against synced juzgados_base.json filters typos and avoids false missing alerts", (t) => {
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
      "--formato-salida",
      "json",
      fixturePath("v4_real_slice.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    const catalogAfter = readJson(ws.catalogosBasePath);
    assert.ok(catalogAfter.includes("Juzgado 9 Penal Municipal"), "Juzgado 9 Penal Municipal synced");
    assert.ok(!catalogAfter.includes("fdsafeasdfe"), "Noise leaked to catalog");
    assert.ok(!catalogAfter.includes("Juzgado 10 Penal Maunicipal"), "Typo leaked to catalog");
  });

  it("T3-08 (F1 x F4): documented CLI flags in PLAN_INGESTA_AUTOAPRENDIZAJE.md match lokus_ingestar --help", () => {
    const docText = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    const helpRes = runIngestar(["--help"]);
    assert.equal(helpRes.status, 0, helpRes.stderr);
    const helpText = `${helpRes.stdout}\n${helpRes.stderr}`;
    for (const flag of [
      "--base",
      "--ledger",
      "--catalogos-base",
      "--reporte",
      "--fuente",
      "--dry-run",
      "--formato-salida",
    ]) {
      assert.ok(docText.includes(flag), `Doc missing CLI flag ${flag}`);
      assert.ok(helpText.includes(flag), `--help missing CLI flag ${flag}`);
    }
  });
});
