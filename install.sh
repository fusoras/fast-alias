#!/bin/sh
# POSIX-compliant bootstrap installer for fast-alias (`fa`)
# Usage: curl -fsSL https://raw.githubusercontent.com/fusoras/fast-alias/develop/install.sh | sh

set -e

# Default version tag (overridden if RELEASE_TAG environment variable is set)
VERSION="${RELEASE_TAG:-v0.1.0-beta.2}"
REPO="fusoras/fast-alias"

# Colors for terminal output
BOLD_GREEN="\033[1;32m"
BOLD_RED="\033[1;31m"
BOLD_CYAN="\033[1;36m"
RESET="\033[0m"

log_info() {
    printf "%b==>%b %s\n" "$BOLD_CYAN" "$RESET" "$1"
}

log_success() {
    printf "%b==>%b %s\n" "$BOLD_GREEN" "$RESET" "$1"
}

log_error() {
    printf "%bERROR:%b %s\n" "$BOLD_RED" "$RESET" "$1" >&2
}

# Pre-flight check: required commands
for cmd in curl tar; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log_error "Required command '$cmd' is not installed."
        exit 1
    fi
done

# Detect Architecture
ARCH_RAW=$(uname -m)
case "$ARCH_RAW" in
    x86_64|amd64)
        ARCH="x86_64"
        ASSET_TARGET="fa-x86_64-unknown-linux-gnu.tar.gz"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ASSET_TARGET="fa-aarch64-unknown-linux-musl.tar.gz"
        ;;
    *)
        log_error "Unsupported architecture: $ARCH_RAW. Supported architectures are x86_64 and aarch64."
        exit 1
        ;;
esac

# Detect Platform and Install Path
if [ -n "$TERMUX_VERSION" ] || [ -d "/data/data/com.termux" ]; then
    PLATFORM="Termux"
    if [ -n "$PREFIX" ] && [ -d "$PREFIX/bin" ]; then
        BIN_DIR="$PREFIX/bin"
    else
        BIN_DIR="$HOME/.local/bin"
    fi
else
    PLATFORM="Debian/Linux"
    BIN_DIR="$HOME/.local/bin"
fi

log_info "Detected platform: $PLATFORM ($ARCH)"
log_info "Target installation path: $BIN_DIR/fa"

# Create temporary directory for download
TMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t 'fa-install')
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET_TARGET"
log_info "Downloading binary asset from: $DOWNLOAD_URL"

if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_TARGET"; then
    # Fallback to latest tag release download
    FALLBACK_URL="https://github.com/$REPO/releases/latest/download/$ASSET_TARGET"
    log_info "Attempting fallback download from: $FALLBACK_URL"
    if ! curl -fsSL "$FALLBACK_URL" -o "$TMP_DIR/$ASSET_TARGET"; then
        log_error "Failed to download release asset from GitHub."
        exit 1
    fi
fi

# Extract archive
tar -xzf "$TMP_DIR/$ASSET_TARGET" -C "$TMP_DIR"

if [ ! -f "$TMP_DIR/fa" ]; then
    log_error "Extracted archive does not contain binary 'fa'."
    exit 1
fi

# Ensure bin directory exists
mkdir -p "$BIN_DIR"

# Install binary
cp "$TMP_DIR/fa" "$BIN_DIR/fa"
chmod +x "$BIN_DIR/fa"

log_success "Successfully installed fa ($VERSION) to $BIN_DIR/fa!"

# PATH Warning if BIN_DIR is not in PATH
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        log_info "Make sure $BIN_DIR is added to your PATH environment variable:"
        printf "  export PATH=\"%s:\$PATH\"\n" "$BIN_DIR"
        ;;
esac
