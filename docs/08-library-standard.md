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
| Daily devotionals (`Feature=DailyDevotion`) | Not supported: recognised, so as not to be taken for a dictionary, but not read, offered or imported. |
| Reading plans | Imported to markdown, as a plan. |

Bibles and the Strong's lexicons stay as SWORD modules because the app reads them by verse and by
number, with Strong's tagging, versification and the module manager that installs them. A reading plan
is a list for a day. It gains from being a note that other notes can link to: a **plan** is a note for each day,
a completion log, optional passages to read and optional text to read (docs/09-reading-plans.md).

A devotional is told apart from a dictionary by its conf, not by its driver: SWORD stores a devotional
as a `zLD` or `RawLD` dictionary keyed by day, and `sword::Module::kind()` answers `Kind::Devotional`
when the conf has `Feature=DailyDevotion` (checked before the driver). That is all the app does with one:
it is not offered from the repository and cannot be opened (`readability` says so).

## Where files live

```
<vault>/library/<Work>/<file>.md
```

`<Work>` is the work's name with what a file name cannot hold (`/ \ : * ? " < > |`) removed, such as
`library/M'Cheyne/`. Importing writes only inside `library/<Work>/`. Importing again replaces
the files.

## Read-only

Everything under `library/` is read-only in the app: it is shown, searched and linked to (`[[library/M'Cheyne/01-01]]`, or by its name), but not edited. What a person writes about a day goes in a note of their own that
links to it. Completion of a plan's days is kept in the plan log, a note outside the library (docs/09).

## Every library file

Frontmatter first, as in docs/02, with at least these properties:

- `type`: what the note is (`plan` for a work's manifest, `plan-day` for a day).
- `tags`: a list holding the type, such as `[plan-day]`.

Then a `# ` title and the body. Scripture is a `[[@Book 1:1]]` link the verse-link parser reads
(`study_core::markdown::find_links`). Text is markdown with the marks a note's text may not use
written literally with a backslash (`\*`, `\_`, `\[`, `\]`, `` \` ``, `\<`, `\>`, `\~`, `\|`,
`\\`), as the commentary tab does. No markup from the source (OSIS, ThML) is left in.

## The plan shape

A work with a note for each day is a folder with a manifest and the days. The shape is described in full in
docs/09-reading-plans.md. A day's note may have text of its own as well as passages to read, written with the rules above.

## In code

- `study_core::plan::find_plans(&notes)` reads every plan from the vault's notes; `Plan::to_files()` writes one.
- `study_core::commentary::{markdown, escape, plain_text, passage_range}` is the shared
  OSIS-to-markdown writing, which the commentary tab uses as well.

The library is out of the plain list of notes, out of the tag chips and out of what opens first; it is found by
searching (the notes selector has a *Library* group while there is a search), by link, and in the Plans window.
