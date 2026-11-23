<script>
  import { Check, ChevronDown, Edit3, Trash2, X } from "@lucide/svelte";

  let {
    id = "",
    name = "",
    value = $bindable(""),
    suggestions = [],
    placeholder = "",
    invalid = false,
    autocorrect = "on",
    autocapitalize = "sentences",
    spellcheck = true,
    maxSuggestions = 12,
    allowEmptyOption = false,
    emptyOptionLabel = "Sin especificar (dejar vacío)",
    headerText = "",
    showDropdownButton = false,
    describedBy = undefined,
    inputEl = $bindable(null),
    onEnter = undefined,
    onSelect = undefined,
    onInput = undefined,
    onEditOption = undefined,
    onDeleteOption = undefined,
  } = $props();

  let open = $state(false);
  let mostrarTodas = $state(false);
  let highlightedIndex = $state(-1);
  let rootEl = $state(null);
  let editingItem = $state(null);
  let editDraft = $state("");
  let editError = $state("");
  let editInputEl = $state(null);
  let ultimoEditingFoco = null;
  let busyOption = $state(false);
  const fallbackId = `ac-${Math.random().toString(36).slice(2, 8)}`;
  let listboxId = $derived(`${id || fallbackId}-listbox`);

  $effect(() => {
    if (editingItem && editInputEl && ultimoEditingFoco !== editingItem) {
      ultimoEditingFoco = editingItem;
      editInputEl.focus();
      editInputEl.select?.();
    } else if (!editingItem) {
      ultimoEditingFoco = null;
    }
  });

  function normalizar(txt) {
    return String(txt || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .trim();
  }

  let filtradas = $derived.by(() => {
    const q = normalizar(value);
    const tokens = mostrarTodas || !q ? [] : q.split(/\s+/).filter(Boolean);
    const unicas = [];
    const vistas = new Set();

    for (const s of suggestions) {
      const clean = String(s || "").trim();
      if (!clean) continue;
      const key = normalizar(clean);
      if (vistas.has(key)) continue;
      const coincide =
        tokens.length === 0 || tokens.every((tok) => key.includes(tok));
      if (coincide) {
        vistas.add(key);
        unicas.push(clean);
        if (unicas.length >= maxSuggestions) break;
      }
    }

    if (
      !mostrarTodas &&
      unicas.length === 1 &&
      q &&
      normalizar(unicas[0]) === q
    ) {
      return [];
    }

    return unicas;
  });

  function limpiarEdicion() {
    editingItem = null;
    editDraft = "";
    editError = "";
  }

  function selectSuggestion(item) {
    limpiarEdicion();
    value = item;
    open = false;
    mostrarTodas = false;
    highlightedIndex = -1;
    onSelect?.(item);
  }

  function toggleDropdown() {
    if (suggestions.length === 0) return;
    if (open) {
      limpiarEdicion();
      open = false;
      mostrarTodas = false;
      highlightedIndex = -1;
    } else {
      limpiarEdicion();
      open = true;
      mostrarTodas = true;
      highlightedIndex = -1;
      inputEl?.focus();
    }
  }

  function iniciarEdicion(item) {
    if (busyOption) return;
    open = true;
    mostrarTodas = true;
    ultimoEditingFoco = null;
    editingItem = item;
    editDraft = item;
    editError = "";
    highlightedIndex = -1;
  }

  function cancelarEdicion() {
    limpiarEdicion();
    inputEl?.focus();
  }

  async function confirmarEdicion() {
    if (!editingItem || busyOption || !onEditOption) return;
    const actual = editingItem;
    const nuevo = editDraft.trim().replace(/\s+/g, " ");
    if (!nuevo) {
      editError = "El nombre no puede estar vacío.";
      return;
    }
    if (nuevo === actual) {
      limpiarEdicion();
      return;
    }
    busyOption = true;
    editError = "";
    try {
      const res = await onEditOption(actual, nuevo);
      if (res && res.ok) {
        const valorFinal = (res.valor || nuevo).trim();
        if (normalizar(value) === normalizar(actual)) {
          value = valorFinal;
        }
        limpiarEdicion();
        mostrarTodas = true;
        setTimeout(() => inputEl?.focus(), 10);
      } else {
        editError = res?.error || "No se pudo guardar el cambio.";
      }
    } catch (err) {
      editError = String(err);
    } finally {
      busyOption = false;
    }
  }

  async function confirmarEliminacion(item) {
    if (busyOption || !onDeleteOption) return;
    busyOption = true;
    editError = "";
    try {
      await onDeleteOption(item);
      if (editingItem && normalizar(editingItem) === normalizar(item)) {
        limpiarEdicion();
      }
      if (normalizar(value) === normalizar(item)) {
        value = "";
      }
      const quedan = suggestions.some((s) => normalizar(s) !== normalizar(item));
      if (!quedan) {
        open = false;
        mostrarTodas = false;
      } else {
        mostrarTodas = true;
      }
    } finally {
      busyOption = false;
    }
  }

  function bloquearCancelDialogo() {
    const dlg = rootEl?.closest("dialog");
    if (dlg) {
      dlg.dataset.blockCancelUntil = String(Date.now() + 150);
    }
  }

  function handleKeydown(e) {
    if (e.isComposing) return;

    if (e.key === "Escape" && open && (filtradas.length > 0 || (allowEmptyOption && value.trim() !== ""))) {
      e.preventDefault();
      e.stopPropagation();
      bloquearCancelDialogo();
      limpiarEdicion();
      open = false;
      mostrarTodas = false;
      highlightedIndex = -1;
      return;
    }

    if (e.key === "ArrowDown" && filtradas.length > 0) {
      e.preventDefault();
      open = true;
      highlightedIndex =
        highlightedIndex < filtradas.length - 1 ? highlightedIndex + 1 : 0;
      return;
    }

    if (e.key === "ArrowUp" && open && filtradas.length > 0) {
      e.preventDefault();
      highlightedIndex =
        highlightedIndex > 0 ? highlightedIndex - 1 : filtradas.length - 1;
      return;
    }

    if (e.key === "Enter") {
      if (open && highlightedIndex >= 0 && filtradas[highlightedIndex]) {
        e.preventDefault();
        e.stopPropagation();
        selectSuggestion(filtradas[highlightedIndex]);
      } else if (onEnter) {
        e.preventDefault();
        limpiarEdicion();
        open = false;
        mostrarTodas = false;
        onEnter();
      }
    }
  }

  function handleWindowPointerDown(e) {
    if (open && rootEl && !rootEl.contains(e.target)) {
      limpiarEdicion();
      open = false;
      mostrarTodas = false;
      highlightedIndex = -1;
    }
  }

  function contenerWheelEnLista(e) {
    const el = e.currentTarget;
    const enTopeSuperior = e.deltaY < 0 && el.scrollTop <= 0;
    const enTopeInferior =
      e.deltaY > 0 && el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
    if (enTopeSuperior || enTopeInferior) {
      e.preventDefault();
    }
    e.stopPropagation();
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="autocomplete-root" bind:this={rootEl}>
  <div class="autocomplete-input-wrap">
    <input
      {id}
      {name}
      type="text"
      class="input {invalid ? 'is-invalid' : ''} {showDropdownButton && suggestions.length > 0 ? 'has-toggle' : ''}"
      bind:this={inputEl}
      bind:value
      {placeholder}
      autocomplete="off"
      {autocorrect}
      {autocapitalize}
      {spellcheck}
      role="combobox"
      aria-autocomplete="list"
      aria-controls={listboxId}
      aria-describedby={describedBy}
      aria-expanded={open && (filtradas.length > 0 || (allowEmptyOption && value.trim() !== ""))}
      aria-activedescendant={open && highlightedIndex >= 0 ? `${listboxId}-opt-${highlightedIndex}` : undefined}
      onfocus={() => {
        if (suggestions.length > 0) open = true;
      }}
      oninput={(e) => {
        open = true;
        mostrarTodas = false;
        highlightedIndex = -1;
        onInput?.(e.currentTarget.value);
      }}
      onkeydown={handleKeydown}
    />

    {#if showDropdownButton && suggestions.length > 0}
      <button
        type="button"
        class="autocomplete-toggle-btn"
        aria-label="Mostrar opciones guardadas"
        aria-expanded={open && (filtradas.length > 0 || (allowEmptyOption && value.trim() !== ""))}
        onpointerdown={(e) => {
          e.preventDefault();
          toggleDropdown();
        }}
        onclick={(e) => {
          // Soporte de activación por teclado (Enter/Space) cuando el foco está en el botón
          if (e.detail === 0) toggleDropdown();
        }}
      >
        <ChevronDown size={15} class={open ? "is-rotated" : ""} />
      </button>
    {/if}
  </div>

  {#if open && (filtradas.length > 0 || (allowEmptyOption && value.trim() !== ""))}
    <div
      id={listboxId}
      class="custom-select-popover autocomplete-popover"
      role="listbox"
      onwheel={(e) => {
        if (!e.target.closest(".custom-select-list")) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
    >
      {#if headerText}
        <div class="autocomplete-header">
          <span>{headerText}</span>
        </div>
      {/if}
      <div
        class="custom-select-list scroller"
        style="max-height: 210px"
        onwheel={contenerWheelEnLista}
      >
        {#if allowEmptyOption}
          {@const isEmptySelected = value.trim() === ""}
          <button
            type="button"
            role="option"
            aria-selected={isEmptySelected}
            class="custom-select-option {isEmptySelected ? 'selected' : ''}"
            onpointerdown={(e) => {
              e.preventDefault();
              selectSuggestion("");
            }}
          >
            <span class="opt-text" style="color: var(--text-muted); font-style: italic">
              {emptyOptionLabel}
            </span>
            {#if isEmptySelected}
              <span class="opt-check">
                <Check size={14} strokeWidth={2.5} />
              </span>
            {/if}
          </button>
        {/if}
        {#each filtradas as item, idx}
          {@const isSelected = normalizar(value) === normalizar(item)}
          {@const isHighlighted = highlightedIndex === idx}
          {#if editingItem === item}
            <div class="autocomplete-edit-block">
              <div class="custom-select-add-form autocomplete-edit-row">
                <input
                  type="text"
                  class="custom-select-add-input"
                  bind:this={editInputEl}
                  bind:value={editDraft}
                  aria-label="Nuevo nombre para {item}"
                  autocorrect="off"
                  spellcheck="false"
                  disabled={busyOption}
                  onkeydown={(e) => {
                    if (!e.isComposing && e.key === "Enter") {
                      e.preventDefault();
                      e.stopPropagation();
                      confirmarEdicion();
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      e.stopPropagation();
                      bloquearCancelDialogo();
                      cancelarEdicion();
                    }
                  }}
                />
                <button
                  type="button"
                  class="btn btn-solid custom-select-add-save"
                  disabled={busyOption || !editDraft.trim()}
                  onpointerdown={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                  }}
                  onclick={(e) => {
                    e.stopPropagation();
                    confirmarEdicion();
                  }}
                >
                  Guardar
                </button>
                <button
                  type="button"
                  class="clear-search-btn"
                  title="Cancelar edición"
                  aria-label="Cancelar edición"
                  disabled={busyOption}
                  onpointerdown={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                  }}
                  onclick={(e) => {
                    e.stopPropagation();
                    cancelarEdicion();
                  }}
                >
                  <X size={13} />
                </button>
              </div>
              {#if editError}
                <div class="autocomplete-edit-error" role="alert">{editError}</div>
              {/if}
            </div>
          {:else}
            <div
              class="custom-select-option-row {isSelected ? 'selected' : ''} {isHighlighted ? 'highlighted' : ''}"
            >
              <button
                id="{listboxId}-opt-{idx}"
                type="button"
                role="option"
                aria-selected={isSelected}
                class="custom-select-option"
                onmouseenter={() => (highlightedIndex = idx)}
                onpointerdown={(e) => {
                  e.preventDefault();
                  selectSuggestion(item);
                }}
              >
                <span class="opt-text">{item}</span>
                {#if isSelected}
                  <span class="opt-check">
                    <Check size={14} strokeWidth={2.5} />
                  </span>
                {/if}
              </button>
              {#if onEditOption}
                <button
                  type="button"
                  class="custom-select-edit-btn"
                  title="Editar '{item}'"
                  aria-label="Editar {item}"
                  disabled={busyOption}
                  onpointerdown={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                  }}
                  onclick={(e) => {
                    e.stopPropagation();
                    iniciarEdicion(item);
                  }}
                >
                  <Edit3 size={13} />
                </button>
              {/if}
              {#if onDeleteOption}
                <button
                  type="button"
                  class="custom-select-delete-btn"
                  title="Eliminar '{item}' de la lista"
                  aria-label="Eliminar {item}"
                  disabled={busyOption}
                  onpointerdown={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                  }}
                  onclick={(e) => {
                    e.stopPropagation();
                    confirmarEliminacion(item);
                  }}
                >
                  <Trash2 size={13} />
                </button>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {/if}
</div>
