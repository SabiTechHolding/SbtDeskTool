<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { marked } from "marked";
  import CmEditor from "../components/CmEditor.svelte";
  import DeleteConfirm from "../components/DeleteConfirm.svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import { saveSetting } from "../stores/settings";

  let {
    compact, wordWrap, fontSize, autoSave: initialAutoSave, initialSidebarWidth, onZoom, onToggleWrap, onCursorChange, onStatusUpdate, onNotesChange,
  }: {
    compact: boolean;
    wordWrap: boolean;
    fontSize: number;
    autoSave: boolean;
    initialSidebarWidth: number;
    onZoom: (delta: number) => void;
    onToggleWrap: () => void;
    onCursorChange?: (line: number, col: number, selLen: number, chars: number) => void;
    onStatusUpdate?: (text: string, kind: string) => void;
    onNotesChange?: (notes: Array<{ id: number; title: string }>, selectedId: number | null) => void;
  } = $props();

  interface Note {
    id: number;
    title: string;
    body: string;
    created_at: string;
    updated_at: string;
  }

  let notes = $state<Note[]>([]);
  let currentNoteId = $state<number | null>(null);
  let title = $state("");
  let body = $state("");
  let autoSave = $state(true);
  $effect(() => { autoSave = initialAutoSave; });
  let isDirty = $state(false);
  let searchFilter = $state("");
  let showPreview = $state(false);
  let previewHtml = $state("");
  let showDeleteConfirm = $state(false);
  let sidebarWidth = $state(220);
  let panesEl: HTMLDivElement;
  $effect(() => { sidebarWidth = Math.max(180, initialSidebarWidth); });

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let noteEditor = $state<CmEditor>();
  $effect(() => {
    onNotesChange?.(notes.map((note) => ({ id: note.id, title: note.title })), currentNoteId);
  });

  function handleDropEvent(e: Event) {
    const detail = (e as CustomEvent).detail;
    if (typeof detail === "string") {
      body = detail;
      isDirty = true;
      scheduleAutoSave();
      updatePreview();
    }
  }

  function handleAppFlush(event?: Event) {
    if (!autoSave || !isDirty) return;
    const task = maybeSave();
    const detail = (event as CustomEvent<{ tasks?: Promise<unknown>[] }> | undefined)?.detail;
    detail?.tasks?.push(task);
  }

  onMount(async () => {
    document.addEventListener("note:setContent", handleDropEvent);
    document.addEventListener("app:flush", handleAppFlush);
    try {
      notes = await invoke<Note[]>("list_notes");
    } catch (error) {
      onStatusUpdate?.(`Cannot load notes: ${error}`, "error");
    }
    if (notes.length > 0) {
      selectNote(notes[0].id);
    }
  });

  onDestroy(() => {
    document.removeEventListener("note:setContent", handleDropEvent);
    document.removeEventListener("app:flush", handleAppFlush);
    if (debounceTimer) clearTimeout(debounceTimer);
    handleAppFlush();
  });

  function currentNote() {
    return notes.find((n) => n.id === currentNoteId) ?? null;
  }

  async function selectNote(id: number) {
    if (id === currentNoteId) return;
    if (isDirty && autoSave) await maybeSave();
    const note = notes.find((n) => n.id === id);
    if (note) {
      currentNoteId = id;
      title = note.title;
      body = note.body;
      isDirty = false;
      updatePreview();
    }
  }

  export function selectNoteById(id: number) {
    void selectNote(id);
  }

  async function newNote() {
    if (isDirty && autoSave) await maybeSave();
    try {
      const now = Date.now();
      const note: Note = {
        id: now,
        title: "New Note",
        body: "",
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      await invoke("save_note", { note });
      notes = [...notes, note];
      await selectNote(note.id);
      onStatusUpdate?.("Note created", "success");
    } catch (error) {
      onStatusUpdate?.(`Cannot create note: ${error}`, "error");
    }
  }

  function requestDelete() {
    if (currentNoteId !== null) {
      showDeleteConfirm = true;
    }
  }

  async function confirmDelete() {
    showDeleteConfirm = false;
    if (currentNoteId === null) return;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    try {
      const deletedIndex = notes.findIndex((note) => note.id === currentNoteId);
      await invoke("delete_note", { id: currentNoteId });
      notes = notes.filter((n) => n.id !== currentNoteId);
      const nextIndex = Math.min(Math.max(deletedIndex, 0), notes.length - 1);
      currentNoteId = notes.length > 0 ? notes[nextIndex].id : null;
      if (currentNoteId) {
        const n = currentNote();
        title = n?.title ?? "";
        body = n?.body ?? "";
      } else {
        title = "";
        body = "";
      }
      isDirty = false;
      updatePreview();
      onStatusUpdate?.("Note deleted", "success");
    } catch (error) {
      onStatusUpdate?.(`Cannot delete note: ${error}`, "error");
    }
  }

  async function maybeSave() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (currentNoteId === null) {
      if (!title.trim() && !body) return;
      const now = Date.now();
      const note: Note = {
        id: now,
        title: title.trim() || "Untitled",
        body,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      try {
        await invoke("save_note", { note });
        notes = [...notes, note];
        currentNoteId = note.id;
        isDirty = false;
        onStatusUpdate?.("Note created", "success");
      } catch (error) {
        onStatusUpdate?.(`Cannot create note: ${error}`, "error");
      }
      return;
    }
    const note = currentNote();
    if (!note) return;
    try {
      const updatedAt = new Date().toISOString();
      await invoke("save_note", {
        note: { ...note, title, body, updated_at: updatedAt },
      });
      notes = notes.map((n) => n.id === currentNoteId ? { ...n, title, body, updated_at: updatedAt } : n);
      isDirty = false;
      onStatusUpdate?.("Note saved", "success");
    } catch (error) {
      onStatusUpdate?.(`Cannot save note: ${error}`, "error");
    }
  }

  function onTitleInput() {
    isDirty = true;
    scheduleAutoSave();
  }

  function onBodyChange(text: string) {
    body = text;
    isDirty = true;
    scheduleAutoSave();
    updatePreview();
  }

  function scheduleAutoSave() {
    if (!autoSave) return;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(maybeSave, 2000);
  }

  function toggleAutoSave() {
    saveSetting("note_auto_save", autoSave);
    if (!autoSave && debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    } else if (autoSave && isDirty) {
      scheduleAutoSave();
    }
  }

  function updatePreview() {
    try {
      previewHtml = marked.parse(body) as string;
    } catch {
      previewHtml = body;
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey && (e.key === "=" || e.key === "-")) {
      e.preventDefault();
      onZoom(e.key === "=" ? 1 : -1);
    }
    if (e.ctrlKey && e.key === "Enter") {
      e.preventDefault();
      void maybeSave();
    }
  }

  function filterNotes() {
    if (!searchFilter) return notes;
    return notes.filter((n) =>
      n.title.toLowerCase().includes(searchFilter.toLowerCase()) ||
      n.body.toLowerCase().includes(searchFilter.toLowerCase())
    );
  }

  async function copyText(text: string) {
    try {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
    } catch {
      await navigator.clipboard.writeText(text);
    }
    onStatusUpdate?.("Copied", "success");
  }

  function formatDate(iso: string) {
    try {
      return new Date(iso).toLocaleDateString();
    } catch {
      return "";
    }
  }

  function startSidebarDrag(event: MouseEvent) {
    event.preventDefault();
    const rect = panesEl.getBoundingClientRect();
    function onMove(moveEvent: MouseEvent) {
      sidebarWidth = Math.max(180, Math.min(rect.width - 260, rect.right - moveEvent.clientX));
    }
    function onUp() {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      void saveSetting("note_sidebar_width", Math.round(sidebarWidth));
    }
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }
</script>

<div class="note-tab">
  <div class="panes" bind:this={panesEl}>
    <div class="editor-pane">
      {#if !compact}
      <div class="editor-header">
        <input
          type="text"
          class="title-input"
          bind:value={title}
          placeholder="Note title..."
          oninput={onTitleInput}
        />
        <div class="editor-actions">
          <button class="pane-btn" class:active={showPreview} onclick={() => (showPreview = !showPreview)} title="Toggle Preview">
            <AppIcon name="preview" size={12} /><span class="btn-label">Preview</span>
          </button>
          <label class="auto-toggle">
            <input type="checkbox" bind:checked={autoSave} onchange={toggleAutoSave} /><span class="auto-label">Auto</span>
          </label>
          <span class="save-state" class:dirty={isDirty}>
            {isDirty ? "●" : "✓"}
          </span>
          <button class="pane-btn" onclick={maybeSave} title="Save Note"><AppIcon name="save" size={12} /><span class="btn-label">Save</span></button>
          <button class="pane-btn" onclick={() => copyText(body)} title="Copy Note"><AppIcon name="copy" size={12} /><span class="btn-label">Copy</span></button>
          <button class="pane-btn" class:active={wordWrap} onclick={onToggleWrap} title="Toggle word wrap for Notes"><AppIcon name="wrap" size={12} /><span class="btn-label">Wrap</span></button>
        </div>
      </div>
      {/if}
      {#if showPreview && !compact}
        <div class="preview" style="font-size: {fontSize}px">
          {@html previewHtml}
        </div>
      {:else}
        <CmEditor
          bind:this={noteEditor}
          value={body}
          {fontSize}
          {wordWrap}
          showLineNumbers={true}
          language="markdown"
          onChange={onBodyChange}
          onKeyDown={handleKeyDown}
          onCursorChange={(line, col, selLen, chars) => onCursorChange?.(line, col, selLen, chars)}
          onWheel={(e) => { e.preventDefault(); onZoom(e.deltaY < 0 ? 1 : -1); }}
        />
      {/if}
    </div>

    {#if !compact}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="sidebar-divider" role="separator" aria-orientation="vertical" onmousedown={startSidebarDrag}></div>
    <div class="sidebar" style:width={`${sidebarWidth}px`}>
      <div class="sidebar-header">
        <span class="sidebar-title"><AppIcon name="notes" size={14} /> Notes <span class="note-count">{notes.length}</span></span>
        <div class="sidebar-actions">
          <button class="toolbar-btn" onclick={newNote} title="New note" aria-label="New note"><AppIcon name="add" size={14} /></button>
          <button class="toolbar-btn danger" onclick={requestDelete} title="Delete selected note" aria-label="Delete selected note" disabled={!currentNoteId}><AppIcon name="delete" size={14} /></button>
        </div>
      </div>
      <label class="filter-box">
        <AppIcon name="search" size={13} />
        <input type="text" class="sidebar-filter" placeholder="Filter notes..." bind:value={searchFilter} />
      </label>
      <div class="note-list">
        {#each filterNotes() as note (note.id)}
          <button
            class="note-item"
            class:active={note.id === currentNoteId}
            onclick={() => selectNote(note.id)}
          >
            <span class="note-title">{note.title || "Untitled"}</span>
            <span class="note-preview">{note.body.split("\n")[0] || ""}</span>
            <span class="note-date">{formatDate(note.updated_at)}</span>
          </button>
        {/each}
      </div>
    </div>
    {/if}
  </div>
</div>

{#if showDeleteConfirm}
  <DeleteConfirm
    title="Delete Note"
    message="Are you sure you want to delete this note? This cannot be undone."
    onConfirm={confirmDelete}
    onCancel={() => (showDeleteConfirm = false)}
  />
{/if}

<style>
  .note-tab {
    display: flex;
    flex: 1;
    flex-direction: column;
    overflow: hidden;
  }

  .panes {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .editor-pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .editor-header {
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0 8px;
    background: var(--bg3);
    gap: 6px;
    flex-shrink: 0;
  }

  .title-input {
    flex: 1;
    min-width: 0;
    padding: 3px 6px;
    background: var(--bg3);
    color: var(--fg);
    border: none;
    outline: none;
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
  }

  .title-input:focus {
    background: var(--bg2);
  }

  .editor-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
  }

  .auto-toggle {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 11px;
    color: var(--fg2);
    cursor: pointer;
  }

  .auto-toggle input { accent-color: var(--accent); }

  .save-state {
    font-size: 11px;
    color: var(--success);
  }

  .save-state.dirty {
    color: var(--warning);
  }

  .pane-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    height: var(--control-height);
    padding: 0 var(--control-padding-x);
    background: transparent;
    border: none;
    color: var(--fg2);
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    white-space: nowrap;
    flex: 0 0 auto;
    cursor: pointer;
    border-radius: var(--control-radius);
  }

  .pane-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .pane-btn:disabled { opacity: 0.4; cursor: default; }
  .pane-btn.active { background: var(--accent); color: var(--bg); }

  .preview {
    flex: 1;
    padding: 8px 12px;
    background: var(--bg);
    color: var(--fg);
    overflow: auto;
    line-height: 1.6;
  }

  .preview :global(h1) { font-size: 1.5em; margin: 0.5em 0; }
  .preview :global(h2) { font-size: 1.3em; margin: 0.4em 0; }
  .preview :global(h3) { font-size: 1.15em; margin: 0.3em 0; }
  .preview :global(p) { margin: 0.5em 0; }
  .preview :global(code) {
    background: var(--bg2);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: 'JetBrains Mono', 'Consolas', monospace;
    font-size: 0.9em;
  }
  .preview :global(pre) {
    background: var(--bg2);
    padding: 8px;
    border-radius: 4px;
    overflow-x: auto;
  }
  .preview :global(pre code) {
    background: none;
    padding: 0;
  }
  .preview :global(blockquote) {
    border-left: 3px solid var(--accent);
    padding-left: 8px;
    margin: 0.5em 0;
    color: var(--fg2);
  }
  .preview :global(ul), .preview :global(ol) {
    padding-left: 20px;
    margin: 0.3em 0;
  }
  .preview :global(a) { color: var(--accent); }

  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--bg2);
    flex-shrink: 0;
  }

  .sidebar-divider {
    width: 5px;
    flex-shrink: 0;
    cursor: col-resize;
    background: var(--border);
  }

  .sidebar-divider:hover { background: var(--accent); }

  .sidebar-header {
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0 6px 0 8px;
    background: var(--bg3);
    border-bottom: 1px solid var(--border);
    gap: 6px;
  }

  .sidebar-title { display: inline-flex; align-items: center; gap: 5px; min-width: 0; color: var(--fg); font-size: 11px; font-weight: 600; }
  .note-count { min-width: 16px; padding: 0 4px; border-radius: 8px; background: var(--bg2); color: var(--fg2); font-size: 9px; line-height: 15px; text-align: center; }
  .sidebar-actions { display: flex; align-items: center; gap: 2px; margin-left: auto; }

  .toolbar-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height);
    height: var(--control-height);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--control-radius);
    background: transparent;
    color: var(--fg2);
    cursor: pointer;
  }
  .toolbar-btn:hover:not(:disabled) { background: var(--btn-hover); border-color: var(--border); color: var(--fg); }
  .toolbar-btn.danger:hover:not(:disabled) { color: var(--error); }
  .toolbar-btn:disabled { opacity: 0.35; cursor: default; }

  .filter-box {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    margin: 6px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--fg2);
  }
  .filter-box:focus-within { border-color: var(--accent); }

  .sidebar-filter {
    flex: 1;
    min-width: 0;
    padding: 0;
    background: transparent;
    color: var(--fg);
    border: none;
    font-family: inherit;
    font-size: 11px;
    outline: none;
  }

  .note-list {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px 6px;
  }

  .note-item {
    display: block;
    width: 100%;
    margin-bottom: 3px;
    padding: 7px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    text-align: left;
    cursor: pointer;
  }

  .note-item:hover {
    background: var(--btn-hover);
    border-color: var(--border);
  }

  .note-item.active {
    background: var(--accent);
    border-color: var(--accent);
  }

  .note-item.active .note-title,
  .note-item.active .note-preview,
  .note-item.active .note-date {
    color: var(--bg);
  }

  .note-title {
    display: block;
    font-size: 12px;
    font-weight: 600;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-preview {
    display: block;
    margin-top: 2px;
    font-size: 10px;
    color: var(--fg2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-date {
    display: block;
    margin-top: 3px;
    font-size: 9px;
    color: var(--fg2);
    opacity: 0.7;
  }

  @media (max-width: 680px) {
    .editor-header { padding-inline: 4px; gap: 3px; }
    .editor-actions { gap: 1px; }
    .pane-btn { width: var(--control-height); min-width: var(--control-height); padding: 0; }
    .btn-label, .auto-label { display: none; }
    .auto-toggle { width: var(--control-height); justify-content: center; }
  }
</style>
