#!/bin/sh
# The folders a build for developing (cargo run, dev.sh, cargo test) uses are its own: zoe-dev, where the installed app
# uses zoe. This looks after them.
#
# Usage: scripts/dev-sandbox.sh <status | reset | copy> [-y] [--no-modules]
#   status   where each is, and how big
#   reset    delete the sandbox (the next run starts as a first run, with the sample vault)
#   copy     reset, then fill it with a copy of the installed app's: settings, session, vault and modules (the
#            folder of SWORD modules is copied unless --no-modules), to try a change on real notes
#            without risk. On a Mac the copy shares disk space until something is changed.
#   -y       do not ask before deleting
#
# ZOE_HOME=/some/folder cargo run -p zoe puts the app's folders (config/, data/) wherever you like instead.
set -e
config_base="${XDG_CONFIG_HOME:-$HOME/.config}"
data_base="${XDG_DATA_HOME:-$HOME/.local/share}"
real_config="$config_base/zoe"; real_data="$data_base/zoe"; real_modules="$config_base/sword"
dev_config="$config_base/zoe-dev"; dev_data="$data_base/zoe-dev"
yes=""; modules=1; command=""
for arg in "$@"; do
    case "$arg" in
        -y) yes=1 ;;
        --no-modules) modules="" ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
        status|reset|copy) command="$arg" ;;
        *) echo "Unknown: $arg" >&2; exit 1 ;;
    esac
done
[ -n "$command" ] || { sed -n '2,13p' "$0"; exit 1; }

size() { if [ -e "$1" ]; then du -sh "$1" 2>/dev/null | cut -f1; else echo "-"; fi; }
status() {
    printf '%-9s %-40s %s\n' "" "config" "data"
    printf '%-9s %-40s %s\n' installed "$real_config ($(size "$real_config"))" "$real_data ($(size "$real_data"))"
    printf '%-9s %-40s %s\n' sandbox "$dev_config ($(size "$dev_config"))" "$dev_data ($(size "$dev_data"))"
}
reset() {
    if [ -z "$yes" ] && { [ -e "$dev_config" ] || [ -e "$dev_data" ]; }; then
        printf 'Delete %s and %s? [y/N] ' "$dev_config" "$dev_data"
        read -r answer
        [ "$answer" = y ] || { echo "Left as they were."; exit 1; }
    fi
    rm -rf "$dev_config" "$dev_data"
}
clone() {
    [ -e "$1" ] || return 0
    mkdir -p "$(dirname "$2")"
    if [ "$(uname -s)" = Darwin ]; then cp -cR "$1" "$2" 2>/dev/null || cp -R "$1" "$2"; else cp -R --reflink=auto "$1" "$2" 2>/dev/null || cp -R "$1" "$2"; fi
}
case "$command" in
    status) status ;;
    reset) reset; echo "The sandbox is gone."; status ;;
    copy)
        reset
        clone "$real_config" "$dev_config"
        clone "$real_data" "$dev_data"
        # Not the installed app's record of its version, nor its backups: the sandbox is its own.
        rm -f "$dev_data/version.json"; rm -rf "$dev_data/backups"
        # The vault is the one thing the copy must not share: the settings name the installed app's folder for it, so the
        # sandbox gets a copy of that folder of its own, and the name is dropped from its settings (it looks there first).
        vault=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("vault") or "")' "$real_config/config.json" 2>/dev/null || true)
        [ -n "$vault" ] || vault="$real_data/vault"
        if [ -d "$vault" ] && [ "$vault" != "$real_data/vault" ]; then
            rm -rf "$dev_data/vault"
            clone "$vault" "$dev_data/vault"
        fi
        if [ -f "$dev_config/config.json" ]; then
            python3 - "$dev_config/config.json" <<'PY'
import json, sys
path = sys.argv[1]
config = json.load(open(path))
config.pop("vault", None)
json.dump(config, open(path, "w"), indent=2)
PY
        fi
        # (The search indexes are caches kept by vault: the sandbox's start again.)
        rm -rf "$dev_data/cache"
        if [ -n "$modules" ]; then clone "$real_modules" "$dev_config/sword"; fi
        echo "The sandbox is a copy of the installed app's folders."
        status
        ;;
esac
