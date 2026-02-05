# Clawblox installer for Windows PowerShell
# Usage: irm https://clawblox.com/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "nacloos/clawblox"
$BinaryName = "clawblox"
$InstallDir = "$env:LOCALAPPDATA\clawblox"

function Write-Info {
    param([string]$Message)
    Write-Host "info: " -ForegroundColor Green -NoNewline
    Write-Host $Message
}

function Write-Warn {
    param([string]$Message)
    Write-Host "warn: " -ForegroundColor Yellow -NoNewline
    Write-Host $Message
}

function Write-Error {
    param([string]$Message)
    Write-Host "error: " -ForegroundColor Red -NoNewline
    Write-Host $Message
    exit 1
}

function Get-Architecture {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64" { return "x86_64" }
        "Arm64" { return "aarch64" }
        default { Write-Error "Unsupported architecture: $arch" }
    }
}

function Get-LatestVersion {
    $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "clawblox-installer" }
    return $response.tag_name
}

function Get-DownloadUrl {
    param(
        [string]$Version,
        [string]$Arch
    )
    $target = "$Arch-pc-windows-msvc"
    return "https://github.com/$Repo/releases/download/$Version/$BinaryName-$target.zip"
}

function Main {
    Write-Info "Detecting system..."

    $arch = Get-Architecture
    Write-Info "Architecture: $arch"

    Write-Info "Fetching latest version..."
    $version = Get-LatestVersion

    if (-not $version) {
        Write-Error "Failed to get latest version"
    }

    Write-Info "Latest version: $version"

    $url = Get-DownloadUrl -Version $version -Arch $arch
    Write-Info "Downloading from: $url"

    # Create temp directory
    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    try {
        $zipPath = Join-Path $tmpDir "$BinaryName.zip"

        # Download
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

        # Extract
        Expand-Archive -Path $zipPath -DestinationPath $tmpDir -Force

        # Create install directory
        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir | Out-Null
        }

        # Install binary
        $binaryPath = Join-Path $InstallDir "$BinaryName.exe"
        Move-Item -Path (Join-Path $tmpDir "$BinaryName.exe") -Destination $binaryPath -Force

        Write-Info "Installed $BinaryName to $binaryPath"

        # Check if InstallDir is in PATH
        $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        if ($userPath -notlike "*$InstallDir*") {
            Write-Warn "$InstallDir is not in your PATH"
            Write-Host ""
            Write-Host "Adding to PATH..."

            $newPath = "$InstallDir;$userPath"
            [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
            $env:PATH = "$InstallDir;$env:PATH"

            Write-Info "Added $InstallDir to PATH"
            Write-Host ""
            Write-Host "Note: Restart your terminal for PATH changes to take effect"
        }

        Write-Info "Run '$BinaryName --help' to get started"
    }
    finally {
        # Cleanup
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Main
