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
- **Unlisted gaps** of the Vim mode, the module manager, and the reading-plan parser are listed in their docs and reports.

## More ways to mark text (dashed, wavy, and striped are done)

Marks are drawn by the reader as rectangles per line of text (`reader_text.rs`, `MarkKind`, and `paint_mark`); each look is a
variant of `MarkStyle` (`margin.rs`, the file's `style:` word), a case in `bible_tab.rs`'s draw conversion, a row in the
note tab's style drop-down (`StyleChoice::ALL`) and, to be used with a pen, in `PenStyle` (`state.rs`).

- **Done:** dashed underline, wavy underline, and a striped highlight (bands of the color alternating with clear ones, an SVG
  pattern). All have rounded corners, and underlines sit below a Strong's word's dots. A box was tried and dropped. A dotted underline is **kept for Strong's words** and is not offered. A look a file has that this
  version does not know is read as a highlight, so the mark is not lost.
- **Not done (cheap, rectangles only):** double underline; strikethrough (a line at mid-height); marker (the lower half of
  the line, a highlighter stroke); a bar in the margin beside the marked lines.
- **Costlier (they change the text, not what is drawn under it):** a text color (the spans of the marked words are rebuilt
  with that color before layout, which touches selection, hover, and Strong's words), bold or italic (they change glyph
  widths and so the line breaks of the whole verse: not worth it). Leave until asked for.
