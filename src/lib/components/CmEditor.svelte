<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { applyMonacoTheme, configureMonaco, currentAppTheme, monaco } from "../utils/monaco";
  import { installMonacoFindTooltip } from "../utils/monacoFindTooltip";
  import { installNotepadPlusPlusKeybindings } from "../utils/editorKeybindings";
  import ContextMenu from "./ContextMenu.svelte";
  import type { ContextItem } from "./ContextMenu.svelte";

  type SearchFlags = { caseSensitive: boolean; wholeWord: boolean; regex: boolean };
  export interface EditorScrollState {
    topLine: number;
    lineOffset: number;
    horizontalRatio: number;
  }
  type MonacoFindState = {
    readonly isRevealed: boolean;
    onFindReplaceStateChange: (listener: (event: { isRevealed?: boolean }) => void) => monaco.IDisposable;
  };
  type MonacoFindController = {
    getState: () => MonacoFindState;
    closeFindWidget: () => void;
  };

  let {
    value = "",
    fontSize = 11,
    readonly = false,
    wordWrap = false,
    showWhitespace = false,
    textColor = "var(--fg)",
    showLineNumbers = false,
    language = "plaintext",
    onChange,
    onKeyDown,
    onContextMenu,
    onCursorChange,
    onWheel,
    onScrollChange,
    onFindVisibilityChange,
  }: {
    value?: string;
    fontSize?: number;
    readonly?: boolean;
    wordWrap?: boolean;
    showWhitespace?: boolean;
    textColor?: string;
    showLineNumbers?: boolean;
    language?: string;
    onChange?: (text: string) => void;
    onKeyDown?: (e: KeyboardEvent) => void;
    onContextMenu?: (e: MouseEvent) => void;
    onCursorChange?: (line: number, col: number, selLen: number, chars: number) => void;
    onWheel?: (e: WheelEvent) => void;
    onScrollChange?: (state: EditorScrollState) => void;
    onFindVisibilityChange?: (visible: boolean) => void;
  } = $props();

  let container: HTMLDivElement;
  let editor: monaco.editor.IStandaloneCodeEditor | undefined;
  let model: monaco.editor.ITextModel | undefined;
  let searchDecorations: monaco.editor.IEditorDecorationsCollection | undefined;
  let applyingExternalValue = false;
  let mounted = $state(false);
  let themeObserver: MutationObserver | undefined;
  let disposeFindTooltip: (() => void) | undefined;
  let activeSearch: { query: string; flags: SearchFlags } | null = null;
  let applyingSyncedScroll = false;
  let contextMenu = $state<{ items: ContextItem[]; x: number; y: number } | null>(null);
  const disposables: monaco.IDisposable[] = [];
  const WORD_SEPARATORS = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

  function matches(query: string, flags: SearchFlags) {
    if (!model || !query) return [] as monaco.editor.FindMatch[];
    try {
      return model.findMatches(
        query, false, flags.regex, flags.caseSensitive,
        flags.wholeWord ? WORD_SEPARATORS : null, false, 10000,
      );
    } catch {
      return [];
    }
  }

  export function countMatches(query: string, flags: SearchFlags) {
    return matches(query, flags).length;
  }

  export function highlightMatches(query: string, flags: SearchFlags) {
    activeSearch = query ? { query, flags } : null;
    const found = matches(query, flags);
    searchDecorations?.set(found.map(({ range }) => ({
      range,
      options: { inlineClassName: "sbt-editor-find-match" },
    })));
    return found.length;
  }

  export function selectMatch(query: string, flags: SearchFlags, index: number) {
    const match = matches(query, flags)[index];
    if (!editor || !match) return false;
    editor.setSelection(match.range);
    editor.revealRangeInCenter(match.range);
    editor.focus();
    return true;
  }

  export function focus() { editor?.focus(); }

  export function selectionText() {
    const selection = editor?.getSelection();
    return selection && model ? model.getValueInRange(selection) : "";
  }

  export function selectAll() {
    if (!editor || !model) return;
    editor.setSelection(model.getFullModelRange());
    editor.focus();
  }

  export async function copySelection() {
    const text = selectionText();
    if (!text) return false;
    try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
  }

  export async function cutSelection() {
    if (readonly || !editor || !model) return false;
    const selection = editor.getSelection();
    const text = selection ? model.getValueInRange(selection) : "";
    if (!selection || !text) return false;
    try { await navigator.clipboard.writeText(text); } catch { return false; }
    editor.executeEdits("sbt-cut", [{ range: selection, text: "", forceMoveMarkers: true }]);
    editor.focus();
    return true;
  }

  export async function pasteText() {
    if (readonly || !editor) return false;
    try {
      const text = await navigator.clipboard.readText();
      const selection = editor.getSelection();
      if (!text || !selection) return false;
      editor.executeEdits("sbt-paste", [{ range: selection, text, forceMoveMarkers: true }]);
      editor.focus();
      return true;
    } catch { return false; }
  }

  export function showFind() { void editor?.getAction("actions.find")?.run(); }
  export function hideFind() { void editor?.getAction("closeFindWidget")?.run(); }

  export function syncScroll(state: EditorScrollState) {
    if (!editor) return;
    const maxHorizontal = Math.max(0, editor.getScrollWidth() - editor.getLayoutInfo().contentWidth);
    const scrollTop = editor.getTopForLineNumber(state.topLine) + state.lineOffset;
    const scrollLeft = maxHorizontal * state.horizontalRatio;
    applyingSyncedScroll = true;
    editor.setScrollPosition({ scrollTop, scrollLeft });
    requestAnimationFrame(() => { applyingSyncedScroll = false; });
  }

  function findController() {
    return editor?.getContribution("editor.contrib.findController") as unknown as MonacoFindController | null | undefined;
  }

  export async function toggleFind() {
    if (!editor) return false;
    const controller = findController();
    if (controller?.getState().isRevealed) {
      controller.closeFindWidget();
      editor.focus();
      return false;
    }
    editor.focus();
    await editor.getAction("actions.find")?.run();
    return findController()?.getState().isRevealed ?? false;
  }

  function showSimpleContextMenu(event: MouseEvent) {
    const hasSelection = Boolean(selectionText());
    contextMenu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: "Cut", action: () => void cutSelection(), disabled: readonly || !hasSelection },
        { label: "Copy", action: () => void copySelection(), disabled: !hasSelection },
        { label: "Paste", action: () => void pasteText(), disabled: readonly },
        { label: "Select All", action: selectAll, disabled: !model?.getValueLength() },
      ],
    };
  }

  function emitCursor() {
    if (!editor || !model) return;
    const position = editor.getPosition();
    const selection = editor.getSelection();
    if (!position) return;
    const selectionLength = selection ? model.getValueInRange(selection).length : 0;
    onCursorChange?.(position.lineNumber, position.column, selectionLength, model.getValueLength());
  }

  function handleWheelEvent(event: WheelEvent) {
    if (!event.ctrlKey) return;
    event.preventDefault();
    event.stopPropagation();
    onWheel?.(event);
  }

  function handleFindShortcut(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.key.toLowerCase() !== "f") return;
    event.preventDefault();
    event.stopPropagation();
    void editor?.getAction("actions.find")?.run();
  }

  onMount(() => {
    configureMonaco();
    applyMonacoTheme(currentAppTheme());
    model = monaco.editor.createModel(value, language);
    editor = monaco.editor.create(container, {
      model,
      automaticLayout: true,
      readOnly: readonly,
      domReadOnly: readonly,
      wordWrap: wordWrap ? "on" : "off",
      renderWhitespace: showWhitespace ? "all" : "none",
      lineNumbers: showLineNumbers ? "on" : "off",
      lineNumbersMinChars: 3,
      glyphMargin: false,
      folding: false,
      minimap: { enabled: false },
      overviewRulerLanes: 0,
      hideCursorInOverviewRuler: true,
      scrollBeyondLastLine: false,
      renderLineHighlight: showLineNumbers ? "all" : "none",
      renderLineHighlightOnlyWhenFocus: true,
      fontFamily: "Consolas, 'JetBrains Mono', monospace",
      fontWeight: "400",
      fontSize,
      lineHeight: 0,
      padding: { top: 6, bottom: 6 },
      contextmenu: false,
      fixedOverflowWidgets: true,
      mouseWheelZoom: false,
      stickyScroll: { enabled: false },
      bracketPairColorization: { enabled: false },
      guides: { indentation: false, bracketPairs: false },
      unicodeHighlight: { ambiguousCharacters: false },
    });
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyF, () => {
      void editor?.getAction("actions.find")?.run();
    });
    installNotepadPlusPlusKeybindings(editor);
    const findState = findController()?.getState();
    if (findState) {
      disposables.push(findState.onFindReplaceStateChange((event) => {
        if (typeof event.isRevealed === "boolean") onFindVisibilityChange?.(findState.isRevealed);
      }));
    }
    searchDecorations = editor.createDecorationsCollection();
    editor.getDomNode()?.querySelector("textarea")?.setAttribute("spellcheck", "true");
    disposables.push(
      model.onDidChangeContent(() => {
        if (!applyingExternalValue) onChange?.(model?.getValue() ?? "");
        if (activeSearch) queueMicrotask(() => highlightMatches(activeSearch!.query, activeSearch!.flags));
      }),
      editor.onDidChangeCursorPosition(emitCursor),
      editor.onDidChangeCursorSelection(emitCursor),
      editor.onDidFocusEditorText(emitCursor),
      editor.onDidScrollChange((event) => {
        if (applyingSyncedScroll || (!event.scrollTopChanged && !event.scrollLeftChanged)) return;
        const topLine = editor?.getVisibleRanges()[0]?.startLineNumber ?? 1;
        const scrollTop = editor?.getScrollTop() ?? 0;
        const lineTop = editor?.getTopForLineNumber(topLine) ?? 0;
        const maxHorizontal = Math.max(0, (editor?.getScrollWidth() ?? 0) - (editor?.getLayoutInfo().contentWidth ?? 0));
        onScrollChange?.({
          topLine,
          lineOffset: Math.max(0, scrollTop - lineTop),
          horizontalRatio: maxHorizontal > 0 ? (editor?.getScrollLeft() ?? 0) / maxHorizontal : 0,
        });
      }),
      editor.onKeyDown((event) => onKeyDown?.(event.browserEvent)),
    );
    container.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      if (onContextMenu) onContextMenu(event);
      else showSimpleContextMenu(event);
    });
    container.addEventListener("wheel", handleWheelEvent, { capture: true, passive: false });
    container.addEventListener("keydown", handleFindShortcut, { capture: true });
    disposeFindTooltip = installMonacoFindTooltip(container);
    themeObserver = new MutationObserver(() => applyMonacoTheme(currentAppTheme()));
    themeObserver.observe(document.querySelector(".app-root") ?? document.documentElement, {
      attributes: true, attributeFilter: ["data-theme"],
    });
    mounted = true;
    onFindVisibilityChange?.(false);
    emitCursor();
  });

  $effect(() => {
    if (!mounted || !model) return;
    if (model.getValue() !== value) {
      applyingExternalValue = true;
      model.setValue(value);
      applyingExternalValue = false;
    }
  });

  $effect(() => {
    if (!mounted || !editor) return;
    editor.updateOptions({
      readOnly: readonly,
      domReadOnly: readonly,
      wordWrap: wordWrap ? "on" : "off",
      renderWhitespace: showWhitespace ? "all" : "none",
      lineNumbers: showLineNumbers ? "on" : "off",
      renderLineHighlight: showLineNumbers ? "all" : "none",
      fontSize,
    });
  });

  $effect(() => {
    if (!mounted || !model) return;
    if (model.getLanguageId() !== language) monaco.editor.setModelLanguage(model, language);
  });

  onDestroy(() => {
    themeObserver?.disconnect();
    disposeFindTooltip?.();
    container?.removeEventListener("wheel", handleWheelEvent, { capture: true });
    container?.removeEventListener("keydown", handleFindShortcut, { capture: true });
    for (const disposable of disposables) disposable.dispose();
    searchDecorations?.clear();
    editor?.dispose();
    model?.dispose();
  });
</script>

<div bind:this={container} class="monaco-editor-root" style:--editor-text-color={textColor}></div>

{#if contextMenu}
  <ContextMenu items={contextMenu.items} x={contextMenu.x} y={contextMenu.y} onClose={() => contextMenu = null} />
{/if}

<style>
  .monaco-editor-root { flex: 1; min-width: 0; min-height: 0; overflow: hidden; }
  .monaco-editor-root :global(.view-lines) { color: var(--editor-text-color); }
  .monaco-editor-root :global(.sbt-editor-find-match) {
    background: color-mix(in srgb, var(--warning) 42%, transparent);
    border-radius: 2px;
  }
  .monaco-editor-root :global(.find-widget) { z-index: 40 !important; }
  .monaco-editor-root :global(.monaco-hover) { z-index: 60 !important; }
</style>
