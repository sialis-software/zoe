# Margins, Documents, and the Derived Index

## 1. Two kinds of user content

| | **Margin note** | **Document** |
|---|---|---|
| Relationship to Scripture | **Anchored** to a verse range, optionally to specific words | **Linked** through `[[John 3:16]]` references in the body |
| Mental model | Writing and highlighting in the margins of a paper Bible | A separate piece of writing: journal, study, prayer list, sermon notes |
| File | `type: margin`, in `margins/` | Any markdown file, anywhere |
| Shown | Layered **on the reader** (underlines, highlights, margin cards) | In the writer pane |

They are not separate worlds. Both feed the same index, so any verse can answer
"everything touching this": marks, margin notes, and documents that link to it.

A **highlight or underline is a margin note with no body**: frontmatter only. A margin
note with a comment is the same file with markdown text. One model, one file type.

Users will misjudge which kind they meant, so a margin note offers two actions, both
shown in the writer while its file is open:
- **Unlink from text:** the highlight goes, the note stays. The file moves out of `margins/`
  to `notes/` as an ordinary document that keeps the quote (as a blockquote), the comment,
  and a `[[reference]]` link back to the verses.
- **Delete:** the note and its mark are removed (to the trash, in a real vault).
The reverse, *attach to passage*, is planned.

**One selection model.** There is a single way to select text: click-and-drag (with
double-click for words and triple-click for verses). A **verse number is only a shortcut**
for it: clicking one selects that verse's text, Shift-click grows the selection to another
verse, and clicking the only selected verse clears it. Everything that acts on a selection
(pens, copy, insert reference) works the same however the selection was made.

**Mode vs. layer.** The margin layer is a visibility toggle on the reader, not a screen.
What differs by mode is what a text selection does: in margins mode a selection offers pen
tools; otherwise it simply copies. Margins mode uses the full width (reader plus margin
column); document mode is the reader/writer split.

## 2. The margin file

```yaml
---
type: margin
anchor: John.3.16        # verse or range, in the reference versification (KJV)
module: KJV              # module the marked words were chosen in
quote: "For God so loved"
prefix: ""               # a few characters of context before and after
suffix: " the world"
style: highlight         # highlight | underline | none (a plain margin note)
color: "#F5D90A"
tags: [promise]
created: 2026-10-12T18:03:00
---
Optional markdown comment, with [[links]] like any document.
```

**Verse-level marks.** If the selection when a pen is applied is whole verses (from a verse
number, a triple-click, or a drag that happens to cover them exactly), the mark is tied to
the *reference*, not to words: no `quote`, `prefix`, `suffix` or `module` keys.

```yaml
---
type: margin
anchor: Ps.23.4
style: highlight
color: "#B48CFF"
tags: [comfort]
---
```
It cannot go stale when a module's wording changes, and it paints the whole verse in
**every** module that has that verse. Word-level marks stay with the module they were made in.

**Pens.** `.biblestudy/config.toml` defines pens: (name, style, colour, tag) — e.g.
"Promise: yellow highlight, tag `promise`". A mark stores the *resolved* colour and tags,
so changing or losing the pen list never alters or breaks existing marks.

## 3. Anchoring

Goal: marks survive display-option changes, translation switches, and module updates, and
never disappear silently.

**Canonical verse text.** For each verse, the concatenation of the text of its spans
that are real scripture text (not verse numbers, footnote markers, or headings),
whitespace-normalised. It does not change when Strong's words, footnotes, or red-letter
styling are toggled, so `quote`/`prefix`/`suffix` stay valid. The reader already
computes exactly this for copying.

**Resolution** — deterministic from the markdown plus the module:
1. Look for `prefix + quote + suffix` in the canonical text of the anchored verses.
   Found exactly once → **exact**; draw the underline or highlight there.
2. Found only with fuzzy matching, or the module text changed slightly → **moved**;
   draw it, flag it for review.
3. Not found → **orphaned**; kept, listed in a review list; never deleted.
4. A different module than `module` is displayed (word-level marks only) → no word-level
   underline (word ranges do not map between translations); the verse gets a gutter tick
   meaning "something is attached here", and its notes remain reachable. (Gutter ticks are
   not built yet; today such marks are simply not drawn.)
5. A **verse-level mark** (no quote) has nothing to match: it resolves to the whole text of
   its verses in whatever module is shown, and is orphaned only if the module lacks them.

Anything the user decides (accepting a re-anchor, re-attaching an orphan) is written
back to the markdown. Otherwise rebuilding the index would undo it.

Versification: `anchor` uses the reference versification; mapping to the displayed
module's numbering is done at display time (01 §4).

## 4. Rendering on the reader

- **Highlights/underlines:** drawn by the reader widget as quads under or over the glyphs,
  from the same per-line geometry already used for selection and span backgrounds.
  Several marks may overlap; draw order is creation order, with blending.
- **Pen tools:** a small toolbar floats under a settled text selection, however it was
  made. Clicking a marked stretch (without dragging) opens that mark's note in the writer;
  when marks overlap there, a chooser appears under the click.
- **Margin cards (marks with a comment):** the reader widget hosts them, so they scroll with
  the text and always know where each verse is. What the reader does depends only on its width:
  - *Wide enough* (text column 640px + gap + at least 220px): a real **margin** is always
    visible to the right of the text, a faintly different panel with an edge, even when empty.
    Every card with a comment stands in it, level with the first line of its mark; cards that
    would collide stack in anchor order with a small gap, and the card being edited holds its
    place while the others make way. A thin connector with a colour tick joins each marked line
    to its card, bending in the middle of the gap. A connector whose vertical stretch would
    overlap another's steps aside to a lane just left or right of the middle, so lone ones look
    as they always did; the hovered one is drawn solid and thicker. Spare width beyond a
    comfortable card goes half to the text column (up to about 120px more) and half to the cards
    (up to 560px), so a wide window is not left with a blank strip. Clicking marked text edits
    its card.
  - *Narrower*: **no margin and no cards**. Clicking a mark floats its card (or the cards, when
    marks overlap) under the click, like a toolbar; clicking elsewhere or Esc dismisses it. A
    bare highlight shows a card offering "Add a note…". Popup cards always offer Open in writer,
    Unlink and Delete.
  With no margin the text takes the whole width, so no empty space is left at the right.
  The reader tells the app which of the two it chose, so the app decides what a click means.
  Hovering a card or its marked text emphasises both; an underline or note gets a calmer
  (less saturated, translucent) highlight behind it for the occasion, as a highlight already has. Long cards are shortened until expanded.
- **A resting card** shows its comment rendered by the same engine as the writer (hidden markers,
  links as their label, checkboxes, embeds), read-only: a plain click on a link follows it, a click
  on a checkbox ticks it, and a click anywhere else starts editing.
- **Editing in place:** clicking a card puts its text in the live editor (raw markdown for the element
  at the caret, a blinking caret) and writes every keystroke through to the mark and its file. **Comment** on the
  pen toolbar adds a note-only mark (`style: none`, drawn as a faint line) to the selection and
  opens its card to type in; if you stop without writing anything it is removed.
- **Hiding the writer** (the *Writer* toggle in the top bar) gives the reader the whole width,
  which is what makes room for the margin on an ordinary window.

## 5. Vault organisation

See 02 §2. In short: `margins/<Book>/<NN>/…` for marks; everything else is free-form
documents; `type` decides behaviour, folders do not.

## 6. The derived index (SQLite)

The index connects the three sources — markdown files, SWORD modules, and the reference
versification — and is **always regenerable**.

**Invariant:** the database is a pure function of *(vault files, installed modules and
their versions, versification tables)*. A test enforces it: build, delete, rebuild,
compare. Anything user-authored must therefore have a markdown home; anything that would
exist only in SQLite is a bug.

**Two databases**
- **Vault index** — links, anchors, note metadata, full-text of notes. Small; quick to
  rebuild.
- **Module index** — verse text full-text and Strong's/lemma lookups, one per module and
  version, in the OS cache dir. Modules are shared between vaults through `~/.sword`, and
  indexing a whole Bible takes much longer than indexing notes, so it must not be redone
  on every vault rebuild.

**Location.** Both live in the OS cache directory, keyed by vault id / module+version,
*not* inside the vault: a live SQLite file (especially in WAL mode) can be corrupted by
sync tools such as Syncthing, iCloud, and Dropbox. A new machine rebuilds on first open.

**Join key.** Each verse has an integer ordinal in the reference versification. Anchors
and verse links are integer ranges, so "all notes touching John 3:16" is an interval
overlap query; versification mapping happens once, at index time.

Sketch (vault index):

```
files(id, path UNIQUE, type, title, mtime, hash)
links(file_id, kind, target, start_ord, end_ord, line)        -- kind: verse|note|sword
anchors(file_id, start_ord, end_ord, module, module_version,
        quote, prefix, suffix, style, color, state,
        resolved_start, resolved_end)                          -- state: exact|moved|orphaned
tags(file_id, tag)
notes_fts USING fts5(title, body)
```
Module index: `verse_text(ord, text)` + FTS5, `word_lemma(ord, strongs, morph)`.

**Mechanics**
- `rusqlite` with the bundled SQLite feature: identical on macOS, Windows, and Linux.
- One writer thread (the indexer); the UI reads through the `core` traits.
- Incremental updates keyed on file mtime + content hash; full rebuild on schema-version
  mismatch.
- SQLite **FTS5** covers notes, verse text, and lemma/Strong's filters, replacing
  the earlier plan to use tantivy. Revisit only if ranking or speed disappoints.
- Resolved anchor positions may be cached (resolution is deterministic).

## 7. Prototype status

The app now runs on a real vault and real modules (crates `vault`, `sword`).

- **Vault**: markdown files under `$BIBLE_STUDY_HOME/vault` (default
  `~/Library/Application Support/bible-study/vault` on macOS). Every edit is written through
  to its file (atomic write-then-rename) after each update; deleting or renaming a mark
  removes the old file. Marks load from `margins/` on start.
- **Seed data**: an empty vault is seeded with four linked notes and nine marks anchored in
  ASV's real text (a quote a module words differently is skipped). `bible-study --reseed`
  restores the samples without touching other files.
- **Index**: SQLite at `<home>/cache/index.sqlite` (outside the vault, per §6): `files`,
  `links` (verse/embed/note/sword links plus each mark's anchor, as book/chapter/verse
  ranges), and an FTS5 table. It syncs incrementally by content hash on start and per save,
  and a test deletes and rebuilds it. The UI does not read it yet: `annotated_verses` and
  "notes on this selection" still scan the in-memory notes. Verse ranges use
  (book, chapter, v1, v2) rather than the integer ordinals sketched above; switch when
  versification mapping arrives.

## 8. Open questions

- Highlights across translations: word-level marks stay with their module and verse-level
  marks show everywhere. Is best-effort word re-matching in other modules worth it later?
- Overlapping marks of different pens on the same words: layering rules, and whether the
  UI offers "show only pen X".
- Margin column vs. inline threshold (pane width), and per-user preference.
- Does a long margin note belong in the margin at all, or should long text promote to a
  document automatically?
- Printing/exporting a chapter with its margins (PDF/markdown) — nice to have.

## 9. Series (planned)

A **series** is a collection of related files of any kind: documents, margin notes,
highlights. It is for organisation that folders and tags do not give: a study of Romans
might include a document per week, forty highlights spread across the epistle, and a few
free-standing notes, all in different places on disk.

Not built yet. The prototype's marks have no `series` key. This section records the intended
design so nothing built now rules it out.

**Representation (recommended)**
- A series is a **document** (`type: series`) with a title and optional description body.
  It can live anywhere; `series/` is the suggested default.
- **Members declare membership** with a frontmatter key, on any file including a bare
  highlight:
  ```yaml
  series: ["[[Romans Study]]", "[[Advent 2026]]"]
  ```
  Wikilinks, not plain names, so the existing rename handling (02 §7) rewrites them and no
  second naming mechanism is needed.
- The **index derives membership** as a new link kind (`series`) in the `links` table
  (§6). "Everything in this series" is a backlink query. Nothing about series needs to be
  stored outside markdown, so the invariant in §6 still holds.
- **Ordering** is optional and lives in the series document (an ordered list of links in
  its body). Without it, members sort by `created`.

**Why not the alternatives**
- *Tags:* a tag is a flat topic label with no identity of its own. A series has a description,
  can be ordered, and is something you open.
- *Folders:* a file lives in one folder, and marks live in `margins/`. Membership has to
  cut across both.
- *A manifest file listing members:* ordering is natural there, but members would not
  describe themselves, a heavily highlighted series would put hundreds of entries in one file
  that merges badly when synced, and renaming a member would need the manifest edited.

**Ideas this enables**
- An **active series**: while it is set, newly created marks, margin notes and documents
  join it automatically.
- A series view: its members by date, or by the passage each covers (derived from anchors
  and links).
- Marks in the chooser (reader) can show which series they belong to.
- A series exported as a single document or as a SWORD-style book (see 02 §6). An
  ordered series is close to a user "book", so those may merge later.

**Open questions**
- Can series nest (a series within a series)? Sub-series are probably better modelled by
  ordering plus sections in the series document.
- Does a series carry a Scripture scope, or is that always derived from its members?
- Should `books/` and `studies/` be absorbed into series, leaving one concept?
- Does deleting a series document remove membership, or leave dangling links that the app
  offers to repair?
