# Giving an alpha to testers

For the person releasing: make a release as usual (`scripts/release.sh minor`, or `patch` for a follow-up, then push the
commit and the tag). The Release workflow drafts a GitHub release with the macOS, Windows, and Linux downloads. Tick
**Set as a pre-release** when publishing the draft, and send the testers the link (the repository must be public or the
testers invited to it; see `11-releasing.md`). Until the packages are signed, send the notes below with it.

Versions stay plain numbers (0.4.0, 0.4.1): a suffix such as `-alpha.1` is not handled by `release.sh`'s ordering and is
not a valid macOS bundle version.

## Notes for testers

Zoe is an alpha: it can have bugs, and its files may change between builds. Your notes are plain markdown files in the
vault folder (Settings, Vault), so they are yours whatever happens to the program. Keep a copy of that folder if the notes
matter. Each upgrade also backs up the settings before it changes anything.

Installing, by system. The packages are not signed yet, so each system asks you to confirm that you trust it once.

- **Windows.** Run `Zoe-X.Y.Z-setup.exe`. Windows SmartScreen says "Windows protected your PC": choose **More info**, then
  **Run anyway**.
- **macOS.** Open the `.dmg` and drag Zoe to Applications. If it will not open (or says it is damaged), open
  **System Settings**, then **Privacy & Security**, scroll to **Security**, and choose **Open Anyway** next to the
  message about Zoe (try opening Zoe first, so the message appears). If that does not work, run
  `xattr -dr com.apple.quarantine /Applications/Zoe.app` in Terminal. Once the build is signed and notarized this is not
  needed.
- **Linux.** The `.deb` installs with `sudo apt install ./Zoe_X.Y.Z_amd64.deb`. The AppImage runs after `chmod +x` on it.

Update by installing the newer build over the old one. Your notes and settings stay.

## Sending feedback

Say what you did, what you expected, and what happened; include the version from **Settings, About** and your system. A
screenshot helps. If the program will not start, say so and what it printed, if anything.
