//! `cargo run -p sword --example show -- LSB "John 3"`: print a chapter as our AST sees it.

use scripture::RefRange;
use study_core::ast::*;

fn inline(i: &Inline) -> String {
    match i {
        Inline::Text(t) => t.clone(),
        Inline::RedLetter(v) => format!("«{}»", v.iter().map(inline).collect::<String>()),
        Inline::Emphasis(v) => format!("_{}_", v.iter().map(inline).collect::<String>()),
        Inline::DivineName(t) => format!("[{t}]"),
        Inline::Word { text, strongs } => format!("{text}{{{strongs}}}"),
        Inline::Footnote { id, text } => format!("^{id}[{text}]"),
        Inline::CrossRef { refs } => format!("↗[{}]", refs.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("; ")),
        Inline::Ref { label, target } => format!("{label}->{}", target.osis()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [module, reference] = &args[..] else { return eprintln!("usage: show MODULE \"Book N\"") };
    let lib = sword::Library::discover();
    let bible = lib.get(module).expect("module").bible().expect("readable");
    let r = RefRange::parse(reference).expect("reference");
    let chapter = bible.chapter(r.book, r.chapter).expect("chapter");
    for block in &chapter.blocks {
        match block {
            Block::Heading(h) => println!("\n## {h}"),
            Block::Parallel(line) => println!("   {}", line.iter().map(inline).collect::<String>()),
            Block::Paragraph(p) => {
                let pad = match p.style {
                    ParagraphStyle::Prose => String::from("¶ "),
                    ParagraphStyle::Poetry { indent } => format!("{}| ", "  ".repeat(indent as usize)),
                };
                let body: Vec<String> = p
                    .runs
                    .iter()
                    .map(|r| {
                        let n = if r.show_number { format!("{} ", r.verse) } else { String::new() };
                        format!("{n}{}", r.inlines.iter().map(inline).collect::<String>())
                    })
                    .collect();
                println!("{pad}{}", body.join(" "));
            }
        }
    }
}
