<script lang="ts">
  import { LANGUAGES } from "../utils/languages";

  let {
    engine, srcLang, destLang, onSetLang, onSwapText,
  }: {
    engine: string;
    srcLang: string;
    destLang: string;
    onSetLang: (field: "src_lang" | "dest_lang" | "engine", val: string) => void;
    onSwapText?: () => void;
  } = $props();

  const ENGINES = ["Google Translate"];

  let showEngineMenu = $state(false);
  let showSrcMenu = $state(false);
  let showDestMenu = $state(false);
  let engineFilter = $state("");
  let srcFilter = $state("");
  let destFilter = $state("");
  let engineInput = $state<HTMLInputElement>();
  let srcInput = $state<HTMLInputElement>();
  let destInput = $state<HTMLInputElement>();

  function toggleEngineMenu() {
    showEngineMenu = !showEngineMenu;
    if (showEngineMenu) queueMicrotask(() => engineInput?.focus());
  }

  function toggleSrcMenu() {
    showSrcMenu = !showSrcMenu;
    if (showSrcMenu) queueMicrotask(() => srcInput?.focus());
  }

  function toggleDestMenu() {
    showDestMenu = !showDestMenu;
    if (showDestMenu) queueMicrotask(() => destInput?.focus());
  }

  function filterItems(items: { name: string; code: string }[], filter: string) {
    if (!filter) return items;
    return items.filter((i) => i.name.toLowerCase().includes(filter.toLowerCase()));
  }

  function filterEngineNames(items: string[], filter: string) {
    if (!filter) return items;
    return items.filter((i) => i.toLowerCase().includes(filter.toLowerCase()));
  }

  function swapLangs() {
    if (srcLang === "Auto Detect") return;
    onSwapText?.();
  }
</script>

<div class="langbar" role="toolbar">
  <div class="lang-group">
    <span class="label">Engine</span>
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="dropdown-wrapper" role="presentation" onmouseleave={() => (showEngineMenu = false)}>
      <button class="dropdown-trigger" onclick={toggleEngineMenu}>
        {engine} ▾
      </button>
      {#if showEngineMenu}
        <div class="dropdown-menu">
          <input
            bind:this={engineInput}
            class="dropdown-filter"
            type="text"
            placeholder="Filter..."
            bind:value={engineFilter}
          />
          {#each filterEngineNames(ENGINES, engineFilter) as item}
            <button
              class="dropdown-item"
              class:active={item === engine}
              onclick={() => { onSetLang("engine", item); showEngineMenu = false; engineFilter = ""; }}
            >{item}</button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="lang-group">
    <span class="label">From</span>
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="dropdown-wrapper" role="presentation" onmouseleave={() => (showSrcMenu = false)}>
      <button class="dropdown-trigger" onclick={toggleSrcMenu}>
        {srcLang} ▾
      </button>
      {#if showSrcMenu}
        <div class="dropdown-menu">
          <input
            bind:this={srcInput}
            class="dropdown-filter"
            type="text"
            placeholder="Filter..."
            bind:value={srcFilter}
          />
          {#each filterItems(LANGUAGES, srcFilter) as item}
            <button
              class="dropdown-item"
              class:active={item.name === srcLang}
              onclick={() => { onSetLang("src_lang", item.name); showSrcMenu = false; srcFilter = ""; }}
            >{item.name}</button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <button class="swap-btn" onclick={swapLangs} title="Swap languages">⇄</button>

  <div class="lang-group">
    <span class="label">To</span>
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="dropdown-wrapper" role="presentation" onmouseleave={() => (showDestMenu = false)}>
      <button class="dropdown-trigger" onclick={toggleDestMenu}>
        {destLang} ▾
      </button>
      {#if showDestMenu}
        <div class="dropdown-menu">
          <input
            bind:this={destInput}
            class="dropdown-filter"
            type="text"
            placeholder="Filter..."
            bind:value={destFilter}
          />
          {#each filterItems(LANGUAGES.filter((item) => item.code !== "auto"), destFilter) as item}
            <button
              class="dropdown-item"
              class:active={item.name === destLang}
              onclick={() => { onSetLang("dest_lang", item.name); showDestMenu = false; destFilter = ""; }}
            >{item.name}</button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .langbar {
    display: flex;
    align-items: center;
    height: 34px;
    padding: 0 8px;
    background: var(--bg2);
    gap: 8px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }

  .lang-group {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .label {
    font-size: 11px;
    color: var(--fg2);
    white-space: nowrap;
  }

  .dropdown-wrapper {
    position: relative;
  }

  .dropdown-trigger {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 8px;
    background: var(--combo-bg);
    color: var(--combo-fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1;
  }

  .dropdown-trigger:hover {
    border-color: var(--accent);
  }

  .dropdown-menu {
    position: absolute;
    top: 100%;
    left: 0;
    background: var(--bg3);
    border: 1px solid var(--border);
    border-radius: 4px;
    z-index: 100;
    min-width: 200px;
    max-height: 300px;
    overflow-y: auto;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .dropdown-filter {
    width: 100%;
    padding: 4px 8px;
    background: var(--bg2);
    color: var(--fg);
    border: none;
    border-bottom: 1px solid var(--border);
    font-family: inherit;
    font-size: 11px;
    outline: none;
    box-sizing: border-box;
  }

  .dropdown-item {
    display: block;
    width: 100%;
    padding: 4px 8px;
    background: transparent;
    border: none;
    color: var(--fg);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    text-align: left;
  }

  .dropdown-item:hover {
    background: var(--btn-hover);
  }

  .dropdown-item.active {
    color: var(--accent);
    background: var(--bg2);
  }

  .swap-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    color: var(--accent);
    font-size: 16px;
    cursor: pointer;
    border-radius: 3px;
  }

  .swap-btn:hover {
    background: var(--btn-hover);
  }

  @media (max-width: 680px) {
    .langbar { gap: 4px; padding-inline: 4px; }
    .label { display: none; }
    .lang-group, .dropdown-wrapper { min-width: 0; }
    .lang-group { flex: 1 1 0; }
    .dropdown-trigger { width: 100%; max-width: none; padding-inline: 5px; }
    .swap-btn { flex: 0 0 24px; }
  }
</style>
