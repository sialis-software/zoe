# Usability suggestions

A running list of ways to make the app easier for people who are new to it: walkthroughs, tooltips, discoverability, and new features. Add to the bottom. Move an item to `10-backlog.md` when it is agreed, and delete it when it ships.

## 1. Markdown formatting buttons in the note editor

**Status: built, not yet released** (the next release; delete this item then). The commands are `md-editor`'s `document/format.rs`, the bar is `NoteTab::format_bar`, and `06-wysiwyg-editor.md` §0 says how it behaves. What was decided on the open questions is under them below.

**Problem.** The editor stores Markdown, and rendering hides the syntax. Someone who has never typed `**bold**` has no way to find out how to make bold text, a heading, a list, or a link. The commands exist (`md-editor`'s `document.rs` toggles bold, italic, and code, toggles tasks, and indents and outdents) but only keyboard shortcuts and typing reach them.

**Suggestion.** A formatting bar above the editor with buttons for:

- Bold, italic, inline code, strikethrough
- Heading level (a drop-down: paragraph, H1 to H3)
- Bulleted list, numbered list, task list
- Quote, divider, code block
- Link, and insert verse or note link (opens the same picker as typing `[[`)

**Behavior.**

- Every button acts on the selection. With no selection, a character-level button goes round the word the caret is in, or inserts its markers with the caret between them when there is no word, and a line-level button changes the current line.
- A button shows as pressed when the caret is inside that format, so the bar also tells the user what they are looking at.
- Each button's tooltip names the action and its keyboard shortcut ("Bold (Cmd+B)"), so the bar teaches the shortcuts.
- Each button is a pure edit `(range, text)` plus a new caret, as in `06-wysiwyg-editor.md` §254, so it joins undo as one step and is unit-testable in `md-editor` without the GUI.
- A setting hides the bar for people who prefer the keyboard.

**Open questions.**

- Is the bar always visible, or does it appear only when the editor is focused? (Leaning: always visible, one row, so it takes no surprise space.) Built as always visible, in every note that can be written in (not in the library, which is read-only).
- Which of the buttons are worth the width in a narrow split pane? Overflow into a "more" menu on the right. Built so far as a bar that scrolls sideways; a "more" menu is not done.
- Which shortcuts are free on each platform? Free and added: Cmd+K (Ctrl+K) for a link and Cmd+Shift+X for strikethrough. Headings, lists, quote and the rest have none: the number keys that other editors use change character under Shift or Option by layout, and the app reads keys by character. A shortcut for them wants the keyboard's physical key (as the reader's settings do).

---

Items 2 onward come from a read through the app crate (`crates/app/src`). They are suggestions from reading code, not from watching anyone use the app, so each is worth a short check in the running app before building.

## 2. A first-run welcome and short tour

**Status: built, not yet released.** `tour.rs`. Open: the welcome note is not opened automatically; the "open a passage" step ticks on any chapter change; it points at controls by name in its hints, not with highlights on them.

**Finding.** `bundled.rs` installs a Bible and the Strong's dictionaries on first run so there is something to read, but nothing then tells the user what the app is for or where to start. The ideas that matter (panes and tabs, margins, marks and tags, backlinks, reading plans, the vault of notes) are not visible from a blank window.

**Suggestion.** After the first run, one welcome card with three or four steps: open a passage, click a Strong's word, add a mark, make a note. Each step points at the real control and is checked off when done. The card can be skipped, is never shown again by itself, and is reopened from Help ("Take the tour again"). A sample note in a new vault that uses wikilinks, a verse embed, and a task list teaches the editor by example.

## 3. Tooltips on every icon-only button

**Status: built, not yet released.** `icons::with_tip` and `Icon::square_button`/`arrow_button`. Open: the plan tab's tick glyph buttons, and `plans_window`, `search_tab`, `vault_setup`, were not audited in depth; a bare `button(...)` still compiles without a tip.

**Finding.** `tooltip` is used in about a dozen places (`main.rs`, `bible_tab.rs`, `note_tab.rs`), while the app has well over a hundred `button(` calls, and many are icons (`icons.rs`: chevrons, menu, arrows). An icon with no label cannot be learned except by pressing it.

**Suggestion.** Go through the icon buttons and give each a tooltip that names the action and, where one exists, its shortcut (`Shortcut::display` already formats it per platform). One helper (an `icon_button` that takes the tip as a required argument) makes a missing tooltip a compile error rather than a review comment.

## 4. A shortcut list and a command palette

**Finding.** The commands in `commands.rs` appear in the menu bar on macOS. On Windows and Linux there is no menu, so the shortcuts (`Ctrl+T`, `Ctrl+Shift+[`, and so on, handled in `App::subscription`) can only be found by reading the docs.

**Suggestion.**
- A "Keyboard shortcuts" sheet, opened from a `?` button and from a shortcut, generated from `Command::ALL` and the editor's shortcuts so that it cannot drift from the real ones.
- A command palette (Ctrl/Cmd+K): type a few letters, get the matching command with its shortcut shown. It doubles as the way to find a command without a menu, and it can offer "Go to passage", "Open note", and "Search" in the same box.

## 5. Empty states that say what to do

**Finding.** Several empty states describe the problem and stop, or send the user to a file in the repo:
- Plans list: "Import one … See docs/09-reading-plans.md." A user of an installed app has no such file.
- "No notes yet" has a Create button, which is the right shape; "No modules found yet" and "No modules found in the folders searched" have no action beside them.
- Search with no results gives a bare "No note matches."

**Suggestion.** Every empty state gets the one next action as a button (Import a plan, Open the module manager, Clear the filter), plus a line on what the thing is for. Replace links to repo docs with an in-app help page or a short example.

## 6. Make errors and status messages actionable

**Finding.** Messages go to a single status line (`sh.status`), for example "Could not save the session: {e}" and "No note named “X” yet". They vanish or are overwritten, and the second one does not offer to create the note.

**Suggestion.** Messages that have an obvious fix carry a button ("Create note “X”", "Retry", "Show in folder"). Errors that lose work (a failed save) stay until dismissed rather than being replaced by the next status.

## 7. An in-app help page

**Finding.** The documentation lives in `docs/` and the repo `README.md`. The app has an About window (`about.rs`) but no Help.

**Suggestion.** A Help menu item opening a searchable page of short how-tos written for users, not developers: marking text, tags and pens, linking notes, reading plans, modules, Vim mode, sync. The tour (item 2) and the shortcut sheet (item 4) live under it.

## Ideas not yet looked into

- Undo for destructive actions that have none today (deleting a mark, closing a note tab, removing a plan).
- Discoverability of right-click and hover features (pointing at a mark shows its tags; a Strong's word opens a card). A one-time hint on first hover.
- A settings search box, and plain-language descriptions under each setting.
- Accessibility pass: font scale, contrast of the `Palette` colors, keyboard-only navigation of every pane.
