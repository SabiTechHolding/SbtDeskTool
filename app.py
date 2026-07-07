"""
SbtDeskTran - Main Application Window
Layout: TopBar (tabs left, modes right) -> LangBar (tran only) -> Content -> StatusBar
"""
import tkinter as tk
from tkinter import ttk, messagebox
import threading, json, os, sys, ctypes

from translator_engine import (
    LANGUAGES, LANG_CODE_TO_NAME, get_engine, ENGINES, TranslationError
)
from theme import THEMES
from widgets import FlatButton, DiffViewer, themed_scrollbar
from logger import log

LANG_NAMES      = list(LANGUAGES.keys())
# Resolve data directory: exe folder when frozen, script folder otherwise
_APP_DIR        = os.path.dirname(os.path.abspath(sys.executable if getattr(sys, "frozen", False) else __file__))
NOTES_FILE      = os.path.join(_APP_DIR, "notes.json")
_DEFAULT_ENGINE = list(ENGINES.keys())[0]
FONT_MIN, FONT_MAX = 7, 28
WINDOW_EFFECTS = [
    ("solid", "◎ Solid", 1.00, False),
    ("blur", "◍ Blur", 0.98, True),
    ("frosted", "◐ Frosted", 0.92, True),
    ("transp", "○ Transparent", 0.85, False),
    ("dim", "◑ Dim", 0.88, False),
    ("ghost", "◌ Ghost", 0.80, False),
    ("clear", "□ Clear", 0.45, False),
]


# ─────────────────────────────────────────────
# FilterableCombobox
# ─────────────────────────────────────────────
class FilterableCombobox(ttk.Combobox):
    def __init__(self, parent, all_values, on_select=None, **kwargs):
        self._all = list(all_values)
        self._on_select = on_select
        super().__init__(parent, values=self._all, **kwargs)
        self.bind("<KeyRelease>",         self._filter)
        self.bind("<FocusOut>",           self._restore)
        self.bind("<<ComboboxSelected>>", self._selected)

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

    def _selected(self, _=None):
        self["values"] = self._all
        if self._on_select:
            self._on_select()

    def set_all(self, values):
        self._all = list(values)
        self["values"] = self._all

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
        self._font_size = settings.get("font_size", 10)
        self._word_wrap = settings.get("word_wrap", True)
        self._window_effect = settings.get("window_effect", settings.get("opacity_mode", "blur"))
        if self._window_effect not in [effect[0] for effect in WINDOW_EFFECTS]:
            self._window_effect = "blur"
        self._timer     = None
        self._busy      = False
        self._notes     = self._load_notes()
        self._note_idx  = -1
        self._note_timer = None
        self._src_cache  = ""
        self._dst_cache  = ""
        # diff tab preserved text
        self._diff_left_cache  = ""
        self._diff_right_cache = ""
        self._note_body_cache  = ""

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
            on_select=self._do_translate,
            textvariable=self.engine_var, width=15,
            font=t["font_ui"]).pack(side="left", padx=(0,8))

        tk.Label(inner, text="From:", bg=t["bg2"], fg=t["fg2"],
                 font=t["font_small"]).pack(side="left", padx=(0,2))
        self.src_lang_var = tk.StringVar(value=self.settings.get("src_lang","Auto Detect"))
        FilterableCombobox(inner, LANG_NAMES,
            on_select=self._do_translate,
            textvariable=self.src_lang_var, width=17,
            font=t["font_ui"]).pack(side="left", padx=(0,2))

        FlatButton(inner, text="⇄", theme=t,
            command=self._swap_langs).pack(side="left", padx=3)

        tk.Label(inner, text="To:", bg=t["bg2"], fg=t["fg2"],
                 font=t["font_small"]).pack(side="left", padx=(0,2))
        dest_langs = [l for l in LANG_NAMES if l != "Auto Detect"]
        self.dest_lang_var = tk.StringVar(value=self.settings.get("dest_lang","English"))
        FilterableCombobox(inner, dest_langs,
            on_select=self._do_translate,
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
            on_select=self._do_translate,
            textvariable=self.src_lang_var, width=12,
            font=t["font_small"]).pack(side="left", padx=(6,2), pady=4)
        tk.Label(self.compact_bar, text="→", bg=t["bg2"],
                 fg=t["fg2"], font=t["font_ui"]).pack(side="left")
        FilterableCombobox(self.compact_bar, dest_langs,
            on_select=self._do_translate,
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
    def _switch_tab(self, tab_id: str, force: bool = False):
        if tab_id == self._tab and not force:
            return
        # Save text from current tab before switching
        if self._tab == "diff" and not force:
            try:
                self._diff_left_cache  = self.diff_left.get("1.0","end-1c")
                self._diff_right_cache = self.diff_right.get("1.0","end-1c")
            except Exception:
                pass
        if self._tab == "note" and not force:
            try:
                self._note_body_cache = self.note_body.get("1.0","end-1c")
            except Exception:
                pass

        self._tab = tab_id
        self.settings["active_tab"] = tab_id
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

        # Rebuild content area
        for w in self.content.winfo_children():
            w.destroy()
        {"tran": self._build_tran_tab,
         "diff": self._build_diff_tab,
         "note": self._build_note_tab}.get(tab_id, self._build_tran_tab)()

    # ──────────────────────────────────────────
    # TAB: Translate
    # ──────────────────────────────────────────
    def _build_tran_tab(self):
        t = self.theme

        # ── Split panes ──────────────────────────────────────────────────────
        orient = tk.HORIZONTAL if self._layout == "horizontal" else tk.VERTICAL
        pw = tk.PanedWindow(self.content, orient=orient,
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
            sc_x.pack(side="bottom", fill="x")
        self.src_text.pack(fill="both", expand=True)
        self.src_text.bind("<KeyRelease>",         self._on_src_key)
        self.src_text.bind("<Control-Return>",     lambda e: self._do_translate())
        self.src_text.bind("<Control-MouseWheel>", self._on_zoom)
        if self._src_cache:
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
            ds_x.pack(side="bottom", fill="x")
        self.dest_text.pack(fill="both", expand=True)
        self.dest_text.bind("<Control-MouseWheel>", self._on_zoom)
        if self._dst_cache:
            self.dest_text.config(state="normal")
            self.dest_text.insert("1.0", self._dst_cache)
            self.dest_text.config(state="disabled")

    # ──────────────────────────────────────────
    # TAB: Diff (two independent free-text boxes)
    # ──────────────────────────────────────────
    def _build_diff_tab(self):
        t = self.theme

        # Top section: two input panes side by side (fixed height via PanedWindow)
        pw_outer = tk.PanedWindow(self.content, orient=tk.HORIZONTAL,
            bg=t["border"], sashwidth=5, sashrelief="flat", bd=0, handlesize=0)
        pw_outer.pack(fill="both", expand=True, side="top")

        saved_auto = self.settings.get("diff_auto", True)
        self.diff_auto_var = tk.BooleanVar(value=saved_auto)

        for side, lbl, cache in (
            ("left",  "◀  Left",  self._diff_left_cache),
            ("right", "▶  Right", self._diff_right_cache),
        ):
            fr = tk.Frame(pw_outer, bg=t["bg"])
            pw_outer.add(fr, stretch="always", minsize=100)
            hdr = tk.Frame(fr, bg=t["bg3"], height=28)
            hdr.pack(fill="x"); hdr.pack_propagate(False)
            tk.Label(hdr, text=f"  {lbl}", bg=t["bg3"], fg=t["fg2"],
                font=t["font_ui_bold"], anchor="w").pack(side="left", fill="y")
            FlatButton(hdr, text="✕ Clear", theme=t,
                command=lambda s=side: getattr(self, f"diff_{s}").delete("1.0","end")).pack(side="right", padx=4)
            FlatButton(hdr, text="⎘ Copy", theme=t,
                command=lambda s=side: self._copy_text(getattr(self, f"diff_{s}"))).pack(side="right", padx=2)
            # Run Diff + Auto — chỉ ở Right header, ẩn khi compact
            if side == "right" and not self._compact:
                def _on_auto_toggle():
                    self.settings["diff_auto"] = self.diff_auto_var.get()
                    self.save_fn(self.settings)
                    if self.diff_auto_var.get(): self._run_diff()
                tk.Checkbutton(hdr, text="Auto", variable=self.diff_auto_var,
                    command=_on_auto_toggle,
                    bg=t["bg3"], fg=t["fg2"], selectcolor=t["bg3"],
                    activebackground=t["bg3"], activeforeground=t["fg"],
                    font=t["font_small"]).pack(side="right", padx=2)
                FlatButton(hdr, text="▶ Run Diff", theme=t,
                    command=self._run_diff).pack(side="right", padx=4)
            tf = tk.Frame(fr, bg=t["bg"]); tf.pack(fill="both", expand=True)
            wrap_mode = "word" if self._word_wrap else "none"
            txt = tk.Text(tf, bg=t["bg"], fg=t["fg"],
                font=("Consolas", self._font_size), wrap=wrap_mode,
                relief="flat", bd=0, padx=8, pady=6, height=7,
                insertbackground=t["fg"], selectbackground=t["selection"], undo=True)
            sc = themed_scrollbar(tf, t, command=txt.yview)
            txt.config(yscrollcommand=sc.set)
            sc.pack(side="right", fill="y")
            if not self._word_wrap:
                # Pack horizontal scrollbar BEFORE txt so it's not pushed out
                sc_x = themed_scrollbar(fr, t, orient="horizontal", command=txt.xview)
                txt.config(xscrollcommand=sc_x.set)
                sc_x.pack(side="bottom", fill="x", before=tf)
            txt.pack(fill="both", expand=True)
            txt.bind("<Control-MouseWheel>", self._on_zoom)
            if cache:
                txt.insert("1.0", cache)
            setattr(self, f"diff_{side}", txt)

        def _key_handler(e):
            if self.diff_auto_var.get():
                if hasattr(self, "_diff_key_timer") and self._diff_key_timer:
                    self.root.after_cancel(self._diff_key_timer)
                self._diff_key_timer = self.root.after(500, self._run_diff)
        self.diff_left.bind("<KeyRelease>",  _key_handler)
        self.diff_right.bind("<KeyRelease>", _key_handler)

        # Diff viewer
        self.diff_viewer = DiffViewer(self.content, theme=t,
            word_wrap=self._word_wrap, font_size=self._font_size,
            on_zoom=self._on_zoom, bg=t["bg2"])
        self.diff_viewer.pack(fill="both", expand=True)

    def _run_diff(self):
        try:
            left  = self.diff_left.get("1.0","end-1c")
            right = self.diff_right.get("1.0","end-1c")
            self.diff_viewer.render(left, right)
            # Show stats in statusbar instead of diff header
            from diff_engine import get_diff_stats, compute_line_diff
            stats = get_diff_stats(compute_line_diff(left, right))
            self._set_status(
                f"+{stats['added']} added   −{stats['removed']} removed   "
                f"{stats['changed_blocks']} block(s) changed")
        except Exception as e:
            self._set_status(f"Diff error: {e}", "error")
    # ──────────────────────────────────────────
    # TAB: Notes
    # ──────────────────────────────────────────
    def _build_note_tab(self):
        t = self.theme
        outer = tk.PanedWindow(self.content, orient=tk.HORIZONTAL,
            bg=t["border"], sashwidth=5, sashrelief="flat", bd=0, handlesize=0)
        outer.pack(fill="both", expand=True)

        # Editor — bên trái, chiếm phần lớn
        ef = tk.Frame(outer, bg=t["bg"])
        outer.add(ef, stretch="always", minsize=200)

        # Editor header: title entry + Save
        eh = tk.Frame(ef, bg=t["bg3"], height=28); eh.pack(fill="x"); eh.pack_propagate(False)
        self.note_title = tk.StringVar()
        tk.Entry(eh, textvariable=self.note_title, bg=t["bg3"], fg=t["fg"],
            font=t["font_ui_bold"], relief="flat", bd=0,
            insertbackground=t["fg"]).pack(side="left", fill="both", expand=True, padx=8, pady=4)
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
        FlatButton(sh, text="💾", theme=t, command=self._note_save).pack(side="left", padx=2)
        FlatButton(sh, text="🗑", theme=t, command=self._note_del).pack(side="left", padx=2)

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
        if self._notes:
            idx = max(0, min(self._note_idx, len(self._notes)-1))
            self.note_list.selection_set(idx)
            self._note_load(idx)
        # After note_load, restore unsaved body if toggling wrap
        if _restore_body:
            self.note_body.delete("1.0", "end")
            self.note_body.insert("1.0", _restore_body)
            self._note_body_cache = ""

    def _load_notes(self):
        if not os.path.exists(NOTES_FILE):
            # First run — create empty notes file
            try:
                with open(NOTES_FILE, "w", encoding="utf-8") as f:
                    json.dump([], f)
            except Exception:
                pass
            return []
        try:
            with open(NOTES_FILE, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
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
        except Exception:
            pass

    def _note_select(self, _=None):
        sel = self.note_list.curselection()
        if sel: self._note_load(sel[0])

    def _note_load(self, idx):
        self._note_idx = idx
        n = self._notes[idx]
        self.note_title.set(n.get("title",""))
        self.note_body.delete("1.0","end")
        self.note_body.insert("1.0", n.get("body",""))

    def _note_save(self):
        idx = getattr(self,"_note_idx",-1)
        if idx < 0 or idx >= len(self._notes): return
        self._notes[idx]["title"] = self.note_title.get() or "Untitled"
        self._notes[idx]["body"]  = self.note_body.get("1.0","end-1c")
        self._save_notes(); self._note_refresh()
        self.note_list.selection_set(idx)
        self._set_status("Note saved", "success")

    def _note_new(self):
        self._notes.append({"title":"New note","body":""})
        self._note_refresh()
        idx = len(self._notes)-1
        self.note_list.selection_clear(0,"end")
        self.note_list.selection_set(idx)
        self.note_list.see(idx)
        self._note_load(idx)

    def _note_del(self):
        idx = getattr(self,"_note_idx",-1)
        if idx < 0 or idx >= len(self._notes): return
        if len(self._notes) == 1:
            self._notes = [{"title":"My first note","body":""}]
            self._save_notes(); self._note_refresh(); self._note_load(0); return
        self._notes.pop(idx); self._save_notes(); self._note_refresh()
        ni = min(idx, len(self._notes)-1)
        self.note_list.selection_set(ni); self._note_load(ni)

    def _note_debounce(self, _=None):
        if self._note_timer: self.root.after_cancel(self._note_timer)
        self._note_timer = self.root.after(2000, self._note_save)
    # ──────────────────────────────────────────
    # Zoom
    # ──────────────────────────────────────────
    def _on_zoom(self, event):
        self._font_size = max(FONT_MIN, min(FONT_MAX, self._font_size + (1 if event.delta > 0 else -1)))
        self.settings["font_size"] = self._font_size
        font = ("Consolas", self._font_size)
        for attr in ("src_text","dest_text","diff_left","diff_right","note_body"):
            try: getattr(self, attr).config(font=font)
            except Exception: pass
        try: self.diff_viewer.set_font_size(self._font_size)
        except Exception: pass
        self._set_status(f"Font: {self._font_size}px")
        self._save_settings_debounced()
        return "break"

    # ──────────────────────────────────────────
    # Translation
    # ──────────────────────────────────────────
    def _on_src_key(self, _=None):
        try:
            text = self.src_text.get("1.0","end-1c")
            self.char_lbl.config(text=f"{len(text)} chars  ")
            if not getattr(self,"auto_var",None) or not self.auto_var.get(): return
            if self._timer: self.root.after_cancel(self._timer)
            self._timer = self.root.after(700, self._do_translate)
        except Exception: pass

    def _do_translate(self):
        try: text = self.src_text.get("1.0","end-1c")
        except Exception: return
        if not text.strip():
            self._set_dst(""); return
        if self._busy: return
        self._busy = True
        self._set_status("Translating…","warning")

        eng  = getattr(self,"engine_var", None)
        eng_name  = eng.get() if eng else _DEFAULT_ENGINE
        src_code  = LANGUAGES.get(getattr(self,"src_lang_var",None) and self.src_lang_var.get(),"auto") if hasattr(self,"src_lang_var") else "auto"
        dest_code = LANGUAGES.get(getattr(self,"dest_lang_var",None) and self.dest_lang_var.get(),"en") if hasattr(self,"dest_lang_var") else "en"

        def worker():
            try:
                res = get_engine(eng_name).translate(text, src=src_code, dest=dest_code)
                def done():
                    translated = res.get("translated","")
                    detected   = res.get("detected_lang", src_code)
                    det_name   = LANG_CODE_TO_NAME.get(detected, detected)
                    self._src_cache = text
                    self._dst_cache = translated
                    self._set_dst(translated)
                    try:
                        lbl = f"  Translated  (detected: {det_name})" if src_code=="auto" else "  Translated"
                        self.det_lbl.config(text=lbl)
                    except Exception: pass
                    self._set_status(f"Translated via {eng_name}","success")
                    self._busy = False
                self.root.after(0, done)
            except TranslationError as e:
                self.root.after(0, lambda: (self._set_status(str(e),"error"), setattr(self,"_busy",False)))
            except Exception as e:
                self.root.after(0, lambda: (self._set_status(f"Error: {e}","error"), setattr(self,"_busy",False)))
        threading.Thread(target=worker, daemon=True).start()

    def _set_dst(self, text):
        try:
            self.dest_text.config(state="normal")
            self.dest_text.delete("1.0","end")
            self.dest_text.insert("1.0", text)
            self.dest_text.config(state="disabled")
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
            self.char_lbl.config(text="")
            self._src_cache = self._dst_cache = ""
            self._set_status("Cleared")
        except Exception: pass

    def _swap_langs(self):
        if not hasattr(self,"src_lang_var"): return
        s, d = self.src_lang_var.get(), self.dest_lang_var.get()
        if s == "Auto Detect":
            self._set_status("Cannot swap when source is Auto Detect","warning"); return
        self.src_lang_var.set(d); self.dest_lang_var.set(s)
        try:
            st = self.src_text.get("1.0","end-1c")
            dt = self.dest_text.get("1.0","end-1c")
            self.src_text.delete("1.0","end"); self.src_text.insert("1.0",dt)
            self._set_dst(st)
        except Exception: pass
        self._do_translate()

    def _set_status(self, msg, kind="normal"):
        try:
            t = self.theme
            colors = {"normal":t["status_fg"],"success":t["success"],"warning":t["warning"],"error":t["error"]}
            self.status_lbl.config(text=f"  {msg}", fg=colors.get(kind, t["status_fg"]))
        except Exception: pass

    # ──────────────────────────────────────────
    # Toggles
    # ──────────────────────────────────────────
    def _toggle_theme(self):
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
        # Rebuild tab content để ẩn/hiện inline controls
        self._switch_tab(self._tab, force=True)
        self.save_fn(self.settings)

    def _toggle_layout(self):
        self._layout = "vertical" if self._layout=="horizontal" else "horizontal"
        self.settings["layout"] = self._layout
        try: self.layout_btn.config(text="⊟" if self._layout=="horizontal" else "⊞")
        except Exception: pass
        if self._tab == "tran":
            self._switch_tab("tran", force=True)
        self.save_fn(self.settings)

    def _toggle_wrap(self):
        # 1. Save all current text before rebuilding
        try:
            self._src_cache = self.src_text.get("1.0", "end-1c")
        except Exception:
            pass
        try:
            self.dest_text.config(state="normal")
            self._dst_cache = self.dest_text.get("1.0", "end-1c")
            self.dest_text.config(state="disabled")
        except Exception:
            pass
        try:
            self._diff_left_cache  = self.diff_left.get("1.0", "end-1c")
            self._diff_right_cache = self.diff_right.get("1.0", "end-1c")
        except Exception:
            pass
        try:
            self._note_body_cache = self.note_body.get("1.0", "end-1c")
        except Exception:
            pass

        self._word_wrap = not self._word_wrap
        self.settings["word_wrap"] = self._word_wrap
        try: self.wrap_btn.set_toggled(self._word_wrap)
        except Exception: pass
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
