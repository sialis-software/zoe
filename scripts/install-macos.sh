#!/bin/sh
# Put a built Zoe in /Applications, over the one there (an upgrade, as far as the app can tell: its data is not touched).
#
# Usage: scripts/install-macos.sh [path/to/Zoe.app | path/to/Zoe-X.Y.Z-macos-arch.dmg]   (default target/Zoe.app)
#   --force   install a build for developing too (it is not meant to be: it is slow and keeps its own folders)
#
# Quit Zoe first (this asks it to). The app is not signed: the first time, open it with right-click > Open.
# The version installed is printed, and what the app does about an older version's folders is said at its first start.
set -e
cd "$(dirname "$0")/.."
force=""
source=""
for arg in "$@"; do
    case "$arg" in
        --force) force=1 ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *) source="$arg" ;;
    esac
done
[ "$(uname -s)" = Darwin ] || { echo "This is for a Mac." >&2; exit 1; }
source="${source:-target/Zoe.app}"
mounted=""
case "$source" in
    *.dmg)
        mounted=$(mktemp -d)
        hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mounted" "$source"
        trap 'hdiutil detach -quiet "$mounted" 2>/dev/null; rmdir "$mounted" 2>/dev/null' EXIT
        source="$mounted/Zoe.app"
        ;;
esac
[ -d "$source" ] || { echo "No app at $source: build one with scripts/release.sh or scripts/bundle-macos.sh." >&2; exit 1; }
plist="$source/Contents/Info.plist"
read_key() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || true; }
version=$(read_key CFBundleShortVersionString)
channel=$(read_key ZoeChannel)
if [ "$channel" = dev ] && [ -z "$force" ]; then
    echo "$source is a build for developing, not a release (use --force to install it anyway)." >&2
    exit 1
fi

target=/Applications/Zoe.app
before=""
[ -d "$target" ] && before=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$target/Contents/Info.plist" 2>/dev/null || true)

if pgrep -x Zoe >/dev/null; then
    echo "Zoe is running: asking it to quit."
    osascript -e 'tell application "Zoe" to quit' >/dev/null 2>&1 || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do pgrep -x Zoe >/dev/null || break; sleep 1; done
    if pgrep -x Zoe >/dev/null; then
        echo "It is still running: quit it, then run this again." >&2
        exit 1
    fi
fi

rm -rf "$target"
cp -R "$source" "$target"
# (Copied from a disk image or built here: not downloaded, but the system may mark it as if it were.)
xattr -dr com.apple.quarantine "$target" 2>/dev/null || true
if [ -n "$before" ]; then
    echo "Installed Zoe $version over $before."
else
    echo "Installed Zoe $version."
fi
echo "Its folders: config ~/.config/zoe, data ~/.local/share/zoe (a build for developing uses zoe-dev beside them)."
