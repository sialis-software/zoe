#!/bin/sh
# Make the packages for one or all platforms with cargo-packager (config: Packager.toml), from a Mac
# or Linux machine. The packages land in target/dist and are not signed.
# Usage: scripts/package.sh [macos|windows|linux|all]   (default: the platform this runs on)
#   macos    .app and .dmg for this Mac's architecture (on a Mac)
#   windows  NSIS installer for x64, cross-built (on a Mac: needs cargo-xwin, brew's llvm@21 and lld@21, makensis)
#   linux    .deb and AppImage, built in Docker for this machine's architecture (LINUX_PLATFORM=linux/amd64 for x86_64)
# Needs: cargo install cargo-packager cargo-xwin --locked
set -e
cd "$(dirname "$0")/.."
what="${1:-}"
if [ -z "$what" ]; then
    case "$(uname -s)" in Darwin) what=macos ;; *) what=linux ;; esac
fi

macos() {
    cargo build --release -p zoe
    cargo packager --release --formats app,dmg -c Packager.toml
}

windows() {
    triple=x86_64-pc-windows-msvc
    rustup target add "$triple"
    PATH="/opt/homebrew/opt/llvm@21/bin:/opt/homebrew/opt/lld@21/bin:$PATH" cargo xwin build --release -p zoe --target "$triple"
    # The packager looks for the program where the config says, and relative paths are the config's.
    config=.packager-windows.toml
    (echo "binaries-dir = \"target/$triple/release\""; cat Packager.toml) > "$config"
    trap 'rm -f "$config"' EXIT
    cargo packager --release --formats nsis --target "$triple" -c "$config"
}

linux() {
    # Built in a Linux container, with its own target folder so that the Mac's is left alone.
    platform="${LINUX_PLATFORM:-}"
    docker run --rm ${platform:+--platform "$platform"} \
        -v "$PWD":/work -w /work \
        -v zoe-cargo:/usr/local/cargo/registry \
        -e CARGO_TARGET_DIR=/work/target/linux -e APPIMAGE_EXTRACT_AND_RUN=1 \
        rust:1-bookworm sh -c '
            set -e
            apt-get update -qq
            apt-get install -y -qq pkg-config libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev \
                libxrandr-dev libvulkan-dev libgtk-3-dev libssl-dev file wget squashfs-tools
            command -v cargo-packager >/dev/null || cargo install cargo-packager --locked
            cargo build --release -p zoe
            (echo "binaries-dir = \"target/linux/release\""; cat Packager.toml) > .packager-linux.toml
            cargo packager --release --formats deb,appimage -c .packager-linux.toml
            rm -f .packager-linux.toml
        '
}

case "$what" in
    macos) macos ;;
    windows) windows ;;
    linux) linux ;;
    all) macos; windows; linux ;;
    *) echo "Usage: $0 [macos|windows|linux|all]" >&2; exit 1 ;;
esac
