"""
Shared network utilities: proxy detection, SSL configuration, HTTP requests,
PowerShell fallback, and error diagnostics for corporate/VPN/proxy environments.
"""
import json
import socket
import ssl
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request

try:
    from logger import log
except Exception:
    import logging
    log = logging.getLogger("SbtDeskTran")


DEFAULT_USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/120.0.0.0 Safari/537.36"
)


# ── Proxy detection ─────────────────────────────────────────────

def get_system_proxy() -> dict:
    """Read Windows system proxy settings from registry."""
    proxy = {}
    try:
        import winreg
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        )
        try:
            auto_config, _ = winreg.QueryValueEx(key, "AutoConfigURL")
            log.debug(f"WinINet proxy PAC detected: {auto_config}")
        except Exception:
            pass
        try:
            auto_detect, _ = winreg.QueryValueEx(key, "AutoDetect")
            log.debug(f"WinINet proxy auto-detect: {auto_detect}")
        except Exception:
            pass
        enabled, _ = winreg.QueryValueEx(key, "ProxyEnable")
        if enabled:
            server, _ = winreg.QueryValueEx(key, "ProxyServer")
            if server:
                if "://" not in server:
                    server = "http://" + server
                proxy = {"http": server, "https": server}
                log.debug(f"System proxy detected: {server}")
    except Exception as e:
        log.debug(f"No system proxy or registry read failed: {e}")
    return proxy


def log_winhttp_proxy_once():
    if getattr(log_winhttp_proxy_once, "_done", False) or sys.platform != "win32":
        return
    log_winhttp_proxy_once._done = True
    try:
        proc = subprocess.run(
            ["netsh", "winhttp", "show", "proxy"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=5,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        text = (proc.stdout or proc.stderr).decode("utf-8", errors="replace").strip()
        if text:
            log.debug("WinHTTP proxy: " + " | ".join(
                line.strip() for line in text.splitlines() if line.strip()
            ))
    except Exception as e:
        log.debug(f"WinHTTP proxy check failed: {e}")


# ── Opener builder ──────────────────────────────────────────────

def build_opener(proxy: dict = None, ssl_verify: bool = True) -> urllib.request.OpenerDirector:
    """Build urllib opener with optional proxy and SSL settings.

    * proxy=None  — use urllib default system/env proxy lookup
    * proxy={}    — explicitly direct (no proxy)
    * proxy=dict  — configured proxy
    """
    handlers = []

    if proxy is None:
        handlers.append(urllib.request.ProxyHandler())
    else:
        handlers.append(urllib.request.ProxyHandler(proxy))

    if not ssl_verify:
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        handlers.append(urllib.request.HTTPSHandler(context=ctx))

    return urllib.request.build_opener(*handlers)


def proxy_label(proxy) -> str:
    if proxy == "windows":
        return "windows-proxy"
    if proxy is None:
        return "system-proxy"
    if proxy:
        return "configured-proxy"
    return "direct"


# ── HTTP requests ───────────────────────────────────────────────

def do_request(url: str, timeout: int, opener: urllib.request.OpenerDirector,
               user_agent: str = DEFAULT_USER_AGENT) -> bytes:
    """Perform an HTTP GET via *opener* with standard headers."""
    req = urllib.request.Request(url)
    req.add_header("User-Agent", user_agent)
    req.add_header("Accept", "application/json, text/plain, */*")
    req.add_header("Accept-Language", "en-US,en;q=0.9")
    with opener.open(req, timeout=timeout) as resp:
        return resp.read()


def do_request_powershell(url: str, timeout: int,
                          user_agent: str = DEFAULT_USER_AGENT) -> bytes:
    """Use Windows PowerShell/.NET networking as a corporate proxy/PAC fallback."""
    if sys.platform != "win32":
        raise OSError("PowerShell fallback is only available on Windows")

    script = r"""
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$payload = [Console]::In.ReadToEnd() | ConvertFrom-Json
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
$proxy = [System.Net.WebRequest]::DefaultWebProxy
if ($proxy -ne $null) {
    $proxy.Credentials = [System.Net.CredentialCache]::DefaultCredentials
}
$headers = @{
    'User-Agent' = '%s'
    'Accept' = 'application/json, text/plain, */*'
    'Accept-Language' = 'en-US,en;q=0.9'
}
$resp = Invoke-WebRequest -Uri $payload.url -UseBasicParsing -TimeoutSec $payload.timeout -Headers $headers
[Console]::Out.Write($resp.Content)
""" % user_agent.replace("'", "''")

    startupinfo = None
    creationflags = 0
    if sys.platform == "win32":
        startupinfo = subprocess.STARTUPINFO()
        startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    payload = json.dumps({"url": url, "timeout": timeout})
    proc = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive",
         "-ExecutionPolicy", "Bypass", "-Command", script],
        input=payload.encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout + 8,
        startupinfo=startupinfo,
        creationflags=creationflags,
    )
    if proc.returncode != 0:
        err = proc.stderr.decode("utf-8", errors="replace").strip()
        raise OSError(err or f"PowerShell exited with code {proc.returncode}")
    return proc.stdout


# ── Multi-strategy retry ────────────────────────────────────────

def build_strategies(url: str, user_agent: str = DEFAULT_USER_AGENT) -> list:
    """Return list of (url, proxy, ssl_verify, timeout, transport) tuples."""
    log_winhttp_proxy_once()
    configured_proxy = get_system_proxy()
    system_proxy = configured_proxy or None
    return [
        (url, system_proxy, True,  12, "urllib"),      # system proxy
        (url, "windows",    True,  25, "powershell"),  # Windows proxy/PAC
        (url, system_proxy, False, 12, "urllib"),      # SSL off
        (url, {},           True,  15, "urllib"),      # direct
        (url, {},           False, 15, "urllib"),      # direct, SSL off
    ]


def request_with_strategies(
    url: str,
    user_agent: str = DEFAULT_USER_AGENT,
    working_strategy: int = -1,
    settings: dict = None,
    strategy_key: str = "network_strategy",
) -> tuple[bytes, int]:
    """Try network strategies in order, return (response_body, strategy_index).

    If *working_strategy* >= 0, that strategy is attempted first as a cache.
    When *settings* is provided, the working strategy is persisted to
    ``settings[strategy_key]`` after a successful attempt.
    """
    if working_strategy < 0 and settings is not None:
        working_strategy = settings.get(strategy_key, -1)

    strategies = build_strategies(url, user_agent)

    # Try cached strategy first
    if 0 <= working_strategy < len(strategies):
        url_s, proxy, ssl_verify, timeout, transport = strategies[working_strategy]
        log.debug(f"Using cached strategy {working_strategy} ({proxy_label(proxy)})")
        try:
            if transport == "powershell":
                data = do_request_powershell(url_s, timeout, user_agent=user_agent)
            else:
                opener = build_opener(proxy, ssl_verify)
                data = do_request(url_s, timeout, opener, user_agent=user_agent)
            return data, working_strategy
        except (urllib.error.URLError, socket.timeout, TimeoutError,
                OSError, subprocess.SubprocessError) as e:
            log.warning(f"Cached strategy {working_strategy} failed: {e}")

    # Full retry
    last_error = None
    for i, (url_s, proxy, ssl_verify, timeout, transport) in enumerate(strategies):
        pl = proxy_label(proxy)
        ssl_l = "ssl-on" if ssl_verify else "ssl-off"
        log.debug(f"Attempt {i+1}/{len(strategies)}: {pl} {ssl_l} {transport}")
        try:
            if transport == "powershell":
                data = do_request_powershell(url_s, timeout, user_agent=user_agent)
            else:
                opener = build_opener(proxy, ssl_verify)
                data = do_request(url_s, timeout, opener, user_agent=user_agent)
            log.info(f"Strategy {i} ({pl}, {ssl_l}) succeeded — cached")
            if settings is not None:
                settings[strategy_key] = i
            return data, i
        except (urllib.error.URLError, socket.timeout, TimeoutError,
                OSError, subprocess.SubprocessError) as e:
            log.warning(f"Attempt {i+1} failed: {type(e).__name__}: {e}")
            last_error = e

    raise last_error or ConnectionError("All network strategies failed")


# ── Error diagnostics ───────────────────────────────────────────

def network_hint(error) -> str:
    """Return a user-friendly diagnostic message for a network error."""
    reason = getattr(error, "reason", error)
    winerror = getattr(reason, "winerror", None) or getattr(error, "winerror", None)
    if winerror == 10013:
        return (
            "Socket permission denied (WinError 10013). Windows Firewall, "
            "endpoint security, AppLocker, or policy for apps launched from a "
            "network share is blocking outbound HTTPS."
        )
    if isinstance(reason, socket.gaierror):
        return ("DNS lookup failed. Check DNS, VPN, proxy, "
                "or remote-session network settings.")
    if isinstance(reason, ssl.SSLError):
        return ("SSL certificate/inspection error. "
                "Check corporate proxy or certificate trust.")
    if isinstance(error, subprocess.TimeoutExpired):
        return ("Windows/PowerShell web request timed out. "
                "Proxy/PAC route may be unreachable from this process.")
    return str(reason)
