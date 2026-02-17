#!/bin/bash
set -e

RELEASES_URL="https://releases.clawblox.com"
DOWNLOAD_DIR="$HOME/.clawblox/downloads"

# Check for curl or wget
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    echo "Error: curl or wget is required" >&2
    exit 1
fi

download() {
    local url="$1"
    local output="$2"
    if [ "$DOWNLOADER" = "curl" ]; then
        if [ -n "$output" ]; then
            curl -fsSL -o "$output" "$url"
        else
            curl -fsSL "$url"
        fi
    else
        if [ -n "$output" ]; then
            wget -q -O "$output" "$url"
        else
            wget -q -O - "$url"
        fi
    fi
}

# Detect platform
case "$(uname -s)" in
    Darwin) os="darwin" ;;
    Linux) os="linux" ;;
    *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64|amd64) arch="x64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

platform="${os}-${arch}"
mkdir -p "$DOWNLOAD_DIR"

echo "Detecting platform: $platform"

# Get latest version
version=$(download "$RELEASES_URL/latest")
echo "Latest version: $version"

# Get manifest and extract checksum
manifest=$(download "$RELEASES_URL/$version/manifest.json")
checksum=$(echo "$manifest" | grep -A1 "\"$platform\"" | grep "checksum" | sed 's/.*: *"\([^"]*\)".*/\1/')

if [ -z "$checksum" ]; then
    echo "Error: Platform $platform not found in manifest" >&2
    exit 1
fi

# Download binary
binary_path="$DOWNLOAD_DIR/clawblox-$version-$platform"
echo "Downloading clawblox..."
download "$RELEASES_URL/$version/$platform/clawblox" "$binary_path"

# Verify checksum
if [ "$(uname -s)" = "Darwin" ]; then
    actual=$(shasum -a 256 "$binary_path" | cut -d' ' -f1)
else
    actual=$(sha256sum "$binary_path" | cut -d' ' -f1)
fi

if [ "$actual" != "$checksum" ]; then
    echo "Error: Checksum verification failed" >&2
    echo "  Expected: '$checksum'" >&2
    echo "  Actual:   '$actual'" >&2
    rm -f "$binary_path"
    exit 1
fi

chmod +x "$binary_path"

# Run installer
echo "Installing..."
"$binary_path" install

# Cleanup
rm -f "$binary_path"

echo ""
echo "Installation complete!"
echo "Run 'clawblox --help' to get started."
