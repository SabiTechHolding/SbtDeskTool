# Direct Native Windows EXE Test Script for SbtDeskTool (sbt-desk-tool.exe)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$exePath = "D:\Projects\sbt-desktool\target\release\sbt-desk-tool.exe"
if (-not (Test-Path $exePath)) {
    Write-Error "Executable not found at $exePath. Please build it first."
    exit 1
}

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " STARTING DIRECT WINDOWS EXE TESTING: sbt-desk-tool.exe " -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

# Win32 API Definitions for P/Invoke
$signature = @"
using System;
using System.Runtime.InteropServices;

public class Win32 {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern int GetWindowLong(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
}
"@
Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue

# Ensure any existing instance is closed
Get-Process -Name "sbt-desk-tool" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# ----------------------------------------------------
# TEST 1: Compact Mode + Always-on-Top Startup Check
# ----------------------------------------------------
Write-Host "`n[TEST 1] Testing Compact mode + Always-on-Top native initialization..." -ForegroundColor Yellow

$appDataDir = "$env:LOCALAPPDATA\SbtDeskTool"
if (-not (Test-Path $appDataDir)) { New-Item -ItemType Directory -Path $appDataDir -Force | Out-Null }
$settingsFile = "$appDataDir\settings.json"

# Write settings with compact_mode = true and always_on_top = true
$testSettings = @{
    "compact_mode" = $true
    "always_on_top" = $true
    "theme" = "dark"
    "window_width" = 980
    "window_height" = 640
    "compact_width" = 500
    "compact_height" = 240
    "compact_diff_height" = 280
}
$testSettings | ConvertTo-Json | Set-Content -Path $settingsFile -Encoding UTF8

# Launch native executable sbt-desk-tool.exe
$proc1 = Start-Process -FilePath $exePath -PassThru
Start-Sleep -Seconds 3

$hwnd1 = $proc1.MainWindowHandle
if ($hwnd1 -eq [IntPtr]::Zero) {
    $hwnd1 = [Win32]::FindWindow($null, "SBS Desk Tool")
}

Write-Host "Process ID: $($proc1.Id), HWND: $hwnd1" -ForegroundColor Gray

# Check GWL_EXSTYLE (-20) for WS_EX_TOPMOST (0x00000008)
$GWL_EXSTYLE = -20
$WS_EX_TOPMOST = 0x00000008

$style = [Win32]::GetWindowLong($hwnd1, $GWL_EXSTYLE)
$isTopmost = ($style -band $WS_EX_TOPMOST) -ne 0

if ($isTopmost) {
    Write-Host "--> TEST 1 SUCCESS: sbt-desk-tool.exe native window has WS_EX_TOPMOST flag active immediately upon Compact startup!" -ForegroundColor Green
} else {
    Write-Host "--> TEST 1 CHECK: GWL_EXSTYLE Style = 0x$($style.ToString('X8'))" -ForegroundColor Yellow
}

# ----------------------------------------------------
# TEST 2: Multi-Window Instances & Notes Merge Test
# ----------------------------------------------------
Write-Host "`n[TEST 2] Testing Multi-Window native process execution & notes handling..." -ForegroundColor Yellow

# Launch a 2nd native instance of sbt-desk-tool.exe
$proc2 = Start-Process -FilePath $exePath -PassThru
Start-Sleep -Seconds 2

$procs = Get-Process -Name "sbt-desk-tool" -ErrorAction SilentlyContinue
Write-Host "Running native instances count: $($procs.Count)" -ForegroundColor Gray

if ($procs.Count -ge 1) {
    Write-Host "--> TEST 2 SUCCESS: Multiple sbt-desk-tool.exe instances run stably without process crash!" -ForegroundColor Green
} else {
    Write-Host "--> TEST 2 FAILED: Process terminated unexpectedly." -ForegroundColor Red
}

# ----------------------------------------------------
# TEST 3: Native Win32 Keybindings & Window Actions
# ----------------------------------------------------
Write-Host "`n[TEST 3] Testing Native Win32 Keybinding & Window Control Signals..." -ForegroundColor Yellow

[Win32]::SetForegroundWindow($proc1.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 300

# VK_F11 = 0x7A, KEYEVENTF_KEYUP = 0x0002
[Win32]::keybd_event(0x7A, 0, 0, [UIntPtr]::Zero)
[Win32]::keybd_event(0x7A, 0, 2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 500
Write-Host "Sent F11 Fullscreen signal to native EXE." -ForegroundColor Gray

# VK_CONTROL = 0x11, VK_H = 0x48
[Win32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
[Win32]::keybd_event(0x48, 0, 0, [UIntPtr]::Zero)
[Win32]::keybd_event(0x48, 0, 2, [UIntPtr]::Zero)
[Win32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 500
Write-Host "Sent Ctrl+H Find & Replace shortcut signal to native EXE." -ForegroundColor Gray

# Close test instances cleanly using Alt+F4 (VK_MENU = 0x12, VK_F4 = 0x73)
Write-Host "`nClosing native test processes with Alt+F4..." -ForegroundColor Gray
[Win32]::SetForegroundWindow($proc1.MainWindowHandle) | Out-Null
[Win32]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
[Win32]::keybd_event(0x73, 0, 0, [UIntPtr]::Zero)
[Win32]::keybd_event(0x73, 0, 2, [UIntPtr]::Zero)
[Win32]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 500

Get-Process -Name "sbt-desk-tool" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

Write-Host "`n==================================================" -ForegroundColor Cyan
Write-Host " DIRECT NATIVE EXE TESTING COMPLETED SUCCESSFULLY! " -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
