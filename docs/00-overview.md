# Bible Study App — Overview

Working title: **TBD**. This document is the entry point; see the others for detail:

| Doc | Contents |
|---|---|
| [01-sword-integration.md](01-sword-integration.md) | What CrossWire/SWORD gives us, what we must implement, risks |
| [02-vault-and-markdown.md](02-vault-and-markdown.md) | On-disk layout and markdown dialect for user content |
| [03-architecture.md](03-architecture.md) | Crates, indexing, search, UI options |
| [04-scope-and-roadmap.md](04-scope-and-roadmap.md) | Size of the project, milestones, open questions |
| [05-margins-and-index.md](05-margins-and-index.md) | Margin notes vs documents, anchoring, and the derived SQLite index |
| [07-panes-and-tabs.md](07-panes-and-tabs.md) | The workspace: panes holding tabs, and the Bible and note tab types |

## Vision

A friendly, offline desktop Bible study app for ordinary people (not seminarians) to
read Scripture and keep a lasting record of their own journey with it: notes, prayers,
reading logs, sermon notes, personal commentary. Logos and Xiphos are the reference
points, but Logos is heavyweight and commercial, and Xiphos is capable but its UX and
user-authoring story are dated.

## Principles

1. **Your files are yours.** All user content is plain markdown in a normal folder.
   Readable in any editor, syncable with any tool (Git, Syncthing, iCloud), and
   usable even if this app disappears.
2. **Offline, local-first.** No account, no server. Network is only ever an explicit,
   user-initiated action (downloading modules).
3. **Honor the CrossWire ecosystem.** Read existing SWORD modules unmodified, ideally from
   the same `~/.sword` library that Xiphos/BibleTime/others use. Never fork the format.
4. **Everything links to Scripture.** A verse reference is a first-class link in any note;
   any verse can show every note, prayer, and commentary touching it.
5. **Derived data is disposable.** Indexes and caches can be deleted and rebuilt from
   the vault + modules. Only markdown (and a few small plain-text files) is truth.
6. **Simple by default.** Hide Strong's numbers, morphology, variants, and versification
   until asked for.

## Target user and core journeys

- Read a chapter in a chosen translation; compare a second translation side by side.
- Tap a verse: see cross-references, a commentary entry, my notes on it.
- Write a note while reading; type `John 3:16` and have it become a live link.
- Keep a prayer list or journal as an ordinary document, with checkboxes and links.
- Highlight and underline in the margins as in a paper Bible; add margin notes.
- Follow a reading plan; keep a daily journal that links to what I read.
- Look up a word (Strong's) or search a phrase across Bibles *and* my notes.
- Move my study folder to a new computer by copying it.

## Non-goals (v1)

- Cloud sync, accounts, collaboration (the filesystem is the sync story).
- Mobile apps (design the core as a library so this stays possible).
- Original-language power tools: syntax trees, interlinear alignment editing, etc.
- Selling or bundling copyrighted modules.
- Authoring/publishing SWORD modules (export is a stretch goal, see 04).

## Core idea: a markdown layer over SWORD

SWORD modules are a **read-only, binary, verse-keyed** library. The user vault is a
**read-write, textual, free-form** library. The app joins them through one shared
vocabulary: **canonical scripture references** (OSIS-style IDs like `John.3.16`).

```
 SWORD modules (read-only)            Vault of markdown (read-write)
 Bibles, commentaries, lexicons  <--  [[John 3:16]]  verse links
 general books (GenBook)         <--  [[sword:Josephus/War/1/1]] doc links
        |                                   |
        +--------- reference index ---------+
              verse -> notes, notes -> verses, word -> verses
```

User-authored "commentaries" and "books" are ordinary markdown that the app can
*present* as if they were modules — a soft replacement for CrossWire's non-Bible
document types without needing the SWORD binary toolchain.
