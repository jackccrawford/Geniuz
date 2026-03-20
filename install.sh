#!/bin/sh
set -e

REPO="jackccrawford/clawmark"
INSTALL_DIR="${CLAWMARK_INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="linux" ;;
  Darwin) os="darwin" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch="amd64" ;;
  aarch64|arm64) arch="arm64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

NAME="${os}-${arch}"

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
if [ -z "$LATEST" ]; then
  echo "Failed to fetch latest release" >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${LATEST}/clawmark-${NAME}.tar.gz"
SHA_URL="https://github.com/${REPO}/releases/download/${LATEST}/clawmark-${NAME}.tar.gz.sha256"

echo "Installing clawmark ${LATEST} (${NAME})..."

# Download to temp
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "$TMPDIR/clawmark.tar.gz"
curl -fsSL "$SHA_URL" -o "$TMPDIR/clawmark.sha256" 2>/dev/null || true

# Verify checksum if available
if [ -f "$TMPDIR/clawmark.sha256" ]; then
  EXPECTED=$(cat "$TMPDIR/clawmark.sha256" | awk '{print $1}')
  if command -v sha256sum > /dev/null 2>&1; then
    ACTUAL=$(sha256sum "$TMPDIR/clawmark.tar.gz" | awk '{print $1}')
  elif command -v shasum > /dev/null 2>&1; then
    ACTUAL=$(shasum -a 256 "$TMPDIR/clawmark.tar.gz" | awk '{print $1}')
  else
    echo "  Warning: no sha256sum or shasum found, skipping verification" >&2
    ACTUAL="$EXPECTED"
  fi

  if [ "$ACTUAL" != "$EXPECTED" ]; then
    echo "Checksum verification failed!" >&2
    echo "  Expected: $EXPECTED" >&2
    echo "  Got:      $ACTUAL" >&2
    exit 1
  fi
  echo "  Checksum verified."
fi

# Extract and install
mkdir -p "$INSTALL_DIR"
tar xzf "$TMPDIR/clawmark.tar.gz" -C "$INSTALL_DIR"
chmod +x "${INSTALL_DIR}/clawmark"

# Verify
if "${INSTALL_DIR}/clawmark" --version > /dev/null 2>&1; then
  VERSION=$("${INSTALL_DIR}/clawmark" --version)
  echo ""
  echo "  Installed: ${VERSION}"
  echo "  Location:  ${INSTALL_DIR}/clawmark"
  echo ""
  # Check PATH
  case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *) echo "  Add to PATH: export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
  esac
  echo "  Next: clawmark signal -c \"Hello from clawmark\" -g \"first signal\""
else
  echo "Installation failed" >&2
  exit 1
fi
