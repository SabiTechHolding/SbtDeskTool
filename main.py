"""
SbtDeskTran - Entry point
Desktop translation app for Windows 11
No extra dependencies required (uses Python standard library only).

Usage:
    python main.py
"""
import json
import os
import sys
import tkinter as tk

from app_paths import data_file, data_file_candidates


SETTINGS_FILE = data_file("settings.json")

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
    "diff_word_diff": True,
    "diff_ignore_whitespace": False,
    "note_auto_save": True,
    "word_wrap": True,
    "window_effect": "blur",
    "window_width": 980,
    "window_height": 640,
    "compact_width": 500,
    "compact_height": 200,
    "compact_diff_height": 420,
}


def load_settings() -> dict:
    for path in data_file_candidates("settings.json"):
        if not os.path.exists(path):
            continue
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            settings = DEFAULT_SETTINGS.copy()
            settings.update(data)
            if os.path.normcase(os.path.abspath(path)) != os.path.normcase(os.path.abspath(SETTINGS_FILE)):
                save_settings(settings)
            return settings
        except Exception:
            pass

    settings = DEFAULT_SETTINGS.copy()
    save_settings(settings)
    return settings


def save_settings(settings: dict):
    try:
        with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
            json.dump(settings, f, indent=2, ensure_ascii=False)
    except Exception:
        pass


def main():
    settings = load_settings()

    from logger import log
    log.info("SbtDeskTran starting")
    log.debug(f"Settings: {settings}")
    log.debug(f"Settings file: {SETTINGS_FILE}")

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

    root.after(1500, app.check_for_updates)

    root.mainloop()


if __name__ == "__main__":
    main()
