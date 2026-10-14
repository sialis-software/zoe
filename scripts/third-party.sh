#!/bin/sh
# List the libraries the program is built from, with their versions and licences, into
# crates/app/assets/third-party.txt (shown in the About page). Run after the dependencies change.
set -e
cd "$(dirname "$0")/.."
cargo metadata --format-version 1 --locked | python3 -c '
import json, sys
meta = json.load(sys.stdin)
rows = sorted({(p["name"], p["version"], p["license"] or "unknown") for p in meta["packages"] if p["source"] or "/vendor/" in p["manifest_path"]})
for name, version, license in rows:
    print(f"{name} {version}  {license}")
' > crates/app/assets/third-party.txt
echo "Made crates/app/assets/third-party.txt ($(wc -l < crates/app/assets/third-party.txt | tr -d ' ') libraries)"
