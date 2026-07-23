<script lang="ts">
  import CmEditor from "../components/CmEditor.svelte";
  import type { EditorScrollState } from "../components/CmEditor.svelte";
  import ContextMenu from "../components/ContextMenu.svelte";
  import type { ContextItem } from "../components/ContextMenu.svelte";
  import PopupDict from "../components/PopupDict.svelte";
  import FindBar from "../components/FindBar.svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import { loadSettings, saveSetting } from "../stores/settings";
  import { langNameFromCode, mapLang } from "../utils/languages";
  import { onMount, onDestroy } from "svelte";

  let {
    layout, compact, wordWrap, showWhitespace, fontSize, srcLang, destLang, onZoom, onCursorChange, onStatusUpdate, sashPos: initialSash = 50,
  }: {
    layout: "horizontal" | "vertical";
    compact: boolean;
    wordWrap: boolean;
    showWhitespace: boolean;
    fontSize: number;
    srcLang: string;
    destLang: string;
    onZoom: (delta: number) => void;
    onCursorChange?: (side: string, line: number, col: number, selLen: number, chars: number) => void;
    onStatusUpdate?: (text: string, kind: string, transTime: number, transChars: number) => void;
    sashPos?: number;
  } = $props();

  let sourceText = $state("");
  let translatedText = $state("");
  let detectedLang = $state("");
  let isTranslating = $state(false);
  let findOpen = $state(false);
  let findBar = $state<FindBar>();
  let sourceFindOpen = $state(false);
  let translatedFindOpen = $state(false);

  // Unit-based incremental translation state
  interface TranUnit {
    source: string;
    translated: string;
    detectedLang?: string;
  }
  let tranUnits = $state<TranUnit[]>([]);
  let prevSignature = $state(""); // srcLang+destLang to detect engine change

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let sourceSaveTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingTranslate = false;
  let sourceTouched = false;
  let disposed = false;

  let sashPos = $state(50);
  $effect(() => { sashPos = initialSash; });
  let isDragging = $state(false);
  let sashEl: HTMLDivElement;

  let startTime = $state(0);
  let lastSrcLen = $state(0);

  // ── Unit-based incremental translation ────────────────────
  function splitUnits(text: string): string[] {
    // One physical line is one reusable unit. The previous implementation
    // merged every adjacent non-empty line into a paragraph, so editing one
    // line incorrectly marked the whole paragraph as new.
    return text.match(/.*(?:\r\n|\n|\r)|.+$/g) ?? (text ? [text] : []);
  }

  let languageSignature = "";
  $effect(() => {
    const signature = `${srcLang}|${destLang}`;
    if (srcLang !== "Auto Detect") {
      detectedLang = sourceText.trim() ? srcLang : "";
    } else if (languageSignature && !languageSignature.startsWith("Auto Detect|")) {
      detectedLang = "";
    }
    if (languageSignature && signature !== languageSignature && sourceText.trim()) {
      if (debounceTimer) clearTimeout(debounceTimer);
      queueMicrotask(doTranslate);
    }
    languageSignature = signature;
  });

  async function doTranslate() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (!sourceText.trim()) {
      translatedText = "";
      detectedLang = "";
      tranUnits = [];
      prevSignature = "";
      onStatusUpdate?.("Ready", "normal", 0, 0);
      return;
    }
    if (isTranslating) {
      pendingTranslate = true;
      return;
    }
    isTranslating = true;
    pendingTranslate = false;
    const requestText = sourceText;
    onStatusUpdate?.("Translating…", "warning", 0, requestText.length);
    startTime = performance.now();
    lastSrcLen = sourceText.length;

    const signature = `${srcLang}|${destLang}`;
    const isNewEngine = signature !== prevSignature;

    const units = splitUnits(requestText);
    const newUnits: TranUnit[] = [];
    let newCount = 0;
    let reusedCount = 0;

    try {
      const { invoke } = await import("@tauri-apps/api/core");

      // Collect units that need fresh translation
      const toTranslate: string[] = [];
      const translateIndices: number[] = [];

      const reusable = new Map<string, TranUnit[]>();
      if (!isNewEngine) {
        for (const unit of tranUnits) {
          const core = unit.source.replace(/[\r\n]+$/, "");
          const matches = reusable.get(core) ?? [];
          matches.push(unit);
          reusable.set(core, matches);
        }
      }

      for (let i = 0; i < units.length; i++) {
        const src = units[i];
        const core = src.replace(/[\r\n]+$/, "");
        const cached = reusable.get(core)?.shift() ?? null;

        if (cached && src.trim()) {
          const trailing = src.match(/[\r\n]+$/)?.[0] ?? "";
          const translatedCore = cached.translated.replace(/[\r\n]+$/, "");
          newUnits.push({
            source: src,
            translated: translatedCore + trailing,
            detectedLang: cached.detectedLang,
          });
          reusedCount++;
        } else if (src.trim()) {
          toTranslate.push(src.replace(/[\r\n]+$/, ""));
          translateIndices.push(i);
          newUnits.push({ source: src, translated: "" }); // placeholder
        } else {
          // Blank line — keep as-is
          newUnits.push({ source: src, translated: src });
        }
      }

      // Batch translate new units
      if (toTranslate.length > 0) {
        const src = srcLang === "Auto Detect" ? "auto" : mapLang(srcLang);
        const dest = mapLang(destLang);

        const results = await invoke<Array<{ translated: string; detected_lang: string | null }>>(
          "translate_units", { texts: toTranslate, src, dest }
        );

        for (let j = 0; j < translateIndices.length; j++) {
          const idx = translateIndices[j];
          const r = results[j];
          if (!r) throw new Error("Translation service returned an incomplete result");
          const trailing = units[idx].match(/[\r\n]+$/)?.[0] ?? "";
          newUnits[idx].translated = r.translated + trailing;
          if (r.detected_lang && r.detected_lang !== "auto") {
            newUnits[idx].detectedLang = langNameFromCode(r.detected_lang);
          }
        }
        newCount = toTranslate.length;
      }

      // Reassemble
      if (sourceText !== requestText || `${srcLang}|${destLang}` !== signature) {
        pendingTranslate = true;
        return;
      }
      translatedText = newUnits.map(u => u.translated).join("");
      if (srcLang === "Auto Detect") {
        const languageWeights = new Map<string, number>();
        for (const unit of newUnits) {
          if (!unit.detectedLang) continue;
          const weight = Math.max(1, unit.source.replace(/\s/g, "").length);
          languageWeights.set(
            unit.detectedLang,
            (languageWeights.get(unit.detectedLang) ?? 0) + weight,
          );
        }
        detectedLang = [...languageWeights.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
      } else {
        detectedLang = srcLang;
      }
      tranUnits = newUnits;
      prevSignature = signature;

      // Status update with stats
      const elapsed = performance.now() - startTime;
      onStatusUpdate?.(
        newCount > 0
          ? `Translated: ${newCount} new, ${reusedCount} reused`
          : `Translated: all ${reusedCount} reused`,
        "success",
        elapsed,
        requestText.length
      );

    } catch (e) {
      if (sourceText === requestText && `${srcLang}|${destLang}` === signature) {
        onStatusUpdate?.(`Error: ${e}`, "error", 0, 0);
      }
    } finally {
      isTranslating = false;
      if (pendingTranslate || sourceText !== requestText) {
        pendingTranslate = false;
        if (debounceTimer) {
          clearTimeout(debounceTimer);
          debounceTimer = null;
        }
        queueMicrotask(doTranslate);
      }
    }
  }

  function onSourceChange(text: string) {
    sourceTouched = true;
    sourceText = text;
    queueMicrotask(() => findBar?.refresh());
    scheduleSourceSave();
    scheduleTranslate();
  }

  function scheduleSourceSave() {
    if (sourceSaveTimer) clearTimeout(sourceSaveTimer);
    sourceSaveTimer = setTimeout(() => {
      sourceSaveTimer = null;
      void saveSetting("tran_source_text", sourceText);
    }, 350);
  }

  function flushSource(): Promise<void> {
    if (sourceSaveTimer) {
      clearTimeout(sourceSaveTimer);
      sourceSaveTimer = null;
    }
    return saveSetting("tran_source_text", sourceText);
  }

  function handleAppFlush(event: Event) {
    const task = flushSource();
    const detail = (event as CustomEvent<{ tasks?: Promise<unknown>[] }>).detail;
    detail?.tasks?.push(task);
  }

  function scheduleTranslate() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(doTranslate, 700);
  }

  export function openCommonFind() {
    findOpen = true;
    queueMicrotask(() => findBar?.focus());
  }

  function handleSourceKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey && (e.key === "=" || e.key === "-")) {
      e.preventDefault();
      onZoom(e.key === "=" ? 1 : -1);
    }
  }

  function handleWheel(e: WheelEvent) {
    if (e.ctrlKey) {
      e.preventDefault();
      onZoom(e.deltaY < 0 ? 1 : -1);
    }
  }

  function handleCursorChange(side: string, line: number, col: number, selLen: number, chars: number) {
    onCursorChange?.(side, line, col, selLen, chars);
  }

  function syncEditorScroll(side: "source" | "translated", state: EditorScrollState) {
    if (side === "source") translatedEditor?.syncScroll(state);
    else sourceEditor?.syncScroll(state);
  }

  let sourceEditor: CmEditor;
  let translatedEditor: CmEditor;
  let findIndex = -1;
  let lastFindSignature = "";

  async function toggleEditorFind(side: "source" | "translated") {
    const visible = await (side === "source" ? sourceEditor : translatedEditor)?.toggleFind() ?? false;
    if (side === "source") sourceFindOpen = visible;
    else translatedFindOpen = visible;
  }

  function handleEditorFindVisibility(side: "source" | "translated", visible: boolean) {
    if (side === "source") sourceFindOpen = visible;
    else translatedFindOpen = visible;
  }

  function handleFind(
    query: string,
    flags: { caseSensitive: boolean; wholeWord: boolean; regex: boolean },
    direction: "next" | "prev" | "refresh",
  ) {
    sourceEditor?.highlightMatches(query, flags);
    translatedEditor?.highlightMatches(query, flags);
    const counts = [sourceEditor?.countMatches(query, flags) ?? 0, translatedEditor?.countMatches(query, flags) ?? 0];
    const total = counts[0] + counts[1];
    const signature = `${query}|${flags.caseSensitive}|${flags.wholeWord}|${flags.regex}`;
    if (!total) { findIndex = -1; return { count: 0, index: 0 }; }
    if (direction === "refresh") {
      findIndex = signature === lastFindSignature && findIndex >= 0 ? Math.min(findIndex, total - 1) : 0;
      lastFindSignature = signature;
      return { count: total, index: findIndex + 1 };
    }
    if (signature !== lastFindSignature) findIndex = direction === "next" ? -1 : 0;
    findIndex = (findIndex + (direction === "next" ? 1 : -1) + total) % total;
    lastFindSignature = signature;
    if (findIndex < counts[0]) sourceEditor.selectMatch(query, flags, findIndex);
    else translatedEditor.selectMatch(query, flags, findIndex - counts[0]);
    return { count: total, index: findIndex + 1 };
  }

  function clearSrc() {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    pendingTranslate = false;
    sourceText = "";
    sourceTouched = true;
    translatedText = "";
    detectedLang = "";
    tranUnits = [];
    prevSignature = "";
    queueMicrotask(() => findBar?.refresh());
    scheduleSourceSave();
    onStatusUpdate?.("Cleared", "normal", 0, 0);
  }

  async function copyText(text: string) {
    try {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
    } catch {
      await navigator.clipboard.writeText(text);
    }
    onStatusUpdate?.("Copied", "success", 0, 0);
  }

  export function getDetectedLanguage() {
    return detectedLang;
  }

  let ctxMenu = $state<{ items: ContextItem[]; x: number; y: number } | null>(null);
  let popupDict = $state<{ text: string; x: number; y: number } | null>(null);

  function handleDropEvent(e: Event) {
    const detail = (e as CustomEvent).detail;
    if (typeof detail === "string") {
      sourceTouched = true;
      sourceText = detail;
      scheduleSourceSave();
      scheduleTranslate();
    }
  }

  onMount(() => {
    document.addEventListener("tran:setSource", handleDropEvent);
    document.addEventListener("app:flush", handleAppFlush);
    void loadSettings().then((settings) => {
      if (disposed || sourceTouched) return;
      sourceText = settings.tran_source_text;
      if (sourceText.trim()) scheduleTranslate();
    });
  });

  onDestroy(() => {
    disposed = true;
    if (debounceTimer) clearTimeout(debounceTimer);
    document.removeEventListener("app:flush", handleAppFlush);
    document.removeEventListener("tran:setSource", handleDropEvent);
    void flushSource();
  });

  async function contextCopy(editor: { copySelection: () => Promise<boolean> } | undefined) {
    if (await editor?.copySelection()) onStatusUpdate?.("Copied", "success", 0, 0);
  }

  async function contextCut(editor: { cutSelection: () => Promise<boolean> } | undefined) {
    if (await editor?.cutSelection()) onStatusUpdate?.("Cut", "success", 0, 0);
  }

  async function contextPaste(editor: { pasteText: () => Promise<boolean> } | undefined) {
    if (await editor?.pasteText()) onStatusUpdate?.("Pasted", "success", 0, 0);
  }

  function showContextMenu(e: MouseEvent, isSource: boolean) {
    const editor = isSource ? sourceEditor : translatedEditor;
    const sel = editor?.selectionText() || window.getSelection()?.toString() || "";
    const items: ContextItem[] = [
      { label: "Cut", action: () => void contextCut(sourceEditor), disabled: !isSource || !sel },
      { label: "Copy", action: () => void contextCopy(editor), disabled: !sel },
      { label: "Paste", action: () => void contextPaste(sourceEditor), disabled: !isSource },
      { label: "Select All", action: () => editor?.selectAll() },
      { label: "", action: () => {}, divider: true },
      { label: "Select All", action: () => editor?.selectAll() },
    ];
    if (isSource && sel) {
      items.push(
        { label: "", action: () => {}, divider: true },
        { label: "Look Up", action: () => { popupDict = { text: sel, x: e.clientX, y: e.clientY }; } },
      );
    }
    ctxMenu = { items, x: e.clientX, y: e.clientY };
  }

  function startDrag(e: MouseEvent) {
    isDragging = true;
    const panes = (e.target as HTMLElement).closest(".panes") as HTMLElement;
    if (!panes) return;
    const rect = panes.getBoundingClientRect();
    const isHoriz = layout === "horizontal";

    function onMove(ev: MouseEvent) {
      if (isHoriz) {
        sashPos = ((ev.clientX - rect.left) / rect.width) * 100;
      } else {
        sashPos = ((ev.clientY - rect.top) / rect.height) * 100;
      }
      sashPos = Math.max(20, Math.min(80, sashPos));
    }

    function onUp() {
      isDragging = false;
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      saveSetting("sash_pos_tran", Math.round(sashPos));
    }

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }
</script>

<div class="tran-tab">
  {#if findOpen}
    <FindBar bind:this={findBar} settingsPrefix="tran" onSearch={handleFind} onClose={() => { findOpen = false; sourceEditor?.focus(); }} />
  {/if}
  <div
    class="panes"
    class:horizontal={layout === "horizontal"}
    class:vertical={layout === "vertical"}
    class:dragging={isDragging}
  >
    <div class="pane source-pane" style={layout === "horizontal" ? `flex: 0 0 ${sashPos}%` : `flex: 0 0 ${sashPos}%`}>
      {#if !compact}
      <div class="pane-header">
        <span class="pane-title">Source</span>
        <div class="pane-actions">
          {#if isTranslating}<span class="translation-state">Translating...</span>{/if}
          <button class="pane-btn" onclick={clearSrc} title="Clear Source" aria-label="Clear Source"><AppIcon name="clear" size={14} /><span class="btn-label">Clear</span></button>
          <button class="pane-btn" onclick={() => copyText(sourceText)} title="Copy Source" aria-label="Copy Source"><AppIcon name="copy" size={14} /><span class="btn-label">Copy</span></button>
          <button class="pane-btn" class:toggled={sourceFindOpen} onclick={() => void toggleEditorFind("source")} title="Show or hide Source editor search"><AppIcon name="search-left" size={14} /><span class="btn-label">S</span></button>
        </div>
      </div>
      {/if}
      <CmEditor
        bind:this={sourceEditor}
        value={sourceText}
        {fontSize}
        {wordWrap}
        {showWhitespace}
        onChange={onSourceChange}
        onKeyDown={handleSourceKeyDown}
        onContextMenu={(e) => showContextMenu(e, true)}
        onScrollChange={(state) => syncEditorScroll("source", state)}
        onCursorChange={(line, col, selLen, chars) => handleCursorChange("source", line, col, selLen, chars)}
        onFindVisibilityChange={(visible) => handleEditorFindVisibility("source", visible)}
        onWheel={handleWheel}
      />
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={sashEl}
      class="pane-divider"
      class:horizontal={layout === "horizontal"}
      class:vertical={layout === "vertical"}
      role="separator"
      aria-orientation={layout === "horizontal" ? "vertical" : "horizontal"}
      onmousedown={startDrag}
    ></div>
    <div class="pane dest-pane">
      {#if !compact}
      <div class="pane-header">
        <span class="pane-title">
          Translated
          {#if detectedLang}
            <span class="detected-badge">{detectedLang}</span>
          {/if}
        </span>
        <div class="pane-actions">
          <button class="pane-btn" onclick={() => copyText(translatedText)} title="Copy Translated" aria-label="Copy Translated"><AppIcon name="copy" size={14} /><span class="btn-label">Copy</span></button>
          <button class="pane-btn" class:toggled={translatedFindOpen} onclick={() => void toggleEditorFind("translated")} title="Show or hide Translated editor search"><AppIcon name="search-right" size={14} /><span class="btn-label">T</span></button>
        </div>
      </div>
      {/if}
      <CmEditor
        bind:this={translatedEditor}
        value={translatedText}
        {fontSize}
        {wordWrap}
        {showWhitespace}
        textColor="var(--accent2)"
        readonly={true}
        onKeyDown={handleSourceKeyDown}
        onContextMenu={(e) => showContextMenu(e, false)}
        onScrollChange={(state) => syncEditorScroll("translated", state)}
        onCursorChange={(line, col, selLen, chars) => handleCursorChange("translated", line, col, selLen, chars)}
        onFindVisibilityChange={(visible) => handleEditorFindVisibility("translated", visible)}
        onWheel={handleWheel}
      />
    </div>
  </div>
</div>

{#if ctxMenu}
  <ContextMenu items={ctxMenu.items} x={ctxMenu.x} y={ctxMenu.y} onClose={() => ctxMenu = null} />
{/if}

{#if popupDict}
  <PopupDict text={popupDict.text} x={popupDict.x} y={popupDict.y} onClose={() => popupDict = null} />
{/if}

<style>
  .tran-tab { display: flex; flex-direction: column; flex: 1; overflow: hidden; }
  .panes { display: flex; flex: 1; overflow: hidden; }
  .panes.horizontal { flex-direction: row; }
  .panes.vertical { flex-direction: column; }
  .panes.dragging { user-select: none; cursor: col-resize; }
  .panes.vertical.dragging { cursor: row-resize; }
  .pane { display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; }
  /* The source pane owns the persisted sash basis; the translated pane must
     grow into everything left over. Without an explicit flex value it can
     collapse to its intrinsic width (zero in compact mode, where its header
     is hidden), making a successful translation look like an empty result. */
  .dest-pane { flex: 1 1 0; }
  .pane-header { display: flex; align-items: center; height: 28px; padding: 0 8px; background: var(--bg3); gap: 6px; flex-shrink: 0; }
  .pane-title { font-size: 11px; font-weight: 600; color: var(--fg2); display: flex; align-items: center; gap: 6px; }
  .detected-badge { font-size: 10px; padding: 1px 5px; background: var(--accent); color: var(--bg); border-radius: 3px; font-weight: 500; }
  .pane-actions { display: flex; align-items: center; gap: 4px; margin-left: auto; }
  .pane-btn { display: inline-flex; align-items: center; justify-content: center; gap: 3px; height: var(--control-height); padding: 0 var(--control-padding-x); background: transparent; border: none; color: var(--fg2); font-family: inherit; font-size: 11px; line-height: 1; white-space: nowrap; flex: 0 0 auto; cursor: pointer; border-radius: var(--control-radius); }
  .pane-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .pane-btn :global(.app-icon) { width: 15px; height: 15px; }
  .pane-btn:disabled { opacity: 0.5; cursor: default; }
  .pane-btn.toggled { background: var(--accent); color: var(--bg); }
  .translation-state { color: var(--warning); font-size: 10px; white-space: nowrap; }
  .pane-divider { flex-shrink: 0; background: var(--border); position: relative; z-index: 10; }
  .pane-divider.horizontal { width: 5px; cursor: col-resize; }
  .pane-divider.vertical { height: 5px; cursor: row-resize; }
  .pane-divider:hover { background: var(--accent); }

  @media (max-width: 680px) {
    .pane-header { padding-inline: 4px; gap: 3px; }
    .pane-actions { gap: 1px; }
    .pane-btn { width: var(--control-height); min-width: var(--control-height); padding: 0; }
    .btn-label { display: none; }
    .translation-state { max-width: 58px; overflow: hidden; text-overflow: ellipsis; }
  }
</style>
