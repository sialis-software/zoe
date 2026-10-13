# SWORD / CrossWire Integration

Sources: CrossWire wiki (DevTools:Modules, DevTools:conf_Files, OSIS Bibles), the
rsword_chirho crate docs, and the module library already on this machine at `~/.sword`.
Items marked **[verify]** are from memory of the SWORD source and must be confirmed
against real module bytes before relying on them.

## 1. Module library layout

```
~/.sword/
  mods.d/<name>.conf        one INI-style config per module
  modules/<DataPath>/...    binary data, location given by conf DataPath
  InstallMgr/               repo lists / install state (CrossWire installer)
```

Frontends also search `$SWORD_PATH`, `/usr/share/sword`, and `~/.sword`, and honor
`DataPath=` lines in a `sword.conf` (SWORD convention, not a formal spec; Windows and
macOS defaults vary by frontend **[verify]**). Our app should read the same locations and
let the user add more, so modules installed by Xiphos or `installmgr` "just appear".

Present locally (good test fixtures): `asv` (zText Bible), `strongsgreek` (zLD),
`strongshebrew` (RawLD).

## 2. The `.conf` file

INI-like: first line `[ModName]`, then `Key=Value`, repeated keys allowed, a trailing
`\` joins continuation lines, locale-suffixed keys (`Description_fr`). Keys that drive
decoding:

| Key | Meaning |
|---|---|
| `ModDrv` | storage driver: RawText(4), zText(4), RawCom(4), zCom(4), HREFCom, RawFiles, RawLD(4), zLD, RawGenBook |
| `DataPath` | relative path under library root |
| `SourceType` | markup: Plaintext, OSIS, TEI, (legacy GBF, ThML) |
| `Encoding` | UTF-8, UTF-16, SCSU, (legacy Latin-1) |
| `CompressType` | LZSS (default), ZIP, BZIP2, XZ |
| `BlockType` | BOOK, CHAPTER (default), VERSE — compression block granularity |
| `Versification` | KJV (default), KJVA, NRSV(A), Catholic(2), LXX, Vulg, Luther, German, Synodal(Prot), MT, Leningrad, Orthodox, Segond, Calvin, Darby_Fr |
| `GlobalOptionFilter` | toggleable features: OSISStrongs, OSISMorph, OSISLemma, OSISFootnotes, OSISHeadings, OSISRedLetterWords, OSISVariants, UTF8Cantillation, … |
| `Feature` | StrongsNumbers, GreekDef, HebrewDef, DailyDevotion, Glossary, Images, NoParagraphs |
| `CipherKey` | present-but-empty = locked; value = unlocked |
| `KeyType`, `CaseSensitiveKeys`, `BlockCount`, `Direction`, `DisplayLevel`, `Lang`, `Category` | misc. |
| `DistributionLicense`, `Copyright*`, `About`, `Version`, `MinimumVersion` | metadata we should surface in UI (license matters, see §8) |

## 3. Module types and what each needs

| Category | Drivers | Key space | Notes |
|---|---|---|---|
| Bible | zText, RawText (+4 variants) | verse (versification-dependent) | Highest priority. Most modules in the wild are zText. |
| Commentary | zCom, RawCom, RawFiles, HREFCom | verse | Same verse indexing as Bibles; entries often cover ranges. |
| Lexicon / dictionary / daily devotional | zLD, RawLD | string key (case-insensitive by default) | Strong's lookups; devotionals keyed `MM.DD`. |
| General book | RawGenBook | **tree** key (`/Chapter/Section`) | `.bdt/.idx/.dat` family; hierarchical TOC. |

### Bible / commentary binary layout (zText family) **[verify]**

Per testament, three files: `ot.bzs ot.bzv ot.bzz` (and `nt.*`).
- `.bzz` — concatenated compressed blocks (the text).
- `.bzs` — block table: per block, `offset`, `compressed size`, `uncompressed size`
  (u32 little-endian each).
- `.bzv` — verse index: per verse, `block number` (u32), `offset in uncompressed block`
  (u32 for zText4 / see below), `length` (u16 for zText, u32 for zText4).
- Verse record N is located by its ordinal in the module's **versification**
  (hence versification tables are mandatory, not optional).
- RawText family: `.vss` per testament + `ot/nt` data and an index of (offset,size)
  pairs, no compression.

Plan: write a tiny `zText` parser, run it on the local `asv`, and diff against output
from the real `diatheke`/`mod2imp` to validate. This is the single most valuable
first spike.

### Lexicon (zLD / RawLD)
`.idx` (offsets into `.dat`), `.dat` (entries; key line + body), plus `.zdx/.zdt` for
compressed. Strong's keys are zero-padded (see `StrongsPadding`); Greek `G0001`-style
vs Hebrew `H0001`. Entries are in ThML/OSIS/plain per `SourceType`.

### GenBook (RawGenBook)
Tree stored as nodes with parent/first-child/next-sibling offsets in `.bdt/.idx`.
Needed for most non-Bible "documents" users will want (CCEL-style books, creeds, etc.).

## 4. Versification — the hidden hard part

A SWORD module is indexed by *ordinal verse position in its canon*. KJV, NRSV,
Catholic, LXX, Synodal, Vulgate, Luther, MT… differ in book order, extra books, and
chapter/verse boundaries (Psalms numbering, Malachi 3/4, 3 John, Joel, etc.).

Requirements:
- Embed canon tables for every versification in the conf list (book list, chapter
  verse-counts). SWORD ships these in `canon*.h` / versification files; these are the
  *data* we need, not code we need to copy.
- A **mapping** between versifications (SWORD's "verse mappings") so a note linking
  `Ps.51.1` shows the right text in a Bible with different numbering.
- Rule: **notes store references in one reference versification (KJV)** and map at
  display time. See 02.

## 5. Text markup → rendering

Raw verse strings are marked-up fragments, not HTML. We need a pipeline:

```
raw entry -> parse by SourceType -> internal inline AST -> render (UI)
                                   ^ GlobalOptionFilter toggles applied here
```

- **OSIS** (modern modules): `<w lemma="strong:H07225" morph="...">`, `<note>`,
  `<q who="Jesus">` (red letter), `<divineName>`, `<l>`/`<lg>` poetry, `<reference osisRef>`,
  `<transChange>`, `<title>`. Sub-verse structure uses milestones (`sID`/`eID`).
- **ThML** (older commentaries/books, CCEL): HTML-like, `<scripRef passage="">`, `<sync type="Strongs">`.
- **GBF** (legacy): `<FI>`, `<WG123>`, `<RF>…<Rf>`.
- **TEI** (dictionaries), **plain**.

Output AST nodes should include: text, paragraph/line break, heading, emphasis,
red-letter, divine name, footnote, **scripture reference (parsed)**, **Strong's/morph
word span**, image, link to other module key. Scripture references in module text
(`osisRef`/`scripRef passage`) feed the same link index as user notes.

Estimated as the largest single chunk of "SWORD work" — OSIS+ThML+plain covers the
vast majority of useful modules; GBF/TEI can come later.

## 6. Locked modules
`CipherKey` modules use a stream cipher (Sapphire II **[verify]**). Supporting
user-supplied unlock keys is small once reading works. Store keys outside the vault.

## 7. Distribution / install

- CrossWire hosts module repositories (FTP/HTTP, index of `mods.d` confs + zip
  archives, "Zipped modules" format). Offline principle: downloading is an explicit
  user action; also support **sideloading** a `.zip` or a folder.
- Respect `DistributionLicense` when listing; surface "Copyrighted" modules' terms.
- Compatibility rule: **never mutate an installed module**. We only read.

## 8. Rust landscape and licensing

- **rsword_chirho** (docs.rs, v0.3.0, GPL-2.0-or-later): pure-Rust port claiming
  zText/zCom/zLD/RawGenBook and multiple versifications. Young and apparently
  single-author; evaluate quality before depending on it. Pre-1.0, naming suggests
  an unusual provenance — read the code before trusting it.
- The reference **libsword is GPL-2**. Independent reimplementation from the
  *format* is legally cleaner than translating C++; but if we copy/translate source or
  depend on a GPL crate, our application must be GPL-compatible.
  **Decision needed (see 04):** GPL application (simplest, fits ecosystem — Xiphos and
  BibleTime are GPL) vs. clean-room format reader under a permissive license.
- Other readers exist as format references: **pysword** (Python, ztext/ztext4/rawtext/rawtext4),
  **swordjs** (zText only). Both are useful as a second implementation to cross-check.
- Existing C++ tools (`mod2imp`, `diatheke`, `osis2mod`) make an ideal **oracle** for tests.

## 9. Integration options

| Option | Pros | Cons |
|---|---|---|
| A. Own pure-Rust reader (`sword-rs` crate) | native, no C++ build, fully controlled, reusable | most work; need exhaustive testing |
| B. Adopt/fork rsword_chirho | head start | quality/maintenance unknown; GPL |
| C. FFI to libsword (C++) | proven, complete | build/packaging pain, not "pure Rust", GPL |

**Recommendation:** A, using B and pysword as references and the C++ tools as test
oracles. Start read-only; no module writing in v1.

## 10. Status

Implemented in `crates/sword`, all read-only and checked against `mod2imp` on every verse
or entry of the installed modules (ASV, LSB, StrongsGreek, StrongsHebrew, LSBG, LSBH):

- `.conf` parsing; library search over `ZOE_SWORD_PATH` (replaces the rest), folders
  added in Settings → Bible modules (kept in `config.json`), `SWORD_PATH` and the app's own
  `~/.config/sword` (made on start if missing). Other frontends' folders are not searched; add
  one in Settings if wanted.
- zText Bibles (ZIP-compressed, KJV versification, OSIS markup) into the shared AST:
  paragraphs, poetry, headings, footnotes, Strong's words, words of Jesus.
- RawLD and zLD dictionaries, with TEI, ThML and plain entries parsed to lemma,
  transliteration and definition.

Not yet: other versifications and the mapping between them, LZSS/XZ/BZIP2 compression,
RawText, commentaries, GenBooks, ThML/GBF Bibles, cipher keys, cross-chapter `osisRef`s.
Modules we cannot read are left out of the module list rather than erroring.
