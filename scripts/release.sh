#!/bin/sh
# Make a release: check, number, record, tag and build it. Nothing is pushed.
#
# Usage: scripts/release.sh <patch|minor|major|X.Y.Z> [--dry-run] [--skip-tests] [--no-build] [--universal]
#
#   1. The tree must be clean, and CHANGELOG.md must say what is new (under "## [Unreleased]").
#   2. The tests run (--skip-tests to not).
#   3. The version is set in Cargo.toml and Packager.toml (and Cargo.lock follows), the changelog's new part is
#      dated and numbered, and all of it is committed as "vX.Y.Z release" and tagged vX.Y.Z.
#   4. On a Mac the app is built: target/Zoe.app and target/dist/Zoe-X.Y.Z-macos-<arch>.dmg (--universal: Apple
#      silicon and Intel in one; --no-build to leave that for scripts/bundle-macos.sh or scripts/package.sh).
#
# --dry-run says what would be done and changes nothing. To put the result on this Mac: scripts/install-macos.sh.
# To undo a release that is not pushed: git tag -d vX.Y.Z && git reset --hard HEAD~1.
set -e
cd "$(dirname "$0")/.."

bump=""
dry=""
tests=1
build=1
universal=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry=1 ;;
        --skip-tests) tests="" ;;
        --no-build) build="" ;;
        --universal) universal=1 ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        -*) echo "Unknown option $arg" >&2; exit 1 ;;
        *) bump="$arg" ;;
    esac
done
[ -n "$bump" ] || { echo "Usage: $0 <patch|minor|major|X.Y.Z> [--dry-run] [--skip-tests] [--no-build] [--universal]" >&2; exit 1; }

current=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
packager=$(grep -m1 '^version' Packager.toml | sed 's/.*"\(.*\)".*/\1/')
if [ "$current" != "$packager" ]; then
    echo "Cargo.toml says $current and Packager.toml $packager: make them one before a release." >&2
    exit 1
fi

major=${current%%.*}; rest=${current#*.}; minor=${rest%%.*}; patch=${rest#*.}; patch=${patch%%[!0-9]*}
case "$bump" in
    patch) new="$major.$minor.$((patch + 1))" ;;
    minor) new="$major.$((minor + 1)).0" ;;
    major) new="$((major + 1)).0.0" ;;
    [0-9]*.[0-9]*.[0-9]*) new="$bump" ;;
    *) echo "Not a version, or patch, minor or major: $bump" >&2; exit 1 ;;
esac
tag="v$new"

# The new version has to be after the old one.
newer=$(printf '%s\n%s\n' "$current" "$new" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)
if [ "$new" = "$current" ] || [ "$newer" != "$new" ]; then
    echo "$new is not after $current." >&2
    exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "The tag $tag is already there." >&2
    exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "The tree has changes that are not committed: a release is made from a clean one." >&2
    git status --short >&2
    exit 1
fi
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = main ] || echo "Note: this is $branch, not main."

# What is new: the lines under "## [Unreleased]" up to the next heading.
if [ ! -f CHANGELOG.md ] || ! grep -q '^## \[Unreleased\]' CHANGELOG.md; then
    echo "CHANGELOG.md needs a \"## [Unreleased]\" section saying what is new." >&2
    exit 1
fi
news=$(awk '/^## \[Unreleased\]/{on=1; next} /^## /{on=0} on' CHANGELOG.md | sed '/^[[:space:]]*$/d')
if [ -z "$news" ]; then
    echo "Nothing is written under \"## [Unreleased]\" in CHANGELOG.md: say what is new first." >&2
    exit 1
fi

echo "Release $current -> $new ($tag) on $branch, from $(git rev-parse --short HEAD)"
echo "$news" | sed 's/^/    /'
if [ -n "$dry" ]; then
    echo "(a dry run: nothing was changed. It would run the tests, set the version, commit, tag and build.)"
    exit 0
fi

if [ -n "$tests" ]; then
    cargo test --workspace
fi

date=$(date +%Y-%m-%d)
# (awk, not sed: the first line is the one that counts, and the address for that is not the same in every sed.)
for file in Cargo.toml Packager.toml; do
    awk -v old="$current" -v new="$new" '!done && $0 == "version = \"" old "\"" { print "version = \"" new "\""; done = 1; next } { print }' "$file" > "$file.new" && mv "$file.new" "$file"
done
grep -q "^version = \"$new\"" Cargo.toml && grep -q "^version = \"$new\"" Packager.toml || { echo "Could not set the version in Cargo.toml and Packager.toml: nothing is committed." >&2; exit 1; }
awk -v v="$new" -v d="$date" '
    /^## \[Unreleased\]/ { print; print ""; print "## [" v "] - " d; next }
    { print }' CHANGELOG.md > CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md
# The lock file follows the version (the program is rebuilt anyway, so this costs nothing).
cargo update --workspace --quiet
git add Cargo.toml Cargo.lock Packager.toml CHANGELOG.md
git commit -q -m "$tag release"
git tag -a "$tag" -m "Zoe $new"
echo "Committed and tagged $tag. Not pushed."

if [ -n "$build" ] && [ "$(uname -s)" = Darwin ]; then
    UNIVERSAL=$universal scripts/bundle-macos.sh release
    echo
    echo "Built target/Zoe.app and the disk image above. To install it here: scripts/install-macos.sh"
else
    echo "Not built here: scripts/package.sh makes the packages."
fi
echo "To publish: git push && git push origin $tag (nothing here does)."
