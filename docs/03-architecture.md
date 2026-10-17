# Architecture

## 1. Shape

A Cargo workspace; everything except the UI crate is GUI-agnostic so tests are fast, a CLI is easy, and other frontends stay possible.

```
zoe/
  crates/
    sword/       # read SWORD library: conf parser, drivers, versification, markup->AST
    scripture/   # canon tables, OSIS refs, reference parser, versification mapping
    vault/       # fs watcher, markdown + frontmatter parse, link extraction, note CRUD
    index/       # derived data: link/backlink index, full-text search, Strong's search
    core/        # app services: open module, render passage, "what touches this verse"
    app/         # UI binary
    cli/         # dev tool: dump module, resolve refs, rebuild index (also the test oracle driver)
```

Dependency direction: `app -> core -> {sword, vault, index} -> scripture`.

## 2. Key components

**`scripture`** — the shared vocabulary. Book/canon tables per versification, `Ref`/`RefRange` types (OSIS-compatible), reference parser (lenient, multilingual), mapping between versifications. Used by both SWORD readers and notes, so it has no dependency on either.

**`sword`** — read-only access. `Library::discover(paths)` -> modules (parsed `.conf`). Traits by key shape: `VerseModule::read(Ref) -> RawEntry`, `DictModule::get(key)`, `TreeModule::children(path)`. Markup filters (OSIS/ThML/GBF/TEI/plain) produce an inline AST. Memory-map files; keep a small LRU of decompressed blocks (chapter blocks make this effective).

**`vault`** — the user's folder. Walk + watch (`notify`), parse each markdown file (CommonMark + wikilinks + callouts + frontmatter), extract links/refs, atomic writes (write temp + rename) so a crash or a sync tool never sees half a file. Handles external edits gracefully (reload, conflict prompt only if unsaved edits).

**`index`** — derived and disposable.
- Link graph: note <-> note, note -> OSIS ranges, module text -> OSIS ranges (cross-refs). An interval structure (ordinal verse ranges) answers "all notes touching John 3:16" including enclosing ranges.
- Full text: SQLite FTS5 for notes and per-module verse text, built lazily in the background, searchable by word, phrase, Strong's number, lemma.
- Storage: two SQLite databases (`rusqlite`, bundled) in the OS cache dir, **not** the vault: a vault index and per-module indexes. Fully regenerable; version-stamped, rebuilt on mismatch. Details and invariant in 05 §6.
- Incremental: file mtime+hash; rebuild only changed notes.

**`core`** — use-case layer: `render_passage(ref, modules, options) -> Document`, `notes_for(ref)`, `search(query, scope)`, `create_note_from_selection`, etc. The UI only calls this.

## 3. Data flow: reading a chapter

1. UI asks `core` for John 3 in `ASV` (+ optional parallel).
2. `core` maps the ref into ASV's versification, `sword` reads the chapter's verse entries (decompress block, slice, decode).
3. Markup filter -> AST, applying user toggles (Strong's, footnotes, red letters).
4. `index` supplies per-verse badges: note count, highlights, bookmarks.
5. UI renders AST; tapping a verse queries `notes_for` and cross-refs.

## 4. UI technology — the biggest open decision

The app is mostly **rich text**: selectable, inline-clickable (verse refs, Strong's words, footnote markers), with a good markdown *editor*. That is exactly where Rust-native UI toolkits are weakest (text selection across styled spans, IME, accessibility, rich editing). Treat this as the top technical risk, and time-box a spike.

| Option | "Native Rust"? | Rich text / editor | Notes |
|---|---|---|---|
| **iced** | yes | custom work; `cosmic-text` shaping | Elm-style architecture; pairs well with a core library |
| **egui** | yes | weak rich text + selection | quick to prototype, tool-like look |
| **Slint** | yes (DSL for UI) | limited rich text | polished, licensing terms to review |
| **Dioxus desktop / Tauri** | Rust logic, UI in a webview | excellent (CodeMirror/ProseMirror, HTML for verse text) | pragmatic; not "pure" native, larger footprint |
| **GPUI / Xilem / others** | yes | evolving | check maturity at decision time |

Facts here move quickly; **re-verify current state of each before choosing**.

**Decision:** native is preferred, and Phase 1 prototypes the UI against a mock `core` backend (see 04 §2). The recommendation below stands as the method.

**Recommendation:** keep all logic in GUI-agnostic crates (above), then run a 1–2 week spike rendering a verse-with-clickable-refs passage + a markdown editor in iced and a webview option, and decide on evidence. If "native Rust" is a hard requirement, iced is the likely candidate with a custom editor widget on `cosmic-text`; if it is a preference, a webview shell removes most of the UI risk.

## 5. UX sketch

- **Left:** navigator (module list, vault tree, search, reading plan).
- **Center:** tabbed panes — Bible (single/parallel), notes editor, commentary.
- **Right:** context panel for the focused verse: cross-references, commentary entries from chosen modules, **my notes**, Strong's popover.
- Verse tap -> "Add note", "Highlight", "Copy ref", "Pray about this".
- Plain-English labels; advanced options (Strong's, morphology, versification) behind a "More".

## 6. Cross-cutting concerns

- **Performance:** startup should not scan module bodies; parse `.conf` only. Index in background with progress; first launch works before indexing completes.
- **Testing:** golden tests against `mod2imp`/`diatheke` output for real modules; property tests for reference parser round-trips; fixture vaults for link extraction. Never commit copyrighted modules; use public-domain ones (ASV, KJV, WEB) and small synthesized modules.
- **Errors:** a bad module or note never takes down the app; show per-item diagnostics.
- **Encoding:** UTF-8 internally; handle UTF-16/SCSU/Latin-1 modules at the edge. Normalize Unicode for search (Greek accents, Hebrew points toggles).
- **Accessibility/i18n:** RTL text (Hebrew modules set `Direction`), font fallback for polytonic Greek/Hebrew. Design strings for localization from day one.
- **Config:** TOML via `serde`; OS config dir for machine state.
- **Licensing:** see 01 §8; module licenses displayed in UI.

## 7. Candidate crates (verify at adoption time)

`pulldown-cmark` (markdown), `serde`/`serde_yaml`/`toml`, `notify` (fs watch), `memmap2`, `flate2` / `bzip2` / `xz2` (compression; LZSS is tiny, write ourselves), `quick-xml` (OSIS/ThML), `rusqlite` (index + FTS5), `unicode-normalization`, `directories` (paths), `ignore`/`walkdir`, `thiserror`, `tracing`.
