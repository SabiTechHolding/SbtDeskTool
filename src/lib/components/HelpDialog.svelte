<script lang="ts">
  import { onDestroy, onMount } from "svelte";

  let { onclose }: { onclose: () => void } = $props();

  const shortcuts = [
    ["Ctrl + F", "Open search in the focused text area"],
    ["Ctrl + Shift + F · Diff/Translate", "Open common search across both text areas"],
    ["Enter / Shift + Enter", "Next / previous common-search result"],
    ["Ctrl + Enter · Notes", "Save the current note"],
    ["Ctrl + mouse wheel or Ctrl + / −", "Increase or decrease editor font size"],
    ["Ctrl + Z / Ctrl + Y", "Undo / redo in editable text areas"],
    ["Ctrl + Alt + T", "Show or hide SBS Desk Tool globally"],
    ["Escape", "Close search or this guide"],
  ];

  function handleKey(event: KeyboardEvent) {
    if (event.key === "Escape") onclose();
  }

  onMount(() => window.addEventListener("keydown", handleKey));
  onDestroy(() => window.removeEventListener("keydown", handleKey));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_interactive_supports_focus -->
<div class="overlay" onclick={onclose} onkeydown={handleKey}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <section class="dialog" aria-label="Help and keyboard shortcuts" onclick={(event) => event.stopPropagation()} onkeydown={() => {}}>
    <header>
      <div>
        <h2>Help & shortcuts</h2>
        <p>Quick guide for Diff, Translate and Notes.</p>
      </div>
      <button class="close" onclick={onclose} title="Close (Escape)">×</button>
    </header>

    <div class="content">
      <section>
        <h3>Editors</h3>
        <p>Source, Left, Right and Notes are editable; Translated is read-only. Each text area keeps its own search, cursor position, word wrap and zoom state.</p>
      </section>
      <section>
        <h3>Diff</h3>
        <p>Diff updates automatically. Word controls inline highlighting; Ignore WS ignores whitespace-only changes; Wrap wraps long lines; Show WS displays whitespace characters.</p>
        <p>Legacy/Advanced selects one diff algorithm. Detail shows the focused line, Actions toggles the center copy/revert gutter, and All/L/R control common or per-editor search.</p>
      </section>
      <section>
        <h3>Translate & Notes</h3>
        <p>Translate runs automatically after Source changes and reuses unchanged lines. Notes support Markdown preview, auto-save, filtering and quick note selection in Compact mode.</p>
      </section>
      <section>
        <h3>Keyboard</h3>
        <div class="shortcut-list">
          {#each shortcuts as shortcut}
            <kbd>{shortcut[0]}</kbd><span>{shortcut[1]}</span>
          {/each}
        </div>
      </section>
    </div>

    <footer><button class="primary" onclick={onclose}>Close</button></footer>
  </section>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 1200; display: grid; place-items: center; padding: 20px; background: rgba(0,0,0,.48); }
  .dialog { width: min(680px, 92vw); max-height: min(620px, 86vh); display: flex; flex-direction: column; overflow: hidden; background: var(--bg); color: var(--fg); border: 1px solid var(--border); border-radius: 10px; box-shadow: 0 18px 55px rgba(0,0,0,.45); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; padding: 16px 18px 12px; border-bottom: 1px solid var(--border); }
  h2 { margin: 0; font-size: 17px; } header p { margin: 4px 0 0; color: var(--fg2); font-size: 11px; }
  .close { width: 28px; height: 28px; border: 0; border-radius: 5px; background: transparent; color: var(--fg2); font-size: 20px; cursor: pointer; }
  .close:hover { background: var(--btn-hover); color: var(--fg); }
  .content { padding: 4px 18px 14px; overflow: auto; }
  section section { margin-top: 14px; }
  h3 { margin: 0 0 5px; color: var(--accent); font-size: 12px; }
  p { margin: 4px 0; color: var(--fg2); font-size: 11px; line-height: 1.55; }
  .shortcut-list { display: grid; grid-template-columns: max-content 1fr; align-items: center; gap: 7px 14px; font-size: 11px; color: var(--fg2); }
  kbd { padding: 3px 7px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 4px; background: var(--bg2); color: var(--fg); font: 10px "Consolas", monospace; text-align: center; }
  footer { display: flex; justify-content: flex-end; padding: 10px 18px; border-top: 1px solid var(--border); }
  .primary { min-width: 72px; height: 28px; padding: 0 13px; border: 1px solid var(--accent); border-radius: 5px; background: var(--accent); color: var(--bg); font: inherit; font-size: 11px; line-height: 1; white-space: nowrap; cursor: pointer; }
</style>
