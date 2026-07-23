<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { readTextFile } from "@tauri-apps/plugin-fs";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getVersion } from "@tauri-apps/api/app";
  import appLogo from "../src-tauri/icons/app-icon.png";
  import TopBar from "./lib/components/TopBar.svelte";
  import LangBar from "./lib/components/LangBar.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import CompactBar from "./lib/components/CompactBar.svelte";
  import TranslateTab from "./lib/tabs/TranslateTab.svelte";
  import DiffTab from "./lib/tabs/DiffTab.svelte";
  import NotesTab from "./lib/tabs/NotesTab.svelte";
  import AppDialog from "./lib/components/AppDialog.svelte";
  import { themeStore } from "./lib/stores/theme";
  import { loadSettings, saveSetting, type AppSettings } from "./lib/stores/settings";
  import { checkForUpdates, type DialogRequest, type UpdateProgress } from "./lib/utils/updater";
  import { formatAppVersion } from "./lib/utils/version";
  import { swapLanguages } from "./lib/utils/languages";

  type TabId = "tran" | "diff" | "note";
  type StatusKind = "normal" | "success" | "warning" | "error";
  type TabStatus = { text: string; kind: StatusKind };

  let activeTab = $state<TabId>("diff");
  let booting = $state(true);
  let appVersion = $state("");
  let compact = $state(false);
  let layout = $state<"horizontal" | "vertical">("horizontal");
  let wordWraps = $state<Record<TabId, boolean>>({ tran: true, diff: true, note: true });
  let windowEffect = $state("blur");
  let onTop = $state(false);
  let engine = $state("Google Translate");
  let srcLang = $state("Auto Detect");
  let destLang = $state("English");
  let fontSizes = $state<Record<TabId, number>>({ tran: 10, diff: 10, note: 10 });
  let dragOver = $state(false);
  let dialog = $state<DialogRequest | null>(null);
  let dialogResolver: ((accepted: boolean) => void) | null = null;
  let updateDownloadVersion = $state<string | null>(null);
  let compactNotes = $state<Array<{ id: number; title: string }>>([]);
  let compactNoteId = $state<number | null>(null);

  // Status messages and focused panels belong to a tab. Hidden tabs can
  // continue async work without replacing the visible tab's status.
  let tabStatuses = $state<Record<TabId, TabStatus>>({
    diff: { text: "Ready", kind: "normal" },
    tran: { text: "Ready", kind: "normal" },
    note: { text: "Ready", kind: "normal" },
  });

  // Each tab tracks its own cursor; we show the active tab's values
  let cursorInfo = $state<Record<string, { line: number; col: number; selLen: number; chars: number }>>({
    "tran-source": { line: 1, col: 1, selLen: 0, chars: 0 },
    "tran-translated": { line: 1, col: 1, selLen: 0, chars: 0 },
    "diff-left": { line: 1, col: 1, selLen: 0, chars: 0 },
    "diff-right": { line: 1, col: 1, selLen: 0, chars: 0 },
    "note-editor": { line: 1, col: 1, selLen: 0, chars: 0 },
  });
  let activePanels = $state<Record<TabId, string>>({
    diff: "diff-left",
    tran: "tran-source",
    note: "note-editor",
  });

  let transTime = $state(0);
  let transChars = $state(0);
  let diffStats = $state({ added: 0, removed: 0, changed_blocks: 0 });

  let translateTab = $state<TranslateTab>();
  let diffTab = $state<DiffTab>();
  let notesTab = $state<NotesTab>();
  let sashPosTran = $state(50);
  let diffWordDiff = $state(true);
  let diffIgnoreWhitespace = $state(false);
  let showWhitespaces = $state<Record<TabId, boolean>>({ tran: false, diff: false, note: false });
  let diffAlgorithm = $state<"legacy" | "advanced">("legacy");
  let noteAutoSave = $state(true);
  let diffLeftRatio = $state(0.5);
  let noteSidebarWidth = $state(220);

  const tabs: TabId[] = ["diff", "tran", "note"];
  // ── Derived status bar values ─────────────────────────────
  let activePanel = $derived(activePanels[activeTab]);
  let activeStatus = $derived(tabStatuses[activeTab]);
  let activeCursorInfo = $derived(cursorInfo[activePanel] ?? { line: 1, col: 1, selLen: 0, chars: 0 });
  let charCount = $derived(activeCursorInfo.chars);
  let lineCount = $derived(activeCursorInfo.line);
  let colCount = $derived(activeCursorInfo.col);
  let selLength = $derived(activeCursorInfo.selLen);
  let diffLeftChars = $derived(cursorInfo["diff-left"].chars);
  let diffRightChars = $derived(cursorInfo["diff-right"].chars);
  const panelNames: Record<string, string> = {
    "tran-source": "Source",
    "tran-translated": "Translated",
    "diff-left": "Left",
    "diff-right": "Right",
    "note-editor": "Note",
  };
  let activePanelName = $derived(panelNames[activePanel] ?? activePanel);

  function setTabStatus(tab: TabId, text: string, kind: string) {
    tabStatuses[tab] = {
      text,
      kind: (["normal", "success", "warning", "error"].includes(kind) ? kind : "normal") as StatusKind,
    };
  }

  function showDialog(request: DialogRequest): Promise<boolean> {
    if (dialogResolver) dialogResolver(false);
    dialog = request;
    return new Promise((resolve) => { dialogResolver = resolve; });
  }

  function closeDialog(accepted: boolean) {
    const resolve = dialogResolver;
    dialogResolver = null;
    dialog = null;
    resolve?.(accepted);
  }

  async function handleCheckUpdate(onProgress?: (progress: UpdateProgress) => void, force = true) {
    await checkForUpdates(force, (progress) => {
      updateDownloadVersion = progress.phase === "downloading" ? progress.version ?? "" : null;
      onProgress?.(progress);
    }, showDialog);
  }

  onMount(() => {
    let unlistenClose: (() => void) | undefined;
    let unlistenResize: (() => void) | undefined;
    let unlistenQuit: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;
    let unlistenDragOver: (() => void) | undefined;
    let unlistenDragLeave: (() => void) | undefined;
    let updateTimer: ReturnType<typeof setTimeout> | undefined;
    let closing = false;
    void (async () => {
    const splashStarted = performance.now();
    const [s, version] = await Promise.all([
      loadSettings(),
      getVersion().catch(() => "1.26.7-13.37"),
    ]);
    appVersion = `v${formatAppVersion(version)}`;
    const normalizedTheme = s.theme === "light" ? "light" : "dark";
    themeStore.init(normalizedTheme);
    compact = s.compact_mode;
    layout = s.layout;
    wordWraps = {
      tran: s.word_wrap_tran,
      diff: s.word_wrap_diff,
      note: s.word_wrap_note,
    };
    const validEffects = /Linux/i.test(navigator.userAgent)
      ? ["solid"]
      : /Mac/i.test(navigator.userAgent)
        ? ["solid", "blur", "frosted"]
        : ["solid", "blur", "frosted", "transp", "dim", "ghost", "clear"];
    windowEffect = validEffects.includes(s.window_effect) ? s.window_effect : "blur";
    onTop = s.always_on_top;
    srcLang = s.src_lang;
    destLang = s.dest_lang === "Auto Detect" ? "English" : s.dest_lang;
    if (s.dest_lang === "Auto Detect") saveSetting("dest_lang", "English");
    engine = s.engine === "Google Translate" ? s.engine : "Google Translate";
    activeTab = tabs.includes(s.active_tab as TabId) ? s.active_tab as TabId : "diff";
    if (s.theme !== normalizedTheme) void saveSetting("theme", normalizedTheme);
    if (s.window_effect !== windowEffect) void saveSetting("window_effect", windowEffect);
    if (s.engine !== engine) void saveSetting("engine", engine);
    if (s.active_tab !== activeTab) void saveSetting("active_tab", activeTab);
    sashPosTran = s.sash_pos_tran ?? 50;
    diffWordDiff = s.diff_word_diff;
    diffIgnoreWhitespace = s.diff_ignore_whitespace;
    showWhitespaces = {
      tran: s.tran_show_whitespace,
      diff: s.diff_show_whitespace,
      note: s.note_show_whitespace,
    };
    diffAlgorithm = s.diff_algorithm === "advanced" ? "advanced" : "legacy";
    noteAutoSave = s.note_auto_save;
    diffLeftRatio = s.diff_left_ratio;
    noteSidebarWidth = s.note_sidebar_width;
    fontSizes = {
      tran: s.font_size_tran,
      diff: s.font_size_diff,
      note: s.font_size_note,
    };

    try { await invoke("set_window_effect", { effect: windowEffect }); } catch {}
    try { await invoke("set_always_on_top", { onTop: s.always_on_top }); } catch {}
    try {
      await invoke("set_window_size", {
        width: s.compact_mode ? s.compact_width : s.window_width,
        height: s.compact_mode ? (activeTab === "diff" ? s.compact_diff_height : s.compact_height) : s.window_height,
        compact: s.compact_mode,
      });
    } catch {}

    const splashRemaining = 420 - (performance.now() - splashStarted);
    if (splashRemaining > 0) await new Promise((resolve) => setTimeout(resolve, splashRemaining));
    booting = false;

    updateTimer = setTimeout(() => void handleCheckUpdate(undefined, false), 1500);

    unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
      if (closing) return;
      event.preventDefault();
      closing = true;
      await flushAndExit();
    });

    unlistenResize = await listen("window-resized", (e: { payload: { width: number; height: number } }) => {
      if (closing) return;
      if (compact && e.payload.width >= 280 && e.payload.height >= 120) {
        debouncedSaveSetting("compact_width", e.payload.width);
        debouncedSaveSetting(activeTab === "diff" ? "compact_diff_height" : "compact_height", e.payload.height);
      } else if (!compact && e.payload.width >= 340 && e.payload.height >= 120) {
        debouncedSaveSetting("window_width", e.payload.width);
        debouncedSaveSetting("window_height", e.payload.height);
      }
    });
    unlistenQuit = await listen("app-quit-requested", async () => {
      if (closing) return;
      closing = true;
      await flushAndExit();
    });
    // Native Tauri file drag/drop events are used when dragDropEnabled is on.
    // Keep the HTML5 handlers below as a browser/dev fallback, but handle
    // native paths here so packaged builds receive the same behavior.
    unlistenDragOver = await listen("tauri://drag-over", () => {
      if (!closing) dragOver = true;
    });
    unlistenDragLeave = await listen("tauri://drag-leave", () => {
      dragOver = false;
    });
    unlistenDrop = await listen<string[]>("tauri://drag-drop", async (event) => {
      dragOver = false;
      const path = event.payload?.[0];
      if (!path) return;
      try {
        const text = await readTextFile(path);
        dispatchDroppedText(text);
      } catch (error) {
        setTabStatus(activeTab, `Cannot read dropped file: ${error}`, "error");
      }
    });
    document.addEventListener("keydown", handleGlobalFindShortcut, true);
    })();
    return () => {
      unlistenClose?.();
      unlistenResize?.();
      unlistenQuit?.();
      unlistenDrop?.();
      unlistenDragOver?.();
      unlistenDragLeave?.();
      document.removeEventListener("keydown", handleGlobalFindShortcut, true);
      if (updateTimer) clearTimeout(updateTimer);
    };
  });

  const pendingSettingSaves = new Map<keyof AppSettings, {
    timer: ReturnType<typeof setTimeout>;
    value: AppSettings[keyof AppSettings];
  }>();

  function debouncedSaveSetting<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    const pending = pendingSettingSaves.get(key);
    if (pending) clearTimeout(pending.timer);
    const timer = setTimeout(() => {
      pendingSettingSaves.delete(key);
      void saveSetting(key, value);
    }, 300);
    pendingSettingSaves.set(key, { timer, value });
  }

  async function flushPendingSettings() {
    const entries = [...pendingSettingSaves.entries()];
    pendingSettingSaves.clear();
    for (const [, pending] of entries) clearTimeout(pending.timer);
    await Promise.allSettled(entries.map(([key, pending]) => saveSetting(key, pending.value)));
  }

  async function flushAndExit() {
    const tasks: Promise<unknown>[] = [];
    try {
      document.dispatchEvent(new CustomEvent("app:flush", { detail: { tasks } }));
      await Promise.allSettled(tasks);
      await flushPendingSettings();
      await Promise.allSettled([
        invoke("save_settings_flush"),
        invoke("flush_notes"),
      ]);
    } finally {
      // A failed persistence operation must not leave a window permanently
      // stuck after close was already accepted/prevented.
      try { await invoke("exit_app"); } catch { /* process may already be exiting */ }
    }
  }

  function handleTabSwitch(tab: TabId) {
    if (tab === activeTab) return;
    activeTab = tab;
    debouncedSaveSetting("active_tab", tab);
  }

  function handleToggleCompact() {
    compact = !compact;
    debouncedSaveSetting("compact_mode", compact);
    invoke("toggle_compact", { compact, tab: activeTab });
  }

  function handleToggleLayout() {
    layout = layout === "horizontal" ? "vertical" : "horizontal";
    debouncedSaveSetting("layout", layout);
  }

  function handleToggleWrap(tab: TabId) {
    wordWraps[tab] = !wordWraps[tab];
    const key = tab === "tran" ? "word_wrap_tran" : tab === "diff" ? "word_wrap_diff" : "word_wrap_note";
    debouncedSaveSetting(key, wordWraps[tab]);
  }

  function handleToggleWhitespace(tab: TabId) {
    if (tab === "diff") {
      diffTab?.toggleWhitespaceControl();
      return;
    }
    showWhitespaces[tab] = !showWhitespaces[tab];
    const key = tab === "tran" ? "tran_show_whitespace" : "note_show_whitespace";
    debouncedSaveSetting(key, showWhitespaces[tab]);
  }

  function handleToggleOnTop() {
    onTop = !onTop;
    invoke("set_always_on_top", { onTop });
    debouncedSaveSetting("always_on_top", onTop);
  }

  function handleSelectEffect(effect: string) {
    windowEffect = effect;
    invoke("set_window_effect", { effect });
    debouncedSaveSetting("window_effect", effect);
  }

  function handleSetLang(field: "src_lang" | "dest_lang" | "engine", val: string) {
    if (field === "src_lang") srcLang = val;
    else if (field === "dest_lang") destLang = val;
    else engine = val;
    debouncedSaveSetting(field, val);
  }

  function handleSwapLanguageState() {
    const swapped = swapLanguages(srcLang, destLang, translateTab?.getDetectedLanguage());
    if (!swapped) {
      setTabStatus("tran", "Enter text and wait for language detection before swapping", "warning");
      return;
    }
    srcLang = swapped.source;
    destLang = swapped.destination;
    debouncedSaveSetting("src_lang", srcLang);
    debouncedSaveSetting("dest_lang", destLang);
  }

  // ── Cursor tracking from editors ──────────────────────────
  function handleTranCursor(side: string, line: number, col: number, selLen: number, chars: number) {
    const key = `tran-${side}`;
    cursorInfo[key] = { line, col, selLen, chars };
    activePanels.tran = key;
  }

  function handleDiffCursor(side: string, line: number, col: number, selLen: number, chars: number) {
    const key = `diff-${side}`;
    cursorInfo[key] = { line, col, selLen, chars };
    activePanels.diff = key;
  }

  function handleNoteCursor(line: number, col: number, selLen: number, chars: number) {
    cursorInfo["note-editor"] = { line, col, selLen, chars };
    activePanels.note = "note-editor";
  }

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const file = files[0];
      const text = await file.text();
      dispatchDroppedText(text);
    }
  }

  function dispatchDroppedText(text: string) {
    if (activeTab === "tran") {
      document.dispatchEvent(new CustomEvent("tran:setSource", { detail: text }));
    } else if (activeTab === "diff") {
      document.dispatchEvent(new CustomEvent("diff:setLeft", { detail: text }));
    } else if (activeTab === "note") {
      document.dispatchEvent(new CustomEvent("note:setContent", { detail: text }));
    }
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleZoom(tab: TabId, delta: number) {
    const current = fontSizes[tab];
    const next = Math.max(7, Math.min(28, current + (delta > 0 ? 1 : -1)));
    fontSizes[tab] = next;
    const key = tab === "tran" ? "font_size_tran" : tab === "diff" ? "font_size_diff" : "font_size_note";
    debouncedSaveSetting(key as any, next);
  }

  function handleGlobalFindShortcut(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "f") return;

    if (event.shiftKey) {
      event.preventDefault();
      event.stopPropagation();
      if (activeTab === "diff") diffTab?.openCommonFind();
      else if (activeTab === "tran") translateTab?.openCommonFind();
      return;
    }

    const target = event.target;
    if (target instanceof Element && target.closest(".monaco-editor")) return;
    event.preventDefault();
    event.stopPropagation();
  }

  function disableNativeContextMenu(event: MouseEvent) {
    // Editor components still receive this event and open the app's own menu.
    // Preventing the default at document level only disables the WebView menu.
    event.preventDefault();
  }
</script>

<svelte:document oncontextmenu={disableNativeContextMenu} />

{#if booting}
  <div class="splash-root" data-theme={$themeStore.current}>
    <img src={appLogo} alt="SBS Desk Tool" />
    <div class="splash-name">SBS Desk Tool</div>
    <div class="splash-version">{appVersion || "Starting..."}</div>
    <div class="splash-loading"><span></span> Loading workspace...</div>
  </div>
{:else}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="app-root"
  class:dragover={dragOver}
  data-theme={$themeStore.current}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  {#if !compact}
    <TopBar
      {activeTab}
      {compact}
      {windowEffect}
      {onTop}
      updateDownloading={updateDownloadVersion !== null}
      onTabSwitch={handleTabSwitch}
      onToggleCompact={handleToggleCompact}
      onToggleOnTop={handleToggleOnTop}
      onSelectEffect={handleSelectEffect}
      onCheckUpdate={(onProgress) => handleCheckUpdate(onProgress, true)}
    />
    {#if activeTab === "tran"}
      <LangBar
        {engine} {srcLang} {destLang} {layout}
        wordWrap={wordWraps.tran}
        showWhitespace={showWhitespaces.tran}
        onSetLang={handleSetLang}
        onSwapText={handleSwapLanguageState}
        onToggleLayout={handleToggleLayout}
        onToggleWrap={() => handleToggleWrap("tran")}
        onToggleWhitespace={() => handleToggleWhitespace("tran")}
      />
    {/if}
  {:else}
    <CompactBar
      {activeTab}
      {layout}
      {wordWraps}
      {showWhitespaces}
      {diffWordDiff}
      {diffIgnoreWhitespace}
      {diffAlgorithm}
      {srcLang} {destLang}
      {onTop}
      notes={compactNotes}
      selectedNoteId={compactNoteId}
      onTabSwitch={handleTabSwitch}
      onSetLang={handleSetLang}
      onSelectNote={(id) => notesTab?.selectNoteById(id)}
      onToggleCompact={handleToggleCompact}
      onToggleOnTop={handleToggleOnTop}
      onToggleLayout={handleToggleLayout}
      onToggleWrap={handleToggleWrap}
      onToggleWhitespace={handleToggleWhitespace}
      onSwapText={handleSwapLanguageState}
      onToggleDiffWord={() => diffTab?.toggleWordControl()}
      onToggleDiffIgnoreWhitespace={() => diffTab?.toggleIgnoreWhitespaceControl()}
      onSetDiffAlgorithm={(algorithm) => diffTab?.setAlgorithmControl(algorithm)}
    />
  {/if}

  {#if updateDownloadVersion !== null}
    <div class="update-download" role="status" aria-live="polite">
      <span>Downloading update {updateDownloadVersion}...</span>
      <div class="update-progress-track" role="progressbar" aria-label={`Downloading update ${updateDownloadVersion}`}>
        <span></span>
      </div>
    </div>
  {/if}

  {#if dragOver}
    <div class="drop-overlay">Drop file here</div>
  {/if}

  <!-- Keep all tabs mounted but hide inactive for state preservation -->
  <main class="content">
    <div class="tab-panel" class:active={activeTab === "tran"}>
      <TranslateTab
        bind:this={translateTab}
        {layout}
        {compact}
        wordWrap={wordWraps.tran}
        showWhitespace={showWhitespaces.tran}
        {srcLang} {destLang}
        fontSize={fontSizes.tran}
        sashPos={sashPosTran}
        onZoom={(d) => handleZoom("tran", d)}
        onCursorChange={handleTranCursor}
        onStatusUpdate={(text, kind, time, chars) => {
          setTabStatus("tran", text, kind);
          transTime = time;
          transChars = chars;
        }}
      />
    </div>
    <div class="tab-panel" class:active={activeTab === "diff"}>
      <DiffTab
        bind:this={diffTab}
        {compact}
        wordWrap={wordWraps.diff}
        theme={$themeStore.current}
        fontSize={fontSizes.diff}
        wordDiff={diffWordDiff}
        ignoreWhitespace={diffIgnoreWhitespace}
        showWhitespace={showWhitespaces.diff}
        initialAlgorithm={diffAlgorithm}
        initialSashRatio={diffLeftRatio}
        onZoom={(d) => handleZoom("diff", d)}
        onToggleWrap={() => handleToggleWrap("diff")}
        onControlStateChange={(state) => {
          diffWordDiff = state.wordDiff;
          diffIgnoreWhitespace = state.ignoreWhitespace;
          showWhitespaces.diff = state.showWhitespace;
          diffAlgorithm = state.algorithm;
        }}
        onCursorChange={handleDiffCursor}
        onStatusUpdate={(text, kind) => setTabStatus("diff", text, kind)}
        onStatsUpdate={(stats) => { diffStats = stats; }}
      />
    </div>
    <div class="tab-panel" class:active={activeTab === "note"}>
      <NotesTab
        bind:this={notesTab}
        {compact}
        wordWrap={wordWraps.note}
        showWhitespace={showWhitespaces.note}
        fontSize={fontSizes.note}
        autoSave={noteAutoSave}
        initialSidebarWidth={noteSidebarWidth}
        onZoom={(d) => handleZoom("note", d)}
        onToggleWrap={() => handleToggleWrap("note")}
        onToggleWhitespace={() => handleToggleWhitespace("note")}
        onCursorChange={handleNoteCursor}
        onStatusUpdate={(text, kind) => setTabStatus("note", text, kind)}
        onNotesChange={(notes, selectedId) => {
          compactNotes = notes;
          compactNoteId = selectedId;
        }}
      />
    </div>
  </main>

  {#if !compact}
    <StatusBar
      {activeTab}
      statusText={activeStatus.text}
      statusKind={activeStatus.kind}
      {charCount}
      {lineCount}
      {colCount}
      {selLength}
      {diffLeftChars}
      {diffRightChars}
      translationTime={transTime}
      translationChars={transChars}
      panelName={activePanelName}
      diffStatsText={`+${diffStats.added} -${diffStats.removed} / ${diffStats.changed_blocks} blocks`}
    />
  {/if}
</div>
{/if}

{#if dialog}
  <AppDialog
    title={dialog.title}
    message={dialog.message}
    details={dialog.details}
    confirmLabel={dialog.confirmLabel}
    cancelLabel={dialog.cancelLabel}
    showCancel={dialog.showCancel}
    tone={dialog.tone}
    onConfirm={() => closeDialog(true)}
    onCancel={() => closeDialog(false)}
  />
{/if}

<style>
  .splash-root {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 100vh;
    background: radial-gradient(circle at 50% 35%, var(--bg3), var(--bg) 72%);
    color: var(--fg);
    user-select: none;
  }
  .splash-root img { width: 70px; height: 70px; object-fit: contain; filter: drop-shadow(0 5px 12px rgba(0,0,0,.32)); }
  .splash-name { margin-top: 2px; font-size: 19px; font-weight: 650; letter-spacing: .2px; }
  .splash-version { color: var(--fg2); font-size: 11px; }
  .splash-loading { display: flex; align-items: center; gap: 7px; margin-top: 10px; color: var(--fg2); font-size: 11px; }
  .splash-loading span { width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: splash-spin .8s linear infinite; }
  @keyframes splash-spin { to { transform: rotate(360deg); } }
  .update-download { display: flex; flex: none; align-items: center; justify-content: center; gap: 10px; min-height: 24px; padding: 0 10px; border-bottom: 1px solid var(--border); background: var(--bg3); color: var(--fg2); font-size: 11px; }
  .update-progress-track { width: min(220px, 38vw); height: 4px; overflow: hidden; border-radius: 999px; background: var(--border); }
  .update-progress-track span { display: block; width: 42%; height: 100%; border-radius: inherit; background: var(--accent); animation: update-progress 1.1s ease-in-out infinite; }
  @keyframes update-progress { from { transform: translateX(-110%); } to { transform: translateX(340%); } }
  .app-root { display: flex; flex-direction: column; height: 100vh; background: var(--bg); }
  .content { flex: 1; overflow: hidden; display: flex; flex-direction: column; position: relative; }
  .tab-panel { display: none; flex: 1; overflow: hidden; }
  .tab-panel.active { display: flex; }
  .app-root.dragover { outline: 3px dashed var(--accent); outline-offset: -3px; }
  .drop-overlay { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0,0,0,0.3); color: white; font-size: 18px; font-weight: 600; z-index: 999; pointer-events: none; }
</style>
