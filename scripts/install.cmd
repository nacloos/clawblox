@echo off
REM Clawblox installer for Windows CMD
REM Usage: curl -fsSL https://clawblox.com/install.cmd -o install.cmd && install.cmd && del install.cmd

setlocal enabledelayedexpansion

set "REPO=nacloos/clawblox"
set "BINARY_NAME=clawblox"
set "INSTALL_DIR=%LOCALAPPDATA%\clawblox"

echo info: Detecting architecture...

REM Detect architecture
if "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
    set "ARCH=x86_64"
) else if "%PROCESSOR_ARCHITECTURE%"=="ARM64" (
    set "ARCH=aarch64"
) else (
    echo error: Unsupported architecture: %PROCESSOR_ARCHITECTURE%
    exit /b 1
)

echo info: Architecture: %ARCH%

echo info: Fetching latest version...

REM Get latest version using curl and basic parsing
for /f "tokens=*" %%i in ('curl -fsSL "https://api.github.com/repos/%REPO%/releases/latest" ^| findstr /C:"tag_name"') do (
    set "VERSION_LINE=%%i"
)

REM Extract version from JSON line
for /f "tokens=2 delims=:" %%a in ("!VERSION_LINE!") do (
    set "VERSION=%%a"
)
set "VERSION=!VERSION:"=!"
set "VERSION=!VERSION: =!"
set "VERSION=!VERSION:,=!"

if "!VERSION!"=="" (
    echo error: Failed to get latest version
    exit /b 1
)

echo info: Latest version: !VERSION!

set "TARGET=%ARCH%-pc-windows-msvc"
set "URL=https://github.com/%REPO%/releases/download/!VERSION!/%BINARY_NAME%-%TARGET%.zip"

echo info: Downloading from: !URL!

REM Create temp directory
set "TMP_DIR=%TEMP%\clawblox-install-%RANDOM%"
mkdir "!TMP_DIR!"

REM Download
curl -fsSL "!URL!" -o "!TMP_DIR!\%BINARY_NAME%.zip"
if errorlevel 1 (
    echo error: Download failed
    rmdir /s /q "!TMP_DIR!" 2>nul
    exit /b 1
)

REM Extract using tar (available on Windows 10+)
tar -xf "!TMP_DIR!\%BINARY_NAME%.zip" -C "!TMP_DIR!"
if errorlevel 1 (
    echo error: Extraction failed
    rmdir /s /q "!TMP_DIR!" 2>nul
    exit /b 1
)

REM Create install directory
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

REM Install binary
move /y "!TMP_DIR!\%BINARY_NAME%.exe" "%INSTALL_DIR%\%BINARY_NAME%.exe" >nul
if errorlevel 1 (
    echo error: Installation failed
    rmdir /s /q "!TMP_DIR!" 2>nul
    exit /b 1
)

echo info: Installed %BINARY_NAME% to %INSTALL_DIR%\%BINARY_NAME%.exe

REM Cleanup
rmdir /s /q "!TMP_DIR!" 2>nul

REM Check PATH
echo !PATH! | findstr /C:"%INSTALL_DIR%" >nul
if errorlevel 1 (
    echo warn: %INSTALL_DIR% is not in your PATH
    echo.
    echo To add it, run:
    echo   setx PATH "%%PATH%%;%INSTALL_DIR%"
    echo.
)

echo info: Run '%BINARY_NAME% --help' to get started

endlocal
