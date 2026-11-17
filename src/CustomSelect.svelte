<script>
  import { ChevronDown, Check, Search, X } from "@lucide/svelte";

  let {
    id = "",
    value = $bindable(""),
    options = [],
    placeholder = "Seleccione una opción...",
    invalid = false,
    onchange = undefined,
  } = $props();

  let open = $state(false);
  let searchQuery = $state("");
  let highlightedIndex = $state(0);
  let containerEl = $state(null);
  let searchInputEl = $state(null);

  let usableOptions = $derived(
    options.filter((opt) => !opt.startsWith("--"))
  );

  let filteredOptions = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return usableOptions;
    return usableOptions.filter((opt) => opt.toLowerCase().includes(q));
  });

  let displayValue = $derived(
    !value || value.startsWith("--") ? "" : value
  );

  function toggleMenu() {
    open = !open;
    if (open) {
      searchQuery = "";
      const currentIdx = usableOptions.indexOf(value);
      highlightedIndex = currentIdx >= 0 ? currentIdx : 0;
      setTimeout(() => {
        if (searchInputEl) searchInputEl.focus();
      }, 15);
    }
  }

  function selectOption(opt) {
    value = opt;
    open = false;
    searchQuery = "";
    if (onchange) onchange(opt);
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

<div class="custom-select-root" bind:this={containerEl}>
  <button
    type="button"
    {id}
    class="custom-select-trigger {open ? 'is-open' : ''} {invalid ? 'is-invalid' : ''}"
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={toggleMenu}
    onkeydown={handleKeydown}
  >
    <span class="custom-select-label {displayValue ? 'has-value' : 'is-placeholder'}">
      {displayValue || placeholder}
    </span>
    <span class="custom-select-chevron {open ? 'rotated' : ''}">
      <ChevronDown size={16} strokeWidth={2} />
    </span>
  </button>

  {#if open}
    <div class="custom-select-popover" role="listbox" tabindex="-1">
      <div class="custom-select-search-box">
        <Search size={14} strokeWidth={2} class="search-icon" />
        <input
          type="text"
          class="custom-select-search-input"
          bind:this={searchInputEl}
          bind:value={searchQuery}
          oninput={() => (highlightedIndex = 0)}
          onkeydown={handleKeydown}
          placeholder="Filtrar tipo de audiencia..."
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

      <div class="custom-select-list">
        {#if filteredOptions.length === 0}
          <div class="custom-select-empty">Sin coincidencias</div>
        {:else}
          {#each filteredOptions as opt, idx}
            {@const isSelected = value === opt}
            {@const isHighlighted = highlightedIndex === idx}
            <button
              type="button"
              role="option"
              aria-selected={isSelected}
              class="custom-select-option {isSelected ? 'selected' : ''} {isHighlighted ? 'highlighted' : ''}"
              onmouseenter={() => (highlightedIndex = idx)}
              onclick={() => selectOption(opt)}
            >
              <span class="opt-text">{opt}</span>
              {#if isSelected}
                <span class="opt-check">
                  <Check size={15} strokeWidth={2.5} />
                </span>
              {/if}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
