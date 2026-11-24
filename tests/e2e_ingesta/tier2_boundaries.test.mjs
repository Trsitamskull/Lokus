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
  writeTempFixture,
} from "./fixtures.mjs";
import {
  assertAllNumbersFinite,
  assertBaseInvariantPreserved,
  createIsolatedWorkspace,
  DOCS_PLAN_PATH,
  ensureIngestarBuilt,
  extractLedgerSources,
  extractTotalAudienciasFromLedger,
  findReportFile,
  loadFrontendConstants,
  normalizarTextoSinTildes,
  parseModelsRsCatalogs,
  readJson,
  REAL_BASE_PATH,
  REAL_CATALOGOS_PATH,
  REAL_LEDGER_PATH,
  REPO_ROOT,
  runIngestar,
  sha256File,
  TEMPLATE_XLSX_PATH,
  TEMPLATES_DIR,
} from "./helpers.mjs";

before(() => {
  ensureIngestarBuilt();
});

describe("Tier 2 — F1: Proposal & Architecture Doc Edge Cases", () => {
  it("T2-F1-01: PLAN_INGESTA_AUTOAPRENDIZAJE.md defines explicit rule retirement/discard criteria", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /retirada/i);
    assert.match(content, /umbral|confianza|descart/i);
  });

  it("T2-F1-02: documents collision & fallback handling between formato: 4 and formato: 5", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /formato:\s*4|formato\s+4/i);
    assert.match(content, /sembrado_en|--fuente|huella/i);
  });

  it("T2-F1-03: documents immutability of base_invariante against contradicting external rules", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /base_invariante/);
    assert.match(content, /inmutab|proteg|nunca/i);
  });

  it("T2-F1-04: documents strategy for false [PROCESADO] redactions in toponyms and legal nouns", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /\[PROCESADO\]/);
    assert.match(content, /cuarentena|top[oó]nimo|San Alberto|falsos?\s+positivos?/i);
  });

  it("T2-F1-05: documents minimum frequency threshold for 'otra' single-use entries and typos", () => {
    const content = fs.readFileSync(DOCS_PLAN_PATH, "utf8");
    assert.match(content, /otra|typo|frecuencia\s+m[ií]nima|umbral/i);
  });
});

describe("Tier 2 — F2: Format Boundaries, Malformed Inputs & Zero-Count Tolerance", () => {
  it("T2-F2-01: malformed JSON fails cleanly without corrupting --base or --ledger", (t) => {
    const ws = createIsolatedWorkspace(t);
    const baseHashBefore = sha256File(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("invalid_malformed.json"),
    ]);
    assert.notEqual(res.status, 0, "Malformed JSON must fail with non-zero exit code");
    assert.ok((res.stderr || res.stdout).trim().length > 0, "Expected readable error message");
    assert.equal(sha256File(ws.basePath), baseHashBefore, "--base corrupted on malformed JSON");
    assert.equal(fs.existsSync(ws.ledgerPath), false, "--ledger must not be created on failure");
  });

  it("T2-F2-02: unsupported future formato: 99 is rejected cleanly", (t) => {
    const ws = createIsolatedWorkspace(t);
    const baseHashBefore = sha256File(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("invalid_future_v99.json"),
    ]);
    assert.notEqual(res.status, 0, "formato: 99 must be rejected");
    assert.match(`${res.stderr}\n${res.stdout}`, /formato|99|soportad/i);
    assert.equal(sha256File(ws.basePath), baseHashBefore);
  });

  it("T2-F2-03: package with wrong app name or missing critical fields is rejected without mutating --base", (t) => {
    const ws = createIsolatedWorkspace(t);
    const baseHashBefore = sha256File(ws.basePath);
    const badPkg = crearPaqueteV4({ app: "OtroSistema" });
    const badPath = writeTempFixture(ws.tmpDir, "wrong_app.json", badPkg);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      badPath,
    ]);
    assert.notEqual(res.status, 0, "Package with app != 'Lokus' must be rejected");
    assert.equal(sha256File(ws.basePath), baseHashBefore);
  });

  it("T2-F2-04: formato: 5 with empty arrays and null preferencias_raw is tolerated via #[serde(default)]", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-empty-v5",
      exportacion_seq: 1,
      fuentes_importadas: [],
      tabla_alias_raw: [],
      preferencias_raw: null,
      preferencias_por_defensor_raw: {},
      ejemplos_aciertos: [],
      reglas: [],
      correcciones: [],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "v5_nullable.json", pkg);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "json",
      pkgPath,
    ]);
    assert.equal(res.status, 0, `Expected exit 0 for nullable v5 fields: ${res.stderr}`);
    assert.ok(res.json, "Expected valid JSON output");
    assertAllNumbersFinite(res.json, "v5_nullable_stdout");
  });

  it("T2-F2-05: minimal empty v4 package (0 rules, 0 corrections, 0 audiencias) exits 0 without NaN", (t) => {
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
      fixturePath("empty_minimal_v4.json"),
    ]);
    assert.equal(res.status, 0, res.stderr);
    assert.ok(res.json, "Expected valid JSON output");
    assertAllNumbersFinite(res.json, "empty_minimal_stdout");
    assertAllNumbersFinite(readJson(ws.basePath), "empty_minimal_base");
    assertAllNumbersFinite(readJson(ws.ledgerPath), "empty_minimal_ledger");
    const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
    if (reportPath) {
      const text = fs.readFileSync(reportPath, "utf8");
      assert.ok(!/\bNaN\b/.test(text), "Report must not contain NaN");
    }
  });
});

describe("Tier 2 — F3: Quarantine, Contradictions & Canonicalization Boundaries", () => {
  it("T2-F3-01: rule containing [PROCESADO] in text or triggers is quarantined and never active in --base", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-rule-procesado",
      reglas: [
        crearRegla({
          id: "regla-procesado-body",
          regla: "Usar Juzgado 1 de San [PROCESADO] siempre.",
          triggers: ["juzgado", "san", "procesado"],
        }),
        crearRegla({
          id: "regla-procesado-trigger",
          regla: "Regla con trigger redactado.",
          triggers: ["[PROCESADO]", "despacho"],
        }),
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "rule_procesado.json", pkg);
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
    for (const badId of ["regla-procesado-body", "regla-procesado-trigger"]) {
      const found = (updatedBase.reglas || []).find((r) => r.id === badId);
      assert.ok(!found || found.estado !== "activa", `${badId} must not be active in --base`);
    }
  });

  it("T2-F3-02: rule with mismatched campo_objetivo vs tipo_error (motivo vs tipo) is normalized or quarantined", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV4({
      reglas: [
        crearRegla({
          id: "regla-mismatch-motivo-tipo",
          tipo_error: "motivo",
          campo_objetivo: "tipo",
          regla: "Clasificar motivo cuando el abogado solicite aplazamiento.",
          triggers: ["abogado", "solicite", "aplazamiento"],
        }),
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "mismatch.json", pkg);
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
    const rule = (updatedBase.reglas || []).find((r) => r.id === "regla-mismatch-motivo-tipo");
    if (rule && rule.estado === "activa") {
      assert.equal(
        rule.campo_objetivo,
        "motivo",
        "Active rule must have campo_objetivo aligned with tipo_error='motivo'"
      );
    } else {
      const reportPath = findReportFile(ws.reportePath, ws.tmpDir);
      assert.ok(reportPath, "Expected report flagging/quarantining mismatched rule");
    }
  });

  it("T2-F3-03: rule with 100% stopword/numeric triggers is degraded/quarantined or stripped of junk tokens", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-stopword-triggers",
      reglas: [
        crearRegla({
          id: "regla-junk-triggers",
          regla: "Regla con triggers exclusivamente numéricos o vacíos.",
          triggers: ["1", "2026", "el", "de", "la"],
        }),
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "junk_triggers.json", pkg);
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
    const found = (updatedBase.reglas || []).find((r) => r.id === "regla-junk-triggers");
    if (found) {
      assert.ok(
        found.estado !== "activa" ||
          !(found.triggers || []).some((tr) => ["1", "2026", "el", "de", "la"].includes(tr)),
        "Rule with 100% stopword/numeric triggers must not stay active with junk triggers"
      );
    }
  });

  it("T2-F3-04: external rule contradicting base-abreviacion-cto is rejected/quarantined", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-contradict-cto",
      reglas: [
        crearRegla({
          id: "regla-contradict-cto",
          tipo_error: "abreviacion",
          campo_objetivo: "juzgado",
          regla: "Siempre expandir la abreviación CTO a Circuito en todos los juzgados.",
          triggers: ["cto", "circuito", "juzgado"],
          alpha: 20.0,
          beta: 1.0,
          impresiones: 20,
          aciertos_directos: 19,
        }),
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "contradict_cto.json", pkg);
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
    assertBaseInvariantPreserved(originalBase, updatedBase);
    const bad = (updatedBase.reglas || []).find((r) => r.id === "regla-contradict-cto");
    assert.ok(!bad || bad.estado !== "activa", "Contradicting rule must not become active");
  });

  it("T2-F3-05: accent variants in catalogs/aliases canonicalize without duplicate entries", (t) => {
    const ws = createIsolatedWorkspace(t);
    const pkg = crearPaqueteV5({
      instancia_id: "inst-accents",
      juzgados_agregados_usuario: [
        "Juzgado 9 Penal Municipal",
        "Juzgado 9 Penal Municípal",
      ],
      tipos_audiencia_agregados_usuario: [
        "Audiencia de formulacion de acusacion",
        "Audiencia de formulación de acusación",
      ],
      alias_validados: [
        {
          campo: "tipo",
          entrada_norm: "form acusacion",
          canonico: "Audiencia de formulación de acusación",
          defensor_alcance: "*",
          confianza_q10: 0.8,
        },
        {
          campo: "tipo",
          entrada_norm: "form acusacion",
          canonico: "Audiencia de formulacion de acusacion",
          defensor_alcance: "*",
          confianza_q10: 0.78,
        },
      ],
      tabla_alias_raw: [
        {
          campo: "tipo",
          entrada_norm: "form acusacion",
          canonico: "Audiencia de formulación de acusación",
          alpha: 7.0,
          beta: 1.0,
          defensor_alcance: "*",
          defensores_confirmantes: ["david felipe ariza perez"],
          ultima_actualizacion: "2026-11-05T10:00:00-05:00",
        },
        {
          campo: "tipo",
          entrada_norm: "form acusacion",
          canonico: "Audiencia de formulacion de acusacion",
          alpha: 6.0,
          beta: 1.0,
          defensor_alcance: "*",
          defensores_confirmantes: ["ivan cordoba"],
          ultima_actualizacion: "2026-11-06T10:00:00-05:00",
        },
      ],
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "accents.json", pkg);
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
    const formAliases = (updatedBase.tabla_alias || []).filter(
      (a) => a.entrada_norm === "form acusacion" && a.defensor_alcance === "*"
    );
    assert.ok(formAliases.length <= 1, "Equivalent accent variants must deduplicate in tabla_alias");
  });
});

describe("Tier 2 — F4: CLI Argument Errors, Atomicity & Capacity Compaction", () => {
  it("T2-F4-01: invocation without input JSON paths exits non-zero with usage message", () => {
    const res = runIngestar([]);
    assert.notEqual(res.status, 0, "Missing input files must exit non-zero");
    assert.ok(`${res.stderr}\n${res.stdout}`.trim().length > 0, "Expected usage error message");
  });

  it("T2-F4-02: non-existent --base or --catalogos-base fails cleanly without panic", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      path.join(ws.tmpDir, "does_not_exist_base.json"),
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.notEqual(res.status, 0, "Missing --base path must exit non-zero");
    assert.ok(!/panicked at/i.test(res.stderr), "Must return controlled error, not Rust panic");
  });

  it("T2-F4-03: invalid --formato-salida value exits non-zero", (t) => {
    const ws = createIsolatedWorkspace(t);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      "--formato-salida",
      "xml_invalido",
      fixturePath("v4_mes1_usuario_a.json"),
    ]);
    assert.notEqual(res.status, 0, "Unsupported --formato-salida must fail");
  });

  it("T2-F4-04: batch where 2nd input path does not exist preserves original --base and --ledger", (t) => {
    const ws = createIsolatedWorkspace(t);
    const baseHashBefore = sha256File(ws.basePath);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
      path.join(ws.tmpDir, "missing_second_file.json"),
    ]);
    assert.notEqual(res.status, 0, "Missing second batch file must fail atomically");
    assert.equal(sha256File(ws.basePath), baseHashBefore, "--base was partially mutated on batch failure");
    assert.equal(fs.existsSync(ws.ledgerPath), false, "--ledger must not be written on batch failure");
  });

  it("T2-F4-05: >100 incoming rules/corrections trigger bounded compaction while keeping base_invariante", (t) => {
    const ws = createIsolatedWorkspace(t);
    const originalBase = readJson(ws.basePath);
    const manyRules = [];
    const manyCorrs = [];
    for (let i = 1; i <= 120; i += 1) {
      manyRules.push(
        crearRegla({
          id: `regla-cap-${String(i).padStart(3, "0")}`,
          regla: `Regla de prueba de capacidad número ${i} para despacho municipal ${i}.`,
          triggers: [`despacho${i}`, "audiencia", "penal"],
          alpha: 4.0 + (i % 15),
          beta: 1.5,
          impresiones: 5 + (i % 20),
          aciertos_directos: 4 + (i % 19),
          fallos_directos: 1,
          origen_correccion_id: `corr-cap-${String(i).padStart(3, "0")}`,
        })
      );
      manyCorrs.push(
        crearCorreccion({
          id: `corr-cap-${String(i).padStart(3, "0")}`,
          texto_original: `Audiencia en despacho ${i} contra [PROCESADO]`,
          regla_id_asociada: `regla-cap-${String(i).padStart(3, "0")}`,
        })
      );
    }
    const pkg = crearPaqueteV5({
      instancia_id: "inst-capacity-stress",
      reglas: manyRules,
      correcciones: manyCorrs,
    });
    const pkgPath = writeTempFixture(ws.tmpDir, "capacity_120.json", pkg);
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
    assertBaseInvariantPreserved(originalBase, updatedBase);
    assert.ok(
      updatedBase.reglas.length < 123,
      `Expected compaction below 123 rules, got ${updatedBase.reglas.length}`
    );
    assert.ok(
      updatedBase.correcciones.length <= 100,
      `Expected compacted corrections <= 100, got ${updatedBase.correcciones.length}`
    );
  });
});

describe("Tier 2 — F5: Out-of-Order, Collision & Cross-Source Ledger Boundaries", () => {
  it("T2-F5-01: out-of-order Month 3 then Month 1 ignores stale Month 1 (keeps 55 audiencias)", (t) => {
    const ws = createIsolatedWorkspace(t);
    const r3 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v4_mes3_usuario_a_superset.json"),
    ]);
    assert.equal(r3.status, 0, r3.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 55);

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
    assert.equal(
      extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)),
      55,
      "Stale Month 1 must not regress Month 3 count"
    );
  });

  it("T2-F5-02: same formato: 4 user re-exported with newer exportado timestamp & identical content is idempotent", (t) => {
    const ws = createIsolatedWorkspace(t);
    const m1Path = fixturePath("v4_mes1_usuario_a.json");
    const r1 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      m1Path,
    ]);
    assert.equal(r1.status, 0, r1.stderr);
    const baseCount1 = readJson(ws.basePath).reglas.length;

    const reExported = readJson(m1Path);
    reExported.exportado = "2026-10-31T21:00:00-05:00";
    const m1RePath = writeTempFixture(ws.tmpDir, "v4_reexported.json", reExported);

    const r2 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      m1RePath,
    ]);
    assert.equal(r2.status, 0, r2.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 20);
    assert.equal(readJson(ws.basePath).reglas.length, baseCount1);
  });

  it("T2-F5-03: two distinct formato: 4 users with different sembrado_en and zero ID overlap are summed as separate sources", (t) => {
    const ws = createIsolatedWorkspace(t);
    const u1 = crearPaqueteV4({
      exportado: "2026-11-15T10:00:00-05:00",
      reglas: [crearRegla({ id: "regla-u1-only", triggers: ["u1", "valledupar"] })],
      correcciones: [crearCorreccion({ id: "corr-u1-only" })],
      uso_catalogos_overrides: {
        total_audiencias_contabilizadas: 15,
        sembrado_en: "2026-09-01T08:00:00-05:00",
      },
    });
    const u2 = crearPaqueteV4({
      exportado: "2026-11-15T10:01:00-05:00",
      reglas: [crearRegla({ id: "regla-u2-only", triggers: ["u2", "riohacha"] })],
      correcciones: [crearCorreccion({ id: "corr-u2-only", defensor_clave: "carlos mendez" })],
      defensores_directorio: [
        {
          clave: "carlos mendez",
          nombre: "Carlos Mendez",
          aciertos: 10,
          correcciones: 1,
          tasa_acierto: 0.9,
          por_campo: [],
        },
      ],
      uso_catalogos_overrides: {
        total_audiencias_contabilizadas: 25,
        sembrado_en: "2026-09-10T14:00:00-05:00",
      },
    });
    const p1 = writeTempFixture(ws.tmpDir, "u1_v4.json", u1);
    const p2 = writeTempFixture(ws.tmpDir, "u2_v4.json", u2);
    const res = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      p1,
      p2,
    ]);
    assert.equal(res.status, 0, res.stderr);
    const ledger = readJson(ws.ledgerPath);
    assert.ok(extractLedgerSources(ledger).length >= 2);
    assert.equal(extractTotalAudienciasFromLedger(ledger), 40);
  });

  it("T2-F5-04: formato: 5 decreasing exportacion_seq (seq: 2 then seq: 1) is treated as stale", (t) => {
    const ws = createIsolatedWorkspace(t);
    const r2 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq2.json"),
    ]);
    assert.equal(r2.status, 0, r2.stderr);
    assert.equal(extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)), 65);

    const r1 = runIngestar([
      "--base",
      ws.basePath,
      "--ledger",
      ws.ledgerPath,
      "--catalogos-base",
      ws.catalogosBasePath,
      fixturePath("v5_usuario_a_seq1.json"),
    ]);
    assert.equal(r1.status, 0, r1.stderr);
    assert.equal(
      extractTotalAudienciasFromLedger(readJson(ws.ledgerPath)),
      65,
      "Decreasing exportacion_seq must not regress accumulated count"
    );
  });

  it("T2-F5-05: shared rule confirmed by 2 distinct sources (User A & User C) merges without duplicating rule ID", (t) => {
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
    const updatedBase = readJson(ws.basePath);
    const shared = (updatedBase.reglas || []).filter((r) => r.id === "regla-multi-confirmada");
    assert.equal(shared.length, 1, "Shared rule must appear exactly once in --base");
    assert.ok(
      (shared[0].defensores_confirmantes || []).length >= 2 || shared[0].refuerzos >= 3,
      "Merged rule should combine confirmation evidence from both users"
    );
  });
});

describe("Tier 2 — F6: Catalog Structural Integrity & Template Immutability", () => {
  it("T2-F6-01: no court in juzgados_base.json or JUZGADOS_BASE has placeholders, empty strings, or double spaces", async () => {
    const juzgados = readJson(REAL_CATALOGOS_PATH);
    const js = await loadFrontendConstants();
    for (const list of [juzgados, js.JUZGADOS_BASE]) {
      assert.ok(list.length >= 96);
      for (const item of list) {
        assert.equal(typeof item, "string");
        assert.ok(item.trim().length > 0, "Empty court entry");
        assert.equal(item, item.trim(), `Leading/trailing whitespace in "${item}"`);
        assert.ok(!/\s{2,}/.test(item), `Double space in "${item}"`);
        assert.ok(!/\[PROCESADO\]|\[RADICADO\]/i.test(item), `Placeholder leaked in "${item}"`);
      }
    }
  });

  it("T2-F6-02: no duplicate courts or hearing types after normalizarClaveCatalogo", async () => {
    const juzgados = readJson(REAL_CATALOGOS_PATH);
    const rust = parseModelsRsCatalogs();
    const normJuzgados = juzgados.map((j) => normalizarTextoSinTildes(j));
    assert.equal(new Set(normJuzgados).size, normJuzgados.length, "Normalized duplicate court found");
    const normTipos = rust.TIPOS_AUDIENCIA.map((t) => normalizarTextoSinTildes(t));
    assert.equal(new Set(normTipos).size, normTipos.length, "Normalized duplicate hearing type found");
  });

  it("T2-F6-03: HORAS_RAPIDAS in Rust and JS uses valid HH:MM (00..23:00..59) and AM/PM labels", async () => {
    const rust = parseModelsRsCatalogs();
    const js = await loadFrontendConstants();
    for (const h of rust.HORAS_RAPIDAS) {
      assert.match(h.h24, /^(?:[01]\d|2[0-3]):[0-5]\d$/, `Invalid Rust HH:MM: ${h.h24}`);
      assert.match(h.label, /^\d{1,2}:\d{2}\s(?:AM|PM)$/, `Invalid Rust label: ${h.label}`);
    }
    for (const h of js.HORAS_RAPIDAS) {
      assert.match(`${h.h24}:${h.m}`, /^(?:[01]\d|2[0-3]):[0-5]\d$/);
      assert.match(h.label, /^\d{1,2}:\d{2}\s(?:AM|PM)$/);
    }
  });

  it("T2-F6-04: MOTIVOS_LABELS preserves exact 8 Defensoría columns I..P in immutable order", async () => {
    const expected = [
      "Juez",
      "Fiscalía",
      "Usuario",
      "INPEC",
      "Víctima",
      "ICBF",
      "Defensor Confianza",
      "Defensor Público",
    ];
    const rust = parseModelsRsCatalogs();
    const js = await loadFrontendConstants();
    assert.deepEqual(rust.MOTIVOS_LABELS, expected);
    assert.deepEqual(js.MOTIVOS_LABELS, expected);
  });

  it("T2-F6-05: templates/plantilla_audiencias.xlsx remains intact and clean", () => {
    assert.ok(fs.existsSync(TEMPLATE_XLSX_PATH), "Official Excel template missing");
    const stats = fs.statSync(TEMPLATE_XLSX_PATH);
    assert.ok(stats.size > 15_000, "Official Excel template unexpectedly small/corrupted");
    const entries = fs.readdirSync(TEMPLATES_DIR);
    assert.deepEqual(
      entries,
      ["plantilla_audiencias.xlsx"],
      `Unexpected temp/extra files in templates/: ${entries.join(", ")}`
    );
  });
});

describe("Tier 2 — F7: Repo Base/Ledger Integrity & Dry-Run CLI Safety", () => {
  it("T2-F7-01: no rule in repo aprendizaje_base.json has duplicate id or empty regla text", () => {
    const baseObj = readJson(REAL_BASE_PATH);
    const ids = new Set();
    for (const r of baseObj.reglas || []) {
      assert.ok(typeof r.id === "string" && r.id.trim().length > 0, "Rule missing id");
      assert.ok(!ids.has(r.id), `Duplicate rule id in repo base: ${r.id}`);
      ids.add(r.id);
      assert.ok(typeof r.regla === "string" && r.regla.trim().length > 0, `Empty regla in ${r.id}`);
    }
  });

  it("T2-F7-02: all corrections in repo aprendizaje_base.json preserve all 8 AudienciaInput fields", () => {
    const baseObj = readJson(REAL_BASE_PATH);
    const fields = ["radicado", "tipo", "fecha", "hora", "juzgado", "realizada", "motivo", "observaciones"];
    for (const c of baseObj.correcciones || []) {
      for (const side of ["prediccion", "final_usuario"]) {
        assert.ok(c[side] && typeof c[side] === "object", `${c.id} missing ${side}`);
        for (const f of fields) {
          assert.equal(typeof c[side][f], "string", `${c.id}.${side}.${f} must be string`);
        }
      }
    }
  });

  it("T2-F7-03: repo ingesta_ledger.json has finite non-negative numbers across all sources", () => {
    assert.ok(fs.existsSync(REAL_LEDGER_PATH), `Missing ${REAL_LEDGER_PATH}`);
    const ledger = readJson(REAL_LEDGER_PATH);
    assertAllNumbersFinite(ledger, "ingesta_ledger.json");
    const assertNonNegative = (val, p = "ledger") => {
      if (typeof val === "number") {
        assert.ok(val >= 0, `Negative number at ${p}: ${val}`);
      } else if (Array.isArray(val)) {
        val.forEach((v, i) => assertNonNegative(v, `${p}[${i}]`));
      } else if (val && typeof val === "object") {
        for (const [k, v] of Object.entries(val)) {
          assertNonNegative(v, `${p}.${k}`);
        }
      }
    };
    assertNonNegative(ledger);
  });

  it("T2-F7-04: --dry-run without --reporte prints summary to stdout and creates no files in CWD", (t) => {
    const ws = createIsolatedWorkspace(t);
    const emptyCwd = path.join(ws.tmpDir, "cwd_check");
    fs.mkdirSync(emptyCwd);
    const res = runIngestar(
      [
        "--base",
        ws.basePath,
        "--ledger",
        ws.ledgerPath,
        "--catalogos-base",
        ws.catalogosBasePath,
        "--dry-run",
        fixturePath("v4_real_slice.json"),
      ],
      { cwd: emptyCwd }
    );
    assert.equal(res.status, 0, res.stderr);
    assert.ok(res.stdout.trim().length > 0, "Expected stdout summary when --reporte omitted");
    assert.deepEqual(fs.readdirSync(emptyCwd), [], "Unexpected files created in CWD");
  });

  it("T2-F7-05: base_invariante rules in repo aprendizaje_base.json match canonical texts and priors", () => {
    const baseObj = readJson(REAL_BASE_PATH);
    const byId = new Map((baseObj.reglas || []).map((r) => [r.id, r]));
    for (const id of [
      "base-abreviacion-cto",
      "base-jerarquia-causas-reacciones",
      "base-defensor-confianza-contractual",
    ]) {
      const rule = byId.get(id);
      assert.ok(rule, `Missing canonical base rule ${id}`);
      assert.equal(rule.origen, "base_invariante");
      assert.equal(rule.estado, "activa");
      assert.equal(rule.alpha, 6.0);
      assert.equal(rule.beta, 1.0);
    }
    assert.match(byId.get("base-abreviacion-cto").regla, /CTO/);
    assert.match(byId.get("base-jerarquia-causas-reacciones").regla, /causa principal/i);
    assert.match(byId.get("base-defensor-confianza-contractual").regla, /Defensor Confianza/);
  });
});
