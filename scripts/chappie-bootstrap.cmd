@echo off
REM Chappie auto-bootstrap (Windows cmd).
REM Wired to the Claude Code SessionStart hook. Downloads the prebuilt
REM chappie-daemon.exe on first session. Idempotent and never fatal:
REM always exits 0 so a failure cannot block the session.
setlocal EnableExtensions

set "REPO=Mic-360/chappie"

REM Resolve all paths relative to this script's own location.
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "PLUGIN_ROOT=%%~fI"
set "BIN_DIR=%PLUGIN_ROOT%\target\release"
set "BIN=%BIN_DIR%\chappie-daemon.exe"

set "LOG_DIR=%USERPROFILE%\.claude\.chappie_state"
if not exist "%LOG_DIR%" mkdir "%LOG_DIR%" >nul 2>&1
set "LOG=%LOG_DIR%\bootstrap.log"

REM Idempotent: already installed -> exit fast.
if exist "%BIN%" exit /b 0

REM Windows ARM64 transparently emulates x86_64, so one asset serves all.
set "ASSET=chappie-daemon-windows-x86_64.exe"
set "URL=https://github.com/%REPO%/releases/latest/download/%ASSET%"

if not exist "%BIN_DIR%" mkdir "%BIN_DIR%" >nul 2>&1
set "TMP=%BIN_DIR%\.chappie-daemon.download"
if exist "%TMP%" del /f /q "%TMP%" >nul 2>&1

echo [chappie-bootstrap] downloading %URL% >> "%LOG%"
curl -fsSL --retry 2 --max-time 120 -o "%TMP%" "%URL%" >> "%LOG%" 2>&1

set "DLOK="
if exist "%TMP%" for %%A in ("%TMP%") do if %%~zA GTR 0 set "DLOK=1"

if defined DLOK (
  move /y "%TMP%" "%BIN%" >nul 2>&1
  if exist "%BIN%" (
    echo [chappie-bootstrap] installed %BIN% >> "%LOG%"
    exit /b 0
  )
  echo [chappie-bootstrap] move into place failed >> "%LOG%"
)

if exist "%TMP%" del /f /q "%TMP%" >nul 2>&1
echo [chappie-bootstrap] download failed >> "%LOG%"

REM Fallback: build from source if Rust is available.
where cargo >nul 2>&1
if %ERRORLEVEL%==0 (
  echo [chappie-bootstrap] falling back to cargo build >> "%LOG%"
  pushd "%PLUGIN_ROOT%"
  cargo build --release >> "%LOG%" 2>&1
  popd
  if exist "%BIN%" (
    echo [chappie-bootstrap] built from source >> "%LOG%"
    exit /b 0
  )
)

echo [chappie-bootstrap] could not obtain chappie-daemon >> "%LOG%"
exit /b 0
