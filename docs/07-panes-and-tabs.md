# Panes and tabs (the meta-layout)

The window is a workspace: **panes** side by side, each holding **tabs**, one showing. A tab is one of a small set of self-contained *types*, each in its own module with its own state, messages, `update`, and `view`:

| Tab type | Module | Holds | Per-tab state |
|---|---|---|---|
| Bible | `bible_tab.rs` | one reader: module, book, chapter | selection, margin cards, footnote/dictionary cards, popups, scroll position, Strong's setting, the navigation toolbar |
| Dictionary | `dict_tab.rs` | one entry of a dictionary or lexicon | the dictionary, the key being looked up, the entry shown |
| Note | `note_tab.rs` | one markdown document (a note or a margin file) | the document, the selector panel; a header bar like the Bible tab's |

Everything a tab type needs to read or change that is not its own (the backend, the notes, the marks, the vault, theme, and status) is in `Shared` (`shared.rs`); tabs get `&Shared` to draw and `&mut Shared` to update. Nothing in a tab type knows about panes.

## The workspace (`main.rs`)

`App` owns `Shared`, the tabs (by `TabId`), and the panes (ordered tab ids plus the active one). It routes `Message::Bible(id, …)` and `Message::Note(id, …)` to the tab, and carries out what one tab asks of another, so tab types never reach into each other:

- A tab returns a **request** when it cannot do something alone (`bible_tab::Request`, `note_tab::Request`): open this note, show this verse, look up this word.
- A change that other tabs must follow is recorded in `Shared` as an **event** (a mark was removed, a file was rewritten, renamed, or deleted); `App::settle` passes events to the tabs showing them after every update.
- **Which Bible tab?** Notes work with the *reference* Bible tab: the one last used if it is showing, else any showing one. Its selection feeds the strip of notes on the selected verses and its module serves embeds and previews. A verse link in a note shows in it, opening a Bible tab if there is none.
- **Which note tab?** Opening a note (a `[[link]]` in a card, "Open" on a margin card) shows it in the tab already holding it, else navigates the note tab last used, else opens a new one on the right. Two tabs on the same note follow each other's typing.

## Taking verses to a note: the clipboard

Notes are plain text, so verses travel by the clipboard. The toolbar under a selection in a Bible tab has **Copy reference** (`[[@John 3:3-5]]`) and **Copy quote** (`![[@John 3:3-5@LSB]]`, pinned to the module it was taken from); paste them into any note. Copying puts the selection and its toolbar away and shows a toast (`Shared::toast`, a line over the bottom of the window for about two seconds; a clock subscription runs only while one is showing). A selector panel or other pop-up closes when anything outside it is clicked, in any pane.

A margin note opened in a note tab shows its mark's details in place of the properties strip: the verse it is on (a link: it shows that verse in the reader, as a `[[@John 3:16]]` does: right click for where to open it, pull it away to carry it to a tab or pane), the style, module, created time, and tags.

A new tab asks only what *kind* of tab it is to be (Bible, Dictionary, or Notes); which module, dictionary, or note comes after, from the tab's own pickers. It opens on the module the other Bible tab is reading, the first dictionary, and the first note.

## Backlinks

A verse number is green when some note links to it (`[[@John 3:16]]`; not the templates). Selecting such verses adds a **backlinks** button to the selection toolbar: it opens a tab (in the other pane, one at a time) listing the notes that link to any of the selected verses, each with the lines that do; a click shows the note. The tab follows the notes as they are written, and comes back with the workspace.

The list is in **sections**, in this order: the **margin notes** on the passage (those with something written in them, with the words they are on and the start of what is written: they are on it by where they are, not by a link in their words), the **notes** with no type, then **each type of note** (prayer notes, sermon notes, daily notes... by `type:`, by name), and last the **plans** that read it, each plan once with a button for each of its days (named as a person says them: `Day 87`, `Jan 5`, not by their files). A section's header is a bar of its own, with a chevron and how many notes it holds, and a click on it **folds** the section to that bar or opens it; the plans are folded to begin with. The notes under a header are indented, with a line down their side, and are links, each with its tags beside its name. The heading says how many notes there are in all. The reading log is not among them.

A small grey button beside a chapter's title opens the same tab for the whole chapter: the notes that link to any verse of it. (The reading log and the plan log are left out, since they link to every chapter read.) It is not shown in reader mode.

The note tab's hamburger menu has **Backlinks** too: the same tab, for the notes that link to the note by name (`[[Nicodemus]]`, whatever the link shows), not the note itself or the templates.

## Tags (not folders)

People find notes by tag; folders are the app's business. Every new note is filed in `notes/` (the templates say what they are with a starting tag: `prayer`, `topic`, `daily`) and the selector lists notes flat, with chips for the tags in use (with counts, colored if a pen has the tag). Click chips to narrow (a note must have them all; `topic` includes `topic/grace`); the search box matches names, text, and tags, and `#words` match tags only. Tags are kept lower case, `-` for spaces, `/` to nest. A tag box offers the tags in use as you type, in the note's property strip, in the card of a mark being written in (so a mark's note can be tagged from the text), and when naming a pen's tag.

**Pens** (Settings) are tags with a color: each is a toolbar button that marks the selected text with that style (highlight or underline) in that color and opens a note on it tagged with the tag. Clicking a pen's swatch cycles its color; add one by typing a tag.

## New notes and templates (`core/src/templates.rs`)

The note tab's menu has **New note…**, which asks what kind (and then for a title, unless the kind says it needs none). The kinds are the **templates**: ordinary markdown notes in the vault's `templates/` folder (a new vault is given Prayer list, Topic, Daily, and Blank as files, once), so they sync and back up with the rest of it. They are not found by the note selector or its search (changing what a kind starts with is for those who go looking): the pencil beside a kind in the New note dialog opens its template to edit. Adding a file adds a kind; deleting one removes it.

A template is written as in Obsidian: its text after the frontmatter is the new note's text, with `{{title}}`, `{{date}}` / `{{date:dddd, MMMM D, YYYY}}` and `{{time}}` / `{{time:HH:mm}}` filled in (moment's letters). Whatever the app does differently for a kind is in the template's frontmatter, as properties starting with `template_` (not copied to the notes):

| property | meaning |
|---|---|
| `template_description` | the line under the kind's name in the dialog |
| `template_title` | the title when none is typed or asked (`{{date:YYYY-MM-DD}}` for a daily note) |
| `template_ask_title` | `false`: make the note at once, without a title box |
| `template_unique` | `true`: one note under that title; asking again opens it |

Every other property (`tags`, `type`, your own) is what the new note starts with, placeholders filled. The note's title is the `# heading` it is given (`{{title}}`) and its file name in `notes/`, with a number added if the name is taken; characters a file name cannot have are left out of the name only.

## The note tab's header

Like the Bible tab's toolbar, and the same height: a selector button (styled like the Bible tab's pick lists) naming the open note on the left, and a trash button that asks for confirmation in a dialog over the tab (Cancel, or Delete to remove the note's file; for a margin note, also its highlight). Deleting the last note leaves a fresh empty one. Unlinking a mark from its text is only on the mark's card. The selector opens a panel over the note (a click outside closes it) with a search box and the notes grouped by type, taken from the top folder: Notes first, other types alphabetically (Prayers…), margin notes last, each labelled by its passage ("John 3:16 · note"). The group holding the open note starts open, the others closed; a click on a group's header toggles it. Searching matches names and note text, opens every group that has a match, and lists name matches first.

## Layout

- **Panes** form a tree of splits (`panes.rs`, pure and tested): the two buttons at a pane's bottom right divide it in two, side by side (▥) or top and bottom (▤), the new half starting with a blank tab. The dividers are dragged with the mouse (`split.rs`: a container that owns its divider, so a drag keeps working when the pointer strays over the panes). The `□` beside it fills the window with that pane, and again restores. Closing a pane's last tab closes the pane and its neighbour takes the space; the last pane keeps a blank tab.
- **Tabs** look like a browser's: a chip with the name and a × inside, and a `+` for a new tab right after the last one. A new tab is **blank**, with a selector (every Bible module, every note) that turns it into that type in place. A Bible opens where the Bible last used is reading, so a second translation lines up with the first.
- **Dictionaries never take a margin card.** A Strong's entry is always a pop-over under the click; footnotes are margin cards when the reader has a margin, a pop-over when it does not.
- **Fonts.** The interface names a real family (`fonts.rs`) and the build uses a patched cosmic-text (`vendor/cosmic-text`, one condition in `shape.rs`): upstream 0.15.0 shaped a whole ASCII word with its first character's font, so in `**bold**` the word `**bold` lost its bold while the markers showed. Drop the patch when iced moves to a release with the fix.

Widget ids that must be unique (the reader's scroll area, the card editor, the note editor) are derived from the tab id, so two Bible tabs on screen do not answer for each other.

## Where a link opens

Every link, from any kind of tab, is followed to *a place* (`opening.rs`). A plain click opens it **here**; keys held while clicking, or the right-click menu, send it elsewhere:

| Click | Opens |
|---|---|
| plain | here: in this tab (a note's verse links go to the Bible tab it works with; a dictionary link to the dictionary tab already open, else the other pane) |
| ⌘/Ctrl | in a new tab beside this one |
| Alt | in the other pane (its tab, if it has one of the right kind; else a new tab there; the next pane in order, splitting if there is only one) |
| Shift | in a new pane, split off this one |

A link to a heading of a note (`[[Nicodemus#Questions]]`, or `[[#Questions]]` for the note it is in) follows these keys to the note and then puts the heading's line at the top of its editor, with the caret on it. A heading the note does not have is not an error: the note opens at the top (only `[[#Heading]]`, which would otherwise do nothing, says it is not there). Typing `#` after a note's name offers its headings, and the link is shown as `Nicodemus > Questions`.

Right-clicking a link in the reader, a note, or a card offers the same four, and "Copy link" (the link as it is written in a note). The app follows the keys held through a `ModifiersChanged` subscription, so any link control gets this without handling keys itself. A link in a note's text that is being edited keeps its own rule (Alt-click edits it); the right-click menu is always there.

## Header bars

Every tab's bar is laid out the same way: back and forward arrows, then whatever the tab uses to choose what it shows (the Bible pickers, the note selector, the dictionary, and its lookup box), then a hamburger menu at the right edge. A note's menu holds Rename and Delete.

## History

Each tab keeps its own history, like a browser tab (`history.rs`, up to 100 places). Back and forward arrows sit at the left of the tab's bar, greyed when there is nowhere to go; ⌘[ / ⌘] (Ctrl elsewhere) and the mouse's back and forward buttons act on the focused pane's tab. A Bible tab records module, chapter, selected verses, and how far down it was scrolled (restored on return); changing module counts as a move, scrolling does not. A note tab records the notes it showed (renames are followed, deleted notes dropped); a dictionary tab the entries. Going somewhere new forgets what was ahead. Opening in a new tab or pane starts a fresh history. Right-clicking the Bible tab's arrows lists its places to jump to.

## Dragging

- **A tab** is pressed (which selects it) and moved a few pixels to pick it up. Over the middle of a pane it goes to that pane; over a band along an edge it splits that pane and takes the new half on that side (a pane cannot be split for its own only tab); over another tab it goes in front of it; over the empty tab bar, to the end. A pane left empty goes away.
- **A link** pulled away from where it is (in the reader, a card, or a note) is carried the same way and dropped on a pane: a new tab there, or a new pane on an edge. A plain click still follows it; pulling it no longer starts a text selection.
- While carrying, each pane shows its drop places (the middle, and a band along each edge) with the one under the pointer lit; the name of what is carried follows the pointer; Esc cancels. The pointer is followed by a subscription that only runs while something is pressed or carried.

## Reader mode

*Reader Mode* (the View menu on a Mac, or the expand button of a Bible tab's bar) shows one Bible tab alone and full screen: the text, with margins on both sides for the cards if those are on, and nothing else. Controls sit quietly in the corners, small and faint: **back** and **forward** at the top left (the tab's own history first, then the tab it replaced, which ends reader mode), and at the top right a toggle for the margins and a way out (Escape does the same). Entering and leaving it keeps the reader at the same share of the way down the chapter, though the text is laid out again at another width. Opening anything else (a link to a note, say) ends reader mode.

## The reader's menu, references, and dictionaries

A selection is one at a time: words selected in the text, and words selected in a card or in a dictionary entry's pieces, are never held together (a new selection lets go of the other). The hamburger at the right of a Bible tab's bar turns on and off what the text shows: Strong's numbers, marks (their highlights and cards), footnotes, and references. **References** are the module's own cross references (`<note type="crossReference">`): a ↗ marker where the module has them, which opens a pop-over listing each passage as a link with the start of its text. Dictionary entries are always pop-overs, never margin cards; the Strong's pop-over has an **Open in tab** button that opens the definition in a dictionary tab. `[[%G25]]` (a Strong's number) and `[x](sword:Module/key)` links open there too.

## Saved state (`state.rs`, `workspace.rs`)

Three small JSON files in `<data home>/state/`, written through a temporary file, and an unreadable one is renamed `*.bad` instead of failing the start:

- `settings.json`: the color scheme (the name of any of iced's themes), a text size for Bible panes and one for note panes, and the font family for Bible and note text. A size applies to the whole pane: in a Bible pane the reading text, footnote and dictionary cards, and margin cards (these at 75% of it, so notes sit quietly beside the text) and the chapter title (headings and small labels in proportion); in a note pane the writer's text. Changed in the Settings window (its own window, opened from the top bar, so the workspace stays as it is), applied and saved at once. Missing or out-of-range values are repaired on load. The window is fixed-size and cannot be minimized. On macOS (`native.rs`) it is also made a child of the workspace window, so it moves with it and shows over it in full screen; elsewhere it is a plain small window. ⌘, (Ctrl+, elsewhere) opens it from anywhere, and on macOS the application menu has a Settings… item (built with `muda`, which replaces the system's default menu with the app menu: About, Settings…, Services, Hide, Quit).
- `session.json`: the workspace as it was left, restored at start-up. It is written about a second after any change (so scrolling is not a write per pixel) and at once when the window closes.
- `layouts.json`: workspaces saved by name from the **Layouts** list: click one to load it, × to delete it, or "Save as new layout…". **Default** is always first and built in (not in the file, cannot be deleted or replaced): the Bible on the left half, the first note on the right. "Save as new layout…" at the bottom, which asks for a name in a dialog. Loading one replaces what is open; saving under a name already used replaces it.

A saved workspace holds the split tree (axes and sizes), each pane's tabs and which one shows, the focused and zoomed pane, and per tab what it shows: Bible module, book, chapter, scroll, and which markers are on; note path; dictionary and key. Restoring leaves out a tab whose module or note is gone (a chapter the module lacks opens at its first), drops panes left empty, and changes nothing if no tab at all can be put back.

## Not yet

- A tab's back/forward history, and a note's cursor and scroll, are not kept between sessions or in saved layouts.
- Browsing or searching a dictionary's keys (a dictionary tab looks up one key at a time).
- More tab types: search results, a commentary.

## Icons

Icons are SVG files in `crates/app/assets/icons` (strokes on a 24 unit square, no colour of their own), built into the program and drawn by `icons.rs` in the colour of a secondary button's label. To add one: drop in an SVG with `viewBox="0 0 24 24"`, add a variant to `Icon`, and use `Icon::X.view(size)`. Anything that is a picture should be one of these, not text glyphs or boxes drawn in code.

## Commentary tabs

A commentary (a SWORD `zCom` module such as Gill's Exposition) opens in its own tab, beside the reader. It shows the notes on one chapter, verse by verse, under the verse each starts at (a note that covers several verses shows once). Pickers choose the commentary, book, and chapter, and the arrows step to the previous or next chapter across books. Notes are converted from ThML or OSIS to paragraphs with bold, italic, and links; a passage a note points to is a link that shows the verse in the reader (with the same modifier keys as any link). The reader's menu has *Commentary…*, which opens (or reuses) the commentary tab at the chapter, and scrolls toward the selected verse. The text is the editor's own widget in a read-only mode, so it selects like any text (drag, double and triple click, Shift-arrows, Select All) and copies the words without the markdown marks; it cannot be changed. A new tab can also be a commentary, and the tab is saved with the workspace.

## Search tabs

A search tab finds words in one Bible module. The module's verses (their plain words, with no footnotes or Strong's markers) are put in an SQLite full-text index in the cache folder (`bible-<module>.sqlite`) the first time the module is searched, which takes about half a second, and again only if the installed module changes. The query language: words (all must be in the verse), `"a phrase"`, `a OR b`, `-left`, `pre*`, a Strong's number (`G26`, `H430`, found through the module's tagged words), and `in:John`, `in:NT`, `in:AP` (the Apocrypha), `in:Gen-Deut` for where. The results follow the typing (searched when it pauses for a moment; Enter searches at once). Options: match case, other forms of a word (porter stemming), best matches first (else canon order). Results list each verse with its matched words marked and a count per book that narrows the list; a click shows the verse in the reader with just those words selected (and the usual modifier keys choose where it opens). A new tab can be a search; the reader's selection toolbar searches the selected words; Cmd/Ctrl-Shift-F brings the search tab forward. The tab is saved with the workspace (words and options) and searched again when restored if its index is already made.

## Your own commentary

The commentary tab's last choice is *My commentary*: the person's own, a running, ever-changing set of conclusions about each chapter and verse. It is not a different kind of thing from a note. It is one ordinary note for each book (`notes/Commentary on John.md`, type `commentary`, tagged `commentary`), whose only difference is its structure: a `## John 3` heading for each chapter and under it a `### John 3:16` (or `### John 3:16-21`) heading for each verse or run of verses written on, kept in the order of the text. The tab shows the chapter's part of the note, each heading a link to its passage with an *Edit* link that shows the note in the writer with the caret under that heading. A box under the text names a passage; *Add section* puts its heading in the right place (making the note and the chapter's heading first if need be) and opens the note there, or goes to the section if it is there already. With no commentary installed the tab opens on this one.

## The reading log

A chapter is added to the note `Reading log.md` (made the first time, tagged `reading-log`) under a heading for the day, as a bullet with a link and no time (`- [[@John 3]]`); a chapter is in a day once, however often it is read. When today's note already exists (the daily note: a one-a-day kind of note made without asking a title) the chapter is also added under its `## Reading` heading, filling the empty bullet the template starts with (and never making the note). A button at the foot of a chapter's text, *Mark as read*, does it by hand; it shows a green check mark (*Marked as read*) for as long as the chapter is in today's log, and pressing the check takes the mark back (from the log and today's note). A small clock to its left opens a card with the days the chapter was read, latest first, and a way to the log itself; the card floats over the text like the reader's other cards (a click elsewhere or Escape closes it) and does not move the text. Settings → Reading can also have a chapter logged by itself once it has been in front of the reader for a chosen number of seconds (10 to 600, thirty to begin with); that is **off** to begin with, and so is nothing else: the button is always there. A note open in the writer while something is added to it keeps the caret in the words it was in.

The log has a **tab of its own** (*Reading log* in a new tab, or the way to it from the clock card of a chapter): it is the app's note, not one of the notes. It grows **down** (the earliest day first, the latest last), and the tab is shown scrolled to the end, where the caret is. No note leads to it and it leads to none: the notes selector, links (`[[Reading log]]` finds no note), searching the notes, and a chapter's backlinks all leave it out (the plan log is left out of them too), a note tab will not show it, and a link to a note in the log does nothing (a link to a verse, or a web address, does what it does anywhere). The tab is written in with everything a note has (the Vim keys, links, embeds), but it has no header, no selector, history, or menu, no frontmatter section, no rename, and its title (`# Reading log`) is shown but out of reach of the caret, the selection, and editing, like the frontmatter every note has (a `Document::lock_title`). A session saved when the log was a note tab opens the new tab.

The same page of the settings has *Add each new highlight or note to today's note, under Bookmarks* (off to begin with): a new mark (made with a pen, Add Note, or a verse number's toolbar) adds a line such as `- [[margins/John/03/16-highlight-a1|John 3:16]]` (a link to the mark's own note, called by its passage) under the day's `## Bookmarks`, only when today's note is there (the heading is added at the end if it has none, as Reading's is; the Daily template of a new vault has it). The link follows the note if writing in it renames its file.

## Notes

The writer's menu has *Copy link*, which puts `[[Nicodemus]]` on the clipboard (by path and with the name as the label, `[[notes/Nicodemus|Nicodemus]]`, when another note has the same name). A tab keeps where it was scrolled to and puts it back when it is shown again; what a reader was asked to do once (select a verse, scroll to it) is not done again when its tab returns to view.

## Selections held, cards opened apart, the passage of a margin note

- A selection in a reader is a place held, like a dog-ear: pressing elsewhere (another pane, another tab, a note) does not end it, and it is put back, drawn as it was, when the reader is shown again after its tab was put away. Pressing in the reader's own text makes a new one, as ever. The toolbar offered under a selection is shown only while that reader is the one being used.
- A card pressed with the command key (Ctrl elsewhere) opens in a tab of its own, beside the reader's: a margin note's card as its note, a dictionary entry's card as the dictionary tab.
- A margin note shows the passage it is on, as the Bible has it with the marked words in the mark's color, directly above its properties; the reference at the head of it is a link (right click for where to open it, pull to carry it). The properties no longer repeat the link.
- In the writer a bracket or quote typed comes with its partner (`(` `[` `{` `"` `` ` ``) when nothing follows it that it would be put before (and, for quotes, no word is ending: `don't` is left alone); typing the partner steps over it, a selection is wrapped, and Backspace between an empty pair takes both.
