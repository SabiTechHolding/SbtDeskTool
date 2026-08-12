<script lang="ts">
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { copyFile, writeFile, writeTextFile } from "@tauri-apps/plugin-fs";
  import DiffEditor from "../components/DiffEditor.svelte";
  import FolderDiffPanel from "../components/FolderDiffPanel.svelte";
  import FindBar from "../components/FindBar.svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import ContextMenu from "../components/ContextMenu.svelte";
  import type { ContextItem } from "../components/ContextMenu.svelte";
  import { loadSettings, saveSetting } from "../stores/settings";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy, tick } from "svelte";

  interface InlineToken {
    text: string;
    kind: "equal" | "insert" | "delete";
  }

  interface DiffLine {
    kind: string;
    left_text: string;
    right_text: string;
    left_present: boolean;
    right_present: boolean;
    left_tokens: InlineToken[];
    right_tokens: InlineToken[];
  }

  interface ReadFileResult {
    content: string;
  }

  interface DiffTabState {
    id: number;
    title: string;
    folderPath: string;
    previewKind: string;
    leftPath: string;
    rightPath: string;
    leftText: string;
    rightText: string;
    pinned: boolean;
    untitled: boolean;
  }

  interface FolderDiffEntry {
    relative_path: string;
    status: "left_only" | "right_only" | "different" | "equal";
    left_path: string | null;
    right_path: string | null;
    left_size: number | null;
    right_size: number | null;
  }

  let {
    compact, wordWrap, theme, fontSize, wordDiff: initialWordDiff, ignoreWhitespace: initialIgnoreWhitespace, showWhitespace: initialShowWhitespace, initialAlgorithm, initialSashRatio, onZoom, onToggleWrap, onControlStateChange, onCursorChange, onStatusUpdate, onStatsUpdate,
  }: {
    compact: boolean;
    wordWrap: boolean;
    theme: "dark" | "light";
    fontSize: number;
    wordDiff: boolean;
    ignoreWhitespace: boolean;
    showWhitespace: boolean;
    initialAlgorithm: "legacy" | "advanced";
    initialSashRatio: number;
    onZoom: (delta: number) => void;
    onToggleWrap: () => void;
    onControlStateChange?: (state: { wordDiff: boolean; ignoreWhitespace: boolean; showWhitespace: boolean; algorithm: "legacy" | "advanced" }) => void;
    onCursorChange?: (side: string, line: number, col: number, selLen: number, chars: number) => void;
    onStatusUpdate?: (text: string, kind: string) => void;
    onStatsUpdate?: (stats: { added: number; removed: number; changed_blocks: number }) => void;
  } = $props();

  let leftText = $state("");
  let rightText = $state("");
  let diffData = $state<DiffLine[]>([]);
  let diffStats = $state({ added: 0, removed: 0, changed_blocks: 0 });
  let wordDiff = $state(true);
  let ignoreWhitespace = $state(true);
  let showWhitespace = $state(false);
  let diffAlgorithm = $state<"legacy" | "advanced">("legacy");
  $effect(() => {
    wordDiff = initialWordDiff;
    ignoreWhitespace = initialIgnoreWhitespace;
    showWhitespace = initialShowWhitespace;
    diffAlgorithm = initialAlgorithm === "advanced" ? "advanced" : "legacy";
  });
  let findOpen = $state(false);
  let findBar = $state<FindBar>();
  let detailLeft = $state("");
  let detailRight = $state("");
  let detailLeftTokens = $state<InlineToken[]>([]);
  let detailRightTokens = $state<InlineToken[]>([]);
  let detailKind = $state("");
  let showDetail = $state(true);
  let showCenterControls = $state(false);
  let compareMode = $state<"text" | "folder">("text");
  let selectedFolderFile = $state("");
  let viewMode = $state<"diff" | "preview">("diff");
  let previewLeftUrl = $state("");
  let previewRightUrl = $state("");
  let previewKind = $state("");
  let largeEntry = $state<{ relative_path: string; left_path: string | null; right_path: string | null } | null>(null);
  let largePreviewLeft = $state("");
  let largePreviewRight = $state("");
  let savedTextDiff = $state({ left: "", right: "" });
  let folderSidebarWidth = $state(360);
  let diffRoot = $state<HTMLDivElement>();
  let leftFindOpen = $state(false);
  let rightFindOpen = $state(false);
  let editorGeneration = $state(0);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let diffRequestId = 0;
  let diffTabs = $state<DiffTabState[]>([]);
  let activeDiffTabId = $state<number | null>(null);
  let untitledDiffTabId = $state<number | null>(null);
  let leftPathInput = $state("");
  let rightPathInput = $state("");
  let nextDiffTabId = 1;
  let draggedDiffTabId = $state<number | null>(null);
  let activeEditorSide: "left" | "right" = "left";
  let tabContextMenu = $state<{ tab: DiffTabState; x: number; y: number } | null>(null);
  const activeDiffTab = $derived(diffTabs.find((tab) => tab.id === activeDiffTabId));

  function shortTabTitle(label: string) {
    const name = label.replaceAll("\\", "/").split("/").pop() || label || "Untitled diff";
    return name.length > 28 ? `${name.slice(0, 25)}…` : name;
  }

  /**
   * A tab is added before it is made active so callers can populate both sides
   * atomically. This avoids the transient blank editor state that used to be
   * visible while opening a folder entry or restoring a saved tab.
   */
  function createDiffTab(pinned = true, untitled = false, activate = true, persist = true) {
    const tab: DiffTabState = { id: nextDiffTabId++, title: "New diff", folderPath: "", previewKind: "", leftPath: "", rightPath: "", leftText: "", rightText: "", pinned, untitled };
    diffTabs = [...diffTabs, tab];
    if (untitled) untitledDiffTabId = tab.id;
    if (activate) activateDiffTab(tab);
    if (pinned && persist) persistPinnedTabs();
    return tab;
  }

  function createUntitledDiff() {
    const existing = diffTabs.find((tab) => tab.id === untitledDiffTabId);
    return existing ?? createDiffTab(false, true);
  }

  function syncPathInputs(tab: DiffTabState) {
    leftPathInput = tab.leftPath;
    rightPathInput = tab.rightPath;
  }

  function activateDiffTab(tab: DiffTabState) {

    activeDiffTabId = tab.id;
    selectedFolderFile = tab.folderPath;
    syncPathInputs(tab);
    leftText = tab.leftText;
    rightText = tab.rightText;
    previewKind = tab.previewKind;
    viewMode = tab.previewKind ? "preview" : "diff";
    previewLeftUrl = tab.previewKind && tab.leftPath ? convertFileSrc(tab.leftPath) : "";
    previewRightUrl = tab.previewKind && tab.rightPath ? convertFileSrc(tab.rightPath) : "";
    if (!tab.previewKind) clearFolderPreview(true);
    void runDiff();
  }

  function updateActiveTab(patch: Partial<DiffTabState>) {
    if (activeDiffTab) Object.assign(activeDiffTab, patch);
  }

  function ensurePreviewTab(pinned = false) {
    if (!pinned) {
      const preview = diffTabs.find((tab) => !tab.pinned);
      if (preview) return preview;
    }
    return createDiffTab(pinned, false, false);
  }

  function updateFileTabTitle(tab: DiffTabState) {
    tab.title = tab.leftPath && tab.rightPath ? `${shortTabTitle(tab.leftPath)} ↔ ${shortTabTitle(tab.rightPath)}` : shortTabTitle(tab.leftPath || tab.rightPath || "New diff");
  }

  function pinDiffTab(tab: DiffTabState) {
    if (tab.pinned) return;
    tab.pinned = true;
    if (tab.id === untitledDiffTabId) untitledDiffTabId = null;
    tab.untitled = false;
    if (tab.title === "New diff") tab.title = shortTabTitle(selectedFolderFile) || "Untitled diff";
    diffTabs = [...diffTabs];
    persistPinnedTabs();
  }

  function unpinDiffTab(tab: DiffTabState) {
    if (!tab.pinned) return;
    const previousPreview = diffTabs.find((candidate) => candidate.id !== tab.id && !candidate.pinned && !candidate.untitled);
    if (previousPreview) closeDiffTab(previousPreview);
    tab.pinned = false;
    tab.untitled = false;
    diffTabs = [...diffTabs];
    persistPinnedTabs();
  }

  function closeDiffTab(tab: DiffTabState) {
    const index = diffTabs.findIndex((item) => item.id === tab.id);
    if (index < 0) return;
    const remaining = diffTabs.filter((item) => item.id !== tab.id);
    diffTabs = remaining;
    if (tab.id === untitledDiffTabId) untitledDiffTabId = null;
    persistPinnedTabs();
    if (activeDiffTabId === tab.id) {
      const next = remaining[Math.min(index, remaining.length - 1)];
      if (next) activateDiffTab(next);
      else {
        activeDiffTabId = null;
        selectedFolderFile = "";
        leftText = "";
        rightText = "";
        leftPathInput = "";
        rightPathInput = "";
        clearFolderPreview();
      }
    }
  }

  function persistPinnedTabs() {
    const saved = diffTabs.filter((tab) => tab.pinned).map((tab) => ({ title: tab.title, folderPath: tab.folderPath, previewKind: tab.previewKind, leftPath: tab.leftPath, rightPath: tab.rightPath }));
    void saveSetting("diff_pinned_tabs", JSON.stringify(saved));
  }

  async function restorePinnedTabs(serialized: string) {
    try {
      const saved = JSON.parse(serialized) as Array<{ title?: string; folderPath?: string; previewKind?: string; leftPath?: string; rightPath?: string }>;
      for (const item of saved) {
        if (!item.leftPath && !item.rightPath) continue;
        const tab = createDiffTab(true, false, false, false);
        tab.title = item.title || shortTabTitle(item.leftPath || item.rightPath || "New diff");
        tab.previewKind = item.previewKind || "";
        tab.folderPath = item.folderPath || "";
        tab.leftPath = item.leftPath || "";
        tab.rightPath = item.rightPath || "";
        const [left, right] = await Promise.all([
          tab.leftPath ? invoke<ReadFileResult>("read_folder_diff_file", { path: tab.leftPath }).catch(() => ({ content: "" })) : Promise.resolve({ content: "" }),
          tab.rightPath ? invoke<ReadFileResult>("read_folder_diff_file", { path: tab.rightPath }).catch(() => ({ content: "" })) : Promise.resolve({ content: "" }),
        ]);
        tab.leftText = left.content;
        tab.rightText = right.content;
        if (tab.id === activeDiffTabId) {
          leftText = left.content;
          rightText = right.content;
        }
      }
      if (!activeDiffTab && diffTabs.length) activateDiffTab(diffTabs[0]);
    } catch {
      // Ignore malformed or legacy tab settings.
    }
  }

  function tabContextItems(tab: DiffTabState): ContextItem[] {
    return [
      { label: tab.pinned ? "Unpin" : "Keep open", action: () => tab.pinned ? unpinDiffTab(tab) : pinDiffTab(tab) },
      { label: "Close", action: () => closeDiffTab(tab) },
      { label: "Close others", action: () => { diffTabs = diffTabs.filter((item) => item.id === tab.id); untitledDiffTabId = tab.untitled ? tab.id : null; persistPinnedTabs(); activateDiffTab(tab); } },
      { label: "Close all", action: () => { diffTabs = []; untitledDiffTabId = null; persistPinnedTabs(); activeDiffTabId = null; leftText = ""; rightText = ""; selectedFolderFile = ""; } },
    ];
  }

  function handleTabContextMenu(event: MouseEvent, tab: DiffTabState) {
    event.preventDefault();
    event.stopPropagation();
    tabContextMenu = { tab, x: event.clientX, y: event.clientY };
  }

  function handleTabMouseDown(event: MouseEvent, tab: DiffTabState) {
    if (event.button !== 1) return;
    event.preventDefault();
    event.stopPropagation();
    closeDiffTab(tab);
  }


  function handleTabDragStart(event: DragEvent, tab: DiffTabState) {
    draggedDiffTabId = tab.id;
    event.dataTransfer?.setData("text/plain", String(tab.id));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function handleTabDragOver(event: DragEvent) {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  }

  function handleTabDrop(event: DragEvent, target: DiffTabState) {
    event.preventDefault();
    const sourceId = draggedDiffTabId ?? Number(event.dataTransfer?.getData("text/plain"));
    draggedDiffTabId = null;
    if (!sourceId || sourceId === target.id) return;
    const source = diffTabs.find((tab) => tab.id === sourceId);
    if (!source) return;
    const withoutSource = diffTabs.filter((tab) => tab.id !== sourceId);
    const targetIndex = withoutSource.findIndex((tab) => tab.id === target.id);
    if (targetIndex < 0) return;
    diffTabs = [...withoutSource.slice(0, targetIndex), source, ...withoutSource.slice(targetIndex)];
    persistPinnedTabs();
  }

  function handleTabDragEnd() { draggedDiffTabId = null; }
  async function runDiff() {
    const requestId = ++diffRequestId;
    const requestLeft = leftText;
    const requestRight = rightText;
    const requestIgnoreWhitespace = ignoreWhitespace;
    const requestWordDiff = wordDiff;
    const requestAlgorithm = diffAlgorithm;
    try {
      const result = await invoke<{
        added: number; removed: number; changed_blocks: number;
        lines: DiffLine[];
      }>("compute_diff", {
        left: requestLeft,
        right: requestRight,
        ignoreWhitespace: requestIgnoreWhitespace,
        wordDiff: requestWordDiff,
      });
      if (
        requestId !== diffRequestId ||
        requestLeft !== leftText ||
        requestRight !== rightText ||
        requestIgnoreWhitespace !== ignoreWhitespace ||
        requestWordDiff !== wordDiff ||
        requestAlgorithm !== diffAlgorithm
      ) return;
      diffStats = { added: result.added, removed: result.removed, changed_blocks: result.changed_blocks };
      onStatsUpdate?.(diffStats);
      diffData = result.lines;
      await tick();
      diffEditor?.refreshFocusedDetail();

      onStatusUpdate?.(
        requestIgnoreWhitespace ? "Diff updated · whitespace ignored" : "Diff updated",
        "normal"
      );
    } catch (e) {
      if (requestId !== diffRequestId) return;
    if (activeDiffTab && !activeDiffTab.pinned && !activeDiffTab.untitled) pinDiffTab(activeDiffTab);
      detailLeft = `Error: ${e}`;
      detailRight = "";
      detailLeftTokens = [{ text: detailLeft, kind: "delete" }];
      detailRightTokens = [];
      detailKind = "error";
      onStatusUpdate?.(`Diff error: ${e}`, "error");
    }
  }

  function onLeftChange(text: string) {
    leftText = text;
    updateActiveTab({ leftText: text });
    queueMicrotask(() => findBar?.refresh());
    if (activeDiffTab && !activeDiffTab.pinned && !activeDiffTab.untitled) pinDiffTab(activeDiffTab);
    scheduleDiff();
  }

  function onRightChange(text: string) {
    rightText = text;
    updateActiveTab({ rightText: text });
    queueMicrotask(() => findBar?.refresh());
    if (activeDiffTab && !activeDiffTab.pinned && !activeDiffTab.untitled) pinDiffTab(activeDiffTab);
    scheduleDiff();
  }

  function scheduleDiff() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(runDiff, 500);
  }

  function handleKeyDown(e: KeyboardEvent) {
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
    if (side === "left" || side === "right") activeEditorSide = side;
    onCursorChange?.(side, line, col, selLen, chars);
  }

  let diffEditor = $state<DiffEditor>();
  function handleFind(
    query: string,
    flags: { caseSensitive: boolean; wholeWord: boolean; regex: boolean },
    direction: "next" | "prev" | "refresh",
  ) {
    return diffEditor?.search(query, flags, direction) ?? { count: 0, index: 0 };
  }

  function clearLeft() {
    leftText = "";
    queueMicrotask(() => findBar?.refresh());
    void runDiff();
    onStatusUpdate?.("Cleared", "normal");
  }
  function clearRight() {
    rightText = "";
    queueMicrotask(() => findBar?.refresh());
    void runDiff();
    onStatusUpdate?.("Cleared", "normal");
  }

  async function setAlgorithm(algorithm: "legacy" | "advanced") {
    if (diffAlgorithm === algorithm) return;
    diffAlgorithm = algorithm;
    void saveSetting("diff_algorithm", diffAlgorithm);
    onControlStateChange?.({ wordDiff, ignoreWhitespace, showWhitespace, algorithm: diffAlgorithm });
    detailLeft = "";
    detailRight = "";
    detailLeftTokens = [];
    detailRightTokens = [];
    detailKind = "equal";
    editorGeneration += 1;
    await tick();
    await runDiff();
    queueMicrotask(() => findBar?.refresh());
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

  function handleDropEvent(e: Event) {
    const detail = (e as CustomEvent).detail;
    if (typeof detail === "string") {
      leftText = detail;
      if (debounceTimer) clearTimeout(debounceTimer);
      void runDiff();
    }
  }

  function resizeFolderSidebar(width: number) {
    const max = Math.max(280, (diffRoot?.clientWidth ?? 800) * 0.65);
    folderSidebarWidth = Math.round(Math.max(280, Math.min(max, width)));
  }

  function startFolderResize(event: PointerEvent) {
    if (!diffRoot) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = folderSidebarWidth;
    const move = (moveEvent: PointerEvent) => resizeFolderSidebar(startWidth + moveEvent.clientX - startX);
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }

  function handleFolderSplitterKey(event: KeyboardEvent) {
    if (event.key === "ArrowLeft") resizeFolderSidebar(folderSidebarWidth - 24);
    if (event.key === "ArrowRight") resizeFolderSidebar(folderSidebarWidth + 24);
  }

  function clearFolderPreview(preserveText = false) {
    diffRequestId += 1;
    if (!preserveText) selectedFolderFile = "";
    if (!preserveText) {
      leftText = "";
      rightText = "";
    }
    previewKind = "";
    largeEntry = null;
    largePreviewLeft = "";
    largePreviewRight = "";
    diffData = [];
    diffStats = { added: 0, removed: 0, changed_blocks: 0 };
    detailLeft = "";
    detailRight = "";
    detailLeftTokens = [];
    detailRightTokens = [];
    detailKind = "";
  }

  function enterFolderCompare() {
    if (compareMode === "folder") return;
    savedTextDiff = { left: leftText, right: rightText };
    compareMode = "folder";
    void saveSetting("folder_diff_enabled", true);
    clearFolderPreview();
  }

  function exitFolderCompare() {
    if (compareMode !== "folder") return;
    compareMode = "text";
    void saveSetting("folder_diff_enabled", false);
    selectedFolderFile = "";
    largeEntry = null;
    leftText = savedTextDiff.left;
    rightText = savedTextDiff.right;
    void runDiff();
  }

  function toggleFolderCompare() {
    if (compareMode === "folder") exitFolderCompare();
    else enterFolderCompare();
  }

  $effect(() => {
    if (compact && compareMode === "folder") exitFolderCompare();
  });

  function openFolderFiles(left: string, right: string, label: string, kind = "text", entry?: { relative_path: string; left_path: string | null; right_path: string | null }, pinned = false) {
    const existingPinned = diffTabs.find((candidate) => candidate.pinned && candidate.leftPath === (entry?.left_path ?? "") && candidate.rightPath === (entry?.right_path ?? ""));
    const tab = existingPinned ?? ensurePreviewTab(pinned);
    if (existingPinned) activeDiffTabId = existingPinned.id;
    tab.title = shortTabTitle(label);
    tab.folderPath = label;
    tab.previewKind = kind === "large" || mediaExts.includes(kind) ? kind : "";
    tab.leftPath = entry?.left_path ?? "";
    tab.rightPath = entry?.right_path ?? "";
    syncPathInputs(tab);

    tab.leftText = left;
    tab.untitled = false;
    if (tab.id === untitledDiffTabId) untitledDiffTabId = null;
    tab.rightText = right;
    activeDiffTabId = tab.id;
    leftText = left;
    selectedFolderFile = label;
    rightText = right;
    previewKind = kind;
    largeEntry = kind === "large" ? entry ?? null : null;
    largePreviewLeft = kind === "large" ? left : "";
    largePreviewRight = kind === "large" ? right : "";
    viewMode = kind === "large" ? "preview" : "diff";
    queueMicrotask(() => findBar?.refresh());
    if (kind !== "large") void runDiff();
    onStatusUpdate?.(`Loaded ${label} from folder comparison`, "normal");
  }

  async function openDiffPaths(paths: string[]) {
    if (paths.length !== 2) return;
    try {
      const [left, right] = await Promise.all(paths.map((path) => invoke<ReadFileResult>("read_folder_diff_file", { path })));
      if (compareMode === "folder") {
        compareMode = "text";
        void saveSetting("folder_diff_enabled", false);
      }
      const tab = createDiffTab(true, false, false);
      tab.title = `${shortTabTitle(paths[0])} ↔ ${shortTabTitle(paths[1])}`;
      tab.folderPath = "";
      tab.leftPath = paths[0];
      tab.rightPath = paths[1];
      tab.leftText = left.content;
      tab.rightText = right.content;
      activateDiffTab(tab);
      onStatusUpdate?.("Opened files from command line", "success");
    } catch {
      enterFolderCompare();
      await tick();
      document.dispatchEvent(new CustomEvent("folder:setPaths", { detail: paths }));
    }
  }

  function handleOpenDiffPaths(event: Event) {
    const paths = (event as CustomEvent<string[]>).detail;
    if (Array.isArray(paths)) void openDiffPaths(paths);
  }

  export function openPaths(paths: string[]) { void openDiffPaths(paths); }

  async function chooseTabFile(side: "left" | "right") {
    const selected = await open({ multiple: false, directory: false, title: `Choose ${side} diff file` });
    if (typeof selected !== "string") return;
    const tab = activeDiffTab ?? createDiffTab(true);
    try {
      const file = await invoke<ReadFileResult>("read_folder_diff_file", { path: selected });
      if (side === "left") {
        tab.leftPath = selected;
        tab.leftText = file.content;
        leftText = file.content;
      } else {
        tab.rightPath = selected;
        tab.rightText = file.content;
        rightText = file.content;
      }
      tab.untitled = false;
      tab.pinned = true;
      if (tab.id === untitledDiffTabId) untitledDiffTabId = null;
      updateFileTabTitle(tab);
      tab.folderPath = "";
      tab.previewKind = "";
      diffTabs = [...diffTabs];
      activateDiffTab(tab);
      persistPinnedTabs();
    } catch (reason) {
      onStatusUpdate?.(`Could not open ${selected}: ${String(reason)}`, "error");
    }
  }

  function toggleDetail() { showDetail = !showDetail; }

  function clearTabPath(side: "left" | "right") {
    const tab = activeDiffTab;
    if (!tab) return;
    if (side === "left") {
      tab.leftPath = "";
      tab.leftText = "";
      leftPathInput = "";
      leftText = "";
    } else {
      tab.rightPath = "";
      tab.rightText = "";
      rightPathInput = "";
      rightText = "";
    }
    tab.folderPath = "";
    tab.previewKind = "";
    selectedFolderFile = "";
    previewKind = "";
    viewMode = "diff";
    updateFileTabTitle(tab);
    diffTabs = [...diffTabs];
    if (tab.pinned) persistPinnedTabs();
    void runDiff();
  }
  async function applyPathInput(side: "left" | "right") {
    const path = (side === "left" ? leftPathInput : rightPathInput).trim();
    if (!path) return;
    const current = diffTabs.find((tab) => tab.id === activeDiffTabId) ?? diffTabs.find((tab) => tab.id === untitledDiffTabId);
    const currentPath = side === "left" ? current?.leftPath : current?.rightPath;
    try {
      const file = await invoke<ReadFileResult>("read_folder_diff_file", { path });
      const fillsMissingSide = !!current && (side === "left" ? !current.leftPath : !current.rightPath);
      const replaceCurrent = !!current && (current.untitled || currentPath === path || fillsMissingSide);
      const tab = replaceCurrent ? current! : createDiffTab(true, false, false);
      if (!replaceCurrent && current) {
        tab.leftPath = current.leftPath;
        tab.rightPath = current.rightPath;
        tab.leftText = current.leftText;
        tab.rightText = current.rightText;
      }
      if (side === "left") {
        tab.leftPath = path;
        tab.leftText = file.content;
      } else {
        tab.rightPath = path;
        tab.rightText = file.content;
      }
      if (tab.id === untitledDiffTabId) untitledDiffTabId = null;
      tab.folderPath = "";
      tab.untitled = false;
      tab.pinned = true;
      tab.previewKind = "";
      updateFileTabTitle(tab);
      diffTabs = [...diffTabs];
      activateDiffTab(tab);
    } catch (reason) {
      onStatusUpdate?.(`Could not open ${path}: ${String(reason)}`, "error");
    }
  }

  function handleFolderPreview(entry: { relative_path: string; left_path: string | null; right_path: string | null }, pinned = false) {
    const existingPinned = diffTabs.find((candidate) => candidate.pinned && candidate.leftPath === (entry.left_path ?? "") && candidate.rightPath === (entry.right_path ?? ""));
    const tab = existingPinned ?? ensurePreviewTab(pinned);
    if (existingPinned) activeDiffTabId = existingPinned.id;
    tab.title = shortTabTitle(entry.relative_path);
    tab.folderPath = entry.relative_path;
    tab.previewKind = entry.relative_path.split(".").pop()?.toLowerCase() ?? "";
    tab.leftPath = entry.left_path ?? "";
    tab.rightPath = entry.right_path ?? "";
    syncPathInputs(tab);

    activeDiffTabId = tab.id;
    selectedFolderFile = entry.relative_path;
    previewLeftUrl = entry.left_path ? convertFileSrc(entry.left_path) : "";
    previewRightUrl = entry.right_path ? convertFileSrc(entry.right_path) : "";
    previewKind = entry.relative_path.split(".").pop()?.toLowerCase() ?? "";
    viewMode = "preview";
    void loadFolderFileText(entry);
    onStatusUpdate?.(`Previewing ${entry.relative_path}`, "normal");
  }

  async function pinFolderFile(entry: FolderDiffEntry) {
    const preview = diffTabs.find((candidate) => !candidate.pinned && !candidate.untitled && candidate.leftPath === (entry.left_path ?? "") && candidate.rightPath === (entry.right_path ?? ""));
    if (preview) {
      pinDiffTab(preview);
      activateDiffTab(preview);
      return;
    }
    const ext = entry.relative_path.split(".").pop()?.toLowerCase() ?? "";
    const media = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tiff", "tif", "pdf", "mp4", "webm", "mov", "avi", "mkv", "mp3", "wav", "flac"];
    try {
      const [left, right] = await Promise.all([
        entry.left_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.left_path }) : Promise.resolve({ content: "" }),
        entry.right_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.right_path }) : Promise.resolve({ content: "" }),
      ]);
      const large = (entry.left_size ?? 0) > 4 * 1024 * 1024 || (entry.right_size ?? 0) > 4 * 1024 * 1024;
      const kind = large ? "large" : media.includes(ext) ? ext : "text";
      openFolderFiles(left.content, right.content, entry.relative_path, kind, entry, true);
    } catch (reason) {
      onStatusUpdate?.(`Could not pin ${entry.relative_path}: ${String(reason)}`, "error");
    }
  }

  async function loadFolderFileText(entry: { relative_path: string; left_path: string | null; right_path: string | null }) {
    const [left, right] = await Promise.all([
      entry.left_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.left_path }) : Promise.resolve({ content: "" }),
      entry.right_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.right_path }) : Promise.resolve({ content: "" }),
    ]);
    if (selectedFolderFile !== entry.relative_path) return;
    leftText = left.content;
    rightText = right.content;
    updateActiveTab({ leftText: left.content, rightText: right.content });
    queueMicrotask(() => findBar?.refresh());
    void runDiff();
  }

  function decodeHexdump(content: string): Uint8Array | null {
    const lines = content.split(/\r?\n/).filter((line) => line.length > 0);
    if (!lines.length || lines.some((line) => !/^[0-9a-f]{8}\s{2}/i.test(line) || !line.includes("|"))) return null;
    const bytes: number[] = [];
    for (const line of lines) {
      const hex = line.slice(10, line.indexOf("|"));
      const matches = hex.match(/[0-9a-f]{2}/gi);
      if (!matches?.length) return null;
      bytes.push(...matches.map((value) => Number.parseInt(value, 16)));
    }
    return Uint8Array.from(bytes);
  }

  async function savePath(path: string, content: string, label: string, askConfirmation: boolean) {
    if (!path) {
      onStatusUpdate?.(`Save failed for ${label}: no file path`, "error");
      return false;
    }
    if (askConfirmation) {
      const ok = await confirm(`Overwrite ${path}? A backup will be created beside the original file.`, { title: "Save diff file", kind: "warning" });
      if (!ok) return false;
    }
    try {
      await copyFile(path, `${path}.sbt-desktool.bak`);
      const binary = decodeHexdump(content);
      if (binary) await writeFile(path, binary);
      else await writeTextFile(path, content);
      onStatusUpdate?.(`Saved ${label}`, "success");
      return true;
    } catch (reason) {
      onStatusUpdate?.(`Save failed for ${label}: ${String(reason)}`, "error");
      return false;
    }
  }

  async function saveFolderFile(entry: FolderDiffEntry, side: "left" | "right") {
    if (selectedFolderFile !== entry.relative_path) {
      onStatusUpdate?.("Open the file before saving editor content", "error");
      return;
    }
    const path = side === "left" ? entry.left_path : entry.right_path;
    const content = side === "left" ? leftText : rightText;
    if (await savePath(path ?? "", content, `${entry.relative_path} (${side})`, true)) document.dispatchEvent(new CustomEvent("folder:refresh"));
  }

  async function saveActiveDiffFile(side: "left" | "right") {
    const tab = activeDiffTab;
    if (!tab) {
      onStatusUpdate?.("Save failed: no active diff tab", "error");
      return;
    }
    const path = side === "left" ? tab.leftPath : tab.rightPath;
    const content = side === "left" ? leftText : rightText;
    if (await savePath(path, content, `${tab.title} (${side})`, false) && compareMode === "folder") document.dispatchEvent(new CustomEvent("folder:refresh"));
  }

  async function handleExternalFolderChange(entry: FolderDiffEntry | null) {
    if (!entry) {
      const ok = await confirm("The active folder-diff file was removed. Clear it from the editor?", { title: "File removed", kind: "warning" });
      if (ok) {
        selectedFolderFile = "";
        leftText = "";
        rightText = "";
        clearFolderPreview();
        await runDiff();
      }
      return;
    }
    if (selectedFolderFile !== entry.relative_path) return;
    const ok = await confirm(`${entry.relative_path} changed on disk. Reload it and discard unsaved editor changes?`, { title: "File changed", kind: "warning" });
    if (!ok) return;
    if (isPreviewable()) {
      previewLeftUrl = entry.left_path ? convertFileSrc(entry.left_path) : "";
      previewRightUrl = entry.right_path ? convertFileSrc(entry.right_path) : "";
    }
    await loadFolderFileText(entry);
    onStatusUpdate?.(`Reloaded ${entry.relative_path}`, "normal");
  }

  const mediaExts = ["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","tif","pdf","mp4","webm","mov","avi","mkv","mp3","wav","flac"];
  function isPreviewable() { return compareMode === "folder" && (mediaExts.includes(previewKind) || previewKind === "large"); }

async function showTextDiff() {
    if (previewKind === "large" && largeEntry) {
      const entry = largeEntry;
      const [left, right] = await Promise.all([
        entry.left_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.left_path }) : Promise.resolve({ content: "" }),
        entry.right_path ? invoke<ReadFileResult>("read_folder_diff_file", { path: entry.right_path }) : Promise.resolve({ content: "" }),
      ]);
      if (selectedFolderFile !== entry.relative_path) return;
      leftText = left.content;
      rightText = right.content;
      queueMicrotask(() => findBar?.refresh());
      void runDiff();
      onStatusUpdate?.(`Loaded text content of ${entry.relative_path}`, "normal");
    }
    viewMode = "diff";
  }

  function showPreview() {
    viewMode = "preview";
  }

  function toggleWordDiff() {
    wordDiff = !wordDiff;
    void saveSetting("diff_word_diff", wordDiff);
    onControlStateChange?.({ wordDiff, ignoreWhitespace, showWhitespace, algorithm: diffAlgorithm });
    void runDiff();
  }

  function toggleIgnoreWhitespace() {
    ignoreWhitespace = !ignoreWhitespace;
    void saveSetting("diff_ignore_whitespace", ignoreWhitespace);
    onControlStateChange?.({ wordDiff, ignoreWhitespace, showWhitespace, algorithm: diffAlgorithm });
    void runDiff();
  }

  function toggleShowWhitespace() {
    showWhitespace = !showWhitespace;
    void saveSetting("diff_show_whitespace", showWhitespace);
    onControlStateChange?.({ wordDiff, ignoreWhitespace, showWhitespace, algorithm: diffAlgorithm });
  }

  export function toggleWordControl() { toggleWordDiff(); }
  export function toggleIgnoreWhitespaceControl() { toggleIgnoreWhitespace(); }
  export function toggleWhitespaceControl() { toggleShowWhitespace(); }
  export function setAlgorithmControl(algorithm: "legacy" | "advanced") { void setAlgorithm(algorithm); }

  function toggleCommonFind() {
    findOpen = !findOpen;
    if (findOpen) queueMicrotask(() => findBar?.focus());
    else {
      diffEditor?.search("", { caseSensitive: false, wholeWord: false, regex: false }, "refresh");
      diffEditor?.focusActive();
    }
  }

  export function openCommonFind() {
    findOpen = true;
    queueMicrotask(() => findBar?.focus());
  }

  function closeCommonFind() {
    findOpen = false;
    diffEditor?.focusActive();
  }

  async function toggleSideFind(side: "left" | "right") {
    const visible = await diffEditor?.toggleEditorFind(side) ?? false;
    if (side === "left") leftFindOpen = visible;
    else rightFindOpen = visible;
  }

  function handleSideFindVisibility(side: "left" | "right", visible: boolean) {
    if (side === "left") leftFindOpen = visible;
    else rightFindOpen = visible;
  }

  async function handleActionGutterChange() {
    showCenterControls = !showCenterControls;
    // Monaco can retain already-created revert-arrow view zones after a live
    // option update. Recreate the editor so its initial options and DOM agree
    // with the button state. Text remains in the parent state.
    editorGeneration += 1;
    await tick();
    queueMicrotask(() => findBar?.refresh());
  }

  onMount(() => {
    document.addEventListener("diff:openPaths", handleOpenDiffPaths);
    document.addEventListener("diff:setLeft", handleDropEvent);
    void (async () => {
      const settings = await loadSettings();
      if (settings.folder_diff_enabled && !compact) compareMode = "folder";
      await restorePinnedTabs(settings.diff_pinned_tabs);
      await runDiff();
    })();
  });

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
    // Ignore a command that completes after this tab has been removed.
    diffRequestId++;
    document.removeEventListener("diff:setLeft", handleDropEvent);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={diffRoot} class="diff-tab" class:folder-layout={compareMode === "folder" && !compact} style={`--folder-sidebar-width: ${folderSidebarWidth}px`} onkeydown={handleKeyDown} onwheel={handleWheel}>
  {#if !compact}
  <div class="diff-header">
    <div class="pane-header left-header">
      <button class="pane-btn" onclick={clearLeft} title="Clear Left" aria-label="Clear Left"><AppIcon name="clear" size={14} /><span class="btn-label">Clear</span></button>
      <button class="pane-btn" onclick={() => copyText(leftText)} title="Copy Left" aria-label="Copy Left"><AppIcon name="copy" size={14} /><span class="btn-label">Copy</span></button>
      {#if compareMode !== "folder"}<button class="pane-btn" onclick={() => void chooseTabFile("left")} title="Choose left diff file" aria-label="Choose left diff file"><AppIcon name="add" size={14} /><span class="btn-label">Choose</span></button>{/if}
    </div>
      <div class="diff-actions">
        <div class="control-group" aria-label="Comparison mode">
          <button class="icon-btn folder-mode-btn" class:toggled={compareMode === "folder"} onclick={toggleFolderCompare} title="Show or hide folder comparison"><AppIcon name="diff" size={14} /><span class="btn-label">Folder</span></button>
        </div>
        <div class="control-group diff-view-group" aria-label="Diff text display">
          <button class="icon-btn" class:toggled={wordDiff} aria-pressed={wordDiff} onclick={toggleWordDiff} title="Highlight changed words and characters"><AppIcon name="word" size={14} /><span class="btn-label">Word</span></button>
          <button class="icon-btn" class:toggled={ignoreWhitespace} aria-pressed={ignoreWhitespace} onclick={toggleIgnoreWhitespace} title="Ignore whitespace-only changes"><AppIcon name="ignore-whitespace" size={14} /><span class="btn-label">Ignore WS</span></button>
          <button class="icon-btn" class:toggled={wordWrap} aria-pressed={wordWrap} onclick={onToggleWrap} title="Toggle word wrap for Diff"><AppIcon name="wrap" size={14} /><span class="btn-label">Wrap</span></button>
          <button class="icon-btn" class:toggled={showWhitespace} aria-pressed={showWhitespace} onclick={toggleShowWhitespace} title="Show or hide whitespace characters"><AppIcon name="whitespace" size={14} /><span class="btn-label">Show WS</span></button>
        </div>
        <div class="control-group" aria-label="Diff algorithm">
          <button class="icon-btn algorithm-btn" class:toggled={diffAlgorithm === "legacy"} onclick={() => void setAlgorithm("legacy")} title="Use legacy diff alignment"><AppIcon name="legacy" size={14} /><span class="btn-label">Legacy</span></button>
          <button class="icon-btn algorithm-btn" class:toggled={diffAlgorithm === "advanced"} onclick={() => void setAlgorithm("advanced")} title="Use advanced diff alignment"><AppIcon name="advanced" size={14} /><span class="btn-label">Advanced</span></button>
        </div>
        <div class="control-group" aria-label="Layout visibility">
          <button class="icon-btn layout-btn" class:toggled={showDetail} onclick={toggleDetail} title="Show or hide focused-line details">
            <AppIcon name="detail" size={14} /><span class="btn-label">Detail</span>
          </button>
          <button class="icon-btn layout-btn" class:toggled={showCenterControls} onclick={() => void handleActionGutterChange()} title="Show or hide copy/revert action gutter between editors">
            <AppIcon name="actions" size={14} /><span class="btn-label">Actions</span>
          </button>
        </div>
        {#if isPreviewable()}
        <div class="control-group" aria-label="View mode">
          <button class="icon-btn" class:toggled={viewMode === "diff"} onclick={() => void showTextDiff()} title="Text diff view"><AppIcon name="diff" size={14} /><span class="btn-label">Text Diff</span></button>
          <button class="icon-btn" class:toggled={viewMode === "preview"} onclick={showPreview} title="Preview view"><AppIcon name="preview" size={14} /><span class="btn-label">Preview</span></button>
        </div>
        {/if}
        <div class="control-group" aria-label="Search controls">
          <button class="icon-btn" class:toggled={findOpen} onclick={toggleCommonFind} title="Show or hide common search"><AppIcon name="search" size={14} /><span class="btn-label">All</span></button>
          <button class="icon-btn" class:toggled={leftFindOpen} onclick={() => void toggleSideFind("left")} title="Show or hide Left editor search"><AppIcon name="search-left" size={14} /><span class="btn-label">L</span></button>
          <button class="icon-btn" class:toggled={rightFindOpen} onclick={() => void toggleSideFind("right")} title="Show or hide Right editor search"><AppIcon name="search-right" size={14} /><span class="btn-label">R</span></button>
        </div>
      </div>
    <div class="pane-header right-header">
      <button class="pane-btn" onclick={clearRight} title="Clear Right" aria-label="Clear Right"><AppIcon name="clear" size={14} /><span class="btn-label">Clear</span></button>
      <button class="pane-btn" onclick={() => copyText(rightText)} title="Copy Right" aria-label="Copy Right"><AppIcon name="copy" size={14} /><span class="btn-label">Copy</span></button>
      {#if compareMode !== "folder"}<button class="pane-btn" onclick={() => void chooseTabFile("right")} title="Choose right diff file" aria-label="Choose right diff file"><AppIcon name="add" size={14} /><span class="btn-label">Choose</span></button>{/if}
    </div>
  </div>
  {/if}

  {#if compareMode === "folder" && !compact}
    <aside class="folder-sidebar" aria-label="Folder comparison">
      <FolderDiffPanel onOpenFiles={openFolderFiles} {onStatusUpdate} selectedFile={selectedFolderFile} onPreview={handleFolderPreview} onPinFile={pinFolderFile} onSaveFile={saveFolderFile} onExternalChange={handleExternalFolderChange} />
    </aside>
    <div class="folder-splitter" role="separator" aria-orientation="vertical" aria-label="Resize folder panel" tabindex="0" onpointerdown={startFolderResize} onkeydown={handleFolderSplitterKey}></div>
  {/if}

  {#if findOpen}
    <div class="diff-find">
      <FindBar bind:this={findBar} settingsPrefix="diff" onSearch={handleFind} onClose={closeCommonFind} />
    </div>
  {/if}

  <div class="diff-tabbar" role="tablist" aria-label="Diff tabs">
    {#if !diffTabs.length}
      <span class="no-tabs-label">No diff tabs open</span>
    {:else}
      {#each diffTabs as tab (tab.id)}
        <button class="diff-tab-item" class:active={tab.id === activeDiffTabId} class:preview={!tab.pinned} class:dragging={draggedDiffTabId === tab.id}
          draggable="true"
          role="tab" aria-selected={tab.id === activeDiffTabId} title={tab.title}
          onclick={() => activateDiffTab(tab)} ondblclick={() => pinDiffTab(tab)} onmousedown={(event) => handleTabMouseDown(event, tab)}
          ondragstart={(event) => handleTabDragStart(event, tab)} ondragover={handleTabDragOver} ondrop={(event) => handleTabDrop(event, tab)} ondragend={handleTabDragEnd}
          oncontextmenu={(event) => handleTabContextMenu(event, tab)}>
          <span class="diff-tab-title">{tab.title}</span>
          <span class="diff-tab-close" role="button" tabindex="0" onclick={(event) => { event.stopPropagation(); closeDiffTab(tab); }} onkeydown={(event) => { if (event.key === "Enter" || event.key === " ") { event.stopPropagation(); closeDiffTab(tab); } }} aria-label={`Close ${tab.title}`}>×</span>
        </button>
      {/each}
    {/if}
    <button class="diff-tab-add" onclick={() => { const tab = createUntitledDiff(); activateDiffTab(tab); }} title="New diff tab" aria-label="New diff tab">+</button>
  </div>
  {#if tabContextMenu}
    <ContextMenu items={tabContextItems(tabContextMenu.tab)} x={tabContextMenu.x} y={tabContextMenu.y} onClose={() => (tabContextMenu = null)} />
  {/if}
  {#if activeDiffTab}
    <div class="diff-pathbar" aria-label="Diff file paths">
      <div class="path-side">
        <span class="path-label">Left</span>
        <div class="path-input-group">
        <input class="path-input" value={leftPathInput} placeholder="Left file path" title={leftPathInput || "Left file path"} oninput={(event) => (leftPathInput = event.currentTarget.value)} onkeydown={(event) => { if (event.key === "Enter") void applyPathInput("left"); }} aria-label="Left file path" />
        <button class="path-action" onclick={() => void chooseTabFile("left")} title="Browse left file" aria-label="Browse left file"><AppIcon name="add" size={13} /></button>
        <button class="path-action" onclick={() => clearTabPath("left")} title="Clear left file" aria-label="Clear left file"><AppIcon name="clear" size={13} /></button>
        </div>
      </div>
      <div class="path-side">
        <span class="path-label">Right</span>
        <div class="path-input-group">
        <input class="path-input" value={rightPathInput} placeholder="Right file path" title={rightPathInput || "Right file path"} oninput={(event) => (rightPathInput = event.currentTarget.value)} onkeydown={(event) => { if (event.key === "Enter") void applyPathInput("right"); }} aria-label="Right file path" />
        <button class="path-action" onclick={() => void chooseTabFile("right")} title="Browse right file" aria-label="Browse right file"><AppIcon name="add" size={13} /></button>
        <button class="path-action" onclick={() => clearTabPath("right")} title="Clear right file" aria-label="Clear right file"><AppIcon name="clear" size={13} /></button>
        </div>
      </div>
    </div>
  {/if}

  <div class="diff-body">
    {#if !diffTabs.length}
      <div class="no-diff-tabs">No diff tab is open. Click + to create one, or choose a file from Folder Diff.</div>
    {:else if activeDiffTab && !activeDiffTab.leftPath && !activeDiffTab.rightPath}
      <div class="no-diff-tabs">New diff tab: enter a left and/or right file path above, then press Enter to compare.</div>
    {:else if viewMode === "preview" && previewKind === "large"}
      <div class="media-preview">
        <div class="media-side large-side">
          <span class="large-placeholder">{largePreviewLeft || "No file on left"}</span>
        </div>
        <div class="media-side large-side">
          <span class="large-placeholder">{largePreviewRight || "No file on right"}</span>
        </div>
      </div>
    {:else if viewMode === "preview" && isPreviewable()}
      <div class="media-preview">
        <div class="media-side" class:empty={!previewLeftUrl}>
          {#if previewLeftUrl}
            {#if ["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","tif"].includes(previewKind)}
              <img src={previewLeftUrl} alt="" />
            {:else if previewKind === "pdf"}
              <iframe src={previewLeftUrl} title="Left"></iframe>
            {:else if ["mp4","webm","mov","avi","mkv"].includes(previewKind)}
              <video src={previewLeftUrl} controls></video>
            {:else if ["mp3","wav","flac"].includes(previewKind)}
              <audio src={previewLeftUrl} controls></audio>
            {/if}
          {:else}
            <span class="media-empty">No file on left</span>
          {/if}
        </div>
        <div class="media-side" class:empty={!previewRightUrl}>
          {#if previewRightUrl}
            {#if ["png","jpg","jpeg","gif","webp","bmp","svg","ico","tiff","tif"].includes(previewKind)}
              <img src={previewRightUrl} alt="" />
            {:else if previewKind === "pdf"}
              <iframe src={previewRightUrl} title="Right"></iframe>
            {:else if ["mp4","webm","mov","avi","mkv"].includes(previewKind)}
              <video src={previewRightUrl} controls></video>
            {:else if ["mp3","wav","flac"].includes(previewKind)}
              <audio src={previewRightUrl} controls></audio>
            {/if}
          {:else}
            <span class="media-empty">No file on right</span>
          {/if}
        </div>
      </div>
    {:else}
      {#key editorGeneration}
        <DiffEditor
          bind:this={diffEditor}
          {fontSize}
          {wordWrap}
          {theme}
          {wordDiff}
          {ignoreWhitespace}
          {showWhitespace}
          {diffAlgorithm}
          showCenterControls={showCenterControls && !compact}
          leftValue={leftText}
          rightValue={rightText}
          diffData={diffData}
          sashRatio={initialSashRatio}
          onChangeLeft={onLeftChange}
          onChangeRight={onRightChange}
          onSave={(side) => void saveActiveDiffFile(side)}
          onCursorChange={handleCursorChange}
          onFindVisibilityChange={handleSideFindVisibility}
          {onZoom}
          onDetailChange={(left, right, kind, leftTokens, rightTokens) => {
            detailLeft = left;
            detailRight = right;
            detailLeftTokens = leftTokens;
            detailRightTokens = rightTokens;
            detailKind = kind;
          }}
        />
      {/key}
    {/if}
  </div>

  {#if showDetail && !compact}
    <div class="detail-panel">
      <span class="detail-label">Left</span>
      <div class="detail-side" class:deleted={detailKind === "delete" || detailKind === "replace"}>
        {#if detailLeft}
          <span class="detail-prefix">−</span>
          <span class="detail-text">
            {#if detailLeftTokens.length}
            {#each detailLeftTokens as token}
              <span class:detail-delete={token.kind === "delete"}>{token.text}</span>
            {/each}
            {:else}{detailLeft}{/if}
          </span>
        {/if}
      </div>
      <span class="detail-label">Right</span>
      <div class="detail-side" class:inserted={detailKind === "insert" || detailKind === "replace"}>
        {#if detailRight}
          <span class="detail-prefix">+</span>
          <span class="detail-text">
            {#if detailRightTokens.length}
            {#each detailRightTokens as token}
              <span class:detail-insert={token.kind === "insert"}>{token.text}</span>
            {/each}
            {:else}{detailRight}{/if}
          </span>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .diff-tab { display: flex; flex-direction: column; flex: 1; overflow: hidden; }
  .diff-tab.folder-layout { display: grid; grid-template-columns: minmax(280px, 36%) minmax(0, 1fr); grid-template-rows: 28px auto 28px 30px minmax(0, 1fr) auto; }
  .folder-sidebar { min-width: 0; overflow: hidden; }
  .folder-layout .folder-sidebar { display: flex; grid-column: 1; grid-row: 1 / -1; border-right: 1px solid var(--border); }
  .folder-layout .diff-header { grid-column: 2; grid-row: 1; }
  .folder-layout .diff-find { grid-column: 2; grid-row: 2; min-width: 0; }
  .folder-layout .diff-tabbar { grid-column: 2; grid-row: 3; min-width: 0; }
  .folder-layout .diff-pathbar { grid-column: 2; grid-row: 4; min-width: 0; }
  .folder-layout .diff-body { grid-column: 2; grid-row: 5; min-width: 0; min-height: 0; }
  .folder-layout .detail-panel { grid-column: 2; grid-row: 6; }
  .folder-layout .pane-header { visibility: hidden; pointer-events: none; }
  .folder-layout.new-diff-layout .pane-header { visibility: visible; pointer-events: auto; }
  .folder-layout .diff-actions { margin-inline: auto; }
  .diff-tab.folder-layout { grid-template-columns: minmax(280px, max-content) minmax(0, 1fr); }
  .folder-layout .folder-sidebar { width: 360px; min-width: 280px; max-width: 55vw; resize: horizontal; overflow: auto; }
  .folder-layout .folder-sidebar::-webkit-resizer { background: var(--border); }
  .diff-tab.folder-layout { grid-template-columns: minmax(280px, var(--folder-sidebar-width)) minmax(0, 1fr); }
  .folder-layout .folder-sidebar { width: auto; min-width: 0; max-width: none; resize: none; overflow: hidden; }
  .folder-layout .folder-splitter { grid-column: 1; grid-row: 1 / -1; justify-self: end; width: 8px; transform: translateX(4px); cursor: col-resize; z-index: 5; touch-action: none; }
  .folder-layout .folder-splitter:hover, .folder-layout .folder-splitter:focus { background: color-mix(in srgb, var(--accent) 55%, transparent); outline: none; }
  .diff-header { display: flex; align-items: center; height: 28px; background: var(--bg3); border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .pane-header { display: flex; align-items: center; gap: 4px; padding: 0 8px; }
  .left-header { flex: 1; padding-left: 0; }
  .right-header { flex: 1; justify-content: flex-end; }
  .pane-title { font-size: 11px; font-weight: 600; color: var(--fg2); padding-left: 5px; }
  .diff-actions { display: flex; align-items: center; gap: 6px; padding: 0 8px; }
  .pane-btn { display: inline-flex; align-items: center; justify-content: center; gap: 3px; height: var(--control-height); padding: 0 var(--control-padding-x); background: transparent; border: none; color: var(--fg2); font-family: inherit; font-size: 11px; line-height: 1; white-space: nowrap; flex: 0 0 auto; cursor: pointer; border-radius: var(--control-radius); }
  .pane-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .path-input { flex: 1 1 0; min-width: 0; height: 22px; padding: 0 6px; border: 1px solid var(--border); border-radius: 3px; background: var(--bg2); color: var(--fg); font: inherit; font-size: 11px; }
  .pane-btn :global(.app-icon), .icon-btn :global(.app-icon) { width: 15px; height: 15px; }
  .control-group { display: inline-flex; align-items: center; border: 1px solid var(--border); border-radius: 4px; overflow: hidden; }
  .icon-btn { display: inline-flex; align-items: center; justify-content: center; gap: 3px; height: var(--control-height); min-width: var(--control-height); padding: 0 5px; border: 0; border-right: 1px solid var(--border); background: var(--bg2); color: var(--fg2); font: inherit; font-size: 10px; line-height: 1; white-space: nowrap; flex: 0 0 auto; cursor: pointer; }
  .icon-btn:last-child { border-right: 0; }
  .icon-btn:hover { background: var(--btn-hover); color: var(--fg); }
  .icon-btn.toggled { background: color-mix(in srgb, var(--accent) 22%, var(--bg2)); color: var(--accent); }
  .algorithm-btn { min-width: 52px; }
  .layout-btn { min-width: 58px; }
  .diff-view-group .icon-btn { min-width: 48px; }
  .diff-view-group .icon-btn:nth-child(2), .diff-view-group .icon-btn:nth-child(4) { min-width: 68px; }
  .diff-body { display: flex; flex: 1; overflow: hidden; }
  .diff-tabbar { display: flex; align-items: center; gap: 2px; height: 28px; min-height: 28px; padding: 2px 5px; overflow-x: auto; background: var(--bg2); border-bottom: 1px solid var(--border); }
  .no-tabs-label { color: var(--fg3); font-size: 11px; padding-inline: 5px; }
  .diff-tab-item, .diff-tab-add { display: inline-flex; align-items: center; gap: 7px; height: 23px; min-width: 88px; max-width: 220px; padding: 0 7px; border: 1px solid transparent; border-radius: 3px 3px 0 0; background: transparent; color: var(--fg2); font: inherit; font-size: 11px; cursor: pointer; white-space: nowrap; }
  .diff-pathbar { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); padding: 3px 0; background: var(--bg3); border-bottom: 1px solid var(--border); }
  .path-side { display: flex; align-items: center; gap: 5px; min-width: 0; padding: 0 6px; }
  .path-side + .path-side { border-left: 1px solid var(--border); }
  .path-label { flex: 0 0 auto; color: var(--fg2); font-size: 10px; font-weight: 600; }
  .path-input-group { display: flex; align-items: stretch; flex: 1; min-width: 0; height: 23px; overflow: hidden; border: 1px solid var(--border); border-radius: 3px; background: var(--bg2); }
  .diff-pathbar .path-input { flex: 1; min-width: 0; width: auto; height: 21px; padding: 0 6px; border: 0; border-radius: 0; background: transparent; }
  .path-action { display: inline-flex; align-items: center; justify-content: center; width: 24px; padding: 0; border: 0; border-left: 1px solid var(--border); border-radius: 0; background: transparent; color: var(--fg2); cursor: pointer; }
  .path-action:hover { background: var(--btn-hover); color: var(--fg); }
  .diff-tab-item:hover, .diff-tab-add:hover { background: var(--btn-hover); color: var(--fg); }
  .diff-tab-item.active { border-color: var(--border); border-bottom-color: var(--accent); background: var(--bg3); color: var(--fg); }
  .diff-tab-item.preview .diff-tab-title { font-style: italic; }
  .diff-tab-title { overflow: hidden; text-overflow: ellipsis; }
  .diff-tab-close { margin-left: auto; color: var(--fg3); font-size: 14px; line-height: 1; }
  .diff-tab-close:hover { color: var(--error); }
  .diff-tab-add { min-width: 24px; width: 24px; justify-content: center; padding: 0; font-size: 17px; }
  .no-diff-tabs { display: flex; align-items: center; justify-content: center; flex: 1; color: var(--fg3); font-size: 12px; }
  .media-preview { display: flex; flex: 1; gap: 1px; background: var(--border); }
  .media-side { flex: 1; display: flex; align-items: center; justify-content: center; background: var(--bg); overflow: hidden; padding: 8px; }
  .media-side img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .media-side.empty { background: color-mix(in srgb, var(--bg2) 60%, var(--bg)); }
  .media-side.empty .media-empty { color: var(--fg3); font-size: 12px; }
  .media-side iframe { width: 100%; height: 100%; border: 0; }
  .media-side video, .media-side audio { max-width: 100%; max-height: 100%; }
  .media-empty { color: var(--fg3); font-size: 13px; }
  .media-side.large-side { align-items: center; justify-content: center; }
  .large-placeholder { font-family: 'JetBrains Mono','Consolas',monospace; font-size: 12px; color: var(--fg2); padding: 12px; }
  .detail-panel { display: grid; grid-template-columns: auto minmax(0, 1fr); grid-template-rows: 1fr 1fr; align-items: center; gap: 2px 6px; height: 58px; padding: 3px 8px; background: var(--bg2); border-top: 1px solid var(--border); font-family: 'JetBrains Mono','Consolas',monospace; font-size: 11px; flex-shrink: 0; overflow: hidden; }
  .detail-label { color: var(--fg2); font-size: 11px; font-weight: 600; flex-shrink: 0; }
  .detail-side { display: flex; min-width: 0; gap: 4px; padding: 2px 5px; background: var(--bg); border: 1px solid var(--border); }
  .detail-side.deleted { background: var(--diff-del-bg); }
  .detail-side.inserted { background: var(--diff-add-bg); }
  .detail-prefix { color: var(--accent2); flex-shrink: 0; }
  .detail-text { color: var(--fg); white-space: pre; overflow: hidden; text-overflow: ellipsis; }
  .detail-delete { background: color-mix(in srgb, var(--diff-del-inline) 68%, transparent); border-radius: 2px; }
  .detail-insert { background: color-mix(in srgb, var(--diff-add-inline) 68%, transparent); border-radius: 2px; }
  .diff-tab-item.dragging { opacity: .45; }

  @media (max-width: 980px) {
    .diff-actions { gap: 2px; padding-inline: 2px; }
    .pane-header { gap: 1px; padding-inline: 2px; }
    .pane-btn, .icon-btn, .algorithm-btn, .layout-btn,
    .diff-view-group .icon-btn,
    .diff-view-group .icon-btn:nth-child(2),
    .diff-view-group .icon-btn:nth-child(4) {
      width: var(--control-height);
      min-width: var(--control-height);
      padding: 0;
    }
    .btn-label { display: none; }
  }

  @media (max-width: 520px) {
    .pane-title { display: none; }
    .diff-actions { gap: 1px; padding-inline: 1px; }
    .control-group { border-radius: 3px; }
  }
</style>
