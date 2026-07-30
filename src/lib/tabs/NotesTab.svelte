<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { marked } from "marked";
  import CmEditor from "../components/CmEditor.svelte";
  import DeleteConfirm from "../components/DeleteConfirm.svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import { saveSetting } from "../stores/settings";

  let {
    compact, wordWrap, showWhitespace, fontSize, autoSave: initialAutoSave, initialSidebarWidth, onZoom, onToggleWrap, onToggleWhitespace, onCursorChange, onStatusUpdate, onNotesChange,
  }: {
    compact: boolean;
    wordWrap: boolean;
    showWhitespace: boolean;
    fontSize: number;
    autoSave: boolean;
    initialSidebarWidth: number;
    onZoom: (delta: number) => void;
    onToggleWrap: () => void;
    onToggleWhitespace: () => void;
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
  let draggedNoteId = $state<number | null>(null);
  let dropTargetId = $state<number | null>(null);
  let dropAfter = $state(false);
  let pendingDragNoteId: number | null = null;
  let dragStartY = 0;
  let suppressNoteClick = false;
  let panesEl: HTMLDivElement;
  let noteListEl = $state<HTMLDivElement>();
  $effect(() => { sidebarWidth = Math.max(180, initialSidebarWidth); });

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let noteEditor = $state<CmEditor>();
  let unlistenNotesUpdated: UnlistenFn | undefined;

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

  function mergeText(local: string, remote: string): string {
    if (local === remote) return local;
    if (!local.trim()) return remote;
    if (!remote.trim()) return local;
    if (remote.includes(local)) return remote;
    if (local.includes(remote)) return local;

    const linesLocal = local.split("\n");
    const linesRemote = remote.split("\n");
    const merged: string[] = [];
    let idxL = 0;
    let idxR = 0;

    while (idxL < linesLocal.length || idxR < linesRemote.length) {
      if (idxL < linesLocal.length && idxR < linesRemote.length) {
        if (linesLocal[idxL] === linesRemote[idxR]) {
          merged.push(linesLocal[idxL]);
          idxL++;
          idxR++;
        } else {
          const inRemotePos = linesRemote.slice(idxR).indexOf(linesLocal[idxL]);
          const inLocalPos = linesLocal.slice(idxL).indexOf(linesRemote[idxR]);
          if (inLocalPos !== -1 && (inRemotePos === -1 || inLocalPos <= inRemotePos)) {
            merged.push(linesLocal[idxL]);
            idxL++;
          } else if (inRemotePos !== -1) {
            merged.push(linesRemote[idxR]);
            idxR++;
          } else {
            merged.push(linesLocal[idxL]);
            merged.push(linesRemote[idxR]);
            idxL++;
            idxR++;
          }
        }
      } else if (idxL < linesLocal.length) {
        merged.push(linesLocal[idxL]);
        idxL++;
      } else {
        merged.push(linesRemote[idxR]);
        idxR++;
      }
    }
    return merged.join("\n");
  }

  async function refreshNotesFromDisk() {
    try {
      const updatedNotes = await invoke<Note[]>("list_notes");
      notes = updatedNotes;
      if (currentNoteId !== null) {
        const updated = updatedNotes.find((n) => n.id === currentNoteId);
        if (updated) {
          if (!isDirty) {
            body = updated.body;
            title = updated.title;
            updatePreview();
          } else {
            const merged = mergeText(body, updated.body);
            if (merged !== body) {
              body = merged;
              updatePreview();
            }
          }
        }
      }
    } catch {}
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
    try {
      unlistenNotesUpdated = await listen("notes-updated", () => {
        void refreshNotesFromDisk();
      });
    } catch {}
  });

  onDestroy(() => {
    document.removeEventListener("note:setContent", handleDropEvent);
    document.removeEventListener("app:flush", handleAppFlush);
    unlistenNotesUpdated?.();
    if (debounceTimer) clearTimeout(debounceTimer);
    removeNotePointerListeners();
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

  function startNotePointerDrag(event: PointerEvent, noteId: number) {
    if (event.button !== 0) return;
    pendingDragNoteId = noteId;
    dragStartY = event.clientY;
    draggedNoteId = null;
    dropTargetId = null;
    event.currentTarget.setPointerCapture(event.pointerId);
    document.addEventListener("pointermove", moveNotePointerDrag, { passive: false });
    document.addEventListener("pointerup", finishNotePointerDrag);
    document.addEventListener("pointercancel", finishNotePointerDrag);
  }

  function moveNotePointerDrag(event: PointerEvent) {
    if (pendingDragNoteId === null) return;
    if (draggedNoteId === null && Math.abs(event.clientY - dragStartY) < 4) return;
    event.preventDefault();
    draggedNoteId = pendingDragNoteId;

    const listBounds = noteListEl?.getBoundingClientRect();
    if (listBounds && event.clientY < listBounds.top + 24) noteListEl.scrollTop -= 8;
    else if (listBounds && event.clientY > listBounds.bottom - 24) noteListEl.scrollTop += 8;

    const target = document.elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>(".note-item[data-note-id]");
    const noteId = Number(target?.dataset.noteId);
    if (!target || !Number.isFinite(noteId) || noteId === draggedNoteId) {
      dropTargetId = null;
      return;
    }
    const bounds = target.getBoundingClientRect();
    dropTargetId = noteId;
    dropAfter = event.clientY >= bounds.top + bounds.height / 2;
  }

  function finishNotePointerDrag() {
    const draggedId = draggedNoteId;
    const targetId = dropTargetId;
    const insertAfter = dropAfter;
    removeNotePointerListeners();
    pendingDragNoteId = null;
    draggedNoteId = null;
    dropTargetId = null;
    dropAfter = false;

    if (draggedId !== null) {
      suppressNoteClick = true;
      setTimeout(() => { suppressNoteClick = false; }, 0);
    }
    if (draggedId !== null && targetId !== null && draggedId !== targetId) {
      void reorderNote(draggedId, targetId, insertAfter);
    }
  }

  function removeNotePointerListeners() {
    document.removeEventListener("pointermove", moveNotePointerDrag);
    document.removeEventListener("pointerup", finishNotePointerDrag);
    document.removeEventListener("pointercancel", finishNotePointerDrag);
  }

  function handleNoteClick(event: MouseEvent, noteId: number) {
    if (suppressNoteClick) {
      event.preventDefault();
      suppressNoteClick = false;
      return;
    }
    void selectNote(noteId);
  }

  async function reorderNote(draggedId: number, targetId: number, insertAfter: boolean) {
    if (isDirty && autoSave) await maybeSave();

    const previousNotes = notes;
    const draggedNote = notes.find((note) => note.id === draggedId);
    if (!draggedNote) return;
    const reordered = notes.filter((note) => note.id !== draggedId);
    const targetIndex = reordered.findIndex((note) => note.id === targetId);
    if (targetIndex < 0) return;
    reordered.splice(targetIndex + (insertAfter ? 1 : 0), 0, draggedNote);
    notes = reordered;

    try {
      await invoke("reorder_notes", { ids: reordered.map((note) => note.id) });
      onStatusUpdate?.("Notes reordered", "success");
    } catch (error) {
      notes = previousNotes;
      onStatusUpdate?.(`Cannot reorder notes: ${error}`, "error");
    }
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
            <AppIcon name="preview" size={14} /><span class="btn-label">Preview</span>
          </button>
          <label class="auto-toggle">
            <input type="checkbox" bind:checked={autoSave} onchange={toggleAutoSave} /><span class="auto-label">Auto</span>
          </label>
          <span class="save-state" class:dirty={isDirty}>
            {isDirty ? "●" : "✓"}
          </span>
          <button class="pane-btn" onclick={maybeSave} title="Save Note"><AppIcon name="save" size={14} /><span class="btn-label">Save</span></button>
          <button class="pane-btn" onclick={() => copyText(body)} title="Copy Note"><AppIcon name="copy" size={14} /><span class="btn-label">Copy</span></button>
          <div class="control-group" aria-label="Notes editor display">
            <button class="control-btn" class:toggled={wordWrap} onclick={onToggleWrap} title="Toggle word wrap for Notes"><AppIcon name="wrap" size={14} /><span class="btn-label">Wrap</span></button>
            <button class="control-btn" class:toggled={showWhitespace} onclick={onToggleWhitespace} title="Show or hide whitespace characters"><AppIcon name="whitespace" size={14} /><span class="btn-label">Show WS</span></button>
          </div>
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
          {showWhitespace}
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
      <div class="note-list" bind:this={noteListEl}>
        {#each filterNotes() as note (note.id)}
          <button
            class="note-item"
            class:active={note.id === currentNoteId}
            class:dragging={note.id === draggedNoteId}
            class:drop-before={note.id === dropTargetId && !dropAfter}
            class:drop-after={note.id === dropTargetId && dropAfter}
            data-note-id={note.id}
            onclick={(event) => handleNoteClick(event, note.id)}
            onpointerdown={(event) => startNotePointerDrag(event, note.id)}
            title="Drag to reorder"
          >
            <span class="note-title">{note.title || "Untitled"}</span>
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
  .pane-btn :global(.app-icon), .control-btn :global(.app-icon) { width: 15px; height: 15px; }
  .pane-btn:disabled { opacity: 0.4; cursor: default; }
  .pane-btn.active { background: var(--accent); color: var(--bg); }
  .control-group { display: inline-flex; align-items: center; overflow: hidden; border: 1px solid var(--border); border-radius: 4px; }
  .control-btn { display: inline-flex; align-items: center; justify-content: center; gap: 4px; height: var(--control-height); min-width: var(--control-height); padding: 0 6px; border: 0; border-right: 1px solid var(--border); background: var(--bg2); color: var(--fg2); font: inherit; font-size: 10px; line-height: 1; white-space: nowrap; cursor: pointer; }
  .control-btn:last-child { border-right: 0; }
  .control-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .control-btn.toggled { background: color-mix(in srgb, var(--accent) 22%, var(--bg2)); color: var(--accent); }

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
    margin-bottom: 2px;
    padding: 4px 7px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    text-align: left;
    cursor: grab;
    position: relative;
    user-select: none;
    touch-action: none;
  }

  .note-item:active { cursor: grabbing; }
  .note-item.dragging { opacity: 0.45; }
  .note-item.drop-before::before,
  .note-item.drop-after::after {
    content: "";
    position: absolute;
    left: 2px;
    right: 2px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .note-item.drop-before::before { top: -2px; }
  .note-item.drop-after::after { bottom: -2px; }

  .note-item:hover {
    background: var(--btn-hover);
    border-color: var(--border);
  }

  .note-item.active {
    background: var(--accent);
    border-color: var(--accent);
  }

  .note-item.active .note-title,
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

  .note-date {
    display: block;
    margin-top: 1px;
    font-size: 9px;
    color: var(--fg2);
    opacity: 0.7;
  }

  @media (max-width: 680px) {
    .editor-header { padding-inline: 4px; gap: 3px; }
    .editor-actions { gap: 1px; }
    .pane-btn, .control-btn { width: var(--control-height); min-width: var(--control-height); padding: 0; }
    .btn-label, .auto-label { display: none; }
    .auto-toggle { width: var(--control-height); justify-content: center; }
  }
</style>
