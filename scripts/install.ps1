$ErrorActionPreference = "Stop"
$ProgressPreference = 'SilentlyContinue'

$ReleasesUrl = "https://releases.clawblox.com"
$DownloadDir = "$env:USERPROFILE\.clawblox\downloads"

# Detect architecture
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Arch -eq "Arm64") {
    $Platform = "win32-arm64"
} else {
    $Platform = "win32-x64"
}

# Create download directory
New-Item -ItemType Directory -Force -Path $DownloadDir | Out-Null

Write-Output "Detecting platform: $Platform"

# Get latest version
try {
    $Version = Invoke-RestMethod -Uri "$ReleasesUrl/latest"
    Write-Output "Latest version: $Version"
} catch {
    Write-Error "Failed to get latest version: $_"
    exit 1
}

# Get manifest
try {
    $Manifest = Invoke-RestMethod -Uri "$ReleasesUrl/$Version/manifest.json"
    $Checksum = $Manifest.platforms.$Platform.checksum.Trim()

    if (-not $Checksum) {
        Write-Error "Platform $Platform not found in manifest"
        exit 1
    }
} catch {
    Write-Error "Failed to get manifest: $_"
    exit 1
}

# Download binary
$BinaryPath = "$DownloadDir\clawblox-$Version-$Platform.exe"
Write-Output "Downloading clawblox..."

try {
    Invoke-WebRequest -Uri "$ReleasesUrl/$Version/$Platform/clawblox.exe" -OutFile $BinaryPath
} catch {
    Write-Error "Failed to download: $_"
    exit 1
}

# Verify checksum
$ActualChecksum = (Get-FileHash -Path $BinaryPath -Algorithm SHA256).Hash.ToLower()

if ($ActualChecksum -ne $Checksum) {
    Write-Output "Checksum verification failed!"
    Write-Output "  Expected: '$Checksum'"
    Write-Output "  Actual:   '$ActualChecksum'"
    Remove-Item -Force $BinaryPath
    exit 1
}

# Run installer
Write-Output "Installing..."
try {
    & $BinaryPath install
} finally {
    Start-Sleep -Seconds 1
    Remove-Item -Force $BinaryPath -ErrorAction SilentlyContinue
}

# Refresh PATH in the current session so the command works immediately
$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$MachinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$env:PATH = "$UserPath;$MachinePath"

Write-Output ""
Write-Output "Installation complete!"
Write-Output "Run 'clawblox --help' to get started."
