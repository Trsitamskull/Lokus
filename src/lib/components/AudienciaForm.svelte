<script>
  import {
    Sparkles,
    CheckCircle2,
    XCircle,
    Edit3,
    SlidersHorizontal,
    HelpCircle,
    Plus,
  } from "@lucide/svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import DatePickerInput from "./DatePickerInput.svelte";
  import {
    TIPOS_AUDIENCIA,
    MOTIVOS_LABELS,
    HORAS_RAPIDAS,
    JUZGADOS_BASE,
    normalizarClaveCatalogo,
  } from "../constants.js";

  let {
    archivoActual = null,
    registros = [],
    juzgadosGuardados = [],
    tiposGuardados = [],
    totales,
    aiStatus,
    textoIa = $bindable(""),
    procesandoIa = false,
    errorIa = "",
    modoEdicion = false,
    filaEditando = null,
    numeroEditando = null,
    radicado = $bindable(""),
    juzgadoSeleccionado = $bindable(""),
    juzgadoOtra = $bindable(""),
    tipoSeleccionado = $bindable(""),
    tipoOtra = $bindable(""),
    realizada = $bindable(""),
    fecha = $bindable(""),
    hora = $bindable(""),
    minuto = $bindable(""),
    motivoSeleccionado = $bindable(""),
    observaciones = $bindable(""),
    inputRadicadoEl = $bindable(null),
    formResetTick = 0,
    onSubmit,
    onCancelarEdicion,
    onLimpiar,
    onProcesarConIa,
    onAbrirConfigIa,
    onVerTodos,
    onEditarRegistro,
    onAgregarJuzgado,
    onEliminarJuzgado,
    onAgregarTipo,
    onEliminarTipo,
    aclaracionPendiente = null,
    respondiendoAclaracion = false,
    onResponderAclaracion,
    onOmitirAclaracion,
  } = $props();

  let iaTextareaEl = $state(null);
  let horaInputEl = $state(null);
  let minutoInputEl = $state(null);
  let amBtnEl = $state(null);
  let pmBtnEl = $state(null);
  let notaAclaracion = $state("");
  let clarifyInputEl = $state(null);
  let guardandoJuzgadoOtra = $state(false);
  let guardandoTipoOtra = $state(false);
  let juzgadoOtraInputEl = $state(null);
  let tipoOtraInputEl = $state(null);
  let fechaTouched = $state(false);
  let horaTouched = $state(false);
  let intentoEnviar = $state(false);
  let hora12 = $state("");
  let periodo = $state("AM");
  let ultimoHora24Sync = "";
  let ultimoResetTick = 0;
  let ultimoAclaracionId = "";

  const ETIQUETAS_CAMPO_IA = {
    realizada: "Realizada",
    motivo: "Motivo",
    juzgado: "Juzgado",
    tipo: "Tipo",
    observaciones: "Observaciones",
    radicado: "Radicado",
    fecha: "Fecha",
    hora: "Hora",
  };

  function etiquetaCampoIa(campo) {
    return ETIQUETAS_CAMPO_IA[campo] || campo;
  }

  function resumirValorDiff(val) {
    const limpio = (val || "").trim();
    if (!limpio) return "vacío";
    return limpio.length > 42 ? `${limpio.slice(0, 39).trimEnd()}...` : limpio;
  }

  $effect(() => {
    const idActual = aclaracionPendiente?.correccion_id || "";
    if (idActual !== ultimoAclaracionId) {
      ultimoAclaracionId = idActual;
      notaAclaracion = "";
    }
  });

  async function enviarNotaAclaracion() {
    if (respondiendoAclaracion) return;
    const texto = notaAclaracion.trim();
    if (!texto) return;
    const ok = await onResponderAclaracion?.(texto, false);
    if (ok !== false) {
      notaAclaracion = "";
    }
  }

  function enfocarCampoOtra(getInputEl, seleccionarNumero = true) {
    setTimeout(() => {
      const el = getInputEl();
      if (!el) return;
      el.focus();
      const val = el.value || "";
      if (seleccionarNumero) {
        const match = /\b\d+\b/.exec(val);
        if (match) {
          el.setSelectionRange(match.index, match.index + match[0].length);
          return;
        }
      }
      el.setSelectionRange(val.length, val.length);
    }, 15);
  }

  function usarJuzgadoComoPlantilla(nombre) {
    juzgadoSeleccionado = "Otra";
    juzgadoOtra = String(nombre || "").trim();
    enfocarCampoOtra(() => juzgadoOtraInputEl, true);
  }

  function usarTipoComoPlantilla(nombre) {
    tipoSeleccionado = "Otra";
    tipoOtra = String(nombre || "").trim();
    enfocarCampoOtra(() => tipoOtraInputEl, true);
  }

  async function guardarJuzgadoDesdeOtra() {
    const limpio = juzgadoOtra.trim().replace(/\s+/g, " ");
    if (!limpio || guardandoJuzgadoOtra) return;
    guardandoJuzgadoOtra = true;
    try {
      const resultado = onAgregarJuzgado ? await onAgregarJuzgado(limpio) : limpio;
      if (resultado === null || resultado === false) return;
      juzgadoSeleccionado =
        typeof resultado === "string" && resultado.trim() ? resultado.trim() : limpio;
      juzgadoOtra = "";
    } finally {
      guardandoJuzgadoOtra = false;
    }
  }

  async function guardarTipoDesdeOtra() {
    const limpio = tipoOtra.trim().replace(/\s+/g, " ");
    if (!limpio || guardandoTipoOtra) return;
    guardandoTipoOtra = true;
    try {
      const resultado = onAgregarTipo ? await onAgregarTipo(limpio) : limpio;
      if (resultado === null || resultado === false) return;
      tipoSeleccionado =
        typeof resultado === "string" && resultado.trim() ? resultado.trim() : limpio;
      tipoOtra = "";
    } finally {
      guardandoTipoOtra = false;
    }
  }

  async function marcarDatoExternoAclaracion() {
    if (respondiendoAclaracion) return;
    const ok = await onResponderAclaracion?.("Dato externo (no estaba en el texto)", true);
    if (ok !== false) {
      notaAclaracion = "";
    }
  }

  function convertir24a12(h24Str) {
    const limpio = (h24Str || "").trim();
    if (!limpio || !/^\d{1,2}$/.test(limpio)) return null;
    const h = Number(limpio);
    if (h < 0 || h > 23) return null;
    if (h === 0) return { hora12: "12", periodo: "AM" };
    if (h < 12) return { hora12: String(h).padStart(2, "0"), periodo: "AM" };
    if (h === 12) return { hora12: "12", periodo: "PM" };
    return { hora12: String(h - 12).padStart(2, "0"), periodo: "PM" };
  }

  function convertir12a24(h12Str, per) {
    const limpio = (h12Str || "").trim();
    if (!limpio || !/^\d{1,2}$/.test(limpio)) return "";
    const h = Number(limpio);
    if (h < 1 || h > 12) return "";
    if (per === "AM") return String(h === 12 ? 0 : h).padStart(2, "0");
    return String(h === 12 ? 12 : h + 12).padStart(2, "0");
  }

  $effect(() => {
    if (formResetTick !== ultimoResetTick) {
      ultimoResetTick = formResetTick;
      const propHora = hora;
      ultimoHora24Sync = propHora;
      if (!propHora.trim()) {
        hora12 = "";
        periodo = "AM";
      } else {
        const conv = convertir24a12(propHora);
        if (conv) {
          hora12 = conv.hora12;
          periodo = conv.periodo;
        }
      }
      fechaTouched = false;
      horaTouched = false;
      intentoEnviar = false;
    }
  });

  $effect(() => {
    const propHora = hora;
    if (propHora === ultimoHora24Sync) return;
    ultimoHora24Sync = propHora;
    if (!propHora.trim()) {
      hora12 = "";
      periodo = "AM";
      return;
    }
    const conv = convertir24a12(propHora);
    if (conv) {
      hora12 = conv.hora12;
      periodo = conv.periodo;
    }
  });

  $effect(() => {
    if (!radicado && !fecha && !hora && !minuto && !realizada && !motivoSeleccionado) {
      fechaTouched = false;
      horaTouched = false;
      intentoEnviar = false;
    }
  });

  function esFechaCalendarioValida(str) {
    const m = /^(\d{2})\/(\d{2})\/(\d{4})$/.exec((str || "").trim());
    if (!m) return false;
    const dia = Number(m[1]);
    const mes = Number(m[2]);
    const anio = Number(m[3]);
    if (anio < 2018 || anio > 2050 || mes < 1 || mes > 12 || dia < 1 || dia > 31) {
      return false;
    }
    const obj = new Date(anio, mes - 1, dia);
    return (
      obj.getFullYear() === anio &&
      obj.getMonth() === mes - 1 &&
      obj.getDate() === dia
    );
  }

  let errorFecha = $derived.by(() => {
    if (!(fechaTouched || intentoEnviar)) return "";
    if (!fecha.trim()) return "";
    return esFechaCalendarioValida(fecha)
      ? ""
      : "Ingresa una fecha válida en formato DD/MM/AAAA (2018–2050).";
  });

  let errorHora = $derived.by(() => {
    if (!(horaTouched || intentoEnviar)) return "";
    const h12 = hora12.trim();
    const m = minuto.trim();
    if (!h12 && !m) return "";
    if (!h12 && m) return "Indica la hora (01 a 12) junto con los minutos.";
    if (!/^\d{1,2}$/.test(h12) || Number(h12) < 1 || Number(h12) > 12) {
      return "La hora debe estar entre 01 y 12.";
    }
    if (m && (!/^\d{1,2}$/.test(m) || Number(m) > 59)) {
      return "Los minutos deben estar entre 00 y 59.";
    }
    return "";
  });

  let errorRealizada = $derived(
    intentoEnviar && !realizada ? "Selecciona si la audiencia se realizó o no." : ""
  );

  let errorMotivo = $derived(
    intentoEnviar && realizada === "NO" && !motivoSeleccionado
      ? "Selecciona la causal de aplazamiento."
      : ""
  );

  function sincronizarHora24Desde12(h12Str = hora12, per = periodo) {
    const h24 = convertir12a24(h12Str, per);
    ultimoHora24Sync = h24;
    hora = h24;
  }

  function manejarHoraInput(e) {
    const limpio = e.currentTarget.value.replace(/\D/g, "").slice(0, 2);
    e.currentTarget.value = limpio;
    hora12 = limpio;
    sincronizarHora24Desde12(limpio, periodo);
    if (
      limpio.length === 2 &&
      Number(limpio) >= 1 &&
      Number(limpio) <= 12 &&
      (e.inputType || "").startsWith("insert")
    ) {
      minutoInputEl?.focus();
      minutoInputEl?.select();
    }
  }

  function manejarMinutoInput(e) {
    const limpio = e.currentTarget.value.replace(/\D/g, "").slice(0, 2);
    e.currentTarget.value = limpio;
    minuto = limpio;
  }

  function manejarTeclaHora(e) {
    const el = e.currentTarget;
    if (
      e.key === "ArrowRight" &&
      el.selectionStart === el.selectionEnd &&
      el.selectionStart === el.value.length
    ) {
      e.preventDefault();
      minutoInputEl?.focus();
      minutoInputEl?.setSelectionRange(0, 0);
    }
  }

  function manejarTeclaMinuto(e) {
    const el = e.currentTarget;
    if (
      e.key === "ArrowLeft" &&
      el.selectionStart === el.selectionEnd &&
      el.selectionStart === 0
    ) {
      e.preventDefault();
      horaInputEl?.focus();
      const len = horaInputEl?.value.length || 0;
      horaInputEl?.setSelectionRange(len, len);
    } else if (
      e.key === "ArrowRight" &&
      el.selectionStart === el.selectionEnd &&
      el.selectionStart === el.value.length
    ) {
      e.preventDefault();
      (periodo === "PM" ? pmBtnEl : amBtnEl)?.focus();
    } else if (e.key === "Backspace" && el.value === "") {
      e.preventDefault();
      horaInputEl?.focus();
      const len = horaInputEl?.value.length || 0;
      horaInputEl?.setSelectionRange(len, len);
    }
  }

  function manejarTeclaMeridiem(e, actual) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      if (actual === "PM") {
        amBtnEl?.focus();
      } else {
        minutoInputEl?.focus();
        const len = minutoInputEl?.value.length || 0;
        minutoInputEl?.setSelectionRange(len, len);
      }
    } else if (e.key === "ArrowRight" && actual === "AM") {
      e.preventDefault();
      pmBtnEl?.focus();
    }
  }

  function seleccionarPeriodo(nuevoPeriodo) {
    periodo = nuevoPeriodo;
    sincronizarHora24Desde12(hora12, nuevoPeriodo);
    if (hora12.trim() || minuto.trim()) horaTouched = true;
  }

  function aplicarHoraRapida(preset) {
    hora12 = preset.h12;
    minuto = preset.m;
    periodo = preset.periodo;
    ultimoHora24Sync = preset.h24;
    hora = preset.h24;
    horaTouched = true;
  }

  function manejarBlurCajaHora(e) {
    if (e.currentTarget.contains(e.relatedTarget)) return;
    if (hora12.trim().length === 1) {
      hora12 = hora12.trim().padStart(2, "0");
    }
    sincronizarHora24Desde12(hora12, periodo);
    if (minuto.trim().length === 1) minuto = minuto.trim().padStart(2, "0");
    horaTouched = true;
  }

  function manejarSubmitFormulario(e) {
    e.preventDefault();
    intentoEnviar = true;
    if (errorFecha) {
      document.getElementById("fecha")?.focus();
      return;
    }
    if (errorHora) {
      horaInputEl?.focus();
      return;
    }
    if (errorRealizada || errorMotivo) {
      return;
    }
    onSubmit();
  }

  let opcionesJuzgado = $derived.by(() => {
    const vistos = new Set();
    const lista = [
      { value: "", label: "Sin especificar (dejar vacío)", templateable: false },
      {
        value: "Otra",
        label: "Otra (escribir nuevo o usar solo aquí)",
        templateable: false,
      },
    ];
    for (const j of juzgadosGuardados) {
      const k = normalizarClaveCatalogo(j);
      if (k && k !== "otra" && !vistos.has(k)) {
        vistos.add(k);
        lista.push({ value: j.trim(), label: j.trim(), deletable: true, templateable: true });
      }
    }
    for (const j of JUZGADOS_BASE) {
      const k = normalizarClaveCatalogo(j);
      if (k && k !== "otra" && !vistos.has(k)) {
        vistos.add(k);
        lista.push({ value: j.trim(), label: j.trim(), deletable: false, templateable: true });
      }
    }
    return lista;
  });

  let opcionesTipoAudiencia = $derived.by(() => {
    const vistos = new Set();
    const lista = [
      { value: "", label: "Sin especificar (dejar vacío)", templateable: false },
      {
        value: "Otra",
        label: "Otra (escribir nuevo o usar solo aquí)",
        templateable: false,
      },
    ];
    for (const t of tiposGuardados) {
      const k = normalizarClaveCatalogo(t);
      if (k && k !== "otra" && !vistos.has(k)) {
        vistos.add(k);
        lista.push({ value: t.trim(), label: t.trim(), deletable: true, templateable: true });
      }
    }
    for (const t of TIPOS_AUDIENCIA) {
      const k = normalizarClaveCatalogo(t);
      if (k && k !== "otra" && !vistos.has(k)) {
        vistos.add(k);
        lista.push({ value: t.trim(), label: t.trim(), deletable: false, templateable: true });
      }
    }
    return lista;
  });

  function ponerFechaHoy() {
    const hoy = new Date();
    const d = String(hoy.getDate()).padStart(2, "0");
    const m = String(hoy.getMonth() + 1).padStart(2, "0");
    const y = hoy.getFullYear();
    fecha = `${d}/${m}/${y}`;
  }

  async function manejarExtraer() {
    if (!textoIa.trim()) {
      iaTextareaEl?.focus();
    }
    await onProcesarConIa();
  }
</script>

<div class="dossier-stack">
  <!-- FORMULARIO PRINCIPAL AMPLIO -->
  <form
    class="dossier-form-card"
    onsubmit={manejarSubmitFormulario}
  >
    <div class="dossier-card-head">
      <div>
        <h1 class="dossier-title">
          {modoEdicion ? `Editando registro #${numeroEditando}` : "Nueva Audiencia Judicial"}
        </h1>
        <p class="dossier-subtitle">
          {archivoActual
            ? "Los registros se ordenan automáticamente por fecha y hora al guardar"
            : "Selecciona o crea una planilla en el panel izquierdo para comenzar"}
        </p>
      </div>
    </div>

    {#if modoEdicion}
      <div class="dossier-banner-edit">
        <span>Editando registro #{numeroEditando}</span>
        <div class="action-with-shortcut">
          <span class="shortcut-hint">Esc</span>
          <button type="button" class="btn btn-subtle" onclick={onCancelarEdicion}>
            Cancelar edición
          </button>
        </div>
      </div>
    {:else if aiStatus.configured}
      <!-- CAJA DE TEXTO DE IA SIEMPRE VISIBLE ARRIBA -->
      <div class="extract-panel">
        <div class="extract-panel-head">
          <div class="extract-panel-title-wrap">
            <label for="ia-inline-textarea" class="field-label extract-label-ia">
              Texto de la audiencia (IA)
            </label>
            <span class="shortcut-hint">
              Pega la nota o el acta para rellenar el formulario
            </span>
          </div>
          <button
            type="button"
            class="mini-icon-btn"
            onclick={onAbrirConfigIa}
            title="Configurar modelo ({aiStatus.model})"
            aria-label="Configurar modelo de IA"
          >
            <SlidersHorizontal size={14} />
          </button>
        </div>

        <textarea
          id="ia-inline-textarea"
          bind:this={iaTextareaEl}
          class="textarea"
          rows="3"
          bind:value={textoIa}
          placeholder="Pega aquí la nota de WhatsApp, correo o acta de la audiencia..."
          onkeydown={(e) => {
            if (e.ctrlKey && e.key === "Enter") {
              e.preventDefault();
              manejarExtraer();
            }
          }}
        ></textarea>
        {#if errorIa}
          <p class="field-error-msg">{errorIa}</p>
        {/if}

        <div class="extract-panel-actions">
          <div class="action-with-shortcut">
            <button
              type="button"
              class="btn btn-ia-action"
              disabled={procesandoIa}
              onclick={manejarExtraer}
              title="Extraer datos del texto al formulario ({aiStatus.model})"
            >
              <Sparkles size={14} strokeWidth={2.2} />
              <span>{procesandoIa ? "Extrayendo..." : "Autocompletar con IA"}</span>
            </button>
            <span class="shortcut-hint">Ctrl + Enter</span>
          </div>
          {#if textoIa.trim()}
            <button
              type="button"
              class="inline-action-link"
              disabled={procesandoIa}
              onclick={() => (textoIa = "")}
            >
              Borrar texto
            </button>
          {/if}
        </div>
      </div>
    {:else}
      <!-- AVISO SOLO CUANDO FALTA CONFIGURAR LA API DE IA -->
      <div class="extract-panel is-unconfigured">
        <div class="extract-panel-head">
          <span style="font-size: 12.5px; font-weight: 600; color: var(--accent-warm)">
            Configura una clave de OpenAI o Gemini para habilitar el autocompletado con IA
          </span>
          <button type="button" class="btn btn-ai-warm" onclick={onAbrirConfigIa}>
            Configurar API Key
          </button>
        </div>
      </div>
    {/if}

    {#if !modoEdicion && aclaracionPendiente}
      <div
        class="extract-panel ia-clarify-panel"
        role="region"
        aria-label="Aclaración de aprendizaje de la IA"
      >
        <div class="extract-panel-head">
          <div class="ia-clarify-title-wrap">
            <HelpCircle size={15} class="ia-clarify-icon" aria-hidden="true" />
            <span class="field-label ia-clarify-question">
              {aclaracionPendiente.pregunta}
            </span>
          </div>
          <button
            type="button"
            class="btn btn-subtle ia-clarify-skip-btn"
            disabled={respondiendoAclaracion}
            onclick={() => {
              notaAclaracion = "";
              onOmitirAclaracion?.();
            }}
          >
            Omitir
          </button>
        </div>

        {#if aclaracionPendiente.campos?.length > 0}
          <div class="ia-clarify-diff-strip">
            {#each aclaracionPendiente.campos.slice(0, 3) as c}
              <span
                class="ia-diag-pill ia-clarify-diff-pill"
                title={`${etiquetaCampoIa(c.campo)}: ${c.valor_ia || "vacío"} → ${c.valor_usuario || "vacío"}`}
              >
                <strong>{etiquetaCampoIa(c.campo)}:</strong>
                <span>{resumirValorDiff(c.valor_ia)} &rarr; {resumirValorDiff(c.valor_usuario)}</span>
              </span>
            {/each}
          </div>
        {/if}

        <div class="ia-clarify-chips" role="group" aria-label="Respuestas rápidas de aclaración">
          <button
            type="button"
            class="preset-btn ia-clarify-chip"
            disabled={respondiendoAclaracion}
            onclick={marcarDatoExternoAclaracion}
          >
            Dato externo (no estaba en el texto)
          </button>
          <button
            type="button"
            class="preset-btn ia-clarify-chip"
            disabled={respondiendoAclaracion}
            onclick={() => {
              notaAclaracion = "Criterio habitual de este defensor: ";
              clarifyInputEl?.focus();
            }}
          >
            Criterio de este defensor
          </button>
          <button
            type="button"
            class="preset-btn ia-clarify-chip"
            disabled={respondiendoAclaracion}
            onclick={() => {
              notaAclaracion = "Abreviación habitual: ";
              clarifyInputEl?.focus();
            }}
          >
            Sigla / abreviación habitual
          </button>
        </div>

        <div class="ia-clarify-row">
          <input
            type="text"
            class="input"
            bind:this={clarifyInputEl}
            bind:value={notaAclaracion}
            disabled={respondiendoAclaracion}
            placeholder="Nota breve opcional para recordarlo en próximas extracciones..."
            onkeydown={(e) => {
              if (!e.isComposing && e.key === "Enter") {
                e.preventDefault();
                enviarNotaAclaracion();
              }
            }}
          />
          <button
            type="button"
            class="btn btn-solid"
            disabled={respondiendoAclaracion || !notaAclaracion.trim()}
            onclick={enviarNotaAclaracion}
          >
            <span>{respondiendoAclaracion && notaAclaracion.trim() ? "Guardando..." : "Guardar regla"}</span>
          </button>
        </div>
      </div>
    {/if}

    <!-- FILA 1: RADICADO AMPLIO -->
    <div class="field-group">
      <div class="field-header">
        <label class="field-label" for="radicado">Radicado o Procesado</label>
      </div>
      <input
        id="radicado"
        name="radicado"
        type="text"
        class="input"
        bind:this={inputRadicadoEl}
        bind:value={radicado}
        autocorrect="off"
        autocapitalize="none"
        spellcheck="false"
        placeholder="Ej: JUAN PEREZ - 11001600000020250000100"
      />
    </div>

    <!-- FILA 2: DESPACHO Y TIPO DE AUDIENCIA -->
    <div class="dossier-row-2col">
      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="juzgado">Juzgado o Despacho Judicial</label>
          <span class="shortcut-hint">Opcional</span>
        </div>
        <CustomSelect
          id="juzgado"
          bind:value={juzgadoSeleccionado}
          options={opcionesJuzgado}
          placeholder="Sin especificar (opcional)..."
          searchPlaceholder="Filtrar por municipio o juzgado (ej: Valledupar 2 Circuito)..."
          templateHint='Pulsa copiar en cualquier opción para usarla como plantilla en "Otra"'
          onUseAsTemplate={usarJuzgadoComoPlantilla}
          onDeleteOption={onEliminarJuzgado}
          onchange={(v) => {
            if (v === "Otra") enfocarCampoOtra(() => juzgadoOtraInputEl, false);
          }}
        />
        {#if juzgadoSeleccionado === "Otra"}
          <div class="otra-inline-block">
            <div class="otra-inline-row">
              <input
                id="juzgado-otra"
                type="text"
                class="input"
                bind:this={juzgadoOtraInputEl}
                bind:value={juzgadoOtra}
                placeholder="Escribe o edita el juzgado..."
              />
              <button
                type="button"
                class="btn btn-subtle otra-add-btn"
                disabled={!juzgadoOtra.trim() || guardandoJuzgadoOtra}
                title="Guardar en mi lista permanente y seleccionarlo"
                aria-label="Guardar juzgado en la lista"
                onclick={guardarJuzgadoDesdeOtra}
              >
                <Plus size={15} strokeWidth={2.2} />
              </button>
            </div>
            <span class="shortcut-hint">
              Solo para esta audiencia, o pulsa + para guardarlo en tu lista.
            </span>
          </div>
        {/if}
      </div>

      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="tipo-audiencia">Tipo de Audiencia</label>
          <span class="shortcut-hint">Opcional</span>
        </div>
        <CustomSelect
          id="tipo-audiencia"
          bind:value={tipoSeleccionado}
          options={opcionesTipoAudiencia}
          placeholder="Sin especificar (opcional)..."
          searchPlaceholder="Filtrar tipo de audiencia..."
          templateHint='Pulsa copiar en cualquier opción para usarla como plantilla en "Otra"'
          onUseAsTemplate={usarTipoComoPlantilla}
          onDeleteOption={onEliminarTipo}
          onchange={(v) => {
            if (v === "Otra") enfocarCampoOtra(() => tipoOtraInputEl, false);
          }}
        />
        {#if tipoSeleccionado === "Otra"}
          <div class="otra-inline-block">
            <div class="otra-inline-row">
              <input
                id="tipo-otra"
                type="text"
                class="input"
                bind:this={tipoOtraInputEl}
                bind:value={tipoOtra}
                placeholder="Escribe o edita el tipo de audiencia..."
              />
              <button
                type="button"
                class="btn btn-subtle otra-add-btn"
                disabled={!tipoOtra.trim() || guardandoTipoOtra}
                title="Guardar en mi lista permanente y seleccionarlo"
                aria-label="Guardar tipo de audiencia en la lista"
                onclick={guardarTipoDesdeOtra}
              >
                <Plus size={15} strokeWidth={2.2} />
              </button>
            </div>
            <span class="shortcut-hint">
              Solo para esta audiencia, o pulsa + para guardarlo en tu lista.
            </span>
          </div>
        {/if}
      </div>
    </div>

    <!-- FILA 3: FECHA Y HORA -->
    <div class="dossier-row-2col">
      <div class="field-group">
        <div class="field-header">
          <label class="field-label" for="fecha">Fecha</label>
          <div class="field-header">
            <span class="shortcut-hint">Opcional</span>
            <button type="button" class="inline-action-link" onclick={() => { ponerFechaHoy(); fechaTouched = true; }}>
              Hoy
            </button>
          </div>
        </div>
        <DatePickerInput
          id="fecha"
          name="fecha"
          bind:value={fecha}
          {registros}
          invalid={Boolean(errorFecha)}
          describedBy={errorFecha ? "fecha-error-msg" : undefined}
          onBlur={() => (fechaTouched = true)}
          placeholder="DD/MM/AAAA (opcional)"
        />
        {#if errorFecha}
          <span id="fecha-error-msg" class="field-error-msg">{errorFecha}</span>
        {/if}
      </div>

      <div class="field-group time-field-group">
        <div class="field-header">
          <label class="field-label" for="hora">Hora</label>
          <span class="shortcut-hint">12h · Opcional</span>
        </div>
        <div class="time-inline-box {errorHora ? 'is-invalid' : ''}" onfocusout={manejarBlurCajaHora}>
          <input
            id="hora"
            type="text"
            inputmode="numeric"
            maxlength="2"
            class="time-part-input"
            bind:this={horaInputEl}
            value={hora12}
            oninput={manejarHoraInput}
            onkeydown={manejarTeclaHora}
            onclick={(e) => e.currentTarget.select()}
            aria-invalid={errorHora ? "true" : "false"}
            aria-describedby={errorHora ? "hora-error-msg" : undefined}
            placeholder="HH"
            aria-label="Hora (1 a 12)"
          />
          <span class="time-sep">:</span>
          <input
            id="minuto"
            type="text"
            inputmode="numeric"
            maxlength="2"
            class="time-part-input"
            bind:this={minutoInputEl}
            value={minuto}
            oninput={manejarMinutoInput}
            onkeydown={manejarTeclaMinuto}
            onclick={(e) => e.currentTarget.select()}
            aria-invalid={errorHora ? "true" : "false"}
            aria-describedby={errorHora ? "hora-error-msg" : undefined}
            placeholder="MM"
            aria-label="Minutos"
          />
          <div class="time-meridiem-group" role="group" aria-label="Período AM o PM">
            <button
              type="button"
              bind:this={amBtnEl}
              class="time-meridiem-btn {periodo === 'AM' ? 'is-active' : ''}"
              aria-pressed={periodo === "AM"}
              onclick={() => seleccionarPeriodo("AM")}
              onkeydown={(e) => manejarTeclaMeridiem(e, "AM")}
            >
              AM
            </button>
            <button
              type="button"
              bind:this={pmBtnEl}
              class="time-meridiem-btn {periodo === 'PM' ? 'is-active' : ''}"
              aria-pressed={periodo === "PM"}
              onclick={() => seleccionarPeriodo("PM")}
              onkeydown={(e) => manejarTeclaMeridiem(e, "PM")}
            >
              PM
            </button>
          </div>
        </div>
        <div class="time-presets" role="group" aria-label="Accesos rápidos de hora">
          {#each HORAS_RAPIDAS as preset}
            <button
              type="button"
              class="preset-btn {hora === preset.h24 && minuto === preset.m ? 'active' : ''}"
              aria-pressed={hora === preset.h24 && minuto === preset.m}
              onclick={() => aplicarHoraRapida(preset)}
            >
              {preset.label}
            </button>
          {/each}
        </div>
        {#if errorHora}
          <span id="hora-error-msg" class="field-error-msg">{errorHora}</span>
        {/if}
      </div>
    </div>

    <div class="dossier-divider"></div>

    <!-- FILA 3: ESTADO DE REALIZACIÓN -->
    <div class="field-group">
      <span class="field-label">Resultado de la diligencia</span>
      <div
        class="segmented-control {errorRealizada ? 'is-invalid' : ''}"
        role="radiogroup"
        aria-label="¿Se realizó la audiencia?"
      >
        <button
          type="button"
          role="radio"
          aria-checked={realizada === "SI"}
          class="segmented-option {realizada === 'SI' ? 'is-si' : ''}"
          onclick={() => {
            realizada = "SI";
            motivoSeleccionado = "";
          }}
        >
          <CheckCircle2 size={15} strokeWidth={2} />
          <span>Realizada (SI)</span>
        </button>

        <button
          type="button"
          role="radio"
          aria-checked={realizada === "NO"}
          class="segmented-option {realizada === 'NO' ? 'is-no' : ''}"
          onclick={() => {
            realizada = "NO";
          }}
        >
          <XCircle size={15} strokeWidth={2} />
          <span>No realizada / Aplazada (NO)</span>
        </button>
      </div>
      {#if errorRealizada}
        <span class="field-error-msg">{errorRealizada}</span>
      {/if}
    </div>

    <!-- FILA 4: CAUSAL -->
    {#if realizada === "NO"}
      <div class="motivos-drawer {errorMotivo ? 'is-invalid' : ''}">
        <div class="field-header">
          <span class="field-label">Causal atribuible a (seleccione una)</span>
          <span class="shortcut-hint">Causales de aplazamiento</span>
        </div>
        <div class="motivos-grid" role="radiogroup" aria-label="Causal de no realización">
          {#each MOTIVOS_LABELS as m, idx}
            {@const isSelected = motivoSeleccionado === m}
            <button
              type="button"
              role="radio"
              aria-checked={isSelected}
              class="motivo-item {isSelected ? 'selected' : ''}"
              onclick={() => {
                motivoSeleccionado = motivoSeleccionado === m ? "" : m;
              }}
            >
              <span>{m}</span>
              <span class="motivo-badge">{totales.totales_motivos[idx] ?? 0}</span>
            </button>
          {/each}
        </div>
        {#if errorMotivo}
          <span class="field-error-msg">{errorMotivo}</span>
        {/if}
      </div>
    {/if}

    <!-- FILA 5: OBSERVACIONES -->
    <div class="field-group">
      <div class="field-header">
        <label class="field-label" for="observaciones">Observaciones</label>
        <span class="shortcut-hint">Opcional</span>
      </div>
      <textarea
        id="observaciones"
        class="textarea"
        rows="3"
        bind:value={observaciones}
        placeholder="Constancias, causas secundarias o fecha de reprogramación..."
      ></textarea>
    </div>

    <div class="dossier-divider"></div>

    <!-- ACCIONES FINALES: LIMPIAR Y REGISTRAR A LA DERECHA -->
    <div class="dossier-actions">
      <div class="dossier-actions-right">
        {#if !modoEdicion}
          <div class="action-with-shortcut">
            <span class="shortcut-hint">Ctrl + L</span>
            <button type="button" class="btn btn-subtle" onclick={onLimpiar}>
              Limpiar
            </button>
          </div>
        {/if}
        <div class="action-with-shortcut">
          <span class="shortcut-hint">Ctrl + S</span>
          <button type="submit" class="btn btn-solid dossier-submit-btn">
            <span>{modoEdicion ? "Guardar cambios" : "Registrar audiencia"}</span>
          </button>
        </div>
      </div>
    </div>
  </form>

  <!-- ACTIVIDAD RECIENTE ABAJO -->
  {#if registros.length > 0}
    <section class="dossier-recent-card">
      <div class="field-header">
        <span class="field-label">Últimos registros en {archivoActual?.replace(/\.xlsx$/i, "")}</span>
        <button type="button" class="inline-action-link" onclick={onVerTodos}>
          Ver todos ({registros.length}) →
        </button>
      </div>

      <div class="recent-strip-list">
        {#each registros.slice(0, 3) as r}
          <div class="recent-strip-row">
            <div class="recent-strip-meta">
              <strong style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                #{r.numero} · {r.radicado}
              </strong>
              <span class="shortcut-hint" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                {r.fecha || "Sin fecha"} · {r.hora || "sin hora"}{r.tipo ? ` · ${r.tipo}` : ""}
              </span>
            </div>
            <div class="recent-strip-right">
              <span class={r.realizada === "SI" ? "tag-si" : "tag-no"}>
                {r.realizada}
              </span>
              <button
                type="button"
                class="mini-icon-btn"
                onclick={() => onEditarRegistro(r)}
                title="Editar este registro"
                aria-label="Editar este registro"
              >
                <Edit3 size={13} />
              </button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}
</div>
