<script>
  import { ChevronDown, Check, Search, X, Plus, Trash2 } from "@lucide/svelte";

  let {
    id = "",
    value = $bindable(""),
    options = [],
    placeholder = "Seleccione una opción...",
    searchPlaceholder = "Filtrar opciones...",
    searchable = true,
    compact = false,
    invalid = false,
    allowAdd = false,
    addButtonLabel = "Agregar nuevo a la lista...",
    addPlaceholder = "Escribe el nuevo nombre...",
    onAddOption = undefined,
    onDeleteOption = undefined,
    onchange = undefined,
  } = $props();

  let open = $state(false);
  let searchQuery = $state("");
  let highlightedIndex = $state(0);
  let addingNew = $state(false);
  let newOptionText = $state("");
  let containerEl = $state(null);
  let searchInputEl = $state(null);
  let newOptionInputEl = $state(null);

  function normalizarTexto(s) {
    return String(s || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase();
  }

  let normalizedOptions = $derived.by(() => {
    const out = [];
    for (const opt of options) {
      if (typeof opt === "string") {
        if (!opt.startsWith("--")) {
          out.push({ value: opt, label: opt, deletable: false });
        }
      } else if (opt && typeof opt === "object") {
        const val = opt.value ?? "";
        const lbl = String(opt.label ?? val);
        if (!lbl.startsWith("--")) {
          out.push({
            value: val,
            label: lbl,
            deletable: Boolean(opt.deletable),
          });
        }
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
      addingNew = false;
      newOptionText = "";
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
    addingNew = false;
    newOptionText = "";
    onchange?.(opt.value);
  }

  function startAddingNew() {
    addingNew = true;
    newOptionText = searchQuery.trim();
    setTimeout(() => {
      newOptionInputEl?.focus();
    }, 15);
  }

  async function confirmAddNew() {
    const limpio = newOptionText.trim().replace(/\s+/g, " ");
    if (!limpio) return;
    await onAddOption?.(limpio);
    value = limpio;
    addingNew = false;
    newOptionText = "";
    searchQuery = "";
    open = false;
    onchange?.(limpio);
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

      <div class="custom-select-list scroller" onwheel={contenerWheelEnLista}>
        {#if filteredOptions.length === 0}
          <div class="custom-select-empty">Sin coincidencias</div>
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
          {/each}
        {/if}
      </div>

      {#if allowAdd && onAddOption}
        <div class="custom-select-footer">
          {#if addingNew}
            <div class="custom-select-add-form">
              <input
                type="text"
                class="custom-select-add-input"
                bind:this={newOptionInputEl}
                bind:value={newOptionText}
                placeholder={addPlaceholder}
                autocorrect="off"
                spellcheck="false"
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    e.stopPropagation();
                    confirmAddNew();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    e.stopPropagation();
                    addingNew = false;
                  }
                }}
              />
              <button
                type="button"
                class="btn btn-solid custom-select-add-save"
                disabled={!newOptionText.trim()}
                onclick={confirmAddNew}
              >
                Guardar
              </button>
              <button
                type="button"
                class="clear-search-btn"
                title="Cancelar"
                aria-label="Cancelar"
                onclick={() => {
                  addingNew = false;
                  newOptionText = "";
                }}
              >
                <X size={14} />
              </button>
            </div>
          {:else}
            <button
              type="button"
              class="custom-select-add-trigger"
              onclick={startAddingNew}
            >
              <Plus size={14} strokeWidth={2.2} />
              <span>
                {searchQuery.trim()
                  ? `Agregar "${searchQuery.trim()}" a la lista`
                  : addButtonLabel}
              </span>
            </button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
