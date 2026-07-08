# SbtDeskTran

Desktop translation app for Windows 11. No installation required when using the standalone build.

## Requirements

- Python 3.8+ for source runs
- No runtime Python packages beyond the standard library

## Run From Source

```bat
python main.py
```

or double-click:

```bat
SbtDeskTran.bat
```

## Build Standalone EXE

Double-click `build.bat`, or run:

```bat
build.bat
```

The build script installs/checks PyInstaller, generates `icon.ico` if needed, and writes:

```text
dist\SbtDeskTran.exe
```

CI/CD creates the update assets after the exe is built. `SbtDeskTran-<version>.zip`
is the auto-update package and contains only `SbtDeskTran.exe`. Publish it as a
GitHub release asset together with `version_changes.txt`.

## Auto Update

Auto-update runs only in the Windows standalone exe build. It checks the latest
public GitHub release from:

```text
https://api.github.com/repos/SabiTechHolding/SbtDeskTran/releases/latest
```

The latest release tag, for example `v2026.07.07.8`, is compared with the
current app version. If the release is newer, the app downloads the release
asset named like `SbtDeskTran-2026.07.07.8.zip`, or a direct `SbtDeskTran.exe`.

When a newer version is found, the app downloads the package, extracts only
`SbtDeskTran.exe`, closes, replaces the exe in the running app folder, and
starts again. Version change text is read from `version_changes.txt` instead
of being stored in a separate update file.

For CI/CD releases, stamp the release tag before PyInstaller builds, then
create release assets after `dist\SbtDeskTran.exe` exists:

```bat
python ci_release.py set-version %GITHUB_REF_NAME%
python ci_release.py ensure-changes %GITHUB_REF_NAME% --commit --push
python -m PyInstaller ...
python ci_release.py assets %GITHUB_REF_NAME%
```

Tags like `v2026.07.07.8` are accepted; `version.py` uses `2026.07.07.8`.
If CI passes release text such as
`[v2026.07.07.8](https://github.com/.../releases/tag/v2026.07.07.8)`,
the release helper extracts the first numeric tag automatically.

If `version_changes.txt` has not changed since the previous release tag, the
release helper appends an auto-generated section from commits and changed files
since that tag, then optionally commits and pushes it.

Use a full checkout in CI, including tags, so the helper can find the previous
release tag. For GitHub Actions, set `fetch-depth: 0` on `actions/checkout`.

## Features

| Feature | Details |
| --- | --- |
| Translation | Google Translate with auto language detection and corporate proxy/PAC fallback |
| Translate layout | Horizontal or vertical split; horizontal scrollbars appear when Wrap is off |
| Diff mode | Resizable top/bottom panes, synced left/right pane width, line and word-level highlighting |
| Diff scrolling | Synced vertical scrolling and synced horizontal scrolling for left/right diff panes |
| Notes | Local notes with optional Auto Save, saved/unsaved indicator, empty note list support, delete confirmation |
| Status bar | Per-panel total chars, current line/character, and selected character count |
| Font zoom | `Ctrl+MouseWheel` zoom, remembered separately for Translate, Diff, and Notes |
| Window effects | Solid, Blur, Frosted, Transparent, Dim, Ghost, and Clear modes |
| Compact mode | Small minimal window for quick translation |
| Always on top | Pin the window above other windows |
| Themes | Dark and Light themes |
| Shortcuts | `Ctrl+Enter` translates immediately; `Ctrl+MouseWheel` changes font size |

## Runtime Data

`settings.json`, `notes.json`, and `app.log` are written to the first writable app data directory.

When the app is launched from a UNC/network share, it prefers local user data directories before falling back to the executable folder. Set `SBTDESKTRAN_DATA_DIR` to force a specific runtime data directory.

## Translation Networking

The Google engine tries multiple routes:

- urllib using system/configured proxy
- PowerShell/Windows networking using default proxy/PAC credentials
- urllib direct
- SSL-off retries for corporate TLS inspection edge cases
- fallback Google Translate endpoint

Logs include the attempted route, proxy mode, and SSL mode.

## Adding More Translation Engines

Edit `translator_engine.py`, add a class with a `translate()` method, and register it in `ENGINES`:

```python
class MyEngine:
    name = "My Engine"

    def translate(self, text, src="auto", dest="en") -> dict:
        return {"translated": "...", "detected_lang": "en", "source": self.name}


ENGINES["My Engine"] = MyEngine()
```

## File Structure

```text
translator/
  main.py              - Entry point
  app.py               - Main window and UI logic
  widgets.py           - FlatButton and DiffViewer widgets
  translator_engine.py - Translation backends
  diff_engine.py       - Line/word-level diff algorithm
  theme.py             - Dark and Light theme definitions
  app_paths.py         - Writable runtime data path resolution
  logger.py            - Rotating application logger
  settings.json        - Auto-saved user preferences when local app dir is writable
  notes.json           - Notes when local app dir is writable
  icon.ico             - App icon
  create_icon.py       - Regenerate icon.ico
  build.bat            - Build standalone dist\SbtDeskTran.exe
  ci_release.py        - CI version stamping and release asset generation
  updater.py           - Auto-update check/download/apply logic
  version_changes.txt  - External version change text
  SbtDeskTran.bat      - Run with Python directly
```
