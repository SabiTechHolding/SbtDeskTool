<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { formatAppVersion } from "../utils/version";

  let { onclose, onCheckUpdate }: { onclose: () => void; onCheckUpdate: (onProgress: (message: string) => void) => Promise<void> } = $props();

  let checking = $state(false);
  let progress = $state("");
  let version = $state("unknown");

  onMount(async () => {
    try {
      version = formatAppVersion(await getVersion());
    } catch {
      version = "unknown";
    }
  });

  async function checkUpdate() {
    checking = true;
    await onCheckUpdate((message) => (progress = message));
    checking = false;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_interactive_supports_focus -->
<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === 'Escape' && onclose()}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="dialog" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <h2>SBS Desk Tool</h2>
    <p class="version">Version: {version}</p>
    <p class="link">GitHub: SabiTechHolding/SbtDeskTool</p>
    <div class="actions">
      <button class="btn" onclick={checkUpdate} disabled={checking}>
        {checking ? (progress || "Checking...") : "Check Update"}
      </button>
      <button class="btn" onclick={onclose}>Close</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 20px 24px;
    min-width: 280px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  h2 {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 4px;
    color: var(--fg);
  }

  .version {
    font-size: 12px;
    color: var(--fg2);
    margin-bottom: 4px;
  }

  .link {
    font-size: 11px;
    color: var(--fg2);
    margin-bottom: 16px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .btn {
    min-width: 72px;
    height: 28px;
    padding: 0 13px;
    background: var(--btn-bg);
    color: var(--btn-fg);
    border: 1px solid var(--border);
    border-radius: 5px;
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    background: var(--btn-hover);
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
