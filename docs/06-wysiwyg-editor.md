# The live-preview editor

Read with 03 §4 (UI technology) and 02 (vault and markdown).

## 0. How it works

The writer pane and the margin cards use one custom editor: the markdown source is the single source of truth, shown block by block with the syntax hidden except where the caret is.

**`crates/md-editor`** (no GUI, 37 tests): `blocks.rs` splits source into one block per line (heading, quote, list/task item, rule, embed, fence, code, frontmatter, paragraph); `render.rs` turns a block into styled display runs plus a display/source offset map, either *revealed* (raw, syntax dimmed) or *rendered* (markers hidden, bullets/checkboxes as decorations, wikilinks as their label with a payload); `document.rs` is the source string, the selection, and an undo log (typing grouped by word, backspace runs grouped, structural commands one step each), with grapheme-aware motion, smart Enter (continues lists, numbers, and tasks, ends them on an empty item, continues quotes), Backspace removing line syntax first, toggles for bold/italic/code, task toggling, indent/outdent.

**`crates/app/src/live_editor.rs`**: lays out each block as its own paragraph, draws decorations (quote bar, code background, rule, bullet, number, checkbox), selection, caret, underline/strike, an `![[passage]]` line as a quote block fed by the app, and a slim scrollbar; handles keys, mouse (click, drag, double/triple-click, Shift-click, Cmd/Ctrl-click to follow a link, clicking a rendered checkbox), the clipboard, input-method commits, focus (as a `Focusable`, so it can be focused by a Task), vertical movement with a remembered column, and caret-follow plus wheel scrolling inside itself (so it works with a height cap, as in cards).

**Decisions taken while building**
- Reveal depends on the kind of line. **Headings, rules, embeds, and code** reveal the whole line when the caret is on it. **Paragraphs, list items, and quotes** reveal only the inline element the caret is in or touching (a bold span, an italic, a code span, a link), so the rest of the line stays rendered and does not move. A selection spanning several lines keeps everything rendered, so the view holds still.
- List bullets, numbers, task boxes, and quote bars stay rendered while their line is edited. The caret cannot rest inside the hidden marker: Left from the start of the text goes to the line above, Right from there lands at the text, Home goes to the text, and Backspace at the start of the text removes the marker.
- **Links in the writer open on a plain click** when they are showing as their label. Following happens when the button comes up without the pointer having moved, so a drag that starts on a link still selects. To edit a link: arrow into it or click beside it (the caret touching it shows its `[[...]]`), Option/Alt-click it, or click it once it is already showing raw. Cmd/Ctrl-click always follows. **In a card** it is the mode that decides: a resting card follows on a plain click, a card being edited places the caret (Cmd/Ctrl-click still follows).
- **An embed's title is a link to its source** (`John 3:16 · KJV`, drawn as a link): clicking it shows the passage in the reader; the body of the embed is part of its line, so clicking there places the caret. A pin to a module that is not installed shows a placeholder naming that module.
- **Rendered links never wrap internally**: every space inside a rendered link is a no-break space (each maps back to its source space), so `John 3:16` stays in one piece. Raw links, shown only while being edited, can wrap.
- **Rendered, a pinned reference shows the module small after a small gap** (a no-break space; its underline sits at the same depth as the reference's) (`John 3:3 KJV`); raw, it is the `@KJV` as typed.
- **Checkboxes and embed titles are buttons**: every click acts, and none starts a selection (a quick second click on a box ticks it back rather than selecting the word beside it).
- A press only becomes a drag after the pointer has moved a few pixels. Pressing reveals syntax, which re-lays-out the line under a pointer that has not moved; without the threshold that was read as a drag and selected text by itself.
- Block per source line, not per paragraph, as CodeMirror does; wrapping happens inside a line.
- Marker runs in a revealed heading keep the heading's size, so revealing a heading does not change its height.
- Copy and cut take the markdown source, never the rendered text.

**Not done:** auto-scroll while dragging past the edge; drawing the IME preedit; bidi-aware caret movement (Hebrew in mixed lines); tables; folding frontmatter; find and spellcheck; accessibility. The drawing code is verified only by tests of its logic and hit-testing, not by eye.

### Formatting bar

Above a note's text (not in the library, which is only read) is a row of buttons: a heading level (paragraph, H1 to H3), bold, italic, strikethrough, code, bulleted, numbered and task lists, quote, divider, code block, a link `[text](address)`, and a link to a passage or note `[[ ]]`. Settings, Appearance, "Formatting buttons above a note" hides it. A narrow pane scrolls it sideways. A tooltip gives the name and, where there is one, the shortcut: Cmd/Ctrl+B, I, E (code), K (link), and Shift+X (strikethrough). A button is lit while the caret is in what it makes (`Document::active_format`), and after one is pressed the keyboard is back in the text.

The edits are in `crates/md-editor/src/document/format.rs`, as `Action`s (`Wrap`, `Heading`, `List`, `Quote`, `Divider`, `CodeBlock`, `Link`, `WikiLink`), each one undo step with the caret put where typing goes on.
- **Line breaks** are shown as paragraphs, which is not what Markdown does (it joins the lines of a paragraph with a space): two lines of text with no blank line between are drawn with a small gap (`PARAGRAPH_GAP` in `live_editor.rs`, a third of a line), and a blank line between them is the bigger gap it always was. Only the drawing differs; the file keeps its lines, so another program shows them joined.
- **Character-level** (bold, italic, strikethrough, code): wrap the selection, line by line if it spans lines, outside its white space. With nothing selected they go round the word the caret is in or touching (letters and digits), and with no word there as a pair with the caret between. Pressed again with the caret inside the span (or on a selection of the span, markers and all) they come off. `***x***` is both bold and italic and taking off one leaves the other. Only `*`, `` ` `` and `~~` are read as markers, not `_`; `***a** b*` (overlapping) is not understood.
- **Line-level** act on the lines the selection touches (not a line it only reaches the start of; not blank lines when there are several; never the frontmatter, a locked title, code or rules). Heading is set, not toggled, and replaces a list or quote marker. A list kind is toggled: if all the lines are that kind they become plain, otherwise they become it (a bullet keeps its character, an item keeps its indent, numbers count from 1). Quote adds `> ` to the lines not quoted, or takes one level off if all are. Divider puts `---` below, after a blank line (under text it would make a heading), with the caret below it. Code block fences the lines, or takes the fences off the block the caret is in.
- **Link**: the selection becomes `[text](|)`; a selected address becomes `[|](address)`; with nothing selected, `[|]()`. In a link already it comes off and the label stays. It is not made over several lines. `[[ ]]` goes round the selection, or empty, and the choices for a link open as when `[[` is typed.

### Vim mode

Settings, Appearance, "Vim key bindings in the writer" (off by default) turns on modal editing in the writer. The logic is `crates/md-editor/src/vim.rs`: pure, fed `Key`s, editing the `Document` through `perform`, so each command is one undo step. The widget routes keys to it (`LiveEditor::vim`) and draws a block cursor in Normal and Visual mode; the note tab keeps the state and shows `-- NORMAL --`, `-- INSERT --`, or `-- VISUAL --` under the text. In Insert mode only Esc and Ctrl-[ are Vim's, so typing, auto-pairing, and input methods work as usual. In Normal mode Esc with nothing pending lets go of the keyboard, as it does with Vim off. A mouse selection puts Vim in Visual mode; a click leaves it in Normal mode. Cmd (and on other systems Ctrl) shortcuts keep working, apart from Ctrl-r, Ctrl-d, and Ctrl-u.

Supported, all with counts:

- Motions: `h j k l` (by source line, not wrapped line), `w b e W B E`, `0 ^ $`, `gg G {n}G`, `f t F T ; ,`, `%`, `{ }`, `+ -`, Ctrl-d and Ctrl-u (ten lines), and the arrow keys.
- Operators `d c y > <` with every motion, doubled (`dd cc yy >> <<`), and with the text objects `iw aw`, `i" a"` (also `'` and backtick), `i( a(` (`b`), `i[ a[`, `i{ a{` (`B`), `i< a<`. Shortcuts `D C Y x X s`. `>` and `<` move two spaces.
- `r{char}`, `J`, `p P` (charwise and linewise), `~`, `u`, Ctrl-r, and `.` (repeats the last change, including the text typed in Insert mode).
- Insert: `i a I A o O`, with a count repeating the text. `o` continues a list or quote, as Enter does.
- Visual `v` and `V`: motions and text objects, `d x c s y > < ~ u U J r p o`, `v`/`V` to switch, Esc to leave.

Deliberate gaps: named registers and the system clipboard (the unnamed register is Vim's own), marks, macros, search (`/ ? n N *`), blockwise Visual, `g~ gu gU gJ`, `ip ap`, `H M L`, `:` commands, and `.` after a Visual operator. `j` and `k` do not follow wrapped lines. `c` is two undo steps (the delete, then the typing).

## Auto-closing pairs

Typing `(`, `[`, `{`, `"` or a backtick puts in its partner and leaves the caret between, unless a word follows (or, for quotes, a word is ending before it, so `don't` and the quote that closes a phrase are left alone). Typing the partner right before the one that is there steps over it; with text selected the pair is put round it and it stays selected; Backspace between an empty pair takes both. Making a pair is one step of undo, and pasting makes none. `[[` for a link comes out as `[[]]`. See `Document::auto_close`.
