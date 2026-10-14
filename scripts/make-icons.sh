#!/bin/sh
# Make the icon files the Linux and Windows packages use from the rounded artwork:
# packaging/icons/zoe-<size>.png and packaging/icons/zoe.ico (PNG-compressed, 16 to 256 px).
# Needs macOS (sips) and python3; the results are kept in git, so this only runs when the artwork changes.
set -e
cd "$(dirname "$0")/.."
src=crates/app/assets/app/zoe-rounded.png
out=packaging/icons
mkdir -p "$out"
for size in 16 32 48 64 128 256 512; do
    sips -z $size $size "$src" --out "$out/zoe-$size.png" >/dev/null
done
python3 - "$out" <<'PY'
import struct, sys
out = sys.argv[1]
sizes = [16, 32, 48, 64, 128, 256]
images = [open(f"{out}/zoe-{s}.png", "rb").read() for s in sizes]
head = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b""
for s, data in zip(sizes, images):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
open(f"{out}/zoe.ico", "wb").write(head + entries + b"".join(images))
PY
echo "Made $out"
