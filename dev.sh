#!/bin/sh
# Rebuild and restart the app whenever source or manifests change. A build for developing keeps its own folders
# (zoe-dev: settings, session, vault, modules), apart from the installed app's: see scripts/dev-sandbox.sh.
# Needs cargo-watch (cargo install cargo-watch).
cd "$(dirname "$0")" || exit 1
exec cargo watch --watch crates --watch Cargo.toml --clear --exec 'run -p zoe'
