"""
SbtDeskTran - Entry point
Desktop translation app for Windows 11
No extra dependencies required (uses Python standard library only).

Usage:
    python main.py
"""
import tkinter as tk
import sys
import os
import json

# ──────────────────────────────────────────────
# Resolve the directory where user-data files live.
# - When frozen by PyInstaller (--onefile):  directory of the .exe
# - When run as plain Python:                directory of this .py file
# ──────────────────────────────────────────────
def _app_dir() -> str:
    if getattr(sys, "frozen", False):
        # Running inside a PyInstaller bundle → use exe location
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(os.path.abspath(__file__))

APP_DIR       = _app_dir()
SETTINGS_FILE = os.path.join(APP_DIR, "settings.json")

DEFAULT_SETTINGS = {
    "theme": "dark",
    "layout": "horizontal",
    "always_on_top": False,
    "compact_mode": False,
    "engine": "",
    "src_lang": "Auto Detect",
    "dest_lang": "English",
    "font_size": 10,
    "active_tab": "tran",
    "tran_auto": True,
    "diff_auto": True,
    "word_wrap": True,
    "window_effect": "blur",
    "window_width": 980,
    "window_height": 640,
    "compact_width": 500,
    "compact_height": 200,
    "compact_diff_height": 420,
}


def load_settings() -> dict:
    if not os.path.exists(SETTINGS_FILE):
        # First run — create file with defaults right away
        s = DEFAULT_SETTINGS.copy()
        save_settings(s)
        return s
    try:
        with open(SETTINGS_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
        s = DEFAULT_SETTINGS.copy()
        s.update(data)
        return s
    except Exception:
        return DEFAULT_SETTINGS.copy()


def save_settings(settings: dict):
    try:
        with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
            json.dump(settings, f, indent=2, ensure_ascii=False)
    except Exception:
        pass


# ──────────────────────────────────────────────
# Entry point
# ──────────────────────────────────────────────
def main():
    settings = load_settings()

    from logger import log
    log.info("SbtDeskTran starting")
    log.debug(f"Settings: {settings}")

    from version import __version__
    root = tk.Tk()
    root.title(f"SbtDeskTran v{__version__}")

    # High-DPI awareness on Windows
    try:
        from ctypes import windll
        windll.shcore.SetProcessDpiAwareness(1)
    except Exception:
        pass

    # Bring window to front on start
    root.lift()
    root.focus_force()

    from app import SbtDeskTranApp
    app = SbtDeskTranApp(root, settings, save_settings)

    root.mainloop()


if __name__ == "__main__":
    main()
