<script>
  import { ChevronDown, Check, Search, X } from "@lucide/svelte";

  let {
    id = "",
    value = $bindable(""),
    options = [],
    placeholder = "Seleccione una opción...",
    searchPlaceholder = "Filtrar opciones...",
    searchable = true,
    compact = false,
    invalid = false,
    onchange = undefined,
  } = $props();

  let open = $state(false);
  let searchQuery = $state("");
  let highlightedIndex = $state(0);
  let containerEl = $state(null);
  let searchInputEl = $state(null);

  let normalizedOptions = $derived.by(() => {
    const out = [];
    for (const opt of options) {
      if (typeof opt === "string") {
        if (!opt.startsWith("--")) {
          out.push({ value: opt, label: opt });
        }
      } else if (opt && typeof opt === "object") {
        const val = opt.value ?? "";
        const lbl = String(opt.label ?? val);
        if (!lbl.startsWith("--")) {
          out.push({ value: val, label: lbl });
        }
      }
    }
    return out;
  });

  let filteredOptions = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!searchable || !q) return normalizedOptions;
    return normalizedOptions.filter((opt) =>
      opt.label.toLowerCase().includes(q)
    );
  });

  let selectedOption = $derived(
    normalizedOptions.find((opt) => opt.value === value) || null
  );

  let displayLabel = $derived(selectedOption ? selectedOption.label : "");

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
    <div class="custom-select-popover" role="listbox" tabindex="-1">
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

      <div class="custom-select-list scroller">
        {#if filteredOptions.length === 0}
          <div class="custom-select-empty">Sin coincidencias</div>
        {:else}
          {#each filteredOptions as opt, idx}
            {@const isSelected = value === opt.value}
            {@const isHighlighted = highlightedIndex === idx}
            <button
              type="button"
              role="option"
              aria-selected={isSelected}
              class="custom-select-option {isSelected ? 'selected' : ''} {isHighlighted ? 'highlighted' : ''}"
              onmouseenter={() => (highlightedIndex = idx)}
              onclick={() => selectOption(opt)}
            >
              <span class="opt-text">{opt.label}</span>
              {#if isSelected}
                <span class="opt-check">
                  <Check size={14} strokeWidth={2.5} />
                </span>
              {/if}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
