# Vault Layout and Markdown Dialect

The **vault** is a normal folder the user chooses (default `~/Documents/BibleStudy`).
Everything user-created lives in it as UTF-8 markdown (`.md`) with optional YAML
frontmatter. Design goal: open it in Obsidian/VS Code/any editor and it still makes sense.

The two kinds of user content — **margin notes** attached to Scripture text, and free
**documents** — are described in [05-margins-and-index.md](05-margins-and-index.md).
This document covers the layout, metadata, and link syntax they share.

## 1. Rules

1. **App-managed vs user-managed.** `margins/` is created and organised by the app (one
   file per mark). Everything else is the user's; unknown folders are indexed as documents.
2. **Files are self-describing.** A mark carries its own colour and tags; it does not
   depend on a settings file to be understood.
3. **No special machinery for ordinary content.** A prayer list is a document. A journal
   is documents in a folder. Structure is the user's.
4. **Nothing precious outside markdown.** Indexes live elsewhere and are rebuildable
   (see 05 §6). Per-device state (window layout, module paths, unlock keys) lives in the OS
   config dir, never in the vault, so the vault stays portable.

## 2. Layout

```
Vault/
  README.md                       explains the layout (auto-created, user may delete)
  .biblestudy/
    config.toml                   vault settings: default module, pen definitions, plan defaults
    trash/                        deleted marks/notes, recoverable
  margins/                        app-managed: one file per mark or margin note
    John/03/16-highlight-k3f9.md
    John/03/16-17-note-p2xq.md
    1Cor/13/04-7-underline-9d1e.md
  journal/2026/10/2026-10-12.md   suggested default, nothing depends on it
  plans/                          reading plans; progress is checkboxes in the plan file
  studies/  commentaries/  books/ user-authored multi-part work (see §6)
  attachments/                    images, PDFs
  (anything else)                 free-form documents
```

Margin file naming: `margins/<OsisBook>/<chapter, 2+ digits>/<verse or range>-<style>-<4 random chars>.md`.
- OSIS book ids (`1Cor`, `Song`) are language-independent and unambiguous.
- The random suffix means two devices can mark the same verse without a sync collision.
- Sharding by book and chapter keeps folders to a few dozen files even for a heavy user.

## 3. Frontmatter

All keys optional unless noted. Keys are flat (not nested) so other tools can show and
query them.

| Key | Used by | Meaning |
|---|---|---|
| `type` | all | `margin`, `plan`, `book`, `commentary`; absent = ordinary document |
| `title` | documents | not written by the app: a note's title is its file name and its first heading. A `title:` already in a note is kept as it is (and followed when the note is renamed) |
| `tags` | all | list of topics (see §4) |
| `created` | all | ISO-8601 timestamp |
| `aliases` | documents | alternative names for link resolution |
| `series` | all (marks, notes, documents) | the series this file belongs to; a list of wikilinks (see §4, 05 §9) |
| `anchor`, `style`, `color` | `margin` | see 05 §2 |
| `module`, `quote`, `prefix`, `suffix` | `margin` (word-level only) | omitted on verse-level marks, which are tied to the reference and show in every module |
| `refs` | documents | optional loose Scripture references in addition to body links |

### As built (the prototype)

The app knows four frontmatter fields and writes them in this order; anything else a person
put there is kept as written, after them (`core/src/frontmatter.rs`):

```yaml
---
title: Notes on Nicodemus
created: 2026-10-12T23:41:05-07:00   # RFC 3339 with the offset it was made in
tags: [john, new-birth]
type: prayer
---
```

- **`created`** is stamped on every note the app makes (a new note, an unlinked margin note,
  seeded samples) and on every margin mark's file (`created:` under `type: margin`). Existing
  files are never rewritten behind anyone's back: a note without one shows "No created time"
  with a button to stamp it.
- **The raw block is not edited as text.** In a note tab the frontmatter is hidden and locked:
  the caret and selection cannot enter it, Backspace stops at its edge, and Select All takes the
  text only (`Document::lock_frontmatter`). It changes only through controls, as one undoable
  edit (`replace_frontmatter`), rewritten in the app's order. Margin notes' lines are the
  mark's own: shown as a one-line summary, never edited by hand.
- **What the app knows gets guided controls.** A strip above the text shows when the note was
  made and its tags (chips that remove on click, a box to add one). "Properties" opens the
  editor: a title box, a Type picker (none, prayer, sermon, study, journal, or whatever a file
  already says), and the created time.
- **Every other property gets a generic editor by its type.** A value's type is what it looks
  like: Yes / No (a checkbox), Number, Date (`2026-10-12`, with or without a time), List
  (comma separated), or Text. A value that cannot be stored as its type yet (a half typed
  number, a date not yet complete) stays in the box and is only stored once it is valid. New
  properties are added by name and type; names the app uses (`title`, `created`, `tags`,
  `type`) and names already there are refused. Text that merely looks like another type
  (`"42"`) is quoted so it stays text. A block the editor does not model (a nested map, a
  multi-line value, a comment) is listed as "Not editable here" and written back exactly as it
  was, so nothing a person put in a file is lost.
- **New note** (in the note selector) makes `notes/Untitled.md` (then `Untitled 2.md`…) with
  its title and created time already in place.

## 4. Type, tags, status

Three separate axes, deliberately not mixed:

- **`type`** — structural, known to the app. Small closed set.
- **Tags** — the user's own topics, free-form. Both frontmatter `tags: [grace]` and inline
  `#grace` are read. Hierarchy with slashes: `#topic/grace`. The app writes a tag only on
  behalf of a pen (a pen can carry a tag, e.g. yellow = `promise`).
- **Series** — a named, curated collection that cuts across file types and folders: a
  study's documents, its margin notes and highlights, related notes. Unlike a tag it has
  its own document and identity, and may be ordered. Membership is declared by the member:
  `series: ["[[Romans Study]]"]` (quoted, as YAML needs). A file can be in several series.
  Planned, not yet built; see 05 §9.
- **Task checkboxes** — `- [ ]` / `- [x]` in any document. The app may offer cross-vault
  views of open items later, but ordinary documents stay ordinary markdown.

## 5. Link syntax

Constraint: readable raw, compatible with common markdown tools, resolvable without
the app where possible.

| Purpose | Syntax | Notes |
|---|---|---|
| Verse / range / chapter | `[[@John 3:16]]`, `[[@Jn 3:16-18]]`, `[[@Rom 8]]` | The `@` marks a passage. Parsed by a lenient reference parser; stored as written, indexed as OSIS (`John.3.16`). Display text via `[[@John 3:16\|this verse]]`. One range per link. |
| Another document | `[[Nicodemus]]` / `[[notes/Nicodemus]]` | Wikilink, resolved by title/alias/path; Obsidian-compatible. With no marker a link is always a note, whatever it is spelt like (`[[John 3]]` is the note called "John 3"). |
| Strong's entry | `[[%G5547]]`, `[[%H215\|light]]` | The `%` marks a dictionary entry: a letter and up to four digits, opened in whichever installed dictionary has it. |
| SWORD non-Bible entry | `[Module/key](sword:Module/key)` e.g. `[Webster/grace](sword:Webster/grace)` | An ordinary markdown link with a `sword:` address: dictionary key or GenBook tree path. Not a wikilink. |
| Verse in a specific module | `[[@John 3:16@KJV]]` | Pins translation; otherwise reader's default. |
| Quote verse text (transclusion) | `![[@John 3:16]]` | Rendered live from the module — **text is never copied into the file**, avoiding licensing and staleness. |
| Plain markdown link | `[x](https://…)`, `[x](../notes/a.md)` | Standard; `bible:John.3.16` accepted as a URI scheme in portable exports. |

A note's name may not begin with `%` or `@`: they begin the links above, so no note can be mistaken for a passage
or an entry, and a note may be called anything else, however much it looks like a reference. The editor offers
choices as a link is written: after `[[` the notes, after `[[%` the Strong's entries by number, and after `[[@`
the passage, step by step (the book, then the chapter, then the verse, then a Bible after a second `@`); the
arrow keys choose, Enter or Tab takes, Escape puts the list away.

Vaults made before the markers had plain `[[John 3:16]]` and `[[sword:…]]` links. The first time this app opens such
a vault of its own it writes them in the new form, once, keeping the old files in a folder beside the index (a vault
that is another program's is left alone).

The reference parser is the single source of truth for turning prose into links. It also
**auto-detects bare references** in prose (`see John 3:16`) as a *suggestion*
(configurable), never silently rewriting files.

### Reference identifiers: what is standard and what is ours

- **Identity: OSIS** (the CrossWire standard). A reference is stored, indexed, and written into
  mark files as an OSIS `osisRef`: `John.3.16`, `Ps.23`, and ranges as two full ids joined by a
  hyphen, `John.3.16-John.3.17`. Our 66 book ids are the same as CrossWire's list (checked).
  OSIS also defines more than we handle yet: ranges across chapters, whole books, the
  deuterocanon (Tob, Jdt, Wis, Sir, Bar, 1Macc...), `!` sub-identifiers (notes) and a `Work:`
  prefix, so reading a module's own `osisRef`s should go through the same parser.
- **Interchange URL: `sword://MODULE/OSISREF`** (the form Xiphos opens, e.g. `sword://ESV/Gen.1.1`).
  Not a written specification, only a convention, but it is the nearest thing to a cross-app
  link in this ecosystem. `[[@John 3:16@KJV]]` maps onto it directly; support it for copy-as-link
  and for being opened by the OS.
- **What we write by hand** (no standard exists): the human-typed form `[[@John 3:16]]` (the
  `[[...]]` wrapper is the wiki/Obsidian convention; the book names and abbreviations are ours),
  the `@MODULE` pin, `[[%G5547]]`, `[x](sword:Module/key)`, and the anchor of a margin mark. The anchor's
  `quote`/`prefix`/`suffix` are the W3C Web Annotation *TextQuoteSelector*'s
  `exact`/`prefix`/`suffix` under friendlier names.
- **Not adopted:** USFM/USX book codes (`JHN`, `MAT`) differ from OSIS ids; relevant only when
  importing Paratext or DBL material, where a one-to-one mapping table is enough.
- **Open gaps in `scripture`:** cross-chapter ranges and whole books, the deuterocanon,
  lists (`John 3:16; 4:1`), book-name abbreviations and locales (SWORD ships name tables to
  reuse), the `Work:` prefix, versification mapping, and the `sword://` handler.

### Versification rule
Stored reference IDs are in the **reference versification (KJV)**. If the user's
Bible has different numbering, mapping happens at display and index time (see 01 §4).

## 6. User commentaries and books

These are collections of ordinary files that the app can *present* like installed modules.

**Commentary** — a folder of documents whose `refs`/links tie them to verses; shown in the
module list so selecting a verse lists matching entries. Margin notes are the lighter-weight
sibling: anchored to text rather than organised as a work.

```
commentaries/My Notes on John/
  commentary.toml            # name, abbreviation, versification, description
  john-3.md
```

**Book** — a folder with an ordered table of contents (mirrors a SWORD GenBook):

```
books/Systematic Notes/
  book.toml                  # title, author, order = ["01-intro.md", ...]
  01-intro.md
  02-god/
    _index.md
    attributes.md
```
Linkable via `[[book:Systematic Notes/God/Attributes]]`, mirroring `sword:` GenBook paths,
so the same UI serves both worlds.

## 7. Identity, renames, deletes

- Links resolve by title/alias/path; when the app renames a file it rewrites incoming
  links (like Obsidian), showing a diff.
- No mandatory IDs. Optional `id:` for people who want rename-proof links.
- Deleting a mark or note moves it to `.biblestudy/trash/` (undoable); emptying the trash
  is explicit.

## 8. Exporting / interop

- The vault is the export. Plus "portable export" that rewrites `[[John 3:16]]` into
  standard markdown links and optionally inlines verse text (with licensing warning).
- Stretch: generate **OSIS XML** from a commentary/book folder so it can be turned into a
  real SWORD module with `osis2mod`.

## 9. Parser notes

- CommonMark parser (e.g. `pulldown-cmark`) with a custom extension for `[[…]]`, plus a
  YAML frontmatter parser and an inline-tag scanner.
- The *reference parser* needs: book name aliases in many languages/abbreviations
  (`Jn`, `Jhn`, `1Co`, `Song of Songs`/`Canticles`), list/range syntax (`3:16-18, 21; 4:1`),
  chapter-only, whole-book, cross-chapter ranges. Locale-aware book names (SWORD ships
  locale files for this). Prototype status: `scripture` handles a single chapter and
  verse ranges only.
