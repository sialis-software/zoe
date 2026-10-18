# Changelog

What changed in each version of Zoe, newest first. The part under **Unreleased** is written as changes are made; `scripts/release.sh` dates it and numbers it when a release is made (see `docs/11-releasing.md`). Only what a person using Zoe would notice is listed; the rest is in the git log.

## [Unreleased]

## [0.6.0] - 2026-10-17

- "Add Note" on the toolbar under a selection is now "Add Mark", and it opens a card of its own: the caret is in the tags (Enter adds a tag and stays for the next, Tab goes on to the words), then a color (choosing one makes the mark a highlight in it) and a drop-down for the look, then buttons. The words are not shown until you press "Add note" (or Tab). A mark with tags and no words is no longer thrown away when you stop. Marks made with a pen work as before.
- Pointing at a mark shows its tags in a label over the left end of its first line, each tag in its pen's color where it has one.
- A link can name a heading of a note: `[[Nicodemus#Questions]]` opens the note with that heading scrolled to the top and the caret on it (the link shows as "Nicodemus > Questions"), `[[#Questions]]` goes to a heading of the note you are in, and typing `#` after a note's name offers its headings. Renaming a note keeps the headings in the links to it.
- A reading ticked in a plan is written in the plan log as a link to its passage from the day's note (`- [[@Genesis 1]] from [[library/Plan/Day 001|…]]`), where it was a `#` after the day's name, which now names a heading. Lines written before are still read, and taken back. The plan log is no longer among the notes that link to a chapter.

## [0.5.0] - 2026-10-17

- The passages a Bible prints under a section's heading, `(Colossians 1:1-2; Philemon 1:1-3)`, are a small quiet line under the heading, not a second bold heading, and each passage in it is a link.
- A font setting of "System Font" (macOS's name for its interface font, which has no bold face of its own) now means the default font, so bold headings stay in the same face as the text.

## [0.4.0] - 2026-10-17

- Nothing a person using Zoe would notice: tests and the release builds now run on GitHub.

## [0.3.0] - 2026-10-16

- The app notes the version that last opened its folders. On an upgrade it copies the settings and session to `backups/` first, runs the migrations for the versions crossed, and says "Updated to X from Y". A newer version's folders opened by an older one are left alone, and the person is told.
- The About page shows the commit the program was built from.
- The card of the days a chapter was read is an overlay card like the reader's others (over the text, closed by a click elsewhere or Escape) and no longer pushes the text down.
- A small grey button beside a chapter's title opens the notes that link to the chapter, in a tab.
- Reader mode keeps the reader's place: it is put back at the same share of the way down when it begins and when it ends.
- Reader mode has small back and forward buttons at its top left, and no chapter button beside the title.
- Only one selection at a time in a dictionary entry, and between the reader's text and its cards.

## [0.2.0] - 2026-10-15

- Reader fullscreen mode, with margins on both sides of the text; the margin settings are in the reader's menu.
- A macOS menu bar with the reader's toggles; shortcuts for tabs and panes; tabs go back and forward across kinds.
- Links by kind: `[[@John 3:16]]` for passages and `[[%G25]]` for Strong's entries, completed as they are typed; dictionary embeds; vaults made before are rewritten once, with the old notes kept.
- Selectable, copyable reference cards and dictionary entries; Strong's numbers in entries are links.
- The reading log: days link to their daily notes, a last chapter taken back takes the day with it, and autofill.
- Layouts keep what the readers show, not what they read; a default layout taken from the saved session.
- Pen colors have a column of greys; footnote markers stay with the word they come before.

## [0.1.0]

- The first version: a SWORD reader with marks and notes, a vault of Markdown files, tabs and panes, reading plans.
