#!/bin/sh
# Make target/Zoe.app: the app as a macOS bundle, so the Dock, the menu bar and Cmd-Tab call it Zoe.
# Usage: scripts/bundle-macos.sh [release|debug]   (default release)
set -e
cd "$(dirname "$0")/.."
profile="${1:-release}"
if [ "$profile" = release ]; then
    cargo build --release -p zoe
else
    cargo build -p zoe
fi
app="target/Zoe.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/Zoe" "$app/Contents/MacOS/Zoe"
# The icon: the artwork on a rounded square, at every size.
swift scripts/make-icon.swift crates/app/assets/app/zoe.png target/Zoe.iconset
iconutil -c icns target/Zoe.iconset -o "$app/Contents/Resources/Zoe.icns"
rm -rf target/Zoe.iconset
version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Zoe</string>
    <key>CFBundleDisplayName</key><string>Zoe</string>
    <key>CFBundleExecutable</key><string>Zoe</string>
    <key>CFBundleIconFile</key><string>Zoe</string>
    <key>CFBundleIdentifier</key><string>app.zoe.Zoe</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST
echo "Made $app"
