"""
Path helpers for user-writable application data.

The app is often launched from a shared or network-mounted folder. In that
case the executable directory may be readable but not writable, so runtime
files must fall back to a local per-user directory.
"""
import os
import sys
import tempfile


APP_NAME = "SbtDeskTran"
_DATA_DIR = None


def app_dir() -> str:
    if getattr(sys, "frozen", False):
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(os.path.abspath(__file__))


def _is_writable_dir(path: str) -> bool:
    try:
        os.makedirs(path, exist_ok=True)
        test_path = os.path.join(path, ".write_test")
        with open(test_path, "w", encoding="utf-8") as f:
            f.write("ok")
        try:
            os.remove(test_path)
        except Exception:
            pass
        return True
    except Exception:
        return False


def _candidate_dirs():
    candidates = []
    env_dir = os.environ.get("SBTDESKTRAN_DATA_DIR")
    if env_dir:
        candidates.append(env_dir)

    bundled_dir = app_dir()
    bundled_is_unc = bundled_dir.startswith("\\\\")
    if not bundled_is_unc:
        candidates.append(bundled_dir)

    for env_name in ("LOCALAPPDATA", "APPDATA"):
        base = os.environ.get(env_name)
        if base:
            candidates.append(os.path.join(base, APP_NAME))

    home = os.path.expanduser("~")
    if home and home != "~":
        candidates.append(os.path.join(home, "AppData", "Local", APP_NAME))

    candidates.append(os.path.join(tempfile.gettempdir(), APP_NAME))
    if bundled_is_unc:
        candidates.append(bundled_dir)

    unique = []
    seen = set()
    for path in candidates:
        path = os.path.abspath(path)
        key = os.path.normcase(path)
        if key not in seen:
            unique.append(path)
            seen.add(key)
    return unique


def data_dir() -> str:
    global _DATA_DIR
    if _DATA_DIR:
        return _DATA_DIR

    for path in _candidate_dirs():
        if _is_writable_dir(path):
            _DATA_DIR = path
            return path

    _DATA_DIR = tempfile.gettempdir()
    return _DATA_DIR


def data_file(name: str) -> str:
    return os.path.join(data_dir(), name)


def data_file_candidates(name: str):
    candidates = [data_file(name)]
    bundled = os.path.join(app_dir(), name)
    if os.path.normcase(os.path.abspath(bundled)) != os.path.normcase(os.path.abspath(candidates[0])):
        candidates.append(bundled)
    return candidates
