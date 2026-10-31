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
endlocal
exit /b 0
