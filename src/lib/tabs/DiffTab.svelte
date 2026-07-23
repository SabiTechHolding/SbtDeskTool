<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import DiffEditor from "../components/DiffEditor.svelte";
  import FindBar from "../components/FindBar.svelte";
  import AppIcon from "../components/AppIcon.svelte";
  import { saveSetting } from "../stores/settings";
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

  let {
    compact, wordWrap, theme, fontSize, wordDiff: initialWordDiff, ignoreWhitespace: initialIgnoreWhitespace, showWhitespace: initialShowWhitespace, initialAlgorithm, initialSashRatio, onZoom, onToggleWrap, onCursorChange, onStatusUpdate, onStatsUpdate,
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
  let leftFindOpen = $state(false);
  let rightFindOpen = $state(false);
  let editorGeneration = $state(0);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let diffRequestId = 0;

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
    queueMicrotask(() => findBar?.refresh());
    scheduleDiff();
  }

  function onRightChange(text: string) {
    rightText = text;
    queueMicrotask(() => findBar?.refresh());
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

  function toggleDetail() { showDetail = !showDetail; }

  function toggleWordDiff() {
    wordDiff = !wordDiff;
    void saveSetting("diff_word_diff", wordDiff);
    void runDiff();
  }

  function toggleIgnoreWhitespace() {
    ignoreWhitespace = !ignoreWhitespace;
    void saveSetting("diff_ignore_whitespace", ignoreWhitespace);
    void runDiff();
  }

  function toggleShowWhitespace() {
    showWhitespace = !showWhitespace;
    void saveSetting("diff_show_whitespace", showWhitespace);
  }

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
    document.addEventListener("diff:setLeft", handleDropEvent);
    void runDiff();
  });

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
    // Ignore a command that completes after this tab has been removed.
    diffRequestId++;
    document.removeEventListener("diff:setLeft", handleDropEvent);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="diff-tab" onkeydown={handleKeyDown} onwheel={handleWheel}>
  {#if !compact}
  <div class="diff-header">
    <div class="pane-header left-header">
      <span class="pane-title">◀ Left</span>
      <button class="pane-btn" onclick={clearLeft} title="Clear Left" aria-label="Clear Left"><AppIcon name="clear" size={12} /><span class="btn-label">Clear</span></button>
      <button class="pane-btn" onclick={() => copyText(leftText)} title="Copy Left" aria-label="Copy Left"><AppIcon name="copy" size={12} /><span class="btn-label">Copy</span></button>
    </div>
      <div class="diff-actions">
        <div class="control-group diff-view-group" aria-label="Diff text display">
          <button class="icon-btn" class:toggled={wordDiff} aria-pressed={wordDiff} onclick={toggleWordDiff} title="Highlight changed words and characters"><AppIcon name="word" size={12} /><span class="btn-label">Word</span></button>
          <button class="icon-btn" class:toggled={ignoreWhitespace} aria-pressed={ignoreWhitespace} onclick={toggleIgnoreWhitespace} title="Ignore whitespace-only changes"><AppIcon name="ignore-whitespace" size={12} /><span class="btn-label">Ignore WS</span></button>
          <button class="icon-btn" class:toggled={wordWrap} aria-pressed={wordWrap} onclick={onToggleWrap} title="Toggle word wrap for Diff"><AppIcon name="wrap" size={12} /><span class="btn-label">Wrap</span></button>
          <button class="icon-btn" class:toggled={showWhitespace} aria-pressed={showWhitespace} onclick={toggleShowWhitespace} title="Show or hide whitespace characters"><AppIcon name="whitespace" size={12} /><span class="btn-label">Show WS</span></button>
        </div>
        <div class="control-group" aria-label="Diff algorithm">
          <button class="icon-btn algorithm-btn" class:toggled={diffAlgorithm === "legacy"} onclick={() => void setAlgorithm("legacy")} title="Use legacy diff alignment"><AppIcon name="legacy" size={12} /><span class="btn-label">Legacy</span></button>
          <button class="icon-btn algorithm-btn" class:toggled={diffAlgorithm === "advanced"} onclick={() => void setAlgorithm("advanced")} title="Use advanced diff alignment"><AppIcon name="advanced" size={12} /><span class="btn-label">Advanced</span></button>
        </div>
        <div class="control-group" aria-label="Layout visibility">
          <button class="icon-btn layout-btn" class:toggled={showDetail} onclick={toggleDetail} title="Show or hide focused-line details">
            <AppIcon name="detail" size={12} /><span class="btn-label">Detail</span>
          </button>
          <button class="icon-btn layout-btn" class:toggled={showCenterControls} onclick={() => void handleActionGutterChange()} title="Show or hide copy/revert action gutter between editors">
            <AppIcon name="actions" size={12} /><span class="btn-label">Actions</span>
          </button>
        </div>
        <div class="control-group" aria-label="Search controls">
          <button class="icon-btn" class:toggled={findOpen} onclick={toggleCommonFind} title="Show or hide common search"><AppIcon name="search" size={12} /><span class="btn-label">All</span></button>
          <button class="icon-btn" class:toggled={leftFindOpen} onclick={() => void toggleSideFind("left")} title="Show or hide Left editor search"><AppIcon name="search-left" size={12} /><span class="btn-label">L</span></button>
          <button class="icon-btn" class:toggled={rightFindOpen} onclick={() => void toggleSideFind("right")} title="Show or hide Right editor search"><AppIcon name="search-right" size={12} /><span class="btn-label">R</span></button>
        </div>
      </div>
    <div class="pane-header right-header">
      <span class="pane-title">▶ Right</span>
      <button class="pane-btn" onclick={clearRight} title="Clear Right" aria-label="Clear Right"><AppIcon name="clear" size={12} /><span class="btn-label">Clear</span></button>
      <button class="pane-btn" onclick={() => copyText(rightText)} title="Copy Right" aria-label="Copy Right"><AppIcon name="copy" size={12} /><span class="btn-label">Copy</span></button>
    </div>
  </div>
  {/if}

  {#if findOpen}
    <FindBar bind:this={findBar} settingsPrefix="diff" onSearch={handleFind} onClose={closeCommonFind} />
  {/if}

  <div class="diff-body">
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
  .diff-header { display: flex; align-items: center; height: 28px; background: var(--bg3); border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .pane-header { display: flex; align-items: center; gap: 4px; padding: 0 8px; }
  .left-header { flex: 1; }
  .right-header { flex: 1; justify-content: flex-end; }
  .pane-title { font-size: 11px; font-weight: 600; color: var(--fg2); }
  .diff-actions { display: flex; align-items: center; gap: 6px; padding: 0 8px; }
  .pane-btn { display: inline-flex; align-items: center; justify-content: center; gap: 3px; height: var(--control-height); padding: 0 var(--control-padding-x); background: transparent; border: none; color: var(--fg2); font-family: inherit; font-size: 11px; line-height: 1; white-space: nowrap; flex: 0 0 auto; cursor: pointer; border-radius: var(--control-radius); }
  .pane-btn:hover { background: var(--btn-hover); color: var(--fg); }
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
  .detail-panel { display: grid; grid-template-columns: auto minmax(0, 1fr); grid-template-rows: 1fr 1fr; align-items: center; gap: 2px 6px; height: 58px; padding: 3px 8px; background: var(--bg2); border-top: 1px solid var(--border); font-family: 'JetBrains Mono','Consolas',monospace; font-size: 11px; flex-shrink: 0; overflow: hidden; }
  .detail-label { color: var(--fg2); font-size: 11px; font-weight: 600; flex-shrink: 0; }
  .detail-side { display: flex; min-width: 0; gap: 4px; padding: 2px 5px; background: var(--bg); border: 1px solid var(--border); }
  .detail-side.deleted { background: var(--diff-del-bg); }
  .detail-side.inserted { background: var(--diff-add-bg); }
  .detail-prefix { color: var(--accent2); flex-shrink: 0; }
  .detail-text { color: var(--fg); white-space: pre; overflow: hidden; text-overflow: ellipsis; }
  .detail-delete { background: color-mix(in srgb, var(--diff-del-inline) 68%, transparent); border-radius: 2px; }
  .detail-insert { background: color-mix(in srgb, var(--diff-add-inline) 68%, transparent); border-radius: 2px; }

  @media (max-width: 760px) {
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
