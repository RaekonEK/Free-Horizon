@echo off
rem Builds the Free-Horizon launcher as a release .exe (optimized, shaders compiled in)
rem and prints where the file is. Double-click, or run from a terminal.
setlocal EnableExtensions
cd /d "%~dp0"

rem gpui compiles its HLSL shaders with fxc.exe from the Windows SDK. The tool is
rem installed per user; fall back to whatever fxc is on PATH.
set "GPUI_FXC_PATH=%LOCALAPPDATA%\Microsoft\Windows Kits\10\bin\10.0.28000.0\x64\fxc.exe"
if not exist "%GPUI_FXC_PATH%" for /f "delims=" %%F in ('where fxc.exe 2^>nul') do if not defined GPUI_FXC_PATH set "GPUI_FXC_PATH=%%F"
if not exist "%GPUI_FXC_PATH%" (
  echo [!] fxc.exe was not found.
  echo     Install the Windows SDK, or set GPUI_FXC_PATH to your fxc.exe.
  exit /b 1
)
echo Using fxc: %GPUI_FXC_PATH%

rem The launcher's version is the official one: Free-Horizon carries the number of
rem horizon-rust/horizon's newest launcher release, so this build shows the same version as the
rem released launcher instead of a number of its own. Offline, the build keeps its Cargo version.
set "LAUNCHER_VERSION="
set "FH_VERSION_FILE=%TEMP%\free-horizon-launcher-version.txt"
if exist "%FH_VERSION_FILE%" del "%FH_VERSION_FILE%" >nul 2>&1
powershell -NoProfile -ExecutionPolicy Bypass -Command "try { $r = Invoke-RestMethod -Uri 'https://api.github.com/repos/horizon-rust/horizon/releases?per_page=100' -Headers @{ 'User-Agent' = 'Free-Horizon' }; $v = @($r.tag_name | Where-Object { $_ -match '^launcher[@-]v?[0-9]' } | ForEach-Object { $_ -replace '^launcher[@-]v?', '' } | Select-Object -First 1); if ($v.Count -gt 0) { Write-Output $v[0] } } catch { }" > "%FH_VERSION_FILE%" 2>nul
if exist "%FH_VERSION_FILE%" set /p LAUNCHER_VERSION=<"%FH_VERSION_FILE%"
del "%FH_VERSION_FILE%" >nul 2>&1
if defined LAUNCHER_VERSION (
  set "HORIZON_LAUNCHER_VERSION=%LAUNCHER_VERSION%"
  set "HORIZON_LAUNCHER_TAG=fork-launcher-v%LAUNCHER_VERSION%"
  echo Launcher version: %LAUNCHER_VERSION% ^(the official launcher's number^)
) else (
  echo The official launcher version could not be read; building launcher\Cargo.toml's version.
)

pushd "%~dp0launcher"
cargo build --release
set "CODE=%ERRORLEVEL%"
popd
if not "%CODE%"=="0" (
  echo.
  echo [!] Build failed. See the cargo output above.
  exit /b %CODE%
)

echo.
echo Built: %~dp0launcher\target\release\horizon-launcher.exe
if defined LAUNCHER_VERSION echo Version: %LAUNCHER_VERSION%
endlocal
exit /b 0
