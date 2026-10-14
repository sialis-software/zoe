# Backlog

Things talked about and not started, newest first. Delete a line when it is done.

- **Better pronunciation with a local model.** The system voices say Greek and Hebrew as modern Greek and
  modern Hebrew. For more: convert words to IPA by rule for a chosen tradition (Erasmian, Koine, modern;
  biblical or modern Hebrew), show that as readable text, then optionally feed the IPA to a small offline
  neural TTS (Piper, Kokoro, XTTS via ONNX Runtime) fetched from the module manager. No free voices exist for
  Koine or biblical Hebrew, so the converter is what decides quality. See the discussion of pronunciation in
  the session notes.
- **The Today tab**: the plan reading of the day in one place. (The plan reading is in the Plans window.)
- **CrossWire / SWORD reading plan import**, once a sample plan file is at hand (`plan::from_sword_lines` is a guess).
- **Unlisted gaps** of the Vim mode, the module manager and the reading-plan parser are listed in their docs and reports.

## More ways to mark text (scoped, not started)

Marks have two looks (highlight, underline) and a plain note. The reader draws each as rectangles per line of text
(`reader_text.rs`, `MarkKind`), so most new looks are a few more rectangles per line, and each is a variant of
`MarkStyle` (`margin.rs`, the file's `style:` word), a case in `bible_tab.rs`'s draw conversion, a row in the note tab's
style drop-down (`StyleChoice::ALL`) and, to be used with a pen, in `PenStyle` (`state.rs`, the pens settings).

- **Cheap (rectangles only):** double underline; dotted and dashed underline (a run of short rectangles); wavy underline
  (short rectangles stepping up and down); strikethrough (a line at mid-height); box (a hollow bordered rectangle: the
  border is a quad's own, drawn per line, with the ends left open where the mark wraps); marker (the lower half of the line,
  a highlighter stroke) and a bar in the margin beside the marked lines. About a day for the set with tests and the pens.
- **Costlier (they change the text, not what is drawn under it):** a text color (the spans of the marked words are rebuilt
  with that color before layout, which touches selection, hover and Strong's words), bold or italic (they change glyph
  widths and so the line breaks of the whole verse: not worth it). Leave until asked for.
- **Decisions:** which of the cheap ones (each adds a thing to learn and a name to the drop-down); whether *Plain* (a note
  with only a faint line) is listed in the drop-down (a bare Plain mark with no comment is deleted when its card closes, so
  switching one to it needs care); whether a style carries its own default color; and what an older app does with a file
  whose `style:` it does not know (today `Mark::from_markdown` returns nothing, so the mark is not drawn and is not listed;
  it should fall back to a highlight so a file from a newer version still shows).
