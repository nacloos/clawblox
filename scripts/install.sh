#!/bin/sh
# Clawblox installer for macOS, Linux, and WSL
# Usage: curl -fsSL https://clawblox.com/install.sh | sh

set -e

REPO="nacloos/clawblox"
BINARY_NAME="clawblox"
INSTALL_DIR="${HOME}/.local/bin"

# Colors (if terminal supports them)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

info() {
    printf "${GREEN}info:${NC} %s\n" "$1"
}

warn() {
    printf "${YELLOW}warn:${NC} %s\n" "$1"
}

error() {
    printf "${RED}error:${NC} %s\n" "$1" >&2
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       error "Unsupported OS: $(uname -s)" ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        arm64|aarch64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

# Get the latest release version from GitHub
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

# Build the download URL
get_download_url() {
    local version="$1"
    local os="$2"
    local arch="$3"

    local target=""
    case "${os}" in
        linux)  target="${arch}-unknown-linux-musl" ;;
        darwin) target="${arch}-apple-darwin" ;;
    esac

    echo "https://github.com/${REPO}/releases/download/${version}/${BINARY_NAME}-${target}.tar.gz"
}

main() {
    info "Detecting system..."

    local os=$(detect_os)
    local arch=$(detect_arch)

    info "OS: ${os}, Architecture: ${arch}"

    info "Fetching latest version..."
    local version=$(get_latest_version)

    if [ -z "$version" ]; then
        error "Failed to get latest version"
    fi

    info "Latest version: ${version}"

    local url=$(get_download_url "$version" "$os" "$arch")
    info "Downloading from: ${url}"

    # Create temp directory
    local tmp_dir=$(mktemp -d)
    trap "rm -rf ${tmp_dir}" EXIT

    # Download and extract
    curl -fsSL "$url" | tar -xz -C "$tmp_dir"

    # Create install directory if needed
    mkdir -p "$INSTALL_DIR"

    # Install binary
    mv "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    info "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"

    # Check if INSTALL_DIR is in PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            ;;
        *)
            warn "${INSTALL_DIR} is not in your PATH"
            echo ""
            echo "Add it to your shell profile:"
            echo ""
            echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
            echo ""
            ;;
    esac

    info "Run '${BINARY_NAME} --help' to get started"
}

main
