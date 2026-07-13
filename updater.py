"""
Small self-updater for the one-file Windows build.

The updater checks the latest public GitHub release, downloads the release
asset, extracts only SbtDeskTran.exe, and replaces the running executable
through a helper batch file after the app exits.
"""
import json
import hashlib
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
from typing import Callable, Optional, Tuple

from app_paths import app_dir
from network import request_with_strategies
from version import __version__


APP_EXE_NAME = "SbtDeskTran.exe"
STAGED_EXE_SUFFIX = ".update"
UPDATE_ARCHIVE_PREFIX = "SbtDeskTran-"
LEGACY_UPDATE_ARCHIVE_NAME = "SbtDeskTran.zip"
VERSION_CHANGES_NAME = "version_changes.txt"
GITHUB_LATEST_RELEASE_API = "https://api.github.com/repos/SabiTechHolding/SbtDeskTran/releases/latest"
ENV_RELEASE_API_URL = "SBTDESKTRAN_RELEASE_API_URL"
PYINSTALLER_COOKIE = b"MEI\014\013\012\013\016"
PYINSTALLER_ENV_VARS = (
    "_MEIPASS2",
    "_PYI_ARCHIVE_FILE",
    "_PYI_APPLICATION_HOME_DIR",
    "_PYI_BOOTLOADER_IGNORE_SIGNALS",
    "_PYI_LINUX_PROCESS_NAME",
    "_PYI_PARENT_PROCESS_LEVEL",
    "_PYI_SPLASH_IPC",
)


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
        if data.strip() == b"System.Byte[]":
            raise ValueError("Downloaded update payload is not binary data")
        with open(path, "wb") as out:
            out.write(data)
        return path
    except Exception:
        try:
            os.remove(path)
        except Exception:
            pass
        raise


def _cleanup_path(path: str) -> None:
    if not path:
        return
    try:
        if os.path.isdir(path):
            shutil.rmtree(path, ignore_errors=True)
        elif os.path.exists(path):
            os.remove(path)
    except Exception:
        pass


def _extract_exe(download_path: str) -> Tuple[str, str]:
    target_dir = tempfile.mkdtemp(prefix="SbtDeskTran-update-extract-")
    target_exe = os.path.join(target_dir, APP_EXE_NAME)

    try:
        expected_zip = download_path.lower().endswith(".zip")
        if expected_zip and not zipfile.is_zipfile(download_path):
            raise ValueError("Downloaded update is not a valid zip archive")

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

        _validate_windows_exe(target_exe)
        return target_exe, target_dir
    except Exception:
        _cleanup_path(target_dir)
        raise


def _validate_windows_exe(path: str) -> None:
    try:
        with open(path, "rb") as f:
            header = f.read(2)
            f.seek(0, os.SEEK_END)
            size = f.tell()
            f.seek(max(0, size - 1024 * 1024))
            tail = f.read()
    except Exception as exc:
        raise ValueError(f"Could not read downloaded executable: {exc}") from exc
    if header != b"MZ":
        raise ValueError("Downloaded update does not look like a valid Windows executable")
    if size < 1024 * 1024:
        raise ValueError("Downloaded executable is unexpectedly small")
    if PYINSTALLER_COOKIE not in tail:
        raise ValueError("Downloaded executable does not contain a valid PyInstaller archive")


def _file_sha256(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def _clean_pyinstaller_env() -> dict:
    env = os.environ.copy()
    for key in list(env):
        upper_key = key.upper()
        if upper_key.startswith("_PYI") or upper_key.startswith("PYINSTALLER_"):
            env.pop(key, None)
    for key in PYINSTALLER_ENV_VARS:
        env.pop(key, None)
    env["PYINSTALLER_RESET_ENVIRONMENT"] = "1"
    return env


def _stage_exe_in_app_dir(new_exe: str, current_exe: str) -> str:
    base, ext = os.path.splitext(current_exe)
    staged_exe = f"{base}{STAGED_EXE_SUFFIX}{ext or '.exe'}"
    expected_size = os.path.getsize(new_exe)
    expected_sha256 = _file_sha256(new_exe)

    _cleanup_path(staged_exe)
    shutil.copy2(new_exe, staged_exe)
    try:
        _validate_windows_exe(staged_exe)
        if os.path.getsize(staged_exe) != expected_size:
            raise ValueError("Staged executable size does not match the downloaded update")
        if _file_sha256(staged_exe) != expected_sha256:
            raise ValueError("Staged executable hash does not match the downloaded update")
        return staged_exe
    except Exception:
        _cleanup_path(staged_exe)
        raise


def _write_helper_batch(
    staged_exe: str, current_exe: str, restart: bool,
) -> str:
    bat_path = os.path.join(tempfile.gettempdir(), "SbtDeskTran-apply-update.bat")
    backup_exe = current_exe + ".bak"
    staged_exe_size = os.path.getsize(staged_exe)
    staged_exe_sha256 = _file_sha256(staged_exe)
    pyinstaller_env_lines = "\n".join(
        [f'set "{name}="' for name in PYINSTALLER_ENV_VARS]
        + ['set "PYINSTALLER_RESET_ENVIRONMENT=1"']
    )
    restart_block = """if defined APP_DIR (
    if "!APP_DIR:~0,2!"=="\\\\" (
        powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$wd=$env:APP_DIR; Set-Location -LiteralPath $wd; Start-Process -FilePath $env:CURRENT_EXE -WorkingDirectory $wd"
    ) else (
        pushd "%APP_DIR%" >nul 2>&1
        if errorlevel 1 (
            powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath $env:CURRENT_EXE -WorkingDirectory $env:APP_DIR"
        ) else (
            start "" "%APP_FILE%"
            popd >nul 2>&1
        )
    )
) else (
    start "" "%CURRENT_EXE%"
)""" if restart else "rem restart disabled"
    cleanup_lines = r'''
if defined TEMP (
    del /f /q "%TEMP%\SbtDeskTran-update-*" >nul 2>&1
    for /d %%D in ("%TEMP%\SbtDeskTran-update-extract-*") do rd /s /q "%%D" >nul 2>&1
)
if defined TMP (
    del /f /q "%TMP%\SbtDeskTran-update-*" >nul 2>&1
    for /d %%D in ("%TMP%\SbtDeskTran-update-extract-*") do rd /s /q "%%D" >nul 2>&1
)'''
    script = f"""@echo off
setlocal EnableDelayedExpansion
set "STAGED_EXE={staged_exe}"
set "CURRENT_EXE={current_exe}"
set "BACKUP_EXE={backup_exe}"
set "STAGED_SIZE={staged_exe_size}"
set "STAGED_SHA256={staged_exe_sha256}"
set "PID={os.getpid()}"
set "APP_DIR="
set "APP_FILE="
set /a RETRY_COUNT=0
for %%I in ("%CURRENT_EXE%") do (
    set "APP_DIR=%%~dpI"
    set "APP_FILE=%%~nxI"
)

if not exist "%STAGED_EXE%" exit /b 1
timeout /t 2 /nobreak >nul
taskkill /PID %PID% /T /F >nul 2>&1

:replace_app
set /a RETRY_COUNT+=1
if exist "%BACKUP_EXE%" del /f /q "%BACKUP_EXE%" >nul 2>&1
if exist "%CURRENT_EXE%" (
    move /y "%CURRENT_EXE%" "%BACKUP_EXE%" >nul 2>&1
    if errorlevel 1 (
        if !RETRY_COUNT! GEQ 30 (
            {cleanup_lines}
            exit /b 1
        )
        timeout /t 1 /nobreak >nul
        goto replace_app
    )
)
move /y "%STAGED_EXE%" "%CURRENT_EXE%" >nul 2>&1
if errorlevel 1 (
    if exist "%BACKUP_EXE%" move /y "%BACKUP_EXE%" "%CURRENT_EXE%" >nul 2>&1
    if !RETRY_COUNT! GEQ 30 (
        {cleanup_lines}
        exit /b 1
    )
    timeout /t 1 /nobreak >nul
    goto replace_app
)
for %%A in ("%CURRENT_EXE%") do set "CUR_SIZE=%%~zA"
if not "!STAGED_SIZE!"=="!CUR_SIZE!" (
    if exist "%CURRENT_EXE%" del /f /q "%CURRENT_EXE%" >nul 2>&1
    if exist "%BACKUP_EXE%" move /y "%BACKUP_EXE%" "%CURRENT_EXE%" >nul
    {cleanup_lines}
    exit /b 1
)
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "if ((Get-FileHash -LiteralPath $env:CURRENT_EXE -Algorithm SHA256).Hash -ne $env:STAGED_SHA256) {{ exit 1 }}"
if errorlevel 1 (
    if exist "%CURRENT_EXE%" del /f /q "%CURRENT_EXE%" >nul 2>&1
    if exist "%BACKUP_EXE%" move /y "%BACKUP_EXE%" "%CURRENT_EXE%" >nul 2>&1
    {cleanup_lines}
    exit /b 1
)
{cleanup_lines}
timeout /t 3 /nobreak >nul
{pyinstaller_env_lines}
{restart_block}
endlocal
del /f /q "%~f0" >nul 2>&1
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

    download_path = ""
    extract_dir = ""
    staged_exe = ""
    try:
        download_path = _download_to_temp(info.download_url, settings=settings)
        new_exe, extract_dir = _extract_exe(download_path)
        staged_exe = _stage_exe_in_app_dir(new_exe, current_exe)
        _cleanup_path(download_path)
        _cleanup_path(extract_dir)
        return _write_helper_batch(staged_exe, current_exe, restart)
    except Exception:
        _cleanup_path(download_path)
        _cleanup_path(extract_dir)
        _cleanup_path(staged_exe)
        raise


def run_update_helper(helper_bat: str) -> None:
    creationflags = 0
    startupinfo = None
    if sys.platform == "win32":
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        startupinfo = subprocess.STARTUPINFO()
        startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        startupinfo.wShowWindow = 0
    subprocess.Popen(
        ["cmd.exe", "/d", "/q", "/c", helper_bat],
        cwd=tempfile.gettempdir(),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=_clean_pyinstaller_env(),
        close_fds=True,
        startupinfo=startupinfo,
        creationflags=creationflags,
    )
