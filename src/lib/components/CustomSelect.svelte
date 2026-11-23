<script>
  import { ChevronDown, Check, Search, X, Copy, Trash2 } from "@lucide/svelte";

  let {
    id = "",
    value = $bindable(""),
    options = [],
    placeholder = "Seleccione una opción...",
    searchPlaceholder = "Filtrar opciones...",
    searchable = true,
    compact = false,
    invalid = false,
    onUseAsTemplate = undefined,
    templateHint = "",
    onDeleteOption = undefined,
    onchange = undefined,
  } = $props();

  let open = $state(false);
  let searchQuery = $state("");
  let highlightedIndex = $state(0);
  let containerEl = $state(null);
  let searchInputEl = $state(null);

  function normalizarTexto(s) {
    return String(s || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^\p{L}\p{N}]+/gu, " ")
      .trim()
      .toLowerCase();
  }

  let normalizedOptions = $derived.by(() => {
    const out = [];
    for (const opt of options) {
      if (typeof opt === "string") {
        out.push({ value: opt, label: opt, deletable: false, templateable: false });
      } else if (opt && typeof opt === "object") {
        const val = opt.value ?? "";
        const lbl = String(opt.label ?? val);
        out.push({
          value: val,
          label: lbl,
          deletable: Boolean(opt.deletable),
          templateable: Boolean(opt.templateable),
        });
      }
    }
    return out;
  });

  let filteredOptions = $derived.by(() => {
    const q = normalizarTexto(searchQuery.trim());
    if (!searchable || !q) return normalizedOptions;
    const tokens = q.split(/\s+/).filter(Boolean);
    return normalizedOptions.filter((opt) => {
      const norm = normalizarTexto(opt.label);
      return tokens.every((t) => norm.includes(t));
    });
  });

  let selectedOption = $derived(
    normalizedOptions.find((opt) => opt.value === value) || null
  );

  let displayLabel = $derived(
    selectedOption && selectedOption.value !== "" ? selectedOption.label : ""
  );

  function toggleMenu() {
    open = !open;
    if (open) {
      searchQuery = "";
      const currentIdx = normalizedOptions.findIndex((o) => o.value === value);
      highlightedIndex = currentIdx >= 0 ? currentIdx : 0;
      if (searchable) {
        setTimeout(() => {
          searchInputEl?.focus();
        }, 15);
      }
    }
  }

  function selectOption(opt) {
    value = opt.value;
    open = false;
    searchQuery = "";
    onchange?.(opt.value);
  }

  function handleUseAsTemplate(texto) {
    open = false;
    searchQuery = "";
    onUseAsTemplate?.(texto);
  }

  async function handleDeleteOption(opt) {
    await onDeleteOption?.(opt.value);
    if (value === opt.value) {
      value = "";
      onchange?.("");
    }
  }

  function handleKeydown(e) {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        toggleMenu();
      }
      return;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (filteredOptions.length > 0) {
        highlightedIndex = (highlightedIndex + 1) % filteredOptions.length;
      }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (filteredOptions.length > 0) {
        highlightedIndex =
          (highlightedIndex - 1 + filteredOptions.length) %
          filteredOptions.length;
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filteredOptions[highlightedIndex]) {
        selectOption(filteredOptions[highlightedIndex]);
      }
    }
  }

  function handleWindowPointerDown(e) {
    if (open && containerEl && !containerEl.contains(e.target)) {
      open = false;
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

<div class="custom-select-root {compact ? 'is-compact' : ''}" bind:this={containerEl}>
  <button
    type="button"
    {id}
    class="custom-select-trigger {open ? 'is-open' : ''} {invalid ? 'is-invalid' : ''}"
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={toggleMenu}
    onkeydown={handleKeydown}
  >
    <span class="custom-select-label {displayLabel ? 'has-value' : 'is-placeholder'}">
      {displayLabel || placeholder}
    </span>
    <span class="custom-select-chevron {open ? 'rotated' : ''}">
      <ChevronDown size={compact ? 14 : 16} strokeWidth={2} />
    </span>
  </button>

  {#if open}
    <div
      class="custom-select-popover"
      role="listbox"
      tabindex="-1"
      onwheel={(e) => {
        if (!e.target.closest(".custom-select-list")) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
    >
      {#if searchable}
        <div class="custom-select-search-box">
          <Search size={14} strokeWidth={2} class="search-icon" />
          <input
            type="text"
            class="custom-select-search-input"
            bind:this={searchInputEl}
            bind:value={searchQuery}
            oninput={() => (highlightedIndex = 0)}
            onkeydown={handleKeydown}
            placeholder={searchPlaceholder}
            autocorrect="off"
            autocapitalize="none"
            spellcheck="false"
          />
          {#if searchQuery}
            <button
              type="button"
              class="clear-search-btn"
              onclick={() => {
                searchQuery = "";
                highlightedIndex = 0;
                searchInputEl?.focus();
              }}
              aria-label="Limpiar filtro"
            >
              <X size={13} />
            </button>
          {/if}
        </div>
      {/if}

      {#if onUseAsTemplate && templateHint}
        <div class="custom-select-template-hint">
          <Copy size={11} aria-hidden="true" />
          <span>{templateHint}</span>
        </div>
      {/if}

      <div class="custom-select-list scroller" onwheel={contenerWheelEnLista}>
        {#if filteredOptions.length === 0}
          <div class="custom-select-empty">
            <span>Sin coincidencias</span>
            {#if onUseAsTemplate && searchQuery.trim()}
              <button
                type="button"
                class="custom-select-empty-action"
                onclick={() => handleUseAsTemplate(searchQuery.trim())}
              >
                Usar "{searchQuery.trim()}" en Otra
              </button>
            {/if}
          </div>
        {:else}
          {#each filteredOptions as opt, idx}
            {@const isSelected = value === opt.value}
            {@const isHighlighted = highlightedIndex === idx}
            <div
              class="custom-select-option-row {isSelected ? 'selected' : ''} {isHighlighted ? 'highlighted' : ''}"
            >
              <button
                type="button"
                role="option"
                aria-selected={isSelected}
                class="custom-select-option"
                onmouseenter={() => (highlightedIndex = idx)}
                onclick={() => selectOption(opt)}
              >
                <span class="opt-text">
                  {opt.label}
                  {#if opt.deletable}
                    <span class="opt-user-tag">Guardado</span>
                  {/if}
                </span>
                {#if isSelected}
                  <span class="opt-check">
                    <Check size={14} strokeWidth={2.5} />
                  </span>
                {/if}
              </button>
              {#if (opt.templateable && onUseAsTemplate) || (opt.deletable && onDeleteOption)}
                <div class="custom-select-row-actions">
                  {#if opt.templateable && onUseAsTemplate}
                    <button
                      type="button"
                      class="custom-select-template-btn"
                      title="Usar '{opt.label}' como plantilla en Otra"
                      aria-label="Usar {opt.label} como plantilla"
                      onclick={(e) => {
                        e.stopPropagation();
                        handleUseAsTemplate(opt.value);
                      }}
                    >
                      <Copy size={13} />
                    </button>
                  {/if}
                  {#if opt.deletable && onDeleteOption}
                    <button
                      type="button"
                      class="custom-select-delete-btn"
                      title="Eliminar '{opt.label}' de mi lista"
                      aria-label="Eliminar {opt.label}"
                      onclick={(e) => {
                        e.stopPropagation();
                        handleDeleteOption(opt);
                      }}
                    >
                      <Trash2 size={13} />
                    </button>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
