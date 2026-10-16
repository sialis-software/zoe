# Syncing between devices, with an optional server

Status: a plan for later, nothing built. The app works without any of this, and must keep doing so.

The aim: one person's vault (notes, marks, the reading log, plans) available on several devices, with a dedicated mobile
app on another platform that shares a Rust core with the desktop app. The server is optional: no server, no change in how
the desktop app behaves.

## 1. Where things stand

- The vault is a folder of `.md` files (`crates/vault/src/files.rs`: `read_all`, `read`, `write`, `remove`). The app loads
  all of it into a map of path to text at start, edits the map, and writes back only the changed files
  (`persist.rs`: `NoteMap` and `Persist::flush`). Writes are atomic (temp file, then rename).
- The SQLite indexes (links, search, verse index) are caches in the data folder and are rebuilt from the notes.
- Per-device state (settings, layouts, session, SWORD module paths) is outside the vault and stays per-device.
- Mark files have a random suffix (`margins/John/03/16-highlight-k3f9.md`), so two devices marking the same verse do not
  collide. Daily notes and the reading log are the likeliest to be edited from two places.
- The app never watches the disk: a file changed behind it goes unseen until the next start. There is no network code.

## 2. Shape of the solution

```
 desktop (full replica, works offline)  <--sync-->  server (canonical copy, API)  <--API-->  mobile (thin client)
        local vault folder                             object store + SQLite                    cache only
```

- **Server:** holds the canonical copy of each account's vault, and a revision history.
- **Desktop:** keeps a full local vault as it does now and syncs with the server when there is one configured. Offline it
  is exactly today's app.
- **Mobile:** works against the server API. It does not hold the vault the way the desktop does: at most a cache of what
  was recently opened, and a queue of edits made while offline. (An offline-first mobile replica can be added later without
  changing the server, because the sync protocol of the desktop is the same one.)
- Two kinds of client, one server: *replicas* (desktop) speak the **sync protocol** (section 5), *thin clients* (mobile)
  speak the **document API** (section 6). Both sit on the same store and the same revision numbers.

## 3. Prerequisite work in the desktop app (worth doing first, even without a server)

1. **A storage trait.** `Vault` becomes a trait (`list`, `read`, `write`, `remove`, `stat`) with the folder as its first
   implementation. Find the places that assume a local path (`vault.root()`, attachments, the old-home migration, the
   vault-link migration's backups).
2. **Noticing outside changes.** A file watcher (the `notify` crate), or a rescan on window focus, that reloads changed
   files into `NoteMap` and the index. Open editors reload their text (`note_tab.rs` already has a reload path for a
   change made behind the tab); one with unsaved edits asks first, or keeps both.
3. **Conflict copies.** Folder-sync tools (iCloud, Dropbox, Syncthing) leave "(conflicted copy)" files. Detect them and
   offer to merge, rather than letting them become stray notes. This alone makes a synced folder safe to use.
4. **Tracking what a file was when last synced.** Keep, per file, the content hash and the text last agreed with the
   other side (in the data folder, not the vault). A three-way merge needs that text as its base.
5. **Renames.** Today a rename is a delete plus a write. Record it as a move so that sync can carry it, and so that
   links rewritten by the rename arrive as edits, not surprises.

Steps 1 to 3 give a safe synced-folder mode on their own and are useful to people who will never run a server.

## 4. Identity of a file, and the revision log

- The server keeps an append-only log: each change gets a number (a sequence per account), with `path`, `op`
  (`put`, `delete`, `move`), the new content hash, the time and the device that made it.
- A file's state is *(path, content hash, revision)*. Deletes are **tombstones** in the log, so a device that was away
  does not bring a deleted note back.
- Content is addressed by hash (SHA-256 of the bytes), so an unchanged file is never sent again, and attachments (images,
  PDFs) are blobs of the same kind, stored once.
- The log doubles as history: "restore the note as it was last Tuesday" is a read of an older revision. Retention is a
  server setting.
- Paths follow the vault's rules (`/` separators, no `..`, case-insensitive collisions refused, since macOS and Windows
  disagree with Linux about them). The server normalizes and rejects, so a bad name cannot be created from one device and
  break another.

## 5. The sync protocol (for replicas)

A pull and a push over HTTPS, with optional server-sent events or a WebSocket for "something changed, pull now".

1. `GET /v1/changes?since=<rev>` returns the log entries after `rev`: paths, ops, hashes, revision numbers. Cheap, because
   the log is small and hashes are short.
2. The device compares with its own state, then `GET /v1/blobs/<hash>` for what it lacks. Blobs are immutable, so they
   cache and resume well.
3. `POST /v1/changes` with the device's edits, each carrying `base_rev` (the revision it was made on top of) and the new
   blob. The blob is uploaded first (`PUT /v1/blobs/<hash>`), so a half-finished push leaves nothing visible.
4. The server accepts a change when `base_rev` is the file's current revision. Otherwise it answers **conflict** with the
   current revision, and the device resolves it and pushes again (section 7).
5. The device keeps *last synced revision* and, per file, *last synced text* (the merge base).

Properties to hold to: pushes are idempotent (an id per change, so a retry after a dropped connection is harmless); a
sync that stops half-way can be resumed from the log position; the device never marks a file synced until the server has
acknowledged it.

## 6. The document API (for thin clients)

The mobile app should not need the whole vault to open a note, run a search or add a mark. The same store is exposed by
document, not by log:

- `GET /v1/notes?prefix=` (listing with titles, modified time, tags), `GET /v1/notes/<path>` (text and revision),
  `PUT /v1/notes/<path>` (with `If-Match: <revision>`), `DELETE`, and `POST /v1/notes/<path>/move`.
- `GET /v1/search?q=` and `GET /v1/links?passage=` and `GET /v1/backlinks?note=`: the queries the app answers from its
  SQLite index today. The server builds the same index with the **same code** (the `vault` crate's `Index`), which is the
  main reason to share the Rust core.
- `GET /v1/marks?book=&chapter=`: what the reader needs to draw a chapter's margin (the verses with notes, the marks of
  that chapter), so a phone need not fetch every mark to show one chapter.
- `POST /v1/reading-log`: append an entry to the day's note; an append is a better unit than a whole-file write for
  something two devices both add to.
- Optimistic concurrency throughout: a write carries the revision it was made from; a stale one is refused with the
  current text so the client can merge and retry.

An offline queue on the phone is a list of these requests with their base revisions, replayed in order when the
connection returns; a request that conflicts is shown to the person, not dropped.

## 7. Conflicts

Notes are small markdown files edited by one person, mostly one device at a time, so true conflicts are rare. Policy:

- **Different files:** no conflict.
- **Same file, one side unchanged since the base:** take the other (fast-forward).
- **Same file, both changed:** a **three-way merge by line** against the base text (the `similar` or `diffy` crate).
  Disjoint edits merge silently. Overlapping edits are not guessed at: both versions are kept, the merged-as-far-as-possible
  text is written, and the note shows the conflict (conflict markers, or a "keep mine / keep theirs / keep both" prompt).
- **Edit versus delete:** the edit wins and the file is restored; deleting something someone just wrote is the worse
  mistake. (The history still has the deletion.)
- **Append-mostly files** (the reading log, the daily note): merge by *entries*, not lines, so two devices each adding a
  line on the same day both keep theirs, whichever order they arrive in.
- **Marks:** one file each, with a random suffix, so concurrent creation never conflicts. Concurrent edits to one mark
  merge like notes.
- **Never last-writer-wins:** it loses text silently, and this is a place people write prayers and sermon notes.

## 8. Accounts, security and encryption

This is the decision with the largest consequences, so it comes before any code:

- **Transport:** TLS only. **Authentication:** per-device tokens (revocable one by one), obtained by signing in; passkeys or
  an emailed link rather than passwords if accounts are ever offered to others.
- **Server-side encryption at rest** is the minimum.
- **End-to-end encryption** (the server stores only ciphertext; keys stay on devices) suits private devotional writing,
  but it works against the design above:
  - Server-side **search, backlinks and the verse index** (section 6) need the plaintext. With E2E, a thin mobile client
    cannot ask the server to search, so mobile would have to hold an index (and so much of the vault) itself: the
    offline replica the plan hoped to avoid.
  - Server-side three-way merge is impossible; merging moves to the clients.
  - Recovery: a lost key is a lost vault, so a recovery key or phrase has to be part of setup.
- **Suggested path:** version one is TLS plus encryption at rest, with the server run by the person (self-hosted); leave room in the format for an E2E mode later (blobs addressed by hash of ciphertext, a
  per-vault key id in the manifest). If E2E is a requirement from the start, plan on the mobile client being a replica.
- The vault's `README.md`, attachments and the `.`-folders are subject to the same rules as notes (hidden folders such as
  `.biblestudy` hold vault settings and are synced; app caches never are).

## 9. The shared Rust core

What can move to a mobile app (iOS and Android through UniFFI, or a cross-platform Rust UI such as Dioxus or Slint; the
platform is not decided here):

| Crate | Reusable on mobile | Notes |
|---|---|---|
| `scripture` | yes | books, references, versification; pure Rust |
| `study-core` | yes | frontmatter, links, marks, plans; pure Rust |
| `vault` (`Index`, `BibleIndex`) | yes | `rusqlite` with `bundled` builds for iOS and Android; the folder `Vault` becomes one backend of the storage trait |
| `md-editor` | partly | the document model yes; the rendering is iced-specific |
| `sword` | yes, with care | reads module files from disk; the modules and their licences are the issue, not the code |
| `app` | no | the iced desktop UI |

So the work is: factor a `sync-core` crate (the protocol types, the merge, the manifest, the storage trait), used by the
desktop app, the server and the mobile core alike. One definition of the wire types (serde structs) in that crate keeps the
three from drifting.

Open question for mobile: **where do Bible texts come from?** The desktop reads SWORD modules the person installed. A
phone either carries its own modules, or the server serves chapter text from modules it holds. Serving licensed text to
other devices needs a look at each module's licence (many allow personal use only), so the safe default is that the server
syncs *the person's notes*, and each device has its own copy of the texts it is licensed to have.

## 10. The server

- **Shape:** one Rust binary (axum or similar), SQLite for the log and the index, a directory (or an S3-compatible bucket)
  for blobs. It runs on a small VPS or a home machine; a single container image is the install.
- **One owner** with several device tokens (the self-hosted case); the storage is laid out per vault, so accounts could
  be added without a migration, but none are built.
- **Easy to run yourself:** one binary or container, one config file (listen address, data folder, TLS certificate or a
  reverse proxy in front), a first-run command that makes the owner and prints a device-pairing code, and a health
  endpoint. Documented behind Caddy or nginx for TLS.
- **Index:** the server keeps an `Index` per vault, updated on each accepted change, for the search and link endpoints.
- **Operations:** backup is the SQLite file plus the blobs; export is "download my vault" as a folder of `.md` files, so
  leaving is always possible. That export matters: it is the promise that the vault stays portable, as
  `02-vault-and-markdown.md` requires.
- **Limits:** request sizes, rate limits per token, a maximum file size.

## 11. In the desktop app

- Settings gain a **Sync** section: the server address, sign-in, this device's name, "sync now", the last result, and
  devices (revoke one).
- A quiet indicator beside the vault state: synced, syncing, offline, a conflict waiting. Conflicts are listed in one
  place and opened as notes, never raised as dialogs.
- `version.json` and the upgrade backups (see `11-releasing.md`) are per-device and are not synced. The sync state (last
  revision, per-file bases) lives in the data folder and is rebuilt by a full compare if lost; a device that has lost it
  does a first sync as a merge, never as an overwrite.
- First sync of an existing vault to an empty server uploads; of an existing vault to a populated one, it offers: merge
  (three-way, with empty bases, so every difference of a same-named file is a conflict shown to the person), or replace
  this device.

## 12. Order of work

1. Storage trait, outside-change detection, conflict-copy handling (section 3). Ships alone as safe synced-folder support.
2. `sync-core` crate: wire types, manifest, three-way merge, the entry-wise merge for logs. Heavy unit testing here, with a
   property test that two devices applying edits in any order converge.
3. Server with the sync protocol and an in-memory fake of it for the app's tests; the desktop Sync section behind a setting.
4. The document API and the server-side index, once a mobile client exists to use them.
5. Mobile client on the shared core, thin first (online, with a small cache and an edit queue).
6. History/restore UI and device management.
7. End-to-end encryption mode, if it is decided to want it (section 8).

## 13. Decisions

Made:

- **Self-hosted only.** The person runs the server (a small VPS, a home machine, one container image). No hosted service,
  no billing, no sign-up flow.
- **One owner per server, several device tokens.** Accounts for other people are out of scope; the per-account structure in
  the storage layout is kept so that it can be added later, but nothing is built for it.
- **Encryption:** TLS plus encryption at rest in version one. End-to-end is left for later; the format leaves room for it
  (blobs addressed by a hash, a vault key id in the manifest), and mobile is a thin client until then.
- **Mobile** is thin first: online against the document API, with a small cache and an offline edit queue. An offline
  replica comes only if it is wanted later.
- **Bible texts** are not served by the server. It syncs the person's notes; each device has the texts it is licensed to have.
- **`.biblestudy/` settings** (pens, plan defaults) sync as part of the vault; app caches and per-device state never do.
- **Retention** of the revision log and trashed files is a server setting, with a default of a year.

Still open:

- Mobile platform and UI technology, and whether it needs any offline reading of notes.
- Whether a hosted option is ever wanted (it would reopen accounts, quotas and the encryption question).
