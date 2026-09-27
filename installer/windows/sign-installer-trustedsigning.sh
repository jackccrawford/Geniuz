#!/usr/bin/env bash
# sign-installer-trustedsigning.sh — Sign Windows binaries via Azure Trusted
# Signing using jsign on macOS. No hardware token, no Windows host required.
#
# Usage:
#   ./sign-installer-trustedsigning.sh INPUT.exe [OUTPUT.exe]
#
# If OUTPUT.exe is omitted, the input is signed in place.
#
# Requires:
#   - Homebrew: jsign, azure-cli, (osslsigncode for verify step)
#   - `az login` completed as jcc@managedv.com (or as a service principal
#     with the "Trusted Signing Certificate Profile Signer" role)
#   - A completed Identity Validation and an active Certificate Profile in
#     the Trusted Signing Account
#
# Token lifecycle:
#   A fresh access token is fetched on every invocation via
#   `az account get-access-token --resource https://codesigning.azure.net`.
#   Tokens expire in ~60 min; nothing is cached on disk.

set -euo pipefail

# --- Tunables (override via env if needed) ---
TS_ENDPOINT="${GENIUZ_TS_ENDPOINT:-https://wus2.codesigning.azure.net}"
TS_ACCOUNT="${GENIUZ_TS_ACCOUNT:-MVLLC}"
TS_PROFILE="${GENIUZ_TS_PROFILE:-geniuz-free-prod}"
TIMESTAMP_URL="${GENIUZ_SIGN_TIMESTAMP_URL:-http://timestamp.acs.microsoft.com/}"
DESCRIPTION="${GENIUZ_SIGN_DESCRIPTION:-Geniuz}"
INFO_URL="${GENIUZ_SIGN_INFO_URL:-https://geniuz.life}"

# --- Args ---
INPUT="${1:-}"
OUTPUT="${2:-}"

if [[ -z "$INPUT" ]]; then
  cat >&2 <<EOF
Usage: $0 INPUT.exe [OUTPUT.exe]

If OUTPUT.exe is omitted, INPUT.exe is signed in place.

Examples:
  $0 installer/windows/output/Geniuz-Setup.exe
  $0 build/geniuz.exe build/geniuz-signed.exe

Environment overrides:
  GENIUZ_TS_ENDPOINT  (default: $TS_ENDPOINT)
  GENIUZ_TS_ACCOUNT   (default: $TS_ACCOUNT)
  GENIUZ_TS_PROFILE   (default: $TS_PROFILE)
EOF
  exit 1
fi

if [[ ! -f "$INPUT" ]]; then
  echo "Error: input file not found: $INPUT" >&2
  exit 1
fi

IN_PLACE=false
if [[ -z "$OUTPUT" ]]; then
  IN_PLACE=true
  OUTPUT="${INPUT}.signed.tmp"
fi

# --- Preflight ---
echo "→ Verifying signing tools..."
for tool in jsign az; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Error: $tool not found in PATH." >&2
    echo "Install via: brew install jsign azure-cli" >&2
    exit 1
  fi
done

echo "→ Verifying Azure login..."
if ! az account show >/dev/null 2>&1; then
  echo "Error: not logged into Azure. Run: az login" >&2
  exit 1
fi

AZ_ACCOUNT_NAME="$(az account show --query user.name -o tsv)"
echo "  Signed in as: $AZ_ACCOUNT_NAME"

echo "→ Fetching access token for Trusted Signing..."
AZURE_ACCESS_TOKEN="$(az account get-access-token \
  --resource https://codesigning.azure.net \
  --query accessToken -o tsv)"

if [[ -z "$AZURE_ACCESS_TOKEN" ]]; then
  echo "Error: failed to fetch access token." >&2
  exit 1
fi

# Copy input to output path if not signing in place, since jsign signs
# the file at the path you give it. (osslsigncode took -in/-out; jsign
# operates on the file directly.)
if [[ "$IN_PLACE" == false ]]; then
  cp "$INPUT" "$OUTPUT"
fi
TARGET="$OUTPUT"
if [[ "$IN_PLACE" == true ]]; then
  TARGET="$INPUT"
fi

# --- Sign ---
echo "→ Signing $TARGET via Azure Trusted Signing..."
echo "  Account/Profile: $TS_ACCOUNT/$TS_PROFILE"
echo "  Endpoint:        $TS_ENDPOINT"

jsign \
  --storetype TRUSTEDSIGNING \
  --keystore "$TS_ENDPOINT" \
  --storepass "$AZURE_ACCESS_TOKEN" \
  --alias "$TS_ACCOUNT/$TS_PROFILE" \
  --tsaurl "$TIMESTAMP_URL" \
  --tsmode RFC3161 \
  --name "$DESCRIPTION" \
  --url "$INFO_URL" \
  "$TARGET"

# --- Verify ---
echo
echo "→ Verifying signature..."
if command -v osslsigncode >/dev/null 2>&1; then
  # Informational only. The Mac's osslsigncode lacks Microsoft's roots and
  # reports "Failed" on a good signature; under pipefail that aborted
  # sign-binaries.sh after its first file. The authoritative check is
  # `signtool verify /pa /v` on Windows.
  osslsigncode verify -in "$TARGET" 2>&1 \
    | grep -E 'Subject:|Issuer:|Signing time|Timestamp|Number of verified|Succeeded|Failed' \
    | sed 's/^/  /' || true
else
  echo "  (osslsigncode not installed; skipping local verify."
  echo "   Install with: brew install osslsigncode"
  echo "   Or verify on Windows: signtool verify /pa /v $TARGET)"
fi

echo
echo "✅ Signed: $TARGET"
