@echo off
setlocal
cd /d "%~dp0"

where npm >nul 2>&1
if errorlevel 1 (
  echo ERROR: Node.js/npm was not found in PATH.
  exit /b 1
)

where cargo >nul 2>&1
if errorlevel 1 (
  if not exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    echo ERROR: Rust/cargo was not found in PATH or %%USERPROFILE%%\.cargo\bin.
    exit /b 1
  )
  set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
)

if not exist node_modules\ (
  call npm ci
  if errorlevel 1 exit /b 1
)

set "CARGO_TARGET_DIR=%CD%\target"
echo Building SbtDeskTool EXE for local testing without changing the release version...
node scripts\build-exe.mjs
if errorlevel 1 exit /b 1

echo.
echo Build completed:
echo   App: target\release\sbt-desk-tool.exe
endlocal
