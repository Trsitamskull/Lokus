<script>
  import { Check, ChevronDown } from "@lucide/svelte";

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
  } = $props();

  let open = $state(false);
  let mostrarTodas = $state(false);
  let highlightedIndex = $state(-1);
  let rootEl = $state(null);
  const fallbackId = `ac-${Math.random().toString(36).slice(2, 8)}`;
  let listboxId = $derived(`${id || fallbackId}-listbox`);

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

  function selectSuggestion(item) {
    value = item;
    open = false;
    mostrarTodas = false;
    highlightedIndex = -1;
    onSelect?.(item);
  }

  function toggleDropdown() {
    if (suggestions.length === 0) return;
    if (open) {
      open = false;
      mostrarTodas = false;
      highlightedIndex = -1;
    } else {
      open = true;
      mostrarTodas = true;
      highlightedIndex = -1;
      inputEl?.focus();
    }
  }

  function handleKeydown(e) {
    if (e.isComposing) return;

    if (e.key === "Escape" && open && (filtradas.length > 0 || (allowEmptyOption && value.trim() !== ""))) {
      e.preventDefault();
      e.stopPropagation();
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
        open = false;
        mostrarTodas = false;
        onEnter();
      }
    }
  }

  function handleWindowPointerDown(e) {
    if (open && rootEl && !rootEl.contains(e.target)) {
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
      oninput={() => {
        open = true;
        mostrarTodas = false;
        highlightedIndex = -1;
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
          <button
            id="{listboxId}-opt-{idx}"
            type="button"
            role="option"
            aria-selected={isSelected}
            class="custom-select-option {isSelected ? 'selected' : ''} {isHighlighted ? 'highlighted' : ''}"
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
        {/each}
      </div>
    </div>
  {/if}
</div>
