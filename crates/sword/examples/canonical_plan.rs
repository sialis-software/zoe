//! Writes the CSV of the one-year canonical plan to the standard output:
//! `cargo run -p sword --example canonical_plan > crates/core/data/canonical.csv`.
//!
//! Genesis to Revelation in order, 365 days, whole chapters only, with each day ending where the
//! verses read so far are closest to an even share of all the verses, so that days are about the
//! same length to read (not the same number of chapters). A day that crosses into the next book
//! has a reading for each book.

use scripture::Book;

const DAYS: usize = 365;

fn main() {
    let chapters: Vec<(Book, u16, u32)> = Book::canon()
        .flat_map(|b| (1..=sword::chapter_count(b)).map(move |c| (b, c, u32::from(sword::verse_count(b, c)))))
        .collect();
    let total: u32 = chapters.iter().map(|c| c.2).sum();
    println!("day,readings");
    let (mut next, mut so_far) = (0usize, 0u32);
    let mut sizes: Vec<u32> = Vec::new();
    for day in 1..=DAYS {
        let goal = (total as f64 * day as f64 / DAYS as f64).round() as u32;
        let first = next;
        // At least one chapter a day, and enough left for the days after it.
        let must_leave = DAYS - day;
        while next < chapters.len() && chapters.len() - next > must_leave {
            let with = so_far + chapters[next].2;
            // Stop before a chapter that would overshoot more than stopping undershoots.
            if next > first && with > goal && (with - goal) > (goal.saturating_sub(so_far)) {
                break;
            }
            so_far = with;
            next += 1;
        }
        if day == DAYS {
            // Whatever is left is the last day's.
            while next < chapters.len() {
                so_far += chapters[next].2;
                next += 1;
            }
        }
        // One reading for each book of the day: `Exodus 1-2`.
        let mut readings: Vec<String> = Vec::new();
        let mut i = first;
        while i < next {
            let (book, from, _) = chapters[i];
            let mut to = from;
            while i + 1 < next && chapters[i + 1].0 == book {
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
        "{} chapters, {total} verses; verses a day: least {}, most {}, mean {:.1}",
        chapters.len(),
        sizes.iter().min().unwrap(),
        sizes.iter().max().unwrap(),
        total as f64 / DAYS as f64
    );
}
