//! Writes the CSV of the one-year chronological plan to the standard output, from the source list
//! of passages in chronological order:
//! `cargo run -q -p sword --example chronological_plan -- chronological_bible.csv > crates/core/data/chronological.csv`.
//!
//! The source (columns `seq,phase,book,chapter_from,chapter_to,...`) is read in `seq` order and
//! never reordered. Its chapters are cut into 365 days whole-chapter by whole-chapter, each day
//! ending where the verses read so far are closest to an even share, like `canonical_plan`. A
//! day that crosses into another passage has a reading for each book run. The checks (chapters
//! missing or repeated, names that do not parse) go to the standard error.

use scripture::Book;
use std::collections::HashMap;

const DAYS: usize = 365;

/// The fields of one CSV line, with quoted fields.
fn fields(line: &str) -> Vec<String> {
    let (mut out, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    out.push(cell);
    out
}

fn main() {
    let path = std::env::args().nth(1).expect("the source CSV");
    let text = std::fs::read_to_string(path).unwrap();
    let mut chapters: Vec<(Book, u16, u32)> = Vec::new();
    for line in text.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let f = fields(line);
        let book = Book::parse(f[2].trim()).unwrap_or_else(|| panic!("book name {:?} does not parse", f[2]));
        let (from, to): (u16, u16) = (f[3].trim().parse().unwrap(), f[4].trim().parse().unwrap());
        for c in from..=to {
            chapters.push((book, c, u32::from(sword::verse_count(book, c))));
        }
    }
    let mut times: HashMap<(Book, u16), u32> = HashMap::new();
    for c in &chapters {
        *times.entry((c.0, c.1)).or_default() += 1;
    }
    for b in Book::canon() {
        for c in 1..=sword::chapter_count(b) {
            match times.get(&(b, c)).copied().unwrap_or(0) {
                1 => {}
                n => eprintln!("{} {c}: read {n} times", b.name()),
            }
        }
    }
    let total: u32 = chapters.iter().map(|c| c.2).sum();
    println!("day,readings");
    let (mut next, mut so_far) = (0usize, 0u32);
    let mut sizes: Vec<u32> = Vec::new();
    for day in 1..=DAYS {
        let goal = (total as f64 * day as f64 / DAYS as f64).round() as u32;
        let first = next;
        let must_leave = DAYS - day;
        while next < chapters.len() && chapters.len() - next > must_leave {
            let with = so_far + chapters[next].2;
            if next > first && with > goal && (with - goal) > (goal.saturating_sub(so_far)) {
                break;
            }
            so_far = with;
            next += 1;
        }
        if day == DAYS {
            while next < chapters.len() {
                so_far += chapters[next].2;
                next += 1;
            }
        }
        let mut readings: Vec<String> = Vec::new();
        let mut i = first;
        while i < next {
            let (book, from, _) = chapters[i];
            let mut to = from;
            // Consecutive chapters of the same book only (a book met again later starts a new reading).
            while i + 1 < next && chapters[i + 1].0 == book && chapters[i + 1].1 == to + 1 {
                i += 1;
                to = chapters[i].1;
            }
            i += 1;
            readings.push(if to == from { format!("{} {from}", book.name()) } else { format!("{} {from}-{to}", book.name()) });
        }
        sizes.push(chapters[first..next].iter().map(|c| c.2).sum());
        println!("{day},{}", readings.join(","));
    }
    eprintln!(
        "{} chapters, {total} verses; verses a day: least {} (day {}), most {} (day {}), mean {:.1}",
        chapters.len(),
        sizes.iter().min().unwrap(),
        sizes.iter().position(|s| s == sizes.iter().min().unwrap()).unwrap() + 1,
        sizes.iter().max().unwrap(),
        sizes.iter().position(|s| s == sizes.iter().max().unwrap()).unwrap() + 1,
        total as f64 / DAYS as f64
    );
}
