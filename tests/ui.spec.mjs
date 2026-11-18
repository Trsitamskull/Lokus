import { test, expect } from "@playwright/test";

function mockTauriInit() {
  const carpetas = ["2026", "2026/Enero"];
  const archivos = ["2026/Enero/audiencias_enero.xlsx"];
  const colores = { "2026": "azul", "2026/Enero": "verde" };
  const defensores = ["Dra. Laura Gómez", "Dr. Carlos Mendoza"];
  let temaActual = "dark";

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args = {}) => {
      switch (cmd) {
        case "cmd_obtener_config":
          return {
            provider: "openai",
            openai_model: "gpt-5-mini",
            gemini_model: "gemini-2.5-flash",
            theme: temaActual,
            defensores,
            juzgados: [],
            tipos_audiencia: [],
            status: {
              configured: false,
              provider: "openai",
              model: "gpt-5-mini",
              has_openai_key: false,
              has_gemini_key: false,
              masked_openai_key: "",
              masked_gemini_key: "",
            },
          };
        case "cmd_guardar_tema":
          temaActual = args.tema || "dark";
          return {
            status: {
              configured: false,
              provider: "openai",
              model: "gpt-5-mini",
              has_openai_key: false,
              has_gemini_key: false,
              masked_openai_key: "",
              masked_gemini_key: "",
            },
          };
        case "cmd_listar_archivos":
          return archivos;
        case "cmd_listar_carpetas":
          return carpetas;
        case "cmd_obtener_orden_carpetas":
          return carpetas;
        case "cmd_obtener_colores_carpetas":
          return colores;
        case "cmd_resumen_aprendizaje_ia":
          return {
            aciertos: 12,
            total_correcciones: 2,
            total_reglas: 3,
            pendientes_exportar: 1,
            hubo_correccion: false,
            aclaracion_pendiente: null,
          };
        case "cmd_diagnostico_aprendizaje_ia":
          return {
            aciertos_totales: 12,
            correcciones_totales: 2,
            ejemplos_positivos_totales: 10,
            llamadas_induccion_llm_total: 1,
            clusters_pendientes: 1,
            clusters_listos: 0,
            drift_campos_activos: [],
            enfriamiento_aclaracion: 0,
            reglas_por_estado: { activa: 2, prueba: 1 },
            metricas_por_campo: [
              {
                campo: "realizada",
                evaluaciones: 14,
                aciertos: 13,
                correcciones: 1,
                precision_ewma: 0.93,
                confianza_q10: 0.72,
                drift_activo: false,
              },
              {
                campo: "motivo",
                evaluaciones: 14,
                aciertos: 12,
                correcciones: 2,
                precision_ewma: 0.86,
                confianza_q10: 0.61,
                drift_activo: false,
              },
            ],
            metricas_por_modelo: [
              {
                clave: "openai:gpt-5-mini",
                evaluaciones: 14,
                aciertos: 12,
                correcciones: 2,
                precision_media: 0.86,
              },
            ],
            preferencias_estilo: {
              incluir_causas_secundarias_prob: 0.75,
              incluir_reprogramacion_prob: 0.8,
              se_fija_aplazamiento_prob: 0.5,
            },
            alias_validados: [],
            reglas_destacadas: [
              {
                id: "base-1",
                estado: "activa",
                campo_objetivo: "juzgado",
                origen: "base_invariante",
                regla: "Conservar abreviaciones CTO, MPAL y PCTO tal como aparecen",
                impresiones: 6,
                aciertos_directos: 6,
                fallos_directos: 0,
                confianza_q10: 0.81,
              },
            ],
          };
        case "cmd_leer_archivo":
          return {
            registros: [
              {
                numero: 1,
                fila_excel: 11,
                radicado: "200016001075202500123",
                tipo: "Audiencia de acusación",
                fecha: "15/01/2026",
                hora: "09:00",
                juzgado: "Juzgado 1 Penal del Circuito de Valledupar",
                realizada: "SI",
                motivo: "",
                observaciones: "Instalada sin novedad.",
              },
            ],
            totales: {
              num_registros: 1,
              total_si: 1,
              total_no: 0,
              totales_motivos: [0, 0, 0, 0, 0, 0, 0, 0],
            },
            defensor: "Dra. Laura Gómez",
          };
        default:
          return null;
      }
    },
    transformCallback: () => 0,
  };
}

test.describe("Lokus UI smoke checks", () => {
  test("menú contextual en carpetas, plegado activo, selector de defensores y temas", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit);
    await page.goto("/");

    const carpeta2026Row = page.locator('[data-context-folder="2026"]');
    await expect(carpeta2026Row).toBeVisible();

    // Sin barra flotante de botones en filas de carpeta
    await expect(carpeta2026Row.locator(".file-row-actions")).toHaveCount(0);

    // Desactiva el menú contextual nativo dentro de .sidebar-files
    const defaultPrevented = await page.evaluate(() => {
      const target = document.querySelector(".sidebar-files");
      const ev = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: 40,
        clientY: 200,
      });
      target.dispatchEvent(ev);
      return ev.defaultPrevented;
    });
    expect(defaultPrevented).toBe(true);

    // Clic derecho estilo Linux/WebKitGTK (pointerdown -> contextmenu -> pointerup)
    // no debe cerrarse al soltar el botón derecho
    const box = await carpeta2026Row.boundingBox();
    await page.mouse.move(box.x + 24, box.y + box.height / 2);
    await page.mouse.down({ button: "right" });
    await page.evaluate(
      ({ x, y }) => {
        const btn = document.querySelector('[data-context-folder="2026"] .sidebar-folder-btn');
        btn.dispatchEvent(
          new MouseEvent("contextmenu", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: x,
            clientY: y,
          })
        );
      },
      { x: box.x + 24, y: box.y + box.height / 2 }
    );
    await page.mouse.up({ button: "right" });

    const ctxMenu = page.locator(".folder-context-menu");
    await expect(ctxMenu).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Nueva planilla aquí" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Nueva subcarpeta" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Cambiar color" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Renombrar o mover" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Eliminar carpeta" })).toBeVisible();

    // Escape cierra el menú contextual
    await page.keyboard.press("Escape");
    await expect(ctxMenu).toBeHidden();

    // Abrir "Nueva planilla aquí" desde el menú contextual y validar lista de defensores
    await carpeta2026Row.click({ button: "right" });
    await ctxMenu.getByRole("button", { name: "Nueva planilla aquí" }).click();
    await expect(page.getByRole("heading", { name: "Nuevo archivo de audiencias" })).toBeVisible();
    await page.getByRole("button", { name: "Mostrar opciones guardadas" }).click();
    await expect(page.getByRole("option", { name: "Dra. Laura Gómez" })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Cancelar" }).click();

    // Archivo Excel tampoco muestra barra flotante; abre menú contextual con clic derecho
    const archivoRow = page.locator('[data-context-file="2026/Enero/audiencias_enero.xlsx"]');
    await expect(archivoRow).toBeVisible();
    await expect(archivoRow.locator(".file-row-actions")).toHaveCount(0);
    await archivoRow.click({ button: "right" });
    await expect(ctxMenu).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Mover a otra carpeta" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Asignar o editar defensor" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Historial de respaldos" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Exportar copia a Descargas" })).toBeVisible();
    await expect(ctxMenu.getByRole("button", { name: "Eliminar planilla" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(ctxMenu).toBeHidden();

    // Plegar una carpeta aunque contenga la planilla activa
    const carpetaBtn = carpeta2026Row.locator(".sidebar-folder-btn");
    await expect(carpetaBtn).toHaveAttribute("aria-expanded", "true");
    await carpetaBtn.click();
    await expect(carpetaBtn).toHaveAttribute("aria-expanded", "false");
    await expect(carpeta2026Row.locator(".folder-active-dot")).toBeVisible();

    // Colapsar y expandir barra lateral en escritorio con botón visible y Ctrl+B
    const toggleSidebarBtn = page.locator(".sidebar-toggle-btn");
    await expect(toggleSidebarBtn).toHaveAttribute("aria-expanded", "true");
    await toggleSidebarBtn.click();
    await expect(page.locator(".desktop-frame")).toHaveClass(/sidebar-collapsed/);
    await expect(toggleSidebarBtn).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("Control+b");
    await expect(page.locator(".desktop-frame")).not.toHaveClass(/sidebar-collapsed/);

    // Acceso directo al defensor desde .active-doc-bar
    await page.locator(".active-doc-defensor-btn").click();
    await expect(page.getByRole("heading", { name: "Defensor Público de la planilla" })).toBeVisible();
    await page.keyboard.press("Escape");

    // Validación 12h, AM/PM, flechas ←/→ y presets en Hora
    const horaInput = page.locator("#hora");
    const minutoInput = page.locator("#minuto");
    await horaInput.fill("15");
    await expect(page.locator("#hora-error-msg")).toHaveCount(0);
    await minutoInput.blur();
    await page.locator("#radicado").focus();
    await expect(page.locator("#hora-error-msg")).toContainText("01 y 12");
    await horaInput.fill("02");
    await minutoInput.fill("30");
    await expect(page.locator("#hora-error-msg")).toHaveCount(0);

    // Navegación con flechas ←/→ entre HH, MM y AM/PM
    await horaInput.focus();
    await page.evaluate(() => {
      const h = document.getElementById("hora");
      h.setSelectionRange(h.value.length, h.value.length);
    });
    await page.keyboard.press("ArrowRight");
    await expect(minutoInput).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    await expect(horaInput).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    const amBtn = page.locator(".time-meridiem-btn", { hasText: "AM" });
    const pmBtn = page.locator(".time-meridiem-btn", { hasText: "PM" });
    await expect(amBtn).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(pmBtn).toBeFocused();
    await pmBtn.click();
    await expect(pmBtn).toHaveAttribute("aria-pressed", "true");
    await page.locator(".preset-btn", { hasText: "8:00 AM" }).click();
    await expect(horaInput).toHaveValue("08");
    await expect(minutoInput).toHaveValue("00");
    await expect(amBtn).toHaveAttribute("aria-pressed", "true");

    // Navegación por rueda del ratón y rejillas 4x3 de mes y año en DatePickerInput
    const fechaInput = page.locator("#fecha");
    await fechaInput.fill("18/01/2026");
    const calPopover = page.locator(".calendar-popover");
    await expect(calPopover).toBeVisible();
    const mesBtn = calPopover.locator(".calendar-select-trigger").first();
    const anioBtn = calPopover.locator(".calendar-select-trigger.is-year");
    await expect(mesBtn).toContainText("Enero");
    await expect(anioBtn).toContainText("2026");
    await mesBtn.hover();
    await page.mouse.wheel(0, 100);
    await expect(mesBtn).toContainText("Febrero");
    await mesBtn.hover();
    await page.mouse.wheel(0, -100);
    await expect(mesBtn).toContainText("Enero");
    await anioBtn.hover();
    await page.mouse.wheel(0, 100);
    await expect(anioBtn).toContainText("2027");

    // Rejilla 4x3 de meses
    await mesBtn.click();
    const monthsMatrix = calPopover.locator(".calendar-months-matrix");
    await expect(monthsMatrix).toBeVisible();
    await expect(monthsMatrix.locator(".calendar-cell-btn")).toHaveCount(12);
    await monthsMatrix.getByRole("button", { name: "Marzo" }).click();
    await expect(mesBtn).toContainText("Marzo");
    await expect(calPopover.locator(".calendar-grid")).toBeVisible();

    // Rejilla 4x3 paginada de años (2018-2050)
    await anioBtn.click();
    const yearsMatrix = calPopover.locator(".calendar-years-matrix");
    await expect(yearsMatrix).toBeVisible();
    await expect(calPopover.locator(".calendar-subview-label")).toContainText("2018 – 2029");
    await calPopover.getByRole("button", { name: "Años siguientes" }).click();
    await expect(calPopover.locator(".calendar-subview-label")).toContainText("2030 – 2041");
    await yearsMatrix.getByRole("button", { name: "2030" }).click();
    await expect(anioBtn).toContainText("2030");
    await expect(calPopover.locator(".calendar-grid")).toBeVisible();

    await fechaInput.focus();
    await page.keyboard.press("Escape");
    await expect(calPopover).toBeHidden();

    // Verificar encabezado <h1> en vista Registros
    await page.getByRole("tab", { name: /Registros/ }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Registros de la Planilla" })).toBeVisible();

    // Verificar sección diagnóstica discreta de autoaprendizaje en modal Configurar IA
    await page.getByRole("button", { name: /Configurar IA/i }).click();
    const diagSummary = page.locator("summary.ia-diag-summary");
    await expect(diagSummary).toBeVisible();
    await diagSummary.click();
    await expect(page.getByRole("heading", { level: 3, name: "Fiabilidad por campo" })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "Modelos evaluados" })).toBeVisible();
    await expect(page.locator(".ia-diag-table")).toContainText("realizada");
    await expect(page.locator(".ia-diag-rules-list")).toContainText("CTO");
    await page.getByRole("button", { name: "Cancelar" }).click();

    // Cambio de tema y verificación de no desbordamiento horizontal
    await toggleSidebarBtn.click();
    await page.keyboard.press("Control+b");
    await page.getByRole("button", { name: "Cambiar tema" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth
    );
    expect(overflow).toBe(false);
    expect(errors).toEqual([]);
  });
});
