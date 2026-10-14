# 09 - Plans

A reading **plan**. A plan has a note for each day (named by an absolute date or by a
number relative to when the plan was started), a completion log, optional passages to read each day, and optional text to
read each day. A plan is not edited, but its notes are linked to like any note, and every plan appears on the
plan screen and tracks progress. The code is `crates/core/src/plan.rs` (plans) and `plan_log.rs` (the completion log).

## The standard

A plan is a folder in the library, `library/<Plan name>/` (docs/08), with a manifest note and one note for each day. Everything
there is read-only in the app.

### The manifest

`library/<Plan name>/<Plan name>.md` (the "folder note"):

```markdown
---
type: plan
name: Gospels in 30 days
description: Matthew to John, a chapter or two a day.
kind: sequential
start: 2026-01-01
source: Imported from gospels.csv
tags: [plan]
---
# Gospels in 30 days

Matthew to John, a chapter or two a day.
```

- `type: plan` is what makes a note a plan's manifest. `name` is the plan's name (the title, else the file's name, when it is
  missing). `description` is optional (the body's text under the title is used when the property is missing).
- `kind` says what the days' keys are: `sequential` (`Day N`), `dated` (a date) or `calendar` (a day of the year, `MM-DD`,
  every year). It is the kind of the first day when it is missing.
- `start: YYYY-MM-DD` is optional. It is the date day 1 falls on, set when a sequential plan is started. Only sequential plans
  have one.
- `source` is optional: where the plan came from.

### The day notes

One note for each day, `library/<Plan name>/<key>.md`, where the key is `Day 001` (sequential, zero-padded to the width of the
plan's length), `2026-02-18` (dated) or `01-01` (calendar, `MM-DD`; February 29 is `02-29`):

```markdown
---
type: plan-day
plan: Gospels in 30 days
day: 1
title: Beginnings
readings: [Matthew 1-2]
tags: [plan-day]
---
# Beginnings

## Reading

- [[Matthew 1]]
- [[Matthew 2]]
```

- `type: plan-day`, `plan: <Plan name>` and exactly one of `day: 1` (relative), `date: 2026-02-18` (absolute) or `date: 01-01`
  (calendar). When none is there, the file's name is read as the key.
- `title` is optional. The body's `# ` heading is the title, or else the key.
- `readings` is optional: a list of passages, the machine-readable ones (`Matthew 1-2`, `Psalm 119:1-88`, `John 3:16-4:3`, a book
  name alone for the whole book). It can also be written as a list over lines. A passage that cannot be read is left out and
  named in the plan's `problems`, with the note it was in; the rest is read.
- `## Reading` holds one `- [[Matthew 1]]` link for each chapter when the day has passages, so that they can be followed.
- Whatever else the day has follows: headings, a key verse in a blockquote, and so on.
  Both the passages and the text are optional: a reading-only day has no text, and a day of text may have no passages (docs/08).
- A chapter range (`Genesis 1-3`) is read as one passage for each chapter, and a verse range across chapters
  (`John 3:16-4:3`) is split by chapter, so that progress can be counted in chapters. Written back, the pieces are joined again.
- Book names go through `Book::parse`: `Gen`, `Genesis`, `1 Cor`, `Psalm` and `Psalms` all work. Text after a spaced dash is a note
  and is not read for passages.

### The three shapes

| Kind | Day key | Day it falls on |
| --- | --- | --- |
| `sequential` | `Day 3` / `day: 3` | the plan's `start` plus 2 days |
| `dated` | `2026-01-01` | exactly that date |
| `calendar` | `01-01` | that month and day, every year; February 29 falls only in leap years |

`Plan::day_on(date, started)` finds the day for a date. A plan has no pace: nothing is ever *behind* and there is nothing to catch
up, only a log of the days that were finished. *Today* is the next day that is not done, whatever the date
(`PlanProgress::today`); `day_on` is for finding a day by its date.

### Legacy forms

These are still read:

- A plan written as one note, `type: reading-plan`, with a `## Day N` (or `## 2026-01-01`, `## January 1`, `## Jan 1`, `## 01-01`)
  heading for each day and bullets of readings under it as `[[wikilinks]]` or plain text (`Genesis 1-3; Matthew 1`), and an
  optional `start`. It is found by `find_plans` as a plan with no folder and no day notes (`Plan::folder` is `None`), and is the
  form `Plan::parse` and `Plan::render` work in, for authoring and for the importers. A heading may carry a title after a dash
  or colon (`## Day 3 — Creation`). Other headings are not days.

## The completion log

Progress is kept in a note `Plan log` (the app owns its path), which works like the reading log (docs/02): a `## YYYY-MM-DD`
heading for each day, and under it a bullet for each plan day completed that day, linking to the day's note by its path with a
friendly label:

```markdown
## 2026-10-13

- [[library/Morning and Evening/01-01|Morning and Evening · January 1]]
```

`plan_log::{new_log, add, remove, completed}`: `add(log, day_path, label, now)` adds a day under today's heading (`None` when it
is there), `remove(log, day_path, now)` takes today's line back, and `completed(log)` maps each day note's path (with `.md`) to
the latest date it was completed.

A day is **done** when it has a completion entry on or after the plan's baseline, and only then: reading the chapters elsewhere
does not count (marking a day done also logs its chapters in the reading log, but not the other way round).

The **baseline** is the plan's `start` (sequential, so a plan not started has none and nothing is done), the earliest date
(dated), or January 1 of the current year (calendar, so last year's completions do not count).

- `Plan::baseline(today)`, `Plan::is_started()`.
- `Plan::day_done(day, today, completed)` and `Plan::progress(today, completed)`, which gives `PlanProgress { baseline, days_done,
  today, total }`; `completed` is `plan_log::completed`.

## Finding and writing plans

- `find_plans(&notes)` takes the vault's notes (path to text) and returns every plan, sorted by name: each manifest with the day
  notes of its folder, sorted by day (each `Day` has its note's path in `note`), a plan written as one note, and in `problems` what
  could not be read.
- `Plan::to_files()` is the plan as its manifest and a note for each day, as `(path, text)` for the app to write. `Plan::from_files`
  reads them back (`find_plans` on them).

## Importers

Each importer returns a `Plan` (with its `problems`), and the app writes `plan.to_files()`. A single-file markdown plan goes through
`Plan::parse` and then `to_files` the same way. `Plan::render` still writes the single-file form.

- `plan_from_csv(name, text)`: columns `day,readings` or `date,readings`; the header is optional (a first cell that is not a day is
  skipped); quoted fields are allowed, with `""` for a quote. A first column that is a number makes a sequential plan, an ISO date a
  dated plan, `Jan 1` or `01-01` a calendar plan. Further columns are more readings for the same day.
- `plan_from_ics(name, text)`: iCalendar. Each VEVENT with a `DTSTART` date (`DTSTART;VALUE=DATE:20260101` or a date-time) and readings in
  `SUMMARY` (or, if the summary has none, `DESCRIPTION`) becomes a dated day. Folded lines are unfolded, and events on one date are one day.
- `to_ics(&plan, start)`: export as all-day events, one per day, with a UID per day, CRLF line endings and lines folded at 75 octets. Day 1
  falls on `start`; a calendar day on its first date on or after `start`.
- `plan_from_sword_lines(name, text)`: EXPERIMENTAL. The CrossWire/Xiphos plan file shape is not confirmed, because no sample was available. It
  reads the guessed common shape, one day per line: `1:Gen.1,Gen.2` or `2=Gen 3-4; Ps.23`, with readings in OSIS or in words, and `#` or `;`
  comment lines. Until a real sample is checked, treat the result as unreliable. The line shape lives in `sword_line` and the readings in
  `sword_reading`, so adjusting it is a small change.
- `from_csv`, `from_ics` and `from_sword_lines` are the same importers returning the single-file markdown.

Readings an importer cannot read are left out of the plan; they are in its `problems`.

## The plans window and the plan tab

The **Plans** button in the top bar opens a window for managing plans: each plan with its size and progress and *Open*
(read it in the plan tab), *Start today* (a numbered plan) and *Remove* (asks first); *Import…* for a markdown plan, a CSV,
or an iCalendar file or the experimental SWORD lines.

Plans are read in the plan tab (also *Plans* in a new tab): the list of plans with how far each has got, then one day at
a time, its note read-only, with ‹ › for the day before and after, *Today* and
*Mark as done*. Each reading of the day (a chapter) has a box to tick as it is done; ticking one records it in the plan log
(`- [[library/Plan/Day 001#Genesis 1|label]]`, which is not a completion) and in the reading log, the last one ticked completes the
day, and unticking any takes the completion back. A link to a note in `library/` opens in this tab at that day.

## The plans a new vault starts with

A vault made by the first-run window (and any empty folder opened as a new vault) gets the plans in
`crates/core/src/starter_plans.rs`, written once into `library/` like an imported plan (`Options::plans`):

- **Bible in a year**: Genesis to Revelation in order, a numbered plan of 365 days, from `crates/core/data/canonical.csv`. Whole chapters
  only, each day ending near an even share of the verses read so far (28 to 176 verses a day; Psalm 119 is a day on its own). Made by
  `cargo run -p sword --example canonical_plan > crates/core/data/canonical.csv`, which also reports the spread.
- **M'Cheyne**: Robert Murray M'Cheyne's 1842 calendar (public domain), a calendar plan with four readings a day, from
  `crates/core/data/mcheyne.csv` (columns: date, two family readings, two private ones). February 29 has no reading. The file was
  made from a published transcription, checked against the plan's own rules: every chapter of the Old Testament once, the New
  Testament and the Psalms twice, each column reading on in order, and day 1 and day 365 as printed. The test
  `mcheyne_has_a_day_for_each_date_and_reads_every_chapter_as_often_as_it_should_be` keeps it so.
- **Chronological**: the whole Bible in a chronological order, a numbered plan of 365 days, from `crates/core/data/chronological.csv`.
  The order is that of a list of 77 passages (book and chapter range, with a phase and a note for each) compiled by the project owner
  from their own research and released by them into the public domain (the file itself states neither; this is the owner's word, and
  the plan's source says so). Its order is kept exactly (Job after
  Genesis, Psalms and Proverbs among the kings, the Gospels side by side, the letters set among Acts). The chapters were cut into days
  by `cargo run -q -p sword --example chronological_plan -- chronological_bible.csv > crates/core/data/chronological.csv`, whole
  chapters only, each day ending near an even share of the verses (7 to 176 a day, mean 85; Psalm 119 is a day and Psalm 120 follows
  alone). Checks: every book name parses; all 1,189 chapters of the Bible are read, none twice and none missing; the day numbers and
  the order are tested in `the_chronological_plan_reads_every_chapter_once_in_the_order_of_the_source_in_365_days`. The two
  Ezekiel and Daniel passages and the Matthew/Luke birth narratives are split in the source and kept split ("Song of Solomon" and the
  other names needed no change).

CrossWire's daily devotionals (`Feature=DailyDevotion`) are not supported: an earlier version could make them into plans (and still-existing
notes of `type: devotional` from it are ordinary notes now, not plans); they are no longer offered from the repository or read.
