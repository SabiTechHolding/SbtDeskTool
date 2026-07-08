import tkinter as tk
import os
import re
from dataclasses import dataclass
from typing import Callable, Optional

from diff_engine import compute_inline_diff, compute_line_diff, get_diff_stats
from widgets import FlatButton, themed_scrollbar


@dataclass
class VisualRow:
    left_idx: Optional[int]
    right_idx: Optional[int]
    left_insert: int
    right_insert: int
    kind: str


class VirtualDiffEditor(tk.Frame):
    """Editable side-by-side diff view backed by raw left/right text."""

    def __init__(
        self,
        parent,
        theme,
        settings,
        save_fn,
        save_debounced,
        left_text="",
        right_text="",
        word_wrap=True,
        font_size=10,
        compact=False,
        on_zoom=None,
        on_status: Optional[Callable[[str, str], None]] = None,
        on_changed: Optional[Callable[[], None]] = None,
        **kwargs,
    ):
        super().__init__(parent, bg=theme["bg"], **kwargs)
        self.theme = theme
        self.settings = settings
        self.save_fn = save_fn
        self.save_debounced = save_debounced
        self._word_wrap = word_wrap
        self._font_size = font_size
        self._compact = compact
        self._on_zoom = on_zoom
        self._on_status = on_status
        self._on_changed = on_changed
        self.left_lines = self._to_lines(left_text)
        self.right_lines = self._to_lines(right_text)
        self.rows: list[VisualRow] = []
        self._rendering = False
        self._syncing_y = False
        self._hover_row: Optional[int] = None
        self._active_side = "left"
        self._active_row: Optional[int] = None
        self._undo: list[tuple[list[str], list[str]]] = []
        self._redo: list[tuple[list[str], list[str]]] = []
        self._find_matches: list[tuple[str, int, int, int]] = []
        self._find_index = -1
        self._find_timer = None

        self.auto_var = tk.BooleanVar(value=settings.get("diff_auto", True))
        self.word_diff_var = tk.BooleanVar(value=settings.get("diff_word_diff", True))
        self.ignore_ws_var = tk.BooleanVar(
            value=settings.get("diff_ignore_whitespace", False))
        self.find_var = tk.StringVar()
        self.find_case_var = tk.BooleanVar(value=settings.get("diff_find_case", False))
        self.find_word_var = tk.BooleanVar(value=settings.get("diff_find_word", False))
        self.find_regex_var = tk.BooleanVar(value=settings.get("diff_find_regex", False))

        self._build_ui()
        self.render()

    def _to_lines(self, text):
        return text.splitlines() or [""]

    def _from_lines(self, lines):
        if len(lines) == 1 and lines[0] == "":
            return ""
        return "\n".join(lines)

    def get_text(self, side):
        return self._from_lines(self.left_lines if side == "left" else self.right_lines)

    def set_text(self, side, text):
        if side == "left":
            self.left_lines = self._to_lines(text)
        else:
            self.right_lines = self._to_lines(text)
        self.render()

    def set_font_size(self, font_size):
        self._font_size = font_size
        font = ("Consolas", font_size)
        for widget in (self.left_text, self.right_text,
                       self.left_gutter, self.right_gutter):
            widget.config(font=font)
        for name in ("detail_left", "detail_right"):
            if hasattr(self, name):
                getattr(self, name).config(font=font)
        self.render()

    def set_word_wrap(self, word_wrap):
        self._word_wrap = word_wrap
        wrap = "word" if word_wrap else "none"
        self.left_text.config(wrap=wrap)
        self.right_text.config(wrap=wrap)

    def _build_ui(self):
        t = self.theme
        self._build_search_bar()
        self.pane = tk.PanedWindow(self, orient=tk.HORIZONTAL,
            bg=t["border"], sashwidth=5, sashrelief="flat", bd=0, handlesize=0)
        self.pane.pack(fill="both", expand=True)

        for side, label in (("left", "◀  Left"), ("right", "▶  Right")):
            frame = tk.Frame(self.pane, bg=t["bg"])
            self.pane.add(frame, stretch="always", minsize=100)
            header = tk.Frame(frame, bg=t["bg3"], height=28)
            header.pack(fill="x")
            header.pack_propagate(False)
            tk.Label(header, text=f"  {label}", bg=t["bg3"], fg=t["fg2"],
                font=t["font_ui_bold"], anchor="w").pack(side="left", fill="y")

            FlatButton(header, text="✕ Clear", theme=t,
                command=lambda s=side: self.clear_side(s)).pack(side="right", padx=4)
            FlatButton(header, text="⎘ Copy", theme=t,
                command=lambda s=side: self.copy_side(s)).pack(side="right", padx=2)

            if side == "right" and not self._compact:
                tk.Checkbutton(header, text="Word Diff", variable=self.word_diff_var,
                    command=self._toggle_word_diff,
                    bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                    activebackground=t["bg3"], activeforeground=t["fg"],
                    font=t["font_small"]).pack(side="right", padx=2)
                tk.Checkbutton(header, text="Ignore WS", variable=self.ignore_ws_var,
                    command=self._toggle_ignore_ws,
                    bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                    activebackground=t["bg3"], activeforeground=t["fg"],
                    font=t["font_small"]).pack(side="right", padx=2)
                tk.Checkbutton(header, text="Auto", variable=self.auto_var,
                    command=self._toggle_auto,
                    bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                    activebackground=t["bg3"], activeforeground=t["fg"],
                    font=t["font_small"]).pack(side="right", padx=2)
                FlatButton(header, text="▶ Run Diff", theme=t,
                    command=self.render).pack(side="right", padx=4)

            body = tk.Frame(frame, bg=t["bg"])
            body.pack(fill="both", expand=True)
            gutter = tk.Text(body, bg=t["diff_line_num_bg"], fg=t["diff_line_num_fg"],
                font=("Consolas", self._font_size), wrap="none",
                relief="flat", bd=0, padx=4, pady=6, width=5,
                state="disabled", cursor="arrow", takefocus=False)
            text = tk.Text(body, bg=t["bg"], fg=t["fg"],
                font=("Consolas", self._font_size),
                wrap="word" if self._word_wrap else "none",
                relief="flat", bd=0, padx=8, pady=6,
                insertbackground=t["fg"], selectbackground=t["selection"],
                undo=False)
            scroll_y = themed_scrollbar(body, t,
                command=lambda *args, s=side: self._sync_y_from_scroll(s, *args))
            text.config(yscrollcommand=lambda *args, s=side: self._on_yscroll(s, *args))
            scroll_y.pack(side="right", fill="y")
            if side == "right":
                self.minimap = tk.Canvas(body, width=12, bg=t["bg2"],
                    highlightthickness=0, bd=0, cursor="hand2")
                self.minimap.pack(side="right", fill="y")
                self.minimap.bind("<Configure>", lambda e: self._draw_minimap(), add="+")
                self.minimap.bind("<Button-1>", self._on_minimap_click, add="+")
                self.minimap.bind("<B1-Motion>", self._on_minimap_click, add="+")
            gutter.pack(side="left", fill="y")
            text.pack(side="left", fill="both", expand=True)

            if not self._word_wrap:
                scroll_x = themed_scrollbar(frame, t, orient="horizontal",
                    command=lambda *args, s=side: self._sync_x(s, *args))
                text.config(xscrollcommand=scroll_x.set)
                scroll_x.pack(side="bottom", fill="x", before=body)
                setattr(self, f"{side}_scroll_x", scroll_x)

            text.bind("<KeyPress>", lambda e, s=side: self._on_key(e, s), add="+")
            text.bind("<Control-v>", lambda e, s=side: self._paste(s), add="+")
            text.bind("<Control-V>", lambda e, s=side: self._paste(s), add="+")
            text.bind("<Control-c>", lambda e, s=side: self._copy_selection(s), add="+")
            text.bind("<Control-C>", lambda e, s=side: self._copy_selection(s), add="+")
            text.bind("<Control-x>", lambda e, s=side: self._cut_selection(s), add="+")
            text.bind("<Control-X>", lambda e, s=side: self._cut_selection(s), add="+")
            text.bind("<Control-z>", lambda e: self.undo(), add="+")
            text.bind("<Control-Z>", lambda e: self.undo(), add="+")
            text.bind("<Control-y>", lambda e: self.redo(), add="+")
            text.bind("<Control-Y>", lambda e: self.redo(), add="+")
            text.bind("<Control-a>", lambda e, w=text: self._select_all(w), add="+")
            text.bind("<Control-A>", lambda e, w=text: self._select_all(w), add="+")
            text.bind("<Control-f>", lambda e: self._focus_find(), add="+")
            text.bind("<Control-F>", lambda e: self._focus_find(), add="+")
            text.bind("<Shift-MouseWheel>", self._on_shift_mousewheel, add="+")
            text.bind("<Motion>", lambda e, s=side: self._on_pointer_motion(e, s), add="+")
            text.bind("<Leave>", lambda e: self._clear_hover_line(), add="+")
            text.bind("<ButtonRelease-1>", lambda e, s=side: self._update_active_line(s), add="+")
            text.bind("<KeyRelease>", lambda e, s=side: self._update_active_line(s), add="+")
            text.bind("<FocusIn>", lambda e, s=side: self._update_active_line(s), add="+")
            if self._on_zoom:
                text.bind("<Control-MouseWheel>", self._on_zoom, add="+")

            setattr(self, f"{side}_frame", frame)
            setattr(self, f"{side}_gutter", gutter)
            setattr(self, f"{side}_text", text)
            setattr(self, f"{side}_scroll_y", scroll_y)

        self.pane.bind("<ButtonRelease-1>", self._save_sash, add="+")
        self._build_detail_panel()
        self.after_idle(self._restore_sash)
        self.after(80, self._restore_sash)

    def _build_search_bar(self):
        t = self.theme
        bar = tk.Frame(self, bg=t["bg3"], height=30)
        bar.pack(fill="x")
        bar.pack_propagate(False)
        entry = tk.Entry(bar, textvariable=self.find_var, bg=t["bg"], fg=t["fg"],
            insertbackground=t["fg"], relief="flat", bd=1,
            highlightthickness=1, highlightbackground=t["border"],
            highlightcolor=t["accent"], font=t["font_ui"])
        entry.pack(side="left", fill="x", expand=True, padx=(6, 4), pady=4)
        entry.insert(0, "")
        entry.bind("<KeyRelease>", self._schedule_find, add="+")
        entry.bind("<Return>", lambda e: self._find_next(), add="+")
        entry.bind("<Shift-Return>", lambda e: self._find_prev(), add="+")
        entry.bind("<Escape>", lambda e: self._leave_find(), add="+")

        self.find_status = tk.Label(bar, text="", bg=t["bg3"], fg=t["fg2"],
            font=t["font_small"], width=12, anchor="w")
        self.find_status.pack(side="left", padx=2)
        for text, var in (("Aa", self.find_case_var),
                          ("ab", self.find_word_var),
                          (".*", self.find_regex_var)):
            tk.Checkbutton(bar, text=text, variable=var, command=self._toggle_find_option,
                bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                activebackground=t["bg3"], activeforeground=t["fg"],
                font=t["font_small"], padx=2).pack(side="left")
        FlatButton(bar, text="Prev", theme=t, command=self._find_prev).pack(side="left", padx=2)
        FlatButton(bar, text="Next", theme=t, command=self._find_next).pack(side="left", padx=2)
        FlatButton(bar, text="X", theme=t, command=self._clear_find).pack(side="left", padx=(2, 6))
        self.find_entry = entry

    def _build_detail_panel(self):
        t = self.theme
        panel = tk.Frame(self, bg=t["bg2"], height=58)
        panel.pack(fill="x")
        panel.pack_propagate(False)
        self.detail_panel = panel
        body = tk.Frame(panel, bg=t["bg2"])
        body.pack(fill="both", expand=True)
        for side, label in (("left", "Left"), ("right", "Right")):
            row = tk.Frame(body, bg=t["bg2"], height=20)
            row.pack(fill="x", expand=True)
            tk.Label(row, text=label, bg=t["bg2"], fg=t["fg2"],
                font=t["font_small"], width=6, anchor="e").pack(side="left", padx=(4, 4))
            text = tk.Text(row, bg=t["bg"], fg=t["fg"], font=("Consolas", self._font_size),
                wrap="none", relief="flat", bd=0, padx=6, pady=0, height=1,
                insertbackground=t["fg"], selectbackground=t["selection"],
                state="disabled")
            text.pack(side="left", fill="x", expand=True)
            text.bind("<Shift-MouseWheel>", self._on_detail_shift_mousewheel, add="+")
            setattr(self, f"detail_{side}", text)
        scroll_x = themed_scrollbar(panel, t, orient="horizontal", command=self._sync_detail_x)
        scroll_x.pack(fill="x")
        self.detail_scroll_x = scroll_x

    def _toggle_auto(self):
        self.settings["diff_auto"] = self.auto_var.get()
        self.save_fn(self.settings)
        if self.auto_var.get():
            self.render()

    def _toggle_ignore_ws(self):
        self.settings["diff_ignore_whitespace"] = self.ignore_ws_var.get()
        self.save_fn(self.settings)
        self.render()

    def _toggle_word_diff(self):
        self.settings["diff_word_diff"] = self.word_diff_var.get()
        self.save_fn(self.settings)
        self.render()

    def _toggle_find_option(self):
        self.settings["diff_find_case"] = self.find_case_var.get()
        self.settings["diff_find_word"] = self.find_word_var.get()
        self.settings["diff_find_regex"] = self.find_regex_var.get()
        self.save_fn(self.settings)
        self._run_find()

    def _focus_find(self):
        self.find_entry.focus_set()
        self.find_entry.selection_range(0, "end")
        self._run_find()
        return "break"

    def _leave_find(self):
        widget = getattr(self, f"{self._active_side}_text", None)
        if widget:
            widget.focus_set()
        return "break"

    def _clear_find(self):
        self.find_var.set("")
        self._run_find()
        self._leave_find()
        return "break"

    def _schedule_find(self, _=None):
        if self._find_timer:
            self.after_cancel(self._find_timer)
        self._find_timer = self.after(120, self._run_find)

    def _compiled_find_pattern(self):
        query = self.find_var.get()
        if not query:
            return None, None
        flags = 0 if self.find_case_var.get() else re.IGNORECASE
        try:
            pattern = query if self.find_regex_var.get() else re.escape(query)
            if self.find_word_var.get():
                pattern = rf"(?<!\w)(?:{pattern})(?!\w)"
            return re.compile(pattern, flags), None
        except re.error as exc:
            return None, str(exc)

    def _run_find(self, keep_index=False):
        if self._find_timer:
            self.after_cancel(self._find_timer)
            self._find_timer = None
        for widget in (getattr(self, "left_text", None), getattr(self, "right_text", None)):
            if widget:
                widget.tag_remove("diff_search", "1.0", "end")
                widget.tag_remove("diff_search_current", "1.0", "end")
        old_current = None
        if keep_index and 0 <= self._find_index < len(self._find_matches):
            old_current = self._find_matches[self._find_index]
        self._find_matches = []
        self._find_index = -1

        pattern, error = self._compiled_find_pattern()
        if error:
            self.find_status.config(text="Bad regex")
            return
        if pattern is None:
            self.find_status.config(text="")
            return

        for side in ("left", "right"):
            for row_idx, row in enumerate(self.rows):
                text = self._visual_line_text(side, row)
                if not text:
                    continue
                for match in pattern.finditer(text):
                    if match.end() == match.start():
                        continue
                    self._find_matches.append((side, row_idx, match.start(), match.end()))

        for side, row_idx, start, end in self._find_matches:
            widget = getattr(self, f"{side}_text")
            line = row_idx + 1
            widget.tag_add("diff_search", f"{line}.{start}", f"{line}.{end}")

        if self._find_matches:
            if old_current in self._find_matches:
                self._find_index = self._find_matches.index(old_current)
            else:
                self._find_index = 0
            self._mark_current_find()
        else:
            self.find_status.config(text="No results")

    def _mark_current_find(self):
        for widget in (self.left_text, self.right_text):
            widget.tag_remove("diff_search_current", "1.0", "end")
        if not self._find_matches:
            self.find_status.config(text="No results" if self.find_var.get() else "")
            return
        side, row_idx, start, end = self._find_matches[self._find_index]
        widget = getattr(self, f"{side}_text")
        line = row_idx + 1
        widget.tag_add("diff_search_current", f"{line}.{start}", f"{line}.{end}")
        for w in (self.left_text, self.right_text):
            w.tag_raise("diff_search")
            w.tag_raise("diff_search_current")
            w.tag_raise("sel")
        self.find_status.config(text=f"{self._find_index + 1}/{len(self._find_matches)}")

    def _goto_find_match(self):
        if not self._find_matches:
            return "break"
        side, row_idx, start, _ = self._find_matches[self._find_index]
        widget = getattr(self, f"{side}_text")
        widget.focus_set()
        widget.mark_set("insert", f"{row_idx + 1}.{start}")
        widget.see(f"{row_idx + 1}.{start}")
        self._active_side = side
        self._active_row = row_idx
        self._apply_focus_tags()
        self._mark_current_find()
        return "break"

    def _find_next(self):
        if not self._find_matches:
            self._run_find()
        if self._find_matches:
            self._find_index = (self._find_index + 1) % len(self._find_matches)
            return self._goto_find_match()
        return "break"

    def _find_prev(self):
        if not self._find_matches:
            self._run_find()
        if self._find_matches:
            self._find_index = (self._find_index - 1) % len(self._find_matches)
            return self._goto_find_match()
        return "break"

    def _save_sash(self, _=None):
        try:
            x = self.pane.sash_coord(0)[0]
            self.settings["diff_left_width"] = max(100, x)
            self.settings["diff_left_ratio"] = x / max(1, self.pane.winfo_width())
            self.save_debounced()
        except Exception:
            pass

    def _restore_sash(self):
        try:
            ratio = self.settings.get("diff_left_ratio")
            if ratio is None:
                saved = self.settings.get("diff_left_width", 0)
                ratio = saved / max(1, self.pane.winfo_width()) if saved else 0.5
            x = int(max(0.1, min(0.9, float(ratio))) * max(1, self.pane.winfo_width()))
            self.pane.sash_place(0, max(100, x), self.pane.sash_coord(0)[1])
        except Exception:
            pass

    def _snapshot(self):
        return (self.left_lines[:], self.right_lines[:])

    def _push_undo(self):
        self._undo.append(self._snapshot())
        if len(self._undo) > 100:
            self._undo.pop(0)
        self._redo.clear()

    def undo(self):
        if not self._undo:
            return "break"
        self._redo.append(self._snapshot())
        self.left_lines, self.right_lines = self._undo.pop()
        self.render()
        return "break"

    def redo(self):
        if not self._redo:
            return "break"
        self._undo.append(self._snapshot())
        self.left_lines, self.right_lines = self._redo.pop()
        self.render()
        return "break"

    def _select_all(self, widget):
        widget.tag_add("sel", "1.0", "end-1c")
        widget.mark_set("insert", "1.0")
        widget.see("insert")
        return "break"

    def copy_side(self, side):
        try:
            self.clipboard_clear()
            self.clipboard_append(self.get_text(side))
            self._status("Copied", "success")
        except Exception:
            pass

    def clear_side(self, side):
        self._push_undo()
        if side == "left":
            self.left_lines = [""]
        else:
            self.right_lines = [""]
        self.render()
        self._changed()

    def _changed(self):
        if self._on_changed:
            self._on_changed()

    def _status(self, message, kind="normal"):
        if self._on_status:
            self._on_status(message, kind)

    def _build_rows(self):
        chunks = compute_line_diff(
            self.get_text("left"), self.get_text("right"),
            ignore_whitespace=self.ignore_ws_var.get())
        rows: list[VisualRow] = []
        for chunk in chunks:
            if chunk.kind == "equal":
                for i in range(len(chunk.old_lines)):
                    rows.append(VisualRow(chunk.old_start + i, chunk.new_start + i,
                                          chunk.old_start + i, chunk.new_start + i, "equal"))
            elif chunk.kind == "delete":
                for i in range(len(chunk.old_lines)):
                    rows.append(VisualRow(chunk.old_start + i, None,
                                          chunk.old_start + i, chunk.new_start, "delete"))
            elif chunk.kind == "insert":
                for i in range(len(chunk.new_lines)):
                    rows.append(VisualRow(None, chunk.new_start + i,
                                          chunk.old_start, chunk.new_start + i, "insert"))
            elif chunk.kind == "replace":
                max_lines = max(len(chunk.old_lines), len(chunk.new_lines))
                for i in range(max_lines):
                    left_idx = chunk.old_start + i if i < len(chunk.old_lines) else None
                    right_idx = chunk.new_start + i if i < len(chunk.new_lines) else None
                    rows.append(VisualRow(
                        left_idx, right_idx,
                        left_idx if left_idx is not None else chunk.old_start + len(chunk.old_lines),
                        right_idx if right_idx is not None else chunk.new_start + len(chunk.new_lines),
                        "replace",
                    ))
        if not rows:
            rows.append(VisualRow(0, 0, 0, 0, "equal"))
        return rows, chunks

    def render(self):
        left_top = self.left_text.yview()[0] if hasattr(self, "left_text") else 0
        left_x = self.left_text.xview()[0] if hasattr(self, "left_text") else 0
        right_x = self.right_text.xview()[0] if hasattr(self, "right_text") else 0
        cursor_side, cursor_raw, cursor_col = self._cursor_raw_position()

        self.rows, chunks = self._build_rows()
        left_visual, right_visual = [], []
        left_nums, right_nums = [], []
        for row in self.rows:
            left_visual.append(self.left_lines[row.left_idx] if row.left_idx is not None else self._placeholder_text())
            right_visual.append(self.right_lines[row.right_idx] if row.right_idx is not None else self._placeholder_text())
            left_nums.append("" if row.left_idx is None else str(row.left_idx + 1))
            right_nums.append("" if row.right_idx is None else str(row.right_idx + 1))

        self._rendering = True
        self._set_widget_text(self.left_text, "\n".join(left_visual))
        self._set_widget_text(self.right_text, "\n".join(right_visual))
        self._set_gutter_text(self.left_gutter, left_nums)
        self._set_gutter_text(self.right_gutter, right_nums)
        self._configure_tags()
        self._apply_row_tags()
        self._apply_inline_tags()
        self._run_find(keep_index=True)
        self._apply_focus_tags()
        self._rendering = False

        self.left_text.yview_moveto(left_top)
        self.right_text.yview_moveto(left_top)
        self.left_gutter.yview_moveto(left_top)
        self.right_gutter.yview_moveto(left_top)
        self.left_text.xview_moveto(left_x)
        self.right_text.xview_moveto(right_x)
        self._restore_cursor(cursor_side, cursor_raw, cursor_col)
        self._update_active_line(cursor_side)
        self._draw_minimap()

        stats = get_diff_stats(chunks)
        suffix = " (whitespace ignored)" if self.ignore_ws_var.get() else ""
        self._status(
            f"+{stats['added']} added   -{stats['removed']} removed   "
            f"{stats['changed_blocks']} block(s) changed{suffix}")
        return "break"

    def _placeholder_text(self):
        return " "

    def _placeholder_stipple(self):
        path = os.path.join(os.path.dirname(__file__),
            "assets", "diff_placeholder_diag.xbm")
        if os.path.exists(path):
            return "@" + path.replace("\\", "/")
        return "gray25"

    def _placeholder_colors(self):
        if self.theme.get("name") == "dark":
            return "#333333", "#999999"
        return "#e4e8ee", "#333333"

    def _set_widget_text(self, widget, text):
        widget.delete("1.0", "end")
        widget.insert("1.0", text)

    def _set_gutter_text(self, widget, nums):
        width = max(4, max((len(n) for n in nums), default=1) + 1)
        text = "\n".join(f"{n:>{width - 1}}" if n else " " * (width - 1) for n in nums)
        widget.config(state="normal", width=width)
        widget.delete("1.0", "end")
        widget.insert("1.0", text)
        widget.config(state="disabled")

    def _configure_tags(self):
        t = self.theme
        active_bg = "#2f2817" if t.get("name") == "dark" else "#fff3c4"
        hover_bg = "#222222" if t.get("name") == "dark" else "#eef6ff"
        search_bg = "#5c4b1f" if t.get("name") == "dark" else "#ffe58a"
        search_current_bg = "#8a6f22" if t.get("name") == "dark" else "#ffbd2e"
        placeholder_stipple = self._placeholder_stipple()
        placeholder_bg, placeholder_fg = self._placeholder_colors()
        for widget in (self.left_text, self.right_text):
            widget.tag_configure("diff_delete_line",
                background=t["diff_del_bg"], foreground=t["diff_del_fg"])
            widget.tag_configure("diff_insert_line",
                background=t["diff_add_bg"], foreground=t["diff_add_fg"])
            widget.tag_configure("diff_placeholder",
                background=placeholder_bg, foreground=placeholder_fg,
                bgstipple=placeholder_stipple)
            widget.tag_configure("diff_delete_inline",
                background=t["diff_del_inline"], foreground="#ffffff")
            widget.tag_configure("diff_insert_inline",
                background=t["diff_add_inline"], foreground="#ffffff")
            widget.tag_configure("diff_hover_line",
                background=hover_bg)
            widget.tag_configure("diff_active_line",
                background=active_bg)
            widget.tag_configure("diff_search",
                background=search_bg, foreground=t["fg"])
            widget.tag_configure("diff_search_current",
                background=search_current_bg, foreground="#ffffff")
            widget.tag_raise("diff_hover_line")
            widget.tag_raise("diff_active_line")
            widget.tag_raise("diff_delete_inline")
            widget.tag_raise("diff_insert_inline")
            widget.tag_raise("diff_search")
            widget.tag_raise("diff_search_current")
            widget.tag_raise("sel")
        for name in ("detail_left", "detail_right"):
            if hasattr(self, name):
                widget = getattr(self, name)
                widget.tag_configure("diff_delete_line",
                    background=t["diff_del_bg"], foreground=t["diff_del_fg"])
                widget.tag_configure("diff_insert_line",
                    background=t["diff_add_bg"], foreground=t["diff_add_fg"])
                widget.tag_configure("diff_placeholder",
                    background=placeholder_bg, foreground=placeholder_fg,
                    bgstipple=placeholder_stipple)
                widget.tag_configure("diff_delete_inline",
                    background=t["diff_del_inline"], foreground="#ffffff")
                widget.tag_configure("diff_insert_inline",
                    background=t["diff_add_inline"], foreground="#ffffff")

    def _line_range(self, row_idx):
        line = row_idx + 1
        return f"{line}.0", f"{line}.0 lineend +1c"

    def _apply_focus_tags(self):
        for widget in (self.left_text, self.right_text):
            widget.tag_remove("diff_hover_line", "1.0", "end")
            widget.tag_remove("diff_active_line", "1.0", "end")
        if self._hover_row is not None and 0 <= self._hover_row < len(self.rows):
            start, end = self._line_range(self._hover_row)
            self.left_text.tag_add("diff_hover_line", start, end)
            self.right_text.tag_add("diff_hover_line", start, end)
        if self._active_row is not None and 0 <= self._active_row < len(self.rows):
            start, end = self._line_range(self._active_row)
            self.left_text.tag_add("diff_active_line", start, end)
            self.right_text.tag_add("diff_active_line", start, end)
        for widget in (self.left_text, self.right_text):
            widget.tag_raise("diff_hover_line")
            widget.tag_raise("diff_active_line")
            widget.tag_raise("diff_delete_inline")
            widget.tag_raise("diff_insert_inline")
            widget.tag_raise("diff_search")
            widget.tag_raise("diff_search_current")
            widget.tag_raise("sel")
        self._update_detail_panel()

    def _on_pointer_motion(self, event, side):
        widget = getattr(self, f"{side}_text")
        try:
            row = int(widget.index(f"@{event.x},{event.y}").split(".", 1)[0]) - 1
        except Exception:
            return
        if row != self._hover_row:
            self._hover_row = row
            self._apply_focus_tags()

    def _clear_hover_line(self):
        self._hover_row = None
        self._apply_focus_tags()

    def _update_active_line(self, side=None):
        if side:
            self._active_side = side
        widget = getattr(self, f"{self._active_side}_text")
        try:
            self._active_row = int(widget.index("insert").split(".", 1)[0]) - 1
        except Exception:
            self._active_row = None
        self._apply_focus_tags()

    def _apply_row_tags(self):
        for idx, row in enumerate(self.rows, start=1):
            if row.left_idx is None:
                self.left_text.tag_add("diff_placeholder", f"{idx}.0", f"{idx}.0 lineend +1c")
            if row.right_idx is None:
                self.right_text.tag_add("diff_placeholder", f"{idx}.0", f"{idx}.0 lineend +1c")
            if row.kind in ("delete", "replace") and row.left_idx is not None:
                self.left_text.tag_add("diff_delete_line", f"{idx}.0", f"{idx}.0 lineend +1c")
            if row.kind in ("insert", "replace") and row.right_idx is not None:
                self.right_text.tag_add("diff_insert_line", f"{idx}.0", f"{idx}.0 lineend +1c")

    def _apply_inline_tags(self):
        for visual_idx, row in enumerate(self.rows, start=1):
            if row.kind != "replace" or row.left_idx is None or row.right_idx is None:
                continue
            old_toks, new_toks = compute_inline_diff(
                self.left_lines[row.left_idx],
                self.right_lines[row.right_idx],
                word_diff=self.word_diff_var.get())
            self._tag_inline(self.left_text, visual_idx, old_toks, "diff_delete_inline")
            self._tag_inline(self.right_text, visual_idx, new_toks, "diff_insert_inline")

    def _tag_inline(self, widget, lineno, tokens, tag):
        col = 0
        for token in tokens:
            next_col = col + len(token.text)
            if token.kind != "equal" and next_col > col:
                widget.tag_add(tag, f"{lineno}.{col}", f"{lineno}.{next_col}")
            col = next_col

    def _visual_line_text(self, side, row):
        idx = row.left_idx if side == "left" else row.right_idx
        if idx is None:
            return ""
        lines = self.left_lines if side == "left" else self.right_lines
        return lines[idx] if 0 <= idx < len(lines) else ""

    def _visual_line_display_text(self, side, row):
        text = self._visual_line_text(side, row)
        if text:
            return text
        idx = row.left_idx if side == "left" else row.right_idx
        return self._placeholder_text() if idx is None else ""

    def _set_detail_widget_text(self, widget, text):
        widget.config(state="normal")
        widget.delete("1.0", "end")
        widget.insert("1.0", text)
        widget.config(state="disabled")

    def _update_detail_panel(self):
        if not hasattr(self, "detail_left"):
            return
        row_idx = self._active_row if self._active_row is not None else 0
        if row_idx < 0 or row_idx >= len(self.rows):
            row_idx = 0
        row = self.rows[row_idx] if self.rows else VisualRow(None, None, 0, 0, "equal")
        left_text = self._visual_line_display_text("left", row)
        right_text = self._visual_line_display_text("right", row)
        self._set_detail_widget_text(self.detail_left, left_text)
        self._set_detail_widget_text(self.detail_right, right_text)
        for widget in (self.detail_left, self.detail_right):
            widget.config(state="normal")
            for tag in ("diff_delete_line", "diff_insert_line", "diff_placeholder",
                        "diff_delete_inline", "diff_insert_inline"):
                widget.tag_remove(tag, "1.0", "end")
        if row.left_idx is None:
            self.detail_left.tag_add("diff_placeholder", "1.0", "1.0 lineend")
        elif row.kind in ("delete", "replace"):
            self.detail_left.tag_add("diff_delete_line", "1.0", "1.0 lineend")
        if row.right_idx is None:
            self.detail_right.tag_add("diff_placeholder", "1.0", "1.0 lineend")
        elif row.kind in ("insert", "replace"):
            self.detail_right.tag_add("diff_insert_line", "1.0", "1.0 lineend")
        if row.kind == "replace" and row.left_idx is not None and row.right_idx is not None:
            old_toks, new_toks = compute_inline_diff(
                self.left_lines[row.left_idx],
                self.right_lines[row.right_idx],
                word_diff=self.word_diff_var.get())
            self._tag_inline(self.detail_left, 1, old_toks, "diff_delete_inline")
            self._tag_inline(self.detail_right, 1, new_toks, "diff_insert_inline")
        self.detail_left.config(state="disabled")
        self.detail_right.config(state="disabled")
        self._sync_detail_x("moveto", self.detail_left.xview()[0])

    def _sync_detail_x(self, *args):
        for widget in (getattr(self, "detail_left", None), getattr(self, "detail_right", None)):
            if widget:
                widget.xview(*args)
        if hasattr(self, "detail_scroll_x"):
            self.detail_scroll_x.set(*self.detail_left.xview())

    def _on_detail_shift_mousewheel(self, event):
        units = -1 if event.delta > 0 else 1
        self.detail_left.xview_scroll(units, "units")
        self.detail_right.xview_scroll(units, "units")
        if hasattr(self, "detail_scroll_x"):
            self.detail_scroll_x.set(*self.detail_left.xview())
        return "break"

    def _draw_minimap(self):
        canvas = getattr(self, "minimap", None)
        if not canvas:
            return
        canvas.delete("all")
        width = max(1, canvas.winfo_width())
        height = max(1, canvas.winfo_height())
        total = max(1, len(self.rows))
        colors = {
            "delete": self.theme["diff_del_inline"],
            "insert": self.theme["diff_add_inline"],
            "replace": self.theme["accent"],
        }
        for idx, row in enumerate(self.rows):
            color = colors.get(row.kind)
            if not color:
                continue
            y1 = int(idx / total * height)
            y2 = max(y1 + 2, int((idx + 1) / total * height))
            canvas.create_rectangle(1, y1, width - 2, y2, fill=color, outline="")
        try:
            top, bottom = self.left_text.yview()
            canvas.create_rectangle(0, int(top * height), width - 1,
                max(int(bottom * height), int(top * height) + 3),
                outline=self.theme["fg2"])
        except Exception:
            pass

    def _on_minimap_click(self, event):
        canvas = getattr(self, "minimap", None)
        if not canvas:
            return "break"
        height = max(1, canvas.winfo_height())
        fraction = max(0.0, min(1.0, event.y / height))
        self.left_text.yview_moveto(fraction)
        self.right_text.yview_moveto(fraction)
        self.left_gutter.yview_moveto(fraction)
        self.right_gutter.yview_moveto(fraction)
        self._draw_minimap()
        return "break"

    def _cursor_raw_position(self):
        focus = self.focus_get()
        side = "right" if focus is getattr(self, "right_text", None) else "left"
        try:
            return (side, *self._visual_to_raw(side, getattr(self, f"{side}_text").index("insert")))
        except Exception:
            return side, 0, 0

    def _restore_cursor(self, side, raw_idx, col):
        visual = self._raw_to_visual(side, raw_idx)
        widget = getattr(self, f"{side}_text")
        if visual is None:
            visual = max(0, len(self.rows) - 1)
            col = 0
        line = self._line_for(side, raw_idx)
        col = min(col, len(line))
        widget.mark_set("insert", f"{visual + 1}.{col}")
        widget.see("insert")

    def _raw_to_visual(self, side, raw_idx):
        attr = "left_idx" if side == "left" else "right_idx"
        for i, row in enumerate(self.rows):
            if getattr(row, attr) == raw_idx:
                return i
        return None

    def _visual_to_raw(self, side, index):
        visual_line, col_s = index.split(".", 1)
        row_idx = max(0, min(len(self.rows) - 1, int(visual_line) - 1))
        col = int(col_s)
        row = self.rows[row_idx]
        raw_attr = "left_idx" if side == "left" else "right_idx"
        insert_attr = "left_insert" if side == "left" else "right_insert"
        raw_idx = getattr(row, raw_attr)
        if raw_idx is None:
            raw_idx = max(0, min(len(self._lines_for(side)), getattr(row, insert_attr)))
        line = self._line_for(side, raw_idx)
        return raw_idx, min(col, len(line))

    def _selected_visual_range(self, side):
        widget = getattr(self, f"{side}_text")
        try:
            start = widget.index("sel.first")
            end = widget.index("sel.last")
        except tk.TclError:
            return None
        return start, end

    def _row_raw_idx(self, side, visual_row):
        if visual_row < 0 or visual_row >= len(self.rows):
            return None
        row = self.rows[visual_row]
        return row.left_idx if side == "left" else row.right_idx

    def _delete_selection(self, side):
        selected = self._selected_visual_range(side)
        if not selected:
            return None

        start, end = selected
        start_line, start_col_s = start.split(".", 1)
        end_line, end_col_s = end.split(".", 1)
        start_row = max(0, int(start_line) - 1)
        end_row = max(0, int(end_line) - 1)
        start_col = int(start_col_s)
        end_col = int(end_col_s)
        lines = self._lines_for(side)

        pieces = []
        first_raw = None
        first_col = 0
        for visual_row in range(start_row, end_row + 1):
            raw_idx = self._row_raw_idx(side, visual_row)
            if raw_idx is None or raw_idx >= len(lines):
                continue
            line = lines[raw_idx]
            lo = start_col if visual_row == start_row else 0
            hi = end_col if visual_row == end_row else len(line)
            lo = max(0, min(len(line), lo))
            hi = max(lo, min(len(line), hi))
            if first_raw is None:
                first_raw = raw_idx
                first_col = lo
            pieces.append((raw_idx, lo, hi))

        if not pieces:
            return None

        first_raw, first_col, _ = pieces[0]
        last_raw, _, last_hi = pieces[-1]
        prefix = lines[first_raw][:first_col]
        suffix = lines[last_raw][last_hi:]
        lines[first_raw:last_raw + 1] = [prefix + suffix]
        if not lines:
            lines.append("")
        return first_raw, first_col

    def _lines_for(self, side):
        return self.left_lines if side == "left" else self.right_lines

    def _set_lines_for(self, side, lines):
        if side == "left":
            self.left_lines = lines
        else:
            self.right_lines = lines

    def _line_for(self, side, raw_idx):
        lines = self._lines_for(side)
        if not lines:
            lines.append("")
        raw_idx = max(0, min(len(lines) - 1, raw_idx))
        return lines[raw_idx]

    def _ensure_raw_line(self, side, index):
        raw_idx, col = self._visual_to_raw(side, index)
        lines = self._lines_for(side)
        if raw_idx >= len(lines):
            lines.append("")
        row = self.rows[max(0, min(len(self.rows) - 1, int(index.split(".", 1)[0]) - 1))]
        if (side == "left" and row.left_idx is None) or (side == "right" and row.right_idx is None):
            if not (len(lines) == 1 and lines[0] == "" and raw_idx == 0):
                lines.insert(raw_idx, "")
            col = 0
        return raw_idx, col

    def _is_placeholder_index(self, side, index):
        try:
            visual_line = int(index.split(".", 1)[0]) - 1
            row = self.rows[max(0, min(len(self.rows) - 1, visual_line))]
            return row.left_idx is None if side == "left" else row.right_idx is None
        except Exception:
            return False

    def _on_key(self, event, side):
        if self._rendering:
            return "break"
        if event.state & 0x4:
            return None
        if event.keysym in ("Left", "Right", "Up", "Down", "Home", "End",
                            "Prior", "Next", "Shift_L", "Shift_R",
                            "Control_L", "Control_R", "Escape"):
            return None
        if event.keysym == "BackSpace":
            return self._backspace(side)
        if event.keysym == "Delete":
            return self._delete(side)
        if event.keysym == "Return":
            return self._insert_text(side, "\n")
        if event.keysym == "Tab":
            return self._insert_text(side, "\t")
        if event.char:
            return self._insert_text(side, event.char)
        return "break"

    def _paste(self, side):
        try:
            return self._insert_text(side, self.clipboard_get())
        except Exception:
            return "break"

    def _selected_raw_text(self, side):
        selected = self._selected_visual_range(side)
        if not selected:
            return ""
        start, end = selected
        start_line, start_col_s = start.split(".", 1)
        end_line, end_col_s = end.split(".", 1)
        start_row = max(0, int(start_line) - 1)
        end_row = max(0, int(end_line) - 1)
        start_col = int(start_col_s)
        end_col = int(end_col_s)
        lines = self._lines_for(side)
        parts = []
        for visual_row in range(start_row, end_row + 1):
            raw_idx = self._row_raw_idx(side, visual_row)
            if raw_idx is None or raw_idx >= len(lines):
                continue
            line = lines[raw_idx]
            lo = start_col if visual_row == start_row else 0
            hi = end_col if visual_row == end_row else len(line)
            lo = max(0, min(len(line), lo))
            hi = max(lo, min(len(line), hi))
            parts.append(line[lo:hi])
        return "\n".join(parts)

    def _copy_selection(self, side):
        text = self._selected_raw_text(side)
        if text:
            self.clipboard_clear()
            self.clipboard_append(text)
            self._status("Copied", "success")
        return "break"

    def _cut_selection(self, side):
        self._push_undo()
        text = self._selected_raw_text(side)
        if text:
            self.clipboard_clear()
            self.clipboard_append(text)
        deleted_at = self._delete_selection(side)
        if deleted_at:
            raw_idx, col = deleted_at
            self.render()
            self._restore_cursor(side, raw_idx, col)
            self._changed()
        return "break"

    def _insert_text(self, side, text):
        self._push_undo()
        widget = getattr(self, f"{side}_text")
        selected = self._selected_visual_range(side)
        deleted_at = self._delete_selection(side)
        if deleted_at:
            raw_idx, col = deleted_at
        elif selected:
            raw_idx, col = self._ensure_raw_line(side, selected[0])
        else:
            raw_idx, col = self._ensure_raw_line(side, widget.index("insert"))
        lines = self._lines_for(side)
        current = lines[raw_idx]
        parts = text.split("\n")
        if len(parts) == 1:
            lines[raw_idx] = current[:col] + text + current[col:]
            new_raw, new_col = raw_idx, col + len(text)
        else:
            new_lines = [current[:col] + parts[0]]
            new_lines.extend(parts[1:-1])
            new_lines.append(parts[-1] + current[col:])
            lines[raw_idx:raw_idx + 1] = new_lines
            new_raw, new_col = raw_idx + len(new_lines) - 1, len(parts[-1])
        self.render()
        self._restore_cursor(side, new_raw, new_col)
        self._changed()
        return "break"

    def _backspace(self, side):
        self._push_undo()
        widget = getattr(self, f"{side}_text")
        deleted_at = self._delete_selection(side)
        if deleted_at:
            raw_idx, col = deleted_at
            self.render()
            self._restore_cursor(side, raw_idx, col)
            self._changed()
            return "break"
        if self._is_placeholder_index(side, widget.index("insert")):
            return "break"
        raw_idx, col = self._ensure_raw_line(side, widget.index("insert"))
        lines = self._lines_for(side)
        if col > 0:
            lines[raw_idx] = lines[raw_idx][:col - 1] + lines[raw_idx][col:]
            new_raw, new_col = raw_idx, col - 1
        elif raw_idx > 0:
            new_col = len(lines[raw_idx - 1])
            lines[raw_idx - 1] += lines[raw_idx]
            lines.pop(raw_idx)
            new_raw = raw_idx - 1
        else:
            return "break"
        if not lines:
            lines.append("")
        self.render()
        self._restore_cursor(side, new_raw, new_col)
        self._changed()
        return "break"

    def _delete(self, side):
        self._push_undo()
        widget = getattr(self, f"{side}_text")
        deleted_at = self._delete_selection(side)
        if deleted_at:
            raw_idx, col = deleted_at
            self.render()
            self._restore_cursor(side, raw_idx, col)
            self._changed()
            return "break"
        if self._is_placeholder_index(side, widget.index("insert")):
            return "break"
        raw_idx, col = self._ensure_raw_line(side, widget.index("insert"))
        lines = self._lines_for(side)
        if col < len(lines[raw_idx]):
            lines[raw_idx] = lines[raw_idx][:col] + lines[raw_idx][col + 1:]
        elif raw_idx < len(lines) - 1:
            lines[raw_idx] += lines.pop(raw_idx + 1)
        else:
            return "break"
        if not lines:
            lines.append("")
        self.render()
        self._restore_cursor(side, raw_idx, col)
        self._changed()
        return "break"

    def _sync_y_from_scroll(self, side, *args):
        self._syncing_y = True
        try:
            self.left_text.yview(*args)
            self.right_text.yview(*args)
            y = self.left_text.yview()[0]
            self.left_gutter.yview_moveto(y)
            self.right_gutter.yview_moveto(y)
            self._draw_minimap()
        finally:
            self._syncing_y = False

    def _on_yscroll(self, side, *args):
        getattr(self, f"{side}_scroll_y").set(*args)
        getattr(self, f"{side}_gutter").yview_moveto(args[0])
        self._draw_minimap()
        if self._syncing_y:
            return
        other = "right" if side == "left" else "left"
        self._syncing_y = True
        try:
            getattr(self, f"{other}_text").yview_moveto(args[0])
            getattr(self, f"{other}_gutter").yview_moveto(args[0])
            self._draw_minimap()
        finally:
            self._syncing_y = False

    def _sync_x(self, side, *args):
        self.left_text.xview(*args)
        self.right_text.xview(*args)

    def _on_shift_mousewheel(self, event):
        units = -1 if event.delta > 0 else 1
        self.left_text.xview_scroll(units, "units")
        self.right_text.xview_scroll(units, "units")
        return "break"
