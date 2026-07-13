"""
SbtDeskTran - Main Application Window
Layout: TopBar (tabs left, modes right) -> LangBar (tran only) -> Content -> StatusBar
"""
import tkinter as tk
from tkinter import ttk, messagebox
import threading, json, os, sys, ctypes, difflib, re

from translator_engine import (
    LANGUAGES, LANG_CODE_TO_NAME, get_engine, ENGINES, TranslationError
)
from theme import THEMES
from widgets import FlatButton, themed_scrollbar
from virtual_diff_editor import VirtualDiffEditor
from logger import log
from app_paths import data_file, data_file_candidates

LANG_NAMES      = list(LANGUAGES.keys())
NOTES_FILE      = data_file("notes.json")
_DEFAULT_ENGINE = list(ENGINES.keys())[0]
FONT_MIN, FONT_MAX = 7, 28
VERSION_SEPARATOR_MIN_LEN = 10
WINDOW_EFFECTS = [
    ("solid", "◎ Solid", 1.00, False),
    ("blur", "◍ Blur", 0.98, True),
    ("frosted", "◐ Frosted", 0.92, True),
    ("transp", "○ Transparent", 0.85, False),
    ("dim", "◑ Dim", 0.88, False),
    ("ghost", "◌ Ghost", 0.80, False),
    ("clear", "□ Clear", 0.45, False),
]


def _is_version_separator(line: str) -> bool:
    stripped = line.strip()
    return len(stripped) >= VERSION_SEPARATOR_MIN_LEN and set(stripped) == {"-"}


# ─────────────────────────────────────────────
# FilterableCombobox
# ─────────────────────────────────────────────
class FilterableCombobox(ttk.Combobox):
    def __init__(self, parent, all_values, on_select=None, **kwargs):
        self._all = list(all_values)
        self._on_select = on_select
        self._user_postcommand = kwargs.pop("postcommand", None)
        kwargs.setdefault("state", "readonly")
        super().__init__(parent, values=self._all, **kwargs)
        self.configure(postcommand=self._begin_filter)
        self.bind("<KeyRelease>",         self._filter)
        self.bind("<FocusOut>",           self._restore)
        self.bind("<<ComboboxSelected>>", self._selected)

    def _begin_filter(self):
        self.config(state="normal")
        self["values"] = self._all
        try:
            self.selection_range(0, "end")
        except Exception:
            pass
        if self._user_postcommand:
            self._user_postcommand()

    def _filter(self, event):
        nav = {"Return","Escape","Tab","Up","Down","Left","Right",
               "Shift_L","Shift_R","Control_L","Control_R","Alt_L","Alt_R"}
        if event.keysym in nav:
            return
        typed = self.get().lower().strip()
        hits = [v for v in self._all if typed in v.lower()] if typed else self._all
        self["values"] = hits or self._all
        if hits:
            try: self.event_generate("<Down>")
            except Exception: pass

    def _restore(self, _=None):
        typed = self.get()
        m = next((v for v in self._all if v.lower() == typed.lower()), None)
        self.set(m if m else (self._all[0] if self._all else ""))
        self["values"] = self._all
        self.config(state="readonly")

    def _selected(self, _=None):
        self["values"] = self._all
        self.config(state="readonly")
        if self._on_select:
            self._on_select()

    def set_all(self, values):
        self._all = list(values)
        self["values"] = self._all


class TextFindBar(tk.Frame):
    """Find bar for one or more Text widgets."""

    def __init__(self, parent, theme, settings, save_fn, prefix, **kwargs):
        super().__init__(parent, bg=theme["bg3"], height=30, **kwargs)
        self.theme = theme
        self.settings = settings
        self.save_fn = save_fn
        self.prefix = prefix
        self.widgets = []
        self.matches = []
        self.match_index = -1
        self._timer = None

        self.find_var = tk.StringVar()
        self.find_case_var = tk.BooleanVar(
            value=settings.get(f"{prefix}_find_case", False))
        self.find_word_var = tk.BooleanVar(
            value=settings.get(f"{prefix}_find_word", False))
        self.find_regex_var = tk.BooleanVar(
            value=settings.get(f"{prefix}_find_regex", False))

        self.pack_propagate(False)
        self._build()

    def _build(self):
        t = self.theme
        self.find_entry = tk.Entry(self, textvariable=self.find_var,
            bg=t["bg"], fg=t["fg"], insertbackground=t["fg"],
            relief="flat", bd=1, highlightthickness=1,
            highlightbackground=t["border"], highlightcolor=t["accent"],
            font=t["font_ui"])
        self.find_entry.pack(side="left", fill="x", expand=True, padx=(6, 4), pady=4)
        self.find_entry.bind("<KeyRelease>", self._schedule_find, add="+")
        self.find_entry.bind("<Return>", lambda e: self.find_next(), add="+")
        self.find_entry.bind("<Shift-Return>", lambda e: self.find_prev(), add="+")
        self.find_entry.bind("<Escape>", lambda e: self.leave_find(), add="+")

        self.find_status = tk.Label(self, text="", bg=t["bg3"], fg=t["fg2"],
            font=t["font_small"], width=16, anchor="w")
        self.find_status.pack(side="left", padx=2)

        for text, var in (("Aa", self.find_case_var),
                          ("ab", self.find_word_var),
                          (".*", self.find_regex_var)):
            tk.Checkbutton(self, text=text, variable=var,
                command=self._toggle_find_option,
                bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                activebackground=t["bg3"], activeforeground=t["fg"],
                font=t["font_small"], padx=2).pack(side="left")

        FlatButton(self, text="Prev", theme=t, command=self.find_prev).pack(side="left", padx=2)
        FlatButton(self, text="Next", theme=t, command=self.find_next).pack(side="left", padx=2)
        FlatButton(self, text="X", theme=t, command=self.clear_find).pack(side="left", padx=(2, 6))

    def set_widgets(self, widgets):
        self.widgets = [(name, widget) for name, widget in widgets if widget]
        for _, widget in self.widgets:
            self._configure_tags(widget)
            widget.bind("<Control-f>", lambda e: self.focus_find(), add="+")
            widget.bind("<Control-F>", lambda e: self.focus_find(), add="+")
            widget.bind("<KeyRelease>", self._schedule_find, add="+")
        self.run_find()

    def _configure_tags(self, widget):
        t = self.theme
        search_bg = "#5c4b1f" if t.get("name") == "dark" else "#ffe58a"
        current_bg = "#8a6f22" if t.get("name") == "dark" else "#ffbd2e"
        widget.tag_configure("app_search", background=search_bg, foreground=t["fg"])
        widget.tag_configure("app_search_current", background=current_bg, foreground="#ffffff")

    def _toggle_find_option(self):
        self.settings[f"{self.prefix}_find_case"] = self.find_case_var.get()
        self.settings[f"{self.prefix}_find_word"] = self.find_word_var.get()
        self.settings[f"{self.prefix}_find_regex"] = self.find_regex_var.get()
        self.save_fn(self.settings)
        self.run_find()

    def focus_find(self):
        self.find_entry.focus_set()
        self.find_entry.selection_range(0, "end")
        self.run_find()
        return "break"

    def leave_find(self):
        if self.matches and 0 <= self.match_index < len(self.matches):
            _, widget, start, _ = self.matches[self.match_index]
            try:
                widget.focus_set()
                widget.mark_set("insert", start)
            except Exception:
                pass
        return "break"

    def clear_find(self):
        self.find_var.set("")
        self.run_find()
        self.leave_find()
        return "break"

    def _schedule_find(self, _=None):
        if self._timer:
            self.after_cancel(self._timer)
        self._timer = self.after(120, self.run_find)

    def _compiled_pattern(self):
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

    def run_find(self, keep_index=False):
        if self._timer:
            self.after_cancel(self._timer)
            self._timer = None
        old_current = None
        if keep_index and 0 <= self.match_index < len(self.matches):
            old_current = self.matches[self.match_index]
        for _, widget in self.widgets:
            widget.tag_remove("app_search", "1.0", "end")
            widget.tag_remove("app_search_current", "1.0", "end")
        self.matches = []
        self.match_index = -1

        pattern, error = self._compiled_pattern()
        if error:
            self.find_status.config(text="Bad regex")
            return
        if pattern is None:
            self.find_status.config(text="")
            return

        for name, widget in self.widgets:
            try:
                text = widget.get("1.0", "end-1c")
            except Exception:
                continue
            for match in pattern.finditer(text):
                if match.end() == match.start():
                    continue
                start = widget.index(f"1.0 + {match.start()} chars")
                end = widget.index(f"1.0 + {match.end()} chars")
                self.matches.append((name, widget, start, end))
                widget.tag_add("app_search", start, end)

        if self.matches:
            self.match_index = self.matches.index(old_current) if old_current in self.matches else 0
            self._mark_current()
        else:
            self.find_status.config(text="No results")

    def _mark_current(self):
        for _, widget in self.widgets:
            widget.tag_remove("app_search_current", "1.0", "end")
        if not self.matches:
            self.find_status.config(text="No results" if self.find_var.get() else "")
            return
        name, widget, start, end = self.matches[self.match_index]
        widget.tag_add("app_search_current", start, end)
        for _, w in self.widgets:
            w.tag_raise("app_search")
            w.tag_raise("app_search_current")
            w.tag_raise("sel")
        label = f"{self.match_index + 1}/{len(self.matches)}"
        self.find_status.config(text=f"{label} {name}" if len(self.widgets) > 1 else label)

    def _goto_match(self):
        if not self.matches:
            return "break"
        _, widget, start, end = self.matches[self.match_index]
        try:
            widget.focus_set()
            widget.mark_set("insert", start)
            widget.tag_remove("sel", "1.0", "end")
            widget.tag_add("sel", start, end)
            widget.see(start)
        except Exception:
            pass
        self._mark_current()
        return "break"

    def find_next(self):
        if not self.matches:
            self.run_find()
        if self.matches:
            self.match_index = (self.match_index + 1) % len(self.matches)
            return self._goto_match()
        return "break"

    def find_prev(self):
        if not self.matches:
            self.run_find()
        if self.matches:
            self.match_index = (self.match_index - 1) % len(self.matches)
            return self._goto_match()
        return "break"

# ─────────────────────────────────────────────
# Main App
# ─────────────────────────────────────────────
class SbtDeskTranApp:
    def __init__(self, root: tk.Tk, settings: dict, save_fn):
        self.root       = root
        self.settings   = settings
        self.save_fn    = save_fn
        self.theme      = THEMES[settings.get("theme", "dark")]
        self._compact   = settings.get("compact_mode", False)
        self._layout    = settings.get("layout", "horizontal")
        self._tab       = settings.get("active_tab", "tran")
        default_font_size = settings.get("font_size", 10)
        self._font_sizes = {
            "tran": settings.get("font_size_tran", default_font_size),
            "diff": settings.get("font_size_diff", default_font_size),
            "note": settings.get("font_size_note", default_font_size),
        }
        self._font_size = self._font_sizes.get(self._tab, default_font_size)
        self._word_wrap = settings.get("word_wrap", True)
        self._status_widget = None
        self._status_panel = ""
        self._window_effect = settings.get("window_effect", settings.get("opacity_mode", "blur"))
        if self._window_effect not in [effect[0] for effect in WINDOW_EFFECTS]:
            self._window_effect = "blur"
        self._timer     = None
        self._busy      = False
        self._pending_text = None
        self._notes     = self._load_notes()
        self._note_idx  = -1
        self._note_timer = None
        self._src_cache  = ""
        self._dst_cache  = ""
        self._tran_units = []
        self._tran_signature = None
        self._src_snapshot = None
        self._dst_snapshot = None
        # diff tab preserved text
        self._diff_left_cache  = ""
        self._diff_right_cache = ""
        self._note_body_cache  = ""
        self._note_title_cache = None
        self._note_dirty_cache = None
        self._note_snapshot = None
        self._note_dirty = False

        self._save_timer = None   # debounce handle for _save_settings


        from version import __version__
        self.root.title(f"SbtDeskTran v{__version__}")
        self.root.configure(bg=self.theme["bg"])

        self._setup_window()
        self._apply_window_effect()
        self._apply_ttk_style()
        self._build_ui()

        if settings.get("always_on_top", False):
            self.root.attributes("-topmost", True)

    # ──────────────────────────────────────────
    # Window geometry
    # ──────────────────────────────────────────
    def _setup_window(self):
        s = self.settings
        if self._compact:
            w, h = s.get("compact_width", 500), s.get("compact_height", 200)
        else:
            w, h = s.get("window_width", 980), s.get("window_height", 640)
        self.root.update_idletasks()
        sw, sh = self.root.winfo_screenwidth(), self.root.winfo_screenheight()
        x = max(0, (sw - w) // 2)
        y = max(0, (sh - h) // 2)
        self.root.geometry(f"{w}x{h}+{x}+{y}")
        self.root.minsize(340, 120)
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.bind("<Configure>", self._on_resize)

    def _on_close(self):
        """Standard window close — save and exit."""
        self._flush_note_auto_save()
        self.save_fn(self.settings)
        self.root.destroy()

    def _save_settings_debounced(self, delay_ms: int = 300):
        """Save settings after a short delay. Use for rapid changes like window resize."""
        if self._save_timer:
            self.root.after_cancel(self._save_timer)
        self._save_timer = self.root.after(delay_ms, lambda: self.save_fn(self.settings))

    # ──────────────────────────────────────────
    # Build UI  (called once; compact just hides/shows frames)
    # ──────────────────────────────────────────
    def _build_ui(self):
        t = self.theme

        # Dùng grid cho root để statusbar luôn ở đáy bất kể content height
        self.root.rowconfigure(0, weight=0)  # topbar
        self.root.rowconfigure(1, weight=0)  # langbar / compact_bar
        self.root.rowconfigure(2, weight=1)  # content (expand)
        self.root.rowconfigure(3, weight=0)  # statusbar
        self.root.columnconfigure(0, weight=1)

        # ── Top bar ────────────────────────────
        self.topbar = tk.Frame(self.root, bg=t["bg2"], height=36)
        self.topbar.grid(row=0, column=0, sticky="ew")
        self.topbar.grid_propagate(False)
        self._build_topbar()

        # ── Secondary bar row (langbar OR compact_bar, mutually exclusive) ──
        self.langbar = tk.Frame(self.root, bg=t["bg2"], height=34)
        self.langbar.grid_propagate(False)
        self._build_langbar()

        self.compact_bar = tk.Frame(self.root, bg=t["bg2"], height=30)
        self.compact_bar.grid_propagate(False)
        self._build_compact_bar()

        # ── Content ────────────────────────────
        self.content = tk.Frame(self.root, bg=t["bg"])
        self.content.grid(row=2, column=0, sticky="nsew")
        self._tab_frames = {}

        # ── Status bar — row 3, always at bottom ─
        self.statusbar = tk.Frame(self.root, bg=t["status_bg"], height=22)
        self.statusbar.grid_propagate(False)
        self.status_lbl = tk.Label(self.statusbar, text="  Ready",
            bg=t["status_bg"], fg=t["status_fg"], font=t["font_small"], anchor="w")
        self.status_lbl.pack(side="left", fill="y")
        self.char_lbl = tk.Label(self.statusbar, text="",
            bg=t["status_bg"], fg=t["status_fg"], font=t["font_small"], anchor="e", padx=8)
        self.char_lbl.pack(side="right", fill="y")

        # Show correct bars
        self._apply_bar_visibility()
        # Draw active tab
        self._switch_tab(self._tab, force=True)

    def _apply_bar_visibility(self):
        """Show/hide bars using grid rows. Statusbar always visible."""
        if self._compact:
            self.topbar.grid_remove()
            self.langbar.grid_remove()
            self.statusbar.grid_remove()
            self.compact_bar.grid(row=1, column=0, sticky="ew")
        else:
            self.compact_bar.grid_remove()
            self.topbar.grid(row=0, column=0, sticky="ew")
            if self._tab == "tran":
                self.langbar.grid(row=1, column=0, sticky="ew")
            else:
                self.langbar.grid_remove()
            # Statusbar always shown (not compact)
            self.statusbar.grid(row=3, column=0, sticky="ew")

    # ──────────────────────────────────────────
    # Top bar
    # ──────────────────────────────────────────
    def _build_topbar(self):
        t = self.theme
        for w in self.topbar.winfo_children():
            w.destroy()

        # Left: tabs
        left = tk.Frame(self.topbar, bg=t["bg2"])
        left.pack(side="left", fill="y")
        TABS = [("tran","⇄  Translate"), ("diff","⊕  Diff"), ("note","✎  Notes")]
        self._tab_btns = {}
        for tid, lbl in TABS:
            active = (tid == self._tab)
            btn = tk.Label(left,
                text=f"  {lbl}  ",
                bg=t["bg"] if active else t["bg2"],
                fg=t["fg"] if active else t["fg2"],
                font=t["font_ui_bold"] if active else t["font_ui"],
                cursor="hand2", padx=4, pady=0, relief="flat")
            btn.pack(side="left", fill="y")
            btn.bind("<Button-1>", lambda e, i=tid: self._switch_tab(i))
            btn.bind("<Enter>",  lambda e, b=btn, i=tid: b.config(bg=t["bg3"]) if i != self._tab else None)
            btn.bind("<Leave>",  lambda e, b=btn, i=tid: b.config(bg=t["bg"] if i==self._tab else t["bg2"]))
            self._tab_btns[tid] = btn

        # Separator
        tk.Frame(self.topbar, bg=t["border"], width=1).pack(side="left", fill="y", pady=6)

        # Right: mode buttons
        right = tk.Frame(self.topbar, bg=t["bg2"])
        right.pack(side="right", fill="y", padx=4)

        FlatButton(right, text="ⓘ About", theme=t,
            command=self._show_about).pack(side="right", padx=2)

        # Theme toggle
        self.theme_btn = FlatButton(right,
            text="☀ Light" if t["name"] == "dark" else "🌙 Dark",
            theme=t, command=self._toggle_theme)
        self.theme_btn.pack(side="right", padx=2)

        # Compact
        FlatButton(right, text="▣ Compact", theme=t,
            command=self._toggle_compact).pack(side="right", padx=2)

        # Always on top
        self.ontop_btn = FlatButton(right, text="📌 On Top", theme=t,
            command=self._toggle_ontop, toggle=True)
        self.ontop_btn.set_toggled(self.settings.get("always_on_top", False))
        self.ontop_btn.pack(side="right", padx=2)

        # Layout toggle — only shown on Tran tab
        if self._tab == "tran":
            self.layout_btn = FlatButton(right,
                text="⊟ Horizontal" if self._layout == "horizontal" else "⊞ Vertical",
                theme=t, command=self._toggle_layout)
            self.layout_btn.pack(side="right", padx=2)

        # Word wrap toggle — all tabs
        self.wrap_btn = FlatButton(right, text="↵ Wrap", theme=t,
            command=self._toggle_wrap, toggle=True)
        self.wrap_btn.set_toggled(self._word_wrap)
        self.wrap_btn.pack(side="right", padx=2)

        # Window opacity / blur toggle
        effect_labels = {key: label for key, label, _, _ in WINDOW_EFFECTS}
        cur_label = effect_labels.get(self._window_effect, effect_labels["solid"])
        self.opacity_btn = FlatButton(right, text=cur_label, theme=t,
            command=self._cycle_window_effect)
        self.opacity_btn.pack(side="right", padx=2)

        tk.Frame(self.topbar, bg=t["border"], height=2).pack(side="bottom", fill="x")

    def _show_about(self):
        t = self.theme
        try:
            from version import __version__
        except Exception:
            __version__ = "unknown"

        win = tk.Toplevel(self.root)
        win.title("About SbtDeskTran")
        win.transient(self.root)
        win.resizable(False, False)
        win.configure(bg=t["bg"])

        body = tk.Frame(win, bg=t["bg"], padx=18, pady=16)
        body.pack(fill="both", expand=True)

        tk.Label(body, text="SbtDeskTran", bg=t["bg"], fg=t["fg"],
                 font=("Segoe UI", 14, "bold")).pack(anchor="w")
        tk.Label(body, text=f"Version: {__version__}", bg=t["bg"], fg=t["fg2"],
                 font=t["font_ui"]).pack(anchor="w", pady=(4, 0))
        tk.Label(body, text="GitHub releases: SabiTechHolding/SbtDeskTran",
                 bg=t["bg"], fg=t["fg2"], font=t["font_small"]).pack(anchor="w", pady=(8, 0))

        actions = tk.Frame(body, bg=t["bg"])
        actions.pack(fill="x", pady=(14, 0))
        FlatButton(actions, text="Check Update", theme=t,
            command=lambda: self.check_for_updates(force=True, parent=win)).pack(side="left")
        FlatButton(actions, text="Close", theme=t,
            command=win.destroy).pack(side="right")

        win.update_idletasks()
        x = self.root.winfo_rootx() + max(0, (self.root.winfo_width() - win.winfo_width()) // 2)
        y = self.root.winfo_rooty() + max(0, (self.root.winfo_height() - win.winfo_height()) // 2)
        win.geometry(f"+{x}+{y}")
        win.lift()
        win.focus_force()

    def check_for_updates(self, force: bool = False, parent=None):
        try:
            import updater
        except Exception as exc:
            if force:
                messagebox.showerror("SbtDeskTran Update", f"Could not load updater:\n{exc}",
                                     parent=parent or self.root)
            return

        if not updater.is_supported_runtime():
            if force:
                messagebox.showinfo(
                    "SbtDeskTran Update",
                    "Update check is available only in the Windows executable build.",
                    parent=parent or self.root,
                )
            return

        if force:
            self._set_status("Checking for updates...", "warning")

        def on_checked(info, error):
            def handle():
                if error:
                    if force:
                        messagebox.showerror(
                            "SbtDeskTran Update",
                            f"Could not check for updates:\n{error}",
                            parent=parent or self.root,
                        )
                        self._set_status("Update check failed", "error")
                    return
                if not info:
                    if force:
                        messagebox.showinfo(
                            "SbtDeskTran Update",
                            "You are already using the latest version.",
                            parent=parent or self.root,
                        )
                        self._set_status("Already up to date", "success")
                    return
                self._prompt_update(info, parent=parent or self.root)

            self.root.after(0, handle)

        updater.check_for_update_async(on_checked, settings=self.settings)

    def _prompt_update(self, info, parent=None):
        ok = self._show_update_dialog(info, parent=parent or self.root)
        if ok:
            self._install_update(info, parent=parent or self.root)

    def _show_update_dialog(self, info, parent=None):
        t = self.theme
        win = tk.Toplevel(parent or self.root)
        win.title("SbtDeskTran Update")
        win.transient(parent or self.root)
        win.resizable(False, False)
        win.configure(bg=t["bg"])

        body = tk.Frame(win, bg=t["bg"], padx=18, pady=16)
        body.pack(fill="both", expand=True)

        tk.Label(body, text=f"Version {info.version} is available",
                 bg=t["bg"], fg=t["fg"],
                 font=("Segoe UI", 13, "bold")).pack(anchor="w")

        if info.notes:
            notes_lines = info.notes.strip().split("\n")
            sep_indices = [i for i, line in enumerate(notes_lines) if _is_version_separator(line)]
            if len(sep_indices) >= 3:
                short_text = "\n".join(notes_lines[sep_indices[0]:sep_indices[2]]).strip()
            elif len(sep_indices) == 2:
                short_text = "\n".join(notes_lines[sep_indices[0]:]).strip()
            else:
                short_text = info.notes.strip()

            notes_frame = tk.Frame(body, bg=t["bg"])
            notes_frame.pack(fill="both", expand=True, pady=(8, 0))
            tk.Label(notes_frame, text="Version changes:",
                     bg=t["bg"], fg=t["fg2"],
                     font=t["font_ui"]).pack(anchor="w")

            text_height = min(short_text.count("\n") + 1, 12)
            text_widget = tk.Text(notes_frame, height=text_height, width=65,
                                  bg=t["bg2"], fg=t["fg2"],
                                  font=("Consolas", 9),
                                  wrap="word", relief="flat", bd=4,
                                  padx=6, pady=4)
            text_widget.insert("1.0", short_text)
            text_widget.configure(state="disabled")
            text_widget.pack(fill="both", expand=True, pady=(4, 0))

            scrollbar = themed_scrollbar(notes_frame, t, orient="vertical",
                                         command=text_widget.yview)
            scrollbar.pack(side="right", fill="y", before=text_widget)
            text_widget.configure(yscrollcommand=scrollbar.set)

        actions = tk.Frame(body, bg=t["bg"])
        actions.pack(fill="x", pady=(12, 0))

        result = [False]
        def on_yes():
            result[0] = True
            win.destroy()
        def on_no():
            win.destroy()

        FlatButton(actions, text="Download & Install", theme=t,
                   command=on_yes).pack(side="left")
        FlatButton(actions, text="Cancel", theme=t,
                   command=on_no).pack(side="right")

        win.update_idletasks()
        pw = parent or self.root
        x = pw.winfo_rootx() + max(0, (pw.winfo_width() - win.winfo_width()) // 2)
        y = pw.winfo_rooty() + max(0, (pw.winfo_height() - win.winfo_height()) // 2)
        win.geometry(f"+{x}+{y}")
        win.lift()
        win.focus_force()
        win.grab_set()
        pw.wait_window(win)
        return result[0]

    def _install_update(self, info, parent=None):
        def worker():
            try:
                import updater
                helper = updater.download_and_stage_update(info, restart=True, settings=self.settings)

                def done():
                    t = self.theme
                    win = tk.Toplevel(parent or self.root)
                    win.title("SbtDeskTran Update")
                    win.transient(parent or self.root)
                    win.resizable(False, False)
                    win.configure(bg=t["bg"])

                    body = tk.Frame(win, bg=t["bg"], padx=24, pady=20)
                    body.pack(fill="both", expand=True)

                    tk.Label(body, text="✓ Update downloaded successfully",
                             bg=t["bg"], fg="#2ecc71",
                             font=("Segoe UI", 13, "bold")).pack(anchor="center", pady=(0, 6))
                    tk.Label(body, text=f"Version {info.version} has been downloaded.",
                             bg=t["bg"], fg=t["fg2"],
                             font=t["font_ui"]).pack(anchor="center")
                    tk.Label(body, text="The app will now close and restart to apply the update.",
                             bg=t["bg"], fg=t["fg2"],
                             font=t["font_ui"]).pack(anchor="center", pady=(4, 12))

                    def close_and_restart():
                        win.destroy()
                        self._flush_note_auto_save()
                        self.save_fn(self.settings)
                        updater.run_update_helper(helper)
                        self.root.destroy()
                        os._exit(0)

                    FlatButton(body, text="Restart Now", theme=t,
                               command=close_and_restart).pack()

                    win.update_idletasks()
                    pw = parent or self.root
                    x = pw.winfo_rootx() + max(0, (pw.winfo_width() - win.winfo_width()) // 2)
                    y = pw.winfo_rooty() + max(0, (pw.winfo_height() - win.winfo_height()) // 2)
                    win.geometry(f"+{x}+{y}")
                    win.lift()
                    win.focus_force()
                    win.grab_set()
                    pw.wait_window(win)

                self.root.after(0, done)
            except Exception as exc:
                self.root.after(0, lambda: (
                    messagebox.showerror(
                        "SbtDeskTran Update",
                        f"Could not install update:\n{exc}",
                        parent=parent or self.root,
                    ),
                    self._set_status("Update install failed", "error"),
                ))

        self._set_status(f"Downloading update {info.version}...", "warning")
        threading.Thread(target=worker, daemon=True).start()

    # ──────────────────────────────────────────
    # Lang bar (shown below topbar for Tran tab)
    # ──────────────────────────────────────────
    def _build_langbar(self):
        t = self.theme
        for w in self.langbar.winfo_children():
            w.destroy()
        self.langbar.config(bg=t["bg2"])

        inner = tk.Frame(self.langbar, bg=t["bg2"])
        inner.pack(side="left", fill="y", padx=6)

        tk.Label(inner, text="Engine:", bg=t["bg2"], fg=t["fg2"],
                 font=t["font_small"]).pack(side="left", padx=(0,2))
        saved = self.settings.get("engine", _DEFAULT_ENGINE)
        if saved not in ENGINES: saved = _DEFAULT_ENGINE
        self.engine_var = tk.StringVar(value=saved)
        FilterableCombobox(inner, list(ENGINES.keys()),
            on_select=self._on_translate_option_select,
            textvariable=self.engine_var, width=15,
            font=t["font_ui"]).pack(side="left", padx=(0,8))

        tk.Label(inner, text="From:", bg=t["bg2"], fg=t["fg2"],
                 font=t["font_small"]).pack(side="left", padx=(0,2))
        self.src_lang_var = tk.StringVar(value=self.settings.get("src_lang","Auto Detect"))
        FilterableCombobox(inner, LANG_NAMES,
            on_select=self._on_translate_option_select,
            textvariable=self.src_lang_var, width=17,
            font=t["font_ui"]).pack(side="left", padx=(0,2))

        FlatButton(inner, text="⇄", theme=t,
            command=self._swap_langs).pack(side="left", padx=3)

        tk.Label(inner, text="To:", bg=t["bg2"], fg=t["fg2"],
                 font=t["font_small"]).pack(side="left", padx=(0,2))
        dest_langs = [l for l in LANG_NAMES if l != "Auto Detect"]
        self.dest_lang_var = tk.StringVar(value=self.settings.get("dest_lang","English"))
        FilterableCombobox(inner, dest_langs,
            on_select=self._on_translate_option_select,
            textvariable=self.dest_lang_var, width=17,
            font=t["font_ui"]).pack(side="left", padx=(0,4))

        tk.Frame(self.langbar, bg=t["border"], height=1).pack(side="bottom", fill="x")

    # ──────────────────────────────────────────
    # Compact bar
    # ──────────────────────────────────────────
    def _build_compact_bar(self):
        t = self.theme
        for w in self.compact_bar.winfo_children():
            w.destroy()
        self.compact_bar.config(bg=t["bg2"])

        # Ensure vars exist
        if not hasattr(self, "src_lang_var"):
            self.src_lang_var  = tk.StringVar(value=self.settings.get("src_lang","Auto Detect"))
        if not hasattr(self, "dest_lang_var"):
            self.dest_lang_var = tk.StringVar(value=self.settings.get("dest_lang","English"))

        dest_langs = [l for l in LANG_NAMES if l != "Auto Detect"]
        FilterableCombobox(self.compact_bar, LANG_NAMES,
            on_select=self._on_translate_option_select,
            textvariable=self.src_lang_var, width=12,
            font=t["font_small"]).pack(side="left", padx=(6,2), pady=4)
        tk.Label(self.compact_bar, text="→", bg=t["bg2"],
                 fg=t["fg2"], font=t["font_ui"]).pack(side="left")
        FilterableCombobox(self.compact_bar, dest_langs,
            on_select=self._on_translate_option_select,
            textvariable=self.dest_lang_var, width=12,
            font=t["font_small"]).pack(side="left", padx=(2,6), pady=4)

        # ⤢ Full — exit compact
        ex = tk.Label(self.compact_bar, text="⤢ Full",
            bg=t["accent"], fg=t["bg"], font=t["font_ui_bold"],
            cursor="hand2", padx=10, pady=1, relief="flat")
        ex.pack(side="right", padx=6, pady=4)
        ex.bind("<Button-1>", lambda e: self._toggle_compact())
        ex.bind("<Enter>",    lambda e: ex.config(bg=t["btn_hover"], fg=t["fg"]))
        ex.bind("<Leave>",    lambda e: ex.config(bg=t["accent"],    fg=t["bg"]))

        about = tk.Label(self.compact_bar, text="ⓘ",
            bg=t["btn_bg"], fg=t["btn_fg"],
            cursor="hand2", padx=7, pady=1, font=t["font_ui"], relief="flat")
        about.pack(side="right", padx=2, pady=4)
        about.bind("<Button-1>", lambda e: self._show_about())
        about.bind("<Enter>",    lambda e: about.config(bg=t["btn_hover"], fg=t["fg"]))
        about.bind("<Leave>",    lambda e: about.config(bg=t["btn_bg"], fg=t["btn_fg"]))

        on = self.settings.get("always_on_top", False)
        pin = tk.Label(self.compact_bar, text="📌",
            bg=t["accent"] if on else t["btn_bg"],
            fg=t["bg"]    if on else t["btn_fg"],
            cursor="hand2", padx=6, pady=1, font=t["font_ui"], relief="flat")
        pin.pack(side="right", padx=2, pady=4)
        def _pin(_):
            self._toggle_ontop()
            on2 = self.settings.get("always_on_top", False)
            pin.config(bg=t["accent"] if on2 else t["btn_bg"],
                       fg=t["bg"]    if on2 else t["btn_fg"])
        pin.bind("<Button-1>", _pin)
    # ──────────────────────────────────────────
    # Tab switching
    # ──────────────────────────────────────────
    def _discard_tab_frame(self, tab_id=None):
        frames = self._tab_frames if tab_id is None else {tab_id: self._tab_frames.get(tab_id)}
        for tid, frame in list(frames.items()):
            if frame is None:
                continue
            try:
                frame.destroy()
            except Exception:
                pass
            self._tab_frames.pop(tid, None)

    def _activate_tab_status(self, tab_id: str):
        try:
            if tab_id == "tran" and hasattr(self, "src_text"):
                self._status_widget = self.src_text
                self._status_panel = "Source"
            elif tab_id == "diff" and hasattr(self, "diff_left"):
                self._status_widget = self.diff_left
                self._status_panel = "Left"
            elif tab_id == "note" and hasattr(self, "note_body"):
                self._status_widget = self.note_body
                self._status_panel = "Note"
            self._update_status_metrics()
        except Exception:
            pass

    def _switch_tab(self, tab_id: str, force: bool = False):
        if tab_id == self._tab and not force:
            return
        if not force:
            self._capture_live_tab_state()
        # Save text from current tab before switching
        if self._tab == "diff" and not force:
            try:
                if hasattr(self, "diff_editor"):
                    self._diff_left_cache = self.diff_editor.get_text("left")
                    self._diff_right_cache = self.diff_editor.get_text("right")
                else:
                    self._diff_left_cache  = self.diff_left.get("1.0","end-1c")
                    self._diff_right_cache = self.diff_right.get("1.0","end-1c")
            except Exception:
                pass
        if self._tab == "note" and not force:
            try:
                self._flush_note_auto_save()
                self._note_body_cache = self.note_body.get("1.0","end-1c")
            except Exception:
                pass

        self._tab = tab_id
        self.settings["active_tab"] = tab_id
        self._font_size = self._font_sizes.get(tab_id, self.settings.get("font_size", 10))
        self.save_fn(self.settings)

        # Refresh tab button styles
        try: self._build_topbar()
        except Exception: pass

        # Show/hide langbar; statusbar always shown
        if not self._compact:
            if tab_id == "tran":
                self.langbar.grid(row=1, column=0, sticky="ew")
            else:
                self.langbar.grid_remove()
            self.statusbar.grid(row=3, column=0, sticky="ew")

        if force:
            self._discard_tab_frame(tab_id)

        for tid, frame in list(self._tab_frames.items()):
            try:
                if tid != tab_id:
                    frame.pack_forget()
            except Exception:
                pass

        if tab_id not in self._tab_frames:
            self._tab_frames[tab_id] = tk.Frame(self.content, bg=self.theme["bg"])
            {"tran": self._build_tran_tab,
             "diff": self._build_diff_tab,
             "note": self._build_note_tab}.get(tab_id, self._build_tran_tab)()

        self._tab_frames[tab_id].pack(fill="both", expand=True)
        self._activate_tab_status(tab_id)

    # ──────────────────────────────────────────
    # TAB: Translate
    # ──────────────────────────────────────────
    def _build_tran_tab(self):
        t = self.theme
        parent = self._tab_frames.get("tran", self.content)
        self.tran_find = TextFindBar(parent, t, self.settings, self.save_fn, "tran")
        self.tran_find.pack(fill="x")

        # ── Split panes ──────────────────────────────────────────────────────
        orient = tk.HORIZONTAL if self._layout == "horizontal" else tk.VERTICAL
        pw = tk.PanedWindow(parent, orient=orient,
            bg=t["border"], sashwidth=5, sashrelief="flat", bd=0, handlesize=0)
        pw.pack(fill="both", expand=True)

        # Source pane
        sf = tk.Frame(pw, bg=t["bg"]); pw.add(sf, stretch="always")
        h1 = tk.Frame(sf, bg=t["bg3"], height=28); h1.pack(fill="x"); h1.pack_propagate(False)
        tk.Label(h1, text="  Source", bg=t["bg3"], fg=t["fg2"],
                 font=t["font_ui_bold"], anchor="w").pack(side="left", fill="y")
        # Translate + Auto inline — ẩn khi compact
        if not self._compact:
            FlatButton(h1, text="▶ Translate", theme=t,
                command=self._do_translate).pack(side="left", padx=(8,2), pady=4)
            self.auto_var = tk.BooleanVar(value=self.settings.get("tran_auto", True))
            def _on_auto_tran():
                self.settings["tran_auto"] = self.auto_var.get()
                self.save_fn(self.settings)
            tk.Checkbutton(h1, text="Auto", variable=self.auto_var,
                command=_on_auto_tran,
                bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                activebackground=t["bg3"], activeforeground=t["fg"],
                font=t["font_small"]).pack(side="left", padx=2)
        else:
            self.auto_var = tk.BooleanVar(value=self.settings.get("tran_auto", True))
        FlatButton(h1, text="✕ Clear", theme=t, command=self._clear_src).pack(side="right", padx=4)
        FlatButton(h1, text="⎘ Copy", theme=t,
            command=lambda: self._copy_text(self.src_text)).pack(side="right", padx=2)

        stf = tk.Frame(sf, bg=t["bg"]); stf.pack(fill="both", expand=True)
        wrap_mode = "word" if self._word_wrap else "none"
        self.src_text = tk.Text(stf, bg=t["bg"], fg=t["fg"],
            font=("Consolas", self._font_size), wrap=wrap_mode,
            relief="flat", bd=0, padx=8, pady=6,
            insertbackground=t["fg"], selectbackground=t["selection"], undo=True)
        sc = themed_scrollbar(stf, t, command=self.src_text.yview)
        self.src_text.config(yscrollcommand=sc.set)
        sc.pack(side="right", fill="y")
        if not self._word_wrap:
            sc_x = themed_scrollbar(sf, t, orient="horizontal", command=self.src_text.xview)
            self.src_text.config(xscrollcommand=sc_x.set)
            sc_x.pack(side="bottom", fill="x", before=stf)
        self.src_text.pack(fill="both", expand=True)
        self.src_text.bind("<KeyRelease>",         self._on_src_key)
        self.src_text.bind("<<Paste>>",            self._on_src_key)
        self.src_text.bind("<Control-Return>",     lambda e: self._do_translate())
        self.src_text.bind("<Control-MouseWheel>", self._on_zoom)
        self._bind_status_metrics(self.src_text, "Source")
        if self._src_snapshot:
            self._restore_text_widget(self.src_text, self._src_snapshot)
        elif self._src_cache:
            self.src_text.insert("1.0", self._src_cache)

        # Dest pane
        df = tk.Frame(pw, bg=t["bg"]); pw.add(df, stretch="always")
        h2 = tk.Frame(df, bg=t["bg3"], height=28); h2.pack(fill="x"); h2.pack_propagate(False)
        self.det_lbl = tk.Label(h2, text="  Translated",
            bg=t["bg3"], fg=t["accent"], font=t["font_ui_bold"], anchor="w")
        self.det_lbl.pack(side="left", fill="y")
        FlatButton(h2, text="⎘ Copy", theme=t,
            command=lambda: self._copy_text(self.dest_text)).pack(side="right", padx=4)

        dtf = tk.Frame(df, bg=t["bg"]); dtf.pack(fill="both", expand=True)
        self.dest_text = tk.Text(dtf, bg=t["bg"], fg=t["accent2"],
            font=("Consolas", self._font_size), wrap=wrap_mode,
            relief="flat", bd=0, padx=8, pady=6,
            insertbackground=t["fg"], selectbackground=t["selection"],
            state="disabled", cursor="arrow")  # readonly
        ds = themed_scrollbar(dtf, t, command=self.dest_text.yview)
        self.dest_text.config(yscrollcommand=ds.set)
        ds.pack(side="right", fill="y")
        if not self._word_wrap:
            ds_x = themed_scrollbar(df, t, orient="horizontal", command=self.dest_text.xview)
            self.dest_text.config(xscrollcommand=ds_x.set)
            ds_x.pack(side="bottom", fill="x", before=dtf)
        self.dest_text.pack(fill="both", expand=True)
        self.dest_text.bind("<Control-MouseWheel>", self._on_zoom)
        self._bind_status_metrics(self.dest_text, "Translated")
        if self._dst_snapshot:
            self._restore_text_widget(self.dest_text, self._dst_snapshot, readonly=True)
        elif self._dst_cache:
            self.dest_text.config(state="normal")
            self.dest_text.insert("1.0", self._dst_cache)
            self.dest_text.config(state="disabled")
        self.tran_find.set_widgets((
            ("Source", self.src_text),
            ("Translated", self.dest_text),
        ))
        self._status_widget = self.src_text
        self._status_panel = "Source"
        self._update_status_metrics()

    # ──────────────────────────────────────────
    # TAB: Diff (two independent free-text boxes)
    # ──────────────────────────────────────────
    def _build_diff_tab(self):
        t = self.theme
        parent = self._tab_frames.get("diff", self.content)

        self.diff_editor = VirtualDiffEditor(
            parent,
            theme=t,
            settings=self.settings,
            save_fn=self.save_fn,
            save_debounced=self._save_settings_debounced,
            left_text=self._diff_left_cache,
            right_text=self._diff_right_cache,
            word_wrap=self._word_wrap,
            font_size=self._font_size,
            compact=self._compact,
            on_zoom=self._on_zoom,
            on_status=self._set_status,
            on_changed=self._update_status_metrics,
        )
        self.diff_editor.pack(fill="both", expand=True)
        self.diff_left = self.diff_editor.left_text
        self.diff_right = self.diff_editor.right_text
        self.diff_auto_var = self.diff_editor.auto_var
        self.diff_word_diff_var = self.diff_editor.word_diff_var
        self.diff_ignore_ws_var = self.diff_editor.ignore_ws_var
        self._bind_status_metrics(self.diff_left, "Left")
        self._bind_status_metrics(self.diff_right, "Right")
        self._status_widget = self.diff_left
        self._status_panel = "Left"
        self._update_status_metrics()

    def _diff_input_xscroll(self, side, *args):
        try:
            getattr(self, f"diff_{side}_scroll_x").set(*args)
        except Exception:
            pass

    def _sync_diff_input_x(self, side, *args):
        try:
            self.diff_left.xview(*args)
            self.diff_right.xview(*args)
        except Exception:
            pass

    def _diff_input_yscroll(self, side, *args):
        try:
            getattr(self, f"diff_{side}_scroll_y").set(*args)
            getattr(self, f"diff_{side}_gutter").yview_moveto(args[0])
            if getattr(self, "_syncing_diff_y", False):
                return
            self._sync_diff_y_from_side(side)
        except Exception:
            self._syncing_diff_y = False

    def _sync_diff_input_y(self, side, *args):
        try:
            self._syncing_diff_y = True
            self.diff_left.yview(*args)
            self.diff_right.yview(*args)
            self.diff_left_gutter.yview_moveto(self.diff_left.yview()[0])
            self.diff_right_gutter.yview_moveto(self.diff_right.yview()[0])
        except Exception:
            pass
        finally:
            self._syncing_diff_y = False

    def _sync_diff_y_from_side(self, side):
        other = "right" if side == "left" else "left"
        src = getattr(self, f"diff_{side}")
        dst = getattr(self, f"diff_{other}")
        try:
            top_line = int(src.index("@0,0").split(".", 1)[0])
            dst_total = max(1, int(dst.index("end-1c").split(".", 1)[0]))
            self._syncing_diff_y = True
            dst.yview_moveto(max(0, top_line - 1) / dst_total)
            getattr(self, f"diff_{other}_gutter").yview_moveto(dst.yview()[0])
            getattr(self, f"diff_{side}_gutter").yview_moveto(src.yview()[0])
        except Exception:
            pass
        finally:
            self._syncing_diff_y = False

    def _on_diff_shift_mousewheel(self, event):
        try:
            units = -1 if event.delta > 0 else 1
            self.diff_left.xview_scroll(units, "units")
            self.diff_right.xview_scroll(units, "units")
            return "break"
        except Exception:
            return None

    def _clear_diff_side(self, side):
        try:
            if hasattr(self, "diff_editor"):
                self.diff_editor.clear_side(side)
            else:
                getattr(self, f"diff_{side}").delete("1.0", "end")
                self._run_diff()
                self._refresh_diff_line_numbers()
            self._update_status_metrics()
        except Exception:
            pass

    def _refresh_diff_line_numbers(self):
        for side in ("left", "right"):
            try:
                text = getattr(self, f"diff_{side}")
                gutter = getattr(self, f"diff_{side}_gutter")
                line_count = max(1, int(text.index("end-1c").split(".", 1)[0]))
                width = max(4, len(str(line_count)) + 1)
                nums = "\n".join(f"{i:>{width - 1}}" for i in range(1, line_count + 1))
                gutter.config(state="normal", width=width)
                gutter.delete("1.0", "end")
                gutter.insert("1.0", nums)
                gutter.config(state="disabled")
                gutter.yview_moveto(text.yview()[0])
            except Exception:
                pass

    def _configure_diff_input_tags(self, widget):
        t = self.theme
        widget.tag_configure("diff_delete_line",
            background=t["diff_del_bg"], foreground=t["diff_del_fg"])
        widget.tag_configure("diff_insert_line",
            background=t["diff_add_bg"], foreground=t["diff_add_fg"])
        widget.tag_configure("diff_delete_inline",
            background=t["diff_del_inline"], foreground="#ffffff")
        widget.tag_configure("diff_insert_inline",
            background=t["diff_add_inline"], foreground="#ffffff")
        widget.tag_raise("diff_delete_inline")
        widget.tag_raise("diff_insert_inline")

    def _clear_diff_input_tags(self):
        for widget in (self.diff_left, self.diff_right):
            for tag in ("diff_delete_line", "diff_insert_line",
                        "diff_delete_inline", "diff_insert_inline"):
                widget.tag_remove(tag, "1.0", "end")

    def _tag_diff_line(self, widget, lineno, tag):
        widget.tag_add(tag, f"{lineno}.0", f"{lineno}.0 lineend +1c")

    def _tag_diff_inline(self, widget, lineno, tokens, inline_tag):
        col = 0
        for token in tokens:
            next_col = col + len(token.text)
            if token.kind != "equal" and next_col > col:
                widget.tag_add(inline_tag, f"{lineno}.{col}", f"{lineno}.{next_col}")
            col = next_col

    def _apply_diff_highlights(self, chunks):
        from diff_engine import compute_inline_diff

        self._clear_diff_input_tags()
        old_lineno = 1
        new_lineno = 1

        for chunk in chunks:
            if chunk.kind == "equal":
                old_lineno += len(chunk.old_lines)
                new_lineno += len(chunk.new_lines)
            elif chunk.kind == "delete":
                for _ in chunk.old_lines:
                    self._tag_diff_line(self.diff_left, old_lineno, "diff_delete_line")
                    old_lineno += 1
            elif chunk.kind == "insert":
                for _ in chunk.new_lines:
                    self._tag_diff_line(self.diff_right, new_lineno, "diff_insert_line")
                    new_lineno += 1
            elif chunk.kind == "replace":
                max_lines = max(len(chunk.old_lines), len(chunk.new_lines))
                for i in range(max_lines):
                    old_line = chunk.old_lines[i] if i < len(chunk.old_lines) else None
                    new_line = chunk.new_lines[i] if i < len(chunk.new_lines) else None
                    if old_line is not None:
                        self._tag_diff_line(self.diff_left, old_lineno, "diff_delete_line")
                    if new_line is not None:
                        self._tag_diff_line(self.diff_right, new_lineno, "diff_insert_line")
                    if old_line is not None and new_line is not None:
                        old_toks, new_toks = compute_inline_diff(old_line, new_line)
                        self._tag_diff_inline(
                            self.diff_left, old_lineno, old_toks, "diff_delete_inline")
                        self._tag_diff_inline(
                            self.diff_right, new_lineno, new_toks, "diff_insert_inline")
                    if old_line is not None:
                        old_lineno += 1
                    if new_line is not None:
                        new_lineno += 1

    def _run_diff(self):
        try:
            if hasattr(self, "diff_editor"):
                self.diff_editor.render()
                return
            left  = self.diff_left.get("1.0","end-1c")
            right = self.diff_right.get("1.0","end-1c")
            ignore_ws = (
                bool(self.diff_ignore_ws_var.get())
                if hasattr(self, "diff_ignore_ws_var")
                else self.settings.get("diff_ignore_whitespace", False)
            )
            # Show stats in statusbar instead of diff header
            from diff_engine import get_diff_stats, compute_line_diff
            chunks = compute_line_diff(left, right, ignore_whitespace=ignore_ws)
            self._refresh_diff_line_numbers()
            self._apply_diff_highlights(chunks)
            stats = get_diff_stats(chunks)
            suffix = " (whitespace ignored)" if ignore_ws else ""
            self._set_status(
                f"+{stats['added']} added   -{stats['removed']} removed   "
                f"{stats['changed_blocks']} block(s) changed{suffix}")
        except Exception as e:
            self._set_status(f"Diff error: {e}", "error")
    # ──────────────────────────────────────────
    # TAB: Notes
    # ──────────────────────────────────────────
    def _build_note_tab(self):
        t = self.theme
        parent = self._tab_frames.get("note", self.content)
        self.note_find = TextFindBar(parent, t, self.settings, self.save_fn, "note")
        self.note_find.pack(fill="x")
        outer = tk.PanedWindow(parent, orient=tk.HORIZONTAL,
            bg=t["border"], sashwidth=5, sashrelief="flat", bd=0, handlesize=0)
        outer.pack(fill="both", expand=True)

        # Editor — bên trái, chiếm phần lớn
        ef = tk.Frame(outer, bg=t["bg"])
        outer.add(ef, stretch="always", minsize=200)

        # Editor header: title entry + Save
        eh = tk.Frame(ef, bg=t["bg3"], height=28); eh.pack(fill="x"); eh.pack_propagate(False)
        self.note_title = tk.StringVar()
        self.note_title_entry = tk.Entry(eh, textvariable=self.note_title, bg=t["bg3"], fg=t["fg"],
            font=t["font_ui_bold"], relief="flat", bd=0,
            insertbackground=t["fg"])
        self.note_title_entry.pack(side="left", fill="both", expand=True, padx=8, pady=4)
        self.note_title_entry.bind("<KeyRelease>", self._note_debounce)
        self.note_auto_var = tk.BooleanVar(value=self.settings.get("note_auto_save", True))
        tk.Checkbutton(eh, text="Auto Save", variable=self.note_auto_var,
            command=self._toggle_note_auto_save,
            bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
            activebackground=t["bg3"], activeforeground=t["fg"],
            font=t["font_small"]).pack(side="right", padx=4)
        self.note_save_state_lbl = tk.Label(eh, bg=t["bg3"], fg=t["success"],
            text="✓ Saved", font=t["font_small"], padx=6)
        self.note_save_state_lbl.pack(side="right", fill="y")
        FlatButton(eh, text="💾 Save", theme=t, command=self._note_save).pack(side="right", padx=4)

        # Editor body
        etf = tk.Frame(ef, bg=t["bg"]); etf.pack(fill="both", expand=True)
        wrap_mode = "word" if self._word_wrap else "none"
        self.note_body = tk.Text(etf, bg=t["bg"], fg=t["fg"],
            font=("Consolas", self._font_size), wrap=wrap_mode,
            relief="flat", bd=0, padx=8, pady=6,
            insertbackground=t["fg"], selectbackground=t["selection"], undo=True)
        esc = themed_scrollbar(etf, t, command=self.note_body.yview)
        self.note_body.config(yscrollcommand=esc.set)
        esc.pack(side="right", fill="y")
        if not self._word_wrap:
            esc_x = themed_scrollbar(ef, t, orient="horizontal", command=self.note_body.xview)
            self.note_body.config(xscrollcommand=esc_x.set)
            esc_x.pack(side="bottom", fill="x")
        self.note_body.pack(fill="both", expand=True)
        self.note_body.bind("<Control-Return>",     lambda e: self._note_save())
        self.note_body.bind("<Control-MouseWheel>", self._on_zoom)
        self.note_body.bind("<KeyRelease>",          self._note_debounce)
        self._bind_status_metrics(self.note_body, "Note")
        self.note_find.set_widgets((("Note", self.note_body),))

        # Sidebar — bên phải, width vừa đủ 3 nút
        saved_sb_w = self.settings.get("note_sidebar_width", 130)
        sb = tk.Frame(outer, bg=t["bg2"])
        outer.add(sb, width=saved_sb_w, minsize=100, stretch="never")

        def _on_sash(_=None):
            try:
                coords = outer.sash_coord(0)
                # sash 0 is between ef and sb — width of sb = total - sash_x
                total = outer.winfo_width()
                self.settings["note_sidebar_width"] = max(100, total - coords[0])
            except Exception:
                pass
        outer.bind("<ButtonRelease-1>", _on_sash)

        # Sidebar header: 3 buttons
        sh = tk.Frame(sb, bg=t["bg3"], height=28); sh.pack(fill="x"); sh.pack_propagate(False)
        FlatButton(sh, text="＋", theme=t, command=self._note_new).pack(side="left", padx=2)
        self.note_del_btn = FlatButton(sh, text="🗑", theme=t, command=self._note_del)
        self.note_del_btn.pack(side="left", padx=2)

        # Note list
        lf = tk.Frame(sb, bg=t["bg2"]); lf.pack(fill="both", expand=True)
        self.note_list = tk.Listbox(lf, bg=t["bg2"], fg=t["fg"], font=t["font_small"],
            selectbackground=t["accent"], selectforeground=t["bg"],
            relief="flat", bd=0, activestyle="none", highlightthickness=0)
        nsc = themed_scrollbar(lf, t, command=self.note_list.yview)
        self.note_list.config(yscrollcommand=nsc.set)
        nsc.pack(side="right", fill="y"); self.note_list.pack(fill="both", expand=True)
        self.note_list.bind("<<ListboxSelect>>", self._note_select)

        self._note_refresh()
        self._note_idx = getattr(self, "_note_idx", 0)
        _restore_body = getattr(self, "_note_body_cache", "")
        _restore_title = getattr(self, "_note_title_cache", None)
        _restore_dirty = self._note_dirty if getattr(self, "_note_dirty_cache", None) is None else self._note_dirty_cache
        if self._notes:
            idx = max(0, min(self._note_idx, len(self._notes)-1))
            self.note_list.selection_set(idx)
            self._note_load(idx)
        # After note_load, restore unsaved body if toggling wrap
        if self._note_snapshot:
            self._restore_text_widget(self.note_body, self._note_snapshot)
            self._note_snapshot = None
            self._note_body_cache = ""
        elif _restore_body:
            self.note_body.delete("1.0", "end")
            self.note_body.insert("1.0", _restore_body)
            self._note_body_cache = ""
        if _restore_title is not None:
            self.note_title.set(_restore_title)
            self._note_title_cache = None
        self._note_dirty_cache = None
        self._status_widget = self.note_body
        self._status_panel = "Note"
        self._set_note_dirty(_restore_dirty)
        self._update_status_metrics()

    def _load_notes(self):
        for path in data_file_candidates("notes.json"):
            if not os.path.exists(path):
                continue
            try:
                with open(path, "r", encoding="utf-8") as f:
                    notes = json.load(f)
                if os.path.normcase(os.path.abspath(path)) != os.path.normcase(os.path.abspath(NOTES_FILE)):
                    try:
                        with open(NOTES_FILE, "w", encoding="utf-8") as f:
                            json.dump(notes, f, indent=2, ensure_ascii=False)
                    except Exception:
                        pass
                return notes
            except Exception:
                pass
        try:
            with open(NOTES_FILE, "w", encoding="utf-8") as f:
                json.dump([], f)
        except Exception:
            pass
        return []

    def _save_notes(self):
        try:
            with open(NOTES_FILE, "w", encoding="utf-8") as f:
                json.dump(self._notes, f, indent=2, ensure_ascii=False)
        except Exception:
            pass

    def _note_refresh(self):
        try:
            self.note_list.delete(0, "end")
            for n in self._notes:
                self.note_list.insert("end", "  " + n.get("title","Untitled"))
            self._note_update_buttons()
        except Exception:
            pass

    def _note_update_buttons(self):
        try:
            enabled = bool(self._notes)
            self.note_del_btn.config(
                fg=self.theme["btn_fg"] if enabled else self.theme["fg2"],
                bg=self.theme["btn_bg"],
                cursor="hand2" if enabled else "arrow",
            )
        except Exception:
            pass

    def _set_note_dirty(self, dirty: bool):
        self._note_dirty = dirty
        try:
            if dirty:
                self.note_save_state_lbl.config(
                    text="● Unsaved",
                    fg=self.theme["warning"],
                    bg=self.theme["bg3"],
                )
            else:
                self.note_save_state_lbl.config(
                    text="✓ Saved",
                    fg=self.theme["success"],
                    bg=self.theme["bg3"],
                )
        except Exception:
            pass

    def _note_select(self, _=None):
        sel = self.note_list.curselection()
        if sel:
            idx = sel[0]
            if idx == getattr(self, "_note_idx", -1):
                return
            if self._note_dirty and self.settings.get("note_auto_save", True):
                self._note_save()
            self._note_snapshot = None
            self._note_load(idx)

    def _cancel_note_timer(self):
        if self._note_timer:
            try:
                self.root.after_cancel(self._note_timer)
            except Exception:
                pass
            self._note_timer = None

    def _note_load(self, idx):
        self._cancel_note_timer()
        if idx < 0 or idx >= len(self._notes):
            self._note_idx = -1
            self.note_title.set("")
            self.note_body.delete("1.0","end")
            self._set_note_dirty(False)
            if hasattr(self, "note_find"):
                self.note_find.run_find()
            return
        self._note_idx = idx
        n = self._notes[idx]
        self.note_title.set(n.get("title",""))
        self.note_body.delete("1.0","end")
        self.note_body.insert("1.0", n.get("body",""))
        self._set_note_dirty(False)
        if hasattr(self, "note_find"):
            self.note_find.run_find()

    def _note_save(self):
        self._cancel_note_timer()
        idx = getattr(self,"_note_idx",-1)
        if idx < 0 or idx >= len(self._notes):
            title = self.note_title.get().strip() or "Untitled"
            body = self.note_body.get("1.0","end-1c")
            self._notes.append({"title": title, "body": body})
            idx = len(self._notes) - 1
            self._note_idx = idx
            self._save_notes()
            self._note_refresh()
            self.note_list.selection_clear(0,"end")
            self.note_list.selection_set(idx)
            self.note_list.see(idx)
            self._set_note_dirty(False)
            self._set_status("Note created", "success")
            return
        self._notes[idx]["title"] = self.note_title.get() or "Untitled"
        self._notes[idx]["body"]  = self.note_body.get("1.0","end-1c")
        self._save_notes(); self._note_refresh()
        self.note_list.selection_set(idx)
        self._set_note_dirty(False)
        self._set_status("Note saved", "success")

    def _toggle_note_auto_save(self):
        state = bool(self.note_auto_var.get()) if hasattr(self, "note_auto_var") else True
        self.settings["note_auto_save"] = state
        if not state and self._note_timer:
            try:
                self.root.after_cancel(self._note_timer)
            except Exception:
                pass
            self._note_timer = None
        if state and self._note_dirty:
            self._note_debounce()
        self._set_status("Note auto save: " + ("ON" if state else "OFF"))
        self.save_fn(self.settings)

    def _flush_note_auto_save(self):
        try:
            if not self.settings.get("note_auto_save", True) or not self._note_dirty:
                return
            idx = getattr(self, "_note_idx", -1)
            if idx < 0 or idx >= len(self._notes):
                return
            self._note_save()
        except Exception:
            pass

    def _note_new(self):
        self._note_snapshot = None
        self._notes.append({"title":"New note","body":""})
        self._note_refresh()
        idx = len(self._notes)-1
        self.note_list.selection_clear(0,"end")
        self.note_list.selection_set(idx)
        self.note_list.see(idx)
        self._note_load(idx)
        self._set_note_dirty(False)

    def _note_del(self):
        self._note_snapshot = None
        idx = getattr(self,"_note_idx",-1)
        if idx < 0 or idx >= len(self._notes): return
        title = self._notes[idx].get("title", "Untitled")
        if not messagebox.askyesno("Delete note", f"Delete note '{title}'?"):
            return
        self._notes.pop(idx); self._save_notes(); self._note_refresh()
        if self._notes:
            ni = min(idx, len(self._notes)-1)
            self.note_list.selection_set(ni); self._note_load(ni)
        else:
            self._note_load(-1)
        self._set_note_dirty(False)

    def _note_debounce(self, _=None):
        self._note_snapshot = None
        self._set_note_dirty(True)
        if not self.settings.get("note_auto_save", True):
            return
        idx = getattr(self, "_note_idx", -1)
        if idx < 0 or idx >= len(self._notes):
            return
        if self._note_timer: self.root.after_cancel(self._note_timer)
        self._note_timer = self.root.after(2000, self._note_save)
    # ──────────────────────────────────────────
    # Zoom
    # ──────────────────────────────────────────
    def _on_zoom(self, event):
        self._font_size = max(FONT_MIN, min(FONT_MAX, self._font_size + (1 if event.delta > 0 else -1)))
        self._font_sizes[self._tab] = self._font_size
        self.settings[f"font_size_{self._tab}"] = self._font_size
        self.settings["font_size"] = self._font_size
        font = ("Consolas", self._font_size)
        for attr in ("src_text","dest_text","diff_left","diff_right","note_body"):
            try: getattr(self, attr).config(font=font)
            except Exception: pass
        try:
            self.diff_editor.set_font_size(self._font_size)
        except Exception:
            pass
        self._set_status(f"Font: {self._font_size}px")
        self._update_status_metrics()
        self._save_settings_debounced()
        return "break"

    # ──────────────────────────────────────────
    # Translation
    # ──────────────────────────────────────────
    def _save_translate_options(self):
        try:
            if hasattr(self, "engine_var"):
                self.settings["engine"] = self.engine_var.get()
            if hasattr(self, "src_lang_var"):
                self.settings["src_lang"] = self.src_lang_var.get()
            if hasattr(self, "dest_lang_var"):
                self.settings["dest_lang"] = self.dest_lang_var.get()
            self.save_fn(self.settings)
        except Exception:
            pass

    def _on_translate_option_select(self):
        self._save_translate_options()
        self._do_translate()

    def _on_src_key(self, _=None):
        try:
            text = self.src_text.get("1.0","end-1c")
            self._update_status_metrics()
            if not getattr(self,"auto_var",None) or not self.auto_var.get(): return
            if self._timer: self.root.after_cancel(self._timer)
            self._timer = self.root.after(700, self._do_translate)
        except Exception: pass

    def _split_translation_units(self, text):
        units = []
        current = []
        for line in text.splitlines(keepends=True):
            if line.strip():
                current.append(line)
            else:
                if current:
                    units.append("".join(current))
                    current = []
                units.append(line)
        if current:
            units.append("".join(current))
        if not units and text:
            units.append(text)
        return units

    def _split_trailing_newlines(self, text):
        idx = len(text)
        while idx > 0 and text[idx - 1] in "\r\n":
            idx -= 1
        return text[:idx], text[idx:]

    def _translate_unit(self, engine, unit, src_code, dest_code):
        if not unit.strip():
            return {
                "source": unit,
                "translated": unit,
                "detected_lang": src_code,
                "chunks": 0,
                "reused": False,
            }
        core, trailing_newlines = self._split_trailing_newlines(unit)
        res = engine.translate(core or unit, src=src_code, dest=dest_code, settings=self.settings)
        return {
            "source": unit,
            "translated": res.get("translated", "") + trailing_newlines,
            "detected_lang": res.get("detected_lang", src_code),
            "chunks": res.get("chunks", 1),
            "reused": False,
        }

    def _translate_incremental(self, text, eng_name, src_code, dest_code):
        signature = (eng_name, src_code, dest_code)
        old_units = self._tran_units if self._tran_signature == signature else []
        old_sources = [self._split_trailing_newlines(unit.get("source", ""))[0] for unit in old_units]
        new_units = self._split_translation_units(text)
        new_sources = [self._split_trailing_newlines(unit)[0] for unit in new_units]
        engine = get_engine(eng_name)
        matcher = difflib.SequenceMatcher(None, old_sources, new_sources, autojunk=False)

        next_units = []
        detected_lang = src_code
        chunks = 0
        reused = 0
        translated_new = 0

        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag == "equal":
                for offset, unit in enumerate(old_units[i1:i2]):
                    source = new_units[j1 + offset]
                    _, trailing_newlines = self._split_trailing_newlines(source)
                    translated_core, _ = self._split_trailing_newlines(unit.get("translated", ""))
                    copied = dict(unit)
                    copied["source"] = source
                    copied["translated"] = translated_core + trailing_newlines
                    copied["reused"] = True
                    next_units.append(copied)
                    reused += 1
                    detected = copied.get("detected_lang")
                    if detected and detected != "auto":
                        detected_lang = detected
                continue

            for source in new_units[j1:j2]:
                unit = self._translate_unit(engine, source, src_code, dest_code)
                next_units.append(unit)
                if source.strip():
                    translated_new += 1
                chunks += unit.get("chunks", 0)
                detected = unit.get("detected_lang")
                if detected and detected != "auto":
                    detected_lang = detected

        return {
            "translated": "".join(unit.get("translated", "") for unit in next_units),
            "detected_lang": detected_lang,
            "source": eng_name,
            "chunks": chunks,
            "units": next_units,
            "reused_units": reused,
            "translated_units": translated_new,
            "total_units": len(next_units),
            "signature": signature,
        }

    def _do_translate(self):
        self._save_translate_options()
        try: text = self.src_text.get("1.0","end-1c")
        except Exception: return
        if not text.strip():
            self._set_dst("")
            self._tran_units = []
            self._tran_signature = None
            return
        if self._busy:
            self._pending_text = text
            return
        self._busy = True
        self._pending_text = None
        self._set_status("Translating…","warning")

        eng  = getattr(self,"engine_var", None)
        eng_name  = eng.get() if eng else _DEFAULT_ENGINE
        src_code  = LANGUAGES.get(getattr(self,"src_lang_var",None) and self.src_lang_var.get(),"auto") if hasattr(self,"src_lang_var") else "auto"
        dest_code = LANGUAGES.get(getattr(self,"dest_lang_var",None) and self.dest_lang_var.get(),"en") if hasattr(self,"dest_lang_var") else "en"

        def worker():
            try:
                res = self._translate_incremental(text, eng_name, src_code, dest_code)
                def done():
                    translated = res.get("translated","")
                    detected   = res.get("detected_lang", src_code)
                    det_name   = LANG_CODE_TO_NAME.get(detected, detected)
                    self._src_cache = text
                    self._dst_cache = translated
                    self._tran_units = res.get("units", [])
                    self._tran_signature = res.get("signature")
                    self._src_snapshot = None
                    self._dst_snapshot = None
                    self._set_dst(translated)
                    try:
                        lbl = f"  Translated  (detected: {det_name})" if src_code=="auto" else "  Translated"
                        self.det_lbl.config(text=lbl)
                    except Exception: pass
                    chunks = res.get("chunks", 1)
                    translated_units = res.get("translated_units", 0)
                    reused_units = res.get("reused_units", 0)
                    suffix_parts = []
                    if chunks and chunks > 1:
                        suffix_parts.append(f"{chunks} chunks")
                    if reused_units:
                        suffix_parts.append(f"{translated_units} new, {reused_units} reused")
                    suffix = f" ({', '.join(suffix_parts)})" if suffix_parts else ""
                    self._set_status(f"Translated via {eng_name}{suffix}","success")
                    self._busy = False
                    if self._pending_text:
                        self._do_translate()
                self.root.after(0, done)
            except TranslationError as e:
                self.root.after(0, lambda: (self._set_status(str(e),"error"), self._free_busy()))
            except Exception as e:
                self.root.after(0, lambda: (self._set_status(f"Error: {e}","error"), self._free_busy()))
        threading.Thread(target=worker, daemon=True).start()

    def _free_busy(self):
        self._busy = False
        if self._pending_text:
            self._do_translate()

    def _set_dst(self, text):
        try:
            self.dest_text.config(state="normal")
            self.dest_text.delete("1.0","end")
            self.dest_text.insert("1.0", text)
            self.dest_text.config(state="disabled")
            self._dst_snapshot = None
            self._update_status_metrics()
            if hasattr(self, "tran_find"):
                self.tran_find.run_find(keep_index=True)
        except Exception: pass

    def _copy_text(self, widget):
        try:
            self.root.clipboard_clear()
            self.root.clipboard_append(widget.get("1.0","end-1c"))
            self._set_status("Copied","success")
        except Exception: pass

    def _clear_src(self):
        try:
            self.src_text.delete("1.0","end")
            self._set_dst("")
            self.det_lbl.config(text="  Translated")
            self._src_cache = self._dst_cache = ""
            self._tran_units = []
            self._tran_signature = None
            self._src_snapshot = self._dst_snapshot = None
            self._set_status("Cleared")
            self._update_status_metrics()
            if hasattr(self, "tran_find"):
                self.tran_find.run_find()
        except Exception: pass

    def _swap_langs(self):
        if not hasattr(self,"src_lang_var"): return
        s, d = self.src_lang_var.get(), self.dest_lang_var.get()
        if s == "Auto Detect":
            self._set_status("Cannot swap when source is Auto Detect","warning"); return
        self.src_lang_var.set(d); self.dest_lang_var.set(s)
        self._save_translate_options()
        try:
            st = self.src_text.get("1.0","end-1c")
            dt = self.dest_text.get("1.0","end-1c")
            self.src_text.delete("1.0","end"); self.src_text.insert("1.0",dt)
            self._set_dst(st)
            self._tran_units = []
            self._tran_signature = None
            self._src_snapshot = self._dst_snapshot = None
        except Exception: pass
        self._do_translate()

    def _set_status(self, msg, kind="normal"):
        try:
            t = self.theme
            colors = {"normal":t["status_fg"],"success":t["success"],"warning":t["warning"],"error":t["error"]}
            self.status_lbl.config(text=f"  {msg}", fg=colors.get(kind, t["status_fg"]))
        except Exception: pass

    def _bind_status_metrics(self, widget, panel: str):
        def update(_=None, w=widget, p=panel):
            self._status_widget = w
            self._status_panel = p
            self._update_status_metrics()
        for event in ("<FocusIn>", "<Enter>", "<KeyRelease>", "<ButtonRelease-1>"):
            widget.bind(event, update, add="+")
        widget.bind("<<Selection>>", update, add="+")

    def _text_metrics(self, widget):
        try:
            text = widget.get("1.0", "end-1c")
        except Exception:
            text = ""
        line = col = 1
        try:
            line_s, col_s = widget.index("insert").split(".")
            line, col = int(line_s), int(col_s) + 1
        except Exception:
            pass
        selected = 0
        try:
            ranges = widget.tag_ranges("sel")
            for i in range(0, len(ranges), 2):
                selected += len(widget.get(ranges[i], ranges[i + 1]))
        except Exception:
            pass
        return len(text), line, col, selected

    def _update_status_metrics(self):
        try:
            if not hasattr(self, "char_lbl"):
                return
            w = self._status_widget
            panel = self._status_panel or self._tab.title()
            if w is None:
                self.char_lbl.config(text="")
                return
            chars, line, col, selected = self._text_metrics(w)
            if self._tab == "diff":
                if hasattr(self, "diff_editor"):
                    left = len(self.diff_editor.get_text("left"))
                    right = len(self.diff_editor.get_text("right"))
                else:
                    left = self._text_metrics(self.diff_left)[0] if hasattr(self, "diff_left") else 0
                    right = self._text_metrics(self.diff_right)[0] if hasattr(self, "diff_right") else 0
                self.char_lbl.config(
                    text=f"Left {left} | Right {right} | {panel}: Ln {line}, Ch {col} | Sel {selected}  ")
            else:
                self.char_lbl.config(
                    text=f"{panel}: {chars} chars | Ln {line}, Ch {col} | Sel {selected}  ")
        except Exception:
            pass

    def _set_paned_sash(self, pane, x: int):
        try:
            y = pane.sash_coord(0)[1]
            pane.sash_place(0, max(100, x), y)
        except Exception:
            pass

    def _set_paned_sash_ratio(self, pane, ratio):
        try:
            width = max(1, pane.winfo_width())
            x = int(width * max(0.1, min(0.9, float(ratio))))
            self._set_paned_sash(pane, x)
        except Exception:
            pass

    def _snapshot_text_widget(self, widget):
        try:
            sel = [(str(ranges[i]), str(ranges[i + 1]))
                   for ranges in [widget.tag_ranges("sel")]
                   for i in range(0, len(ranges), 2)]
            return {
                "text": widget.get("1.0", "end-1c"),
                "insert": str(widget.index("insert")),
                "yview": widget.yview(),
                "xview": widget.xview(),
                "selection": sel,
            }
        except Exception:
            return None

    def _restore_text_widget(self, widget, snapshot, readonly=False):
        if not snapshot:
            return
        try:
            widget.config(state="normal")
            widget.delete("1.0", "end")
            widget.insert("1.0", snapshot.get("text", ""))
            try:
                widget.mark_set("insert", snapshot.get("insert", "1.0"))
            except Exception:
                pass
            widget.tag_remove("sel", "1.0", "end")
            for start, end in snapshot.get("selection", []):
                try:
                    widget.tag_add("sel", start, end)
                except Exception:
                    pass
            try:
                widget.yview_moveto(snapshot.get("yview", (0,))[0])
                widget.xview_moveto(snapshot.get("xview", (0,))[0])
            except Exception:
                pass
            if readonly:
                widget.config(state="disabled")
        except Exception:
            try:
                if readonly:
                    widget.config(state="disabled")
            except Exception:
                pass

    # ──────────────────────────────────────────
    # Toggles
    # ──────────────────────────────────────────
    def _capture_live_tab_state(self):
        """Keep live widget content before rebuilding themed UI."""
        try:
            if hasattr(self, "engine_var"):
                self.settings["engine"] = self.engine_var.get()
            if hasattr(self, "src_lang_var"):
                self.settings["src_lang"] = self.src_lang_var.get()
            if hasattr(self, "dest_lang_var"):
                self.settings["dest_lang"] = self.dest_lang_var.get()
        except Exception:
            pass
        try:
            if hasattr(self, "src_text"):
                self._src_snapshot = self._snapshot_text_widget(self.src_text)
                self._src_cache = self.src_text.get("1.0", "end-1c")
        except Exception:
            pass
        try:
            if hasattr(self, "dest_text"):
                self._dst_snapshot = self._snapshot_text_widget(self.dest_text)
                self._dst_cache = self.dest_text.get("1.0", "end-1c")
        except Exception:
            pass
        try:
            if hasattr(self, "diff_editor"):
                self._diff_left_cache = self.diff_editor.get_text("left")
                self._diff_right_cache = self.diff_editor.get_text("right")
            elif hasattr(self, "diff_left") and hasattr(self, "diff_right"):
                self._diff_left_cache = self.diff_left.get("1.0", "end-1c")
                self._diff_right_cache = self.diff_right.get("1.0", "end-1c")
        except Exception:
            pass
        try:
            if hasattr(self, "note_body"):
                self._note_dirty_cache = self._note_dirty
                self._note_snapshot = self._snapshot_text_widget(self.note_body)
                self._note_body_cache = self.note_body.get("1.0", "end-1c")
                self._note_title_cache = self.note_title.get() if hasattr(self, "note_title") else None
                idx = getattr(self, "_note_idx", -1)
                if 0 <= idx < len(self._notes):
                    self._notes[idx]["title"] = self._note_title_cache or "Untitled"
                    self._notes[idx]["body"] = self._note_body_cache
        except Exception:
            pass

    def _toggle_theme(self):
        self._capture_live_tab_state()
        name = "light" if self.theme["name"]=="dark" else "dark"
        self.theme = THEMES[name]; self.settings["theme"] = name
        self._apply_ttk_style()
        for w in self.root.winfo_children(): w.destroy()
        self._build_ui()
        self.save_fn(self.settings)

    def _toggle_ontop(self):
        state = not self.settings.get("always_on_top", False)
        self.settings["always_on_top"] = state
        self.root.attributes("-topmost", state)
        try: self.ontop_btn.set_toggled(state)
        except Exception: pass
        self._set_status("Always on top: " + ("ON" if state else "OFF"))
        self.save_fn(self.settings)

    def _toggle_compact(self):
        self._capture_live_tab_state()
        self._compact = not self._compact
        self.settings["compact_mode"] = self._compact
        s = self.settings
        if self._compact:
            # Size theo tab: tran ngắn, diff cao hơn để thấy 4 ô
            if self._tab == "diff":
                w = s.get("compact_width", 500)
                h = s.get("compact_diff_height", 420)
            else:
                w = s.get("compact_width", 500)
                h = s.get("compact_height", 200)
            self.root.geometry(f"{w}x{h}")
            self.root.minsize(280, 120)
        else:
            self.root.geometry(f"{s.get('window_width',980)}x{s.get('window_height',640)}")
            self.root.minsize(340, 120)
        self._apply_bar_visibility()
        if self._compact:
            self._build_compact_bar()
        self._discard_tab_frame()
        # Rebuild tab content để ẩn/hiện inline controls
        self._switch_tab(self._tab, force=True)
        self.save_fn(self.settings)

    def _toggle_layout(self):
        self._capture_live_tab_state()
        self._layout = "vertical" if self._layout=="horizontal" else "horizontal"
        self.settings["layout"] = self._layout
        try: self.layout_btn.config(text="⊟" if self._layout=="horizontal" else "⊞")
        except Exception: pass
        self._discard_tab_frame("tran")
        if self._tab == "tran":
            self._switch_tab("tran", force=True)
        self.save_fn(self.settings)

    def _toggle_wrap(self):
        self._capture_live_tab_state()
        self._word_wrap = not self._word_wrap
        self.settings["word_wrap"] = self._word_wrap
        try: self.wrap_btn.set_toggled(self._word_wrap)
        except Exception: pass
        self._discard_tab_frame()
        self._switch_tab(self._tab, force=True)
        self.save_fn(self.settings)

    # ──────────────────────────────────────────
    # TTK styling
    # ──────────────────────────────────────────
    def _cycle_window_effect(self):
        keys = [key for key, _, _, _ in WINDOW_EFFECTS]
        try:
            idx = keys.index(self._window_effect)
        except ValueError:
            idx = 0
        self._window_effect = keys[(idx + 1) % len(keys)]
        self.settings["window_effect"] = self._window_effect
        self._apply_window_effect()

        label = next((label for key, label, _, _ in WINDOW_EFFECTS
                      if key == self._window_effect), "◎ Solid")
        try:
            self.opacity_btn.config(text=label)
        except Exception:
            pass
        self._set_status(f"Window effect: {label.split(' ', 1)[-1]}")
        self.save_fn(self.settings)

    def _apply_window_effect(self):
        effect = next((item for item in WINDOW_EFFECTS
                       if item[0] == self._window_effect), WINDOW_EFFECTS[0])
        _, _, alpha, use_blur = effect
        try:
            self.root.attributes("-alpha", alpha)
        except Exception:
            pass
        self._set_windows_blur(use_blur)

    def _set_windows_blur(self, enabled: bool):
        if sys.platform != "win32":
            return False
        try:
            hwnd = self.root.winfo_id()

            class ACCENTPOLICY(ctypes.Structure):
                _fields_ = [
                    ("AccentState", ctypes.c_int),
                    ("AccentFlags", ctypes.c_int),
                    ("GradientColor", ctypes.c_int),
                    ("AnimationId", ctypes.c_int),
                ]

            class WINDOWCOMPOSITIONATTRIBDATA(ctypes.Structure):
                _fields_ = [
                    ("Attribute", ctypes.c_int),
                    ("Data", ctypes.c_void_p),
                    ("SizeOfData", ctypes.c_size_t),
                ]

            accent = ACCENTPOLICY()
            accent.AccentState = 3 if enabled else 0
            accent.AccentFlags = 2 if enabled else 0
            accent.GradientColor = 0xCC000000 if enabled else 0
            data = WINDOWCOMPOSITIONATTRIBDATA()
            data.Attribute = 19
            data.Data = ctypes.cast(ctypes.pointer(accent), ctypes.c_void_p)
            data.SizeOfData = ctypes.sizeof(accent)
            fn = ctypes.windll.user32.SetWindowCompositionAttribute
            fn.argtypes = [ctypes.c_void_p, ctypes.POINTER(WINDOWCOMPOSITIONATTRIBDATA)]
            fn.restype = ctypes.c_int
            return bool(fn(ctypes.c_void_p(hwnd), ctypes.byref(data)))
        except Exception:
            return False

    def _apply_ttk_style(self):
        t = self.theme
        style = ttk.Style()
        try: style.theme_use("clam")
        except Exception: pass

        # Combobox
        style.configure("TCombobox",
            fieldbackground=t["combo_bg"], background=t["bg3"],
            foreground=t["combo_fg"], selectbackground=t["accent"],
            selectforeground=t["bg"], borderwidth=0, relief="flat",
            arrowcolor=t["fg2"], padding=(4,2))
        style.map("TCombobox",
            fieldbackground=[("readonly",t["combo_bg"]),("!readonly",t["combo_bg"])],
            foreground=[("readonly",t["combo_fg"]),("focus",t["fg"])],
            background=[("readonly",t["bg3"]),("active",t["bg3"])],
            bordercolor=[("focus",t["accent"]),("!focus",t["border"])])
        self.root.option_add("*TCombobox*Listbox.background",        t["bg3"])
        self.root.option_add("*TCombobox*Listbox.foreground",        t["combo_fg"])
        self.root.option_add("*TCombobox*Listbox.selectBackground",  t["accent"])
        self.root.option_add("*TCombobox*Listbox.selectForeground",  t["bg"])
        self.root.option_add("*TCombobox*Listbox.font",              t["font_ui"])

    # ──────────────────────────────────────────
    # Window events
    # ──────────────────────────────────────────
    def _on_resize(self, event):
        if event.widget == self.root and not self._compact:
            self.settings["window_width"]  = self.root.winfo_width()
            self.settings["window_height"] = self.root.winfo_height()
            self._save_settings_debounced(delay_ms=500)
