#!/usr/bin/env bash
# sign-binaries.sh — Sign Geniuz inner Windows binaries before they get
# packaged by Inno Setup. Default signer: Azure Trusted Signing
# (sign-installer-trustedsigning.sh). Fallback: the YubiKey EV path
# (GENIUZ_SIGN_CMD=./sign-installer.sh), which prompts for the PIN once per file.
#
# Usage:
#   ./sign-binaries.sh STAGING-DIR
#
# Expects the staging dir to contain unsigned cross-compiled binaries:
#   geniuz.exe
#   geniuz-embed.exe
#   geniuz-dashboard.exe   (the tray app; built by desktop/dashboard)
#
# Signs each in place. Run this BEFORE copying the staging dir to a Windows
# host for ISCC bundling, so the binaries packed inside Geniuz-Setup.exe are
# already signed. After ISCC produces the outer installer, sign that with the
# same signer (dual-signing: inner binaries signed + outer installer signed).

set -euo pipefail

STAGING="${1:-}"

if [[ -z "$STAGING" ]]; then
  cat >&2 <<EOF
Usage: $0 STAGING-DIR

Signs the three Geniuz inner binaries (geniuz.exe, geniuz-embed.exe,
geniuz-dashboard.exe) in place using \$GENIUZ_SIGN_CMD
(default: sign-installer-trustedsigning.sh). Run before transferring
the staging dir to a Windows host for Inno Setup bundling.
EOF
  exit 1
fi

if [[ ! -d "$STAGING" ]]; then
  echo "Error: staging dir not found: $STAGING" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIGN_CMD="${GENIUZ_SIGN_CMD:-${SCRIPT_DIR}/sign-installer-trustedsigning.sh}"

if [[ ! -x "$SIGN_CMD" ]]; then
  echo "Error: signer not executable: $SIGN_CMD" >&2
  exit 1
fi

# Must match the [Files] Source entries in Geniuz.iss.
BINARIES=(geniuz.exe geniuz-embed.exe geniuz-dashboard.exe)

# Preflight: confirm all binaries exist before signing any of them.
for bin in "${BINARIES[@]}"; do
  path="$STAGING/$bin"
  if [[ ! -f "$path" ]]; then
    echo "Error: missing binary: $path" >&2
    exit 1
  fi
done

echo "→ Signing ${#BINARIES[@]} inner binaries in $STAGING with $(basename "$SIGN_CMD")"

for bin in "${BINARIES[@]}"; do
  echo
  echo "=== $bin ==="
  "$SIGN_CMD" "$STAGING/$bin"
done

echo
echo "✅ All inner binaries signed."
echo "   Next: copy $STAGING to the Windows host and run ISCC.exe Geniuz.iss"
echo "   Then sign the resulting Geniuz-Setup.exe with $(basename "$SIGN_CMD")"
