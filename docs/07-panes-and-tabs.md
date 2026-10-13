# Panes and tabs (the meta-layout)

The window is a workspace: **panes** side by side, each holding **tabs**, one showing.
A tab is one of a small set of self-contained *types*, each in its own module with its own
state, messages, `update` and `view`:

| Tab type | Module | Holds | Per-tab state |
|---|---|---|---|
| Bible | `bible_tab.rs` | one reader: module, book, chapter | selection, margin cards, footnote/dictionary cards, popups, scroll position, Strong's setting, the navigation toolbar |
| Dictionary | `dict_tab.rs` | one entry of a dictionary or lexicon | the dictionary, the key being looked up, the entry shown |
| Note | `note_tab.rs` | one markdown document (a note or a margin file) | the document, the selector panel; a header bar like the Bible tab's |

Everything a tab type needs to read or change that is not its own (the backend, the notes,
the marks, the vault, theme and status) is in `Shared` (`shared.rs`); tabs get `&Shared` to
draw and `&mut Shared` to update. Nothing in a tab type knows about panes.

## The workspace (`main.rs`)

`App` owns `Shared`, the tabs (by `TabId`), and the panes (ordered tab ids plus the active
one). It routes `Message::Bible(id, …)` and `Message::Note(id, …)` to the tab, and carries
out what one tab asks of another, so tab types never reach into each other:

- A tab returns a **request** when it cannot do something alone (`bible_tab::Request`,
  `note_tab::Request`): open this note, show this verse, look up this word.
- A change that other tabs must follow is recorded in `Shared` as an **event** (a mark was
  removed, a file was rewritten, renamed or deleted); `App::settle` passes events to the
  tabs showing them after every update.
- **Which Bible tab?** Notes work with the *reference* Bible tab: the one last used if it is
  showing, else any showing one. Its selection feeds the strip of notes on the selected verses and its module
  serves embeds and previews. A verse link in a note shows in it, opening a Bible tab if
  there is none.
- **Which note tab?** Opening a note (a `[[link]]` in a card, "Open" on a margin card) shows
  it in the tab already holding it, else navigates the note tab last used, else opens a new
  one on the right. Two tabs on the same note follow each other's typing.

## Taking verses to a note: the clipboard

Notes are plain text, so verses travel by the clipboard. The toolbar under a selection in a
Bible tab has **Copy reference** (`[[John 3:3-5]]`) and **Copy quote** (`![[John 3:3-5@LSB]]`,
pinned to the module it was taken from); paste them into any note. Copying puts the selection and
its toolbar away and shows a toast (`Shared::toast`, a line over the bottom of the window for
about two seconds; a clock subscription runs only while one is showing). A selector panel or other
pop-up closes when anything outside it is clicked, in any pane.

## Tags (not folders)

People find notes by tag; folders are the app's business. Every new note is filed in `notes/` (the
templates say what they are with a starting tag: `prayer`, `topic`, `daily`) and the selector lists
notes flat, with chips for the tags in use (with counts, colored if a pen has the tag). Click chips to
narrow (a note must have them all; `topic` includes `topic/grace`); the search box matches names,
text and tags, and `#words` match tags only. Tags are kept lower case, `-` for spaces, `/` to nest.
A tag box offers the tags in use as you type, in the note's property strip, in the card of a mark
being written in (so a mark's note can be tagged from the text), and when naming a pen's tag.

**Pens** (Settings) are tags with a color: each is a toolbar button that marks the selected text with
that style (highlight or underline) in that color and opens a note on it tagged with the tag.
Clicking a pen's swatch cycles its color; add one by typing a tag.

## New notes and templates (`core/src/templates.rs`)

The note tab's menu has **New note…**, which asks what kind (and then for a title, unless the kind
says it needs none). The kinds are the **templates**: ordinary markdown notes in the vault's
`templates/` folder (a new vault is given Prayer list, Topic, Daily and Blank as files, once), so
they sync and back up with the rest of it. They are not found by the note selector or its search (changing
what a kind starts with is for those who go looking): the pencil beside a kind in the New note dialog opens
its template to edit. Adding a file adds a kind; deleting one removes it.

A template is written as in Obsidian: its text after the frontmatter is the new note's text, with
`{{title}}`, `{{date}}` / `{{date:dddd, MMMM D, YYYY}}` and `{{time}}` / `{{time:HH:mm}}` filled in
(moment's letters). Whatever the app does differently for a kind is in the template's frontmatter, as
properties starting with `template_` (not copied to the notes):

| property | meaning |
|---|---|
| `template_description` | the line under the kind's name in the dialog |
| `template_title` | the title when none is typed or asked (`{{date:YYYY-MM-DD}}` for a daily note) |
| `template_ask_title` | `false`: make the note at once, without a title box |
| `template_unique` | `true`: one note under that title; asking again opens it |

Every other property (`tags`, `type`, your own) is what the new note starts with, placeholders
filled. The note's title is the `# heading` it is given (`{{title}}`) and its file name in `notes/`, with
a number added if the name is taken; characters a file name cannot have are left out of the name only.

## The note tab's header

Like the Bible tab's toolbar, and the same height: a selector button (styled like the Bible
tab's pick lists) naming the open note on the left, and a trash button that asks for confirmation in a dialog over the tab (Cancel, or Delete to
remove the note's file; for a margin note, also its highlight). Deleting the last note leaves a
fresh empty one. Unlinking a mark from its
text is only on the mark's card. The selector opens a panel over the note (a click
outside closes it) with a search box and the notes grouped by type, taken from the top folder:
Notes first, other types alphabetically (Prayers…), margin notes last, each labelled by its
passage ("John 3:16 · note"). The group holding the open note starts open, the others closed;
a click on a group's header toggles it. Searching matches names and note text, opens every
group that has a match, and lists name matches first.

## Layout

- **Panes** form a tree of splits (`panes.rs`, pure and tested): the two buttons at a pane's
  bottom right divide it in two, side by side (▥) or top and bottom (▤), the new half
  starting with a blank tab. The dividers
  are dragged with the mouse (`split.rs`: a container that owns its divider, so a drag keeps
  working when the pointer strays over the panes). The `□` beside it fills the window with
  that pane, and again restores. Closing a pane's last tab closes the pane and its neighbour
  takes the space; the last pane keeps a blank tab.
- **Tabs** look like a browser's: a chip with the name and a × inside, and a `+` for a new tab
  right after the last one. A new tab is **blank**, with a selector (every Bible module, every
  note) that turns it into that type in place. A Bible opens where the Bible last used is
  reading, so a second translation lines up with the first.
- **Dictionaries never take a margin card.** A Strong's entry is always a pop-over under the
  click; footnotes are margin cards when the reader has a margin, a pop-over when it does not.
- **Fonts.** The interface names a real family (`fonts.rs`) and the build uses a patched
  cosmic-text (`vendor/cosmic-text`, one condition in `shape.rs`): upstream 0.15.0 shaped a
  whole ASCII word with its first character's font, so in `**bold**` the word `**bold`
  lost its bold while the markers showed. Drop the patch when iced moves to a release
  with the fix.

Widget ids that must be unique (the reader's scroll area, the card editor, the note editor)
are derived from the tab id, so two Bible tabs on screen do not answer for each other.

## Where a link opens

Every link, from any kind of tab, is followed to *a place* (`opening.rs`). A plain click opens it
**here**; keys held while clicking, or the right-click menu, send it elsewhere:

| Click | Opens |
|---|---|
| plain | here: in this tab (a note's verse links go to the Bible tab it works with; a dictionary link to the dictionary tab already open, else the other pane) |
| ⌘/Ctrl | in a new tab beside this one |
| Alt | in the other pane (its tab, if it has one of the right kind; else a new tab there; the next pane in order, splitting if there is only one) |
| Shift | in a new pane, split off this one |

Right-clicking a link in the reader, a note or a card offers the same four, and "Copy link"
(the link as it is written in a note). The app follows the keys held through a
`ModifiersChanged` subscription, so any link control gets this without handling keys itself.
A link in a note's text that is being edited keeps its own rule (Alt-click edits it); the
right-click menu is always there.

## Header bars

Every tab's bar is laid out the same way: back and forward arrows, then whatever the tab uses to
choose what it shows (the Bible pickers, the note selector, the dictionary and its lookup box), then
a hamburger menu at the right edge. A note's menu holds Rename and Delete.

## History

Each tab keeps its own history, like a browser tab (`history.rs`, up to 100 places). Back and
forward arrows sit at the left of the tab's bar, greyed when there is nowhere to go; ⌘[ / ⌘]
(Ctrl elsewhere) and the mouse's back and forward buttons act on the focused pane's tab.
A Bible tab records module, chapter, selected verses and how far down it was scrolled (restored
on return); changing module counts as a move, scrolling does not. A note tab records the notes it
showed (renames are followed, deleted notes dropped); a dictionary tab the entries. Going
somewhere new forgets what was ahead. Opening in a new tab or pane starts a fresh history.
Right-clicking the Bible tab's arrows lists its places to jump to.

## Dragging

- **A tab** is pressed (which selects it) and moved a few pixels to pick it up. Over the middle of
  a pane it goes to that pane; over a band along an edge it splits that pane and takes the new
  half on that side (a pane cannot be split for its own only tab); over another tab it goes in
  front of it; over the empty tab bar, to the end. A pane left empty goes away.
- **A link** pulled away from where it is (in the reader, a card or a note) is carried the same
  way and dropped on a pane: a new tab there, or a new pane on an edge. A plain click still
  follows it; pulling it no longer starts a text selection.
- While carrying, each pane shows its drop places (the middle, and a band along each edge) with
  the one under the pointer lit; the name of what is carried follows the pointer; Esc cancels.
  The pointer is followed by a subscription that only runs while something is pressed or carried.

## The reader's menu, references and dictionaries

The hamburger at the right of a Bible tab's bar turns on and off what the text shows: Strong's
numbers, marks (their highlights and cards), footnotes, and references. **References** are the
module's own cross references (`<note type="crossReference">`): a ↗ marker where the module has
them, which opens a pop-over listing each passage as a link with the start of its text.
Dictionary entries are always pop-overs, never margin cards; the Strong's pop-over has an **Open
in tab** button that opens the definition in a dictionary tab. `[[sword:Module/key]]` links open
there too.

## Saved state (`state.rs`, `workspace.rs`)

Three small JSON files in `<data home>/state/`, written through a temporary file, and an unreadable
one is renamed `*.bad` instead of failing the start:

- `settings.json`: the color scheme (the name of any of iced's themes), a text size for Bible panes and one for note panes, and the font family for
  Bible and note text. A size applies to the whole pane: in a Bible pane the reading text, footnote and
  dictionary cards and margin cards (these at 75% of it, so notes sit quietly beside the text) and the chapter title (headings and small labels in proportion); in a
  note pane the writer's text. Changed in the Settings window (its own window, opened from the top
  bar, so the workspace stays as it is), applied and saved at once. Missing or out-of-range values are repaired on load.
  The window is fixed-size and cannot be minimized. On macOS (`native.rs`) it is also made a child of the
  workspace window, so it moves with it and shows over it in full screen; elsewhere it is a plain small window.
  ⌘, (Ctrl+, elsewhere) opens it from anywhere, and on macOS the application menu has a Settings… item
  (built with `muda`, which replaces the system's default menu with the app menu: About, Settings…, Services,
  Hide, Quit).
- `session.json`: the workspace as it was left, restored at start-up. It is written about a second
  after any change (so scrolling is not a write per pixel) and at once when the window closes.
- `layouts.json`: workspaces saved by name from the **Layouts** list: click one to load it, × to delete it, or
  "Save as new layout…". **Default** is always first and built in (not in the file, cannot be deleted
  or replaced): the Bible on the left half, the first note on the right.
  "Save as new layout…" at the bottom, which asks for a name in a dialog. Loading one replaces what is
  open; saving under a name already used replaces it.

A saved workspace holds the split tree (axes and sizes), each pane's tabs and which one shows, the
focused and zoomed pane, and per tab what it shows: Bible module, book, chapter, scroll and which
markers are on; note path; dictionary and key. Restoring leaves out a tab whose module or note is gone
(a chapter the module lacks opens at its first), drops panes left empty, and changes nothing if no tab
at all can be put back.

## Not yet

- A tab's back/forward history, and a note's cursor and scroll, are not kept between sessions or in saved layouts.
- Browsing or searching a dictionary's keys (a dictionary tab looks up one key at a time).
- More tab types: search results, a commentary.

## Icons

Icons are SVG files in `crates/app/assets/icons` (strokes on a 24 unit square, no colour of
their own), built into the program and drawn by `icons.rs` in the colour of a secondary
button's label. To add one: drop in an SVG with `viewBox="0 0 24 24"`, add a variant to
`Icon`, and use `Icon::X.view(size)`. Anything that is a picture should be one of these,
not text glyphs or boxes drawn in code.
