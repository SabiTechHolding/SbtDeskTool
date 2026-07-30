import { monaco } from "./monaco";

export function installNotepadPlusPlusKeybindings(editor: monaco.editor.ICodeEditor) {
  const { CtrlCmd, Shift, Alt } = monaco.KeyMod;

  const bind = (keybinding: number, actionId?: string) => {
    editor.addCommand(keybinding, () => {
      if (actionId) void editor.getAction(actionId)?.run();
    });
  };

  const bindTrigger = (keybinding: number, handlerId: string) => {
    editor.addCommand(keybinding, () => {
      editor.trigger("keyboard", handlerId, null);
    });
  };

  // Command Palette is intentionally unavailable in this desktop tool.
  bind(monaco.KeyCode.F1);
  bind(CtrlCmd | Shift | monaco.KeyCode.KeyP);

  // Full screen toggle (F11)
  editor.addCommand(monaco.KeyCode.F11, async () => {
    try {
      const win = (await import("@tauri-apps/api/window")).getCurrentWindow();
      const isFull = await win.isFullscreen();
      await win.setFullscreen(!isFull);
    } catch {}
  });

  // Folding controls
  bind(Alt | monaco.KeyCode.Digit0, "editor.foldAll");
  bind(Alt | monaco.KeyCode.Digit1, "editor.foldLevel1");
  bind(Alt | monaco.KeyCode.Digit2, "editor.foldLevel2");
  bind(Alt | monaco.KeyCode.Digit3, "editor.foldLevel3");
  bind(Alt | monaco.KeyCode.Digit4, "editor.foldLevel4");
  bind(Alt | monaco.KeyCode.Digit5, "editor.foldLevel5");
  bind(Alt | monaco.KeyCode.Digit6, "editor.foldLevel6");
  bind(Alt | monaco.KeyCode.Digit7, "editor.foldLevel7");
  bind(Alt | monaco.KeyCode.Digit8, "editor.foldLevel8");
  bind(Shift | Alt | monaco.KeyCode.Digit0, "editor.unfoldAll");

  // Find & Replace
  bind(CtrlCmd | monaco.KeyCode.KeyH, "editor.action.startFindReplaceAction");
  bind(monaco.KeyCode.F4, "editor.action.nextMatchFindAction");
  bind(Shift | monaco.KeyCode.F4, "editor.action.previousMatchFindAction");
  bind(monaco.KeyCode.F3, "editor.action.nextMatchFindAction");
  bind(Shift | monaco.KeyCode.F3, "editor.action.previousMatchFindAction");
  bind(CtrlCmd | monaco.KeyCode.F3, "editor.action.nextSelectionMatchFindAction");
  bind(CtrlCmd | Shift | monaco.KeyCode.F3, "editor.action.previousSelectionMatchFindAction");

  // Bracket navigation
  bind(CtrlCmd | monaco.KeyCode.KeyB, "editor.action.jumpToBracket");

  // Cut / Copy / Paste line & selection
  bind(Shift | monaco.KeyCode.Delete, "editor.action.clipboardCutAction");
  bind(CtrlCmd | monaco.KeyCode.Insert, "editor.action.clipboardCopyAction");
  bind(Shift | monaco.KeyCode.Insert, "editor.action.clipboardPasteAction");

  // Undo / Redo
  bindTrigger(Alt | monaco.KeyCode.Backspace, "undo");
  bindTrigger(CtrlCmd | monaco.KeyCode.KeyY, "redo");

  // Duplicate & Join
  bind(CtrlCmd | monaco.KeyCode.KeyD, "editor.action.duplicateSelection");
  bind(CtrlCmd | monaco.KeyCode.KeyJ, "editor.action.joinLines");

  // Line & Block comments
  bind(CtrlCmd | monaco.KeyCode.KeyQ, "editor.action.commentLine");
  bind(CtrlCmd | Shift | monaco.KeyCode.KeyQ, "editor.action.removeCommentLine");
  bind(CtrlCmd | monaco.KeyCode.KeyK, "editor.action.commentLine");
  bind(CtrlCmd | Shift | monaco.KeyCode.KeyK, "editor.action.blockComment");

  // Deletions
  bind(CtrlCmd | Shift | monaco.KeyCode.KeyL, "editor.action.deleteLines");
  bind(CtrlCmd | monaco.KeyCode.KeyL, "editor.action.deleteLines");
  bindTrigger(CtrlCmd | Shift | monaco.KeyCode.Backspace, "deleteAllLeft");
  bindTrigger(CtrlCmd | Shift | monaco.KeyCode.Delete, "deleteAllRight");
  bindTrigger(Shift | CtrlCmd | monaco.KeyCode.Delete, "deleteAllRight");

  // Case transforms
  bind(CtrlCmd | monaco.KeyCode.KeyU, "editor.action.transformToLowercase");
  bind(CtrlCmd | Shift | monaco.KeyCode.KeyU, "editor.action.transformToUppercase");

  // Move lines
  bind(CtrlCmd | Shift | monaco.KeyCode.UpArrow, "editor.action.moveLinesUpAction");
  bind(CtrlCmd | Shift | monaco.KeyCode.DownArrow, "editor.action.moveLinesDownAction");
  bind(CtrlCmd | monaco.KeyCode.KeyT, "editor.action.moveLinesUpAction");

  // Column selection modes
  bind(Shift | Alt | monaco.KeyCode.DownArrow, "cursorColumnSelectDown");
  bind(Shift | Alt | monaco.KeyCode.LeftArrow, "cursorColumnSelectLeft");
  bind(Shift | Alt | monaco.KeyCode.PageDown, "cursorColumnSelectPageDown");
  bind(Shift | Alt | monaco.KeyCode.PageUp, "cursorColumnSelectPageUp");
  bind(Shift | Alt | monaco.KeyCode.RightArrow, "cursorColumnSelectRight");
  bind(Shift | Alt | monaco.KeyCode.UpArrow, "cursorColumnSelectUp");

  // Toggle Column Selection Mode (Alt+C)
  editor.addCommand(Alt | monaco.KeyCode.KeyC, () => {
    const current = editor.getOption(monaco.editor.EditorOption.columnSelection);
    editor.updateOptions({ columnSelection: !current });
  });

  // Editor / Tab switching
  editor.addCommand(CtrlCmd | monaco.KeyCode.PageUp, () => {
    document.dispatchEvent(new CustomEvent("app:switchTab", { detail: "prev" }));
  });
  editor.addCommand(CtrlCmd | monaco.KeyCode.PageDown, () => {
    document.dispatchEvent(new CustomEvent("app:switchTab", { detail: "next" }));
  });

  // App & File actions
  editor.addCommand(CtrlCmd | Shift | monaco.KeyCode.KeyS, () => {
    document.dispatchEvent(new CustomEvent("app:flush"));
  });
  editor.addCommand(CtrlCmd | Alt | monaco.KeyCode.KeyS, () => {
    document.dispatchEvent(new CustomEvent("app:saveAs"));
  });
  editor.addCommand(Alt | monaco.KeyCode.F4, async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("exit_app");
    } catch {}
  });
  editor.addCommand(CtrlCmd | monaco.KeyCode.KeyW, () => {
    document.dispatchEvent(new CustomEvent("app:closeTab"));
  });

  // Go to line
  bind(CtrlCmd | monaco.KeyCode.KeyG, "editor.action.gotoLine");

  // Bookmarks
  const bookmarks = editor.createDecorationsCollection();
  const bookmarkDecoration = (range: monaco.Range): monaco.editor.IModelDeltaDecoration => ({
    range,
    options: { isWholeLine: true, className: "sbt-bookmarked-line" },
  });
  const bookmarkRanges = () => bookmarks.getRanges().sort((a, b) => a.startLineNumber - b.startLineNumber);

  editor.addCommand(CtrlCmd | monaco.KeyCode.F2, () => {
    const position = editor.getPosition();
    if (!position) return;
    const ranges = bookmarkRanges();
    const existing = ranges.findIndex((range) => range.startLineNumber === position.lineNumber);
    if (existing >= 0) ranges.splice(existing, 1);
    else ranges.push(new monaco.Range(position.lineNumber, 1, position.lineNumber, 1));
    bookmarks.set(ranges.map(bookmarkDecoration));
  });

  const goToBookmark = (forward: boolean) => {
    const ranges = bookmarkRanges();
    const position = editor.getPosition();
    if (!ranges.length || !position) return;
    const target = forward
      ? ranges.find((range) => range.startLineNumber > position.lineNumber) ?? ranges[0]
      : ranges.findLast((range) => range.startLineNumber < position.lineNumber) ?? ranges.at(-1);
    if (!target) return;
    editor.setPosition({ lineNumber: target.startLineNumber, column: 1 });
    editor.revealLineInCenter(target.startLineNumber);
    editor.focus();
  };
  editor.addCommand(monaco.KeyCode.F2, () => goToBookmark(true));
  editor.addCommand(Shift | monaco.KeyCode.F2, () => goToBookmark(false));
  editor.addCommand(CtrlCmd | Shift | monaco.KeyCode.F2, () => bookmarks.clear());
}
