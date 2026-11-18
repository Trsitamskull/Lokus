<script>
  import { Check } from "@lucide/svelte";

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
    maxSuggestions = 8,
    inputEl = $bindable(null),
    onEnter = undefined,
    onSelect = undefined,
  } = $props();

  let open = $state(false);
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
    const unicas = [];
    const vistas = new Set();

    for (const s of suggestions) {
      const clean = String(s || "").trim();
      if (!clean) continue;
      const key = normalizar(clean);
      if (vistas.has(key)) continue;
      if (!q || key.includes(q)) {
        vistas.add(key);
        unicas.push(clean);
        if (unicas.length >= maxSuggestions) break;
      }
    }

    if (
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
    highlightedIndex = -1;
    onSelect?.(item);
  }

  function handleKeydown(e) {
    if (e.isComposing) return;

    if (e.key === "Escape" && open && filtradas.length > 0) {
      e.preventDefault();
      e.stopPropagation();
      open = false;
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
        onEnter();
      }
    }
  }

  function handleWindowPointerDown(e) {
    if (open && rootEl && !rootEl.contains(e.target)) {
      open = false;
      highlightedIndex = -1;
    }
  }
</script>

<svelte:window onpointerdown={handleWindowPointerDown} />

<div class="autocomplete-root" bind:this={rootEl}>
  <input
    {id}
    {name}
    type="text"
    class="input {invalid ? 'is-invalid' : ''}"
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
    aria-expanded={open && filtradas.length > 0}
    onfocus={() => {
      if (suggestions.length > 0) open = true;
    }}
    oninput={() => {
      open = true;
      highlightedIndex = -1;
    }}
    onkeydown={handleKeydown}
  />

  {#if open && filtradas.length > 0}
    <div id={listboxId} class="custom-select-popover autocomplete-popover" role="listbox">
      <div class="autocomplete-header">Sugerencias guardadas</div>
      <div class="custom-select-list scroller" style="max-height: 185px">
        {#each filtradas as item, idx}
          {@const isSelected = normalizar(value) === normalizar(item)}
          {@const isHighlighted = highlightedIndex === idx}
          <button
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
