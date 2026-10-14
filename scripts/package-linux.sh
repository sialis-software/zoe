#!/bin/sh
# Make the Linux packages in target/dist, from a release build:
#   Zoe-<version>-linux-<arch>.tar.gz   always: the program, its icon and desktop file, and install.sh
#   zoe_<version>_<deb arch>.deb        when dpkg-deb is there
#   Zoe-<version>-<arch>.AppImage       when appimagetool is on the PATH (https://appimage.org)
# Usage: scripts/package-linux.sh   (run on Linux; unsigned)
set -e
cd "$(dirname "$0")/.."
cargo build --release -p zoe
version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
arch=$(uname -m)
dist=target/dist
stage=target/package-linux
rm -rf "$stage"
mkdir -p "$dist" "$stage"

# One tree, laid out as it is installed under a prefix (usr).
root="$stage/root"
mkdir -p "$root/usr/bin" "$root/usr/share/applications"
install -m 755 target/release/Zoe "$root/usr/bin/Zoe"
install -m 644 packaging/linux/zoe.desktop "$root/usr/share/applications/zoe.desktop"
for size in 16 32 48 64 128 256 512; do
    dir="$root/usr/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    install -m 644 "packaging/icons/zoe-$size.png" "$dir/zoe.png"
done

# Tarball, with a script that copies the tree into a prefix (default ~/.local).
tar_dir="$stage/Zoe-$version"
mkdir -p "$tar_dir"
cp -R "$root/usr" "$tar_dir/usr"
cat > "$tar_dir/install.sh" <<'INSTALL'
#!/bin/sh
# Install Zoe under a prefix: ./install.sh [prefix]   (default ~/.local)
set -e
prefix="${1:-$HOME/.local}"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$prefix"
cp -R "$here/usr/." "$prefix/"
echo "Installed Zoe in $prefix (the program is $prefix/bin/Zoe)"
INSTALL
chmod +x "$tar_dir/install.sh"
tar -C "$stage" -czf "$dist/Zoe-$version-linux-$arch.tar.gz" "Zoe-$version"
echo "Made $dist/Zoe-$version-linux-$arch.tar.gz"

# .deb
if command -v dpkg-deb >/dev/null 2>&1; then
    deb_arch=$(dpkg --print-architecture)
    deb="$stage/deb"
    cp -R "$root" "$deb"
    mkdir -p "$deb/DEBIAN"
    size=$(du -sk "$root/usr" | cut -f1)
    cat > "$deb/DEBIAN/control" <<CONTROL
Package: zoe
Version: $version
Section: education
Priority: optional
Architecture: $deb_arch
Installed-Size: $size
Depends: libc6, libxkbcommon0, libvulkan1 | libgl1
Maintainer: Zoe
Description: Bible study with SWORD modules and Markdown notes
 A study workspace of panes and tabs: Bible texts, commentaries and
 dictionaries from SWORD modules, and your own notes in a vault of
 Markdown files.
CONTROL
    dpkg-deb --build --root-owner-group "$deb" "$dist/zoe_${version}_$deb_arch.deb" >/dev/null
    echo "Made $dist/zoe_${version}_$deb_arch.deb"
else
    echo "Skipped the .deb: dpkg-deb is not here"
fi

# AppImage
if command -v appimagetool >/dev/null 2>&1; then
    appdir="$stage/Zoe.AppDir"
    cp -R "$root" "$appdir"
    cp packaging/linux/zoe.desktop "$appdir/zoe.desktop"
    cp packaging/icons/zoe-256.png "$appdir/zoe.png"
    printf '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/usr/bin/Zoe" "$@"\n' > "$appdir/AppRun"
    chmod +x "$appdir/AppRun"
    ARCH="$arch" appimagetool "$appdir" "$dist/Zoe-$version-$arch.AppImage" >/dev/null
    echo "Made $dist/Zoe-$version-$arch.AppImage"
else
    echo "Skipped the AppImage: appimagetool is not on the PATH"
fi
