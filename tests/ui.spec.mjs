import { test, expect } from "@playwright/test";

function mockTauriInit(opts = {}) {
  const configured = Boolean(opts?.configured);
  const carpetas = ["2026", "2026/Enero"];
  const archivos = ["2026/Enero/audiencias_enero.xlsx"];
  const colores = { "2026": "azul", "2026/Enero": "verde" };
  const defensores = ["Dra. Laura Gómez", "Dr. Carlos Mendoza"];
  let temaActual = "dark";
  let juzgadosUsuario = [];
  let tiposUsuario = [];

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
            juzgados: juzgadosUsuario,
            tipos_audiencia: tiposUsuario,
            status: {
              configured,
              provider: "openai",
              model: "gpt-5-mini",
              has_openai_key: configured,
              has_gemini_key: false,
              masked_openai_key: configured ? "sk-***1234" : "",
              masked_gemini_key: "",
            },
          };
        case "cmd_agregar_juzgado": {
          const limpio = (args.nombre || "").trim().replace(/\s+/g, " ");
          const norm = limpio
            .normalize("NFD")
            .replace(/[\u0300-\u036f]/g, "")
            .replace(/[^\p{L}\p{N}]+/gu, " ")
            .trim()
            .toLowerCase();
          const yaEnBase = norm === "tribunal superior del distrito judicial de valledupar sala penal";
          if (norm && !yaEnBase && !juzgadosUsuario.some((j) => j.toLowerCase() === limpio.toLowerCase())) {
            juzgadosUsuario = [...juzgadosUsuario, limpio];
          }
          return { juzgados: juzgadosUsuario };
        }
        case "cmd_eliminar_juzgado": {
          const target = (args.nombre || "").trim().toLowerCase();
          juzgadosUsuario = juzgadosUsuario.filter((j) => j.toLowerCase() !== target);
          return { juzgados: juzgadosUsuario };
        }
        case "cmd_agregar_tipo_audiencia": {
          const limpio = (args.nombre || "").trim().replace(/\s+/g, " ");
          const norm = limpio
            .normalize("NFD")
            .replace(/[\u0300-\u036f]/g, "")
            .replace(/[^\p{L}\p{N}]+/gu, " ")
            .trim()
            .toLowerCase();
          if (norm && norm !== "audiencia de imputacion") {
            tiposUsuario = [...tiposUsuario, limpio];
          }
          return { tipos_audiencia: tiposUsuario };
        }
        case "cmd_eliminar_tipo_audiencia": {
          const target = (args.nombre || "").trim().toLowerCase();
          tiposUsuario = tiposUsuario.filter((t) => t.toLowerCase() !== target);
          return { tipos_audiencia: tiposUsuario };
        }
        case "cmd_guardar_tema":
          temaActual = args.tema || "dark";
          return {
            status: {
              configured,
              provider: "openai",
              model: "gpt-5-mini",
              has_openai_key: configured,
              has_gemini_key: false,
              masked_openai_key: configured ? "sk-***1234" : "",
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
            pendientes_exportar: 3,
            pendientes_ia: 1,
            pendientes_uso: 2,
            total_audiencias_contabilizadas: 5,
            puede_exportar: true,
            hubo_correccion: false,
            aclaracion_pendiente: null,
          };
        case "cmd_exportar_aprendizaje_ia":
          return "/home/test/Descargas/lokus_uso_y_aprendizaje_ia_test.json";
        case "cmd_autocompletar_ia":
          return {
            radicado: "200016001075202500999",
            tipo_audiencia: "Audiencia de imputacion",
            tipo_otra: "",
            fecha: "18/01/2026",
            hora: "15",
            minuto: "00",
            juzgado: "juzgado 1 promiscuo municipal de agustin codazzi",
            se_realizo: "SI",
            motivo: "",
            observaciones: "Se realizó audiencia preliminar.",
            reglas_inyectadas: [],
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
    // 6 accesos rápidos de hora en fila única debajo de HH:MM AM/PM
    const presetBtns = page.locator(".time-presets .preset-btn");
    await expect(presetBtns).toHaveCount(6);
    await expect(presetBtns).toHaveText([
      "8:00 AM",
      "9:00 AM",
      "10:00 AM",
      "2:00 PM",
      "3:00 PM",
      "4:00 PM",
    ]);
    const presetLayout = await page.evaluate(() => {
      const container = document.querySelector(".time-presets");
      const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
      const tops = btns.map((b) => Math.round(b.getBoundingClientRect().top));
      const anyClipped = btns.some((b) => b.scrollWidth > b.clientWidth + 1);
      return {
        rowSpread: Math.max(...tops) - Math.min(...tops),
        containerOverflow: container.scrollWidth > container.clientWidth + 1,
        anyClipped,
      };
    });
    expect(presetLayout.rowSpread).toBeLessThan(2);
    expect(presetLayout.containerOverflow).toBe(false);
    expect(presetLayout.anyClipped).toBe(false);

    await page.locator(".time-presets .preset-btn", { hasText: "9:00 AM" }).click();
    await expect(horaInput).toHaveValue("09");
    await expect(minutoInput).toHaveValue("00");
    await expect(amBtn).toHaveAttribute("aria-pressed", "true");

    await page.locator(".time-presets .preset-btn", { hasText: "3:00 PM" }).click();
    await expect(horaInput).toHaveValue("03");
    await expect(minutoInput).toHaveValue("00");
    await expect(pmBtn).toHaveAttribute("aria-pressed", "true");

    await page.locator(".time-presets .preset-btn", { hasText: "8:00 AM" }).click();
    await expect(horaInput).toHaveValue("08");
    await expect(minutoInput).toHaveValue("00");
    await expect(amBtn).toHaveAttribute("aria-pressed", "true");

    // Botón compartir unificado (badge de novedades y limpieza tras exportar)
    const shareBtn = page.getByRole("button", {
      name: "Compartir uso de catálogos y aprendizaje IA",
    });
    await expect(shareBtn).toHaveAttribute("title", /5 audiencias/);
    await expect(shareBtn.locator(".share-badge")).toHaveText("3");
    await shareBtn.click();
    await expect(shareBtn.locator(".share-badge")).toHaveCount(0);
    await expect(page.locator(".toast")).toContainText("Uso de catálogos y aprendizaje exportados");

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

    // Cambio de tema y verificación de no desbordamiento horizontal en tema claro
    await toggleSidebarBtn.click();
    await page.keyboard.press("Control+b");
    await page.getByRole("button", { name: "Cambiar tema" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.getByRole("tab", { name: /Nueva Audiencia/ }).click();
    const lightCheck = await page.evaluate(() => {
      const docOverflow =
        document.documentElement.scrollWidth > document.documentElement.clientWidth;
      const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
      const tops = btns.map((b) => Math.round(b.getBoundingClientRect().top));
      return {
        docOverflow,
        rowSpread: Math.max(...tops) - Math.min(...tops),
      };
    });
    expect(lightCheck.docOverflow).toBe(false);
    expect(lightCheck.rowSpread).toBeLessThan(2);
    expect(errors).toEqual([]);
  });

  test("autocompletado IA empata sin tildes ni mayúsculas con el catálogo y resalta 3:00 PM", async ({
    page,
  }) => {
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.addInitScript(mockTauriInit, { configured: true });
    await page.goto("/");

    const iaTextarea = page.locator("#ia-inline-textarea");
    await expect(iaTextarea).toBeVisible();
    await iaTextarea.fill(
      "Audiencia de imputacion ante juzgado 1 promiscuo municipal de agustin codazzi a las 3 pm"
    );
    await page.getByRole("button", { name: /Autocompletar con IA/i }).click();

    await expect(page.locator("#tipo-audiencia")).toContainText("Audiencia de imputación");
    await expect(page.locator("#tipo-otra")).toHaveCount(0);
    await expect(page.locator("#juzgado")).toContainText(
      "Juzgado 1 Promiscuo Municipal de Agustín Codazzi"
    );
    await expect(page.locator("#juzgado-otra")).toHaveCount(0);
    await expect(
      page.locator(".time-presets .preset-btn", { hasText: "3:00 PM" })
    ).toHaveAttribute("aria-pressed", "true");

    // Verificar diseño en ventana estrecha (3x2 en accesos rápidos sin desbordamiento ni recortes en todos los anchos)
    for (const width of [1280, 1024, 980, 960, 720, 670, 580, 480]) {
      await page.setViewportSize({ width, height: 820 });
      const layout = await page.evaluate(() => {
        const docOverflow =
          document.documentElement.scrollWidth > document.documentElement.clientWidth;
        const container = document.querySelector(".time-presets");
        const btns = Array.from(document.querySelectorAll(".time-presets .preset-btn"));
        const anyClipped = btns.some((b) => b.scrollWidth > b.clientWidth + 1);
        const containerOverflow = container.scrollWidth > container.clientWidth + 1;
        return { docOverflow, anyClipped, containerOverflow };
      });
      expect(layout.docOverflow, `docOverflow at ${width}px`).toBe(false);
      expect(layout.anyClipped, `anyClipped at ${width}px`).toBe(false);
      expect(layout.containerOverflow, `containerOverflow at ${width}px`).toBe(false);
    }

    // Probar flujo "+ Agregar nuevo" en CustomSelect con variante sin tilde y puntuación distinta
    await page.setViewportSize({ width: 1280, height: 820 });
    await page.locator("#tipo-audiencia").click();
    await page.locator(".custom-select-add-trigger").click();
    await page.locator(".custom-select-add-input").fill("Audiencia de imputacion");
    await page.locator(".custom-select-add-save").click();
    await expect(page.locator("#tipo-audiencia")).toContainText("Audiencia de imputación");
    await expect(page.locator(".toast")).toContainText("ya existe en los tipos de audiencia");

    await page.locator("#juzgado").click();
    await page.locator(".custom-select-add-trigger").click();
    await page
      .locator(".custom-select-add-input")
      .fill("Tribunal Superior del Distrito Judicial de Valledupar Sala Penal");
    await page.locator(".custom-select-add-save").click();
    await expect(page.locator("#juzgado")).toContainText(
      "Tribunal Superior del Distrito Judicial de Valledupar - Sala Penal"
    );
    await expect(page.locator(".toast")).toContainText("ya existe en la lista de juzgados");

    // Agregar juzgado nuevo real y luego eliminarlo de la lista
    await page.locator("#juzgado").click();
    await page.locator(".custom-select-add-trigger").click();
    await page.locator(".custom-select-add-input").fill("Juzgado 2 Penal");
    await page.locator(".custom-select-add-save").click();
    await expect(page.locator("#juzgado")).toContainText("Juzgado 2 Penal");
    await expect(page.locator(".toast")).toContainText("agregado a la lista de juzgados");

    await page.locator("#juzgado").click();
    await page.getByRole("button", { name: "Eliminar Juzgado 2 Penal" }).click();
    await expect(page.locator("#juzgado")).toContainText("Sin especificar");

    expect(errors).toEqual([]);
  });
});
