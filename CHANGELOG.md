# Changelog

What changed in each version of Zoe, newest first. The part under **Unreleased** is written as changes are made;
`scripts/release.sh` dates it and numbers it when a release is made (see `docs/11-releasing.md`).

## [Unreleased]

- A build for developing (`cargo run`, `dev.sh`, the tests) keeps its settings, session, vault and modules in `zoe-dev`
  folders of its own, so it can no longer touch the installed app's. `scripts/dev-sandbox.sh` looks at them, clears
  them, or fills them with a copy of the installed app's. `--reseed` works only there.
- The app notes the version that last opened its folders. On an upgrade it copies the settings and session to
  `backups/` first, runs the migrations for the versions crossed, and says "Updated to X from Y". A newer version's
  folders opened by an older one are left alone, and the person is told.
- The About page shows the commit the program was built from, and says so for a build for developing.
- Releases are made by `scripts/release.sh`, and put on a Mac by `scripts/install-macos.sh`.
- The card of the days a chapter was read is an overlay card like the reader's others (over the text, closed by a click
  elsewhere or Escape) and no longer pushes the text down.
- A small grey button beside a chapter's title opens the notes that link to the chapter, in a tab.
- Reader mode keeps the reader's place: it is put back at the same share of the way down when it begins and when it ends.
- Reader mode has small back and forward buttons at its top left, and no chapter button beside the title.
- Only one selection at a time in a dictionary entry, and between the reader's text and its cards.

## [0.2.0] - 2026-10-15

- Reader fullscreen mode, with margins on both sides of the text; the margin settings are in the reader's menu.
- A macOS menu bar with the reader's toggles; shortcuts for tabs and panes; tabs go back and forward across kinds.
- Links by kind: `[[@John 3:16]]` for passages and `[[%G25]]` for Strong's entries, completed as they are typed;
  dictionary embeds; vaults made before are rewritten once, with the old notes kept.
- Selectable, copyable reference cards and dictionary entries; Strong's numbers in entries are links.
- The reading log: days link to their daily notes, a last chapter taken back takes the day with it, and autofill.
- Layouts keep what the readers show, not what they read; a default layout taken from the saved session.
- Pen colors have a column of greys; footnote markers stay with the word they come before.

## [0.1.0]

- The first version: a SWORD reader with marks and notes, a vault of Markdown files, tabs and panes, reading plans.
