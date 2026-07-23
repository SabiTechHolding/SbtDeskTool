<script lang="ts">
  import { LANGUAGES } from "../utils/languages";
  import AppIcon from "./AppIcon.svelte";

  type TabId = "tran" | "diff" | "note";

  const tabs: Array<{ id: TabId; icon: string; label: string }> = [
    { id: "diff", icon: "diff", label: "Diff" },
    { id: "tran", icon: "translate", label: "Translate" },
    { id: "note", icon: "notes", label: "Notes" },
  ];

  let {
    activeTab,
    srcLang,
    destLang,
    onTop,
    notes,
    selectedNoteId,
    onSetLang,
    onSelectNote,
    onTabSwitch,
    onToggleCompact,
    onToggleOnTop,
  }: {
    activeTab: TabId;
    srcLang: string;
    destLang: string;
    onTop: boolean;
    notes: Array<{ id: number; title: string }>;
    selectedNoteId: number | null;
    onSetLang: (field: "src_lang" | "dest_lang" | "engine", val: string) => void;
    onSelectNote: (id: number) => void;
    onTabSwitch: (tab: TabId) => void;
    onToggleCompact: () => void;
    onToggleOnTop: () => void;
  } = $props();

  let showSrcMenu = $state(false);
  let showDestMenu = $state(false);
  let srcFilter = $state("");
  let destFilter = $state("");

  function filterItems(items: { name: string; code: string }[], filter: string) {
    if (!filter) return items;
    return items.filter((item) => item.name.toLowerCase().includes(filter.toLowerCase()));
  }
</script>

<div class="compact-bar">
  <nav class="compact-tabs" aria-label="Tools">
    {#each tabs as tab}
      <button
        class="icon-btn tab-btn"
        class:active={activeTab === tab.id}
        onclick={() => onTabSwitch(tab.id)}
        title={tab.label}
        aria-label={tab.label}
      ><AppIcon name={tab.icon} /></button>
    {/each}
  </nav>

  {#if activeTab === "tran"}
    <span class="separator"></span>
    <div class="dropdown-wrapper" onmouseleave={() => (showSrcMenu = false)} role="presentation">
      <button class="dropdown-trigger" onclick={() => (showSrcMenu = !showSrcMenu)} title={`From: ${srcLang}`}>
        {srcLang}
      </button>
      {#if showSrcMenu}
        <div class="dropdown-menu">
          <input class="dropdown-filter" type="text" placeholder="Filter..." bind:value={srcFilter} />
          {#each filterItems(LANGUAGES, srcFilter) as item}
            <button
              class="dropdown-item"
              onclick={() => { onSetLang("src_lang", item.name); showSrcMenu = false; srcFilter = ""; }}
            >{item.name}</button>
          {/each}
        </div>
      {/if}
    </div>

    <span class="arrow">↔</span>

    <div class="dropdown-wrapper" onmouseleave={() => (showDestMenu = false)} role="presentation">
      <button class="dropdown-trigger" onclick={() => (showDestMenu = !showDestMenu)} title={`To: ${destLang}`}>
        {destLang}
      </button>
      {#if showDestMenu}
        <div class="dropdown-menu right-menu">
          <input class="dropdown-filter" type="text" placeholder="Filter..." bind:value={destFilter} />
          {#each filterItems(LANGUAGES.filter((item) => item.code !== "auto"), destFilter) as item}
            <button
              class="dropdown-item"
              onclick={() => { onSetLang("dest_lang", item.name); showDestMenu = false; destFilter = ""; }}
            >{item.name}</button>
          {/each}
        </div>
      {/if}
    </div>
  {:else if activeTab === "note"}
    <span class="separator"></span>
    <label class="note-select-wrap" title="Select note">
      <AppIcon name="notes" size={12} />
      <select
        class="note-select"
        aria-label="Select note"
        value={selectedNoteId ?? ""}
        onchange={(event) => {
          const id = Number(event.currentTarget.value);
          if (Number.isFinite(id)) onSelectNote(id);
        }}
      >
        {#if notes.length === 0}<option value="">No notes</option>{/if}
        {#each notes as note (note.id)}
          <option value={note.id}>{note.title || "Untitled"}</option>
        {/each}
      </select>
    </label>
  {/if}

  <div class="spacer"></div>
  <button class="icon-btn" class:active={onTop} onclick={onToggleOnTop} title="Always on top" aria-label="Always on top">
    <AppIcon name="pin" />
  </button>
  <button class="full-btn" onclick={onToggleCompact} title="Exit compact mode" aria-label="Exit compact mode"><AppIcon name="expand" size={12} /><span class="btn-label">Full</span></button>
</div>

<style>
  .compact-bar {
    display: flex;
    align-items: center;
    min-width: 0;
    height: 26px;
    padding: 0 3px;
    background: var(--bg2);
    border-bottom: 1px solid var(--border);
    gap: 2px;
    flex-shrink: 0;
  }
  .compact-tabs { display: flex; align-items: stretch; height: 100%; gap: 1px; }
  .icon-btn, .full-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    height: 22px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--fg2);
    font: inherit;
    font-size: 10px;
    cursor: pointer;
    white-space: nowrap;
    line-height: 1;
  }
  .icon-btn { display: grid; place-items: center; width: 24px; padding: 0; }
  .full-btn { padding: 0 6px; }
  .icon-btn:hover, .full-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .icon-btn.active { background: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--accent); }
  .tab-btn.active { box-shadow: inset 0 -2px var(--accent); }
  .separator { width: 1px; height: 16px; margin: 0 2px; background: var(--border); }
  .dropdown-wrapper { position: relative; min-width: 0; }
  .dropdown-trigger {
    display: block;
    max-width: 100px;
    height: 20px;
    padding: 0 5px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--combo-bg);
    color: var(--combo-fg);
    font: inherit;
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .dropdown-menu {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    z-index: 100;
    min-width: 180px;
    max-height: 250px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg3);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }
  .dropdown-filter {
    width: 100%;
    padding: 4px 6px;
    border: 0;
    border-bottom: 1px solid var(--border);
    outline: 0;
    background: var(--bg2);
    color: var(--fg);
    font: inherit;
    font-size: 11px;
  }
  .dropdown-item {
    display: block;
    width: 100%;
    padding: 4px 8px;
    border: 0;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
  }
  .dropdown-item:hover { background: var(--btn-hover); }
  .arrow { color: var(--fg2); font-size: 11px; }
  .note-select-wrap { display: flex; align-items: center; gap: 4px; min-width: 0; max-width: 190px; color: var(--fg2); }
  .note-select {
    min-width: 70px;
    max-width: 165px;
    height: 20px;
    padding: 0 20px 0 5px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--combo-bg);
    color: var(--combo-fg);
    font: inherit;
    font-size: 10px;
  }
  .spacer { flex: 1; min-width: 2px; }

  @media (max-width: 420px) {
    .dropdown-trigger { max-width: 74px; }
    .full-btn { width: 24px; padding: 0; }
    .full-btn .btn-label { display: none; }
  }
</style>
