# 09 - Reading plans

A reading plan is an ordinary note in the vault, usually under `library/Plans/`, though it can be anywhere. There is one markdown standard for plans, and several importers that write it. The code is `crates/core/src/plan.rs`.

## The standard

```markdown
---
type: reading-plan
name: Gospels in 30 days
start: 2026-01-01        # optional: the date day 1 falls on (sequential plans); set when the plan is started
tags: [reading-plan]
---
# Gospels in 30 days

## Day 1

- [[Matthew 1]]
- [[Matthew 2]]

## Day 2

- Matthew 3-4; Psalm 1
```

- `type: reading-plan` is what makes a note a plan. `name` is the plan's name (the first `# ` heading is used when it is missing). `start` is optional and only matters for sequential plans.
- Each level 2 heading is a day. Other headings are not days, and their bullets are ignored.
- Each bullet under a day holds readings, as `[[wikilinks]]` or plain text, several to a bullet separated by `;` or `,`: `Genesis 1-3; Matthew 1`, `Psalm 119:1-88`, `John 3:16-4:3`.
- Text after a spaced dash (` - `, ` – `, ` — `) is a note and is not read for passages.
- A heading may carry a title after a dash or colon (`## Day 3 — Creation`); it is kept as the day's title.
- Book names go through `Book::parse`: `Gen`, `Genesis`, `1 Cor`, `Psalm` and `Psalms` all work. A book name alone is the whole book (`Jude`).
- A chapter range (`Genesis 1-3`) is expanded to one passage per chapter, and a verse range across chapters (`John 3:16-4:3`) is split by chapter, so that progress can be counted in chapters. Written back, the split pieces are joined again.
- Anything in a bullet that cannot be read is left out and reported as a problem with its line number; the rest of the plan is still read.

## The three day shapes

All three are accepted by one parser, and a plan may use any of them.

| Heading | Shape | Day it falls on |
| --- | --- | --- |
| `## Day 3` | sequential | the plan's `start` (or the date the plan was started) plus 2 days |
| `## 2026-01-01` | dated | exactly that date |
| `## January 1`, `## Jan 1`, `## 01-01` | calendar (M'Cheyne style) | that month and day, every year; February 29 is allowed and falls only in leap years |

`Plan::day_on(date, started)` finds the day for a date. `Plan::next_unread(date, started, read)` finds the first day that has come, on or before the date, with a chapter not read, which is where to catch up from.

## Progress

Progress is not stored anywhere. It is derived from the reading log (docs/02, `reading_log.rs`): a day is done when each of its chapters is in the log on or after the plan's start. A verse-level passage counts as read when its chapter is. A chapter that appears twice in a day counts once. `Day::progress` gives (chapters read, chapters in the day) for whatever `read` function the caller supplies, so the app decides what "in the log since the start" means (for a calendar plan, since the start of the year being read).

## Importers

Each importer returns canonical markdown (the same text `Plan::render` writes), which parses back to the same days. Each also has a `plan_from_*` form returning the `Plan` with its `problems`.

- `from_csv(name, text)`: columns `day,readings` or `date,readings`; the header is optional (a first cell that is not a day is skipped); quoted fields are allowed, with `""` for a quote. A first column that is a number makes a sequential plan, an ISO date a dated plan, `Jan 1` or `01-01` a calendar plan. Further columns are more readings for the same day.
- `from_ics(name, text)`: iCalendar. Each VEVENT with a `DTSTART` date (`DTSTART;VALUE=DATE:20260101` or a date-time) and readings in `SUMMARY` (or, if the summary has none, `DESCRIPTION`) becomes a dated day. Folded lines are unfolded, and events on one date are one day.
- `to_ics(&plan, start)`: export as all-day events, one per day, with a UID per day, CRLF line endings and lines folded at 75 octets. Day 1 falls on `start`; a calendar day on its first date on or after `start`.
- `from_sword_lines(name, text)`: EXPERIMENTAL. The CrossWire/Xiphos plan file shape is not confirmed, because no sample was available. It reads the guessed common shape, one day per line: `1:Gen.1,Gen.2` or `2=Gen 3-4; Ps.23`, with readings in OSIS or in words, and `#` or `;` comment lines. Until a real sample is checked, treat the result as unreliable. The line shape lives in `sword_line` and the readings in `sword_reading`, so adjusting it is a small change.

Readings an importer cannot read are dropped from its markdown; use the `plan_from_*` form to see them in `problems`.
