# Releasing, installing, and upgrading

How the app is built for use, kept apart from the one being worked on, and moved from one version to the next.

## 1. Two apps on one machine

There are two kinds of build, and they never share a folder:

| | The installed app | A build for developing |
|---|---|---|
| Made by | `scripts/release.sh`, installed by `scripts/install-macos.sh` | `cargo run -p zoe`, `./dev.sh`, `cargo test` (any debug build) |
| Settings, layouts | `~/.config/zoe/` | `~/.config/zoe-dev/` |
| Session, indexes, default vault, backups | `~/.local/share/zoe/` | `~/.local/share/zoe-dev/` |
| The app's own SWORD modules | `~/.config/sword/` (shared with other SWORD programs) | `~/.config/zoe-dev/sword/` |
| `--reseed` | refused | works |
| About page | `Version 0.2.0 (394fd7c)` | `Version 0.2.0 · dev build (394fd7c+)` |

(`%APPDATA%` and `%LOCALAPPDATA%` replace the two roots on Windows; `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured.)
The choice is made by the build (`cfg!(debug_assertions)`: `vault::folder_name`, `sword::own_folder`), so it cannot be
forgotten at a prompt. `ZOE_HOME=/some/folder` (or `ZOE_CONFIG_HOME` and `ZOE_DATA_HOME`) overrides both kinds, and puts the
SWORD folder in `config/sword` of it, which makes a throwaway sandbox of any build. The tests use temporary folders of their
own; the few that need real modules read (never write) the shared SWORD folder, or `ZOE_TEST_SWORD_PATH`, and skip
without any.

A new sandbox is a first run, with the sample vault and the module that comes with the app. To work on real notes without
risk, copy the installed app's folders into it:

```
scripts/dev-sandbox.sh status          # where each is, and how big
scripts/dev-sandbox.sh copy            # the sandbox becomes a copy: settings, session, vault, modules
scripts/dev-sandbox.sh reset -y        # and back to a first run
```

`copy` gives the sandbox its own copy of the vault (even one kept elsewhere) and drops the setting that named the
original, so that nothing the sandbox does can reach it.

## 2. What a version is

- The number is `version` in the workspace `Cargo.toml` (semantic: major.minor.patch). `Packager.toml` repeats it for the
  packages; a test fails if they disagree, and `scripts/release.sh` sets both.
- Every build also knows the commit it was made from (`crates/app/build.rs`; a `+` after it means the tree had changes). It
  is on the About page.
- `CHANGELOG.md` says what changed. Changes are written under `## [Unreleased]` as they are made; a release dates them
  and numbers them.
- **What goes in the changelog:** what a person using Zoe would notice, such as new things, changed behavior, and fixes. Docs,
  licenses, tests, scripts, CI, and other internal work stay in the git log. It also becomes the notes of the GitHub release.
- **Which number to raise** (before 1.0): `minor` for a release with something new or visibly changed, `patch` for one of
  small fixes only, `major` is kept for 1.0. Fixes that change what a person can do (a line that becomes a link, a
  setting that now works) count as something new.

## 3. Making a release

```
scripts/release.sh <patch|minor|major|X.Y.Z> [--dry-run] [--skip-tests] [--no-build] [--universal]
```

It stops unless the tree is clean, the new version is after the old, the tag is free, and the changelog has something
under Unreleased. Then it runs the tests, sets the version, dates the changelog, commits "vX.Y.Z release", tags `vX.Y.Z`
(annotated) and, on a Mac, builds `target/Zoe.app` and `target/dist/Zoe-X.Y.Z-macos-<arch>.dmg`. It never pushes: that is
`git push && git push origin vX.Y.Z`. A release that is not pushed is undone with `git tag -d vX.Y.Z && git reset --hard HEAD~1`.

Other platforms: `scripts/package.sh [macos|windows|linux|all]` (cargo-packager; see the script for what each needs).
The macOS image is signed and notarized when the Apple secrets are set (below); an unsigned one is opened on another Mac
with right-click > Open the first time. The Windows and Linux packages are not signed, and there is no check for new
versions yet.

### Release bundles in CI

`.github/workflows/release.yml` makes the bundles for every platform when a version tag is pushed:

```
scripts/release.sh minor        # as above: tests, versions, changelog, commit, tag
git push && git push origin v0.4.0
```

The tag must equal the version in `Cargo.toml`, and `Cargo.toml` and `Packager.toml` must agree, or the run stops at
once. Then three builds run side by side (the tests are not repeated: `scripts/release.sh` ran them, and `ci.yml` runs them on every push): a macOS disk image for Apple silicon and Intel in one
(`UNIVERSAL=1 scripts/bundle-macos.sh`), a Windows installer, and a Linux `.deb` and AppImage (both `cargo packager`).
The last job puts them on a **draft** GitHub release, titled "Zoe X.Y.Z", with that version's section of `CHANGELOG.md`
as the notes. Read it, then publish it from the Releases page. Mark it a pre-release when publishing an alpha
(`13-alpha-testing.md`).

To try the builds without a tag: Actions, Release, Run workflow. The bundles are kept as artifacts of the run and no
release is made. `.github/workflows/ci.yml` runs the tests on Linux, macOS, and Windows for every push to `main` and every
pull request.

### Signing and notarizing the macOS image

Without it, every other Mac refuses the app until the person right-clicks, or clears the quarantine mark by hand.
`scripts/sign-macos.sh` signs the app and the disk image with a Developer ID certificate and has Apple notarize them
(the app first, then the image, each stapled, so that they open offline too). `scripts/bundle-macos.sh release` calls it,
so a release that finds the settings below is signed, and one that does not is made unsigned.

What it needs, once (an Apple Developer Program membership, US$99 a year):

1. **A Developer ID Application certificate.** developer.apple.com, Certificates, add one of the type "Developer ID
   Application" (or in Xcode: Settings, Accounts, Manage Certificates). Find it in Keychain Access, export it with its
   private key as `zoe-signing.p12`, with a password of your own.
2. **An App Store Connect API key.** appstoreconnect.apple.com, Users and Access, Integrations, Team Keys: make one with
   the Developer role. Download `AuthKey_XXXXXXXXXX.p8` (it can be downloaded once). Note the Key ID and the Issuer ID
   at the top of that page.
3. **Put them in GitHub as secrets** (Settings, Secrets and variables, Actions, or `gh secret set` from the repository).
   Feeding each from a file keeps it out of your shell history and out of any conversation:

   ```
   base64 -i zoe-signing.p12 | gh secret set MACOS_CERTIFICATE
   gh secret set MACOS_CERTIFICATE_PASSWORD          # asks for it
   base64 -i AuthKey_XXXXXXXXXX.p8 | gh secret set APPLE_API_KEY
   gh secret set APPLE_API_KEY_ID                    # asks for it
   gh secret set APPLE_API_ISSUER                    # asks for it
   ```

   Then delete the `.p12` and `.p8` from the Downloads folder (keep them in a password manager).

Without the secrets the image is made unsigned and the run shows a warning (so that releases can go out before the Apple
account is ready).

To sign by hand on this Mac (for a build to give someone directly):

```
xcrun notarytool store-credentials zoe-notary --key AuthKey_XXXXXXXXXX.p8 --key-id <KEY ID> --issuer <ISSUER ID>
ZOE_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" ZOE_NOTARY_PROFILE=zoe-notary scripts/bundle-macos.sh release
```

`ZOE_SIGN_IDENTITY=-` signs ad hoc (no certificate, no notarizing), which is enough to see that the script and the
hardened runtime work. To check a result: `codesign --verify --strict -vv target/Zoe.app`,
`spctl --assess --type execute -vv target/Zoe.app` ("accepted, source=Notarized Developer ID"), and
`xcrun stapler validate target/Zoe.app`.

Windows and Linux packages are not signed. (Windows shows a SmartScreen warning for an unsigned installer; signing it
needs a code-signing certificate, which is a separate purchase.)

## 4. Installing

```
scripts/install-macos.sh [target/Zoe.app | path/to/Zoe-X.Y.Z-macos-arm64.dmg]
```

Asks a running Zoe to quit, replaces `/Applications/Zoe.app`, clears the quarantine mark, and says which version replaced
which. A build for developing (the bundle says `ZoeChannel: dev`) is refused without `--force`. Data is never touched by
installing.

## 5. What the app does when its version changes

The data folder keeps `version.json`, the version that last opened it (`crates/app/src/upgrade.rs`). At start:

- **First run** (nothing there): the version is noted.
- **Same version:** nothing.
- **Upgrade** (older noted, or none noted but files of an earlier version there): the settings, layouts, and session are
  copied to `backups/<from>-to-<to>/` (the last ten are kept), then each migration for a version after the old one and up
  to the new one runs, oldest first. "Updated to X from Y" shows. If a migration fails, the problem shows in the bar and
  the new version is **not** noted, so the next start tries again.
- **Downgrade** (a newer version noted): nothing is changed, the noted version stays, and a warning shows: what the newer
  version wrote may not be read as written. Restore from `backups/` if it matters.

A migration of the vault's notes is a different thing: the vault is the person's, so it asks nothing of the data folder
and keeps what it changes (links with markers: `live::mark_links_once`, the old notes are in `<index>.links-before/`, and
a vault made by another program is left alone).

To add a migration, put an entry in `upgrade::MIGRATIONS` (`version`: the release that needs it; `run`: a function of the
two folders) and a test next to the existing ones; say what it does in the changelog.

## 6. Trying an upgrade end to end

1. Check out the old release (`git checkout v0.2.0`), build and install it: `scripts/bundle-macos.sh && scripts/install-macos.sh`.
   Run it, make some marks and notes, change a setting. (0.2.0 is from before versions were noted, so it leaves no
   `version.json`; from the next release on, the file is there.)
2. Back on the branch with the new work, add the changelog lines and `scripts/release.sh minor`, then
   `scripts/install-macos.sh`.
3. Start it: the toast says it updated ("Updated to 0.3.0": the old version was never noted, so there is no "from");
   `~/.local/share/zoe/backups/earlier-to-0.3.0/` holds the old settings and session, and `version.json` says 0.3.0.
   From then on a release says "Updated to X from Y" and the backup is `Y-to-X`.
4. To see a downgrade, install the old release again: no warning from 0.2.0 (it knows nothing of versions), but from the
   next release on an older one is warned of what it opens, and `version.json` keeps the newer version.
5. To rehearse an upgrade without touching the real folders: `ZOE_HOME=/tmp/zoe-try` with the old and the new build, in turn.
   The installed app takes `ZOE_HOME` too: `ZOE_HOME=/tmp/zoe-try /Applications/Zoe.app/Contents/MacOS/Zoe`.
