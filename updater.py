"""
Small self-updater for the one-file Windows build.

The updater checks the latest public GitHub release, downloads the release
asset, extracts only SbtDeskTran.exe, and replaces the running executable
through a helper batch file after the app exits.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import urllib.parse
import zipfile
from dataclasses import dataclass
from typing import Callable, Optional

from app_paths import app_dir
from network import request_with_strategies
from version import __version__


APP_EXE_NAME = "SbtDeskTran.exe"
UPDATE_ARCHIVE_PREFIX = "SbtDeskTran-"
LEGACY_UPDATE_ARCHIVE_NAME = "SbtDeskTran.zip"
VERSION_CHANGES_NAME = "version_changes.txt"
GITHUB_LATEST_RELEASE_API = "https://api.github.com/repos/SabiTechHolding/SbtDeskTran/releases/latest"
ENV_RELEASE_API_URL = "SBTDESKTRAN_RELEASE_API_URL"


@dataclass
class UpdateInfo:
    version: str
    download_url: str
    notes: str = ""
    notes_url: str = ""
    release_url: str = ""


def current_version() -> str:
    return str(__version__).lstrip("v")


def _version_parts(value: str):
    parts = re.findall(r"\d+", str(value))
    return tuple(int(part) for part in parts) if parts else (0,)


def is_newer_version(remote: str, local: str) -> bool:
    remote_parts = _version_parts(remote)
    local_parts = _version_parts(local)
    max_len = max(len(remote_parts), len(local_parts))
    remote_parts += (0,) * (max_len - len(remote_parts))
    local_parts += (0,) * (max_len - len(local_parts))
    return remote_parts > local_parts


def release_api_url() -> str:
    return os.environ.get(ENV_RELEASE_API_URL, "").strip() or GITHUB_LATEST_RELEASE_API.strip()


def is_supported_runtime() -> bool:
    return bool(getattr(sys, "frozen", False)) and sys.platform == "win32"


def _read_url(url: str, timeout: int = 25, settings: dict = None) -> bytes:
    ua = f"SbtDeskTran/{current_version()}"
    data, _ = request_with_strategies(url, user_agent=ua, working_strategy=-1,
                                      settings=settings)
    return data


def _find_release_asset(release: dict, *names: str) -> Optional[dict]:
    wanted = {name.lower() for name in names}
    for asset in release.get("assets", []) or []:
        if str(asset.get("name", "")).lower() in wanted:
            return asset
    return None


def _find_update_asset(release: dict) -> Optional[dict]:
    version = str(release.get("tag_name", "")).lstrip("v").strip()
    exact = _find_release_asset(
        release,
        f"{UPDATE_ARCHIVE_PREFIX}{version}.zip" if version else "",
        LEGACY_UPDATE_ARCHIVE_NAME,
        APP_EXE_NAME,
    )
    if exact:
        return exact
    for asset in release.get("assets", []) or []:
        name = str(asset.get("name", "")).lower()
        if name.startswith("sbtdesktran") and (name.endswith(".zip") or name.endswith(".exe")):
            return asset
    return None


def _asset_download_url(asset: Optional[dict]) -> str:
    if not asset:
        return ""
    return str(asset.get("browser_download_url", "") or "")


def check_for_update(settings: dict = None) -> Optional[UpdateInfo]:
    url = release_api_url()
    if not url:
        return None

    raw = _read_url(url, settings=settings)
    release = json.loads(raw.decode("utf-8-sig"))
    version = str(release.get("tag_name", "")).lstrip("v").strip()
    update_asset = _find_update_asset(release)
    download_url = _asset_download_url(update_asset)
    if not version or not download_url:
        raise ValueError(
            f"Latest GitHub release must contain {UPDATE_ARCHIVE_PREFIX}{version}.zip or {APP_EXE_NAME}"
        )

    if not is_newer_version(version, current_version()):
        return None

    notes = str(release.get("body", "") or "").strip()
    notes_asset = _find_release_asset(release, VERSION_CHANGES_NAME)
    notes_url = _asset_download_url(notes_asset)
    if notes_url:
        try:
            notes = _read_url(notes_url, timeout=10, settings=settings).decode("utf-8-sig").strip()
        except Exception:
            notes = notes or f"Version {version} is available."

    return UpdateInfo(
        version=version,
        download_url=download_url,
        notes=notes,
        notes_url=notes_url,
        release_url=str(release.get("html_url", "") or ""),
    )


def check_for_update_async(
    callback: Callable[[Optional[UpdateInfo], Optional[Exception]], None],
    settings: dict = None,
) -> None:
    def worker():
        try:
            callback(check_for_update(settings=settings), None)
        except Exception as exc:
            callback(None, exc)

    threading.Thread(target=worker, daemon=True).start()


def _download_to_temp(url: str, settings: dict = None) -> str:
    suffix = ".zip" if urllib.parse.urlparse(url).path.lower().endswith(".zip") else ".exe"
    fd, path = tempfile.mkstemp(prefix="SbtDeskTran-update-", suffix=suffix)
    os.close(fd)
    try:
        ua = f"SbtDeskTran/{current_version()}"
        data, _ = request_with_strategies(url, user_agent=ua, working_strategy=-1,
                                          settings=settings)
        with open(path, "wb") as out:
            out.write(data)
        return path
    except Exception:
        try:
            os.remove(path)
        except Exception:
            pass
        raise


def _extract_exe(download_path: str) -> str:
    target_dir = tempfile.mkdtemp(prefix="SbtDeskTran-update-extract-")
    target_exe = os.path.join(target_dir, APP_EXE_NAME)

    if zipfile.is_zipfile(download_path):
        with zipfile.ZipFile(download_path) as archive:
            exe_names = [
                name for name in archive.namelist()
                if os.path.basename(name).lower() == APP_EXE_NAME.lower()
            ]
            if not exe_names:
                raise ValueError(f"{APP_EXE_NAME} was not found in update archive")
            with archive.open(exe_names[0]) as src, open(target_exe, "wb") as dst:
                shutil.copyfileobj(src, dst)
    else:
        shutil.copy2(download_path, target_exe)

    return target_exe


def _write_helper_batch(new_exe: str, current_exe: str, restart: bool) -> str:
    bat_path = os.path.join(tempfile.gettempdir(), "SbtDeskTran-apply-update.bat")
    backup_exe = current_exe + ".bak"
    restart_line = f'start "" "{current_exe}"' if restart else "rem restart disabled"
    script = f"""@echo off
setlocal
set "NEW_EXE={new_exe}"
set "CURRENT_EXE={current_exe}"
set "BACKUP_EXE={backup_exe}"
set "PID={os.getpid()}"

:wait_app
tasklist /FI "PID eq %PID%" | find "%PID%" >nul
if not errorlevel 1 (
    timeout /t 1 /nobreak >nul
    goto wait_app
)

if exist "%BACKUP_EXE%" del /f /q "%BACKUP_EXE%"
if exist "%CURRENT_EXE%" move /y "%CURRENT_EXE%" "%BACKUP_EXE%" >nul
copy /y "%NEW_EXE%" "%CURRENT_EXE%" >nul
if errorlevel 1 (
    if exist "%BACKUP_EXE%" move /y "%BACKUP_EXE%" "%CURRENT_EXE%" >nul
    exit /b 1
)
{restart_line}
endlocal
"""
    with open(bat_path, "w", encoding="mbcs") as f:
        f.write(script)
    return bat_path


def download_and_stage_update(info: UpdateInfo, restart: bool = True, settings: dict = None) -> str:
    if not is_supported_runtime():
        raise RuntimeError("Auto-update is available only in the Windows executable build")

    current_exe = os.path.join(app_dir(), APP_EXE_NAME)
    if os.path.normcase(os.path.abspath(sys.executable)) != os.path.normcase(os.path.abspath(current_exe)):
        current_exe = sys.executable

    download_path = _download_to_temp(info.download_url, settings=settings)
    new_exe = _extract_exe(download_path)
    return _write_helper_batch(new_exe, current_exe, restart)


def run_update_helper(helper_bat: str) -> None:
    subprocess.Popen(
        ["cmd", "/c", helper_bat],
        creationflags=subprocess.CREATE_NO_WINDOW if hasattr(subprocess, "CREATE_NO_WINDOW") else 0,
    )
