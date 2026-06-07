#!/bin/bash
# ship.sh — build, sign, notarize, staple, package Geniuz.dmg
# Produces: build/Geniuz.dmg — ready to upload to GitHub Releases.
# Assumes: Developer ID Application cert in Keychain, AC_PASSWORD notary profile.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/desktop"
CLI_ARM64="$ROOT/target/aarch64-apple-darwin/release/geniuz"
DASHBOARD_DIR="$DESKTOP/dashboard"
DASHBOARD_APP="$DASHBOARD_DIR/target/release/bundle/macos/Geniuz.app"
ARCHIVE="$DESKTOP/build/Geniuz.xcarchive"
EXPORT_DIR="$DESKTOP/build/export"
APP="$EXPORT_DIR/Geniuz.app"
DMG="$DESKTOP/build/Geniuz.dmg"
DMG_STAGING="$DESKTOP/build/dmg-staging"
DMG_RW="$DESKTOP/build/Geniuz-rw.dmg"

# Polish assets — dark background + orange-dot volume icon
MAC_ASSETS="$ROOT/installer/mac"
DMG_BG="$MAC_ASSETS/dmg-background.png"
DMG_ICON="$MAC_ASSETS/VolumeIcon.icns"

IDENTITY="Developer ID Application: Managed Ventures LLC (NT5SU826F4)"
TEAM_ID="NT5SU826F4"
NOTARY_PROFILE="AC_PASSWORD"

echo "==> Step 1/10: Build arm64 Rust CLI"
cd "$ROOT"
cargo build --release --target aarch64-apple-darwin

echo "==> Step 2/10: Build Tauri dashboard (Geniuz.app via cargo tauri build)"
cd "$DASHBOARD_DIR"
cargo tauri build
test -d "$DASHBOARD_APP" || {
    echo "FATAL: Tauri Geniuz.app not found at $DASHBOARD_APP"
    exit 1
}

echo "==> Step 3/10: xcodebuild archive (Release, universal Swift)"
cd "$DESKTOP"
rm -rf "$ARCHIVE" "$EXPORT_DIR"
xcodebuild archive \
    -project Geniuz.xcodeproj \
    -scheme Geniuz \
    -configuration Release \
    -archivePath "$ARCHIVE" \
    -destination 'generic/platform=macOS' \
    SKIP_INSTALL=NO \
    | tail -5

ARCHIVED_APP="$ARCHIVE/Products/Applications/Geniuz.app"

echo "==> Step 4/10: Inject Rust CLI into archived .app's Resources/"
cp "$CLI_ARM64" "$ARCHIVED_APP/Contents/Resources/geniuz"
chmod +x "$ARCHIVED_APP/Contents/Resources/geniuz"

echo "==> Step 5/10: Nest Tauri dashboard inside menubar .app's Resources/"
rm -rf "$ARCHIVED_APP/Contents/Resources/Geniuz.app"
cp -R "$DASHBOARD_APP" "$ARCHIVED_APP/Contents/Resources/Geniuz.app"

echo "==> Step 6/10: Sign nested Tauri dashboard (inside-out signing)"
# Sign the dashboard binary first, then the nested .app
codesign --force --options runtime --timestamp \
    --sign "$IDENTITY" \
    "$ARCHIVED_APP/Contents/Resources/Geniuz.app/Contents/MacOS/geniuz-dashboard"
codesign --force --options runtime --timestamp \
    --sign "$IDENTITY" \
    "$ARCHIVED_APP/Contents/Resources/Geniuz.app"

echo "==> Step 7/10: Re-sign the outer .app (CLI + dashboard injection invalidated outer signature)"
codesign --force --options runtime --timestamp \
    --sign "$IDENTITY" \
    "$ARCHIVED_APP/Contents/Resources/geniuz"
codesign --force --options runtime --timestamp \
    --sign "$IDENTITY" \
    --entitlements "$DESKTOP/Geniuz/Geniuz.entitlements" \
    "$ARCHIVED_APP"

echo "==> Step 8/10: Export signed .app to export dir"
mkdir -p "$EXPORT_DIR"
cp -R "$ARCHIVE/Products/Applications/Geniuz.app" "$APP"

echo "==> Step 9/10: Notarize .app (submit → wait → staple)"
APP_ZIP="$DESKTOP/build/Geniuz.app.zip"
rm -f "$APP_ZIP"
ditto -c -k --keepParent "$APP" "$APP_ZIP"
xcrun notarytool submit "$APP_ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"

echo "==> Step 10/10: Build DMG (custom layout) + sign + notarize + staple"
rm -rf "$DMG_STAGING" "$DMG" "$DMG_RW"
mkdir -p "$DMG_STAGING"
cp -R "$APP" "$DMG_STAGING/"
ln -s /Applications "$DMG_STAGING/Applications"
mkdir -p "$DMG_STAGING/.background"
cp "$DMG_BG" "$DMG_STAGING/.background/background.png"
cp "$DMG_ICON" "$DMG_STAGING/.VolumeIcon.icns"

# Build a read-write DMG so Finder can apply layout attributes, then convert to UDZO
hdiutil create -volname "Geniuz" \
    -srcfolder "$DMG_STAGING" \
    -ov -format UDRW \
    -fs HFS+ \
    "$DMG_RW"

MOUNT_POINT="$(hdiutil attach -readwrite -noverify -noautoopen "$DMG_RW" | \
    grep -E '^/dev/' | tail -1 | awk '{print $3}')"
echo "    mounted at: $MOUNT_POINT"

# Tell Finder to use the custom volume icon
SetFile -a C "$MOUNT_POINT"

# Give Finder time to register the new volume by name. Without this, the
# AppleScript 'tell disk "Geniuz"' below fails with error -1728 because
# Finder hasn't yet indexed the mount under that name.
sleep 3

# Apply Finder layout via AppleScript — icons positioned, toolbar hidden, background set
osascript <<APPLESCRIPT
delay 1
tell application "Finder"
    tell disk "Geniuz"
        open
        delay 1
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {200, 200, 840, 600}
        set theViewOptions to the icon view options of container window
        set arrangement of theViewOptions to not arranged
        set icon size of theViewOptions to 128
        set background picture of theViewOptions to file ".background:background.png"
        set position of item "Geniuz.app" of container window to {216, 194}
        set position of item "Applications" of container window to {480, 194}
        close
        open
        update without registering applications
        delay 2
        close
    end tell
end tell
APPLESCRIPT

# Let Finder persist the layout to disk before we unmount
sync
sleep 2

hdiutil detach "$MOUNT_POINT"

# Convert read-write DMG to compressed read-only (what ships)
hdiutil convert "$DMG_RW" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$DMG_RW"

echo "==> Sign + notarize DMG"
codesign --force --timestamp --sign "$IDENTITY" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"

echo ""
echo "✅ Ship complete: $DMG"
ls -lh "$DMG"
