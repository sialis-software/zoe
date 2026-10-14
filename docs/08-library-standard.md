# 08 - The library standard

Some works are imported into the vault as markdown instead of being read from a SWORD module. This
document is the one standard those files follow, so that every imported work is an ordinary note:
it opens in the editor, links to Scripture, is found by search and sync, and can be edited.

## What is imported and what is not

| Kind of work | Where it lives |
|---|---|
| Bibles | SWORD modules, read in place. |
| Strong's lexicons (and other dictionaries) | SWORD modules, read in place. |
| Commentaries | SWORD modules, read in place. |
| Daily devotionals (`Feature=DailyDevotion`) | Imported to markdown. |
| Reading plans | Imported to markdown (the shape is in docs/09-reading-plans.md). |

Bibles and the Strong's lexicons stay as SWORD modules because the app reads them by verse and by
number, with Strong's tagging, versification and the module manager that installs them. A devotional
or a reading plan is prose or a list for a day. It gains from being a note, and a person may want to
change it.

A devotional is told apart from a dictionary by its conf, not by its driver: SWORD stores a devotional
as a `zLD` or `RawLD` dictionary keyed by day, and `sword::Module::kind()` answers `Kind::Devotional`
when the conf has `Feature=DailyDevotion` (checked before the driver).

## Where files live

```
<vault>/library/<Work>/<file>.md
```

`<Work>` is the work's name with what a file name cannot hold (`/ \ : * ? " < > |`) removed, such as
`library/Morning and Evening/`. Importing writes only inside `library/<Work>/`. The files are plain
notes, so changing one is allowed; importing again replaces them.

## Every library file

Frontmatter first, as in docs/02, with at least these properties:

- `type`: what shape the work is (`devotional`).
- `work`: the work's name, as the person knows it.
- `tags`: a list holding the type, such as `[devotional]`.

Then a `# ` title and the body. Scripture is a `[[Book 1:1]]` link the verse-link parser reads
(`study_core::markdown::find_links`). Text is markdown with the marks a note's text may not use
written literally with a backslash (`\*`, `\_`, `\[`, `\]`, `` \` ``, `\<`, `\>`, `\~`, `\|`,
`\\`), as the commentary tab does. No markup from the source (OSIS, ThML) is left in.

## The devotional shape

One file for each day of the year, named `<MM-DD>.md`, from the entry key `MM.DD`. February 29 is a
day.

```
---
type: devotional
work: Morning and Evening
date: 01-01
tags: [devotional]
---
# January 1

## Morning

> *“They did eat of the fruit of the land of Canaan that year.”* — [[Joshua 5:12]]

Israel’s weary wanderings …

## Evening

> *“We will be glad and rejoice in thee.”* — [[Song of Solomon 1:4]]

We will be glad and rejoice in thee. …
```

- `date` is `MM-DD`.
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

## In code

- `sword::Module::devotional()` opens a devotional; `Devotional::keys()` and `raw(key)` read it.
- `study_core::devotional::day_markdown(work, key, raw)` makes one `Day { path, text }`;
  `import(work, entries)` makes all of them.
- `study_core::commentary::{markdown, escape, plain_text, passage_range}` is the shared
  OSIS-to-markdown writing, which the commentary tab will use as well.
