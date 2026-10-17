#!/bin/sh
# Sign (and notarize) a macOS app bundle or disk image so that it opens on another Mac without a warning.
# Usage: scripts/sign-macos.sh <path to Zoe.app or a .dmg>
#
# Configured by environment variables, and a no-op if the first is not set (so a build on a machine with no certificate
# still works, and is simply unsigned):
#   ZOE_SIGN_IDENTITY   the certificate to sign with, as `security find-identity -v -p codesigning` names it
#                       ("Developer ID Application: Name (TEAMID)"), or "-" to sign ad hoc (no certificate, no notarizing:
#                       for trying this script)
# Notarizing (Apple checks the program and gives a ticket that is stapled on), if the identity is a real one and either:
#   ZOE_NOTARY_PROFILE  a profile kept by `xcrun notarytool store-credentials` (for a Mac that releases by hand), or
#   ZOE_NOTARY_KEY, ZOE_NOTARY_KEY_ID, ZOE_NOTARY_ISSUER   an App Store Connect API key file (.p8) and its ids (for CI)
# See docs/11-releasing.md for how to get these.
set -e
target="${1:?Usage: $0 <Zoe.app | file.dmg>}"
identity="${ZOE_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
    echo "Not signing $target (ZOE_SIGN_IDENTITY is not set)"
    exit 0
fi

if [ "$identity" = "-" ]; then
    # Ad hoc: no certificate to vouch for it, so no secure timestamp and nothing to notarize.
    codesign --force --options runtime --timestamp=none --sign - "$target"
else
    codesign --force --options runtime --timestamp --sign "$identity" "$target"
fi
codesign --verify --strict --verbose=2 "$target"
echo "Signed $target"
[ "$identity" = "-" ] && exit 0

# Notarizing: which credentials, if any.
if [ -n "${ZOE_NOTARY_PROFILE:-}" ]; then
    auth="--keychain-profile $ZOE_NOTARY_PROFILE"
elif [ -n "${ZOE_NOTARY_KEY:-}" ] && [ -n "${ZOE_NOTARY_KEY_ID:-}" ] && [ -n "${ZOE_NOTARY_ISSUER:-}" ]; then
    auth="--key $ZOE_NOTARY_KEY --key-id $ZOE_NOTARY_KEY_ID --issuer $ZOE_NOTARY_ISSUER"
else
    echo "Not notarizing $target (no ZOE_NOTARY_PROFILE, or ZOE_NOTARY_KEY with ZOE_NOTARY_KEY_ID and ZOE_NOTARY_ISSUER)"
    exit 0
fi

upload="$target"
case "$target" in
    *.app)
        # An app is sent zipped (the way Finder does it).
        upload="$(mktemp -d)/Zoe.zip"
        ditto -c -k --keepParent "$target" "$upload"
        ;;
esac
# shellcheck disable=SC2086
if ! out=$(xcrun notarytool submit "$upload" $auth --wait 2>&1); then
    echo "$out" >&2
    exit 1
fi
echo "$out"
case "$out" in
    *"status: Accepted"*) ;;
    *)
        id=$(echo "$out" | awk '/^ *id:/ {print $2; exit}')
        echo "Apple did not accept $target; its log:" >&2
        # shellcheck disable=SC2086
        [ -n "$id" ] && xcrun notarytool log "$id" $auth >&2
        exit 1
        ;;
esac
[ "$upload" != "$target" ] && rm -rf "$(dirname "$upload")"
xcrun stapler staple "$target"
xcrun stapler validate "$target"
case "$target" in
    *.app) spctl --assess --type execute --verbose=2 "$target" ;;
    *) spctl --assess --type open --context context:primary-signature --verbose=2 "$target" ;;
esac
echo "Notarized $target"
