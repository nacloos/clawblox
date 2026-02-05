@echo off
setlocal enabledelayedexpansion

set "RELEASES_URL=https://releases.clawblox.com"
set "DOWNLOAD_DIR=%USERPROFILE%\.clawblox\downloads"

:: Detect architecture
if "%PROCESSOR_ARCHITECTURE%"=="ARM64" (
    set "PLATFORM=win32-arm64"
) else (
    set "PLATFORM=win32-x64"
)

if not exist "%DOWNLOAD_DIR%" mkdir "%DOWNLOAD_DIR%"

echo Detecting platform: %PLATFORM%

:: Get latest version
curl -fsSL "%RELEASES_URL%/latest" -o "%DOWNLOAD_DIR%\version.txt"
if errorlevel 1 (
    echo Error: Failed to get latest version
    exit /b 1
)
set /p VERSION=<"%DOWNLOAD_DIR%\version.txt"
echo Latest version: %VERSION%

:: Download binary
echo Downloading clawblox...
set "BINARY_PATH=%DOWNLOAD_DIR%\clawblox-%VERSION%-%PLATFORM%.exe"
curl -fsSL "%RELEASES_URL%/%VERSION%/%PLATFORM%/clawblox.exe" -o "%BINARY_PATH%"
if errorlevel 1 (
    echo Error: Failed to download binary
    exit /b 1
)

:: Run installer
echo Installing...
"%BINARY_PATH%" install

:: Cleanup
del "%BINARY_PATH%" 2>nul
del "%DOWNLOAD_DIR%\version.txt" 2>nul

echo.
echo Installation complete!
echo Run 'clawblox --help' to get started.
