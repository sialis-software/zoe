#!/usr/bin/env python3
"""The licence texts of the libraries the program is built from, from `cargo metadata` on standard input.

A text shared by several libraries is written once, under all their names. Used by scripts/third-party.sh.
"""
import json
import os
import sys

meta = json.load(sys.stdin)
texts = {}
for package in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
    if not (package["source"] or "/vendor/" in package["manifest_path"]):
        continue
    folder = os.path.dirname(package["manifest_path"])
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")) and os.path.isfile(path):
            body = open(path, encoding="utf-8", errors="replace").read().strip()
            texts.setdefault(body, []).append(package["name"] + " " + package["version"])

out = ["The libraries Zoe is built from, and their licence texts.", "Zoe itself: LICENSE-MIT and LICENSE-APACHE.", ""]
for body, who in sorted(texts.items(), key=lambda item: sorted(item[1])[0]):
    out += ["=" * 78, ", ".join(sorted(set(who))), "=" * 78, "", body, ""]
print("\n".join(out))
