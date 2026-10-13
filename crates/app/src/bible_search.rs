//! Searching the words of a Bible module: the index of its verses (kept in the cache folder, made
//! the first time a module is searched) and what a typed query asks of it.

use crate::shared::Shared;
use scripture::{Book, RefRange};
use std::collections::HashSet;
use std::rc::Rc;
use study_core::ModuleId;
use study_core::search::Query;
use vault::{BibleIndex, Search};

/// The most verses one search gives back.
pub const LIMIT: usize = 20_000;

/// A verse that fits, with where in its words the query's are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub range: RefRange,
    pub text: String,
    pub spans: Vec<(usize, usize)>,
}

/// How a search is made, besides the words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Options {
    /// Only words written as typed.
    pub case: bool,
    /// Other forms of a word.
    pub stem: bool,
    /// Best matches first, else in canon order.
    pub by_relevance: bool,
}

/// What a search found.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Results {
    pub found: Vec<Found>,
    /// There were more than [`LIMIT`], and these are the first.
    pub cut: bool,
}

impl Shared {
    /// The file the index of `module` is kept in.
    fn index_file(&self, module: &ModuleId) -> Option<std::path::PathBuf> {
        let name: String = module.0.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        self.cache_dir.as_ref().map(|dir| dir.join(format!("bible-{name}.sqlite")))
    }

    /// What identifies the module as installed: if it changes, the index is made again.
    fn fingerprint(&self, module: &ModuleId) -> String {
        let chapters = self.backend.available_chapters(module);
        let verse = |book: Book, chapter: u16, verse: u16| {
            self.backend.chapter(module, book, chapter).ok().and_then(|c| c.canonical_verses().get(&verse).map(String::len)).unwrap_or(0)
        };
        format!("{}:{}:{}:{}", module.0, chapters.len(), verse(Book::GENESIS, 1, 1), verse(Book::JOHN, 3, 16))
    }

    /// Whether searching `module` can start at once (its index is made).
    pub fn bible_index_ready(&self, module: &ModuleId) -> bool {
        if self.bible_indexes.borrow().contains_key(module) {
            return true;
        }
        let Some(file) = self.index_file(module).filter(|f| f.is_file()) else { return false };
        BibleIndex::open(Some(&file)).is_ok_and(|index| index.is_current(&self.fingerprint(module)))
    }

    /// The index of `module`, made now if it is not (which takes a moment: every verse is read).
    pub fn bible_index(&self, module: &ModuleId) -> Result<Rc<BibleIndex>, String> {
        if let Some(index) = self.bible_indexes.borrow().get(module) {
            return Ok(index.clone());
        }
        let file = self.index_file(module);
        let mut index = BibleIndex::open(file.as_deref()).map_err(|e| e.to_string())?;
        let fingerprint = self.fingerprint(module);
        if !index.is_current(&fingerprint) {
            let mut verses = Vec::new();
            for (book, chapter) in self.backend.available_chapters(module) {
                let Ok(text) = self.backend.chapter(module, book, chapter) else { continue };
                for (verse, words) in text.canonical_verses() {
                    verses.push((book, chapter, verse, words));
                }
            }
            index.build(&fingerprint, verses).map_err(|e| e.to_string())?;
        }
        let index = Rc::new(index);
        self.bible_indexes.borrow_mut().insert(module.clone(), index.clone());
        Ok(index)
    }

    /// The verses of `module` that fit `query`.
    pub fn search_bible(&self, module: &ModuleId, query: &Query, options: Options) -> Result<Results, String> {
        if query.is_empty() {
            return Ok(Results::default());
        }
        let index = self.bible_index(module)?;
        let place = |hit: &vault::VerseHit| RefRange { book: hit.book, chapter: hit.chapter, verses: Some((hit.verse, hit.verse)) };
        // The verses with each Strong's number asked for, and the words in them.
        let numbered: Vec<Vec<crate::shared::Occurrence>> = query.strongs.iter().map(|n| self.strongs_occurrences(module, n)).collect();
        let verse_sets: Vec<HashSet<RefRange>> = numbered.iter().map(|os| os.iter().map(|o| o.range).collect()).collect();
        let in_scope = |book: Book| query.scope.is_none_or(|s| s.contains(book));
        let has_numbers = |range: &RefRange| verse_sets.iter().all(|set| set.contains(range));
        let number_spans = |range: &RefRange| -> Vec<(usize, usize)> {
            numbered.iter().flat_map(|os| os.iter().filter(|o| o.range == *range).flat_map(|o| o.words.clone())).collect()
        };

        let mut results = Results::default();
        let found = |range: RefRange, text: String| {
            let mut spans = query.spans(&text, options.case, options.stem);
            spans.extend(number_spans(&range));
            spans.sort();
            spans.dedup();
            Found { range, text, spans }
        };
        match query.fts() {
            Some(expression) => {
                let books = query.scope.map(|s| (s.first, s.last));
                let search = Search { expression: &expression, stem: options.stem, books, by_relevance: options.by_relevance, limit: LIMIT + 1 };
                let hits = index.search(&search).map_err(|e| e.to_string())?;
                results.cut = hits.len() > LIMIT;
                for hit in hits.into_iter().take(LIMIT) {
                    let range = place(&hit);
                    // The index does not tell capitals apart: the words are checked as typed.
                    if (options.case && !query.fits(&hit.text, true, options.stem)) || !has_numbers(&range) {
                        continue;
                    }
                    results.found.push(found(range, hit.text));
                }
            }
            None => {
                // Only Strong's numbers: the verses that have every one.
                let Some(first) = numbered.first() else { return Ok(results) };
                for occurrence in first {
                    let range = occurrence.range;
                    if !in_scope(range.book) || !has_numbers(&range) {
                        continue;
                    }
                    let Some((verse, _)) = range.verses else { continue };
                    let Some(text) = index.verse(range.book, range.chapter, verse) else { continue };
                    if results.found.len() >= LIMIT {
                        results.cut = true;
                        break;
                    }
                    results.found.push(found(range, text));
                }
            }
        }
        Ok(results)
    }
}
