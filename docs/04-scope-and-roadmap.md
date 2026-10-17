# Scope, Roadmap, Risks, Open Questions

## 1. How big is this?

Rough relative sizing (S ≈ days, M ≈ 1–3 weeks, L ≈ 1–2 months, XL ≈ 2+ months for one experienced developer; treat as order-of-magnitude, not commitments).

| Area | Size | Why |
|---|---|---|
| `.conf` parser + library discovery | S | Simple INI variant |
| zText/RawText Bible reader (read-only) | M | Binary format is small; compression variants + validation take time |
| Versification tables + mapping | M–L | Lots of data/edge cases; correctness is subtle |
| Reference parser (multilingual, lenient) | M | Many aliases and syntaxes; heavily testable |
| OSIS → AST (incl. Strong's, notes, red letter, poetry) | L | Large spec surface, milestone elements |
| ThML / GBF / TEI → AST | M each | Needed for older commentaries and books |
| Commentary / lexicon / GenBook readers | M | Same primitives, different key spaces |
| Vault: markdown parse, links, watcher, safe writes | M | Mostly library glue |
| Link/backlink index + interval queries | M | |
| Full-text + Strong's search | M | SQLite FTS5 makes it tractable |
| Margin layer: pens, anchoring, margin cards | L | Cards need child widgets in the reader; see 05 |
| **UI: reading experience** | L | Rich text with interactive spans |
| **UI: markdown editor** | L–XL | Highest risk in Rust-native toolkits |
| Module install/download, unlock keys | M | Optional for v1 (sideload first) |
| Reading plans, prayer UX, highlights | M | Product features atop the core |
| Packaging, signing, updater (mac/win/linux) | M | |

**Summary:** the *data/engine* side is well-bounded and very doable (months, not years), because SWORD is a simple, documented, read-only format family. The *product/UI* side (rich-text reading + editing in Rust) is the part that decides the schedule.

## 2. Milestones

**Phase 1 / M0 – Frontend prototype with mock data (current focus).** Goal: find out what the UI should feel like, and whether a native Rust toolkit can deliver it, before any SWORD parsing. No real module or vault I/O.
1. Define the `core` service traits (03 §2) and implement them with an in-memory **mock backend**: a few chapters of hand-written (public-domain) text in the AST shape from 01 §5, sample notes, a prayer list, fake Strong's entries, fake commentary.
2. Build the same small set of screens against that mock, in each candidate native toolkit (start with iced; try one more, e.g. Slint or egui, only if iced stalls):
   - Bible reader: chapter text, verse selection, clickable cross-refs, Strong's popover, footnote markers, red-letter/poetry styling, parallel view.
   - Markdown editor with live `[[John 3:16]]` links and a verse-embed block.
   - Context panel: "notes on this verse", cross-refs, commentary.
   - Margin layer over the reader (highlights, underlines, margin cards) — see 05.
3. Evaluate against a checklist: text selection across styled spans, inline clickable spans, IME/keyboard handling, scrolling large chapters, Hebrew/Greek/RTL rendering, editor feel, accessibility, look on macOS/Windows/Linux.
4. Exit criteria: pick the toolkit (or conclude native is not viable and revisit), and settle the `core` trait boundary so the real backend can slot in.

**M0b – Format spikes (after Phase 1).** Parse `~/.sword/mods.d/*.conf`; read John 3:16 from local `asv` zText and match `diatheke`; read Strong's entries from `strongsgreek` (zLD) and `strongshebrew` (RawLD).

**M1 – Read the Bible (CLI + minimal UI).** `scripture`, `sword` (zText/RawText), OSIS and plain rendering, KJV-family versifications. Reading, chapter nav, search by reference.

**M2 – Vault & links.** Vault layout, notes editor, `[[ref]]` links, backlinks panel ("notes on this verse"), per-verse note indicators.

**M3 – Study depth.** Commentaries (zCom/RawCom), lexicons with Strong's popovers, parallel Bibles, footnotes/cross-refs, full-text search across modules + notes, all remaining versifications with mapping.

**M4 – Personal journey.** Margin layer (pens, margin notes), reading plans, bookmarks, user commentaries, and books presented as modules.

**M5 – Documents & ecosystem.** GenBook, ThML/GBF/TEI, locked-module keys, in-app module download from CrossWire repos, import/export (OSIS export of user books).

**M6 – Polish & release.** Onboarding, accessibility, localization, installers.

An MVP worth showing to real users is **M2**; M3 makes it competitive with Xiphos for the casual reader.

## 3. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Rust UI cannot deliver good rich text/editing | schedule | Spike early; keep core GUI-agnostic; fallback to webview shell |
| Versification mapping errors silently misalign text | correctness/trust | Embed official tables, golden tests vs. SWORD, show mapped-refs warning |
| Module markup variety (real modules are messy) | long tail | Fuzz with the full CrossWire repo; render-unknown-as-text fallback |
| Licensing contamination (GPL libsword/rsword) | legal | Decide up front; clean-room from format docs or choose GPL deliberately |
| Existing Rust crate (rsword_chirho) is low quality/unmaintained | rework | Use as reference only; own the reader |
| Copyrighted module text in test data / exports | legal | Public-domain fixtures only; transclude, never copy verse text into notes |
| Index size/time on large libraries | UX | Lazy, incremental, per-module opt-in indexing |
| Scope creep toward "Logos clone" | focus | Hold to non-goals; user-journey-driven backlog |

## 4. Decisions and open questions

**Decided**
- **License:** deferred. Until decided, do not copy or translate GPL code (libsword, rsword_chirho); read them only as format references, so either choice stays open.
- **UI:** keep it native if possible. Phase 1 is a mock-data frontend prototype to test that (see §2). A webview shell is the fallback only if the prototype shows native cannot meet the bar.
- **Reader text:** iced's `rich_text` has no selection, so the reader gets a custom selectable rich-text widget (cosmic-text hit-testing, rendering our AST). Not built yet; the prototype's `rich_text` reader is a stand-in.
- **Editor:** v1 is markdown source with syntax coloring and clickable links (iced `text_editor` + custom highlighter). Obsidian-style live WYSIWYG is a future goal; keep editor logic (link parsing, document model) separate from the widget so it can be swapped. **Built (prototype):** the live-preview editor from 06, as a GUI-free model crate (`md-editor`) plus one custom widget, used by both the writer pane and the margin cards. See 06 §0 for what exists and what does not.
- **Content model:** margin notes (anchored, `type: margin`, one file each; a highlight is a margin note with no body) and documents (linked). Prayer lists, journals, and the like are ordinary documents with no special treatment; plan progress is checkboxes in the plan file. See 02 and 05.
- **Series (future):** a series is a collection of related notes, highlights, and documents. Planned as members carrying `series: ["[[Name]]"]` plus a `type: series` document; not built. See 02 §4 and 05 §9.
- **Margin cards:** built in the prototype. A reader wide enough shows a permanent margin of cards; a narrower one hides them and floats a card under a clicked mark. The Writer toggle hides the writer to test the wide case. Open: which side the margin is on, bare-highlight ticks, whether cards edit in place long-term. See 05 §4.
- **Index:** SQLite, fully regenerable from vault + modules, kept outside the vault; two databases (vault, modules); FTS5 replaces tantivy. See 05 §6.
- **Platforms:** macOS, Windows, and Linux from day one. This favors cross-platform toolkits, avoids platform-specific APIs, and means CI builds all three early.
- **Module library:** use the standard SWORD locations (default `~/.sword`, plus `$SWORD_PATH` and user-added paths) so modules are shared with Xiphos and other frontends. Per-OS defaults still need verifying.

**Still open**
5. **Margin-layer open questions:** see 05 §8.
6. **Auto-linking bare references:** suggestion-only vs. convert-on-type?
7. **Primary language:** English-only v1, or design locale/RTL in from the start?
8. **Mobile later?** If yes, it favors a strict core/UI split and avoids platform-specific deps now.
9. **Original-language features** (interlinear, morphology search): how far in v1?

## 5. Suggested next step

Start **Phase 1**: create the Cargo workspace with `scripture` (types only), `core` (traits + mock backend), and `app`, then build the Bible reader screen in iced against mock data. The format spikes (M0b) follow once the UI direction is settled.
