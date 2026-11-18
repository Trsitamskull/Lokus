<script>
  import { Calendar, ChevronLeft, ChevronRight, ChevronDown } from "@lucide/svelte";

  let {
    id = "fecha",
    name = "fecha",
    value = $bindable(""),
    registros = [],
    placeholder = "DD/MM/AAAA",
    invalid = false,
    describedBy = undefined,
    onBlur = undefined,
  } = $props();

  const MESES = [
    "Enero",
    "Febrero",
    "Marzo",
    "Abril",
    "Mayo",
    "Junio",
    "Julio",
    "Agosto",
    "Septiembre",
    "Octubre",
    "Noviembre",
    "Diciembre",
  ];
  const MESES_CORTOS = [
    "Ene",
    "Feb",
    "Mar",
    "Abr",
    "May",
    "Jun",
    "Jul",
    "Ago",
    "Sep",
    "Oct",
    "Nov",
    "Dic",
  ];
  const ANIO_MIN = 2018;
  const ANIO_MAX = 2050;
  const ANIOS_POR_PAGINA = 12;
  const MAX_YEAR_PAGE_START =
    ANIO_MIN + Math.floor((ANIO_MAX - ANIO_MIN) / ANIOS_POR_PAGINA) * ANIOS_POR_PAGINA;
  const DIAS_SEMANA = ["Lu", "Ma", "Mi", "Ju", "Vi", "Sá", "Do"];

  let open = $state(false);
  let vistaCalendario = $state("dias"); // "dias" | "meses" | "anios"
  let containerEl = $state(null);
  let hoy = new Date();
  let viewYear = $state(hoy.getFullYear());
  let viewMonth = $state(hoy.getMonth()); // 0..11
  let yearPageStart = $state(
    ANIO_MIN + Math.floor((hoy.getFullYear() - ANIO_MIN) / ANIOS_POR_PAGINA) * ANIOS_POR_PAGINA
  );
  let wheelAcumMes = 0;
  let wheelAcumAnio = 0;
  let wheelAcumPagina = 0;

  let popoverId = $derived(`${id || "fecha"}-calendar-popover`);

  let fechaHoyStr = $derived.by(() => {
    const d = new Date();
    const dd = String(d.getDate()).padStart(2, "0");
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const yyyy = d.getFullYear();
    return `${dd}/${mm}/${yyyy}`;
  });

  // Cuenta cuántas audiencias existen en la planilla actual por cada fecha DD/MM/YYYY
  let conteoPorFecha = $derived.by(() => {
    const mapa = new Map();
    for (const r of registros) {
      const f = r.fecha?.trim();
      if (f) mapa.set(f, (mapa.get(f) || 0) + 1);
    }
    return mapa;
  });

  function parsearFechaValida(str) {
    const partes = (str || "").trim().split("/");
    if (partes.length !== 3) return null;
    const dia = Number(partes[0]);
    const mes = Number(partes[1]);
    const anio = Number(partes[2]);
    if (
      !Number.isInteger(dia) ||
      !Number.isInteger(mes) ||
      !Number.isInteger(anio) ||
      anio < 2018 ||
      anio > 2050 ||
      mes < 1 ||
      mes > 12 ||
      dia < 1 ||
      dia > 31
    ) {
      return null;
    }
    const fechaObj = new Date(anio, mes - 1, dia);
    if (
      fechaObj.getFullYear() !== anio ||
      fechaObj.getMonth() !== mes - 1 ||
      fechaObj.getDate() !== dia
    ) {
      return null;
    }
    return { dia, mes: mes - 1, anio };
  }

  function sincronizarMesConValor() {
    const parsed = parsearFechaValida(value);
    if (parsed) {
      viewYear = parsed.anio;
      viewMonth = parsed.mes;
    } else {
      const d = new Date();
      viewYear = d.getFullYear();
      viewMonth = d.getMonth();
    }
  }

  function abrirCalendario() {
    if (!open) {
      sincronizarMesConValor();
      vistaCalendario = "dias";
      open = true;
    }
  }

  function toggleCalendario(e) {
    e.stopPropagation();
    if (open) {
      vistaCalendario = "dias";
      open = false;
    } else {
      sincronizarMesConValor();
      vistaCalendario = "dias";
      open = true;
    }
  }

  function toggleVista(tipo, e) {
    e.stopPropagation();
    if (vistaCalendario === tipo) {
      vistaCalendario = "dias";
      return;
    }
    if (tipo === "anios") {
      yearPageStart =
        ANIO_MIN + Math.floor((viewYear - ANIO_MIN) / ANIOS_POR_PAGINA) * ANIOS_POR_PAGINA;
    }
    vistaCalendario = tipo;
  }

  function formatearFechaInput(e) {
    const digitos = e.currentTarget.value.replace(/\D/g, "").slice(0, 8);
    if (digitos.length <= 2) {
      value = digitos;
    } else if (digitos.length <= 4) {
      value = `${digitos.slice(0, 2)}/${digitos.slice(2)}`;
    } else {
      value = `${digitos.slice(0, 2)}/${digitos.slice(2, 4)}/${digitos.slice(4)}`;
    }
    const parsed = parsearFechaValida(value);
    if (parsed) {
      viewYear = parsed.anio;
      viewMonth = parsed.mes;
    }
  }

  function cambiarMes(delta) {
    let nuevoMes = viewMonth + delta;
    let nuevoAnio = viewYear;
    if (nuevoMes < 0) {
      nuevoMes = 11;
      nuevoAnio -= 1;
    } else if (nuevoMes > 11) {
      nuevoMes = 0;
      nuevoAnio += 1;
    }
    if (nuevoAnio >= 2018 && nuevoAnio <= 2050) {
      viewMonth = nuevoMes;
      viewYear = nuevoAnio;
    }
  }

  function cambiarAnio(delta) {
    const siguiente = Math.min(ANIO_MAX, Math.max(ANIO_MIN, viewYear + delta));
    viewYear = siguiente;
    yearPageStart =
      ANIO_MIN + Math.floor((siguiente - ANIO_MIN) / ANIOS_POR_PAGINA) * ANIOS_POR_PAGINA;
  }

  function cambiarPaginaAnios(delta) {
    yearPageStart = Math.min(
      MAX_YEAR_PAGE_START,
      Math.max(ANIO_MIN, yearPageStart + delta * ANIOS_POR_PAGINA)
    );
  }

  function calcularPasoWheel(e, acumActual) {
    const delta = e.deltaMode === 1 ? e.deltaY * 32 : e.deltaY;
    const umbral = 24;
    if (Math.abs(delta) >= umbral) {
      return { paso: delta > 0 ? 1 : -1, resto: 0 };
    }
    const siguienteAcum = acumActual + delta;
    if (Math.abs(siguienteAcum) >= umbral) {
      return { paso: siguienteAcum > 0 ? 1 : -1, resto: 0 };
    }
    return { paso: 0, resto: siguienteAcum };
  }

  function manejarWheelMes(e) {
    e.preventDefault();
    e.stopPropagation();
    const { paso, resto } = calcularPasoWheel(e, wheelAcumMes);
    wheelAcumMes = resto;
    if (paso !== 0) cambiarMes(paso);
  }

  function manejarWheelAnio(e) {
    e.preventDefault();
    e.stopPropagation();
    const { paso, resto } = calcularPasoWheel(e, wheelAcumAnio);
    wheelAcumAnio = resto;
    if (paso !== 0) cambiarAnio(paso);
  }

  function manejarWheelPaginaAnios(e) {
    e.preventDefault();
    e.stopPropagation();
    const { paso, resto } = calcularPasoWheel(e, wheelAcumPagina);
    wheelAcumPagina = resto;
    if (paso !== 0) cambiarPaginaAnios(paso);
  }

  function navegarEncabezado(delta) {
    if (vistaCalendario === "meses") {
      cambiarAnio(delta);
    } else if (vistaCalendario === "anios") {
      cambiarPaginaAnios(delta);
    } else {
      cambiarMes(delta);
    }
  }

  let aniosPagina = $derived.by(() => {
    const lista = [];
    for (let a = yearPageStart; a < yearPageStart + ANIOS_POR_PAGINA && a <= ANIO_MAX; a++) {
      lista.push(a);
    }
    return lista;
  });

  let anioFinPagina = $derived(Math.min(ANIO_MAX, yearPageStart + ANIOS_POR_PAGINA - 1));
  let ariaNavAnterior = $derived(
    vistaCalendario === "anios"
      ? "Años anteriores"
      : vistaCalendario === "meses"
        ? "Año anterior"
        : "Mes anterior"
  );
  let ariaNavSiguiente = $derived(
    vistaCalendario === "anios"
      ? "Años siguientes"
      : vistaCalendario === "meses"
        ? "Año siguiente"
        : "Mes siguiente"
  );
  let navAnteriorDeshabilitado = $derived(
    (vistaCalendario === "anios" && yearPageStart <= ANIO_MIN) ||
      (vistaCalendario === "meses" && viewYear <= ANIO_MIN) ||
      (vistaCalendario === "dias" && viewYear <= ANIO_MIN && viewMonth === 0)
  );
  let navSiguienteDeshabilitado = $derived(
    (vistaCalendario === "anios" && yearPageStart >= MAX_YEAR_PAGE_START) ||
      (vistaCalendario === "meses" && viewYear >= ANIO_MAX) ||
      (vistaCalendario === "dias" && viewYear >= ANIO_MAX && viewMonth === 11)
  );

  function seleccionarFecha(fechaStr) {
    value = fechaStr;
    vistaCalendario = "dias";
    open = false;
  }

  function seleccionarHoy() {
    const d = new Date();
    viewYear = d.getFullYear();
    viewMonth = d.getMonth();
    value = fechaHoyStr;
    vistaCalendario = "dias";
    open = false;
  }

  let celdasCalendario = $derived.by(() => {
    const primerDiaMes = new Date(viewYear, viewMonth, 1);
    // Convertir domingo (0) a 6 y lunes (1) a 0
    const offsetLunes = (primerDiaMes.getDay() + 6) % 7;
    const diasEnMes = new Date(viewYear, viewMonth + 1, 0).getDate();
    const diasMesAnterior = new Date(viewYear, viewMonth, 0).getDate();

    const celdas = [];

    // Días del mes anterior
    for (let i = offsetLunes - 1; i >= 0; i--) {
      const dia = diasMesAnterior - i;
      const m = viewMonth === 0 ? 11 : viewMonth - 1;
      const y = viewMonth === 0 ? viewYear - 1 : viewYear;
      const str = `${String(dia).padStart(2, "0")}/${String(m + 1).padStart(2, "0")}/${y}`;
      celdas.push({
        dia,
        fechaStr: str,
        otroMes: true,
        conteo: conteoPorFecha.get(str) || 0,
      });
    }

    // Días del mes actual
    for (let dia = 1; dia <= diasEnMes; dia++) {
      const str = `${String(dia).padStart(2, "0")}/${String(viewMonth + 1).padStart(2, "0")}/${viewYear}`;
      celdas.push({
        dia,
        fechaStr: str,
        otroMes: false,
        conteo: conteoPorFecha.get(str) || 0,
      });
    }

    // Días del mes siguiente solo para completar la última semana visible (4, 5 o 6 filas)
    const totalCeldas = Math.ceil(celdas.length / 7) * 7;
    const restantes = totalCeldas - celdas.length;
    for (let dia = 1; dia <= restantes; dia++) {
      const m = viewMonth === 11 ? 0 : viewMonth + 1;
      const y = viewMonth === 11 ? viewYear + 1 : viewYear;
      const str = `${String(dia).padStart(2, "0")}/${String(m + 1).padStart(2, "0")}/${y}`;
      celdas.push({
        dia,
        fechaStr: str,
        otroMes: true,
        conteo: conteoPorFecha.get(str) || 0,
      });
    }

    return celdas;
  });

  function manejarKeydown(e) {
    if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      if (vistaCalendario !== "dias") {
        vistaCalendario = "dias";
      } else {
        open = false;
      }
    } else if (e.key === "ArrowDown" && !open) {
      abrirCalendario();
    }
  }

  function handleWindowPointerDown(e) {
    if (open && containerEl && !containerEl.contains(e.target)) {
      vistaCalendario = "dias";
      open = false;
    }
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="date-picker-root" bind:this={containerEl}>
  <div class="input-with-icon">
    <input
      {id}
      {name}
      type="text"
      inputmode="numeric"
      maxlength="10"
      class="input has-trailing {invalid ? 'is-invalid' : ''}"
      {value}
      aria-invalid={invalid ? "true" : "false"}
      aria-describedby={describedBy}
      onfocus={abrirCalendario}
      onblur={() => onBlur?.()}
      onclick={abrirCalendario}
      oninput={formatearFechaInput}
      onkeydown={manejarKeydown}
      {placeholder}
      autocorrect="off"
      autocapitalize="none"
      spellcheck="false"
    />
    <button
      type="button"
      class="password-reveal-btn date-trigger-btn {open ? 'is-open' : ''}"
      aria-label="Abrir calendario"
      aria-expanded={open}
      aria-controls={popoverId}
      onclick={toggleCalendario}
    >
      <Calendar size={14} strokeWidth={2} />
    </button>
  </div>

  {#if open}
    <div
      id={popoverId}
      class="calendar-popover"
      role="dialog"
      tabindex="-1"
      aria-label="Calendario de fecha de audiencia"
      onkeydown={manejarKeydown}
      onwheel={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div class="calendar-header">
        <button
          type="button"
          class="mini-icon-btn"
          aria-label={ariaNavAnterior}
          disabled={navAnteriorDeshabilitado}
          onclick={() => navegarEncabezado(-1)}
        >
          <ChevronLeft size={15} />
        </button>

        <div class="calendar-title-group">
          <button
            type="button"
            class="calendar-select-trigger {vistaCalendario === 'meses' ? 'is-open' : ''}"
            aria-pressed={vistaCalendario === "meses"}
            title="Clic para abrir rejilla de meses · Rueda del ratón para cambiar de mes"
            onwheel={manejarWheelMes}
            onclick={(e) => toggleVista("meses", e)}
          >
            <span>{MESES[viewMonth]}</span>
            <ChevronDown size={13} strokeWidth={2} />
          </button>

          <button
            type="button"
            class="calendar-select-trigger is-year {vistaCalendario === 'anios' ? 'is-open' : ''}"
            aria-pressed={vistaCalendario === "anios"}
            title="Clic para abrir rejilla de años · Rueda del ratón para cambiar de año"
            onwheel={manejarWheelAnio}
            onclick={(e) => toggleVista("anios", e)}
          >
            <span>{viewYear}</span>
            <ChevronDown size={13} strokeWidth={2} />
          </button>
        </div>

        <button
          type="button"
          class="mini-icon-btn"
          aria-label={ariaNavSiguiente}
          disabled={navSiguienteDeshabilitado}
          onclick={() => navegarEncabezado(1)}
        >
          <ChevronRight size={15} />
        </button>
      </div>

      {#if vistaCalendario === "meses"}
        <div class="calendar-subview-bar">
          <span class="calendar-subview-label">Seleccionar mes · {viewYear}</span>
          <button
            type="button"
            class="inline-action-link"
            onclick={() => (vistaCalendario = "dias")}
          >
            Volver
          </button>
        </div>
        <div
          class="calendar-months-matrix"
          role="grid"
          aria-label="Meses del año"
          onwheel={manejarWheelMes}
        >
          {#each MESES_CORTOS as nombreCorto, idx}
            {@const esMesActual = hoy.getFullYear() === viewYear && hoy.getMonth() === idx}
            <button
              type="button"
              class="calendar-cell-btn {esMesActual ? 'is-today' : ''} {viewMonth === idx ? 'is-selected' : ''}"
              aria-label={MESES[idx]}
              aria-pressed={viewMonth === idx}
              title={MESES[idx]}
              onclick={() => {
                viewMonth = idx;
                vistaCalendario = "dias";
              }}
            >
              {nombreCorto}
            </button>
          {/each}
        </div>
      {:else if vistaCalendario === "anios"}
        <div class="calendar-subview-bar">
          <span class="calendar-subview-label">{yearPageStart} – {anioFinPagina}</span>
          <button
            type="button"
            class="inline-action-link"
            onclick={() => (vistaCalendario = "dias")}
          >
            Volver
          </button>
        </div>
        <div
          class="calendar-years-matrix"
          role="grid"
          aria-label="Años disponibles"
          onwheel={manejarWheelPaginaAnios}
        >
          {#each aniosPagina as anio}
            {@const esAnioActual = hoy.getFullYear() === anio}
            <button
              type="button"
              class="calendar-cell-btn {esAnioActual ? 'is-today' : ''} {viewYear === anio ? 'is-selected' : ''}"
              aria-pressed={viewYear === anio}
              onclick={() => {
                viewYear = anio;
                vistaCalendario = "dias";
              }}
            >
              {anio}
            </button>
          {/each}
        </div>
      {:else}
        <div class="calendar-weekdays" aria-hidden="true">
          {#each DIAS_SEMANA as d}
            <span class="calendar-weekday">{d}</span>
          {/each}
        </div>

        <div class="calendar-grid" onwheel={manejarWheelMes}>
          {#each celdasCalendario as celda}
            {@const esSeleccionada = value === celda.fechaStr}
            {@const esHoy = fechaHoyStr === celda.fechaStr}
            <button
              type="button"
              class="calendar-day {celda.otroMes ? 'is-outside' : ''} {esHoy ? 'is-today' : ''} {esSeleccionada ? 'is-selected' : ''}"
              onclick={() => seleccionarFecha(celda.fechaStr)}
              title={celda.conteo > 0
                ? `${celda.fechaStr} · ${celda.conteo} audiencia(s) en esta planilla`
                : celda.fechaStr}
            >
              <span>{celda.dia}</span>
              {#if celda.conteo > 0}
                <span class="calendar-day-dot" aria-hidden="true"></span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      <div class="calendar-footer">
        <button
          type="button"
          class="inline-action-link"
          onclick={() => {
            value = "";
            vistaCalendario = "dias";
            open = false;
          }}
        >
          Limpiar
        </button>
        <button type="button" class="inline-action-link" onclick={seleccionarHoy}>
          Hoy ({fechaHoyStr})
        </button>
      </div>
    </div>
  {/if}
</div>
