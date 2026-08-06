<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { loadSettings, saveSetting } from "../stores/settings";
  import { open } from "@tauri-apps/plugin-dialog";

  interface FolderDiffEntry {
    relative_path: string;
    status: "left_only" | "right_only" | "different" | "equal";
    left_path: string | null;
    right_path: string | null;
    left_size: number | null;
    right_size: number | null;
  }

  let { onOpenFiles, onStatusUpdate }: {
    onOpenFiles: (left: string, right: string, label: string) => void;
    onStatusUpdate?: (text: string, kind: string) => void;
  } = $props();

  let leftFolder = $state("");
  let rightFolder = $state("");
  let entries = $state<FolderDiffEntry[]>([]);
  let filter = $state<"changes" | "all" | "different" | "left_only" | "right_only">("changes");
  let loading = $state(false);
  let error = $state("");
  let restoringState = $state(true);

  const labelForStatus: Record<FolderDiffEntry["status"], string> = {
    different: "Different",
    left_only: "Left only",
    right_only: "Right only",
    equal: "Equal",
  };

  const iconForStatus: Record<FolderDiffEntry["status"], string> = {
    different: "≠",
    left_only: "←",
    right_only: "→",
    equal: "=",
  };

  function formatSize(size: number | null) {
    if (size === null) return "—";
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }

  function persistState() {
    void saveSetting("folder_diff_left_folder", leftFolder);
    void saveSetting("folder_diff_right_folder", rightFolder);
    void saveSetting("folder_diff_filter", filter);
  }

  function clearSavedState() {
    leftFolder = "";
    rightFolder = "";
    filter = "changes";
    entries = [];
    error = "";
    persistState();
    onStatusUpdate?.("Cleared saved folder comparison", "normal");
  }

  async function chooseFolder(side: "left" | "right") {
    const selected = await open({ directory: true, multiple: false, title: `Choose ${side} folder` });
    if (typeof selected !== "string") return;
    if (side === "left") leftFolder = selected;
    else rightFolder = selected;
    error = "";
    persistState();
  }

  async function compare() {
    if (!leftFolder || !rightFolder || loading) return;
    loading = true;
    error = "";
    try {
      entries = await invoke<FolderDiffEntry[]>("compare_folders", {
        leftRoot: leftFolder,
        rightRoot: rightFolder,
      });
      const changed = entries.filter((entry) => entry.status !== "equal").length;
      persistState();
      onStatusUpdate?.(`Folder comparison complete: ${changed} changed of ${entries.length} files`, "success");
    } catch (reason) {
      entries = [];
      error = String(reason);
      onStatusUpdate?.(`Folder comparison failed: ${error}`, "error");
    } finally {
      loading = false;
    }
  }

  async function restoreSavedState() {
    const settings = await loadSettings();
    leftFolder = settings.folder_diff_left_folder;
    rightFolder = settings.folder_diff_right_folder;
    filter = settings.folder_diff_filter;
    restoringState = false;
    if (leftFolder && rightFolder) await compare();
  }

  export async function openFolders(left: string, right: string) {
    leftFolder = left;
    rightFolder = right;
    await compare();
  }

  async function preview(entry: FolderDiffEntry) {
    error = "";
    try {
      const [left, right] = await Promise.all([
        entry.left_path ? invoke<string>("read_folder_diff_file", { path: entry.left_path }) : Promise.resolve(""),
        entry.right_path ? invoke<string>("read_folder_diff_file", { path: entry.right_path }) : Promise.resolve(""),
      ]);
      onOpenFiles(left, right, entry.relative_path);
      onStatusUpdate?.(`Opened ${entry.relative_path}`, "normal");
    } catch (reason) {
      error = String(reason);
      onStatusUpdate?.(`Could not preview ${entry.relative_path}: ${error}`, "error");
    }
  }

  const visibleEntries = $derived(entries.filter((entry) =>
    filter === "all" ? true : filter === "changes" ? entry.status !== "equal" : entry.status === filter,
  ));

  function handleFolderPaths(event: Event) {
    const paths = (event as CustomEvent<string[]>).detail;
    if (Array.isArray(paths) && paths.length === 2) void openFolders(paths[0], paths[1]);
  }

  onMount(() => {
    document.addEventListener("folder:setPaths", handleFolderPaths);
    void restoreSavedState();
  });
  onDestroy(() => document.removeEventListener("folder:setPaths", handleFolderPaths));
</script>

<section class="folder-diff" aria-label="Folder comparison">
  <div class="folder-controls">
    <div class="folder-picker">
      <span class="folder-side">Left folder</span>
      <button class="folder-path" onclick={() => void chooseFolder("left")} title={leftFolder || "Choose Left folder"}>{leftFolder || "Choose…"}</button>
    </div>
    <div class="folder-picker">
      <span class="folder-side">Right folder</span>
      <button class="folder-path" onclick={() => void chooseFolder("right")} title={rightFolder || "Choose Right folder"}>{rightFolder || "Choose…"}</button>
    </div>
    <button class="compare-button" disabled={!leftFolder || !rightFolder || loading} onclick={() => void compare()}>{loading ? "Comparing…" : "Compare folders"}</button>
    <button class="clear-state-button" onclick={clearSavedState} title="Clear saved folder comparison">Clear</button>
    <div class="filter-group control-group" aria-label="Folder result filter">
      <button class:active={filter === "all"} onclick={() => { filter = "all"; persistState(); }} title="All files" aria-label="All files">&#8801;</button><button class:active={filter === "changes"} onclick={() => { filter = "changes"; persistState(); }} title="Changes only" aria-label="Changes only">&#8800;</button><button class:active={filter === "different"} onclick={() => { filter = "different"; persistState(); }} title="Different" aria-label="Different">&#8776;</button><button class:active={filter === "left_only"} onclick={() => { filter = "left_only"; persistState(); }} title="Left only" aria-label="Left only">&#8592;</button><button class:active={filter === "right_only"} onclick={() => { filter = "right_only"; persistState(); }} title="Right only" aria-label="Right only">&#8594;</button>
    </div>
  </div>

  {#if error}<p class="folder-error">{error}</p>{/if}
  <div class="folder-results" role="list" aria-label="Folder comparison results">
    {#if !entries.length && !loading}
      <p class="folder-empty">Choose two folders, then compare them.</p>
    {:else}
      {#each visibleEntries as entry (entry.relative_path)}
        <button class="folder-row"
          class:equal={entry.status === "equal"} class:different={entry.status === "different"}
          class:left-only={entry.status === "left_only"} class:right-only={entry.status === "right_only"}
          onclick={() => void preview(entry)} title={`Open ${entry.relative_path} in the text diff`}>
          <span class="status" class:different={entry.status === "different"} class:left-only={entry.status === "left_only"} class:right-only={entry.status === "right_only"} title={labelForStatus[entry.status]} aria-label={labelForStatus[entry.status]}>{iconForStatus[entry.status]}</span>
          <span class="file-path">{entry.relative_path}</span>
          <span class="file-size">{formatSize(entry.left_size)}</span>
          <span class="file-size">{formatSize(entry.right_size)}</span>
        </button>
      {:else}
        <p class="folder-empty">No files match this filter.</p>
      {/each}
    {/if}
  </div>
</section>

<style>
  .folder-diff { flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; background: var(--bg2); }
  .folder-controls { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; align-items: end; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .folder-controls .folder-picker { grid-column: 1 / -1; }
  .folder-controls .compare-button { grid-column: 1; }
  .folder-diff { container-type: inline-size; }
  .folder-picker { min-width: 0; display: grid; gap: 2px; }
  .folder-side { color: var(--fg2); font-size: 10px; font-weight: 600; }
  .folder-path, select, .compare-button, .clear-state-button { height: 25px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg); color: var(--fg); font: inherit; font-size: 11px; }
  .folder-path { min-width: 0; padding: 0 7px; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  .folder-path:hover, .compare-button:hover:not(:disabled) { border-color: var(--accent); }
  .compare-button, .clear-state-button { padding: 0 8px; cursor: pointer; background: color-mix(in srgb, var(--accent) 14%, var(--bg)); }
  .compare-button:disabled { cursor: default; opacity: .55; }
  select { padding: 0 5px; cursor: pointer; }
  .folder-controls .compare-button { grid-column: 1; grid-row: 3; }
  .folder-controls .clear-state-button { grid-column: 2; grid-row: 3; }
  .folder-controls select { grid-column: 1 / -1; grid-row: 4; padding-left: 24px; background-image: linear-gradient(var(--fg2), var(--fg2)); background-size: 12px 1px; background-position: 7px 12px; background-repeat: no-repeat; }
  .compare-button::before { content: "↔"; font-size: 14px; line-height: 1; }
  .clear-state-button::before { content: "×"; font-size: 16px; line-height: 1; }
  .compare-button, .clear-state-button { display: inline-flex; align-items: center; gap: 4px; }
  .folder-results { flex: 1; overflow: auto; font-family: 'JetBrains Mono','Consolas',monospace; font-size: 11px; }
  .folder-row { width: 100%; min-height: 25px; display: grid; grid-template-columns: 18px minmax(48px, 1fr) minmax(32px, 14%) minmax(32px, 14%); gap: clamp(3px, 1.4vw, 8px); align-items: center; padding: 2px 8px; border: 0; border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, transparent); background: transparent; color: var(--fg); text-align: left; font: inherit; cursor: pointer; }
  .folder-row:hover { background: var(--btn-hover); }
  .folder-row.different { background: color-mix(in srgb, var(--warning) 12%, transparent); }
  .folder-row.left-only { background: color-mix(in srgb, var(--diff-del-inline) 12%, transparent); }
  .folder-row.right-only { background: color-mix(in srgb, var(--diff-add-inline) 12%, transparent); }
  .folder-row.different:hover, .folder-row.left-only:hover, .folder-row.right-only:hover { background: color-mix(in srgb, var(--accent) 18%, var(--bg)); }
  .folder-row.equal { color: var(--fg2); }
  .status { color: var(--accent2); font-family: inherit; font-size: 13px; font-weight: 700; text-align: center; }
  .status.different { color: var(--warning); }.status.left-only { color: var(--diff-del-inline); }.status.right-only { color: var(--diff-add-inline); }
  .file-path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.file-size { color: var(--fg2); text-align: right; }
  .folder-empty, .folder-error { margin: 0; padding: 8px; font-size: 11px; color: var(--fg2); }.folder-error { color: var(--error); padding-bottom: 0; }
  @media (max-width: 420px) { .folder-row { grid-template-columns: 18px minmax(44px, 1fr) minmax(26px, 14%) minmax(26px, 14%); padding-inline: 5px; }.file-size { font-size: 10px; } }
  @media (max-width: 300px) { .folder-row { grid-template-columns: 18px minmax(0, 1fr); }.file-size { display: none; } }
  .folder-controls { grid-template-columns: minmax(0, 1fr) auto auto; }
  .folder-controls .folder-picker { grid-column: 1 / -1; }
  .folder-controls .compare-button { grid-column: 1; grid-row: 3; justify-self: start; }
  .folder-controls .clear-state-button { grid-column: 2; grid-row: 3; }
  .folder-controls .filter-group { grid-column: 3; grid-row: 3; }
  .filter-group { display: inline-flex; align-items: center; overflow: hidden; border: 1px solid var(--border); border-radius: 4px; background: var(--bg); }
  .filter-group button { width: 24px; height: 23px; padding: 0; border: 0; border-right: 1px solid var(--border); background: transparent; color: var(--fg2); font: inherit; font-size: 13px; cursor: pointer; }
  .filter-group button:last-child { border-right: 0; }
  .filter-group button:hover, .filter-group button.active { background: color-mix(in srgb, var(--accent) 22%, var(--bg)); color: var(--accent); }
  @container (max-width: 430px) { .compare-button, .clear-state-button { width: 25px; padding: 0; justify-content: center; font-size: 0; }.compare-button::before, .clear-state-button::before { font-size: 15px; } }
  .folder-controls { grid-template-columns: auto auto auto; justify-content: start; }
  .folder-controls .folder-picker { grid-column: 1 / -1; }
  .folder-controls .compare-button, .folder-controls .clear-state-button, .folder-controls .filter-group { justify-self: start; }
  @container (max-width: 380px) { .filter-group button { width: 21px; }.compare-button, .clear-state-button { width: 25px; } }
  .folder-controls { display: flex; flex-wrap: wrap; align-items: end; justify-content: flex-start; }
  .folder-controls .folder-picker { flex: 0 0 100%; width: 100%; }
  .folder-controls .compare-button, .folder-controls .clear-state-button, .folder-controls .filter-group { flex: 0 0 auto; }
</style>
