<script lang="ts">
  import { themeStore } from "../stores/theme";
  import AboutDialog from "./AboutDialog.svelte";
  import HelpDialog from "./HelpDialog.svelte";
  import AppIcon from "./AppIcon.svelte";

  type TabId = "tran" | "diff" | "note";

  const TABS: Array<{ id: TabId; icon: string; label: string }> = [
    { id: "diff", icon: "diff", label: "Diff" },
    { id: "tran", icon: "translate", label: "Translate" },
    { id: "note", icon: "notes", label: "Notes" },
  ];

  const ALL_WINDOW_EFFECTS = [
    { key: "solid", label: "Solid" },
    { key: "blur", label: "Blur" },
    { key: "frosted", label: "Frosted" },
    { key: "transp", label: "Transparent" },
    { key: "dim", label: "Dim" },
    { key: "ghost", label: "Ghost" },
    { key: "clear", label: "Clear" },
  ];
  const WINDOW_EFFECTS = /Linux/i.test(navigator.userAgent)
    ? ALL_WINDOW_EFFECTS.filter((effect) => effect.key === "solid")
    : /Mac/i.test(navigator.userAgent)
      ? ALL_WINDOW_EFFECTS.filter((effect) => ["solid", "blur", "frosted"].includes(effect.key))
      : ALL_WINDOW_EFFECTS;

  let {
    activeTab,
    compact,
    layout,
    windowEffect,
    onTop,
    onTabSwitch,
    onToggleCompact,
    onToggleLayout,
    onToggleOnTop,
    onSelectEffect,
    onCheckUpdate,
  }: {
    activeTab: TabId;
    compact: boolean;
    layout: "horizontal" | "vertical";
    windowEffect: string;
    onTop: boolean;
    onTabSwitch: (tab: TabId) => void;
    onToggleCompact: () => void;
    onToggleLayout: () => void;
    onToggleOnTop: () => void;
    onSelectEffect: (key: string) => void;
    onCheckUpdate: (onProgress: (message: string) => void) => Promise<void>;
  } = $props();

  let showAbout = $state(false);
  let showHelp = $state(false);
  let showEffectMenu = $state(false);

  function handleThemeToggle() {
    themeStore.toggle();
  }

  function handleEffectSelect(key: string) {
    onSelectEffect(key);
    showEffectMenu = false;
  }
</script>

<header class="topbar">
  <nav class="tabs">
    {#each TABS as { id, icon, label }}
      <button
        class="tab-btn"
        class:active={activeTab === id}
        onclick={() => onTabSwitch(id)}
        title={label}
      >
        <AppIcon name={icon} /> <span class="btn-label">{label}</span>
      </button>
    {/each}
  </nav>

  <div class="separator"></div>

  <div class="actions">
    {#if activeTab === "tran"}
      <button
        class="action-btn"
        onclick={onToggleLayout}
        title={layout === "horizontal" ? "Switch to Vertical" : "Switch to Horizontal"}
      >
        <AppIcon name="layout" /> <span class="btn-label">{layout === "horizontal" ? "Horizontal" : "Vertical"}</span>
      </button>
    {/if}

    <!-- svelte-ignore a11y_no_static_element_interactions a11y_interactive_supports_focus -->
    <div class="effect-wrapper" onmouseleave={() => (showEffectMenu = false)}>
      <button
        class="action-btn"
        onclick={() => (showEffectMenu = !showEffectMenu)}
        title="Window Effect"
      >
        <AppIcon name="window" /> <span class="btn-label">{WINDOW_EFFECTS.find((e) => e.key === windowEffect)?.label ?? "Solid"}</span>
      </button>
      {#if showEffectMenu}
        <div class="effect-menu">
          {#each WINDOW_EFFECTS as { key, label }}
            <button
              class="effect-item"
              class:active={key === windowEffect}
              onclick={() => handleEffectSelect(key)}
            >{label}</button>
          {/each}
        </div>
      {/if}
    </div>

    <button
      class="action-btn"
      class:toggled={compact}
      onclick={onToggleCompact}
      title="Compact Mode"
    >
      <AppIcon name="compact" /> <span class="btn-label">Compact</span>
    </button>

    <button class="action-btn" class:toggled={onTop} onclick={onToggleOnTop} title="Always on Top">
      <AppIcon name="pin" /> <span class="btn-label">On Top</span>
    </button>

    <button class="action-btn" onclick={handleThemeToggle} title="Toggle Theme">
      <AppIcon name={$themeStore.current === "dark" ? "sun" : "moon"} />
      <span class="btn-label">{$themeStore.current === "dark" ? "Light" : "Dark"}</span>
    </button>

    <button class="action-btn" onclick={() => (showHelp = true)} title="Help and keyboard shortcuts">
      <AppIcon name="help" /> <span class="btn-label">Help</span>
    </button>

    <button class="action-btn" onclick={() => (showAbout = true)} title="About">
      <AppIcon name="info" /> <span class="btn-label">About</span>
    </button>
  </div>
</header>

{#if showAbout}
  <AboutDialog onclose={() => (showAbout = false)} {onCheckUpdate} />
{/if}

{#if showHelp}
  <HelpDialog onclose={() => (showHelp = false)} />
{/if}

<style>
  .topbar {
    display: flex;
    align-items: center;
    height: 36px;
    background: var(--bg2);
    border-bottom: 2px solid var(--border);
    user-select: none;
    flex-shrink: 0;
  }

  .tabs {
    display: flex;
    align-items: stretch;
    height: 100%;
  }

  .tab-btn {
    display: flex;
    align-items: center;
    padding: 0 10px;
    background: transparent;
    border: none;
    color: var(--fg2);
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    position: relative;
    transition: color 0.15s, background 0.15s;
    gap: 5px;
  }

  .tab-btn:hover {
    background: var(--bg3);
    color: var(--fg);
  }

  .tab-btn.active {
    color: var(--fg);
    background: var(--bg);
  }

  .tab-btn.active::after {
    content: "";
    position: absolute;
    bottom: 0;
    left: 4px;
    right: 4px;
    height: 2px;
    background: var(--accent);
    border-radius: 1px 1px 0 0;
  }

  .separator {
    width: 1px;
    height: 24px;
    background: var(--border);
    margin: 0 4px;
    flex-shrink: 0;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    padding: 0 4px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    padding: 0 var(--control-padding-x);
    background: transparent;
    border: none;
    color: var(--fg2);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    border-radius: 3px;
    white-space: nowrap;
    transition: background 0.15s, color 0.15s;
    gap: 4px;
    flex: 0 0 auto;
    line-height: 1;
  }

  .action-btn:hover {
    background: var(--btn-hover);
    color: var(--fg);
  }

  .action-btn.toggled {
    background: var(--accent);
    color: var(--bg);
  }

  .effect-wrapper {
    position: relative;
  }

  .effect-menu {
    position: absolute;
    top: 100%;
    right: 0;
    background: var(--bg3);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px;
    z-index: 100;
    min-width: 140px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .effect-item {
    display: block;
    width: 100%;
    padding: 4px 10px;
    background: transparent;
    border: none;
    color: var(--fg);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    border-radius: 2px;
  }

  .effect-item:hover {
    background: var(--btn-hover);
  }

  .effect-item.active {
    color: var(--accent);
  }

  @media (max-width: 820px) {
    .action-btn .btn-label { display: none; }
    .action-btn { width: 26px; padding: 0; }
  }

  @media (max-width: 560px) {
    .tab-btn .btn-label { display: none; }
    .tab-btn { width: 32px; justify-content: center; padding: 0; }
    .separator { margin-inline: 2px; }
    .actions { gap: 1px; padding-inline: 2px; }
  }
</style>
