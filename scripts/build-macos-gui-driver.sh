#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $(uname -s) != Darwin ]]; then
  echo 'The GUI driver must be built on macOS.' >&2
  exit 1
fi
root=$PWD
app="$root/target/gui-driver/NeomacsGuiDriver.app"
mkdir -p "$app/Contents/MacOS" "$root/tmp/gui-driver/swift-cache"
export TMPDIR="$root/tmp"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.neomacs.GuiDriver</string>
<key>CFBundleExecutable</key><string>NeomacsGuiDriver</string>
<key>CFBundleName</key><string>Neomacs GUI Driver</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>LSUIElement</key><true/>
</dict></plist>
PLIST
swiftc -parse-as-library -swift-version 5 -O \
  -target "$(uname -m)-apple-macosx14.0" \
  -module-cache-path "$root/tmp/gui-driver/swift-cache" \
  crates/neomacs-gui-tests/src/interaction/macos/Driver.swift \
  -o "$app/Contents/MacOS/NeomacsGuiDriver"
codesign --force --sign "${NEOMACS_GUI_DRIVER_SIGN_IDENTITY:--}" --identifier org.neomacs.GuiDriver "$app"
printf 'Built %s\n' "$app"
printf 'Launch in the logged-in desktop: open -n %q --args %q\n' "$app" "$root/tmp/gui-driver.sock"
