#!/bin/sh
# Rebuild and restart the app whenever source or manifests change.
# Needs cargo-watch (cargo install cargo-watch).
cd "$(dirname "$0")" || exit 1
exec cargo watch --watch crates --watch Cargo.toml --clear --exec 'run -p bible-study'
