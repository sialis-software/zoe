# Panes and tabs (the meta-layout)

The window is a workspace: **panes** side by side, each holding **tabs**, one showing.
A tab is one of a small set of self-contained *types*, each in its own module with its own
state, messages, `update` and `view`:

| Tab type | Module | Holds | Per-tab state |
|---|---|---|---|
| Bible | `bible_tab.rs` | one reader: module, book, chapter | selection, margin cards, footnote/dictionary cards, popups, scroll position, Strong's setting, the navigation toolbar |
| Note | `note_tab.rs` | one markdown document (a note or a margin file) | the document, the selector panel, the peek strip; a header bar like the Bible tab's |

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

## Not yet

- Dragging tabs between panes, reordering them, and saving the layout between sessions.
- Opening a note in a *new* tab from a link (Ctrl-click); links navigate the current note tab.
- More tab types: dictionary/lexicon browser, search results, a cross-reference list,
  a commentary.

## Icons

Icons are SVG files in `crates/app/assets/icons` (strokes on a 24 unit square, no colour of
their own), built into the program and drawn by `icons.rs` in the colour of a secondary
button's label. To add one: drop in an SVG with `viewBox="0 0 24 24"`, add a variant to
`Icon`, and use `Icon::X.view(size)`. Anything that is a picture should be one of these,
not text glyphs or boxes drawn in code.
