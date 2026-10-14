# 08 - The library standard

Some works are imported into the vault as markdown instead of being read from a SWORD module. This
document is the one standard those files follow, so that every imported work is an ordinary note:
it opens in the viewer, links to Scripture, is found by search and sync, and can be linked to like any
note. Library notes are read-only in the app (see "Read-only" below).

## What is imported and what is not

| Kind of work | Where it lives |
|---|---|
| Bibles | SWORD modules, read in place. |
| Strong's lexicons (and other dictionaries) | SWORD modules, read in place. |
| Commentaries | SWORD modules, read in place. |
| Daily devotionals (`Feature=DailyDevotion`) | Imported to markdown, as a plan. |
| Reading plans | Imported to markdown, as a plan. |

Bibles and the Strong's lexicons stay as SWORD modules because the app reads them by verse and by
number, with Strong's tagging, versification and the module manager that installs them. A devotional
or a reading plan is prose or a list for a day. It gains from being a note that other notes can link to.
Reading plans and devotionals are the same thing, a **plan**: a note for each day, a completion log, optional
passages to read and optional text to read (docs/09-reading-plans.md).

A devotional is told apart from a dictionary by its conf, not by its driver: SWORD stores a devotional
as a `zLD` or `RawLD` dictionary keyed by day, and `sword::Module::kind()` answers `Kind::Devotional`
when the conf has `Feature=DailyDevotion` (checked before the driver).

## Where files live

```
<vault>/library/<Work>/<file>.md
```

`<Work>` is the work's name with what a file name cannot hold (`/ \ : * ? " < > |`) removed, such as
`library/Morning and Evening/`. Importing writes only inside `library/<Work>/`. Importing again replaces
the files.

## Read-only

Everything under `library/` is read-only in the app: it is shown, searched and linked to (`[[library/Morning and
Evening/01-01]]`, or by its name), but not edited. What a person writes about a day goes in a note of their own that
links to it. Completion of a plan's days is kept in the plan log, a note outside the library (docs/09).

## Every library file

Frontmatter first, as in docs/02, with at least these properties:

- `type`: what the note is (`plan` for a work's manifest, `plan-day` for a day).
- `tags`: a list holding the type, such as `[plan-day]`.

Then a `# ` title and the body. Scripture is a `[[Book 1:1]]` link the verse-link parser reads
(`study_core::markdown::find_links`). Text is markdown with the marks a note's text may not use
written literally with a backslash (`\*`, `\_`, `\[`, `\]`, `` \` ``, `\<`, `\>`, `\~`, `\|`,
`\\`), as the commentary tab does. No markup from the source (OSIS, ThML) is left in.

## The plan shape

A work with a note for each day, devotional or reading plan, is a folder with a manifest and the days. The
shape is described in full in docs/09-reading-plans.md; for a devotional it is a calendar plan: the manifest
`library/Morning and Evening/Morning and Evening.md` (`kind: calendar`, the module's description) and
`library/Morning and Evening/<MM-DD>.md` for each day of the year, from the entry key `MM.DD`. February 29 is a day.

```
---
type: plan-day
plan: Morning and Evening
date: 01-01
readings: [Joshua 5:12, Song of Solomon 1:4]
tags: [plan-day]
---
# January 1

## Morning

> *“They did eat of the fruit of the land of Canaan that year.”* — [[Joshua 5:12]]

Israel’s weary wanderings …

## Evening

> *“We will be glad and rejoice in thee.”* — [[Song of Solomon 1:4]]

We will be glad and rejoice in thee. …
```

- `date` is `MM-DD`. `readings` lists the key verses that the plan reader can read; a devotional day has no `## Reading`
  section, the key verse blockquote is its link.
- The `# ` title is the month and day in words, from the key.
- Each part of the day (an OSIS `<div type="section">`) is a `## ` heading from its title, without the
  date the source repeats in it ("Morning, January 1" is `Morning`). An entry that is not divided
  into parts is one section, named by its title if it has one, else `Today`.
- The key verse, which is the first paragraph (the quotation) and its reference, is a blockquote:
  the words in italics, a dash, then a `[[Reference]]` link. The link text is the words the module
  used when they name a passage, else the passage its OSIS reference names; if neither does, the
  reference is plain text.
- Other paragraphs are plain markdown paragraphs. Bold, italic and passages in them are written as the
  commentary tab writes them (`**bold**`, `*italic*`, `[[John 2:23|Joh 2:23,24]]`). Lines of verse are
  paragraphs of their own.
- Keys that are not a day of the year (`MM.DD`) are not imported.
- Older imports wrote `type: devotional`, `work:` and `date: MM-DD` with no manifest. They are still read: a folder
  of such notes is a calendar plan named by `work`. Importing again writes the new shape.

## In code

- `sword::Module::devotional()` opens a devotional; `Devotional::keys()` and `raw(key)` read it.
- `study_core::devotional::plan_files(work, description, entries)` makes the files (the manifest and each day) as
  `(path, text)`; `day_markdown(work, key, raw)` makes one `Day { path, text }`; `import(work, entries)` all the days.
- `study_core::plan::find_plans(&notes)` reads every plan, devotional or reading, from the vault's notes.
- `study_core::commentary::{markdown, escape, plain_text, passage_range}` is the shared
  OSIS-to-markdown writing, which the commentary tab will use as well.

## Importing a devotional

In the module manager (the Modules button in the top bar), a devotional that is installed has *Import to library*.
It writes the plan's files (`plan_files`) for each day the app can read (the work's name is its description without
the author and the subtitle: *Morning and Evening*), and says how many days it made. Importing again replaces them,
so the library holds what the module says. The library is out of the plain list of notes, out of the tag chips and out
of what opens first; it is found by searching (the notes selector has a *Library* group while there is a search), by
link, and in the Plans window, where the devotional appears as a plan like any other.
