# Obsidian-style live preview editor (research spike)

Status: the research below was done first (nothing built); §0 says what has since been built. Read with 03 §4 (UI technology), 04 §4 ("Editor" decision), and 02 (vault and markdown). Dates and versions below are as of 2026-10-12.

## 0. Status: built as a prototype

The recommended hybrid (B+C, §6) is implemented and in use. It replaced `text_editor` in the writer pane and in the margin cards (so one editor serves both).

**`crates/md-editor`** (no GUI, 37 tests): `blocks.rs` splits source into one block per line (heading, quote, list/task item, rule, embed, fence, code, frontmatter, paragraph); `render.rs` turns a block into styled display runs plus a display/source offset map, either *revealed* (raw, syntax dimmed) or *rendered* (markers hidden, bullets/checkboxes as decorations, wikilinks as their label with a payload); `document.rs` is the source string, the selection, and an undo log (typing grouped by word, backspace runs grouped, structural commands one step each), with grapheme-aware motion, smart Enter (continues lists, numbers, and tasks, ends them on an empty item, continues quotes), Backspace removing line syntax first, toggles for bold/italic/code, task toggling, indent/outdent.

**`crates/app/src/live_editor.rs`**: lays out each block as its own paragraph, draws decorations (quote bar, code background, rule, bullet, number, checkbox), selection, caret, underline/strike, an `![[passage]]` line as a quote block fed by the app, and a slim scrollbar; handles keys, mouse (click, drag, double/triple-click, Shift-click, Cmd/Ctrl-click to follow a link, clicking a rendered checkbox), the clipboard, input-method commits, focus (as a `Focusable`, so it can be focused by a Task), vertical movement with a remembered column, and caret-follow plus wheel scrolling inside itself (so it works with a height cap, as in cards).

**Decisions taken while building**
- Reveal depends on the kind of line. **Headings, rules, embeds, and code** reveal the whole line when the caret is on it. **Paragraphs, list items, and quotes** reveal only the inline element the caret is in or touching (a bold span, an italic, a code span, a link), so the rest of the line stays rendered and does not move. A selection spanning several lines keeps everything rendered, so the view holds still (§8.3, first option).
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

### Vim mode

Settings, Appearance, "Vim key bindings in the writer" (off by default) turns on modal editing in the writer. The logic is `crates/md-editor/src/vim.rs`: pure, fed `Key`s, editing the `Document` through `perform`, so each command is one undo step. The widget routes keys to it (`LiveEditor::vim`) and draws a block cursor in Normal and Visual mode; the note tab keeps the state and shows `-- NORMAL --`, `-- INSERT --`, or `-- VISUAL --` under the text. In Insert mode only Esc and Ctrl-[ are Vim's, so typing, auto-pairing, and input methods work as usual. In Normal mode Esc with nothing pending lets go of the keyboard, as it does with Vim off. A mouse selection puts Vim in Visual mode; a click leaves it in Normal mode. Cmd (and on other systems Ctrl) shortcuts keep working, apart from Ctrl-r, Ctrl-d, and Ctrl-u.

Supported, all with counts:

- Motions: `h j k l` (by source line, not wrapped line), `w b e W B E`, `0 ^ $`, `gg G {n}G`, `f t F T ; ,`, `%`, `{ }`, `+ -`, Ctrl-d and Ctrl-u (ten lines), and the arrow keys.
- Operators `d c y > <` with every motion, doubled (`dd cc yy >> <<`), and with the text objects `iw aw`, `i" a"` (also `'` and backtick), `i( a(` (`b`), `i[ a[`, `i{ a{` (`B`), `i< a<`. Shortcuts `D C Y x X s`. `>` and `<` move two spaces.
- `r{char}`, `J`, `p P` (charwise and linewise), `~`, `u`, Ctrl-r, and `.` (repeats the last change, including the text typed in Insert mode).
- Insert: `i a I A o O`, with a count repeating the text. `o` continues a list or quote, as Enter does.
- Visual `v` and `V`: motions and text objects, `d x c s y > < ~ u U J r p o`, `v`/`V` to switch, Esc to leave.

Deliberate gaps: named registers and the system clipboard (the unnamed register is Vim's own), marks, macros, search (`/ ? n N *`), blockwise Visual, `g~ gu gU gJ`, `ip ap`, `H M L`, `:` commands, and `.` after a Visual operator. `j` and `k` do not follow wrapped lines. `c` is two undo steps (the delete, then the typing).

## 1. Summary and recommendation

**What the user wants.** A writer pane where markdown syntax is hidden or rendered (big headings, bold, styled links, checkboxes, callouts, code blocks, verse embeds), and the raw syntax of the element under the cursor is shown for editing.

**Finding 1: iced's `text_editor` cannot be stretched to do this, and a small fork will not do it either.** The highlighter can only set colour and font per range. More importantly, the editor assumes one uniform line height in the places that matter for editing (caret position, selection rectangles, IME cursor rectangle, scroll window). A fork has to replace most of the editor, not patch it (§3).

**Finding 2: cosmic-text 0.15 is a better base than iced's use of it.** The layout engine already supports a per-span font size and line height (`Attrs::metrics_opt`), and the line height of a visual line is the maximum over its glyphs. So large headings in the same buffer are possible at the shaping level. What cosmic-text does not have is a notion of hidden text or inline widgets, and its `Editor` has no concept of a "display text" that differs from the source (§4).

**Finding 3: no existing Rust crate gives us this for iced.** Nothing found is a reusable live-preview editor. The nearest are design references (Zed's display-map layering, Obsidian, and CodeMirror 6 decorations) and one egui app (Ferrite) with a rendered editing mode (§5).

**Recommendation.** Do it natively, in stages, and keep the model separate from the widget:

1. **Stage 0 (S, now):** keep `text_editor` plus the highlighter; add a read-only rendered preview of the block under the cursor, and of `![[...]]` embeds, below the editor or in a side panel. This buys the Scripture-embed feature and a rendered view for little cost.
2. **Stage 1 (M, no UI risk):** build a GUI-agnostic `editor-model` (parse markdown into a block list, project source to display runs, decide what is revealed for a cursor position, map offsets both ways). Unit-testable without iced. This is needed by every architecture.
3. **Stage 2 (spike, M):** a throwaway custom widget for one paragraph block plus one heading, built from `Paragraph::with_spans` as in `reader_text.rs`, proving caret, IME, hit-testing, and reveal-on-cursor. Go/no-go checkpoint (§9).
4. **Stage 3 (XL):** the real widget: **a block-segmented custom editor widget** (a hybrid of architectures B and C): the raw markdown string is the single source of truth, the document is split into blocks, each block is shaped as its own display paragraph, and the cursor and selection live in one document-wide model (the same `Pos { block, offset }` idea as `reader_text.rs`). Undo, IME, and clipboard are ours.

Fallback if the Stage 2 checkpoint fails: ship Stage 0 as the v1 editor and revisit a webview (architecture E), accepting that it is not "native" and has platform caveats (§6).

**Three biggest risks:** (1) owning IME, caret, undo, and keyboard navigation ourselves (everything `text_editor` gives for free is gone); (2) layout jumps and cursor mapping when raw syntax appears and disappears as the cursor moves; (3) the iced fork/versions treadmill: iced 0.14 pins cosmic-text 0.15 while cosmic-text is already at 0.19, and the widget will depend on iced internals (`Paragraph` details, renderer behaviour).

## 2. What the app already has

- `crates/app/src/editor.rs`: iced `text_editor` with a `Highlighter` (colour and font per range: heading, bold, code, quote, rule, verse/note/sword links). Link parsing comes from `study_core::markdown::find_links`. This is consistent with the decision in 04 §4: v1 is coloured markdown source.
- `crates/app/src/reader_text.rs` (984 lines): a read-only selectable rich-text widget. It builds one `Para` (`iced::advanced::graphics::text::Paragraph`) per block via `Para::with_spans`, lays blocks out vertically itself, hit-tests with `Paragraph::hit_test`, finds spans with `hit_span`, gets per-line geometry from `para.buffer().layout_runs()` (`LayoutRun::highlight` for selection rectangles, glyph `metadata` to recover the span), and draws with `renderer.fill_paragraph` plus `fill_quad` for backgrounds, selection, and underlines. It already has a document-wide `Pos { block, offset }` selection model, drag/word selection, multi-block copy, link hover/click, and edge auto-scroll. All of this is reusable as the read side of an editor. What it lacks: a caret, text input, IME, undo, key navigation, editing at all.

## 3. Findings Q1: iced `text_editor` and `Highlighter`

All verified in source (iced_widget 0.14.2, iced_core 0.14.0, iced_graphics 0.14.0).

**What the highlighter can do.**
- `Highlighter` trait (`iced_core/src/text/highlighter.rs`): per line, returns `(Range<usize>, Highlight)` pairs. The widget maps a highlight to `Format { color: Option<Color>, font: Option<Font> }`. That is the whole output type.
- In `iced_graphics/src/text/editor.rs` `Editor::highlight`, for each visible line it builds an `AttrsList::new(default attrs)`, adds a span per range with colour and (if given) a font-derived attribute set, and calls `line.set_attrs_list`. Metrics are never set per span, so no font size or line height change per range.
- Highlighting is only run for lines from the highlighter's `current_line` to the last visible line, sequentially, with `change_line` notifications when text changes. It has no cursor input. A cursor-aware highlighter is possible only indirectly: `Settings` (must be `PartialEq + Clone`) can carry the cursor line, and a settings change goes through `update`; our `update` could reset `current_line` to 0. That allows "colour differently near the cursor", but still not hide or resize.

**What it cannot do.**
- Font size, line height, hidden text, inline widgets, block decorations, backgrounds, or borders per range, underline, letter spacing.
- Hidden text specifically: the buffer text is the displayed text. There is no display/ source split anywhere in `Editor`.

**Uniform line height is baked into the editor logic** (this is what kills a small fork):
- `Editor::selection()` computes caret and selection rectangle y as `visual_line_index * buffer.metrics().line_height - scroll`, one global line height. With a 28 px heading line in a 20 px buffer, every y below it is wrong.
- `Editor::highlight` computes the visible window as `bounds.height / line_height`.
- `text_editor.rs` `draw` and `input_method` build the caret and IME rectangles as `Size::new(1.0, line_height.to_absolute(text_size))`, again global.
- `Editor::update` sets `buffer.set_metrics` once from widget size and line height.
- Scrolling is internal to the buffer (`Action::Scroll { lines }` times line height), not a parent `scrollable`.

**Other limits found.**
- No undo or redo in the widget or the iced `Editor` trait (a grep of `text_editor.rs` and `iced_core/src/text/editor.rs` finds none). cosmic-text has `Change`/`apply_change`, but iced does not call it. Undo for the current editor would be our own snapshot stack.
- `Editor::move_to` ignores cursor affinity (a TODO in source).
- The editor is a concrete `iced_graphics` type reached through the `text::Editor` associated type on the renderer. A forked `text_editor` widget alone cannot change how lines are laid out or how the caret is computed; we must also replace `Editor` (iced_graphics), which means forking or `[patch]`-ing `iced_graphics` and `iced_core` and tracking iced upgrades. `Internal` is wrapped in `Arc` with `Arc::try_unwrap` and a "TODO: handle multiple strong references", a fragile structure to inherit.
- Pointer actions pass through `cosmic_text::Action::Click/Drag` with integer pixel coordinates, using cosmic's own hit test (which handles per-line heights, since the layout does).

**Could a minimal fork support per-range size/weight/colour/hidden?**
- Colour and weight: already supported.
- Size: needs `Format` to carry `size`, `Editor::highlight` to attach `Attrs::metrics`, and fixes to every y-computation listed above (replace with a loop over `buffer.layout_runs()` using `run.line_top` and `run.line_height`, which cosmic already provides per visual line). Moderate, doable, but the fix surface is spread over iced_graphics and iced_widget. Estimate M.
- Hidden: has no clean route in this design (see §4). Real hiding needs a source/display offset map, which does not exist in `Editor`. Zero-size hacks are fragile (§4).
- Embeds and block widgets: no. There is no place to reserve vertical space for a non-text block inside one `Buffer`, except by faking lines of a given height. Overall: a fork gets you headings and bold. It does not get you live preview.

## 4. Findings Q2: cosmic-text 0.15.0

Verified in `cosmic-text-0.15.0/src`.

- **Rich text.** `Buffer::set_rich_text(font_system, spans: IntoIterator<(&str, Attrs)>, default_attrs, shaping, alignment)` builds the buffer from spans; each `BufferLine` has an `AttrsList` of byte-range spans. iced's `Paragraph::with_spans` uses exactly this, attaching `attrs.metadata(i)` (the span index) per span. That is why `reader_text.rs` can find the span of a glyph via `glyph.metadata`.
- **`Attrs` fields** (`attrs.rs`): `color_opt`, `family`, `stretch`, `style`, `weight`, `metadata: usize`, `cache_key_flags`, `metrics_opt: Option<CacheMetrics>`, `letter_spacing_opt`, `font_features`. No underline, strikethrough, or background; we draw those ourselves from glyph geometry, as `reader_text.rs` already does.
- **Per-span font size and line height.** `Attrs::metrics(Metrics)` sets `metrics_opt`. In `shape.rs` layout, a glyph's size is `glyph.metrics_opt.map_or(font_size, ...)`, and each layout line's `line_height_opt` is the max of its glyphs' line heights (`shape.rs` around lines 1595 to 1690). So mixed sizes in one paragraph work, and per-visual-line heights are correct in `layout_runs()` (`run.line_top`, `run.line_height`, `run.line_y`). iced's `Paragraph::with_spans` already forwards `Span::size` and `Span::line_height` into `metrics_opt` (verified in `iced_graphics/src/text/paragraph.rs`). iced's `Editor` simply never does.
- **Hidden text.** There is no hidden or zero-width attribute. Options, none verified by running code: (a) a tiny font size via `metrics_opt` (a size of 0 may misbehave in shaping, untested); (b) negative `letter_spacing` (present in 0.15, but iced's `Span` does not expose it, and it would still leave glyph rendering and cursor stops in odd places); (c) **do not hide at the cosmic level**: shape a *display string* that omits the hidden characters, and keep an offset map between source and display. (c) is the robust choice and is also how CodeMirror does it (replace decorations plus atomic ranges, see Sources). Cost: our own cursor and selection must be expressed in source offsets and translated through the map for drawing and hit testing, so cosmic's `Editor` cannot be the owner of the cursor.
- **`Edit`/`Editor` API.** `Edit` trait (`edit/mod.rs`): `cursor`, `set_cursor`, `selection`, `set_selection`, `selection_bounds`, `copy_selection`, `delete_selection`, `insert_string`, `action(Action)`, plus undo plumbing: `start_change`, `finish_change() -> Option<Change>`, `apply_change(&Change)` where `Change` is a list of `ChangeItem` (insert/delete with start/end/text). `Editor` also implements `cursor_motion`-based motions via `Buffer::cursor_motion` (word, line, page, buffer, `LayoutCursor`). `ViEditor` (`edit/vi.rs`) and `SyntaxEditor` (`edit/syntect.rs`, feature-gated) wrap it. The undo model is usable, but it records edits to the buffer text, which for us would be the *display* text, not the source. Pointer hit testing: `Buffer::hit(x, y) -> Option<Cursor>` (`buffer.rs:888`).
- **Cursor and affinity.** `Cursor { line, index (byte), affinity }`. Affinity matters at wrapped line ends and bidi boundaries; iced's wrapper drops it in `move_to`.
- **What the `Editor` does not give.** Drawing (the `Editor` draws via a callback in cosmic-edit, iced draws the buffer itself), IME, clipboard, an undo stack with grouping, block decorations. A live-preview editor needs: display/source mapping, per-line style, block-level layout, atomic ranges. Only the shaping part is covered.
- **Version drift.** iced 0.14 depends on cosmic-text 0.15.0; crates.io shows cosmic-text at 0.19.0 (updated 2026-04-22). A custom widget compiled against iced 0.14 uses iced's re-exported 0.15 (`iced::advanced::graphics::text::cosmic_text`, as `reader_text.rs` does). We cannot freely use newer cosmic-text with iced's `Paragraph`. Verify upgrade impact when iced 0.15 appears (crates.io lists iced 0.14.0 as the newest, 2025-12-07).
- **Paragraph API for a custom widget** (iced_core `text/paragraph.rs`, verified): `hit_test(Point) -> Option<Hit>`, `hit_span`, `span_bounds`, `grapheme_position(line, index) -> Option<Point>`, `min_height`, `resize`, `compare`. With `buffer()` on iced's concrete `Paragraph` we get layout runs. `grapheme_position` gives caret placement (note: `line` here is the buffer line, so use display offsets). One limitation: iced's `Paragraph` can only be built from spans or plain text, not from an arbitrary cosmic `Buffer`, so we express everything as spans (fine; that is how `reader_text.rs` works).

## 5. Findings Q3: existing Rust projects and crates

Metadata from crates.io and GitHub APIs fetched 2026-10-12. "Design reference" means read for ideas, not reuse.

| Project | What it is | Licence | Maturity / last activity | Use for us |
|---|---|---|---|---|
| cosmic-text | Pure-Rust text shaping/layout/editing buffer | MIT OR Apache-2.0 | 0.19.0 (2026-04-22), repo pushed 2026-09-30; used by iced 0.14 at 0.15 | Already our base |
| cosmic-edit (pop-os) | COSMIC code editor, built on cosmic-text `Editor` | GPL-3.0 | repo pushed 2026-10-08 | Design reference only (GPL; also source editor, not live preview). Do not copy code, see 04 §4 licence stance |
| iced `text_editor` | Our current widget | MIT | iced 0.14.0 (2025-12-07) | Reference for IME/key binding plumbing; reuse patterns |
| iced_selection | Third-party text selection for iced; its markdown example puts `text_editor` beside a rendered preview | MIT | 0.5.0 (2026-02-05), ~15k downloads, hosted on sourcehut | Possibly useful for the Stage 0 preview; not an editor. Evidence from search snippet and docs listing only; not read in depth |
| Zed / GPUI | Editor with a layered DisplayMap (inlays, folds, tabs, wraps, blocks, creases) | `gpui` Apache-2.0 (crates.io 0.2.2, 2025-10-22); Zed editor crate is not Apache (repo licence shown as "Other"/GPL family, unverified for the editor crate) | Very active (repo pushed 2026-10-12, ~92k stars) | Best design reference for the source/display layering and block-level decorations. Not usable with iced, and adopting GPUI means leaving iced |
| Lapce | Rust code editor | Apache-2.0 | Active (pushed 2026-10-09, ~39k stars) | Design reference only |
| Floem | Lapce's UI library, has a text editor view | MIT | Active (pushed 2026-09-24, ~4.3k stars) | Design reference; not for iced |
| Xilem / Masonry / Parley | Linebender stack. Parley is rich-text layout (Apache-2.0, pushed 2026-10-12). Masonry 0.4.0 (2025-10-29) | Apache-2.0 | Experimental ("An experimental Rust native UI framework") | Parley is an alternative text layout with editing support, but not usable with iced's renderer. Reference only. I did not inspect its editor API |
| egui + Ferrite | Markdown editor app with a "Rendered" WYSIWYG mode, rope buffer (ropey), comrak | MIT | v0.3.0 per its README; pushed date not confirmed | Reference for a block-click-to-edit model; it is egui, not reusable. Read from README summary only, not verified in code |
| iced_wry | Embeds a wry WebView as a child window in an iced app | MIT OR Apache-2.0 | 0.1.0 (2026-03-27), 121 downloads | Only relevant to architecture E; very immature |
| iced_servo / iced_webview_v2 | Servo or Blitz/litehtml rendered into iced textures | not checked | not checked | Listed in search results; not evaluated |
| CodeMirror 6 | Web editor framework; Obsidian's live preview is built on it | MIT | Mature | The conceptual model to copy (replace/widget/block decorations, atomic ranges) |

Not found: any Rust crate that implements Obsidian-style live preview for iced. egui has `egui_commonmark` for viewing markdown; I did not verify an editing equivalent. State plainly: absence of evidence from web search, not proof that none exists.

## 6. Findings Q4: candidate architectures

Sizing scale as in 04 §1 (S days, M 1-3 weeks, L 1-2 months, XL 2+ months, one developer).

### A. Fork iced `text_editor` + cosmic editor wrapper

Add styled and hidden ranges, driven by a markdown parser that reveals syntax near the cursor.

- Pros: inherits IME plumbing, key bindings, clipboard, mouse selection, scrolling.
- Cons: uniform line height assumed in at least five places (§3); no hiding without a display/source map (which then breaks `Editor` cursor, selection, `copy`, edit actions, because they all work on buffer text); no block widgets (embeds); fork spans `iced_widget`, `iced_graphics`, `iced_core`, and must be rebased on every iced release; `Arc::try_unwrap` internals.
- Effort: L to get headings and bold working; **XL and fragile** to get hiding and embeds.
- Key risk: we end up rewriting the editor inside someone else's structure.
- Verdict: **not recommended** beyond a size-per-range experiment.

### B. Custom widget on cosmic-text `Editor`/`Buffer` directly, reusing `reader_text.rs`

- Pros: full control; reuses drawing, hit testing, selection geometry, link handling, and the `Pos{block,offset}` selection model.
- Cons: must implement caret, key map, IME, clipboard, undo, scroll, accessibility. If a single `Buffer` is the whole document, hiding still needs the display/source map, and embeds still need space reserved inside the buffer.
- Effort: XL.
- Key risk: IME and hidden-text/caret mapping.
- Variant used in the recommendation: **one display paragraph per block** (below), not one buffer for the document. This keeps shaping local (cheap re-layout on edit) and makes embeds and block-level styling natural.

### C. Block-based editor (Notion-style)

Each block is its own text editor; the focused block shows raw markdown.

- Pros: simple mental model; matches Obsidian's per-line reveal for most content; embeds, tables, code blocks, and callouts are just block types; reveal is block-granular so layout jumps are contained to one block.
- Cons: cross-block selection, navigation (up/down arrows keep an x position across blocks), copy and undo across blocks are the hard parts; separate editor widgets per block make selection across blocks very hard in iced (each widget owns its own selection).
- Effort: L with one editor widget per block (but cross-block selection will not work well), XL with a document-level cursor.
- Key risk: cross-block selection. Mitigation: a single widget owns all blocks and the selection (this is B's hybrid).

### B+C hybrid: **single custom widget, block-segmented display, document-level cursor** (recommended)

- Source of truth: the raw markdown `String` (or rope later). Every edit is a `(range, replacement)` against the source, so the file on disk is always what the user typed. No round-trip risk.
- Model crate: parse into blocks with source ranges; for each block produce display runs (text, style, role, source range, hidden/reveal), plus offset maps source <-> display. The cursor decides which blocks/inlines are "revealed".
- Widget: for each block, shape a `Para` from display runs; `reader_text.rs` already does per-block layout, selection, hit-test, and draw. Add caret (`grapheme_position`), key handling, IME, clipboard, undo (operation log over source edits).
- Effort: XL (6 to 10 weeks for one experienced developer is my order-of-magnitude guess, not a measurement).
- Key risk: IME, undo granularity, layout jumps (§8).

### D. Document-model (ProseMirror-style) editor

Model is a rich tree; markdown only for save and load.

- Pros: edits are structural, no ambiguity about what the cursor is in; selection and commands are clean.
- Cons: round-trip fidelity. Vault files are plain markdown the user also edits in other tools and sync (see 02); normalising whitespace, list markers, escapes, unknown syntax, frontmatter ordering, and link spelling changes files the user did not mean to change. Obsidian itself avoids this by keeping source as truth. Also still needs all the widget work of B.
- Effort: XL+.
- Verdict: **not recommended.**

### E. Webview fallback (CodeMirror 6 live preview or similar)

- Pros: CodeMirror 6 has decorations, atomic ranges, IME, a11y, undo history, and RTL handled by the browser engine; there are existing live-preview implementations as references (Atomic Editor was named in a search result, not inspected). Lowest editing risk.
- Cons: contradicts the "native Rust" preference; second language and build chain (JS bundle); message bridge between Rust and JS for verse fetching and link clicks; fonts and theming must be kept consistent.
- **Embedding inside an iced window.** Possible but rough. wry's `build_as_child` creates a child view on Windows and macOS (an `NSView` subview) and on Linux X11 only: the wry docs say it "won't work on Wayland", where a GTK container route is needed with a GTK main loop advanced next to ours. `iced_wry` (0.1.0, 121 downloads) wraps this by overlaying a native child window over a placeholder widget. Consequences: the webview is a separate native surface drawn on top of iced's wgpu surface, so iced widgets (popovers, modals, tooltips) cannot overlap it; focus and keyboard handoff between iced and the webview needs care; not tested on our three platforms. The alternative, a separate window, defeats a "pane". Servo or Blitz rendered into a texture (`iced_servo`) avoids the overlay problem but those are experimental.
- Effort: M to build, L to polish embedding and platform quirks.
- Verdict: the right fallback, not the first choice.

### Intermediate: keep `text_editor` + highlighter, add a preview

- Add a rendered preview of the current block (or full note) beside or under the editor, using `reader_text.rs` to render parsed markdown (so selection and links work in the preview), including `![[John 3:16-17]]` embeds as quote blocks. Possibly click-through links in the preview. Add `Ctrl-click` on a link in the editor to follow it (the app can map cursor position to a link via the model without the widget knowing).
- Buys: embeds visible now, checkboxes visible, wikilink follow, no text input risk. Costs: not inline; two panes to look at; scroll syncing is approximate.
- Effort: S to M. It exercises the model crate, so it is not wasted.

### Comparison table

| | A fork text_editor | B custom on Buffer | B+C block hybrid (rec.) | C widget per block | D doc model | E webview | Preview only |
|---|---|---|---|---|---|---|---|
| Size/heading/bold | yes, M | yes | yes | yes | yes | yes | n/a |
| Hide syntax | hard (no map) | needs map | needs map (planned) | needs map | n/a | yes | n/a |
| Embeds inline | no | awkward | natural | natural | natural | yes | outside editor |
| Cross-block selection | built in (1 buffer) | built in | we write (reuse reader_text) | very hard | we write | browser | n/a |
| IME | inherited | we write | we write | per widget | we write | browser | inherited |
| Undo | we add | we write | we write (source ops) | hard | we write | built in | we add |
| File fidelity | exact | exact | exact | exact | risk | exact | exact |
| iced upgrade burden | high | medium | medium | medium | medium | low (but wry) | none |
| Native | yes | yes | yes | yes | yes | no | yes |
| Effort | L, XL for full | XL | XL | L-XL | XL+ | M-L | S-M |
| Main risk | fork rot | IME, mapping | IME, jumps | selection | round trip | embedding, native | not WYSIWYG |

## 7. Findings Q5: embeds, wikilinks, headings

**Wikilinks `[[John 3:16]]`.**
- Model: `find_links` (existing) gives source span and target (`Verse`, `Note`, `Sword`). The display run for a link, when not revealed, is the label (alias or target text) with link style, carrying a link payload in the run. The widget stores the payload per display span, the way `reader_text.rs` stores `Span::link`.
- Click: `hit_span` plus modifiers, as in the reader (`on_link`). In an editor the click must be disambiguated: plain click places the caret (and reveals the raw `[[...]]`), Cmd/Ctrl-click follows the link. Obsidian does the same. Hover shows a pointer on modifier-held only.
- Architecture A/preview-only: link follow on Cmd-click works using a cursor-to-link lookup through the model; no styling change needed.

**Embeds `![[John 3:16-17]]`.**
- Model: an embed is a block whose display content is *not* source text. The model asks `core` for the passage (SWORD read, markup to our AST in 01 §5, 03 §3) and gives the widget a block of kind `Embed { ref, rendered_blocks }`. Keep it read-only and atomic: the cursor treats it as one unit, Backspace over it reveals the source line first rather than deleting 40 characters of invisible text.
- B / B+C / C: draw the embed block as a quote block (left bar, indent) using the same `Block` machinery as `reader_text.rs` (spans with verse numbers, `copy: false` metadata for verse numbers). When the cursor is on the source line, show raw `![[...]]` and the rendered quote beneath, so the user sees both (this is the Obsidian behaviour). The embed content loads asynchronously; the model must tolerate "loading" and "missing module" (show a placeholder with the reference) and invalidate layout when it arrives.
- A (fork) cannot place a non-text block, apart from faking blank lines. E (webview): a CodeMirror block widget fed by a Rust-supplied HTML string.
- Copy: copying a selection that includes an embed should copy the markdown source (`![[John 3:16-17]]`), not the passage text (see 04 non-goals: transclude, never copy verse text into notes; also matches 04 risks "copyrighted module text").

**Headings with larger fonts and cursor/line layout.**
- A: needs the editor patches in §3 (selection y, caret height, visible window).
- B / B+C: each block is its own `Para`; a heading is just a block with a bigger `Span::size` and `line_height`. `grapheme_position` and `layout_runs` give correct caret rectangle and height per visual line, so the caret takes the height of the line it is on. Keep *line height* of a heading the same whether or not `# ` is revealed (see §8), by giving the marker run the heading's size, not the body size.
- C: same as B+C per block.
- E: the browser does it.

## 8. Findings Q6: hard parts that are easy to underestimate

1. **IME composition.** iced exposes `InputMethod::Enabled { cursor, purpose, preedit }` and `text_editor` shows the pattern (verified in `text_editor.rs`: `input_method()`, the `Ime` enum with `Toggle`, `Preedit`, `Commit`). Our widget must report the caret rectangle at the right place (with per-line heights), keep the preedit string out of the document until commit, and make sure that revealing/hiding syntax does not change layout while composing (freeze reveal state during preedit). How preedit is *drawn* (by the window shell or by the widget) was not verified; check iced_winit before relying on it. CJK and dead-key flows need manual testing on all three platforms.
2. **Layout jumps.** When the cursor enters a block, hidden markers appear and text shifts or rewraps; with larger heading fonts a wrapped line can move. Obsidian users report exactly this cursor jumping on show/hide (forum search result, anecdotal). Mitigations: give hidden-but-revealed markers the same font size as the content (not body size); reveal on the *block* (not per inline) when the block is a heading/list item; reveal inline syntax only around the cursor and animate nothing; keep the caret anchored to a source offset, not to a pixel x. Accept a small reflow in a paragraph; do not accept vertical shifts of the line being edited.
3. **Selection across rendered and raw boundaries.** Selection anchors are in source offsets. Highlight rectangles are drawn from display geometry; where a selection covers hidden markers, the hidden range contributes zero width. Selecting through a revealed block while neighbours are rendered gives uneven rectangles. Decision needed: when a multi-block selection exists, either reveal nothing (all rendered) or reveal the endpoint blocks only (Obsidian reveals the cursor/selection lines). Pick the first; simpler and stable.
4. **Undo granularity.** `text_editor` has none, so there is no baseline. Undo must work on *source edits*, grouped by typing bursts (time/word boundaries/cursor jumps), and must restore cursor and reveal state. Structural commands (toggle checkbox, continue list, auto-pair) must be single undo steps. Checkbox clicks in rendered mode are source edits too and must go through the same log. cosmic's `Change`/`apply_change` is available but records against display text; with our source-of-truth design we would write our own.
5. **Copy as markdown vs rendered text.** Two clipboard flavours: markdown source slice (for notes) and plain text (for other apps). Copying from within a rendered block should probably give the source slice. The reader already has a "copy with citation" model (`selected_text`); the editor needs a separate path. Paste must handle rich clipboard content (HTML, URLs on a selection to create links) at least by stripping to text first.
6. **RTL and Hebrew/Greek.** cosmic-text does bidi shaping, and `LayoutRun::highlight` handles RTL (`rtl` flag, per-glyph left/right cursor mapping, verified in `layout.rs`). Risks: caret movement in mixed-direction lines (visual vs logical), cursor affinity at direction boundaries (iced's wrapper drops affinity; our own cursor must keep it), and markers (`**`, `#`) in RTL paragraphs, which should sit on the logical side. Blocks need a direction (auto-detect per paragraph; cosmic's `BidiParagraphs` splits lines). This is not hard to render but easy to get subtly wrong in editing; keep Hebrew test fixtures from the start.
7. **Accessibility.** iced 0.14 has no screen-reader tree for custom text widgets that I could verify (not checked in source; consider this unverified). An editor that renders its own glyphs exposes nothing to VoiceOver/NVDA/Orca. Realistic target: keyboard operability and correct IME; screen-reader support likely out of reach natively. The webview option would get this for free from the browser, which is a real point for E. Note 03 §6 lists accessibility as cross-cutting; flag in M6.
8. **Other items.** Soft-wrap and long-line performance; scrolling and caret reveal when the document is inside a `scrollable` (reader uses `on_scroll`; the editor needs caret-follow scrolling); find-in-note and spellcheck (no support in cosmic-text; defer); tables (large separate feature, treat as raw source in v1); frontmatter collapse (block type with a fold state kept in widget state, not in the file); nested lists and hanging indents on wrap (block `indent`, as in `reader_text.rs`); auto-continuation of lists and tasks; emoji and font fallback; huge notes (shape only visible blocks, as the reader does with `area.intersects(viewport)`); `Tab` and focus traversal.

## 9. Findings Q7: staged plan, with go/no-go checkpoints

Keep to the boundary in 04 §4: markdown logic is separate from the widget.

**Stage 0 (S): preview alongside the current editor.**
- Render the note (or current block) with `reader_text.rs`-style blocks next to the editor, with embeds rendered and checkboxes shown. Cmd-click to follow links from the editor.
- Exit: users can write and see a rendered version. Decide whether this is "enough for v1".

**Stage 1 (M): `editor-model` crate (GUI-agnostic, in `vault`, or a new crate beside it).**
- Input: source string, cursor/selection (source offsets), an `EmbedResolver` trait.
- Output: `Vec<Block>` where each block has kind, source range, display runs with style, link payload and source range, hidden/revealed flags, offset maps both ways, and per-block "raw vs rendered" state derived from the cursor.
- Commands (pure functions on source and selection): toggle bold, toggle checkbox, continue list, indent/outdent, insert link, wrap selection. Return edits `(range, text)` plus the new cursor.
- Parser: `pulldown-cmark` (03 §7) with offsets for blocks and inline spans, plus our own wikilink/embed/callout/frontmatter recognisers (reuse `find_links`). Do not parse inside the widget.
- Tests: property tests for offset maps (source->display->source round trip), snapshot tests for reveal rules, Hebrew/Greek fixtures.
- Go/no-go 1: the model can render the same fixtures into the current highlighter *and* into `reader_text` blocks with no change to its tests. If offset mapping is not clean, stop and rethink before any widget work.

**Stage 2 (M): throwaway widget spike.**
- One heading block and one paragraph block with bold, a wikilink, a verse embed; caret, typing, backspace, arrow keys across blocks, click to place caret, reveal on enter, IME on macOS (Japanese or Pinyin), undo of a typed run.
- Success criteria: (a) the caret never jumps vertically when revealing a heading marker; (b) IME commit and preedit work on macOS and one of Windows/Linux; (c) hit-testing is correct with mixed sizes and hidden runs; (d) frame time for a 5,000-word note stays smooth (blocks shaped lazily).
- **Go/no-go 2:** if (b) or (a) fail with no clear fix in a week, take the fallback: ship Stage 0 as v1 and evaluate a webview editor (E) as a Phase 2 option.

**Stage 3 (XL): the real widget**, built in this order: caret and selection model (reuse the reader's `Pos`), keyboard navigation and word/line motions, clipboard, undo log, IME hardening, inline reveal, block types (lists/tasks, quotes, callouts, code, frontmatter fold, embed), scroll/follow-caret, find. Ship behind a setting ("Source mode" stays available forever as an escape hatch using the current `text_editor`).
- Go/no-go 3 (mid-way): after caret + selection + IME + undo are solid on all three platforms, decide whether to continue to block types. If this took more than ~6 weeks, re-evaluate E.

**Stage 4 (M, optional):** tables, drag-and-drop, spellcheck, accessibility work.

**Sizing summary.** Stage 0: S. Stage 1: M. Stage 2: M. Stage 3: XL. Total about L to XL in 04's scale, consistent with the existing "UI: markdown editor, L-XL" line.

## 10. Open questions

1. Is a fully inline live preview worth XL effort for v1, or is Stage 0 (source plus preview) enough for the M2 milestone? (04 already says live WYSIWYG is a future goal.)
2. Reveal granularity: per block (simple, stable) or per inline element near the cursor (closer to Obsidian, more reflow)? Suggest per block for block markers, per inline for inline syntax.
3. Should the editor read-only-embed the same `reader_text` block types so that selection inside an embed works (copy a verse from inside a note)? Needs a design for selection crossing editable and read-only blocks.
4. How does `core` supply embed content (synchronous from cache, or async with invalidation)? Needs a trait shape agreed with 03 §2.
5. Accessibility target: is "keyboard and IME correct, no screen-reader support" acceptable for v1?
6. Can iced 0.15 (when released) change `Paragraph`, `Editor`, or the cosmic-text version in a way that breaks the widget? Check release notes before Stage 3.
7. Do we want Parley (Linebender) or newer cosmic-text 0.19 features (not inspected) instead of iced's bundled 0.15? That would mean owning the text rendering path, which we should avoid unless a concrete blocker appears.
8. Is Preedit drawn by iced's shell or by the widget (see §8.1)? Verify in iced_winit.
9. Performance of re-shaping a block on every keystroke with mixed sizes, measured on a Hebrew or polytonic Greek note.

## 11. Sources

**Verified in source (on disk, `~/.local/share/cargo/registry/src/index.crates.io-*/`):**
- `iced_widget-0.14.2/src/text_editor.rs`: `input_method()`, `Ime` handling, `draw`, uniform-height caret, no undo, `Binding` enum, highlighter wiring.
- `iced_core-0.14.0/src/text/editor.rs` and `iced_core-0.14.0/src/text/highlighter.rs`: `Highlighter` trait, `Format { color, font }`, `Editor` trait (no undo, no styling).
- `iced_core-0.14.0/src/text/paragraph.rs`: `hit_test`, `hit_span`, `span_bounds`, `grapheme_position`.
- `iced_core-0.14.0/src/input_method.rs`: `InputMethod::Enabled { cursor, purpose, preedit }`.
- `iced_graphics-0.14.0/src/text/editor.rs`: `Editor::selection` (global line height), `highlight` (colour and font only), `update`, `perform`, `move_to` affinity TODO.
- `iced_graphics-0.14.0/src/text/paragraph.rs`: `with_spans` maps `Span::size` / `line_height` to `Attrs::metrics`, `metadata(i)` per span.
- `iced_wgpu-0.14.0/src/text.rs`: editor text drawn directly from `editor.buffer()`.
- `cosmic-text-0.15.0/src/attrs.rs` (`Attrs` fields, `metrics_opt`, `AttrsList`), `buffer.rs` (`set_rich_text`, `hit`, `cursor_motion`, scroll), `shape.rs` (per-glyph `metrics_opt`, max per-line `line_height_opt`), `layout.rs` (`LayoutRun::highlight`, RTL), `edit/mod.rs` and `edit/editor.rs` (`Edit` trait, `Change`, `apply_change`, `start_change`, `finish_change`).
- Project files: `docs/03-architecture.md`, `docs/04-scope-and-roadmap.md`, `crates/app/src/editor.rs`, `crates/app/src/reader_text.rs`.

**Verified from fetched documentation/API metadata (2026-10-12), not from source:**
- https://crates.io/api/v1/crates/cosmic-text (0.19.0, 2026-04-22, MIT OR Apache-2.0)
- https://crates.io/api/v1/crates/iced (0.14.0, 2025-12-07, MIT)
- https://crates.io/api/v1/crates/iced_selection (0.5.0, 2026-02-05, MIT)
- https://crates.io/api/v1/crates/iced_wry (0.1.0, 2026-03-27, MIT OR Apache-2.0)
- https://crates.io/api/v1/crates/gpui (0.2.2, 2025-10-22, Apache-2.0)
- https://crates.io/api/v1/crates/masonry (0.4.0, 2025-10-29, Apache-2.0)
- https://api.github.com/repos/pop-os/cosmic-edit (GPL-3.0, pushed 2026-10-08)
- https://api.github.com/repos/pop-os/cosmic-text (pushed 2026-09-30)
- https://api.github.com/repos/lapce/lapce, /lapce/floem, /linebender/xilem, /linebender/parley, /zed-industries/zed (licences and activity as in the §5 table)
- https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html (`build_as_child` platform support; "won't work on Wayland")
- https://codemirror.net/examples/decoration/ (replace/widget/block decorations, `atomicRanges`)

**From search results or README summaries only (not verified in source or code):**
- https://github.com/OlaProeis/Ferrite (egui app with a Rendered WYSIWYG mode; features taken from a summary of the README, repo activity date not confirmed)
- https://docs.rs/iced_wry, https://docs.rs/iced_servo, https://docs.rs/iced_webview_v2 (descriptions from search snippets; not evaluated)
- https://git.pml68.dev/iced_selection/plain/examples/markdown/src/main.rs?h=v0.3.0 (markdown example pairing `text_editor` with a preview; from a search snippet)
- Obsidian forum threads on live preview (e.g. https://forum.obsidian.md/t/plugin-api-expose-live-edit-functionality-for-extension/73447) describing CodeMirror 6 decorations and layout shifts when syntax toggles; anecdotal.
- Zed `display_map.rs` layering (InlayMap, FoldMap, TabMap, WrapMap, BlockMap), from a search summary of https://git.secluded.site/zed/blob/v0.216.0/crates/editor/src/display_map.rs. The licence of Zed's `editor` crate was not verified.
- Atomic Editor (CodeMirror 6 live preview component) named in a search result; not inspected.

**Unverified / not checked:** whether iced 0.14 exposes any accessibility tree for custom widgets; how IME preedit is drawn (widget or shell); behaviour of cosmic-text with a zero or tiny font size; iced 0.15 plans; Parley's editor API; Floem's and Xilem's actual text editing capabilities for rich text; effort estimates are judgment, not measurements.

## Auto-closing pairs

Typing `(`, `[`, `{`, `"` or a backtick puts in its partner and leaves the caret between, unless a word follows (or, for quotes, a word is ending before it, so `don't` and the quote that closes a phrase are left alone). Typing the partner right before the one that is there steps over it; with text selected the pair is put round it and it stays selected; Backspace between an empty pair takes both. Making a pair is one step of undo, and pasting makes none. `[[` for a link comes out as `[[]]`. See `Document::auto_close`.
