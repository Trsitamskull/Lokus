<script>
  import { Calendar, ChevronLeft, ChevronRight, ChevronDown, Check } from "@lucide/svelte";

  let {
    id = "fecha",
    name = "fecha",
    value = $bindable(""),
    registros = [],
    placeholder = "DD/MM/AAAA",
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
  const ANIOS = Array.from({ length: 2050 - 2018 + 1 }, (_, i) => 2018 + i);
  const DIAS_SEMANA = ["Lu", "Ma", "Mi", "Ju", "Vi", "Sá", "Do"];

  let open = $state(false);
  let menuAbierto = $state(null); // "mes" | "anio" | null
  let yearListEl = $state(null);
  let containerEl = $state(null);
  let hoy = new Date();
  let viewYear = $state(hoy.getFullYear());
  let viewMonth = $state(hoy.getMonth()); // 0..11

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
      menuAbierto = null;
      open = true;
    }
  }

  function toggleCalendario(e) {
    e.stopPropagation();
    if (open) {
      menuAbierto = null;
      open = false;
    } else {
      sincronizarMesConValor();
      menuAbierto = null;
      open = true;
    }
  }

  function toggleSubmenu(tipo, e) {
    e.stopPropagation();
    menuAbierto = menuAbierto === tipo ? null : tipo;
    if (menuAbierto === "anio") {
      setTimeout(() => {
        const activo = yearListEl?.querySelector(".selected");
        activo?.scrollIntoView({ block: "center" });
      }, 10);
    }
  }

  function contenerWheelEnSublista(e) {
    const el = e.currentTarget;
    const enTopeSuperior = e.deltaY < 0 && el.scrollTop <= 0;
    const enTopeInferior =
      e.deltaY > 0 && el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
    if (enTopeSuperior || enTopeInferior) {
      e.preventDefault();
    }
    e.stopPropagation();
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

  function seleccionarFecha(fechaStr) {
    value = fechaStr;
    open = false;
  }

  function seleccionarHoy() {
    const d = new Date();
    viewYear = d.getFullYear();
    viewMonth = d.getMonth();
    value = fechaHoyStr;
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

    // Días del mes siguiente hasta completar 42 celdas (6 semanas)
    const restantes = 42 - celdas.length;
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
      open = false;
    } else if (e.key === "ArrowDown" && !open) {
      abrirCalendario();
    }
  }

  function handleWindowPointerDown(e) {
    if (open && containerEl && !containerEl.contains(e.target)) {
      menuAbierto = null;
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
      class="input has-trailing"
      {value}
      onfocus={abrirCalendario}
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
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      id={popoverId}
      class="calendar-popover"
      role="dialog"
      tabindex="-1"
      aria-label="Calendario de fecha de audiencia"
      onclick={() => (menuAbierto = null)}
      onwheel={(e) => {
        if (!e.target.closest(".calendar-dropdown-list")) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
    >
      <div class="calendar-header">
        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Mes anterior"
          onclick={() => {
            menuAbierto = null;
            cambiarMes(-1);
          }}
        >
          <ChevronLeft size={15} />
        </button>

        <div class="calendar-title-group">
          <div class="calendar-select-wrap">
            <button
              type="button"
              class="calendar-select-trigger {menuAbierto === 'mes' ? 'is-open' : ''}"
              aria-expanded={menuAbierto === "mes"}
              onclick={(e) => toggleSubmenu("mes", e)}
            >
              <span>{MESES[viewMonth]}</span>
              <ChevronDown size={13} strokeWidth={2} />
            </button>
            {#if menuAbierto === "mes"}
              <div
                class="calendar-dropdown-list scroller"
                role="listbox"
                aria-label="Seleccionar mes"
                onwheel={contenerWheelEnSublista}
              >
                {#each MESES as nombreMes, idx}
                  <button
                    type="button"
                    role="option"
                    aria-selected={viewMonth === idx}
                    class="custom-select-option {viewMonth === idx ? 'selected' : ''}"
                    onclick={(e) => {
                      e.stopPropagation();
                      viewMonth = idx;
                      menuAbierto = null;
                    }}
                  >
                    <span class="opt-text">{nombreMes}</span>
                    {#if viewMonth === idx}
                      <span class="opt-check">
                        <Check size={13} strokeWidth={2.5} />
                      </span>
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>

          <div class="calendar-select-wrap">
            <button
              type="button"
              class="calendar-select-trigger is-year {menuAbierto === 'anio' ? 'is-open' : ''}"
              aria-expanded={menuAbierto === "anio"}
              onclick={(e) => toggleSubmenu("anio", e)}
            >
              <span>{viewYear}</span>
              <ChevronDown size={13} strokeWidth={2} />
            </button>
            {#if menuAbierto === "anio"}
              <div
                bind:this={yearListEl}
                class="calendar-dropdown-list is-year scroller"
                role="listbox"
                aria-label="Seleccionar año"
                onwheel={contenerWheelEnSublista}
              >
                {#each ANIOS as anio}
                  <button
                    type="button"
                    role="option"
                    aria-selected={viewYear === anio}
                    class="custom-select-option {viewYear === anio ? 'selected' : ''}"
                    onclick={(e) => {
                      e.stopPropagation();
                      viewYear = anio;
                      menuAbierto = null;
                    }}
                  >
                    <span class="opt-text">{anio}</span>
                    {#if viewYear === anio}
                      <span class="opt-check">
                        <Check size={13} strokeWidth={2.5} />
                      </span>
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>

        <button
          type="button"
          class="mini-icon-btn"
          aria-label="Mes siguiente"
          onclick={() => {
            menuAbierto = null;
            cambiarMes(1);
          }}
        >
          <ChevronRight size={15} />
        </button>
      </div>

      <div class="calendar-weekdays" aria-hidden="true">
        {#each DIAS_SEMANA as d}
          <span class="calendar-weekday">{d}</span>
        {/each}
      </div>

      <div class="calendar-grid">
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

      <div class="calendar-footer">
        <button
          type="button"
          class="inline-action-link"
          onclick={() => {
            value = "";
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
