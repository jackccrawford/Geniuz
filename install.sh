#!/bin/sh
set -e

REPO="jackccrawford/geniuz"
GENIUZ_HOME="${GENIUZ_HOME:-$HOME/.geniuz}"
INSTALL_DIR="${GENIUZ_HOME}/bin"
LIB_DIR="${GENIUZ_HOME}/lib"

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

URL="https://github.com/${REPO}/releases/download/${LATEST}/geniuz-${NAME}.tar.gz"
SHA_URL="https://github.com/${REPO}/releases/download/${LATEST}/geniuz-${NAME}.tar.gz.sha256"

echo "Installing Geniuz ${LATEST} (${NAME})..."

# Download to temp
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "$TMPDIR/geniuz.tar.gz"
curl -fsSL "$SHA_URL" -o "$TMPDIR/geniuz.sha256" 2>/dev/null || true

# Verify checksum if available
if [ -f "$TMPDIR/geniuz.sha256" ]; then
  EXPECTED=$(cat "$TMPDIR/geniuz.sha256" | awk '{print $1}')
  if command -v sha256sum > /dev/null 2>&1; then
    ACTUAL=$(sha256sum "$TMPDIR/geniuz.tar.gz" | awk '{print $1}')
  elif command -v shasum > /dev/null 2>&1; then
    ACTUAL=$(shasum -a 256 "$TMPDIR/geniuz.tar.gz" | awk '{print $1}')
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

# Extract
mkdir -p "$INSTALL_DIR"
mkdir -p "$TMPDIR/extract"
tar xzf "$TMPDIR/geniuz.tar.gz" -C "$TMPDIR/extract"
cp "$TMPDIR/extract/geniuz" "$INSTALL_DIR/"
chmod +x "${INSTALL_DIR}/geniuz"

# Install geniuz-embed if present
if [ -f "$TMPDIR/extract/geniuz-embed" ]; then
  cp "$TMPDIR/extract/geniuz-embed" "$INSTALL_DIR/"
  chmod +x "${INSTALL_DIR}/geniuz-embed"
fi

# macOS: ad-hoc codesign to clear provenance gate (Sequoia+)
if [ "$os" = "darwin" ] && command -v codesign > /dev/null 2>&1; then
  codesign --force --sign - "${INSTALL_DIR}/geniuz" 2>/dev/null
  [ -f "${INSTALL_DIR}/geniuz-embed" ] && codesign --force --sign - "${INSTALL_DIR}/geniuz-embed" 2>/dev/null
  echo "  Signed for macOS."
fi

# Install bundled ONNX Runtime if present
# Linux: libonnxruntime.so.* | Mac: libonnxruntime.*.dylib
BUNDLED_LIB=""
for f in "$TMPDIR/extract"/libonnxruntime.so.* "$TMPDIR/extract"/libonnxruntime.*.dylib; do
  [ -f "$f" ] && BUNDLED_LIB="$f" && break
done

if [ -n "$BUNDLED_LIB" ]; then
  mkdir -p "$LIB_DIR"
  LIB_NAME=$(basename "$BUNDLED_LIB")
  cp "$BUNDLED_LIB" "$LIB_DIR/"

  if [ "$os" = "linux" ]; then
    # Linux symlinks
    ln -sf "$LIB_NAME" "$LIB_DIR/libonnxruntime.so"
    ln -sf "$LIB_NAME" "$LIB_DIR/libonnxruntime.so.1"
    # Wrapper script for LD_LIBRARY_PATH
    mv "${INSTALL_DIR}/geniuz" "${INSTALL_DIR}/geniuz.bin"
    cat > "${INSTALL_DIR}/geniuz" <<'WRAPPER'
#!/bin/sh
SELF="$0"; while [ -L "$SELF" ]; do SELF="$(readlink "$SELF")"; done
DIR="$(cd "$(dirname "$SELF")" && pwd)"
export LD_LIBRARY_PATH="${DIR}/../lib:${LD_LIBRARY_PATH}"
exec "${DIR}/geniuz.bin" "$@"
WRAPPER
    chmod +x "${INSTALL_DIR}/geniuz"
    # Same for geniuz-embed if present
    if [ -f "${INSTALL_DIR}/geniuz-embed" ]; then
      mv "${INSTALL_DIR}/geniuz-embed" "${INSTALL_DIR}/geniuz-embed.bin"
      cat > "${INSTALL_DIR}/geniuz-embed" <<'WRAPPER'
#!/bin/sh
SELF="$0"; while [ -L "$SELF" ]; do SELF="$(readlink "$SELF")"; done
DIR="$(cd "$(dirname "$SELF")" && pwd)"
export LD_LIBRARY_PATH="${DIR}/../lib:${LD_LIBRARY_PATH}"
exec "${DIR}/geniuz-embed.bin" "$@"
WRAPPER
      chmod +x "${INSTALL_DIR}/geniuz-embed"
    fi
  else
    # macOS: copy both versioned and unversioned dylib
    for f in "$TMPDIR/extract"/libonnxruntime*.dylib; do
      [ -f "$f" ] && cp "$f" "$LIB_DIR/"
    done
    # Wrapper script for DYLD_LIBRARY_PATH
    mv "${INSTALL_DIR}/geniuz" "${INSTALL_DIR}/geniuz.bin"
    cat > "${INSTALL_DIR}/geniuz" <<'WRAPPER'
#!/bin/sh
SELF="$0"; while [ -L "$SELF" ]; do SELF="$(readlink "$SELF")"; done
DIR="$(cd "$(dirname "$SELF")" && pwd)"
export DYLD_LIBRARY_PATH="${DIR}/../lib:${DYLD_LIBRARY_PATH}"
exec "${DIR}/geniuz.bin" "$@"
WRAPPER
    chmod +x "${INSTALL_DIR}/geniuz"
    if [ -f "${INSTALL_DIR}/geniuz-embed" ]; then
      mv "${INSTALL_DIR}/geniuz-embed" "${INSTALL_DIR}/geniuz-embed.bin"
      cat > "${INSTALL_DIR}/geniuz-embed" <<'WRAPPER'
#!/bin/sh
SELF="$0"; while [ -L "$SELF" ]; do SELF="$(readlink "$SELF")"; done
DIR="$(cd "$(dirname "$SELF")" && pwd)"
export DYLD_LIBRARY_PATH="${DIR}/../lib:${DYLD_LIBRARY_PATH}"
exec "${DIR}/geniuz-embed.bin" "$@"
WRAPPER
      chmod +x "${INSTALL_DIR}/geniuz-embed"
    fi
  fi
  echo "  Bundled ONNX Runtime installed."
fi

# Symlink to a PATH location
SYMLINK_DIR=""
if [ -d "$HOME/.local/bin" ]; then
  SYMLINK_DIR="$HOME/.local/bin"
elif [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
  SYMLINK_DIR="/usr/local/bin"
fi

if [ -n "$SYMLINK_DIR" ]; then
  ln -sf "${INSTALL_DIR}/geniuz" "$SYMLINK_DIR/geniuz"
  if [ -f "${INSTALL_DIR}/geniuz-embed" ]; then
    ln -sf "${INSTALL_DIR}/geniuz-embed" "$SYMLINK_DIR/geniuz-embed"
  fi
  echo "  Linked to ${SYMLINK_DIR}/geniuz"
fi

# Verify CLI
if ! "${INSTALL_DIR}/geniuz" --version > /dev/null 2>&1; then
  echo "Installation failed" >&2
  exit 1
fi
VERSION=$("${INSTALL_DIR}/geniuz" --version)

echo ""
echo "  Installed CLI: ${VERSION}"
echo "  Location:      ${GENIUZ_HOME}/"

# Dashboard install (Linux only, graphical session only)
DASHBOARD_INSTALLED=""
DASHBOARD_SKIPPED_REASON=""
if [ "$os" = "linux" ]; then
  if [ -n "$GENIUZ_NO_DASHBOARD" ]; then
    DASHBOARD_SKIPPED_REASON="GENIUZ_NO_DASHBOARD set"
  elif [ -z "$DISPLAY" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    DASHBOARD_SKIPPED_REASON="no graphical session ($DISPLAY/$WAYLAND_DISPLAY unset)"
  elif [ "$arch" != "amd64" ]; then
    DASHBOARD_SKIPPED_REASON="dashboard package not built for ${arch} yet"
  else
    # Pick package format
    DEB_URL="https://github.com/${REPO}/releases/download/${LATEST}/Geniuz_${LATEST#v}_amd64.deb"
    RPM_URL="https://github.com/${REPO}/releases/download/${LATEST}/Geniuz-${LATEST#v}-1.x86_64.rpm"
    APPIMAGE_URL="https://github.com/${REPO}/releases/download/${LATEST}/Geniuz_${LATEST#v}_amd64.AppImage"

    if command -v dpkg > /dev/null 2>&1 && command -v apt-get > /dev/null 2>&1; then
      PKG_FMT="deb"; PKG_URL="$DEB_URL"
    elif command -v rpm > /dev/null 2>&1 && (command -v dnf > /dev/null 2>&1 || command -v yum > /dev/null 2>&1); then
      PKG_FMT="rpm"; PKG_URL="$RPM_URL"
    else
      PKG_FMT="appimage"; PKG_URL="$APPIMAGE_URL"
    fi

    echo ""
    echo "  Dashboard:     fetching ${PKG_FMT} (${LATEST})..."
    DASH_TMP=$(mktemp -d)
    DASH_FILE="${DASH_TMP}/$(basename "$PKG_URL")"
    if ! curl -fsSL "$PKG_URL" -o "$DASH_FILE"; then
      echo "  Dashboard:     not published for ${LATEST} yet — CLI + TUI installed and ready."
      echo "                 Desktop dashboard: https://geniuz.life  ·  https://github.com/${REPO}/releases"
      DASHBOARD_SKIPPED_REASON="dashboard package not attached to ${LATEST}"
      rm -rf "$DASH_TMP"
    else
      echo ""
      echo "  Installing the dashboard requires sudo. You'll be prompted for your password."
      echo ""
      case "$PKG_FMT" in
        deb)
          if sudo dpkg -i "$DASH_FILE" 2>/dev/null; then
            DASHBOARD_INSTALLED=1
          else
            # Resolve missing deps and retry
            sudo apt-get install -f -y >/dev/null 2>&1 || true
            sudo dpkg -i "$DASH_FILE" && DASHBOARD_INSTALLED=1 || \
              DASHBOARD_SKIPPED_REASON="dpkg install failed"
          fi
          ;;
        rpm)
          if command -v dnf > /dev/null 2>&1; then
            sudo dnf install -y "$DASH_FILE" && DASHBOARD_INSTALLED=1 || \
              DASHBOARD_SKIPPED_REASON="dnf install failed"
          else
            sudo yum install -y "$DASH_FILE" && DASHBOARD_INSTALLED=1 || \
              DASHBOARD_SKIPPED_REASON="yum install failed"
          fi
          ;;
        appimage)
          mkdir -p "${GENIUZ_HOME}/apps"
          APPIMAGE_DEST="${GENIUZ_HOME}/apps/geniuz-dashboard.AppImage"
          cp "$DASH_FILE" "$APPIMAGE_DEST"
          chmod +x "$APPIMAGE_DEST"
          # Symlink into /usr/local/bin so `geniuz dashboard` resolves it
          if sudo ln -sf "$APPIMAGE_DEST" /usr/local/bin/geniuz-dashboard; then
            DASHBOARD_INSTALLED=1
          else
            DASHBOARD_SKIPPED_REASON="symlink to /usr/local/bin failed"
          fi
          ;;
      esac
      rm -rf "$DASH_TMP"
    fi
  fi
fi

echo ""
echo "  Next steps:"

# CLI run hint
if command -v geniuz > /dev/null 2>&1; then
  echo "    Run the CLI:        geniuz"
elif [ -n "$SYMLINK_DIR" ]; then
  case ":$PATH:" in
    *":${SYMLINK_DIR}:"*) echo "    Run the CLI:        geniuz" ;;
    *) echo "    Run the CLI:        geniuz  (after: export PATH=\"${SYMLINK_DIR}:\$PATH\")" ;;
  esac
else
  echo "    Run the CLI:        geniuz  (after: export PATH=\"${INSTALL_DIR}:\$PATH\")"
fi

# Dashboard hint
if [ "$os" = "linux" ]; then
  if [ -n "$DASHBOARD_INSTALLED" ]; then
    echo "    Open the dashboard: geniuz dashboard"
  elif [ -n "$DASHBOARD_SKIPPED_REASON" ]; then
    echo "    Dashboard skipped:  ${DASHBOARD_SKIPPED_REASON}"
    echo "                        Install later: re-run with a graphical session,"
    echo "                        or grab the package from https://github.com/${REPO}/releases/latest"
  fi
elif [ "$os" = "darwin" ]; then
  if [ -d "/Applications/Geniuz.app" ]; then
    echo "    Open the dashboard: geniuz dashboard  (or open Geniuz in /Applications)"
  else
    echo "    Dashboard for Mac:  download Geniuz.dmg from https://geniuz.life"
  fi
fi

echo ""
echo "    First memory:       geniuz remember -c \"Hello from Geniuz\" -g \"first memory\""
