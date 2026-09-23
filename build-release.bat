@echo off
setlocal EnableExtensions
cd /d "%~dp0"

echo ========================================
echo  AxMusic Release Build
echo ========================================
echo.

set "LOG=out\build-release.log"
set "START_FILE=%TEMP%\axmusic-build-start.txt"
if not exist out mkdir out
echo [%date% %time%] start > "%LOG%"

:: ISO start time via PowerShell (avoid %time% parse crashes)
powershell -NoProfile -Command "(Get-Date).ToString('o')" > "%START_FILE%" 2>nul

:: ---- 0. Kill running AxMusic (green build is AxMusic-vX.Y.Z.exe) ----
echo [0/4] Stopping running AxMusic (if any)...
powershell -NoProfile -Command "Get-Process -Name 'AxMusic*','axmusic*' -ErrorAction SilentlyContinue | Stop-Process -Force"
echo [0/4] Stop done. >> "%LOG%"

:: ---- 1. tauri release build ----
echo [1/4] Building release (this can take several minutes)...
echo [1/4] tauri build --no-bundle >> "%LOG%"
:: --no-bundle: only compile axmusic.exe; skip NSIS installer (copy step uses the raw exe)
call npm run tauri build -- --no-bundle
set "BUILD_RC=%ERRORLEVEL%"
echo tauri build exit=%BUILD_RC% >> "%LOG%"
if not "%BUILD_RC%"=="0" (
    echo.
    echo [ERROR] Build failed. See output above and %LOG%
    call :showElapsed
    echo.
    pause
    exit /b 1
)
echo [1/4] Build done.
echo.

:: ---- 2. Clean old versioned exes ----
echo [2/4] Cleaning old out/ artifacts...
if exist out\AxMusic*.exe (
    del /q out\AxMusic*.exe 2>nul
)
echo [2/4] Clean done.
echo.

:: ---- 3. Versioned copy (prompts y/n if target exe is locked) ----
echo [3/4] Copying artifacts to out\ ...
echo [3/4] dist:copy >> "%LOG%"
call npm run dist:copy
set "COPY_RC=%ERRORLEVEL%"
echo dist:copy exit=%COPY_RC% >> "%LOG%"
if not "%COPY_RC%"=="0" (
    echo.
    echo [ERROR] Copy failed. Close the running exe, then type y in the prompt above.
    echo         Or re-run: npm run dist:copy
    call :showElapsed
    echo.
    pause
    exit /b 1
)

echo.
echo ========================================
echo  Success! Green build in out\
echo  Look for: AxMusic-v*.exe
echo  Log: %LOG%
call :showElapsed
echo ========================================
echo [%date% %time%] success >> "%LOG%"
echo.
pause
exit /b 0

:: ---- Elapsed time (never crash the script) ----
:showElapsed
if not exist "%START_FILE%" (
    echo  Total time: unknown
    exit /b 0
)
powershell -NoProfile -Command ^
  "try { $s=[DateTime]::Parse((Get-Content -Raw '%START_FILE%').Trim()); $d=(Get-Date)-$s; if($d.TotalHours -ge 1){ '  Total time: {0}h {1}m {2}s' -f [int]$d.TotalHours, $d.Minutes, $d.Seconds } elseif($d.TotalMinutes -ge 1){ '  Total time: {0}m {1}s' -f [int]$d.TotalMinutes, $d.Seconds } else { '  Total time: {0:N1}s' -f $d.TotalSeconds } } catch { '  Total time: unknown' }"
del /q "%START_FILE%" 2>nul
exit /b 0
