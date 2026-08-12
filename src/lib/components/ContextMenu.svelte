<script lang="ts">
  import { onMount } from "svelte";

  export interface ContextItem {
    label: string;
    action: () => void;
    disabled?: boolean;
    divider?: boolean;
    shortcut?: string;
  }

  let {
    items = [] as ContextItem[],
    x = 0,
    y = 0,
    onClose,
  }: {
    items: ContextItem[];
    x: number;
    y: number;
    onClose: () => void;
  } = $props();

  let menuEl: HTMLDivElement;

  function handleClick(item: ContextItem) {
    if (!item.disabled) {
      item.action();
      onClose();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  function handleOutsidePointer(event: PointerEvent) {
    if (event.button === 0 && menuEl && !menuEl.contains(event.target as Node)) onClose();
  }

  onMount(() => {
    menuEl?.focus();
  });
</script>

<svelte:window onkeydown={handleKeyDown} onpointerdown={handleOutsidePointer} />

<!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
<div
  bind:this={menuEl}
  class="context-menu"
  style="left: {x}px; top: {y}px;"
  tabindex="-1"
  onclick={() => onClose()}
  onkeydown={handleKeyDown}
>
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div class="menu-items" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
    {#each items as item}
      {#if item.divider}
        <div class="menu-divider"></div>
      {:else}
        <button
          class="menu-item"
          disabled={item.disabled}
          onclick={() => handleClick(item)}
        >
          <span>{item.label}</span>
          {#if item.shortcut}<span class="menu-shortcut">{item.shortcut}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 1000;
    min-width: 160px;
    background: var(--bg3);
    border: 1px solid var(--border);
    border-radius: 4px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    padding: 2px;
    outline: none;
  }

  .menu-items {
    display: flex;
    flex-direction: column;
  }

  .menu-item {
    display: block;
    width: 100%;
    padding: 4px 12px;
    background: transparent;
    border: none;
    color: var(--fg);
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    border-radius: 2px;
  }
  .menu-shortcut { margin-left: 24px; color: var(--fg3); font-size: 10px; }

  .menu-item:hover:not(:disabled) {
    background: var(--btn-hover);
  }

  .menu-item:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .menu-divider {
    height: 1px;
    background: var(--border);
    margin: 2px 4px;
  }
</style>
