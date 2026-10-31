@echo off
rem Updates this folder: origin (Free-Horizon, with fork.rs kept as is) and upstream
rem (horizon-rust/horizon). Your uncommitted changes are stashed and put back.
rem Usage: update.bat
setlocal EnableExtensions
cd /d "%~dp0"

set "UPSTREAM_URL=https://github.com/horizon-rust/horizon.git"
git remote get-url upstream >nul 2>&1
if errorlevel 1 (
  echo Adding the upstream remote...
  git remote add upstream "%UPSTREAM_URL%"
)

set "DIRTY="
for /f "delims=" %%L in ('git status --porcelain') do set "DIRTY=1"
if defined DIRTY (
  echo Keeping your uncommitted changes in a stash...
  git stash push -u -m "update.bat: local changes"
  if errorlevel 1 goto fail
)

echo Fetching origin and upstream...
git fetch origin --prune
if errorlevel 1 goto fail
git fetch upstream --prune
if errorlevel 1 goto fail

echo Updating from origin...
git pull --ff-only origin main
if errorlevel 1 goto fail

rem GitHub already merges upstream into origin/main every 6 hours (fork sync), keeping this
rem fork's side of any hunk upstream also touched. This merge is the fallback for a folder that
rem has been offline; it uses the same rule (-X ours), so fork.rs and the lines that read it stay.
echo Merging upstream/main ^(this fork's own changes stay^)...
git merge --no-edit -X ours upstream/main
if errorlevel 1 goto conflict

if defined DIRTY (
  git stash pop
  if errorlevel 1 goto fail
)

echo.
echo Up to date. Run build.bat to rebuild the launcher.
exit /b 0

:conflict
echo.
echo [!] The merge needs your hand: git left it in progress.
echo     Resolve the files listed above, then:
echo         git add -A ^&^& git commit
if defined DIRTY echo     Your own changes are still in the stash; restore them after the commit with: git stash pop
exit /b 1

:fail
echo.
echo [!] Update stopped; see the message above.
if defined DIRTY echo     Your changes are safe in the stash: git stash list
exit /b 1
