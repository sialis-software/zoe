#!/bin/sh
# Make target/Zoe.app: the app as a macOS bundle, so the Dock, the menu bar and Cmd-Tab call it Zoe.
# Usage: scripts/bundle-macos.sh [release|debug]   (default release)
#   UNIVERSAL=1 scripts/bundle-macos.sh   a release for Apple silicon and Intel in one
#   (needs both targets: rustup target add aarch64-apple-darwin x86_64-apple-darwin)
# The bundle is not signed: on another Mac, open it with right-click > Open the first time.
set -e
cd "$(dirname "$0")/.."
profile="${1:-release}"
program="target/$profile/Zoe"
if [ -n "$UNIVERSAL" ]; then
    profile=release
    cargo build --release -p zoe --target aarch64-apple-darwin
    cargo build --release -p zoe --target x86_64-apple-darwin
    mkdir -p target/universal
    lipo -create -output target/universal/Zoe target/aarch64-apple-darwin/release/Zoe target/x86_64-apple-darwin/release/Zoe
    program=target/universal/Zoe
elif [ "$profile" = release ]; then
    cargo build --release -p zoe
else
    cargo build -p zoe
fi
app="target/Zoe.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$program" "$app/Contents/MacOS/Zoe"
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
# A release is also made into a disk image to hand out: the app and a link to Applications.
if [ "$profile" = release ]; then
    dist=target/dist
    if [ -n "$UNIVERSAL" ]; then arch=universal; else arch=$(uname -m); fi
    dmg="$dist/Zoe-$version-macos-$arch.dmg"
    stage=target/dmg
    rm -rf "$stage" "$dmg"
    mkdir -p "$stage" "$dist"
    cp -R "$app" "$stage/Zoe.app"
    ln -s /Applications "$stage/Applications"
    hdiutil create -quiet -volname "Zoe" -srcfolder "$stage" -ov -format UDZO "$dmg"
    rm -rf "$stage"
    echo "Made $dmg"
fi
