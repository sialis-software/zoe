# Margins, Documents, and the Derived Index

## 1. Two kinds of user content

| | **Margin note** | **Document** |
|---|---|---|
| Relationship to Scripture | **Anchored** to a verse range, optionally to specific words | **Linked** through `[[John 3:16]]` references in the body |
| Mental model | Writing and highlighting in the margins of a paper Bible | A separate piece of writing: journal, study, prayer list, sermon notes |
| File | `type: margin`, in `margins/` | Any markdown file, anywhere |
| Shown | Layered **on the reader** (underlines, highlights, margin cards) | In the writer pane |

They are not separate worlds. Both feed the same index, so any verse can answer "everything touching this": marks, margin notes, and documents that link to it.

A **highlight or underline is a margin note with no body**: frontmatter only. A margin note with a comment is the same file with markdown text. One model, one file type.

Users will misjudge which kind they meant, so the writer (the note tab) treats the two as one note with one interface, and the passage is a section of it. In both, the **tags come first** at the top, outside and above a **Frontmatter** section that folds (folded to begin with, to one line that says what is in it: the passage of a margin note; when an ordinary note was made, and how many other properties it has). In the section are the passage (or the way to give the note one), when it was made and, for an ordinary note, the rest of the properties (the *type* and every other one, each edited by its kind, and the way to add one): those the app gives no control of their own.

- A **margin note** has a *Reference* section in the place the other has its *Mark* button: the passage and the words marked (shown, not edited here; nothing is said of a mark on whole verses, and the quote is shown only if there is one), how it marks them (a drop-down of the styles, and a color swatch whose panel floats over the note instead of making it taller), and two buttons. **Reselect** waits for the next selection to settle in a Bible reader and makes it the note's passage (its reference, quote, and context, and the module the words were chosen in); the note's text, tags, look, and time stay, and its file is renamed for the new passage, the links to it following. **Unlink** lets it go from the text, and takes **only** what only a mark has from the note: the file moves out of `margins/` to `notes/` under the **same name**, with the **same words**, tags, and time, and without `type: margin`, its reference and quote (`anchor`, `module`, `quote`, `prefix`, `suffix`) and its look (`style`, `color`). Nothing is added to it (no link, no blockquote). It is an ordinary note; putting it on a passage again is **Mark**, as for any note.
- An **ordinary note** has *Not on a passage* and **Mark**, which waits for a selection the same way and makes the note a margin note on it, as a pen would (the first pen's look, else a yellow highlight): the note's words become the comment, its tags and time are kept, and its frontmatter is the mark's, made fresh (other properties it had are not kept: a margin file has only the mark's). Links to the note follow it to the margin file.
- Under the text, a margin note shows its **verse preview**: the passage as the Bible has it, with the marked words in the mark's color. It is a rounded card in the **bottom bar** of the note tab: a bar 48 px high that every note has, with the Vim mode line at its left and the pane's own buttons (split, fill the window) floating over its right end; the bar grows upward to hold the card when the note has a passage. The reference at the card's head is a link.
- **Delete:** the note and its mark are removed (to the trash, in a real vault).

**One selection model.** There is a single way to select text: click-and-drag (with double-click for words and triple-click for verses). A **verse number is only a shortcut** for it: clicking one selects that verse's text, Shift-click grows the selection to another verse, and clicking the only selected verse clears it. Everything that acts on a selection (pens, copy, insert reference) works the same however the selection was made.

**Mode vs. layer.** The margin layer is a visibility toggle on the reader, not a screen. What differs by mode is what a text selection does: in margins mode a selection offers pen tools; otherwise it simply copies. Margins mode uses the full width (reader plus margin column); document mode is the reader/writer split.

## 2. The margin file

```yaml
---
type: margin
anchor: John.3.16        # verse or range, in the reference versification (KJV)
module: KJV              # module the marked words were chosen in
quote: "For God so loved"
prefix: ""               # a few characters of context before and after
suffix: " the world"
style: highlight         # highlight | underline | striped | dashed | wavy; no `style` at all is a plain mark (below)
color: "#F5D90A"
tags: [promise]
created: 2026-10-12T18:03:00
---
Optional markdown comment, with [[links]] like any document.
```

**Plain marks.** A mark can be set *Plain* (in the note's style drop-down, or made by *Add Mark*): a reference with no look, the words or verses a note is about and nothing painted on them but a faint grey line (and its card, if it has a comment). A plain mark's file has **no `style` and no `color`** (they are taken out when it is set plain, and the color choice is not offered); a file with neither is a plain mark, and an older file with `style: none` is read as one and loses them when written again. Set back to any look it takes the first pen's color. A `style` this version does not know (from a newer one) is drawn as a highlight.

**Verse-level marks.** If the selection when a pen is applied is whole verses (from a verse number, a triple-click, or a drag that happens to cover them exactly), the mark is tied to the *reference*, not to words: no `quote`, `prefix`, `suffix`, or `module` keys.

```yaml
---
type: margin
anchor: Ps.23.4
style: highlight
color: "#B48CFF"
tags: [comfort]
---
```
It cannot go stale when a module's wording changes, and it paints the whole verse in **every** module that has that verse. Word-level marks stay with the module they were made in.

**Pens.** `.biblestudy/config.toml` defines pens: (name, style, colour, tag) — e.g. "Promise: yellow highlight, tag `promise`". A mark stores the *resolved* colour and tags, so changing or losing the pen list never alters or breaks existing marks.

## 3. Anchoring

Goal: marks survive display-option changes, translation switches, and module updates, and never disappear silently.

**Canonical verse text.** For each verse, the concatenation of the text of its spans that are real scripture text (not verse numbers, footnote markers, or headings), whitespace-normalised. It does not change when Strong's words, footnotes, or red-letter styling are toggled, so `quote`/`prefix`/`suffix` stay valid. The reader already computes exactly this for copying.

**Resolution** — deterministic from the markdown plus the module:
1. Look for `prefix + quote + suffix` in the canonical text of the anchored verses. Found exactly once → **exact**; draw the underline or highlight there.
2. Found only with fuzzy matching, or the module text changed slightly → **moved**; draw it, flag it for review.
3. Not found → **orphaned**; kept, listed in a review list; never deleted.
4. A different module than `module` is displayed (word-level marks only) → no word-level underline (word ranges do not map between translations); the verse gets a gutter tick meaning "something is attached here", and its notes remain reachable. (Gutter ticks are not built yet; today such marks are simply not drawn.)
5. A **verse-level mark** (no quote) has nothing to match: it resolves to the whole text of its verses in whatever module is shown, and is orphaned only if the module lacks them.

Anything the user decides (accepting a re-anchor, re-attaching an orphan) is written back to the markdown. Otherwise rebuilding the index would undo it.

Versification: `anchor` uses the reference versification; mapping to the displayed module's numbering is done at display time (01 §4).

## 4. Rendering on the reader

- **Highlights/underlines:** drawn by the reader widget as quads under or over the glyphs, from the same per-line geometry already used for selection and span backgrounds. Several marks may overlap; draw order is creation order, with blending.
- **Pen tools:** a small toolbar floats under a settled text selection, however it was made. Clicking a marked stretch (without dragging) opens that mark's note in the writer; when marks overlap there, a chooser appears under the click.
- **Margin cards (marks with a comment):** the reader widget hosts them, so they scroll with the text and always know where each verse is. What the reader does depends only on its width:
  - *Wide enough* (text column 640px + gap + at least 220px): a real **margin** is always visible to the right of the text, a faintly different panel with an edge, even when empty. Every card with a comment stands in it, level with the first line of its mark; cards that would collide stack in anchor order with a small gap, and the card being edited holds its place while the others make way. A thin connector with a colour tick joins each marked line to its card, bending in the middle of the gap. A connector whose vertical stretch would overlap another's steps aside to a lane just left or right of the middle, so lone ones look as they always did; the hovered one is drawn solid and thicker. The margin takes about 2/7 of the reader's width (between 220 and 560px, and never so wide that the text falls below its 640px column), and its panel runs alongside the text, from its top to its bottom. The margin has its own menu (a button at its top; its choices open over the cards): the same four kinds as the reader's menu (Strong's numbers, Marks, Footnotes, References), but each says whether that kind's cards sit in the margin or pop over the text where clicked. The reader's menu says whether the kind shows at all. Every cross reference marker (when References is on) has its list of passages in the margin, like a footnote, and each passage is a link with the writer's right click and pull-away. Likewise every word with a Strong's number has its dictionary entry there (looked up when the chapter loads), brought up by pointing at the word, with an "Open in tab" button. Their links in the text (Strong's words, †, ↗) are quiet when the kind is popovers instead of margin cards (plain text, or grey for the marks), and take the link color when pointed at. Words with a Strong's number are always dotted underneath. These two kinds are off by default; unchecked, a click on the word or marker pops the entry or list over the text. In a new stack a note of the user's starts on top of a footnote (not saved). Notes whose cards would run into each other (marks within a card's height of one another, so a few lines) are layered into a **stack**: the top note shows whole, the others peek out behind it, and a bar under it (‹ 2 / 3 ›) cycles through them. Pointing at a mark (or a footnote marker) in the text brings its note to the top; the note being edited is always on top. A stack has one connector, and one footnote sits on the line its marker is on (not the one above, when the marker starts a line). Cards that collide are spread about their marks (some up, some down; the least total distance), not all pushed down. Pointing at a mark, or its footnote marker, lights the edge of its card (or its stack) and dims every other connector and note. Clicking marked text edits its card.
  - *Narrower*: **no margin and no cards**. Clicking a mark floats its card (or the cards, when marks overlap) under the click, like a toolbar; clicking elsewhere or Esc dismisses it. A bare highlight shows a card offering "Add a note…". Popup cards always offer Open in writer, Unlink, and Delete. With no margin the text takes the whole width, so no empty space is left at the right. The reader tells the app which of the two it chose, so the app decides what a click means. Hovering a card or its marked text emphasises both; an underline or note gets a calmer (less saturated, translucent) highlight behind it for the occasion, as a highlight already has. Long cards are shortened until expanded.
- **A resting card** shows its comment rendered by the same engine as the writer (hidden markers, links as their label, checkboxes, embeds), read-only: a plain click on a link follows it, a click on a checkbox ticks it, and a click anywhere else starts editing.
- **Editing in place:** clicking a card puts its text in the live editor (raw markdown for the element at the caret, a blinking caret) and writes every keystroke through to the mark and its file. A **pen** marks the selection in its look and opens the card with the caret in the words. **Add Mark** on the pen toolbar adds a plain mark (`style: none`, drawn as a faint line) to the selection and opens a **card of its own** for it, which stands in for the ordinary card while the mark is first written. It has the caret in the **tags** first (Enter adds a tag and stays for the next, a tag already chosen can be taken off with its ×, and the ones in use are offered as you type), then a row for how the mark marks the text: a **color** first (a ring with nothing in it while the mark is plain; choosing one makes it a **highlight** in that color, and a mark that is already something else keeps its look) and a drop-down for the **look** (highlight, underline, dashed, wavy, striped, plain; plain takes the color away). Both are real floating menus, as in the note tab: a card is drawn by the reader widget, which passes on the overlays of the cards and the toolbar it hosts. Under them are the buttons: **Add note**, which gives the card a place for words (with the caret in it), Open, Unlink, and Delete. Tab from the tags does what Add note does (and takes a tag half typed). There are no words by default, so there is no place for them until one is asked for. A mark that is given nothing (plain, no tags, no words) is removed when you stop; one with tags and no words stays, as does any with a look. Once the card is put away the mark has the ordinary card again, whose small print says its tags. **Pointing at a mark** shows its tags in a label over the left end of the mark's first line (each tag in its pen's color where it has one, as on the card's chips) (under it where there is no room above), which stays there as the pointer moves along the mark (the tags of each mark that overlaps, once each); clicking it opens its card as before.
- **Hiding the writer** (the *Writer* toggle in the top bar) gives the reader the whole width, which is what makes room for the margin on an ordinary window.

## 5. Vault organisation

See 02 §2. In short: `margins/<Book>/<NN>/…` for marks; everything else is free-form documents; `type` decides behaviour, folders do not.

## 6. The derived index (SQLite)

The index connects the three sources — markdown files, SWORD modules, and the reference versification — and is **always regenerable**.

**Invariant:** the database is a pure function of *(vault files, installed modules and their versions, versification tables)*. A test enforces it: build, delete, rebuild, compare. Anything user-authored must therefore have a markdown home; anything that would exist only in SQLite is a bug.

**Two databases**
- **Vault index** — links, anchors, note metadata, full-text of notes. Small; quick to rebuild.
- **Module index** — verse text full-text and Strong's/lemma lookups, one per module and version, in the OS cache dir. Modules are shared between vaults through `~/.sword`, and indexing a whole Bible takes much longer than indexing notes, so it must not be redone on every vault rebuild.

**Location.** Both live in the OS cache directory, keyed by vault id / module+version, *not* inside the vault: a live SQLite file (especially in WAL mode) can be corrupted by sync tools such as Syncthing, iCloud, and Dropbox. A new machine rebuilds on first open.

**Join key.** Each verse has an integer ordinal in the reference versification. Anchors and verse links are integer ranges, so "all notes touching John 3:16" is an interval overlap query; versification mapping happens once, at index time.

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
- Incremental updates keyed on file mtime + content hash; full rebuild on schema-version mismatch.
- SQLite **FTS5** covers notes, verse text, and lemma/Strong's filters, replacing the earlier plan to use tantivy. Revisit only if ranking or speed disappoints.
- Resolved anchor positions may be cached (resolution is deterministic).

## 7. Prototype status

The app now runs on a real vault and real modules (crates `vault`, `sword`).

- **Vault**: any folder of markdown files, chosen by the user. The choice is kept in `config.json` in the config folder (see Names below); with none saved, an earlier version's `vault` folder in the data folder is used. Every edit is written through to its file (atomic write-then-rename) after each update; deleting or renaming a mark removes the old file. Marks load from `margins/` on start.
- **Choosing a vault** (`vault_setup.rs`). On a first run, or when the saved folder is not there (an unplugged drive), a welcome page covers the window: *Create a new vault* (pick a folder, name the new folder inside it), *Open an existing vault* (pick a folder; an Obsidian vault is fine and its `.obsidian` is left alone), or use the default location. The same two actions are in Settings → Vault, with *Show in folder*. A card confirms the folder after it is looked at (note count, Obsidian, or ours) and offers to add sample notes (only to a folder with no notes) and the starting templates (only where there are none). Switching saves what is open, loads the new notes, remembers the choice, and opens the default layout; the old vault is untouched.
- **Index per vault**: `cache/index-<hash of the vault path>.sqlite` in the data folder (the default vault keeps `index.sqlite`), so nothing but notes is ever written into a vault.
- **Seed data**: an empty vault is seeded with four linked notes and nine marks anchored in ASV's real text (a quote a module words differently is skipped). `zoe --reseed` restores the samples without touching other files.
- **Index**: SQLite in the data folder's `cache/` (outside the vault, per §6): `files`, `links` (verse/embed/note/sword links plus each mark's anchor, as book/chapter/verse ranges), and an FTS5 table. It syncs incrementally by content hash on start and per save, and a test deletes and rebuilds it. The UI does not read it yet: `annotated_verses` and "notes on this selection" still scan the in-memory notes. Verse ranges use (book, chapter, v1, v2) rather than the integer ordinals sketched above; switch when versification mapping arrives.

## 8. Open questions

- Highlights across translations: word-level marks stay with their module and verse-level marks show everywhere. Is best-effort word re-matching in other modules worth it later?
- Overlapping marks of different pens on the same words: layering rules, and whether the UI offers "show only pen X".
- Margin column vs. inline threshold (pane width), and per-user preference.
- Does a long margin note belong in the margin at all, or should long text promote to a document automatically?
- Printing/exporting a chapter with its margins (PDF/markdown) — nice to have.

## 9. Series (planned)

A **series** is a collection of related files of any kind: documents, margin notes, highlights. It is for organisation that folders and tags do not give: a study of Romans might include a document per week, forty highlights spread across the epistle, and a few free-standing notes, all in different places on disk.

Not built yet. The prototype's marks have no `series` key. This section records the intended design so nothing built now rules it out.

**Representation (recommended)**
- A series is a **document** (`type: series`) with a title and optional description body. It can live anywhere; `series/` is the suggested default.
- **Members declare membership** with a frontmatter key, on any file including a bare highlight:
  ```yaml
  series: ["[[Romans Study]]", "[[Advent 2026]]"]
  ```
  Wikilinks, not plain names, so the existing rename handling (02 §7) rewrites them and no second naming mechanism is needed.
- The **index derives membership** as a new link kind (`series`) in the `links` table (§6). "Everything in this series" is a backlink query. Nothing about series needs to be stored outside markdown, so the invariant in §6 still holds.
- **Ordering** is optional and lives in the series document (an ordered list of links in its body). Without it, members sort by `created`.

**Why not the alternatives**
- *Tags:* a tag is a flat topic label with no identity of its own. A series has a description, can be ordered, and is something you open.
- *Folders:* a file lives in one folder, and marks live in `margins/`. Membership has to cut across both.
- *A manifest file listing members:* ordering is natural there, but members would not describe themselves, a heavily highlighted series would put hundreds of entries in one file that merges badly when synced, and renaming a member would need the manifest edited.

**Ideas this enables**
- An **active series**: while it is set, newly created marks, margin notes, and documents join it automatically.
- A series view: its members by date, or by the passage each covers (derived from anchors and links).
- Marks in the chooser (reader) can show which series they belong to.
- A series exported as a single document or as a SWORD-style book (see 02 §6). An ordered series is close to a user "book", so those may merge later.

**Open questions**
- Can series nest (a series within a series)? Sub-series are probably better modelled by ordering plus sections in the series document.
- Does a series carry a Scripture scope, or is that always derived from its members?
- Should `books/` and `studies/` be absorbed into series, leaving one concept?
- Does deleting a series document remove membership, or leave dangling links that the app offers to repair?

### Config and data

What the person chose and what the app makes are kept apart, in the places the XDG convention names (on macOS and Linux alike; `%APPDATA%\zoe` and `%LOCALAPPDATA%\zoe` on Windows):

- **Config**, `$XDG_CONFIG_HOME/zoe` (`~/.config/zoe`): `config.json` (the vault and the module folders chosen), `settings.json` (look and feel, text sizes, font), and `layouts.json` (saved layouts).
- **Data**, `$XDG_DATA_HOME/zoe` (`~/.local/share/zoe`): `session.json` (what was open at shutdown), `cache/` (the search indexes, which can be deleted), and `vault/` (the vault made when none was chosen).
- **Modules** are not the app's: `$XDG_CONFIG_HOME/sword` (`~/.config/sword`), shared with other SWORD programs.

`ZOE_CONFIG_HOME` and `ZOE_DATA_HOME` change the first two, and `ZOE_HOME` puts both under one folder (`config/` and `data/` in it). On the first start after this change, the one folder that held everything (`~/Library/Application Support/zoe`, or `bible-study` before the rename, or `~/.local/share/bible-study`) is split up: its files go to where they belong and the vault and module folders chosen in `config.json` are followed to their new places.

### Names

The app is called Zoe. The hidden folder in a vault is `.zoe` (earlier `.bible-study`, moved when the vault is opened), and the environment variables are `ZOE_HOME`, `ZOE_CONFIG_HOME`, `ZOE_DATA_HOME`, `ZOE_SWORD_PATH`, and `ZOE_MODULE` (the old `BIBLE_STUDY_*` names still work). On a Mac, `scripts/bundle-macos.sh` makes `target/Zoe.app` so the Dock and menu bar use the name and the icon (the artwork in `crates/app/assets/app/zoe.png` on a rounded square, made by `scripts/make-icon.swift`, whose 1024 px result is also kept as `zoe-rounded.png`); run outside a bundle the app sets its own Dock icon, and on other systems the window's icon, from that rounded picture.

### Packaging

`scripts/package.sh [macos|windows|linux|all]` makes the packages with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) (config: `Packager.toml`; `cargo install cargo-packager cargo-xwin --locked`), from a Mac. They land in `target/dist`, unsigned:

- macOS: `Zoe.app` and a `.dmg`, for this Mac's architecture.
- Windows: an NSIS installer for x64, cross-built with `cargo xwin` (needs brew's `llvm@21`, `lld@21`, and `makensis`). The icon and name are written into the .exe by `crates/app/build.rs`, and a release build has no console window.
- Linux: a `.deb` and an AppImage, built in a `rust:1-bookworm` Docker container (`LINUX_PLATFORM=linux/amd64` for x86_64, slowly, by emulation).

The Berean Standard Bible and both Strong's dictionaries are compiled into the program (`crates/app/assets/modules`, `src/bundled.rs`) and installed into the app's own module folder once, on the first run, when no Bible is found anywhere; about 9 MB of the program's size.

The icon files in `packaging/icons` (PNGs and `zoe.ico`) are made from the rounded artwork by `scripts/make-icons.sh` and kept in git. `scripts/bundle-macos.sh` is the older, hand-made macOS bundle, kept for what cargo-packager does not do: a universal binary (`UNIVERSAL=1`). Only the macOS, Windows installer, and Linux packages' building has been tried; none has been run on Windows or Linux.

The program file is `Zoe` (a `[[bin]]` in `crates/app/Cargo.toml`; `cargo run -p zoe` still runs it), so that the Dock shows the capital outside a bundle. The mark in the header bar is `crates/app/assets/app/zoe.svg`, the artwork traced to a path (the original, which draws the letter as text in a font the viewer must have, is kept as `zoe-source.svg`), drawn in the color of the text.

### Persecution mode, and About

**Persecution mode** (`network.rs`) turns off everything that could use the network: the module manager's list and downloads (`module_repo::Http::get`, the app's only network client, refuses) and the opening of web and mail links (`native::open_external`, the only way an address is handed to another program). Both ask `network::blocked()`, so no new code path can forget to. The choice is `persecution` in `config.json`, read first thing at start-up, and `ZOE_PERSECUTION=1` forces it on. It is a switch in the first-run window and in the settings under Privacy; the module manager says why its download buttons are off, and modules can still be added from files. The app never checks for updates or sends anything about the person, with or without the mode.

**About** is the last page of the settings (all systems: `about.rs`) with the credits for what comes with the app and the list of libraries it is built from and their licences (`assets/third-party.txt`, made by `scripts/third-party.sh`; re-run it when dependencies change). On a Mac the application menu's About item shows the system's panel with the same credits, the version, and the picture. The libraries' licence *texts* are not included yet, only the names and licences.
