//! The sample note of a new vault, which teaches the editor by example.

/// Where the sample note of a new vault is written.
pub const WELCOME_NOTE: &str = "notes/Welcome to Zoe.md";

/// The sample note of a new vault: the editor taught by example (links to notes and passages, a verse
/// quoted live from the Bible, a list of tasks to tick). Plain Markdown, and the person's to change or delete.
pub fn welcome_note() -> &'static str {
    "# Welcome to Zoe\n\
\n\
This is a note. It is a plain Markdown file in your vault, and everything you write in Zoe is.\n\
\n\
## Links\n\
\n\
Type `[[` to link to a note: [[My first note]] does not exist yet, and opens as a new note when you follow it.\n\
Type `[[@` to link to a passage, such as [[@John 3:16]] or [[@Romans 8:28-30]]. Every note that links to a verse is listed with it, and the verse number turns green in the Bible.\n\
A link can name a heading too: [[Welcome to Zoe#Try it]].\n\
\n\
## Quoting\n\
\n\
Put a `!` in front of a passage link and the verse is shown here, read from your Bible each time, never copied into the file:\n\
\n\
![[@Psalm 23:1]]\n\
\n\
## Try it\n\
\n\
- [x] Read this note\n\
- [ ] Tick this box by clicking it\n\
- [ ] Follow the link to [[My first note]] and write a line there\n\
- [ ] Select a verse in the Bible and press Add Mark\n\
\n\
Delete this note whenever you like.\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_note_teaches_by_example() {
        let note = welcome_note();
        assert!(note.starts_with("# Welcome to Zoe"));
        assert!(note.contains("[[@John 3:16]]") && note.contains("![[@Psalm 23:1]]") && note.contains("[[My first note]]"));
        assert!(note.contains("- [ ] ") && note.contains("- [x] ") && note.contains("[[Welcome to Zoe#Try it]]"));
        assert!(note.contains("\n## Try it\n"), "the heading the link names is there");
    }
}
