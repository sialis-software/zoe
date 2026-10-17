# Zoe (Bible Study App) — Overview

This document is the entry point; see the others for detail:

| Doc | Contents |
|---|---|
| [01-sword-integration.md](01-sword-integration.md) | What CrossWire/SWORD gives us, what we must implement, risks |
| [02-vault-and-markdown.md](02-vault-and-markdown.md) | On-disk layout and markdown dialect for user content |
| [03-architecture.md](03-architecture.md) | Crates, indexing, search, UI options |
| [04-scope-and-roadmap.md](04-scope-and-roadmap.md) | Size of the project, milestones, open questions |
| [05-margins-and-index.md](05-margins-and-index.md) | Margin notes vs documents, anchoring, and the derived SQLite index |
| [07-panes-and-tabs.md](07-panes-and-tabs.md) | The workspace: panes holding tabs, and the Bible and note tab types |

## Vision

A friendly, offline desktop Bible study app for ordinary Christians to
read Scripture and write down what God is speaking, *at the speed of thought*.

## Principles

1. **Emulate the paper Bible experience without its constraints.** This is a study Bible with infinite margins and tools for deeper understanding in one place.
2. **Your files are yours.** All user content is plain markdown in a normal folder. Readable in any editor, syncable with any tool (Git, Syncthing, iCloud), and usable even if this app disappears.
2. **Offline, local-first.** No account, no server. Network is only ever an explicit, user-initiated action (downloading modules).
3. **Honor the CrossWire ecosystem.** Instead of rewriting a Bible format, utilize the CrossWire exosystem to make any Bible's you already have usable here.
4. **Everything links to Scripture.** A verse reference is a first-class link in any note. Your vault becomes a stream of thought connecting scripture together as you read it.

## Core features

- **A Bible reader that just works.** It operates exactly how you would expect, and is configurable to the experience you are going for. It allows for distraction-free, zen reading that emulates a study Bible and higlighters, or every footnote, reference, and Strong's number in one place.
- **Powerful note taking.** Everything is a note; highlights, underlines, prayer lists, sermons. Notes are plain-text Markdown, meaning you can make them as formal or simple as desired. Every note can link to any other, letting you look back on a stream of thought instead of a wall of text.
- **Strong's Numbers.** For supported Bible modules, the Greek and Hebrew words are quickly available and fully integrated into the note-taking system.

## Non-goals (v1)

- Selling or bundling copyrighted modules.
- Authoring/publishing SWORD modules

## Core idea: a markdown layer over SWORD

SWORD modules are a **read-only, binary, verse-keyed** library. The user vault is a **read-write, textual, free-form** library. The app joins them through one shared
vocabulary: **canonical scripture references** (OSIS-style IDs like `John.3.16`).

User-authored "commentaries" and "books" are ordinary markdown that the app can *present* as if they were modules — a soft replacement for CrossWire's non-Bible document types without needing the SWORD binary toolchain.
