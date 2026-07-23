<script lang="ts">
  import { onMount } from "svelte";

  let {
    title,
    message,
    confirmLabel = "OK",
    cancelLabel = "Cancel",
    showCancel = false,
    tone = "normal",
    onConfirm,
    onCancel,
  }: {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    showCancel?: boolean;
    tone?: "normal" | "warning" | "error";
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let confirmButton: HTMLButtonElement;

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") onCancel();
    if (event.key === "Enter") onConfirm();
  }

  onMount(() => confirmButton?.focus());
</script>

<svelte:window onkeydown={handleKeyDown} />

<div class="dialog-overlay" role="presentation" onclick={onCancel}>
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div
    class="app-dialog"
    class:warning={tone === "warning"}
    class:error={tone === "error"}
    role="dialog"
    aria-modal="true"
    aria-labelledby="app-dialog-title"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
    onkeydown={() => {}}
  >
    <h2 id="app-dialog-title">{title}</h2>
    <p>{message}</p>
    <div class="dialog-actions">
      {#if showCancel}
        <button class="secondary" onclick={onCancel}>{cancelLabel}</button>
      {/if}
      <button bind:this={confirmButton} class="primary" onclick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .dialog-overlay { position: fixed; inset: 0; z-index: 1600; display: grid; place-items: center; padding: 20px; background: rgba(0,0,0,.5); }
  .app-dialog { width: min(430px, 92vw); padding: 17px 18px 15px; border: 1px solid var(--border); border-top: 3px solid var(--accent); border-radius: 8px; background: var(--bg3); color: var(--fg); box-shadow: 0 16px 48px rgba(0,0,0,.48); outline: none; }
  .app-dialog.warning { border-top-color: var(--warning, #d8a021); }
  .app-dialog.error { border-top-color: var(--error, #e05252); }
  h2 { margin: 0 0 8px; font-size: 15px; }
  p { margin: 0; color: var(--fg2); font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
  .dialog-actions { display: flex; justify-content: flex-end; gap: 7px; margin-top: 17px; }
  button { min-width: 72px; height: 28px; padding: 0 13px; border: 1px solid var(--border); border-radius: 5px; color: var(--fg); font: inherit; font-size: 11px; line-height: 1; white-space: nowrap; cursor: pointer; }
  button.secondary { background: var(--btn-bg); }
  button.primary { border-color: var(--accent); background: var(--accent); color: var(--bg); }
  button:hover { filter: brightness(1.08); }
</style>
